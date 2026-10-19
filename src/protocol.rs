//! Wire framing inferred from the shipped 1.1.0 JavaScript. No hardware verification.
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};

pub const MODEL_ID: [u8; 3] = [1, 5, 7];
pub const DEFAULT_DEVICE_ID: u8 = 0x10;
pub const IDENTITY_REQUEST: [u8; 6] = [0xf0, 0x7e, 0x7f, 6, 1, 0xf7];
pub const MAX_DATA: usize = 128;

/// A linear 28-bit address, NOT a packed four-byte hex address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Address(u32);
impl Address {
    pub fn new(linear: u32) -> Result<Self> {
        ensure!(linear < (1 << 28), "address exceeds 28 bits");
        Ok(Self(linear))
    }
    pub fn packed(packed: u32) -> Result<Self> {
        Self::from_bytes(packed.to_be_bytes())
    }
    pub fn from_bytes(bytes: [u8; 4]) -> Result<Self> {
        ensure!(bytes.iter().all(|b| *b < 128), "non-seven-bit address");
        Self::new(bytes.iter().fold(0, |n, b| (n << 7) | *b as u32))
    }
    pub fn bytes(self) -> [u8; 4] {
        [
            ((self.0 >> 21) & 127) as u8,
            ((self.0 >> 14) & 127) as u8,
            ((self.0 >> 7) & 127) as u8,
            (self.0 & 127) as u8,
        ]
    }
    pub fn linear(self) -> u32 {
        self.0
    }
    pub fn plus(self, offset: usize) -> Result<Self> {
        let n = u32::try_from(offset)?;
        Self::new(
            self.0
                .checked_add(n)
                .ok_or_else(|| anyhow::anyhow!("address overflow"))?,
        )
    }
}
impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:08X}", u32::from_be_bytes(self.bytes()))
    }
}

pub fn checksum(data: &[u8]) -> u8 {
    (128 - (data.iter().map(|v| *v as u32).sum::<u32>() % 128)) as u8 & 127
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    Data {
        device: u8,
        address: Address,
        data: Vec<u8>,
    },
    Request {
        device: u8,
        address: Address,
        size: u32,
    },
}
impl Message {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let (device, address, command, data) = match self {
            Self::Data {
                device,
                address,
                data,
            } => {
                ensure!(
                    !data.is_empty() && data.len() <= MAX_DATA,
                    "DT1 payload must be 1..128 bytes"
                );
                address.plus(data.len() - 1)?;
                (*device, *address, 0x12, data.clone())
            }
            Self::Request {
                device,
                address,
                size,
            } => {
                ensure!(*size > 0, "empty request");
                address.plus(*size as usize - 1)?;
                (
                    *device,
                    *address,
                    0x11,
                    Address::new(*size)?.bytes().to_vec(),
                )
            }
        };
        ensure!(
            device < 128 && data.iter().all(|b| *b < 128),
            "non-seven-bit MIDI data"
        );
        let mut out = vec![0xf0, 0x41, device];
        out.extend(MODEL_ID);
        out.push(command);
        out.extend(address.bytes());
        out.extend(data);
        out.push(checksum(&out[7..]));
        out.push(0xf7);
        Ok(out)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() >= 14 && bytes[0] == 0xf0 && bytes.last() == Some(&0xf7),
            "invalid SysEx framing"
        );
        ensure!(
            bytes[1] == 0x41 && bytes[3..6] == MODEL_ID,
            "not a Katana Gen 3 message"
        );
        ensure!(
            bytes[2..bytes.len() - 1].iter().all(|b| *b < 128),
            "non-seven-bit message"
        );
        ensure!(
            checksum(&bytes[7..bytes.len() - 1]) == 0,
            "bad Roland checksum"
        );
        let address = Address::from_bytes(bytes[7..11].try_into()?)?;
        let data = &bytes[11..bytes.len() - 2];
        match bytes[6] {
            0x12 => {
                ensure!(data.len() <= MAX_DATA, "oversized DT1");
                address.plus(data.len() - 1)?;
                Ok(Self::Data {
                    device: bytes[2],
                    address,
                    data: data.to_vec(),
                })
            }
            0x11 => {
                ensure!(data.len() == 4, "invalid RQ1 size");
                let size = Address::from_bytes(data.try_into()?)?.linear();
                ensure!(size > 0, "empty RQ1");
                address.plus(size as usize - 1)?;
                Ok(Self::Request {
                    device: bytes[2],
                    address,
                    size,
                })
            }
            _ => bail!("unsupported Roland command"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub device: u8,
    pub model_code: u8,
    pub name: &'static str,
}
impl Identity {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        // Same prefix and model-code positions checked by midi_connect_controller.js.
        ensure!(
            bytes.len() >= 15
                && bytes[0] == 0xf0
                && bytes[1] == 0x7e
                && bytes.last() == Some(&0xf7),
            "not identity reply"
        );
        ensure!(
            bytes[3..8] == [6, 2, 0x41, 7, 5],
            "identity is not Katana Gen 3"
        );
        ensure!(
            bytes[2..bytes.len() - 1].iter().all(|b| *b < 128),
            "invalid identity data"
        );
        let name = match bytes[10] {
            5 => "KATANA-50",
            6 => "KATANA-100",
            7 => "KATANA-100/212",
            8 => "KATANA-HEAD",
            9 => "KATANA Artist",
            10 => "KATANA-50 EX",
            11 => "KATANA Artist HEAD",
            _ => bail!("unsupported Katana model code"),
        };
        Ok(Self {
            device: bytes[2],
            model_code: bytes[10],
            name,
        })
    }
    pub fn slot_exists(&self, slot: usize) -> bool {
        slot < 9 && (self.model_code != 5 || [0, 1, 2, 5, 6].contains(&slot))
    }
}

/// MIDI stream reassembly with real-time interleaving and a bounded buffer.
#[derive(Default)]
pub struct Decoder {
    buffer: Vec<u8>,
}
impl Decoder {
    pub fn feed(&mut self, input: &[u8]) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        for b in input {
            match *b {
                0xf8..=0xff => {}
                0xf0 => {
                    self.buffer.clear();
                    self.buffer.push(*b);
                }
                0xf7 if !self.buffer.is_empty() => {
                    self.buffer.push(*b);
                    frames.push(std::mem::take(&mut self.buffer));
                }
                0x80..=0xf7 => self.buffer.clear(),
                _ if !self.buffer.is_empty() => {
                    if self.buffer.len() < 4096 {
                        self.buffer.push(*b);
                    } else {
                        self.buffer.clear();
                    }
                }
                _ => {}
            }
        }
        frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn golden_gain() {
        let msg = Message::Data {
            device: 0x10,
            address: Address::packed(0x20000600).unwrap(),
            data: vec![50],
        };
        let expected = vec![
            0xf0, 0x41, 0x10, 1, 5, 7, 0x12, 0x20, 0, 6, 0, 50, 0x28, 0xf7,
        ];
        assert_eq!(msg.encode().unwrap(), expected);
        assert_eq!(Message::decode(&expected).unwrap(), msg);
    }
    #[test]
    fn golden_request() {
        let msg = Message::Request {
            device: 16,
            address: Address::packed(0x20000000).unwrap(),
            size: 16,
        };
        assert_eq!(
            msg.encode().unwrap(),
            vec![
                0xf0, 0x41, 16, 1, 5, 7, 0x11, 0x20, 0, 0, 0, 0, 0, 0, 16, 0x50, 0xf7
            ]
        );
    }
    #[test]
    fn carry_is_base128() {
        assert_eq!(
            Address::packed(0x2000007f)
                .unwrap()
                .plus(1)
                .unwrap()
                .to_string(),
            "20000100"
        );
        assert!(Address::packed(0x20000080).is_err());
    }
    #[test]
    fn corrupt_and_truncated() {
        let m = Message::Data {
            device: 16,
            address: Address::new(0).unwrap(),
            data: vec![1],
        }
        .encode()
        .unwrap();
        for n in 0..m.len() {
            assert!(Message::decode(&m[..n]).is_err());
        }
        let mut b = m;
        b[11] ^= 1;
        assert!(Message::decode(&b).is_err());
    }
    #[test]
    fn split_stream() {
        let mut d = Decoder::default();
        assert!(d.feed(&[0xf0, 0x41, 0xf8, 1]).is_empty());
        assert_eq!(
            d.feed(&[0xfe, 2, 0xf7, 0xf0, 3, 0xf7]),
            vec![vec![0xf0, 0x41, 1, 2, 0xf7], vec![0xf0, 3, 0xf7]]
        );
    }
}
