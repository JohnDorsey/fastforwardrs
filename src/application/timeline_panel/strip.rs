//! The strip of one track: its clips, and what the mouse does to them.
//!
//! * Clicking a clip selects it, and clicking an empty part of the strip selects nothing.
//! * Right-clicking a video or sound clip selects it and opens its settings box above it;
//!   right-clicking a text clip selects it and opens the window its text object is edited in.
//! * Double-clicking a text clip opens that window too.
//! * Right-clicking an empty part of the strip opens a menu with "New", which has "Text object".
//!
//! Cutting a track is done with a key (see the `keyboard` module), not with the mouse.

use egui::{Sense, Ui, WidgetInfo, WidgetType, pos2};

use super::clips::{self, rectangles_of};
use super::settings_boxes::SettingsBoxSubject;
use super::strip_view::StripView;
use super::{NewMenuTarget, TimelineContent, TimelineViewState};
use crate::application::clicks::is_a_double_click;
use crate::application::commands::Command;
use crate::application::textures::Textures;
use crate::project::{Clip, ClipLocation, Track};

/// The name the strip of a track is known by to screen readers (and the tests): it says which track.
pub fn strip_label(track_title: &str) -> String {
    format!("Strip of {track_title}")
}

/// The name a clip is known by: its number on its track, and which track.
pub fn clip_label(track_title: &str, clip: crate::project::ClipId) -> String {
    format!("Clip {} of {track_title}", clip.value())
}

/// Draws the strip of `track` where `view` is, and adds a command for each thing the user does to it.
pub(super) fn show(
    ui: &mut Ui,
    view: &StripView,
    track: &Track,
    content: &TimelineContent<'_>,
    state: &mut TimelineViewState,
    textures: &mut Textures,
    commands: &mut Vec<Command>,
) {
    let strip_rectangle = view.rectangle;
    let strip_id = ui.id().with(("track strip", track.id()));
    // Not focusable, so that the keys of the keyboard keep their meanings as shortcuts of the window.
    let background = ui.interact(
        strip_rectangle,
        strip_id,
        if content.can_edit { Sense::CLICK } else { Sense::hover() },
    );
    background.widget_info(|| WidgetInfo::labeled(WidgetType::Other, content.can_edit, strip_label(track.title())));

    let painter = ui.painter_at(strip_rectangle);
    clips::draw_background(&painter, strip_rectangle);
    clips::draw_clips(ui, &painter, track, content, view, textures);
    clips::draw_clip_boundaries(&painter, view, track);

    for placement in track.placements() {
        let rectangles = rectangles_of(&placement, view);
        if rectangles.visible.width() <= 0.0 {
            continue;
        }
        let location = ClipLocation {
            track: track.id(),
            clip: placement.id,
        };
        let response = ui.interact(
            rectangles.visible,
            strip_id.with(("clip", placement.id)),
            if content.can_edit { Sense::CLICK } else { Sense::hover() },
        );
        response.widget_info(|| {
            WidgetInfo::labeled(
                WidgetType::Other,
                content.can_edit,
                clip_label(track.title(), placement.id),
            )
        });
        let is_text = matches!(placement.clip, Clip::Text(_));
        if response.secondary_clicked() {
            commands.push(Command::SelectClip(location));
            if is_text {
                commands.push(Command::EditTextObject(location));
            } else {
                state.open_settings_box(SettingsBoxSubject::Clip(location));
            }
        } else if is_a_double_click(&response) && is_text {
            commands.push(Command::SelectClip(location));
            commands.push(Command::EditTextObject(location));
        } else if response.clicked() {
            commands.push(Command::SelectClip(location));
        }
        // The pointer of its settings box touches the middle of the top edge of what can be seen of the clip.
        state.point_settings_box_at(
            SettingsBoxSubject::Clip(location),
            pos2(rectangles.visible.center().x, rectangles.visible.top()),
        );
    }
    // The outline goes over everything, so that the clips beside the selected one cannot cover it.
    if let Some(selected) = content.selected_clip.filter(|selected| selected.track == track.id())
        && let Some(placement) = track.placement_of(selected.clip)
    {
        let rectangles = rectangles_of(&placement, view);
        if rectangles.visible.width() > 0.0 {
            clips::draw_selection_outline(&painter, rectangles);
        }
    }

    if content.can_edit {
        if background.clicked() {
            commands.push(Command::Deselect);
        }
        if background.secondary_clicked()
            && let Some(pointer) = background.interact_pointer_pos()
        {
            state.new_menu_target = view.frame_at(pointer.x).map(|frame| NewMenuTarget {
                track: track.id(),
                frame,
            });
        }
        background.context_menu(|ui| {
            ui.menu_button("New", |ui| {
                if ui.button("Text object").clicked() {
                    if let Some(target) = state.new_menu_target {
                        commands.push(Command::NewTextObject {
                            track: target.track,
                            frame: target.frame,
                        });
                    }
                    ui.close();
                }
            });
        });
    }
    clips::draw_border(&painter, strip_rectangle);
}
