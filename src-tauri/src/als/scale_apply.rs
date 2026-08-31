use anyhow::{anyhow, Result};
use roxmltree::{Document, Node};
use serde::Serialize;
use std::ops::Range;

use std::collections::HashSet;

use super::scale_candidates::clip_pitch_classes;
use super::scale_constants::SCALE_INTERVALS;
use super::scale_info_schema::{
    ableton_major_version, detect_schema, has_ambiguous_root_tags, read_root_value,
    read_scale_index, scale_information_block, sniff_document_schema, ScaleInfoSchema, ROOT_TAGS,
};

/// Whether every pitch class in `pitch_classes` fits within `(root_note, scale_index)`'s
/// interval set. Empty pitch classes never "fit" -- an unpitched clip has
/// nothing to judge compatibility from.
fn scale_fits(pitch_classes: &HashSet<i32>, root_note: i32, scale_index: i32) -> bool {
    let Some(intervals) = SCALE_INTERVALS.get(scale_index as usize) else {
        return false;
    };
    !pitch_classes.is_empty()
        && pitch_classes.iter().all(|&pc| {
            let offset = (pc - root_note).rem_euclid(12) as u8;
            intervals.contains(&offset)
        })
}

/// Summary of what happened when a scale was applied to a project's clips.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ApplyScaleOutcome {
    /// Clips whose `ScaleInformation` was still at Ableton's default (C
    /// Major) and got rewritten to the target scale.
    pub clips_changed: u32,
    /// Clips with no `ScaleInformation` node at all that were pitch-compatible
    /// with the target scale, so a new block was inserted for them.
    pub clips_created: u32,
    /// Clips whose `ScaleInformation` was already set to something other
    /// than the default, and that setting actually fits the clip's own
    /// notes -- a deliberate, valid choice, left untouched.
    pub clips_already_set: u32,
    /// Clips whose `ScaleInformation` was set to a scale that doesn't
    /// actually fit their own notes (a stale/wrong label) and got
    /// corrected to the target scale, which does fit.
    pub clips_corrected: u32,
    /// Clips whose notes don't fit the target scale -- left untouched.
    pub clips_incompatible: u32,
    /// True when the project's own declared file format predates Ableton's
    /// per-clip Scale feature (Live 11.1/12-era `MajorVersion="5"` schema).
    /// Any `clips_created` blocks written into such a file are structurally
    /// valid XML, but current Ableton appears to parse `MidiClip` according
    /// to the file's declared schema and silently ignores `ScaleInformation`
    /// on an older-schema document -- so the change may not actually show
    /// up when reopened in Ableton until the project is resaved there once
    /// to upgrade its schema.
    pub schema_predates_clip_scale: bool,
}

/// A byte range in the original XML to replace with `block`. The unit both
/// `decide_clip_action` and `apply_edits` trade in, so a rewritten element
/// and a freshly inserted one (an empty range at the insertion point) look
/// the same to every caller.
struct Edit {
    range: Range<usize>,
    block: String,
}

/// What should happen to a single `MidiClip` relative to the target scale.
/// Purely a decision -- carries the edits to make (if any) but doesn't
/// touch `edits` or `ApplyScaleOutcome` itself, so the scoring logic in
/// `decide_clip_action` can be read (and tested) independently of the
/// bookkeeping in `apply_scale_to_xml`'s loop. A touched clip carries one
/// or two edits: always the `ScaleInformation` rewrite/insert, plus an
/// `IsInKey` edit only when that flag wasn't already `true`.
enum ClipAction {
    /// Notes don't fit the target scale -- left untouched.
    Incompatible,
    /// Existing `ScaleInformation` is a deliberate, valid choice -- left
    /// untouched.
    AlreadySet,
    /// No `ScaleInformation` node existed; insert this new block.
    Insert(Vec<Edit>),
    /// Existing `ScaleInformation` was at Ableton's default; rewrite it.
    Changed(Vec<Edit>),
    /// Existing `ScaleInformation` was a stale/wrong label; rewrite it.
    Corrected(Vec<Edit>),
}

/// Decides what should happen to one `MidiClip` given
/// `(target_root_note, target_scale_index)`. Just a dispatcher: the two
/// cases (no `ScaleInformation` yet vs. an existing one to evaluate) are
/// different enough decisions that each gets its own function below.
fn decide_clip_action(
    xml: &str,
    clip: &Node,
    target_root_note: i32,
    target_scale_index: i32,
    default_schema: ScaleInfoSchema,
) -> Result<ClipAction> {
    let pitch_classes = clip_pitch_classes(clip);
    let compatible_with_target = scale_fits(&pitch_classes, target_root_note, target_scale_index);
    let scale_info = clip
        .children()
        .find(|c| c.tag_name().name() == "ScaleInformation");

    match scale_info {
        None => insert_action_for_missing_scale_info(
            xml,
            clip,
            compatible_with_target,
            default_schema,
            target_root_note,
            target_scale_index,
        ),
        Some(node) => Ok(rewrite_action_for_existing_scale_info(
            xml,
            clip,
            &node,
            &pitch_classes,
            compatible_with_target,
            default_schema,
            target_root_note,
            target_scale_index,
        )),
    }
}

/// A clip with no `ScaleInformation` node at all: insert a new block if its
/// notes fit the target scale, otherwise leave it untouched.
fn insert_action_for_missing_scale_info(
    xml: &str,
    clip: &Node,
    compatible_with_target: bool,
    default_schema: ScaleInfoSchema,
    target_root_note: i32,
    target_scale_index: i32,
) -> Result<ClipAction> {
    if !compatible_with_target {
        return Ok(ClipAction::Incompatible);
    }
    let notes_node = clip
        .children()
        .find(|c| c.tag_name().name() == "Notes")
        .ok_or_else(|| anyhow!("MidiClip missing Notes"))?;
    let insert_at = notes_node.range().start;
    let indent = leading_indent(xml, insert_at);

    let mut edits: Vec<Edit> = ensure_clip_in_key_edit(clip, insert_at, &indent)
        .into_iter()
        .collect();
    let block = format!(
        "{}\n{indent}",
        scale_information_block(&indent, default_schema, target_root_note, target_scale_index, ""),
    );
    edits.push(Edit {
        range: insert_at..insert_at,
        block,
    });
    Ok(ClipAction::Insert(edits))
}

/// A clip that already has a `ScaleInformation` node: leave a valid,
/// deliberate label alone; otherwise rewrite it to the target scale if the
/// clip's notes fit it, or leave an incompatible clip untouched.
fn rewrite_action_for_existing_scale_info(
    xml: &str,
    clip: &Node,
    node: &Node,
    pitch_classes: &HashSet<i32>,
    compatible_with_target: bool,
    default_schema: ScaleInfoSchema,
    target_root_note: i32,
    target_scale_index: i32,
) -> ClipAction {
    let root_val = read_root_value(node);
    let name_val = read_scale_index(node);
    let is_default = root_val == Some(0) && name_val == Some(0);
    let is_corrupted = has_ambiguous_root_tags(node);

    // A non-default label is only "already set" (left alone) if it
    // actually fits the clip's own notes -- otherwise it's a stale
    // label the target scale can correct. A node with more than one
    // root tag present is never treated as valid, even if it happens
    // to pitch-fit by coincidence: it can only mean an earlier version
    // of this tool corrupted it, and it must be cleaned up.
    let existing_label_valid = !is_default
        && !is_corrupted
        && matches!((root_val, name_val), (Some(r), Some(n)) if scale_fits(pitch_classes, r, n));

    if existing_label_valid {
        return ClipAction::AlreadySet;
    }

    if !compatible_with_target {
        return ClipAction::Incompatible;
    }

    // Rewrite the whole element rather than editing individual Root/Name
    // attribute values: real projects have been seen with a
    // ScaleInformation node that's missing Root and/or Name entirely, and
    // different Ableton versions use different tag names/value formats for
    // Root and Name (see ScaleInfoSchema) -- per-attribute lookups aren't
    // safe to assume. Any other unrelated children are preserved verbatim,
    // copied from their own original text. The node's own detected schema
    // is reused so the rewrite matches whatever this specific file's
    // Ableton version already expects, falling back to the document-wide
    // sniffed schema only when this node's own shape can't be determined
    // (missing both Root and RootNote).
    let schema = detect_schema(node).unwrap_or(default_schema);
    let scale_info_edit =
        rebuild_scale_information_edit(xml, node, schema, target_root_note, target_scale_index);

    // Scale awareness itself isn't part of ScaleInformation at all: Ableton
    // tracks it as a separate `IsInKey` element, a sibling sitting right
    // before ScaleInformation, not a child of it. Applying a scale should
    // also turn that on, so it's ensured here alongside the rewrite.
    let indent = leading_indent(xml, node.range().start);
    let mut edits: Vec<Edit> = ensure_clip_in_key_edit(clip, node.range().start, &indent)
        .into_iter()
        .collect();
    edits.push(scale_info_edit);

    if is_default {
        ClipAction::Changed(edits)
    } else {
        ClipAction::Corrected(edits)
    }
}

/// Rewrites (or inserts) `<ScaleInformation>` on every `MidiClip` that is
/// pitch-compatible with `(target_root_note, target_scale_index)` and isn't
/// already explicitly set to a different scale, leaving every other byte of
/// the original XML untouched. Returns the new XML string plus a summary of
/// what was/wasn't changed.
pub(super) fn apply_scale_to_xml(
    xml: &str,
    target_root_note: i32,
    target_scale_index: i32,
) -> Result<(String, ApplyScaleOutcome)> {
    let doc = Document::parse(xml).map_err(|e| anyhow!("XML parse error: {}", e))?;
    if target_scale_index as usize >= SCALE_INTERVALS.len() {
        return Err(anyhow!("Invalid scale index {}", target_scale_index));
    }

    let mut edits: Vec<Edit> = Vec::new();
    let mut outcome = ApplyScaleOutcome {
        schema_predates_clip_scale: ableton_major_version(&doc).is_some_and(|v| v < 5),
        ..Default::default()
    };
    let default_schema = sniff_document_schema(&doc);

    for clip in doc.descendants().filter(|n| n.tag_name().name() == "MidiClip") {
        let action =
            decide_clip_action(xml, &clip, target_root_note, target_scale_index, default_schema)?;
        match action {
            ClipAction::Incompatible => outcome.clips_incompatible += 1,
            ClipAction::AlreadySet => outcome.clips_already_set += 1,
            ClipAction::Insert(clip_edits) => {
                edits.extend(clip_edits);
                outcome.clips_created += 1;
            }
            ClipAction::Changed(clip_edits) => {
                edits.extend(clip_edits);
                outcome.clips_changed += 1;
            }
            ClipAction::Corrected(clip_edits) => {
                edits.extend(clip_edits);
                outcome.clips_corrected += 1;
            }
        }
    }

    edits.sort_by_key(|edit| edit.range.start);
    let new_xml = apply_edits(xml, &edits);

    // The edits are byte-range splices, not a re-serialization -- verify the
    // result is still well-formed before it's ever written to disk, since a
    // wrong range here (e.g. from an Ableton version/schema we mis-detected)
    // would otherwise silently corrupt the user's project file.
    Document::parse(&new_xml)
        .map_err(|e| anyhow!("Internal error: edited XML failed to re-parse ({e}), refusing to write"))?;

    Ok((new_xml, outcome))
}

/// Replaces an entire `<ScaleInformation>...</ScaleInformation>` (or
/// self-closing `<ScaleInformation />`) element's byte range with a rebuilt
/// version carrying the target root/scale in `schema`'s shape, preserving
/// any other unrelated element children verbatim, copied from their own
/// original source text -- safe regardless of whether the original had
/// both, one, or neither of its root/name tags present. Both possible
/// root-tag spellings (`Root` and `RootNote`) are excluded from "other
/// children" so the original is always replaced, never duplicated alongside
/// the new one.
fn rebuild_scale_information_edit(
    xml: &str,
    node: &Node,
    schema: ScaleInfoSchema,
    target_root_note: i32,
    target_scale_index: i32,
) -> Edit {
    let indent = leading_indent(xml, node.range().start);
    let other_children: String = node
        .children()
        .filter(|c| {
            c.is_element() && !ROOT_TAGS.contains(&c.tag_name().name()) && c.tag_name().name() != "Name"
        })
        .map(|c| format!("\n{indent}\t{}", &xml[c.range()]))
        .collect();
    let block = scale_information_block(&indent, schema, target_root_note, target_scale_index, &other_children);
    Edit {
        range: node.range(),
        block,
    }
}

/// Ensures a clip's `IsInKey` flag -- Ableton's real scale-awareness
/// toggle, a sibling element rather than a child of `ScaleInformation` --
/// is `true`. Rewrites an existing `false` flag in place, inserts a fresh
/// one immediately before `before` if the clip has none at all, or returns
/// `None` if it's already `true` (nothing to do). `indent` should match
/// whatever `before` is indented to, so an inserted flag lines up with its
/// neighbors.
fn ensure_clip_in_key_edit(clip: &Node, before: usize, indent: &str) -> Option<Edit> {
    match clip.children().find(|c| c.tag_name().name() == "IsInKey") {
        Some(node) if node.attribute("Value") == Some("true") => None,
        Some(node) => Some(Edit {
            range: node.range(),
            block: r#"<IsInKey Value="true" />"#.to_string(),
        }),
        None => Some(Edit {
            range: before..before,
            block: format!("<IsInKey Value=\"true\" />\n{indent}"),
        }),
    }
}

/// Captures the run of horizontal whitespace immediately before `pos` on its
/// line, so an inserted block can match the surrounding file's indentation.
fn leading_indent(xml: &str, pos: usize) -> String {
    let before = &xml[..pos];
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    xml[line_start..pos]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

fn apply_edits(xml: &str, edits: &[Edit]) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut cursor = 0;
    for edit in edits {
        out.push_str(&xml[cursor..edit.range.start]);
        out.push_str(&edit.block);
        cursor = edit.range.end;
    }
    out.push_str(&xml[cursor..]);
    out
}
