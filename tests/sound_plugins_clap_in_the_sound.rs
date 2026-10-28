// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! CLAP plugins in the sound (phase 7, piece 7.8.4), with the real ffmpeg for the clips and the CLAP library written for the tests: **the export and the preview make the same
//! sound, and it is the arithmetic of the plugin**, a value that has keyframes is what the keyframes say, **a plugin that delays the sound is made up for to the sample**, and a
//! folder that nobody has said yes to leaves the sound as it was.
//!
//! The list of the plugins that are installed is for the whole program, so the tests of this file do not run at the same time.

mod support;

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use clap_test_plugins::library_file_for_tests;
use fastforwardrs::composition::LayerEffect;
use fastforwardrs::media::export::{
    LoudnessToExport, SoundClipToExport, SoundGraphSpecification, SoundItemToExport, SoundStripToExport,
};
use fastforwardrs::media::{
    AudioFormat, AudioStreamEvent, AudioStreamRequest, EngineAudioStream, FfmpegToolchain,
    render_sound_to_file_with_plugins,
};
use fastforwardrs::plugin_host::PluginHost;
use fastforwardrs::plugin_scan::effect_of_a_clap_plugin;
use fastforwardrs::project::{
    Animation, EffectInstanceId, EffectKind, FrameRate, Keyframe, Keyframes, PropertyId, PropertyValue,
    SpedUpClipSound, SpeedFactor, set_installed_plugins,
};
use fastforwardrs::sound_engine::PluginRunner;
use support::scratch_directory;
use support::sound_files::the_sources;

const STEREO_AT_48000: AudioFormat = AudioFormat {
    sample_rate: 48_000,
    channel_count: 2,
};

fn the_installed_plugins_to_myself() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    set_installed_plugins(Vec::new());
    guard
}

fn thirty() -> FrameRate {
    FrameRate::new(30, 1).unwrap()
}

/// The kind of effect of the CLAP test plugin `id`, with the CLAP test plugins installed.
fn the_kind_of(id: &str) -> EffectKind {
    let path = library_file_for_tests();
    let scan = clap_host::scan::scan_library(&path, |_| {}).expect("the test library is scanned");
    set_installed_plugins(
        scan.plugins
            .iter()
            .filter_map(|plugin| effect_of_a_clap_plugin(&path, plugin).ok())
            .collect(),
    );
    let key = format!("clap:{id}");
    EffectKind::installed_plugin_kinds()
        .into_iter()
        .find(|kind| kind.key() == key)
        .unwrap_or_else(|| panic!("no plugin called {id}"))
}

fn a_gain(kind: EffectKind, gain: f32) -> LayerEffect {
    LayerEffect {
        state: None,
        clock: Default::default(),
        instance: EffectInstanceId::FIRST,
        kind,
        values: vec![PropertyValue::Number(gain)],
        animation: Animation::default(),
        tables: Vec::new(),
        tail_length: Default::default(),
        shared_by: None,
    }
}

/// A gain that is `first.1` at the timeline frame `first.0` and `last.1` at the frame `last.0`, between which it changes evenly.
fn an_animated_gain(kind: EffectKind, first: (u64, f32), last: (u64, f32)) -> LayerEffect {
    let mut effect = a_gain(kind, 1.0);
    let keyframe = |(frame, value): (u64, f32)| Keyframe {
        frame,
        value: PropertyValue::Number(value),
    };
    effect.animation = Animation::from_keyframes([(
        PropertyId::effect_value(kind, EffectInstanceId::FIRST, 0),
        Keyframes::from_unordered([keyframe(first), keyframe(last)]),
    )]);
    effect
}

/// The sound of 30 frames (a second) of the first file of noise, with `effects` done to it.
fn specification_of_a_second_with(effects: Vec<LayerEffect>) -> SoundGraphSpecification {
    specification_of_frames_with(30, effects)
}

/// The sound of `frames` frames of the first file of noise, with `effects` done to it.
fn specification_of_frames_with(frames: u64, effects: Vec<LayerEffect>) -> SoundGraphSpecification {
    SoundGraphSpecification {
        frame_rate: thirty(),
        total_frame_count: frames,
        strips: vec![SoundStripToExport {
            items: vec![SoundItemToExport::Clip(SoundClipToExport {
                source_input: 0,
                first_source_frame: 0,
                end_source_frame: frames,
                speed_factor: SpeedFactor::new(1).unwrap(),
                timeline_frame_count: frames,
                loudness: LoudnessToExport::UNCHANGED,
                first_timeline_frame: 0,
                frames_of_the_clip_before: 0,
                effects,
                sent_to: None,
                tail_routes: Vec::new(),
                music: None,
            })],
            buses: Vec::new(),
            loudness: Default::default(),
            muted: Vec::new(),
        }],
        sped_up_clip_sound: SpedUpClipSound::MaintainPitch,
        lead_in_frames: 0,
        groups: Vec::new(),
        edge_fade_milliseconds: 10,
    }
}

/// What the export makes of `specification`: the sound in a file, as the video is made from, read back; and what the plugins said.
fn by_the_export(
    toolchain: &FfmpegToolchain,
    specification: &SoundGraphSpecification,
    sources: &[PathBuf],
    plugins: Option<Arc<dyn PluginRunner>>,
) -> (Vec<f32>, Vec<String>) {
    let folder = scratch_directory();
    let file = folder.path().join("sound.f32");
    let notes = render_sound_to_file_with_plugins(
        toolchain,
        specification,
        sources.iter().map(|path| path.clone().into()).collect(),
        &file,
        &|| false,
        plugins,
    )
    .expect("the sound is made");
    let bytes = fs::read(&file).expect("the file of sound is there");
    let samples = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|sample| f32::from_le_bytes(*sample))
        .collect();
    (samples, notes)
}

/// What the preview makes of `specification`: the events of a stream, put together; and what the plugins said by the time it ended.
fn by_the_preview(
    toolchain: &FfmpegToolchain,
    specification: &SoundGraphSpecification,
    sources: &[PathBuf],
    plugins: Option<Arc<dyn PluginRunner>>,
) -> (Vec<f32>, Vec<String>) {
    let request = AudioStreamRequest {
        source_files: sources.iter().map(|path| path.clone().into()).collect(),
        sound: specification.clone(),
        format: STEREO_AT_48000,
    };
    let stream = EngineAudioStream::start_with_plugins(toolchain, &request, Arc::new(|| {}), plugins)
        .expect("the stream starts");
    let started = Instant::now();
    let mut samples = Vec::new();
    loop {
        match stream.try_next_event() {
            Some(AudioStreamEvent::Samples(more)) => samples.extend(more),
            Some(AudioStreamEvent::Finished { error_output }) => {
                assert_eq!(error_output, None, "the engine made the sound");
                let notes = stream.take_notes();
                assert!(stream.take_notes().is_empty(), "what was taken is not given again");
                return (samples, notes);
            }
            None => {
                assert!(
                    started.elapsed() < Duration::from_secs(60),
                    "the stream did not finish in time"
                );
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
}

fn a_host_that_runs_everything() -> Arc<dyn PluginRunner> {
    let host = PluginHost::new();
    host.allow_everything();
    Arc::new(host)
}

#[test]
fn the_export_and_the_preview_make_the_same_sound_with_a_clap_plugin_and_it_is_the_arithmetic_of_the_plugin() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let gain = a_gain(the_kind_of(clap_test_plugins::GAIN), 0.5);

    let (plain, _) = by_the_export(&toolchain, &specification_of_a_second_with(Vec::new()), &sources, None);
    assert!(
        plain.iter().any(|sample| sample.abs() > 0.1),
        "the noise is there to be changed"
    );
    let specification = specification_of_a_second_with(vec![gain]);

    let (exported, export_notes) = by_the_export(
        &toolchain,
        &specification,
        &sources,
        Some(a_host_that_runs_everything()),
    );
    let (previewed, preview_notes) = by_the_preview(
        &toolchain,
        &specification,
        &sources,
        Some(a_host_that_runs_everything()),
    );

    let half: Vec<f32> = plain.iter().map(|sample| sample * 0.5).collect();
    assert_eq!(
        exported, half,
        "the export: the sound of the file, times the gain, to the sample"
    );
    assert_eq!(previewed, exported, "the preview makes the sound the export makes");
    assert_eq!(
        (export_notes, preview_notes),
        (Vec::<String>::new(), Vec::<String>::new())
    );
}

#[test]
fn the_state_that_the_project_keeps_for_a_clap_plugin_is_given_to_it_in_the_export_and_in_the_preview_and_one_that_it_cannot_take_is_said()
 {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let (plain, _) = by_the_export(&toolchain, &specification_of_a_second_with(Vec::new()), &sources, None);
    let editor = the_kind_of(clap_test_plugins::EDITOR);

    // (The test plugin hears an edit that its state says was made as half a gain more: a gain of 0.5 and one edit is 0.5 * 1.5.)
    let mut with_a_state = a_gain(editor, 0.5);
    with_a_state.state = Some(Arc::from(b"edits=1".as_slice()));
    let specification = specification_of_a_second_with(vec![with_a_state]);
    let (exported, export_notes) = by_the_export(
        &toolchain,
        &specification,
        &sources,
        Some(a_host_that_runs_everything()),
    );
    let (previewed, preview_notes) = by_the_preview(
        &toolchain,
        &specification,
        &sources,
        Some(a_host_that_runs_everything()),
    );
    let expected: Vec<f32> = plain.iter().map(|sample| sample * 0.75).collect();
    assert_eq!(exported, expected, "the export: the state was given before the value");
    assert_eq!(previewed, exported, "the preview too");
    assert_eq!((export_notes, preview_notes), (Vec::new(), Vec::new()));

    let without = specification_of_a_second_with(vec![a_gain(editor, 0.5)]);
    let (exported, _) = by_the_export(&toolchain, &without, &sources, Some(a_host_that_runs_everything()));
    let half: Vec<f32> = plain.iter().map(|sample| sample * 0.5).collect();
    assert_eq!(exported, half, "with no state, the sound is of the value alone");

    // A plugin that has no state cannot take one: the sound is of its value, and it is said.
    let mut cannot_take = a_gain(the_kind_of(clap_test_plugins::GAIN), 0.5);
    cannot_take.state = Some(Arc::from(b"edits=1".as_slice()));
    let (exported, notes) = by_the_export(
        &toolchain,
        &specification_of_a_second_with(vec![cannot_take]),
        &sources,
        Some(a_host_that_runs_everything()),
    );
    assert_eq!(exported, half);
    assert!(
        notes.iter().any(|note| note.contains("did not take the state")),
        "{notes:?}"
    );
}

#[test]
fn a_clap_value_that_has_keyframes_is_what_the_keyframes_say_in_the_export_and_in_the_preview() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let kind = the_kind_of(clap_test_plugins::GAIN);
    let (plain, _) = by_the_export(&toolchain, &specification_of_a_second_with(Vec::new()), &sources, None);
    // The gain is 0.5 at the frame 0 and 2 at the frame 30: it rises through the second.
    let specification = specification_of_a_second_with(vec![an_animated_gain(kind, (0, 0.5), (30, 2.0))]);

    let (exported, _) = by_the_export(
        &toolchain,
        &specification,
        &sources,
        Some(a_host_that_runs_everything()),
    );
    let (previewed, _) = by_the_preview(
        &toolchain,
        &specification,
        &sources,
        Some(a_host_that_runs_everything()),
    );

    assert_eq!(previewed, exported);
    assert_eq!(exported.len(), plain.len());
    for moment in [0_usize, 31, 32, 1_599, 1_600, 24_000, 30_000, 47_999] {
        // (The value is looked at every 32 samples, at the frame the first of them is in.)
        let frame = ((moment / 32 * 32) as f64 / 1_600.0).floor();
        let gain = 0.5 + 1.5 * frame / 30.0;
        for channel in 0..2 {
            let expected = plain[moment * 2 + channel] * gain as f32;
            let made = exported[moment * 2 + channel];
            assert!(
                (made - expected).abs() <= 1e-5,
                "sample {moment}, channel {channel}: {made} and {expected}"
            );
        }
    }
}

#[test]
fn a_clap_plugin_that_delays_the_sound_is_made_up_for_in_the_export_and_in_the_preview_to_the_sample() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let delaying = LayerEffect {
        state: None,
        clock: Default::default(),
        instance: EffectInstanceId::FIRST,
        kind: the_kind_of(clap_test_plugins::LATENCY),
        values: Vec::new(),
        animation: Animation::default(),
        tables: Vec::new(),
        tail_length: Default::default(),
        shared_by: None,
    };
    let (plain, _) = by_the_export(&toolchain, &specification_of_a_second_with(Vec::new()), &sources, None);
    let specification = specification_of_a_second_with(vec![delaying]);

    let (exported, export_notes) = by_the_export(
        &toolchain,
        &specification,
        &sources,
        Some(a_host_that_runs_everything()),
    );
    let (previewed, preview_notes) = by_the_preview(
        &toolchain,
        &specification,
        &sources,
        Some(a_host_that_runs_everything()),
    );

    assert_eq!(
        exported, plain,
        "the plugin delays the sound by 64 samples and the sound is where it was"
    );
    assert_eq!(previewed, plain);
    assert_eq!(
        (export_notes, preview_notes),
        (Vec::<String>::new(), Vec::<String>::new())
    );
}

#[test]
fn a_folder_that_nobody_has_said_yes_to_leaves_the_sound_of_a_clap_plugin_as_it_was_and_asks() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let gain = a_gain(the_kind_of(clap_test_plugins::GAIN), 0.5);
    let (plain, _) = by_the_export(&toolchain, &specification_of_a_second_with(Vec::new()), &sources, None);
    let host = Arc::new(PluginHost::new());

    let (made, notes) = by_the_export(
        &toolchain,
        &specification_of_a_second_with(vec![gain]),
        &sources,
        Some(Arc::clone(&host) as Arc<dyn PluginRunner>),
    );

    assert_eq!(made, plain, "nothing was run");
    assert!(notes.is_empty(), "{notes:?}");
    let wanted = host.take_wanted();
    assert_eq!(wanted.len(), 1, "the folder of the library is asked about");
    assert!(host.loaded_libraries().is_empty());
}

/// The sound of the first file of noise from the source frame `first_source_frame` for `frames` frames, with `effects` done to it.
fn specification_from_frame(
    first_source_frame: u64,
    frames: u64,
    effects: Vec<LayerEffect>,
) -> SoundGraphSpecification {
    let mut specification = specification_of_frames_with(frames, effects);
    let SoundItemToExport::Clip(clip) = &mut specification.strips[0].items[0] else {
        unreachable!("the strip is one clip");
    };
    clip.first_source_frame = first_source_frame;
    // (The clip begins at the start of its file: the stretch begins `first_source_frame` frames into it.)
    clip.frames_of_the_clip_before = first_source_frame;
    clip.end_source_frame = first_source_frame + frames;
    specification
}

#[test]
fn a_clip_that_begins_in_the_middle_of_an_echo_has_the_sound_of_playing_through_to_it_in_the_export_and_in_the_preview()
{
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let echo = |pre_roll_samples: usize| {
        // The echo of the test library says a tail of 1000 samples, which is what it asks for; a test can ask for less.
        let kind = the_kind_of(clap_test_plugins::TAIL);
        let mut plugin = kind.installed_plugin().unwrap().clone();
        plugin.pre_roll_samples = pre_roll_samples;
        set_installed_plugins(vec![plugin]);
        LayerEffect {
            state: None,
            clock: Default::default(),
            instance: EffectInstanceId::FIRST,
            kind: EffectKind::installed_plugin_kinds()[0],
            values: Vec::new(),
            animation: Animation::default(),
            tables: Vec::new(),
            tail_length: Default::default(),
            shared_by: None,
        }
    };
    let host = || Some(a_host_that_runs_everything());
    let asked_for = clap_test_plugins::TAIL_SAMPLES as usize;

    // Played through: two seconds of noise through the echo. Then the second second alone, which is where a seek to the middle begins.
    let (through, _) = by_the_export(
        &toolchain,
        &specification_from_frame(0, 60, vec![echo(asked_for)]),
        &sources,
        host(),
    );
    let second_half = &through[30 * 1600 * 2..];
    let from_the_middle = specification_from_frame(30, 30, vec![echo(asked_for)]);
    let (exported, export_notes) = by_the_export(&toolchain, &from_the_middle, &sources, host());
    let (previewed, _) = by_the_preview(&toolchain, &from_the_middle, &sources, host());

    assert_eq!(exported.len(), second_half.len());
    let worst = exported
        .iter()
        .zip(second_half)
        .map(|(made, was)| (made - was).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        worst < 1e-4,
        "the export from the middle is the second half of playing through: the worst difference is {worst}"
    );
    assert_eq!(previewed, exported, "and the preview makes what the export makes");
    assert!(export_notes.is_empty(), "{export_notes:?}");

    // With no sound from before the clip, the echo begins empty, and the first moments are not what they were (the control: the test can tell).
    let (without, _) = by_the_export(
        &toolchain,
        &specification_from_frame(30, 30, vec![echo(0)]),
        &sources,
        host(),
    );
    let worst_at_the_start = without[..400]
        .iter()
        .zip(&second_half[..400])
        .map(|(made, was)| (made - was).abs())
        .fold(0.0_f32, f32::max);
    assert!(worst_at_the_start > 0.05, "{worst_at_the_start}");
}

/// A runner that makes nothing and writes down when a stream tells it that it is being stopped.
#[derive(Default)]
struct WritesDownWhenAStreamStops {
    told: std::sync::Mutex<Vec<fastforwardrs::sound_engine::Notes>>,
}

impl PluginRunner for WritesDownWhenAStreamStops {
    fn processor_for(
        &self,
        _effect: &LayerEffect,
        _clip: &SoundClipToExport,
        _frame_rate: FrameRate,
        _notes: &fastforwardrs::sound_engine::Notes,
        _is_for_the_preview: bool,
    ) -> Option<Box<dyn fastforwardrs::sound_engine::BlockProcessor>> {
        None
    }

    fn stream_is_stopping(&self, notes: &fastforwardrs::sound_engine::Notes) {
        self.told.lock().unwrap().push(Arc::clone(notes));
    }
}

#[test]
fn a_stream_of_the_preview_tells_the_runner_of_its_plugins_when_it_is_stopped_with_the_notes_that_name_it() {
    let toolchain = toolchain_or_skip!();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let runner = Arc::new(WritesDownWhenAStreamStops::default());
    let request = AudioStreamRequest {
        source_files: sources.iter().map(|path| path.clone().into()).collect(),
        sound: specification_of_a_second_with(Vec::new()),
        format: STEREO_AT_48000,
    };
    let stream = EngineAudioStream::start_with_plugins(
        &toolchain,
        &request,
        Arc::new(|| {}),
        Some(Arc::clone(&runner) as Arc<dyn PluginRunner>),
    )
    .expect("the stream starts");
    assert!(runner.told.lock().unwrap().is_empty(), "not while it plays");

    drop(stream);

    let told = runner.told.lock().unwrap();
    assert_eq!(told.len(), 1, "once, when it is stopped");
}

// --- Phase 12.0: what the tail of an effect does today, written down before it is changed (docs/engine-phase-12-design.md, section 1) ---
//
// Before phase 12 an effect was cut where its clip ended; piece 12.2c made it ring on for its tail. The tests below say so with the echo of the CLAP test library (it says a tail of 1000
// samples), and pin what a person who says that an effect has no tail gets: the effect cut where its clip ends.

/// The echo of the test library (it says a tail of 1000 samples, and its echoes are `ECHO_SAMPLES` apart), as an effect of a clip.
fn an_echo() -> LayerEffect {
    LayerEffect {
        state: None,
        clock: Default::default(),
        instance: EffectInstanceId::FIRST,
        kind: the_kind_of(clap_test_plugins::TAIL),
        values: Vec::new(),
        animation: Animation::default(),
        tables: Vec::new(),
        tail_length: Default::default(),
        shared_by: None,
    }
}

/// A strip of `items`, in a project of `frames` frames, made from the strip of a second of noise (whose clip is the one that every clip below is made from).
fn specification_of_items(
    frames: u64,
    items: impl FnOnce(&SoundClipToExport) -> Vec<SoundItemToExport>,
) -> SoundGraphSpecification {
    let mut specification = specification_of_frames_with(30, Vec::new());
    let SoundItemToExport::Clip(template) = &specification.strips[0].items[0] else {
        unreachable!("the strip is one clip");
    };
    specification.strips[0].items = items(template);
    specification.total_frame_count = frames;
    specification
}

const SAMPLES_IN_A_FRAME: usize = 1600 * 2;

/// What the echo of the test library (it says a tail of 1000 samples) does with a second of noise that is its clip, and then a second of silence, when its tail is `tail_length`: the
/// export, the preview, and what the preview and the export say.
fn the_echo_of_a_clip_and_a_second_of_silence(
    toolchain: &FfmpegToolchain,
    sources: &[PathBuf],
    tail_length: fastforwardrs::project::TailLength,
) -> (Vec<f32>, Vec<f32>, Vec<String>) {
    let specification = specification_of_items(60, |clip| {
        let mut with_the_echo = clip.clone();
        with_the_echo.effects = vec![LayerEffect {
            tail_length,
            ..an_echo()
        }];
        vec![
            SoundItemToExport::Clip(with_the_echo),
            SoundItemToExport::Silence { frame_count: 30 },
        ]
    });
    let (exported, notes) = by_the_export(toolchain, &specification, sources, Some(a_host_that_runs_everything()));
    let (previewed, _) = by_the_preview(toolchain, &specification, sources, Some(a_host_that_runs_everything()));
    (exported, previewed, notes)
}

#[test]
fn the_echo_of_an_effect_goes_on_after_its_clip_for_the_tail_it_says_and_for_no_longer_and_none_is_cut_where_the_clip_ends()
 {
    use fastforwardrs::project::TailLength;

    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let (plain, _) = by_the_export(
        &toolchain,
        &specification_of_frames_with(30, Vec::new()),
        &sources,
        None,
    );

    // The echo says a tail of 1000 samples (2000 in the interleaved sound), which is what it rings on for.
    let (exported, previewed, notes) =
        the_echo_of_a_clip_and_a_second_of_silence(&toolchain, &sources, TailLength::Automatic);
    assert_eq!(exported.len(), 60 * SAMPLES_IN_A_FRAME);
    let (inside, after) = exported.split_at(30 * SAMPLES_IN_A_FRAME);
    let ring = clap_test_plugins::TAIL_SAMPLES as usize * 2;
    assert!(
        after[..ring].iter().any(|sample| sample.abs() > 0.001),
        "the echo rings on after the clip"
    );
    assert!(
        after[ring..].iter().all(|sample| *sample == 0.0),
        "and then nothing is heard: the tail it says is over"
    );
    assert!(
        inside
            .iter()
            .zip(&plain)
            .any(|(heard, said)| (heard - said).abs() > 0.05),
        "the echo is there inside the clip (the control: the test can tell)"
    );
    assert_eq!(previewed, exported, "the preview makes what the export makes");
    assert!(notes.is_empty(), "{notes:?}");

    // A person who says that it has no tail has the echo cut where the clip ends, as it always was.
    let (cut, cut_previewed, _) = the_echo_of_a_clip_and_a_second_of_silence(&toolchain, &sources, TailLength::None);
    assert!(cut[30 * SAMPLES_IN_A_FRAME..].iter().all(|sample| *sample == 0.0));
    assert_eq!(cut_previewed, cut);
    assert_eq!(
        &cut[..30 * SAMPLES_IN_A_FRAME],
        inside,
        "(and what is heard of the clip is the same)"
    );

    // One that gives a length: 20 milliseconds is 960 samples, so 1920 in the interleaved sound.
    let (twenty, _, _) = the_echo_of_a_clip_and_a_second_of_silence(&toolchain, &sources, TailLength::Milliseconds(20));
    assert!(
        twenty[30 * SAMPLES_IN_A_FRAME + 1920..]
            .iter()
            .all(|sample| *sample == 0.0)
    );
    assert!(
        twenty[30 * SAMPLES_IN_A_FRAME + 1500..30 * SAMPLES_IN_A_FRAME + 1920]
            .iter()
            .any(|sample| sample.abs() > 0.001),
        "it rings for the length that was said, which is not the one that the plugin says"
    );
}

#[test]
fn a_clip_that_follows_another_one_with_the_same_effect_has_an_echo_of_its_own_and_the_ring_of_the_one_before_is_added_to_its_start()
 {
    use fastforwardrs::project::TailLength;

    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let host = || Some(a_host_that_runs_everything());
    let two_clips = |tail_length: TailLength| {
        // The same second of noise twice, one clip right after the other, each with the echo in its own chain.
        specification_of_items(60, |clip| {
            let mut first = clip.clone();
            first.effects = vec![LayerEffect {
                tail_length,
                ..an_echo()
            }];
            let mut second = first.clone();
            second.first_timeline_frame = 30;
            vec![SoundItemToExport::Clip(first), SoundItemToExport::Clip(second)]
        })
    };

    // With the tail as the echo says it: the second clip begins with its own echo, from nothing, and with the ring of the first added to it.
    let (exported, _) = by_the_export(&toolchain, &two_clips(TailLength::Automatic), &sources, host());
    let (previewed, _) = by_the_preview(&toolchain, &two_clips(TailLength::Automatic), &sources, host());
    let (alone, _, _) = the_echo_of_a_clip_and_a_second_of_silence(&toolchain, &sources, TailLength::Automatic);
    let ring_of_the_first = &alone[30 * SAMPLES_IN_A_FRAME..30 * SAMPLES_IN_A_FRAME + 2000];
    let (first, second) = exported.split_at(30 * SAMPLES_IN_A_FRAME);
    let worst = second[..2000]
        .iter()
        .zip(first[..2000].iter().zip(ring_of_the_first))
        .map(|(heard, (own, ring))| (heard - (own + ring)).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        worst < 1e-4,
        "the start of the second clip is what it is alone, and the ring of the first clip: the worst difference is {worst}"
    );
    assert_ne!(first, second, "(the control: the ring is something)");
    assert_eq!(previewed, exported, "the preview makes what the export makes");

    // With no tail: the second clip sounds as the first does, and begins with an empty echo.
    let (plain, _) = by_the_export(
        &toolchain,
        &specification_of_frames_with(30, Vec::new()),
        &sources,
        None,
    );
    let (cut, _) = by_the_export(&toolchain, &two_clips(TailLength::None), &sources, host());
    let (first, second) = cut.split_at(30 * SAMPLES_IN_A_FRAME);
    assert_eq!(first, second, "the end of the first clip is not heard in the second");
    let echo_free_start = clap_test_plugins::ECHO_SAMPLES * 2;
    assert_eq!(
        &second[..echo_free_start],
        &plain[..echo_free_start],
        "the first echo of the second clip is still to come: its first moments are the file's"
    );
}

// --- Phase 12.2: one instance of the effects of a track for the clips of a run ---

#[test]
fn the_clips_of_a_run_are_heard_by_one_echo_so_two_clips_that_follow_each_other_sound_as_the_one_they_were_cut_from_does()
 {
    use fastforwardrs::media::export::SoundBusToExport;

    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let host = || Some(a_host_that_runs_everything());

    // Two seconds of noise through the echo, as one clip.
    let (played_through, _) = by_the_export(
        &toolchain,
        &specification_of_frames_with(60, vec![an_echo()]),
        &sources,
        host(),
    );

    // The same two seconds as two clips, a second each, one after the other, whose track has the echo: one instance of it hears both.
    let mut as_a_run = specification_of_frames_with(60, Vec::new());
    let SoundItemToExport::Clip(template) = as_a_run.strips[0].items[0].clone() else {
        unreachable!("the strip is one clip");
    };
    let mut second = template.clone();
    second.first_source_frame = 30;
    second.end_source_frame = 60;
    second.first_timeline_frame = 30;
    let mut first = template;
    first.end_source_frame = 30;
    first.timeline_frame_count = 30;
    as_a_run.strips[0].items = vec![
        SoundItemToExport::Clip(first.clone()),
        SoundItemToExport::Clip(second.clone()),
    ];
    let mut as_a_run_of_one_instance = as_a_run.clone();
    as_a_run_of_one_instance.strips[0].buses = vec![SoundBusToExport {
        first_item: 0,
        last_item: 1,
        effects: vec![an_echo()],
        tail_routes: Vec::new(),
        sent_to: None,
    }];

    let (exported, notes) = by_the_export(&toolchain, &as_a_run_of_one_instance, &sources, host());
    let (previewed, _) = by_the_preview(&toolchain, &as_a_run_of_one_instance, &sources, host());

    assert_eq!(exported.len(), played_through.len());
    let worst = exported
        .iter()
        .zip(&played_through)
        .map(|(made, was)| (made - was).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        worst < 1e-4,
        "one echo that hears the two clips sounds as the one clip does: the worst difference is {worst}"
    );
    assert_eq!(previewed, exported, "the preview makes what the export makes");
    assert!(notes.is_empty(), "{notes:?}");

    // The control, the other sharing, with no tail: an echo in the chain of each clip begins again at the second clip, whose first echoes are not those of what came before it.
    let mut per_clip = as_a_run;
    for item in &mut per_clip.strips[0].items {
        let SoundItemToExport::Clip(clip) = item else {
            unreachable!("two clips")
        };
        // (With no tail: with one that rings on, an echo is linear and the copies of it add up to what one instance makes, which the next test of this file says.)
        clip.effects = vec![LayerEffect {
            tail_length: fastforwardrs::project::TailLength::None,
            ..an_echo()
        }];
    }
    let (one_for_each, _) = by_the_export(&toolchain, &per_clip, &sources, host());
    let start_of_the_second = 30 * SAMPLES_IN_A_FRAME;
    let worst_at_the_start = one_for_each[start_of_the_second..start_of_the_second + 400]
        .iter()
        .zip(&played_through[start_of_the_second..start_of_the_second + 400])
        .map(|(made, was)| (made - was).abs())
        .fold(0.0_f32, f32::max);
    assert!(worst_at_the_start > 0.05, "{worst_at_the_start}");
}

#[test]
fn an_echo_for_each_clip_that_rings_on_and_one_echo_for_the_run_sound_the_same_because_an_echo_is_linear() {
    use fastforwardrs::media::export::SoundBusToExport;

    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let host = || Some(a_host_that_runs_everything());
    // (The echo says a tail of 1000 samples, where what it rings is a little more than a thousandth of the sound still: a tail of 200 milliseconds leaves nothing of it out.)
    let a_long_ringing_echo = || LayerEffect {
        tail_length: fastforwardrs::project::TailLength::Milliseconds(200),
        ..an_echo()
    };
    let mut run = specification_of_frames_with(60, Vec::new());
    let SoundItemToExport::Clip(template) = run.strips[0].items[0].clone() else {
        unreachable!("the strip is one clip");
    };
    let mut first = template.clone();
    first.end_source_frame = 30;
    first.timeline_frame_count = 30;
    let mut second = template;
    second.first_source_frame = 30;
    second.end_source_frame = 60;
    second.first_timeline_frame = 30;
    run.strips[0].items = vec![SoundItemToExport::Clip(first), SoundItemToExport::Clip(second)];
    let mut one_instance = run.clone();
    one_instance.strips[0].buses = vec![SoundBusToExport {
        first_item: 0,
        last_item: 1,
        effects: vec![a_long_ringing_echo()],
        tail_routes: Vec::new(),
        sent_to: None,
    }];
    let mut a_copy_for_each_clip = run;
    for item in &mut a_copy_for_each_clip.strips[0].items {
        let SoundItemToExport::Clip(clip) = item else {
            unreachable!("two clips")
        };
        clip.effects = vec![a_long_ringing_echo()];
    }

    let (shared, _) = by_the_export(&toolchain, &one_instance, &sources, host());
    let (separate, _) = by_the_export(&toolchain, &a_copy_for_each_clip, &sources, host());

    let worst = shared
        .iter()
        .zip(&separate)
        .map(|(one, other)| (one - other).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        worst < 1e-4,
        "the same sound for an effect that is linear: the worst difference is {worst}"
    );
}

#[test]
fn sound_that_is_wanted_from_a_frame_after_the_clip_has_the_ring_of_the_clip_through_a_lead_in_in_the_export_and_in_the_preview()
 {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let host = || Some(a_host_that_runs_everything());
    // A second of noise through the echo, and then a second of silence.
    let specification = specification_of_items(60, |clip| {
        let mut with_the_echo = clip.clone();
        with_the_echo.effects = vec![an_echo()];
        vec![
            SoundItemToExport::Clip(with_the_echo),
            SoundItemToExport::Silence { frame_count: 30 },
        ]
    });
    let (played_through, _) = by_the_export(&toolchain, &specification, &sources, host());
    // The same, wanted from the frame where the clip ends (frame 30), with the second before it made and not heard (a lead-in of 30 frames).
    let mut from_the_end_of_the_clip = specification;
    from_the_end_of_the_clip.lead_in_frames = 30;

    let (exported, notes) = by_the_export(&toolchain, &from_the_end_of_the_clip, &sources, host());
    let (previewed, _) = by_the_preview(&toolchain, &from_the_end_of_the_clip, &sources, host());

    let heard_from = 30 * SAMPLES_IN_A_FRAME;
    assert_eq!(exported.len(), played_through.len() - heard_from);
    let worst = exported
        .iter()
        .zip(&played_through[heard_from..])
        .map(|(made, was)| (made - was).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        worst < 1e-6,
        "the sound from the end of the clip is what it is when played through: {worst}"
    );
    assert!(
        exported[..clap_test_plugins::TAIL_SAMPLES as usize * 2]
            .iter()
            .any(|sample| sample.abs() > 0.001),
        "and the ring of the clip is heard in it (the control)"
    );
    assert_eq!(previewed, exported, "the preview makes what the export makes");
    assert!(notes.is_empty(), "{notes:?}");
}

// --- Phase 12.3: the effects of a group are done to the sum of what its members make ---

/// Two strips of a second of noise each, from two different files (one clip on each), and the echo that rings on for 200 milliseconds, which leaves nothing of it out.
fn two_strips_of_noise_and_a_long_echo() -> (SoundGraphSpecification, LayerEffect) {
    let mut specification = specification_of_frames_with(45, Vec::new());
    let mut other = specification.strips[0].clone();
    let SoundItemToExport::Clip(clip) = &mut other.items[0] else {
        unreachable!("the strip is one clip");
    };
    clip.source_input = 1;
    specification.strips.push(other);
    let echo = LayerEffect {
        tail_length: fastforwardrs::project::TailLength::Milliseconds(200),
        ..an_echo()
    };
    (specification, echo)
}

#[test]
fn the_echo_of_a_group_hears_the_sum_of_its_members_so_it_sounds_as_an_echo_for_each_of_them_does_because_an_echo_is_linear()
 {
    use fastforwardrs::media::export::SoundGroupToExport;

    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let host = || Some(a_host_that_runs_everything());
    let (two_strips, echo) = two_strips_of_noise_and_a_long_echo();

    // One echo for the group that both clips are sent to.
    let mut grouped = two_strips.clone();
    for strip in &mut grouped.strips {
        let SoundItemToExport::Clip(clip) = &mut strip.items[0] else {
            unreachable!("one clip")
        };
        clip.sent_to = Some(0);
    }
    grouped.groups = vec![SoundGroupToExport {
        effects: vec![echo.clone()],
        parent: None,
    }];
    // The control: an echo for each clip, and no group.
    let mut one_for_each = two_strips;
    for strip in &mut one_for_each.strips {
        let SoundItemToExport::Clip(clip) = &mut strip.items[0] else {
            unreachable!("one clip")
        };
        clip.effects = vec![echo.clone()];
    }
    let (plain, _) = by_the_export(
        &toolchain,
        &{
            let mut plain = one_for_each.clone();
            for strip in &mut plain.strips {
                let SoundItemToExport::Clip(clip) = &mut strip.items[0] else {
                    unreachable!("one clip")
                };
                clip.effects.clear();
            }
            plain
        },
        &sources,
        None,
    );

    let (by_the_group, notes) = by_the_export(&toolchain, &grouped, &sources, host());
    let (previewed, _) = by_the_preview(&toolchain, &grouped, &sources, host());
    let (by_the_clips, _) = by_the_export(&toolchain, &one_for_each, &sources, host());

    assert_eq!(by_the_group.len(), by_the_clips.len());
    let worst = by_the_group
        .iter()
        .zip(&by_the_clips)
        .map(|(one, other)| (one - other).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        worst < 1e-4,
        "the same sound for an effect that is linear: the worst difference is {worst}"
    );
    assert_eq!(previewed, by_the_group, "the preview makes what the export makes");
    assert!(notes.is_empty(), "{notes:?}");
    // The echo is there at all (a sound that was not given to the effects is the plain sum), and it rings on after the clips, which end at frame 45 of 45... so look inside them.
    let inside = 20 * SAMPLES_IN_A_FRAME;
    let difference_from_plain = by_the_group[inside..inside + 2000]
        .iter()
        .zip(&plain[inside..inside + 2000])
        .map(|(echoed, plain)| (echoed - plain).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        difference_from_plain > 0.05,
        "the echo was done: {difference_from_plain}"
    );
}

// --- Phase 12.3d: the tail policies of groups, with real plugins ---

#[test]
fn the_ring_of_a_clip_that_its_group_releases_is_not_done_to_by_the_effects_of_the_group_from_the_edge_on_in_the_export_and_in_the_preview()
 {
    use fastforwardrs::composition::{TailDestination, TailRoute};
    use fastforwardrs::media::export::SoundGroupToExport;

    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let host = || Some(a_host_that_runs_everything());
    // The clip is a second of noise with the echo, which rings on for 200 milliseconds (a tail long enough to be most of what it is) after it; the group halves what it is given.
    let long_echo = LayerEffect {
        tail_length: fastforwardrs::project::TailLength::Milliseconds(200),
        ..an_echo()
    };
    let halving = a_gain(the_kind_of(clap_test_plugins::GAIN), 0.5);
    let in_the_group = |routes: Vec<TailRoute>| {
        let mut specification = specification_of_frames_with(45, vec![long_echo.clone()]);
        let SoundItemToExport::Clip(clip) = &mut specification.strips[0].items[0] else {
            unreachable!("one clip");
        };
        // (The clip is the first second of a clip of a second and a half, which has silence after it for the ring.)
        clip.timeline_frame_count = 30;
        clip.end_source_frame = 30;
        clip.sent_to = Some(0);
        clip.tail_routes = routes;
        specification.groups = vec![SoundGroupToExport {
            effects: vec![halving.clone()],
            parent: None,
        }];
        specification.edge_fade_milliseconds = 0;
        specification
    };
    // What the clip makes with nothing round it: its echo, and after it the ring.
    let mut alone = specification_of_frames_with(45, vec![long_echo.clone()]);
    let SoundItemToExport::Clip(clip) = &mut alone.strips[0].items[0] else {
        unreachable!("one clip");
    };
    clip.timeline_frame_count = 30;
    clip.end_source_frame = 30;
    let (the_clip_by_itself, _) = by_the_export(&toolchain, &alone, &sources, host());

    let released = in_the_group(vec![TailRoute {
        from_frame: 30,
        to: TailDestination::Sum,
    }]);
    let (exported, notes) = by_the_export(&toolchain, &released, &sources, host());
    let (previewed, _) = by_the_preview(&toolchain, &released, &sources, host());
    let (kept_alive, _) = by_the_export(&toolchain, &in_the_group(Vec::new()), &sources, host());

    let edge = 30 * SAMPLES_IN_A_FRAME;
    assert_eq!(exported.len(), the_clip_by_itself.len());
    let worst = |made: &[f32], expected: &dyn Fn(usize) -> f32, range: std::ops::Range<usize>| {
        range
            .map(|index| (made[index] - expected(index)).abs())
            .fold(0.0_f32, f32::max)
    };
    assert!(
        worst(&exported, &|index| the_clip_by_itself[index] * 0.5, 0..edge) < 1e-6,
        "up to the edge the group halves what the clip makes"
    );
    assert!(
        worst(&exported, &|index| the_clip_by_itself[index], edge..edge + 2 * 9600) < 1e-6,
        "from the edge on the ring is the clip's own, not halved"
    );
    assert!(
        the_clip_by_itself[edge..edge + 2000]
            .iter()
            .any(|sample| sample.abs() > 0.01),
        "(a premise: there is a ring to be not halved)"
    );
    assert!(
        worst(
            &kept_alive,
            &|index| the_clip_by_itself[index] * 0.5,
            0..edge + 2 * 9600
        ) < 1e-6,
        "(the control: kept alive, which is what a group does if nobody says otherwise, the ring is halved too)"
    );
    assert_eq!(previewed, exported, "the preview makes what the export makes");
    assert!(notes.is_empty(), "{notes:?}");
}

// --- Phase 12.4: one instance for the whole track, and the windows that groups take of it ---

#[test]
fn a_group_takes_a_window_of_the_one_echo_of_a_track_and_what_still_rings_in_it_from_before_is_done_to_too_in_the_export_and_in_the_preview()
 {
    use fastforwardrs::composition::{TailDestination, TailRoute};
    use fastforwardrs::media::export::{SoundBusToExport, SoundGroupToExport};

    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let host = || Some(a_host_that_runs_everything());
    // A clip, three frames of silence, and a clip: the echo of the track (200 milliseconds, six frames) is still ringing from the first when the second begins.
    let long_echo = LayerEffect {
        tail_length: fastforwardrs::project::TailLength::Milliseconds(200),
        ..an_echo()
    };
    let one_echo = |windows: bool| {
        let mut specification = specification_of_frames_with(70, Vec::new());
        let SoundItemToExport::Clip(template) = specification.strips[0].items[0].clone() else {
            unreachable!("one clip");
        };
        let mut first = template.clone();
        first.timeline_frame_count = 30;
        first.end_source_frame = 30;
        let mut second = template;
        second.first_timeline_frame = 33;
        second.first_source_frame = 33;
        second.end_source_frame = 63;
        second.timeline_frame_count = 30;
        second.sent_to = windows.then_some(0);
        specification.strips[0].items = vec![
            SoundItemToExport::Clip(first),
            SoundItemToExport::Silence { frame_count: 3 },
            SoundItemToExport::Clip(second),
        ];
        specification.strips[0].buses = vec![SoundBusToExport {
            first_item: 0,
            last_item: 2,
            effects: vec![long_echo.clone()],
            sent_to: None,
            tail_routes: if windows {
                vec![
                    TailRoute {
                        from_frame: 33,
                        to: TailDestination::Group(0),
                    },
                    TailRoute {
                        from_frame: 63,
                        to: TailDestination::Sum,
                    },
                ]
            } else {
                Vec::new()
            },
        }];
        if windows {
            specification.groups = vec![SoundGroupToExport {
                effects: vec![a_gain(the_kind_of(clap_test_plugins::GAIN), 0.5)],
                parent: None,
            }];
        }
        specification.edge_fade_milliseconds = 0;
        specification
    };

    let (alone, _) = by_the_export(&toolchain, &one_echo(false), &sources, host());
    let windows = one_echo(true);
    let (exported, notes) = by_the_export(&toolchain, &windows, &sources, host());
    let (previewed, _) = by_the_preview(&toolchain, &windows, &sources, host());

    let (window_begins, window_ends) = (33 * SAMPLES_IN_A_FRAME, 63 * SAMPLES_IN_A_FRAME);
    assert_eq!(exported.len(), alone.len());
    let worst = |range: std::ops::Range<usize>, weight: f32| {
        range
            .map(|index| (exported[index] - alone[index] * weight).abs())
            .fold(0.0_f32, f32::max)
    };
    assert!(
        worst(0..window_begins, 1.0) < 1e-6,
        "outside the window the sound is the instance's, as it is"
    );
    assert!(
        worst(window_begins..window_ends, 0.5) < 1e-6,
        "inside it the group halves what the instance makes"
    );
    assert!(
        worst(window_ends..exported.len(), 1.0) < 1e-6,
        "and after it the sound is as it was"
    );
    assert!(
        alone[window_begins..window_begins + 3 * SAMPLES_IN_A_FRAME]
            .iter()
            .any(|sample| sample.abs() > 0.01),
        "(a premise: the echo of the first clip is still ringing when the window begins: it is in the window, and halved with the rest, which is what a window cannot help)"
    );
    assert_eq!(previewed, exported, "the preview makes what the export makes");
    assert!(notes.is_empty(), "{notes:?}");
}
