// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The curves between the numbers a picture is stored as (codes, from 0 to 1) and the light they stand for (also 0 to 1, where 1 is the white of the space): the
//! transfer functions. **This is the reference**: the graphics card's version (`engine/shaders/colour.wgsl`) is held to it by tests.
//!
//! Every curve is applied to a code that is cut off at 0 and 1, and to a light that is cut off at 0 below (a negative light, which a colour outside the gamut of a
//! display has, is the code 0 and never a number that is not a number); the light has no limit above, so that a picture brighter than white keeps being brighter.

use serde::{Deserialize, Serialize};

/// A curve between codes and light.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferFunction {
    /// The curve of sRGB (IEC 61966-2-1): a straight piece near black and a power of 2.4 above it, which is nearly a power of 2.2 all along. What computer screens
    /// do, and what pictures are stored as.
    Srgb,
    /// The power 2.2.
    Gamma22,
    /// The power 2.4: the display of BT.1886 (without the black lift), which is what a studio monitor does with video.
    Gamma24,
    /// The power 2.6 (digital cinema).
    Gamma26,
    /// The curve that a **camera** applies to the light it sees for BT.709 (and BT.601): a straight piece near black and a power of 0.45 above it, with the precise
    /// constants that BT.2020 gives (`1.09929682680944` and `0.018053968510807`) and not the rounded `1.099` and `0.018` of BT.709, whose two pieces do not meet
    /// (they are 0.00025 apart): so that the curve has an inverse. Not what a screen does: see the design of phase 3, decision C6.
    Rec709,
    /// No curve: the numbers are light.
    Linear,
}

const SRGB_KNEE_OF_THE_CODE: f64 = 0.04045;
const SRGB_KNEE_OF_THE_LIGHT: f64 = 0.0031308;

const REC709_ALPHA: f64 = 1.099_296_826_809_44;
const REC709_BETA: f64 = 0.018_053_968_510_807;

impl TransferFunction {
    pub const ALL: [Self; 6] = [
        Self::Srgb,
        Self::Gamma22,
        Self::Gamma24,
        Self::Gamma26,
        Self::Rec709,
        Self::Linear,
    ];

    /// What the settings and the dialogs call it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Srgb => "sRGB",
            Self::Gamma22 => "Gamma 2.2",
            Self::Gamma24 => "Gamma 2.4 (BT.1886)",
            Self::Gamma26 => "Gamma 2.6",
            Self::Rec709 => "Rec.709 camera curve",
            Self::Linear => "Linear",
        }
    }

    /// The number that says which curve it is to the shaders (the order of [`Self::ALL`], which a test keeps in step with `colour.wgsl`).
    pub fn number_for_the_shaders(self) -> u32 {
        match self {
            Self::Srgb => 0,
            Self::Gamma22 => 1,
            Self::Gamma24 => 2,
            Self::Gamma26 => 3,
            Self::Rec709 => 4,
            Self::Linear => 5,
        }
    }

    /// The light that a code stands for. The code is cut off at 0 and 1 first.
    pub fn decode_to_light(self, code: f64) -> f64 {
        let code = if code.is_nan() { 0.0 } else { code.clamp(0.0, 1.0) };
        match self {
            Self::Srgb => {
                if code <= SRGB_KNEE_OF_THE_CODE {
                    code / 12.92
                } else {
                    ((code + 0.055) / 1.055).powf(2.4)
                }
            }
            Self::Gamma22 => code.powf(2.2),
            Self::Gamma24 => code.powf(2.4),
            Self::Gamma26 => code.powf(2.6),
            Self::Rec709 => {
                if code < 4.5 * REC709_BETA {
                    code / 4.5
                } else {
                    ((code + (REC709_ALPHA - 1.0)) / REC709_ALPHA).powf(1.0 / 0.45)
                }
            }
            Self::Linear => code,
        }
    }

    /// The code that stands for a light. A light below 0 is 0; a light above 1 gives a code above 1 (it is for whoever makes the picture to cut it off).
    pub fn encode_from_light(self, light: f64) -> f64 {
        let light = if light.is_nan() { 0.0 } else { light.max(0.0) };
        match self {
            Self::Srgb => {
                if light <= SRGB_KNEE_OF_THE_LIGHT {
                    12.92 * light
                } else {
                    1.055 * light.powf(1.0 / 2.4) - 0.055
                }
            }
            Self::Gamma22 => light.powf(1.0 / 2.2),
            Self::Gamma24 => light.powf(1.0 / 2.4),
            Self::Gamma26 => light.powf(1.0 / 2.6),
            Self::Rec709 => {
                if light < REC709_BETA {
                    4.5 * light
                } else {
                    REC709_ALPHA * light.powf(0.45) - (REC709_ALPHA - 1.0)
                }
            }
            Self::Linear => light,
        }
    }
}
