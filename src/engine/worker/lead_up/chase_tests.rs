// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The chase as the engine's thread makes it (EN-14, the lead-up while the project plays): frames that go through states of their own, one after another, until those are of the same frame as the
//! ones that the last frame of the preview used, and then go over to the preview's. Like `tests`, most of these talk to a `Worker` the way the thread's loop does, with the frames of the chase put
//! in the channel before it is stepped; the last ones start the engine, as the window does.
//!
//! The effect is the one that adds a tenth of red to what it kept: the red of the picture is how many frames the state has been through.

use std::sync::Arc;
use std::sync::mpsc::{self, Sender};

use super::tests::{
    an_engine_with_the_accumulating_effect, checkpoints_every, frame_of_the_effect_at, go_on_until_the_lead_up_is_over,
    red_in, the_next_frame, with_the_accumulating_effect,
};
use super::*;
use crate::engine::EngineRequest;
use crate::engine::engine_tests::events_of;
use crate::engine::persistent_buffer_tests::{near, tenths};
use crate::engine::worker::tests::{Told, picture_1, the_frame_in, what_was_said};

/// Starts a chase with this generation, and gives the end of the channel that the thread that describes the frames has.
fn start_a_chase(worker: &mut Worker<'_>, generation: u64) -> Sender<LeadUpItem> {
    let (sender, items) = mpsc::channel();
    assert!(matches!(
        worker.take(vec![EngineRequest::StartTheChase { generation, items }]),
        Next::GoOn
    ));
    sender
}

/// The preview makes the frames `frames` of the effect, as it does when the playhead has jumped to the first of them and the project plays: from nothing, so none is exact.
fn the_preview_jumps_and_goes_on_through(worker: &mut Worker<'_>, told: &Told, frames: std::ops::Range<u64>) {
    for frame in frames {
        assert!(matches!(
            worker.answer(frame, frame_of_the_effect_at(1, frame)),
            Next::GoOn
        ));
    }
    what_was_said(told);
}

/// What the preview makes of frame `frame` of the effect: its red, and whether it says it is not exact.
fn the_preview_makes(worker: &mut Worker<'_>, told: &Told, frame: u64) -> (i32, bool) {
    assert!(matches!(
        worker.answer(1000 + frame, frame_of_the_effect_at(1, frame)),
        Next::GoOn
    ));
    let said = what_was_said(told);
    let (picture, statistics) = the_frame_in(&said[0]);
    (red_in(picture), statistics.approximate)
}

fn said_it_was_done(said: &[EngineEvent], generation: u64, frames_made: u64) -> bool {
    matches!(said, [EngineEvent::LeadUpDone { generation: g, frames_made: n }] if *g == generation && *n == frames_made)
}

#[test]
fn a_chase_that_has_caught_up_with_the_preview_hands_its_states_over_and_the_pictures_are_exact_from_then_on() {
    with_the_accumulating_effect(checkpoints_every(1000), |worker, told| {
        the_preview_jumps_and_goes_on_through(worker, told, 5..8);
        let sender = start_a_chase(worker, 3);
        for frame in 0..8 {
            sender
                .send(LeadUpItem::Frame(frame_of_the_effect_at(1, frame)))
                .unwrap();
        }
        go_on_until_the_lead_up_is_over(worker);
        let said = what_was_said(told);
        assert!(
            said_it_was_done(&said, 3, 8),
            "it ends by catching up, at frame 7, without being told that there are no more frames: {said:?}"
        );
        let (red, approximate) = the_preview_makes(worker, told, 8);
        assert!(
            near(red, tenths(9)),
            "all the frames before it went into the state: {red}"
        );
        assert!(!approximate);
        assert_eq!(
            worker.renderer.states_of_a_chase_kept(),
            (0, 0),
            "the chase's own store is gone"
        );
    });
}

#[test]
fn a_chase_is_not_over_while_it_is_behind_the_preview() {
    with_the_accumulating_effect(checkpoints_every(1000), |worker, told| {
        the_preview_jumps_and_goes_on_through(worker, told, 5..8);
        let sender = start_a_chase(worker, 1);
        for frame in 0..7 {
            sender
                .send(LeadUpItem::Frame(frame_of_the_effect_at(1, frame)))
                .unwrap();
            assert!(matches!(worker.make_a_step_of_the_lead_up(), Next::GoOn));
            assert!(worker.has_a_lead_up(), "after frame {frame}, which is before frame 7");
            assert!(what_was_said(told).is_empty());
        }
        sender.send(LeadUpItem::Frame(frame_of_the_effect_at(1, 7))).unwrap();
        assert!(matches!(worker.make_a_step_of_the_lead_up(), Next::GoOn));
        assert!(!worker.has_a_lead_up(), "frame 7 is where the preview is");
        assert!(said_it_was_done(&what_was_said(told), 1, 8));
    });
}

#[test]
fn a_chase_catches_up_with_a_preview_that_goes_on_while_it_runs() {
    with_the_accumulating_effect(checkpoints_every(1000), |worker, told| {
        the_preview_jumps_and_goes_on_through(worker, told, 2..5);
        let sender = start_a_chase(worker, 2);
        for frame in 0..2 {
            sender
                .send(LeadUpItem::Frame(frame_of_the_effect_at(1, frame)))
                .unwrap();
            assert!(matches!(worker.make_a_step_of_the_lead_up(), Next::GoOn));
        }
        // The project plays on: the preview makes frame 5, which the chase has to reach, and not frame 4, which it would have reached.
        the_preview_jumps_and_goes_on_through(worker, told, 5..6);
        for frame in 2..5 {
            sender
                .send(LeadUpItem::Frame(frame_of_the_effect_at(1, frame)))
                .unwrap();
            assert!(matches!(worker.make_a_step_of_the_lead_up(), Next::GoOn));
            assert!(worker.has_a_lead_up(), "frame {frame} is before frame 5");
        }
        sender.send(LeadUpItem::Frame(frame_of_the_effect_at(1, 5))).unwrap();
        assert!(matches!(worker.make_a_step_of_the_lead_up(), Next::GoOn));
        assert!(said_it_was_done(&what_was_said(told), 2, 6));
        let (red, approximate) = the_preview_makes(worker, told, 6);
        assert!(near(red, tenths(7)), "{red}");
        assert!(!approximate);
    });
}

#[test]
fn a_chase_whose_frames_are_over_before_it_has_caught_up_fails_and_leaves_the_preview_as_it_was() {
    with_the_accumulating_effect(checkpoints_every(1000), |worker, told| {
        the_preview_jumps_and_goes_on_through(worker, told, 5..8);
        let sender = start_a_chase(worker, 4);
        for frame in 0..4 {
            sender
                .send(LeadUpItem::Frame(frame_of_the_effect_at(1, frame)))
                .unwrap();
        }
        sender.send(LeadUpItem::End).unwrap();
        go_on_until_the_lead_up_is_over(worker);
        let said = what_was_said(told);
        assert!(
            matches!(said.as_slice(), [EngineEvent::LeadUpFailed { generation: 4, problem }] if problem.contains("caught up")),
            "{said:?}"
        );
        assert_eq!(
            worker.renderer.states_of_a_chase_kept(),
            (0, 0),
            "what it made is let go of"
        );
        let (red, approximate) = the_preview_makes(worker, told, 8);
        assert!(near(red, tenths(4)), "the preview goes on from its own state: {red}");
        assert!(approximate);
    });
}

#[test]
fn a_chase_that_is_ahead_of_the_preview_fails_at_once() {
    with_the_accumulating_effect(checkpoints_every(1000), |worker, told| {
        the_preview_jumps_and_goes_on_through(worker, told, 3..4);
        let sender = start_a_chase(worker, 5);
        // (The playhead went back, and the thread that describes the frames did not hear of it.)
        sender.send(LeadUpItem::Frame(frame_of_the_effect_at(1, 5))).unwrap();
        go_on_until_the_lead_up_is_over(worker);
        let said = what_was_said(told);
        assert!(
            matches!(said.as_slice(), [EngineEvent::LeadUpFailed { generation: 5, problem }] if problem.contains("later")),
            "{said:?}"
        );
        assert_eq!(worker.renderer.states_of_a_chase_kept(), (0, 0));
    });
}

#[test]
fn a_chase_that_the_preview_needs_nothing_from_is_over_after_its_first_frame() {
    with_the_accumulating_effect(checkpoints_every(1000), |worker, told| {
        // The last frame of the preview has no effect that has a state.
        assert!(matches!(worker.answer(1, FrameDescription::black((8, 8))), Next::GoOn));
        what_was_said(told);
        let sender = start_a_chase(worker, 6);
        for frame in 0..3 {
            sender
                .send(LeadUpItem::Frame(frame_of_the_effect_at(1, frame)))
                .unwrap();
        }
        go_on_until_the_lead_up_is_over(worker);
        assert!(said_it_was_done(&what_was_said(told), 6, 1));
    });
}

#[test]
fn a_chase_that_is_cancelled_lets_go_of_its_states_and_pictures_and_says_nothing() {
    with_the_accumulating_effect(checkpoints_every(1000), |worker, told| {
        the_preview_jumps_and_goes_on_through(worker, told, 5..8);
        let sender = start_a_chase(worker, 7);
        let its_picture = PictureId(1).in_the_lead_up();
        sender
            .send(LeadUpItem::Picture {
                id: its_picture,
                data: Arc::new(picture_1().as_picture_data()),
            })
            .unwrap();
        for frame in 0..3 {
            sender
                .send(LeadUpItem::Frame(frame_of_the_effect_at(its_picture.0, frame)))
                .unwrap();
        }
        for _ in 0..4 {
            assert!(matches!(worker.make_a_step_of_the_lead_up(), Next::GoOn));
        }
        assert_eq!(worker.renderer.states_of_a_chase_kept().0, 1);
        assert!(worker.renderer.picture_size(its_picture).is_some());

        assert!(matches!(
            worker.take(vec![EngineRequest::CancelTheLeadUp { generation: 7 }]),
            Next::GoOn
        ));
        assert!(!worker.has_a_lead_up());
        assert_eq!(worker.renderer.states_of_a_chase_kept(), (0, 0));
        assert!(worker.renderer.picture_size(its_picture).is_none());
        assert!(what_was_said(told).is_empty());
        let (red, approximate) = the_preview_makes(worker, told, 8);
        assert!(near(red, tenths(4)), "the preview was not touched: {red}");
        assert!(approximate);
    });
}

#[test]
fn a_chase_that_is_replaced_by_a_lead_up_lets_go_of_what_it_made() {
    with_the_accumulating_effect(checkpoints_every(1000), |worker, told| {
        the_preview_jumps_and_goes_on_through(worker, told, 5..8);
        let chase = start_a_chase(worker, 1);
        chase.send(LeadUpItem::Frame(frame_of_the_effect_at(1, 0))).unwrap();
        assert!(matches!(worker.make_a_step_of_the_lead_up(), Next::GoOn));
        assert_eq!(worker.renderer.states_of_a_chase_kept().0, 1);
        let _lead_up = {
            let (sender, items) = mpsc::channel();
            assert!(matches!(
                worker.take(vec![EngineRequest::StartTheLeadUp { generation: 2, items }]),
                Next::GoOn
            ));
            sender
        };
        assert_eq!(worker.renderer.states_of_a_chase_kept(), (0, 0));
        assert!(
            chase.send(LeadUpItem::End).is_err(),
            "nobody listens to the chase any more"
        );
    });
}

#[test]
fn what_the_window_asks_comes_before_the_next_frame_of_a_chase_and_the_chase_goes_on_after_it() {
    with_the_accumulating_effect(checkpoints_every(1000), |worker, told| {
        the_preview_jumps_and_goes_on_through(worker, told, 2..5);
        let sender = start_a_chase(worker, 8);
        sender.send(LeadUpItem::Frame(frame_of_the_effect_at(1, 0))).unwrap();
        assert!(matches!(worker.make_a_step_of_the_lead_up(), Next::GoOn));
        // A frame of the preview comes in the middle, and is made with the preview's states: frame 5 goes on from frame 4, and is the fourth tenth.
        let (red, approximate) = the_preview_makes(worker, told, 5);
        assert!(near(red, tenths(4)), "{red}");
        assert!(approximate);
        assert!(worker.has_a_lead_up());
        // The chase's own state is where the chase left it: it goes on with frame 1.
        for frame in 1..=5 {
            sender
                .send(LeadUpItem::Frame(frame_of_the_effect_at(1, frame)))
                .unwrap();
        }
        go_on_until_the_lead_up_is_over(worker);
        assert!(
            said_it_was_done(&what_was_said(told), 8, 6),
            "it caught up with frame 5"
        );
        let (red, approximate) = the_preview_makes(worker, told, 6);
        assert!(near(red, tenths(7)), "{red}");
        assert!(!approximate);
    });
}

#[test]
fn the_engine_makes_the_frames_of_a_chase_for_its_own_states_and_says_when_it_has_caught_up_with_the_preview() {
    let Some(mut engine) = an_engine_with_the_accumulating_effect() else {
        return;
    };
    // The preview made frames 5 to 7 from nothing.
    for frame in 5..8 {
        engine.request_frame(frame_of_the_effect_at(1, frame));
        let (picture, statistics) = the_next_frame(&engine);
        assert!(near(red_in(&picture), tenths(frame as i32 - 4)));
        assert!(statistics.approximate);
    }
    let (sender, items) = mpsc::channel();
    engine.start_the_chase(1, items);
    for frame in 0..8 {
        sender
            .send(LeadUpItem::Frame(frame_of_the_effect_at(1, frame)))
            .unwrap();
    }
    assert!(
        said_it_was_done(&events_of(&engine, 1), 1, 8),
        "the chase says it is done when it has caught up, and the preview's frames before it are not touched"
    );
    engine.request_frame(frame_of_the_effect_at(1, 8));
    let (picture, statistics) = the_next_frame(&engine);
    assert!(near(red_in(&picture), tenths(9)), "{}", red_in(&picture));
    assert!(!statistics.approximate);
}
