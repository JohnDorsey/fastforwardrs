// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The colours of the finished video: which matrix the engine makes the planes with, and what the file says its colours are.
//!
//! **The rule** (`docs/engine-phase-2-design.md`, section 5): the matrix and the tags follow the *first video* of the project. A video that says its matrix is turned
//! into red, green and blue by the engine with that matrix and back into planes with the same one, so the planes of a video that fills the picture are the planes it
//! had (to within a level), and the file says what the video said, as the old export (which kept the planes) left a file. A first video that says nothing is BT.601
//! and **left untagged**, as it was (the engine's compatibility rule is that a video that says nothing is BT.601, limited, because that is what ffmpeg does with it
//! when it makes colours itself). A project without a video is the same. The range of the finished video is always the limited one (a video of full range is
//! squeezed into it, and the file says `tv`: the picture is the same).

use crate::engine::{YuvColour, YuvMatrix, YuvRange};
use crate::media::VideoStreamFormat;

/// What a file says about the colours of its picture, as the filter that writes it is told: the names that ffprobe prints and the filters of ffmpeg take. What is
/// `None` is not said.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OutputTags {
    /// The matrix (`colorspace`), only one of those that the engine can make planes with: `bt709`, `bt470bg`, `smpte170m` or `bt2020nc`.
    pub matrix: Option<String>,
    /// The primaries (`color_primaries`) the first video states, passed on as they are.
    pub primaries: Option<String>,
    /// The transfer (`color_trc`) the first video states, passed on as they are.
    pub transfer: Option<String>,
}

impl OutputTags {
    /// Whether the file says nothing about its colours.
    pub fn is_empty(&self) -> bool {
        self.matrix.is_none() && self.primaries.is_none() && self.transfer.is_none()
    }

    /// The filter that gives a video these tags: `None` if it has none. A file that says anything about its colours says that its range is the limited one (`tv`),
    /// which is what the planes are.
    pub fn setparams_filter(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut options = Vec::new();
        if let Some(matrix) = &self.matrix {
            options.push(format!("colorspace={matrix}"));
        }
        options.push("range=tv".to_owned());
        if let Some(primaries) = &self.primaries {
            options.push(format!("color_primaries={primaries}"));
        }
        if let Some(transfer) = &self.transfer {
            options.push(format!("color_trc={transfer}"));
        }
        Some(format!("setparams={}", options.join(":")))
    }
}

/// How the planes of the finished video are made, and what the file says about them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputColour {
    /// What the engine makes the planes with.
    pub planes: YuvColour,
    pub tags: OutputTags,
}

/// The colours of the finished video of a project whose first video is stored the way `first_video` says (`None` if the project has no video).
pub fn output_colour_for(first_video: Option<&VideoStreamFormat>) -> OutputColour {
    let Some(format) = first_video else {
        return OutputColour {
            planes: YuvColour::UNSPECIFIED,
            tags: OutputTags::default(),
        };
    };
    // (Only the names that are matched below are put in the filter, so that the matrix needs no check of its name: anything else is a matrix that cannot be made.)
    let matrix_stated = format.colour_space.as_deref();
    let (matrix, matrix_tag) = match matrix_stated {
        None => (YuvMatrix::Bt601, None),
        Some(name @ ("bt470bg" | "smpte170m")) => (YuvMatrix::Bt601, Some(name)),
        Some(name @ "bt709") => (YuvMatrix::Bt709, Some(name)),
        Some(name @ "bt2020nc") => (YuvMatrix::Bt2020, Some(name)),
        // A matrix that the engine cannot make planes with (ffmpeg made the colours of such a video itself): the planes are BT.601, and the file does not claim
        // another matrix than that.
        Some(_) => (YuvMatrix::Bt601, None),
    };
    OutputColour {
        planes: YuvColour {
            matrix,
            range: YuvRange::Limited,
        },
        tags: OutputTags {
            matrix: matrix_tag.map(str::to_owned),
            primaries: format.colour_primaries.clone().filter(|name| is_a_name(name)),
            transfer: format.colour_transfer.clone().filter(|name| is_a_name(name)),
        },
    }
}

/// Whether `text` is the kind of name that ffprobe prints for a colour tag (letters, digits, hyphens, underscores), which can be put in a filter as it is: none of the
/// characters that mean something in the text of a filter (a colon, an equals sign, a comma, a quote...) is in it. (Whatever a file says is passed through ffprobe's
/// tables of names, so this is a second lock on the door and not the first.) The primaries and the transfer of a video are passed on as they are, and are checked by this.
fn is_a_name(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
}

#[cfg(test)]
mod tests;
