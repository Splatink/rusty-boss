use super::*;

fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}
fn frame(
    ctx: &egui::Context,
    c: &Control,
    value: &mut i32,
    events: Vec<egui::Event>,
    enabled: bool,
) -> (egui::FullOutput, bool, bool) {
    let mut result = (*value, false, false);
    let output = crate::gui::tests::run_ui(
        ctx,
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(400., 300.),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    result = rotary(ui, c, *value, 0, 100, Color32::WHITE);
                });
                ui.button("Other control").clicked();
            });
        },
    );
    *value = result.0;
    (output, result.1, result.2)
}
fn click(ctx: &egui::Context, c: &Control, value: &mut i32, pos: egui::Pos2) {
    for pressed in [true, false] {
        frame(
            ctx,
            c,
            value,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            true,
        );
    }
}

#[test]
fn dial_arrows_keep_focus_count_repeats_and_clamp_without_phantom_edits() {
    let ctx = egui::Context::default();
    let c = editor::Catalog::load()
        .control("panel-amp-gain-knob")
        .unwrap();
    let mut value = 50;
    let (output, _, _) = frame(&ctx, &c, &mut value, vec![], true);
    let center = output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Circle(circle) if circle.radius == 21. => Some(circle.center),
            _ => None,
        })
        .unwrap();
    click(&ctx, &c, &mut value, center);
    let focus = ctx.memory(|m| m.focused()).unwrap();
    for arrow in [
        egui::Key::ArrowUp,
        egui::Key::ArrowRight,
        egui::Key::ArrowDown,
        egui::Key::ArrowLeft,
    ] {
        for _ in 0..3 {
            let (_, changed, finished) = frame(&ctx, &c, &mut value, vec![key(arrow)], true);
            assert!(changed && finished);
            assert_eq!(ctx.memory(|m| m.focused()), Some(focus));
        }
    }
    assert_eq!(value, 50);
    frame(
        &ctx,
        &c,
        &mut value,
        vec![key(egui::Key::ArrowUp), key(egui::Key::ArrowUp)],
        true,
    );
    assert_eq!(value, 52);
    value = 100;
    let (_, changed, finished) = frame(&ctx, &c, &mut value, vec![key(egui::Key::ArrowUp)], true);
    assert!(!changed && !finished);
    frame(&ctx, &c, &mut value, vec![key(egui::Key::ArrowDown)], false);
    assert_eq!(value, 100);
}

#[test]
fn numeric_arrows_step_integers_and_blur_does_not_send_untouched_values() {
    let ctx = egui::Context::default();
    let c = editor::Catalog::load()
        .control("panel-amp-gain-knob")
        .unwrap();
    let mut value = 50;
    let (output, _, _) = frame(&ctx, &c, &mut value, vec![], true);
    let pos = output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == "50" => Some(t.pos + t.galley.size() / 2.),
            _ => None,
        })
        .unwrap();
    click(&ctx, &c, &mut value, pos);
    let (_, changed, finished) = frame(&ctx, &c, &mut value, vec![key(egui::Key::ArrowUp)], true);
    assert_eq!(value, 51);
    assert!(changed && finished);
    let (_, changed, finished) = frame(&ctx, &c, &mut value, vec![key(egui::Key::ArrowDown)], true);
    assert_eq!(value, 50);
    assert!(changed && finished);
    let (_, changed, finished) = frame(&ctx, &c, &mut value, vec![key(egui::Key::Tab)], true);
    assert!(
        !changed && !finished,
        "value={value}, changed={changed}, finished={finished}"
    );
}
