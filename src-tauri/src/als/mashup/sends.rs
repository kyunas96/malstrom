use super::{apply_edits, block_with_indent, tracks_node};
use anyhow::{anyhow, Result};
use roxmltree::{Document, Node};

/// A track's own `DeviceChain/Mixer/Sends` -- one `TrackSendHolder` per
/// return track in the set, matched to the returns by position (the
/// holders' own Ids are only list-local keys; a real project mixes
/// `0,1,2,3` and `0,3,1,4` across tracks). Walking the exact path, rather
/// than searching for any `<Sends>`, skips the separate Sends lists that
/// rack chains carry inside their devices.
pub(super) fn mixer_sends<'a, 'input>(track: Node<'a, 'input>) -> Option<Node<'a, 'input>> {
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
pub(super) fn first_send_holder(doc: &Document, xml: &str) -> Option<String> {
    tracks_node(doc)
        .ok()?
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
    Ok(format!("\n{}", apply_edits(template, edits)))
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
pub(super) fn pad_track_sends(
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
pub(super) fn append_sends_pre(merged: &mut String, extra: &str) -> Result<()> {
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
