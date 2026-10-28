// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Building the ffmpeg filter graph of the **sound** of a project: the sound of the finished video, and the sound that the live preview plays, which are made by one
//! function ([`build_audio_graph`]) from one specification, so that they are the same. (The picture of the finished video is made by the picture engine: see
//! `video_export`.)
//!
//! This is pure text-building with no processes involved, so every detail of it can be checked in unit tests. It is given the project already worked out into *strips*
//! of sound (see [`crate::composition`]), so it knows nothing about tracks or clips: it only turns those into filters.
//!
//! The sound of every source is first made into one fixed kind (48000 moments a second, stereo). Then for every clip the matching sound is cut out (`atrim`), sped up
//! by the same factor, given its volume, and made exactly as long as the clip's place on the timeline, so that sound and picture stay in step. The clips (and the
//! silence between them) of a track are joined, and the tracks are mixed (`amix`).

mod sound;

use crate::project::{FrameRate, SpedUpClipSound};

pub use sound::{
    LoudnessToExport, SAMPLE_RATE, SoundBusToExport, SoundClipToExport, SoundGroupToExport, SoundItemToExport,
    SoundStripToExport, clip_feed_filters, pre_roll_frames_of,
};

/// The ffmpeg option that reads a filter graph from the file named after it. A graph for a project with many clips is far longer than a command line allows, so it
/// is always given in a file.
pub(crate) const FILTER_GRAPH_FILE_OPTION: &str = "-/filter_complex";

/// The name of the sound the graph makes.
const AUDIO_OUTPUT_LABEL: &str = "finished_audio";

/// How many digits after the decimal point are written for times and tempos: far more than the
/// microsecond precision that matters.
const DECIMAL_PLACES: usize = 9;

/// What the sound of the graph is made from.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundGraphSpecification {
    pub frame_rate: FrameRate,
    /// How long the sound is, in frames. The sound of every track is made this long.
    pub total_frame_count: u64,
    /// The sound of each track.
    pub strips: Vec<SoundStripToExport>,
    /// The clip groups and track groups that have sound effects (phase 12, piece 12.3): where their members' sound is added together and given to the effects. Empty: every clip goes
    /// into the sum of all the sound.
    pub groups: Vec<SoundGroupToExport>,
    /// How long, in thousandths of a second, the fade is where a ring of sound leaves a group, goes into one, or ends at an edge (the setting `EdgeFade`: see `crate::composition::TailRoute`).
    pub edge_fade_milliseconds: u32,
    pub sped_up_clip_sound: SpedUpClipSound,
    /// How many frames at the start of the sound are **made and not heard** (phase 12, piece 12.2d): the sound begins that many frames before the place it is wanted from, so that effects that
    /// remember (a reverb, a filter) have been given what they would have heard on the way there, and what comes out of the graph is the sound from where it is wanted. They are part
    /// of `total_frame_count`; the filter graph of ffmpeg, which does not run the effects, cuts them off the front. 0 for sound that is wanted from its start.
    pub lead_in_frames: u64,
}

impl SoundGraphSpecification {
    /// The same, with the fade at the edges of groups that the setting says.
    #[must_use]
    pub fn with_the_edge_fade(mut self, fade: crate::project::EdgeFade) -> Self {
        self.edge_fade_milliseconds = fade.milliseconds();
        self
    }
}

/// A filter graph that makes only sound: the sound of the finished video and of the live preview.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioGraph {
    pub text: String,
    pub output_label: &'static str,
}

/// Builds the filter graph of the sound.
pub fn build_audio_graph(specification: &SoundGraphSpecification) -> AudioGraph {
    AudioGraph {
        text: sound::sound_statements(specification).join(";"),
        output_label: AUDIO_OUTPUT_LABEL,
    }
}

/// Seconds as ffmpeg wants them: a plain decimal number.
fn format_seconds(seconds: f64) -> String {
    format!("{seconds:.DECIMAL_PLACES$}")
}

/// A number with no trailing zeros: `100`, `2.5`.
fn format_number(number: f64) -> String {
    let text = format!("{number:.DECIMAL_PLACES$}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn numbered_labels(prefix: &str, count: usize) -> Vec<String> {
    (0..count).map(|number| format!("{prefix}_{number}")).collect()
}

/// One statement of a filter graph: `[input][labels]filter,chain[output][labels]`.
fn statement(input_labels: &[&str], filters: &str, output_labels: &[impl AsRef<str>]) -> String {
    let inputs: String = input_labels.iter().map(|label| format!("[{label}]")).collect();
    let outputs: String = output_labels
        .iter()
        .map(|label| format!("[{}]", label.as_ref()))
        .collect();
    format!("{inputs}{filters}{outputs}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::SpeedFactor;

    fn speed(factor: u32) -> SpeedFactor {
        SpeedFactor::new(factor).unwrap()
    }

    fn sound_clip(first: u64, end: u64, factor: u32) -> SoundItemToExport {
        SoundItemToExport::Clip(SoundClipToExport {
            source_input: 0,
            first_source_frame: first,
            end_source_frame: end,
            speed_factor: speed(factor),
            timeline_frame_count: (end - first).div_ceil(u64::from(factor)),
            loudness: LoudnessToExport::UNCHANGED,
            first_timeline_frame: 0,
            frames_of_the_clip_before: 0,
            effects: Vec::new(),
            sent_to: None,
            tail_routes: Vec::new(),
            music: None,
        })
    }

    fn specification(clips: &[(u64, u64, u32)], sped_up_clip_sound: SpedUpClipSound) -> SoundGraphSpecification {
        SoundGraphSpecification {
            frame_rate: FrameRate::new(30, 1).unwrap(),
            total_frame_count: clips
                .iter()
                .map(|&(first, end, factor)| (end - first).div_ceil(u64::from(factor)))
                .sum(),
            strips: vec![SoundStripToExport {
                items: clips
                    .iter()
                    .map(|&(first, end, factor)| sound_clip(first, end, factor))
                    .collect(),
                buses: Vec::new(),
                loudness: Default::default(),
                muted: Vec::new(),
            }],
            sped_up_clip_sound,
            lead_in_frames: 0,
            groups: Vec::new(),
            edge_fade_milliseconds: 10,
        }
    }

    #[test]
    fn the_graph_of_the_sound_has_the_name_of_its_output_and_cuts_each_clip_out_of_its_file() {
        let graph = build_audio_graph(&specification(
            &[(0, 150, 1), (150, 330, 3)],
            SpedUpClipSound::MaintainPitch,
        ));
        assert_eq!(graph.output_label, "finished_audio");
        assert!(graph.text.contains("[0:a:0]"), "{}", graph.text);
        assert!(graph.text.contains("atrim"), "{}", graph.text);
        assert!(graph.text.ends_with("[finished_audio]"), "{}", graph.text);
    }

    #[test]
    fn a_hundred_and_fifty_clips_make_a_graph_far_too_long_for_a_command_line() {
        // This is why the graph is handed to ffmpeg in a file: Windows limits a command line to
        // about 32,000 characters.
        let clips: Vec<(u64, u64, u32)> = (0..150)
            .map(|number| (number * 100, number * 100 + 100, 1 + (number % 5) as u32))
            .collect();
        let graph = build_audio_graph(&specification(&clips, SpedUpClipSound::MaintainPitch)).text;
        assert!(graph.len() > 32_000, "only {} characters", graph.len());
    }

    #[test]
    fn times_and_tempos_are_written_as_plain_decimal_numbers() {
        assert_eq!(format_seconds(1.5), "1.500000000");
        assert_eq!(format_number(100.0), "100");
        assert_eq!(format_number(2.5), "2.5");
        assert_eq!(format_number(0.0), "0");
    }

    #[test]
    fn the_graph_is_read_from_a_file_with_the_option_of_ffmpeg_seven_and_newer() {
        assert_eq!(FILTER_GRAPH_FILE_OPTION, "-/filter_complex");
    }
}
