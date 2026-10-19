use super::*;

impl App {
    pub(super) fn load_library(schema: &Schema) -> (Library, Option<PathBuf>, Option<String>) {
        let isolated = cfg!(test) || std::env::var_os("RUSTY_BOSS_SMOKE_TEST").is_some();
        let path = if isolated { None } else { Library::path() };
        if let Some(p) = path.as_ref().filter(|p| p.exists()) {
            match Library::load(p, schema) {
                Ok(library) => return (library, path, None),
                Err(e) => {
                    return (
                        Library::new(schema),
                        None,
                        Some(format!(
                            "Cannot load {}: {e:#}. Original file preserved; autosave disabled. Export your work before closing.",
                            p.display()
                        )),
                    );
                }
            }
        }
        let error = (!isolated && path.is_none()).then(|| {
            "No local data directory; autosave disabled. Export your work before closing.".into()
        });
        (Library::new(schema), path, error)
    }
    pub(super) fn persist_library(&mut self) {
        self.autosave_at = std::time::Instant::now();
        let Some(path) = &self.library_path else {
            return;
        };
        match self.library.save(path) {
            Ok(()) => {
                self.dirty = false;
                self.library_error = None;
            }
            Err(e) => {
                self.library_error = Some(format!(
                    "Local save failed: {e:#}. Export your work before closing."
                ));
            }
        }
    }
    pub(super) fn switch_collection(&mut self, index: usize) {
        if self.busy || index >= self.library.sets.len() {
            return;
        }
        self.disarm();
        self.library.active = index;
        self.selected = 0;
        self.tone_selection.clear();
        self.pending_bulk.clear();
        self.dirty = true;
    }
    pub(super) fn collection_picker(&mut self, ui: &mut egui::Ui) {
        let mut index = self.library.active;
        ui.add_enabled_ui(!self.busy, |ui| {
            egui::ComboBox::from_id_salt("collection-picker")
                .width(180.)
                .selected_text(&self.library.name)
                .show_ui(ui, |ui| {
                    for (i, set) in self.library.sets.iter().enumerate() {
                        ui.selectable_value(&mut index, i, &set.name);
                    }
                });
        });
        if index != self.library.active {
            self.switch_collection(index);
        }
    }
    pub(super) fn collection_tools(&mut self, ui: &mut egui::Ui) {
        if let Some(error) = self.library_error.clone() {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        ui.horizontal_wrapped(|ui| {
            self.collection_picker(ui);
            if ui
                .add_enabled(
                    self.library.sets.len() < 100,
                    egui::Button::new("New collection"),
                )
                .clicked()
            {
                if let Err(e) = self.library.add(LiveSet::new("New collection")) {
                    self.log(e.to_string());
                }
                self.switch_collection(self.library.active);
            }
            if ui
                .add_enabled(
                    self.library.sets.len() < 100,
                    egui::Button::new("Duplicate collection"),
                )
                .clicked()
            {
                let mut set = self.library.sets[self.library.active].clone();
                set.name.push_str(" copy");
                if let Err(e) = self.library.add(set) {
                    self.log(e.to_string());
                }
                self.switch_collection(self.library.active);
            }
            if ui
                .add_enabled(
                    self.library.sets.len() > 1,
                    egui::Button::new("Delete collection…"),
                )
                .clicked()
            {
                self.confirm = Some("delete-set".into());
            }
        });
        ui.small("Collections save locally. Import adds a set; Export TSL shares the current set.");
    }
    pub(super) fn delete_collection(&mut self) {
        if self.busy || self.library.sets.len() <= 1 {
            return;
        }
        let index = self.library.active;
        self.library.sets.remove(index);
        self.switch_collection(index.min(self.library.sets.len() - 1));
    }
    pub(super) fn delete_selected(&mut self) {
        if self.busy
            || self
                .tone_selection
                .iter()
                .any(|i| *i >= self.library.data[0].len())
        {
            return;
        }
        for i in self.tone_selection.iter().rev() {
            self.library.data[0].remove(*i);
        }
        self.tone_selection.clear();
        self.selected = self
            .selected
            .min(self.library.data[0].len().saturating_sub(1));
        self.dirty = true;
        self.disarm();
    }
    pub(super) fn request_bulk_save(&mut self) {
        if self.busy
            || self.confirm.is_some()
            || self.tone_selection.is_empty()
            || self.tone_selection.len() != self.amp_selection.len()
        {
            self.log("Select the same number of tones and destination channels");
            return;
        }
        if !self
            .amp_selection
            .iter()
            .all(|slot| self.can_save_to(*slot))
            || self
                .tone_selection
                .iter()
                .any(|i| *i >= self.library.data[0].len())
        {
            self.log("Enable amp writes and select available channels first");
            return;
        }
        self.pending_bulk = self
            .amp_selection
            .iter()
            .zip(self.tone_selection.iter())
            .map(|(slot, index)| (*slot, self.library.data[0][*index].clone()))
            .collect();
        self.confirm = Some("save-many".into());
    }
    pub(super) fn bulk_library(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Select & transfer tones").show(ui, |ui| {
            self.tone_selection.retain(|i| *i < self.library.data[0].len());
            ui.horizontal(|ui| {
                if ui.button("Select all tones").clicked() { self.tone_selection = (0..self.library.data[0].len()).collect(); }
                if ui.button("Clear selection").clicked() { self.tone_selection.clear(); }
                ui.label(format!("{} selected",self.tone_selection.len()));
            });
            egui::ScrollArea::vertical().id_salt("bulk-tones").max_height(220.).show(ui, |ui| {
                for (i,patch) in self.library.data[0].iter().enumerate() {
                    let mut selected = self.tone_selection.contains(&i);
                    if ui.checkbox(&mut selected,format!("{:02}  {}",i+1,patch.name())).changed() {
                        if selected { self.tone_selection.insert(i); } else { self.tone_selection.remove(&i); }
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                self.target_set = self.target_set.min(self.library.sets.len()-1);
                egui::ComboBox::from_id_salt("transfer-collection").selected_text(&self.library.sets[self.target_set].name)
                    .show_ui(ui, |ui| { for (i,set) in self.library.sets.iter().enumerate() { if i != self.library.active { ui.selectable_value(&mut self.target_set,i,&set.name); } } });
                let enabled = !self.tone_selection.is_empty() && self.target_set != self.library.active;
                for (label,moving) in [("Copy to collection",false),("Move to collection",true)] {
                    if ui.add_enabled(enabled,egui::Button::new(label)).clicked() {
                        match self.library.transfer(&self.tone_selection,self.target_set,moving) {
                            Ok(()) => { self.tone_selection.clear(); self.selected = self.selected.min(self.library.data[0].len().saturating_sub(1)); self.dirty = true; self.disarm(); }
                            Err(e) => self.log(e.to_string()),
                        }
                    }
                }
                if ui.add_enabled(!self.tone_selection.is_empty(),egui::Button::new("Delete selected…")).clicked() { self.confirm = Some("delete-selected".into()); }
            });
            if let Some(id) = self.connected.clone() {
                ui.separator(); ui.strong("Amp channels");
                self.amp_selection.retain(|slot| id.slot_exists(*slot));
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Select saved channels").clicked() { self.amp_selection = (1..9).filter(|slot| id.slot_exists(*slot)).collect(); }
                    if ui.button("Clear channels").clicked() { self.amp_selection.clear(); }
                    for slot in 0..9 { if id.slot_exists(slot) {
                        let mut selected = self.amp_selection.contains(&slot);
                        if ui.checkbox(&mut selected,slot_label(slot)).changed() { if selected { self.amp_selection.insert(slot); } else { self.amp_selection.remove(&slot); } }
                    } }
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(!self.amp_selection.is_empty() && self.library.data[0].len() + self.amp_selection.len() <= 1000,egui::Button::new("Copy channels into this collection")).clicked() {
                        self.disarm();
                        #[cfg(feature = "midi")]
                        self.submit(Job::ReadSlots(self.amp_selection.iter().copied().collect()));
                    }
                    if ui.add_enabled(self.write_gate.access() == Access::Persistent && !self.tone_selection.is_empty() && self.tone_selection.len() == self.amp_selection.len(),egui::Button::new("Save selected tones to channels…")).clicked() { self.request_bulk_save(); }
                });
                ui.small("Tones and channels are paired in list order. Overwrites are listed for confirmation. PANEL is included only if you explicitly select it.");
            }
        });
    }
}
