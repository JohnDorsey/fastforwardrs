//! What the keys of the keyboard do. They are shortcuts of the whole window, not of any one part
//! of it:
//!
//! * **Space** plays the video, or pauses it while it is playing.
//! * **S** cuts at the current frame: the selected clip if it is there, and otherwise every clip
//!   that is there, on every track.
//! * **Delete** (or **Backspace**) takes the selected clip off its track.
//! * **Escape** deselects.
//! * **Left** and **Right** move the current frame back or forward by one frame, while paused or
//!   while playing. Holding the key repeats.
//!
//! None of them does anything while something that wants the keys itself is open: a text field
//! being typed in, a menu or a drop-down list (Escape closes those), or a box with sliders and boxes
//! to tick (they are given the keys instead).

use egui::{Context, Key, Modifiers, Popup};

use super::commands::Command;
use crate::project::{ClipLocation, TimelineFrameIndex};

/// The key that plays and pauses.
const PLAY_OR_PAUSE_KEY: Key = Key::Space;
/// The key that cuts at the current frame.
const CUT_KEY: Key = Key::S;
const DELETE_KEYS: [Key; 2] = [Key::Delete, Key::Backspace];
/// The key that deselects. (The Edit menu shows it next to its Deselect item.)
pub const DESELECT_KEY: Key = Key::Escape;
const ONE_FRAME_BACK_KEY: Key = Key::ArrowLeft;
const ONE_FRAME_FORWARD_KEY: Key = Key::ArrowRight;

/// What the keys' meanings depend on.
#[derive(Clone, Copy, Debug)]
pub struct KeyboardState {
    pub current_frame: TimelineFrameIndex,
    pub is_playing: bool,
    /// The selected clip, if there is one.
    pub selected_clip: Option<ClipLocation>,
    /// The selected clip, if it is one of the clips that is at the current frame.
    pub selected_clip_at_the_current_frame: Option<ClipLocation>,
    /// Whether a box is open that has controls of its own, which use the keys.
    pub a_box_with_controls_is_open: bool,
}

/// Turns the keys pressed during this frame into commands. The keys are used up, so that no other
/// part of the window reacts to them as well.
pub fn commands_for_the_keys_pressed(context: &Context, state: KeyboardState) -> Vec<Command> {
    // Typing in a text field is not a shortcut: the keys belong to the field. Neither are the keys
    // pressed while a menu is open, which are the menu's (Escape closes it).
    if context.text_edit_focused() || Popup::is_any_open(context) || state.a_box_with_controls_is_open {
        return Vec::new();
    }
    context.input_mut(|input| {
        let mut commands = Vec::new();
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
                only: state.selected_clip_at_the_current_frame,
            });
        }
        let delete_pressed = DELETE_KEYS
            .into_iter()
            .map(|key| input.count_and_consume_key(Modifiers::NONE, key))
            .sum::<usize>()
            > 0;
        if delete_pressed && let Some(selected_clip) = state.selected_clip {
            commands.push(Command::DeleteClip(selected_clip));
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

    fn location(clip: u32) -> ClipLocation {
        ClipLocation {
            track: TrackId::new(0),
            clip: ClipId::new(clip),
        }
    }

    fn state() -> KeyboardState {
        KeyboardState {
            current_frame: TimelineFrameIndex::new(40),
            is_playing: false,
            selected_clip: None,
            selected_clip_at_the_current_frame: None,
            a_box_with_controls_is_open: false,
        }
    }

    /// Runs one frame of a window with nothing but what `add_contents` puts in it.
    fn run_frame(context: &Context, input: egui::RawInput, add_contents: impl FnMut(&mut egui::Ui)) {
        let mut output = context.run_ui(input, add_contents);
        // Nothing here draws, so what the frame would have sent to the graphics card is dropped.
        output.textures_delta.clear();
    }

    fn key_press(key: Key) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }
    }

    /// The commands that pressing `key` gives.
    fn pressing(key: Key, state: KeyboardState) -> Vec<Command> {
        let context = Context::default();
        let mut commands = Vec::new();
        run_frame(
            &context,
            egui::RawInput {
                events: vec![key_press(key)],
                ..Default::default()
            },
            |ui| {
                commands = commands_for_the_keys_pressed(ui.ctx(), state);
            },
        );
        commands
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
    fn s_cuts_the_selected_clip_when_it_is_at_the_current_frame_and_otherwise_everything_there() {
        let cut_everything = Command::SplitAt {
            frame: TimelineFrameIndex::new(40),
            only: None,
        };
        assert_eq!(pressing(Key::S, state()), vec![cut_everything.clone()]);
        // A selected clip that is not at the current frame does not stop the cut from being made.
        let selected_elsewhere = KeyboardState {
            selected_clip: Some(location(3)),
            ..state()
        };
        assert_eq!(pressing(Key::S, selected_elsewhere), vec![cut_everything]);

        let selected_here = KeyboardState {
            selected_clip: Some(location(3)),
            selected_clip_at_the_current_frame: Some(location(3)),
            ..state()
        };
        assert_eq!(
            pressing(Key::S, selected_here),
            vec![Command::SplitAt {
                frame: TimelineFrameIndex::new(40),
                only: Some(location(3)),
            }]
        );
    }

    #[test]
    fn delete_and_backspace_delete_the_selected_clip_and_do_nothing_without_one() {
        let selected = KeyboardState {
            selected_clip: Some(location(2)),
            ..state()
        };
        for key in [Key::Delete, Key::Backspace] {
            assert_eq!(
                pressing(key, selected),
                vec![Command::DeleteClip(location(2))],
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
                    .map(key_press)
                    .collect(),
                ..Default::default()
            },
            |ui| {
                let selected = KeyboardState {
                    selected_clip: Some(location(1)),
                    ..state()
                };
                commands = commands_for_the_keys_pressed(ui.ctx(), selected);
            },
        );
        assert_eq!(commands, Vec::<Command>::new());
    }

    #[test]
    fn nothing_is_a_shortcut_while_a_box_with_controls_of_its_own_is_open() {
        let box_open = KeyboardState {
            selected_clip: Some(location(1)),
            a_box_with_controls_is_open: true,
            ..state()
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
            assert_eq!(pressing(key, box_open), Vec::<Command>::new(), "{key:?}");
        }
    }
}
