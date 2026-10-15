use super::*;

/// Eight standalone tracks: enough units for seeds to differ.
fn source_with_eight_tracks() -> String {
    project_xml_with_sends_pre(
        &(0..8)
            .map(|i| track_with_sends("MidiTrack", i + 1, &format!("T{i}"), &[], 100 + i * 10))
            .collect::<String>(),
        0,
    )
}

fn plan(n: usize, seed: u64) -> Vec<Variation> {
    let a = source_with_nested_groups();
    let b = source_with_eight_tracks();
    let docs = [
        roxmltree::Document::parse(&a).unwrap(),
        roxmltree::Document::parse(&b).unwrap(),
    ];
    plan_variations(&docs, &[], n, 1, seed).unwrap()
}

#[test]
fn the_same_seed_gives_the_same_plan_and_var_1_is_everything() {
    let first = plan(6, 42);
    assert_eq!(first, plan(6, 42));
    assert_eq!(first.len(), 6);
    let names: Vec<&str> = first.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(
        names,
        ["Var 1", "Var 2", "Var 3", "Var 4", "Var 5", "Var 6"]
    );
    assert_eq!(first[0].picks, [vec![0, 1, 2, 3, 4], (0..8).collect()]);
}

#[test]
fn later_variations_take_whole_units_from_both_sides() {
    let sources = [
        MashupSource {
            xml: source_with_nested_groups(),
            semitones: vec![],
        },
        MashupSource {
            xml: source_with_eight_tracks(),
            semitones: vec![],
        },
    ];
    for seed in 0..50 {
        let variations = plan(6, seed);
        for v in &variations[1..] {
            assert!(
                v.picks.iter().all(|p| !p.is_empty()),
                "seed {seed}: {} misses a side",
                v.name
            );
            // The outer group's unit is tracks 0-3: all of them or none.
            let a_group = v.picks[0].iter().filter(|&&i| i < 4).count();
            assert!(
                a_group == 0 || a_group == 4,
                "seed {seed}: {v:?} splits a unit"
            );
        }
        let merged = build_mashup(&sources, &variations).unwrap();
        assert_eq!(diagnostics::check(&merged), Vec::<String>::new());
    }
}

#[test]
fn different_seeds_give_different_plans() {
    assert_ne!(plan(6, 1), plan(6, 2));
}

#[test]
fn variations_outside_one_to_six_are_rejected() {
    let a = source_with_nested_groups();
    let docs = [roxmltree::Document::parse(&a).unwrap()];
    for n in [0, 7] {
        assert!(plan_variations(&docs, &[], n, 1, 1).is_err());
    }
    for heat in [0, 6] {
        assert!(plan_variations(&docs, &[], 2, heat, 1).is_err());
    }
}
