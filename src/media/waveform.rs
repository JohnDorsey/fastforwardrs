//! The shape of a sound, for drawing it on the timeline: how loud it gets, for every hundredth of a
//! second of it.
//!
//! ffmpeg decodes the sound file in the background into a plain stream of samples (mono, at a low
//! sample rate, because only the shape matters), and a thread of ours turns that into *peaks*: the
//! loudest sample in each stretch of a hundredth of a second. The peaks arrive in batches as the
//! decoding goes on, so a long file starts to show its shape at once.

use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
use std::sync::mpsc::SyncSender;

use thiserror::Error;

use super::UserInterfaceWakeUp;
use super::child_process::{StreamingTool, background_command};
use super::ffmpeg_toolchain::FfmpegToolchain;

/// How many peaks a second of sound has.
pub const PEAKS_PER_SECOND: u32 = 100;

/// The sample rate the sound is decoded at for its shape. (Low, because it is only for drawing, but
/// a multiple of [`PEAKS_PER_SECOND`] so that every peak is made of the same number of samples.)
const SAMPLE_RATE: u32 = 8_000;

const SAMPLES_PER_PEAK: usize = (SAMPLE_RATE / PEAKS_PER_SECOND) as usize;

const BYTES_PER_SAMPLE: usize = 2;

/// How far from the start of a peak (as a part of a peak) a moment can be and still count as being at it.
const ROUNDING_TOLERANCE_IN_PEAKS: f64 = 1e-6;

/// How many peaks are sent at once. (A batch is 10 seconds of sound.)
const PEAKS_PER_BATCH: usize = 1_000;

/// How many batches can wait to be collected before the decoding is made to wait.
const BATCHES_BUFFERED_AHEAD: usize = 8;

/// The shape of a sound: its loudness for each hundredth of a second, from 0 (silence) to 255 (as
/// loud as a sample can be).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Waveform {
    peaks: Vec<u8>,
}

impl Waveform {
    pub fn from_peaks(peaks: Vec<u8>) -> Self {
        Self { peaks }
    }

    pub fn peaks(&self) -> &[u8] {
        &self.peaks
    }

    pub fn peak_count(&self) -> usize {
        self.peaks.len()
    }

    /// Adds the peaks that come after the ones already there.
    pub fn extend(&mut self, more_peaks: &[u8]) {
        self.peaks.extend_from_slice(more_peaks);
    }

    /// How loud the sound gets between `from_seconds` and `to_seconds` into it: the largest of the
    /// peaks that stretch touches, or `None` if there are none (it is before the start or after the
    /// end of what is known). A stretch shorter than a peak still has the peak it falls in.
    pub fn loudest_between(&self, from_seconds: f64, to_seconds: f64) -> Option<u8> {
        if from_seconds.is_nan() || to_seconds.is_nan() || to_seconds < 0.0 {
            return None;
        }
        let peaks_per_second = f64::from(PEAKS_PER_SECOND);
        // A moment that is a tiny bit short of where a peak starts, only because of the rounding
        // of the numbers (1.1 seconds is not quite 1.1), counts as that peak's start.
        let first = (from_seconds.max(0.0) * peaks_per_second + ROUNDING_TOLERANCE_IN_PEAKS).floor();
        // The peak that the very end of the stretch falls in is included, unless the stretch ends
        // exactly where a peak starts.
        let end = (to_seconds * peaks_per_second - ROUNDING_TOLERANCE_IN_PEAKS)
            .ceil()
            .max(first + 1.0);
        if first >= self.peaks.len() as f64 {
            return None;
        }
        let first = first as usize;
        let end = (end as usize).min(self.peaks.len());
        self.peaks[first..end].iter().copied().max()
    }
}

/// Makes peaks from samples that arrive in pieces of any size.
#[derive(Debug, Default)]
struct PeakMaker {
    samples_in_this_peak: usize,
    loudest_in_this_peak: u16,
}

impl PeakMaker {
    /// Adds `samples` and puts the peaks they complete on the end of `peaks`.
    fn add_samples(&mut self, samples: impl IntoIterator<Item = i16>, peaks: &mut Vec<u8>) {
        for sample in samples {
            self.loudest_in_this_peak = self.loudest_in_this_peak.max(sample.unsigned_abs());
            self.samples_in_this_peak += 1;
            if self.samples_in_this_peak == SAMPLES_PER_PEAK {
                peaks.push(peak_of_loudest_sample(self.loudest_in_this_peak));
                *self = Self::default();
            }
        }
    }

    /// Puts the peak of the samples that did not make a whole one on the end of `peaks`, if there are any.
    fn finish(self, peaks: &mut Vec<u8>) {
        if self.samples_in_this_peak > 0 {
            peaks.push(peak_of_loudest_sample(self.loudest_in_this_peak));
        }
    }
}

/// The peak for a stretch whose loudest sample has this size (up to 32768, the largest a 16 bit
/// sample can have).
fn peak_of_loudest_sample(loudest_sample: u16) -> u8 {
    let scaled = (u32::from(loudest_sample) * 255).div_ceil(32_768);
    u8::try_from(scaled).unwrap_or(u8::MAX)
}

#[derive(Debug, Error)]
pub enum WaveformError {
    #[error("ffmpeg could not be started: {0}")]
    CouldNotStart(#[source] std::io::Error),
}

/// What a [`WaveformExtraction`] reports.
#[derive(Debug, PartialEq, Eq)]
pub enum WaveformEvent {
    /// The peaks that follow the ones already reported.
    Peaks(Vec<u8>),
    /// The decoding has ended: either the sound is all there, or `error_output` says why not.
    Finished { error_output: Option<String> },
}

/// A running ffmpeg process that decodes a sound file into its [`Waveform`], in the background.
/// Dropping it stops ffmpeg.
pub struct WaveformExtraction {
    tool: StreamingTool<WaveformEvent>,
}

impl WaveformExtraction {
    /// Starts decoding the sound of the file at `path`. The `wake_up` callback is called each time
    /// something new is waiting.
    pub fn start(
        toolchain: &FfmpegToolchain,
        path: &Path,
        wake_up: UserInterfaceWakeUp,
    ) -> Result<Self, WaveformError> {
        let mut command = background_command(toolchain.ffmpeg_path());
        command.args(waveform_arguments(path));
        let tool = StreamingTool::start(
            command,
            BATCHES_BUFFERED_AHEAD,
            move |standard_output, sender, error_output_when_finished| {
                read_peaks(standard_output, &sender, &wake_up, error_output_when_finished);
            },
        )
        .map_err(WaveformError::CouldNotStart)?;
        Ok(Self { tool })
    }

    /// The next event, if one is ready. Never waits.
    pub fn try_next_event(&self) -> Option<WaveformEvent> {
        self.tool.try_next_event()
    }
}

fn read_peaks(
    mut standard_output: impl Read,
    sender: &SyncSender<WaveformEvent>,
    wake_up: &UserInterfaceWakeUp,
    error_output_when_finished: impl FnOnce() -> String,
) {
    let mut maker = PeakMaker::default();
    let mut waiting_peaks: Vec<u8> = Vec::new();
    let mut buffer = vec![0u8; 16 * 1024];
    // A sample's two bytes can be split between two reads.
    let mut spare_byte: Option<u8> = None;
    loop {
        let read_count = match standard_output.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read_count) => read_count,
        };
        let mut bytes = buffer[..read_count].iter().copied();
        let mut samples: Vec<i16> = Vec::with_capacity(read_count / BYTES_PER_SAMPLE + 1);
        if let Some(low_byte) = spare_byte.take()
            && let Some(high_byte) = bytes.next()
        {
            samples.push(i16::from_le_bytes([low_byte, high_byte]));
        }
        let remaining: Vec<u8> = bytes.collect();
        let (pairs, odd_byte_at_the_end) = remaining.as_chunks::<BYTES_PER_SAMPLE>();
        spare_byte = odd_byte_at_the_end.first().copied();
        samples.extend(pairs.iter().map(|pair| i16::from_le_bytes(*pair)));

        maker.add_samples(samples, &mut waiting_peaks);
        while waiting_peaks.len() >= PEAKS_PER_BATCH {
            let batch: Vec<u8> = waiting_peaks.drain(..PEAKS_PER_BATCH).collect();
            if sender.send(WaveformEvent::Peaks(batch)).is_err() {
                return;
            }
            wake_up();
        }
    }
    maker.finish(&mut waiting_peaks);
    if !waiting_peaks.is_empty() && sender.send(WaveformEvent::Peaks(waiting_peaks)).is_err() {
        return;
    }
    let error_output = error_output_when_finished();
    let _ = sender.send(WaveformEvent::Finished {
        error_output: (!error_output.is_empty()).then_some(error_output),
    });
    wake_up();
}

/// The ffmpeg command line that decodes the sound of a file into the samples the peaks are made from.
pub fn waveform_arguments(path: &Path) -> Vec<OsString> {
    let mut arguments: Vec<OsString> = ["-hide_banner", "-loglevel", "error", "-nostdin", "-i"]
        .map(OsString::from)
        .to_vec();
    arguments.push(path.into());
    arguments.extend(["-vn", "-sn", "-dn", "-ac", "1", "-ar"].map(OsString::from));
    arguments.push(SAMPLE_RATE.to_string().into());
    arguments.extend(["-f", "s16le", "pipe:1"].map(OsString::from));
    arguments
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::mpsc;

    use super::*;

    fn peaks_of(samples: &[i16]) -> Vec<u8> {
        let mut maker = PeakMaker::default();
        let mut peaks = Vec::new();
        maker.add_samples(samples.iter().copied(), &mut peaks);
        maker.finish(&mut peaks);
        peaks
    }

    #[test]
    fn a_peak_is_the_loudest_sample_of_its_stretch_whether_it_is_positive_or_negative() {
        let mut samples = vec![0i16; SAMPLES_PER_PEAK * 3];
        samples[5] = 16_384; // half as loud as it gets
        samples[SAMPLES_PER_PEAK + 7] = -32_768; // as loud as it gets, downwards
        samples[SAMPLES_PER_PEAK * 2 + 1] = 100;
        samples[SAMPLES_PER_PEAK * 2 + 2] = -300;
        assert_eq!(peaks_of(&samples), vec![128, 255, 3]);
    }

    #[test]
    fn silence_has_no_height_and_the_faintest_sound_has_some() {
        assert_eq!(peaks_of(&[0i16; SAMPLES_PER_PEAK]), vec![0]);
        assert_eq!(peaks_of(&[1]), vec![1], "a single faint sample still shows");
    }

    #[test]
    fn the_loudest_possible_samples_fit_in_a_byte() {
        assert_eq!(peak_of_loudest_sample(32_768), 255);
        assert_eq!(peak_of_loudest_sample(32_767), 255);
        assert_eq!(peak_of_loudest_sample(0), 0);
    }

    #[test]
    fn a_part_of_a_peak_at_the_end_still_counts() {
        let mut samples = vec![0i16; SAMPLES_PER_PEAK];
        samples.extend([32_000, 0, 0]);
        assert_eq!(peaks_of(&samples), vec![0, 250]);
        assert_eq!(peaks_of(&[]), Vec::<u8>::new());
    }

    #[test]
    fn peaks_come_out_the_same_however_the_samples_are_cut_into_pieces() {
        let samples: Vec<i16> = (0..SAMPLES_PER_PEAK * 7 + 13)
            .map(|index| ((index * 977) % 20_000) as i16 - 10_000)
            .collect();
        let whole = peaks_of(&samples);
        for piece_length in [
            1,
            2,
            7,
            SAMPLES_PER_PEAK - 1,
            SAMPLES_PER_PEAK,
            SAMPLES_PER_PEAK + 1,
            1000,
        ] {
            let mut maker = PeakMaker::default();
            let mut peaks = Vec::new();
            for piece in samples.chunks(piece_length) {
                maker.add_samples(piece.iter().copied(), &mut peaks);
            }
            maker.finish(&mut peaks);
            assert_eq!(peaks, whole, "pieces of {piece_length}");
        }
    }

    fn read_all(bytes: &[u8], read_size: usize) -> (Vec<u8>, Option<String>) {
        /// A reader that gives at most `read_size` bytes at a time, as a pipe does.
        struct Trickle<'a>(&'a [u8], usize);
        impl Read for Trickle<'_> {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                let count = self.0.len().min(self.1).min(buffer.len());
                buffer[..count].copy_from_slice(&self.0[..count]);
                self.0 = &self.0[count..];
                Ok(count)
            }
        }
        let (sender, receiver) = mpsc::sync_channel(10_000);
        let wake_up: UserInterfaceWakeUp = Arc::new(|| {});
        read_peaks(Trickle(bytes, read_size), &sender, &wake_up, || "oops".to_owned());
        drop(sender);
        let mut peaks = Vec::new();
        let mut error = None;
        for event in receiver {
            match event {
                WaveformEvent::Peaks(batch) => peaks.extend(batch),
                WaveformEvent::Finished { error_output } => error = error_output,
            }
        }
        (peaks, error)
    }

    fn bytes_of(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|sample| sample.to_le_bytes()).collect()
    }

    #[test]
    fn the_stream_of_bytes_ffmpeg_prints_becomes_the_same_peaks_however_it_arrives() {
        let samples: Vec<i16> = (0..SAMPLES_PER_PEAK * 5 + 3)
            .map(|index| ((index * 31) % 30_000) as i16 - 15_000)
            .collect();
        let expected = peaks_of(&samples);
        let bytes = bytes_of(&samples);
        for read_size in [1, 3, 80, 161, 4096, 100_000] {
            let (peaks, error) = read_all(&bytes, read_size);
            assert_eq!(peaks, expected, "read {read_size} bytes at a time");
            assert_eq!(error.as_deref(), Some("oops"), "the tool's complaint is passed on");
        }
    }

    #[test]
    fn a_long_sound_is_delivered_in_batches() {
        let samples = vec![1000i16; SAMPLES_PER_PEAK * (PEAKS_PER_BATCH * 2 + 5)];
        let bytes = bytes_of(&samples);
        let (sender, receiver) = mpsc::sync_channel(10_000);
        let wake_up: UserInterfaceWakeUp = Arc::new(|| {});
        read_peaks(&bytes[..], &sender, &wake_up, String::new);
        drop(sender);
        let sizes: Vec<usize> = receiver
            .into_iter()
            .filter_map(|event| match event {
                WaveformEvent::Peaks(batch) => Some(batch.len()),
                WaveformEvent::Finished { .. } => None,
            })
            .collect();
        assert_eq!(sizes, vec![PEAKS_PER_BATCH, PEAKS_PER_BATCH, 5]);
    }

    #[test]
    fn a_sound_with_no_complaint_finishes_without_an_error() {
        let (sender, receiver) = mpsc::sync_channel(10);
        let wake_up: UserInterfaceWakeUp = Arc::new(|| {});
        read_peaks(&[][..], &sender, &wake_up, String::new);
        drop(sender);
        let events: Vec<WaveformEvent> = receiver.into_iter().collect();
        assert_eq!(events, vec![WaveformEvent::Finished { error_output: None }]);
    }

    fn waveform_of_ramp() -> Waveform {
        // Peak number `n` is `n`, so which peaks were looked at can be told from the answer.
        Waveform::from_peaks((0..200).collect())
    }

    #[test]
    fn the_loudest_peak_between_two_moments_is_found() {
        let waveform = waveform_of_ramp();
        assert_eq!(
            waveform.loudest_between(0.0, 0.10),
            Some(9),
            "peaks 0 to 9 are the first tenth of a second"
        );
        assert_eq!(waveform.loudest_between(0.5, 0.52), Some(51), "peaks 50 and 51");
        assert_eq!(
            waveform.loudest_between(0.0, 100.0),
            Some(199),
            "past the end is cut off"
        );
    }

    #[test]
    fn rounding_in_the_numbers_does_not_add_the_peak_that_a_stretch_only_just_does_not_reach() {
        let waveform = waveform_of_ramp();
        // 1.1 seconds is 110.00000000000001 peaks as a number, but the stretch ends where peak 110 begins.
        assert_eq!(waveform.loudest_between(1.0, 1.1), Some(109));
        assert_eq!(waveform.loudest_between(0.07, 0.30000000000000004), Some(29));
    }

    #[test]
    fn a_moment_shorter_than_a_peak_has_the_peak_it_falls_in() {
        let waveform = waveform_of_ramp();
        assert_eq!(waveform.loudest_between(0.503, 0.503), Some(50));
        assert_eq!(waveform.loudest_between(0.503, 0.5031), Some(50));
    }

    #[test]
    fn nothing_is_known_before_the_start_or_after_the_end() {
        let waveform = waveform_of_ramp();
        assert_eq!(
            waveform.loudest_between(2.0, 3.0),
            None,
            "the sound is two seconds long"
        );
        assert_eq!(waveform.loudest_between(-3.0, -1.0), None);
        assert_eq!(waveform.loudest_between(f64::NAN, 1.0), None);
        assert_eq!(waveform.loudest_between(0.0, f64::NAN), None);
        assert_eq!(Waveform::default().loudest_between(0.0, 1.0), None);
        assert_eq!(
            waveform.loudest_between(-1.0, 0.05),
            Some(4),
            "a stretch that starts before the sound has what is inside it"
        );
    }

    #[test]
    fn peaks_can_be_added_as_they_arrive() {
        let mut waveform = Waveform::default();
        waveform.extend(&[1, 2]);
        waveform.extend(&[3]);
        assert_eq!(waveform.peaks(), &[1, 2, 3]);
        assert_eq!(waveform.peak_count(), 3);
    }

    #[test]
    fn the_command_line_decodes_only_the_sound_to_mono_samples_on_standard_output() {
        let arguments: Vec<String> = waveform_arguments(Path::new("/music/song.mp3"))
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect();
        assert!(arguments.windows(2).any(|pair| pair == ["-i", "/music/song.mp3"]));
        assert!(arguments.contains(&"-vn".to_owned()), "no picture");
        assert!(arguments.windows(2).any(|pair| pair == ["-ac", "1"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-ar", "8000"]));
        assert_eq!(&arguments[arguments.len() - 3..], ["-f", "s16le", "pipe:1"]);
    }
}
