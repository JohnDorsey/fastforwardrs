//! Effects: what is done to the picture of a clip, or of a track, on its way to the finished picture.
//!
//! A clip or a track has the effects that were put on it, each at most once, one after another in the order they were put on in (or
//! the order they were put in since: they can be moved, see [`Effects::move_before`]). What an effect does is set by its *values*, which
//! are properties like any other (see [`PropertyId`]): each can be animated, with keyframes, and is changed in the properties panel
//! with the same stopwatch. An effect that is put on a clip or a track has the values it begins with (see
//! [`EffectKind::default_values`]), and is taken off with its keyframes.
//!
//! A group has effects too, and what has effects has a *chain*: its effects, one after another, and among them the **badges** that say
//! where the effects of something else are applied to it (see [`Badge`]). A clip has a badge for each tag it is in, and one that stands
//! for the effects of its track; a group that is in a tag has a badge for it. The chain says in what order everything is applied.
//!
//! What there is to know about each *kind* of effect is a row of one table ([`EffectKind::ALL`] is its rows, in the order
//! they are offered in): its name, what it does, its values, what it can be put on, and how it changes a picture. A new effect is
//! a new row, and the code that is the same for all of them (putting them on, taking them off, saving them, the panel that
//! has them, the preview, the export) is not changed. (So far there is one row, and the ways that an effect can change a picture
//! are the ways of changing its colours; sound, and what is seen of other pictures, come with the effects that need them.)

use std::fmt;

use super::animation::{PropertyId, PropertyValue};
use super::groups::{GroupId, GroupKind};
use super::tracks::ClipKind;
use crate::colour_effects::ColourMatrix;

/// The kinds of effect there are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EffectKind {
    /// The hues of the colours of the picture are turned round the wheel of colours, as far as the effect says.
    HueShift,
}

/// What a kind of effect is: one row of the table.
struct EffectRow {
    kind: EffectKind,
    /// The name it is saved under in a project file, and called by to the people who use it (with a capital letter).
    name: &'static str,
    /// What it does, in a few words, for the menu that it is chosen from.
    description: &'static str,
    /// The values it has, each a property, in the order they are kept and shown in.
    properties: &'static [PropertyId],
    /// What its values are when it is put on, in the same order.
    default_values: fn() -> Vec<PropertyValue>,
    /// Whether it can be put on a video, a picture and a text object (what is seen), and on a track (everything seen on it).
    on_what_is_seen: bool,
    on_a_track: bool,
    /// Whether it can be put on a tag, which has it applied to each member that it can be applied to.
    on_a_tag: bool,
    /// Whether it can be put on a clip group or a track group, which has it applied to the picture that its members make together.
    /// Until the preview and the export can make that picture, an effect is applied to each member instead, which makes the same
    /// picture only for an effect that changes each pixel by itself, whatever the pixels round it are, and does not change how clear
    /// it is (as the effects that are colour matrices do, except where a colour is cut off at the least or the most there is, when it is
    /// a little different: see `groups::effects_of_groups`). An effect that does not make the same picture that way (a blur, say) is
    /// not given a `true` here until they can.
    on_a_composite: bool,
    /// How it changes the colours of a picture, if it does, from its values.
    colour_matrix: Option<fn(&[PropertyValue]) -> ColourMatrix>,
    /// Which values the export uses in place of the values the effect has at a frame: what it has, as near as is worth telling
    /// apart (a change that nobody can see is not a stretch of its own in the export, which is slow to make: see
    /// [`EffectKind::as_exported`]).
    as_exported: fn(&[PropertyValue]) -> Vec<PropertyValue>,
}

const ROWS: [EffectRow; 1] = [EffectRow {
    kind: EffectKind::HueShift,
    name: "hue shift",
    description: "Turns the colours round the wheel of colours",
    properties: &[PropertyId::HueShift],
    default_values: hue_shift_defaults,
    on_what_is_seen: true,
    on_a_track: true,
    on_a_tag: true,
    on_a_composite: true,
    colour_matrix: Some(hue_shift_matrix),
    as_exported: hue_shift_as_exported,
}];

/// The steps, in degrees, that the hue shift is exported in: a turn of the colours that is a degree off is not seen.
const HUE_STEP_IN_DEGREES: f32 = 1.0;

/// The hue shift is exported in whole steps of [`HUE_STEP_IN_DEGREES`]. (An animated hue shift that is slow then makes a stretch
/// of the export of its own for every few frames, and not for every frame.)
fn hue_shift_as_exported(values: &[PropertyValue]) -> Vec<PropertyValue> {
    match values {
        [PropertyValue::Number(degrees)] => {
            vec![PropertyValue::Number(
                (degrees / HUE_STEP_IN_DEGREES).round() * HUE_STEP_IN_DEGREES,
            )]
        }
        other => other.to_vec(),
    }
}

/// The one value of the hue shift, how far it turns the colours, is nothing to begin with.
fn hue_shift_defaults() -> Vec<PropertyValue> {
    vec![PropertyValue::Number(0.0)]
}

/// How the hue shift changes colours: it turns them by the degrees that its one value says.
fn hue_shift_matrix(values: &[PropertyValue]) -> ColourMatrix {
    match values {
        [PropertyValue::Number(degrees)] => ColourMatrix::hue_rotation(*degrees),
        _ => ColourMatrix::NOTHING_CHANGES,
    }
}

impl fmt::Display for EffectKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

impl EffectKind {
    /// Every kind of effect, in the order they are offered in.
    pub const ALL: [EffectKind; 1] = [Self::HueShift];

    fn row(self) -> &'static EffectRow {
        ROWS.iter()
            .find(|row| row.kind == self)
            .expect("every kind of effect has a row in the table")
    }

    /// The name of the kind of effect in a project file, which is what it is called, in lower case.
    pub fn name(self) -> &'static str {
        self.row().name
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// What the effect does, in a few words.
    pub fn description(self) -> &'static str {
        self.row().description
    }

    /// The properties that are the values of the effect, in order.
    pub fn properties(self) -> &'static [PropertyId] {
        self.row().properties
    }

    /// What the values of the effect are when it is put on something, one for each of its properties.
    pub fn default_values(self) -> Vec<PropertyValue> {
        (self.row().default_values)()
    }

    /// Whether a clip of this kind can have the effect.
    pub fn applies_to_a_clip_of_kind(self, kind: ClipKind) -> bool {
        self.row().on_what_is_seen && matches!(kind, ClipKind::Video | ClipKind::Image | ClipKind::Text)
    }

    /// Whether a track can have the effect.
    pub fn applies_to_a_track(self) -> bool {
        self.row().on_a_track
    }

    /// Whether a group of kind `kind` can have the effect: a tag, which has it applied to each member, if the effect can be put on a
    /// tag; and a clip group or a track group, which have it applied to what their members make together, if the effect can be put on
    /// what several things make together.
    pub fn applies_to_a_group_of_kind(self, kind: GroupKind) -> bool {
        match kind {
            GroupKind::Tag => self.row().on_a_tag,
            GroupKind::ClipGroup | GroupKind::TrackGroup => self.row().on_a_composite,
        }
    }

    /// The values that are exported when the values of the effect at a frame are `values` (the same values of the same kinds): the
    /// values as near as is worth telling apart, which is `values` themselves for an effect whose values are all that there is to
    /// see of them. What is the same when it is exported is exported as one thing, which makes the export quicker to get ready.
    pub fn as_exported(self, values: &[PropertyValue]) -> Vec<PropertyValue> {
        (self.row().as_exported)(values)
    }

    /// How the effect changes the colours of a picture when its values are `values` (one for each of its properties, in the
    /// order of [`Self::properties`]), if it changes colours at all. A matrix that changes nothing if the values are not those
    /// of the effect.
    pub fn colour_matrix(self, values: &[PropertyValue]) -> Option<ColourMatrix> {
        self.row().colour_matrix.map(|matrix| matrix(values))
    }
}

/// An effect that a clip or a track has: which, and what its values are when they are not animated.
#[derive(Clone, Debug, PartialEq)]
pub struct Effect {
    kind: EffectKind,
    /// The value of each of the properties of the kind of effect, in the order of [`EffectKind::properties`].
    values: Vec<PropertyValue>,
}

impl Effect {
    /// The effect of this kind as it is when it is put on.
    pub fn new(kind: EffectKind) -> Self {
        Self {
            kind,
            values: kind.default_values(),
        }
    }

    /// The effect of this kind with these values, if there is one for each of its properties and each is the kind of value the
    /// property has.
    pub fn with_values(kind: EffectKind, values: Vec<PropertyValue>) -> Option<Self> {
        let right_values = values.len() == kind.properties().len()
            && values
                .iter()
                .zip(kind.properties())
                .all(|(value, property)| value.kind() == property.kind());
        right_values.then_some(Self { kind, values })
    }

    pub fn kind(&self) -> EffectKind {
        self.kind
    }

    /// The values the effect has when they are not animated, one for each of its properties, in order.
    pub fn values(&self) -> &[PropertyValue] {
        &self.values
    }

    /// The value of `property` when it is not animated, if it is one of the effect's.
    pub fn value_of(&self, property: PropertyId) -> Option<&PropertyValue> {
        let place = self.kind.properties().iter().position(|own| *own == property)?;
        self.values.get(place)
    }

    /// The effect with `value` for `property`. `None` if it is not a property of the effect, or not the kind of value that it has.
    #[must_use]
    pub fn with_value(&self, property: PropertyId, value: &PropertyValue) -> Option<Self> {
        let place = self.kind.properties().iter().position(|own| *own == property)?;
        (value.kind() == property.kind()).then(|| {
            let mut values = self.values.clone();
            values[place] = value.clone();
            Self {
                kind: self.kind,
                values,
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

/// What is in a chain, named so that it can be pointed at without saying where it is: an effect is its kind (there is at most one of
/// each kind in a chain), and a badge is what it is a badge of (there is at most one of each).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChainItemKey {
    Effect(EffectKind),
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
            Self::Effect(effect) => ChainItemKey::Effect(effect.kind()),
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
            Self::Effect(effect) => ChainItemKey::Effect(effect.kind()),
            Self::Badge(badge) => ChainItemKey::Badge(*badge),
        }
    }
}

/// The effects of a clip, of a track or of a group, in the order they are applied in, and with them the badges that say where the
/// effects of something else are applied. There is at most one effect of each kind, and at most one badge of each.
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

    /// The chain that has `entries` in it, in that order (a project file says what a chain has in it this way). `None` if two of the
    /// entries are an effect of the same kind, or are the same badge. (A badge of the track that is last is not kept, as it is the
    /// last thing in the chain without that.)
    pub fn from_entries(entries: impl IntoIterator<Item = ChainEntry>) -> Option<Self> {
        let mut chain = Self::none();
        for entry in entries {
            let key = match &entry {
                ChainEntry::Effect(effect) => ChainItemKey::Effect(effect.kind()),
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

    /// Puts `effect`, with the values it has, on, last (but before the badge of the track, if that is at the end). `false`, and
    /// nothing is changed, if there is one of that kind already.
    pub fn put_on(&mut self, effect: Effect) -> bool {
        if self.contains(effect.kind()) {
            return false;
        }
        self.links.push(Link::Effect(effect));
        true
    }

    /// Whether there is no effect. (There may be badges.)
    pub fn is_empty(&self) -> bool {
        !self.links.iter().any(|link| matches!(link, Link::Effect(_)))
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

    pub fn contains(&self, kind: EffectKind) -> bool {
        self.get(kind).is_some()
    }

    pub fn get(&self, kind: EffectKind) -> Option<&Effect> {
        self.iter().find(|effect| effect.kind == kind)
    }

    /// Puts the effect of `kind` on, last, with the values it begins with. `false`, and nothing is changed, if there is one of
    /// that kind already.
    pub fn add(&mut self, kind: EffectKind) -> bool {
        self.put_on(Effect::new(kind))
    }

    /// Takes the effect of `kind` off. `false` if there was none.
    pub fn remove(&mut self, kind: EffectKind) -> bool {
        let before = self.links.len();
        self.links.retain(|link| link.key() != ChainItemKey::Effect(kind));
        let removed = self.links.len() != before;
        self.settle();
        removed
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
mod tests;
