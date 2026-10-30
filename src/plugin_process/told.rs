// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a program has been told, so that a program that is started in the place of one that ended (phase 14, the restart) can be told it again and play on where the other left off: the last
//! value of each parameter, and what the notes have made of the keys (which are held, where the pedal and the wheel are).
//!
//! It is what an effect has told the plugin, as the plugin would remember it: **a plugin that is reset forgets the notes** (the values it was given are kept, as the copies in the editor's
//! process keep them), and a note that is let go of is not held. What it cannot know is what the plugin does with them (a sustain pedal that keeps a note sounding after its note off: the
//! restarted plugin has the pedal down and not the note).

use std::collections::BTreeMap;

use super::protocol::Command;
use crate::sound_engine::{NoteEvent, NoteEventKind};

/// The most notes that are kept as held; the oldest is let go of past it. (A stream of notes on that never has a note off must not grow for ever.)
pub const MOST_NOTES_HELD: usize = 1024;

/// The controller numbers that let go of every note of a channel (MIDI: all sound off, all notes off).
const ALL_SOUND_OFF: u8 = 120;
const ALL_NOTES_OFF: u8 = 123;

#[derive(Clone, Copy, Debug, PartialEq)]
struct HeldNote {
    channel: u8,
    key: u8,
    velocity: f32,
    id: u32,
}

/// What the program has been told that it would keep.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WhatTheProgramWasTold {
    /// The last value of each parameter (or port) that was set.
    values: BTreeMap<u32, f32>,
    /// The notes that are held, in the order they began.
    notes: Vec<HeldNote>,
    /// Where each controller of each channel was last put.
    controllers: BTreeMap<(u8, u8), u8>,
    /// Where the pitch wheel of each channel was last put.
    wheels: BTreeMap<u8, u16>,
}

impl WhatTheProgramWasTold {
    /// Takes in the commands that are given before a run, in order.
    pub fn after_commands(&mut self, commands: &[Command]) {
        for command in commands {
            match *command {
                Command::SetValue { handle, value, .. } => {
                    self.values.insert(handle, value);
                }
                Command::Reset => {
                    self.notes.clear();
                    self.controllers.clear();
                    self.wheels.clear();
                }
            }
        }
    }

    /// Takes in the notes of a run, in order of time.
    pub fn after_notes(&mut self, notes: &[NoteEvent]) {
        for note in notes {
            match note.what {
                NoteEventKind::On {
                    channel,
                    key,
                    velocity,
                    id,
                } => {
                    self.notes
                        .retain(|held| !(held.channel == channel && held.key == key && held.id == id));
                    if self.notes.len() >= MOST_NOTES_HELD {
                        self.notes.remove(0);
                    }
                    self.notes.push(HeldNote {
                        channel,
                        key,
                        velocity,
                        id,
                    });
                }
                NoteEventKind::Off { channel, key, id, .. } => {
                    // (The note with that id; a plugin that does not tell notes apart by id has given every note the id of its key, and the first of the key goes if none has this id.)
                    let place = self
                        .notes
                        .iter()
                        .position(|held| held.channel == channel && held.key == key && held.id == id)
                        .or_else(|| {
                            self.notes
                                .iter()
                                .position(|held| held.channel == channel && held.key == key)
                        });
                    if let Some(place) = place {
                        self.notes.remove(place);
                    }
                }
                NoteEventKind::AllOff => self.notes.clear(),
                NoteEventKind::Controller { channel, number, value } => {
                    if number == ALL_SOUND_OFF || number == ALL_NOTES_OFF {
                        self.notes.retain(|held| held.channel != channel);
                    } else {
                        self.controllers.insert((channel, number), value);
                    }
                }
                NoteEventKind::PitchBend { channel, value } => {
                    self.wheels.insert(channel, value);
                }
            }
        }
    }

    /// The values to set again in a program that is started in the place of the one that ended: each at the first sample of the run.
    pub fn values_to_set_again(&self) -> Vec<Command> {
        self.values
            .iter()
            .map(|(&handle, &value)| Command::SetValue {
                handle,
                value,
                at_sample: 0,
            })
            .collect()
    }

    /// The events to give again, at the first sample of the run: the controllers and the wheels where they were put, then the notes that are held, so that a program that is started in the place
    /// of the one that ended plays them. (A note begins again from its start: the sound of it is not where it was.)
    pub fn events_to_give_again(&self) -> Vec<NoteEvent> {
        let controllers = self.controllers.iter().map(|(&(channel, number), &value)| NoteEvent {
            time: 0,
            what: NoteEventKind::Controller { channel, number, value },
        });
        let wheels = self.wheels.iter().map(|(&channel, &value)| NoteEvent {
            time: 0,
            what: NoteEventKind::PitchBend { channel, value },
        });
        let notes = self.notes.iter().map(|held| NoteEvent {
            time: 0,
            what: NoteEventKind::On {
                channel: held.channel,
                key: held.key,
                velocity: held.velocity,
                id: held.id,
            },
        });
        controllers.chain(wheels).chain(notes).collect()
    }
}

#[cfg(test)]
mod tests;
