#[cfg(feature = "midi")]
use crate::protocol::Message;
use crate::{
    document::{Backup, LiveSet, ParamSet, Patch},
    editor::{Catalog, Edit, Section},
    library::Library,
    protocol::Identity,
    safety::{Access, WriteGate},
    schema::{Schema, slot_label},
};
use eframe::egui::{self, Color32, RichText};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::PathBuf,
};
mod classic;
mod collections;
mod icons;
mod settings;
use settings::SystemDraft;
#[derive(Clone)]
enum FileAction {
    Import,
    Export,
    BackupOpen,
    BackupSave,
    Icon(String),
}
#[cfg(test)]
mod tests;
mod theme;
mod wordmark;
#[cfg(feature = "midi")]
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::Duration,
};
use theme::Theme;

#[cfg_attr(not(feature = "midi"), allow(dead_code))]
#[derive(Clone)]
struct PreviewRequest {
    generation: u64,
    patch: Patch,
}
#[derive(Clone)]
struct SaveRequest {
    slot: usize,
    patch: Patch,
}
#[derive(Clone)]
struct PatchDrag(Patch);

#[cfg(feature = "midi")]
fn overlay_pending_edits(schema: &Schema, values: &mut ParamSet, edits: &VecDeque<Edit>) {
    for edit in edits {
        if let (Ok(block), Some(data)) = (schema.block(&edit.block), values.get_mut(&edit.block))
            && let Some(parameter) = block.parameters.get(edit.index)
            && data.len() >= parameter.offset + parameter.width
            && edit.bytes.len() == parameter.width
        {
            data[parameter.offset..parameter.offset + parameter.width].copy_from_slice(&edit.bytes);
        }
    }
}

#[cfg(feature = "midi")]
enum Job {
    Connect(String),
    Disconnect,
    ReadCurrent,
    ReadSlot(usize),
    ChannelNames,
    Select(usize),
    Preview(PreviewRequest),
    Save(SaveRequest),
    Backup,
    ReadSystem,
    ApplySystem(Box<SystemDraft>),
    Macro(u64, usize, i32),
    Tap(usize),
    ReadSlots(Vec<usize>),
    SaveMany(Vec<(usize, Patch)>),
    Restore(Box<Backup>),
    Set(u64, String, usize, Vec<u8>),
    Shutdown,
}
#[cfg(feature = "midi")]
enum Event {
    Connected(Identity),
    Disconnected,
    Current(Patch),
    Selected(usize, Patch),
    Slot(usize, Patch),
    ChannelNames([Option<String>; 9]),
    Previewed(u64, ParamSet),
    Saved(SaveRequest),
    Backup(Box<Backup>),
    System(ParamSet),
    Refreshed(u64, ParamSet),
    Slots(Vec<(usize, Patch)>),
    SavedMany(Vec<(usize, Patch)>),
    Restored,
    Progress(usize, usize),
    Done(String),
    Update(Message),
    Error(String),
}
#[cfg(feature = "midi")]
struct Worker {
    tx: Sender<Job>,
    rx: Receiver<Event>,
    cancel: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}
#[cfg(feature = "midi")]
impl Worker {
    fn new(schema: Schema, gate: WriteGate) -> Self {
        let (tx, jobs) = mpsc::channel();
        let (events, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let handle = thread::spawn(move || {
            use crate::{midi::AlsaTransport, safety::Guarded, session::Session};
            let mut session: Option<Session<Guarded<AlsaTransport>>> = None;
            loop {
                let job = match jobs.recv_timeout(Duration::from_millis(20)) {
                    Ok(j) => j,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if let Some(s) = session.as_mut() {
                            match s.poll() {
                                Ok(Some(m)) => {
                                    if events.send(Event::Update(m)).is_err() {
                                        break;
                                    }
                                }
                                Ok(None) => {}
                                Err(e) => {
                                    stop.store(false, Ordering::Relaxed);
                                    session.take();
                                    let _ = events.send(Event::Error(format!("{e:#}")));
                                }
                            }
                        }
                        continue;
                    }
                };
                if matches!(job, Job::Shutdown) {
                    break;
                }
                let result = (|| -> anyhow::Result<Event> {
                    match job {
                        Job::Connect(port) => {
                            if let Some(mut s) = session.take() {
                                s.close()?;
                            }
                            let s = Session::connect(Guarded::new(
                                AlsaTransport::open(&port, stop.clone())?,
                                gate.clone(),
                            ))?;
                            let id = s.identity.clone();
                            session = Some(s);
                            Ok(Event::Connected(id))
                        }
                        Job::Disconnect => {
                            stop.store(false, Ordering::Relaxed);
                            if let Some(mut s) = session.take() {
                                s.close()?;
                            }
                            Ok(Event::Disconnected)
                        }
                        other => {
                            let s = session
                                .as_mut()
                                .ok_or_else(|| anyhow::anyhow!("not connected"))?;
                            Ok(match other {
                                Job::ReadCurrent => Event::Current(Patch {
                                    parameters: s.read_root(&schema, "PATCH")?,
                                    ..Patch::new(&schema)?
                                }),
                                Job::ReadSlot(n) => Event::Slot(n, s.read_slot(&schema, n)?),
                                Job::ChannelNames => {
                                    Event::ChannelNames(s.read_channel_names(&schema)?)
                                }
                                Job::Select(n) => {
                                    s.select_slot(n)?;
                                    Event::Selected(
                                        n,
                                        Patch {
                                            parameters: s.read_root(&schema, "PATCH")?,
                                            ..Patch::new(&schema)?
                                        },
                                    )
                                }
                                Job::Preview(r) => {
                                    s.preview(&schema, &r.patch)?;
                                    Event::Previewed(r.generation, s.read_root(&schema, "PATCH")?)
                                }
                                Job::Save(r) => {
                                    // Preflight before even changing the temporary sound. Per-packet
                                    // checks remain effective if the user revokes permission mid-job.
                                    anyhow::ensure!(
                                        gate.access() == Access::Persistent,
                                        "Channel writes are locked"
                                    );
                                    s.store_patch(&schema, &r.patch, r.slot)?;
                                    Event::Saved(r)
                                }
                                Job::Backup => Event::Backup(Box::new(s.read_backup(&schema)?)),
                                Job::ReadSystem => Event::System(s.read_root(&schema, "SYSTEM")?),
                                Job::ApplySystem(draft) => {
                                    anyhow::ensure!(
                                        gate.access() == Access::Persistent,
                                        "Amp writes are locked"
                                    );
                                    Event::System(s.apply_system(
                                        &schema,
                                        &draft.before,
                                        &draft.values,
                                    )?)
                                }
                                Job::Macro(generation, index, value) => {
                                    s.panel_macro(&schema, index, value)?;
                                    Event::Refreshed(generation, s.read_root(&schema, "PATCH")?)
                                }
                                Job::Tap(index) => {
                                    s.tap(index)?;
                                    Event::Done("Tap sent · Saved channels unchanged".into())
                                }
                                Job::ReadSlots(slots) => {
                                    anyhow::ensure!(
                                        !slots.is_empty()
                                            && slots.len() <= 9
                                            && slots
                                                .iter()
                                                .all(|slot| s.identity.slot_exists(*slot)),
                                        "Invalid channel selection"
                                    );
                                    let mut patches = Vec::new();
                                    for (i, slot) in slots.iter().enumerate() {
                                        patches.push((*slot, s.read_slot(&schema, *slot)?));
                                        let _ = events.send(Event::Progress(i + 1, slots.len()));
                                    }
                                    Event::Slots(patches)
                                }
                                Job::SaveMany(patches) => {
                                    anyhow::ensure!(
                                        gate.access() == Access::Persistent,
                                        "Amp writes are locked"
                                    );
                                    s.store_patches(&schema, &patches, |i, n| {
                                        let _ = events.send(Event::Progress(i, n));
                                    })?;
                                    Event::SavedMany(patches)
                                }
                                Job::Restore(backup) => {
                                    anyhow::ensure!(
                                        gate.access() == Access::Persistent,
                                        "Amp writes are locked"
                                    );
                                    s.restore_backup(&schema, &backup, |i, n| {
                                        let _ = events.send(Event::Progress(i, n));
                                    })?;
                                    Event::Restored
                                }
                                Job::Set(generation, id, i, b) => {
                                    s.set_parameter(&schema, &id, i, b)?;
                                    let coordinated = id == "PATCH%COLOR"
                                        || id.starts_with("PATCH%ASSIGN")
                                        || (i == 0
                                            && ["PATCH%FX(", "PATCH%BOOSTER(", "PATCH%DELAY("]
                                                .iter()
                                                .any(|prefix| id.starts_with(prefix)))
                                        || (i <= 1 && id.starts_with("PATCH%REVERB("));
                                    if coordinated {
                                        Event::Refreshed(generation, s.read_root(&schema, "PATCH")?)
                                    } else {
                                        Event::Done(
                                            "Sound updated · saved channels unchanged".into(),
                                        )
                                    }
                                }
                                _ => unreachable!(),
                            })
                        }
                    }
                })();
                if let Some(s) = session.as_mut() {
                    for m in s.take_updates() {
                        let _ = events.send(Event::Update(m));
                    }
                }
                let event = match result {
                    Ok(e) => e,
                    Err(e) => {
                        stop.store(false, Ordering::Relaxed);
                        session.take();
                        Event::Error(format!("{e:#}"))
                    }
                };
                if events.send(event).is_err() {
                    break;
                }
            }
            stop.store(false, Ordering::Relaxed);
            session.take();
        });
        Self {
            tx,
            rx,
            cancel,
            handle: Some(handle),
        }
    }
}
#[cfg(feature = "midi")]
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        let _ = self.tx.send(Job::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
enum Page {
    Editor,
    Library,
    Settings,
}
struct App {
    schema: Schema,
    catalog: Catalog,
    section: Section,
    offline_model: u8,
    theme: Theme,
    file_dialog: Option<(FileAction, std::sync::mpsc::Receiver<Option<PathBuf>>)>,
    icons: BTreeMap<String, Option<egui::TextureHandle>>,
    write_gate: WriteGate,
    write_busy: bool,
    pending_edits: VecDeque<Edit>,
    generation: u64,
    previewing: bool,
    pending_preview: Option<PreviewRequest>,
    pending_save: Option<SaveRequest>,
    channel_names: [Option<String>; 9],
    channel_patch: Option<Patch>,
    library: Library,
    library_path: Option<PathBuf>,
    library_error: Option<String>,
    autosave_at: std::time::Instant,
    tone_selection: BTreeSet<usize>,
    amp_selection: BTreeSet<usize>,
    target_set: usize,
    pending_bulk: Vec<(usize, Patch)>,
    system: Option<SystemDraft>,
    system_page: usize,
    assign_controller: usize,
    selected: usize,
    page: Page,
    file: String,
    backup_file: String,
    backup: Option<Backup>,
    status: String,
    logs: VecDeque<String>,
    dirty: bool,
    connected: Option<Identity>,
    busy: bool,
    live: bool,
    synced: bool,
    close_dialog: bool,
    slot: usize,
    confirm: Option<String>,
    #[cfg(feature = "midi")]
    worker: Worker,
    #[cfg(feature = "midi")]
    ports: Vec<crate::midi::Port>,
    #[cfg(feature = "midi")]
    port: String,
    smoke_frames: usize,
}
impl App {
    fn new(ctx: &egui::Context) -> Self {
        wordmark::install(ctx);
        let theme = Theme::preferences_path()
            .map(|p| Theme::load(&p))
            .unwrap_or_default();
        theme.apply(ctx);
        let schema = Schema::load();
        let write_gate = WriteGate::default();
        let (library, library_path, library_error) = Self::load_library(&schema);
        Self {
            #[cfg(feature = "midi")]
            worker: Worker::new(schema.clone(), write_gate.clone()),
            #[cfg(feature = "midi")]
            ports: Vec::new(),
            #[cfg(feature = "midi")]
            port: String::new(),
            schema,
            catalog: Catalog::load(),
            section: Section::Amp,
            offline_model: 5,
            theme,
            file_dialog: None,
            icons: BTreeMap::new(),
            write_gate,
            write_busy: false,
            pending_edits: VecDeque::new(),
            generation: 0,
            previewing: false,
            pending_preview: None,
            pending_save: None,
            channel_names: Default::default(),
            channel_patch: None,
            library,
            library_path,
            library_error,
            autosave_at: std::time::Instant::now(),
            tone_selection: BTreeSet::new(),
            amp_selection: BTreeSet::new(),
            target_set: 0,
            pending_bulk: Vec::new(),
            system: None,
            system_page: 0,
            assign_controller: 0,
            selected: 0,
            page: Page::Editor,
            file: "my-tones.tsl".into(),
            backup_file: "katana-backup.alb".into(),
            backup: None,
            status: "Offline · Import a TSL or connect your amp in Settings".into(),
            logs: VecDeque::new(),
            dirty: false,
            connected: None,
            busy: false,
            live: false,
            synced: false,
            close_dialog: false,
            slot: 1,
            confirm: None,
            smoke_frames: 0,
        }
    }
    fn pick_file(&mut self, saving: bool) {
        self.choose_file(if saving {
            FileAction::Export
        } else {
            FileAction::Import
        });
    }
    fn choose_file(&mut self, action: FileAction) {
        if self.file_dialog.is_some() || self.busy {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let current = PathBuf::from(
            if matches!(action, FileAction::BackupOpen | FileAction::BackupSave) {
                &self.backup_file
            } else {
                &self.file
            },
        );
        let selected_action = action.clone();
        std::thread::spawn(move || {
            let (label, extension) = match &selected_action {
                FileAction::BackupOpen | FileAction::BackupSave => ("Katana backup", "alb"),
                FileAction::Icon(_) => ("Effect icon (PNG)", "png"),
                _ => ("Katana tones", "tsl"),
            };
            let mut dialog = rfd::FileDialog::new().add_filter(label, &[extension]);
            if let Some(parent) = current.parent().filter(|p| p.is_dir()) {
                dialog = dialog.set_directory(parent);
            }
            let path = if matches!(selected_action, FileAction::Export | FileAction::BackupSave) {
                dialog
                    .set_file_name(
                        current
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("my-tones.tsl"),
                    )
                    .save_file()
            } else {
                dialog.pick_file()
            };
            let _ = tx.send(path);
        });
        self.file_dialog = Some((action, rx));
    }
    fn poll_file_dialog(&mut self) {
        if self.busy {
            return;
        }
        let Some((action, rx)) = &self.file_dialog else {
            return;
        };
        match rx.try_recv() {
            Ok(path) => {
                let action = action.clone();
                self.file_dialog = None;
                if let Some(path) = path {
                    match action {
                        FileAction::Import | FileAction::Export => {
                            self.file = path.to_string_lossy().into_owned();
                            if matches!(action, FileAction::Export) {
                                self.confirm_action("save-path");
                            } else {
                                self.import_file();
                            }
                        }
                        FileAction::BackupOpen => self.open_backup(&path),
                        FileAction::BackupSave => {
                            self.backup_file = path.to_string_lossy().into_owned();
                            self.confirm = Some("save-backup".into());
                        }
                        FileAction::Icon(key) => {
                            if let Err(e) = self.import_icon(&key, &path) {
                                self.log(format!("Icon not changed: {e:#}"));
                            }
                        }
                    }
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(_) => {
                self.file_dialog = None;
                self.log("File chooser closed");
            }
        }
    }
    fn import_file(&mut self) {
        self.confirm_action("import-path");
    }
    fn log(&mut self, s: impl Into<String>) {
        let s = s.into();
        self.status = s.clone();
        self.logs.push_front(s);
        self.logs.truncate(100);
    }
    fn disarm(&mut self) {
        self.channel_patch = None;
        self.live = false;
        self.synced = false;
        self.pending_edits.clear();
        self.generation = self.generation.wrapping_add(1);
        self.pending_preview = None;
    }
    fn set_access(&mut self, access: Access) {
        // An acknowledged in-flight preview must not re-arm after protection changes.
        let resume = self.synced && self.connected.is_some() && !self.busy;
        let channel_patch = self.channel_patch.take();
        self.disarm();
        self.channel_patch = channel_patch;
        self.write_gate.set(access);
        self.synced = resume;
        self.live = resume && access != Access::ReadOnly;
        self.pending_save = None;
    }
    #[cfg(feature = "midi")]
    fn submit(&mut self, job: Job) {
        if self.busy {
            return;
        }
        self.write_busy = matches!(job, Job::Set(..) | Job::Tap(..));
        self.previewing = matches!(job, Job::Preview(_));
        if !self.write_busy {
            self.pending_edits.clear();
            self.live = false;
        }
        self.worker.cancel.store(false, Ordering::Relaxed);
        if self.worker.tx.send(job).is_ok() {
            self.busy = true;
            self.status = if self.previewing {
                "Previewing tone…"
            } else {
                "Working…"
            }
            .into();
        } else {
            self.log("MIDI connection unavailable");
        }
    }
    fn choose_preset(&mut self, index: usize) {
        if index >= self.library.data[0].len() || (self.busy && !self.previewing) {
            return;
        }
        self.selected = index;
        self.disarm();
        if self.connected.is_none() {
            self.log("Offline · Editing local tone");
            return;
        }
        if self.write_gate.access() == Access::ReadOnly {
            self.log(
                "Read-only · Tone selected locally. Choose Preview & edit to hear it on the amp.",
            );
        } else {
            #[cfg(feature = "midi")]
            {
                let request = PreviewRequest {
                    generation: self.generation,
                    patch: self.library.data[0][index].clone(),
                };
                if self.busy {
                    self.pending_preview = Some(request);
                } else {
                    self.submit(Job::Preview(request));
                }
            }
        }
    }
    fn choose_channel(&mut self, slot: usize) {
        if self.busy
            || !self
                .connected
                .as_ref()
                .is_some_and(|id| id.slot_exists(slot))
        {
            return;
        }
        if self.write_gate.access() == Access::ReadOnly {
            self.log("Read-only · Choose Preview & edit to switch amp channels. Nothing was copied or saved.");
            return;
        }
        self.slot = slot;
        self.disarm();
        #[cfg(feature = "midi")]
        self.submit(Job::Select(slot));
    }
    fn can_save_to(&self, slot: usize) -> bool {
        !self.busy
            && self.confirm.is_none()
            && self.write_gate.access() == Access::Persistent
            && self
                .connected
                .as_ref()
                .is_some_and(|id| id.slot_exists(slot))
    }
    fn request_save(&mut self, patch: Patch, slot: usize) {
        if !self.can_save_to(slot) {
            self.log("Channel writes are locked, the amp is busy, or this channel is unavailable.");
            return;
        }
        self.pending_save = Some(SaveRequest { patch, slot });
        self.confirm = Some("save-channel".into());
    }
    #[cfg(feature = "midi")]
    fn flush_edits(&mut self) {
        if !self.live || self.write_gate.access() == Access::ReadOnly || self.connected.is_none() {
            self.pending_edits.clear();
            return;
        }
        if !self.busy
            && let Some(e) = self.pending_edits.pop_front()
        {
            self.submit(Job::Set(self.generation, e.block, e.index, e.bytes));
        }
    }
    #[cfg(feature = "midi")]
    fn handle_event(&mut self, e: Event) {
        if !matches!(&e, Event::Update(_) | Event::Progress(..)) {
            self.busy = false;
            self.write_busy = false;
            self.previewing = false;
        }
        match e {
            Event::Progress(i, n) => {
                self.status =
                    format!("Working… {i}/{n} · Cancellation can leave partial hardware changes")
            }
            Event::System(values) => {
                self.system = Some(SystemDraft {
                    before: values.clone(),
                    values,
                });
                self.log("Amp settings read · Changes are staged until you apply them");
            }
            Event::Refreshed(generation, mut values) => {
                if generation == self.generation && self.write_gate.access() != Access::ReadOnly {
                    overlay_pending_edits(&self.schema, &mut values, &self.pending_edits);
                    if let Some(patch) = self
                        .channel_patch
                        .as_mut()
                        .or_else(|| self.library.data[0].get_mut(self.selected))
                    {
                        patch.parameters.extend(values);
                    }
                    self.dirty |= self.channel_patch.is_none();
                    self.synced = true;
                    self.live = true;
                    self.log("Sound synchronized with amp · Saved channels unchanged");
                }
            }
            Event::Slots(patches) => {
                for (slot, patch) in patches {
                    self.channel_names[slot] = Some(patch.name());
                    self.library.data[0].push(patch);
                }
                self.dirty = true;
                self.disarm();
                self.log("Selected amp channels copied to this set");
            }
            Event::SavedMany(patches) => {
                for (slot, patch) in patches {
                    self.channel_names[slot] = Some(patch.name());
                }
                self.disarm();
                self.log("Selected tones saved · Click a tone to preview");
            }
            Event::Restored => {
                self.system = None;
                self.disarm();
                self.set_access(Access::ReadOnly);
                self.log("Restore completed · Amp writes locked again");
                self.submit(Job::ChannelNames);
            }
            Event::Connected(id) => {
                self.system = None;
                self.log(format!(
                    "Connected to {} · Choose a tone to preview",
                    id.name
                ));
                self.connected = Some(id);
                self.channel_names = Default::default();
                self.disarm();
                self.submit(Job::ChannelNames);
            }
            Event::Disconnected => {
                self.system = None;
                self.connected = None;
                self.disarm();
                self.pending_save = None;
                self.write_gate.set(Access::ReadOnly);
                self.channel_names = Default::default();
                self.log("Disconnected · Read-only protection restored");
            }
            Event::Selected(slot, p) => {
                self.disarm();
                self.slot = slot;
                self.channel_patch = Some(p);
                self.synced = true;
                self.live = self.write_gate.access() != Access::ReadOnly;
                self.log(format!("{} selected · Nothing copied or saved · Use Copy amp sound to keep a library copy", slot_label(slot)));
            }
            Event::Current(p) => {
                if self.library.data[0].len() >= 1000 {
                    self.disarm();
                    self.log(
                        "Collection full; choose another collection before copying the amp sound",
                    );
                    return;
                }
                // Hardware reads are copies, never destructive replacement of a downloaded tone.
                self.library.data[0].push(p);
                self.selected = self.library.data[0].len() - 1;
                self.dirty = true;
                self.disarm();
                self.synced = true;
                self.live = self.write_gate.access() != Access::ReadOnly;
                self.log("Current amp sound copied to your library");
            }
            Event::ChannelNames(names) => {
                self.channel_names = names;
                self.log("Ready · Click a tone to preview, or enable amp writes and drag it to a channel");
            }
            Event::Slot(slot, p) => {
                if self.library.data[0].len() >= 1000 {
                    self.disarm();
                    self.log("Collection full; choose another collection before copying a channel");
                    return;
                }
                self.channel_names[slot] = Some(p.name());
                self.library.data[0].push(p);
                self.selected = self.library.data[0].len() - 1;
                self.dirty = true;
                self.disarm();
                self.log("Channel copied to your library");
            }
            Event::Previewed(generation, readback) => {
                if let Some(next) = self.pending_preview.take() {
                    if self.write_gate.access() != Access::ReadOnly {
                        self.submit(Job::Preview(next));
                    }
                } else if generation == self.generation
                    && self.write_gate.access() != Access::ReadOnly
                {
                    if let Some(p) = self.library.data[0].get_mut(self.selected) {
                        p.parameters.extend(readback);
                        self.dirty = true;
                    }
                    self.synced = true;
                    self.live = true;
                    self.log("Previewing on amp · Knobs are live · Saved channels unchanged");
                }
            }
            Event::Saved(r) => {
                let name = r.patch.name();
                self.channel_names[r.slot] = Some(name.clone());
                self.disarm();
                self.log(format!(
                    "Saved {name} to {} · Click the channel to hear and edit it",
                    slot_label(r.slot)
                ));
            }
            Event::Backup(b) => {
                for (i, patch) in b.patches.iter().enumerate().take(9) {
                    if let Some(p) = patch {
                        self.channel_names[i] = Some(
                            Patch {
                                parameters: p.0.clone(),
                                ..Patch::new(&self.schema).unwrap()
                            }
                            .name(),
                        );
                    }
                }
                self.backup = Some(*b);
                self.log("Backup read complete · Save the ALB file to keep it");
            }
            Event::Done(s) => self.log(s),
            Event::Error(s) => {
                self.system = None;
                self.connected = None;
                self.disarm();
                self.pending_save = None;
                self.write_gate.set(Access::ReadOnly);
                self.channel_names = Default::default();
                self.log(format!("Connection stopped: {s}. Hardware state may be unknown; do not retry a save blindly."));
            }
            Event::Update(Message::Data { address, data, .. }) => {
                if address.linear() == 0 {
                    self.disarm();
                    self.log("Amp channel changed · Click a tone to preview or copy the amp sound");
                    return;
                }
                let local = self.channel_patch.is_none();
                if self.synced
                    && let Some(patch) = self
                        .channel_patch
                        .as_mut()
                        .or_else(|| self.library.data[0].get_mut(self.selected))
                {
                    let address = address.linear() as usize;
                    for block in self
                        .schema
                        .blocks
                        .iter()
                        .filter(|b| b.id.starts_with("PATCH%"))
                    {
                        let base = block.address as usize;
                        let start = address.max(base);
                        let end = (address + data.len()).min(base + block.size);
                        if start < end
                            && let Some(dst) = patch
                                .parameters
                                .get_mut(&block.id)
                                .filter(|d| d.len() >= end - base)
                        {
                            dst[start - base..end - base]
                                .copy_from_slice(&data[start - address..end - address]);
                            self.dirty |= local;
                        }
                    }
                    overlay_pending_edits(&self.schema, &mut patch.parameters, &self.pending_edits);
                }
            }
            Event::Update(_) => {}
        }
    }
    #[cfg(feature = "midi")]
    fn events(&mut self) {
        while let Ok(e) = self.worker.rx.try_recv() {
            self.handle_event(e);
        }
    }

    fn permissions(&mut self, ui: &mut egui::Ui) {
        let access = self.write_gate.access();
        ui.horizontal_wrapped(|ui| {
            if ui.selectable_label(access == Access::ReadOnly, "Read-only").on_hover_text("Browse and read the amp. No preview, knob or saved-channel writes. Editor connection setup/cleanup still sends mode commands.").clicked() {
                self.set_access(Access::ReadOnly);
            }
            if ui.selectable_label(access == Access::EditBuffer, "Preview & edit").on_hover_text("Click tones to hear them; knobs edit the current sound. Saved channels stay locked.").clicked() {
                self.set_access(Access::EditBuffer);
            }
            if access == Access::Persistent {
                if ui.button("Amp writes enabled · Lock").clicked() { self.set_access(Access::EditBuffer); }
            } else if ui.button("Enable amp writes…").clicked() { self.confirm = Some("unlock".into()); }
        });
    }
    fn header(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(ui.visuals().window_fill)
                    .inner_margin(egui::Margin::symmetric(18, 10)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    wordmark::show(ui, self.theme.accent());
                    ui.label(RichText::new("KATANA / LINUX").size(11.).weak());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.small(
                            self.connected
                                .as_ref()
                                .map(|id| id.name)
                                .unwrap_or("Offline"),
                        );
                        if self.dirty {
                            ui.label(
                                RichText::new("● Saving locally…")
                                    .color(self.theme.accent())
                                    .size(11.),
                            );
                        }
                    });
                });
                ui.horizontal_wrapped(|ui| {
                    for (p, label) in [
                        (Page::Editor, "Editor"),
                        (Page::Library, "Library"),
                        (Page::Settings, "Settings"),
                    ] {
                        if ui.selectable_label(self.page == p, label).clicked() {
                            self.page = p;
                        }
                    }
                    ui.separator();
                    self.permissions(ui);
                });
            });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if self.busy {
                    ui.spinner();
                }
                ui.label(RichText::new(&self.status).size(12.));
                #[cfg(feature = "midi")]
                if self.busy && ui.button("Cancel").clicked() {
                    self.worker.cancel.store(true, Ordering::Relaxed);
                    self.disarm();
                }
            });
        });
    }
    fn tone_list(&mut self, ui: &mut egui::Ui) {
        let mut next = None;
        for (i, p) in self.library.data[0].iter().enumerate() {
            let r = ui.add_enabled(
                !self.busy || self.previewing,
                egui::Button::new(format!("{:02}   {}", i + 1, p.name()))
                    .selected(self.channel_patch.is_none() && i == self.selected)
                    .sense(egui::Sense::click_and_drag())
                    .min_size(egui::vec2(ui.available_width(), 34.)),
            );
            r.dnd_set_drag_payload(PatchDrag(p.clone()));
            if r.clicked() {
                next = Some(i);
            }
            if r.dragged() {
                egui::Tooltip::for_widget(&r).at_pointer().show(|ui| {
                    ui.label(p.name());
                });
            }
        }
        if let Some(i) = next {
            self.choose_preset(i);
        }
        if self.library.data[0].is_empty() {
            ui.weak("Import a TSL to add tones.");
        }
    }
    fn channel_cards(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("AMP CHANNELS").size(11.).strong());
        if self.connected.is_none() {
            ui.weak("Connect your amp in Settings to see its channels.");
            return;
        }
        ui.small(match self.write_gate.access() {
            Access::ReadOnly => "Choose Preview & edit to switch channels.",
            Access::EditBuffer => "Click to switch. Enable amp writes to drag-save.",
            Access::Persistent => "Click to switch. Drag a tone here to save it.",
        });
        for slot in 0..9 {
            if !self
                .connected
                .as_ref()
                .is_some_and(|id| id.slot_exists(slot))
            {
                continue;
            }
            let can_save = self.can_save_to(slot);
            let hovering = egui::DragAndDrop::has_payload_of_type::<PatchDrag>(ui.ctx());
            let title = match &self.channel_names[slot] {
                Some(name) => format!("{}   {name}", slot_label(slot)),
                None => slot_label(slot),
            };
            let r = ui.add(
                egui::Button::new(title)
                    .selected(self.channel_patch.is_some() && self.slot == slot)
                    .min_size(egui::vec2(ui.available_width(), 32.)),
            );
            if hovering && r.contains_pointer() {
                ui.painter().rect_stroke(
                    r.rect,
                    7.,
                    egui::Stroke::new(
                        2_f32,
                        if can_save {
                            self.theme.accent()
                        } else {
                            ui.visuals().error_fg_color
                        },
                    ),
                    egui::StrokeKind::Inside,
                );
            }
            if let Some(p) = r.dnd_release_payload::<PatchDrag>() {
                self.request_save(p.0.clone(), slot);
            }
            if r.clicked() {
                self.choose_channel(slot);
            }
            r.on_hover_text(if self.write_gate.access() == Access::ReadOnly {
                "Choose Preview & edit to switch channels. Copying to the library is a separate action in Settings."
            } else if can_save {
                "Drop a library tone to save. Click to hear this channel (discards unsaved amp-buffer edits)."
            } else {
                "Click to hear this channel (discards unsaved amp-buffer edits). Enable amp writes to save by dropping."
            });
        }
    }
    fn preset_sidebar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("presets")
            .default_size(235.)
            .min_size(190.)
            .max_size(310.)
            .resizable(true)
            .show(ui, |ui| {
                ui.label(
                    RichText::new("MY TONES")
                        .size(11.)
                        .strong()
                        .color(self.theme.accent()),
                );
                self.collection_picker(ui);
                ui.small("Click to preview · Drag to save");
                // Keep the destination cards reachable even with a large imported liveset.
                let height = (ui.available_height() * 0.34).clamp(100., 240.);
                egui::ScrollArea::vertical()
                    .id_salt("tone-list")
                    .max_height(height)
                    .show(ui, |ui| self.tone_list(ui));
                ui.horizontal(|ui| {
                    if ui.button("Import TSL…").clicked() {
                        self.pick_file(false);
                    }
                    if ui.button("Export TSL…").clicked() {
                        self.pick_file(true);
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("channel-list")
                    .show(ui, |ui| self.channel_cards(ui));
            });
    }
    fn library_page(&mut self, ui: &mut egui::Ui) {
        ui.heading("Your tone library");
        self.collection_tools(ui);
        ui.label(
            "Choose a tone to edit or preview. Collections save locally; TSL export shares a set.",
        );
        ui.horizontal(|ui| {
            ui.label("Liveset name:");
            if ui.text_edit_singleline(&mut self.library.name).changed() {
                self.dirty = true;
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.library.data[0].len() < 1000,
                    egui::Button::new("New tone"),
                )
                .clicked()
            {
                self.library.data[0].push(Patch::new(&self.schema).unwrap());
                self.selected = self.library.data[0].len() - 1;
                self.dirty = true;
                self.disarm();
            }
            if ui
                .add_enabled(
                    !self.library.data[0].is_empty() && self.library.data[0].len() < 1000,
                    egui::Button::new("Duplicate"),
                )
                .clicked()
            {
                let p = self.library.data[0][self.selected].clone();
                self.library.data[0].insert(self.selected + 1, p);
                self.tone_selection.clear();
                self.selected += 1;
                self.dirty = true;
                self.disarm();
            }
            if ui
                .add_enabled(
                    !self.library.data[0].is_empty(),
                    egui::Button::new("Delete…"),
                )
                .clicked()
            {
                self.confirm = Some("delete".into());
            }
            if ui
                .add_enabled(self.selected > 0, egui::Button::new("Move up"))
                .clicked()
            {
                self.tone_selection.clear();
                self.library.data[0].swap(self.selected, self.selected - 1);
                self.selected -= 1;
                self.dirty = true;
            }
            if ui
                .add_enabled(
                    self.selected + 1 < self.library.data[0].len(),
                    egui::Button::new("Move down"),
                )
                .clicked()
            {
                self.tone_selection.clear();
                self.library.data[0].swap(self.selected, self.selected + 1);
                self.selected += 1;
                self.dirty = true;
            }
        });
        if let Some(p) = self.library.data[0].get_mut(self.selected) {
            ui.separator();
            ui.label("Tone name:");
            let mut name = p.name();
            let renamed = ui
                .add(egui::TextEdit::singleline(&mut name).char_limit(16))
                .changed()
                && p.rename(&self.schema, &name).is_ok();
            if renamed {
                self.dirty = true;
            }
            ui.label("Notes:");
            if p.memo.is_object() {
                let mut note = p
                    .memo
                    .get("memo")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_owned();
                if ui.text_edit_multiline(&mut note).changed() {
                    p.memo["memo"] = note.into();
                    self.dirty = true;
                }
            } else {
                ui.weak("Original notes format preserved.");
            }
            if renamed {
                self.disarm();
            }
        }
        ui.separator();
        ui.label("Export TSL shares this collection as a file. Dropping onto a channel saves to your amplifier.");
        ui.small("Drop a Gen 3 TSL onto this window to add a collection. Existing tones are kept.");
        self.bulk_library(ui);
    }
    fn connection(&mut self, ui: &mut egui::Ui) {
        ui.heading("Amplifier");
        ui.label(
            "Connect your Katana by USB, refresh the list, then choose its MIDI control port.",
        );
        #[cfg(feature = "midi")]
        {
            if ui
                .add_enabled(!self.busy, egui::Button::new("Refresh MIDI ports"))
                .clicked()
            {
                match crate::midi::ports() {
                    Ok(p) => self.ports = p,
                    Err(e) => self.log(e.to_string()),
                }
            }
            for p in &self.ports {
                ui.radio_value(
                    &mut self.port,
                    p.address.clone(),
                    format!("{} ({})", p.name, p.address),
                );
            }
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.port).hint_text("ALSA MIDI address"));
                if self.connected.is_some() {
                    if ui
                        .add_enabled(!self.busy, egui::Button::new("Disconnect"))
                        .clicked()
                    {
                        self.disarm();
                        self.submit(Job::Disconnect);
                    }
                } else if ui
                    .add_enabled(
                        !self.busy && !self.port.is_empty(),
                        egui::Button::new("Connect"),
                    )
                    .clicked()
                {
                    self.submit(Job::Connect(self.port.clone()));
                }
            });
            if self.ports.is_empty() {
                ui.weak("No ports listed? Refresh first. Check the USB cable and close other MIDI editors if needed.");
            }
        }
        #[cfg(not(feature = "midi"))]
        ui.label("This build is offline-only (no MIDI support).");
        ui.small("Read-only protects tones and settings, including temporary sound changes. The editor connection still sends session setup/cleanup commands. Nothing connects automatically.");
        ui.separator();
        self.settings_page(ui);
        ui.separator();
        ui.heading("Appearance");
        ui.horizontal_wrapped(|ui| {
            for theme in Theme::ALL {
                if ui
                    .selectable_label(
                        self.theme == theme,
                        RichText::new(theme.name()).color(theme.accent()),
                    )
                    .clicked()
                {
                    self.theme = theme;
                    theme.apply(ui.ctx());
                    if let Some(p) = Theme::preferences_path()
                        && let Err(e) = theme.save(&p)
                    {
                        self.log(format!("Could not remember theme: {e}"));
                    }
                }
            }
        });
        ui.small("Puppet · copper & earth    Justice · pearl & pale green    Kill · crimson    Lightning · steel blue & purple");
        ui.separator();
        egui::CollapsingHeader::new("Backup & channel reads").show(ui, |ui| {
            self.backup_page(ui);
        });
        egui::CollapsingHeader::new("Offline amp model").show(ui, |ui| {
            ui.small("Used only when no amp is connected.");
            egui::ComboBox::from_id_salt("offline-model")
                .selected_text(crate::editor::model_name(self.offline_model))
                .show_ui(ui, |ui| {
                    for code in 5..=11 {
                        ui.selectable_value(
                            &mut self.offline_model,
                            code,
                            crate::editor::model_name(code),
                        );
                    }
                });
        });
        egui::CollapsingHeader::new("Help & connection log").show(ui, |ui| {
            ui.label(concat!("Rusty Boss ", env!("CARGO_PKG_VERSION"), " · A BOSS KATANA editor for Linux. Currently supports Gen 3."));
            ui.label("Experimental hardware support: keep a known-good backup before saving channels, applying settings or restoring a backup.");
            ui.label("Independent software. Not affiliated with Roland/BOSS.");
            egui::CollapsingHeader::new("Typeface license").show(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("font-license").max_height(180.).show(ui, |ui| {
                    ui.small(wordmark::FONT_LICENSE);
                });
            });
            for log in &self.logs { ui.small(log); }
        });
    }
    fn backup_page(&mut self, ui: &mut egui::Ui) {
        ui.label("Read saved channels without changing the amp's current sound.");
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("read-slot")
                .selected_text(slot_label(self.slot))
                .show_ui(ui, |ui| {
                    for n in 0..9 {
                        if self.connected.as_ref().is_none_or(|id| id.slot_exists(n)) {
                            ui.selectable_value(&mut self.slot, n, slot_label(n));
                        }
                    }
                });
            #[cfg(feature = "midi")]
            if ui
                .add_enabled(
                    self.connected.is_some() && !self.busy,
                    egui::Button::new("Copy channel to library"),
                )
                .clicked()
            {
                self.submit(Job::ReadSlot(self.slot));
            }
            #[cfg(feature = "midi")]
            if ui
                .add_enabled(
                    self.connected.is_some() && !self.busy,
                    egui::Button::new("Read full backup"),
                )
                .clicked()
            {
                self.submit(Job::Backup);
            }
        });
        ui.small(
            "Leave the amp idle during a full backup. Reads run sequentially, not as one snapshot.",
        );
        ui.horizontal(|ui| {
            ui.label("Backup file:");
            ui.text_edit_singleline(&mut self.backup_file);
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!self.busy, egui::Button::new("Open ALB"))
                .clicked()
            {
                self.choose_file(FileAction::BackupOpen);
            }
            if ui
                .add_enabled(
                    self.backup.is_some() && !self.busy,
                    egui::Button::new("Save ALB…"),
                )
                .clicked()
            {
                self.choose_file(FileAction::BackupSave);
            }
            let matching = self
                .backup
                .as_ref()
                .zip(self.connected.as_ref())
                .is_some_and(|(b, id)| b.model == id.name);
            if ui
                .add_enabled(
                    matching && !self.busy && self.write_gate.access() == Access::Persistent,
                    egui::Button::new("Restore full backup…"),
                )
                .clicked()
            {
                self.confirm = Some("restore".into());
            }
        });
        let mut extract = None;
        if let Some(b) = &self.backup {
            for (slot, patch) in b.patches.iter().enumerate() {
                if let Some(p) = patch {
                    let patch = Patch {
                        parameters: p.0.clone(),
                        ..Patch::new(&self.schema).unwrap()
                    };
                    ui.horizontal(|ui| {
                        ui.label(format!("{} · {}", slot_label(slot), patch.name()));
                        if ui.button("Copy to library").clicked() {
                            extract = Some(patch);
                        }
                    });
                }
            }
        }
        if let Some(mut p) = extract {
            match p.complete(&self.schema) {
                Ok(w) => {
                    self.library.data[0].push(p);
                    self.selected = self.library.data[0].len() - 1;
                    self.dirty = true;
                    self.disarm();
                    self.log(format!(
                        "Tone recovered ({} default-filled blocks)",
                        w.len()
                    ));
                }
                Err(e) => self.log(e.to_string()),
            }
        }
        ui.small("Full restore requires a backup from this exact model and enabled amp writes. It replaces system settings, PANEL and all saved channels. Keep a separate known-good backup.");
    }
    fn confirmation(&mut self, ctx: &egui::Context) {
        let Some(action) = self.confirm.clone() else {
            return;
        };
        let mut yes = false;
        let mut no = false;
        let modal = egui::Modal::new(egui::Id::unique("confirm-operation")).show(ctx, |ui| {
            ui.set_max_width(460.);
            if action == "import-path" || action == "save-path" {
                ui.heading(if action == "import-path" { "Import TSL" } else { "Save TSL file" });
                ui.label("File path:"); ui.text_edit_singleline(&mut self.file);
                ui.small(if action == "import-path" { "Adds a collection without replacing existing tones. The amp is unchanged." } else { "Exports this collection. An existing file at this path will be replaced." });
            } else {
                ui.heading("Confirm");
                let message = match action.as_str() {
                    "unlock" => "Enable persistent amp writes? You can save tones, apply system settings and restore backups. Each operation still asks first. Keep a known-good backup.",

                    "delete" => "Delete this local tone? This cannot be undone.",
                    "save-backup" => "Save the backup ALB to the chosen path? An existing file will be replaced.",
                    "apply-system" => "Apply the staged settings to your amp? These are persistent settings, not just a tone preview. Keep a backup.",
                    "restore" => "Restore this backup's system settings, PANEL and all saved channels? This replaces the entire amp configuration. Use a separate known-good backup. Failure/cancellation can leave a partial restore; never retry blindly.",
                    "delete-set" => "Delete this local collection and all its tones? Export a TSL first if you want to keep it.",
                    "delete-selected" => "Delete the selected local tones? This cannot be undone.",
                    "save-many" => "Overwrite the listed amp channels? This changes the current sound. Each completed save is permanent; cancellation does not roll back earlier channels.",
                    _ => "Continue?",
                };
                if action == "save-many" {
                    ui.label(message);
                    for (slot, patch) in &self.pending_bulk { ui.label(format!("{} ← {}", slot_label(*slot), patch.name())); }
                } else if action == "restore" {
                    ui.label(message);
                    if let Some(b) = &self.backup { ui.strong(format!("Backup: {} · Amp: {}", b.model, self.connected.as_ref().map(|id| id.name).unwrap_or("Disconnected"))); }
                } else if action == "save-channel" {
                    if let Some(r) = &self.pending_save {
                        ui.label(format!("Save “{}” to {}?", r.patch.name(), slot_label(r.slot)));
                        ui.label(if r.slot == 0 {
                            "This replaces the saved PANEL settings, including its effect choices, and changes the current amp sound. Physical knobs can override panel-controlled values. Keep a backup."
                        } else {
                            "This replaces the saved channel and changes the current amp sound. It cannot be undone without a backup."
                        });
                    }
                } else { ui.label(message); }
            }
            ui.horizontal(|ui| {
                yes = ui.add_enabled(!self.busy, egui::Button::new(if action == "import-path" { "Import" } else if action == "save-path" { "Save" } else { "Yes" })).clicked();
                no = ui.button(if action.ends_with("-path") { "Cancel" } else { "No" }).clicked();
            });
        });
        if no || modal.should_close() {
            self.confirm = None;
            self.pending_save = None;
            self.pending_bulk.clear();
            return;
        }
        if yes {
            self.confirm = None;
            self.confirm_action(&action);
        }
    }
    fn confirm_action(&mut self, action: &str) {
        match action {
            "unlock" => {
                self.set_access(Access::Persistent);
                self.log("Amp writes enabled · Saving channels, applying settings and restoring still require confirmation");
            }
            "import-path" => {
                match LiveSet::load(&PathBuf::from(&self.file), &self.schema) {
                    Ok((library, warnings)) => {
                        match self.library.add(library) {
                            Ok(()) => {
                                self.selected = 0;
                                self.tone_selection.clear();
                                self.dirty = true;
                                self.disarm();
                                self.log(format!("TSL added as a collection · {} tones · {} compatibility warnings", self.library.data[0].len(), warnings.len()));
                            }
                            Err(e) => self.log(e.to_string()),
                        }
                    }
                    Err(e) => self.log(format!("Import failed: {e:#}")),
                }
            }
            "save-path" => {
                match self.library.sets[self.library.active].save(&PathBuf::from(&self.file)) {
                    Ok(()) => {
                        self.persist_library();
                        self.log("TSL exported on your computer");
                    }
                    Err(e) => self.log(format!("Save failed: {e:#}")),
                }
            }
            "delete" => {
                if !self.library.data[0].is_empty() {
                    self.library.data[0].remove(self.selected);
                    self.tone_selection.clear();
                    self.selected = self
                        .selected
                        .min(self.library.data[0].len().saturating_sub(1));
                    self.dirty = true;
                    self.disarm();
                }
            }
            "save-backup" => {
                if let Some(b) = &self.backup {
                    match b.save(&PathBuf::from(&self.backup_file)) {
                        Ok(()) => self.log("Backup saved"),
                        Err(e) => self.log(e.to_string()),
                    }
                }
            }
            "delete-set" => self.delete_collection(),
            "delete-selected" => self.delete_selected(),
            "apply-system" => {
                if !self.busy
                    && self.connected.is_some()
                    && self.write_gate.access() == Access::Persistent
                {
                    #[cfg(feature = "midi")]
                    if let Some(draft) = self.system.clone() {
                        self.disarm();
                        self.submit(Job::ApplySystem(Box::new(draft)));
                    }
                }
            }
            "restore" => {
                if !self.busy
                    && self.connected.is_some()
                    && self.write_gate.access() == Access::Persistent
                {
                    #[cfg(feature = "midi")]
                    if let Some(b) = self.backup.clone() {
                        self.disarm();
                        self.submit(Job::Restore(Box::new(b)));
                    }
                }
            }
            "save-many" => {
                let patches = std::mem::take(&mut self.pending_bulk);
                if !patches.is_empty() && patches.iter().all(|(slot, _)| self.can_save_to(*slot)) {
                    self.disarm();
                    #[cfg(feature = "midi")]
                    self.submit(Job::SaveMany(patches));
                }
            }
            "save-channel" => {
                if let Some(r) = self.pending_save.take() {
                    if self.can_save_to(r.slot) {
                        self.disarm();
                        #[cfg(feature = "midi")]
                        self.submit(Job::Save(r));
                    } else {
                        self.log("Save cancelled · Channel writes are locked or amp unavailable");
                    }
                }
            }
            _ => {}
        }
    }
}
impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &ui.ctx().clone();
        #[cfg(feature = "midi")]
        self.events();
        self.poll_file_dialog();
        if !self.busy
            && self.confirm.is_none()
            && let Some(path) = ctx.input(|i| {
                i.raw
                    .dropped_files
                    .iter()
                    .map(|f| f.path().to_path_buf())
                    .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("tsl")))
            })
        {
            self.file = path.to_string_lossy().into_owned();
            self.import_file();
        }
        self.header(ui);
        if self.page != Page::Settings {
            self.preset_sidebar(ui);
        }
        match self.page {
            Page::Editor => self.classic_editor(ui),
            Page::Library => {
                egui::CentralPanel::default().show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.add_enabled_ui(!self.busy, |ui| self.library_page(ui));
                    });
                });
            }
            Page::Settings => {
                egui::CentralPanel::default().show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| self.connection(ui));
                });
            }
        }
        self.confirmation(ctx);
        if self.dirty
            && (self.autosave_at.elapsed().as_millis() >= 750
                || ctx.input(|i| i.viewport().close_requested()))
        {
            self.persist_library();
        }
        #[cfg(feature = "midi")]
        self.flush_edits();
        ctx.request_repaint_after(std::time::Duration::from_millis(if self.busy {
            25
        } else {
            100
        }));
        if std::env::var_os("RUSTY_BOSS_SMOKE_TEST").is_some() {
            self.smoke_frames += 1;
            let frames = std::env::var("RUSTY_BOSS_SMOKE_FRAMES")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(4);
            if self.smoke_frames >= frames {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if ctx.input(|i| i.viewport().close_requested())
            && self.dirty
            && std::env::var_os("RUSTY_BOSS_SMOKE_TEST").is_none()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_dialog = true;
        }
        if self.close_dialog {
            egui::Modal::new(egui::Id::unique("unsaved-close")).show(ctx, |ui| {
                ui.heading("Unsaved tones");
                ui.label("Local autosave has not succeeded. Export the collections you need before closing.");
                if ui.button("Export TSL…").clicked() {
                    self.close_dialog = false;
                    self.pick_file(true);
                }
                if ui.button("Discard and close").clicked() {
                    self.dirty = false;
                    self.close_dialog = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button("Keep editing").clicked() {
                    self.close_dialog = false;
                }
            });
        }
    }
}
pub fn run() -> eframe::Result {
    eframe::run_native(
        "Rusty Boss",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1180., 800.])
                .with_min_inner_size([800., 560.]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(App::new(&cc.egui_ctx)))),
    )
}
