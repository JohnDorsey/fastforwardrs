//! What the extra buttons of a mouse do, which are shortcuts of the whole window, as the keys are (see [`super::keyboard`]):
//!
//! * **Back** (the button under the thumb of most mice that have one) goes back to the view the timeline had before the latest thing
//!   that was done to it: zooming it, scrolling it, framing what is selected. Pressing it again goes back through the views before
//!   that.
//! * **Forward** goes forward again to the view that was gone back from.
//!
//! They do nothing while something is in the way that the keys do nothing for (a dialog that has to be dealt with first).

use egui::{Context, PointerButton};

use super::commands::Command;

/// The button that goes back through the views of the timeline.
pub const BACK_BUTTON: PointerButton = PointerButton::Extra1;

/// The button that goes forward through them.
pub const FORWARD_BUTTON: PointerButton = PointerButton::Extra2;

/// The commands that the extra buttons of the mouse that were pressed in this frame ask for, in the order of the buttons.
pub fn commands_for_the_buttons_pressed(context: &Context) -> Vec<Command> {
    context.input(|input| {
        let mut commands = Vec::new();
        if input.pointer.button_pressed(BACK_BUTTON) {
            commands.push(Command::GoBackInTheTimelineViewHistory);
        }
        if input.pointer.button_pressed(FORWARD_BUTTON) {
            commands.push(Command::GoForwardInTheTimelineViewHistory);
        }
        commands
    })
}
