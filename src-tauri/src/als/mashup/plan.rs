use super::session::filled_rows;
use super::transpose::is_pitched;
use super::{non_return_tracks, Speed, Twist, Variation, HEAT_LEVELS};
use crate::als::categorize::TrackCategory;
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

/// SplitMix64: small, fast and good enough to pick tracks, so no crate.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let z = (self.0 ^ (self.0 >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        let z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// A part that clashes when both sides play one at once.
#[derive(Clone, Copy, PartialEq)]
enum Role {
    Drums,
    Bass,
}

fn role(category: TrackCategory) -> Option<Role> {
    match category {
        TrackCategory::Drums | TrackCategory::Percussion => Some(Role::Drums),
        TrackCategory::Bass => Some(Role::Bass),
        _ => None,
    }
}

/// The twist for one of Var 2 … Var n at `heat` (1 to 5): every strategy in
/// `HEAT_LEVELS` up to it. `picks` is the variation's B tracks, `pitched[s]`
/// says which of source s's tracks are pitched, `midi[s]` which are MIDI
/// tracks, `donors` which of A's tracks can lend clips to a swap, and
/// `b_sections` how many sections B has (a shift only means something when
/// it has more than one).
fn draw_twist(
    rng: &mut Rng,
    heat: usize,
    picks: &[usize],
    pitched: &[Vec<bool>],
    midi: &[Vec<bool>],
    donors: &[usize],
    b_sections: usize,
) -> Twist {
    let mut twist = Twist::default();
    let is = |flags: &[Vec<bool>], s: usize, i: usize| {
        flags.get(s).and_then(|f| f.get(i)) == Some(&true)
    };
    if heat >= 2 && b_sections > 1 {
        twist.scene_shift = 1 + rng.below(b_sections - 1);
    }
    if heat >= 3 {
        let interval = [12, -12, 7, -5][rng.below(4)];
        twist.semitones = (0..midi.get(1).map_or(0, |m| m.len()))
            .map(|i| if is(pitched, 1, i) { interval } else { 0 })
            .collect();
    }
    if heat >= 4 {
        twist.speed = if rng.next() & 1 == 1 {
            Speed::Half
        } else {
            Speed::Double
        };
    }
    if heat >= 5 {
        let bs: Vec<usize> = picks
            .iter()
            .copied()
            .filter(|&i| is(pitched, 1, i) && is(midi, 1, i))
            .collect();
        if !bs.is_empty() && !donors.is_empty() {
            twist.swap = Some((bs[rng.below(bs.len())], donors[rng.below(donors.len())]));
        }
    }
    twist
}

/// `n` variations (1 to 6) over `sources` at `heat` (1 to 5). Var 1 picks
/// every unit and is never twisted, as a reference. Each later one keeps
/// every unit with a 50% chance, but always at least one unit from each
/// source that has any, and is redrawn (up to 20 times) if it repeats an
/// earlier variation's picks; a source with very few units can't always
/// give `n` different ones. When both sides pick drums, or both pick bass,
/// one side's are dropped. Above heat 1 each later variation also gets a
/// `Twist` of B (see `HEAT_LEVELS`). `categories[s]` holds source s's
/// track categories (see `track_categories`); a missing one counts as
/// unpitched and roleless. The same seed and heat always give the same plan.
pub fn plan_variations(
    sources: &[Document],
    categories: &[Vec<TrackCategory>],
    n: usize,
    heat: usize,
    seed: u64,
) -> Result<Vec<Variation>> {
    if !(1..=6).contains(&n) {
        return Err(anyhow!("variations must be 1 to 6, got {n}"));
    }
    if !(1..=HEAT_LEVELS.len()).contains(&heat) {
        return Err(anyhow!(
            "heat must be 1 to {}, got {heat}",
            HEAT_LEVELS.len()
        ));
    }
    let tracks = sources
        .iter()
        .map(non_return_tracks)
        .collect::<Result<Vec<_>>>()?;
    let pitched: Vec<Vec<bool>> = categories
        .iter()
        .map(|c| c.iter().map(|&c| is_pitched(c)).collect())
        .collect();
    let pitched = &pitched;
    let midi: Vec<Vec<bool>> = tracks
        .iter()
        .map(|t| {
            t.iter()
                .map(|n| n.tag_name().name() == "MidiTrack")
                .collect()
        })
        .collect();
    // A's pitched MIDI tracks with Session clips to lend.
    let donors: Vec<usize> = tracks.first().map_or(vec![], |a| {
        (0..a.len())
            .filter(|&i| midi[0][i] && pitched.first().and_then(|p| p.get(i)) == Some(&true))
            .filter(|&i| !filled_rows(&a[i..=i]).is_empty())
            .collect()
    });
    let b_sections = tracks.get(1).map_or(0, |b| filled_rows(b).len());
    // units[s][u] = the track indices of source s's unit u, in order.
    let units = tracks
        .iter()
        .map(|tracks| {
            let tops = unit_tops(tracks)?;
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

    let mut rng = Rng(seed);
    // roles[s][u]: the role every track in source s's unit u plays (group
    // tracks aside), if they share one.
    let roles: Vec<Vec<Option<Role>>> = units
        .iter()
        .enumerate()
        .map(|(s, source_units)| {
            source_units
                .iter()
                .map(|unit| {
                    let mut members = unit
                        .iter()
                        .filter(|&&i| tracks[s][i].tag_name().name() != "GroupTrack")
                        .map(|&i| {
                            categories
                                .get(s)
                                .and_then(|c| c.get(i))
                                .and_then(|&c| role(c))
                        });
                    let first = members.next()??;
                    members.all(|r| r == Some(first)).then_some(first)
                })
                .collect()
        })
        .collect();
    let draw = |rng: &mut Rng| -> Vec<Vec<usize>> {
        let mut kept: Vec<Vec<usize>> = units
            .iter()
            .map(|source_units| {
                let mut kept: Vec<usize> = (0..source_units.len())
                    .filter(|_| rng.next() & 1 == 1)
                    .collect();
                if kept.is_empty() && !source_units.is_empty() {
                    kept.push(rng.below(source_units.len()));
                }
                kept
            })
            .collect();
        // Two drum parts or two basslines fight, so when both sides
        // picked one, a random side drops its own. A side left with nothing
        // gets one of its units with no role instead; one with none keeps
        // its own.
        for role in [Role::Drums, Role::Bass] {
            let has =
                |kept: &[Vec<usize>], s: usize| kept[s].iter().any(|&u| roles[s][u] == Some(role));
            if kept.len() < 2 || !has(&kept, 0) || !has(&kept, 1) {
                continue;
            }
            let first = (rng.next() & 1) as usize;
            for side in [first, 1 - first] {
                let mut rest: Vec<usize> = kept[side]
                    .iter()
                    .copied()
                    .filter(|&u| roles[side][u] != Some(role))
                    .collect();
                if rest.is_empty() {
                    let spare: Vec<usize> = (0..units[side].len())
                        .filter(|&u| roles[side][u].is_none())
                        .collect();
                    rest.extend(spare.get(rng.below(spare.len().max(1))).copied());
                }
                if !rest.is_empty() {
                    kept[side] = rest;
                    break;
                }
            }
        }
        kept.iter()
            .zip(&units)
            .map(|(kept, source_units)| {
                kept.iter()
                    .flat_map(|&u| source_units[u].iter().copied())
                    .collect()
            })
            .collect()
    };
    let mut variations = vec![Variation {
        name: "Var 1".to_string(),
        picks: units.iter().map(|u| u.concat()).collect(),
        ..Default::default()
    }];
    while variations.len() < n {
        let mut picks = draw(&mut rng);
        for _ in 0..20 {
            if !variations.iter().any(|v| v.picks == picks) {
                break;
            }
            picks = draw(&mut rng);
        }
        let twist = if heat > 1 {
            draw_twist(
                &mut rng,
                heat,
                picks.get(1).map_or(&[], |p| p),
                pitched,
                &midi,
                &donors,
                b_sections,
            )
        } else {
            Twist::default()
        };
        let label = twist.label();
        let number = variations.len() + 1;
        variations.push(Variation {
            name: if label.is_empty() {
                format!("Var {number}")
            } else {
                format!("Var {number} · {label}")
            },
            picks,
            twist,
        });
    }
    Ok(variations)
}
