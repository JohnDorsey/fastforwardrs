// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The notes in the sound engine (phase 13, piece 13.4b; docs/engine-phase-13-midi-design.md, 13.4): the events that a synthesizer is given, in samples counted from the start of the sound, and
//! how a chain of effects is given them for a block (the first of its effects that plays notes gets them, at the time that its place in the chain, and the latency of what is before it, make right).

use super::insertions::BlockProcessor;
use super::note_events::{NoteEvent, NoteEventKind};
use crate::composition::Music;

/// An event at a sample counted from the start of the sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimedNote {
    pub time: u64,
    pub what: NoteEventKind,
}

/// Which comes first among events at one sample: the note offs, then the controllers, then the note ons (a note off and the note on of the same key at one sample: the off first, so that the
/// key is let go and pressed again, and a pedal that is let up with the note off is up before the next note).
fn rank_of(what: &NoteEventKind) -> u8 {
    match what {
        NoteEventKind::Off { .. } => 0,
        NoteEventKind::Controller { .. } | NoteEventKind::PitchBend { .. } | NoteEventKind::AllOff => 1,
        NoteEventKind::On { .. } => 2,
    }
}

/// The velocity of a note off: the middle (a synthesizer that takes it as a number takes this one).
const VELOCITY_OF_A_NOTE_OFF: f32 = 64.0 / 127.0;

/// The events of the music of a clip whose item begins at the sample `item_start` and lasts `item_length` samples, with `next_id` the number that the first note is given (the numbers of the
/// notes of one consumer are all different: a note off says which note it ends by it). **A note that begins at or after the end of the item is left out, and one that ends after it ends at it.**
pub fn events_of(music: &Music, item_start: u64, item_length: u64, next_id: &mut u32) -> Vec<TimedNote> {
    let mut events = Vec::with_capacity(music.notes.len() * 2 + music.controls.len());
    for note in &music.notes {
        if note.start >= item_length {
            continue;
        }
        let id = *next_id;
        *next_id += 1;
        let end = note.end.min(item_length).max(note.start + 1);
        events.push(TimedNote {
            time: item_start + note.start,
            what: NoteEventKind::On {
                channel: note.channel,
                key: note.key,
                velocity: f32::from(note.velocity) / 127.0,
                id,
            },
        });
        events.push(TimedNote {
            time: item_start + end,
            what: NoteEventKind::Off {
                channel: note.channel,
                key: note.key,
                velocity: VELOCITY_OF_A_NOTE_OFF,
                id,
            },
        });
    }
    for control in &music.controls {
        let time = item_start + control.at.min(item_length);
        events.push(TimedNote {
            time,
            what: match control.what {
                crate::midi_file::MidiControlKind::Controller { number, value } => NoteEventKind::Controller {
                    channel: control.channel,
                    number,
                    value,
                },
                crate::midi_file::MidiControlKind::PitchBend { value } => NoteEventKind::PitchBend {
                    channel: control.channel,
                    value,
                },
            },
        });
    }
    put_in_order(&mut events);
    events
}

/// Puts events in order of time, and those at one sample in the order of [`rank_of`] (and the others as they were).
pub fn put_in_order(events: &mut [TimedNote]) {
    events.sort_by_key(|event| (event.time, rank_of(&event.what)));
}

/// Gives the first of `processors` that plays notes the events that are in the block it is about to process, which is `length` samples long and whose **input** is the sound `audio_start`
/// samples from the start of the sound (a chain whose effects delay the sound is given its sound ahead: see `BlockProcessor::latency`). The synthesizer is after the processors that are before it
/// in the chain, which delay what goes through them, so that **its block is the one that begins `L_before` samples earlier in the sound**, `L_before` being the sum of their latencies: that is
/// where the notes are taken from (the sum of the latencies of it and the processors after it is how late what it makes comes out, which makes up for the rest). Each event is given at its
/// sample of the block. A chain with no processor that plays notes is given nothing, and **no processor after the first that plays notes is given any**.
pub fn give_the_notes_of_the_block(
    processors: &mut [Box<dyn BlockProcessor>],
    events: &[TimedNote],
    audio_start: u64,
    length: usize,
) {
    let Some(place) = processors.iter().position(|processor| processor.plays_notes()) else {
        return;
    };
    let before: i64 = processors[..place]
        .iter()
        .map(|processor| i64::try_from(processor.latency()).unwrap_or(i64::MAX))
        .sum();
    let start = i64::try_from(audio_start).unwrap_or(i64::MAX).saturating_sub(before);
    let end = start.saturating_add(i64::try_from(length).unwrap_or(i64::MAX));
    let first = events.partition_point(|event| i64::try_from(event.time).is_ok_and(|time| time < start));
    let batch: Vec<NoteEvent> = events[first..]
        .iter()
        .take_while(|event| i64::try_from(event.time).is_ok_and(|time| time < end))
        .map(|event| NoteEvent {
            time: u32::try_from(i64::try_from(event.time).unwrap_or(i64::MAX) - start).unwrap_or(u32::MAX),
            what: event.what,
        })
        .collect();
    processors[place].give_notes(&batch);
}

/// What takes the place of a synthesizer that is not installed, or that could not be run: it plays no notes and makes no sound, and **takes the notes that reach it**, so that they are not
/// played by the next synthesizer in the chain (the plan decided that this chain has the first). The sound that goes through it is as it is.
pub struct TakesTheNotes;

impl BlockProcessor for TakesTheNotes {
    fn process(&mut self, _left: &mut [f32], _right: &mut [f32]) {}

    fn reset(&mut self) {}

    fn plays_notes(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests;
