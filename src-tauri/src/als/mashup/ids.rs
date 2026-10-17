use super::apply_edits;
use anyhow::{anyhow, Result};
use roxmltree::Document;

/// Whether a `*Id`-named element's `Value` points at a pointee id. Not
/// `NextPointeeId` (a counter, see `max_referenced_id`), `GrooveId` (points
/// into the groove pool, see `strip_groove_references`) or `LomId` (Live's
/// runtime object handle: repeats freely, and can be as large as
/// 2147418112, which would size an id range past what Live can read).
fn holds_id_reference(tag: &str) -> bool {
    tag.ends_with("Id") && !matches!(tag, "NextPointeeId" | "GrooveId" | "LomId")
}

/// The highest `Id` any element in the document claims, or that any
/// `*Id`-named element (e.g. `PointeeId`) points at via its `Value`.
/// `NextPointeeId` itself is excluded even though its tag matches: it's a
/// forward-looking counter, not a reference to anything, so counting its
/// own `Value` here would make it impossible to ever raise it far enough
/// (it would always need to exceed itself).
pub(super) fn max_referenced_id(doc: &Document) -> u64 {
    doc.descendants()
        .flat_map(|n| {
            let tag = n.tag_name().name();
            let is_reference_holder = holds_id_reference(tag);
            n.attributes()
                .filter(move |a| a.name() == "Id" || (is_reference_holder && a.name() == "Value"))
                .filter_map(|a| a.value().parse::<u64>().ok())
                .collect::<Vec<_>>()
        })
        .max()
        .unwrap_or(0)
}

/// A clip's `<GrooveSettings><GrooveId Value="N" /></GrooveSettings>`
/// points at an entry in its *document's* top-level
/// `<GroovePool><Grooves><Groove Id="N">` -- a resource shared across the
/// whole project, not owned by the track (confirmed against a real
/// project's decompressed XML; `-1` is Ableton's own "no groove" sentinel,
/// also seen in the wild). Splicing a donor track in without its source
/// document's GroovePool leaves that reference dangling (Live: "GroovePool
/// corrupted"), and remapping it like a normal Id would only point it at
/// some unrelated groove in the base project's pool. Neither is a
/// documented feature v0 promises, so donor tracks simply lose their groove
/// assignment -- same "surfaced, not silently broken" tradeoff as v0's
/// existing external-sample-reference limitation. Base-project tracks are
/// never touched, so their grooves (valid against the base's own retained
/// pool) survive as-is.
pub(super) fn strip_groove_references(block: &str) -> Result<String> {
    let doc = Document::parse(block)
        .map_err(|e| anyhow!("XML parse error while stripping grooves: {e}"))?;
    let edits: Vec<(std::ops::Range<usize>, String)> = doc
        .descendants()
        .filter(|n| n.tag_name().name() == "GrooveId")
        .filter_map(|n| n.attributes().find(|a| a.name() == "Value"))
        .filter(|a| a.value() != "-1")
        .map(|a| (a.range_value(), "-1".to_string()))
        .collect();
    Ok(apply_edits(block, edits))
}

/// Adds `offset` to every id-bearing value in `block` (an extracted track's
/// own XML) -- both `Id="N"` attributes and `Value="N"` on `*Id`-named
/// elements like `PointeeId`, the same pair `max_referenced_id` scans for,
/// so a definition and everything pointing at it move together. Ids are
/// only unique *within* a single project; splicing a whole track's subtree
/// from another project in verbatim reliably collides with the base
/// project's own ids somewhere in that subtree (not just the track's own
/// top-level Id) -- Live calls this "non-unique Pointee IDs" and refuses to
/// load the file. Shifting the entire donor subtree above every id already
/// used by the base document sidesteps the collision without needing to
/// know which specific elements reference which.
pub(super) fn shift_ids_in_block(block: &str, offset: u64) -> Result<String> {
    if offset == 0 {
        return Ok(block.to_string());
    }
    let doc =
        Document::parse(block).map_err(|e| anyhow!("XML parse error while remapping ids: {e}"))?;
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for n in doc.descendants() {
        let tag_ends_in_id = holds_id_reference(n.tag_name().name());
        for attr in n.attributes() {
            let relevant = attr.name() == "Id" || (tag_ends_in_id && attr.name() == "Value");
            let Some(original) = relevant.then(|| attr.value().parse::<u64>().ok()).flatten()
            else {
                continue;
            };
            edits.push((attr.range_value(), (original + offset).to_string()));
        }
    }
    Ok(apply_edits(block, edits))
}

/// Raises `<NextPointeeId Value="...">` to stay above every id now present
/// in `merged`, if the value inherited from `sources[0]` is no longer high
/// enough -- otherwise Live refuses to load the file ("NextPointeeId is too
/// low"). A no-op if the project has no such element (older format) or its
/// value already clears the new high-water mark.
pub(super) fn ensure_next_pointee_id_is_high_enough(merged: &mut String) -> Result<()> {
    let doc = Document::parse(merged).map_err(|e| anyhow!("XML parse error: {e}"))?;
    let Some(node) = doc
        .descendants()
        .find(|n| n.tag_name().name() == "NextPointeeId")
    else {
        return Ok(());
    };
    let Some(current) = node.attribute("Value").and_then(|v| v.parse::<u64>().ok()) else {
        return Ok(());
    };
    let needed = max_referenced_id(&doc) + 1;
    if current >= needed {
        return Ok(());
    }

    let tag_start = node.range().start;
    let tag_end = tag_start
        + merged[tag_start..]
            .find('>')
            .ok_or_else(|| anyhow!("Internal error: malformed NextPointeeId element"))?;
    let rewritten = merged[tag_start..tag_end].replacen(
        &format!("Value=\"{current}\""),
        &format!("Value=\"{needed}\""),
        1,
    );
    merged.replace_range(tag_start..tag_end, &rewritten);
    Ok(())
}
