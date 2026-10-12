//! Tests of what the export makes of layers that have keyframes: the section they are in is cut into the stretches over which what
//! is exported does not change.

use super::tests::{clip_location, frame, plan, project_with_video, stand_in_for_text, video_source};
use super::*;
use crate::composition::{PicturePlan, plan_pictures};
use crate::media::export::animated_sections::{self, MOST_STRETCHES_OF_ANIMATION};
use crate::project::{
    Angle, AnimationTarget, CanvasArea, Placement, PropertyId, PropertyValue, TextColour, TextObject,
};
use crate::test_support::make_a_keyframe;

const PICTURE_SIZE: (u32, u32) = (640, 360);

/// A project of a video of 640x360 pixels that lasts `frames` frames, of which the clip is put in the top left corner, a quarter
/// of the picture across and down (160x90 pixels), and the clip.
fn project_with_a_small_video(frames: u64) -> (Project, ClipId) {
    let mut project = Project::empty();
    project
        .import_source(video_source(1, 640, 360, frames, false), frame(0))
        .unwrap();
    let clip = clip_location(&project, 0, 0);
    project
        .set_clip_placement(clip, Placement::upright(CanvasArea::new(0.0, 0.0, 0.25, 0.25)))
        .unwrap();
    (project, clip)
}

fn position(across_in_pixels: f32) -> PropertyValue {
    // (The middle of a box that is 160 pixels across.)
    PropertyValue::Pair(across_in_pixels / 640.0 + 0.125, 0.125)
}

fn stretches_of(project: &Project) -> Vec<PictureSection> {
    let pictures: PicturePlan = plan_pictures(project, |_| true);
    animated_sections::with_keyframes_resolved(&pictures.sections, PICTURE_SIZE).expect("not too much to export")
}

/// How many pixels from the left edge of the picture the layer at the back of `stretch` is.
fn left_in_pixels(stretch: &PictureSection) -> i32 {
    place_to_export(stretch.layers[0].placement, PICTURE_SIZE).left_in_pixels
}

fn assert_the_stretches_follow_each_other_from_frame_to_frame(stretches: &[PictureSection], first: u64, end: u64) {
    assert_eq!(stretches.first().unwrap().first_timeline_frame, first);
    assert_eq!(stretches.last().unwrap().end_timeline_frame, end);
    for pair in stretches.windows(2) {
        assert_eq!(pair[0].end_timeline_frame, pair[1].first_timeline_frame);
    }
}

// --- where a layer is ---

#[test]
fn a_layer_that_moves_a_whole_pixel_every_frame_is_a_stretch_of_its_own_for_each_frame_it_moves() {
    let (mut project, clip) = project_with_a_small_video(300);
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Position,
        0,
        position(0.0),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Position,
        60,
        position(60.0),
    );

    let stretches = stretches_of(&project);

    // Frames 0 to 59 are one stretch each, and from frame 60 it stays where it got to.
    assert_eq!(stretches.len(), 61);
    assert_the_stretches_follow_each_other_from_frame_to_frame(&stretches, 0, 300);
    for (number, stretch) in stretches[..60].iter().enumerate() {
        assert_eq!(stretch.timeline_frame_count(), 1, "stretch {number}");
        assert_eq!(left_in_pixels(stretch), number as i32, "stretch {number}");
    }
    assert_eq!(
        (stretches[60].first_timeline_frame, stretches[60].end_timeline_frame),
        (60, 300)
    );
    assert_eq!(left_in_pixels(&stretches[60]), 60);
}

#[test]
fn the_stretches_of_a_moving_video_show_the_pictures_that_come_in_their_frames() {
    let (mut project, clip) = project_with_a_small_video(300);
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Position,
        0,
        position(0.0),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Position,
        60,
        position(60.0),
    );

    let stretches = stretches_of(&project);

    for stretch in &stretches {
        let video = stretch.layers[0].video().expect("the video");
        assert_eq!(video.first_source_frame, stretch.first_timeline_frame, "{stretch:?}");
        assert_eq!(video.end_source_frame, stretch.end_timeline_frame, "{stretch:?}");
        assert!(
            !stretch.layers[0].has_keyframes(),
            "each is as it is at its first frame"
        );
    }
}

#[test]
fn a_slow_move_makes_few_stretches_because_the_place_in_whole_pixels_changes_only_when_it_has_moved_one() {
    let (mut project, clip) = project_with_a_small_video(300);
    // Ten pixels across 299 frames.
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Position,
        0,
        position(0.0),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Position,
        299,
        position(10.0),
    );

    let stretches = stretches_of(&project);

    assert_eq!(stretches.len(), 11, "a stretch for each of the places 0 to 10");
    let places: Vec<i32> = stretches.iter().map(left_in_pixels).collect();
    assert_eq!(places, (0..=10).collect::<Vec<_>>());
    assert_the_stretches_follow_each_other_from_frame_to_frame(&stretches, 0, 300);
}

#[test]
fn a_layer_that_is_not_moved_by_its_keyframes_is_one_stretch_wherever_they_are() {
    let (mut project, clip) = project_with_a_small_video(300);
    // The same place at both keyframes, and so at every frame between.
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Position,
        0,
        position(5.0),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Position,
        200,
        position(5.0),
    );

    let stretches = stretches_of(&project);

    assert_eq!(stretches.len(), 1);
    assert_eq!(left_in_pixels(&stretches[0]), 5);
}

// --- how far it is turned ---

#[test]
fn a_turn_by_keyframes_is_made_in_steps_of_a_tenth_of_a_degree() {
    let (mut project, clip) = project_with_a_small_video(300);
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Rotation,
        0,
        PropertyValue::Number(0.0),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Rotation,
        299,
        PropertyValue::Number(3.0),
    );

    let stretches = stretches_of(&project);

    assert_eq!(stretches.len(), 31, "0.0 up to 3.0 degrees in steps of 0.1");
    for (number, stretch) in stretches.iter().enumerate() {
        let degrees = stretch.layers[0].placement.rotation.degrees();
        assert!(
            (degrees - number as f32 * 0.1).abs() < 1e-4,
            "stretch {number}: {degrees}"
        );
    }
}

#[test]
fn a_turn_that_is_not_made_by_keyframes_is_not_changed_to_a_step() {
    let (mut project, clip) = project_with_a_small_video(300);
    project
        .set_clip_placement(
            clip,
            Placement::new(CanvasArea::new(0.0, 0.0, 0.25, 0.25), Angle::from_degrees(12.345)),
        )
        .unwrap();
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Position,
        0,
        position(0.0),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Position,
        10,
        position(10.0),
    );

    let stretches = stretches_of(&project);

    for stretch in &stretches {
        assert_eq!(
            stretch.layers[0].placement.rotation.degrees(),
            12.345,
            "as the clip says"
        );
    }
}

// --- sections that are not animated ---

#[test]
fn sections_whose_layers_have_no_keyframes_are_as_they_were() {
    let (project, _) = project_with_a_small_video(300);
    let pictures = plan_pictures(&project, |_| true);
    let resolved = animated_sections::with_keyframes_resolved(&pictures.sections, PICTURE_SIZE).unwrap();
    assert_eq!(resolved, pictures.sections);
}

#[test]
fn only_the_sections_a_moving_layer_is_in_are_cut_and_the_rest_are_as_they_were() {
    // Text over frames 30 to 90 of a video of 300 frames, which moves 9 pixels across.
    let mut project = project_with_video(640, 360, false);
    let front = project.add_track_in_front();
    let (text, _) = project
        .insert_text(front, frame(30), TextObject::default(), 60)
        .unwrap();
    let across = TextObject::default().area.left() + TextObject::default().area.width() / 2.0;
    let down = TextObject::default().area.top() + TextObject::default().area.height() / 2.0;
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(text),
        PropertyId::Position,
        30,
        PropertyValue::Pair(across, down),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(text),
        PropertyId::Position,
        89,
        PropertyValue::Pair(across + 9.0 / 640.0, down),
    );
    let pictures = plan_pictures(&project, |_| true);
    assert_eq!(pictures.sections.len(), 3);

    let resolved = animated_sections::with_keyframes_resolved(&pictures.sections, PICTURE_SIZE).unwrap();

    assert_eq!(
        resolved.len(),
        1 + 10 + 1,
        "before it, the ten places it is in, and after it"
    );
    assert_eq!(resolved[0], pictures.sections[0]);
    assert_eq!(resolved[11], pictures.sections[2]);
    assert_the_stretches_follow_each_other_from_frame_to_frame(&resolved, 0, 300);
    // The video under the text carries on across the cuts.
    for pair in resolved[1..11].windows(2) {
        let (before, after) = (pair[0].layers[0].video().unwrap(), pair[1].layers[0].video().unwrap());
        assert_eq!(
            before.end_source_frame - before.first_source_frame,
            pair[0].timeline_frame_count()
        );
        assert_eq!(
            after.first_source_frame, before.end_source_frame,
            "the pictures run on from one stretch to the next"
        );
    }
}

// --- pictures of text ---

fn project_with_text_shown_for_60_frames() -> (Project, ClipId) {
    let mut project = project_with_video(640, 360, false);
    let front = project.add_track_in_front();
    let (text, _) = project
        .insert_text(front, frame(30), TextObject::default(), 60)
        .unwrap();
    (project, text)
}

#[test]
fn text_that_only_moves_is_one_picture_however_many_places_it_is_laid_over_at() {
    let (mut project, text) = project_with_text_shown_for_60_frames();
    let object = TextObject::default();
    let across = object.area.left() + object.area.width() / 2.0;
    let down = object.area.top() + object.area.height() / 2.0;
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(text),
        PropertyId::Position,
        30,
        PropertyValue::Pair(across, down),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(text),
        PropertyId::Position,
        89,
        PropertyValue::Pair(across + 9.0 / 640.0, down),
    );

    let plan = plan(&project).unwrap();

    assert_eq!(
        plan.still_pictures.len(),
        1,
        "the picture of the text is the same wherever it is"
    );
    assert_eq!(plan.still_pictures[0].kind, StillPictureKind::Text(text));
    assert_eq!(
        plan.filter_graph.text.matches("overlay=").count(),
        10,
        "and it is laid over at ten places"
    );
}

#[test]
fn text_that_changes_colour_is_a_picture_for_each_colour_it_has() {
    let (mut project, text) = project_with_text_shown_for_60_frames();
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(text),
        PropertyId::Colour,
        30,
        PropertyValue::Colour(TextColour::new(0, 0, 0, 255)),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(text),
        PropertyId::Colour,
        33,
        PropertyValue::Colour(TextColour::new(255, 255, 255, 255)),
    );
    let colours_of_the_pictures = std::cell::RefCell::new(Vec::new());
    let render = |object: &TextObject, width: u32, height: u32| {
        colours_of_the_pictures.borrow_mut().push(object.colour);
        stand_in_for_text(object, width, height)
    };

    let plan = plan_export(&project, Settings::DEFAULT, &render).unwrap();

    assert_eq!(
        *colours_of_the_pictures.borrow(),
        vec![
            TextColour::new(0, 0, 0, 255),
            TextColour::new(85, 85, 85, 255),
            TextColour::new(170, 170, 170, 255),
            TextColour::new(255, 255, 255, 255),
        ],
        "one picture for each colour, made in the order they are seen in"
    );
    assert_eq!(plan.still_pictures.len(), 4);
    assert!(
        plan.still_pictures
            .iter()
            .all(|still| still.kind == StillPictureKind::Text(text))
    );
}

#[test]
fn a_text_object_whose_size_is_animated_has_a_picture_of_each_size() {
    let (mut project, text) = project_with_text_shown_for_60_frames();
    let object = TextObject::default();
    let width = object.area.width();
    let height = object.area.height();
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(text),
        PropertyId::Size,
        30,
        PropertyValue::Pair(width, height),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(text),
        PropertyId::Size,
        32,
        PropertyValue::Pair(width + 4.0 / 640.0, height),
    );
    let plan = plan(&project).unwrap();
    // 0 pixels wider, 2 wider, 4 wider: three pictures that are each as big as their area is.
    let widths: Vec<u32> = plan
        .still_pictures
        .iter()
        .map(|still| still.picture.width_in_pixels)
        .collect();
    assert_eq!(widths.len(), 3, "{widths:?}");
    assert!(widths.windows(2).all(|pair| pair[1] > pair[0]), "{widths:?}");
}

// --- what is too much ---

#[test]
fn an_animation_that_would_need_more_stretches_than_ffmpeg_can_be_asked_for_in_reasonable_time_is_refused() {
    // 4000 frames, and the video goes there and back again across 100 pixels every 100 frames: every frame is a place.
    let (mut project, clip) = project_with_a_small_video(4000);
    for number in 0..40 {
        let across = if number % 2 == 0 { 0.0 } else { 100.0 };
        make_a_keyframe(
            &mut project,
            AnimationTarget::Clip(clip),
            PropertyId::Position,
            number * 100,
            position(across),
        );
    }

    let error = plan(&project).expect_err("it is too much");

    assert_eq!(
        error,
        ExportPlanError::TooMuchAnimation {
            most: MOST_STRETCHES_OF_ANIMATION
        }
    );
    assert!(error.to_string().contains("too much animation"), "{error}");
}

#[test]
fn every_reason_a_plan_cannot_be_made_has_its_own_heading_for_the_message_that_says_so() {
    let nothing = ExportPlanError::NothingToExport;
    let too_much = ExportPlanError::TooMuchAnimation { most: 1 };
    assert_eq!(nothing.heading(), "There is nothing to export");
    assert_eq!(too_much.heading(), "There is too much animation to export");
    assert_ne!(nothing.heading(), too_much.heading());
}

#[test]
fn the_same_length_of_slow_animation_is_exported_because_it_makes_few_stretches() {
    let (mut project, clip) = project_with_a_small_video(4000);
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Position,
        0,
        position(0.0),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        PropertyId::Position,
        3999,
        position(100.0),
    );

    let plan = plan(&project).expect("100 pixels over 4000 frames is 101 places");

    assert_eq!(plan.filter_graph.text.matches("pad=").count(), 101);
}

/// A project of `frames` frames, in which the video is in another place at every frame than at the one before: a stretch for each.
fn project_with_a_video_that_is_somewhere_else_every_frame(frames: u64) -> Project {
    let (mut project, clip) = project_with_a_small_video(frames);
    for timeline_frame in 0..frames {
        let across = if timeline_frame % 2 == 0 { 0.0 } else { 1.0 };
        make_a_keyframe(
            &mut project,
            AnimationTarget::Clip(clip),
            PropertyId::Position,
            timeline_frame,
            position(across),
        );
    }
    project
}

#[test]
fn as_many_stretches_as_may_be_exported_are_exported_and_one_more_is_refused() {
    let most = MOST_STRETCHES_OF_ANIMATION as u64;

    let stretches = stretches_of(&project_with_a_video_that_is_somewhere_else_every_frame(most));
    assert_eq!(stretches.len(), MOST_STRETCHES_OF_ANIMATION, "a stretch for each frame");

    let pictures = plan_pictures(
        &project_with_a_video_that_is_somewhere_else_every_frame(most + 1),
        |_| true,
    );
    assert_eq!(
        animated_sections::with_keyframes_resolved(&pictures.sections, PICTURE_SIZE),
        Err(ExportPlanError::TooMuchAnimation {
            most: MOST_STRETCHES_OF_ANIMATION
        })
    );
}
