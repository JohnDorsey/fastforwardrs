//! Playing the video in the real editor, and measuring how well the preview keeps up with the clock.
//!
//! These tests depend on timing, so they live in their own file: cargo runs each test file one
//! after the other, which keeps the ffmpeg processes of other tests from competing with the
//! decoder that is being measured.
//!
//! The limits below are deliberately looser than what was measured (in a debug build on the
//! development machine, playing through a change of speed: the preview was never more than 1 frame
//! behind the clock, showed every one of the 165 pictures the clock passed, and no picture stayed
//! up for longer than 41 ms), so that a slower or busier machine does not make the tests fail for
//! no reason, while a preview that really stuttered or fell behind would.

mod journey;
mod support;

use std::time::{Duration, Instant};

use journey::Journey;
use support::{TestVideoSpecification, counter_video, test_video};

/// How long after pressing play the preview is given to get going, because the decoder has to start.
const WARM_UP: Duration = Duration::from_millis(600);

/// The most timeline frames the preview may be behind the clock once it is going (200 ms at 30 fps).
const LARGEST_LAG_IN_FRAMES: i64 = 6;

/// The longest any one picture may stay on the screen while playing.
const LONGEST_TIME_ONE_PICTURE_MAY_STAY_UP: Duration = Duration::from_millis(300);

/// What the editor showed on one drawn frame while playing.
#[derive(Clone, Copy, Debug)]
struct Sample {
    since_playing_started: Duration,
    current_timeline_frame: u64,
    /// The timeline frame whose picture the preview showed, if it showed one.
    shown_timeline_frame: Option<u64>,
}

/// Everything drawn while a video played.
struct Recording {
    samples: Vec<Sample>,
}

impl Recording {
    fn after_the_warm_up(&self) -> impl Iterator<Item = &Sample> {
        self.samples
            .iter()
            .filter(|sample| sample.since_playing_started > WARM_UP)
    }

    /// How many timeline frames each picture was behind the clock, once things were going. A frame
    /// drawn with no picture at all counts as `None`.
    fn lags(&self) -> Vec<Option<i64>> {
        self.after_the_warm_up()
            .map(|sample| {
                sample
                    .shown_timeline_frame
                    .map(|shown| sample.current_timeline_frame as i64 - shown as i64)
            })
            .collect()
    }

    /// How many different timeline frames from `first` up to (not including) `end` had their
    /// picture shown at some time.
    fn pictures_shown_between(&self, first: u64, end: u64) -> usize {
        let mut shown: Vec<u64> = self
            .samples
            .iter()
            .filter_map(|sample| sample.shown_timeline_frame)
            .filter(|frame| (first..end).contains(frame))
            .collect();
        shown.sort_unstable();
        shown.dedup();
        shown.len()
    }

    fn last_current_frame(&self) -> u64 {
        self.samples.last().expect("something was drawn").current_timeline_frame
    }

    /// The longest any one picture (or no picture) stayed on the screen, once things were going.
    fn longest_time_one_picture_stayed_up(&self) -> Duration {
        let mut longest = Duration::ZERO;
        let mut up_since = Duration::ZERO;
        let mut previous = None;
        for sample in &self.samples {
            if sample.shown_timeline_frame != previous {
                if sample.since_playing_started > WARM_UP {
                    longest = longest.max(sample.since_playing_started - up_since);
                }
                up_since = sample.since_playing_started;
                previous = sample.shown_timeline_frame;
            }
        }
        longest
    }

    fn assert_the_preview_kept_up(&self) {
        let lags = self.lags();
        assert!(lags.len() > 100, "enough frames were drawn to judge: {}", lags.len());
        let without_a_picture = lags.iter().filter(|lag| lag.is_none()).count();
        assert_eq!(without_a_picture, 0, "frames drawn with no picture at all");
        let mut lags: Vec<i64> = lags.into_iter().flatten().collect();
        lags.sort_unstable();
        let worst = *lags.last().unwrap();
        let best = lags[0];
        let ninety_ninth_percentile = lags[(lags.len() - 1) * 99 / 100];
        eprintln!(
            "preview vs clock over {} frames: 99% at most {ninety_ninth_percentile} behind, worst {worst}; \
             longest a picture stayed up: {:?}",
            lags.len(),
            self.longest_time_one_picture_stayed_up()
        );
        // The picture is for the current frame or an earlier one, never a later one: the clock is
        // read after the frame was drawn, so it can only have moved on.
        assert!(best >= 0, "a picture {} frames ahead of the clock was shown", -best);
        assert!(
            ninety_ninth_percentile <= LARGEST_LAG_IN_FRAMES / 2,
            "99% of the time the picture was at most {ninety_ninth_percentile} frames behind (worst {worst})"
        );
        assert!(
            worst <= LARGEST_LAG_IN_FRAMES,
            "the picture was {worst} frames behind the clock at worst"
        );
        let stall = self.longest_time_one_picture_stayed_up();
        assert!(
            stall <= LONGEST_TIME_ONE_PICTURE_MAY_STAY_UP,
            "one picture stayed on the screen for {stall:?}"
        );
    }
}

/// Draws frames for `how_long`, noting what is shown on each. (The tests press play just before.)
fn record_for(journey: &mut Journey, how_long: Duration) -> Recording {
    let started = Instant::now();
    let mut samples = Vec::new();
    while started.elapsed() < how_long {
        journey.harness.step();
        let application = journey.application();
        samples.push(Sample {
            since_playing_started: started.elapsed(),
            current_timeline_frame: application.current_frame().value(),
            shown_timeline_frame: application
                .preview_picture()
                .and_then(|picture| journey.timeline_frame_showing_source_frame(picture.source, picture.source_frame))
                .map(|frame| frame.value()),
        });
        std::thread::sleep(Duration::from_millis(4));
    }
    Recording { samples }
}

#[test]
fn playing_from_the_start_through_a_change_of_speed_shows_every_picture_on_time() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut journey = Journey::importing(&toolchain, &video.path);
    // The first three seconds at normal speed, the rest at 3x.
    journey.cut_at_frame(90);
    journey.set_the_speed_of_the_clip_at(140, 3);
    assert_eq!(journey.clips(), vec![(0, 90, 1), (90, 600, 3)]);

    // The playhead is where the last cut was made: back to the start for the whole journey.
    journey.press_button("Stop");
    journey.settle();
    assert_eq!(journey.application().current_frame().value(), 0);
    journey.press_button("Play");
    let recording = record_for(&mut journey, Duration::from_millis(5500));
    journey.press_button("Pause");

    recording.assert_the_preview_kept_up();
    // Every picture of the part at normal speed was shown (with a little room for a busy machine)...
    let shown_at_normal_speed = recording.pictures_shown_between(0, 90);
    assert!(
        shown_at_normal_speed >= 80,
        "{shown_at_normal_speed} of the 90 pictures at normal speed were shown"
    );
    // ...and so was almost every picture the clock passed in the sped up part.
    let passed_in_the_sped_up_part = recording.last_current_frame() - 90 + 1;
    let shown_in_the_sped_up_part = recording.pictures_shown_between(90, recording.last_current_frame() + 1);
    assert!(
        shown_in_the_sped_up_part as u64 * 10 >= passed_in_the_sped_up_part * 8,
        "{shown_in_the_sped_up_part} of the {passed_in_the_sped_up_part} pictures in the sped up part were shown"
    );
}

#[test]
fn playing_after_jumping_into_the_middle_catches_up_and_keeps_up_and_pausing_holds_the_picture() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut journey = Journey::importing(&toolchain, &video.path);
    journey.click_the_scrub_slider_at(0.5);
    journey.wait_for_the_preview_to_show_the_current_frame();
    let frame_jumped_to = journey.application().current_frame().value();
    assert!((299..=301).contains(&frame_jumped_to), "{frame_jumped_to}");

    journey.press_button("Play");
    let recording = record_for(&mut journey, Duration::from_millis(2500));
    journey.press_button("Pause");

    recording.assert_the_preview_kept_up();
    let passed = recording.last_current_frame() - frame_jumped_to;
    assert!(
        (60..=90).contains(&passed),
        "2.5 s of playing is about 75 frames, and the clock passed {passed}"
    );
    let shown = recording.pictures_shown_between(frame_jumped_to, recording.last_current_frame() + 1);
    assert!(
        shown as u64 * 10 >= passed * 8,
        "{shown} of the {passed} pictures the clock passed were shown"
    );

    // Paused: the current frame stays put, and the preview shows exactly its picture.
    assert!(!journey.application().is_playing());
    let paused_at = journey.application().current_frame();
    std::thread::sleep(Duration::from_millis(250));
    journey.wait_for_the_preview_to_show_the_current_frame();
    assert_eq!(journey.application().current_frame(), paused_at);
    assert_eq!(journey.frame_number_in_the_preview(), paused_at.value());
}

#[test]
fn playing_to_the_end_stops_on_the_last_picture_and_playing_again_starts_over() {
    let toolchain = toolchain_or_skip!();
    let short_video = test_video(&toolchain, &TestVideoSpecification::PAL_3_SECONDS);
    let mut journey = Journey::importing(&toolchain, &short_video.path);
    journey.click_the_scrub_slider_at(0.9);
    journey.wait_for_the_preview_to_show_the_current_frame();

    // About 7 frames from the end: a second and a half is more than enough to reach it.
    journey.press_button("Play");
    record_for(&mut journey, Duration::from_millis(1500));

    assert!(
        !journey.application().is_playing(),
        "playing stops at the end of the video"
    );
    assert_eq!(journey.application().current_frame().value(), 74, "on the last frame");
    journey.wait_for_the_preview_to_show_the_current_frame();
    assert_eq!(
        journey.frame_number_in_the_preview(),
        74,
        "and the preview shows the last picture"
    );

    // Playing again from the end starts from the beginning.
    journey.press_button("Play");
    record_for(&mut journey, Duration::from_millis(300));
    let frame_after_a_moment = journey.application().current_frame().value();
    assert!(journey.application().is_playing());
    assert!(
        (3..=14).contains(&frame_after_a_moment),
        "300 ms after starting over is about frame 7 at 25 frames per second: {frame_after_a_moment}"
    );
}
