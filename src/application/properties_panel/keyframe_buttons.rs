// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The three buttons that float to the right of a control of a property that can be animated when the pointer is over the
//! control: an arrow that goes to the keyframe before the playhead, a stopwatch that makes a keyframe at the playhead (or
//! takes the one that is there away), and an arrow that goes to the keyframe after it.
//!
//! The stopwatch is orange when the property has a keyframe at the playhead and grey when it has not. Each arrow is orange when
//! there is a keyframe that way (however far off) and grey, and does nothing, when there is not.

use egui::{Color32, CursorIcon, Painter, Pos2, Rect, Sense, Shape, Stroke, Ui, WidgetInfo, WidgetType, pos2, vec2};

use crate::application::commands::Command;
use crate::application::mask_settings_window::WhichValue;
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

/// How much more room a row keeps at its right end for the mask button, which is next to the three buttons of keyframes (on the rows of the numbers of a shader only), in points.
pub(super) const ROOM_FOR_THE_MASK_BUTTON: f32 = BUTTON_SIZE + GAP_BETWEEN_BUTTONS;

/// The mask button of a number of a shader (`docs/shader-masks-design.md`): it opens the window that says what the number is read from.
#[derive(Clone, Copy)]
pub(super) struct MaskButton {
    pub which: WhichValue,
    /// Why the number cannot be read from a mask, if it cannot (the button is dimmed, and says why).
    pub cannot_because: Option<&'static str>,
    /// Whether the number can also be read from a mask **laid in space** (its shader says where in space it works as it uses the number): the window of the mask then has the choice, and the
    /// button has a diamond round it, the outline of a plane seen corner on.
    pub world_space_is_available: bool,
}

/// What the mask button is called to a screen reader and the tests: `name` is what the number is called, in lower case.
pub fn mask_button_label(name: &str) -> String {
    format!("Read {name} from a mask")
}

/// What the mask button of a number that can be read from a mask laid in space is called: it says so.
pub fn mask_button_label_with_world_space(name: &str) -> String {
    format!("Read {name} from a mask, over the picture or in space")
}

/// What the mask button says when the pointer rests on it.
const HINT_OF_THE_MASK_BUTTON: &str = "Read this number from the picture of a track, pixel by pixel";

/// What the mask button of a number that can be read from a mask laid in space says when the pointer rests on it.
const HINT_OF_THE_MASK_BUTTON_WITH_WORLD_SPACE: &str = "Read this number from the picture of a track, pixel by pixel. World space options are available: the window of the mask can lay the picture in space, on a plane, a sphere or a cylinder, so that the number changes from place to place of the shape the shader makes.";

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
/// when there is a keyframe at the playhead already (`has_one_here`). `name` is what the property is called, in lower case (see
/// [`crate::application::naming::name_of_a_property_of`]).
pub fn keyframe_button_label(name: &str, button: KeyframeButton, has_one_here: bool) -> String {
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
/// row). A button that is clicked adds a command. `name` is what the property is called, in lower case.
pub(super) fn show(ui: &mut Ui, row: Rect, here: &KeyframesHere, name: &str, shown: bool, commands: &mut Vec<Command>) {
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
        let label = keyframe_button_label(name, button, here.has_one_here);
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

/// Makes the mask button of `mask` in the room next to the three buttons of keyframes at the right end of `row`, and draws it if `shown` (the pointer is over the row). Clicking it asks for
/// the window of the mask. `name` is what the number is called, in lower case.
pub(super) fn show_the_mask_button(
    ui: &mut Ui,
    row: Rect,
    mask: MaskButton,
    name: &str,
    shown: bool,
    commands: &mut Vec<Command>,
) {
    let [before, ..] = rectangles_of_the_buttons(row);
    let rectangle = Rect::from_min_size(
        pos2(before.left() - GAP_BETWEEN_BUTTONS - BUTTON_SIZE, before.top()),
        vec2(BUTTON_SIZE, BUTTON_SIZE),
    );
    show_the_mask_button_in(ui, rectangle, mask, name, shown, commands);
}

/// The same for a row that has **no buttons of keyframes** (a value of the effect of a group is not animated): the mask button is at the right end of `row`, in [`ROOM_FOR_THE_MASK_BUTTON`].
pub(super) fn show_the_mask_button_at_the_end(
    ui: &mut Ui,
    row: Rect,
    mask: MaskButton,
    name: &str,
    shown: bool,
    commands: &mut Vec<Command>,
) {
    let rectangle = Rect::from_min_size(
        pos2(row.right() - BUTTON_SIZE, row.top() + 2.0),
        vec2(BUTTON_SIZE, BUTTON_SIZE),
    );
    show_the_mask_button_in(ui, rectangle, mask, name, shown, commands);
}

/// The mask button of `mask` in `rectangle`.
fn show_the_mask_button_in(
    ui: &mut Ui,
    rectangle: Rect,
    mask: MaskButton,
    name: &str,
    shown: bool,
    commands: &mut Vec<Command>,
) {
    let can_be_used = mask.cannot_because.is_none() && ui.is_enabled();
    let label = if mask.world_space_is_available {
        mask_button_label_with_world_space(name)
    } else {
        mask_button_label(name)
    };
    let id = ui
        .id()
        .with(("mask button", mask.which.owner, mask.which.instance, mask.which.slot));
    let response = ui.interact(rectangle, id, if can_be_used { Sense::CLICK } else { Sense::hover() });
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, can_be_used, label.clone()));
    if shown || response.hovered() {
        if can_be_used && response.hovered() {
            ui.painter()
                .rect_filled(rectangle, 4.0, theme::KEYFRAME_BUTTON_HOVERED_BACKGROUND);
        }
        draw_a_mask(
            ui.painter(),
            rectangle.center(),
            theme::KEYFRAME_BUTTON_INACTIVE,
            mask.world_space_is_available,
        );
    }
    if can_be_used {
        let response =
            response
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text(if mask.world_space_is_available {
                    HINT_OF_THE_MASK_BUTTON_WITH_WORLD_SPACE
                } else {
                    HINT_OF_THE_MASK_BUTTON
                });
        if response.clicked() {
            commands.push(Command::ShowTheMaskSettings(mask.which));
        }
    } else if let Some(why) = mask.cannot_because {
        response.on_hover_text(why);
    }
}

/// How far the corners of the diamond round a mask that can be laid in space are from its middle, in points: as far as the button allows (it is 18 across, and the corners are in it).
const DIAMOND_RADIUS: f32 = 8.6;

/// How big the round face is inside the diamond, in points: smaller than the face alone, so that the sides of the diamond (which are `DIAMOND_RADIUS` over the square root of 2 from the middle)
/// stand clear of it by more than a pixel, and the diamond is seen as a diamond and not as a rounded blur.
const RADIUS_OF_THE_FACE_IN_A_DIAMOND: f32 = 4.0;

/// A mask at `middle`: a round face with its left half filled in, and, if it can be laid in space, **a diamond round it**: the outline of a plane seen from its corner.
fn draw_a_mask(painter: &Painter, middle: Pos2, colour: Color32, with_a_diamond: bool) {
    if with_a_diamond {
        painter.add(Shape::closed_line(
            vec![
                pos2(middle.x, middle.y - DIAMOND_RADIUS),
                pos2(middle.x + DIAMOND_RADIUS, middle.y),
                pos2(middle.x, middle.y + DIAMOND_RADIUS),
                pos2(middle.x - DIAMOND_RADIUS, middle.y),
            ],
            Stroke::new(1.0, colour),
        ));
    }
    let radius = if with_a_diamond {
        RADIUS_OF_THE_FACE_IN_A_DIAMOND
    } else {
        STOPWATCH_RADIUS
    };
    painter.circle_stroke(middle, radius, Stroke::new(1.5, colour));
    let half: Vec<Pos2> = (0..=12)
        .map(|step| {
            let angle = std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * step as f32 / 12.0;
            pos2(middle.x + radius * angle.cos(), middle.y - radius * angle.sin())
        })
        .collect();
    painter.add(Shape::convex_polygon(half, colour, Stroke::NONE));
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
