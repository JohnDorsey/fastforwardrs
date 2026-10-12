//! The strip of one track: its clips, and what the mouse does to them.
//!
//! * Clicking a clip selects it (with Ctrl held, it is added to what is selected or let go of), and
//!   clicking an empty part of the strip selects nothing.
//! * Dragging a clip moves it, and the other clips that are selected with it, along the track and to
//!   the other tracks: as far as the pointer goes, or as far as the clips next to it leave room (taking
//!   hold of it is here, the moving is in the `clip_dragging` module).
//! * Right-clicking a clip selects it (unless it is one of the clips that are selected already), and the
//!   settings of what is selected are in the properties panel.
//! * Right-clicking an empty part of the strip opens a menu with "New", which has "Text object".
//!
//! Cutting a track is done with a key (see the `keyboard` module), not with the mouse.

use egui::{PointerButton, Rect, Sense, Ui, WidgetInfo, WidgetType, pos2};

use super::clip_shape::ClipShape;
use super::clips::{self, rectangles_of};
use super::drawers::{self, drawer_button_label};
use super::strip_view::StripView;
use super::view_state::ClipDrag;
use super::{NewMenuTarget, TimelineContent, TimelineViewState};
use crate::application::clip_moving::StartTimeMemory;
use crate::application::commands::Command;
use crate::application::selection::SelectionChange;
use crate::application::textures::Textures;
use crate::application::theme;
use crate::project::{ClipId, Track, TrackId};

/// The name the strip of a track is known by to screen readers (and the tests): it says which track.
pub fn strip_label(track_title: &str) -> String {
    format!("Strip of {track_title}")
}

/// The name a clip is known by: its number on its track, and which track.
pub fn clip_label(track_title: &str, clip: crate::project::ClipId) -> String {
    format!("Clip {} of {track_title}", clip.number())
}

/// The command that makes `clip` the only clip that is selected.
fn select_only(clip: ClipId) -> Command {
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
    draw_the_drawers(ui, view, track, state);
    clips::draw_clips(ui, &painter, track, content, view, textures);
    clips::draw_clip_boundaries(&painter, view, track);

    for placement in track.placements() {
        let rectangles = rectangles_of(&placement, view);
        if rectangles.visible.width() <= 0.0 {
            continue;
        }
        let location = placement.id;
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
        let adding_to_the_selection = ui.input(|input| input.modifiers.command);
        if response.secondary_clicked() {
            // Right-clicking a clip that is one of several selected ones leaves them selected.
            if !content.selection.contains(location) {
                commands.push(select_only(location));
            }
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
            drag_the_clip(
                ui,
                view,
                content,
                state,
                (track.id(), location, &placement),
                &response,
                commands,
            );
        }
        operate_the_drawer(ui, &painter, view, (track, &placement, &rectangles), state);
    }
    // The outlines go over everything, so that the clips beside a selected one cannot cover them.
    for selected in content.selection.iter() {
        if let Some(placement) = track.placement_of(selected) {
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

/// Draws the drawers of the clips of `track` that are open, which are behind the clips (they reach up behind them
/// a little: see [`drawers::drawer_shape`]) and hang below the strip.
fn draw_the_drawers(ui: &Ui, view: &StripView, track: &Track, state: &TimelineViewState) {
    let strip = view.rectangle;
    let room_for_drawers = Rect::from_min_max(
        strip.left_top(),
        pos2(strip.right(), strip.bottom() + drawers::DRAWER_HEIGHT),
    );
    let painter = ui.painter_at(room_for_drawers);
    for placement in track.placements() {
        let rectangles = rectangles_of(&placement, view);
        if rectangles.visible.width() <= 0.0 || !drawers::has_an_accessible_drawer(placement.clip) {
            continue;
        }
        let radius = ClipShape::new(rectangles.whole, f32::from(theme::CLIP_CORNER_RADIUS)).radius();
        if let Some(shape) = drawers::drawer_shape(rectangles.whole, radius, state.drawers.openness(placement.id)) {
            drawers::draw_drawer(&painter, shape);
        }
    }
}

/// The button over the bottom edge of a clip that opens and shuts its drawer, and the drawer's own room, which
/// takes what is done in it (nothing, for now) and so is not an empty place of the strip.
fn operate_the_drawer(
    ui: &Ui,
    painter: &egui::Painter,
    view: &StripView,
    (track, placement, rectangles): (&Track, &crate::project::ClipPlacement<'_>, &clips::ClipRectangles),
    state: &mut TimelineViewState,
) {
    if !drawers::has_an_accessible_drawer(placement.clip) {
        return;
    }
    let drawer_id = ui.id().with(("drawer", placement.id));
    let openness = state.drawers.openness(placement.id);
    if openness > 0.0 {
        let strip = view.rectangle;
        let hanging = Rect::from_min_max(
            pos2(rectangles.visible.left(), strip.bottom()),
            pos2(
                rectangles.visible.right(),
                strip.bottom() + drawers::DRAWER_HEIGHT * openness,
            ),
        );
        ui.interact(hanging, drawer_id.with("room"), Sense::CLICK | Sense::DRAG);
    }
    let Some(button) = drawers::button_rectangle(rectangles.visible) else {
        return;
    };
    let response = ui.interact(button, drawer_id.with("button"), Sense::CLICK | Sense::DRAG);
    response.widget_info(|| {
        WidgetInfo::labeled(
            WidgetType::Button,
            true,
            drawer_button_label(track.title(), placement.id),
        )
    });
    if response.clicked() {
        state.drawers.toggle(placement.id);
    }
    drawers::draw_button(painter, button, state.drawers.is_open(placement.id), response.hovered());
}

/// Takes hold of the clips that are to be dragged when `response` (the response of `clip`) begins to
/// be dragged: the selected clips if it is one of them, and otherwise it alone (which is selected, with
/// Ctrl added to what was selected). Where the clips are moved to after that is
/// [`super::clip_dragging`]'s.
fn drag_the_clip(
    ui: &Ui,
    view: &StripView,
    content: &TimelineContent<'_>,
    state: &mut TimelineViewState,
    (track, location, placement): (TrackId, ClipId, &crate::project::ClipPlacement<'_>),
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
            start_time: StartTimeMemory::new(placement.first_timeline_frame.value(), track),
        });
    }
}
