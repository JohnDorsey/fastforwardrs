// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! VST3 plugins in the sound (piece 15.4 of `docs/engine-phase-15-vst3-design.md`), with the real ffmpeg for the clips, the real program that runs plugins and the VST3 module written for the
//! tests: **the export and the preview make the same sound, and it is the arithmetic of the plugin**, a value that has keyframes is what the keyframes say, a plugin that delays the sound is
//! made up for to the sample, a folder that nobody has said yes to leaves the sound as it was and asks, **a module that is gone, and a plugin that crashes its program, leave the sound as it
//! was and say so once**.
//!
//! A VST3 plugin always runs in the program that runs plugins (D7), so every host here is given that program.
//!
//! The list of the plugins that are installed is for the whole program, so the tests of this file do not run at the same time.

mod support;

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
use fastforwardrs::plugin_process::remote::Limits;
use fastforwardrs::plugin_scan::{effect_of_a_vst3_plugin, key_of_a_vst3_plugin};
use fastforwardrs::project::{
    Animation, EffectInstanceId, EffectKind, FrameRate, Keyframe, Keyframes, PropertyId, PropertyValue,
    SpedUpClipSound, SpeedFactor, set_installed_plugins,
};
use fastforwardrs::sound_engine::PluginRunner;
use support::scratch_directory;
use support::sound_files::the_sources;
use vst3_test_plugins as vst3_plugins;

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

fn the_program() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_fastforwardrs-plugin-process"))
}

fn the_id_of(numbers: [u32; 4]) -> vst3_host::ClassId {
    vst3_host::ClassId::from_numbers(numbers[0], numbers[1], numbers[2], numbers[3])
}

/// The plugins of `module` that are effects, scanned in this process and installed; and the kind of effect of the class `numbers`.
fn the_kind_of_in(module: &std::path::Path, numbers: [u32; 4]) -> EffectKind {
    let scan = vst3_host::scan_module(module, &mut |_| {}).expect("the module is scanned");
    set_installed_plugins(
        scan.classes
            .iter()
            .filter_map(|class| effect_of_a_vst3_plugin(module, class).ok())
            .collect(),
    );
    let key = key_of_a_vst3_plugin(the_id_of(numbers));
    EffectKind::installed_plugin_kinds()
        .into_iter()
        .find(|kind| kind.key() == key)
        .unwrap_or_else(|| panic!("no plugin called {key}"))
}

/// The same, for the library of the test plugins itself, which is a module.
fn the_kind_of(numbers: [u32; 4]) -> EffectKind {
    the_kind_of_in(&vst3_plugins::library_file_for_tests(), numbers)
}

/// An effect of `kind` with the values it begins with, and `first` for the first (the gain of the test effect).
fn an_effect(kind: EffectKind, first: Option<f32>) -> LayerEffect {
    let mut values = kind.default_values();
    if let Some(first) = first {
        values[0] = PropertyValue::Number(first);
    }
    LayerEffect {
        state: None,
        clock: Default::default(),
        instance: EffectInstanceId::FIRST,
        kind,
        values,
        animation: Animation::default(),
        tables: Vec::new(),
        tail_length: Default::default(),
        passes_notes_on: false,
        shared_by: None,
        driven: Default::default(),
    }
}

fn a_gain(kind: EffectKind, gain: f32) -> LayerEffect {
    an_effect(kind, Some(gain))
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

/// A host that runs every plugin (every folder is said yes to), VST3 ones in the program that runs plugins.
fn a_host_that_runs_everything() -> Arc<dyn PluginRunner> {
    let host = PluginHost::new().with_the_program_that_runs_plugins(the_program());
    host.allow_everything();
    Arc::new(host)
}

/// The same, whose program is made to misbehave as `how` says (long limits: the tests are about what a crash comes to).
fn a_host_whose_program_misbehaves(how: &str) -> Arc<dyn PluginRunner> {
    let host = PluginHost::new()
        .with_the_program_that_runs_plugins(the_program())
        .with_the_program_given(
            vec![("FFRS_TEST_PLUGIN".to_owned(), how.to_owned())],
            Limits {
                to_connect: Duration::from_secs(60),
                to_make: Duration::from_secs(60),
                to_run: Duration::from_secs(60),
            },
        );
    host.allow_everything();
    Arc::new(host)
}

#[test]
fn the_export_and_the_preview_make_the_same_sound_with_a_vst3_plugin_and_it_is_the_arithmetic_of_the_plugin() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let gain = a_gain(the_kind_of(vst3_plugins::GAIN), 0.5);
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
fn a_vst3_value_that_has_keyframes_is_what_the_keyframes_say_in_the_export_and_in_the_preview() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let kind = the_kind_of(vst3_plugins::GAIN);
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
fn a_vst3_plugin_that_delays_the_sound_is_made_up_for_in_the_export_and_in_the_preview_to_the_sample() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let delaying = an_effect(the_kind_of(vst3_plugins::LATENCY), None);
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
fn a_folder_that_nobody_has_said_yes_to_leaves_the_sound_of_a_vst3_plugin_as_it_was_and_asks() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let gain = a_gain(the_kind_of(vst3_plugins::GAIN), 0.5);
    let (plain, _) = by_the_export(&toolchain, &specification_of_a_second_with(Vec::new()), &sources, None);
    let host = Arc::new(PluginHost::new().with_the_program_that_runs_plugins(the_program()));

    let (made, notes) = by_the_export(
        &toolchain,
        &specification_of_a_second_with(vec![gain]),
        &sources,
        Some(Arc::clone(&host) as Arc<dyn PluginRunner>),
    );

    assert_eq!(made, plain, "nothing was run");
    assert!(notes.is_empty(), "{notes:?}");
    let wanted = host.take_wanted();
    assert_eq!(wanted.len(), 1, "the folder of the module is asked about");
    assert!(host.loaded_libraries().is_empty());
}

/// Checks that `notes` is one line that says what went wrong with the plugin, in these words, and gives it.
fn the_one_line_with(notes: &[String], words: &str) -> String {
    assert_eq!(notes.len(), 1, "said once: {notes:?}");
    assert!(
        notes[0].starts_with("The sound plugin \u{ab}")
            && notes[0].contains(words)
            && notes[0]
                .to_lowercase()
                .ends_with("the sound goes through it unchanged."),
        "{notes:?}"
    );
    notes[0].clone()
}

#[test]
fn a_vst3_module_that_is_gone_leaves_the_sound_as_it_was_and_is_said_once_in_the_export_and_in_the_preview() {
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    // The module is a folder of its own, scanned and installed, and then taken away.
    let modules = folder.path().join("modules");
    std::fs::create_dir_all(&modules).unwrap();
    let module = vst3_plugins::write_module(&modules);
    let gain = a_gain(the_kind_of_in(&module, vst3_plugins::GAIN), 0.5);
    std::fs::remove_dir_all(&module).expect("the module is taken away (it is not loaded here)");
    let (plain, _) = by_the_export(&toolchain, &specification_of_a_second_with(Vec::new()), &sources, None);
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

    assert_eq!(exported, plain, "the sound goes through unchanged");
    assert_eq!(previewed, plain);
    let said = the_one_line_with(&export_notes, "could not be loaded");
    assert_eq!(the_one_line_with(&preview_notes, "could not be loaded"), said);
}

#[test]
fn a_vst3_plugin_that_crashes_its_program_leaves_the_sound_as_it_was_and_is_said_once_in_the_export_and_in_the_preview()
{
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let gain = a_gain(the_kind_of(vst3_plugins::GAIN), 0.5);
    let (plain, _) = by_the_export(&toolchain, &specification_of_a_second_with(Vec::new()), &sources, None);
    let specification = specification_of_a_second_with(vec![gain]);

    let (exported, export_notes) = by_the_export(
        &toolchain,
        &specification,
        &sources,
        Some(a_host_whose_program_misbehaves("crash_when_run")),
    );
    let (previewed, preview_notes) = by_the_preview(
        &toolchain,
        &specification,
        &sources,
        Some(a_host_whose_program_misbehaves("crash_when_run")),
    );

    // (The program was started again five times, and the plugin crashed it each time: the effect is left out, from the start of the sound, and says why. It is the sound of the file.)
    assert_eq!(exported, plain, "the editor is still here, and the sound is as it was");
    assert_eq!(previewed, plain);
    the_one_line_with(&export_notes, "the program that runs the plugin ended");
    the_one_line_with(&preview_notes, "the program that runs the plugin ended");
}

/// A second of noise that is a clip with `effects`, and then `silence` frames of nothing.
fn a_second_of_noise_with_then_silence(effects: Vec<LayerEffect>, silence: u64) -> SoundGraphSpecification {
    let mut specification = specification_of_a_second_with(effects);
    let SoundItemToExport::Clip(clip) = specification.strips[0].items[0].clone() else {
        unreachable!("the strip is one clip");
    };
    specification.strips[0].items = vec![
        SoundItemToExport::Clip(clip),
        SoundItemToExport::Silence { frame_count: silence },
    ];
    specification.total_frame_count = 30 + silence;
    specification
}

/// **Ignored**: it needs the real Dragonfly Hall Reverb (`FFRS_VST3_MODULES` names a folder with `DragonflyHallReverb.vst3` below it, as the other tests of real modules do). Dragonfly says a
/// tail of 0 (no plugin built with DPF says anything else) and rings on for a second or more; until 2026-11-03 that was read as half a second, and its reverb was cut dead 30 dB below the
/// clip. `FFRS_VST3_MODULES=E:/s/sffrs/vst3_real cargo test --test sound_plugins_vst3_in_the_sound -- --ignored --nocapture`
#[test]
#[ignore = "needs the real Dragonfly Hall Reverb (FFRS_VST3_MODULES)"]
fn the_reverb_of_dragonfly_hall_rings_on_after_its_clip_for_the_tail_that_a_reverb_is_given_in_the_export_and_in_the_preview()
 {
    use fastforwardrs::project::TailLength;

    /// Samples in a second of the interleaved sound.
    const A_SECOND: usize = 48_000 * 2;
    let toolchain = toolchain_or_skip!();
    let _alone = the_installed_plugins_to_myself();
    let folder = scratch_directory();
    let sources = the_sources(folder.path());
    let modules = PathBuf::from(std::env::var("FFRS_VST3_MODULES").expect("FFRS_VST3_MODULES names a folder"));
    let module = find_module_below(&modules, "DragonflyHallReverb.vst3").expect("DragonflyHallReverb.vst3 is below it");
    let scan = vst3_host::scan_module(&module, &mut |_| {}).expect("the module is scanned");
    let class = scan.classes.first().expect("it has a plugin").class.id;
    let kind = the_kind_of_in(&module, class.numbers());
    let after_the_clip_with = |tail_length: TailLength| {
        let mut effect = an_effect(kind, None);
        effect.tail_length = tail_length;
        let specification = a_second_of_noise_with_then_silence(vec![effect], 150);
        let (exported, notes) = by_the_export(
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
        assert_eq!(previewed, exported, "the preview makes what the export makes");
        assert_eq!(notes, Vec::<String>::new());
        exported[30 * 1_600 * 2..].to_vec()
    };
    let as_it_is = after_the_clip_with(TailLength::Automatic);
    let for_long = after_the_clip_with(TailLength::Milliseconds(5_000));

    let loud = |samples: &[f32]| samples.iter().fold(0.0_f32, |most, sample| most.max(sample.abs()));
    assert!(
        loud(&as_it_is[A_SECOND / 2..A_SECOND * 3 / 2]) > 0.0005,
        "the reverb is still heard between half a second and a second and a half after the clip (it was cut at half a second)"
    );
    assert_eq!(
        &as_it_is[..2 * A_SECOND],
        &for_long[..2 * A_SECOND],
        "for two seconds the sound is the same as with a tail of five: it is the reverb, to the sample"
    );
    assert!(
        as_it_is[2 * A_SECOND..].iter().all(|sample| *sample == 0.0),
        "and then it is over: a reverb is given two seconds"
    );
    assert!(
        loud(&for_long[2 * A_SECOND..3 * A_SECOND]) > 0.0,
        "(the control: it still rings with a tail of five seconds, so the test can tell)"
    );
}

/// The first file called `name` below `folder`, or none.
fn find_module_below(folder: &std::path::Path, name: &str) -> Option<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(folder)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.file_name().is_some_and(|file| file == name) {
            return Some(path);
        }
        if path.is_dir()
            && let Some(found) = find_module_below(&path, name)
        {
            return Some(found);
        }
    }
    None
}
