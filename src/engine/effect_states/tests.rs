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

// --- checkpoints and the state that is approximate (EN-14, option 3) ---

#[test]
fn where_a_call_goes_on_from_is_the_nearest_state_that_is_not_after_the_frame_and_it_is_exact_only_if_it_is_the_frame_before()
 {
    use GoesOnFrom::*;
    // The same frame again is the same frame again, whatever else there is.
    assert_eq!(where_to_go_on_from(Some(5), &[0, 3], 5), TheSameFrameAgain);
    // The frame after the state it has: it goes on, exact.
    assert_eq!(where_to_go_on_from(Some(5), &[0, 3], 6), TheState { exact: true });
    // A frame that was skipped: it goes on from the state it has, which is not the frame before: not exact.
    assert_eq!(where_to_go_on_from(Some(5), &[0, 3], 8), TheState { exact: false });
    // A checkpoint that is nearer than the state it has is the one to go on from (here the frame is a step back, and the state it has is of a later frame).
    assert_eq!(
        where_to_go_on_from(Some(9), &[0, 3, 6], 5),
        ACheckpoint { frame: 3, exact: false }
    );
    assert_eq!(
        where_to_go_on_from(Some(9), &[0, 3, 6], 4),
        ACheckpoint { frame: 3, exact: true },
        "the checkpoint is of the frame before: nothing is lost"
    );
    assert_eq!(
        where_to_go_on_from(Some(9), &[0, 3, 6], 7),
        ACheckpoint { frame: 6, exact: true }
    );
    // A checkpoint of the frame itself, or after it, is no state before it.
    assert_eq!(
        where_to_go_on_from(None, &[0, 3, 6], 3),
        ACheckpoint { frame: 0, exact: false }
    );
    assert_eq!(
        where_to_go_on_from(None, &[3, 6], 2),
        Nothing { exact: false },
        "none is before it"
    );
    // When the state it has and a checkpoint are of the same frame, the state it has is gone on from.
    assert_eq!(where_to_go_on_from(Some(3), &[3], 5), TheState { exact: false });
    // The state before the first frame is clear, and that is what it is.
    assert_eq!(where_to_go_on_from(Some(40), &[0, 3], 0), Nothing { exact: true });
    assert_eq!(where_to_go_on_from(None, &[], 0), Nothing { exact: true });
    // Without a checkpoint or a state, a later frame starts clear and is not exact.
    assert_eq!(where_to_go_on_from(None, &[], 7), Nothing { exact: false });
    // No overflow.
    assert_eq!(
        where_to_go_on_from(Some(u64::MAX), &[u64::MAX], 0),
        Nothing { exact: true }
    );
}

fn checkpoints_every(interval: u64) -> RulesOfCheckpoints {
    RulesOfCheckpoints {
        interval_in_frames: interval,
        memory_budget_in_bytes: u64::MAX,
    }
}

/// Makes the frames `frames` of the state 1 (one buffer, 4 by 4) as the preview does, and commits each.
fn play(states: &mut StatesOfEffects, device: &wgpu::Device, frames: impl IntoIterator<Item = u64>) {
    for frame in frames {
        let call = states.prepare_for_the_preview(device, 1, frame, &[wanted(4, 4)]);
        states.commit(1, frame, call.continuation);
        states.end_frame();
    }
}

#[test]
fn a_checkpoint_is_taken_after_each_frame_whose_number_is_a_multiple_of_the_interval_and_not_after_the_others() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        states.set_the_rules_of_checkpoints(checkpoints_every(3));
        let mut taken = Vec::new();
        for frame in 0..8 {
            let call = states.prepare_for_the_preview(device, 1, frame, &[wanted(4, 4)]);
            taken.push(call.to_checkpoint.len());
            states.commit(1, frame, call.continuation);
            states.end_frame();
        }
        assert_eq!(
            taken,
            [1, 0, 0, 1, 0, 0, 1, 0],
            "one buffer, a copy of it at frames 0, 3 and 6"
        );
        // Each is a texture of its own, 4 by 4 pixels, half floats: 128 bytes.
        assert_eq!(states.checkpoints_kept(), (3, 3 * 128));
        // The state itself is what it was: two textures.
        assert_eq!(states.bytes(), 2 * 128);
    });
}

#[test]
fn a_frame_that_is_not_the_preview_takes_no_checkpoint_and_restores_none() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        states.set_the_rules_of_checkpoints(checkpoints_every(1));
        for frame in 0..4 {
            let call = states.prepare(device, 1, frame, &[wanted(4, 4)]);
            assert!(call.to_checkpoint.is_empty() && call.to_restore.is_empty() && !call.approximate);
            states.commit(1, frame, call.continuation);
            states.end_frame();
        }
        assert_eq!(states.checkpoints_kept(), (0, 0));
        let jump = states.prepare(device, 1, 9, &[wanted(4, 4)]);
        assert_eq!(
            jump.continuation,
            Continuation::StartClear,
            "the export starts clear, as it always did"
        );
        assert_eq!(jump.to_clear.len(), 1);
        assert!(jump.to_restore.is_empty() && !jump.approximate);
    });
}

#[test]
fn with_no_rules_the_preview_goes_on_as_it_always_did_and_takes_nothing() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        play(&mut states, device, 0..4);
        let skip = states.prepare_for_the_preview(device, 1, 6, &[wanted(4, 4)]);
        assert_eq!(skip.continuation, Continuation::StartClear);
        assert!(skip.to_restore.is_empty() && skip.to_checkpoint.is_empty() && !skip.approximate);
        assert_eq!(states.checkpoints_kept(), (0, 0));
    });
}

#[test]
fn a_jump_goes_on_from_the_nearest_checkpoint_before_the_frame_and_says_it_is_not_exact_unless_that_is_the_frame_before()
 {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        states.set_the_rules_of_checkpoints(checkpoints_every(3));
        play(&mut states, device, 0..8);
        // A step back to frame 5: the state it has is of frame 7; the nearest checkpoint before it is of frame 3.
        let back = states.prepare_for_the_preview(device, 1, 5, &[wanted(4, 4)]);
        assert_eq!(
            back.continuation,
            Continuation::StartClear,
            "it reads the state before the frame, which is restored"
        );
        assert_eq!(back.to_restore.len(), 1);
        assert!(
            back.to_clear.is_empty(),
            "nothing is cleared: the checkpoint is copied over it"
        );
        assert!(
            back.approximate,
            "frame 4 is what the state should be of, and it is of frame 3"
        );
        states.abandon(1);
        // Frame 4: the checkpoint is of the frame before: exact.
        play(&mut states, device, 0..8);
        let exact = states.prepare_for_the_preview(device, 1, 4, &[wanted(4, 4)]);
        assert_eq!(exact.to_restore.len(), 1);
        assert!(!exact.approximate);
        states.abandon(1);
        // Frame 0 again: the state before it is clear, and that is exact.
        play(&mut states, device, 0..8);
        let first = states.prepare_for_the_preview(device, 1, 0, &[wanted(4, 4)]);
        assert_eq!(first.to_clear.len(), 1);
        assert!(first.to_restore.is_empty() && !first.approximate);
    });
}

#[test]
fn a_frame_that_is_skipped_goes_on_from_the_state_there_is_and_not_from_an_older_checkpoint_and_says_it_is_not_exact() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        states.set_the_rules_of_checkpoints(checkpoints_every(3));
        play(&mut states, device, 0..5);
        // The state is of frame 4; frame 6 skips frame 5.
        let skip = states.prepare_for_the_preview(device, 1, 6, &[wanted(4, 4)]);
        assert_eq!(skip.continuation, Continuation::GoOn);
        assert!(skip.to_restore.is_empty() && skip.to_clear.is_empty());
        assert!(skip.approximate);
    });
}

#[test]
fn a_state_made_from_one_that_was_not_exact_stays_not_exact_for_three_hundred_frames_and_then_is_exact_again() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        states.set_the_rules_of_checkpoints(checkpoints_every(1000));
        play(&mut states, device, 0..2);
        // A jump with nothing to go on from but the state it has: not exact from frame 20, for 300 frames.
        let mut said = Vec::new();
        for frame in 20..=(20 + FRAMES_AFTER_A_STATE_IS_NOT_EXACT + 1) {
            let call = states.prepare_for_the_preview(device, 1, frame, &[wanted(4, 4)]);
            said.push(call.approximate);
            states.commit(1, frame, call.continuation);
            states.end_frame();
        }
        assert!(
            said[..300].iter().all(|approximate| *approximate),
            "the first 300 frames after the jump"
        );
        assert!(!said[300] && !said[301], "and then it is exact: {:?}", &said[298..]);
    });
}

#[test]
fn a_checkpoint_taken_from_a_state_that_was_not_exact_is_not_exact_either() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        states.set_the_rules_of_checkpoints(checkpoints_every(10));
        // Frames 0 to 3, then a jump to 12 (not exact), and frame 13, and the checkpoint at 10 would have been taken at frame 10: it is the one at 20 that is, from a state that is not exact.
        play(&mut states, device, 0..4);
        play(&mut states, device, 12..24);
        // A jump back to frame 22 restores the checkpoint of frame 20, which was made from a state that was not exact.
        let back = states.prepare_for_the_preview(device, 1, 21, &[wanted(4, 4)]);
        assert_eq!(back.to_restore.len(), 1);
        assert!(
            back.approximate,
            "the checkpoint is of the frame before, and it is not exact"
        );
    });
}

#[test]
fn the_checkpoints_that_are_forgotten_are_not_gone_on_from() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        states.set_the_rules_of_checkpoints(checkpoints_every(3));
        play(&mut states, device, 0..8);
        assert_eq!(states.checkpoints_kept().0, 3);
        states.forget_the_checkpoints();
        assert_eq!(states.checkpoints_kept(), (0, 0));
        let back = states.prepare_for_the_preview(device, 1, 5, &[wanted(4, 4)]);
        assert_eq!(back.continuation, Continuation::StartClear);
        assert!(back.to_restore.is_empty());
        assert!(back.approximate, "it starts clear at a frame that is not the first");
    });
}

#[test]
fn the_checkpoints_are_kept_within_the_memory_that_they_may_take_and_the_one_used_longest_ago_goes_first() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        // A checkpoint of one buffer of 4 by 4 is 128 bytes: room for two.
        states.set_the_rules_of_checkpoints(RulesOfCheckpoints {
            interval_in_frames: 3,
            memory_budget_in_bytes: 2 * 128,
        });
        play(&mut states, device, 0..8);
        assert_eq!(
            states.checkpoints_kept(),
            (2, 2 * 128),
            "the checkpoints of frames 3 and 6 are kept, and that of frame 0 is gone"
        );
        // Frame 2 has no checkpoint before it any more (the only one before it was of frame 0): the state it has is of a later frame, so it starts clear.
        let early = states.prepare_for_the_preview(device, 1, 2, &[wanted(4, 4)]);
        assert!(early.to_restore.is_empty());
        assert_eq!(early.continuation, Continuation::StartClear);
        states.abandon(1);
        play(&mut states, device, 0..8);
        // Frame 5 is after the checkpoint of frame 3, which is there.
        let later = states.prepare_for_the_preview(device, 1, 5, &[wanted(4, 4)]);
        assert_eq!(later.to_restore.len(), 1);
    });
}

#[test]
fn a_checkpoint_made_for_buffers_of_another_size_is_not_used() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        states.set_the_rules_of_checkpoints(checkpoints_every(3));
        play(&mut states, device, 0..8);
        let bigger = states.prepare_for_the_preview(device, 1, 5, &[wanted(8, 8)]);
        assert!(bigger.to_restore.is_empty(), "the checkpoints are of buffers of 4 by 4");
        assert_eq!(bigger.continuation, Continuation::StartClear);
        assert!(bigger.approximate);
    });
}

#[test]
fn a_frame_that_is_abandoned_leaves_no_checkpoint_and_the_checkpoints_of_the_frames_before_it_stay() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        states.set_the_rules_of_checkpoints(checkpoints_every(3));
        play(&mut states, device, 0..3);
        assert_eq!(states.checkpoints_kept().0, 1);
        let failed = states.prepare_for_the_preview(device, 1, 3, &[wanted(4, 4)]);
        assert_eq!(failed.to_checkpoint.len(), 1);
        states.abandon(1);
        assert_eq!(
            states.checkpoints_kept().0,
            1,
            "the checkpoint of frame 3 was never made"
        );
        assert_eq!(states.len(), 0);
    });
}

#[test]
fn rules_that_take_none_let_go_of_those_there_are() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        states.set_the_rules_of_checkpoints(checkpoints_every(3));
        play(&mut states, device, 0..8);
        states.set_the_rules_of_checkpoints(RulesOfCheckpoints::NONE);
        assert_eq!(states.checkpoints_kept(), (0, 0));
    });
}

#[test]
fn a_frame_made_again_takes_no_second_checkpoint_of_the_frame_that_has_one() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        states.set_the_rules_of_checkpoints(checkpoints_every(3));
        play(&mut states, device, 0..4);
        assert_eq!(states.checkpoints_kept().0, 2, "frames 0 and 3");
        let again = states.prepare_for_the_preview(device, 1, 3, &[wanted(4, 4)]);
        assert_eq!(again.continuation, Continuation::SameFrame);
        assert!(again.to_checkpoint.is_empty(), "frame 3 has its checkpoint");
    });
}

#[test]
fn a_checkpoint_that_is_gone_on_from_is_used_again_and_the_one_that_is_not_goes_first() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        // Room for two checkpoints of 128 bytes.
        states.set_the_rules_of_checkpoints(RulesOfCheckpoints {
            interval_in_frames: 3,
            memory_budget_in_bytes: 2 * 128,
        });
        play(&mut states, device, 0..4);
        assert_eq!(states.checkpoints_kept().0, 2, "frames 0 and 3");
        // A jump back to frame 1 goes on from the checkpoint of frame 0, which is used now, and later than the checkpoint of frame 3 was made.
        let back = states.prepare_for_the_preview(device, 1, 1, &[wanted(4, 4)]);
        assert_eq!(back.to_restore.len(), 1);
        states.commit(1, 1, back.continuation);
        states.end_frame();
        // Going on to frame 6 takes a third checkpoint, and the one used longest ago goes: that of frame 3, and not that of frame 0.
        play(&mut states, device, 2..=6);
        assert_eq!(states.checkpoints_kept().0, 2);
        let jump = states.prepare_for_the_preview(device, 1, 2, &[wanted(4, 4)]);
        assert_eq!(
            jump.to_restore.len(),
            1,
            "the checkpoint of frame 0 is still there, for it was used"
        );
    });
}

#[test]
fn buffers_that_change_size_are_a_start_from_nothing_at_a_frame_after_the_one_the_state_was_made_at_and_say_it_is_not_exact()
 {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        states.set_the_rules_of_checkpoints(checkpoints_every(30));
        play(&mut states, device, 0..4);
        // The next frame, with buffers that are another size: the state it had is gone, and the new one is clear. It is not the state of the frame before, and the call says so (and does
        // not go on from what is not there).
        let bigger = states.prepare_for_the_preview(device, 1, 4, &[wanted(8, 8)]);
        assert_eq!(bigger.continuation, Continuation::StartClear);
        assert_eq!(bigger.to_clear.len(), 1);
        assert!(bigger.approximate);
    });
}

#[test]
fn rules_with_no_interval_take_no_checkpoint_and_let_go_of_those_there_are_whatever_the_memory_they_may_take() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut states = StatesOfEffects::default();
        states.set_the_rules_of_checkpoints(checkpoints_every(3));
        play(&mut states, device, 0..8);
        assert_eq!(states.checkpoints_kept().0, 3);
        states.set_the_rules_of_checkpoints(RulesOfCheckpoints {
            interval_in_frames: 0,
            memory_budget_in_bytes: u64::MAX,
        });
        assert_eq!(states.checkpoints_kept(), (0, 0));
        let call = states.prepare_for_the_preview(device, 1, 8, &[wanted(4, 4)]);
        assert!(call.to_checkpoint.is_empty());
    });
}

// ---- The chase: states made in a store of their own are taken over by the store of the preview when they have caught up with it (EN-14, the lead-up while the project plays).

/// Makes the frames `frames` of the state `id` (one buffer of `size`) as a frame that is not of the preview does, and commits each.
fn go_through(
    states: &mut StatesOfEffects,
    device: &wgpu::Device,
    id: u64,
    frames: impl IntoIterator<Item = u64>,
    size: (u32, u32),
) {
    for frame in frames {
        let call = states.prepare_for_the_preview(device, id, frame, &[wanted(size.0, size.1)]);
        states.commit(id, frame, call.continuation);
        states.end_frame();
    }
}

/// A frame of the preview that uses all of the states `ids` at `frame`, so that they are what the last frame used.
fn a_frame_of_the_preview_using(states: &mut StatesOfEffects, device: &wgpu::Device, ids: &[u64], frame: u64) {
    for id in ids {
        let call = states.prepare_for_the_preview(device, *id, frame, &[wanted(4, 4)]);
        states.commit(*id, frame, call.continuation);
    }
    states.end_frame();
}

#[test]
fn a_store_that_has_not_reached_the_frame_of_the_preview_is_not_taken_over_and_one_that_has_is() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut live = StatesOfEffects::default();
        live.set_the_rules_of_checkpoints(checkpoints_every(1000));
        // The preview jumped to frame 5 and went on to 7: from nothing, so it is not exact.
        go_through(&mut live, device, 1, 5..=7, (4, 4));
        let next = live.prepare_for_the_preview(device, 1, 8, &[wanted(4, 4)]);
        assert!(next.approximate, "a state that started at frame 5 is not exact");
        live.abandon(1);
        go_through(&mut live, device, 1, 5..=7, (4, 4));

        let mut chasing = StatesOfEffects::default();
        go_through(&mut chasing, device, 1, 0..=6, (4, 4));
        assert_eq!(live.take_over_what_has_caught_up(&mut chasing), CatchingUp::NotYet);
        assert_eq!(chasing.len(), 1, "nothing was taken");

        go_through(&mut chasing, device, 1, 7..=7, (4, 4));
        assert_eq!(
            live.take_over_what_has_caught_up(&mut chasing),
            CatchingUp::TakenOver(1)
        );
        assert_eq!(chasing.len(), 0, "it went over to the other store");
        assert_eq!(live.len(), 1);
        // The state is the one of frame 7 and exact: the next frame goes on from it and says so.
        let next = live.prepare_for_the_preview(device, 1, 8, &[wanted(4, 4)]);
        assert_eq!(next.continuation, Continuation::GoOn);
        assert!(!next.approximate, "what the chase made is exact");
        // And the same frame again, which is what a picture that is asked for again is, reads the state before it.
        live.commit(1, 8, next.continuation);
        let again = live.prepare_for_the_preview(device, 1, 8, &[wanted(4, 4)]);
        assert_eq!(again.continuation, Continuation::SameFrame);
        assert!(!again.approximate);
    });
}

#[test]
fn a_state_that_is_taken_over_replaces_the_one_that_the_preview_had_at_the_same_frame() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut live = StatesOfEffects::default();
        live.set_the_rules_of_checkpoints(checkpoints_every(1000));
        go_through(&mut live, device, 1, 5..=7, (4, 4));
        let mut chasing = StatesOfEffects::default();
        go_through(&mut chasing, device, 1, 0..=7, (4, 4));
        assert_eq!(
            live.take_over_what_has_caught_up(&mut chasing),
            CatchingUp::TakenOver(1)
        );
        // Two textures for the buffer, as a state always has, and not four: the one that the preview had is let go of.
        assert_eq!(live.bytes(), 2 * 128);
        assert_eq!(live.len(), 1);
    });
}

#[test]
fn only_the_states_that_the_last_frame_of_the_preview_used_are_needed_and_taken() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut live = StatesOfEffects::default();
        // State 2 was used by a frame before the last, and the last frame used state 1 only.
        a_frame_of_the_preview_using(&mut live, device, &[2], 6);
        a_frame_of_the_preview_using(&mut live, device, &[1], 7);
        let mut chasing = StatesOfEffects::default();
        go_through(&mut chasing, device, 1, 0..=7, (4, 4));
        go_through(&mut chasing, device, 2, 0..=3, (4, 4));
        assert_eq!(
            live.take_over_what_has_caught_up(&mut chasing),
            CatchingUp::TakenOver(1)
        );
        assert_eq!(chasing.len(), 1, "state 2, which nothing needed, is where it was");
        assert_eq!(live.len(), 2);
        // The preview's own state 2 (at frame 6) was not touched: frame 7 goes on from it.
        let two = live.prepare_for_the_preview(device, 2, 7, &[wanted(4, 4)]);
        assert_eq!(two.continuation, Continuation::GoOn);
    });
}

#[test]
fn the_states_are_taken_all_together_or_not_at_all() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut live = StatesOfEffects::default();
        a_frame_of_the_preview_using(&mut live, device, &[1, 2], 4);
        let mut chasing = StatesOfEffects::default();
        go_through(&mut chasing, device, 1, 0..=4, (4, 4));
        go_through(&mut chasing, device, 2, 0..=3, (4, 4));
        assert_eq!(live.take_over_what_has_caught_up(&mut chasing), CatchingUp::NotYet);
        assert_eq!(
            chasing.len(),
            2,
            "state 1 is at the frame but state 2 is not, so neither went"
        );
        go_through(&mut chasing, device, 2, 4..=4, (4, 4));
        assert_eq!(
            live.take_over_what_has_caught_up(&mut chasing),
            CatchingUp::TakenOver(2)
        );
        assert_eq!(chasing.len(), 0);
    });
}

#[test]
fn a_state_that_the_chase_has_not_made_at_all_is_not_yet() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut live = StatesOfEffects::default();
        a_frame_of_the_preview_using(&mut live, device, &[1, 2], 4);
        let mut chasing = StatesOfEffects::default();
        go_through(&mut chasing, device, 1, 0..=4, (4, 4));
        assert_eq!(live.take_over_what_has_caught_up(&mut chasing), CatchingUp::NotYet);
        assert_eq!(chasing.len(), 1);
    });
}

#[test]
fn a_state_that_is_at_a_later_frame_than_the_preview_never_catches_up() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut live = StatesOfEffects::default();
        a_frame_of_the_preview_using(&mut live, device, &[1], 4);
        let mut chasing = StatesOfEffects::default();
        go_through(&mut chasing, device, 1, 0..=5, (4, 4));
        let CatchingUp::NeverWill(why) = live.take_over_what_has_caught_up(&mut chasing) else {
            panic!("a state that is ahead of the preview cannot be taken over");
        };
        assert!(why.contains("later"), "{why}");
        assert_eq!(chasing.len(), 1, "nothing was taken");
    });
}

#[test]
fn a_later_state_is_never_even_when_another_is_not_yet() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut live = StatesOfEffects::default();
        a_frame_of_the_preview_using(&mut live, device, &[1, 2], 4);
        let mut chasing = StatesOfEffects::default();
        go_through(&mut chasing, device, 1, 0..=2, (4, 4));
        go_through(&mut chasing, device, 2, 0..=5, (4, 4));
        assert!(matches!(
            live.take_over_what_has_caught_up(&mut chasing),
            CatchingUp::NeverWill(_)
        ));
    });
}

#[test]
fn a_state_whose_buffers_are_not_the_ones_of_the_preview_never_catches_up() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut live = StatesOfEffects::default();
        a_frame_of_the_preview_using(&mut live, device, &[1], 4);
        let mut chasing = StatesOfEffects::default();
        // (At an earlier frame already: waiting would be for nothing.)
        go_through(&mut chasing, device, 1, 0..=2, (8, 8));
        let CatchingUp::NeverWill(why) = live.take_over_what_has_caught_up(&mut chasing) else {
            panic!("states of another size cannot be taken over");
        };
        assert!(why.contains("buffers"), "{why}");
        assert_eq!(chasing.len(), 1);
        assert_eq!(live.len(), 1);
    });
}

#[test]
fn when_the_last_frame_used_no_state_there_is_nothing_to_take_and_the_chase_is_over() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut live = StatesOfEffects::default();
        a_frame_of_the_preview_using(&mut live, device, &[1], 3);
        live.end_frame();
        let mut chasing = StatesOfEffects::default();
        go_through(&mut chasing, device, 1, 0..=2, (4, 4));
        assert_eq!(
            live.take_over_what_has_caught_up(&mut chasing),
            CatchingUp::TakenOver(0)
        );
        assert_eq!(chasing.len(), 1, "nothing was taken");
        // (A preview that has made no frame at all needs nothing either.)
        assert_eq!(
            StatesOfEffects::default().take_over_what_has_caught_up(&mut chasing),
            CatchingUp::TakenOver(0)
        );
    });
}

#[test]
fn a_state_that_is_taken_over_counts_as_used_just_now_by_the_store_that_has_it() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut live = StatesOfEffects::default();
        for _ in 0..(2 * FRAMES_BEFORE_A_STATE_IS_LET_GO_OF) {
            live.end_frame();
        }
        a_frame_of_the_preview_using(&mut live, device, &[1], 700);
        let mut chasing = StatesOfEffects::default();
        go_through(&mut chasing, device, 1, 699..=700, (4, 4));
        assert_eq!(
            live.take_over_what_has_caught_up(&mut chasing),
            CatchingUp::TakenOver(1)
        );
        live.end_frame();
        assert_eq!(
            live.len(),
            1,
            "it was last used at the chase's own count of frames, 2, which is long ago at the preview's, and it is not let go of for that"
        );
    });
}

#[test]
fn a_state_with_another_number_of_buffers_than_the_preview_s_never_catches_up_either() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let mut live = StatesOfEffects::default();
        a_frame_of_the_preview_using(&mut live, device, &[1], 4);
        let mut chasing = StatesOfEffects::default();
        // (The same first buffer and one more: the shader of the effect was another when the chase was made.)
        for frame in 0..=4 {
            let call = chasing.prepare_for_the_preview(device, 1, frame, &[wanted(4, 4), wanted(4, 4)]);
            chasing.commit(1, frame, call.continuation);
            chasing.end_frame();
        }
        let CatchingUp::NeverWill(why) = live.take_over_what_has_caught_up(&mut chasing) else {
            panic!("a state with more buffers is not the state of an effect with fewer");
        };
        assert!(why.contains("buffers"), "{why}");
        assert_eq!(chasing.len(), 1);
    });
}

#[test]
fn what_is_taken_over_is_exact_whatever_the_store_that_made_it_said_of_it() {
    with_software_graphics!(graphics, {
        let device = &graphics.device.device;
        let a_chase_that_started_from_nothing = |device: &wgpu::Device| {
            let mut chasing = StatesOfEffects::default();
            // (A store that is given rules says whether what it makes is exact; the chase's own never is.)
            chasing.set_the_rules_of_checkpoints(checkpoints_every(1000));
            go_through(&mut chasing, device, 1, 5..=7, (4, 4));
            chasing
        };
        // A premise: what that store made is not exact.
        let mut probe = a_chase_that_started_from_nothing(device);
        assert!(probe.prepare_for_the_preview(device, 1, 8, &[wanted(4, 4)]).approximate);

        let mut live = StatesOfEffects::default();
        a_frame_of_the_preview_using(&mut live, device, &[1], 7);
        let mut chasing = a_chase_that_started_from_nothing(device);
        assert_eq!(
            live.take_over_what_has_caught_up(&mut chasing),
            CatchingUp::TakenOver(1)
        );
        live.set_the_rules_of_checkpoints(checkpoints_every(1000));
        let next = live.prepare_for_the_preview(device, 1, 8, &[wanted(4, 4)]);
        assert_eq!(next.continuation, Continuation::GoOn);
        assert!(
            !next.approximate,
            "a state that went over is the state of the frame before, and exact"
        );
    });
}
