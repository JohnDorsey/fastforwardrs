// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What is asked of the engine for each frame of an export: which layers, in which order, where, with which colours and which pictures; which pictures are sent and
//! when they are let go of. (Whether the engine then draws it is for the tests of the engine; whether the finished video is right is for `tests/video_export_*.rs`.)

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::*;
use crate::composition::LayerContent;
use crate::engine::LayerSource;
use crate::engine::{COLOUR_MATRIX_KEY, CORNERS_OF_THE_WHOLE_PICTURE, PicturePixels};
use crate::project::{
    AnimationTarget, CanvasArea, ClipId, Placement, Project, PropertyId, PropertyValue, Settings, SpeedIndicatorLayout,
    TextObject,
};
use crate::test_support::{make_a_keyframe, put_a_hue_shift_on};
use crate::video_export::plan_export;
use crate::video_export::test_projects::{
    clip_location, frame, picture_source, project_with_video, speed, video_source,
};
use crate::video_export::test_streams::{Script, locations, rgba};

type TextCalls = Rc<RefCell<Vec<(String, u32, u32)>>>;

/// A source of text pictures that remembers what it was asked to draw and draws a picture of 4 by 2 pixels.
fn text_pictures_that_are_recorded(calls: &TextCalls) -> Box<dyn TextPictures> {
    let calls = Rc::clone(calls);
    Box::new(move |object: &TextObject, width: u32, height: u32| {
        calls.borrow_mut().push((object.text.clone(), width, height));
        RgbaPicture {
            width_in_pixels: 4,
            height_in_pixels: 2,
            rgba_pixels: vec![200; 32],
        }
    })
}

/// A source of text pictures that cannot draw (the fonts are not loaded).
struct NoFontsYet;

impl TextPictures for NoFontsYet {
    fn text_picture(&mut self, _: &TextObject, _: u32, _: u32) -> Option<RgbaPicture> {
        None
    }
}

/// The plan of `project`, **in the compatibility colour** (the numbers are codes, and a video picture's id is its serial number): these tests are about how the pictures are
/// sent and let go of, and not about their colour, which has tests of its own below. (They were written before phase 3 and are as they were: only this line is new.)
fn plan_of(project: &Project) -> ExportPlan {
    let mut plan = plan_export(project, Settings::DEFAULT).unwrap();
    plan.colour = crate::engine::FrameColour::default();
    plan
}

/// A script that gives `pictures` pictures to each stream that is started, the picture number `n` all `n`.
fn script_of_pictures(pictures: u8) -> Script {
    let script = Script::default();
    for _ in 0..8 {
        script
            .state()
            .to_say
            .push_back((0..pictures).map(rgba).collect::<Vec<_>>());
    }
    script
}

fn describer_of<'p>(
    plan: &'p ExportPlan,
    script: &Script,
    images: HashMap<SourceMediaIdentifier, RgbaPicture>,
    text: Box<dyn TextPictures + 'p>,
) -> FrameDescriber<'p, Script> {
    let pictures = PicturesOfTheRuns::new(script.clone(), plan, &locations(), &HashMap::new()).unwrap();
    FrameDescriber::new(plan, pictures, images, text)
}

/// Describes `timeline_frame` and gives what was described and what was sent for it.
fn describe(
    describer: &mut FrameDescriber<'_, Script>,
    timeline_frame: u64,
) -> (DescribedFrame, Vec<(PictureId, PictureData)>) {
    let mut sent = Vec::new();
    let described = describer
        .describe(timeline_frame, &AtomicBool::new(false), &mut |id, data| {
            sent.push((id, data));
        })
        .unwrap_or_else(|problem| panic!("frame {timeline_frame}: {problem}"));
    (described, sent)
}

fn ids_of(sent: &[(PictureId, PictureData)]) -> Vec<PictureId> {
    sent.iter().map(|(id, _)| *id).collect()
}

fn source_of_the_layers(described: &DescribedFrame) -> Vec<LayerSource> {
    described
        .description
        .layers
        .iter()
        .map(|layer| layer.source.clone())
        .collect()
}

fn a_picture_file(width: u32, height: u32) -> RgbaPicture {
    RgbaPicture {
        width_in_pixels: width,
        height_in_pixels: height,
        rgba_pixels: vec![77; (width * height * 4) as usize],
    }
}

/// A video of 300 frames at the back, a text on a track in front of it from frame 50 to frame 80, and a picture file on a track in front of that from frame
/// `picture_at` for as long as pictures last. Also gives the first and the last frame of the picture file.
fn video_with_a_text_and_a_picture_over_it(picture_at: u64) -> (Project, (u64, u64)) {
    let mut project = project_with_video(320, 180, false);
    let front = project.add_track_in_front();
    project
        .insert_text(
            front,
            frame(50),
            TextObject {
                text: "hello".to_owned(),
                ..TextObject::default()
            },
            30,
        )
        .unwrap();
    project
        .import_source(picture_source(2, 100, 50), frame(picture_at))
        .unwrap();
    let plan = plan_of(&project);
    let picture_frames: Vec<u64> = plan
        .pictures
        .sections
        .iter()
        .filter(|section| {
            section
                .layers
                .iter()
                .any(|layer| matches!(layer.content, LayerContent::Image { .. }))
        })
        .flat_map(|section| section.first_timeline_frame..section.end_timeline_frame)
        .collect();
    assert!(!picture_frames.is_empty(), "the picture file is seen");
    (
        project,
        (picture_frames[0], *picture_frames.last().expect("it is seen")),
    )
}

#[test]
fn a_video_alone_is_one_layer_that_is_its_picture_filling_the_frame() {
    let plan = plan_of(&project_with_video(320, 180, false));
    let script = script_of_pictures(255);
    let mut describer = describer_of(&plan, &script, HashMap::new(), Box::new(NoFontsYet));
    let (first, sent) = describe(&mut describer, 0);
    assert_eq!(first.description.render_size, (320, 180));
    assert_eq!(first.description.layers.len(), 1);
    let layer = &first.description.layers[0];
    assert_eq!(
        layer.source,
        LayerSource::Picture(PictureId(1)),
        "the first picture there is"
    );
    assert_eq!(layer.corners, CORNERS_OF_THE_WHOLE_PICTURE);
    assert!(layer.effects.is_empty());
    assert!(first.forget.is_empty());
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, PictureId(1));
    assert_eq!((sent[0].1.width_in_pixels, sent[0].1.height_in_pixels), (2, 2));
    assert_eq!(
        sent[0].1.pixels,
        PicturePixels::Rgba8 {
            straight_alpha: vec![0; 16]
        },
        "the pixels of the first picture of the video, as the stream gave them"
    );
}

#[test]
fn the_picture_of_each_frame_of_a_video_is_sent_for_it_and_let_go_of_by_the_next() {
    let plan = plan_of(&project_with_video(320, 180, false));
    let script = script_of_pictures(255);
    let mut describer = describer_of(&plan, &script, HashMap::new(), Box::new(NoFontsYet));
    for timeline_frame in 0..20 {
        let (described, sent) = describe(&mut describer, timeline_frame);
        let this_picture = PictureId(timeline_frame + 1);
        assert_eq!(source_of_the_layers(&described), [LayerSource::Picture(this_picture)]);
        assert_eq!(ids_of(&sent), [this_picture], "frame {timeline_frame}");
        let expected_to_forget: Vec<PictureId> = (timeline_frame > 0)
            .then_some(PictureId(timeline_frame))
            .into_iter()
            .collect();
        assert_eq!(
            described.forget, expected_to_forget,
            "the picture of the frame before is not wanted any more"
        );
    }
}

#[test]
fn layers_are_in_the_order_of_the_tracks_the_back_one_first() {
    // The text is seen from frame 50 to frame 79, and the picture file from frame 60 on.
    let (project, (picture_first, _)) = video_with_a_text_and_a_picture_over_it(60);
    assert_eq!(picture_first, 60);
    let plan = plan_of(&project);
    let script = script_of_pictures(255);
    let images = HashMap::from([(SourceMediaIdentifier::new(2), a_picture_file(100, 50))]);
    let calls = TextCalls::default();
    let mut describer = describer_of(&plan, &script, images, text_pictures_that_are_recorded(&calls));
    let (mut text_id, mut image_id) = (None, None);
    for timeline_frame in 0..=85 {
        let (described, sent) = describe(&mut describer, timeline_frame);
        let video = LayerSource::Picture(PictureId(timeline_frame + 1));
        let new_ones: Vec<PictureId> = ids_of(&sent)
            .into_iter()
            .filter(|id| id.0 != timeline_frame + 1)
            .collect();
        match timeline_frame {
            50 => text_id = Some(new_ones[0]),
            60 => image_id = Some(new_ones[0]),
            _ => {}
        }
        let text = text_id.map(LayerSource::Picture);
        let image = image_id.map(LayerSource::Picture);
        let expected: Vec<LayerSource> = match timeline_frame {
            0..50 => vec![video],
            50..60 => vec![video, text.unwrap()],
            60..80 => vec![video, text.unwrap(), image.unwrap()],
            _ => vec![video, image.unwrap()],
        };
        assert_eq!(source_of_the_layers(&described), expected, "frame {timeline_frame}");
    }
    assert_ne!(text_id, image_id);
}

#[test]
fn a_text_is_drawn_once_for_the_size_of_the_picture_and_kept_for_as_long_as_it_is_seen_and_then_let_go_of() {
    let (project, _) = video_with_a_text_and_a_picture_over_it(150);
    let plan = plan_of(&project);
    let script = script_of_pictures(255);
    let calls = TextCalls::default();
    let mut describer = describer_of(&plan, &script, HashMap::new(), text_pictures_that_are_recorded(&calls));
    let mut text_ids = Vec::new();
    for timeline_frame in 0..=81 {
        let (described, sent) = describe(&mut describer, timeline_frame);
        let video_picture = PictureId(timeline_frame + 1);
        let others: Vec<PictureId> = ids_of(&sent).into_iter().filter(|id| *id != video_picture).collect();
        match timeline_frame {
            50 => {
                assert_eq!(others.len(), 1, "the text is sent when it is first seen");
                text_ids.push(others[0]);
                assert_eq!(
                    calls.borrow().as_slice(),
                    [("hello".to_owned(), 320, 180)],
                    "it is drawn once, for a picture of 320 by 180"
                );
            }
            _ => assert!(
                others.is_empty(),
                "frame {timeline_frame}: nothing but the video's picture is sent"
            ),
        }
        if (50..80).contains(&timeline_frame) {
            assert_eq!(described.description.layers.len(), 2, "frame {timeline_frame}");
            assert_eq!(
                described.description.layers[1].source,
                LayerSource::Picture(text_ids[0]),
                "the text is over the video"
            );
        }
        let forgets_the_text = described
            .forget
            .contains(&text_ids.first().copied().unwrap_or(PictureId(0)));
        assert_eq!(forgets_the_text, timeline_frame == 80, "frame {timeline_frame}");
    }
    assert_eq!(calls.borrow().len(), 1, "never drawn again");
}

#[test]
fn the_pictures_that_are_let_go_of_are_listed_in_order_whatever_order_they_were_sent_in() {
    // At frame 80 the text ends: the video's picture of frame 79 and the text are let go of together. (A set has an order of its own, which is another for each; so
    // many describers are tried.)
    for _ in 0..24 {
        let (project, _) = video_with_a_text_and_a_picture_over_it(150);
        let plan = plan_of(&project);
        let script = script_of_pictures(255);
        let mut describer = describer_of(
            &plan,
            &script,
            HashMap::new(),
            Box::new(|_: &TextObject, _: u32, _: u32| a_picture_file(4, 2)),
        );
        for timeline_frame in 0..80 {
            describe(&mut describer, timeline_frame);
        }
        let (described, _) = describe(&mut describer, 80);
        assert!(described.forget.len() >= 2, "a premise: {:?}", described.forget);
        assert!(
            described.forget.windows(2).all(|pair| pair[0] < pair[1]),
            "{:?}",
            described.forget
        );
    }
}

#[test]
fn a_run_is_started_by_the_frames_before_it_as_soon_as_it_is_near_and_not_before() {
    use crate::video_export::pictures::FRAMES_AHEAD_THAT_A_RUN_IS_STARTED;
    // The video is cut at frame 100: two clips, two runs.
    let mut project = project_with_video(320, 180, false);
    project.split_at(frame(100), None);
    let plan = plan_of(&project);
    assert_eq!(plan.runs.len(), 2, "a premise");
    let script = script_of_pictures(255);
    let mut describer = describer_of(&plan, &script, HashMap::new(), Box::new(NoFontsYet));

    describe(&mut describer, 0);
    assert_eq!(script.state().started.len(), 1, "only the run that is seen");
    for timeline_frame in 1..(100 - FRAMES_AHEAD_THAT_A_RUN_IS_STARTED) {
        describe(&mut describer, timeline_frame);
    }
    assert_eq!(
        script.state().started.len(),
        1,
        "the second run is not started while it is further than {FRAMES_AHEAD_THAT_A_RUN_IS_STARTED} frames away"
    );
    describe(&mut describer, 100 - FRAMES_AHEAD_THAT_A_RUN_IS_STARTED);
    assert_eq!(
        script.state().started.len(),
        2,
        "and it is started when it is that near, long before its first frame is wanted"
    );
}

#[test]
fn a_video_that_is_seen_where_the_plan_has_no_run_of_it_is_a_mistake_of_the_program_and_says_so() {
    let mut plan = plan_of(&project_with_video(320, 180, false));
    plan.runs.clear();
    let script = Script::default();
    let mut describer = describer_of(&plan, &script, HashMap::new(), Box::new(NoFontsYet));

    let problem = describer
        .describe(5, &AtomicBool::new(false), &mut |_, _| {})
        .unwrap_err();

    assert!(matches!(problem, FrameProblem::NoRunForAVideo(5)), "{problem:?}");
}

#[test]
fn the_pictures_that_stand_in_for_those_that_a_file_does_not_have_are_counted() {
    // The first run is of 100 frames, and its file has pictures for 60 of them.
    let mut project = project_with_video(320, 180, false);
    project.split_at(frame(100), None);
    let plan = plan_of(&project);
    let script = Script::default();
    script.state().to_say.push_back((0..60).map(rgba).collect());
    let mut describer = describer_of(&plan, &script, HashMap::new(), Box::new(NoFontsYet));
    assert_eq!(describer.pictures_held(), 0);

    for timeline_frame in 0..100 {
        describe(&mut describer, timeline_frame);
    }

    assert_eq!(describer.pictures_held(), 40);
}

#[test]
fn a_picture_file_is_sent_once_and_kept_for_as_long_as_it_is_seen() {
    let (project, (first, last)) = video_with_a_text_and_a_picture_over_it(100);
    let plan = plan_of(&project);
    let script = script_of_pictures(255);
    let image_source = SourceMediaIdentifier::new(2);
    let images = HashMap::from([(image_source, a_picture_file(100, 50))]);
    let calls = TextCalls::default();
    let mut describer = describer_of(&plan, &script, images, text_pictures_that_are_recorded(&calls));
    for timeline_frame in 0..first {
        describe(&mut describer, timeline_frame);
    }
    let mut image_id = None;
    for timeline_frame in first..=last.min(first + 40) {
        let (described, sent) = describe(&mut describer, timeline_frame);
        assert_eq!(described.description.layers.len(), 2, "frame {timeline_frame}");
        let video_picture = PictureId(timeline_frame + 1);
        let others: Vec<(PictureId, PictureData)> = sent.into_iter().filter(|(id, _)| *id != video_picture).collect();
        if timeline_frame == first {
            assert_eq!(others.len(), 1, "sent when it is first seen");
            assert_eq!((others[0].1.width_in_pixels, others[0].1.height_in_pixels), (100, 50));
            image_id = Some(others[0].0);
        } else {
            assert!(others.is_empty(), "frame {timeline_frame}: it is not sent again");
        }
        assert_eq!(
            described.description.layers[1].source,
            LayerSource::Picture(image_id.expect("sent at the first frame it is seen"))
        );
        assert!(!described.forget.contains(&image_id.unwrap()));
    }
}

#[test]
fn a_video_that_plays_faster_has_the_symbol_over_it_and_one_that_does_not_has_none() {
    // 0..100 at 1x, 100..150 at 4x.
    let mut project = project_with_video(320, 180, false);
    project.split_at(frame(100), None);
    let second = clip_location(&project, 0, 1);
    project.set_clip_speed(second, speed(4)).unwrap();
    let plan = plan_of(&project);
    let script = script_of_pictures(255);
    let mut describer = describer_of(&plan, &script, HashMap::new(), Box::new(NoFontsYet));
    let mut symbol_ids = Vec::new();
    for timeline_frame in 0..150 {
        let (described, sent) = describe(&mut describer, timeline_frame);
        if timeline_frame < 100 {
            assert_eq!(described.description.layers.len(), 1, "frame {timeline_frame}");
        } else {
            assert_eq!(
                described.description.layers.len(),
                2,
                "frame {timeline_frame}: the video and the symbol"
            );
            let LayerSource::Picture(symbol) = described.description.layers[1].source else {
                panic!("the symbol is a picture");
            };
            symbol_ids.push(symbol);
            let corners = described.description.layers[1].corners;
            assert!(
                corners
                    .iter()
                    .all(|&(x, y)| (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y)),
                "{corners:?} is inside the frame"
            );
            assert!(
                corners[1].0 > corners[0].0 && corners[2].1 > corners[1].1,
                "it has a size"
            );
            // It is sent the first time and not again, and it is not let go of.
            let others = sent
                .iter()
                .filter(|(id, _)| *id != PictureId(timeline_frame + 1))
                .count();
            assert_eq!(others, usize::from(timeline_frame == 100), "frame {timeline_frame}");
            assert!(!described.forget.contains(&symbol) || timeline_frame == 100);
        }
    }
    assert!(
        symbol_ids.windows(2).all(|pair| pair[0] == pair[1]),
        "the same symbol all the time"
    );
}

#[test]
fn the_symbol_is_put_where_the_layout_of_the_project_says_and_not_where_the_default_layout_puts_it() {
    let corners_of_the_symbol_with = |layout: SpeedIndicatorLayout| {
        let mut project = project_with_video(320, 180, false);
        let clip = clip_location(&project, 0, 0);
        project.set_clip_speed(clip, speed(4)).unwrap();
        project.set_speed_indicator_layout(layout);
        let plan = plan_of(&project);
        let script = script_of_pictures(255);
        let mut describer = describer_of(&plan, &script, HashMap::new(), Box::new(NoFontsYet));
        let (described, _) = describe(&mut describer, 0);
        assert_eq!(
            described.description.layers.len(),
            2,
            "a premise: the video and the symbol"
        );
        described.description.layers[1].corners
    };

    let by_default = corners_of_the_symbol_with(SpeedIndicatorLayout::DEFAULT);
    let elsewhere = corners_of_the_symbol_with(SpeedIndicatorLayout::new(0.5, 0.25, 0.2));

    assert_ne!(by_default, elsewhere);
    // Its left edge is half way across the picture, its top a quarter of the way down, and it is a fifth of the picture's height tall.
    assert!((elsewhere[0].0 - 0.5).abs() < 1e-3, "{elsewhere:?}");
    assert!((elsewhere[0].1 - 0.25).abs() < 1e-3, "{elsewhere:?}");
    assert!((elsewhere[3].1 - elsewhere[0].1 - 0.2).abs() < 1e-3, "{elsewhere:?}");
}

#[test]
fn where_a_layer_is_at_a_frame_is_what_its_keyframes_say_for_that_frame() {
    let mut project = project_with_video(320, 180, false);
    let clip = clip_location(&project, 0, 0);
    for (at, size) in [(0, 0.25), (100, 0.5), (200, 0.25)] {
        make_a_keyframe(
            &mut project,
            AnimationTarget::Clip(clip),
            PropertyId::Size,
            at,
            PropertyValue::Pair(size, size),
        );
    }
    let plan = plan_of(&project);
    let script = script_of_pictures(255);
    let mut describer = describer_of(&plan, &script, HashMap::new(), Box::new(NoFontsYet));
    let mut widths = Vec::new();
    for timeline_frame in 0..=200 {
        let (described, _) = describe(&mut describer, timeline_frame);
        let corners = described.description.layers[0].corners;
        // The oracle is the layer of the plan, asked about this very frame.
        let layer = &plan.pictures.section_at(frame(timeline_frame)).unwrap().layers[0];
        let placement = layer.placement_at(frame(timeline_frame));
        let expected = [
            crate::project::Corner::TopLeft,
            crate::project::Corner::TopRight,
            crate::project::Corner::BottomRight,
            crate::project::Corner::BottomLeft,
        ]
        .map(|corner| placement.corner_position(corner, (320.0, 180.0)));
        assert_eq!(corners, expected, "frame {timeline_frame}");
        widths.push(corners[1].0 - corners[0].0);
    }
    assert!((widths[0] - 0.25).abs() < 1e-6, "{}", widths[0]);
    assert!((widths[100] - 0.5).abs() < 1e-6, "{}", widths[100]);
    assert!((widths[200] - 0.25).abs() < 1e-6, "{}", widths[200]);
    let between = widths[50];
    assert!(
        between > 0.25 + 1e-3 && between < 0.5 - 1e-3,
        "half way between the keyframes it is between the sizes of them: {between}"
    );
    for (number, pair) in widths[..=100].windows(2).enumerate() {
        assert!(
            pair[1] > pair[0],
            "every frame is bigger than the one before, from the first keyframe to the second: frames {number} and {}",
            number + 1
        );
    }
}

#[test]
fn what_the_effects_do_to_the_colours_at_a_frame_is_one_colour_matrix_and_nothing_when_nothing_changes() {
    let mut project = project_with_video(320, 180, false);
    let clip = clip_location(&project, 0, 0);
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(clip), 0.0);
    // Turned half way round from frame 100 on, with a keyframe at 0 that leaves it as it is.
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        crate::test_support::HUE_SHIFT_AS_THE_FIRST_EFFECT,
        0,
        PropertyValue::Number(0.0),
    );
    make_a_keyframe(
        &mut project,
        AnimationTarget::Clip(clip),
        crate::test_support::HUE_SHIFT_AS_THE_FIRST_EFFECT,
        100,
        PropertyValue::Number(180.0),
    );
    let plan = plan_of(&project);
    let script = script_of_pictures(255);
    let mut describer = describer_of(&plan, &script, HashMap::new(), Box::new(NoFontsYet));
    let (at_the_start, _) = describe(&mut describer, 0);
    assert!(
        at_the_start.description.layers[0].effects.is_empty(),
        "a hue shift of nothing changes nothing, so it is not asked for"
    );
    let mut matrices = Vec::new();
    for timeline_frame in 1..=100 {
        let (described, _) = describe(&mut describer, timeline_frame);
        let effects = &described.description.layers[0].effects;
        assert_eq!(effects.len(), 1, "frame {timeline_frame}");
        assert_eq!(effects[0].key, COLOUR_MATRIX_KEY);
        assert_eq!(effects[0].values.len(), 9, "a matrix of 3 by 3 numbers");
        matrices.push(effects[0].values.clone());
    }
    assert_ne!(
        matrices[10], matrices[60],
        "the colours are changed differently at different frames"
    );
    assert_ne!(matrices[60], matrices[99]);
    let layer = &plan.pictures.section_at(frame(100)).unwrap().layers[0];
    let expected = layer.colour_matrix_at(frame(100));
    assert_eq!(
        matrices[99],
        expected.rows().into_iter().flatten().collect::<Vec<f32>>()
    );
}

#[test]
fn a_text_that_cannot_be_drawn_is_left_out_and_the_rest_of_the_frame_is_made() {
    let (project, _) = video_with_a_text_and_a_picture_over_it(150);
    let plan = plan_of(&project);
    let script = script_of_pictures(255);
    let mut describer = describer_of(&plan, &script, HashMap::new(), Box::new(NoFontsYet));
    for timeline_frame in [49, 50, 51] {
        let (described, _) = describe(&mut describer, timeline_frame);
        assert_eq!(
            described.description.layers.len(),
            1,
            "only the video, at {timeline_frame}"
        );
    }
}

#[test]
fn when_the_engine_has_lost_its_pictures_the_next_frame_sends_the_ones_it_uses_again() {
    let (project, _) = video_with_a_text_and_a_picture_over_it(150);
    let plan = plan_of(&project);
    let script = script_of_pictures(255);
    let calls = TextCalls::default();
    let mut describer = describer_of(&plan, &script, HashMap::new(), text_pictures_that_are_recorded(&calls));
    for timeline_frame in 0..=51 {
        describe(&mut describer, timeline_frame);
    }
    let (_, sent_at_52) = describe(&mut describer, 52);
    assert_eq!(sent_at_52.len(), 1, "the picture of the video and nothing else");
    describer.forget_what_was_sent();
    let (described, sent_at_53) = describe(&mut describer, 53);
    assert_eq!(sent_at_53.len(), 2, "the picture of the video, and the text again");
    assert_eq!(described.description.layers.len(), 2);
    assert!(described.forget.is_empty());
}

#[test]
fn a_frame_that_is_not_on_the_timeline_is_refused_and_a_video_that_gives_no_picture_is_a_mistake() {
    let plan = plan_of(&project_with_video(320, 180, false));
    let script = script_of_pictures(255);
    let mut describer = describer_of(&plan, &script, HashMap::new(), Box::new(NoFontsYet));
    let problem = describer
        .describe(300, &AtomicBool::new(false), &mut |_, _| {})
        .unwrap_err();
    assert!(matches!(problem, FrameProblem::NotOnTheTimeline(300)), "{problem:?}");
    assert_eq!(problem.to_string(), "frame 300 is not a frame of the project");

    let silent = Script::default();
    let mut describer = describer_of(&plan, &silent, HashMap::new(), Box::new(NoFontsYet));
    let problem = describer
        .describe(0, &AtomicBool::new(false), &mut |_, _| {})
        .unwrap_err();
    assert!(
        matches!(problem, FrameProblem::Picture(RunPictureProblem::NoPicture { .. })),
        "{problem:?}"
    );
}

#[test]
fn cancelling_is_noticed_while_the_picture_of_a_video_is_waited_for() {
    let plan = plan_of(&project_with_video(320, 180, false));
    let script = Script::default();
    script.state().never_ends = true;
    let mut describer = describer_of(&plan, &script, HashMap::new(), Box::new(NoFontsYet));
    let problem = describer
        .describe(0, &AtomicBool::new(true), &mut |_, _| {})
        .unwrap_err();
    assert!(
        matches!(problem, FrameProblem::Picture(RunPictureProblem::Cancelled)),
        "{problem:?}"
    );
}

#[test]
fn two_videos_at_once_are_two_layers_with_a_picture_each_from_their_own_run() {
    // The second video is in a quarter of the picture, over the first, from frame 100 to frame 200.
    let mut project = project_with_video(320, 180, false);
    project
        .import_source(video_source(2, 320, 180, 100, false), frame(100))
        .unwrap();
    let small: ClipId = clip_location(&project, 0, 0);
    project
        .set_clip_placement(small, Placement::upright(CanvasArea::new(0.5, 0.5, 0.25, 0.25)))
        .unwrap();
    let plan = plan_of(&project);
    assert_eq!(plan.runs.len(), 2);
    let script = Script::default();
    // The pictures of the video behind are valued 0, 1, ... and those of the video in front 150, 151, ..., so that they can be told apart.
    script.state().to_say.push_back((0..255).map(rgba).collect());
    script
        .state()
        .to_say
        .push_back((0..100).map(|value| rgba(value + 150)).collect());
    let mut describer = describer_of(&plan, &script, HashMap::new(), Box::new(NoFontsYet));
    let mut seen_with_two_layers = 0;
    for timeline_frame in 0..200 {
        let (described, sent) = describe(&mut describer, timeline_frame);
        if timeline_frame < 100 {
            assert_eq!(described.description.layers.len(), 1);
        } else {
            seen_with_two_layers += 1;
            assert_eq!(described.description.layers.len(), 2, "frame {timeline_frame}");
            let mut first_bytes: Vec<u8> = sent
                .iter()
                .map(|(_, data)| match &data.pixels {
                    PicturePixels::Rgba8 { straight_alpha } => straight_alpha[0],
                    PicturePixels::Yuv420p8 { y, .. } => y[0],
                })
                .collect();
            first_bytes.sort_unstable();
            assert_eq!(
                first_bytes,
                [timeline_frame as u8, (timeline_frame - 100) as u8 + 150],
                "a picture of each video: frame {timeline_frame}"
            );
            let front = described.description.layers[1].corners;
            assert!(
                front[1].0 - front[0].0 < 0.5,
                "the one in front is small: {front:?} at {timeline_frame}"
            );
        }
    }
    assert_eq!(seen_with_two_layers, 100);
}

// ---- the colour of the frames (phase 3)

/// The plan of `project` with the colour that the settings give it, which with the default settings is linear light in the primaries of Rec.709.
fn plan_in_linear_light_of(project: &Project) -> ExportPlan {
    plan_export(project, Settings::DEFAULT).unwrap()
}

#[test]
fn a_frame_of_the_export_has_the_colour_of_the_plan_and_the_pictures_it_sends_are_made_for_its_working_space() {
    use crate::engine::WorkingSpace;
    let project = project_with_video(320, 180, false);
    let plan = plan_in_linear_light_of(&project);
    assert!(
        matches!(plan.colour.working, WorkingSpace::Linear(_)),
        "{:?}",
        plan.colour
    );
    let script = script_of_pictures(255);
    let mut describer = describer_of(&plan, &script, HashMap::new(), Box::new(NoFontsYet));

    let (described, sent) = describe(&mut describer, 0);

    assert_eq!(described.description.colour, plan.colour);
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0].1.working, plan.colour.working,
        "what is sent is made for the working space of the frame"
    );
    assert_eq!(sent[0].1.colour, crate::engine::PictureColour::SRGB);
}

#[test]
fn the_id_of_a_picture_says_the_colour_it_is_made_in_so_that_a_changed_setting_does_not_use_the_pictures_of_the_one_before()
 {
    use crate::project::{ProjectSettings, WorkingColourSpace};
    let project = project_with_video(320, 180, false);
    let in_rec709 = plan_in_linear_light_of(&project);
    let in_acescg = plan_export(
        &project,
        ProjectSettings {
            working_colour_space: Some(WorkingColourSpace::AcesCg),
            ..ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS
        }
        .resolve(Settings::DEFAULT),
    )
    .unwrap();
    let compatibility = plan_of(&project);
    let id_in = |plan: &ExportPlan| {
        let script = script_of_pictures(255);
        let mut describer = describer_of(plan, &script, HashMap::new(), Box::new(NoFontsYet));
        let (described, _) = describe(&mut describer, 0);
        let LayerSource::Picture(id) = described.description.layers[0].source else {
            panic!("a picture")
        };
        id
    };
    let ids = [id_in(&compatibility), id_in(&in_rec709), id_in(&in_acescg)];
    assert_eq!(
        ids[0],
        PictureId(1),
        "in the compatibility mode the id is the serial number of the picture, as it was"
    );
    assert_ne!(ids[1], ids[0]);
    assert_ne!(ids[2], ids[0]);
    assert_ne!(
        ids[1], ids[2],
        "the same picture made for another working space is another picture to the engine"
    );
}

#[test]
fn a_picture_file_and_a_text_are_made_for_the_working_space_too() {
    use crate::engine::WorkingSpace;
    let (project, (first, _)) = video_with_a_text_and_a_picture_over_it(100);
    let plan = plan_in_linear_light_of(&project);
    let script = script_of_pictures(255);
    let image_source = SourceMediaIdentifier::new(2);
    let images = HashMap::from([(image_source, a_picture_file(100, 50))]);
    let calls = TextCalls::default();
    let mut describer = describer_of(&plan, &script, images, text_pictures_that_are_recorded(&calls));
    let mut every_picture_sent = Vec::new();
    for timeline_frame in 0..=first {
        every_picture_sent.extend(describe(&mut describer, timeline_frame).1);
    }
    assert!(
        every_picture_sent.len() >= 3,
        "a video picture, a text and a picture file were sent"
    );
    for (_, data) in &every_picture_sent {
        assert!(matches!(data.working, WorkingSpace::Linear(_)), "{:?}", data.working);
    }
}

// --- groups (phase 5): grouping and ungrouping do not change what the picture is, until the group has an effect ---

/// What is described for the frame `timeline_frame` of `project`: with a text and a picture file over a video, as `video_with_a_text_and_a_picture_over_it` makes it.
fn described_at(project: &Project, timeline_frame: u64) -> crate::engine::FrameDescription {
    let plan = plan_of(project);
    let script = script_of_pictures(255);
    let images = HashMap::from([(SourceMediaIdentifier::new(2), a_picture_file(100, 50))]);
    let calls = TextCalls::default();
    let mut describer = describer_of(&plan, &script, images, text_pictures_that_are_recorded(&calls));
    describe(&mut describer, timeline_frame).0.description
}

#[test]
fn a_group_with_no_effects_changes_nothing_in_the_frame_a_group_with_an_effect_is_a_layer_that_holds_its_members_and_ungrouping_changes_nothing()
 {
    use crate::project::{EffectKind, GroupKind, Member};
    let (mut project, _) = video_with_a_text_and_a_picture_over_it(60);
    let (image, text) = (clip_location(&project, 0, 0), clip_location(&project, 1, 0));
    let flat = described_at(&project, 65);
    assert_eq!(flat.layers.len(), 3, "the video, the text and the picture file");

    // Grouped, with no effect: pass-through, the same frame to the last number.
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(text), Member::Clip(image)])
        .unwrap();
    assert_eq!(
        described_at(&project, 65),
        flat,
        "grouping does not change what the picture is"
    );

    // With an effect: the video is where it was, and the other two are one layer, a group, that has the effect.
    project.add_effect_to_the_group(group, EffectKind::HUE_SHIFT).unwrap();
    project
        .set_effect_value_of_the_group(
            group,
            crate::test_support::HUE_SHIFT_AS_THE_FIRST_EFFECT,
            &PropertyValue::Number(90.0),
        )
        .unwrap();
    let grouped = described_at(&project, 65);
    assert_eq!(grouped.layers.len(), 2);
    assert_eq!(grouped.layers[0], flat.layers[0], "the video is not in the group");
    let LayerSource::Group(held) = &grouped.layers[1].source else {
        panic!("a group was expected: {:?}", grouped.layers[1].source);
    };
    assert_eq!(held.layers, flat.layers[1..], "the text and the picture, as they were");
    assert_eq!(grouped.layers[1].corners, CORNERS_OF_THE_WHOLE_PICTURE);
    assert_eq!(grouped.layers[1].effects.len(), 1);
    assert_eq!(grouped.layers[1].effects[0].key, COLOUR_MATRIX_KEY);
    // (Before the group is in the picture, where it is not seen, nothing is a group.)
    assert_eq!(described_at(&project, 10).layers.len(), 1);

    // Ungrouped: the frame it was.
    project.disband_group(group).unwrap();
    assert_eq!(described_at(&project, 65), flat, "ungrouping gives the same frame");
}

#[test]
fn an_adjustment_clip_is_a_group_of_what_is_behind_it_over_black_with_its_effects_for_as_long_as_it_lasts() {
    let (mut project, _) = video_with_a_text_and_a_picture_over_it(150);
    let front = project.add_track_in_front();
    let (adjustment, _) = project.try_insert_adjustment_clip(front, frame(60), 30).unwrap();
    put_a_hue_shift_on(&mut project, AnimationTarget::Clip(adjustment), 90.0);

    // Before it and after it the frame is what it was.
    assert_eq!(described_at(&project, 55).layers.len(), 2, "the video and the text");
    assert_eq!(described_at(&project, 95).layers.len(), 1, "the video");
    let during = described_at(&project, 65);

    assert_eq!(
        during.layers.len(),
        1,
        "a group, which is everything behind the adjustment clip"
    );
    assert_eq!(during.layers[0].effects.len(), 1);
    assert_eq!(during.layers[0].effects[0].key, COLOUR_MATRIX_KEY);
    let LayerSource::Group(held) = &during.layers[0].source else {
        panic!("a group was expected: {:?}", during.layers[0].source);
    };
    assert_eq!(
        held.layers[0].source,
        LayerSource::Solid([0, 0, 0, 255]),
        "over black first"
    );
    assert_eq!(held.layers.len(), 3, "the black, the video and the text");
}

// --- a transition, from the project to the pixels (phase 6.6) ---

/// A transition that is the mix of its two pictures by its progress.
const A_TRANSITION: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"startImage\", \"TYPE\": \"image\" }, { \"NAME\": \"endImage\", \"TYPE\": \"image\" }, { \"NAME\": \"progress\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 1.0, \"DEFAULT\": 0.0 } ] }*/\nvoid main() { gl_FragColor = mix(IMG_THIS_PIXEL(startImage), IMG_THIS_PIXEL(endImage), progress); }\n";

fn a_flat_opaque_picture(width: u32, height: u32, colour: [u8; 3]) -> RgbaPicture {
    RgbaPicture {
        width_in_pixels: width,
        height_in_pixels: height,
        rgba_pixels: [colour[0], colour[1], colour[2], 255].repeat((width * height) as usize),
    }
}

/// Describes `timeline_frame` of the export of `plan` and draws it on the software adapter, with the pictures that the describer sends (kept from one frame to the next, as the engine
/// keeps them) and the shaders that the frame asks for, which are taken off the shelf as the engine does. Gives the finished frame and the description.
fn drawn_by_the_engine(
    renderer: &mut crate::engine::FrameRenderer,
    describer: &mut FrameDescriber<'_, Script>,
    timeline_frame: u64,
) -> (crate::engine::RenderedFrame, FrameDescription) {
    let (described, sent) = describe(describer, timeline_frame);
    for (id, data) in &sent {
        renderer.upload_picture(*id, data).unwrap();
    }
    for layer in &described.description.flatten().layers {
        for effect in &layer.layer.effects {
            if let Some((source, structure)) = crate::engine::published_effect_shader_with_structure(&effect.key) {
                renderer
                    .install_effect_with_structure(
                        &effect.key,
                        &crate::engine::guard_wgsl(&source, crate::engine::DEFAULT_MOST_PASSES_OF_A_LOOP).unwrap(),
                        &structure,
                    )
                    .unwrap();
            }
        }
    }
    let (frame, statistics) = renderer
        .render(&described.description, std::time::Duration::from_secs(60))
        .unwrap();
    assert_eq!(statistics.effects_not_run, 0, "frame {timeline_frame}");
    (frame, described.description)
}

#[test]
fn a_clip_over_another_that_has_a_transition_with_its_progress_animated_comes_over_it_as_the_progress_goes_from_0_to_1()
{
    // The way that transitions are made: one clip on a track above another, and the effect on the clip on top. The one below is red, the one on top green, both are picture files that
    // are not the whole frame (100 by 50, in a frame that is 16 by 9), and the progress is 0 at the first frame of the clip on top and 1 forty frames later.
    let mut project = Project::empty();
    project.import_source(picture_source(2, 100, 50), frame(0)).unwrap();
    project.import_source(picture_source(3, 100, 50), frame(0)).unwrap();
    assert_eq!(
        project.tracks().len(),
        2,
        "a premise: the second picture file is on a track of its own, in front"
    );
    // (The tracks are listed from the front: the first is the one that was made last, which has the green picture.)
    let on_top = project.tracks()[0].placement_at_index(0).unwrap().id;
    assert_eq!(
        project
            .source(SourceMediaIdentifier::new(3))
            .map(|source| source.identifier),
        Some(SourceMediaIdentifier::new(3)),
        "a premise: the green picture is a source of the project"
    );
    let target = AnimationTarget::Clip(on_top);
    let file = crate::isf::read(A_TRANSITION).unwrap();
    let kind =
        crate::project::EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "mix.fs", A_TRANSITION)).unwrap();
    assert!(kind.is_installed() && kind.is_a_transition(), "a premise");
    let instance = project.add_effect(target, kind).unwrap();
    let progress = PropertyId::effect_value(kind, instance, 0);
    make_a_keyframe(&mut project, target, progress, 0, PropertyValue::Number(0.0));
    make_a_keyframe(&mut project, target, progress, 40, PropertyValue::Number(1.0));

    let plan = plan_export(&project, Settings::DEFAULT).unwrap();
    let script = script_of_pictures(0);
    let images = HashMap::from([
        (
            SourceMediaIdentifier::new(2),
            a_flat_opaque_picture(100, 50, [200, 40, 40]),
        ),
        (
            SourceMediaIdentifier::new(3),
            a_flat_opaque_picture(100, 50, [40, 200, 40]),
        ),
    ]);
    let mut describer = describer_of(&plan, &script, images, Box::new(NoFontsYet));
    crate::engine::testing::with_software_graphics!(graphics, {
        let mut renderer = crate::engine::FrameRenderer::new(graphics.device.clone());
        let mut centre_of = |timeline_frame: u64| {
            let (frame, description) = drawn_by_the_engine(&mut renderer, &mut describer, timeline_frame);
            assert!(
                description
                    .layers
                    .iter()
                    .any(|layer| matches!(layer.source, LayerSource::Group(_)) && layer.effects.len() == 1),
                "the clip with the transition is a group that has the transition: {description:?}"
            );
            frame.pixel(frame.width_in_pixels / 2, frame.height_in_pixels / 2)
        };
        let near = |got: [u8; 4], wanted: [u8; 4], when: &str| {
            let off = (0..4).map(|number| got[number].abs_diff(wanted[number])).max().unwrap();
            assert!(off <= 3, "{when}: {got:?} where {wanted:?} is wanted");
        };
        near(
            centre_of(0),
            [200, 40, 40, 255],
            "at progress 0 what is behind (the red one) is all that is seen",
        );
        near(
            centre_of(20),
            [120, 120, 40, 255],
            "at progress 0.5 the two are mixed half and half",
        );
        near(centre_of(10), [160, 80, 40, 255], "at progress 0.25");
        near(
            centre_of(40),
            [40, 200, 40, 255],
            "at progress 1 the clip on top is all that is seen",
        );
        near(centre_of(41), [40, 200, 40, 255], "and it stays so");
    });
}

#[test]
fn a_transition_on_a_clip_group_brings_the_picture_of_the_group_over_what_is_behind_it() {
    // Red at the back, and in front of it a group of a green clip and a blue one over it (which covers it): the group has the transition, at progress 0.5, which a group does not animate.
    let mut project = Project::empty();
    project.import_source(picture_source(2, 100, 50), frame(0)).unwrap();
    project.import_source(picture_source(3, 100, 50), frame(0)).unwrap();
    project.import_source(picture_source(4, 100, 50), frame(0)).unwrap();
    assert_eq!(project.tracks().len(), 3, "a premise");
    let (blue, green) = (
        project.tracks()[0].placement_at_index(0).unwrap().id,
        project.tracks()[1].placement_at_index(0).unwrap().id,
    );
    let group = project
        .create_group(
            crate::project::GroupKind::ClipGroup,
            &[crate::project::Member::Clip(green), crate::project::Member::Clip(blue)],
        )
        .unwrap();
    let file = crate::isf::read(A_TRANSITION).unwrap();
    let kind =
        crate::project::EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "mix.fs", A_TRANSITION)).unwrap();
    assert!(
        kind.applies_to_a_group_of_kind(crate::project::GroupKind::ClipGroup),
        "a premise"
    );
    let instance = project.add_effect_to_the_group(group, kind).unwrap();
    project
        .set_effect_value_of_the_group(
            group,
            PropertyId::effect_value(kind, instance, 0),
            &PropertyValue::Number(0.5),
        )
        .unwrap();

    let plan = plan_export(&project, Settings::DEFAULT).unwrap();
    let script = script_of_pictures(0);
    let images = HashMap::from([
        (
            SourceMediaIdentifier::new(2),
            a_flat_opaque_picture(100, 50, [200, 40, 40]),
        ),
        (
            SourceMediaIdentifier::new(3),
            a_flat_opaque_picture(100, 50, [40, 200, 40]),
        ),
        (
            SourceMediaIdentifier::new(4),
            a_flat_opaque_picture(100, 50, [40, 40, 200]),
        ),
    ]);
    let mut describer = describer_of(&plan, &script, images, Box::new(NoFontsYet));
    crate::engine::testing::with_software_graphics!(graphics, {
        let mut renderer = crate::engine::FrameRenderer::new(graphics.device.clone());
        let (frame, description) = drawn_by_the_engine(&mut renderer, &mut describer, 0);
        assert_eq!(
            description.layers.len(),
            2,
            "the red clip, and the group over it: {description:?}"
        );
        assert!(
            matches!(description.layers[1].source, LayerSource::Group(_)) && description.layers[1].effects.len() == 1,
            "the group has the transition, and it was not made a group of another"
        );
        let got = frame.pixel(frame.width_in_pixels / 2, frame.height_in_pixels / 2);
        let wanted = [120, 40, 120, 255];
        let off = (0..4).map(|number| got[number].abs_diff(wanted[number])).max().unwrap();
        assert!(
            off <= 3,
            "{got:?} where the red and the blue (the picture of the group) mixed half and half is {wanted:?}"
        );
    });
}

#[test]
fn a_transition_on_a_tag_is_on_each_clip_of_the_tag_and_each_clip_is_made_a_group_for_it() {
    let mut project = Project::empty();
    project.import_source(picture_source(2, 100, 50), frame(0)).unwrap();
    project.import_source(picture_source(3, 100, 50), frame(0)).unwrap();
    let (front, back) = (
        project.tracks()[0].placement_at_index(0).unwrap().id,
        project.tracks()[1].placement_at_index(0).unwrap().id,
    );
    let tag = project
        .create_group(
            crate::project::GroupKind::Tag,
            &[crate::project::Member::Clip(front), crate::project::Member::Clip(back)],
        )
        .unwrap();
    let file = crate::isf::read(A_TRANSITION).unwrap();
    let kind =
        crate::project::EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "mix.fs", A_TRANSITION)).unwrap();
    project.add_effect_to_the_group(tag, kind).unwrap();

    let plan = plan_export(&project, Settings::DEFAULT).unwrap();
    let script = script_of_pictures(0);
    let images = HashMap::from([
        (
            SourceMediaIdentifier::new(2),
            a_flat_opaque_picture(100, 50, [200, 40, 40]),
        ),
        (
            SourceMediaIdentifier::new(3),
            a_flat_opaque_picture(100, 50, [40, 200, 40]),
        ),
    ]);
    let mut describer = describer_of(&plan, &script, images, Box::new(NoFontsYet));
    let (described, _) = describe(&mut describer, 0);
    let layers = &described.description.layers;
    assert_eq!(layers.len(), 2);
    for layer in layers {
        assert!(
            matches!(layer.source, LayerSource::Group(_)),
            "each clip is made a group: {layer:?}"
        );
        assert_eq!(layer.effects.len(), 1, "with the transition of the tag: {layer:?}");
        assert!(layer.effects[0].key.starts_with("isf:mix-"));
    }
}
