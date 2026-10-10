//! What the keys of the keyboard do. They are shortcuts of the whole window, not of any one part
//! of it:
//!
//! * **S** cuts the video track at the current frame.
//! * **Left** and **Right** move the current frame back or forward by one frame, while paused or
//!   while playing. Holding the key repeats.

use egui::{Context, Key, Modifiers};

use super::commands::Command;
use crate::project::TimelineFrameIndex;

/// The key that cuts the video track at the current frame.
const CUT_KEY: Key = Key::S;
const ONE_FRAME_BACK_KEY: Key = Key::ArrowLeft;
const ONE_FRAME_FORWARD_KEY: Key = Key::ArrowRight;

/// Turns the keys pressed during this frame into commands. The keys are used up, so that no other
/// part of the window reacts to them as well.
pub fn commands_for_the_keys_pressed(context: &Context, current_frame: TimelineFrameIndex) -> Vec<Command> {
    context.input_mut(|input| {
        let mut commands = Vec::new();
        if input.count_and_consume_key(Modifiers::NONE, CUT_KEY) > 0 {
            commands.push(Command::SplitAt(current_frame));
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
