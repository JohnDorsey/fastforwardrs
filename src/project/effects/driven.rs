// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A value of an effect that is read from a **mask** instead of from its slider (`docs/shader-masks-design.md`): what the mask is made of, which channel of its colour gives the
//! value, and the number the value is where the mask has nothing. The slider of the value is kept (with its keyframes) and is not used while the value is driven.

use std::collections::HashSet;

use crate::project::groups::GroupId;
use crate::project::tracks::TrackId;

mod graph;

pub use crate::isf::MaskChannel;
pub use graph::{Graph, LEAST_DISTANCE_BETWEEN_PLACES, MOST_POINTS_OF_A_GRAPH, NUMBERS_OF_A_GRAPH};

/// What a mask is made of: where the picture that a value is read from comes from.
///
/// * **A track**: what its clips make, with the effects of the clips and of the track, whether or not the track's eye is open.
/// * **A clip group** or **a track group**: what the clips in it make (for a track group, the clips on its tracks), with the effects of the clips, of their tracks and of **the group itself and
///   the groups in it**, but not of the groups that it is in. (The picture that the group makes of its own, as the picture of the project shows it, without what is round it.)
///
/// A layer that is not on a track is not a kind of source: the eye of a track does what it was thought of for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MaskSource {
    Track(TrackId),
    ClipGroup(GroupId),
    TrackGroup(GroupId),
}

impl MaskSource {
    /// The group that the source is, if it is one.
    pub fn group(self) -> Option<GroupId> {
        match self {
            Self::Track(_) => None,
            Self::ClipGroup(group) | Self::TrackGroup(group) => Some(group),
        }
    }
}

/// How the value of the mask's channel becomes the place of the value in the range of the value that is driven.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum MaskMap {
    /// The place is the value of the channel: nothing of it is the least of the range and all of it is the most.
    #[default]
    Straight,
    /// The place is the height of a graph of line segments at the value of the channel.
    Graph(Graph),
}

impl MaskMap {
    /// How the shader that reads the value is made: with a graph (whose numbers the call carries) or without.
    pub fn mapping(self) -> crate::isf::Mapping {
        match self {
            Self::Straight => crate::isf::Mapping::Straight,
            Self::Graph(_) => crate::isf::Mapping::Graph,
        }
    }

    /// The numbers of a call that the map is, if it is a graph.
    pub fn numbers_of_the_call(self) -> Option<[f32; NUMBERS_OF_A_GRAPH]> {
        match self {
            Self::Straight => None,
            Self::Graph(graph) => Some(graph.numbers()),
        }
    }

    /// The graph the map is, or the straight line, which is what it is until a graph is drawn.
    pub fn graph_or_straight(self) -> Graph {
        match self {
            Self::Straight => Graph::straight(),
            Self::Graph(graph) => graph,
        }
    }
}

/// A value of an effect that is read from a mask.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrivenValue {
    pub source: MaskSource,
    /// Which part of the colour of the mask is the value (spread over the range of the value: nothing of the colour is 0 of the range and all of it is the whole).
    pub channel: MaskChannel,
    /// What the value is **where the mask has nothing** (a gap in the track, or the part of the picture its clips do not cover), and, mixed in by how much the mask covers, where it
    /// covers some. A number of the value's own range, set by a slider made for it in the settings of the mask.
    pub value_where_nothing: f32,
    /// How the channel's value becomes the place in the range: straight, or by a graph the person drew.
    pub map: MaskMap,
    /// Whether a **transition** among the effects of the clips of the source **sees the real backdrop**: the tracks behind the source in the frame (`docs/shader-masks-design.md`, 10). While a
    /// clip of the source has a transition, the mask is then the source over the tracks behind it, as the frame shows it; when it is off the mask is the source on its own and a transition
    /// is done over nothing. On when a value is first read from a mask.
    pub real_backdrop: bool,
}

/// What a mask is: what it is made of, and whether the transitions in it see the real backdrop. **Two values that read the same source with another setting read two masks.**
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MaskUse {
    pub source: MaskSource,
    pub real_backdrop: bool,
}

impl MaskUse {
    /// The mask of the track `track`, with the transitions seeing the real backdrop (what a value read from a mask has until it is said otherwise).
    pub fn of_a_track(track: TrackId) -> Self {
        Self {
            source: MaskSource::Track(track),
            real_backdrop: true,
        }
    }
}

impl DrivenValue {
    /// A value read from the track `track`, as the channel `channel` says, and `value_where_nothing` where the track has nothing.
    pub fn from_a_track(track: TrackId, channel: MaskChannel, value_where_nothing: f32) -> Self {
        Self {
            source: MaskSource::Track(track),
            channel,
            value_where_nothing,
            map: MaskMap::Straight,
            real_backdrop: true,
        }
    }

    /// The same value, with the transitions of the source seeing the real backdrop or not.
    #[must_use]
    pub fn with_the_real_backdrop(self, real_backdrop: bool) -> Self {
        Self { real_backdrop, ..self }
    }

    /// The mask that the value reads.
    pub fn mask_use(&self) -> MaskUse {
        MaskUse {
            source: self.source,
            real_backdrop: self.real_backdrop,
        }
    }

    /// The same value, mapped as `map` says.
    #[must_use]
    pub fn with_the_map(self, map: MaskMap) -> Self {
        Self { map, ..self }
    }

    /// Whether the number is one that can be kept (it is a number).
    pub fn is_well_formed(&self) -> bool {
        self.value_where_nothing.is_finite()
    }
}

/// The values of an effect that are read from masks, by the place of each among the effect's values, in the order of the places. At most one for a value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DrivenValues {
    values: Vec<(u16, DrivenValue)>,
}

impl DrivenValues {
    pub fn none() -> Self {
        Self::default()
    }

    /// The values `values` give, if no place is there twice and every number is one.
    pub fn from_places(mut values: Vec<(u16, DrivenValue)>) -> Option<Self> {
        values.sort_by_key(|(place, _)| *place);
        let twice = values.windows(2).any(|pair| pair[0].0 == pair[1].0);
        (!twice && values.iter().all(|(_, driven)| driven.is_well_formed())).then_some(Self { values })
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// The driven values and the place of each, in the order of the places.
    pub fn iter(&self) -> impl Iterator<Item = (usize, &DrivenValue)> {
        self.values.iter().map(|(place, driven)| (usize::from(*place), driven))
    }

    /// What the value in place `place` is read from, if it is read from a mask.
    pub fn of(&self, place: usize) -> Option<&DrivenValue> {
        self.values
            .iter()
            .find(|(found, _)| usize::from(*found) == place)
            .map(|(_, driven)| driven)
    }

    /// The same, with the value in place `place` read as `driven` says (or from its slider again, for `None`). `None` if `driven` is a number that is not one.
    #[must_use]
    pub fn with(&self, place: u16, driven: Option<DrivenValue>) -> Option<Self> {
        if driven.as_ref().is_some_and(|driven| !driven.is_well_formed()) {
            return None;
        }
        let mut values: Vec<(u16, DrivenValue)> = self
            .values
            .iter()
            .copied()
            .filter(|(found, _)| *found != place)
            .collect();
        values.extend(driven.map(|driven| (place, driven)));
        values.sort_by_key(|(found, _)| *found);
        Some(Self { values })
    }

    /// The masks that the values read, each once, in the order they are first met in: the number of a mask among them is the number the shader reads it by.
    pub fn uses(&self) -> Vec<MaskUse> {
        let mut uses: Vec<MaskUse> = Vec::new();
        for (_, driven) in &self.values {
            if !uses.contains(&driven.mask_use()) {
                uses.push(driven.mask_use());
            }
        }
        uses
    }

    /// The sources that any of the values is read from, each once, in the order they are first met in.
    pub fn sources(&self) -> Vec<MaskSource> {
        let mut sources: Vec<MaskSource> = Vec::new();
        for (_, driven) in &self.values {
            if !sources.contains(&driven.source) {
                sources.push(driven.source);
            }
        }
        sources
    }

    /// The same, without any value that is read from the track `track` (what happens to a value whose track is gone, if the project is to have no such value).
    #[must_use]
    pub fn without_the_track(&self, track: TrackId) -> Self {
        Self {
            values: self
                .values
                .iter()
                .copied()
                .filter(|(_, driven)| driven.source != MaskSource::Track(track))
                .collect(),
        }
    }

    /// The same, without any value that is read from a group that is not among `groups` (what happens to a value whose group is gone).
    #[must_use]
    pub fn without_the_groups_not_among(&self, groups: &HashSet<GroupId>) -> Self {
        Self {
            values: self
                .values
                .iter()
                .copied()
                .filter(|(_, driven)| driven.source.group().is_none_or(|group| groups.contains(&group)))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests;
