// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The table of the sets of primaries that the program knows (CM-15: a colour space is data, so that one more is a row and not code), and the matrices between
//! them.

use serde::{Deserialize, Serialize};

use super::matrix::{self, Matrix3};
use super::primaries::{ACES_WHITE, Chromaticity, D65, Primaries, bradford_adaptation};

/// A set of primaries that the program knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StandardPrimaries {
    /// ITU-R BT.709, which sRGB shares: most video and nearly every picture and screen.
    Rec709,
    /// ITU-R BT.601 for 525 lines (SMPTE C, ffprobe's `smpte170m`): old American standard-definition video.
    Bt601Ntsc,
    /// ITU-R BT.601 for 625 lines (EBU, ffprobe's `bt470bg`): old European standard-definition video.
    Bt601Pal,
    /// ITU-R BT.2020: ultra-high-definition video.
    Rec2020,
    /// Display P3 (D65): wide-gamut displays.
    DisplayP3,
    /// ACEScg, the AP1 primaries of ACES: the colour space for compositing.
    AcesCg,
    /// ACES2065-1, the AP0 primaries of ACES, which hold every visible colour (and one that is not): for exchange and archives, not for compositing.
    Aces2065,
}

impl StandardPrimaries {
    pub const ALL: [Self; 7] = [
        Self::Rec709,
        Self::Bt601Ntsc,
        Self::Bt601Pal,
        Self::Rec2020,
        Self::DisplayP3,
        Self::AcesCg,
        Self::Aces2065,
    ];

    /// What the settings and the dialogs call it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Rec709 => "Rec.709 / sRGB",
            Self::Bt601Ntsc => "Rec.601 (525 lines)",
            Self::Bt601Pal => "Rec.601 (625 lines)",
            Self::Rec2020 => "Rec.2020",
            Self::DisplayP3 => "Display P3",
            Self::AcesCg => "ACEScg",
            Self::Aces2065 => "ACES2065-1",
        }
    }

    /// The number that says which set it is to the shaders (the place in [`Self::ALL`]).
    pub fn number_for_the_shaders(self) -> u32 {
        Self::ALL
            .iter()
            .position(|candidate| *candidate == self)
            .expect("every set is in the list of all of them") as u32
    }

    /// The chromaticities of the red, green and blue and of the white.
    pub fn primaries(self) -> Primaries {
        let (red, green, blue, white) = match self {
            Self::Rec709 => ((0.640, 0.330), (0.300, 0.600), (0.150, 0.060), D65),
            Self::Bt601Ntsc => ((0.630, 0.340), (0.310, 0.595), (0.155, 0.070), D65),
            Self::Bt601Pal => ((0.640, 0.330), (0.290, 0.600), (0.150, 0.060), D65),
            Self::Rec2020 => ((0.708, 0.292), (0.170, 0.797), (0.131, 0.046), D65),
            Self::DisplayP3 => ((0.680, 0.320), (0.265, 0.690), (0.150, 0.060), D65),
            Self::AcesCg => ((0.713, 0.293), (0.165, 0.830), (0.128, 0.044), ACES_WHITE),
            Self::Aces2065 => ((0.7347, 0.2653), (0.0, 1.0), (0.0001, -0.0770), ACES_WHITE),
        };
        Primaries {
            red: Chromaticity::new(red.0, red.1),
            green: Chromaticity::new(green.0, green.1),
            blue: Chromaticity::new(blue.0, blue.1),
            white,
        }
    }

    /// The matrix from the red, green and blue light of this set to X, Y and Z.
    pub fn rgb_to_xyz(self) -> Matrix3 {
        self.primaries()
            .rgb_to_xyz()
            .expect("the primaries of the table make colours")
    }

    /// The matrix from the red, green and blue **light** of this set to that of `target`: through X, Y and Z, with the white of this set adapted to the white of the
    /// target (the Bradford transform: the identity when the whites are the same). It acts on light and never on codes.
    pub fn conversion_to(self, target: Self) -> Matrix3 {
        if self == target {
            return matrix::IDENTITY;
        }
        let from = self.primaries();
        let to = target.primaries();
        let to_target_rgb = to.xyz_to_rgb().expect("the primaries of the table make colours");
        matrix::multiply(
            &to_target_rgb,
            &matrix::multiply(&bradford_adaptation(from.white, to.white), &self.rgb_to_xyz()),
        )
    }
}
