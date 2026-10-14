//! Tests of copying a clip beside itself, and a track in front of or behind itself, with what is in their drawers.

use super::racks_tests::{
    a_big_video_and_a_small_text, assert_the_racks_are_sound, db, keyframe_frames, names_of_the_racks_in,
    put_a_rack_in, where_the_video_has_keyframes,
};
use super::tests::frame;
use super::*;

// --- copying clips ---

#[test]
fn a_clip_is_copied_to_its_right_with_a_copy_of_its_drawer_in_which_what_animated_the_clip_animates_the_copy() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    let owner = AnimationTarget::Clip(text);
    put_a_rack_in(
        &mut project,
        owner,
        owner,
        PropertyId::Rotation,
        &[(5, PropertyValue::Number(10.0))],
    );
    put_a_rack_in(
        &mut project,
        owner,
        big,
        PropertyId::Gain,
        &[(10, db(-20.0)), (40, db(0.0))],
    );

    let copy = project.duplicate_clip(text, ClipSide::Right).unwrap();

    let placement = project.placement_of(copy).unwrap();
    assert_eq!(
        placement.first_timeline_frame,
        frame(160),
        "right after the text, which ends at 160"
    );
    assert_eq!(placement.timeline_frame_count(), 60);
    assert_eq!(
        placement.clip,
        project.placement_of(text).unwrap().clip,
        "it is the same thing"
    );
    assert_ne!(copy, text);
    let racks = placement.drawer.racks();
    assert_eq!(racks.len(), 2);
    assert_eq!(racks[0].property(), PropertyId::Rotation);
    assert_eq!(
        racks[0].target(),
        AnimationTarget::Clip(copy),
        "what animated the text animates its copy"
    );
    assert_eq!(racks[1].target(), big, "what animated the video still does");
    assert_eq!(
        where_the_video_has_keyframes(&project, video, PropertyId::Gain),
        vec![110, 140, 170, 200],
        "so the video has the keyframes of the copy too, 60 frames on"
    );
    let originals = names_of_the_racks_in(&project, owner);
    assert!(
        racks.iter().all(|rack| !originals.contains(&rack.id())),
        "the racks of the copy are its own"
    );
    assert_eq!(originals.len(), 2, "the text has its own, as it had");
    assert_eq!(
        project.property_value_at(AnimationTarget::Clip(copy), PropertyId::Rotation, frame(170)),
        Some(PropertyValue::Number(10.0))
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_clip_is_copied_to_its_left_and_not_to_before_the_start_of_the_timeline() {
    let (mut project, _, text) = a_big_video_and_a_small_text();
    assert!(
        project.room_for_a_copy_of(text, ClipSide::Left),
        "it starts at frame 100 and lasts 60"
    );

    let copy = project.duplicate_clip(text, ClipSide::Left).unwrap();

    assert_eq!(project.placement_of(copy).unwrap().first_timeline_frame, frame(40));
    assert_eq!(project.placement_of(copy).unwrap().end_timeline_frame, frame(100));
    // The copy starts at 40: a copy of it before it would start at -20.
    assert!(!project.room_for_a_copy_of(copy, ClipSide::Left));
    let before = project.clone();
    assert_eq!(
        project.duplicate_clip(copy, ClipSide::Left),
        Err(EditError::NoRoomForACopy { clip: copy })
    );
    assert_eq!(project, before);
}

#[test]
fn a_clip_is_not_copied_where_there_is_no_room_for_it_and_nothing_changes() {
    let (mut project, _, text) = a_big_video_and_a_small_text();
    let front = project.tracks()[0].id();
    project
        .insert_text(front, frame(160), TextObject::default(), 30)
        .unwrap();
    let before = project.clone();

    assert!(
        !project.room_for_a_copy_of(text, ClipSide::Right),
        "another text is right there"
    );
    assert_eq!(
        project.duplicate_clip(text, ClipSide::Right),
        Err(EditError::NoRoomForACopy { clip: text })
    );
    assert_eq!(project, before);

    let nobody = ClipId::new(TrackId::new(50), 0);
    assert!(!project.room_for_a_copy_of(nobody, ClipSide::Right));
    assert_eq!(
        project.duplicate_clip(nobody, ClipSide::Right),
        Err(EditError::Clip(ClipEditError::UnknownClip { clip: nobody }))
    );
    assert_eq!(project, before);
}

#[test]
fn a_video_with_nothing_after_it_is_copied_whole_to_its_right_and_has_no_room_on_its_left_at_the_start() {
    let (mut project, video, _) = a_big_video_and_a_small_text();
    // The video fills its track from frame 0 to 600.
    assert!(!project.room_for_a_copy_of(video, ClipSide::Left));
    assert!(project.room_for_a_copy_of(video, ClipSide::Right));

    let copy = project.duplicate_clip(video, ClipSide::Right).unwrap();

    let (original, copied) = (
        project.placement_of(video).unwrap(),
        project.placement_of(copy).unwrap(),
    );
    assert_eq!(copied.first_timeline_frame, frame(600));
    assert_eq!(copied.timeline_frame_count(), original.timeline_frame_count());
    assert_eq!(copied.clip, original.clip, "it shows the same part of the same file");
    assert_eq!(project.total_frame_count(), 1200);
}

#[test]
fn racks_kept_elsewhere_that_animate_a_clip_are_not_copied_with_it() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let big = AnimationTarget::Clip(video);
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(10, db(-20.0))],
    );
    // (The video is cut in two so that there is room to the right of the second half, which is the one that is copied.)
    let cut = project.split_at(frame(300), Some(&[video]));
    let second_half = cut[0].1.second_half;
    let keyframes_of_the_video_before = where_the_video_has_keyframes(&project, video, PropertyId::Gain);

    let copy = project.duplicate_clip(second_half, ClipSide::Right).unwrap();

    assert!(
        project
            .keyframes_of(AnimationTarget::Clip(copy), PropertyId::Gain)
            .is_empty(),
        "the keyframes in the drawer of the text are of the video, and the copy is another clip"
    );
    assert_eq!(
        where_the_video_has_keyframes(&project, video, PropertyId::Gain),
        keyframes_of_the_video_before
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_crate_is_copied_with_its_racks() {
    let (mut project, video, _) = a_big_video_and_a_small_text();
    let front = project.tracks()[0].id();
    let crate_id = project.insert_crate(front, frame(300), 100).unwrap();
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(crate_id),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(10, db(-20.0)), (30, db(-6.0))],
    );
    // (The crate lasts from its first keyframe to its last, so it is from frame 310 to frame 330 and not as long as it was made.)
    let lasting = |project: &Project, clip| {
        let placement = project.placement_of(clip).unwrap();
        (placement.first_timeline_frame, placement.timeline_frame_count())
    };
    assert_eq!(lasting(&project, crate_id), (frame(310), 21));

    let copy = project.duplicate_clip(crate_id, ClipSide::Right).unwrap();

    assert!(matches!(project.placement_of(copy).unwrap().clip, Clip::Crate(_)));
    assert_eq!(
        lasting(&project, copy),
        (frame(331), 21),
        "right after the crate, as long"
    );
    assert_eq!(
        where_the_video_has_keyframes(&project, video, PropertyId::Gain),
        vec![310, 330, 331, 351]
    );
    assert_the_racks_are_sound(&project);
}

// --- copying tracks ---

#[test]
fn a_track_is_copied_above_or_below_itself_with_everything_on_it_and_in_its_drawers() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let front = project.tracks()[0].id();
    let big = AnimationTarget::Clip(video);
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        big,
        PropertyId::Gain,
        &[(10, db(-20.0))],
    );
    put_a_rack_in(
        &mut project,
        AnimationTarget::Track(front),
        AnimationTarget::Track(front),
        PropertyId::Gain,
        &[(30, db(-1.0))],
    );
    project.set_track_title(front, "captions").unwrap();
    project.set_track_gain(front, Gain::from_decibels(-3.0)).unwrap();
    project.set_track_muted(front, true).unwrap();
    project.set_track_height(front, TrackHeight::from_points(60.0)).unwrap();
    project.set_track_speed(front, SpeedFactor::new(2).unwrap()).unwrap();

    let copy = project.duplicate_track(front, TrackSide::Below).unwrap();

    assert_eq!(project.tracks().len(), 3);
    assert_eq!(project.tracks()[0].id(), front);
    assert_eq!(project.tracks()[1].id(), copy, "below the original");
    assert_ne!(copy, front);
    let copied = project.track(copy).unwrap();
    assert_eq!(copied.title(), "captions copy");
    assert_eq!(copied.gain(), Gain::from_decibels(-3.0));
    assert!(copied.is_muted());
    assert_eq!(copied.height(), TrackHeight::from_points(60.0));
    assert_eq!(copied.speed(), SpeedFactor::new(2).unwrap());
    assert_eq!(copied.entries().len(), 1);
    let copied_text = copied.entries()[0].id();
    assert_eq!(copied_text.origin(), copy, "its clips are made on the new track");
    assert_ne!(copied_text, text);
    assert_eq!(
        copied.entries()[0].clip(),
        project.track(front).unwrap().entries()[0].clip()
    );
    assert_eq!(
        copied.entries()[0].drawer().racks()[0].target(),
        big,
        "what the text animated that is not on the track is still animated"
    );
    assert_eq!(
        copied.drawer().racks()[0].target(),
        AnimationTarget::Track(copy),
        "what animated the track animates its copy"
    );
    assert_eq!(
        project.drawer_of(AnimationTarget::Track(front)).unwrap().racks()[0].target(),
        AnimationTarget::Track(front),
        "and the original is as it was"
    );
    assert_eq!(
        where_the_video_has_keyframes(&project, video, PropertyId::Gain),
        vec![110],
        "the video is animated by the copy of the text too, at the same frame, where two keyframes make one"
    );
    assert_the_racks_are_sound(&project);

    let above = project.duplicate_track(front, TrackSide::Above).unwrap();
    assert_eq!(project.tracks()[0].id(), above, "in front of the original");
    assert_eq!(project.tracks()[1].id(), front);
    assert_eq!(project.tracks()[2].id(), copy);
    assert_the_racks_are_sound(&project);
}

#[test]
fn racks_in_a_copied_track_that_animate_clips_of_the_same_track_animate_the_copies_of_them() {
    let (mut project, video, _) = a_big_video_and_a_small_text();
    let back = project.tracks()[1].id();
    let own = AnimationTarget::Clip(video);
    // In the drawer of the video: keyframes of the video itself, and keyframes of the track it is on.
    put_a_rack_in(&mut project, own, own, PropertyId::Gain, &[(10, db(-3.0))]);
    put_a_rack_in(
        &mut project,
        own,
        AnimationTarget::Track(back),
        PropertyId::Gain,
        &[(20, db(-6.0))],
    );

    let copy = project.duplicate_track(back, TrackSide::Above).unwrap();

    let copied_video = project.track(copy).unwrap().entries()[0].id();
    let targets: Vec<AnimationTarget> = project.track(copy).unwrap().entries()[0]
        .drawer()
        .racks()
        .iter()
        .map(Rack::target)
        .collect();
    assert_eq!(
        targets,
        vec![AnimationTarget::Clip(copied_video), AnimationTarget::Track(copy)]
    );
    assert_eq!(
        where_the_video_has_keyframes(&project, video, PropertyId::Gain),
        vec![10],
        "the original is as it was"
    );
    assert_eq!(
        keyframe_frames(&project, AnimationTarget::Track(back), PropertyId::Gain),
        vec![20]
    );
    assert_eq!(
        keyframe_frames(&project, AnimationTarget::Track(copy), PropertyId::Gain),
        vec![20]
    );
    assert_the_racks_are_sound(&project);
}

#[test]
fn a_copied_track_has_names_of_its_own_so_that_it_can_be_edited_apart_from_the_original() {
    let (mut project, video, text) = a_big_video_and_a_small_text();
    let front = project.tracks()[0].id();
    put_a_rack_in(
        &mut project,
        AnimationTarget::Clip(text),
        AnimationTarget::Clip(video),
        PropertyId::Gain,
        &[(10, db(-20.0))],
    );
    let copy = project.duplicate_track(front, TrackSide::Below).unwrap();
    let copied_text = project.track(copy).unwrap().entries()[0].id();

    project.delete_clip(copied_text).unwrap();

    assert!(project.placement_of(text).is_some(), "the original text is still there");
    assert_eq!(
        where_the_video_has_keyframes(&project, video, PropertyId::Gain),
        vec![110],
        "and so are its keyframes"
    );
    assert_eq!(project.track(front).unwrap().entries()[0].drawer().len(), 1);
    assert_the_racks_are_sound(&project);
}

/// The titles of the tracks in the order of the list of them.
fn titles_of_the_tracks(project: &Project) -> Vec<String> {
    project.tracks().iter().map(|track| track.title().to_owned()).collect()
}

#[test]
fn a_copy_of_a_track_is_titled_like_it_with_copy_after_it_and_a_number_when_that_title_is_taken() {
    let mut project = Project::empty();
    let original = project.tracks()[0].id();
    project.set_track_title(original, "captions").unwrap();

    let first = project.duplicate_track(original, TrackSide::Below).unwrap();
    let second = project.duplicate_track(original, TrackSide::Below).unwrap();
    let third = project.duplicate_track(original, TrackSide::Above).unwrap();

    assert_eq!(
        titles_of_the_tracks(&project),
        ["captions copy 3", "captions", "captions copy 2", "captions copy"]
    );
    assert_eq!(project.track(first).unwrap().title(), "captions copy");
    assert_eq!(project.track(second).unwrap().title(), "captions copy 2");
    assert_eq!(project.track(third).unwrap().title(), "captions copy 3");
}

#[test]
fn a_copy_of_a_copy_is_a_copy_of_what_it_is_a_copy_of_and_not_a_copy_of_a_copy() {
    let mut project = Project::empty();
    let original = project.tracks()[0].id();
    project.set_track_title(original, "captions").unwrap();
    let copy = project.duplicate_track(original, TrackSide::Below).unwrap();

    let copy_of_the_copy = project.duplicate_track(copy, TrackSide::Below).unwrap();

    assert_eq!(project.track(copy_of_the_copy).unwrap().title(), "captions copy 2");
    let copy_of_that = project.duplicate_track(copy_of_the_copy, TrackSide::Below).unwrap();
    assert_eq!(project.track(copy_of_that).unwrap().title(), "captions copy 3");
}

#[test]
fn a_title_that_is_free_again_is_given_again_to_the_next_copy() {
    let mut project = Project::empty();
    let original = project.tracks()[0].id();
    project.set_track_title(original, "captions").unwrap();
    let first = project.duplicate_track(original, TrackSide::Below).unwrap();
    project.set_track_title(first, "something else").unwrap();

    let next = project.duplicate_track(original, TrackSide::Below).unwrap();

    assert_eq!(project.track(next).unwrap().title(), "captions copy");
}

#[test]
fn a_track_that_is_called_by_a_number_is_copied_like_any_other_and_one_with_no_title_is_just_a_copy() {
    let mut project = Project::empty();
    let original = project.tracks()[0].id();
    let title = project.track(original).unwrap().title().to_owned();
    assert!(
        title.chars().any(|letter| letter.is_ascii_digit()),
        "{title:?} is named by a number"
    );
    let copy = project.duplicate_track(original, TrackSide::Below).unwrap();
    assert_eq!(project.track(copy).unwrap().title(), format!("{title} copy"));

    project.set_track_title(original, "").unwrap();
    let from_nothing = project.duplicate_track(original, TrackSide::Below).unwrap();
    assert_eq!(project.track(from_nothing).unwrap().title(), "copy");
    let from_nothing_again = project.duplicate_track(original, TrackSide::Below).unwrap();
    assert_eq!(project.track(from_nothing_again).unwrap().title(), "copy 2");
}

#[test]
fn a_track_that_is_not_there_is_not_copied() {
    let mut project = Project::empty();
    let nobody = TrackId::new(50);
    let before = project.clone();

    assert_eq!(
        project.duplicate_track(nobody, TrackSide::Below),
        Err(EditError::UnknownTrack { track: nobody })
    );
    assert_eq!(project, before);
}

#[test]
fn a_track_made_by_copying_is_not_given_the_name_of_one_that_was_deleted() {
    let mut project = Project::empty();
    let original = project.tracks()[0].id();
    let first_copy = project.duplicate_track(original, TrackSide::Below).unwrap();
    project.delete_track(first_copy).unwrap();

    let second_copy = project.duplicate_track(original, TrackSide::Below).unwrap();

    assert_ne!(second_copy, first_copy);
    let added = project.add_track_in_front();
    assert_ne!(added, second_copy);
}
