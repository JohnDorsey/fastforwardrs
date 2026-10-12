//! What the timeline remembers between frames of the window: how far it is scrolled and zoomed,
//! what is being dragged, and the like.

use egui::{Pos2, Rect};

use super::drawers::Drawers;
use super::flash::CutFlashes;
use super::track_reordering::{RowMotion, TrackReorder};
use crate::application::clip_moving::StartTimeMemory;
use crate::application::timeline_geometry::TimelineGeometry;
use crate::project::{ClipId, TimelineFrameIndex, TrackId};

/// Where a click on an empty part of a strip happened, for the menu that puts something new there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NewMenuTarget {
    pub(super) track: TrackId,
    pub(super) frame: TimelineFrameIndex,
}

/// A track that is being made taller or shorter, by the boundary under its title box being dragged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TrackResize {
    pub(super) track: TrackId,
    /// How tall the track was when the boundary was taken hold of, in points.
    pub(super) height_at_the_start: f32,
    /// Where the pointer was then, up and down the screen.
    pub(super) pointer_y_at_the_start: f32,
    /// How tall the rows were together then, which they stay while the track is being resized
    /// (see [`super::show`]).
    pub(super) rows_area_height: f32,
}

/// Clips that are being dragged along their tracks.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ClipDrag {
    /// All the clips that move: the selected clips if the one that was taken hold of was one of them,
    /// and otherwise that clip alone.
    pub(super) moving: Vec<ClipId>,
    /// The clip that was taken hold of, whose place the pointer is kept at.
    pub(super) grabbed: ClipId,
    /// The frame that was under the pointer when the clip was taken hold of.
    pub(super) pointer_frame_at_the_start: u64,
    /// Where the grabbed clip was then, which it is held to when it is on another track (see [`StartTimeMemory`]).
    pub(super) start_time: StartTimeMemory,
}

/// A rectangle being drawn round clips to select them, by dragging from an empty place.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct RubberBand {
    /// Where the pointer was when it was pressed: one corner of the rectangle, the other being where it is now.
    pub(super) origin: Pos2,
    /// The clips that were selected then and stay selected (when Ctrl was held), to which those in the rectangle are added.
    pub(super) kept: Vec<ClipId>,
}

/// What the timeline remembers between frames of the window.
#[derive(Default)]
pub struct TimelineViewState {
    /// How far along the strips the left edge of the view is, in points.
    pub(in crate::application) scroll_offset: f64,
    /// How many points of the screen a second of the finished video takes up, if the user has
    /// zoomed. (Otherwise it is whatever the timeline starts at, which depends on the shape of the
    /// picture.)
    pub(super) points_per_second: Option<f64>,
    /// The current frame the view last scrolled to follow.
    pub(super) last_followed_frame: Option<TimelineFrameIndex>,
    /// The frame that was picked on the ruler during the previous frame of the window. The user
    /// chose that place in the view, so the view must not scroll to put it in the middle.
    pub(super) frame_picked_on_the_ruler: Option<TimelineFrameIndex>,
    /// How far down the rows are scrolled, in points. (The scroll bar moves them too: what it did
    /// is read back into this each frame.)
    pub(in crate::application) rows_scroll_offset: f32,
    /// How tall the rows were together in the last frame, in points.
    pub(super) rows_area_height: f32,
    /// The track that is being resized, if one is.
    pub(super) track_resize: Option<TrackResize>,
    /// The track that is being dragged to another place in the list, if one is.
    pub(super) track_reorder: Option<TrackReorder>,
    /// Where the rows are shown while tracks are being moved about.
    pub(super) row_motion: RowMotion,
    /// The clips that are being dragged along their tracks, if there are some.
    pub(super) clip_drag: Option<ClipDrag>,
    /// The rectangle that is being drawn round clips, if there is one.
    pub(super) rubber_band: Option<RubberBand>,
    /// Where the ruler and the strips were in the last frame, so that the mouse wheel scrolls
    /// them along only while the pointer is over them.
    pub(super) strips_area_in_the_last_frame: Option<Rect>,
    /// Where the whole timeline (the title boxes too) was in the last frame, so that the mouse
    /// wheel with Alt held scrolls the rows up and down only while the pointer is over it.
    pub(super) timeline_area_in_the_last_frame: Option<Rect>,
    /// Where the "New" menu was last opened.
    pub(super) new_menu_target: Option<NewMenuTarget>,
    /// The flashes of light at the cuts that were just made.
    pub(super) cut_flashes: CutFlashes,
    /// Which drawers on clips are open.
    pub(super) drawers: Drawers,
}

impl TimelineViewState {
    /// Forgets the scroll position and the zoom (when a different project is loaded).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Makes a flash of light at the cut that was just made in the clip on `track` at `frame`, which shows
    /// from the next frame of the window on.
    pub fn flash_a_cut(&mut self, track: TrackId, frame: TimelineFrameIndex) {
        self.cut_flashes.add(track, frame);
    }

    /// The scale the strips are drawn at: `normal`, the scale the timeline starts at, as far as the
    /// user has zoomed it.
    pub fn geometry(&self, normal: TimelineGeometry) -> TimelineGeometry {
        self.points_per_second.map_or(normal, |points_per_second| {
            normal.with_points_per_second(points_per_second)
        })
    }
}
