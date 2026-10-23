// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The chase as the renderer makes it (EN-14, the lead-up while the project plays): frames that are made in a store of states of their own, which the preview's store takes over from when it has caught up
//! with it. Made on the software adapter, with the shader that adds a tenth of red to what it kept in every frame: the red of the picture is how many frames the state has been through.

use std::time::Duration;

use super::effect_states::CatchingUp;
use super::effect_tests::whole_frame;
use super::oracle_tests::TestPicture;
use super::persistent_buffer_tests::{
    ADD_A_TENTH_OF_RED, THE_PICTURE_AND_A_BUFFER, near, one_buffer_that_lasts, stateful, tenths,
};
use super::testing::with_software_graphics;
use super::{
    DEFAULT_MOST_PASSES_OF_A_LOOP, FrameDescription, FrameRenderer, FrameStatistics, PictureId, RenderedFrame,
    RulesOfCheckpoints, foreign_effect_tests::guarded,
};

const A_LONG_TIME: Duration = Duration::from_secs(60);

/// A renderer that has the accumulating effect and the picture 1, and takes checkpoints (so that it says when a picture is not exact) every 1000 frames of the effect.
fn renderer_with_the_accumulating_effect(graphics: &super::testing::SoftwareGraphics) -> FrameRenderer {
    let mut renderer = FrameRenderer::new(graphics.device.clone());
    renderer
        .upload_picture(PictureId(1), &TestPicture::gradient(8, 8).as_picture_data())
        .unwrap();
    renderer
        .install_effect_with_structure(
            "test:accumulate",
            &guarded(
                &format!("{THE_PICTURE_AND_A_BUFFER}\n{ADD_A_TENTH_OF_RED}"),
                DEFAULT_MOST_PASSES_OF_A_LOOP,
            ),
            &one_buffer_that_lasts(),
        )
        .unwrap();
    renderer.set_the_rules_of_checkpoints(RulesOfCheckpoints {
        interval_in_frames: 1000,
        memory_budget_in_bytes: u64::MAX,
    });
    renderer
}

fn frame_of_the_effect_at(frame: u64) -> FrameDescription {
    let mut description = FrameDescription::black((8, 8));
    description.layers = vec![whole_frame(1, vec![stateful("test:accumulate", 1, frame)])];
    description
}

fn red_in(frame: &RenderedFrame) -> i32 {
    i32::from(frame.pixel(4, 4)[0])
}

/// The frame of the preview at `frame` of the effect: its red, and whether it says it is not exact.
fn the_preview_makes(renderer: &mut FrameRenderer, frame: u64) -> (i32, bool) {
    let (picture, statistics) = renderer.render(&frame_of_the_effect_at(frame), A_LONG_TIME).unwrap();
    (red_in(&picture), statistics.approximate)
}

/// The frame of the chase at `frame` of the effect: its red.
fn the_chase_makes(renderer: &mut FrameRenderer, frame: u64) -> i32 {
    let (picture, _): (RenderedFrame, FrameStatistics) = renderer
        .render_for_a_chase(&frame_of_the_effect_at(frame), A_LONG_TIME)
        .unwrap();
    red_in(&picture)
}

#[test]
fn the_frames_of_a_chase_go_on_from_each_other_in_a_store_of_their_own_and_leave_the_preview_alone() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_with_the_accumulating_effect(&graphics);
        // The preview made frames 5 to 7 from nothing: 1, 2 and 3 tenths.
        for frame in 5..8 {
            let (red, approximate) = the_preview_makes(&mut renderer, frame);
            assert!(near(red, tenths(frame as i32 - 4)), "frame {frame}: {red}");
            assert!(
                approximate,
                "frame {frame} goes on from a state that started from nothing at frame 5"
            );
        }
        let (states_of_the_preview, bytes_of_the_preview) = renderer.effect_states_kept();
        assert_eq!((states_of_the_preview, renderer.states_of_a_chase_kept().0), (1, 0));

        // The chase goes through frames 0 to 4 in order: each has one tenth more than the one before.
        for frame in 0..5 {
            assert!(
                near(the_chase_makes(&mut renderer, frame), tenths(frame as i32 + 1)),
                "frame {frame}"
            );
        }
        assert_eq!(
            renderer.states_of_a_chase_kept().0,
            1,
            "the chase has a state of its own"
        );
        assert_eq!(
            renderer.effect_states_kept(),
            (states_of_the_preview, bytes_of_the_preview),
            "and the preview's is what it was"
        );
        // The preview goes on from where it was: frame 8 is its fourth tenth, and not the sixth that the chase would have made it.
        let (red, _) = the_preview_makes(&mut renderer, 8);
        assert!(near(red, tenths(4)), "{red}");
    });
}

#[test]
fn a_chase_that_has_caught_up_with_the_preview_hands_over_states_that_make_the_pictures_exact() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_with_the_accumulating_effect(&graphics);
        // The preview jumped to frame 5 and goes on: from nothing, so its frames are not exact and have too little red.
        for frame in 5..8 {
            the_preview_makes(&mut renderer, frame);
        }
        // The chase makes the frames from the first, and after each asks whether it has caught up.
        let mut asked = Vec::new();
        for frame in 0..8 {
            the_chase_makes(&mut renderer, frame);
            asked.push(renderer.take_over_what_the_chase_has_caught_up_with());
        }
        assert_eq!(
            asked,
            [
                CatchingUp::NotYet,
                CatchingUp::NotYet,
                CatchingUp::NotYet,
                CatchingUp::NotYet,
                CatchingUp::NotYet,
                CatchingUp::NotYet,
                CatchingUp::NotYet,
                CatchingUp::TakenOver(1)
            ],
            "it has caught up when it has made frame 7, which is where the preview is"
        );
        assert_eq!(renderer.states_of_a_chase_kept().0, 0, "the state went over");
        // The preview's next frame is frame 8, exactly: nine tenths, and it does not say that it is not exact.
        let (red, approximate) = the_preview_makes(&mut renderer, 8);
        assert!(near(red, tenths(9)), "{red}");
        assert!(!approximate);
    });
}

#[test]
fn the_frame_that_the_preview_shows_again_after_a_handover_is_exact_too() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_with_the_accumulating_effect(&graphics);
        for frame in 5..8 {
            the_preview_makes(&mut renderer, frame);
        }
        for frame in 0..8 {
            the_chase_makes(&mut renderer, frame);
        }
        assert_eq!(
            renderer.take_over_what_the_chase_has_caught_up_with(),
            CatchingUp::TakenOver(1)
        );
        // (A paused preview asks for its frame again, which is frame 7: its picture is what the state before it makes, and is exact now.)
        let (red, approximate) = the_preview_makes(&mut renderer, 7);
        assert!(near(red, tenths(8)), "{red}");
        assert!(!approximate);
    });
}

#[test]
fn a_chase_that_is_let_go_of_leaves_nothing_and_the_preview_is_what_it_was() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_with_the_accumulating_effect(&graphics);
        for frame in 5..8 {
            the_preview_makes(&mut renderer, frame);
        }
        let kept = renderer.effect_states_kept();
        for frame in 0..4 {
            the_chase_makes(&mut renderer, frame);
        }
        assert_eq!(renderer.states_of_a_chase_kept().0, 1);
        renderer.let_go_of_what_a_chase_made();
        assert_eq!(renderer.states_of_a_chase_kept(), (0, 0));
        assert_eq!(renderer.effect_states_kept(), kept);
        let (red, approximate) = the_preview_makes(&mut renderer, 8);
        assert!(near(red, tenths(4)), "{red}");
        assert!(
            approximate,
            "frame 8 goes on from the frame 5 that it started at, as before"
        );
    });
}

#[test]
fn a_frame_of_a_chase_that_the_card_does_not_finish_leaves_the_states_of_the_preview_in_their_place() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_with_the_accumulating_effect(&graphics);
        for frame in 5..8 {
            the_preview_makes(&mut renderer, frame);
        }
        let kept = renderer.effect_states_kept();
        renderer.pretend_the_card_never_finishes();
        // (A frame with two states of its own, so that the store it went through is told from the preview's, which has one.)
        let mut description = FrameDescription::black((8, 8));
        description.layers = vec![whole_frame(
            1,
            vec![stateful("test:accumulate", 2, 0), stateful("test:accumulate", 3, 0)],
        )];
        assert!(renderer.render_for_a_chase(&description, A_LONG_TIME).is_err());
        assert_eq!(
            renderer.states_of_a_chase_kept().0,
            2,
            "the frame went through the chase's store"
        );
        assert_eq!(
            renderer.effect_states_kept(),
            kept,
            "the preview's states are in the renderer again, and not in the chase's store"
        );
    });
}

#[test]
fn a_chase_with_nothing_made_is_over_when_the_preview_needs_no_state() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_with_the_accumulating_effect(&graphics);
        // The preview made a frame with no effect that has a state.
        renderer.render(&FrameDescription::black((8, 8)), A_LONG_TIME).unwrap();
        assert_eq!(
            renderer.take_over_what_the_chase_has_caught_up_with(),
            CatchingUp::TakenOver(0)
        );
    });
}
