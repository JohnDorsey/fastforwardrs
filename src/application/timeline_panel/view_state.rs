//! What the timeline remembers between frames of the window: how far it is scrolled and zoomed,
//! what is being dragged, and the like.

use egui::{Pos2, Rect};

use super::drawers::{DrawerSound, Drawers};
use super::flash::CutFlashes;
use super::racks::{DropTarget, RackLabel};
use super::track_reordering::{RowMotion, TrackReorder};
use crate::application::selection::SelectionItem;
use crate::application::start_time_hold::StartTimeMemory;
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

/// What was taken hold of to be dragged, and where it was then, which it is held to when it is in another row (see
/// [`StartTimeMemory`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Grabbed {
    Clip {
        clip: ClipId,
        start_time: StartTimeMemory<TrackId>,
    },
    Keyframe {
        keyframe: KeyframeId,
        start_time: StartTimeMemory<RackId>,
    },
    /// A rack that is dragged along the timeline with other things that are selected: clips, keyframes, or other racks. (A rack that
    /// is alone is dragged to other drawers as well: see [`RackDrag`].) It goes along the timeline only, and is never held to a
    /// frame: it does not change rows.
    Rack {
        rack: RackId,
        /// The frame of the timeline that the first keyframe of the rack was at when it was taken hold of.
        first_keyframe_frame: u64,
    },
}

impl Grabbed {
    /// The frame of the timeline that the grabbed clip started at, or the grabbed keyframe or rack was at, when it was taken hold of.
    pub(super) fn frame_at_the_start(&self) -> u64 {
        match self {
            Self::Clip { start_time, .. } => start_time.start(),
            Self::Keyframe { start_time, .. } => start_time.start(),
            Self::Rack {
                first_keyframe_frame, ..
            } => *first_keyframe_frame,
        }
    }
}

/// Clips, keyframes and racks of keyframes that are being dragged along the timeline (the clips along their tracks and to the other
/// tracks, the keyframes along their racks and to the other racks that they can be in).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ItemDrag {
    /// All the clips that move: the selected clips if the one that was taken hold of was one of them (or if it was a
    /// keyframe or a rack that was selected with them), and otherwise the clip that was taken hold of alone.
    pub(super) clips: Vec<ClipId>,
    /// All the keyframes that move on their own, likewise: the selected keyframes, and the keyframes of the selected racks (a rack
    /// is moved along the timeline by moving each of its keyframes the same number of frames). Not those in the drawers of the
    /// clips that move: they go with the clips.
    pub(super) keyframes: Vec<KeyframeId>,
    /// The selected racks, which are moved along the timeline whole: their keyframes stay in them, whatever else is dragged up and
    /// down to other racks.
    pub(super) racks: Vec<RackId>,
    /// What was taken hold of, whose place the pointer is kept at.
    pub(super) grabbed: Grabbed,
    /// The frame that was under the pointer when it was taken hold of.
    pub(super) pointer_frame_at_the_start: u64,
}

/// How tall the rows were in the last frame, which what they are going to be in the next is told from (see
/// [`super::prepare`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::application) struct RowsInTheLastFrame {
    /// How tall the rows were together, in points: what the area they scroll in found to be in it.
    pub(in crate::application) content_height: f32,
    /// How tall the rows were laid out to be (see [`super::rows::RowsLayout::total_height`]). It is only ever compared with
    /// what it was in another frame, so what it counts and what it leaves out does not matter, as long as it is the same.
    pub(in crate::application) height_laid_out: f32,
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
    /// What the rows were like in the last frame, once there has been one.
    pub(in crate::application) rows_in_the_last_frame: Option<RowsInTheLastFrame>,
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
    /// wheel with Alt held scrolls the rows up and down only while the pointer is over it. (The bottom of it is the bottom of what
    /// can be seen of the rows.)
    pub(in crate::application) timeline_area_in_the_last_frame: Option<Rect>,
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
    /// How far down the screen each open drawer reaches, as it was drawn in this frame, which is how far down the rows are
    /// scrolled to bring a drawer that is opening into view. (Made again in every frame.)
    pub(super) drawer_bottoms: Vec<(crate::project::AnimationTarget, f32)>,
    /// Where each rack that can be touched is on the screen (as far as it can be seen), as it was drawn in this frame, which is
    /// what a keyframe that is dragged to another rack is put in the rack that the pointer is over by. (Made again in every
    /// frame.)
    pub(super) rack_rectangles: Vec<(RackId, Rect)>,
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

    /// The sounds that drawers have begun to make since this was last called, in order (see [`Drawers::take_the_sounds`]), which
    /// are then forgotten.
    pub fn take_the_drawer_sounds(&mut self) -> Vec<DrawerSound> {
        self.drawers.take_the_sounds()
    }

    /// Scrolls and zooms the strips so that the frames from `first_frame` up to (not including) `end_frame` are in the middle of the
    /// view and nearly fill its width (see [`TimelineGeometry::framing`]); `normal` is the scale the strips start at, and `total_frames`
    /// how long the timeline is. The view does not follow the current frame again until that changes. It does nothing before the
    /// strips have been drawn, which is when how wide the view is becomes known.
    pub fn frame_these_frames(
        &mut self,
        normal: TimelineGeometry,
        total_frames: u64,
        first_frame: u64,
        end_frame: u64,
    ) {
        let Some(strips) = self.strips_area_in_the_last_frame else {
            return;
        };
        let view_width = f64::from(strips.width());
        let (geometry, scroll_offset) = self.geometry(normal).framing(first_frame, end_frame, view_width);
        self.points_per_second = Some(geometry.points_per_second());
        self.scroll_offset = geometry.clamp_scroll_offset(scroll_offset, view_width, total_frames);
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

#[cfg(test)]
mod tests {
    use egui::{pos2, vec2};

    use super::*;
    use crate::project::FrameRate;

    /// The scale the strips start at: pictures of 16 by 9 with strips 100 points tall, at 30 frames a second.
    fn normal() -> TimelineGeometry {
        TimelineGeometry::for_picture(FrameRate::new(30, 1).unwrap(), 1920, 1080, 100.0)
    }

    /// A view whose strips were drawn in the last frame, `width` points wide.
    fn drawn_view_of_width(width: f32) -> TimelineViewState {
        TimelineViewState {
            strips_area_in_the_last_frame: Some(Rect::from_min_size(pos2(100.0, 0.0), vec2(width, 200.0))),
            ..TimelineViewState::default()
        }
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1.0e-6, "{actual} is not {expected}");
    }

    /// Two seconds of frames framed in a view 800 points wide are 0.92 of it: 368 points a second.
    const POINTS_A_SECOND_WHEN_TWO_SECONDS_ARE_FRAMED: f64 = 368.0;

    #[test]
    fn frames_in_the_middle_of_the_timeline_are_put_in_the_middle_of_the_view() {
        let mut view = drawn_view_of_width(800.0);
        // Frames 120 to 180 are the seconds 4 to 6 of a timeline of 10: their middle is at the second 5.
        view.frame_these_frames(normal(), 300, 120, 180);
        assert_close(
            view.geometry(normal()).points_per_second(),
            POINTS_A_SECOND_WHEN_TWO_SECONDS_ARE_FRAMED,
        );
        assert_close(
            view.scroll_offset,
            5.0 * POINTS_A_SECOND_WHEN_TWO_SECONDS_ARE_FRAMED - 400.0,
        );
    }

    #[test]
    fn frames_at_the_start_of_the_timeline_are_not_put_in_the_middle_if_that_would_scroll_the_view_before_the_start() {
        let mut view = drawn_view_of_width(800.0);
        // (Their middle is at the second 1, which would put the left edge of the view 32 points before the strip.)
        view.frame_these_frames(normal(), 300, 0, 60);
        assert_close(view.scroll_offset, 0.0);
    }

    #[test]
    fn frames_at_the_end_of_the_timeline_are_not_put_in_the_middle_if_that_would_scroll_the_view_past_the_end() {
        let mut view = drawn_view_of_width(800.0);
        // (Their middle is at the second 9, which would put the right edge of the view 32 points past the strip, of 10 seconds.)
        view.frame_these_frames(normal(), 300, 240, 300);
        assert_close(
            view.scroll_offset,
            10.0 * POINTS_A_SECOND_WHEN_TWO_SECONDS_ARE_FRAMED - 800.0,
        );
    }

    #[test]
    fn nothing_is_framed_before_the_strips_have_been_drawn_which_is_when_the_width_of_the_view_becomes_known() {
        let mut view = TimelineViewState {
            scroll_offset: 77.0,
            ..TimelineViewState::default()
        };
        view.frame_these_frames(normal(), 300, 120, 180);
        assert_close(view.scroll_offset, 77.0);
        assert_eq!(view.points_per_second, None, "and the zoom is as it was");
    }
}
