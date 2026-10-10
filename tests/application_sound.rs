//! The sound of the live preview in the real editor, with the real ffmpeg: the right sound must
//! reach the speakers, at the right time.
//!
//! The speakers are stand-ins that record what they are given (nothing is played aloud). The
//! test video's sound has a different pitch for each second, so that what was heard says where in
//! the video it came from, and the recording says when it was heard.
//!
//! These tests depend on timing, so they live in their own file: cargo runs each test file one
//! after the other, which keeps the ffmpeg processes of other tests from competing with the
//! decoder.

mod journey;
mod support;

use std::time::{Duration, Instant};

use fastforwardrs::media::FfmpegToolchain;
use journey::Journey;
use journey::speakers::{self, RecordingSpeakers};
use support::{TestVideo, TestVideoSpecification, test_video};

/// A video whose sound has a different pitch for each second: 300 Hz in the first, 500 Hz in the
/// second, 700 Hz in the third and so on.
fn video_with_a_new_pitch_every_second(toolchain: &FfmpegToolchain) -> TestVideo {
    test_video(
        toolchain,
        &TestVideoSpecification {
            name: "stepped_tones_20_seconds",
            audio_expression: Some("0.3*sin(2*PI*(300+200*floor(t))*t)"),
            ..TestVideoSpecification::COUNTER
        },
    )
}

/// What the speakers played, and the moments that matter for working out when.
struct Heard {
    sound: Vec<f32>,
    /// When the recording started: the speakers were opened.
    recording_started_at: Instant,
    /// When play was pressed, with the playhead at the start: from then on the timeline time is
    /// the time that has passed.
    playing_started_at: Instant,
}

impl Heard {
    /// The 100 milliseconds of recording that are heard while the timeline is at `seconds`.
    fn around(&self, seconds: f64) -> &[f32] {
        let real_time = self.playing_started_at + Duration::from_secs_f64(seconds);
        let seconds_into_the_recording = real_time.duration_since(self.recording_started_at).as_secs_f64();
        let middle = (seconds_into_the_recording * f64::from(speakers::SAMPLE_RATE)) as usize;
        let half_window = (0.05 * f64::from(speakers::SAMPLE_RATE)) as usize;
        &self.sound[middle - half_window..middle + half_window]
    }

    fn pitch_at(&self, seconds: f64) -> f64 {
        speakers::pitch_in_hertz(self.around(seconds))
    }

    fn loudest_at(&self, seconds: f64) -> f32 {
        self.around(seconds)
            .iter()
            .fold(0.0, |loudest, sample| f32::max(loudest, sample.abs()))
    }
}

fn assert_pitch(heard: f64, expected: f64, context: &str) {
    assert!(
        (heard - expected).abs() < expected * 0.05,
        "{context}: heard {heard:.0} Hz, expected {expected:.0} Hz"
    );
}

/// Cuts the timeline at frame 90 and makes everything after it 3x, sends the sound to recording
/// speakers, plays from the start for `seconds`, and pauses.
fn play_the_edited_timeline_into(journey: &mut Journey, speakers: &RecordingSpeakers, seconds: f64) -> Heard {
    journey.application_mut().play_sound_through(speakers.opener());
    journey.cut_at_frame(90);
    journey.drag_the_speed_slider_up(140, 100.0);
    assert_eq!(journey.clips(), vec![(0, 90, 1), (90, 600, 3)]);
    journey.press_button("Stop");
    journey.settle();

    let playing_started_at = Instant::now();
    journey.press_button("Play");
    journey.keep_drawing_for(Duration::from_secs_f64(seconds));
    journey.press_button("Pause");
    // A window that is open goes on drawing frames: the next one is where the sound stops.
    journey.keep_drawing_for(Duration::from_millis(100));
    assert!(speakers.were_opened(), "playing opened the speakers");
    Heard {
        sound: speakers.heard(),
        recording_started_at: speakers.opened_at(),
        playing_started_at,
    }
}

#[test]
fn playing_sends_the_sound_of_the_timeline_to_the_speakers_in_step_with_the_clock() {
    let toolchain = toolchain_or_skip!();
    let video = video_with_a_new_pitch_every_second(&toolchain);
    let mut journey = Journey::importing(&toolchain, &video.path);
    let speakers = RecordingSpeakers::new();

    let heard = play_the_edited_timeline_into(&mut journey, &speakers, 4.4);

    assert_eq!(journey.application().sound_problem(), None);
    // The first three seconds are the video's sound at normal speed: a new pitch each second.
    for (seconds, expected) in [(0.55, 300.0), (1.5, 500.0), (2.5, 700.0)] {
        assert_pitch(
            heard.pitch_at(seconds),
            expected,
            &format!("at {seconds} s of the timeline"),
        );
    }
    // After that the clip is sped up 3x: three seconds of the video go by in each second, so the
    // pitch changes every third of a second. (Speeding sound up this way does not make it higher:
    // it plays the same tones, faster.) At 3.17 s the video's sound is at 3.5 s (900 Hz); at 3.5 s
    // it is at 4.5 s (1100 Hz); at 3.83 s at 5.5 s (1300 Hz).
    for (seconds, expected) in [(3.17, 900.0), (3.5, 1100.0), (3.83, 1300.0)] {
        assert_pitch(
            heard.pitch_at(seconds),
            expected,
            &format!("at {seconds} s of the timeline, in the sped up clip"),
        );
    }
}

#[test]
fn the_sound_begins_soon_after_play_is_pressed() {
    let toolchain = toolchain_or_skip!();
    let video = video_with_a_new_pitch_every_second(&toolchain);
    let mut journey = Journey::importing(&toolchain, &video.path);
    let speakers = RecordingSpeakers::new();

    let heard = play_the_edited_timeline_into(&mut journey, &speakers, 2.0);

    let first_sound = heard
        .sound
        .iter()
        .position(|sample| sample.abs() > 0.01)
        .expect("something was heard");
    let seconds_until_it_was_heard = first_sound as f64 / f64::from(speakers::SAMPLE_RATE)
        - heard
            .playing_started_at
            .duration_since(heard.recording_started_at)
            .as_secs_f64();
    println!("the sound began {seconds_until_it_was_heard:.3} s after play was pressed");
    assert!(
        seconds_until_it_was_heard < 1.0,
        "the sound began {seconds_until_it_was_heard} s after play was pressed"
    );
}

#[test]
fn pausing_silences_the_speakers_straight_away() {
    let toolchain = toolchain_or_skip!();
    let video = video_with_a_new_pitch_every_second(&toolchain);
    let mut journey = Journey::importing(&toolchain, &video.path);
    let speakers = RecordingSpeakers::new();
    play_the_edited_timeline_into(&mut journey, &speakers, 2.0);

    // Paused: half a second later, nothing at all is coming out.
    std::thread::sleep(Duration::from_millis(500));
    let sound = speakers.heard();
    let last_quarter_second = &sound[sound.len() - speakers::SAMPLE_RATE as usize / 4..];
    assert!(
        last_quarter_second.iter().all(|sample| *sample == 0.0),
        "the speakers went on playing after the pause"
    );
    assert!(
        !journey.application().sound_is_running(),
        "and nothing is decoding the sound any more"
    );
    assert!(
        speakers.times_cleared() >= 1,
        "what was waiting at the speakers was thrown away"
    );
}

#[test]
fn with_sped_up_clips_muted_the_sound_stops_for_the_sped_up_clip_and_goes_on_for_the_rest() {
    let toolchain = toolchain_or_skip!();
    let video = video_with_a_new_pitch_every_second(&toolchain);
    let mut journey = Journey::importing(&toolchain, &video.path);
    journey.change_global_setting("Mute sped-up clips");
    assert!(journey.application().settings_in_effect().mute_sped_up_clips);
    let speakers = RecordingSpeakers::new();

    let heard = play_the_edited_timeline_into(&mut journey, &speakers, 4.4);

    assert_pitch(heard.pitch_at(1.5), 500.0, "the clip at normal speed keeps its sound");
    for seconds in [3.2, 3.5, 3.8] {
        let loudest = heard.loudest_at(seconds);
        assert!(
            loudest < 0.001,
            "the sped up clip is silent at {seconds} s, but the loudest sample is {loudest}"
        );
    }
}

#[test]
fn moving_the_playhead_while_playing_makes_the_sound_jump_with_it() {
    let toolchain = toolchain_or_skip!();
    let video = video_with_a_new_pitch_every_second(&toolchain);
    let mut journey = Journey::importing(&toolchain, &video.path);
    let speakers = RecordingSpeakers::new();
    journey.application_mut().play_sound_through(speakers.opener());
    journey.press_button("Play");
    journey.keep_drawing_for(Duration::from_millis(1500));

    // Jump a long way on, with the slider along the bottom: to 10 seconds in.
    journey.click_the_scrub_slider_at(0.5);
    let jumped_at = Instant::now();
    journey.keep_drawing_for(Duration::from_millis(1500));
    journey.press_button("Pause");

    let heard = Heard {
        sound: speakers.heard(),
        recording_started_at: speakers.opened_at(),
        playing_started_at: jumped_at - Duration::from_secs(10), // timeline time is 10 s at the jump
    };
    // A second after the jump the sound is at 11 s: the twelfth second of the video, 2500 Hz.
    // (The jump itself and the sound starting over take a little while, so the very next second
    // is allowed as well.)
    let pitch = heard.pitch_at(11.0);
    assert!(
        (pitch - 2500.0).abs() < 125.0 || (pitch - 2300.0).abs() < 115.0,
        "a second after jumping to 10 s the sound should be near 2300 to 2500 Hz, but it is {pitch:.0}"
    );
}

#[test]
fn the_whole_path_to_the_real_speakers_works_with_a_silent_video() {
    let toolchain = toolchain_or_skip!();
    // No sound output on this computer: nothing to try. (Checked by opening the speakers once.)
    if let Err(error) = fastforwardrs::playback::DeviceAudioSink::open() {
        eprintln!("SKIPPED: no sound output to try ({error})");
        return;
    }
    // A video whose sound is silence, so that trying this makes no noise.
    let video = test_video(
        &toolchain,
        &TestVideoSpecification {
            name: "silent_sound_20_seconds",
            audio_expression: Some("0"),
            ..TestVideoSpecification::COUNTER
        },
    );
    let mut journey = Journey::importing(&toolchain, &video.path);
    journey
        .application_mut()
        .play_sound_through(fastforwardrs::playback::opener_of_the_default_speakers());

    journey.press_button("Play");
    journey.keep_drawing_for(Duration::from_millis(1500));

    assert_eq!(journey.application().sound_problem(), None);
    assert!(
        journey.application().sound_is_running(),
        "the sound is being decoded and played"
    );
    journey.press_button("Pause");
    journey.keep_drawing_for(Duration::from_millis(100));
    assert!(!journey.application().sound_is_running());
}
