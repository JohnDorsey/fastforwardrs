// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The layers of a section as a tree of the groups that make pictures of their own (phase 5; the design is `docs/engine-phase-5-design.md`, H3 to H5 and H7).
//!
//! The rules of groups say that the layers of a section, back to front, are **contiguous for every group**, and that groups nest (a clip group inside the track groups of its tracks).
//! So each layer comes with the groups it is in, the outermost first, and consecutive layers that are in one group are the children of one node. **A group that has no effect that does
//! anything is not a node** (it is pass-through: its layers are children of what it is in); that is decided before, when the groups' effects are made
//! ([`effects_of_the_groups_that_make_pictures`]).
//!
//! **An adjustment clip is a group made on the spot** (H7): everything behind it in its own container (the frame, or the group it is in) becomes the children of a node that has the
//! adjustment's effects, and the adjustment itself is no layer. At the frame (and not inside a group) what is behind is over opaque black, as everything is.
//!
//! What is behind the front-most thing that covers the picture cannot be seen, and is left out: inside each group, and in the frame. A group covers the picture if a thing in it does
//! **and its effects keep every pixel as opaque as it was** (a colour mix, an exposure and a table do; a blur, a crop and an opacity do not).

use std::collections::HashMap;

use super::{GroupSpan, Layer, LayerContent, LayerEffect, SpanOwner, effects_keep_the_opacity};
use crate::project::{GroupId, Project, ResolvedAnimations, TimelineFrameIndex};

/// A layer, with where its track is (0 is the front-most track) and the groups that make pictures of their own that it is in, the outermost first.
pub(super) struct Entry {
    pub(super) track_position: usize,
    pub(super) layer: Layer,
    pub(super) shapes: Vec<GroupId>,
}

/// A thing in a container (the frame, or a group): a layer, or a group with the things in it.
pub(super) enum Node {
    Layer {
        track_position: usize,
        layer: Layer,
    },
    Group {
        owner: SpanOwner,
        effects: Vec<LayerEffect>,
        /// The things in it are over opaque black (an adjustment clip at the frame).
        over_black: bool,
        children: Vec<Node>,
    },
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
        if !super::operations_of_effects(&effects, TimelineFrameIndex::new(0), 0).is_empty() {
            made.insert(group.id(), effects);
        }
    }
    made
}

/// The things that `entries` are, from the level `depth` of their groups: a run of entries that are in one group at that level is a group node, and the others are layers.
pub(super) fn nodes_of(
    entries: Vec<Entry>,
    depth: usize,
    group_effects: &HashMap<GroupId, Vec<LayerEffect>>,
) -> Vec<Node> {
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
            owner: SpanOwner::Group(id),
            effects: group_effects.get(&id).cloned().unwrap_or_default(),
            over_black: false,
            children: nodes_of(run, depth + 1, group_effects),
        });
    }
    nodes
}

/// Makes each adjustment clip in `nodes` (and in the groups in them) the group that it is: everything in front of it in its container, behind it, becomes the children of a group that
/// has its effects, and the clip is not a layer any more. An adjustment with nothing behind it, or with no effect, does nothing and is left out. `is_the_frame` says that `nodes` are
/// what the frame is made of and not what is in a group, so that what is behind an adjustment is over black.
pub(super) fn make_the_adjustments_groups(nodes: Vec<Node>, is_the_frame: bool) -> Vec<Node> {
    let mut made: Vec<Node> = Vec::new();
    for node in nodes {
        match node {
            Node::Group {
                owner,
                effects,
                over_black,
                children,
            } => made.push(Node::Group {
                owner,
                effects,
                over_black,
                children: make_the_adjustments_groups(children, false),
            }),
            Node::Layer { layer, .. } if matches!(layer.content, LayerContent::Adjustment) => {
                let behind = std::mem::take(&mut made);
                if behind.is_empty() || layer.effects.is_empty() {
                    made = behind;
                    continue;
                }
                made.push(Node::Group {
                    owner: SpanOwner::Adjustment(layer.clip),
                    effects: layer.effects,
                    over_black: is_the_frame,
                    children: behind,
                });
            }
            other => made.push(other),
        }
    }
    made
}

impl Node {
    /// Whether nothing behind this can be seen: a layer that covers the picture, or a group that keeps what is in it as opaque as it was and has something in it that covers it.
    pub(super) fn covers_the_picture(&self) -> bool {
        match self {
            Self::Layer { layer, .. } => layer.covers_the_picture(),
            Self::Group { effects, children, .. } => {
                effects_keep_the_opacity(effects) && children.iter().any(Node::covers_the_picture)
            }
        }
    }

    /// Whether this is a layer, or a group, or has in it, an effect that depends on earlier frames (phase 6.4.2): it has to be made in every frame to go on, hidden or not.
    fn depends_on_earlier_frames(&self) -> bool {
        match self {
            Self::Layer { layer, .. } => layer
                .effects
                .iter()
                .any(|effect| effect.kind.depends_on_earlier_frames()),
            Self::Group { effects, children, .. } => {
                effects.iter().any(|effect| effect.kind.depends_on_earlier_frames())
                    || children.iter().any(Node::depends_on_earlier_frames)
            }
        }
    }

    /// The place of the track of the front-most layer in this that covers the picture (a node that does cover it).
    pub(super) fn front_most_covering_track_position(&self) -> usize {
        match self {
            Self::Layer { track_position, .. } => *track_position,
            Self::Group { children, .. } => children
                .iter()
                .rev()
                .find(|child| child.covers_the_picture())
                .map_or(usize::MAX, Node::front_most_covering_track_position),
        }
    }
}

/// Takes away what is behind the front-most thing that covers the picture, in each group and in `nodes`.
pub(super) fn drain_what_is_behind_what_covers(nodes: &mut Vec<Node>) {
    for node in nodes.iter_mut() {
        if let Node::Group { children, .. } = node {
            drain_what_is_behind_what_covers(children);
        }
    }
    if let Some(position) = nodes.iter().rposition(Node::covers_the_picture) {
        // (What has an effect that depends on earlier frames is kept, though it cannot be seen: it has to be made in every frame to go on from the one before, or it would start clear
        // when it was seen again, and the export would not be what it says.)
        let kept: Vec<Node> = nodes
            .drain(..position)
            .filter(Node::depends_on_earlier_frames)
            .collect();
        nodes.splice(0..0, kept);
    }
}

/// The layers of `nodes`, back to front, put in `layers`, and the groups among them in `groups` (the one that holds another first), with their effects.
pub(super) fn flatten(nodes: Vec<Node>, layers: &mut Vec<Layer>, groups: &mut Vec<GroupSpan>) {
    for node in nodes {
        match node {
            Node::Layer { layer, .. } => layers.push(layer),
            Node::Group {
                owner,
                effects,
                over_black,
                children,
            } => {
                let first_layer = layers.len();
                let place = groups.len();
                groups.push(GroupSpan {
                    owner,
                    first_layer,
                    end_layer: first_layer,
                    effects,
                    over_black,
                });
                flatten(children, layers, groups);
                groups[place].end_layer = layers.len();
            }
        }
    }
}
