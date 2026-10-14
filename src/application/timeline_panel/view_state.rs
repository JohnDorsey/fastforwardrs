//! What the timeline remembers between frames of the window: how far it is scrolled and zoomed,
//! what is being dragged, and the like.

use egui::{Pos2, Rect};

use super::drawers::{DrawerSound, Drawers};
use super::flash::CutFlashes;
use super::racks::{DropTarget, RackLabel};
use super::track_reordering::{RowMotion, TrackReorder};
use super::vertical_zoom::VerticalZoom;
use super::view_history::{View, ViewAction, ViewHistory};
use crate::application::selection::SelectionItem;
use crate::application::start_time_hold::StartTimeMemory;
use crate::application::timeline_geometry::TimelineGeometry;
use crate::project::{
    ClipGroupBeforeTheDrag, ClipGroupEdge, ClipId, GroupId, KeyframeId, RackId, TimelineFrameIndex, TrackId,
};

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
    /// How tall the row of the track was drawn when the boundary was taken hold of, in points. (With the rows zoomed up or
    /// down that is not the height of the track: see [`VerticalZoom`].)
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
    /// A group that is dragged by its title bar: the clips that are in it go along with it (they are in [`ItemDrag::clips`]), and it is not
    /// held to a frame when it goes to other rows.
    Group {
        group: GroupId,
        /// The first frame of the rectangle of the group when it was taken hold of.
        first_frame: u64,
        /// The row (the place of the track in the list of them) that the pointer was over then: the group goes down as many rows as the
        /// pointer does from there.
        row: usize,
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
            Self::Group { first_frame, .. } => *first_frame,
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
    /// The clip group that each of those clips was in when the drag began, if it was in one: a clip that was in none joins the clip group it
    /// is dragged into, and is out of it again when it is dragged out of it, which is told from this (see
    /// [`crate::project::Project::move_clips_remembering`]).
    pub(super) clip_groups_before_the_drag: Vec<ClipGroupBeforeTheDrag>,
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

/// The left or the right edge of a clip group, which is being moved by the pointer (see [`super::group_edges`]), and is shown moving.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct GroupEdgeMove {
    pub(super) group: GroupId,
    pub(super) edge: ClipGroupEdge,
    /// The frames that the edge can be put at, as they were when it was taken hold of.
    pub(super) places: Vec<std::ops::RangeInclusive<u64>>,
    /// The frame that the edge is shown at, which is not a whole number while it moves: it goes along with the pointer where the pointer is at
    /// a place the edge can be put at, and goes quickly to the nearest place where it is not.
    pub(super) shown: f64,
    /// The frame that the edge is put at if the button is let go now: the nearest place to the pointer.
    pub(super) held_at: u64,
    /// Whether the button has been let go: the group has the frames that it was given, and the edge shown goes to where it has it now.
    pub(super) is_settling: bool,
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

/// Racks of keyframes that are being dragged together, to another drawer or onto a track: the rack that was taken hold of, and
/// the others that are selected with it if they are all in the same drawer.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct RackDrag {
    /// The racks, the one that is highest in its drawer first, so that they keep their order wherever they go.
    pub(super) racks: Vec<RackId>,
    /// The frame that was under the pointer when the racks were taken hold of.
    pub(super) pointer_frame_at_the_start: u64,
    /// The frame of the timeline that the first keyframe of any of the racks was at then.
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
    /// How far the rows are zoomed up and down: the rows are as much taller than their tracks say as this says.
    pub(in crate::application) vertical_zoom: VerticalZoom,
    /// The views the timeline had before what the user has done to it lately, which the Back and Forward buttons of a mouse go
    /// through.
    pub(super) view_history: ViewHistory,
    /// The view as it was at the start of this frame of the window, before anything that the user did in it was done to it, while
    /// the frame is going on (see [`Self::begin_a_frame_of_the_view`]).
    view_at_the_start_of_the_frame: Option<View>,
    /// The kind of thing the user did to the view in this frame, if they did one.
    view_action_in_this_frame: Option<ViewAction>,
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
    /// The edge of a clip group that is being moved, if there is one.
    pub(super) group_edge_move: Option<GroupEdgeMove>,
    /// The rectangle that is being drawn round clips, if there is one.
    pub(super) rubber_band: Option<RubberBand>,
    /// Where the ruler and the strips were in the last frame, so that the mouse wheel scrolls
    /// them along only while the pointer is over them.
    pub(super) strips_area_in_the_last_frame: Option<Rect>,
    /// Where the whole timeline (the title boxes too) was in the last frame, so that the mouse
    /// wheel with Alt held scrolls the rows up and down only while the pointer is over it, and with Ctrl as well zooms them up and
    /// down. (The bottom of it is the bottom of what can be seen of the rows.)
    pub(in crate::application) timeline_area_in_the_last_frame: Option<Rect>,
    /// Where what can be seen of the rows was in the last frame (under the ruler, from the title boxes to the end of the strips),
    /// which is where the rows are scrolled in.
    pub(super) rows_view_in_the_last_frame: Option<Rect>,
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
    /// strips have been drawn, which is when how wide the view is becomes known. (Doing it is a step of the history of views, if it
    /// changes the view.)
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
        let before = self.view();
        let view_width = f64::from(strips.width());
        let (geometry, scroll_offset) = self.geometry(normal).framing(first_frame, end_frame, view_width);
        self.points_per_second = Some(geometry.points_per_second());
        self.scroll_offset = geometry.clamp_scroll_offset(scroll_offset, view_width, total_frames);
        if self.view() != before {
            // (Every framing is a step of its own, whenever it was made: the time says nothing.)
            self.view_history.note_a_change(ViewAction::Framing, before, 0.0);
        }
    }

    /// Begins a frame of the window, as far as what the user does to the view in it is concerned: what it is now is where the
    /// things that are done to it in this frame begin from.
    pub(super) fn begin_a_frame_of_the_view(&mut self) {
        self.view_at_the_start_of_the_frame = Some(self.view());
        self.view_action_in_this_frame = None;
    }

    /// Notes that the user is doing something of the kind `action` to the view in this frame (zooming it takes the place of
    /// scrolling it, if both are done).
    pub(super) fn note_that_the_user_changes_the_view(&mut self, action: ViewAction) {
        if self.view_action_in_this_frame != Some(ViewAction::Zooming) {
            self.view_action_in_this_frame = Some(action);
        }
    }

    /// Ends the frame: if the user did something to the view in it that changed it (scrolling it as far as it goes, which does
    /// nothing, does not), that is a step of the history of views. `time` is how many seconds into the program's run it is.
    pub(super) fn end_a_frame_of_the_view(&mut self, time: f64) {
        if let (Some(before), Some(action)) = (
            self.view_at_the_start_of_the_frame.take(),
            self.view_action_in_this_frame.take(),
        ) && self.view() != before
        {
            self.view_history.note_a_change(action, before, time);
        }
    }

    /// How the timeline is looked at now.
    pub(super) fn view(&self) -> View {
        View {
            scroll_offset: self.scroll_offset,
            points_per_second: self.points_per_second,
            rows_scroll_offset: self.rows_scroll_offset,
            vertical_zoom: self.vertical_zoom,
        }
    }

    /// Looks at the timeline as `view` says.
    fn look_at(&mut self, view: View) {
        self.scroll_offset = view.scroll_offset;
        self.points_per_second = view.points_per_second;
        self.rows_scroll_offset = view.rows_scroll_offset;
        self.vertical_zoom = view.vertical_zoom;
    }

    /// Goes back to the view the timeline had before the latest thing that was done to it (zooming it, scrolling it, framing what is
    /// selected), if there is one.
    pub fn go_back_in_the_view_history(&mut self) {
        if let Some(view) = self.view_history.go_back(self.view()) {
            self.look_at(view);
        }
    }

    /// Goes forward again to the view that was gone back from, if there is one.
    pub fn go_forward_in_the_view_history(&mut self) {
        if let Some(view) = self.view_history.go_forward(self.view()) {
            self.look_at(view);
        }
    }

    /// How many views there are to go back through.
    #[cfg(test)]
    pub(in crate::application) fn view_history_steps_back(&self) -> usize {
        self.view_history.steps_back()
    }

    /// How many views there are to go forward through again.
    #[cfg(test)]
    pub(in crate::application) fn view_history_steps_forward(&self) -> usize {
        self.view_history.steps_forward()
    }

    /// Makes a flash of light at the cut that was just made in the clip on `track` at `frame`, which shows
    /// from the next frame of the window on.
    pub fn flash_a_cut(&mut self, track: TrackId, frame: TimelineFrameIndex) {
        self.cut_flashes.add(track, frame);
    }

    /// Whether the pointer has been taken by a drag that does not go on to what is under it: a rack that is dragged to another drawer, a track
    /// that is made taller, a rectangle that is drawn round clips, or an edge of a clip group that is moved. (Not the dragging of clips,
    /// keyframes and groups along the timeline, `item_drag`, which what is under the pointer matters to, and not the moving of a track, which
    /// the rows and the groups are not drawn as they are for.) Nothing else is shown or taken hold of as being under the pointer while one
    /// of these goes on.
    pub(super) fn is_in_a_gesture_other_than_item_dragging(&self) -> bool {
        self.rack_drag.is_some()
            || self.track_resize.is_some()
            || self.rubber_band.is_some()
            || self.group_edge_move.is_some()
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

    /// A state that is as it is at the start, with `change` done to it.
    fn state_with(change: impl FnOnce(&mut TimelineViewState)) -> TimelineViewState {
        let mut state = TimelineViewState::default();
        change(&mut state);
        state
    }

    #[test]
    fn dragging_a_rack_making_a_track_taller_drawing_a_rectangle_and_moving_an_edge_are_each_a_gesture_other_than_item_dragging()
     {
        assert!(!TimelineViewState::default().is_in_a_gesture_other_than_item_dragging());

        assert!(
            state_with(|state| {
                state.rack_drag = Some(RackDrag {
                    racks: vec![],
                    pointer_frame_at_the_start: 0,
                    first_keyframe_frame_at_the_start: 0,
                });
            })
            .is_in_a_gesture_other_than_item_dragging(),
            "a rack dragged to another drawer"
        );
        assert!(
            state_with(|state| {
                state.track_resize = Some(TrackResize {
                    track: TrackId::new(0),
                    height_at_the_start: 100.0,
                    pointer_y_at_the_start: 0.0,
                    rows_area_height: 300.0,
                });
            })
            .is_in_a_gesture_other_than_item_dragging(),
            "a track made taller"
        );
        assert!(
            state_with(|state| {
                state.rubber_band = Some(RubberBand {
                    origin: pos2(0.0, 0.0),
                    kept: vec![],
                });
            })
            .is_in_a_gesture_other_than_item_dragging(),
            "a rectangle drawn round clips"
        );
        assert!(
            state_with(|state| {
                state.group_edge_move = Some(GroupEdgeMove {
                    group: GroupId::new(0),
                    edge: ClipGroupEdge::Left,
                    places: vec![],
                    shown: 0.0,
                    held_at: 0,
                    is_settling: false,
                });
            })
            .is_in_a_gesture_other_than_item_dragging(),
            "an edge of a clip group moved (and one that has been let go of and goes to where it is put)"
        );
    }

    #[test]
    fn dragging_items_and_moving_a_track_are_not_gestures_other_than_item_dragging() {
        assert!(
            !state_with(|state| {
                state.item_drag = Some(ItemDrag {
                    clips: vec![],
                    clip_groups_before_the_drag: vec![],
                    keyframes: vec![],
                    racks: vec![],
                    grabbed: Grabbed::Rack {
                        rack: RackId::new(TrackId::new(0), 0),
                        first_keyframe_frame: 0,
                    },
                    pointer_frame_at_the_start: 0,
                });
            })
            .is_in_a_gesture_other_than_item_dragging()
        );
        assert!(
            !state_with(|state| {
                state.track_reorder = Some(TrackReorder {
                    track: TrackId::new(0),
                    grabbed_at: 0.0,
                    to_position: 0,
                });
            })
            .is_in_a_gesture_other_than_item_dragging()
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
