//! Trait fusion: every partnered B track plays its A partner's rhythm with
//! its own pitches. The partner's notes keep their times, lengths and
//! velocities and take B's keys in order, cycling round (see
//! `Variation::partners`). MIDI only: an audio slot is left as it is.

use anyhow::Result;

use super::notes::{self, Note};
use super::pipeline::{Mix, Stage};
use super::twist::clear_envelopes;

pub struct Fuse;

impl Stage for Fuse {
    fn run(&self, mix: &mut Mix) -> Result<()> {
        mix.rework_partnered(|own, partner| {
            let Some(own) = own else {
                return Ok(None);
            };
            let (Some(b), Some(a)) = (notes::read(own)?, notes::read(partner)?) else {
                return Ok(Some(own.to_string()));
            };
            let mut keys: Vec<(f64, i32)> = b.notes.iter().map(|n| (n.time, n.key)).collect();
            keys.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
            if keys.is_empty() {
                return Ok(Some(own.to_string()));
            }
            let mut rhythm = a.notes;
            rhythm.sort_by(|x, y| x.time.total_cmp(&y.time).then(x.key.cmp(&y.key)));
            let fused: Vec<Note> = rhythm
                .into_iter()
                .zip(keys.iter().cycle())
                .map(|(note, &(_, key))| Note { key, ..note })
                .collect();
            // The partner's clip carries its rhythm and loop; its envelopes
            // point at A's devices.
            notes::write(&clear_envelopes(partner)?, &fused, None).map(Some)
        })
    }
}
