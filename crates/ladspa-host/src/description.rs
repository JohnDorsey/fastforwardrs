// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a plugin says about itself, as safe values that can be kept and sent to another program (the scan of a library is done by a child process, which writes this as text).
//! There is nothing here that touches a library: the descriptors are read in the module `library`, and what they say is turned into these.
//!
//! The arithmetic of the hints (what the default of a port is, what its range is) is written once, here, in `f64`, as `ladspa.h` says it.

use serde::{Deserialize, Serialize};

use crate::abi;

/// Which way the sound or the value goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortDirection {
    Input,
    Output,
}

/// What a port carries: one value for a block (a knob, or a meter), or the sound itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortKind {
    Control,
    Audio,
}

/// What a port's hint says its default is (`LADSPA_HINT_DEFAULT_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DefaultHint {
    /// Nothing was said.
    None,
    Minimum,
    /// A quarter of the way up the range (a quarter of the way up in the logarithm, if the range is logarithmic).
    Low,
    Middle,
    /// Three quarters of the way up.
    High,
    Maximum,
    Zero,
    One,
    OneHundred,
    FourHundredFortyHertz,
}

/// What a plugin says about the values a port takes (`LADSPA_PortRangeHint`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortHint {
    /// The lowest value, if the port has one and it is a number. (As the plugin says it: if [`Self::relative_to_sample_rate`] is set, the lowest value is this times the sample rate.)
    pub lower: Option<f32>,
    /// The highest value, in the same way.
    pub upper: Option<f32>,
    /// The value is on or off: above zero is on.
    pub toggled: bool,
    /// The bounds are fractions of the sample rate.
    pub relative_to_sample_rate: bool,
    /// The value is best moved on a logarithmic scale.
    pub logarithmic: bool,
    /// The value is a whole number.
    pub integer: bool,
    pub default: DefaultHint,
}

impl PortHint {
    /// The hint that the bits and the two bounds of a descriptor say. A bound that the bits do not say is there, or that is not a number, is not there.
    pub(crate) fn from_bits(bits: i32, lower: f32, upper: f32) -> Self {
        let has = |flag: i32| bits & flag != 0;
        Self {
            lower: (has(abi::HINT_BOUNDED_BELOW) && lower.is_finite()).then_some(lower),
            upper: (has(abi::HINT_BOUNDED_ABOVE) && upper.is_finite()).then_some(upper),
            toggled: has(abi::HINT_TOGGLED),
            relative_to_sample_rate: has(abi::HINT_SAMPLE_RATE),
            logarithmic: has(abi::HINT_LOGARITHMIC),
            integer: has(abi::HINT_INTEGER),
            default: match bits & abi::HINT_DEFAULT_MASK {
                abi::HINT_DEFAULT_MINIMUM => DefaultHint::Minimum,
                abi::HINT_DEFAULT_LOW => DefaultHint::Low,
                abi::HINT_DEFAULT_MIDDLE => DefaultHint::Middle,
                abi::HINT_DEFAULT_HIGH => DefaultHint::High,
                abi::HINT_DEFAULT_MAXIMUM => DefaultHint::Maximum,
                abi::HINT_DEFAULT_ZERO => DefaultHint::Zero,
                abi::HINT_DEFAULT_ONE => DefaultHint::One,
                abi::HINT_DEFAULT_ONE_HUNDRED => DefaultHint::OneHundred,
                abi::HINT_DEFAULT_FOUR_HUNDRED_FORTY => DefaultHint::FourHundredFortyHertz,
                abi::HINT_DEFAULT_NONE => DefaultHint::None,
                // (The numbers that `ladspa.h` leaves unused say nothing either.)
                _ => DefaultHint::None,
            },
        }
    }

    /// The lowest and the highest value at this sample rate (the bounds that are fractions of the sample rate are made into the values they are).
    pub fn bounds(&self, sample_rate: u32) -> (Option<f64>, Option<f64>) {
        let scale = if self.relative_to_sample_rate {
            f64::from(sample_rate)
        } else {
            1.0
        };
        (
            self.lower.map(|bound| f64::from(bound) * scale),
            self.upper.map(|bound| f64::from(bound) * scale),
        )
    }

    /// The value that the port starts with at this sample rate: the one the hint asks for, made a whole number if the port is one, and kept within the bounds. Where the hint
    /// asks for nothing, or asks for something its bounds do not allow (a quarter of the way up a range that has no top), it is zero, or the nearest value that is in the bounds.
    pub fn default_value(&self, sample_rate: u32) -> f32 {
        let (lower, upper) = self.bounds(sample_rate);
        // The value a quarter, a half or three quarters of the way up (`weight` is how much of the upper bound there is). It is on the logarithmic scale where the range
        // is logarithmic, which needs both bounds to be above zero; where they are not, the range is straight.
        let between = |weight: f64| -> Option<f64> {
            let (lower, upper) = (lower?, upper?);
            Some(if self.logarithmic && lower > 0.0 && upper > 0.0 {
                (lower.ln() * (1.0 - weight) + upper.ln() * weight).exp()
            } else {
                lower * (1.0 - weight) + upper * weight
            })
        };
        let asked_for = match self.default {
            DefaultHint::None => None,
            DefaultHint::Minimum => lower,
            DefaultHint::Low => between(0.25),
            DefaultHint::Middle => between(0.5),
            DefaultHint::High => between(0.75),
            DefaultHint::Maximum => upper,
            DefaultHint::Zero => Some(0.0),
            DefaultHint::One => Some(1.0),
            DefaultHint::OneHundred => Some(100.0),
            DefaultHint::FourHundredFortyHertz => Some(440.0),
        };
        let mut value = asked_for.unwrap_or(0.0);
        if self.integer {
            value = value.round();
        }
        if let Some(lower) = lower {
            value = value.max(lower);
        }
        if let Some(upper) = upper {
            value = value.min(upper);
        }
        value as f32
    }
}

/// One port of a plugin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortDescription {
    pub name: String,
    pub direction: PortDirection,
    pub kind: PortKind,
    pub hint: PortHint,
}

impl PortDescription {
    /// The port that a name, the bits of a port descriptor and a hint say, or why they do not say one (a port has to be an input or an output, and a control or audio, and
    /// not both).
    pub(crate) fn from_bits(name: String, bits: i32, hint: PortHint) -> Result<Self, String> {
        let direction = match (bits & abi::PORT_INPUT != 0, bits & abi::PORT_OUTPUT != 0) {
            (true, false) => PortDirection::Input,
            (false, true) => PortDirection::Output,
            (false, false) => return Err(format!("port \"{name}\" is neither an input nor an output")),
            (true, true) => return Err(format!("port \"{name}\" is both an input and an output")),
        };
        let kind = match (bits & abi::PORT_CONTROL != 0, bits & abi::PORT_AUDIO != 0) {
            (true, false) => PortKind::Control,
            (false, true) => PortKind::Audio,
            (false, false) => return Err(format!("port \"{name}\" is neither control nor audio")),
            (true, true) => return Err(format!("port \"{name}\" is both control and audio")),
        };
        Ok(Self {
            name,
            direction,
            kind,
            hint,
        })
    }
}

/// What a plugin says about how it may be run (`LADSPA_Properties`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginProperties {
    /// The plugin has a deadline: it must be run soon after the sound it works on is there.
    pub realtime: bool,
    /// The plugin gives wrong sound if an input and an output are the same buffer.
    pub inplace_broken: bool,
    /// The plugin never waits for anything while it runs (memory, a file, a lock).
    pub hard_realtime_capable: bool,
}

impl PluginProperties {
    pub(crate) fn from_bits(bits: i32) -> Self {
        Self {
            realtime: bits & abi::PROPERTY_REALTIME != 0,
            inplace_broken: bits & abi::PROPERTY_INPLACE_BROKEN != 0,
            hard_realtime_capable: bits & abi::PROPERTY_HARD_REALTIME_CAPABLE != 0,
        }
    }
}

/// A plugin of a library, as the library describes it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginDescription {
    /// Which plugin of the library it is: the number to give `ladspa_descriptor`. (It can change when the library does.)
    pub index: usize,
    /// The number the maker gave it. Plugins in the wild share them, so it is not a name.
    pub unique_id: u64,
    /// The name that is meant for programs: the library file's name and this make a plugin's key.
    pub label: String,
    /// The name that is meant for people.
    pub name: String,
    pub maker: String,
    pub copyright: String,
    pub properties: PluginProperties,
    /// The ports, in the order of their numbers.
    pub ports: Vec<PortDescription>,
}

impl PluginDescription {
    /// The numbers of the ports of one direction and one kind, in order.
    pub fn ports_of(&self, direction: PortDirection, kind: PortKind) -> impl Iterator<Item = usize> + '_ {
        self.ports
            .iter()
            .enumerate()
            .filter(move |(_, port)| port.direction == direction && port.kind == kind)
            .map(|(number, _)| number)
    }

    /// How many audio inputs and how many audio outputs the plugin has: what decides whether it is made once for each channel or once for both.
    pub fn audio_port_counts(&self) -> (usize, usize) {
        (
            self.ports_of(PortDirection::Input, PortKind::Audio).count(),
            self.ports_of(PortDirection::Output, PortKind::Audio).count(),
        )
    }
}
