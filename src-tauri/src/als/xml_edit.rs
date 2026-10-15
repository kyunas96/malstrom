use std::ops::Range;

/// Replaces the text at each `(range, new_text)` in one front-to-back pass.
/// Ranges must not overlap; an empty range inserts at that point, and
/// inserts at the same point keep their order.
pub(super) fn apply_edits(xml: &str, mut edits: Vec<(Range<usize>, String)>) -> String {
    edits.sort_by_key(|(range, _)| range.start);
    let mut out = String::with_capacity(xml.len());
    let mut cursor = 0;
    for (range, new_text) in edits {
        out.push_str(&xml[cursor..range.start]);
        out.push_str(&new_text);
        cursor = range.end;
    }
    out.push_str(&xml[cursor..]);
    out
}
