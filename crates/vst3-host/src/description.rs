// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a module says about its classes, and what a class says about itself (its buses and its parameters), as safe values that can be sent between programs.

#![allow(unsafe_code)]

use std::ffi::c_char;

use serde::{Deserialize, Serialize};
use vst3::ComPtr;
use vst3::Steinberg::Vst::TChar;
use vst3::Steinberg::*;

use crate::classid::ClassId;
use crate::module::AUDIO_MODULE_CLASS;

/// The most classes that a factory may have: one that says it has more is read as far as this.
const MOST_CLASSES: i32 = 4096;

/// What a factory says of itself.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactoryDescription {
    pub vendor: String,
    pub url: String,
}

/// A class of a factory: a plugin, or another sort of object that the module makes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassDescription {
    pub id: ClassId,
    pub name: String,
    /// `Audio Module Class` for a plugin; `Component Controller Class` for the controller of one that has it in a class of its own; others for objects hosts do not make.
    pub category: String,
    /// The sub-categories (`Fx`, `Instrument`, `Reverb`, ...), where the factory says them.
    pub sub_categories: Vec<String>,
    pub vendor: String,
    pub version: String,
    pub sdk_version: String,
}

impl ClassDescription {
    /// Whether the class is a plugin (the others are never made by a host).
    pub fn is_a_plugin(&self) -> bool {
        self.category == AUDIO_MODULE_CLASS
    }
}

/// A bus of audio or of events.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BusDescription {
    pub name: String,
    pub channels: u32,
    /// A main bus (`kMain`), or an auxiliary one (a side chain, an extra output).
    pub is_main: bool,
    /// Whether it is on until it is switched off.
    pub is_on_by_default: bool,
}

/// A parameter of a controller.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParameterDescription {
    pub id: u32,
    pub title: String,
    pub short_title: String,
    pub units: String,
    /// 0 for a continuous parameter; for a stepped one the number of steps (a switch has 1, a list of three has 2): its values are the `step_count + 1` whole numbers.
    pub step_count: i32,
    pub default_normalized: f64,
    /// What the plugin says the default is as a plain value (not worked out from the range: a parameter need not be linear).
    pub default_plain: f64,
    /// What the plugin says the value is at the bottom and at the top of the normalised range (the plain value).
    pub plain_minimum: f64,
    pub plain_maximum: f64,
    /// For a stepped parameter of at most 256 steps, the text of each step (in order); else empty.
    pub texts: Vec<String>,
    pub can_automate: bool,
    pub is_read_only: bool,
    pub is_hidden: bool,
    pub is_a_list: bool,
    pub is_the_bypass: bool,
    pub is_a_program_change: bool,
    pub wraps_around: bool,
}

/// What a class is, found by making it: its buses, its parameters, whether its controller maps MIDI controllers, and its latency and tail.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ClassFacts {
    pub audio_inputs: Vec<BusDescription>,
    pub audio_outputs: Vec<BusDescription>,
    pub event_inputs: Vec<BusDescription>,
    pub parameters: Vec<ParameterDescription>,
    /// Whether the plugin has a controller at all (a plugin that has none has no parameters).
    pub has_a_controller: bool,
    /// Whether the controller is a class of its own (the host connects the two), or the component itself.
    pub controller_is_separate: bool,
    pub has_midi_mapping: bool,
    pub latency: u32,
    pub tail: u32,
}

impl ClassFacts {
    /// The place and description of the first main audio input.
    pub fn main_audio_input(&self) -> Option<(usize, &BusDescription)> {
        self.audio_inputs.iter().enumerate().find(|(_, bus)| bus.is_main)
    }

    /// The place and description of the first main audio output.
    pub fn main_audio_output(&self) -> Option<(usize, &BusDescription)> {
        self.audio_outputs.iter().enumerate().find(|(_, bus)| bus.is_main)
    }

    /// Whether the plugin takes notes (it has an event input).
    pub fn takes_events(&self) -> bool {
        !self.event_inputs.is_empty()
    }
}

/// The text in a field of bytes that ends at the first zero.
pub(crate) fn text_of(field: &[c_char]) -> String {
    let bytes: Vec<u8> = field
        .iter()
        .take_while(|byte| **byte != 0)
        .map(|byte| *byte as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The text in a field of UTF-16 units that ends at the first zero.
pub(crate) fn wide_text_of(field: &[TChar]) -> String {
    let end = field.iter().position(|unit| *unit == 0).unwrap_or(field.len());
    String::from_utf16_lossy(&field[..end])
}

fn sub_categories_of(text: &str) -> Vec<String> {
    text.split('|')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The classes of a factory, with the most that it can say of each (`IPluginFactory3` has the names in Unicode, `IPluginFactory2` the sub-categories and the vendor).
pub(crate) fn classes_of(factory: &ComPtr<IPluginFactory>) -> Vec<ClassDescription> {
    // SAFETY: the factory is a live COM object (it is held by the module); counting its classes has no arguments.
    let count = unsafe { factory.countClasses() }.clamp(0, MOST_CLASSES);
    let factory3 = factory.cast::<IPluginFactory3>();
    let factory2 = factory.cast::<IPluginFactory2>();
    let mut classes = Vec::new();
    for index in 0..count {
        // SAFETY: `PClassInfo` is a plain struct of integers and arrays of them, for which all zeros is a value.
        let mut plain: PClassInfo = unsafe { std::mem::zeroed() };
        // SAFETY: `plain` lives through the call, which fills it.
        if unsafe { factory.getClassInfo(index, &mut plain) } != kResultOk {
            continue;
        }
        let mut class = ClassDescription {
            id: ClassId::from_tuid(&plain.cid),
            name: text_of(&plain.name),
            category: text_of(&plain.category),
            sub_categories: Vec::new(),
            vendor: String::new(),
            version: String::new(),
            sdk_version: String::new(),
        };
        if let Some(factory3) = &factory3 {
            // SAFETY: as for `PClassInfo`.
            let mut wide: PClassInfoW = unsafe { std::mem::zeroed() };
            // SAFETY: `wide` lives through the call, which fills it.
            if unsafe { factory3.getClassInfoUnicode(index, &mut wide) } == kResultOk {
                class.name = wide_text_of(&wide.name);
                class.sub_categories = sub_categories_of(&text_of(&wide.subCategories));
                class.vendor = wide_text_of(&wide.vendor);
                class.version = wide_text_of(&wide.version);
                class.sdk_version = wide_text_of(&wide.sdkVersion);
                classes.push(class);
                continue;
            }
        }
        if let Some(factory2) = &factory2 {
            // SAFETY: as for `PClassInfo`.
            let mut two: PClassInfo2 = unsafe { std::mem::zeroed() };
            // SAFETY: `two` lives through the call, which fills it.
            if unsafe { factory2.getClassInfo2(index, &mut two) } == kResultOk {
                class.sub_categories = sub_categories_of(&text_of(&two.subCategories));
                class.vendor = text_of(&two.vendor);
                class.version = text_of(&two.version);
                class.sdk_version = text_of(&two.sdkVersion);
            }
        }
        classes.push(class);
    }
    classes
}
