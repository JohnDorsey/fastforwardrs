// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

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
use fastforwardrs::project::SpedUpClipSound;
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
    journey.set_the_speed_of_the_clip_at(140, 3);
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
fn with_the_pitch_raised_the_sped_up_clip_is_heard_three_times_higher_and_the_rest_is_as_it_was() {
    let toolchain = toolchain_or_skip!();
    let video = video_with_a_new_pitch_every_second(&toolchain);
    let mut journey = Journey::importing(&toolchain, &video.path);
    journey.choose_in_a_global_setting_dropdown("Maintain pitch", "Increase pitch");
    assert_eq!(
        journey.application().settings_in_effect().sped_up_clip_sound,
        SpedUpClipSound::IncreasePitch
    );
    let speakers = RecordingSpeakers::new();

    let heard = play_the_edited_timeline_into(&mut journey, &speakers, 4.4);

    assert_eq!(journey.application().sound_problem(), None);
    assert_pitch(heard.pitch_at(1.5), 500.0, "the clip at normal speed keeps its sound");
    // Without raising the pitch these would be 900, 1100 and 1300 Hz (see the first test).
    for (seconds, expected) in [(3.17, 2700.0), (3.5, 3300.0), (3.83, 3900.0)] {
        assert_pitch(
            heard.pitch_at(seconds),
            expected,
            &format!("at {seconds} s of the timeline, in the sped up clip"),
        );
    }
}

#[test]
fn with_sped_up_clips_muted_the_sound_stops_for_the_sped_up_clip_and_goes_on_for_the_rest() {
    let toolchain = toolchain_or_skip!();
    let video = video_with_a_new_pitch_every_second(&toolchain);
    let mut journey = Journey::importing(&toolchain, &video.path);
    journey.choose_in_a_global_setting_dropdown("Maintain pitch", "Mute");
    assert_eq!(
        journey.application().settings_in_effect().sped_up_clip_sound,
        SpedUpClipSound::Mute
    );
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

/// A video whose sound is a steady 440 Hz tone, a tenth as loud as it can be.
fn video_with_a_steady_tone(toolchain: &FfmpegToolchain) -> TestVideo {
    test_video(
        toolchain,
        &TestVideoSpecification {
            name: "steady_tone_20_seconds",
            audio_expression: Some("0.1*sin(2*PI*440*t)"),
            ..TestVideoSpecification::COUNTER
        },
    )
}

/// The loudest sample of `sound` (one entry for every 1/48000 of a second) between two times.
fn loudest_between(sound: &[f32], from_seconds: f64, to_seconds: f64) -> f32 {
    let moment = |seconds: f64| (seconds * f64::from(speakers::SAMPLE_RATE)) as usize;
    sound[moment(from_seconds)..moment(to_seconds)]
        .iter()
        .fold(0.0, |loudest, sample| f32::max(loudest, sample.abs()))
}

/// The longest time, in seconds, between two times in which the speakers made no sound at all. (A
/// steady tone passes through zero twice in every cycle, but never stays there.)
fn longest_silence_between(sound: &[f32], from_seconds: f64, to_seconds: f64) -> f64 {
    let moment = |seconds: f64| (seconds * f64::from(speakers::SAMPLE_RATE)) as usize;
    let (mut longest, mut current) = (0, 0);
    for sample in &sound[moment(from_seconds)..moment(to_seconds)] {
        current = if sample.abs() < 0.0005 { current + 1 } else { 0 };
        longest = longest.max(current);
    }
    longest as f64 / f64::from(speakers::SAMPLE_RATE)
}

#[test]
fn changing_a_volume_while_playing_makes_the_sound_louder_and_does_not_interrupt_it() {
    let toolchain = toolchain_or_skip!();
    let video = video_with_a_steady_tone(&toolchain);
    let mut journey = Journey::importing(&toolchain, &video.path);
    let speakers = RecordingSpeakers::new();
    journey.application_mut().play_sound_through(speakers.opener());
    journey.press_button("Play");
    journey.keep_drawing_for(Duration::from_millis(1500));

    // The volume of the clip is turned up by 12 dB (nearly four times as loud) in the properties panel, as it
    // plays; the sound is made again for that, and takes over from the sound that is playing.
    journey.click(journey.strip_of_track(0).center());
    let edited_at = Instant::now();
    journey.set_slider("Gain", 12.0);
    journey.keep_drawing_for(Duration::from_millis(1500));
    let paused_at = Instant::now();
    journey.press_button("Pause");
    journey.keep_drawing_for(Duration::from_millis(100));

    assert_eq!(journey.application().sound_problem(), None);
    let sound = speakers.heard();
    let opened_at = speakers.opened_at();
    let seconds = |instant: Instant| instant.duration_since(opened_at).as_secs_f64();
    let first_sound = sound
        .iter()
        .position(|sample| sample.abs() > 0.01)
        .expect("something was heard") as f64
        / f64::from(speakers::SAMPLE_RATE);

    let silence = longest_silence_between(&sound, first_sound, seconds(paused_at) - 0.05);
    assert!(
        silence < 0.02,
        "the sound stopped for {silence:.3} s somewhere between the start and the pause"
    );
    assert!(speakers.times_spliced() >= 1, "the new sound took over from the old");
    assert_eq!(
        speakers.times_cleared(),
        1,
        "the speakers were only cleared once, by the pause: not by the edit"
    );

    let before = loudest_between(&sound, seconds(edited_at) - 0.5, seconds(edited_at) - 0.1);
    let after = loudest_between(&sound, seconds(paused_at) - 0.5, seconds(paused_at) - 0.1);
    let ratio = f64::from(after) / f64::from(before);
    assert!(
        (ratio - 3.98).abs() < 0.5,
        "12 dB is nearly four times as loud: it went from {before} to {after}, which is {ratio} times"
    );
}

#[test]
fn dragging_a_volume_slider_while_playing_changes_the_sound_as_it_goes_and_never_interrupts_it() {
    let toolchain = toolchain_or_skip!();
    let video = video_with_a_steady_tone(&toolchain);
    let mut journey = Journey::importing(&toolchain, &video.path);
    let speakers = RecordingSpeakers::new();
    journey.application_mut().play_sound_through(speakers.opener());
    journey.press_button("Play");
    journey.keep_drawing_for(Duration::from_millis(1000));
    journey.right_click(journey.strip_of_track(0).center());

    // The slider is moved a little in every frame for two seconds, from 0 dB up to 12 dB.
    let dragged_from = Instant::now();
    for step in 0..60 {
        journey.set_slider("Gain", 12.0 * f64::from(step) / 59.0);
        journey.keep_drawing_for(Duration::from_millis(30));
    }
    let dragged_until = Instant::now();
    journey.keep_drawing_for(Duration::from_millis(600));
    let paused_at = Instant::now();
    journey.press_button("Pause");
    journey.keep_drawing_for(Duration::from_millis(100));

    assert_eq!(journey.application().sound_problem(), None);
    let sound = speakers.heard();
    let opened_at = speakers.opened_at();
    let seconds = |instant: Instant| instant.duration_since(opened_at).as_secs_f64();
    let first_sound = sound
        .iter()
        .position(|sample| sample.abs() > 0.01)
        .expect("something was heard") as f64
        / f64::from(speakers::SAMPLE_RATE);
    let silence = longest_silence_between(&sound, first_sound, seconds(paused_at) - 0.05);
    assert!(
        silence < 0.02,
        "the sound stopped for {silence:.3} s while the slider was dragged"
    );
    assert!(
        speakers.times_spliced() >= 4,
        "the sound was made again, and took over, several times as the slider moved: {} times",
        speakers.times_spliced()
    );

    // The sound follows the slider: louder at the end of the drag than at the start of it.
    let at_the_start = loudest_between(&sound, seconds(dragged_from) - 0.4, seconds(dragged_from) - 0.05);
    let at_the_end = loudest_between(&sound, seconds(paused_at) - 0.45, seconds(paused_at) - 0.1);
    assert!(
        f64::from(at_the_end) > 3.0 * f64::from(at_the_start),
        "nearly four times as loud at the end: {at_the_start} then {at_the_end}"
    );
    let _ = dragged_until;
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
