//! Experimental KATANA-AIR protocol. Kept separate from the Gen 3 address map.
//! Source provenance and unverified behavior: docs/KATANA-AIR.md.
use crate::{
    protocol::{Address, checksum},
    safety::{Access, WriteGate},
    session::Transport,
};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

pub const MODEL_ID: [u8; 4] = [0, 0, 0, 0x50];
pub const IDENTITY_REQUEST: [u8; 6] = [0xf0, 0x7e, 0, 6, 1, 0xf7];
pub const CHANNEL_PROGRAMS: [u8; 7] = [0, 1, 2, 3, 5, 6, 7];
pub const CHANNEL_LABELS: [&str; 7] = [
    "PANEL", "A Green", "A Red", "A Yellow", "B Green", "B Red", "B Yellow",
];

#[derive(Clone, Debug, Deserialize)]
pub struct Parameter {
    pub id: String,
    pub address: [u8; 4],
    pub label: String,
    pub options: BTreeMap<u16, String>,
    pub bytes: usize,
    pub numeric: bool,
}
impl Parameter {
    pub fn value(&self, snapshot: &Snapshot) -> Option<u16> {
        snapshot.values.get(&self.id).copied()
    }
    pub fn text(&self, value: u16) -> String {
        self.options
            .get(&value)
            .cloned()
            .unwrap_or_else(|| format!("Unknown ({value})"))
    }
}
#[derive(Clone, Debug, Deserialize)]
pub struct Block {
    pub id: String,
    pub title: String,
    pub common: Vec<String>,
    pub variants: Vec<Vec<String>>,
    pub controls: Vec<String>,
    pub selector: Option<String>,
    pub toggle: Option<String>,
    #[serde(default)]
    pub variant_values: Vec<u16>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Schema {
    pub parameters: BTreeMap<String, Parameter>,
    pub blocks: Vec<Block>,
    pub attribution: String,
}
impl Schema {
    pub fn bundled() -> Result<Self> {
        let schema: Self = serde_json::from_str(include_str!("../data/katana-air.json"))?;
        for (id, p) in &schema.parameters {
            ensure!(
                *id == p.id && (1..=2).contains(&p.bytes) && !p.options.is_empty(),
                "invalid Air parameter {id}"
            );
            let address = Address::from_bytes(p.address)?;
            let end = address.plus(p.bytes - 1)?.linear();
            let tone = Address::packed(0x60000000)?.linear()..Address::packed(0x60001300)?.linear();
            let global =
                Address::packed(0x00000430)?.linear()..Address::packed(0x0000043f)?.linear();
            ensure!(
                (tone.contains(&address.linear()) && tone.contains(&end))
                    || (global.contains(&address.linear()) && global.contains(&end)),
                "Air parameter {id} outside supported ranges"
            );
            ensure!(
                p.options
                    .keys()
                    .all(|n| (*n as usize) < (1usize << (7 * p.bytes))),
                "invalid option for {id}"
            );
        }
        for block in &schema.blocks {
            for id in block
                .common
                .iter()
                .chain(block.controls.iter())
                .chain(block.variants.iter().flatten())
                .chain(block.selector.iter())
                .chain(block.toggle.iter())
            {
                ensure!(
                    schema.parameters.contains_key(id),
                    "Air section references missing parameter {id}"
                );
            }
            ensure!(
                block.variants.is_empty() || block.variants.len() == block.variant_values.len(),
                "invalid Air effect variants"
            );
        }
        Ok(schema)
    }
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    /// Linear base-128 addresses, never packed hexadecimal addresses.
    pub memory: BTreeMap<u32, u8>,
    pub values: BTreeMap<String, u16>,
    pub selected_channel: Option<usize>,
    pub warnings: Vec<String>,
}
impl Snapshot {
    fn from_memory(memory: BTreeMap<u32, u8>, schema: &Schema) -> Self {
        let mut values = BTreeMap::new();
        for (id, p) in &schema.parameters {
            let start = Address::from_bytes(p.address)
                .expect("validated metadata")
                .linear();
            let value = (0..p.bytes).try_fold(0u16, |v, offset| {
                memory
                    .get(&(start + offset as u32))
                    .map(|b| v * 128 + *b as u16)
            });
            if let Some(value) = value {
                values.insert(id.clone(), value);
            }
        }
        let channel_start = Address::packed(0x00010000)
            .expect("constant address")
            .linear();
        let selected_channel = match (memory.get(&channel_start), memory.get(&(channel_start + 1)))
        {
            (Some(0), Some(program)) => CHANNEL_PROGRAMS.iter().position(|n| n == program),
            _ => None,
        };
        Self {
            memory,
            values,
            selected_channel,
            warnings: Vec::new(),
        }
    }
}

fn frame(command: u8, address: Address, data: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        matches!(command, 0x11 | 0x12)
            && !data.is_empty()
            && data.len() <= 128
            && data.iter().all(|b| *b < 128),
        "invalid Air message"
    );
    address.plus(data.len() - 1)?;
    let mut out = vec![0xf0, 0x41, 0];
    out.extend(MODEL_ID);
    out.push(command);
    out.extend(address.bytes());
    out.extend(data);
    out.push(checksum(&out[8..]));
    out.push(0xf7);
    Ok(out)
}
fn decode_data(bytes: &[u8]) -> Result<(Address, &[u8])> {
    ensure!(
        bytes.len() >= 15
            && bytes[..8] == [0xf0, 0x41, 0, 0, 0, 0, 0x50, 0x12]
            && bytes.last() == Some(&0xf7),
        "not Air DT1"
    );
    ensure!(
        bytes[2..bytes.len() - 1].iter().all(|b| *b < 128)
            && checksum(&bytes[8..bytes.len() - 1]) == 0,
        "invalid Air DT1 data/checksum"
    );
    let address = Address::from_bytes(bytes[8..12].try_into()?)?;
    let data = &bytes[12..bytes.len() - 2];
    ensure!(
        !data.is_empty() && data.len() <= 128,
        "invalid Air payload length"
    );
    address.plus(data.len() - 1)?;
    Ok((address, data))
}
fn identity_reply(bytes: &[u8]) -> bool {
    bytes.len() >= 15
        && bytes[..8] == [0xf0, 0x7e, 0, 6, 2, 0x41, 0x50, 0]
        && bytes.last() == Some(&0xf7)
        && bytes[2..bytes.len() - 1].iter().all(|b| *b < 128)
}

pub struct AirSession<T: Transport> {
    transport: T,
    gate: WriteGate,
    schema: Schema,
    snapshot: Option<Snapshot>,
    timeout: Duration,
}
impl<T: Transport> AirSession<T> {
    /// Sends identity only. No editor-mode setup or cleanup writes are used.
    pub fn connect(mut transport: T, gate: WriteGate) -> Result<Self> {
        let schema = Schema::bundled()?;
        transport.send(&IDENTITY_REQUEST)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            ensure!(
                Instant::now() < deadline,
                "KATANA-AIR identity request timed out; select the matching amplifier and MIDI port"
            );
            if let Some(bytes) = transport.receive(Duration::from_millis(20))?
                && identity_reply(&bytes)
            {
                break;
            }
        }
        Ok(Self {
            transport,
            gate,
            schema,
            snapshot: None,
            timeout: Duration::from_secs(4),
        })
    }
    pub fn snapshot(&self) -> Option<&Snapshot> {
        self.snapshot.as_ref()
    }
    pub fn is_connected(&self) -> bool {
        self.transport.is_connected()
    }
    fn read(&mut self, address: Address, size: usize) -> Result<Vec<u8>> {
        ensure!((1..=128).contains(&size), "Air reads must be 1..128 bytes");
        address.plus(size - 1)?;
        self.transport
            .send(&frame(0x11, address, &Address::new(size as u32)?.bytes())?)?;
        let deadline = Instant::now() + self.timeout;
        let mut bytes = vec![None; size];
        loop {
            ensure!(
                Instant::now() < deadline,
                "KATANA-AIR read at {address} timed out; refresh before editing"
            );
            if let Some(response) = self.transport.receive(Duration::from_millis(20))?
                && let Ok((start, data)) = decode_data(&response)
            {
                for (offset, value) in data.iter().enumerate() {
                    let linear = start.linear() + offset as u32;
                    if let Some(index) = linear
                        .checked_sub(address.linear())
                        .filter(|i| (*i as usize) < size)
                    {
                        let slot = &mut bytes[index as usize];
                        ensure!(
                            slot.is_none() || *slot == Some(*value),
                            "conflicting Air read response"
                        );
                        *slot = Some(*value);
                    }
                }
                if let Some(complete) = bytes.iter().copied().collect::<Option<Vec<_>>>() {
                    return Ok(complete);
                }
            }
        }
    }
    pub fn refresh(&mut self) -> Result<Snapshot> {
        self.snapshot = None;
        let mut memory = BTreeMap::new();
        let start = Address::packed(0x60000000)?;
        let size = 0x980usize;
        for offset in (0..size).step_by(128) {
            let address = start.plus(offset)?;
            let bytes = self
                .read(address, (size - offset).min(128))
                .with_context(|| format!("Air refresh incomplete at {address}"))?;
            memory.extend(
                bytes
                    .into_iter()
                    .enumerate()
                    .map(|(i, b)| (address.linear() + i as u32, b)),
            );
        }
        let mut warnings = Vec::new();
        // Some firmware does not return these optional system blocks. Never
        // reuse old globals: absent bytes leave the associated controls disabled.
        for (packed, size) in [(0x00000430, 15usize), (0x00010000, 2)] {
            let address = Address::packed(packed)?;
            match self.read(address, size) {
                Ok(bytes) => memory.extend(
                    bytes
                        .into_iter()
                        .enumerate()
                        .map(|(i, b)| (address.linear() + i as u32, b)),
                ),
                Err(error) => warnings.push(format!(
                    "Optional Air setting at {address} unavailable: {error}"
                )),
            }
        }
        let mut snapshot = Snapshot::from_memory(memory, &self.schema);
        snapshot.warnings = warnings;
        self.snapshot = Some(snapshot.clone());
        Ok(snapshot)
    }
    pub fn set_parameter(&mut self, id: &str, value: u16) -> Result<Snapshot> {
        let result = self.set_parameter_inner(id, value);
        if result.is_err() {
            self.snapshot = None;
        }
        result
    }
    fn set_parameter_inner(&mut self, id: &str, value: u16) -> Result<Snapshot> {
        ensure!(self.snapshot.is_some(), "Refresh the Air before editing");
        let p = self
            .schema
            .parameters
            .get(id)
            .context("Unknown Air parameter")?
            .clone();
        ensure!(
            p.options.contains_key(&value),
            "Value outside the Air parameter range"
        );
        let address = Address::from_bytes(p.address)?;
        let original = self.read(address, p.bytes)?;
        let old = original.iter().fold(0u16, |v, b| v * 128 + *b as u16);
        ensure!(
            p.options.contains_key(&old),
            "Air returned an unknown current value; no edit was sent"
        );
        let expected = if p.bytes == 1 {
            vec![value as u8]
        } else {
            vec![(value / 128) as u8, (value % 128) as u8]
        };
        if original != expected {
            let packet = frame(0x12, address, &expected)?;
            // Check the shared gate immediately before every edit packet. Gen 3's
            // gate decoder deliberately does not recognize this model framing.
            let access = self.gate.access();
            ensure!(
                access != Access::ReadOnly,
                "Preset protection: Air editing requires Preview permission"
            );
            ensure!(
                p.address[0] == 0x60 || access == Access::Persistent,
                "Air cabinet/global EQ requires persistent-write permission"
            );
            self.transport.send(&packet)?;
            self.transport.wait(Duration::from_millis(70))?;
            ensure!(
                self.read(address, p.bytes)? == expected,
                "Air edit verification failed; refresh and check this setting on the amplifier"
            );
        }
        let mut memory = self
            .snapshot
            .as_ref()
            .expect("checked snapshot")
            .memory
            .clone();
        memory.extend(
            expected
                .into_iter()
                .enumerate()
                .map(|(i, b)| (address.linear() + i as u32, b)),
        );
        let mut snapshot = Snapshot::from_memory(memory, &self.schema);
        snapshot.warnings = self.snapshot.as_ref().unwrap().warnings.clone();
        self.snapshot = Some(snapshot.clone());
        Ok(snapshot)
    }
    pub fn select_channel(&mut self, index: usize) -> Result<Snapshot> {
        let result = self.select_channel_inner(index);
        if result.is_err() {
            self.snapshot = None;
        }
        result
    }
    fn select_channel_inner(&mut self, index: usize) -> Result<Snapshot> {
        ensure!(
            self.snapshot.is_some(),
            "Refresh the Air before changing channels"
        );
        let program = *CHANNEL_PROGRAMS.get(index).context("Unknown Air channel")?;
        let address = Address::packed(0x00010000)?;
        let original = self.read(address, 2)?;
        ensure!(
            original[0] == 0 && CHANNEL_PROGRAMS.contains(&original[1]),
            "Air returned an unknown channel; no recall was sent"
        );
        let expected = [0, program];
        if original != expected {
            let packet = frame(0x12, address, &expected)?;
            ensure!(
                self.gate.access() != Access::ReadOnly,
                "Preset protection: Air channel recall requires Preview permission"
            );
            self.transport.send(&packet)?;
            self.transport.wait(Duration::from_millis(100))?;
            ensure!(
                self.read(address, 2)? == expected,
                "Experimental Air channel recall failed verification; refresh and check the amplifier"
            );
        }
        self.refresh()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, collections::VecDeque, rc::Rc};
    #[derive(Default)]
    struct Device {
        sent: Vec<Vec<u8>>,
        responses: VecDeque<Vec<u8>>,
        memory: BTreeMap<u32, u8>,
        ignore_write: bool,
        revoke_on_read: Option<WriteGate>,
        fragment: bool,
        omit_globals: bool,
    }
    struct Mock(Rc<RefCell<Device>>);
    impl Transport for Mock {
        fn send(&mut self, bytes: &[u8]) -> Result<()> {
            let mut d = self.0.borrow_mut();
            d.sent.push(bytes.to_vec());
            if bytes == IDENTITY_REQUEST {
                d.responses.push_back(vec![
                    0xf0, 0x7e, 0, 6, 2, 0x41, 0x50, 0, 0, 0, 0, 0, 0, 0, 0xf7,
                ]);
                return Ok(());
            }
            let address = Address::from_bytes(bytes[8..12].try_into()?)?;
            match bytes[7] {
                0x11 => {
                    if d.omit_globals && address == Address::packed(0x00000430)? {
                        return Ok(());
                    }
                    let size = Address::from_bytes(bytes[12..16].try_into()?)?.linear() as usize;
                    assert!(size <= 128);
                    if let Some(gate) = d.revoke_on_read.take() {
                        gate.set(Access::ReadOnly);
                    }
                    let values: Vec<_> = (0..size)
                        .map(|i| *d.memory.get(&(address.linear() + i as u32)).unwrap_or(&0))
                        .collect();
                    if d.fragment && size > 1 {
                        // Reordered split responses plus an unrelated, valid frame.
                        d.responses
                            .push_back(frame(0x12, Address::packed(0x6000127f)?, &[1])?);
                        let half = size / 2;
                        d.responses
                            .push_back(frame(0x12, address.plus(half)?, &values[half..])?);
                        d.responses
                            .push_back(frame(0x12, address, &values[..half])?);
                    } else {
                        d.responses.push_back(frame(0x12, address, &values)?);
                    }
                }
                0x12 if !d.ignore_write => {
                    d.memory.extend(
                        bytes[12..bytes.len() - 2]
                            .iter()
                            .copied()
                            .enumerate()
                            .map(|(i, b)| (address.linear() + i as u32, b)),
                    );
                }
                0x12 => {}
                _ => anyhow::bail!("unexpected command"),
            }
            Ok(())
        }
        fn receive(&mut self, _: Duration) -> Result<Option<Vec<u8>>> {
            Ok(self.0.borrow_mut().responses.pop_front())
        }
        fn wait(&mut self, _: Duration) -> Result<()> {
            Ok(())
        }
    }
    fn session() -> (AirSession<Mock>, Rc<RefCell<Device>>, WriteGate) {
        let d = Rc::new(RefCell::new(Device::default()));
        let gate = WriteGate::default();
        let s = AirSession::connect(Mock(d.clone()), gate.clone()).unwrap();
        (s, d, gate)
    }
    #[test]
    fn golden_air_framing_has_its_own_model_and_checksum() {
        let a = Address::packed(0x60000052).unwrap();
        let packet = frame(0x12, a, &[50]).unwrap();
        assert_eq!(
            packet,
            [
                0xf0, 0x41, 0, 0, 0, 0, 0x50, 0x12, 0x60, 0, 0, 0x52, 50, 0x1c, 0xf7
            ]
        );
        assert_eq!(decode_data(&packet).unwrap(), (a, &[50][..]));
        assert!(crate::protocol::Message::decode(&packet).is_err());
        let mut bad = packet.clone();
        bad[13] ^= 1;
        assert!(decode_data(&bad).is_err());
        let mut wrong = packet;
        wrong[6] = 0x51;
        assert!(decode_data(&wrong).is_err());
        assert!(!identity_reply(&[
            0xf0, 0x7e, 0x10, 6, 2, 0x41, 7, 5, 0, 0, 5, 0, 0, 0, 0xf7
        ]));
    }
    #[test]
    fn metadata_exposes_only_supported_ranges_and_algorithm_controls() {
        let schema = Schema::bundled().unwrap();
        assert_eq!(schema.parameters.len(), 480);
        assert!(!schema.parameters.contains_key("00010000"));
        assert_eq!(schema.parameters["60000051"].options[&23], "Brown");
        assert_eq!(
            schema
                .blocks
                .iter()
                .find(|b| b.id == "mod")
                .unwrap()
                .variants
                .len(),
            27
        );
    }
    #[test]
    fn connect_and_refresh_are_only_identity_and_bounded_reads() {
        let (mut s, d, _) = session();
        assert_eq!(d.borrow().sent, vec![IDENTITY_REQUEST.to_vec()]);
        // Zero-valued memory is a deliberately imperfect schema value; reads
        // must preserve unknown values and leave their controls disabled.
        let snapshot = s.refresh().unwrap();
        assert_eq!(snapshot.memory.len(), 0x980 + 15 + 2);
        assert_eq!(snapshot.values["60000054"], 0);
        assert!(d.borrow().sent[1..].iter().all(|p| p[7] == 0x11));
        assert_eq!(d.borrow().sent.len(), 22);
        assert_eq!(snapshot.selected_channel, Some(0));
    }
    #[test]
    fn read_tracks_each_byte_across_reordered_fragments() {
        let (mut s, d, _) = session();
        d.borrow_mut().fragment = true;
        let a = Address::packed(0x6000007e).unwrap();
        d.borrow_mut()
            .memory
            .extend((0..4).map(|i| (a.linear() + i, i as u8 + 1)));
        assert_eq!(s.read(a, 4).unwrap(), [1, 2, 3, 4]);
    }
    #[test]
    fn preview_edits_verified_tone_but_blocks_global_edits() {
        let (mut s, d, g) = session();
        s.refresh().unwrap();
        g.set(Access::EditBuffer);
        assert_eq!(
            s.set_parameter("60000054", 42).unwrap().values["60000054"],
            42
        );
        assert_eq!(
            d.borrow()
                .sent
                .iter()
                .filter(|p| p.len() > 7 && p[7] == 0x12)
                .count(),
            1
        );
        assert!(s.set_parameter("00000432", 1).is_err());
        assert!(s.snapshot().is_none());
        assert_eq!(
            d.borrow()
                .sent
                .iter()
                .filter(|p| p.len() > 7 && p[7] == 0x12)
                .count(),
            1
        );
        s.refresh().unwrap();
        g.set(Access::Persistent);
        assert_eq!(
            s.set_parameter("00000432", 1).unwrap().values["00000432"],
            1
        );
    }
    #[test]
    fn permission_revoked_during_preread_blocks_edit_packet() {
        let (mut s, d, g) = session();
        s.refresh().unwrap();
        g.set(Access::EditBuffer);
        d.borrow_mut().revoke_on_read = Some(g);
        assert!(s.set_parameter("60000054", 42).is_err());
        assert!(!d.borrow().sent.iter().any(|p| p.len() > 7 && p[7] == 0x12));
        assert!(s.snapshot().is_none());
    }
    #[test]
    fn unknown_original_and_verification_failure_invalidate_snapshot() {
        let (mut s, d, g) = session();
        s.refresh().unwrap();
        g.set(Access::EditBuffer);
        let a = Address::packed(0x60000054).unwrap().linear();
        d.borrow_mut().memory.insert(a, 127);
        assert!(s.set_parameter("60000054", 42).is_err());
        assert!(!d.borrow().sent.iter().any(|p| p.len() > 7 && p[7] == 0x12));
        d.borrow_mut().memory.insert(a, 0);
        s.refresh().unwrap();
        d.borrow_mut().ignore_write = true;
        assert!(s.set_parameter("60000054", 42).is_err());
        assert!(s.snapshot().is_none());
        assert_eq!(
            d.borrow()
                .sent
                .iter()
                .filter(|p| p.len() > 7 && p[7] == 0x12)
                .count(),
            1
        );
    }
    #[test]
    fn channel_recall_uses_sparse_air_programs_and_preview_gate() {
        let (mut s, d, g) = session();
        s.refresh().unwrap();
        assert!(s.select_channel(4).is_err());
        assert!(!d.borrow().sent.iter().any(|p| p.len() > 7 && p[7] == 0x12));
        s.refresh().unwrap();
        g.set(Access::EditBuffer);
        let snapshot = s.select_channel(4).unwrap();
        assert_eq!(snapshot.selected_channel, Some(4));
        let packets: Vec<_> = d
            .borrow()
            .sent
            .iter()
            .filter(|p| p.len() > 7 && p[7] == 0x12)
            .cloned()
            .collect();
        assert_eq!(packets.len(), 1);
        assert_eq!(
            packets[0],
            frame(0x12, Address::packed(0x00010000).unwrap(), &[0, 5]).unwrap()
        );
        assert!(s.select_channel(7).is_err());
        assert!(s.snapshot().is_none());
    }
    #[test]
    fn incomplete_optional_globals_are_unavailable_in_fresh_tone_snapshot() {
        let (mut s, d, _) = session();
        s.refresh().unwrap();
        d.borrow_mut().omit_globals = true;
        s.timeout = Duration::from_millis(1);
        let snapshot = s.refresh().unwrap();
        assert_eq!(snapshot.values["60000054"], 0);
        assert!(!snapshot.values.contains_key("00000432"));
        assert_eq!(snapshot.warnings.len(), 1);
    }
}
