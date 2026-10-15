// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tests of sending pictures to the engine, keeping them, and drawing frames with them (the placing itself is checked against the window's own
//! renderer in `oracle_tests`).

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::time::{Duration, Instant};

use super::oracle_tests::{TestLayer, TestSource, engine_frame, upright};
use super::testing::{software_graphics, with_software_graphics};
use super::{
    AdapterChoice, Engine, EngineEvent, EngineSettings, FrameDescription, FrameRenderer, LayerDescription, LayerSource,
    PictureData, PictureId, PicturePixels, RenderProblem, UniformBytes,
};

const A_LONG_TIME: Duration = Duration::from_secs(60);

fn layer_of(picture: u64) -> LayerDescription {
    LayerDescription {
        source: LayerSource::Picture(PictureId(picture)),
        corners: [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)],
        effects: Vec::new(),
    }
}

fn frame_of(size: (u32, u32), pictures: &[u64]) -> FrameDescription {
    FrameDescription {
        render_size: size,
        layers: pictures.iter().map(|&picture| layer_of(picture)).collect(),
    }
}

/// A picture of one colour, `side` pixels across and down.
fn flat(side: u32, colour: [u8; 4]) -> PictureData {
    PictureData::rgba8(side, side, colour.repeat((side * side) as usize)).unwrap()
}

#[test]
fn a_picture_that_was_sent_is_on_the_card_with_its_size_and_a_frame_may_use_it() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        assert_eq!(renderer.picture_size(PictureId(1)), None);
        renderer
            .upload_picture(PictureId(1), &flat(4, [10, 200, 30, 255]))
            .unwrap();
        assert_eq!(renderer.picture_size(PictureId(1)), Some((4, 4)));
        let (frame, _) = renderer.render(&frame_of((4, 4), &[1]), A_LONG_TIME).unwrap();
        assert!(frame.every_pixel_is([10, 200, 30, 255]));
        // 4 by 4 pixels of 8 bytes each on the card.
        assert_eq!(renderer.pictures_on_the_card(), (1, 128));
    });
}

#[test]
fn a_frame_that_uses_pictures_that_are_not_there_says_which_each_once_and_draws_nothing() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        renderer.upload_picture(PictureId(2), &flat(2, [1, 2, 3, 255])).unwrap();
        // 9 is used by two layers and 5 by one, 2 is there.
        let problem = renderer
            .render(&frame_of((8, 8), &[9, 2, 5, 9]), A_LONG_TIME)
            .unwrap_err();
        let RenderProblem::PicturesMissing(missing) = &problem else {
            panic!("{problem:?}")
        };
        assert_eq!(missing, &vec![PictureId(9), PictureId(5)]);
        assert!(problem.to_string().contains("2 of the pictures"), "{problem}");
        // A frame with no picture that is missing is made all the same.
        assert!(renderer.render(&frame_of((8, 8), &[2]), A_LONG_TIME).is_ok());
        // And so is one that has no pictures at all, only colours.
        let mut colours = FrameDescription::black((8, 8));
        colours.layers.push(LayerDescription {
            source: LayerSource::Solid([9, 9, 9, 255]),
            corners: [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)],
            effects: Vec::new(),
        });
        assert!(renderer.render(&colours, A_LONG_TIME).is_ok());
    });
}

#[test]
fn forgetting_a_picture_makes_it_missing_and_forgetting_one_that_is_not_there_does_no_harm() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        renderer.upload_picture(PictureId(1), &flat(2, [1, 1, 1, 255])).unwrap();
        renderer.upload_picture(PictureId(2), &flat(2, [2, 2, 2, 255])).unwrap();
        renderer.forget_pictures([PictureId(1), PictureId(77)]);
        assert_eq!(renderer.picture_size(PictureId(1)), None);
        assert_eq!(renderer.picture_size(PictureId(2)), Some((2, 2)));
        assert_eq!(renderer.pictures_on_the_card(), (1, 32));
        assert!(matches!(
            renderer.render(&frame_of((2, 2), &[1]), A_LONG_TIME),
            Err(RenderProblem::PicturesMissing(_))
        ));
        assert!(renderer.render(&frame_of((2, 2), &[2]), A_LONG_TIME).is_ok());
    });
}

#[test]
fn when_the_card_has_no_room_for_all_the_pictures_the_one_that_no_frame_used_for_the_longest_goes() {
    with_software_graphics!(graphics, {
        // A picture of 8 by 8 takes 512 bytes on the card: a budget of 1100 has room for two.
        let mut renderer = FrameRenderer::with_picture_budget(graphics.device.clone(), 1100);
        renderer.upload_picture(PictureId(1), &flat(8, [1, 1, 1, 255])).unwrap();
        renderer.upload_picture(PictureId(2), &flat(8, [2, 2, 2, 255])).unwrap();
        // A frame that uses 1 counts it as used just now.
        renderer.render(&frame_of((8, 8), &[1]), A_LONG_TIME).unwrap();
        renderer.upload_picture(PictureId(3), &flat(8, [3, 3, 3, 255])).unwrap();
        assert_eq!(
            renderer.picture_size(PictureId(2)),
            None,
            "2 was used least recently, and went"
        );
        assert_eq!(renderer.picture_size(PictureId(1)), Some((8, 8)));
        assert_eq!(renderer.picture_size(PictureId(3)), Some((8, 8)));
        assert_eq!(renderer.pictures_on_the_card(), (2, 1024));
    });
}

#[test]
fn a_picture_sent_again_under_the_same_id_is_the_picture_from_then_on() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        renderer
            .upload_picture(PictureId(1), &flat(2, [200, 0, 0, 255]))
            .unwrap();
        renderer
            .upload_picture(PictureId(1), &flat(2, [0, 0, 200, 255]))
            .unwrap();
        let (frame, _) = renderer.render(&frame_of((2, 2), &[1]), A_LONG_TIME).unwrap();
        assert!(frame.every_pixel_is([0, 0, 200, 255]));
        assert_eq!(
            renderer.pictures_on_the_card(),
            (1, 32),
            "the old one is not kept as well"
        );
    });
}

#[test]
fn a_picture_that_cannot_be_used_is_refused_and_not_kept() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let wrong_number_of_bytes = PictureData {
            width_in_pixels: 4,
            height_in_pixels: 4,
            pixels: PicturePixels::Rgba8 {
                straight_alpha: vec![0; 10],
            },
        };
        let empty = PictureData {
            width_in_pixels: 0,
            height_in_pixels: 4,
            pixels: PicturePixels::Rgba8 { straight_alpha: vec![] },
        };
        let largest = graphics.device.largest_texture_side;
        let wider_than_the_card_takes = PictureData {
            width_in_pixels: largest + 1,
            height_in_pixels: 1,
            pixels: PicturePixels::Rgba8 {
                straight_alpha: vec![0; (largest as usize + 1) * 4],
            },
        };
        for (what, picture) in [
            ("wrong number of bytes", wrong_number_of_bytes),
            ("empty", empty),
            ("too wide", wider_than_the_card_takes),
        ] {
            let problem = renderer.upload_picture(PictureId(1), &picture).unwrap_err();
            assert!(matches!(problem, RenderProblem::BadPicture(_)), "{what}: {problem:?}");
        }
        assert_eq!(renderer.pictures_on_the_card(), (0, 0));
    });
}

#[test]
fn every_layer_of_a_frame_has_the_numbers_of_its_own_whatever_the_card() {
    // (On the software adapter of Windows, two draws that used different uniform buffers both got the data of the last one: the layers' numbers are
    // in one buffer, each at an offset of its own, and this is the test that would show it if that came back.)
    with_software_graphics!(graphics, {
        let size = (60, 10);
        let layers: Vec<TestLayer> = (0..6u32)
            .map(|number| TestLayer {
                source: TestSource::Solid([40 * number as u8 + 20, 250 - 40 * number as u8, 100, 255]),
                corners: upright(number as f32 * 0.1, 0.0, 0.1, 1.0),
            })
            .collect();
        let frame = engine_frame(&graphics, size, &layers);
        for number in 0..6usize {
            let first = (5 * 60 + number * 6 + 3) * 4; // (each layer is a tenth of the 60 pixels across: 6 pixels)
            let expected = [40 * number as u8 + 20, 250 - 40 * number as u8, 100, 255];
            assert_eq!(
                &frame[first..first + 4],
                &expected,
                "the layer {number} has its own colour and its own place"
            );
        }
    });
}

#[test]
fn a_frame_of_very_many_layers_needs_more_room_for_its_numbers_than_the_buffer_starts_with_and_gets_it() {
    with_software_graphics!(graphics, {
        // 40 layers at 256 bytes apart are 10 240 bytes, more than the 4096 the buffer starts with.
        let size = (40, 4);
        let layers: Vec<TestLayer> = (0..40u32)
            .map(|number| TestLayer {
                source: TestSource::Solid([number as u8 * 6, 255 - number as u8 * 6, 77, 255]),
                corners: upright(number as f32 / 40.0, 0.0, 1.0 / 40.0, 1.0),
            })
            .collect();
        let frame = engine_frame(&graphics, size, &layers);
        for number in 0..40usize {
            let first = (2 * 40 + number) * 4;
            assert_eq!(
                &frame[first..first + 4],
                &[number as u8 * 6, 255 - number as u8 * 6, 77, 255],
                "layer {number}"
            );
        }
    });
}

#[test]
fn the_buffer_of_numbers_is_made_bigger_when_a_frame_needs_more_and_keeps_its_room_for_smaller_frames() {
    with_software_graphics!(graphics, {
        let mut arena = super::uniform_arena::UniformArena::default();
        let device = &graphics.device;
        let slots = |count: usize| {
            let mut bytes = UniformBytes::new(device.uniform_alignment);
            for _ in 0..count {
                bytes.add_slot(|writer| {
                    writer.f32(1.0);
                });
            }
            bytes
        };
        assert!(
            arena
                .upload(&device.device, &device.queue, &UniformBytes::new(256))
                .is_none(),
            "no numbers, no buffer"
        );
        assert!(arena.upload(&device.device, &device.queue, &slots(2)).is_some());
        let small = arena.capacity_in_bytes();
        assert_eq!(small, 4096);
        arena.upload(&device.device, &device.queue, &slots(40));
        let big = arena.capacity_in_bytes();
        assert!(
            big >= (40 * device.uniform_alignment) as u64 && big.is_power_of_two(),
            "{big}"
        );
        arena.upload(&device.device, &device.queue, &slots(2));
        assert_eq!(
            arena.capacity_in_bytes(),
            big,
            "a smaller frame afterwards does not make it smaller"
        );
    });
}

// ---- through the engine's thread ----

fn started_engine() -> (Engine, Arc<AtomicUsize>) {
    let woken = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&woken);
    let engine = Engine::start(
        EngineSettings::new(AdapterChoice::SoftwareOnly),
        Arc::new(move || {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }),
    );
    (engine, woken)
}

fn next_event(engine: &Engine) -> EngineEvent {
    let started = Instant::now();
    loop {
        if let Some(event) = engine.try_next_event() {
            return event;
        }
        assert!(started.elapsed() < A_LONG_TIME, "no event came");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn a_frame_that_uses_a_picture_not_sent_comes_back_as_missing_and_is_made_once_the_picture_is_sent() {
    let Some(_graphics) = software_graphics() else { return };
    let (mut engine, _) = started_engine();
    let serial = engine.request_frame(frame_of((4, 4), &[7]));
    let EngineEvent::PicturesMissing { serial: answered, ids } = next_event(&engine) else {
        panic!("the picture was never sent")
    };
    assert_eq!((answered, ids), (serial, vec![PictureId(7)]));

    engine.upload_picture(PictureId(7), Arc::new(flat(2, [90, 80, 70, 255])));
    let serial = engine.request_frame(frame_of((4, 4), &[7]));
    let EngineEvent::Frame {
        serial: answered,
        frame,
        ..
    } = next_event(&engine)
    else {
        panic!("the picture was sent")
    };
    assert_eq!(answered, serial);
    assert!(frame.every_pixel_is([90, 80, 70, 255]));
}

#[test]
fn a_picture_that_cannot_be_used_is_reported_by_its_id() {
    let Some(_graphics) = software_graphics() else { return };
    let (engine, _) = started_engine();
    engine.upload_picture(
        PictureId(3),
        Arc::new(PictureData {
            width_in_pixels: 5,
            height_in_pixels: 5,
            pixels: PicturePixels::Rgba8 {
                straight_alpha: vec![1],
            },
        }),
    );
    let EngineEvent::PictureRejected { id, problem } = next_event(&engine) else {
        panic!("a picture with one byte is not a picture of 5 by 5")
    };
    assert_eq!(id, PictureId(3));
    assert!(problem.contains("100 bytes"), "{problem}");
}

#[test]
fn forgetting_a_picture_through_the_engine_makes_frames_that_use_it_missing_again() {
    let Some(_graphics) = software_graphics() else { return };
    let (mut engine, _) = started_engine();
    engine.upload_picture(PictureId(1), Arc::new(flat(2, [5, 6, 7, 255])));
    engine.request_frame(frame_of((2, 2), &[1]));
    assert!(matches!(next_event(&engine), EngineEvent::Frame { .. }));
    engine.forget_pictures(vec![PictureId(1)]);
    // (The same frame as before: the cache of finished frames has it, and gives it: a frame that was made is still the frame.)
    engine.request_frame(frame_of((2, 2), &[1]));
    let EngineEvent::Frame { statistics, .. } = next_event(&engine) else {
        panic!("the frame was kept")
    };
    assert!(statistics.came_from_the_cache);
    // A frame of another size that uses it is not in the cache, and the picture is gone.
    engine.request_frame(frame_of((3, 3), &[1]));
    assert!(matches!(next_event(&engine), EngineEvent::PicturesMissing { .. }));
}

#[test]
fn sending_a_picture_again_under_the_same_id_does_not_let_an_old_frame_be_given_in_its_place() {
    let Some(_graphics) = software_graphics() else { return };
    let (mut engine, _) = started_engine();
    engine.upload_picture(PictureId(1), Arc::new(flat(2, [200, 0, 0, 255])));
    engine.request_frame(frame_of((2, 2), &[1]));
    let EngineEvent::Frame { frame, .. } = next_event(&engine) else {
        panic!()
    };
    assert!(frame.every_pixel_is([200, 0, 0, 255]));
    engine.upload_picture(PictureId(1), Arc::new(flat(2, [0, 0, 200, 255])));
    engine.request_frame(frame_of((2, 2), &[1]));
    let EngineEvent::Frame { frame, statistics, .. } = next_event(&engine) else {
        panic!()
    };
    assert!(!statistics.came_from_the_cache);
    assert!(
        frame.every_pixel_is([0, 0, 200, 255]),
        "the new picture, not the frame that was made of the old one"
    );
}
