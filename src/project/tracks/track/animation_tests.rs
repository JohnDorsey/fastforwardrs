//! Tests of the keyframes of a track and of its clips: where they are kept, and what happens to them when a clip is
//! cut, changes its length, is deleted or moved.

use super::tests::{clip_id, frame, picture, sound, spans, speed, text, track_of, video};
use super::*;
use crate::project::{AnimationTarget, Drawer, Effects, FrameRate, PropertyId, PropertyValue, Rack, RackId};
use crate::test_support::DeterministicRandomNumbers;

fn db(decibels: f32) -> PropertyValue {
    PropertyValue::Number(decibels)
}

/// A drawer that has a rack of the loudness of `clip` with keyframes at these frames.
fn a_drawer_with_the_loudness_of(clip: ClipId, frames_and_decibels: &[(u64, f32)]) -> Drawer {
    let mut rack = Rack::new(
        RackId::new(TrackId::new(9), 0),
        AnimationTarget::Clip(clip),
        PropertyId::Gain,
    );
    for (frame, decibels) in frames_and_decibels {
        rack.set(*frame, db(*decibels)).unwrap();
    }
    Drawer::with_racks([rack])
}

/// A track with a video of 100 frames (clip 0) whose loudness is 0 dB at its first frame and -20 dB at its last.
fn track_with_a_fading_video() -> Track {
    let mut track = track_of(vec![(0, video(0, 100, 1))]);
    track
        .set_clip_keyframe(clip_id(0), PropertyId::Gain, 0, db(0.0))
        .unwrap();
    track
        .set_clip_keyframe(clip_id(0), PropertyId::Gain, 99, db(-20.0))
        .unwrap();
    track
}

fn gain_of(track: &Track, clip: ClipId, frames_in: u64) -> f32 {
    match track
        .placement_of(clip)
        .unwrap()
        .own_animation()
        .value_at(PropertyId::Gain, frames_in)
    {
        Some(PropertyValue::Number(decibels)) => decibels,
        other => panic!("the gain of {clip} at {frames_in} is {other:?}"),
    }
}

fn keyframe_frames(track: &Track, clip: ClipId, property: PropertyId) -> Vec<u64> {
    track
        .placement_of(clip)
        .unwrap()
        .own_animation()
        .keyframes_of(property)
        .map(|keyframes| keyframes.iter().map(|keyframe| keyframe.frame).collect())
        .unwrap_or_default()
}

/// Every keyframe of every clip is at a frame the clip has.
fn assert_keyframes_are_inside_their_clips(track: &Track) {
    for placement in track.placements() {
        for (property, keyframes) in placement.own_animation().properties() {
            for keyframe in keyframes.iter() {
                assert!(
                    keyframe.frame < placement.timeline_frame_count(),
                    "{property:?} of {} has a keyframe at {}, and the clip has {} frames",
                    placement.id,
                    keyframe.frame,
                    placement.timeline_frame_count()
                );
            }
        }
    }
}

// --- putting them on and taking them off ---

#[test]
fn nothing_is_animated_to_begin_with() {
    let track = track_of(vec![(0, video(0, 100, 1))]);
    assert!(track.own_animation().is_empty());
    assert!(track.placement_of(clip_id(0)).unwrap().own_animation().is_empty());
    assert!(track.entries()[0].own_animation().is_empty());
}

#[test]
fn a_keyframe_can_be_put_on_a_clip_replaced_and_taken_away_again() {
    let mut track = track_of(vec![(10, video(0, 100, 1))]);
    track
        .set_clip_keyframe(clip_id(0), PropertyId::Gain, 20, db(-5.0))
        .unwrap();
    assert_eq!(keyframe_frames(&track, clip_id(0), PropertyId::Gain), vec![20]);
    track
        .set_clip_keyframe(clip_id(0), PropertyId::Gain, 20, db(-9.0))
        .unwrap();
    assert_eq!(gain_of(&track, clip_id(0), 20), -9.0, "in place of the one there");
    assert_eq!(keyframe_frames(&track, clip_id(0), PropertyId::Gain), vec![20]);
}

#[test]
fn a_keyframe_counts_frames_from_the_first_frame_of_the_clip_wherever_the_clip_is_on_the_timeline() {
    let mut track = track_of(vec![(500, video(0, 100, 1))]);
    track
        .set_clip_keyframe(clip_id(0), PropertyId::Gain, 3, db(1.0))
        .unwrap();
    assert_eq!(keyframe_frames(&track, clip_id(0), PropertyId::Gain), vec![3]);
    assert_eq!(spans(&track), vec![(500, 600)]);
}

#[test]
fn a_keyframe_is_refused_where_the_clip_is_not_for_a_property_it_does_not_have_or_for_the_wrong_kind_of_value() {
    let mut track = track_of(vec![(0, video(0, 100, 1)), (0, text(30)), (0, sound(0, 10, 1))]);
    let before = track.clone();
    assert_eq!(
        track.set_clip_keyframe(clip_id(0), PropertyId::Gain, 100, db(0.0)),
        Err(ClipEditError::NotWithinTheClip {
            clip: clip_id(0),
            frames_in: 100
        }),
        "the clip has frames 0 to 99"
    );
    assert_eq!(
        track.set_clip_keyframe(clip_id(1), PropertyId::Gain, 5, db(0.0)),
        Err(ClipEditError::NotAPropertyOfThisClip {
            clip: clip_id(1),
            property: PropertyId::Gain
        }),
        "text has no loudness"
    );
    assert_eq!(
        track.set_clip_keyframe(clip_id(2), PropertyId::Position, 5, PropertyValue::Pair(0.5, 0.5)),
        Err(ClipEditError::NotAPropertyOfThisClip {
            clip: clip_id(2),
            property: PropertyId::Position
        }),
        "a sound is not on the picture"
    );
    assert_eq!(
        track.set_clip_keyframe(clip_id(0), PropertyId::Gain, 5, PropertyValue::Flag(true)),
        Err(ClipEditError::NotAPropertyOfThisClip {
            clip: clip_id(0),
            property: PropertyId::Gain
        }),
        "a loudness is a number"
    );
    assert_eq!(
        track.set_clip_keyframe(clip_id(99), PropertyId::Gain, 5, db(0.0)),
        Err(ClipEditError::UnknownClip { clip: clip_id(99) })
    );
    assert_eq!(track, before, "nothing was changed");
}

#[test]
fn a_track_has_keyframes_of_its_own_for_the_properties_it_has() {
    let mut track = track_of(vec![(0, video(0, 100, 1))]);
    assert!(track.set_own_keyframe(PropertyId::Gain, frame(10), db(-3.0)));
    assert!(
        track.set_own_keyframe(PropertyId::Muted, frame(500), PropertyValue::Flag(true)),
        "even past its clips"
    );
    assert!(!track.set_own_keyframe(PropertyId::Position, frame(10), PropertyValue::Pair(0.0, 0.0)));
    assert!(!track.set_own_keyframe(PropertyId::Gain, frame(11), PropertyValue::Flag(true)));
    assert_eq!(track.own_animation().keyframes_of(PropertyId::Gain).unwrap().len(), 1);
}

// --- cutting ---

#[test]
fn each_half_of_a_cut_clip_has_what_the_clip_had_at_every_frame_and_its_keyframes_from_its_own_first_frame() {
    let mut track = track_with_a_fading_video();
    let outcome = track.split_at(frame(40)).unwrap();

    assert_eq!(outcome.first_half, clip_id(0));
    for frames_in in 0..40 {
        let expected = -20.0 * frames_in as f32 / 99.0;
        assert!(
            (gain_of(&track, outcome.first_half, frames_in) - expected).abs() < 1e-3,
            "first half, frame {frames_in}"
        );
    }
    for frames_in in 0..60 {
        let expected = -20.0 * (frames_in + 40) as f32 / 99.0;
        assert!(
            (gain_of(&track, outcome.second_half, frames_in) - expected).abs() < 1e-3,
            "second half, frame {frames_in}"
        );
    }
    assert_eq!(
        keyframe_frames(&track, outcome.first_half, PropertyId::Gain),
        vec![0, 39]
    );
    assert_eq!(
        keyframe_frames(&track, outcome.second_half, PropertyId::Gain),
        vec![0, 59]
    );
    assert_keyframes_are_inside_their_clips(&track);
}

#[test]
fn cutting_a_clip_that_is_not_animated_leaves_both_halves_not_animated() {
    let mut track = track_of(vec![(0, video(0, 100, 1))]);
    let outcome = track.split_at(frame(40)).unwrap();
    for half in [outcome.first_half, outcome.second_half] {
        assert!(track.placement_of(half).unwrap().own_animation().is_empty());
    }
}

#[test]
fn what_is_animated_in_a_text_clip_or_a_picture_is_cut_with_it() {
    let mut track = track_of(vec![(0, text(90)), (0, picture(60))]);
    track
        .set_clip_keyframe(clip_id(0), PropertyId::Rotation, 0, db(0.0))
        .unwrap();
    track
        .set_clip_keyframe(clip_id(0), PropertyId::Rotation, 89, db(90.0))
        .unwrap();
    track
        .set_clip_keyframe(clip_id(1), PropertyId::Size, 10, PropertyValue::Pair(0.5, 0.5))
        .unwrap();
    let text_halves = track.split_at(frame(30)).unwrap();
    let picture_halves = track.split_at(frame(100)).unwrap();
    assert!(
        track
            .placement_of(text_halves.second_half)
            .unwrap()
            .own_animation()
            .is_animated(PropertyId::Rotation)
    );
    for half in [picture_halves.first_half, picture_halves.second_half] {
        assert!(
            track
                .placement_of(half)
                .unwrap()
                .own_animation()
                .is_animated(PropertyId::Size)
        );
    }
    assert_keyframes_are_inside_their_clips(&track);
}

// --- changing how long a clip lasts ---

#[test]
fn a_clip_that_is_sped_up_has_its_keyframes_as_far_through_it_as_they_were() {
    let mut track = track_of(vec![(0, video(0, 100, 1))]);
    track
        .set_clip_keyframe(clip_id(0), PropertyId::Gain, 40, db(-4.0))
        .unwrap();
    track
        .set_clip_keyframe(clip_id(0), PropertyId::Gain, 80, db(-8.0))
        .unwrap();

    track.set_clip_speed(clip_id(0), speed(2)).unwrap();

    assert_eq!(spans(&track), vec![(0, 50)]);
    assert_eq!(keyframe_frames(&track, clip_id(0), PropertyId::Gain), vec![20, 40]);
    assert_eq!(
        gain_of(&track, clip_id(0), 30),
        -6.0,
        "half way between them, as it was"
    );
    assert_keyframes_are_inside_their_clips(&track);

    track.set_clip_speed(clip_id(0), speed(1)).unwrap();
    assert_eq!(
        keyframe_frames(&track, clip_id(0), PropertyId::Gain),
        vec![40, 80],
        "and back"
    );
}

#[test]
fn the_speed_of_a_track_moves_the_keyframes_of_its_clips_and_leaves_the_tracks_own_where_they_are_in_time() {
    let mut track = track_of(vec![(0, video(0, 100, 1)), (0, text(40))]);
    track
        .set_clip_keyframe(clip_id(0), PropertyId::Gain, 50, db(-1.0))
        .unwrap();
    track
        .set_clip_keyframe(clip_id(1), PropertyId::Rotation, 20, db(45.0))
        .unwrap();
    track.set_own_keyframe(PropertyId::Gain, frame(70), db(-6.0));

    track.set_speed(speed(2));

    assert_eq!(
        keyframe_frames(&track, clip_id(0), PropertyId::Gain),
        vec![25],
        "the video is half as long"
    );
    assert_eq!(
        keyframe_frames(&track, clip_id(1), PropertyId::Rotation),
        vec![20],
        "a text object lasts as long as it said whatever the speed"
    );
    assert_eq!(
        track
            .own_animation()
            .keyframes_of(PropertyId::Gain)
            .unwrap()
            .iter()
            .next()
            .unwrap()
            .frame,
        70
    );
    assert_keyframes_are_inside_their_clips(&track);
}

#[test]
fn a_clip_that_is_pushed_along_by_another_that_got_longer_keeps_its_keyframes_as_they_were() {
    let mut track = track_of(vec![(0, video(0, 100, 2)), (0, video(0, 100, 1))]);
    track
        .set_clip_keyframe(clip_id(1), PropertyId::Gain, 30, db(-2.0))
        .unwrap();
    track.set_clip_speed(clip_id(0), speed(1)).unwrap();
    assert_eq!(spans(&track), vec![(0, 100), (100, 200)]);
    assert_eq!(keyframe_frames(&track, clip_id(1), PropertyId::Gain), vec![30]);
}

#[test]
fn a_text_object_that_is_made_shorter_brings_in_the_keyframes_it_no_longer_has_a_frame_for_with_their_values() {
    let mut track = track_of(vec![(0, text(100))]);
    track
        .set_clip_keyframe(clip_id(0), PropertyId::Rotation, 0, db(0.0))
        .unwrap();
    track
        .set_clip_keyframe(clip_id(0), PropertyId::Rotation, 80, db(80.0))
        .unwrap();
    assert_eq!(track.set_clip_duration(clip_id(0), 50), Ok(50));
    assert_eq!(keyframe_frames(&track, clip_id(0), PropertyId::Rotation), vec![0, 49]);
    assert_eq!(
        track
            .placement_of(clip_id(0))
            .unwrap()
            .own_animation()
            .value_at(PropertyId::Rotation, 49),
        Some(db(80.0)),
        "the keyframe that was at 80 is at its last frame, and has the value it had"
    );
    // Made longer again, it has what it has: the keyframes do not go back to where they were.
    assert_eq!(track.set_clip_duration(clip_id(0), 100), Ok(100));
    assert_eq!(keyframe_frames(&track, clip_id(0), PropertyId::Rotation), vec![0, 49]);
    assert_keyframes_are_inside_their_clips(&track);
}

#[test]
fn a_text_object_made_shorter_loses_keyframes_only_when_it_has_more_of_them_than_frames() {
    let mut track = track_of(vec![(0, text(100))]);
    for frame in [0, 20, 40, 60, 80] {
        track
            .set_clip_keyframe(clip_id(0), PropertyId::Rotation, frame, db(frame as f32))
            .unwrap();
    }
    assert_eq!(track.set_clip_duration(clip_id(0), 6), Ok(6));
    assert_eq!(
        keyframe_frames(&track, clip_id(0), PropertyId::Rotation),
        vec![0, 2, 3, 4, 5],
        "five keyframes in six frames: none is lost, and the four that were cut off are against the end"
    );
    assert_eq!(track.set_clip_duration(clip_id(0), 3), Ok(3));
    assert_eq!(
        keyframe_frames(&track, clip_id(0), PropertyId::Rotation),
        vec![0, 1, 2],
        "five in three: the first three are what is left"
    );
    assert_keyframes_are_inside_their_clips(&track);
}

// --- deleting, moving, counting again ---

#[test]
fn deleting_a_clip_takes_its_keyframes_and_leaves_those_of_the_others() {
    let mut track = track_of(vec![(0, video(0, 100, 1)), (0, video(0, 50, 1))]);
    track
        .set_clip_keyframe(clip_id(0), PropertyId::Gain, 10, db(1.0))
        .unwrap();
    track
        .set_clip_keyframe(clip_id(1), PropertyId::Gain, 20, db(2.0))
        .unwrap();
    track.delete_clip(clip_id(0)).unwrap();
    assert_eq!(keyframe_frames(&track, clip_id(1), PropertyId::Gain), vec![20]);
    assert_eq!(track.entries().len(), 1);
}

#[test]
fn clips_moved_along_a_track_take_their_keyframes_with_them() {
    let mut track = track_of(vec![(0, video(0, 100, 1)), (0, video(0, 50, 1))]);
    track
        .set_clip_keyframe(clip_id(0), PropertyId::Gain, 10, db(1.0))
        .unwrap();
    track
        .set_clip_keyframe(clip_id(1), PropertyId::Gain, 20, db(2.0))
        .unwrap();
    track.move_clips(&[(clip_id(1), 0), (clip_id(0), 60)]).unwrap();
    assert_eq!(ids(&track), vec![clip_id(1), clip_id(0)], "they changed places");
    assert_eq!(keyframe_frames(&track, clip_id(1), PropertyId::Gain), vec![20]);
    assert_eq!(keyframe_frames(&track, clip_id(0), PropertyId::Gain), vec![10]);
}

fn ids(track: &Track) -> Vec<ClipId> {
    track.entries().iter().map(TrackEntry::id).collect()
}

#[test]
fn a_clip_arranged_on_a_track_that_plays_at_another_speed_has_its_keyframes_stretched_to_what_it_lasts_there() {
    let mut fast = Track::new(TrackId::new(2), "fast").with_speed(speed(2));
    fast.arrange_clips(vec![ClipToArrange {
        id: clip_id(7),
        clip: video(0, 100, 1),
        drawer: a_drawer_with_the_loudness_of(clip_id(7), &[(60, -6.0)]),
        effects: Effects::none(),
        start: 0,
        frames_it_lasted_before: 100,
    }])
    .unwrap();
    assert_eq!(spans(&fast), vec![(0, 50)], "half as long on a track twice as fast");
    assert_eq!(keyframe_frames(&fast, clip_id(7), PropertyId::Gain), vec![30]);

    // On a track at the same speed it lasts the same and its keyframes are where they were.
    let mut same = Track::new(TrackId::new(3), "same");
    same.arrange_clips(vec![ClipToArrange {
        id: clip_id(7),
        clip: video(0, 100, 1),
        drawer: fast.entries()[0]
            .drawer()
            .with_each_rack_changed(|rack| rack.rescaled(50, 100)),
        effects: Effects::none(),
        start: 5,
        frames_it_lasted_before: 100,
    }])
    .unwrap();
    assert_eq!(keyframe_frames(&same, clip_id(7), PropertyId::Gain), vec![60]);
}

#[test]
fn a_track_counted_again_at_another_frame_rate_has_its_keyframes_counted_again_too() {
    let mut track = track_of(vec![(10, video(0, 90, 1)), (0, text(30))]);
    track
        .set_clip_keyframe(clip_id(0), PropertyId::Gain, 45, db(-3.0))
        .unwrap();
    track
        .set_clip_keyframe(clip_id(1), PropertyId::Rotation, 15, db(10.0))
        .unwrap();
    track.set_own_keyframe(PropertyId::Gain, frame(60), db(-9.0));

    let at_60 = track.retimed(FrameRate::new(30, 1).unwrap(), FrameRate::new(60, 1).unwrap());

    assert_eq!(spans(&at_60), vec![(20, 200), (200, 260)]);
    assert_eq!(
        keyframe_frames(&at_60, clip_id(0), PropertyId::Gain),
        vec![90],
        "as far through the clip as it was"
    );
    assert_eq!(keyframe_frames(&at_60, clip_id(1), PropertyId::Rotation), vec![30]);
    assert_eq!(
        at_60
            .own_animation()
            .keyframes_of(PropertyId::Gain)
            .unwrap()
            .iter()
            .next()
            .unwrap()
            .frame,
        120,
        "the track's own: at the same moment of time, which is twice the frame"
    );
    assert_keyframes_are_inside_their_clips(&at_60);
}

// --- no keyframe is lost while there is a frame for it ---

/// A track with a video of 100 frames (clip 0) whose loudness has a keyframe at each of `frames`, the values 0, 1, 2 and so on.
fn track_with_a_video_animated_at(frames: &[u64]) -> Track {
    let mut track = track_of(vec![(0, video(0, 100, 1))]);
    for (number, at) in frames.iter().enumerate() {
        track
            .set_clip_keyframe(clip_id(0), PropertyId::Gain, *at, db(number as f32))
            .unwrap();
    }
    track
}

/// The values of the loudness keyframes of clip 0, in the order they are in.
fn values_of_the_keyframes_of_the_first_clip(track: &Track) -> Vec<f32> {
    track
        .placement_of(clip_id(0))
        .unwrap()
        .own_animation()
        .keyframes_of(PropertyId::Gain)
        .map(|keyframes| {
            keyframes
                .iter()
                .map(|keyframe| match keyframe.value {
                    PropertyValue::Number(decibels) => decibels,
                    ref other => panic!("not a number: {other:?}"),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The bug of this round, for a clip that is sped up: keyframes that were close together landed on one frame, and all but one of
/// them were lost, though the clip had room for every one.
#[test]
fn a_clip_that_is_sped_up_keeps_keyframes_that_are_close_together_each_at_a_frame_of_its_own() {
    let mut track = track_with_a_video_animated_at(&[40, 41, 42, 43, 44]);

    track.set_clip_speed(clip_id(0), speed(4)).unwrap();

    assert_eq!(spans(&track), vec![(0, 25)]);
    assert_eq!(
        keyframe_frames(&track, clip_id(0), PropertyId::Gain),
        vec![9, 10, 11, 12, 13],
        "round the frame they were all at (10 or 11 of the 25)"
    );
    assert_eq!(
        values_of_the_keyframes_of_the_first_clip(&track),
        vec![0.0, 1.0, 2.0, 3.0, 4.0]
    );
    assert_keyframes_are_inside_their_clips(&track);
}

#[test]
fn a_track_that_is_made_faster_keeps_the_keyframes_of_its_clips_that_are_close_together_each_at_a_frame_of_its_own() {
    let mut track = track_with_a_video_animated_at(&[40, 41, 42, 43, 44]);

    track.set_speed(speed(4));

    assert_eq!(spans(&track), vec![(0, 25)]);
    assert_eq!(
        keyframe_frames(&track, clip_id(0), PropertyId::Gain),
        vec![9, 10, 11, 12, 13]
    );
    assert_eq!(
        values_of_the_keyframes_of_the_first_clip(&track),
        vec![0.0, 1.0, 2.0, 3.0, 4.0]
    );
}

#[test]
fn a_clip_arranged_on_a_track_that_plays_faster_keeps_keyframes_that_are_close_together_each_at_a_frame_of_its_own() {
    let mut fast = Track::new(TrackId::new(2), "fast").with_speed(speed(2));
    fast.arrange_clips(vec![ClipToArrange {
        id: clip_id(7),
        clip: video(0, 100, 1),
        drawer: a_drawer_with_the_loudness_of(clip_id(7), &[(40, 0.0), (41, 1.0), (42, 2.0), (43, 3.0), (44, 4.0)]),
        effects: Effects::none(),
        start: 0,
        frames_it_lasted_before: 100,
    }])
    .unwrap();

    assert_eq!(spans(&fast), vec![(0, 50)]);
    assert_eq!(
        keyframe_frames(&fast, clip_id(7), PropertyId::Gain),
        vec![19, 20, 21, 22, 23]
    );
}

#[test]
fn a_track_counted_again_at_a_lower_frame_rate_keeps_keyframes_that_are_close_together_each_at_a_frame_of_its_own() {
    let mut track = track_of(vec![(0, video(0, 100, 1))]);
    for (number, at) in [50, 51, 52, 53, 54].into_iter().enumerate() {
        track
            .set_clip_keyframe(clip_id(0), PropertyId::Gain, at, db(number as f32))
            .unwrap();
    }
    // The keyframes of the track itself are at the frames of the timeline: four in a row, which a lower rate puts on three frames.
    for (number, at) in [100, 101, 102, 103].into_iter().enumerate() {
        track.set_own_keyframe(PropertyId::Gain, frame(at), db(number as f32));
    }

    let at_24 = track.retimed(FrameRate::new(30, 1).unwrap(), FrameRate::new(24, 1).unwrap());

    assert_eq!(spans(&at_24), vec![(0, 80)]);
    assert_eq!(
        keyframe_frames(&at_24, clip_id(0), PropertyId::Gain),
        vec![40, 41, 42, 43, 44],
        "the one that would have been on top of the one before it is put after it"
    );
    assert_eq!(
        values_of_the_keyframes_of_the_first_clip(&at_24),
        vec![0.0, 1.0, 2.0, 3.0, 4.0]
    );
    let own: Vec<u64> = at_24
        .own_animation()
        .keyframes_of(PropertyId::Gain)
        .unwrap()
        .iter()
        .map(|keyframe| keyframe.frame)
        .collect();
    assert_eq!(own, vec![80, 81, 82, 83], "the four of the track are still four");
}

#[test]
fn the_keyframes_of_the_track_itself_are_counted_again_however_far_along_the_timeline_they_are() {
    let mut track = track_of(vec![(0, video(0, 100, 1))]);
    for at in [3000, 3001, 3002, 3003, 30_000] {
        track.set_own_keyframe(PropertyId::Gain, frame(at), db(at as f32));
    }

    let at_24 = track.retimed(FrameRate::new(30, 1).unwrap(), FrameRate::new(24, 1).unwrap());

    let own: Vec<u64> = at_24
        .own_animation()
        .keyframes_of(PropertyId::Gain)
        .unwrap()
        .iter()
        .map(|keyframe| keyframe.frame)
        .collect();
    assert_eq!(
        own,
        vec![2400, 2401, 2402, 2403, 24_000],
        "the four in a row are four in a row (3000 to 3003 are 2400 to 2402.4), and the track has no end for the last to be held to"
    );
}

// --- whatever is done ---

#[test]
fn whatever_cuts_and_speeds_are_done_every_keyframe_stays_inside_its_clip_and_every_clip_stays_animated() {
    let mut random = DeterministicRandomNumbers::with_seed(66);
    for round in 0..200 {
        let mut track = track_of(vec![
            (0, video(0, 60 + random.next_below(100), 1)),
            (5, text(20 + random.next_below(60))),
        ]);
        for clip in [clip_id(0), clip_id(1)] {
            let length = track.placement_of(clip).unwrap().timeline_frame_count();
            let property = if clip == clip_id(0) {
                PropertyId::Gain
            } else {
                PropertyId::Rotation
            };
            for _ in 0..1 + random.next_below(4) {
                let _ = track.set_clip_keyframe(
                    clip,
                    property,
                    random.next_below(length),
                    db(random.next_below(40) as f32 - 20.0),
                );
            }
        }
        for step in 0..8 {
            let total = track.total_frame_count().max(1);
            match random.next_below(4) {
                0 => {
                    let _ = track.split_at(frame(random.next_below(total)));
                }
                1 => {
                    let _ = track.set_clip_speed(clip_id(0), speed(1 + random.next_below(5) as u32));
                }
                2 => track.set_speed(speed(1 + random.next_below(3) as u32)),
                _ => {
                    let target = track.entries().len();
                    if target > 1 {
                        let victim = track.entries()[random.next_below(target as u64) as usize].id();
                        let _ = track.set_clip_duration(victim, 1 + random.next_below(40));
                    }
                }
            }
            assert_keyframes_are_inside_their_clips(&track);
            // What was animated stays animated, and nothing is animated that was not.
            for placement in track.placements() {
                let kinds_animated = placement.own_animation().properties().count();
                assert!(
                    kinds_animated <= 1,
                    "round {round} step {step}: {:?}",
                    placement.own_animation()
                );
                assert!(
                    placement
                        .own_animation()
                        .properties()
                        .all(|(_, keyframes)| !keyframes.is_empty()),
                    "round {round}"
                );
            }
        }
    }
}
