use super::*;

#[test]
fn collections_autosave_without_export_and_switch_without_amp_traffic() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.json");
    app.library_path = Some(path.clone());
    app.library.name = "Saved collection".into();
    app.dirty = true;
    app.persist_library();
    assert!(!app.dirty && app.library_error.is_none());
    let loaded = Library::load(&path, &app.schema).unwrap();
    assert_eq!(loaded.name, "Saved collection");
    app.library.add(LiveSet::new("Another set")).unwrap();
    app.live = true;
    app.synced = true;
    app.switch_collection(0);
    assert!(!app.live && !app.synced && app.tone_selection.is_empty());
    app.persist_library();
    assert_eq!(Library::load(&path, &app.schema).unwrap().sets.len(), 2);
    std::fs::write(&path, b"broken").unwrap();
    app.dirty = true;
    app.persist_library();
    assert!(app.dirty && app.library_error.is_some());
    assert_eq!(std::fs::read(&path).unwrap(), b"broken");
}
#[test]
fn settings_and_controllers_render_without_implicit_changes() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    let original = app.schema.defaults("SYSTEM").unwrap();
    app.system = Some(SystemDraft {
        before: original.clone(),
        values: original.clone(),
    });
    let tone = app.library.data[0][0].parameters.clone();
    for model in 5..=11 {
        app.offline_model = model;
        for page in 0..5 {
            app.system_page = page;
            let _ = crate::gui::tests::run_ui(&ctx, egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| match page {
                    0 => app.global_eq_settings(ui),
                    1 => app.system_group(ui, "system-line-out-setting-page", 1),
                    2 => app.system_group(ui, "system-usb-setting-page", 1),
                    3 => app.system_group(ui, "system-midi-setting-page", 1),
                    _ => app.system_group(ui, "system-gafc-page", 1),
                });
            });
            assert_eq!(app.system.as_ref().unwrap().values, original);
        }
        app.section = Section::Assignments;
        for controller in 0..4 {
            app.assign_controller = controller;
            render(&mut app, &ctx, 800., 560.);
        }
    }
    assert_eq!(app.library.data[0][0].parameters, tone);
    assert_eq!(app.write_gate.access(), Access::ReadOnly);
    assert!(!app.busy && !app.dirty && app.pending_edits.is_empty());
}
#[cfg(feature = "midi")]
#[test]
fn bulk_writes_require_permission_confirmation_and_keep_snapshot_order() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    let jobs = fake_connected(&mut app);
    app.library.data[0] = vec![named(&app, "One"), named(&app, "Two")];
    app.tone_selection = BTreeSet::from([0, 1]);
    app.amp_selection = BTreeSet::from([1, 2]);
    app.request_bulk_save();
    assert!(app.pending_bulk.is_empty());
    assert!(jobs.try_recv().is_err());
    app.set_access(Access::Persistent);
    app.request_bulk_save();
    assert_eq!(app.confirm.as_deref(), Some("save-many"));
    assert!(jobs.try_recv().is_err());
    app.library.data[0][0] = named(&app, "Later edit");
    app.confirm = None;
    app.confirm_action("save-many");
    match jobs.try_recv().unwrap() {
        Job::SaveMany(patches) => {
            assert_eq!(
                patches.iter().map(|(slot, _)| *slot).collect::<Vec<_>>(),
                [1, 2]
            );
            assert_eq!(patches[0].1.name(), "One");
            assert_eq!(patches[1].1.name(), "Two");
        }
        _ => panic!("expected captured bulk save"),
    }
    assert!(jobs.try_recv().is_err());
}
#[cfg(feature = "midi")]
#[test]
fn applying_settings_and_restoring_require_persistent_permission() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    let jobs = fake_connected(&mut app);
    let before = app.schema.defaults("SYSTEM").unwrap();
    let mut values = before.clone();
    values.get_mut("SYSTEM%USB_COM").unwrap()[2] = 73;
    app.system = Some(SystemDraft {
        before: before.clone(),
        values: values.clone(),
    });
    app.set_access(Access::EditBuffer);
    app.confirm_action("apply-system");
    assert!(jobs.try_recv().is_err());
    app.set_access(Access::Persistent);
    app.confirm_action("apply-system");
    match jobs.try_recv().unwrap() {
        Job::ApplySystem(draft) => {
            assert_eq!(draft.before, before);
            assert_eq!(draft.values, values);
        }
        _ => panic!("expected staged settings"),
    }
    app.busy = false;
    app.set_access(Access::ReadOnly);
    app.backup = Some(Backup {
        revision: "0000".into(),
        device: crate::document::DEVICE.into(),
        model: "KATANA-50".into(),
        system: before,
        setup: ParamSet::new(),
        patches: vec![],
        extra: BTreeMap::new(),
    });
    app.confirm_action("restore");
    assert!(jobs.try_recv().is_err());
    app.set_access(Access::Persistent);
    app.confirm_action("restore");
    assert!(matches!(jobs.try_recv().unwrap(), Job::Restore(_)));
    app.handle_event(Event::Progress(1, 10));
    assert!(app.busy);
    app.handle_event(Event::Error("cancelled".into()));
    assert_eq!(app.write_gate.access(), Access::ReadOnly);
    assert!(!app.live && !app.busy);
}
#[cfg(feature = "midi")]
#[test]
fn coordinated_readback_preserves_unsent_edits_and_cannot_rearm_after_lock() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    let _jobs = fake_connected(&mut app);
    app.set_access(Access::EditBuffer);
    app.synced = true;
    app.live = true;
    let c = app.catalog.control("panel-amp-gain-knob").unwrap();
    let mut values = app.library.data[0][0].parameters.clone();
    app.library.data[0][0]
        .parameters
        .insert("PATCH%FUTURE".into(), vec![1, 2, 3]);
    let edit = c
        .set(&app.schema, &mut app.library.data[0][0].parameters, 78)
        .unwrap();
    app.pending_edits.push_back(edit);
    c.set(&app.schema, &mut values, 31).unwrap();
    app.handle_event(Event::Refreshed(app.generation, values.clone()));
    assert_eq!(
        c.value(&app.schema, &app.library.data[0][0].parameters)
            .unwrap(),
        78
    );
    assert_eq!(app.library.data[0][0].parameters["PATCH%FUTURE"], [1, 2, 3]);
    let generation = app.generation;
    app.set_access(Access::ReadOnly);
    app.handle_event(Event::Refreshed(generation, values));
    assert!(!app.live);
    assert_eq!(
        c.value(&app.schema, &app.library.data[0][0].parameters)
            .unwrap(),
        78
    );
}
#[cfg(feature = "midi")]
#[test]
fn tap_does_not_wait_for_a_full_snapshot_or_disarm_live_editing() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    let jobs = fake_connected(&mut app);
    app.set_access(Access::EditBuffer);
    app.live = true;
    app.synced = true;
    app.submit(Job::Tap(0));
    assert!(app.live && app.write_busy);
    assert!(matches!(jobs.try_recv().unwrap(), Job::Tap(0)));
    app.handle_event(Event::Done("Tap sent".into()));
    assert!(!app.busy && app.live);
    app.submit(Job::Tap(0));
    assert!(matches!(jobs.try_recv().unwrap(), Job::Tap(0)));
}
#[cfg(feature = "midi")]
#[test]
fn multi_block_notifications_preserve_pending_edits_and_channel_changes_disarm() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    let _jobs = fake_connected(&mut app);
    app.set_access(Access::EditBuffer);
    app.synced = true;
    app.live = true;
    let gain = app.catalog.control("panel-amp-gain-knob").unwrap();
    let edit = gain
        .set(&app.schema, &mut app.library.data[0][0].parameters, 78)
        .unwrap();
    app.pending_edits.push_back(edit);
    let amp = app.schema.block("PATCH%AMP").unwrap();
    let switches = app.schema.block("PATCH%SW").unwrap();
    let start = amp.address as usize - 1;
    let end = switches.address as usize + switches.size;
    app.handle_event(Event::Update(Message::Data {
        device: 0x10,
        address: crate::protocol::Address::new(start as u32).unwrap(),
        data: vec![0; end - start],
    }));
    assert_eq!(
        gain.value(&app.schema, &app.library.data[0][0].parameters)
            .unwrap(),
        78
    );
    assert!(
        app.library.data[0][0].parameters["PATCH%SW"]
            .iter()
            .all(|v| *v == 0)
    );
    assert_eq!(app.library.data[0][0].parameters["PATCH%AMP"][3], 0);
    app.handle_event(Event::Update(Message::Data {
        device: 0x10,
        address: crate::protocol::Address::new(0).unwrap(),
        data: vec![0, 2],
    }));
    assert!(!app.live && !app.synced && app.pending_edits.is_empty());
}
#[test]
fn custom_icons_are_copied_validated_and_never_change_tones() {
    use eframe::icon_data::IconDataExt;
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx);
    let dir = tempfile::tempdir().unwrap();
    app.library_path = Some(dir.path().join("library.json"));
    let png = egui::IconData {
        rgba: vec![255, 0, 0, 255],
        width: 1,
        height: 1,
    }
    .to_png_bytes()
    .unwrap();
    let path = dir.path().join("chosen.png");
    std::fs::write(&path, &png).unwrap();
    let before = app.library.data[0][0].parameters.clone();
    let key = app.icon_key(Section::Booster).unwrap();
    app.import_icon(&key, &path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(app.effect_icon(&ctx, Section::Booster).is_some());
    assert_eq!(app.library.data[0][0].parameters, before);
    assert!(app.pending_edits.is_empty() && !app.dirty);
}
