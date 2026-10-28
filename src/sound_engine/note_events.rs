// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Notes, as the sound engine gives them to a synthesizer (phase 13): the same facts that the host crates take (`clap_host::NoteEvent`, `lv2_host::NoteEvent`), in one type of the
//! program's own, so that the plan and the engine do not name a plugin format. (Not to be confused with [`super::Notes`], which are lines for the person.)

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
    /// A controller (the sustain pedal is 64, the modulation wheel 1).
    Controller { channel: u8, number: u8, value: u8 },
    /// The pitch wheel, `0..=16383` with 8192 in the middle.
    PitchBend { channel: u8, value: u16 },
    /// Every voice is silenced at once (a choke, not a release).
    AllOff,
}

impl NoteEvent {
    /// The same event `by` samples earlier: for the notes of a piece of a block, which are counted from the start of the piece. (`None` if it is before the piece.)
    pub fn earlier_by(self, by: usize) -> Option<Self> {
        let time = (self.time as usize).checked_sub(by)?;
        Some(Self {
            time: u32::try_from(time).unwrap_or(u32::MAX),
            what: self.what,
        })
    }
}

impl From<NoteEvent> for clap_host::NoteEvent {
    fn from(note: NoteEvent) -> Self {
        Self {
            time: note.time,
            what: match note.what {
                NoteEventKind::On {
                    channel,
                    key,
                    velocity,
                    id,
                } => clap_host::NoteEventKind::On {
                    channel,
                    key,
                    velocity,
                    id,
                },
                NoteEventKind::Off {
                    channel,
                    key,
                    velocity,
                    id,
                } => clap_host::NoteEventKind::Off {
                    channel,
                    key,
                    velocity,
                    id,
                },
                NoteEventKind::Controller { channel, number, value } => {
                    clap_host::NoteEventKind::Controller { channel, number, value }
                }
                NoteEventKind::PitchBend { channel, value } => clap_host::NoteEventKind::PitchBend { channel, value },
                NoteEventKind::AllOff => clap_host::NoteEventKind::AllOff,
            },
        }
    }
}

impl From<NoteEvent> for lv2_host::NoteEvent {
    fn from(note: NoteEvent) -> Self {
        Self {
            time: note.time,
            what: match note.what {
                NoteEventKind::On {
                    channel, key, velocity, ..
                } => lv2_host::NoteEventKind::On { channel, key, velocity },
                NoteEventKind::Off {
                    channel, key, velocity, ..
                } => lv2_host::NoteEventKind::Off { channel, key, velocity },
                NoteEventKind::Controller { channel, number, value } => {
                    lv2_host::NoteEventKind::Controller { channel, number, value }
                }
                NoteEventKind::PitchBend { channel, value } => lv2_host::NoteEventKind::PitchBend { channel, value },
                NoteEventKind::AllOff => lv2_host::NoteEventKind::AllOff,
            },
        }
    }
}
