// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Exporting projects whose clips and tracks have effects with the real ffmpeg, checked colour by colour against what the effect
//! says: a hue shift of a flat colour comes out as the colour that the arithmetic of the effect says, whether the effect is of the
//! clip or of the track, and for a picture, a text object, and a value that has keyframes; and the hue shift of a group, of each of the three
//! kinds, which is done to the picture of each of the clips that are in it (see the placeholder in `project::groups::effects_of_groups`).

mod support;

use std::path::Path;

use fastforwardrs::colour_effects::ColourMatrix;
use fastforwardrs::media::{FfmpegToolchain, probe_source_media};
use fastforwardrs::project::{
    AnimationTarget, ClipId, EffectInstanceId, EffectKind, GroupId, GroupKind, Member, Project, PropertyId,
    PropertyValue, SourceMedia, SourceMediaIdentifier, SourceMediaLink, TextColour, TextObject, TimelineFrameIndex,
};
use support::{
    DecodedVideo, decode_video, export, export_with_text_pictures, make_a_keyframe,
    project_with_a_picture_over_a_video, scratch_directory, solid_text_picture,
};

/// The one value of the hue shift, when the hue shift is the first effect of its clip, track or group (as it is in every project here).
const HUE: PropertyId = PropertyId::effect_value(EffectKind::HUE_SHIFT, EffectInstanceId::FIRST, 0);

const WIDTH: usize = 320;
const HEIGHT: usize = 180;

/// How far a colour that came out of ffmpeg may be, in each of its three numbers, from the one that was worked out (ffmpeg changes
/// the colours of a picture that is made of luminance and chroma with whole numbers, and a picture of this size has its chroma
/// at half the resolution).
const ALLOWED_DIFFERENCE: i32 = 10;

/// The orange that the videos are made of. A hue shift of 120 degrees of it is a green, which is well inside the colours there are.
const ORANGE: (u8, u8, u8) = (200, 100, 50);

/// A project of a video that is all of one colour, and the clip it is.
fn project_of_a_flat_colour_video(
    toolchain: &FfmpegToolchain,
    folder: &Path,
    colour: (u8, u8, u8),
) -> (Project, ClipId) {
    let path = support::flat_colour_video(toolchain, folder, colour);
    let properties = probe_source_media(toolchain, &path).expect("the video can be probed");
    let mut project = Project::empty();
    let imported = project
        .import_source(
            SourceMedia {
                identifier: SourceMediaIdentifier::new(1),
                link: SourceMediaLink::from_absolute_path(path),
                properties,
            },
            TimelineFrameIndex::new(0),
        )
        .unwrap();
    let clip = imported.clip.expect("a video makes a clip");
    (project, clip)
}

/// Puts a hue shift on `target`, turned by `degrees`, and gives the value of it as a property (which is `HUE` when it is the first).
fn put_a_hue_shift_on(project: &mut Project, target: AnimationTarget, degrees: f32) -> PropertyId {
    let instance = project.add_effect(target, EffectKind::HUE_SHIFT).unwrap();
    let value = PropertyId::effect_value(EffectKind::HUE_SHIFT, instance, 0);
    project
        .set_property_at(
            target,
            value,
            PropertyValue::Number(degrees),
            TimelineFrameIndex::new(0),
        )
        .unwrap();
    value
}

/// The colour that `colour` becomes when the hue is turned by `degrees`, by the arithmetic of the effect.
fn turned(colour: (u8, u8, u8), degrees: f32) -> (u8, u8, u8) {
    let mut pixel = [colour.0, colour.1, colour.2, 255];
    ColourMatrix::hue_rotation(degrees).apply_to_rgba_pixels(&mut pixel);
    (pixel[0], pixel[1], pixel[2])
}

fn colour_at(picture: &[u8], x: usize, y: usize) -> (u8, u8, u8) {
    let start = (y * WIDTH + x) * 3;
    (picture[start], picture[start + 1], picture[start + 2])
}

fn assert_near(found: (u8, u8, u8), wanted: (u8, u8, u8), when: &str) {
    let differences = [
        i32::from(found.0) - i32::from(wanted.0),
        i32::from(found.1) - i32::from(wanted.1),
        i32::from(found.2) - i32::from(wanted.2),
    ];
    assert!(
        differences
            .iter()
            .all(|difference| difference.abs() <= ALLOWED_DIFFERENCE),
        "{when}: the colour is {found:?} and should be {wanted:?}"
    );
}

fn exported(toolchain: &FfmpegToolchain, project: &Project, folder: &Path, name: &str) -> DecodedVideo {
    let destination = folder.join(name);
    export(toolchain, project, &destination);
    decode_video(toolchain, &destination, WIDTH as u32, HEIGHT as u32)
}

/// Checks that the middle of every picture of `decoded` is `wanted`.
fn assert_every_picture_is(decoded: &DecodedVideo, wanted: (u8, u8, u8), what: &str) {
    assert_eq!(decoded.pictures.len(), 60, "{what}");
    for (number, picture) in decoded.pictures.iter().enumerate() {
        assert_near(
            colour_at(picture, WIDTH / 2, HEIGHT / 2),
            wanted,
            &format!("{what}, frame {number}"),
        );
    }
}

// --- a clip and a track ---

#[test]
fn a_video_with_no_effect_comes_out_the_colour_it_is_which_is_what_the_other_tests_here_are_measured_against() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let (project, _) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);

    let decoded = exported(&toolchain, &project, scratch.path(), "plain.mp4");

    assert_every_picture_is(&decoded, ORANGE, "no effect");
}

#[test]
fn the_hue_shift_of_a_clip_turns_the_colours_of_its_video_as_the_arithmetic_of_the_effect_says() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    for degrees in [120.0, -90.0, 200.0] {
        let (mut project, clip) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
        put_a_hue_shift_on(&mut project, AnimationTarget::Clip(clip), degrees);

        let decoded = exported(&toolchain, &project, scratch.path(), "clip_hue.mp4");

        let wanted = turned(ORANGE, degrees);
        assert_ne!(
            wanted, ORANGE,
            "the colour is not changed by {degrees} degrees: the test would show nothing"
        );
        assert_every_picture_is(&decoded, wanted, &format!("{degrees} degrees"));
    }
}

#[test]
fn a_whole_turn_leaves_the_colours_as_they_were() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let (mut project, clip) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(clip), 360.0);

    let decoded = exported(&toolchain, &project, scratch.path(), "whole_turn.mp4");

    assert_every_picture_is(&decoded, ORANGE, "a whole turn");
}

#[test]
fn the_hue_shift_of_a_track_turns_the_colours_of_the_videos_on_it() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let (mut project, _) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    let track = project.tracks()[0].id();
    put_a_hue_shift_on(&mut project, AnimationTarget::Track(track), 120.0);

    let decoded = exported(&toolchain, &project, scratch.path(), "track_hue.mp4");

    assert_every_picture_is(&decoded, turned(ORANGE, 120.0), "the track turned by 120 degrees");
}

#[test]
fn the_hue_shifts_of_a_clip_and_of_its_track_are_both_done() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let (mut project, clip) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    let track = project.tracks()[0].id();
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(clip), 70.0);
    put_a_hue_shift_on(&mut project, AnimationTarget::Track(track), 50.0);

    let decoded = exported(&toolchain, &project, scratch.path(), "both_hue.mp4");

    assert_every_picture_is(&decoded, turned(ORANGE, 120.0), "70 degrees and then 50");
}

#[test]
fn two_hue_shifts_of_one_clip_are_both_done_and_turn_the_colours_by_both_turns() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let (mut project, clip) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    let first = put_a_hue_shift_on(&mut project, AnimationTarget::Clip(clip), 70.0);
    let second = put_a_hue_shift_on(&mut project, AnimationTarget::Clip(clip), 50.0);
    assert_ne!(first, second, "they are two effects");

    let decoded = exported(&toolchain, &project, scratch.path(), "two_hue_shifts.mp4");

    assert_every_picture_is(&decoded, turned(ORANGE, 120.0), "70 degrees and then 50, of one clip");
}

#[test]
fn the_keyframes_of_the_second_of_two_hue_shifts_turn_the_colours_by_what_the_second_says_on_top_of_the_first() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let (mut project, clip) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    let target = AnimationTarget::Clip(clip);
    put_a_hue_shift_on(&mut project, target, 30.0);
    let second = put_a_hue_shift_on(&mut project, target, 0.0);
    make_a_keyframe(&mut project, target, second, 0, PropertyValue::Number(0.0));
    make_a_keyframe(&mut project, target, second, 59, PropertyValue::Number(177.0));

    let decoded = exported(&toolchain, &project, scratch.path(), "second_hue_shift_animated.mp4");

    assert_eq!(decoded.pictures.len(), 60);
    for (number, picture) in decoded.pictures.iter().enumerate() {
        // (The first is 30 degrees all the time, and the second goes three degrees a frame.)
        let wanted = turned(ORANGE, 30.0 + 3.0 * number as f32);
        assert_near(
            colour_at(picture, WIDTH / 2, HEIGHT / 2),
            wanted,
            &format!("frame {number}"),
        );
    }
}

#[test]
fn only_the_clip_that_has_the_effect_has_its_colours_turned() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let (mut project, first) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    // Cut the video in two at frame 30: the first half has the effect, the second does not.
    project.split_at(TimelineFrameIndex::new(30), None);
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(first), 120.0);

    let decoded = exported(&toolchain, &project, scratch.path(), "one_clip.mp4");

    assert_eq!(decoded.pictures.len(), 60);
    for (number, picture) in decoded.pictures.iter().enumerate() {
        let wanted = if number < 30 { turned(ORANGE, 120.0) } else { ORANGE };
        assert_near(
            colour_at(picture, WIDTH / 2, HEIGHT / 2),
            wanted,
            &format!("frame {number}"),
        );
    }
}

// --- keyframes ---

#[test]
fn a_hue_shift_with_keyframes_turns_the_colours_a_little_more_at_each_frame() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let (mut project, clip) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    let target = AnimationTarget::Clip(clip);
    project.add_effect(target, EffectKind::HUE_SHIFT).unwrap();
    make_a_keyframe(&mut project, target, HUE, 0, PropertyValue::Number(0.0));
    make_a_keyframe(&mut project, target, HUE, 59, PropertyValue::Number(177.0));

    let decoded = exported(&toolchain, &project, scratch.path(), "animated_hue.mp4");

    assert_eq!(decoded.pictures.len(), 60);
    for (number, picture) in decoded.pictures.iter().enumerate() {
        // (Three degrees a frame, which is a whole number of degrees, as the export has it.)
        let wanted = turned(ORANGE, 3.0 * number as f32);
        assert_near(
            colour_at(picture, WIDTH / 2, HEIGHT / 2),
            wanted,
            &format!("frame {number}"),
        );
    }
}

// --- what is laid over the video ---

#[test]
fn a_picture_file_with_a_hue_shift_has_each_of_its_colours_turned_and_the_video_behind_it_is_not() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let (mut project, picture) = project_with_a_picture_over_a_video(&toolchain, scratch.path(), 0, 60);
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(picture), 120.0);

    let decoded = exported(&toolchain, &project, scratch.path(), "picture_hue.mp4");

    // (The picture is twice as wide as it is tall: it is 320 by 160, with 10 rows of black above it and below it.)
    for number in [0, 30, 59] {
        let picture = &decoded.pictures[number];
        assert_near(
            colour_at(picture, WIDTH / 4, HEIGHT / 2),
            turned((255, 0, 0), 120.0),
            &format!("the red half, frame {number}"),
        );
        assert_near(
            colour_at(picture, 3 * WIDTH / 4, HEIGHT / 2),
            turned((0, 0, 255), 120.0),
            &format!("the blue half, frame {number}"),
        );
    }
}

#[test]
fn a_text_object_with_a_hue_shift_has_its_colour_turned() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let (mut project, _) = project_of_a_flat_colour_video(&toolchain, scratch.path(), (20, 20, 20));
    let front = project.add_track_in_front();
    let object = TextObject {
        colour: TextColour::new(ORANGE.0, ORANGE.1, ORANGE.2, 255),
        ..TextObject::default()
    };
    let (text, _) = project
        .insert_text(front, TimelineFrameIndex::new(0), object.clone(), 60)
        .unwrap();
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(text), 120.0);
    let middle_of_the_text = (
        ((object.area.left() + object.area.width() / 2.0) * WIDTH as f32) as usize,
        ((object.area.top() + object.area.height() / 2.0) * HEIGHT as f32) as usize,
    );
    let destination = scratch.path().join("text_hue.mp4");

    export_with_text_pictures(&toolchain, &project, &destination, &solid_text_picture);
    let decoded = decode_video(&toolchain, &destination, WIDTH as u32, HEIGHT as u32);

    assert_eq!(decoded.pictures.len(), 60);
    for number in [0, 30, 59] {
        assert_near(
            colour_at(&decoded.pictures[number], middle_of_the_text.0, middle_of_the_text.1),
            turned(ORANGE, 120.0),
            &format!("the text, frame {number}"),
        );
        assert_near(
            colour_at(&decoded.pictures[number], 4, 4),
            (20, 20, 20),
            &format!("the video behind it, frame {number}"),
        );
    }
}

/// What stands in for the picture of a text object when the text is only half there (the real renderer needs fonts): its area filled
/// with the colour of the text, with half of it clear.
fn half_clear_text_picture(
    object: &TextObject,
    picture_width_in_pixels: u32,
    picture_height_in_pixels: u32,
) -> fastforwardrs::rgba_picture::RgbaPicture {
    let (width, height) =
        fastforwardrs::text_rendering::area_size_in_pixels(object, picture_width_in_pixels, picture_height_in_pixels);
    let colour = object.colour;
    fastforwardrs::rgba_picture::RgbaPicture {
        width_in_pixels: width,
        height_in_pixels: height,
        rgba_pixels: [colour.red, colour.green, colour.blue, 128].repeat((width * height) as usize),
    }
}

#[test]
fn text_that_is_half_clear_has_its_colour_turned_and_is_still_half_clear() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let (mut project, _) = project_of_a_flat_colour_video(&toolchain, scratch.path(), (20, 20, 20));
    let front = project.add_track_in_front();
    let object = TextObject {
        colour: TextColour::new(ORANGE.0, ORANGE.1, ORANGE.2, 255),
        ..TextObject::default()
    };
    let (text, _) = project
        .insert_text(front, TimelineFrameIndex::new(0), object.clone(), 60)
        .unwrap();
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(text), 120.0);
    let middle_of_the_text = (
        ((object.area.left() + object.area.width() / 2.0) * WIDTH as f32) as usize,
        ((object.area.top() + object.area.height() / 2.0) * HEIGHT as f32) as usize,
    );
    let destination = scratch.path().join("half_clear_text_hue.mp4");

    export_with_text_pictures(&toolchain, &project, &destination, &half_clear_text_picture);
    let decoded = decode_video(&toolchain, &destination, WIDTH as u32, HEIGHT as u32);

    // Half the turned colour (18, 151, 80) over the video behind it (20, 20, 20).
    let wanted = (19, 85, 50);
    for number in [0, 30, 59] {
        assert_near(
            colour_at(&decoded.pictures[number], middle_of_the_text.0, middle_of_the_text.1),
            wanted,
            &format!("the text, frame {number}"),
        );
    }
}

// --- groups ---

fn put_a_hue_shift_on_the_group(project: &mut Project, group: GroupId, degrees: f32) {
    project.add_effect_to_the_group(group, EffectKind::HUE_SHIFT).unwrap();
    project
        .set_effect_value_of_the_group(group, HUE, &PropertyValue::Number(degrees))
        .unwrap();
}

/// A project of a video that is all of one colour, cut in two at frame 30, and the clips that are the first half and the second.
fn project_of_a_flat_colour_video_cut_in_two(toolchain: &FfmpegToolchain, folder: &Path) -> (Project, ClipId, ClipId) {
    let (mut project, first) = project_of_a_flat_colour_video(toolchain, folder, ORANGE);
    project.split_at(TimelineFrameIndex::new(30), None);
    let second = project.tracks()[0].entries()[1].id();
    (project, first, second)
}

#[test]
fn the_hue_shift_of_a_tag_turns_the_colours_of_the_clips_in_it_and_of_no_other() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let (mut project, first, _) = project_of_a_flat_colour_video_cut_in_two(&toolchain, scratch.path());
    let tag = project.create_group(GroupKind::Tag, &[Member::Clip(first)]).unwrap();
    put_a_hue_shift_on_the_group(&mut project, tag, 120.0);

    let decoded = exported(&toolchain, &project, scratch.path(), "tag_hue.mp4");

    assert_eq!(decoded.pictures.len(), 60);
    for (number, picture) in decoded.pictures.iter().enumerate() {
        let wanted = if number < 30 { turned(ORANGE, 120.0) } else { ORANGE };
        assert_near(
            colour_at(picture, WIDTH / 2, HEIGHT / 2),
            wanted,
            &format!("frame {number}"),
        );
    }
}

#[test]
fn the_hue_shift_of_a_clip_group_turns_the_colours_of_the_clips_in_it() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let (mut project, first, second) = project_of_a_flat_colour_video_cut_in_two(&toolchain, scratch.path());
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    put_a_hue_shift_on_the_group(&mut project, group, 90.0);

    let decoded = exported(&toolchain, &project, scratch.path(), "clip_group_hue.mp4");

    assert_every_picture_is(&decoded, turned(ORANGE, 90.0), "the clip group turned by 90 degrees");
}

#[test]
fn the_hue_shift_of_a_track_group_turns_the_colours_of_the_clips_on_its_tracks() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let (mut project, _, _) = project_of_a_flat_colour_video_cut_in_two(&toolchain, scratch.path());
    let track = project.tracks()[0].id();
    let group = project
        .create_group(GroupKind::TrackGroup, &[Member::Track(track)])
        .unwrap();
    put_a_hue_shift_on_the_group(&mut project, group, 150.0);

    let decoded = exported(&toolchain, &project, scratch.path(), "track_group_hue.mp4");

    assert_every_picture_is(&decoded, turned(ORANGE, 150.0), "the track group turned by 150 degrees");
}

#[test]
fn the_effects_of_a_clip_its_track_a_tag_a_clip_group_and_a_track_group_are_all_done() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    let (mut project, first, second) = project_of_a_flat_colour_video_cut_in_two(&toolchain, scratch.path());
    let track = project.tracks()[0].id();
    let tag = project
        .create_group(GroupKind::Tag, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    let clip_group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    let track_group = project
        .create_group(GroupKind::TrackGroup, &[Member::Track(track)])
        .unwrap();
    // (Turns of the colours add up, so how far they turn it in all is the sum, whatever order they are done in: it is that all five are done that
    // this shows.)
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(first), 10.0);
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(second), 10.0);
    put_a_hue_shift_on(&mut project, AnimationTarget::Track(track), 20.0);
    put_a_hue_shift_on_the_group(&mut project, tag, 30.0);
    put_a_hue_shift_on_the_group(&mut project, clip_group, 40.0);
    put_a_hue_shift_on_the_group(&mut project, track_group, 50.0);

    let decoded = exported(&toolchain, &project, scratch.path(), "everything_hue.mp4");

    assert_every_picture_is(
        &decoded,
        turned(ORANGE, 150.0),
        "10 and 20 and 30 and 40 and 50 degrees",
    );
}
