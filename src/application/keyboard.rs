// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the keys of the keyboard do. They are shortcuts of the whole window, not of any one part
//! of it:
//!
//! * **Space** plays the video, or pauses it while it is playing.
//! * **S** cuts the selected clips that are at the current frame, there, and **Shift+S** cuts every clip that is at the current
//!   frame, on every track, whatever is selected.
//! * **Delete** takes the selected clips off their tracks, the selected keyframes and racks away, and the selected groups apart (what
//!   was in a group stays).
//! * **Escape** deselects.
//! * **Ctrl+Z** undoes the latest edit, and **Ctrl+Y** (or **Ctrl+Shift+Z**) does it again.
//! * **Left** and **Right** move the current frame back or forward by one frame, while paused or
//!   while playing. Holding the key repeats.
//! * **Ctrl+Left** and **Ctrl+Right** move the current frame to the nearest place before or after it where a clip begins or ends:
//!   of the selected clips if some are selected, and otherwise of any clip on the timeline.
//! * **F** makes the picture of the preview fit its panel while the pointer is over the preview, like the Fit button; anywhere
//!   else it scrolls and zooms the timeline so that what is selected (clips, keyframes, racks of keyframes) is in the middle of
//!   it and nearly fills its width.
//!
//! None of them does anything while something that wants the keys itself is open: a text field
//! being typed in (the title of a track, the words of a text object), or a menu or a drop-down list (Escape
//! closes those).
//!
//! Every key is one row of [`BINDINGS`]: which key, with which modifiers held, with the pointer where, and what it asks for.
//! A key press is used up by the first row it matches, and a row matches a press that has *more* modifiers held than it asks
//! for (Shift and Alt are only looked at when a row asks for them), so a row for a key with more modifiers has to come before
//! one for the same key with fewer: a test checks the table for that.

use egui::{Context, Key, KeyboardShortcut, Modifiers, Popup};

use super::commands::Command;
use crate::project::{ClipId, GroupId, KeyframeId, RackId, TimelineFrameIndex};

/// The shortcut that undoes (with Ctrl, or Command on a Mac), and the ones that redo.
pub const UNDO_SHORTCUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Z);
pub const REDO_SHORTCUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Y);
/// The key that deselects. (The Edit menu shows it next to its Deselect item.)
pub const DESELECT_KEY: Key = Key::Escape;

/// What the keys' meanings depend on.
#[derive(Clone, Debug)]
pub struct KeyboardState {
    pub current_frame: TimelineFrameIndex,
    pub is_playing: bool,
    /// Whether nothing is on the timeline. The keys that are about what is on it then do nothing.
    pub timeline_is_empty: bool,
    /// The selected clips.
    pub selected_clips: Vec<ClipId>,
    /// The selected keyframes.
    pub selected_keyframes: Vec<KeyframeId>,
    /// The selected racks of keyframes.
    pub selected_racks: Vec<RackId>,
    /// The selected groups.
    pub selected_groups: Vec<GroupId>,
    /// The selected clips that are at the current frame.
    pub selected_clips_at_the_current_frame: Vec<ClipId>,
    /// Whether the pointer is over the preview panel, which gives some keys another meaning than they have elsewhere.
    pub pointer_is_over_the_preview: bool,
    /// The frames where clips begin and end that the keys that go to the end of a clip go between, in order and each once: those
    /// of the selected clips if some are selected, and otherwise of every clip there is. None is past the last frame.
    pub clip_ends_to_go_between: Vec<TimelineFrameIndex>,
    /// Whether there is an edit to undo, and one that was undone to redo.
    pub can_undo: bool,
    pub can_redo: bool,
}

/// Where the pointer has to be for a key to mean what it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PointerIs {
    Anywhere,
    OverThePreview,
    /// Anywhere but over the preview.
    ElsewhereThanOverThePreview,
}

impl PointerIs {
    fn holds(self, over_the_preview: bool) -> bool {
        match self {
            Self::Anywhere => true,
            Self::OverThePreview => over_the_preview,
            Self::ElsewhereThanOverThePreview => !over_the_preview,
        }
    }

    /// Whether there is a place for the pointer to be that both of these hold in.
    #[cfg(test)]
    fn can_hold_together_with(self, other: Self) -> bool {
        !matches!(
            (self, other),
            (Self::OverThePreview, Self::ElsewhereThanOverThePreview)
                | (Self::ElsewhereThanOverThePreview, Self::OverThePreview)
        )
    }
}

/// One key, and what pressing it asks for.
struct Binding {
    key: Key,
    /// The modifiers that have to be held (more may be: see the top of this file).
    modifiers: Modifiers,
    pointer: PointerIs,
    /// Whether the key means anything when nothing is on the timeline: undoing and redoing do (they are wanted most when it has
    /// just been emptied), and the others are about what is on it.
    works_on_an_empty_timeline: bool,
    /// The commands that pressing the key asks for, when it was pressed this many times in one frame (a key that is held down
    /// repeats).
    commands: fn(&KeyboardState, usize) -> Vec<Command>,
}

/// Every key that does something, the ones that need more modifiers before the ones that need fewer.
const BINDINGS: &[Binding] = &[
    Binding {
        // Redoes (with Shift).
        key: Key::Z,
        modifiers: Modifiers::COMMAND.plus(Modifiers::SHIFT),
        pointer: PointerIs::Anywhere,
        works_on_an_empty_timeline: true,
        commands: redo,
    },
    Binding {
        // Undoes.
        key: Key::Z,
        modifiers: Modifiers::COMMAND,
        pointer: PointerIs::Anywhere,
        works_on_an_empty_timeline: true,
        commands: undo,
    },
    Binding {
        // Redoes.
        key: Key::Y,
        modifiers: Modifiers::COMMAND,
        pointer: PointerIs::Anywhere,
        works_on_an_empty_timeline: true,
        commands: redo,
    },
    Binding {
        // Plays and pauses.
        key: Key::Space,
        modifiers: Modifiers::NONE,
        pointer: PointerIs::Anywhere,
        works_on_an_empty_timeline: false,
        commands: play_or_pause,
    },
    Binding {
        // Cuts every clip at the current frame.
        key: Key::S,
        modifiers: Modifiers::SHIFT,
        pointer: PointerIs::Anywhere,
        works_on_an_empty_timeline: false,
        commands: cut_every_clip_at_the_current_frame,
    },
    Binding {
        // Cuts the selected clips at the current frame.
        key: Key::S,
        modifiers: Modifiers::NONE,
        pointer: PointerIs::Anywhere,
        works_on_an_empty_timeline: false,
        commands: cut_the_selected_clips_at_the_current_frame,
    },
    Binding {
        // Deletes.
        key: Key::Delete,
        modifiers: Modifiers::NONE,
        pointer: PointerIs::Anywhere,
        works_on_an_empty_timeline: false,
        commands: delete_what_is_selected,
    },
    Binding {
        // Deselects.
        key: DESELECT_KEY,
        modifiers: Modifiers::NONE,
        pointer: PointerIs::Anywhere,
        works_on_an_empty_timeline: false,
        commands: deselect,
    },
    Binding {
        // Goes to the next end of a clip.
        key: Key::ArrowRight,
        modifiers: Modifiers::COMMAND,
        pointer: PointerIs::Anywhere,
        works_on_an_empty_timeline: false,
        commands: to_the_next_end_of_a_clip,
    },
    Binding {
        // Goes to the last end of a clip.
        key: Key::ArrowLeft,
        modifiers: Modifiers::COMMAND,
        pointer: PointerIs::Anywhere,
        works_on_an_empty_timeline: false,
        commands: to_the_last_end_of_a_clip,
    },
    Binding {
        // Goes one frame on.
        key: Key::ArrowRight,
        modifiers: Modifiers::NONE,
        pointer: PointerIs::Anywhere,
        works_on_an_empty_timeline: false,
        commands: one_frame_forward,
    },
    Binding {
        // Goes one frame back.
        key: Key::ArrowLeft,
        modifiers: Modifiers::NONE,
        pointer: PointerIs::Anywhere,
        works_on_an_empty_timeline: false,
        commands: one_frame_back,
    },
    Binding {
        // Makes the picture of the preview fit its panel.
        key: Key::F,
        modifiers: Modifiers::NONE,
        pointer: PointerIs::OverThePreview,
        works_on_an_empty_timeline: false,
        commands: fit_the_preview,
    },
    Binding {
        // Frames what is selected in the timeline.
        key: Key::F,
        modifiers: Modifiers::NONE,
        pointer: PointerIs::ElsewhereThanOverThePreview,
        works_on_an_empty_timeline: false,
        commands: frame_the_selection_in_the_timeline,
    },
];

fn redo(state: &KeyboardState, _presses: usize) -> Vec<Command> {
    if state.can_redo {
        vec![Command::Redo]
    } else {
        Vec::new()
    }
}

fn undo(state: &KeyboardState, _presses: usize) -> Vec<Command> {
    if state.can_undo {
        vec![Command::Undo]
    } else {
        Vec::new()
    }
}

fn play_or_pause(state: &KeyboardState, _presses: usize) -> Vec<Command> {
    vec![if state.is_playing {
        Command::Pause
    } else {
        Command::Play
    }]
}

/// Cuts the selected clips that are at the current frame. (Nothing if there are none: the clips that are not selected are cut
/// by Shift and S.)
fn cut_the_selected_clips_at_the_current_frame(state: &KeyboardState, _presses: usize) -> Vec<Command> {
    if state.selected_clips_at_the_current_frame.is_empty() {
        return Vec::new();
    }
    vec![Command::SplitAt {
        frame: state.current_frame,
        only: Some(state.selected_clips_at_the_current_frame.clone()),
    }]
}

/// Cuts every clip that is at the current frame, on every track, whatever is selected.
fn cut_every_clip_at_the_current_frame(state: &KeyboardState, _presses: usize) -> Vec<Command> {
    vec![Command::SplitAt {
        frame: state.current_frame,
        only: None,
    }]
}

fn delete_what_is_selected(state: &KeyboardState, _presses: usize) -> Vec<Command> {
    let mut commands = Vec::new();
    // (The keyframes and the racks first: those that are in the drawers of the clips that go go with them.)
    if !state.selected_keyframes.is_empty() {
        commands.push(Command::DeleteKeyframes(state.selected_keyframes.clone()));
    }
    if !state.selected_racks.is_empty() {
        commands.push(Command::DeleteRacks(state.selected_racks.clone()));
    }
    if !state.selected_clips.is_empty() {
        commands.push(Command::DeleteClips(state.selected_clips.clone()));
    }
    // (A group that is selected is taken apart, and what was in it stays: the clips that are deleted with it are those that are selected.
    // A group that had nothing but those clips in it is gone with them, and has nothing left to take apart.)
    commands.extend(state.selected_groups.iter().map(|group| Command::DisbandGroup(*group)));
    commands
}

fn deselect(_state: &KeyboardState, _presses: usize) -> Vec<Command> {
    vec![Command::Deselect]
}

fn one_frame_forward(_state: &KeyboardState, presses: usize) -> Vec<Command> {
    vec![Command::MoveCurrentFrameBy(presses as i64)]
}

fn one_frame_back(_state: &KeyboardState, presses: usize) -> Vec<Command> {
    vec![Command::MoveCurrentFrameBy(-(presses as i64))]
}

/// The frames that the keys that go to the end of a clip go between, from the frame where each of `clips` begins and the frame
/// just after its last (where it ends): in order and each once, and none past the last frame of a timeline of `total_frames`,
/// because that is as far as the current frame can go. (The end of the last clip of the timeline is just past it, and so is
/// where the last frame is taken to be.) There are none on a timeline with no frames.
pub fn clip_ends_to_go_between(
    clips: impl IntoIterator<Item = (TimelineFrameIndex, TimelineFrameIndex)>,
    total_frames: u64,
) -> Vec<TimelineFrameIndex> {
    let Some(last_frame) = total_frames.checked_sub(1) else {
        return Vec::new();
    };
    let frames: std::collections::BTreeSet<u64> = clips
        .into_iter()
        .flat_map(|(first, end)| [first.value().min(last_frame), end.value().min(last_frame)])
        .collect();
    frames.into_iter().map(TimelineFrameIndex::new).collect()
}

/// Goes to the frame that is `presses` ends of clips on from the current frame (as far as there are some), if there is one.
fn to_the_next_end_of_a_clip(state: &KeyboardState, presses: usize) -> Vec<Command> {
    let frame = state
        .clip_ends_to_go_between
        .iter()
        .filter(|end| **end > state.current_frame)
        .take(presses)
        .last();
    frame.map(|frame| Command::SeekTo(*frame)).into_iter().collect()
}

/// Goes to the frame that is `presses` ends of clips back from the current frame (as far as there are some), if there is one.
fn to_the_last_end_of_a_clip(state: &KeyboardState, presses: usize) -> Vec<Command> {
    let frame = state
        .clip_ends_to_go_between
        .iter()
        .rev()
        .filter(|end| **end < state.current_frame)
        .take(presses)
        .last();
    frame.map(|frame| Command::SeekTo(*frame)).into_iter().collect()
}

fn fit_the_preview(_state: &KeyboardState, _presses: usize) -> Vec<Command> {
    vec![Command::FitThePreview]
}

/// Frames what is selected, which is something to frame only if there is a clip, a keyframe or a rack of keyframes among it.
fn frame_the_selection_in_the_timeline(state: &KeyboardState, _presses: usize) -> Vec<Command> {
    let something_is_selected =
        !state.selected_clips.is_empty() || !state.selected_keyframes.is_empty() || !state.selected_racks.is_empty();
    if something_is_selected {
        vec![Command::FrameTheSelectionInTheTimeline]
    } else {
        Vec::new()
    }
}

/// Puts the moves of the current frame that come one after another into one move (and none if they cancel out), as pressing
/// both arrow keys at once does.
fn with_the_moves_of_the_current_frame_joined(commands: Vec<Command>) -> Vec<Command> {
    let mut joined: Vec<Command> = Vec::with_capacity(commands.len());
    for command in commands {
        match (&command, joined.last_mut()) {
            (Command::MoveCurrentFrameBy(more), Some(Command::MoveCurrentFrameBy(so_far))) => *so_far += more,
            _ => joined.push(command),
        }
    }
    joined.retain(|command| *command != Command::MoveCurrentFrameBy(0));
    joined
}

/// Turns the keys pressed during this frame into commands. The keys are used up, so that no other
/// part of the window reacts to them as well.
pub fn commands_for_the_keys_pressed(context: &Context, state: KeyboardState) -> Vec<Command> {
    // Typing in a text field is not a shortcut: the keys belong to the field. Neither are the keys
    // pressed while a menu is open, which are the menu's (Escape closes it).
    if context.text_edit_focused() || Popup::is_any_open(context) {
        return Vec::new();
    }
    context.input_mut(|input| {
        let mut commands = Vec::new();
        for binding in BINDINGS {
            // With nothing on the timeline there is nothing for most keys to do, and they are left alone.
            if (state.timeline_is_empty && !binding.works_on_an_empty_timeline)
                || !binding.pointer.holds(state.pointer_is_over_the_preview)
            {
                continue;
            }
            let presses = input.count_and_consume_key(binding.modifiers, binding.key);
            if presses > 0 {
                commands.extend((binding.commands)(&state, presses));
            }
        }
        with_the_moves_of_the_current_frame_joined(commands)
    })
}

#[cfg(test)]
mod tests;
