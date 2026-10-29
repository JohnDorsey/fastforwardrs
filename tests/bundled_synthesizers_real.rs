// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The synthesizers that the program can fetch (the sets of `bundled-plugins/manifest.json` whose names say "synthesizer"), **played by the program as a person would play them**: found by the
//! scan program as synthesizers, made into effects of the sound, given a note, and the sound that comes out looked at; and then **a MIDI clip of a project played by each of them through the whole
//! engine** (the plan, the graph, the plugin host). **Run by hand** on a folder that `tools/bundled_plugins/fetch_bundled_plugins.py` made (it is downloaded, so the tests are ignored unless
//! asked for), with the folder named by `FFRS_BUNDLED_PLUGINS`:
//!
//! ```text
//! py tools/bundled_plugins/fetch_bundled_plugins.py --out target/bundled/plugins
//! FFRS_BUNDLED_PLUGINS=target/bundled/plugins cargo test --release --test bundled_synthesizers_real -- --ignored --nocapture
//! ```
//!
//! What they check is what a person is promised, and what a test can say of a sound: every synthesizer that the manifest lists is found, **is taken for a synthesizer**, makes sound from a note
//! (finite, not silence, not absurdly loud), is quiet again some time after its note ends, **makes no sound before a note and makes sound in it, where the clip puts the note**, and holds a
//! note that began before a seek. Whether the sound is *good* is for a listener; and what a real plugin makes is not exact arithmetic, so no sample is compared with a number.
//!
//! The list of the plugins that are installed is for the whole program, so the tests of this file do not run at the same time.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use fastforwardrs::composition::{LayerEffect, plan_sound};
use fastforwardrs::media::export::{LoudnessToExport, SoundClipToExport, sound_graph_specification};
use fastforwardrs::plugin_host::PluginHost;
use fastforwardrs::plugin_scan::{PluginScanner, ScanCache, bundled_plugins_folder, plugins_found_in};
use fastforwardrs::project::{
    Animation, AnimationTarget, EffectInstanceId, EffectKind, FrameRate, MidiClip, Project, ResourceKind,
    SourceFrameIndex, SourceMediaLink, SpedUpClipSound, SpeedFactor, TimelineFrameIndex, set_installed_plugins,
};
use fastforwardrs::sound_engine::{
    BlockProcessor, ClipFeed, ClipRequest, ClipSpawner, LookAheadFeeds, NoteEvent, NoteEventKind, Notes,
    PluginInsertions, PluginRunner, SoundGraph,
};

const SCANNER: &str = env!("CARGO_BIN_EXE_fastforwardrs-plugin-scan");
const SAMPLE_RATE: usize = 48_000;
const BLOCK: usize = 512;

fn the_installed_plugins_to_myself() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    set_installed_plugins(Vec::new());
    guard
}

fn manifest() -> serde_json::Value {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("bundled-plugins")
        .join("manifest.json");
    serde_json::from_str(&std::fs::read_to_string(file).expect("the manifest is read")).expect("the manifest is JSON")
}

fn thirty() -> FrameRate {
    FrameRate::new(30, 1).unwrap()
}

/// A synthesizer of the manifest, found by the scan of the folder.
struct Found {
    /// The file or the bundle it is in.
    file: String,
    name: String,
    kind: EffectKind,
}

/// **Installs** the plugins that the scan of the folder of bundled plugins finds, and gives the synthesizers that the manifest names for this system, each after checking that it is taken for one.
fn the_synthesizers_of_the_manifest_found_in_the_folder() -> Vec<Found> {
    let folder = bundled_plugins_folder().expect("FFRS_BUNDLED_PLUGINS names the folder that the script made");
    let platform = fastforwardrs::plugin_bundles::this_platform().expect("builds for this system");
    // (The files of the sets that have a build for this system and say "synthesizer" in their names.)
    let wanted: Vec<String> = manifest()["sets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|set| set["name"].as_str().unwrap().contains("synthesizer"))
        .filter_map(|set| set["platforms"][platform]["members"].as_array())
        .flatten()
        .map(|member| member["to"].as_str().unwrap().to_owned())
        .collect();
    assert!(!wanted.is_empty(), "the manifest names no synthesizer for {platform}");

    let mut scanner = PluginScanner::new(PathBuf::from(SCANNER), ScanCache::in_memory())
        .with_time_limit(std::time::Duration::from_secs(60));
    let scan = scanner.scan_folders(std::slice::from_ref(&folder));
    assert_eq!(scan.problems, Vec::<String>::new());
    let found = plugins_found_in(&scan.libraries);
    set_installed_plugins(found.effects.clone());

    wanted
        .iter()
        .map(|file| {
            let plugin = found
                .effects
                .iter()
                .find(|plugin| {
                    plugin
                        .library
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy() == file.as_str())
                })
                .unwrap_or_else(|| panic!("no plugin of {file} is used: {:?}", found.not_used));
            let name = plugin.descriptor.name.to_string();
            assert!(
                plugin.descriptor.makes_sound_from_notes,
                "{file}: {name} is not taken for a synthesizer"
            );
            let kind = EffectKind::installed_plugin_kinds()
                .into_iter()
                .find(|kind| kind.key() == plugin.descriptor.key.as_ref())
                .expect("the plugin is a kind of effect");
            Found {
                file: file.clone(),
                name,
                kind,
            }
        })
        .collect()
}

fn an_effect_with_the_values_it_has_when_it_is_put_on(kind: EffectKind) -> LayerEffect {
    LayerEffect {
        state: None,
        clock: Default::default(),
        instance: EffectInstanceId::FIRST,
        values: kind.default_values(),
        kind,
        animation: Animation::default(),
        tables: Vec::new(),
        tail_length: Default::default(),
        passes_notes_on: false,
        shared_by: None,
    }
}

fn the_processor_of(effect: &LayerEffect) -> Box<dyn BlockProcessor> {
    let host = PluginHost::new();
    host.allow_everything();
    let clip = SoundClipToExport {
        source_input: 0,
        first_source_frame: 0,
        end_source_frame: 1000,
        speed_factor: SpeedFactor::new(1).unwrap(),
        timeline_frame_count: 1000,
        loudness: LoudnessToExport::UNCHANGED,
        first_timeline_frame: 0,
        frames_of_the_clip_before: 0,
        effects: vec![effect.clone()],
        sent_to: None,
        tail_routes: Vec::new(),
        music: None,
    };
    host.processor_for(effect, &clip, thirty(), &Notes::default(), false)
        .expect("the plugin is run")
}

fn on(time: u32, key: u8, velocity: f32) -> NoteEvent {
    NoteEvent {
        time,
        what: NoteEventKind::On {
            channel: 0,
            key,
            velocity,
            id: u32::from(key),
        },
    }
}

fn off(time: u32, key: u8) -> NoteEvent {
    NoteEvent {
        time,
        what: NoteEventKind::Off {
            channel: 0,
            key,
            velocity: 0.5,
            id: u32::from(key),
        },
    }
}

/// What a synthesizer made of one note: the sound of both channels, block by block.
fn play_a_note(
    processor: &mut dyn BlockProcessor,
    key: u8,
    held_samples: usize,
    seconds_in_all: usize,
) -> (Vec<f32>, Vec<f32>) {
    let total = seconds_in_all * SAMPLE_RATE;
    let (mut heard_left, mut heard_right) = (Vec::with_capacity(total), Vec::with_capacity(total));
    let mut start = 0;
    while start < total {
        let mut notes = Vec::new();
        if start == 0 {
            notes.push(on(0, key, 0.8));
        }
        if (start..start + BLOCK).contains(&held_samples) {
            notes.push(off((held_samples - start) as u32, key));
        }
        let (mut left, mut right) = (vec![0.0_f32; BLOCK], vec![0.0_f32; BLOCK]);
        processor.give_notes(&notes);
        processor.process(&mut left, &mut right);
        heard_left.extend(left);
        heard_right.extend(right);
        start += BLOCK;
    }
    (heard_left, heard_right)
}

fn rms(samples: &[f32]) -> f64 {
    (samples.iter().map(|sample| f64::from(*sample).powi(2)).sum::<f64>() / samples.len().max(1) as f64).sqrt()
}

#[test]
#[ignore = "needs the folder of bundled plugins that tools/bundled_plugins/fetch_bundled_plugins.py makes (FFRS_BUNDLED_PLUGINS)"]
fn every_synthesizer_of_the_manifest_is_found_as_one_makes_sound_from_a_note_and_is_quiet_after_it() {
    let _alone = the_installed_plugins_to_myself();
    for Found { file, name, kind } in the_synthesizers_of_the_manifest_found_in_the_folder() {
        let effect = an_effect_with_the_values_it_has_when_it_is_put_on(kind);
        let mut processor = the_processor_of(&effect);
        // (Middle C held for one second, in all four seconds.)
        let (left, right) = play_a_note(&mut *processor, 60, SAMPLE_RATE, 4);

        let all_finite = left.iter().chain(&right).all(|sample| sample.is_finite());
        let peak = left
            .iter()
            .chain(&right)
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        let while_held = rms(&left[SAMPLE_RATE / 10..SAMPLE_RATE]);
        let at_the_end = rms(&left[SAMPLE_RATE * 7 / 2..]);
        let in_the_right = rms(&right[SAMPLE_RATE / 10..SAMPLE_RATE]);
        let first_sound = left.iter().position(|sample| sample.abs() > 1e-4);
        println!(
            "{file:<20} {name:<28} peak {peak:>7.4}  rms while held {while_held:>8.5} (right {in_the_right:>8.5})  rms in the last half second {at_the_end:>9.6}  first sound at sample {first_sound:?}  latency {}",
            processor.latency()
        );
        assert!(all_finite, "{name} made a sample that is not a number");
        assert!(peak < 8.0, "{name} made a peak of {peak}");
        assert!(
            while_held > 1e-4,
            "{name} made no sound from a note held for a second ({while_held})"
        );
        assert!(
            at_the_end < while_held / 10.0,
            "{name} is still as loud three seconds after its note ended ({at_the_end} and {while_held})"
        );
    }
}

// --- a MIDI clip of a project, played through the whole engine ---

/// A Standard MIDI File of one track with these notes, each `(start in milliseconds, end in milliseconds, key)`, at the velocity 100 on the first channel: a tick is a millisecond.
fn music_with(notes: &[(u32, u32, u8)]) -> Arc<[u8]> {
    let mut events: Vec<(u32, u8, [u8; 3])> = Vec::new();
    for &(start, end, key) in notes {
        events.push((start, 1, [0x90, key, 100]));
        events.push((end, 0, [0x80, key, 64]));
    }
    events.sort();
    let mut body = vec![0x00, 0xFF, 0x51, 0x03, 0x0F, 0x42, 0x40];
    let mut tick = 0_u32;
    for (at, _, bytes) in events {
        let mut delta = at - tick;
        tick = at;
        let mut number = vec![(delta & 0x7F) as u8];
        delta >>= 7;
        while delta > 0 {
            number.push((delta & 0x7F) as u8 | 0x80);
            delta >>= 7;
        }
        number.reverse();
        body.extend(number);
        body.extend_from_slice(&bytes);
    }
    body.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);
    let mut file = b"MThd\0\0\0\x06\0\0\0\x01\x03\xE8".to_vec();
    file.extend_from_slice(b"MTrk");
    file.extend_from_slice(&u32::try_from(body.len()).unwrap().to_be_bytes());
    file.extend_from_slice(&body);
    Arc::from(file)
}

/// A project of 30 frames a second with the file `tune.mid` (a note of key 60 from 0.1 s to 0.4 s, one of key 72 from 0.5 s to 1.5 s and one of key 48 from 2.0 s to 2.2 s) as a MIDI clip of
/// 90 frames (3 seconds) at the start, with the synthesizer `kind` among the effects of the clip.
fn project_played_by(kind: EffectKind) -> Project {
    let mut project = Project::empty().with_frame_rate(Some(thirty()));
    project
        .add_resource(
            "tune.mid",
            ResourceKind::Midi,
            music_with(&[(100, 400, 60), (500, 1500, 72), (2000, 2200, 48)]),
            SourceMediaLink::from_absolute_path(PathBuf::from("/music/tune.mid")),
        )
        .unwrap();
    let track = project.tracks()[0].id();
    project
        .try_insert_midi_clip(
            track,
            TimelineFrameIndex::new(0),
            MidiClip::new(
                "tune.mid",
                0,
                SourceFrameIndex::new(0),
                SourceFrameIndex::new(90),
                SpeedFactor::NORMAL,
            )
            .unwrap(),
        )
        .unwrap();
    let clip = AnimationTarget::Clip(project.tracks()[0].placement_at_index(0).unwrap().id);
    project.add_effect(clip, kind).unwrap();
    project
}

/// The sound that has to be made for the clip and nothing else: a feed that is never asked for.
struct NoFile;

impl ClipSpawner for NoFile {
    fn spawn(&mut self, _request: &ClipRequest<'_>) -> Box<dyn ClipFeed> {
        panic!("a MIDI clip has no file, and no feed is asked for it");
    }
}

/// The left channel of the sound of the project from the frame `from`, made by the graph with the plugin host, in blocks of `block` samples.
fn heard_from(project: &Project, from: u64, block: usize) -> Vec<f32> {
    let plan = plan_sound(project, TimelineFrameIndex::new(from), |_| true);
    let specification = sound_graph_specification(&plan, &[], SpedUpClipSound::MaintainPitch);
    let host = PluginHost::new();
    host.allow_everything();
    let runner: Arc<dyn PluginRunner> = Arc::new(host);
    let insertions = PluginInsertions::new(runner, specification.frame_rate, Notes::default(), false);
    let feeds = LookAheadFeeds::new(specification.clone(), NoFile);
    let mut graph = SoundGraph::new(specification, Box::new(feeds), Box::new(insertions));
    graph.render_to_the_end(block).0
}

#[test]
#[ignore = "needs the folder of bundled plugins that tools/bundled_plugins/fetch_bundled_plugins.py makes (FFRS_BUNDLED_PLUGINS)"]
fn every_synthesizer_of_the_manifest_plays_a_midi_clip_where_its_notes_are_and_holds_a_note_across_a_seek() {
    let _alone = the_installed_plugins_to_myself();
    for Found { file, name, kind } in the_synthesizers_of_the_manifest_found_in_the_folder() {
        let project = project_played_by(kind);
        // (Seconds are 48000 samples: the notes are at 4800 to 19200, 24000 to 72000 and 96000 to 105600, in a sound of 144000 samples.)
        let sound = heard_from(&project, 0, 4096);
        assert_eq!(sound.len(), 144_000, "{name}: the length of the sound");

        let first_sound = sound.iter().position(|sample| sample.abs() > 1e-4);
        let before_the_first_note = rms(&sound[..4_700]);
        let first_note = rms(&sound[6_000..19_000]);
        let second_note = rms(&sound[30_000..70_000]);
        let third_note = rms(&sound[97_000..105_000]);
        let after_the_last = rms(&sound[125_000..]);
        let peak = sound.iter().fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        println!(
            "{file:<20} {name:<12} first sound at {first_sound:?}  rms: before the first note {before_the_first_note:.6}, first {first_note:.5}, second {second_note:.5}, third {third_note:.5}, after the last {after_the_last:.7}  peak {peak:.3}"
        );
        assert!(
            sound.iter().all(|sample| sample.is_finite()),
            "{name}: a sample that is not a number"
        );
        assert!(peak < 8.0, "{name}: a peak of {peak}");
        // (Nothing sounds before the first note, and what sounds first is the first note, a little after its start for a plugin whose sound builds up.)
        assert!(
            first_sound.is_some_and(|at| (4_800..4_800 + 6_000).contains(&at)),
            "{name}: the first sound is at {first_sound:?}, and the first note is at 4800"
        );
        assert!(
            before_the_first_note < 1e-4,
            "{name}: sound before the first note ({before_the_first_note})"
        );
        for (which, level) in [("first", first_note), ("second", second_note), ("third", third_note)] {
            assert!(level > 1e-3, "{name}: no sound in the {which} note ({level})");
        }
        assert!(
            after_the_last < third_note / 10.0,
            "{name}: still sounding long after the last note ({after_the_last} and {third_note})"
        );

        // From a seek into the middle of the second note (frame 30 is 1 s: the note is held from 0.5 s to 1.5 s, so for the first 24000 samples of what is heard), the note is started again at once.
        let after_a_seek = heard_from(&project, 30, 4096);
        assert_eq!(after_a_seek.len(), 144_000 - 48_000, "{name}: the length after a seek");
        let held = rms(&after_a_seek[2_000..22_000]);
        let silence_before_the_third = rms(&after_a_seek[40_000..47_000]);
        let third_after_the_seek = rms(&after_a_seek[49_000..57_000]);
        println!(
            "{file:<20} {name:<12} after a seek to 1 s: the held note {held:.5}, the quiet before the third {silence_before_the_third:.7}, the third {third_after_the_seek:.5}"
        );
        assert!(held > 1e-3, "{name}: a note held across a seek is not heard ({held})");
        assert!(
            third_after_the_seek > 1e-3,
            "{name}: the note after the seek is not heard ({third_after_the_seek})"
        );
        assert!(
            silence_before_the_third < held / 10.0,
            "{name}: sound before the third note after a seek ({silence_before_the_third} and {held})"
        );
    }
}
