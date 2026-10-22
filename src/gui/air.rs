//! Air presentation and worker. Gen 3 library bytes never enter the Air session.
use super::*;
use crate::air::{Schema as AirSchema, Snapshot};

pub(super) const MODEL_CODE: u8 = 0x50;

// UI values are labels (for example -10 dB), while the wire may use an offset
// integer. Accept a unique displayed value; never interpret typed units as raw MIDI.
fn parse_display_value(parameter: &crate::air::Parameter, text: &str) -> Option<f64> {
    let numeric = |value: &str| {
        value
            .split_whitespace()
            .next()?
            .trim_end_matches('%')
            .parse::<f64>()
            .ok()
    };
    let mut matches = parameter.options.iter().filter(|(_, label)| {
        label.trim().eq_ignore_ascii_case(text.trim())
            || numeric(text).is_some_and(|number| numeric(label) == Some(number))
    });
    let value = *matches.next()?.0;
    matches.next().is_none().then_some(value as f64)
}

pub(super) struct AirState {
    schema: AirSchema,
    snapshot: Option<Snapshot>,
    draft: BTreeMap<String, u16>,
    section: usize,
    connected: bool,
    busy: bool,
    status: String,
    pending_global: Option<(String, u16)>,
    #[cfg(feature = "bluetooth")]
    devices: Vec<crate::bluetooth::Device>,
    #[cfg(feature = "bluetooth")]
    bluetooth_device: String,
    #[cfg(feature = "midi")]
    ports: Vec<crate::midi::Port>,
    #[cfg(feature = "midi")]
    port: String,
    #[cfg(feature = "midi")]
    worker: AirWorker,
}
impl AirState {
    pub(super) fn new(gate: WriteGate) -> Self {
        #[cfg(not(feature = "midi"))]
        let _ = gate;
        Self {
            schema: AirSchema::bundled().expect("embedded Air definitions"),
            snapshot: None,
            draft: BTreeMap::new(),
            section: 0,
            connected: false,
            busy: false,
            status: "Offline · Connect to read the Air's current tone".into(),
            pending_global: None,
            #[cfg(feature = "bluetooth")]
            devices: Vec::new(),
            #[cfg(feature = "bluetooth")]
            bluetooth_device: String::new(),
            #[cfg(feature = "midi")]
            ports: Vec::new(),
            #[cfg(feature = "midi")]
            port: String::new(),
            #[cfg(feature = "midi")]
            worker: AirWorker::new(gate),
        }
    }
    pub(super) fn is_connected(&self) -> bool {
        self.connected
    }
    pub(super) fn cancel_operation(&self) {
        #[cfg(feature = "midi")]
        if self.busy {
            self.worker.cancel.store(true, Ordering::Relaxed);
        }
    }
    pub(super) fn clear_draft_confirmation(&mut self) {
        self.pending_global = None;
        if let Some(snapshot) = &self.snapshot {
            self.draft = snapshot.values.clone();
        }
    }
    #[cfg(feature = "midi")]
    fn submit(&mut self, job: AirJob) {
        if self.busy {
            return;
        }
        self.worker.cancel.store(false, Ordering::Relaxed);
        let status = match &job {
            #[cfg(feature = "bluetooth")]
            AirJob::ScanBluetooth => "Searching for nearby Bluetooth MIDI devices…",
            #[cfg(feature = "bluetooth")]
            AirJob::PairBluetooth(_) => "Pairing… Respond to any desktop Bluetooth prompt",
            #[cfg(feature = "bluetooth")]
            AirJob::ConnectBluetooth(_) => "Connecting Bluetooth MIDI and reading the Air…",
            _ => "Reading / verifying Air settings…",
        };
        if self.worker.tx.send(job).is_ok() {
            self.busy = true;
            self.status = status.into();
        }
    }
    #[cfg(feature = "midi")]
    fn events(&mut self) {
        while let Ok(event) = self.worker.rx.try_recv() {
            self.busy = false;
            match event {
                #[cfg(feature = "bluetooth")]
                AirEvent::Devices(devices) => {
                    if !devices.iter().any(|d| d.id == self.bluetooth_device) {
                        self.bluetooth_device =
                            devices.first().map(|d| d.id.clone()).unwrap_or_default();
                    }
                    self.status = if devices.is_empty() {
                        "No Bluetooth MIDI devices found. Turn on the amp and Bluetooth, close other editors, then search again.".into()
                    } else {
                        "Choose your amplifier, then Connect Air".into()
                    };
                    self.devices = devices;
                }
                #[cfg(feature = "bluetooth")]
                AirEvent::Paired(id) => {
                    if let Some(device) = self.devices.iter_mut().find(|d| d.id == id) {
                        device.paired = true;
                    }
                    self.status = "Device paired · Connect Air to read its settings".into();
                }
                AirEvent::Snapshot(snapshot) => {
                    self.connected = true;
                    self.draft = snapshot.values.clone();
                    self.status = if snapshot.warnings.is_empty() {
                        "Air settings read · Refresh after changing physical controls".into()
                    } else {
                        format!("Air tone read · {}", snapshot.warnings.join(" · "))
                    };
                    self.snapshot = Some(snapshot);
                }
                AirEvent::Disconnected => {
                    self.connected = false;
                    self.snapshot = None;
                    self.draft.clear();
                    self.status = "Disconnected".into();
                }
                AirEvent::Error(error) => {
                    self.connected = false;
                    self.pending_global = None;
                    self.snapshot = None;
                    self.draft.clear();
                    self.status = error;
                }
            }
        }
    }
}

#[cfg(feature = "midi")]
enum AirJob {
    Connect(String),
    #[cfg(feature = "bluetooth")]
    ScanBluetooth,
    #[cfg(feature = "bluetooth")]
    ConnectBluetooth(crate::bluetooth::Device),
    #[cfg(feature = "bluetooth")]
    PairBluetooth(crate::bluetooth::Device),
    Disconnect,
    Refresh,
    Set(String, u16),
    Select(usize),
    Shutdown,
}
#[cfg(feature = "midi")]
enum AirEvent {
    Snapshot(Snapshot),
    #[cfg(feature = "bluetooth")]
    Devices(Vec<crate::bluetooth::Device>),
    #[cfg(feature = "bluetooth")]
    Paired(String),
    Disconnected,
    Error(String),
}
#[cfg(feature = "midi")]
struct AirWorker {
    tx: Sender<AirJob>,
    rx: Receiver<AirEvent>,
    cancel: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}
#[cfg(feature = "midi")]
impl AirWorker {
    fn new(gate: WriteGate) -> Self {
        let (tx, jobs) = mpsc::channel();
        let (events, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let handle = thread::spawn(move || {
            use crate::{air::AirSession, midi::SequencerTransport, session::Transport};
            let mut session: Option<AirSession<Box<dyn Transport>>> = None;
            loop {
                let job = match jobs.recv_timeout(Duration::from_millis(200)) {
                    Ok(job) => job,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if session.as_ref().is_some_and(|s| !s.is_connected()) {
                            gate.set(Access::ReadOnly);
                            session.take();
                            let _ = events.send(AirEvent::Error("Bluetooth connection lost · Connect again to read the current sound".into()));
                        }
                        continue;
                    }
                };
                let shutdown = matches!(job, AirJob::Shutdown);
                let result = (|| -> anyhow::Result<AirEvent> {
                    match job {
                        AirJob::Shutdown => Ok(AirEvent::Disconnected),
                        AirJob::Disconnect => {
                            session.take();
                            Ok(AirEvent::Disconnected)
                        }
                        AirJob::Connect(port) => {
                            session.take();
                            let transport = SequencerTransport::open(&port, stop.clone())?;
                            let mut s = AirSession::connect(
                                Box::new(transport) as Box<dyn Transport>,
                                gate.clone(),
                            )?;
                            let snapshot = s.refresh()?;
                            session = Some(s);
                            Ok(AirEvent::Snapshot(snapshot))
                        }
                        #[cfg(feature = "bluetooth")]
                        AirJob::ScanBluetooth => {
                            Ok(AirEvent::Devices(crate::bluetooth::discover(stop.clone())?))
                        }
                        #[cfg(feature = "bluetooth")]
                        AirJob::PairBluetooth(device) => {
                            crate::bluetooth::pair(&device, stop.clone())?;
                            Ok(AirEvent::Paired(device.id))
                        }
                        #[cfg(feature = "bluetooth")]
                        AirJob::ConnectBluetooth(device) => {
                            session.take();
                            let transport =
                                crate::bluetooth::BluetoothTransport::open(&device, stop.clone())?;
                            let mut s = AirSession::connect(
                                Box::new(transport) as Box<dyn Transport>,
                                gate.clone(),
                            )?;
                            let snapshot = s.refresh()?;
                            session = Some(s);
                            Ok(AirEvent::Snapshot(snapshot))
                        }
                        AirJob::Refresh => Ok(AirEvent::Snapshot(
                            session
                                .as_mut()
                                .ok_or_else(|| anyhow::anyhow!("Connect the Air first"))?
                                .refresh()?,
                        )),
                        AirJob::Set(id, value) => Ok(AirEvent::Snapshot(
                            session
                                .as_mut()
                                .ok_or_else(|| anyhow::anyhow!("Connect the Air first"))?
                                .set_parameter(&id, value)?,
                        )),
                        AirJob::Select(slot) => Ok(AirEvent::Snapshot(
                            session
                                .as_mut()
                                .ok_or_else(|| anyhow::anyhow!("Connect the Air first"))?
                                .select_channel(slot)?,
                        )),
                    }
                })();
                let event = result.unwrap_or_else(|e| {
                    gate.set(Access::ReadOnly);
                    session.take();
                    AirEvent::Error(format!("{e:#}"))
                });
                if events.send(event).is_err() || shutdown {
                    break;
                }
            }
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
impl Drop for AirWorker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        let _ = self.tx.send(AirJob::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl App {
    pub(super) fn is_air(&self) -> bool {
        self.connected.is_none() && self.offline_model == MODEL_CODE
    }
    pub(super) fn select_amp_profile(&mut self, code: u8) {
        if self.connected.is_some()
            || self.busy
            || self.air.connected
            || self.air.busy
            || !((5..=11).contains(&code) || code == MODEL_CODE)
        {
            return;
        }
        if self.offline_model == code {
            return;
        }
        self.set_access(Access::ReadOnly);
        self.offline_model = code;
        self.section = Section::Amp;
        self.system = None;
        self.confirm = None;
        self.log(if self.is_air() {
            "Katana Air selected · Experimental support"
        } else {
            "Gen 3 selected · Read-only"
        });
    }
    pub(super) fn amp_selector(&mut self, ui: &mut egui::Ui) {
        let mut chosen = self.offline_model;
        ui.label("Amplifier");
        ui.add_enabled_ui(
            self.connected.is_none() && !self.busy && !self.air.connected && !self.air.busy,
            |ui| {
                egui::ComboBox::from_id_salt("amp-profile")
                    .selected_text(if self.is_air() {
                        "KATANA-AIR · Experimental".into()
                    } else {
                        format!("{} Gen 3", crate::editor::model_name(self.model_code()))
                    })
                    .show_ui(ui, |ui| {
                        for code in 5..=11 {
                            ui.selectable_value(
                                &mut chosen,
                                code,
                                format!("{} Gen 3", crate::editor::model_name(code)),
                            );
                        }
                        ui.selectable_value(&mut chosen, MODEL_CODE, "KATANA-AIR · Experimental");
                    });
            },
        );
        self.select_amp_profile(chosen);
    }
    pub(super) fn air_page(&mut self, ui: &mut egui::Ui) {
        #[cfg(feature = "midi")]
        self.air.events();
        if self.page == Page::Editor {
            egui::Panel::left("air-channels").default_size(210.).show(ui, |ui| {
                ui.heading("Air tone settings");
                ui.small("CH A / CH B · three colours each");
                for (slot, label) in crate::air::CHANNEL_LABELS.iter().enumerate() {
                    let selected = self.air.snapshot.as_ref().and_then(|s| s.selected_channel) == Some(slot);
                    let can_recall = self.air.connected && !self.air.busy && self.write_gate.access() != Access::ReadOnly;
                    let clicked = ui.add_enabled(can_recall, egui::Button::new(*label).selected(selected).min_size(egui::vec2(ui.available_width(), 34.)))
                        .on_hover_text("Experimental Air tone recall · changes the current sound; saved tones remain unchanged")
                        .on_disabled_hover_text("Connect and enable Preview & edit to recall a tone.").clicked();
                    #[cfg(feature = "midi")]
                    if clicked { self.air.submit(AirJob::Select(slot)); }
                    #[cfg(not(feature = "midi"))]
                    let _ = (clicked, slot);
                }
                ui.separator();
                ui.label("Choose Preview & edit to recall a tone. Refresh after changing tones on the amp.");
                ui.small("Saved-tone writes are not yet supported for Air.");
            });
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("KATANA-AIR");
            ui.label(RichText::new("Experimental · hardware validation pending").color(self.theme.accent()));
            if self.air.busy { ui.spinner(); }
            ui.label(&self.air.status);
            ui.separator();
            match self.page {
                Page::Settings => { egui::ScrollArea::vertical().id_salt("air-settings").show(ui, |ui| self.air_connection(ui)); },
                Page::Library => {
                    ui.heading("Air tones");
                    ui.label("The Air editor works with the current amp sound. Gen 3 TSL collections and backups use a different format and remain in the Gen 3 library.");
                    ui.label("Air file import/export and saved-tone writes are not yet supported.");
                }
                Page::Editor => {
                    #[cfg(feature = "midi")]
                    if ui.add_enabled(self.air.connected && !self.air.busy, egui::Button::new("Refresh from amp")).clicked() { self.air.submit(AirJob::Refresh); }
                    ui.small("Changes apply to the current sound after a completed gesture and are read back for verification. Refresh after turning physical knobs or changing tones.");
                    let blocks = self.air.schema.blocks.clone();
                    ui.horizontal_wrapped(|ui| {
                        for (i, block) in blocks.iter().enumerate().filter(|(_, b)| b.id != "global") {
                            ui.selectable_value(&mut self.air.section, i, &block.title);
                        }
                    });
                    ui.separator();
                    egui::ScrollArea::vertical().id_salt("air-editor").show(ui, |ui| {
                        if let Some(block) = blocks.get(self.air.section).filter(|b| b.id != "global") {
                            ui.heading(&block.title);
                            let mut controls = block.common.clone();
                            if let Some(value) = block.selector.as_ref().and_then(|id| self.air.snapshot.as_ref()?.values.get(id))
                                && let Some(index) = block.variant_values.iter().position(|v| v == value)
                                && let Some(variant) = block.variants.get(index)
                            { controls.extend(variant.iter().cloned()); }
                            let mut seen = BTreeSet::new();
                            for id in controls { if seen.insert(id.clone()) { self.air_control(ui, &id); } }
                        }
                    });
                }
            }
        });
        if let Some((id, value)) = self.air.pending_global.clone() {
            egui::Modal::new(egui::Id::unique("air-global-confirm")).show(ui.ctx(), |ui| {
                ui.heading("Apply Air setting?");
                if let Some(parameter) = self.air.schema.parameters.get(&id) {
                    ui.label(format!(
                        "{}: {}",
                        parameter.label,
                        parameter.options.get(&value).cloned().unwrap_or_default()
                    ));
                }
                ui.label("This changes the amplifier's settings across tones.");
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.air.pending_global = None;
                        if let Some(snapshot) = &self.air.snapshot {
                            self.air.draft = snapshot.values.clone();
                        }
                    }
                    if ui
                        .add_enabled(
                            self.air.connected
                                && !self.air.busy
                                && self.write_gate.access() == Access::Persistent,
                            egui::Button::new("Apply to Air"),
                        )
                        .clicked()
                    {
                        self.air.pending_global = None;
                        #[cfg(feature = "midi")]
                        self.air.submit(AirJob::Set(id.clone(), value));
                    }
                });
            });
        }
    }
    fn air_connection(&mut self, ui: &mut egui::Ui) {
        ui.label("Turn on your amplifier and Bluetooth on this computer. Find your amp's MIDI connection below, then Connect Air. Close phone or desktop editors that are already using it.");
        #[cfg(feature = "bluetooth")]
        {
            if ui
                .add_enabled(
                    !self.air.busy && !self.air.connected,
                    egui::Button::new("Find Bluetooth amplifiers"),
                )
                .clicked()
            {
                self.air.submit(AirJob::ScanBluetooth);
            }
            for device in &self.air.devices {
                ui.add_enabled_ui(!self.air.connected && !self.air.busy, |ui| {
                    ui.radio_value(
                        &mut self.air.bluetooth_device,
                        device.id.clone(),
                        format!(
                            "{} · {}{}",
                            device.name,
                            device.address,
                            if device.paired { " · Paired" } else { "" }
                        ),
                    );
                });
            }
            let chosen = self
                .air
                .devices
                .iter()
                .find(|d| d.id == self.air.bluetooth_device)
                .cloned();
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!self.air.busy && !self.air.connected && chosen.is_some(), egui::Button::new("Connect Air")).clicked() && let Some(device) = chosen.clone() {
                    self.set_access(Access::ReadOnly);
                    self.air.submit(AirJob::ConnectBluetooth(device));
                }
                if ui.add_enabled(!self.air.busy && !self.air.connected && chosen.as_ref().is_some_and(|d| !d.paired), egui::Button::new("Pair device…")).on_hover_text("Use if the device requires pairing. Respond to the normal desktop Bluetooth prompt, then connect.").clicked() && let Some(device) = chosen {
                    self.air.submit(AirJob::PairBluetooth(device));
                }
            });
            ui.small(
                "Bluetooth connects directly through the app. No separate MIDI bridge is needed.",
            );
        }
        #[cfg(all(feature = "midi", not(feature = "bluetooth")))]
        ui.label("This build has no native Bluetooth support. Use an existing ALSA MIDI connection below, or rebuild with --features bluetooth.");
        #[cfg(feature = "bluetooth")]
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.air.connected && !self.air.busy,
                    egui::Button::new("Disconnect Air"),
                )
                .clicked()
            {
                self.set_access(Access::ReadOnly);
                self.air.submit(AirJob::Disconnect);
            }
            if ui
                .add_enabled(
                    self.air.connected && !self.air.busy,
                    egui::Button::new("Read Air settings"),
                )
                .clicked()
            {
                self.air.submit(AirJob::Refresh);
            }
            if self.air.busy && ui.button("Cancel Air operation").clicked() {
                self.write_gate.set(Access::ReadOnly);
                self.air.worker.cancel.store(true, Ordering::Relaxed);
            }
        });
        #[cfg(feature = "midi")]
        egui::CollapsingHeader::new("Advanced: existing ALSA MIDI port").show(ui, |ui| {
            if ui
                .add_enabled(!self.air.busy, egui::Button::new("Refresh Air MIDI ports"))
                .clicked()
            {
                match crate::midi::sequencer_ports() {
                    Ok(ports) => self.air.ports = ports,
                    Err(e) => self.air.status = format!("{e:#}"),
                }
            }
            for port in &self.air.ports {
                ui.add_enabled_ui(!self.air.connected && !self.air.busy, |ui| {
                    ui.radio_value(
                        &mut self.air.port,
                        port.address.clone(),
                        format!("{} ({})", port.name, port.address),
                    );
                });
            }
            ui.horizontal_wrapped(|ui| {
                ui.add_enabled(
                    !self.air.connected && !self.air.busy,
                    egui::TextEdit::singleline(&mut self.air.port).hint_text("seq:client:port"),
                );
                if self.air.connected {
                    if ui
                        .add_enabled(!self.air.busy, egui::Button::new("Disconnect Air"))
                        .clicked()
                    {
                        self.set_access(Access::ReadOnly);
                        self.air.submit(AirJob::Disconnect);
                    }
                } else if ui
                    .add_enabled(
                        !self.air.busy && !self.air.port.is_empty(),
                        egui::Button::new("Connect via ALSA"),
                    )
                    .clicked()
                {
                    self.set_access(Access::ReadOnly);
                    self.air.submit(AirJob::Connect(self.air.port.clone()));
                }
                if self.air.busy && ui.button("Cancel Air operation").clicked() {
                    self.write_gate.set(Access::ReadOnly);
                    self.air.worker.cancel.store(true, Ordering::Relaxed);
                }
                if ui
                    .add_enabled(
                        self.air.connected && !self.air.busy,
                        egui::Button::new("Read Air settings"),
                    )
                    .clicked()
                {
                    self.air.submit(AirJob::Refresh);
                }
            });
        });
        #[cfg(not(feature = "midi"))]
        ui.label("This build is offline-only (no MIDI support).");
        ui.small("After the Bluetooth connection is established, Air connect sends only identity and parameter reads. Nothing connects automatically.");
        ui.separator();
        if let Some(block) = self
            .air
            .schema
            .blocks
            .iter()
            .find(|b| b.id == "global")
            .cloned()
        {
            egui::CollapsingHeader::new(&block.title).show(ui, |ui| {
                ui.small("These settings apply across tones and require enabled amp writes. Each change asks for confirmation.");
                for id in block.common { self.air_control(ui, &id); }
            });
        }
        ui.separator();
        ui.heading("Appearance");
        ui.horizontal_wrapped(|ui| {
            for theme in Theme::ALL {
                if ui
                    .selectable_label(self.theme == theme, theme.name())
                    .clicked()
                {
                    self.theme = theme;
                    theme.apply(ui.ctx());
                    if let Some(path) = Theme::preferences_path()
                        && let Err(e) = theme.save(&path)
                    {
                        self.log(e.to_string());
                    }
                }
            }
        });
    }
    fn air_control(&mut self, ui: &mut egui::Ui, id: &str) {
        let Some(parameter) = self.air.schema.parameters.get(id).cloned() else {
            return;
        };
        let value = self
            .air
            .snapshot
            .as_ref()
            .and_then(|s| s.values.get(id))
            .copied();
        let Some(old) = value.filter(|v| parameter.options.contains_key(v)) else {
            ui.horizontal(|ui| {
                ui.label(&parameter.label);
                ui.weak(if value.is_some() {
                    "Unrecognized value · editing unavailable"
                } else {
                    "Connect and refresh to read"
                });
            });
            return;
        };
        let global = parameter.address[0] == 0;
        let editable = self.air.connected
            && !self.air.busy
            && if global {
                self.write_gate.access() == Access::Persistent
            } else {
                self.write_gate.access() != Access::ReadOnly
            };
        let value = self.air.draft.entry(id.into()).or_insert(old);
        let mut complete = false;
        ui.push_id(id, |ui| {
            ui.add_enabled_ui(editable, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.add_sized([160., 24.], egui::Label::new(&parameter.label));
                    if parameter.numeric {
                        let min = *parameter.options.keys().next().unwrap();
                        let max = *parameter.options.keys().next_back().unwrap();
                        let response = ui.add(
                            egui::Slider::new(value, min..=max)
                                .show_value(false)
                                .clamping(egui::SliderClamping::Never),
                        );
                        complete |=
                            response.drag_stopped() || (response.changed() && !response.dragged());
                        let response = ui.add(
                            egui::DragValue::new(value)
                                .update_while_editing(false)
                                .range(min..=max)
                                .clamp_existing_to_range(false)
                                .custom_parser(|text| parse_display_value(&parameter, text))
                                .custom_formatter(|v, _| {
                                    parameter
                                        .options
                                        .get(&(v as u16))
                                        .cloned()
                                        .unwrap_or_else(|| v.to_string())
                                }),
                        );
                        complete |= response.drag_stopped()
                            || response.lost_focus()
                            || (response.changed() && !response.dragged());
                    } else {
                        egui::ComboBox::from_id_salt("air-value")
                            .selected_text(
                                parameter.options.get(value).cloned().unwrap_or_default(),
                            )
                            .show_ui(ui, |ui| {
                                for (option, label) in &parameter.options {
                                    complete |=
                                        ui.selectable_value(value, *option, label).changed();
                                }
                            });
                    }
                });
            });
        });
        let chosen = *value;
        #[cfg(feature = "midi")]
        if complete && editable && chosen != old && parameter.options.contains_key(&chosen) {
            if global {
                self.air.pending_global = Some((id.into(), chosen));
            } else {
                self.air.submit(AirJob::Set(id.into(), chosen));
            }
        }
        #[cfg(not(feature = "midi"))]
        let _ = (chosen, complete);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn render(app: &mut App, ctx: &egui::Context, width: f32) -> egui::FullOutput {
        super::super::tests::run_ui(
            ctx,
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 850.),
                )),
                ..Default::default()
            },
            |ui| {
                app.header(ui);
                app.air_page(ui);
            },
        )
    }
    fn text(output: &egui::FullOutput) -> String {
        output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t.galley.text()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    #[test]
    fn typed_values_use_displayed_units_and_bias() {
        let schema = AirSchema::bundled().unwrap();
        let bottom = &schema.parameters["60000033"];
        assert_eq!(parse_display_value(bottom, "-10"), Some(40.));
        assert_eq!(parse_display_value(bottom, "0"), Some(50.));
        let time = &schema.parameters["60000562"];
        assert_eq!(parse_display_value(time, "250 ms"), Some(250.));
        let drive = &schema.parameters["60000032"];
        assert_eq!(parse_display_value(drive, "0"), Some(20.));
        assert_eq!(parse_display_value(drive, "0-25"), Some(0.));
        assert_eq!(parse_display_value(bottom, "not a number"), None);
    }
    #[cfg(feature = "midi")]
    #[test]
    fn locking_cancels_an_air_write_waiting_for_transport() {
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx);
        app.select_amp_profile(MODEL_CODE);
        app.air.busy = true;
        app.write_gate.set(Access::Persistent);
        app.air.worker.cancel.store(false, Ordering::Relaxed);
        app.set_access(Access::ReadOnly);
        assert!(app.air.worker.cancel.load(Ordering::Relaxed));
        assert_eq!(app.write_gate.access(), Access::ReadOnly);
    }
    #[test]
    fn profile_switch_revokes_permissions_and_keeps_gen3_tones_separate() {
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx);
        let tones = app.library.data[0][0].parameters.clone();
        app.write_gate.set(Access::Persistent);
        app.live = true;
        app.select_amp_profile(MODEL_CODE);
        assert!(app.is_air());
        assert!(!app.live && app.pending_edits.is_empty());
        assert_eq!(app.write_gate.access(), Access::ReadOnly);
        app.air.connected = true;
        app.select_amp_profile(5);
        assert!(app.is_air(), "connected device cannot be silently changed");
        app.air.connected = false;
        app.select_amp_profile(5);
        assert!(!app.is_air());
        assert_eq!(app.library.data[0][0].parameters, tones);
    }
    #[test]
    fn air_layout_has_six_memories_and_only_air_controls() {
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx);
        app.select_amp_profile(MODEL_CODE);
        for width in [800., 1180.] {
            let output = render(&mut app, &ctx, width);
            let labels = text(&output);
            for channel in crate::air::CHANNEL_LABELS {
                assert!(labels.contains(channel), "missing {channel}");
            }
            for unavailable in [
                "A4",
                "B4",
                "VARIATION",
                "BLOOM",
                "CONTOUR",
                "GA-FC",
                "Send/Return",
            ] {
                assert!(
                    !labels.contains(unavailable),
                    "Gen 3 control leaked into Air: {unavailable}"
                );
            }
            assert!(labels.contains("Delay 2") && labels.contains("Experimental"));
        }
        assert!(!app.air.connected && !app.air.busy);
        assert_eq!(app.write_gate.access(), Access::ReadOnly);
    }
    #[test]
    fn browsing_every_air_algorithm_is_inert_and_settings_use_the_air_map() {
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx);
        app.select_amp_profile(MODEL_CODE);
        let tones = app.library.data[0][0].parameters.clone();
        let mut snapshot = Snapshot {
            values: app
                .air
                .schema
                .parameters
                .iter()
                .map(|(id, p)| (id.clone(), *p.options.keys().next().unwrap()))
                .collect(),
            ..Default::default()
        };
        for (section, block) in app.air.schema.blocks.clone().iter().enumerate() {
            app.air.section = section;
            for value in block
                .variant_values
                .iter()
                .copied()
                .chain(std::iter::once(0))
            {
                if let Some(id) = &block.selector {
                    snapshot.values.insert(id.clone(), value);
                }
                app.air.snapshot = Some(snapshot.clone());
                app.air.draft = snapshot.values.clone();
                assert!(!render(&mut app, &ctx, 1180.).shapes.is_empty());
                assert_eq!(app.air.snapshot.as_ref().unwrap().values, snapshot.values);
            }
        }
        app.page = Page::Settings;
        let labels = text(&render(&mut app, &ctx, 1180.));
        assert!(labels.contains("Cabinet & global EQ"));
        assert!(!labels.contains("GA-FC") && !labels.contains("Power amp in"));
        assert_eq!(app.library.data[0][0].parameters, tones);
        assert_eq!(app.write_gate.access(), Access::ReadOnly);
        assert!(app.pending_edits.is_empty() && !app.air.busy);
    }
}
