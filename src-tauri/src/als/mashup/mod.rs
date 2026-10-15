use anyhow::{anyhow, Result};
use roxmltree::{Document, Node};

pub mod diagnostics;
mod ids;
mod plan;
mod sends;

use ids::{
    ensure_next_pointee_id_is_high_enough, max_referenced_id, shift_ids_in_block,
    strip_groove_references,
};
pub use plan::plan_variations;
use plan::{unit_tops, whole_units};
use sends::{append_sends_pre, first_send_holder, pad_track_sends};

/// An empty Live 12 `GroupTrack` (no devices, no clips, `TrackGroupId -1`,
/// an empty `<Sends />` for `pad_track_sends` to fill), cut from a real
/// Live 12.2 set with its Ids renumbered from 1. Most projects have no group
/// track of their own to clone, so each mashup variation's group comes from
/// here.
pub const GROUP_TRACK_TEMPLATE: &str = include_str!("../templates/group_track.xml");

/// One source project's raw (decompressed) XML.
pub struct MashupSource {
    pub xml: String,
}

/// One group in the mashup. `picks[s]` holds indices into source `s`'s
/// `non_return_tracks` (the same order `categorize::list_tracks` returns).
/// They must cover whole units: a top-level track plus every track grouped
/// under it. A missing entry picks nothing from that source.
#[derive(Debug, PartialEq)]
pub struct Variation {
    pub name: String,
    pub picks: Vec<Vec<usize>>,
}

/// The set's top-level `<Tracks>` list.
fn tracks_node<'a, 'input>(doc: &'a Document<'input>) -> Result<Node<'a, 'input>> {
    doc.descendants()
        .find(|n| n.tag_name().name() == "Tracks")
        .ok_or_else(|| anyhow!("no <Tracks> element found in project"))
}

pub fn non_return_tracks<'a, 'input>(doc: &'a Document<'input>) -> Result<Vec<Node<'a, 'input>>> {
    Ok(tracks_node(doc)?
        .children()
        .filter(|n| n.is_element())
        .filter(|n| n.tag_name().name() != "ReturnTrack")
        .collect())
}

fn return_tracks<'a, 'input>(doc: &'a Document<'input>) -> Result<Vec<Node<'a, 'input>>> {
    Ok(tracks_node(doc)?
        .children()
        .filter(|n| n.tag_name().name() == "ReturnTrack")
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

/// A track's own `DeviceChain/AudioOutputRouting`.
fn audio_output_routing<'a, 'input>(track: Node<'a, 'input>) -> Option<Node<'a, 'input>> {
    ["DeviceChain", "AudioOutputRouting"]
        .iter()
        .try_fold(track, |node, tag| {
            node.children().find(|c| c.tag_name().name() == *tag)
        })
}

fn child_value<'a, 'input>(
    node: Node<'a, 'input>,
    tag: &str,
) -> Option<roxmltree::Attribute<'a, 'input>> {
    node.children()
        .find(|c| c.tag_name().name() == tag)
        .and_then(|c| c.attributes().find(|a| a.name() == "Value"))
}

/// Puts a unit's top-level track inside the group `group_id`: its
/// `TrackGroupId` points at the group, and if it played to the main out it
/// now plays into the group, as Live writes it (`AudioOut/GroupTrack`).
fn into_group(block: &str, group_id: u64) -> Result<String> {
    let doc = Document::parse(block)
        .map_err(|e| anyhow!("XML parse error while grouping a track: {e}"))?;
    let track = doc.root_element();
    let mut edits = Vec::new();
    if let Some(attr) = child_value(track, "TrackGroupId") {
        edits.push((attr.range_value(), group_id.to_string()));
    }
    if let Some(routing) = audio_output_routing(track) {
        if let Some(target) = child_value(routing, "Target")
            .filter(|t| matches!(t.value(), "AudioOut/Main" | "AudioOut/Master"))
        {
            edits.push((target.range_value(), "AudioOut/GroupTrack".to_string()));
            if let Some(display) = child_value(routing, "UpperDisplayString") {
                edits.push((display.range_value(), "Group".to_string()));
            }
        }
    }
    Ok(apply_attribute_edits(block, edits))
}

/// A copy of `GROUP_TRACK_TEMPLATE` named `name`, with its ids shifted by
/// `offset` (its own track Id becomes `offset`), playing to `main_out`, and
/// with one send per return in the merged set, all turned down.
fn group_block(
    name: &str,
    offset: u64,
    main_out: &str,
    total_returns: usize,
    send_template: Option<&str>,
    next_id: &mut u64,
) -> Result<String> {
    let block = shift_ids_in_block(GROUP_TRACK_TEMPLATE.trim_end(), offset)?;
    let doc =
        Document::parse(&block).map_err(|e| anyhow!("XML parse error in group template: {e}"))?;
    let group = doc.root_element();
    let name = name
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;");
    let name_node = group
        .children()
        .find(|c| c.tag_name().name() == "Name")
        .ok_or_else(|| anyhow!("group template has no <Name>"))?;
    let mut edits = Vec::new();
    for tag in ["EffectiveName", "UserName"] {
        let attr = child_value(name_node, tag)
            .ok_or_else(|| anyhow!("group template has no Name/{tag}"))?;
        edits.push((attr.range_value(), name.clone()));
    }
    let target = audio_output_routing(group)
        .and_then(|r| child_value(r, "Target"))
        .ok_or_else(|| anyhow!("group template has no AudioOutputRouting/Target"))?;
    edits.push((target.range_value(), main_out.to_string()));
    let block = apply_attribute_edits(&block, edits);
    pad_track_sends(&block, 0, total_returns, send_template, next_id)
}

/// Builds one set holding a group per variation, each with the picked units
/// from every source, A's then B's. Tempo, time signature and the master
/// track come from `sources[0]`. Every source's return tracks are kept once,
/// in source order, and every track's sends are padded so each one still
/// feeds its own project's returns (see `pad_track_sends`). Returns the
/// merged XML; callers decide where (and whether) to write it.
///
/// Ids are only unique within one project, and a source can be copied into
/// several variations, so every copy gets its own id range, sized to the
/// source's highest id. Source 0's first copy keeps its own ids (they
/// already fit around the rest of the base document). Every other source's
/// first copy shares a range with that source's returns and `SendsPre`
/// flags. Every later copy, every group and every padding send takes the
/// next free ids above all of those.
pub fn build_mashup(sources: &[MashupSource], variations: &[Variation]) -> Result<String> {
    let base = sources
        .first()
        .ok_or_else(|| anyhow!("no source projects given"))?;
    if variations.is_empty() {
        return Err(anyhow!("no variations given"));
    }
    let docs = sources
        .iter()
        .map(|s| Document::parse(&s.xml).map_err(|e| anyhow!("XML parse error: {e}")))
        .collect::<Result<Vec<_>>>()?;
    let base_tracks = tracks_node(&docs[0])?;
    let source_tracks = docs
        .iter()
        .map(non_return_tracks)
        .collect::<Result<Vec<_>>>()?;
    let tops = source_tracks
        .iter()
        .map(|t| unit_tops(t))
        .collect::<Result<Vec<_>>>()?;
    // Live 11 calls the main out "Master", Live 12 "Main".
    let main_out = if base.xml.contains("\"AudioOut/Master\"") {
        "AudioOut/Master"
    } else {
        "AudioOut/Main"
    };

    let spans: Vec<u64> = docs.iter().map(|d| max_referenced_id(d) + 1).collect();
    let group_span =
        max_referenced_id(&Document::parse(GROUP_TRACK_TEMPLATE).map_err(|e| anyhow!("{e}"))?) + 1;
    let mut next_id = spans[0];
    let mut home = vec![0];
    for span in &spans[1..] {
        home.push(next_id);
        next_id += span;
    }

    let send_template = docs
        .iter()
        .zip(sources)
        .find_map(|(doc, s)| first_send_holder(doc, &s.xml));
    let return_counts = docs
        .iter()
        .map(|doc| return_tracks(doc).map(|r| r.len()))
        .collect::<Result<Vec<_>>>()?;
    let total_returns: usize = return_counts.iter().sum();
    // Each source's returns land after every earlier source's, so its
    // tracks need that many padding sends ahead of their own, and one per
    // later source's return behind them.
    let befores: Vec<usize> = return_counts
        .iter()
        .scan(0, |sum, n| {
            let before = *sum;
            *sum += n;
            Some(before)
        })
        .collect();

    let prepare = |s: usize, node: Node, offset: u64, next_id: &mut u64| -> Result<String> {
        let block = block_with_indent(&sources[s].xml, &node);
        let block = if s > 0 {
            strip_groove_references(&block)?
        } else {
            block
        };
        let block = shift_ids_in_block(&block, offset)?;
        let after = total_returns - befores[s] - return_counts[s];
        let block = pad_track_sends(&block, befores[s], after, send_template.as_deref(), next_id)?;
        Ok(format!("\n{block}"))
    };

    let mut body = String::new();
    let mut copies = vec![0usize; sources.len()];
    for variation in variations {
        if variation.picks.len() > sources.len() {
            return Err(anyhow!(
                "{} picks from {} sources but only {} were given",
                variation.name,
                variation.picks.len(),
                sources.len()
            ));
        }
        let picks = (0..sources.len())
            .map(|s| whole_units(variation.picks.get(s).map_or(&[], |p| p), &tops[s]))
            .collect::<Result<Vec<_>>>()?;
        if picks.iter().all(|p| p.is_empty()) {
            return Err(anyhow!("{} picks no tracks", variation.name));
        }

        let group_id = next_id;
        next_id += group_span;
        body.push('\n');
        body.push_str(&group_block(
            &variation.name,
            group_id,
            main_out,
            total_returns,
            send_template.as_deref(),
            &mut next_id,
        )?);

        for (s, picks) in picks.iter().enumerate().filter(|(_, p)| !p.is_empty()) {
            let offset = if copies[s] == 0 {
                home[s]
            } else {
                next_id += spans[s];
                next_id - spans[s]
            };
            copies[s] += 1;
            for &idx in picks {
                let block = prepare(s, source_tracks[s][idx], offset, &mut next_id)?;
                body.push_str(&if tops[s][idx] == idx {
                    into_group(&block, group_id)?
                } else {
                    block
                });
            }
        }
    }

    let mut donor_sends_pre = String::new();
    for (s, doc) in docs.iter().enumerate() {
        for node in return_tracks(doc)? {
            body.push_str(&prepare(s, node, home[s], &mut next_id)?);
        }
        if s == 0 {
            continue;
        }
        if let Some(sends_pre) = doc
            .descendants()
            .find(|n| n.tag_name().name() == "SendsPre")
        {
            for flag in sends_pre.children().filter(|n| n.is_element()) {
                donor_sends_pre.push('\n');
                donor_sends_pre.push_str(&shift_ids_in_block(
                    &block_with_indent(&sources[s].xml, &flag),
                    home[s],
                )?);
            }
        }
    }

    let mut merged = String::with_capacity(base.xml.len());
    merged.push_str(&base.xml[..base_tracks.range().start]);
    merged.push_str(&format!("<Tracks>{body}\n\t\t</Tracks>"));
    merged.push_str(&base.xml[base_tracks.range().end..]);

    append_sends_pre(&mut merged, &donor_sends_pre)?;
    ensure_next_pointee_id_is_high_enough(&mut merged)?;

    // Byte-range splice, not a re-serialization -- verify it's still
    // well-formed before any caller writes it to disk.
    Document::parse(&merged).map_err(|e| {
        anyhow!("Internal error: merged XML failed to re-parse ({e}), refusing to write")
    })?;

    Ok(merged)
}
