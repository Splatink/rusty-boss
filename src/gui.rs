#[cfg(feature = "midi")]
use crate::protocol::Message;
use crate::{
    document::{Backup, LiveSet, ParamSet, Patch},
    protocol::Identity,
    schema::{slot_label, Schema},
};
use crate::{
    editor::{Catalog, Edit, Section},
    safety::{Access, WriteGate},
};
use eframe::egui::{self, Color32, RichText};
mod classic;
use std::{collections::VecDeque, path::PathBuf};
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

#[cfg(feature = "midi")]
enum Job {
    Connect(String),
    Disconnect,
    ReadCurrent,
    ReadSystem,
    ReadSlot(usize),
    Select(usize),
    Preview(Patch),
    Commit(usize),
    Backup,
    Set(String, usize, Vec<u8>),
    Shutdown,
}
#[cfg(feature = "midi")]
enum Event {
    Connected(Identity),
    Disconnected,
    Current(Patch),
    Slot(Patch),
    System(ParamSet),
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
                                Job::ReadSystem => Event::System(s.read_root(&schema, "SYSTEM")?),
                                Job::ReadSlot(n) => Event::Slot(s.read_slot(&schema, n)?),
                                Job::Select(n) => {
                                    s.select_slot(n)?;
                                    Event::Current(Patch {
                                        parameters: s.read_root(&schema, "PATCH")?,
                                        ..Patch::new(&schema)?
                                    })
                                }
                                Job::Preview(p) => {
                                    s.preview(&schema, &p)?;
                                    Event::Done("Preview sent to edit buffer; stored slots unchanged. Read back before committing.".into())
                                }
                                Job::Commit(n) => {
                                    s.commit(n)?;
                                    Event::Done(format!(
                                        "Commit acknowledged for {}",
                                        slot_label(n)
                                    ))
                                }
                                Job::Backup => Event::Backup(Box::new(s.read_backup(&schema)?)),
                                Job::Set(id, i, b) => {
                                    s.set_parameter(&schema, &id, i, b)?;
                                    Event::Done(if id.starts_with("SYSTEM%") {
                                        "System parameter sent; persistence depends on the amplifier".into()
                                    } else {
                                        "Edit-buffer parameter sent (not committed to a saved slot)"
                                            .into()
                                    })
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
    Librarian,
    System,
    Backup,
    Connection,
    About,
}
struct App {
    schema: Schema,
    catalog: Catalog,
    section: Section,
    expert: bool,
    offline_model: u8,
    write_gate: WriteGate,
    write_busy: bool,
    pending_edits: VecDeque<Edit>,
    confirm_text: String,
    library: LiveSet,
    selected: usize,
    system: ParamSet,
    page: Page,
    filter: String,
    block: String,
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
    system_synced: bool,
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
        let mut style = (*ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        let v = &mut style.visuals;
        v.panel_fill = Color32::from_rgb(19, 22, 27);
        v.window_fill = Color32::from_rgb(27, 31, 38);
        v.extreme_bg_color = Color32::from_rgb(14, 17, 21);
        v.faint_bg_color = Color32::from_rgb(32, 37, 44);
        v.override_text_color = Some(Color32::from_rgb(218, 222, 229));
        v.selection.bg_fill = Color32::from_rgb(91, 61, 39);
        v.selection.stroke = egui::Stroke::new(1_f32, Color32::from_rgb(245, 170, 91));
        for w in [
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
        ] {
            w.corner_radius = egui::CornerRadius::same(6);
        }
        v.widgets.inactive.bg_fill = Color32::from_rgb(37, 43, 52);
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(29, 34, 41);
        v.widgets.inactive.bg_stroke = egui::Stroke::new(1_f32, Color32::from_rgb(53, 61, 72));
        v.widgets.hovered.bg_fill = Color32::from_rgb(52, 59, 69);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(43, 49, 58);
        style.spacing.item_spacing = egui::vec2(10., 10.);
        style.spacing.button_padding = egui::vec2(12., 7.);
        style.spacing.interact_size.y = 30.;
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(14.));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(13.));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(23.));
        ctx.set_style(style);
        let schema = Schema::load();
        let write_gate = WriteGate::default();
        let mut library = LiveSet::new("Rusty Boss");
        library.data[0].push(Patch::new(&schema).unwrap());
        Self {
            system: schema.defaults("SYSTEM").unwrap(),
            #[cfg(feature = "midi")]
            worker: Worker::new(schema.clone(), write_gate.clone()),
            #[cfg(feature = "midi")]
            ports: Vec::new(),
            catalog: Catalog::load(),
            section: Section::Amp,
            expert: false,
            offline_model: 5,
            write_gate,
            write_busy: false,
            pending_edits: VecDeque::new(),
            confirm_text: String::new(),
            schema,
            library,
            selected: 0,
            page: Page::Editor,
            filter: String::new(),
            block: "PATCH%AMP".into(),
            file: "rusty-boss.tsl".into(),
            backup_file: "katana-backup.alb".into(),
            backup: None,
            status: "Offline — factory defaults, not a hardware snapshot".into(),
            logs: VecDeque::new(),
            dirty: false,
            connected: None,
            busy: false,
            live: false,
            synced: false,
            system_synced: false,
            close_dialog: false,
            slot: 1,
            confirm: None,
            #[cfg(feature = "midi")]
            port: String::new(),
            smoke_frames: 0,
        }
    }
    fn log(&mut self, s: impl Into<String>) {
        let s = s.into();
        self.status = s.clone();
        self.logs.push_front(s);
        self.logs.truncate(100);
    }
    #[cfg(feature = "midi")]
    fn submit(&mut self, job: Job) {
        if self.busy {
            return;
        }
        self.write_busy = matches!(job, Job::Set(..));
        if !self.write_busy {
            self.pending_edits.clear();
            self.live = false;
        }
        self.worker.cancel.store(false, Ordering::Relaxed);
        if self.worker.tx.send(job).is_ok() {
            self.busy = true;
            self.status = "Working — MIDI operations run off the UI thread".into();
        } else {
            self.log("MIDI worker unavailable");
        }
    }
    fn disarm(&mut self) {
        self.live = false;
        self.pending_edits.clear();
        self.synced = false;
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
    fn events(&mut self) {
        while let Ok(e) = self.worker.rx.try_recv() {
            if !matches!(&e, Event::Update(_)) {
                self.busy = false;
                self.write_busy = false;
            }
            match e {
                Event::Connected(id) => {
                    self.log(format!(
                        "Connected to {}. Read the edit buffer to synchronize.",
                        id.name
                    ));
                    self.connected = Some(id);
                    self.disarm();
                    self.system_synced = false;
                }
                Event::Disconnected => {
                    self.connected = None;
                    self.disarm();
                    self.log("Disconnected");
                }
                Event::Current(p) => {
                    if self.library.data[0].is_empty() {
                        self.library.data[0].push(p);
                        self.selected = 0;
                    } else {
                        self.library.data[0][self.selected] = p;
                    }
                    self.dirty = true;
                    self.synced = true;
                    self.live = false;
                    self.log("Hardware edit buffer read. Arm live editing explicitly if desired.");
                }
                Event::Slot(p) => {
                    let name = p.name();
                    self.library.data[0].push(p);
                    self.selected = self.library.data[0].len() - 1;
                    self.dirty = true;
                    self.disarm();
                    self.log(format!("Added hardware slot: {name}"));
                }
                Event::System(data) => {
                    self.system = data;
                    self.system_synced = true;
                    self.log(
                        "System settings read; changes remain offline unless live editing is armed",
                    );
                }
                Event::Backup(b) => {
                    self.backup = Some(*b);
                    self.log("Complete read finished. Export ALB to retain this snapshot.");
                }
                Event::Done(s) => self.log(s),
                Event::Error(s) => {
                    self.connected = None;
                    self.disarm();
                    self.log(format!("MIDI ERROR: {s}"));
                }
                Event::Update(Message::Data { address, data, .. }) => {
                    if address.linear() == 0 {
                        self.disarm();
                        self.log(
                            "Hardware channel changed — read edit buffer again before live editing",
                        );
                        continue;
                    }
                    if self.synced {
                        for b in &self.schema.blocks {
                            let first = address.linear() as i64 - b.address as i64;
                            if first >= 0 && (first as usize) + data.len() <= b.size {
                                let target = if b.id.starts_with("PATCH%") {
                                    self.library.data[0]
                                        .get_mut(self.selected)
                                        .map(|p| &mut p.parameters)
                                } else if b.id.starts_with("SYSTEM%") {
                                    Some(&mut self.system)
                                } else {
                                    None
                                };
                                if let Some(set) = target {
                                    if let Some(dst) = set.get_mut(&b.id) {
                                        dst[first as usize..first as usize + data.len()]
                                            .copy_from_slice(&data);
                                        if b.id.starts_with("PATCH%") {
                                            self.dirty = true;
                                        }
                                    }
                                }
                                break;
                            }
                        }
                    }
                }
                Event::Update(_) => {}
            }
        }
    }
    fn header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(23, 27, 33))
                    .inner_margin(egui::Margin::symmetric(20, 12)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("RUSTY BOSS")
                            .size(22.)
                            .strong()
                            .color(Color32::from_rgb(245, 170, 91)),
                    );
                    ui.label(RichText::new("/  KATANA Gen 3").size(13.).weak());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(
                                self.connected
                                    .as_ref()
                                    .map(|i| i.name)
                                    .unwrap_or("OFFLINE  ·  LOCAL EDITOR"),
                            )
                            .size(12.)
                            .color(if self.connected.is_some() {
                                Color32::from_rgb(98, 203, 151)
                            } else {
                                Color32::from_gray(150)
                            }),
                        );
                        if self.dirty {
                            ui.label(
                                RichText::new("● Unsaved")
                                    .size(12.)
                                    .color(Color32::from_rgb(245, 170, 91)),
                            );
                        }
                    });
                });
                ui.add_space(4.);
                ui.horizontal_wrapped(|ui| {
                    for (p, label) in [
                        (Page::Editor, "Editor"),
                        (Page::Librarian, "Librarian"),
                        (Page::System, "System"),
                        (Page::Backup, "Backup"),
                        (Page::Connection, "Connection"),
                        (Page::About, "About / evidence"),
                    ] {
                        if ui.selectable_label(self.page == p, label).clicked() {
                            self.page = p;
                            self.live = false;
                            self.filter.clear();
                            self.block = if p == Page::System {
                                "SYSTEM%COM"
                            } else {
                                "PATCH%AMP"
                            }
                            .into();
                        }
                    }
                    ui.separator();
                    if self.page == Page::Editor {
                        let before = self.expert;
                        ui.selectable_value(&mut self.expert, false, "Standard");
                        ui.selectable_value(&mut self.expert, true, "Advanced raw");
                        if before != self.expert {
                            self.disarm();
                        }
                    }
                });
            });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.busy {
                    ui.spinner();
                }
                ui.label(RichText::new(&self.status).size(12.).weak());
                #[cfg(feature = "midi")]
                if self.busy && ui.button("Cancel").clicked() {
                    self.worker.cancel.store(true, Ordering::Relaxed);
                    self.live = false;
                }
            });
            egui::CollapsingHeader::new("Operation log").show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(110.)
                    .show(ui, |ui| {
                        for s in &self.logs {
                            ui.label(s);
                        }
                    });
            });
        });
    }
    fn editor(&mut self, ctx: &egui::Context, system: bool) {
        if !system && !self.expert {
            self.classic_editor(ctx);
            return;
        }
        let root = if system { "SYSTEM%" } else { "PATCH%" };
        egui::SidePanel::left("blocks")
            .resizable(true)
            .default_width(245.)
            .show(ctx, |ui| {
                ui.heading(if system {
                    "System settings"
                } else {
                    "Effect / parameter blocks"
                });
                ui.text_edit_singleline(&mut self.filter);
                ui.small("Search block or parameter names");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let q = self.filter.to_lowercase();
                    for b in &self.schema.blocks {
                        if !b.id.starts_with(root) {
                            continue;
                        }
                        if !q.is_empty()
                            && !b.id.to_lowercase().contains(&q)
                            && !b
                                .parameters
                                .iter()
                                .any(|p| p.name.to_lowercase().contains(&q))
                        {
                            continue;
                        }
                        let label = b.id.trim_start_matches(root).replace('_', " ");
                        if ui.selectable_label(self.block == b.id, label).clicked() {
                            self.block = b.id.clone();
                        }
                    }
                });
            });
        egui::CentralPanel::default().show(ctx,|ui|{
            if !system && self.library.data[0].is_empty(){ui.label("Add a patch in the Librarian first.");return;}
            if !system {
                ui.heading(self.library.data[0][self.selected].name());
                ui.horizontal(|ui|{
                    ui.label("Patch name:");let mut name=self.library.data[0][self.selected].name();
                    let response=ui.add(egui::TextEdit::singleline(&mut name).char_limit(16));
                    if response.changed(){let result=self.library.data[0][self.selected].rename(&self.schema,&name);match result{Ok(())=>{self.dirty=true;self.disarm();},Err(e)=>self.log(e.to_string())}}
                });
            }
            #[cfg(feature="midi")]
            ui.horizontal_wrapped(|ui|{
                #[cfg(feature="midi")]
                {
                    if ui.add_enabled(self.connected.is_some()&&!self.busy,egui::Button::new(if system{"Read system"}else{"Read edit buffer (replaces local patch)"})).clicked(){self.confirm=Some(if system{"read-system"}else{"read-current"}.into());}
                    let allowed=self.connected.is_some()&&(if system {self.system_synced&&self.write_gate.access()==Access::Persistent}else{self.synced&&self.write_gate.access()!=Access::ReadOnly})&&!self.busy;
                    ui.add_enabled(allowed,egui::Checkbox::new(&mut self.live,"Arm LIVE editing"));
                    if !system && ui.add_enabled(self.connected.is_some()&&!self.busy&&self.write_gate.access()!=Access::ReadOnly,egui::Button::new("Preview local patch on amp…")).clicked(){self.confirm=Some("preview".into());}
                }
            });
            if system { ui.colored_label(Color32::YELLOW,"Offline system values are transient and are NOT saved in TSL livesets. Read hardware and arm live editing to apply individual changes. Use hardware ALB backup to retain applied settings."); }
            ui.small("Advanced view: raw device units, all banks and inactive algorithms. Use Standard editor for named controls and algorithm-specific pages.");
            ui.separator();
            let Some(b)=self.schema.blocks.iter().find(|b|b.id==self.block).cloned() else{return;};
            ui.heading(b.id.replace('%'," / "));ui.monospace(format!("Address {}  •  {} bytes",b.address(),b.size));
            if b.readonly{ui.label("Read-only telemetry / stored knob state");}
            let mut write_job=None;
            let mut local_changed=false;
            egui::ScrollArea::vertical().show(ui,|ui|{
                ui.add_enabled_ui(!self.busy&&!b.readonly,|ui|{
                    let set=if system{&mut self.system}else{&mut self.library.data[0][self.selected].parameters};
                    let data=set.entry(b.id.clone()).or_insert_with(||b.defaults().unwrap());
                    for (i,p) in b.parameters.iter().enumerate(){
                        ui.push_id(i,|ui|{
                            let label=p.label();
                            if p.encoding=="ascii"{return;}
                            let value=p.decode_number(data);
                            let Ok(mut value)=value else{ui.colored_label(Color32::YELLOW,format!("{label}: invalid raw encoding"));return;};
                            let mut finished=false;
                            if !p.choices.is_empty(){
                                ui.horizontal(|ui|{
                                    ui.label(&label);
                                    let display=p.choices.iter().find(|c|c.value==value).map(|c|c.label.clone()).unwrap_or_else(||format!("Unknown ({value})"));
                                    egui::ComboBox::from_id_salt(i).selected_text(display).show_ui(ui,|ui|{
                                        for c in &p.choices{if ui.selectable_value(&mut value,c.value,&c.label).changed(){finished=true;}}
                                    });
                                });
                            } else if p.min==0&&p.max==1 {
                                let mut on=value!=0;if ui.checkbox(&mut on,&label).changed(){value=i32::from(on);finished=true;}
                            }else{
                                let r=ui.add(egui::Slider::new(&mut value,p.min..=p.max).text(&label));
                                if r.changed()&&!system{self.dirty=true;}
                                finished=r.drag_stopped()||(r.changed()&&!r.dragged());
                            }
                            if let Ok(bytes)=p.encode_number(value){
                                if data[p.offset..p.offset+p.width]!=bytes {data[p.offset..p.offset+p.width].copy_from_slice(&bytes);if !system{self.dirty=true;}local_changed=true;}
                                if finished&&self.live {write_job=Some((b.id.clone(),i,bytes));}
                            }
                        });
                    }
                });
            });
            if local_changed && !self.live { if system {self.system_synced=false;} else {self.synced=false;} }
            #[cfg(feature="midi")]
            if let Some((id,i,bytes))=write_job{self.submit(Job::Set(id,i,bytes));}
            #[cfg(not(feature="midi"))]
            let _=write_job;
        });
    }
    fn librarian(&mut self, ui: &mut egui::Ui) {
        ui.heading("Local liveset librarian");
        ui.horizontal(|ui| {
            ui.label("Liveset name:");
            if ui.text_edit_singleline(&mut self.library.name).changed() {
                self.dirty = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("TSL path:");
            ui.text_edit_singleline(&mut self.file);
        });
        ui.horizontal(|ui| {
            if ui.button("Import TSL…").clicked() {
                self.confirm = Some("import".into());
            }
            if ui.button("Save TSL…").clicked() {
                self.confirm = Some("save".into());
            }
            if ui.button("New patch").clicked() {
                self.library.data[0].push(Patch::new(&self.schema).unwrap());
                self.selected = self.library.data[0].len() - 1;
                self.dirty = true;
                self.disarm();
            }
            let exists = !self.library.data[0].is_empty();
            if ui
                .add_enabled(exists, egui::Button::new("Duplicate"))
                .clicked()
            {
                let p = self.library.data[0][self.selected].clone();
                self.library.data[0].insert(self.selected + 1, p);
                self.selected += 1;
                self.dirty = true;
                self.disarm();
            }
            if ui
                .add_enabled(exists, egui::Button::new("Delete…"))
                .clicked()
            {
                self.confirm = Some("delete".into());
            }
            if ui
                .add_enabled(exists && self.selected > 0, egui::Button::new("Move up"))
                .clicked()
            {
                self.library.data[0].swap(self.selected, self.selected - 1);
                self.selected -= 1;
                self.dirty = true;
            }
            if ui
                .add_enabled(
                    exists && self.selected + 1 < self.library.data[0].len(),
                    egui::Button::new("Move down"),
                )
                .clicked()
            {
                self.library.data[0].swap(self.selected, self.selected + 1);
                self.selected += 1;
                self.dirty = true;
            }
        });
        egui::ScrollArea::vertical()
            .max_height(260.)
            .show(ui, |ui| {
                for (i, p) in self.library.data[0].iter().enumerate() {
                    if ui
                        .selectable_label(self.selected == i, format!("{:03}  {}", i + 1, p.name()))
                        .clicked()
                    {
                        self.selected = i;
                        self.live = false;
                        self.synced = false;
                    }
                }
            });
        if let Some(p) = self.library.data[0].get_mut(self.selected) {
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Tone name:");
                let mut name = p.name();
                if ui
                    .add(egui::TextEdit::singleline(&mut name).char_limit(16))
                    .changed()
                    && p.rename(&self.schema, &name).is_ok()
                {
                    self.dirty = true;
                    self.live = false;
                    self.synced = false;
                    self.pending_edits.clear();
                }
            });
            ui.label("Patch notes:");
            if !p.memo.is_object() {
                ui.small("This patch's legacy memo value is preserved verbatim.");
            } else {
                let mut note = p
                    .memo
                    .get("memo")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();
                if ui.text_edit_multiline(&mut note).changed() {
                    p.memo["memo"] = serde_json::Value::String(note);
                    self.dirty = true;
                }
            }
        }
        ui.separator();
        self.slot_controls(ui);
        ui.separator();
        ui.hyperlink_to(
            "Open BOSS Tone Exchange in your browser",
            "https://bosstoneexchange.com/gear/katana_gen_3",
        );
        ui.small("Download Gen 3 TSL files using your browser, then import here. Embedded account/cloud integration and MkI/MkII conversion are not implemented.");
    }
    fn slot_controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Hardware channels");
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("slot")
                .selected_text(slot_label(self.slot))
                .show_ui(ui, |ui| {
                    for i in 0..9 {
                        if self.connected.as_ref().is_none_or(|id| id.slot_exists(i)) {
                            ui.selectable_value(&mut self.slot, i, slot_label(i));
                        }
                    }
                });
            #[cfg(feature = "midi")]
            ui.add_enabled_ui(self.connected.is_some() && !self.busy, |ui| {
                if ui.button("Read slot into library").clicked() {
                    self.submit(Job::ReadSlot(self.slot));
                }
                if ui
                    .add_enabled(
                        self.write_gate.access() != Access::ReadOnly,
                        egui::Button::new("Select on amplifier…"),
                    )
                    .clicked()
                {
                    self.confirm = Some("select".into());
                }
                if ui
                    .add_enabled(
                        self.slot > 0 && self.write_gate.access() == Access::Persistent,
                        egui::Button::new("Commit hardware edit buffer…"),
                    )
                    .clicked()
                {
                    self.confirm_text.clear();
                    self.confirm = Some("commit".into());
                }
            });
        });
        ui.small("Commit stores what is CURRENTLY on the amplifier, not the selected local patch. Preview local changes first, then read back and verify. PANEL cannot be overwritten here.");
    }
    fn backup_page(&mut self, ui: &mut egui::Ui) {
        ui.heading("All-data backup / file recovery");
        ui.label("Hardware backup reads system settings, setup and each channel present on the detected model. A failed or cancelled read is discarded.");
        ui.horizontal(|ui| {
            ui.label("ALB path:");
            ui.text_edit_singleline(&mut self.backup_file);
        });
        ui.horizontal(|ui| {
            #[cfg(feature = "midi")]
            if ui
                .add_enabled(
                    self.connected.is_some() && !self.busy,
                    egui::Button::new("Read amplifier backup"),
                )
                .clicked()
            {
                self.backup = None;
                self.submit(Job::Backup);
            }
            if ui.button("Load ALB file").clicked() {
                match Backup::load(&PathBuf::from(&self.backup_file)) {
                    Ok(b) => {
                        self.backup = Some(b);
                        self.log("Backup loaded locally — nothing sent to hardware");
                    }
                    Err(e) => self.log(format!("{e:#}")),
                }
            }
            if ui
                .add_enabled(self.backup.is_some(), egui::Button::new("Export ALB…"))
                .clicked()
            {
                self.confirm = Some("save-backup".into());
            }
        });
        if let Some(b) = &self.backup {
            ui.label(format!(
                "Model: {} • {} system blocks • {} rows",
                b.model,
                b.system.len(),
                b.patches.len()
            ));
            let mut extract = None;
            for (i, p) in b.patches.iter().enumerate() {
                if let Some(p) = p {
                    let patch = Patch {
                        parameters: p.0.clone(),
                        ..Patch::new(&self.schema).unwrap()
                    };
                    ui.horizontal(|ui| {
                        ui.label(format!("{} — {}", slot_label(i), patch.name()));
                        if ui.button("Copy to library").clicked() {
                            extract = Some(patch);
                        }
                    });
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
                            "Copied backup patch to library ({} default-fill warnings)",
                            w.len()
                        ));
                    }
                    Err(e) => self.log(e.to_string()),
                }
            }
        }
        ui.separator();
        ui.colored_label(Color32::YELLOW,"Bulk hardware restore is deliberately unavailable until model-specific write/export behavior has been hardware-verified. ALB round-trip and patch recovery are available.");
    }
    fn connection(&mut self, ui: &mut egui::Ui) {
        ui.heading("Preset protection");
        ui.label("Permissions start locked every time you launch. Connecting and reading do not select or overwrite saved channels. Editor-session setup/cleanup still sends mode commands.");
        let mut access = self.write_gate.access();
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut access, Access::ReadOnly, "Protected · reads only");
            ui.selectable_value(
                &mut access,
                Access::EditBuffer,
                "Allow temporary edit-buffer changes",
            );
        });
        if access != self.write_gate.access() {
            self.disarm();
            self.write_gate.set(access);
        }
        if access == Access::Persistent {
            ui.colored_label(
                Color32::LIGHT_RED,
                "Saved preset / system writes are UNLOCKED. Click Protected to lock them again.",
            );
        } else if ui.button("Unlock saved preset / system writes…").clicked() {
            self.confirm_text.clear();
            self.confirm = Some("unlock".into());
        }
        ui.small("Temporary permission allows current-sound parameter edits, preview and explicit channel selection. It blocks saved-channel commits, system writes and export/reset commands at the transport boundary.");
        ui.separator();
        if self.connected.is_none() {
            ui.horizontal(|ui| {
                ui.label("Offline model preview:");
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
        }
        ui.heading("Linux ALSA USB-MIDI");
        ui.label("Connect the Katana by USB. Select the control MIDI port, not an audio PCM port. Rusty Boss never auto-connects or writes on startup.");
        #[cfg(feature = "midi")]
        {
            if ui.button("Refresh MIDI ports").clicked() {
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
            if self.ports.is_empty() {
                ui.label("No ports listed yet. Click Refresh MIDI ports to enumerate ALSA ports; startup performs no MIDI enumeration or connection. Check kernel driver and /dev/snd permissions if the list stays empty. Do not run as root.");
            }
            ui.horizontal(|ui| {
                ui.label("ALSA address:");
                ui.text_edit_singleline(&mut self.port);
                if ui
                    .add_enabled(
                        !self.port.is_empty() && !self.busy && self.connected.is_none(),
                        egui::Button::new("Connect"),
                    )
                    .clicked()
                {
                    self.submit(Job::Connect(self.port.clone()));
                }
                if ui
                    .add_enabled(
                        self.connected.is_some() && !self.busy,
                        egui::Button::new("Disconnect"),
                    )
                    .clicked()
                {
                    self.submit(Job::Disconnect);
                }
            });
        }
        #[cfg(not(feature = "midi"))]
        ui.label("This build has no MIDI backend. Build with --features midi.");
        ui.separator();
        ui.label("Connect sends Universal Identity Request, reads communication level, then enables editor mode only for a recognized Gen 3 model. Disconnect disables editor mode. Audio DSP runs in the amplifier, not in this editor.");
        ui.colored_label(Color32::YELLOW,"EXPERIMENTAL: the user reports the initial implementation works with their amp, but model/firmware, full write paths and vendor-file interoperability are not independently verified. Back up with Tone Studio before enabling saved writes.");
    }
    fn confirmation(&mut self, ctx: &egui::Context) {
        let Some(action) = self.confirm.clone() else {
            return;
        };
        let mut yes = false;
        let mut no = false;
        egui::Window::new("Confirm operation").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER,[0.,0.]).show(ctx,|ui|{
            let message=match action.as_str(){
                "import"=>"Replace the current local liveset with the selected TSL file? Unsaved edits will be lost.",
                "save"=>"Save the local liveset to the chosen TSL path? An existing file will be atomically replaced.",
                "delete"=>"Delete this local patch? This cannot be undone.",
                "read-current"=>"Replace the selected LOCAL patch with the amplifier's edit buffer? Unsaved local edits to that patch will be lost.",
                "read-system"=>"Replace local system values with a fresh hardware read?",
                "preview"=>"Send the entire local patch to the amplifier edit buffer? This changes the current sound and discards unsaved edits on the amp. Stored slots are not committed.",
                "select"=>"Change the amplifier channel and replace the current local patch with its edit buffer? This can discard unsaved edits.",
                "commit"=>"OVERWRITE the selected hardware channel with the amplifier's CURRENT edit buffer? This is persistent and cannot be undone. Have you read back and verified the sound?",
                "save-backup"=>"Write the loaded/read backup to the chosen ALB path? An existing file will be replaced.",
                "unlock"=>"Allow writes to saved amplifier presets and system settings? This removes preset protection. Keep this locked unless you have a verified backup.",_=>"Proceed?",
            };
            ui.set_max_width(490.);ui.label(message);
            let phrase=if action=="unlock" {Some("ALLOW SAVED WRITES".to_owned())}else if action=="commit"{Some(slot_label(self.slot))}else{None};
            if let Some(expected)=&phrase{ui.colored_label(Color32::YELLOW,format!("Type {expected} to confirm:"));ui.text_edit_singleline(&mut self.confirm_text);}
            ui.horizontal(|ui|{yes=ui.add_enabled(phrase.as_ref().is_none_or(|p|self.confirm_text==*p),egui::Button::new("Confirm")).clicked();no=ui.button("Cancel").clicked();});
        });
        if no {
            self.confirm = None;
        }
        if !yes {
            return;
        }
        self.confirm = None;
        match action.as_str() {
            "unlock" => {
                self.disarm();
                self.write_gate.set(Access::Persistent);
                self.confirm_text.clear();
                self.log("WARNING: persistent writes unlocked for this session");
            }
            "import" => match LiveSet::load(&PathBuf::from(&self.file), &self.schema) {
                Ok((s, w)) => {
                    self.library = s;
                    self.selected = 0;
                    self.dirty = false;
                    self.disarm();
                    self.log(format!(
                        "Imported TSL ({} default-fill warnings). Hardware unchanged.",
                        w.len()
                    ));
                }
                Err(e) => self.log(format!("{e:#}")),
            },
            "save" => match self.library.save(&PathBuf::from(&self.file)) {
                Ok(()) => {
                    self.dirty = false;
                    self.log("TSL saved");
                }
                Err(e) => self.log(format!("{e:#}")),
            },
            "delete" => {
                self.library.data[0].remove(self.selected);
                self.selected = self
                    .selected
                    .min(self.library.data[0].len().saturating_sub(1));
                self.dirty = true;
                self.disarm();
            }
            "save-backup" => {
                if let Some(b) = &self.backup {
                    let r = b.save(&PathBuf::from(&self.backup_file));
                    match r {
                        Ok(()) => self.log("ALB saved"),
                        Err(e) => self.log(format!("{e:#}")),
                    }
                }
            }
            #[cfg(feature = "midi")]
            "read-current" => {
                self.disarm();
                self.submit(Job::ReadCurrent);
            }
            #[cfg(feature = "midi")]
            "read-system" => self.submit(Job::ReadSystem),
            #[cfg(feature = "midi")]
            "preview" => {
                self.disarm();
                if let Some(p) = self.library.data[0].get(self.selected).cloned() {
                    self.submit(Job::Preview(p));
                }
            }
            #[cfg(feature = "midi")]
            "select" => {
                self.disarm();
                self.submit(Job::Select(self.slot));
            }
            #[cfg(feature = "midi")]
            "commit" => {
                self.disarm();
                self.submit(Job::Commit(self.slot));
            }
            _ => {}
        }
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(feature = "midi")]
        self.events();
        self.header(ctx);
        if self.page == Page::Editor || self.page == Page::System {
            self.editor(ctx, self.page == Page::System);
        } else {
            egui::CentralPanel::default().show(ctx,|ui|{egui::ScrollArea::vertical().show(ui,|ui|{match self.page{
                Page::Librarian=>{ui.add_enabled_ui(!self.busy, |ui|self.librarian(ui));},Page::Backup=>{ui.add_enabled_ui(!self.busy, |ui|self.backup_page(ui));},Page::Connection=>self.connection(ui),
                Page::About=>{
                    ui.heading("Rusty Boss 0.1.0");ui.label("Independent Rust + egui reconstruction. Not affiliated with Roland or BOSS.");
                    ui.label("Recovered from BOSS TONE STUDIO for KATANA Gen 3 1.1.0 (26).");
                    ui.label("106 parameter blocks • 2,463 parameter instances • Linux-first ALSA MIDI");
                    ui.label("This is a working foundation, NOT a complete feature-parity release. See README.md and docs/PARITY.md for evidence, implemented behavior and remaining gaps.");
                    ui.separator();ui.label("Implemented: offline parameter editing, named choices, local livesets and notes, Gen 3 TSL import/export, ALB reading/export and patch recovery, protocol codec and experimental MIDI reads/preview/commit.");
                    ui.label("Standard editor: amp controls, algorithm-specific pages, green/red/yellow banks, finite display-unit tables, 9 chain patterns with position selectors, and basic model capabilities. Preset protection is enforced per outgoing packet.");
                    ui.label("Still incomplete: bulk restore, legacy conversion, integrated cloud services, chain dragging, dynamic assignments, every coordinated-control rule and vendor-file interoperability. User-reported initial hardware success is not verification of every path.");
                },_=>{},}});});
        }
        self.confirmation(ctx);
        #[cfg(feature = "midi")]
        self.flush_edits();
        if self.busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(25));
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
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
        // Prevent a window-close from silently discarding local changes.
        if ctx.input(|i| i.viewport().close_requested())
            && self.dirty
            && std::env::var_os("RUSTY_BOSS_SMOKE_TEST").is_none()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_dialog = true;
        }
        if self.close_dialog {
            egui::Window::new("Unsaved liveset").show(ctx, |ui| {
                ui.label("Save the liveset in Librarian before closing, or discard it.");
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn render_every_editable_block_and_empty_library() {
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx);
        app.expert = true;
        let ids: Vec<_> = app
            .schema
            .blocks
            .iter()
            .filter(|b| b.id.starts_with("PATCH%") || b.id.starts_with("SYSTEM%"))
            .map(|b| b.id.clone())
            .collect();
        for id in ids {
            let system = id.starts_with("SYSTEM%");
            app.block = id;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1180., 800.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    app.header(ctx);
                    app.editor(ctx, system);
                },
            );
            assert!(!output.shapes.is_empty());
        }
        app.library.data[0].clear();
        let _ = ctx.run(Default::default(), |ctx| {
            app.header(ctx);
            app.editor(ctx, false);
        });
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                app.librarian(ui);
                app.backup_page(ui);
                app.connection(ui);
            });
        });
        assert!(!app.live && app.connected.is_none());
    }
    fn render_standard(app: &mut App, ctx: &egui::Context) {
        let out = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1180., 800.),
                )),
                ..Default::default()
            },
            |ctx| {
                app.header(ctx);
                app.editor(ctx, false);
            },
        );
        assert!(!out.shapes.is_empty());
    }
    #[test]
    fn standard_pages_are_inert_in_every_algorithm_and_color() {
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx);
        assert_eq!(app.write_gate.access(), Access::ReadOnly);
        #[cfg(feature = "midi")]
        assert!(app.ports.is_empty());
        for section in Section::ALL {
            app.section = section;
            let before = app.library.data[0][0].parameters.clone();
            render_standard(&mut app, &ctx);
            assert_eq!(app.library.data[0][0].parameters, before);
        }
        for model in 5..=11 {
            app.offline_model = model;
            for section in [Section::Amp, Section::Solo] {
                app.section = section;
                let before = app.library.data[0][0].parameters.clone();
                render_standard(&mut app, &ctx);
                assert_eq!(app.library.data[0][0].parameters, before);
            }
        }
        for (section, count) in [
            (Section::Mod, 31),
            (Section::Fx, 31),
            (Section::Delay, 8),
            (Section::Delay2, 8),
            (Section::Reverb, 5),
            (Section::Eq, 2),
            (Section::Eq2, 2),
            (Section::Pedal, 3),
            (Section::Booster, 23),
        ] {
            app.section = section;
            for color in 0..3 {
                if let Some(i) = section.color_index() {
                    app.library.data[0][0]
                        .parameters
                        .get_mut("PATCH%COLOR")
                        .unwrap()[i] = color;
                }
                for value in 0..count {
                    let id = app.selector(section).unwrap();
                    let c = app.remap_controls(section, app.catalog.control(id).unwrap());
                    c.set(&app.schema, &mut app.library.data[0][0].parameters, value)
                        .unwrap();
                    let before = app.library.data[0][0].parameters.clone();
                    render_standard(&mut app, &ctx);
                    assert_eq!(
                        app.library.data[0][0].parameters, before,
                        "{section:?} color {color} value {value}"
                    );
                }
            }
        }
        assert!(app.pending_edits.is_empty());
        assert!(!app.dirty && !app.live && app.connected.is_none());
    }
    #[test]
    fn responsive_standard_layout_keeps_unknown_panel_data_untouched() {
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx);
        app.library.data[0][0]
            .parameters
            .get_mut("PATCH%PATCH_KNOB_READONLY")
            .unwrap()[4] = 127;
        let before = app.library.data[0][0].parameters.clone();
        for (width, height) in [(800., 560.), (1180., 800.), (1600., 1000.)] {
            for section in Section::ALL {
                app.section = section;
                let out = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, height),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        app.header(ctx);
                        app.editor(ctx, false);
                    },
                );
                assert!(!out.shapes.is_empty());
                assert_eq!(app.library.data[0][0].parameters, before);
            }
        }
        assert!(app.pending_edits.is_empty());
        assert!(!app.dirty);
    }
    #[test]
    fn queued_edits_coalesce_by_field_and_disarming_drops_them() {
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx);
        app.write_gate.set(Access::EditBuffer);
        app.live = true;
        for (index, value) in [(0, 10), (1, 20), (0, 30)] {
            app.record_edit(Edit {
                block: "PATCH%AMP".into(),
                index,
                bytes: vec![value],
            });
        }
        assert_eq!(app.pending_edits.len(), 2);
        assert_eq!(app.pending_edits.back().unwrap().bytes, [30]);
        app.disarm();
        assert!(app.pending_edits.is_empty());
        assert!(!app.synced && !app.live);
    }
}
