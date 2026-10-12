//! Tests of what the racks of a project say together, wherever they are kept: the keyframes of a clip that are in the drawer of
//! another clip, or of a track, and what happens to them when clips are moved, cut, shortened and deleted.

use super::tests::{first_clip_location, frame, project_with_a_video};
use super::*;
use crate::test_support::DeterministicRandomNumbers;

pub(super) fn db(decibels: f32) -> PropertyValue {
    PropertyValue::Number(decibels)
}

/// A project with a video of 600 frames on its track, and in front of it a track with a text object from frame 100 to 160.
pub(super) fn a_big_video_and_a_small_text() -> (Project, ClipId, ClipId) {
    let mut project = project_with_a_video(600);
    let video = first_clip_location(&project, 0);
    let in_front = project.add_track_in_front();
    let (text, lasts) = project
        .insert_text(in_front, frame(100), TextObject::default(), 60)
        .unwrap();
    assert_eq!(lasts, 60);
    (project, video, text)
}

/// Puts a rack in the drawer of `owner`, of the keyframes of `property` of `target` at these frames (counted from the start of the
/// drawer), without anything else being done.
pub(super) fn put_a_rack_in(
    project: &mut Project,
    owner: AnimationTarget,
    target: AnimationTarget,
    property: PropertyId,
    keyframes: &[(u64, PropertyValue)],
) -> RackId {
    let position = match owner {
        AnimationTarget::Clip(clip) => project.track_position_holding(clip).unwrap(),
        AnimationTarget::Track(track) => project.track_position(track).unwrap(),
    };
    let id = project.tracks[position].make_a_rack_id();
    let mut rack = Rack::new(id, target, property);
    for (frame, value) in keyframes {
        rack.set(*frame, value.clone()).unwrap();
    }
    project.drawer_mut(owner).unwrap().push(rack);
    id
}

pub(super) fn gain_at(project: &Project, target: AnimationTarget, at: u64) -> f32 {
    match project.property_value_at(target, PropertyId::Gain, frame(at)) {
        Some(PropertyValue::Number(decibels)) => decibels,
        other => panic!("not a gain: {other:?}"),
    }
}

/// The names of the racks in the drawer of `owner`, in the order they are in.
pub(super) fn names_of_the_racks_in(project: &Project, owner: AnimationTarget) -> Vec<RackId> {
    project
        .drawer_of(owner)
        .expect("the drawer is in the project")
        .racks()
        .iter()
        .map(Rack::id)
        .collect()
}

/// The frames of the timeline that the keyframes of `property` of the clip `video` are at, from all the racks that have some.
pub(super) fn where_the_video_has_keyframes(project: &Project, video: ClipId, property: PropertyId) -> Vec<u64> {
    keyframe_frames(project, AnimationTarget::Clip(video), property)
}

pub(super) fn keyframe_frames(project: &Project, target: AnimationTarget, property: PropertyId) -> Vec<u64> {
    project
        .keyframes_of(target, property)
        .iter()
        .map(|keyframe| keyframe.frame)
        .collect()
}

pub(super) fn assert_the_racks_are_sound(project: &Project) {
    assert_eq!(project.problems_with_the_racks(), Vec::<String>::new());
}

// --- what a property is when its keyframes are kept elsewhere ---

#[test]
fn keyframes_kept_in_the_drawer_of_another_clip_animate_the_clip_they_are_of_at_frames_of_the_timeline() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    // The drawer of the text starts at frame 100: its keyframes are at frames 110 and 140.
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(10, db(-20.0)), (40, db(0.0))],
    );

    assert!(project.is_animated(big, PropertyId::Gain));
    assert!(!project.is_animated(AnimationTarget::Clip(text), PropertyId::Gain));
    assert_eq!(gain_at(&project, big, 110), -20.0);
    assert_eq!(gain_at(&project, big, 125), -10.0);
    assert_eq!(gain_at(&project, big, 140), 0.0);
    assert_eq!(
        gain_at(&project, big, 0),
        -20.0,
        "before the first it is what the first says"
    );
    assert_eq!(
        gain_at(&project, big, 500),
        0.0,
        "and after the last, what the last says"
    );
    assert_eq!(keyframe_frames(&project, big, PropertyId::Gain), vec![110, 140]);
    assert_eq!(
        project.keyframe_before(big, PropertyId::Gain, frame(125)),
        Some(frame(110))
    );
    assert_eq!(
        project.keyframe_after(big, PropertyId::Gain, frame(125)),
        Some(frame(140))
    );
    assert!(project.has_keyframe_at(big, PropertyId::Gain, frame(110)));
    assert!(!project.has_keyframe_at(big, PropertyId::Gain, frame(111)));
    assert_the_racks_are_sound(&project);
}

#[test]
fn racks_in_the_drawers_of_several_clips_and_of_a_track_are_one_curve() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let front = project.tracks()[0].id();
    let (second_text, _) = project
        .insert_text(front, frame(300), TextObject::default(), 60)
        .unwrap();
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(0, db(0.0)), (10, db(-12.0))],
    );
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(second_text),
        big,
        PropertyId::Gain,
        &[(0, db(0.0)), (10, db(-12.0))],
    );
    put_a_rack_in(
        &mut project,
        AnimationTarget::Track(front),
        big,
        PropertyId::Gain,
        &[(200, db(6.0))],
    );

    assert_eq!(
        keyframe_frames(&project, big, PropertyId::Gain),
        vec![100, 110, 200, 300, 310]
    );
    // From the last keyframe of the first dip (-12 dB at frame 110) to the one the track has (6 dB at frame 200).
    assert!((gain_at(&project, big, 150) + 4.0).abs() < 1e-4);
    assert!((gain_at(&project, big, 155) + 3.0).abs() < 1e-4);
    assert!(
        (gain_at(&project, big, 250) - 3.0).abs() < 1e-4,
        "half way from 6 dB to the 0 dB at frame 300"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn where_two_keyframes_of_a_property_are_at_one_frame_the_one_in_the_rack_that_comes_later_is_the_one_that_counts() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    // (The track in front is first, so the drawer of the video, which is on the track at the back, comes later.)
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(0, db(-10.0))],
    );
    put_a_rack_in(&mut project, big, big, PropertyId::Gain, &[(100, db(-5.0))]);

    assert_eq!(gain_at(&project, big, 100), -5.0);
    let location = project.locate_the_keyframe_at(big, PropertyId::Gain, 100).unwrap();
    assert_eq!(location.owner, big, "in the drawer of the video itself");
    assert_eq!(
        keyframe_frames(&project, big, PropertyId::Gain),
        vec![100],
        "they are one keyframe"
    );
}

#[test]
fn the_resolved_animations_say_what_the_animation_of_each_thing_is() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(0, db(0.0)), (30, db(-9.0))],
    );
    put_a_rack_in(
        &mut project,
        big,
        big,
        PropertyId::Muted,
        &[(5, PropertyValue::Flag(true))],
    );

    let resolved = project.resolved_animations();

    assert_eq!(resolved.of(big), &project.animation_of(big));
    assert!(resolved.of(big).is_animated(PropertyId::Gain) && resolved.of(big).is_animated(PropertyId::Muted));
    assert_eq!(
        resolved.of(AnimationTarget::Clip(text)),
        &Animation::default(),
        "nothing animates the text"
    );
    assert_eq!(resolved.of(big).value_at(PropertyId::Gain, 115), Some(db(-4.5)));
}

// --- changing and making and taking away keyframes that are kept elsewhere ---

#[test]
fn changing_a_property_at_a_frame_that_has_its_keyframe_kept_elsewhere_changes_it_there() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(10, db(-20.0))],
    );

    let made = project
        .set_property_at(big, PropertyId::Gain, db(-3.0), frame(110))
        .unwrap();

    assert_eq!(made, None, "no keyframe was made");
    assert_eq!(gain_at(&project, big, 110), -3.0);
    assert!(
        project.placement_of(video).unwrap().drawer.is_empty(),
        "the clip's own drawer has nothing in it"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_keyframe_made_for_a_clip_whose_other_keyframes_are_kept_elsewhere_goes_in_its_own_drawer_and_the_result_says_so() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(10, db(-20.0))],
    );

    let made = project
        .add_keyframe(big, PropertyId::Gain, frame(300))
        .unwrap()
        .unwrap();

    assert_eq!(made.owner, big);
    let own = project
        .placement_of(video)
        .unwrap()
        .drawer
        .rack_for(big, PropertyId::Gain)
        .unwrap();
    assert_eq!(own.id(), made.rack);
    assert_eq!(own.keyframe(made.keyframe).unwrap().frame, 300);
    assert_eq!(keyframe_frames(&project, big, PropertyId::Gain), vec![110, 300]);
    assert_eq!(
        project.add_keyframe(big, PropertyId::Gain, frame(300)).unwrap(),
        None,
        "one is there already, and nothing changes"
    );
    assert_eq!(
        project.add_keyframe(big, PropertyId::Gain, frame(110)).unwrap(),
        None,
        "and one that is kept elsewhere counts"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn taking_away_a_keyframe_that_is_kept_elsewhere_takes_it_from_there_and_the_rack_goes_when_it_has_none_left() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let owner = AnimationTarget::Clip(text);
    put_a_rack_in(
        &mut project,
        owner,
        big,
        PropertyId::Gain,
        &[(10, db(-12.0)), (40, db(-6.0))],
    );

    project.remove_keyframe(big, PropertyId::Gain, frame(110)).unwrap();
    assert_eq!(keyframe_frames(&project, big, PropertyId::Gain), vec![140]);
    assert_eq!(project.drawer_of(owner).unwrap().len(), 1);

    project.remove_keyframe(big, PropertyId::Gain, frame(140)).unwrap();
    assert!(project.drawer_of(owner).unwrap().is_empty(), "the rack has gone");
    assert!(!project.is_animated(big, PropertyId::Gain));
    assert_eq!(
        project.placement_of(video).unwrap().clip.as_media().unwrap().gain(),
        Gain::from_decibels(-6.0),
        "the video stays as the last keyframe said"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_keyframe_at_a_frame_that_two_racks_have_is_taken_from_both_and_does_not_show_again() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(0, db(-10.0)), (30, db(0.0))],
    );
    put_a_rack_in(&mut project, big, big, PropertyId::Gain, &[(100, db(-5.0))]);

    project.remove_keyframe(big, PropertyId::Gain, frame(100)).unwrap();

    assert_eq!(keyframe_frames(&project, big, PropertyId::Gain), vec![130]);
    assert!(project.placement_of(video).unwrap().drawer.is_empty());
    assert_the_racks_are_sound(&project);
}

// --- moving, deleting ---

#[test]
fn the_keyframes_in_the_drawer_of_a_clip_move_with_it_whatever_they_are_of() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let front = project.tracks()[0].id();
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(10, db(-20.0))],
    );

    project
        .move_clips(&[ClipMove {
            clip: text,
            to_track: front,
            start: frame(250),
        }])
        .unwrap();

    assert_eq!(keyframe_frames(&project, big, PropertyId::Gain), vec![260]);
    assert_eq!(gain_at(&project, big, 100), -20.0, "still the same before the first");
    assert_the_racks_are_sound(&project);
}

#[test]
fn deleting_a_clip_takes_the_racks_that_animate_it_from_the_drawers_of_other_clips_and_a_clip_that_holds_racks_takes_them_with_it()
 {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let owner = AnimationTarget::Clip(text);
    put_a_rack_in(&mut project, owner, big, PropertyId::Gain, &[(10, db(-20.0))]);
    put_a_rack_in(
        &mut project,
        big,
        owner,
        PropertyId::Rotation,
        &[(5, PropertyValue::Number(30.0))],
    );
    assert_the_racks_are_sound(&project);

    project.delete_clip(video).unwrap();

    assert!(
        project.drawer_of(owner).unwrap().is_empty(),
        "a rack of the loudness of a clip that is gone has nothing to do"
    );
    assert_the_racks_are_sound(&project);

    // The text is animated by a rack in the drawer of another text, which goes when that is deleted.
    let front = project.tracks()[0].id();
    let (other, _) = project
        .insert_text(front, frame(300), TextObject::default(), 30)
        .unwrap();
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(other),
        owner,
        PropertyId::Rotation,
        &[(2, PropertyValue::Number(10.0))],
    );
    assert!(project.is_animated(owner, PropertyId::Rotation));
    project.delete_clip(other).unwrap();
    assert!(
        !project.is_animated(owner, PropertyId::Rotation),
        "the keyframes went with the drawer they were in"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn deleting_a_track_takes_the_racks_that_animate_it_and_its_clips_from_the_drawers_that_are_left() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let front = project.tracks()[0].id();
    let back = project.tracks()[1].id();
    // The text on the track in front holds racks of the loudness of the video and of its track, which are at the back.
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(10, db(-20.0))],
    );
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        AnimationTarget::Track(back),
        PropertyId::Gain,
        &[(5, db(-1.0))],
    );
    put_a_rack_in(
        &mut project,
        AnimationTarget::Track(front),
        AnimationTarget::Track(back),
        PropertyId::Muted,
        &[(3, PropertyValue::Flag(true))],
    );
    assert_the_racks_are_sound(&project);

    project.delete_track(back).unwrap();

    assert!(project.drawer_of(AnimationTarget::Clip(text)).unwrap().is_empty());
    assert!(project.drawer_of(AnimationTarget::Track(front)).unwrap().is_empty());
    assert_the_racks_are_sound(&project);
}

// --- cutting ---

#[test]
fn a_clip_that_is_cut_goes_on_being_animated_in_both_halves_by_the_keyframes_that_are_kept_elsewhere() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(0, db(0.0)), (40, db(-12.0))],
    );

    let cut = project.split_at(frame(120), Some(&[video]));

    let second_half = cut[0].1.second_half;
    assert_eq!(gain_at(&project, big, 110), -3.0);
    assert_eq!(gain_at(&project, big, 119), -5.7, "the first half is as the whole was");
    assert!((gain_at(&project, AnimationTarget::Clip(second_half), 120) + 6.0).abs() < 1e-4);
    assert!(
        (gain_at(&project, AnimationTarget::Clip(second_half), 130) + 9.0).abs() < 1e-4,
        "and so is the second"
    );
    assert_eq!(
        project.drawer_of(AnimationTarget::Clip(text)).unwrap().len(),
        2,
        "the drawer of the text has a rack for each half"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn cutting_a_clip_that_holds_racks_of_other_clips_divides_them_and_leaves_what_they_animate_as_it_was() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(0, db(0.0)), (20, db(-12.0)), (50, db(-3.0))],
    );
    let before: Vec<f32> = (0..600).map(|at| gain_at(&project, big, at)).collect();

    // (Only the text is cut: the video is not at the frame the cut is made at.)
    let cut = project.split_at(frame(130), Some(&[text]));

    assert_eq!(cut.len(), 1);
    let after: Vec<f32> = (0..600).map(|at| gain_at(&project, big, at)).collect();
    assert_eq!(before, after);
    let racks: Vec<usize> = project.located_racks().map(|located| located.rack.len()).collect();
    assert_eq!(
        racks,
        vec![2, 1],
        "keyframes before and after the cut, in the drawer of each half"
    );
    assert_the_racks_are_sound(&project);
}

/// Whatever racks animate a clip, and wherever they are, cutting at any frame leaves every frame of it with what it had.
#[test]
fn whatever_racks_there_are_a_cut_leaves_every_frame_of_the_clips_with_the_value_the_whole_had() {
    for seed in 1..=150u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed);
        let (mut project, video, text) = a_big_video_and_a_small_text();
        let big = AnimationTarget::Clip(video);
        let front = project.tracks()[0].id();
        let mut random_keyframes = |limit: u64| -> Vec<(u64, PropertyValue)> {
            (0..1 + random.next_below(4))
                .map(|_| {
                    (
                        random.next_below(limit),
                        db(random.next_below(300) as f32 / 10.0 - 15.0),
                    )
                })
                .collect()
        };
        // In its own drawer, in the drawer of the text, and in the drawer of the track in front.
        let own = random_keyframes(600);
        let held_by_the_text = random_keyframes(60);
        let held_by_the_track = random_keyframes(600);
        put_a_rack_in(&mut project, big, big, PropertyId::Gain, &own);
        put_a_rack_in(
            &mut project,
            AnimationTarget::Clip(text),
            big,
            PropertyId::Gain,
            &held_by_the_text,
        );
        put_a_rack_in(
            &mut project,
            AnimationTarget::Track(front),
            big,
            PropertyId::Gain,
            &held_by_the_track,
        );
        let before: Vec<f32> = (0..600).map(|at| gain_at(&project, big, at)).collect();
        let cut_at = 1 + random.next_below(599);

        let cut = project.split_at(frame(cut_at), None);

        let (first_half, second_half) = cut
            .iter()
            .map(|(_, outcome)| (outcome.first_half, outcome.second_half))
            .find(|(first, _)| *first == video)
            .expect("the video is cut");
        for (at, expected) in before.iter().enumerate() {
            let half = if (at as u64) < cut_at { first_half } else { second_half };
            let now = gain_at(&project, AnimationTarget::Clip(half), at as u64);
            assert!(
                (now - expected).abs() < 1e-3,
                "seed {seed}, cut at {cut_at}: frame {at} is {now} dB and was {expected}"
            );
        }
        assert_eq!(project.problems_with_the_racks(), Vec::<String>::new(), "seed {seed}");
    }
}

// --- making a clip shorter ---

#[test]
fn making_a_clip_shorter_cuts_off_the_keyframes_it_holds_and_the_clip_that_they_animate_goes_on_having_the_rest() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(10, db(-20.0)), (50, db(0.0))],
    );

    let lasts = project.set_clip_duration(text, 30).unwrap();

    assert_eq!(lasts, 30);
    // What is kept: the keyframe at 10 and, at the new last frame, the value the keyframes had there.
    assert_eq!(keyframe_frames(&project, big, PropertyId::Gain), vec![110, 129]);
    assert!(
        (gain_at(&project, big, 129) + 10.5).abs() < 1e-4,
        "19 of the 40 frames of the way from -20 dB to 0 dB"
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn making_a_clip_shorter_keeps_its_own_keyframes_as_they_were_by_all_the_keyframes_there_are_for_it() {
    for seed in 1..=100u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed);
        let (mut project, _, text) = a_big_video_and_a_small_text();
        let mine = AnimationTarget::Clip(text);
        let front = project.tracks()[0].id();
        let mut random_keyframes = |limit: u64| -> Vec<(u64, PropertyValue)> {
            (0..1 + random.next_below(4))
                .map(|_| {
                    (
                        random.next_below(limit),
                        PropertyValue::Number(random.next_below(200) as f32 / 10.0),
                    )
                })
                .collect()
        };
        let own = random_keyframes(60);
        let held_by_the_track = random_keyframes(400);
        put_a_rack_in(&mut project, mine, mine, PropertyId::Rotation, &own);
        put_a_rack_in(
            &mut project,
            AnimationTarget::Track(front),
            mine,
            PropertyId::Rotation,
            &held_by_the_track,
        );
        let rotation_at =
            |project: &Project, at: u64| match project.property_value_at(mine, PropertyId::Rotation, frame(at)) {
                Some(PropertyValue::Number(degrees)) => degrees,
                other => panic!("not a rotation: {other:?}"),
            };
        let before: Vec<f32> = (100..160).map(|at| rotation_at(&project, at)).collect();
        let new_length = 1 + random.next_below(59);

        project.set_clip_duration(text, new_length).unwrap();

        for (index, expected) in before.iter().take(new_length as usize).enumerate() {
            let now = rotation_at(&project, 100 + index as u64);
            assert!(
                (now - expected).abs() < 1e-3,
                "seed {seed}, shortened to {new_length}: frame {} is {now} and was {expected}",
                100 + index
            );
        }
        assert_eq!(project.problems_with_the_racks(), Vec::<String>::new(), "seed {seed}");
    }
}

// --- the checks themselves ---

#[test]
fn a_project_whose_racks_are_in_order_has_no_problems_and_what_is_wrong_is_said() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let owner = AnimationTarget::Clip(text);
    put_a_rack_in(
        &mut project,
        owner,
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(10, db(-20.0))],
    );
    assert_the_racks_are_sound(&project);

    // A keyframe beyond the end of the drawer it is in, a rack of something that is not there, a property that clip has not.
    put_a_rack_in(
        &mut project,
        owner,
        AnimationTarget::Clip(video),
        PropertyId::Muted,
        &[(61, PropertyValue::Flag(true))],
    );
    put_a_rack_in(
        &mut project,
        owner,
        AnimationTarget::Clip(ClipId::new(TrackId::new(77), 0)),
        PropertyId::Gain,
        &[(1, db(0.0))],
    );
    put_a_rack_in(&mut project, owner, owner, PropertyId::Gain, &[(1, db(0.0))]);
    let problems = project.problems_with_the_racks();
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("drawer it is in lasts 60 frames"))
    );
    assert!(problems.iter().any(|problem| problem.contains("is not there")));
    assert!(problems.iter().any(|problem| problem.contains("does not have it")));
}
