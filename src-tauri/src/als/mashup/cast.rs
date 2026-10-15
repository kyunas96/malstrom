//! Notes × instruments: every partnered B track keeps its instrument but
//! plays its A partner's clip, as A wrote it (see `Variation::partners`).

use anyhow::Result;

use super::pipeline::{Mix, Stage};
use super::twist::clear_envelopes;

pub struct Cast;

impl Stage for Cast {
    fn run(&self, mix: &mut Mix) -> Result<()> {
        mix.rework_partnered(|_, partner| clear_envelopes(partner).map(Some))
    }
}
