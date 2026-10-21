// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Groups in the engine (phase 5, piece 5.1; the design is `docs/engine-phase-5-design.md`, H1 and H2): a layer that is a group is drawn from the composite of the layers
//! in it, which is made in a picture of its own the size of the frame, **and the effects of the layer are done to that picture**.
//!
//! The oracle is the flat description: a group that has no effects, drawn over black, is the frame that its layers make flat (to the rounding of half floats); a group that has an
//! effect is the effect done to the picture that the flat frame is (an opaque picture, sent back to the engine), and **it is not what the effect done to each layer is**, which
//! is what the placeholder did.

use super::effect_tests::{frame_of, matrix_effect, noisy, render, whole_frame};
use super::oracle_tests::TestPicture;
use super::plan::{EffectsThatRun, PassPlan, plan_frame};
use super::testing::with_software_graphics;
use super::{
    CORNERS_OF_THE_WHOLE_PICTURE, EffectCall, FrameDescription, FrameRenderer, GroupDescription, LayerDescription,
    LayerSource, RenderedFrame,
};
use crate::colour_effects::ColourMatrix;

const SIZE: (u32, u32) = (64, 48);

/// A layer that is the group of `layers`, over the whole frame, with `effects` done to the group's picture.
fn a_group(layers: Vec<LayerDescription>, effects: Vec<EffectCall>) -> LayerDescription {
    LayerDescription {
        source: LayerSource::Group(GroupDescription { layers }),
        corners: CORNERS_OF_THE_WHOLE_PICTURE,
        effects,
    }
}

fn the_largest_difference(a: &RenderedFrame, b: &RenderedFrame) -> u8 {
    assert_eq!(a.rgba_pixels.len(), b.rgba_pixels.len());
    a.rgba_pixels
        .iter()
        .zip(&b.rgba_pixels)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0)
}

/// The three pictures that the tests are made of: an opaque one that covers the frame, and two that are partly see-through.
fn the_pictures() -> [TestPicture; 3] {
    [
        TestPicture::gradient(SIZE.0, SIZE.1),
        noisy(SIZE.0, SIZE.1, 7, true),
        noisy(SIZE.0, SIZE.1, 11, true),
    ]
}

fn send_all(renderer: &mut FrameRenderer, pictures: &[TestPicture; 3]) {
    for (number, picture) in pictures.iter().enumerate() {
        renderer
            .upload_picture(super::PictureId(number as u64 + 1), &picture.as_picture_data())
            .unwrap();
    }
}

/// A strong change of colours that cuts off at both ends, so that it is not the same done before the layers are put over one another and after.
fn a_matrix_that_cuts_off() -> ColourMatrix {
    ColourMatrix::from_rows([[1.5, 0.5, 0.0], [-0.5, 1.2, 0.3], [0.2, -0.4, 1.8]])
}

// --- the plan (no graphics card) ---

fn the_plan_of(description: &FrameDescription) -> Vec<PassPlan> {
    plan_frame(description, &EffectsThatRun::built_in_only()).passes
}

#[test]
fn the_groups_are_drawn_before_the_layers_they_are_made_of_the_last_group_first_and_the_frames_own_layers_last() {
    let mut description = FrameDescription::black(SIZE);
    description.layers = vec![
        whole_frame(1, Vec::new()),
        a_group(
            vec![
                whole_frame(2, vec![matrix_effect(a_matrix_that_cuts_off())]),
                a_group(vec![whole_frame(3, Vec::new())], Vec::new()),
            ],
            vec![matrix_effect(ColourMatrix::hue_rotation(90.0))],
        ),
    ];

    // The frame has layers 0 (picture 1) and 1 (the first group); the first group is container 1 and has layers 2 (picture 2) and 3 (the second group); the second group is
    // container 2 and has layer 4 (picture 3).
    assert_eq!(
        the_plan_of(&description),
        vec![
            PassPlan::PlaceGroup {
                container: 2,
                layers: vec![4]
            },
            PassPlan::Effects {
                layer: 2,
                calls: vec![0]
            },
            PassPlan::PlaceGroup {
                container: 1,
                layers: vec![2, 3]
            },
            PassPlan::Effects {
                layer: 1,
                calls: vec![0]
            },
            PassPlan::PlaceLayers { layers: vec![0, 1] },
            PassPlan::Output,
        ]
    );
}

#[test]
fn a_description_without_groups_has_the_plan_it_had_before_there_were_groups() {
    let mut description = FrameDescription::black(SIZE);
    description.layers = vec![
        whole_frame(1, vec![matrix_effect(a_matrix_that_cuts_off())]),
        whole_frame(2, Vec::new()),
    ];
    assert_eq!(
        the_plan_of(&description),
        vec![
            PassPlan::Effects {
                layer: 0,
                calls: vec![0]
            },
            PassPlan::PlaceLayers { layers: vec![0, 1] },
            PassPlan::Output,
        ]
    );
    assert_eq!(
        the_plan_of(&FrameDescription::black(SIZE)),
        vec![PassPlan::ClearAccumulation, PassPlan::Output]
    );
}

#[test]
fn a_group_with_no_layers_is_a_picture_of_nothing_and_is_drawn_all_the_same() {
    let mut description = FrameDescription::black(SIZE);
    description.layers = vec![a_group(Vec::new(), Vec::new())];
    assert_eq!(
        the_plan_of(&description),
        vec![
            PassPlan::PlaceGroup {
                container: 1,
                layers: vec![]
            },
            PassPlan::PlaceLayers { layers: vec![0] },
            PassPlan::Output,
        ]
    );
}

#[test]
fn every_layer_of_every_group_is_in_the_flat_list_in_the_order_the_plan_numbers_them() {
    let mut description = FrameDescription::black(SIZE);
    description.layers = vec![
        a_group(vec![whole_frame(5, Vec::new()), whole_frame(6, Vec::new())], Vec::new()),
        whole_frame(1, Vec::new()),
        a_group(vec![a_group(vec![whole_frame(9, Vec::new())], Vec::new())], Vec::new()),
    ];

    let flat = description.flatten();

    assert_eq!(flat.layers.len(), 7);
    assert_eq!(flat.containers, vec![0..3, 3..5, 5..6, 6..7]);
    // (The groups are containers 1, 2, 3 in the order they are met: the group that is the frame's first layer, the one that is its last, and then the group in the last one.
    // The frame's own layers come first in the list, and then those of each group in turn; so the pictures are found in that order.)
    assert_eq!(flat.layers[0].group, Some(1));
    assert_eq!(flat.layers[1].group, None);
    assert_eq!(flat.layers[2].group, Some(2));
    assert_eq!(flat.layers[5].group, Some(3));
    assert!(flat.layers.iter().all(|layer| layer.container < flat.containers.len()));
    assert_eq!(
        description.pictures_used(),
        vec![
            super::PictureId(1),
            super::PictureId(5),
            super::PictureId(6),
            super::PictureId(9)
        ]
    );
}

// --- what a group is, drawn ---

#[test]
fn a_group_with_no_effects_is_the_frame_its_layers_make_flat_wherever_it_is_and_however_deep() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let pictures = the_pictures();
        send_all(&mut renderer, &pictures);
        let (flat, flat_statistics) = render(
            &mut renderer,
            SIZE,
            vec![
                whole_frame(1, Vec::new()),
                whole_frame(2, Vec::new()),
                whole_frame(3, Vec::new()),
            ],
        );
        assert_eq!(flat_statistics.passes_recorded, 2);

        let (the_top_two, statistics) = render(
            &mut renderer,
            SIZE,
            vec![
                whole_frame(1, Vec::new()),
                a_group(vec![whole_frame(2, Vec::new()), whole_frame(3, Vec::new())], Vec::new()),
            ],
        );
        assert_eq!(
            statistics.passes_recorded, 3,
            "the group's layers, and the frame's, and the frame"
        );
        let (all_three, _) = render(
            &mut renderer,
            SIZE,
            vec![a_group(
                vec![
                    whole_frame(1, Vec::new()),
                    whole_frame(2, Vec::new()),
                    whole_frame(3, Vec::new()),
                ],
                Vec::new(),
            )],
        );
        let (nested, statistics) = render(
            &mut renderer,
            SIZE,
            vec![a_group(
                vec![
                    whole_frame(1, Vec::new()),
                    a_group(vec![whole_frame(2, Vec::new()), whole_frame(3, Vec::new())], Vec::new()),
                ],
                Vec::new(),
            )],
        );
        assert_eq!(statistics.passes_recorded, 4);

        // (The group's picture is half floats, as the frame's is: what differs is the rounding of the premultiplied numbers, a level at most.)
        for (name, grouped) in [
            ("the top two", &the_top_two),
            ("all three", &all_three),
            ("nested", &nested),
        ] {
            let difference = the_largest_difference(&flat, grouped);
            eprintln!("{name}: {difference} levels from the flat frame");
            assert!(difference <= 1, "{name}: {difference} levels from the flat frame");
        }
    });
}

#[test]
fn a_group_that_is_a_picture_over_the_whole_frame_is_not_resampled() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let pictures = the_pictures();
        send_all(&mut renderer, &pictures);
        let (plain, _) = render(&mut renderer, SIZE, vec![whole_frame(1, Vec::new())]);

        let (in_a_group, _) = render(
            &mut renderer,
            SIZE,
            vec![a_group(vec![whole_frame(1, Vec::new())], Vec::new())],
        );

        assert_eq!(
            in_a_group, plain,
            "the picture of the group is put on the frame pixel for pixel"
        );
    });
}

#[test]
fn the_effect_of_a_group_is_done_to_the_composite_and_not_to_each_layer() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let pictures = the_pictures();
        send_all(&mut renderer, &pictures);
        let effect = matrix_effect(a_matrix_that_cuts_off());

        // The composite of the two layers is opaque (the one behind is), so the frame they make flat is the picture of the group: sent back, it is what the effect is done to.
        let (composite, _) = frame_of(
            &mut renderer,
            SIZE,
            &[],
            vec![whole_frame(1, Vec::new()), whole_frame(2, Vec::new())],
        );
        let composite_picture = TestPicture {
            width: SIZE.0,
            height: SIZE.1,
            rgba: composite.rgba_pixels.clone(),
        };
        renderer
            .upload_picture(super::PictureId(9), &composite_picture.as_picture_data())
            .unwrap();
        let (done_to_the_composite, _) = render(&mut renderer, SIZE, vec![whole_frame(9, vec![effect.clone()])]);

        let (group, statistics) = render(
            &mut renderer,
            SIZE,
            vec![a_group(
                vec![whole_frame(1, Vec::new()), whole_frame(2, Vec::new())],
                vec![effect.clone()],
            )],
        );
        let (done_to_each, _) = render(
            &mut renderer,
            SIZE,
            vec![whole_frame(1, vec![effect.clone()]), whole_frame(2, vec![effect])],
        );

        assert_eq!(
            statistics.passes_recorded, 4,
            "the group's layers, its effect, the frame's layer, the frame"
        );
        assert_eq!(statistics.effects_not_run, 0);
        // (The composite that was sent back was rounded to 8 bits and the group's was not: two levels at most, as measured on 2026-10-21; and what the placeholder did is 108 levels away.)
        let from_the_composite = the_largest_difference(&group, &done_to_the_composite);
        eprintln!("group against the effect done to the composite: {from_the_composite}");
        assert!(
            from_the_composite <= 2,
            "the group is the effect done to the composite: {from_the_composite} levels away"
        );
        let from_each = the_largest_difference(&group, &done_to_each);
        eprintln!("group against the effect done to each layer: {from_each}");
        assert!(
            from_each > 50,
            "and it is not what the effect done to each layer is (the placeholder): only {from_each} levels away"
        );
    });
}

#[test]
fn the_blur_of_a_group_is_a_blur_of_the_composite_where_the_layers_meet() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let behind = TestPicture::gradient(SIZE.0, SIZE.1);
        let in_front = noisy(SIZE.0, SIZE.1, 5, false);
        renderer
            .upload_picture(super::PictureId(1), &behind.as_picture_data())
            .unwrap();
        renderer
            .upload_picture(super::PictureId(2), &in_front.as_picture_data())
            .unwrap();
        // A picture of a quarter of the frame, in the middle: its edges are in the middle of the other.
        let small = |picture: u64| LayerDescription {
            source: LayerSource::Picture(super::PictureId(picture)),
            corners: super::oracle_tests::upright(0.25, 0.25, 0.5, 0.5),
            effects: Vec::new(),
        };
        let blur = EffectCall::blur(60.0);

        let (composite, _) = render(&mut renderer, SIZE, vec![whole_frame(1, Vec::new()), small(2)]);
        let composite_picture = TestPicture {
            width: SIZE.0,
            height: SIZE.1,
            rgba: composite.rgba_pixels.clone(),
        };
        renderer
            .upload_picture(super::PictureId(9), &composite_picture.as_picture_data())
            .unwrap();
        let (blurred_composite, _) = render(&mut renderer, SIZE, vec![whole_frame(9, vec![blur.clone()])]);
        let (group, _) = render(
            &mut renderer,
            SIZE,
            vec![a_group(vec![whole_frame(1, Vec::new()), small(2)], vec![blur.clone()])],
        );
        let mut blurred_layers = small(2);
        blurred_layers.effects = vec![blur.clone()];
        let (each, _) = render(&mut renderer, SIZE, vec![whole_frame(1, vec![blur]), blurred_layers]);

        let from_the_composite = the_largest_difference(&group, &blurred_composite);
        eprintln!("blur: group against the blur of the composite: {from_the_composite}");
        assert!(
            from_the_composite <= 3,
            "a blur of the composite: {from_the_composite} levels away"
        );
        let from_each = the_largest_difference(&group, &each);
        eprintln!("blur: group against the blur of each layer: {from_each}");
        assert!(
            from_each > 40,
            "not the blur of each layer, whose small picture keeps a hard edge over the other: {from_each} levels away"
        );
    });
}

#[test]
fn a_group_in_a_group_has_its_effects_done_in_order_from_the_inside() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let pictures = the_pictures();
        send_all(&mut renderer, &pictures);
        let inner = matrix_effect(ColourMatrix::hue_rotation(70.0));
        let outer = matrix_effect(a_matrix_that_cuts_off());
        let layers = || vec![whole_frame(1, Vec::new()), whole_frame(2, Vec::new())];

        let (in_two_groups, statistics) = render(
            &mut renderer,
            SIZE,
            vec![a_group(
                vec![a_group(layers(), vec![inner.clone()])],
                vec![outer.clone()],
            )],
        );
        let (in_one_group, _) = render(&mut renderer, SIZE, vec![a_group(layers(), vec![inner, outer])]);

        assert_eq!(statistics.passes_recorded, 6);
        let difference = the_largest_difference(&in_two_groups, &in_one_group);
        eprintln!("two groups against one: {difference}");
        assert!(difference <= 1, "{difference} levels apart");
    });
}

#[test]
fn a_group_that_is_not_over_the_whole_frame_is_put_where_its_corners_say() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let pictures = the_pictures();
        send_all(&mut renderer, &pictures);
        let (behind, _) = render(&mut renderer, SIZE, vec![whole_frame(1, Vec::new())]);
        let mut shrunk = a_group(
            vec![LayerDescription {
                source: LayerSource::Solid([255, 0, 0, 255]),
                corners: CORNERS_OF_THE_WHOLE_PICTURE,
                effects: Vec::new(),
            }],
            Vec::new(),
        );
        shrunk.corners = super::oracle_tests::upright(0.25, 0.25, 0.5, 0.5);

        let (frame, _) = render(&mut renderer, SIZE, vec![whole_frame(1, Vec::new()), shrunk]);

        let pixel = |frame: &RenderedFrame, across: u32, down: u32| {
            let first = ((down * SIZE.0 + across) * 4) as usize;
            [
                frame.rgba_pixels[first],
                frame.rgba_pixels[first + 1],
                frame.rgba_pixels[first + 2],
            ]
        };
        assert_eq!(pixel(&frame, 32, 24), [255, 0, 0], "in the middle the group's red is");
        assert_eq!(
            pixel(&frame, 2, 2),
            pixel(&behind, 2, 2),
            "at the corner what is behind shows"
        );
        assert_eq!(pixel(&frame, 61, 45), pixel(&behind, 61, 45));
    });
}

#[test]
fn the_pictures_of_the_layers_of_a_group_must_be_on_the_card_like_any_others() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let mut description = FrameDescription::black(SIZE);
        description.layers = vec![a_group(vec![whole_frame(77, Vec::new())], Vec::new())];

        let problem = renderer
            .render(&description, std::time::Duration::from_secs(60))
            .unwrap_err();

        assert!(
            matches!(&problem, super::RenderProblem::PicturesMissing(missing) if missing == &[super::PictureId(77)]),
            "{problem}"
        );
    });
}

// --- the description ---

#[test]
fn a_description_with_groups_written_as_json_and_read_back_is_the_same_and_one_without_is_as_it_was() {
    let mut description = FrameDescription::black(SIZE);
    description.layers = vec![
        whole_frame(1, Vec::new()),
        a_group(
            vec![a_group(vec![whole_frame(2, Vec::new())], vec![EffectCall::blur(3.0)])],
            vec![EffectCall::exposure(1.0)],
        ),
    ];

    let text = serde_json::to_string(&description).unwrap();
    let read_back: FrameDescription = serde_json::from_str(&text).unwrap();

    assert_eq!(read_back, description);
    assert_eq!(read_back.fingerprint(), description.fingerprint());
    // A layer that is a picture is written as it always was.
    assert!(
        serde_json::to_string(&whole_frame(1, Vec::new()).source)
            .unwrap()
            .contains("\"Picture\"")
    );
}

#[test]
fn everything_in_a_group_changes_the_fingerprint_and_a_group_is_not_its_layers_flat() {
    let make = |effect_value: f32, picture: u64| {
        let mut description = FrameDescription::black(SIZE);
        description.layers = vec![a_group(
            vec![whole_frame(picture, vec![EffectCall::exposure(effect_value)])],
            Vec::new(),
        )];
        description
    };
    let original = make(1.0, 2);
    assert_ne!(
        original.fingerprint(),
        make(1.5, 2).fingerprint(),
        "a number of an effect in a group"
    );
    assert_ne!(
        original.fingerprint(),
        make(1.0, 3).fingerprint(),
        "a picture in a group"
    );

    let mut flat = FrameDescription::black(SIZE);
    flat.layers = vec![whole_frame(2, vec![EffectCall::exposure(1.0)])];
    assert_ne!(
        flat.fingerprint(),
        original.fingerprint(),
        "a group is another frame than its layers flat"
    );
    assert_ne!(
        flat.fingerprint_of_what_is_shown(),
        original.fingerprint_of_what_is_shown()
    );
    let mut with_effects_on_the_group = original.clone();
    with_effects_on_the_group.layers[0].effects = vec![EffectCall::exposure(0.5)];
    assert_ne!(with_effects_on_the_group.fingerprint(), original.fingerprint());
}

#[test]
fn a_group_is_clear_where_none_of_its_layers_are_so_what_is_behind_it_shows() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let pictures = the_pictures();
        send_all(&mut renderer, &pictures);
        let (behind, _) = render(&mut renderer, SIZE, vec![whole_frame(1, Vec::new())]);
        // A group of one picture that is a quarter of the frame, in the middle.
        let small = LayerDescription {
            source: LayerSource::Picture(super::PictureId(3)),
            corners: super::oracle_tests::upright(0.25, 0.25, 0.5, 0.5),
            effects: Vec::new(),
        };
        let (flat, _) = render(&mut renderer, SIZE, vec![whole_frame(1, Vec::new()), small.clone()]);

        let (grouped, _) = render(
            &mut renderer,
            SIZE,
            vec![whole_frame(1, Vec::new()), a_group(vec![small], Vec::new())],
        );

        assert!(
            the_largest_difference(&grouped, &flat) <= 1,
            "the group's picture is clear outside the small one"
        );
        let pixel = |frame: &RenderedFrame, across: u32, down: u32| {
            let first = ((down * SIZE.0 + across) * 4) as usize;
            [
                frame.rgba_pixels[first],
                frame.rgba_pixels[first + 1],
                frame.rgba_pixels[first + 2],
            ]
        };
        assert_eq!(
            pixel(&grouped, 2, 2),
            pixel(&behind, 2, 2),
            "at the corner what is behind shows"
        );
    });
}

#[test]
fn a_group_that_begins_with_opaque_black_has_the_effect_done_to_the_black_as_well_and_one_that_does_not_has_nothing_to_change_there()
 {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let pictures = the_pictures();
        send_all(&mut renderer, &pictures);
        // A change of colours that makes everything lighter by a fifth, which is something that black has to be there for.
        let lighter =
            EffectCall::colour_matrix_with_offsets([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], [0.2; 3]);
        // A picture a quarter of the frame, in the middle: round it the group has nothing, unless it begins with black.
        let small = LayerDescription {
            source: LayerSource::Picture(super::PictureId(3)),
            corners: super::oracle_tests::upright(0.25, 0.25, 0.5, 0.5),
            effects: Vec::new(),
        };
        let black = LayerDescription {
            source: LayerSource::Solid([0, 0, 0, 255]),
            corners: CORNERS_OF_THE_WHOLE_PICTURE,
            effects: Vec::new(),
        };
        let pixel = |frame: &RenderedFrame, across: u32, down: u32| {
            let first = ((down * SIZE.0 + across) * 4) as usize;
            [
                frame.rgba_pixels[first],
                frame.rgba_pixels[first + 1],
                frame.rgba_pixels[first + 2],
            ]
        };

        let (over_black, _) = render(
            &mut renderer,
            SIZE,
            vec![a_group(vec![black, small.clone()], vec![lighter.clone()])],
        );
        let (over_nothing, _) = render(&mut renderer, SIZE, vec![a_group(vec![small], vec![lighter])]);

        let corner = pixel(&over_black, 2, 2);
        assert!(
            corner.iter().all(|level| (50..=52).contains(level)),
            "a fifth of the way up from black, which is 51 of the 255: {corner:?}"
        );
        assert_eq!(
            pixel(&over_nothing, 2, 2),
            [0, 0, 0],
            "nothing there to be made lighter: it is clear, over black"
        );
    });
}
