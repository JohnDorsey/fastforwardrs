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
        instance: EffectInstanceId::FIRST,
        kind,
        values: vec![PropertyValue::Number(gain)],
        animation: Animation::default(),
        tables: Vec::new(),
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
                effects,
            })],
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
        instance: EffectInstanceId::FIRST,
        kind: the_kind_of(clap_test_plugins::LATENCY),
        values: Vec::new(),
        animation: Animation::default(),
        tables: Vec::new(),
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
