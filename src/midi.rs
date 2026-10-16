//! Linux ALSA raw MIDI; selecting a port does not send anything until Connect.
use crate::{protocol::Decoder, session::Transport};
use alsa::{
    card,
    rawmidi::{Iter, Rawmidi},
    Ctl, Direction,
};
use anyhow::{ensure, Context, Result};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
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
