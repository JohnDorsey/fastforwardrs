// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the mask of a driven value is laid: **over the picture** (the value at a pixel is the mask at that pixel) or **in space** (a shader that works in three dimensions, such as a ray
//! marcher, says where in space it is as it uses the value, and the value there is the mask at the place of a shape, a plane, a sphere or a cylinder, that is nearest to that point).
//! A shape is put in space by its position, how it is turned and how big it is (`docs/shader-masks-design.md`, the section on masks in space).
//!
//! A shape's frame is its own: the origin is its middle, **y is up the picture, and z is the front** (the way the picture faces, and the side it is read the right way round from). The
//! **turn** is how the shape is turned in space, as an aircraft is turned: first about its own front (z) by the *roll*, then about its own x by the *pitch*, then about the world's y by the
//! *yaw*. A shape that is turned by a yaw of 90 degrees has its front towards +x.

use crate::isf::{NUMBERS_OF_A_WORLD_PLACEMENT, Space, WorldShape};

/// How far from the origin a shape's middle may be, and how big a shape may be: the numbers of a call are single-precision floats, and a place that far out has no precision left to
/// read a picture at.
pub const LARGEST_DISTANCE_OF_A_SHAPE: f32 = 1.0e6;

/// The smallest width or height a shape may have: a shape of no size is not one.
pub const SMALLEST_SIZE_OF_A_SHAPE: f32 = 1.0e-4;

/// Where a mask is laid, what shape it is wrapped on, and how that shape is put in space. **It keeps all of that whichever shape is chosen** (the size of a sphere is not used, but is
/// kept for when the plane is chosen again).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldPlacement {
    pub shape: WorldShape,
    /// Where the middle of the shape is, in the units of the shader's space.
    pub position: [f32; 3],
    /// How the shape is turned, in degrees: the yaw (about the world's y), the pitch (about the shape's own x) and the roll (about the shape's own front, z): see the module's text.
    pub yaw_pitch_roll_degrees: [f32; 3],
    /// How wide a plane is (along its x). Not used by a sphere or a cylinder.
    pub width: f32,
    /// How tall a plane or a cylinder is (along its y). Not used by a sphere.
    pub height: f32,
}

impl Default for WorldPlacement {
    /// A plane at the middle of space, not turned, **3.2 wide and 3.2 tall** (a bit more than the sphere of radius 1.6 that the shaders of the program have their shapes in), so that a mask
    /// that is drawn over the whole of the picture covers what the shader shows. (The picture is stretched to the plane, which is square: set the height to 1.8 to keep a 16 by 9 frame's
    /// proportions.)
    fn default() -> Self {
        Self {
            shape: WorldShape::Plane,
            position: [0.0; 3],
            yaw_pitch_roll_degrees: [0.0; 3],
            width: 3.2,
            height: 3.2,
        }
    }
}

/// A quaternion `[x, y, z, w]`.
type Quaternion = [f32; 4];

fn product(first: Quaternion, second: Quaternion) -> Quaternion {
    let [ax, ay, az, aw] = first;
    let [bx, by, bz, bw] = second;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

/// The quaternion of a turn of `degrees` about the axis `axis` (a unit vector).
fn turn_about(axis: [f32; 3], degrees: f32) -> Quaternion {
    let half = degrees.to_radians() / 2.0;
    let (sine, cosine) = half.sin_cos();
    [axis[0] * sine, axis[1] * sine, axis[2] * sine, cosine]
}

impl WorldPlacement {
    /// Whether the numbers are ones a shape can be put in space with: all of them numbers, the middle not out of reach, and a size that is some.
    pub fn is_well_formed(&self) -> bool {
        let numbers_are_numbers = self
            .position
            .iter()
            .chain(&self.yaw_pitch_roll_degrees)
            .chain([&self.width, &self.height])
            .all(|number| number.is_finite());
        numbers_are_numbers
            && self
                .position
                .iter()
                .all(|distance| distance.abs() <= LARGEST_DISTANCE_OF_A_SHAPE)
            && [self.width, self.height]
                .iter()
                .all(|size| (SMALLEST_SIZE_OF_A_SHAPE..=LARGEST_DISTANCE_OF_A_SHAPE).contains(size))
    }

    /// The quaternion that turns a point of space into the frame of the shape: the opposite of the turn that the shape has in space (roll first, then pitch, then yaw), as the conjugate.
    pub fn quaternion_into_the_frame_of_the_shape(&self) -> [f32; 4] {
        let [yaw, pitch, roll] = self.yaw_pitch_roll_degrees;
        // (The shape's turn: the roll about its front first, then the pitch about its x, then the yaw about the world's y, so the quaternion is the yaw's times the pitch's times the roll's.)
        let turn = product(
            product(turn_about([0.0, 1.0, 0.0], yaw), turn_about([1.0, 0.0, 0.0], pitch)),
            turn_about([0.0, 0.0, 1.0], roll),
        );
        let [x, y, z, w] = turn;
        // (A turn of the whole shape the other way takes a point of space into its frame. Zero is not -0.)
        [0.0 - x, 0.0 - y, 0.0 - z, w]
    }

    /// The numbers of the call that the placement is, in the order the rewriting step reads them in: the middle (3), the quaternion into the frame of the shape (4), the width and the height.
    pub fn numbers(&self) -> [f32; NUMBERS_OF_A_WORLD_PLACEMENT] {
        let [x, y, z] = self.position;
        let [qx, qy, qz, qw] = self.quaternion_into_the_frame_of_the_shape();
        [x, y, z, qx, qy, qz, qw, self.width, self.height]
    }

    /// The same, wrapped on `shape`.
    #[must_use]
    pub fn with_the_shape(self, shape: WorldShape) -> Self {
        Self { shape, ..self }
    }
}

/// Where the mask of a driven value is laid.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum MaskSpace {
    /// Over the picture: the value at a pixel is read from the mask at that pixel. What every value is until it is said otherwise.
    #[default]
    Screen,
    /// In space: the value, at each point of space the shader says it is working at, is read from the mask at the place of the shape that is nearest to that point.
    World(WorldPlacement),
}

impl MaskSpace {
    /// How the shader that reads the value is made: for a mask over the picture, or for one on this shape.
    pub fn for_the_shader(self) -> Space {
        match self {
            Self::Screen => Space::Screen,
            Self::World(placement) => Space::World(placement.shape),
        }
    }

    /// The numbers of a call that the place in space is, if it is in space.
    pub fn numbers_of_the_call(self) -> Option<[f32; NUMBERS_OF_A_WORLD_PLACEMENT]> {
        match self {
            Self::Screen => None,
            Self::World(placement) => Some(placement.numbers()),
        }
    }

    /// Whether the mask is laid in space.
    pub fn is_in_space(self) -> bool {
        matches!(self, Self::World(_))
    }

    /// The placement it has, or `to_begin_with` if it is over the picture.
    pub fn placement_or(self, to_begin_with: WorldPlacement) -> WorldPlacement {
        match self {
            Self::Screen => to_begin_with,
            Self::World(placement) => placement,
        }
    }

    /// Whether the numbers are ones that can be kept.
    pub fn is_well_formed(self) -> bool {
        match self {
            Self::Screen => true,
            Self::World(placement) => placement.is_well_formed(),
        }
    }
}

#[cfg(test)]
mod tests;
