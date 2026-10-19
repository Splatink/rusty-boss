//! Offline UI/state tests. These never open a MIDI port or run a vendor app.
#[path = "tests/advanced.rs"]
mod advanced;
use super::*;

fn render(app: &mut App, ctx: &egui::Context, width: f32, height: f32) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, height),
            )),
            ..Default::default()
        },
        |ctx| {
            app.header(ctx);
            if app.page != Page::Settings {
                app.preset_sidebar(ctx);
            }
            match app.page {
                Page::Editor => app.classic_editor(ctx),
                Page::Library => {
                    egui::CentralPanel::default().show(ctx, |ui| app.library_page(ui));
                }
                Page::Settings => {
                    egui::CentralPanel::default().show(ctx, |ui| app.connection(ui));
                }
            }
            app.confirmation(ctx);
        },
    )
}
#[test]
fn themes_pages_and_sizes_render_without_changing_tones_or_permissions() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    let before = app.library.data[0][0].parameters.clone();
    for theme in Theme::ALL {
        app.theme = theme;
        theme.apply(&ctx);
        for (width, height) in [(800., 560.), (1180., 800.), (1600., 1000.)] {
            for page in [Page::Editor, Page::Library, Page::Settings] {
                app.page = page;
                assert!(!render(&mut app, &ctx, width, height).shapes.is_empty());
                assert_eq!(app.library.data[0][0].parameters, before);
                assert_eq!(app.write_gate.access(), Access::ReadOnly);
            }
        }
    }
    app.library.data[0].clear();
    for page in [Page::Editor, Page::Library, Page::Settings] {
        app.page = page;
        render(&mut app, &ctx, 800., 560.);
    }
    assert!(app.connected.is_none() && !app.live && app.pending_edits.is_empty());
    #[cfg(feature = "midi")]
    assert!(app.ports.is_empty());
}
#[test]
fn every_algorithm_color_and_model_browse_is_inert() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    app.library.data[0][0]
        .parameters
        .get_mut("PATCH%PATCH_KNOB_READONLY")
        .unwrap()[4] = 127;
    for model in 5..=11 {
        app.offline_model = model;
        for section in Section::ALL {
            app.section = section;
            let before = app.library.data[0][0].parameters.clone();
            render(&mut app, &ctx, 1180., 800.);
            assert_eq!(app.library.data[0][0].parameters, before);
        }
    }
    for (section, count) in [
        (Section::Mod, 31),
        (Section::Fx, 31),
        (Section::Delay, 8),
        (Section::Delay2, 8),
        (Section::Reverb, 5),
        (Section::Eq, 2),
        (Section::Eq2, 2),
        (Section::Pedal, 3),
        (Section::Booster, 23),
    ] {
        app.section = section;
        for color in 0..3 {
            if let Some(i) = section.color_index() {
                app.library.data[0][0]
                    .parameters
                    .get_mut("PATCH%COLOR")
                    .unwrap()[i] = color;
            }
            for value in 0..count {
                let c = app.remap_controls(
                    section,
                    app.catalog.control(app.selector(section).unwrap()).unwrap(),
                );
                c.set(&app.schema, &mut app.library.data[0][0].parameters, value)
                    .unwrap();
                let before = app.library.data[0][0].parameters.clone();
                render(&mut app, &ctx, 1180., 800.);
                assert_eq!(app.library.data[0][0].parameters, before);
            }
        }
    }
    assert!(!app.dirty && !app.live && app.pending_edits.is_empty());
}
#[test]
fn edits_coalesce_and_readonly_revokes_pending_work() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    app.write_gate.set(Access::EditBuffer);
    app.live = true;
    for (index, v) in [(0, 10), (1, 20), (0, 30)] {
        app.record_edit(Edit {
            block: "PATCH%AMP".into(),
            index,
            bytes: vec![v],
        });
    }
    assert_eq!(app.pending_edits.len(), 2);
    assert_eq!(app.pending_edits.back().unwrap().bytes, [30]);
    app.set_access(Access::ReadOnly);
    assert!(app.pending_edits.is_empty() && app.pending_preview.is_none() && !app.live);
}
#[test]
fn importing_is_local_and_failed_import_preserves_work() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("tones.tsl");
    let mut set = LiveSet::new("Downloaded");
    let mut p = Patch::new(&app.schema).unwrap();
    p.rename(&app.schema, "Downloaded tone").unwrap();
    set.data[0].push(p);
    set.save(&file).unwrap();
    app.file = file.to_string_lossy().into_owned();
    app.dirty = true;
    app.import_file();
    assert!(app.confirm.is_none());
    assert_eq!(app.library.sets.len(), 2);
    assert_eq!(app.library.sets[0].name, "My tones");
    assert_eq!(app.library.name, "Downloaded");
    assert!(!app.busy && !app.live && app.dirty);
    app.dirty = true;
    app.file = "/no/such/file.tsl".into();
    app.confirm_action("import-path");
    assert_eq!(app.library.name, "Downloaded");
    assert!(app.dirty);
}

#[cfg(feature = "midi")]
fn fake_connected(app: &mut App) -> Receiver<Job> {
    // Replace the idle worker with a channel collector. No hardware, not even enumeration.
    let (tx, jobs) = mpsc::channel();
    let (_events, rx) = mpsc::channel();
    app.worker = Worker {
        tx,
        rx,
        cancel: Arc::new(AtomicBool::new(false)),
        handle: None,
    };
    app.connected = Some(Identity {
        device: 16,
        model_code: 5,
        name: "KATANA-50",
    });
    jobs
}
#[cfg(feature = "midi")]
fn named(app: &App, name: &str) -> Patch {
    let mut p = Patch::new(&app.schema).unwrap();
    p.rename(&app.schema, name).unwrap();
    p
}
#[cfg(feature = "midi")]
#[test]
fn click_previews_once_and_automatically_arms_only_after_readback() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    let jobs = fake_connected(&mut app);
    app.choose_preset(0);
    assert!(jobs.try_recv().is_err()); // Read-only is absolute for tone changes.
    app.set_access(Access::EditBuffer);
    app.choose_preset(0);
    let r = match jobs.try_recv().unwrap() {
        Job::Preview(r) => r,
        _ => panic!("expected preview"),
    };
    assert_eq!(r.patch.parameters, app.library.data[0][0].parameters);
    assert!(app.busy && !app.live);
    app.handle_event(Event::Previewed(r.generation, r.patch.parameters));
    assert!(app.live && app.synced && !app.busy);
    assert!(jobs.try_recv().is_err()); // No saved-channel commit follows preview.
}
#[cfg(feature = "midi")]
#[test]
fn rapid_clicks_preview_latest_and_stale_completion_cannot_arm_live() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    let jobs = fake_connected(&mut app);
    let second = named(&app, "Second");
    let third = named(&app, "Third");
    app.library.data[0].extend([second, third]);
    app.set_access(Access::EditBuffer);
    app.choose_preset(0);
    let first = match jobs.try_recv().unwrap() {
        Job::Preview(r) => r,
        _ => panic!(),
    };
    app.choose_preset(1);
    app.choose_preset(2);
    assert!(jobs.try_recv().is_err());
    app.handle_event(Event::Previewed(first.generation, first.patch.parameters));
    let last = match jobs.try_recv().unwrap() {
        Job::Preview(r) => r,
        _ => panic!(),
    };
    assert_eq!(last.patch.name(), "Third");
    assert!(!app.live);
    app.set_access(Access::ReadOnly);
    app.handle_event(Event::Previewed(last.generation, last.patch.parameters));
    assert!(!app.live && !app.synced);
    assert!(jobs.try_recv().is_err());
}
#[cfg(feature = "midi")]
#[test]
fn drop_requires_permission_and_yes_saves_captured_tone_and_destination() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    let jobs = fake_connected(&mut app);
    let patch = named(&app, "Dragged tone");
    for access in [Access::ReadOnly, Access::EditBuffer] {
        app.set_access(access);
        app.request_save(patch.clone(), 1);
        assert!(app.pending_save.is_none() && app.confirm.is_none());
    }
    app.confirm_action("unlock"); // Simple affirmative action; no magic phrase.
    for slot in [3, 9] {
        app.request_save(patch.clone(), slot);
        assert!(app.pending_save.is_none());
    }
    app.request_save(patch.clone(), 1);
    assert!(jobs.try_recv().is_err()); // Drop only opens confirmation.
    assert_eq!(app.confirm.as_deref(), Some("save-channel"));
    app.slot = 5; // Destination must remain the one captured by the drop.
    app.library.data[0][0] = named(&app, "Different tone");
    app.confirm = None;
    app.confirm_action("save-channel");
    let save = match jobs.try_recv().unwrap() {
        Job::Save(r) => r,
        _ => panic!(),
    };
    assert_eq!(save.slot, 1);
    assert_eq!(save.patch.name(), "Dragged tone");
    app.handle_event(Event::Saved(save));
    assert_eq!(app.channel_names[1].as_deref(), Some("Dragged tone"));
    assert!(!app.live); // Don't link controls for a different selected tone.
    assert!(jobs.try_recv().is_err());
}
#[cfg(feature = "midi")]
#[test]
fn panel_save_requires_permission_and_confirmation() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    let jobs = fake_connected(&mut app);
    for access in [Access::ReadOnly, Access::EditBuffer] {
        app.set_access(access);
        app.request_save(named(&app, "Panel effects"), 0);
        assert!(app.pending_save.is_none());
        assert!(jobs.try_recv().is_err());
    }
    app.set_access(Access::Persistent);
    app.request_save(named(&app, "Panel effects"), 0);
    assert_eq!(app.confirm.as_deref(), Some("save-channel"));
    assert!(jobs.try_recv().is_err());
    app.confirm = None;
    app.confirm_action("save-channel");
    assert!(
        matches!(jobs.try_recv().unwrap(), Job::Save(r) if r.slot == 0 && r.patch.name() == "Panel effects")
    );
}
#[cfg(feature = "midi")]
#[test]
fn no_cancel_lock_busy_disconnect_and_errors_cannot_save_or_rearm() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    let jobs = fake_connected(&mut app);
    app.set_access(Access::Persistent);
    app.request_save(named(&app, "Tone"), 1);
    // No button or Escape discards the captured request without issuing a job.
    app.confirm = None;
    app.pending_save = None;
    assert!(jobs.try_recv().is_err());
    app.request_save(named(&app, "Tone"), 1);
    app.set_access(Access::ReadOnly);
    app.confirm = None;
    app.confirm_action("save-channel");
    assert!(jobs.try_recv().is_err());
    app.set_access(Access::Persistent);
    app.busy = true;
    app.request_save(named(&app, "Tone"), 1);
    assert!(app.pending_save.is_none());
    app.handle_event(Event::Error("lost acknowledgement".into()));
    assert!(app.connected.is_none() && !app.live && !app.synced && app.pending_preview.is_none());
    assert_eq!(app.write_gate.access(), Access::ReadOnly);
    assert!(jobs.try_recv().is_err());
}
#[cfg(feature = "midi")]
#[test]
fn hardware_read_keeps_downloaded_patch_and_does_not_preview_or_commit() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    let jobs = fake_connected(&mut app);
    let before = app.library.data[0][0].parameters.clone();
    app.handle_event(Event::Current(named(&app, "Amp sound")));
    assert_eq!(app.library.data[0].len(), 2);
    assert_eq!(app.library.data[0][0].parameters, before);
    assert_eq!(app.selected, 1);
    assert!(!app.live);
    assert!(jobs.try_recv().is_err());
}

#[cfg(feature = "midi")]
fn pointer_frame(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(600., 800.),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.set_max_width(250.);
                app.tone_list(ui);
                ui.separator();
                app.channel_cards(ui);
            });
        },
    )
}
#[cfg(feature = "midi")]
fn text_point(output: &egui::FullOutput, text: &str) -> egui::Pos2 {
    output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == text => Some(t.pos + egui::vec2(5., 7.)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("Missing label {text}"))
}
#[cfg(feature = "midi")]
fn pointer_button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        pressed,
        button: egui::PointerButton::Primary,
        modifiers: Default::default(),
    }
}
#[cfg(feature = "midi")]
#[test]
fn real_pointer_click_and_drag_drop_use_the_same_safe_workflow() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    let jobs = fake_connected(&mut app);
    app.library.data[0][0] = named(&app, "Drag source");
    app.set_access(Access::EditBuffer);
    let output = pointer_frame(&mut app, &ctx, vec![]);
    let source = text_point(&output, "01   Drag source");
    pointer_frame(
        &mut app,
        &ctx,
        vec![
            egui::Event::PointerMoved(source),
            pointer_button(source, true),
        ],
    );
    pointer_frame(&mut app, &ctx, vec![pointer_button(source, false)]);
    let preview = match jobs.try_recv().unwrap() {
        Job::Preview(r) => r,
        _ => panic!(),
    };
    app.handle_event(Event::Previewed(
        preview.generation,
        preview.patch.parameters,
    ));
    app.set_access(Access::Persistent);
    let output = pointer_frame(&mut app, &ctx, vec![]);
    let destination = text_point(&output, &slot_label(1));
    pointer_frame(
        &mut app,
        &ctx,
        vec![
            egui::Event::PointerMoved(source),
            pointer_button(source, true),
        ],
    );
    pointer_frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(source + egui::vec2(20., 0.))],
    );
    assert!(egui::DragAndDrop::has_payload_of_type::<PatchDrag>(&ctx));
    pointer_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(destination)]);
    pointer_frame(&mut app, &ctx, vec![pointer_button(destination, false)]);
    let request = app
        .pending_save
        .as_ref()
        .expect("dropping on a channel opens confirmation");
    assert_eq!(request.slot, 1);
    assert_eq!(request.patch.name(), "Drag source");
    assert_eq!(app.confirm.as_deref(), Some("save-channel"));
    assert!(jobs.try_recv().is_err()); // Neither click selection nor commit leaks through a drop.
}

#[cfg(feature = "midi")]
#[test]
fn channel_clicks_read_without_switching_when_protected_and_select_when_previewing() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    let jobs = fake_connected(&mut app);
    app.choose_channel(3);
    assert!(jobs.try_recv().is_err()); // Absent on KATANA-50.
    app.choose_channel(1);
    assert!(matches!(jobs.try_recv().unwrap(), Job::ReadSlot(1)));
    app.handle_event(Event::Slot(1, named(&app, "Read channel")));
    app.set_access(Access::EditBuffer);
    app.choose_channel(5);
    assert!(matches!(jobs.try_recv().unwrap(), Job::Select(5)));
    assert!(app.confirm.is_none());
}
#[cfg(feature = "midi")]
#[test]
fn large_liveset_keeps_tones_and_channel_drop_zone_visible_together() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    let _jobs = fake_connected(&mut app);
    app.connected.as_mut().unwrap().model_code = 6;
    for i in 0..128 {
        let p = named(&app, &format!("Tone {i}"));
        app.library.data[0].push(p);
    }
    for (w, h) in [(800., 560.), (1180., 800.)] {
        let output = render(&mut app, &ctx, w, h);
        for label in [
            format!("01   {}", app.library.data[0][0].name()),
            slot_label(1),
        ] {
            assert!(output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == label && s.clip_rect.contains(t.pos))),"not visible: {label}");
        }
    }
}
#[cfg(feature = "midi")]
#[test]
fn overwrite_modal_no_escape_and_yes_buttons_are_wired_without_typed_input() {
    fn frame(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 560.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |_| {});
                app.confirmation(ctx);
            },
        )
    }
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    let jobs = fake_connected(&mut app);
    app.set_access(Access::Persistent);
    app.request_save(named(&app, "Confirmed tone"), 1);
    frame(&mut app, &ctx, vec![]); // Size/placement settle.
    let out = frame(&mut app, &ctx, vec![]);
    let no = text_point(&out, "No");
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(no), pointer_button(no, true)],
    );
    frame(&mut app, &ctx, vec![pointer_button(no, false)]);
    assert!(app.confirm.is_none() && app.pending_save.is_none());
    assert!(jobs.try_recv().is_err());

    app.request_save(named(&app, "Confirmed tone"), 1);
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
    );
    assert!(app.confirm.is_none() && app.pending_save.is_none());
    assert!(jobs.try_recv().is_err());

    app.request_save(named(&app, "Confirmed tone"), 1);
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    let yes = text_point(&out, "Yes");
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(yes), pointer_button(yes, true)],
    );
    frame(&mut app, &ctx, vec![pointer_button(yes, false)]);
    assert!(
        matches!(jobs.try_recv().unwrap(),Job::Save(r) if r.slot == 1 && r.patch.name() == "Confirmed tone")
    );
}
