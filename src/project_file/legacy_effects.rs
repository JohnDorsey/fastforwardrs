// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! How the files that were written before the effects were open named the effects and their values: by the names of the table that the program
//! had then (version 10 of the file, and version 11 as it was written while the effects were being given names of their own in their chains), which are
//! the names of the effects that the program has in itself. A file of those says `hue shift` where a file of today says `builtin:hue_shift`, and
//! `hue shift` for the value that a file of today calls `degrees`. **This table does not change**: what it says is what the old files said.

use crate::project::{Effect, EffectKind, HUE_SHIFT_KEY, PropertyId};

/// An effect as the old files named it, with the key it is now, and its values as the old files named them, with the ids they have now.
struct OldNames {
    effect: &'static str,
    key: &'static str,
    values: &'static [(&'static str, &'static str)],
}

const OLD_NAMES: [OldNames; 1] = [OldNames {
    effect: "hue shift",
    key: HUE_SHIFT_KEY,
    values: &[("hue shift", "degrees")],
}];

/// The effect of the program that an old file means by `name`, if there is one.
pub(super) fn the_effect_called(name: &str) -> Option<EffectKind> {
    let old = OLD_NAMES.iter().find(|old| old.effect == name)?;
    EffectKind::built_in_with_key(old.key)
}

/// The effect of the program that has a value that an old file calls `name`, if there is one.
pub(super) fn the_effect_that_has_a_value_called(name: &str) -> Option<EffectKind> {
    let old = OLD_NAMES
        .iter()
        .find(|old| old.values.iter().any(|(called, _)| *called == name))?;
    EffectKind::built_in_with_key(old.key)
}

/// The ids that the values of the effect of the program with the key `key` have now, in the order that an old file listed them in.
pub(super) fn the_ids_of_the_values_in_the_order_old_files_had_them(key: &str) -> Option<Vec<&'static str>> {
    let old = OLD_NAMES.iter().find(|old| old.key == key)?;
    Some(old.values.iter().map(|(_, id)| *id).collect())
}

/// The property of `effect` that an old file calls `name`, if it has one.
pub(super) fn the_value_of_the_effect_called(effect: &Effect, name: &str) -> Option<PropertyId> {
    let old = OLD_NAMES.iter().find(|old| old.key == effect.kind().key())?;
    let (_, id) = old.values.iter().find(|(called, _)| *called == name)?;
    effect.property_with_id(id)
}
