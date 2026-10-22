//! BLE MIDI 1.0 framing. No amplifier identities, addresses or commands belong here.
use anyhow::{Result, bail, ensure};

pub const MAX_MESSAGE: usize = 4096;

/// Encode one complete MIDI message. `capacity` is ATT MTU minus three bytes.
/// Keep the timestamp and EOX together, even at a packet boundary.
pub fn encode(message: &[u8], milliseconds: u16, capacity: usize) -> Result<Vec<Vec<u8>>> {
    ensure!(capacity >= 5, "Bluetooth MIDI packet capacity is too small");
    ensure!(
        !message.is_empty() && message.len() <= MAX_MESSAGE,
        "Invalid MIDI message size"
    );
    let header = 0x80 | ((milliseconds >> 7) as u8 & 0x3f);
    let timestamp = 0x80 | (milliseconds as u8 & 0x7f);
    if message[0] != 0xf0 {
        ensure!(
            message.len() == message_size(message[0]).unwrap_or(0)
                && message[1..].iter().all(|b| *b < 128),
            "Expected one complete MIDI message"
        );
        let mut packet = vec![header, timestamp];
        packet.extend(message);
        return Ok(vec![packet]);
    }
    ensure!(
        message.len() >= 2
            && message.last() == Some(&0xf7)
            && message[1..message.len() - 1].iter().all(|b| *b < 128),
        "Invalid MIDI SysEx message"
    );
    let mut remaining = &message[..message.len() - 1];
    let mut packets = Vec::new();
    let mut first = true;
    loop {
        let mut packet = vec![header];
        if first {
            packet.push(timestamp);
            first = false;
        }
        let available = capacity - packet.len();
        if remaining.len() + 2 <= available {
            packet.extend(remaining);
            packet.extend([timestamp, 0xf7]);
            packets.push(packet);
            return Ok(packets);
        }
        let count = remaining.len().min(available);
        packet.extend(&remaining[..count]);
        remaining = &remaining[count..];
        packets.push(packet);
    }
}

fn message_size(status: u8) -> Option<usize> {
    match status {
        0x80..=0xbf | 0xe0..=0xef | 0xf2 => Some(3),
        0xc0..=0xdf | 0xf1 | 0xf3 => Some(2),
        0xf6 | 0xf8..=0xff => Some(1),
        _ => None,
    }
}

#[derive(Default)]
pub struct Decoder {
    sysex: Vec<u8>,
}
impl Decoder {
    /// Reassemble SysEx continuations, strip timestamps, restore running status,
    /// and deliver real-time messages independently of any unfinished SysEx.
    pub fn feed(&mut self, packet: &[u8]) -> Result<Vec<Vec<u8>>> {
        let result = self.decode(packet);
        if result.is_err() {
            self.sysex.clear();
        }
        result
    }
    fn decode(&mut self, packet: &[u8]) -> Result<Vec<Vec<u8>>> {
        ensure!(
            (2..=512).contains(&packet.len()) && packet[0] & 0xc0 == 0x80,
            "Invalid Bluetooth MIDI header"
        );
        let mut frames = Vec::new();
        let mut timestamp = false;
        let mut running = None;
        let mut short = Vec::new();
        for &byte in &packet[1..] {
            if !timestamp && byte >= 128 {
                timestamp = true;
                continue;
            }
            // A high byte after a timestamp is an actual MIDI status. A low
            // byte can be running-status data, or SysEx continuation data.
            timestamp = false;
            if byte >= 0xf8 {
                frames.push(vec![byte]);
                continue;
            }
            if byte == 0xf0 {
                self.sysex.clear();
                self.sysex.push(byte);
                short.clear();
                running = None;
            } else if !self.sysex.is_empty() {
                ensure!(
                    byte < 128 || byte == 0xf7,
                    "Unexpected status inside Bluetooth SysEx"
                );
                ensure!(
                    self.sysex.len() < MAX_MESSAGE,
                    "Bluetooth SysEx exceeded buffer limit"
                );
                self.sysex.push(byte);
                if byte == 0xf7 {
                    frames.push(std::mem::take(&mut self.sysex));
                }
            } else if byte >= 128 {
                let size = message_size(byte)
                    .ok_or_else(|| anyhow::anyhow!("Invalid Bluetooth MIDI status"))?;
                ensure!(short.is_empty(), "Incomplete Bluetooth MIDI message");
                running = (byte < 0xf0).then_some(byte);
                if size == 1 {
                    frames.push(vec![byte]);
                } else {
                    short.push(byte);
                }
            } else {
                if short.is_empty() {
                    short
                        .push(running.ok_or_else(|| {
                            anyhow::anyhow!("Bluetooth MIDI data without status")
                        })?);
                }
                short.push(byte);
                if short.len() == message_size(short[0]).unwrap_or(0) {
                    frames.push(std::mem::take(&mut short));
                }
            }
        }
        if timestamp || !short.is_empty() {
            bail!("Truncated Bluetooth MIDI message");
        }
        Ok(frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sysex_roundtrip_at_all_small_packet_boundaries_and_timestamp_values() {
        for capacity in [5, 20, 61, 244] {
            for size in 2..300 {
                let mut message = vec![0xf0];
                message.extend((0..size - 2).map(|i| (i % 128) as u8));
                message.push(0xf7);
                for time in [0, 127, 128, 0x70, 0x77, 8191] {
                    let packets = encode(&message, time, capacity).unwrap();
                    let mut decoder = Decoder::default();
                    let mut frames = Vec::new();
                    for packet in &packets {
                        assert!(packet.len() <= capacity);
                        frames.extend(decoder.feed(packet).unwrap());
                    }
                    assert_eq!(frames, vec![message.clone()]);
                    let last = packets.last().unwrap();
                    assert_eq!(&last[last.len() - 2..], &[0x80 | (time as u8 & 127), 0xf7]);
                }
            }
        }
    }
    #[test]
    fn incoming_fixture_handles_continuations_realtime_and_running_status() {
        let mut d = Decoder::default();
        assert!(d.feed(&[0x80, 0x80, 0xf0, 0x41, 0x00]).unwrap().is_empty());
        assert_eq!(
            d.feed(&[0x80, 0x01, 0x82, 0xf8, 0x02, 0x83, 0xf7]).unwrap(),
            vec![vec![0xf8], vec![0xf0, 0x41, 0, 1, 2, 0xf7]]
        );
        assert_eq!(
            d.feed(&[0x80, 0x80, 0xb0, 7, 64, 8, 65, 0x81, 0xc0, 3])
                .unwrap(),
            vec![vec![0xb0, 7, 64], vec![0xb0, 8, 65], vec![0xc0, 3]]
        );
        assert!(
            d.feed(&[0x80, 0x80, 7, 64]).is_err(),
            "running status cannot cross packets"
        );
    }
    #[test]
    fn corruption_and_overflow_discard_incomplete_sysex() {
        let mut d = Decoder::default();
        d.feed(&[0x80, 0x80, 0xf0, 1]).unwrap();
        assert!(d.feed(&[0, 2]).is_err());
        assert!(d.feed(&[0x80, 2, 0x80, 0xf7]).is_err());
        d.feed(&[0x80, 0x80, 0xf0]).unwrap();
        assert!(
            d.feed(&[vec![0x80], vec![0; MAX_MESSAGE]].concat())
                .is_err()
        );
        for malformed in [vec![], vec![0xf0], vec![0x90, 64], vec![0xf0, 0x80, 0xf7]] {
            assert!(encode(&malformed, 0, 20).is_err());
        }
    }
}
