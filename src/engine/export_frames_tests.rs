// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The frames of the export, which are asked for in order and all of them made: begun one after another and finished afterwards, and asked of the engine.
//! (What the engine's thread does when one goes wrong is in `worker/tests.rs`.)

use std::sync::Arc;
use std::time::Duration;

use super::engine_tests::{events_of, software_engine_with};
use super::testing::{Gate, software_graphics, with_software_graphics};
use super::yuv::reference_yuv420p;
use super::{
    AdapterChoice, EngineEvent, EngineSettings, FrameDescription, FrameOutput, FrameRenderer, LayerDescription,
    LayerSource, PendingFrame, PictureData, PictureId, RenderedPlanes, YuvColour,
};

const A_LONG_TIME: Duration = Duration::from_secs(60);
const WHOLE: [(f32, f32); 4] = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];

/// A frame that is one flat grey of this level, 8 by 8 pixels.
fn grey_frame(level: u8) -> FrameDescription {
    FrameDescription {
        colour: Default::default(),
        render_size: (8, 8),
        layers: vec![LayerDescription {
            source: LayerSource::Solid([level, level, level, 255]),
            corners: WHOLE,
            effects: Vec::new(),
        }],
    }
}

/// The brightness (in the limited range of BT.601) that a flat grey of this level has in the planes.
fn brightness_of_grey(level: u8) -> u8 {
    reference_yuv420p(1, 1, &[level, level, level, 255], YuvColour::UNSPECIFIED).0[0]
}

#[test]
fn frames_that_are_begun_one_after_another_and_finished_afterwards_are_what_they_are_when_they_are_made_one_at_a_time()
{
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let mut bytes = Vec::new();
        for down in 0..16u32 {
            for across in 0..16u32 {
                bytes.extend([(across * 16) as u8, (down * 16) as u8, ((across + down) * 8) as u8, 255]);
            }
        }
        renderer
            .upload_picture(PictureId(1), &PictureData::rgba8(16, 16, bytes).unwrap())
            .unwrap();
        // Four frames that differ in where the picture is and in what is over it, so that the numbers each is given (they are kept in one buffer, one after the
        // other) are different, and a frame given another's would show.
        let descriptions: Vec<FrameDescription> = (0..4)
            .map(|number| {
                let shift = number as f32 * 0.1;
                FrameDescription {
                    colour: Default::default(),
                    render_size: (48, 27),
                    layers: vec![
                        LayerDescription {
                            source: LayerSource::Picture(PictureId(1)),
                            corners: [(shift, 0.0), (1.0, 0.0), (1.0, 1.0 - shift), (0.0, 1.0)],
                            effects: Vec::new(),
                        },
                        LayerDescription {
                            source: LayerSource::Solid([200 - number * 40, number * 60, 90, 128]),
                            corners: [(0.5, 0.5), (1.0, 0.5), (1.0, 1.0), (0.5, 1.0)],
                            effects: Vec::new(),
                        },
                    ],
                }
            })
            .collect();
        let colour = YuvColour::UNSPECIFIED;
        let one_at_a_time: Vec<RenderedPlanes> = descriptions
            .iter()
            .map(|description| renderer.render_planes(description, colour, A_LONG_TIME).unwrap().0)
            .collect();
        let on_their_way: Vec<PendingFrame> = descriptions
            .iter()
            .map(|description| {
                renderer
                    .begin_frame(description, FrameOutput::Planes(colour), A_LONG_TIME)
                    .unwrap()
            })
            .collect();
        let together: Vec<RenderedPlanes> = on_their_way
            .into_iter()
            .map(|pending| renderer.finish_frame(pending).unwrap().0.into_planes())
            .collect();
        assert_eq!(together, one_at_a_time);
        for number in 1..4 {
            assert_ne!(
                together[number],
                together[number - 1],
                "the frames are not all alike, or this shows nothing"
            );
        }
    });
}

#[test]
fn every_frame_of_the_export_is_made_in_the_order_it_was_asked_for_however_many_are_waiting() {
    let Some(_graphics) = software_graphics() else { return };
    let gate = Gate::closed();
    let mut settings = EngineSettings::new(AdapterChoice::SoftwareOnly);
    settings.hold_the_first_batch_until = Some(Arc::clone(&gate));
    let (mut engine, _) = software_engine_with(settings);
    let serials: Vec<u64> = (0..30u8)
        .map(|number| engine.request_export_frame(grey_frame(number * 8), YuvColour::UNSPECIFIED))
        .collect();
    assert_eq!(serials, (1..=30).collect::<Vec<u64>>());
    // (Thirty requests are waiting when the engine looks: the preview would have been given the last of them and no other.)
    gate.open();
    let events = events_of(&engine, 30);
    for (number, event) in events.iter().enumerate() {
        let EngineEvent::ExportFrame {
            serial,
            planes,
            statistics,
        } = event
        else {
            panic!("{number}: {event:?}")
        };
        assert_eq!(*serial, number as u64 + 1, "in the order they were asked in");
        assert_eq!((planes.width_in_pixels, planes.height_in_pixels), (8, 8));
        let expected = brightness_of_grey(number as u8 * 8);
        assert!(
            planes.luma().iter().all(|level| level.abs_diff(expected) <= 1),
            "frame {number}: {:?} is not {expected}",
            planes.luma()
        );
        assert!(!statistics.came_from_the_cache);
        assert!(statistics.passes_recorded > 0);
    }
}

#[test]
fn the_same_frame_asked_for_again_for_the_export_is_made_again_and_not_taken_from_the_cache() {
    let Some(_graphics) = software_graphics() else { return };
    let (mut engine, _) = software_engine_with(EngineSettings::new(AdapterChoice::SoftwareOnly));
    for _ in 0..2 {
        engine.request_export_frame(grey_frame(100), YuvColour::UNSPECIFIED);
    }
    for event in events_of(&engine, 2) {
        let EngineEvent::ExportFrame { statistics, .. } = event else {
            panic!("{event:?}")
        };
        assert!(!statistics.came_from_the_cache);
        assert!(statistics.passes_recorded > 0, "something was drawn");
    }
}

#[test]
fn a_frame_of_the_export_that_cannot_be_made_is_answered_in_its_place_and_the_frames_around_it_are_made() {
    let Some(_graphics) = software_graphics() else { return };
    let gate = Gate::closed();
    let mut settings = EngineSettings::new(AdapterChoice::SoftwareOnly);
    settings.hold_the_first_batch_until = Some(Arc::clone(&gate));
    let (mut engine, _) = software_engine_with(settings);
    let colour = YuvColour::UNSPECIFIED;
    engine.request_export_frame(grey_frame(20), colour);
    // The second needs a picture that was never sent, and the fourth is of no size.
    let mut needs_a_picture = grey_frame(30);
    needs_a_picture.layers.push(LayerDescription {
        source: LayerSource::Picture(PictureId(7)),
        corners: WHOLE,
        effects: Vec::new(),
    });
    engine.request_export_frame(needs_a_picture, colour);
    engine.request_export_frame(grey_frame(40), colour);
    engine.request_export_frame(FrameDescription::black((0, 8)), colour);
    engine.request_export_frame(grey_frame(50), colour);
    gate.open();
    let events = events_of(&engine, 5);
    assert!(
        matches!(&events[0], EngineEvent::ExportFrame { serial: 1, .. }),
        "{events:?}"
    );
    assert!(
        matches!(&events[1], EngineEvent::PicturesMissing { serial: 2, ids } if *ids == vec![PictureId(7)]),
        "{events:?}"
    );
    assert!(
        matches!(&events[2], EngineEvent::ExportFrame { serial: 3, .. }),
        "{events:?}"
    );
    assert!(
        matches!(&events[3], EngineEvent::FrameFailed { serial: 4, problem } if problem.contains("0 by 8")),
        "{events:?}"
    );
    assert!(
        matches!(&events[4], EngineEvent::ExportFrame { serial: 5, .. }),
        "{events:?}"
    );
}

#[test]
fn the_frames_of_the_export_and_the_frames_of_the_preview_are_made_by_the_same_engine_the_same_way() {
    let Some(_graphics) = software_graphics() else { return };
    let (mut engine, _) = software_engine_with(EngineSettings::new(AdapterChoice::SoftwareOnly));
    let colour = YuvColour::UNSPECIFIED;
    engine.request_frame(grey_frame(77));
    let mut events = events_of(&engine, 1);
    engine.request_export_frame(grey_frame(77), colour);
    events.extend(events_of(&engine, 1));
    let (EngineEvent::Frame { frame, .. }, EngineEvent::ExportFrame { planes, .. }) = (&events[0], &events[1]) else {
        panic!("{events:?}")
    };
    // The planes are made of the very 8-bit frame that the preview is given.
    let (y, u, v) = reference_yuv420p(8, 8, &frame.rgba_pixels, colour);
    for (made, expected) in [
        (planes.luma(), &y),
        (planes.blue_difference(), &u),
        (planes.red_difference(), &v),
    ] {
        assert!(made.iter().zip(expected).all(|(a, b)| a.abs_diff(*b) <= 1));
    }
}
