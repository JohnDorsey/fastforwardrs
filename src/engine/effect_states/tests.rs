// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::engine::testing::with_software_graphics;

fn wanted(width: u32, height: u32) -> Option<BufferWanted> {
    Some(BufferWanted {
        size: (width, height),
        format: TargetFormat::Half,
    })
}

#[test]
fn a_call_goes_on_from_the_frame_before_makes_the_same_frame_again_and_starts_clear_for_any_other() {
    assert_eq!(continuation(None, 0), Continuation::StartClear);
    assert_eq!(continuation(None, 7), Continuation::StartClear);
    assert_eq!(continuation(Some(4), 5), Continuation::GoOn);
    assert_eq!(continuation(Some(4), 4), Continuation::SameFrame);
    assert_eq!(
        continuation(Some(0), 0),
        Continuation::SameFrame,
        "the first frame again is the first frame again"
    );
    assert_eq!(continuation(Some(4), 6), Continuation::StartClear, "a frame skipped");
    assert_eq!(continuation(Some(4), 3), Continuation::StartClear, "a step back");
    assert_eq!(
        continuation(Some(40), 0),
        Continuation::StartClear,
        "the start of the clip again"
    );
    assert_eq!(continuation(Some(u64::MAX), 0), Continuation::StartClear, "no overflow");
}

#[test]
fn the_first_call_of_a_state_starts_clear_and_is_given_a_place_for_each_buffer_that_lasts() {
    with_software_graphics!(graphics, {
        let mut states = StatesOfEffects::default();
        let buffers = states.prepare(&graphics.device.device, 1, 0, &[None, wanted(8, 8), wanted(4, 2)]);
        assert_eq!(buffers.continuation, Continuation::StartClear);
        assert_eq!(
            buffers.base.iter().map(Option::is_some).collect::<Vec<_>>(),
            [false, true, true]
        );
        assert_eq!(
            buffers.target.iter().map(Option::is_some).collect::<Vec<_>>(),
            [false, true, true]
        );
        assert_eq!(buffers.to_clear.len(), 2, "the state before the frame is cleared");
        // Two textures for each buffer, in half floats (8 bytes a pixel).
        assert_eq!(states.len(), 1);
        assert_eq!(states.bytes(), 2 * (8 * 8 * 8) + 2 * (4 * 2 * 8));
    });
}

#[test]
fn what_is_committed_is_what_the_next_call_goes_on_from_and_what_is_not_is_not() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        let first = states.prepare(device, 1, 0, &[wanted(4, 4)]);
        // (Not committed yet: another call at the next frame still has nothing to go on from.)
        assert_eq!(first.continuation, Continuation::StartClear);
        assert_eq!(
            states.prepare(device, 1, 1, &[wanted(4, 4)]).continuation,
            Continuation::StartClear
        );

        states.commit(1, 0, Continuation::StartClear);
        assert_eq!(
            states.prepare(device, 1, 1, &[wanted(4, 4)]).continuation,
            Continuation::GoOn
        );
        assert_eq!(
            states.prepare(device, 1, 0, &[wanted(4, 4)]).continuation,
            Continuation::SameFrame
        );
        assert_eq!(
            states.prepare(device, 1, 2, &[wanted(4, 4)]).continuation,
            Continuation::StartClear
        );

        states.commit(1, 1, Continuation::GoOn);
        assert_eq!(
            states.prepare(device, 1, 2, &[wanted(4, 4)]).continuation,
            Continuation::GoOn
        );
        // The state is of frame 1 now: frame 0 is a step back.
        assert_eq!(
            states.prepare(device, 1, 0, &[wanted(4, 4)]).continuation,
            Continuation::StartClear
        );
    });
}

#[test]
fn a_call_that_goes_on_clears_nothing_and_one_that_starts_clear_clears_the_state_before_the_frame() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        let first = states.prepare(device, 1, 0, &[wanted(4, 4), wanted(2, 2)]);
        states.commit(1, 0, first.continuation);
        let goes_on = states.prepare(device, 1, 1, &[wanted(4, 4), wanted(2, 2)]);
        assert_eq!(goes_on.continuation, Continuation::GoOn);
        assert!(goes_on.to_clear.is_empty());
        let again = states.prepare(device, 1, 0, &[wanted(4, 4), wanted(2, 2)]);
        assert_eq!(again.continuation, Continuation::SameFrame);
        assert!(again.to_clear.is_empty(), "the state before frame 0 is what it was");
        let elsewhere = states.prepare(device, 1, 9, &[wanted(4, 4), wanted(2, 2)]);
        assert_eq!(elsewhere.continuation, Continuation::StartClear);
        assert_eq!(elsewhere.to_clear.len(), 2);
    });
}

#[test]
fn a_buffer_that_changes_size_or_format_or_that_was_not_there_makes_the_whole_call_start_clear() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        let first = states.prepare(device, 1, 0, &[wanted(4, 4), wanted(2, 2)]);
        states.commit(1, 0, first.continuation);
        assert_eq!(
            states.prepare(device, 1, 1, &[wanted(4, 4), wanted(2, 2)]).continuation,
            Continuation::GoOn
        );
        let bigger = states.prepare(device, 1, 1, &[wanted(4, 4), wanted(3, 2)]);
        assert_eq!(
            bigger.continuation,
            Continuation::StartClear,
            "the second buffer is another size"
        );
        assert_eq!(bigger.to_clear.len(), 2, "all of the state before the frame is clear");
        states.commit(1, 1, bigger.continuation);
        assert_eq!(
            states.bytes(),
            2 * (4 * 4 * 8) + 2 * (3 * 2 * 8),
            "the textures of the old size are gone"
        );

        let eight_bits = Some(BufferWanted {
            size: (4, 4),
            format: TargetFormat::EightBit,
        });
        let other_format = states.prepare(device, 1, 2, &[eight_bits, wanted(3, 2)]);
        assert_eq!(other_format.continuation, Continuation::StartClear);
        states.commit(1, 2, other_format.continuation);
        assert_eq!(
            states.bytes(),
            2 * (4 * 4 * 4) + 2 * (3 * 2 * 8),
            "8 bits of each colour are 4 bytes a pixel"
        );

        let one_more = states.prepare(device, 1, 3, &[eight_bits, wanted(3, 2), wanted(1, 1)]);
        assert_eq!(one_more.continuation, Continuation::StartClear);
    });
}

#[test]
fn two_states_do_not_share_anything() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        let a = states.prepare(device, 1, 0, &[wanted(4, 4)]);
        states.commit(1, 0, a.continuation);
        assert_eq!(
            states.prepare(device, 2, 1, &[wanted(4, 4)]).continuation,
            Continuation::StartClear
        );
        assert_eq!(
            states.prepare(device, 1, 1, &[wanted(4, 4)]).continuation,
            Continuation::GoOn
        );
        assert_eq!(states.len(), 2);
    });
}

#[test]
fn a_state_that_is_abandoned_is_gone_and_its_effect_starts_clear() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        let first = states.prepare(device, 1, 0, &[wanted(4, 4)]);
        states.commit(1, 0, first.continuation);
        let _frame_that_failed = states.prepare(device, 1, 1, &[wanted(4, 4)]);
        states.abandon(1);
        assert_eq!(states.len(), 0);
        assert_eq!(states.bytes(), 0);
        assert_eq!(
            states.prepare(device, 1, 1, &[wanted(4, 4)]).continuation,
            Continuation::StartClear
        );
        // Committing and abandoning what is not there does nothing.
        states.commit(77, 3, Continuation::GoOn);
        states.abandon(78);
        assert_eq!(states.len(), 1);
    });
}

#[test]
fn a_state_that_has_not_been_used_for_a_long_time_is_let_go_of_and_one_that_is_used_is_not() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        for id in [1, 2] {
            let call = states.prepare(device, id, 0, &[wanted(2, 2)]);
            states.commit(id, 0, call.continuation);
        }
        states.end_frame();
        for frame in 1..=FRAMES_BEFORE_A_STATE_IS_LET_GO_OF {
            // The second is used in every frame, the first is not.
            let call = states.prepare(device, 2, frame, &[wanted(2, 2)]);
            states.commit(2, frame, call.continuation);
            states.end_frame();
        }
        assert_eq!(states.len(), 1, "the first has been let go of");
        assert_eq!(
            states
                .prepare(device, 2, FRAMES_BEFORE_A_STATE_IS_LET_GO_OF + 1, &[wanted(2, 2)])
                .continuation,
            Continuation::GoOn
        );
        assert_eq!(
            states.prepare(device, 1, 1, &[wanted(2, 2)]).continuation,
            Continuation::StartClear
        );
    });
}

#[test]
fn a_state_used_just_within_the_time_is_kept() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        let call = states.prepare(device, 1, 0, &[wanted(2, 2)]);
        states.commit(1, 0, call.continuation);
        for _ in 0..FRAMES_BEFORE_A_STATE_IS_LET_GO_OF {
            states.end_frame();
        }
        assert_eq!(states.len(), 1, "unused for exactly as many frames as it may be");
        states.end_frame();
        assert_eq!(states.len(), 0, "one more");
    });
}

#[test]
fn when_there_are_too_many_bytes_the_state_used_longest_ago_goes_first() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        // Three states of 2 * 8 * 8 * 8 = 1024 bytes each, used in the order 1, 2, 3.
        for id in [1, 2, 3] {
            let call = states.prepare(device, id, 0, &[wanted(8, 8)]);
            states.commit(id, 0, call.continuation);
            states.end_frame_within(u64::MAX);
        }
        assert_eq!(states.bytes(), 3 * 1024);
        states.end_frame_within(2 * 1024);
        assert_eq!(states.len(), 2);
        // (The first was used longest ago: it is not there, and a call of it starts clear. It is let go of again, to count the others.)
        assert_eq!(
            states.prepare(device, 1, 1, &[wanted(8, 8)]).continuation,
            Continuation::StartClear,
            "the first is gone"
        );
        states.abandon(1);
        // The third is used, and then, in a frame after it, the second: the third is the one used longest ago.
        let third = states.prepare(device, 3, 1, &[wanted(8, 8)]);
        assert_eq!(third.continuation, Continuation::GoOn);
        states.commit(3, 1, third.continuation);
        states.end_frame_within(u64::MAX);
        let second = states.prepare(device, 2, 1, &[wanted(8, 8)]);
        states.commit(2, 1, second.continuation);
        states.end_frame_within(u64::MAX);
        states.end_frame_within(1024);
        assert_eq!(states.len(), 1);
        assert_eq!(
            states.prepare(device, 2, 2, &[wanted(8, 8)]).continuation,
            Continuation::GoOn,
            "the second was used last"
        );
    });
}

#[test]
fn a_state_that_alone_is_over_the_budget_is_let_go_of_too() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        let call = states.prepare(device, 1, 0, &[wanted(8, 8)]);
        states.commit(1, 0, call.continuation);
        states.end_frame_within(10);
        assert_eq!(states.len(), 0);
    });
}
