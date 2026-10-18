// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `.cube` host (PH-01 to PH-05): reads the look-up tables that people send as `.cube` files, 1-D and 3-D, and says exactly what is wrong with one it cannot read.
//!
//! **What is read.** The format that Adobe wrote down (Cube LUT Specification 1.0) and that DaVinci Resolve, ffmpeg and most colour tools write: lines that are blank or begin with
//! `#` are skipped; `TITLE "name"`; `DOMAIN_MIN r g b` and `DOMAIN_MAX r g b` (the range of the input that the table covers, 0 to 1 if not said); `LUT_1D_SIZE n` or
//! `LUT_3D_SIZE n`; then the table: one row of three numbers (the red, green and blue that the colour becomes) for each point, **red changing fastest**, then green, then blue,
//! for a 3-D table. **The DaVinci Resolve variant is read, and is read only**: `LUT_1D_INPUT_RANGE min max` and `LUT_3D_INPUT_RANGE min max` are the domain, the same for all three
//! channels. A table of both kinds in one file is refused (nothing in the format says how they combine), as is any line that is not one of these.
//!
//! **What is refused, and said with the number of the line:** a size that is not what the rows say, a number that is not one (or is not finite), a domain that is not a range,
//! a keyword after the table has begun, a line that is none of the above. **A 3-D table of more than [`MOST_POINTS_A_SIDE_FOR_NOW`] points a side is refused for now**
//! (F15: a 65-point cube is 275 thousand rows, which is what the programs that people use make; the limit is this constant and the message that says so).
//!
//! A parsed table is [`CubeTable`]. It also knows how to look up a colour in itself on the processor ([`CubeTable::look_up`]), the way the card does, which is how the card's
//! pass is held to it (and both to ffmpeg's `lut3d` filter).

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

/// The most points a side that a 3-D table may have, for now.
pub const MOST_POINTS_A_SIDE_FOR_NOW: usize = 65;

/// The most points a 1-D table may have.
pub const MOST_POINTS_OF_A_1D_TABLE: usize = 65_536;

/// The fewest points a table may have on a side.
pub const FEWEST_POINTS: usize = 2;

/// The numbers of the tables, so that the engine can tell one table from another without looking at the points.
static NEXT_SERIAL_NUMBER: AtomicU64 = AtomicU64::new(1);

/// How a table's points are laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CubeShape {
    /// One output colour for each of this many input levels (the same input level for red, green and blue: each channel is looked up in its own column).
    OneDimensional { points: usize },
    /// One output colour for each of this many points a side of the cube of input colours.
    ThreeDimensional { points_a_side: usize },
}

impl CubeShape {
    /// How many rows the table has.
    pub fn rows(self) -> usize {
        match self {
            Self::OneDimensional { points } => points,
            Self::ThreeDimensional { points_a_side } => points_a_side * points_a_side * points_a_side,
        }
    }
}

/// How to look a colour up between the points of a 3-D table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Interpolation {
    /// By the four corners of the tetrahedron of the little cube that the colour is in: what colourists' programs and ffmpeg do, and the default.
    #[default]
    Tetrahedral,
    /// By all eight corners of the little cube.
    Trilinear,
}

impl Interpolation {
    pub const ALL: [Self; 2] = [Self::Tetrahedral, Self::Trilinear];

    /// What names it in a project file, and in the choice of the effect that uses it.
    pub fn id(self) -> &'static str {
        match self {
            Self::Tetrahedral => "tetrahedral",
            Self::Trilinear => "trilinear",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|way| way.id() == id)
    }
}

/// A look-up table that was read from a `.cube` file.
#[derive(Clone, Debug)]
pub struct CubeTable {
    title: String,
    domain_min: [f32; 3],
    domain_max: [f32; 3],
    shape: CubeShape,
    /// The rows of the table: the red, green and blue that each point becomes, in the order of the file (for a 3-D table, red changes fastest, then green, then blue).
    rows: Vec<[f32; 3]>,
    /// What tells this table from every other one that was read (not from one that was read from the same bytes: it is a count of the tables read in this run).
    serial_number: u64,
}

/// Two tables are equal when they have the same title, domain, shape and rows (the serial number does not count).
impl PartialEq for CubeTable {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title
            && self.domain_min == other.domain_min
            && self.domain_max == other.domain_max
            && self.shape == other.shape
            && self.rows == other.rows
    }
}

impl CubeTable {
    /// A table made of `rows` (for the tests, which make tables by formula; a table from a file is [`parse_cube`]). It is an error if the rows are not as many as the shape needs
    /// or the domain is not a range.
    pub fn new(
        title: &str,
        domain_min: [f32; 3],
        domain_max: [f32; 3],
        shape: CubeShape,
        rows: Vec<[f32; 3]>,
    ) -> Result<Self, String> {
        if rows.len() != shape.rows() {
            return Err(format!(
                "{shape:?} needs {} rows, and there are {}",
                shape.rows(),
                rows.len()
            ));
        }
        if let Some(problem) = problem_with_the_domain(domain_min, domain_max) {
            return Err(problem);
        }
        Ok(Self {
            title: title.to_owned(),
            domain_min,
            domain_max,
            shape,
            rows,
            serial_number: NEXT_SERIAL_NUMBER.fetch_add(1, Ordering::Relaxed),
        })
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn domain_min(&self) -> [f32; 3] {
        self.domain_min
    }

    pub fn domain_max(&self) -> [f32; 3] {
        self.domain_max
    }

    pub fn shape(&self) -> CubeShape {
        self.shape
    }

    /// The rows of the table, in the order of the file.
    pub fn rows(&self) -> &[[f32; 3]] {
        &self.rows
    }

    /// What tells this table from every other one read in this run.
    pub fn serial_number(&self) -> u64 {
        self.serial_number
    }

    /// What the colour `rgb` (as the table takes it, so in the domain; a colour outside it is taken for the nearest one in it) becomes, looked up the way the card does it:
    /// a 1-D table by linear interpolation in each channel's own column, a 3-D table as `interpolation` says.
    pub fn look_up(&self, rgb: [f32; 3], interpolation: Interpolation) -> [f32; 3] {
        let scaled = |channel: usize, points: usize| {
            let (least, most) = (self.domain_min[channel], self.domain_max[channel]);
            ((rgb[channel].clamp(least, most) - least) / (most - least)) * (points - 1) as f32
        };
        match self.shape {
            CubeShape::OneDimensional { points } => [0, 1, 2].map(|channel| {
                let at = scaled(channel, points);
                let below = (at.floor() as usize).min(points - 1);
                let above = (below + 1).min(points - 1);
                let fraction = at - below as f32;
                self.rows[below][channel] * (1.0 - fraction) + self.rows[above][channel] * fraction
            }),
            CubeShape::ThreeDimensional { points_a_side } => {
                let at = [0, 1, 2].map(|channel| scaled(channel, points_a_side));
                let below = at.map(|number| (number.floor() as usize).min(points_a_side - 1));
                let above = below.map(|number| (number + 1).min(points_a_side - 1));
                let fraction = [0, 1, 2].map(|channel| at[channel] - below[channel] as f32);
                let row = |red: usize, green: usize, blue: usize| {
                    self.rows[(blue * points_a_side + green) * points_a_side + red]
                };
                match interpolation {
                    Interpolation::Trilinear => trilinear(row, below, above, fraction),
                    Interpolation::Tetrahedral => tetrahedral(row, below, above, fraction),
                }
            }
        }
    }
}

/// The colour between the eight corners of the little cube from `below` to `above`, mixed by all of them.
fn trilinear(
    row: impl Fn(usize, usize, usize) -> [f32; 3],
    below: [usize; 3],
    above: [usize; 3],
    d: [f32; 3],
) -> [f32; 3] {
    let corner = |red: bool, green: bool, blue: bool| {
        row(
            if red { above[0] } else { below[0] },
            if green { above[1] } else { below[1] },
            if blue { above[2] } else { below[2] },
        )
    };
    let mix = |a: [f32; 3], b: [f32; 3], fraction: f32| {
        [0, 1, 2].map(|place| a[place] * (1.0 - fraction) + b[place] * fraction)
    };
    let c00 = mix(corner(false, false, false), corner(true, false, false), d[0]);
    let c10 = mix(corner(false, true, false), corner(true, true, false), d[0]);
    let c01 = mix(corner(false, false, true), corner(true, false, true), d[0]);
    let c11 = mix(corner(false, true, true), corner(true, true, true), d[0]);
    mix(mix(c00, c10, d[1]), mix(c01, c11, d[1]), d[2])
}

/// The colour between the corners of the little cube, by the four corners of the tetrahedron that it is in (the six tetrahedra of the cube share the diagonal from the
/// corner `below` to the corner `above`): which one it is in is told by which of the three fractions is the largest, and which the smallest.
fn tetrahedral(
    row: impl Fn(usize, usize, usize) -> [f32; 3],
    below: [usize; 3],
    above: [usize; 3],
    d: [f32; 3],
) -> [f32; 3] {
    let c000 = row(below[0], below[1], below[2]);
    let c111 = row(above[0], above[1], above[2]);
    let c100 = row(above[0], below[1], below[2]);
    let c010 = row(below[0], above[1], below[2]);
    let c001 = row(below[0], below[1], above[2]);
    let c110 = row(above[0], above[1], below[2]);
    let c101 = row(above[0], below[1], above[2]);
    let c011 = row(below[0], above[1], above[2]);
    // The weights of the four corners of the tetrahedron: the first and the last corner are always there; the two between them are the ones that the largest and then the
    // second largest fraction step to.
    let (first, second, third, weights): ([f32; 3], [f32; 3], [f32; 3], [f32; 4]) = if d[0] > d[1] {
        if d[1] > d[2] {
            (c100, c110, c111, [1.0 - d[0], d[0] - d[1], d[1] - d[2], d[2]])
        } else if d[0] > d[2] {
            (c100, c101, c111, [1.0 - d[0], d[0] - d[2], d[2] - d[1], d[1]])
        } else {
            (c001, c101, c111, [1.0 - d[2], d[2] - d[0], d[0] - d[1], d[1]])
        }
    } else if d[2] > d[1] {
        (c001, c011, c111, [1.0 - d[2], d[2] - d[1], d[1] - d[0], d[0]])
    } else if d[2] > d[0] {
        (c010, c011, c111, [1.0 - d[1], d[1] - d[2], d[2] - d[0], d[0]])
    } else {
        (c010, c110, c111, [1.0 - d[1], d[1] - d[0], d[0] - d[2], d[2]])
    };
    [0, 1, 2].map(|place| {
        weights[0] * c000[place] + weights[1] * first[place] + weights[2] * second[place] + weights[3] * third[place]
    })
}

/// What is wrong with `.cube` text, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CubeProblem {
    /// The number of the line, counted from 1; 0 for a problem with the file as a whole.
    pub line: usize,
    pub message: String,
}

impl fmt::Display for CubeProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(formatter, "{}", self.message)
        } else {
            write!(formatter, "line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for CubeProblem {}

fn problem(line: usize, message: impl Into<String>) -> CubeProblem {
    CubeProblem {
        line,
        message: message.into(),
    }
}

/// Why `min` and `max` are not a range for each channel, if they are not.
fn problem_with_the_domain(min: [f32; 3], max: [f32; 3]) -> Option<String> {
    (0..3).find_map(|channel| {
        let (least, most) = (min[channel], max[channel]);
        (!(least.is_finite() && most.is_finite() && least < most)).then(|| {
            format!(
                "the domain of the {} is from {least} to {most}, which is not a range",
                ["red", "green", "blue"][channel]
            )
        })
    })
}

/// Reads the text of a `.cube` file.
pub fn parse_cube(text: &str) -> Result<CubeTable, CubeProblem> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut title = String::new();
    let mut domain_min = [0.0_f32; 3];
    let mut domain_max = [1.0_f32; 3];
    let mut size_1d: Option<(usize, usize)> = None;
    let mut size_3d: Option<(usize, usize)> = None;
    let mut rows: Vec<[f32; 3]> = Vec::new();
    let mut first_row_line: Option<usize> = None;
    for (index, raw_line) in text.lines().enumerate() {
        let number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let starts_a_number = line
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_digit() || matches!(first, '-' | '+' | '.'));
        if starts_a_number {
            first_row_line.get_or_insert(number);
            let shape = size_3d.or(size_1d);
            let Some((_, wanted_rows)) = shape else {
                return Err(problem(
                    number,
                    "a row of numbers comes before the size of the table (LUT_1D_SIZE or LUT_3D_SIZE)",
                ));
            };
            if rows.len() == wanted_rows {
                return Err(problem(
                    number,
                    format!("there are more rows than the {wanted_rows} that the size says"),
                ));
            }
            rows.push(three_numbers(line, number, "a row of the table")?);
            continue;
        }
        let (keyword, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let rest = rest.trim();
        if let Some(first_row) = first_row_line {
            return Err(problem(
                number,
                format!("\"{keyword}\" comes after the table has begun (at line {first_row})"),
            ));
        }
        match keyword {
            "TITLE" => title = rest.trim_matches('"').to_owned(),
            "DOMAIN_MIN" => domain_min = three_numbers(rest, number, "DOMAIN_MIN")?,
            "DOMAIN_MAX" => domain_max = three_numbers(rest, number, "DOMAIN_MAX")?,
            "LUT_1D_INPUT_RANGE" | "LUT_3D_INPUT_RANGE" => {
                let [least, most] = two_numbers(rest, number, keyword)?;
                domain_min = [least; 3];
                domain_max = [most; 3];
            }
            "LUT_1D_SIZE" => {
                if size_1d.is_some() {
                    return Err(problem(number, "LUT_1D_SIZE is said twice"));
                }
                let points = size(rest, number, "LUT_1D_SIZE", FEWEST_POINTS, MOST_POINTS_OF_A_1D_TABLE)?;
                size_1d = Some((points, points));
            }
            "LUT_3D_SIZE" => {
                if size_3d.is_some() {
                    return Err(problem(number, "LUT_3D_SIZE is said twice"));
                }
                let points = size(rest, number, "LUT_3D_SIZE", FEWEST_POINTS, 256)?;
                if points > MOST_POINTS_A_SIDE_FOR_NOW {
                    return Err(problem(
                        number,
                        format!(
                            "a table of {points} points a side is not used yet: the most is {MOST_POINTS_A_SIDE_FOR_NOW} (a table of that size has {} rows)",
                            MOST_POINTS_A_SIDE_FOR_NOW.pow(3)
                        ),
                    ));
                }
                size_3d = Some((points, points * points * points));
            }
            other => {
                return Err(problem(
                    number,
                    format!("\"{other}\" is not something that a .cube file says"),
                ));
            }
        }
    }
    let shape = match (size_1d, size_3d) {
        (Some(_), Some(_)) => {
            return Err(problem(
                0,
                "the file has a 1-D table and a 3-D table; a file with both is not used (nothing says how they combine)",
            ));
        }
        (Some((points, _)), None) => CubeShape::OneDimensional { points },
        (None, Some((points_a_side, _))) => CubeShape::ThreeDimensional { points_a_side },
        (None, None) => {
            return Err(problem(
                0,
                "the file does not say how big the table is (LUT_1D_SIZE or LUT_3D_SIZE)",
            ));
        }
    };
    if rows.len() != shape.rows() {
        let (kind, size) = match shape {
            CubeShape::OneDimensional { points } => ("LUT_1D_SIZE", points),
            CubeShape::ThreeDimensional { points_a_side } => ("LUT_3D_SIZE", points_a_side),
        };
        return Err(problem(
            0,
            format!(
                "{kind} {size} needs {} rows, and the file has {}",
                shape.rows(),
                rows.len()
            ),
        ));
    }
    if let Some(message) = problem_with_the_domain(domain_min, domain_max) {
        return Err(problem(0, message));
    }
    CubeTable::new(&title, domain_min, domain_max, shape, rows).map_err(|message| problem(0, message))
}

/// The three numbers on a line, after the word that begins it.
fn three_numbers(text: &str, line: usize, what: &str) -> Result<[f32; 3], CubeProblem> {
    let mut parts = text.split_whitespace();
    let mut numbers = [0.0_f32; 3];
    for number in &mut numbers {
        let part = parts
            .next()
            .ok_or_else(|| problem(line, format!("{what} needs three numbers")))?;
        *number = finite_number(part, line)?;
    }
    if let Some(extra) = parts.next() {
        return Err(problem(
            line,
            format!("{what} needs three numbers, and there is more: \"{extra}\""),
        ));
    }
    Ok(numbers)
}

fn two_numbers(text: &str, line: usize, what: &str) -> Result<[f32; 2], CubeProblem> {
    let parts: Vec<&str> = text.split_whitespace().collect();
    let [first, second] = parts[..] else {
        return Err(problem(line, format!("{what} needs two numbers")));
    };
    Ok([finite_number(first, line)?, finite_number(second, line)?])
}

fn finite_number(part: &str, line: usize) -> Result<f32, CubeProblem> {
    let number: f32 = part
        .parse()
        .map_err(|_| problem(line, format!("\"{part}\" is not a number")))?;
    if number.is_finite() {
        Ok(number)
    } else {
        Err(problem(
            line,
            format!("\"{part}\" is not a number that a table can have"),
        ))
    }
}

fn size(text: &str, line: usize, what: &str, least: usize, most: usize) -> Result<usize, CubeProblem> {
    let points: usize = text.trim().parse().map_err(|_| {
        problem(
            line,
            format!("{what} needs a whole number, and \"{}\" is not one", text.trim()),
        )
    })?;
    if (least..=most).contains(&points) {
        Ok(points)
    } else {
        Err(problem(
            line,
            format!("{what} is {points}, and a table has from {least} to {most} points"),
        ))
    }
}

#[cfg(test)]
mod tests;
