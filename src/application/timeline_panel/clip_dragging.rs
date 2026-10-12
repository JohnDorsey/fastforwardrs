//! Dragging clips: where the pointer takes them.
//!
//! A drag is begun by pressing a clip and moving the pointer a little (see the `strip` module, which
//! selects the clip, if it is not, and notes what was taken hold of). From then on, in each frame of the
//! window until the button is let go, the clips are moved towards where the pointer is: along the
//! timeline to the frame it is over (as far as the clips next to them leave room), and up and down to
//! the track it is over, if there is room for them there. They all go the same number of frames and
//! tracks, so that they keep their places in relation to one another. See [`crate::application::clip_moving`]
//! for how far that is.
//!
//! The drag is worked out here, once for all the strips, from the pointer and not from the clip that was
//! taken hold of: that clip is drawn in another strip once it has gone to another track.
//!
//! A clip that is dragged to another track is held to the time it started at, if it is near that time (see
//! [`crate::application::clip_moving::StartTimeMemory`]): the pointer never goes straight down.

use egui::{CursorIcon, Rect, Ui};

use super::strip_view::StripView;
use super::{TimelineContent, TimelineViewState};
use crate::application::clip_moving::{MoveBy, allowed_move_holding, moves_by};
use crate::application::commands::Command;
use crate::project::Track;

/// If clips are being dragged, adds the command that moves them towards the pointer, as far as that can
/// be done: `rows` are the tracks with where their strips are on the screen, in the order of the tracks, and
/// `visible_rows` is the part of the screen that rows can be seen in.
pub(super) fn keep_dragging(
    ui: &Ui,
    content: &TimelineContent<'_>,
    state: &mut TimelineViewState,
    rows: &[(&Track, StripView)],
    visible_rows: Rect,
    commands: &mut Vec<Command>,
) {
    let Some(drag) = state.clip_drag.as_mut() else {
        return;
    };
    // A drag ends with the button, whichever of the strips the pointer is over, or none.
    if !content.can_edit || !ui.input(|input| input.pointer.primary_down()) {
        state.clip_drag = None;
        return;
    }
    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) else {
        return;
    };
    let (Some(grabbed), Some(grabbed_row), Some((_, view))) = (
        content.project.placement_of(drag.grabbed),
        content.project.track_position_holding(drag.grabbed),
        rows.first(),
    ) else {
        return;
    };

    let frame_under = |screen_x: f32| view.frame_at(screen_x).map_or(0, |frame| frame.value());
    let frames_the_pointer_has_gone = frame_under(pointer.x) as i64 - drag.pointer_frame_at_the_start as i64;
    let frames_the_clip_has_gone = grabbed.first_timeline_frame.value() as i64 - drag.start_time.start() as i64;
    let tracks_the_pointer_is_down =
        row_nearest_to(rows, visible_rows, pointer.y).map_or(0, |row| row as i64 - grabbed_row as i64);

    let points_per_frame = view.points_per_frame();
    drag.start_time.note_where_the_clip_is(
        content.project.tracks()[grabbed_row].id(),
        grabbed.first_timeline_frame.value(),
        points_per_frame,
    );
    let by = allowed_move_holding(
        content.project,
        &drag.moving,
        MoveBy {
            tracks_down: tracks_the_pointer_is_down,
            frames: frames_the_pointer_has_gone - frames_the_clip_has_gone,
        },
        drag.start_time.snap(drag.grabbed, points_per_frame),
    );
    if by != MoveBy::default() {
        commands.push(Command::MoveClips(moves_by(content.project, &drag.moving, by)));
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
    use crate::project::{FrameRate, TrackId};

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
}
