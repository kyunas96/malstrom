// Compiled independently into each integration test binary that includes
// it, so not every item is used by every one of them.
#![allow(dead_code)]

use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Live 12.x-shaped synthetic Live Set: `MajorVersion="5"`, numeric
/// `Root`/`Name`, one MIDI clip with a C Major triad and an explicit
/// `ScaleInformation`.
pub const LIVE_12_STYLE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Ableton MajorVersion="5" MinorVersion="12.0_12203" SchemaChangeCount="3" Creator="Ableton Live 12.2.7 (synthetic fixture)" Revision="0000000000000000000000000000000000000000">
	<LiveSet>
		<Tracks>
			<MidiTrack Id="1">
				<DeviceChain>
					<MainSequencer>
						<ClipSlotList>
							<ClipSlot>
								<ClipSlot>
									<Value>
										<MidiClip Id="1" Time="0">
											<Notes>
												<KeyTracks>
													<KeyTrack Id="0">
														<Notes>
															<MidiNoteEvent Time="0" Duration="1" Velocity="100" OffVelocity="0" NoteId="0" />
														</Notes>
														<MidiKey Value="60" />
													</KeyTrack>
													<KeyTrack Id="1">
														<Notes>
															<MidiNoteEvent Time="1" Duration="1" Velocity="100" OffVelocity="0" NoteId="1" />
														</Notes>
														<MidiKey Value="64" />
													</KeyTrack>
													<KeyTrack Id="2">
														<Notes>
															<MidiNoteEvent Time="2" Duration="1" Velocity="100" OffVelocity="0" NoteId="2" />
														</Notes>
														<MidiKey Value="67" />
													</KeyTrack>
												</KeyTracks>
											</Notes>
											<IsInKey Value="true" />
											<ScaleInformation>
												<Root Value="0" />
												<Name Value="0" />
											</ScaleInformation>
										</MidiClip>
									</Value>
								</ClipSlot>
							</ClipSlot>
						</ClipSlotList>
					</MainSequencer>
				</DeviceChain>
			</MidiTrack>
		</Tracks>
	</LiveSet>
</Ableton>
"#;

/// Live 9.x-shaped synthetic Live Set: `MajorVersion="4"`, predating the
/// per-clip Scale feature, so no `ScaleInformation` node exists anywhere --
/// one MIDI clip with a D Minor/Dorian-compatible triad (D F A).
pub const LIVE_9_STYLE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Ableton MajorVersion="4" MinorVersion="9.5_327" Creator="Ableton Live 9.7.7 (synthetic fixture)" Revision="0000000000000000000000000000000000000000">
	<LiveSet>
		<Tracks>
			<MidiTrack Id="1">
				<DeviceChain>
					<MainSequencer>
						<ClipSlotList>
							<ClipSlot>
								<ClipSlot>
									<Value>
										<MidiClip Id="1" Time="0">
											<Notes>
												<KeyTracks>
													<KeyTrack Id="0">
														<Notes>
															<MidiNoteEvent Time="0" Duration="1" Velocity="100" OffVelocity="0" NoteId="0" />
														</Notes>
														<MidiKey Value="62" />
													</KeyTrack>
													<KeyTrack Id="1">
														<Notes>
															<MidiNoteEvent Time="1" Duration="1" Velocity="100" OffVelocity="0" NoteId="1" />
														</Notes>
														<MidiKey Value="65" />
													</KeyTrack>
													<KeyTrack Id="2">
														<Notes>
															<MidiNoteEvent Time="2" Duration="1" Velocity="100" OffVelocity="0" NoteId="2" />
														</Notes>
														<MidiKey Value="69" />
													</KeyTrack>
												</KeyTracks>
											</Notes>
										</MidiClip>
									</Value>
								</ClipSlot>
							</ClipSlot>
						</ClipSlotList>
					</MainSequencer>
				</DeviceChain>
			</MidiTrack>
		</Tracks>
	</LiveSet>
</Ableton>
"#;

/// Trivial Live Set with no clips at all -- used where a test only needs
/// *a* well-formed, openable `.als` file on disk and doesn't care about its
/// content (e.g. proving a nested file is excluded by directory scanning).
pub const EMPTY_XML: &str =
    r#"<?xml version="1.0" encoding="UTF-8"?><Ableton MajorVersion="5"><LiveSet /></Ableton>"#;

/// Gzip-compresses `xml` and writes it to `path` as a real `.als` file,
/// exercising the same decompression path `AlsInspector::open` uses against
/// real Ableton output. Creates any missing parent directories.
pub fn write_als_fixture(path: &Path, xml: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = GzEncoder::new(file, Compression::default());
    encoder.write_all(xml.as_bytes()).unwrap();
    encoder.finish().unwrap();
}

/// A fresh, uniquely-named temp directory for a single test to write its
/// fixtures into, so parallel tests (which all share one process id) never
/// collide. Callers should give each call site its own `label`.
pub fn temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "malstrom-test-{label}-{}",
        std::process::id()
    ));
    // Guard against a stale directory left behind by a previous run (e.g.
    // interrupted mid-test) so each run starts from a clean, predictable set
    // of files.
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
