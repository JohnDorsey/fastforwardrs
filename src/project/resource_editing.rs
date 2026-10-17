// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The resources of a project (see [`Resources`]) as the project holds them: adding one, and which of them the effects use.

use std::collections::BTreeSet;
use std::sync::Arc;

use super::effects::{Effects, ParameterControl};
use super::source_media::SourceMediaLink;
use super::{Project, PropertyValue, Resource, ResourceKind, ResourceProblem, Resources};

impl Project {
    /// The files that the effects of the project use.
    pub fn resources(&self) -> &Resources {
        &self.resources
    }

    /// The same project with these resources. (What a project file brought.)
    #[must_use]
    pub fn with_resources(mut self, resources: Resources) -> Self {
        self.resources = resources;
        self
    }

    /// Adds a resource called `wanted_name`, with `bytes`, kept at `link` (see [`Resources::add`]), and gives the name it has: another name than the wanted one if that is
    /// taken by other bytes, and the name of the resource that is there if it has the same bytes. It is an error, and nothing is added, if the name cannot be one, the file
    /// is empty or too big, or there are too many resources.
    pub fn add_resource(
        &mut self,
        wanted_name: &str,
        kind: ResourceKind,
        bytes: Arc<[u8]>,
        link: SourceMediaLink,
    ) -> Result<String, ResourceProblem> {
        self.resources.add(wanted_name, kind, bytes, link)
    }

    /// Gives the missing resource `name` the bytes that were found for it. Whether it had none and was given them.
    pub fn give_the_bytes_of_a_resource(&mut self, name: &str, bytes: Arc<[u8]>) -> bool {
        self.resources.found_the_bytes_of(name, bytes)
    }

    /// Makes the resource `name` kept at `link`. Whether there is such a resource.
    pub fn keep_a_resource_at(&mut self, name: &str, link: SourceMediaLink) -> bool {
        self.resources.keep_at(name, link)
    }

    /// The names of the resources that the effects of the project name, in the values that are resources: the effects of the clips, of the tracks and of the groups,
    /// whether or not the project has a resource of that name. (Not what keyframes say, which is not for a value that is a resource: it has no keyframes.)
    pub fn resources_in_use(&self) -> BTreeSet<String> {
        let mut used = BTreeSet::new();
        for track in self.tracks() {
            names_of_resources_in(track.effects(), &mut used);
            for entry in track.entries() {
                names_of_resources_in(entry.effects(), &mut used);
            }
        }
        for group in self.groups() {
            names_of_resources_in(group.effects(), &mut used);
        }
        used
    }

    /// The resources that a project file is to have: those that the effects use and the project has, in the order they were added in.
    pub fn resources_to_save(&self) -> Resources {
        self.resources.only_those_named_in(&self.resources_in_use())
    }

    /// The names of the resources that the effects use and that cannot be used: the project has none of the name, or it is missing (a linked file that was not there to be read).
    /// In the order of the names. (The effects that use them are kept, and do nothing, like an effect that the program does not have.)
    pub fn resources_in_use_that_are_missing(&self) -> Vec<String> {
        self.resources_in_use()
            .into_iter()
            .filter(|name| self.resources.get(name).is_none_or(Resource::is_missing))
            .collect()
    }

    /// The resource that the value `value` of an effect with the control `control` names, if it names one.
    pub fn resource_named_by(&self, control: &ParameterControl, value: &PropertyValue) -> Option<&Resource> {
        match (control, value) {
            (ParameterControl::Resource(_), PropertyValue::Words(name)) => self.resources.get(name),
            _ => None,
        }
    }
}

/// Adds to `used` the name that each value of an effect of `effects` that is a resource names, if it names any.
fn names_of_resources_in(effects: &Effects, used: &mut BTreeSet<String>) {
    for effect in effects.iter() {
        for (parameter, value) in effect.kind().parameters().iter().zip(effect.values()) {
            if let (ParameterControl::Resource(_), PropertyValue::Words(name)) = (&parameter.control, value)
                && !name.is_empty()
            {
                used.insert(name.clone());
            }
        }
    }
}
