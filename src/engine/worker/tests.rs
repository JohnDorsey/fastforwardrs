// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the engine's thread does when the graphics device goes wrong: a device that was lost, and a frame that the card did not finish, are answered with a new
//! device (and the window is told, and effects of strangers are not trusted again); a device that has to be replaced again and again stops the engine.
//!
//! There are two kinds of test here. Some take the thread's own `Worker` and talk to it as the thread's loop does, which is how what happens in the middle of a
//! batch can be looked at (and how a frame that does not finish can be made to happen: no card can be made to take longer than a limit on purpose without
//! spending the processor for as long as the program runs, and the test of a shader that really does not finish is in `hang_tests`, in a process of its own).
//! The others start the engine, as the window does, and look at what it says.
//!
//! **What a lost device looks like**, found by experiment on the software adapter of Windows (2026-10-16): a device that was destroyed says nothing until the
//! library is asked what has happened to it; a picture sent to it is taken as if nothing were wrong; and a frame made on it fails at once with a mistake about a
//! buffer that is not valid. So the thread asks the library at the start of every batch, and again after a frame has failed.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::*;
use crate::engine::engine_tests::{events_of, software_engine_with};
use crate::engine::foreign_effect_tests::{
    COUNT_THE_PASSES_OF_A_LOOP_THAT_NEVER_ENDS, INVERT, SHOW_THE_NUMBERS, TOO_MANY_NUMBERS, call, effect_source,
    guarded, inverted, the_passes_it_says,
};
use crate::engine::oracle_tests::{TestPicture, compare};
use crate::engine::testing::{DevicesMade, Gate, really_software_graphics, software_graphics};
use crate::engine::{
    AdapterChoice, CORNERS_OF_THE_WHOLE_PICTURE, DEFAULT_MOST_PASSES_OF_A_LOOP, EffectCall, Engine, LayerDescription,
    LayerSource, RenderedFrame,
};

const A_LONG_TIME: Duration = Duration::from_secs(60);

/// What the thread said, in order.
type Told = Mutex<Vec<EngineEvent>>;

fn what_was_said(told: &Told) -> Vec<EngineEvent> {
    std::mem::take(&mut *told.lock().unwrap())
}

/// A worker on the software adapter, and what it says.
fn with_a_worker(settings: EngineSettings, body: impl FnOnce(&mut Worker<'_>, &Told)) {
    let told = Told::default();
    let tell = |event: EngineEvent| {
        told.lock().unwrap().push(event);
        true
    };
    let mut worker = Worker::start(&settings, &tell).expect("the software adapter gave a device");
    body(&mut worker, &told);
}

fn software_settings() -> EngineSettings {
    EngineSettings::new(AdapterChoice::SoftwareOnly)
}

/// A frame of 8 by 8 that has the picture `1` all over it with `effects` done to it.
fn description_of_picture_1(effects: Vec<EffectCall>) -> FrameDescription {
    let mut description = FrameDescription::black((8, 8));
    description.layers.push(LayerDescription {
        source: LayerSource::Picture(PictureId(1)),
        corners: CORNERS_OF_THE_WHOLE_PICTURE,
        effects,
    });
    description
}

fn picture_1() -> TestPicture {
    TestPicture::gradient(8, 8)
}

fn give_the_effect(worker: &mut Worker<'_>, key: &str, stage: &str) {
    worker
        .take_an_effect(
            key.to_owned(),
            Arc::new(guarded(&effect_source(stage), DEFAULT_MOST_PASSES_OF_A_LOOP)),
        )
        .unwrap();
}

fn send_the_picture(worker: &mut Worker<'_>) {
    worker
        .take_a_picture(PictureId(1), &picture_1().as_picture_data())
        .unwrap();
}

/// The frame in the event, and what it came to, if the event is a frame.
fn the_frame_in(event: &EngineEvent) -> (&RenderedFrame, FrameStatistics) {
    match event {
        EngineEvent::Frame { frame, statistics, .. } => (frame, *statistics),
        other => panic!("not a frame: {other:?}"),
    }
}

fn is_within(frame: &RenderedFrame, expected: &[u8], levels: u8) -> bool {
    compare(&frame.rgba_pixels, expected).largest <= levels
}

// ---- what a worker does ----

#[test]
fn a_try_that_fails_because_the_device_was_lost_is_made_again_on_the_new_one_and_a_try_that_fails_for_any_other_reason_is_not()
 {
    let Some(_graphics) = software_graphics() else { return };
    with_a_worker(software_settings(), |worker, told| {
        // Fails with the device all right: told as it is, once, and nothing is replaced.
        let mut tries = 0;
        let result = worker
            .attempt_and_again_on_a_new_device_if_it_was_lost(|_| {
                tries += 1;
                Err::<(), _>(RenderProblem::GraphicsMistake(
                    "a mistake of the engine's own".to_owned(),
                ))
            })
            .unwrap();
        assert!(matches!(result, Err(RenderProblem::GraphicsMistake(_))));
        assert_eq!(tries, 1);
        assert!(what_was_said(told).is_empty());

        // Fails because the card was reset while it was tried: the device is found to be lost, replaced, and the try is made once more on the new one.
        let mut tries = 0;
        let result = worker
            .attempt_and_again_on_a_new_device_if_it_was_lost(|renderer| {
                tries += 1;
                if tries == 1 {
                    renderer.device().destroy();
                    Err(RenderProblem::GraphicsMistake("the card was reset".to_owned()))
                } else {
                    Ok(tries)
                }
            })
            .unwrap();
        assert_eq!(result.unwrap(), 2);
        let said = what_was_said(told);
        assert!(
            matches!(said.as_slice(), [EngineEvent::DeviceReplaced { why, .. }] if why.contains("lost")),
            "{said:?}"
        );

        // Fails again on the new device: it is not tried a third time, and is told as it is.
        let mut tries = 0;
        let result = worker
            .attempt_and_again_on_a_new_device_if_it_was_lost(|renderer| {
                tries += 1;
                renderer.device().destroy();
                Err::<(), _>(RenderProblem::GraphicsMistake(format!("try number {tries}")))
            })
            .unwrap();
        assert_eq!(tries, 2);
        assert!(matches!(result, Err(RenderProblem::GraphicsMistake(words)) if words == "try number 2"));
    });
}

#[test]
fn a_frame_that_fails_on_a_lost_device_is_told_to_have_failed_and_then_the_device_is_replaced_and_the_effects_of_strangers_in_it_are_switched_off()
 {
    let Some(_graphics) = software_graphics() else { return };
    with_a_worker(software_settings(), |worker, told| {
        give_the_effect(worker, "test:invert", INVERT);
        give_the_effect(worker, "test:numbers", SHOW_THE_NUMBERS);
        send_the_picture(worker);
        let with_the_effect = description_of_picture_1(vec![call("test:invert", &[1.0])]);
        assert!(matches!(worker.answer(1, with_the_effect), Next::GoOn));
        let said = what_was_said(told);
        let (frame, statistics) = the_frame_in(&said[0]);
        assert_eq!(statistics.effects_not_run, 0);
        assert!(is_within(frame, &inverted(&picture_1(), 1.0), 1));

        // The card is reset. The next frame is another one (so it is not in the cache), and fails.
        worker.renderer.device().destroy();
        let another = description_of_picture_1(vec![call("test:invert", &[0.5])]);
        assert!(matches!(worker.answer(2, another.clone()), Next::GoOn));
        let said = what_was_said(told);
        assert!(
            matches!(said.as_slice(), [
                EngineEvent::FrameFailed { serial: 2, .. },
                EngineEvent::DeviceReplaced { why, effects_switched_off },
            ] if why.contains("lost") && effects_switched_off == &["test:invert".to_owned()]),
            "the frame failed, and then the device was replaced, and the effect that was in the frame is not trusted: {said:?}"
        );
        assert_eq!(worker.switched_off, vec!["test:invert".to_owned()]);
        assert_eq!(
            worker
                .given_effects
                .iter()
                .map(|(key, _)| key.as_str())
                .collect::<Vec<_>>(),
            vec!["test:numbers"],
            "the effect that is trusted goes on the new device, the other is not given to it"
        );

        // The new device has no pictures: the frame says so, and when they are sent again it is made, without the effect.
        assert!(matches!(worker.answer(3, another.clone()), Next::GoOn));
        let said = what_was_said(told);
        assert!(
            matches!(said.as_slice(), [EngineEvent::PicturesMissing { serial: 3, ids }] if ids == &[PictureId(1)]),
            "{said:?}"
        );
        send_the_picture(worker);
        assert!(matches!(worker.answer(4, another), Next::GoOn));
        let said = what_was_said(told);
        let (frame, statistics) = the_frame_in(&said[0]);
        assert_eq!(
            statistics.effects_not_run, 1,
            "the effect that is switched off is left out and counted"
        );
        assert!(is_within(frame, &picture_1().rgba, 1), "and the picture is as it is");
    });
}

#[test]
fn a_frame_that_the_card_did_not_finish_is_told_to_have_failed_and_the_device_is_replaced_and_what_was_made_before_is_not_given_again()
 {
    let Some(_graphics) = really_software_graphics() else {
        return;
    };
    with_a_worker(software_settings(), |worker, told| {
        give_the_effect(worker, "test:invert", INVERT);
        give_the_effect(worker, "test:numbers", SHOW_THE_NUMBERS);
        send_the_picture(worker);
        let before = description_of_picture_1(vec![call("test:numbers", &[0.2])]);
        assert!(matches!(worker.answer(1, before.clone()), Next::GoOn));
        what_was_said(told);
        // (It is in the cache now.)
        assert!(matches!(worker.answer(2, before.clone()), Next::GoOn));
        assert!(the_frame_in(&what_was_said(told)[0]).1.came_from_the_cache);

        worker.renderer.pretend_the_card_never_finishes();
        let slow = description_of_picture_1(vec![call("test:invert", &[1.0]), call("test:numbers", &[0.4])]);
        assert!(matches!(worker.answer(3, slow), Next::GoOn));
        let said = what_was_said(told);
        let [
            EngineEvent::FrameFailed {
                serial: 3,
                problem: failure,
            },
            EngineEvent::DeviceReplaced {
                why,
                effects_switched_off,
            },
        ] = said.as_slice()
        else {
            panic!("{said:?}")
        };
        assert!(failure.contains("did not finish"), "{failure}");
        assert!(why.contains("did not finish"), "{why}");
        assert!(
            why.contains("processor"),
            "on a software adapter the words say that what was abandoned goes on: {why}"
        );
        assert_eq!(
            effects_switched_off,
            &["test:invert".to_owned(), "test:numbers".to_owned()],
            "everything of strangers that was in the frame, in the order it came"
        );
        assert!(
            worker.given_effects.is_empty(),
            "none of them is given to the new device"
        );

        // The new device has not got the picture, and the frame from before is not in the cache: it is not given as if nothing had happened.
        assert!(matches!(worker.answer(4, before), Next::GoOn));
        let said = what_was_said(told);
        assert!(
            matches!(said.as_slice(), [EngineEvent::PicturesMissing { serial: 4, .. }]),
            "{said:?}"
        );
    });
}

#[test]
fn an_effect_that_is_switched_off_cannot_be_given_to_the_engine_again_and_the_others_still_can() {
    let Some(_graphics) = software_graphics() else { return };
    with_a_worker(software_settings(), |worker, told| {
        give_the_effect(worker, "test:invert", INVERT);
        send_the_picture(worker);
        worker.renderer.pretend_the_card_never_finishes();
        let slow = description_of_picture_1(vec![call("test:invert", &[1.0])]);
        assert!(matches!(worker.answer(1, slow), Next::GoOn));
        what_was_said(told);
        assert_eq!(worker.switched_off, vec!["test:invert".to_owned()]);

        give_the_effect(worker, "test:invert", INVERT);
        let said = what_was_said(told);
        assert!(
            matches!(said.as_slice(), [EngineEvent::EffectRejected { key, problem }]
                if key == "test:invert" && problem.contains("switched off")),
            "{said:?}"
        );
        assert!(worker.given_effects.is_empty());

        give_the_effect(worker, "test:numbers", SHOW_THE_NUMBERS);
        assert!(what_was_said(told).is_empty());
        assert_eq!(worker.given_effects.len(), 1);
        send_the_picture(worker);
        let frame = description_of_picture_1(vec![call("test:numbers", &[0.2])]);
        assert!(matches!(worker.answer(2, frame), Next::GoOn));
        let said = what_was_said(told);
        assert!(the_frame_in(&said[0]).0.every_pixel_is([0, 51, 0, 255]));
    });
}

#[test]
fn a_mistake_that_is_not_the_cards_does_not_make_a_new_device_a_picture_that_cannot_be_used_an_effect_the_library_refuses_and_a_size_that_is_wrong()
 {
    let Some(_graphics) = software_graphics() else { return };
    with_a_worker(software_settings(), |worker, told| {
        // A picture that is bigger than the card takes.
        let too_wide = worker.renderer.device().largest_texture_side + 1;
        let too_big = PictureData::rgba8(too_wide, 1, vec![0; too_wide as usize * 4]).unwrap();
        worker.take_a_picture(PictureId(7), &too_big).unwrap();
        let said = what_was_said(told);
        assert!(
            matches!(said.as_slice(), [EngineEvent::PictureRejected { id: PictureId(7), .. }]),
            "{said:?}"
        );
        // An effect the library will not take.
        worker
            .take_an_effect(
                "test:too_many".to_owned(),
                Arc::new(guarded(TOO_MANY_NUMBERS, DEFAULT_MOST_PASSES_OF_A_LOOP)),
            )
            .unwrap();
        let said = what_was_said(told);
        assert!(
            matches!(said.as_slice(), [EngineEvent::EffectRejected { key, .. }] if key == "test:too_many"),
            "{said:?}"
        );
        assert!(
            worker.given_effects.is_empty(),
            "it is not kept to be given to a new device"
        );
        // A frame of no size.
        assert!(matches!(worker.answer(1, FrameDescription::black((0, 4))), Next::GoOn));
        let said = what_was_said(told);
        assert!(
            matches!(said.as_slice(), [EngineEvent::FrameFailed { serial: 1, .. }]),
            "{said:?}"
        );
        assert_eq!(
            worker.replacements_made_so_far(),
            0,
            "none of them is a reason to make a new device"
        );
    });
}

#[test]
fn a_new_shader_under_a_key_is_the_effect_from_then_on_and_frames_made_with_the_old_one_are_not_given_for_it() {
    let Some(_graphics) = software_graphics() else { return };
    with_a_worker(software_settings(), |worker, told| {
        give_the_effect(worker, "test:effect", INVERT);
        send_the_picture(worker);
        let description = description_of_picture_1(vec![call("test:effect", &[1.0])]);
        assert!(matches!(worker.answer(1, description.clone()), Next::GoOn));
        let first = what_was_said(told);
        assert!(is_within(the_frame_in(&first[0]).0, &inverted(&picture_1(), 1.0), 1));

        give_the_effect(worker, "test:effect", SHOW_THE_NUMBERS);
        assert_eq!(worker.given_effects.len(), 1, "one shader for a key");
        assert!(matches!(worker.answer(2, description), Next::GoOn));
        let second = what_was_said(told);
        let (frame, statistics) = the_frame_in(&second[0]);
        assert!(!statistics.came_from_the_cache, "what a description means changed");
        assert!(frame.every_pixel_is([0, 255, 0, 255]), "{:?}", frame.pixel(0, 0));
    });
}

#[test]
fn a_batch_that_comes_after_the_device_was_lost_is_taken_by_the_new_device_and_its_frame_is_made() {
    let Some(_graphics) = software_graphics() else { return };
    with_a_worker(software_settings(), |worker, told| {
        worker.renderer.device().destroy();
        let batch = vec![
            EngineRequest::Upload {
                id: PictureId(1),
                data: Arc::new(picture_1().as_picture_data()),
            },
            EngineRequest::Render {
                serial: 9,
                description: description_of_picture_1(Vec::new()),
            },
        ];
        assert!(matches!(worker.take(batch), Next::GoOn));
        let said = what_was_said(told);
        let [
            EngineEvent::DeviceReplaced { why, .. },
            EngineEvent::Frame { serial: 9, frame, .. },
        ] = said.as_slice()
        else {
            panic!("{said:?}")
        };
        assert!(why.contains("lost"), "{why}");
        assert!(
            is_within(frame, &picture_1().rgba, 1),
            "the picture went to the new device and not to the one that was lost"
        );
    });
}

#[test]
fn a_batch_that_has_a_stop_in_it_makes_no_frame_and_says_why_it_stops() {
    let Some(_graphics) = software_graphics() else { return };
    with_a_worker(software_settings(), |worker, told| {
        let batch = vec![
            EngineRequest::Render {
                serial: 1,
                description: FrameDescription::black((4, 4)),
            },
            EngineRequest::Stop,
        ];
        let Next::Stop(why) = worker.take(batch) else {
            panic!("it did not stop")
        };
        assert!(why.contains("told to stop"), "{why}");
        assert!(what_was_said(told).is_empty());
    });
}

#[test]
fn a_device_that_has_to_be_replaced_for_the_third_time_within_a_minute_is_not_and_the_engine_stops_saying_how_often() {
    let Some(_graphics) = software_graphics() else { return };
    with_a_worker(software_settings(), |worker, told| {
        for round in 1..=2 {
            worker.renderer.device().destroy();
            let batch = vec![EngineRequest::Render {
                serial: round,
                description: FrameDescription::black((4, 4)),
            }];
            assert!(matches!(worker.take(batch), Next::GoOn), "round {round}");
            let said = what_was_said(told);
            assert!(
                matches!(
                    said.as_slice(),
                    [EngineEvent::DeviceReplaced { .. }, EngineEvent::Frame { .. }]
                ),
                "round {round}: {said:?}"
            );
        }
        worker.renderer.device().destroy();
        let batch = vec![EngineRequest::Render {
            serial: 3,
            description: FrameDescription::black((4, 4)),
        }];
        let Next::Stop(why) = worker.take(batch) else {
            panic!("it did not stop")
        };
        assert!(why.contains("replaced 2 times within 60s"), "{why}");
        assert!(why.contains("lost") && why.contains("stops"), "{why}");
        assert!(
            what_was_said(told).is_empty(),
            "no frame, and nothing said but the stop, which the thread says"
        );
    });
}

#[test]
fn what_waits_for_the_card_when_it_is_let_go_of_is_let_go_of_on_a_thread_of_its_own_and_the_caller_does_not_wait_for_it()
 {
    /// What waits for the card when it is let go of, as the last handle to a device does: here, for a gate that nothing opens until the test says.
    struct WaitsWhenLetGoOf {
        gate: Arc<Gate>,
        the_thread_it_was_let_go_of_on: Arc<Mutex<Option<String>>>,
    }
    impl Drop for WaitsWhenLetGoOf {
        fn drop(&mut self) {
            *self.the_thread_it_was_let_go_of_on.lock().unwrap() = std::thread::current().name().map(str::to_owned);
            self.gate.wait_until_open();
        }
    }
    let gate = Gate::closed();
    let the_thread = Arc::new(Mutex::new(None));
    let started = Instant::now();
    let_go_of_it_on_another_thread(WaitsWhenLetGoOf {
        gate: Arc::clone(&gate),
        the_thread_it_was_let_go_of_on: Arc::clone(&the_thread),
    });
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "it came back, with what it let go of waiting for a gate that is still shut"
    );
    let waited_since = Instant::now();
    while the_thread.lock().unwrap().is_none() {
        assert!(waited_since.elapsed() < A_LONG_TIME, "it was never let go of");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        the_thread.lock().unwrap().as_deref(),
        Some(THE_THREAD_THAT_LETS_GO_OF_OLD_DEVICES)
    );
    gate.open();
}

// ---- the engine as the window has it ----

fn engine_that_shows_its_devices(settings: EngineSettings) -> (Engine, DevicesMade) {
    let devices = DevicesMade::default();
    let mut settings = settings;
    settings.watch_the_devices = Some(devices.watcher());
    let (engine, _) = software_engine_with(settings);
    (engine, devices)
}

fn the_engine_makes_the_frame(
    engine: &mut Engine,
    description: FrameDescription,
) -> (Arc<RenderedFrame>, FrameStatistics) {
    engine.request_frame(description);
    let mut events = events_of(engine, 1);
    match events.remove(0) {
        EngineEvent::Frame { frame, statistics, .. } => (frame, statistics),
        other => panic!("not a frame: {other:?}"),
    }
}

#[test]
fn a_device_that_is_lost_between_two_frames_is_replaced_before_the_second_and_the_window_is_told_first() {
    let Some(_graphics) = software_graphics() else { return };
    let (mut engine, devices) = engine_that_shows_its_devices(software_settings());
    the_engine_makes_the_frame(&mut engine, FrameDescription::black((16, 16)));
    devices.nth(1, A_LONG_TIME).destroy();
    engine.request_frame(FrameDescription::black((17, 16)));
    let events = events_of(&engine, 2);
    assert!(
        matches!(&events[0], EngineEvent::DeviceReplaced { why, effects_switched_off }
            if why.contains("lost") && effects_switched_off.is_empty()),
        "{events:?}"
    );
    let EngineEvent::Frame { serial: 2, frame, .. } = &events[1] else {
        panic!("{events:?}")
    };
    assert_eq!((frame.width_in_pixels, frame.height_in_pixels), (17, 16));
    assert!(frame.every_pixel_is([0, 0, 0, 255]));
    assert_eq!(devices.count(), 2, "one new device, and no more");
    assert!(engine.is_running());
}

#[test]
fn after_the_device_was_replaced_the_pictures_are_missing_until_they_are_sent_again_and_then_the_frame_is_as_it_was() {
    let Some(_graphics) = software_graphics() else { return };
    let (mut engine, devices) = engine_that_shows_its_devices(software_settings());
    engine.upload_picture(PictureId(1), Arc::new(picture_1().as_picture_data()));
    let (before, _) = the_engine_makes_the_frame(&mut engine, description_of_picture_1(Vec::new()));
    assert!(is_within(&before, &picture_1().rgba, 1));

    devices.nth(1, A_LONG_TIME).destroy();
    engine.request_frame(description_of_picture_1(Vec::new()));
    let events = events_of(&engine, 2);
    assert!(matches!(&events[0], EngineEvent::DeviceReplaced { .. }), "{events:?}");
    assert!(
        matches!(&events[1], EngineEvent::PicturesMissing { serial: 2, ids } if ids == &[PictureId(1)]),
        "the frame that was made is not given again, for the picture is not on the new device: {events:?}"
    );

    engine.upload_picture(PictureId(1), Arc::new(picture_1().as_picture_data()));
    let (after, statistics) = the_engine_makes_the_frame(&mut engine, description_of_picture_1(Vec::new()));
    assert!(!statistics.came_from_the_cache);
    assert_eq!(after, before);
}

#[test]
fn the_effects_that_were_given_to_the_engine_are_on_the_new_device_and_do_the_same_there() {
    let Some(_graphics) = software_graphics() else { return };
    let (mut engine, devices) = engine_that_shows_its_devices(software_settings());
    engine
        .compile_effect_shader("test:invert", &effect_source(INVERT))
        .expect("the guard takes it");
    engine.upload_picture(PictureId(1), Arc::new(picture_1().as_picture_data()));
    let inverted_picture = description_of_picture_1(vec![call("test:invert", &[1.0])]);
    let (before, statistics) = the_engine_makes_the_frame(&mut engine, inverted_picture.clone());
    assert_eq!(statistics.effects_not_run, 0);
    assert!(is_within(&before, &inverted(&picture_1(), 1.0), 1));

    devices.nth(1, A_LONG_TIME).destroy();
    engine.request_frame(inverted_picture.clone());
    let events = events_of(&engine, 2);
    assert!(
        matches!(&events[0], EngineEvent::DeviceReplaced { effects_switched_off, .. } if effects_switched_off.is_empty()),
        "{events:?}"
    );
    engine.upload_picture(PictureId(1), Arc::new(picture_1().as_picture_data()));
    let (after, statistics) = the_engine_makes_the_frame(&mut engine, inverted_picture);
    assert_eq!(statistics.effects_not_run, 0, "the effect is there on the new device");
    assert_eq!(after, before);
}

#[test]
fn a_device_that_is_lost_again_and_again_stops_the_engine_the_third_time_and_says_so() {
    let Some(_graphics) = software_graphics() else { return };
    let (mut engine, devices) = engine_that_shows_its_devices(software_settings());
    the_engine_makes_the_frame(&mut engine, FrameDescription::black((8, 8)));
    for round in 1..=2 {
        devices.nth(round, A_LONG_TIME).destroy();
        engine.request_frame(FrameDescription::black((8, 8 + round as u32)));
        let events = events_of(&engine, 2);
        assert!(
            matches!(&events[0], EngineEvent::DeviceReplaced { .. }) && matches!(&events[1], EngineEvent::Frame { .. }),
            "round {round}: {events:?}"
        );
    }
    devices.nth(3, A_LONG_TIME).destroy();
    engine.request_frame(FrameDescription::black((8, 20)));
    let events = events_of(&engine, 1);
    let EngineEvent::Stopped { problem } = &events[0] else {
        panic!("{events:?}")
    };
    assert!(
        problem.contains("replaced 2 times") && problem.contains("stops"),
        "{problem}"
    );
    let started = Instant::now();
    while engine.is_running() && started.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!engine.is_running());
    assert_eq!(devices.count(), 3, "no fourth device was made");
}

#[test]
fn a_shader_that_the_guard_refuses_is_refused_at_once_with_the_place_and_never_reaches_the_engine() {
    let Some(_graphics) = software_graphics() else { return };
    let (engine, _) = software_engine_with(software_settings());
    let typo = effect_source(
        "@fragment\nfn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {\n    let x = 1.0\n    return vec4<f32>(x);\n}\n",
    );
    let problem = engine.compile_effect_shader("test:typo", &typo).unwrap_err();
    // (The `;` that is missing is found where the next thing begins: the `return` that is on the line after the `let`.)
    let line_of_the_return = typo.lines().position(|line| line.contains("return")).unwrap() as u32 + 1;
    assert_eq!(
        (problem.line, problem.column),
        (Some(line_of_the_return), Some(5)),
        "{problem}"
    );

    let nothing_to_run = engine
        .compile_effect_shader("test:vertex_only", "@vertex\nfn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {\n    return vec4<f32>(0.0, 0.0, 0.0, 1.0);\n}\n")
        .unwrap_err();
    assert!(nothing_to_run.message.contains("fs_main"), "{nothing_to_run}");

    assert!(
        engine
            .compile_effect_shader("", &effect_source(INVERT))
            .unwrap_err()
            .message
            .contains("key")
    );
    for key in ["engine:colour_matrix", "engine:mine"] {
        let belongs = engine.compile_effect_shader(key, &effect_source(INVERT)).unwrap_err();
        assert!(
            belongs.message.contains(key) && belongs.message.contains("engine"),
            "{belongs}"
        );
    }
    std::thread::sleep(Duration::from_millis(100));
    assert!(engine.try_next_event().is_none(), "none of them was sent to the engine");

    // One that is all right says how many of its loops were given a limit.
    let report = engine
        .compile_effect_shader("test:loop", &effect_source(COUNT_THE_PASSES_OF_A_LOOP_THAT_NEVER_ENDS))
        .unwrap();
    assert_eq!(report.loops_capped, 1);
    assert_eq!(
        engine
            .compile_effect_shader("test:invert", &effect_source(INVERT))
            .unwrap()
            .loops_capped,
        0
    );
}

#[test]
fn a_shader_the_graphics_library_will_not_take_is_told_as_an_event_and_frames_that_ask_for_the_effect_are_made_without_it()
 {
    let Some(_graphics) = software_graphics() else { return };
    let (mut engine, _) = software_engine_with(software_settings());
    engine
        .compile_effect_shader("test:too_many", TOO_MANY_NUMBERS)
        .expect("the guard takes it: it is the library that will not");
    let events = events_of(&engine, 1);
    assert!(
        matches!(&events[0], EngineEvent::EffectRejected { key, problem }
            if key == "test:too_many" && problem.contains("would not take it")),
        "{events:?}"
    );
    engine.upload_picture(PictureId(1), Arc::new(picture_1().as_picture_data()));
    let (frame, statistics) = the_engine_makes_the_frame(
        &mut engine,
        description_of_picture_1(vec![call("test:too_many", &[1.0])]),
    );
    assert_eq!(statistics.effects_not_run, 1);
    assert!(is_within(&frame, &picture_1().rgba, 1));
}

#[test]
fn a_loop_that_never_ends_in_a_shader_given_to_the_engine_ends_after_the_limit_the_engine_has_and_the_frame_comes_in_under_a_second()
 {
    let Some(_graphics) = software_graphics() else { return };
    for (most_passes, expected) in [(None, DEFAULT_MOST_PASSES_OF_A_LOOP), (Some(800), 800)] {
        let mut settings = software_settings();
        if let Some(most_passes) = most_passes {
            settings.most_passes_of_a_loop = most_passes;
        }
        let (mut engine, _) = software_engine_with(settings);
        engine
            .compile_effect_shader(
                "test:never_ends",
                &effect_source(COUNT_THE_PASSES_OF_A_LOOP_THAT_NEVER_ENDS),
            )
            .unwrap();
        engine.upload_picture(PictureId(1), Arc::new(picture_1().as_picture_data()));
        // (The first frame is made, and thrown away: the engine makes its pipelines and the card compiles its shaders when they are first used.)
        the_engine_makes_the_frame(&mut engine, description_of_picture_1(Vec::new()));
        let started = Instant::now();
        let (frame, statistics) = the_engine_makes_the_frame(
            &mut engine,
            description_of_picture_1(vec![call("test:never_ends", &[])]),
        );
        let took = started.elapsed();
        assert_eq!(statistics.effects_not_run, 0);
        assert_eq!(the_passes_it_says(&frame), expected);
        assert!(
            took < Duration::from_secs(1),
            "{expected} passes of a loop that never ends took {took:?}"
        );
    }
}

#[test]
fn a_shader_that_is_given_under_a_key_again_replaces_the_first_and_the_frame_that_was_made_with_the_first_is_not_given_for_it()
 {
    let Some(_graphics) = software_graphics() else { return };
    let (mut engine, _) = software_engine_with(software_settings());
    engine.upload_picture(PictureId(1), Arc::new(picture_1().as_picture_data()));
    engine
        .compile_effect_shader("test:effect", &effect_source(INVERT))
        .unwrap();
    let description = description_of_picture_1(vec![call("test:effect", &[1.0])]);
    let (first, _) = the_engine_makes_the_frame(&mut engine, description.clone());
    assert!(is_within(&first, &inverted(&picture_1(), 1.0), 1));
    // (The same frame again is the one that was made.)
    assert!(
        the_engine_makes_the_frame(&mut engine, description.clone())
            .1
            .came_from_the_cache
    );

    engine
        .compile_effect_shader("test:effect", &effect_source(SHOW_THE_NUMBERS))
        .unwrap();
    let (second, statistics) = the_engine_makes_the_frame(&mut engine, description);
    assert!(!statistics.came_from_the_cache);
    assert!(second.every_pixel_is([0, 255, 0, 255]), "{:?}", second.pixel(0, 0));
}

// ---- the frames of the export ----

/// A frame of 8 by 8 that is one flat grey of this level.
fn grey(level: u8) -> FrameDescription {
    FrameDescription {
        colour: Default::default(),
        render_size: (8, 8),
        layers: vec![LayerDescription {
            source: LayerSource::Solid([level, level, level, 255]),
            corners: CORNERS_OF_THE_WHOLE_PICTURE,
            effects: Vec::new(),
        }],
    }
}

fn export_request(serial: u64, level: u8) -> EngineRequest {
    EngineRequest::RenderForExport {
        serial,
        description: grey(level),
        colour: YuvColour::UNSPECIFIED,
    }
}

fn the_serials_of_the_export_frames_in(said: &[EngineEvent]) -> Vec<u64> {
    said.iter()
        .map(|event| match event {
            EngineEvent::ExportFrame { serial, .. } => *serial,
            other => panic!("not a frame of the export: {other:?}"),
        })
        .collect()
}

#[test]
fn a_frame_of_the_export_is_finished_when_as_many_are_on_their_way_as_there_is_room_for_and_the_rest_at_the_end_of_the_batch()
 {
    let Some(_graphics) = software_graphics() else { return };
    for room in [1usize, 2, 3] {
        let mut settings = software_settings();
        settings.export_frames_in_flight = room;
        with_a_worker(settings, |worker, told| {
            let mut serials = Vec::new();
            for serial in 1..=5u64 {
                let next = worker.begin_an_export_frame(serial, &grey(serial as u8 * 10), YuvColour::UNSPECIFIED);
                assert!(matches!(next, Next::GoOn));
                // The card has the new frame to work on while the one before is read back: as many are on their way as there is room for, less the one that was
                // finished to make room (with room for one there is no overlap, and a frame is finished as soon as it is begun).
                assert_eq!(
                    worker.on_their_way.len(),
                    (serial as usize).min(room - 1),
                    "room for {room}, frame {serial}"
                );
                serials.extend(the_serials_of_the_export_frames_in(&what_was_said(told)));
                assert_eq!(serials.len() + worker.on_their_way.len(), serial as usize);
            }
            assert!(matches!(worker.finish_the_export_frames_on_their_way(), Next::GoOn));
            serials.extend(the_serials_of_the_export_frames_in(&what_was_said(told)));
            assert_eq!(
                serials,
                vec![1, 2, 3, 4, 5],
                "room for {room}: in the order they were asked in, none left out"
            );
            assert!(worker.on_their_way.is_empty());
        });
    }
}

#[test]
fn all_the_frames_of_the_export_of_a_batch_are_answered_by_the_end_of_it() {
    let Some(_graphics) = software_graphics() else { return };
    with_a_worker(software_settings(), |worker, told| {
        let batch = (1..=7)
            .map(|serial| export_request(serial, serial as u8 * 10))
            .collect();
        assert!(matches!(worker.take(batch), Next::GoOn));
        assert_eq!(
            the_serials_of_the_export_frames_in(&what_was_said(told)),
            (1..=7).collect::<Vec<u64>>()
        );
        assert!(
            worker.on_their_way.is_empty(),
            "nothing is left on its way between batches"
        );
    });
}

#[test]
fn when_a_frame_of_the_export_does_not_finish_the_frames_begun_after_it_are_given_up_with_it_the_device_is_replaced_and_the_next_frames_are_made()
 {
    let Some(_graphics) = software_graphics() else { return };
    let mut settings = software_settings();
    settings.export_frames_in_flight = 3;
    with_a_worker(settings, |worker, told| {
        worker.renderer.pretend_the_card_never_finishes();
        assert!(matches!(
            worker.take(vec![export_request(1, 10), export_request(2, 20)]),
            Next::GoOn
        ));
        let said = what_was_said(told);
        assert_eq!(said.len(), 3, "{said:?}");
        assert!(
            matches!(&said[0], EngineEvent::FrameFailed { serial: 1, problem } if problem.contains("did not finish")),
            "{said:?}"
        );
        assert!(
            matches!(&said[1], EngineEvent::FrameFailed { serial: 2, problem } if problem.contains("given up with the frame 1")),
            "{said:?}"
        );
        assert!(matches!(&said[2], EngineEvent::DeviceReplaced { .. }), "{said:?}");
        assert!(worker.on_their_way.is_empty());
        assert_eq!(worker.replacements_made_so_far(), 1);

        // The new device makes the frames after them.
        assert!(matches!(worker.take(vec![export_request(3, 30)]), Next::GoOn));
        assert_eq!(the_serials_of_the_export_frames_in(&what_was_said(told)), vec![3]);
    });
}

#[test]
fn a_device_that_was_lost_before_a_batch_of_the_export_is_replaced_before_its_frames_are_made() {
    let Some(_graphics) = software_graphics() else { return };
    with_a_worker(software_settings(), |worker, told| {
        worker.renderer.device().destroy();
        assert!(matches!(
            worker.take(vec![export_request(1, 10), export_request(2, 20)]),
            Next::GoOn
        ));
        let said = what_was_said(told);
        assert!(matches!(&said[0], EngineEvent::DeviceReplaced { .. }), "{said:?}");
        assert_eq!(the_serials_of_the_export_frames_in(&said[1..]), vec![1, 2]);
    });
}

#[test]
fn a_stop_in_a_batch_of_the_export_stops_the_engine_without_waiting_for_the_frames_on_their_way() {
    let Some(_graphics) = software_graphics() else { return };
    let mut settings = software_settings();
    settings.export_frames_in_flight = 3;
    with_a_worker(settings, |worker, _| {
        let next = worker.take(vec![export_request(1, 10), export_request(2, 20), EngineRequest::Stop]);
        assert!(matches!(next, Next::Stop(_)));
        assert!(worker.on_their_way.is_empty());
    });
}

#[test]
fn an_effect_whose_shader_is_on_the_shelf_is_run_when_a_frame_asks_for_it_without_the_engine_being_given_it() {
    let Some(_graphics) = software_graphics() else { return };
    let (mut engine, _) = software_engine_with(software_settings());
    crate::engine::publish_effect_shader("test:from_the_shelf", &effect_source(INVERT));
    engine.upload_picture(PictureId(1), Arc::new(picture_1().as_picture_data()));

    let (frame, statistics) = the_engine_makes_the_frame(
        &mut engine,
        description_of_picture_1(vec![call("test:from_the_shelf", &[1.0])]),
    );

    assert_eq!(statistics.effects_not_run, 0, "the effect was run");
    assert!(is_within(&frame, &inverted(&picture_1(), 1.0), 1));

    // A key that is not on the shelf is an effect that the engine does not have, as it was.
    let (_, statistics) = the_engine_makes_the_frame(
        &mut engine,
        description_of_picture_1(vec![call("test:nowhere", &[1.0])]),
    );
    assert_eq!(statistics.effects_not_run, 1);
    // And one that is put on the shelf later is run from then on.
    crate::engine::publish_effect_shader("test:nowhere", &effect_source(INVERT));
    let (_, statistics) = the_engine_makes_the_frame(
        &mut engine,
        description_of_picture_1(vec![call("test:nowhere", &[1.0])]),
    );
    assert_eq!(statistics.effects_not_run, 0);
}

#[test]
fn a_shader_on_the_shelf_that_the_guard_refuses_is_a_frame_made_without_the_effect_and_not_a_stop() {
    let Some(_graphics) = software_graphics() else { return };
    let (mut engine, _) = software_engine_with(software_settings());
    crate::engine::publish_effect_shader("test:shelf_typo", "this is not WGSL");
    engine.upload_picture(PictureId(1), Arc::new(picture_1().as_picture_data()));

    // (The numbers differ, so that the second frame is not the first again, which the engine would give from its cache.)
    for number in [1.0, 2.0] {
        let (frame, statistics) = the_engine_makes_the_frame(
            &mut engine,
            description_of_picture_1(vec![call("test:shelf_typo", &[number])]),
        );
        assert_eq!(statistics.effects_not_run, 1);
        assert!(is_within(&frame, &picture_1().rgba, 1), "the picture as it was");
    }
}

#[test]
fn the_shelf_gives_back_what_was_put_on_it_and_a_second_shader_under_a_key_replaces_the_first() {
    use crate::engine::{publish_effect_shader, published_effect_shader};
    assert!(published_effect_shader("test:shelf_nothing").is_none());
    publish_effect_shader("test:shelf_replaced", "first");
    assert_eq!(&*published_effect_shader("test:shelf_replaced").unwrap(), "first");
    publish_effect_shader("test:shelf_replaced", "second");
    assert_eq!(&*published_effect_shader("test:shelf_replaced").unwrap(), "second");
}

#[test]
fn a_frame_with_an_effect_that_depends_on_earlier_frames_is_neither_taken_from_the_cache_nor_put_in_it() {
    use crate::engine::persistent_buffer_tests::{
        ADD_A_TENTH_OF_RED, THE_PICTURE_AND_A_BUFFER, one_buffer_that_lasts, stateful,
    };
    with_a_worker(software_settings(), |worker, told| {
        worker
            .take_an_effect_with_structure(
                "test:accumulate".to_owned(),
                Arc::new(guarded(
                    &format!("{THE_PICTURE_AND_A_BUFFER}\n{ADD_A_TENTH_OF_RED}"),
                    DEFAULT_MOST_PASSES_OF_A_LOOP,
                )),
                Arc::new(one_buffer_that_lasts()),
            )
            .unwrap();
        send_the_picture(worker);

        // The same description, asked for twice: made twice (and it is the same picture, the second time from the state as it was before the frame).
        let stateful_frame = description_of_picture_1(vec![stateful("test:accumulate", 1, 0)]);
        assert!(stateful_frame.depends_on_earlier_frames());
        let mut seen = Vec::new();
        for serial in [1, 2, 3] {
            assert!(matches!(worker.answer(serial, stateful_frame.clone()), Next::GoOn));
            let said = what_was_said(told);
            let (frame, statistics) = the_frame_in(&said[0]);
            assert!(!statistics.came_from_the_cache, "serial {serial}");
            seen.push(frame.pixel(4, 4)[0]);
        }
        assert_eq!(seen[0], seen[1]);
        assert_eq!(seen[1], seen[2]);

        // A frame that has no state to go on with depends on nothing, and is cached as any frame is.
        let plain = description_of_picture_1(vec![]);
        assert!(!plain.depends_on_earlier_frames());
        assert!(matches!(worker.answer(4, plain.clone()), Next::GoOn));
        assert!(!the_frame_in(&what_was_said(told)[0]).1.came_from_the_cache);
        assert!(matches!(worker.answer(5, plain), Next::GoOn));
        assert!(the_frame_in(&what_was_said(told)[0]).1.came_from_the_cache);
    });
}
