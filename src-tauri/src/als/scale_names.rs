use anyhow::{anyhow, Result};
use roxmltree::Node;

use super::scale_constants::{NOTE_NAMES, SCALE_NAMES};

/// Reads the `Value` attribute of a self-closing child element, e.g. `<RootNote Value="2" />`.
pub(super) fn child_value<'a>(parent: &Node<'a, 'a>, tag: &str) -> Option<&'a str> {
    parent
        .children()
        .find(|c| c.is_element() && c.tag_name().name() == tag)
        .and_then(|c| c.attribute("Value"))
}

fn name_at_index(names: &[&str], index: i32, label: &str) -> Result<String> {
    names
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .map(|n| n.to_string())
        .ok_or_else(|| {
            anyhow!(
                "Invalid {} value {} (expected 0-{})",
                label,
                index,
                names.len() - 1
            )
        })
}

pub(super) fn root_note_name(root_note: i32) -> Result<String> {
    name_at_index(&NOTE_NAMES, root_note, "RootNote")
}

pub(super) fn validate_scale_name(scale_index: i32) -> Result<String> {
    name_at_index(&SCALE_NAMES, scale_index, "Scale")
}
