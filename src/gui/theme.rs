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
            // Pale green is used for fills; text/knob accents need contrast on white.
            Self::Justice => Color32::from_rgb(43, 101, 68),
            Self::Kill => Color32::from_rgb(245, 79, 91),
            Self::Puppet => Color32::from_rgb(244, 166, 94),
            Self::Lightning => Color32::from_rgb(137, 183, 234),
        }
    }
    fn selection_accent(self) -> Color32 {
        match self {
            Self::Lightning => Color32::from_rgb(185, 149, 240),
            _ => self.accent(),
        }
    }
    fn selection_fill(self) -> Color32 {
        match self {
            Self::Justice => Color32::from_rgb(193, 223, 199),
            Self::Kill => Color32::from_rgb(60, 24, 33),
            Self::Puppet => Color32::from_rgb(87, 58, 36),
            Self::Lightning => Color32::from_rgb(58, 47, 86),
        }
    }
    pub fn apply(self, ctx: &egui::Context) {
        let light = self == Self::Justice;
        let (background, surface, raised, text, border) = match self {
            Self::Justice => (
                (244, 248, 242),
                (254, 255, 252),
                (227, 237, 226),
                (34, 48, 39),
                (186, 206, 187),
            ),
            Self::Kill => (
                (24, 15, 20),
                (35, 23, 29),
                (54, 32, 41),
                (245, 227, 231),
                (81, 47, 59),
            ),
            Self::Puppet => (
                (26, 21, 19),
                (39, 31, 27),
                (58, 45, 36),
                (239, 230, 216),
                (85, 66, 51),
            ),
            Self::Lightning => (
                (17, 19, 33),
                (27, 39, 58),
                (44, 59, 82),
                (226, 235, 247),
                (76, 94, 124),
            ),
        };
        let rgb = |(r, g, b)| Color32::from_rgb(r, g, b);
        let mut style = (*ctx.global_style()).clone();
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
        v.selection.bg_fill = self.selection_fill();
        v.selection.stroke = Stroke::new(1_f32, self.selection_accent());
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
        // Carry the palette into interaction states, not just the page background.
        let hover = match self {
            Self::Justice => Color32::from_rgb(216, 234, 217),
            Self::Kill => Color32::from_rgb(65, 34, 44),
            Self::Puppet => Color32::from_rgb(73, 52, 38),
            Self::Lightning => Color32::from_rgb(52, 61, 91),
        };
        v.widgets.hovered.bg_fill = hover;
        v.widgets.hovered.weak_bg_fill = hover;
        v.widgets.hovered.bg_stroke = Stroke::new(1_f32, self.selection_accent());
        v.widgets.active.bg_fill = self.selection_fill();
        v.widgets.active.weak_bg_fill = self.selection_fill();
        v.widgets.active.bg_stroke = Stroke::new(1.5_f32, self.selection_accent());
        v.widgets.open.bg_fill = self.selection_fill();
        v.widgets.open.weak_bg_fill = self.selection_fill();
        v.widgets.open.bg_stroke = Stroke::new(1_f32, self.selection_accent());
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
        ctx.set_global_style(style);
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
            assert_eq!(
                ctx.global_style().visuals.dark_mode,
                theme != Theme::Justice
            );
            assert_eq!(
                ctx.global_style().visuals.selection.stroke.color,
                theme.selection_accent()
            );
            let v = &ctx.global_style().visuals;
            let luminance = |c: Color32| (c.r() as i32 + c.g() as i32 + c.b() as i32) / 3;
            assert!((luminance(v.text_color()) - luminance(v.panel_fill)).abs() > 130);
        }
    }
    #[test]
    fn palettes_follow_the_requested_green_blue_purple_and_copper_directions() {
        let ctx = egui::Context::default();
        Theme::Lightning.apply(&ctx);
        let v = ctx.global_style().visuals.clone();
        let blue = Theme::Lightning.accent();
        let purple = v.selection.stroke.color;
        assert!(blue.b() > blue.g() && blue.g() > blue.r());
        assert!(purple.b() > purple.r() && purple.r() > purple.g());
        assert_ne!(blue, purple);
        assert_eq!(v.widgets.active.bg_stroke.color, purple);
        assert_eq!(v.widgets.hovered.bg_stroke.color, purple);

        // Switching to Justice must not retain Lightning's purple interaction colours.
        Theme::Justice.apply(&ctx);
        let v = ctx.global_style().visuals.clone();
        assert!(!v.dark_mode);
        assert!(v.window_fill.r() >= 250 && v.window_fill.g() >= 250 && v.window_fill.b() >= 250);
        for fill in [
            v.selection.bg_fill,
            v.widgets.hovered.bg_fill,
            v.widgets.active.bg_fill,
            v.widgets.open.bg_fill,
        ] {
            assert!(fill.r() >= 180 && fill.g() > fill.r() && fill.g() > fill.b());
        }
        for ink in [
            v.selection.stroke.color,
            v.widgets.hovered.bg_stroke.color,
            v.hyperlink_color,
        ] {
            assert!(ink.g() > ink.r() && ink.g() > ink.b());
        }

        Theme::Puppet.apply(&ctx);
        let v = ctx.global_style().visuals.clone();
        for surface in [
            v.panel_fill,
            v.window_fill,
            v.widgets.inactive.bg_fill,
            v.widgets.hovered.bg_fill,
            v.widgets.active.bg_fill,
            v.widgets.open.bg_fill,
            v.widgets.noninteractive.bg_stroke.color,
        ] {
            assert!(
                surface.r() > surface.g() && surface.g() > surface.b(),
                "Puppet must carry warm copper/earth tones through all surfaces"
            );
        }
    }
    #[test]
    fn text_and_selection_accents_have_readable_contrast() {
        fn luminance(c: Color32) -> f32 {
            let linear = |n: u8| {
                let v = n as f32 / 255.;
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
        }
        fn contrast(a: Color32, b: Color32) -> f32 {
            let a = luminance(a);
            let b = luminance(b);
            (a.max(b) + 0.05) / (a.min(b) + 0.05)
        }
        let ctx = egui::Context::default();
        for theme in Theme::ALL {
            theme.apply(&ctx);
            let v = ctx.global_style().visuals.clone();
            for bg in [
                v.panel_fill,
                v.window_fill,
                v.widgets.inactive.bg_fill,
                v.widgets.hovered.bg_fill,
            ] {
                assert!(contrast(v.text_color(), bg) >= 4.5, "{theme:?} body text");
            }
            assert!(
                contrast(theme.accent(), v.window_fill) >= 4.5,
                "{theme:?} accent text"
            );
            assert!(
                contrast(v.selection.stroke.color, v.selection.bg_fill) >= 4.5,
                "{theme:?} selection"
            );
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
