//! Local collections are independent of exported TSLs and hardware permissions.
use crate::{
    document::{atomic_json, read_json, LiveSet, Patch},
    schema::Schema,
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Library {
    pub sets: Vec<LiveSet>,
    pub active: usize,
    #[serde(default)]
    revision: u64,
}
impl Deref for Library {
    type Target = LiveSet;
    fn deref(&self) -> &LiveSet {
        &self.sets[self.active]
    }
}
impl DerefMut for Library {
    fn deref_mut(&mut self) -> &mut LiveSet {
        &mut self.sets[self.active]
    }
}
impl Library {
    pub fn new(schema: &Schema) -> Self {
        let mut set = LiveSet::new("My tones");
        set.data[0].push(Patch::new(schema).expect("embedded defaults"));
        Self {
            sets: vec![set],
            active: 0,
            revision: 0,
        }
    }
    pub fn path() -> Option<PathBuf> {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .map(|p| p.join("rusty-boss/library.json"))
    }
    pub fn load(path: &Path, schema: &Schema) -> Result<Self> {
        let mut library: Self = read_json(path)?;
        ensure!(
            !library.sets.is_empty()
                && library.sets.len() <= 100
                && library.active < library.sets.len(),
            "invalid local library"
        );
        for set in &mut library.sets {
            set.validate(schema)?;
        }
        Ok(library)
    }
    pub fn save(&mut self, path: &Path) -> Result<()> {
        ensure!(
            !self.sets.is_empty()
                && self.sets.len() <= 100
                && self.active < self.sets.len()
                && self
                    .sets
                    .iter()
                    .all(|s| s.data.len() == 1 && s.data[0].len() <= 1000),
            "Invalid collection sizes"
        );
        let revision = self
            .revision
            .checked_add(1)
            .context("Library revision limit reached; export your work")?;
        // shortcut: one bounded 16 MiB document, split by collection when large libraries need it.
        // Reserve one byte for a revision digit rollover; atomic_json writes pretty JSON.
        ensure!(
            serde_json::to_vec_pretty(self)?.len() < 16 * 1024 * 1024,
            "Local library exceeds 16 MiB; export/remove unused sets before saving"
        );
        std::fs::create_dir_all(
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )?;
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.with_extension("lock"))?;
        lock.try_lock()
            .context("Another app is saving the library; try again shortly")?;
        if path.exists() {
            let old: Self = read_json(path)
                .context("Existing library is unreadable; refusing to overwrite it")?;
            ensure!(
                old.revision == self.revision,
                "Library changed in another app instance; export your work before restarting"
            );
            atomic_json(&path.with_extension("previous.json"), &old)?;
        } else {
            ensure!(
                self.revision == 0,
                "Library file disappeared; export your work before restarting"
            );
        }
        self.revision = revision;
        if let Err(e) = atomic_json(path, self) {
            self.revision -= 1;
            return Err(e);
        }
        Ok(())
    }
    pub fn add(&mut self, set: LiveSet) -> Result<()> {
        ensure!(self.sets.len() < 100, "Library holds at most 100 sets");
        self.sets.push(set);
        self.active = self.sets.len() - 1;
        Ok(())
    }
    pub fn transfer(
        &mut self,
        indices: &BTreeSet<usize>,
        destination: usize,
        moving: bool,
    ) -> Result<()> {
        ensure!(
            destination < self.sets.len() && destination != self.active,
            "Choose another set"
        );
        ensure!(
            !indices.is_empty() && indices.iter().all(|i| *i < self.data[0].len()),
            "Select valid tones"
        );
        ensure!(
            self.sets[destination].data[0].len() + indices.len() <= 1000,
            "Destination set would exceed 1000 tones"
        );
        let patches: Vec<_> = indices.iter().map(|i| self.data[0][*i].clone()).collect();
        self.sets[destination].data[0].extend(patches);
        if moving {
            for i in indices.iter().rev() {
                self.data[0].remove(*i);
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collections_restart_roundtrip_and_previous_copy() {
        let s = Schema::load();
        let mut library = Library::new(&s);
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("library.json");
        library.save(&path).unwrap();
        library.add(LiveSet::new("Second")).unwrap();
        library.save(&path).unwrap();
        let loaded = Library::load(&path, &s).unwrap();
        assert_eq!(loaded.sets.len(), 2);
        assert_eq!(loaded.active, 1);
        assert_eq!(
            Library::load(&path.with_extension("previous.json"), &s)
                .unwrap()
                .sets
                .len(),
            1
        );
    }
    #[test]
    fn corruption_and_other_instances_cannot_be_overwritten() {
        let s = Schema::load();
        let mut a = Library::new(&s);
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("library.json");
        a.save(&path).unwrap();
        let mut b = Library::load(&path, &s).unwrap();
        a.name = "Updated".into();
        a.save(&path).unwrap();
        assert!(b.save(&path).is_err());
        assert_eq!(Library::load(&path, &s).unwrap().name, "Updated");
        std::fs::write(&path, b"broken").unwrap();
        assert!(a.save(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"broken");
    }
    #[test]
    fn simultaneous_saves_and_invalid_documents_cannot_replace_a_good_library() {
        let schema = Schema::load();
        let mut library = Library::new(&schema);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        library.save(&path).unwrap();
        let original = std::fs::read(&path).unwrap();
        let lock = std::fs::File::open(path.with_extension("lock")).unwrap();
        lock.lock().unwrap();
        assert!(library.save(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        drop(lock);
        library.save(&path).unwrap();
        let original = std::fs::read(&path).unwrap();
        library.active = 3;
        assert!(library.save(&path).is_err());
        library.active = 0;
        library.revision = u64::MAX;
        assert!(library.save(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let mut library = Library::load(&path, &schema).unwrap();
        library.sets[0].extra.insert(
            "large-future-field".into(),
            serde_json::json!(vec!["x"; 1_500_000]),
        );
        assert!(serde_json::to_vec(&library).unwrap().len() < 16 * 1024 * 1024);
        assert!(library.save(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(Library::load(&path, &schema).is_ok());
    }
    #[test]
    fn transfer_preserves_order_metadata_and_validates_before_mutating() {
        let s = Schema::load();
        let mut l = Library::new(&s);
        for name in ["Two", "Three"] {
            let mut p = Patch::new(&s).unwrap();
            p.rename(&s, name).unwrap();
            l.data[0].push(p);
        }
        l.add(LiveSet::new("Destination")).unwrap();
        l.active = 0;
        let before = serde_json::to_value(&l).unwrap();
        assert!(l.transfer(&BTreeSet::from([0, 9]), 1, true).is_err());
        assert_eq!(serde_json::to_value(&l).unwrap(), before);
        l.transfer(&BTreeSet::from([0, 2]), 1, false).unwrap();
        assert_eq!(l.data[0].len(), 3);
        l.transfer(&BTreeSet::from([0, 2]), 1, true).unwrap();
        assert_eq!(l.data[0][0].name(), "Two");
        assert_eq!(l.sets[1].data[0].len(), 4);
        assert_eq!(l.sets[1].data[0][1].name(), "Three");
    }
}
