// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! MIDI clips in the sound, whole (phase 13, piece 13.4; docs/engine-phase-13-midi-design.md): a project with a MIDI clip and the real instruments of the CLAP library and the LV2 bundle that
//! are written for the tests, planned (`plan_sound`), made into the specification of the graph, and made into sound by the graph with the plugin host: **the notes are played by the first
//! synthesizer on the road, at their samples, and the sound that is made from a seek is what the whole sound has from there**.
//!
//! What the instruments make is plain arithmetic (see the head of `crates/clap-test-plugins/src/lib.rs`): while a note is held, `velocity * key / 100`; the notes of the files made here are at
//! the velocity 100, so a note of key 60 is 60 / 127 = 0.4724...
//!
//! The list of the plugins that are installed is for the whole program, so the tests of this file do not run at the same time.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use clap_test_plugins::{INSTRUMENT, library_file_for_tests};
use fastforwardrs::composition::plan_sound;
use fastforwardrs::media::export::sound_graph_specification;
use fastforwardrs::plugin_host::PluginHost;
use fastforwardrs::plugin_scan::{effect_of_a_clap_plugin, effect_of_an_lv2_plugin, key_of_an_lv2_plugin};
use fastforwardrs::project::{
    AnimationTarget, EffectKind, FrameRate, GroupKind, Member, MidiClip, Project, ResourceKind, SourceFrameIndex,
    SourceMediaLink, SpedUpClipSound, SpeedFactor, TimelineFrameIndex, set_installed_plugins,
};
use fastforwardrs::sound_engine::{
    ClipFeed, ClipRequest, ClipSpawner, LookAheadFeeds, Notes, PluginInsertions, PluginRunner, SoundGraph,
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

/// The kinds of effect of the CLAP test plugins that are effects (the instruments among them), installed; and the kind of the plugin `id`.
fn the_clap_kind_of(id: &str) -> EffectKind {
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

fn the_lv2_kind_of(folder: &Path, uri: &str) -> EffectKind {
    let bundle = lv2_test_plugins::write_bundle(folder);
    let scan = lv2_host::scan_bundle(&bundle, &mut |_| {}).expect("the bundle is scanned");
    set_installed_plugins(
        scan.plugins
            .iter()
            .filter_map(|plugin| effect_of_an_lv2_plugin(&bundle, plugin).ok())
            .collect(),
    );
    let key = key_of_an_lv2_plugin(uri);
    EffectKind::installed_plugin_kinds()
        .into_iter()
        .find(|kind| kind.key() == key)
        .unwrap_or_else(|| panic!("no plugin called {uri}"))
}

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

/// A project of 30 frames a second with the file `tune.mid` (a note of key 60 from 0.1 s to 0.4 s, one of key 72 from 0.5 s to 1.5 s and one of key 48 from 2.0 s to 2.2 s), and these tracks, the
/// one in front first: each has a MIDI clip of 90 frames (3 seconds) from the frame it says.
fn project_with_music(tracks: &[u64]) -> Project {
    let mut project = Project::empty().with_frame_rate(Some(thirty()));
    project
        .add_resource(
            "tune.mid",
            ResourceKind::Midi,
            music_with(&[(100, 400, 60), (500, 1500, 72), (2000, 2200, 48)]),
            SourceMediaLink::from_absolute_path(PathBuf::from("/music/tune.mid")),
        )
        .unwrap();
    // (`Project::empty` has a track of its own, which is the first of these.)
    for (number, first_frame) in tracks.iter().enumerate() {
        let track = if number == 0 {
            project.tracks()[0].id()
        } else {
            project.add_track_in_front()
        };
        project
            .try_insert_midi_clip(
                track,
                TimelineFrameIndex::new(*first_frame),
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
    }
    project
}

fn the_clip_of(project: &Project, track: usize) -> AnimationTarget {
    AnimationTarget::Clip(project.tracks()[track].placement_at_index(0).unwrap().id)
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

/// What the sound is where a note of the key `key` is held from `start` to `end` (in samples from the start of the sound): the number that the test instrument makes of it, 60 / 127 for the
/// key 60, and so on.
fn level_of(key: u8) -> f32 {
    f32::from(key) / 127.0
}

/// The sound that the three notes of `tune.mid` make, from the sample `from` on, where the sound begins at `at` (the clip's first sample, counted from the start of what is wanted).
fn the_sound_of_the_tune(samples: usize, at: usize) -> Vec<f32> {
    let mut sound = vec![0.0_f32; samples];
    // (Seconds are 48000 samples: 0.1 s is 4800, and so on.)
    for (start, end, key) in [(4_800, 19_200, 60_u8), (24_000, 72_000, 72), (96_000, 105_600, 48)] {
        let end = (at + end).min(samples);
        if let Some(inside) = sound.get_mut(at + start..end) {
            for sample in inside {
                *sample += level_of(key);
            }
        }
    }
    sound
}

fn close(heard: &[f32], wanted: &[f32], what: &str) {
    assert_eq!(heard.len(), wanted.len(), "{what}");
    for (place, (heard, wanted)) in heard.iter().zip(wanted).enumerate() {
        assert!(
            (heard - wanted).abs() < 1e-5,
            "{what}: sample {place} is {heard} and not {wanted}"
        );
    }
}

#[test]
fn a_clap_instrument_among_the_effects_of_a_midi_clip_plays_its_notes() {
    let _alone = the_installed_plugins_to_myself();
    let instrument = the_clap_kind_of(INSTRUMENT);
    let mut project = project_with_music(&[0]);
    project.add_effect(the_clip_of(&project, 0), instrument).unwrap();

    let sound = heard_from(&project, 0, 4096);

    close(&sound, &the_sound_of_the_tune(144_000, 0), "the notes of the file");
}

#[test]
fn the_notes_are_where_the_clip_is_on_the_timeline_whatever_the_size_of_the_blocks() {
    let _alone = the_installed_plugins_to_myself();
    let instrument = the_clap_kind_of(INSTRUMENT);
    let mut project = project_with_music(&[30]);
    project.add_effect(the_clip_of(&project, 0), instrument).unwrap();

    // (The sound is silent for the first second, which is 48000 samples, and then the tune.)
    for block in [64, 1000, 4096, 100_000] {
        let sound = heard_from(&project, 0, block);

        let mut wanted = vec![0.0; 48_000];
        wanted.extend(the_sound_of_the_tune(144_000, 0));
        close(&sound, &wanted, &format!("blocks of {block}"));
    }
}

#[test]
fn an_instrument_among_the_effects_of_the_track_plays_the_notes_of_its_clips_and_one_among_those_of_a_group_those_of_the_clips_in_it()
 {
    let _alone = the_installed_plugins_to_myself();
    let instrument = the_clap_kind_of(INSTRUMENT);
    let mut on_the_track = project_with_music(&[0]);
    let track = AnimationTarget::Track(on_the_track.tracks()[0].id());
    on_the_track.add_effect(track, instrument).unwrap();

    let mut in_a_group = project_with_music(&[0, 0]);
    let (first, second) = (
        in_a_group.tracks()[0].placement_at_index(0).unwrap().id,
        in_a_group.tracks()[1].placement_at_index(0).unwrap().id,
    );
    let group = in_a_group
        .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    in_a_group.add_effect_to_the_group(group, instrument).unwrap();

    close(
        &heard_from(&on_the_track, 0, 777),
        &the_sound_of_the_tune(144_000, 0),
        "the track's instrument",
    );
    // (Two tracks with the same clip, both played by the one instrument of the group: it holds each note of each clip, so what it makes is twice the tune.)
    let wanted: Vec<f32> = the_sound_of_the_tune(144_000, 0)
        .into_iter()
        .map(|made| made * 2.0)
        .collect();
    close(
        &heard_from(&in_a_group, 0, 777),
        &wanted,
        "the group's instrument, two clips",
    );
}

#[test]
fn an_effect_after_the_instrument_is_done_to_what_the_instrument_makes() {
    let _alone = the_installed_plugins_to_myself();
    let instrument = the_clap_kind_of(INSTRUMENT);
    let gain = the_clap_kind_of("org.fastforwardrs.test.gain");
    let instrument = EffectKind::installed_plugin_kinds()
        .into_iter()
        .find(|kind| kind.key() == instrument.key())
        .expect("both are installed");
    let mut project = project_with_music(&[0]);
    let clip = the_clip_of(&project, 0);
    project.add_effect(clip, instrument).unwrap();
    let doubled = project.add_effect(clip, gain).unwrap();
    project
        .set_property_at(
            clip,
            fastforwardrs::project::PropertyId::effect_value(gain, doubled, 0),
            fastforwardrs::project::PropertyValue::Number(2.0),
            TimelineFrameIndex::new(0),
        )
        .unwrap();

    let sound = heard_from(&project, 0, 4096);

    let wanted: Vec<f32> = the_sound_of_the_tune(144_000, 0)
        .into_iter()
        .map(|made| made * 2.0)
        .collect();
    close(&sound, &wanted, "twice what the instrument makes");
}

#[test]
fn the_first_instrument_takes_the_notes_and_the_one_after_it_is_silent() {
    let _alone = the_installed_plugins_to_myself();
    let instrument = the_clap_kind_of(INSTRUMENT);
    let mut project = project_with_music(&[0]);
    let clip = the_clip_of(&project, 0);
    project.add_effect(clip, instrument).unwrap();
    project.add_effect(clip, instrument).unwrap();

    let sound = heard_from(&project, 0, 4096);

    close(
        &sound,
        &the_sound_of_the_tune(144_000, 0),
        "one instrument's worth, not two",
    );
}

#[test]
fn an_instrument_that_is_not_installed_takes_the_notes_and_the_instrument_after_it_plays_none() {
    let _alone = the_installed_plugins_to_myself();
    let instrument = the_clap_kind_of(INSTRUMENT);
    // (A kind of effect that a file's snapshot made, of a plugin that is not here, and that is a synthesizer.)
    let mut descriptor = instrument.descriptor().clone();
    descriptor.key = "clap:gone.instrument".into();
    let gone = EffectKind::from_snapshot(descriptor).unwrap();
    assert!(!gone.is_installed() && gone.makes_sound_from_notes());
    let mut project = project_with_music(&[0]);
    let clip = the_clip_of(&project, 0);
    project.add_effect(clip, gone).unwrap();
    project.add_effect(clip, instrument).unwrap();

    let sound = heard_from(&project, 0, 4096);

    assert_eq!(sound, vec![0.0; 144_000], "the notes went to the one that is gone");
}

#[test]
fn the_sound_that_is_made_from_a_seek_is_what_the_whole_sound_has_from_there_notes_that_are_held_included() {
    let _alone = the_installed_plugins_to_myself();
    let instrument = the_clap_kind_of(INSTRUMENT);
    let mut project = project_with_music(&[0]);
    project.add_effect(the_clip_of(&project, 0), instrument).unwrap();
    let whole = heard_from(&project, 0, 4096);

    // (Frame 20 is 0.67 s, in the middle of the note of key 72; frame 30 is where it is held; frame 55 is after it; frame 70 is in the last note.)
    for from in [1, 8, 20, 30, 44, 55, 61, 70, 74] {
        let from_there = heard_from(&project, from, 1000);

        let samples_before = (from * 1600) as usize;
        assert_eq!(from_there.len(), whole.len() - samples_before, "from frame {from}");
        close(&from_there, &whole[samples_before..], &format!("from frame {from}"));
    }
}

#[test]
fn an_lv2_instrument_plays_the_notes_of_a_midi_clip_too() {
    let _alone = the_installed_plugins_to_myself();
    let folder = tempfile::tempdir().unwrap();
    let instrument = the_lv2_kind_of(folder.path(), lv2_test_plugins::INSTRUMENT);
    let mut project = project_with_music(&[0]);
    project.add_effect(the_clip_of(&project, 0), instrument).unwrap();

    let sound = heard_from(&project, 0, 2048);

    // (The LV2 instrument makes the same arithmetic of the 7 bits of the velocity: 100 / 127 of key / 100 is key / 127.)
    close(
        &sound,
        &the_sound_of_the_tune(144_000, 0),
        "the notes of the file, through LV2",
    );
    let whole = sound;
    let from_there = heard_from(&project, 30, 1000);
    close(&from_there, &whole[48_000..], "from frame 30, a note held through LV2");
}
