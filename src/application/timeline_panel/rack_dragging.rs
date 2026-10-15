// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Dragging racks of keyframes: where the pointer takes them.
//!
//! A drag is begun by pressing a rack (see the `racks` module, which notes what was taken hold of): the rack that was pressed, and
//! the others that are selected with it, if they are all in its drawer and nothing else is selected. From then on, in each frame of
//! the window until the button is let go, the racks are moved to where the pointer is: into the drawer (or the keyframe crate) that
//! they are over, or if they are over an empty stretch of a track, into a new crate, which is as long as the keyframes in them are
//! (and which is made as long as they need again as they go on being dragged, in the crate that they are in then: see
//! `Project::fit_the_crate`). In a drawer they go to the place among the racks that are in it that the pointer is at: they have above
//! them the racks that the pointer is below the middle of, so that they change places with the rack that the pointer goes past,
//! in the drawer they are in as in another (several racks that are dragged together are only moved along the timeline in the
//! drawer they are in, and do not change places in it: they go to a place among the racks of another drawer, as a block). They go
//! along the timeline as far as the pointer does (as far as the drawer they are over has room for), so that they can be put where
//! they are wanted. Racks are as long as the drawer they are in, so they cannot go where the first of all their keyframes and the
//! last would have to be closer together than they are: over such a place the words "not enough space here" are shown by the
//! pointer, and the racks stay where they were last put.

use egui::{CursorIcon, Pos2, Ui};

use super::racks::{DropTarget, top_of_the_rack};
use super::strip_view::StripView;
use super::{TimelineContent, TimelineViewState};
use crate::application::commands::Command;
use crate::application::theme;
use crate::project::{AnimationTarget, Clip, EditError, Project, RackDestination, RackId, Room, Track};

/// What is shown by the pointer when the rack cannot go where it is over because it is not long enough there for it.
pub const NOT_ENOUGH_SPACE_MESSAGE: &str = "not enough space here";

/// Where a rack goes when it is let go: where, and at what place among the racks that are there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Landing {
    pub(super) destination: RackDestination,
    /// How many racks it has above it in the drawer it goes to (not counting itself, if it is in that drawer already). `None`
    /// for a new crate, where it is alone, and for racks that stay in the crate they are in, where they stay at the place they have.
    pub(super) slot: Option<usize>,
}

/// How many racks racks that are dragged have above them in a drawer, when the pointer is at height `pointer_y`: as many of the racks
/// that are in the drawer as are above the pointer by the middle of them. (The drawer has `rack_count` racks, the first at
/// `first_rack_top`, and the racks that are dragged are the ones at `dragged_places` of them if they are in the drawer: they are not
/// counted, as they are what is moving.)
pub(super) fn slot_for_the_pointer(
    first_rack_top: f32,
    rack_count: usize,
    dragged_places: &[usize],
    pointer_y: f32,
) -> usize {
    (0..rack_count)
        .filter(|place| !dragged_places.contains(place))
        .filter(|place| top_of_the_rack(first_rack_top, *place) + theme::RACK_HEIGHT / 2.0 < pointer_y)
        .count()
}

/// Where the racks `racks`, which are let go at `pointer`, go: into the drawer (or crate) that the pointer is over, of those that
/// are open (`drop_targets`, the last of them being the one that is on top), at the place among its racks that the pointer is at,
/// or if it is over an empty stretch of a track (in `rows`) into a new crate, somewhere in that stretch, as far as the finished
/// video lasts. (Racks that are in a crate of that track already stay in it while the pointer is over the empty frames round it,
/// which is the room it has: it is made longer, or begins earlier, where they are taken, and no other crate is made where the
/// pointer is.) `None` if it is over nothing racks can go to.
pub(super) fn landing_under(
    pointer: Pos2,
    drop_targets: &[DropTarget],
    rows: &[(&Track, StripView)],
    project: &Project,
    racks: &[RackId],
) -> Option<Landing> {
    if let Some(target) = drop_targets
        .iter()
        .rev()
        .find(|target| target.rectangle.contains(pointer))
    {
        let dragged_places: Vec<usize> = match target.destination {
            RackDestination::Drawer(owner) => project
                .drawer_of(owner)
                .map(|drawer| racks.iter().filter_map(|rack| drawer.position_of(*rack)).collect())
                .unwrap_or_default(),
            RackDestination::NewCrate { .. } => Vec::new(),
        };
        return Some(Landing {
            destination: target.destination,
            slot: Some(slot_for_the_pointer(
                target.first_rack_top,
                target.rack_count,
                &dragged_places,
                pointer.y,
            )),
        });
    }
    let (track, view) = rows.iter().find(|(_, view)| view.rectangle.contains(pointer))?;
    let frame = view.frame_at(pointer.x)?;
    if let Some(crate_clip) = crate_of_the_track_that_holds(racks, project, track)
        && project
            .extent_of_the_drawer_of(AnimationTarget::Clip(crate_clip))
            .is_some_and(|extent| extent.room.contains(frame.value()))
    {
        return Some(Landing {
            destination: RackDestination::Drawer(AnimationTarget::Clip(crate_clip)),
            slot: None,
        });
    }
    match track.room_at(frame) {
        Room::Free {
            from_frame,
            until_frame,
        } => {
            // (A crate does not make the video longer: it is made in the part of the empty stretch that is in it.)
            let until_frame = project.end_of_the_room_for_a_crate_in_a_stretch(until_frame);
            (from_frame < until_frame).then_some(Landing {
                destination: RackDestination::NewCrate {
                    track: track.id(),
                    from_frame,
                    until_frame,
                },
                slot: None,
            })
        }
        Room::Occupied(_) => None,
    }
}

/// The keyframe crate on `track` that all of the racks `racks` are in, if there is one.
fn crate_of_the_track_that_holds(racks: &[RackId], project: &Project, track: &Track) -> Option<crate::project::ClipId> {
    let mut owners = racks
        .iter()
        .filter_map(|rack| project.located_rack(*rack))
        .map(|located| located.owner);
    let AnimationTarget::Clip(clip) = owners.next()? else {
        return None;
    };
    let is_a_crate_of_this_track = track
        .placement_of(clip)
        .is_some_and(|placement| matches!(placement.clip, Clip::Crate(_)));
    (is_a_crate_of_this_track && owners.all(|owner| owner == AnimationTarget::Clip(clip))).then_some(clip)
}

/// Whether a rack that is in the drawer of `owner`, at the place `place_now` among its racks, has to be moved to go where the pointer
/// wants it (`landing`) `shift` frames along: not if it is in the drawer it is wanted in, at the place it is wanted at (or where it is,
/// when no place is said), and no frames are wanted. (Moving it would change nothing, and a command is made in every frame of a drag.)
fn has_to_be_moved(landing: Landing, owner: AnimationTarget, place_now: Option<usize>, shift: i64) -> bool {
    let is_in_it_already = landing.destination == RackDestination::Drawer(owner);
    let is_at_the_place_already = is_in_it_already && (landing.slot.is_none() || landing.slot == place_now);
    !is_at_the_place_already || shift != 0
}

/// If racks are being dragged, adds the command that moves them towards the pointer, as far as that can be done, and says by
/// the pointer when they cannot go where they are over because there is not room.
pub(super) fn keep_dragging(
    ui: &Ui,
    content: &TimelineContent<'_>,
    state: &mut TimelineViewState,
    rows: &[(&Track, StripView)],
    commands: &mut Vec<Command>,
) {
    let Some(drag) = state.rack_drag.clone() else {
        return;
    };
    // A drag ends with the button.
    if !content.can_edit || !ui.input(|input| input.pointer.primary_down()) {
        state.rack_drag = None;
        return;
    }
    let project = content.project;
    let located: Vec<_> = drag
        .racks
        .iter()
        .filter_map(|rack| project.located_rack(*rack))
        .collect();
    let (Some(first_of_them), Some(pointer), Some((_, view))) = (
        located.first(),
        ui.input(|input| input.pointer.interact_pos()),
        rows.first(),
    ) else {
        return;
    };
    let owner = first_of_them.owner;
    let Some(first_frame_now) = located
        .iter()
        .filter_map(|located| located.rack.first_frame().map(|frame| located.timeline_frame_of(frame)))
        .min()
    else {
        return;
    };
    // Where its first keyframe is wanted: as far from where it was when it was taken hold of as the pointer has gone.
    let frames_the_pointer_has_gone =
        view.frame_at(pointer.x).map_or(0, |frame| frame.value()) as i64 - drag.pointer_frame_at_the_start as i64;
    let wanted_shift =
        drag.first_keyframe_frame_at_the_start as i64 + frames_the_pointer_has_gone - first_frame_now as i64;

    let Some(mut landing) = landing_under(pointer, &state.drop_targets, rows, project, &drag.racks) else {
        ui.ctx().set_cursor_icon(CursorIcon::NotAllowed);
        return;
    };
    // Several racks that are dragged together are only moved along the timeline in the drawer they are in: they do not change
    // places in it (they are one place among the racks of another drawer, together, wherever they are let go).
    let several_stay_where_they_are = drag.racks.len() > 1 && landing.destination == RackDestination::Drawer(owner);
    if several_stay_where_they_are {
        landing.slot = None;
    }
    match project.shift_that_fits(&drag.racks, landing.destination, wanted_shift) {
        Ok(shift) => {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
            // (Where the first of the racks is in the drawer it is in: it is only a place that is asked for when it is one rack that is
            // dragged, as several that are dragged together keep to their places, and go to a drawer as one block.)
            let place_now = drag
                .racks
                .first()
                .and_then(|rack| project.drawer_of(owner).and_then(|drawer| drawer.position_of(*rack)));
            if has_to_be_moved(landing, owner, place_now, shift) {
                commands.push(Command::MoveRacks {
                    racks: drag.racks.clone(),
                    to: landing.destination,
                    shift_in_frames: shift,
                    slot: landing.slot,
                });
            }
        }
        Err(error) => {
            ui.ctx().set_cursor_icon(CursorIcon::NotAllowed);
            if let Some(message) = message_for_racks_that_cannot_go(&error) {
                state.pointer_message = Some(message.to_owned());
            }
        }
    }
}

/// What is said by the pointer when racks cannot go where it is over, for the reason `error`: that there is not enough space, if that is
/// why. (For any other reason it says nothing: the pointer says that they cannot go, and a word about space would be wrong.)
fn message_for_racks_that_cannot_go(error: &EditError) -> Option<&'static str> {
    matches!(error, EditError::NoRoomForTheKeyframes).then_some(NOT_ENOUGH_SPACE_MESSAGE)
}

#[cfg(test)]
mod tests {
    use egui::{Rect, pos2, vec2};

    use super::*;
    use crate::application::timeline_geometry::TimelineGeometry;
    use crate::project::{ClipId, FrameRate, PropertyId, PropertyValue, TextObject, TimelineFrameIndex, TrackId};
    use crate::test_support::make_a_keyframe;

    /// A drawer whose first rack's top is at 100, with the racks one under another (each 16 tall with 4 between): the middles
    /// of the racks are at 108, 128, 148 and 168.
    const FIRST_TOP: f32 = 100.0;

    #[test]
    fn a_rack_has_as_many_racks_above_it_as_the_pointer_is_below_the_middle_of() {
        let slot = |pointer_y: f32| slot_for_the_pointer(FIRST_TOP, 4, &[], pointer_y);

        assert_eq!(slot(50.0), 0, "above them all");
        assert_eq!(slot(107.9), 0, "above the middle of the first");
        assert_eq!(slot(108.1), 1, "below the middle of the first");
        assert_eq!(slot(127.9), 1);
        assert_eq!(slot(128.1), 2);
        assert_eq!(slot(168.1), 4, "below the middle of the last: all of them are above");
        assert_eq!(slot(9_999.0), 4);
    }

    #[test]
    fn the_rack_that_is_dragged_does_not_count_among_the_racks_it_has_above_it() {
        // The second is the one that is dragged. Its middle is at 128 and is not looked at: only the three others are.
        let slot = |pointer_y: f32| slot_for_the_pointer(FIRST_TOP, 4, &[1], pointer_y);

        assert_eq!(slot(50.0), 0);
        assert_eq!(slot(108.1), 1, "below the middle of the first");
        assert_eq!(
            slot(128.1),
            1,
            "past where its own middle is: still only the first is above"
        );
        assert_eq!(slot(148.1), 2, "below the middle of the third");
        assert_eq!(slot(168.1), 3, "all the others");
    }

    #[test]
    fn a_rack_that_is_dragged_past_the_middle_of_the_next_changes_places_with_it_and_does_not_change_back_at_once() {
        // Three racks: the first is dragged, with the pointer in the middle of it: it has no rack above it. The pointer goes down
        // to just past the middle of the second: now it has one.
        assert_eq!(slot_for_the_pointer(FIRST_TOP, 3, &[0], 108.0), 0);
        assert_eq!(slot_for_the_pointer(FIRST_TOP, 3, &[0], 128.1), 1);
        // The racks are then in the other order (the dragged one second): the pointer at the same height is over the dragged
        // rack, which is where it is, and its slot is the same: nothing changes back.
        assert_eq!(slot_for_the_pointer(FIRST_TOP, 3, &[1], 128.1), 1);
    }

    /// A project with a text object that has three racks in its drawer (its position, its size and how far it is turned, in
    /// that order), and the drawer of it as a place a rack can be put at `first_top` on the screen.
    fn a_drawer_of_three_racks() -> (Project, AnimationTarget, [RackId; 3], DropTarget) {
        let mut project = Project::empty();
        let track = project.tracks()[0].id();
        let (clip, _) = project
            .insert_text(track, TimelineFrameIndex::new(0), TextObject::default(), 100)
            .unwrap();
        let text = AnimationTarget::Clip(clip);
        for (property, value) in [
            (PropertyId::Position, PropertyValue::Pair(0.5, 0.5)),
            (PropertyId::Size, PropertyValue::Pair(0.5, 0.5)),
            (PropertyId::Rotation, PropertyValue::Number(10.0)),
        ] {
            make_a_keyframe(&mut project, text, property, 10, value);
        }
        let racks: Vec<RackId> = project
            .drawer_of(text)
            .unwrap()
            .racks()
            .iter()
            .map(|rack| rack.id())
            .collect();
        let target = DropTarget {
            rectangle: Rect::from_min_max(pos2(0.0, 92.0), pos2(500.0, 170.0)),
            destination: RackDestination::Drawer(text),
            first_rack_top: FIRST_TOP,
            rack_count: 3,
        };
        (project, text, [racks[0], racks[1], racks[2]], target)
    }

    fn landing_at(project: &Project, target: DropTarget, rack: RackId, x: f32, y: f32) -> Option<Landing> {
        landing_under(pos2(x, y), &[target], &[], project, &[rack])
    }

    #[test]
    fn a_rack_that_is_let_go_over_a_drawer_goes_in_it_at_the_place_the_pointer_is_at() {
        let (project, text, [position, size, turn], target) = a_drawer_of_three_racks();
        let nowhere = RackId::new(TrackId::new(9), 9);

        let landing = |rack: RackId, y: f32| landing_at(&project, target, rack, 50.0, y).map(|landing| landing.slot);

        // A rack from another drawer has the racks above it that the pointer is below the middle of.
        assert_eq!(landing(nowhere, 100.0), Some(Some(0)));
        assert_eq!(landing(nowhere, 118.0), Some(Some(1)));
        assert_eq!(landing(nowhere, 150.0), Some(Some(3)), "below all of them");
        // One that is in the drawer is not one of the racks that are above it.
        assert_eq!(landing(position, 150.0), Some(Some(2)));
        assert_eq!(landing(turn, 100.0), Some(Some(0)));
        assert_eq!(landing(size, 100.0), Some(Some(0)));
        assert_eq!(
            landing(size, 140.0),
            Some(Some(1)),
            "above the middle of the last rack, which is not the size"
        );
        assert_eq!(landing(size, 150.0), Some(Some(2)));
        assert_eq!(
            landing_at(&project, target, size, 50.0, 100.0).map(|landing| landing.destination),
            Some(RackDestination::Drawer(text))
        );
    }

    #[test]
    fn a_rack_let_go_where_there_is_no_drawer_and_no_empty_stretch_of_a_track_goes_nowhere() {
        let (project, _, [position, ..], target) = a_drawer_of_three_racks();

        assert_eq!(
            landing_at(&project, target, position, 50.0, 400.0),
            None,
            "below the drawer"
        );
        assert_eq!(landing_at(&project, target, position, 800.0, 100.0), None, "beside it");
    }

    #[test]
    fn of_two_drawers_that_overlap_the_one_that_is_on_top_is_the_one_a_rack_goes_to() {
        let (project, text, [position, ..], target) = a_drawer_of_three_racks();
        let underneath = DropTarget {
            destination: RackDestination::Drawer(AnimationTarget::Track(project.tracks()[0].id())),
            rack_count: 0,
            ..target
        };

        let landing = landing_under(pos2(50.0, 100.0), &[underneath, target], &[], &project, &[position]).unwrap();
        assert_eq!(
            landing.destination,
            RackDestination::Drawer(text),
            "the last is the one on top"
        );
        let landing = landing_under(pos2(50.0, 100.0), &[target, underneath], &[], &project, &[position]).unwrap();
        assert_eq!(landing.destination, underneath.destination, "and the other way round");
        assert_eq!(landing.slot, Some(0), "it has no racks, so none are above");
    }

    #[test]
    fn a_rack_let_go_over_an_empty_stretch_of_a_track_goes_in_a_new_crate_and_has_no_place_among_racks() {
        let (mut project, _, [position, ..], _) = a_drawer_of_three_racks();
        let first_track = project.tracks()[0].id();
        // Another text object, from frame 300, so that the video lasts to 350 and the stretch from 100 to 300 is empty.
        project
            .insert_text(first_track, TimelineFrameIndex::new(300), TextObject::default(), 50)
            .unwrap();
        let track = &project.tracks()[0];
        let geometry = TimelineGeometry::for_picture(FrameRate::new(30, 1).unwrap(), 100, 100, 120.0);
        let view = StripView::new(
            geometry,
            0.0,
            Rect::from_min_size(pos2(100.0, 300.0), vec2(2000.0, 40.0)),
        );
        let rows = [(track, view)];
        // The first text is from frame 0 to 100, and the second from 300.
        let past_the_text = view.screen_x_of_frame(200);

        let landing = landing_under(pos2(past_the_text, 320.0), &[], &rows, &project, &[position]).unwrap();

        assert_eq!(
            landing.destination,
            RackDestination::NewCrate {
                track: first_track,
                from_frame: 100,
                until_frame: 300,
            }
        );
        assert_eq!(landing.slot, None);
        let over_the_text = view.screen_x_of_frame(50);
        assert_eq!(
            landing_under(pos2(over_the_text, 320.0), &[], &rows, &project, &[position]),
            None,
            "over a clip"
        );
    }

    #[test]
    fn a_rack_let_go_over_a_stretch_that_is_one_frame_long_is_put_in_a_new_crate_of_that_one_frame() {
        let (mut project, _, [position, ..], _) = a_drawer_of_three_racks();
        let first_track = project.tracks()[0].id();
        // Another text object from frame 101 to frame 150: the first is from 0 to 100, and the frame 100 is the one that is between them.
        project
            .insert_text(first_track, TimelineFrameIndex::new(101), TextObject::default(), 49)
            .unwrap();
        let track = &project.tracks()[0];
        let geometry = TimelineGeometry::for_picture(FrameRate::new(30, 1).unwrap(), 100, 100, 120.0);
        let view = StripView::new(
            geometry,
            0.0,
            Rect::from_min_size(pos2(100.0, 300.0), vec2(2000.0, 40.0)),
        );
        let rows = [(track, view)];
        let in_the_frame = view.screen_x_of_frame(100) + 1.0;

        let landing = landing_under(pos2(in_the_frame, 320.0), &[], &rows, &project, &[position]);

        assert_eq!(
            landing,
            Some(Landing {
                destination: RackDestination::NewCrate {
                    track: first_track,
                    from_frame: 100,
                    until_frame: 101,
                },
                slot: None,
            })
        );
    }

    #[test]
    fn the_pointer_says_that_there_is_not_enough_space_when_that_is_why_racks_cannot_go_and_nothing_if_it_is_not() {
        assert_eq!(
            message_for_racks_that_cannot_go(&EditError::NoRoomForTheKeyframes),
            Some(NOT_ENOUGH_SPACE_MESSAGE)
        );
        assert_eq!(
            message_for_racks_that_cannot_go(&EditError::UnknownRack {
                rack: RackId::new(TrackId::new(9), 9)
            }),
            None
        );
        assert_eq!(
            message_for_racks_that_cannot_go(&EditError::UnknownTrack { track: TrackId::new(9) }),
            None
        );
    }

    /// The project of [`a_drawer_of_three_racks`] (a text object from frame 0 to frame 100, with the three racks in its drawer) with another
    /// text object from frame 300 to frame 350, so that the video lasts to 350, and between them, in the empty stretch from frame 100 to
    /// frame 300, a keyframe crate from frame 150 to frame 170, with a rack in it of the loudness of the track. Gives the project, the
    /// track, the crate and its rack, the first rack of the text, and the strips of the track.
    fn a_crate_between_two_texts() -> (Project, TrackId, crate::project::ClipId, RackId, RackId, StripView) {
        let (mut project, _, [position, ..], _) = a_drawer_of_three_racks();
        let track = project.tracks()[0].id();
        project
            .insert_text(track, TimelineFrameIndex::new(300), TextObject::default(), 50)
            .unwrap();
        let the_crate = crate::test_support::make_a_crate(
            &mut project,
            track,
            150,
            170,
            AnimationTarget::Track(track),
            PropertyId::Gain,
        );
        let in_the_crate = project.drawer_of(AnimationTarget::Clip(the_crate)).unwrap().racks()[0].id();
        let view = StripView::new(
            TimelineGeometry::for_picture(FrameRate::new(30, 1).unwrap(), 100, 100, 120.0),
            0.0,
            Rect::from_min_size(pos2(100.0, 300.0), vec2(2000.0, 40.0)),
        );
        (project, track, the_crate, in_the_crate, position, view)
    }

    #[test]
    fn racks_that_are_in_a_crate_stay_in_it_while_the_pointer_is_over_the_empty_frames_round_it_and_other_racks_make_a_new_one()
     {
        let (project, track_id, the_crate, in_the_crate, position, view) = a_crate_between_two_texts();
        let rows = [(&project.tracks()[0], view)];
        let landing_at_frame = |frame: u64, rack: RackId| {
            landing_under(
                pos2(view.screen_x_of_frame(frame), 320.0),
                &[],
                &rows,
                &project,
                &[rack],
            )
        };

        // Before the crate, past it, and just before the clip that is after it: it is made longer, there is no other made.
        for frame in [101, 149, 171, 250, 299] {
            assert_eq!(
                landing_at_frame(frame, in_the_crate),
                Some(Landing {
                    destination: RackDestination::Drawer(AnimationTarget::Clip(the_crate)),
                    slot: None,
                }),
                "at frame {frame}"
            );
        }
        assert_eq!(landing_at_frame(50, in_the_crate), None, "over a clip");
        assert_eq!(landing_at_frame(320, in_the_crate), None, "over another");
        // A rack that is not in the crate goes in a new crate in the part of the empty stretch that is there.
        assert_eq!(
            landing_at_frame(250, position),
            Some(Landing {
                destination: RackDestination::NewCrate {
                    track: track_id,
                    from_frame: 171,
                    until_frame: 300,
                },
                slot: None,
            })
        );
    }

    #[test]
    fn racks_that_are_in_a_crate_with_a_rack_that_is_not_dragged_stay_in_it_too() {
        let (mut project, track_id, the_crate, in_the_crate, _, view) = a_crate_between_two_texts();
        // (Another rack, of another property, in the same crate.)
        let other = project
            .add_keyframe(
                AnimationTarget::Track(track_id),
                PropertyId::Muted,
                TimelineFrameIndex::new(160),
            )
            .unwrap()
            .unwrap()
            .rack;
        project
            .move_racks(
                &[other],
                RackDestination::Drawer(AnimationTarget::Clip(the_crate)),
                0,
                None,
            )
            .unwrap();
        assert_eq!(project.drawer_of(AnimationTarget::Clip(the_crate)).unwrap().len(), 2);
        let rows = [(&project.tracks()[0], view)];

        let landing = landing_under(
            pos2(view.screen_x_of_frame(250), 320.0),
            &[],
            &rows,
            &project,
            &[in_the_crate],
        );

        assert_eq!(
            landing.map(|landing| landing.destination),
            Some(RackDestination::Drawer(AnimationTarget::Clip(the_crate)))
        );
    }

    #[test]
    fn a_rack_is_moved_unless_it_is_in_the_drawer_it_is_wanted_in_at_the_place_it_is_wanted_at_with_no_frames_to_go() {
        let owner = AnimationTarget::Clip(ClipId::new(TrackId::new(0), 0));
        let other = AnimationTarget::Clip(ClipId::new(TrackId::new(0), 1));
        let landing = |destination, slot| Landing { destination, slot };
        let here = RackDestination::Drawer(owner);

        assert!(
            !has_to_be_moved(landing(here, Some(2)), owner, Some(2), 0),
            "where it is, and staying"
        );
        assert!(
            !has_to_be_moved(landing(here, None), owner, Some(2), 0),
            "where it is, with no place said (it keeps the one it has), and staying"
        );
        assert!(
            has_to_be_moved(landing(here, Some(2)), owner, Some(2), 3),
            "where it is, but frames to go"
        );
        assert!(
            has_to_be_moved(landing(here, None), owner, Some(2), -3),
            "with no place said, but frames to go"
        );
        assert!(has_to_be_moved(landing(here, Some(2)), owner, Some(2), -1));
        assert!(
            has_to_be_moved(landing(here, Some(0)), owner, Some(2), 0),
            "another place in the drawer"
        );
        assert!(
            has_to_be_moved(landing(RackDestination::Drawer(other), Some(2)), owner, Some(2), 0),
            "another drawer, at the place that it has in this one"
        );
        let a_new_crate = RackDestination::NewCrate {
            track: TrackId::new(0),
            from_frame: 0,
            until_frame: 10,
        };
        assert!(
            has_to_be_moved(landing(a_new_crate, None), owner, Some(2), 0),
            "a new crate"
        );
    }

    #[test]
    fn a_drawer_with_no_racks_has_none_above_a_rack_that_is_put_in_it() {
        assert_eq!(slot_for_the_pointer(FIRST_TOP, 0, &[], 500.0), 0);
        assert_eq!(
            slot_for_the_pointer(FIRST_TOP, 1, &[0], 500.0),
            0,
            "the only rack is the dragged one"
        );
    }
}
