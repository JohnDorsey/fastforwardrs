// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! LV2 plugins in the sound (piece 11.3 of `docs/engine-phase-11-lv2-design.md`), with the real ffmpeg for the clips and the LV2 bundle written for the tests: **the export and the
//! preview make the same sound, and it is the arithmetic of the plugin**, a value that has keyframes is what the keyframes say, **a plugin that delays the sound is made up for to
//! the sample**, the state that a project keeps is given to the plugin, and a folder that nobody has said yes to leaves the sound as it was.
//!
//! The list of the plugins that are installed is for the whole program, so the tests of this file do not run at the same time.

mod support;

use std::path::{Path, PathBuf};
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
use fastforwardrs::plugin_scan::effect_of_an_lv2_plugin;
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

/// The test bundle written in `folder`, scanned in this process, and its plugins that are effects installed; and the kind of effect of the plugin `uri`.
fn the_kind_of(folder: &Path, uri: &str) -> EffectKind {
    let bundle = lv2_test_plugins::write_bundle(folder);
    let scan = lv2_host::scan_bundle(&bundle, &mut |_| {}).expect("the bundle is scanned");
    set_installed_plugins(
        scan.plugins
            .iter()
            .filter_map(|plugin| effect_of_an_lv2_plugin(&bundle, plugin).ok())
            .collect(),
    );
    let key = fastforwardrs::plugin_scan::key_of_an_lv2_plugin(uri);
    EffectKind::installed_plugin_kinds()
        .into_iter()
        .find(|kind| kind.key() == key)
        .unwrap_or_else(|| panic!("no plugin called {uri}"))
}

fn an_effect(kind: EffectKind, values: Vec<PropertyValue>) -> LayerEffect {
    LayerEffect {
        state: None,
        clock: Default::default(),
        instance: EffectInstanceId::FIRST,
        kind,
        values,
        animation: Animation::default(),
        tables: Vec::new(),
    }
}

fn a_gain(kind: EffectKind, gain: f32) -> LayerEffect {
    an_effect(kind, vec![PropertyValue::Number(gain)])
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
    SoundGraphSpecification {
        frame_rate: thirty(),
        total_frame_count: 30,
        strips: vec![SoundStripToExport {
            items: vec![SoundItemToExport::Clip(SoundClipToExport {
                source_input: 0,
                first_source_frame: 0,
                end_source_frame: 30,
                speed_factor: SpeedFactor::new(1).unwrap(),
                timeline_frame_count: 30,
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
    let bytes = std::fs::read(&file).expect("the file of sound is there");
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
                return (samples, stream.take_notes());
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
fn the_export_and_the_preview_make_the_same_sound_with_an_lv2_plugin_and_it_is_the_arithmetic_of_the_plugin() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let gain = a_gain(the_kind_of(folder.path(), lv2_test_plugins::GAIN), 0.5);
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
fn an_lv2_value_that_has_keyframes_is_what_the_keyframes_say_in_the_export_and_in_the_preview() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let kind = the_kind_of(folder.path(), lv2_test_plugins::GAIN);
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
    // The value is looked at every 32 samples (the plugin is run in pieces, and the value is set between them), at the frame the first of them is in.
    for moment in [0_usize, 31, 32, 1_599, 1_600, 24_000, 30_000, 47_999] {
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
fn an_lv2_plugin_that_delays_the_sound_is_made_up_for_in_the_export_and_in_the_preview_to_the_sample() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let delaying = an_effect(the_kind_of(folder.path(), lv2_test_plugins::LATENCY), Vec::new());
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
        "the plugin delays the sound by 64 samples (it says so on its port) and the sound is where it was"
    );
    assert_eq!(previewed, plain);
    assert_eq!(
        (export_notes, preview_notes),
        (Vec::<String>::new(), Vec::<String>::new())
    );
}

#[test]
fn the_state_that_the_project_keeps_for_an_lv2_plugin_is_given_to_it_before_its_values_in_the_export_and_in_the_preview()
 {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let kind = the_kind_of(folder.path(), lv2_test_plugins::STATE);
    let (plain, _) = by_the_export(&toolchain, &specification_of_a_second_with(Vec::new()), &sources, None);

    // A state that a plugin saved after it had been asked to boost the sound threefold (the plugin keeps the boost in its state, and not in a value).
    let state = {
        let bundle = folder.path().join("test.lv2");
        let description = lv2_host::describe_bundle(&bundle)
            .unwrap()
            .plugins
            .into_iter()
            .find(|plugin| plugin.uri == lv2_test_plugins::STATE)
            .unwrap();
        let library = lv2_host::Library::open(&description.binary).unwrap();
        let mut instance = lv2_host::Instance::new(&library, &description, 48_000.0, 512).unwrap();
        instance.set_control(3, 3.0);
        let (mut input, mut output) = (vec![0.0_f32; 16], vec![0.0_f32; 16]);
        instance.run(&mut [&mut input], &mut [&mut output], 16).unwrap();
        instance.save_state().unwrap()
    };
    let values = vec![PropertyValue::Number(0.0), PropertyValue::Number(0.0)];
    let mut with_a_state = an_effect(kind, values.clone());
    with_a_state.state = Some(Arc::from(state));

    let specification = specification_of_a_second_with(vec![with_a_state]);
    let (exported, export_notes) = by_the_export(
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
    let tripled: Vec<f32> = plain.iter().map(|sample| sample * 3.0).collect();
    assert_eq!(exported, tripled, "the plugin was given the state: its boost is three");
    assert_eq!(previewed, exported);
    assert!(export_notes.is_empty(), "{export_notes:?}");

    // Without the state, the plugin is as it was made.
    let (without, _) = by_the_export(
        &toolchain,
        &specification_of_a_second_with(vec![an_effect(kind, values)]),
        &sources,
        Some(a_host_that_runs_everything()),
    );
    assert_eq!(without, plain);
}

#[test]
fn a_state_that_an_lv2_plugin_cannot_take_is_said_and_the_sound_goes_on_with_its_values() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let kind = the_kind_of(folder.path(), lv2_test_plugins::STATE);
    let (plain, _) = by_the_export(&toolchain, &specification_of_a_second_with(Vec::new()), &sources, None);
    let mut with_garbage = an_effect(kind, vec![PropertyValue::Number(0.0), PropertyValue::Number(0.0)]);
    with_garbage.state = Some(Arc::from(b"these bytes are not a state".as_slice()));

    let (made, notes) = by_the_export(
        &toolchain,
        &specification_of_a_second_with(vec![with_garbage]),
        &sources,
        Some(a_host_that_runs_everything()),
    );

    assert_eq!(made, plain, "the plugin goes on as it was made");
    assert!(
        notes.iter().any(|note| note.contains("did not take the state")),
        "{notes:?}"
    );
}

#[test]
fn a_folder_that_nobody_has_said_yes_to_leaves_the_sound_of_an_lv2_plugin_as_it_was_and_asks() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let gain = a_gain(the_kind_of(folder.path(), lv2_test_plugins::GAIN), 0.5);
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
    assert_eq!(wanted.len(), 1, "the folder that the bundle is in is asked about");
    assert_eq!(
        wanted[0].libraries,
        vec!["test.lv2".to_owned()],
        "and the bundle is what it names"
    );
    assert!(host.loaded_libraries().is_empty());
}
