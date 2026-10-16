// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! 3 by 3 matrices of `f64`, which is all that the conversions between colour spaces need. They are made in `f64` (so that a chain of them keeps its precision) and
//! given to the graphics card as `f32`.

/// A matrix by rows: `matrix[row][column]`. It multiplies a column of red, green and blue (or X, Y and Z).
pub type Matrix3 = [[f64; 3]; 3];

pub const IDENTITY: Matrix3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// `left * right`: the matrix that does `right` first and then `left`.
pub fn multiply(left: &Matrix3, right: &Matrix3) -> Matrix3 {
    let mut product = [[0.0; 3]; 3];
    for (row, product_row) in product.iter_mut().enumerate() {
        for (column, number) in product_row.iter_mut().enumerate() {
            *number = (0..3).map(|k| left[row][k] * right[k][column]).sum();
        }
    }
    product
}

/// `matrix * vector`.
pub fn apply(matrix: &Matrix3, vector: [f64; 3]) -> [f64; 3] {
    matrix.map(|row| row[0] * vector[0] + row[1] * vector[1] + row[2] * vector[2])
}

/// How small the determinant of a matrix may be, beside the cube of its biggest number, before it is taken to be 0: the determinant of a matrix whose rows are
/// alike in a computer is a few last digits of rounding and not 0, and its inverse is then numbers of a size that is no use to anyone.
const SMALLEST_DETERMINANT_BESIDE_THE_SIZE_OF_THE_MATRIX: f64 = 1e-12;

/// The matrix that undoes `matrix`, or `None` if it cannot be undone (its determinant is 0 or so near it that rounding decides, or is not a number).
pub fn inverse(matrix: &Matrix3) -> Option<Matrix3> {
    let [[a, b, c], [d, e, f], [g, h, i]] = *matrix;
    let determinant = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    let biggest_number = matrix
        .iter()
        .flatten()
        .fold(0.0_f64, |biggest, number| biggest.max(number.abs()));
    if !determinant.is_finite()
        || determinant.abs() <= SMALLEST_DETERMINANT_BESIDE_THE_SIZE_OF_THE_MATRIX * biggest_number.powi(3)
    {
        return None;
    }
    Some([
        [
            (e * i - f * h) / determinant,
            (c * h - b * i) / determinant,
            (b * f - c * e) / determinant,
        ],
        [
            (f * g - d * i) / determinant,
            (a * i - c * g) / determinant,
            (c * d - a * f) / determinant,
        ],
        [
            (d * h - e * g) / determinant,
            (b * g - a * h) / determinant,
            (a * e - b * d) / determinant,
        ],
    ])
}

/// The same matrix in `f32`, which is what a graphics card is given.
pub fn to_f32(matrix: &Matrix3) -> [[f32; 3]; 3] {
    matrix.map(|row| row.map(|number| number as f32))
}

/// The same matrix as a diagonal one made of `diagonal`.
pub fn diagonal(diagonal: [f64; 3]) -> Matrix3 {
    [
        [diagonal[0], 0.0, 0.0],
        [0.0, diagonal[1], 0.0],
        [0.0, 0.0, diagonal[2]],
    ]
}
