//! The timeline: a top row with the ruler, and below it a row for every track, the one in front at
//! the top.
//!
//! A row has, at its left, a box with the track's title and its settings gear, and then the strip
//! of the track, which has the clips on it. The strips are wider than the window, and scroll
//! (together) to keep the current frame in view. In the top row, over the title boxes, there is a
//! round button that adds a track. With more tracks than fit, the rows scroll up and down.
//!
//! * Clicking or dragging along the ruler moves the playhead there.
//! * Clicking a clip selects it. Right-clicking a clip opens a box above it with its settings.
//!   Right-clicking an empty part of a strip opens a menu to put something new there.
//! * The gear of a track opens a box above it with the settings of everything on the track.
//! * The mouse wheel (or a trackpad) scrolls the strips without moving the current frame.
//!
//! Cutting is done with a key (see the `keyboard` module), not with the mouse.

mod bevel;
mod clips;
mod header;
mod icons;
mod playhead;
mod ruler;
mod scrolling;
mod settings_boxes;
mod strip;
mod strip_view;
mod top_row;
mod track_resize;
mod view_state;

use std::collections::HashMap;

use egui::scroll_area::{DragScroll, ScrollSource};
use egui::{Id, PointerButton, Rect, ScrollArea, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

// The names the parts of the timeline are known by to screen readers, which the tests look them up by.
#[cfg(test)]
pub use header::{CONTENT_HEIGHT as HEIGHT_THE_TITLE_AND_GEAR_NEED, gear_label};
#[cfg(test)]
pub use settings_boxes::SettingsBoxSubject;
#[cfg(test)]
pub use strip::strip_label;
pub use view_state::TimelineViewState;

/// What the empty room of the timeline, which a rectangle is drawn in to select the clips it goes
/// round, is known by.
pub fn rows_area_id() -> Id {
    Id::new("the empty room of the timeline")
}

use self::clips::rectangles_of;
use self::scrolling::update_view;
use self::view_state::{NewMenuTarget, RubberBand};
use super::commands::Command;
use super::selection::Selection;
use super::textures::Textures;
use super::theme;
use super::timeline_geometry::TimelineGeometry;
use crate::media::{ThumbnailLibrary, Waveform};
use crate::project::{ClipLocation, Project, SourceMediaIdentifier, TimelineFrameIndex, Track, TrackHeight};
use crate::rgba_picture::RgbaPicture;

/// The name the button that adds a track is known by to screen readers (and the tests).
pub const ADD_TRACK_LABEL: &str = "Add a track";

/// The name the ruler is known by.
pub const RULER_LABEL: &str = "Ruler";

/// What the timeline draws of a source file.
pub struct SourceLook<'a> {
    /// Whether the file can be used. The clips made from one that cannot are drawn as missing.
    pub is_available: bool,
    /// The pictures for a video's clips.
    pub thumbnails: Option<&'a ThumbnailLibrary>,
    /// How many pixels wide and tall the pictures of a video, or a picture file, are, which is the
    /// shape its clips' pictures are drawn in.
    pub picture_size_in_pixels: (u32, u32),
    /// The picture of an image file, for its clips, once it is decoded.
    pub picture: Option<&'a RgbaPicture>,
    /// The shape of the sound of a sound file, for its clips.
    pub waveform: Option<&'a Waveform>,
    /// The file's name, to say which file a clip is from.
    pub file_name: String,
}

/// What the timeline shows.
pub struct TimelineContent<'a> {
    pub project: &'a Project,
    /// The scale the strips start at, which the user can zoom in and out from (see [`TimelineViewState`]).
    pub geometry: TimelineGeometry,
    pub current_frame: TimelineFrameIndex,
    pub selection: &'a Selection,
    /// What to draw of each of the project's source files.
    pub source_looks: &'a HashMap<SourceMediaIdentifier, SourceLook<'a>>,
    /// Whether clicking and dragging may change the project or move the playhead.
    pub can_edit: bool,
}

/// How tall the rows can be together before they scroll, in points: as many rows of the normal
/// height as are shown at once. (However tall the rows are made, they never take up more of the
/// window than that.)
fn tallest_rows_area() -> f32 {
    let rows = theme::MOST_TRACKS_SHOWN_AT_ONCE as f32;
    rows * TrackHeight::DEFAULT_IN_POINTS + (rows - 1.0) * theme::SPACE_BETWEEN_TRACKS
}

/// Draws the ruler, the rows and the button under them, and adds a command for each thing the
/// user does to them.
pub fn show(
    ui: &mut Ui,
    content: &TimelineContent<'_>,
    state: &mut TimelineViewState,
    textures: &mut Textures,
    commands: &mut Vec<Command>,
) {
    let context = ui.ctx().clone();
    state.close_settings_box_if_pressed_outside(&context);
    if let Some(open) = state.settings_box.as_mut() {
        // Where it points is worked out again, from where its subject is drawn in this frame.
        open.anchor = None;
    }

    let width = ui.available_width();
    let strips_offset = theme::TRACK_HEADER_WIDTH + theme::TRACK_HEADER_GAP;
    let strips_width = (width - strips_offset - theme::SCROLL_BAR_ROOM).max(1.0);
    let view_width = f64::from(strips_width);
    let geometry = update_view(ui, content, state, view_width);

    // The row above the tracks has the ruler (over the strips) and the round button that adds a
    // track (over the title boxes), and sits right on top of the first row, with nothing between them.
    let previous_item_spacing = ui.spacing().item_spacing.y;
    ui.spacing_mut().item_spacing.y = 0.0;
    let top_row = top_row::show(ui, content, state, geometry, (strips_offset, strips_width), commands);
    let ruler_rectangle = top_row.ruler_rectangle;
    let ruler_view = top_row.ruler_view;

    // Under all the strips (and the gaps between them), so that the clips and the strips are over it,
    // is what a rectangle is drawn on to select the clips it goes round, from any empty place.
    let rows_top = top_row.rectangle.bottom();
    let rows_area = Rect::from_min_max(
        pos2(ruler_rectangle.left(), rows_top),
        pos2(ruler_rectangle.right(), rows_top + state.rows_area_height),
    );
    // (The constants, and not `Sense::click_and_drag()`, which can take the keyboard focus: then Tab
    // would stop at this, which cannot be seen, and Space would deselect.)
    let rows_response = ui.interact(
        rows_area,
        rows_area_id(),
        if content.can_edit {
            Sense::CLICK | Sense::DRAG
        } else {
            Sense::hover()
        },
    );
    let mut strip_views: Vec<(&Track, self::strip_view::StripView)> = Vec::new();

    // While a track is being made taller or shorter the rows keep the height they had, however tall
    // they would be: the window is at the bottom of the screen and grows upward, so if it grew with
    // the rows the boundary being dragged would stay where it is on the screen while the pointer
    // went on without it. (When the button is let go the rows are as tall as they are.)
    let height_while_resizing = state.track_resize.map(|resize| resize.rows_area_height);
    // The scroll bar of the rows is a thin one that stays in sight (in the room left for it at the
    // right) while there are more rows than fit, so that it is clear that there are more.
    let rows_scope = ui.scope(|ui| {
        let scroll_bar = &mut ui.spacing_mut().scroll;
        scroll_bar.dormant_handle_opacity = 0.55;
        scroll_bar.dormant_background_opacity = 0.25;
        scroll_bar.foreground_color = true;
        ScrollArea::vertical()
            .id_salt("timeline tracks")
            .max_height(height_while_resizing.unwrap_or_else(tallest_rows_area))
            .auto_shrink([false, height_while_resizing.is_none()])
            .vertical_scroll_offset(state.rows_scroll_offset)
            .scroll_source(ScrollSource {
                scroll_bar: true,
                drag: DragScroll::Never,
                // The mouse wheel scrolls the strips along instead (and, with Alt, up and down
                // by the code that follows this).
                mouse_wheel: false,
            })
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = theme::SPACE_BETWEEN_TRACKS;
                let rows_area_height = state.rows_area_height;
                for track in content.project.tracks() {
                    let height = track.height().points();
                    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
                    let header_rectangle = Rect::from_min_size(row.min, vec2(theme::TRACK_HEADER_WIDTH, height));
                    let strip_rectangle =
                        Rect::from_min_size(pos2(row.left() + strips_offset, row.top()), vec2(strips_width, height));
                    let can_delete = content.project.tracks().len() > 1;
                    header::show(
                        ui,
                        header_rectangle,
                        track,
                        (content.can_edit, can_delete),
                        state,
                        commands,
                    );
                    let strip_view = ruler_view.shown_in(strip_rectangle);
                    strip::show(ui, &strip_view, track, content, state, textures, commands);
                    strip_views.push((track, strip_view));
                    track_resize::show(
                        ui,
                        header_rectangle,
                        track,
                        content.can_edit,
                        rows_area_height,
                        state,
                        commands,
                    );
                }
                // Room under the last row (after the space there is between the rows) for the boundary
                // under the last track to be taken hold of: without anything after the last row the
                // rows would end where it does.
                ui.add_space(track_resize::REACH_INTO_THE_BOXES);
            })
    });
    let rows = rows_scope.inner;
    ui.spacing_mut().item_spacing.y = previous_item_spacing;
    if content.can_edit {
        draw_a_rubber_band(ui, &rows_response, rows_area, &strip_views, content, state, commands);
    }

    let strips_rectangle = Rect::from_min_max(
        pos2(ruler_rectangle.left(), ruler_rectangle.bottom()),
        pos2(ruler_rectangle.right(), rows.inner_rect.bottom()),
    );
    state.strips_area_in_the_last_frame = Some(ruler_rectangle.union(strips_rectangle));
    state.timeline_area_in_the_last_frame = Some(top_row.rectangle.union(rows.inner_rect));
    state.rows_scroll_offset = rows.state.offset.y;
    state.rows_area_height = rows_scope.response.rect.height();
    playhead::draw(
        ui.painter(),
        &ruler_view.shown_in(strips_rectangle),
        ruler_rectangle,
        content.current_frame,
    );

    settings_boxes::show(&context, content, state, commands);
    if !context.input(|input| input.pointer.any_down()) {
        state.settings_box_closed_by_the_press = None;
        state.track_resize = None;
    }
}

/// Selects the clips that a rectangle goes round while it is dragged from an empty place, and
/// draws it. (With Ctrl held when it was begun, what was selected stays selected.) Clicking an empty
/// place selects nothing.
fn draw_a_rubber_band(
    ui: &mut Ui,
    response: &egui::Response,
    rows_area: Rect,
    strip_views: &[(&Track, self::strip_view::StripView)],
    content: &TimelineContent<'_>,
    state: &mut TimelineViewState,
    commands: &mut Vec<Command>,
) {
    if response.clicked() {
        commands.push(Command::Deselect);
    }
    if response.drag_started_by(PointerButton::Primary)
        && let Some(origin) = ui.input(|input| input.pointer.press_origin())
    {
        let adding = ui.input(|input| input.modifiers.command);
        state.rubber_band = Some(RubberBand {
            origin,
            kept: if adding { content.selection.to_vec() } else { Vec::new() },
        });
    }
    if response.dragged_by(PointerButton::Primary)
        && let Some(band) = state.rubber_band.as_ref()
        && let Some(pointer) = ui.input(|input| input.pointer.interact_pos())
    {
        let rectangle = Rect::from_two_pos(band.origin, pointer);
        let mut selected = band.kept.clone();
        for (track, view) in strip_views {
            for placement in track.placements() {
                let visible = rectangles_of(&placement, view).visible;
                let location = ClipLocation {
                    track: track.id(),
                    clip: placement.id,
                };
                if visible.width() > 0.0 && visible.intersects(rectangle) && !selected.contains(&location) {
                    selected.push(location);
                }
            }
        }
        commands.push(Command::Select {
            clips: selected,
            change: crate::application::selection::SelectionChange::Replace,
        });
        let painter = ui.painter().with_clip_rect(rows_area);
        painter.rect_filled(rectangle, 0.0, theme::RUBBER_BAND_FILL);
        painter.rect_stroke(
            rectangle,
            0.0,
            Stroke::new(1.0, theme::SELECTION_OUTLINE),
            StrokeKind::Inside,
        );
    }
    if response.drag_stopped() {
        state.rubber_band = None;
    }
}
