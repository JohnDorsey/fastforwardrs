// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Notes for an instrument: what a host gives a plugin's event input. The same facts as the notes of the other host crates (`clap_host::NoteEvent`, `lv2_host::NoteEvent`).

/// A note, or what goes with notes, at the sample `time` of the block it is given with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteEvent {
    pub time: u32,
    pub what: NoteEventKind,
}

/// What a [`NoteEvent`] says. Channels are `0..=15`, keys and controller numbers `0..=127`, velocities `0.0..=1.0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoteEventKind {
    /// A key is pressed. `id` tells this note from another on the same key and channel (`0..i32::MAX`), and is given again by its [`Self::Off`].
    On {
        channel: u8,
        key: u8,
        velocity: f32,
        id: u32,
    },
    /// A key is let go.
    Off {
        channel: u8,
        key: u8,
        velocity: f32,
        id: u32,
    },
    /// A controller (the sustain pedal is 64, the modulation wheel 1). VST3 has no event for it: it reaches a plugin that maps controllers to parameters (`IMidiMapping`).
    Controller { channel: u8, number: u8, value: u8 },
    /// The pitch wheel, `0..=16383` with 8192 in the middle. Reaches a plugin as a controller too (number 129).
    PitchBend { channel: u8, value: u16 },
    /// Every voice is silenced at once. Reaches a plugin as controller 123 (all notes off) if it maps it; else the plugin is made to forget its voices by being stopped and started.
    AllOff,
}

/// The controller number that VST3 gives to the pitch wheel (`kPitchBend`), and to "all notes off" (`kCtrlAllNotesOff`).
pub(crate) const CONTROLLER_PITCH_BEND: i16 = 129;
pub(crate) const CONTROLLER_ALL_NOTES_OFF: i16 = 123;
