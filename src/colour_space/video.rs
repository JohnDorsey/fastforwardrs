// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The two things that a video file says about its colours that are not primaries and not a curve: the **matrix** that makes red, green and blue of brightness and the
//! colour differences, and the **range** its numbers use.

use serde::{Deserialize, Serialize};

/// The matrices that the program can turn the planes of a video into colours with itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoMatrix {
    /// ITU-R BT.601: old and standard-definition video.
    Bt601,
    /// ITU-R BT.709: nearly all high-definition video.
    Bt709,
    /// ITU-R BT.2020 (non-constant luminance): ultra-high-definition video.
    Bt2020,
}

impl VideoMatrix {
    pub const ALL: [Self; 3] = [Self::Bt601, Self::Bt709, Self::Bt2020];

    /// What the dialog of a source calls it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Bt601 => "BT.601",
            Self::Bt709 => "BT.709",
            Self::Bt2020 => "BT.2020",
        }
    }
}

/// Which numbers a video uses for black and white and for no colour difference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoRange {
    /// 16 to 235 for brightness and 16 to 240 for the colour differences: as nearly all video is.
    Limited,
    /// 0 to 255 for all of them.
    Full,
}

impl VideoRange {
    pub const ALL: [Self; 2] = [Self::Limited, Self::Full];

    /// What the dialog of a source calls it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Limited => "Limited (16 to 235)",
            Self::Full => "Full (0 to 255)",
        }
    }

    /// Whether this is the full range (what `NativeYuv420p::full_range` and the file's `pc` mean).
    pub fn is_full(self) -> bool {
        self == Self::Full
    }
}
