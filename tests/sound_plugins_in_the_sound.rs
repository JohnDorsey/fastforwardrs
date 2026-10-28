// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sound plugins in the sound (phase 7, piece 7.6), with the real ffmpeg for the clips and the plugin library written for the tests: **the export and the preview make the same
//! sound, and it is the arithmetic of the plugin**, a value that has keyframes is what the keyframes say, a folder that nobody has said yes to leaves the sound as it was and is
//! asked about, and a plugin that cannot be loaded is said once.
//!
//! The list of the plugins that are installed is for the whole program, so the tests of this file do not run at the same time.

mod support;

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use fastforwardrs::composition::LayerEffect;
use fastforwardrs::media::export::{
    LoudnessToExport, SoundClipToExport, SoundGraphSpecification, SoundItemToExport, SoundStripToExport,
};
use fastforwardrs::media::{
    AudioFormat, AudioStreamEvent, AudioStreamRequest, EngineAudioStream, FfmpegToolchain,
    render_sound_to_file_with_plugins,
};
use fastforwardrs::plugin_host::PluginHost;
use fastforwardrs::plugin_scan::effect_of_a_plugin;
use fastforwardrs::project::{
    Animation, EffectInstanceId, EffectKind, FrameRate, Keyframe, Keyframes, PropertyId, PropertyValue,
    SpedUpClipSound, SpeedFactor, set_installed_plugins,
};
use fastforwardrs::sound_engine::PluginRunner;
use ladspa_test_plugins::library_file_for_tests;
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

/// The kind of effect of the plugin `label` of the test library, with the test plugins installed.
fn the_kind_of(label: &str) -> EffectKind {
    let path = library_file_for_tests();
    let scan = ladspa_host::scan_library(&path, |_| {}).expect("the test library is scanned");
    set_installed_plugins(
        scan.plugins
            .iter()
            .filter_map(|plugin| effect_of_a_plugin(&path, plugin).ok())
            .collect(),
    );
    EffectKind::installed_plugin_kinds()
        .into_iter()
        .find(|kind| kind.key().ends_with(&format!(":{label}")))
        .unwrap_or_else(|| panic!("no plugin called {label}"))
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
            })],
            buses: Vec::new(),
        }],
        sped_up_clip_sound: SpedUpClipSound::MaintainPitch,
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
fn the_export_and_the_preview_make_the_same_sound_and_it_is_the_arithmetic_of_the_plugin() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let gain = a_gain(the_kind_of("ffrs_test_gain"), 0.5);

    let (plain, _) = by_the_export(&toolchain, &specification_of_a_second_with(Vec::new()), &sources, None);
    assert_eq!(plain.len(), 48_000 * 2, "a second of stereo");
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
fn a_value_that_has_keyframes_is_what_the_keyframes_say_in_the_export_and_in_the_preview() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let kind = the_kind_of("ffrs_test_gain");
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
        let frame = (moment / 32 * 32) as f64 / 1_600.0;
        let frame = frame.floor();
        let gain = 0.5 + 1.5 * frame / 30.0;
        for channel in 0..2 {
            let (made, was) = (exported[moment * 2 + channel], plain[moment * 2 + channel]);
            assert!(
                (f64::from(made) - f64::from(was) * gain).abs() < 1e-5,
                "sample {moment} of channel {channel}: {made} and {was} times {gain}"
            );
        }
    }
    assert_ne!(
        exported[0] / plain[0],
        exported[47_998] / plain[47_998],
        "the gain changed"
    );
}

#[test]
fn a_folder_that_nobody_has_said_yes_to_leaves_the_sound_as_it_was_and_is_asked_about() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let gain = a_gain(the_kind_of("ffrs_test_gain"), 0.5);
    let specification = specification_of_a_second_with(vec![gain]);
    let (plain, _) = by_the_export(&toolchain, &specification_of_a_second_with(Vec::new()), &sources, None);

    let host = Arc::new(PluginHost::new());
    let (exported, notes) = by_the_export(&toolchain, &specification, &sources, Some(host.clone()));
    assert_eq!(exported, plain, "nobody said yes: the sound goes through unchanged");
    assert!(notes.is_empty(), "(a person is asked, and told nothing)");
    let wanted = host.take_wanted();
    assert_eq!(wanted.len(), 1, "{wanted:?}");
    assert_eq!(wanted[0].plugins, ["test plugin ffrs_test_gain"]);

    // Then the folder is approved: the sound is made again, and has the plugin in it.
    let library = library_file_for_tests();
    host.approve_folder(&fastforwardrs::plugin_trust::folder_of(&library));
    let (with_the_plugin, _) = by_the_export(&toolchain, &specification, &sources, Some(host));
    assert_eq!(
        with_the_plugin,
        plain.iter().map(|sample| sample * 0.5).collect::<Vec<f32>>()
    );
}

#[test]
fn a_plugin_that_cannot_be_loaded_is_said_once_in_the_export_and_in_the_preview_and_the_sound_goes_through_unchanged() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let path = library_file_for_tests();
    let scan = ladspa_host::scan_library(&path, |_| {}).unwrap();
    let mut plugin = effect_of_a_plugin(&path, &scan.plugins[0]).unwrap();
    plugin.library = folder.path().join("a_library_that_is_not_there.dll");
    set_installed_plugins(vec![plugin]);
    let kind = EffectKind::installed_plugin_kinds()[0];
    let specification = specification_of_a_second_with(vec![a_gain(kind, 0.5)]);
    let (plain, _) = by_the_export(&toolchain, &specification_of_a_second_with(Vec::new()), &sources, None);

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

    assert_eq!(exported, plain);
    assert_eq!(previewed, plain);
    for notes in [export_notes, preview_notes] {
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(
            notes[0].contains("could not be loaded") && notes[0].contains("a_library_that_is_not_there"),
            "{notes:?}"
        );
    }
}

#[test]
fn a_clip_with_no_plugin_effects_is_made_by_a_host_exactly_as_it_is_made_without_one() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let specification = specification_of_a_second_with(Vec::new());
    let (without, _) = by_the_export(&toolchain, &specification, &sources, None);
    let (with, notes) = by_the_export(
        &toolchain,
        &specification,
        &sources,
        Some(a_host_that_runs_everything()),
    );
    assert_eq!(with, without);
    assert!(notes.is_empty());
}

#[test]
fn a_plugin_that_is_too_slow_is_left_out_of_the_preview_and_said_and_the_export_waits_for_it() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    // The plugin that passes the sound through, and takes 30 milliseconds of the 42 that a block of sound lasts for each of the two channels.
    let slow = a_gain(the_kind_of("ffrs_test_counted"), 0.3);
    let specification = specification_of_frames_with(90, vec![slow]);
    let (plain, _) = by_the_export(
        &toolchain,
        &specification_of_frames_with(90, Vec::new()),
        &sources,
        None,
    );

    let (previewed, preview_notes) = by_the_preview(
        &toolchain,
        &specification,
        &sources,
        Some(a_host_that_runs_everything()),
    );
    let (exported, export_notes) = by_the_export(
        &toolchain,
        &specification,
        &sources,
        Some(a_host_that_runs_everything()),
    );

    assert_eq!(preview_notes.len(), 1, "{preview_notes:?}");
    assert!(
        preview_notes[0].contains("too slow")
            && preview_notes[0].contains("2 seconds")
            && preview_notes[0].contains("unchanged"),
        "{preview_notes:?}"
    );
    assert!(export_notes.is_empty(), "the export waited for it: {export_notes:?}");
    assert_eq!(
        (previewed, exported),
        (plain.clone(), plain),
        "the plugin passes the sound through"
    );
}

#[test]
fn the_starter_of_the_editor_makes_the_sound_with_the_plugins_it_is_given_and_says_what_may_be_run() {
    use fastforwardrs::playback::{AudioSource, AudioStreamStarter, FfmpegAudioStreamStarter};
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let gain = a_gain(the_kind_of("ffrs_test_gain"), 0.5);
    let (plain, _) = by_the_export(&toolchain, &specification_of_a_second_with(Vec::new()), &sources, None);
    let host = Arc::new(PluginHost::new());
    let starter =
        FfmpegAudioStreamStarter::new(Some(Arc::new(toolchain.clone())), Arc::new(|| {})).with_plugins(host.clone());
    assert_eq!(starter.generation(), host.generation());
    host.allow_everything();
    assert_eq!(starter.generation(), host.generation(), "it says what the host says");
    let request = AudioStreamRequest {
        source_files: sources.iter().map(|path| path.clone().into()).collect(),
        sound: specification_of_a_second_with(vec![gain]),
        format: STEREO_AT_48000,
    };

    let stream = starter.start(&request).expect("the stream starts");
    let started = Instant::now();
    let mut samples = Vec::new();
    loop {
        match stream.try_next_event() {
            Some(AudioStreamEvent::Samples(more)) => samples.extend(more),
            Some(AudioStreamEvent::Finished { error_output }) => {
                assert_eq!(error_output, None);
                break;
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

    assert_eq!(samples, plain.iter().map(|sample| sample * 0.5).collect::<Vec<f32>>());
}
