//! The heat twists: what a variation does to B's own clips (scene shift,
//! extra pitch, speed, and one swapped track), drawn by `plan.rs`.

use super::pipeline::{Mix, Stage};
use super::{apply_edits, child_value, transpose_track};
use anyhow::{anyhow, Result};
use roxmltree::{Document, Node};

/// Runs each variation's `Twist` on B's slots. The scene shift isn't here:
/// the layout stage reads it.
pub struct TwistStage;

impl Stage for TwistStage {
    fn run(&self, mix: &mut Mix) -> Result<()> {
        for (var, v) in mix.variations.iter().enumerate() {
            let twist = &v.twist;
            let block = &mix.blocks[var];
            let mut reworked = Vec::new();
            for &idx in mix.picks[var].get(1).map_or(&[][..], |p| p) {
                let donor = twist.swap.filter(|&(b, _)| b == idx).map(|(_, a)| a);
                let slots = block.slots[&(1, idx)]
                    .iter()
                    .zip(&block.rows)
                    .map(|(own, rows)| {
                        let donor_clip = donor.and_then(|a| mix.clip(0, a, rows[0]));
                        twist.apply(idx, own.as_deref(), donor_clip)
                    })
                    .collect::<Result<Vec<_>>>()?;
                reworked.push((idx, slots));
            }
            for (idx, slots) in reworked {
                mix.blocks[var].slots.insert((1, idx), slots);
            }
        }
        Ok(())
    }
}

/// The strategies the heat dial adds, mildest first. Heat `h` turns on the
/// first `h` of them; "stack" (random track sets, B in A's key) is always on.
pub const HEAT_LEVELS: [&str; 5] = ["stack", "shuffle", "pitch", "stretch", "swap"];

/// How fast B's Session clips play.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum Speed {
    #[default]
    Same,
    Half,
    Double,
}

/// What a variation does to B beyond picking its tracks. The default
/// changes nothing.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Twist {
    /// Each of the variation's scenes plays B's section j + scene_shift
    /// against A's section j, so B's parts meet different parts of A.
    pub scene_shift: usize,
    /// Extra semitones for each of B's `non_return_tracks`, on top of its
    /// key move. Drums get 0.
    pub semitones: Vec<i32>,
    pub speed: Speed,
    /// `(b, a)`: B's track `b` keeps its instrument but plays A's track
    /// `a`'s clips, as A wrote them.
    pub swap: Option<(usize, usize)>,
}

impl Twist {
    /// A short label for the variation's group name, e.g. "scenes+2 · +7 ·
    /// half · swap". Empty when nothing changes.
    pub fn label(&self) -> String {
        let mut parts = Vec::new();
        if self.scene_shift > 0 {
            parts.push(format!("scenes+{}", self.scene_shift));
        }
        if let Some(&s) = self.semitones.iter().find(|&&s| s != 0) {
            parts.push(format!("{s:+}"));
        }
        match self.speed {
            Speed::Same => {}
            Speed::Half => parts.push("half".to_string()),
            Speed::Double => parts.push("double".to_string()),
        }
        if self.swap.is_some() {
            parts.push("swap".to_string());
        }
        parts.join(" · ")
    }

    /// Runs the twist's clip stages on one Session slot of B's track `idx`,
    /// in order: source (swap), time (speed), pitch (extra semitones).
    /// `b_clip` is B's own clip there and `donor_clip` A's clip on the swap
    /// donor's track at the same row. Selection (`scene_shift`) already
    /// happened in `layout`, which picked those rows. `None` leaves the
    /// slot empty.
    pub(super) fn apply(
        &self,
        idx: usize,
        b_clip: Option<&str>,
        donor_clip: Option<&str>,
    ) -> Result<Option<String>> {
        // A swapped track plays A's clip as A wrote it, so no later stage
        // touches it.
        if self.swap.is_some_and(|(b, _)| b == idx) {
            return donor_clip.map(clear_envelopes).transpose();
        }
        let Some(clip) = b_clip else {
            return Ok(None);
        };
        let clip = change_speed(clip, self.speed)?;
        let extra = self.semitones.get(idx).copied().unwrap_or(0);
        transpose_track(&clip, extra).map(Some)
    }
}

fn parse(block: &str) -> Result<Document<'_>> {
    Document::parse(block).map_err(|e| anyhow!("XML parse error while twisting a clip: {e}"))
}

/// The clips in `value` (a slot's `<Value>`).
fn clips<'a, 'i>(doc: &'a Document<'i>) -> impl Iterator<Item = Node<'a, 'i>> {
    doc.root_element()
        .children()
        .filter(|c| matches!(c.tag_name().name(), "MidiClip" | "AudioClip"))
}

/// `value` with its clip envelopes cleared, for a clip moving to another
/// track: they point at its old track's devices.
fn clear_envelopes(value: &str) -> Result<String> {
    let doc = parse(value)?;
    let edits = doc
        .descendants()
        .filter(|n| n.tag_name().name() == "Envelopes")
        .filter(|n| {
            n.parent()
                .is_some_and(|p| p.tag_name().name() == "Envelopes")
        })
        .map(|n| (n.range(), "<Envelopes />".to_string()))
        .collect();
    Ok(apply_edits(value, edits))
}

/// `value` with its clips set to Live palette colour `color`.
pub(super) fn recolor(value: &str, color: u32) -> Result<String> {
    let doc = parse(value)?;
    let edits = clips(&doc)
        .filter_map(|c| child_value(c, "Color"))
        .map(|a| (a.range_value(), color.to_string()))
        .collect();
    Ok(apply_edits(value, edits))
}

/// Plays the clips in `value` (a slot's `<Value>`) at `speed` by
/// stretching every beat position in them: loop and start/end markers, MIDI
/// note times and lengths, and warp markers. Unwarped audio is left alone,
/// since it can't change speed without changing pitch. Clip envelopes and
/// follow-action times aren't stretched.
fn change_speed(value: &str, speed: Speed) -> Result<String> {
    let factor = match speed {
        Speed::Same => return Ok(value.to_string()),
        Speed::Half => 2.0,
        Speed::Double => 0.5,
    };
    let doc = parse(value)?;
    let scale = |v: &str| v.parse::<f64>().ok().map(|x| format!("{}", x * factor));
    let mut edits = Vec::new();
    let clips = clips(&doc).filter(|c| {
        c.tag_name().name() == "MidiClip"
            || child_value(*c, "IsWarped").is_some_and(|w| w.value() == "true")
    });
    for clip in clips {
        let loop_node = clip.children().find(|c| c.tag_name().name() == "Loop");
        let markers = ["CurrentStart", "CurrentEnd"]
            .iter()
            .filter_map(|t| child_value(clip, t))
            .chain(loop_node.into_iter().flat_map(|l| {
                l.children()
                    .filter(|c| c.tag_name().name() != "LoopOn")
                    .filter_map(|c| c.attributes().find(|a| a.name() == "Value"))
            }));
        let events = clip.descendants().flat_map(|n| {
            let names: &[&str] = match n.tag_name().name() {
                "MidiNoteEvent" => &["Time", "Duration"],
                "WarpMarker" => &["BeatTime"],
                _ => &[],
            };
            n.attributes().filter(move |a| names.contains(&a.name()))
        });
        for attr in markers.chain(events) {
            if let Some(v) = scale(attr.value()) {
                edits.push((attr.range_value(), v));
            }
        }
    }
    Ok(apply_edits(value, edits))
}
