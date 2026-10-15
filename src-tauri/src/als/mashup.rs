use anyhow::{anyhow, Result};
use roxmltree::{Document, Node};
use std::collections::HashMap;

/// An empty Live 12 `GroupTrack` (no devices, no clips, `TrackGroupId -1`,
/// an empty `<Sends />` for `pad_track_sends` to fill), cut from a real
/// Live 12.2 set with its Ids renumbered from 1. Most projects have no group
/// track of their own to clone, so each mashup variation's group comes from
/// here.
pub const GROUP_TRACK_TEMPLATE: &str = include_str!("templates/group_track.xml");

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
        // A GrooveId points into the document's GroovePool, not into the
        // track, so it must not move with the track's own ids.
        let tag = n.tag_name().name();
        let tag_ends_in_id = tag.ends_with("Id") && tag != "GrooveId";
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

fn track_group_id<'a>(track: Node<'a, '_>) -> Option<&'a str> {
    track
        .children()
        .find(|c| c.tag_name().name() == "TrackGroupId")
        .and_then(|n| n.attribute("Value"))
}

/// For each of `tracks` (one project's `non_return_tracks`), the index of
/// the top-level track its unit hangs from, following `TrackGroupId` up
/// through groups inside groups. A top-level track is its own top.
fn unit_tops(tracks: &[Node]) -> Result<Vec<usize>> {
    let index_of: HashMap<&str, usize> = tracks
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t.attribute("Id").map(|id| (id, i)))
        .collect();
    (0..tracks.len())
        .map(|i| {
            let mut at = i;
            for _ in 0..=tracks.len() {
                match track_group_id(tracks[at]) {
                    None | Some("-1") => return Ok(at),
                    Some(group) => {
                        at = *index_of
                            .get(group)
                            .ok_or_else(|| anyhow!("track grouped under missing group {group}"))?
                    }
                }
            }
            Err(anyhow!("track grouping loops back on itself"))
        })
        .collect()
}

/// `picks` sorted into the source's own track order, after checking every
/// index is in range and every picked unit is picked whole.
fn whole_units(picks: &[usize], tops: &[usize]) -> Result<Vec<usize>> {
    let mut picks = picks.to_vec();
    picks.sort_unstable();
    picks.dedup();
    if let Some(&idx) = picks.iter().find(|&&i| i >= tops.len()) {
        return Err(anyhow!("track index {idx} out of range for source project"));
    }
    for &idx in &picks {
        let top = tops[idx];
        if let Some(missing) =
            (0..tops.len()).find(|&j| tops[j] == top && picks.binary_search(&j).is_err())
        {
            return Err(anyhow!(
                "track {idx} is picked but track {missing} of the same group is not; groups are picked whole"
            ));
        }
    }
    Ok(picks)
}

/// SplitMix64: small, fast and good enough to pick tracks, so no crate.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let z = (self.0 ^ (self.0 >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        let z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// `n` variations (1 to 6) over `sources`. Var 1 picks every unit. Each
/// later one keeps every unit with a 50% chance, but always at least one
/// unit from each source that has any, and is redrawn (up to 20 times) if
/// it repeats an earlier variation; a source with very few units can't
/// always give `n` different ones. The same seed always gives the same
/// plan.
pub fn plan_variations(sources: &[Document], n: usize, seed: u64) -> Result<Vec<Variation>> {
    if !(1..=6).contains(&n) {
        return Err(anyhow!("variations must be 1 to 6, got {n}"));
    }
    // units[s][u] = the track indices of source s's unit u, in order.
    let units = sources
        .iter()
        .map(|doc| {
            let tops = unit_tops(&non_return_tracks(doc)?)?;
            let mut units: Vec<Vec<usize>> = Vec::new();
            for (idx, &top) in tops.iter().enumerate() {
                match units.iter_mut().find(|u| tops[u[0]] == top) {
                    Some(unit) => unit.push(idx),
                    None => units.push(vec![idx]),
                }
            }
            Ok(units)
        })
        .collect::<Result<Vec<_>>>()?;

    let mut rng = SplitMix64(seed);
    let mut draw = || -> Vec<Vec<usize>> {
        units
            .iter()
            .map(|source_units| {
                let mut kept: Vec<usize> = (0..source_units.len())
                    .filter(|_| rng.next() & 1 == 1)
                    .collect();
                if kept.is_empty() && !source_units.is_empty() {
                    kept.push((rng.next() % source_units.len() as u64) as usize);
                }
                kept
            })
            .collect()
    };
    let mut plans: Vec<Vec<Vec<usize>>> =
        vec![units.iter().map(|u| (0..u.len()).collect()).collect()];
    while plans.len() < n {
        let mut plan = draw();
        for _ in 0..20 {
            if !plans.contains(&plan) {
                break;
            }
            plan = draw();
        }
        plans.push(plan);
    }

    Ok(plans
        .into_iter()
        .enumerate()
        .map(|(i, plan)| Variation {
            name: format!("Var {}", i + 1),
            picks: plan
                .iter()
                .zip(&units)
                .map(|(kept, source_units)| {
                    kept.iter()
                        .flat_map(|&u| source_units[u].iter().copied())
                        .collect()
                })
                .collect(),
        })
        .collect())
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
    let tracks_node = docs[0]
        .descendants()
        .find(|n| n.tag_name().name() == "Tracks")
        .ok_or_else(|| anyhow!("no <Tracks> element found in project"))?;
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
    merged.push_str(&base.xml[..tracks_node.range().start]);
    merged.push_str(&format!("<Tracks>{body}\n\t\t</Tracks>"));
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
    use super::{max_referenced_id, mixer_sends, track_group_id, Document};
    use std::collections::HashSet;

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

            // Live reads groups top-down: a track can only belong to a
            // group listed above it ("Track grouping corrupt" otherwise).
            let mut groups_so_far = HashSet::new();
            for track in tracks_node.children().filter(|n| n.is_element()) {
                if let Some(group) = track_group_id(track).filter(|g| *g != "-1") {
                    if !groups_so_far.contains(group) {
                        problems.push(format!(
                            "{} Id={} has TrackGroupId {group}, which is not an earlier GroupTrack",
                            track.tag_name().name(),
                            track.attribute("Id").unwrap_or("?")
                        ));
                    }
                }
                if track.tag_name().name() == "GroupTrack" {
                    groups_so_far.extend(track.attribute("Id"));
                }
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
