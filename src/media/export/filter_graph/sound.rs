//! The sound half of the filter graph: each clip cut out of its file and made to fit, the clips of a
//! track joined with silence between, and the tracks mixed together.

use std::collections::BTreeMap;

use super::{AUDIO_OUTPUT_LABEL, SoundGraphSpecification, format_number, format_seconds, numbered_labels, statement};
use crate::project::{FrameRate, SpedUpClipSound, SpeedFactor};

/// How many moments of sound a second the sound is made into before it is cut up. Everything
/// after that can then rely on it, and sound from different sources can be joined together.
const SAMPLE_RATE: u32 = 48_000;

/// The kind of sound everything is made into: 32-bit floating point, [`SAMPLE_RATE`] moments a
/// second, two channels (a mono source is played on both, a source with more channels is mixed
/// down to two).
const SAMPLE_FORMAT_FILTER: &str = "aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo";

/// The fastest a clip can be played and still have its pitch raised. `asetrate` takes the new
/// rate as a whole number of at most about two billion, and at a speed like this all of what
/// anybody can hear has been moved out of the range of hearing anyway: such a clip is silent.
const LARGEST_SPEED_THAT_RAISES_THE_PITCH: u32 = 1000;

/// The highest speed a single `atempo` filter accepts.
const LARGEST_AUDIO_TEMPO_PER_FILTER: f64 = 100.0;

/// A clip whose sped-up sound would be shorter than this many seconds gets silence instead.
///
/// `atempo` works in pieces of about 21 milliseconds, and measurements with ffmpeg 9 show that it
/// produces no sound at all when the sped-up result would be shorter than that. A sound stream
/// with nothing in it then breaks the joining of the clips ("Invalid data found when processing
/// input"). A blip of sound under a tenth of a second could not be made out anyway, so it is
/// replaced by silence of the same length, which is always safe.
const SHORTEST_SPED_UP_SOUND_IN_SECONDS: f64 = 0.1;

/// How loud the sound of a clip is made.
#[derive(Clone, Debug, PartialEq)]
pub enum LoudnessToExport {
    /// The number the samples are multiplied by, the same all the way through.
    Constant(f64),
    /// Louder at some frames than at others: how loud, in decibels, at each of some frames counted from the start of the
    /// clip, between which it changes evenly (in decibels), and before the first and after the last as at that one.
    /// There are at least two.
    Changing(Vec<(u64, f32)>),
}

impl LoudnessToExport {
    /// The sound as it is: nothing is done to it.
    pub const UNCHANGED: Self = Self::Constant(1.0);
}

/// A stretch of the sound of one source file.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundClipToExport {
    /// Which of the graph's inputs the file is.
    pub source_input: usize,
    pub first_source_frame: u64,
    /// The frame just after the last source frame.
    pub end_source_frame: u64,
    pub speed_factor: SpeedFactor,
    /// How many frames of the timeline the clip lasts, which is how long its sound is made.
    pub timeline_frame_count: u64,
    pub loudness: LoudnessToExport,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SoundItemToExport {
    Silence { frame_count: u64 },
    Clip(SoundClipToExport),
}

impl SoundItemToExport {
    fn timeline_frame_count(&self) -> u64 {
        match self {
            Self::Silence { frame_count } => *frame_count,
            Self::Clip(clip) => clip.timeline_frame_count,
        }
    }
}

/// The sound of one track: its items one after another.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundStripToExport {
    pub items: Vec<SoundItemToExport>,
}

impl SoundStripToExport {
    fn timeline_frame_count(&self) -> u64 {
        self.items.iter().map(SoundItemToExport::timeline_frame_count).sum()
    }
}

pub(super) fn sound_statements(specification: &SoundGraphSpecification) -> Vec<String> {
    let frame_rate = specification.frame_rate;
    let total_length = format_seconds(frame_rate.seconds_at_frame(specification.total_frame_count));
    let mut statements = Vec::new();

    // Each source file is made into the one kind of sound once, and split into a copy for each
    // clip that uses it. The copies are numbered in the order the clips come in the strips.
    let mut clip_numbers_using_each_source: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut next_clip_number = 0;
    for item in specification.strips.iter().flat_map(|strip| &strip.items) {
        if let SoundItemToExport::Clip(clip) = item {
            clip_numbers_using_each_source
                .entry(clip.source_input)
                .or_default()
                .push(next_clip_number);
            next_clip_number += 1;
        }
    }
    for (source_input, clip_numbers) in &clip_numbers_using_each_source {
        let copy_labels: Vec<String> = clip_numbers
            .iter()
            .map(|clip_number| format!("clip_source_audio_{clip_number}"))
            .collect();
        statements.push(statement(
            &[&format!("{source_input}:a:0")],
            &format!("{SAMPLE_FORMAT_FILTER},asplit={}", clip_numbers.len()),
            &copy_labels,
        ));
    }

    // Each strip: its clips made to fit and joined with the silences between.
    let strip_count = specification.strips.len();
    let mut clip_number = 0;
    let mut strip_labels = Vec::with_capacity(strip_count);
    let mut strip_is_as_long_as_the_sound = Vec::with_capacity(strip_count);
    for (strip_number, strip) in specification.strips.iter().enumerate() {
        let item_labels = numbered_labels(&format!("strip_{strip_number}_item"), strip.items.len());
        for (item, item_label) in strip.items.iter().zip(&item_labels) {
            match item {
                SoundItemToExport::Silence { frame_count } => {
                    let length = format_seconds(frame_rate.seconds_at_frame(*frame_count));
                    statements.push(statement(
                        &[],
                        &format!(
                            "anullsrc=r={SAMPLE_RATE}:cl=stereo,{SAMPLE_FORMAT_FILTER},atrim=end={length},asetpts=PTS-STARTPTS"
                        ),
                        &[item_label],
                    ));
                }
                SoundItemToExport::Clip(clip) => {
                    statements.extend(clip_statements(
                        frame_rate,
                        clip,
                        clip_number,
                        item_label,
                        specification.sped_up_clip_sound,
                    ));
                    clip_number += 1;
                }
            }
        }
        let is_as_long = strip.timeline_frame_count() == specification.total_frame_count;
        // A strip that is all there is, and as long as the sound is, needs nothing more done to it.
        let label = if strip_count == 1 && is_as_long {
            AUDIO_OUTPUT_LABEL.to_owned()
        } else {
            format!("strip_{strip_number}")
        };
        statements.push(statement(
            &item_labels.iter().map(String::as_str).collect::<Vec<_>>(),
            &format!("concat=n={}:v=0:a=1", item_labels.len()),
            &[&label],
        ));
        strip_labels.push(label);
        strip_is_as_long_as_the_sound.push(is_as_long);
    }

    if strip_count == 1 && strip_is_as_long_as_the_sound[0] {
        return statements;
    }
    // Every strip is made as long as the sound, so that they end together, and then they are mixed.
    // (Mixing divides each by the number of strips, which the volume afterwards puts back.)
    let mut padded_labels = Vec::with_capacity(strip_count);
    for (strip_number, label) in strip_labels.iter().enumerate() {
        let padded_label = if strip_count == 1 {
            AUDIO_OUTPUT_LABEL.to_owned()
        } else {
            format!("padded_strip_{strip_number}")
        };
        statements.push(statement(
            &[label],
            &format!("apad=whole_dur={total_length},atrim=end={total_length},asetpts=PTS-STARTPTS"),
            &[&padded_label],
        ));
        padded_labels.push(padded_label);
    }
    if strip_count > 1 {
        statements.push(statement(
            &padded_labels.iter().map(String::as_str).collect::<Vec<_>>(),
            &format!("amix=inputs={strip_count}:duration=longest:dropout_transition=0,volume={strip_count}"),
            &[AUDIO_OUTPUT_LABEL],
        ));
    }
    statements
}

/// The statements that make the sound of `clip` (the `clip_number`th clip of the project, whose
/// source's sound is `clip_source_audio_<clip_number>`) into the item `item_label`: the sound cut out
/// of its file and changed as the clip says, laid on silence of exactly the length of the clip, so
/// that it is that long whether the file has more than enough sound, too little, or none at all.
///
/// The silence is a source of its own that the sound is mixed into, rather than the sound being
/// padded with `apad`, because `apad` gives frames *without timestamps* when what it pads has no
/// frames at all (a clip that is wholly past the end of its file's sound, which happens when a
/// project is pointed at a shorter file), and the sound after such a clip comes out at the wrong
/// time. (Measured with ffmpeg 9.0.2: the finished file's sound ended at the start of such a clip.)
/// Mixing is exact: the silence adds nothing, and the volume afterwards makes up for the mix
/// halving the volume of each input.
fn clip_statements(
    frame_rate: FrameRate,
    clip: &SoundClipToExport,
    clip_number: usize,
    item_label: &str,
    sped_up_clip_sound: SpedUpClipSound,
) -> Vec<String> {
    let exact_length = format_seconds(frame_rate.seconds_at_frame(clip.timeline_frame_count));
    let cut_label = format!("clip_cut_audio_{clip_number}");
    let silence_label = format!("clip_silence_{clip_number}");
    vec![
        statement(
            &[&format!("clip_source_audio_{clip_number}")],
            &clip_filters(frame_rate, clip, sped_up_clip_sound),
            &[&cut_label],
        ),
        statement(
            &[],
            &format!(
                "anullsrc=r={SAMPLE_RATE}:cl=stereo,{SAMPLE_FORMAT_FILTER},atrim=end={exact_length},asetpts=PTS-STARTPTS"
            ),
            &[&silence_label],
        ),
        statement(
            &[&silence_label, &cut_label],
            "amix=inputs=2:duration=first:dropout_transition=0,volume=2,asetpts=PTS-STARTPTS",
            &[item_label],
        ),
    ]
}

/// The filters that cut the sound of `clip` out of its file and change it as the clip says.
fn clip_filters(frame_rate: FrameRate, clip: &SoundClipToExport, sped_up_clip_sound: SpedUpClipSound) -> String {
    let (sound_start, sound_end) = audio_cut_times(frame_rate, clip);
    let mut filters = format!("atrim=start={sound_start}:end={sound_end},asetpts=PTS-STARTPTS");
    filters.push_str(&treatment_of(frame_rate, clip, sped_up_clip_sound).filters());
    match &clip.loudness {
        LoudnessToExport::Constant(factor) if *factor == 1.0 => {}
        LoudnessToExport::Constant(factor) => filters.push_str(&format!(",volume={}", format_number(*factor))),
        // The volume is worked out again for every piece of the sound that goes through, from the time it is at (the
        // time into the clip: the sound has been made as long as the clip's place on the timeline by now).
        LoudnessToExport::Changing(points) => filters.push_str(&format!(
            ",volume='pow(10,({})/20)':eval=frame",
            decibels_expression(frame_rate, points)
        )),
    }
    filters
}

/// The expression, in the time `t` in seconds, for how loud in decibels a clip is made at that time: between two of the
/// `points` (frames, and decibels at them) it changes evenly from what the first says to what the second says, and before
/// the first and after the last it is what that one says.
fn decibels_expression(frame_rate: FrameRate, points: &[(u64, f32)]) -> String {
    let times_and_decibels: Vec<(f64, f64)> = points
        .iter()
        .map(|&(frame, decibels)| (frame_rate.seconds_at_frame(frame), f64::from(decibels)))
        .collect();
    let (_, last_decibels) = *times_and_decibels.last().expect("there are points");
    let mut expression = format_number(last_decibels);
    for pair in times_and_decibels.windows(2).rev() {
        let ((from_time, from_decibels), (to_time, to_decibels)) = (pair[0], pair[1]);
        let decibels_a_second = (to_decibels - from_decibels) / (to_time - from_time);
        expression = format!(
            "if(lt(t,{}),{}+({})*(t-{}),{expression})",
            format_number(to_time),
            format_number(from_decibels),
            format_number(decibels_a_second),
            format_number(from_time)
        );
    }
    expression
}

/// What is done to the sound of a clip, after it has been cut out, to make it fit the clip.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ClipSoundTreatment {
    /// Nothing: the clip plays at normal speed.
    None,
    /// Silence of the right length: the sound is made inaudible, and the silence it is mixed into
    /// makes it exactly as long as the clip's pictures.
    Silence,
    /// Played faster with its pitch kept: the `atempo` filters that together make the speed.
    FasterAtTheSamePitch { tempos: Vec<String> },
    /// Played faster like a tape, so that the pitch rises with the speed.
    FasterWithHigherPitch { speed_factor: u32 },
}

impl ClipSoundTreatment {
    /// The filters that do it, each starting with a comma.
    fn filters(&self) -> String {
        match self {
            Self::None => String::new(),
            Self::Silence => ",volume=0".to_owned(),
            Self::FasterAtTheSamePitch { tempos } => tempos.iter().map(|tempo| format!(",atempo={tempo}")).collect(),
            Self::FasterWithHigherPitch { speed_factor } => {
                // Saying that the sound has a higher sample rate than it has is what speeds it up
                // (and raises its pitch); converting back to the fixed rate then drops the extra.
                format!(
                    ",asetrate={},aresample={SAMPLE_RATE}",
                    u64::from(SAMPLE_RATE) * u64::from(*speed_factor)
                )
            }
        }
    }
}

/// What to do with the sound of `clip`, given what the sound of a sped-up clip is to be like.
fn treatment_of(
    frame_rate: FrameRate,
    clip: &SoundClipToExport,
    sped_up_clip_sound: SpedUpClipSound,
) -> ClipSoundTreatment {
    if !clip.speed_factor.is_accelerated() {
        return ClipSoundTreatment::None;
    }
    if sped_up_clip_sound == SpedUpClipSound::Mute || is_too_short_to_speed_up(frame_rate, clip) {
        return ClipSoundTreatment::Silence;
    }
    match sped_up_clip_sound {
        SpedUpClipSound::Mute => ClipSoundTreatment::Silence,
        SpedUpClipSound::MaintainPitch => ClipSoundTreatment::FasterAtTheSamePitch {
            tempos: audio_tempo_filters(clip.speed_factor),
        },
        SpedUpClipSound::IncreasePitch if clip.speed_factor.value() <= LARGEST_SPEED_THAT_RAISES_THE_PITCH => {
            ClipSoundTreatment::FasterWithHigherPitch {
                speed_factor: clip.speed_factor.value(),
            }
        }
        SpedUpClipSound::IncreasePitch => ClipSoundTreatment::Silence,
    }
}

/// The times between which the sound of `clip` is cut out: exactly from the start of its first
/// frame to the start of the frame after its last (sound has no frames to lose or duplicate).
fn audio_cut_times(frame_rate: FrameRate, clip: &SoundClipToExport) -> (String, String) {
    (
        format_seconds(frame_rate.seconds_at_frame(clip.first_source_frame)),
        format_seconds(frame_rate.seconds_at_frame(clip.end_source_frame)),
    )
}

/// Whether `clip` is accelerated but so short that its sped-up sound would be under
/// [`SHORTEST_SPED_UP_SOUND_IN_SECONDS`].
fn is_too_short_to_speed_up(frame_rate: FrameRate, clip: &SoundClipToExport) -> bool {
    let source_seconds = frame_rate.seconds_at_frame(clip.end_source_frame - clip.first_source_frame);
    let sped_up_seconds = source_seconds / f64::from(clip.speed_factor.value());
    clip.speed_factor.is_accelerated() && sped_up_seconds < SHORTEST_SPED_UP_SOUND_IN_SECONDS
}

/// The `atempo` settings that together speed sound up by `speed_factor`. A single `atempo` filter
/// cannot go beyond [`LARGEST_AUDIO_TEMPO_PER_FILTER`], so a larger speed-up is made from several
/// in a row (250x is 100 followed by 2.5). Normal speed needs none.
fn audio_tempo_filters(speed_factor: SpeedFactor) -> Vec<String> {
    let mut tempos = Vec::new();
    let mut still_to_apply = f64::from(speed_factor.value());
    while still_to_apply > LARGEST_AUDIO_TEMPO_PER_FILTER {
        tempos.push(format_number(LARGEST_AUDIO_TEMPO_PER_FILTER));
        still_to_apply /= LARGEST_AUDIO_TEMPO_PER_FILTER;
    }
    if still_to_apply > 1.0 {
        tempos.push(format_number(still_to_apply));
    }
    tempos
}

#[cfg(test)]
mod tests;
