// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a set of red, green and blue is, as colours that light can have: three primaries and a white point, as CIE xy chromaticities; and the matrices that follow
//! from them (red, green and blue of such a set to the X, Y and Z of the standard observer, and from one white to another).

use super::matrix::{self, Matrix3};

/// A colour of light, without how bright it is: CIE 1931 xy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chromaticity {
    pub x: f64,
    pub y: f64,
}

impl Chromaticity {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// The X, Y and Z of this colour at a brightness (Y) of 1.
    pub fn xyz(self) -> [f64; 3] {
        [self.x / self.y, 1.0, (1.0 - self.x - self.y) / self.y]
    }
}

/// The white of daylight at 6504 K, which the displays and the video standards use (D65).
pub const D65: Chromaticity = Chromaticity::new(0.3127, 0.3290);

/// The white of the ACES colour spaces (about 6000 K, a little off the daylight line).
pub const ACES_WHITE: Chromaticity = Chromaticity::new(0.32168, 0.33767);

/// A set of red, green and blue primaries and the white that equal amounts of them make.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Primaries {
    pub red: Chromaticity,
    pub green: Chromaticity,
    pub blue: Chromaticity,
    pub white: Chromaticity,
}

impl Primaries {
    /// The matrix that makes X, Y and Z of the light that red, green and blue (each from 0 to 1, in light, not in code) of this set stand for: the columns are the
    /// primaries' own X, Y, Z, each scaled so that red, green and blue of 1 together are the white at a brightness of 1.
    ///
    /// `None` if the primaries lie on one line (they make no colours), which no set of the table does.
    pub fn rgb_to_xyz(&self) -> Option<Matrix3> {
        let columns = [self.red.xyz(), self.green.xyz(), self.blue.xyz()];
        let unscaled: Matrix3 = std::array::from_fn(|row| std::array::from_fn(|column| columns[column][row]));
        let scales = matrix::apply(&matrix::inverse(&unscaled)?, self.white.xyz());
        Some(matrix::multiply(&unscaled, &matrix::diagonal(scales)))
    }

    /// The matrix the other way: from X, Y and Z to red, green and blue of this set.
    pub fn xyz_to_rgb(&self) -> Option<Matrix3> {
        matrix::inverse(&self.rgb_to_xyz()?)
    }
}

/// The cone response matrix of the Bradford transform, which turns X, Y and Z into the responses that the adaptation of the eye is worked out in.
const BRADFORD: Matrix3 = [
    [0.8951, 0.2664, -0.1614],
    [-0.7502, 1.7135, 0.0367],
    [0.0389, -0.0685, 1.0296],
];

/// The matrix, on X, Y and Z, that makes a scene lit by the white `from` look as it would lit by the white `to` (the Bradford transform). **The identity when the two
/// whites are the same**, which is every pair of the table but the ones that have an ACES space in them.
pub fn bradford_adaptation(from: Chromaticity, to: Chromaticity) -> Matrix3 {
    if from == to {
        return matrix::IDENTITY;
    }
    let responses_of_from = matrix::apply(&BRADFORD, from.xyz());
    let responses_of_to = matrix::apply(&BRADFORD, to.xyz());
    let scales = std::array::from_fn(|channel| responses_of_to[channel] / responses_of_from[channel]);
    let inverse_bradford = matrix::inverse(&BRADFORD).expect("the Bradford matrix can be undone");
    matrix::multiply(
        &inverse_bradford,
        &matrix::multiply(&matrix::diagonal(scales), &BRADFORD),
    )
}
