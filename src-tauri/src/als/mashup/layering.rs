//! Layering: A and B play at the same time. Scene j stacks B's section
//! j + the variation's `scene_shift` on A's section j, both wrapping round.

use anyhow::Result;

use super::pipeline::{Mix, Stage};

pub struct Layering;

impl Stage for Layering {
    fn run(&self, mix: &mut Mix) -> Result<()> {
        for var in 0..mix.variations.len() {
            let shift = mix.variations[var].twist.scene_shift;
            let rows = (0..mix.block_len())
                .map(|j| [mix.section(0, j), mix.section(1, j + shift)])
                .collect();
            mix.lay_out(var, rows);
        }
        Ok(())
    }
}
