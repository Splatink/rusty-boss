use rusty_boss::{
    document::Patch,
    editor::{Catalog, bloom, gafc, solo_delay},
    schema::Schema,
};
#[test]
fn all_recovered_ui_options_encode_without_corrupting_neighbor_bytes() {
    let s = Schema::load();
    let u = Catalog::load();
    let mut set = s.defaults("PATCH").unwrap();
    set.extend(s.defaults("SYSTEM").unwrap());
    for c in u.controls.values() {
        let (i, p) = c.parameter(&s).unwrap();
        let b = s.block(&c.block).unwrap();
        if b.readonly {
            continue;
        }
        for opt in &c.choices {
            let mut data = set.clone();
            let before = data[&c.block].clone();
            let edit = c
                .set(&s, &mut data, opt.value)
                .unwrap_or_else(|e| panic!("{} value {}: {e}", c.id, opt.value));
            assert_eq!(edit.index, i);
            assert_eq!(c.value(&s, &data).unwrap(), opt.value, "{}", c.id);
            assert_eq!(&data[&c.block][..p.offset], &before[..p.offset]);
            assert_eq!(
                &data[&c.block][p.offset + p.width..],
                &before[p.offset + p.width..]
            );
        }
    }
}
#[test]
fn display_units_and_direct_entry_are_inverse() {
    let u = Catalog::load();
    for c in u.controls.values().filter(|c| c.choices.is_empty()) {
        for value in [-12, 0, 12, 100] {
            if c.display == "off_negative" && value < -1 {
                continue;
            }
            assert_eq!(
                c.parse_text(&c.text(value)).unwrap(),
                value as f64,
                "{}: {}",
                c.id,
                c.text(value)
            );
        }
    }
}
#[test]
fn each_mod_fx_color_keeps_its_own_algorithm_and_parameters() {
    let s = Schema::load();
    let u = Catalog::load();
    let mut p = Patch::new(&s).unwrap();
    for bank in 1..=6 {
        let c = u
            .control("modfx-mod-type-select-box")
            .unwrap()
            .remap("FX(1)", &format!("FX({bank})"));
        c.set(&s, &mut p.parameters, (bank - 1) * 5).unwrap();
        let c = u
            .control("modfx-twah-sens-dial")
            .unwrap()
            .remap("FX_DETAIL(1)", &format!("FX_DETAIL({bank})"));
        c.set(&s, &mut p.parameters, bank * 10).unwrap();
    }
    for bank in 1..=6 {
        let c = u
            .control("modfx-mod-type-select-box")
            .unwrap()
            .remap("FX(1)", &format!("FX({bank})"));
        assert_eq!(c.value(&s, &p.parameters).unwrap(), (bank - 1) * 5);
        let c = u
            .control("modfx-twah-sens-dial")
            .unwrap()
            .remap("FX_DETAIL(1)", &format!("FX_DETAIL({bank})"));
        assert_eq!(c.value(&s, &p.parameters).unwrap(), bank * 10);
    }
}
#[test]
fn shipped_gen3_model_flags() {
    for model in 5..=11 {
        assert_eq!(bloom(model), [8, 9, 11].contains(&model));
        assert_eq!(solo_delay(model), [9, 11].contains(&model));
        assert_eq!(gafc(model), model != 5);
    }
}
