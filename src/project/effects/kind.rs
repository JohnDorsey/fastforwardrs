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

use super::built_in::{
    self, BLUR, BRIGHTNESS_AND_CONTRAST, CROP, EXPOSURE, HUE_SHIFT, LUT, OPACITY, PictureOperation, SATURATION, TEXT,
};
use super::installed::{InstalledPlugin, installed_plugin_for, installed_plugins, installed_shaders};
use super::{EffectDescriptor, EffectDomain, ParameterDescriptor, ShaderSource};
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

    /// The saturation, which makes the colours more or less colourful.
    pub const SATURATION: EffectKind = EffectKind(&SATURATION);

    /// Brightness and contrast, which make the picture lighter or darker and its lights and darks further apart or nearer together.
    pub const BRIGHTNESS_AND_CONTRAST: EffectKind = EffectKind(&BRIGHTNESS_AND_CONTRAST);

    /// The exposure, which makes the light of the picture brighter or darker by whole and part stops.
    pub const EXPOSURE: EffectKind = EffectKind(&EXPOSURE);

    /// The opacity, which makes the picture more or less see-through.
    pub const OPACITY: EffectKind = EffectKind(&OPACITY);

    /// The blur, which softens the picture.
    pub const BLUR: EffectKind = EffectKind(&BLUR);

    /// The crop, which cuts the edges off the picture.
    pub const CROP: EffectKind = EffectKind(&CROP);

    /// The look-up table, which looks the colours of the picture up in a table (a `.cube` file).
    pub const LUT: EffectKind = EffectKind(&LUT);

    /// The text, which a text clip is made of (phase 9): words, in a font, of a size and a colour. **It is not one of the effects that the program offers** ([`Self::BUILT_IN`] has not got it):
    /// only a text clip can have it, as its content, and it asks nothing of a picture.
    pub const TEXT: EffectKind = EffectKind(&TEXT);

    /// The effects that the program has in itself that are what a clip is made of and not something that is done to a picture: they are not offered as effects, and are found by their key.
    pub const BUILT_IN_CONTENTS: [EffectKind; 1] = [Self::TEXT];

    /// The effects that the program has in itself, in the order they are offered in.
    pub const BUILT_IN: [EffectKind; 8] = [
        Self::HUE_SHIFT,
        Self::SATURATION,
        Self::BRIGHTNESS_AND_CONTRAST,
        Self::EXPOSURE,
        Self::OPACITY,
        Self::BLUR,
        Self::CROP,
        Self::LUT,
    ];

    /// The kind of effect that a snapshot of a descriptor, which a project file kept, says one is: the effect of the program that can stand in for
    /// it (see [`EffectDescriptor::can_stand_in_for`]) if there is one, or a plugin that is installed and can, and otherwise a kind of effect of its own, which the program
    /// does not have (it is *not installed*: see [`Self::is_installed`]). Nothing is lost either way: the descriptor is what says what the effect's
    /// values are. The error says what is wrong with a descriptor that cannot be used.
    pub fn from_snapshot(snapshot: EffectDescriptor) -> Result<Self, String> {
        if let Some(problem) = snapshot.problem() {
            return Err(problem);
        }
        if let Some(own) = Self::BUILT_IN
            .into_iter()
            .chain(Self::BUILT_IN_CONTENTS)
            .find(|own| own.descriptor().can_stand_in_for(&snapshot))
        {
            return Ok(own);
        }
        if let Some(plugin) = installed_plugin_for(&snapshot) {
            return Ok(Self(&plugin.descriptor));
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
        Self::BUILT_IN
            .into_iter()
            .chain(Self::BUILT_IN_CONTENTS)
            .find(|own| own.key() == key)
    }

    /// The descriptor of the type of effect.
    pub fn descriptor(self) -> &'static EffectDescriptor {
        self.0
    }

    /// Whether it is a synthesizer: it makes sound from the notes that reach it (phase 13).
    pub fn makes_sound_from_notes(self) -> bool {
        self.0.makes_sound_from_notes
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

    /// Whether the program has this effect in itself, or a plugin that is installed now can stand in for it, or it is a shader that compiles and that this version can run, so that
    /// it does what it says. An effect that a file brought and that
    /// the program does not have is *not installed*: it is kept, with its values and keyframes, and does nothing to a picture or a sound. (This can change while the program
    /// runs: the plugins are found in the background.)
    pub fn is_installed(self) -> bool {
        Self::BUILT_IN.contains(&self)
            || Self::BUILT_IN_CONTENTS.contains(&self)
            || self.installed_plugin().is_some()
            || crate::isf::is_usable(self.0)
    }

    /// Whether the effect is a shader that makes a picture and reads none (a generator: see [`crate::isf::Shape`]), which a clip is made of and which is not put on a picture.
    pub fn is_a_generator(self) -> bool {
        crate::isf::shape_of(self.0) == Some(crate::isf::Shape::Generator)
    }

    /// Whether the effect is a shader that reads what is behind a clip and the clip itself (a transition: see [`crate::isf::Shape`]), which is put on the clip that is on top, with
    /// its progress animated, so that the clip comes over what is behind it.
    pub fn is_a_transition(self) -> bool {
        crate::isf::shape_of(self.0) == Some(crate::isf::Shape::Transition)
    }

    /// Whether the effect is a shader that this version runs and that keeps something from one frame to the next (a buffer with `PERSISTENT`), so that what it shows depends on the
    /// frames before it.
    pub fn depends_on_earlier_frames(self) -> bool {
        self.0.shader.is_some() && crate::isf::depends_on_earlier_frames(self.0)
    }

    /// The shader that runs the effect, if it is one (see [`ShaderSource`]): the text of its file, which the project carries.
    pub fn shader_source(self) -> Option<&'static ShaderSource> {
        self.0.shader.as_ref()
    }

    /// The plugin that is installed now and does what this effect says, if this is the type of effect of a plugin.
    pub fn installed_plugin(self) -> Option<&'static InstalledPlugin> {
        installed_plugin_for(self.0)
    }

    /// The kinds of effect that are plugins and are installed now, in the order of the folders they were found in.
    pub fn installed_plugin_kinds() -> Vec<EffectKind> {
        installed_plugins()
            .into_iter()
            .map(|plugin| Self(&plugin.descriptor))
            .collect()
    }

    /// The kinds of effect that are shaders, found in the folders of the plugin manager and run by this version (phase 6.8), in the order of the folders they were found in.
    pub fn installed_shader_kinds() -> Vec<EffectKind> {
        installed_shaders()
    }

    /// The installed shaders that make a picture and read none: what a generator clip is made of (they are not offered as effects).
    pub fn installed_generators() -> Vec<EffectKind> {
        installed_shaders()
            .into_iter()
            .filter(|kind| kind.is_a_generator())
            .collect()
    }

    /// The effects that a thing can be offered: those the program has in itself, then the shaders that are installed now (the filters, and the transitions: a generator is
    /// not an effect, it is made into a clip), then the plugins that are installed now.
    pub fn offered() -> Vec<EffectKind> {
        let mut kinds = Self::BUILT_IN.to_vec();
        kinds.extend(installed_shaders().into_iter().filter(|kind| !kind.is_a_generator()));
        kinds.extend(Self::installed_plugin_kinds());
        kinds
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
            .map(ParameterDescriptor::value_it_begins_as)
            .collect()
    }

    /// Whether a clip of this kind can have the effect: a picture effect can be on what is seen (a video, a picture, a text object, and an adjustment clip, which is what
    /// is behind it, seen: **but not a transition, which is put on a clip that has a picture of its own, phase 6.6**), and a
    /// sound effect on what is heard (a sound, **and a video, which has a soundtrack**: it was only a sound until phase 7).
    pub fn applies_to_a_clip_of_kind(self, kind: ClipKind) -> bool {
        // (The text is what a text clip is made of, and nothing else can have it.)
        if self == Self::TEXT {
            return kind == ClipKind::Text;
        }
        match self.0.domain {
            EffectDomain::Picture => match kind {
                ClipKind::Video | ClipKind::Image | ClipKind::Text | ClipKind::Generator => true,
                // (An adjustment clip is what is behind it, seen: it has no picture of its own for a transition to bring over what is behind it.)
                ClipKind::Adjustment => !self.is_a_transition(),
                _ => false,
            },
            EffectDomain::Sound => matches!(kind, ClipKind::Audio | ClipKind::Video),
        }
    }

    /// Whether a track can have the effect. (Not a transition: a track has no picture of its own, it is the clips on it, and a transition is put on the clip that is on top.)
    pub fn applies_to_a_track(self) -> bool {
        self != Self::TEXT && !self.is_a_transition()
    }

    /// Whether a group of kind `kind` can have the effect: a tag, which has it applied to each member, can have any; and a clip group or a track group, which have it done to
    /// what their members make together, any: a picture effect to the picture (phase 5: the engine makes that picture), **and a sound effect to the sum of the sound of the
    /// members** (phase 12, piece 12.3: `composition::sound` makes a node of the group, and the sound engine sums what its members make and gives it to the effects).
    pub fn applies_to_a_group_of_kind(self, _kind: GroupKind) -> bool {
        self != Self::TEXT
    }

    /// What the effect asks to be done to a picture when its values are `values` (one for each of its values, in order), if the program has the effect. (`None`
    /// for an effect that is not installed, which changes nothing.)
    pub fn picture_operation(self, values: &[PropertyValue]) -> Option<PictureOperation> {
        built_in::picture_operation(self, values, &[])
    }

    /// [`Self::picture_operation`] for an effect that uses tables: `tables` has one place for each of its values, `Some(table)` for a value that names a resource of the
    /// project that is a table (see `Project::tables_of_the_values_of`). A look-up table effect with none asks nothing of the picture.
    pub fn picture_operation_with_tables(
        self,
        values: &[PropertyValue],
        tables: &[Option<std::sync::Arc<crate::cube_table::CubeTable>>],
    ) -> Option<PictureOperation> {
        built_in::picture_operation(self, values, tables)
    }

    /// How the effect changes the colours of a picture when its values are `values` (one for each of its values, in order), if it changes
    /// colours by mixing them. (`None` for an effect that is not installed, which changes nothing, and for one that is not a mixing of colours, such as exposure.)
    pub fn colour_matrix(self, values: &[PropertyValue]) -> Option<ColourMatrix> {
        match self.picture_operation(values)? {
            PictureOperation::Mix(matrix) => Some(matrix),
            PictureOperation::Exposure { .. }
            | PictureOperation::Opacity { .. }
            | PictureOperation::Blur { .. }
            | PictureOperation::Crop { .. }
            | PictureOperation::Lut(_)
            | PictureOperation::Shader { .. } => None,
        }
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
