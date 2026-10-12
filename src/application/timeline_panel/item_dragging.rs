//! Dragging clips and keyframes: where the pointer takes them.
//!
//! A drag is begun by pressing a clip or a keyframe and moving the pointer a little (see the `strip` and `racks` modules,
//! which select what was pressed, if it is not selected, and note what was taken hold of). From then on, in each frame of
//! the window until the button is let go, what is dragged is moved towards where the pointer is: along the timeline to the
//! frame it is over, and (for clips) up and down to the track it is over. Everything that is dragged goes the same number of
//! frames, so that it all keeps its place in time in relation to the rest. What is dragged is the selected clips and the
//! selected keyframes, if the one that was taken hold of is one of them, and otherwise it alone (with Ctrl, as well as what
//! is selected).
//!
//! * Clips are moved as far as the clips next to them leave room (see [`crate::application::clip_moving`]), and go to the
//!   track the pointer is over if there is room for them there.
//! * Keyframes go as far as the drawer they are in has frames for, and stop short of the keyframes of their rack that are not
//!   dragged (see [`crate::project::Project::allowed_shift_of_keyframes`]). A keyframe in the drawer of a clip that is
//!   dragged is not dragged on its own: it goes with the clip, where it is in relation to it.
//!
//! The drag is worked out here, once for all the strips, from the pointer and not from what was taken hold of: a clip is
//! drawn in another strip once it has gone to another track.
//!
//! A clip that is dragged to another track is held to the time it started at, if it is near that time (see
//! [`crate::application::clip_moving::StartTimeMemory`]): the pointer never goes straight down.

use egui::{CursorIcon, Rect, Ui};

use super::strip_view::StripView;
use super::view_state::Grabbed;
use super::{TimelineContent, TimelineViewState};
use crate::application::clip_moving::{MoveBy, allowed_move_holding, moves_by};
use crate::application::commands::Command;
use crate::project::{AnimationTarget, ClipId, KeyframeId, Project, Track};

/// Which of `keyframes` are dragged on their own when the clips `clips` are dragged too: those that are not in the drawer of
/// one of the clips, because the drawer goes with its clip and what is in it with the drawer.
pub(super) fn keyframes_dragged_on_their_own(
    project: &Project,
    clips: &[ClipId],
    keyframes: &[KeyframeId],
) -> Vec<KeyframeId> {
    keyframes
        .iter()
        .copied()
        .filter(|keyframe| {
            let owner = project.located_rack(keyframe.rack()).map(|located| located.owner);
            !matches!(owner, Some(AnimationTarget::Clip(clip)) if clips.contains(&clip))
        })
        .collect()
}

/// If something is being dragged, adds the commands that move it towards the pointer, as far as that can be done: `rows` are
/// the tracks with where their strips are on the screen, in the order of the tracks, and `visible_rows` is the part of the
/// screen that rows can be seen in.
pub(super) fn keep_dragging(
    ui: &Ui,
    content: &TimelineContent<'_>,
    state: &mut TimelineViewState,
    rows: &[(&Track, StripView)],
    visible_rows: Rect,
    commands: &mut Vec<Command>,
) {
    let Some(drag) = state.item_drag.as_mut() else {
        return;
    };
    // A drag ends with the button, whichever of the strips the pointer is over, or none.
    if !content.can_edit || !ui.input(|input| input.pointer.primary_down()) {
        state.item_drag = None;
        return;
    }
    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) else {
        return;
    };
    let Some((_, view)) = rows.first() else {
        return;
    };
    let project = content.project;
    let frame_under = |screen_x: f32| view.frame_at(screen_x).map_or(0, |frame| frame.value());
    let frames_the_pointer_has_gone = frame_under(pointer.x) as i64 - drag.pointer_frame_at_the_start as i64;
    // Where what was taken hold of is now: how far it has gone (which is not how far the pointer has when something was in its
    // way) is what is left of what the pointer has gone.
    let grabbed_frame_now = match drag.grabbed {
        Grabbed::Clip(clip) => project
            .placement_of(clip)
            .map(|placement| placement.first_timeline_frame.value()),
        Grabbed::Keyframe(keyframe) => project.timeline_frame_of_keyframe(keyframe),
    };
    let Some(grabbed_frame_now) = grabbed_frame_now else {
        return;
    };
    let frames_the_grabbed_has_gone = grabbed_frame_now as i64 - drag.grabbed_frame_at_the_start as i64;
    let wanted_frames = frames_the_pointer_has_gone - frames_the_grabbed_has_gone;

    let mut by = MoveBy {
        tracks_down: 0,
        frames: wanted_frames,
    };
    if !drag.clips.is_empty() {
        let mut tracks_the_pointer_is_down = 0;
        let mut snap = None;
        if let Grabbed::Clip(grabbed) = drag.grabbed
            && let (Some(placement), Some(grabbed_row), Some(start_time)) = (
                project.placement_of(grabbed),
                project.track_position_holding(grabbed),
                drag.start_time.as_mut(),
            )
        {
            tracks_the_pointer_is_down =
                row_nearest_to(rows, visible_rows, pointer.y).map_or(0, |row| row as i64 - grabbed_row as i64);
            let points_per_frame = view.points_per_frame();
            start_time.note_where_the_clip_is(
                project.tracks()[grabbed_row].id(),
                placement.first_timeline_frame.value(),
                points_per_frame,
            );
            snap = start_time.snap(grabbed, points_per_frame);
        }
        by = allowed_move_holding(
            project,
            &drag.clips,
            MoveBy {
                tracks_down: tracks_the_pointer_is_down,
                frames: wanted_frames,
            },
            snap,
        );
    }
    if !drag.keyframes.is_empty() {
        by.frames = project.allowed_shift_of_keyframes(&drag.keyframes, by.frames);
    }
    // (When clips and keyframes are dragged together both commands are made in every frame, even one that does nothing, so
    // that the frames of the drag are all alike and are one edit of the history.)
    let both = !drag.clips.is_empty() && !drag.keyframes.is_empty();
    if !drag.clips.is_empty() && (by != MoveBy::default() || both) {
        commands.push(Command::MoveClips(moves_by(project, &drag.clips, by)));
    }
    if !drag.keyframes.is_empty() && (by.frames != 0 || both) {
        commands.push(Command::MoveKeyframes(
            project.moves_of_keyframes_by(&drag.keyframes, by.frames),
        ));
    }
}

/// The row (its place in the list of tracks) that is nearest to `y` up and down the screen, of those that
/// can be seen in `visible_rows`: the one the pointer is over, or if it is between two, or above them all,
/// or below them all, the one it is nearest to.
fn row_nearest_to(rows: &[(&Track, StripView)], visible_rows: Rect, y: f32) -> Option<usize> {
    let how_far = |rectangle: Rect| (rectangle.top() - y).max(y - rectangle.bottom()).max(0.0);
    rows.iter()
        .enumerate()
        .filter(|(_, (_, view))| view.rectangle.intersects(visible_rows))
        .min_by(|(_, (_, first)), (_, (_, second))| how_far(first.rectangle).total_cmp(&how_far(second.rectangle)))
        .map(|(row, _)| row)
}

#[cfg(test)]
mod tests {
    use egui::{pos2, vec2};

    use super::*;
    use crate::application::timeline_geometry::TimelineGeometry;
    use crate::project::{FrameRate, PropertyId, RackId, TextObject, TimelineFrameIndex, TrackId};

    fn strip_at(top: f32, height: f32) -> StripView {
        let geometry = TimelineGeometry::for_picture(FrameRate::new(30, 1).unwrap(), 100, 100, 120.0);
        StripView::new(
            geometry,
            0.0,
            Rect::from_min_size(pos2(100.0, top), vec2(500.0, height)),
        )
    }

    #[test]
    fn the_row_the_pointer_is_over_is_the_row_and_between_rows_or_beyond_them_it_is_the_nearest() {
        let tracks: Vec<Track> = (0..3).map(|number| Track::new(TrackId::new(number), "track")).collect();
        let rows: Vec<(&Track, StripView)> = vec![
            (&tracks[0], strip_at(10.0, 40.0)),  // 10 to 50
            (&tracks[1], strip_at(56.0, 40.0)),  // 56 to 96
            (&tracks[2], strip_at(102.0, 80.0)), // 102 to 182
        ];
        let everything = Rect::from_min_max(pos2(0.0, 0.0), pos2(700.0, 200.0));
        let row_at = |y: f32| row_nearest_to(&rows, everything, y);
        assert_eq!(row_at(30.0), Some(0));
        assert_eq!(row_at(60.0), Some(1));
        assert_eq!(row_at(150.0), Some(2));
        assert_eq!(row_at(52.0), Some(0), "nearer to the bottom of the first");
        assert_eq!(row_at(54.5), Some(1), "nearer to the top of the second");
        assert_eq!(row_at(53.0), Some(0), "half way: the upper");
        assert_eq!(row_at(-500.0), Some(0), "above them all");
        assert_eq!(row_at(5000.0), Some(2), "below them all");
    }

    #[test]
    fn a_row_that_is_scrolled_out_of_sight_is_not_one_that_the_pointer_can_be_nearest_to() {
        let tracks: Vec<Track> = (0..3).map(|number| Track::new(TrackId::new(number), "track")).collect();
        let rows: Vec<(&Track, StripView)> = vec![
            (&tracks[0], strip_at(-100.0, 40.0)), // scrolled out above
            (&tracks[1], strip_at(10.0, 40.0)),
            (&tracks[2], strip_at(56.0, 40.0)),
        ];
        let seen = Rect::from_min_max(pos2(0.0, 0.0), pos2(700.0, 100.0));
        assert_eq!(
            row_nearest_to(&rows, seen, -80.0),
            Some(1),
            "not the one that cannot be seen"
        );
        assert_eq!(row_nearest_to(&rows, seen, 80.0), Some(2));
        assert_eq!(row_nearest_to(&rows, Rect::NOTHING, 80.0), None, "nothing can be seen");
    }

    #[test]
    fn a_keyframe_in_the_drawer_of_a_clip_that_is_dragged_goes_with_the_clip_and_the_others_are_dragged_on_their_own() {
        let mut project = Project::empty();
        let track = project.tracks()[0].id();
        let (first, _) = project
            .insert_text(track, TimelineFrameIndex::new(0), TextObject::default(), 30)
            .unwrap();
        let (second, _) = project
            .insert_text(track, TimelineFrameIndex::new(40), TextObject::default(), 30)
            .unwrap();
        let in_the_first = project
            .add_keyframe(
                AnimationTarget::Clip(first),
                PropertyId::Rotation,
                TimelineFrameIndex::new(5),
            )
            .unwrap()
            .unwrap()
            .keyframe;
        let in_the_second = project
            .add_keyframe(
                AnimationTarget::Clip(second),
                PropertyId::Rotation,
                TimelineFrameIndex::new(45),
            )
            .unwrap()
            .unwrap()
            .keyframe;
        let nowhere = KeyframeId::new(RackId::new(TrackId::new(50), 0), 0);
        let both = [in_the_first, in_the_second, nowhere];

        assert_eq!(
            keyframes_dragged_on_their_own(&project, &[first], &both),
            vec![in_the_second, nowhere],
            "the first goes with the first clip (and one that is not there is left in the list: nothing happens to it)"
        );
        assert_eq!(keyframes_dragged_on_their_own(&project, &[], &both), both.to_vec());
        assert_eq!(
            keyframes_dragged_on_their_own(&project, &[first, second], &both),
            vec![nowhere]
        );
    }
}
