//! Tests of animating the properties of clips and tracks through the project.

use super::tests::{audio_source, first_clip_location, frame, project_with_a_video};
use super::*;

fn clip_target(project: &Project) -> AnimationTarget {
    AnimationTarget::Clip(first_clip_location(project, 0))
}

fn gain_of(project: &Project, target: AnimationTarget, at: u64) -> f32 {
    match project.property_value_at(target, PropertyId::Gain, frame(at)) {
        Some(PropertyValue::Number(decibels)) => decibels,
        other => panic!("not a gain: {other:?}"),
    }
}

fn a_project_with_a_text_object() -> (Project, ClipId, TrackId) {
    let mut project = Project::empty();
    let track = project.tracks()[0].id();
    let (clip, _) = project
        .insert_text(
            track,
            frame(100),
            TextObject {
                text: "Hello".to_owned(),
                ..TextObject::default()
            },
            50,
        )
        .unwrap();
    (project, clip, track)
}

// --- what a clip is at a frame ---

#[test]
fn a_clip_is_where_it_says_it_is_until_it_has_keyframes_for_where_it_is() {
    let mut project = project_with_a_video(300);
    let clip = first_clip_location(&project, 0);
    let target = AnimationTarget::Clip(clip);
    let fitted = project.placement_at(clip, frame(10)).unwrap();
    assert_eq!(
        fitted,
        project.placement_at(clip, frame(250)).unwrap(),
        "the same at every frame"
    );

    project.add_keyframe(target, PropertyId::Rotation, frame(0)).unwrap();
    project
        .set_property_at(target, PropertyId::Rotation, PropertyValue::Number(0.0), frame(0))
        .unwrap();
    project.add_keyframe(target, PropertyId::Rotation, frame(100)).unwrap();
    project
        .set_property_at(target, PropertyId::Rotation, PropertyValue::Number(90.0), frame(100))
        .unwrap();

    assert_eq!(project.placement_at(clip, frame(0)).unwrap().rotation.degrees(), 0.0);
    assert_eq!(project.placement_at(clip, frame(50)).unwrap().rotation.degrees(), 45.0);
    assert_eq!(project.placement_at(clip, frame(100)).unwrap().rotation.degrees(), 90.0);
    assert_eq!(
        project.placement_at(clip, frame(280)).unwrap().rotation.degrees(),
        90.0,
        "it stays as it got to"
    );
    assert_eq!(
        project.placement_at(clip, frame(50)).unwrap().area,
        fitted.area,
        "the rest is as it was"
    );
}

#[test]
fn a_text_object_is_what_it_says_until_it_has_keyframes_for_how_it_looks() {
    let (mut project, text, _) = a_project_with_a_text_object();
    let target = AnimationTarget::Clip(text);
    for (at, words) in [(100, "First"), (125, "Second")] {
        project.add_keyframe(target, PropertyId::Text, frame(at)).unwrap();
        project
            .set_property_at(
                target,
                PropertyId::Text,
                PropertyValue::Words(words.to_owned()),
                frame(at),
            )
            .unwrap();
    }

    let said_at = |at: u64| project.text_object_at(text, frame(at)).unwrap().text;
    assert_eq!(said_at(100), "First");
    assert_eq!(said_at(124), "First");
    assert_eq!(said_at(125), "Second");
    assert_eq!(said_at(1000), "Second");
    assert_eq!(said_at(0), "First", "before the clip, it is as at its first frame");
}

#[test]
fn what_is_not_there_or_not_of_that_kind_has_no_place_and_no_text_object() {
    let project = project_with_a_video(300);
    let video = first_clip_location(&project, 0);
    assert!(project.placement_at(video, frame(0)).is_some());
    assert_eq!(
        project.text_object_at(video, frame(0)),
        None,
        "a video is not a text object"
    );
    assert_eq!(
        project.placement_at(ClipId::new(project.tracks()[0].id(), 99), frame(0)),
        None
    );

    let mut with_a_sound = project_with_a_video(300);
    with_a_sound.import_source(audio_source(2, 5), frame(0)).unwrap();
    let sound = with_a_sound.tracks()[0].entries()[0].id();
    assert_eq!(
        with_a_sound.placement_at(sound, frame(0)),
        None,
        "a sound is not on the picture"
    );
}

// --- what a property is ---

#[test]
fn a_property_that_is_not_animated_is_what_the_clip_says_at_every_frame() {
    let mut project = project_with_a_video(300);
    let clip = first_clip_location(&project, 0);
    project.set_clip_gain(clip, Gain::from_decibels(-6.0)).unwrap();
    for at in [0, 150, 299, 5000] {
        assert_eq!(gain_of(&project, clip_target(&project), at), -6.0, "at {at}");
    }
    assert_eq!(
        project.property_value_at(clip_target(&project), PropertyId::Muted, frame(3)),
        Some(PropertyValue::Flag(false))
    );
    assert!(!project.is_animated(clip_target(&project), PropertyId::Gain));
}

#[test]
fn something_that_does_not_have_a_property_has_no_value_for_it() {
    let (project, text, track) = a_project_with_a_text_object();
    assert_eq!(
        project.property_value_at(AnimationTarget::Clip(text), PropertyId::Gain, frame(100)),
        None,
        "text is silent"
    );
    assert_eq!(
        project.property_value_at(AnimationTarget::Track(track), PropertyId::Position, frame(0)),
        None
    );
    assert_eq!(
        project.property_value_at(
            AnimationTarget::Clip(ClipId::new(track, 99)),
            PropertyId::Gain,
            frame(0)
        ),
        None
    );
    let video = project_with_a_video(300);
    assert_eq!(project_value(&video, PropertyId::Text), None, "a video has no words");
    assert_eq!(project_value(&video, PropertyId::Colour), None);

    let mut with_a_sound = Project::empty();
    with_a_sound.import_source(audio_source(2, 10), frame(0)).unwrap();
    let sound = AnimationTarget::Clip(first_clip_location(&with_a_sound, 0));
    assert_eq!(
        with_a_sound.property_value_at(sound, PropertyId::Position, frame(0)),
        None,
        "a sound is not on the picture"
    );
    assert!(
        with_a_sound
            .property_value_at(sound, PropertyId::Gain, frame(0))
            .is_some()
    );
}

fn project_value(project: &Project, property: PropertyId) -> Option<PropertyValue> {
    project.property_value_at(clip_target(project), property, frame(0))
}

#[test]
fn where_a_video_is_on_the_picture_is_where_it_is_fitted_until_it_has_been_put_somewhere() {
    let mut project = project_with_a_video(300);
    let clip = first_clip_location(&project, 0);
    let target = AnimationTarget::Clip(clip);
    assert_eq!(
        project.property_value_at(target, PropertyId::Position, frame(0)),
        Some(PropertyValue::Pair(0.5, 0.5)),
        "a video of the shape of the picture fills it"
    );
    assert_eq!(
        project.property_value_at(target, PropertyId::Size, frame(0)),
        Some(PropertyValue::Pair(1.0, 1.0))
    );
    assert_eq!(
        project.property_value_at(target, PropertyId::Rotation, frame(0)),
        Some(PropertyValue::Number(0.0))
    );

    project
        .set_property_at(target, PropertyId::Rotation, PropertyValue::Number(30.0), frame(10))
        .unwrap();
    let placed = project
        .placement_of(clip)
        .unwrap()
        .clip
        .placement()
        .expect("it was put somewhere");
    assert_eq!(placed.rotation.degrees(), 30.0);
    assert_eq!(
        placed.area,
        CanvasArea::WHOLE_PICTURE,
        "where it was fitted, which is all of the picture"
    );
    assert!(
        !project.is_animated(target, PropertyId::Rotation),
        "no keyframe: it is the clip's own"
    );
}

// --- keyframes ---

#[test]
fn a_keyframe_makes_the_property_animated_with_the_value_it_has_there() {
    let mut project = project_with_a_video(300);
    let target = clip_target(&project);
    project
        .set_clip_gain(first_clip_location(&project, 0), Gain::from_decibels(-4.0))
        .unwrap();

    project.add_keyframe(target, PropertyId::Gain, frame(40)).unwrap();

    assert!(project.is_animated(target, PropertyId::Gain));
    assert!(project.has_keyframe_at(target, PropertyId::Gain, frame(40)));
    assert!(!project.has_keyframe_at(target, PropertyId::Gain, frame(41)));
    assert!(!project.is_animated(target, PropertyId::Muted), "only the one property");
    assert_eq!(gain_of(&project, target, 0), -4.0);
    assert_eq!(gain_of(&project, target, 299), -4.0);
}

#[test]
fn changing_an_animated_property_at_a_frame_puts_a_keyframe_there_and_the_property_changes_evenly_between() {
    let mut project = project_with_a_video(300);
    let target = clip_target(&project);
    project.add_keyframe(target, PropertyId::Gain, frame(0)).unwrap();
    project.add_keyframe(target, PropertyId::Gain, frame(100)).unwrap();

    project
        .set_property_at(target, PropertyId::Gain, PropertyValue::Number(-10.0), frame(100))
        .unwrap();

    assert_eq!(gain_of(&project, target, 100), -10.0);
    assert_eq!(gain_of(&project, target, 50), -5.0, "half way");
    assert_eq!(
        gain_of(&project, target, 250),
        -10.0,
        "after the last keyframe it is what that says"
    );
    let clip = project.placement_of(first_clip_location(&project, 0)).unwrap();
    assert_eq!(
        clip.clip.as_media().unwrap().gain(),
        Gain::UNCHANGED,
        "what the clip itself says is not touched"
    );
    assert_eq!(clip.own_animation().keyframes_of(PropertyId::Gain).unwrap().len(), 2);
}

#[test]
fn changing_a_property_that_is_not_animated_changes_the_property_itself() {
    let mut project = project_with_a_video(300);
    let target = clip_target(&project);
    project
        .set_property_at(target, PropertyId::Gain, PropertyValue::Number(-12.0), frame(77))
        .unwrap();
    assert_eq!(gain_of(&project, target, 0), -12.0);
    assert!(!project.is_animated(target, PropertyId::Gain));
    project
        .set_property_at(target, PropertyId::Muted, PropertyValue::Flag(true), frame(3))
        .unwrap();
    assert_eq!(
        project.property_value_at(target, PropertyId::Muted, frame(200)),
        Some(PropertyValue::Flag(true))
    );
}

#[test]
fn taking_away_the_last_keyframe_leaves_the_property_what_that_keyframe_said_and_not_animated() {
    let mut project = project_with_a_video(300);
    let target = clip_target(&project);
    project.add_keyframe(target, PropertyId::Gain, frame(10)).unwrap();
    project
        .set_property_at(target, PropertyId::Gain, PropertyValue::Number(-7.0), frame(10))
        .unwrap();
    assert_eq!(gain_of(&project, target, 200), -7.0);

    project.remove_keyframe(target, PropertyId::Gain, frame(10)).unwrap();

    assert!(!project.is_animated(target, PropertyId::Gain));
    assert_eq!(gain_of(&project, target, 200), -7.0, "it did not jump");
    let clip = project.placement_of(first_clip_location(&project, 0)).unwrap();
    assert_eq!(
        clip.clip.as_media().unwrap().gain(),
        Gain::from_decibels(-7.0),
        "the clip says it now"
    );
}

#[test]
fn taking_away_one_of_several_keyframes_leaves_the_others_and_the_property_animated() {
    let mut project = project_with_a_video(300);
    let target = clip_target(&project);
    for at in [10, 20, 30] {
        project.add_keyframe(target, PropertyId::Gain, frame(at)).unwrap();
    }
    project.remove_keyframe(target, PropertyId::Gain, frame(20)).unwrap();
    assert!(project.is_animated(target, PropertyId::Gain));
    assert!(project.has_keyframe_at(target, PropertyId::Gain, frame(10)));
    assert!(!project.has_keyframe_at(target, PropertyId::Gain, frame(20)));
    assert!(project.has_keyframe_at(target, PropertyId::Gain, frame(30)));
    // Taking away what is not there is nothing.
    let before = project.clone();
    project.remove_keyframe(target, PropertyId::Gain, frame(21)).unwrap();
    assert_eq!(project, before);
}

#[test]
fn the_keyframes_before_and_after_a_frame_are_found_however_far_off_they_are() {
    let mut project = project_with_a_video(300);
    let target = clip_target(&project);
    for at in [50, 150] {
        project.add_keyframe(target, PropertyId::Gain, frame(at)).unwrap();
    }
    let before = |at| project.keyframe_before(target, PropertyId::Gain, frame(at));
    let after = |at| project.keyframe_after(target, PropertyId::Gain, frame(at));
    assert_eq!((before(0), after(0)), (None, Some(frame(50))));
    assert_eq!((before(50), after(50)), (None, Some(frame(150))));
    assert_eq!((before(100), after(100)), (Some(frame(50)), Some(frame(150))));
    assert_eq!((before(150), after(150)), (Some(frame(50)), None));
    assert_eq!((before(299), after(299)), (Some(frame(150)), None));
    assert_eq!(
        project.keyframe_before(target, PropertyId::Muted, frame(100)),
        None,
        "a property that is not animated has none"
    );
}

#[test]
fn the_keyframes_of_a_clip_are_counted_from_its_first_frame_wherever_it_starts_on_the_timeline() {
    let (mut project, text, _) = a_project_with_a_text_object(); // frames 100 to 150
    let target = AnimationTarget::Clip(text);
    project.add_keyframe(target, PropertyId::Rotation, frame(110)).unwrap();
    project.add_keyframe(target, PropertyId::Rotation, frame(140)).unwrap();
    let placement = project.placement_of(text).unwrap();
    let in_the_clip: Vec<u64> = placement
        .own_animation()
        .keyframes_of(PropertyId::Rotation)
        .unwrap()
        .iter()
        .map(|keyframe| keyframe.frame)
        .collect();
    assert_eq!(in_the_clip, vec![10, 40]);
    // The questions are asked and answered in frames of the timeline.
    assert_eq!(
        project.keyframe_after(target, PropertyId::Rotation, frame(110)),
        Some(frame(140))
    );
    assert_eq!(
        project.keyframe_before(target, PropertyId::Rotation, frame(140)),
        Some(frame(110))
    );
    assert_eq!(
        project.keyframe_after(target, PropertyId::Rotation, frame(5)),
        Some(frame(110)),
        "from before the clip"
    );
    assert_eq!(project.keyframe_before(target, PropertyId::Rotation, frame(5)), None);
    assert_eq!(
        project.keyframe_before(target, PropertyId::Rotation, frame(900)),
        Some(frame(140)),
        "from after it"
    );
}

#[test]
fn a_keyframe_cannot_be_made_where_the_clip_is_not_and_taking_one_away_there_is_nothing() {
    let (mut project, text, _) = a_project_with_a_text_object(); // frames 100 to 150
    let target = AnimationTarget::Clip(text);
    let before = project.clone();
    assert!(project.add_keyframe(target, PropertyId::Rotation, frame(99)).is_err());
    assert!(
        project.add_keyframe(target, PropertyId::Rotation, frame(150)).is_err(),
        "the end is exclusive"
    );
    assert_eq!(
        project.set_property_at(target, PropertyId::Rotation, PropertyValue::Number(5.0), frame(10)),
        Ok(None),
        "a property that is not animated is changed wherever the playhead is"
    );
    assert_ne!(project, before);
    let mut animated = before.clone();
    animated.add_keyframe(target, PropertyId::Rotation, frame(110)).unwrap();
    let still = animated.clone();
    assert!(
        animated
            .set_property_at(target, PropertyId::Rotation, PropertyValue::Number(5.0), frame(10))
            .is_err(),
        "one that is animated has no frame to be changed at there"
    );
    assert_eq!(animated, still);
    assert_eq!(
        animated.remove_keyframe(target, PropertyId::Rotation, frame(10)),
        Ok(())
    );
    assert_eq!(animated, still);
}

#[test]
fn what_a_text_object_says_can_be_animated_and_changes_at_the_keyframe() {
    let (mut project, text, _) = a_project_with_a_text_object();
    let target = AnimationTarget::Clip(text);
    project.add_keyframe(target, PropertyId::Text, frame(100)).unwrap();
    project
        .set_property_at(
            target,
            PropertyId::Text,
            PropertyValue::Words("Goodbye".to_owned()),
            frame(120),
        )
        .unwrap();
    let words = |at| project.property_value_at(target, PropertyId::Text, frame(at));
    assert_eq!(words(100), Some(PropertyValue::Words("Hello".to_owned())));
    assert_eq!(
        words(119),
        Some(PropertyValue::Words("Hello".to_owned())),
        "it waits for the keyframe"
    );
    assert_eq!(words(120), Some(PropertyValue::Words("Goodbye".to_owned())));
    assert_eq!(
        project.placement_of(text).unwrap().clip.as_text().unwrap().object.text,
        "Hello",
        "the object itself says what it did"
    );
}

/// (It used to go the short way round: the angle that was given was changed into the turn that was nearest the one before it.
/// What is given is what it is now, so that there can be as many whole turns between two keyframes as are wanted.)
#[test]
fn a_turn_is_what_it_is_given_even_past_half_a_turn() {
    let (mut project, text, _) = a_project_with_a_text_object();
    let target = AnimationTarget::Clip(text);
    project
        .set_property_at(target, PropertyId::Rotation, PropertyValue::Number(170.0), frame(100))
        .unwrap();
    project.add_keyframe(target, PropertyId::Rotation, frame(100)).unwrap();
    project
        .set_property_at(target, PropertyId::Rotation, PropertyValue::Number(-170.0), frame(140))
        .unwrap();
    let at = |frame_number| project.property_value_at(target, PropertyId::Rotation, frame(frame_number));
    assert_eq!(at(140), Some(PropertyValue::Number(-170.0)), "kept as it was given");
    let Some(PropertyValue::Number(half_way)) = at(120) else {
        panic!()
    };
    assert!(
        half_way.abs() < 0.01,
        "half way it is through 0, 170 degrees back and 170 more: {half_way}"
    );
}

// --- tracks ---

#[test]
fn a_track_has_keyframes_for_its_loudness_and_its_mute_switch_in_frames_of_the_timeline() {
    let mut project = project_with_a_video(300);
    let track = project.tracks()[0].id();
    let target = AnimationTarget::Track(track);
    project.add_keyframe(target, PropertyId::Gain, frame(200)).unwrap();
    project
        .set_property_at(target, PropertyId::Gain, PropertyValue::Number(-9.0), frame(300))
        .unwrap();
    assert_eq!(
        project.track(track).unwrap().gain(),
        Gain::UNCHANGED,
        "the track's own gain is not touched"
    );
    assert_eq!(gain_of(&project, target, 250), -4.5);
    assert_eq!(
        project.keyframe_before(target, PropertyId::Gain, frame(250)),
        Some(frame(200))
    );
    assert_eq!(
        project.keyframe_after(target, PropertyId::Gain, frame(250)),
        Some(frame(300))
    );
    project.add_keyframe(target, PropertyId::Muted, frame(0)).unwrap();
    assert!(project.is_animated(target, PropertyId::Muted));
}

#[test]
fn a_track_has_no_keyframes_for_what_it_does_not_have() {
    let mut project = project_with_a_video(300);
    let target = AnimationTarget::Track(project.tracks()[0].id());
    let before = project.clone();
    assert!(project.add_keyframe(target, PropertyId::Position, frame(0)).is_err());
    assert!(
        project
            .set_property_at(target, PropertyId::Gain, PropertyValue::Flag(true), frame(0))
            .is_err()
    );
    assert_eq!(project, before);
}

#[test]
fn the_properties_of_a_clip_that_is_not_there_are_an_error() {
    let mut project = project_with_a_video(300);
    let unknown = AnimationTarget::Clip(ClipId::new(project.tracks()[0].id(), 77));
    assert!(project.add_keyframe(unknown, PropertyId::Gain, frame(0)).is_err());
    assert!(project.remove_keyframe(unknown, PropertyId::Gain, frame(0)).is_err());
    assert!(
        project
            .set_property_at(unknown, PropertyId::Gain, PropertyValue::Number(1.0), frame(0))
            .is_err()
    );
}

#[test]
fn what_a_clip_has_animated_goes_with_it_to_another_track() {
    let mut project = project_with_a_video(300);
    let clip = first_clip_location(&project, 0);
    let target = AnimationTarget::Clip(clip);
    project.add_keyframe(target, PropertyId::Gain, frame(20)).unwrap();
    project
        .set_property_at(target, PropertyId::Gain, PropertyValue::Number(-9.0), frame(80))
        .unwrap();
    let in_front = project.add_track_in_front();
    let fast = project.tracks()[1].id();
    project.set_track_speed(fast, SpeedFactor::new(2).unwrap()).unwrap();

    project
        .move_clips(&[ClipMove {
            clip,
            to_track: in_front,
            start: frame(30),
        }])
        .unwrap();
    assert_eq!(
        gain_of(&project, target, 30 + 80),
        -9.0,
        "on the front track, which plays at normal speed"
    );
    assert!(project.has_keyframe_at(target, PropertyId::Gain, frame(30 + 20)));

    project
        .move_clips(&[ClipMove {
            clip,
            to_track: fast,
            start: frame(30),
        }])
        .unwrap();
    // It lasts half as long on the track that plays twice as fast, and its keyframes are as far through it.
    assert!(project.has_keyframe_at(target, PropertyId::Gain, frame(30 + 10)));
    assert!(project.has_keyframe_at(target, PropertyId::Gain, frame(30 + 40)));
}

#[test]
fn the_keyframes_of_a_clip_are_part_of_what_makes_two_projects_equal() {
    let mut animated = project_with_a_video(300);
    let plain = animated.clone();
    assert_eq!(animated, plain);
    animated
        .add_keyframe(clip_target(&animated), PropertyId::Gain, frame(10))
        .unwrap();
    assert_ne!(
        animated, plain,
        "or an edit of keyframes would not be an edit that can be undone"
    );
}

// --- turning more than once ---

#[test]
fn keyframes_whole_turns_apart_make_a_clip_go_round_that_many_times_between_them() {
    let mut project = project_with_a_video(300);
    let clip = first_clip_location(&project, 0);
    let target = AnimationTarget::Clip(clip);
    for (at, degrees) in [(0, 0.0), (100, 720.0)] {
        project.add_keyframe(target, PropertyId::Rotation, frame(at)).unwrap();
        project
            .set_property_at(target, PropertyId::Rotation, PropertyValue::Number(degrees), frame(at))
            .unwrap();
    }

    for (at, degrees) in [
        (0, 0.0),
        (25, 180.0),
        (50, 360.0),
        (75, 540.0),
        (100, 720.0),
        (250, 720.0),
    ] {
        assert_eq!(
            project.placement_at(clip, frame(at)).unwrap().rotation.degrees(),
            degrees,
            "at frame {at}"
        );
    }
}

#[test]
fn what_is_typed_for_the_turn_of_a_keyframe_is_the_turn_it_has_and_does_not_go_the_short_way_round() {
    let mut project = project_with_a_video(300);
    let clip = first_clip_location(&project, 0);
    let target = AnimationTarget::Clip(clip);
    project.add_keyframe(target, PropertyId::Rotation, frame(0)).unwrap();
    project
        .set_property_at(target, PropertyId::Rotation, PropertyValue::Number(170.0), frame(0))
        .unwrap();

    // From 170 degrees to -170 is 340 degrees back, which is what was asked for, and not 20 on.
    project.add_keyframe(target, PropertyId::Rotation, frame(60)).unwrap();
    project
        .set_property_at(target, PropertyId::Rotation, PropertyValue::Number(-170.0), frame(60))
        .unwrap();

    assert_eq!(
        project.property_value_at(target, PropertyId::Rotation, frame(60)),
        Some(PropertyValue::Number(-170.0))
    );
    assert_eq!(
        project.property_value_at(target, PropertyId::Rotation, frame(30)),
        Some(PropertyValue::Number(0.0)),
        "half way between 170 and -170 the long way is 0"
    );
}
