//! What the timeline remembers between frames of the window: how far it is scrolled and zoomed,
//! what is being dragged, and the like.

use egui::{Pos2, Rect};

use super::drawers::Drawers;
use super::flash::CutFlashes;
use super::racks::{DropTarget, RackLabel};
use super::track_reordering::{RowMotion, TrackReorder};
use crate::application::clip_moving::StartTimeMemory;
use crate::application::selection::SelectionItem;
use crate::application::timeline_geometry::TimelineGeometry;
use crate::project::{ClipId, KeyframeId, RackId, TimelineFrameIndex, TrackId};

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

/// What was taken hold of to be dragged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Grabbed {
    Clip(ClipId),
    Keyframe(KeyframeId),
}

/// Clips and keyframes that are being dragged along the timeline (the clips along their tracks and to the other tracks).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ItemDrag {
    /// All the clips that move: the selected clips if the one that was taken hold of was one of them (or if it was a
    /// keyframe that was selected with them), and otherwise the clip that was taken hold of alone.
    pub(super) clips: Vec<ClipId>,
    /// All the keyframes that move on their own, likewise. (Not those in the drawers of the clips that move: they go with the
    /// clips.)
    pub(super) keyframes: Vec<KeyframeId>,
    /// What was taken hold of, whose place the pointer is kept at.
    pub(super) grabbed: Grabbed,
    /// The frame that was under the pointer when it was taken hold of.
    pub(super) pointer_frame_at_the_start: u64,
    /// The frame of the timeline that the grabbed clip started at, or the grabbed keyframe was at, then.
    pub(super) grabbed_frame_at_the_start: u64,
    /// Where the grabbed clip was then, which it is held to when it is on another track (see [`StartTimeMemory`]). `None`
    /// when what was taken hold of is a keyframe.
    pub(super) start_time: Option<StartTimeMemory>,
}

/// A rack of keyframes that is being dragged, to another drawer or onto a track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct RackDrag {
    pub(super) rack: RackId,
    /// The frame that was under the pointer when the rack was taken hold of.
    pub(super) pointer_frame_at_the_start: u64,
    /// The frame of the timeline that the first keyframe of the rack was at then.
    pub(super) first_keyframe_frame_at_the_start: u64,
}

/// A rectangle being drawn round clips to select them, by dragging from an empty place.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct RubberBand {
    /// Where the pointer was when it was pressed: one corner of the rectangle, the other being where it is now.
    pub(super) origin: Pos2,
    /// What was selected then and stays selected (when Ctrl was held), to which what is in the rectangle is added.
    pub(super) kept: Vec<SelectionItem>,
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
    /// The clips and keyframes that are being dragged along the timeline, if there are some.
    pub(super) item_drag: Option<ItemDrag>,
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
    /// Which drawers on clips and tracks are open.
    pub(super) drawers: Drawers,
    /// The labels of the racks of keyframes that the pointer is over, which are drawn over everything at the end of the
    /// frame. (Made again in every frame.)
    pub(super) rack_labels: Vec<RackLabel>,
    /// The rack of keyframes that is being dragged to another drawer, if there is one.
    pub(super) rack_drag: Option<RackDrag>,
    /// The drawers and crates that a rack can be put in, as they were drawn in this frame, the one drawn last being the one on
    /// top. (Made again in every frame.)
    pub(super) drop_targets: Vec<DropTarget>,
    /// What is said by the pointer in this frame, if anything is: why a rack that is dragged cannot go where it is.
    pub(super) pointer_message: Option<String>,
    /// Where each keyframe that can be taken hold of is on the screen, as it was drawn in this frame, which is what the
    /// rectangle drawn round things to select them goes round. (Made again in every frame.)
    pub(super) keyframe_grips: Vec<(KeyframeId, Rect)>,
}

impl TimelineViewState {
    /// Forgets the scroll position and the zoom (when a different project is loaded).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Opens the drawer of the clip or the track `owner`, if it is not open, so that what was put in it is seen: it slides
    /// open in the next frames of the window.
    pub fn open_the_drawer_of(&mut self, owner: crate::project::AnimationTarget) {
        self.drawers.open_it(owner);
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
