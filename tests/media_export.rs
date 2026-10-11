//! Exporting projects with the real ffmpeg, checked picture by picture against what the project
//! says the finished video must contain.

mod support;

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use fastforwardrs::media::FfmpegToolchain;
use fastforwardrs::media::export::{ExportError, ExportEvent, ExportResult, VideoExport, plan_export};
use fastforwardrs::project::{Project, Settings, SpedUpClipSound, SpeedFactor, SpeedIndicatorLayout};
use fastforwardrs::speed_indicator::indicator_for_frame;
use support::{
    RandomNumbers, TestVideo, TestVideoSpecification, counter_video, decode_video, decoded_sound, edited,
    expected_source_frames, loudness, pitch_in_hertz, project_for, scratch_directory, stream_durations, test_video,
    transparent_text_picture,
};

const TIME_ALLOWED_FOR_AN_EXPORT: Duration = Duration::from_secs(180);

struct ExportOutcome {
    result: ExportResult,
    progress_updates: Vec<u64>,
}

fn export_and_wait(toolchain: &FfmpegToolchain, project: &Project, source: &Path, destination: &Path) -> ExportOutcome {
    export_and_wait_with(toolchain, project, Settings::DEFAULT, source, destination)
}

fn export_and_wait_with(
    toolchain: &FfmpegToolchain,
    project: &Project,
    settings: Settings,
    source: &Path,
    destination: &Path,
) -> ExportOutcome {
    let plan = plan_export(project, settings, &transparent_text_picture).expect("the project can be exported");
    let export = VideoExport::start(toolchain, &plan, &[source.to_path_buf()], destination, Arc::new(|| {}))
        .expect("the export starts");
    wait_for_export(&export)
}

fn wait_for_export(export: &VideoExport) -> ExportOutcome {
    let started = Instant::now();
    let mut progress_updates = Vec::new();
    loop {
        match export.try_next_event() {
            Some(ExportEvent::Progress { frames_written }) => progress_updates.push(frames_written),
            Some(ExportEvent::Finished(result)) => {
                return ExportOutcome {
                    result,
                    progress_updates,
                };
            }
            None => {
                assert!(
                    started.elapsed() < TIME_ALLOWED_FOR_AN_EXPORT,
                    "the export did not finish in time"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

#[test]
fn an_unedited_project_exports_every_frame_of_the_video_with_its_sound() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let project = project_for(&toolchain, &video);
    let scratch = scratch_directory();
    let destination = scratch.path().join("unedited.mp4");

    let outcome = export_and_wait(&toolchain, &project, &video.path, &destination);

    assert_eq!(outcome.result, ExportResult::Completed);
    let decoded = decode_video(&toolchain, &destination, 320, 180);
    assert_eq!(decoded.frame_numbers(), (0..600).collect::<Vec<u64>>());
    let (video_seconds, audio_seconds) = stream_durations(&toolchain, &destination);
    assert!((video_seconds.unwrap() - 20.0).abs() < 0.01, "{video_seconds:?}");
    assert!((audio_seconds.unwrap() - 20.0).abs() < 0.05, "{audio_seconds:?}");
}

#[test]
fn an_edited_project_exports_exactly_the_frames_its_clips_describe() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    // Clips: 0..150 at 1x, 150..330 at 3x, 330..375 at 1x, 375..600 at 5x.
    // All the splits are made before any speed is changed, so the timeline frames are source frames.
    let project = edited(project_for(&toolchain, &video), &[150, 330, 375], &[1, 3, 1, 5]);
    let expected = expected_source_frames(&project);
    assert_eq!(expected.len() as u64, project.total_frame_count());
    let scratch = scratch_directory();
    let destination = scratch.path().join("edited.mp4");

    let outcome = export_and_wait(&toolchain, &project, &video.path, &destination);

    assert_eq!(outcome.result, ExportResult::Completed);
    let decoded = decode_video(&toolchain, &destination, 320, 180);
    assert_eq!(decoded.frame_numbers(), expected);
    // Progress is reported, never goes backwards, and ends at the full length.
    assert!(!outcome.progress_updates.is_empty());
    assert!(
        outcome.progress_updates.windows(2).all(|pair| pair[0] <= pair[1]),
        "{:?}",
        outcome.progress_updates
    );
    assert_eq!(*outcome.progress_updates.last().unwrap(), expected.len() as u64);
}

#[test]
fn random_edits_always_export_exactly_the_frames_the_project_describes() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    for seed in 1..=6u64 {
        let mut random = RandomNumbers::with_seed(seed);
        let mut project = project_for(&toolchain, &video);
        for _ in 0..8 {
            let frame = random.next_below(project.total_frame_count());
            support::split(&mut project, frame);
        }
        for clip_index in 0..support::clip_count(&project) {
            let speed = [1u32, 1, 2, 3, 4, 5, 7, 8, 16, 33][random.next_below(10) as usize];
            support::set_clip_speed(&mut project, clip_index, speed);
        }
        let expected = expected_source_frames(&project);
        let destination = scratch.path().join(format!("random_{seed}.mp4"));

        let outcome = export_and_wait(&toolchain, &project, &video.path, &destination);

        assert_eq!(
            outcome.result,
            ExportResult::Completed,
            "seed {seed}: clips {:?}\ngraph: {}",
            project.tracks(),
            plan_export(&project, Settings::DEFAULT, &transparent_text_picture)
                .unwrap()
                .filter_graph
                .text
        );
        let frame_numbers = decode_video(&toolchain, &destination, 320, 180).frame_numbers();
        assert_eq!(
            frame_numbers.len(),
            expected.len(),
            "seed {seed}: clips {:?}",
            project.tracks()
        );
        assert_eq!(frame_numbers, expected, "seed {seed}: clips {:?}", project.tracks());
    }
}

/// Regression test: with the source's frame slots found by rounding timestamps up, a Matroska
/// file (timestamps rounded to milliseconds) exported duplicated and skipped pictures.
#[test]
fn files_whose_timestamps_are_rounded_to_milliseconds_export_exactly_too() {
    let toolchain = toolchain_or_skip!();
    let scratch = scratch_directory();
    for specification in [
        TestVideoSpecification {
            name: "counter_20_seconds_mkv",
            container: "mkv",
            ..TestVideoSpecification::COUNTER
        },
        TestVideoSpecification {
            name: "ntsc_20_seconds_mkv",
            container: "mkv",
            frame_rate: "30000/1001",
            ..TestVideoSpecification::COUNTER
        },
    ] {
        let video = test_video(&toolchain, &specification);
        let project = edited(project_for(&toolchain, &video), &[150, 330, 375], &[1, 3, 1, 5]);
        let expected = expected_source_frames(&project);
        let destination = scratch.path().join(format!("{}.mp4", specification.name));

        let outcome = export_and_wait(&toolchain, &project, &video.path, &destination);

        assert_eq!(outcome.result, ExportResult::Completed, "{}", specification.name);
        assert_eq!(
            decode_video(&toolchain, &destination, 320, 180).frame_numbers(),
            expected,
            "{}",
            specification.name
        );
    }
}

/// All the source frame numbers kept in the variable frame rate test video, which are the frames
/// that are on screen at some moment of it.
fn kept_frames_of_the_variable_frame_rate_video() -> Vec<u64> {
    (0..600u64).filter(|&frame| frame >= 300 || frame % 3 == 0).collect()
}

/// The kept frames nearest in time to `seconds` (several when two are equally near).
fn kept_frames_nearest_to(seconds: f64) -> Vec<u64> {
    let kept = kept_frames_of_the_variable_frame_rate_video();
    let distance = |frame: u64| (frame as f64 / 30.0 - seconds).abs();
    let smallest = kept.iter().map(|&frame| distance(frame)).fold(f64::INFINITY, f64::min);
    kept.into_iter()
        .filter(|&frame| distance(frame) <= smallest + 1e-9)
        .collect()
}

#[test]
fn a_variable_frame_rate_source_exports_the_picture_nearest_in_time_to_every_slot_and_keeps_its_length() {
    let toolchain = toolchain_or_skip!();
    let video = test_video(
        &toolchain,
        &TestVideoSpecification {
            name: "variable_frame_rate_20_seconds",
            with_audio: false,
            extra_video_filter: Some("select='lt(n,300)*not(mod(n,3))+gte(n,300)'"),
            extra_output_arguments: &["-fps_mode", "vfr"],
            ..TestVideoSpecification::COUNTER
        },
    );
    // 400 pictures in 20 seconds: the project runs at 20 frames per second.
    let mut project = project_for(&toolchain, &video);
    assert_eq!(project.frame_rate().frames_per_second(), 20.0);
    assert_eq!(project.total_frame_count(), 400);
    // Clips: slots 0..100 at 1x (a static stretch), 100..300 at 4x, 300..400 at 1x.
    for frame in [100, 300] {
        support::split(&mut project, frame);
    }
    support::set_clip_speed(&mut project, 1, 4);
    let scratch = scratch_directory();
    let destination = scratch.path().join("variable.mp4");

    let outcome = export_and_wait(&toolchain, &project, &video.path, &destination);

    assert_eq!(outcome.result, ExportResult::Completed);
    let frame_numbers = decode_video(&toolchain, &destination, 320, 180).frame_numbers();
    assert_eq!(
        frame_numbers.len() as u64,
        project.total_frame_count(),
        "the video is as long as the timeline"
    );
    assert_eq!(frame_numbers.len(), 100 + 50 + 100);
    // Each picture is the one on screen (nearest in time) at the moment its timeline frame shows.
    let source_slots: Vec<u64> = expected_source_frames(&project);
    for (timeline_frame, (&frame_number, &source_slot)) in frame_numbers.iter().zip(&source_slots).enumerate() {
        let nearest = kept_frames_nearest_to(source_slot as f64 / 20.0);
        assert!(
            nearest.contains(&frame_number),
            "timeline frame {timeline_frame} (source slot {source_slot} = {} s) shows source frame {frame_number}, but the nearest are {nearest:?}",
            source_slot as f64 / 20.0
        );
    }
}

#[test]
fn the_sound_stays_as_long_as_the_picture_however_the_clips_are_sped_up() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut project = project_for(&toolchain, &video);
    for frame in [100, 190, 260, 300] {
        support::split(&mut project, frame);
    }
    for (clip_index, speed) in [1u32, 4, 1, 16, 3].into_iter().enumerate() {
        support::set_clip_speed(&mut project, clip_index, speed);
    }
    let scratch = scratch_directory();
    let destination = scratch.path().join("sound.mp4");

    let outcome = export_and_wait(&toolchain, &project, &video.path, &destination);

    assert_eq!(outcome.result, ExportResult::Completed);
    let expected_seconds = project.total_frame_count() as f64 / 30.0;
    let (video_seconds, audio_seconds) = stream_durations(&toolchain, &destination);
    assert!(
        (video_seconds.unwrap() - expected_seconds).abs() < 0.01,
        "video {video_seconds:?}, expected {expected_seconds}"
    );
    assert!(
        (audio_seconds.unwrap() - expected_seconds).abs() < 0.05,
        "audio {audio_seconds:?} must match the video {expected_seconds} to within a frame or two"
    );
}

#[test]
fn the_sound_of_every_clip_is_really_there_not_silence() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut project = project_for(&toolchain, &video);
    support::split(&mut project, 300);
    support::set_clip_speed(&mut project, 1, 4);
    let scratch = scratch_directory();
    let destination = scratch.path().join("sound_not_silent.mp4");
    assert_eq!(
        export_and_wait(&toolchain, &project, &video.path, &destination).result,
        ExportResult::Completed
    );

    // Measure how loud each part is.
    let samples = decoded_sound(&toolchain, &destination, 1, 48_000);
    // The first clip is 10 s (300 frames at 30 fps); the second is 300 frames at 4x = 2.5 s.
    let first_clip_loudness = loudness(&samples[48_000..48_000 * 9]);
    let second_clip_loudness = loudness(&samples[48_000 * 10 + 4_800..48_000 * 12]);
    assert!(
        first_clip_loudness > 0.05,
        "the normal speed clip sounds silent: {first_clip_loudness}"
    );
    assert!(
        second_clip_loudness > 0.05,
        "the sped up clip sounds silent: {second_clip_loudness}"
    );
}

#[test]
fn with_sped_up_clips_muted_those_clips_are_silent_and_the_others_still_have_their_sound() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    // Clips: 0..300 at 1x (10 s), 300..600 at 4x (2.5 s).
    let project = edited(project_for(&toolchain, &video), &[300], &[1, 4]);
    let scratch = scratch_directory();
    let settings = Settings {
        sped_up_clip_sound: SpedUpClipSound::Mute,
        ..Settings::DEFAULT
    };

    let destination = scratch.path().join("muted.mp4");
    let outcome = export_and_wait_with(&toolchain, &project, settings, &video.path, &destination);
    assert_eq!(outcome.result, ExportResult::Completed);

    let samples = decoded_sound(&toolchain, &destination, 1, 48_000);
    let first_clip = loudness(&samples[48_000..48_000 * 9]);
    let sped_up_clip = loudness(&samples[48_000 * 10 + 4_800..48_000 * 12]);
    assert!(first_clip > 0.05, "the normal speed clip keeps its sound: {first_clip}");
    assert!(sped_up_clip < 0.001, "the sped up clip is silent: {sped_up_clip}");
    // Silence is as long as the picture: nothing shifted.
    let (video_seconds, audio_seconds) = stream_durations(&toolchain, &destination);
    assert!((video_seconds.unwrap() - 12.5).abs() < 0.01, "{video_seconds:?}");
    assert!((audio_seconds.unwrap() - 12.5).abs() < 0.05, "{audio_seconds:?}");
    // The pictures are exactly what they are with the sound on.
    assert_eq!(
        decode_video(&toolchain, &destination, 320, 180).frame_numbers(),
        expected_source_frames(&project)
    );

    // And with the setting off the very same project has sound in the sped up clip.
    let with_sound = scratch.path().join("with_sound.mp4");
    export_and_wait(&toolchain, &project, &video.path, &with_sound);
    let samples = decoded_sound(&toolchain, &with_sound, 1, 48_000);
    assert!(loudness(&samples[48_000 * 10 + 4_800..48_000 * 12]) > 0.05);
}

#[test]
fn a_sped_up_clip_keeps_its_pitch_or_has_it_raised_as_the_setting_says_and_is_as_long_as_its_pictures() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    // Clips: 0..300 at 1x (10 s), 300..600 at 4x (2.5 s). The sound is a steady 440 Hz tone.
    let project = edited(project_for(&toolchain, &video), &[300], &[1, 4]);
    let scratch = scratch_directory();

    for (sound, expected_pitch_in_the_fast_clip) in [
        (SpedUpClipSound::MaintainPitch, 440.0),
        (SpedUpClipSound::IncreasePitch, 4.0 * 440.0),
    ] {
        let settings = Settings {
            sped_up_clip_sound: sound,
            ..Settings::DEFAULT
        };
        let destination = scratch.path().join(format!("{sound:?}.mp4"));
        let outcome = export_and_wait_with(&toolchain, &project, settings, &video.path, &destination);
        assert_eq!(outcome.result, ExportResult::Completed, "{sound:?}");

        let samples = decoded_sound(&toolchain, &destination, 1, 48_000);
        let normal_clip = pitch_in_hertz(&samples[48_000..48_000 * 9], 48_000);
        let fast_clip = pitch_in_hertz(&samples[48_000 * 10 + 4_800..48_000 * 12], 48_000);
        assert!(
            (normal_clip - 440.0).abs() < 10.0,
            "{sound:?}: the normal clip is at {normal_clip} Hz"
        );
        assert!(
            (fast_clip - expected_pitch_in_the_fast_clip).abs() < expected_pitch_in_the_fast_clip * 0.03,
            "{sound:?}: the fast clip is at {fast_clip} Hz, expected {expected_pitch_in_the_fast_clip}"
        );
        assert!(
            loudness(&samples[48_000 * 10 + 4_800..48_000 * 12]) > 0.05,
            "{sound:?} is audible"
        );
        // Nothing shifted: the sound is as long as the pictures.
        let (video_seconds, audio_seconds) = stream_durations(&toolchain, &destination);
        assert!(
            (video_seconds.unwrap() - 12.5).abs() < 0.01,
            "{sound:?}: {video_seconds:?}"
        );
        assert!(
            (audio_seconds.unwrap() - 12.5).abs() < 0.05,
            "{sound:?}: {audio_seconds:?}"
        );
    }
}

/// Whether any pixel inside the symbol's place on the picture is bright white.
fn symbol_is_visible(picture: &[u8], picture_width: u32, left: u32, top: u32, width: u32, height: u32) -> bool {
    (top..top + height).any(|y| {
        (left..left + width).any(|x| {
            let offset = ((y * picture_width + x) * 3) as usize;
            picture[offset] > 235 && picture[offset + 1] > 235 && picture[offset + 2] > 235
        })
    })
}

#[test]
fn only_accelerated_clips_show_the_fast_forward_symbol() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut project = project_for(&toolchain, &video);
    for frame in [150, 330] {
        support::split(&mut project, frame);
    }
    for (clip_index, speed) in [1u32, 4, 1].into_iter().enumerate() {
        support::set_clip_speed(&mut project, clip_index, speed);
    }
    let scratch = scratch_directory();
    let destination = scratch.path().join("symbols.mp4");
    assert_eq!(
        export_and_wait(&toolchain, &project, &video.path, &destination).result,
        ExportResult::Completed
    );

    let decoded = decode_video(&toolchain, &destination, 320, 180);
    let symbol = indicator_for_frame(SpeedFactor::new(4).unwrap(), project.speed_indicator_layout(), 320, 180);
    let visible: Vec<bool> = decoded
        .pictures
        .iter()
        .map(|picture| {
            symbol_is_visible(
                picture,
                320,
                symbol.left_in_pixels,
                symbol.top_in_pixels,
                symbol.picture.width_in_pixels,
                symbol.picture.height_in_pixels,
            )
        })
        .collect();
    // Timeline: clip 0 = frames 0..150 (1x), clip 1 = 150..195 (180 source frames at 4x), clip 2 = 195..240.
    assert!(
        visible[..150].iter().all(|&shown| !shown),
        "no symbol on the first, normal speed clip"
    );
    assert!(
        visible[150..195].iter().all(|&shown| shown),
        "the symbol is on every frame of the accelerated clip"
    );
    assert!(
        visible[195..].iter().all(|&shown| !shown),
        "no symbol on the last, normal speed clip"
    );
}

#[test]
fn the_symbol_is_drawn_where_the_layout_says_and_nowhere_else() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut project = project_for(&toolchain, &video);
    support::set_clip_speed(&mut project, 0, 2);
    project.set_speed_indicator_layout(SpeedIndicatorLayout::new(0.6, 0.55, 0.25));
    let scratch = scratch_directory();
    let destination = scratch.path().join("placed.mp4");
    assert_eq!(
        export_and_wait(&toolchain, &project, &video.path, &destination).result,
        ExportResult::Completed
    );

    let picture = &decode_video(&toolchain, &destination, 320, 180).pictures[10];
    let symbol = indicator_for_frame(SpeedFactor::new(2).unwrap(), project.speed_indicator_layout(), 320, 180);
    assert!(symbol_is_visible(
        picture,
        320,
        symbol.left_in_pixels,
        symbol.top_in_pixels,
        symbol.picture.width_in_pixels,
        symbol.picture.height_in_pixels
    ));
    // The lower-left quarter of the picture is far from the symbol, and stays plain grey.
    assert!(
        !symbol_is_visible(picture, 320, 0, 100, 100, 80),
        "nothing bright outside the symbol's place"
    );
}

#[test]
fn a_source_without_sound_exports_a_video_without_sound() {
    let toolchain = toolchain_or_skip!();
    let video = test_video(
        &toolchain,
        &TestVideoSpecification {
            name: "silent_3_seconds",
            duration_in_seconds: 3,
            with_audio: false,
            ..TestVideoSpecification::COUNTER
        },
    );
    let mut project = project_for(&toolchain, &video);
    support::split(&mut project, 30);
    support::set_clip_speed(&mut project, 1, 2);
    let scratch = scratch_directory();
    let destination = scratch.path().join("silent.mp4");

    let outcome = export_and_wait(&toolchain, &project, &video.path, &destination);

    assert_eq!(outcome.result, ExportResult::Completed);
    assert_eq!(
        decode_video(&toolchain, &destination, 320, 180).frame_numbers(),
        expected_source_frames(&project)
    );
    assert_eq!(stream_durations(&toolchain, &destination).1, None, "no audio stream");
}

#[test]
fn a_project_with_a_hundred_and_fifty_clips_exports_even_though_its_graph_cannot_fit_on_a_command_line() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut project = project_for(&toolchain, &video);
    for frame in 1..150u64 {
        support::split(&mut project, frame * 4);
    }
    assert_eq!(support::clip_count(&project), 150);
    for clip_index in 0..150 {
        support::set_clip_speed(&mut project, clip_index, 1 + (clip_index as u32 % 4));
    }
    let plan = plan_export(&project, Settings::DEFAULT, &transparent_text_picture).unwrap();
    assert!(
        plan.filter_graph.text.len() > 32_768,
        "the graph must be too long for a Windows command line: {} characters",
        plan.filter_graph.text.len()
    );
    let expected = expected_source_frames(&project);
    let scratch = scratch_directory();
    let destination = scratch.path().join("many_clips.mp4");

    let outcome = export_and_wait(&toolchain, &project, &video.path, &destination);

    assert_eq!(outcome.result, ExportResult::Completed);
    assert_eq!(
        decode_video(&toolchain, &destination, 320, 180).frame_numbers(),
        expected
    );
}

/// Regression test: a clip with only one picture (four frames sped up by four, say) made the
/// clip after it start at the same instant, and the encoder dropped that clip's first frame.
#[test]
fn clips_that_are_a_single_picture_long_never_cost_the_next_clip_its_first_frame() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut project = project_for(&toolchain, &video);
    for frame in 1..40u64 {
        support::split(&mut project, frame * 4);
    }
    // Clips of 4 frames: every other one at 4x (a single picture), the rest at normal speed.
    for clip_index in 0..support::clip_count(&project) {
        let speed = if clip_index % 2 == 0 { 4 } else { 1 };
        support::set_clip_speed(&mut project, clip_index, speed);
    }
    let expected = expected_source_frames(&project);
    let scratch = scratch_directory();
    let destination = scratch.path().join("single_pictures.mp4");

    let outcome = export_and_wait(&toolchain, &project, &video.path, &destination);

    assert_eq!(outcome.result, ExportResult::Completed);
    assert_eq!(
        decode_video(&toolchain, &destination, 320, 180).frame_numbers(),
        expected
    );
}

#[test]
fn the_finished_video_has_the_exact_constant_frame_rate_whatever_the_clips() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut project = project_for(&toolchain, &video);
    for frame in [3, 10, 50, 51, 200] {
        support::split(&mut project, frame);
    }
    for (clip_index, speed) in [7u32, 1, 3, 100, 2, 5].into_iter().enumerate() {
        support::set_clip_speed(&mut project, clip_index, speed);
    }
    let scratch = scratch_directory();
    let destination = scratch.path().join("constant_rate.mp4");
    assert_eq!(
        export_and_wait(&toolchain, &project, &video.path, &destination).result,
        ExportResult::Completed
    );

    let output = std::process::Command::new(toolchain.ffprobe_path())
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "frame=pts_time",
            "-of",
            "csv=p=0",
        ])
        .arg(&destination)
        .output()
        .unwrap();
    let times: Vec<f64> = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .map(|text| text.trim_end_matches(',').parse().unwrap())
        .collect();
    assert_eq!(times.len() as u64, project.total_frame_count());
    for (frame_number, &time) in times.iter().enumerate() {
        let expected_time = frame_number as f64 / 30.0;
        assert!(
            (time - expected_time).abs() < 1e-4,
            "frame {frame_number} is at {time}, expected {expected_time}"
        );
    }
}

#[test]
fn a_file_name_with_unusual_characters_is_fine_for_source_and_destination() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let oddly_named_source = scratch.path().join("holiday #1 (final) - café & friends [2026].mp4");
    std::fs::copy(&video.path, &oddly_named_source).unwrap();
    let mut project = project_for(&toolchain, &video);
    support::split(&mut project, 100);
    support::set_clip_speed(&mut project, 1, 3);
    let destination = scratch.path().join("résumé of 'the' holiday; cut #2.mp4");

    let outcome = export_and_wait(&toolchain, &project, &oddly_named_source, &destination);

    assert_eq!(outcome.result, ExportResult::Completed);
    assert!(destination.exists());
}

#[test]
fn the_container_follows_the_destination_extension() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut project = project_for(&toolchain, &video);
    support::set_clip_speed(&mut project, 0, 10);
    let scratch = scratch_directory();
    for extension in ["mkv", "mov"] {
        let destination = scratch.path().join(format!("container.{extension}"));
        let outcome = export_and_wait(&toolchain, &project, &video.path, &destination);
        assert_eq!(outcome.result, ExportResult::Completed, "{extension}");
        assert_eq!(
            decode_video(&toolchain, &destination, 320, 180).frame_numbers(),
            expected_source_frames(&project),
            "{extension}"
        );
    }
}

#[test]
fn a_rotated_source_exports_upright_at_its_displayed_size() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let rotated = scratch.path().join("rotated.mp4");
    let status = std::process::Command::new(toolchain.ffmpeg_path())
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-y",
            "-display_rotation",
            "90",
            "-i",
        ])
        .arg(&video.path)
        .args(["-c", "copy"])
        .arg(&rotated)
        .status()
        .unwrap();
    assert!(status.success());
    let rotated_video = TestVideo {
        path: rotated.clone(),
        specification: video.specification.clone(),
    };
    let mut project = project_for(&toolchain, &rotated_video);
    support::set_clip_speed(&mut project, 0, 20);
    let destination = scratch.path().join("rotated_export.mp4");

    let outcome = export_and_wait(&toolchain, &project, &rotated, &destination);

    assert_eq!(outcome.result, ExportResult::Completed);
    let size = support::probe_json(&toolchain, &destination, &["-show_streams", "-select_streams", "v:0"]);
    assert_eq!(
        (
            size["streams"][0]["width"].as_u64(),
            size["streams"][0]["height"].as_u64()
        ),
        (Some(180), Some(320))
    );
    assert_eq!(
        size["streams"][0]["nb_frames"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap(),
        30
    );
}

#[test]
fn non_square_pixels_are_exported_as_the_square_pixel_picture_they_are_shown_as() {
    let toolchain = toolchain_or_skip!();
    let video = test_video(
        &toolchain,
        &TestVideoSpecification {
            name: "wide_pixels_2_seconds",
            duration_in_seconds: 2,
            with_audio: false,
            extra_video_filter: Some("setsar=2/1"),
            ..TestVideoSpecification::COUNTER
        },
    );
    let project = project_for(&toolchain, &video);
    let scratch = scratch_directory();
    let destination = scratch.path().join("anamorphic.mp4");

    let outcome = export_and_wait(&toolchain, &project, &video.path, &destination);

    assert_eq!(outcome.result, ExportResult::Completed);
    let decoded = decode_video(&toolchain, &destination, 640, 180);
    assert_eq!(decoded.pictures.len(), 60);
}

#[test]
fn exporting_over_the_source_video_is_refused_before_anything_is_touched() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let scratch = scratch_directory();
    let source_copy = scratch.path().join("source.mp4");
    std::fs::copy(&video.path, &source_copy).unwrap();
    let project = project_for(&toolchain, &video);
    let plan = plan_export(&project, Settings::DEFAULT, &transparent_text_picture).unwrap();
    let original_bytes = std::fs::read(&source_copy).unwrap();

    for destination in [source_copy.clone(), scratch.path().join(".").join("source.mp4")] {
        let result = VideoExport::start(
            &toolchain,
            &plan,
            std::slice::from_ref(&source_copy),
            &destination,
            Arc::new(|| {}),
        );
        assert!(
            matches!(result, Err(ExportError::DestinationIsASourceFile { .. })),
            "{destination:?}"
        );
    }
    assert_eq!(std::fs::read(&source_copy).unwrap(), original_bytes);
    assert_eq!(
        std::fs::read_dir(scratch.path()).unwrap().count(),
        1,
        "no stray files were made"
    );
}

#[test]
fn a_failed_export_reports_why_and_leaves_an_existing_file_untouched() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let project = project_for(&toolchain, &video);
    let scratch = scratch_directory();
    let destination = scratch.path().join("precious.mp4");
    std::fs::write(&destination, b"the user's earlier export").unwrap();
    let missing_source = scratch.path().join("gone.mp4");

    let outcome = export_and_wait(&toolchain, &project, &missing_source, &destination);

    match outcome.result {
        ExportResult::Failed { reason } => assert!(!reason.is_empty()),
        other => panic!("expected a failure, got {other:?}"),
    }
    assert_eq!(std::fs::read(&destination).unwrap(), b"the user's earlier export");
    let leftover: Vec<String> = std::fs::read_dir(scratch.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        leftover,
        vec!["precious.mp4".to_owned()],
        "no temporary files are left behind"
    );
}

#[test]
fn a_successful_export_replaces_an_existing_file() {
    let toolchain = toolchain_or_skip!();
    let video = counter_video(&toolchain);
    let mut project = project_for(&toolchain, &video);
    support::set_clip_speed(&mut project, 0, 50);
    let scratch = scratch_directory();
    let destination = scratch.path().join("replaced.mp4");
    std::fs::write(&destination, b"old contents").unwrap();

    let outcome = export_and_wait(&toolchain, &project, &video.path, &destination);

    assert_eq!(outcome.result, ExportResult::Completed);
    assert_eq!(
        decode_video(&toolchain, &destination, 320, 180).frame_numbers(),
        expected_source_frames(&project)
    );
    assert_eq!(
        std::fs::read_dir(scratch.path()).unwrap().count(),
        1,
        "only the finished video remains"
    );
}

#[test]
fn cancelling_stops_the_export_and_leaves_no_file_and_an_existing_file_untouched() {
    let toolchain = toolchain_or_skip!();
    // A long, large video so that the export is still running when it is cancelled.
    let video = test_video(
        &toolchain,
        &TestVideoSpecification {
            name: "long_big_120_seconds",
            width_in_pixels: 1280,
            height_in_pixels: 720,
            duration_in_seconds: 120,
            with_audio: true,
            ..TestVideoSpecification::COUNTER
        },
    );
    let project = project_for(&toolchain, &video);
    let scratch = scratch_directory();
    let destination = scratch.path().join("cancelled.mp4");
    std::fs::write(&destination, b"the user's earlier export").unwrap();
    let plan = plan_export(&project, Settings::DEFAULT, &transparent_text_picture).unwrap();
    let export = VideoExport::start(
        &toolchain,
        &plan,
        std::slice::from_ref(&video.path),
        &destination,
        Arc::new(|| {}),
    )
    .unwrap();

    std::thread::sleep(Duration::from_millis(400));
    export.cancel();
    let outcome = wait_for_export(&export);

    assert_eq!(outcome.result, ExportResult::Cancelled);
    assert_eq!(std::fs::read(&destination).unwrap(), b"the user's earlier export");
    let leftover: Vec<String> = std::fs::read_dir(scratch.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        leftover,
        vec!["cancelled.mp4".to_owned()],
        "the half-written file was removed"
    );
}

#[test]
fn dropping_a_running_export_cancels_it() {
    let toolchain = toolchain_or_skip!();
    let video = test_video(
        &toolchain,
        &TestVideoSpecification {
            name: "long_big_120_seconds",
            width_in_pixels: 1280,
            height_in_pixels: 720,
            duration_in_seconds: 120,
            with_audio: true,
            ..TestVideoSpecification::COUNTER
        },
    );
    let project = project_for(&toolchain, &video);
    let scratch = scratch_directory();
    let destination = scratch.path().join("dropped.mp4");
    let plan = plan_export(&project, Settings::DEFAULT, &transparent_text_picture).unwrap();
    let export = VideoExport::start(
        &toolchain,
        &plan,
        std::slice::from_ref(&video.path),
        &destination,
        Arc::new(|| {}),
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(300));

    drop(export);

    // Dropping waits for the clean-up, so there is nothing left the moment it returns. That
    // matters when the whole program is about to exit and could not finish the clean-up itself.
    assert_eq!(
        std::fs::read_dir(scratch.path()).unwrap().count(),
        0,
        "the half-written file was removed"
    );
    assert!(!destination.exists());
}
