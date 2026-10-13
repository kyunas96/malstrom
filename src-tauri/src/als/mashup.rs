use anyhow::{anyhow, Result};
use roxmltree::{Document, Node};

/// One source project's raw (decompressed) XML plus the positions of the
/// tracks to pull from it -- indices into the same top-level, non-return
/// track order `categorize::list_tracks` returns, so the frontend's
/// checklist and this merge always agree on "track 2".
pub struct MashupSource {
    pub xml: String,
    pub track_indices: Vec<usize>,
}

pub fn non_return_tracks<'a, 'input>(doc: &'a Document<'input>) -> Result<Vec<Node<'a, 'input>>> {
    let tracks_node = doc
        .descendants()
        .find(|n| n.tag_name().name() == "Tracks")
        .ok_or_else(|| anyhow!("no <Tracks> element found in project"))?;
    Ok(tracks_node
        .children()
        .filter(|n| n.is_element())
        .filter(|n| n.tag_name().name() != "ReturnTrack")
        .collect())
}

/// A node's own text plus its leading indentation, so splicing several of
/// these back-to-back reproduces readable, indented XML rather than one
/// run-on line.
fn block_with_indent(xml: &str, node: &Node) -> String {
    let start = node.range().start;
    let line_start = xml[..start].rfind('\n').map(|i| i + 1).unwrap_or(start);
    xml[line_start..node.range().end].to_string()
}

/// The highest `Id` any element in the document claims, or that any
/// `*Id`-named element (e.g. `PointeeId`) points at via its `Value`.
/// `NextPointeeId` itself is excluded even though its tag matches: it's a
/// forward-looking counter, not a reference to anything, so counting its
/// own `Value` here would make it impossible to ever raise it far enough
/// (it would always need to exceed itself).
fn max_referenced_id(doc: &Document) -> u64 {
    doc.descendants()
        .flat_map(|n| {
            let tag = n.tag_name().name();
            let is_reference_holder = tag.ends_with("Id") && tag != "NextPointeeId";
            n.attributes()
                .filter(move |a| a.name() == "Id" || (is_reference_holder && a.name() == "Value"))
                .filter_map(|a| a.value().parse::<u64>().ok())
                .collect::<Vec<_>>()
        })
        .max()
        .unwrap_or(0)
}

/// Replaces the text at each `(range, new_value)` edit, back-to-front so
/// earlier ranges stay valid regardless of how a replacement's length
/// differs from the original.
fn apply_attribute_edits(xml: &str, mut edits: Vec<(std::ops::Range<usize>, String)>) -> String {
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut out = xml.to_string();
    for (range, new_value) in edits {
        out.replace_range(range, &new_value);
    }
    out
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
fn strip_groove_references(block: &str) -> Result<String> {
    let doc = Document::parse(block).map_err(|e| anyhow!("XML parse error while stripping grooves: {e}"))?;
    let edits: Vec<(std::ops::Range<usize>, String)> = doc
        .descendants()
        .filter(|n| n.tag_name().name() == "GrooveId")
        .filter_map(|n| n.attributes().find(|a| a.name() == "Value"))
        .filter(|a| a.value() != "-1")
        .map(|a| (a.range_value(), "-1".to_string()))
        .collect();
    Ok(apply_attribute_edits(block, edits))
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
fn shift_ids_in_block(block: &str, offset: u64) -> Result<String> {
    if offset == 0 {
        return Ok(block.to_string());
    }
    let doc = Document::parse(block).map_err(|e| anyhow!("XML parse error while remapping ids: {e}"))?;
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for n in doc.descendants() {
        let tag_ends_in_id = n.tag_name().name().ends_with("Id");
        for attr in n.attributes() {
            let relevant = attr.name() == "Id" || (tag_ends_in_id && attr.name() == "Value");
            let Some(original) = relevant.then(|| attr.value().parse::<u64>().ok()).flatten() else {
                continue;
            };
            edits.push((attr.range_value(), (original + offset).to_string()));
        }
    }
    Ok(apply_attribute_edits(block, edits))
}

/// Raises `<NextPointeeId Value="...">` to stay above every id now present
/// in `merged`, if the value inherited from `sources[0]` is no longer high
/// enough -- otherwise Live refuses to load the file ("NextPointeeId is too
/// low"). A no-op if the project has no such element (older format) or its
/// value already clears the new high-water mark.
fn ensure_next_pointee_id_is_high_enough(merged: &mut String) -> Result<()> {
    let doc = Document::parse(merged).map_err(|e| anyhow!("XML parse error: {e}"))?;
    let Some(node) = doc.descendants().find(|n| n.tag_name().name() == "NextPointeeId") else {
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
    let rewritten =
        merged[tag_start..tag_end].replacen(&format!("Value=\"{current}\""), &format!("Value=\"{needed}\""), 1);
    merged.replace_range(tag_start..tag_end, &rewritten);
    Ok(())
}

/// Merges the selected tracks from each source into `sources[0]`'s project.
/// Tempo, time signature, and the master/return tracks all come from that
/// first source -- the documented v0 limitation, not solved here. Every
/// other track in every source (checked or not) is dropped; only the
/// checked ones are spliced in, in source order. Returns the merged XML;
/// callers decide where (and whether) to write it.
pub fn build_mashup(sources: &[MashupSource]) -> Result<String> {
    let base = sources
        .first()
        .ok_or_else(|| anyhow!("no source projects given"))?;
    let base_doc = Document::parse(&base.xml).map_err(|e| anyhow!("XML parse error: {e}"))?;
    let tracks_node = base_doc
        .descendants()
        .find(|n| n.tag_name().name() == "Tracks")
        .ok_or_else(|| anyhow!("no <Tracks> element found in project"))?;

    let return_tracks: String = tracks_node
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "ReturnTrack")
        .map(|n| block_with_indent(&base.xml, &n))
        .collect();

    // Ids are only unique within their own project. The base source (index
    // 0) keeps its ids as-is; every other source's picked tracks get every
    // id-bearing value in their subtree shifted above the base document's
    // own highest id, so nothing in them can collide with it. v0 caps
    // sources at exactly 2, so there's only ever one donor to shift.
    let base_max_id = max_referenced_id(&base_doc);

    let mut picked_tracks = String::new();
    for (source_index, source) in sources.iter().enumerate() {
        let doc = Document::parse(&source.xml).map_err(|e| anyhow!("XML parse error: {e}"))?;
        let tracks = non_return_tracks(&doc)?;
        let offset = if source_index == 0 { 0 } else { base_max_id + 1 };
        for &idx in &source.track_indices {
            let node = tracks
                .get(idx)
                .ok_or_else(|| anyhow!("track index {idx} out of range for source project"))?;
            let block = block_with_indent(&source.xml, node);
            let block = if offset > 0 { strip_groove_references(&block)? } else { block };
            picked_tracks.push_str(&shift_ids_in_block(&block, offset)?);
        }
    }

    let new_tracks_block = format!("<Tracks>{picked_tracks}{return_tracks}\n\t\t</Tracks>");
    let mut merged = String::with_capacity(base.xml.len());
    merged.push_str(&base.xml[..tracks_node.range().start]);
    merged.push_str(&new_tracks_block);
    merged.push_str(&base.xml[tracks_node.range().end..]);

    ensure_next_pointee_id_is_high_enough(&mut merged)?;

    // Byte-range splice, not a re-serialization -- verify it's still
    // well-formed before any caller writes it to disk.
    Document::parse(&merged)
        .map_err(|e| anyhow!("Internal error: merged XML failed to re-parse ({e}), refusing to write"))?;

    Ok(merged)
}

/// Structural checks against a `build_mashup` result, standing in for
/// "will Ableton Live refuse to load this file" without launching Live --
/// every invariant here is one that Live's own loader has actually
/// rejected during this project's development, so a clean result is
/// necessary but not sufficient proof the file opens cleanly. Shared
/// between `mashup.rs`'s own tests and the `mashup_batch_test` CLI so both
/// check exactly the same things.
pub mod diagnostics {
    use super::{max_referenced_id, Document};

    /// Human-readable problems found in `xml`; empty means every known
    /// invariant holds.
    pub fn check(xml: &str) -> Vec<String> {
        let doc = match Document::parse(xml) {
            Ok(doc) => doc,
            Err(e) => return vec![format!("XML does not parse: {e}")],
        };

        let mut problems = Vec::new();

        // `Id` is only meaningful as a unique key within specific lists --
        // e.g. every real, untouched, valid project already reuses Id
        // values across separate ClipSlot/KeyTrack/AutomationLane/etc.
        // scopes by the hundreds, so checking the whole document would flag
        // pristine files as corrupt. The one list this project has actually
        // seen Live reject on ("Non-unique list ids") is the top-level
        // <Tracks> list itself, so that's what's checked here.
        if let Some(tracks_node) = doc.descendants().find(|n| n.tag_name().name() == "Tracks") {
            let ids: Vec<&str> = tracks_node
                .children()
                .filter(|n| n.is_element())
                .filter_map(|n| n.attribute("Id"))
                .collect();
            let mut unique_ids = ids.clone();
            unique_ids.sort();
            unique_ids.dedup();
            if unique_ids.len() != ids.len() {
                problems.push(format!(
                    "duplicate Id among top-level tracks ({} tracks, {} unique Ids)",
                    ids.len(),
                    unique_ids.len()
                ));
            }
        }

        if let Some(current) = doc
            .descendants()
            .find(|n| n.tag_name().name() == "NextPointeeId")
            .and_then(|n| n.attribute("Value"))
            .and_then(|v| v.parse::<u64>().ok())
        {
            let needed = max_referenced_id(&doc) + 1;
            if current < needed {
                problems.push(format!("NextPointeeId ({current}) is too low; must be at least {needed}"));
            }
        }

        problems
    }
}
