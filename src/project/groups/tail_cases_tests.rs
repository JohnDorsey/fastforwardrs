// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The cases of phase 12 (docs/engine-phase-12-design.md, section 4), found with **the real rules of groups**: every layout of clips on a small grid, every set of clip groups that
//! the editing (`create_group`) accepts, and, for each clip, the places its tail would be present, each classified by what it passes through: how many clip groups it **exits**, how many
//! it **enters** (a group it is not in whose rectangle it lands in), whether two of those entered groups **claim the place** (neither is in the other), and whether it lands on a clip.
//!
//! Two things are checked. The **rules**: the editing accepts a set of groups exactly when a model that is written here in a few lines says it should (every group's rectangle holds only what
//! is in it, and two groups are nested or have nothing in common): that is what `tools/group_cases/overlap_cases.py` assumes, and what the design's numbers come from. And the **cases**: the
//! classes found, and how many of each, are the numbers of that script. A new class (a case nobody had thought of) fails the test with the layout that shows it.
//!
//! The grid is `tracks` x `frames` cells, a clip is one cell, and a tail is `TAIL` cells long on the track of its clip. The grids of two tracks are tests of the program (a few seconds); the one of three tracks (the first that
//! has a place that two groups claim) is `#[ignore]`d (`cargo test --lib tail_cases -- --ignored`) because it takes a minute and a half.

use std::collections::{BTreeMap, HashMap};

use super::{GroupKind, Member};
use crate::project::groups::test_projects::{Laid, lay_out};
use crate::project::{ClipId, GroupId, Project};

const CELL: u64 = 10;
const TAIL: usize = 2;

/// What a tail passes through at a place: the clip groups it leaves, the ones it enters, whether two of those claim the place, and whether it is on a clip.
type Class = (usize, usize, bool, bool);

/// One row of what the script found: the class and how many situations are of it.
type Row = (usize, usize, bool, bool, usize);

/// What `tools/group_cases/overlap_cases.py 2 3 2 2` finds (the layouts, the situations, and the rows).
const SMALL: (usize, usize, &[Row]) = (
    648,
    2690,
    &[
        (0, 0, false, false, 468),
        (0, 0, false, true, 916),
        (0, 1, false, false, 48),
        (0, 1, false, true, 368),
        (0, 2, false, true, 48),
        (1, 0, false, false, 274),
        (1, 0, false, true, 368),
        (1, 1, false, false, 8),
        (1, 1, false, true, 96),
        (2, 0, false, false, 48),
        (2, 0, false, true, 48),
    ],
);

/// `... 2 4 2 2`.
const TWO_BY_FOUR: (usize, usize, &[Row]) = (
    6805,
    45274,
    &[
        (0, 0, false, false, 8212),
        (0, 0, false, true, 14302),
        (0, 1, false, false, 1222),
        (0, 1, false, true, 5894),
        (0, 2, false, false, 100),
        (0, 2, false, true, 1056),
        (1, 0, false, false, 4610),
        (1, 0, false, true, 5894),
        (1, 1, false, false, 254),
        (1, 1, false, true, 1618),
        (2, 0, false, false, 1056),
        (2, 0, false, true, 1056),
    ],
);

/// `... 3 4 2 2`: the first grid that has a place that two groups claim.
const THREE_BY_FOUR: (usize, usize, &[Row]) = (
    474_110,
    4_404_071,
    &[
        (0, 0, false, false, 975_281),
        (0, 0, false, true, 1_355_380),
        (0, 1, false, false, 146_041),
        (0, 1, false, true, 500_997),
        (0, 2, false, false, 17_364),
        (0, 2, false, true, 91_680),
        (0, 2, true, false, 192),
        (1, 0, false, false, 446_589),
        (1, 0, false, true, 500_997),
        (1, 1, false, false, 38_901),
        (1, 1, false, true, 147_289),
        (2, 0, false, false, 91_680),
        (2, 0, false, true, 91_680),
    ],
);

struct Grid {
    tracks: usize,
    frames: usize,
}

/// The cells that have a clip, in order (track by track), for the layout `mask` (bit `track * frames + frame`).
fn occupied(grid: &Grid, mask: u32) -> Vec<(usize, usize)> {
    (0..grid.tracks * grid.frames)
        .filter(|bit| mask >> bit & 1 == 1)
        .map(|bit| (bit / grid.frames, bit % grid.frames))
        .collect()
}

/// A project that has a clip in each of `cells`, and the clip of each.
fn laid_out(grid: &Grid, cells: &[(usize, usize)]) -> (Laid, HashMap<(usize, usize), ClipId>) {
    let mut layout: Vec<Vec<(u64, u64)>> = vec![Vec::new(); grid.tracks];
    for &(track, frame) in cells {
        layout[track].push((frame as u64 * CELL, CELL));
    }
    let slices: Vec<&[(u64, u64)]> = layout.iter().map(Vec::as_slice).collect();
    let laid = lay_out(&slices);
    let mut clip_of = HashMap::new();
    for (track, on_the_track) in laid.clips.iter().enumerate() {
        let mut frames = cells.iter().filter(|cell| cell.0 == track).map(|cell| cell.1);
        for clip in on_the_track {
            clip_of.insert((track, frames.next().expect("a cell for each clip")), *clip);
        }
    }
    (laid, clip_of)
}

/// The cells of the clips that the set of clips `mask` (bits are places in `cells`) is made of.
fn cells_of(cells: &[(usize, usize)], mask: u32) -> Vec<(usize, usize)> {
    cells
        .iter()
        .enumerate()
        .filter(|(place, _)| mask >> place & 1 == 1)
        .map(|(_, cell)| *cell)
        .collect()
}

/// What the rules of the model say: the smallest rectangle that holds the clips of `mask` holds no clip that is not in `mask`.
fn is_convex(cells: &[(usize, usize)], mask: u32) -> bool {
    let inside = cells_of(cells, mask);
    let (first_track, last_track) = (inside.iter().map(|c| c.0).min(), inside.iter().map(|c| c.0).max());
    let (first_frame, last_frame) = (inside.iter().map(|c| c.1).min(), inside.iter().map(|c| c.1).max());
    cells.iter().enumerate().all(|(place, cell)| {
        let in_the_rectangle = first_track <= Some(cell.0)
            && Some(cell.0) <= last_track
            && first_frame <= Some(cell.1)
            && Some(cell.1) <= last_frame;
        mask >> place & 1 == 1 || !in_the_rectangle
    })
}

/// What the rules of the model say about a set of groups, each the set of clips in `masks`: each is convex, and any two are nested or have no clip in common.
fn the_model_accepts(cells: &[(usize, usize)], masks: &[u32]) -> bool {
    masks.iter().all(|mask| is_convex(cells, *mask))
        && masks
            .iter()
            .enumerate()
            .all(|(i, a)| masks[i + 1..].iter().all(|b| a & b == 0 || a & b == *a || a & b == *b))
}

/// Makes the groups of `masks` (each a set of clips) in a copy of the project, the smallest first and nested ones as members of the group that holds them, **as the editing
/// would**: `None` if it refuses one.
fn made_by_the_editing(
    laid: &Laid,
    cells: &[(usize, usize)],
    clip_of: &HashMap<(usize, usize), ClipId>,
    masks: &[u32],
) -> Option<(Project, Vec<(GroupId, u32)>)> {
    let mut project = laid.project.clone();
    let mut sorted = masks.to_vec();
    sorted.sort_by_key(|mask| (mask.count_ones(), *mask));
    let mut made: Vec<(GroupId, u32)> = Vec::new();
    for mask in sorted {
        let inside: Vec<(GroupId, u32)> = made
            .iter()
            .copied()
            .filter(|(_, other)| other & mask == *other && *other != mask)
            .collect();
        let maximal: Vec<(GroupId, u32)> = inside
            .iter()
            .copied()
            .filter(|(_, mine)| !inside.iter().any(|(_, other)| other != mine && other & mine == *mine))
            .collect();
        let covered = maximal.iter().fold(0, |all, (_, other)| all | other);
        let mut members: Vec<Member> = maximal.iter().map(|(group, _)| Member::Group(*group)).collect();
        members.extend(
            cells_of(cells, mask & !covered)
                .into_iter()
                .map(|cell| Member::Clip(clip_of[&cell])),
        );
        let group = project.create_group(GroupKind::ClipGroup, &members).ok()?;
        made.push((group, mask));
    }
    Some((project, made))
}

/// Where the tail of the clip at `from` is, `distance` cells later on its track, according to the **real** groups of the project: what it passes through.
fn class_of(
    project: &Project,
    clip_of: &HashMap<(usize, usize), ClipId>,
    groups: &[(GroupId, u32)],
    cells: &[(usize, usize)],
    from: (usize, usize),
    distance: usize,
) -> Class {
    let place = (from.0, from.1 + distance);
    let clip = clip_of[&from];
    let in_its_groups = |group: GroupId| project.clip_is_inside(clip, group);
    let in_the_rectangle = |group: GroupId| {
        let rectangle = project
            .rectangle_of_the_group(group)
            .expect("the group has a rectangle");
        rectangle.reaches_the_track_at(place.0)
            && rectangle.has_any_of_the_frames(place.1 as u64 * CELL, place.1 as u64 * CELL + CELL)
    };
    let exited = groups
        .iter()
        .filter(|(group, _)| in_its_groups(*group) && !in_the_rectangle(*group))
        .count();
    let entered: Vec<GroupId> = groups
        .iter()
        .filter(|(group, _)| !in_its_groups(*group) && in_the_rectangle(*group))
        .map(|(group, _)| *group)
        .collect();
    let in_one_another = |a: GroupId, b: GroupId| {
        let (of_a, of_b) = (project.clips_in(a), project.clips_in(b));
        of_a.iter().all(|clip| of_b.contains(clip)) || of_b.iter().all(|clip| of_a.contains(clip))
    };
    let claimed_twice = entered
        .iter()
        .enumerate()
        .any(|(i, a)| entered[i + 1..].iter().any(|b| !in_one_another(*a, *b)));
    (exited, entered.len(), claimed_twice, cells.contains(&place))
}

/// Every set of at most `most_groups` groups on `grid` (all the sets of two clips or more, if `every_set`, and only the ones that the model says may be a group otherwise), what
/// the editing accepts, and the classes of the tails of what it accepts. Returns the number of layouts accepted, the situations, and how many of each class.
fn enumerate(grid: &Grid, most_groups: usize, every_set: bool) -> (usize, usize, BTreeMap<Class, usize>) {
    let mut layouts = 0;
    let mut table: BTreeMap<Class, usize> = BTreeMap::new();
    for mask in 1..1_u32 << (grid.tracks * grid.frames) {
        let cells = occupied(grid, mask);
        let (laid, clip_of) = laid_out(grid, &cells);
        let sets: Vec<u32> = (1..1_u32 << cells.len())
            .filter(|set| set.count_ones() >= 2 && (every_set || is_convex(&cells, *set)))
            .collect();
        let mut families: Vec<Vec<u32>> = vec![Vec::new()];
        families.extend(sets.iter().map(|set| vec![*set]));
        if most_groups >= 2 {
            for (i, a) in sets.iter().enumerate() {
                families.extend(sets[i + 1..].iter().map(|b| vec![*a, *b]));
            }
        }
        for family in families {
            let predicted = the_model_accepts(&cells, &family);
            let made = made_by_the_editing(&laid, &cells, &clip_of, &family);
            assert_eq!(
                made.is_some(),
                predicted,
                "the editing and the model disagree about {:?} on the clips {cells:?}",
                family.iter().map(|set| cells_of(&cells, *set)).collect::<Vec<_>>()
            );
            let Some((project, groups)) = made else {
                continue;
            };
            assert_eq!(
                project.problems_with_the_groups(),
                [],
                "what the editing makes breaks no rule"
            );
            layouts += 1;
            for &from in &cells {
                for distance in 1..=TAIL {
                    if from.1 + distance < grid.frames {
                        *table
                            .entry(class_of(&project, &clip_of, &groups, &cells, from, distance))
                            .or_default() += 1;
                    }
                }
            }
        }
    }
    (layouts, table.values().sum(), table)
}

fn the_classes_are(expected: (usize, usize, &[Row]), found: (usize, usize, BTreeMap<Class, usize>)) {
    let (layouts, situations, rows) = expected;
    let expected_table: BTreeMap<Class, usize> = rows
        .iter()
        .map(|&(exited, entered, claimed, on_a_clip, count)| ((exited, entered, claimed, on_a_clip), count))
        .collect();
    assert_eq!(found.2, expected_table, "the classes, and how many places are of each");
    assert_eq!((found.0, found.1), (layouts, situations), "the layouts and the places");
}

#[test]
fn on_a_grid_of_two_tracks_and_three_frames_the_editing_accepts_the_groups_that_the_model_says_and_the_cases_are_the_ones_of_the_design()
 {
    the_classes_are(SMALL, enumerate(&Grid { tracks: 2, frames: 3 }, 2, true));
}

#[test]
fn a_tail_that_lands_where_two_groups_that_are_not_in_one_another_both_have_their_rectangle_is_a_case_of_its_own_and_the_editing_allows_the_groups_that_make_it()
 {
    // The first layout the grid found: tracks 1-2 x frames 1-2 and tracks 2-3 x frames 2-3 share one empty cell on the second track, where the tail of the clip before them lands.
    let grid = Grid { tracks: 3, frames: 4 };
    let cells = [(0, 2), (1, 0), (1, 1), (1, 3), (2, 2)];
    let (laid, clip_of) = laid_out(&grid, &cells);
    let masks = [0b00101, 0b11000]; // {(0,2),(1,1)} and {(1,3),(2,2)}
    let (project, groups) = made_by_the_editing(&laid, &cells, &clip_of, &masks)
        .expect("two clip groups whose rectangles overlap where there is no clip are allowed (G-06)");
    assert_eq!(project.problems_with_the_groups(), []);
    assert_eq!(
        class_of(&project, &clip_of, &groups, &cells, (1, 0), 2),
        (0, 2, true, false)
    );
    assert_eq!(
        class_of(&project, &clip_of, &groups, &cells, (1, 0), 1),
        (0, 1, false, true),
        "the cell before is the clip that is in only one of them"
    );
}

#[test]
fn on_a_grid_of_two_tracks_and_four_frames_the_editing_accepts_the_groups_that_the_model_says_and_the_cases_are_the_ones_of_the_design()
 {
    the_classes_are(TWO_BY_FOUR, enumerate(&Grid { tracks: 2, frames: 4 }, 2, true));
}

#[test]
#[ignore = "takes about a minute and a half; run it with --ignored when the rules of groups change"]
fn on_a_grid_of_three_tracks_and_four_frames_every_group_that_the_model_allows_is_accepted_and_the_cases_are_the_ones_of_the_design()
 {
    the_classes_are(THREE_BY_FOUR, enumerate(&Grid { tracks: 3, frames: 4 }, 2, false));
}
