//! Request/response state machine; isolated from ALSA so it can be tested offline.
use crate::{
    document::{Backup, BackupPatch, ParamSet, Patch, DEVICE},
    protocol::*,
    schema::{slot_address, Block, Schema},
};
use anyhow::{bail, ensure, Context, Result};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

pub trait Transport {
    fn send(&mut self, bytes: &[u8]) -> Result<()>;
    fn receive(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>>;
}
/// Tracks every requested byte, not just the first matching DT1. Unrelated,
/// duplicate, reordered and partial device replies cannot cause false completion.
pub struct ReadBuffer {
    address: Address,
    bytes: Vec<u8>,
    seen: Vec<bool>,
}
impl ReadBuffer {
    pub fn new(address: Address, size: usize) -> Result<Self> {
        ensure!(size > 0 && size <= 32768, "invalid read size");
        address.plus(size - 1)?;
        Ok(Self {
            address,
            bytes: vec![0; size],
            seen: vec![false; size],
        })
    }
    pub fn accept(&mut self, address: Address, data: &[u8]) -> Result<bool> {
        let first = address.linear() as i64 - self.address.linear() as i64;
        for (i, b) in data.iter().enumerate() {
            let offset = first + i as i64;
            if offset < 0 || offset >= self.bytes.len() as i64 {
                continue;
            }
            let n = offset as usize;
            ensure!(
                !self.seen[n] || self.bytes[n] == *b,
                "conflicting duplicate response"
            );
            self.bytes[n] = *b;
            self.seen[n] = true;
        }
        Ok(self.seen.iter().all(|b| *b))
    }
}
pub struct Session<T: Transport> {
    transport: T,
    pub identity: Identity,
    timeout: Duration,
    active: bool,
}
impl<T: Transport> Session<T> {
    /// Strict observation: identity and RQ1 only, no editor-mode setup or cleanup.
    /// Even accidentally calling a write method is blocked at the transport.
    pub fn observe(transport: T) -> Result<Session<crate::safety::ReadRequestsOnly<T>>> {
        Session::identify(crate::safety::ReadRequestsOnly::new(transport))
    }
    fn identify(mut transport: T) -> Result<Self> {
        transport.send(&IDENTITY_REQUEST)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        let identity = loop {
            ensure!(
                Instant::now() < deadline,
                "Katana identity request timed out"
            );
            if let Some(bytes) = transport.receive(Duration::from_millis(20))? {
                if let Ok(id) = Identity::decode(&bytes) {
                    break id;
                }
            }
        };
        Ok(Self {
            transport,
            identity,
            timeout: Duration::from_secs(15),
            active: false,
        })
    }
    pub fn connect(transport: T) -> Result<Self> {
        let mut s = Self::identify(transport)?;
        let level = s.read(Address::packed(0x7f000000)?, 1)?;
        ensure!(
            level == [1],
            "unsupported editor communication level {level:?}"
        );
        s.dt1(Address::packed(0x7f000001)?, vec![1])?;
        s.active = true;
        Ok(s)
    }
    fn dt1(&mut self, address: Address, data: Vec<u8>) -> Result<()> {
        self.transport.send(
            &Message::Data {
                device: self.identity.device,
                address,
                data,
            }
            .encode()?,
        )
    }
    pub fn read(&mut self, address: Address, size: usize) -> Result<Vec<u8>> {
        let mut read = ReadBuffer::new(address, size)?;
        self.transport.send(
            &Message::Request {
                device: self.identity.device,
                address,
                size: size as u32,
            }
            .encode()?,
        )?;
        let deadline = Instant::now() + self.timeout;
        while Instant::now() < deadline {
            if let Some(frame) = self.transport.receive(Duration::from_millis(10))? {
                if let Ok(Message::Data {
                    device,
                    address,
                    data,
                }) = Message::decode(&frame)
                {
                    if (device == self.identity.device || device == 0x7f)
                        && read.accept(address, &data)?
                    {
                        return Ok(read.bytes);
                    }
                }
            }
        }
        bail!("RQ1 at {address} timed out; read discarded (no partial snapshot published)")
    }
    fn read_block(&mut self, b: &Block, address: Address) -> Result<Vec<u8>> {
        let mut data = vec![0; b.size];
        for (offset, size) in b.chunks() {
            data[offset..offset + size].copy_from_slice(&self.read(address.plus(offset)?, size)?);
        }
        Ok(data)
    }
    pub fn read_root(&mut self, schema: &Schema, root: &str) -> Result<ParamSet> {
        ensure!(
            ["PATCH", "SYSTEM", "SETUP", "TEMP"].contains(&root),
            "root not readable here"
        );
        let mut set = BTreeMap::new();
        for b in schema
            .blocks
            .iter()
            .filter(|b| b.id.starts_with(&format!("{root}%")))
        {
            set.insert(b.id.clone(), self.read_block(b, b.address())?);
        }
        Ok(set)
    }
    pub fn read_slot(&mut self, schema: &Schema, slot: usize) -> Result<Patch> {
        ensure!(
            self.identity.slot_exists(slot),
            "slot unavailable on {}",
            self.identity.name
        );
        let base = slot_address(slot)?;
        let temp = Address::packed(0x20000000)?.linear();
        let mut p = Patch::new(schema)?;
        for b in schema.blocks.iter().filter(|b| b.id.starts_with("PATCH%")) {
            let address = base.plus((b.address - temp) as usize)?;
            p.parameters
                .insert(b.id.clone(), self.read_block(b, address)?);
        }
        Ok(p)
    }
    /// Read just the saved names for channel cards; never select a channel.
    pub fn read_channel_names(&mut self, schema: &Schema) -> Result<[Option<String>; 9]> {
        let b = schema.block("PATCH%COM")?;
        let name = &b.parameters[0];
        let temp = Address::packed(0x20000000)?.linear();
        let offset = (b.address - temp) as usize + name.offset;
        let mut names = std::array::from_fn(|_| None);
        for (slot, target) in names.iter_mut().enumerate().skip(1) {
            if self.identity.slot_exists(slot) {
                let bytes = self.read(slot_address(slot)?.plus(offset)?, name.width)?;
                *target = Some(String::from_utf8_lossy(&bytes).trim_end().to_owned());
            }
        }
        Ok(names)
    }
    pub fn read_backup(&mut self, schema: &Schema) -> Result<Backup> {
        let system = self.read_root(schema, "SYSTEM")?;
        let setup = self.read_root(schema, "SETUP")?;
        let mut patches = Vec::new();
        for slot in 0..10 {
            patches.push(if self.identity.slot_exists(slot) {
                Some(BackupPatch(self.read_slot(schema, slot)?.parameters))
            } else {
                None
            });
        }
        Ok(Backup {
            revision: "0000".into(),
            device: DEVICE.into(),
            model: self.identity.name.into(),
            system,
            setup,
            patches,
            extra: BTreeMap::new(),
        })
    }
    pub fn set_parameter(
        &mut self,
        schema: &Schema,
        id: &str,
        index: usize,
        bytes: Vec<u8>,
    ) -> Result<()> {
        let b = schema.block(id)?;
        ensure!(!b.readonly, "read-only block");
        let p = b.parameters.get(index).context("unknown parameter")?;
        ensure!(
            bytes.len() == p.width && bytes.iter().all(|b| *b < 128),
            "parameter size/data invalid"
        );
        if p.encoding == "ascii" {
            p.encode_text(std::str::from_utf8(&bytes)?)?;
        } else {
            let mut data = vec![0; p.offset + p.width];
            data[p.offset..].copy_from_slice(&bytes);
            p.encode_number(p.decode_number(&data)?)?;
        }
        self.dt1(b.address().plus(p.offset)?, bytes)
    }
    /// Explicit preview only; never writes a stored slot implicitly.
    pub fn preview(&mut self, schema: &Schema, patch: &Patch) -> Result<()> {
        // Validate the entire operation before emitting any write.
        let mut tasks = Vec::new();
        for b in schema
            .blocks
            .iter()
            .filter(|b| b.id.starts_with("PATCH%") && !b.readonly)
        {
            let data = patch
                .parameters
                .get(&b.id)
                .with_context(|| format!("missing block {}", b.id))?;
            ensure!(data.len() >= b.size, "short patch block {}", b.id);
            for p in &b.parameters {
                if p.encoding == "ascii" {
                    p.encode_text(std::str::from_utf8(&data[p.offset..p.offset + p.width])?)?;
                } else {
                    p.encode_number(p.decode_number(data)?)?;
                }
            }
            for (offset, size) in b.chunks() {
                tasks.push((
                    b.address().plus(offset)?,
                    data[offset..offset + size].to_vec(),
                ));
            }
        }
        for (addr, data) in tasks {
            self.dt1(addr, data)?;
        }
        Ok(())
    }
    /// Save the supplied snapshot, never an unrelated or stale hardware buffer.
    /// The caller must hold persistent permission; Guarded checks every packet.
    /// Failure/cancellation does not roll back the amp and must not be retried automatically.
    pub fn store_patch(&mut self, schema: &Schema, patch: &Patch, slot: usize) -> Result<()> {
        ensure!(
            slot > 0 && self.identity.slot_exists(slot),
            "cannot overwrite PANEL or absent slot"
        );
        self.preview(schema, patch)?;
        self.commit(slot)
    }
    pub fn select_slot(&mut self, slot: usize) -> Result<()> {
        ensure!(self.identity.slot_exists(slot), "slot unavailable");
        self.dt1(Address::packed(0x7f000100)?, vec![0, slot as u8])?;
        // Do not read the old edit buffer before the channel transition is visible.
        let deadline = Instant::now() + self.timeout;
        while Instant::now() < deadline {
            if self.read(Address::packed(0)?, 2)? == [0, slot as u8] {
                return Ok(());
            }
        }
        bail!("channel selection was not confirmed; hardware state unknown")
    }
    fn command_ack(&mut self, address: Address, data: Vec<u8>) -> Result<()> {
        self.dt1(address, data)?;
        let deadline = Instant::now() + self.timeout;
        while Instant::now() < deadline {
            if let Some(f) = self.transport.receive(Duration::from_millis(10))? {
                if let Ok(Message::Data {
                    device, address: a, ..
                }) = Message::decode(&f)
                {
                    if (device == self.identity.device || device == 127) && address == a {
                        return Ok(());
                    }
                }
            }
        }
        bail!("command at {address} not acknowledged; hardware state is UNKNOWN, do not retry blindly")
    }
    /// Commit the CURRENT hardware edit buffer. Caller must separately preview
    /// local changes first and explicitly confirm the destructive destination.
    pub fn commit(&mut self, slot: usize) -> Result<()> {
        ensure!(
            slot > 0 && self.identity.slot_exists(slot),
            "cannot overwrite PANEL or absent slot"
        );
        self.command_ack(Address::packed(0x7f000104)?, vec![0, slot as u8])?;
        self.dt1(Address::packed(0x7f010109)?, vec![1])?;
        let result = self.command_ack(Address::packed(0x7f01010a)?, vec![0x40, slot as u8]);
        if result.is_err() {
            // Best effort only; an export acknowledgement can be lost. Clearing
            // preview-mute is nonpersistent and must not trigger a second commit.
            let _ = self.dt1(Address::packed(0x7f010109)?, vec![0]);
        }
        result
    }
    pub fn poll(&mut self) -> Result<Option<Message>> {
        if let Some(frame) = self.transport.receive(Duration::from_millis(1))? {
            if let Ok(m @ Message::Data { device, .. }) = Message::decode(&frame) {
                if device == self.identity.device || device == 127 {
                    return Ok(Some(m));
                }
            }
        }
        Ok(None)
    }
    pub fn close(&mut self) -> Result<()> {
        if self.active {
            self.dt1(Address::packed(0x7f000001)?, vec![0])?;
            self.active = false;
        }
        Ok(())
    }
}
impl<T: Transport> Drop for Session<T> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    #[test]
    fn split_reordered_and_duplicate() {
        let a = Address::packed(0x20000000).unwrap();
        let mut r = ReadBuffer::new(a, 4).unwrap();
        assert!(!r.accept(a.plus(2).unwrap(), &[3, 4]).unwrap());
        assert!(!r.accept(a.plus(2).unwrap(), &[3, 4]).unwrap());
        assert!(r.accept(a, &[1, 2]).unwrap());
        assert!(r.accept(a, &[2]).is_err());
        assert_eq!(r.bytes, [1, 2, 3, 4]);
    }
    struct Mock {
        incoming: VecDeque<Vec<u8>>,
        sent: Vec<Vec<u8>>,
    }
    impl Transport for Mock {
        fn send(&mut self, b: &[u8]) -> Result<()> {
            self.sent.push(b.to_vec());
            Ok(())
        }
        fn receive(&mut self, _: Duration) -> Result<Option<Vec<u8>>> {
            Ok(self.incoming.pop_front())
        }
    }
    fn offline_session(incoming: VecDeque<Vec<u8>>) -> Session<Mock> {
        Session {
            transport: Mock {
                incoming,
                sent: vec![],
            },
            identity: Identity {
                device: 16,
                model_code: 6,
                name: "KATANA-100",
            },
            timeout: Duration::from_millis(5),
            active: false,
        }
    }
    #[test]
    fn read_filters_wrong_device_corruption_and_accepts_split() {
        let a = Address::packed(0x20000000).unwrap();
        let frame = |device, address, data| {
            Message::Data {
                device,
                address,
                data,
            }
            .encode()
            .unwrap()
        };
        let mut corrupt = frame(16, a, vec![1, 2]);
        corrupt[11] ^= 1;
        let mut s = offline_session(VecDeque::from([
            frame(17, a, vec![9, 9]),
            corrupt,
            frame(16, a.plus(1).unwrap(), vec![2]),
            frame(127, a, vec![1]),
        ]));
        assert_eq!(s.read(a, 2).unwrap(), [1, 2]);
        assert_eq!(s.transport.sent.len(), 1);
    }
    #[test]
    fn incomplete_read_does_not_publish_or_retry() {
        let a = Address::packed(0x20000000).unwrap();
        let frame = Message::Data {
            device: 16,
            address: a,
            data: vec![1],
        }
        .encode()
        .unwrap();
        let mut s = offline_session(VecDeque::from([frame]));
        assert!(s.read(a, 2).is_err());
        assert_eq!(s.transport.sent.len(), 1);
    }
    #[test]
    fn preview_validates_before_any_write() {
        let schema = Schema::load();
        let mut p = Patch::new(&schema).unwrap();
        p.parameters.remove("PATCH%ASSIGN_FS");
        let mut s = offline_session(VecDeque::new());
        assert!(s.preview(&schema, &p).is_err());
        assert!(s.transport.sent.is_empty());
        assert!(s
            .set_parameter(&schema, "COMMAND%COMMAND_COM", 0, vec![1])
            .is_err());
        assert!(s.commit(0).is_err());
        assert!(s.commit(9).is_err());
        assert!(s.transport.sent.is_empty());
    }
    #[test]
    fn channel_names_are_model_filtered_reads_only() {
        let schema = Schema::load();
        let b = schema.block("PATCH%COM").unwrap();
        let offset = (b.address - Address::packed(0x20000000).unwrap().linear()) as usize;
        let incoming = [1, 2, 5, 6]
            .into_iter()
            .map(|slot| {
                Message::Data {
                    device: 16,
                    address: slot_address(slot).unwrap().plus(offset).unwrap(),
                    data: b.parameters[0]
                        .encode_text(&format!("Tone {slot}"))
                        .unwrap(),
                }
                .encode()
                .unwrap()
            })
            .collect();
        let mut s = offline_session(incoming);
        s.identity.model_code = 5;
        let names = s.read_channel_names(&schema).unwrap();
        for (slot, name) in names.iter().enumerate() {
            if [1, 2, 5, 6].contains(&slot) {
                assert_eq!(name.as_deref(), Some(format!("Tone {slot}").as_str()));
            } else {
                assert!(name.is_none());
            }
        }
        assert_eq!(s.transport.sent.len(), 4);
        assert!(s
            .transport
            .sent
            .iter()
            .all(|f| matches!(Message::decode(f), Ok(Message::Request { .. }))));
    }
    #[test]
    fn failed_preview_during_store_never_reaches_commit() {
        struct Fails {
            sent: Vec<Vec<u8>>,
            calls: usize,
        }
        impl Transport for Fails {
            fn send(&mut self, bytes: &[u8]) -> Result<()> {
                self.calls += 1;
                ensure!(self.calls < 2, "simulated USB loss");
                self.sent.push(bytes.to_vec());
                Ok(())
            }
            fn receive(&mut self, _: Duration) -> Result<Option<Vec<u8>>> {
                Ok(None)
            }
        }
        let schema = Schema::load();
        let mut s = Session {
            transport: Fails {
                sent: vec![],
                calls: 0,
            },
            identity: Identity {
                device: 16,
                model_code: 6,
                name: "KATANA-100",
            },
            timeout: Duration::from_millis(5),
            active: false,
        };
        assert!(s
            .store_patch(&schema, &Patch::new(&schema).unwrap(), 1)
            .is_err());
        assert_eq!(s.transport.calls, 2);
        assert!(s.transport.sent.iter().all(|f| matches!(Message::decode(f), Ok(Message::Data {address,..}) if address.linear() < Address::packed(0x20100000).unwrap().linear())));
    }
    #[test]
    fn store_preflights_destination_and_patch_before_writing() {
        let schema = Schema::load();
        let mut p = Patch::new(&schema).unwrap();
        let mut s = offline_session(VecDeque::new());
        for slot in [0, 9] {
            assert!(s.store_patch(&schema, &p, slot).is_err());
        }
        s.identity.model_code = 5; // KATANA-50 has no A3.
        assert!(s.store_patch(&schema, &p, 3).is_err());
        p.parameters.remove("PATCH%AMP");
        assert!(s.store_patch(&schema, &p, 1).is_err());
        assert!(s.transport.sent.is_empty());
    }
    #[test]
    fn store_sends_snapshot_before_single_commit_and_never_selects_channel() {
        let schema = Schema::load();
        let mut p = Patch::new(&schema).unwrap();
        p.rename(&schema, "Dropped tone").unwrap();
        let ack = |packed| {
            Message::Data {
                device: 16,
                address: Address::packed(packed).unwrap(),
                data: vec![0],
            }
            .encode()
            .unwrap()
        };
        let mut s = offline_session(VecDeque::from([ack(0x7f000104), ack(0x7f01010a)]));
        s.store_patch(&schema, &p, 2).unwrap();
        let messages: Vec<_> = s
            .transport
            .sent
            .iter()
            .map(|f| Message::decode(f).unwrap())
            .collect();
        let mut expected = offline_session(VecDeque::new());
        expected.preview(&schema, &p).unwrap();
        assert_eq!(
            &s.transport.sent[..expected.transport.sent.len()],
            &expected.transport.sent
        );
        assert_eq!(messages.len(), expected.transport.sent.len() + 3);
        assert!(messages.iter().all(|m| !matches!(m, Message::Data { address, .. } if *address == Address::packed(0x7f000100).unwrap())));
        assert!(
            matches!(&messages[messages.len()-1], Message::Data {address, data, ..} if *address == Address::packed(0x7f01010a).unwrap() && data == &[0x40, 2])
        );
    }
    #[test]
    fn missing_save_ack_never_retries_commit() {
        let schema = Schema::load();
        let p = Patch::new(&schema).unwrap();
        let mut s = offline_session(VecDeque::new());
        assert!(s.store_patch(&schema, &p, 1).is_err());
        let commands = s.transport.sent.iter().filter(|f| matches!(Message::decode(f), Ok(Message::Data {address, ..}) if address == Address::packed(0x7f000104).unwrap())).count();
        assert_eq!(commands, 1);
        assert!(!s.transport.sent.iter().any(|f| matches!(Message::decode(f), Ok(Message::Data {address, ..}) if address == Address::packed(0x7f01010a).unwrap())));
    }
    #[test]
    fn handshake_and_safe_close() {
        let id = vec![
            0xf0, 0x7e, 0x10, 6, 2, 0x41, 7, 5, 0, 0, 6, 0, 0, 0, 0, 0xf7,
        ];
        let level = Message::Data {
            device: 16,
            address: Address::packed(0x7f000000).unwrap(),
            data: vec![1],
        }
        .encode()
        .unwrap();
        let mock = Mock {
            incoming: VecDeque::from([id, level]),
            sent: vec![],
        };
        let mut s = Session::connect(mock).unwrap();
        assert_eq!(s.identity.name, "KATANA-100");
        assert_eq!(s.transport.sent[0], IDENTITY_REQUEST);
        s.close().unwrap();
        assert_eq!(s.transport.sent.len(), 4);
        assert!(s.commit(0).is_err());
    }
}
