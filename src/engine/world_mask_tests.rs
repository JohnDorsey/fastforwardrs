// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A float input of a shader read from a mask **laid in space** (a plane, a sphere or a cylinder), made on the software adapter with the real rewriter and compiler. The frames are 8 by 8, the
//! masks are pictures of the same size, and the shaders put the point of space they work at where the test wants it (a flat sheet of x and y, or the points of a sphere or a cylinder round
//! the middle), so that **which pixel of the frame reads which pixel of the mask is worked out here by hand** and the generated code has to agree: the sides, the offsets, the rotations (as
//! the quaternion that turns the world into the frame of the shape), where there is nothing, and that the size of a sphere or a cylinder does not matter.
//!
//! The numbers are chosen so that every read is at the middle of a pixel of the mask (the sampler smooths between pixels), or between two pixels of one colour.

use super::FrameRenderer;
use super::mask_tests::{a_colour_over, all_of_it, installed_with_masks, is_near, mask_of, shown, the_left_half_of_it};
use super::testing::with_software_graphics;
use crate::isf::{DrivenInput, MOST_POINTS_OF_A_GRAPH, Mapping, MaskChannel, Space, WorldShape};

/// The numbers of the placement of a shape that is not turned and is at the middle of space, with this size.
fn placement_at_the_middle(width: f32, height: f32) -> Vec<f32> {
    placement([0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], [width, height])
}

/// The nine numbers of a placement: where the middle is, the quaternion (`x`, `y`, `z`, `w`) that turns a point of space into the frame of the shape, and the size.
fn placement(middle: [f32; 3], turn: [f32; 4], size: [f32; 2]) -> Vec<f32> {
    let mut numbers = Vec::new();
    numbers.extend(middle);
    numbers.extend(turn);
    numbers.extend(size);
    numbers
}

/// The quaternion that turns a point by `degrees` round the y axis (as the shape is turned the same way, a point of space that is turned by it is in the frame of the shape).
fn turn_round_y(degrees: f32) -> [f32; 4] {
    let half = degrees.to_radians() / 2.0;
    [0.0, half.sin(), 0.0, half.cos()]
}

fn turn_round_z(degrees: f32) -> [f32; 4] {
    let half = degrees.to_radians() / 2.0;
    [0.0, 0.0, half.sin(), half.cos()]
}

fn level_in_space(shape: WorldShape, channel: MaskChannel) -> DrivenInput {
    DrivenInput {
        name: "level".to_owned(),
        mask: 0,
        channel,
        mapping: Mapping::Straight,
        space: Space::World(shape),
    }
}

/// A filter that works at the points of a flat sheet in the plane of x and y, -1 to 1 across the frame and up it, and shows `level` there as a grey.
const A_SHEET_OF_POINTS: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"level\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 1.0, \"DEFAULT\": 0.5, \"WORLD_SPACE\": true } ] }*/\nvoid main() {\n\tisf_world_position = vec3((isf_FragNormCoord - 0.5) * 2.0, 0.0);\n\tgl_FragColor = vec4(vec3(level), 1.0);\n}\n";

/// A filter that works at the points of a ball of radius `reach` (an input, 0 to 10): the direction goes from -90 to 90 degrees across the frame and up it, so a point is at the longitude and
/// the latitude that the pixel is at; it shows `level` there as a grey.
const A_BALL_OF_POINTS: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"level\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 1.0, \"DEFAULT\": 0.5, \"WORLD_SPACE\": true }, { \"NAME\": \"reach\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 10.0, \"DEFAULT\": 1.0 } ] }*/\nvoid main() {\n\tvec2 angle = (isf_FragNormCoord - 0.5) * 3.14159265359;\n\tisf_world_position = reach * vec3(sin(angle.x) * cos(angle.y), sin(angle.y), cos(angle.x) * cos(angle.y));\n\tgl_FragColor = vec4(vec3(level), 1.0);\n}\n";

/// A filter that works at the points of a tube of radius `reach` round the y axis: the direction round goes from -90 to 90 degrees across the frame, and the height from -1 to 1 up it; it shows
/// `level` there as a grey.
const A_TUBE_OF_POINTS: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"level\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 1.0, \"DEFAULT\": 0.5, \"WORLD_SPACE\": true }, { \"NAME\": \"reach\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 10.0, \"DEFAULT\": 1.0 } ] }*/\nvoid main() {\n\tfloat angle = (isf_FragNormCoord.x - 0.5) * 3.14159265359;\n\tisf_world_position = vec3(reach * sin(angle), (isf_FragNormCoord.y - 0.5) * 2.0, reach * cos(angle));\n\tgl_FragColor = vec4(vec3(level), 1.0);\n}\n";

/// A filter that uses `level` at two places of a sheet in one pass: red is at x = -0.875 and green is at x = 0.875 (so the input is read at each use, not once).
const LEVEL_AT_TWO_PLACES: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"level\", \"TYPE\": \"float\", \"MIN\": 0.0, \"MAX\": 1.0, \"DEFAULT\": 0.5, \"WORLD_SPACE\": true } ] }*/\nvoid main() {\n\tisf_world_position = vec3(-0.875, 0.0, 0.0);\n\tfloat on_the_left = level;\n\tisf_world_position = vec3(0.875, 0.0, 0.0);\n\tfloat on_the_right = level;\n\tgl_FragColor = vec4(on_the_left, on_the_right, 0.0, 1.0);\n}\n";

/// The mask: 200 of red on the left half and 50 on the right half.
fn red_200_on_the_left_and_50_on_the_right() -> Vec<crate::engine::GroupDescription> {
    vec![mask_of(vec![
        all_of_it([50, 0, 0, 255]),
        the_left_half_of_it([200, 0, 0, 255]),
    ])]
}

/// The mask: 200 of red on the top half and 50 on the bottom half.
fn red_200_on_the_top_and_50_at_the_bottom() -> Vec<crate::engine::GroupDescription> {
    vec![mask_of(vec![
        all_of_it([50, 0, 0, 255]),
        a_colour_over([(0.0, 0.0), (1.0, 0.0), (1.0, 0.5), (0.0, 0.5)], [200, 0, 0, 255]),
    ])]
}

/// What the frame shows in the pixel `across` and `down`, as the grey it is (the red of it), after saying that it is a grey.
fn grey_at(frame: &crate::engine::RenderedFrame, across: u32, down: u32) -> u8 {
    let pixel = frame.pixel(across, down);
    assert!(
        pixel[0].abs_diff(pixel[1]) <= 1 && pixel[1].abs_diff(pixel[2]) <= 1,
        "not a grey: {pixel:?}"
    );
    pixel[0]
}

/// Whether every pixel of the columns `columns`, in every row, is `wanted` to within 2; says where not.
fn columns_are(frame: &crate::engine::RenderedFrame, columns: std::ops::RangeInclusive<u32>, wanted: u8) {
    for across in columns {
        for down in 0..8 {
            let found = grey_at(frame, across, down);
            assert!(
                found.abs_diff(wanted) <= 2,
                "at {across}, {down}: {found}, wanted {wanted}"
            );
        }
    }
}

fn rows_are(frame: &crate::engine::RenderedFrame, rows: std::ops::RangeInclusive<u32>, wanted: u8) {
    for down in rows {
        for across in 0..8 {
            let found = grey_at(frame, across, down);
            assert!(
                found.abs_diff(wanted) <= 2,
                "at {across}, {down}: {found}, wanted {wanted}"
            );
        }
    }
}

/// The frame of the shader `text` with `level` read from the mask, laid as `shape` in space, with `inputs` as the numbers of the call (the input's own, the other inputs, the placement).
fn frame_of(
    renderer: &mut FrameRenderer,
    key: &str,
    text: &str,
    shape: WorldShape,
    inputs: &[f32],
    masks: Vec<crate::engine::GroupDescription>,
) -> crate::engine::RenderedFrame {
    installed_with_masks(renderer, key, text, &[level_in_space(shape, MaskChannel::Red)]);
    let (frame, statistics) = shown(renderer, key, inputs, masks);
    assert_eq!(statistics.effects_not_run, 0, "{key}");
    frame
}

/// The own number of `level` in these tests (0.5, which is 128 of 255): what is read where the mask has nothing.
const OWN: f32 = 0.5;
const NOTHING: u8 = 128;

fn inputs_of_a_sheet(placement: Vec<f32>) -> Vec<f32> {
    let mut inputs = vec![OWN];
    inputs.extend(placement);
    inputs
}

fn inputs_of_a_ball_or_a_tube(reach: f32, placement: Vec<f32>) -> Vec<f32> {
    let mut inputs = vec![OWN, reach];
    inputs.extend(placement);
    inputs
}

#[test]
fn a_plane_gives_each_side_of_the_frame_the_colour_of_the_same_side_of_the_mask() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // 2 wide and 2 tall, at the middle: x from -1 to 1 is the picture from the left edge to the right edge.
        let frame = frame_of(
            &mut renderer,
            "test:plane",
            A_SHEET_OF_POINTS,
            WorldShape::Plane,
            &inputs_of_a_sheet(placement_at_the_middle(2.0, 2.0)),
            red_200_on_the_left_and_50_on_the_right(),
        );

        columns_are(&frame, 0..=2, 200);
        columns_are(&frame, 5..=7, 50);
    });
}

#[test]
fn a_plane_has_the_top_of_the_picture_at_the_top_of_space() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let frame = frame_of(
            &mut renderer,
            "test:plane:top",
            A_SHEET_OF_POINTS,
            WorldShape::Plane,
            &inputs_of_a_sheet(placement_at_the_middle(2.0, 2.0)),
            red_200_on_the_top_and_50_at_the_bottom(),
        );

        rows_are(&frame, 0..=2, 200);
        rows_are(&frame, 5..=7, 50);
    });
}

#[test]
fn the_middle_of_a_plane_is_moved_by_its_position() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // The sheet moved a quarter of a unit to the right (one pixel of the mask): the mask's pixel 0 is under the frame's column 1, and the frame's column 0 is left of the sheet.
        let moved = placement([0.25, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], [2.0, 2.0]);
        let frame = frame_of(
            &mut renderer,
            "test:plane:moved",
            A_SHEET_OF_POINTS,
            WorldShape::Plane,
            &inputs_of_a_sheet(moved),
            red_200_on_the_left_and_50_on_the_right(),
        );

        columns_are(&frame, 0..=0, NOTHING);
        columns_are(&frame, 1..=4, 200);
        columns_are(&frame, 5..=7, 50);
    });
}

#[test]
fn where_a_plane_is_not_there_the_mask_has_nothing_and_the_value_is_the_number_the_call_has() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // 1 wide: the sheet is x from -0.5 to 0.5, which is the columns 2 to 5.
        let frame = frame_of(
            &mut renderer,
            "test:plane:narrow",
            A_SHEET_OF_POINTS,
            WorldShape::Plane,
            &inputs_of_a_sheet(placement_at_the_middle(1.0, 2.0)),
            red_200_on_the_left_and_50_on_the_right(),
        );

        columns_are(&frame, 0..=1, NOTHING);
        columns_are(&frame, 2..=3, 200);
        columns_are(&frame, 4..=5, 50);
        columns_are(&frame, 6..=7, NOTHING);
        // And 1 tall: y from -0.5 to 0.5, which is the rows 2 to 5, whatever the mask has (all of one colour here).
        let flat = vec![mask_of(vec![all_of_it([200, 0, 0, 255])])];
        let frame = frame_of(
            &mut renderer,
            "test:plane:short",
            A_SHEET_OF_POINTS,
            WorldShape::Plane,
            &inputs_of_a_sheet(placement_at_the_middle(2.0, 1.0)),
            flat,
        );
        rows_are(&frame, 0..=1, NOTHING);
        rows_are(&frame, 2..=5, 200);
        rows_are(&frame, 6..=7, NOTHING);
    });
}

#[test]
fn a_plane_turned_half_a_turn_round_y_is_the_picture_seen_from_behind() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // A turn of 180 degrees about y takes x to -x (and z to -z): the left of the frame is the right of the picture.
        let turned = placement([0.0, 0.0, 0.0], turn_round_y(180.0), [2.0, 2.0]);
        let frame = frame_of(
            &mut renderer,
            "test:plane:behind",
            A_SHEET_OF_POINTS,
            WorldShape::Plane,
            &inputs_of_a_sheet(turned),
            red_200_on_the_left_and_50_on_the_right(),
        );

        columns_are(&frame, 0..=2, 50);
        columns_are(&frame, 5..=7, 200);
    });
}

#[test]
fn a_quarter_turn_of_a_plane_round_z_puts_the_top_of_the_picture_at_the_right_of_the_frame() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // The quaternion turns a point of space into the frame of the shape: a quarter turn about z takes x (the right of the frame) to y (the top of the picture).
        let turned = placement([0.0, 0.0, 0.0], turn_round_z(90.0), [2.0, 2.0]);
        let frame = frame_of(
            &mut renderer,
            "test:plane:quarter",
            A_SHEET_OF_POINTS,
            WorldShape::Plane,
            &inputs_of_a_sheet(turned),
            red_200_on_the_top_and_50_at_the_bottom(),
        );

        columns_are(&frame, 0..=3, 50);
        columns_are(&frame, 4..=7, 200);
    });
}

#[test]
fn a_sphere_is_the_picture_wrapped_round_it_with_its_middle_facing_z() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // The ball of points looks at -90 to 90 degrees of longitude, the middle of the picture facing z, which is the straight-ahead (0): the left of the frame is the left half of the picture.
        let frame = frame_of(
            &mut renderer,
            "test:sphere",
            A_BALL_OF_POINTS,
            WorldShape::Sphere,
            &inputs_of_a_ball_or_a_tube(2.0, placement_at_the_middle(1.0, 1.0)),
            red_200_on_the_left_and_50_on_the_right(),
        );
        columns_are(&frame, 0..=2, 200);
        columns_are(&frame, 5..=7, 50);

        // And the top of the picture is the pole at +y: the top of the frame (latitude up) reads the top of the picture.
        let frame = frame_of(
            &mut renderer,
            "test:sphere:poles",
            A_BALL_OF_POINTS,
            WorldShape::Sphere,
            &inputs_of_a_ball_or_a_tube(2.0, placement_at_the_middle(1.0, 1.0)),
            red_200_on_the_top_and_50_at_the_bottom(),
        );
        rows_are(&frame, 0..=3, 200);
        rows_are(&frame, 4..=7, 50);
    });
}

#[test]
fn the_size_of_a_sphere_does_not_matter_only_the_direction_from_its_middle() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let near = frame_of(
            &mut renderer,
            "test:sphere:near",
            A_BALL_OF_POINTS,
            WorldShape::Sphere,
            &inputs_of_a_ball_or_a_tube(0.5, placement_at_the_middle(1.0, 1.0)),
            red_200_on_the_left_and_50_on_the_right(),
        );
        let far = frame_of(
            &mut renderer,
            "test:sphere:far",
            A_BALL_OF_POINTS,
            WorldShape::Sphere,
            &inputs_of_a_ball_or_a_tube(7.0, placement_at_the_middle(1.0, 1.0)),
            red_200_on_the_left_and_50_on_the_right(),
        );

        for down in 0..8 {
            for across in 0..8 {
                let (a, b) = (grey_at(&near, across, down), grey_at(&far, across, down));
                assert!(a.abs_diff(b) <= 1, "at {across}, {down}: {a} against {b}");
            }
        }
        // (And the picture is not flat: it has both colours, so that this is not the same nothing twice.)
        assert!(grey_at(&near, 0, 4).abs_diff(grey_at(&near, 7, 4)) > 100);
    });
}

#[test]
fn the_middle_of_a_sphere_is_not_on_the_sphere_so_the_mask_has_nothing_there() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // A ball of radius 0: every point is the middle itself.
        let frame = frame_of(
            &mut renderer,
            "test:sphere:middle",
            A_BALL_OF_POINTS,
            WorldShape::Sphere,
            &inputs_of_a_ball_or_a_tube(0.0, placement_at_the_middle(1.0, 1.0)),
            red_200_on_the_left_and_50_on_the_right(),
        );

        columns_are(&frame, 0..=7, NOTHING);
    });
}

#[test]
fn a_sphere_turned_a_quarter_round_y_shows_the_other_half_of_the_picture_to_the_front() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // The points at longitudes -90 to 90 are, in the frame of the sphere, at 0 to 180: the right half of the picture, but for the pixels near the middle of the picture (column 0 is at 11 degrees
        // and the sampler smooths between the halves there).
        let turned = placement([0.0, 0.0, 0.0], turn_round_y(90.0), [1.0, 1.0]);
        let frame = frame_of(
            &mut renderer,
            "test:sphere:turned",
            A_BALL_OF_POINTS,
            WorldShape::Sphere,
            &inputs_of_a_ball_or_a_tube(2.0, turned),
            red_200_on_the_left_and_50_on_the_right(),
        );

        columns_are(&frame, 1..=7, 50);
    });
}

#[test]
fn a_sphere_is_moved_by_its_position() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // The ball of points is round the origin. A sphere whose middle is far to the left of it at x = -10 sees the ball from its right side (the direction is +x, longitude 90, which is the
        // right edge of the picture's right half): the whole frame reads the right half.
        let moved = placement([-10.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0, 1.0]);
        let frame = frame_of(
            &mut renderer,
            "test:sphere:moved",
            A_BALL_OF_POINTS,
            WorldShape::Sphere,
            &inputs_of_a_ball_or_a_tube(0.5, moved),
            red_200_on_the_left_and_50_on_the_right(),
        );

        // (The longitudes are all within a few degrees of 90 from there: the right half of the picture, 50, even where the pixel is at the pole's side.)
        columns_are(&frame, 0..=7, 50);
    });
}

#[test]
fn a_cylinder_is_the_picture_wrapped_once_round_it_and_as_tall_as_its_height() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // Round: as the sphere's. Up: 2 tall, so the heights -1 to 1 are the picture from the bottom to the top.
        let frame = frame_of(
            &mut renderer,
            "test:cylinder",
            A_TUBE_OF_POINTS,
            WorldShape::Cylinder,
            &inputs_of_a_ball_or_a_tube(2.0, placement_at_the_middle(1.0, 2.0)),
            red_200_on_the_left_and_50_on_the_right(),
        );
        columns_are(&frame, 0..=2, 200);
        columns_are(&frame, 5..=7, 50);

        let frame = frame_of(
            &mut renderer,
            "test:cylinder:top",
            A_TUBE_OF_POINTS,
            WorldShape::Cylinder,
            &inputs_of_a_ball_or_a_tube(2.0, placement_at_the_middle(1.0, 2.0)),
            red_200_on_the_top_and_50_at_the_bottom(),
        );
        rows_are(&frame, 0..=2, 200);
        rows_are(&frame, 5..=7, 50);
    });
}

#[test]
fn a_cylinder_has_nothing_above_or_below_its_height_and_nothing_on_its_axis() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // 1 tall: the heights -0.5 to 0.5, which are the rows 2 to 5 of the frame.
        let flat = vec![mask_of(vec![all_of_it([200, 0, 0, 255])])];
        let frame = frame_of(
            &mut renderer,
            "test:cylinder:short",
            A_TUBE_OF_POINTS,
            WorldShape::Cylinder,
            &inputs_of_a_ball_or_a_tube(2.0, placement_at_the_middle(1.0, 1.0)),
            flat.clone(),
        );
        rows_are(&frame, 0..=1, NOTHING);
        rows_are(&frame, 2..=5, 200);
        rows_are(&frame, 6..=7, NOTHING);

        // A tube of radius 0 is the axis itself.
        let frame = frame_of(
            &mut renderer,
            "test:cylinder:axis",
            A_TUBE_OF_POINTS,
            WorldShape::Cylinder,
            &inputs_of_a_ball_or_a_tube(0.0, placement_at_the_middle(1.0, 2.0)),
            flat,
        );
        rows_are(&frame, 0..=7, NOTHING);
    });
}

#[test]
fn the_size_round_of_a_cylinder_does_not_matter_and_neither_does_the_width_it_is_given() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let thin = frame_of(
            &mut renderer,
            "test:cylinder:thin",
            A_TUBE_OF_POINTS,
            WorldShape::Cylinder,
            &inputs_of_a_ball_or_a_tube(0.5, placement_at_the_middle(0.1, 2.0)),
            red_200_on_the_left_and_50_on_the_right(),
        );
        let wide = frame_of(
            &mut renderer,
            "test:cylinder:wide",
            A_TUBE_OF_POINTS,
            WorldShape::Cylinder,
            &inputs_of_a_ball_or_a_tube(7.0, placement_at_the_middle(40.0, 2.0)),
            red_200_on_the_left_and_50_on_the_right(),
        );

        for down in 0..8 {
            for across in 0..8 {
                let (a, b) = (grey_at(&thin, across, down), grey_at(&wide, across, down));
                assert!(a.abs_diff(b) <= 1, "at {across}, {down}: {a} against {b}");
            }
        }
        assert!(grey_at(&thin, 0, 4).abs_diff(grey_at(&thin, 7, 4)) > 100);
    });
}

#[test]
fn an_input_is_read_again_at_each_use_at_the_point_the_shader_has_set_by_then() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        installed_with_masks(
            &mut renderer,
            "test:two places",
            LEVEL_AT_TWO_PLACES,
            &[level_in_space(WorldShape::Plane, MaskChannel::Red)],
        );

        let (frame, statistics) = shown(
            &mut renderer,
            "test:two places",
            &inputs_of_a_sheet(placement_at_the_middle(2.0, 2.0)),
            red_200_on_the_left_and_50_on_the_right(),
        );

        assert_eq!(statistics.effects_not_run, 0);
        // Red is what the left of the sheet reads (200) and green what the right reads (50), in every pixel.
        for (across, down) in [(0, 0), (3, 4), (7, 7)] {
            assert!(
                is_near(frame.pixel(across, down), [200, 50, 0], 1),
                "{across}, {down}: {:?}",
                frame.pixel(across, down)
            );
        }
    });
}

#[test]
fn a_graph_maps_what_is_read_in_space_as_it_maps_what_is_read_over_the_picture() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        let through_a_graph = DrivenInput {
            mapping: Mapping::Graph,
            ..level_in_space(WorldShape::Plane, MaskChannel::Red)
        };
        installed_with_masks(
            &mut renderer,
            "test:graph in space",
            A_SHEET_OF_POINTS,
            &[through_a_graph],
        );
        // The graph that turns the value upside down: from (0, 1) to (1, 0), and the other points repeat the last (the numbers of a graph come before those of the placement).
        let mut places = vec![0.0];
        places.resize(MOST_POINTS_OF_A_GRAPH, 1.0);
        let mut heights = vec![1.0];
        heights.resize(MOST_POINTS_OF_A_GRAPH, 0.0);
        let mut inputs = vec![OWN];
        inputs.extend(places);
        inputs.extend(heights);
        inputs.extend(placement_at_the_middle(2.0, 2.0));

        let (frame, statistics) = shown(
            &mut renderer,
            "test:graph in space",
            &inputs,
            red_200_on_the_left_and_50_on_the_right(),
        );

        assert_eq!(statistics.effects_not_run, 0);
        // 200 of 255 is 0.784, which the graph makes 0.216 (55); 50 is 0.196, which it makes 0.804 (205).
        columns_are(&frame, 0..=2, 55);
        columns_are(&frame, 5..=7, 205);
    });
}

#[test]
fn a_mask_that_is_partly_see_through_gives_a_mixture_in_space_too() {
    with_software_graphics!(graphics, {
        let mut renderer = FrameRenderer::new(graphics.device.clone());
        // Half as opaque as it can be and as red as it can be, where the call's own number is 0.5: 0.5 * 0.498 + 1.0 * 0.502 = 0.751, which is 191.5 of 255.
        let frame = frame_of(
            &mut renderer,
            "test:plane:see through",
            A_SHEET_OF_POINTS,
            WorldShape::Plane,
            &inputs_of_a_sheet(placement_at_the_middle(2.0, 2.0)),
            vec![mask_of(vec![all_of_it([255, 0, 0, 128])])],
        );

        columns_are(&frame, 0..=7, 191);
    });
}
