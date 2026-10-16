// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Colour spaces, as numbers (phase 3 of the plan of action; the design is `docs/engine-phase-3-design.md`): the sets of primaries that the program knows, the curves
//! between codes and light, and the matrices that turn the light of one set into the light of another. **This is the reference implementation** (CM-08): it is written in
//! `f64`, tested against published numbers and by round trips, and the graphics card's version of it (`engine/shaders/colour.wgsl`) is held to it by tests.
//!
//! A colour space is data (CM-15): [`StandardPrimaries`] is the table of sets of primaries, [`TransferFunction`] the list of curves. What a conversion is made of
//! is the same everywhere: decode the codes to light with the curve of the source, multiply by [`StandardPrimaries::conversion_to`] to have the light of the target's
//! primaries, encode with the curve of the target.

pub mod matrix;
mod primaries;
mod table;
mod transfer;
mod video;

pub use primaries::{ACES_WHITE, Chromaticity, D65, Primaries, bradford_adaptation};
pub use table::StandardPrimaries;
pub use transfer::TransferFunction;
pub use video::{ChromaSiting, VideoMatrix, VideoRange};

#[cfg(test)]
mod tests;
