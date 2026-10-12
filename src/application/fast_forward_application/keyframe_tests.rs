//! Tests of what the commands about keyframes do in the editor, and of how changing a property that is animated is
//! making a keyframe at the playhead.

use egui::{PointerButton, vec2};

use super::preview_layer_tests::{middle_of_the_layer, picture_rectangle_on_the_screen, select};
use super::test_window::{Window, project_of, project_with_a_video_and_a_sound};
use crate::application::commands::Command;
use crate::project::{
    Angle, AnimationTarget, CanvasArea, ClipId, Gain, Placement, Project, PropertyId, PropertyValue, TextColour,
    TextObject, TimelineFrameIndex,
};

fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

/// A window with a video of 300 frames, with nothing unsaved and nothing to undo, and its one clip.
pub(super) fn window_with_a_video() -> (Window, ClipId) {
    let mut window = Window::showing(project_of(300));
    window.application_mut().note_everything_is_saved();
    let clip = window.clip(0, 0);
    (window, clip)
}

/// A window with a video of 300 frames on its track and, in front of it, a track with a text object that is shown from
/// frame 20 to frame 80, with nothing unsaved and nothing to undo.
fn window_with_a_video_and_a_text_object() -> (Window, ClipId) {
    let mut project: Project = project_of(300);
    let track = project.add_track_in_front();
    let (clip, frames) = project
        .insert_text(track, frame(20), TextObject::default(), 60)
        .expect("there is room");
    assert_eq!(frames, 60);
    let mut window = Window::showing(project);
    window.application_mut().note_everything_is_saved();
    (window, clip)
}

fn add_a_keyframe(window: &mut Window, target: AnimationTarget, property: PropertyId, at: u64) {
    window.do_this(Command::AddKeyframe {
        target,
        property,
        frame: frame(at),
    });
}

fn remove_a_keyframe(window: &mut Window, target: AnimationTarget, property: PropertyId, at: u64) {
    window.do_this(Command::RemoveKeyframe {
        target,
        property,
        frame: frame(at),
    });
}

fn value_at(window: &Window, target: AnimationTarget, property: PropertyId, at: u64) -> Option<PropertyValue> {
    window
        .application()
        .project()
        .property_value_at(target, property, frame(at))
}

/// The frames of the keyframes of `property` of the clip `clip`, counted from the clip's first frame.
pub(super) fn keyframes_of(window: &Window, clip: ClipId, property: PropertyId) -> Vec<u64> {
    window
        .application()
        .project()
        .placement_of(clip)
        .unwrap()
        .animation
        .keyframes_of(property)
        .map(|keyframes| keyframes.iter().map(|keyframe| keyframe.frame).collect())
        .unwrap_or_default()
}

/// What the clip `clip` says its own gain is, which is what it is wherever its gain is not animated.
fn gain_the_clip_says(window: &Window, clip: ClipId) -> Gain {
    window
        .application()
        .project()
        .placement_of(clip)
        .unwrap()
        .clip
        .as_media()
        .unwrap()
        .gain()
}

/// Where the clip `clip` says it is on the picture, which is where it is wherever its place is not animated.
fn placement_the_clip_says(window: &Window, clip: ClipId) -> Placement {
    let project = window.application().project();
    project
        .placement_on_the_picture(project.placement_of(clip).unwrap().clip)
        .expect("a clip that is on the picture")
}

fn the_object_the_text_clip_says(window: &Window, clip: ClipId) -> TextObject {
    window
        .application()
        .project()
        .placement_of(clip)
        .unwrap()
        .clip
        .as_text()
        .unwrap()
        .object
        .clone()
}

// --- the commands ---

#[test]
fn adding_a_keyframe_and_taking_it_away_are_edits_that_are_undone_one_at_a_time() {
    let (mut window, clip) = window_with_a_video();
    let target = AnimationTarget::Clip(clip);

    add_a_keyframe(&mut window, target, PropertyId::Gain, 40);
    assert_eq!(keyframes_of(&window, clip, PropertyId::Gain), vec![40]);
    assert!(window.application().has_unsaved_changes());

    remove_a_keyframe(&mut window, target, PropertyId::Gain, 40);
    assert_eq!(keyframes_of(&window, clip, PropertyId::Gain), Vec::<u64>::new());

    window.do_this(Command::Undo);
    assert_eq!(
        keyframes_of(&window, clip, PropertyId::Gain),
        vec![40],
        "the keyframe is back"
    );
    window.do_this(Command::Undo);
    assert_eq!(
        keyframes_of(&window, clip, PropertyId::Gain),
        Vec::<u64>::new(),
        "and the edit before that is taken back too"
    );
    assert!(
        !window.application().has_unsaved_changes(),
        "which leaves the project as it was saved"
    );
}

#[test]
fn a_keyframe_that_cannot_be_made_is_not_an_edit() {
    let (mut window, clip) = window_with_a_video();
    let target = AnimationTarget::Clip(clip);
    // Words are not a property of a video, and frame 5000 is not in a clip that is 300 frames long.
    add_a_keyframe(&mut window, target, PropertyId::Text, 10);
    add_a_keyframe(&mut window, target, PropertyId::Gain, 5000);
    assert!(!window.application().has_unsaved_changes());
    assert!(!window.application().can_undo());
    assert!(!window.application().project().is_animated(target, PropertyId::Gain));
}

#[test]
fn taking_away_a_keyframe_that_is_not_there_is_not_an_edit() {
    let (mut window, clip) = window_with_a_video();
    let target = AnimationTarget::Clip(clip);
    add_a_keyframe(&mut window, target, PropertyId::Gain, 40);
    window.application_mut().note_everything_is_saved();

    remove_a_keyframe(&mut window, target, PropertyId::Gain, 41);
    remove_a_keyframe(&mut window, target, PropertyId::Muted, 40);

    assert!(!window.application().has_unsaved_changes());
    assert_eq!(keyframes_of(&window, clip, PropertyId::Gain), vec![40]);
}

#[test]
fn taking_away_the_last_keyframe_leaves_the_property_as_that_keyframe_said_it_was() {
    let (mut window, clip) = window_with_a_video();
    let target = AnimationTarget::Clip(clip);
    add_a_keyframe(&mut window, target, PropertyId::Gain, 0);
    window.move_current_frame_to(50);
    window.do_this(Command::SetClipGain {
        clip,
        gain: Gain::from_decibels(-6.0),
    });
    assert_eq!(keyframes_of(&window, clip, PropertyId::Gain), vec![0, 50]);

    remove_a_keyframe(&mut window, target, PropertyId::Gain, 0);
    assert_eq!(
        keyframes_of(&window, clip, PropertyId::Gain),
        vec![50],
        "one is left, so it is still animated"
    );
    assert_eq!(
        value_at(&window, target, PropertyId::Gain, 0),
        Some(PropertyValue::Number(-6.0))
    );

    remove_a_keyframe(&mut window, target, PropertyId::Gain, 50);
    assert!(!window.application().project().is_animated(target, PropertyId::Gain));
    assert_eq!(
        gain_the_clip_says(&window, clip),
        Gain::from_decibels(-6.0),
        "and what it said is what the clip says"
    );
}

// --- a property that is not animated ---

#[test]
fn a_property_that_is_not_animated_is_changed_for_the_whole_clip_wherever_the_playhead_is() {
    let (mut window, clip) = window_with_a_video();
    window.move_current_frame_to(120);
    window.do_this(Command::SetClipGain {
        clip,
        gain: Gain::from_decibels(-8.0),
    });
    assert_eq!(gain_the_clip_says(&window, clip), Gain::from_decibels(-8.0));
    assert!(
        window
            .application()
            .project()
            .placement_of(clip)
            .unwrap()
            .animation
            .is_empty(),
        "no keyframe was made"
    );
}

// --- a property that is animated ---

#[test]
fn changing_the_loudness_of_a_clip_that_has_keyframes_for_it_makes_a_keyframe_at_the_playhead_and_leaves_the_clip_alone()
 {
    let (mut window, clip) = window_with_a_video();
    let target = AnimationTarget::Clip(clip);
    add_a_keyframe(&mut window, target, PropertyId::Gain, 0);
    add_a_keyframe(&mut window, target, PropertyId::Gain, 100);
    window.move_current_frame_to(50);

    window.do_this(Command::SetClipGain {
        clip,
        gain: Gain::from_decibels(-10.0),
    });

    assert_eq!(keyframes_of(&window, clip, PropertyId::Gain), vec![0, 50, 100]);
    assert_eq!(
        value_at(&window, target, PropertyId::Gain, 50),
        Some(PropertyValue::Number(-10.0))
    );
    assert_eq!(
        value_at(&window, target, PropertyId::Gain, 25),
        Some(PropertyValue::Number(-5.0)),
        "it changes evenly from the keyframe before to the new one"
    );
    assert_eq!(
        value_at(&window, target, PropertyId::Gain, 75),
        Some(PropertyValue::Number(-5.0)),
        "and evenly back to the one after"
    );
    assert_eq!(
        gain_the_clip_says(&window, clip),
        Gain::UNCHANGED,
        "what the clip itself says is not touched"
    );
}

#[test]
fn changing_it_again_at_the_same_frame_changes_that_keyframe_and_does_not_make_another() {
    let (mut window, clip) = window_with_a_video();
    let target = AnimationTarget::Clip(clip);
    add_a_keyframe(&mut window, target, PropertyId::Gain, 10);
    window.move_current_frame_to(50);
    for decibels in [-3.0, -6.0, -9.0] {
        window.do_this(Command::SetClipGain {
            clip,
            gain: Gain::from_decibels(decibels),
        });
    }
    assert_eq!(keyframes_of(&window, clip, PropertyId::Gain), vec![10, 50]);
    assert_eq!(
        value_at(&window, target, PropertyId::Gain, 50),
        Some(PropertyValue::Number(-9.0))
    );
}

#[test]
fn a_change_that_made_a_keyframe_is_taken_back_by_undoing_it() {
    let (mut window, clip) = window_with_a_video();
    let target = AnimationTarget::Clip(clip);
    add_a_keyframe(&mut window, target, PropertyId::Gain, 10);
    window.move_current_frame_to(50);
    window.do_this(Command::SetClipGain {
        clip,
        gain: Gain::from_decibels(-9.0),
    });
    assert_eq!(keyframes_of(&window, clip, PropertyId::Gain), vec![10, 50]);

    window.do_this(Command::Undo);

    assert_eq!(keyframes_of(&window, clip, PropertyId::Gain), vec![10]);
}

#[test]
fn changing_what_is_animated_with_the_playhead_not_on_the_clip_changes_nothing() {
    // The video lasts 100 frames and a sound in front of it lasts 300, so that the timeline goes on after the video.
    let mut window = Window::showing(project_with_a_video_and_a_sound(100, 10));
    let clip = window.clip(1, 0);
    let target = AnimationTarget::Clip(clip);
    add_a_keyframe(&mut window, target, PropertyId::Gain, 10);
    window.application_mut().note_everything_is_saved();
    window.move_current_frame_to(200);

    window.do_this(Command::SetClipGain {
        clip,
        gain: Gain::from_decibels(-9.0),
    });

    assert!(
        !window.application().has_unsaved_changes(),
        "there is no frame of the clip to put a keyframe at"
    );
    assert_eq!(keyframes_of(&window, clip, PropertyId::Gain), vec![10]);
    assert_eq!(gain_the_clip_says(&window, clip), Gain::UNCHANGED);
}

#[test]
fn muting_a_clip_that_has_keyframes_for_it_makes_a_keyframe_at_the_playhead() {
    let (mut window, clip) = window_with_a_video();
    let target = AnimationTarget::Clip(clip);
    add_a_keyframe(&mut window, target, PropertyId::Muted, 0);
    window.move_current_frame_to(70);

    window.do_this(Command::SetClipMuted { clip, muted: true });

    assert_eq!(keyframes_of(&window, clip, PropertyId::Muted), vec![0, 70]);
    assert_eq!(
        value_at(&window, target, PropertyId::Muted, 69),
        Some(PropertyValue::Flag(false))
    );
    assert_eq!(
        value_at(&window, target, PropertyId::Muted, 70),
        Some(PropertyValue::Flag(true))
    );
    assert!(
        !window
            .application()
            .project()
            .placement_of(clip)
            .unwrap()
            .clip
            .as_media()
            .unwrap()
            .is_muted(),
        "the clip itself is not muted"
    );
}

#[test]
fn muting_a_track_that_has_keyframes_for_it_makes_a_keyframe_at_the_playhead() {
    let (mut window, _) = window_with_a_video();
    let track = window.application().project().tracks()[0].id();
    let target = AnimationTarget::Track(track);
    add_a_keyframe(&mut window, target, PropertyId::Muted, 0);
    window.move_current_frame_to(70);

    window.do_this(Command::SetTrackMuted { track, muted: true });

    assert_eq!(
        value_at(&window, target, PropertyId::Muted, 69),
        Some(PropertyValue::Flag(false))
    );
    assert_eq!(
        value_at(&window, target, PropertyId::Muted, 70),
        Some(PropertyValue::Flag(true))
    );
    assert!(!window.application().project().track(track).unwrap().is_muted());
}

#[test]
fn changing_the_loudness_of_a_track_that_has_keyframes_for_it_makes_one_at_the_playhead() {
    let (mut window, _) = window_with_a_video();
    let track = window.application().project().tracks()[0].id();
    let target = AnimationTarget::Track(track);
    add_a_keyframe(&mut window, target, PropertyId::Gain, 0);
    window.move_current_frame_to(200);

    window.do_this(Command::SetTrackGain {
        track,
        gain: Gain::from_decibels(-12.0),
    });

    assert_eq!(
        value_at(&window, target, PropertyId::Gain, 200),
        Some(PropertyValue::Number(-12.0))
    );
    assert_eq!(
        value_at(&window, target, PropertyId::Gain, 100),
        Some(PropertyValue::Number(-6.0))
    );
    assert_eq!(
        window.application().project().track(track).unwrap().gain(),
        Gain::UNCHANGED
    );
}

#[test]
fn keyframes_made_at_two_frames_soon_after_each_other_are_two_steps_of_the_history() {
    let (mut window, clip) = window_with_a_video();
    let target = AnimationTarget::Clip(clip);
    add_a_keyframe(&mut window, target, PropertyId::Gain, 0);
    window.move_current_frame_to(10);
    window.do_this(Command::SetClipGain {
        clip,
        gain: Gain::from_decibels(-3.0),
    });
    window.move_current_frame_to(50);
    window.do_this(Command::SetClipGain {
        clip,
        gain: Gain::from_decibels(-6.0),
    });
    assert_eq!(keyframes_of(&window, clip, PropertyId::Gain), vec![0, 10, 50]);

    window.do_this(Command::Undo);
    assert_eq!(
        keyframes_of(&window, clip, PropertyId::Gain),
        vec![0, 10],
        "only the keyframe at frame 50 is taken back"
    );
    window.do_this(Command::Undo);
    assert_eq!(keyframes_of(&window, clip, PropertyId::Gain), vec![0]);
}

#[test]
fn changing_the_same_keyframe_again_and_again_is_still_one_step_of_the_history() {
    let (mut window, clip) = window_with_a_video();
    let target = AnimationTarget::Clip(clip);
    add_a_keyframe(&mut window, target, PropertyId::Gain, 0);
    window.move_current_frame_to(50);
    for decibels in [-3.0, -6.0, -9.0] {
        window.do_this(Command::SetClipGain {
            clip,
            gain: Gain::from_decibels(decibels),
        });
    }

    window.do_this(Command::Undo);

    assert_eq!(
        keyframes_of(&window, clip, PropertyId::Gain),
        vec![0],
        "all the changes were one gesture"
    );
}

// --- where something is on the picture ---

#[test]
fn moving_a_clip_whose_position_is_animated_makes_a_position_keyframe_at_the_playhead_and_changes_only_that() {
    let (mut window, clip) = window_with_a_video();
    let target = AnimationTarget::Clip(clip);
    window.do_this(Command::SetClipPlacement {
        clip,
        placement: Placement::upright(CanvasArea::new(0.0, 0.0, 0.5, 0.5)),
    });
    add_a_keyframe(&mut window, target, PropertyId::Position, 0);
    window.move_current_frame_to(60);

    // The same box, moved across and down.
    window.do_this(Command::SetClipPlacement {
        clip,
        placement: Placement::upright(CanvasArea::new(0.5, 0.5, 0.5, 0.5)),
    });

    assert_eq!(keyframes_of(&window, clip, PropertyId::Position), vec![0, 60]);
    assert_eq!(
        value_at(&window, target, PropertyId::Position, 0),
        Some(PropertyValue::Pair(0.25, 0.25))
    );
    assert_eq!(
        value_at(&window, target, PropertyId::Position, 60),
        Some(PropertyValue::Pair(0.75, 0.75))
    );
    assert_eq!(
        value_at(&window, target, PropertyId::Position, 30),
        Some(PropertyValue::Pair(0.5, 0.5))
    );
    assert_eq!(
        keyframes_of(&window, clip, PropertyId::Size),
        Vec::<u64>::new(),
        "the size is not animated"
    );
    assert_eq!(keyframes_of(&window, clip, PropertyId::Rotation), Vec::<u64>::new());
    assert_eq!(
        placement_the_clip_says(&window, clip),
        Placement::upright(CanvasArea::new(0.0, 0.0, 0.5, 0.5)),
        "what the clip itself says is not touched"
    );
}

#[test]
fn resizing_and_turning_a_clip_whose_position_is_animated_change_those_properties_for_the_whole_clip() {
    let (mut window, clip) = window_with_a_video();
    let target = AnimationTarget::Clip(clip);
    add_a_keyframe(&mut window, target, PropertyId::Position, 0);
    window.move_current_frame_to(60);

    // The same middle (the middle of the picture), a smaller box, turned.
    window.do_this(Command::SetClipPlacement {
        clip,
        placement: Placement::new(CanvasArea::new(0.25, 0.25, 0.5, 0.5), Angle::from_degrees(45.0)),
    });

    assert_eq!(
        keyframes_of(&window, clip, PropertyId::Position),
        vec![0],
        "the middle is where it was"
    );
    assert_eq!(keyframes_of(&window, clip, PropertyId::Size), Vec::<u64>::new());
    assert_eq!(
        value_at(&window, target, PropertyId::Size, 200),
        Some(PropertyValue::Pair(0.5, 0.5))
    );
    assert_eq!(
        value_at(&window, target, PropertyId::Rotation, 200),
        Some(PropertyValue::Number(45.0))
    );
}

/// Regression test: a clip that is turned round by keyframes by more than half a turn is shown to be turned by the angle that
/// is the same as that (-168 degrees for 192), so that the preview moving it, which gives the angle it was shown with back,
/// looked like a change of how far it is turned, and made a keyframe for that at the playhead.
#[test]
fn moving_a_clip_that_keyframes_have_turned_more_than_half_a_turn_makes_no_keyframe_for_its_turn() {
    let (mut window, clip) = window_with_a_video();
    let target = AnimationTarget::Clip(clip);
    add_a_keyframe(&mut window, target, PropertyId::Rotation, 0);
    for (frame, degrees) in [(100, 120.0), (200, 240.0)] {
        window.move_current_frame_to(frame);
        let placement = placement_the_clip_says(&window, clip);
        window.do_this(Command::SetClipPlacement {
            clip,
            placement: Placement::new(placement.area, Angle::from_degrees(degrees)),
        });
    }
    assert_eq!(keyframes_of(&window, clip, PropertyId::Rotation), vec![0, 100, 200]);
    assert_eq!(
        value_at(&window, target, PropertyId::Rotation, 200),
        Some(PropertyValue::Number(240.0)),
        "turned round on"
    );

    // 60 frames on from the keyframe at 100 it is 192 degrees round, which is shown as -168.
    window.move_current_frame_to(160);
    let shown = window.application().project().placement_at(clip, frame(160)).unwrap();
    assert!((shown.rotation.degrees() + 168.0).abs() < 1e-3, "{shown:?}");
    window.do_this(Command::SetClipPlacement {
        clip,
        placement: shown.moved_by(0.1, 0.0),
    });

    assert_eq!(
        keyframes_of(&window, clip, PropertyId::Rotation),
        vec![0, 100, 200],
        "the turn was not changed, so it has no keyframe of its own at the playhead"
    );
}

#[test]
fn a_clip_that_has_nothing_about_its_place_animated_is_put_where_it_is_told_as_it_always_was() {
    let (mut window, clip) = window_with_a_video();
    window.move_current_frame_to(150);
    let somewhere_else = Placement::new(CanvasArea::new(0.1, 0.2, 0.3, 0.4), Angle::from_degrees(30.0));

    window.do_this(Command::SetClipPlacement {
        clip,
        placement: somewhere_else,
    });

    assert_eq!(placement_the_clip_says(&window, clip), somewhere_else);
    assert!(
        window
            .application()
            .project()
            .placement_of(clip)
            .unwrap()
            .animation
            .is_empty()
    );
}

#[test]
fn turning_a_clip_whose_turn_is_animated_the_short_way_round_keeps_the_turns_in_step() {
    let (mut window, clip) = window_with_a_video();
    let target = AnimationTarget::Clip(clip);
    window.do_this(Command::SetClipPlacement {
        clip,
        placement: Placement::new(CanvasArea::WHOLE_PICTURE, Angle::from_degrees(170.0)),
    });
    add_a_keyframe(&mut window, target, PropertyId::Rotation, 0);
    window.move_current_frame_to(60);

    // 170 degrees to -170 is 20 degrees more, not 340 less.
    window.do_this(Command::SetClipPlacement {
        clip,
        placement: Placement::new(CanvasArea::WHOLE_PICTURE, Angle::from_degrees(-170.0)),
    });

    assert_eq!(
        value_at(&window, target, PropertyId::Rotation, 60),
        Some(PropertyValue::Number(190.0))
    );
    assert_eq!(
        value_at(&window, target, PropertyId::Rotation, 30),
        Some(PropertyValue::Number(180.0))
    );
}

// --- text ---

#[test]
fn changing_a_text_object_that_has_its_colour_animated_changes_the_keyframe_of_the_colour_and_the_rest_for_the_whole_clip()
 {
    let (mut window, clip) = window_with_a_video_and_a_text_object();
    let target = AnimationTarget::Clip(clip);
    add_a_keyframe(&mut window, target, PropertyId::Colour, 20);
    window.move_current_frame_to(60);
    let red = TextColour::new(255, 0, 0, 255);
    let changed = TextObject {
        colour: red,
        text: "Changed".to_owned(),
        ..the_object_the_text_clip_says(&window, clip)
    };

    window.do_this(Command::SetTextObject { clip, object: changed });

    // The clip begins at frame 20, so frame 60 is 40 frames in.
    assert_eq!(keyframes_of(&window, clip, PropertyId::Colour), vec![0, 40]);
    assert_eq!(
        value_at(&window, target, PropertyId::Colour, 60),
        Some(PropertyValue::Colour(red))
    );
    assert_eq!(
        value_at(&window, target, PropertyId::Colour, 20),
        Some(PropertyValue::Colour(TextColour::WHITE)),
        "the colour at the other keyframe is as it was"
    );
    assert_eq!(
        keyframes_of(&window, clip, PropertyId::Text),
        Vec::<u64>::new(),
        "the words are not animated"
    );
    assert_eq!(
        the_object_the_text_clip_says(&window, clip).text,
        "Changed",
        "so they are changed for the whole clip"
    );
    assert_eq!(
        the_object_the_text_clip_says(&window, clip).colour,
        TextColour::WHITE,
        "and what the clip says its colour is is not touched"
    );
}

#[test]
fn a_text_object_that_has_nothing_animated_is_changed_as_it_always_was() {
    let (mut window, clip) = window_with_a_video_and_a_text_object();
    window.move_current_frame_to(60);
    let changed = TextObject {
        colour: TextColour::new(0, 255, 0, 255),
        text: "Changed".to_owned(),
        ..the_object_the_text_clip_says(&window, clip)
    };

    window.do_this(Command::SetTextObject {
        clip,
        object: changed.clone(),
    });

    assert_eq!(the_object_the_text_clip_says(&window, clip), changed);
    assert!(
        window
            .application()
            .project()
            .placement_of(clip)
            .unwrap()
            .animation
            .is_empty()
    );
}

// --- the preview ---

/// A window with a video of 300 frames that is selected, whose middle is at the middle of the picture at frame 0 and
/// a quarter of the picture across and down from there at frame 200: its position has keyframes at those frames.
fn window_with_a_video_that_moves_on_the_preview() -> (Window, ClipId) {
    let (mut window, clip) = window_with_a_video();
    select(&mut window, clip);
    add_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Position, 0);
    window.move_current_frame_to(200);
    window.do_this(Command::SetClipPlacement {
        clip,
        placement: Placement::upright(CanvasArea::new(0.25, 0.25, 1.0, 1.0)),
    });
    assert_eq!(keyframes_of(&window, clip, PropertyId::Position), vec![0, 200]);
    (window, clip)
}

#[test]
fn the_preview_shows_a_clip_where_its_keyframes_say_it_is_at_the_current_frame() {
    let (mut window, _) = window_with_a_video_that_moves_on_the_preview();
    let picture = picture_rectangle_on_the_screen(&window);

    window.move_current_frame_to(0);
    let at_the_start = middle_of_the_layer(&window);
    assert!(
        (at_the_start - picture.center()).length() < 1.0,
        "the middle of the picture: {at_the_start:?}"
    );

    for (frame, fraction_of_the_way) in [(100, 0.5), (200, 1.0), (250, 1.0)] {
        window.move_current_frame_to(frame);
        let middle = middle_of_the_layer(&window);
        let expected = at_the_start + vec2(0.25 * picture.width(), 0.25 * picture.height()) * fraction_of_the_way;
        assert!(
            (middle - expected).length() < 1.0,
            "frame {frame}: the middle is at {middle:?}, not {expected:?}"
        );
    }
}

/// What the position of the video of a window like [`window_with_a_video_that_moves_on_the_preview`] (with its keyframes if
/// `animated` and without if not) is at frame 100, as a pair of fractions of the picture.
fn position_at_frame_100(window: &Window, clip: ClipId) -> (f32, f32) {
    match value_at(window, AnimationTarget::Clip(clip), PropertyId::Position, 100) {
        Some(PropertyValue::Pair(across, down)) => (across, down),
        other => panic!("the position is a pair, not {other:?}"),
    }
}

/// A window with the video selected and the playhead at frame 100, in which the video has the keyframes that
/// [`window_with_a_video_that_moves_on_the_preview`] makes (if `animated`) or none (if not).
fn window_with_a_video_to_drag_at_frame_100(animated: bool) -> (Window, ClipId) {
    let (mut window, clip) = if animated {
        window_with_a_video_that_moves_on_the_preview()
    } else {
        let (mut window, clip) = window_with_a_video();
        select(&mut window, clip);
        (window, clip)
    };
    window.move_current_frame_to(100);
    (window, clip)
}

#[test]
fn dragging_an_animated_clip_on_the_preview_moves_it_as_far_as_dragging_one_that_is_not_animated_but_in_a_keyframe() {
    let how_far_a_drag_moves_it = |animated: bool| {
        let (mut window, clip) = window_with_a_video_to_drag_at_frame_100(animated);
        let before = position_at_frame_100(&window, clip);
        let middle = middle_of_the_layer(&window);
        window.drag(middle, middle + vec2(40.0, 20.0), PointerButton::Primary);
        let after = position_at_frame_100(&window, clip);
        (after.0 - before.0, after.1 - before.1)
    };

    let (across_if_not_animated, down_if_not_animated) = how_far_a_drag_moves_it(false);
    let (across, down) = how_far_a_drag_moves_it(true);

    assert!(
        across_if_not_animated > 0.03 && down_if_not_animated > 0.03,
        "the drag moved the clip that was not animated"
    );
    assert!(
        (across - across_if_not_animated).abs() < 1e-4,
        "across: {across} and not {across_if_not_animated}"
    );
    assert!(
        (down - down_if_not_animated).abs() < 1e-4,
        "down: {down} and not {down_if_not_animated}"
    );
}

#[test]
fn dragging_a_clip_whose_position_is_animated_on_the_preview_makes_a_keyframe_at_the_current_frame_and_moves_only_there()
 {
    let (mut window, clip) = window_with_a_video_to_drag_at_frame_100(true);
    let target = AnimationTarget::Clip(clip);
    let middle = middle_of_the_layer(&window);
    window.application_mut().note_everything_is_saved();

    window.drag(middle, middle + vec2(40.0, 20.0), PointerButton::Primary);

    assert_eq!(
        keyframes_of(&window, clip, PropertyId::Position),
        vec![0, 100, 200],
        "a keyframe where the playhead is"
    );
    let (across, down) = position_at_frame_100(&window, clip);
    assert!(
        across > 0.625 + 0.03 && down > 0.625 + 0.03,
        "it was moved from half way: {across}, {down}"
    );
    assert_eq!(
        value_at(&window, target, PropertyId::Position, 0),
        Some(PropertyValue::Pair(0.5, 0.5)),
        "the first keyframe is as it was"
    );
    assert_eq!(
        value_at(&window, target, PropertyId::Position, 200),
        Some(PropertyValue::Pair(0.75, 0.75)),
        "and so is the last"
    );
    assert!(
        window
            .application()
            .project()
            .placement_of(clip)
            .unwrap()
            .clip
            .placement()
            .is_none(),
        "the clip itself was not put anywhere"
    );

    window.do_this(Command::Undo);
    assert_eq!(
        keyframes_of(&window, clip, PropertyId::Position),
        vec![0, 200],
        "the whole drag is one step of the history"
    );
    assert_eq!(position_at_frame_100(&window, clip), (0.625, 0.625));
}
