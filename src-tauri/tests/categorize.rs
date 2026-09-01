use malstrom_lib::als::{categorize_by_db_tags, categorize_by_name, TrackCategory};

#[test]
fn categorizes_by_name() {
    assert_eq!(categorize_by_name("Drums 2"), TrackCategory::Drums);
    assert_eq!(categorize_by_name("drum_bounce"), TrackCategory::Drums);
    assert_eq!(categorize_by_name("Lead Vocal"), TrackCategory::Vocals);
    assert_eq!(categorize_by_name("Synth 3"), TrackCategory::Other);
}

#[test]
fn categorizes_by_db_tags() {
    assert_eq!(
        categorize_by_db_tags(&["Kick".to_string()]),
        Some(TrackCategory::Drums)
    );
    assert_eq!(
        categorize_by_db_tags(&["Loop".to_string(), "Drum Loop".to_string()]),
        Some(TrackCategory::Drums)
    );
    assert_eq!(categorize_by_db_tags(&[]), None);
    assert_eq!(categorize_by_db_tags(&["Loop".to_string()]), None);
}
