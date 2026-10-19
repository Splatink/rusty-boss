use super::*;

#[test]
fn diagram_drag_updates_routing_and_queues_only_changed_parameters() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    for (block, offset) in [
        ("PATCH%OTHER", 0),
        ("PATCH%PEDALFX_COM", 0),
        ("PATCH%SENDRETURN", 1),
        ("PATCH%EQ_EACH(1)", 0),
        ("PATCH%EQ_EACH(2)", 0),
    ] {
        let c = app.simple(block, offset, "Routing").unwrap();
        app.set_control(&c, 0);
    }
    app.set_access(Access::EditBuffer);
    app.live = true;
    app.synced = true;
    let mut frame = |events| {
        crate::gui::tests::run_ui(
            &ctx,
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(2000., 300.),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| app.chain_view(ui));
            },
        )
    };
    let output = frame(vec![]);
    let point = |label| {
        output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == label => {
                    Some(t.pos + t.galley.size() / 2.)
                }
                _ => None,
            })
            .unwrap()
    };
    let source = point("EQ");
    let destination = point("AMP");
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(vec![
        egui::Event::PointerMoved(source),
        button(source, true),
    ]);
    frame(vec![egui::Event::PointerMoved(destination)]);
    frame(vec![button(destination, false)]);
    assert_eq!(app.number("PATCH%EQ_EACH(1)", 0), 1);
    assert_eq!(app.number("PATCH%EQ_EACH(2)", 0), 0);
    assert_eq!(app.pending_edits.len(), 1);
    assert_eq!(app.pending_edits[0].block, "PATCH%EQ_EACH(1)");
    assert!(app.dirty);
}
