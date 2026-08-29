# als

Handles reading Ableton Live Set (`.als`) files and extracting scale
information from their XML contents.

The top-level module (`als.rs`) exposes the public API: `AlsInspector`.
It opens a `.als` file (`open`) and caches its decompressed XML, or wraps an
already-decompressed XML string directly (`from_xml`, mainly useful for
tests working against synthetic XML). Either way, `extract_scale_candidates`
can be called against it without re-parsing the document more than once per
call.

## Submodules

### inspector

Defines `AlsInspector`, described above.

### scale_constants

Static lookup tables used to translate the numeric indices found in the
Live Set XML into human-readable names: `NOTE_NAMES` (the 12 chromatic
pitches) and `SCALE_NAMES` (the 35 scale types Live supports). The first
15 entries of `SCALE_NAMES` are the common Western scales/modes; the rest
are less common "exotic" scales.

### scale_names

Shared helpers for resolving numeric root/scale indices against
`scale_constants` into human-readable names (`root_note_name`,
`validate_scale_name`), plus `child_value` for reading a self-closing XML
child's `Value` attribute. Used by both `scale_candidates` and
`scale_info_schema`/`scale_apply`.

### scale_candidates

Infers compatible scales from actual note content rather than explicit
assignment. `extract_from_document` scans the parsed document for every
`MidiClip` element, reads the `MidiKey` value of each of its `KeyTrack`s to
build the set of pitch classes used in that clip, and checks each
(root, scale) pair from `scale_constants` for exact compatibility (every
pitch class in the clip must belong to that scale). Each compatible scale is
scored by summing the number of `MidiNoteEvent`s across every clip it
matches, so scales that fit note-dense clips outrank ones that only fit
sparse clips. The `ScaleCandidates` struct returned to callers holds these
`ScaleCandidate { root_name, scale_name, common, score, clip_count }`
entries sorted by descending score, with the first entry being the scale
most compatible with the project as a whole. Partial/near-matches (off by
one or two notes) are explicitly out of scope for now.
