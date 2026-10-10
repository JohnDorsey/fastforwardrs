//! Building the ffmpeg filter graph that turns a project into the finished video.
//!
//! This is pure text-building with no processes involved, so every detail of it can be checked in
//! unit tests.
//!
//! **Video.** The whole source is first resampled to the project's constant frame rate (`fps` with
//! `round=near`), so that it has exactly one picture for every frame of the timeline, however its
//! own timestamps are stored: Matroska files round them to milliseconds, and screen recordings
//! from phones have a variable frame rate. After that a clip is simply a range of picture
//! numbers (`trim` by frame number), and an accelerated clip keeps every `speed_factor`th picture
//! of it, starting with the first (`select`). No clip boundary depends on a timestamp, so none
//! can be lost or duplicated by rounding. For an accelerated clip the fast-forward symbol is laid
//! over the picture (`overlay`).
//!
//! **Audio.** For every clip the matching sound is cut out (`atrim`), sped up by the same factor
//! (`atempo`), and made exactly as long as the clip's pictures, so that sound and picture stay in
//! step across every clip.
//!
//! Finally all the clips are joined together (`concat`).

use std::collections::BTreeMap;

use crate::project::{FrameRate, SpeedFactor};

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

/// How many digits after the decimal point are written for times and tempos: far more than the
/// microsecond precision that matters.
const DECIMAL_PLACES: usize = 9;

const JOINED_VIDEO_LABEL: &str = "joined_video";
const VIDEO_OUTPUT_LABEL: &str = "finished_video";
const AUDIO_OUTPUT_LABEL: &str = "finished_audio";

/// What the graph needs to know about one clip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipToExport {
    pub first_source_frame: u64,
    /// The frame just after the clip's last frame.
    pub end_source_frame: u64,
    pub speed_factor: SpeedFactor,
}

impl ClipToExport {
    /// How many frames the clip occupies in the finished video.
    pub fn timeline_frame_count(&self) -> u64 {
        (self.end_source_frame - self.first_source_frame).div_ceil(u64::from(self.speed_factor.value()))
    }
}

/// Where the fast-forward symbol of one speed goes on the finished picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndicatorOverlay {
    pub left_in_pixels: u32,
    pub top_in_pixels: u32,
}

/// Everything the graph is built from.
#[derive(Clone, Debug)]
pub struct FilterGraphSpecification {
    pub frame_rate: FrameRate,
    pub clips: Vec<ClipToExport>,
    /// The size of the finished picture. Both numbers should be even: video encoders for most
    /// formats cannot make odd sizes.
    pub picture_width_in_pixels: u32,
    pub picture_height_in_pixels: u32,
    /// Whether the source video has sound to cut up and speed up.
    pub source_has_audio: bool,
    /// For each speed factor that shows the symbol: where it goes. The symbol pictures are given
    /// to ffmpeg as inputs 1, 2, ... in the order of this map (input 0 is the source video).
    pub indicator_overlays: BTreeMap<SpeedFactor, IndicatorOverlay>,
}

/// The finished graph and the names of its outputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterGraph {
    pub text: String,
    pub video_output_label: &'static str,
    /// `None` when the project has no sound.
    pub audio_output_label: Option<&'static str>,
}

/// Builds the filter graph. The source video is ffmpeg input 0; the symbol pictures follow as
/// inputs 1, 2, ... in the order of `specification.indicator_overlays`.
pub fn build_filter_graph(specification: &FilterGraphSpecification) -> FilterGraph {
    let mut statements = video_statements(specification);
    let audio_output_label = specification.source_has_audio.then(|| {
        statements.extend(audio_statements(specification));
        AUDIO_OUTPUT_LABEL
    });
    FilterGraph {
        text: statements.join(";"),
        video_output_label: VIDEO_OUTPUT_LABEL,
        audio_output_label,
    }
}

fn video_statements(specification: &FilterGraphSpecification) -> Vec<String> {
    let frame_rate = specification.frame_rate;
    let clip_count = specification.clips.len();
    let source_labels = numbered_labels("clip_source_video", clip_count);
    // Scale first and resample second: resampling a variable frame rate video repeats pictures to
    // fill the gaps, and repeating an already scaled picture costs nothing.
    let mut statements = vec![statement(
        &["0:v:0"],
        &format!(
            "scale={}:{},setsar=1,fps={frame_rate}:round=near,split={clip_count}",
            specification.picture_width_in_pixels, specification.picture_height_in_pixels
        ),
        &source_labels,
    )];

    // A picture stream can only be used once, so each accelerated clip gets its own copy of the
    // symbol picture for its speed.
    let mut unused_symbol_copies: BTreeMap<SpeedFactor, Vec<String>> = BTreeMap::new();
    for (position, speed_factor) in specification.indicator_overlays.keys().enumerate() {
        let copies_needed = specification
            .clips
            .iter()
            .filter(|clip| clip.speed_factor == *speed_factor)
            .count();
        if copies_needed == 0 {
            continue;
        }
        let input_number = position + 1;
        let copy_labels = numbered_labels(&format!("symbol_{}x_copy", speed_factor.value()), copies_needed);
        statements.push(statement(
            &[&format!("{input_number}:v")],
            &format!("split={copies_needed}"),
            &copy_labels,
        ));
        unused_symbol_copies.insert(*speed_factor, copy_labels);
    }

    let mut finished_clip_labels = Vec::with_capacity(clip_count);
    for (clip_number, clip) in specification.clips.iter().enumerate() {
        let cut_and_retime = cut_and_retime_filters(frame_rate, clip);
        let finished_label = format!("finished_clip_{clip_number}");
        let symbol_for_this_clip = unused_symbol_copies
            .get_mut(&clip.speed_factor)
            .and_then(|copies| copies.pop());
        match symbol_for_this_clip {
            Some(symbol_label) => {
                let overlay = specification.indicator_overlays[&clip.speed_factor];
                let picture_label = format!("clip_picture_{clip_number}");
                statements.push(statement(
                    &[&source_labels[clip_number]],
                    &cut_and_retime,
                    &[&picture_label],
                ));
                statements.push(statement(
                    &[&picture_label, &symbol_label],
                    &format!(
                        "overlay=x={}:y={}:format=auto",
                        overlay.left_in_pixels, overlay.top_in_pixels
                    ),
                    &[&finished_label],
                ));
            }
            None => statements.push(statement(
                &[&source_labels[clip_number]],
                &cut_and_retime,
                &[&finished_label],
            )),
        }
        finished_clip_labels.push(finished_label);
    }

    statements.push(statement(
        &finished_clip_labels.iter().map(String::as_str).collect::<Vec<_>>(),
        &format!("concat=n={clip_count}:v=1:a=0"),
        &[JOINED_VIDEO_LABEL],
    ));
    // Give every finished picture the timestamp its position calls for, exactly one frame after
    // the one before. Joining clips cannot be relied on to do this: it believes a clip with a
    // single picture (a few frames sped up beyond their number) lasts no time at all, so the clip
    // after it starts at the same instant, and the encoder then throws a picture away.
    statements.push(statement(
        &[JOINED_VIDEO_LABEL],
        &renumber_pictures_filter(frame_rate),
        &[VIDEO_OUTPUT_LABEL],
    ));
    statements
}

fn audio_statements(specification: &FilterGraphSpecification) -> Vec<String> {
    let frame_rate = specification.frame_rate;
    let clip_count = specification.clips.len();
    let source_labels = numbered_labels("clip_source_audio", clip_count);
    let mut statements = vec![statement(&["0:a:0"], &format!("asplit={clip_count}"), &source_labels)];

    let mut finished_clip_labels = Vec::with_capacity(clip_count);
    for (clip_number, clip) in specification.clips.iter().enumerate() {
        let (sound_start, sound_end) = audio_cut_times(frame_rate, clip);
        let exact_length = format_seconds(frame_rate.seconds_at_frame(clip.timeline_frame_count()));
        let mut filters = format!("atrim=start={sound_start}:end={sound_end},asetpts=PTS-STARTPTS");
        if is_too_short_to_speed_up(frame_rate, clip) {
            filters.push_str(",volume=0");
        } else {
            for tempo in audio_tempo_filters(clip.speed_factor) {
                filters.push_str(&format!(",atempo={tempo}"));
            }
        }
        filters.push_str(&format!(
            ",apad=whole_dur={exact_length},atrim=end={exact_length},asetpts=PTS-STARTPTS"
        ));
        let finished_label = format!("finished_sound_{clip_number}");
        statements.push(statement(&[&source_labels[clip_number]], &filters, &[&finished_label]));
        finished_clip_labels.push(finished_label);
    }

    statements.push(statement(
        &finished_clip_labels.iter().map(String::as_str).collect::<Vec<_>>(),
        &format!("concat=n={clip_count}:v=0:a=1"),
        &[AUDIO_OUTPUT_LABEL],
    ));
    statements
}

/// The filters that cut `clip` out of the resampled source and make it play at its speed: take
/// its range of pictures, keep every `speed_factor`th one, and number them from 0 at the
/// project's frame rate.
fn cut_and_retime_filters(frame_rate: FrameRate, clip: &ClipToExport) -> String {
    let mut filters = format!(
        "trim=start_frame={}:end_frame={}",
        clip.first_source_frame, clip.end_source_frame
    );
    if clip.speed_factor.is_accelerated() {
        // `n` counts the pictures that reach `select`, from 0: the clip's own first picture.
        filters.push_str(&format!(",select='not(mod(n,{}))'", clip.speed_factor.value()));
    }
    filters.push_str(&format!(",{}", renumber_pictures_filter(frame_rate)));
    filters
}

/// A `setpts` filter that gives each picture the timestamp its position calls for: picture
/// number `N` at exactly `N` frames after the first.
fn renumber_pictures_filter(frame_rate: FrameRate) -> String {
    format!("setpts=N*{}/({}*TB)", frame_rate.denominator(), frame_rate.numerator())
}

/// The times between which the sound of `clip` is cut out: exactly from the start of its first
/// frame to the start of the frame after its last (sound has no frames to lose or duplicate).
fn audio_cut_times(frame_rate: FrameRate, clip: &ClipToExport) -> (String, String) {
    (
        format_seconds(frame_rate.seconds_at_frame(clip.first_source_frame)),
        format_seconds(frame_rate.seconds_at_frame(clip.end_source_frame)),
    )
}

/// Whether `clip` is accelerated but so short that its sped-up sound would be under
/// [`SHORTEST_SPED_UP_SOUND_IN_SECONDS`].
fn is_too_short_to_speed_up(frame_rate: FrameRate, clip: &ClipToExport) -> bool {
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

    fn speed(factor: u32) -> SpeedFactor {
        SpeedFactor::new(factor).unwrap()
    }

    fn clip(first: u64, end: u64, factor: u32) -> ClipToExport {
        ClipToExport {
            first_source_frame: first,
            end_source_frame: end,
            speed_factor: speed(factor),
        }
    }

    fn specification(clips: Vec<ClipToExport>, source_has_audio: bool) -> FilterGraphSpecification {
        FilterGraphSpecification {
            frame_rate: FrameRate::new(30, 1).unwrap(),
            clips,
            picture_width_in_pixels: 320,
            picture_height_in_pixels: 180,
            source_has_audio,
            indicator_overlays: BTreeMap::new(),
        }
    }

    fn statements_of(graph: &FilterGraph) -> Vec<&str> {
        graph.text.split(';').collect()
    }

    #[test]
    fn a_single_normal_speed_clip_is_scaled_resampled_cut_by_picture_number_and_renumbered() {
        let graph = build_filter_graph(&specification(vec![clip(0, 90, 1)], false));
        assert_eq!(
            statements_of(&graph),
            [
                "[0:v:0]scale=320:180,setsar=1,fps=30/1:round=near,split=1[clip_source_video_0]",
                "[clip_source_video_0]trim=start_frame=0:end_frame=90,setpts=N*1/(30*TB)[finished_clip_0]",
                "[finished_clip_0]concat=n=1:v=1:a=0[joined_video]",
                "[joined_video]setpts=N*1/(30*TB)[finished_video]",
            ]
        );
        assert_eq!(graph.video_output_label, "finished_video");
        assert_eq!(graph.audio_output_label, None);
    }

    #[test]
    fn an_accelerated_clip_keeps_every_nth_picture_and_a_normal_one_keeps_them_all() {
        let graph = build_filter_graph(&specification(vec![clip(0, 150, 1), clip(150, 330, 3)], false));
        assert_eq!(
            statements_of(&graph),
            [
                "[0:v:0]scale=320:180,setsar=1,fps=30/1:round=near,split=2[clip_source_video_0][clip_source_video_1]",
                "[clip_source_video_0]trim=start_frame=0:end_frame=150,setpts=N*1/(30*TB)[finished_clip_0]",
                "[clip_source_video_1]trim=start_frame=150:end_frame=330,select='not(mod(n,3))',setpts=N*1/(30*TB)[finished_clip_1]",
                "[finished_clip_0][finished_clip_1]concat=n=2:v=1:a=0[joined_video]",
                "[joined_video]setpts=N*1/(30*TB)[finished_video]",
            ]
        );
    }

    /// Regression test: joining clips gave the clip after a single-picture clip the same
    /// timestamp as that picture, and the encoder dropped a picture. Renumbering the joined
    /// pictures by position removes the problem whatever the join does.
    #[test]
    fn the_joined_pictures_are_numbered_by_position_at_the_exact_frame_rate() {
        for (numerator, denominator, expected) in [
            (30, 1, "setpts=N*1/(30*TB)"),
            (30000, 1001, "setpts=N*1001/(30000*TB)"),
            (24000, 1001, "setpts=N*1001/(24000*TB)"),
            (60, 1, "setpts=N*1/(60*TB)"),
        ] {
            let mut spec = specification(vec![clip(0, 4, 4), clip(4, 8, 1)], false);
            spec.frame_rate = FrameRate::new(numerator, denominator).unwrap();
            let graph = build_filter_graph(&spec);
            let last_statement = statements_of(&graph).last().copied().unwrap().to_owned();
            assert_eq!(last_statement, format!("[joined_video]{expected}[finished_video]"));
        }
    }

    #[test]
    fn neighbouring_clips_share_their_boundary_picture_number_so_none_is_lost_or_doubled() {
        let graph = build_filter_graph(&specification(
            vec![clip(0, 100, 1), clip(100, 250, 2), clip(250, 300, 1)],
            false,
        ))
        .text;
        assert!(graph.contains("trim=start_frame=0:end_frame=100,"), "{graph}");
        assert!(graph.contains("trim=start_frame=100:end_frame=250,"), "{graph}");
        assert!(graph.contains("trim=start_frame=250:end_frame=300,"), "{graph}");
    }

    /// Regression test: with timestamps rounded to milliseconds (as in Matroska files) a third of
    /// the pictures landed in the wrong frame slot when the source was resampled with
    /// `round=up`. `round=near` puts every picture in its own slot.
    #[test]
    fn the_source_is_resampled_to_the_project_frame_rate_to_the_nearest_slot() {
        for (numerator, denominator) in [(30, 1), (30000, 1001), (24000, 1001), (60, 1)] {
            let mut spec = specification(vec![clip(0, 100, 1)], false);
            spec.frame_rate = FrameRate::new(numerator, denominator).unwrap();
            let graph = build_filter_graph(&spec).text;
            assert!(
                graph.contains(&format!("fps={numerator}/{denominator}:round=near,split=1")),
                "{graph}"
            );
            assert!(!graph.contains("round=up"), "{graph}");
        }
    }

    #[test]
    fn clips_are_cut_by_picture_number_so_the_frame_rate_does_not_matter_for_the_cuts() {
        let mut spec = specification(vec![clip(30000, 60000, 1)], false);
        spec.frame_rate = FrameRate::new(30000, 1001).unwrap();
        let graph = build_filter_graph(&spec).text;
        assert!(
            graph.contains("trim=start_frame=30000:end_frame=60000,setpts=N*1001/(30000*TB)"),
            "{graph}"
        );
    }

    #[test]
    fn normal_speed_clips_have_no_select_filter_and_the_select_counts_from_the_clips_own_first_picture() {
        let graph = build_filter_graph(&specification(vec![clip(0, 100, 1), clip(100, 250, 5)], false)).text;
        assert_eq!(graph.matches("select=").count(), 1, "{graph}");
        assert!(
            graph.contains("trim=start_frame=100:end_frame=250,select='not(mod(n,5))',"),
            "{graph}"
        );
    }

    #[test]
    fn sound_is_cut_sped_up_and_forced_to_the_exact_length_of_the_pictures() {
        let graph = build_filter_graph(&specification(vec![clip(0, 150, 1), clip(150, 330, 3)], true));
        assert_eq!(graph.audio_output_label, Some("finished_audio"));
        let statements = statements_of(&graph);
        // Five video statements (split, two clips, join, renumber), then the four audio ones.
        assert_eq!(statements.len(), 5 + 4, "{statements:?}");
        assert_eq!(
            &statements[5..],
            [
                "[0:a:0]asplit=2[clip_source_audio_0][clip_source_audio_1]",
                // Normal speed: no atempo. 150 frames at 30 fps are exactly 5 s.
                "[clip_source_audio_0]atrim=start=0.000000000:end=5.000000000,asetpts=PTS-STARTPTS,apad=whole_dur=5.000000000,atrim=end=5.000000000,asetpts=PTS-STARTPTS[finished_sound_0]",
                // 180 source frames at 3x are 60 frames = exactly 2 s.
                "[clip_source_audio_1]atrim=start=5.000000000:end=11.000000000,asetpts=PTS-STARTPTS,atempo=3,apad=whole_dur=2.000000000,atrim=end=2.000000000,asetpts=PTS-STARTPTS[finished_sound_1]",
                "[finished_sound_0][finished_sound_1]concat=n=2:v=0:a=1[finished_audio]",
            ]
        );
    }

    #[test]
    fn the_exact_length_of_sound_follows_the_rounded_up_frame_count_not_the_raw_duration() {
        // 10 frames at 4x show 3 pictures (0, 4, 8) = 0.1 s, though 10/4 frames would be 0.0833 s.
        let graph = build_filter_graph(&specification(vec![clip(0, 10, 4)], true)).text;
        assert!(
            graph.contains("apad=whole_dur=0.100000000,atrim=end=0.100000000"),
            "{graph}"
        );
    }

    #[test]
    fn a_source_without_sound_leaves_the_sound_out_of_the_graph_entirely() {
        let graph = build_filter_graph(&specification(vec![clip(0, 150, 1), clip(150, 330, 3)], false));
        assert!(!graph.text.contains("atrim"), "{}", graph.text);
        assert!(!graph.text.contains("[0:a"), "{}", graph.text);
    }

    fn sound_chain_of_only_clip(clip: ClipToExport) -> String {
        let graph = build_filter_graph(&specification(vec![clip], true));
        statements_of(&graph)
            .into_iter()
            .find(|statement| statement.contains("atrim=start"))
            .unwrap()
            .to_owned()
    }

    /// Regression test: `atempo` produces no sound for a very short clip at a high speed, and
    /// joining a clip like that to the others made ffmpeg fail ("Invalid data found when
    /// processing input"). Such clips now get silence of the right length instead.
    #[test]
    fn a_clip_whose_sped_up_sound_would_be_a_blip_gets_silence_instead_of_atempo() {
        // 2 source frames (66 ms) at 33x: 2 ms of sound.
        let chain = sound_chain_of_only_clip(clip(0, 2, 33));
        assert!(
            chain.contains(",volume=0,apad=whole_dur=0.033333333,atrim=end=0.033333333"),
            "{chain}"
        );
        assert!(!chain.contains("atempo"), "{chain}");
    }

    #[test]
    fn a_clip_whose_sped_up_sound_is_long_enough_keeps_its_sound() {
        // 30 source frames (1 s) at 8x: 125 ms of sound, over the 100 ms limit.
        let chain = sound_chain_of_only_clip(clip(0, 30, 8));
        assert!(chain.contains(",atempo=8,"), "{chain}");
        assert!(!chain.contains("volume=0"), "{chain}");
    }

    #[test]
    fn the_silence_rule_switches_exactly_at_a_tenth_of_a_second_of_sped_up_sound() {
        // 3 frames at 30 fps = 0.1 s of source. At 1x that is not accelerated, so it is never silenced.
        assert!(!sound_chain_of_only_clip(clip(0, 3, 1)).contains("volume=0"));
        // 6 source frames (0.2 s) at 2x is exactly 0.1 s of sound: not under the limit.
        assert!(sound_chain_of_only_clip(clip(0, 6, 2)).contains("atempo=2"));
        // 5 source frames (0.1667 s) at 2x is 0.0833 s: under the limit.
        assert!(sound_chain_of_only_clip(clip(0, 5, 2)).contains("volume=0"));
    }

    #[test]
    fn normal_speed_clips_are_never_silenced_however_short() {
        let chain = sound_chain_of_only_clip(clip(100, 101, 1));
        assert!(!chain.contains("volume=0") && !chain.contains("atempo"), "{chain}");
    }

    #[test]
    fn audio_tempo_filters_multiply_up_to_the_speed_factor() {
        for factor in [
            1u32,
            2,
            3,
            7,
            50,
            99,
            100,
            101,
            150,
            250,
            999,
            1000,
            10_000,
            12_345,
            1_000_000,
            4_000_000_000,
        ] {
            let tempos = audio_tempo_filters(speed(factor));
            let product: f64 = tempos.iter().map(|tempo| tempo.parse::<f64>().unwrap()).product();
            let relative_error = (product - f64::from(factor)).abs() / f64::from(factor);
            assert!(
                relative_error < 1e-8,
                "factor {factor}: tempos {tempos:?} multiply to {product}"
            );
        }
    }

    #[test]
    fn audio_tempo_filters_never_exceed_the_per_filter_limit_or_slow_sound_down() {
        for factor in [2u32, 100, 101, 250, 1000, 10_000, 1_000_000, 4_000_000_000] {
            for tempo in audio_tempo_filters(speed(factor)) {
                let tempo: f64 = tempo.parse().unwrap();
                assert!((1.0..=100.0).contains(&tempo), "factor {factor} gave tempo {tempo}");
            }
        }
    }

    #[test]
    fn normal_speed_needs_no_tempo_filter_and_small_factors_need_exactly_one() {
        assert!(audio_tempo_filters(speed(1)).is_empty());
        assert_eq!(audio_tempo_filters(speed(8)), ["8"]);
        assert_eq!(audio_tempo_filters(speed(100)), ["100"]);
        assert_eq!(audio_tempo_filters(speed(250)), ["100", "2.5"]);
        assert_eq!(audio_tempo_filters(speed(1000)), ["100", "10"]);
        assert_eq!(audio_tempo_filters(speed(10_000)), ["100", "100"]);
    }

    fn overlays(entries: &[(u32, u32, u32)]) -> BTreeMap<SpeedFactor, IndicatorOverlay> {
        entries
            .iter()
            .map(|&(factor, left, top)| {
                (
                    speed(factor),
                    IndicatorOverlay {
                        left_in_pixels: left,
                        top_in_pixels: top,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn an_accelerated_clip_gets_its_symbol_laid_over_it_and_a_normal_clip_does_not() {
        let mut spec = specification(vec![clip(0, 150, 1), clip(150, 330, 3)], false);
        spec.indicator_overlays = overlays(&[(3, 12, 8)]);
        let graph = build_filter_graph(&spec);
        assert_eq!(
            statements_of(&graph),
            [
                "[0:v:0]scale=320:180,setsar=1,fps=30/1:round=near,split=2[clip_source_video_0][clip_source_video_1]",
                "[1:v]split=1[symbol_3x_copy_0]",
                "[clip_source_video_0]trim=start_frame=0:end_frame=150,setpts=N*1/(30*TB)[finished_clip_0]",
                "[clip_source_video_1]trim=start_frame=150:end_frame=330,select='not(mod(n,3))',setpts=N*1/(30*TB)[clip_picture_1]",
                "[clip_picture_1][symbol_3x_copy_0]overlay=x=12:y=8:format=auto[finished_clip_1]",
                "[finished_clip_0][finished_clip_1]concat=n=2:v=1:a=0[joined_video]",
                "[joined_video]setpts=N*1/(30*TB)[finished_video]",
            ]
        );
    }

    #[test]
    fn a_symbol_used_by_several_clips_is_copied_once_for_each_and_every_copy_is_used_once() {
        let mut spec = specification(
            vec![clip(0, 90, 4), clip(90, 180, 1), clip(180, 270, 4), clip(270, 360, 4)],
            false,
        );
        spec.indicator_overlays = overlays(&[(4, 10, 10)]);
        let graph = build_filter_graph(&spec).text;
        assert!(
            graph.contains("[1:v]split=3[symbol_4x_copy_0][symbol_4x_copy_1][symbol_4x_copy_2]"),
            "{graph}"
        );
        for copy in 0..3 {
            let label = format!("[symbol_4x_copy_{copy}]");
            assert_eq!(
                graph.matches(&label).count(),
                2,
                "{label} is made once and used once in {graph}"
            );
        }
        assert_eq!(graph.matches("overlay=").count(), 3);
    }

    #[test]
    fn different_speeds_use_different_symbol_pictures_in_the_order_of_the_overlay_map() {
        let mut spec = specification(vec![clip(0, 90, 8), clip(90, 180, 2)], false);
        spec.indicator_overlays = overlays(&[(8, 5, 5), (2, 7, 7)]);
        let graph = build_filter_graph(&spec).text;
        // Map order is by speed: speed 2 is input 1, then speed 8 is input 2.
        assert!(graph.contains("[1:v]split=1[symbol_2x_copy_0]"), "{graph}");
        assert!(graph.contains("[2:v]split=1[symbol_8x_copy_0]"), "{graph}");
        assert!(
            graph.contains("[clip_picture_0][symbol_8x_copy_0]overlay=x=5:y=5"),
            "{graph}"
        );
        assert!(
            graph.contains("[clip_picture_1][symbol_2x_copy_0]overlay=x=7:y=7"),
            "{graph}"
        );
    }

    #[test]
    fn a_symbol_that_no_clip_uses_is_left_out_of_the_graph() {
        let mut spec = specification(vec![clip(0, 90, 1)], false);
        spec.indicator_overlays = overlays(&[(4, 10, 10)]);
        let graph = build_filter_graph(&spec).text;
        assert!(!graph.contains("overlay"), "{graph}");
        assert!(!graph.contains("[1:v]"), "{graph}");
    }

    #[test]
    fn the_picture_is_scaled_to_the_requested_size_with_square_pixels_before_anything_else() {
        let mut spec = specification(vec![clip(0, 90, 1)], false);
        spec.picture_width_in_pixels = 1280;
        spec.picture_height_in_pixels = 720;
        assert!(
            build_filter_graph(&spec)
                .text
                .starts_with("[0:v:0]scale=1280:720,setsar=1,fps=")
        );
    }

    #[test]
    fn a_hundred_and_fifty_clips_make_a_graph_far_too_long_for_a_command_line() {
        // This is why the graph is handed to ffmpeg in a file: Windows limits a command line to
        // about 32,000 characters.
        let clips: Vec<ClipToExport> = (0..150)
            .map(|number| clip(number * 100, number * 100 + 100, 1 + (number % 5) as u32))
            .collect();
        let graph = build_filter_graph(&specification(clips, true)).text;
        assert!(graph.len() > 32_000, "only {} characters", graph.len());
    }

    #[test]
    fn timeline_frame_counts_round_a_partial_group_up() {
        assert_eq!(clip(0, 10, 4).timeline_frame_count(), 3);
        assert_eq!(clip(0, 100, 1).timeline_frame_count(), 100);
        assert_eq!(clip(5, 6, 100).timeline_frame_count(), 1);
    }
}
