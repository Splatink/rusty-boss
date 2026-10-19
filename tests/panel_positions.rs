//! Regression: physical macro position and algorithm Effect Level are distinct.
use rusty_boss::{
    document::Patch,
    editor::{Catalog, Section},
    schema::Schema,
};

#[test]
fn recorded_katana50_snapshot_has_panel_52_and_spring_level_68() {
    use rusty_boss::{
        document::ParamSet,
        protocol::{IDENTITY_REQUEST, Message},
    };
    let report: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/katana50-reverb-readonly.json")).unwrap();
    let set: ParamSet = serde_json::from_value(report["blocks"].clone()).unwrap();
    let s = Schema::load();
    let u = Catalog::load();
    assert_eq!(set["PATCH%COLOR"][4], 1); // red
    assert_eq!(set["PATCH%ASSIGN_KNOBS"][2], 0); // PRESET
    assert_eq!(u.panel_position(&s, &set, Section::Reverb), Some(52));
    let level = u
        .control("reverb-reverb-effect-level-dial")
        .unwrap()
        .remap("REVERB(1)", "REVERB(2)");
    assert_eq!(level.value(&s, &set).unwrap(), 68);
    let kind = u
        .control("reverb-type-select-box")
        .unwrap()
        .remap("REVERB(1)", "REVERB(2)");
    assert_eq!(kind.text(kind.value(&s, &set).unwrap()), "SPRING");
    assert_eq!(report["bookends_match"], true);
    let mut requests = 0;
    let mut identity_requests = 0;
    for frame in report["frames"].as_array().unwrap() {
        let bytes: Vec<u8> = serde_json::from_value(frame["bytes"].clone()).unwrap();
        if frame["direction"] == "tx" {
            if bytes == IDENTITY_REQUEST {
                identity_requests += 1;
            } else {
                assert!(matches!(
                    Message::decode(&bytes).unwrap(),
                    Message::Request { .. }
                ));
                requests += 1;
            }
        } else if bytes.get(1) == Some(&0x41) {
            assert!(matches!(
                Message::decode(&bytes).unwrap(),
                Message::Data { .. }
            ));
        }
    }
    assert_eq!((identity_requests, requests), (1, 8));
}

#[test]
fn reverb_panel_50_and_effect_level_68_are_independent() {
    let s = Schema::load();
    let u = Catalog::load();
    let mut patch = Patch::new(&s).unwrap();
    let level = u.control("reverb-reverb-effect-level-dial").unwrap();
    level.set(&s, &mut patch.parameters, 68).unwrap();
    // Shipped PATCH_KNOB_READONLY watcher: position 50 + wire bias 1.
    patch
        .parameters
        .get_mut("PATCH%PATCH_KNOB_READONLY")
        .unwrap()[4] = 51;
    let before = patch.parameters.clone();
    assert_eq!(
        u.panel_position(&s, &patch.parameters, Section::Reverb),
        Some(50)
    );
    assert_eq!(level.value(&s, &patch.parameters).unwrap(), 68);
    assert_eq!(patch.parameters, before);
    level.set(&s, &mut patch.parameters, 25).unwrap();
    assert_eq!(
        u.panel_position(&s, &patch.parameters, Section::Reverb),
        Some(50)
    );
}

#[test]
fn panel_sentinel_bias_bounds_and_missing_data() {
    let s = Schema::load();
    let u = Catalog::load();
    let mut patch = Patch::new(&s).unwrap();
    let sections = [
        Section::Booster,
        Section::Mod,
        Section::Fx,
        Section::Delay,
        Section::Reverb,
    ];
    for (i, section) in sections.into_iter().enumerate() {
        assert_eq!(u.panel_position(&s, &patch.parameters, section), None);
        for (wire, expected) in [
            (0, None),
            (1, Some(0)),
            (51, Some(50)),
            (101, Some(100)),
            (102, None),
            (127, None),
            (255, None),
        ] {
            patch
                .parameters
                .get_mut("PATCH%PATCH_KNOB_READONLY")
                .unwrap()[i] = wire;
            assert_eq!(u.panel_position(&s, &patch.parameters, section), expected);
        }
    }
    assert_eq!(
        u.panel_position(&s, &patch.parameters, Section::Delay2),
        None
    );
    patch
        .parameters
        .insert("PATCH%PATCH_KNOB_READONLY".into(), vec![]);
    assert_eq!(
        u.panel_position(&s, &patch.parameters, Section::Reverb),
        None
    );
    patch.parameters.remove("PATCH%PATCH_KNOB_READONLY");
    assert_eq!(
        u.panel_position(&s, &patch.parameters, Section::Reverb),
        None
    );
}

#[test]
fn panel_watchers_remain_readonly_and_no_writable_level_is_substituted() {
    let s = Schema::load();
    assert!(s.block("PATCH%PATCH_KNOB_READONLY").unwrap().readonly);
    let u = Catalog::load();
    let mut patch = Patch::new(&s).unwrap();
    for bank in 1..=3 {
        let level = u
            .control("reverb-reverb-effect-level-dial")
            .unwrap()
            .remap("REVERB(1)", &format!("REVERB({bank})"));
        level.set(&s, &mut patch.parameters, bank * 20).unwrap();
    }
    assert_eq!(
        u.panel_position(&s, &patch.parameters, Section::Reverb),
        None
    );
}
