// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Effects: what is done to the picture of a clip, or of a track, on its way to the finished picture.
//!
//! A clip or a track has the effects that were put on it, one after another in the order they were put on in (or the order they were put in
//! since: they can be moved, see [`Effects::move_before`]). It may have several effects of one kind: two hue shifts, say, each with values
//! of its own. What an effect does is set by its *values*, which are properties like any other (see [`PropertyId`]): each can be animated, with
//! keyframes, and is changed in the properties panel with the same stopwatch. An effect that is put on a clip or a track has the values it begins
//! with (see [`EffectKind::default_values`]), and is taken off with its keyframes.
//!
//! **Each effect in a chain is named by an [`EffectInstanceId`]**, which is what its values are named by too (see
//! [`PropertyId::EffectValue`]): a value is *the value `slot` of the effect `instance`*, whatever kind of effect that is. The names are those of
//! one chain: two chains (of a clip and of its track, say) may use the same, and keyframes are kept for a clip or a track, not for a name.
//!
//! A group has effects too, and what has effects has a *chain*: its effects, one after another, and among them the **badges** that say
//! where the effects of something else are applied to it (see [`Badge`]). A clip has a badge for each tag it is in, and one that stands
//! for the effects of its track; a group that is in a tag has a badge for it. The chain says in what order everything is applied.
//!
//! **The kinds of effect are open**: what there is to know about a *kind* of effect is data, a descriptor (see [`EffectDescriptor`]): its key,
//! its name, what it does, its values and what it can be put on. The program has some in itself (see [`EffectKind::BUILT_IN`]) and a project
//! file can bring others, which are kept with all their values and keyframes and do nothing to a picture until the program has them (see
//! [`EffectKind::is_installed`]). (So far the one effect that the program has is the hue shift, which changes colours; sound, and what is
//! seen of other pictures, come with the effects that need them.)

use std::sync::Arc;

use super::animation::{PropertyId, PropertyValue};
use super::groups::GroupId;

mod built_in;
mod descriptor;
mod installed;
mod kind;

pub use built_in::{HUE_SHIFT_KEY, ImportedPicture, LutOperation, LutSpace, PictureOperation};
pub use descriptor::{
    ChoiceOption, EffectDescriptor, EffectDomain, MOST_BYTES_OF_A_SHADER, MOST_BYTES_OF_AN_IMAGE_OF_A_SHADER,
    MOST_IMAGES_A_SHADER_BRINGS, MOST_PIXELS_ACROSS_OF_AN_IMAGE_OF_A_SHADER, MOST_VALUES_OF_AN_EFFECT,
    ParameterControl, ParameterDescriptor, ShaderImage, ShaderLanguage, ShaderSource, UnknownFields, ValueScale,
};
pub use installed::{
    EffectPreset, InstalledPlugin, PluginAddress, installed_plugin_for, installed_plugins, installed_shaders,
    set_installed_plugins, set_installed_shaders,
};
pub use kind::EffectKind;

use std::fmt;

/// Which of the effects of a chain an effect is: a number that no other effect of the chain has, and that the effect keeps for as long as it is in
/// the chain (the one that is put on a chain that has none is [`Self::FIRST`], and each after it has a number more than the largest there is
/// then). It names the effect's values (see [`PropertyId::EffectValue`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EffectInstanceId(u32);

impl EffectInstanceId {
    /// The name of the effect that is put on a chain that has no effects.
    pub const FIRST: Self = Self(0);

    pub const fn new(number: u32) -> Self {
        Self(number)
    }

    pub const fn number(self) -> u32 {
        self.0
    }
}

impl fmt::Display for EffectInstanceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "effect {}", self.0)
    }
}

/// An effect that a clip or a track has: which, and what its values are when they are not animated.
#[derive(Clone, Debug, PartialEq)]
pub struct Effect {
    instance: EffectInstanceId,
    kind: EffectKind,
    /// The value of each of the values of the kind of effect, in the order of [`EffectKind::parameters`].
    values: Vec<PropertyValue>,
    /// What a project file said of the effect that this version of the program does not know, to be written back as it was.
    unknown_fields: UnknownFields,
    /// Whether the effect is done. An effect that is **switched off** (bypassed) stays in its chain with its values and keyframes and is not applied, not exported and not
    /// missed if it is one that the program does not have. It is on when it is put on.
    enabled: bool,
    /// What a plugin keeps that is not one of its values, saved by the plugin itself (its `clap.state`), if its window made it say so (phase 10). It is bytes that only the plugin
    /// can read; the program keeps them as they are and gives them back to the plugin, in the sound and in the window.
    state: Option<Arc<[u8]>>,
    /// A number made from the bytes of the state (see [`fingerprint_of_a_state`]), so that what is heard can be told to have changed without reading them again.
    state_fingerprint: u64,
}

/// A number that is different whenever the bytes are (to all intents and purposes): it is worked out once, when an effect is given a state, and not at every frame that the sound asks
/// whether anything is different.
fn fingerprint_of_a_state(state: Option<&Arc<[u8]>>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    state.map(|bytes| &bytes[..]).hash(&mut hasher);
    hasher.finish()
}

impl Effect {
    /// The effect of this kind as it is when it is put on. (It is named [`EffectInstanceId::FIRST`] until a chain gives it the name it has
    /// there: see [`Effects::put_on`].)
    pub fn new(kind: EffectKind) -> Self {
        Self {
            instance: EffectInstanceId::FIRST,
            kind,
            values: kind.default_values(),
            unknown_fields: UnknownFields::none(),
            enabled: true,
            state: None,
            state_fingerprint: fingerprint_of_a_state(None),
        }
    }

    /// The effect of this kind with these values, if there is one for each of its values and each is the kind of value it has.
    pub fn with_values(kind: EffectKind, values: Vec<PropertyValue>) -> Option<Self> {
        let right_values = values.len() == kind.value_count()
            && values
                .iter()
                .enumerate()
                .all(|(slot, value)| kind.value_kind(slot) == Some(value.kind()));
        right_values.then_some(Self {
            instance: EffectInstanceId::FIRST,
            kind,
            values,
            unknown_fields: UnknownFields::none(),
            enabled: true,
            state: None,
            state_fingerprint: fingerprint_of_a_state(None),
        })
    }

    /// The same effect, switched on or off.
    #[must_use]
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Whether the effect is switched on (see [`Self::with_enabled`]).
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// The same effect, with the state that its plugin saved (see [`Self::state`]).
    #[must_use]
    pub fn with_state(mut self, state: Option<Arc<[u8]>>) -> Self {
        self.state_fingerprint = fingerprint_of_a_state(state.as_ref());
        self.state = state;
        self
    }

    /// A number made from the bytes of the state: different when they are (see [`Self::state`]), and the same for an effect that has none.
    pub fn state_fingerprint(&self) -> u64 {
        self.state_fingerprint
    }

    /// What its plugin saved of what is not one of its values, if it did (phase 10); only a plugin has any.
    pub fn state(&self) -> Option<&Arc<[u8]>> {
        self.state.as_ref()
    }

    /// The same effect, named `instance`.
    #[must_use]
    pub fn named(mut self, instance: EffectInstanceId) -> Self {
        self.instance = instance;
        self
    }

    /// The same effect, with what a file said of it that this version does not know.
    #[must_use]
    pub fn with_unknown_fields(mut self, unknown_fields: UnknownFields) -> Self {
        self.unknown_fields = unknown_fields;
        self
    }

    pub fn instance(&self) -> EffectInstanceId {
        self.instance
    }

    pub fn kind(&self) -> EffectKind {
        self.kind
    }

    /// What a file said of the effect that this version does not know.
    pub fn unknown_fields(&self) -> &UnknownFields {
        &self.unknown_fields
    }

    /// The values the effect has when they are not animated, one for each of its values, in order.
    pub fn values(&self) -> &[PropertyValue] {
        &self.values
    }

    /// The property that is the value in place `slot` of this effect, if it has one there.
    pub fn property(&self, slot: usize) -> Option<PropertyId> {
        self.kind.value_kind(slot)?;
        Some(PropertyId::effect_value(
            self.kind,
            self.instance,
            u8::try_from(slot).ok()?,
        ))
    }

    /// The property that is the value of this effect that has the id `id`, if it has one.
    pub fn property_with_id(&self, id: &str) -> Option<PropertyId> {
        self.property(self.kind.slot_of_the_value_with_id(id)?)
    }

    /// The properties that are the values of this effect, in order.
    pub fn properties(&self) -> impl Iterator<Item = PropertyId> + '_ {
        (0..self.kind.value_count()).filter_map(|slot| self.property(slot))
    }

    /// Where the value that `property` is, is among the values of this effect, if it is one of them.
    fn slot_of(&self, property: PropertyId) -> Option<usize> {
        let PropertyId::EffectValue { effect, instance, slot } = property else {
            return None;
        };
        let slot = usize::from(slot);
        (effect == self.kind && instance == self.instance && slot < self.kind.value_count()).then_some(slot)
    }

    /// The value of `property` when it is not animated, if it is one of the effect's.
    pub fn value_of(&self, property: PropertyId) -> Option<&PropertyValue> {
        self.values.get(self.slot_of(property)?)
    }

    /// The effect with `value` for `property`. `None` if it is not a property of the effect, or not the kind of value that it has.
    #[must_use]
    pub fn with_value(&self, property: PropertyId, value: &PropertyValue) -> Option<Self> {
        let place = self.slot_of(property)?;
        (value.kind() == property.kind()).then(|| {
            let mut values = self.values.clone();
            values[place] = value.clone();
            Self {
                instance: self.instance,
                kind: self.kind,
                values,
                unknown_fields: self.unknown_fields.clone(),
                enabled: self.enabled,
                state: self.state.clone(),
                state_fingerprint: self.state_fingerprint,
            }
        })
    }
}

/// A place in a chain where the effects of something else are applied to what has the chain, and so what stands for those effects in
/// the properties panel and in the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Badge {
    /// The effects of the track that a clip is on, all of them, one after another. (Only a clip has one: it is not kept in the chain
    /// while it is at the end of it, which is where it begins; see [`Effects`].)
    Track,
    /// The effects of a tag that what has the chain is in.
    Tag(GroupId),
}

/// What is in a chain, named so that it can be pointed at without saying where it is: an effect is its instance, and a badge is what it
/// is a badge of (there is at most one of each).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChainItemKey {
    Effect(EffectInstanceId),
    Badge(Badge),
}

/// One thing in a chain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChainItem<'chain> {
    Effect(&'chain Effect),
    Badge(Badge),
}

impl ChainItem<'_> {
    /// What names the item.
    pub fn key(&self) -> ChainItemKey {
        match self {
            Self::Effect(effect) => ChainItemKey::Effect(effect.instance()),
            Self::Badge(badge) => ChainItemKey::Badge(*badge),
        }
    }
}

/// One thing to put in a chain that is made from its parts: an effect, or a badge. (See [`Effects::from_entries`].)
#[derive(Clone, Debug, PartialEq)]
pub enum ChainEntry {
    Effect(Effect),
    Badge(Badge),
}

/// What a chain keeps: an effect, or a badge that is not at the end of the chain if it is the badge of the track.
#[derive(Clone, Debug, PartialEq)]
enum Link {
    Effect(Effect),
    Badge(Badge),
}

impl Link {
    fn key(&self) -> ChainItemKey {
        match self {
            Self::Effect(effect) => ChainItemKey::Effect(effect.instance()),
            Self::Badge(badge) => ChainItemKey::Badge(*badge),
        }
    }
}

/// The effects of a clip, of a track or of a group, in the order they are applied in, and with them the badges that say where the
/// effects of something else are applied. There may be several effects of one kind (each has a name of its own in the chain: see
/// [`EffectInstanceId`]), and there is at most one badge of each.
///
/// A chain that has nothing in it is the chain of a clip that has no effects and is in no tag: it is, as it was before there were
/// badges. The badge of the track that a clip is on is not kept while it is **at the end** of the chain, where it begins: it is the
/// last thing in the chain then without being in it, so that a chain that was never changed is the same as ever, and so that what is put
/// on the chain goes before it (the effects of a clip, and then those of its track, are what it was before badges could be moved).
/// Once it is moved to another place it is kept there, until it is moved back to the end, or the things after it are taken away.
/// (Whether a chain has the badge of a track is for what has the chain to say: it is for the clip, never for a track or a group, see
/// the `has_the_track_badge` that the methods that need it are given.)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effects {
    links: Vec<Link>,
}

/// The most effects that one clip, track or group can have (SF-11). A person who wants more is making a chain no card will run; a project file that has more is refused.
pub const MOST_EFFECTS_ON_ONE_THING: usize = 64;

/// The effects of a chain, one after another: what [`Effects::iter`] gives.
pub struct EffectsIter<'chain> {
    links: std::slice::Iter<'chain, Link>,
    left: usize,
}

impl<'chain> Iterator for EffectsIter<'chain> {
    type Item = &'chain Effect;

    fn next(&mut self) -> Option<Self::Item> {
        for link in self.links.by_ref() {
            if let Link::Effect(effect) = link {
                self.left -= 1;
                return Some(effect);
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.left, Some(self.left))
    }
}

impl ExactSizeIterator for EffectsIter<'_> {}

impl Effects {
    /// No effects, no badges.
    pub fn none() -> Self {
        Self::default()
    }

    /// The chain that has `entries` in it, in that order, each effect with the name it has (a project file says what a chain has in it
    /// this way). `None` if two of the entries are effects of the same name (whatever their kinds), or are the same badge. (A badge of the
    /// track that is last is not kept, as it is the last thing in the chain without that.)
    pub fn from_entries(entries: impl IntoIterator<Item = ChainEntry>) -> Option<Self> {
        let mut chain = Self::none();
        for entry in entries {
            let key = match &entry {
                ChainEntry::Effect(effect) => ChainItemKey::Effect(effect.instance()),
                ChainEntry::Badge(badge) => ChainItemKey::Badge(*badge),
            };
            if chain.links.iter().any(|link| link.key() == key) {
                return None;
            }
            chain.links.push(match entry {
                ChainEntry::Effect(effect) => Link::Effect(effect),
                ChainEntry::Badge(badge) => Link::Badge(badge),
            });
        }
        chain.settle();
        Some(chain)
    }

    /// Everything that is kept in the chain, in the order it is applied in, as entries (so that a chain can be made again from them with
    /// [`Self::from_entries`]): the badge of the track is among them only if it is not at the end.
    pub fn entries(&self) -> Vec<ChainEntry> {
        self.links
            .iter()
            .map(|link| match link {
                Link::Effect(effect) => ChainEntry::Effect(effect.clone()),
                Link::Badge(badge) => ChainEntry::Badge(*badge),
            })
            .collect()
    }

    /// Puts `effect`, with the values it has, on, last (but before the badge of the track, if that is at the end), named as the next
    /// effect of the chain is (see [`Self::next_instance`]), which is the name that it is given back as. (There may be effects of its kind
    /// on the chain already.)
    pub fn put_on(&mut self, effect: Effect) -> EffectInstanceId {
        let instance = self.next_instance();
        self.links.push(Link::Effect(effect.named(instance)));
        instance
    }

    /// The name that the next effect that is put on gets: [`EffectInstanceId::FIRST`] if the chain has no effects, and otherwise one more
    /// than the largest name there is (so that the name of an effect that is still there is never given again).
    pub fn next_instance(&self) -> EffectInstanceId {
        self.iter()
            .map(|effect| effect.instance())
            .max()
            .map_or(EffectInstanceId::FIRST, |largest| {
                EffectInstanceId::new(largest.number() + 1)
            })
    }

    /// The effect that is named `instance`, if the chain has it.
    pub fn instance(&self, instance: EffectInstanceId) -> Option<&Effect> {
        self.iter().find(|effect| effect.instance() == instance)
    }

    /// Whether there is no effect. (There may be badges.)
    pub fn is_empty(&self) -> bool {
        !self.links.iter().any(|link| matches!(link, Link::Effect(_)))
    }

    /// Whether there are [`MOST_EFFECTS_ON_ONE_THING`] effects, and no more can be put on.
    pub fn is_full(&self) -> bool {
        self.len() >= MOST_EFFECTS_ON_ONE_THING
    }

    /// How many effects there are. (The badges are not counted.)
    pub fn len(&self) -> usize {
        self.links.iter().filter(|link| matches!(link, Link::Effect(_))).count()
    }

    /// The effects, in the order they are applied in.
    pub fn iter(&self) -> EffectsIter<'_> {
        EffectsIter {
            links: self.links.iter(),
            left: self.len(),
        }
    }

    /// Whether the chain has an effect that is named `instance`.
    pub fn has_the_instance(&self, instance: EffectInstanceId) -> bool {
        self.instance(instance).is_some()
    }

    /// Which of the effects of its kind the effect named `instance` is, counting from 1 in the order they are applied in, and how many
    /// effects of that kind the chain has: `(2, 3)` for the second of three hue shifts. `None` if there is no such effect.
    pub fn place_among_its_kind(&self, instance: EffectInstanceId) -> Option<(usize, usize)> {
        let kind = self.instance(instance)?.kind();
        let of_its_kind = || self.iter().filter(|effect| effect.kind() == kind);
        let before_it = of_its_kind().take_while(|effect| effect.instance() != instance).count();
        Some((before_it + 1, of_its_kind().count()))
    }

    /// Puts the effect of `kind` on, last, with the values it begins with, and gives back the name it has in the chain.
    pub fn add(&mut self, kind: EffectKind) -> EffectInstanceId {
        self.put_on(Effect::new(kind))
    }

    /// Takes the effect that is named `instance` off. `false` if there was none.
    pub fn remove(&mut self, instance: EffectInstanceId) -> bool {
        let before = self.links.len();
        self.links
            .retain(|link| !matches!(link, Link::Effect(effect) if effect.instance() == instance));
        let removed = self.links.len() != before;
        self.settle();
        removed
    }

    /// Switches the effect that is named `instance` on or off. `false` if there is none.
    pub fn set_enabled(&mut self, instance: EffectInstanceId, enabled: bool) -> bool {
        for link in &mut self.links {
            if let Link::Effect(effect) = link
                && effect.instance() == instance
            {
                effect.enabled = enabled;
                return true;
            }
        }
        false
    }

    /// Gives the effect that is named `instance` the state its plugin saved (or takes it away: `None`). `false` if there is no such effect.
    pub fn set_state(&mut self, instance: EffectInstanceId, state: Option<Arc<[u8]>>) -> bool {
        for link in &mut self.links {
            if let Link::Effect(effect) = link
                && effect.instance() == instance
            {
                effect.state_fingerprint = fingerprint_of_a_state(state.as_ref());
                effect.state = state;
                return true;
            }
        }
        false
    }

    /// Whether `property` is the value of one of the effects of the chain.
    pub fn has_the_value(&self, property: PropertyId) -> bool {
        self.iter().any(|effect| effect.value_of(property).is_some())
    }

    /// The effect that `property` is a value of, if the chain has it.
    pub fn effect_of_the_value(&self, property: PropertyId) -> Option<&Effect> {
        self.iter().find(|effect| effect.value_of(property).is_some())
    }

    /// The value of `property` when it is not animated, if it is a property of one of the effects.
    pub fn value_of(&self, property: PropertyId) -> Option<PropertyValue> {
        self.iter().find_map(|effect| effect.value_of(property)).cloned()
    }

    /// Gives `property`, which must be one of an effect that is here, `value`. `false` if it is not, or is not that kind of value.
    pub fn set_value(&mut self, property: PropertyId, value: &PropertyValue) -> bool {
        for link in &mut self.links {
            if let Link::Effect(effect) = link
                && let Some(changed) = effect.with_value(property, value)
            {
                *effect = changed;
                return true;
            }
        }
        false
    }

    /// Whether there is nothing in the chain: no effect, and no badge that is kept.
    pub fn has_nothing_in_it(&self) -> bool {
        self.links.is_empty()
    }

    /// Everything in the chain in the order it is applied in. A chain of a clip (`has_the_track_badge`) has the badge of the track in
    /// it, at its end if it has not been put anywhere else; the chain of anything else has not.
    pub fn items(&self, has_the_track_badge: bool) -> Vec<ChainItem<'_>> {
        let mut items: Vec<ChainItem<'_>> = self
            .links
            .iter()
            .filter(|link| has_the_track_badge || !matches!(link, Link::Badge(Badge::Track)))
            .map(|link| match link {
                Link::Effect(effect) => ChainItem::Effect(effect),
                Link::Badge(badge) => ChainItem::Badge(*badge),
            })
            .collect();
        if has_the_track_badge && !self.keeps_the_badge_of_the_track() {
            items.push(ChainItem::Badge(Badge::Track));
        }
        items
    }

    /// The names of everything in the chain, in the order it is applied in (see [`Self::items`]).
    pub fn keys(&self, has_the_track_badge: bool) -> Vec<ChainItemKey> {
        self.items(has_the_track_badge).iter().map(ChainItem::key).collect()
    }

    /// The tags that the chain has a badge of, in the order the badges are in.
    pub fn tag_badges(&self) -> impl Iterator<Item = GroupId> + '_ {
        self.links.iter().filter_map(|link| match link {
            Link::Badge(Badge::Tag(group)) => Some(*group),
            Link::Effect(_) | Link::Badge(Badge::Track) => None,
        })
    }

    pub fn has_the_tag_badge(&self, group: GroupId) -> bool {
        self.tag_badges().any(|badge| badge == group)
    }

    /// Whether the badge of the track is kept in the chain: it is, when it has been put anywhere but the end.
    pub fn keeps_the_badge_of_the_track(&self) -> bool {
        self.links.contains(&Link::Badge(Badge::Track))
    }

    /// Puts the badge of the tag `group` in the chain, last (but before the badge of the track, if that is at the end). `false`, and
    /// nothing is changed, if it is there already.
    pub fn add_tag_badge(&mut self, group: GroupId) -> bool {
        if self.has_the_tag_badge(group) {
            return false;
        }
        self.links.push(Link::Badge(Badge::Tag(group)));
        true
    }

    /// Takes the badge of the tag `group` out of the chain. `false` if it was not there.
    pub fn remove_tag_badge(&mut self, group: GroupId) -> bool {
        let before = self.links.len();
        self.links
            .retain(|link| link.key() != ChainItemKey::Badge(Badge::Tag(group)));
        let removed = self.links.len() != before;
        self.settle();
        removed
    }

    /// Puts the badges of the tags `groups`, in that order, where the badge of the tag `group` is, and takes that one out. A badge of
    /// one of them that the chain has already stays where it is, and is not put again. `false`, and nothing is changed, if the badge of
    /// `group` was not there.
    pub fn replace_tag_badge(&mut self, group: GroupId, groups: &[GroupId]) -> bool {
        let Some(place) = self
            .links
            .iter()
            .position(|link| link.key() == ChainItemKey::Badge(Badge::Tag(group)))
        else {
            return false;
        };
        self.links.remove(place);
        let new: Vec<Link> = groups
            .iter()
            .filter(|other| **other != group && !self.has_the_tag_badge(**other))
            .map(|other| Link::Badge(Badge::Tag(*other)))
            .collect();
        self.links.splice(place..place, new);
        self.settle();
        true
    }

    /// Moves `item` so that it is right before `before`, or last if that is `None`; the others keep their order. `false`, and nothing is
    /// changed, if `item` or `before` is not in the chain (see [`Self::items`] for what is, and `has_the_track_badge`). Putting
    /// something before itself changes nothing.
    pub fn move_before(&mut self, item: ChainItemKey, before: Option<ChainItemKey>, has_the_track_badge: bool) -> bool {
        let mut order = self.keys(has_the_track_badge);
        let Some(from) = order.iter().position(|key| *key == item) else {
            return false;
        };
        if let Some(target) = before
            && !order.contains(&target)
        {
            return false;
        }
        if before == Some(item) {
            return true;
        }
        order.remove(from);
        let to = before.map_or(order.len(), |target| {
            order
                .iter()
                .position(|key| *key == target)
                .expect("the one to go before is in the chain: it was just looked for")
        });
        order.insert(to, item);
        let mut kept: Vec<(ChainItemKey, Link)> = self.links.drain(..).map(|link| (link.key(), link)).collect();
        self.links = order
            .into_iter()
            .filter_map(|key| match kept.iter().position(|(kept_key, _)| *kept_key == key) {
                Some(place) => Some(kept.remove(place).1),
                // (The badge of the track that was at the end without being kept, and is now somewhere else.)
                None if key == ChainItemKey::Badge(Badge::Track) => Some(Link::Badge(Badge::Track)),
                None => None,
            })
            .collect();
        self.settle();
        true
    }

    /// Puts the chain in the one form that it has: the badge of the track is not kept when it is last, because it is at the end of the
    /// chain without being kept there.
    fn settle(&mut self) {
        while self.links.last() == Some(&Link::Badge(Badge::Track)) {
            self.links.pop();
        }
    }
}

#[cfg(test)]
mod chain_tests;
#[cfg(test)]
mod installed_tests;
#[cfg(test)]
mod kind_tests;
#[cfg(test)]
mod shader_image_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod text_tests;
