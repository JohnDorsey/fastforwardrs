//! What the keys of the keyboard do. They are shortcuts of the whole window, not of any one part
//! of it:
//!
//! * **Space** plays the video, or pauses it while it is playing.
//! * **S** cuts the selected clips that are at the current frame, there, and **Shift+S** cuts every clip that is at the current
//!   frame, on every track, whatever is selected.
//! * **Delete** takes the selected clips off their tracks, and the selected keyframes and racks away.
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
use crate::project::{ClipId, KeyframeId, RackId, TimelineFrameIndex};

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
mod tests {
    use egui::Event;

    use super::*;
    use crate::project::{ClipId, TrackId};

    fn location(clip: u32) -> ClipId {
        ClipId::new(TrackId::new(0), clip)
    }

    fn state() -> KeyboardState {
        KeyboardState {
            current_frame: TimelineFrameIndex::new(40),
            is_playing: false,
            selected_clips: Vec::new(),
            selected_keyframes: Vec::new(),
            selected_racks: Vec::new(),
            selected_clips_at_the_current_frame: Vec::new(),
            pointer_is_over_the_preview: false,
            clip_ends_to_go_between: Vec::new(),
            timeline_is_empty: false,
            can_undo: false,
            can_redo: false,
        }
    }

    /// Runs one frame of a window with nothing but what `add_contents` puts in it.
    fn run_frame(context: &Context, input: egui::RawInput, add_contents: impl FnMut(&mut egui::Ui)) {
        let mut output = context.run_ui(input, add_contents);
        // Nothing here draws, so what the frame would have sent to the graphics card is dropped.
        output.textures_delta.clear();
    }

    fn key_press(key: Key, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// The commands that pressing `key` with `modifiers` held gives.
    fn pressing_with(modifiers: Modifiers, key: Key, state: KeyboardState) -> Vec<Command> {
        let context = Context::default();
        let mut commands = Vec::new();
        run_frame(
            &context,
            egui::RawInput {
                events: vec![key_press(key, modifiers)],
                ..Default::default()
            },
            |ui| {
                commands = commands_for_the_keys_pressed(ui.ctx(), state.clone());
            },
        );
        commands
    }

    /// The commands that pressing `key` gives.
    fn pressing(key: Key, state: KeyboardState) -> Vec<Command> {
        pressing_with(Modifiers::NONE, key, state)
    }

    #[test]
    fn space_plays_and_pauses() {
        assert_eq!(pressing(Key::Space, state()), vec![Command::Play]);
        let playing = KeyboardState {
            is_playing: true,
            ..state()
        };
        assert_eq!(pressing(Key::Space, playing), vec![Command::Pause]);
    }

    #[test]
    fn s_cuts_the_selected_clips_that_are_at_the_current_frame_and_nothing_else() {
        let selected_here = KeyboardState {
            selected_clips: vec![location(3), location(4), location(8)],
            selected_clips_at_the_current_frame: vec![location(3), location(8)],
            ..state()
        };
        assert_eq!(
            pressing(Key::S, selected_here),
            vec![Command::SplitAt {
                frame: TimelineFrameIndex::new(40),
                only: Some(vec![location(3), location(8)]),
            }],
            "only the selected clips that the playhead is on"
        );
    }

    #[test]
    fn s_cuts_nothing_when_nothing_is_selected_or_what_is_selected_is_not_at_the_current_frame() {
        assert_eq!(
            pressing(Key::S, state()),
            Vec::<Command>::new(),
            "nothing is selected: there is nothing to cut, and the clips that are there are not cut"
        );
        let selected_elsewhere = KeyboardState {
            selected_clips: vec![location(3)],
            ..state()
        };
        assert_eq!(
            pressing(Key::S, selected_elsewhere),
            Vec::<Command>::new(),
            "the clips that are there are not cut in place of the selected ones that are not"
        );
        let only_keyframes = KeyboardState {
            selected_keyframes: vec![KeyframeId::new(RackId::new(TrackId::new(0), 1), 0)],
            ..state()
        };
        assert_eq!(pressing(Key::S, only_keyframes), Vec::<Command>::new());
    }

    #[test]
    fn shift_s_cuts_every_clip_that_is_at_the_current_frame_whatever_is_selected() {
        let cut_everything = vec![Command::SplitAt {
            frame: TimelineFrameIndex::new(40),
            only: None,
        }];
        assert_eq!(pressing_with(Modifiers::SHIFT, Key::S, state()), cut_everything);
        let selected_here = KeyboardState {
            selected_clips: vec![location(3)],
            selected_clips_at_the_current_frame: vec![location(3)],
            ..state()
        };
        assert_eq!(
            pressing_with(Modifiers::SHIFT, Key::S, selected_here),
            cut_everything,
            "the selection does not narrow it down"
        );
        let selected_elsewhere = KeyboardState {
            selected_clips: vec![location(3)],
            ..state()
        };
        assert_eq!(
            pressing_with(Modifiers::SHIFT, Key::S, selected_elsewhere),
            cut_everything
        );
    }

    #[test]
    fn delete_deletes_all_the_selected_clips_and_does_nothing_without_any() {
        let selected = KeyboardState {
            selected_clips: vec![location(2), location(5)],
            ..state()
        };
        assert_eq!(
            pressing(Key::Delete, selected.clone()),
            vec![Command::DeleteClips(vec![location(2), location(5)])]
        );
        assert_eq!(
            pressing(Key::Delete, state()),
            Vec::<Command>::new(),
            "with nothing selected"
        );
    }

    #[test]
    fn backspace_is_not_a_shortcut_of_anything() {
        let everything_selected = KeyboardState {
            selected_clips: vec![location(2)],
            selected_keyframes: vec![KeyframeId::new(RackId::new(TrackId::new(0), 1), 0)],
            selected_racks: vec![RackId::new(TrackId::new(0), 1)],
            selected_clips_at_the_current_frame: vec![location(2)],
            ..with_edits_to_undo_and_redo()
        };
        assert_eq!(
            pressing(Key::Backspace, everything_selected.clone()),
            Vec::<Command>::new()
        );
        assert_eq!(
            pressing_with(Modifiers::SHIFT, Key::Backspace, everything_selected),
            Vec::<Command>::new()
        );
    }

    #[test]
    fn delete_takes_away_the_selected_keyframes_before_the_selected_clips_go() {
        let keyframe =
            |number| KeyframeId::new(crate::project::RackId::new(crate::project::TrackId::new(0), 1), number);
        let both = KeyboardState {
            selected_clips: vec![location(2)],
            selected_keyframes: vec![keyframe(0), keyframe(4)],
            ..state()
        };
        assert_eq!(
            pressing(Key::Delete, both),
            vec![
                Command::DeleteKeyframes(vec![keyframe(0), keyframe(4)]),
                Command::DeleteClips(vec![location(2)])
            ],
            "the keyframes of the clips that go would go with them"
        );
        let only_keyframes = KeyboardState {
            selected_keyframes: vec![keyframe(7)],
            ..state()
        };
        assert_eq!(
            pressing(Key::Delete, only_keyframes),
            vec![Command::DeleteKeyframes(vec![keyframe(7)])]
        );
    }

    #[test]
    fn delete_takes_away_the_selected_racks_after_the_keyframes_and_before_the_clips() {
        let racks = vec![
            crate::project::RackId::new(crate::project::TrackId::new(0), 1),
            crate::project::RackId::new(crate::project::TrackId::new(0), 4),
        ];
        let keyframe = KeyframeId::new(racks[0], 0);
        let everything = KeyboardState {
            selected_clips: vec![location(2)],
            selected_keyframes: vec![keyframe],
            selected_racks: racks.clone(),
            ..state()
        };
        assert_eq!(
            pressing(Key::Delete, everything),
            vec![
                Command::DeleteKeyframes(vec![keyframe]),
                Command::DeleteRacks(racks.clone()),
                Command::DeleteClips(vec![location(2)])
            ]
        );
        let only_racks = KeyboardState {
            selected_racks: racks.clone(),
            ..state()
        };
        assert_eq!(
            pressing(Key::Delete, only_racks.clone()),
            vec![Command::DeleteRacks(racks)]
        );
        assert_eq!(
            pressing(Key::Space, only_racks),
            vec![Command::Play],
            "the other keys are not about racks"
        );
    }

    #[test]
    fn escape_deselects() {
        assert_eq!(pressing(Key::Escape, state()), vec![Command::Deselect]);
    }

    #[test]
    fn the_arrow_keys_move_one_frame_at_a_time() {
        assert_eq!(pressing(Key::ArrowRight, state()), vec![Command::MoveCurrentFrameBy(1)]);
        assert_eq!(pressing(Key::ArrowLeft, state()), vec![Command::MoveCurrentFrameBy(-1)]);
    }

    #[test]
    fn a_key_that_repeats_moves_as_many_frames_as_it_was_pressed_in_the_frame_and_both_keys_cancel_out() {
        let times = |key: Key, count: usize| {
            let context = Context::default();
            let mut commands = Vec::new();
            run_frame(
                &context,
                egui::RawInput {
                    events: vec![key_press(key, Modifiers::NONE); count],
                    ..Default::default()
                },
                |ui| commands = commands_for_the_keys_pressed(ui.ctx(), state()),
            );
            commands
        };
        assert_eq!(times(Key::ArrowRight, 3), vec![Command::MoveCurrentFrameBy(3)]);
        assert_eq!(times(Key::ArrowLeft, 2), vec![Command::MoveCurrentFrameBy(-2)]);

        let context = Context::default();
        let mut commands = Vec::new();
        run_frame(
            &context,
            egui::RawInput {
                events: vec![
                    key_press(Key::ArrowRight, Modifiers::NONE),
                    key_press(Key::ArrowLeft, Modifiers::NONE),
                ],
                ..Default::default()
            },
            |ui| commands = commands_for_the_keys_pressed(ui.ctx(), state()),
        );
        assert_eq!(commands, Vec::<Command>::new(), "pressing both at once cancels out");
    }

    // --- going to the end of a clip ---

    /// A state whose current frame is 40, and in which the clips begin and end at `frames`.
    fn with_clips_beginning_and_ending_at(frames: &[u64]) -> KeyboardState {
        KeyboardState {
            clip_ends_to_go_between: frames.iter().map(|frame| TimelineFrameIndex::new(*frame)).collect(),
            ..state()
        }
    }

    fn seek_to(frame: u64) -> Vec<Command> {
        vec![Command::SeekTo(TimelineFrameIndex::new(frame))]
    }

    #[test]
    fn control_right_goes_to_the_nearest_end_of_a_clip_after_the_current_frame_and_control_left_to_the_one_before() {
        let state = with_clips_beginning_and_ending_at(&[0, 10, 25, 60, 90, 120]);
        assert_eq!(
            pressing_with(Modifiers::COMMAND, Key::ArrowRight, state.clone()),
            seek_to(60)
        );
        assert_eq!(pressing_with(Modifiers::COMMAND, Key::ArrowLeft, state), seek_to(25));
    }

    #[test]
    fn the_end_of_a_clip_that_the_current_frame_is_at_is_not_the_one_that_is_gone_to() {
        let state = with_clips_beginning_and_ending_at(&[10, 40, 70]);
        assert_eq!(
            pressing_with(Modifiers::COMMAND, Key::ArrowRight, state.clone()),
            seek_to(70)
        );
        assert_eq!(pressing_with(Modifiers::COMMAND, Key::ArrowLeft, state), seek_to(10));
    }

    #[test]
    fn there_is_nowhere_to_go_past_the_last_end_of_a_clip_or_before_the_first() {
        let all_after = with_clips_beginning_and_ending_at(&[50, 60]);
        assert_eq!(
            pressing_with(Modifiers::COMMAND, Key::ArrowLeft, all_after),
            Vec::<Command>::new()
        );
        let all_before = with_clips_beginning_and_ending_at(&[10, 20]);
        assert_eq!(
            pressing_with(Modifiers::COMMAND, Key::ArrowRight, all_before),
            Vec::<Command>::new()
        );
        let none = with_clips_beginning_and_ending_at(&[]);
        for key in [Key::ArrowLeft, Key::ArrowRight] {
            assert_eq!(
                pressing_with(Modifiers::COMMAND, key, none.clone()),
                Vec::<Command>::new()
            );
        }
    }

    #[test]
    fn a_control_arrow_that_repeats_goes_that_many_ends_on_and_stops_at_the_last_there_is() {
        let state = with_clips_beginning_and_ending_at(&[0, 10, 25, 60, 90, 120]);
        let times = |key: Key, count: usize| {
            let context = Context::default();
            let mut commands = Vec::new();
            run_frame(
                &context,
                egui::RawInput {
                    events: vec![key_press(key, Modifiers::COMMAND); count],
                    ..Default::default()
                },
                |ui| commands = commands_for_the_keys_pressed(ui.ctx(), state.clone()),
            );
            commands
        };
        assert_eq!(times(Key::ArrowRight, 2), seek_to(90), "60 and then 90");
        assert_eq!(times(Key::ArrowRight, 9), seek_to(120), "no further than the last");
        assert_eq!(times(Key::ArrowLeft, 2), seek_to(10), "25 and then 10");
        assert_eq!(times(Key::ArrowLeft, 9), seek_to(0));
    }

    #[test]
    fn a_control_arrow_does_not_also_move_one_frame_and_an_arrow_does_not_go_to_the_end_of_a_clip() {
        let state = with_clips_beginning_and_ending_at(&[0, 60]);
        assert_eq!(
            pressing_with(Modifiers::COMMAND, Key::ArrowRight, state.clone()),
            seek_to(60)
        );
        assert_eq!(pressing(Key::ArrowRight, state), vec![Command::MoveCurrentFrameBy(1)]);
    }

    // --- the frames that those keys go between ---

    /// Clips that begin and end at the frames given as (where the clip begins, the frame just after its last).
    fn clips_at(frames: &[(u64, u64)]) -> Vec<(TimelineFrameIndex, TimelineFrameIndex)> {
        frames
            .iter()
            .map(|(first, end)| (TimelineFrameIndex::new(*first), TimelineFrameIndex::new(*end)))
            .collect()
    }

    fn frames_numbered(numbers: &[u64]) -> Vec<TimelineFrameIndex> {
        numbers.iter().map(|number| TimelineFrameIndex::new(*number)).collect()
    }

    #[test]
    fn the_frames_to_go_between_are_where_each_clip_begins_and_ends_in_order_and_each_once() {
        // Clips of several tracks, in no order, where one begins where another ends and where two begin at the same frame.
        let clips = clips_at(&[(100, 200), (0, 100), (50, 150), (100, 120)]);
        assert_eq!(
            clip_ends_to_go_between(clips, 300),
            frames_numbered(&[0, 50, 100, 120, 150, 200])
        );
    }

    #[test]
    fn the_end_of_the_last_clip_is_the_last_frame_and_not_past_it() {
        // A timeline of 300 frames, whose last clip ends at 300, just after the last frame, 299: that is as far as it goes.
        assert_eq!(
            clip_ends_to_go_between(clips_at(&[(0, 100), (100, 300)]), 300),
            frames_numbered(&[0, 100, 299])
        );
        // A clip of one frame at the end begins at 299 and ends at 300, which are both taken to be 299: once.
        assert_eq!(
            clip_ends_to_go_between(clips_at(&[(0, 299), (299, 300)]), 300),
            frames_numbered(&[0, 299])
        );
    }

    #[test]
    fn no_frame_to_go_between_is_past_the_last_frame_whatever_is_asked() {
        assert_eq!(
            clip_ends_to_go_between(clips_at(&[(0, 10), (290, 300), (300, 310)]), 300),
            frames_numbered(&[0, 10, 290, 299])
        );
    }

    #[test]
    fn there_are_no_frames_to_go_between_when_there_are_no_clips_or_no_frames() {
        assert_eq!(clip_ends_to_go_between(clips_at(&[]), 300), Vec::new());
        assert_eq!(
            clip_ends_to_go_between(clips_at(&[(0, 0)]), 0),
            Vec::new(),
            "a timeline with no frames has no last frame to go to"
        );
    }

    // --- F ---

    #[test]
    fn f_over_the_preview_makes_the_picture_fit_the_panel_whatever_is_selected() {
        let over_the_preview = KeyboardState {
            pointer_is_over_the_preview: true,
            ..state()
        };
        assert_eq!(pressing(Key::F, over_the_preview.clone()), vec![Command::FitThePreview]);
        let with_a_selection = KeyboardState {
            selected_clips: vec![location(1)],
            ..over_the_preview
        };
        assert_eq!(
            pressing(Key::F, with_a_selection),
            vec![Command::FitThePreview],
            "it is not the timeline that is framed"
        );
    }

    #[test]
    fn f_elsewhere_frames_what_is_selected_in_the_timeline() {
        let keyframe = KeyframeId::new(RackId::new(TrackId::new(0), 1), 0);
        let rack = RackId::new(TrackId::new(0), 1);
        let selections = [
            (
                "a clip",
                KeyboardState {
                    selected_clips: vec![location(1)],
                    ..state()
                },
            ),
            (
                "a keyframe",
                KeyboardState {
                    selected_keyframes: vec![keyframe],
                    ..state()
                },
            ),
            (
                "a rack",
                KeyboardState {
                    selected_racks: vec![rack],
                    ..state()
                },
            ),
        ];
        for (what, selected) in selections {
            assert_eq!(
                pressing(Key::F, selected),
                vec![Command::FrameTheSelectionInTheTimeline],
                "with {what} selected"
            );
        }
    }

    #[test]
    fn f_elsewhere_with_nothing_selected_does_nothing() {
        assert_eq!(pressing(Key::F, state()), Vec::<Command>::new());
    }

    // --- undo and redo ---

    fn with_edits_to_undo_and_redo() -> KeyboardState {
        KeyboardState {
            can_undo: true,
            can_redo: true,
            ..state()
        }
    }

    #[test]
    fn control_z_undoes() {
        assert_eq!(
            pressing_with(Modifiers::COMMAND, Key::Z, with_edits_to_undo_and_redo()),
            vec![Command::Undo]
        );
    }

    #[test]
    fn control_y_and_control_shift_z_both_redo() {
        assert_eq!(
            pressing_with(Modifiers::COMMAND, Key::Y, with_edits_to_undo_and_redo()),
            vec![Command::Redo]
        );
        assert_eq!(
            pressing_with(
                Modifiers::COMMAND.plus(Modifiers::SHIFT),
                Key::Z,
                with_edits_to_undo_and_redo()
            ),
            vec![Command::Redo],
            "and Ctrl+Shift+Z is a redo and not an undo"
        );
    }

    #[test]
    fn there_is_nothing_to_do_when_there_is_nothing_to_undo_or_redo() {
        assert_eq!(
            pressing_with(Modifiers::COMMAND, Key::Z, state()),
            Vec::<Command>::new()
        );
        assert_eq!(
            pressing_with(Modifiers::COMMAND, Key::Y, state()),
            Vec::<Command>::new()
        );
        let can_only_redo = KeyboardState {
            can_redo: true,
            ..state()
        };
        assert_eq!(
            pressing_with(Modifiers::COMMAND, Key::Z, can_only_redo),
            Vec::<Command>::new()
        );
    }

    #[test]
    fn undo_and_redo_work_with_nothing_on_the_timeline_and_the_other_keys_do_nothing() {
        // (Deleting the only clip, or undoing the first import, leaves nothing on the timeline,
        // and that is just when undoing or redoing is wanted.)
        let empty = KeyboardState {
            timeline_is_empty: true,
            ..with_edits_to_undo_and_redo()
        };
        assert_eq!(
            pressing_with(Modifiers::COMMAND, Key::Z, empty.clone()),
            vec![Command::Undo]
        );
        assert_eq!(
            pressing_with(Modifiers::COMMAND, Key::Y, empty.clone()),
            vec![Command::Redo]
        );
        let with_a_selection = KeyboardState {
            selected_clips: vec![location(1)],
            ..empty
        };
        for key in [
            Key::Space,
            Key::S,
            Key::Delete,
            Key::Escape,
            Key::ArrowLeft,
            Key::ArrowRight,
            Key::F,
        ] {
            assert_eq!(
                pressing(key, with_a_selection.clone()),
                Vec::<Command>::new(),
                "{key:?}"
            );
        }
        for (modifiers, key) in [
            (Modifiers::SHIFT, Key::S),
            (Modifiers::COMMAND, Key::ArrowLeft),
            (Modifiers::COMMAND, Key::ArrowRight),
        ] {
            assert_eq!(
                pressing_with(modifiers, key, with_a_selection.clone()),
                Vec::<Command>::new(),
                "{modifiers:?} {key:?}"
            );
        }
    }

    #[test]
    fn z_and_y_without_control_are_not_shortcuts() {
        for key in [Key::Z, Key::Y] {
            assert_eq!(
                pressing(key, with_edits_to_undo_and_redo()),
                Vec::<Command>::new(),
                "{key:?}"
            );
        }
    }

    // --- the table ---

    #[test]
    fn a_row_of_the_table_never_comes_before_a_row_for_the_same_key_that_needs_more_modifiers() {
        for (earlier_place, earlier) in BINDINGS.iter().enumerate() {
            for later in &BINDINGS[earlier_place + 1..] {
                if earlier.key != later.key || !earlier.pointer.can_hold_together_with(later.pointer) {
                    continue;
                }
                // What is pressed for the later row is what the earlier row would use up, if it matches it.
                assert!(
                    !later.modifiers.matches_logically(earlier.modifiers),
                    "{:?} with {:?} held would be used up by the row before it, for the same key with {:?}",
                    later.key,
                    later.modifiers,
                    earlier.modifiers
                );
            }
        }
    }

    #[test]
    fn two_rows_of_the_table_are_never_for_the_very_same_key_and_modifiers_in_the_same_place() {
        for (earlier_place, earlier) in BINDINGS.iter().enumerate() {
            for later in &BINDINGS[earlier_place + 1..] {
                let same = earlier.key == later.key
                    && earlier.modifiers == later.modifiers
                    && earlier.pointer.can_hold_together_with(later.pointer);
                assert!(
                    !same,
                    "{:?} with {:?} held is the key of two rows",
                    earlier.key, earlier.modifiers
                );
            }
        }
    }

    // --- when the keys are not shortcuts ---

    #[test]
    fn typing_in_a_text_field_is_not_a_shortcut() {
        let context = Context::default();
        let mut text = String::new();
        // One frame in which a text field gets the keyboard focus...
        run_frame(&context, egui::RawInput::default(), |ui| {
            ui.text_edit_singleline(&mut text).request_focus();
        });
        run_frame(&context, egui::RawInput::default(), |ui| {
            ui.text_edit_singleline(&mut text);
        });
        assert!(context.text_edit_focused(), "the text field has the focus");
        // ...and then the keys are pressed while it has it.
        let mut commands = vec![Command::Play];
        run_frame(
            &context,
            egui::RawInput {
                events: [Key::Space, Key::S, Key::Delete, Key::Backspace, Key::ArrowLeft]
                    .into_iter()
                    .map(|key| key_press(key, Modifiers::NONE))
                    .chain([key_press(Key::Z, Modifiers::COMMAND)])
                    .collect(),
                ..Default::default()
            },
            |ui| {
                let selected = KeyboardState {
                    selected_clips: vec![location(1)],
                    can_undo: true,
                    ..state()
                };
                commands = commands_for_the_keys_pressed(ui.ctx(), selected);
            },
        );
        assert_eq!(commands, Vec::<Command>::new());
    }
}
