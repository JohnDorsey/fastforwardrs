//! Keeping the sound of the live preview in step with the clock.
//!
//! While the video plays, [`AudioSupplier`] has an [`AudioStream`] decode the sound of the
//! timeline from the playhead on, and feeds it to the speakers (an [`AudioSink`]) a little ahead of
//! when it is heard. The sound must stay where the picture is:
//!
//! * when the stream starts, decoding takes a moment, and the clock has moved on by the time the
//!   first sound arrives; so the sound that is already late is skipped;
//! * a small difference later on (the playhead stepped by an arrow key, a hiccup) is put right the
//!   same way: sound that is late is skipped, and sound that is early is held back by a little
//!   silence;
//! * a big difference (the playhead was moved a long way) or a change to what the sound is made of
//!   (a clip was sped up) makes it start again from the playhead;
//! * when the video is not playing there is no sound, and nothing is running.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::composition::plan_sound;
use crate::media::export::{sound_graph_specification, sources_of_sound};
use crate::media::{
    AudioFormat, AudioStream, AudioStreamError, AudioStreamEvent, AudioStreamRequest, FfmpegToolchain,
    UserInterfaceWakeUp,
};
use crate::project::{Clip, Project, SourceMediaIdentifier, SpedUpClipSound, TimelineFrameIndex};

/// How much sound is kept waiting at the speakers, in seconds. Enough to ride out a late frame of
/// the window without a gap, and little enough that a change (pausing, say) is heard at once.
pub const SECONDS_OF_SOUND_KEPT_WAITING: f64 = 0.3;

/// How far the sound may be from where the clock says it should be before it is nudged into place,
/// in seconds. Smaller differences are not worth a correction, and the clock and the speakers are
/// not read at the same instant, so they never agree exactly.
pub const SMALLEST_DIFFERENCE_TO_CORRECT_IN_SECONDS: f64 = 0.03;

/// How far the sound may be from where the clock says it should be before it is started again
/// from the playhead instead of being nudged, in seconds.
pub const LARGEST_DIFFERENCE_TO_CORRECT_IN_SECONDS: f64 = 0.25;

/// How long a stream may take to produce its first sound, in seconds, before it is given up on. Up
/// to this much of the start of the sound is skipped to catch up with the clock (decoding takes
/// longer for a big file that has to be found on the disk first), whereas later on a difference
/// of more than [`LARGEST_DIFFERENCE_TO_CORRECT_IN_SECONDS`] means that the playhead was moved.
pub const LONGEST_START_UP_TO_CATCH_UP_WITH_IN_SECONDS: f64 = 3.0;

/// A new stream is not started more often than this. (Starting ffmpeg takes tens of milliseconds
/// and a person holding down an arrow key moves the playhead on every key repeat.)
pub const MINIMUM_TIME_BETWEEN_STREAM_STARTS: Duration = Duration::from_millis(100);

/// After something fails (the speakers or ffmpeg could not be started) it is not tried again for
/// this long, so that a failure that will not go away does not cost anything every frame.
pub const MINIMUM_TIME_BETWEEN_FAILED_ATTEMPTS: Duration = Duration::from_secs(2);

/// Where the sound goes: the speakers, or something standing in for them.
pub trait AudioSink {
    /// What the speakers want the sound to be like.
    fn format(&self) -> AudioFormat;
    /// Adds samples (interleaved by channel, whole moments) to be played after those already waiting.
    fn queue(&mut self, samples: &[f32]);
    /// Throws away what is waiting to be played, so that there is silence at once.
    fn clear(&mut self);
    /// How many moments of sound are waiting to be played.
    fn queued_moment_count(&self) -> u64;
    /// Whether the speakers still work (a sound device can be unplugged while it is in use).
    fn is_working(&self) -> bool {
        true
    }
}

/// Opens the speakers when they are first needed. (They are not opened at startup, so that a
/// program that never plays anything does not take hold of the sound device.)
pub type AudioSinkOpener = Box<dyn FnMut() -> Result<Box<dyn AudioSink>, String>>;

/// A running source of sound. ffmpeg is one; the tests have their own.
pub trait AudioSource {
    /// The next thing the stream has to report, if anything is ready. Never waits.
    fn try_next_event(&self) -> Option<AudioStreamEvent>;
}

impl AudioSource for AudioStream {
    fn try_next_event(&self) -> Option<AudioStreamEvent> {
        AudioStream::try_next_event(self)
    }
}

/// Starts streams of sound.
pub trait AudioStreamStarter {
    type Stream: AudioSource;

    fn start(&self, request: &AudioStreamRequest) -> Result<Self::Stream, AudioStreamError>;
}

/// The starter that runs ffmpeg. Without ffmpeg (`None`) every start fails, so that the rest of the
/// editor keeps working without sound.
pub struct FfmpegAudioStreamStarter {
    toolchain: Option<Arc<FfmpegToolchain>>,
    wake_up: UserInterfaceWakeUp,
}

impl FfmpegAudioStreamStarter {
    pub fn new(toolchain: Option<Arc<FfmpegToolchain>>, wake_up: UserInterfaceWakeUp) -> Self {
        Self { toolchain, wake_up }
    }
}

impl AudioStreamStarter for FfmpegAudioStreamStarter {
    type Stream = AudioStream;

    fn start(&self, request: &AudioStreamRequest) -> Result<AudioStream, AudioStreamError> {
        match &self.toolchain {
            Some(toolchain) => AudioStream::start(toolchain, request, Arc::clone(&self.wake_up)),
            None => Err(AudioStreamError::CouldNotStart(std::io::Error::other(
                "ffmpeg could not be found",
            ))),
        }
    }
}

/// What the sound is made of.
#[derive(Clone, Copy)]
pub struct AudioContent<'a> {
    pub project: &'a Project,
    /// Where each source file is that can be used. The sound of a file that is not here is left out.
    pub source_paths: &'a HashMap<SourceMediaIdentifier, PathBuf>,
    pub sped_up_clip_sound: SpedUpClipSound,
}

impl AudioContent<'_> {
    /// A number that is different whenever what the sound is made of is different, wherever the
    /// playhead is. (Equal numbers for different content are possible but astronomically unlikely,
    /// and would only keep the old sound until the playhead is moved.) Only what is heard counts:
    /// a change to text, or to where the picture is, leaves the sound as it is.
    fn signature(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.sped_up_clip_sound.hash(&mut hasher);
        let frame_rate = self.project.frame_rate();
        frame_rate.numerator().hash(&mut hasher);
        frame_rate.denominator().hash(&mut hasher);
        for source in self.project.sources() {
            source.identifier.hash(&mut hasher);
            source.properties.has_sound().hash(&mut hasher);
            source.properties.frame_count_at(frame_rate).hash(&mut hasher);
            self.source_paths.get(&source.identifier).hash(&mut hasher);
        }
        for track in self.project.tracks() {
            track.id().hash(&mut hasher);
            track.gain().decibels().to_bits().hash(&mut hasher);
            track.speed().value().hash(&mut hasher);
            track.is_muted().hash(&mut hasher);
            for entry in track.entries() {
                let media = match entry.clip() {
                    Clip::Video(media) | Clip::Audio(media) => media,
                    // Text has no sound, and where it is changes nothing but the gaps (hashed below).
                    Clip::Text(_) => continue,
                };
                entry.id().hash(&mut hasher);
                entry.gap_before_frames().hash(&mut hasher);
                media.source().hash(&mut hasher);
                media.first_source_frame().value().hash(&mut hasher);
                media.end_source_frame().value().hash(&mut hasher);
                media.speed().value().hash(&mut hasher);
                media.gain().decibels().to_bits().hash(&mut hasher);
                media.is_muted().hash(&mut hasher);
            }
        }
        hasher.finish()
    }
}

/// What the clock says, for one frame of the window.
#[derive(Clone, Copy, Debug)]
pub struct ClockReading {
    pub frame: TimelineFrameIndex,
    /// The time into the timeline in seconds, more exactly than the whole frame.
    pub seconds: f64,
    pub is_playing: bool,
}

/// A stream that is running, and where in the timeline its sound starts.
struct RunningStream<Stream> {
    stream: Stream,
    /// What the sound was made of when the stream started.
    signature: u64,
    /// The time into the timeline, in seconds, of the stream's first moment of sound.
    origin_seconds: f64,
    /// How many moments of sound have been taken from the stream, whether played or skipped.
    moments_taken: u64,
    /// Whether any of the stream's sound has been queued yet. Until then it is still catching up
    /// with the clock after the time decoding took to start.
    has_caught_up: bool,
    /// Whether the stream has ended (all the sound was delivered, or it failed).
    finished: bool,
}

impl<Stream> RunningStream<Stream> {
    /// How many seconds early the end of the sound that is queued will be heard, against the clock
    /// (negative if late), given `queued_moments` waiting at `moments_per_second`. When this is 0
    /// the sound is exactly where the clock says it should be; it stays 0 while all goes well,
    /// because the clock and the speakers go on at the same pace.
    fn early_by(&self, clock_seconds: f64, queued_moments: u64, moments_per_second: f64) -> f64 {
        let is_sound_of = self.origin_seconds + self.moments_taken as f64 / moments_per_second;
        let heard_at = clock_seconds + queued_moments as f64 / moments_per_second;
        is_sound_of - heard_at
    }

    /// Whether `early_by` seconds is too much to be put right by skipping or holding back sound.
    fn is_too_far_off(&self, early_by: f64) -> bool {
        if !self.has_caught_up {
            // Decoding takes time, so the clock has moved on: that much is skipped, whatever it is.
            early_by > LARGEST_DIFFERENCE_TO_CORRECT_IN_SECONDS
                || early_by < -LONGEST_START_UP_TO_CATCH_UP_WITH_IN_SECONDS
        } else {
            early_by.abs() > LARGEST_DIFFERENCE_TO_CORRECT_IN_SECONDS
        }
    }
}

/// Gets the sound of the preview to the speakers, in step with the clock.
pub struct AudioSupplier<Starter: AudioStreamStarter> {
    starter: Starter,
    open_sink: Option<AudioSinkOpener>,
    sink: Option<Box<dyn AudioSink>>,
    running: Option<RunningStream<Starter::Stream>>,
    last_stream_start_at: Option<Instant>,
    last_failed_speakers_attempt_at: Option<Instant>,
    last_failed_stream_attempt_at: Option<Instant>,
    last_problem: Option<String>,
}

impl<Starter: AudioStreamStarter> AudioSupplier<Starter> {
    /// A supplier that makes no sound until it is told where to play it ([`Self::play_through`]).
    pub fn new(starter: Starter) -> Self {
        Self {
            starter,
            open_sink: None,
            sink: None,
            running: None,
            last_stream_start_at: None,
            last_failed_speakers_attempt_at: None,
            last_failed_stream_attempt_at: None,
            last_problem: None,
        }
    }

    /// Plays the sound through speakers made by `open_sink`, which is called when sound is first
    /// needed (and again, after a failure, a little later).
    pub fn play_through(&mut self, open_sink: AudioSinkOpener) {
        self.stop();
        self.sink = None;
        self.open_sink = Some(open_sink);
        self.last_failed_speakers_attempt_at = None;
    }

    /// Whether a stream is running.
    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// What last went wrong with the sound, if anything has. The sound is a convenience, so
    /// a problem with it is for the program to remember and not to interrupt the user with.
    pub fn last_problem(&self) -> Option<&str> {
        self.last_problem.as_deref()
    }

    /// Does what the sound needs this frame of the window. `content` is `None` when there is no
    /// sound to play (no video, or a video without sound, or one that cannot be read).
    pub fn update(&mut self, content: Option<AudioContent<'_>>, clock: ClockReading, now: Instant) {
        let Some(content) = content.filter(|_| clock.is_playing) else {
            self.stop();
            return;
        };
        if self.sink.as_ref().is_some_and(|sink| !sink.is_working()) {
            self.stop();
            self.sink = None;
            self.last_failed_speakers_attempt_at = Some(now);
            self.last_problem = Some("the speakers stopped working".to_owned());
        }
        if !self.make_sure_there_are_speakers(now) {
            return;
        }
        let signature = content.signature();
        if self
            .running
            .as_ref()
            .is_some_and(|running| running.signature != signature)
        {
            self.stop();
        }
        if self.running.is_none() {
            self.start(&content, signature, clock, now);
        }
        if self.feed_the_speakers(clock) == SoundIs::TooFarOff {
            // Started again from the playhead as soon as it is allowed (see below).
            self.stop();
            self.start(&content, signature, clock, now);
        }
    }

    /// Stops the sound at once. (If nothing was running there is nothing to stop, and the speakers
    /// are left alone.)
    fn stop(&mut self) {
        if self.running.take().is_some()
            && let Some(sink) = &mut self.sink
        {
            sink.clear();
        }
    }

    /// Opens the speakers if that has not been done. Returns whether there are speakers.
    fn make_sure_there_are_speakers(&mut self, now: Instant) -> bool {
        if self.sink.is_some() {
            return true;
        }
        let Some(open_sink) = &mut self.open_sink else {
            return false;
        };
        if self
            .last_failed_speakers_attempt_at
            .is_some_and(|failed_at| now.saturating_duration_since(failed_at) < MINIMUM_TIME_BETWEEN_FAILED_ATTEMPTS)
        {
            return false;
        }
        match open_sink() {
            Ok(sink) => {
                self.sink = Some(sink);
                true
            }
            Err(problem) => {
                self.last_failed_speakers_attempt_at = Some(now);
                self.last_problem = Some(format!("the speakers could not be opened: {problem}"));
                false
            }
        }
    }

    /// Starts a stream of the sound from the playhead on, unless it is too soon after the last one.
    fn start(&mut self, content: &AudioContent<'_>, signature: u64, clock: ClockReading, now: Instant) {
        let Some(sink) = &self.sink else {
            return;
        };
        if self
            .last_stream_start_at
            .is_some_and(|started_at| now.saturating_duration_since(started_at) < MINIMUM_TIME_BETWEEN_STREAM_STARTS)
        {
            return;
        }
        if self
            .last_failed_stream_attempt_at
            .is_some_and(|failed_at| now.saturating_duration_since(failed_at) < MINIMUM_TIME_BETWEEN_FAILED_ATTEMPTS)
        {
            return;
        }
        let plan = plan_sound(content.project, clock.frame, |source| {
            content.source_paths.contains_key(&source)
        });
        if plan.strips.is_empty() {
            return;
        }
        let source_identifiers: Vec<SourceMediaIdentifier> = sources_of_sound(&plan).into_iter().collect();
        let request = AudioStreamRequest {
            source_files: source_identifiers
                .iter()
                .map(|identifier| content.source_paths[identifier].clone())
                .collect(),
            sound: sound_graph_specification(&plan, &source_identifiers, content.sped_up_clip_sound),
            format: sink.format(),
        };
        match self.starter.start(&request) {
            Ok(stream) => {
                self.running = Some(RunningStream {
                    stream,
                    signature,
                    origin_seconds: content.project.frame_rate().seconds_at_frame(clock.frame.value()),
                    moments_taken: 0,
                    has_caught_up: false,
                    finished: false,
                });
                self.last_stream_start_at = Some(now);
            }
            Err(error) => {
                self.last_failed_stream_attempt_at = Some(now);
                self.last_problem = Some(error.to_string());
            }
        }
    }

    /// Moves sound from the stream to the speakers until a little is waiting there, putting it
    /// where the clock says it belongs.
    fn feed_the_speakers(&mut self, clock: ClockReading) -> SoundIs {
        let (Some(running), Some(sink)) = (&mut self.running, &mut self.sink) else {
            return SoundIs::Fine;
        };
        let format = sink.format();
        let moments_per_second = f64::from(format.sample_rate);
        let channel_count = usize::from(format.channel_count);
        let moments_to_keep_waiting = (SECONDS_OF_SOUND_KEPT_WAITING * moments_per_second) as u64;

        // The playhead may have been moved a long way since sound was last queued, in which case
        // what is waiting is the wrong sound, and must not go on playing while the speakers use
        // up the rest of it. (A stream that has ended has nothing left to be wrong about.)
        if !running.finished
            && running.is_too_far_off(running.early_by(clock.seconds, sink.queued_moment_count(), moments_per_second))
        {
            return SoundIs::TooFarOff;
        }

        while !running.finished && sink.queued_moment_count() < moments_to_keep_waiting {
            match running.stream.try_next_event() {
                None => break,
                Some(AudioStreamEvent::Finished { error_output }) => {
                    running.finished = true;
                    if let Some(error_output) = error_output {
                        self.last_problem = Some(format!("the sound could not be decoded: {error_output}"));
                    }
                }
                Some(AudioStreamEvent::Samples(samples)) => {
                    let moments_in_chunk = (samples.len() / channel_count) as u64;
                    let early_by = running.early_by(clock.seconds, sink.queued_moment_count(), moments_per_second);
                    if running.is_too_far_off(early_by) {
                        return SoundIs::TooFarOff;
                    }
                    running.moments_taken += moments_in_chunk;

                    let moments_to_move = (early_by.abs() * moments_per_second) as u64;
                    if early_by < -SMALLEST_DIFFERENCE_TO_CORRECT_IN_SECONDS {
                        // Late: what should already have been heard is skipped.
                        let moments_to_skip = moments_to_move.min(moments_in_chunk) as usize;
                        sink.queue(&samples[moments_to_skip * channel_count..]);
                        running.has_caught_up |= (moments_to_skip as u64) < moments_in_chunk;
                    } else {
                        if early_by > SMALLEST_DIFFERENCE_TO_CORRECT_IN_SECONDS {
                            // Early: a little silence holds it back until it is time.
                            sink.queue(&vec![0.0; moments_to_move as usize * channel_count]);
                        }
                        sink.queue(&samples);
                        running.has_caught_up = true;
                    }
                }
            }
        }
        SoundIs::Fine
    }
}

/// How the sound stands against the clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SoundIs {
    Fine,
    /// Too far from where the clock is to be nudged into place: it has to start again.
    TooFarOff,
}

#[cfg(test)]
mod tests;
