//! The box at the left of each track: the track's title, which can be changed by typing in it. Clicking the
//! box (or its title) selects the track, which is not the clips on it: the settings of the track are then
//! shown in the properties panel. Right-clicking the box (or its title) opens a menu with Delete track.
//! Dragging the box up or down moves the track to another place in the list (see [`super::track_reordering`]),
//! and so does dragging the title up or down: dragging it along (sideways) selects the text, as it does in any
//! text box.

use egui::{
    Button, CornerRadius, Rect, Response, Sense, Stroke, StrokeKind, TextEdit, Ui, UiBuilder, WidgetInfo, pos2,
};

use crate::application::commands::Command;
use crate::application::selection::SelectionChange;
use crate::application::theme;
use crate::project::Track;

/// How tall the box for the title is, in points. (The least a track can be is just taller than this, which a
/// test checks.)
pub const TITLE_HEIGHT: f32 = 26.0;

/// How far the title keeps from the edges of the box, in points.
const MARGIN: f32 = 8.0;

/// What the menu of the box of a track offers to delete.
pub const DELETE_TRACK_LABEL: &str = "Delete track";

/// The menu that opens when the box of `track` is right-clicked.
fn show_the_menu(ui: &mut Ui, track: &Track, can_delete: bool, commands: &mut Vec<Command>) {
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
    commands.push(Command::SelectTrack {
        track: track.id(),
        change: if adding {
            SelectionChange::Toggle
        } else {
            SelectionChange::Replace
        },
    });
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
        .rect_filled(header_rectangle, CornerRadius::same(10), theme::TRACK_HEADER_BACKGROUND);
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
    let middle_y = header_rectangle.center().y;

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
            CornerRadius::same(10),
            Stroke::new(2.0, theme::SELECTION_OUTLINE),
            StrokeKind::Inside,
        );
    }
    Handles {
        box_round_the_title: background,
        title: title_response,
    }
}
