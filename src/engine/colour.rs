// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the numbers of the engine's pictures mean (phase 3 of the plan of action; the design is `docs/engine-phase-3-design.md`): the **working space** that the layers
//! are put together in, how the numbers of a **picture that is sent** are to be read, and what **the finished frame** is encoded as; and the numbers that the passes of
//! the card are given to do the conversions between them.
//!
//! There are two working spaces. [`WorkingSpace::DisplayReferred`] is how the engine began (the compatibility mode of CM-20): every number is a code divided by 255, nothing is
//! converted, and layers are mixed in codes, as the window's own drawing mixes them. It is not a setting, and has no way to be asked for in the program: it is what the tests
//! that compare the engine with the old drawing run in, and what a frame is described in when nothing says otherwise. [`WorkingSpace::Linear`] is the colour-managed one: the
//! numbers are **light**, in the primaries of a set of the table, and mixing is physics.

use serde::{Deserialize, Serialize};

use super::uniform_bytes::SlotWriter;
use crate::colour_space::{StandardPrimaries, TransferFunction, matrix};

/// What the numbers in the engine's pictures, and in the layers put together, are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkingSpace {
    /// Codes (the numbers of a picture file or of what a video's matrix makes, divided by 255), and nothing is converted: the engine as it was in phases 1 and 2.
    #[default]
    DisplayReferred,
    /// Light, in the primaries of this set.
    Linear(StandardPrimaries),
}

impl WorkingSpace {
    /// The light of `primaries` in the numbers of this working space, as a matrix on light (the identity for the compatibility mode, which has no primaries).
    fn matrix_from(self, primaries: StandardPrimaries) -> matrix::Matrix3 {
        match self {
            Self::DisplayReferred => matrix::IDENTITY,
            Self::Linear(working) => primaries.conversion_to(working),
        }
    }

    /// The matrix on light that takes the numbers of this working space to the light of `primaries` (the identity for the compatibility mode).
    fn matrix_to(self, primaries: StandardPrimaries) -> matrix::Matrix3 {
        match self {
            Self::DisplayReferred => matrix::IDENTITY,
            Self::Linear(working) => working.conversion_to(primaries),
        }
    }
}

/// How the numbers of a picture that is sent are to be read: the primaries of its colours and the curve of its codes. (The codes of a video picture are the red, green and
/// blue that its matrix makes; see [`super::YuvColour`].) A picture file, a text and a symbol are sRGB.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PictureColour {
    pub primaries: StandardPrimaries,
    pub curve: TransferFunction,
}

impl PictureColour {
    /// The colour of a picture file: sRGB, which has the primaries of Rec.709.
    pub const SRGB: Self = Self {
        primaries: StandardPrimaries::Rec709,
        curve: TransferFunction::Srgb,
    };
}

impl Default for PictureColour {
    fn default() -> Self {
        Self::SRGB
    }
}

/// What the finished frame is encoded as: the primaries and the curve that its numbers are made with. For the preview it is what the display wants; for the export, the
/// output space. (Not used in the compatibility mode, where the numbers are shown as they are.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OutputEncoding {
    pub primaries: StandardPrimaries,
    pub curve: TransferFunction,
}

impl OutputEncoding {
    /// A display that shows sRGB.
    pub const SRGB: Self = Self {
        primaries: StandardPrimaries::Rec709,
        curve: TransferFunction::Srgb,
    };
}

impl Default for OutputEncoding {
    fn default() -> Self {
        Self::SRGB
    }
}

/// The colour of a frame: what its layers are put together in, and what the frame is encoded as.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FrameColour {
    pub working: WorkingSpace,
    pub output: OutputEncoding,
}

/// The numbers of the pass that makes a picture of bytes into one that the frames work on (`ingest.wgsl`): the conversion of the light of the picture's primaries to the
/// working space's, and the curve that its codes are decoded with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IngestConversion {
    pub to_working: [[f32; 3]; 3],
    pub curve: u32,
}

impl IngestConversion {
    /// The conversion of a picture whose colours are `colour` into `working`. **In the compatibility mode it does nothing**: the identity, and the curve that is not one.
    pub fn of(colour: PictureColour, working: WorkingSpace) -> Self {
        match working {
            WorkingSpace::DisplayReferred => Self {
                to_working: matrix::to_f32(&matrix::IDENTITY),
                curve: TransferFunction::Linear.number_for_the_shaders(),
            },
            WorkingSpace::Linear(_) => Self {
                to_working: matrix::to_f32(&working.matrix_from(colour.primaries)),
                curve: colour.curve.number_for_the_shaders(),
            },
        }
    }

    /// Writes the slot, in the order of `struct Ingest` in `ingest.wgsl`.
    pub fn write(&self, writer: &mut SlotWriter) {
        writer.mat3x3_from_rows(self.to_working).u32(self.curve);
    }
}

/// The numbers of the effect that mixes colours (`colour_matrix.wgsl`) that are about the colour domain: the conversions of the light of the working space to the codes
/// of the display-referred domain (sRGB codes, Rec.709 primaries) and back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayDomainConversion {
    pub into_domain: [[f32; 3]; 3],
    pub out_of_domain: [[f32; 3]; 3],
    pub curve: u32,
}

impl DisplayDomainConversion {
    /// The conversions for `working`. **In the compatibility mode they do nothing**, because the numbers already are the codes of the domain.
    pub fn of(working: WorkingSpace) -> Self {
        match working {
            WorkingSpace::DisplayReferred => Self {
                into_domain: matrix::to_f32(&matrix::IDENTITY),
                out_of_domain: matrix::to_f32(&matrix::IDENTITY),
                curve: TransferFunction::Linear.number_for_the_shaders(),
            },
            WorkingSpace::Linear(_) => Self {
                into_domain: matrix::to_f32(&working.matrix_to(StandardPrimaries::Rec709)),
                out_of_domain: matrix::to_f32(&working.matrix_from(StandardPrimaries::Rec709)),
                curve: TransferFunction::Srgb.number_for_the_shaders(),
            },
        }
    }

    /// Writes the numbers that follow the mixing matrix in the slot, in the order of `struct Mixing` in `colour_matrix.wgsl`.
    pub fn write(&self, writer: &mut SlotWriter) {
        writer
            .mat3x3_from_rows(self.into_domain)
            .mat3x3_from_rows(self.out_of_domain)
            .u32(self.curve);
    }
}

/// The numbers of the pass that makes the frame: the conversion of the working space's light to the output's primaries, and the curve that the output's codes are made with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutputConversion {
    pub to_output: [[f32; 3]; 3],
    pub curve: u32,
}

impl OutputConversion {
    /// The conversion of the light of `working` into `colour.output`. **In the compatibility mode it does nothing.**
    pub fn of(colour: FrameColour) -> Self {
        match colour.working {
            WorkingSpace::DisplayReferred => Self {
                to_output: matrix::to_f32(&matrix::IDENTITY),
                curve: TransferFunction::Linear.number_for_the_shaders(),
            },
            WorkingSpace::Linear(_) => Self {
                to_output: matrix::to_f32(&colour.working.matrix_to(colour.output.primaries)),
                curve: colour.output.curve.number_for_the_shaders(),
            },
        }
    }

    /// Writes the slot, in the order of `struct Output` in `output.wgsl`.
    pub fn write(&self, writer: &mut SlotWriter) {
        writer.mat3x3_from_rows(self.to_output).u32(self.curve);
    }
}

/// The colour domain that an effect works in (CM-10): the numbers it is given are the light of the working space, or **codes of the display** (sRGB-encoded numbers with
/// the primaries of Rec.709, which is what the hue shift, and nearly every shader that was written for a screen, expect).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColourDomain {
    SceneLinear,
    DisplayReferred,
}
