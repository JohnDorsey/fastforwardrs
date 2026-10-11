//! The box at the left of each track: the track's title, which can be changed by typing in it, and
//! the settings gear that opens the box with the settings of everything on the track. Right-clicking
//! the box (or its title) opens a menu with Delete track.

use egui::{Button, CornerRadius, Rect, Sense, TextEdit, Ui, UiBuilder, WidgetInfo, WidgetType, pos2, vec2};

use super::TimelineViewState;
use super::icons::draw_gear;
use super::settings_boxes::SettingsBoxSubject;
use crate::application::commands::Command;
use crate::application::theme;
use crate::project::Track;

/// How big the gear's button is, in points.
const GEAR_BUTTON_SIZE: f32 = 28.0;

/// How tall the box for the title is, in points.
const TITLE_HEIGHT: f32 = 26.0;

/// How far the title and the gear keep from the edges of the box, in points.
const MARGIN: f32 = 8.0;

/// How tall the box has to be to hold the title and the gear: the taller of the two. (The least a
/// track can be is just taller than this, which a test checks.)
#[cfg(test)]
pub const CONTENT_HEIGHT: f32 = if GEAR_BUTTON_SIZE > TITLE_HEIGHT {
    GEAR_BUTTON_SIZE
} else {
    TITLE_HEIGHT
};

/// The name the gear's button is known by to screen readers (and the tests): it says which track.
pub fn gear_label(track_title: &str) -> String {
    format!("Settings of {track_title}")
}

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

/// Draws the box of `track` in `header_rectangle`, and adds a command when its title is typed in or
/// the track is deleted from the menu (`can_delete` says whether it may be: not the last one).
pub(super) fn show(
    ui: &mut Ui,
    header_rectangle: Rect,
    track: &Track,
    (can_edit, can_delete): (bool, bool),
    state: &mut TimelineViewState,
    commands: &mut Vec<Command>,
) {
    ui.painter()
        .rect_filled(header_rectangle, CornerRadius::same(10), theme::TRACK_HEADER_BACKGROUND);
    // The whole box senses the right button (under the title and the gear, which are put over it).
    // It takes no clicks of the left: it is only there for the menu.
    let background = ui.interact(
        header_rectangle,
        ui.id().with(("track header", track.id())),
        if can_edit { Sense::CLICK } else { Sense::hover() },
    );
    background.context_menu(|ui| show_the_menu(ui, track, can_delete, commands));
    let middle_y = header_rectangle.center().y;

    let gear_rectangle = Rect::from_center_size(
        pos2(header_rectangle.right() - MARGIN - GEAR_BUTTON_SIZE / 2.0, middle_y),
        vec2(GEAR_BUTTON_SIZE, GEAR_BUTTON_SIZE),
    );
    let title_rectangle = Rect::from_min_max(
        pos2(header_rectangle.left() + MARGIN, middle_y - TITLE_HEIGHT / 2.0),
        pos2(gear_rectangle.left() - MARGIN / 2.0, middle_y + TITLE_HEIGHT / 2.0),
    );

    // (The box is made in a child of its own, so that putting it here does not move the place the
    // next track is laid out at, which `Ui::put` would.)
    let mut title = track.title().to_owned();
    let mut title_ui = ui.new_child(UiBuilder::new().max_rect(title_rectangle));
    let title_response = title_ui.add(
        TextEdit::singleline(&mut title)
            .desired_width(title_rectangle.width())
            .interactive(can_edit),
    );
    title_response
        .widget_info(|| WidgetInfo::text_edit(can_edit, track.title(), title.clone(), "The title of the track"));
    title_response.context_menu(|ui| show_the_menu(ui, track, can_delete, commands));
    if title != track.title() {
        commands.push(Command::SetTrackTitle {
            track: track.id(),
            title,
        });
    }

    // Not focusable, so that the keys of the keyboard keep their meanings as shortcuts of the window.
    let gear = ui.interact(
        gear_rectangle,
        ui.id().with(("track settings gear", track.id())),
        if can_edit { Sense::CLICK } else { Sense::hover() },
    );
    gear.widget_info(|| WidgetInfo::labeled(WidgetType::Button, can_edit, gear_label(track.title())));
    let subject = SettingsBoxSubject::Track(track.id());
    if gear.clicked() {
        state.open_settings_box(subject);
    }
    let box_is_open = state.settings_box_subject() == Some(subject);
    if box_is_open {
        state.point_settings_box_at(subject, gear_rectangle.center_top());
    }
    if gear.hovered() || box_is_open {
        ui.painter()
            .rect_filled(gear_rectangle, CornerRadius::same(6), theme::ICON_BUTTON_HOVER);
    }
    draw_gear(
        ui.painter(),
        gear_rectangle.center(),
        GEAR_BUTTON_SIZE * 0.36,
        theme::ICON,
        if gear.hovered() || box_is_open {
            theme::ICON_BUTTON_HOVER
        } else {
            theme::TRACK_HEADER_BACKGROUND
        },
    );
    gear.on_hover_text("Track settings");
}
