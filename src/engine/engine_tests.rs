// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tests of the engine that use a graphics device: always a software one (see `testing`).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::testing::{Gate, software_graphics, software_graphics_with_a_device_of_its_own, with_software_graphics};
use super::{
    AdapterChoice, DeviceTrouble, Engine, EngineEvent, EngineSettings, FrameDescription, FrameRenderer, RenderProblem,
};

const A_LONG_TIME: Duration = Duration::from_secs(60);

fn renderer(graphics: &super::testing::SoftwareGraphics) -> FrameRenderer {
    FrameRenderer::new(graphics.device.clone())
}

/// The events of `engine` until `count` of them have come (or the time is up, which fails the test).
fn events_of(engine: &Engine, count: usize) -> Vec<EngineEvent> {
    let started = Instant::now();
    let mut events = Vec::new();
    while events.len() < count {
        match engine.try_next_event() {
            Some(event) => events.push(event),
            None => {
                assert!(
                    started.elapsed() < A_LONG_TIME,
                    "only {} of the {count} events came: {events:?}",
                    events.len()
                );
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
    events
}

fn software_engine() -> (Engine, Arc<AtomicUsize>) {
    software_engine_with(EngineSettings::new(AdapterChoice::SoftwareOnly))
}

fn software_engine_with(settings: EngineSettings) -> (Engine, Arc<AtomicUsize>) {
    let woken = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&woken);
    let engine = Engine::start(
        settings,
        Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }),
    );
    (engine, woken)
}

#[test]
fn a_frame_with_nothing_in_it_is_black_and_opaque_at_every_size_even_where_the_rows_need_padding() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer(&graphics);
        // (A row of 7 pixels is 28 bytes, which the card pads to 256; 65 pixels are 260, padded to 512.)
        for (width, height) in [(1, 1), (7, 5), (64, 36), (65, 2), (257, 3)] {
            let (frame, statistics) = renderer
                .render(&FrameDescription::black((width, height)), A_LONG_TIME)
                .unwrap();
            assert_eq!((frame.width_in_pixels, frame.height_in_pixels), (width, height));
            assert_eq!(frame.rgba_pixels.len(), (width * height * 4) as usize);
            assert!(
                frame.every_pixel_is([0, 0, 0, 255]),
                "{width} by {height} is not all opaque black"
            );
            assert_eq!(statistics.passes_recorded, 2);
            assert!(!statistics.came_from_the_cache);
        }
    });
}

#[test]
fn the_textures_of_a_frame_are_used_again_by_the_next_frame_of_the_same_size_and_not_by_one_of_another_size() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer(&graphics);
        renderer
            .render(&FrameDescription::black((32, 18)), A_LONG_TIME)
            .unwrap();
        let first = renderer.pool_statistics();
        assert_eq!(
            (first.made, first.reused),
            (2, 0),
            "the picture put together, and the output"
        );
        renderer
            .render(&FrameDescription::black((32, 18)), A_LONG_TIME)
            .unwrap();
        let second = renderer.pool_statistics();
        assert_eq!((second.made, second.reused), (2, 2));
        renderer
            .render(&FrameDescription::black((33, 18)), A_LONG_TIME)
            .unwrap();
        let third = renderer.pool_statistics();
        assert_eq!(
            (third.made, third.reused),
            (4, 2),
            "a frame of another size needs textures of its own"
        );
    });
}

#[test]
fn the_read_back_buffers_are_kept_for_the_next_frame_but_not_without_limit() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer(&graphics);
        renderer
            .render(&FrameDescription::black((20, 10)), A_LONG_TIME)
            .unwrap();
        assert_eq!(renderer.readback_buffers_waiting(), 1);
        renderer
            .render(&FrameDescription::black((20, 10)), A_LONG_TIME)
            .unwrap();
        assert_eq!(
            renderer.readback_buffers_waiting(),
            1,
            "the same buffer again, not a second one"
        );
        // (Buffers have the size of the padded rows, so widths that pad to the same row are the same size: these four need rows of 512, 768, 1024
        // and 1280 bytes, which with the 256 of the first make five sizes.)
        for width in [65, 129, 193, 257] {
            renderer
                .render(&FrameDescription::black((width, 10)), A_LONG_TIME)
                .unwrap();
        }
        assert_eq!(
            renderer.readback_buffers_waiting(),
            2,
            "only the last two of the five sizes are kept"
        );
    });
}

#[test]
fn a_frame_that_is_empty_or_too_big_is_refused_and_the_words_give_its_size() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer(&graphics);
        let largest = graphics.device.largest_texture_side;
        for (width, height) in [(0, 10), (10, 0), (largest + 1, 4), (4, largest + 1)] {
            let problem = renderer
                .render(&FrameDescription::black((width, height)), A_LONG_TIME)
                .unwrap_err();
            assert!(matches!(problem, RenderProblem::BadSize { .. }), "{problem:?}");
            let words = problem.to_string();
            assert!(words.contains(&format!("{width} by {height}")), "{words}");
            assert!(words.contains(&largest.to_string()), "{words}");
        }
        // The renderer goes on after a refusal.
        assert!(renderer.render(&FrameDescription::black((4, 4)), A_LONG_TIME).is_ok());
    });
}

#[test]
fn the_device_says_what_it_can_do_in_numbers_that_make_sense() {
    with_software_graphics!(graphics, {
        let device = &graphics.device;
        assert!(device.uniform_alignment.is_power_of_two());
        assert!(
            (16..=1024).contains(&device.uniform_alignment),
            "{}",
            device.uniform_alignment
        );
        assert!(device.largest_texture_side >= 2048, "{}", device.largest_texture_side);
        assert_eq!(
            device.adapter_information.device_type,
            wgpu::DeviceType::Cpu,
            "the tests only use a software adapter"
        );
    });
}

#[test]
fn a_mistake_in_what_the_card_is_asked_is_caught_by_the_scope_and_does_not_reach_the_handler_of_the_window() {
    with_software_graphics!(graphics, {
        let device = &graphics.device;
        let (_, mistake) = device.catching_errors(|| {
            device.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("a texture with no width"),
                size: wgpu::Extent3d {
                    width: 0,
                    height: 4,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        });
        assert!(mistake.is_some_and(|words| !words.is_empty()));
        assert_eq!(device.take_trouble(), Vec::new(), "nothing got past the scope");
        let (_, nothing_wrong) = device.catching_errors(|| 1 + 1);
        assert_eq!(nothing_wrong, None);
    });
}

#[test]
fn a_device_that_is_destroyed_says_it_was_lost() {
    let Some(graphics) = software_graphics_with_a_device_of_its_own() else {
        return;
    };
    graphics.device.destroy();
    let started = Instant::now();
    let mut trouble = Vec::new();
    while trouble.is_empty() && started.elapsed() < Duration::from_secs(10) {
        let _ = graphics.device.device.poll(wgpu::PollType::Poll);
        trouble = graphics.device.take_trouble();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        matches!(trouble.first(), Some(DeviceTrouble::Lost { reason, .. }) if reason.contains("Destroyed")),
        "{trouble:?}"
    );
}

#[test]
fn the_engine_makes_the_frame_it_is_asked_for_and_says_which_request_it_answers_and_wakes_the_window() {
    let Some(_graphics) = software_graphics() else { return };
    let (mut engine, woken) = software_engine();
    let serial = engine.request_frame(FrameDescription::black((48, 27)));
    assert_eq!(serial, 1);
    let events = events_of(&engine, 1);
    let EngineEvent::Frame {
        serial: answered,
        frame,
        statistics,
    } = &events[0]
    else {
        panic!("{events:?}")
    };
    assert_eq!(*answered, 1);
    assert_eq!((frame.width_in_pixels, frame.height_in_pixels), (48, 27));
    assert!(frame.every_pixel_is([0, 0, 0, 255]));
    assert_eq!(statistics.passes_recorded, 2);
    assert!(!statistics.came_from_the_cache);
    assert_eq!(
        woken.load(Ordering::SeqCst),
        1,
        "the window is woken once for the one event"
    );
    assert!(engine.is_running());
}

#[test]
fn the_same_frame_asked_for_again_comes_from_the_cache_and_another_frame_does_not() {
    let Some(_graphics) = software_graphics() else { return };
    let (mut engine, _) = software_engine();
    engine.request_frame(FrameDescription::black((16, 9)));
    let first = events_of(&engine, 1);
    engine.request_frame(FrameDescription::black((16, 9)));
    let second = events_of(&engine, 1);
    engine.request_frame(FrameDescription::black((17, 9)));
    let third = events_of(&engine, 1);
    let (
        EngineEvent::Frame {
            frame: first_frame,
            statistics: first_statistics,
            ..
        },
        EngineEvent::Frame {
            frame: second_frame,
            statistics: second_statistics,
            serial: second_serial,
        },
        EngineEvent::Frame {
            statistics: third_statistics,
            ..
        },
    ) = (&first[0], &second[0], &third[0])
    else {
        panic!("{first:?} {second:?} {third:?}")
    };
    assert!(!first_statistics.came_from_the_cache);
    assert!(second_statistics.came_from_the_cache);
    assert_eq!(second_statistics.passes_recorded, 0, "nothing was drawn");
    assert_eq!(*second_serial, 2);
    assert!(
        Arc::ptr_eq(first_frame, second_frame),
        "the very same frame is given again"
    );
    assert!(!third_statistics.came_from_the_cache, "another size is another frame");
}

#[test]
fn of_the_requests_that_have_piled_up_only_the_newest_is_made_and_the_others_are_not_answered() {
    let Some(_graphics) = software_graphics() else { return };
    let gate = Gate::closed();
    let mut settings = EngineSettings::new(AdapterChoice::SoftwareOnly);
    settings.hold_the_first_batch_until = Some(Arc::clone(&gate));
    let (mut engine, _) = software_engine_with(settings);
    let serials: Vec<u64> = (1..=5)
        .map(|number| engine.request_frame(FrameDescription::black((8 * number, 8))))
        .collect();
    assert_eq!(
        serials,
        vec![1, 2, 3, 4, 5],
        "serial numbers go up by one for each request"
    );
    gate.open();
    let events = events_of(&engine, 1);
    let EngineEvent::Frame { serial, frame, .. } = &events[0] else {
        panic!("{events:?}")
    };
    assert_eq!(*serial, 5);
    assert_eq!(
        frame.width_in_pixels, 40,
        "the newest request, the fifth, is the one that was made"
    );
    // And nothing else comes for the four that were passed over.
    engine.request_frame(FrameDescription::black((8, 8)));
    let after = events_of(&engine, 1);
    let EngineEvent::Frame { serial, .. } = &after[0] else {
        panic!("{after:?}")
    };
    assert_eq!(*serial, 6, "the next answer is for the next request");
}

#[test]
fn a_frame_the_engine_cannot_make_is_reported_with_its_serial_number_and_the_engine_goes_on() {
    let Some(_graphics) = software_graphics() else { return };
    let (mut engine, _) = software_engine();
    let serial = engine.request_frame(FrameDescription::black((0, 5)));
    let events = events_of(&engine, 1);
    let EngineEvent::FrameFailed {
        serial: failed,
        problem,
    } = &events[0]
    else {
        panic!("{events:?}")
    };
    assert_eq!(*failed, serial);
    assert!(problem.contains("0 by 5"), "{problem}");
    engine.request_frame(FrameDescription::black((5, 5)));
    let events = events_of(&engine, 1);
    assert!(matches!(&events[0], EngineEvent::Frame { serial: 2, .. }), "{events:?}");
}

#[test]
fn an_engine_that_cannot_make_its_device_says_so_and_stops() {
    let (mut engine, woken) = software_engine_with(EngineSettings::new(AdapterChoice::NoAdapterAtAll));
    let events = events_of(&engine, 1);
    let EngineEvent::Stopped { problem } = &events[0] else {
        panic!("{events:?}")
    };
    assert!(problem.contains("could not start"), "{problem}");
    assert!(problem.contains("no adapter at all"), "{problem}");
    assert_eq!(woken.load(Ordering::SeqCst), 1);
    let started = Instant::now();
    while engine.is_running() && started.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!engine.is_running());
    // Asking a stopped engine for a frame does no harm.
    engine.request_frame(FrameDescription::black((4, 4)));
    assert!(engine.try_next_event().is_none());
}

#[test]
fn dropping_the_engine_ends_its_thread_without_waiting_for_anything_that_is_asked() {
    let Some(_graphics) = software_graphics() else { return };
    let (mut engine, _) = software_engine();
    engine.request_frame(FrameDescription::black((8, 8)));
    events_of(&engine, 1);
    let started = Instant::now();
    drop(engine);
    assert!(started.elapsed() < Duration::from_secs(5), "{:?}", started.elapsed());
}
