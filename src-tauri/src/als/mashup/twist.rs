use super::{apply_edits, child_value};
use anyhow::{anyhow, Result};
use roxmltree::{Document, Node};

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
pub(super) fn clear_envelopes(value: &str) -> Result<String> {
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
pub(super) fn change_speed(value: &str, speed: Speed) -> Result<String> {
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
