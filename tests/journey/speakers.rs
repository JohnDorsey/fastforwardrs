//! Stand-in speakers for the tests: they take the sound at the speed of real time, as a sound device
//! does, and keep everything they played, so that a test can look at what was heard and when.
//!
//! Every moment of the recording stands for 1/48000 of a second since the speakers were opened,
//! and a moment in which the speakers had no sound to play is silence in it (as it would be in the
//! room), so a place in the recording can be turned into a time and back.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use fastforwardrs::media::AudioFormat;
use fastforwardrs::playback::{AudioSink, AudioSinkOpener};

pub const SAMPLE_RATE: u32 = 48_000;

struct Recording {
    opened_at: Instant,
    /// What is waiting to be played, mono.
    waiting: VecDeque<f32>,
    /// What has been played, mono, one entry for every moment since `opened_at`.
    heard: Vec<f32>,
    times_cleared: usize,
}

impl Recording {
    /// Plays, from what is waiting, as much as real time has used up since the last call.
    fn play_up_to_now(&mut self) {
        let moments_that_have_passed = (self.opened_at.elapsed().as_secs_f64() * f64::from(SAMPLE_RATE)) as usize;
        while self.heard.len() < moments_that_have_passed {
            self.heard.push(self.waiting.pop_front().unwrap_or(0.0));
        }
    }
}

/// What a test holds on to, to look at what the speakers played.
#[derive(Clone)]
pub struct RecordingSpeakers {
    recording: Arc<Mutex<Option<Recording>>>,
}

impl RecordingSpeakers {
    pub fn new() -> Self {
        Self {
            recording: Arc::new(Mutex::new(None)),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<Recording>> {
        self.recording.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// What the application is given, to open the speakers when it first needs them.
    pub fn opener(&self) -> AudioSinkOpener {
        let speakers = self.clone();
        Box::new(move || {
            *speakers.lock() = Some(Recording {
                opened_at: Instant::now(),
                waiting: VecDeque::new(),
                heard: Vec::new(),
                times_cleared: 0,
            });
            Ok(Box::new(SpeakersInTheRoom {
                speakers: speakers.clone(),
            }) as Box<dyn AudioSink>)
        })
    }

    /// Whether the application has opened the speakers.
    pub fn were_opened(&self) -> bool {
        self.lock().is_some()
    }

    /// When the speakers were opened, which is when the recording starts.
    pub fn opened_at(&self) -> Instant {
        self.lock().as_ref().expect("the speakers were opened").opened_at
    }

    /// Everything played so far, up to this moment. One entry for every 1/48000 of a second since
    /// the speakers were opened.
    pub fn heard(&self) -> Vec<f32> {
        let mut guard = self.lock();
        let recording = guard.as_mut().expect("the speakers were opened");
        recording.play_up_to_now();
        recording.heard.clone()
    }

    pub fn times_cleared(&self) -> usize {
        self.lock().as_ref().map_or(0, |recording| recording.times_cleared)
    }
}

/// The speakers as the application sees them.
struct SpeakersInTheRoom {
    speakers: RecordingSpeakers,
}

impl AudioSink for SpeakersInTheRoom {
    fn format(&self) -> AudioFormat {
        AudioFormat {
            sample_rate: SAMPLE_RATE,
            channel_count: 1,
        }
    }

    fn queue(&mut self, samples: &[f32]) {
        let mut guard = self.speakers.lock();
        let recording = guard.as_mut().expect("open");
        recording.play_up_to_now();
        recording.waiting.extend(samples);
    }

    fn clear(&mut self) {
        let mut guard = self.speakers.lock();
        let recording = guard.as_mut().expect("open");
        recording.play_up_to_now();
        recording.waiting.clear();
        recording.times_cleared += 1;
    }

    fn queued_moment_count(&self) -> u64 {
        let mut guard = self.speakers.lock();
        let recording = guard.as_mut().expect("open");
        recording.play_up_to_now();
        recording.waiting.len() as u64
    }
}

/// The pitch in hertz of what was heard (see [`crate::support::pitch_in_hertz`]).
pub fn pitch_in_hertz(samples: &[f32]) -> f64 {
    crate::support::pitch_in_hertz(samples, SAMPLE_RATE)
}
