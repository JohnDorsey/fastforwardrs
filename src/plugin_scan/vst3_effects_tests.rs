// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a VST3 plugin that was scanned is, as a type of effect (piece 15.2 of `docs/engine-phase-15-vst3-design.md`), with plugins that are described by hand so that each case is
//! exactly what is tried. (The plugins of the test library, scanned by the real scan program, are in `tests/plugin_scan_vst3.rs`.)

use std::path::{Path, PathBuf};

use vst3_host::{
    BusDescription, ClassDescription, ClassFacts, ClassId, FactoryDescription, ModuleScan, ParameterDescription,
    ScannedClass,
};

use super::effects::{
    MOST_PRE_ROLL_SAMPLES, effect_of_a_vst3_plugin, key_of_a_vst3_plugin, plugins_found_in, pre_roll_of_a_vst3_plugin,
};
use super::scanner::{LibraryOutcome, LibraryReport};
use crate::project::{MOST_VALUES_OF_AN_EFFECT, ParameterControl, PluginAddress, PropertyValue, ValueScale};
use crate::sound_engine::SAMPLE_RATE;

fn the_id() -> ClassId {
    ClassId::from_numbers(0x1234_5678, 0x9ABC_DEF0, 0x0FED_CBA9, 0x8765_4321)
}

fn a_bus(channels: u32, is_main: bool) -> BusDescription {
    BusDescription {
        name: "bus".to_owned(),
        channels,
        is_main,
        is_on_by_default: true,
    }
}

fn a_parameter(id: u32, title: &str) -> ParameterDescription {
    ParameterDescription {
        id,
        title: title.to_owned(),
        short_title: title.to_owned(),
        units: String::new(),
        step_count: 0,
        default_normalized: 0.0,
        default_plain: 0.0,
        plain_minimum: 0.0,
        plain_maximum: 1.0,
        texts: Vec::new(),
        can_automate: true,
        is_read_only: false,
        is_hidden: false,
        is_a_list: false,
        is_the_bypass: false,
        is_a_program_change: false,
        wraps_around: false,
    }
}

fn a_stereo_effect_with(parameters: Vec<ParameterDescription>) -> ScannedClass {
    ScannedClass {
        class: ClassDescription {
            id: the_id(),
            name: "The Amp".to_owned(),
            category: "Audio Module Class".to_owned(),
            sub_categories: vec!["Fx".to_owned(), "Dynamics".to_owned()],
            vendor: "Somebody".to_owned(),
            version: "1.0".to_owned(),
            sdk_version: "VST 3.7".to_owned(),
        },
        facts: Some(ClassFacts {
            audio_inputs: vec![a_bus(2, true)],
            audio_outputs: vec![a_bus(2, true)],
            event_inputs: Vec::new(),
            parameters,
            has_a_controller: true,
            controller_is_separate: true,
            has_midi_mapping: false,
            latency: 0,
            tail: 0,
        }),
        problem: None,
        has_state: false,
        has_a_window: false,
    }
}

fn with_facts(mut class: ScannedClass, change: impl FnOnce(&mut ClassFacts)) -> ScannedClass {
    change(class.facts.as_mut().expect("it has facts"));
    class
}

fn the_effect(class: &ScannedClass) -> Result<crate::project::InstalledPlugin, String> {
    effect_of_a_vst3_plugin(Path::new("/plugins/amp.vst3"), class)
}

fn the_value(parameter: ParameterDescription) -> crate::project::ParameterDescriptor {
    let effect = the_effect(&a_stereo_effect_with(vec![parameter])).expect("it is an effect");
    assert_eq!(effect.descriptor.parameters.len(), 1, "the parameter is a value");
    effect.descriptor.parameters[0].clone()
}

#[test]
fn a_plugin_is_an_effect_with_the_key_the_name_the_address_and_the_channels_that_it_says() {
    let effect = the_effect(&a_stereo_effect_with(Vec::new())).expect("it is an effect");

    assert_eq!(effect.descriptor.key, "vst3:123456789ABCDEF00FEDCBA987654321");
    assert_eq!(key_of_a_vst3_plugin(the_id()), "vst3:123456789ABCDEF00FEDCBA987654321");
    assert_eq!(
        effect.descriptor.name, "the amp",
        "in lower case, as the names of effects are"
    );
    assert_eq!(effect.descriptor.category, "Somebody", "the vendor");
    assert_eq!(effect.descriptor.description, "VST3 plugin (Fx, Dynamics), in amp.vst3");
    assert_eq!(effect.address, PluginAddress::Vst3 { class_id: the_id() });
    assert_eq!(effect.library, PathBuf::from("/plugins/amp.vst3"));
    assert_eq!((effect.audio_inputs, effect.audio_outputs), (2, 2));
    assert!(!effect.descriptor.makes_sound_from_notes);
    assert!(!effect.has_a_window, "the scan found no window");
    assert_eq!(effect.start_warning, None);
    assert_eq!(effect.descriptor.problem(), None);
}

#[test]
fn a_plugin_has_a_window_when_the_scan_found_that_its_controller_makes_one() {
    let mut class = a_stereo_effect_with(Vec::new());
    class.has_a_window = true;

    let effect = the_effect(&class).expect("it is an effect");

    assert!(effect.has_a_window);
    assert!(!the_effect(&a_stereo_effect_with(Vec::new())).unwrap().has_a_window);
}

#[test]
fn a_class_without_a_vendor_is_in_the_category_plugins_and_a_name_with_capitals_is_in_lower_case() {
    let mut class = a_stereo_effect_with(Vec::new());
    class.class.vendor = "  ".to_owned();
    class.class.name = "  LOUD Thing ".to_owned();
    class.class.sub_categories = Vec::new();
    let effect = the_effect(&class).expect("it is an effect");
    assert_eq!(effect.descriptor.category, "Plugins");
    assert_eq!(effect.descriptor.name, "loud thing");
    assert_eq!(effect.descriptor.description, "VST3 plugin, in amp.vst3");
}

#[test]
fn mono_and_stereo_main_buses_are_used_and_other_numbers_of_channels_are_refused_in_words() {
    let mono = with_facts(a_stereo_effect_with(Vec::new()), |facts| {
        facts.audio_inputs = vec![a_bus(1, true)];
        facts.audio_outputs = vec![a_bus(1, true)];
    });
    let effect = the_effect(&mono).expect("a mono effect");
    assert_eq!((effect.audio_inputs, effect.audio_outputs), (1, 1));

    let one_in_two_out = with_facts(a_stereo_effect_with(Vec::new()), |facts| {
        facts.audio_inputs = vec![a_bus(1, true)]
    });
    let effect = the_effect(&one_in_two_out).expect("a mono input and a stereo output");
    assert_eq!((effect.audio_inputs, effect.audio_outputs), (1, 2));

    let surround = with_facts(a_stereo_effect_with(Vec::new()), |facts| {
        facts.audio_outputs = vec![a_bus(6, true)]
    });
    let reason = the_effect(&surround).expect_err("six channels are not used");
    assert!(reason.contains("main audio output has 6 channels"), "{reason}");
    let nothing = with_facts(a_stereo_effect_with(Vec::new()), |facts| {
        facts.audio_inputs = vec![a_bus(0, true)]
    });
    let reason = the_effect(&nothing).expect_err("no channels are not used");
    assert!(reason.contains("main audio input has 0 channels"), "{reason}");
}

#[test]
fn the_main_bus_is_the_one_that_says_so_and_a_side_chain_before_it_or_after_it_is_left_off() {
    let with_a_side_chain = with_facts(a_stereo_effect_with(Vec::new()), |facts| {
        facts.audio_inputs = vec![a_bus(2, false), a_bus(1, true)];
        facts.audio_outputs = vec![a_bus(2, true), a_bus(2, false), a_bus(2, false)];
    });
    let effect = the_effect(&with_a_side_chain).expect("an effect with a side chain");
    assert_eq!(
        (effect.audio_inputs, effect.audio_outputs),
        (1, 2),
        "the main input is the second bus"
    );
}

#[test]
fn a_class_with_no_main_input_and_an_event_input_is_a_synthesizer_and_one_with_neither_is_not_used() {
    let synthesizer = with_facts(a_stereo_effect_with(Vec::new()), |facts| {
        facts.audio_inputs = vec![a_bus(2, false)]; // (Surge XT: its only input is a side chain)
        facts.event_inputs = vec![a_bus(16, true)];
    });
    let effect = the_effect(&synthesizer).expect("a synthesizer");
    assert!(effect.descriptor.makes_sound_from_notes);
    assert_eq!((effect.audio_inputs, effect.audio_outputs), (0, 2));

    let effect_with_notes = with_facts(a_stereo_effect_with(Vec::new()), |facts| {
        facts.event_inputs = vec![a_bus(16, true)]
    });
    assert!(
        !the_effect(&effect_with_notes)
            .expect("an effect that also takes notes")
            .descriptor
            .makes_sound_from_notes,
        "an effect with a main input is an effect, whatever else it takes"
    );

    let generator = with_facts(a_stereo_effect_with(Vec::new()), |facts| {
        facts.audio_inputs = Vec::new()
    });
    let reason = the_effect(&generator).expect_err("a generator is not used");
    assert!(reason.contains("makes sound by itself"), "{reason}");

    let silent = with_facts(a_stereo_effect_with(Vec::new()), |facts| {
        facts.audio_outputs = vec![a_bus(2, false)]
    });
    assert_eq!(
        the_effect(&silent).expect_err("no main output"),
        "it has no main audio output"
    );
}

#[test]
fn a_class_that_could_not_be_made_or_run_says_why_and_is_not_an_effect() {
    let mut class = a_stereo_effect_with(Vec::new());
    class.problem = Some("the plugin would not initialize (it said 0x1)".to_owned());
    let reason = the_effect(&class).expect_err("it is not used");
    assert!(
        reason.contains("could not be made or run") && reason.contains("would not initialize"),
        "{reason}"
    );
    class.problem = None;
    class.facts = None;
    assert_eq!(the_effect(&class).expect_err("no facts"), "it could not be made");
}

#[test]
fn a_parameter_is_a_value_only_if_it_is_shown_can_be_set_can_be_automated_and_is_not_a_change_of_program() {
    let mut hidden = a_parameter(1, "Hidden");
    hidden.is_hidden = true;
    let mut read_only = a_parameter(2, "Meter");
    read_only.is_read_only = true;
    let mut fixed = a_parameter(3, "Fixed");
    fixed.can_automate = false;
    let mut program = a_parameter(4, "Program");
    program.is_a_program_change = true;
    let shown = a_parameter(5, "Shown");

    let effect = the_effect(&a_stereo_effect_with(vec![hidden, read_only, fixed, program, shown])).expect("an effect");

    assert_eq!(effect.value_handles, [5], "only the fifth is a value");
    assert_eq!(effect.descriptor.parameters.len(), 1);
    assert_eq!(effect.descriptor.parameters[0].id, "param5");
    assert_eq!(effect.descriptor.parameters[0].label, "Shown");
    assert_eq!(effect.descriptor.parameters[0].name, "shown");
}

#[test]
fn a_continuous_parameter_is_a_number_in_the_plugins_own_units_with_its_default_and_its_range() {
    let mut gain = a_parameter(1, "Gain");
    gain.units = " dB ".to_owned();
    gain.plain_minimum = -60.0;
    gain.plain_maximum = 12.0;
    gain.default_plain = 0.0;
    gain.default_normalized = 60.0 / 72.0;

    let value = the_value(gain);

    assert_eq!(value.control, ParameterControl::Automatic);
    assert_eq!(value.default, PropertyValue::Number(0.0));
    assert_eq!(value.range, Some((-60.0, 12.0)));
    assert_eq!(value.unit, "dB", "the units, without the spaces");
    assert_eq!(value.scale, ValueScale::Linear);
}

#[test]
fn a_default_outside_the_range_is_held_to_it_and_a_range_that_is_not_one_makes_the_plugin_unusable() {
    let mut outside = a_parameter(1, "Out");
    outside.default_plain = 5.0;
    assert_eq!(the_value(outside).default, PropertyValue::Number(1.0));
    let mut not_a_number = a_parameter(2, "Nan");
    not_a_number.default_plain = f64::NAN;
    assert_eq!(the_value(not_a_number).default, PropertyValue::Number(0.0), "the least");

    let mut backwards = a_parameter(3, "Backwards");
    backwards.plain_minimum = 2.0;
    backwards.plain_maximum = 1.0;
    let reason = the_effect(&a_stereo_effect_with(vec![backwards])).expect_err("a bad range");
    assert!(reason.contains("Backwards") && reason.contains("2 to 1"), "{reason}");
    let mut infinite = a_parameter(4, "Endless");
    infinite.plain_maximum = f64::INFINITY;
    assert!(the_effect(&a_stereo_effect_with(vec![infinite])).is_err());
}

#[test]
fn a_range_of_a_thousand_times_above_zero_is_on_a_logarithmic_slider_and_less_is_not() {
    let mut frequency = a_parameter(1, "Frequency");
    frequency.plain_minimum = 20.0;
    frequency.plain_maximum = 20_000.0;
    assert_eq!(the_value(frequency.clone()).scale, ValueScale::Logarithmic);
    frequency.plain_maximum = 19_000.0;
    assert_eq!(
        the_value(frequency.clone()).scale,
        ValueScale::Linear,
        "a little less than a thousand"
    );
    frequency.plain_minimum = 0.0;
    frequency.plain_maximum = 20_000.0;
    assert_eq!(the_value(frequency).scale, ValueScale::Linear, "from nought is not");
}

#[test]
fn a_parameter_of_one_step_from_nought_to_one_is_a_switch_and_the_bypass_is_one_too() {
    let mut on_off = a_parameter(1, "Power");
    on_off.step_count = 1;
    on_off.default_plain = 1.0;
    on_off.texts = vec!["Off".to_owned(), "On".to_owned()];
    let value = the_value(on_off);
    assert_eq!(value.control, ParameterControl::Toggle);
    assert_eq!(value.default, PropertyValue::Flag(true));

    let mut bypass = a_parameter(2, "Bypass");
    bypass.step_count = 1;
    bypass.is_the_bypass = true;
    let value = the_value(bypass);
    assert_eq!(value.control, ParameterControl::Toggle);
    assert_eq!(value.default, PropertyValue::Flag(false));
}

#[test]
fn a_list_with_a_text_for_each_step_is_a_choice_whose_options_are_the_plain_values() {
    let mut mode = a_parameter(1, "Mode");
    mode.step_count = 2;
    mode.is_a_list = true;
    mode.plain_maximum = 2.0;
    mode.default_plain = 1.0;
    mode.texts = vec!["Plain".to_owned(), " ".to_owned(), "Triple".to_owned()];

    let value = the_value(mode);

    let ParameterControl::Choice(options) = &value.control else {
        panic!("a choice: {:?}", value.control);
    };
    let seen: Vec<(&str, &str)> = options.iter().map(|option| (&*option.id, &*option.label)).collect();
    assert_eq!(
        seen,
        [("0", "Plain"), ("1", "1"), ("2", "Triple")],
        "a blank text is shown as its number"
    );
    assert_eq!(value.default, PropertyValue::Words("1".to_owned()));
}

#[test]
fn a_list_that_starts_above_nought_has_options_that_start_there() {
    let mut octave = a_parameter(1, "Octave");
    octave.step_count = 2;
    octave.plain_minimum = -1.0;
    octave.plain_maximum = 1.0;
    octave.default_plain = 5.0; // (past the range: held to it)
    octave.texts = vec!["Down".to_owned(), "Same".to_owned(), "Up".to_owned()];
    let value = the_value(octave);
    let ParameterControl::Choice(options) = &value.control else {
        panic!("a choice");
    };
    assert_eq!(
        options.iter().map(|option| &*option.id).collect::<Vec<_>>(),
        ["-1", "0", "1"]
    );
    assert_eq!(value.default, PropertyValue::Words("1".to_owned()));
}

#[test]
fn a_stepped_parameter_without_texts_that_fit_is_a_whole_number() {
    let mut voices = a_parameter(1, "Voices");
    voices.step_count = 15;
    voices.plain_minimum = 1.0;
    voices.plain_maximum = 16.0;
    voices.default_plain = 4.4;
    let value = the_value(voices);
    assert_eq!(value.control, ParameterControl::Integer);
    assert_eq!(value.default, PropertyValue::Number(4.0));
    assert_eq!(value.range, Some((1.0, 16.0)));

    // Texts for another number of steps than the range holds are not the options of a choice.
    let mut odd = a_parameter(2, "Odd");
    odd.step_count = 2;
    odd.plain_maximum = 1.0;
    odd.texts = vec!["A".to_owned(), "B".to_owned(), "C".to_owned()];
    assert_eq!(the_value(odd).control, ParameterControl::Integer);
}

#[test]
fn a_parameter_without_a_title_is_called_by_its_number() {
    let value = the_value(a_parameter(77, "  "));
    assert_eq!(value.label, "parameter 77");
    assert_eq!(value.id, "param77");
}

#[test]
fn a_plugin_with_hundreds_of_values_has_them_all() {
    // (Surge XT has 774 automatable parameters; a limit of 255 left out three of every four until 2026-11-03.)
    let parameters: Vec<ParameterDescription> = (0..3000).map(|id| a_parameter(id + 1000, &format!("P{id}"))).collect();

    let effect = the_effect(&a_stereo_effect_with(parameters)).expect("an effect");

    assert_eq!(effect.descriptor.parameters.len(), 3000);
    assert_eq!(effect.value_handles.len(), 3000);
    assert_eq!(effect.value_handles[2999], 1000 + 2999);
    assert_eq!(effect.descriptor.parameters[2999].label, "P2999");
}

#[test]
fn a_plugin_with_more_values_than_an_effect_can_have_keeps_the_first_ones() {
    let parameters: Vec<ParameterDescription> = (0..MOST_VALUES_OF_AN_EFFECT as u32 + 40)
        .map(|id| a_parameter(id + 1000, &format!("P{id}")))
        .collect();
    let effect = the_effect(&a_stereo_effect_with(parameters)).expect("an effect");
    assert_eq!(effect.descriptor.parameters.len(), MOST_VALUES_OF_AN_EFFECT);
    assert_eq!(effect.value_handles.len(), MOST_VALUES_OF_AN_EFFECT);
    assert_eq!(effect.value_handles[0], 1000);
    assert_eq!(
        effect.value_handles[MOST_VALUES_OF_AN_EFFECT - 1],
        1000 + MOST_VALUES_OF_AN_EFFECT as u32 - 1
    );

    // (Parameters that are not values do not count towards the limit.)
    let mut mixed: Vec<ParameterDescription> = (0..100)
        .map(|id| {
            let mut hidden = a_parameter(id, "h");
            hidden.is_hidden = true;
            hidden
        })
        .collect();
    mixed.extend((0..MOST_VALUES_OF_AN_EFFECT as u32).map(|id| a_parameter(id + 500, "v")));
    let effect = the_effect(&a_stereo_effect_with(mixed)).expect("an effect");
    assert_eq!(effect.descriptor.parameters.len(), MOST_VALUES_OF_AN_EFFECT);
    assert_eq!(effect.value_handles[0], 500);
}

fn tagged(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn a_tail_of_nothing_is_read_as_half_a_second_and_a_longer_one_up_to_two_seconds() {
    let half_a_second = SAMPLE_RATE as usize / 2;
    assert_eq!(pre_roll_of_a_vst3_plugin(0, &tagged(&["Fx", "EQ"])), half_a_second);
    assert_eq!(pre_roll_of_a_vst3_plugin(0, &[]), half_a_second);
    assert_eq!(pre_roll_of_a_vst3_plugin(1000, &[]), 1000);
    assert_eq!(
        pre_roll_of_a_vst3_plugin(u32::MAX, &[]),
        MOST_PRE_ROLL_SAMPLES,
        "for ever is the most"
    );
    assert_eq!(pre_roll_of_a_vst3_plugin(10_000_000, &[]), MOST_PRE_ROLL_SAMPLES);
    let effect = the_effect(&with_facts(a_stereo_effect_with(Vec::new()), |facts| {
        facts.tail = 96_000
    }))
    .expect("an effect");
    assert_eq!(effect.pre_roll_samples, 96_000);
}

#[test]
fn a_reverb_or_a_delay_that_says_a_tail_of_nothing_is_given_two_seconds_and_what_a_plugin_says_is_kept() {
    // Measured with the real plugins (2026-11-03): every effect built with DPF says 0, Dragonfly Reverb's four, MVerb and the reverbs of Max-Gen among them, and they ring on for
    // between half a second and six seconds; they are all tagged Reverb. (Half a second cut a reverb dead while it was still 30 dB below the clip: the sound of "no tail".)
    let half_a_second = SAMPLE_RATE as usize / 2;
    for kinds in [
        &["Fx", "Reverb", "Stereo"][..],
        &["Fx", "Reverb"],
        &["Fx", "Delay"],
        &["Fx", "reverb"],
    ] {
        assert_eq!(
            pre_roll_of_a_vst3_plugin(0, &tagged(kinds)),
            MOST_PRE_ROLL_SAMPLES,
            "{kinds:?}"
        );
    }
    for kinds in [
        &["Fx", "EQ"][..],
        &["Fx", "Distortion", "Stereo"],
        &["Fx", "Spatial"],
        &["Fx", "Reverberation"],
        &["Instrument", "Synth"],
    ] {
        assert_eq!(pre_roll_of_a_vst3_plugin(0, &tagged(kinds)), half_a_second, "{kinds:?}");
    }
    // A reverb that says how long it rings is believed (Surge XT Effects says two seconds).
    assert_eq!(pre_roll_of_a_vst3_plugin(1000, &tagged(&["Fx", "Reverb"])), 1000);
    assert_eq!(pre_roll_of_a_vst3_plugin(96_000, &tagged(&["Fx", "Reverb"])), 96_000);

    // And it is what the effect of a scanned class has.
    let mut class = a_stereo_effect_with(Vec::new());
    class.class.sub_categories = tagged(&["Fx", "Reverb", "Stereo"]);
    assert_eq!(
        the_effect(&class).expect("an effect").pre_roll_samples,
        MOST_PRE_ROLL_SAMPLES
    );
    class.class.sub_categories = tagged(&["Fx", "Dynamics"]);
    assert_eq!(the_effect(&class).expect("an effect").pre_roll_samples, half_a_second);
}

#[test]
fn a_plugin_that_keeps_settings_only_its_window_can_change_says_so_when_it_has_hardly_any_values() {
    let mut class = a_stereo_effect_with(vec![a_parameter(1, "Only")]);
    class.has_state = true;
    let effect = the_effect(&class).expect("an effect");
    assert!(effect.descriptor.description.contains("only its own window can change"));
    class.has_state = false;
    assert!(!the_effect(&class).unwrap().descriptor.description.contains("window"));
    let mut many = a_stereo_effect_with(vec![a_parameter(1, "a"), a_parameter(2, "b"), a_parameter(3, "c")]);
    many.has_state = true;
    assert!(!the_effect(&many).unwrap().descriptor.description.contains("window"));
}

fn a_report(path: &str, classes: Vec<ScannedClass>) -> LibraryReport {
    LibraryReport {
        path: PathBuf::from(path),
        outcome: LibraryOutcome::ScannedVst3(ModuleScan {
            factory: FactoryDescription::default(),
            classes,
        }),
        from_cache: false,
    }
}

#[test]
fn the_plugins_found_in_a_module_are_effects_or_say_why_not_and_the_first_of_two_with_one_id_wins() {
    let good = a_stereo_effect_with(vec![a_parameter(1, "Gain")]);
    let mut broken = a_stereo_effect_with(Vec::new());
    broken.class.id = ClassId::from_numbers(1, 2, 3, 4);
    broken.class.name = "Broken".to_owned();
    broken.problem = Some("the first block failed".to_owned());
    let reports = [
        a_report("/first/amp.vst3", vec![good.clone(), broken]),
        a_report("/second/amp.vst3", vec![good]),
    ];

    let found = plugins_found_in(&reports);

    assert_eq!(found.effects.len(), 1);
    assert_eq!(
        found.effects[0].library,
        PathBuf::from("/first/amp.vst3"),
        "the folder that comes first"
    );
    assert_eq!(found.not_used.len(), 2);
    assert_eq!(found.not_used[0].name, "Broken");
    assert_eq!(
        found.not_used[0].label, "00000001000000020000000300000004",
        "the class id names it"
    );
    assert!(found.not_used[0].reason.contains("the first block failed"));
    assert!(found.not_used[1].reason.contains("same key"));
}

#[test]
fn a_parameter_whose_range_is_one_number_is_a_value_and_one_of_one_step_on_another_range_is_not_a_switch() {
    let mut constant = a_parameter(1, "Constant");
    constant.plain_minimum = 3.0;
    constant.plain_maximum = 3.0;
    constant.default_plain = 3.0;
    let value = the_value(constant);
    assert_eq!(value.range, Some((3.0, 3.0)));
    assert_eq!(value.default, PropertyValue::Number(3.0));

    let mut two_to_three = a_parameter(2, "Two");
    two_to_three.step_count = 1;
    two_to_three.plain_minimum = 2.0;
    two_to_three.plain_maximum = 3.0;
    two_to_three.default_plain = 2.0;
    assert_eq!(the_value(two_to_three).control, ParameterControl::Integer);

    let mut nought_to_two = a_parameter(3, "Wide");
    nought_to_two.step_count = 1;
    nought_to_two.plain_maximum = 2.0;
    assert_eq!(
        the_value(nought_to_two).control,
        ParameterControl::Integer,
        "not a flag: it has more than 0 and 1"
    );
}

#[test]
fn texts_for_a_list_whose_first_plain_value_is_not_a_whole_number_are_not_the_options_of_a_choice() {
    let mut half = a_parameter(1, "Half");
    half.step_count = 2;
    half.plain_minimum = 0.5;
    half.plain_maximum = 2.5;
    half.default_plain = 0.5;
    half.texts = vec!["A".to_owned(), "B".to_owned(), "C".to_owned()];
    assert_eq!(the_value(half).control, ParameterControl::Integer);
}

#[test]
fn the_note_about_settings_only_the_window_can_change_is_for_two_values_or_fewer() {
    let mut two = a_stereo_effect_with(vec![a_parameter(1, "a"), a_parameter(2, "b")]);
    two.has_state = true;
    assert!(
        the_effect(&two)
            .unwrap()
            .descriptor
            .description
            .contains("only its own window can change")
    );
}

#[test]
fn a_list_with_more_texts_than_steps_is_not_taken_for_a_choice() {
    let mut mode = a_parameter(1, "Mode");
    mode.step_count = 2;
    mode.plain_maximum = 2.0;
    mode.texts = vec!["A".to_owned(), "B".to_owned(), "C".to_owned(), "D".to_owned()];
    assert_eq!(
        the_value(mode).control,
        ParameterControl::Integer,
        "four texts for three steps"
    );
}
