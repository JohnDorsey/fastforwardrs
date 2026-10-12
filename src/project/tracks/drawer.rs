//! A drawer: the racks of keyframes that a clip, or a track, keeps with it.
//!
//! Every clip has a drawer and so does every track, and what is in it goes where it goes: the keyframes of a clip, which
//! are in its drawer, move along with the clip when it is moved, are cut with it, and are stretched with it. A drawer
//! can hold the racks of keyframes of other things than the clip or track it belongs to, which is what makes them a place
//! to bundle keyframes with the clips that they need to be moved with. (A keyframe crate is a clip that has nothing but a
//! drawer, which is always open.)
//!
//! A drawer has at most one rack for a property of a thing: its racks are told apart by what they animate. Its racks
//! are in order, the one at the top first, which is the order they are shown in.

use super::ids::RackId;
use super::rack::Rack;
use crate::project::{AnimationTarget, PropertyId};

/// The racks of a clip or of a track.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drawer {
    racks: Vec<Rack>,
}

impl Drawer {
    /// A drawer with these racks (which must not be two of the same property of the same thing).
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

    pub fn rack_mut(&mut self, id: RackId) -> Option<&mut Rack> {
        self.racks.iter_mut().find(|rack| rack.id() == id)
    }

    /// The rack of the keyframes of `property` of `target`, if the drawer has one.
    pub fn rack_for(&self, target: AnimationTarget, property: PropertyId) -> Option<&Rack> {
        self.racks
            .iter()
            .find(|rack| rack.target() == target && rack.property() == property)
    }

    pub fn rack_for_mut(&mut self, target: AnimationTarget, property: PropertyId) -> Option<&mut Rack> {
        self.racks
            .iter_mut()
            .find(|rack| rack.target() == target && rack.property() == property)
    }

    /// Whether the drawer could take `rack` in without having two racks of one property of one thing.
    pub fn has_room_for(&self, rack: &Rack) -> bool {
        self.rack_for(rack.target(), rack.property()).is_none()
    }

    /// Puts `rack` in the drawer, under the others. (A drawer that has a rack for the same property of the same thing, or
    /// one of the same name, is not given another: see [`Self::has_room_for`].)
    pub fn push(&mut self, rack: Rack) {
        debug_assert!(
            self.has_room_for(&rack),
            "a drawer has one rack for a property of a thing"
        );
        debug_assert!(self.rack(rack.id()).is_none(), "a drawer has a rack once");
        self.racks.push(rack);
    }

    /// Puts `rack` in the drawer, in place of the rack for the same property of the same thing if there is one, and under the
    /// others if there is not.
    pub fn put(&mut self, rack: Rack) {
        match self
            .racks
            .iter_mut()
            .find(|existing| existing.target() == rack.target() && existing.property() == rack.property())
        {
            Some(existing) => *existing = rack,
            None => self.push(rack),
        }
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
