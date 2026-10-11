//! Building the ffmpeg filter graph that turns a project into the finished video.
//!
//! This is pure text-building with no processes involved, so every detail of it can be checked in
//! unit tests. It is given the project already worked out into *sections* of picture and *strips*
//! of sound (see [`crate::composition`]), so it knows nothing about tracks or clips: it only turns
//! those into filters.
//!
//! **Pictures** ([`pictures`]). Each source video is first resampled to the project's constant
//! frame rate (`fps` with `round=near`), so that it has exactly one picture for every frame of the
//! timeline, however its own timestamps are stored: Matroska files round them to milliseconds, and
//! screen recordings from phones have a variable frame rate. After that a section's video is
//! simply a range of picture numbers (`trim` by frame number), of which an accelerated one keeps
//! every `speed_factor`th, starting with the first (`select`), scaled to where it is on the
//! picture. No boundary depends on a timestamp, so none can be lost or duplicated by rounding.
//! Each section is black with its layers (videos, and still pictures such as the fast-forward
//! symbol and text, each of which may be turned) laid over it (`overlay`), the one at the back
//! first, and all the sections are joined together (`concat`).
//!
//! **Sound** ([`sound`]). The sound of every source is first made into one fixed kind (48000
//! moments a second, stereo). Then for every clip the matching sound is cut out (`atrim`), sped up
//! by the same factor, given its volume, and made exactly as long as the clip's place on the
//! timeline, so that sound and picture stay in step. The clips (and the silence between them) of a
//! track are joined, and the tracks are mixed (`amix`).

mod pictures;
mod sound;

use crate::project::{FrameRate, SpedUpClipSound};

pub use pictures::{LayerContentToExport, LayerToExport, PlaceToExport, SectionToExport, VideoToExport};
pub use sound::{SoundClipToExport, SoundItemToExport, SoundStripToExport};

/// The name of the video the graph makes.
const VIDEO_OUTPUT_LABEL: &str = "finished_video";
/// The name of the sound the graph makes.
const AUDIO_OUTPUT_LABEL: &str = "finished_audio";

/// How many digits after the decimal point are written for times and tempos: far more than the
/// microsecond precision that matters.
const DECIMAL_PLACES: usize = 9;

/// What the picture of the graph is made from.
#[derive(Clone, Debug)]
pub struct PictureGraphSpecification {
    pub frame_rate: FrameRate,
    /// The size of the finished picture. Both numbers should be even: video encoders for most
    /// formats cannot make odd sizes.
    pub picture_width_in_pixels: u32,
    pub picture_height_in_pixels: u32,
    /// The stretches of the picture, in order. (The source files are ffmpeg inputs 0, 1, ..., and
    /// the still pictures to lay over the picture are the inputs after these.)
    pub sections: Vec<SectionToExport>,
}

/// What the sound of the graph is made from.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundGraphSpecification {
    pub frame_rate: FrameRate,
    /// How long the sound is, in frames. The sound of every track is made this long.
    pub total_frame_count: u64,
    /// The sound of each track.
    pub strips: Vec<SoundStripToExport>,
    pub sped_up_clip_sound: SpedUpClipSound,
}

/// Everything the graph is built from.
#[derive(Clone, Debug)]
pub struct FilterGraphSpecification {
    pub pictures: PictureGraphSpecification,
    pub sound: SoundGraphSpecification,
}

/// The finished graph and the names of its outputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterGraph {
    pub text: String,
    pub video_output_label: &'static str,
    /// `None` when the project has no sound.
    pub audio_output_label: Option<&'static str>,
}

/// A filter graph that makes only sound, for when only the sound is wanted, such as the live
/// preview's. The export's graph has the very same sound in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioGraph {
    pub text: String,
    pub output_label: &'static str,
}

/// Builds the filter graph of the sound alone.
pub fn build_audio_graph(specification: &SoundGraphSpecification) -> AudioGraph {
    AudioGraph {
        text: sound::sound_statements(specification).join(";"),
        output_label: AUDIO_OUTPUT_LABEL,
    }
}

/// Builds the filter graph.
pub fn build_filter_graph(specification: &FilterGraphSpecification) -> FilterGraph {
    let mut statements = pictures::picture_statements(&specification.pictures);
    let audio_output_label = (!specification.sound.strips.is_empty()).then(|| {
        statements.extend(sound::sound_statements(&specification.sound));
        AUDIO_OUTPUT_LABEL
    });
    FilterGraph {
        text: statements.join(";"),
        video_output_label: VIDEO_OUTPUT_LABEL,
        audio_output_label,
    }
}

/// A `setpts` filter that gives each picture the timestamp its position calls for: picture
/// number `N` at exactly `N` frames after the first.
fn renumber_pictures_filter(frame_rate: FrameRate) -> String {
    format!("setpts=N*{}/({}*TB)", frame_rate.denominator(), frame_rate.numerator())
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

    fn picture_section(first: u64, end: u64, factor: u32) -> SectionToExport {
        SectionToExport {
            frame_count: (end - first).div_ceil(u64::from(factor)),
            layers: vec![LayerToExport {
                content: LayerContentToExport::Video(VideoToExport {
                    source_input: 0,
                    first_source_frame: first,
                    end_source_frame: end,
                    speed_factor: speed(factor),
                }),
                place: PlaceToExport {
                    left_in_pixels: 0,
                    top_in_pixels: 0,
                    width_in_pixels: 320,
                    height_in_pixels: 180,
                    turn_in_degrees: 0.0,
                },
            }],
        }
    }

    fn sound_clip(first: u64, end: u64, factor: u32) -> SoundItemToExport {
        SoundItemToExport::Clip(SoundClipToExport {
            source_input: 0,
            first_source_frame: first,
            end_source_frame: end,
            speed_factor: speed(factor),
            timeline_frame_count: (end - first).div_ceil(u64::from(factor)),
            loudness_factor: 1.0,
        })
    }

    fn specification(
        clips: &[(u64, u64, u32)],
        with_sound: bool,
        sped_up_clip_sound: SpedUpClipSound,
    ) -> FilterGraphSpecification {
        let frame_rate = FrameRate::new(30, 1).unwrap();
        let total_frame_count = clips
            .iter()
            .map(|&(first, end, factor)| (end - first).div_ceil(u64::from(factor)))
            .sum();
        FilterGraphSpecification {
            pictures: PictureGraphSpecification {
                frame_rate,
                picture_width_in_pixels: 320,
                picture_height_in_pixels: 180,
                sections: clips
                    .iter()
                    .map(|&(first, end, factor)| picture_section(first, end, factor))
                    .collect(),
            },
            sound: SoundGraphSpecification {
                frame_rate,
                total_frame_count,
                strips: if with_sound {
                    vec![SoundStripToExport {
                        items: clips
                            .iter()
                            .map(|&(first, end, factor)| sound_clip(first, end, factor))
                            .collect(),
                    }]
                } else {
                    Vec::new()
                },
                sped_up_clip_sound,
            },
        }
    }

    #[test]
    fn the_graph_of_a_project_with_no_sound_has_no_sound_in_it_at_all() {
        let graph = build_filter_graph(&specification(
            &[(0, 150, 1), (150, 330, 3)],
            false,
            SpedUpClipSound::MaintainPitch,
        ));
        assert_eq!(graph.audio_output_label, None);
        assert_eq!(graph.video_output_label, "finished_video");
        assert!(!graph.text.contains("atrim"), "{}", graph.text);
        assert!(!graph.text.contains("[0:a"), "{}", graph.text);
    }

    #[test]
    fn the_graph_of_a_project_with_sound_has_the_sound_after_the_pictures() {
        let graph = build_filter_graph(&specification(
            &[(0, 150, 1), (150, 330, 3)],
            true,
            SpedUpClipSound::MaintainPitch,
        ));
        assert_eq!(graph.audio_output_label, Some("finished_audio"));
        let video_end = graph.text.find("[finished_video]").unwrap();
        let sound_start = graph.text.find("[0:a:0]").unwrap();
        assert!(video_end < sound_start, "{}", graph.text);
    }

    #[test]
    fn the_sound_graph_on_its_own_is_exactly_the_sound_part_of_the_whole_graph() {
        for sound in SpedUpClipSound::ALL {
            let clips = [(0, 150, 1), (150, 330, 3), (330, 331, 40), (331, 600, 1)];
            let whole = specification(&clips, true, sound);
            let whole_graph = build_filter_graph(&whole);
            // The pictures take a statement to resample and split, one for each section, and two to
            // join and renumber; all the rest is sound.
            let video_statement_count = 1 + clips.len() + 2;
            let whole_sound = whole_graph
                .text
                .split(';')
                .skip(video_statement_count)
                .collect::<Vec<_>>()
                .join(";");

            let sound_only = build_audio_graph(&whole.sound);

            assert_eq!(sound_only.text, whole_sound, "{sound:?}");
            assert_eq!(Some(sound_only.output_label), whole_graph.audio_output_label);
        }
    }

    #[test]
    fn a_hundred_and_fifty_clips_make_a_graph_far_too_long_for_a_command_line() {
        // This is why the graph is handed to ffmpeg in a file: Windows limits a command line to
        // about 32,000 characters.
        let clips: Vec<(u64, u64, u32)> = (0..150)
            .map(|number| (number * 100, number * 100 + 100, 1 + (number % 5) as u32))
            .collect();
        let graph = build_filter_graph(&specification(&clips, true, SpedUpClipSound::MaintainPitch)).text;
        assert!(graph.len() > 32_000, "only {} characters", graph.len());
    }

    #[test]
    fn times_and_tempos_are_written_as_plain_decimal_numbers() {
        assert_eq!(format_seconds(1.5), "1.500000000");
        assert_eq!(format_number(100.0), "100");
        assert_eq!(format_number(2.5), "2.5");
        assert_eq!(format_number(0.0), "0");
    }
}
