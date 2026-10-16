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

/// Where the colour differences of a video that stores them for every square of four pixels (4:2:0) sit among the pixels of the brightness: it decides how they are
/// blended back into one for every pixel. A file says it by its `chroma_location`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ChromaSiting {
    /// Level with the first column of the square, and half way down it: MPEG-2 and what H.264 means when it says nothing (so it is what the program means when the file
    /// does not say).
    #[default]
    Left,
    /// In the middle of the square: JPEG and MPEG-1.
    Center,
    /// On the first pixel of the square.
    TopLeft,
}

impl ChromaSiting {
    pub const ALL: [Self; 3] = [Self::Left, Self::Center, Self::TopLeft];

    /// How far across and down the colour difference of a square is from the middle of the first pixel of the square, in pixels of the brightness.
    pub fn offsets_in_luma_pixels(self) -> (f32, f32) {
        match self {
            Self::Left => (0.0, 0.5),
            Self::Center => (0.5, 0.5),
            Self::TopLeft => (0.0, 0.0),
        }
    }

    /// What `chroma_location` of ffprobe's description of a stream means (`None` for a file that does not say). The places the program does not have (`top`, `bottom`,
    /// `bottomleft`, which nothing uses) are read as the usual one.
    pub fn from_ffprobe(name: Option<&str>) -> Self {
        match name {
            Some("center") => Self::Center,
            Some("topleft") => Self::TopLeft,
            _ => Self::Left,
        }
    }

    /// What the dialog of a source calls it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Center => "Centre",
            Self::TopLeft => "Top left",
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
