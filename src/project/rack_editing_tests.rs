//! Tests of what is done to racks and to what holds them: moving keyframes, taking them away, moving a rack to another drawer
//! or onto a track, and keyframe crates. (Copying clips and tracks with what is in their drawers is in `duplicating_tests`.)

use super::racks_tests::{
    a_big_video_and_a_small_text, assert_the_racks_are_sound, db, frames_in_the_rack, gain_at,
    names_of_the_keyframes_in, names_of_the_racks_in, put_a_rack_in, where_the_video_has_keyframes,
};
use super::tests::frame;
use super::*;
use crate::test_support::DeterministicRandomNumbers;

/// The move of `keyframe`, which is in the rack `rack`, to frame `to_frame` of the drawer that rack is in: it stays in the rack.
fn within_its_rack(rack: RackId, keyframe: KeyframeId, to_frame: u64) -> KeyframeMove {
    KeyframeMove {
        keyframe,
        to_rack: rack,
        to_frame,
    }
}

/// Each of `keyframes` with the rack it is in now, which is where it is to stay when it is moved in time only. (The names of
/// keyframes that are not there are left out.)
fn in_their_own_racks(project: &Project, keyframes: &[KeyframeId]) -> Vec<KeyframeDestination> {
    keyframes
        .iter()
        .filter_map(|keyframe| {
            let located = project.locate_keyframe(*keyframe)?;
            Some(KeyframeDestination {
                keyframe: *keyframe,
                rack: located.rack.rack.id(),
            })
        })
        .collect()
}

/// How far the keyframes can be dragged all together when `wanted` is how far, each staying in the rack it is in.
fn the_shift_allowed(project: &Project, keyframes: &[KeyframeId], wanted: i64) -> i64 {
    project.allowed_shift_of_keyframes(&in_their_own_racks(project, keyframes), wanted)
}

/// The moves that put each of the keyframes `shift` frames later than it is, in the rack it is in.
fn moves_by(project: &Project, keyframes: &[KeyframeId], shift: i64) -> Vec<KeyframeMove> {
    project.moves_of_keyframes_by(&in_their_own_racks(project, keyframes), shift)
}

// --- moving keyframes ---

#[test]
fn keyframes_are_moved_to_the_frames_given_in_their_own_drawer_and_keep_their_names() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let rack = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(10, db(-20.0)), (40, db(0.0))],
    );
    let names = names_of_the_keyframes_in(&project, rack);

    project
        .move_keyframes(&[within_its_rack(rack, names[0], 20), within_its_rack(rack, names[1], 59)])
        .unwrap();

    assert_eq!(frames_in_the_rack(&project, rack), vec![20, 59]);
    assert_eq!(
        names_of_the_keyframes_in(&project, rack),
        names,
        "the same keyframes, in the same order"
    );
    assert_eq!(
        where_the_video_has_keyframes(&project, video, PropertyId::Gain),
        vec![120, 159]
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn what_a_property_is_follows_its_keyframes_when_they_are_moved() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let rack = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(10, db(-20.0)), (40, db(0.0))],
    );
    let names = names_of_the_keyframes_in(&project, rack);

    project.move_keyframes(&[within_its_rack(rack, names[0], 20)]).unwrap();

    assert_eq!(
        gain_at(&project, big, 110),
        -20.0,
        "before the first it is what the first says"
    );
    assert_eq!(gain_at(&project, big, 120), -20.0);
    assert_eq!(gain_at(&project, big, 130), -10.0);
    assert_eq!(gain_at(&project, big, 140), 0.0);
}

#[test]
fn keyframes_that_cannot_all_be_moved_where_they_are_asked_to_go_leave_everything_as_it_was() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let rack = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(10, db(-20.0)), (40, db(0.0))],
    );
    let names = names_of_the_keyframes_in(&project, rack);
    let before = project.clone();

    // The drawer of the text lasts 60 frames: it has frames 0 to 59, and 60 is the first it does not have.
    let past_the_end =
        project.move_keyframes(&[within_its_rack(rack, names[0], 12), within_its_rack(rack, names[1], 60)]);
    assert_eq!(past_the_end, Err(EditError::NoRoomForTheKeyframes));
    assert_eq!(project, before, "the first was not moved either");

    let onto_another = project.move_keyframes(&[within_its_rack(rack, names[0], 40)]);
    assert_eq!(onto_another, Err(EditError::Rack(RackEditError::FramesWouldCoincide)));
    assert_eq!(project, before);

    let nobody = KeyframeId::new(RackId::new(TrackId::new(50), 5), 0);
    let not_there = project.move_keyframes(&[within_its_rack(rack, nobody, 1)]);
    assert_eq!(not_there, Err(EditError::Rack(RackEditError::NoSuchKeyframe)));
    let not_in_the_rack = project.move_keyframes(&[within_its_rack(rack, KeyframeId::new(rack, 77), 1)]);
    assert_eq!(not_in_the_rack, Err(EditError::Rack(RackEditError::NoSuchKeyframe)));
    assert_eq!(project, before);
}

#[test]
fn keyframes_of_several_racks_are_moved_together_or_not_at_all() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let owner = AnimationTarget::Clip(text);
    let big = AnimationTarget::Clip(video);
    let loudness = put_a_rack_in(&mut project, owner, big, PropertyId::Gain, &[(10, db(-20.0))]);
    let switch = put_a_rack_in(
        &mut project,
        owner,
        big,
        PropertyId::Muted,
        &[(5, PropertyValue::Flag(true))],
    );
    let (first, second) = (
        names_of_the_keyframes_in(&project, loudness)[0],
        names_of_the_keyframes_in(&project, switch)[0],
    );
    let before = project.clone();

    let one_of_them_too_far = project.move_keyframes(&[
        within_its_rack(loudness, first, 15),
        within_its_rack(switch, second, 600),
    ]);
    assert_eq!(one_of_them_too_far, Err(EditError::NoRoomForTheKeyframes));
    assert_eq!(project, before);

    project
        .move_keyframes(&[
            within_its_rack(loudness, first, 15),
            within_its_rack(switch, second, 10),
        ])
        .unwrap();
    assert_eq!(frames_in_the_rack(&project, loudness), vec![15]);
    assert_eq!(frames_in_the_rack(&project, switch), vec![10]);
}

#[test]
fn the_keyframes_in_the_drawer_of_a_track_can_be_moved_as_far_along_the_timeline_as_wanted() {
    let (mut project, video, _) = a_big_video_and_a_small_text();
    let track = AnimationTarget::Track(project.tracks()[0].id());
    let rack = put_a_rack_in(
        &mut project,
        track,
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(10, db(-20.0))],
    );
    let name = names_of_the_keyframes_in(&project, rack)[0];

    project.move_keyframes(&[within_its_rack(rack, name, 5_000)]).unwrap();

    assert_eq!(frames_in_the_rack(&project, rack), vec![5_000]);
}

// --- how far keyframes can be dragged ---

/// A video and, in front of it, a text object from frame 100 to 160 whose drawer has a rack of the loudness of the video
/// with keyframes at these frames of its own (counted from the start of the text), and one more rack with a keyframe at 30.
fn a_drawer_with_a_rack_of_keyframes_at(frames: &[u64]) -> (Project, RackId, Vec<KeyframeId>, AnimationTarget) {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let owner = AnimationTarget::Clip(text);
    let keyframes: Vec<(u64, PropertyValue)> = frames.iter().map(|frame| (*frame, db(-(*frame as f32)))).collect();
    let rack = put_a_rack_in(
        &mut project,
        owner,
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &keyframes,
    );
    let names = names_of_the_keyframes_in(&project, rack);
    (project, rack, names, owner)
}

#[test]
fn keyframes_can_be_dragged_as_far_as_wanted_when_nothing_is_in_the_way() {
    let (project, _, names, _) = a_drawer_with_a_rack_of_keyframes_at(&[20, 30]);

    assert_eq!(the_shift_allowed(&project, &names, 5), 5);
    assert_eq!(the_shift_allowed(&project, &names, -5), -5);
    assert_eq!(the_shift_allowed(&project, &names, 0), 0);
    assert_eq!(
        the_shift_allowed(&project, &[], 7),
        7,
        "nothing is moving, so nothing is in the way"
    );
}

#[test]
fn keyframes_that_are_dragged_stop_at_the_ends_of_their_drawer() {
    // The drawer of the text has the frames 0 to 59.
    let (project, _, names, _) = a_drawer_with_a_rack_of_keyframes_at(&[20, 30]);

    assert_eq!(
        the_shift_allowed(&project, &names, 100),
        29,
        "the last goes to frame 59"
    );
    assert_eq!(
        the_shift_allowed(&project, &names, -100),
        -20,
        "the first goes to frame 0"
    );
    assert_eq!(the_shift_allowed(&project, &names[1..], 100), 29);
    assert_eq!(the_shift_allowed(&project, &names[..1], -100), -20);
}

#[test]
fn keyframes_that_are_dragged_stop_short_of_a_keyframe_that_is_not_moving_and_do_not_pass_it() {
    let (project, _, names, _) = a_drawer_with_a_rack_of_keyframes_at(&[10, 20, 40]);

    // The middle one is dragged: it stops one frame before the one at 40, and one frame after the one at 10.
    assert_eq!(the_shift_allowed(&project, &names[1..2], 100), 19);
    assert_eq!(the_shift_allowed(&project, &names[1..2], -100), -9);
    // The first two together: the second stops before the third, and the first has all the room before it.
    assert_eq!(the_shift_allowed(&project, &names[..2], 100), 19);
    assert_eq!(the_shift_allowed(&project, &names[..2], -100), -10);
    // The first and the last are dragged and the middle one is in the way of both.
    let ends = [names[0], names[2]];
    assert_eq!(
        the_shift_allowed(&project, &ends, 100),
        9,
        "the first stops before the middle one"
    );
    assert_eq!(
        the_shift_allowed(&project, &ends, -100),
        -10,
        "the first goes to frame 0, which is before the last would be stopped by the middle one"
    );
}

#[test]
fn keyframes_that_are_dragged_together_keep_their_places_in_relation_to_each_other() {
    let (mut project, rack, names, _) = a_drawer_with_a_rack_of_keyframes_at(&[10, 12, 14]);

    let shift = the_shift_allowed(&project, &names, 100);
    let moves = moves_by(&project, &names, shift);
    project.move_keyframes(&moves).unwrap();

    assert_eq!(shift, 45, "the last is stopped by the end of the drawer, at frame 59");
    assert_eq!(frames_in_the_rack(&project, rack), vec![55, 57, 59]);
    assert_eq!(
        names_of_the_keyframes_in(&project, rack),
        names,
        "the same keyframes in the same order"
    );
}

#[test]
fn keyframes_of_several_racks_that_are_dragged_are_stopped_by_whichever_is_stopped_first() {
    let (mut project, _, mut names, owner) = a_drawer_with_a_rack_of_keyframes_at(&[10, 20]);
    let video = project.tracks()[1].entries()[0].id();
    let other = put_a_rack_in(
        &mut project,
        owner,
        AnimationTarget::Clip(video),
        PropertyId::Muted,
        &[(40, PropertyValue::Flag(true)), (45, PropertyValue::Flag(false))],
    );
    let in_the_other_rack = names_of_the_keyframes_in(&project, other);
    names.extend(in_the_other_rack.iter().copied());

    // The keyframes of the first rack could go on, but the last of the other rack is at 45 and the drawer ends at 59.
    assert_eq!(the_shift_allowed(&project, &names, 100), 14);
    assert_eq!(the_shift_allowed(&project, &names, -100), -10);
}

#[test]
fn the_keyframes_in_the_drawer_of_a_track_can_be_dragged_as_far_along_the_timeline_as_wanted_but_not_before_its_start()
{
    let (mut project, video, _) = a_big_video_and_a_small_text();
    let track = AnimationTarget::Track(project.tracks()[0].id());
    let rack = put_a_rack_in(
        &mut project,
        track,
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(100, db(-1.0)), (130, db(-2.0))],
    );
    let names = names_of_the_keyframes_in(&project, rack);

    assert_eq!(the_shift_allowed(&project, &names, 100_000), 100_000);
    assert_eq!(the_shift_allowed(&project, &names, -100_000), -100);
}

#[test]
fn what_is_dragged_can_always_be_moved_by_the_shift_that_is_allowed_and_not_by_one_frame_more() {
    for seed in 1..=150u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed);
        let (mut project, video, text) = a_big_video_and_a_small_text();
        let big = AnimationTarget::Clip(video);
        let owner = AnimationTarget::Clip(text);
        let mut all_names = Vec::new();
        for property in [PropertyId::Gain, PropertyId::Muted, PropertyId::Rotation] {
            let mut frames: Vec<u64> = (0..1 + random.next_below(5)).map(|_| random.next_below(60)).collect();
            frames.sort_unstable();
            frames.dedup();
            let value = match property {
                PropertyId::Muted => PropertyValue::Flag(true),
                PropertyId::Rotation => PropertyValue::Number(5.0),
                _ => db(-3.0),
            };
            let keyframes: Vec<(u64, PropertyValue)> = frames.iter().map(|frame| (*frame, value.clone())).collect();
            let rack = put_a_rack_in(&mut project, owner, big, property, &keyframes);
            all_names.extend(names_of_the_keyframes_in(&project, rack));
        }
        // A random choice of what is dragged.
        let moving: Vec<KeyframeId> = all_names
            .iter()
            .copied()
            .filter(|_| random.next_below(2) == 0)
            .collect();
        let wanted = random.next_below(121) as i64 - 60;

        let allowed = the_shift_allowed(&project, &moving, wanted);

        let context = format!(
            "seed {seed}: {} of {} dragged, {wanted} wanted, {allowed} allowed",
            moving.len(),
            all_names.len()
        );
        assert!(allowed.abs() <= wanted.abs() && allowed * wanted >= 0, "{context}");
        let mut moved = project.clone();
        let moves = moves_by(&project, &moving, allowed);
        assert_eq!(moves.len(), moving.len(), "{context}: all of them can be moved");
        moved
            .move_keyframes(&moves)
            .unwrap_or_else(|error| panic!("{context}: {error}"));
        assert_eq!(moved.problems_with_the_racks(), Vec::<String>::new(), "{context}");
        if allowed != wanted {
            let one_more = allowed + wanted.signum();
            let goes_before_the_start = moving.iter().any(|keyframe| {
                let frame = project.locate_keyframe(*keyframe).unwrap().keyframe.frame;
                frame as i64 + one_more < 0
            });
            let moves = moves_by(&project, &moving, one_more);
            let result = project.clone().move_keyframes(&moves);
            assert!(
                goes_before_the_start || result.is_err(),
                "{context}: one frame more, {one_more}, could have been done"
            );
        }
    }
}

// --- taking keyframes away ---

#[test]
fn keyframes_taken_away_by_their_names_are_gone_and_a_rack_with_none_left_goes() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let owner = AnimationTarget::Clip(text);
    let big = AnimationTarget::Clip(video);
    let rack = put_a_rack_in(
        &mut project,
        owner,
        big,
        PropertyId::Gain,
        &[(10, db(-20.0)), (40, db(-6.0))],
    );
    let names = names_of_the_keyframes_in(&project, rack);

    project.remove_keyframes(&[names[0]]).unwrap();
    assert_eq!(frames_in_the_rack(&project, rack), vec![40]);
    assert!(project.is_animated(big, PropertyId::Gain));

    project.remove_keyframes(&[names[1]]).unwrap();
    assert!(project.drawer_of(owner).unwrap().is_empty(), "the rack has gone");
    assert!(!project.is_animated(big, PropertyId::Gain));
    assert_eq!(
        project.placement_of(video).unwrap().clip.as_media().unwrap().gain(),
        Gain::from_decibels(-6.0),
        "the property stays as the last keyframe said"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_property_that_loses_all_its_keyframes_at_once_stays_as_the_last_of_them_in_time_said_whatever_order_they_go_in() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let rack = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(10, db(-20.0)), (40, db(-6.0))],
    );
    let mut names = names_of_the_keyframes_in(&project, rack);
    names.reverse();

    project.remove_keyframes(&names).unwrap();

    assert_eq!(
        project.placement_of(video).unwrap().clip.as_media().unwrap().gain(),
        Gain::from_decibels(-6.0)
    );
}

#[test]
fn a_property_with_keyframes_in_two_racks_is_still_animated_when_one_rack_loses_all_of_its_own() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let held_by_the_text = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(10, db(-20.0))],
    );
    put_a_rack_in(&mut project, big, big, PropertyId::Gain, &[(300, db(0.0))]);

    project
        .remove_keyframes(&names_of_the_keyframes_in(&project, held_by_the_text))
        .unwrap();

    assert!(project.is_animated(big, PropertyId::Gain));
    assert_eq!(
        where_the_video_has_keyframes(&project, video, PropertyId::Gain),
        vec![300]
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_keyframe_that_is_not_there_is_left_alone_when_others_are_taken_away() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let rack = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(10, db(-20.0)), (40, db(0.0))],
    );
    let names = names_of_the_keyframes_in(&project, rack);
    let nobody = KeyframeId::new(rack, 99);
    let in_a_rack_that_is_not_there = KeyframeId::new(RackId::new(TrackId::new(50), 0), 0);

    project
        .remove_keyframes(&[nobody, in_a_rack_that_is_not_there, names[0], names[0]])
        .unwrap();

    assert_eq!(frames_in_the_rack(&project, rack), vec![40]);
}

#[test]
fn a_crate_whose_last_keyframe_is_taken_away_goes_with_its_last_rack_and_one_with_another_rack_stays() {
    let (mut project, video, _) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let front = project.tracks()[0].id();
    let crate_id = project.insert_crate(front, frame(300), 100).unwrap();
    let inside = AnimationTarget::Clip(crate_id);
    let loudness = put_a_rack_in(&mut project, inside, big, PropertyId::Gain, &[(10, db(-20.0))]);
    let switch = put_a_rack_in(
        &mut project,
        inside,
        big,
        PropertyId::Muted,
        &[(5, PropertyValue::Flag(true))],
    );

    project
        .remove_keyframes(&names_of_the_keyframes_in(&project, loudness))
        .unwrap();
    assert!(
        project.placement_of(crate_id).is_some(),
        "it still holds the other rack"
    );
    assert_eq!(names_of_the_racks_in(&project, inside), vec![switch]);

    project
        .remove_keyframes(&names_of_the_keyframes_in(&project, switch))
        .unwrap();
    assert!(project.placement_of(crate_id).is_none(), "nothing is left in it");
    assert!(project.tracks()[0].entries().iter().all(|entry| entry.id() != crate_id));
    assert_the_racks_are_sound(&project);
}

// --- moving a rack ---

#[test]
fn a_rack_moved_to_another_drawer_keeps_its_keyframes_at_the_frames_of_the_timeline_they_were_at() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    // The drawer of the big video starts at frame 0, so its keyframes are at 110 and 140. The text is from 100 to 160.
    let rack = put_a_rack_in(
        &mut project,
        big,
        big,
        PropertyId::Gain,
        &[(110, db(-20.0)), (140, db(0.0))],
    );
    let before: Vec<f32> = (0..300).map(|at| gain_at(&project, big, at)).collect();

    project
        .move_rack(rack, RackDestination::Drawer(AnimationTarget::Clip(text)), 0, None)
        .unwrap();

    assert_eq!(
        frames_in_the_rack(&project, rack),
        vec![10, 40],
        "counted from the start of the text, which is frame 100"
    );
    assert!(project.drawer_of(big).unwrap().is_empty());
    assert_eq!(names_of_the_racks_in(&project, AnimationTarget::Clip(text)), vec![rack]);
    let after: Vec<f32> = (0..300).map(|at| gain_at(&project, big, at)).collect();
    assert_eq!(before, after, "the video has what it had at every frame");
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_rack_can_be_shifted_in_time_as_it_is_moved_and_its_keyframes_keep_the_distance_between_them() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let rack = put_a_rack_in(
        &mut project,
        big,
        big,
        PropertyId::Gain,
        &[(300, db(-20.0)), (330, db(0.0))],
    );

    // The text is from 100 to 160: with the keyframes 200 frames earlier they are at 100 and 130, which is 0 and 30 into it.
    project
        .move_rack(rack, RackDestination::Drawer(AnimationTarget::Clip(text)), -200, None)
        .unwrap();

    assert_eq!(frames_in_the_rack(&project, rack), vec![0, 30]);
    assert_eq!(
        where_the_video_has_keyframes(&project, video, PropertyId::Gain),
        vec![100, 130]
    );

    // And back to the video, where the keyframes are counted from the start of the timeline, 250 frames later.
    project
        .move_rack(rack, RackDestination::Drawer(big), 250, None)
        .unwrap();
    assert_eq!(frames_in_the_rack(&project, rack), vec![350, 380]);
}

#[test]
fn a_rack_cannot_be_moved_where_its_keyframes_do_not_all_fit_and_nothing_changes() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let owner = AnimationTarget::Clip(text);
    let rack = put_a_rack_in(
        &mut project,
        big,
        big,
        PropertyId::Gain,
        &[(110, db(-20.0)), (240, db(0.0))],
    );
    let before = project.clone();

    // The first is at 110, which is in the text (100 to 160), and the second at 240, which is not.
    assert_eq!(
        project.move_rack(rack, RackDestination::Drawer(owner), 0, None),
        Err(EditError::NoRoomForTheKeyframes)
    );
    // Shifted so that the first is at the text's first frame, they are 130 frames apart, and the text has only 60.
    assert_eq!(
        project.move_rack(rack, RackDestination::Drawer(owner), -10, None),
        Err(EditError::NoRoomForTheKeyframes)
    );
    // Shifted to before the start of the timeline.
    assert_eq!(
        project.move_rack(rack, RackDestination::Drawer(big), -111, None),
        Err(EditError::NoRoomForTheKeyframes)
    );
    assert_eq!(project, before);
    assert_eq!(
        project.check_the_move_of_a_rack(rack, RackDestination::Drawer(owner), 0),
        Err(EditError::NoRoomForTheKeyframes),
        "asking whether it can says no too"
    );
    assert_eq!(project, before);
}

#[test]
fn a_rack_just_fits_in_a_drawer_when_its_first_and_last_keyframes_are_at_its_first_and_last_frames() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let owner = AnimationTarget::Clip(text);
    let rack = put_a_rack_in(
        &mut project,
        big,
        big,
        PropertyId::Gain,
        &[(100, db(-20.0)), (159, db(0.0))],
    );

    assert_eq!(
        project.check_the_move_of_a_rack(rack, RackDestination::Drawer(owner), 1),
        Err(EditError::NoRoomForTheKeyframes),
        "the last would be one frame too late"
    );
    assert_eq!(
        project.check_the_move_of_a_rack(rack, RackDestination::Drawer(owner), -1),
        Err(EditError::NoRoomForTheKeyframes),
        "the first would be one frame too early"
    );
    project
        .move_rack(rack, RackDestination::Drawer(owner), 0, None)
        .unwrap();
    assert_eq!(frames_in_the_rack(&project, rack), vec![0, 59]);
}

#[test]
fn a_rack_moved_to_a_drawer_that_has_a_rack_for_the_same_property_of_the_same_thing_goes_in_beside_it_and_they_are_not_merged()
 {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let owner = AnimationTarget::Clip(text);
    let there_already = put_a_rack_in(&mut project, owner, big, PropertyId::Gain, &[(10, db(-20.0))]);
    let arriving = put_a_rack_in(&mut project, big, big, PropertyId::Gain, &[(120, db(0.0))]);

    project
        .move_rack(arriving, RackDestination::Drawer(owner), 0, None)
        .unwrap();

    assert_eq!(
        names_of_the_racks_in(&project, owner),
        vec![there_already, arriving],
        "two racks: the one that came is under the one that was there"
    );
    assert_eq!(frames_in_the_rack(&project, there_already), vec![10]);
    assert_eq!(
        frames_in_the_rack(&project, arriving),
        vec![20],
        "counted from the start of the text, which is frame 100"
    );
    assert_eq!(
        where_the_video_has_keyframes(&project, video, PropertyId::Gain),
        vec![110, 120],
        "all the keyframes of the loudness of the video are its curve, whichever rack they are in"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_rack_moved_to_a_drawer_that_has_a_rack_for_another_property_or_another_thing_goes_in_as_well() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let owner = AnimationTarget::Clip(text);
    put_a_rack_in(&mut project, owner, big, PropertyId::Gain, &[(10, db(-20.0))]);
    let switch = put_a_rack_in(
        &mut project,
        big,
        big,
        PropertyId::Muted,
        &[(120, PropertyValue::Flag(true))],
    );

    project
        .move_rack(switch, RackDestination::Drawer(owner), 0, None)
        .unwrap();

    assert_eq!(names_of_the_racks_in(&project, owner).len(), 2);
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_rack_moved_within_its_drawer_is_only_shifted_and_keeps_its_place_among_the_racks() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let owner = AnimationTarget::Clip(text);
    let first = put_a_rack_in(&mut project, owner, big, PropertyId::Gain, &[(10, db(-20.0))]);
    let second = put_a_rack_in(
        &mut project,
        owner,
        big,
        PropertyId::Muted,
        &[(5, PropertyValue::Flag(true))],
    );

    project
        .move_rack(first, RackDestination::Drawer(owner), 7, None)
        .unwrap();

    assert_eq!(names_of_the_racks_in(&project, owner), vec![first, second]);
    assert_eq!(frames_in_the_rack(&project, first), vec![17]);
}

#[test]
fn a_rack_moved_to_another_drawer_goes_under_the_racks_that_are_in_it() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let owner = AnimationTarget::Clip(text);
    let already_there = put_a_rack_in(&mut project, owner, big, PropertyId::Gain, &[(10, db(-20.0))]);
    let arriving = put_a_rack_in(
        &mut project,
        big,
        big,
        PropertyId::Muted,
        &[(120, PropertyValue::Flag(true))],
    );

    project
        .move_rack(arriving, RackDestination::Drawer(owner), 0, None)
        .unwrap();

    assert_eq!(names_of_the_racks_in(&project, owner), vec![already_there, arriving]);
}

#[test]
fn a_rack_moved_to_a_track_goes_in_the_drawer_of_the_track_and_counts_from_the_start_of_the_timeline() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let rack = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(10, db(-20.0))],
    );
    let track = AnimationTarget::Track(project.tracks()[0].id());

    project
        .move_rack(rack, RackDestination::Drawer(track), 0, None)
        .unwrap();

    assert_eq!(frames_in_the_rack(&project, rack), vec![110]);
    assert_eq!(names_of_the_racks_in(&project, track), vec![rack]);
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_rack_cannot_be_moved_to_a_clip_or_a_track_that_is_not_there_and_a_rack_that_is_not_there_is_not_moved() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let rack = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(10, db(-20.0))],
    );
    let before = project.clone();
    let no_clip = ClipId::new(TrackId::new(50), 0);
    let no_track = TrackId::new(50);
    let no_rack = RackId::new(TrackId::new(50), 0);

    assert_eq!(
        project.move_rack(rack, RackDestination::Drawer(AnimationTarget::Clip(no_clip)), 0, None),
        Err(EditError::Clip(ClipEditError::UnknownClip { clip: no_clip }))
    );
    assert_eq!(
        project.move_rack(rack, RackDestination::Drawer(AnimationTarget::Track(no_track)), 0, None),
        Err(EditError::UnknownTrack { track: no_track })
    );
    assert_eq!(
        project.move_rack(
            rack,
            RackDestination::NewCrate {
                track: no_track,
                start: 0,
                length: 100
            },
            0,
            None
        ),
        Err(EditError::UnknownTrack { track: no_track })
    );
    assert_eq!(
        project.move_rack(no_rack, RackDestination::Drawer(AnimationTarget::Clip(video)), 0, None),
        Err(EditError::UnknownRack { rack: no_rack })
    );
    assert_eq!(project, before);
}

#[test]
fn a_rack_moved_onto_an_empty_stretch_of_a_track_makes_a_crate_that_has_it_and_a_crate_left_with_no_rack_goes() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let front = project.tracks()[0].id();
    let rack = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(10, db(-20.0)), (40, db(0.0))],
    );

    // A crate from frame 200 for 100 frames, with the keyframes 100 frames later than they were: at 210 and 240.
    project
        .move_rack(
            rack,
            RackDestination::NewCrate {
                track: front,
                start: 200,
                length: 100,
            },
            100,
            None,
        )
        .unwrap();

    let crate_id = project.tracks()[0].entries()[1].id();
    let placement = project.placement_of(crate_id).unwrap();
    assert!(matches!(placement.clip, Clip::Crate(_)));
    assert_eq!(placement.first_timeline_frame, frame(200));
    assert_eq!(placement.timeline_frame_count(), 100);
    assert_eq!(
        frames_in_the_rack(&project, rack),
        vec![10, 40],
        "counted from the start of the crate"
    );
    assert_eq!(
        where_the_video_has_keyframes(&project, video, PropertyId::Gain),
        vec![210, 240]
    );
    assert!(project.drawer_of(AnimationTarget::Clip(text)).unwrap().is_empty());
    assert_the_racks_are_sound(&project);

    // Taken out of the crate and put in the drawer of the text (from 100 to 160), the crate has no rack left, and goes.
    project
        .move_rack(rack, RackDestination::Drawer(AnimationTarget::Clip(text)), -100, None)
        .unwrap();
    assert!(project.placement_of(crate_id).is_none(), "the crate has gone");
    assert_eq!(project.tracks()[0].entries().len(), 1);
    assert_eq!(frames_in_the_rack(&project, rack), vec![10, 40]);
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_crate_that_still_has_a_rack_stays_when_another_rack_goes_out_of_it() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let front = project.tracks()[0].id();
    let crate_id = project.insert_crate(front, frame(200), 100).unwrap();
    let inside = AnimationTarget::Clip(crate_id);
    let leaving = put_a_rack_in(&mut project, inside, big, PropertyId::Gain, &[(10, db(-20.0))]);
    let staying = put_a_rack_in(
        &mut project,
        inside,
        big,
        PropertyId::Muted,
        &[(5, PropertyValue::Flag(true))],
    );

    // (Its keyframe is at 210: 100 frames earlier it is at 110, which is 10 frames into the text.)
    project
        .move_rack(
            leaving,
            RackDestination::Drawer(AnimationTarget::Clip(text)),
            -100,
            None,
        )
        .unwrap();

    assert!(project.placement_of(crate_id).is_some());
    assert_eq!(names_of_the_racks_in(&project, inside), vec![staying]);
}

#[test]
fn the_last_rack_of_a_crate_moved_to_a_new_crate_leaves_the_old_one_to_go() {
    let (mut project, video, _) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let front = project.tracks()[0].id();
    let old_crate = project.insert_crate(front, frame(200), 50).unwrap();
    let rack = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(old_crate),
        big,
        PropertyId::Gain,
        &[(10, db(-20.0))],
    );

    // The new crate is made where nothing is: from frame 400.
    project
        .move_rack(
            rack,
            RackDestination::NewCrate {
                track: front,
                start: 400,
                length: 60,
            },
            200,
            None,
        )
        .unwrap();

    assert!(project.placement_of(old_crate).is_none());
    assert_eq!(project.tracks()[0].entries().len(), 2, "the text and the new crate");
    assert_eq!(
        where_the_video_has_keyframes(&project, video, PropertyId::Gain),
        vec![410]
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_crate_cannot_be_made_where_a_clip_is_or_would_run_into_one_and_the_rack_stays_where_it_was() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let front = project.tracks()[0].id();
    let rack = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(10, db(-20.0))],
    );
    let before = project.clone();

    // The text is from 100 to 160.
    for (start, length, shift) in [(150, 100, 40), (50, 100, -60), (0, 600, 0)] {
        let result = project.move_rack(
            rack,
            RackDestination::NewCrate {
                track: front,
                start,
                length,
            },
            shift,
            None,
        );
        assert!(
            matches!(result, Err(EditError::Insert(_))),
            "from {start} for {length}: {result:?}"
        );
        assert_eq!(project, before);
    }
    let for_no_time = project.move_rack(
        rack,
        RackDestination::NewCrate {
            track: front,
            start: 300,
            length: 0,
        },
        190,
        None,
    );
    assert_eq!(for_no_time, Err(EditError::EmptyCrate(EmptyCrate)));
    assert_eq!(project, before);
}

#[test]
fn the_keyframes_of_a_rack_that_would_not_all_be_inside_the_new_crate_stop_it_being_made() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let front = project.tracks()[0].id();
    let rack = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(10, db(-20.0)), (50, db(0.0))],
    );
    let before = project.clone();

    // Shifted by 200 they are at 310 and 350, and a crate from 300 for 30 frames has frames 300 to 329.
    let result = project.move_rack(
        rack,
        RackDestination::NewCrate {
            track: front,
            start: 300,
            length: 30,
        },
        200,
        None,
    );

    assert_eq!(result, Err(EditError::NoRoomForTheKeyframes));
    assert_eq!(project, before);
}

#[test]
fn a_crate_is_not_made_inside_the_rectangle_of_a_clip_group_where_only_its_members_may_be_and_the_check_says_so_beforehand()
 {
    // A clip group of the text and the video: frames 0 to 600 across both tracks, with room in it on the front track after the text.
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(text), Member::Clip(video)])
        .unwrap();
    let front = project.tracks()[0].id();
    let rack = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(10, db(-20.0)), (40, db(0.0))],
    );
    let before = project.clone();
    // (The keyframes are at 110 and 140 on the timeline: shifted by 100 they are at 210 and 240, inside a crate that is from 200.)
    let inside_the_group = RackDestination::NewCrate {
        track: front,
        start: 200,
        length: 100,
    };

    let asked = project.check_the_move_of_a_rack(rack, inside_the_group, 100);
    let done = project.move_rack(rack, inside_the_group, 100, None);

    assert!(
        matches!(
            done,
            Err(EditError::BreaksTheRulesOfGroups(GroupProblem::IsInsideAClipGroupWithoutBeingInIt { group: found, .. })) if found == group
        ),
        "{done:?}"
    );
    assert_eq!(asked, done, "the check says what making the move says");
    assert_eq!(project, before, "the rack is where it was, and no crate was made");

    // After the end of the rectangle there is nothing to keep out: the same rack goes into a crate there.
    let after_the_group = RackDestination::NewCrate {
        track: front,
        start: 650,
        length: 100,
    };
    assert_eq!(project.check_the_move_of_a_rack(rack, after_the_group, 550), Ok(()));
    project.move_rack(rack, after_the_group, 550, None).unwrap();
    assert_eq!(project.problems_with_the_groups(), vec![]);
    assert_eq!(
        project.tracks()[0].entries().len(),
        2,
        "the text, and the crate after it"
    );
}

#[test]
fn asking_whether_a_rack_can_be_moved_says_what_moving_it_does_and_changes_nothing() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let front = project.tracks()[0].id();
    let rack = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(10, db(-20.0)), (40, db(0.0))],
    );
    let before = project.clone();
    let destinations = [
        (RackDestination::Drawer(AnimationTarget::Clip(text)), 5),
        (RackDestination::Drawer(AnimationTarget::Clip(text)), 500),
        (RackDestination::Drawer(AnimationTarget::Track(front)), 0),
        (
            RackDestination::NewCrate {
                track: front,
                start: 300,
                length: 100,
            },
            200,
        ),
        (
            RackDestination::NewCrate {
                track: front,
                start: 100,
                length: 100,
            },
            0,
        ),
        (
            RackDestination::NewCrate {
                track: front,
                start: 300,
                length: 0,
            },
            190,
        ),
    ];

    for (destination, shift) in destinations {
        let asked = project.check_the_move_of_a_rack(rack, destination, shift);
        assert_eq!(project, before, "asking changes nothing");
        let done = project.move_rack(rack, destination, shift, None);
        assert_eq!(asked, done, "{destination:?} shifted by {shift}");
        project = before.clone();
    }
}

#[test]
fn whatever_racks_are_moved_where_the_project_stays_sound_and_a_move_that_cannot_be_made_changes_nothing() {
    for seed in 1..=150u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed);
        let (mut project, video, text) = a_big_video_and_a_small_text();
        let big = AnimationTarget::Clip(video);
        let front = project.tracks()[0].id();
        let other_text = project
            .insert_text(front, frame(300), TextObject::default(), 100)
            .map(|(clip, _)| clip)
            .unwrap();
        let crate_id = project.insert_crate(front, frame(450), 80).unwrap();
        let owners = [
            big,
            AnimationTarget::Clip(text),
            AnimationTarget::Clip(other_text),
            AnimationTarget::Clip(crate_id),
            AnimationTarget::Track(front),
        ];
        // A rack for each of these, each in a drawer chosen at random, with keyframes at frames the drawer has.
        for (target, property, value) in [
            (big, PropertyId::Gain, db(-10.0)),
            (big, PropertyId::Muted, PropertyValue::Flag(true)),
            (big, PropertyId::Rotation, PropertyValue::Number(30.0)),
            (
                AnimationTarget::Clip(text),
                PropertyId::Rotation,
                PropertyValue::Number(10.0),
            ),
            (AnimationTarget::Track(front), PropertyId::Gain, db(-3.0)),
        ] {
            let owner = owners[random.next_below(owners.len() as u64) as usize];
            let length = project.extent_of_the_drawer_of(owner).unwrap().length.unwrap_or(600);
            let frames: Vec<(u64, PropertyValue)> = (0..1 + random.next_below(3))
                .map(|_| (random.next_below(length), value.clone()))
                .collect();
            put_a_rack_in(&mut project, owner, target, property, &frames);
        }
        assert_the_racks_are_sound(&project);

        for step in 0..30 {
            let racks: Vec<RackId> = project.located_racks().map(|located| located.rack.id()).collect();
            if racks.is_empty() {
                break;
            }
            let rack = racks[random.next_below(racks.len() as u64) as usize];
            let destination = if random.next_below(4) == 0 {
                RackDestination::NewCrate {
                    track: front,
                    start: random.next_below(700),
                    length: 1 + random.next_below(150),
                }
            } else {
                RackDestination::Drawer(owners[random.next_below(owners.len() as u64) as usize])
            };
            let shift = random.next_below(401) as i64 - 200;
            let before = project.clone();
            let located_before = before.located_rack(rack).unwrap();
            let absolute_frames_before: Vec<u64> = located_before
                .rack
                .keyframes()
                .iter()
                .map(|keyframe| located_before.start + keyframe.frame)
                .collect();
            let values_before: Vec<PropertyValue> = located_before
                .rack
                .keyframes()
                .iter()
                .map(|keyframe| keyframe.value.clone())
                .collect();

            let asked = project.check_the_move_of_a_rack(rack, destination, shift);
            let result = project.move_rack(rack, destination, shift, None);

            let context = format!("seed {seed}, step {step}: {rack} to {destination:?} shifted by {shift}");
            assert_eq!(asked, result, "{context}");
            match result {
                Err(_) => assert_eq!(project, before, "{context}"),
                Ok(()) => {
                    let located = project.located_rack(rack).expect(&context);
                    let absolute_frames_after: Vec<u64> = located
                        .rack
                        .keyframes()
                        .iter()
                        .map(|keyframe| located.start + keyframe.frame)
                        .collect();
                    let expected: Vec<u64> = absolute_frames_before
                        .iter()
                        .map(|frame| frame.checked_add_signed(shift).unwrap())
                        .collect();
                    assert_eq!(absolute_frames_after, expected, "{context}");
                    let values_after: Vec<PropertyValue> = located
                        .rack
                        .keyframes()
                        .iter()
                        .map(|keyframe| keyframe.value.clone())
                        .collect();
                    assert_eq!(values_after, values_before, "{context}");
                    assert_eq!(
                        project.located_racks().count(),
                        before.located_racks().count(),
                        "no rack is lost or made: {context}"
                    );
                }
            }
            assert_eq!(project.problems_with_the_racks(), Vec::<String>::new(), "{context}");
        }
    }
}

#[test]
fn a_rack_that_is_wanted_further_along_than_the_drawer_goes_is_put_as_far_as_the_drawer_goes_and_not_refused() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let to = RackDestination::Drawer(AnimationTarget::Clip(text));
    // At 110 and 130 of the timeline: 20 frames between them, and the text (100 to 160) has 60.
    let rack = put_a_rack_in(
        &mut project,
        big,
        big,
        PropertyId::Gain,
        &[(110, db(-20.0)), (130, db(0.0))],
    );

    assert_eq!(project.shift_that_fits(rack, to, 0), Ok(0), "it fits where it is");
    assert_eq!(project.shift_that_fits(rack, to, 7), Ok(7));
    assert_eq!(
        project.shift_that_fits(rack, to, 1_000),
        Ok(29),
        "the last keyframe goes to the last frame, 159"
    );
    assert_eq!(
        project.shift_that_fits(rack, to, -1_000),
        Ok(-10),
        "the first keyframe goes to the first frame, 100"
    );
    // And what was said fits is accepted.
    for wanted in [-1_000, -3, 0, 40, 1_000] {
        let shift = project.shift_that_fits(rack, to, wanted).unwrap();
        assert!(
            project.check_the_move_of_a_rack(rack, to, shift).is_ok(),
            "{wanted} became {shift}"
        );
    }
}

#[test]
fn a_rack_whose_first_and_last_keyframes_are_further_apart_than_the_drawer_is_long_fits_nowhere_in_it() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let to = RackDestination::Drawer(AnimationTarget::Clip(text));
    let rack = put_a_rack_in(
        &mut project,
        big,
        big,
        PropertyId::Gain,
        &[(100, db(-20.0)), (160, db(0.0))],
    );
    let almost = put_a_rack_in(
        &mut project,
        big,
        big,
        PropertyId::Muted,
        &[(100, PropertyValue::Flag(true)), (159, PropertyValue::Flag(false))],
    );

    assert_eq!(
        project.shift_that_fits(rack, to, 0),
        Err(EditError::NoRoomForTheKeyframes),
        "60 frames apart, in a drawer of 60"
    );
    assert_eq!(project.shift_that_fits(almost, to, 500), Ok(0), "59 apart just fits");
}

#[test]
fn a_rack_put_on_a_new_crate_is_put_as_far_as_the_crate_goes_and_the_track_drawer_has_no_end() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let front = project.tracks()[0].id();
    let rack = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(10, db(-20.0)), (30, db(0.0))],
    );
    let crate_at_300 = RackDestination::NewCrate {
        track: front,
        start: 300,
        length: 50,
    };

    // The keyframes are at 110 and 130 of the timeline, and the crate has the frames 300 to 349.
    assert_eq!(
        project.shift_that_fits(rack, crate_at_300, 0),
        Ok(190),
        "the first keyframe goes to the first frame of the crate"
    );
    assert_eq!(
        project.shift_that_fits(rack, crate_at_300, 10_000),
        Ok(219),
        "and the last to its last frame, 349"
    );
    let on_the_track = RackDestination::Drawer(AnimationTarget::Track(front));
    assert_eq!(project.shift_that_fits(rack, on_the_track, 100_000), Ok(100_000));
    assert_eq!(
        project.shift_that_fits(rack, on_the_track, -100_000),
        Ok(-110),
        "not before the start of the timeline"
    );
}

#[test]
fn a_shift_that_fits_is_not_found_for_a_place_that_is_not_there_or_a_rack_that_is_not() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let owner = AnimationTarget::Clip(text);
    put_a_rack_in(&mut project, owner, big, PropertyId::Gain, &[(10, db(-20.0))]);
    let rack = put_a_rack_in(&mut project, big, big, PropertyId::Gain, &[(120, db(0.0))]);
    let nowhere = ClipId::new(TrackId::new(50), 0);

    assert_eq!(
        project.shift_that_fits(rack, RackDestination::Drawer(owner), 0),
        Ok(0),
        "a drawer that has a rack of the same property of the same thing is a place it can go"
    );
    assert_eq!(
        project.shift_that_fits(rack, RackDestination::Drawer(AnimationTarget::Clip(nowhere)), 0),
        Err(EditError::Clip(ClipEditError::UnknownClip { clip: nowhere }))
    );
    assert_eq!(
        project.shift_that_fits(RackId::new(TrackId::new(50), 0), RackDestination::Drawer(owner), 0),
        Err(EditError::UnknownRack {
            rack: RackId::new(TrackId::new(50), 0)
        })
    );
}

// --- keyframe crates ---

#[test]
fn a_crate_is_cut_moved_and_deleted_like_a_clip_with_what_it_holds() {
    let (mut project, video, _) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let front = project.tracks()[0].id();
    let crate_id = project.insert_crate(front, frame(300), 120).unwrap();
    let rack = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(crate_id),
        big,
        PropertyId::Gain,
        &[(10, db(-20.0)), (100, db(0.0))],
    );

    // Cut at frame 350: two crates, each with its part of the rack.
    let cut = project.split_at(frame(350), Some(&[crate_id]));
    assert_eq!(cut.len(), 1);
    let second = cut[0].1.second_half;
    assert_eq!(project.placement_of(crate_id).unwrap().timeline_frame_count(), 50);
    assert_eq!(project.placement_of(second).unwrap().timeline_frame_count(), 70);
    assert_eq!(frames_in_the_rack(&project, rack), vec![10]);
    assert_eq!(
        where_the_video_has_keyframes(&project, video, PropertyId::Gain),
        vec![310, 400],
        "the video has the keyframes it had"
    );
    assert_the_racks_are_sound(&project);

    // Moved, the second crate takes its keyframes with it: it is from 350 to 420, and goes to 500.
    project
        .move_clips(&[ClipMove {
            clip: second,
            to_track: front,
            start: frame(500),
        }])
        .unwrap();
    assert_eq!(
        where_the_video_has_keyframes(&project, video, PropertyId::Gain),
        vec![310, 550]
    );

    // Deleted, it takes what is in it, and what is left is the first crate's.
    project.delete_clip(second).unwrap();
    assert_eq!(
        where_the_video_has_keyframes(&project, video, PropertyId::Gain),
        vec![310]
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_crate_is_put_on_an_empty_stretch_of_a_track_and_not_where_a_clip_is_or_for_no_time() {
    let (mut project, _, _) = a_big_video_and_a_small_text();
    let front = project.tracks()[0].id();

    assert!(
        matches!(
            project.insert_crate(front, frame(130), 100),
            Err(EditError::Insert(InsertError::Occupied { .. }))
        ),
        "the text is from 100 to 160"
    );
    assert!(
        matches!(
            project.insert_crate(front, frame(50), 100),
            Err(EditError::Insert(InsertError::NotEnoughRoom { .. }))
        ),
        "it would run into the text"
    );
    assert_eq!(
        project.insert_crate(front, frame(50), 0),
        Err(EditError::EmptyCrate(EmptyCrate))
    );
    assert!(matches!(
        project.insert_crate(TrackId::new(50), frame(0), 10),
        Err(EditError::UnknownTrack { .. })
    ));
    let made = project.insert_crate(front, frame(50), 50).unwrap();
    assert_eq!(project.placement_of(made).unwrap().timeline_frame_count(), 50);
    assert!(project.drawer_of(AnimationTarget::Clip(made)).unwrap().is_empty());
}

// --- parts that disagree about racks ---

#[test]
fn parts_with_a_rack_of_what_is_not_there_are_refused() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(1, db(0.0))],
    );
    // The track the video is on is left out, and the rack of its loudness has nothing to animate.
    let tracks_without_the_video: Vec<Track> = project.tracks()[..1].to_vec();

    let result = Project::from_parts(
        project.sources().to_vec(),
        tracks_without_the_video,
        project.speed_indicator_layout(),
    );

    assert!(
        matches!(result, Err(InconsistentProject::RacksAreWrong { .. })),
        "{result:?}"
    );
    // And with all the tracks it is fine.
    assert!(
        Project::from_parts(
            project.sources().to_vec(),
            project.tracks().to_vec(),
            project.speed_indicator_layout()
        )
        .is_ok()
    );
}
