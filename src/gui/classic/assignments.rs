use super::*;

impl App {
    pub(super) fn assignments_ui(&mut self, ui: &mut egui::Ui) {
        let gafc = editor::gafc(self.model_code());
        if !gafc {
            self.assign_controller = self.assign_controller.min(1);
        }
        ui.horizontal_wrapped(|ui| {
            for (i, label) in [
                "Panel knobs",
                "Amp pedal / footswitch",
                "GA-FC / EX",
                "Expanded GA-FC / EX",
            ]
            .iter()
            .enumerate()
            {
                if i < 2 || gafc {
                    ui.selectable_value(&mut self.assign_controller, i, *label);
                }
            }
        });
        ui.add_space(8.);
        if self.assign_controller == 0 {
            let mut keys = BTreeSet::new();
            for section in [
                Section::Booster,
                Section::Mod,
                Section::Fx,
                Section::Delay,
                Section::Reverb,
            ] {
                let Some(key) = self
                    .patch_set()
                    .and_then(|set| self.catalog.assignment_key(section, &self.schema, set))
                else {
                    continue;
                };
                if !keys.insert(key.clone()) {
                    continue;
                }
                let label = if matches!(section, Section::Mod | Section::Fx) {
                    format!("{} · {}", section.label(), self.type_name(section))
                } else {
                    section.label().into()
                };
                if let Some(c) = self.catalog.knob_assignment(&key, &label) {
                    self.dropdown(ui, c);
                }
            }
            ui.small("MOD and FX share a knob assignment when they use the same algorithm. Hidden assignments are preserved.");
        } else if self.assign_controller == 1 {
            for (id, label) in [
                ("asgn-tip-function-select-box", "Footswitch tip"),
                ("asgn-ring-function-select-box", "Footswitch ring"),
            ] {
                if let Some(mut c) = self.catalog.control(id) {
                    c.label = label.into();
                    self.dropdown(ui, c);
                }
            }
            self.expression_ui(ui, "pedal", false);
        } else {
            let expanded = self.assign_controller == 3;
            for n in 1..=3 {
                if let Some(mut c) = self.catalog.control(&format!(
                    "asgn-{}fs{n}-function-select-box",
                    if expanded { "expand-" } else { "" }
                )) {
                    c.label = format!("Footswitch {n}");
                    self.dropdown(ui, c);
                }
            }
            for n in 1..=3 {
                egui::CollapsingHeader::new(format!("Expression {n}"))
                    .id_salt(("expression", expanded, n))
                    .show(ui, |ui| {
                        self.expression_ui(ui, &format!("exp{n}"), expanded)
                    });
            }
        }
    }
    fn expression_ui(&mut self, ui: &mut egui::Ui, pedal: &str, expanded: bool) {
        let Some(set) = self.patch_set() else {
            return;
        };
        let Ok(controls) = self
            .catalog
            .pedal_assignment(&self.schema, set, pedal, expanded)
        else {
            ui.weak(
                "Assignment data is unavailable or unsupported; original values are preserved.",
            );
            return;
        };
        let old: Vec<_> = controls
            .iter()
            .take(2)
            .map(|c| c.value(&self.schema, set).ok())
            .collect();
        for c in controls.iter().take(2) {
            self.dropdown(ui, c.clone());
        }
        let Some(set) = self.patch_set() else {
            return;
        };
        let Ok(controls) = self
            .catalog
            .pedal_assignment(&self.schema, set, pedal, expanded)
        else {
            return;
        };
        let new: Vec<_> = controls
            .iter()
            .take(2)
            .map(|c| c.value(&self.schema, set).ok())
            .collect();
        if old != new {
            for (i, c) in controls.iter().skip(2).enumerate() {
                if let Ok((min, max)) = c.range(&self.schema) {
                    self.set_control(c, if i == 0 { min } else { max });
                }
            }
        }
        for c in controls.into_iter().skip(2) {
            if c.choices.is_empty() {
                self.field(ui, c, self.theme.accent(), true);
            } else {
                self.dropdown(ui, c);
            }
        }
        ui.small("Heel and toe may be reversed for an inverted sweep. Changing the function/target resets its range.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detailed_expression_controls_render_without_normalizing_loaded_ranges() {
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx);
        for (pedal, expanded) in [("pedal", false), ("exp1", false), ("exp1", true)] {
            let mut function = app
                .catalog
                .control(&format!("asgn-{pedal}-function-select-box"))
                .unwrap();
            if expanded {
                function = function.remap("(1)", "(2)");
            }
            for value in [4, 5, 7, 9] {
                function
                    .set(&app.schema, &mut app.library.data[0][0].parameters, value)
                    .unwrap();
                let before = app.library.data[0][0].parameters.clone();
                let _ = crate::gui::tests::run_ui(&ctx, egui::RawInput::default(), |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        app.expression_ui(ui, pedal, expanded);
                    });
                });
                assert_eq!(app.library.data[0][0].parameters, before);
                assert!(app.pending_edits.is_empty() && !app.dirty);
            }
        }
    }
}
