// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tails that were measured by running an effect (docs/engine-phase-15-vst3-design.md, section 16): what a measurement is for, which effects want one, and that the plan
//! counts a measured tail in place of the guess of the scan, and only where the person has not said.

use std::sync::Arc;

use super::*;
use crate::composition::test_projects::{SOUND_FILE, media_clip, project_of};
use crate::project::tracks::Clip;
use crate::project::{EffectKind, PropertyValue, TailLength, set_installed_plugins};
use crate::test_support::{an_installed_sound_plugin, put_an_effect_on, the_installed_plugins_to_myself};

/// The effect of the one sound plugin that is installed (whose scan says `pre_roll_samples`), with these values, this state and this tail length.
fn the_effect_with(
    pre_roll_samples: usize,
    values: Vec<PropertyValue>,
    state: Option<&[u8]>,
    tail_length: TailLength,
) -> LayerEffect {
    let mut plugin = an_installed_sound_plugin("ladspa:a:reverb", "Maker", &[("decay", 1.0, Some((0.0, 10.0)))]);
    plugin.pre_roll_samples = pre_roll_samples;
    set_installed_plugins(vec![plugin]);
    LayerEffect {
        clock: Default::default(),
        instance: crate::project::EffectInstanceId::FIRST,
        kind: EffectKind::installed_plugin_kinds()[0],
        values,
        animation: Default::default(),
        tables: Vec::new(),
        state: state.map(Arc::from),
        tail_length,
        passes_notes_on: false,
        shared_by: None,
        driven: Default::default(),
    }
}

fn key_of(effect: &LayerEffect) -> u64 {
    key_of_the_tail_of(effect.kind, &effect.values, effect.state.as_ref()).expect("a plugin that is installed")
}

#[test]
fn a_measured_tail_is_counted_where_the_person_said_nothing_and_never_where_they_said_something() {
    let _alone = the_installed_plugins_to_myself();
    let decay = |seconds: f32| vec![PropertyValue::Number(seconds)];
    let automatic = the_effect_with(24_000, decay(10.0), None, TailLength::Automatic);
    assert_eq!(
        tail_samples_of(&automatic),
        24_000,
        "nothing measured: what the scan says"
    );

    note_the_measured_tail(key_of(&automatic), 480_000);

    assert_eq!(tail_samples_of(&automatic), 480_000, "ten seconds, as it was measured");
    let by_hand = |tail_length| the_effect_with(24_000, decay(10.0), None, tail_length);
    assert_eq!(tail_samples_of(&by_hand(TailLength::None)), 0);
    assert_eq!(
        tail_samples_of(&by_hand(TailLength::Milliseconds(250))),
        12_000,
        "a length that was said is not the length that was measured"
    );
    assert_eq!(
        tail_samples_of_a_chain(&[automatic.clone(), by_hand(TailLength::Milliseconds(100))]),
        480_000 + 4_800,
        "a chain adds the tails up, a measured one too"
    );
}

#[test]
fn a_measured_tail_is_the_tail_of_the_values_it_was_measured_at_and_of_no_others() {
    let _alone = the_installed_plugins_to_myself();
    let at = |decay: f32, state: Option<&[u8]>| {
        the_effect_with(24_000, vec![PropertyValue::Number(decay)], state, TailLength::Automatic)
    };
    let measured = at(10.0, None);
    note_the_measured_tail(key_of(&measured), 480_000);

    assert_eq!(tail_samples_of(&at(10.0, None)), 480_000, "the same values");
    assert_eq!(
        tail_samples_of(&at(1.3, None)),
        24_000,
        "another value of the decay: not measured yet, so the scan says"
    );
    assert_eq!(
        tail_samples_of(&at(10.0, Some(b"a preset"))),
        24_000,
        "what the plugin saved is part of what it is"
    );
    assert_ne!(
        key_of(&at(10.0, Some(b"a preset"))),
        key_of(&at(10.0, Some(b"another")))
    );
    assert_ne!(
        key_of(&at(10.0, None)),
        key_of(&at(10.000_001, None)),
        "a value that is a little different is another value"
    );
    // (Keyframes are not part of it: a measurement is made at the values the effect has when they are not animated.)
    let mut animated = at(10.0, None);
    animated.animation = crate::project::Animation::default();
    assert_eq!(key_of(&animated), key_of(&measured));
}

#[test]
fn a_measurement_is_made_for_the_plugin_that_is_installed_and_not_for_one_that_stands_in_a_project_by_the_same_values()
{
    let _alone = the_installed_plugins_to_myself();
    let effect = the_effect_with(24_000, vec![PropertyValue::Number(1.0)], None, TailLength::Automatic);
    let key = key_of(&effect);
    note_the_measured_tail(key, 480_000);

    // The same values on a plugin of another library: not the same sound.
    let mut other = an_installed_sound_plugin("ladspa:b:reverb", "Maker", &[("decay", 1.0, Some((0.0, 10.0)))]);
    other.pre_roll_samples = 24_000;
    set_installed_plugins(vec![other]);
    let other_kind = EffectKind::installed_plugin_kinds()[0];
    assert_ne!(
        key_of_the_tail_of(other_kind, &effect.values, None),
        Some(key),
        "the kind of effect is part of the key"
    );

    // An effect that is not a plugin, or a plugin that is not installed, has no key: nothing is measured for it.
    assert_eq!(key_of_the_tail_of(EffectKind::HUE_SHIFT, &[], None), None);
    set_installed_plugins(Vec::new());
    assert_eq!(key_of_the_tail_of(effect.kind, &effect.values, None), None);
}

#[test]
fn what_is_measured_is_counted_in_a_number_that_the_sound_of_the_preview_is_told_by() {
    let _alone = the_installed_plugins_to_myself();
    let effect = the_effect_with(24_000, vec![PropertyValue::Number(1.0)], None, TailLength::Automatic);
    let before = how_many_tails_have_been_measured();

    note_the_measured_tail(key_of(&effect), 480_000);
    let after_the_first = how_many_tails_have_been_measured();
    note_the_measured_tail(key_of(&effect), 480_000);
    let after_the_same_again = how_many_tails_have_been_measured();
    note_the_measured_tail(key_of(&effect), 96_000);
    let after_another_length = how_many_tails_have_been_measured();

    assert_ne!(
        after_the_first, before,
        "a tail that was not known is a change of the sound"
    );
    assert_eq!(
        after_the_same_again, after_the_first,
        "the same length again is not (the sound is not made again for it)"
    );
    assert_ne!(after_another_length, after_the_same_again);
    assert_eq!(tail_samples_of(&effect), 96_000);
}

/// A track of one sound clip with these effects on it, on the one sound plugin that is installed.
fn a_project_with_effects_on_its_clip(effects: &[(Vec<PropertyValue>, TailLength)]) -> Project {
    let mut project = project_of(vec![vec![(0, Clip::Audio(media_clip(SOUND_FILE, 0, 60, 1)))]]);
    let clip = project.tracks()[0].placement_at_index(0).unwrap().id;
    for (values, tail_length) in effects {
        let kind = the_effect_with(24_000, values.clone(), None, *tail_length).kind;
        let instance = put_an_effect_on(&mut project, AnimationTarget::Clip(clip), kind, values);
        project
            .set_effect_tail_length(Member::Clip(clip), instance, *tail_length)
            .expect("an effect that was just put on");
    }
    project
}

#[test]
fn the_effects_whose_tail_is_to_be_measured_are_the_sound_plugins_of_the_project_that_have_no_tail_said_and_none_measured_each_once()
 {
    let _alone = the_installed_plugins_to_myself();
    let decay = |seconds: f32| vec![PropertyValue::Number(seconds)];
    let project = a_project_with_effects_on_its_clip(&[
        (decay(10.0), TailLength::Automatic),
        (decay(10.0), TailLength::Automatic),
        (decay(2.0), TailLength::Automatic),
        (decay(3.0), TailLength::Milliseconds(500)),
        (decay(4.0), TailLength::None),
    ]);

    let wanted = the_tails_to_measure_in(&project);

    let mut seconds: Vec<_> = wanted.iter().map(|want| want.effect.values[0].clone()).collect();
    seconds.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    assert_eq!(
        seconds,
        [PropertyValue::Number(10.0), PropertyValue::Number(2.0)],
        "the two that are automatic, the same values once, and not the two whose tail was said"
    );
    for want in &wanted {
        assert_eq!(want.key, key_of(&want.effect));
        assert!(
            want.effect.animation.is_empty(),
            "it is measured as it is when it is not animated"
        );
    }

    note_the_measured_tail(wanted[0].key, 1_000);
    assert_eq!(
        the_tails_to_measure_in(&project).len(),
        1,
        "what has been measured is not wanted again"
    );
    note_the_measured_tail(wanted[1].key, 1_000);
    assert!(the_tails_to_measure_in(&project).is_empty());
}

#[test]
fn a_project_with_no_sound_plugin_wants_no_measurement() {
    let _alone = the_installed_plugins_to_myself();
    let project = project_of(vec![vec![(0, Clip::Audio(media_clip(SOUND_FILE, 0, 60, 1)))]]);
    assert!(the_tails_to_measure_in(&project).is_empty());
}

#[test]
fn the_lead_in_counts_a_measured_tail_as_the_plan_does() {
    let _alone = the_installed_plugins_to_myself();
    let values = vec![PropertyValue::Number(10.0)];
    let project = a_project_with_effects_on_its_clip(&[(values.clone(), TailLength::Automatic)]);
    let effect = the_effect_with(24_000, values, None, TailLength::Automatic);
    let lead_in = |project: &Project| plan_sound(project, TimelineFrameIndex::new(50), |_| true).lead_in_frames;
    // (At 30 frames a second, half a second is 15 frames, and the plan starts no more than ten seconds before.)
    assert_eq!(lead_in(&project), 15, "the scan says half a second");

    note_the_measured_tail(key_of(&effect), 4 * 48_000);

    assert_eq!(
        lead_in(&project),
        50,
        "four seconds are 120 frames, and there are 50 before the place"
    );
    note_the_measured_tail(key_of(&effect), 48_000);
    assert_eq!(lead_in(&project), 30, "a second");
}

#[test]
fn the_effect_that_is_measured_is_the_effect_the_plan_counts_for_a_clip_and_for_a_track_alike() {
    let _alone = the_installed_plugins_to_myself();
    let mut project = a_project_with_effects_on_its_clip(&[(vec![PropertyValue::Number(10.0)], TailLength::Automatic)]);
    let kind = EffectKind::installed_plugin_kinds()[0];
    let track = project.tracks()[0].id();
    put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track),
        kind,
        &[PropertyValue::Number(2.0)],
    );
    // (The effect of the clip has a state that its plugin saved: the key is made of it the same way by the plan, which has the bytes, and by what is wanted, which has the project's.)
    let clip = project.tracks()[0].placement_at_index(0).unwrap().id;
    let instance = project.effects_applied_to_every_clip()[&clip][0].effect.instance();
    project
        .set_effect_state(Member::Clip(clip), instance, Some(Arc::from(&b"a preset"[..])))
        .unwrap();
    let the_plan_counts = |project: &Project| {
        let plan = plan_sound(project, TimelineFrameIndex::new(0), |_| true);
        let strip = &plan.strips[0];
        let SoundItem::Clip(clip) = &strip.items[0] else {
            panic!("a clip");
        };
        (
            tail_samples_of_a_chain(&clip.effects),
            tail_samples_of_a_chain(&strip.buses[0].effects),
        )
    };
    assert_eq!(
        the_plan_counts(&project),
        (24_000, 24_000),
        "the scan, before anything is measured"
    );

    let wanted = the_tails_to_measure_in(&project);
    assert_eq!(wanted.len(), 2, "the two values are two effects");
    for want in &wanted {
        let seconds = match want.effect.values[0] {
            PropertyValue::Number(seconds) => seconds,
            _ => unreachable!("a number"),
        };
        note_the_measured_tail(want.key, (seconds * 48_000.0) as u64);
    }

    assert_eq!(
        the_plan_counts(&project),
        (480_000, 96_000),
        "ten seconds for the effect of the clip and two for the one of the track, each by its own values"
    );
    assert!(the_tails_to_measure_in(&project).is_empty());
}

#[test]
fn a_synthesizer_is_not_measured_its_notes_end_when_they_end() {
    let _alone = the_installed_plugins_to_myself();
    let mut project = project_of(vec![vec![(0, Clip::Audio(media_clip(SOUND_FILE, 0, 60, 1)))]]);
    set_installed_plugins(vec![crate::test_support::an_installed_synthesizer("ladspa:a:synth")]);
    let kind = EffectKind::installed_plugin_kinds()[0];
    assert!(kind.makes_sound_from_notes());
    let clip = project.tracks()[0].placement_at_index(0).unwrap().id;
    put_an_effect_on(&mut project, AnimationTarget::Clip(clip), kind, &[]);
    assert!(the_tails_to_measure_in(&project).is_empty());
}
