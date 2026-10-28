// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Notes for an instrument (phase 13, piece 13.1): what a host gives a plugin's MIDI input, and how it is written into the atom sequence that the port holds.
//!
//! An LV2 atom port that "supports `midi:MidiEvent`" is a buffer that holds an `atom:Sequence`: a header (the size of what follows, the type of the sequence, the unit of the time stamps
//! and four bytes of padding: 16 bytes in all) and then the events, each of them a time stamp (here a `i64` of frames, since the unit is 0), an atom header (the size of the body and its
//! type) and the body, which is padded so that the next event begins at a multiple of 8 bytes. A MIDI event's body is its 3 bytes.

/// A note, or what goes with notes, at the frame `time` of the block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteEvent {
    pub time: u32,
    pub what: NoteEventKind,
}

/// What a [`NoteEvent`] says. Channels are `0..=15`, keys and controller numbers `0..=127`, velocities `0.0..=1.0`. (MIDI has no ids for notes: a note off ends the note of its channel and key.)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoteEventKind {
    /// A key is pressed.
    On { channel: u8, key: u8, velocity: f32 },
    /// A key is let go.
    Off { channel: u8, key: u8, velocity: f32 },
    /// A controller (the sustain pedal is 64, the modulation wheel 1).
    Controller { channel: u8, number: u8, value: u8 },
    /// The pitch wheel, `0..=16383` with 8192 in the middle.
    PitchBend { channel: u8, value: u16 },
    /// Every voice is silenced at once: "all sound off" (controller 120) on every channel.
    AllOff,
}

/// The bytes of the MIDI events that a note is, in order (one, except for [`NoteEventKind::AllOff`], which is one for each channel).
pub(crate) fn midi_bytes_of(what: NoteEventKind) -> Vec<[u8; 3]> {
    // (A velocity of a note on is never 0 in MIDI, where that means a note off.)
    let seven_bits = |velocity: f32, least: u8| ((velocity.clamp(0.0, 1.0) * 127.0).round() as u8).max(least);
    match what {
        NoteEventKind::On { channel, key, velocity } => {
            vec![[0x90 | (channel & 15), key & 127, seven_bits(velocity, 1)]]
        }
        NoteEventKind::Off { channel, key, velocity } => {
            vec![[0x80 | (channel & 15), key & 127, seven_bits(velocity, 0)]]
        }
        NoteEventKind::Controller { channel, number, value } => {
            vec![[0xB0 | (channel & 15), number & 127, value & 127]]
        }
        NoteEventKind::PitchBend { channel, value } => {
            let value = value.min(16383);
            vec![[0xE0 | (channel & 15), (value & 127) as u8, (value >> 7) as u8]]
        }
        NoteEventKind::AllOff => (0..16u8).map(|channel| [0xB0 | channel, 120, 0]).collect(),
    }
}

/// The bytes of an `atom:Sequence` (in the byte order of this machine, as a plugin that is in the same process reads it) that holds `notes`, in order of time, for the block's frames: at most
/// `room` bytes of it, **the events that do not fit are left out**. `sequence` and `midi_event` are the numbers that the host's map gave the URIs of the sequence and of the MIDI event.
pub(crate) fn sequence_of(notes: &[NoteEvent], sequence: u32, midi_event: u32, room: usize) -> Vec<u8> {
    let mut bytes = vec![0_u8; 16];
    'notes: for note in notes {
        for midi in midi_bytes_of(note.what) {
            // (A time stamp of 8 bytes, the atom's 8, and the body's 3 made 8.)
            if bytes.len() + 24 > room {
                break 'notes;
            }
            bytes.extend_from_slice(&i64::from(note.time).to_ne_bytes());
            bytes.extend_from_slice(&3_u32.to_ne_bytes());
            bytes.extend_from_slice(&midi_event.to_ne_bytes());
            bytes.extend_from_slice(&midi);
            bytes.extend_from_slice(&[0; 5]);
        }
    }
    let size = u32::try_from(bytes.len() - 8).unwrap_or(u32::MAX);
    bytes[0..4].copy_from_slice(&size.to_ne_bytes());
    bytes[4..8].copy_from_slice(&sequence.to_ne_bytes());
    // (The unit is 0, which is frames, and the padding is 0.)
    bytes
}
