// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A drawer: the racks of keyframes that a clip, or a track, keeps with it.
//!
//! Every clip has a drawer and so does every track, and what is in it goes where it goes: the keyframes of a clip, which
//! are in its drawer, move along with the clip when it is moved, are cut with it, and are stretched with it. A drawer
//! can hold the racks of keyframes of other things than the clip or track it belongs to, which is what makes them a place
//! to bundle keyframes with the clips that they need to be moved with. (A keyframe crate is a clip that has nothing but a
//! drawer, which is always open.)
//!
//! A drawer can have several racks of one property of one thing: they are told apart by their names (see [`RackId`]), and
//! are not merged unless that is asked for. Its racks are in order, the one at the top first, which is the order they are
//! shown in, and which of two keyframes at one frame counts when the racks are of the same property of the same thing (the one
//! in the rack that comes later).

use super::ids::RackId;
use super::rack::Rack;
use crate::project::{AnimationTarget, PropertyId};

/// The racks of a clip or of a track.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drawer {
    racks: Vec<Rack>,
}

impl Drawer {
    /// A drawer with these racks (which must have names that are not each other's), the first at the top.
    pub fn with_racks(racks: impl IntoIterator<Item = Rack>) -> Self {
        let mut drawer = Self::default();
        for rack in racks {
            drawer.push(rack);
        }
        drawer
    }

    /// The racks, the one at the top first.
    pub fn racks(&self) -> &[Rack] {
        &self.racks
    }

    /// Whether there is nothing in the drawer.
    pub fn is_empty(&self) -> bool {
        self.racks.is_empty()
    }

    pub fn len(&self) -> usize {
        self.racks.len()
    }

    pub fn rack(&self, id: RackId) -> Option<&Rack> {
        self.racks.iter().find(|rack| rack.id() == id)
    }

    /// The place of the rack called `id` in the drawer, counted from the top, which is 0.
    pub fn position_of(&self, id: RackId) -> Option<usize> {
        self.racks.iter().position(|rack| rack.id() == id)
    }

    pub fn rack_mut(&mut self, id: RackId) -> Option<&mut Rack> {
        self.racks.iter_mut().find(|rack| rack.id() == id)
    }

    /// The first (the one nearest the top) of the racks of the keyframes of `property` of `target`, if the drawer has any.
    pub fn first_rack_for(&self, target: AnimationTarget, property: PropertyId) -> Option<&Rack> {
        self.racks_for(target, property).next()
    }

    pub fn first_rack_for_mut(&mut self, target: AnimationTarget, property: PropertyId) -> Option<&mut Rack> {
        self.racks
            .iter_mut()
            .find(|rack| rack.target() == target && rack.property() == property)
    }

    /// Every rack of the keyframes of `property` of `target` that the drawer has, the one at the top first.
    pub fn racks_for(&self, target: AnimationTarget, property: PropertyId) -> impl Iterator<Item = &Rack> {
        self.racks
            .iter()
            .filter(move |rack| rack.target() == target && rack.property() == property)
    }

    /// Puts `rack` in the drawer, under the others. (A drawer does not have two racks of the same name.)
    pub fn push(&mut self, rack: Rack) {
        self.insert(self.racks.len(), rack);
    }

    /// Puts `rack` in the drawer so that it has `slot` racks above it: at the top if `slot` is 0, and under all the others if
    /// it is as many as there are (or more).
    pub fn insert(&mut self, slot: usize, rack: Rack) {
        debug_assert!(self.rack(rack.id()).is_none(), "a drawer has a rack once");
        self.racks.insert(slot.min(self.racks.len()), rack);
    }

    /// Takes the rack called `id` out of the drawer, and gives it.
    pub fn take(&mut self, id: RackId) -> Option<Rack> {
        let position = self.racks.iter().position(|rack| rack.id() == id)?;
        Some(self.racks.remove(position))
    }

    /// Takes away the racks that have no keyframes.
    pub fn remove_the_racks_with_no_keyframes(&mut self) {
        self.racks.retain(|rack| !rack.is_empty());
    }

    /// Takes away the racks that animate `target`, and gives how many there were.
    pub fn remove_the_racks_of(&mut self, target: AnimationTarget) -> usize {
        let before = self.racks.len();
        self.racks.retain(|rack| rack.target() != target);
        before - self.racks.len()
    }

    /// The same drawer with each of its racks changed as `change` says (a rack that is changed into nothing, which is a
    /// rack with no keyframes, is not kept).
    #[must_use]
    pub fn with_each_rack_changed(&self, change: impl Fn(&Rack) -> Rack) -> Drawer {
        let mut changed = Drawer {
            racks: self.racks.iter().map(change).collect(),
        };
        changed.remove_the_racks_with_no_keyframes();
        changed
    }
}

#[cfg(test)]
mod tests;
