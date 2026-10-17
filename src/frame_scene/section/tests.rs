// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the layers and the symbol of a section of the plan are at a frame: what is in the section, in its order, where its keyframes say for *that* frame, with the
//! colours its effects make *at that frame*, and with what the callers say a video and a picture file are.

use super::*;
use crate::colour_effects::ColourMatrix;
use crate::composition::{PicturePlan, plan_pictures};
use crate::engine::PictureColour;
use crate::project::{AnimationTarget, CanvasArea, ClipId, Placement, Project, PropertyId, PropertyValue, TextObject};
use crate::test_support::{HUE_SHIFT_AS_THE_FIRST_EFFECT, make_a_keyframe, put_a_hue_shift_on};
use crate::video_export::test_projects::{
    clip_location, frame, picture_source, project_with_video, speed, video_source,
};

fn plan_of(project: &Project) -> PicturePlan {
    plan_pictures(project, |_| true)
}

/// A video of 300 frames at the back, and from frame 50 for 60 frames a text on a track in front of it and a picture file over that. Gives the clips of the video and
/// the text.
fn a_video_with_a_text_and_a_picture_file_over_it() -> (Project, ClipId, ClipId) {
    let mut project = project_with_video(320, 180, false);
    let video = clip_location(&project, 0, 0);
    let front = project.add_track_in_front();
    let (text, _) = project
        .insert_text(
            front,
            frame(50),
            TextObject {
                text: "hello".to_owned(),
                ..TextObject::default()
            },
            60,
        )
        .unwrap();
    project.import_source(picture_source(2, 100, 50), frame(50)).unwrap();
    (project, video, text)
}

/// What the callers were asked for, as the layers were made.
#[derive(Default)]
struct WhatWasAsked {
    videos: Vec<(ClipId, SourceMediaIdentifier)>,
    pictures: Vec<SourceMediaIdentifier>,
}

fn the_layers_at<'a>(
    plan: &'a PicturePlan,
    timeline_frame: u64,
    asked: &mut WhatWasAsked,
    the_picture_file: Option<&'a RgbaPicture>,
) -> Vec<PreviewLayer<'a>> {
    let section = plan
        .section_at(frame(timeline_frame))
        .expect("the frame is on the timeline");
    layers_of_a_section(
        section,
        frame(timeline_frame),
        |layer, video| {
            asked.videos.push((layer.clip, video.source));
            VideoInThePicture::Waiting
        },
        |source| {
            asked.pictures.push(source);
            (the_picture_file, PictureColour::SRGB)
        },
    )
}

fn kind_of_the_layer_in_the_plan(layer: &Layer) -> &'static str {
    match layer.content {
        LayerContent::Video(_) => "video",
        LayerContent::Image { .. } => "picture file",
        LayerContent::Text(_) => "text",
    }
}

fn kind_of_the_layer_made(layer: &PreviewLayer<'_>) -> &'static str {
    match layer.content {
        PreviewLayerContent::Video(_) => "video",
        PreviewLayerContent::Image { .. } => "picture file",
        PreviewLayerContent::Text(_) => "text",
    }
}

#[test]
fn the_layers_are_those_of_the_section_in_its_order_and_each_is_of_its_clip_and_its_track() {
    let (project, _, _) = a_video_with_a_text_and_a_picture_file_over_it();
    let plan = plan_of(&project);
    let section = plan.section_at(frame(60)).unwrap();
    let planned_kinds: Vec<&str> = section.layers.iter().map(kind_of_the_layer_in_the_plan).collect();
    assert_eq!(planned_kinds.len(), 3, "a premise: all three are seen at frame 60");
    for kind in ["video", "picture file", "text"] {
        assert_eq!(
            planned_kinds.iter().filter(|planned| **planned == kind).count(),
            1,
            "{kind}"
        );
    }

    let layers = the_layers_at(&plan, 60, &mut WhatWasAsked::default(), None);

    assert_eq!(
        layers.iter().map(kind_of_the_layer_made).collect::<Vec<_>>(),
        planned_kinds,
        "in the order of the plan, which is back to front"
    );
    for (made, planned) in layers.iter().zip(&section.layers) {
        assert_eq!(made.clip, planned.clip);
        assert_eq!(made.track, planned.track);
    }
    let tracks: std::collections::HashSet<_> = layers.iter().map(|layer| layer.track).collect();
    assert!(
        tracks.len() >= 2,
        "they are not all of one track, so a mix-up of tracks would show"
    );
}

#[test]
fn a_video_is_asked_about_once_with_its_layer_and_a_picture_file_once_with_its_source_and_a_text_is_not_asked_about() {
    let (project, video, _) = a_video_with_a_text_and_a_picture_file_over_it();
    let plan = plan_of(&project);
    let the_picture_file = RgbaPicture {
        width_in_pixels: 2,
        height_in_pixels: 1,
        rgba_pixels: vec![9; 8],
    };
    let mut asked = WhatWasAsked::default();

    let layers = the_layers_at(&plan, 60, &mut asked, Some(&the_picture_file));

    assert_eq!(asked.videos, vec![(video, SourceMediaIdentifier::new(1))]);
    assert_eq!(asked.pictures, vec![SourceMediaIdentifier::new(2)]);
    let mut picture_files = 0;
    for layer in &layers {
        match &layer.content {
            PreviewLayerContent::Image {
                source,
                picture,
                colour,
            } => {
                picture_files += 1;
                assert_eq!(*source, SourceMediaIdentifier::new(2));
                assert!(
                    std::ptr::eq(picture.expect("the picture was given"), &the_picture_file),
                    "the picture that the caller gave is the layer's"
                );
                assert_eq!(
                    *colour,
                    PictureColour::SRGB,
                    "and so is the colour that the caller gave"
                );
            }
            PreviewLayerContent::Video(shown) => assert!(matches!(shown, VideoInThePicture::Waiting)),
            PreviewLayerContent::Text(text) => assert_eq!(text.text, "hello"),
        }
    }
    assert_eq!(picture_files, 1);
}

#[test]
fn a_picture_file_that_is_not_decoded_yet_is_a_layer_without_a_picture() {
    let (project, _, _) = a_video_with_a_text_and_a_picture_file_over_it();
    let plan = plan_of(&project);

    let layers = the_layers_at(&plan, 60, &mut WhatWasAsked::default(), None);

    let without_a_picture = layers
        .iter()
        .filter(|layer| matches!(layer.content, PreviewLayerContent::Image { picture: None, .. }))
        .count();
    assert_eq!(without_a_picture, 1);
}

#[test]
fn where_a_layer_is_is_where_its_keyframes_say_for_the_frame_asked_for_and_not_for_the_first_frame_of_its_section() {
    let mut project = project_with_video(320, 180, false);
    let clip = clip_location(&project, 0, 0);
    for (at, size) in [(0, 0.25), (200, 0.75)] {
        make_a_keyframe(
            &mut project,
            AnimationTarget::Clip(clip),
            PropertyId::Size,
            at,
            PropertyValue::Pair(size, size),
        );
    }
    let plan = plan_of(&project);
    assert_eq!(plan.sections.len(), 1, "a premise: the whole timeline is one section");
    let planned = &plan.sections[0].layers[0];

    let mut areas = Vec::new();
    for timeline_frame in [0, 1, 100, 199, 200] {
        let layers = the_layers_at(&plan, timeline_frame, &mut WhatWasAsked::default(), None);
        assert_eq!(layers.len(), 1);
        assert_eq!(
            layers[0].placement,
            planned.placement_at(frame(timeline_frame)),
            "frame {timeline_frame}"
        );
        areas.push(layers[0].placement.area);
    }

    for pair in areas.windows(2) {
        assert_ne!(
            pair[0], pair[1],
            "every one of these frames has the layer in another place"
        );
    }
    assert_ne!(
        areas[0], planned.placement.area,
        "and it is not where the clip says without the keyframes"
    );
}

#[test]
fn the_colours_of_a_layer_are_what_its_effects_make_at_the_frame_asked_for_and_nothing_when_it_has_none() {
    let (mut project, video, _) = a_video_with_a_text_and_a_picture_file_over_it();
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(video), 90.0);
    for (at, degrees) in [(0, 0.0), (100, 60.0)] {
        make_a_keyframe(
            &mut project,
            AnimationTarget::Clip(video),
            HUE_SHIFT_AS_THE_FIRST_EFFECT,
            at,
            PropertyValue::Number(degrees),
        );
    }
    let plan = plan_of(&project);

    // Frame 25 is in the section that has the video alone, and frame 75 in the one that has the text and the picture file over it too.
    for (timeline_frame, turn, layers_in_the_section) in [(25, 15.0, 1), (75, 45.0, 3)] {
        let layers = the_layers_at(&plan, timeline_frame, &mut WhatWasAsked::default(), None);

        assert_eq!(layers.len(), layers_in_the_section, "a premise: frame {timeline_frame}");
        for layer in &layers {
            let expected = if layer.clip == video {
                ColourMatrix::hue_rotation(turn)
            } else {
                ColourMatrix::NOTHING_CHANGES
            };
            assert_eq!(
                layer.colour(),
                expected,
                "frame {timeline_frame}, a {}",
                kind_of_the_layer_made(layer)
            );
        }
    }
}

#[test]
fn a_text_is_as_its_keyframes_make_it_at_the_frame_asked_for() {
    let (mut project, _, text) = a_video_with_a_text_and_a_picture_file_over_it();
    for (at, size) in [(50, 0.1), (109, 0.3)] {
        make_a_keyframe(
            &mut project,
            AnimationTarget::Clip(text),
            PropertyId::FontSize,
            at,
            PropertyValue::Number(size),
        );
    }
    let plan = plan_of(&project);

    let mut font_sizes = Vec::new();
    for timeline_frame in [50, 80, 109] {
        let layers = the_layers_at(&plan, timeline_frame, &mut WhatWasAsked::default(), None);
        let PreviewLayerContent::Text(made) = &layers
            .iter()
            .find(|layer| layer.clip == text)
            .expect("the text is a layer")
            .content
        else {
            panic!("the layer of a text is a text");
        };
        let planned = plan
            .section_at(frame(timeline_frame))
            .unwrap()
            .layers
            .iter()
            .find(|layer| layer.clip == text)
            .unwrap();
        assert_eq!(
            **made,
            *planned.text_at(frame(timeline_frame)).unwrap(),
            "frame {timeline_frame}"
        );
        font_sizes.push(made.font_size);
    }

    assert!(
        font_sizes[0] != font_sizes[1] && font_sizes[1] != font_sizes[2],
        "the letters are another size at each of these frames: {font_sizes:?}"
    );
}

/// The symbol that a section has when the video in front plays at `speed_in_front` and a video behind it, which is seen too, at `speed_behind`; and the clip in front.
fn symbol_of_two_videos(speed_in_front: u32, speed_behind: u32) -> (Option<SymbolOnThePreview>, ClipId) {
    let mut project = project_with_video(320, 180, false);
    let behind = clip_location(&project, 0, 0);
    project.set_clip_speed(behind, speed(speed_behind)).unwrap();
    // A second video in front of the first, on a track of its own and small, so that the first is seen behind it.
    project
        .import_source(video_source(2, 320, 180, 300, false), frame(0))
        .unwrap();
    let in_front = clip_location(&project, 0, 0);
    assert_ne!(in_front, behind, "a premise: the second video is in front");
    project
        .set_clip_placement(in_front, Placement::upright(CanvasArea::new(0.5, 0.5, 0.25, 0.25)))
        .unwrap();
    project.set_clip_speed(in_front, speed(speed_in_front)).unwrap();
    let plan = plan_of(&project);
    let section = plan.section_at(frame(0)).unwrap();
    assert_eq!(section.video_layers().count(), 2, "a premise: two videos are seen");
    (symbol_of_a_section(section), in_front)
}

#[test]
fn the_symbol_is_over_the_video_that_is_in_front_when_that_plays_faster_than_normal_and_not_otherwise() {
    let (normal_over_normal, _) = symbol_of_two_videos(1, 1);
    let (fast_in_front, the_one_in_front) = symbol_of_two_videos(4, 1);
    let (fast_behind, _) = symbol_of_two_videos(1, 4);

    assert!(normal_over_normal.is_none());
    let symbol = fast_in_front.expect("the video in front plays at 4x");
    assert_eq!(symbol.speed, speed(4));
    assert_eq!(symbol.over, the_one_in_front);
    assert!(
        fast_behind.is_none(),
        "the symbol tells the speed of the video that is in front, and a faster one behind it does not count"
    );
}

#[test]
fn a_text_in_front_of_a_video_that_plays_faster_does_not_take_the_symbol_away() {
    let mut project = project_with_video(320, 180, false);
    let video = clip_location(&project, 0, 0);
    project.set_clip_speed(video, speed(3)).unwrap();
    let front = project.add_track_in_front();
    project.insert_text(front, frame(0), TextObject::default(), 40).unwrap();
    let plan = plan_of(&project);
    let section = plan.section_at(frame(0)).unwrap();
    assert_eq!(section.layers.len(), 2, "a premise: the text is seen over the video");

    let symbol = symbol_of_a_section(section).expect("the video plays at 3x");

    assert_eq!(symbol.speed, speed(3));
    assert_eq!(symbol.over, video);
}

#[test]
fn a_section_with_nothing_in_it_has_no_layers_and_no_symbol() {
    let section = PictureSection {
        first_timeline_frame: 0,
        end_timeline_frame: 10,
        layers: Vec::new(),
    };

    let layers = layers_of_a_section(
        &section,
        frame(3),
        |_, _| panic!("there is no video"),
        |_| panic!("there is no picture file"),
    );

    assert!(layers.is_empty());
    assert!(symbol_of_a_section(&section).is_none());
}

#[test]
fn the_colour_that_the_caller_gives_for_a_picture_file_is_the_colour_of_its_layer() {
    use crate::colour_space::{StandardPrimaries, TransferFunction};
    let mut project = project_with_video(320, 180, false);
    project.import_source(picture_source(2, 64, 48), frame(0)).unwrap();
    let plan = plan_pictures(&project, |_| true);
    let section = plan.section_at(frame(0)).expect("the frame is on the timeline");
    let chosen = PictureColour {
        primaries: StandardPrimaries::Rec2020,
        curve: TransferFunction::Gamma24,
    };
    let the_picture_file = RgbaPicture {
        width_in_pixels: 1,
        height_in_pixels: 1,
        rgba_pixels: vec![1, 2, 3, 255],
    };

    let layers = layers_of_a_section(
        section,
        frame(0),
        |_, _| VideoInThePicture::Waiting,
        |_| (Some(&the_picture_file), chosen),
    );

    let colours: Vec<PictureColour> = layers
        .iter()
        .filter_map(|layer| match &layer.content {
            PreviewLayerContent::Image { colour, .. } => Some(*colour),
            _ => None,
        })
        .collect();
    assert_eq!(colours, vec![chosen]);
}
