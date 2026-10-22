//! Linux ALSA raw and sequencer MIDI; selecting a port sends nothing until Connect.
use crate::{protocol::Decoder, session::Transport};
use alsa::{
    Ctl, Direction, card,
    rawmidi::{Iter, Rawmidi},
    seq::{Addr, ClientIter, Event, EventType, PortCap, PortIter, PortSubscribe, PortType, Seq},
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
#[derive(Clone, Debug)]
pub struct Port {
    pub address: String,
    pub name: String,
}
pub fn ports() -> Result<Vec<Port>> {
    let mut ports = BTreeMap::new();
    for c in card::Iter::new() {
        let c = c?;
        let ctl = Ctl::from_card(&c, false)?;
        for info in Iter::new(&ctl) {
            let info = info?;
            let address = format!(
                "hw:{},{},{}",
                c.get_index(),
                info.get_device(),
                info.get_subdevice()
            );
            let name = format!(
                "{} — {}",
                c.get_name()?,
                info.get_subdevice_name().unwrap_or_default()
            );
            let entry = ports.entry(address).or_insert((name, false, false));
            if info.get_stream() == Direction::Capture {
                entry.1 = true;
            } else {
                entry.2 = true;
            }
        }
    }
    Ok(ports
        .into_iter()
        .filter(|(_, (_, i, o))| *i && *o)
        .map(|(address, (name, _, _))| Port { address, name })
        .collect())
}

/// Enumerate duplex sequencer ports, including BLE-MIDI devices exported by BlueZ.
/// Bluetooth pairing and connection belong to the OS; discovery never connects
/// or subscribes to a device and never transmits MIDI.
pub fn sequencer_ports() -> Result<Vec<Port>> {
    let seq = Seq::open(None, None, true).context("open ALSA MIDI sequencer")?;
    let own_client = seq.client_id()?;
    let required = PortCap::READ | PortCap::SUBS_READ | PortCap::WRITE | PortCap::SUBS_WRITE;
    let mut ports = Vec::new();
    for client in ClientIter::new(&seq) {
        if client.get_client() == own_client {
            continue;
        }
        for port in PortIter::new(&seq, client.get_client()) {
            let capabilities = port.get_capability();
            if capabilities.contains(required) && !capabilities.contains(PortCap::NO_EXPORT) {
                ports.push(Port {
                    address: format!("seq:{}:{}", port.get_client(), port.get_port()),
                    name: format!("{} — {}", client.get_name()?, port.get_name()?),
                });
            }
        }
    }
    Ok(ports)
}

fn sequencer_address(port: &str) -> Result<Addr> {
    let mut parts = port.split(':');
    ensure!(parts.next() == Some("seq"), "expected seq:client:port");
    let client: u8 = parts
        .next()
        .context("missing sequencer client")?
        .parse()
        .context("invalid sequencer client")?;
    let port: u8 = parts
        .next()
        .context("missing sequencer port")?
        .parse()
        .context("invalid sequencer port")?;
    ensure!(parts.next().is_none(), "expected seq:client:port");
    Ok(Addr {
        client: i32::from(client),
        port: i32::from(port),
    })
}

/// MIDI transport for an already paired, OS-connected BLE-MIDI device.
/// Closing `seq` also removes both subscriptions, including on a failed open.
pub struct SequencerTransport {
    seq: Seq,
    local_port: i32,
    decoder: Decoder,
    frames: std::collections::VecDeque<Vec<u8>>,
    next_send: Instant,
    pub cancel: Arc<AtomicBool>,
}

impl SequencerTransport {
    pub fn open(port: &str, cancel: Arc<AtomicBool>) -> Result<Self> {
        let remote = sequencer_address(port)?;
        ensure!(!cancel.load(Ordering::Relaxed), "operation cancelled");
        let seq = Seq::open(None, None, true).context("open ALSA MIDI sequencer")?;
        seq.set_client_name(c"Rusty Boss")?;
        let local_port = seq.create_simple_port(
            c"Amplifier MIDI",
            PortCap::READ | PortCap::SUBS_READ | PortCap::WRITE | PortCap::SUBS_WRITE,
            PortType::MIDI_GENERIC | PortType::APPLICATION,
        )?;
        let local = Addr {
            client: seq.client_id()?,
            port: local_port,
        };
        let incoming = PortSubscribe::empty()?;
        incoming.set_sender(remote);
        incoming.set_dest(local);
        seq.subscribe_port(&incoming)
            .context("subscribe ALSA sequencer MIDI input")?;
        let outgoing = PortSubscribe::empty()?;
        outgoing.set_sender(local);
        outgoing.set_dest(remote);
        seq.subscribe_port(&outgoing)
            .context("subscribe ALSA sequencer MIDI output")?;
        Ok(Self {
            seq,
            local_port,
            decoder: Decoder::default(),
            frames: Default::default(),
            next_send: Instant::now(),
            cancel,
        })
    }

    fn check(&self) -> Result<()> {
        ensure!(
            !self.cancel.load(Ordering::Relaxed),
            "operation cancelled; hardware state may be partial"
        );
        Ok(())
    }
}

impl Transport for SequencerTransport {
    fn wait(&mut self, duration: Duration) -> Result<()> {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            self.check()?;
            thread::sleep(Duration::from_millis(10));
        }
        self.check()
    }

    fn send(&mut self, bytes: &[u8]) -> Result<()> {
        ensure!(
            bytes.first() == Some(&0xf0) && bytes.last() == Some(&0xf7),
            "ALSA sequencer transport requires a complete SysEx frame"
        );
        while Instant::now() < self.next_send {
            self.check()?;
            thread::sleep(Duration::from_millis(2));
        }
        let mut event = Event::new_ext(EventType::Sysex, bytes);
        event.set_source(self.local_port);
        event.set_subs();
        event.set_direct();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            self.check()?;
            ensure!(
                Instant::now() < deadline,
                "ALSA sequencer write timed out; output state unknown"
            );
            match self.seq.event_output_direct(&mut event) {
                Ok(_) => break,
                Err(e) if e.errno() == 11 => thread::sleep(Duration::from_millis(2)),
                Err(e) => return Err(e).context("ALSA sequencer output disconnected"),
            }
        }
        self.next_send =
            Instant::now() + Duration::from_micros(20000 + bytes.len() as u64 * 1_000_000 / 3125);
        Ok(())
    }

    fn receive(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let deadline = Instant::now() + timeout;
        // Input borrows Seq, so keep it scoped to this call rather than storing
        // a self-referential handle. Decoder survives calls and BLE fragments.
        let mut input = self.seq.input();
        loop {
            self.check()?;
            if let Some(frame) = self.frames.pop_front() {
                return Ok(Some(frame));
            }
            let mut idle = false;
            match input.event_input() {
                Ok(event) if event.get_type() == EventType::Sysex => {
                    if let Some(bytes) = event.get_ext() {
                        self.frames.extend(self.decoder.feed(bytes));
                    }
                }
                Ok(_) => {}
                Err(e) if e.errno() == 11 => idle = true,
                Err(e) => return Err(e).context("ALSA sequencer input disconnected"),
            }
            if Instant::now() >= deadline {
                return Ok(self.frames.pop_front());
            }
            if idle {
                thread::sleep(Duration::from_millis(2));
            }
        }
    }
}
pub struct AlsaTransport {
    input: Rawmidi,
    output: Rawmidi,
    decoder: Decoder,
    frames: std::collections::VecDeque<Vec<u8>>,
    next_send: Instant,
    pub cancel: Arc<AtomicBool>,
}
fn would_block(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::WouldBlock || matches!(e.raw_os_error(), Some(11) | Some(-11))
}
impl AlsaTransport {
    pub fn open(port: &str, cancel: Arc<AtomicBool>) -> Result<Self> {
        ensure!(!port.contains('\0'), "invalid port name");
        Ok(Self {
            input: Rawmidi::new(port, Direction::Capture, true).context("open ALSA MIDI input")?,
            output: Rawmidi::new(port, Direction::Playback, true)
                .context("open ALSA MIDI output")?,
            decoder: Decoder::default(),
            frames: Default::default(),
            next_send: Instant::now(),
            cancel,
        })
    }
    fn check(&self) -> Result<()> {
        ensure!(
            !self.cancel.load(Ordering::Relaxed),
            "operation cancelled; hardware state may be partial"
        );
        Ok(())
    }
}
impl Transport for AlsaTransport {
    fn wait(&mut self, duration: Duration) -> Result<()> {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            self.check()?;
            thread::sleep(Duration::from_millis(10));
        }
        self.check()
    }
    fn send(&mut self, bytes: &[u8]) -> Result<()> {
        while Instant::now() < self.next_send {
            self.check()?;
            thread::sleep(Duration::from_millis(2));
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut offset = 0;
        while offset < bytes.len() {
            self.check()?;
            ensure!(
                Instant::now() < deadline,
                "ALSA write timed out; output state unknown"
            );
            match self.output.io().write(&bytes[offset..]) {
                Ok(0) => thread::sleep(Duration::from_millis(2)),
                Ok(n) => offset += n,
                Err(e) if would_block(&e) => thread::sleep(Duration::from_millis(2)),
                Err(e) => return Err(e).context("ALSA output disconnected"),
            }
        }
        self.next_send =
            Instant::now() + Duration::from_micros(20000 + bytes.len() as u64 * 1_000_000 / 3125);
        Ok(())
    }
    fn receive(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let deadline = Instant::now() + timeout;
        loop {
            self.check()?;
            if let Some(frame) = self.frames.pop_front() {
                return Ok(Some(frame));
            }
            let mut buffer = [0; 1024];
            match self.input.io().read(&mut buffer) {
                Ok(n) if n > 0 => self.frames.extend(self.decoder.feed(&buffer[..n])),
                Ok(_) => {}
                Err(e) if would_block(&e) => {}
                Err(e) => return Err(e).context("ALSA input disconnected"),
            }
            if Instant::now() >= deadline {
                return Ok(self.frames.pop_front());
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequencer_addresses_are_explicit_and_bounded() {
        let address = sequencer_address("seq:128:0").unwrap();
        assert_eq!((address.client, address.port), (128, 0));
        for invalid in [
            "128:0",
            "hw:1,0,0",
            "seq:128",
            "seq:128:0:1",
            "seq:-1:0",
            "seq:256:0",
            "seq:128:256",
            "seq:128:air",
            "seq::0",
            "seq:128:0\0",
        ] {
            assert!(sequencer_address(invalid).is_err(), "{invalid:?}");
        }
    }
}
