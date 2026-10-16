//! Tone Studio-style presentation. No I/O: all changes go through App's edit queue.
use super::*;
use crate::editor::{self, Control, Edit, Section};
use crate::safety::Access;
use egui::{vec2, Sense, Stroke};
const ORANGE: Color32 = Color32::from_rgb(231, 144, 65);
const BANKS: [Color32; 3] = [
    Color32::from_rgb(66, 195, 119),
    Color32::from_rgb(232, 90, 86),
    Color32::from_rgb(238, 199, 69),
];

/// Integer rotary control with keyboard, drag and direct numeric entry. Rendering
/// must never normalize/clamp an imported value unless the user actually edits it.
fn rotary(
    ui: &mut egui::Ui,
    c: &Control,
    value: i32,
    min: i32,
    max: i32,
    accent: Color32,
) -> (i32, bool, bool) {
    let mut value = value;
    let mut changed = false;
    let mut finished = false;
    ui.vertical(|ui| {
        ui.set_width(122.);
        ui.add_sized(
            [122., 30.],
            egui::Label::new(RichText::new(&c.label).size(11.).strong()).wrap(),
        );
        let (rect, mut r) = ui.allocate_exact_size(vec2(122., 62.), Sense::click_and_drag());
        r.widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), value as f64, &c.label));
        if r.clicked() {
            r.request_focus();
        }
        if r.drag_started() {
            ui.data_mut(|d| d.insert_temp(r.id.with("fraction"), value as f32));
        }
        if r.dragged() {
            let delta = ui.input(|i| -i.pointer.delta().y + i.pointer.delta().x * 0.25);
            let mut v = ui
                .data(|d| d.get_temp::<f32>(r.id.with("fraction")))
                .unwrap_or(value as f32);
            v = (v + delta * (max - min).max(1) as f32 / 170.).clamp(min as f32, max as f32);
            ui.data_mut(|d| d.insert_temp(r.id.with("fraction"), v));
            let n = v.round() as i32;
            changed = n != value;
            value = n;
        }
        if r.has_focus() {
            let step = ui.input(|i| {
                if i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::ArrowRight) {
                    1
                } else if i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::ArrowLeft)
                {
                    -1
                } else {
                    0
                }
            });
            if step != 0 {
                value = (value + step).clamp(min, max);
                changed = true;
                finished = true;
            }
        }
        if changed {
            r.mark_changed();
        }
        finished |= r.drag_stopped();
        let center = rect.center();
        let painter = ui.painter();
        let color = if ui.is_enabled() {
            accent
        } else {
            Color32::from_gray(85)
        };
        let fraction = ((value - min) as f32 / (max - min).max(1) as f32).clamp(0., 1.);
        let start = std::f32::consts::PI * 0.75;
        let sweep = std::f32::consts::PI * 1.5;
        let arc = |end: f32| -> Vec<egui::Pos2> {
            (0..=48)
                .map(|i| {
                    center
                        + vec2(
                            (start + end * i as f32 / 48.).cos(),
                            (start + end * i as f32 / 48.).sin(),
                        ) * 26.
                })
                .collect()
        };
        painter.add(egui::Shape::line(
            arc(sweep),
            Stroke::new(4_f32, Color32::from_gray(58)),
        ));
        painter.add(egui::Shape::line(
            arc(sweep * fraction),
            Stroke::new(4_f32, color),
        ));
        painter.circle_filled(center, 20., Color32::from_gray(43));
        let a = start + sweep * fraction;
        painter.line_segment(
            [
                center + vec2(a.cos(), a.sin()) * 8.,
                center + vec2(a.cos(), a.sin()) * 18.,
            ],
            Stroke::new(3_f32, Color32::WHITE),
        );
        if r.has_focus() {
            painter.circle_stroke(center, 30., Stroke::new(1_f32, color));
        }
        ui.add_space(1.);
        if !c.choices.is_empty() {
            egui::ComboBox::from_id_salt("values")
                .width(116.)
                .truncate()
                .selected_text(c.text(value))
                .show_ui(ui, |ui| {
                    for choice in &c.choices {
                        if ui
                            .selectable_value(&mut value, choice.value, &choice.label)
                            .changed()
                        {
                            changed = true;
                            finished = true;
                        }
                    }
                });
        } else {
            let r = ui.add(
                egui::DragValue::new(&mut value)
                    .range(min..=max)
                    .clamp_existing_to_range(false)
                    .speed(0.25)
                    .custom_formatter(|v, _| c.text(v as i32))
                    .custom_parser(|text| c.parse_text(text)),
            );
            changed |= r.changed();
            finished |= r.drag_stopped() || r.lost_focus() || (r.changed() && !r.dragged());
        }
    });
    (value, changed, finished)
}

impl App {
    fn model_code(&self) -> u8 {
        self.connected
            .as_ref()
            .map(|i| i.model_code)
            .unwrap_or(self.offline_model)
    }
    fn patch_set(&self) -> Option<&ParamSet> {
        self.library.data[0]
            .get(self.selected)
            .map(|p| &p.parameters)
    }
    fn number(&self, block: &str, offset: usize) -> i32 {
        self.patch_set()
            .and_then(|set| set.get(block))
            .and_then(|d| {
                self.schema
                    .block(block)
                    .ok()
                    .and_then(|b| b.parameters.iter().find(|p| p.offset == offset))
                    .and_then(|p| p.decode_number(d).ok())
            })
            .unwrap_or(0)
    }
    fn simple(&self, block: &str, offset: usize, label: &str) -> Option<Control> {
        let p = self
            .schema
            .block(block)
            .ok()?
            .parameters
            .iter()
            .find(|p| p.offset == offset)?;
        Some(Control {
            id: format!("{block}%{offset}"),
            block: block.into(),
            offset,
            label: label.into(),
            choices: p.choices.clone(),
            display: "raw".into(),
            min: None,
            max: None,
            bias: p.bias,
        })
    }
    fn color(&self, section: Section) -> usize {
        section
            .color_index()
            .map(|n| self.number("PATCH%COLOR", n).clamp(0, 2) as usize)
            .unwrap_or(0)
    }
    pub(super) fn record_edit(&mut self, edit: Edit) {
        self.dirty = true;
        if self.live && self.write_gate.access() != Access::ReadOnly {
            self.pending_edits
                .retain(|e| e.block != edit.block || e.index != edit.index);
            self.pending_edits.push_back(edit);
        } else {
            self.synced = false;
        }
    }
    fn set_control(&mut self, c: &Control, value: i32) {
        let Some(p) = self.library.data[0].get_mut(self.selected) else {
            return;
        };
        match c.set(&self.schema, &mut p.parameters, value) {
            Ok(e) => self.record_edit(e),
            Err(e) => self.log(e.to_string()),
        }
    }
    fn field(&mut self, ui: &mut egui::Ui, c: Control, accent: Color32, enabled: bool) {
        let Some(set) = self.patch_set() else {
            return;
        };
        let Ok(value) = c.value(&self.schema, set) else {
            ui.label(format!("{}: not loaded", c.label));
            return;
        };
        let Ok((min, max)) = c.range(&self.schema) else {
            return;
        };
        let (new, changed, finished) = ui
            .push_id((&c.id, &c.block), |ui| {
                ui.add_enabled_ui(enabled && !self.blocking_operation(), |ui| {
                    rotary(ui, &c, value, min, max, accent)
                })
                .inner
            })
            .inner;
        if changed && new != value {
            // Update the local byte immediately, but only transmit a completed gesture.
            if let Some(p) = self.library.data[0].get_mut(self.selected) {
                match c.set(&self.schema, &mut p.parameters, new) {
                    Ok(_) => {
                        self.dirty = true;
                        if !self.live {
                            self.synced = false;
                        }
                    }
                    Err(e) => self.log(e.to_string()),
                }
            }
        }
        if finished && self.live && enabled {
            if let Some(p) = self.library.data[0].get_mut(self.selected) {
                if let Ok(edit) = c.set(&self.schema, &mut p.parameters, new) {
                    self.record_edit(edit);
                }
            }
        }
    }
    fn dropdown(&mut self, ui: &mut egui::Ui, c: Control) {
        let Some(set) = self.patch_set() else {
            return;
        };
        let Ok(mut v) = c.value(&self.schema, set) else {
            return;
        };
        let old = v;
        ui.push_id((&c.id, &c.block), |ui| {
            ui.label(&c.label);
            egui::ComboBox::from_id_salt("select")
                .width(180.)
                .selected_text(c.text(v))
                .show_ui(ui, |ui| {
                    for opt in &c.choices {
                        ui.selectable_value(&mut v, opt.value, &opt.label);
                    }
                });
        });
        if old != v {
            self.set_control(&c, v);
        }
    }
    fn effect_switch(&mut self, ui: &mut egui::Ui, section: Section) {
        if let Some(c) = section.switch().and_then(|id| self.catalog.control(id)) {
            let on = self
                .patch_set()
                .and_then(|s| c.value(&self.schema, s).ok())
                .unwrap_or(0)
                != 0;
            let button = egui::Button::new(if on { "●  ON" } else { "○  OFF" }).fill(if on {
                Color32::from_rgb(30, 90, 65)
            } else {
                Color32::from_gray(42)
            });
            if ui.add_enabled(!self.blocking_operation(), button).clicked() {
                self.set_control(&c, i32::from(!on));
            }
        }
    }
    fn select_section(&mut self, section: Section) {
        self.section = section;
    }
    fn field_rows(&mut self, ui: &mut egui::Ui, fields: Vec<(Control, bool)>, accent: Color32) {
        let columns = ((ui.available_width() + 10.) / 132.).floor().max(1.) as usize;
        for row in fields.chunks(columns) {
            ui.horizontal(|ui| {
                for (c, enabled) in row {
                    self.field(ui, c.clone(), accent, *enabled);
                }
            });
        }
    }
    fn amp_panel(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style())
            .fill(Color32::from_rgb(28, 32, 38))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("AMPLIFIER").strong().color(ORANGE));
                    ui.separator();
                    if let Some(mut c) = self.catalog.control("panel-amp-type-knob") {
                        c.choices = ["ACOUSTIC", "CLEAN", "PUSHED", "CRUNCH", "LEAD", "BROWN"]
                            .iter()
                            .enumerate()
                            .map(|(i, s)| crate::schema::Choice {
                                value: i as i32,
                                label: s.to_string(),
                            })
                            .collect();
                        self.dropdown(ui, c);
                    }
                    for id in ["panel-amp-type-vari-btn", "panel-amp-type-bloom-btn"] {
                        if id.contains("bloom") && !editor::bloom(self.model_code()) {
                            continue;
                        }
                        if let Some(mut c) = self.catalog.control(id) {
                            if id.contains("vari") {
                                c.label = "VARIATION".into();
                            }
                            let on = self
                                .patch_set()
                                .and_then(|s| c.value(&self.schema, s).ok())
                                .unwrap_or(0)
                                != 0;
                            if ui.selectable_label(on, &c.label).clicked() {
                                self.set_control(&c, i32::from(!on));
                            }
                        }
                    }
                });
                let fields = [
                    "panel-amp-gain-knob",
                    "panel-amp-volume-knob",
                    "panel-eq-bass-knob",
                    "panel-eq-middle-knob",
                    "panel-eq-treble-knob",
                    "panel-presence-knob",
                ]
                .iter()
                .filter_map(|id| self.catalog.control(id).map(|c| (c, true)))
                .collect();
                self.field_rows(ui, fields, ORANGE);
            });
    }
    pub(super) fn remap_controls(&self, section: Section, c: Control) -> Control {
        let color = self.color(section);
        match section {
            Section::Booster => c.remap("BOOSTER(1)", &format!("BOOSTER({})", color + 1)),
            Section::Mod | Section::Fx => {
                let bank = color + if section == Section::Mod { 1 } else { 4 };
                c.remap("FX_DETAIL(1)", &format!("FX_DETAIL({bank})"))
                    .remap("FX(1)", &format!("FX({bank})"))
            }
            Section::Delay | Section::Delay2 => c.remap(
                "DELAY(1)",
                &format!(
                    "DELAY({})",
                    color + if section == Section::Delay { 1 } else { 4 }
                ),
            ),
            Section::Reverb => c.remap("REVERB(1)", &format!("REVERB({})", color + 1)),
            _ => c,
        }
    }
    pub(super) fn selector(&self, section: Section) -> Option<&'static str> {
        match section {
            Section::Booster => Some("booster-type-select-box"),
            Section::Mod | Section::Fx => Some("modfx-mod-type-select-box"),
            Section::Delay | Section::Delay2 => Some("delay-delay1-type-select-box"),
            Section::Reverb => Some("reverb-type-select-box"),
            Section::Pedal => Some("pedalfx-type-select-box"),
            Section::Eq => Some("eq-type-select-box"),
            Section::Eq2 => Some("eq2-type-select-box"),
            _ => None,
        }
    }
    fn type_name(&self, section: Section) -> String {
        self.selector(section)
            .and_then(|id| self.catalog.control(id))
            .map(|c| self.remap_controls(section, c))
            .and_then(|c| {
                self.patch_set()
                    .and_then(|s| c.value(&self.schema, s).ok())
                    .map(|v| c.text(v))
            })
            .unwrap_or_default()
    }
    fn colors(&mut self, ui: &mut egui::Ui, section: Section) {
        if let Some(index) = section.color_index() {
            let selected = self.color(section);
            for (i, label) in ["GREEN", "RED", "YELLOW"].iter().enumerate() {
                let button = egui::Button::new(RichText::new(*label).color(BANKS[i]))
                    .selected(selected == i);
                if ui.add_enabled(!self.blocking_operation(), button).clicked() {
                    if let Some(c) = self.simple("PATCH%COLOR", index, "COLOR BANK") {
                        self.set_control(&c, i as i32);
                    }
                }
            }
        }
    }
    fn detail_controls(&self, section: Section) -> Vec<Control> {
        let group = match section {
            Section::Booster => Some("booster-content-page"),
            Section::Noise => Some("editor-ns-page"),
            Section::SendReturn => Some("editor-sr-page"),
            Section::Solo => Some("editor-solo-page"),
            Section::Contour => Some("editor-contour-page"),
            _ => None,
        };
        let group = group.map(str::to_string).or_else(|| {
            self.selector(section).and_then(|id| {
                let c = self.remap_controls(section, self.catalog.control(id)?);
                let v = c.value(&self.schema, self.patch_set()?).ok()?;
                self.catalog.page(id, v).map(str::to_string)
            })
        });
        let mut fields = group.map(|g| self.catalog.group(&g)).unwrap_or_default();
        fields.retain(|c| {
            !c.id.starts_with("id_")
                && Some(c.id.as_str()) != section.switch()
                && !c.id.ends_with("-sw-btn")
        });
        if section == Section::Solo && !editor::solo_delay(self.model_code()) {
            fields.retain(|c| !c.id.starts_with("solo-delay"));
        }
        fields
            .into_iter()
            .map(|mut c| {
                if section == Section::Solo {
                    if c.id.starts_with("solo-delay") {
                        c.label = format!("DELAY · {}", c.label);
                    } else if c.id.starts_with("solo-eq") {
                        c.label = format!("EQ · {}", c.label);
                    }
                }
                if section == Section::Contour {
                    if let Some(color) =
                        c.id.rsplit('-')
                            .next()
                            .and_then(|s| s.parse::<usize>().ok())
                            .filter(|i| (1..=3).contains(i))
                    {
                        c.label = format!("CONTOUR {color} · {}", c.label);
                    }
                }
                self.remap_controls(section, c)
            })
            .collect()
    }
    fn effects_row(&mut self, ui: &mut egui::Ui) {
        let columns = ((ui.available_width() + 10.) / 192.).floor().max(1.) as usize;
        let sections = [
            Section::Booster,
            Section::Mod,
            Section::Fx,
            Section::Delay,
            Section::Reverb,
        ];
        for row in sections.chunks(columns) {
            ui.horizontal(|ui| {
                for section in row.iter().copied() {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.set_width(170.);
                            if ui
                                .selectable_label(
                                    self.section == section,
                                    RichText::new(section.label()).strong(),
                                )
                                .clicked()
                            {
                                self.select_section(section);
                            }
                            self.effect_switch(ui, section);
                            ui.label(
                                RichText::new(self.type_name(section))
                                    .color(BANKS[self.color(section)])
                                    .size(11.),
                            );
                            ui.horizontal(|ui| self.colors(ui, section));
                        });
                    });
                }
            });
        }
    }
    fn chain_view(&mut self, ui: &mut egui::Ui) {
        let chain = self.catalog.chain(
            self.number("PATCH%OTHER", 0),
            self.number("PATCH%PEDALFX_COM", 0),
            self.number("PATCH%SENDRETURN", 1),
            self.number("PATCH%EQ_EACH(1)", 0),
            self.number("PATCH%EQ_EACH(2)", 0),
        );
        let Ok(chain) = chain else {
            ui.colored_label(
                Color32::YELLOW,
                "Unknown routing values — inspect Advanced before editing",
            );
            return;
        };
        egui::ScrollArea::horizontal()
            .id_salt("signal-chain")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (i, id) in chain.iter().enumerate() {
                        if i > 0 {
                            ui.label(">");
                        }
                        let section = match id.as_str() {
                            "pdl" => Some(Section::Pedal),
                            "bst" => Some(Section::Booster),
                            "amp" => Some(Section::Amp),
                            "mod" => Some(Section::Mod),
                            "fx" => Some(Section::Fx),
                            "eq" => Some(Section::Eq),
                            "eq2" => Some(Section::Eq2),
                            "dly1" => Some(Section::Delay),
                            "dly2" => Some(Section::Delay2),
                            "rev" => Some(Section::Reverb),
                            "sr" => Some(Section::SendReturn),
                            _ => None,
                        };
                        let text = section.map(|s| s.label()).unwrap_or(match id.as_str() {
                            "in" => "IN",
                            "fv" => "VOLUME",
                            _ => "OUT",
                        });
                        if ui
                            .selectable_label(section == Some(self.section), text)
                            .clicked()
                        {
                            if let Some(s) = section {
                                self.section = s;
                            }
                        }
                    }
                });
            });
    }
    fn details(&mut self, ui: &mut egui::Ui) {
        let section = self.section;
        ui.horizontal_wrapped(|ui| {
            ui.heading(section.label());
            self.effect_switch(ui, section);
            self.colors(ui, section);
            if let Some(id) = self.selector(section) {
                if let Some(mut c) = self.catalog.control(id) {
                    c = self.remap_controls(section, c);
                    c.label = "TYPE".into();
                    self.dropdown(ui, c);
                }
            }
        });
        ui.separator();
        match section {
            Section::Chain => {
                if let Some(mut c) = self.simple("PATCH%OTHER", 0, "CHAIN PATTERN") {
                    c.choices = (0..9)
                        .map(|n| crate::schema::Choice {
                            value: n,
                            label: format!("CHAIN {}", n + 1),
                        })
                        .collect();
                    self.dropdown(ui, c);
                }
                self.chain_view(ui);
                ui.horizontal_wrapped(|ui| {
                    for id in [
                        "pedalfx-position-select-box",
                        "sr-position-select-box",
                        "eq-position-select-box",
                        "eq2-position-select-box",
                    ] {
                        if let Some(mut c) = self.catalog.control(id) {
                            c.label = match id {
                                "pedalfx-position-select-box" => "PEDAL FX POSITION",
                                "sr-position-select-box" => "SEND / RETURN POSITION",
                                "eq-position-select-box" => "EQ POSITION",
                                _ => "EQ 2 POSITION",
                            }
                            .into();
                            self.dropdown(ui, c);
                        }
                    }
                });
                ui.small("Nine original chain patterns. Position selectors update the diagram; selecting a diagram block opens its editor. Drag-to-reorder is not implemented.");
            }
            Section::Amp => {
                let fields = [
                    "panel-cab-resonance-select-box",
                    "modfx-harmonist-master-key-dial",
                    "panel-resonance-knob",
                ]
                .iter()
                .filter_map(|id| self.catalog.control(id).map(|c| (c, true)))
                .collect();
                self.field_rows(ui, fields, ORANGE);
            }
            Section::Assignments => {
                ui.label("Panel knob functions and footswitches");
                let ids = [
                    "asgn-knobs-booster-select-box",
                    "asgn-knobs-mod-select-box",
                    "asgn-knobs-fx-select-box",
                    "asgn-knobs-delay-select-box",
                    "asgn-knobs-reverb-select-box",
                    "asgn-pedal-function-select-box",
                    "asgn-tip-function-select-box",
                    "asgn-ring-function-select-box",
                ];
                for id in ids {
                    if let Some(c) = self.catalog.control(id) {
                        self.dropdown(ui, c);
                    }
                }
                ui.small("Detailed expression ranges and GA-FC routing remain available in Advanced. Automatic dynamic assignment rules are not yet fully ported.");
            }
            _ => {
                let fields = self.detail_controls(section);
                if fields.is_empty() {
                    ui.colored_label(Color32::YELLOW,"No known page for this algorithm. Its raw values are preserved in Advanced.");
                }
                let fields = fields
                    .into_iter()
                    .map(|c| {
                        let bank = self.color(section) + if section == Section::Fx { 4 } else { 1 };
                        let enabled = self
                            .patch_set()
                            .map(|s| self.catalog.enabled(&c.id, &self.schema, s, bank))
                            .unwrap_or(false);
                        (c, enabled)
                    })
                    .collect();
                self.field_rows(ui, fields, BANKS[self.color(section)]);
            }
        }
    }
    pub(super) fn blocking_operation(&self) -> bool {
        self.busy && !self.write_busy
    }
    pub(super) fn classic_editor(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("classic-sidebar")
            .default_width(168.)
            .min_width(155.)
            .max_width(230.)
            .show(ctx, |ui| {
                ui.heading("TONES");
                ui.small("Local library — no auto-select on amp");
                egui::ScrollArea::vertical()
                    .id_salt("tone-list")
                    .max_height(190.)
                    .show(ui, |ui| {
                        let mut next = None;
                        for (i, p) in self.library.data[0].iter().enumerate() {
                            if ui
                                .add_enabled_ui(!self.busy, |ui| {
                                    ui.selectable_label(
                                        i == self.selected,
                                        format!("{:02}  {}", i + 1, p.name()),
                                    )
                                })
                                .inner
                                .clicked()
                            {
                                next = Some(i);
                            }
                        }
                        if let Some(i) = next {
                            self.selected = i;
                            self.disarm();
                        }
                    });
                if ui.button("Open librarian…").clicked() {
                    self.page = Page::Librarian;
                    self.live = false;
                }
                ui.separator();
                ui.heading("EDITOR");
                egui::ScrollArea::vertical()
                    .id_salt("section-list")
                    .show(ui, |ui| {
                        for s in Section::ALL {
                            if ui.selectable_label(self.section == s, s.label()).clicked() {
                                self.select_section(s);
                            }
                        }
                    });
            });
        egui::CentralPanel::default().show(ctx,|ui|{
            if self.library.data[0].is_empty(){ui.heading("No local tones");if ui.button("Create a new tone").clicked(){self.library.data[0].push(Patch::new(&self.schema).unwrap());self.selected=0;self.dirty=true;}return;}
            ui.horizontal_wrapped(|ui|{
                ui.heading(self.library.data[0][self.selected].name());
                ui.label(RichText::new(match self.write_gate.access(){Access::ReadOnly=>"PROTECTED · hardware read-only",Access::EditBuffer=>"TEMPORARY EDITS · stored presets protected",Access::Persistent=>"WARNING · saved writes enabled"}).color(if self.write_gate.access()==Access::Persistent{Color32::LIGHT_RED}else{BANKS[0]}));
            });
            #[cfg(feature="midi")]
            ui.horizontal_wrapped(|ui|{
                if ui.add_enabled(self.connected.is_some()&&!self.busy,egui::Button::new("Read current sound…")).clicked(){self.confirm=Some("read-current".into());}
                let allowed=self.connected.is_some()&&self.synced&&self.write_gate.access()!=Access::ReadOnly&&!self.blocking_operation();
                ui.add_enabled(allowed,egui::Checkbox::new(&mut self.live,"Live edit buffer"));
                if ui.button("Connection / protection…").clicked(){self.page=Page::Connection;self.live=false;}
            });
            if self.connected.is_none(){ui.small("OFFLINE: editing local data only. The initial tone contains defaults, not your amplifier presets.");}
            egui::ScrollArea::vertical().id_salt("classic-main").show(ui,|ui|{
                ui.add_enabled_ui(!self.blocking_operation(),|ui|{self.amp_panel(ui);ui.add_space(8.);self.effects_row(ui);ui.add_space(8.);ui.push_id("summary-chain",|ui|self.chain_view(ui));ui.separator();self.details(ui);});
            });
        });
    }
}
