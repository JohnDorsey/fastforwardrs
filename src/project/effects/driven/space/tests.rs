// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

type Vector = [f32; 3];

fn matrix_product(left: [[f32; 3]; 3], right: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut made = [[0.0; 3]; 3];
    for row in 0..3 {
        for column in 0..3 {
            made[row][column] = (0..3).map(|k| left[row][k] * right[k][column]).sum();
        }
    }
    made
}

fn about_x(degrees: f32) -> [[f32; 3]; 3] {
    let (s, c) = degrees.to_radians().sin_cos();
    [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]]
}

fn about_y(degrees: f32) -> [[f32; 3]; 3] {
    let (s, c) = degrees.to_radians().sin_cos();
    [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]]
}

fn about_z(degrees: f32) -> [[f32; 3]; 3] {
    let (s, c) = degrees.to_radians().sin_cos();
    [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]
}

fn applied(matrix: [[f32; 3]; 3], vector: Vector) -> Vector {
    [0, 1, 2].map(|row| (0..3).map(|k| matrix[row][k] * vector[k]).sum())
}

/// What the shader does with the quaternion: the same formula as the generated code, written out here as the oracle for the quaternion's meaning.
fn turned_by_the_quaternion(quaternion: [f32; 4], vector: Vector) -> Vector {
    fn cross(a: Vector, b: Vector) -> Vector {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }
    let axis = [quaternion[0], quaternion[1], quaternion[2]];
    let w = quaternion[3];
    let inner = cross(axis, vector);
    let inner = [
        inner[0] + w * vector[0],
        inner[1] + w * vector[1],
        inner[2] + w * vector[2],
    ];
    let outer = cross(axis, inner);
    [
        vector[0] + 2.0 * outer[0],
        vector[1] + 2.0 * outer[1],
        vector[2] + 2.0 * outer[2],
    ]
}

fn near(left: Vector, right: Vector) -> bool {
    left.iter().zip(&right).all(|(a, b)| (a - b).abs() < 1.0e-5)
}

#[test]
fn a_plane_that_is_not_moved_or_turned_has_the_numbers_of_an_unmoved_unturned_shape_of_the_default_size() {
    let numbers = WorldPlacement::default().numbers();

    assert_eq!(numbers, [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 3.2, 3.2]);
    assert_eq!(WorldPlacement::default().shape, WorldShape::Plane);
}

#[test]
fn the_numbers_are_the_middle_then_the_quaternion_then_the_size() {
    let placement = WorldPlacement {
        position: [1.0, 2.0, 3.0],
        width: 5.0,
        height: 7.0,
        ..WorldPlacement::default()
    };

    assert_eq!(placement.numbers(), [1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 1.0, 5.0, 7.0]);
}

#[test]
fn the_quaternion_takes_a_point_of_space_into_the_frame_of_a_shape_that_was_turned_roll_first_then_pitch_then_yaw() {
    for (yaw, pitch, roll) in [
        (90.0, 0.0, 0.0),
        (0.0, 90.0, 0.0),
        (0.0, 0.0, 90.0),
        (30.0, 0.0, 0.0),
        (30.0, 50.0, 0.0),
        (30.0, 50.0, 70.0),
        (-120.0, 15.0, -40.0),
        (200.0, -80.0, 170.0),
    ] {
        let placement = WorldPlacement {
            yaw_pitch_roll_degrees: [yaw, pitch, roll],
            ..WorldPlacement::default()
        };
        // The turn of the shape, as matrices (the roll is done to the shape first, so its matrix is the one next to the point).
        let shape_in_space = matrix_product(about_y(yaw), matrix_product(about_x(pitch), about_z(roll)));
        for point_of_the_shape in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.3, -0.7, 0.5]] {
            let point_of_space = applied(shape_in_space, point_of_the_shape);

            let found = turned_by_the_quaternion(placement.quaternion_into_the_frame_of_the_shape(), point_of_space);

            assert!(
                near(found, point_of_the_shape),
                "yaw {yaw}, pitch {pitch}, roll {roll}: the point {point_of_the_shape:?} of the shape is at {point_of_space:?} in space, and the quaternion brought it to {found:?}"
            );
        }
    }
}

#[test]
fn a_yaw_of_a_quarter_turn_has_the_front_of_the_shape_towards_x() {
    let placement = WorldPlacement {
        yaw_pitch_roll_degrees: [90.0, 0.0, 0.0],
        ..WorldPlacement::default()
    };

    // The front is +z in the frame of the shape: so +x of space is its front.
    let front_of_the_shape =
        turned_by_the_quaternion(placement.quaternion_into_the_frame_of_the_shape(), [1.0, 0.0, 0.0]);

    assert!(near(front_of_the_shape, [0.0, 0.0, 1.0]), "{front_of_the_shape:?}");
}

#[test]
fn the_quaternion_has_length_one_and_no_negative_zero() {
    for angles in [[0.0, 0.0, 0.0], [10.0, 20.0, 30.0], [-170.0, 80.0, 359.0]] {
        let placement = WorldPlacement {
            yaw_pitch_roll_degrees: angles,
            ..WorldPlacement::default()
        };
        let quaternion = placement.quaternion_into_the_frame_of_the_shape();

        let length: f32 = quaternion.iter().map(|part| part * part).sum::<f32>().sqrt();
        assert!((length - 1.0).abs() < 1.0e-5, "{angles:?}: {length}");
    }
    let still = WorldPlacement::default().quaternion_into_the_frame_of_the_shape();
    assert!(
        still.iter().all(|part| part.to_bits() != (-0.0f32).to_bits()),
        "{still:?}"
    );
}

#[test]
fn a_placement_is_well_formed_only_with_numbers_that_a_shape_can_be_put_in_space_with() {
    assert!(WorldPlacement::default().is_well_formed());
    let default = WorldPlacement::default();
    let bad = [
        WorldPlacement {
            position: [f32::NAN, 0.0, 0.0],
            ..default
        },
        WorldPlacement {
            position: [0.0, f32::INFINITY, 0.0],
            ..default
        },
        WorldPlacement {
            position: [0.0, 0.0, 2.0e6],
            ..default
        },
        WorldPlacement {
            yaw_pitch_roll_degrees: [0.0, f32::NAN, 0.0],
            ..default
        },
        WorldPlacement { width: 0.0, ..default },
        WorldPlacement { width: -1.0, ..default },
        WorldPlacement {
            height: f32::NAN,
            ..default
        },
        WorldPlacement {
            height: 1.0e-7,
            ..default
        },
        WorldPlacement {
            height: 2.0e6,
            ..default
        },
    ];
    for placement in bad {
        assert!(!placement.is_well_formed(), "{placement:?}");
        assert!(!MaskSpace::World(placement).is_well_formed(), "{placement:?}");
    }
    assert!(MaskSpace::Screen.is_well_formed());
    // (The edges that are allowed.)
    assert!(
        WorldPlacement {
            width: SMALLEST_SIZE_OF_A_SHAPE,
            height: LARGEST_DISTANCE_OF_A_SHAPE,
            position: [-LARGEST_DISTANCE_OF_A_SHAPE, 0.0, 0.0],
            ..default
        }
        .is_well_formed()
    );
}

#[test]
fn a_mask_is_over_the_picture_until_it_is_put_in_space_and_says_which_shader_it_needs() {
    assert_eq!(MaskSpace::default(), MaskSpace::Screen);
    assert_eq!(MaskSpace::Screen.for_the_shader(), Space::Screen);
    assert_eq!(MaskSpace::Screen.numbers_of_the_call(), None);
    assert!(!MaskSpace::Screen.is_in_space());

    for shape in WorldShape::ALL {
        let in_space = MaskSpace::World(WorldPlacement::default().with_the_shape(shape));
        assert_eq!(in_space.for_the_shader(), Space::World(shape));
        assert_eq!(
            in_space.numbers_of_the_call(),
            Some(WorldPlacement::default().numbers())
        );
        assert!(in_space.is_in_space());
    }
}

#[test]
fn changing_the_shape_keeps_everything_else_so_that_a_plane_chosen_again_is_where_it_was() {
    let placed = WorldPlacement {
        position: [1.0, 2.0, 3.0],
        yaw_pitch_roll_degrees: [10.0, 20.0, 30.0],
        width: 4.0,
        height: 5.0,
        ..WorldPlacement::default()
    };

    let there_and_back = placed
        .with_the_shape(WorldShape::Sphere)
        .with_the_shape(WorldShape::Plane);

    assert_eq!(there_and_back, placed);
    assert_eq!(placed.with_the_shape(WorldShape::Cylinder).numbers(), placed.numbers());
}

#[test]
fn a_mask_that_is_over_the_picture_begins_in_space_as_the_placement_it_is_given() {
    let to_begin_with = WorldPlacement {
        width: 23.0,
        height: 23.0,
        ..WorldPlacement::default()
    };
    assert_eq!(MaskSpace::Screen.placement_or(to_begin_with), to_begin_with);
    let placed = WorldPlacement {
        width: 9.0,
        ..WorldPlacement::default()
    };
    assert_eq!(MaskSpace::World(placed).placement_or(to_begin_with), placed);
}
