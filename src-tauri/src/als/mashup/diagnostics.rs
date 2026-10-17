//! Structural checks against a `build_mashup` result, standing in for
//! "will Ableton Live refuse to load this file" without launching Live --
//! every invariant here is one that Live's own loader has actually
//! rejected during this project's development, so a clean result is
//! necessary but not sufficient proof the file opens cleanly. Shared
//! between `mashup.rs`'s own tests and the `mashup_batch_test` CLI so both
//! check exactly the same things.

use super::ids::max_referenced_id;
use super::plan::track_group_id;
use super::sends::mixer_sends;
use roxmltree::Document;
use std::collections::HashSet;

/// Human-readable problems found in `xml`; empty means every known
/// invariant holds.
pub fn check(xml: &str) -> Vec<String> {
    let doc = match Document::parse(xml) {
        Ok(doc) => doc,
        Err(e) => return vec![format!("XML does not parse: {e}")],
    };

    let mut problems = Vec::new();

    // `Id` is only meaningful as a unique key within specific lists --
    // e.g. every real, untouched, valid project already reuses Id
    // values across separate ClipSlot/KeyTrack/AutomationLane/etc.
    // scopes by the hundreds, so checking the whole document would flag
    // pristine files as corrupt. The one list this project has actually
    // seen Live reject on ("Non-unique list ids") is the top-level
    // <Tracks> list itself, so that's what's checked here.
    if let Ok(tracks_node) = super::tracks_node(&doc) {
        let ids: Vec<&str> = tracks_node
            .children()
            .filter(|n| n.is_element())
            .filter_map(|n| n.attribute("Id"))
            .collect();
        let mut unique_ids = ids.clone();
        unique_ids.sort();
        unique_ids.dedup();
        if unique_ids.len() != ids.len() {
            problems.push(format!(
                "duplicate Id among top-level tracks ({} tracks, {} unique Ids)",
                ids.len(),
                unique_ids.len()
            ));
        }

        // Live reads groups top-down: a track can only belong to a
        // group listed above it ("Track grouping corrupt" otherwise).
        let mut groups_so_far = HashSet::new();
        for track in tracks_node.children().filter(|n| n.is_element()) {
            if let Some(group) = track_group_id(track).filter(|g| *g != "-1") {
                if !groups_so_far.contains(group) {
                    problems.push(format!(
                        "{} Id={} has TrackGroupId {group}, which is not an earlier GroupTrack",
                        track.tag_name().name(),
                        track.attribute("Id").unwrap_or("?")
                    ));
                }
            }
            if track.tag_name().name() == "GroupTrack" {
                groups_so_far.extend(track.attribute("Id"));
            }
        }

        // Sends match returns by position, so every track (returns
        // included) needs exactly one per return, as does SendsPre.
        let return_count = tracks_node
            .children()
            .filter(|n| n.tag_name().name() == "ReturnTrack")
            .count();
        for track in tracks_node.children().filter(|n| n.is_element()) {
            if let Some(sends) = mixer_sends(track) {
                let count = sends.children().filter(|n| n.is_element()).count();
                if count != return_count {
                    problems.push(format!(
                        "{} Id={} has {count} sends but the set has {return_count} return tracks",
                        track.tag_name().name(),
                        track.attribute("Id").unwrap_or("?")
                    ));
                }
            }
        }
        if let Some(sends_pre) = doc
            .descendants()
            .find(|n| n.tag_name().name() == "SendsPre")
        {
            let count = sends_pre.children().filter(|n| n.is_element()).count();
            if count != return_count {
                problems.push(format!(
                    "SendsPre has {count} entries but the set has {return_count} return tracks"
                ));
            }
        }
    }

    if let Some(current) = doc
        .descendants()
        .find(|n| n.tag_name().name() == "NextPointeeId")
        .and_then(|n| n.attribute("Value"))
        .and_then(|v| v.parse::<u64>().ok())
    {
        let needed = max_referenced_id(&doc) + 1;
        // Live reads ids as 32-bit ints ("Unexpected value for int node").
        if current > i32::MAX as u64 {
            problems.push(format!(
                "NextPointeeId ({current}) is past Live's limit of {}",
                i32::MAX
            ));
        }
        if current < needed {
            problems.push(format!(
                "NextPointeeId ({current}) is too low; must be at least {needed}"
            ));
        }
    }

    problems
}
