// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The layers of a section as a tree of the groups that make pictures of their own (phase 5; the design is `docs/engine-phase-5-design.md`, H3 to H5).
//!
//! The rules of groups say that the layers of a section, back to front, are **contiguous for every group**, and that groups nest (a clip group inside the track groups of its tracks).
//! So each layer comes with the groups it is in, the outermost first, and consecutive layers that are in one group are the children of one node. **A group that has no effect that does
//! anything is not a node** (it is pass-through: its layers are children of what it is in); that is decided before, when the groups' effects are made ([`effects_of_the_groups_that_make_pictures`]).
//!
//! What is behind the front-most thing that covers the picture cannot be seen, and is left out: inside each group, and in the frame. A group covers the picture if a thing in it does
//! **and its effects keep every pixel as opaque as it was** (a colour mix, an exposure and a table do; a blur, a crop and an opacity do not).

use std::collections::HashMap;

use super::{GroupSpan, Layer, LayerEffect, operations_of_effects};
use crate::project::{GroupId, PictureOperation, Project, ResolvedAnimations, TimelineFrameIndex};

/// A layer, with where its track is (0 is the front-most track) and the groups that make pictures of their own that it is in, the outermost first.
pub(super) struct Entry {
    pub(super) track_position: usize,
    pub(super) layer: Layer,
    pub(super) shapes: Vec<GroupId>,
}

/// A thing in a container (the frame, or a group): a layer, or a group with the things in it.
pub(super) enum Node {
    Layer { track_position: usize, layer: Layer },
    Group { id: GroupId, children: Vec<Node> },
}

/// The effects of the clip groups and track groups of `project` that make a picture of their own: those that have an effect that is installed, is a picture effect, and does
/// something. (The others are pass-through.) A group's effects have no keyframes.
pub(super) fn effects_of_the_groups_that_make_pictures(
    project: &Project,
    animations: &ResolvedAnimations,
) -> HashMap<GroupId, Vec<LayerEffect>> {
    let mut made = HashMap::new();
    for group in project.groups().iter().filter(|group| group.kind().is_a_shape()) {
        let effects: Vec<LayerEffect> = project
            .effects_applied_to_the_group(group.id())
            .iter()
            // (An effect that the program does not have, which a project file brought, does nothing to the picture.)
            .filter(|applied| applied.effect.kind().is_installed())
            .map(|applied| super::layer_effect_of(project, animations, applied))
            .collect();
        if !operations_of_effects(&effects, TimelineFrameIndex::new(0)).is_empty() {
            made.insert(group.id(), effects);
        }
    }
    made
}

/// The things that `entries` are, from the level `depth` of their groups: a run of entries that are in one group at that level is a group node, and the others are layers.
pub(super) fn nodes_of(entries: Vec<Entry>, depth: usize) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut remaining = entries.into_iter().peekable();
    while let Some(entry) = remaining.next() {
        let Some(&id) = entry.shapes.get(depth) else {
            nodes.push(Node::Layer {
                track_position: entry.track_position,
                layer: entry.layer,
            });
            continue;
        };
        let mut run = vec![entry];
        while remaining.peek().is_some_and(|next| next.shapes.get(depth) == Some(&id)) {
            run.extend(remaining.next());
        }
        nodes.push(Node::Group {
            id,
            children: nodes_of(run, depth + 1),
        });
    }
    nodes
}

/// Whether the effects of a group keep every pixel of its picture as opaque as it was.
fn keeps_the_opacity(id: GroupId, group_effects: &HashMap<GroupId, Vec<LayerEffect>>) -> bool {
    group_effects.get(&id).is_some_and(|effects| {
        operations_of_effects(effects, TimelineFrameIndex::new(0))
            .iter()
            .all(|operation| {
                matches!(
                    operation,
                    PictureOperation::Mix(_) | PictureOperation::Exposure { .. } | PictureOperation::Lut(_)
                )
            })
    })
}

impl Node {
    /// Whether nothing behind this can be seen: a layer that covers the picture, or a group that keeps what is in it as opaque as it was and has something in it that covers it.
    pub(super) fn covers_the_picture(&self, group_effects: &HashMap<GroupId, Vec<LayerEffect>>) -> bool {
        match self {
            Self::Layer { layer, .. } => layer.covers_the_picture(),
            Self::Group { id, children } => {
                keeps_the_opacity(*id, group_effects)
                    && children.iter().any(|child| child.covers_the_picture(group_effects))
            }
        }
    }

    /// The place of the track of the front-most layer in this that covers the picture (a node that does cover it).
    pub(super) fn front_most_covering_track_position(
        &self,
        group_effects: &HashMap<GroupId, Vec<LayerEffect>>,
    ) -> usize {
        match self {
            Self::Layer { track_position, .. } => *track_position,
            Self::Group { children, .. } => children
                .iter()
                .rev()
                .find(|child| child.covers_the_picture(group_effects))
                .map_or(usize::MAX, |child| {
                    child.front_most_covering_track_position(group_effects)
                }),
        }
    }
}

/// Takes away what is behind the front-most thing that covers the picture, in each group and in `nodes`.
pub(super) fn drain_what_is_behind_what_covers(
    nodes: &mut Vec<Node>,
    group_effects: &HashMap<GroupId, Vec<LayerEffect>>,
) {
    for node in nodes.iter_mut() {
        if let Node::Group { children, .. } = node {
            drain_what_is_behind_what_covers(children, group_effects);
        }
    }
    if let Some(position) = nodes.iter().rposition(|node| node.covers_the_picture(group_effects)) {
        nodes.drain(..position);
    }
}

/// The layers of `nodes`, back to front, put in `layers`, and the groups among them in `groups` (the one that holds another first), with their effects.
pub(super) fn flatten(
    nodes: Vec<Node>,
    group_effects: &HashMap<GroupId, Vec<LayerEffect>>,
    layers: &mut Vec<Layer>,
    groups: &mut Vec<GroupSpan>,
) {
    for node in nodes {
        match node {
            Node::Layer { layer, .. } => layers.push(layer),
            Node::Group { id, children } => {
                let first_layer = layers.len();
                let place = groups.len();
                groups.push(GroupSpan {
                    group: id,
                    first_layer,
                    end_layer: first_layer,
                    effects: group_effects.get(&id).cloned().unwrap_or_default(),
                });
                flatten(children, group_effects, layers, groups);
                groups[place].end_layer = layers.len();
            }
        }
    }
}
