use super::*;
use crate::document::Patch;
#[test]
fn every_dynamic_target_has_a_named_menu_and_representable_range() {
    let schema = Schema::load();
    let catalog = Catalog::load();
    for (key, assignment) in &catalog.assignments {
        let knob = catalog.knob_assignment(key, key).unwrap();
        assert_eq!(knob.offset, assignment.knob_offset);
        assert_eq!(knob.choices.len(), assignment.targets.len());
        for (index, target) in assignment.targets.iter().enumerate() {
            assert!(!target.label.trim().is_empty(), "{key}/{index}");
            let mut patch = Patch::new(&schema).unwrap();
            knob.set(&schema, &mut patch.parameters, index as i32)
                .unwrap();
            let detail = catalog
                .control(&format!("asgn-pedal-{key}-select-box"))
                .unwrap();
            detail
                .set(&schema, &mut patch.parameters, index as i32)
                .unwrap();
            let (min, max) = if target.block.is_empty() {
                (0, 100)
            } else {
                target.range(&schema).unwrap()
            };
            for suffix in ["min", "max"] {
                let mut range = catalog
                    .control(&format!("asgn-pedal-{key}-{suffix}-spinner"))
                    .unwrap();
                range.bias = target.bias;
                range.min = Some(min);
                range.max = Some(max);
                for value in [min, max] {
                    range.set(&schema, &mut patch.parameters, value).unwrap();
                    assert_eq!(
                        range.value(&schema, &patch.parameters).unwrap(),
                        value,
                        "{key}/{index}"
                    );
                }
            }
        }
    }
}
#[test]
fn expression_controls_follow_algorithm_color_controller_and_expansion() {
    let schema = Schema::load();
    let catalog = Catalog::load();
    let mut patch = Patch::new(&schema).unwrap();
    for pedal in ["pedal", "exp1", "exp2", "exp3"] {
        for expanded in [false, true] {
            if pedal == "pedal" && expanded {
                continue;
            }
            let mut func = catalog
                .control(&format!("asgn-{pedal}-function-select-box"))
                .unwrap();
            if expanded {
                func = func.remap("(1)", "(2)");
            }
            for value in 0..10 {
                func.set(&schema, &mut patch.parameters, value).unwrap();
                for color in 0..3 {
                    patch.parameters.get_mut("PATCH%COLOR").unwrap()[1] = color;
                    patch.parameters.get_mut("PATCH%COLOR").unwrap()[2] = color;
                    for algorithm in 0..31 {
                        patch
                            .parameters
                            .get_mut(&format!("PATCH%FX({})", color + 1))
                            .unwrap()[0] = algorithm;
                        patch
                            .parameters
                            .get_mut(&format!("PATCH%FX({})", color + 4))
                            .unwrap()[0] = algorithm;
                        let controls = catalog
                            .pedal_assignment(&schema, &patch.parameters, pedal, expanded)
                            .unwrap();
                        assert_eq!(controls.len(), if value < 4 { 1 } else { 4 });
                        assert!(controls.iter().all(|c| !c.block.contains(if expanded {
                            "(1)"
                        } else {
                            "(2)"
                        })));
                        for c in controls.iter().skip(2) {
                            let (min, max) = c.range(&schema).unwrap();
                            let before = patch.parameters.clone();
                            c.set(&schema, &mut patch.parameters, min).unwrap();
                            assert_eq!(c.value(&schema, &patch.parameters).unwrap(), min);
                            c.set(&schema, &mut patch.parameters, max).unwrap();
                            assert_eq!(c.value(&schema, &patch.parameters).unwrap(), max);
                            patch.parameters = before;
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn signed_and_delay_targets_keep_their_wire_bias_and_units() {
    let schema = Schema::load();
    let catalog = Catalog::load();
    let mut patch = Patch::new(&schema).unwrap();
    catalog
        .control("asgn-pedal-function-select-box")
        .unwrap()
        .set(&schema, &mut patch.parameters, 4)
        .unwrap();
    catalog
        .control("asgn-pedal-booster-select-box")
        .unwrap()
        .set(&schema, &mut patch.parameters, 2)
        .unwrap();
    let controls = catalog
        .pedal_assignment(&schema, &patch.parameters, "pedal", false)
        .unwrap();
    let heel = &controls[2];
    assert_eq!(heel.range(&schema).unwrap(), (-50, 50));
    heel.set(&schema, &mut patch.parameters, -12).unwrap();
    assert_eq!(patch.parameters["PATCH%ASSIGN_EXPPDL_MIN"][0], 38);
    assert_eq!(heel.value(&schema, &patch.parameters).unwrap(), -12);
    catalog
        .control("asgn-pedal-function-select-box")
        .unwrap()
        .set(&schema, &mut patch.parameters, 7)
        .unwrap();
    catalog
        .control("asgn-pedal-delay-select-box")
        .unwrap()
        .set(&schema, &mut patch.parameters, 1)
        .unwrap();
    let controls = catalog
        .pedal_assignment(&schema, &patch.parameters, "pedal", false)
        .unwrap();
    assert_eq!(controls[2].display, "ms");
    controls[3]
        .set(&schema, &mut patch.parameters, 1500)
        .unwrap();
    assert_eq!(controls[3].value(&schema, &patch.parameters).unwrap(), 1500);
}
