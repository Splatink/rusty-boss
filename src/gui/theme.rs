//! Appearance preferences only. Hardware permissions are never persisted.
use eframe::egui::{self, Color32, Stroke};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Theme {
    Justice,
    Kill,
    #[default]
    Puppet,
    Lightning,
}
impl Theme {
    pub const ALL: [Self; 4] = [Self::Puppet, Self::Justice, Self::Kill, Self::Lightning];
    pub fn name(self) -> &'static str {
        match self {
            Self::Justice => "Justice",
            Self::Kill => "Kill",
            Self::Puppet => "Puppet",
            Self::Lightning => "Lightning",
        }
    }
    pub fn accent(self) -> Color32 {
        match self {
            Self::Justice => Color32::from_rgb(100, 86, 148),
            Self::Kill => Color32::from_rgb(245, 79, 91),
            Self::Puppet => Color32::from_rgb(244, 161, 85),
            Self::Lightning => Color32::from_rgb(62, 195, 255),
        }
    }
    pub fn apply(self, ctx: &egui::Context) {
        let light = self == Self::Justice;
        let (background, surface, raised, text, border) = match self {
            Self::Justice => (
                (239, 238, 243),
                (252, 251, 254),
                (227, 225, 234),
                (43, 40, 54),
                (194, 190, 207),
            ),
            Self::Kill => (
                (24, 15, 20),
                (35, 23, 29),
                (54, 32, 41),
                (245, 227, 231),
                (81, 47, 59),
            ),
            Self::Puppet => (
                (20, 22, 27),
                (29, 32, 39),
                (43, 47, 56),
                (230, 229, 228),
                (64, 66, 75),
            ),
            Self::Lightning => (
                (11, 20, 34),
                (19, 31, 49),
                (28, 47, 69),
                (219, 238, 252),
                (43, 73, 99),
            ),
        };
        let rgb = |(r, g, b)| Color32::from_rgb(r, g, b);
        let mut style = (*ctx.style()).clone();
        let mut v = if light {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };
        v.panel_fill = rgb(background);
        v.window_fill = rgb(surface);
        v.extreme_bg_color = rgb(background);
        v.faint_bg_color = rgb(surface);
        v.override_text_color = Some(rgb(text));
        v.selection.bg_fill = self
            .accent()
            .gamma_multiply(if light { 0.18 } else { 0.28 });
        v.selection.stroke = Stroke::new(1_f32, self.accent());
        v.hyperlink_color = self.accent();
        v.window_stroke = Stroke::new(1_f32, rgb(border));
        v.widgets.noninteractive.bg_fill = rgb(surface);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1_f32, rgb(border));
        v.widgets.noninteractive.fg_stroke = Stroke::new(1_f32, rgb(text));
        for w in [
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = egui::CornerRadius::same(7);
            w.bg_fill = rgb(raised);
            w.weak_bg_fill = rgb(surface);
            w.bg_stroke = Stroke::new(1_f32, rgb(border));
            w.fg_stroke = Stroke::new(1_f32, rgb(text));
        }
        v.widgets.hovered.bg_stroke = Stroke::new(1_f32, self.accent());
        v.widgets.active.bg_stroke = Stroke::new(1.5_f32, self.accent());
        style.visuals = v;
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
    }
    pub fn preferences_path() -> Option<PathBuf> {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
            .map(|p| p.join("rusty-boss/appearance.json"))
    }
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn save(self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_vec(&self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn four_distinct_palettes_and_only_justice_is_light() {
        let ctx = egui::Context::default();
        for theme in Theme::ALL {
            theme.apply(&ctx);
            assert_eq!(ctx.style().visuals.dark_mode, theme != Theme::Justice);
            assert_eq!(ctx.style().visuals.selection.stroke.color, theme.accent());
            let v = &ctx.style().visuals;
            let luminance = |c: Color32| (c.r() as i32 + c.g() as i32 + c.b() as i32) / 3;
            assert!((luminance(v.text_color()) - luminance(v.panel_fill)).abs() > 130);
        }
    }
    #[test]
    fn preferences_roundtrip_and_bad_settings_fall_back_to_puppet() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("appearance.json");
        assert_eq!(Theme::load(&p), Theme::Puppet);
        for theme in Theme::ALL {
            theme.save(&p).unwrap();
            assert_eq!(Theme::load(&p), theme);
        }
        std::fs::write(&p, b"not json").unwrap();
        assert_eq!(Theme::load(&p), Theme::Puppet);
    }
}
