// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The outlines of groups on the timeline: a line in the colour of the group that goes round everything that is in it, drawn over the clips
//! and under the playhead, 2 points thick (3 while the group is selected).
//!
//! * A **clip group** has a rounded rectangle round its members: the rectangle of frames and tracks that holds them (see
//!   [`crate::project::Project::rectangle_of_the_group`]). It goes across the rows between the first track and the last, with the drawers that
//!   are open between them.
//! * A **track group** has the same, reaching across the whole of the timeline (from frame 0 to the end of the project), and reaching to the
//!   left over the boxes of the titles of its tracks.
//! * A **tag** is round members that are anywhere: one loop that goes round all of them, and the *spokes* that join them (rectangles, which are
//!   not drawn, that are in the rows and columns that two members have in common, or bend once between two that have none), as a loop of yarn
//!   would, rounded off where it turns (see [`shape`]). The loop goes over the clips that are between the members and are not in the tag.
//!
//! The outline of a group is always **outside** those of the groups in it: each level of groups in groups is 4 points further out than the
//! level below. A tag goes round the outlines of the groups that are in it, as it does round its clips.
//!
//! What can be done to a group on the timeline is in [`super::group_title_bars`] (its name, which is where it is clicked and dragged, when the
//! pointer is over it) and [`super::group_edges`] (the edges of a clip group, which are moved to take in or let go of clips).

use std::collections::HashMap;

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2};

use self::shape::{Piece, outlines_of, spokes_between};
use super::clips::rectangles_of;
use super::strip_view::StripView;
use crate::application::selection::Selection;
use crate::application::theme;
use crate::project::{ClipGroupEdge, Group, GroupId, GroupKind, Member, Project, Track};

mod shape;

/// How far the outline of a group that has no group in it is from what it goes round, in points, and how much further out it is for each level of
/// groups that are in groups.
const PADDING_OF_THE_INNERMOST: f32 = 3.0;
const PADDING_FOR_EACH_LEVEL: f32 = 4.0;

/// How wide the spokes that join the members of a tag are, in points.
const SPOKE_WIDTH: f32 = 8.0;

/// How round an outline is where it turns inwards, in points.
const INWARD_CORNER_RADIUS: f32 = 8.0;

/// How thick the outline of a group is, and how thick it is while the group is selected, in points.
const THICKNESS: f32 = 2.0;
const THICKNESS_WHEN_SELECTED: f32 = 3.0;

/// How far above the first row an outline is let go, in points: as far out as the outline of a group that has two levels of groups in it
/// goes (11), and the line itself.
pub(super) const MOST_ROOM_ABOVE_THE_ROWS: f32 =
    PADDING_OF_THE_INNERMOST + 2.0 * PADDING_FOR_EACH_LEVEL + THICKNESS_WHEN_SELECTED;

/// Where the rows of the timeline are on the screen, which is what the outlines are put round.
#[derive(Clone, Copy)]
pub(super) struct WhereTheRowsAre<'a> {
    /// The strip of each track, in the order of the tracks.
    pub views: &'a [(&'a Track, StripView)],
    /// How far left the boxes of the titles of the tracks begin: where the outline of a track group reaches to.
    pub left_of_the_title_boxes: f32,
    /// The edge of a clip group that is shown somewhere else than where the group has it, while the edge is being moved.
    pub an_edge_shown_elsewhere: Option<EdgeShownElsewhere>,
}

/// An edge of the rectangle of a clip group that is drawn somewhere else than where the group has it: at a place that is a number of frames
/// along (not a whole number while it moves).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct EdgeShownElsewhere {
    pub group: GroupId,
    pub edge: ClipGroupEdge,
    pub frame: f64,
}

/// The outline of one group, as the loops that go round it.
pub(super) struct OutlineOfAGroup {
    pub group: GroupId,
    pub colour: Color32,
    pub kind: GroupKind,
    /// How many levels of groups in groups there are in it: 0 if there are none.
    pub height: usize,
    pub loops: Vec<Vec<Pos2>>,
}

/// The outline of every group of `project`, the groups that have the most in them (the outermost) first, so that they are drawn first and what
/// is in them over them.
pub(super) fn outlines_of_the_groups(project: &Project, rows: WhereTheRowsAre<'_>) -> Vec<OutlineOfAGroup> {
    let mut working = Working {
        project,
        rows,
        heights: HashMap::new(),
        bases: HashMap::new(),
    };
    let mut outlines: Vec<OutlineOfAGroup> = project
        .groups()
        .iter()
        .map(|group| {
            let height = working.height_of(group.id());
            let padding = padding_at(height);
            let pieces: Vec<Piece> = working
                .base_rectangles_of(group.id())
                .into_iter()
                .map(|rectangle| Piece {
                    rectangle: rectangle.expand(padding),
                    radius: f32::from(theme::CLIP_CORNER_RADIUS) + padding,
                })
                .collect();
            OutlineOfAGroup {
                group: group.id(),
                colour: colour_of(group),
                kind: group.kind(),
                height,
                loops: outlines_of(&pieces, INWARD_CORNER_RADIUS),
            }
        })
        .collect();
    // (Outermost first; and among those that are as high, the ones that were made first first.)
    outlines.sort_by_key(|outline| std::cmp::Reverse(outline.height));
    outlines
}

/// How far out the outline of a group that has `height` levels of groups in it is.
fn padding_at(height: usize) -> f32 {
    PADDING_OF_THE_INNERMOST + PADDING_FOR_EACH_LEVEL * height as f32
}

fn colour_of(group: &Group) -> Color32 {
    let colour = group.colour();
    Color32::from_rgb(colour.red, colour.green, colour.blue)
}

/// What is worked out while the outlines are: how high each group is, and the rectangles that each goes round.
struct Working<'a> {
    project: &'a Project,
    rows: WhereTheRowsAre<'a>,
    heights: HashMap<GroupId, usize>,
    bases: HashMap<GroupId, Vec<Rect>>,
}

impl Working<'_> {
    /// How many levels of groups in groups there are in the group `group`: none if none of what is in it is a group. (A group that is in
    /// itself, which the rules do not allow, is not looked into for ever.)
    fn height_of(&mut self, group: GroupId) -> usize {
        self.height_below(group, self.project.groups().len())
    }

    fn height_below(&mut self, group: GroupId, depth: usize) -> usize {
        if let Some(height) = self.heights.get(&group) {
            return *height;
        }
        let Some(found) = self.project.group(group) else {
            return 0;
        };
        let members: Vec<GroupId> = found
            .members()
            .iter()
            .filter_map(|member| match member {
                Member::Group(inner) => Some(*inner),
                Member::Clip(_) | Member::Track(_) => None,
            })
            .collect();
        let height = match depth.checked_sub(1) {
            Some(depth) => members
                .into_iter()
                .map(|inner| 1 + self.height_below(inner, depth))
                .max()
                .unwrap_or(0),
            None => 0,
        };
        self.heights.insert(group, height);
        height
    }

    /// The rectangles that the outline of `group` goes round, before it is padded: for a clip group or a track group the rectangle that holds
    /// its members, and for a tag the places of all of its members and the spokes that join them. (None for a group that has nothing in it, or
    /// whose members are not in the rows.)
    fn base_rectangles_of(&mut self, group: GroupId) -> Vec<Rect> {
        self.base_rectangles_below(group, self.project.groups().len())
    }

    fn base_rectangles_below(&mut self, group: GroupId, depth: usize) -> Vec<Rect> {
        if let Some(rectangles) = self.bases.get(&group) {
            return rectangles.clone();
        }
        let Some(found) = self.project.group(group) else {
            return Vec::new();
        };
        let rectangles = match found.kind() {
            GroupKind::ClipGroup => self
                .where_the_rectangle_of_the_group_is(group, false)
                .into_iter()
                .collect(),
            GroupKind::TrackGroup => self
                .where_the_rectangle_of_the_group_is(group, true)
                .into_iter()
                .collect(),
            GroupKind::Tag => {
                let members = found.members().to_vec();
                let mut components: Vec<Vec<Rect>> = Vec::new();
                for member in members {
                    let component = match member {
                        Member::Clip(clip) => self.where_the_clip_is(clip).into_iter().collect(),
                        Member::Group(inner) => match depth.checked_sub(1) {
                            Some(depth) => self.base_rectangles_below(inner, depth),
                            None => Vec::new(),
                        },
                        Member::Track(_) => Vec::new(),
                    };
                    if !component.is_empty() {
                        components.push(component);
                    }
                }
                let mut all: Vec<Rect> = components.iter().flatten().copied().collect();
                all.extend(spokes_between(&components, SPOKE_WIDTH));
                all
            }
        };
        self.bases.insert(group, rectangles.clone());
        rectangles
    }

    /// Where the clip `clip` is on the screen: its whole length on the strip of its track, even where it is scrolled out of view.
    fn where_the_clip_is(&self, clip: crate::project::ClipId) -> Option<Rect> {
        let position = self.project.track_position_holding(clip)?;
        let (_, view) = self.rows.views.get(position)?;
        let placement = self.project.placement_of(clip)?;
        Some(rectangles_of(&placement, view).whole)
    }

    /// Where the rectangle of the group `group` is on the screen: from the first of its frames to the end of the last, and across the strips of
    /// all its tracks (and what is between them). `over_the_title_boxes` has it reach left to the title boxes of the tracks.
    fn where_the_rectangle_of_the_group_is(&self, group: GroupId, over_the_title_boxes: bool) -> Option<Rect> {
        let rectangle = self.project.rectangle_of_the_group(group)?;
        let (_, first) = self.rows.views.get(rectangle.first_track)?;
        let (_, last) = self.rows.views.get(rectangle.last_track)?;
        let shown_at = |edge: ClipGroupEdge, frame: u64| match self.rows.an_edge_shown_elsewhere {
            Some(elsewhere) if elsewhere.group == group && elsewhere.edge == edge => {
                first.screen_x_of_fractional_frame(elsewhere.frame)
            }
            _ => first.screen_x_of_frame(frame),
        };
        let left = if over_the_title_boxes {
            self.rows.left_of_the_title_boxes
        } else {
            shown_at(ClipGroupEdge::Left, rectangle.first_frame)
        };
        let right = shown_at(ClipGroupEdge::Right, rectangle.end_frame).max(left);
        Some(Rect::from_min_max(
            pos2(left, first.rectangle.top()),
            pos2(right, last.rectangle.bottom()),
        ))
    }
}

/// Draws `outlines` (see [`outlines_of_the_groups`]) over what is drawn in `painter` so far: in the colour of each group, thicker for those
/// that are selected. A clip group or a tag is only drawn where the strips are (`strips_area`), and a track group where the title boxes are
/// as well (`rows_area`).
pub(super) fn draw(
    painter: &Painter,
    outlines: &[OutlineOfAGroup],
    selection: &Selection,
    (strips_area, rows_area): (Rect, Rect),
) {
    for outline in outlines {
        let thickness = if selection.contains_group(outline.group) {
            THICKNESS_WHEN_SELECTED
        } else {
            THICKNESS
        };
        let area = if outline.kind == GroupKind::TrackGroup {
            rows_area
        } else {
            strips_area
        };
        let painter = painter.with_clip_rect(area.intersect(painter.clip_rect()));
        for points in &outline.loops {
            painter.add(Shape::closed_line(
                points.clone(),
                Stroke::new(thickness, outline.colour),
            ));
        }
    }
}

#[cfg(test)]
mod tests;
