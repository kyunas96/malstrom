pub const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Semitone offsets from the root for each entry in SCALE_NAMES, in the same order.
pub const SCALE_INTERVALS: [&[u8]; 35] = [
    &[0, 2, 4, 5, 7, 9, 11],     // Major
    &[0, 2, 3, 5, 7, 8, 10],     // Minor
    &[0, 2, 3, 5, 7, 9, 10],     // Dorian
    &[0, 2, 4, 5, 7, 9, 10],     // Mixolydian
    &[0, 2, 4, 6, 7, 9, 11],     // Lydian
    &[0, 1, 3, 5, 7, 8, 10],     // Phrygian
    &[0, 1, 3, 5, 6, 8, 10],     // Locrian
    &[0, 2, 4, 6, 8, 10],        // Whole Tone
    &[0, 1, 3, 4, 6, 7, 9, 10],  // Half-whole Dim.
    &[0, 2, 3, 5, 6, 8, 9, 11],  // Whole-half Dim.
    &[0, 3, 5, 6, 7, 10],        // Minor Blues
    &[0, 3, 5, 7, 10],           // Minor Pentatonic
    &[0, 2, 4, 7, 9],            // Major Pentatonic
    &[0, 2, 3, 5, 7, 8, 11],     // Harmonic Minor
    &[0, 2, 4, 5, 7, 8, 11],     // Harmonic Major
    &[0, 2, 3, 6, 7, 9, 11],     // Dorian #4
    &[0, 1, 4, 5, 7, 8, 10],     // Phrygian Dominant
    &[0, 2, 3, 5, 7, 9, 11],     // Melodic Minor
    &[0, 2, 4, 6, 8, 9, 11],     // Lydian Augmented
    &[0, 2, 4, 6, 7, 9, 10],     // Lydian Dominant
    &[0, 1, 3, 4, 6, 8, 10],     // Super Locrian
    &[0, 1, 4, 5, 7, 8, 10, 11], // 8-Tone Spanish
    &[0, 1, 4, 5, 7, 8, 11],     // Bhairav
    &[0, 2, 3, 6, 7, 8, 11],     // Hungarian Minor
    &[0, 2, 3, 7, 8],            // Hirajoshi
    &[0, 1, 5, 7, 10],           // In-Sen
    &[0, 1, 5, 6, 10],           // Iwato
    &[0, 2, 3, 7, 9],            // Kumoi
    &[0, 1, 3, 7, 8],            // Pelog Selisir
    &[0, 1, 5, 7, 8],            // Pelog Tembung
    &[0, 2, 3, 4, 6, 7, 8, 10, 11], // Messiaen 3
    &[0, 1, 2, 5, 6, 7, 8, 11],  // Messiaen 4
    &[0, 1, 5, 6, 7, 11],        // Messiaen 5
    &[0, 2, 4, 5, 6, 8, 10, 11], // Messiaen 6
    &[0, 1, 2, 3, 5, 6, 7, 8, 9, 11], // Messiaen 7
];

pub const SCALE_NAMES: [&str; 35] = [
    "Major",
    "Minor",
    "Dorian",
    "Mixolydian",
    "Lydian",
    "Phrygian",
    "Locrian",
    "Whole Tone",
    "Half-whole Dim.",
    "Whole-half Dim.",
    "Minor Blues",
    "Minor Pentatonic",
    "Major Pentatonic",
    "Harmonic Minor",
    "Harmonic Major",
    "Dorian #4",
    "Phrygian Dominant",
    "Melodic Minor",
    "Lydian Augmented",
    "Lydian Dominant",
    "Super Locrian",
    "8-Tone Spanish",
    "Bhairav",
    "Hungarian Minor",
    "Hirajoshi",
    "In-Sen",
    "Iwato",
    "Kumoi",
    "Pelog Selisir",
    "Pelog Tembung",
    "Messiaen 3",
    "Messiaen 4",
    "Messiaen 5",
    "Messiaen 6",
    "Messiaen 7",
];
