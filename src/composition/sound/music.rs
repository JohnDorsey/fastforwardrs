// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The notes of a MIDI clip as the sound plan has them (phase 13, piece 13.4a; the design is `docs/engine-phase-13-midi-design.md`, 13.4): **where each note begins and ends in samples counted
//! from the start of a stretch of the clip**, which of them are played at all, which are begun where the stretch begins (chased), and what the pedal and the wheel are left as.
//!
//! Everything here is whole numbers (`u128` where they are large) and is rounded down once: the same file gives the same samples on every machine.

use std::collections::BTreeMap;

use crate::midi_file::{MidiControlKind, MidiTrack};
use crate::project::FrameRate;

/// A note, in samples counted from the start of the stretch it is in: it begins at `start` and its note off is at `end` (which is at least a sample after `start`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlannedNote {
    pub start: u64,
    pub end: u64,
    /// `0..=15`.
    pub channel: u8,
    /// `0..=127`.
    pub key: u8,
    /// `1..=127`, as the file says.
    pub velocity: u8,
}

/// A change of a controller or of the pitch wheel, in samples counted from the start of the stretch it is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlannedControl {
    pub at: u64,
    pub channel: u8,
    pub what: MidiControlKind,
}

/// What plays the notes of a MIDI clip: the first synthesizer of a chain of its road (see `docs/engine-phase-13-midi-design.md`, 13.4; and 13.5 for the notes that go on to the next chain).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteConsumer {
    /// The effects of the clip itself.
    Clip,
    /// The instance of the effects of its track that hears the clip.
    Bus,
    /// The effects of a group that the clip is sent to, or that holds one that it is sent to (the place in `SoundPlan::groups`).
    Group(usize),
}

/// What a MIDI clip says to the synthesizer that plays it, over a stretch of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Music {
    /// In the order of their starts.
    pub notes: Vec<PlannedNote>,
    /// In the order of their times.
    pub controls: Vec<PlannedControl>,
    /// The chains whose synthesizers play the notes, in the order of the road. **Empty: no synthesizer is on the road, and nothing plays them.**
    pub consumers: Vec<NoteConsumer>,
}

/// Which part of a track of a MIDI file a stretch of a clip plays, and how.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StretchOfMusic {
    pub frame_rate: FrameRate,
    /// The source frame where the clip itself begins: a note that begins before it is not played, though the stretch may begin later (a mute cut the clip, or the plan began later).
    pub clip_first_source_frame: u64,
    /// The source frame where the stretch begins, and the one just after the last of it.
    pub first_source_frame: u64,
    pub end_source_frame: u64,
    /// How many source frames pass in a frame of the timeline: the speed of the clip times that of its track.
    pub speed: u32,
}

/// The pedal is the controller 64; down is from 64.
const PEDAL: u8 = 64;
const PEDAL_IS_DOWN_FROM: u8 = 64;
const WHEEL_IN_THE_MIDDLE: u16 = 8192;

/// Which notes and controllers of `track` a stretch plays, in samples counted from its start (see the top of this file, and 13.4 of the design, for the rules).
pub fn music_of_a_stretch(track: &MidiTrack, stretch: StretchOfMusic) -> (Vec<PlannedNote>, Vec<PlannedControl>) {
    let (numerator, denominator) = (
        u128::from(stretch.frame_rate.numerator()),
        u128::from(stretch.frame_rate.denominator()),
    );
    let samples_a_second = u128::from(super::SOUND_SAMPLES_A_SECOND);
    let speed = u128::from(stretch.speed.max(1));
    // A time in microseconds, times the frames a second, is compared with a frame times the seconds to a frame (in microseconds) times the frames a second, which is this.
    let boundary_of = |frame: u64| u128::from(frame) * denominator * 1_000_000;
    let (clip_start, start, end) = (
        boundary_of(stretch.clip_first_source_frame),
        boundary_of(stretch.first_source_frame),
        boundary_of(stretch.end_source_frame),
    );
    let samples_after_the_start = |time_times_the_rate: u128| -> u64 {
        u64::try_from((time_times_the_rate - start) * samples_a_second / (numerator * 1_000_000 * speed))
            .unwrap_or(u64::MAX)
    };
    let limit = u64::try_from(
        u128::from(stretch.end_source_frame - stretch.first_source_frame) * denominator * samples_a_second
            / (numerator * speed),
    )
    .unwrap_or(u64::MAX);

    let mut notes = Vec::new();
    for note in &track.notes {
        let (note_start, note_end) = (
            u128::from(note.start_us) * numerator,
            u128::from(note.end_us) * numerator,
        );
        if note_start < clip_start || note_start >= end {
            continue;
        }
        let begins_at = if note_start < start {
            // (It began before the stretch: it is played from the start of it if it is still sounding there.)
            if note_end <= start {
                continue;
            }
            0
        } else {
            samples_after_the_start(note_start)
        };
        let ends_at = if note_end >= end {
            limit
        } else {
            samples_after_the_start(note_end)
        };
        notes.push(PlannedNote {
            start: begins_at,
            // (A note of no length is a sample long, so that its note off is never before its note on.)
            end: ends_at.min(limit).max(begins_at + 1),
            channel: note.channel,
            key: note.key,
            velocity: note.velocity,
        });
    }

    // The controllers: the last of each before the stretch (in the clip) is given where the stretch begins, then those in it; and at the end the pedal and the wheel are put back.
    let mut before_the_stretch: BTreeMap<(u8, u8), MidiControlKind> = BTreeMap::new();
    let mut in_the_stretch: Vec<PlannedControl> = Vec::new();
    for control in &track.controls {
        let time = u128::from(control.time_us) * numerator;
        if time < clip_start || time >= end {
            continue;
        }
        if time < start {
            before_the_stretch.insert((control.channel, which_it_is(control.what)), control.what);
        } else {
            in_the_stretch.push(PlannedControl {
                at: samples_after_the_start(time),
                channel: control.channel,
                what: control.what,
            });
        }
    }
    let mut controls: Vec<PlannedControl> = before_the_stretch
        .into_iter()
        .map(|((channel, _), what)| PlannedControl { at: 0, channel, what })
        .collect();
    controls.extend(in_the_stretch);
    controls.sort_by_key(|control| control.at);
    // What the pedal and the wheel are at the end of each channel's music: let up and in the middle.
    let mut pedal_down = [false; 16];
    let mut wheel_bent = [false; 16];
    for control in &controls {
        let channel = usize::from(control.channel & 15);
        match control.what {
            MidiControlKind::Controller { number, value } if number == PEDAL => {
                pedal_down[channel] = value >= PEDAL_IS_DOWN_FROM;
            }
            MidiControlKind::PitchBend { value } => wheel_bent[channel] = value != WHEEL_IN_THE_MIDDLE,
            MidiControlKind::Controller { .. } => {}
        }
    }
    for channel in 0..16_u8 {
        if pedal_down[usize::from(channel)] {
            controls.push(PlannedControl {
                at: limit,
                channel,
                what: MidiControlKind::Controller {
                    number: PEDAL,
                    value: 0,
                },
            });
        }
        if wheel_bent[usize::from(channel)] {
            controls.push(PlannedControl {
                at: limit,
                channel,
                what: MidiControlKind::PitchBend {
                    value: WHEEL_IN_THE_MIDDLE,
                },
            });
        }
    }
    (notes, controls)
}

/// A number that tells the controllers and the wheel apart, for what is kept of each.
fn which_it_is(what: MidiControlKind) -> u8 {
    match what {
        MidiControlKind::Controller { number, .. } => number,
        // (The controllers are 0 to 127.)
        MidiControlKind::PitchBend { .. } => 128,
    }
}

#[cfg(test)]
mod tests;
