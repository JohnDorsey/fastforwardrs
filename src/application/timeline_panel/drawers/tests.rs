// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::project::{TrackHeight, TrackId};
use crate::test_support::{RACKS_IN_THE_CRATE, project_with_a_crate_that_has_a_lot_of_racks};

/// How tall the strip of `track` is drawn when the rows are not zoomed up or down: as tall as the track says.
fn strip_height_of(track: &Track) -> f32 {
    track.height().points()
}

fn clip(number: u32) -> AnimationTarget {
    AnimationTarget::Clip(ClipId::new(TrackId::new(0), number))
}

fn track(number: u32) -> AnimationTarget {
    AnimationTarget::Track(TrackId::new(number))
}

// --- the button ---

#[test]
fn the_button_is_five_times_as_wide_as_it_is_tall() {
    assert_eq!(BUTTON_WIDTH / BUTTON_HEIGHT, 5.0);
}

#[test]
fn the_button_covers_less_than_half_of_a_clip_even_at_the_least_height_of_a_track() {
    let clip_rectangle = Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, TrackHeight::SMALLEST_IN_POINTS));
    let button = button_rectangle(clip_rectangle).unwrap();
    assert!(
        button.height() < clip_rectangle.height() / 2.0,
        "{button:?} on {clip_rectangle:?}"
    );
}

#[test]
fn the_button_is_over_the_clip_in_the_middle_with_its_bottom_edge_the_bottom_edge_of_the_clip() {
    let clip_rectangle = Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 104.0));
    let button = button_rectangle(clip_rectangle).unwrap();
    assert_eq!(button.size(), vec2(BUTTON_WIDTH, BUTTON_HEIGHT));
    assert_eq!(button.center().x, 250.0, "in the middle");
    assert!(clip_rectangle.contains_rect(button), "over the clip");
    assert_eq!(button.bottom(), clip_rectangle.bottom(), "the same bottom edge");
}

#[test]
fn the_two_bottom_corners_of_the_button_are_square_and_the_two_at_the_top_are_round() {
    let corners = button_corners();
    assert_eq!((corners.sw, corners.se), (0, 0));
    assert!(corners.nw > 0 && corners.nw == corners.ne);
    assert!(
        f32::from(corners.nw) <= BUTTON_HEIGHT,
        "no rounder than the button is tall"
    );
}

#[test]
fn a_clip_that_is_too_narrow_for_the_whole_button_has_a_smaller_one_and_a_very_narrow_clip_has_none() {
    let narrow = Rect::from_min_size(pos2(0.0, 0.0), vec2(50.0, 104.0));
    let button = button_rectangle(narrow).unwrap();
    assert_eq!(button.width(), 30.0, "room is kept at both sides");
    assert!(narrow.contains_rect(button));
    let very_narrow = Rect::from_min_size(pos2(0.0, 0.0), vec2(35.0, 104.0));
    assert_eq!(button_rectangle(very_narrow), None);
    assert_eq!(
        button_rectangle(Rect::from_min_size(pos2(0.0, 0.0), vec2(0.0, 104.0))),
        None
    );
}

#[test]
fn the_button_is_shown_when_what_it_is_on_is_hovered_when_the_drawer_is_open_and_when_it_has_something_in_it() {
    assert!(button_is_shown(true, false, false));
    assert!(button_is_shown(false, true, false));
    assert!(button_is_shown(false, false, true));
    assert!(button_is_shown(true, true, true));
    assert!(!button_is_shown(false, false, false), "otherwise it is not there");
}

// --- the drawer ---

/// How tall the drawers of the shape tests are.
const HEIGHT: f32 = 71.0;

#[test]
fn a_drawer_that_is_shut_is_not_there() {
    let clip_rectangle = Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 104.0));
    assert_eq!(drawer_shape(clip_rectangle, 27.0, 0.0, HEIGHT), None);
    assert_eq!(drawer_shape(clip_rectangle, 27.0, -0.5, HEIGHT), None);
    assert_eq!(track_drawer_shape(clip_rectangle, 0.0, 0.0, HEIGHT), None);
}

#[test]
fn an_open_drawer_is_as_wide_as_its_clip_and_hangs_from_the_bottom_of_it_reaching_up_into_the_room_by_its_corners() {
    let clip_rectangle = Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 104.0));
    let shape = drawer_shape(clip_rectangle, 27.0, 1.0, HEIGHT).unwrap();
    assert_eq!(shape.rectangle.left(), 100.0);
    assert_eq!(shape.rectangle.right(), 400.0);
    assert_eq!(
        shape.rectangle.top(),
        154.0 - 27.0,
        "up to where the corners of the clip are rounded away"
    );
    assert_eq!(shape.rectangle.bottom(), 154.0 + HEIGHT);
}

#[test]
fn the_two_lower_corners_of_a_drawer_are_rounded_like_the_clip_and_the_two_that_face_the_clip_are_square() {
    let clip_rectangle = Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 104.0));
    let corners = drawer_shape(clip_rectangle, 27.0, 1.0, HEIGHT).unwrap().corners;
    assert_eq!(
        corners,
        CornerRadius {
            nw: 0,
            ne: 0,
            sw: 27,
            se: 27
        }
    );
}

#[test]
fn a_drawer_that_has_hardly_opened_has_corners_no_rounder_than_it_is_tall() {
    let clip_rectangle = Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 104.0));
    let shape = drawer_shape(clip_rectangle, 27.0, 0.05, HEIGHT).unwrap();
    assert!(shape.rectangle.height() < 10.0);
    assert!(f32::from(shape.corners.sw) <= shape.rectangle.height());
    assert_eq!(shape.corners.sw, shape.corners.se);
}

#[test]
fn a_drawer_grows_as_it_opens_in_height_and_in_how_far_it_reaches_up() {
    let clip_rectangle = Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 104.0));
    let mut last = drawer_shape(clip_rectangle, 27.0, 0.01, HEIGHT).unwrap().rectangle;
    for step in 2..=20 {
        let now = drawer_shape(clip_rectangle, 27.0, step as f32 / 20.0, HEIGHT)
            .unwrap()
            .rectangle;
        assert!(
            now.bottom() > last.bottom() && now.top() < last.top(),
            "{last:?} then {now:?}"
        );
        last = now;
    }
}

#[test]
fn the_corners_of_a_drawer_of_a_narrow_clip_are_no_rounder_than_half_its_width() {
    let narrow = Rect::from_min_size(pos2(100.0, 50.0), vec2(30.0, 104.0));
    let shape = drawer_shape(narrow, 15.0, 1.0, HEIGHT).unwrap();
    assert_eq!(shape.corners.sw, 15);
    let very_narrow = Rect::from_min_size(pos2(100.0, 50.0), vec2(20.0, 104.0));
    assert_eq!(drawer_shape(very_narrow, 27.0, 1.0, HEIGHT).unwrap().corners.sw, 10);
}

#[test]
fn the_drawer_of_a_track_is_as_wide_as_the_strip_hangs_from_the_bottom_of_it_and_has_square_corners() {
    let strip = Rect::from_min_size(pos2(180.0, 50.0), vec2(1000.0, 104.0));
    let shape = track_drawer_shape(strip, 0.0, 1.0, HEIGHT).unwrap();
    assert_eq!(shape.rectangle.left(), 180.0);
    assert_eq!(shape.rectangle.right(), 1180.0);
    assert_eq!(shape.rectangle.top(), 154.0, "it does not reach up into the strip");
    assert_eq!(shape.rectangle.bottom(), 154.0 + HEIGHT);
    assert_eq!(shape.corners, CornerRadius::ZERO);
    let half = track_drawer_shape(strip, 0.0, 0.5, HEIGHT).unwrap();
    assert_eq!(half.rectangle.bottom(), 154.0 + HEIGHT / 2.0);
}

#[test]
fn the_drawer_of_a_track_hangs_from_where_the_drawers_of_its_clips_end() {
    let strip = Rect::from_min_size(pos2(180.0, 50.0), vec2(1000.0, 104.0));
    let shape = track_drawer_shape(strip, 40.0, 1.0, HEIGHT).unwrap();
    assert_eq!(shape.rectangle.top(), 154.0 + 40.0);
    assert_eq!(shape.rectangle.bottom(), 154.0 + 40.0 + HEIGHT);
    assert_eq!(
        track_drawer_shape(strip, 40.0, 0.0, HEIGHT),
        None,
        "shut it is not there"
    );
}

#[test]
fn the_racks_of_a_crate_are_in_the_middle_of_it_up_and_down_when_they_fit_and_it_is_as_tall_as_the_strip() {
    let rack = theme::RACK_HEIGHT;
    let one = crate_layout(104.0, 1);
    assert_eq!(one.first_rack_top, (104.0 - rack) / 2.0);
    assert_eq!(one.reach_below_the_strip, 0.0);
    let three = crate_layout(104.0, 3);
    let stack = 3.0 * rack + 2.0 * theme::SPACE_BETWEEN_RACKS;
    assert_eq!(three.first_rack_top, (104.0 - stack) / 2.0);
    assert!(three.first_rack_top >= theme::CRATE_LEAST_MARGIN);
    assert_eq!(three.reach_below_the_strip, 0.0);
    assert_eq!(
        crate_layout(104.0, 0).reach_below_the_strip,
        0.0,
        "nothing to make room for"
    );
}

#[test]
fn a_crate_with_more_racks_than_fit_has_them_from_near_its_top_and_reaches_below_the_strip_as_far_as_they_need() {
    let rack = theme::RACK_HEIGHT;
    let gap = theme::SPACE_BETWEEN_RACKS;
    let strip = 104.0;
    let stack = |count: usize| count as f32 * rack + count.saturating_sub(1) as f32 * gap;
    let mut count = 1;
    while stack(count) + 2.0 * theme::CRATE_LEAST_MARGIN <= strip {
        count += 1;
    }
    let too_many = crate_layout(strip, count);
    assert_eq!(too_many.first_rack_top, theme::CRATE_LEAST_MARGIN);
    assert_eq!(
        too_many.reach_below_the_strip,
        theme::CRATE_LEAST_MARGIN + stack(count) + theme::CRATE_MARGIN - strip
    );
    assert!(too_many.reach_below_the_strip > 0.0);
    // With one more it reaches that much further: another rack and the room between.
    let one_more = crate_layout(strip, count + 1);
    assert_eq!(
        one_more.reach_below_the_strip - too_many.reach_below_the_strip,
        rack + gap
    );
    // The racks all end inside the crate, with the margin under the last.
    let end_of_the_last = one_more.first_rack_top + stack(count + 1);
    assert_eq!(
        strip + one_more.reach_below_the_strip - end_of_the_last,
        theme::CRATE_MARGIN
    );
}

#[test]
fn a_short_strip_has_a_crate_with_even_one_rack_in_the_middle_of_it_if_it_just_fits() {
    let strip = theme::RACK_HEIGHT + 2.0 * theme::CRATE_LEAST_MARGIN;
    assert_eq!(crate_layout(strip, 1).reach_below_the_strip, 0.0);
    assert!(crate_layout(strip - 1.0, 1).reach_below_the_strip > 0.0);
}

#[test]
fn a_drawer_is_as_tall_as_its_racks_and_the_room_round_them_need() {
    let rack = theme::RACK_HEIGHT;
    let radius = f32::from(theme::CLIP_CORNER_RADIUS);
    assert_eq!(
        drawer_height(0),
        theme::SPACE_ABOVE_THE_RACKS + radius,
        "empty: no racks"
    );
    assert_eq!(drawer_height(1), theme::SPACE_ABOVE_THE_RACKS + rack + radius);
    assert_eq!(
        drawer_height(3),
        theme::SPACE_ABOVE_THE_RACKS + 3.0 * rack + 2.0 * theme::SPACE_BETWEEN_RACKS + radius,
        "room between the racks, but none after the last but the room for the corners"
    );
    let heights: Vec<f32> = (0..6).map(drawer_height).collect();
    assert!(heights.windows(2).all(|pair| pair[1] > pair[0]), "{heights:?}");
}

// --- which are open ---

fn advance_for(drawers: &mut Drawers, seconds: f32) -> bool {
    let frames = (seconds * 60.0).round() as usize;
    (0..frames).fold(false, |_, _| drawers.advance(1.0 / 60.0))
}

#[test]
fn a_drawer_is_shut_to_begin_with_and_toggling_opens_it_and_shuts_it_again() {
    let mut drawers = Drawers::default();
    assert!(!drawers.is_open(clip(1)));
    assert_eq!(drawers.openness(clip(1)), 0.0);
    drawers.toggle(clip(1), OpenMode::PushingTheRowsUnder);
    assert!(drawers.is_open(clip(1)));
    drawers.toggle(clip(1), OpenMode::PushingTheRowsUnder);
    assert!(!drawers.is_open(clip(1)));
    assert!(!drawers.is_open(clip(2)), "the drawer of another clip is not touched");
}

#[test]
fn a_drawer_that_is_opened_opens_once_whether_or_not_it_was_already_and_is_not_shut_by_it() {
    let mut drawers = Drawers::default();
    drawers.open_it(clip(1));
    assert!(drawers.is_open(clip(1)));
    drawers.open_it(clip(1));
    assert!(drawers.is_open(clip(1)), "opening what is open leaves it open");
    drawers.toggle(clip(1), OpenMode::PushingTheRowsUnder);
    assert!(!drawers.is_open(clip(1)));
    drawers.open_it(clip(1));
    assert!(drawers.is_open(clip(1)), "and what was shut is opened");
    assert!(!drawers.is_open(clip(2)));
}

// --- the sounds ---

#[test]
fn a_drawer_that_begins_to_open_or_to_shut_announces_a_sound_for_it_and_the_sounds_are_taken_once() {
    let mut drawers = Drawers::default();
    assert_eq!(drawers.take_the_sounds(), vec![], "none to begin with");

    drawers.toggle(clip(1), OpenMode::PushingTheRowsUnder);
    drawers.toggle(track(2), OpenMode::LyingOverTheRowsUnder);
    assert_eq!(
        drawers.take_the_sounds(),
        vec![DrawerSound::Opening, DrawerSound::Opening],
        "one for each drawer that began to open, in order"
    );
    assert_eq!(drawers.take_the_sounds(), vec![], "and they were taken");

    advance_for(&mut drawers, 0.5);
    drawers.toggle(clip(1), OpenMode::PushingTheRowsUnder);
    assert_eq!(drawers.take_the_sounds(), vec![DrawerSound::Closing]);
}

#[test]
fn opening_a_drawer_that_is_open_already_makes_no_sound_and_opening_one_that_is_shutting_makes_one() {
    let mut drawers = Drawers::default();
    drawers.open_it(clip(1));
    drawers.open_it(clip(1));
    assert_eq!(
        drawers.take_the_sounds(),
        vec![DrawerSound::Opening],
        "once, as it opened"
    );

    advance_for(&mut drawers, 0.5);
    drawers.toggle(clip(1), OpenMode::PushingTheRowsUnder);
    advance_for(&mut drawers, 0.05);
    drawers.open_it(clip(1));
    assert_eq!(
        drawers.take_the_sounds(),
        vec![DrawerSound::Closing, DrawerSound::Opening],
        "it shut a little, and then opened again from where it was"
    );
}

#[test]
fn a_drawer_that_goes_with_the_clip_that_is_taken_away_makes_no_sound() {
    let mut drawers = Drawers::default();
    drawers.toggle(clip(77), OpenMode::PushingTheRowsUnder);
    drawers.take_the_sounds();

    drawers.forget_what_is_gone(&Project::empty());

    assert_eq!(drawers.take_the_sounds(), vec![]);
}

#[test]
fn the_slide_of_the_sounds_is_as_long_as_the_slide_of_the_drawer() {
    // The sounds are made by the playback module, which knows nothing of the drawers: the two numbers are the same only
    // because this says so.
    assert_eq!(SECONDS_TO_SLIDE, crate::playback::DRAWER_SOUND_SLIDE_SECONDS);
}

#[test]
fn the_drawer_of_a_track_is_not_the_drawer_of_a_clip_whatever_their_numbers() {
    let mut drawers = Drawers::default();
    drawers.toggle(track(1), OpenMode::PushingTheRowsUnder);
    assert!(drawers.is_open(track(1)));
    assert!(!drawers.is_open(clip(1)));
    assert!(!drawers.is_open(track(2)));
}

#[test]
fn a_drawer_slides_open_in_a_quick_animation_and_is_then_still() {
    let mut drawers = Drawers::default();
    drawers.toggle(clip(1), OpenMode::PushingTheRowsUnder);
    let mut seen = vec![drawers.openness(clip(1))];
    let mut frames_sliding = 0;
    loop {
        let is_sliding = drawers.advance(1.0 / 60.0);
        seen.push(drawers.openness(clip(1)));
        if !is_sliding {
            break;
        }
        frames_sliding += 1;
        assert!(frames_sliding < 60, "it never arrives: {seen:?}");
    }
    assert_eq!(seen[0], 0.0);
    assert_eq!(*seen.last().unwrap(), 1.0);
    assert!(
        seen.windows(2).all(|pair| pair[1] >= pair[0]),
        "it only opens: {seen:?}"
    );
    assert!(seen[1] > 0.0 && seen[1] < 0.5, "it is not open at once: {seen:?}");
    assert!((6..=20).contains(&frames_sliding), "quick: {frames_sliding} frames");
    assert!(!drawers.advance(1.0 / 60.0), "still, once it is open");
}

#[test]
fn a_drawer_that_has_just_been_opened_is_not_forgotten_by_a_frame_in_which_no_time_has_gone_by() {
    let mut drawers = Drawers::default();
    drawers.toggle(clip(1), OpenMode::PushingTheRowsUnder);

    let is_sliding = drawers.advance(0.0);

    assert!(is_sliding, "it has not begun to slide, and is to");
    assert!(drawers.is_open(clip(1)), "it is still open");
    assert_eq!(drawers.openness(clip(1)), 0.0, "and has not slid yet");
    advance_for(&mut drawers, 0.5);
    assert_eq!(drawers.openness(clip(1)), 1.0, "it slides open afterwards");
}

#[test]
fn it_starts_quickly_and_slows_as_it_arrives() {
    let mut drawers = Drawers::default();
    drawers.toggle(clip(1), OpenMode::PushingTheRowsUnder);
    let mut openness = vec![0.0];
    for _ in 0..11 {
        drawers.advance(1.0 / 60.0);
        openness.push(drawers.openness(clip(1)));
    }
    let first_step = openness[1] - openness[0];
    let last_step = openness[11] - openness[10];
    assert!(first_step > 2.0 * last_step, "{openness:?}");
}

#[test]
fn a_drawer_shuts_the_same_way_and_is_forgotten_when_it_has_shut() {
    let mut drawers = Drawers::default();
    drawers.toggle(clip(1), OpenMode::PushingTheRowsUnder);
    advance_for(&mut drawers, 0.5);
    assert_eq!(drawers.openness(clip(1)), 1.0);
    drawers.toggle(clip(1), OpenMode::PushingTheRowsUnder);
    assert!(drawers.advance(1.0 / 60.0), "sliding shut");
    assert!(drawers.openness(clip(1)) < 1.0);
    assert!(
        drawers.is_to_be_seen(clip(1)),
        "some of it is still to be seen as it slides"
    );
    advance_for(&mut drawers, 0.5);
    assert_eq!(drawers.openness(clip(1)), 0.0);
    assert!(!drawers.is_to_be_seen(clip(1)));
    assert!(
        drawers.slides.is_empty(),
        "nothing is remembered of a drawer that is shut"
    );
    assert!(!drawers.advance(1.0 / 60.0));
}

#[test]
fn a_drawer_toggled_while_it_slides_goes_back_from_where_it_is() {
    let mut drawers = Drawers::default();
    drawers.toggle(clip(1), OpenMode::PushingTheRowsUnder);
    advance_for(&mut drawers, 0.09);
    let half_way = drawers.openness(clip(1));
    assert!(half_way > 0.3 && half_way < 1.0, "{half_way}");
    drawers.toggle(clip(1), OpenMode::PushingTheRowsUnder);
    drawers.advance(1.0 / 60.0);
    let after = drawers.openness(clip(1));
    assert!(
        after < half_way && after > 0.0,
        "it turns round where it is: {half_way} then {after}"
    );
}

#[test]
fn a_long_frame_goes_as_far_as_the_same_time_in_short_ones() {
    let mut one_long = Drawers::default();
    one_long.toggle(clip(1), OpenMode::PushingTheRowsUnder);
    one_long.advance(0.05);
    let mut short_ones = Drawers::default();
    short_ones.toggle(clip(1), OpenMode::PushingTheRowsUnder);
    advance_for(&mut short_ones, 0.05);
    assert!((one_long.openness(clip(1)) - short_ones.openness(clip(1))).abs() < 1e-5);
}

#[test]
fn the_drawers_of_clips_and_tracks_that_are_gone_are_forgotten_and_those_that_are_there_are_not() {
    let project = Project::empty();
    let track_that_is_there = AnimationTarget::Track(project.tracks()[0].id());
    let mut drawers = Drawers::default();
    drawers.toggle(clip(1), OpenMode::PushingTheRowsUnder);
    drawers.toggle(track(77), OpenMode::PushingTheRowsUnder);
    drawers.toggle(track_that_is_there, OpenMode::PushingTheRowsUnder);
    drawers.advance(0.05);
    drawers.forget_what_is_gone(&project);
    assert!(!drawers.is_open(clip(1)));
    assert!(!drawers.is_open(track(77)));
    assert!(drawers.is_open(track_that_is_there));
    assert_eq!(drawers.slides.len(), 1);
}

#[test]
fn how_far_a_drawer_hangs_is_how_open_it_is_times_its_height_and_a_shut_one_hangs_not_at_all() {
    let project = Project::empty();
    let owner = AnimationTarget::Track(project.tracks()[0].id());
    let mut drawers = Drawers::default();
    assert_eq!(drawers.hanging_height_of(owner, &project), 0.0);
    drawers.toggle(owner, OpenMode::PushingTheRowsUnder);
    advance_for(&mut drawers, 0.5);
    assert_eq!(drawers.hanging_height_of(owner, &project), drawer_height(0));
    assert_eq!(
        drawers.reach_under_the_row_of(&project.tracks()[0], strip_height_of(&project.tracks()[0]), &project),
        drawer_height(0)
    );
}

#[test]
fn the_drawer_of_a_track_reaches_as_far_under_its_row_as_its_clips_drawers_and_its_own_height_together() {
    let mut project = Project::empty();
    let track_id = project.tracks()[0].id();
    let (text, _) = project
        .insert_text(
            track_id,
            crate::project::TimelineFrameIndex::new(20),
            crate::project::TextObject::default(),
            10,
        )
        .unwrap();
    let mut drawers = Drawers::default();
    drawers.toggle(AnimationTarget::Clip(text), OpenMode::PushingTheRowsUnder);
    advance_for(&mut drawers, 0.5);
    let by_the_clips = drawers.reach_of_the_drawers_of_the_clips_of(
        &project.tracks()[0],
        strip_height_of(&project.tracks()[0]),
        &project,
    );
    assert_eq!(by_the_clips, drawer_height(0));
    drawers.toggle(AnimationTarget::Track(track_id), OpenMode::PushingTheRowsUnder);
    advance_for(&mut drawers, 0.5);
    assert_eq!(
        drawers.reach_under_the_row_of(&project.tracks()[0], strip_height_of(&project.tracks()[0]), &project),
        2.0 * drawer_height(0)
    );
    assert_eq!(
        drawers.reach_of_the_drawers_of_the_clips_of(
            &project.tracks()[0],
            strip_height_of(&project.tracks()[0]),
            &project
        ),
        by_the_clips,
        "the track's own drawer is not one of its clips'"
    );
}

// --- how a drawer opens, and what it does to the rows under it ---

const PUSHING: OpenMode = OpenMode::PushingTheRowsUnder;
const LYING_OVER: OpenMode = OpenMode::LyingOverTheRowsUnder;

#[test]
fn a_drawer_opens_by_pushing_the_rows_under_it_unless_shift_is_held_when_it_is_clicked() {
    assert_eq!(OpenMode::when_clicked(false), PUSHING);
    assert_eq!(OpenMode::when_clicked(true), LYING_OVER);
    assert_eq!(
        OpenMode::default(),
        PUSHING,
        "which is how a drawer opens when nothing says how"
    );
}

#[test]
fn a_drawer_opens_the_way_it_is_asked_to_and_goes_on_that_way_until_it_has_shut() {
    let mut drawers = Drawers::default();
    assert_eq!(
        drawers.mode_of(clip(1)),
        PUSHING,
        "a drawer that is not open: pushing, as far as that goes"
    );
    drawers.toggle(clip(1), LYING_OVER);
    drawers.toggle(clip(2), PUSHING);
    advance_for(&mut drawers, 0.5);
    assert_eq!(
        (drawers.mode_of(clip(1)), drawers.mode_of(clip(2))),
        (LYING_OVER, PUSHING)
    );

    drawers.toggle(clip(1), PUSHING);
    assert!(!drawers.is_open(clip(1)));
    assert_eq!(
        drawers.mode_of(clip(1)),
        LYING_OVER,
        "shutting does not change the way it opened"
    );
    advance_for(&mut drawers, 0.09);
    assert_eq!(drawers.mode_of(clip(1)), LYING_OVER, "nor while it is sliding shut");
    advance_for(&mut drawers, 0.5);
    assert!(
        !drawers.slides.contains_key(&clip(1)),
        "when it has shut it is forgotten"
    );
    drawers.toggle(clip(1), PUSHING);
    assert_eq!(
        drawers.mode_of(clip(1)),
        PUSHING,
        "and opens the way it is asked to when it is next opened"
    );
    advance_for(&mut drawers, 0.5);
    drawers.toggle(clip(1), LYING_OVER);
    advance_for(&mut drawers, 0.5);
    drawers.toggle(clip(1), LYING_OVER);
    assert_eq!(drawers.mode_of(clip(1)), LYING_OVER);
}

#[test]
fn a_drawer_that_is_opened_again_while_it_slides_shut_goes_on_the_way_it_was_so_that_the_rows_do_not_jump() {
    let mut drawers = Drawers::default();
    drawers.toggle(clip(1), LYING_OVER);
    advance_for(&mut drawers, 0.5);
    drawers.toggle(clip(1), PUSHING);
    advance_for(&mut drawers, 0.05);
    assert!(
        drawers.openness(clip(1)) > 0.0,
        "it is sliding shut, and some of it is open"
    );

    drawers.toggle(clip(1), PUSHING);

    assert!(drawers.is_open(clip(1)));
    assert_eq!(drawers.mode_of(clip(1)), LYING_OVER);
}

#[test]
fn a_drawer_that_is_opened_to_show_what_was_put_in_it_pushes_the_rows_under_it() {
    let mut drawers = Drawers::default();
    drawers.open_it(clip(1));
    assert_eq!(drawers.mode_of(clip(1)), PUSHING);
    drawers.toggle(clip(2), LYING_OVER);
    drawers.open_it(clip(2));
    assert_eq!(
        drawers.mode_of(clip(2)),
        LYING_OVER,
        "one that is open is left as it is"
    );
}

/// A track with two text objects, and the drawers of the two clips and of the track, which are opened in the ways given
/// (`None` for a drawer that is shut) and have finished sliding. Gives the track and what is needed to ask about its
/// drawers.
fn a_track_with_drawers_opened_like(
    first_clip: Option<OpenMode>,
    second_clip: Option<OpenMode>,
    the_track: Option<OpenMode>,
) -> (Project, Drawers) {
    use crate::project::{TextObject, TimelineFrameIndex};

    let mut project = Project::empty();
    let track_id = project.tracks()[0].id();
    let (first, _) = project
        .insert_text(track_id, TimelineFrameIndex::new(0), TextObject::default(), 10)
        .unwrap();
    let (second, _) = project
        .insert_text(track_id, TimelineFrameIndex::new(20), TextObject::default(), 10)
        .unwrap();
    // The second clip's drawer is taller by a rack, so that the two reach different distances.
    project
        .add_keyframe(
            AnimationTarget::Clip(second),
            crate::project::PropertyId::Rotation,
            TimelineFrameIndex::new(22),
        )
        .unwrap();
    let mut drawers = Drawers::default();
    for (owner, mode) in [
        (AnimationTarget::Clip(first), first_clip),
        (AnimationTarget::Clip(second), second_clip),
        (AnimationTarget::Track(track_id), the_track),
    ] {
        if let Some(mode) = mode {
            drawers.toggle(owner, mode);
        }
    }
    advance_for(&mut drawers, 0.5);
    (project, drawers)
}

#[test]
fn the_rows_under_a_row_are_pushed_down_by_as_far_as_the_lowest_drawer_of_a_clip_that_pushed_reaches() {
    let (project, drawers) = a_track_with_drawers_opened_like(Some(PUSHING), Some(PUSHING), None);
    let track = &project.tracks()[0];

    assert_eq!(
        drawers.extra_height_of(track, strip_height_of(track), &project),
        drawer_height(1),
        "the second has a rack in its drawer, and the first none: the lowest is the second's"
    );
    assert_eq!(
        drawers.reach_over_the_rows_under_the_row_of(track, strip_height_of(track), &project),
        0.0,
        "all of what hangs from the row is in the row"
    );
    let (project, nothing_open) = a_track_with_drawers_opened_like(None, None, None);
    assert_eq!(
        nothing_open.extra_height_of(&project.tracks()[0], strip_height_of(&project.tracks()[0]), &project),
        0.0
    );
}

#[test]
fn a_drawer_of_a_clip_that_lies_over_the_rows_under_it_does_not_push_them_and_the_one_that_pushes_does() {
    let (project, drawers) = a_track_with_drawers_opened_like(Some(PUSHING), Some(LYING_OVER), None);
    let track = &project.tracks()[0];
    let pushing = drawer_height(0);
    let lying_over = drawer_height(1);

    assert_eq!(
        drawers.extra_height_of(track, strip_height_of(track), &project),
        pushing,
        "only the first pushes"
    );
    assert_eq!(
        drawers.reach_under_the_row_of(track, strip_height_of(track), &project),
        lying_over,
        "the second reaches further"
    );
    assert_eq!(
        drawers.reach_over_the_rows_under_the_row_of(track, strip_height_of(track), &project),
        lying_over - pushing,
        "the rest of it is over the rows under"
    );
}

#[test]
fn a_drawer_of_a_clip_that_lies_over_the_rows_under_it_alone_makes_its_row_no_taller() {
    let (project, drawers) = a_track_with_drawers_opened_like(None, Some(LYING_OVER), None);
    let track = &project.tracks()[0];

    assert_eq!(drawers.extra_height_of(track, strip_height_of(track), &project), 0.0);
    assert_eq!(
        drawers.reach_over_the_rows_under_the_row_of(track, strip_height_of(track), &project),
        drawer_height(1)
    );
}

#[test]
fn the_drawer_of_a_track_that_pushes_makes_room_for_what_it_hangs_from_the_end_of_as_well_as_for_itself() {
    // The drawer of the second clip, which lies over the rows under it, is what the drawer of the track hangs under: that
    // is in the row, because the drawer of the track that is under it pushes.
    let (project, drawers) = a_track_with_drawers_opened_like(None, Some(LYING_OVER), Some(PUSHING));
    let track = &project.tracks()[0];

    assert_eq!(
        drawers.extra_height_of(track, strip_height_of(track), &project),
        drawer_height(1) + drawer_height(0)
    );
    assert_eq!(
        drawers.reach_over_the_rows_under_the_row_of(track, strip_height_of(track), &project),
        0.0
    );
}

#[test]
fn the_drawer_of_a_track_that_lies_over_the_rows_under_it_pushes_nothing_and_the_drawers_of_its_clips_that_pushed_still_do()
 {
    let (project, drawers) = a_track_with_drawers_opened_like(Some(PUSHING), None, Some(LYING_OVER));
    let track = &project.tracks()[0];

    assert_eq!(
        drawers.extra_height_of(track, strip_height_of(track), &project),
        drawer_height(0),
        "the clip's drawer pushes"
    );
    assert_eq!(
        drawers.reach_over_the_rows_under_the_row_of(track, strip_height_of(track), &project),
        drawer_height(0),
        "and the drawer of the track, which hangs under it, is over the rows under"
    );
}

#[test]
fn what_pushes_grows_with_how_far_the_drawer_has_slid_and_what_lies_over_the_rows_does_not_push_at_any_moment() {
    use crate::project::{TextObject, TimelineFrameIndex};

    let mut project = Project::empty();
    let track_id = project.tracks()[0].id();
    let (clip, _) = project
        .insert_text(track_id, TimelineFrameIndex::new(0), TextObject::default(), 10)
        .unwrap();
    let mut pushing = Drawers::default();
    pushing.toggle(AnimationTarget::Clip(clip), PUSHING);
    let mut lying_over = Drawers::default();
    lying_over.toggle(AnimationTarget::Clip(clip), LYING_OVER);
    let mut extras = Vec::new();
    for _ in 0..12 {
        pushing.advance(1.0 / 60.0);
        lying_over.advance(1.0 / 60.0);
        extras.push(pushing.extra_height_of(&project.tracks()[0], strip_height_of(&project.tracks()[0]), &project));
        assert_eq!(
            lying_over.extra_height_of(&project.tracks()[0], strip_height_of(&project.tracks()[0]), &project),
            0.0
        );
    }
    assert!(
        extras.windows(2).all(|pair| pair[1] >= pair[0]) && extras[0] < extras[11],
        "{extras:?}"
    );
    assert_eq!(
        *extras.last().unwrap(),
        pushing.reach_under_the_row_of(&project.tracks()[0], strip_height_of(&project.tracks()[0]), &project)
    );
}

// --- bringing a drawer that opens into view ---

#[test]
fn the_drawer_that_was_opened_last_is_the_one_that_is_brought_into_view_until_it_has_stopped_sliding() {
    let mut drawers = Drawers::default();
    assert_eq!(drawers.revealing(), None);
    drawers.toggle(clip(1), PUSHING);
    assert_eq!(drawers.revealing(), Some(clip(1)));
    drawers.toggle(clip(2), PUSHING);
    assert_eq!(drawers.revealing(), Some(clip(2)), "the last one");

    drawers.advance(0.05);
    drawers.stop_revealing_if_it_has_stopped_sliding();
    assert_eq!(drawers.revealing(), Some(clip(2)), "while it slides");
    advance_for(&mut drawers, 0.5);
    drawers.stop_revealing_if_it_has_stopped_sliding();
    assert_eq!(drawers.revealing(), None, "when it has stopped");
}

#[test]
fn a_drawer_that_is_shut_is_not_brought_into_view_and_one_that_is_gone_is_forgotten() {
    let mut drawers = Drawers::default();
    drawers.toggle(clip(1), PUSHING);
    advance_for(&mut drawers, 0.5);
    drawers.stop_revealing_if_it_has_stopped_sliding();
    drawers.toggle(clip(1), PUSHING);
    assert_eq!(drawers.revealing(), None, "shutting is not opening");

    let project = Project::empty();
    drawers.toggle(clip(3), PUSHING);
    assert_eq!(drawers.revealing(), Some(clip(3)));
    drawers.forget_what_is_gone(&project);
    assert_eq!(drawers.revealing(), None, "there is no such clip");
}

#[test]
fn the_room_that_the_drawers_of_a_row_need_is_worked_out_for_the_strip_as_it_is_drawn_when_the_rows_are_zoomed() {
    let (project, crate_track) = project_with_a_crate_that_has_a_lot_of_racks();
    let mut drawers = Drawers::default();
    drawers.toggle(AnimationTarget::Track(crate_track), OpenMode::PushingTheRowsUnder);
    advance_for(&mut drawers, 0.5);
    let position = project
        .tracks()
        .iter()
        .position(|track| track.id() == crate_track)
        .unwrap();
    let track = &project.tracks()[position];

    // The rows are zoomed down to a half: the strip is drawn as tall as half of what the track says.
    let layout = crate::application::timeline_panel::rows::RowsLayout::of(
        &project,
        &drawers,
        crate::application::timeline_panel::vertical_zoom::VerticalZoom::of(0.5),
    );

    let strip = layout.strip_heights[position];
    assert!(
        strip < track.height().points() - 20.0,
        "the setup: the strip is drawn shorter: {strip}"
    );
    let wanted = drawers.extra_height_of(track, strip, &project);
    let if_it_were_not_zoomed = drawers.extra_height_of(track, track.height().points(), &project);
    assert!(
        (wanted - if_it_were_not_zoomed).abs() > 20.0,
        "the setup: the room the crate and the drawer need is not the same for the strip as it is: {wanted} and {if_it_were_not_zoomed}"
    );
    assert_eq!(layout.heights[position], strip + wanted);

    // With nothing open the crate of the first row lies over the row under it, and past it: how far past is what the room under the last
    // row is.
    assert_eq!(
        (position, layout.heights.len()),
        (0, 2),
        "the setup: the crate is on the first of two rows"
    );
    let nothing_open = Drawers::default();
    let layout = crate::application::timeline_panel::rows::RowsLayout::of(
        &project,
        &nothing_open,
        crate::application::timeline_panel::vertical_zoom::VerticalZoom::of(0.5),
    );
    let reach_over_the_row_under = nothing_open.reach_over_the_rows_under_the_row_of(track, strip, &project);
    let if_it_were_not_zoomed =
        nothing_open.reach_over_the_rows_under_the_row_of(track, track.height().points(), &project);
    assert!(
        (reach_over_the_row_under - if_it_were_not_zoomed).abs() > 20.0,
        "the setup: how far the crate reaches depends on the strip: {reach_over_the_row_under} and {if_it_were_not_zoomed}"
    );
    assert_eq!(
        layout.room_after_the_last_row,
        (reach_over_the_row_under - crate::application::theme::SPACE_BETWEEN_TRACKS - layout.heights[1]).max(0.0)
    );
    assert!(
        layout.room_after_the_last_row > 10.0,
        "the setup: it does reach past the row under it"
    );
}

#[test]
fn how_far_a_keyframe_crate_reaches_below_its_strip_is_told_by_how_tall_the_strip_is_drawn_and_not_by_the_height_of_its_track()
 {
    let (project, crate_track) = project_with_a_crate_that_has_a_lot_of_racks();
    let drawers = Drawers::default();
    let track = project.track(crate_track).unwrap();

    let reach_at = |strip_height: f32| drawers.reach_under_the_row_of(track, strip_height, &project);

    assert_eq!(
        reach_at(104.0),
        crate_layout(104.0, RACKS_IN_THE_CRATE).reach_below_the_strip
    );
    assert_eq!(
        reach_at(44.0),
        crate_layout(44.0, RACKS_IN_THE_CRATE).reach_below_the_strip
    );
    assert!(
        reach_at(44.0) > reach_at(104.0) + 50.0,
        "a row that is drawn shorter has the crate reach further below it: {} and {}",
        reach_at(44.0),
        reach_at(104.0)
    );
    assert_eq!(
        reach_at(312.0),
        0.0,
        "and a row that is drawn three times as tall has room for all the racks in it"
    );
    assert_eq!(
        drawers.extra_height_of(track, 44.0, &project),
        0.0,
        "whichever it is, the crate lies over the rows under it and does not push them"
    );
}

#[test]
fn a_keyframe_crate_with_more_racks_than_fit_in_it_lies_over_the_rows_under_it_and_does_not_push_them() {
    let (project, crate_track) = project_with_a_crate_that_has_a_lot_of_racks();
    let drawers = Drawers::default();
    let track = project.track(crate_track).unwrap();
    let overhang = crate_layout(track.height().points(), RACKS_IN_THE_CRATE).reach_below_the_strip;
    assert!(overhang > 0.0, "the racks do not fit in the crate");

    assert_eq!(
        drawers.reach_under_the_row_of(track, strip_height_of(track), &project),
        overhang,
        "the crate hangs below its row"
    );
    assert_eq!(
        drawers.extra_height_of(track, strip_height_of(track), &project),
        0.0,
        "and does not make the row taller: what is under it is not pushed down"
    );
    assert_eq!(
        drawers.reach_over_the_rows_under_the_row_of(track, strip_height_of(track), &project),
        overhang,
        "all of it is over the rows under"
    );
}

#[test]
fn a_keyframe_crate_has_no_drawer_to_reach_with_even_if_it_has_racks_that_would_make_it_reach_further() {
    use crate::project::{PropertyId, TextObject, TimelineFrameIndex};

    let mut project = Project::empty();
    let front = project.tracks()[0].id();
    let behind = project.add_track_in_front();
    let (text, _) = project
        .insert_text(front, TimelineFrameIndex::new(20), TextObject::default(), 10)
        .unwrap();
    // A crate on the track that is in front, over the same frames, with a rack in it (which makes it taller than a
    // drawer that has none).
    let crate_clip = crate::test_support::make_a_crate(
        &mut project,
        behind,
        20,
        29,
        AnimationTarget::Clip(text),
        PropertyId::Rotation,
    );
    let mut drawers = Drawers::default();
    drawers.toggle(AnimationTarget::Clip(crate_clip), OpenMode::PushingTheRowsUnder);
    advance_for(&mut drawers, 0.5);

    assert_eq!(
        drawers.reach_of_the_drawers_of_the_clips_of(
            project.track(behind).unwrap(),
            strip_height_of(project.track(behind).unwrap()),
            &project
        ),
        0.0,
        "a crate is nothing but a drawer that is always open: it has no other to open, and one rack fits in it"
    );
    assert!(!has_an_accessible_drawer(
        project.placement_of(crate_clip).unwrap().clip
    ));
    assert!(has_an_accessible_drawer(project.placement_of(text).unwrap().clip));
}
