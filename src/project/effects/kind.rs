// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The kind of an effect: which type of effect it is, which is a descriptor (see [`EffectDescriptor`]) that the program has in itself or that a
//! project file brought.
//!
//! A kind is **a small value that can be copied, compared and put in order**, so that it can be a part of what a keyframe is of (see
//! [`PropertyId::EffectValue`](crate::project::PropertyId::EffectValue)) without a project or a list of the effects there are being at
//! hand to say what the value of an effect is called or is made of. It is made by being *interned*: a descriptor that is made into a kind is
//! kept for the rest of the run of the program (they are few and small), and the same descriptor made into a kind twice is the same kind. A
//! kind is therefore a reference to a descriptor that is never changed or freed, and **is equal to another only when it is the same
//! descriptor**.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, PoisonError};

use super::built_in::{self, HUE_SHIFT};
use super::{EffectDescriptor, EffectDomain, ParameterDescriptor};
use crate::colour_effects::ColourMatrix;
use crate::project::animation::{PropertyValue, ValueKind};
use crate::project::groups::GroupKind;
use crate::project::tracks::ClipKind;

/// The descriptors that files brought, and that are not those of an effect of the program: each only once.
static BROUGHT_BY_FILES: Mutex<Vec<&'static EffectDescriptor>> = Mutex::new(Vec::new());

/// Which type of effect an effect is.
#[derive(Clone, Copy)]
pub struct EffectKind(&'static EffectDescriptor);

impl EffectKind {
    /// The hue shift, which turns the colours of a picture round the wheel of colours.
    pub const HUE_SHIFT: EffectKind = EffectKind(&HUE_SHIFT);

    /// The effects that the program has in itself, in the order they are offered in.
    pub const BUILT_IN: [EffectKind; 1] = [Self::HUE_SHIFT];

    /// The kind of effect that a snapshot of a descriptor, which a project file kept, says one is: the effect of the program that can stand in for
    /// it (see [`EffectDescriptor::can_stand_in_for`]) if there is one, and otherwise a kind of effect of its own, which the program does not
    /// have (it is *not installed*: see [`Self::is_installed`]). Nothing is lost either way: the descriptor is what says what the effect's
    /// values are. The error says what is wrong with a descriptor that cannot be used.
    pub fn from_snapshot(snapshot: EffectDescriptor) -> Result<Self, String> {
        if let Some(problem) = snapshot.problem() {
            return Err(problem);
        }
        if let Some(own) = Self::BUILT_IN
            .into_iter()
            .find(|own| own.descriptor().can_stand_in_for(&snapshot))
        {
            return Ok(own);
        }
        let mut brought = BROUGHT_BY_FILES.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(already) = brought.iter().find(|already| ***already == snapshot) {
            return Ok(Self(already));
        }
        let kept: &'static EffectDescriptor = Box::leak(Box::new(snapshot));
        brought.push(kept);
        Ok(Self(kept))
    }

    /// The effect of the program that has this key, if there is one.
    pub fn built_in_with_key(key: &str) -> Option<Self> {
        Self::BUILT_IN.into_iter().find(|own| own.key() == key)
    }

    /// The descriptor of the type of effect.
    pub fn descriptor(self) -> &'static EffectDescriptor {
        self.0
    }

    /// What names the type of effect: a namespace, a colon and an id (`builtin:hue_shift`).
    pub fn key(self) -> &'static str {
        &self.0.key
    }

    /// The major and the minor number of the version of the type of effect.
    pub fn version(self) -> (u32, u32) {
        self.0.version
    }

    /// What the type of effect is called, in lower case.
    pub fn name(self) -> &'static str {
        &self.0.name
    }

    /// What the effect does, in a few words.
    pub fn description(self) -> &'static str {
        &self.0.description
    }

    pub fn domain(self) -> EffectDomain {
        self.0.domain
    }

    /// Whether the program has this effect in itself, so that it does what it says. An effect that a file brought and that the program does
    /// not have is *not installed*: it is kept, with its values and keyframes, and does nothing to a picture.
    pub fn is_installed(self) -> bool {
        Self::BUILT_IN.contains(&self)
    }

    /// The values that the effect has, in the order they are kept and shown in.
    pub fn parameters(self) -> &'static [ParameterDescriptor] {
        &self.0.parameters
    }

    /// How many values the effect has.
    pub fn value_count(self) -> usize {
        self.0.parameters.len()
    }

    /// What kind of value the value in place `slot` is, if the effect has one there.
    pub fn value_kind(self, slot: usize) -> Option<ValueKind> {
        self.0.parameters.get(slot).map(ParameterDescriptor::value_kind)
    }

    /// The place of the value that has the id `id`, if the effect has one.
    pub fn slot_of_the_value_with_id(self, id: &str) -> Option<usize> {
        self.0.parameters.iter().position(|parameter| parameter.id == id)
    }

    /// What the values of the effect are when it is put on something, one for each of its values.
    pub fn default_values(self) -> Vec<PropertyValue> {
        self.0
            .parameters
            .iter()
            .map(|parameter| parameter.default.clone())
            .collect()
    }

    /// Whether a clip of this kind can have the effect: a picture effect can be on what is seen (a video, a picture, a text object), and a
    /// sound effect on a sound.
    pub fn applies_to_a_clip_of_kind(self, kind: ClipKind) -> bool {
        match self.0.domain {
            EffectDomain::Picture => matches!(kind, ClipKind::Video | ClipKind::Image | ClipKind::Text),
            EffectDomain::Sound => kind == ClipKind::Audio,
        }
    }

    /// Whether a track can have the effect.
    pub fn applies_to_a_track(self) -> bool {
        true
    }

    /// Whether a group of kind `kind` can have the effect: a tag, which has it applied to each member, can have any; and a clip group or a track
    /// group, which have it applied to what their members make together, only a picture effect that changes each pixel by itself.
    pub fn applies_to_a_group_of_kind(self, kind: GroupKind) -> bool {
        match kind {
            GroupKind::Tag => true,
            GroupKind::ClipGroup | GroupKind::TrackGroup => {
                self.0.domain == EffectDomain::Picture && self.0.changes_each_pixel_by_itself
            }
        }
    }

    /// The values that are exported when the values of the effect at a frame are `values` (the same values of the same kinds): the values as
    /// near as is worth telling apart, which is `values` themselves for an effect whose values are all that there is to see of them. What
    /// is the same when it is exported is exported as one thing, which makes the export quicker to get ready.
    pub fn as_exported(self, values: &[PropertyValue]) -> Vec<PropertyValue> {
        built_in::as_exported(self, values)
    }

    /// How the effect changes the colours of a picture when its values are `values` (one for each of its values, in order), if it changes
    /// colours at all. (`None` for an effect that is not installed, which changes nothing.)
    pub fn colour_matrix(self, values: &[PropertyValue]) -> Option<ColourMatrix> {
        built_in::colour_matrix(self, values)
    }

    fn address(self) -> usize {
        std::ptr::from_ref(self.0) as usize
    }
}

impl PartialEq for EffectKind {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

impl Eq for EffectKind {}

impl Hash for EffectKind {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.address().hash(hasher);
    }
}

impl PartialOrd for EffectKind {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// In the order of the keys, and of the versions of one key. (Two kinds that have the same key and version, which differ in what is in their
/// descriptors, are told apart by where they are in memory: they are not the same kind, and something has to say which comes first. That is
/// only for what is not a file that a person has made by hand.)
impl Ord for EffectKind {
    fn cmp(&self, other: &Self) -> Ordering {
        if self == other {
            return Ordering::Equal;
        }
        self.0
            .key
            .cmp(&other.0.key)
            .then(self.0.version.cmp(&other.0.version))
            .then_with(|| self.address().cmp(&other.address()))
    }
}

impl fmt::Debug for EffectKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "EffectKind({} {}.{})",
            self.0.key, self.0.version.0, self.0.version.1
        )
    }
}

impl fmt::Display for EffectKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}
