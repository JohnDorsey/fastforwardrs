//! The box at the left of each track: the track's title, which can be changed by typing in it. Clicking the
//! box (or its title) selects the track, which is not the clips on it: the settings of the track are then
//! shown in the properties panel. Right-clicking the box (or its title) opens a menu to put a copy of the track above it or
//! below it, and to delete it.
//! Dragging the box up or down moves the track to another place in the list (see [`super::track_reordering`]),
//! and so does dragging the title up or down: dragging it along (sideways) selects the text, as it does in any
//! text box. The grip dots on the top of the box say that it can be dragged.
//!
//! The box is rounded at the corners at its left, and square at the two that face the strip: the box and the strip
//! meet with nothing notched between them.

use egui::{
    Button, CornerRadius, Rect, Response, Sense, Stroke, StrokeKind, TextEdit, Ui, UiBuilder, WidgetInfo, pos2,
};

use super::grip_dots::{self, Arrangement};
use crate::application::commands::Command;
use crate::application::selection::SelectionChange;
use crate::application::theme;
use crate::project::{Track, TrackSide};

/// How tall the box for the title is, in points.
pub const TITLE_HEIGHT: f32 = 26.0;

/// How far the title keeps from the sides of the box, in points.
const MARGIN: f32 = 8.0;

/// How far the grip dots keep from the top of the box, in points.
const DOTS_MARGIN_ABOVE: f32 = 3.0;

/// How much of the top of the box is the dots', which the title keeps below: the room above the dots, the dots, and a
/// little under them. (A test checks that they fit.)
const ROOM_FOR_THE_DOTS: f32 = 14.0;

/// How far the title keeps from the bottom of the box when the box is as short as it can be, in points.
const MARGIN_BELOW_THE_TITLE: f32 = 2.0;

/// How tall the box has to be to have the dots and the title in it, which is the least a track can be (that is
/// [`crate::project::TrackHeight::SMALLEST_IN_POINTS`], just more than this, which a test checks).
#[cfg(test)]
pub const HEIGHT_THE_BOX_NEEDS: f32 = ROOM_FOR_THE_DOTS + TITLE_HEIGHT + MARGIN_BELOW_THE_TITLE;

/// How round the corners of the box are, which are the two at its left: the two that face the strip are square.
const CORNERS: CornerRadius = CornerRadius {
    nw: 10,
    sw: 10,
    ne: 0,
    se: 0,
};

/// What the menu of the box of a track offers to delete.
pub const DELETE_TRACK_LABEL: &str = "Delete track";

/// What the menu of the box of a track offers to put a copy of the track in front of it (which is above it on the screen), and
/// behind it.
pub const DUPLICATE_ABOVE_LABEL: &str = "Duplicate above";
pub const DUPLICATE_BELOW_LABEL: &str = "Duplicate below";

/// The menu that opens when the box of `track` is right-clicked. What deletes the track is last, apart from the others, so that
/// it is not what a click that has hardly moved from the right-click chooses.
fn show_the_menu(ui: &mut Ui, track: &Track, can_delete: bool, commands: &mut Vec<Command>) {
    for (label, side) in [
        (DUPLICATE_ABOVE_LABEL, TrackSide::Above),
        (DUPLICATE_BELOW_LABEL, TrackSide::Below),
    ] {
        if ui.button(label).clicked() {
            commands.push(Command::DuplicateTrack {
                track: track.id(),
                side,
            });
            ui.close();
        }
    }
    ui.separator();
    let item = ui.add_enabled(can_delete, Button::new(DELETE_TRACK_LABEL));
    if item.clicked() {
        commands.push(Command::DeleteTrack(track.id()));
        ui.close();
    }
    if !can_delete {
        item.on_disabled_hover_text("A project always has a track");
    }
}

/// How a click on the box of `track` changes what is selected: the track is selected, and with Ctrl held it
/// is added to what is selected (or let go of if it was).
fn select_the_track(ui: &Ui, track: &Track, commands: &mut Vec<Command>) {
    let adding = ui.input(|input| input.modifiers.command);
    commands.push(Command::select_track(
        track.id(),
        if adding {
            SelectionChange::Toggle
        } else {
            SelectionChange::Replace
        },
    ));
}

/// What a track can be taken hold of by, to be dragged to another place in the list.
pub(super) struct Handles {
    /// The box, which is all of it but the title.
    pub(super) box_round_the_title: Response,
    /// The title, which is dragged along to select its text and up and down to move the track.
    pub(super) title: Response,
}

/// Draws the box of `track` in `header_rectangle` (`is_selected` says whether the track is selected), and adds
/// a command when it is clicked, when its title is typed in or when the track is deleted from the menu
/// (`can_delete` says whether it may be: not the last one). Gives what the track can be taken hold of by.
pub(super) fn show(
    ui: &mut Ui,
    header_rectangle: Rect,
    track: &Track,
    (can_edit, can_delete, is_selected): (bool, bool, bool),
    commands: &mut Vec<Command>,
) -> Handles {
    ui.painter()
        .rect_filled(header_rectangle, CORNERS, theme::TRACK_HEADER_BACKGROUND);
    grip_dots::draw(
        ui.painter(),
        Arrangement::FOR_A_TRACK_BOX,
        pos2(
            header_rectangle.center().x,
            header_rectangle.top() + DOTS_MARGIN_ABOVE + Arrangement::FOR_A_TRACK_BOX.size().y / 2.0,
        ),
    );
    // The whole box senses clicks, and being dragged (under the title, which is put over it).
    let background = ui.interact(
        header_rectangle,
        ui.id().with(("track header", track.id())),
        if can_edit {
            Sense::CLICK | Sense::DRAG
        } else {
            Sense::hover()
        },
    );
    background.context_menu(|ui| show_the_menu(ui, track, can_delete, commands));
    if background.clicked() {
        select_the_track(ui, track, commands);
    }
    // The title is in the middle of the box, as far down as the dots need it to be in a box that is short.
    let highest_middle = header_rectangle.top() + ROOM_FOR_THE_DOTS + TITLE_HEIGHT / 2.0;
    let lowest_middle = (header_rectangle.bottom() - MARGIN_BELOW_THE_TITLE - TITLE_HEIGHT / 2.0).max(highest_middle);
    let middle_y = header_rectangle.center().y.clamp(highest_middle, lowest_middle);

    let title_rectangle = Rect::from_min_max(
        pos2(header_rectangle.left() + MARGIN, middle_y - TITLE_HEIGHT / 2.0),
        pos2(header_rectangle.right() - MARGIN, middle_y + TITLE_HEIGHT / 2.0),
    );

    // (The box is made in a child of its own, so that putting it here does not move the place the
    // next track is laid out at, which `Ui::put` would. The box has a name that is the track's: otherwise its name,
    // which what is typed in it and what is being dragged are kept by, would be a number that is counted as the
    // rows are drawn, and the rows are not always drawn in the order they are in.)
    let mut title = track.title().to_owned();
    let mut title_ui = ui.new_child(
        UiBuilder::new()
            .id_salt(("title of track", track.id()))
            .max_rect(title_rectangle),
    );
    let title_response = title_ui.add(
        TextEdit::singleline(&mut title)
            .id_salt(("the title box of track", track.id()))
            .desired_width(title_rectangle.width())
            .interactive(can_edit),
    );
    title_response
        .widget_info(|| WidgetInfo::text_edit(can_edit, track.title(), title.clone(), "The title of the track"));
    title_response.context_menu(|ui| show_the_menu(ui, track, can_delete, commands));
    if title_response.clicked() {
        select_the_track(ui, track, commands);
    }
    if title != track.title() {
        commands.push(Command::SetTrackTitle {
            track: track.id(),
            title,
        });
    }

    // The outline goes over the title, as it does over a clip.
    if is_selected {
        ui.painter().rect_stroke(
            header_rectangle,
            CORNERS,
            Stroke::new(2.0, theme::SELECTION_OUTLINE),
            StrokeKind::Inside,
        );
    }
    Handles {
        box_round_the_title: background,
        title: title_response,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_room_kept_for_the_dots_at_the_top_of_the_box_is_enough_for_them_and_what_is_above_them() {
        assert!(
            ROOM_FOR_THE_DOTS >= DOTS_MARGIN_ABOVE + Arrangement::FOR_A_TRACK_BOX.size().y,
            "{ROOM_FOR_THE_DOTS} is not enough for {DOTS_MARGIN_ABOVE} and {:?}",
            Arrangement::FOR_A_TRACK_BOX.size()
        );
    }

    #[test]
    fn the_box_needs_the_dots_the_title_and_a_little_under_it() {
        assert_eq!(
            HEIGHT_THE_BOX_NEEDS,
            ROOM_FOR_THE_DOTS + TITLE_HEIGHT + MARGIN_BELOW_THE_TITLE
        );
    }
}
