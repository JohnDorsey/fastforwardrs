// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! When a plugin is a synthesizer (phase 13, piece 13.2): a plugin with **no audio input and a note input** (CLAP) or a MIDI input (LV2). Plugins that are described by hand, so that each
//! case is exactly what is tried.

use std::path::{Path, PathBuf};

use clap_host::scan::{AudioPort, NOTE_DIALECT_CLAP, NOTE_DIALECT_MIDI, NotePort, ScannedPlugin as ScannedClapPlugin};
use lv2_host::{
    AtomPortDescription, PluginDescription, PortDescription, PortDirection, PortKind, ScannedPlugin as ScannedLv2Plugin,
};

use super::effects::{effect_of_a_clap_plugin, effect_of_an_lv2_plugin};
use crate::project::EffectDomain;

const MIDI_EVENT: &str = "http://lv2plug.in/ns/ext/midi#MidiEvent";
const PATCH_MESSAGE: &str = "http://lv2plug.in/ns/ext/patch#Message";

// --- CLAP ---

fn an_audio_port(channels: u32) -> AudioPort {
    AudioPort {
        id: 0,
        name: "Main".to_owned(),
        channel_count: channels,
        port_type: Some(if channels == 1 { "mono" } else { "stereo" }.to_owned()),
        is_main: true,
    }
}

fn a_note_port(dialects: u32) -> NotePort {
    NotePort {
        id: 0,
        name: "Notes".to_owned(),
        supported_dialects: dialects,
        preferred_dialect: 0,
    }
}

/// A CLAP plugin with these audio ports (the channels of each) and note inputs.
fn a_clap_plugin(inputs: &[u32], outputs: &[u32], note_inputs: Vec<NotePort>) -> ScannedClapPlugin {
    ScannedClapPlugin {
        description: clap_host::PluginDescription {
            id: "com.example.synth".to_owned(),
            name: "Big Synth".to_owned(),
            vendor: "Some Maker".to_owned(),
            description: "A synthesizer".to_owned(),
            ..clap_host::PluginDescription::default()
        },
        audio_inputs: inputs.iter().map(|channels| an_audio_port(*channels)).collect(),
        audio_outputs: outputs.iter().map(|channels| an_audio_port(*channels)).collect(),
        note_inputs,
        parameters: Vec::new(),
        latency: 0,
        tail: None,
        has_state: false,
        has_gui: false,
        start: clap_host::start_check::StartCheck::NotChecked,
        problem: None,
    }
}

fn clap_effect(plugin: &ScannedClapPlugin) -> Result<crate::project::InstalledPlugin, String> {
    effect_of_a_clap_plugin(Path::new("/clap/maker.clap"), plugin)
}

#[test]
fn a_clap_plugin_with_no_audio_input_and_a_note_input_is_a_synthesizer_with_the_channels_of_its_output() {
    for (outputs, wanted) in [(1, 1), (2, 2)] {
        let plugin = a_clap_plugin(
            &[],
            &[outputs],
            vec![a_note_port(NOTE_DIALECT_CLAP | NOTE_DIALECT_MIDI)],
        );

        let effect = clap_effect(&plugin).expect("a synthesizer can be an effect");

        assert_eq!((effect.audio_inputs, effect.audio_outputs), (0, wanted));
        assert!(effect.descriptor.makes_sound_from_notes);
        assert_eq!(effect.descriptor.domain, EffectDomain::Sound);
        assert_eq!(effect.descriptor.key, "clap:com.example.synth");
    }
}

#[test]
fn a_note_input_that_takes_the_events_of_clap_or_midi_makes_a_synthesizer_and_one_that_takes_neither_does_not() {
    for dialects in [NOTE_DIALECT_CLAP, NOTE_DIALECT_MIDI] {
        let plugin = a_clap_plugin(&[], &[2], vec![a_note_port(dialects)]);
        assert!(
            clap_effect(&plugin).unwrap().descriptor.makes_sound_from_notes,
            "{dialects}"
        );
    }
    // (MIDI with the expression of MPE (bit 2) and MIDI 2 (bit 3) are not what the host gives.)
    for dialects in [0, 1 << 2, 1 << 3] {
        let plugin = a_clap_plugin(&[], &[2], vec![a_note_port(dialects)]);
        let error = clap_effect(&plugin).unwrap_err();
        assert!(
            error.contains("no audio input and no note input"),
            "{dialects}: {error}"
        );
    }
}

#[test]
fn a_clap_plugin_that_has_a_second_note_input_is_a_synthesizer_if_any_takes_notes() {
    let plugin = a_clap_plugin(&[], &[2], vec![a_note_port(0), a_note_port(NOTE_DIALECT_CLAP)]);

    assert!(clap_effect(&plugin).unwrap().descriptor.makes_sound_from_notes);
}

#[test]
fn a_clap_plugin_with_neither_an_audio_input_nor_a_note_input_is_a_generator_and_is_not_used() {
    let plugin = a_clap_plugin(&[], &[2], Vec::new());

    let error = clap_effect(&plugin).unwrap_err();

    assert!(error.contains("no audio input and no note input"), "{error}");
    assert!(error.contains("generator"), "{error}");
}

#[test]
fn a_clap_plugin_with_an_audio_input_is_an_effect_even_if_it_has_a_note_input() {
    let plugin = a_clap_plugin(&[2], &[2], vec![a_note_port(NOTE_DIALECT_CLAP | NOTE_DIALECT_MIDI)]);

    let effect = clap_effect(&plugin).unwrap();

    assert_eq!((effect.audio_inputs, effect.audio_outputs), (2, 2));
    assert!(
        !effect.descriptor.makes_sound_from_notes,
        "it hears sound, and the notes are not its to take"
    );
}

#[test]
fn a_clap_synthesizer_needs_an_output_that_is_mono_or_stereo_and_one_that_can_be_run() {
    let no_output = a_clap_plugin(&[], &[], vec![a_note_port(NOTE_DIALECT_CLAP)]);
    assert!(clap_effect(&no_output).unwrap_err().contains("no audio output"));

    let surround = a_clap_plugin(&[], &[6], vec![a_note_port(NOTE_DIALECT_CLAP)]);
    let error = clap_effect(&surround).unwrap_err();
    assert!(error.contains("6 channels"), "{error}");

    let mut broken = a_clap_plugin(&[], &[2], vec![a_note_port(NOTE_DIALECT_CLAP)]);
    broken.problem = Some("it panicked".to_owned());
    assert!(clap_effect(&broken).unwrap_err().contains("could not be made or run"));
}

// --- LV2 ---

fn an_lv2_port(index: u32, direction: PortDirection, kind: PortKind, supports: &[&str]) -> PortDescription {
    PortDescription {
        index,
        symbol: format!("port{index}"),
        name: format!("Port {index}"),
        direction,
        kind,
        default: None,
        minimum: None,
        maximum: None,
        toggled: false,
        integer: false,
        enumeration: false,
        logarithmic: false,
        sample_rate_relative: false,
        connection_is_optional: false,
        reports_latency: false,
        designations: Vec::new(),
        scale_points: Vec::new(),
        unit: None,
        atom: (kind == PortKind::Atom).then(|| AtomPortDescription {
            buffer_type: Some("http://lv2plug.in/ns/ext/atom#Sequence".to_owned()),
            supports: supports.iter().map(|kind| (*kind).to_owned()).collect(),
            minimum_size: None,
        }),
    }
}

/// An LV2 plugin with `audio_inputs` audio inputs, `audio_outputs` audio outputs, and this atom port (if any: its direction and what it supports), numbered after them.
fn an_lv2_plugin(audio_inputs: u32, audio_outputs: u32, atom: Option<(PortDirection, &[&str])>) -> ScannedLv2Plugin {
    let mut ports = Vec::new();
    for _ in 0..audio_inputs {
        ports.push(an_lv2_port(
            ports.len() as u32,
            PortDirection::Input,
            PortKind::Audio,
            &[],
        ));
    }
    for _ in 0..audio_outputs {
        ports.push(an_lv2_port(
            ports.len() as u32,
            PortDirection::Output,
            PortKind::Audio,
            &[],
        ));
    }
    if let Some((direction, supports)) = atom {
        ports.push(an_lv2_port(ports.len() as u32, direction, PortKind::Atom, supports));
    }
    ScannedLv2Plugin {
        description: PluginDescription {
            uri: "http://example.org/plugins/synth".to_owned(),
            name: "The Synth".to_owned(),
            categories: Vec::new(),
            author: String::new(),
            bundle: PathBuf::from("/lv2/synth.lv2"),
            binary: PathBuf::from("/lv2/synth.lv2/synth.so"),
            minor_version: 1,
            micro_version: 0,
            ports,
            required_features: Vec::new(),
            optional_features: Vec::new(),
            extension_data: Vec::new(),
            user_interfaces: Vec::new(),
        },
        problem: None,
        latency: 0,
        has_state: false,
    }
}

fn lv2_effect(plugin: &ScannedLv2Plugin) -> Result<crate::project::InstalledPlugin, String> {
    effect_of_an_lv2_plugin(Path::new("/lv2/synth.lv2"), plugin)
}

#[test]
fn an_lv2_plugin_with_no_audio_input_and_a_midi_input_is_a_synthesizer_with_one_or_two_outputs() {
    for outputs in [1, 2] {
        let plugin = an_lv2_plugin(0, outputs, Some((PortDirection::Input, &[MIDI_EVENT])));

        let effect = lv2_effect(&plugin).expect("a synthesizer can be an effect");

        assert_eq!((effect.audio_inputs, effect.audio_outputs), (0, outputs as usize));
        assert!(effect.descriptor.makes_sound_from_notes);
        assert_eq!(effect.descriptor.domain, EffectDomain::Sound);
    }
}

#[test]
fn an_atom_input_that_does_not_take_midi_or_a_midi_output_does_not_make_a_synthesizer() {
    let message = an_lv2_plugin(0, 2, Some((PortDirection::Input, &[PATCH_MESSAGE])));
    let only_out = an_lv2_plugin(0, 2, Some((PortDirection::Output, &[MIDI_EVENT])));
    let nothing = an_lv2_plugin(0, 2, None);

    for plugin in [message, only_out, nothing] {
        let error = lv2_effect(&plugin).unwrap_err();
        assert!(error.contains("no audio input and no MIDI input"), "{error}");
    }
}

#[test]
fn an_lv2_plugin_that_supports_midi_among_other_messages_takes_midi() {
    let plugin = an_lv2_plugin(0, 2, Some((PortDirection::Input, &[PATCH_MESSAGE, MIDI_EVENT])));

    assert!(lv2_effect(&plugin).unwrap().descriptor.makes_sound_from_notes);
}

#[test]
fn an_lv2_instrument_with_no_output_or_with_more_than_two_is_not_used_and_says_why() {
    let none = an_lv2_plugin(0, 0, Some((PortDirection::Input, &[MIDI_EVENT])));
    assert!(lv2_effect(&none).unwrap_err().contains("no audio output"));

    let many = an_lv2_plugin(0, 3, Some((PortDirection::Input, &[MIDI_EVENT])));
    let error = lv2_effect(&many).unwrap_err();
    assert!(error.contains("an instrument with 3 audio outputs"), "{error}");
}

#[test]
fn an_lv2_plugin_with_an_audio_input_is_an_effect_even_if_it_has_a_midi_input() {
    let plugin = an_lv2_plugin(2, 2, Some((PortDirection::Input, &[MIDI_EVENT])));

    let effect = lv2_effect(&plugin).unwrap();

    assert_eq!((effect.audio_inputs, effect.audio_outputs), (2, 2));
    assert!(!effect.descriptor.makes_sound_from_notes);
}

#[test]
fn a_plugin_that_is_a_synthesizer_cannot_stand_in_for_a_snapshot_of_an_effect_of_the_same_key_and_the_other_way_round()
{
    let synthesizer = clap_effect(&a_clap_plugin(&[], &[2], vec![a_note_port(NOTE_DIALECT_CLAP)])).unwrap();
    let mut as_an_effect = synthesizer.descriptor.clone();
    as_an_effect.makes_sound_from_notes = false;

    assert!(!synthesizer.descriptor.can_stand_in_for(&as_an_effect));
    assert!(!as_an_effect.can_stand_in_for(&synthesizer.descriptor));
    assert!(synthesizer.descriptor.can_stand_in_for(&synthesizer.descriptor));
}
