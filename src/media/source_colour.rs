// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the colours of a video are: **what its file says, and what is assumed where it says nothing** (CM-03; the design is `docs/engine-phase-3-design.md`, section 5). One
//! function reads the four tags that ffprobe gives (`VideoStreamFormat`) and the size of the picture, and the preview, the export and the command line all use it, so that they
//! cannot disagree about what a file's numbers stand for.
//!
//! **The rules** (decisions C6 and C7 of the design):
//!
//! * **The matrix** is what the file says (`bt709`; `bt470bg` and `smpte170m`, which are BT.601; `bt2020nc`). A file that says nothing gets **BT.709 if its picture is more
//!   than 1024 across or more than 576 down, and BT.601 if it is not** (the rule that players use for high-definition and standard-definition video). It used to be BT.601
//!   for every size, because that is what ffmpeg does when it makes the colours itself.
//! * **The range** is limited unless the file says it is the full one (`pc`, `jpeg`).
//! * **The primaries** are what the file says where it says a set that the program has, and Rec.709's otherwise.
//! * **The curve**: a file that says `bt709` (or the transfer of any of the video standards of standard dynamic range) or says nothing is read as **the curve that a screen
//!   shows it with: sRGB's**, because a picture that is tagged so is nearly always a recording of a screen, or made to be seen on one, and every player shows it that way (so
//!   that a video that is only shown comes out with the numbers it had). It is **not** read as the camera curve of BT.709, which would lift every shadow. The two
//!   curves for high dynamic range (`smpte2084`, `arib-std-b67`) are not converted in this phase and are read as sRGB's, and the notes say so.
//!
//! Everything that was assumed, and everything that is not what the file says, is in [`VideoColour::assumptions`], in words that the dialog of a source shows.

use super::source_probe::VideoStreamFormat;
use crate::colour_space::{StandardPrimaries, TransferFunction, VideoMatrix, VideoRange};
use crate::project::InputColourOverride;

/// How wide a picture may be, and how tall, to be taken for one of standard definition when its file does not say which matrix it uses.
pub const WIDEST_PICTURE_OF_STANDARD_DEFINITION: u32 = 1024;
pub const TALLEST_PICTURE_OF_STANDARD_DEFINITION: u32 = 576;

/// What the colours of a video are, and what had to be assumed to say so.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoColour {
    /// The matrix that makes red, green and blue of brightness and the colour differences, or `None` for a matrix that the program cannot make colours with (ffmpeg
    /// makes the colours of such a video itself).
    pub matrix: Option<VideoMatrix>,
    /// Whether the numbers use all of 0 to 255, or (as nearly all video does) 16 to 235 and 16 to 240. `None` for what the file says that is not one of the two.
    pub full_range: Option<bool>,
    /// The primaries of the colours.
    pub primaries: StandardPrimaries,
    /// The curve that the codes of the red, green and blue are made light with.
    pub curve: TransferFunction,
    /// What was assumed, or is not what the file says, in the order the four things above are.
    pub assumptions: Vec<Assumption>,
}

/// A thing that was assumed in reading the colours of a video.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Assumption {
    /// The file does not say its matrix: it is `assumed` because of the size of the picture.
    MatrixNotSaid {
        assumed: VideoMatrix,
        width: u32,
        height: u32,
    },
    /// The file says a matrix that the program has not got.
    MatrixNotKnown { said: String },
    /// The file does not say its range: it is the limited one.
    RangeNotSaid,
    /// The file says a range that is neither of the two.
    RangeNotKnown { said: String },
    /// The file does not say its primaries: they are Rec.709's.
    PrimariesNotSaid,
    /// The file says primaries that the program has not got: they are read as Rec.709's.
    PrimariesNotKnown { said: String },
    /// The file does not say its transfer: it is read as sRGB's.
    TransferNotSaid,
    /// The file says a transfer for high dynamic range, which is not converted yet: it is read as sRGB's.
    TransferIsForHighDynamicRange { said: String },
    /// The file says a transfer that the program has not got: it is read as sRGB's.
    TransferNotKnown { said: String },
    /// The user chose the matrix, whatever the file says.
    MatrixChosen { chosen: VideoMatrix },
    /// The user chose the range, whatever the file says.
    RangeChosen { chosen: VideoRange },
    /// The user chose the primaries, whatever the file says.
    PrimariesChosen { chosen: StandardPrimaries },
    /// The user chose the curve, whatever the file says.
    CurveChosen { chosen: TransferFunction },
}

impl Assumption {
    /// What the dialog of a source says of it.
    pub fn in_words(&self) -> String {
        match self {
            Self::MatrixNotSaid { assumed, width, height } => format!(
                "The file does not say which matrix its colours use: {} is assumed, because its picture is {width} by {height}.",
                matrix_name(*assumed)
            ),
            Self::MatrixNotKnown { said } => {
                format!("The file says that its colours use the matrix {said}, which this version does not have: ffmpeg's own is used.")
            }
            Self::RangeNotSaid => "The file does not say its range: the limited one (16 to 235) is assumed.".to_owned(),
            Self::RangeNotKnown { said } => format!("The file says that its range is {said}: the limited one (16 to 235) is assumed."),
            Self::PrimariesNotSaid => "The file does not say its primaries: Rec.709's are assumed.".to_owned(),
            Self::PrimariesNotKnown { said } => {
                format!("The file says that its primaries are {said}, which this version does not have: Rec.709's are assumed.")
            }
            Self::TransferNotSaid => "The file does not say its transfer function: the curve of sRGB is assumed, which is how a screen shows it.".to_owned(),
            Self::TransferIsForHighDynamicRange { said } => format!(
                "The file says that its transfer function is {said}, for high dynamic range, which this version does not convert: the curve of sRGB is used, and the colours will not be right."
            ),
            Self::TransferNotKnown { said } => {
                format!("The file says that its transfer function is {said}, which this version does not have: the curve of sRGB is assumed.")
            }
            Self::MatrixChosen { chosen } => format!("You chose {} as the matrix of the colours of this file.", matrix_name(*chosen)),
            Self::RangeChosen { chosen } => format!("You chose {} as the range of the numbers of this file.", chosen.label()),
            Self::PrimariesChosen { chosen } => format!("You chose {} as the primaries of the colours of this file.", chosen.label()),
            Self::CurveChosen { chosen } => format!("You chose {} as the transfer function of this file.", chosen.label()),
        }
    }
}

/// What the matrix is called in the words of the dialog.
pub fn matrix_name(matrix: VideoMatrix) -> &'static str {
    matrix.label()
}

/// The matrix that a picture of this size is taken to use when its file does not say: BT.709 for high definition, BT.601 for standard definition.
pub fn matrix_assumed_for(width: u32, height: u32) -> VideoMatrix {
    if width > WIDEST_PICTURE_OF_STANDARD_DEFINITION || height > TALLEST_PICTURE_OF_STANDARD_DEFINITION {
        VideoMatrix::Bt709
    } else {
        VideoMatrix::Bt601
    }
}

impl VideoStreamFormat {
    /// What the colours of a video of this format are, whose picture is `width` by `height` pixels (see the module documentation for the rules).
    pub fn colour_of_a_picture_of_size(&self, width: u32, height: u32) -> VideoColour {
        self.colour_of_a_picture_of_size_with(width, height, &InputColourOverride::AUTOMATIC)
    }

    /// What the colours of a video of this format are when the user has said some of them (`said`): each part that they have said is what they said, and each part
    /// that they have not is what [`Self::colour_of_a_picture_of_size`] makes of it. **This is the one function that the preview, the export and the command line read
    /// the colours of a video with.**
    pub fn colour_of_a_picture_of_size_with(&self, width: u32, height: u32, said: &InputColourOverride) -> VideoColour {
        let mut assumptions = Vec::new();

        let matrix = match self.colour_space.as_deref() {
            _ if said.matrix.is_some() => {
                let chosen = said.matrix.expect("the guard says it is there");
                assumptions.push(Assumption::MatrixChosen { chosen });
                Some(chosen)
            }
            None => {
                let assumed = matrix_assumed_for(width, height);
                assumptions.push(Assumption::MatrixNotSaid { assumed, width, height });
                Some(assumed)
            }
            Some("bt709") => Some(VideoMatrix::Bt709),
            Some("bt470bg" | "smpte170m") => Some(VideoMatrix::Bt601),
            Some("bt2020nc") => Some(VideoMatrix::Bt2020),
            Some(other) => {
                assumptions.push(Assumption::MatrixNotKnown { said: other.to_owned() });
                None
            }
        };

        let full_range = match self.colour_range.as_deref() {
            _ if said.range.is_some() => {
                let chosen = said.range.expect("the guard says it is there");
                assumptions.push(Assumption::RangeChosen { chosen });
                Some(chosen.is_full())
            }
            None => {
                assumptions.push(Assumption::RangeNotSaid);
                Some(false)
            }
            Some("tv" | "mpeg") => Some(false),
            Some("pc" | "jpeg") => Some(true),
            Some(other) => {
                assumptions.push(Assumption::RangeNotKnown { said: other.to_owned() });
                None
            }
        };

        let primaries = match self.colour_primaries.as_deref() {
            _ if said.primaries.is_some() => {
                let chosen = said.primaries.expect("the guard says it is there");
                assumptions.push(Assumption::PrimariesChosen { chosen });
                chosen
            }
            None => {
                assumptions.push(Assumption::PrimariesNotSaid);
                StandardPrimaries::Rec709
            }
            Some("bt709") => StandardPrimaries::Rec709,
            Some("smpte170m") => StandardPrimaries::Bt601Ntsc,
            Some("bt470bg") => StandardPrimaries::Bt601Pal,
            Some("bt2020") => StandardPrimaries::Rec2020,
            Some("smpte432") => StandardPrimaries::DisplayP3,
            Some(other) => {
                assumptions.push(Assumption::PrimariesNotKnown { said: other.to_owned() });
                StandardPrimaries::Rec709
            }
        };

        let curve = match self.colour_transfer.as_deref() {
            _ if said.curve.is_some() => {
                let chosen = said.curve.expect("the guard says it is there");
                assumptions.push(Assumption::CurveChosen { chosen });
                chosen
            }
            None => {
                assumptions.push(Assumption::TransferNotSaid);
                TransferFunction::Srgb
            }
            // The transfer functions of the video standards of standard dynamic range, and sRGB's own, are read as sRGB's: how a screen shows them (see the documentation).
            Some("bt709" | "smpte170m" | "bt470bg" | "smpte240m" | "bt2020-10" | "bt2020-12" | "iec61966-2-1") => {
                TransferFunction::Srgb
            }
            Some("linear") => TransferFunction::Linear,
            Some("gamma22") => TransferFunction::Gamma22,
            Some(high_dynamic_range @ ("smpte2084" | "arib-std-b67")) => {
                assumptions.push(Assumption::TransferIsForHighDynamicRange {
                    said: high_dynamic_range.to_owned(),
                });
                TransferFunction::Srgb
            }
            Some(other) => {
                assumptions.push(Assumption::TransferNotKnown { said: other.to_owned() });
                TransferFunction::Srgb
            }
        };

        VideoColour {
            matrix,
            full_range,
            primaries,
            curve,
            assumptions,
        }
    }
}

#[cfg(test)]
mod tests;
