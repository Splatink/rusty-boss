//! Gen 3 TSL/ALB interchange. Unknown JSON fields and raw bytes survive round trips.
use crate::schema::Schema;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
};
pub const DEVICE: &str = "KATANA Gen3";
pub type ParamSet = BTreeMap<String, Vec<u8>>;

pub mod hex_set {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        v: &ParamSet,
        s: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let map: BTreeMap<_, Vec<String>> = v
            .iter()
            .map(|(k, v)| (k, v.iter().map(|b| format!("{b:02X}")).collect()))
            .collect();
        map.serialize(s)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> std::result::Result<ParamSet, D::Error> {
        let raw = BTreeMap::<String, Vec<String>>::deserialize(d)?;
        raw.into_iter()
            .map(|(key, values)| {
                if values.len() > 32768 {
                    return Err(serde::de::Error::custom("oversized parameter block"));
                }
                let bytes = values
                    .iter()
                    .map(|v| {
                        if v.is_empty() || v.len() > 2 {
                            return Err(serde::de::Error::custom("invalid hex byte"));
                        }
                        let n = u8::from_str_radix(v, 16).map_err(serde::de::Error::custom)?;
                        if n >= 128 {
                            return Err(serde::de::Error::custom("non-seven-bit parameter byte"));
                        }
                        Ok(n)
                    })
                    .collect::<std::result::Result<Vec<_>, D::Error>>()?;
                Ok((key, bytes))
            })
            .collect()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Patch {
    #[serde(default)]
    pub memo: Value,
    #[serde(rename = "paramSet", with = "hex_set")]
    pub parameters: ParamSet,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl Patch {
    pub fn new(schema: &Schema) -> Result<Self> {
        Ok(Self {
            memo: serde_json::json!({"memo":"", "isToneCentralPatch":false}),
            parameters: schema.defaults("PATCH")?,
            extra: BTreeMap::new(),
        })
    }
    pub fn name(&self) -> String {
        self.parameters
            .get("PATCH%COM")
            .map(|d| {
                String::from_utf8_lossy(&d[..d.len().min(16)])
                    .trim_end()
                    .to_owned()
            })
            .unwrap_or_else(|| "Unnamed".into())
    }
    pub fn rename(&mut self, schema: &Schema, name: &str) -> Result<()> {
        let p = &schema.block("PATCH%COM")?.parameters[0];
        self.parameters
            .insert("PATCH%COM".into(), p.encode_text(name)?);
        Ok(())
    }
    /// Match the vendor loader's tail-fill behavior, but report it to the caller.
    pub fn complete(&mut self, schema: &Schema) -> Result<Vec<String>> {
        let mut warnings = Vec::new();
        for b in schema.blocks.iter().filter(|b| b.id.starts_with("PATCH%")) {
            let d = self.parameters.entry(b.id.clone()).or_default();
            if d.len() < b.size {
                warnings.push(format!(
                    "{} missing {} bytes; filled with defaults",
                    b.id,
                    b.size - d.len()
                ));
                d.extend_from_slice(&b.defaults()?[d.len()..]);
            }
            ensure!(
                d.len() <= 32768 && d.iter().all(|n| *n < 128),
                "invalid block {}",
                b.id
            );
        }
        Ok(warnings)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiveSet {
    pub name: String,
    #[serde(rename = "formatRev")]
    pub revision: String,
    pub device: String,
    pub data: Vec<Vec<Patch>>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl LiveSet {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            revision: "0000".into(),
            device: DEVICE.into(),
            data: vec![vec![]],
            extra: BTreeMap::new(),
        }
    }
    pub fn load(path: &Path, schema: &Schema) -> Result<(Self, Vec<String>)> {
        let mut s: Self = read_json(path)?;
        let warnings = s.validate(schema)?;
        Ok((s, warnings))
    }
    pub fn validate(&mut self, schema: &Schema) -> Result<Vec<String>> {
        let s = self;
        ensure!(
            s.device == DEVICE,
            "unsupported device {}; MkI/MkII conversion is not implemented",
            s.device
        );
        ensure!(
            s.revision == "0000",
            "unsupported TSL revision {}",
            s.revision
        );
        ensure!(
            s.data.len() == 1 && s.data[0].len() <= 1000,
            "expected one librarian mode and <=1000 patches"
        );
        let mut warnings = Vec::new();
        for (i, p) in s.data[0].iter_mut().enumerate() {
            warnings.extend(
                p.complete(schema)?
                    .into_iter()
                    .map(|w| format!("Patch {}: {w}", i + 1)),
            );
        }
        Ok(warnings)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        atomic_json(path, self)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Backup {
    #[serde(rename = "formatRev")]
    pub revision: String,
    pub device: String,
    pub model: String,
    #[serde(with = "hex_set")]
    pub system: ParamSet,
    #[serde(default, with = "hex_set")]
    pub setup: ParamSet,
    #[serde(rename = "userPatch")]
    pub patches: Vec<Option<BackupPatch>>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BackupPatch(#[serde(with = "hex_set")] pub ParamSet);
impl Backup {
    pub fn load(path: &Path) -> Result<Self> {
        let b: Self = read_json(path)?;
        ensure!(
            b.device == DEVICE && b.revision == "0000",
            "unsupported backup device or revision"
        );
        ensure!(b.patches.len() <= 10, "too many backup rows");
        Ok(b)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        atomic_json(path, self)
    }
}
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let mut text = String::new();
    fs::File::open(path)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_string(&mut text)?;
    ensure!(text.len() <= 16 * 1024 * 1024, "file exceeds 16 MiB");
    serde_json::from_str(text.trim_start_matches('\u{feff}')).context("invalid JSON document")
}
/// Same-directory rename: failures never truncate the previous file. No fixed tmp
/// filename, symlink-following overwrite, or cross-filesystem rename.
pub fn atomic_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    ensure!(
        bytes.len() <= 16 * 1024 * 1024,
        "Document exceeds 16 MiB; split large collections before exporting"
    );
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    ensure!(path.file_name().is_some(), "destination must name a file");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let tmp = parent.join(format!(".rusty-boss-{}-{nonce}.tmp", std::process::id()));
    let result = (|| -> Result<()> {
        let mut f = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)?;
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tsl_roundtrip() {
        let schema = Schema::load();
        let mut set = LiveSet::new("Test");
        let mut p = Patch::new(&schema).unwrap();
        p.rename(&schema, "Rusty Boss").unwrap();
        p.extra.insert("future".into(), serde_json::json!(true));
        set.data[0].push(p);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.tsl");
        set.save(&path).unwrap();
        let (loaded, w) = LiveSet::load(&path, &schema).unwrap();
        assert!(w.is_empty());
        assert_eq!(loaded.data[0][0].name(), "Rusty Boss");
        assert_eq!(
            serde_json::to_value(&set).unwrap(),
            serde_json::to_value(&loaded).unwrap()
        );
        let raw = fs::read_to_string(path).unwrap();
        assert!(raw.contains("\"32\""));
    }
    #[test]
    fn reject_bad_bytes() {
        assert!(serde_json::from_str::<Patch>(r#"{"paramSet":{"PATCH%COM":["80"]}}"#).is_err());
    }
    #[test]
    fn short_patch_fills_and_reports() {
        let s = Schema::load();
        let mut p = Patch::new(&s).unwrap();
        p.parameters.get_mut("PATCH%AMP").unwrap().truncate(1);
        assert_eq!(p.complete(&s).unwrap().len(), 1);
        assert_eq!(p.parameters["PATCH%AMP"].len(), 10);
    }
    #[test]
    fn oversized_exports_cannot_replace_a_readable_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tones.tsl");
        atomic_json(&path, &"keep this file").unwrap();
        let original = fs::read(&path).unwrap();
        assert!(atomic_json(&path, &"x".repeat(16 * 1024 * 1024)).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(read_json::<String>(&path).unwrap(), "keep this file");
    }
    #[test]
    fn backup_roundtrip() {
        let s = Schema::load();
        let b = Backup {
            revision: "0000".into(),
            device: DEVICE.into(),
            model: "KATANA-100".into(),
            system: s.defaults("SYSTEM").unwrap(),
            setup: s.defaults("SETUP").unwrap(),
            patches: vec![Some(BackupPatch(s.defaults("PATCH").unwrap())), None],
            extra: BTreeMap::new(),
        };
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("backup.alb");
        b.save(&p).unwrap();
        let c = Backup::load(&p).unwrap();
        assert_eq!(c.patches.len(), 2);
        assert!(c.patches[1].is_none());
        assert_eq!(
            serde_json::to_value(b).unwrap(),
            serde_json::to_value(c).unwrap()
        );
    }
}
