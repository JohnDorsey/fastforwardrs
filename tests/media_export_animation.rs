//! Exporting projects whose clips have keyframes with the real ffmpeg, checked picture by picture against what the keyframes say:
//! where a video is, how big it is and how far it is turned at each frame, and what a text object is like.

mod support;

use fastforwardrs::media::FfmpegToolchain;
use fastforwardrs::project::{
    AnimationTarget, CanvasArea, ClipId, Placement, Project, PropertyId, PropertyValue, TextColour, TextObject,
    TimelineFrameIndex, with_placement_value,
};
use support::{
    ClipOfTheVideo, clip_number, counter_video, decode_video, export, export_with_text_pictures, make_a_keyframe,
    project_with_tracks_of_videos, scratch_directory, solid_text_picture,
};

/// How bright the pixel at `(x, y)` of a picture of the size of the test pictures is.
fn brightness_at(picture: &[u8], x: usize, y: usize) -> u8 {
    support::brightness_at(picture, WIDTH, x, y)
}

/// The smallest box that has all of the pixels of the picture that are not black.
fn box_of_what_is_not_black(picture: &[u8]) -> (usize, usize, usize, usize) {
    support::box_of_what_is_not_black(picture, WIDTH, HEIGHT)
}

const WIDTH: usize = 320;
const HEIGHT: usize = 180;

fn the_first_60_frames() -> ClipOfTheVideo {
    ClipOfTheVideo {
        gap_before_frames: 0,
        first_source_frame: 0,
        end_source_frame: 60,
        speed: 1,
    }
}

/// A project of the first 60 frames of the counter video, put at `area` of the picture, and the clip.
fn project_with_a_video_at(toolchain: &FfmpegToolchain, area: CanvasArea) -> (Project, ClipId) {
    let video = counter_video(toolchain);
    let mut project = project_with_tracks_of_videos(toolchain, &[&video], &[(0, &[the_first_60_frames()])]);
    let clip = clip_number(&project, 0);
    project.set_clip_placement(clip, Placement::upright(area)).unwrap();
    (project, clip)
}

fn keyframe(project: &mut Project, clip: ClipId, property: PropertyId, frame: u64, value: PropertyValue) {
    make_a_keyframe(project, AnimationTarget::Clip(clip), property, frame, value);
}

/// The picture that frame `timeline_frame` of the export of `animated` (a project of the first frames of the counter video, whose
/// clip `clip` has keyframes for where it is, and plays the video at normal speed) should be, made the way pictures have always
/// been made, with no keyframes: the one picture of the video that is shown then, put where the keyframes say it is at that frame.
fn the_picture_made_without_keyframes(
    toolchain: &FfmpegToolchain,
    animated: &Project,
    clip: ClipId,
    timeline_frame: u64,
) -> Vec<u8> {
    the_picture_of_a_source_frame_made_without_keyframes(toolchain, animated, clip, timeline_frame, timeline_frame, 1)
}

/// Like [`the_picture_made_without_keyframes`], for a clip that plays at `speed` times normal speed and shows the picture
/// `source_frame` of the video at the frame `timeline_frame` (a clip that is sped up does not show the one with the same number,
/// and has the fast-forward symbol over it, which the picture made for comparison is to have too).
fn the_picture_of_a_source_frame_made_without_keyframes(
    toolchain: &FfmpegToolchain,
    animated: &Project,
    clip: ClipId,
    timeline_frame: u64,
    source_frame: u64,
    speed: u32,
) -> Vec<u8> {
    let target = AnimationTarget::Clip(clip);
    let frame = TimelineFrameIndex::new(timeline_frame);
    let clip_says = animated
        .placement_on_the_picture(animated.placement_of(clip).unwrap().clip)
        .unwrap();
    let mut placement = [PropertyId::Position, PropertyId::Size, PropertyId::Rotation]
        .into_iter()
        .filter(|property| animated.is_animated(target, *property))
        .fold(clip_says, |placement, property| {
            let value = animated.property_value_at(target, property, frame).unwrap();
            with_placement_value(placement, property, &value)
        });
    if animated.is_animated(target, PropertyId::Rotation) {
        // (The export turns things that keyframes turn in steps of a tenth of a degree.)
        placement.rotation = placement.rotation.snapped_to(0.1);
    }
    let video = counter_video(toolchain);
    let mut project = project_with_tracks_of_videos(
        toolchain,
        &[&video],
        &[(
            0,
            &[ClipOfTheVideo {
                gap_before_frames: 0,
                first_source_frame: source_frame,
                end_source_frame: source_frame + u64::from(speed),
                speed,
            }],
        )],
    );
    let only_clip = clip_number(&project, 0);
    project.set_clip_placement(only_clip, placement).unwrap();
    let scratch = scratch_directory();
    let destination = scratch.path().join("without_keyframes.mp4");
    export(toolchain, &project, &destination);
    let decoded = decode_video(toolchain, &destination, WIDTH as u32, HEIGHT as u32);
    assert_eq!(decoded.pictures.len(), 1);
    decoded.pictures.into_iter().next().unwrap()
}

/// How different two pictures are: the mean difference between a pixel's colours in them, out of 255.
fn mean_difference(first: &[u8], second: &[u8]) -> f64 {
    assert_eq!(first.len(), second.len());
    first
        .iter()
        .zip(second)
        .map(|(a, b)| f64::from(a.abs_diff(*b)))
        .sum::<f64>()
        / first.len() as f64
}

/// The most that two pictures that are the same, made twice by the encoder, may differ by (see [`mean_difference`]).
const SAME_PICTURE: f64 = 1.5;

fn assert_close(actual: usize, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual as f64 - expected).abs() <= tolerance,
        "{what}: {actual}, expected {expected} (give or take {tolerance})"
    );
}

#[test]
fn a_video_that_moves_is_exported_where_its_keyframes_say_at_every_frame() {
    let toolchain = toolchain_or_skip!();
    // 160x90 pixels at the top left of the 320x180 picture, whose middle goes across to the top right by frame 59.
    let (mut project, clip) = project_with_a_video_at(&toolchain, CanvasArea::new(0.0, 0.0, 0.5, 0.5));
    keyframe(
        &mut project,
        clip,
        PropertyId::Position,
        0,
        PropertyValue::Pair(0.25, 0.25),
    );
    keyframe(
        &mut project,
        clip,
        PropertyId::Position,
        59,
        PropertyValue::Pair(0.75, 0.25),
    );
    let scratch = scratch_directory();
    let destination = scratch.path().join("moving.mp4");

    export(&toolchain, &project, &destination);

    let decoded = decode_video(&toolchain, &destination, WIDTH as u32, HEIGHT as u32);
    assert_eq!(decoded.pictures.len(), 60);
    for (timeline_frame, picture) in decoded.pictures.iter().enumerate() {
        let (left, _, right, bottom) = box_of_what_is_not_black(picture);
        // (The place is in whole pixels, and has to be an even one: both bring it up to two pixels out.)
        let expected_left = 160.0 * timeline_frame as f64 / 59.0;
        assert_close(
            left,
            expected_left,
            2.0,
            &format!("frame {timeline_frame}: the left edge"),
        );
        assert_close(
            right - left + 1,
            160.0,
            2.0,
            &format!("frame {timeline_frame}: how wide"),
        );
        assert_close(bottom, 89.0, 2.0, &format!("frame {timeline_frame}: the bottom edge"));
    }
}

#[test]
fn a_video_that_is_turned_by_keyframes_is_exported_as_far_turned_as_they_say() {
    let toolchain = toolchain_or_skip!();
    // 160x90 pixels in the middle of the picture, turned a quarter of a turn by frame 59.
    let (mut project, clip) = project_with_a_video_at(&toolchain, CanvasArea::new(0.25, 0.25, 0.5, 0.5));
    keyframe(&mut project, clip, PropertyId::Rotation, 0, PropertyValue::Number(0.0));
    keyframe(
        &mut project,
        clip,
        PropertyId::Rotation,
        59,
        PropertyValue::Number(90.0),
    );
    let scratch = scratch_directory();
    let destination = scratch.path().join("turning.mp4");

    export(&toolchain, &project, &destination);

    let decoded = decode_video(&toolchain, &destination, WIDTH as u32, HEIGHT as u32);
    assert_eq!(decoded.pictures.len(), 60);
    // Each picture is the one that is made with no keyframes, by turning the video that far.
    for timeline_frame in [0, 1, 15, 30, 44, 59] {
        let without_keyframes = the_picture_made_without_keyframes(&toolchain, &project, clip, timeline_frame as u64);
        let difference = mean_difference(&decoded.pictures[timeline_frame], &without_keyframes);
        assert!(
            difference < SAME_PICTURE,
            "frame {timeline_frame}: the pictures differ by {difference}"
        );
    }
    // (And it is turned: the last picture is not the upright one.)
    let (never_turned, its_clip) = project_with_a_video_at(&toolchain, CanvasArea::new(0.25, 0.25, 0.5, 0.5));
    let upright = the_picture_made_without_keyframes(&toolchain, &never_turned, its_clip, 59);
    assert!(
        mean_difference(&decoded.pictures[59], &upright) > 8.0,
        "the last picture is the video turned a quarter of a turn, not as it was"
    );
}

#[test]
fn a_video_that_is_sped_up_and_moves_shows_the_pictures_a_sped_up_clip_shows_where_the_keyframes_say() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    // 180 pictures of the video at 3x are 60 frames, and the video is in the top left corner at a quarter of the picture.
    let mut project = project_with_tracks_of_videos(
        &toolchain,
        &[&video],
        &[(
            0,
            &[ClipOfTheVideo {
                gap_before_frames: 0,
                first_source_frame: 0,
                end_source_frame: 180,
                speed: 3,
            }],
        )],
    );
    let clip = clip_number(&project, 0);
    project
        .set_clip_placement(clip, Placement::upright(CanvasArea::new(0.0, 0.0, 0.5, 0.5)))
        .unwrap();
    assert_eq!(project.total_frame_count(), 60);
    keyframe(
        &mut project,
        clip,
        PropertyId::Position,
        0,
        PropertyValue::Pair(0.25, 0.25),
    );
    keyframe(
        &mut project,
        clip,
        PropertyId::Position,
        59,
        PropertyValue::Pair(0.75, 0.25),
    );
    let scratch = scratch_directory();
    let destination = scratch.path().join("sped_up_and_moving.mp4");

    export(&toolchain, &project, &destination);

    let decoded = decode_video(&toolchain, &destination, WIDTH as u32, HEIGHT as u32);
    assert_eq!(decoded.pictures.len(), 60);
    for timeline_frame in [0usize, 1, 7, 20, 41, 59] {
        // The picture that frame has is the video's picture number three times the frame's.
        let without_keyframes = the_picture_of_a_source_frame_made_without_keyframes(
            &toolchain,
            &project,
            clip,
            timeline_frame as u64,
            3 * timeline_frame as u64,
            3,
        );
        let difference = mean_difference(&decoded.pictures[timeline_frame], &without_keyframes);
        assert!(
            difference < SAME_PICTURE,
            "frame {timeline_frame}: the pictures differ by {difference}"
        );
    }
}

#[test]
fn a_video_whose_size_is_animated_is_exported_as_big_as_its_keyframes_say() {
    let toolchain = toolchain_or_skip!();
    // 160x90 pixels in the middle of the picture, getting to 80x45 by frame 59 and staying round the same middle.
    let (mut project, clip) = project_with_a_video_at(&toolchain, CanvasArea::new(0.25, 0.25, 0.5, 0.5));
    keyframe(&mut project, clip, PropertyId::Size, 0, PropertyValue::Pair(0.5, 0.5));
    keyframe(
        &mut project,
        clip,
        PropertyId::Size,
        59,
        PropertyValue::Pair(0.25, 0.25),
    );
    let scratch = scratch_directory();
    let destination = scratch.path().join("shrinking.mp4");

    export(&toolchain, &project, &destination);

    let decoded = decode_video(&toolchain, &destination, WIDTH as u32, HEIGHT as u32);
    assert_eq!(decoded.pictures.len(), 60);
    for timeline_frame in [0, 15, 30, 45, 59] {
        let (left, _, right, bottom) = box_of_what_is_not_black(&decoded.pictures[timeline_frame]);
        let expected_width = 160.0 - 80.0 * timeline_frame as f64 / 59.0;
        let expected_height = 90.0 - 45.0 * timeline_frame as f64 / 59.0;
        assert_close(
            right - left + 1,
            expected_width,
            3.0,
            &format!("frame {timeline_frame}: how wide"),
        );
        assert_close(
            (left + right) / 2,
            160.0,
            3.0,
            &format!("frame {timeline_frame}: the middle across"),
        );
        assert_close(
            bottom,
            90.0 + expected_height / 2.0,
            3.0,
            &format!("frame {timeline_frame}: the bottom edge"),
        );
    }
}

#[test]
fn text_that_moves_and_changes_colour_is_exported_as_it_is_at_each_frame_over_a_video_that_goes_on_unbroken() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut project = project_with_tracks_of_videos(
        &toolchain,
        &[&video],
        &[(
            0,
            &[ClipOfTheVideo {
                gap_before_frames: 0,
                first_source_frame: 0,
                end_source_frame: 90,
                speed: 1,
            }],
        )],
    );
    // A box of 64x36 pixels, black at frame 20 and white at frame 60, which goes from a third of the way across to
    // two thirds of the way across between frames 20 and 69.
    let front = project.add_track_in_front();
    let object = TextObject {
        area: CanvasArea::new(0.2, 0.4, 0.2, 0.2),
        colour: TextColour::new(0, 0, 0, 255),
        ..TextObject::default()
    };
    let (text, _) = project
        .insert_text(front, TimelineFrameIndex::new(20), object, 50)
        .expect("there is room");
    let colour = |level: u8| PropertyValue::Colour(TextColour::new(level, level, level, 255));
    keyframe(&mut project, text, PropertyId::Colour, 20, colour(0));
    keyframe(&mut project, text, PropertyId::Colour, 60, colour(255));
    keyframe(
        &mut project,
        text,
        PropertyId::Position,
        20,
        PropertyValue::Pair(0.3, 0.5),
    );
    keyframe(
        &mut project,
        text,
        PropertyId::Position,
        69,
        PropertyValue::Pair(0.7, 0.5),
    );
    let scratch = scratch_directory();
    let destination = scratch.path().join("text.mp4");

    export_with_text_pictures(&toolchain, &project, &destination, &solid_text_picture);

    let decoded = decode_video(&toolchain, &destination, WIDTH as u32, HEIGHT as u32);
    assert_eq!(decoded.pictures.len(), 90);
    assert_eq!(
        decoded.frame_numbers(),
        (0..90).collect::<Vec<u64>>(),
        "the video under the text is every picture once, in order, however the text is cut into stretches"
    );
    for timeline_frame in [20usize, 30, 45, 60, 69] {
        let middle_across = 320.0 * (0.3 + 0.4 * (timeline_frame as f64 - 20.0) / 49.0);
        let expected_level = (255.0 * (timeline_frame as f64 - 20.0) / 40.0).clamp(0.0, 255.0);
        let level = brightness_at(&decoded.pictures[timeline_frame], middle_across as usize, 90);
        assert!(
            (f64::from(level) - expected_level).abs() <= 14.0,
            "frame {timeline_frame}: the text at x = {middle_across} is {level} bright, expected {expected_level}"
        );
    }
    // Before and after, there is no text there, just the video.
    assert!((f64::from(brightness_at(&decoded.pictures[10], 96, 90)) - 90.0).abs() <= 14.0);
    assert!((f64::from(brightness_at(&decoded.pictures[80], 224, 90)) - 90.0).abs() <= 14.0);
}
