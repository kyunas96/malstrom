use roxmltree::{Document, Node};

use super::scale_constants::SCALE_NAMES;
use super::scale_names::child_value;

/// Ableton's own file-format major version, read from the root `<Ableton
/// MajorVersion="...">` element. Per-clip Scale support first appears at
/// `MajorVersion="5"` (Live 11.1/12-era); a lower value means clips in this
/// file never had `ScaleInformation` under their original Ableton version.
pub(super) fn ableton_major_version(doc: &Document) -> Option<i32> {
    doc.root_element()
        .attribute("MajorVersion")
        .and_then(|v| v.parse::<i32>().ok())
}

/// Ableton has used two different `ScaleInformation` shapes across
/// versions: Live 11.x writes `<RootNote Value="N" /><Name Value="Major" />`
/// (the root tag is `RootNote`, and `Name`'s value is the scale's literal
/// name string); Live 12.x writes `<Root Value="N" /><Name Value="N" />`
/// (both numeric indices). Any code touching an existing node must detect
/// and preserve whichever shape it already uses -- writing the wrong one
/// alongside the original (rather than replacing it) corrupts the file with
/// duplicate, conflicting root information.
pub(super) const ROOT_TAGS: [&str; 2] = ["Root", "RootNote"];

/// Which `ScaleInformation` shape a node uses: the tag name carrying the
/// root note (`"Root"` or `"RootNote"`), and whether `Name`'s value is a
/// numeric scale index (Live 12.x) rather than a literal scale name string
/// (Live 11.x).
#[derive(Clone, Copy)]
pub(super) struct ScaleInfoSchema {
    root_tag: &'static str,
    numeric_name: bool,
}

const DEFAULT_SCHEMA: ScaleInfoSchema = ScaleInfoSchema {
    root_tag: "Root",
    numeric_name: true,
};

/// True when a node has more than one of the possible root tags present at
/// once (e.g. both `Root` and `RootNote`) -- an impossible, corrupted state
/// that can only come from an earlier version of this tool's own bug
/// (writing a new-schema root tag without removing an existing one of a
/// different shape). Such a node must always be cleaned up on next touch,
/// never treated as an "already validly set" label.
pub(super) fn has_ambiguous_root_tags(node: &Node) -> bool {
    ROOT_TAGS
        .iter()
        .filter(|&&tag| node.children().any(|c| c.tag_name().name() == tag))
        .count()
        > 1
}

/// Detects which schema an existing `ScaleInformation` node uses, from
/// whichever root tag is actually present and whether `Name`'s value parses
/// as a number. Returns `None` if neither root tag is present (e.g. a node
/// with only an unrelated child like `IsScaleEnabled`), or if the node has
/// more than one root tag present (an ambiguous, corrupted state -- see
/// `has_ambiguous_root_tags`), so callers fall back to a clean schema
/// instead of perpetuating or trusting the corruption.
pub(super) fn detect_schema(node: &Node) -> Option<ScaleInfoSchema> {
    if has_ambiguous_root_tags(node) {
        return None;
    }
    let root_tag = *ROOT_TAGS
        .iter()
        .find(|&&tag| node.children().any(|c| c.tag_name().name() == tag))?;
    let numeric_name = child_value(node, "Name")
        .map(|v| v.parse::<i32>().is_ok())
        .unwrap_or(true);
    Some(ScaleInfoSchema {
        root_tag,
        numeric_name,
    })
}

/// Reads the root note value from a node, checking both possible tag names.
pub(super) fn read_root_value(node: &Node) -> Option<i32> {
    ROOT_TAGS
        .iter()
        .find_map(|&tag| child_value(node, tag))
        .and_then(|v| v.parse::<i32>().ok())
}

/// Reads the scale index from a node's `Name` child, whether it's stored as
/// a numeric index (Live 12.x) or a literal scale name string (Live 11.x).
pub(super) fn read_scale_index(node: &Node) -> Option<i32> {
    let raw = child_value(node, "Name")?;
    raw.parse::<i32>()
        .ok()
        .or_else(|| SCALE_NAMES.iter().position(|&n| n == raw).map(|i| i as i32))
}

/// Sniffs which `ScaleInformation` schema this document's Ableton version
/// uses, from any existing node found anywhere in the file (clips are
/// nearly always saved by the same Ableton version, so the first detectable
/// node is a reliable proxy for the rest). Falls back to `DEFAULT_SCHEMA`
/// when no existing node anywhere in the file has a recognizable schema
/// (e.g. no clip has `ScaleInformation` at all yet).
pub(super) fn sniff_document_schema(doc: &Document) -> ScaleInfoSchema {
    doc.descendants()
        .filter(|n| n.tag_name().name() == "ScaleInformation")
        .find_map(|n| detect_schema(&n))
        .unwrap_or(DEFAULT_SCHEMA)
}

pub(super) fn scale_information_block(
    indent: &str,
    schema: ScaleInfoSchema,
    root_note: i32,
    scale_index: i32,
    other_children: &str,
) -> String {
    let name_value = if schema.numeric_name {
        scale_index.to_string()
    } else {
        SCALE_NAMES[scale_index as usize].to_string()
    };
    format!(
        "<ScaleInformation>\n{indent}\t<{root_tag} Value=\"{root_note}\" />\n{indent}\t<Name Value=\"{name_value}\" />{other_children}\n{indent}</ScaleInformation>",
        root_tag = schema.root_tag,
    )
}
