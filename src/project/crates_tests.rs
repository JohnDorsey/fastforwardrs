//! Tests of how long keyframe crates last and where they are, as keyframes are put in them, moved and taken out: a crate lasts from its first
//! keyframe to its last (and so at least one frame), has some or is not there, and is only ever made longer where there is room for it, which
//! is the empty frames round it, as far as the end of the video.

use std::collections::BTreeMap;

use super::racks_tests::{
    a_big_video_and_a_small_text, assert_the_racks_are_sound, db, frames_in_the_rack, names_of_the_keyframes_in,
    put_a_rack_in, where_the_video_has_keyframes,
};
use super::tests::{frame, frame_rate};
use super::*;
use crate::test_support::DeterministicRandomNumbers;

/// Where a crate is on its track, and how long it lasts: the first frame of the timeline it is at and the one after its last.
fn extent_of(project: &Project, crate_clip: ClipId) -> (u64, u64) {
    let placement = project.placement_of(crate_clip).expect("the crate is there");
    (
        placement.first_timeline_frame.value(),
        placement.end_timeline_frame.value(),
    )
}

/// A video of 600 frames, a text object from frame 100 to frame 160 on the track in front of it, and on that track a keyframe crate with
/// a rack of the loudness of the video, which has keyframes at the frames 310 and 320 of the timeline, and so is from 310 to 321. Another
/// rack is in it, of whether the video is muted, with a keyframe at 315. Gives the project, the video, the crate, and the two racks.
fn a_crate_with_two_racks() -> (Project, ClipId, ClipId, RackId, RackId) {
    let (mut project, video, _) = a_big_video_and_a_small_text();
    let front = project.tracks()[0].id();
    let crate_id = project.insert_crate(front, frame(300), 200).unwrap();
    let loudness = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(crate_id),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(10, db(-1.0)), (20, db(-2.0))],
    );
    // (The frames of the crate are counted from where it begins, which is 310 now.)
    let muted = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(crate_id),
        AnimationTarget::Clip(video),
        PropertyId::Muted,
        &[(5, PropertyValue::Flag(true))],
    );
    assert_eq!(extent_of(&project, crate_id), (310, 321));
    assert_the_racks_are_sound(&project);
    (project, video, crate_id, loudness, muted)
}

fn the_keyframe_moved(project: &Project, rack: RackId, which: usize, to_frame: i64) -> KeyframeMove {
    KeyframeMove {
        keyframe: names_of_the_keyframes_in(project, rack)[which],
        to_rack: rack,
        to_frame,
    }
}

// --- how long a crate lasts ---

#[test]
fn a_crate_lasts_from_its_first_keyframe_to_its_last_and_the_frames_of_its_racks_are_counted_from_where_it_begins() {
    let (project, video, crate_id, loudness, muted) = a_crate_with_two_racks();

    assert_eq!(extent_of(&project, crate_id), (310, 321));
    assert_eq!(frames_in_the_rack(&project, loudness), vec![0, 10]);
    assert_eq!(frames_in_the_rack(&project, muted), vec![5]);
    assert_eq!(
        where_the_video_has_keyframes(&project, video, PropertyId::Gain),
        vec![310, 320],
        "the video has them at the frames of the timeline"
    );
}

#[test]
fn a_crate_with_one_keyframe_lasts_one_frame() {
    let (mut project, video, _) = a_big_video_and_a_small_text();
    let front = project.tracks()[0].id();
    let crate_id = project.insert_crate(front, frame(300), 200).unwrap();

    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(crate_id),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(77, db(-1.0))],
    );

    assert_eq!(extent_of(&project, crate_id), (377, 378));
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_crate_with_nothing_in_it_is_not_one_that_stays_when_it_is_looked_at() {
    let (mut project, _, _) = a_big_video_and_a_small_text();
    let front = project.tracks()[0].id();
    let crate_id = project.insert_crate(front, frame(300), 20).unwrap();
    assert_eq!(
        project.problems_with_the_racks().len(),
        1,
        "it is said to be wrong: {:?}",
        project.problems_with_the_racks()
    );

    project.fit_the_crate(AnimationTarget::Clip(crate_id)).unwrap();

    assert!(project.placement_of(crate_id).is_none());
    assert_the_racks_are_sound(&project);
}

// --- counted again at another frame rate ---

#[test]
fn a_crate_keeps_its_keyframes_at_the_moments_of_time_they_were_at_when_the_frame_rate_changes_and_comes_back_when_it_does()
 {
    let (mut project, video, crate_id, loudness, muted) = a_crate_with_two_racks();
    assert_eq!(project.frame_rate(), frame_rate(30));

    project.set_frame_rate(frame_rate(60));

    assert_eq!(
        extent_of(&project, crate_id),
        (620, 641),
        "it begins when it did, at twice the frame, and lasts to its last keyframe"
    );
    assert_eq!(frames_in_the_rack(&project, loudness), vec![0, 20]);
    assert_eq!(frames_in_the_rack(&project, muted), vec![10]);
    assert_eq!(
        where_the_video_has_keyframes(&project, video, PropertyId::Gain),
        vec![620, 640]
    );
    assert_the_racks_are_sound(&project);

    project.set_frame_rate(frame_rate(30));

    assert_eq!(extent_of(&project, crate_id), (310, 321));
    assert_eq!(frames_in_the_rack(&project, loudness), vec![0, 10]);
    assert_eq!(frames_in_the_rack(&project, muted), vec![5]);
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_crate_counted_again_at_a_lower_frame_rate_has_its_keyframes_at_the_same_moments_and_lasts_as_long_as_they_need() {
    let (mut project, video, _) = a_big_video_and_a_small_text();
    let front = project.tracks()[0].id();
    let crate_id = project.insert_crate(front, frame(300), 200).unwrap();
    let rack = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(crate_id),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(10, db(-1.0)), (30, db(-2.0))],
    );
    assert_eq!(extent_of(&project, crate_id), (310, 331));

    project.set_frame_rate(frame_rate(15));

    // (At 15 frames per second, the 10 seconds and a third and the 11 seconds that the keyframes are at are the frames 155 and 165.)
    assert_eq!(extent_of(&project, crate_id), (155, 166));
    assert_eq!(frames_in_the_rack(&project, rack), vec![0, 10]);
    assert_eq!(
        where_the_video_has_keyframes(&project, video, PropertyId::Gain),
        vec![155, 165]
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_crate_with_one_keyframe_keeps_it_when_the_frame_rate_drops_so_far_that_a_frame_would_last_no_time() {
    let (mut project, video, _) = a_big_video_and_a_small_text();
    let front = project.tracks()[0].id();
    let crate_id = project.insert_crate(front, frame(300), 200).unwrap();
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(crate_id),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(77, db(-1.0))],
    );
    assert_eq!(extent_of(&project, crate_id), (377, 378));

    // (A frame at 30 per second is 0.4 of a frame at 12 per second, which is no frame; the crate still lasts one, and has its keyframe.)
    project.set_frame_rate(frame_rate(12));

    assert_eq!(extent_of(&project, crate_id), (151, 152));
    assert_eq!(
        where_the_video_has_keyframes(&project, video, PropertyId::Gain),
        vec![151]
    );
    assert_the_racks_are_sound(&project);
}

// --- growing ---

#[test]
fn a_keyframe_moved_past_the_end_of_a_crate_makes_it_longer_and_one_moved_before_its_start_makes_it_begin_earlier() {
    let (mut project, video, crate_id, loudness, muted) = a_crate_with_two_racks();

    // The last keyframe of the loudness, from 320 to 340.
    project
        .move_keyframes(&[the_keyframe_moved(&project, loudness, 1, 30)])
        .unwrap();
    assert_eq!(extent_of(&project, crate_id), (310, 341), "longer at the end");
    assert_eq!(frames_in_the_rack(&project, loudness), vec![0, 30]);
    assert_eq!(
        frames_in_the_rack(&project, muted),
        vec![5],
        "the others are where they were"
    );

    // The first, from 310 to 300: the crate begins earlier, and the frames of the other rack are counted from there.
    project
        .move_keyframes(&[the_keyframe_moved(&project, loudness, 0, -10)])
        .unwrap();
    assert_eq!(extent_of(&project, crate_id), (300, 341), "and earlier at the start");
    assert_eq!(frames_in_the_rack(&project, loudness), vec![0, 40]);
    assert_eq!(
        frames_in_the_rack(&project, muted),
        vec![15],
        "the muting is at the frame 315 of the timeline as it was, and that is 15 frames into the crate now"
    );
    assert_eq!(
        where_the_video_has_keyframes(&project, video, PropertyId::Gain),
        vec![300, 340]
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn keyframes_moved_together_before_the_start_of_a_crate_make_it_begin_as_early_as_the_earliest_of_them_whatever_order_they_are_in()
 {
    let (mut project, video, crate_id, loudness, muted) = a_crate_with_two_racks();
    // The crate is from 310. The last keyframe of the loudness (at 320) goes to 305, and the one of the muting (at 315) goes to 300: the
    // later of the two frames is the first that is asked for.
    let moves = [
        the_keyframe_moved(&project, loudness, 1, -5),
        the_keyframe_moved(&project, muted, 0, -10),
    ];

    project.move_keyframes(&moves).unwrap();

    assert_eq!(
        extent_of(&project, crate_id),
        (300, 311),
        "from the earliest keyframe, 300, to the last, 310"
    );
    assert_eq!(
        where_the_video_has_keyframes(&project, video, PropertyId::Gain),
        vec![305, 310]
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_crate_that_is_longer_than_its_keyframes_need_at_either_end_is_said_to_be_wrong_and_one_that_is_not_is_not() {
    let (mut project, video, _) = a_big_video_and_a_small_text();
    let front = project.tracks()[0].id();
    // Crates of 20 frames (from 300, 330, 360 and 390), each with a rack that is put straight in, where an edit would have fitted them.
    let mut crate_with_keyframes_at = |first_frame: u64, keyframes: [u64; 2]| {
        let crate_id = project.insert_crate(front, frame(first_frame), 20).unwrap();
        let position = project.track_position_holding(crate_id).unwrap();
        let id = project.tracks[position].make_a_rack_id();
        let mut rack = crate::project::tracks::Rack::new(id, AnimationTarget::Clip(video), PropertyId::Gain);
        for keyframe in keyframes {
            rack.set(keyframe, db(-1.0)).unwrap();
        }
        project.drawer_mut(AnimationTarget::Clip(crate_id)).unwrap().push(rack);
        project.problems_with_the_racks()
    };

    assert_eq!(
        crate_with_keyframes_at(300, [0, 19]),
        Vec::<String>::new(),
        "as long as its keyframes need"
    );
    let too_long_at_the_end = crate_with_keyframes_at(330, [0, 15]);
    assert_eq!(too_long_at_the_end.len(), 1, "{too_long_at_the_end:?}");
    assert!(
        too_long_at_the_end[0].contains("keyframe crate"),
        "{too_long_at_the_end:?}"
    );
    let too_long_at_the_start = crate_with_keyframes_at(360, [3, 19]);
    assert_eq!(
        too_long_at_the_start.len(),
        2,
        "the one from before, and this: {too_long_at_the_start:?}"
    );
    let too_long_at_both_ends = crate_with_keyframes_at(390, [3, 15]);
    assert_eq!(too_long_at_both_ends.len(), 3, "{too_long_at_both_ends:?}");
}

#[test]
fn a_crate_is_made_longer_where_a_rack_is_put_in_it_whose_keyframes_are_beyond_it_and_not_where_it_has_no_room() {
    let (mut project, video, crate_id, loudness, _) = a_crate_with_two_racks();
    // A rack of the turn of the video, in the drawer of the video: its keyframes are at 340 and 350, which are after the crate.
    for at in [340, 350] {
        crate::test_support::make_a_keyframe(
            &mut project,
            AnimationTarget::Clip(video),
            PropertyId::Rotation,
            at,
            PropertyValue::Number(10.0),
        );
    }
    let turn = project
        .drawer_of(AnimationTarget::Clip(video))
        .unwrap()
        .first_rack_for(AnimationTarget::Clip(video), PropertyId::Rotation)
        .unwrap()
        .id();

    project
        .move_racks(
            &[turn],
            RackDestination::Drawer(AnimationTarget::Clip(crate_id)),
            0,
            None,
        )
        .unwrap();

    assert_eq!(
        extent_of(&project, crate_id),
        (310, 351),
        "it lasts through the keyframes that were put in it"
    );
    assert_eq!(frames_in_the_rack(&project, turn), vec![30, 40]);
    assert_eq!(
        frames_in_the_rack(&project, loudness),
        vec![0, 10],
        "and what was in it is where it was"
    );
    assert_the_racks_are_sound(&project);

    // A rack whose keyframes are in the text object that is from 100 to 160, which is not empty: the crate has no room to go there.
    for at in [110, 120] {
        crate::test_support::make_a_keyframe(
            &mut project,
            AnimationTarget::Clip(video),
            PropertyId::Position,
            at,
            PropertyValue::Pair(0.1, 0.1),
        );
    }
    let position = project
        .drawer_of(AnimationTarget::Clip(video))
        .unwrap()
        .first_rack_for(AnimationTarget::Clip(video), PropertyId::Position)
        .unwrap()
        .id();
    let before = project.clone();
    assert_eq!(
        project.move_racks(
            &[position],
            RackDestination::Drawer(AnimationTarget::Clip(crate_id)),
            0,
            None
        ),
        Err(EditError::NoRoomForTheKeyframes)
    );
    assert_eq!(project, before, "nothing is changed");
}

// --- where it has room ---

#[test]
fn a_crate_has_room_for_the_empty_frames_round_it_as_far_as_the_clips_beside_it_the_start_of_the_timeline_and_the_end_of_the_video()
 {
    let (mut project, _, crate_id, loudness, _) = a_crate_with_two_racks();
    let front = project.tracks()[0].id();
    // A text object from frame 340: the crate can last until the frame before it.
    project
        .insert_text(front, frame(340), TextObject::default(), 20)
        .unwrap();
    let room = project
        .extent_of_the_drawer_of(AnimationTarget::Clip(crate_id))
        .unwrap()
        .room;
    assert_eq!(
        (room.first_frame, room.end_frame),
        (160, Some(340)),
        "from where the text object before it ends to where the one after it begins"
    );
    let before = project.clone();

    // (The last keyframe goes to the frame 340 of the timeline, which is 30 frames into the crate, and is not a frame it has room for.)
    assert_eq!(
        project.move_keyframes(&[the_keyframe_moved(&project, loudness, 1, 30)]),
        Err(EditError::NoRoomForTheKeyframes)
    );
    assert_eq!(project, before);
    project
        .move_keyframes(&[the_keyframe_moved(&project, loudness, 1, 29)])
        .unwrap();
    assert_eq!(
        extent_of(&project, crate_id),
        (310, 340),
        "the last frame there is room for"
    );
    // The first, the other way: the text object before it ends at 160, so the first frame it has room for is 160.
    assert_eq!(
        project.move_keyframes(&[the_keyframe_moved(&project, loudness, 0, -151)]),
        Err(EditError::NoRoomForTheKeyframes),
        "frame 159 is the last of the text"
    );
    project
        .move_keyframes(&[the_keyframe_moved(&project, loudness, 0, -150)])
        .unwrap();
    assert_eq!(extent_of(&project, crate_id), (160, 340));
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_crate_does_not_go_past_the_end_of_the_video_or_before_the_start_of_the_timeline() {
    let (mut project, _, crate_id, loudness, _) = a_crate_with_two_racks();
    // The video lasts 600 frames. The crate is not made to last past them, and does not make the video longer.
    assert_eq!(project.total_frame_count(), 600);
    let before = project.clone();

    assert_eq!(
        project.move_keyframes(&[the_keyframe_moved(&project, loudness, 1, 290)]),
        Err(EditError::NoRoomForTheKeyframes),
        "frame 600"
    );
    assert_eq!(project, before);
    project
        .move_keyframes(&[the_keyframe_moved(&project, loudness, 1, 289)])
        .unwrap();
    assert_eq!(
        extent_of(&project, crate_id),
        (310, 600),
        "frame 599 is the last of the video"
    );
    assert_eq!(project.total_frame_count(), 600);

    // And at the other end of the timeline: the text object is from 100, so the crate is moved to a frame before it, in a project that
    // has nothing before the crate.
    let (mut project, video, _) = a_big_video_and_a_small_text();
    let front = project.tracks()[0].id();
    let early = project.insert_crate(front, frame(5), 10).unwrap();
    let rack = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(early),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(0, db(-1.0)), (4, db(-2.0))],
    );
    assert_eq!(extent_of(&project, early), (5, 10));
    let before = project.clone();

    assert_eq!(
        project.move_keyframes(&[the_keyframe_moved(&project, rack, 0, -6)]),
        Err(EditError::NoRoomForTheKeyframes),
        "before the first frame of the timeline"
    );
    assert_eq!(project, before);
    project
        .move_keyframes(&[the_keyframe_moved(&project, rack, 0, -5)])
        .unwrap();
    assert_eq!(extent_of(&project, early), (0, 10), "the first frame of the timeline");
}

/// Puts a rack of the turn of `video` in the drawer of the crate straight, with a keyframe at the frame `in_the_crate` of the crate and nothing
/// else done (an edit would look at the room first, and this is what fitting the crate must still look at).
fn a_rack_put_straight_in_a_crate(project: &mut Project, crate_id: ClipId, video: ClipId, in_the_crate: u64) {
    let position = project.track_position_holding(crate_id).unwrap();
    let id = project.tracks[position].make_a_rack_id();
    let mut rack = crate::project::tracks::Rack::new(id, AnimationTarget::Clip(video), PropertyId::Rotation);
    rack.set(in_the_crate, PropertyValue::Number(5.0)).unwrap();
    project.drawer_mut(AnimationTarget::Clip(crate_id)).unwrap().push(rack);
}

#[test]
fn a_crate_that_has_a_keyframe_past_the_end_of_the_video_is_not_made_as_long_as_that_and_nothing_is_changed() {
    let (mut project, video, crate_id, _, _) = a_crate_with_two_racks();
    // The crate begins at 310, so this keyframe is at the frame 600 of the timeline, and the last frame of the video is 599.
    a_rack_put_straight_in_a_crate(&mut project, crate_id, video, 290);
    let before = project.clone();

    assert_eq!(
        project.fit_the_crate(AnimationTarget::Clip(crate_id)),
        Err(EditError::NoRoomForTheKeyframes)
    );
    assert_eq!(project, before, "nothing is changed");
    assert_eq!(extent_of(&project, crate_id), (310, 321));
}

#[test]
fn a_crate_that_has_a_keyframe_in_the_clip_after_it_is_not_made_as_long_as_that_and_nothing_is_changed() {
    let (mut project, video, crate_id, _, _) = a_crate_with_two_racks();
    let front = project.tracks()[0].id();
    project
        .insert_text(front, frame(340), TextObject::default(), 20)
        .unwrap();
    // The text object is from 340, so this keyframe, at the frame 340 of the timeline, is in it.
    a_rack_put_straight_in_a_crate(&mut project, crate_id, video, 30);
    let before = project.clone();

    assert_eq!(
        project.fit_the_crate(AnimationTarget::Clip(crate_id)),
        Err(EditError::NoRoomForTheKeyframes)
    );
    assert_eq!(project, before, "nothing is changed");
}

#[test]
fn a_crate_in_a_clip_group_is_not_made_longer_if_the_rectangle_of_the_group_would_take_in_another_clip_which_is_not_in_it()
 {
    let (mut project, _, crate_id, loudness, _) = a_crate_with_two_racks();
    // A track in front of the track that the crate is on, with a text object from frame 300 to frame 330 (which is in a clip group with the
    // crate, so that the group is from frame 300 to frame 330, across the two tracks), and another from 340 to 350, which is not.
    let above = project.add_track_in_front();
    let (member, _) = project
        .insert_text(above, frame(300), TextObject::default(), 30)
        .unwrap();
    let (outside, _) = project
        .insert_text(above, frame(340), TextObject::default(), 10)
        .unwrap();
    project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(crate_id), Member::Clip(member)])
        .unwrap();
    assert_eq!(project.problems_with_the_groups(), vec![]);
    let before = project.clone();

    // The last keyframe goes to frame 355: the crate lasts until there, and the group would be from 300 to 356, with the text object
    // that is not in it inside.
    let result = project.move_keyframes(&[the_keyframe_moved(&project, loudness, 1, 45)]);

    assert!(
        matches!(result, Err(EditError::BreaksTheRulesOfGroups(_))),
        "the text object would be inside the clip group and not in it: {result:?}"
    );
    assert_eq!(project, before, "nothing is changed");
    // And to frame 335, the group is from 300 to 336, and nothing is inside it that is not in it.
    project
        .move_keyframes(&[the_keyframe_moved(&project, loudness, 1, 25)])
        .unwrap();
    assert_eq!(extent_of(&project, crate_id), (310, 336));
    assert_eq!(project.problems_with_the_groups(), vec![]);
    assert!(project.placement_of(outside).is_some());
}

// --- getting shorter, and going ---

#[test]
fn a_crate_gets_shorter_when_its_first_or_last_keyframe_goes_and_goes_when_its_last_keyframe_does() {
    let (mut project, _, _) = a_big_video_and_a_small_text();
    let video = project.tracks()[1].entries()[0].id();
    let front = project.tracks()[0].id();
    let crate_id = project.insert_crate(front, frame(300), 200).unwrap();
    let rack = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(crate_id),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(10, db(-1.0)), (20, db(-2.0)), (40, db(-3.0))],
    );
    assert_eq!(extent_of(&project, crate_id), (310, 341));
    let names = names_of_the_keyframes_in(&project, rack);

    project.remove_keyframes(&[names[0]]).unwrap();
    assert_eq!(
        extent_of(&project, crate_id),
        (320, 341),
        "the first has gone: it begins later"
    );
    assert_eq!(frames_in_the_rack(&project, rack), vec![0, 20]);

    project.remove_keyframes(&[names[2]]).unwrap();
    assert_eq!(
        extent_of(&project, crate_id),
        (320, 321),
        "the last has gone: it ends earlier"
    );
    assert_eq!(frames_in_the_rack(&project, rack), vec![0]);

    project.remove_keyframes(&[names[1]]).unwrap();
    assert!(project.placement_of(crate_id).is_none(), "nothing is left in it");
    assert_eq!(
        project.tracks()[0].entries().len(),
        1,
        "only the text object is on the track"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_keyframe_moved_to_a_rack_of_another_crate_makes_the_one_it_left_shorter_and_the_other_longer() {
    let (mut project, video, first_crate, loudness, _) = a_crate_with_two_racks();
    let front = project.tracks()[0].id();
    let other_crate = project.insert_crate(front, frame(400), 100).unwrap();
    let in_the_other = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(other_crate),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(5, db(-5.0))],
    );
    assert_eq!(extent_of(&project, other_crate), (405, 406));

    // The last keyframe of the first crate goes to the other, at frame 450 of the timeline, which is 45 frames after where the other begins.
    project
        .move_keyframes(&[KeyframeMove {
            keyframe: names_of_the_keyframes_in(&project, loudness)[1],
            to_rack: in_the_other,
            to_frame: 45,
        }])
        .unwrap();

    assert_eq!(
        extent_of(&project, first_crate),
        (310, 316),
        "the first now has the keyframe at 310 and the muting at 315"
    );
    assert_eq!(extent_of(&project, other_crate), (405, 451));
    assert_eq!(frames_in_the_rack(&project, in_the_other), vec![0, 45]);
    assert_the_racks_are_sound(&project);
}

// --- the racks of what is gone ---

#[test]
fn the_racks_that_animate_a_clip_that_is_deleted_are_taken_from_a_crate_and_it_gets_shorter_or_goes() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let front = project.tracks()[0].id();
    let crate_id = project.insert_crate(front, frame(300), 200).unwrap();
    // A rack that animates the video, from the frame 310 to the frame 320 of the timeline, and one that animates the text object, at 340.
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(crate_id),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(10, db(-1.0)), (20, db(-2.0))],
    );
    let of_the_text = put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(crate_id),
        AnimationTarget::Clip(text),
        PropertyId::Rotation,
        &[(30, PropertyValue::Number(5.0))],
    );
    assert_eq!(extent_of(&project, crate_id), (310, 341));

    // The text object goes, and with it the rack that animates it: the crate is as long as what is left in it needs.
    project.delete_clip(text).unwrap();

    assert!(project.located_rack(of_the_text).is_none());
    assert_eq!(extent_of(&project, crate_id), (310, 321));
    assert_the_racks_are_sound(&project);

    // The video goes: nothing is left in the crate, and it goes too.
    project.delete_clip(video).unwrap();

    assert!(project.placement_of(crate_id).is_none());
    assert_the_racks_are_sound(&project);
}

#[test]
fn the_racks_that_animate_a_track_that_is_deleted_are_taken_from_a_crate_and_it_goes_if_it_had_no_others() {
    let (mut project, _, _) = a_big_video_and_a_small_text();
    let front = project.tracks()[0].id();
    let other_track = project.add_track_in_front();
    let crate_id = project.insert_crate(front, frame(300), 200).unwrap();
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(crate_id),
        AnimationTarget::Track(other_track),
        PropertyId::Gain,
        &[(10, db(-1.0)), (20, db(-2.0))],
    );
    assert_eq!(extent_of(&project, crate_id), (310, 321));

    project.delete_track(other_track).unwrap();

    assert!(project.placement_of(crate_id).is_none());
    assert_the_racks_are_sound(&project);
}

// --- how long it is is not for anyone to say ---

#[test]
fn a_crate_is_not_given_a_length_and_the_error_says_it_lasts_as_long_as_its_keyframes() {
    let (mut project, _, crate_id, _, _) = a_crate_with_two_racks();
    let before = project.clone();

    assert_eq!(
        project.set_clip_duration(crate_id, 100),
        Err(EditError::Clip(ClipEditError::LastsAsLongAsItsKeyframes {
            clip: crate_id
        }))
    );
    assert_eq!(project, before);
    assert_eq!(
        project.tracks()[0].duration_a_clip_would_be_given(crate_id, 100),
        Err(ClipEditError::LastsAsLongAsItsKeyframes { clip: crate_id })
    );
}

#[test]
fn the_extent_of_a_crate_is_set_only_for_a_crate_inside_the_empty_stretch_round_it() {
    let (mut project, video, crate_id, _, _) = a_crate_with_two_racks();
    let video_track = project.track_position_holding(video).unwrap();
    let front = project.track_position_holding(crate_id).unwrap();
    let text = project.tracks()[0].entries()[0].id();
    let before = project.clone();

    let not_a_crate = project.tracks[video_track].set_the_extent_of_a_crate(video, 0, 10);
    assert_eq!(not_a_crate, Err(ClipEditError::NotAKeyframeCrate { clip: video }));
    let not_there = project.tracks[front].set_the_extent_of_a_crate(ClipId::new(TrackId::new(77), 3), 0, 10);
    assert_eq!(
        not_there,
        Err(ClipEditError::UnknownClip {
            clip: ClipId::new(TrackId::new(77), 3)
        })
    );
    let in_the_text = project.tracks[front].set_the_extent_of_a_crate(crate_id, 150, 321);
    assert_eq!(
        in_the_text,
        Err(ClipEditError::WouldOverlap { clip: crate_id }),
        "the text is until 160"
    );
    let past_the_end = project.tracks[front].set_the_extent_of_a_crate(crate_id, 310, 10_000);
    assert_eq!(
        past_the_end,
        Ok(()),
        "this does not look at the video: the track has nothing after the crate"
    );
    let for_no_time = project.tracks[front].set_the_extent_of_a_crate(crate_id, 320, 320);
    assert_eq!(
        for_no_time,
        Err(ClipEditError::LastsAsLongAsItsKeyframes { clip: crate_id })
    );
    assert!(project.placement_of(text).is_some());
    assert_ne!(project, before, "the one that was possible was made");
}

#[test]
fn the_extent_of_a_crate_does_not_go_into_the_clip_after_it_and_that_clip_stays_where_it_is_whatever_the_crate_is_made()
{
    let (mut project, _, crate_id, _, _) = a_crate_with_two_racks();
    let front = project.tracks()[0].id();
    // A text object from 340 to 360, after the crate (310 to 321).
    let (text, _) = project
        .insert_text(front, frame(340), TextObject::default(), 20)
        .unwrap();
    let position = project.track_position_holding(crate_id).unwrap();
    let text_is_at = |project: &Project| extent_of(project, text);
    assert_eq!(text_is_at(&project), (340, 360));

    assert_eq!(
        project.tracks[position].set_the_extent_of_a_crate(crate_id, 310, 341),
        Err(ClipEditError::WouldOverlap { clip: crate_id }),
        "one frame into the text"
    );
    assert_eq!(extent_of(&project, crate_id), (310, 321), "nothing was changed");
    assert_eq!(
        project.tracks[position].set_the_extent_of_a_crate(crate_id, 310, 340),
        Ok(()),
        "as far as the frame before the text"
    );
    assert_eq!(extent_of(&project, crate_id), (310, 340));
    assert_eq!(text_is_at(&project), (340, 360));

    assert_eq!(
        project.tracks[position].set_the_extent_of_a_crate(crate_id, 320, 330),
        Ok(())
    );
    assert_eq!(extent_of(&project, crate_id), (320, 330), "shorter at both ends");
    assert_eq!(text_is_at(&project), (340, 360), "the text object has not moved");
}

// --- whatever is done ---

/// How the crates of a project are, by name: where each begins and ends.
fn extents_of_the_crates(project: &Project) -> BTreeMap<ClipId, (u64, u64)> {
    project
        .tracks()
        .iter()
        .flat_map(|track| track.placements())
        .filter(|placement| matches!(placement.clip, Clip::Crate(_)))
        .map(|placement| {
            (
                placement.id,
                (
                    placement.first_timeline_frame.value(),
                    placement.end_timeline_frame.value(),
                ),
            )
        })
        .collect()
}

/// What a random edit made of the crates, counted over a run.
#[derive(Default, Debug)]
struct WhatHappenedToCrates {
    made_longer_at_the_end: usize,
    made_longer_at_the_start: usize,
    made_shorter: usize,
    went: usize,
    were_made: usize,
}

impl WhatHappenedToCrates {
    fn note(&mut self, before: &BTreeMap<ClipId, (u64, u64)>, after: &BTreeMap<ClipId, (u64, u64)>) {
        for (clip, (start, end)) in before {
            match after.get(clip) {
                None => self.went += 1,
                Some((new_start, new_end)) => {
                    self.made_longer_at_the_end += usize::from(new_end > end);
                    self.made_longer_at_the_start += usize::from(new_start < start);
                    self.made_shorter += usize::from(new_end < end || new_start > start);
                }
            }
        }
        self.were_made += after.keys().filter(|clip| !before.contains_key(clip)).count();
    }
}

#[test]
fn whatever_is_done_to_keyframes_racks_and_crates_each_crate_lasts_exactly_as_long_as_its_keyframes_need_and_one_that_is_refused_changes_nothing()
 {
    let mut happened = WhatHappenedToCrates::default();
    let mut made = 0;
    for seed in 1..=300u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed);
        let (mut project, video, text) = a_big_video_and_a_small_text();
        let front = project.tracks()[0].id();
        let video_track = project.tracks()[1].id();
        let targets_and_properties = [
            (AnimationTarget::Clip(video), PropertyId::Gain),
            (AnimationTarget::Clip(video), PropertyId::Rotation),
            (AnimationTarget::Clip(text), PropertyId::Rotation),
            (AnimationTarget::Clip(text), PropertyId::Position),
            (AnimationTarget::Track(front), PropertyId::Gain),
            (AnimationTarget::Track(video_track), PropertyId::Muted),
        ];
        for step in 0..60 {
            let before = project.clone();
            let crates_before = extents_of_the_crates(&before);
            let racks: Vec<RackId> = project.located_racks().map(|located| located.rack.id()).collect();
            let keyframes: Vec<KeyframeId> = project
                .located_racks()
                .flat_map(|located| {
                    located
                        .rack
                        .keyframes()
                        .iter()
                        .map(RackKeyframe::id)
                        .collect::<Vec<_>>()
                })
                .collect();
            let crates: Vec<ClipId> = crates_before.keys().copied().collect();
            let clips: Vec<ClipId> = project
                .tracks()
                .iter()
                .flat_map(|track| track.entries().iter().map(|entry| entry.id()))
                .collect();
            let (what, result): (&str, Result<(), EditError>) = match random.next_below(14) {
                0..=2 => {
                    let (target, property) =
                        targets_and_properties[random.next_below(targets_and_properties.len() as u64) as usize];
                    let at = frame(random.next_below(600));
                    ("add a keyframe", project.add_keyframe(target, property, at).map(|_| ()))
                }
                3 if !keyframes.is_empty() => {
                    // (One of those that are in a crate, if there are any, half of the time: they are what is being tested.)
                    let in_crates: Vec<KeyframeId> = project
                        .located_racks()
                        .filter(
                            |located| matches!(located.owner, AnimationTarget::Clip(clip) if crates.contains(&clip)),
                        )
                        .flat_map(|located| {
                            located
                                .rack
                                .keyframes()
                                .iter()
                                .map(RackKeyframe::id)
                                .collect::<Vec<_>>()
                        })
                        .collect();
                    let from_which = if !in_crates.is_empty() && random.next_below(2) == 0 {
                        &in_crates
                    } else {
                        &keyframes
                    };
                    let keyframe = from_which[random.next_below(from_which.len() as u64) as usize];
                    let from = project.locate_keyframe(keyframe).unwrap();
                    // To the rack it is in or, if there is another of the same property of the same thing, to that.
                    let same: Vec<RackId> = project
                        .located_racks()
                        .filter(|located| {
                            located.rack.target() == from.rack.rack.target()
                                && located.rack.property() == from.rack.rack.property()
                        })
                        .map(|located| located.rack.id())
                        .collect();
                    let to_rack = same[random.next_below(same.len() as u64) as usize];
                    let to = project.located_rack(to_rack).unwrap();
                    let first = i64::try_from(to.room.first_frame).unwrap() - i64::try_from(to.start).unwrap();
                    let end =
                        i64::try_from(to.room.end_frame.unwrap_or(700)).unwrap() - i64::try_from(to.start).unwrap();
                    let to_frame = if random.next_below(2) == 0 {
                        // Anywhere it has room for...
                        first + i64::try_from(random.next_below(u64::try_from(end - first).unwrap())).unwrap()
                    } else {
                        // ...or a few frames from where it is, which is what makes a crate a little longer or shorter.
                        i64::try_from(from.keyframe.frame).unwrap() + i64::try_from(random.next_below(17)).unwrap() - 8
                    };
                    (
                        "move a keyframe",
                        project.move_keyframes(&[KeyframeMove {
                            keyframe,
                            to_rack,
                            to_frame,
                        }]),
                    )
                }
                4..=7 if !racks.is_empty() => {
                    let rack = racks[random.next_below(racks.len() as u64) as usize];
                    let to = if random.next_below(5) < 3 {
                        let from_frame = random.next_below(500);
                        RackDestination::NewCrate {
                            track: if random.next_below(2) == 0 { front } else { video_track },
                            from_frame,
                            until_frame: from_frame + 1 + random.next_below(200),
                        }
                    } else {
                        let owners: Vec<AnimationTarget> = [AnimationTarget::Clip(video), AnimationTarget::Clip(text)]
                            .into_iter()
                            .chain(crates.iter().map(|clip| AnimationTarget::Clip(*clip)))
                            .chain([AnimationTarget::Track(front)])
                            .collect();
                        RackDestination::Drawer(owners[random.next_below(owners.len() as u64) as usize])
                    };
                    let shift = i64::try_from(random.next_below(121)).unwrap() - 60;
                    ("move a rack", project.move_racks(&[rack], to, shift, None))
                }
                8 if !keyframes.is_empty() => {
                    let keyframe = keyframes[random.next_below(keyframes.len() as u64) as usize];
                    ("take away a keyframe", project.remove_keyframes(&[keyframe]))
                }
                9 if !keyframes.is_empty() => {
                    let chosen: Vec<KeyframeId> = keyframes
                        .iter()
                        .copied()
                        .filter(|_| random.next_below(4) == 0)
                        .take(3)
                        .collect();
                    let towards = if random.next_below(2) == 0 {
                        TimeDirection::Earlier
                    } else {
                        TimeDirection::Later
                    };
                    (
                        "copy keyframes",
                        project
                            .duplicate_keyframes(&chosen, towards, random.next_below(30))
                            .map(|_| ()),
                    )
                }
                10 if racks.len() >= 2 => {
                    let chosen: Vec<RackId> = racks.iter().copied().filter(|_| random.next_below(3) == 0).collect();
                    ("merge racks", project.merge_racks(&chosen).map(|_| ()))
                }
                11 if !crates.is_empty() => {
                    let crate_id = crates[random.next_below(crates.len() as u64) as usize];
                    let track = if random.next_below(3) == 0 { video_track } else { front };
                    (
                        "move a crate",
                        project.move_clips(&[ClipMove {
                            clip: crate_id,
                            to_track: track,
                            start: frame(random.next_below(550)),
                        }]),
                    )
                }
                12 if random.next_below(3) > 0 => {
                    // (A cut that cuts nothing is not refused, and changes nothing.)
                    project.split_at(frame(random.next_below(600)), None);
                    ("cut", Ok(()))
                }
                12 if !crates.is_empty() => {
                    let crate_id = crates[random.next_below(crates.len() as u64) as usize];
                    let side = if random.next_below(2) == 0 {
                        ClipSide::Left
                    } else {
                        ClipSide::Right
                    };
                    ("copy a crate", project.duplicate_clip(crate_id, side).map(|_| ()))
                }
                13 => {
                    let clip = clips[random.next_below(clips.len() as u64) as usize];
                    // (The video is what the others are animated in the drawers of: it is not taken away, so that there is more to do.)
                    if clip == video {
                        continue;
                    }
                    ("delete a clip", project.delete_clip(clip))
                }
                _ => continue,
            };
            made += usize::from(result.is_ok());
            let context = format!("seed {seed}, step {step}: {what}: {result:?}");
            if result.is_err() {
                assert_eq!(
                    project, before,
                    "an edit that was refused changed the project: {context}"
                );
            }
            assert_eq!(project.problems_with_the_racks(), Vec::<String>::new(), "{context}");
            happened.note(&crates_before, &extents_of_the_crates(&project));
        }
    }
    if std::env::var_os("FASTFORWARDRS_SHOW_RANDOM_EDIT_STATISTICS").is_some() {
        println!("{made} edits were made; {happened:?}");
    }
    assert!(made > 1500, "the test edits: {made} were made");
    // The test is only worth anything if the crates were really made longer and shorter and taken away, in all the ways.
    for (what, count) in [
        ("longer at the end", happened.made_longer_at_the_end),
        ("longer at the start", happened.made_longer_at_the_start),
        ("shorter", happened.made_shorter),
        ("gone", happened.went),
        ("made", happened.were_made),
    ] {
        assert!(count >= 30, "crates were made {what} only {count} times: {happened:?}");
    }
}
