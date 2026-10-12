//! Dragging clips and keyframes: where the pointer takes them.
//!
//! A drag is begun by pressing a clip or a keyframe and moving the pointer a little (see the `strip` and `racks` modules,
//! which select what was pressed, if it is not selected, and note what was taken hold of). From then on, in each frame of
//! the window until the button is let go, what is dragged is moved towards where the pointer is: along the timeline to the
//! frame it is over, and up and down to the track (for clips) or the rack (for keyframes) it is over. Everything that is
//! dragged goes the same number of frames, so that it all keeps its place in time in relation to the rest. What is dragged is
//! the selected clips and the selected keyframes, if the one that was taken hold of is one of them, and otherwise it alone
//! (with Ctrl, as well as what is selected). What was taken hold of says what goes up and down: the clips go to other tracks
//! when a clip was taken hold of, and the keyframes to other racks when a keyframe was; what is dragged with it goes along the
//! timeline only.
//!
//! * Clips are moved as far as the clips next to them leave room (see [`crate::application::clip_moving`]), and go to the
//!   track the pointer is over if there is room for them there.
//! * Keyframes go as far as the drawer they are in has frames for, and stop short of the keyframes of their rack that are not
//!   dragged (see [`crate::application::keyframe_moving`]), and go to the rack of the same property of the same thing that the
//!   pointer is over, if they fit in it. A keyframe in the drawer of a clip that is dragged is not dragged on its own: it
//!   goes with the clip, where it is in relation to it.
//!
//! The drag is worked out here, once for all the strips, from the pointer and not from what was taken hold of: a clip is
//! drawn in another strip once it has gone to another track, and a keyframe in another rack once it has gone to another.
//!
//! What is dragged to another track or rack is held to the time it started at, if it is near that time (see
//! [`crate::application::start_time_hold`]): the pointer never goes straight down.

use egui::{CursorIcon, Pos2, Rect, Ui, vec2};

use super::strip_view::StripView;
use super::view_state::Grabbed;
use super::{TimelineContent, TimelineViewState};
use crate::application::clip_moving::{MoveBy, Snap, allowed_move_holding, moves_by};
use crate::application::commands::Command;
use crate::application::keyframe_moving::{self, Hold};
use crate::application::theme;
use crate::project::{AnimationTarget, ClipId, KeyframeId, KeyframeMove, Project, RackId, Track};

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
            let owner = project.locate_keyframe(*keyframe).map(|located| located.rack.owner);
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
    let points_per_frame = view.points_per_frame();
    let frame_under = |screen_x: f32| view.frame_at(screen_x).map_or(0, |frame| frame.value());
    let frames_the_pointer_has_gone = frame_under(pointer.x) as i64 - drag.pointer_frame_at_the_start as i64;
    // Where what was taken hold of is now: how far it has gone (which is not how far the pointer has when something was in its
    // way) is what is left of what the pointer has gone.
    let grabbed_frame_now = match drag.grabbed {
        Grabbed::Clip { clip, .. } => project
            .placement_of(clip)
            .map(|placement| placement.first_timeline_frame.value()),
        Grabbed::Keyframe { keyframe, .. } => project.timeline_frame_of_keyframe(keyframe),
    };
    let Some(grabbed_frame_now) = grabbed_frame_now else {
        return;
    };
    let frames_the_grabbed_has_gone = grabbed_frame_now as i64 - drag.grabbed.frame_at_the_start() as i64;
    let wanted_frames = frames_the_pointer_has_gone - frames_the_grabbed_has_gone;

    let racks_on_the_screen = &state.rack_rectangles;
    let racks_each_can_go_to = |rack: RackId| racks_a_keyframe_in_can_go_to(project, racks_on_the_screen, rack);
    let mut by = MoveBy {
        tracks_down: 0,
        frames: wanted_frames,
    };
    let keyframe_moves = match &mut drag.grabbed {
        Grabbed::Clip { clip, start_time } => {
            let mut tracks_the_pointer_is_down = 0;
            let mut snap = None;
            if let (Some(grabbed_row), Some(placement)) =
                (project.track_position_holding(*clip), project.placement_of(*clip))
            {
                tracks_the_pointer_is_down =
                    row_nearest_to(rows, visible_rows, pointer.y).map_or(0, |row| row as i64 - grabbed_row as i64);
                start_time.note_where_it_is(
                    project.tracks()[grabbed_row].id(),
                    placement.first_timeline_frame.value(),
                    points_per_frame,
                );
                snap = Snap::held_by(*clip, start_time, points_per_frame);
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
            // The keyframes that are dragged with the clips go along the timeline only, in the racks they are in; and if they are
            // stopped sooner than the clips are, so are the clips.
            let along = keyframe_moving::allowed_move_holding(
                project,
                &drag.keyframes,
                &|rack| vec![rack],
                keyframe_moving::MoveBy {
                    racks_down: 0,
                    frames: by.frames,
                },
                None,
            );
            by.frames = along.by.frames;
            along.moves
        }
        Grabbed::Keyframe { keyframe, start_time } => {
            let mut racks_the_pointer_is_down = 0;
            let mut hold = None;
            if drag.keyframes.contains(keyframe)
                && let Some(own_rack) = project.locate_keyframe(*keyframe).map(|located| located.rack.rack.id())
            {
                let reachable = racks_each_can_go_to(own_rack);
                racks_the_pointer_is_down = rack_under(pointer, &reachable, racks_on_the_screen)
                    .zip(reachable.iter().position(|rack| *rack == own_rack))
                    .map_or(0, |(there, here)| there as i64 - here as i64);
                start_time.note_where_it_is(own_rack, grabbed_frame_now, points_per_frame);
                hold = start_time.held_time(points_per_frame).map(|time| Hold {
                    keyframe: *keyframe,
                    time,
                });
            }
            let moved = keyframe_moving::allowed_move_holding(
                project,
                &drag.keyframes,
                &racks_each_can_go_to,
                keyframe_moving::MoveBy {
                    racks_down: racks_the_pointer_is_down,
                    frames: wanted_frames,
                },
                hold,
            );
            by.frames = moved.by.frames;
            // The clips that are dragged with the keyframes go along the timeline only, and if they are stopped sooner than the
            // keyframes are, so are the keyframes.
            let clips_by = allowed_move_holding(
                project,
                &drag.clips,
                MoveBy {
                    tracks_down: 0,
                    frames: by.frames,
                },
                None,
            );
            if clips_by.frames == by.frames {
                moved.moves
            } else {
                by.frames = clips_by.frames;
                keyframe_moving::moves_by(
                    project,
                    &drag.keyframes,
                    &racks_each_can_go_to,
                    keyframe_moving::MoveBy {
                        racks_down: moved.by.racks_down,
                        frames: by.frames,
                    },
                )
            }
        }
    };
    push_the_commands(project, (&drag.clips, by), (&drag.keyframes, keyframe_moves), commands);
}

/// Adds the commands that move the clips `clips` by `by`, and make the moves `moves` of the keyframes `keyframes` (the moves
/// are those that put them where they go: they say nothing if they are not dragged).
///
/// When clips and keyframes are dragged together both commands are made in every frame, even one that does nothing, so that the
/// frames of the drag are all alike and are one edit of the history.
fn push_the_commands(
    project: &Project,
    (clips, by): (&[ClipId], MoveBy),
    (keyframes, moves): (&[KeyframeId], Vec<KeyframeMove>),
    commands: &mut Vec<Command>,
) {
    let both = !clips.is_empty() && !keyframes.is_empty();
    if !clips.is_empty() && (by != MoveBy::default() || both) {
        commands.push(Command::MoveClips(moves_by(project, clips, by)));
    }
    let any_is_moved = moves.iter().any(|moved| {
        project
            .locate_keyframe(moved.keyframe)
            .is_some_and(|located| located.rack.rack.id() != moved.to_rack || located.keyframe.frame != moved.to_frame)
    });
    if !keyframes.is_empty() && (any_is_moved || both) {
        commands.push(Command::MoveKeyframes(moves));
    }
}

/// The racks that a keyframe that is in the rack `rack` can go to: those that are on the screen (`on_the_screen`) and animate the
/// same property of the same thing as it does, one under another, the one at the top first. (It is among them when it is on the
/// screen itself. If it is not, a keyframe in it has nowhere else to go.)
fn racks_a_keyframe_in_can_go_to(project: &Project, on_the_screen: &[(RackId, Rect)], rack: RackId) -> Vec<RackId> {
    let Some(this) = project.located_rack(rack) else {
        return Vec::new();
    };
    let (target, property) = (this.rack.target(), this.rack.property());
    let mut animating_the_same: Vec<(RackId, Rect)> = on_the_screen
        .iter()
        .copied()
        .filter(|(other, _)| {
            project
                .located_rack(*other)
                .is_some_and(|located| located.rack.target() == target && located.rack.property() == property)
        })
        .collect();
    // (Racks that are side by side, in the drawers of two clips, are in the order they are along the strip.)
    animating_the_same.sort_by(|(_, first), (_, second)| {
        first
            .top()
            .total_cmp(&second.top())
            .then(first.left().total_cmp(&second.left()))
    });
    animating_the_same.into_iter().map(|(rack, _)| rack).collect()
}

/// The place among `racks` of the one that `pointer` is over (or a little above or below it: the room between racks is not a
/// place of its own), whose places on the screen are in `on_the_screen`. `None` if it is over none of them.
fn rack_under(pointer: Pos2, racks: &[RackId], on_the_screen: &[(RackId, Rect)]) -> Option<usize> {
    racks.iter().position(|rack| {
        on_the_screen.iter().any(|(other, rectangle)| {
            other == rack
                && rectangle
                    .expand2(vec2(0.0, theme::SPACE_BETWEEN_RACKS))
                    .contains(pointer)
        })
    })
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
    use crate::project::{
        FrameRate, PropertyId, PropertyValue, RackDestination, TextObject, TimelineFrameIndex, TrackId,
    };
    use crate::test_support::make_a_keyframe;

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

    // --- the racks a keyframe is dragged among ---

    /// Two texts, the first from frame 0 and the second from 150, each 100 frames: the turn of the first has two racks, one in
    /// the drawer of the track and one in the drawer of the first, and the turn of the second has one in its own drawer.
    struct Racks {
        project: Project,
        turn_of_the_first_in_the_track: RackId,
        turn_of_the_first_in_the_first: RackId,
        turn_of_the_second: RackId,
    }

    fn racks_of_two_turns() -> Racks {
        let mut project = Project::empty();
        let track = project.tracks()[0].id();
        let (first, _) = project
            .insert_text(track, TimelineFrameIndex::new(0), TextObject::default(), 100)
            .unwrap();
        let (second, _) = project
            .insert_text(track, TimelineFrameIndex::new(150), TextObject::default(), 100)
            .unwrap();
        let (first, second) = (AnimationTarget::Clip(first), AnimationTarget::Clip(second));
        let the_rack_in_the_drawer_of = |project: &Project, owner: AnimationTarget, target: AnimationTarget| {
            project
                .drawer_of(owner)
                .unwrap()
                .first_rack_for(target, PropertyId::Rotation)
                .unwrap()
                .id()
        };
        make_a_keyframe(
            &mut project,
            first,
            PropertyId::Rotation,
            10,
            PropertyValue::Number(1.0),
        );
        let turn_of_the_first_in_the_track = the_rack_in_the_drawer_of(&project, first, first);
        project
            .move_rack(
                turn_of_the_first_in_the_track,
                RackDestination::Drawer(AnimationTarget::Track(track)),
                0,
                None,
            )
            .unwrap();
        make_a_keyframe(
            &mut project,
            first,
            PropertyId::Rotation,
            20,
            PropertyValue::Number(2.0),
        );
        let turn_of_the_first_in_the_first = the_rack_in_the_drawer_of(&project, first, first);
        make_a_keyframe(
            &mut project,
            second,
            PropertyId::Rotation,
            160,
            PropertyValue::Number(3.0),
        );
        let turn_of_the_second = the_rack_in_the_drawer_of(&project, second, second);
        Racks {
            project,
            turn_of_the_first_in_the_track,
            turn_of_the_first_in_the_first,
            turn_of_the_second,
        }
    }

    fn rack_at(left: f32, top: f32, width: f32) -> Rect {
        Rect::from_min_size(pos2(left, top), vec2(width, 16.0))
    }

    #[test]
    fn the_racks_a_keyframe_can_go_to_are_those_on_the_screen_of_the_same_property_of_the_same_thing_the_higher_first()
    {
        let racks = racks_of_two_turns();
        let (in_the_track, in_the_first, of_the_second) = (
            racks.turn_of_the_first_in_the_track,
            racks.turn_of_the_first_in_the_first,
            racks.turn_of_the_second,
        );
        // The rack in the drawer of the first is higher on the screen than the one in the drawer of the track.
        let on_the_screen = [
            (in_the_track, rack_at(0.0, 200.0, 500.0)),
            (in_the_first, rack_at(0.0, 100.0, 100.0)),
            (of_the_second, rack_at(150.0, 100.0, 100.0)),
        ];

        assert_eq!(
            racks_a_keyframe_in_can_go_to(&racks.project, &on_the_screen, in_the_first),
            vec![in_the_first, in_the_track]
        );
        assert_eq!(
            racks_a_keyframe_in_can_go_to(&racks.project, &on_the_screen, in_the_track),
            vec![in_the_first, in_the_track],
            "the same racks whichever of them the keyframe is in"
        );
        assert_eq!(
            racks_a_keyframe_in_can_go_to(&racks.project, &on_the_screen, of_the_second),
            vec![of_the_second],
            "the turn of the other text is not the turn of this one"
        );
    }

    #[test]
    fn racks_that_are_at_one_height_are_in_the_order_they_are_along_the_strip() {
        let racks = racks_of_two_turns();
        let (in_the_track, in_the_first) = (
            racks.turn_of_the_first_in_the_track,
            racks.turn_of_the_first_in_the_first,
        );
        let side_by_side = [
            (in_the_first, rack_at(300.0, 100.0, 100.0)),
            (in_the_track, rack_at(100.0, 100.0, 100.0)),
        ];

        assert_eq!(
            racks_a_keyframe_in_can_go_to(&racks.project, &side_by_side, in_the_first),
            vec![in_the_track, in_the_first]
        );
    }

    #[test]
    fn racks_that_are_not_on_the_screen_are_not_gone_to_and_a_rack_that_is_not_there_has_none() {
        let racks = racks_of_two_turns();
        let in_the_first = racks.turn_of_the_first_in_the_first;
        let nowhere = RackId::new(TrackId::new(9), 9);
        let on_the_screen = [(nowhere, rack_at(0.0, 100.0, 100.0))];

        assert_eq!(
            racks_a_keyframe_in_can_go_to(&racks.project, &on_the_screen, in_the_first),
            Vec::<RackId>::new(),
            "neither is on the screen, and the rack that is on it is not in the project"
        );
        assert_eq!(
            racks_a_keyframe_in_can_go_to(&racks.project, &on_the_screen, nowhere),
            Vec::<RackId>::new()
        );
    }

    #[test]
    fn a_rack_of_another_property_of_the_same_thing_is_not_one_a_keyframe_can_go_to() {
        let mut racks = racks_of_two_turns();
        // The size of the first text has a rack of its own, in the drawer of the first.
        let first = AnimationTarget::Clip(racks.project.tracks()[0].entries()[0].id());
        make_a_keyframe(
            &mut racks.project,
            first,
            PropertyId::Size,
            30,
            PropertyValue::Pair(0.5, 0.5),
        );
        let the_size = racks
            .project
            .drawer_of(first)
            .unwrap()
            .first_rack_for(first, PropertyId::Size)
            .unwrap()
            .id();
        let (in_the_first, in_the_track) = (
            racks.turn_of_the_first_in_the_first,
            racks.turn_of_the_first_in_the_track,
        );
        let on_the_screen = [
            (in_the_first, rack_at(0.0, 100.0, 100.0)),
            (the_size, rack_at(0.0, 120.0, 100.0)),
            (in_the_track, rack_at(0.0, 200.0, 500.0)),
        ];

        assert_eq!(
            racks_a_keyframe_in_can_go_to(&racks.project, &on_the_screen, in_the_first),
            vec![in_the_first, in_the_track],
            "the size is not the turn, though it is between the two racks of the turn on the screen"
        );
        assert_eq!(
            racks_a_keyframe_in_can_go_to(&racks.project, &on_the_screen, the_size),
            vec![the_size]
        );
    }

    #[test]
    fn keyframes_that_are_not_moved_make_no_command_unless_clips_are_dragged_with_them_which_makes_one_of_each_in_every_frame()
     {
        let racks = racks_of_two_turns();
        let rack = racks.turn_of_the_first_in_the_first;
        let keyframe = racks.project.located_rack(rack).unwrap().rack.keyframes()[0].id();
        let clip = racks.project.tracks()[0].entries()[0].id();
        // (The keyframe is at frame 20 of the rack, which is at the start of the timeline.)
        let stays = KeyframeMove {
            keyframe,
            to_rack: rack,
            to_frame: 20,
        };
        let goes = KeyframeMove { to_frame: 25, ..stays };
        let commands_for = |clips: &[ClipId], moves: Vec<KeyframeMove>| {
            let mut commands = Vec::new();
            push_the_commands(
                &racks.project,
                (clips, MoveBy::default()),
                (&[keyframe], moves),
                &mut commands,
            );
            commands
        };

        assert_eq!(commands_for(&[], vec![stays]), vec![], "nothing moves: nothing is sent");
        assert_eq!(commands_for(&[], vec![goes]), vec![Command::MoveKeyframes(vec![goes])]);
        let both = commands_for(&[clip], vec![stays]);
        assert_eq!(
            both.len(),
            2,
            "clips and keyframes dragged together: each is sent in every frame: {both:?}"
        );
        assert!(matches!(both[0], Command::MoveClips(_)), "{both:?}");
        assert!(matches!(both[1], Command::MoveKeyframes(_)), "{both:?}");
    }

    #[test]
    fn the_rack_the_pointer_is_over_is_found_among_the_racks_and_so_is_the_room_a_little_above_and_below_it() {
        let racks = [
            RackId::new(TrackId::new(0), 0),
            RackId::new(TrackId::new(0), 1),
            RackId::new(TrackId::new(0), 2),
        ];
        let on_the_screen = [
            (racks[0], rack_at(0.0, 100.0, 300.0)),
            (racks[1], rack_at(0.0, 120.0, 300.0)),
            // The third is only as wide as a drawer of a clip: from 500 to 600.
            (racks[2], rack_at(500.0, 140.0, 100.0)),
        ];
        let among = [racks[0], racks[1], racks[2]];
        let place = |x: f32, y: f32| rack_under(pos2(x, y), &among, &on_the_screen);

        assert_eq!(place(50.0, 108.0), Some(0));
        assert_eq!(place(50.0, 128.0), Some(1));
        assert_eq!(
            place(50.0, 118.0),
            Some(0),
            "in the room between the first and the second, which the first has as far as it goes (and the second too)"
        );
        assert_eq!(place(50.0, 96.5), Some(0), "a little above the first");
        assert_eq!(place(50.0, 90.0), None, "well above it");
        assert_eq!(place(550.0, 148.0), Some(2));
        assert_eq!(place(450.0, 148.0), None, "beside the third: its drawer is not there");
        assert_eq!(
            rack_under(pos2(50.0, 108.0), &[racks[1]], &on_the_screen),
            None,
            "not among those asked about"
        );
    }
}
