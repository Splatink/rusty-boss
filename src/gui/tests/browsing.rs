//! Search is a local view: indices, selection, and write permissions stay intact.

#[test]
fn sidebar_controls_fit_narrow_content_widths() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    app.library.name = "A very long collection name that must not widen the sidebar".into();
    app.library.data[0] = vec![named(&app, "WWWWWWWWWWWWWWWW")];
    for theme in Theme::ALL {
        theme.apply(&ctx);
        for width in [174., 219., 294.] {
            for control in 0..4 {
                for _ in 0..2 {
                    run_ui(&ctx, egui::RawInput::default(), |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            ui.scope_builder(
                                egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                                    ui.cursor().min,
                                    egui::vec2(width, 500.),
                                )),
                                |ui| {
                                    ui.set_width(width);
                                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                                    match control {
                                        0 => app.collection_picker(ui),
                                        1 => app.tone_search(ui),
                                        2 => app.tone_list(ui),
                                        _ => app.channel_cards(ui),
                                    }
                                    assert!(
                                        ui.min_rect().width() <= width + 0.1,
                                        "control {control}: {} > {width}",
                                        ui.min_rect().width()
                                    );
                                },
                            );
                        });
                    });
                }
            }
        }
    }
}
use super::*;

#[test]
fn search_matches_names_and_notes_without_reindexing_or_selecting() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    let mut clean = named(&app, "Glass Clean");
    clean.memo = serde_json::json!({"memo": "Warm chorus"});
    app.library.data[0] = vec![clean, named(&app, "Heavy Lead"), named(&app, "Warm Crunch")];
    app.selected = 1;
    let before = app.library.data[0].clone();
    for (query, expected) in [
        ("", vec![0, 1, 2]),
        ("   ", vec![0, 1, 2]),
        ("  CLEAN ", vec![0]),
        ("warm", vec![0, 2]),
        ("HEAVY", vec![1]),
        ("missing", vec![]),
    ] {
        app.tone_query = query.into();
        assert_eq!(app.filtered_tones(), expected);
        render(&mut app, &ctx, 800., 560.);
        assert_eq!(app.selected, 1);
        assert_eq!(app.write_gate.access(), Access::ReadOnly);
        assert!(app.pending_edits.is_empty());
    }
    assert_eq!(
        serde_json::to_value(&app.library.data[0]).unwrap(),
        serde_json::to_value(before).unwrap()
    );
}

#[test]
fn collection_switch_clears_search_but_blocked_switch_preserves_it() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    app.tone_query = "clean".into();
    app.busy = true;
    app.switch_collection(0);
    assert_eq!(app.tone_query, "clean");
    app.busy = false;
    app.switch_collection(0);
    assert!(app.tone_query.is_empty());
}

#[test]
fn search_empty_states_and_save_errors_render_in_all_themes() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    app.library_error = Some("Local save failed. Export your work before closing.".into());
    for theme in Theme::ALL {
        app.theme = theme;
        theme.apply(&ctx);
        for page in [Page::Editor, Page::Library, Page::Settings] {
            app.page = page;
            app.tone_query = "no matching tone".into();
            assert!(!render(&mut app, &ctx, 800., 560.).shapes.is_empty());
        }
    }
    app.library.data[0].clear();
    app.page = Page::Library;
    render(&mut app, &ctx, 800., 560.);
    assert!(app.connected.is_none());
    assert_eq!(app.write_gate.access(), Access::ReadOnly);
}
