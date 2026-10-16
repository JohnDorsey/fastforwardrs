// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The colours of the finished video: which matrix the engine makes the planes with, and what the file says its colours are.
//!
//! **The rule** (`docs/engine-phase-3-design.md`, decisions C9 and C14): both follow the **output colour space** of the project, and nothing else. The planes are made with
//! the matrix of BT.709 in the limited range, and the file always says so: its matrix, its primaries, its transfer (`bt709` for the two spaces that are Rec.709 video, and
//! `iec61966-2-1` for the one that is sRGB) and its range. Until phase 3 they followed the *first video* of the project, and a first video that said nothing left a file that
//! said nothing; a video of another matrix is converted now, in the light of the working space, and the file says what the numbers are.

use crate::engine::{YuvColour, YuvMatrix, YuvRange};
use crate::project::OutputColourSpace;

/// What a file says about the colours of its picture, as the filter that writes it is told: the names that ffprobe prints and the filters of ffmpeg take. What is
/// `None` is not said.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OutputTags {
    /// The matrix (`colorspace`), one of those that the engine can make planes with: `bt709`, `bt470bg`, `smpte170m` or `bt2020nc`.
    pub matrix: Option<String>,
    /// The primaries (`color_primaries`).
    pub primaries: Option<String>,
    /// The transfer (`color_trc`).
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

/// The colours of the finished video of a project whose output colour space is `space`.
pub fn output_colour_for(space: OutputColourSpace) -> OutputColour {
    OutputColour {
        planes: YuvColour {
            matrix: YuvMatrix::Bt709,
            range: YuvRange::Limited,
        },
        tags: OutputTags {
            matrix: Some("bt709".to_owned()),
            // (Every output space is Rec.709's primaries; `tests` says so, so that a space with other primaries makes this fail instead of making a file that lies.)
            primaries: Some("bt709".to_owned()),
            transfer: Some(space.transfer_tag().to_owned()),
        },
    }
}

#[cfg(test)]
mod tests;
