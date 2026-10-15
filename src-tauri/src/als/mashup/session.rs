use super::{apply_edits, child_value};
use anyhow::{anyhow, Result};
use roxmltree::{Document, Node};

/// Live palette indices, one per variation, so each variation's scenes
/// and clips read as one coloured band in the Session grid.
const VAR_COLORS: [u32; 6] = [13, 1, 4, 8, 11, 17];

/// One scene row in the merged set.
#[derive(Debug, PartialEq)]
pub(super) enum Row {
    /// The variation's first row: named for what it does, with no clips.
    Header(usize),
    /// Variation `var` playing source `s`'s section `rows[s]` (a scene
    /// row of that source), or nothing from it when `None`.
    Play {
        var: usize,
        rows: [Option<usize>; 2],
    },
    /// An empty row between variations.
    Gap,
}

/// A track's Session View slots: `DeviceChain/MainSequencer/ClipSlotList`.
pub(super) fn session_slots<'a, 'i>(track: Node<'a, 'i>) -> Vec<Node<'a, 'i>> {
    slot_list(track, "MainSequencer")
        .map(|list| {
            list.children()
                .filter(|c| c.tag_name().name() == "ClipSlot")
                .collect()
        })
        .unwrap_or_default()
}

fn slot_list<'a, 'i>(track: Node<'a, 'i>, sequencer: &str) -> Option<Node<'a, 'i>> {
    ["DeviceChain", sequencer, "ClipSlotList"]
        .iter()
        .try_fold(track, |n, tag| {
            n.children().find(|c| c.tag_name().name() == *tag)
        })
}

/// A slot's `ClipSlot/Value`, when it holds a clip.
fn clip_value<'a, 'i>(slot: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
    slot.children()
        .find(|c| c.tag_name().name() == "ClipSlot")?
        .children()
        .find(|c| c.tag_name().name() == "Value")
        .filter(|v| v.children().any(|c| c.is_element()))
}

/// The scene rows where any of `tracks` has a Session clip, in order: the
/// project's sections.
pub(super) fn filled_rows(tracks: &[Node]) -> Vec<usize> {
    let mut rows: Vec<usize> = tracks
        .iter()
        .flat_map(|t| {
            session_slots(*t)
                .into_iter()
                .enumerate()
                .filter(|(_, s)| clip_value(*s).is_some())
                .map(|(i, _)| i)
        })
        .collect();
    rows.sort_unstable();
    rows.dedup();
    rows
}

/// The `<Value>` holding the clip `track` plays in scene row `row`: its own
/// clip there, or else its last clip above it, or else its first clip
/// below, so a sparse sketch still sounds whole in every section.
pub(super) fn clip_near<'a, 'i>(track: Node<'a, 'i>, row: usize) -> Option<Node<'a, 'i>> {
    let clips: Vec<(usize, Node)> = session_slots(track)
        .into_iter()
        .enumerate()
        .filter_map(|(i, s)| clip_value(s).map(|v| (i, v)))
        .collect();
    clips
        .iter()
        .rev()
        .find(|(i, _)| *i <= row)
        .or(clips.first())
        .map(|&(_, v)| v)
}

pub(super) fn var_color(var: usize) -> u32 {
    VAR_COLORS[var % VAR_COLORS.len()]
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}

/// Rebuilds one track's slot lists to hold `values` (one `<Value>` per
/// scene row): the Session clips, plus the freeze list (when it has slots)
/// and a group's `Slots`, emptied to the same length. A track without
/// these lists comes back unchanged.
pub(super) fn rebuild_slots(block: &str, values: &[String]) -> Result<String> {
    let doc = Document::parse(block)
        .map_err(|e| anyhow!("XML parse error while laying out scenes: {e}"))?;
    let track = doc.root_element();
    let slot = |i: usize, value: &str| {
        format!(
            r#"<ClipSlot Id="{i}"><LomId Value="0" /><ClipSlot>{value}</ClipSlot><HasStop Value="true" /><NeedRefreeze Value="true" /></ClipSlot>"#
        )
    };
    let mut edits = Vec::new();
    if let Some(list) = slot_list(track, "MainSequencer") {
        let slots: String = values.iter().enumerate().map(|(i, v)| slot(i, v)).collect();
        edits.push((
            list.range(),
            format!("<ClipSlotList>{slots}</ClipSlotList>"),
        ));
    }
    if let Some(list) = slot_list(track, "FreezeSequencer").filter(|l| l.has_children()) {
        let slots: String = (0..values.len()).map(|i| slot(i, "<Value />")).collect();
        edits.push((
            list.range(),
            format!("<ClipSlotList>{slots}</ClipSlotList>"),
        ));
    }
    if let Some(list) = track.children().find(|c| c.tag_name().name() == "Slots") {
        let slots: String = (0..values.len())
            .map(|i| format!(r#"<GroupTrackSlot Id="{i}"><LomId Value="0" /></GroupTrackSlot>"#))
            .collect();
        edits.push((list.range(), format!("<Slots>{slots}</Slots>")));
    }
    Ok(apply_edits(block, edits))
}

/// The `<Scenes>` list for `rows`, each a copy of `template` (one of the
/// base set's own scenes) renumbered, named and coloured. `name(row)` gives
/// a row's name.
pub(super) fn scenes_xml(
    template: &str,
    rows: &[Row],
    name: impl Fn(&Row) -> String,
) -> Result<String> {
    let doc = Document::parse(template)
        .map_err(|e| anyhow!("XML parse error in the base set's scene: {e}"))?;
    let scene = doc.root_element();
    let id = scene
        .attributes()
        .find(|a| a.name() == "Id")
        .ok_or_else(|| anyhow!("scene has no Id"))?;
    let name_attr = child_value(scene, "Name").ok_or_else(|| anyhow!("scene has no Name"))?;
    let color = child_value(scene, "Color").ok_or_else(|| anyhow!("scene has no Color"))?;
    let mut out = String::from("<Scenes>");
    for (i, row) in rows.iter().enumerate() {
        let tint = match row {
            Row::Header(var) | Row::Play { var, .. } => var_color(*var) as i64,
            Row::Gap => -1,
        };
        out.push_str(&apply_edits(
            template,
            vec![
                (id.range_value(), i.to_string()),
                (name_attr.range_value(), escape(&name(row))),
                (color.range_value(), tint.to_string()),
            ],
        ));
    }
    out.push_str("</Scenes>");
    Ok(out)
}
