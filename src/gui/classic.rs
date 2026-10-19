//! Tone Studio-style presentation. No I/O: all changes go through App's edit queue.
use super::*;
use crate::editor::{self, Control, Edit, Section};
use crate::safety::Access;
use egui::{Sense, Stroke, vec2};
mod assignments;
#[cfg(test)]
mod chain_tests;
#[cfg(test)]
mod tests;
fn card(ui: &egui::Ui) -> egui::Frame {
    egui::Frame::new()
        .fill(ui.visuals().window_fill)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(10)
        .inner_margin(16)
}
fn bank_color(ui: &egui::Ui, bank: usize) -> Color32 {
    if ui.visuals().dark_mode {
        [
            Color32::from_rgb(66, 195, 119),
            Color32::from_rgb(232, 90, 86),
            Color32::from_rgb(238, 199, 69),
        ][bank]
    } else {
        [
            Color32::from_rgb(18, 116, 58),
            Color32::from_rgb(174, 32, 46),
            Color32::from_rgb(135, 96, 0),
        ][bank]
    }
}

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
    ui.allocate_ui_with_layout(vec2(122., 142.), egui::Layout::top_down(egui::Align::Center), |ui| {
        ui.set_width(122.);
        ui.spacing_mut().item_spacing.y = 4.;
        ui.add_sized(
            [122., 22.],
            egui::Label::new(RichText::new(&c.label).size(11.).color(ui.visuals().weak_text_color()).strong()).wrap(),
        );
        let (rect, r) = ui.allocate_exact_size(vec2(122., 72.), Sense::click_and_drag());
        let mut r = r.on_hover_text("Drag up/down to adjust · Arrow keys for single steps\nClick the value below to type an exact value");
        r.widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), value as f64, &c.label));
        if r.clicked() || r.drag_started() {
            r.request_focus();
        }
        if r.clicked() {
            ui.ctx().request_discard("Initialize rotary keyboard focus");
        }
        if r.has_focus() {
            ui.memory_mut(|m| m.set_focus_lock_filter(r.id, egui::EventFilter {
                horizontal_arrows: true,
                vertical_arrows: true,
                ..Default::default()
            }));
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
        if ui.is_enabled() && r.has_focus() {
            let step = ui.input_mut(|i| {
                let modifiers = egui::Modifiers::NONE;
                i.count_and_consume_key(modifiers, egui::Key::ArrowUp) as i32
                    + i.count_and_consume_key(modifiers, egui::Key::ArrowRight) as i32
                    - i.count_and_consume_key(modifiers, egui::Key::ArrowDown) as i32
                    - i.count_and_consume_key(modifiers, egui::Key::ArrowLeft) as i32
            });
            if step != 0 {
                let new = value.saturating_add(step).clamp(min, max);
                changed |= new != value;
                value = new;
                finished = changed;
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
        for i in 0..=10 {
            let a = start + sweep * i as f32 / 10.;
            let dir = vec2(a.cos(), a.sin());
            painter.line_segment([center + dir * 33., center + dir * 36.], Stroke::new(1_f32, ui.visuals().widgets.noninteractive.bg_stroke.color));
        }
        painter.add(egui::Shape::line(
            arc(sweep),
            Stroke::new(3_f32, ui.visuals().widgets.inactive.bg_fill),
        ));
        painter.add(egui::Shape::line(
            arc(sweep * fraction),
            Stroke::new(4_f32, color),
        ));
        painter.circle_filled(center + vec2(0., 2.), 22., ui.visuals().extreme_bg_color);
        painter.circle_filled(center, 21., ui.visuals().widgets.inactive.bg_fill);
        painter.circle_stroke(center, 21., Stroke::new(1_f32, ui.visuals().widgets.noninteractive.bg_stroke.color));
        let a = start + sweep * fraction;
        painter.line_segment(
            [
                center + vec2(a.cos(), a.sin()) * 8.,
                center + vec2(a.cos(), a.sin()) * 18.,
            ],
            Stroke::new(3_f32, ui.visuals().text_color()),
        );
        if r.has_focus() {
            painter.circle_stroke(center, 30., Stroke::new(1_f32, color));
        }
        ui.add_space(3.);
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
                    .speed(1.)
                    .custom_formatter(|v, _| c.text(v as i32))
                    .custom_parser(|text| c.parse_text(text)),
            );
            if r.gained_focus() {
                ui.ctx().request_discard("Initialize numeric keyboard focus");
            }
            if r.has_focus() {
                ui.memory_mut(|m| {
                    m.set_focus_lock_filter(r.id, egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..Default::default()
                    });
                });
            }
            changed |= r.changed();
            finished |= r.drag_stopped() || r.lost_focus() || (r.changed() && !r.dragged());
        }
    });
    let gesture = ui.scope_id().with("rotary-edited");
    let edited = changed || ui.data(|d| d.get_temp::<bool>(gesture).unwrap_or(false));
    ui.data_mut(|d| d.insert_temp(gesture, edited && !finished));
    (value, changed, finished && edited)
}

#[derive(Clone)]
struct ChainDrag(String);

impl App {
    pub(super) fn model_code(&self) -> u8 {
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
        if finished
            && self.live
            && enabled
            && let Some(p) = self.library.data[0].get_mut(self.selected)
            && let Ok(edit) = c.set(&self.schema, &mut p.parameters, new)
        {
            self.record_edit(edit);
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
                ui.visuals().selection.bg_fill
            } else {
                ui.visuals().widgets.inactive.bg_fill
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
        card(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("AMPLIFIER")
                        .size(12.)
                        .strong()
                        .color(self.theme.accent()),
                );
                ui.add_space(12.);
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
            self.field_rows(ui, fields, self.theme.accent());
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
    pub(super) fn type_name(&self, section: Section) -> String {
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
                let button = egui::Button::new(
                    RichText::new(["G", "R", "Y"][i])
                        .size(12.)
                        .strong()
                        .color(bank_color(ui, i)),
                )
                .min_size(vec2(30., 26.))
                .corner_radius(13)
                .stroke(Stroke::new(
                    1_f32,
                    if selected == i {
                        bank_color(ui, i)
                    } else {
                        Color32::TRANSPARENT
                    },
                ));
                if ui
                    .add_enabled(!self.blocking_operation(), button)
                    .on_hover_text(format!("{label} algorithm bank"))
                    .clicked()
                    && let Some(c) = self.simple("PATCH%COLOR", index, "COLOR BANK")
                {
                    self.set_control(&c, i as i32);
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
                if section == Section::Contour
                    && let Some(color) =
                        c.id.rsplit('-')
                            .next()
                            .and_then(|s| s.parse::<usize>().ok())
                            .filter(|i| (1..=3).contains(i))
                {
                    c.label = format!("CONTOUR {color} · {}", c.label);
                }
                self.remap_controls(section, c)
            })
            .collect()
    }
    fn panel_snapshot(&mut self, ui: &mut egui::Ui, section: Section) {
        let Some(snapshot) = self
            .patch_set()
            .and_then(|set| self.catalog.panel_position(&self.schema, set, section))
        else {
            ui.weak("PANEL —")
                .on_hover_text("Position unavailable; read or preview the amp sound first.");
            return;
        };
        let index = if section == Section::Solo {
            5
        } else {
            section.color_index().unwrap_or(0)
        };
        let enabled = self.live
            && !self.busy
            && self.pending_edits.is_empty()
            && self.connected.is_some()
            && self.write_gate.access() != Access::ReadOnly;
        let id = ui.scope_id().with(("panel-macro", index, self.generation));
        let value = ui.data(|d| d.get_temp::<i32>(id)).unwrap_or(snapshot);
        let c = Control {
            id: format!("panel-macro-{index}"),
            block: String::new(),
            offset: 0,
            label: "PANEL".into(),
            choices: vec![],
            display: "raw".into(),
            min: Some(0),
            max: Some(100),
            bias: 1,
        };
        let (new, changed, finished) = ui
            .add_enabled_ui(enabled, |ui| {
                rotary(ui, &c, value, 0, 100, self.theme.accent())
            })
            .inner;
        if changed {
            ui.data_mut(|d| d.insert_temp(id, new));
        }
        if finished {
            ui.data_mut(|d| d.remove::<i32>(id));
            if enabled && new != snapshot {
                #[cfg(feature = "midi")]
                self.submit(Job::Macro(self.generation, index, new));
            }
        }
    }
    fn effects_row(&mut self, ui: &mut egui::Ui) {
        let columns = ((ui.available_width() + 10.) / 173.).floor().clamp(1., 5.) as usize;
        let width = (ui.available_width() - 10. * (columns - 1) as f32) / columns as f32 - 26.;
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
                    let accent = bank_color(ui, self.color(section));
                    let selected = self.section == section;
                    card(ui)
                        .inner_margin(12)
                        .stroke(Stroke::new(
                            1_f32,
                            if selected {
                                accent
                            } else {
                                ui.visuals().widgets.noninteractive.bg_stroke.color
                            },
                        ))
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 6.;
                                ui.set_width(width);
                                if let Some(icon) = self.effect_icon(ui.ctx(), section) {
                                    ui.image((icon.id(), vec2(32., 32.)));
                                }
                                if ui
                                    .add_sized(
                                        [width, 28.],
                                        egui::Button::new(
                                            RichText::new(section.label()).strong().color(
                                                if selected {
                                                    accent
                                                } else {
                                                    ui.visuals().text_color()
                                                },
                                            ),
                                        )
                                        .frame(false),
                                    )
                                    .clicked()
                                {
                                    self.select_section(section);
                                }
                                ui.add_sized(
                                    [width, 20.],
                                    egui::Label::new(
                                        RichText::new(self.type_name(section))
                                            .size(11.)
                                            .color(accent),
                                    )
                                    .truncate(),
                                );
                                ui.horizontal(|ui| self.colors(ui, section));
                                ui.add_space(2.);
                                self.panel_snapshot(ui, section);
                                self.effect_switch(ui, section);
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
                "Unknown routing values preserved; choose a known signal path to edit",
            );
            return;
        };
        let mut movement = None;
        let enabled = !self.blocking_operation();
        egui::ScrollArea::horizontal()
            .id_salt("signal-chain")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (i, id) in chain.iter().enumerate() {
                        if i > 0 {
                            ui.label(RichText::new("›").color(ui.visuals().weak_text_color()));
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
                        let label = RichText::new(text).size(10.).strong();
                        let button =
                            match section.and_then(|section| self.effect_icon(ui.ctx(), section)) {
                                Some(icon) => egui::Button::image_and_text(
                                    egui::Image::new((icon.id(), vec2(24., 24.))),
                                    label,
                                ),
                                None => egui::Button::new(label),
                            };
                        let r = ui.add_enabled(
                            enabled,
                            button
                                .selected(section == Some(self.section))
                                .sense(Sense::click_and_drag())
                                .min_size(vec2(44., 32.)),
                        );
                        let movable =
                            !["in", "speaker", "amp", "fv", "dly2", "rev"].contains(&id.as_str());
                        if movable {
                            r.dnd_set_drag_payload(ChainDrag(id.clone()));
                            r.context_menu(|ui| {
                                ui.label("Move after:");
                                for target in &chain {
                                    if let Ok(route) =
                                        self.catalog.move_in_chain(&chain, id, target)
                                        && ui.button(target.to_uppercase()).clicked()
                                    {
                                        movement = Some(route);
                                        ui.close();
                                    }
                                }
                            });
                        }
                        if let Some(source) = r.dnd_hover_payload::<ChainDrag>()
                            && let Ok(route) = self.catalog.move_in_chain(&chain, &source.0, id)
                        {
                            ui.painter().rect_stroke(
                                r.rect,
                                4.,
                                Stroke::new(2_f32, self.theme.accent()),
                                egui::StrokeKind::Outside,
                            );
                            if enabled && r.dnd_release_payload::<ChainDrag>().is_some() {
                                movement = Some(route);
                            }
                        }
                        if r.clicked()
                            && let Some(s) = section
                        {
                            self.section = s;
                        }
                    }
                });
            });
        if let Some(route) = movement {
            for ((block, offset), value) in [
                ("PATCH%OTHER", 0),
                ("PATCH%PEDALFX_COM", 0),
                ("PATCH%SENDRETURN", 1),
                ("PATCH%EQ_EACH(1)", 0),
                ("PATCH%EQ_EACH(2)", 0),
            ]
            .into_iter()
            .zip(route)
            {
                if self.number(block, offset) != value
                    && let Some(c) = self.simple(block, offset, "Routing")
                {
                    self.set_control(&c, value);
                }
            }
        }
    }
    fn details(&mut self, ui: &mut egui::Ui) {
        let section = self.section;
        ui.horizontal_wrapped(|ui| {
            ui.heading(section.label());
            self.effect_switch(ui, section);
            self.colors(ui, section);
            if let Some(id) = self.selector(section)
                && let Some(mut c) = self.catalog.control(id)
            {
                c = self.remap_controls(section, c);
                c.label = "TYPE".into();
                self.dropdown(ui, c);
            }
        });
        ui.add_space(10.);
        self.icon_tools(ui, section);
        if matches!(section, Section::Delay | Section::Delay2 | Section::Solo)
            && (section != Section::Solo || editor::solo_delay(self.model_code()))
            && ui
                .add_enabled(
                    self.live
                        && !self.busy
                        && self.pending_edits.is_empty()
                        && self.write_gate.access() != Access::ReadOnly,
                    egui::Button::new("Tap tempo"),
                )
                .clicked()
        {
            #[cfg(feature = "midi")]
            self.submit(Job::Tap(match section {
                Section::Delay => 0,
                Section::Delay2 => 1,
                _ => 2,
            }));
        }
        if matches!(section, Section::Reverb | Section::Delay2)
            && let Some(mut c) = self.catalog.control(&format!(
                "reverb-layer-mode-{}-select-box",
                ["grn", "red", "ylw"][self.color(section)]
            ))
        {
            c.label = "Delay 2 / Reverb layer".into();
            self.dropdown(ui, c);
        }
        if section == Section::Solo && editor::solo_delay(self.model_code()) {
            self.panel_snapshot(ui, section);
        }
        if section == Section::Reverb {
            ui.horizontal_wrapped(|ui| {
                self.panel_snapshot(ui, section);
                ui.label(RichText::new("Assignable panel macro · separate from Effect Level").size(12.).color(ui.visuals().weak_text_color()))
                    .on_hover_text("Preview a tone to enable this macro. The firmware coordinates its assigned parameters; values are read back after each gesture.");
            });
            ui.add_space(12.);
        }
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
                ui.small("Drag a block onto another to move it after that block, or right-click for destinations. Only amp-supported routes are allowed. Click a block to open its editor.");
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
                self.field_rows(ui, fields, self.theme.accent());
            }
            Section::Assignments => self.assignments_ui(ui),
            _ => {
                let fields = self.detail_controls(section);
                if fields.is_empty() {
                    ui.colored_label(
                        Color32::YELLOW,
                        "No controls available for this algorithm yet. Its values are preserved.",
                    );
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
                self.field_rows(ui, fields, bank_color(ui, self.color(section)));
            }
        }
    }
    pub(super) fn blocking_operation(&self) -> bool {
        self.busy && !self.write_busy
    }
    pub(super) fn classic_editor(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(18).fill(ui.visuals().panel_fill)).show(ui, |ui| {
            if self.library.data[0].is_empty() {
                ui.heading("Your next sound starts here");
                ui.label("Import a Gen 3 TSL using the button on the left, or create a tone in Library.");
                return;
            }
            ui.horizontal_wrapped(|ui| {
                ui.heading(self.library.data[0][self.selected].name());
                ui.label(RichText::new(if self.live { "● Live on amp" } else if self.connected.is_none() { "Local tone" } else { "Local · click tone to preview" }).size(12.).color(self.theme.accent()));
                #[cfg(feature="midi")]
                if ui.add_enabled(self.connected.is_some() && !self.busy, egui::Button::new("Copy amp sound")).on_hover_text("Read the current amp sound into a new local tone. Your existing tones are kept.").clicked() {
                    self.disarm(); self.submit(Job::ReadCurrent);
                }
            });
            ui.horizontal_wrapped(|ui| {
                for section in Section::ALL {
                    if ui.selectable_label(self.section == section, section.label()).clicked() { self.select_section(section); }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().id_salt("classic-main").auto_shrink([false, false]).show(ui, |ui| {
                ui.add_enabled_ui(!self.blocking_operation(), |ui| {
                    if self.section == Section::Amp {
                        self.amp_panel(ui);
                        egui::CollapsingHeader::new("Cabinet, resonance & master key").show(ui, |ui| self.details(ui));
                        ui.add_space(10.);
                    }
                    self.effects_row(ui);
                    ui.add_space(10.);
                    if self.section != Section::Amp {
                        card(ui).show(ui, |ui| { ui.set_width(ui.available_width()); self.details(ui); });
                        ui.add_space(10.);
                    }
                    egui::CollapsingHeader::new("Signal path").default_open(true).show(ui, |ui| {
                        self.chain_view(ui);
                    });
                });
            });
        });
    }
}
