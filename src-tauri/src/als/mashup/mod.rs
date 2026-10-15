use anyhow::{anyhow, Result};
use roxmltree::{Document, Node};

pub mod diagnostics;
mod ids;
mod layering;
mod medley;
pub mod pipeline;
mod plan;
mod sends;
mod session;
mod transpose;
mod twist;

use ids::{
    ensure_next_pointee_id_is_high_enough, max_referenced_id, shift_ids_in_block,
    strip_groove_references,
};
use pipeline::{parse_pipeline, Mix, Stage, DEFAULT_PIPELINE};
pub use plan::plan_variations;
use plan::{unit_tops, whole_units};
use sends::{append_sends_pre, first_send_holder, pad_track_sends};
use session::{filled_rows, rebuild_slots, scenes_xml, var_color, Row};
use transpose::transpose_track;
pub use transpose::{is_pitched, key_shift, track_categories, track_semitones};
use twist::recolor;
pub use twist::{Speed, Twist, HEAT_LEVELS};

use super::xml_edit::apply_edits;

/// One source project's raw (decompressed) XML.
pub struct MashupSource {
    pub xml: String,
    /// Semitones to move each track by, indexed like `non_return_tracks`
    /// (see `track_semitones`). Empty, or short, moves nothing.
    pub semitones: Vec<i32>,
}

/// One block of scenes in the mashup. `picks[s]` holds indices into source
/// `s`'s `non_return_tracks` (the same order `categorize::list_tracks`
/// returns): the tracks that play in this block. They must cover whole
/// units: a top-level track plus every track grouped under it. A missing
/// entry picks nothing from that source. `twist` applies to source 1 (B)
/// only.
#[derive(Debug, Default, PartialEq)]
pub struct Variation {
    pub name: String,
    pub picks: Vec<Vec<usize>>,
    pub twist: Twist,
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

fn child_value<'a, 'input>(
    node: Node<'a, 'input>,
    tag: &str,
) -> Option<roxmltree::Attribute<'a, 'input>> {
    node.children()
        .find(|c| c.tag_name().name() == tag)
        .and_then(|c| c.attributes().find(|a| a.name() == "Value"))
}

/// Builds one set holding every track any variation picks, once each, A's
/// then B's. Tempo, time signature and the master track come from
/// `sources[0]`. Every source's return tracks are kept once, in source
/// order, and every track's sends are padded so each one still feeds its
/// own project's returns (see `pad_track_sends`). Returns the merged XML;
/// callers decide where (and whether) to write it.
///
/// The Session View is laid out anew by `DEFAULT_PIPELINE` (see
/// `build_mashup_with`): each variation is a coloured block of scenes, a
/// header named for it, one scene per section, then a gap. In a block's
/// scenes every picked track plays its clip for that section, reworked by
/// the variation's `twist`; tracks it doesn't pick stay silent.
/// Arrangement clips are kept as they are.
///
/// Ids are only unique within one project, so every source after the first
/// is shifted into its own id range, above the base document's ids and
/// sized to the source's highest id. Padding sends take the next free ids
/// above all of those.
pub fn build_mashup(sources: &[MashupSource], variations: &[Variation]) -> Result<String> {
    build_mashup_with(sources, variations, &parse_pipeline(DEFAULT_PIPELINE)?)
}

/// `build_mashup`, with the Session View built by `pipeline` (see
/// `pipeline.rs`) instead of the default.
pub fn build_mashup_with(
    sources: &[MashupSource],
    variations: &[Variation],
    pipeline: &[Box<dyn Stage>],
) -> Result<String> {
    let base = sources
        .first()
        .ok_or_else(|| anyhow!("no source projects given"))?;
    if variations.is_empty() {
        return Err(anyhow!("no variations given"));
    }
    let docs = parse_sources(sources)?;
    let base_tracks = tracks_node(&docs[0])?;
    let source_tracks = docs
        .iter()
        .map(non_return_tracks)
        .collect::<Result<Vec<_>>>()?;
    let picks = check_picks(variations, &source_tracks)?;

    // Pipeline: every track in A's key, then each stage in turn reworks
    // the variations' scenes.
    let blocks = source_tracks
        .iter()
        .enumerate()
        .map(|(s, tracks)| {
            tracks
                .iter()
                .enumerate()
                .map(|(idx, node)| {
                    let semitones = sources[s].semitones.get(idx).copied().unwrap_or(0);
                    transpose_track(&block_with_indent(&sources[s].xml, node), semitones)
                })
                .collect::<Result<Vec<_>>>()
        })
        .collect::<Result<Vec<_>>>()?;
    let sections = [0, 1].map(|s| source_tracks.get(s).map_or(vec![], |t| filled_rows(t)));
    let mut mix = Mix::new(variations, picks, sections, &blocks)?;
    for stage in pipeline {
        stage.run(&mut mix)?;
    }
    if mix.blocks.iter().any(|b| b.rows.is_empty()) {
        return Err(anyhow!(
            "the pipeline needs a layout stage (layering or medley) first"
        ));
    }
    let rows: Vec<Row> = mix
        .blocks
        .iter()
        .enumerate()
        .flat_map(|(var, block)| {
            std::iter::once(Row::Header(var))
                .chain(block.rows.iter().map(move |&rows| Row::Play { var, rows }))
                .chain(std::iter::once(Row::Gap))
        })
        .collect();

    // Ids: each source after the first gets its own range above A's.
    let spans: Vec<u64> = docs.iter().map(|d| max_referenced_id(d) + 1).collect();
    let mut next_id = spans[0];
    let mut home = vec![0];
    for span in &spans[1..] {
        home.push(next_id);
        next_id += span;
    }

    // Sends: every source's returns are kept, so each track is padded to
    // feed all of them.
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

    let prepare = |s: usize, block: String, offset: u64, next_id: &mut u64| -> Result<String> {
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

    // Tracks: every picked one, with its slots from the pipeline,
    // recoloured to its variation.
    let mut body = String::new();
    for (s, tracks) in blocks.iter().enumerate() {
        for (idx, block) in tracks.iter().enumerate() {
            if !mix.picks.iter().any(|p| p[s].contains(&idx)) {
                continue;
            }
            let mut values = Vec::new();
            for (var, b) in mix.blocks.iter().enumerate() {
                values.push("<Value />".to_string());
                for j in 0..b.rows.len() {
                    values.push(match b.slots.get(&(s, idx)).and_then(|v| v[j].as_deref()) {
                        Some(value) => recolor(value, var_color(var))?,
                        None => "<Value />".to_string(),
                    });
                }
                values.push("<Value />".to_string());
            }
            let block = rebuild_slots(block, &values)?;
            body.push_str(&prepare(s, block, home[s], &mut next_id)?);
        }
    }

    // Returns, in source order, and B's SendsPre flags to match.
    let mut donor_sends_pre = String::new();
    for (s, doc) in docs.iter().enumerate() {
        for node in return_tracks(doc)? {
            let block = block_with_indent(&sources[s].xml, &node);
            body.push_str(&prepare(s, block, home[s], &mut next_id)?);
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

    // Splice: the new tracks and scenes into A's XML.
    let mut edits = vec![(
        base_tracks.range(),
        format!("<Tracks>{body}\n\t\t</Tracks>"),
    )];
    edits.extend(scenes_edit(&docs[0], &base.xml, &rows, variations)?);
    let mut merged = apply_edits(&base.xml, edits);

    append_sends_pre(&mut merged, &donor_sends_pre)?;
    ensure_next_pointee_id_is_high_enough(&mut merged)?;

    // Byte-range splice, not a re-serialization -- verify it's still
    // well-formed before any caller writes it to disk.
    Document::parse(&merged).map_err(|e| {
        anyhow!("Internal error: merged XML failed to re-parse ({e}), refusing to write")
    })?;

    Ok(merged)
}

/// Parses every source, refusing a mix of Live versions.
fn parse_sources(sources: &[MashupSource]) -> Result<Vec<Document<'_>>> {
    let docs = sources
        .iter()
        .map(|s| Document::parse(&s.xml).map_err(|e| anyhow!("XML parse error: {e}")))
        .collect::<Result<Vec<_>>>()?;
    // The merged set keeps A's header, so Live reads B's devices with A's
    // schema; Live 11 and 12 write some device values differently (e.g.
    // Saturator's Oversampling as 0/1 vs false/true) and refuse the mix.
    let live_version = |doc: &Document| {
        doc.root_element()
            .attribute("MinorVersion")
            .and_then(|v| v.split('.').next())
            .unwrap_or("")
            .to_string()
    };
    if let Some(other) = docs[1..]
        .iter()
        .map(live_version)
        .find(|v| *v != live_version(&docs[0]))
    {
        return Err(anyhow!(
            "can't mash a Live {} set with a Live {other} set; save both in the same version first",
            live_version(&docs[0])
        ));
    }
    Ok(docs)
}

/// `picks[v][s]`: variation v's tracks from source s, checked to be whole
/// units and not empty.
fn check_picks(
    variations: &[Variation],
    source_tracks: &[Vec<Node>],
) -> Result<Vec<Vec<Vec<usize>>>> {
    let tops = source_tracks
        .iter()
        .map(|t| unit_tops(t))
        .collect::<Result<Vec<_>>>()?;
    variations
        .iter()
        .map(|v| {
            if v.picks.len() > source_tracks.len() {
                return Err(anyhow!(
                    "{} picks from {} sources but only {} were given",
                    v.name,
                    v.picks.len(),
                    source_tracks.len()
                ));
            }
            let picks = (0..source_tracks.len())
                .map(|s| whole_units(v.picks.get(s).map_or(&[], |p| p), &tops[s]))
                .collect::<Result<Vec<_>>>()?;
            if picks.iter().all(|p| p.is_empty()) {
                return Err(anyhow!("{} picks no tracks", v.name));
            }
            Ok(picks)
        })
        .collect()
}

/// The edit replacing A's scenes with one per row, built from A's first
/// scene. None when A has no scenes to copy.
fn scenes_edit(
    base: &Document,
    xml: &str,
    rows: &[Row],
    variations: &[Variation],
) -> Result<Option<(std::ops::Range<usize>, String)>> {
    let Some(scenes) = base
        .root_element()
        .children()
        .flat_map(|n| n.children())
        .find(|n| n.tag_name().name() == "Scenes")
    else {
        return Ok(None);
    };
    let Some(first) = scenes.children().find(|n| n.is_element()) else {
        return Ok(None);
    };
    let name = |row: &Row| match *row {
        Row::Header(var) => format!("━━ {} ━━", variations[var].name),
        Row::Play { var, rows: at } => {
            let side = |s: usize, label: &str| at[s].map(|r| format!("{label}{}", r + 1));
            let parts: Vec<String> = [side(0, "A"), side(1, "B")].into_iter().flatten().collect();
            format!("Var {} · {}", var + 1, parts.join(" × "))
        }
        Row::Gap => String::new(),
    };
    Ok(Some((
        scenes.range(),
        scenes_xml(&xml[first.range()], rows, name)?,
    )))
}
