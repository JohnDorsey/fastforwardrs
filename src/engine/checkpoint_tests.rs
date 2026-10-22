// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The checkpoints of the states of effects and the picture that says it is not exact (EN-14, option 3; the design is `docs/engine-phase-6-6-design.md`, section 4), made on the software
//! adapter: a shader that adds a tenth of red to what it kept in every frame is a picture of how many frames it has been through, so what a jump goes on from can be read in the colour.

use std::time::Duration;

use super::effect_tests::{render, whole_frame};
use super::oracle_tests::TestPicture;
use super::persistent_buffer_tests::{
    ADD_A_TENTH_OF_RED, THE_PICTURE_AND_A_BUFFER, near, one_buffer_that_lasts, stateful, tenths,
};
use super::testing::with_software_graphics;
use super::{
    DEFAULT_MOST_PASSES_OF_A_LOOP, EffectCall, FrameOutput, FrameRenderer, PictureId, RulesOfCheckpoints,
    foreign_effect_tests::guarded,
};

const A_LONG_TIME: Duration = Duration::from_secs(60);

fn renderer_that_takes_checkpoints_every(graphics: &super::testing::SoftwareGraphics, interval: u64) -> FrameRenderer {
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
        interval_in_frames: interval,
        memory_budget_in_bytes: u64::MAX,
    });
    renderer
}

/// The red of the middle of the frame made at `frame` of the effect, and whether the picture says it is not exact.
fn red_and_whether_it_is_approximate(renderer: &mut FrameRenderer, frame: u64) -> (i32, bool) {
    let (picture, statistics) = render(
        renderer,
        (8, 8),
        vec![whole_frame(1, vec![stateful("test:accumulate", 1, frame)])],
    );
    (i32::from(picture.pixel(4, 4)[0]), statistics.approximate)
}

fn play(renderer: &mut FrameRenderer, frames: impl IntoIterator<Item = u64>) {
    for frame in frames {
        let (red, approximate) = red_and_whether_it_is_approximate(renderer, frame);
        assert!(near(red, tenths(frame as i32 + 1)), "frame {frame}: {red}");
        assert!(!approximate, "frame {frame} was made in order from the first");
    }
}

#[test]
fn a_jump_back_goes_on_from_the_checkpoint_before_the_frame_and_the_picture_says_it_is_not_exact() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_that_takes_checkpoints_every(&graphics, 3);
        play(&mut renderer, 0..8);
        // Frame 5, after frame 7: the checkpoint is of frame 3 (4 tenths), the frame adds one, and the frame before it should have had 5.
        let (red, approximate) = red_and_whether_it_is_approximate(&mut renderer, 5);
        assert!(
            near(red, tenths(5)),
            "{red}: the checkpoint of frame 3 and one more tenth, and not the 6 tenths that frame 5 is"
        );
        assert!(approximate);
        // And the frames after it go on from there, and say so.
        let (red, approximate) = red_and_whether_it_is_approximate(&mut renderer, 6);
        assert!(near(red, tenths(6)));
        assert!(approximate, "it goes on being said, for 300 frames");
    });
}

#[test]
fn a_jump_to_the_frame_after_a_checkpoint_is_exact() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_that_takes_checkpoints_every(&graphics, 3);
        play(&mut renderer, 0..8);
        let (red, approximate) = red_and_whether_it_is_approximate(&mut renderer, 4);
        assert!(near(red, tenths(5)), "{red}: exactly what frame 4 is");
        assert!(!approximate);
        // Frame 6 after frame 4 skips frame 5: the checkpoint of frame 3 is older than the state there is (of frame 4), so it goes on from that, a tenth short.
        let (red, approximate) = red_and_whether_it_is_approximate(&mut renderer, 6);
        assert!(
            near(red, tenths(6)),
            "{red}: the state of frame 4 and one tenth, where exactly frame 6 is 7 tenths"
        );
        assert!(approximate, "and says so");
    });
}

#[test]
fn a_jump_to_the_first_frame_of_the_effect_starts_clear_and_is_exact() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_that_takes_checkpoints_every(&graphics, 3);
        play(&mut renderer, 0..8);
        let (red, approximate) = red_and_whether_it_is_approximate(&mut renderer, 0);
        assert!(near(red, tenths(1)));
        assert!(!approximate);
    });
}

#[test]
fn a_skipped_frame_goes_on_from_the_state_there_is_and_says_it_is_not_exact() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_that_takes_checkpoints_every(&graphics, 3);
        play(&mut renderer, 0..5);
        let (red, approximate) = red_and_whether_it_is_approximate(&mut renderer, 6);
        assert!(
            near(red, tenths(6)),
            "{red}: the state of frame 4 and one tenth, where it starts clear and has one tenth if no checkpoints are kept"
        );
        assert!(approximate);
    });
}

#[test]
fn with_no_rules_a_jump_starts_clear_as_it_always_did_and_no_picture_says_it_is_not_exact() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_that_takes_checkpoints_every(&graphics, 0);
        play(&mut renderer, 0..8);
        let (red, approximate) = red_and_whether_it_is_approximate(&mut renderer, 5);
        assert!(near(red, tenths(1)), "{red}");
        assert!(!approximate);
        assert_eq!(renderer.checkpoints_kept(), (0, 0));
    });
}

#[test]
fn forgetting_the_checkpoints_makes_a_jump_start_clear_and_say_it_is_not_exact() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_that_takes_checkpoints_every(&graphics, 3);
        play(&mut renderer, 0..8);
        assert_eq!(renderer.checkpoints_kept().0, 3);
        renderer.forget_the_checkpoints();
        assert_eq!(renderer.checkpoints_kept(), (0, 0));
        let (red, approximate) = red_and_whether_it_is_approximate(&mut renderer, 5);
        assert!(near(red, tenths(1)), "{red}");
        assert!(approximate, "it starts clear at a frame that is not the first");
    });
}

#[test]
fn the_export_takes_no_checkpoint_and_goes_on_from_nothing_but_the_frame_before() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_that_takes_checkpoints_every(&graphics, 1);
        for frame in 0..5 {
            let pending = renderer
                .begin_frame(
                    &{
                        let mut description = super::FrameDescription::black((8, 8));
                        description.layers = vec![whole_frame(1, vec![stateful("test:accumulate", 1, frame)])];
                        description
                    },
                    FrameOutput::Planes(super::YuvColour::UNSPECIFIED),
                    A_LONG_TIME,
                )
                .unwrap();
            let (_, statistics) = renderer.finish_frame(pending).unwrap();
            assert!(!statistics.approximate);
        }
        assert_eq!(
            renderer.checkpoints_kept(),
            (0, 0),
            "the export has no jumps to go back from"
        );
    });
}

#[test]
fn the_picture_of_a_frame_that_goes_on_from_a_checkpoint_is_what_the_state_it_was_copied_from_made() {
    // The checkpoint is a copy: what the effect did to the state after it was taken does not change it.
    with_software_graphics!(graphics, {
        // (The red of a frame is cut off at 255, which is 10 tenths: the frames stay within 9.)
        let mut renderer = renderer_that_takes_checkpoints_every(&graphics, 4);
        play(&mut renderer, 0..9);
        // (Checkpoints of frames 0, 4 and 8.) Frame 6 goes on from the checkpoint of frame 4 (5 tenths) and makes 6; frame 7 makes 7.
        let (red, _) = red_and_whether_it_is_approximate(&mut renderer, 6);
        assert!(near(red, tenths(6)), "{red}");
        let (red, _) = red_and_whether_it_is_approximate(&mut renderer, 7);
        assert!(near(red, tenths(7)), "{red}");
        // And a second jump to the same place goes on from the same checkpoint and makes the same picture: it was not changed by the first.
        let (red, _) = red_and_whether_it_is_approximate(&mut renderer, 6);
        assert!(near(red, tenths(6)), "{red}");
    });
}

#[test]
fn another_effect_call_is_not_given_the_checkpoints_of_this_one() {
    with_software_graphics!(graphics, {
        let mut renderer = renderer_that_takes_checkpoints_every(&graphics, 3);
        play(&mut renderer, 0..8);
        let other = |frame: u64| -> EffectCall { stateful("test:accumulate", 2, frame) };
        let (picture, statistics) = render(&mut renderer, (8, 8), vec![whole_frame(1, vec![other(5)])]);
        assert!(
            near(i32::from(picture.pixel(4, 4)[0]), tenths(1)),
            "a state of its own: it has none, and starts clear"
        );
        assert!(statistics.approximate);
    });
}
