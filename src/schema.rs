use crate::protocol::{Address, MAX_DATA};
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Choice {
    pub value: i32,
    pub label: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Parameter {
    pub name: String,
    pub offset: usize,
    pub width: usize,
    pub encoding: String,
    pub bias: i32,
    pub initial: Value,
    pub min: i32,
    pub max: i32,
    pub choices: Vec<Choice>,
}
impl Parameter {
    pub fn label(&self) -> String {
        self.name
            .strip_prefix("PRMID_")
            .unwrap_or(&self.name)
            .replace('_', " ")
    }
    pub fn decode_number(&self, data: &[u8]) -> Result<i32> {
        let bytes = data
            .get(self.offset..self.offset + self.width)
            .context("short parameter block")?;
        let bits = match self.encoding.as_str() {
            "u7" | "u14" => 7,
            "nibbles" => 4,
            _ => bail!("not numeric"),
        };
        let mask = (1 << bits) - 1;
        ensure!(
            bytes.iter().all(|b| *b <= mask),
            "invalid parameter encoding"
        );
        Ok(bytes.iter().fold(0i32, |v, b| (v << bits) | *b as i32) - self.bias)
    }
    pub fn encode_number(&self, value: i32) -> Result<Vec<u8>> {
        ensure!(
            (self.min..=self.max).contains(&value),
            "{} must be {}..{}",
            self.name,
            self.min,
            self.max
        );
        let bits = match self.encoding.as_str() {
            "u7" | "u14" => 7,
            "nibbles" => 4,
            _ => bail!("not numeric"),
        };
        let encoded = value.checked_add(self.bias).context("value overflow")?;
        ensure!(
            encoded >= 0 && encoded < (1 << (bits * self.width)),
            "value cannot be encoded"
        );
        Ok((0..self.width)
            .rev()
            .map(|i| ((encoded >> (i * bits)) & ((1 << bits) - 1)) as u8)
            .collect())
    }
    pub fn encode_text(&self, value: &str) -> Result<Vec<u8>> {
        ensure!(self.encoding == "ascii", "not text");
        ensure!(
            value.len() <= self.width
                && value
                    .bytes()
                    .all(|b| (self.min..=self.max).contains(&(b as i32))),
            "text must be <= {} printable ASCII characters",
            self.width
        );
        let mut data = vec![32; self.width];
        data[..value.len()].copy_from_slice(value.as_bytes());
        Ok(data)
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Block {
    pub id: String,
    pub address: u32,
    pub size: usize,
    pub readonly: bool,
    pub parameters: Vec<Parameter>,
}
impl Block {
    pub fn address(&self) -> Address {
        Address::new(self.address).expect("validated embedded schema")
    }
    pub fn defaults(&self) -> Result<Vec<u8>> {
        let mut data = vec![0; self.size];
        for p in &self.parameters {
            let bytes = if p.encoding == "ascii" {
                p.encode_text(p.initial.as_str().context("text default")?)?
            } else {
                p.encode_number(p.initial.as_i64().context("numeric default")? as i32)?
            };
            data[p.offset..p.offset + p.width].copy_from_slice(&bytes);
        }
        Ok(data)
    }
    /// Do not split multi-byte values across request boundaries.
    pub fn chunks(&self) -> Vec<(usize, usize)> {
        let mut chunks = Vec::new();
        let mut start = 0;
        let mut end = 0;
        for p in &self.parameters {
            let next = p.offset + p.width;
            if next - start > MAX_DATA {
                if end > start {
                    chunks.push((start, end - start));
                }
                start = p.offset;
            }
            end = next;
        }
        if end > start {
            chunks.push((start, end - start));
        }
        chunks
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Schema {
    pub version: String,
    pub model_id: Vec<u8>,
    pub blocks: Vec<Block>,
    pub sources: BTreeMap<String, String>,
}
impl Schema {
    pub fn load() -> Self {
        let s: Self = serde_json::from_str(include_str!("../data/katana-gen3.json"))
            .expect("embedded schema JSON");
        s.validate().expect("embedded schema invariants");
        s
    }
    pub fn validate(&self) -> Result<()> {
        for b in &self.blocks {
            Address::new(b.address)?.plus(b.size.saturating_sub(1))?;
            let mut last = 0;
            for p in &b.parameters {
                ensure!(
                    p.offset >= last && p.offset + p.width <= b.size,
                    "overlapping parameter {}",
                    p.name
                );
                last = p.offset + p.width;
            }
            b.defaults()?;
        }
        Ok(())
    }
    pub fn block(&self, id: &str) -> Result<&Block> {
        self.blocks
            .iter()
            .find(|b| b.id == id)
            .with_context(|| format!("unknown block {id}"))
    }
    pub fn defaults(&self, root: &str) -> Result<BTreeMap<String, Vec<u8>>> {
        self.blocks
            .iter()
            .filter(|b| b.id.starts_with(&format!("{root}%")))
            .map(|b| Ok((b.id.clone(), b.defaults()?)))
            .collect()
    }
}

/// Slot 0 is PANEL; slots 1..4 bank A; slots 5..8 bank B. Row 9 is a
/// vendor library staging row, not a user-selectable amplifier channel.
pub fn slot_address(slot: usize) -> Result<Address> {
    ensure!(
        slot < 9,
        "slot must be PANEL (0), A1..A4 (1..4) or B1..B4 (5..8)"
    );
    let packed = [
        0x20100000, 0x20200000, 0x20300000, 0x20400000, 0x20500000, 0x20600000, 0x20700000,
        0x21000000, 0x21100000,
    ][slot];
    Address::packed(packed)
}
pub fn slot_label(slot: usize) -> String {
    match slot {
        0 => "PANEL".into(),
        1..=4 => format!("A{slot}"),
        5..=8 => format!("B{}", slot - 4),
        _ => "Staging".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_schema() {
        let s = Schema::load();
        assert_eq!(s.blocks.len(), 106);
        assert_eq!(
            s.blocks.iter().map(|b| b.parameters.len()).sum::<usize>(),
            2463
        );
        let amp = s.block("PATCH%AMP").unwrap();
        assert_eq!(amp.address().to_string(), "20000600");
        for b in &s.blocks {
            for (_, n) in b.chunks() {
                assert!(n <= 128);
            }
        }
    }
    #[test]
    fn signed_and_nibble() {
        let s = Schema::load();
        let b = s.block("PATCH%BOOSTER(1)").unwrap();
        let p = &b.parameters[2];
        let mut d = b.defaults().unwrap();
        d[p.offset..p.offset + p.width].copy_from_slice(&p.encode_number(-50).unwrap());
        assert_eq!(p.decode_number(&d).unwrap(), -50);
        assert!(p.encode_number(51).is_err());
        let p = s
            .block("PATCH%DELAY(1)")
            .unwrap()
            .parameters
            .iter()
            .find(|p| p.encoding == "nibbles")
            .unwrap();
        let n = p.min.max(1000).min(p.max);
        let bytes = p.encode_number(n).unwrap();
        assert!(bytes.iter().all(|b| *b < 16));
    }
}
