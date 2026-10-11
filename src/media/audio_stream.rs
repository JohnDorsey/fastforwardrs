//! Decoding the sound of the live preview.
//!
//! The preview's sound must be what the export will have. So a stream is told, in the editor's own
//! terms, what the sound is (the sound of every track from where the playhead is, on: see
//! [`crate::composition::plan_sound`]), and it runs the very same filter graph that the export uses
//! for its sound (see [`crate::media::export`]): every clip is cut out of its file, sped up, given
//! its volume and made exactly as long as its place on the timeline, and the tracks are mixed. What
//! comes out is raw samples, ready to be played.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::mpsc::SyncSender;

use tempfile::TempDir;
use thiserror::Error;

use super::UserInterfaceWakeUp;
use super::child_process::{StreamingTool, background_command};
use super::export::{AudioGraph, SoundGraphSpecification, build_audio_graph, filter_graph_file_option};
use super::ffmpeg_toolchain::FfmpegToolchain;

/// How many sets of samples may wait to be played before decoding pauses. The decoder is far
/// faster than playback, and without a limit it would decode the whole rest of the video at once.
const CHUNKS_BUFFERED_AHEAD: usize = 24;

/// How many moments of sound (one sample for each channel) make up one set of samples: about 43
/// milliseconds at 48000 moments per second.
const MOMENTS_PER_CHUNK: usize = 2048;

const BYTES_PER_SAMPLE: usize = size_of::<f32>();

/// How the sound is delivered: the way the speakers, or whatever else plays it, want it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioFormat {
    /// Moments of sound per second.
    pub sample_rate: u32,
    /// How many samples make up each moment: 1 for mono, 2 for stereo.
    pub channel_count: u16,
}

/// What to decode.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioStreamRequest {
    /// The files the sound is made from, which are ffmpeg's inputs 0, 1, ... in this order.
    pub source_files: Vec<PathBuf>,
    /// What the sound is: the frame rate and length, what each track sounds like, and what the
    /// sound of sped-up clips is like. Its clips name the files by their place in `source_files`.
    pub sound: SoundGraphSpecification,
    pub format: AudioFormat,
}

/// What a running stream reports.
#[derive(Debug)]
pub enum AudioStreamEvent {
    /// The next samples, interleaved by channel (left, right, left, right...). Whole moments only.
    Samples(Vec<f32>),
    /// The stream has ended: all of the sound was delivered, or ffmpeg stopped. `error_output`
    /// holds ffmpeg's complaint if it stopped because of an error.
    Finished { error_output: Option<String> },
}

#[derive(Debug, Error)]
pub enum AudioStreamError {
    #[error("there is no sound to decode: the request has no sound in it")]
    NoSound,
    #[error("ffmpeg could not be started to decode the sound: {0}")]
    CouldNotStart(#[source] io::Error),
    #[error("the sound's filter graph could not be written to a temporary file: {0}")]
    CouldNotWriteGraph(#[source] io::Error),
}

/// A running ffmpeg process that delivers decoded sound in the background. Dropping the stream
/// stops ffmpeg.
pub struct AudioStream {
    // The tool is dropped (and so stopped) before the folder that holds its graph is removed.
    tool: StreamingTool<AudioStreamEvent>,
    _graph_folder: TempDir,
}

impl AudioStream {
    /// Starts decoding. The sound is decoded a little at a time, ahead of when it is asked for;
    /// the `wake_up` callback is called when the first of it is ready and when it has ended.
    pub fn start(
        toolchain: &FfmpegToolchain,
        request: &AudioStreamRequest,
        wake_up: UserInterfaceWakeUp,
    ) -> Result<Self, AudioStreamError> {
        if request.sound.strips.is_empty() {
            return Err(AudioStreamError::NoSound);
        }
        let graph = build_audio_graph(&request.sound);
        // The graph goes in a file: for a project with many clips it is longer than a command line.
        let graph_folder = tempfile::Builder::new()
            .prefix("fastforwardrs_preview_sound_")
            .tempdir()
            .map_err(AudioStreamError::CouldNotWriteGraph)?;
        let graph_file = graph_folder.path().join("graph.txt");
        fs::write(&graph_file, &graph.text).map_err(AudioStreamError::CouldNotWriteGraph)?;

        let mut command = background_command(toolchain.ffmpeg_path());
        command.args(audio_stream_arguments(toolchain, request, &graph, &graph_file));
        let channel_count = usize::from(request.format.channel_count);
        let tool = StreamingTool::start(
            command,
            CHUNKS_BUFFERED_AHEAD,
            move |standard_output, sender, error_output_when_finished| {
                read_samples(
                    standard_output,
                    channel_count,
                    &sender,
                    &wake_up,
                    error_output_when_finished,
                );
            },
        )
        .map_err(AudioStreamError::CouldNotStart)?;
        Ok(Self {
            tool,
            _graph_folder: graph_folder,
        })
    }

    /// The next event, if one is ready. Never waits.
    pub fn try_next_event(&self) -> Option<AudioStreamEvent> {
        self.tool.try_next_event()
    }
}

/// Reads sound from `standard_output` and sends it on in sets of whole moments, until the data
/// ends. Then sends [`AudioStreamEvent::Finished`] with whatever `error_output_when_finished`
/// returns.
fn read_samples(
    mut standard_output: impl Read,
    channel_count: usize,
    sender: &SyncSender<AudioStreamEvent>,
    wake_up: &UserInterfaceWakeUp,
    error_output_when_finished: impl FnOnce() -> String,
) {
    let bytes_per_moment = channel_count * BYTES_PER_SAMPLE;
    let mut bytes = vec![0u8; MOMENTS_PER_CHUNK * bytes_per_moment];
    let mut is_first_chunk = true;
    loop {
        let byte_count = read_as_much_as_possible(&mut standard_output, &mut bytes);
        // Only whole moments are sent: a half moment at the very end would shift the channels.
        let whole_moments_byte_count = byte_count - byte_count % bytes_per_moment;
        if whole_moments_byte_count > 0 {
            let samples = bytes[..whole_moments_byte_count]
                .as_chunks::<BYTES_PER_SAMPLE>()
                .0
                .iter()
                .map(|sample| f32::from_le_bytes(*sample))
                .collect();
            // A full buffer makes this wait, which is what keeps the decoder from racing ahead. An
            // error means the stream was dropped, so nobody wants more sound.
            if sender.send(AudioStreamEvent::Samples(samples)).is_err() {
                return;
            }
            if is_first_chunk {
                wake_up();
                is_first_chunk = false;
            }
        }
        if byte_count < bytes.len() {
            break;
        }
    }
    let error_output = error_output_when_finished();
    let _ = sender.send(AudioStreamEvent::Finished {
        error_output: (!error_output.is_empty()).then_some(error_output),
    });
    wake_up();
}

/// Fills `buffer` from `reader` as far as the data goes, and says how many bytes that was: all of
/// them unless the data ended first. (A single `read` may give fewer bytes than there are.)
fn read_as_much_as_possible(reader: &mut impl Read, buffer: &mut [u8]) -> usize {
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..]) {
            Ok(0) | Err(_) => break,
            Ok(count) => filled += count,
        }
    }
    filled
}

/// The ffmpeg command line that decodes `request`, whose sound graph is `graph` and has been
/// written to `graph_file`.
pub fn audio_stream_arguments(
    toolchain: &FfmpegToolchain,
    request: &AudioStreamRequest,
    graph: &AudioGraph,
    graph_file: &Path,
) -> Vec<OsString> {
    let mut arguments: Vec<OsString> = ["-hide_banner", "-loglevel", "error", "-nostdin"]
        .map(OsString::from)
        .to_vec();
    for source_file in &request.source_files {
        arguments.push("-i".into());
        arguments.push(source_file.clone().into());
    }
    arguments.push(filter_graph_file_option(toolchain).into());
    arguments.push(graph_file.into());
    arguments.push("-map".into());
    arguments.push(format!("[{}]", graph.output_label).into());
    arguments.extend(["-f", "f32le", "-ar"].map(OsString::from));
    arguments.push(request.format.sample_rate.to_string().into());
    arguments.push("-ac".into());
    arguments.push(request.format.channel_count.to_string().into());
    arguments.push("pipe:1".into());
    arguments
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;
    use crate::media::do_nothing_when_woken;
    use crate::media::export::{SoundClipToExport, SoundItemToExport, SoundStripToExport};
    use crate::project::{FrameRate, SpedUpClipSound, SpeedFactor};

    const STEREO: usize = 2;

    /// `moments` moments of stereo sound as the bytes ffmpeg writes: sample `n` is `n as f32`.
    fn stereo_bytes(moments: usize) -> Vec<u8> {
        (0..moments * STEREO)
            .flat_map(|sample_number| (sample_number as f32).to_le_bytes())
            .collect()
    }

    fn read_everything(bytes: &[u8], channel_count: usize) -> Vec<AudioStreamEvent> {
        let (sender, receiver) = mpsc::sync_channel(1000);
        read_samples(bytes, channel_count, &sender, &do_nothing_when_woken(), String::new);
        receiver.try_iter().collect()
    }

    fn sample_counts(events: &[AudioStreamEvent]) -> Vec<usize> {
        events
            .iter()
            .filter_map(|event| match event {
                AudioStreamEvent::Samples(samples) => Some(samples.len()),
                AudioStreamEvent::Finished { .. } => None,
            })
            .collect()
    }

    #[test]
    fn sound_is_delivered_in_sets_of_whole_moments_and_then_the_stream_finishes() {
        // Two whole sets and a part of a third.
        let events = read_everything(&stereo_bytes(MOMENTS_PER_CHUNK * 2 + 100), STEREO);
        assert_eq!(
            sample_counts(&events),
            vec![MOMENTS_PER_CHUNK * STEREO, MOMENTS_PER_CHUNK * STEREO, 100 * STEREO]
        );
        assert!(matches!(
            events.last(),
            Some(AudioStreamEvent::Finished { error_output: None })
        ));
    }

    #[test]
    fn the_samples_are_the_ones_that_were_written_in_the_order_they_were_written() {
        let events = read_everything(&stereo_bytes(10), STEREO);
        let AudioStreamEvent::Samples(samples) = &events[0] else {
            panic!("expected samples first: {events:?}")
        };
        assert_eq!(samples, &(0..20).map(|number| number as f32).collect::<Vec<f32>>());
    }

    #[test]
    fn half_a_moment_at_the_very_end_is_left_out_so_the_channels_cannot_be_shifted() {
        let mut bytes = stereo_bytes(10);
        bytes.extend_from_slice(&[1, 2, 3, 4]); // one sample of the left channel, with no right one
        let events = read_everything(&bytes, STEREO);
        assert_eq!(sample_counts(&events), vec![20]);

        // And a stray byte or two that is not even a whole sample.
        let mut bytes = stereo_bytes(10);
        bytes.extend_from_slice(&[1, 2]);
        assert_eq!(sample_counts(&read_everything(&bytes, STEREO)), vec![20]);
    }

    #[test]
    fn mono_sound_is_cut_into_moments_of_one_sample() {
        let bytes: Vec<u8> = (0..5).flat_map(|number| (number as f32).to_le_bytes()).collect();
        assert_eq!(sample_counts(&read_everything(&bytes, 1)), vec![5]);
    }

    #[test]
    fn no_sound_at_all_is_just_the_end() {
        let events = read_everything(&[], STEREO);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], AudioStreamEvent::Finished { error_output: None }));
    }

    #[test]
    fn what_ffmpeg_said_goes_with_the_end_of_the_stream() {
        let (sender, receiver) = mpsc::sync_channel(10);
        read_samples(&[][..], STEREO, &sender, &do_nothing_when_woken(), || {
            "Invalid data".to_owned()
        });
        let events: Vec<AudioStreamEvent> = receiver.try_iter().collect();
        assert!(matches!(
            &events[0],
            AudioStreamEvent::Finished { error_output: Some(text) } if text == "Invalid data"
        ));
    }

    #[test]
    fn a_reader_that_gives_a_few_bytes_at_a_time_makes_the_same_sets_as_one_that_gives_all() {
        /// Gives out at most three bytes for each read.
        struct Trickle<'a>(&'a [u8]);
        impl Read for Trickle<'_> {
            fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
                let count = self.0.len().min(buffer.len()).min(3);
                buffer[..count].copy_from_slice(&self.0[..count]);
                self.0 = &self.0[count..];
                Ok(count)
            }
        }
        let bytes = stereo_bytes(MOMENTS_PER_CHUNK + 7);
        let (sender, receiver) = mpsc::sync_channel(100);
        read_samples(Trickle(&bytes), STEREO, &sender, &do_nothing_when_woken(), String::new);
        let events: Vec<AudioStreamEvent> = receiver.try_iter().collect();
        assert_eq!(sample_counts(&events), vec![MOMENTS_PER_CHUNK * STEREO, 7 * STEREO]);
    }

    #[test]
    fn nobody_listening_ends_the_reading_quietly() {
        let (sender, receiver) = mpsc::sync_channel(1);
        drop(receiver);
        // Returns instead of waiting or panicking.
        read_samples(
            &stereo_bytes(MOMENTS_PER_CHUNK * 3)[..],
            STEREO,
            &sender,
            &do_nothing_when_woken(),
            String::new,
        );
    }

    fn request() -> AudioStreamRequest {
        AudioStreamRequest {
            source_files: vec![PathBuf::from("holiday.mp4")],
            sound: SoundGraphSpecification {
                frame_rate: FrameRate::new(30, 1).unwrap(),
                total_frame_count: 90,
                strips: vec![SoundStripToExport {
                    items: vec![SoundItemToExport::Clip(SoundClipToExport {
                        source_input: 0,
                        first_source_frame: 0,
                        end_source_frame: 90,
                        speed_factor: SpeedFactor::NORMAL,
                        timeline_frame_count: 90,
                        loudness_factor: 1.0,
                    })],
                }],
                sped_up_clip_sound: SpedUpClipSound::MaintainPitch,
            },
            format: AudioFormat {
                sample_rate: 44_100,
                channel_count: 2,
            },
        }
    }

    fn as_text(arguments: Vec<OsString>) -> Vec<String> {
        arguments
            .into_iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn the_command_line_reads_the_graph_from_a_file_and_writes_raw_samples_at_the_wanted_format() {
        let toolchain =
            FfmpegToolchain::unchecked_for_tests(PathBuf::from("ffmpeg"), PathBuf::from("ffprobe"), Some(9));
        let graph = AudioGraph {
            text: String::new(),
            output_label: "finished_audio",
        };
        assert_eq!(
            as_text(audio_stream_arguments(
                &toolchain,
                &request(),
                &graph,
                Path::new("graph.txt")
            )),
            [
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-i",
                "holiday.mp4",
                "-/filter_complex",
                "graph.txt",
                "-map",
                "[finished_audio]",
                "-f",
                "f32le",
                "-ar",
                "44100",
                "-ac",
                "2",
                "pipe:1",
            ]
        );
    }

    #[test]
    fn an_older_ffmpeg_gets_the_older_name_of_the_graph_file_option() {
        let toolchain =
            FfmpegToolchain::unchecked_for_tests(PathBuf::from("ffmpeg"), PathBuf::from("ffprobe"), Some(6));
        let graph = AudioGraph {
            text: String::new(),
            output_label: "finished_audio",
        };
        let arguments = as_text(audio_stream_arguments(
            &toolchain,
            &request(),
            &graph,
            Path::new("graph.txt"),
        ));
        assert!(
            arguments.contains(&"-filter_complex_script".to_owned()),
            "{arguments:?}"
        );
    }

    #[test]
    fn every_source_file_is_an_input_in_order() {
        let toolchain =
            FfmpegToolchain::unchecked_for_tests(PathBuf::from("ffmpeg"), PathBuf::from("ffprobe"), Some(9));
        let mut several = request();
        several.source_files = ["a.mp4", "b.mp3"].map(PathBuf::from).to_vec();
        let graph = AudioGraph {
            text: String::new(),
            output_label: "finished_audio",
        };
        let arguments = as_text(audio_stream_arguments(
            &toolchain,
            &several,
            &graph,
            Path::new("graph.txt"),
        ));
        let inputs: Vec<&String> = arguments
            .windows(2)
            .filter(|pair| pair[0] == "-i")
            .map(|pair| &pair[1])
            .collect();
        assert_eq!(inputs, ["a.mp4", "b.mp3"]);
    }

    #[test]
    fn a_request_with_no_sound_is_refused_before_anything_is_started() {
        let toolchain =
            FfmpegToolchain::unchecked_for_tests(PathBuf::from("ffmpeg"), PathBuf::from("ffprobe"), Some(9));
        let mut empty = request();
        empty.sound.strips.clear();
        assert!(matches!(
            AudioStream::start(&toolchain, &empty, do_nothing_when_woken()),
            Err(AudioStreamError::NoSound)
        ));
    }
}
