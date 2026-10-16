// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The governor of the size of the frames, with times that are made up.

use super::*;

/// A frame of the project lasts a tenth of a second here, so that the parts of it are easy to count.
const A_FRAME: Duration = Duration::from_millis(100);

fn governor() -> QualityGovernor {
    QualityGovernor::new(QualityRules::default())
}

fn took(milliseconds: u64) -> Duration {
    Duration::from_millis(milliseconds)
}

/// Frames that came while the clock was going, each having taken as long as the numbers say, in milliseconds.
fn came(governor: &mut QualityGovernor, latencies: &[u64]) {
    for latency in latencies {
        governor.a_frame_came(took(*latency), A_FRAME, true);
    }
}

const LATE: u64 = 95;
const COMFORTABLE: u64 = 20;
const IN_THE_BAND: u64 = 50;

#[test]
fn the_rules_are_the_numbers_of_the_design() {
    let rules = QualityRules::default();
    assert_eq!(rules.late_above, 0.8);
    assert_eq!(rules.comfortable_below, 0.35);
    assert_eq!(rules.late_frames_that_make_it_smaller, 2);
    assert_eq!(rules.comfortable_frames_that_make_it_bigger, 10);
    assert_eq!(
        rules.how_long_before_a_still_picture_is_made_whole,
        Duration::from_millis(120)
    );
}

#[test]
fn the_sizes_of_the_scales_keep_the_shape_of_the_picture_and_are_never_less_than_two_pixels() {
    assert_eq!(PreviewScale::Whole.of_the_size((1600, 900)), (1600, 900));
    assert_eq!(PreviewScale::Half.of_the_size((1600, 900)), (800, 450));
    assert_eq!(PreviewScale::Quarter.of_the_size((1600, 900)), (400, 225));
    assert_eq!(
        PreviewScale::Half.of_the_size((501, 333)),
        (251, 167),
        "rounded: 250.5 and 166.5 go up"
    );
    assert_eq!(PreviewScale::Quarter.of_the_size((5, 3)), (2, 2));
    assert_eq!(PreviewScale::Half.of_the_size((1, 1)), (2, 2));
    assert_eq!(
        PreviewScale::Whole.of_the_size((1, 1)),
        (1, 1),
        "a whole picture is the picture, whatever it is"
    );
}

#[test]
fn it_begins_with_whole_pictures() {
    assert_eq!(governor().scale(), PreviewScale::Whole);
}

#[test]
fn two_late_frames_in_a_row_make_the_picture_smaller_and_one_does_not() {
    let mut governor = governor();
    came(&mut governor, &[LATE]);
    assert_eq!(governor.scale(), PreviewScale::Whole);
    came(&mut governor, &[LATE]);
    assert_eq!(governor.scale(), PreviewScale::Half);
}

#[test]
fn a_late_frame_that_is_followed_by_one_that_is_not_late_does_not_count_with_the_next_one() {
    for in_between in [COMFORTABLE, IN_THE_BAND] {
        let mut governor = governor();
        came(&mut governor, &[LATE, in_between, LATE]);
        assert_eq!(governor.scale(), PreviewScale::Whole, "in between: {in_between}");
        came(&mut governor, &[LATE]);
        assert_eq!(governor.scale(), PreviewScale::Half, "and two in a row after it do");
    }
}

#[test]
fn it_goes_on_to_a_quarter_and_no_further() {
    let mut governor = governor();
    came(&mut governor, &[LATE, LATE]);
    assert_eq!(governor.scale(), PreviewScale::Half);
    came(&mut governor, &[LATE]);
    assert_eq!(
        governor.scale(),
        PreviewScale::Half,
        "the count begins again after a step: one late frame is not two"
    );
    came(&mut governor, &[LATE]);
    assert_eq!(governor.scale(), PreviewScale::Quarter);
    came(&mut governor, &[LATE, LATE, LATE, LATE, LATE]);
    assert_eq!(governor.scale(), PreviewScale::Quarter);
}

#[test]
fn ten_comfortable_frames_in_a_row_make_the_picture_bigger_and_nine_do_not() {
    let mut governor = governor();
    came(&mut governor, &[LATE, LATE, LATE, LATE]);
    assert_eq!(governor.scale(), PreviewScale::Quarter);
    came(&mut governor, &[COMFORTABLE; 9]);
    assert_eq!(governor.scale(), PreviewScale::Quarter);
    came(&mut governor, &[COMFORTABLE]);
    assert_eq!(governor.scale(), PreviewScale::Half);
    came(&mut governor, &[COMFORTABLE; 9]);
    assert_eq!(
        governor.scale(),
        PreviewScale::Half,
        "the count begins again after a step"
    );
    came(&mut governor, &[COMFORTABLE]);
    assert_eq!(governor.scale(), PreviewScale::Whole);
    came(&mut governor, &[COMFORTABLE; 30]);
    assert_eq!(governor.scale(), PreviewScale::Whole, "it is not bigger than whole");
}

#[test]
fn a_frame_that_is_late_or_in_the_band_stops_the_count_of_the_comfortable_ones() {
    for spoiler in [LATE, IN_THE_BAND] {
        let mut governor = governor();
        came(&mut governor, &[LATE, LATE]);
        assert_eq!(governor.scale(), PreviewScale::Half);
        came(&mut governor, &[COMFORTABLE; 9]);
        came(&mut governor, &[spoiler]);
        came(&mut governor, &[COMFORTABLE; 9]);
        assert_eq!(governor.scale(), PreviewScale::Half, "spoiled by {spoiler}");
        came(&mut governor, &[COMFORTABLE]);
        assert_eq!(governor.scale(), PreviewScale::Whole, "ten after it do");
    }
}

#[test]
fn the_band_between_late_and_comfortable_changes_nothing_however_many_frames_are_in_it() {
    let mut governor = governor();
    came(&mut governor, &[LATE, LATE]);
    came(&mut governor, &[IN_THE_BAND; 100]);
    assert_eq!(governor.scale(), PreviewScale::Half);
}

#[test]
fn what_is_late_and_what_is_comfortable_is_a_part_of_the_time_a_frame_of_the_project_lasts() {
    // Late is more than 8 tenths of a frame, comfortable less than 35 hundredths.
    let mut governor = governor();
    came(&mut governor, &[79, 79, 79, 79]);
    assert_eq!(governor.scale(), PreviewScale::Whole, "79 of 100 is not late");
    came(&mut governor, &[81, 81]);
    assert_eq!(governor.scale(), PreviewScale::Half, "81 of 100 is");
    came(&mut governor, &[36; 30]);
    assert_eq!(governor.scale(), PreviewScale::Half, "36 of 100 is not comfortable");
    came(&mut governor, &[34; 10]);
    assert_eq!(governor.scale(), PreviewScale::Whole, "34 of 100 is");
    // The same latency is late for frames that are shorter and comfortable for frames that are longer.
    let mut governor_of_fast_frames = QualityGovernor::new(QualityRules::default());
    governor_of_fast_frames.a_frame_came(took(30), Duration::from_millis(33), true);
    governor_of_fast_frames.a_frame_came(took(30), Duration::from_millis(33), true);
    assert_eq!(governor_of_fast_frames.scale(), PreviewScale::Half, "30 of 33 is late");
    let mut governor_of_slow_frames = QualityGovernor::new(QualityRules::default());
    governor_of_slow_frames.a_frame_came(took(30), Duration::from_millis(1000), true);
    governor_of_slow_frames.a_frame_came(took(30), Duration::from_millis(1000), true);
    assert_eq!(
        governor_of_slow_frames.scale(),
        PreviewScale::Whole,
        "30 of 1000 is not"
    );
}

#[test]
fn frames_that_come_while_the_clock_is_stopped_or_of_a_project_with_no_time_in_a_frame_say_nothing() {
    let mut governor = governor();
    for _ in 0..10 {
        governor.a_frame_came(took(LATE), A_FRAME, false);
        governor.a_frame_came(took(LATE), Duration::ZERO, true);
    }
    assert_eq!(governor.scale(), PreviewScale::Whole);
    // And they do not spoil the count of the ones that do: two late frames while playing, with a stopped one between.
    governor.a_frame_came(took(LATE), A_FRAME, true);
    governor.a_frame_came(took(COMFORTABLE), A_FRAME, false);
    governor.a_frame_came(took(LATE), A_FRAME, true);
    assert_eq!(governor.scale(), PreviewScale::Half);
}

// ---- when the picture is still ----

fn now() -> Instant {
    Instant::now()
}

fn a_governor_at_a_quarter() -> QualityGovernor {
    let mut governor = governor();
    came(&mut governor, &[LATE; 4]);
    assert_eq!(governor.scale(), PreviewScale::Quarter);
    governor
}

#[test]
fn a_picture_that_is_still_is_made_whole_when_it_has_been_still_for_120_milliseconds_and_not_before() {
    let start = now();
    let mut governor = a_governor_at_a_quarter();
    assert_eq!(governor.the_window_is_drawn(start, false, false), PreviewScale::Quarter);
    assert_eq!(
        governor.the_window_is_drawn(start + Duration::from_millis(119), false, false),
        PreviewScale::Quarter
    );
    assert_eq!(
        governor.the_window_is_drawn(start + Duration::from_millis(120), false, false),
        PreviewScale::Whole
    );
    assert_eq!(governor.scale(), PreviewScale::Whole);
    assert_eq!(
        governor.the_window_is_drawn(start + Duration::from_millis(500), false, false),
        PreviewScale::Whole,
        "and it stays whole"
    );
}

#[test]
fn the_window_is_told_when_to_draw_again_for_a_still_picture_to_be_made_whole() {
    let start = now();
    let mut governor = a_governor_at_a_quarter();
    assert_eq!(
        governor.how_long_until_the_picture_is_made_whole(start),
        None,
        "while the clock is going or the picture is being changed, nothing waits"
    );
    governor.the_window_is_drawn(start, false, false);
    assert_eq!(
        governor.how_long_until_the_picture_is_made_whole(start),
        Some(Duration::from_millis(120))
    );
    assert_eq!(
        governor.how_long_until_the_picture_is_made_whole(start + Duration::from_millis(50)),
        Some(Duration::from_millis(70))
    );
    assert_eq!(
        governor.how_long_until_the_picture_is_made_whole(start + Duration::from_millis(400)),
        Some(Duration::ZERO),
        "late is no time to wait"
    );
    governor.the_window_is_drawn(start + Duration::from_millis(130), false, false);
    assert_eq!(
        governor.how_long_until_the_picture_is_made_whole(start + Duration::from_millis(130)),
        None,
        "a whole picture waits for nothing"
    );
}

#[test]
fn a_picture_that_is_being_changed_keeps_the_size_it_had_however_long_it_goes_on() {
    let start = now();
    let mut governor = a_governor_at_a_quarter();
    for step in 0..100 {
        let at = start + Duration::from_millis(step * 50);
        assert_eq!(
            governor.the_window_is_drawn(at, false, true),
            PreviewScale::Quarter,
            "step {step}"
        );
    }
    assert_eq!(
        governor.how_long_until_the_picture_is_made_whole(start + Duration::from_secs(5)),
        None
    );
}

#[test]
fn when_the_changing_stops_the_120_milliseconds_are_counted_from_there() {
    let start = now();
    let mut governor = a_governor_at_a_quarter();
    governor.the_window_is_drawn(start, false, false);
    // 100 milliseconds of still picture, and then it changes: what was counted is lost.
    governor.the_window_is_drawn(start + Duration::from_millis(100), false, true);
    assert_eq!(
        governor.the_window_is_drawn(start + Duration::from_millis(200), false, false),
        PreviewScale::Quarter,
        "this is where it is still again"
    );
    assert_eq!(
        governor.the_window_is_drawn(start + Duration::from_millis(319), false, false),
        PreviewScale::Quarter
    );
    assert_eq!(
        governor.the_window_is_drawn(start + Duration::from_millis(320), false, false),
        PreviewScale::Whole
    );
}

#[test]
fn a_clock_that_goes_makes_a_picture_that_is_not_still_whatever_the_picture_is_doing() {
    let start = now();
    let mut governor = a_governor_at_a_quarter();
    governor.the_window_is_drawn(start, false, false);
    // 100 milliseconds still, then the clock goes: the picture is not made whole, and the time it was still is not counted when it stops again.
    assert_eq!(
        governor.the_window_is_drawn(start + Duration::from_millis(100), true, false),
        PreviewScale::Quarter
    );
    assert_eq!(
        governor.the_window_is_drawn(start + Duration::from_millis(1000), true, false),
        PreviewScale::Quarter,
        "however long it goes"
    );
    assert_eq!(
        governor.the_window_is_drawn(start + Duration::from_millis(1100), false, false),
        PreviewScale::Quarter
    );
    assert_eq!(
        governor.the_window_is_drawn(start + Duration::from_millis(1220), false, false),
        PreviewScale::Whole
    );
}

#[test]
fn a_picture_that_was_made_whole_starts_the_counting_of_late_frames_again() {
    let start = now();
    let mut governor = governor();
    came(&mut governor, &[LATE]);
    came(&mut governor, &[LATE]);
    assert_eq!(governor.scale(), PreviewScale::Half);
    came(&mut governor, &[LATE]);
    governor.the_window_is_drawn(start, false, false);
    governor.the_window_is_drawn(start + Duration::from_millis(200), false, false);
    assert_eq!(governor.scale(), PreviewScale::Whole);
    // One late frame was counted before it was made whole: it does not count with this one.
    came(&mut governor, &[LATE]);
    assert_eq!(governor.scale(), PreviewScale::Whole);
    came(&mut governor, &[LATE]);
    assert_eq!(governor.scale(), PreviewScale::Half);
}

#[test]
fn a_governor_that_is_always_whole_never_waits_for_anything() {
    let start = now();
    let mut governor = governor();
    for step in 0..10 {
        assert_eq!(
            governor.the_window_is_drawn(start + Duration::from_millis(step * 100), false, false),
            PreviewScale::Whole
        );
        assert_eq!(governor.how_long_until_the_picture_is_made_whole(start), None);
    }
}
