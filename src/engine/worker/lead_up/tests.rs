// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The lead-up on a seek as the engine's thread makes it (EN-14, option 1): the frames that it is brought go through the engine for the states of the effects, the pictures that it sends are
//! let go of with it, what the window asks comes first, and it ends with a word (and, if the window cancels it, with none).
//!
//! Most of these talk to a `Worker` the way the thread's loop does, with the frames of the lead-up put in the channel before it is stepped (nothing in them waits for anything but the card). The
//! last ones start the engine, as the window does, to look at what the loop does with a lead-up that is waiting for its frames.
//!
//! The effect is the one that adds a tenth of red to what it kept: the red of the picture is how many frames the state has been through.

use std::sync::mpsc::{self, Sender};

use super::super::tests::{
    Told, picture_1, send_the_picture, software_settings, the_frame_in, what_was_said, with_a_worker,
};
use super::super::{Waiting, what_is_waiting};
use super::*;
use crate::engine::effect_tests::whole_frame;
use crate::engine::engine_tests::{events_of, software_engine};
use crate::engine::foreign_effect_tests::guarded;
use crate::engine::persistent_buffer_tests::{
    ADD_A_TENTH_OF_RED, THE_PICTURE_AND_A_BUFFER, near, one_buffer_that_lasts, stateful, tenths,
};
use crate::engine::testing::software_graphics;
use crate::engine::{DEFAULT_MOST_PASSES_OF_A_LOOP, EngineRequest, FrameStatistics, RenderedFrame, RulesOfCheckpoints};
use std::sync::Arc;
use std::time::{Duration, Instant};

const A_LONG_TIME: Duration = Duration::from_secs(60);

fn checkpoints_every(interval: u64) -> RulesOfCheckpoints {
    RulesOfCheckpoints {
        interval_in_frames: interval,
        memory_budget_in_bytes: u64::MAX,
    }
}

/// The frame of the effect at `frame` of its own clock, made of the picture `picture`: 8 by 8.
fn frame_of_the_effect_at(picture: u64, frame: u64) -> FrameDescription {
    let mut description = FrameDescription::black((8, 8));
    description.layers = vec![whole_frame(picture, vec![stateful("test:accumulate", 1, frame)])];
    description
}

/// A worker that has the effect that adds a tenth of red to what it kept, the picture 1 (which is what the preview sent), and the rules of checkpoints that are given.
fn with_the_accumulating_effect(rules: RulesOfCheckpoints, body: impl FnOnce(&mut Worker<'_>, &Told)) {
    let Some(_graphics) = software_graphics() else { return };
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
        assert!(matches!(
            worker.take(vec![EngineRequest::SetTheRulesOfCheckpoints(rules)]),
            Next::GoOn
        ));
        body(worker, told);
    });
}

/// Starts a lead-up with this generation, and gives the end of the channel that the thread that describes the frames has.
fn start_a_lead_up(worker: &mut Worker<'_>, generation: u64) -> Sender<LeadUpItem> {
    let (sender, items) = mpsc::channel();
    assert!(matches!(
        worker.take(vec![EngineRequest::StartTheLeadUp { generation, items }]),
        Next::GoOn
    ));
    sender
}

/// Steps the lead-up until it is over, which it is when it has been given an end (or a failure, or nobody to be given anything).
fn go_on_until_the_lead_up_is_over(worker: &mut Worker<'_>) {
    let mut steps = 0;
    while worker.has_a_lead_up() {
        assert!(matches!(worker.make_a_step_of_the_lead_up(), Next::GoOn));
        steps += 1;
        assert!(steps < 10_000, "the lead-up does not end");
    }
}

fn red_in(frame: &RenderedFrame) -> i32 {
    i32::from(frame.pixel(4, 4)[0])
}

#[test]
fn the_frames_of_a_lead_up_make_the_state_that_the_frame_after_them_goes_on_from() {
    with_the_accumulating_effect(checkpoints_every(60), |worker, told| {
        let sender = start_a_lead_up(worker, 7);
        assert!(worker.has_a_lead_up());
        for frame in 0..5 {
            sender
                .send(LeadUpItem::Frame(frame_of_the_effect_at(1, frame)))
                .unwrap();
        }
        sender.send(LeadUpItem::End).unwrap();
        go_on_until_the_lead_up_is_over(worker);
        let said = what_was_said(told);
        assert!(
            matches!(
                said.as_slice(),
                [EngineEvent::LeadUpDone {
                    generation: 7,
                    frames_made: 5
                }]
            ),
            "{said:?}"
        );

        // Frame 5 goes on from the state that five frames made: six tenths, and exact.
        assert!(matches!(worker.answer(1, frame_of_the_effect_at(1, 5)), Next::GoOn));
        let said = what_was_said(told);
        let (frame, statistics) = the_frame_in(&said[0]);
        assert!(near(red_in(frame), tenths(6)), "{}", red_in(frame));
        assert!(!statistics.approximate);
    });
    // (And without the lead-up it starts clear, and says so.)
    with_the_accumulating_effect(checkpoints_every(60), |worker, told| {
        assert!(matches!(worker.answer(1, frame_of_the_effect_at(1, 5)), Next::GoOn));
        let said = what_was_said(told);
        let (frame, statistics) = the_frame_in(&said[0]);
        assert!(near(red_in(frame), tenths(1)), "{}", red_in(frame));
        assert!(statistics.approximate);
    });
}

#[test]
fn a_lead_up_goes_on_after_a_frame_and_after_a_wait_for_the_next_and_says_nothing_until_it_is_over() {
    with_the_accumulating_effect(checkpoints_every(60), |worker, told| {
        let sender = start_a_lead_up(worker, 1);
        sender.send(LeadUpItem::Frame(frame_of_the_effect_at(1, 0))).unwrap();
        assert!(matches!(worker.make_a_step_of_the_lead_up(), Next::GoOn));
        assert!(worker.has_a_lead_up(), "it is not over until it is told it is");
        // Nothing is there to be made yet: a step waits a moment, and goes on.
        assert!(matches!(worker.make_a_step_of_the_lead_up(), Next::GoOn));
        assert!(worker.has_a_lead_up());
        assert!(what_was_said(told).is_empty(), "and it says nothing of the frames");
        drop(sender);
    });
}

#[test]
fn a_lead_up_that_is_cancelled_is_let_go_of_with_the_pictures_it_sent_and_nothing_is_said_of_it() {
    with_the_accumulating_effect(checkpoints_every(60), |worker, told| {
        let sender = start_a_lead_up(worker, 3);
        let its_picture = PictureId(1).in_the_lead_up();
        sender
            .send(LeadUpItem::Picture {
                id: its_picture,
                data: Arc::new(picture_1().as_picture_data()),
            })
            .unwrap();
        sender
            .send(LeadUpItem::Frame(frame_of_the_effect_at(its_picture.0, 0)))
            .unwrap();
        assert!(matches!(worker.make_a_step_of_the_lead_up(), Next::GoOn));
        assert!(matches!(worker.make_a_step_of_the_lead_up(), Next::GoOn));
        assert!(
            worker.renderer.picture_size(its_picture).is_some(),
            "it has its picture"
        );

        // A cancel of another lead-up does nothing.
        assert!(matches!(
            worker.take(vec![EngineRequest::CancelTheLeadUp { generation: 4 }]),
            Next::GoOn
        ));
        assert!(worker.has_a_lead_up());
        assert!(sender.send(LeadUpItem::Forget(Vec::new())).is_ok());

        assert!(matches!(
            worker.take(vec![EngineRequest::CancelTheLeadUp { generation: 3 }]),
            Next::GoOn
        ));
        assert!(!worker.has_a_lead_up());
        assert!(
            sender.send(LeadUpItem::End).is_err(),
            "nobody is listening: the thread that describes the frames finds that out"
        );
        assert!(
            worker.renderer.picture_size(its_picture).is_none(),
            "its picture is gone"
        );
        assert!(
            worker.renderer.picture_size(PictureId(1)).is_some(),
            "and the picture of the preview, which has another id, is not"
        );
        assert!(what_was_said(told).is_empty(), "the window that cancelled it knows");
    });
}

#[test]
fn a_lead_up_that_is_started_lets_go_of_the_one_that_was_going_on() {
    with_the_accumulating_effect(checkpoints_every(60), |worker, told| {
        let first = start_a_lead_up(worker, 1);
        let second = start_a_lead_up(worker, 2);
        assert!(
            first.send(LeadUpItem::End).is_err(),
            "the first has nobody listening now"
        );
        second.send(LeadUpItem::Frame(frame_of_the_effect_at(1, 0))).unwrap();
        second.send(LeadUpItem::End).unwrap();
        go_on_until_the_lead_up_is_over(worker);
        let said = what_was_said(told);
        assert!(
            matches!(
                said.as_slice(),
                [EngineEvent::LeadUpDone {
                    generation: 2,
                    frames_made: 1
                }]
            ),
            "{said:?}"
        );
    });
}

#[test]
fn a_lead_up_that_fails_says_why_and_lets_go_of_its_pictures() {
    with_the_accumulating_effect(checkpoints_every(60), |worker, told| {
        let sender = start_a_lead_up(worker, 5);
        let its_picture = PictureId(1).in_the_lead_up();
        sender
            .send(LeadUpItem::Picture {
                id: its_picture,
                data: Arc::new(picture_1().as_picture_data()),
            })
            .unwrap();
        sender
            .send(LeadUpItem::Failed("the video cannot be read".to_owned()))
            .unwrap();
        go_on_until_the_lead_up_is_over(worker);
        let said = what_was_said(told);
        assert!(
            matches!(said.as_slice(), [EngineEvent::LeadUpFailed { generation: 5, problem }] if problem == "the video cannot be read"),
            "{said:?}"
        );
        assert!(worker.renderer.picture_size(its_picture).is_none());
    });
}

#[test]
fn a_lead_up_whose_frame_is_made_of_a_picture_that_is_not_there_fails() {
    with_the_accumulating_effect(checkpoints_every(60), |worker, told| {
        let sender = start_a_lead_up(worker, 6);
        sender.send(LeadUpItem::Frame(frame_of_the_effect_at(99, 0))).unwrap();
        go_on_until_the_lead_up_is_over(worker);
        let said = what_was_said(told);
        assert!(
            matches!(said.as_slice(), [EngineEvent::LeadUpFailed { generation: 6, problem }] if problem.contains("were not there")),
            "{said:?}"
        );
    });
}

#[test]
fn a_picture_that_a_lead_up_lets_go_of_is_gone_and_a_frame_that_is_made_of_it_after_that_fails_the_lead_up() {
    with_the_accumulating_effect(checkpoints_every(60), |worker, told| {
        let sender = start_a_lead_up(worker, 8);
        let its_picture = PictureId(1).in_the_lead_up();
        sender
            .send(LeadUpItem::Picture {
                id: its_picture,
                data: Arc::new(picture_1().as_picture_data()),
            })
            .unwrap();
        sender
            .send(LeadUpItem::Frame(frame_of_the_effect_at(its_picture.0, 0)))
            .unwrap();
        sender.send(LeadUpItem::Forget(vec![its_picture])).unwrap();
        for _ in 0..3 {
            assert!(matches!(worker.make_a_step_of_the_lead_up(), Next::GoOn));
        }
        assert!(worker.has_a_lead_up());
        assert!(worker.renderer.picture_size(its_picture).is_none(), "it is gone");
        sender
            .send(LeadUpItem::Frame(frame_of_the_effect_at(its_picture.0, 1)))
            .unwrap();
        go_on_until_the_lead_up_is_over(worker);
        let said = what_was_said(told);
        assert!(
            matches!(said.as_slice(), [EngineEvent::LeadUpFailed { generation: 8, problem }] if problem.contains("were not there")),
            "{said:?}"
        );
    });
}

#[test]
fn a_lead_up_whose_thread_ends_without_an_end_fails() {
    with_the_accumulating_effect(checkpoints_every(60), |worker, told| {
        let sender = start_a_lead_up(worker, 9);
        sender.send(LeadUpItem::Frame(frame_of_the_effect_at(1, 0))).unwrap();
        drop(sender);
        go_on_until_the_lead_up_is_over(worker);
        let said = what_was_said(told);
        assert!(
            matches!(said.as_slice(), [EngineEvent::LeadUpFailed { generation: 9, problem }] if problem.contains("ended before it was done")),
            "{said:?}"
        );
    });
}

/// Makes a frame of the preview, which the tests of the watchdog do before they destroy the device: **a device on which nothing has been made does not say that it is lost for a while** (found by
/// experiment, 2026-10-22, three runs: the first check after the destroy said nothing and one 20 milliseconds later said it; after a frame it says it at once), and what these tests are about is what the
/// engine does when it is told.
fn made_a_frame_so_that_the_card_says_when_it_is_lost(worker: &mut Worker<'_>, told: &Told) {
    assert!(matches!(worker.answer(100, frame_of_the_effect_at(1, 0)), Next::GoOn));
    what_was_said(told);
}

#[test]
fn a_frame_of_a_lead_up_that_the_card_cannot_make_fails_the_lead_up_and_is_answered_as_any_frame_that_fails_is() {
    with_the_accumulating_effect(checkpoints_every(60), |worker, told| {
        let sender = start_a_lead_up(worker, 10);
        made_a_frame_so_that_the_card_says_when_it_is_lost(worker, told);
        worker.renderer.device().destroy();
        sender.send(LeadUpItem::Frame(frame_of_the_effect_at(1, 0))).unwrap();
        assert!(matches!(worker.make_a_step_of_the_lead_up(), Next::GoOn));
        assert!(!worker.has_a_lead_up());
        let said = what_was_said(told);
        assert!(
            matches!(
                said.as_slice(),
                [
                    EngineEvent::LeadUpFailed { generation: 10, .. },
                    EngineEvent::FrameFailed { serial: 0, .. },
                    EngineEvent::DeviceReplaced { .. },
                ]
            ),
            "the lead-up fails, then the frame, then the device is replaced as it is for any frame: {said:?}"
        );
    });
}

#[test]
fn a_device_that_is_replaced_while_a_lead_up_goes_on_ends_it() {
    with_the_accumulating_effect(checkpoints_every(60), |worker, told| {
        let sender = start_a_lead_up(worker, 11);
        made_a_frame_so_that_the_card_says_when_it_is_lost(worker, told);
        worker.renderer.device().destroy();
        // A frame of the preview is what finds it out.
        assert!(matches!(worker.answer(1, frame_of_the_effect_at(1, 0)), Next::GoOn));
        assert!(!worker.has_a_lead_up());
        let said = what_was_said(told);
        assert!(
            said.iter().any(|event| matches!(event,
                EngineEvent::LeadUpFailed { generation: 11, problem } if problem.contains("replaced"))),
            "{said:?}"
        );
        assert!(sender.send(LeadUpItem::End).is_err());
    });
}

#[test]
fn what_is_waiting_is_all_that_the_window_has_asked_and_the_engine_has_not_looked_at() {
    let (sender, requests) = mpsc::channel();
    assert!(matches!(what_is_waiting(&requests), Waiting::Nothing));
    sender.send(EngineRequest::ForgetTheCheckpoints).unwrap();
    sender.send(EngineRequest::Forget(Vec::new())).unwrap();
    assert!(matches!(what_is_waiting(&requests), Waiting::Requests(batch) if batch.len() == 2));
    assert!(
        matches!(what_is_waiting(&requests), Waiting::Nothing),
        "they were taken"
    );
    sender.send(EngineRequest::ForgetTheCheckpoints).unwrap();
    drop(sender);
    assert!(
        matches!(what_is_waiting(&requests), Waiting::Requests(batch) if batch.len() == 1),
        "what was asked before the window went is still done"
    );
    assert!(matches!(what_is_waiting(&requests), Waiting::NobodyIsListening));
}

// ---- the thread ----

/// The engine of the window's kind: its own thread, the effect that adds a tenth of red, the picture 1, and rules of checkpoints.
fn an_engine_with_the_accumulating_effect() -> Option<crate::engine::Engine> {
    let _graphics = software_graphics()?;
    let (engine, _woken) = software_engine();
    engine
        .compile_effect_shader_with_structure(
            "test:accumulate",
            &format!("{THE_PICTURE_AND_A_BUFFER}\n{ADD_A_TENTH_OF_RED}"),
            one_buffer_that_lasts(),
        )
        .unwrap();
    engine.upload_picture(PictureId(1), Arc::new(picture_1().as_picture_data()));
    engine.set_the_rules_of_checkpoints(checkpoints_every(60));
    Some(engine)
}

/// The frame that the engine makes next (any other thing it says fails the test).
fn the_next_frame(engine: &crate::engine::Engine) -> (Arc<RenderedFrame>, FrameStatistics) {
    match events_of(engine, 1).remove(0) {
        EngineEvent::Frame { frame, statistics, .. } => (frame, statistics),
        other => panic!("not a frame: {other:?}"),
    }
}

#[test]
fn the_engine_makes_a_frame_of_the_preview_while_a_lead_up_waits_for_its_next_frame_and_then_goes_on_with_it() {
    let Some(mut engine) = an_engine_with_the_accumulating_effect() else {
        return;
    };
    let (sender, items) = mpsc::channel();
    engine.start_the_lead_up(1, items);
    // The thread that describes the frames has nothing for the engine yet. A frame of the preview is made all the same, and at once.
    engine.request_frame(frame_of_the_effect_at(1, 5));
    let (frame, statistics) = the_next_frame(&engine);
    assert!(near(red_in(&frame), tenths(1)), "it starts clear: {}", red_in(&frame));
    assert!(statistics.approximate);

    // Then the lead-up goes on, in the engine's own time, and says when it is done.
    for number in 0..5 {
        sender
            .send(LeadUpItem::Frame(frame_of_the_effect_at(1, number)))
            .unwrap();
    }
    sender.send(LeadUpItem::End).unwrap();
    assert!(
        matches!(
            events_of(&engine, 1).as_slice(),
            [EngineEvent::LeadUpDone {
                generation: 1,
                frames_made: 5
            }]
        ),
        "done"
    );
    engine.request_frame(frame_of_the_effect_at(1, 5));
    let (frame, statistics) = the_next_frame(&engine);
    assert!(
        near(red_in(&frame), tenths(6)),
        "it goes on from the lead-up: {}",
        red_in(&frame)
    );
    assert!(!statistics.approximate);
}

#[test]
fn a_lead_up_that_the_engine_is_told_to_cancel_stops_its_thread_from_sending_and_the_engine_goes_on() {
    let Some(mut engine) = an_engine_with_the_accumulating_effect() else {
        return;
    };
    let (sender, items) = mpsc::channel();
    engine.start_the_lead_up(2, items);
    engine.cancel_the_lead_up(2);
    // (The cancel is looked at when the engine's thread gets to it: the thread that describes the frames finds out when it sends.)
    let started = Instant::now();
    while sender.send(LeadUpItem::Forget(Vec::new())).is_ok() {
        assert!(
            started.elapsed() < A_LONG_TIME,
            "the engine did not let go of the lead-up"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    // And a frame of the preview is made as it always was.
    engine.request_frame(frame_of_the_effect_at(1, 0));
    let (frame, statistics) = the_next_frame(&engine);
    assert!(near(red_in(&frame), tenths(1)));
    assert!(!statistics.approximate);
}

#[test]
fn a_lead_up_that_is_replaced_lets_go_of_the_pictures_that_the_one_before_it_sent() {
    with_the_accumulating_effect(checkpoints_every(60), |worker, _told| {
        let first = start_a_lead_up(worker, 1);
        let its_picture = PictureId(1).in_the_lead_up();
        first
            .send(LeadUpItem::Picture {
                id: its_picture,
                data: Arc::new(picture_1().as_picture_data()),
            })
            .unwrap();
        assert!(matches!(worker.make_a_step_of_the_lead_up(), Next::GoOn));
        assert!(worker.renderer.picture_size(its_picture).is_some());
        let _second = start_a_lead_up(worker, 2);
        assert!(worker.renderer.picture_size(its_picture).is_none());
        assert!(
            worker.renderer.picture_size(PictureId(1)).is_some(),
            "the preview's picture is not touched"
        );
    });
}

#[test]
fn a_lead_up_that_is_whole_lets_go_of_the_pictures_that_it_sent() {
    with_the_accumulating_effect(checkpoints_every(60), |worker, told| {
        let sender = start_a_lead_up(worker, 4);
        let its_picture = PictureId(1).in_the_lead_up();
        sender
            .send(LeadUpItem::Picture {
                id: its_picture,
                data: Arc::new(picture_1().as_picture_data()),
            })
            .unwrap();
        sender
            .send(LeadUpItem::Frame(frame_of_the_effect_at(its_picture.0, 0)))
            .unwrap();
        sender.send(LeadUpItem::End).unwrap();
        go_on_until_the_lead_up_is_over(worker);
        let said = what_was_said(told);
        assert!(
            matches!(
                said.as_slice(),
                [EngineEvent::LeadUpDone {
                    generation: 4,
                    frames_made: 1
                }]
            ),
            "{said:?}"
        );
        assert!(
            worker.renderer.picture_size(its_picture).is_none(),
            "its picture is gone"
        );
        assert!(worker.renderer.picture_size(PictureId(1)).is_some());
    });
}

#[test]
fn a_lead_up_whose_picture_the_engine_cannot_use_fails_and_says_so() {
    with_the_accumulating_effect(checkpoints_every(60), |worker, told| {
        let sender = start_a_lead_up(worker, 5);
        let mut empty = picture_1().as_picture_data();
        empty.width_in_pixels = 0;
        sender
            .send(LeadUpItem::Picture {
                id: PictureId(2).in_the_lead_up(),
                data: Arc::new(empty),
            })
            .unwrap();
        go_on_until_the_lead_up_is_over(worker);
        let said = what_was_said(told);
        assert!(
            matches!(said.as_slice(), [EngineEvent::LeadUpFailed { generation: 5, problem }] if problem.contains("could not be used")),
            "{said:?}"
        );
    });
}
