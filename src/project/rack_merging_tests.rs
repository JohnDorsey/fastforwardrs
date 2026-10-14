//! Tests of merging racks of one property of one thing into one rack.

use super::racks_tests::{
    a_big_video_and_a_small_text, assert_the_racks_are_sound, db, frames_in_the_rack, gain_at,
    names_of_the_keyframes_in, names_of_the_racks_in, put_a_rack_in,
};
use super::tests::frame;
use super::*;
use crate::test_support::DeterministicRandomNumbers;

/// A video of 600 frames on the back track, and on the front track a text object from frame 100 to 160 and another from 300 to 400,
/// where racks of the loudness of the video are put (and see [`a_crate_after_the_texts`]).
struct Places {
    project: Project,
    video: AnimationTarget,
    text: AnimationTarget,
    other_text: AnimationTarget,
    front_track: AnimationTarget,
}

fn places() -> Places {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let front = project.tracks()[0].id();
    let other_text = project
        .insert_text(front, frame(300), TextObject::default(), 100)
        .map(|(clip, _)| clip)
        .unwrap();
    Places {
        project,
        video: AnimationTarget::Clip(video),
        text: AnimationTarget::Clip(text),
        other_text: AnimationTarget::Clip(other_text),
        front_track: AnimationTarget::Track(front),
    }
}

/// A keyframe crate on the front track of `places`, from frame 450 to 530, with no rack in it. (A crate with no rack is taken away as soon as
/// it is looked at, by `Project::fit_the_crate`: the test that makes one puts racks in it, or fits it.)
fn a_crate_after_the_texts(places: &mut Places) -> AnimationTarget {
    let front = places.project.tracks()[0].id();
    AnimationTarget::Clip(places.project.insert_crate(front, frame(450), 80).unwrap())
}

/// Puts a rack of the loudness of the video in the drawer of `owner`, with these keyframes at frames of the timeline (and values
/// in decibels). A keyframe crate is made to begin earlier first, as it is when keyframes are put in it by an edit, if the keyframes are before it (and
/// is made as long as the keyframes need once they are in it).
fn a_rack_of_the_loudness_in(
    project: &mut Project,
    video: AnimationTarget,
    owner: AnimationTarget,
    keyframes: &[(u64, f32)],
) -> RackId {
    let first = keyframes
        .iter()
        .map(|(timeline_frame, _)| *timeline_frame)
        .min()
        .unwrap();
    project
        .make_the_crate_begin_by(owner, first)
        .expect("a crate has room for them");
    let start = project.extent_of_the_drawer_of(owner).unwrap().start;
    let in_the_drawer: Vec<(u64, PropertyValue)> = keyframes
        .iter()
        .map(|(timeline_frame, decibels)| (timeline_frame - start, db(*decibels)))
        .collect();
    put_a_rack_in(project, owner, video, PropertyId::Gain, &in_the_drawer)
}

fn the_loudness_of_the_video_at_every_frame(project: &Project, video: AnimationTarget) -> Vec<f32> {
    (0..620).map(|at| gain_at(project, video, at)).collect()
}

fn values_in_the_rack(project: &Project, rack: RackId) -> Vec<PropertyValue> {
    project
        .located_rack(rack)
        .unwrap()
        .rack
        .keyframes()
        .iter()
        .map(|keyframe| keyframe.value.clone())
        .collect()
}

// --- what merging makes ---

#[test]
fn racks_are_merged_into_the_rack_of_the_first_drawer_that_has_room_for_all_their_keyframes() {
    let mut places = places();
    let in_the_text = a_rack_of_the_loudness_in(
        &mut places.project,
        places.video,
        places.text,
        &[(110, -20.0), (140, 0.0)],
    );
    let in_the_video = a_rack_of_the_loudness_in(
        &mut places.project,
        places.video,
        places.video,
        &[(50, -6.0), (120, -3.0)],
    );
    let Places {
        mut project,
        video,
        text,
        ..
    } = places;
    let names_from_the_text = names_of_the_keyframes_in(&project, in_the_text);
    let names_from_the_video = names_of_the_keyframes_in(&project, in_the_video);
    let curve_before = the_loudness_of_the_video_at_every_frame(&project, video);

    // The text has the frames 100 to 159 and the keyframe at 50 is not one of them: the drawer of the video is the one.
    let kept = project.merge_racks(&[in_the_text, in_the_video]).unwrap();

    assert_eq!(kept, in_the_video);
    assert_eq!(frames_in_the_rack(&project, in_the_video), vec![50, 110, 120, 140]);
    assert!(project.located_rack(in_the_text).is_none(), "the other rack has gone");
    assert!(project.drawer_of(text).unwrap().is_empty());
    assert_eq!(
        names_of_the_keyframes_in(&project, in_the_video),
        vec![
            names_from_the_video[0],
            names_from_the_text[0],
            names_from_the_video[1],
            names_from_the_text[1]
        ],
        "every keyframe has the name it had"
    );
    assert_eq!(
        values_in_the_rack(&project, in_the_video),
        vec![db(-6.0), db(-20.0), db(-3.0), db(0.0)]
    );
    assert_eq!(
        the_loudness_of_the_video_at_every_frame(&project, video),
        curve_before,
        "the video is as loud as it was at every frame"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn racks_of_one_drawer_are_merged_into_the_one_that_is_higher_in_it_which_keeps_its_place_whatever_order_they_are_named_in()
 {
    for order in [[0, 2], [2, 0]] {
        let mut places = places();
        let first = a_rack_of_the_loudness_in(
            &mut places.project,
            places.video,
            places.text,
            &[(110, -20.0), (140, 0.0)],
        );
        let between = put_a_rack_in(
            &mut places.project,
            places.text,
            places.video,
            PropertyId::Muted,
            &[(5, PropertyValue::Flag(true))],
        );
        let last = a_rack_of_the_loudness_in(&mut places.project, places.video, places.text, &[(120, -9.0)]);
        let Places { mut project, text, .. } = places;
        let racks = [first, between, last];

        let kept = project.merge_racks(&[racks[order[0]], racks[order[1]]]).unwrap();

        assert_eq!(kept, first, "{order:?}");
        assert_eq!(names_of_the_racks_in(&project, text), vec![first, between], "{order:?}");
        assert_eq!(frames_in_the_rack(&project, first), vec![10, 20, 40], "{order:?}");
        assert_the_racks_are_sound(&project);
    }
}

#[test]
fn a_drawer_has_no_room_for_a_keyframe_one_frame_before_its_first_or_one_frame_after_its_last() {
    // The text has the frames 100 to 159. A keyframe at 99 or at 160 is not in it, so the racks are merged in the drawer of the
    // video, which is after the text in the order but has room for them.
    for (frame_of_the_other, name_of_the_edge) in
        [(99, "the frame before the first"), (160, "the frame after the last")]
    {
        let mut places = places();
        let in_the_text = a_rack_of_the_loudness_in(&mut places.project, places.video, places.text, &[(110, -20.0)]);
        let in_the_video = a_rack_of_the_loudness_in(
            &mut places.project,
            places.video,
            places.video,
            &[(frame_of_the_other, -6.0)],
        );
        let Places { mut project, .. } = places;

        let kept = project.merge_racks(&[in_the_text, in_the_video]).unwrap();

        assert_eq!(kept, in_the_video, "{name_of_the_edge}");
        assert_the_racks_are_sound(&project);
    }
    // And with the keyframes at the first and the last frames of the text, the text has room: it is the first.
    let mut places = places();
    let first = a_rack_of_the_loudness_in(&mut places.project, places.video, places.text, &[(100, -20.0)]);
    let last = a_rack_of_the_loudness_in(&mut places.project, places.video, places.video, &[(159, -6.0)]);
    let Places { mut project, .. } = places;
    assert_eq!(project.merge_racks(&[first, last]), Ok(first));
    assert_eq!(frames_in_the_rack(&project, first), vec![0, 59]);
}

#[test]
fn three_racks_of_three_drawers_are_merged_into_the_rack_of_the_track_whose_drawer_comes_first() {
    let mut places = places();
    let in_the_video = a_rack_of_the_loudness_in(&mut places.project, places.video, places.video, &[(50, -6.0)]);
    let in_the_text = a_rack_of_the_loudness_in(&mut places.project, places.video, places.text, &[(110, -20.0)]);
    let on_the_track = a_rack_of_the_loudness_in(&mut places.project, places.video, places.front_track, &[(500, -1.0)]);
    let Places { mut project, video, .. } = places;
    let curve_before = the_loudness_of_the_video_at_every_frame(&project, video);

    let kept = project.merge_racks(&[in_the_video, in_the_text, on_the_track]).unwrap();

    assert_eq!(
        kept, on_the_track,
        "the drawer of the track has room for any frame, and comes first"
    );
    assert_eq!(frames_in_the_rack(&project, on_the_track), vec![50, 110, 500]);
    assert!(project.located_rack(in_the_video).is_none() && project.located_rack(in_the_text).is_none());
    assert_eq!(the_loudness_of_the_video_at_every_frame(&project, video), curve_before);
    assert_the_racks_are_sound(&project);
}

// --- keyframes at one frame ---

#[test]
fn where_two_keyframes_are_at_one_frame_the_one_that_counted_is_kept_so_that_the_property_does_not_change() {
    let mut places = places();
    // At 120 the rack of the video counts, because it is later than the rack of the text.
    let in_the_text = a_rack_of_the_loudness_in(&mut places.project, places.video, places.text, &[(120, -9.0)]);
    let in_the_video = a_rack_of_the_loudness_in(
        &mut places.project,
        places.video,
        places.video,
        &[(50, -6.0), (120, -3.0)],
    );
    let Places { mut project, video, .. } = places;
    let curve_before = the_loudness_of_the_video_at_every_frame(&project, video);
    let kept_keyframe = names_of_the_keyframes_in(&project, in_the_video)[1];
    let lost_keyframe = names_of_the_keyframes_in(&project, in_the_text)[0];

    project.merge_racks(&[in_the_text, in_the_video]).unwrap();

    assert_eq!(frames_in_the_rack(&project, in_the_video), vec![50, 120]);
    assert_eq!(values_in_the_rack(&project, in_the_video), vec![db(-6.0), db(-3.0)]);
    assert!(project.locate_keyframe(kept_keyframe).is_some());
    assert!(
        project.locate_keyframe(lost_keyframe).is_none(),
        "the one that did not count is gone"
    );
    assert_eq!(the_loudness_of_the_video_at_every_frame(&project, video), curve_before);
    assert_the_racks_are_sound(&project);
}

#[test]
fn the_keyframe_of_the_rack_that_is_kept_gives_way_to_that_of_a_rack_that_comes_later_at_the_same_frame() {
    let mut places = places();
    // The drawer of the track comes before the drawer of the text, so the keyframe in the text counts at 120.
    let on_the_track = a_rack_of_the_loudness_in(&mut places.project, places.video, places.front_track, &[(120, -1.0)]);
    let in_the_text = a_rack_of_the_loudness_in(&mut places.project, places.video, places.text, &[(120, -9.0)]);
    let Places { mut project, video, .. } = places;
    let curve_before = the_loudness_of_the_video_at_every_frame(&project, video);
    let from_the_text = names_of_the_keyframes_in(&project, in_the_text)[0];
    let from_the_track = names_of_the_keyframes_in(&project, on_the_track)[0];

    let kept = project.merge_racks(&[on_the_track, in_the_text]).unwrap();

    assert_eq!(kept, on_the_track);
    assert_eq!(names_of_the_keyframes_in(&project, on_the_track), vec![from_the_text]);
    assert!(project.locate_keyframe(from_the_track).is_none());
    assert_eq!(values_in_the_rack(&project, on_the_track), vec![db(-9.0)]);
    assert_eq!(the_loudness_of_the_video_at_every_frame(&project, video), curve_before);
}

// --- crates ---

#[test]
fn a_crate_that_is_left_with_no_rack_by_a_merging_goes_and_one_that_has_another_rack_stays() {
    let mut places = places();
    let keyframe_crate = a_crate_after_the_texts(&mut places);
    let in_the_crate = a_rack_of_the_loudness_in(&mut places.project, places.video, keyframe_crate, &[(460, -20.0)]);
    let in_the_video = a_rack_of_the_loudness_in(&mut places.project, places.video, places.video, &[(50, -6.0)]);
    let Places { mut project, .. } = places;
    let AnimationTarget::Clip(crate_id) = keyframe_crate else {
        unreachable!("the crate is a clip");
    };

    project.merge_racks(&[in_the_crate, in_the_video]).unwrap();

    assert!(project.placement_of(crate_id).is_none(), "nothing is left in it");
    assert_eq!(frames_in_the_rack(&project, in_the_video), vec![50, 460]);
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_crate_whose_rack_the_others_are_merged_into_begins_earlier_when_a_keyframe_of_them_is_before_it() {
    let mut places = places();
    let keyframe_crate = a_crate_after_the_texts(&mut places);
    let in_the_crate = a_rack_of_the_loudness_in(
        &mut places.project,
        places.video,
        keyframe_crate,
        &[(460, -20.0), (490, -10.0)],
    );
    // A rack in the drawer of the video, with a keyframe at frame 420, before the crate in the empty frames round it (there is a text object until
    // frame 400), and one at 470, inside it. The drawer of the crate comes before that of the video (the front track is before the back track).
    let in_the_video = a_rack_of_the_loudness_in(
        &mut places.project,
        places.video,
        places.video,
        &[(420, -6.0), (470, -3.0)],
    );
    let Places { mut project, video, .. } = places;
    let curve_before = the_loudness_of_the_video_at_every_frame(&project, video);
    let AnimationTarget::Clip(crate_id) = keyframe_crate else {
        unreachable!("the crate is a clip");
    };

    let kept = project.merge_racks(&[in_the_crate, in_the_video]).unwrap();

    assert_eq!(
        kept, in_the_crate,
        "the crate has room for all of them, and comes first"
    );
    let placement = project.placement_of(crate_id).unwrap();
    assert_eq!(
        (placement.first_timeline_frame.value(), placement.timeline_frame_count()),
        (420, 71),
        "the crate begins at the first of the keyframes and ends at the last"
    );
    let located = project.located_rack(kept).unwrap();
    let frames_of_the_timeline: Vec<u64> = located
        .rack
        .keyframes()
        .iter()
        .map(|keyframe| located.timeline_frame_of(keyframe.frame))
        .collect();
    assert_eq!(frames_of_the_timeline, vec![420, 460, 470, 490]);
    assert!(project.located_rack(in_the_video).is_none(), "the other rack has gone");
    assert_eq!(
        the_loudness_of_the_video_at_every_frame(&project, video),
        curve_before,
        "the video is as loud as it was at every frame"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_crate_that_has_another_rack_stays_when_one_of_its_racks_is_merged_into_another_drawer() {
    let mut places = places();
    let keyframe_crate = a_crate_after_the_texts(&mut places);
    let in_the_crate = a_rack_of_the_loudness_in(&mut places.project, places.video, keyframe_crate, &[(460, -20.0)]);
    // (The crate begins at 460 now, where its first keyframe is, so this one is at frame 465.)
    let other_in_the_crate = put_a_rack_in(
        &mut places.project,
        keyframe_crate,
        places.video,
        PropertyId::Muted,
        &[(5, PropertyValue::Flag(true))],
    );
    let in_the_video = a_rack_of_the_loudness_in(&mut places.project, places.video, places.video, &[(50, -6.0)]);
    let Places { mut project, .. } = places;

    project.merge_racks(&[in_the_crate, in_the_video]).unwrap();

    assert_eq!(
        names_of_the_racks_in(&project, keyframe_crate),
        vec![other_in_the_crate]
    );
    assert_the_racks_are_sound(&project);
}

// --- what cannot be merged ---

#[test]
fn racks_cannot_be_merged_when_there_are_fewer_than_two_or_one_is_not_there_and_nothing_changes() {
    let mut places = places();
    let in_the_text = a_rack_of_the_loudness_in(&mut places.project, places.video, places.text, &[(110, -20.0)]);
    let in_the_video = a_rack_of_the_loudness_in(&mut places.project, places.video, places.video, &[(50, -6.0)]);
    let Places { mut project, .. } = places;
    let before = project.clone();
    let no_rack = RackId::new(TrackId::new(50), 0);

    assert_eq!(project.merge_racks(&[]), Err(EditError::TooFewRacksToMerge));
    assert_eq!(project.merge_racks(&[in_the_text]), Err(EditError::TooFewRacksToMerge));
    assert_eq!(
        project.merge_racks(&[in_the_text, in_the_text]),
        Err(EditError::TooFewRacksToMerge),
        "one rack is not two, however often it is named"
    );
    assert_eq!(
        project.merge_racks(&[in_the_text, no_rack, in_the_video]),
        Err(EditError::UnknownRack { rack: no_rack })
    );
    assert_eq!(project, before);
}

#[test]
fn racks_of_different_properties_or_of_different_things_cannot_be_merged() {
    let mut places = places();
    let loudness = a_rack_of_the_loudness_in(&mut places.project, places.video, places.video, &[(50, -6.0)]);
    let other_loudness = a_rack_of_the_loudness_in(&mut places.project, places.video, places.text, &[(110, -6.0)]);
    let switch = put_a_rack_in(
        &mut places.project,
        places.video,
        places.video,
        PropertyId::Muted,
        &[(5, PropertyValue::Flag(true))],
    );
    let turn_of_the_video = put_a_rack_in(
        &mut places.project,
        places.video,
        places.video,
        PropertyId::Rotation,
        &[(5, PropertyValue::Number(10.0))],
    );
    let turn_of_the_text = put_a_rack_in(
        &mut places.project,
        places.text,
        places.text,
        PropertyId::Rotation,
        &[(7, PropertyValue::Number(20.0))],
    );
    let Places { mut project, .. } = places;
    let before = project.clone();

    for racks in [
        vec![loudness, switch],
        vec![loudness, other_loudness, switch],
        vec![turn_of_the_video, turn_of_the_text],
    ] {
        assert_eq!(
            project.merge_racks(&racks),
            Err(EditError::RacksOfDifferentThingsCannotBeMerged),
            "{racks:?}"
        );
        assert_eq!(
            project.check_the_merging_of_racks(&racks),
            project.clone().merge_racks(&racks).map(|_| ())
        );
    }
    assert_eq!(project, before);
}

#[test]
fn racks_whose_keyframes_do_not_all_fit_in_any_drawer_that_one_of_them_is_in_cannot_be_merged() {
    let mut places = places();
    let in_the_text = a_rack_of_the_loudness_in(
        &mut places.project,
        places.video,
        places.text,
        &[(110, -20.0), (140, 0.0)],
    );
    let in_the_other_text =
        a_rack_of_the_loudness_in(&mut places.project, places.video, places.other_text, &[(310, -6.0)]);
    let Places { mut project, .. } = places;
    let before = project.clone();

    // The drawer of the text has the frames 100 to 159 and that of the other text 300 to 399: none has 110 and 310.
    assert_eq!(
        project.merge_racks(&[in_the_text, in_the_other_text]),
        Err(EditError::NoRoomForTheKeyframes)
    );
    assert_eq!(
        project.check_the_merging_of_racks(&[in_the_text, in_the_other_text]),
        Err(EditError::NoRoomForTheKeyframes)
    );
    assert_eq!(project, before);
}

// --- whatever is merged ---

#[test]
fn whatever_racks_are_merged_the_property_is_what_it_was_at_every_frame_and_what_cannot_be_merged_changes_nothing() {
    let mut merged = 0;
    for seed in 1..=200u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed);
        let mut places = places();
        let keyframe_crate = a_crate_after_the_texts(&mut places);
        let owners = [
            places.video,
            places.text,
            places.other_text,
            keyframe_crate,
            places.front_track,
        ];
        for _ in 0..3 + random.next_below(5) {
            let owner = owners[random.next_below(owners.len() as u64) as usize];
            // (Where a keyframe crate can have keyframes is the empty stretch round it, and not only where it is.)
            let room = places.project.extent_of_the_drawer_of(owner).unwrap().room;
            let length = room.end_frame.unwrap_or(600) - room.first_frame;
            let mut frames: Vec<u64> = (0..1 + random.next_below(3))
                .map(|_| room.first_frame + random.next_below(length))
                .collect();
            frames.sort_unstable();
            frames.dedup();
            let keyframes: Vec<(u64, f32)> = frames
                .iter()
                .map(|frame| (*frame, -(random.next_below(300) as f32) / 10.0))
                .collect();
            a_rack_of_the_loudness_in(&mut places.project, places.video, owner, &keyframes);
        }
        // (If no rack was put in the crate it is not a crate that stays.)
        places.project.fit_the_crate(keyframe_crate).unwrap();
        let Places { mut project, video, .. } = places;
        assert_the_racks_are_sound(&project);

        let racks: Vec<RackId> = project.located_racks().map(|located| located.rack.id()).collect();
        let chosen: Vec<RackId> = racks.iter().copied().filter(|_| random.next_below(2) == 0).collect();
        let before = project.clone();
        let context = format!("seed {seed}: {chosen:?}");
        let curve_before = the_loudness_of_the_video_at_every_frame(&project, video);
        // (Where a keyframe of a rack that is not merged is at the frame of one that is, which of the two counts is a matter of
        // which rack is later, and merging moves keyframes to another rack: see the test of that.)
        let frames_of = |racks: &mut dyn Iterator<Item = LocatedRack<'_>>| -> std::collections::BTreeSet<u64> {
            racks
                .flat_map(|located| {
                    located
                        .rack
                        .keyframes()
                        .iter()
                        .map(|keyframe| located.timeline_frame_of(keyframe.frame))
                        .collect::<Vec<_>>()
                })
                .collect()
        };
        let frames_of_the_merged = frames_of(&mut before.located_racks().filter(|l| chosen.contains(&l.rack.id())));
        let frames_of_the_others = frames_of(&mut before.located_racks().filter(|l| !chosen.contains(&l.rack.id())));
        let there_are_ties_with_other_racks = frames_of_the_merged
            .intersection(&frames_of_the_others)
            .next()
            .is_some();

        let asked = project.check_the_merging_of_racks(&chosen);
        let result = project.merge_racks(&chosen);

        assert_eq!(asked, result.map(|_| ()), "{context}");
        match result {
            Err(_) => assert_eq!(project, before, "{context}"),
            Ok(kept) => {
                merged += 1;
                assert!(chosen.contains(&kept), "{context}");
                if !there_are_ties_with_other_racks {
                    assert_eq!(
                        the_loudness_of_the_video_at_every_frame(&project, video),
                        curve_before,
                        "{context}"
                    );
                }
                for gone in chosen.iter().filter(|rack| **rack != kept) {
                    assert!(project.located_rack(*gone).is_none(), "{gone} is gone: {context}");
                }
                // One keyframe for every frame of the timeline that any of the racks had one at.
                let frames_before: std::collections::BTreeSet<u64> = chosen
                    .iter()
                    .flat_map(|rack| {
                        let located = before.located_rack(*rack).unwrap();
                        located
                            .rack
                            .keyframes()
                            .iter()
                            .map(|keyframe| located.timeline_frame_of(keyframe.frame))
                            .collect::<Vec<_>>()
                    })
                    .collect();
                let kept_rack = project.located_rack(kept).unwrap();
                let frames_after: std::collections::BTreeSet<u64> = kept_rack
                    .rack
                    .keyframes()
                    .iter()
                    .map(|keyframe| kept_rack.timeline_frame_of(keyframe.frame))
                    .collect();
                assert_eq!(frames_after, frames_before, "{context}");
                assert_eq!(kept_rack.rack.len(), frames_before.len(), "{context}");
                // The rack that was kept is the first of those named that is in a drawer that has room for every keyframe.
                let (first, last) = (*frames_before.first().unwrap(), *frames_before.last().unwrap());
                let expected = before
                    .located_racks()
                    .filter(|located| chosen.contains(&located.rack.id()))
                    .find(|located| located.room.contains(first) && located.room.contains(last))
                    .map(|located| located.rack.id());
                assert_eq!(Some(kept), expected, "{context}");
            }
        }
        assert_eq!(project.problems_with_the_racks(), Vec::<String>::new(), "{context}");
    }
    assert!(merged > 30, "the test merges racks: {merged} merges were made");
}

#[test]
fn a_keyframe_that_is_at_one_frame_with_a_keyframe_of_a_rack_that_is_not_merged_may_stop_counting_because_the_rack_it_is_in_has_changed()
 {
    let mut places = places();
    // At 120 two racks have a keyframe: the one in the text and the one in the video, which is later and so counts. Merged with a
    // rack of the drawer of the track, which comes before the text, the keyframe is in a rack that is not later than the text's.
    let in_the_text = a_rack_of_the_loudness_in(&mut places.project, places.video, places.text, &[(120, -9.0)]);
    let in_the_video = a_rack_of_the_loudness_in(&mut places.project, places.video, places.video, &[(120, -3.0)]);
    let on_the_track = a_rack_of_the_loudness_in(&mut places.project, places.video, places.front_track, &[(300, -1.0)]);
    let Places { mut project, video, .. } = places;
    assert_eq!(gain_at(&project, video, 120), -3.0, "the one in the video counts");

    project.merge_racks(&[in_the_video, on_the_track]).unwrap();

    assert_eq!(
        gain_at(&project, video, 120),
        -9.0,
        "the rack that has the keyframe is now before the text's, and the later rack counts"
    );
    assert_eq!(frames_in_the_rack(&project, on_the_track), vec![120, 300]);
    assert!(project.located_rack(in_the_text).is_some());
}
