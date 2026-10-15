//! Medley: A and B take turns by section, like a DJ set. Scenes alternate
//! A's next section and B's next section (from the variation's
//! `scene_shift` on), so launching them in order plays one after the other.

use anyhow::Result;

use super::pipeline::{Mix, Stage};

pub struct Medley;

impl Stage for Medley {
    fn run(&self, mix: &mut Mix) -> Result<()> {
        for var in 0..mix.variations.len() {
            let shift = mix.variations[var].twist.scene_shift;
            // Twice layering's block: one turn each per section.
            let rows = (0..2 * mix.block_len())
                .map(|j| match j % 2 {
                    0 => [mix.section(0, j / 2), None],
                    _ => [None, mix.section(1, j / 2 + shift)],
                })
                .collect();
            mix.lay_out(var, rows);
        }
        Ok(())
    }
}
