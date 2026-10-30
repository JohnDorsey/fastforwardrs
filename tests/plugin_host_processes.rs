// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The host that runs plugins in processes of their own (phase 14): **the sound of a clip through a copy in a program is the sound through a copy in this process**, bit for bit, for
//! every format; the words for what goes wrong are the same words; the setting says which plugins go to a program; a program that is not there, or is not of this build, leaves the sound as
//! it was and says why; and a plugin that crashes its program while it plays is left out of that play and does not end the test program.
//!
//! The list of the plugins that are installed is for the whole program, so the tests of this file do not run at the same time.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use clap_test_plugins as clap_plugins;
use fastforwardrs::composition::LayerEffect;
use fastforwardrs::media::export::{LoudnessToExport, SoundClipToExport};
use fastforwardrs::plugin_host::{InAProcessOfItsOwn, PluginHost};
use fastforwardrs::plugin_process::remote::Limits;
use fastforwardrs::plugin_scan::{
    effect_of_a_clap_plugin, effect_of_a_plugin, effect_of_an_lv2_plugin, key_of_an_lv2_plugin,
};
use fastforwardrs::project::{
    Animation, EffectInstanceId, EffectKind, FrameRate, InstalledPlugin, PropertyValue, SpeedFactor,
    set_installed_plugins,
};
use fastforwardrs::sound_engine::{BlockProcessor, NoteEvent, NoteEventKind, Notes, PluginRunner};

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

fn a_host_with(which: InAProcessOfItsOwn) -> PluginHost {
    let host = PluginHost::new()
        .running_in_a_process_of_its_own(which)
        .with_the_program_that_runs_plugins(the_program());
    host.allow_everything();
    host
}

/// The plugins of the CLAP test library, scanned once (a scan takes seconds: the plugins are tried).
fn the_clap_plugins() -> Vec<InstalledPlugin> {
    static SCANNED: std::sync::OnceLock<Vec<InstalledPlugin>> = std::sync::OnceLock::new();
    SCANNED
        .get_or_init(|| {
            let path = clap_plugins::library_file_for_tests();
            let scan = clap_host::scan::scan_library(&path, |_| {}).expect("the test library is scanned");
            scan.plugins
                .iter()
                .filter_map(|plugin| effect_of_a_clap_plugin(&path, plugin).ok())
                .collect()
        })
        .clone()
}

fn the_kind_of(plugins: Vec<InstalledPlugin>, key: &str) -> EffectKind {
    set_installed_plugins(plugins);
    EffectKind::installed_plugin_kinds()
        .into_iter()
        .find(|kind| kind.key() == key)
        .unwrap_or_else(|| panic!("no plugin called {key}"))
}

fn the_clap_kind_of(id: &str) -> EffectKind {
    the_kind_of(the_clap_plugins(), &format!("clap:{id}"))
}

fn the_lv2_kind_of(folder: &Path, uri: &str) -> EffectKind {
    let bundle = lv2_test_plugins::write_bundle(folder);
    let scan = lv2_host::scan_bundle(&bundle, &mut |_| {}).expect("the bundle is scanned");
    let plugins = scan
        .plugins
        .iter()
        .filter_map(|plugin| effect_of_an_lv2_plugin(&bundle, plugin).ok())
        .collect();
    the_kind_of(plugins, &key_of_an_lv2_plugin(uri))
}

fn the_ladspa_kind_of(label: &str) -> EffectKind {
    let path = ladspa_test_plugins::library_file_for_tests();
    let scan = ladspa_host::scan_library(&path, |_| {}).expect("the test library is scanned");
    let plugins: Vec<InstalledPlugin> = scan
        .plugins
        .iter()
        .filter_map(|plugin| effect_of_a_plugin(&path, plugin).ok())
        .collect();
    let key = plugins
        .iter()
        .find(|plugin| plugin.descriptor.key.ends_with(label))
        .map(|plugin| plugin.descriptor.key.to_string())
        .unwrap_or_else(|| panic!("no plugin called {label}"));
    the_kind_of(plugins, &key)
}

fn an_effect(kind: EffectKind, values: Vec<PropertyValue>, state: Option<&[u8]>) -> LayerEffect {
    LayerEffect {
        state: state.map(Arc::from),
        clock: Default::default(),
        instance: EffectInstanceId::FIRST,
        kind,
        values,
        animation: Animation::default(),
        tables: Vec::new(),
        tail_length: Default::default(),
        passes_notes_on: false,
        shared_by: None,
    }
}

fn a_clip_with(effect: &LayerEffect) -> SoundClipToExport {
    SoundClipToExport {
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
    }
}

fn the_processor(host: &PluginHost, effect: &LayerEffect, notes: &Notes) -> Option<Box<dyn BlockProcessor>> {
    host.processor_for(effect, &a_clip_with(effect), thirty(), notes, false)
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

/// The sound that passes by the processor in block `number`: a different number at every sample.
fn the_sound_passing_by(number: usize, frames: usize) -> (Vec<f32>, Vec<f32>) {
    (
        (0..frames)
            .map(|sample| ((sample + 3 * number) % 17) as f32 / 40.0 - 0.2)
            .collect(),
        (0..frames)
            .map(|sample| ((sample + 5 * number) % 13) as f32 / 30.0 - 0.2)
            .collect(),
    )
}

fn as_bits(sound: &[f32]) -> Vec<u32> {
    sound.iter().map(|sample| sample.to_bits()).collect()
}

/// What a processor makes of blocks of a sound that passes by it, as bits: the blocks are `(frames, notes)`.
fn play(processor: &mut dyn BlockProcessor, blocks: &[(usize, Vec<NoteEvent>)]) -> Vec<(Vec<u32>, Vec<u32>)> {
    blocks
        .iter()
        .enumerate()
        .map(|(number, (frames, notes))| {
            let (mut left, mut right) = the_sound_passing_by(number, *frames);
            processor.give_notes(notes);
            processor.process(&mut left, &mut right);
            (as_bits(&left), as_bits(&right))
        })
        .collect()
}

fn a_script_of_notes() -> Vec<(usize, Vec<NoteEvent>)> {
    vec![
        (512, vec![on(10, 60, 0.5), on(300, 72, 1.0)]),
        (512, Vec::new()),
        (200, vec![off(50, 60)]),
        (512, vec![on(0, 48, 0.25), off(400, 72)]),
    ]
}

fn the_notes_of(notes: &Notes) -> Vec<String> {
    notes.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

/// What `kind` makes of `blocks` with `values`, in this process and in the program, and what the host says of it each time; the two are demanded to be the same.
fn the_same_through_a_program(
    kind: &EffectKind,
    values: Vec<PropertyValue>,
    blocks: &[(usize, Vec<NoteEvent>)],
) -> Vec<(Vec<u32>, Vec<u32>)> {
    let effect = an_effect(*kind, values, None);
    let (here, there) = (
        a_host_with(InAProcessOfItsOwn::Nothing),
        a_host_with(InAProcessOfItsOwn::Everything),
    );
    let (notes_here, notes_there) = (Notes::default(), Notes::default());

    let mut processor_here = the_processor(&here, &effect, &notes_here).expect("the plugin is run in this process");
    let mut processor_there = the_processor(&there, &effect, &notes_there).expect("the plugin is run in a program");
    let (sound_here, sound_there) = (play(&mut *processor_here, blocks), play(&mut *processor_there, blocks));

    assert_eq!(sound_there, sound_here);
    assert_eq!(processor_there.latency(), processor_here.latency());
    assert_eq!(
        the_notes_of(&notes_there),
        the_notes_of(&notes_here),
        "what the host says"
    );
    let key = kind.key();
    assert_eq!(there.meters_of(key), here.meters_of(key), "the meters of the window");
    assert!(
        there.loaded_libraries().is_empty(),
        "the editor loaded no library: the program did"
    );
    sound_here
}

#[test]
fn by_default_no_plugin_runs_in_a_process_of_its_own_and_the_setting_says_which_do() {
    let _alone = the_installed_plugins_to_myself();
    assert_eq!(
        PluginHost::new().runs_in_a_process_of_its_own(),
        InAProcessOfItsOwn::Nothing
    );
    let instrument = the_clap_kind_of(clap_plugins::INSTRUMENT);
    let instrument = instrument.installed_plugin().unwrap();
    let gain = the_clap_kind_of(clap_plugins::GAIN);
    let gain = gain.installed_plugin().unwrap();

    assert!(!InAProcessOfItsOwn::Nothing.includes(instrument) && !InAProcessOfItsOwn::Nothing.includes(gain));
    assert!(InAProcessOfItsOwn::Instruments.includes(instrument) && !InAProcessOfItsOwn::Instruments.includes(gain));
    assert!(InAProcessOfItsOwn::Everything.includes(instrument) && InAProcessOfItsOwn::Everything.includes(gain));
    let host = PluginHost::new().running_in_a_process_of_its_own(InAProcessOfItsOwn::Instruments);
    assert_eq!(host.runs_in_a_process_of_its_own(), InAProcessOfItsOwn::Instruments);
}

#[test]
fn with_the_setting_instruments_an_instrument_is_in_a_program_and_an_effect_is_in_this_process() {
    let _alone = the_installed_plugins_to_myself();
    let host = a_host_with(InAProcessOfItsOwn::Instruments);
    let notes = Notes::default();

    let instrument = an_effect(
        the_clap_kind_of(clap_plugins::INSTRUMENT),
        vec![PropertyValue::Number(1.0)],
        None,
    );
    let mut processor = the_processor(&host, &instrument, &notes).expect("the instrument is run");
    let sound = play(&mut *processor, &[(64, vec![on(0, 50, 1.0)])]);

    assert!(
        host.loaded_libraries().is_empty(),
        "the editor did not load the library of the instrument"
    );
    assert!(f32::from_bits(sound[0].0[10]) > 0.4, "and it was heard");
    let gain = an_effect(
        the_clap_kind_of(clap_plugins::GAIN),
        vec![PropertyValue::Number(1.0)],
        None,
    );
    let _ = the_processor(&host, &gain, &notes).expect("the effect is run");
    assert_eq!(
        host.loaded_libraries().len(),
        1,
        "the effect is run here: the editor loaded its library"
    );
}

#[test]
fn with_the_setting_nothing_every_plugin_is_run_in_this_process_as_it_always_was() {
    let _alone = the_installed_plugins_to_myself();
    let host = a_host_with(InAProcessOfItsOwn::Nothing);
    let instrument = an_effect(
        the_clap_kind_of(clap_plugins::INSTRUMENT),
        vec![PropertyValue::Number(1.0)],
        None,
    );

    let _ = the_processor(&host, &instrument, &Notes::default()).expect("the instrument is run");

    assert_eq!(host.loaded_libraries().len(), 1);
}

#[test]
fn a_clap_instrument_makes_the_same_sound_in_a_program_as_in_this_process() {
    let _alone = the_installed_plugins_to_myself();
    let kind = the_clap_kind_of(clap_plugins::INSTRUMENT);

    let sound = the_same_through_a_program(&kind, vec![PropertyValue::Number(0.5)], &a_script_of_notes());

    // (Not silence against silence: key 60 at velocity 0.5 is 0.3, times the gain 0.5, added to the sound that passes by.)
    assert!(
        sound[0]
            .0
            .iter()
            .any(|sample| (f32::from_bits(*sample) - 0.15).abs() > 0.1)
    );
}

#[test]
fn a_clap_effect_that_is_made_twice_for_its_two_channels_makes_the_same_sound_in_two_programs() {
    let _alone = the_installed_plugins_to_myself();
    let kind = the_clap_kind_of(clap_plugins::MONO_HALF);

    the_same_through_a_program(&kind, Vec::new(), &[(512, Vec::new()), (100, Vec::new())]);
}

#[test]
fn clap_effects_with_values_a_latency_and_a_tail_make_the_same_sound_in_a_program() {
    let _alone = the_installed_plugins_to_myself();
    let blocks = [(512, Vec::new()), (512, Vec::new()), (37, Vec::new())];
    the_same_through_a_program(
        &the_clap_kind_of(clap_plugins::GAIN),
        vec![PropertyValue::Number(2.5)],
        &blocks,
    );
    the_same_through_a_program(&the_clap_kind_of(clap_plugins::LATENCY), Vec::new(), &blocks);
    the_same_through_a_program(&the_clap_kind_of(clap_plugins::TAIL), Vec::new(), &blocks);
    // It asks to be made again, in the program, between blocks.
    the_same_through_a_program(
        &the_clap_kind_of(clap_plugins::CALLBACKS),
        vec![PropertyValue::Number(1.5)],
        &std::iter::repeat_n((512, Vec::new()), 8).collect::<Vec<_>>(),
    );
}

#[test]
fn an_lv2_instrument_and_an_lv2_effect_make_the_same_sound_in_a_program_and_the_meters_are_the_same() {
    let _alone = the_installed_plugins_to_myself();
    let folder = tempfile::tempdir().unwrap();
    let instrument = the_lv2_kind_of(folder.path(), lv2_test_plugins::INSTRUMENT);
    the_same_through_a_program(&instrument, Vec::new(), &a_script_of_notes());
    let gain = the_lv2_kind_of(folder.path(), lv2_test_plugins::GAIN);
    the_same_through_a_program(
        &gain,
        vec![PropertyValue::Number(0.5)],
        &[(512, Vec::new()), (96, Vec::new())],
    );
}

#[test]
fn a_ladspa_effect_makes_the_same_sound_in_a_program() {
    let _alone = the_installed_plugins_to_myself();
    let gain = the_ladspa_kind_of("ffrs_test_gain");

    the_same_through_a_program(
        &gain,
        vec![PropertyValue::Number(0.75)],
        &[(512, Vec::new()), (64, Vec::new())],
    );
}

#[test]
fn the_state_that_the_project_keeps_reaches_the_plugin_in_the_program() {
    let _alone = the_installed_plugins_to_myself();
    let kind = the_clap_kind_of(clap_plugins::EDITOR);
    let blocks = [(64, Vec::new())];
    let sound_with = |state: Option<&[u8]>, which| {
        let effect = an_effect(kind, vec![PropertyValue::Number(1.0)], state);
        let host = a_host_with(which);
        let mut processor = the_processor(&host, &effect, &Notes::default()).expect("the plugin is run");
        play(&mut *processor, &blocks)
    };

    let without = sound_with(None, InAProcessOfItsOwn::Everything);
    let with = sound_with(Some(b"edits=2"), InAProcessOfItsOwn::Everything);

    // (The sound that passes by is replaced by the plugin's: twice as loud with two edits in its state.)
    assert_eq!(f32::from_bits(with[0].0[5]), f32::from_bits(without[0].0[5]) * 2.0);
    assert_eq!(with, sound_with(Some(b"edits=2"), InAProcessOfItsOwn::Nothing));
}

/// What the notes say when `which` is asked for the plugin that `change` made of `base` (and `state` is the plugin's state), and whether a processor was made.
fn what_is_said_when(
    which: InAProcessOfItsOwn,
    change: impl FnOnce(&mut InstalledPlugin),
    state: Option<&[u8]>,
) -> (Vec<String>, bool, Option<String>) {
    let _alone = the_installed_plugins_to_myself();
    let mut plugins = the_clap_plugins();
    let gain = plugins
        .iter_mut()
        .find(|plugin| plugin.descriptor.key.as_ref() == format!("clap:{}", clap_plugins::GAIN))
        .unwrap();
    change(gain);
    let kind = the_kind_of(plugins, &format!("clap:{}", clap_plugins::GAIN));
    let effect = an_effect(kind, vec![PropertyValue::Number(1.0)], state);
    let host = a_host_with(which);
    let notes = Notes::default();

    let made = the_processor(&host, &effect, &notes).is_some();

    (the_notes_of(&notes), made, host.problem_with(kind.key()))
}

#[test]
fn the_words_for_what_goes_wrong_with_a_plugin_are_the_same_in_a_program_as_in_this_process() {
    type Change = fn(&mut InstalledPlugin);
    let cases: [(&str, Change, Option<&[u8]>); 4] = [
        (
            "a library that is not there",
            |plugin| plugin.library = PathBuf::from("/there/is/no/library.clap"),
            None,
        ),
        ("other ports than were scanned", |plugin| plugin.audio_outputs = 1, None),
        (
            "a parameter that it has not",
            |plugin| plugin.value_handles.push(99),
            None,
        ),
        (
            "a plugin that is not in the library",
            |plugin| {
                plugin.address = fastforwardrs::project::PluginAddress::Clap {
                    id: "org.example.nobody".to_owned(),
                }
            },
            None,
        ),
    ];
    for (what, change, state) in cases {
        let here = what_is_said_when(InAProcessOfItsOwn::Nothing, change, state);
        let there = what_is_said_when(InAProcessOfItsOwn::Everything, change, state);

        assert!(!here.0.is_empty(), "{what}: something is said");
        assert_eq!(there, here, "{what}");
        assert!(!here.1, "{what}: no processor is made");
    }
}

#[test]
fn a_plugin_whose_state_it_cannot_read_is_run_all_the_same_and_the_words_are_the_same() {
    let say = |which| {
        let _alone = the_installed_plugins_to_myself();
        let kind = the_clap_kind_of(clap_plugins::EDITOR);
        let effect = an_effect(kind, vec![PropertyValue::Number(1.0)], Some(&[0xFF, 0xFE, 0xFD]));
        let host = a_host_with(which);
        let notes = Notes::default();
        let made = the_processor(&host, &effect, &notes).is_some();
        (the_notes_of(&notes), made, host.problem_with(kind.key()))
    };

    let here = say(InAProcessOfItsOwn::Nothing);
    let there = say(InAProcessOfItsOwn::Everything);

    assert!(
        here.1 && here.0.iter().any(|line| line.contains("did not take the state")),
        "{here:?}"
    );
    assert_eq!(there, here);
}

#[test]
fn a_program_that_is_not_there_or_is_not_of_this_build_leaves_the_sound_as_it_was_and_says_why() {
    let _alone = the_installed_plugins_to_myself();
    let instrument = an_effect(
        the_clap_kind_of(clap_plugins::INSTRUMENT),
        vec![PropertyValue::Number(1.0)],
        None,
    );
    let this_test_program = std::env::current_exe().unwrap();

    for (program, words) in [
        (PathBuf::from("/there/is/no/program"), "could not be run"),
        (this_test_program, "was built from other code than the editor"),
    ] {
        let host = PluginHost::new()
            .running_in_a_process_of_its_own(InAProcessOfItsOwn::Instruments)
            .with_the_program_that_runs_plugins(program.clone());
        host.allow_everything();
        let notes = Notes::default();

        let processor = the_processor(&host, &instrument, &notes);

        assert!(
            processor.is_none(),
            "{program:?}: the instrument is not run in this process instead"
        );
        assert!(host.loaded_libraries().is_empty(), "{program:?}");
        let said = the_notes_of(&notes);
        assert!(
            said.len() == 1 && said[0].contains(words) && said[0].contains("The sound goes through it unchanged"),
            "{program:?}: {said:?}"
        );
        assert_eq!(host.problem_with(instrument.kind.key()), Some(said[0].clone()));
    }
}

#[test]
fn a_stream_that_is_being_stopped_makes_no_instrument() {
    let _alone = the_installed_plugins_to_myself();
    let host = a_host_with(InAProcessOfItsOwn::Instruments);
    let instrument = an_effect(
        the_clap_kind_of(clap_plugins::INSTRUMENT),
        vec![PropertyValue::Number(1.0)],
        None,
    );
    let notes = Notes::default();
    host.stream_is_stopping(&notes);

    assert!(the_processor(&host, &instrument, &notes).is_none());
    assert!(
        the_processor(&host, &instrument, &Notes::default()).is_some(),
        "another stream makes one"
    );
}

#[test]
fn a_plugin_that_crashes_its_program_while_it_plays_is_left_out_of_that_play_and_said_and_the_next_one_is_made_again() {
    let _alone = the_installed_plugins_to_myself();
    let host = PluginHost::new()
        .running_in_a_process_of_its_own(InAProcessOfItsOwn::Instruments)
        .with_the_program_that_runs_plugins(the_program())
        .with_the_program_given(
            vec![("FFRS_TEST_PLUGIN".to_owned(), "crash_when_run".to_owned())],
            Limits {
                to_connect: Duration::from_secs(30),
                to_make: Duration::from_secs(60),
                to_run: Duration::from_secs(60),
            },
        );
    host.allow_everything();
    let instrument = an_effect(
        the_clap_kind_of(clap_plugins::INSTRUMENT),
        vec![PropertyValue::Number(1.0)],
        None,
    );
    let notes = Notes::default();
    let mut processor = the_processor(&host, &instrument, &notes).expect("the instrument is made");

    let sound = play(&mut *processor, &[(64, vec![on(0, 50, 1.0)]), (64, Vec::new())]);

    // The sound that passes by is as it was (an instrument that is left out adds nothing), and the person is told.
    for (number, block) in sound.iter().enumerate() {
        let (left, right) = the_sound_passing_by(number, 64);
        assert_eq!(
            (&block.0, &block.1),
            (&as_bits(&left), &as_bits(&right)),
            "block {number}"
        );
    }
    let said = the_notes_of(&notes);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(
        said[0].contains("could not be run") && said[0].contains("the program that runs the plugin ended"),
        "{said:?}"
    );
    assert_eq!(host.problem_with(instrument.kind.key()), Some(said[0].clone()));
    // The next play makes a new program (which crashes the same way, but is made).
    assert!(the_processor(&host, &instrument, &Notes::default()).is_some());
}

#[test]
fn a_plugin_that_crashes_its_program_while_it_is_made_leaves_the_sound_as_it_was_and_says_so_in_these_words() {
    let _alone = the_installed_plugins_to_myself();
    let host = PluginHost::new()
        .running_in_a_process_of_its_own(InAProcessOfItsOwn::Instruments)
        .with_the_program_that_runs_plugins(the_program())
        .with_the_program_given(
            vec![("FFRS_TEST_PLUGIN".to_owned(), "crash_when_made".to_owned())],
            Limits {
                to_connect: Duration::from_secs(30),
                to_make: Duration::from_secs(60),
                to_run: Duration::from_secs(60),
            },
        );
    host.allow_everything();
    let instrument = an_effect(
        the_clap_kind_of(clap_plugins::INSTRUMENT),
        vec![PropertyValue::Number(1.0)],
        None,
    );
    let notes = Notes::default();

    let processor = the_processor(&host, &instrument, &notes);

    assert!(
        processor.is_none(),
        "an instrument that could not be made is not run in this process instead"
    );
    let said = the_notes_of(&notes);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(
        said[0].starts_with("The sound plugin \u{ab}")
            && said[0].contains("could not be run in a program of its own: the program that runs the plugin ended")
            && said[0].ends_with("The sound goes through it unchanged."),
        "{said:?}"
    );
    assert_eq!(host.problem_with(instrument.kind.key()), Some(said[0].clone()));
    assert!(host.loaded_libraries().is_empty());
}

#[test]
fn an_instrument_whose_program_crashes_now_and_then_is_heard_as_if_it_did_not_and_the_person_is_told_each_time() {
    let _alone = the_installed_plugins_to_myself();
    let crashing = PluginHost::new()
        .running_in_a_process_of_its_own(InAProcessOfItsOwn::Instruments)
        .with_the_program_that_runs_plugins(the_program())
        .with_the_program_given(
            vec![("FFRS_TEST_PLUGIN".to_owned(), "crash_at_every_fourth_run".to_owned())],
            Limits {
                to_connect: Duration::from_secs(60),
                to_make: Duration::from_secs(60),
                to_run: Duration::from_secs(60),
            },
        );
    crashing.allow_everything();
    let plain = a_host_with(InAProcessOfItsOwn::Nothing);
    let instrument = an_effect(
        the_clap_kind_of(clap_plugins::INSTRUMENT),
        vec![PropertyValue::Number(1.0)],
        None,
    );
    // Twelve blocks with notes that are held across the blocks the program crashes in (the fourth, the seventh and the tenth).
    let blocks: Vec<(usize, Vec<NoteEvent>)> = vec![
        (512, vec![on(10, 60, 0.5)]),
        (512, Vec::new()),
        (512, vec![on(100, 72, 1.0)]),
        (512, vec![off(50, 60)]),
        (512, Vec::new()),
        (512, Vec::new()),
        (512, vec![on(0, 48, 0.25)]),
        (512, vec![off(300, 72)]),
        (512, Vec::new()),
        (512, vec![on(5, 55, 0.8)]),
        (512, Vec::new()),
        (512, vec![off(100, 55)]),
    ];
    let (notes_crashing, notes_plain) = (Notes::default(), Notes::default());
    let mut processor_crashing = the_processor(&crashing, &instrument, &notes_crashing).expect("the instrument is run");
    let mut processor_plain = the_processor(&plain, &instrument, &notes_plain).expect("the instrument is run");

    let heard = play(&mut *processor_crashing, &blocks);
    let wanted = play(&mut *processor_plain, &blocks);

    assert_eq!(
        heard, wanted,
        "the sound is the sound of a program that does not crash, bit for bit"
    );
    assert!(
        heard[3].0.iter().any(|sample| f32::from_bits(*sample) > 0.5),
        "and it is not silence"
    );
    let said = the_notes_of(&notes_crashing);
    assert_eq!(said.len(), 3, "once for each crash: {said:?}");
    assert!(
        said.iter().all(|line| line.starts_with("The sound plugin \u{ab}")
            && line.contains("the program that runs the plugin ended")
            && line.contains("and was started again; the notes that were held are played again from here")),
        "{said:?}"
    );
    assert!(the_notes_of(&notes_plain).is_empty());
    assert_eq!(
        crashing.problem_with(instrument.kind.key()),
        None,
        "an instrument that was started again is not left out"
    );
}
