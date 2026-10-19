//! A last-line, per-packet write gate. UI hiding alone is not preset protection.
use crate::{
    protocol::{Address, Message, IDENTITY_REQUEST},
    session::Transport,
};
use anyhow::{ensure, Result};
use std::{
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    time::Duration,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Access {
    ReadOnly = 0,
    EditBuffer = 1,
    Persistent = 2,
}
#[derive(Clone, Default)]
pub struct WriteGate(Arc<AtomicU8>);
impl WriteGate {
    pub fn set(&self, access: Access) {
        self.0.store(access as u8, Ordering::SeqCst);
    }
    pub fn access(&self) -> Access {
        match self.0.load(Ordering::SeqCst) {
            1 => Access::EditBuffer,
            2 => Access::Persistent,
            _ => Access::ReadOnly,
        }
    }
    pub fn check(&self, frame: &[u8]) -> Result<()> {
        if frame == IDENTITY_REQUEST {
            return Ok(());
        }
        match Message::decode(frame)? {
            Message::Request { .. } => Ok(()),
            Message::Data { address, data, .. } => {
                // Enter/leave editor session mode, never channel selection/reset.
                if address == Address::packed(0x7f000001)? && (data == [0] || data == [1]) {
                    return Ok(());
                }
                let access = self.access();
                ensure!(
                    access != Access::ReadOnly,
                    "Preset protection: hardware writes are disabled"
                );
                let start = Address::packed(0x20000000)?.linear();
                let stop = Address::packed(0x20100000)?.linear();
                let end = address.plus(data.len() - 1)?.linear();
                let active_patch = address.linear() >= start && end < stop;
                let select = address == Address::packed(0x7f000100)?
                    && data.len() == 2
                    && data[0] == 0
                    && data[1] < 9;
                let macro_knob = ((Address::packed(0x30000200)?.linear()
                    ..=Address::packed(0x30000204)?.linear())
                    .contains(&address.linear())
                    || address == Address::packed(0x30000400)?)
                    && data.len() == 1
                    && (1..=101).contains(&data[0]);
                let tap = (Address::packed(0x7f010106)?.linear()
                    ..=Address::packed(0x7f010108)?.linear())
                    .contains(&address.linear())
                    && data == [0];
                ensure!(active_patch || select || macro_knob || tap || access==Access::Persistent,
                    "Preset protection: stored channels, system settings and export commands require persistent-write permission");
                Ok(())
            }
        }
    }
}
/// Absolute no-write transport for hardware diagnostics. Unlike the GUI's
/// protected editor mode, this has NO editor-mode or cleanup exception and no
/// permission switch. Every DT1, program change and unrecognized frame fails
/// before reaching the underlying transport.
pub struct ReadRequestsOnly<T: Transport> {
    inner: T,
}
impl<T: Transport> ReadRequestsOnly<T> {
    pub fn new(inner: T) -> Self {
        Self { inner }
    }
}
impl<T: Transport> Transport for ReadRequestsOnly<T> {
    fn send(&mut self, frame: &[u8]) -> Result<()> {
        ensure!(
            frame == IDENTITY_REQUEST
                || matches!(Message::decode(frame), Ok(Message::Request { .. })),
            "Strict read-only diagnostics: only identity and RQ1 permitted; ALL writes blocked"
        );
        self.inner.send(frame)
    }
    fn receive(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        self.inner.receive(timeout)
    }
    fn wait(&mut self, duration: Duration) -> Result<()> {
        self.inner.wait(duration)
    }
}

pub struct Guarded<T: Transport> {
    inner: T,
    gate: WriteGate,
    muted_by_us: bool,
}
impl<T: Transport> Guarded<T> {
    pub fn new(inner: T, gate: WriteGate) -> Self {
        Self {
            inner,
            gate,
            muted_by_us: false,
        }
    }
}
impl<T: Transport> Transport for Guarded<T> {
    fn send(&mut self, frame: &[u8]) -> Result<()> {
        let mute = match Message::decode(frame) {
            Ok(Message::Data { address, data, .. }) if address == Address::packed(0x7f010109)? => {
                Some(data)
            }
            _ => None,
        };
        // Revoking permission during a commit must not block cleanup of a mute
        // this transport already sent. This exception cannot enable a fresh mute.
        if !(self.muted_by_us && mute.as_deref() == Some(&[0])) {
            self.gate.check(frame)?;
        }
        self.inner.send(frame)?;
        if let Some(data) = mute {
            if data == [1] {
                self.muted_by_us = true;
            } else if data == [0] {
                self.muted_by_us = false;
            }
        }
        Ok(())
    }
    fn receive(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        self.inner.receive(timeout)
    }
    fn wait(&mut self, duration: Duration) -> Result<()> {
        self.inner.wait(duration)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn dt(addr: u32, data: Vec<u8>) -> Vec<u8> {
        Message::Data {
            device: 16,
            address: Address::packed(addr).unwrap(),
            data,
        }
        .encode()
        .unwrap()
    }
    #[test]
    fn strict_observer_rejects_all_writes_before_transport() {
        struct Sink(usize);
        impl Transport for Sink {
            fn send(&mut self, _: &[u8]) -> Result<()> {
                self.0 += 1;
                Ok(())
            }
            fn receive(&mut self, _: Duration) -> Result<Option<Vec<u8>>> {
                Ok(None)
            }
        }
        let mut t = ReadRequestsOnly::new(Sink(0));
        for address in [
            0x7f000001, 0x20000600, 0x20100000, 0x30000204, 0x7f000100, 0x7f000104, 0x7f010109,
        ] {
            for value in [0, 1, 50] {
                assert!(t.send(&dt(address, vec![value])).is_err());
            }
        }
        for frame in [&[0xc0, 1][..], &[0xb0, 1, 50][..], &[0xf0, 1, 0xf7][..]] {
            assert!(t.send(frame).is_err());
        }
        assert_eq!(t.inner.0, 0);
        t.send(&IDENTITY_REQUEST).unwrap();
        let rq = Message::Request {
            device: 16,
            address: Address::packed(0x20011c00).unwrap(),
            size: 5,
        }
        .encode()
        .unwrap();
        t.send(&rq).unwrap();
        assert_eq!(t.inner.0, 2);
    }
    #[test]
    fn readonly_allows_reads_and_session_mode_only() {
        let g = WriteGate::default();
        assert!(g.check(&IDENTITY_REQUEST).is_ok());
        assert!(g
            .check(
                &Message::Request {
                    device: 16,
                    address: Address::packed(0x20100000).unwrap(),
                    size: 16
                }
                .encode()
                .unwrap()
            )
            .is_ok());
        for n in [0, 1] {
            assert!(g.check(&dt(0x7f000001, vec![n])).is_ok());
        }
        for a in [
            0x20000600, 0x20100000, 0x10000000, 0x7f000100, 0x7f000104, 0x7f000106, 0x7f00010e,
            0x7f01010a,
        ] {
            assert!(g.check(&dt(a, vec![0])).is_err());
        }
    }
    #[test]
    fn edit_buffer_never_allows_persistent_writes() {
        let g = WriteGate::default();
        g.set(Access::EditBuffer);
        assert!(g.check(&dt(0x20000600, vec![50])).is_ok());
        assert!(g.check(&dt(0x7f000100, vec![0, 1])).is_ok());
        for a in [
            0x20100000, 0x10000200, 0x7f000104, 0x7f000106, 0x7f00010e, 0x7f010109, 0x7f01010a,
        ] {
            assert!(g.check(&dt(a, vec![0])).is_err());
        }
        g.set(Access::Persistent);
        assert!(g.check(&dt(0x7f000104, vec![0, 1])).is_ok());
        g.set(Access::ReadOnly);
        assert!(g.check(&dt(0x20000600, vec![50])).is_err());
    }
    #[test]
    fn preview_permission_allows_only_bounded_macros_and_tap_presses() {
        let g = WriteGate::default();
        g.set(Access::EditBuffer);
        for addr in [0x30000200, 0x30000204, 0x30000400] {
            for value in [1, 53, 101] {
                assert!(g.check(&dt(addr, vec![value])).is_ok());
            }
            for data in [vec![0], vec![102], vec![127], vec![50, 50]] {
                assert!(g.check(&dt(addr, data)).is_err());
            }
        }
        for addr in [0x7f010106, 0x7f010107, 0x7f010108] {
            assert!(g.check(&dt(addr, vec![0])).is_ok());
            assert!(g.check(&dt(addr, vec![1])).is_err());
        }
        for addr in [0x30000205, 0x30000300, 0x30000401, 0x7f010109] {
            assert!(g.check(&dt(addr, vec![1])).is_err());
        }
        g.set(Access::ReadOnly);
        assert!(g.check(&dt(0x30000204, vec![53])).is_err());
        assert!(g.check(&dt(0x7f010106, vec![0])).is_err());
    }
    #[test]
    fn permission_revocation_still_allows_our_own_unmute_cleanup() {
        struct Sink;
        impl Transport for Sink {
            fn send(&mut self, _: &[u8]) -> Result<()> {
                Ok(())
            }
            fn receive(&mut self, _: Duration) -> Result<Option<Vec<u8>>> {
                Ok(None)
            }
        }
        let g = WriteGate::default();
        let mut t = Guarded::new(Sink, g.clone());
        assert!(t.send(&dt(0x7f010109, vec![0])).is_err());
        g.set(Access::Persistent);
        t.send(&dt(0x7f010109, vec![1])).unwrap();
        g.set(Access::ReadOnly);
        assert!(t.send(&dt(0x7f01010a, vec![64, 1])).is_err());
        t.send(&dt(0x7f010109, vec![0])).unwrap();
        assert!(!t.muted_by_us);
        assert!(t.send(&dt(0x7f010109, vec![1])).is_err());
    }
    #[test]
    fn a_rejected_packet_never_reaches_transport() {
        struct Sink(usize);
        impl Transport for Sink {
            fn send(&mut self, _: &[u8]) -> Result<()> {
                self.0 += 1;
                Ok(())
            }
            fn receive(&mut self, _: Duration) -> Result<Option<Vec<u8>>> {
                Ok(None)
            }
        }
        let gate = WriteGate::default();
        let mut guarded = Guarded::new(Sink(0), gate.clone());
        assert!(guarded.send(&dt(0x7f000104, vec![0, 1])).is_err());
        assert_eq!(guarded.inner.0, 0);
        gate.set(Access::EditBuffer);
        guarded.send(&dt(0x20000600, vec![50])).unwrap();
        assert_eq!(guarded.inner.0, 1);
        gate.set(Access::ReadOnly);
        assert!(guarded.send(&dt(0x20000600, vec![1])).is_err());
        assert_eq!(guarded.inner.0, 1);
    }
}
