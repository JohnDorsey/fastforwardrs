//! The three buttons that float to the right of a control of a property that can be animated when the pointer is over the
//! control: an arrow that goes to the keyframe before the playhead, a stopwatch that makes a keyframe at the playhead (or
//! takes the one that is there away), and an arrow that goes to the keyframe after it.
//!
//! The stopwatch is orange when the property has a keyframe at the playhead and grey when it has not. Each arrow is orange when
//! there is a keyframe that way (however far off) and grey, and does nothing, when there is not.

use egui::{Color32, CursorIcon, Painter, Pos2, Rect, Sense, Shape, Stroke, Ui, WidgetInfo, WidgetType, pos2, vec2};

use crate::application::commands::Command;
use crate::application::theme;
use crate::project::{AnimationTarget, Project, PropertyId, TimelineFrameIndex};

/// How wide and tall one of the buttons is, in points.
const BUTTON_SIZE: f32 = 18.0;

/// How much room there is between the buttons, in points.
const GAP_BETWEEN_BUTTONS: f32 = 2.0;

/// How much room is kept between a control and its buttons, in points.
const GAP_BEFORE_THE_BUTTONS: f32 = 8.0;

/// How much room at the right of a control is kept for its buttons, whether they are shown or not (so that nothing moves when they
/// come), in points.
pub(super) const ROOM_FOR_THE_BUTTONS: f32 = 3.0 * BUTTON_SIZE + 2.0 * GAP_BETWEEN_BUTTONS + GAP_BEFORE_THE_BUTTONS;

/// How the faces of the stopwatch and the arrows are drawn: how far the stopwatch's face is from its middle, and how wide and tall
/// an arrow is, in points.
const STOPWATCH_RADIUS: f32 = 5.5;
const ARROW_HALF_HEIGHT: f32 = 5.0;
const ARROW_LENGTH: f32 = 7.0;

/// Which of the three buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyframeButton {
    /// The arrow to the keyframe before the playhead.
    Before,
    /// The stopwatch.
    Stopwatch,
    /// The arrow to the keyframe after the playhead.
    After,
}

/// The name a button is known by to screen readers (and the tests): the stopwatch says what it does, which is the other thing
/// when there is a keyframe at the playhead already (`has_one_here`).
pub fn keyframe_button_label(property: PropertyId, button: KeyframeButton, has_one_here: bool) -> String {
    let name = property.name();
    match (button, has_one_here) {
        (KeyframeButton::Before, _) => format!("Previous keyframe of {name}"),
        (KeyframeButton::After, _) => format!("Next keyframe of {name}"),
        (KeyframeButton::Stopwatch, false) => format!("Make a keyframe of {name} here"),
        (KeyframeButton::Stopwatch, true) => format!("Take away the keyframe of {name} here"),
    }
}

/// What there is to know of the keyframes of one property of a clip or a track, for its buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct KeyframesHere {
    pub target: AnimationTarget,
    pub property: PropertyId,
    /// The frame the playhead is at.
    pub frame: TimelineFrameIndex,
    /// The frame of the last keyframe before the playhead, however far before.
    pub before: Option<TimelineFrameIndex>,
    /// The frame of the first keyframe after the playhead, however far after.
    pub after: Option<TimelineFrameIndex>,
    /// Whether there is a keyframe at the playhead.
    pub has_one_here: bool,
    /// Whether a keyframe can be made at the playhead: the playhead is on the clip (a track is everywhere).
    pub can_make_one_here: bool,
}

impl KeyframesHere {
    pub fn of(project: &Project, target: AnimationTarget, property: PropertyId, frame: TimelineFrameIndex) -> Self {
        let can_make_one_here = match target {
            AnimationTarget::Clip(clip) => project
                .placement_of(clip)
                .is_some_and(|placement| placement.offset_of(frame).is_some()),
            AnimationTarget::Track(track) => project.track(track).is_some(),
        };
        Self {
            target,
            property,
            frame,
            before: project.keyframe_before(target, property, frame),
            after: project.keyframe_after(target, property, frame),
            has_one_here: project.has_keyframe_at(target, property, frame),
            can_make_one_here,
        }
    }
}

/// The room the three buttons take up at the right end of `row` (which is as tall as the control is, and reaches to the
/// right of the room that is kept for the buttons), one after another from the left.
fn rectangles_of_the_buttons(row: Rect) -> [Rect; 3] {
    let first_left = row.right() - ROOM_FOR_THE_BUTTONS + GAP_BEFORE_THE_BUTTONS;
    // (A control that is tall, such as the box the words of a text object are in, has them at its top.)
    let top = row.top() + 2.0;
    [0.0, 1.0, 2.0].map(|number| {
        Rect::from_min_size(
            pos2(first_left + number * (BUTTON_SIZE + GAP_BETWEEN_BUTTONS), top),
            vec2(BUTTON_SIZE, BUTTON_SIZE),
        )
    })
}

/// Makes the three buttons of `here` in the room at the right end of `row`, and draws them if `shown` (the pointer is over the
/// row). A button that is clicked adds a command.
pub(super) fn show(ui: &mut Ui, row: Rect, here: &KeyframesHere, shown: bool, commands: &mut Vec<Command>) {
    let [before, stopwatch, after] = rectangles_of_the_buttons(row);
    let enabled = ui.is_enabled();
    let property = here.property;

    let buttons = [
        (KeyframeButton::Before, before, here.before.is_some()),
        (
            KeyframeButton::Stopwatch,
            stopwatch,
            here.has_one_here || here.can_make_one_here,
        ),
        (KeyframeButton::After, after, here.after.is_some()),
    ];
    for (button, rectangle, can_be_used) in buttons {
        let can_be_used = can_be_used && enabled;
        let label = keyframe_button_label(property, button, here.has_one_here);
        let id = ui.id().with(("keyframe button", here.target, property, button));
        let response = ui.interact(rectangle, id, if can_be_used { Sense::CLICK } else { Sense::hover() });
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, can_be_used, label.clone()));
        let is_orange = match button {
            KeyframeButton::Before => here.before.is_some(),
            KeyframeButton::Stopwatch => here.has_one_here,
            KeyframeButton::After => here.after.is_some(),
        };
        if shown || response.hovered() {
            let colour = if is_orange && enabled {
                theme::KEYFRAME_BUTTON_ACTIVE
            } else {
                theme::KEYFRAME_BUTTON_INACTIVE
            };
            if can_be_used && response.hovered() {
                ui.painter()
                    .rect_filled(rectangle, 4.0, theme::KEYFRAME_BUTTON_HOVERED_BACKGROUND);
            }
            draw(ui.painter(), rectangle, button, colour, here.has_one_here);
        }
        if can_be_used {
            let response = response
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text(hint_of(button, here));
            if response.clicked() {
                commands.push(command_of(button, here));
            }
        } else if matches!(button, KeyframeButton::Stopwatch) && enabled {
            response.on_hover_text("Move the playhead onto the clip to make a keyframe.");
        }
    }
}

/// What `button` does, for what is shown when the pointer rests on it.
fn hint_of(button: KeyframeButton, here: &KeyframesHere) -> &'static str {
    match button {
        KeyframeButton::Before => "Go to the keyframe before this frame",
        KeyframeButton::After => "Go to the keyframe after this frame",
        KeyframeButton::Stopwatch if here.has_one_here => "Take away the keyframe at this frame",
        KeyframeButton::Stopwatch => "Make a keyframe at this frame, with what it is now",
    }
}

/// What clicking `button` asks for. (Only for a button that can be used.)
fn command_of(button: KeyframeButton, here: &KeyframesHere) -> Command {
    match button {
        KeyframeButton::Before => Command::SeekTo(here.before.expect("an arrow with nowhere to go cannot be clicked")),
        KeyframeButton::After => Command::SeekTo(here.after.expect("an arrow with nowhere to go cannot be clicked")),
        KeyframeButton::Stopwatch if here.has_one_here => Command::RemoveKeyframe {
            target: here.target,
            property: here.property,
            frame: here.frame,
        },
        KeyframeButton::Stopwatch => Command::AddKeyframe {
            target: here.target,
            property: here.property,
            frame: here.frame,
        },
    }
}

/// Draws `button` in `rectangle` in `colour`. (The stopwatch has its face tinted when it is on a keyframe.)
fn draw(painter: &Painter, rectangle: Rect, button: KeyframeButton, colour: Color32, is_on_a_keyframe: bool) {
    let middle = rectangle.center();
    match button {
        KeyframeButton::Before => draw_an_arrow(painter, middle, -1.0, colour),
        KeyframeButton::After => draw_an_arrow(painter, middle, 1.0, colour),
        KeyframeButton::Stopwatch => draw_a_stopwatch(painter, middle, colour, is_on_a_keyframe),
    }
}

/// An arrow head (a triangle) at `middle` that points left if `direction` is -1 and right if it is 1.
fn draw_an_arrow(painter: &Painter, middle: Pos2, direction: f32, colour: Color32) {
    let tip = pos2(middle.x + direction * ARROW_LENGTH / 2.0, middle.y);
    let base_x = middle.x - direction * ARROW_LENGTH / 2.0;
    painter.add(Shape::convex_polygon(
        vec![
            tip,
            pos2(base_x, middle.y - ARROW_HALF_HEIGHT),
            pos2(base_x, middle.y + ARROW_HALF_HEIGHT),
        ],
        colour,
        Stroke::NONE,
    ));
}

/// A stopwatch at `middle`: a round face with a hand in it and a button on top.
fn draw_a_stopwatch(painter: &Painter, middle: Pos2, colour: Color32, tinted: bool) {
    let face_middle = pos2(middle.x, middle.y + 1.0);
    let stroke = Stroke::new(1.5, colour);
    if tinted {
        painter.circle_filled(face_middle, STOPWATCH_RADIUS, colour.gamma_multiply(0.35));
    }
    painter.circle_stroke(face_middle, STOPWATCH_RADIUS, stroke);
    // The button on top, and the hand.
    let top = pos2(face_middle.x, face_middle.y - STOPWATCH_RADIUS);
    painter.line_segment([top, pos2(top.x, top.y - 2.0)], stroke);
    painter.line_segment([pos2(top.x - 2.0, top.y - 2.0), pos2(top.x + 2.0, top.y - 2.0)], stroke);
    painter.line_segment([face_middle, pos2(face_middle.x + 2.5, face_middle.y - 2.5)], stroke);
}
