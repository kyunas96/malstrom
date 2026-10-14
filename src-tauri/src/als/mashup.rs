use anyhow::{anyhow, Result};
use roxmltree::{Document, Node};

/// An empty Live 12 `GroupTrack` (no devices, no clips, `TrackGroupId -1`,
/// an empty `<Sends />` for `pad_track_sends` to fill), cut from a real
/// Live 12.2 set with its Ids renumbered from 1. Most projects have no group
/// track of their own to clone, so each mashup variation's group comes from
/// here.
pub const GROUP_TRACK_TEMPLATE: &str = include_str!("templates/group_track.xml");

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

fn return_tracks<'a, 'input>(doc: &'a Document<'input>) -> Result<Vec<Node<'a, 'input>>> {
    let tracks_node = doc
        .descendants()
        .find(|n| n.tag_name().name() == "Tracks")
        .ok_or_else(|| anyhow!("no <Tracks> element found in project"))?;
    Ok(tracks_node
        .children()
        .filter(|n| n.tag_name().name() == "ReturnTrack")
        .collect())
}

/// A track's own `DeviceChain/Mixer/Sends` -- one `TrackSendHolder` per
/// return track in the set, matched to the returns by position (the
/// holders' own Ids are only list-local keys; a real project mixes
/// `0,1,2,3` and `0,3,1,4` across tracks). Walking the exact path, rather
/// than searching for any `<Sends>`, skips the separate Sends lists that
/// rack chains carry inside their devices.
fn mixer_sends<'a, 'input>(track: Node<'a, 'input>) -> Option<Node<'a, 'input>> {
    ["DeviceChain", "Mixer", "Sends"]
        .iter()
        .try_fold(track, |node, tag| {
            node.children().find(|c| c.tag_name().name() == *tag)
        })
}

/// The first send holder on any track in `doc`, as indented XML, to clone
/// padding sends from -- so they match the project's own Live version's
/// format instead of a hard-coded one. `None` when no track has a send,
/// i.e. the project has no return tracks.
fn first_send_holder(doc: &Document, xml: &str) -> Option<String> {
    let tracks_node = doc
        .descendants()
        .find(|n| n.tag_name().name() == "Tracks")?;
    tracks_node
        .children()
        .filter_map(mixer_sends)
        .find_map(|sends| sends.first_element_child())
        .map(|holder| block_with_indent(xml, &holder))
}

/// A copy of `template` (a `TrackSendHolder`) turned all the way down and
/// with fresh Ids from `next_id`, so it can't collide with anything already
/// in the merged set. "Down" is the send's own `MidiControllerRange/Min`
/// (-70 dB in Live 12), which is what Live itself stores for an unused send.
fn padding_send_holder(template: &str, next_id: &mut u64) -> Result<String> {
    let doc =
        Document::parse(template).map_err(|e| anyhow!("XML parse error in send template: {e}"))?;
    let send = doc
        .root_element()
        .children()
        .find(|n| n.tag_name().name() == "Send")
        .ok_or_else(|| anyhow!("send template has no <Send>"))?;
    let off = send
        .children()
        .find(|n| n.tag_name().name() == "MidiControllerRange")
        .and_then(|range| range.children().find(|n| n.tag_name().name() == "Min"))
        .and_then(|min| min.attribute("Value"))
        .ok_or_else(|| anyhow!("send template has no MidiControllerRange/Min"))?;
    let manual = send
        .children()
        .find(|n| n.tag_name().name() == "Manual")
        .and_then(|manual| manual.attributes().find(|a| a.name() == "Value"))
        .ok_or_else(|| anyhow!("send template has no Manual value"))?;

    let mut edits = vec![(manual.range_value(), off.to_string())];
    for node in doc.descendants() {
        for attr in node.attributes().filter(|a| a.name() == "Id") {
            edits.push((attr.range_value(), next_id.to_string()));
            *next_id += 1;
        }
    }
    Ok(format!("\n{}", apply_attribute_edits(template, edits)))
}

/// Splices `prepend`/`append` in as `node`'s first/last children, expanding
/// a self-closing `<Tag />` (an empty Sends list in a set with no returns)
/// into an open/close pair first.
fn insert_children(xml: &str, node: &Node, prepend: &str, append: &str) -> Result<String> {
    let start = node.range().start;
    let open_end = start
        + xml[start..].find('>').ok_or_else(|| {
            anyhow!(
                "Internal error: malformed <{}> element",
                node.tag_name().name()
            )
        })?
        + 1;
    let mut out = String::with_capacity(xml.len() + prepend.len() + append.len());
    if xml[..open_end].ends_with("/>") {
        out.push_str(xml[..open_end - 2].trim_end());
        out.push_str(&format!(">{prepend}{append}</{}>", node.tag_name().name()));
        out.push_str(&xml[open_end..]);
        return Ok(out);
    }
    let append_at = node
        .last_element_child()
        .map_or(open_end, |c| c.range().end);
    out.push_str(&xml[..open_end]);
    out.push_str(prepend);
    out.push_str(&xml[open_end..append_at]);
    out.push_str(append);
    out.push_str(&xml[append_at..]);
    Ok(out)
}

/// Gives a track `before` padding sends ahead of its own and `after` behind
/// them, so its own sends line up with its own project's returns once
/// every source's returns sit side by side in the merged set. A no-op for
/// a track with no Mixer Sends.
fn pad_track_sends(
    block: &str,
    before: usize,
    after: usize,
    template: Option<&str>,
    next_id: &mut u64,
) -> Result<String> {
    if before + after == 0 {
        return Ok(block.to_string());
    }
    let doc =
        Document::parse(block).map_err(|e| anyhow!("XML parse error while padding sends: {e}"))?;
    let Some(sends) = mixer_sends(doc.root_element()) else {
        return Ok(block.to_string());
    };
    let template =
        template.ok_or_else(|| anyhow!("Internal error: no send holder to pad sends with"))?;
    let mut holders = |count: usize| -> Result<String> {
        (0..count)
            .map(|_| padding_send_holder(template, next_id))
            .collect()
    };
    let prepend = holders(before)?;
    let append = holders(after)?;
    insert_children(block, &sends, &prepend, &append)
}

/// Appends `extra` (donor `SendPreBool`s, already id-shifted) to the merged
/// set's `<SendsPre>` -- one pre/post-fader flag per return, positional
/// like the sends. A no-op if the base has no SendsPre (older format).
fn append_sends_pre(merged: &mut String, extra: &str) -> Result<()> {
    let updated = {
        let doc = Document::parse(merged).map_err(|e| anyhow!("XML parse error: {e}"))?;
        let Some(sends_pre) = doc
            .descendants()
            .find(|n| n.tag_name().name() == "SendsPre")
        else {
            return Ok(());
        };
        insert_children(merged, &sends_pre, "", extra)?
    };
    *merged = updated;
    Ok(())
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
    let doc = Document::parse(block)
        .map_err(|e| anyhow!("XML parse error while stripping grooves: {e}"))?;
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
    let doc =
        Document::parse(block).map_err(|e| anyhow!("XML parse error while remapping ids: {e}"))?;
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for n in doc.descendants() {
        let tag_ends_in_id = n.tag_name().name().ends_with("Id");
        for attr in n.attributes() {
            let relevant = attr.name() == "Id" || (tag_ends_in_id && attr.name() == "Value");
            let Some(original) = relevant.then(|| attr.value().parse::<u64>().ok()).flatten()
            else {
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

/// Merges the selected tracks from each source into `sources[0]`'s project.
/// Tempo, time signature, and the master track come from that first source
/// -- the documented v0 limitation, not solved here. Every source's return
/// tracks are kept, in source order, and every track's sends are padded so
/// each one still feeds its own project's returns (see `pad_track_sends`).
/// Every other track in every source (checked or not) is dropped; only the
/// checked ones are spliced in, in source order. Returns the merged XML;
/// callers decide where (and whether) to write it.
pub fn build_mashup(sources: &[MashupSource]) -> Result<String> {
    let base = sources
        .first()
        .ok_or_else(|| anyhow!("no source projects given"))?;
    let docs = sources
        .iter()
        .map(|s| Document::parse(&s.xml).map_err(|e| anyhow!("XML parse error: {e}")))
        .collect::<Result<Vec<_>>>()?;
    let base_doc = &docs[0];
    let tracks_node = base_doc
        .descendants()
        .find(|n| n.tag_name().name() == "Tracks")
        .ok_or_else(|| anyhow!("no <Tracks> element found in project"))?;

    // Ids are only unique within their own project. The base source (index
    // 0) keeps its ids as-is; every other source's picked tracks get every
    // id-bearing value in their subtree shifted above the base document's
    // own highest id, so nothing in them can collide with it. v0 caps
    // sources at exactly 2, so there's only ever one donor to shift.
    let base_max_id = max_referenced_id(base_doc);
    let offsets: Vec<u64> = (0..sources.len())
        .map(|i| if i == 0 { 0 } else { base_max_id + 1 })
        .collect();

    // Padding sends need ids of their own, above everything any source
    // occupies once shifted.
    let mut next_id = docs
        .iter()
        .zip(&offsets)
        .map(|(doc, offset)| max_referenced_id(doc) + offset)
        .max()
        .unwrap_or(0)
        + 1;
    let send_template = docs
        .iter()
        .zip(sources)
        .find_map(|(doc, s)| first_send_holder(doc, &s.xml));
    let return_counts = docs
        .iter()
        .map(|doc| return_tracks(doc).map(|r| r.len()))
        .collect::<Result<Vec<_>>>()?;
    let total_returns: usize = return_counts.iter().sum();

    let mut picked_tracks = String::new();
    let mut all_returns = String::new();
    let mut donor_sends_pre = String::new();
    let mut returns_before = 0;
    for (source_index, (source, doc)) in sources.iter().zip(&docs).enumerate() {
        let offset = offsets[source_index];
        // This source's returns land after every earlier source's, so its
        // tracks need that many padding sends ahead of their own, and one
        // per later source's return behind them.
        let before = returns_before;
        let after = total_returns - before - return_counts[source_index];
        returns_before += return_counts[source_index];

        let mut prepare = |node: Node| -> Result<String> {
            let block = block_with_indent(&source.xml, &node);
            let block = if offset > 0 {
                strip_groove_references(&block)?
            } else {
                block
            };
            let block = shift_ids_in_block(&block, offset)?;
            pad_track_sends(
                &block,
                before,
                after,
                send_template.as_deref(),
                &mut next_id,
            )
        };

        let tracks = non_return_tracks(doc)?;
        for &idx in &source.track_indices {
            let node = tracks
                .get(idx)
                .ok_or_else(|| anyhow!("track index {idx} out of range for source project"))?;
            picked_tracks.push_str(&prepare(*node)?);
        }
        for node in return_tracks(doc)? {
            all_returns.push_str(&prepare(node)?);
        }

        if offset > 0 {
            if let Some(sends_pre) = doc
                .descendants()
                .find(|n| n.tag_name().name() == "SendsPre")
            {
                for flag in sends_pre.children().filter(|n| n.is_element()) {
                    donor_sends_pre.push('\n');
                    donor_sends_pre.push_str(&shift_ids_in_block(
                        &block_with_indent(&source.xml, &flag),
                        offset,
                    )?);
                }
            }
        }
    }

    let new_tracks_block = format!("<Tracks>{picked_tracks}{all_returns}\n\t\t</Tracks>");
    let mut merged = String::with_capacity(base.xml.len());
    merged.push_str(&base.xml[..tracks_node.range().start]);
    merged.push_str(&new_tracks_block);
    merged.push_str(&base.xml[tracks_node.range().end..]);

    append_sends_pre(&mut merged, &donor_sends_pre)?;
    ensure_next_pointee_id_is_high_enough(&mut merged)?;

    // Byte-range splice, not a re-serialization -- verify it's still
    // well-formed before any caller writes it to disk.
    Document::parse(&merged).map_err(|e| {
        anyhow!("Internal error: merged XML failed to re-parse ({e}), refusing to write")
    })?;

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
    use super::{max_referenced_id, mixer_sends, Document};

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

            // Sends match returns by position, so every track (returns
            // included) needs exactly one per return, as does SendsPre.
            let return_count = tracks_node
                .children()
                .filter(|n| n.tag_name().name() == "ReturnTrack")
                .count();
            for track in tracks_node.children().filter(|n| n.is_element()) {
                if let Some(sends) = mixer_sends(track) {
                    let count = sends.children().filter(|n| n.is_element()).count();
                    if count != return_count {
                        problems.push(format!(
                            "{} Id={} has {count} sends but the set has {return_count} return tracks",
                            track.tag_name().name(),
                            track.attribute("Id").unwrap_or("?")
                        ));
                    }
                }
            }
            if let Some(sends_pre) = doc
                .descendants()
                .find(|n| n.tag_name().name() == "SendsPre")
            {
                let count = sends_pre.children().filter(|n| n.is_element()).count();
                if count != return_count {
                    problems.push(format!(
                        "SendsPre has {count} entries but the set has {return_count} return tracks"
                    ));
                }
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
                problems.push(format!(
                    "NextPointeeId ({current}) is too low; must be at least {needed}"
                ));
            }
        }

        problems
    }
}
