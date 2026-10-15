// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the decisions of [`AudioSupplier`], with a fake decoder and fake speakers.
//!
//! The fake decoder's sound is its own position: moment `k` of a stream is the number `k` (and on
//! the right channel, if there is one, `-k`). So whatever reaches the speakers shows exactly which
//! moments were played, skipped or held back.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;

use super::*;
use crate::media::export::{LoudnessToExport, SoundClipToExport, SoundItemToExport};
use crate::playback::splice_waiting_sound;
use crate::project::SourceMediaProperties;
use crate::project::{
    AnimationTarget, FrameRate, Gain, MediaClip, PropertyId, SourceFrameIndex, SourceMedia, SourceMediaLink,
    SourceVideoProperties, SpeedFactor, SpeedIndicatorLayout, Track, TrackId,
};

const SAMPLE_RATE: u32 = 48_000;
const A_FRAME_OF_THE_WINDOW: Duration = Duration::from_micros(16_667);

// --- the fake speakers ---

#[derive(Default)]
struct SpeakerState {
    /// What is waiting to be played, one entry for each sample.
    waiting: VecDeque<f32>,
    /// Everything that has been played, one entry for each sample.
    played: Vec<f32>,
    times_cleared: usize,
    /// How many moments the speakers were asked to play when nothing was waiting: a gap in the sound.
    underrun_moments: u64,
    /// Every time new sound was joined onto what was waiting: how much was kept, how long the
    /// fade was, and how many moments of new sound there were.
    splices: Vec<(u64, u64, usize)>,
    /// Set to make the speakers report that they no longer work.
    broken: bool,
}

struct FakeSpeakers {
    state: Rc<RefCell<SpeakerState>>,
    format: AudioFormat,
}

impl AudioSink for FakeSpeakers {
    fn format(&self) -> AudioFormat {
        self.format
    }

    fn queue(&mut self, samples: &[f32]) {
        self.state.borrow_mut().waiting.extend(samples);
    }

    fn clear(&mut self) {
        let mut state = self.state.borrow_mut();
        state.waiting.clear();
        state.times_cleared += 1;
    }

    fn queued_moment_count(&self) -> u64 {
        (self.state.borrow().waiting.len() / usize::from(self.format.channel_count)) as u64
    }

    fn replace_waiting_sound(&mut self, keep_moments: u64, fade_moments: u64, new_sound: &[f32]) {
        let channel_count = usize::from(self.format.channel_count);
        let mut state = self.state.borrow_mut();
        state
            .splices
            .push((keep_moments, fade_moments, new_sound.len() / channel_count));
        splice_waiting_sound(&mut state.waiting, channel_count, keep_moments, fade_moments, new_sound);
    }

    fn is_working(&self) -> bool {
        !self.state.borrow().broken
    }
}

// --- the fake decoder ---

struct StreamState {
    /// How many moments have been decoded so far (the test raises this to let the sound arrive).
    decoded_moments: Cell<u64>,
    delivered_moments: Cell<u64>,
    /// Set to end the stream once everything decoded has been delivered.
    finishes_with: RefCell<Option<Option<String>>>,
    was_stopped: Cell<bool>,
    channel_count: usize,
}

struct FakeStream {
    state: Rc<StreamState>,
}

impl AudioSource for FakeStream {
    fn try_next_event(&self) -> Option<AudioStreamEvent> {
        let state = &self.state;
        let first = state.delivered_moments.get();
        let available = state.decoded_moments.get() - first;
        if available > 0 {
            let moments = available.min(2048);
            state.delivered_moments.set(first + moments);
            let samples = (first..first + moments)
                .flat_map(|moment| {
                    (0..state.channel_count)
                        .map(move |channel| if channel == 0 { moment as f32 } else { -(moment as f32) })
                })
                .collect();
            return Some(AudioStreamEvent::Samples(samples));
        }
        state
            .finishes_with
            .borrow_mut()
            .take()
            .map(|error_output| AudioStreamEvent::Finished { error_output })
    }
}

impl Drop for FakeStream {
    fn drop(&mut self) {
        self.state.was_stopped.set(true);
    }
}

#[derive(Clone)]
struct FakeStarter {
    requests: Rc<RefCell<Vec<AudioStreamRequest>>>,
    streams: Rc<RefCell<Vec<Rc<StreamState>>>>,
    /// How many moments each new stream has decoded as soon as it starts.
    decoded_at_the_start: Rc<Cell<u64>>,
    attempts: Rc<Cell<usize>>,
    fails: Rc<Cell<bool>>,
}

impl AudioStreamStarter for FakeStarter {
    type Stream = FakeStream;

    fn start(&self, request: &AudioStreamRequest) -> Result<FakeStream, AudioStreamError> {
        self.attempts.set(self.attempts.get() + 1);
        if self.fails.get() {
            return Err(AudioStreamError::CouldNotStart(std::io::Error::other(
                "no ffmpeg in a test",
            )));
        }
        self.requests.borrow_mut().push(request.clone());
        let state = Rc::new(StreamState {
            decoded_moments: Cell::new(self.decoded_at_the_start.get()),
            delivered_moments: Cell::new(0),
            finishes_with: RefCell::new(None),
            was_stopped: Cell::new(false),
            channel_count: usize::from(request.format.channel_count),
        });
        self.streams.borrow_mut().push(Rc::clone(&state));
        Ok(FakeStream { state })
    }
}

// --- the test bench ---

struct Bench {
    supplier: AudioSupplier<FakeStarter>,
    starter: FakeStarter,
    speakers: Rc<RefCell<SpeakerState>>,
    project: Project,
    source_locations: HashMap<SourceMediaIdentifier, MediaLocation>,
    now: Instant,
    sped_up_clip_sound: SpedUpClipSound,
    channel_count: u16,
}

const VIDEO: SourceMediaIdentifier = SourceMediaIdentifier::new(1);

fn frame_rate() -> FrameRate {
    FrameRate::new(30, 1).unwrap()
}

/// A project with a video of 600 frames (20 seconds) with sound, and one track holding clips of
/// it one after another: each is the source frames `first` up to `end` at `speed`.
fn project_of_clips(clips: &[(u64, u64, u32)]) -> Project {
    let video = SourceMedia::video(
        VIDEO,
        SourceMediaLink::from_absolute_path(PathBuf::from("holiday.mp4")),
        SourceVideoProperties {
            frame_rate: frame_rate(),
            total_frame_count: 600,
            display_width_in_pixels: 320,
            display_height_in_pixels: 180,
            has_audio: true,
        },
    );
    let clips = clips.iter().map(|&(first, end, speed)| {
        let media = MediaClip::new(
            VIDEO,
            SourceFrameIndex::new(first),
            SourceFrameIndex::new(end),
            SpeedFactor::new(speed).unwrap(),
        )
        .unwrap();
        (0, Clip::Video(media))
    });
    let track = Track::new(TrackId::new(0), "track 0").with_clips(clips);
    Project::from_parts(vec![video], vec![track], SpeedIndicatorLayout::DEFAULT).unwrap()
}

/// The same project as `project_of_clips(&[(0, 600, 1)])` but with its clip `decibels` louder, which
/// is a change to what is heard that leaves the sound as long as it was.
fn project_with_a_clip_that_is_louder_by(decibels: f32) -> Project {
    let mut project = project_of_clips(&[(0, 600, 1)]);
    let location = project.tracks()[0].placement_at_index(0).unwrap().id;
    project.set_clip_gain(location, Gain::from_decibels(decibels)).unwrap();
    project
}

/// The first clip of the first track of the sound a request asks for.
fn first_sound_clip(request: &AudioStreamRequest) -> SoundClipToExport {
    match &request.sound.strips[0].items[0] {
        SoundItemToExport::Clip(clip) => clip.clone(),
        SoundItemToExport::Silence { .. } => panic!("the sound starts with silence"),
    }
}

impl Bench {
    /// 20 seconds of video at 30 frames per second, as one clip, with mono sound at 48000 a second.
    fn new() -> Self {
        Self::with(project_of_clips(&[(0, 600, 1)]), 1)
    }

    fn with(project: Project, channel_count: u16) -> Self {
        let starter = FakeStarter {
            requests: Rc::default(),
            streams: Rc::default(),
            decoded_at_the_start: Rc::new(Cell::new(u64::MAX / 4)),
            attempts: Rc::default(),
            fails: Rc::default(),
        };
        let speakers = Rc::new(RefCell::new(SpeakerState::default()));
        let mut supplier = AudioSupplier::new(starter.clone());
        {
            let speakers = Rc::clone(&speakers);
            supplier.play_through(Box::new(move || {
                Ok(Box::new(FakeSpeakers {
                    state: Rc::clone(&speakers),
                    format: AudioFormat {
                        sample_rate: SAMPLE_RATE,
                        channel_count,
                    },
                }) as Box<dyn AudioSink>)
            }));
        }
        Self {
            supplier,
            starter,
            speakers,
            project,
            source_locations: HashMap::from([(VIDEO, MediaLocation::from("holiday.mp4"))]),
            now: Instant::now(),
            sped_up_clip_sound: SpedUpClipSound::MaintainPitch,
            channel_count,
        }
    }

    fn clock_at(seconds: f64, is_playing: bool) -> ClockReading {
        ClockReading {
            frame: TimelineFrameIndex::new(frame_rate().frame_at_seconds(seconds)),
            seconds,
            is_playing,
        }
    }

    /// One frame of the window with the clock at `seconds`, playing.
    fn update_playing_at(&mut self, seconds: f64) {
        self.update_at(seconds, true);
    }

    fn update_paused_at(&mut self, seconds: f64) {
        self.update_at(seconds, false);
    }

    fn update_at(&mut self, seconds: f64, is_playing: bool) {
        let content = AudioContent {
            project: &self.project,
            source_locations: &self.source_locations,
            sped_up_clip_sound: self.sped_up_clip_sound,
        };
        self.supplier
            .update(Some(content), Self::clock_at(seconds, is_playing), self.now);
    }

    fn update_without_sound_to_play(&mut self, seconds: f64) {
        self.supplier.update(None, Self::clock_at(seconds, true), self.now);
    }

    /// Lets time pass, with the speakers playing what is waiting for them.
    fn let_time_pass(&mut self, duration: Duration) {
        self.now += duration;
        let moments = (duration.as_secs_f64() * f64::from(SAMPLE_RATE)) as usize;
        self.play(moments);
    }

    fn play(&self, moments: usize) {
        let channel_count = usize::from(self.channel_count);
        let mut state = self.speakers.borrow_mut();
        let moments_waiting = state.waiting.len() / channel_count;
        state.underrun_moments += moments.saturating_sub(moments_waiting) as u64;
        for _ in 0..(moments * channel_count).min(state.waiting.len()) {
            let sample = state.waiting.pop_front().unwrap();
            state.played.push(sample);
        }
    }

    /// Plays on for `seconds` of the window, steadily, with the clock keeping the right time from
    /// `from_seconds`. Returns the time the clock ends at.
    fn play_steadily(&mut self, from_seconds: f64, seconds: f64) -> f64 {
        let mut clock_seconds = from_seconds;
        let frames = (seconds * 60.0).round() as u64;
        for _ in 0..frames {
            self.let_time_pass(A_FRAME_OF_THE_WINDOW);
            clock_seconds += A_FRAME_OF_THE_WINDOW.as_secs_f64();
            self.update_playing_at(clock_seconds);
        }
        clock_seconds
    }

    fn streams_started(&self) -> usize {
        self.starter.streams.borrow().len()
    }

    fn stream(&self, number: usize) -> Rc<StreamState> {
        Rc::clone(&self.starter.streams.borrow()[number])
    }

    fn waiting_moments(&self) -> usize {
        self.speakers.borrow().waiting.len() / usize::from(self.channel_count)
    }

    /// The sound played so far, left channel (or the only one).
    fn played(&self) -> Vec<f32> {
        let channel_count = usize::from(self.channel_count);
        self.speakers
            .borrow()
            .played
            .iter()
            .step_by(channel_count)
            .copied()
            .collect()
    }
}

/// The sound played, with the silence in front taken away: what is left should be consecutive
/// moments of the stream, with none skipped and none repeated, where the sound is steady.
fn assert_consecutive(played: &[f32], context: &str) {
    for pair in played.windows(2) {
        assert_eq!(
            pair[1],
            pair[0] + 1.0,
            "{context}: {} was followed by {}",
            pair[0],
            pair[1]
        );
    }
}

// --- when there is sound ---

#[test]
fn nothing_runs_while_the_video_is_paused() {
    let mut bench = Bench::new();
    bench.update_paused_at(3.0);
    bench.update_paused_at(3.5);
    assert_eq!(bench.starter.attempts.get(), 0);
    assert!(!bench.supplier.is_running());
    assert_eq!(bench.waiting_moments(), 0);
}

#[test]
fn nothing_runs_when_there_is_no_sound_to_play() {
    let mut bench = Bench::new();
    bench.update_without_sound_to_play(3.0);
    assert_eq!(bench.starter.attempts.get(), 0);
    assert_eq!(bench.supplier.last_problem(), None, "that is not a problem");
}

#[test]
fn playing_starts_a_stream_from_the_playhead_in_the_speakers_format() {
    let mut bench = Bench::with(project_of_clips(&[(0, 300, 1), (300, 600, 3)]), 2);
    bench.sped_up_clip_sound = SpedUpClipSound::Mute;

    // Timeline frame 320 is 20 frames into the second clip, which shows source frames 300, 303, ...
    // (The clips make 300 frames and then 100, so there are 80 frames from there to the end.)
    bench.update_playing_at((320.0 + 0.1) / 30.0);

    assert_eq!(bench.streams_started(), 1);
    let request = bench.starter.requests.borrow()[0].clone();
    assert_eq!(request.source_files, vec![MediaLocation::from("holiday.mp4")]);
    assert_eq!(request.sound.frame_rate, frame_rate());
    assert_eq!(request.sound.total_frame_count, 80);
    assert_eq!(
        request.sound.strips[0].items,
        vec![SoundItemToExport::Clip(SoundClipToExport {
            source_input: 0,
            first_source_frame: 300 + 20 * 3,
            end_source_frame: 600,
            speed_factor: SpeedFactor::new(3).unwrap(),
            timeline_frame_count: 80,
            loudness: LoudnessToExport::UNCHANGED,
        })]
    );
    assert_eq!(request.sound.sped_up_clip_sound, SpedUpClipSound::Mute);
    assert_eq!(
        request.format,
        AudioFormat {
            sample_rate: SAMPLE_RATE,
            channel_count: 2
        }
    );
}

/// A behaviour that changed when there came to be several tracks: a clip that reaches past the end
/// of a source file that is shorter than it was edited on is not cut short (which would make the
/// timeline shorter and put the other tracks out of step). It keeps its place, and what the file
/// does not have is silence.
#[test]
fn a_clip_that_is_longer_than_its_source_file_keeps_its_whole_place_on_the_timeline() {
    let mut bench = Bench::new();
    let video = *bench.project.sources()[0].properties.video().unwrap();
    let shorter = SourceVideoProperties {
        total_frame_count: 450,
        ..video
    };
    bench
        .project
        .relink_source(
            VIDEO,
            SourceMediaLink::from_absolute_path(PathBuf::from("holiday.mp4")),
            SourceMediaProperties::Video(shorter),
        )
        .unwrap();
    bench.update_playing_at(0.0);
    let request = bench.starter.requests.borrow()[0].clone();
    let clip = first_sound_clip(&request);
    assert_eq!(
        (
            clip.first_source_frame,
            clip.end_source_frame,
            clip.timeline_frame_count
        ),
        (0, 600, 600)
    );
}

#[test]
fn at_the_very_end_there_is_nothing_left_to_play_and_nothing_is_started() {
    let mut bench = Bench::new();
    bench.update_playing_at(600.0 / 30.0);
    assert_eq!(bench.starter.attempts.get(), 0);
}

#[test]
fn the_speakers_are_only_fed_until_a_little_is_waiting_so_the_decoder_does_not_run_away() {
    let mut bench = Bench::new();
    bench.update_playing_at(0.0);
    bench.update_playing_at(0.0);
    let waiting = bench.waiting_moments() as f64 / f64::from(SAMPLE_RATE);
    assert!(
        (SECONDS_OF_SOUND_KEPT_WAITING..SECONDS_OF_SOUND_KEPT_WAITING + 0.05).contains(&waiting),
        "{waiting} s waiting"
    );
    assert!(
        bench.stream(0).delivered_moments.get() < 20_000,
        "most of the sound is still in the decoder"
    );
}

// --- staying in step ---

#[test]
fn sound_that_arrives_late_has_the_part_that_is_already_over_skipped() {
    let mut bench = Bench::new();
    bench.starter.decoded_at_the_start.set(0); // decoding takes a while to produce anything
    let start = 3.0;
    bench.update_playing_at(start);
    let mut clock = start;
    for _ in 0..8 {
        bench.let_time_pass(A_FRAME_OF_THE_WINDOW);
        clock += A_FRAME_OF_THE_WINDOW.as_secs_f64();
        bench.update_playing_at(clock);
    }
    assert_eq!(bench.waiting_moments(), 0, "nothing has arrived yet");

    // The sound arrives 0.133 seconds after the playhead was where the stream starts.
    bench.stream(0).decoded_moments.set(u64::MAX / 4);
    bench.update_playing_at(clock);
    bench.play_steadily(clock, 1.0);

    let played = bench.played();
    let expected_first_moment = (clock - start) * f64::from(SAMPLE_RATE);
    assert!(
        (played[0] as f64 - expected_first_moment).abs() < 1200.0,
        "the first moment heard is {} but it should be near {expected_first_moment}",
        played[0]
    );
    assert_consecutive(&played, "after the start");
}

#[test]
fn steady_playing_goes_on_with_no_gaps_no_skips_and_no_new_stream() {
    let mut bench = Bench::new();
    bench.update_playing_at(3.0);
    bench.play_steadily(3.0, 3.0);

    let played = bench.played();
    assert!(played.len() > 100_000, "three seconds were heard: {}", played.len());
    assert_consecutive(&played, "steady");
    assert_eq!(bench.streams_started(), 1);
    assert_eq!(bench.supplier.last_problem(), None);
}

#[test]
fn the_playhead_stepping_back_one_frame_holds_the_sound_back_a_little_without_a_new_stream() {
    let mut bench = Bench::new();
    bench.update_playing_at(3.0);
    let clock = bench.play_steadily(3.0, 1.0);

    // The left arrow key: the clock goes back one frame (a thirtieth of a second).
    let before = bench.played().len();
    bench.update_playing_at(clock - 1.0 / 30.0);
    let clock = bench.play_steadily(clock - 1.0 / 30.0, 1.0);
    let _ = clock;

    assert_eq!(bench.streams_started(), 1, "no new stream for a small step");
    let after_the_step = &bench.played()[before..];
    let silent_moments = after_the_step.iter().filter(|sample| **sample == 0.0).count();
    assert!(
        (1400..1800).contains(&silent_moments),
        "about a thirtieth of a second of silence holds the sound back: {silent_moments} moments"
    );
}

#[test]
fn the_playhead_stepping_on_one_frame_skips_a_little_sound_without_a_new_stream() {
    let mut bench = Bench::new();
    bench.update_playing_at(3.0);
    let clock = bench.play_steadily(3.0, 1.0);

    let before = bench.played().len();
    bench.update_playing_at(clock + 1.0 / 30.0);
    bench.play_steadily(clock + 1.0 / 30.0, 1.0);

    assert_eq!(bench.streams_started(), 1);
    let after_the_step = bench.played()[before..].to_vec();
    let jumps: Vec<f32> = after_the_step
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .filter(|difference| *difference != 1.0)
        .collect();
    assert_eq!(jumps.len(), 1, "one jump forward: {jumps:?}");
    assert!(
        (1400.0..1800.0).contains(&jumps[0]),
        "by about a thirtieth of a second: {}",
        jumps[0]
    );
}

#[test]
fn moving_the_playhead_a_long_way_while_playing_starts_the_sound_again_from_there() {
    let mut bench = Bench::new();
    bench.update_playing_at(3.0);
    let clock = bench.play_steadily(3.0, 1.0);
    assert_eq!(bench.streams_started(), 1);
    let times_cleared_before = bench.speakers.borrow().times_cleared;

    // A seek ten seconds on.
    bench.let_time_pass(A_FRAME_OF_THE_WINDOW);
    bench.update_playing_at(clock + 10.0);
    bench.update_playing_at(clock + 10.0);

    assert!(bench.stream(0).was_stopped.get(), "the old stream was stopped");
    assert!(
        bench.speakers.borrow().times_cleared > times_cleared_before,
        "and what was waiting thrown away"
    );
    assert_eq!(bench.streams_started(), 2);
    let request = bench.starter.requests.borrow()[1].clone();
    let expected_first_frame = frame_rate().frame_at_seconds(clock + 10.0);
    assert_eq!(first_sound_clip(&request).first_source_frame, expected_first_frame);
}

#[test]
fn a_change_to_what_the_sound_is_made_of_starts_it_again_even_though_the_clock_is_steady() {
    let mut bench = Bench::new();
    bench.update_playing_at(3.0);
    let clock = bench.play_steadily(3.0, 1.0);
    assert_eq!(bench.streams_started(), 1);

    // A clip is sped up (the playhead is in the first clip, which is now shorter and faster).
    bench.project = project_of_clips(&[(0, 300, 2), (300, 600, 1)]);
    bench.let_time_pass(A_FRAME_OF_THE_WINDOW);
    bench.update_playing_at(clock);
    bench.let_time_pass(Duration::from_millis(150));
    bench.update_playing_at(clock);

    assert!(bench.stream(0).was_stopped.get());
    assert_eq!(bench.streams_started(), 2);
}

/// `project` with a keyframe for `property` of `target` at frame 100.
fn with_a_keyframe_at_frame_100(mut project: Project, target: AnimationTarget, property: PropertyId) -> Project {
    project
        .add_keyframe(target, property, TimelineFrameIndex::new(100))
        .unwrap();
    project
}

/// The same project as `project_of_clips(&[(0, 600, 1)])` but with a keyframe for `property` of its clip at frame 100.
fn project_with_a_keyframe_for(property: PropertyId) -> Project {
    let project = project_of_clips(&[(0, 600, 1)]);
    let clip = project.tracks()[0].placement_at_index(0).unwrap().id;
    with_a_keyframe_at_frame_100(project, AnimationTarget::Clip(clip), property)
}

/// The same project as `project_of_clips(&[(0, 600, 1)])` but with a keyframe for `property` of its track at frame 100.
fn project_with_a_keyframe_of_the_track_for(property: PropertyId) -> Project {
    let project = project_of_clips(&[(0, 600, 1)]);
    let track = project.tracks()[0].id();
    with_a_keyframe_at_frame_100(project, AnimationTarget::Track(track), property)
}

/// How many streams of sound have been started once the project has become `changed_project` while the sound has been
/// playing steadily from the start of `project_of_clips(&[(0, 600, 1)])`: one if the sound goes on as it was, and two if it
/// is made again.
fn streams_started_when_the_project_becomes(changed_project: Project) -> usize {
    let mut bench = Bench::new();
    bench.update_playing_at(3.0);
    let clock = bench.play_steadily(3.0, 1.0);
    assert_eq!(bench.streams_started(), 1);

    bench.project = changed_project;
    bench.let_time_pass(A_FRAME_OF_THE_WINDOW);
    bench.update_playing_at(clock);
    bench.let_time_pass(Duration::from_millis(150));
    bench.update_playing_at(clock);
    bench.streams_started()
}

#[test]
fn a_keyframe_for_how_loud_a_clip_is_changes_what_the_sound_is_made_of_and_starts_it_again() {
    assert_eq!(
        streams_started_when_the_project_becomes(project_with_a_keyframe_for(PropertyId::Gain)),
        2,
        "the sound is made again, with its keyframes"
    );
}

/// Found by the planted-mistakes check: leaving the keyframes of a track out of what the sound is made of changed no test,
/// so a track whose loudness was changed by a keyframe while the preview played was heard as it was until something else
/// made the sound again.
#[test]
fn a_keyframe_for_anything_that_is_heard_of_a_clip_or_of_a_track_starts_the_sound_again() {
    for property in PropertyId::OWN.into_iter().filter(|property| property.is_heard()) {
        let projects = [
            ("a clip", project_with_a_keyframe_for(property)),
            ("a track", project_with_a_keyframe_of_the_track_for(property)),
        ];
        for (whose, project) in projects {
            assert_eq!(
                streams_started_when_the_project_becomes(project),
                2,
                "a keyframe for {property:?} of {whose} makes the sound again, with its keyframes"
            );
        }
    }
}

#[test]
fn a_keyframe_for_where_a_clip_is_on_the_picture_changes_nothing_that_is_heard() {
    assert_eq!(
        streams_started_when_the_project_becomes(project_with_a_keyframe_for(PropertyId::Position)),
        1,
        "the sound goes on as it was"
    );
}

// --- editing while the video plays ---

#[test]
fn an_edit_while_playing_leaves_the_old_sound_playing_until_the_new_is_ready_and_then_joins_it_without_a_gap() {
    let mut bench = Bench::new();
    bench.update_playing_at(3.0);
    let clock = bench.play_steadily(3.0, 1.0);
    assert_eq!(bench.streams_started(), 1);

    // A volume is changed, and the sound that is made for that takes a while to produce anything.
    bench.starter.decoded_at_the_start.set(0);
    bench.speakers.borrow_mut().underrun_moments = 0;
    let times_cleared_before = bench.speakers.borrow().times_cleared;
    bench.project = project_with_a_clip_that_is_louder_by(6.0);
    let clock = bench.play_steadily(clock, 0.25);

    assert_eq!(
        bench.streams_started(),
        2,
        "the sound for the edited project is being made"
    );
    assert!(!bench.stream(0).was_stopped.get(), "the old sound goes on while it is");
    assert_eq!(bench.speakers.borrow().underrun_moments, 0, "with no gap in it");
    assert_eq!(
        bench.speakers.borrow().times_cleared,
        times_cleared_before,
        "and nothing thrown away"
    );

    // The new sound arrives. The replacement was started where the playhead was 0.25 s ago, so its
    // first moments are late and are skipped, and it joins in step with the clock.
    let replacement_started_at = 3.0 + 1.0;
    bench.stream(1).decoded_moments.set(u64::MAX / 4);
    let clock = bench.play_steadily(clock, 0.5);

    assert!(bench.stream(0).was_stopped.get(), "the old stream has been let go of");
    assert_eq!(bench.streams_started(), 2);
    assert_eq!(bench.speakers.borrow().underrun_moments, 0, "no gap at the join either");
    assert_eq!(bench.speakers.borrow().times_cleared, times_cleared_before);
    let splices = bench.speakers.borrow().splices.clone();
    assert_eq!(splices.len(), 1, "joined once: {splices:?}");
    assert_eq!(
        (splices[0].0, splices[0].1),
        (
            (SECONDS_OF_OLD_SOUND_KEPT_WHEN_TAKING_OVER * f64::from(SAMPLE_RATE)).round() as u64,
            (SECONDS_TO_FADE_WHEN_TAKING_OVER * f64::from(SAMPLE_RATE)).round() as u64
        ),
        "a little of the old sound is kept, and it fades into the new"
    );

    // What is heard at the end is the new stream's sound, and it is where the clock says.
    let played = bench.played();
    let expected = (clock - replacement_started_at) * f64::from(SAMPLE_RATE);
    let last = f64::from(*played.last().unwrap());
    assert!(
        (last - expected).abs() < 2500.0,
        "the sound at the end is at moment {last} of the new stream, near {expected}"
    );
    let tail = &played[played.len() - 4000..];
    assert_consecutive(tail, "the new sound is steady once it has taken over");
}

#[test]
fn edits_that_keep_coming_make_the_sound_catch_up_every_so_often_and_it_never_has_a_gap() {
    let mut bench = Bench::new();
    bench.update_playing_at(3.0);
    let mut clock = bench.play_steadily(3.0, 1.0);
    bench.speakers.borrow_mut().underrun_moments = 0;
    let streams_before = bench.streams_started();

    // A slider is dragged for two seconds: what the sound is made of is different in every frame.
    for step in 0..120 {
        bench.project = project_with_a_clip_that_is_louder_by(step as f32 * 0.1);
        clock = bench.play_steadily(clock, 1.0 / 60.0);
    }

    let new_streams = bench.streams_started() - streams_before;
    assert!(
        new_streams >= 8,
        "the sound is made again every so often: {new_streams} times in 2 s"
    );
    assert!(
        new_streams <= 21,
        "but never more often than every 100 milliseconds: {new_streams} times in 2 s"
    );
    assert_eq!(bench.speakers.borrow().underrun_moments, 0, "and there is never a gap");
}

#[test]
fn a_replacement_that_arrives_hopelessly_late_is_dropped_and_the_old_sound_goes_on_with_another_one_made() {
    let mut bench = Bench::new();
    bench.update_playing_at(3.0);
    let clock = bench.play_steadily(3.0, 1.0);
    bench.starter.decoded_at_the_start.set(0);
    bench.speakers.borrow_mut().underrun_moments = 0;
    bench.project = project_with_a_clip_that_is_louder_by(6.0);
    let clock = bench.play_steadily(clock, 4.0);
    assert_eq!(bench.streams_started(), 2, "the replacement is still being waited for");

    // Its first sound comes four seconds late: of no use. The next one is quick.
    bench.starter.decoded_at_the_start.set(u64::MAX / 4);
    bench.stream(1).decoded_moments.set(u64::MAX / 4);
    bench.play_steadily(clock, 0.5);

    assert!(
        bench.stream(1).was_stopped.get(),
        "the replacement that was too late is let go of"
    );
    assert_eq!(bench.streams_started(), 3, "and another is made");
    assert!(bench.stream(0).was_stopped.get(), "which took over");
    assert_eq!(
        bench.speakers.borrow().underrun_moments,
        0,
        "through all of it the sound did not stop"
    );
}

#[test]
fn an_edit_that_leaves_nothing_to_hear_silences_the_sound_at_once() {
    let mut bench = Bench::new();
    bench.update_playing_at(3.0);
    let clock = bench.play_steadily(3.0, 1.0);
    let times_cleared_before = bench.speakers.borrow().times_cleared;

    // The only clip is muted.
    let mut muted = project_of_clips(&[(0, 600, 1)]);
    let location = muted.tracks()[0].placement_at_index(0).unwrap().id;
    muted.set_clip_muted(location, true).unwrap();
    bench.project = muted;
    bench.let_time_pass(A_FRAME_OF_THE_WINDOW);
    bench.update_playing_at(clock);

    assert_eq!(bench.waiting_moments(), 0, "nothing is left waiting at the speakers");
    assert_eq!(bench.speakers.borrow().times_cleared, times_cleared_before + 1);
    assert!(bench.stream(0).was_stopped.get());
    assert!(!bench.supplier.is_running());
}

#[test]
fn moving_the_playhead_a_long_way_while_a_replacement_is_being_made_starts_everything_again_from_there() {
    let mut bench = Bench::new();
    bench.update_playing_at(3.0);
    let clock = bench.play_steadily(3.0, 1.0);
    bench.starter.decoded_at_the_start.set(0);
    bench.project = project_with_a_clip_that_is_louder_by(6.0);
    let clock = bench.play_steadily(clock, 0.2);
    assert_eq!(bench.streams_started(), 2);

    bench.starter.decoded_at_the_start.set(u64::MAX / 4);
    bench.let_time_pass(A_FRAME_OF_THE_WINDOW);
    bench.update_playing_at(clock + 10.0);
    bench.let_time_pass(MINIMUM_TIME_BETWEEN_STREAM_STARTS);
    bench.update_playing_at(clock + 10.1);

    assert!(bench.stream(0).was_stopped.get());
    assert!(
        bench.stream(1).was_stopped.get(),
        "the replacement was for a place the playhead has left"
    );
    assert_eq!(bench.streams_started(), 3);
    let request = bench.starter.requests.borrow()[2].clone();
    assert_eq!(
        first_sound_clip(&request).first_source_frame,
        frame_rate().frame_at_seconds(clock + 10.0),
        "from where the playhead is now"
    );
}

#[test]
fn pausing_while_a_replacement_is_being_made_lets_go_of_it_too() {
    let mut bench = Bench::new();
    bench.update_playing_at(3.0);
    let clock = bench.play_steadily(3.0, 1.0);
    bench.starter.decoded_at_the_start.set(0);
    bench.project = project_with_a_clip_that_is_louder_by(6.0);
    let clock = bench.play_steadily(clock, 0.2);
    bench.update_paused_at(clock);
    assert!(bench.stream(0).was_stopped.get());
    assert!(bench.stream(1).was_stopped.get());
    assert!(!bench.supplier.is_running());
}

#[test]
fn changing_what_sped_up_sound_is_like_while_playing_starts_the_sound_again_in_the_new_way() {
    for (chosen, started_streams_so_far) in [(SpedUpClipSound::Mute, 2), (SpedUpClipSound::IncreasePitch, 2)] {
        let mut bench = Bench::with(project_of_clips(&[(0, 300, 1), (300, 600, 3)]), 1);
        bench.update_playing_at(1.0);
        let clock = bench.play_steadily(1.0, 0.5);

        bench.sped_up_clip_sound = chosen;
        bench.let_time_pass(Duration::from_millis(150));
        bench.update_playing_at(clock + 0.15);

        assert_eq!(bench.streams_started(), started_streams_so_far, "{chosen:?}");
        assert_eq!(
            bench.starter.requests.borrow()[0].sound.sped_up_clip_sound,
            SpedUpClipSound::MaintainPitch
        );
        assert_eq!(bench.starter.requests.borrow()[1].sound.sped_up_clip_sound, chosen);
    }
}

#[test]
fn a_stream_that_takes_a_second_to_start_is_caught_up_with_and_not_started_over() {
    let mut bench = Bench::new();
    bench.starter.decoded_at_the_start.set(0); // a big file on a slow disk
    bench.update_playing_at(3.0);
    let mut clock = 3.0;
    for _ in 0..60 {
        bench.let_time_pass(A_FRAME_OF_THE_WINDOW);
        clock += A_FRAME_OF_THE_WINDOW.as_secs_f64();
        bench.update_playing_at(clock);
    }
    assert_eq!(
        bench.streams_started(),
        1,
        "waiting for the first sound is not a reason to start again"
    );

    bench.stream(0).decoded_moments.set(u64::MAX / 4);
    bench.update_playing_at(clock);
    bench.play_steadily(clock, 0.5);

    assert_eq!(bench.streams_started(), 1);
    let played = bench.played();
    let expected_first_moment = (clock - 3.0) * f64::from(SAMPLE_RATE);
    assert!(
        (played[0] as f64 - expected_first_moment).abs() < 1500.0,
        "the sound starts a second in, where the clock is: {} against {expected_first_moment}",
        played[0]
    );
}

#[test]
fn a_stream_that_never_gets_going_is_given_up_on_and_started_again() {
    let mut bench = Bench::new();
    bench.starter.decoded_at_the_start.set(0);
    bench.update_playing_at(3.0);
    let mut clock = 3.0;
    for _ in 0..(4 * 60) {
        bench.let_time_pass(A_FRAME_OF_THE_WINDOW);
        clock += A_FRAME_OF_THE_WINDOW.as_secs_f64();
        bench.update_playing_at(clock);
    }
    // Four seconds later, with the decoder still silent, the first sound arrives: it is too late to
    // be of use, and the sound is started again from where the playhead is now.
    bench.stream(0).decoded_moments.set(u64::MAX / 4);
    bench.update_playing_at(clock);
    assert_eq!(bench.streams_started(), 2);
}

#[test]
fn a_stream_that_has_ended_is_left_alone_while_what_is_left_of_the_sound_runs_out() {
    let mut bench = Bench::new();
    bench.starter.decoded_at_the_start.set(30_000);
    bench.update_playing_at(3.0);
    *bench.stream(0).finishes_with.borrow_mut() = Some(None);
    let clock = bench.play_steadily(3.0, 2.0);

    // All 30000 moments were heard (well under a second); then two more seconds of nothing.
    assert_eq!(
        bench.streams_started(),
        1,
        "an ended stream is not started again and again"
    );
    assert_eq!(bench.played().len(), 30_000);
    assert!(clock > 4.9);
}

// --- stopping and starting ---

#[test]
fn pausing_silences_the_speakers_at_once_and_stops_the_decoder() {
    let mut bench = Bench::new();
    bench.update_playing_at(3.0);
    bench.play_steadily(3.0, 0.5);
    assert!(bench.waiting_moments() > 0);

    bench.update_paused_at(3.5);

    assert_eq!(bench.waiting_moments(), 0);
    assert!(bench.stream(0).was_stopped.get());
    assert!(!bench.supplier.is_running());
}

#[test]
fn playing_again_after_a_pause_starts_a_new_stream_from_where_the_playhead_is() {
    let mut bench = Bench::new();
    bench.update_playing_at(3.0);
    bench.play_steadily(3.0, 0.5);
    bench.update_paused_at(3.5);
    bench.let_time_pass(Duration::from_secs(2));

    bench.update_playing_at(3.5);

    assert_eq!(bench.streams_started(), 2);
    assert_eq!(
        first_sound_clip(&bench.starter.requests.borrow()[1]).first_source_frame,
        105
    );
}

#[test]
fn pausing_over_and_over_does_not_keep_clearing_the_speakers() {
    let mut bench = Bench::new();
    bench.update_paused_at(1.0);
    bench.update_paused_at(1.0);
    bench.update_paused_at(1.0);
    assert_eq!(
        bench.speakers.borrow().times_cleared,
        0,
        "nothing was running, so nothing needed stopping"
    );
}

#[test]
fn streams_are_not_started_more_often_than_the_minimum() {
    let mut bench = Bench::new();
    bench.update_playing_at(1.0);
    assert_eq!(bench.streams_started(), 1);

    // Two long seeks 30 ms apart: the second restart has to wait.
    bench.let_time_pass(Duration::from_millis(30));
    bench.update_playing_at(10.0);
    assert_eq!(bench.streams_started(), 1, "too soon after the last start");
    assert!(!bench.supplier.is_running());

    bench.let_time_pass(MINIMUM_TIME_BETWEEN_STREAM_STARTS);
    bench.update_playing_at(10.1);
    assert_eq!(bench.streams_started(), 2, "now it may");
}

// --- when things go wrong ---

#[test]
fn a_decoder_that_cannot_be_started_is_remembered_and_not_tried_again_for_a_while() {
    let mut bench = Bench::new();
    bench.starter.fails.set(true);

    bench.update_playing_at(1.0);
    assert!(
        bench
            .supplier
            .last_problem()
            .is_some_and(|problem| problem.contains("ffmpeg"))
    );
    let attempts = bench.starter.attempts.get();
    assert_eq!(attempts, 1);

    for _ in 0..30 {
        bench.let_time_pass(A_FRAME_OF_THE_WINDOW);
        bench.update_playing_at(1.0);
    }
    assert_eq!(bench.starter.attempts.get(), 1, "not on every frame");

    bench.let_time_pass(MINIMUM_TIME_BETWEEN_FAILED_ATTEMPTS);
    bench.starter.fails.set(false);
    bench.update_playing_at(3.5);
    assert_eq!(bench.streams_started(), 1, "and when it works again, it plays");
}

#[test]
fn a_stream_that_ends_in_an_error_is_remembered_and_is_not_started_over_and_over() {
    let mut bench = Bench::new();
    bench.starter.decoded_at_the_start.set(0); // nothing comes out of it
    bench.update_playing_at(1.0);
    *bench.stream(0).finishes_with.borrow_mut() = Some(Some("Invalid data found".to_owned()));

    bench.update_playing_at(1.0);
    for _ in 0..20 {
        bench.let_time_pass(A_FRAME_OF_THE_WINDOW);
        bench.update_playing_at(1.0);
    }

    assert!(
        bench
            .supplier
            .last_problem()
            .is_some_and(|problem| problem.contains("Invalid data found"))
    );
    assert_eq!(bench.streams_started(), 1);
}

#[test]
fn speakers_that_cannot_be_opened_are_a_remembered_problem_and_are_tried_again_later() {
    let attempts = Rc::new(Cell::new(0));
    let speakers = Rc::new(RefCell::new(SpeakerState::default()));
    let working = Rc::new(Cell::new(false));
    let mut bench = Bench::new();
    {
        let attempts = Rc::clone(&attempts);
        let speakers = Rc::clone(&speakers);
        let working = Rc::clone(&working);
        bench.supplier.play_through(Box::new(move || {
            attempts.set(attempts.get() + 1);
            if working.get() {
                Ok(Box::new(FakeSpeakers {
                    state: Rc::clone(&speakers),
                    format: AudioFormat {
                        sample_rate: SAMPLE_RATE,
                        channel_count: 1,
                    },
                }) as Box<dyn AudioSink>)
            } else {
                Err("no sound device".to_owned())
            }
        }));
    }

    bench.update_playing_at(1.0);
    assert_eq!(bench.starter.attempts.get(), 0, "no sound without speakers");
    assert!(
        bench
            .supplier
            .last_problem()
            .is_some_and(|problem| problem.contains("no sound device"))
    );
    for _ in 0..30 {
        bench.let_time_pass(A_FRAME_OF_THE_WINDOW);
        bench.update_playing_at(1.0);
    }
    assert_eq!(attempts.get(), 1, "not on every frame");

    working.set(true);
    bench.let_time_pass(MINIMUM_TIME_BETWEEN_FAILED_ATTEMPTS);
    bench.update_playing_at(2.0);
    assert_eq!(attempts.get(), 2);
    assert_eq!(bench.starter.attempts.get(), 1, "and now there is sound");
}

#[test]
fn without_any_speakers_given_there_is_no_sound_and_no_complaint() {
    let starter = FakeStarter {
        requests: Rc::default(),
        streams: Rc::default(),
        decoded_at_the_start: Rc::new(Cell::new(u64::MAX / 4)),
        attempts: Rc::default(),
        fails: Rc::default(),
    };
    let mut supplier = AudioSupplier::new(starter.clone());
    let project = project_of_clips(&[(0, 600, 1)]);
    let source_locations = HashMap::from([(VIDEO, MediaLocation::from("holiday.mp4"))]);
    let content = AudioContent {
        project: &project,
        source_locations: &source_locations,
        sped_up_clip_sound: SpedUpClipSound::MaintainPitch,
    };
    supplier.update(Some(content), Bench::clock_at(1.0, true), Instant::now());
    assert_eq!(starter.attempts.get(), 0);
    assert_eq!(supplier.last_problem(), None);
}

#[test]
fn speakers_that_stop_working_are_let_go_of_and_opened_again_a_little_later() {
    let mut bench = Bench::new();
    bench.update_playing_at(3.0);
    let clock = bench.play_steadily(3.0, 0.5);
    assert_eq!(bench.streams_started(), 1);

    bench.speakers.borrow_mut().broken = true; // unplugged
    bench.let_time_pass(A_FRAME_OF_THE_WINDOW);
    bench.update_playing_at(clock);
    assert!(
        bench.stream(0).was_stopped.get(),
        "no point decoding sound nobody can hear"
    );
    assert!(
        bench
            .supplier
            .last_problem()
            .is_some_and(|problem| problem.contains("stopped working"))
    );

    // Plugged in again (the opener makes working speakers each time it is asked), and a little later:
    bench.speakers.borrow_mut().broken = false;
    bench.let_time_pass(MINIMUM_TIME_BETWEEN_FAILED_ATTEMPTS);
    bench.update_playing_at(clock + 2.0);
    assert_eq!(bench.streams_started(), 2, "the sound goes on");
}

// --- more than one channel ---

#[test]
fn skipping_late_sound_never_splits_a_moment_so_the_channels_stay_together() {
    let mut bench = Bench::with(project_of_clips(&[(0, 600, 1)]), 2);
    bench.starter.decoded_at_the_start.set(0);
    bench.update_playing_at(3.0);
    bench.let_time_pass(Duration::from_millis(100));
    bench.stream(0).decoded_moments.set(u64::MAX / 4);
    bench.update_playing_at(3.1);
    bench.play_steadily(3.1, 0.5);

    let samples = bench.speakers.borrow().played.clone();
    assert!(samples.len() > 1000);
    for moment in samples.as_chunks::<2>().0 {
        assert_eq!(
            moment[0], -moment[1],
            "left and right belong to the same moment: {moment:?}"
        );
    }
}
