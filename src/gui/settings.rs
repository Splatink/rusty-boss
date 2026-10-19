use super::*;
use crate::editor::Control;

#[derive(Clone)]
pub(super) struct SystemDraft {
    pub before: ParamSet,
    pub values: ParamSet,
}
impl App {
    pub(super) fn open_backup(&mut self, path: &std::path::Path) {
        match Backup::load(path) {
            Ok(backup) => {
                self.backup = Some(backup);
                self.backup_file = path.to_string_lossy().into_owned();
                self.log("Backup opened · Review the model before restoring");
            }
            Err(e) => self.log(format!("Backup not opened: {e:#}")),
        }
    }
    pub(super) fn settings_page(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Amp settings").show(ui, |ui| {
            ui.small("Read once, adjust locally, then Apply. These settings persist on the amp; preview permission alone cannot change them.");
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(self.connected.is_some() && !self.busy, egui::Button::new(if self.system.is_some() { "Reload (discard staged edits)" } else { "Read amp settings" })).clicked() {
                    #[cfg(feature = "midi")]
                    self.submit(Job::ReadSystem);
                }
                let changed = self.system.as_ref().is_some_and(|s| s.before != s.values);
                if ui.add_enabled(changed && !self.busy && self.connected.is_some() && self.write_gate.access() == Access::Persistent, egui::Button::new("Apply to amp…")).clicked() { self.confirm = Some("apply-system".into()); }
                if ui.add_enabled(changed && !self.busy,egui::Button::new("Discard changes")).clicked()
                    && let Some(draft) = &mut self.system { draft.values = draft.before.clone(); }
            });
            if self.system.is_none() { return; }
            ui.add_enabled_ui(!self.busy, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (i,label) in ["Global EQ", "Outputs", "USB", "MIDI", "Foot control"].iter().enumerate() { ui.selectable_value(&mut self.system_page,i,*label); }
                });
                match self.system_page {
                    0 => self.global_eq_settings(ui),
                    1 => {
                        self.system_group(ui,"system-line-out-setting-page",1);
                        ui.separator(); self.system_group(ui,"system-power-amp-in-setting-page",1);
                        if [9,11].contains(&self.model_code()) { ui.separator(); self.system_group(ui,"system-powercontrol-page",1); }
                    }
                    2 => self.system_group(ui,"system-usb-setting-page",1),
                    3 => self.system_group(ui,"system-midi-setting-page",1),
                    _ => {
                        if crate::editor::gafc(self.model_code()) { self.system_group(ui,"system-gafc-page",1); }
                        else { ui.weak("This amp model has no GA-FC support."); }
                    }
                }
            });
        });
    }
    pub(super) fn global_eq_settings(&mut self, ui: &mut egui::Ui) {
        if let Some(c) = self.catalog.control("global-eq-sw-btn") {
            self.setting_field(ui, c);
        }
        if let Some(mut c) = self.catalog.control("toneshape-type-select-box") {
            c.block = "SYSTEM%GLOBAL_COM".into();
            c.offset = 1;
            c.label = "EQ bank".into();
            c.choices = ["Green", "Red", "Yellow"]
                .iter()
                .enumerate()
                .map(|(i, label)| crate::schema::Choice {
                    value: i as i32,
                    label: (*label).into(),
                })
                .collect();
            self.setting_field(ui, c);
        }
        let bank = self
            .system
            .as_ref()
            .and_then(|d| d.values.get("SYSTEM%GLOBAL_COM"))
            .map(|v| v[1].min(2) as usize + 1)
            .unwrap_or(1);
        for id in ["global-eq-position-select-box", "global-eq-type-select-box"] {
            if let Some(c) = self.catalog.control(id) {
                self.setting_field(ui, c.remap("(1)", &format!("({bank})")));
            }
        }
        let eq_type = self
            .system
            .as_ref()
            .and_then(|d| d.values.get(&format!("SYSTEM%GLOBAL_EACH({bank})")))
            .map(|v| v[1])
            .unwrap_or(0);
        self.system_group(
            ui,
            if eq_type == 0 {
                "global-eq-peq-page"
            } else {
                "global-eq-geq-page"
            },
            bank,
        );
    }
    pub(super) fn system_group(&mut self, ui: &mut egui::Ui, group: &str, bank: usize) {
        let mut seen = BTreeSet::new();
        for c in self.catalog.group(group) {
            if !c.block.starts_with("SYSTEM%")
                || c.id.starts_with("id_")
                || !seen.insert((c.block.clone(), c.offset))
            {
                continue;
            }
            if self.model_code() == 5
                && (c.id.contains("gafc")
                    || c.id.contains("-rx-pc-p3-")
                    || c.id.contains("-rx-pc-p4-")
                    || c.id.contains("-rx-pc-p7-")
                    || c.id.contains("-rx-pc-p8-"))
            {
                continue;
            }
            self.setting_field(ui, c.remap("(1)", &format!("({bank})")));
        }
    }
    fn setting_field(&mut self, ui: &mut egui::Ui, c: Control) {
        let Some(draft) = &self.system else {
            return;
        };
        let (Ok(mut value), Ok((min, max))) =
            (c.value(&self.schema, &draft.values), c.range(&self.schema))
        else {
            return;
        };
        let old = value;
        ui.push_id((&c.block, c.offset), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(&c.label);
                if !c.choices.is_empty() {
                    egui::ComboBox::from_id_salt("setting")
                        .selected_text(c.text(value))
                        .show_ui(ui, |ui| {
                            for choice in &c.choices {
                                ui.selectable_value(&mut value, choice.value, &choice.label);
                            }
                        });
                } else if (min, max) == (0, 1) {
                    let mut enabled = value != 0;
                    if ui.checkbox(&mut enabled, "On").changed() {
                        value = i32::from(enabled);
                    }
                } else {
                    ui.add(
                        egui::DragValue::new(&mut value)
                            .range(min..=max)
                            .custom_formatter(|v, _| c.text(v as i32))
                            .custom_parser(|text| c.parse_text(text)),
                    );
                }
            });
        });
        if value != old
            && let Some(draft) = &mut self.system
            && let Err(e) = c.set(&self.schema, &mut draft.values, value)
        {
            self.log(e.to_string());
        }
    }
}
