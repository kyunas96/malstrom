//! Cut-up: every partnered B track alternates bars of its own notes and its
//! A partner's, so one instrument drifts between the two songs (see
//! `Variation::partners`). MIDI only: an audio slot is left as it is.

use anyhow::Result;

use super::notes::{self, Note, Notes};
use super::pipeline::{Mix, Stage};

pub struct CutUp;

/// ponytail: assumes 4/4. Read the set's time signature if odd meters matter.
const BAR: f64 = 4.0;

impl Stage for CutUp {
    fn run(&self, mix: &mut Mix) -> Result<()> {
        mix.rework_partnered(|own, partner| {
            let Some(own) = own else {
                return Ok(None);
            };
            let (Some(b), Some(a)) = (notes::read(own)?, notes::read(partner)?) else {
                return Ok(Some(own.to_string()));
            };
            // Long enough for both loops, and at least one bar of each.
            let length = (b.loop_end - b.loop_start)
                .max(a.loop_end - a.loop_start)
                .max(2.0 * BAR);
            let bars = ((length / BAR) / 2.0).ceil() as usize * 2;
            let cut: Vec<Note> = (0..bars)
                .flat_map(|k| bar(if k % 2 == 0 { &b } else { &a }, k, b.loop_start))
                .collect();
            notes::write(own, &cut, Some(b.loop_start + bars as f64 * BAR)).map(Some)
        })
    }
}

/// Bar `k` of `clip`'s loop (wrapping round), placed at bar `k` from `start`.
fn bar(clip: &Notes, k: usize, start: f64) -> Vec<Note> {
    let length = clip.loop_end - clip.loop_start;
    if length <= 0.0 {
        return vec![];
    }
    let from = clip.loop_start + (k as f64 * BAR) % length;
    clip.notes
        .iter()
        .filter(|n| n.time >= from && n.time < from + BAR)
        .map(|n| Note {
            time: start + k as f64 * BAR + (n.time - from),
            ..n.clone()
        })
        .collect()
}
