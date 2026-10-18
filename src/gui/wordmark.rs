//! Header typography uses egui's DPI-aware, antialiased font pipeline, not raw meshes.
//! Russo One is bundled unmodified under the SIL OFL; see assets/fonts/.
use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId, RichText};
use egui::emath::GuiRounding as _;

const FAMILY: &str = "rusty-boss-wordmark";
const FONT: &[u8] = include_bytes!("../../assets/fonts/RussoOne-Regular.ttf");
const SIZE: f32 = 26.;
const HEIGHT: f32 = 40.;
pub(super) const FONT_LICENSE: &str = include_str!("../../assets/fonts/OFL-RussoOne.txt");

/// Install only a dedicated display family. The rest of the UI keeps its fonts.
pub(super) fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert(FAMILY.into(), FontData::from_static(FONT).into());
    fonts
        .families
        .insert(FontFamily::Name(FAMILY.into()), vec![FAMILY.into()]);
    ctx.set_fonts(fonts);
}
fn layout(size: f32, ink: Color32, accent: Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    for (text, color) in [("RUSTY ", ink), ("BOSS", accent)] {
        job.append(
            text,
            0.,
            egui::TextFormat {
                font_id: FontId::new(size, FontFamily::Name(FAMILY.into())),
                color,
                extra_letter_spacing: 0.35,
                ..Default::default()
            },
        );
    }
    job
}
pub(super) fn show(ui: &mut egui::Ui, accent: Color32) -> egui::Response {
    let ink = ui.visuals().text_color();
    let mut size = SIZE;
    let mut galley = ui.fonts(|f| f.layout_job(layout(size, ink, accent)));
    let available = ui.available_width().max(1.);
    // Font hinting/letter spacing make width slightly nonlinear. Recheck the actual
    // layout rather than clipping a last letter after a single proportional fit.
    for _ in 0..4 {
        if galley.size().x <= available {
            break;
        }
        size *= (available - 1.).max(1.) / galley.size().x;
        galley = ui.fonts(|f| f.layout_job(layout(size, ink, accent)));
    }
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(galley.size().x.min(available), HEIGHT),
        egui::Sense::hover(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Label, ui.is_enabled(), "Rusty Boss")
    });
    if ui.is_rect_visible(rect) {
        let pos =
            (rect.center() - galley.size() * 0.5).round_to_pixels(ui.painter().pixels_per_point());
        ui.painter()
            .with_clip_rect(rect.intersect(ui.clip_rect()))
            .galley(pos, galley, ink);
    }
    response.on_hover_ui(|ui| {
        ui.label(RichText::new("Rusty Boss").strong());
        ui.small("Katana editor for Linux");
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::theme::Theme;

    #[test]
    fn live_dpi_changes_rerasterize_glyphs_instead_of_scaling_old_pixels() {
        let ctx = egui::Context::default();
        install(&ctx);
        let mut texel_widths = Vec::new();
        let mut logical_widths = Vec::new();
        // Same context, including a move back to 1x: exercise the monitor-change path.
        for ppp in [1., 1.5, 2., 3., 1.] {
            ctx.set_pixels_per_point(ppp);
            let output = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    show(ui, Theme::Puppet.accent());
                });
            });
            let text = output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.job.text == "RUSTY BOSS" => Some(t),
                    _ => None,
                })
                .expect("wordmark must be antialiased font text, not hand-built meshes");
            assert_eq!(text.galley.pixels_per_point, ppp);
            let uv = text.galley.rows[0].glyphs[0].uv_rect;
            texel_widths.push(uv.max[0] - uv.min[0]);
            logical_widths.push(text.galley.size().x);
        }
        for pair in texel_widths[..4].windows(2) {
            assert!(pair[1] > pair[0]);
        }
        assert_eq!(texel_widths[0], texel_widths[4]);
        let baseline = logical_widths[0];
        for width in logical_widths {
            assert!((width - baseline).abs() < 4.);
        }
    }
    #[test]
    fn dedicated_family_does_not_restyle_the_ui() {
        let ctx = egui::Context::default();
        let text_styles = ctx.style().text_styles.clone();
        let sample = || {
            egui::text::LayoutJob::simple(
                "Body text remains the same".into(),
                FontId::proportional(14.),
                Color32::WHITE,
                f32::INFINITY,
            )
        };
        let mut before = egui::Vec2::ZERO;
        let _ = ctx.run(Default::default(), |ctx| {
            before = ctx.fonts(|f| f.layout_job(sample())).size();
        });
        install(&ctx);
        let _ = ctx.run(Default::default(), |ctx| {
            assert_eq!(ctx.fonts(|f| f.layout_job(sample())).size(), before);
        });
        assert_eq!(ctx.style().text_styles, text_styles);
    }
    #[test]
    fn themes_and_constrained_widths_keep_a_readable_single_line() {
        let ctx = egui::Context::default();
        install(&ctx);
        for theme in Theme::ALL {
            theme.apply(&ctx);
            for ppp in [1., 2., 3.] {
                ctx.set_pixels_per_point(ppp);
                for width in [120., 180., 260.] {
                    let output = ctx.run(Default::default(), |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let area = egui::Rect::from_min_size(
                                ui.cursor().min,
                                egui::vec2(width, HEIGHT),
                            );
                            ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
                                let r = show(ui, theme.accent());
                                assert!(r.rect.width() <= width + 1.);
                            });
                        });
                    });
                    let text = output
                        .shapes
                        .iter()
                        .find_map(|s| match &s.shape {
                            egui::Shape::Text(t) if t.galley.job.text == "RUSTY BOSS" => Some(t),
                            _ => None,
                        })
                        .unwrap();
                    assert_eq!(text.galley.rows.len(), 1);
                    assert!(!text.galley.elided);
                    assert!(
                        text.galley.size().x <= width + 1.,
                        "{} > {width} at {ppp}x",
                        text.galley.size().x
                    );
                    assert_eq!(text.galley.job.sections[1].format.color, theme.accent());
                }
            }
        }
        assert!(FONT_LICENSE.contains("SIL OPEN FONT LICENSE"));
    }
}
