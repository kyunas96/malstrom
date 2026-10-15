//! A mashup as a pipeline: a list of stages run in order over one `Mix`.
//! Each paradigm is a module with one stage, and every stage has the same
//! interface, so a pipeline is composed by naming stages, e.g.
//! `medley,cast,twist`.
//!
//! A layout stage (`layering` or `medley`) goes first. It decides which of
//! A's and B's sections each scene plays, and fills every picked track's
//! slots from them. The stages after it rework those slots.

use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use roxmltree::Document;

use super::cast::Cast;
use super::cutup::CutUp;
use super::fuse::Fuse;
use super::layering::Layering;
use super::medley::Medley;
use super::session::clip_near;
use super::twist::TwistStage;
use super::Variation;

/// One step of a mashup. Every paradigm module exports one.
pub trait Stage {
    fn run(&self, mix: &mut Mix) -> Result<()>;
}

/// B's sections stacked on A's, then the heat twists.
pub const DEFAULT_PIPELINE: &str = "layering,twist";

/// The stages named in `names`, comma-separated, in order.
pub fn parse_pipeline(names: &str) -> Result<Vec<Box<dyn Stage>>> {
    names
        .split(',')
        .map(|name| -> Result<Box<dyn Stage>> {
            Ok(match name.trim() {
                "layering" => Box::new(Layering),
                "medley" => Box::new(Medley),
                "twist" => Box::new(TwistStage),
                "cast" => Box::new(Cast),
                "cutup" => Box::new(CutUp),
                "fuse" => Box::new(Fuse),
                other => {
                    return Err(anyhow!(
                        "no stage called {other:?}; stages are layering, medley, twist, cast, cutup, fuse"
                    ))
                }
            })
        })
        .collect()
}

/// The most scenes one variation gets.
const MAX_BLOCK: usize = 8;

/// One variation's scenes.
#[derive(Default)]
pub struct Block {
    /// A's and B's section in each scene, as scene rows of each source.
    /// `None` plays nothing from that side.
    pub rows: Vec<[Option<usize>; 2]>,
    /// The `<Value>` each picked track plays in each scene, keyed by
    /// (source, track). `None` leaves the slot empty.
    pub slots: BTreeMap<(usize, usize), Vec<Option<String>>>,
}

/// The mashup while it's built: one `Block` per variation, plus every
/// track's clip at every section. Stages read and rework it, then
/// `build_mashup` turns it into XML.
pub struct Mix<'v> {
    pub variations: &'v [Variation],
    /// `picks[v][s]`: variation v's tracks from source s.
    pub picks: Vec<Vec<Vec<usize>>>,
    /// A's and B's sections: the scene rows where they have clips.
    pub sections: [Vec<usize>; 2],
    pub blocks: Vec<Block>,
    /// `clips[s][track][row]`: the `<Value>` the track plays in that
    /// section (see `clip_near`).
    clips: Vec<Vec<BTreeMap<usize, String>>>,
}

impl<'v> Mix<'v> {
    /// `tracks[s]` holds source s's track blocks, already in A's key.
    pub(super) fn new(
        variations: &'v [Variation],
        picks: Vec<Vec<Vec<usize>>>,
        sections: [Vec<usize>; 2],
        tracks: &[Vec<String>],
    ) -> Result<Self> {
        let clips = tracks
            .iter()
            .enumerate()
            .map(|(s, blocks)| {
                blocks
                    .iter()
                    .map(|block| {
                        let doc = Document::parse(block)
                            .map_err(|e| anyhow!("XML parse error while reading clips: {e}"))?;
                        Ok(sections[s.min(1)]
                            .iter()
                            .filter_map(|&row| {
                                clip_near(doc.root_element(), row)
                                    .map(|c| (row, block[c.range()].to_string()))
                            })
                            .collect())
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Mix {
            variations,
            picks,
            sections,
            blocks: variations.iter().map(|_| Block::default()).collect(),
            clips,
        })
    }

    /// The clip track `track` of source `s` plays in section `row`.
    pub fn clip(&self, s: usize, track: usize, row: Option<usize>) -> Option<&str> {
        self.clips
            .get(s)?
            .get(track)?
            .get(&row?)
            .map(String::as_str)
    }

    /// How many scenes a block gets: one per section of the busier source,
    /// up to `MAX_BLOCK`.
    pub fn block_len(&self) -> usize {
        self.sections[0]
            .len()
            .max(self.sections[1].len())
            .clamp(1, MAX_BLOCK)
    }

    /// Source `s`'s `j`-th section, wrapping round. `None` when it has none.
    pub fn section(&self, s: usize, j: usize) -> Option<usize> {
        let rows = &self.sections[s];
        (!rows.is_empty()).then(|| rows[j % rows.len()])
    }

    /// Lays variation `var` out as `rows`, and fills every track it picks
    /// with its clip in each of them.
    pub fn lay_out(&mut self, var: usize, rows: Vec<[Option<usize>; 2]>) {
        let slots = self.picks[var]
            .iter()
            .enumerate()
            .flat_map(|(s, picks)| picks.iter().map(move |&t| (s, t)))
            .map(|(s, t)| {
                let values = rows
                    .iter()
                    .map(|r| self.clip(s, t, r[s.min(1)]).map(str::to_string))
                    .collect();
                ((s, t), values)
            })
            .collect();
        self.blocks[var] = Block { rows, slots };
    }

    /// Runs `mix(own, partner)` on every slot of every partnered B track
    /// (see `Variation::partners`) whose A partner plays in that scene.
    /// `own` is the slot as earlier stages left it.
    pub fn rework_partnered(
        &mut self,
        mix: impl Fn(Option<&str>, &str) -> Result<Option<String>>,
    ) -> Result<()> {
        for (var, v) in self.variations.iter().enumerate() {
            for &(b, a) in &v.partners {
                let block = &self.blocks[var];
                let partners: Vec<Option<String>> = block
                    .rows
                    .iter()
                    .map(|r| self.clip(0, a, r[0]).map(str::to_string))
                    .collect();
                let Some(slots) = self.blocks[var].slots.get_mut(&(1, b)) else {
                    continue;
                };
                for (slot, partner) in slots.iter_mut().zip(&partners) {
                    if let Some(partner) = partner {
                        *slot = mix(slot.as_deref(), partner)?;
                    }
                }
            }
        }
        Ok(())
    }
}
