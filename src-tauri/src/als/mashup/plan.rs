use super::{non_return_tracks, Variation};
use anyhow::{anyhow, Result};
use roxmltree::{Document, Node};
use std::collections::HashMap;

pub(super) fn track_group_id<'a>(track: Node<'a, '_>) -> Option<&'a str> {
    track
        .children()
        .find(|c| c.tag_name().name() == "TrackGroupId")
        .and_then(|n| n.attribute("Value"))
}

/// For each of `tracks` (one project's `non_return_tracks`), the index of
/// the top-level track its unit hangs from, following `TrackGroupId` up
/// through groups inside groups. A top-level track is its own top.
pub(super) fn unit_tops(tracks: &[Node]) -> Result<Vec<usize>> {
    let index_of: HashMap<&str, usize> = tracks
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t.attribute("Id").map(|id| (id, i)))
        .collect();
    (0..tracks.len())
        .map(|i| {
            let mut at = i;
            for _ in 0..=tracks.len() {
                match track_group_id(tracks[at]) {
                    None | Some("-1") => return Ok(at),
                    Some(group) => {
                        at = *index_of
                            .get(group)
                            .ok_or_else(|| anyhow!("track grouped under missing group {group}"))?
                    }
                }
            }
            Err(anyhow!("track grouping loops back on itself"))
        })
        .collect()
}

/// `picks` sorted into the source's own track order, after checking every
/// index is in range and every picked unit is picked whole.
pub(super) fn whole_units(picks: &[usize], tops: &[usize]) -> Result<Vec<usize>> {
    let mut picks = picks.to_vec();
    picks.sort_unstable();
    picks.dedup();
    if let Some(&idx) = picks.iter().find(|&&i| i >= tops.len()) {
        return Err(anyhow!("track index {idx} out of range for source project"));
    }
    for &idx in &picks {
        let top = tops[idx];
        if let Some(missing) =
            (0..tops.len()).find(|&j| tops[j] == top && picks.binary_search(&j).is_err())
        {
            return Err(anyhow!(
                "track {idx} is picked but track {missing} of the same group is not; groups are picked whole"
            ));
        }
    }
    Ok(picks)
}

/// `n` variations (1 to 6) over `sources`. Var 1 picks every unit. Each
/// later one keeps every unit with a 50% chance, but always at least one
/// unit from each source that has any, and is redrawn (up to 20 times) if
/// it repeats an earlier variation; a source with very few units can't
/// always give `n` different ones. The same seed always gives the same
/// plan.
pub fn plan_variations(sources: &[Document], n: usize, seed: u64) -> Result<Vec<Variation>> {
    if !(1..=6).contains(&n) {
        return Err(anyhow!("variations must be 1 to 6, got {n}"));
    }
    // units[s][u] = the track indices of source s's unit u, in order.
    let units = sources
        .iter()
        .map(|doc| {
            let tops = unit_tops(&non_return_tracks(doc)?)?;
            let mut units: Vec<Vec<usize>> = Vec::new();
            for (idx, &top) in tops.iter().enumerate() {
                match units.iter_mut().find(|u| tops[u[0]] == top) {
                    Some(unit) => unit.push(idx),
                    None => units.push(vec![idx]),
                }
            }
            Ok(units)
        })
        .collect::<Result<Vec<_>>>()?;

    // SplitMix64: small, fast and good enough to pick tracks, so no crate.
    let mut state = seed;
    let mut next = || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let z = (state ^ (state >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        let z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    let mut draw = || -> Vec<Vec<usize>> {
        units
            .iter()
            .map(|source_units| {
                let mut kept: Vec<usize> = (0..source_units.len())
                    .filter(|_| next() & 1 == 1)
                    .collect();
                if kept.is_empty() && !source_units.is_empty() {
                    kept.push((next() % source_units.len() as u64) as usize);
                }
                kept.iter()
                    .flat_map(|&u| source_units[u].iter().copied())
                    .collect()
            })
            .collect()
    };
    let mut plans: Vec<Vec<Vec<usize>>> = vec![units.iter().map(|u| u.concat()).collect()];
    while plans.len() < n {
        let mut plan = draw();
        for _ in 0..20 {
            if !plans.contains(&plan) {
                break;
            }
            plan = draw();
        }
        plans.push(plan);
    }

    Ok(plans
        .into_iter()
        .enumerate()
        .map(|(i, picks)| Variation {
            name: format!("Var {}", i + 1),
            picks,
        })
        .collect())
}
