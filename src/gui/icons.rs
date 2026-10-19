use super::*;
use anyhow::{Context, Result, ensure};
use std::io::Read;

fn png(bytes: &[u8]) -> Result<egui::ColorImage> {
    ensure!(
        bytes.len() >= 24 && bytes.len() <= 2 * 1024 * 1024 && &bytes[..8] == b"\x89PNG\r\n\x1a\n",
        "Choose a PNG under 2 MiB"
    );
    let width = u32::from_be_bytes(bytes[16..20].try_into()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into()?);
    ensure!(
        (1..=512).contains(&width) && (1..=512).contains(&height),
        "Icons must be at most 512 × 512 pixels"
    );
    let image = eframe::icon_data::from_png_bytes(bytes)?;
    ensure!(
        image.width == width && image.height == height,
        "Invalid PNG dimensions"
    );
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        &image.rgba,
    ))
}
impl App {
    pub(super) fn icon_key(&self, section: Section) -> Option<String> {
        let selector = self.selector(section)?;
        let c = self.remap_controls(section, self.catalog.control(selector)?);
        let set = self.library.data[0].get(self.selected)?;
        let value = c.value(&self.schema, &set.parameters).ok()?;
        if value < 0 {
            return None;
        }
        let category = match section {
            Section::Booster => "booster",
            Section::Mod | Section::Fx => "modfx",
            Section::Delay | Section::Delay2 => "delay",
            Section::Reverb => "reverb",
            Section::Pedal => "pedal",
            Section::Eq | Section::Eq2 => "eq",
            _ => return None,
        };
        Some(format!("{category}-{value}"))
    }
    fn icon_path(&self, key: &str) -> Result<PathBuf> {
        ensure!(
            !key.is_empty()
                && key.len() <= 64
                && key
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
            "invalid icon key"
        );
        Ok(self
            .library_path
            .as_ref()
            .context("Local storage is unavailable")?
            .parent()
            .context("missing data directory")?
            .join("icons")
            .join(format!("{key}.json")))
    }
    pub(super) fn import_icon(&mut self, key: &str, path: &std::path::Path) -> Result<()> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        png(&bytes)?;
        let destination = self.icon_path(key)?;
        std::fs::create_dir_all(destination.parent().context("missing icon directory")?)?;
        crate::document::atomic_json(&destination, &bytes)?;
        self.icons.remove(key);
        self.log("Custom icon saved locally · Tone and amp settings unchanged");
        Ok(())
    }
    pub(super) fn effect_icon(
        &mut self,
        ctx: &egui::Context,
        section: Section,
    ) -> Option<egui::TextureHandle> {
        let key = self.icon_key(section)?;
        if !self.icons.contains_key(&key) {
            let texture = self
                .icon_path(&key)
                .ok()
                .filter(|p| p.is_file())
                .and_then(|path| crate::document::read_json::<Vec<u8>>(&path).ok())
                .and_then(|bytes| png(&bytes).ok())
                .map(|image| ctx.load_texture(&key, image, egui::TextureOptions::LINEAR));
            self.icons.insert(key.clone(), texture);
        }
        self.icons.get(&key).cloned().flatten()
    }
    pub(super) fn icon_tools(&mut self, ui: &mut egui::Ui, section: Section) {
        let Some(key) = self.icon_key(section) else {
            return;
        };
        ui.menu_button("Icon…", |ui| {
            if ui
                .add_enabled(
                    self.library_path.is_some(),
                    egui::Button::new("Choose PNG…"),
                )
                .clicked()
            {
                self.choose_file(FileAction::Icon(key.clone()));
                ui.close();
            }
            if ui.button("Use default").clicked() {
                match self
                    .icon_path(&key)
                    .and_then(|path| match std::fs::remove_file(path) {
                        Ok(()) => Ok(()),
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                        Err(e) => Err(e.into()),
                    }) {
                    Ok(()) => {
                        self.icons.remove(&key);
                        self.log("Default effect icon restored");
                    }
                    Err(e) => self.log(e.to_string()),
                }
                ui.close();
            }
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_or_oversized_icons_are_rejected_before_decode() {
        assert!(png(b"not a png").is_err());
        let mut bytes = b"\x89PNG\r\n\x1a\n00000000".to_vec();
        bytes.extend_from_slice(&513u32.to_be_bytes());
        bytes.extend_from_slice(&1u32.to_be_bytes());
        assert!(png(&bytes).unwrap_err().to_string().contains("512"));
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx);
        app.library_path = Some(PathBuf::from("/tmp/rusty-boss-test/library.json"));
        assert!(app.icon_path("../escape").is_err());
        assert!(
            app.icon_path("modfx-23")
                .unwrap()
                .ends_with("icons/modfx-23.json")
        );
    }
}
