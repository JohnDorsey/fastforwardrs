//! The strip of one track: its clips, and what the mouse does to them.
//!
//! * Clicking a clip selects it (with Ctrl held, it is added to what is selected or let go of), and
//!   clicking an empty part of the strip selects nothing.
//! * Dragging a clip moves it, and the other clips that are selected with it, along the track: as
//!   far as the pointer goes, or as far as the clips next to it leave room.
//! * Right-clicking a video or sound clip selects it and opens its settings box above it;
//!   right-clicking a text clip selects it and opens the window its text object is edited in.
//! * Double-clicking a text clip opens that window too.
//! * Right-clicking an empty part of the strip opens a menu with "New", which has "Text object".
//!
//! Cutting a track is done with a key (see the `keyboard` module), not with the mouse.

use egui::{CursorIcon, PointerButton, Sense, Ui, WidgetInfo, WidgetType, pos2};

use super::clips::{self, rectangles_of};
use super::settings_boxes::SettingsBoxSubject;
use super::strip_view::StripView;
use super::view_state::ClipDrag;
use super::{NewMenuTarget, TimelineContent, TimelineViewState};
use crate::application::clicks::is_a_double_click;
use crate::application::clip_moving::{allowed_move_in_frames, starts_after_moving_by};
use crate::application::commands::Command;
use crate::application::selection::SelectionChange;
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

/// The command that makes `clip` the only clip that is selected.
fn select_only(clip: ClipLocation) -> Command {
    Command::Select {
        clips: vec![clip],
        change: SelectionChange::Replace,
    }
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
            if content.can_edit {
                Sense::CLICK | Sense::DRAG
            } else {
                Sense::hover()
            },
        );
        response.widget_info(|| {
            WidgetInfo::labeled(
                WidgetType::Other,
                content.can_edit,
                clip_label(track.title(), placement.id),
            )
        });
        let is_text = matches!(placement.clip, Clip::Text(_));
        let adding_to_the_selection = ui.input(|input| input.modifiers.command);
        if response.secondary_clicked() {
            // Right-clicking a clip that is one of several selected ones leaves them selected.
            if !content.selection.contains(location) {
                commands.push(select_only(location));
            }
            if is_text {
                commands.push(Command::EditTextObject(location));
            } else {
                state.open_settings_box(SettingsBoxSubject::Clip(location));
            }
        } else if is_a_double_click(&response) && is_text {
            commands.push(select_only(location));
            commands.push(Command::EditTextObject(location));
        } else if response.clicked() {
            commands.push(if adding_to_the_selection {
                Command::Select {
                    clips: vec![location],
                    change: SelectionChange::Toggle,
                }
            } else {
                select_only(location)
            });
        }
        if content.can_edit {
            drag_the_clip(ui, view, content, state, (location, &placement), &response, commands);
        }
        // The pointer of its settings box touches the middle of the top edge of what can be seen of the clip.
        state.point_settings_box_at(
            SettingsBoxSubject::Clip(location),
            pos2(rectangles.visible.center().x, rectangles.visible.top()),
        );
    }
    // The outlines go over everything, so that the clips beside a selected one cannot cover them.
    for selected in content.selection.iter().filter(|selected| selected.track == track.id()) {
        if let Some(placement) = track.placement_of(selected.clip) {
            let rectangles = rectangles_of(&placement, view);
            if rectangles.visible.width() > 0.0 {
                clips::draw_selection_outline(&painter, rectangles);
            }
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
}

/// Moves the clips that are being dragged when `response` (the response of `clip`) is being dragged:
/// takes hold of them when the drag starts (selecting the clip first if it was not selected), and
/// from then on keeps the clip where the pointer is, as far as the other clips allow.
fn drag_the_clip(
    ui: &Ui,
    view: &StripView,
    content: &TimelineContent<'_>,
    state: &mut TimelineViewState,
    (location, placement): (ClipLocation, &crate::project::ClipPlacement<'_>),
    response: &egui::Response,
    commands: &mut Vec<Command>,
) {
    let frame_under = |pointer: egui::Pos2| view.frame_at(pointer.x).map_or(0, |frame| frame.value());
    let adding_to_the_selection = ui.input(|input| input.modifiers.command);
    // A drag starts once the pointer has gone a little way from where it was pressed, which is where
    // the clip was taken hold of.
    if response.drag_started_by(PointerButton::Primary)
        && let Some(pointer) = ui.input(|input| input.pointer.press_origin())
    {
        let moving = if content.selection.contains(location) {
            content.selection.to_vec()
        } else {
            commands.push(if adding_to_the_selection {
                Command::Select {
                    clips: vec![location],
                    change: SelectionChange::Toggle,
                }
            } else {
                select_only(location)
            });
            let mut moving = if adding_to_the_selection {
                content.selection.to_vec()
            } else {
                Vec::new()
            };
            moving.push(location);
            moving
        };
        state.clip_drag = Some(ClipDrag {
            moving,
            grabbed: location,
            pointer_frame_at_the_start: frame_under(pointer),
            grabbed_start_at_the_start: placement.first_timeline_frame.value(),
        });
    }
    if response.dragged_by(PointerButton::Primary)
        && let Some(drag) = state.clip_drag.as_ref().filter(|drag| drag.grabbed == location)
        && let Some(pointer) = ui.input(|input| input.pointer.interact_pos())
    {
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        let frames_the_pointer_has_gone = frame_under(pointer) as i64 - drag.pointer_frame_at_the_start as i64;
        let frames_the_clip_has_gone =
            placement.first_timeline_frame.value() as i64 - drag.grabbed_start_at_the_start as i64;
        let frames_to_go_now = allowed_move_in_frames(
            content.project,
            &drag.moving,
            frames_the_pointer_has_gone - frames_the_clip_has_gone,
        );
        if frames_to_go_now != 0 {
            commands.push(Command::MoveClips(starts_after_moving_by(
                content.project,
                &drag.moving,
                frames_to_go_now,
            )));
        }
    }
    if response.drag_stopped() {
        state.clip_drag = None;
    }
}
