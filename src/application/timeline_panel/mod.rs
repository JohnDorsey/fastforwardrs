//! The timeline: a top row with the ruler, and below it a row for every track, the one in front at
//! the top.
//!
//! A row has, at its left, a box with the track's title, and then the strip
//! of the track, which has the clips on it. The strips are wider than the window, and scroll
//! (together) to keep the current frame in view. In the top row, over the title boxes, there is a
//! round button that adds a track. With more tracks than fit, the rows scroll up and down.
//!
//! * Clicking or dragging along the ruler moves the playhead there.
//! * Clicking a clip selects it, and so does right-clicking it (what is selected has its settings in the
//!   properties panel). Right-clicking an empty part of a strip opens a menu to put something new there.
//! * Clicking the box of a track selects the track.
//! * The mouse wheel (or a trackpad) scrolls the strips without moving the current frame.
//!
//! Cutting is done with a key (see the `keyboard` module), not with the mouse.

mod bevel;
mod clip_menu;
mod clip_shape;
mod clips;
mod drawers;
mod flash;
mod grip_dots;
mod header;
mod item_dragging;
mod playhead;
mod rack_dragging;
mod rack_look;
mod rack_menus;
mod racks;
mod rows;
mod ruler;
mod scrolling;
mod strip;
mod strip_view;
mod top_row;
mod track_reordering;
mod track_resize;
mod view_state;

use std::collections::HashMap;

use egui::scroll_area::{DragScroll, ScrollSource};
use egui::{Id, PointerButton, Rect, ScrollArea, Sense, Stroke, StrokeKind, Ui, pos2};

// The names the parts of the timeline are known by to screen readers, which the tests look them up by.
#[cfg(test)]
pub use drawers::{drawer_button_label, track_drawer_button_label};
#[cfg(test)]
pub use grip_dots::{Arrangement as GripDotArrangement, VERTICES_PER_DOT as VERTICES_IN_A_GRIP_DOT};
#[cfg(test)]
pub use header::{HEIGHT_THE_BOX_NEEDS, TITLE_HEIGHT as HEIGHT_OF_THE_ROOM_FOR_A_TITLE};
#[cfg(test)]
pub use rack_dragging::NOT_ENOUGH_SPACE_MESSAGE;
pub use rack_look::grain_image as grain_of_the_racks;
#[cfg(test)]
pub use racks::keyframe_label;
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
use super::selection::{Selection, SelectionItem};
use super::textures::Textures;
use super::theme;
use super::timeline_geometry::TimelineGeometry;
use crate::media::{ThumbnailLibrary, Waveform};
use crate::project::{Project, SourceMediaIdentifier, TimelineFrameIndex, Track, TrackHeight};
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
    // The flashes at cuts that were just made begin now, and go on until they are over.
    if state.cut_flashes.update(context.input(|input| input.time)) {
        context.request_repaint();
    }
    state.rack_labels.clear();
    state.keyframe_grips.clear();
    state.rack_rectangles.clear();
    state.drawer_bottoms.clear();
    state.drop_targets.clear();
    state.pointer_message = None;
    // The drawers that are sliding go on sliding, and those of clips and tracks that are gone are forgotten.
    state.drawers.forget_what_is_gone(content.project);
    if state.drawers.advance(context.input(|input| input.stable_dt)) {
        context.request_repaint();
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
                let strip_views = rows::show(
                    ui,
                    content,
                    (ruler_view, strips_offset, strips_width),
                    rows_area_height,
                    state,
                    textures,
                    commands,
                );
                // Room under the last row (after the space there is between the rows) for the boundary
                // under the last track to be taken hold of: without anything after the last row the
                // rows would end where it does.
                ui.add_space(track_resize::REACH_INTO_THE_BOXES);
                strip_views
            })
    });
    let rows = rows_scope.inner;
    let strip_views = rows.inner;
    ui.spacing_mut().item_spacing.y = previous_item_spacing;
    item_dragging::keep_dragging(ui, content, state, &strip_views, rows.inner_rect, commands);
    rack_dragging::keep_dragging(ui, content, state, &strip_views, commands);
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
    bring_the_drawer_that_is_opening_into_view(state, rows.inner_rect, &context);
    // A track dragged to the top or the bottom of the rows brings the rows beyond it into view.
    if state.track_reorder.is_some() {
        state.rows_scroll_offset += track_reordering::scroll_while_dragging_a_track(
            context
                .input(|input| input.pointer.interact_pos())
                .map(|pointer| pointer.y),
            (rows.inner_rect.top(), rows.inner_rect.bottom()),
            context.input(|input| input.stable_dt),
        );
    }
    playhead::draw(
        ui.painter(),
        &ruler_view.shown_in(strips_rectangle),
        ruler_rectangle,
        content.current_frame,
    );
    racks::draw_the_labels(&context, &state.rack_labels);
    racks::draw_the_message_by_the_pointer(&context, state.pointer_message.as_deref());
    flash::draw_over(
        &ui.painter().with_clip_rect(rows.inner_rect),
        strip_views.iter().map(|(track, view)| (track.id(), view)),
        &state.cut_flashes,
        context.input(|input| input.time),
    );

    if !context.input(|input| input.pointer.any_down()) {
        state.track_resize = None;
        state.track_reorder = None;
    }
}

/// While a drawer that was just opened slides open, scrolls the rows down, as far as it takes for the bottom of the drawer to be
/// in view (`visible_rows` is the part of the screen the rows are seen in): a drawer opened on a track at the bottom reaches
/// below what was in view. It never scrolls up, and leaves the rows alone once the drawer has stopped sliding.
fn bring_the_drawer_that_is_opening_into_view(
    state: &mut TimelineViewState,
    visible_rows: Rect,
    context: &egui::Context,
) {
    let Some(owner) = state.drawers.revealing() else {
        return;
    };
    if let Some((_, bottom)) = state.drawer_bottoms.iter().find(|(drawer, _)| *drawer == owner) {
        let how_far_below = bottom + theme::MARGIN_UNDER_A_DRAWER_BROUGHT_INTO_VIEW - visible_rows.bottom();
        if how_far_below > 0.0 {
            state.rows_scroll_offset += how_far_below;
            context.request_repaint();
        }
    }
    state.drawers.stop_revealing_if_it_has_stopped_sliding();
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
            kept: if adding {
                content.selection.to_items()
            } else {
                Vec::new()
            },
        });
    }
    if response.dragged_by(PointerButton::Primary)
        && let Some(band) = state.rubber_band.as_ref()
        && let Some(pointer) = ui.input(|input| input.pointer.interact_pos())
    {
        let rectangle = Rect::from_two_pos(band.origin, pointer);
        let mut selected = band.kept.clone();
        let mut add = |item: SelectionItem| {
            if !selected.contains(&item) {
                selected.push(item);
            }
        };
        for (track, view) in strip_views {
            for placement in track.placements() {
                let visible = rectangles_of(&placement, view).visible;
                if visible.width() > 0.0 && visible.intersects(rectangle) {
                    add(SelectionItem::Clip(placement.id));
                }
            }
        }
        for (keyframe, grip) in &state.keyframe_grips {
            if grip.intersects(rectangle) {
                add(SelectionItem::Keyframe(*keyframe));
            }
        }
        commands.push(Command::SelectItems {
            items: selected,
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
