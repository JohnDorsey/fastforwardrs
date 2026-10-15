// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Moving a track to another place in the list by dragging its title box.
//!
//! While the box is dragged the whole row of the track (the box and the strip) follows the pointer up and down,
//! over the other rows, which move out of its way: the place the dragged track would go to if it was let go of
//! now is the place whose top it is nearest to, and the tracks that are in the way of it there are shown a place
//! along, where the dragged one started (and so, when the dragged one has gone as far as covering the one beside
//! it, that one zips back to the place the dragged one left, and is less covered). When the button is let go the
//! track is put there (by a command) and the rows settle into the places they are in then.
//!
//! The rows are shown at places that are worked out here from their heights (this module knows nothing of what is
//! drawn in them). Where a row is *laid out*, so that the rows area is as tall as all of them and scrolls, is not
//! changed: a row is only drawn somewhere else for as long as it is moving. All of this is counted in points from
//! the top of the rows.

use std::collections::HashMap;

use egui::{CursorIcon, PointerButton, Ui};

use super::header::Handles;
use crate::application::commands::Command;
use crate::project::TrackId;

/// How long a row takes, to go most of the way (63 %) to where it is going, in seconds: the rows settle in about
/// a tenth of a second.
const SECONDS_TO_GO_MOST_OF_THE_WAY: f32 = 0.045;

/// A row that is this near, in points, to where it is going has arrived.
const CLOSE_ENOUGH_TO_HAVE_ARRIVED: f32 = 0.25;

/// How far inside the top and the bottom of the rows the pointer has to be, in points, for dragging a track to bring
/// the rows beyond into view.
const EDGE_OF_THE_ROWS_THAT_SCROLLS: f32 = 28.0;

/// How fast the rows scroll, in points a second, when the pointer is at the edge of the zone that scrolls them,
/// and twice as fast when it is as far past that.
const SCROLL_SPEED_AT_THE_EDGE_OF_THE_ZONE: f32 = 500.0;

/// A track that is being dragged to another place in the list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TrackReorder {
    pub(super) track: TrackId,
    /// How far under the top of its row the pointer took hold of it, in points.
    pub(super) grabbed_at: f32,
    /// Where the track would go if the button were let go now.
    pub(super) to_position: usize,
}

/// How many points the rows scroll down (up, if negative) in a frame that took `seconds`, while a track is dragged and
/// the pointer is at `pointer_y` on the screen and the rows can be seen between `visible` (their top and bottom): they
/// do not while the pointer is well inside, and they do faster the further it is into the zone at the edge or beyond
/// the rows.
pub(super) fn scroll_while_dragging_a_track(pointer_y: Option<f32>, (top, bottom): (f32, f32), seconds: f32) -> f32 {
    let Some(pointer_y) = pointer_y else {
        return 0.0;
    };
    let depth_into_the_top = (top + EDGE_OF_THE_ROWS_THAT_SCROLLS - pointer_y).max(0.0);
    let depth_into_the_bottom = (pointer_y - (bottom - EDGE_OF_THE_ROWS_THAT_SCROLLS)).max(0.0);
    let depth = (depth_into_the_bottom - depth_into_the_top).clamp(
        -2.0 * EDGE_OF_THE_ROWS_THAT_SCROLLS,
        2.0 * EDGE_OF_THE_ROWS_THAT_SCROLLS,
    );
    depth / EDGE_OF_THE_ROWS_THAT_SCROLLS * SCROLL_SPEED_AT_THE_EDGE_OF_THE_ZONE * seconds
}

/// Whether the pointer, which took hold of something at `press_origin`, has gone up or down more than it has gone
/// along, which is what a drag that is to move a track does and what a drag that selects text does not.
fn is_up_or_down(press_origin: egui::Pos2, pointer: egui::Pos2) -> bool {
    let gone = pointer - press_origin;
    gone.y.abs() > gone.x.abs()
}

/// Takes care of the box of the track `track`, which is at place `position` in the list and is shown with its top
/// at `shown_top` on the screen, as the track is taken hold of by `handles`: notes that the track is being dragged
/// when the box is, or the title is dragged up or down (which ends the typing in the title), and when the button
/// is let go adds the command that moves the track to the place it would go to, if that is another.
pub(super) fn follow_the_handles(
    ui: &Ui,
    handles: &Handles,
    (track, position, shown_top): (TrackId, usize, f32),
    reorder: &mut Option<TrackReorder>,
    commands: &mut Vec<Command>,
) {
    let (press, pointer) = ui.input(|input| (input.pointer.press_origin(), input.pointer.interact_pos()));
    let taken_hold_of_by_the_box = handles.box_round_the_title.drag_started_by(PointerButton::Primary);
    let taken_hold_of_by_the_title = handles.title.drag_started_by(PointerButton::Primary)
        && press
            .zip(pointer)
            .is_some_and(|(press, pointer)| is_up_or_down(press, pointer));
    if (taken_hold_of_by_the_box || taken_hold_of_by_the_title)
        && let Some(press) = press
    {
        *reorder = Some(TrackReorder {
            track,
            grabbed_at: press.y - shown_top,
            to_position: position,
        });
    }
    let is_being_moved = reorder.is_some_and(|reorder| reorder.track == track);
    if is_being_moved {
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        // The title box asks for the keys again in every frame that it is dragged (it does, to select text), and
        // would keep them when the button was let go: the keys are not to be typed in it.
        ui.memory_mut(|memory| memory.surrender_focus(handles.title.id));
    }
    if (handles.box_round_the_title.drag_stopped() || handles.title.drag_stopped())
        && let Some(finished) = reorder.take_if(|reorder| reorder.track == track)
        && finished.to_position != position
    {
        commands.push(Command::MoveTrack {
            track,
            to_position: finished.to_position,
        });
    }
}

/// Where the top of each row is when they are in the order of `heights`, with `spacing` between each and the next.
pub(super) fn tops_of(heights: &[f32], spacing: f32) -> Vec<f32> {
    let mut top = 0.0;
    heights
        .iter()
        .map(|height| {
            let this = top;
            top += height + spacing;
            this
        })
        .collect()
}

/// The place in the list of the tracks that a row with its top at `top` goes to if it is let go of, when
/// `other_heights` are the heights of the other rows in their order: the place, among the places it could be put
/// at, where the top of the row would be nearest to where it is now. (The nearest is the lowest, when two are
/// as near.)
pub(super) fn place_nearest_to(other_heights: &[f32], top: f32, spacing: f32) -> usize {
    tops_of_the_places_for_a_row_among(other_heights, spacing)
        .into_iter()
        .enumerate()
        .min_by(|(_, first), (_, second)| (first - top).abs().total_cmp(&(second - top).abs()))
        .map_or(0, |(place, _)| place)
}

/// The tops of the places that one more row could be put at among the rows of `other_heights`: first, after the
/// first, and so on to after the last. They do not depend on how tall the row is.
fn tops_of_the_places_for_a_row_among(other_heights: &[f32], spacing: f32) -> Vec<f32> {
    let mut tops = tops_of(other_heights, spacing);
    tops.push(other_heights.iter().map(|height| height + spacing).sum());
    tops
}

/// Where the top of each row is (in the order the rows are in now) when the row at place `from` is put at place
/// `to` and the others make room for it, keeping their order. The row at `from` is the one that is moved.
pub(super) fn tops_when_moved(heights: &[f32], from: usize, to: usize, spacing: f32) -> Vec<f32> {
    let mut order: Vec<usize> = (0..heights.len()).filter(|row| *row != from).collect();
    order.insert(to.min(order.len()), from);
    let tops = tops_of(&order.iter().map(|row| heights[*row]).collect::<Vec<_>>(), spacing);
    let mut by_row = vec![0.0; heights.len()];
    for (place, row) in order.into_iter().enumerate() {
        by_row[row] = tops[place];
    }
    by_row
}

/// Moves `shown` towards `target` as a row does that has been going for `seconds`, which is a fraction of the
/// distance whatever the length of a frame is (so that two short frames go as far as one long one), and
/// is at `target` once it is near enough.
pub(super) fn chase(shown: f32, target: f32, seconds: f32) -> f32 {
    let remaining = target - shown;
    if remaining.abs() < CLOSE_ENOUGH_TO_HAVE_ARRIVED {
        return target;
    }
    shown + remaining * (1.0 - (-seconds / SECONDS_TO_GO_MOST_OF_THE_WAY).exp())
}

/// Where the rows that are moving are shown now.
#[derive(Debug, Default)]
pub(super) struct RowMotion {
    /// The top of each row that is moving as it is shown, counted from the top of the rows.
    shown_tops: HashMap<TrackId, f32>,
}

impl RowMotion {
    /// Whether no row is moving: all of them are where they are laid out.
    pub(super) fn is_idle(&self) -> bool {
        self.shown_tops.is_empty()
    }

    fn move_towards(&mut self, track: TrackId, target: f32, started_at: f32, seconds: f32) -> f32 {
        let shown = self.shown_tops.entry(track).or_insert(started_at);
        *shown = chase(*shown, target, seconds);
        *shown
    }
}

/// Where the rows are shown in one frame of the window.
#[derive(Debug, PartialEq)]
pub(super) struct PlannedRows {
    /// The top of each row as it is shown, in the order of the tracks.
    pub(super) tops: Vec<f32>,
    /// Whether anything is still moving, which is when the window has to be drawn again soon.
    pub(super) is_moving: bool,
}

/// Works out where each row is shown in this frame of the window. `tracks` are the tracks in their order and
/// how tall each is; `reorder` is the track being dragged, if there is one, with `pointer_y` the place the pointer
/// is at (counted from the top of the rows); `seconds` is how long the last frame took.
///
/// A row that is dragged is where the pointer has it, as far as the rows go. The others go to where they are when
/// it is put where it would go, a little at a time, and when nothing is dragged to where they are laid out.
pub(super) fn plan_rows(
    tracks: &[(TrackId, f32)],
    spacing: f32,
    reorder: Option<&mut TrackReorder>,
    motion: &mut RowMotion,
    pointer_y: Option<f32>,
    seconds: f32,
) -> PlannedRows {
    let heights: Vec<f32> = tracks.iter().map(|(_, height)| *height).collect();
    let laid_out = tops_of(&heights, spacing);

    if let Some(reorder) = reorder
        && let Some(dragged) = tracks.iter().position(|(track, _)| *track == reorder.track)
    {
        let rows_height = laid_out
            .last()
            .zip(heights.last())
            .map_or(0.0, |(top, height)| top + height);
        let dragged_top = match pointer_y {
            Some(y) => y - reorder.grabbed_at,
            None => motion
                .shown_tops
                .get(&reorder.track)
                .copied()
                .unwrap_or(laid_out[dragged]),
        }
        .clamp(0.0, (rows_height - heights[dragged]).max(0.0));
        let others: Vec<f32> = heights
            .iter()
            .enumerate()
            .filter(|(row, _)| *row != dragged)
            .map(|(_, height)| *height)
            .collect();
        reorder.to_position = place_nearest_to(&others, dragged_top, spacing);
        let wanted = tops_when_moved(&heights, dragged, reorder.to_position, spacing);
        let tops = tracks
            .iter()
            .enumerate()
            .map(|(row, (track, _))| {
                if row == dragged {
                    motion.shown_tops.insert(*track, dragged_top);
                    dragged_top
                } else {
                    motion.move_towards(*track, wanted[row], laid_out[row], seconds)
                }
            })
            .collect();
        return PlannedRows { tops, is_moving: true };
    }

    if motion.is_idle() {
        return PlannedRows {
            tops: laid_out,
            is_moving: false,
        };
    }
    let tops: Vec<f32> = tracks
        .iter()
        .enumerate()
        .map(|(row, (track, _))| motion.move_towards(*track, laid_out[row], laid_out[row], seconds))
        .collect();
    let has_arrived = tops.iter().zip(&laid_out).all(|(shown, laid_out)| shown == laid_out);
    if has_arrived {
        motion.shown_tops.clear();
    }
    PlannedRows {
        tops,
        is_moving: !has_arrived,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::DeterministicRandomNumbers;

    const SPACING: f32 = 6.0;
    const FRAME: f32 = 1.0 / 60.0;

    fn track(number: u32) -> TrackId {
        TrackId::new(number)
    }

    fn tracks_of_heights(heights: &[f32]) -> Vec<(TrackId, f32)> {
        heights
            .iter()
            .enumerate()
            .map(|(number, height)| (track(number as u32), *height))
            .collect()
    }

    // --- places ---

    #[test]
    fn rows_are_laid_out_one_under_another_with_the_spacing_between() {
        assert_eq!(tops_of(&[100.0, 50.0, 100.0], SPACING), vec![0.0, 106.0, 162.0]);
        assert_eq!(tops_of(&[], SPACING), Vec::<f32>::new());
    }

    #[test]
    fn a_dragged_row_goes_to_the_place_its_top_is_nearest_to() {
        // Rows of 104 are 110 apart: the places for the third of three are at 0, 110 and 220, whatever its height.
        let others = [104.0, 104.0];
        assert_eq!(place_nearest_to(&others, 0.0, SPACING), 0);
        assert_eq!(place_nearest_to(&others, 54.0, SPACING), 0);
        assert_eq!(
            place_nearest_to(&others, 55.0, SPACING),
            0,
            "half way: the first of the two places"
        );
        assert_eq!(place_nearest_to(&others, 56.0, SPACING), 1);
        assert_eq!(place_nearest_to(&others, 164.0, SPACING), 1);
        assert_eq!(place_nearest_to(&others, 166.0, SPACING), 2);
        assert_eq!(place_nearest_to(&others, -50.0, SPACING), 0, "above all of them");
        assert_eq!(place_nearest_to(&others, 900.0, SPACING), 2, "below all of them");
    }

    #[test]
    fn rows_of_different_heights_have_places_where_the_rows_before_them_end() {
        // After a row of 30 there is room from 36, and after that and one of 80 from 122.
        let others = [30.0, 80.0];
        assert_eq!(place_nearest_to(&others, 17.0, SPACING), 0);
        assert_eq!(place_nearest_to(&others, 19.0, SPACING), 1);
        assert_eq!(place_nearest_to(&others, 78.0, SPACING), 1);
        assert_eq!(place_nearest_to(&others, 80.0, SPACING), 2);
    }

    #[test]
    fn the_only_row_has_only_one_place() {
        assert_eq!(place_nearest_to(&[], 123.0, SPACING), 0);
    }

    #[test]
    fn when_a_row_is_moved_the_ones_in_between_take_its_place_and_the_others_stay() {
        let heights = [104.0, 50.0, 80.0];
        // The first goes to the end: the others go up one place each.
        assert_eq!(tops_when_moved(&heights, 0, 2, SPACING), vec![142.0, 0.0, 56.0]);
        // The last goes to the front.
        assert_eq!(tops_when_moved(&heights, 2, 0, SPACING), vec![86.0, 196.0, 0.0]);
        // The middle one goes up one place, which is past the first.
        assert_eq!(tops_when_moved(&heights, 1, 0, SPACING), vec![56.0, 0.0, 166.0]);
    }

    #[test]
    fn a_row_moved_to_where_it_is_leaves_all_the_rows_where_they_were() {
        let heights = [104.0, 50.0, 80.0, 40.0];
        let laid_out = tops_of(&heights, SPACING);
        for row in 0..heights.len() {
            assert_eq!(tops_when_moved(&heights, row, row, SPACING), laid_out, "row {row}");
        }
    }

    #[test]
    fn moved_rows_never_overlap_and_take_as_much_room_as_before_for_random_rows_and_moves() {
        let mut random = DeterministicRandomNumbers::with_seed(8);
        for round in 0..300 {
            let heights: Vec<f32> = (0..2 + random.next_below(5))
                .map(|_| 28.0 + random.next_below(120) as f32)
                .collect();
            let from = random.next_below(heights.len() as u64) as usize;
            let to = random.next_below(heights.len() as u64 + 2) as usize;
            let tops = tops_when_moved(&heights, from, to, SPACING);

            let mut rows: Vec<(f32, f32)> = tops.iter().zip(&heights).map(|(top, height)| (*top, *height)).collect();
            rows.sort_by(|first, second| first.0.total_cmp(&second.0));
            assert_eq!(rows[0].0, 0.0, "round {round}: they start at the top");
            for pair in rows.windows(2) {
                assert_eq!(
                    pair[1].0,
                    pair[0].0 + pair[0].1 + SPACING,
                    "round {round}: each starts where the one before ends"
                );
            }
            let end = rows.last().map(|row| row.0 + row.1).unwrap();
            let before = tops_of(&heights, SPACING);
            assert_eq!(
                end,
                before.last().unwrap() + heights.last().unwrap(),
                "round {round}: as tall together as before"
            );
            // The ones that were not moved are still in the order they were in.
            let order_of_others: Vec<usize> = {
                let mut others: Vec<usize> = (0..heights.len()).filter(|row| *row != from).collect();
                others.sort_by(|first, second| tops[*first].total_cmp(&tops[*second]));
                others
            };
            assert!(
                order_of_others.windows(2).all(|pair| pair[0] < pair[1]),
                "round {round}"
            );
            // And the moved one is where it was sent.
            let place_of_moved = tops.iter().filter(|top| **top < tops[from]).count();
            assert_eq!(place_of_moved, to.min(heights.len() - 1), "round {round}");
        }
    }

    // --- up or down ---

    #[test]
    fn a_drag_is_up_or_down_when_it_goes_further_up_or_down_than_along() {
        let press = egui::pos2(100.0, 100.0);
        assert!(is_up_or_down(press, egui::pos2(103.0, 110.0)));
        assert!(is_up_or_down(press, egui::pos2(97.0, 85.0)));
        assert!(!is_up_or_down(press, egui::pos2(120.0, 104.0)));
        assert!(!is_up_or_down(press, egui::pos2(60.0, 80.0)));
        assert!(
            !is_up_or_down(press, egui::pos2(110.0, 110.0)),
            "as far one way as the other: along"
        );
    }

    // --- scrolling ---

    #[test]
    fn the_rows_do_not_scroll_while_the_pointer_is_well_inside_them() {
        assert_eq!(scroll_while_dragging_a_track(Some(200.0), (100.0, 400.0), FRAME), 0.0);
        assert_eq!(
            scroll_while_dragging_a_track(Some(128.0), (100.0, 400.0), FRAME),
            0.0,
            "at the edge of the zone"
        );
        assert_eq!(
            scroll_while_dragging_a_track(None, (100.0, 400.0), FRAME),
            0.0,
            "or when there is no pointer"
        );
    }

    #[test]
    fn the_rows_scroll_up_near_the_top_and_down_near_the_bottom_faster_the_further_the_pointer_goes() {
        let at = |y: f32| scroll_while_dragging_a_track(Some(y), (100.0, 400.0), 1.0);
        assert!(at(114.0) < 0.0, "up, inside the top of the rows");
        assert!(at(100.0) < at(114.0), "faster at the very top");
        assert!(at(60.0) < at(100.0), "and faster above it");
        assert!(at(386.0) > 0.0, "down, inside the bottom");
        assert!(at(440.0) > at(400.0));
        assert_eq!(at(0.0), at(-5000.0), "up to a speed that is the most there is");
        assert_eq!(
            at(100.0),
            -500.0,
            "the speed at the top edge is a set number of points a second"
        );
        assert_eq!(at(400.0), 500.0, "as it is at the bottom");
    }

    #[test]
    fn how_far_the_rows_scroll_is_as_much_as_the_frame_was_long() {
        let long = scroll_while_dragging_a_track(Some(400.0), (100.0, 400.0), 0.1);
        let short = scroll_while_dragging_a_track(Some(400.0), (100.0, 400.0), 0.05);
        assert_eq!(long, 2.0 * short);
    }

    // --- moving ---

    #[test]
    fn a_row_chases_where_it_is_going_without_passing_it_and_arrives() {
        let mut shown = 0.0;
        let mut last_remaining = f32::MAX;
        let mut frames = 0;
        while shown != 100.0 {
            shown = chase(shown, 100.0, FRAME);
            let remaining = 100.0 - shown;
            assert!(
                remaining >= 0.0 && remaining < last_remaining,
                "frame {frames}: {shown}"
            );
            last_remaining = remaining;
            frames += 1;
            assert!(frames < 60, "it never arrives");
        }
        assert!(
            (6..=30).contains(&frames),
            "it takes a short while, not {frames} frames"
        );
    }

    #[test]
    fn a_row_goes_the_same_way_in_two_short_frames_as_in_one_long_one() {
        let one_long = chase(0.0, 100.0, 2.0 * FRAME);
        let two_short = chase(chase(0.0, 100.0, FRAME), 100.0, FRAME);
        assert!((one_long - two_short).abs() < 0.01, "{one_long} and {two_short}");
    }

    #[test]
    fn a_row_that_is_near_enough_is_there_and_one_that_is_not_goes_towards_it() {
        assert_eq!(chase(99.9, 100.0, FRAME), 100.0);
        assert_eq!(chase(100.2, 100.0, FRAME), 100.0);
        assert!(chase(98.0, 100.0, FRAME) > 98.0);
        assert!(
            chase(102.0, 100.0, FRAME) < 102.0,
            "and it goes the other way when it is on the other side"
        );
    }

    // --- all the rows of a frame ---

    #[test]
    fn when_no_row_is_dragged_and_none_is_moving_the_rows_are_where_they_are_laid_out() {
        let tracks = tracks_of_heights(&[104.0, 50.0]);
        let mut motion = RowMotion::default();
        let planned = plan_rows(&tracks, SPACING, None, &mut motion, Some(70.0), FRAME);
        assert_eq!(
            planned,
            PlannedRows {
                tops: vec![0.0, 110.0],
                is_moving: false
            }
        );
        assert!(motion.is_idle());
    }

    fn reorder_of(track_number: u32, from: usize, grabbed_at: f32) -> TrackReorder {
        TrackReorder {
            track: track(track_number),
            grabbed_at,
            to_position: from,
        }
    }

    #[test]
    fn a_dragged_row_is_where_the_pointer_has_it_and_the_others_make_room_for_it() {
        let tracks = tracks_of_heights(&[100.0, 100.0, 100.0]);
        let mut motion = RowMotion::default();
        let mut reorder = reorder_of(0, 0, 10.0);

        // The pointer is 10 points under the top of the first row, where it took hold: nothing has moved.
        let planned = plan_rows(&tracks, SPACING, Some(&mut reorder), &mut motion, Some(10.0), FRAME);
        assert_eq!(planned.tops, vec![0.0, 106.0, 212.0]);
        assert_eq!(reorder.to_position, 0);
        assert!(planned.is_moving);

        // It is taken 90 points down: the top of the dragged row is at 80, nearer the second place (106).
        let planned = plan_rows(&tracks, SPACING, Some(&mut reorder), &mut motion, Some(90.0), FRAME);
        assert_eq!(planned.tops[0], 80.0, "the dragged row is where the pointer has it");
        assert_eq!(reorder.to_position, 1);
        assert!(
            planned.tops[1] < 106.0,
            "the second row has begun to go up to where the dragged one was: {planned:?}"
        );
        assert_eq!(planned.tops[2], 212.0, "the third has nothing to make room for");

        // After a while the second is where the dragged row started.
        let mut planned = planned;
        for _ in 0..40 {
            planned = plan_rows(&tracks, SPACING, Some(&mut reorder), &mut motion, Some(90.0), FRAME);
        }
        assert_eq!(planned.tops, vec![80.0, 0.0, 212.0]);
    }

    #[test]
    fn a_dragged_row_stays_among_the_rows_however_far_the_pointer_goes() {
        let tracks = tracks_of_heights(&[100.0, 60.0, 100.0]);
        let mut motion = RowMotion::default();
        let mut reorder = reorder_of(1, 1, 5.0);

        let planned = plan_rows(&tracks, SPACING, Some(&mut reorder), &mut motion, Some(-500.0), FRAME);
        assert_eq!(planned.tops[1], 0.0, "not above the first place");
        assert_eq!(reorder.to_position, 0);

        let planned = plan_rows(&tracks, SPACING, Some(&mut reorder), &mut motion, Some(9000.0), FRAME);
        assert_eq!(
            planned.tops[1], 212.0,
            "not below the last place, where a row of 60 ends at the end of the rows"
        );
        assert_eq!(reorder.to_position, 2);
    }

    #[test]
    fn a_row_that_is_dragged_with_no_pointer_to_say_where_stays_where_it_was() {
        let tracks = tracks_of_heights(&[100.0, 100.0]);
        let mut motion = RowMotion::default();
        let mut reorder = reorder_of(0, 0, 10.0);
        plan_rows(&tracks, SPACING, Some(&mut reorder), &mut motion, Some(60.0), FRAME);
        let planned = plan_rows(&tracks, SPACING, Some(&mut reorder), &mut motion, None, FRAME);
        assert_eq!(planned.tops[0], 50.0);
    }

    #[test]
    fn when_the_row_is_let_go_the_rows_settle_into_the_places_they_are_laid_out_at_and_then_stop_moving() {
        // The first row was dragged down past the second, which is now where it started...
        let mut motion = RowMotion::default();
        let mut reorder = reorder_of(0, 0, 0.0);
        let before = tracks_of_heights(&[100.0, 100.0]);
        for _ in 0..40 {
            plan_rows(&before, SPACING, Some(&mut reorder), &mut motion, Some(80.0), FRAME);
        }
        // ...and let go of: the project now has them the other way round.
        let after = vec![(track(1), 100.0), (track(0), 100.0)];
        let first = plan_rows(&after, SPACING, None, &mut motion, None, FRAME);
        assert!(first.is_moving);
        assert_eq!(
            first.tops[0], 0.0,
            "the one that was out of the way is where it is laid out"
        );
        assert!(
            first.tops[1] > 80.0 && first.tops[1] < 106.0,
            "the dragged one has begun to go to its place: {first:?}"
        );

        let mut planned = first;
        for _ in 0..60 {
            planned = plan_rows(&after, SPACING, None, &mut motion, None, FRAME);
        }
        assert_eq!(
            planned,
            PlannedRows {
                tops: vec![0.0, 106.0],
                is_moving: false
            }
        );
        assert!(motion.is_idle(), "nothing is remembered when everything has arrived");
    }

    #[test]
    fn a_track_that_is_dragged_and_then_not_there_any_more_leaves_the_others_to_settle_and_nothing_moving() {
        let tracks = tracks_of_heights(&[100.0, 100.0]);
        let mut motion = RowMotion::default();
        let mut reorder = reorder_of(0, 0, 0.0);
        plan_rows(&tracks, SPACING, Some(&mut reorder), &mut motion, Some(50.0), FRAME);
        let without_it = vec![(track(1), 100.0)];
        let mut planned = plan_rows(&without_it, SPACING, Some(&mut reorder), &mut motion, Some(50.0), FRAME);
        assert!(
            planned.tops[0] < 106.0,
            "the row that is left goes up to the place the other left: {planned:?}"
        );
        for _ in 0..60 {
            planned = plan_rows(&without_it, SPACING, Some(&mut reorder), &mut motion, Some(50.0), FRAME);
        }
        assert_eq!(
            planned,
            PlannedRows {
                tops: vec![0.0],
                is_moving: false
            }
        );
        assert!(motion.is_idle());
    }
}
