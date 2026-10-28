// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugins that are really installed on a computer, put on a clip, played and exported (phase 7, piece 7.6; the real set the design asks for is the 119 libraries of LMMS).
//! **Not a check, a report**: what is installed is not ours, and a plugin that someone else wrote may do anything. It is **ignored** by default, and runs on the maintainer's
//! machine (not in the CI) with `FASTFORWARDRS_LADSPA_FOLDER` set to a folder of LADSPA libraries:
//!
//! ```text
//! cargo test --test sound_plugins_lmms_set -- --ignored --nocapture
//! ```
//!
//! (`FASTFORWARDRS_DEPENDENCY_FOLDER` is a folder with libraries that plugin libraries need, and `FASTFORWARDRS_SEARCH_PARENTS=1` looks in the folders above each library too, as
//! in the test of the scan.) **Each plugin is tried in a child process** (this test program, run again for one plugin), because plugins run in the process that uses them: one that
//! crashes, hangs or goes wrong takes only its own child, and the report says so. For each plugin: a second of noise with the plugin on it at the values it begins with is made
//! by the export and by the preview, and the report says whether the two made the same sound, whether it is still a sound (no number that is not a number), whether the plugin
//! changed it, how loud it is, and what the plugin said.

mod support;

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fastforwardrs::composition::LayerEffect;
use fastforwardrs::media::export::{
    LoudnessToExport, SoundClipToExport, SoundGraphSpecification, SoundItemToExport, SoundStripToExport,
};
use fastforwardrs::media::{
    AudioFormat, AudioStreamEvent, AudioStreamRequest, EngineAudioStream, render_sound_to_file_with_plugins,
};
use fastforwardrs::plugin_host::PluginHost;
use fastforwardrs::plugin_scan::{PluginScanner, ScanCache, plugins_found_in};
use fastforwardrs::project::{
    Animation, EffectInstanceId, EffectKind, FrameRate, SpedUpClipSound, SpeedFactor, set_installed_plugins,
};
use fastforwardrs::sound_engine::PluginRunner;
use support::scratch_directory;
use support::sound_files::the_sources;

const SCANNER: &str = env!("CARGO_BIN_EXE_fastforwardrs-plugin-scan");

/// How long one plugin may take for the second of sound, in its child, before it is stopped.
const TIME_ALLOWED: Duration = Duration::from_secs(90);

/// What a line of the report of one plugin begins with.
const RESULT_MARK: &str = "LMMS-RESULT ";

fn scanner(dependency_folders: Vec<PathBuf>, search_parents: bool) -> PluginScanner {
    PluginScanner::new(PathBuf::from(SCANNER), ScanCache::in_memory())
        .looking_for_dependencies_in(dependency_folders, search_parents)
}

fn dependency_settings() -> (Vec<PathBuf>, bool) {
    let folders = std::env::var_os("FASTFORWARDRS_DEPENDENCY_FOLDER")
        .map(PathBuf::from)
        .into_iter()
        .collect();
    let search_parents = std::env::var("FASTFORWARDRS_SEARCH_PARENTS").is_ok_and(|value| value == "1");
    (folders, search_parents)
}

/// The driver: finds the plugins that can be effects, and tries each in a child.
#[test]
#[ignore = "runs the plugins that are installed on this computer; run with --ignored --nocapture and FASTFORWARDRS_LADSPA_FOLDER set"]
fn every_plugin_of_a_folder_is_put_on_a_clip_played_and_exported_each_in_a_process_of_its_own() {
    let folder = PathBuf::from(
        std::env::var_os("FASTFORWARDRS_LADSPA_FOLDER")
            .expect("set FASTFORWARDRS_LADSPA_FOLDER to a folder of LADSPA libraries"),
    );
    let (dependency_folders, search_parents) = dependency_settings();
    let scan = scanner(dependency_folders, search_parents).scan_folders(&[folder]);
    let found = plugins_found_in(&scan.libraries);
    println!(
        "{} libraries scanned, {} plugins can be effects, {} cannot",
        scan.libraries.len(),
        found.effects.len(),
        found.not_used.len()
    );
    let program = std::env::current_exe().expect("this program");
    let mut tally = std::collections::BTreeMap::<String, usize>::new();
    let mut effects = found.effects;
    effects.sort_by(|a, b| a.descriptor.key.cmp(&b.descriptor.key));
    for plugin in &effects {
        let began = Instant::now();
        let mut child = Command::new(&program)
            .args([
                "--ignored",
                "--exact",
                "one_plugin_of_a_library_on_a_clip_for_the_driver",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("FFRS_ONE_PLUGIN_LIBRARY", &plugin.library)
            .env("FFRS_ONE_PLUGIN_KEY", &*plugin.descriptor.key)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the child starts");
        let outcome = loop {
            match child.try_wait().expect("the child can be waited for") {
                Some(status) => {
                    let output = child.wait_with_output().expect("the child's words");
                    let text = String::from_utf8_lossy(&output.stdout).into_owned();
                    break match text
                        .lines()
                        .find_map(|line| line.split_once(RESULT_MARK).map(|(_, result)| result))
                    {
                        Some(result) => result.to_owned(),
                        None => format!("NO-RESULT exit {status}"),
                    };
                }
                None if began.elapsed() > TIME_ALLOWED => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break "TIMED-OUT".to_owned();
                }
                None => std::thread::sleep(Duration::from_millis(20)),
            }
        };
        let word = outcome.split_whitespace().next().unwrap_or("?").to_owned();
        *tally.entry(word).or_default() += 1;
        println!(
            "{} ({}): {outcome} [{:.1?}]",
            plugin.descriptor.key,
            plugin.library.file_name().unwrap().to_string_lossy(),
            began.elapsed()
        );
    }
    println!("TALLY {tally:?}");
    assert!(!effects.is_empty(), "there were plugins to try");
}

/// What the driver runs for one plugin: the plugin of the key `FFRS_ONE_PLUGIN_KEY` of the library `FFRS_ONE_PLUGIN_LIBRARY`, on a second of noise, made by the export and by the
/// preview. Says one line that begins with `LMMS-RESULT`.
#[test]
#[ignore = "run by the test that tries every plugin"]
fn one_plugin_of_a_library_on_a_clip_for_the_driver() {
    let (Some(library), Some(key)) = (
        std::env::var_os("FFRS_ONE_PLUGIN_LIBRARY").map(PathBuf::from),
        std::env::var("FFRS_ONE_PLUGIN_KEY").ok(),
    ) else {
        println!("not run by the driver: it needs FFRS_ONE_PLUGIN_LIBRARY and FFRS_ONE_PLUGIN_KEY");
        return;
    };
    let toolchain = toolchain_or_skip!();
    let (dependency_folders, search_parents) = dependency_settings();
    let report = scanner(dependency_folders.clone(), search_parents)
        .scan_library(&library)
        .expect("the library is scanned");
    set_installed_plugins(plugins_found_in(&[report]).effects);
    let Some(kind) = EffectKind::installed_plugin_kinds()
        .into_iter()
        .find(|kind| kind.key() == key)
    else {
        println!("{RESULT_MARK}NOT-FOUND the plugin is not an effect when the library is scanned alone");
        return;
    };
    let effect = LayerEffect {
        state: None,
        clock: Default::default(),
        instance: EffectInstanceId::FIRST,
        kind,
        values: kind.default_values(),
        animation: Animation::default(),
        tables: Vec::new(),
        tail_length: Default::default(),
    };
    let specification = |effects: Vec<LayerEffect>| SoundGraphSpecification {
        frame_rate: FrameRate::new(30, 1).unwrap(),
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
            })],
            buses: Vec::new(),
            loudness: Default::default(),
            muted: Vec::new(),
        }],
        sped_up_clip_sound: SpedUpClipSound::MaintainPitch,
        lead_in_frames: 0,
        groups: Vec::new(),
    };
    let scratch = scratch_directory();
    let sources = the_sources(scratch.path());
    let host = Arc::new(PluginHost::new());
    host.allow_everything();
    host.look_for_dependencies_in(dependency_folders, search_parents);
    let runner = || -> Option<Arc<dyn PluginRunner>> { Some(host.clone()) };
    let file_of_sound = |name: &str| scratch.path().join(name);
    let read = |path: &PathBuf| -> Vec<f32> {
        fs::read(path)
            .expect("the file of sound")
            .as_chunks::<4>()
            .0
            .iter()
            .map(|sample| f32::from_le_bytes(*sample))
            .collect()
    };
    let sources_as_locations = || sources.iter().map(|path| path.clone().into()).collect::<Vec<_>>();
    let plain_file = file_of_sound("plain.f32");
    render_sound_to_file_with_plugins(
        &toolchain,
        &specification(Vec::new()),
        sources_as_locations(),
        &plain_file,
        &|| false,
        None,
    )
    .expect("the plain sound is made");
    let exported_file = file_of_sound("exported.f32");
    let notes = match render_sound_to_file_with_plugins(
        &toolchain,
        &specification(vec![effect.clone()]),
        sources_as_locations(),
        &exported_file,
        &|| false,
        runner(),
    ) {
        Ok(notes) => notes,
        Err(error) => {
            println!("{RESULT_MARK}EXPORT-FAILED {error}");
            return;
        }
    };
    let (plain, exported) = (read(&plain_file), read(&exported_file));
    // The same export again: a plugin that does not make the same sound twice (it has a random number in it, or uses what was left in memory) cannot be held to the preview.
    let again_file = file_of_sound("exported_again.f32");
    let _ = render_sound_to_file_with_plugins(
        &toolchain,
        &specification(vec![effect.clone()]),
        sources_as_locations(),
        &again_file,
        &|| false,
        runner(),
    );
    let repeats = read(&again_file) == exported;

    let request = AudioStreamRequest {
        source_files: sources_as_locations(),
        sound: specification(vec![effect]),
        format: AudioFormat {
            sample_rate: 48_000,
            channel_count: 2,
        },
    };
    let stream = EngineAudioStream::start_with_plugins(&toolchain, &request, Arc::new(|| {}), runner())
        .expect("the preview starts");
    let began = Instant::now();
    let mut previewed = Vec::new();
    loop {
        match stream.try_next_event() {
            Some(AudioStreamEvent::Samples(more)) => previewed.extend(more),
            Some(AudioStreamEvent::Finished { .. }) => break,
            None if began.elapsed() > TIME_ALLOWED => {
                println!("{RESULT_MARK}PREVIEW-HUNG");
                return;
            }
            None => std::thread::sleep(Duration::from_millis(2)),
        }
    }

    let finite = exported.iter().all(|sample| sample.is_finite());
    let peak = exported.iter().fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    let rms = (exported.iter().map(|sample| sample * sample).sum::<f32>() / exported.len().max(1) as f32).sqrt();
    let changed = exported != plain;
    let same_in_the_preview = previewed == exported;
    if !same_in_the_preview && previewed.len() == exported.len() {
        let differing = previewed.iter().zip(&exported).filter(|(a, b)| a != b).count();
        let first = previewed.iter().zip(&exported).position(|(a, b)| a != b).unwrap();
        println!(
            "{differing} of {} samples differ, the first at {first}: {} in the preview and {} in the export",
            exported.len(),
            previewed[first],
            exported[first]
        );
    } else if !same_in_the_preview {
        println!(
            "{} samples in the preview and {} in the export",
            previewed.len(),
            exported.len()
        );
    }
    let verdict = match (finite, same_in_the_preview, repeats, notes.is_empty()) {
        (false, ..) => "NOT-A-NUMBER",
        (true, true, true, true) => "OK",
        (true, true, true, false) => "OK-WITH-NOTES",
        (true, _, false, _) => "NOT-REPEATABLE",
        (true, false, true, _) => "PREVIEW-DIFFERS",
    };
    println!(
        "{RESULT_MARK}{verdict} changed={changed} repeats={repeats} same_in_the_preview={same_in_the_preview} peak={peak:.3} rms={rms:.3} notes={notes:?}"
    );
}
