#[cfg(feature = "midi")]
use crate::protocol::Message;
use crate::{
    document::{Backup, LiveSet, ParamSet, Patch},
    editor::{Catalog, Edit, Section},
    protocol::Identity,
    safety::{Access, WriteGate},
    schema::{slot_label, Schema},
};
use eframe::egui::{self, Color32, RichText};
use std::{collections::VecDeque, path::PathBuf};
mod classic;
#[cfg(test)]
mod tests;
mod theme;
mod wordmark;
#[cfg(feature = "midi")]
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
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
    Set(String, usize, Vec<u8>),
    Shutdown,
}
#[cfg(feature = "midi")]
enum Event {
    Connected(Identity),
    Disconnected,
    Current(Patch),
    Slot(usize, Patch),
    ChannelNames([Option<String>; 9]),
    Previewed(u64, ParamSet),
    Saved(SaveRequest),
    Backup(Box<Backup>),
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
                                    Event::Current(Patch {
                                        parameters: s.read_root(&schema, "PATCH")?,
                                        ..Patch::new(&schema)?
                                    })
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
                                Job::Set(id, i, b) => {
                                    s.set_parameter(&schema, &id, i, b)?;
                                    Event::Done("Sound updated · saved channels unchanged".into())
                                }
                                _ => unreachable!(),
                            })
                        }
                    }
                })();
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
    file_dialog: Option<(bool, std::sync::mpsc::Receiver<Option<PathBuf>>)>,
    write_gate: WriteGate,
    write_busy: bool,
    pending_edits: VecDeque<Edit>,
    generation: u64,
    previewing: bool,
    pending_preview: Option<PreviewRequest>,
    pending_save: Option<SaveRequest>,
    channel_names: [Option<String>; 9],
    library: LiveSet,
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
        let mut library = LiveSet::new("My tones");
        library.data[0].push(Patch::new(&schema).unwrap());
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
            write_gate,
            write_busy: false,
            pending_edits: VecDeque::new(),
            generation: 0,
            previewing: false,
            pending_preview: None,
            pending_save: None,
            channel_names: Default::default(),
            library,
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
        if self.file_dialog.is_some() || self.busy {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let current = PathBuf::from(&self.file);
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new().add_filter("Katana tones", &["tsl"]);
            if let Some(parent) = current.parent().filter(|p| p.is_dir()) {
                dialog = dialog.set_directory(parent);
            }
            let path = if saving {
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
        self.file_dialog = Some((saving, rx));
    }
    fn poll_file_dialog(&mut self) {
        if self.busy {
            return;
        }
        let Some((saving, rx)) = &self.file_dialog else {
            return;
        };
        match rx.try_recv() {
            Ok(path) => {
                let saving = *saving;
                self.file_dialog = None;
                if let Some(path) = path {
                    self.file = path.to_string_lossy().into_owned();
                    if saving {
                        self.confirm_action("save-path");
                    } else {
                        self.import_file();
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
        if self.dirty {
            self.confirm = Some("import-path".into());
        } else {
            self.confirm_action("import-path");
        }
    }
    fn log(&mut self, s: impl Into<String>) {
        let s = s.into();
        self.status = s.clone();
        self.logs.push_front(s);
        self.logs.truncate(100);
    }
    fn disarm(&mut self) {
        self.live = false;
        self.synced = false;
        self.pending_edits.clear();
        self.generation = self.generation.wrapping_add(1);
        self.pending_preview = None;
    }
    fn set_access(&mut self, access: Access) {
        // An acknowledged in-flight preview must not re-arm after protection changes.
        let resume = self.synced && self.connected.is_some() && !self.busy;
        self.disarm();
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
        self.write_busy = matches!(job, Job::Set(..));
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
        self.slot = slot;
        self.disarm();
        #[cfg(feature = "midi")]
        self.submit(if self.write_gate.access() == Access::ReadOnly {
            Job::ReadSlot(slot)
        } else {
            Job::Select(slot)
        });
    }
    fn can_save_to(&self, slot: usize) -> bool {
        !self.busy
            && self.confirm.is_none()
            && self.write_gate.access() == Access::Persistent
            && slot > 0
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
        if !self.busy {
            if let Some(e) = self.pending_edits.pop_front() {
                self.submit(Job::Set(e.block, e.index, e.bytes));
            }
        }
    }
    #[cfg(feature = "midi")]
    fn handle_event(&mut self, e: Event) {
        if !matches!(&e, Event::Update(_)) {
            self.busy = false;
            self.write_busy = false;
            self.previewing = false;
        }
        match e {
            Event::Connected(id) => {
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
                self.connected = None;
                self.disarm();
                self.pending_save = None;
                self.write_gate.set(Access::ReadOnly);
                self.channel_names = Default::default();
                self.log("Disconnected · Read-only protection restored");
            }
            Event::Current(p) => {
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
                self.log("Ready · Click a tone to preview, or enable channel writes and drag it to a channel");
            }
            Event::Slot(slot, p) => {
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
                    }
                    self.synced = true;
                    self.live = true;
                    self.log("Previewing on amp · Knobs are live · Saved channels unchanged");
                }
            }
            Event::Saved(r) => {
                let name = r.patch.name();
                self.channel_names[r.slot] = Some(name.clone());
                let matching = self.library.data[0]
                    .get(self.selected)
                    .is_some_and(|p| p.parameters == r.patch.parameters);
                self.disarm();
                self.synced = matching;
                self.live = matching && self.write_gate.access() != Access::ReadOnly;
                self.log(format!(
                    "Saved {name} to {}{}",
                    slot_label(r.slot),
                    if self.live {
                        " · Knobs are live"
                    } else {
                        " · Click a tone to preview"
                    }
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
                if self.synced {
                    for b in self
                        .schema
                        .blocks
                        .iter()
                        .filter(|b| b.id.starts_with("PATCH%"))
                    {
                        let first = address.linear() as i64 - b.address as i64;
                        if first >= 0 && first as usize + data.len() <= b.size {
                            if let Some(dst) = self.library.data[0]
                                .get_mut(self.selected)
                                .and_then(|p| p.parameters.get_mut(&b.id))
                            {
                                dst[first as usize..first as usize + data.len()]
                                    .copy_from_slice(&data);
                                self.dirty = true;
                            }
                            break;
                        }
                    }
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
                if ui.button("Channel writes enabled · Lock").clicked() { self.set_access(Access::EditBuffer); }
            } else if ui.button("Enable channel writes…").clicked() { self.confirm = Some("unlock".into()); }
        });
    }
    fn header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.window_fill)
                    .inner_margin(egui::Margin::symmetric(18, 10)),
            )
            .show(ctx, |ui| {
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
                                RichText::new("● Unsaved file")
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
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
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
                    .selected(i == self.selected)
                    .sense(egui::Sense::click_and_drag())
                    .min_size(egui::vec2(ui.available_width(), 34.)),
            );
            r.dnd_set_drag_payload(PatchDrag(p.clone()));
            if r.clicked() {
                next = Some(i);
            }
            if r.dragged() {
                egui::show_tooltip_at_pointer(ui.ctx(), ui.layer_id(), r.id.with("drag"), |ui| {
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
        ui.small(if self.write_gate.access() == Access::Persistent {
            "Drag a tone here to save it."
        } else {
            "Saved channels locked. Enable channel writes to save a tone."
        });
        for slot in 1..9 {
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
            let r =
                ui.add(egui::Button::new(title).min_size(egui::vec2(ui.available_width(), 32.)));
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
                "Click to copy this channel into your library without changing the amp. Saving is locked."
            } else if can_save {
                "Drop a library tone to save. Click to hear this channel (discards unsaved amp-buffer edits)."
            } else {
                "Click to hear this channel (discards unsaved amp-buffer edits). Enable channel writes to save by dropping."
            });
        }
    }
    fn preset_sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("presets")
            .default_width(235.)
            .min_width(190.)
            .max_width(310.)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("MY TONES")
                        .size(11.)
                        .strong()
                        .color(self.theme.accent()),
                );
                ui.small(&self.library.name);
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
                    if ui.button("Save file…").clicked() {
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
        ui.label("Choose a tone on the left to edit or preview it. Drag it onto an amp channel to save it there.");
        ui.horizontal(|ui| {
            ui.label("Liveset name:");
            if ui.text_edit_singleline(&mut self.library.name).changed() {
                self.dirty = true;
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("New tone").clicked() {
                self.library.data[0].push(Patch::new(&self.schema).unwrap());
                self.selected = self.library.data[0].len() - 1;
                self.dirty = true;
                self.disarm();
            }
            if ui
                .add_enabled(
                    !self.library.data[0].is_empty(),
                    egui::Button::new("Duplicate"),
                )
                .clicked()
            {
                let p = self.library.data[0][self.selected].clone();
                self.library.data[0].insert(self.selected + 1, p);
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
        ui.label("Save file saves a TSL on your computer. Dropping onto a channel saves to your amplifier.");
        ui.hyperlink_to(
            "Find Gen 3 tones on BOSS Tone Exchange",
            "https://bosstoneexchange.com/gear/katana_gen_3",
        );
        ui.small("You can also drop a .tsl file onto this window to import it. Import replaces the local liveset; save your work first.");
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
                    if let Some(p) = Theme::preferences_path() {
                        if let Err(e) = theme.save(&p) {
                            self.log(format!("Could not remember theme: {e}"));
                        }
                    }
                }
            }
        });
        ui.small("Puppet · warm orange    Justice · pearl light    Kill · crimson    Lightning · plasma blue");
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
            ui.label("Rusty Boss · A BOSS KATANA editor for Linux. Currently supports Gen 3.");
            ui.label("Experimental hardware support: keep a known-good backup before saving channels. MkI/MkII conversion and bulk restore are not available.");
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
                match Backup::load(&PathBuf::from(&self.backup_file)) {
                    Ok(b) => {
                        self.backup = Some(b);
                        self.log("Backup opened");
                    }
                    Err(e) => self.log(e.to_string()),
                }
            }
            if ui
                .add_enabled(
                    self.backup.is_some() && !self.busy,
                    egui::Button::new("Save ALB…"),
                )
                .clicked()
            {
                self.confirm = Some("save-backup".into());
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
        ui.small("Bulk restore is not available. You can recover individual tones and drag them to channels.");
    }
    fn confirmation(&mut self, ctx: &egui::Context) {
        let Some(action) = self.confirm.clone() else {
            return;
        };
        let mut yes = false;
        let mut no = false;
        let modal = egui::Modal::new(egui::Id::new("confirm-operation")).show(ctx, |ui| {
            ui.set_max_width(460.);
            if action == "import-path" || action == "save-path" {
                ui.heading(if action == "import-path" { "Import TSL" } else { "Save TSL file" });
                ui.label("File path:"); ui.text_edit_singleline(&mut self.file);
                ui.small(if action == "import-path" { "Replaces the local liveset. Save any unsaved work first. The amp is not changed until you click a tone in preview mode." } else { "Saves all local tones, including edits. An existing file at this path will be replaced." });
            } else {
                ui.heading("Confirm");
                let message = match action.as_str() {
                    "unlock" => "Enable channel writes? You will be able to overwrite saved tones by dropping them onto a channel. Each overwrite still asks first. Keep a backup.",

                    "delete" => "Delete this local tone? This cannot be undone.",
                    "save-backup" => "Save the backup ALB to the chosen path? An existing file will be replaced.",
                    _ => "Continue?",
                };
                if action == "save-channel" {
                    if let Some(r) = &self.pending_save {
                        ui.label(format!("Save “{}” to {}?", r.patch.name(), slot_label(r.slot)));
                        ui.label("This replaces the saved channel and changes the current amp sound. It cannot be undone without a backup.");
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
                self.log("Channel writes enabled · Drag a tone onto a channel to save");
            }
            "import-path" => match LiveSet::load(&PathBuf::from(&self.file), &self.schema) {
                Ok((library, warnings)) => {
                    self.library = library;
                    self.selected = 0;
                    self.dirty = false;
                    self.disarm();
                    self.log(format!("TSL imported · {} tones · {} compatibility warnings · Click a tone to preview", self.library.data[0].len(), warnings.len()));
                }
                Err(e) => self.log(format!("Import failed: {e:#}")),
            },
            "save-path" => match self.library.save(&PathBuf::from(&self.file)) {
                Ok(()) => {
                    self.dirty = false;
                    self.log("TSL saved on your computer");
                }
                Err(e) => self.log(format!("Save failed: {e:#}")),
            },
            "delete" => {
                if !self.library.data[0].is_empty() {
                    self.library.data[0].remove(self.selected);
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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(feature = "midi")]
        self.events();
        self.poll_file_dialog();
        if !self.busy && self.confirm.is_none() {
            if let Some(path) = ctx.input(|i| {
                i.raw
                    .dropped_files
                    .iter()
                    .filter_map(|f| f.path.clone())
                    .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("tsl")))
            }) {
                self.file = path.to_string_lossy().into_owned();
                self.import_file();
            }
        }
        self.header(ctx);
        if self.page != Page::Settings {
            self.preset_sidebar(ctx);
        }
        match self.page {
            Page::Editor => self.classic_editor(ctx),
            Page::Library => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.add_enabled_ui(!self.busy, |ui| self.library_page(ui));
                    });
                });
            }
            Page::Settings => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| self.connection(ui));
                });
            }
        }
        self.confirmation(ctx);
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
            egui::Modal::new(egui::Id::new("unsaved-close")).show(ctx, |ui| {
                ui.heading("Unsaved tones");
                ui.label("Save your TSL file before closing?");
                if ui.button("Save file…").clicked() {
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
