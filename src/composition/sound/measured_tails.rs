// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tails that were **measured** by running an effect (docs/engine-phase-15-vst3-design.md, section 16): how long a plugin effect, with the values it has now, goes on making
//! sound after its input has ended.
//!
//! What a plugin says of its tail is little (13 of the 14 real VST3 effects say nothing), and what the kind of plugin guesses is a guess: the Decay of Dragonfly Hall goes from a
//! fraction of a second to ten. So the program runs the effect, with noise and then silence, and listens for how long it rings (`sound_engine::ring_out`); the result is kept
//! here, by what the effect is (the plugin, its values and its state), and [`tail_samples_of`](super::tail_samples_of) counts it where the person has not said how long. **The plan
//! reads this and never makes it**: the plan is made on the thread of the window, and a measurement is made by a thread that runs the plugin (`plugin_host::ring_outs`). Nothing
//! measured yet is the guess of the scan, which is how it was before.
//!
//! **A change of what is measured changes the sound**, so a number counts the changes ([`how_many_tails_have_been_measured`]) and the preview makes its sound again when it is not the
//! one it made the sound at. Like the list of installed plugins this is a list for the whole program.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, PoisonError, RwLock};

use super::{AppliedEffect, EffectDomain, LayerEffect, Project, layer_effect_of};
use crate::project::{EffectKind, PropertyValue, TailLength, fingerprint_of_a_state};

/// The state that a plugin saved, as far as a key goes: its bytes (which are read, to make the number), or the number that was made from them when the state was given to the effect
/// (`Effect::state_fingerprint`). **The sound is asked at every frame whether it has changed, and must not read the bytes of a state each time**; the plan, which is made when the sound
/// begins, has only the bytes. The two numbers are the same for the same bytes.
#[derive(Clone, Copy)]
pub enum TheState<'a> {
    Bytes(Option<&'a Arc<[u8]>>),
    Fingerprint(u64),
}

impl TheState<'_> {
    fn fingerprint(self) -> u64 {
        match self {
            Self::Bytes(bytes) => fingerprint_of_a_state(bytes),
            Self::Fingerprint(fingerprint) => fingerprint,
        }
    }
}

/// How long each effect that has been measured rings on, in samples, by what the effect is (see [`key_of_the_tail_of`]).
static MEASURED: LazyLock<RwLock<HashMap<u64, u64>>> = LazyLock::new(|| RwLock::new(HashMap::new()));

/// How many times what is measured has changed.
static CHANGES: AtomicU64 = AtomicU64::new(0);

/// A number that tells what a plugin effect is as far as its tail goes: the plugin (its kind, and the library and the place in it that stands in for the kind now), the values it has
/// when they are not animated, and the state that the plugin saved. **`None` for what is not a plugin that is installed**: nothing is measured for it.
///
/// (Keyframes are not part of it: a measurement is made at the values the effect has when nothing moves them. A decay that is animated is not followed; the person says its tail.)
pub fn key_of_the_tail_of(kind: EffectKind, values: &[PropertyValue], state: Option<&Arc<[u8]>>) -> Option<u64> {
    key_of_the_tail_with(kind, values, TheState::Bytes(state))
}

/// [`key_of_the_tail_of`] with the state as [`TheState`] says it.
pub fn key_of_the_tail_with(kind: EffectKind, values: &[PropertyValue], state: TheState<'_>) -> Option<u64> {
    let plugin = kind.installed_plugin()?;
    let mut hasher = DefaultHasher::new();
    kind.key().hash(&mut hasher);
    kind.version().hash(&mut hasher);
    (&plugin.library, &plugin.address).hash(&mut hasher);
    for value in values {
        value.hash_into(&mut hasher);
    }
    state.fingerprint().hash(&mut hasher);
    Some(hasher.finish())
}

/// How many samples the plugin effect with these values and this state was measured to ring on for, if it has been.
pub fn the_measured_tail_of(kind: EffectKind, values: &[PropertyValue], state: Option<&Arc<[u8]>>) -> Option<u64> {
    the_measured_tail_with(kind, values, TheState::Bytes(state))
}

/// [`the_measured_tail_of`] with the state as [`TheState`] says it. **Nothing for a synthesizer** (it is not measured, and its state may be tens of megabytes), and nothing, without
/// reading anything, while nothing has been measured.
pub fn the_measured_tail_with(kind: EffectKind, values: &[PropertyValue], state: TheState<'_>) -> Option<u64> {
    if kind.makes_sound_from_notes() || MEASURED.read().unwrap_or_else(PoisonError::into_inner).is_empty() {
        return None;
    }
    let key = key_of_the_tail_with(kind, values, state)?;
    MEASURED
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
        .copied()
}

/// Keeps that the effect whose key is `key` was measured to ring on for `samples` samples. The sound is a different sound from now on if that is not what was known.
pub fn note_the_measured_tail(key: u64, samples: u64) {
    let before = MEASURED
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key, samples);
    if before != Some(samples) {
        CHANGES.fetch_add(1, Ordering::SeqCst);
    }
}

/// A number that is different whenever what has been measured has changed.
pub fn how_many_tails_have_been_measured() -> u64 {
    CHANGES.load(Ordering::SeqCst)
}

/// Forgets every measurement (a test begins and ends with none; and the maintainer's scan of the plugins again: a plugin that has changed is not the plugin that was measured).
pub fn forget_the_measured_tails() {
    let mut measured = MEASURED.write().unwrap_or_else(PoisonError::into_inner);
    if !measured.is_empty() {
        measured.clear();
        CHANGES.fetch_add(1, Ordering::SeqCst);
    }
}

/// An effect whose tail is to be measured: the effect, as it is when nothing moves its values, and the key that its measurement is kept by.
#[derive(Clone, Debug)]
pub struct TailToMeasure {
    pub key: u64,
    pub effect: LayerEffect,
}

/// The effects of the sound of `project` whose tail is to be measured, **each once**: a plugin that is installed and makes sound from sound (not a synthesizer, whose notes end
/// when they end), that has no length that its person said, and has not been measured at the values it has.
pub fn the_tails_to_measure_in(project: &Project) -> Vec<TailToMeasure> {
    let applied = project.effects_applied_to_every_clip();
    let wants_one = |applied: &AppliedEffect<'_>| {
        let effect = applied.effect;
        effect.kind().domain() == EffectDomain::Sound
            && !effect.kind().makes_sound_from_notes()
            && effect.tail_length() == TailLength::Automatic
            && key_of_the_tail_with(
                effect.kind(),
                effect.values(),
                TheState::Fingerprint(effect.state_fingerprint()),
            )
            .is_some_and(|key| !is_measured(key))
    };
    if !applied.values().flatten().any(wants_one) {
        return Vec::new();
    }
    let animations = project.resolved_animations();
    let mut seen = HashSet::new();
    let mut wanted = Vec::new();
    for track in project.tracks() {
        for entry in track.entries() {
            for applied in applied
                .get(&entry.id())
                .into_iter()
                .flatten()
                .filter(|each| wants_one(each))
            {
                let mut effect = layer_effect_of(project, &animations, applied);
                effect.animation = Default::default();
                let Some(key) = key_of_the_tail_of(effect.kind, &effect.values, effect.state.as_ref()) else {
                    continue;
                };
                if seen.insert(key) {
                    wanted.push(TailToMeasure { key, effect });
                }
            }
        }
    }
    wanted
}

/// Whether the effect whose key is `key` has been measured.
pub fn is_measured(key: u64) -> bool {
    MEASURED
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .contains_key(&key)
}
