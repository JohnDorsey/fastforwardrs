//! What the keys of the keyboard do. They are shortcuts of the whole window, not of any one part
//! of it:
//!
//! * **Space** plays the video, or pauses it while it is playing.
//! * **S** cuts at the current frame: the selected clips that are there, and otherwise every clip
//!   that is there, on every track.
//! * **Delete** (or **Backspace**) takes the selected clips off their tracks.
//! * **Escape** deselects.
//! * **Ctrl+Z** undoes the latest edit, and **Ctrl+Y** (or **Ctrl+Shift+Z**) does it again.
//! * **Left** and **Right** move the current frame back or forward by one frame, while paused or
//!   while playing. Holding the key repeats.
//!
//! None of them does anything while something that wants the keys itself is open: a text field
//! being typed in (the title of a track, the words of a text object), or a menu or a drop-down list (Escape
//! closes those).

use egui::{Context, Key, KeyboardShortcut, Modifiers, Popup};

use super::commands::Command;
use crate::project::{ClipId, TimelineFrameIndex};

/// The key that plays and pauses.
const PLAY_OR_PAUSE_KEY: Key = Key::Space;
/// The key that cuts at the current frame.
const CUT_KEY: Key = Key::S;
const DELETE_KEYS: [Key; 2] = [Key::Delete, Key::Backspace];
/// The key that deselects. (The Edit menu shows it next to its Deselect item.)
pub const DESELECT_KEY: Key = Key::Escape;
/// The shortcut that undoes (with Ctrl, or Command on a Mac), and the ones that redo.
pub const UNDO_SHORTCUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Z);
pub const REDO_SHORTCUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Y);
const REDO_WITH_SHIFT_SHORTCUT: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::Z);
const ONE_FRAME_BACK_KEY: Key = Key::ArrowLeft;
const ONE_FRAME_FORWARD_KEY: Key = Key::ArrowRight;

/// What the keys' meanings depend on.
#[derive(Clone, Debug)]
pub struct KeyboardState {
    pub current_frame: TimelineFrameIndex,
    pub is_playing: bool,
    /// Whether nothing is on the timeline. The keys that are about what is on it then do nothing.
    pub timeline_is_empty: bool,
    /// The selected clips.
    pub selected_clips: Vec<ClipId>,
    /// The selected clips that are at the current frame.
    pub selected_clips_at_the_current_frame: Vec<ClipId>,
    /// Whether there is an edit to undo, and one that was undone to redo.
    pub can_undo: bool,
    pub can_redo: bool,
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
        // Undo and redo first, the most specific shortcut first: Ctrl+Shift+Z is not Ctrl+Z.
        if input.consume_shortcut(&REDO_WITH_SHIFT_SHORTCUT) || input.consume_shortcut(&REDO_SHORTCUT) {
            if state.can_redo {
                commands.push(Command::Redo);
            }
        } else if input.consume_shortcut(&UNDO_SHORTCUT) && state.can_undo {
            commands.push(Command::Undo);
        }
        // Everything else is about what is on the timeline: with nothing on it there is nothing for
        // the keys to do, and they are left alone. (Undoing and redoing are not about what is on it,
        // and are wanted most when it has just been emptied.)
        if state.timeline_is_empty {
            return commands;
        }
        if input.count_and_consume_key(Modifiers::NONE, PLAY_OR_PAUSE_KEY) > 0 {
            commands.push(if state.is_playing {
                Command::Pause
            } else {
                Command::Play
            });
        }
        if input.count_and_consume_key(Modifiers::NONE, CUT_KEY) > 0 {
            commands.push(Command::SplitAt {
                frame: state.current_frame,
                only: (!state.selected_clips_at_the_current_frame.is_empty())
                    .then(|| state.selected_clips_at_the_current_frame.clone()),
            });
        }
        let delete_pressed = DELETE_KEYS
            .into_iter()
            .map(|key| input.count_and_consume_key(Modifiers::NONE, key))
            .sum::<usize>()
            > 0;
        if delete_pressed && !state.selected_clips.is_empty() {
            commands.push(Command::DeleteClips(state.selected_clips.clone()));
        }
        if input.count_and_consume_key(Modifiers::NONE, DESELECT_KEY) > 0 {
            commands.push(Command::Deselect);
        }
        // Every press counts, including the repeats of a key that is held down; pressing both keys
        // in the same frame cancels out.
        let frames_forward = input.count_and_consume_key(Modifiers::NONE, ONE_FRAME_FORWARD_KEY) as i64;
        let frames_back = input.count_and_consume_key(Modifiers::NONE, ONE_FRAME_BACK_KEY) as i64;
        if frames_forward != frames_back {
            commands.push(Command::MoveCurrentFrameBy(frames_forward - frames_back));
        }
        commands
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
            selected_clips_at_the_current_frame: Vec::new(),
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
    fn s_cuts_the_selected_clips_that_are_at_the_current_frame_and_otherwise_everything_there() {
        let cut_everything = Command::SplitAt {
            frame: TimelineFrameIndex::new(40),
            only: None,
        };
        assert_eq!(pressing(Key::S, state()), vec![cut_everything.clone()]);
        // Selected clips that are not at the current frame do not stop the cut from being made.
        let selected_elsewhere = KeyboardState {
            selected_clips: vec![location(3)],
            ..state()
        };
        assert_eq!(pressing(Key::S, selected_elsewhere), vec![cut_everything]);

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
            }]
        );
    }

    #[test]
    fn delete_and_backspace_delete_all_the_selected_clips_and_do_nothing_without_any() {
        let selected = KeyboardState {
            selected_clips: vec![location(2), location(5)],
            ..state()
        };
        for key in [Key::Delete, Key::Backspace] {
            assert_eq!(
                pressing(key, selected.clone()),
                vec![Command::DeleteClips(vec![location(2), location(5)])],
                "{key:?}"
            );
            assert_eq!(
                pressing(key, state()),
                Vec::<Command>::new(),
                "{key:?} with nothing selected"
            );
        }
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
            Key::Backspace,
            Key::Escape,
            Key::ArrowLeft,
            Key::ArrowRight,
        ] {
            assert_eq!(
                pressing(key, with_a_selection.clone()),
                Vec::<Command>::new(),
                "{key:?}"
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
