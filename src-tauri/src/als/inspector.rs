use anyhow::{anyhow, Result};
use flate2::read::GzDecoder;
use roxmltree::Document;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use super::{categorize, scale_apply, scale_candidates};
use categorize::TrackSummary;
use scale_apply::ApplyScaleOutcome;
use scale_candidates::ScaleCandidates;

/// Holds a decompressed Live Set XML document so multiple extractions can be
/// run against it without re-reading or re-decompressing the .als file.
pub struct AlsInspector {
    xml: String,
}

impl AlsInspector {
    /// Loads and decompresses a .als file for inspection.
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let mut decoder = GzDecoder::new(file);
        let mut xml = String::new();
        decoder.read_to_string(&mut xml)?;
        Ok(Self { xml })
    }

    /// Wraps an already-decompressed Live Set XML string for inspection.
    pub fn from_xml(xml: String) -> Self {
        Self { xml }
    }

    fn parse(&self) -> Result<Document<'_>> {
        Document::parse(&self.xml).map_err(|e| anyhow!("XML parse error: {}", e))
    }

    /// Extracts which scales are exactly compatible with the pitch classes
    /// used across every MIDI clip's notes in the file.
    pub fn extract_scale_candidates(&self) -> Result<ScaleCandidates> {
        let doc = self.parse()?;
        scale_candidates::extract_from_document(&doc)
    }

    /// Applies a (root, scale) pair to every eligible clip and returns the
    /// mutated XML plus a summary. Does not write to disk -- callers decide
    /// the output path.
    pub fn apply_scale(
        &self,
        root_note: i32,
        scale_index: i32,
    ) -> Result<(String, ApplyScaleOutcome)> {
        scale_apply::apply_scale_to_xml(&self.xml, root_note, scale_index)
    }

    /// Lists every track with a derived category. `live_db_path`, when
    /// given, is tried first via the Live Database; every track without a
    /// resolved DB tag (or when no path is given) falls back to
    /// name-keyword matching.
    pub fn extract_tracks(&self, live_db_path: Option<&Path>) -> Result<Vec<TrackSummary>> {
        let doc = self.parse()?;
        categorize::list_tracks(&doc, live_db_path)
    }
}
