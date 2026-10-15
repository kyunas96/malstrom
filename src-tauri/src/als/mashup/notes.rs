//! Reading and rewriting a MIDI clip's notes, for the stages that make new
//! note content (`cutup`, `fuse`).

use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use roxmltree::{Document, Node};

use super::{apply_edits, child_value};

/// One MIDI note: its key, its start in beats, and its `MidiNoteEvent`
/// attributes as Live wrote them.
#[derive(Clone)]
pub(super) struct Note {
    pub key: i32,
    pub time: f64,
    pub attrs: Vec<(String, String)>,
}

/// A MIDI clip's notes and its loop, in beats.
pub(super) struct Notes {
    pub notes: Vec<Note>,
    pub loop_start: f64,
    pub loop_end: f64,
}

fn parse(value: &str) -> Result<Document<'_>> {
    Document::parse(value).map_err(|e| anyhow!("XML parse error while reading notes: {e}"))
}

fn midi_clip<'a, 'i>(doc: &'a Document<'i>) -> Option<Node<'a, 'i>> {
    doc.descendants()
        .find(|n| n.tag_name().name() == "MidiClip")
}

fn loop_value<'a, 'i>(clip: Node<'a, 'i>, tag: &str) -> Option<roxmltree::Attribute<'a, 'i>> {
    clip.children()
        .find(|c| c.tag_name().name() == "Loop")
        .and_then(|l| child_value(l, tag))
}

/// The notes of the MIDI clip in `value` (a slot's `<Value>`). `None` when
/// it holds no MIDI clip.
pub(super) fn read(value: &str) -> Result<Option<Notes>> {
    let doc = parse(value)?;
    let Some(clip) = midi_clip(&doc) else {
        return Ok(None);
    };
    let beats = |tag| {
        loop_value(clip, tag)
            .and_then(|a| a.value().parse().ok())
            .unwrap_or(0.0)
    };
    let notes = clip
        .descendants()
        .filter(|n| n.tag_name().name() == "KeyTrack")
        .flat_map(|track| {
            let key = child_value(track, "MidiKey")
                .and_then(|a| a.value().parse().ok())
                .unwrap_or(60);
            track
                .descendants()
                .filter(|n| n.tag_name().name() == "MidiNoteEvent")
                .map(move |event| Note {
                    key,
                    time: event
                        .attribute("Time")
                        .and_then(|t| t.parse().ok())
                        .unwrap_or(0.0),
                    attrs: event
                        .attributes()
                        .map(|a| (a.name().to_string(), a.value().to_string()))
                        .collect(),
                })
        })
        .collect();
    Ok(Some(Notes {
        notes,
        loop_start: beats("LoopStart"),
        loop_end: beats("LoopEnd"),
    }))
}

/// `value` with its MIDI clip's notes replaced by `notes`, one KeyTrack
/// per key. NoteIds are renumbered from 1 and per-note (MPE) events are
/// dropped, since they point at the old NoteIds. With `loop_end`, the
/// clip's loop and end markers move there too.
pub(super) fn write(value: &str, notes: &[Note], loop_end: Option<f64>) -> Result<String> {
    let doc = parse(value)?;
    let clip = midi_clip(&doc).ok_or_else(|| anyhow!("no MIDI clip to write notes into"))?;
    let find = |tag: &str| clip.descendants().find(|n| n.tag_name().name() == tag);
    let key_tracks = find("KeyTracks").ok_or_else(|| anyhow!("MIDI clip has no KeyTracks"))?;

    let mut by_key: BTreeMap<i32, Vec<&Note>> = BTreeMap::new();
    for note in notes {
        by_key.entry(note.key).or_default().push(note);
    }
    let mut next_id = 1;
    let mut tracks = String::from("<KeyTracks>");
    for (i, (key, mut notes)) in by_key.into_iter().enumerate() {
        notes.sort_by(|a, b| a.time.total_cmp(&b.time));
        tracks.push_str(&format!(r#"<KeyTrack Id="{i}"><Notes>"#));
        for note in notes {
            tracks.push_str("<MidiNoteEvent");
            for (name, v) in &note.attrs {
                let v = match name.as_str() {
                    "Time" => note.time.to_string(),
                    "NoteId" => {
                        next_id += 1;
                        (next_id - 1).to_string()
                    }
                    _ => v.clone(),
                };
                tracks.push_str(&format!(r#" {name}="{v}""#));
            }
            tracks.push_str(" />");
        }
        tracks.push_str(&format!(r#"</Notes><MidiKey Value="{key}" /></KeyTrack>"#));
    }
    tracks.push_str("</KeyTracks>");

    let mut edits = vec![(key_tracks.range(), tracks)];
    if let Some(lists) = find("PerNoteEventStore")
        .and_then(|s| s.children().find(|c| c.tag_name().name() == "EventLists"))
    {
        edits.push((lists.range(), "<EventLists />".to_string()));
    }
    if let Some(id) = find("NoteIdGenerator").and_then(|g| child_value(g, "NextId")) {
        edits.push((id.range_value(), next_id.to_string()));
    }
    if let Some(end) = loop_end {
        let markers = ["LoopEnd", "OutMarker", "HiddenLoopEnd"]
            .iter()
            .filter_map(|t| loop_value(clip, t))
            .chain(child_value(clip, "CurrentEnd"));
        edits.extend(markers.map(|a| (a.range_value(), end.to_string())));
    }
    Ok(apply_edits(value, edits))
}
