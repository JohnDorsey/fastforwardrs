// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Effects in a project file.
//!
//! * `plugins` has **a snapshot of the descriptor of each type of effect that the project uses** (see [`EffectDescriptor`]), so that the project
//!   can be read, shown and edited without the effect being installed: the key and the version, what it is called and does, and each of its
//!   values with its id, its kind, what it begins as and what its control looks like.
//! * An effect (of a clip, a track or a group) says the key (and version) of its type, a number of its own in its chain (`instance`), the values
//!   that are not what they begin as, **by the id of each**, and anything it has that this version of the program does not know, which is kept
//!   as it was.
//! * A rack of the keyframes of a value of an effect says the id of the value and the `instance` of the effect (in the chain of what the keyframes
//!   are of), see `drawers`.
//!
//! Files that were written before the effects were open say `hue shift` where this says `builtin:hue_shift` and list the values in order: they are
//! read by the table of [`super::legacy_effects`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::document::KeyframeValueDocument;
use super::{legacy_effects, value_document_of, value_from_document};
use crate::project::{
    ChainEntry, ChoiceOption, Effect, EffectDescriptor, EffectDomain, EffectInstanceId, EffectKind, Effects,
    MOST_BYTES_OF_A_PLUGIN_STATE, MOST_BYTES_OF_AN_IMAGE_OF_A_SHADER, MOST_EFFECTS_ON_ONE_THING,
    MOST_IMAGES_A_SHADER_BRINGS, ParameterControl, ParameterDescriptor, Project, ShaderImage, ShaderLanguage,
    ShaderSource, UnknownFields, ValueKind, ValueScale,
};

/// What a type of effect is, as a file says it: a snapshot of its descriptor.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct PluginDocument {
    /// What names the type of effect (see [`EffectDescriptor::key`]).
    pub key: String,
    /// The major and the minor number of its version.
    pub version: [u32; 2],
    /// What it is called, in lower case.
    pub name: String,
    /// What it does, in a few words. Left out when it says nothing.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// The heading it is listed under in the menu that effects are chosen from. Left out when it has none, and in the files of versions before 13.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub category: String,
    pub domain: DomainDocument,
    /// Whether it changes each pixel of a picture by itself, whatever the pixels round it are. Left out when it does not.
    #[serde(default, skip_serializing_if = "is_false")]
    pub changes_each_pixel_by_itself: bool,
    /// Its values, in the order they are kept and shown in. Left out when it has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<ParameterDocument>,
    /// The shader that runs the type of effect, if it is one (format 16): `{ "language": "isf", "source": "..." }`, and (format 17) `"images": [ { "name": "...", "data": "<base 64>" } ]` if
    /// it brings images with it. One of a language that this version does not know is **kept as it was**, in `unknown`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shader: Option<Value>,
    /// What this version of the program does not know of the type of effect, which is kept as it was.
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

fn is_false(flag: &bool) -> bool {
    !*flag
}

fn yes() -> bool {
    true
}

fn is_true(flag: &bool) -> bool {
    *flag
}

/// What a type of effect works on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DomainDocument {
    Picture,
    Sound,
}

/// What kind of value a value of an effect is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ParameterKindDocument {
    Number,
    Pair,
    Colour,
    Flag,
    Words,
}

/// One of the values of a type of effect, as a file says it.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct ParameterDocument {
    /// What names the value, for as long as the type of effect exists (see [`ParameterDescriptor::id`]).
    pub id: String,
    /// What the value is called when it is told apart from the other properties of a clip or a track, in lower case. Read as the id when it is
    /// not there.
    #[serde(default)]
    pub name: Option<String>,
    /// What the control of the value says. Read as the name when it is not there.
    #[serde(default)]
    pub label: Option<String>,
    #[serde(rename = "type")]
    pub kind: ParameterKindDocument,
    /// What the value is when the effect is put on.
    pub default: KeyframeValueDocument,
    /// The least and the most that the control of a number shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<[f32; 2]>,
    /// What is written after a number. Left out when there is nothing.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unit: String,
    /// The most decimals a number is shown with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decimals: Option<u8>,
    /// `"logarithmic"` for a number that is laid along its slider by ratios. Left out for the usual (linear) one, and in the files of versions before 13. (Read as a
    /// value of JSON so that a scale that this version does not know is kept, and not refused: see [`scale_from_document`].)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<Value>,
    /// How the control of the value works: `"slider"`, `"integer"`, `"toggle"`, `{"choice": [{"id": ..., "label": ...}]}` or `{"resource": "cube"}`. Left out for the
    /// control that the kind of value has, and in the files of versions before 13. (Read as a value of JSON for the same reason as the scale.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control: Option<Value>,
    /// What this version of the program does not know of the value, which is kept as it was.
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

/// An effect that a clip, a track or a group has.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct EffectDocument {
    /// The key of its type of effect (see [`EffectKind::key`]). (A file that was written before the effects were open says what the effect
    /// is called here, and has no colon in it: see [`legacy_effects`].)
    pub effect: String,
    /// The major and the minor number of the version of the type of effect that the effect was made with, which says which of the snapshots in
    /// `plugins` is its type's when there are several of one key. Left out in the files that were written before the effects were open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<[u32; 2]>,
    /// The name the effect has in its chain (see [`crate::project::EffectInstanceId`]), which is what the racks of keyframes of its values say
    /// they are of. A chain may have several effects of one kind, and then it is this that tells them apart. Left out in the files of
    /// versions before 11, which have at most one effect of each kind: the effects of a chain are named in the order they are in then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<u32>,
    /// What its values are when they are not animated, by the id of each. Left out when they are what the effect begins with. (The keyframes of
    /// its values are racks, like any other.)
    #[serde(default, skip_serializing_if = "ValuesDocument::is_empty")]
    pub values: ValuesDocument,
    /// Whether the effect is done: `false` for one that is switched off (bypassed). Left out when it is on, and in the files of versions before 13.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
    /// What the plugin of the effect saved of what is not one of its values (its state), in base 64, which only the plugin can read. Left out when it saved none, and in the files of
    /// versions before 19.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// What this version of the program does not know of the effect, which is kept as it was.
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

/// The values of an effect: by the id of each, or (in the files that were written before the effects were open) a list, one for each of
/// the values of the effect, in the order that the program had them in then.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum ValuesDocument {
    ById(BTreeMap<String, KeyframeValueDocument>),
    InOrder(Vec<KeyframeValueDocument>),
}

impl Default for ValuesDocument {
    fn default() -> Self {
        Self::ById(BTreeMap::new())
    }
}

impl ValuesDocument {
    fn is_empty(&self) -> bool {
        match self {
            Self::ById(values) => values.is_empty(),
            Self::InOrder(values) => values.is_empty(),
        }
    }
}

// --- reading ---

/// The types of effect that a file's `plugins` make, which the effects of the file are of.
pub(super) struct KindsOfEffects {
    /// Each snapshot, by its key and version, and the kind it is.
    snapshots: Vec<(String, (u32, u32), EffectKind)>,
}

impl KindsOfEffects {
    /// The kinds that the snapshots in `plugins` make. It is an error for one to be a descriptor that cannot be used, or for two to have
    /// the same key and version.
    pub(super) fn from_documents(plugins: Vec<PluginDocument>) -> Result<Self, String> {
        let mut snapshots: Vec<(String, (u32, u32), EffectKind)> = Vec::with_capacity(plugins.len());
        for (number, document) in plugins.into_iter().enumerate() {
            let describe = |reason: String| format!("plugin number {}: {reason}", number + 1);
            let key = document.key.clone();
            let version = (document.version[0], document.version[1]);
            let kind = descriptor_from_document(document)
                .and_then(EffectKind::from_snapshot)
                .map_err(describe)?;
            if snapshots
                .iter()
                .any(|(other, other_version, _)| *other == key && *other_version == version)
            {
                return Err(describe(format!(
                    "\"{key}\" version {}.{} is described twice",
                    version.0, version.1
                )));
            }
            snapshots.push((key, version, kind));
        }
        Ok(Self { snapshots })
    }

    /// The kind of effect that `document` is an effect of.
    fn kind_of(&self, document: &EffectDocument) -> Result<EffectKind, String> {
        let key = document.effect.as_str();
        if !key.contains(':') {
            return legacy_effects::the_effect_called(key)
                .ok_or_else(|| format!("\"{key}\" is not an effect there is"));
        }
        let with_the_key: Vec<&(String, (u32, u32), EffectKind)> = self
            .snapshots
            .iter()
            .filter(|(snapshot, ..)| snapshot.as_str() == key)
            .collect();
        let wanted_version = document.version.map(|version| (version[0], version[1]));
        let described = match wanted_version {
            Some(version) => with_the_key
                .iter()
                .find(|(_, snapshot_version, _)| *snapshot_version == version),
            None if with_the_key.len() > 1 => {
                return Err(format!(
                    "the effect \"{key}\" does not say which version it is of, and the file has {} of them in `plugins`",
                    with_the_key.len()
                ));
            }
            None => with_the_key.first(),
        };
        if let Some((_, _, kind)) = described {
            return Ok(*kind);
        }
        // (A file that is written by hand may leave a snapshot of an effect of the program out.)
        EffectKind::built_in_with_key(key)
            .filter(|own| wanted_version.is_none_or(|version| own.version().0 == version.0))
            .ok_or_else(|| {
                format!("the effect \"{key}\" is not described in `plugins`, and the program does not have it")
            })
    }
}

/// The descriptor that the snapshot `document` says.
fn descriptor_from_document(document: PluginDocument) -> Result<EffectDescriptor, String> {
    let parameters = document
        .params
        .into_iter()
        .map(|parameter| {
            parameter_from_document(parameter).map_err(|reason| format!("the effect \"{}\": {reason}", document.key))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut unknown = document.unknown;
    let shader = match document.shader {
        Some(Value::Object(object))
            if object.len() == 2 + usize::from(object.contains_key("images"))
                && object
                    .get("language")
                    .and_then(Value::as_str)
                    .and_then(ShaderLanguage::from_name_in_a_file)
                    .is_some()
                && object.get("source").is_some_and(Value::is_string) =>
        {
            let language = object["language"]
                .as_str()
                .and_then(ShaderLanguage::from_name_in_a_file)
                .expect("checked above");
            Some(ShaderSource {
                language,
                text: object["source"].as_str().unwrap_or_default().to_owned().into(),
                images: shader_images_from_document(object.get("images"))
                    .map_err(|reason| format!("the effect \"{}\": the images of its shader: {reason}", document.key))?,
            })
        }
        // (A language that this version does not know, or a shader that says more than it should: kept as it was, to be written back.)
        Some(other) => {
            unknown.insert("shader".to_owned(), other);
            None
        }
        None => None,
    };
    Ok(EffectDescriptor {
        shader,
        key: document.key.into(),
        version: (document.version[0], document.version[1]),
        name: document.name.into(),
        description: document.description.into(),
        category: document.category.into(),
        domain: match document.domain {
            DomainDocument::Picture => EffectDomain::Picture,
            DomainDocument::Sound => EffectDomain::Sound,
        },
        changes_each_pixel_by_itself: document.changes_each_pixel_by_itself,
        parameters: parameters.into(),
        unknown_fields: UnknownFields::new(unknown),
    })
}

fn parameter_from_document(document: ParameterDocument) -> Result<ParameterDescriptor, String> {
    let value_kind = match document.kind {
        ParameterKindDocument::Number => ValueKind::Number,
        ParameterKindDocument::Pair => ValueKind::Pair,
        ParameterKindDocument::Colour => ValueKind::Colour,
        ParameterKindDocument::Flag => ValueKind::Flag,
        ParameterKindDocument::Words => ValueKind::Words,
    };
    let default = value_from_document(document.default, value_kind)
        .map_err(|reason| format!("the value \"{}\": {reason}", document.id))?;
    let name = document.name.unwrap_or_else(|| document.id.clone());
    let label = document.label.unwrap_or_else(|| name.clone());
    let mut unknown = document.unknown;
    let scale = scale_from_document(document.scale, &mut unknown);
    let control = control_from_document(document.control, &mut unknown);
    Ok(ParameterDescriptor {
        id: document.id.into(),
        name: name.into(),
        label: label.into(),
        default,
        range: document.range.map(|[least, most]| (least, most)),
        unit: document.unit.into(),
        decimals: document.decimals,
        unknown_fields: UnknownFields::new(unknown),
        scale,
        control,
    })
}

/// The scale that the document says. One that this version does not know is the usual one, and is **kept in `unknown`** to be written back as it was.
fn scale_from_document(document: Option<Value>, unknown: &mut BTreeMap<String, Value>) -> ValueScale {
    match document {
        None => ValueScale::Linear,
        Some(Value::String(name)) if name == "linear" => ValueScale::Linear,
        Some(Value::String(name)) if name == "logarithmic" => ValueScale::Logarithmic,
        Some(other) => {
            unknown.insert("scale".to_owned(), other);
            ValueScale::Linear
        }
    }
}

/// The control that the document says. One that this version does not know is the one that the kind of value has, and is **kept in `unknown`** to be written back as it
/// was.
fn control_from_document(document: Option<Value>, unknown: &mut BTreeMap<String, Value>) -> ParameterControl {
    let Some(document) = document else {
        return ParameterControl::Automatic;
    };
    let known = match &document {
        Value::String(name) => match name.as_str() {
            "slider" => Some(ParameterControl::Slider),
            "integer" => Some(ParameterControl::Integer),
            "toggle" => Some(ParameterControl::Toggle),
            "multiline" => Some(ParameterControl::Multiline),
            "font" => Some(ParameterControl::Font),
            "points_of_the_picture_height" => Some(ParameterControl::PointsOfThePictureHeight),
            _ => None,
        },
        Value::Object(object) if object.len() == 1 => {
            if let Some(Value::Array(options)) = object.get("choice") {
                options
                    .iter()
                    .map(|option| {
                        Some(ChoiceOption {
                            id: option.get("id")?.as_str()?.to_owned().into(),
                            label: option.get("label")?.as_str()?.to_owned().into(),
                        })
                    })
                    .collect::<Option<Vec<_>>>()
                    .map(|options| ParameterControl::Choice(options.into()))
            } else if let Some(Value::String(kind)) = object.get("resource") {
                Some(ParameterControl::Resource(kind.clone().into()))
            } else {
                None
            }
        }
        _ => None,
    };
    known.unwrap_or_else(|| {
        unknown.insert("control".to_owned(), document);
        ParameterControl::Automatic
    })
}

/// The effect that `document` describes, which is of the kind `kind`: with the values it says and what it begins with for the others.
fn effect_from_document(document: EffectDocument, kind: EffectKind) -> Result<Effect, String> {
    let mut values = kind.default_values();
    let mut say = |id: &str, value: KeyframeValueDocument| -> Result<(), String> {
        let slot = kind
            .slot_of_the_value_with_id(id)
            .ok_or_else(|| format!("the {kind} effect has no value with the id \"{id}\""))?;
        let value_kind = kind.value_kind(slot).expect("the place was just found");
        values[slot] =
            value_from_document(value, value_kind).map_err(|reason| format!("the {kind} effect: {reason}"))?;
        Ok(())
    };
    match document.values {
        ValuesDocument::ById(by_id) => {
            for (id, value) in by_id {
                say(&id, value)?;
            }
        }
        ValuesDocument::InOrder(in_order) if in_order.is_empty() => {}
        ValuesDocument::InOrder(in_order) => {
            let ids = legacy_effects::the_ids_of_the_values_in_the_order_old_files_had_them(kind.key())
                .ok_or_else(|| format!("the values of the {kind} effect are a list, which only an old file has"))?;
            if in_order.len() != ids.len() {
                return Err(format!(
                    "the values of the {kind} effect should be a list of {}, and the file's list is of {}",
                    ids.len(),
                    in_order.len()
                ));
            }
            for (id, value) in ids.into_iter().zip(in_order) {
                say(id, value)?;
            }
        }
    }
    let state = document
        .state
        .map(|text| {
            let bytes = crate::base64::decode(&text).map_err(|reason| format!("the state of the {kind} effect: {reason}"))?;
            if bytes.len() > MOST_BYTES_OF_A_PLUGIN_STATE {
                return Err(format!(
                    "the state of the {kind} effect is of {} bytes, and a project keeps at most {MOST_BYTES_OF_A_PLUGIN_STATE}",
                    bytes.len()
                ));
            }
            Ok(std::sync::Arc::<[u8]>::from(bytes))
        })
        .transpose()?;
    Effect::with_values(kind, values)
        .map(|effect| {
            effect
                .with_unknown_fields(UnknownFields::new(document.unknown))
                .with_enabled(document.enabled)
                .with_state(state)
        })
        .ok_or_else(|| format!("the values of the {kind} effect are not the kind it has"))
}

/// The effects that the documents describe, in their order, each named as the document names it: a file of version 10 names none, and
/// they are named in the order they are in (as they would be if they were put on one after another). It is an error to put an effect on
/// something that cannot have it (a sound), to name some of the effects of a chain and not the others, or to give two the same name.
pub(super) fn effects_from_documents(
    documents: Vec<EffectDocument>,
    kinds: &KindsOfEffects,
    can_have: impl Fn(EffectKind) -> bool,
) -> Result<Vec<Effect>, String> {
    if documents.len() > MOST_EFFECTS_ON_ONE_THING {
        return Err(format!(
            "it has {} effects, and the most one can have is {MOST_EFFECTS_ON_ONE_THING}",
            documents.len()
        ));
    }
    let named = documents.iter().filter(|document| document.instance.is_some()).count();
    if named != 0 && named != documents.len() {
        return Err("some of its effects have a name in its chain and some have not".to_owned());
    }
    let mut effects: Vec<Effect> = Vec::with_capacity(documents.len());
    for (place, document) in documents.into_iter().enumerate() {
        let instance = EffectInstanceId::new(match document.instance {
            Some(number) => number,
            None => u32::try_from(place).map_err(|_| "it has too many effects".to_owned())?,
        });
        let kind = kinds.kind_of(&document)?;
        let effect = effect_from_document(document, kind)?.named(instance);
        if !can_have(kind) {
            return Err(format!("it cannot have the {kind} effect"));
        }
        if effects.iter().any(|other| other.instance() == instance) {
            return Err(format!("it has two effects that are named {instance}"));
        }
        effects.push(effect);
    }
    Ok(effects)
}

/// The chain that the documents of the effects of a track make (a track has no badges in its chain).
pub(super) fn chain_of_the_effects_from_documents(
    documents: Vec<EffectDocument>,
    kinds: &KindsOfEffects,
    can_have: impl Fn(EffectKind) -> bool,
) -> Result<Effects, String> {
    Effects::from_entries(
        effects_from_documents(documents, kinds, can_have)?
            .into_iter()
            .map(ChainEntry::Effect),
    )
    .ok_or_else(|| "it has two effects with the same name".to_owned())
}

// --- writing ---

/// The documents of the effects `effects`, in their order, with the values left out of each that has the values it begins with.
pub(super) fn effect_documents_of(effects: &Effects) -> Vec<EffectDocument> {
    effects
        .iter()
        .map(|effect| {
            let kind = effect.kind();
            let values: BTreeMap<String, KeyframeValueDocument> = kind
                .parameters()
                .iter()
                .zip(effect.values())
                .filter(|(parameter, value)| parameter.value_it_begins_as() != **value)
                .map(|(parameter, value)| (parameter.id.to_string(), value_document_of(value)))
                .collect();
            EffectDocument {
                effect: kind.key().to_owned(),
                version: Some([kind.version().0, kind.version().1]),
                instance: Some(effect.instance().number()),
                values: ValuesDocument::ById(values),
                enabled: effect.is_enabled(),
                state: effect.state().map(|bytes| crate::base64::encode(bytes)),
                unknown: effect.unknown_fields().fields().clone(),
            }
        })
        .collect()
}

/// The kinds of effect that the effects of `project` (of its clips, its tracks and its groups) are of, each once, in the order of their keys
/// and versions.
fn kinds_used_by(project: &Project) -> Vec<EffectKind> {
    let chains = project
        .tracks()
        .iter()
        .flat_map(|track| std::iter::once(track.effects()).chain(track.entries().iter().map(|entry| entry.effects())))
        .chain(project.groups().iter().map(|group| group.effects()));
    let mut kinds: Vec<EffectKind> = chains.flat_map(|chain| chain.iter().map(Effect::kind)).collect();
    kinds.sort();
    kinds.dedup();
    kinds
}

/// The snapshots of the descriptors of the kinds of effect that `project` uses, for `plugins`.
pub(super) fn plugin_documents_of(project: &Project) -> Vec<PluginDocument> {
    kinds_used_by(project)
        .into_iter()
        .map(|kind| plugin_document_of(kind.descriptor()))
        .collect()
}

fn plugin_document_of(descriptor: &EffectDescriptor) -> PluginDocument {
    PluginDocument {
        key: descriptor.key.to_string(),
        version: [descriptor.version.0, descriptor.version.1],
        name: descriptor.name.to_string(),
        description: descriptor.description.to_string(),
        category: descriptor.category.to_string(),
        domain: match descriptor.domain {
            EffectDomain::Picture => DomainDocument::Picture,
            EffectDomain::Sound => DomainDocument::Sound,
        },
        changes_each_pixel_by_itself: descriptor.changes_each_pixel_by_itself,
        params: descriptor.parameters.iter().map(parameter_document_of).collect(),
        shader: descriptor.shader.as_ref().map(|shader| {
            let mut object = serde_json::json!({
                "language": shader.language.name_in_a_file(),
                "source": shader.text.as_ref(),
            });
            if !shader.images.is_empty() {
                object["images"] = Value::Array(
                    shader
                        .images
                        .iter()
                        .map(|image| {
                            serde_json::json!({
                                "name": image.name(),
                                "data": crate::base64::encode(image.bytes()),
                            })
                        })
                        .collect(),
                );
            }
            object
        }),
        unknown: descriptor.unknown_fields.fields().clone(),
    }
}

/// The images that a shader brings, from the `images` of its document: a list of `{ "name", "data" }` of at most as many as a shader may bring, each of at most as many bytes as an image
/// may have. (Whether the bytes are a picture is for whoever runs the shader to find out; here they are bytes.)
fn shader_images_from_document(images: Option<&Value>) -> Result<Vec<ShaderImage>, String> {
    let Some(images) = images else {
        return Ok(Vec::new());
    };
    let Some(items) = images.as_array() else {
        return Err("it is not a list".to_owned());
    };
    if items.len() > MOST_IMAGES_A_SHADER_BRINGS {
        return Err(format!(
            "it has {} and {MOST_IMAGES_A_SHADER_BRINGS} are the most",
            items.len()
        ));
    }
    items
        .iter()
        .enumerate()
        .map(|(number, item)| {
            let place = number + 1;
            let object = item
                .as_object()
                .filter(|object| object.len() == 2)
                .ok_or_else(|| format!("image {place} is not a name and some data"))?;
            let (Some(name), Some(data)) = (
                object.get("name").and_then(Value::as_str),
                object.get("data").and_then(Value::as_str),
            ) else {
                return Err(format!("image {place} is not a name and some data"));
            };
            // (A base 64 text that is more than a third as long again as the most bytes cannot be that many bytes: not decoded, to keep a file from making the program write a gigabyte.)
            if data.len() > MOST_BYTES_OF_AN_IMAGE_OF_A_SHADER.div_ceil(3) * 4 {
                return Err(format!(
                    "image {place} has more than {MOST_BYTES_OF_AN_IMAGE_OF_A_SHADER} bytes"
                ));
            }
            let bytes = crate::base64::decode(data).map_err(|reason| format!("image {place}: {reason}"))?;
            if bytes.len() > MOST_BYTES_OF_AN_IMAGE_OF_A_SHADER {
                return Err(format!(
                    "image {place} has more than {MOST_BYTES_OF_AN_IMAGE_OF_A_SHADER} bytes"
                ));
            }
            Ok(ShaderImage::new(name, bytes))
        })
        .collect()
}

fn parameter_document_of(parameter: &ParameterDescriptor) -> ParameterDocument {
    ParameterDocument {
        id: parameter.id.to_string(),
        name: Some(parameter.name.to_string()),
        label: Some(parameter.label.to_string()),
        kind: match parameter.value_kind() {
            ValueKind::Number => ParameterKindDocument::Number,
            ValueKind::Pair => ParameterKindDocument::Pair,
            ValueKind::Colour => ParameterKindDocument::Colour,
            ValueKind::Flag => ParameterKindDocument::Flag,
            ValueKind::Words => ParameterKindDocument::Words,
        },
        default: value_document_of(&parameter.value_it_begins_as()),
        range: parameter.range.map(|(least, most)| [least, most]),
        unit: parameter.unit.to_string(),
        decimals: parameter.decimals,
        scale: match parameter.scale {
            ValueScale::Linear => None,
            ValueScale::Logarithmic => Some(Value::String("logarithmic".to_owned())),
        },
        control: control_document_of(&parameter.control),
        unknown: parameter.unknown_fields.fields().clone(),
    }
}

/// What the document of a control says: nothing for the control that the kind of value has.
fn control_document_of(control: &ParameterControl) -> Option<Value> {
    match control {
        ParameterControl::Automatic => None,
        ParameterControl::Slider => Some(Value::String("slider".to_owned())),
        ParameterControl::Integer => Some(Value::String("integer".to_owned())),
        ParameterControl::Toggle => Some(Value::String("toggle".to_owned())),
        ParameterControl::Choice(options) => Some(serde_json::json!({
            "choice": options
                .iter()
                .map(|option| serde_json::json!({"id": option.id, "label": option.label}))
                .collect::<Vec<_>>()
        })),
        ParameterControl::Resource(kind) => Some(serde_json::json!({ "resource": kind })),
        ParameterControl::Multiline => Some(Value::String("multiline".to_owned())),
        ParameterControl::Font => Some(Value::String("font".to_owned())),
        ParameterControl::PointsOfThePictureHeight => Some(Value::String("points_of_the_picture_height".to_owned())),
    }
}
