// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Exporting projects whose clips and tracks have effects with the real ffmpeg, checked colour by colour against what the effect
//! says: a hue shift of a flat colour comes out as the colour that the arithmetic of the effect says, whether the effect is of the
//! clip or of the track, and for a picture, a text object, and a value that has keyframes; and the hue shift of a group, of each of the three
//! kinds, which is done to the picture that the clips in it make together (phase 5: it was done to the picture of each clip as a placeholder before), and of an adjustment clip,
//! which is done to everything behind it for as long as it lasts.

mod support;

use std::path::Path;

use fastforwardrs::colour_effects::ColourMatrix;
use fastforwardrs::media::{FfmpegToolchain, probe_source_media};
use fastforwardrs::project::{
    AnimationTarget, ClipId, EffectInstanceId, EffectKind, GroupId, GroupKind, InputColourOverride, Member, Project,
    PropertyId, PropertyValue, SourceMedia, SourceMediaIdentifier, SourceMediaLink, TextColour, TextObject,
    TimelineFrameIndex,
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
                colour: InputColourOverride::AUTOMATIC,
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
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let (project, _) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);

    let decoded = exported(&toolchain, &project, scratch.path(), "plain.mp4");

    assert_every_picture_is(&decoded, ORANGE, "no effect");
}

#[test]
fn the_hue_shift_of_a_clip_turns_the_colours_of_its_video_as_the_arithmetic_of_the_effect_says() {
    let toolchain = toolchain_and_graphics_or_skip!();
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
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let (mut project, clip) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(clip), 360.0);

    let decoded = exported(&toolchain, &project, scratch.path(), "whole_turn.mp4");

    assert_every_picture_is(&decoded, ORANGE, "a whole turn");
}

#[test]
fn the_hue_shift_of_a_track_turns_the_colours_of_the_videos_on_it() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let (mut project, _) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    let track = project.tracks()[0].id();
    put_a_hue_shift_on(&mut project, AnimationTarget::Track(track), 120.0);

    let decoded = exported(&toolchain, &project, scratch.path(), "track_hue.mp4");

    assert_every_picture_is(&decoded, turned(ORANGE, 120.0), "the track turned by 120 degrees");
}

#[test]
fn the_hue_shifts_of_a_clip_and_of_its_track_are_both_done() {
    let toolchain = toolchain_and_graphics_or_skip!();
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
    let toolchain = toolchain_and_graphics_or_skip!();
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
    let toolchain = toolchain_and_graphics_or_skip!();
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
    let toolchain = toolchain_and_graphics_or_skip!();
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
    let toolchain = toolchain_and_graphics_or_skip!();
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
    let toolchain = toolchain_and_graphics_or_skip!();
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
    let toolchain = toolchain_and_graphics_or_skip!();
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

    export_with_text_pictures(&toolchain, &project, &destination, solid_text_picture);
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
    let toolchain = toolchain_and_graphics_or_skip!();
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

    export_with_text_pictures(&toolchain, &project, &destination, half_clear_text_picture);
    let decoded = decode_video(&toolchain, &destination, WIDTH as u32, HEIGHT as u32);

    // Half the turned colour (18, 151, 80) over the video behind it (20, 20, 20), **mixed as light is mixed** (rewritten on purpose in phase 3, when the layers began to be
    // put together in linear light: mixed as codes the same pixel was (19, 85, 50); the half of the light that the text makes is a brighter picture, and the green of the
    // text is what lifts it). The text is 128 of 255 clear.
    let wanted = mixed_in_light((18, 151, 80), (20, 20, 20), 128.0 / 255.0);
    assert_eq!(wanted, (19, 111, 59), "a premise: the arithmetic of the standards");
    for number in [0, 30, 59] {
        assert_near(
            colour_at(&decoded.pictures[number], middle_of_the_text.0, middle_of_the_text.1),
            wanted,
            &format!("the text, frame {number}"),
        );
    }
}

/// The colour (as sRGB codes) that `over` is when it is laid over `under` with `opacity`, if the mixing is of light and not of codes: each is made light with the curve of
/// sRGB, the lights are mixed, and the mean is made codes again.
fn mixed_in_light(over: (u8, u8, u8), under: (u8, u8, u8), opacity: f64) -> (u8, u8, u8) {
    use fastforwardrs::colour_space::TransferFunction::Srgb;
    let mixed = |over: u8, under: u8| {
        let light = opacity * Srgb.decode_to_light(f64::from(over) / 255.0)
            + (1.0 - opacity) * Srgb.decode_to_light(f64::from(under) / 255.0);
        (Srgb.encode_from_light(light) * 255.0).round() as u8
    };
    (mixed(over.0, under.0), mixed(over.1, under.1), mixed(over.2, under.2))
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
    let toolchain = toolchain_and_graphics_or_skip!();
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
    let toolchain = toolchain_and_graphics_or_skip!();
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
    let toolchain = toolchain_and_graphics_or_skip!();
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
    let toolchain = toolchain_and_graphics_or_skip!();
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

// --- a look-up table ---

/// The text of a 3-D table by formula: the warm, curved look of the sample tables.
fn a_warm_grade() -> String {
    let size = 17;
    let mut text = String::from("TITLE \"a warm grade\"\nLUT_3D_SIZE 17\n");
    for blue in 0..size {
        for green in 0..size {
            for red in 0..size {
                let (r, g, b) = (red as f32 / 16.0, green as f32 / 16.0, blue as f32 / 16.0);
                let (r, g, b) = (
                    r.powf(0.8) * 1.05,
                    g.powf(1.1) * 0.95 + 0.02 * r,
                    (b * 0.9 + 0.1 * g).powf(1.3),
                );
                text.push_str(&format!("{r:.6} {g:.6} {b:.6}\n"));
            }
        }
    }
    text
}

/// The colour that the table makes of `colour`, as the reference look-up on the processor does it (the codes of a screen in, the codes of a screen out).
fn looked_up(table: &fastforwardrs::cube_table::CubeTable, colour: (u8, u8, u8)) -> (u8, u8, u8) {
    let out = table.look_up(
        [colour.0, colour.1, colour.2].map(|channel| f32::from(channel) / 255.0),
        fastforwardrs::cube_table::Interpolation::Tetrahedral,
    );
    let byte = |channel: f32| (channel * 255.0).round().clamp(0.0, 255.0) as u8;
    (byte(out[0]), byte(out[1]), byte(out[2]))
}

/// Puts the look-up table effect on `target`, with `table_name` (the project has the resource of that name), and gives nothing else of it changed.
fn put_a_look_up_table_on(project: &mut Project, target: AnimationTarget, table_name: &str) {
    let instance = project.add_effect(target, EffectKind::LUT).unwrap();
    project
        .set_property_at(
            target,
            PropertyId::effect_value(EffectKind::LUT, instance, 0),
            PropertyValue::Words(table_name.to_owned()),
            TimelineFrameIndex::new(0),
        )
        .unwrap();
}

fn add_the_warm_grade_to(project: &mut Project, link: SourceMediaLink) -> String {
    project
        .add_resource(
            "warm.cube",
            fastforwardrs::project::ResourceKind::Cube,
            std::sync::Arc::from(a_warm_grade().as_bytes()),
            link,
        )
        .unwrap()
}

#[test]
fn the_look_up_table_of_a_clip_gives_the_exported_colours_the_table_says() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let table = fastforwardrs::cube_table::parse_cube(&a_warm_grade()).unwrap();
    let (mut project, clip) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    let name = add_the_warm_grade_to(
        &mut project,
        SourceMediaLink::from_absolute_path(scratch.path().join("warm.cube")),
    );
    put_a_look_up_table_on(&mut project, AnimationTarget::Clip(clip), &name);

    let decoded = exported(&toolchain, &project, scratch.path(), "lut.mp4");

    let wanted = looked_up(&table, ORANGE);
    assert_ne!(
        wanted, ORANGE,
        "the table does not change the colour: the test would show nothing"
    );
    assert_every_picture_is(&decoded, wanted, "the warm grade");
}

#[test]
fn a_look_up_table_that_is_embedded_in_a_project_file_that_is_saved_and_opened_again_still_gives_the_colours_the_table_says()
 {
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let table = fastforwardrs::cube_table::parse_cube(&a_warm_grade()).unwrap();
    let (mut project, clip) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    let bytes = a_warm_grade().into_bytes();
    let link = fastforwardrs::project_file::link_of_a_new_resource(
        &scratch.path().join("warm.cube"),
        "warm.cube",
        &bytes,
        true,
    );
    let name = add_the_warm_grade_to(&mut project, link);
    put_a_look_up_table_on(&mut project, AnimationTarget::Clip(clip), &name);
    let saved_as = scratch.path().join("graded.ffrs");
    fastforwardrs::project_file::write_project_file(&project, &saved_as).unwrap();
    // (The table was never a file: it is in the project file and nowhere else.)
    assert!(!scratch.path().join("warm.cube").exists());

    let opened = fastforwardrs::project_file::read_project_file(&saved_as).unwrap();
    assert!(
        opened
            .resources()
            .get("warm.cube")
            .is_some_and(|resource| resource.is_included())
    );
    let decoded = exported(&toolchain, &opened, scratch.path(), "embedded.mp4");

    assert_every_picture_is(&decoded, looked_up(&table, ORANGE), "the embedded table");
}

#[test]
fn a_look_up_table_that_is_missing_leaves_the_video_as_it_is_and_a_table_that_is_not_one_does_too() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    // A table that the project does not have.
    let (mut project, clip) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    put_a_look_up_table_on(&mut project, AnimationTarget::Clip(clip), "not there.cube");
    let decoded = exported(&toolchain, &project, scratch.path(), "no_table.mp4");
    assert_every_picture_is(&decoded, ORANGE, "a table that the project has not got");

    // A file that is not a table.
    let (mut project, clip) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    project
        .add_resource(
            "bad.cube",
            fastforwardrs::project::ResourceKind::Cube,
            std::sync::Arc::from(&b"this is not a table\n"[..]),
            SourceMediaLink::from_absolute_path(scratch.path().join("bad.cube")),
        )
        .unwrap();
    put_a_look_up_table_on(&mut project, AnimationTarget::Clip(clip), "bad.cube");
    let decoded = exported(&toolchain, &project, scratch.path(), "bad_table.mp4");
    assert_every_picture_is(&decoded, ORANGE, "a file that is not a table");
}

#[test]
fn the_look_up_table_of_a_track_and_of_a_tag_gives_the_colours_of_the_table_to_the_videos_they_have() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let table = fastforwardrs::cube_table::parse_cube(&a_warm_grade()).unwrap();

    let (mut project, _) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    let name = add_the_warm_grade_to(
        &mut project,
        SourceMediaLink::from_absolute_path(scratch.path().join("warm.cube")),
    );
    let track = project.tracks()[0].id();
    put_a_look_up_table_on(&mut project, AnimationTarget::Track(track), &name);
    let decoded = exported(&toolchain, &project, scratch.path(), "track_lut.mp4");
    assert_every_picture_is(&decoded, looked_up(&table, ORANGE), "the table of a track");

    let (mut project, clip) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    let name = add_the_warm_grade_to(
        &mut project,
        SourceMediaLink::from_absolute_path(scratch.path().join("warm.cube")),
    );
    let tag = project.create_group(GroupKind::Tag, &[Member::Clip(clip)]).unwrap();
    let instance = project.add_effect_to_the_group(tag, EffectKind::LUT).unwrap();
    project
        .set_effect_value_of_the_group(
            tag,
            PropertyId::effect_value(EffectKind::LUT, instance, 0),
            &PropertyValue::Words(name),
        )
        .unwrap();
    let decoded = exported(&toolchain, &project, scratch.path(), "tag_lut.mp4");
    assert_every_picture_is(&decoded, looked_up(&table, ORANGE), "the table of a tag");
}

// --- an adjustment clip (phase 5) ---

#[test]
fn an_adjustment_clip_turns_the_colours_of_everything_behind_it_for_as_long_as_it_lasts_and_not_before_or_after() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let (mut project, _) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    let front = project.add_track_in_front();
    let (adjustment, lasts) = project
        .try_insert_adjustment_clip(front, TimelineFrameIndex::new(20), 20)
        .unwrap();
    assert_eq!(lasts, 20);
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(adjustment), 120.0);

    let decoded = exported(&toolchain, &project, scratch.path(), "adjusted.mp4");

    assert_eq!(decoded.pictures.len(), 60);
    for (number, picture) in decoded.pictures.iter().enumerate() {
        let wanted = if (20..40).contains(&number) {
            turned(ORANGE, 120.0)
        } else {
            ORANGE
        };
        assert_near(
            colour_at(picture, WIDTH / 2, HEIGHT / 2),
            wanted,
            &format!("frame {number}"),
        );
    }
}

#[test]
fn two_adjustment_clips_one_over_the_other_turn_the_colours_by_both_turns() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let (mut project, _) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    for degrees in [60.0, 70.0] {
        let track = project.add_track_in_front();
        let (adjustment, _) = project
            .try_insert_adjustment_clip(track, TimelineFrameIndex::new(0), 60)
            .unwrap();
        put_a_hue_shift_on(&mut project, AnimationTarget::Clip(adjustment), degrees);
    }

    let decoded = exported(&toolchain, &project, scratch.path(), "two_adjustments.mp4");

    assert_every_picture_is(&decoded, turned(ORANGE, 130.0), "turned by 60 and then by 70 degrees");
}

// --- an ISF shader (phase 6) ---

/// Makes the kind of effect that the ISF file `source` called `name` is, and puts one of it on `target`.
fn put_a_shader_on(project: &mut Project, target: AnimationTarget, name: &str, source: &str) -> EffectInstanceId {
    let file = fastforwardrs::isf::read(source).unwrap_or_else(|problem| panic!("{problem}"));
    let kind = EffectKind::from_snapshot(fastforwardrs::isf::descriptor_of(&file, name, source)).unwrap();
    assert!(
        kind.is_installed(),
        "{:?}",
        fastforwardrs::isf::why_not_usable(kind.descriptor())
    );
    project.add_effect(target, kind).unwrap()
}

const TINT_SHADER: &str = r#"/*{
	"DESCRIPTION": "mixes the picture with a colour",
	"INPUTS": [
		{ "NAME": "inputImage", "TYPE": "image" },
		{ "NAME": "tint", "TYPE": "color", "DEFAULT": [1.0, 0.0, 0.0, 1.0] },
		{ "NAME": "amount", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 }
	]
}*/
void main() {
	vec4 c = IMG_THIS_NORM_PIXEL(inputImage);
	gl_FragColor = vec4(mix(c.rgb, tint.rgb, amount), c.a);
}
"#;

#[test]
fn a_shader_that_mixes_the_picture_with_a_colour_gives_the_colour_its_arithmetic_says_in_the_codes_of_the_screen() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let (mut project, clip) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    put_a_shader_on(&mut project, AnimationTarget::Clip(clip), "tint.fs", TINT_SHADER);

    let decoded = exported(&toolchain, &project, scratch.path(), "shader_tint.mp4");

    // Half way from orange (200, 100, 50) to red (255, 0, 0): shaders work on the codes of a screen.
    assert_every_picture_is(&decoded, (228, 50, 25), "orange, half red");
}

#[test]
fn the_values_of_a_shader_are_the_values_of_its_effect_and_keyframes_of_them_change_the_picture() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let (mut project, clip) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    let target = AnimationTarget::Clip(clip);
    let instance = put_a_shader_on(&mut project, target, "tint.fs", TINT_SHADER);
    let kind = project.effects_of(target).unwrap().instance(instance).unwrap().kind();
    let amount = PropertyId::effect_value(kind, instance, 1);
    make_a_keyframe(&mut project, target, amount, 0, PropertyValue::Number(0.0));
    make_a_keyframe(&mut project, target, amount, 59, PropertyValue::Number(1.0));

    let decoded = exported(&toolchain, &project, scratch.path(), "shader_amount.mp4");

    assert_eq!(decoded.pictures.len(), 60);
    assert_near(
        colour_at(&decoded.pictures[0], WIDTH / 2, HEIGHT / 2),
        ORANGE,
        "none of the colour at the first keyframe",
    );
    assert_near(
        colour_at(&decoded.pictures[59], WIDTH / 2, HEIGHT / 2),
        (255, 0, 0),
        "all of it at the last",
    );
    let halfway = colour_at(&decoded.pictures[30], WIDTH / 2, HEIGHT / 2);
    assert!(
        (225..=240).contains(&halfway.0) && (35..=65).contains(&halfway.1),
        "{halfway:?}"
    );
}

#[test]
fn time_in_a_shader_is_the_time_of_the_clip_and_render_size_is_the_size_of_the_picture() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let (mut project, clip) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    // Red that is TIME over two (a clip of 60 frames at 30 a second ends at 2 seconds), and the size of the picture in the other two, as thousandths.
    let source = r#"/*{ "INPUTS": [ { "NAME": "inputImage", "TYPE": "image" } ] }*/
void main() {
	vec4 c = IMG_THIS_NORM_PIXEL(inputImage);
	gl_FragColor = vec4(TIME / 2.0, RENDERSIZE.x / 1000.0, RENDERSIZE.y / 1000.0, 1.0) + c * 0.0;
}
"#;
    put_a_shader_on(&mut project, AnimationTarget::Clip(clip), "time.fs", source);

    let decoded = exported(&toolchain, &project, scratch.path(), "shader_time.mp4");

    let at = |frame: usize| colour_at(&decoded.pictures[frame], WIDTH / 2, HEIGHT / 2);
    assert_near(at(0), (0, 82, 46), "frame 0: no time, and a picture of 320 by 180");
    assert_near(
        at(30),
        (127, 82, 46),
        "frame 30 is a second in, which is half of two seconds",
    );
    assert_near(at(59), (251, 82, 46), "the last frame");
}

#[test]
fn a_shader_has_the_y_axis_of_opengl_where_up_is_the_top_of_the_picture() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let (mut project, clip) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    // Red where the coordinate of the shader is more than a half, which is the top of the picture; green at the left; blue where gl_FragCoord is in the upper half.
    let source = r#"/*{ "INPUTS": [ { "NAME": "inputImage", "TYPE": "image" } ] }*/
void main() {
	vec4 c = IMG_THIS_NORM_PIXEL(inputImage);
	float top = isf_FragNormCoord.y > 0.5 ? 1.0 : 0.0;
	float left = gl_FragCoord.x < RENDERSIZE.x * 0.5 ? 1.0 : 0.0;
	float high = gl_FragCoord.y > RENDERSIZE.y * 0.5 ? 1.0 : 0.0;
	gl_FragColor = vec4(top, left, high, 1.0) + c * 0.0;
}
"#;
    put_a_shader_on(&mut project, AnimationTarget::Clip(clip), "axes.fs", source);

    let decoded = exported(&toolchain, &project, scratch.path(), "shader_axes.mp4");

    let picture = &decoded.pictures[10];
    assert_near(
        colour_at(picture, WIDTH / 4, HEIGHT / 4),
        (255, 255, 255),
        "top left: up, left, high",
    );
    assert_near(
        colour_at(picture, WIDTH * 3 / 4, HEIGHT / 4),
        (255, 0, 255),
        "top right: up, not left, high",
    );
    assert_near(
        colour_at(picture, WIDTH / 4, HEIGHT * 3 / 4),
        (0, 255, 0),
        "bottom left: down, left, low",
    );
    assert_near(
        colour_at(picture, WIDTH * 3 / 4, HEIGHT * 3 / 4),
        (0, 0, 0),
        "bottom right",
    );
}

#[test]
fn a_shader_that_gives_the_picture_back_gives_it_back_the_way_up_it_went_in() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let (mut project, _) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    // A text object in front, so that the picture is not flat.
    let front = project.add_track_in_front();
    let (text, _) = project
        .insert_text(front, TimelineFrameIndex::new(0), TextObject::default(), 60)
        .unwrap();
    let pass_through = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ] }*/\nvoid main() { gl_FragColor = IMG_THIS_NORM_PIXEL(inputImage); }\n";
    let without = exported(&toolchain, &project, scratch.path(), "text_alone.mp4");
    put_a_shader_on(&mut project, AnimationTarget::Clip(text), "pass.fs", pass_through);
    let with = exported(&toolchain, &project, scratch.path(), "text_through_a_shader.mp4");

    assert_eq!(with.pictures.len(), without.pictures.len());
    let difference = |first: &[u8], second: &[u8]| {
        first
            .iter()
            .zip(second)
            .map(|(one, other)| i32::from(*one).abs_diff(i32::from(*other)))
            .max()
            .unwrap_or(0)
    };
    let worst = (0..with.pictures.len())
        .map(|frame| difference(&with.pictures[frame], &without.pictures[frame]))
        .max()
        .unwrap();
    assert!(
        worst <= 6,
        "a shader that gives the picture back changes it by {worst} of 255 at the most"
    );
}

#[test]
fn a_project_that_is_saved_and_read_again_carries_the_shader_and_exports_the_same() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let (mut project, clip) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    put_a_shader_on(&mut project, AnimationTarget::Clip(clip), "tint.fs", TINT_SHADER);
    let before = exported(&toolchain, &project, scratch.path(), "before_saving.mp4");

    let text = fastforwardrs::project_file::project_to_json_text(&project, scratch.path()).unwrap();
    assert!(
        text.contains("\"shader\"") && text.contains("\"language\": \"isf\""),
        "the file carries the shader"
    );
    let loaded = fastforwardrs::project_file::project_from_json_text(&text).unwrap();
    let kind = loaded.tracks()[0]
        .placement_at_index(0)
        .unwrap()
        .effects
        .iter()
        .next()
        .unwrap()
        .kind();
    assert_eq!(kind.shader_source().unwrap().text, TINT_SHADER);
    assert!(kind.is_installed());
    let after = exported(&toolchain, &loaded, scratch.path(), "after_loading.mp4");

    assert_eq!(before.pictures, after.pictures);
}

// --- a generator clip (phase 6): an ISF shader that reads no picture ---

const SOLID_GENERATOR: &str = r#"/*{
	"DESCRIPTION": "one colour",
	"INPUTS": [ { "NAME": "colour", "TYPE": "color", "DEFAULT": [0.2, 0.4, 0.9, 1.0] } ]
}*/
void main() {
	gl_FragColor = colour;
}
"#;

/// The colour that `SOLID_GENERATOR` makes, in the codes of a screen.
const SOLID_GENERATOR_COLOUR: (u8, u8, u8) = (51, 102, 230);

/// Puts a generator clip made by the shader `source` on a track in front of the project's video, from frame `first_frame` for `frames` frames, and gives it.
fn put_a_generator_in_front(project: &mut Project, name: &str, source: &str, first_frame: u64, frames: u64) -> ClipId {
    let file = fastforwardrs::isf::read(source).unwrap_or_else(|problem| panic!("{problem}"));
    let kind = EffectKind::from_snapshot(fastforwardrs::isf::descriptor_of(&file, name, source)).unwrap();
    assert!(
        kind.is_installed() && kind.is_a_generator(),
        "{:?}",
        fastforwardrs::isf::why_not_usable(kind.descriptor())
    );
    let front = project.add_track_in_front();
    let (clip, lasts) = project
        .try_insert_generator_clip(front, TimelineFrameIndex::new(first_frame), kind, frames)
        .unwrap();
    assert_eq!(lasts, frames);
    clip
}

#[test]
fn a_generator_clip_covers_the_picture_with_what_its_shader_makes_for_as_long_as_it_lasts_and_not_before_or_after() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let (mut project, _) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    put_a_generator_in_front(&mut project, "solid.fs", SOLID_GENERATOR, 20, 20);

    let decoded = exported(&toolchain, &project, scratch.path(), "generator.mp4");

    assert_eq!(decoded.pictures.len(), 60);
    for (number, picture) in decoded.pictures.iter().enumerate() {
        let wanted = if (20..40).contains(&number) {
            SOLID_GENERATOR_COLOUR
        } else {
            ORANGE
        };
        for (x, y) in [(1, 1), (WIDTH / 2, HEIGHT / 2), (WIDTH - 2, HEIGHT - 2)] {
            assert_near(colour_at(picture, x, y), wanted, &format!("frame {number}, at {x},{y}"));
        }
    }
}

#[test]
fn the_values_of_a_generator_are_values_of_its_content_and_keyframes_of_them_change_what_it_makes() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let (mut project, _) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    let clip = put_a_generator_in_front(&mut project, "solid.fs", SOLID_GENERATOR, 0, 60);
    let target = AnimationTarget::Clip(clip);
    let content = project
        .placement_of(clip)
        .unwrap()
        .clip
        .as_generator()
        .unwrap()
        .content();
    let kind = project.effects_of(target).unwrap().instance(content).unwrap().kind();
    let colour = PropertyId::effect_value(kind, content, 0);
    make_a_keyframe(
        &mut project,
        target,
        colour,
        0,
        PropertyValue::Colour(TextColour::new(240, 20, 20, 255)),
    );
    make_a_keyframe(
        &mut project,
        target,
        colour,
        59,
        PropertyValue::Colour(TextColour::new(20, 20, 240, 255)),
    );

    let decoded = exported(&toolchain, &project, scratch.path(), "generator_changing.mp4");

    let at = |frame: usize| colour_at(&decoded.pictures[frame], WIDTH / 2, HEIGHT / 2);
    assert_near(at(0), (240, 20, 20), "at the first keyframe");
    assert_near(at(59), (20, 20, 240), "at the last");
    let halfway = at(30);
    assert!(
        (100..=160).contains(&halfway.0) && (100..=160).contains(&halfway.2),
        "{halfway:?}"
    );
}

#[test]
fn a_generator_that_is_put_somewhere_is_a_rectangle_of_what_it_makes_over_what_is_behind_it() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let (mut project, _) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    let clip = put_a_generator_in_front(&mut project, "solid.fs", SOLID_GENERATOR, 0, 60);
    project
        .set_clip_placement(
            clip,
            fastforwardrs::project::Placement::upright(fastforwardrs::project::CanvasArea::new(0.0, 0.0, 0.5, 0.5)),
        )
        .unwrap();

    let decoded = exported(&toolchain, &project, scratch.path(), "generator_rectangle.mp4");

    let picture = &decoded.pictures[10];
    assert_near(
        colour_at(picture, WIDTH / 4, HEIGHT / 4),
        SOLID_GENERATOR_COLOUR,
        "inside it",
    );
    assert_near(colour_at(picture, WIDTH * 3 / 4, HEIGHT * 3 / 4), ORANGE, "outside it");
}

#[test]
fn an_effect_put_on_a_generator_is_done_to_what_it_makes() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let (mut project, _) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    let clip = put_a_generator_in_front(&mut project, "solid.fs", SOLID_GENERATOR, 0, 60);
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(clip), 100.0);

    let decoded = exported(&toolchain, &project, scratch.path(), "generator_turned.mp4");

    assert_every_picture_is(&decoded, turned(SOLID_GENERATOR_COLOUR, 100.0), "what it makes, turned");
}

#[test]
fn the_time_of_a_generator_is_the_time_of_its_clip() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let (mut project, _) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    // Red is TIME over two seconds; the clip begins at frame 20 and lasts 40 frames.
    let source = r#"/*{ "INPUTS": [] }*/
void main() { gl_FragColor = vec4(TIME / 2.0, 0.0, 0.0, 1.0); }
"#;
    put_a_generator_in_front(&mut project, "time.fs", source, 20, 40);

    let decoded = exported(&toolchain, &project, scratch.path(), "generator_time.mp4");

    let at = |frame: usize| colour_at(&decoded.pictures[frame], WIDTH / 2, HEIGHT / 2);
    assert_near(
        at(20),
        (0, 0, 0),
        "the first frame of the clip is time 0, not the 20 frames of the timeline",
    );
    assert_near(at(50), (127, 0, 0), "30 frames in is a second");
}

#[test]
fn a_project_with_a_generator_is_saved_and_read_again_and_exports_the_same() {
    let toolchain = toolchain_and_graphics_or_skip!();
    let scratch = scratch_directory();
    let (mut project, _) = project_of_a_flat_colour_video(&toolchain, scratch.path(), ORANGE);
    put_a_generator_in_front(&mut project, "solid.fs", SOLID_GENERATOR, 10, 30);
    let before = exported(&toolchain, &project, scratch.path(), "generator_before.mp4");

    let text = fastforwardrs::project_file::project_to_json_text(&project, scratch.path()).unwrap();
    assert!(text.contains("\"kind\": \"generator\""), "{text}");
    let loaded = fastforwardrs::project_file::project_from_json_text(&text).unwrap();
    let after = exported(&toolchain, &loaded, scratch.path(), "generator_after.mp4");

    assert_eq!(before.pictures, after.pictures);
}
