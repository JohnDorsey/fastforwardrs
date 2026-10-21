// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Plugins as effects of the sound, with the real test plugins (`crates/ladspa-test-plugins`, loaded in this process: they do nothing but arithmetic): each channel layout, the
//! values and their keyframes at the frames they say, a plugin that remembers, the sound that is not a number, a plugin that is too slow, and what the host does about folders
//! that nobody has said anything about.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::*;
use crate::composition::LayerEffect;
use crate::media::export::{LoudnessToExport, SoundClipToExport};
use crate::plugin_trust::FolderStanding;
use crate::plugin_trust::folder_of;
use crate::project::{
    Animation, EffectInstanceId, EffectKind, FrameRate, InstalledPlugin, Keyframe, Keyframes, PropertyId,
    PropertyValue, SpeedFactor, set_installed_plugins,
};
use crate::sound_engine::{BlockProcessor, ClipRequest, Insertions, Notes, PluginInsertions, PluginRunner};
use crate::test_support::{the_installed_plugins_to_myself, the_plugins_of_the_test_library};

fn thirty() -> FrameRate {
    FrameRate::new(30, 1).unwrap()
}

/// The plugins of the test library, as the scan makes them effects, by their labels.
fn the_test_plugins() -> Vec<InstalledPlugin> {
    the_plugins_of_the_test_library()
}

/// The kind of the plugin `label` of the test library, with the list of what is installed made of the test plugins.
fn the_kind_of(label: &str) -> EffectKind {
    set_installed_plugins(the_test_plugins());
    EffectKind::installed_plugin_kinds()
        .into_iter()
        .find(|kind| kind.key().ends_with(&format!(":{label}")))
        .unwrap_or_else(|| panic!("no plugin called {label}"))
}

fn the_effect(kind: EffectKind, values: &[f32]) -> LayerEffect {
    LayerEffect {
        instance: EffectInstanceId::FIRST,
        kind,
        values: values.iter().map(|value| PropertyValue::Number(*value)).collect(),
        animation: Animation::default(),
        tables: Vec::new(),
    }
}

fn a_clip_at(first_timeline_frame: u64, effects: Vec<LayerEffect>) -> SoundClipToExport {
    SoundClipToExport {
        source_input: 0,
        first_source_frame: 0,
        end_source_frame: 1000,
        speed_factor: SpeedFactor::new(1).unwrap(),
        timeline_frame_count: 1000,
        loudness: LoudnessToExport::UNCHANGED,
        first_timeline_frame,
        frames_of_the_clip_before: 0,
        effects,
    }
}

/// A host that lets everything be run, and the processor it makes for `effect` on a clip that begins at the frame `first_frame`.
fn processor_of(effect: &LayerEffect, first_frame: u64) -> (Box<dyn BlockProcessor>, Notes) {
    let host = PluginHost::new();
    host.allow_everything();
    let notes = Notes::default();
    let clip = a_clip_at(first_frame, vec![effect.clone()]);
    let processor = host
        .processor_for(effect, &clip, thirty(), &notes, false)
        .expect("the plugin is run");
    (processor, notes)
}

fn ramp(count: usize, offset: f32) -> Vec<f32> {
    (0..count).map(|moment| offset + moment as f32 * 0.001).collect()
}

// --- the arithmetic ---

#[test]
fn a_sample_is_in_the_frame_that_its_time_is_in_counted_from_the_frame_the_clip_begins_at() {
    assert_eq!(frame_of_a_sample(thirty(), 10, 0), 10);
    assert_eq!(
        frame_of_a_sample(thirty(), 10, 1599),
        10,
        "a frame is 1600 samples at 30 a second"
    );
    assert_eq!(frame_of_a_sample(thirty(), 10, 1600), 11);
    assert_eq!(frame_of_a_sample(thirty(), 0, 48_000), 30);
    let ntsc = FrameRate::new(30_000, 1001).unwrap();
    assert_eq!(
        frame_of_a_sample(ntsc, 0, 1601),
        0,
        "a frame is 1601.6 samples at 29.97 a second"
    );
    assert_eq!(frame_of_a_sample(ntsc, 0, 1602), 1);
    assert_eq!(
        frame_of_a_sample(thirty(), 0, u64::MAX / 4),
        u64::MAX / 4 / 1600,
        "no overflow"
    );
}

#[test]
fn the_layouts_that_can_be_effects_are_four_and_each_says_how_many_instances_it_needs() {
    assert_eq!(Layout::of(1, 1), Some(Layout::EachChannelOnItsOwn));
    assert_eq!(Layout::of(2, 2), Some(Layout::Stereo));
    assert_eq!(Layout::of(1, 2), Some(Layout::MonoToStereo));
    assert_eq!(Layout::of(2, 1), Some(Layout::StereoToMono));
    for (inputs, outputs) in [(0, 1), (1, 0), (3, 3), (2, 3)] {
        assert_eq!(Layout::of(inputs, outputs), None);
    }
    assert_eq!(Layout::EachChannelOnItsOwn.instances(), 2);
    assert_eq!(Layout::Stereo.instances(), 1);
    assert_eq!(Layout::MonoToStereo.instances(), 1);
    assert_eq!(Layout::StereoToMono.instances(), 1);
}

#[test]
fn a_plugin_that_is_too_slow_for_two_seconds_of_sound_in_a_row_is_to_be_left_out_and_a_good_block_starts_the_count_again()
 {
    let mut guard = SlownessGuard::default();
    let block = 512.0 / 48_000.0;
    let blocks_in_two_seconds = (SECONDS_TOO_SLOW / block).ceil() as usize;
    for number in 0..blocks_in_two_seconds - 1 {
        assert!(!guard.is_it_too_slow(block, block * 2.0), "block {number}: not yet");
    }
    assert!(
        guard.is_it_too_slow(block, block * 2.0),
        "two seconds of sound that took too long"
    );

    let mut guard = SlownessGuard::default();
    for _ in 0..blocks_in_two_seconds - 1 {
        assert!(!guard.is_it_too_slow(block, block * 2.0));
    }
    assert!(
        !guard.is_it_too_slow(block, block * 0.5),
        "one that kept up: begin again"
    );
    for _ in 0..blocks_in_two_seconds - 1 {
        assert!(
            !guard.is_it_too_slow(block, block * 2.0),
            "and it takes two seconds all over again"
        );
    }
    // Taking exactly as long as the block lasts is keeping up.
    let mut guard = SlownessGuard::default();
    for _ in 0..blocks_in_two_seconds * 2 {
        assert!(!guard.is_it_too_slow(block, block));
    }
}

// --- the layouts, with the plugins that have them ---

#[test]
fn a_plugin_of_one_input_and_one_output_is_run_on_each_channel_on_its_own_with_the_values_of_its_controls() {
    let _alone = the_installed_plugins_to_myself();
    let effect = the_effect(the_kind_of("ffrs_test_gain"), &[0.5]);
    let (mut processor, notes) = processor_of(&effect, 0);
    let (mut left, mut right) = (ramp(512, 1.0), ramp(512, -3.0));
    let (expected_left, expected_right): (Vec<f32>, Vec<f32>) = (
        left.iter().map(|sample| sample * 0.5).collect(),
        right.iter().map(|sample| sample * 0.5).collect(),
    );
    processor.process(&mut left, &mut right);
    assert_eq!((left, right), (expected_left, expected_right));
    assert!(notes.lock().unwrap().is_empty());
}

#[test]
fn a_plugin_of_two_inputs_and_two_outputs_is_given_both_channels_and_gives_both() {
    let _alone = the_installed_plugins_to_myself();
    let effect = the_effect(the_kind_of("ffrs_test_swap"), &[]);
    let (mut processor, _) = processor_of(&effect, 0);
    let (mut left, mut right) = (ramp(300, 1.0), ramp(300, -1.0));
    let (was_left, was_right) = (left.clone(), right.clone());
    processor.process(&mut left, &mut right);
    assert_eq!((left, right), (was_right, was_left), "swapped");
}

#[test]
fn a_plugin_of_one_input_and_two_outputs_is_given_the_mix_and_a_plugin_of_two_inputs_and_one_output_gives_both_channels_its_output()
 {
    let _alone = the_installed_plugins_to_myself();
    let (mut left, mut right) = (ramp(200, 1.0), ramp(200, 3.0));
    let (was_left, was_right) = (left.clone(), right.clone());

    let (mut mono_to_both, _) = processor_of(&the_effect(the_kind_of("ffrs_test_mono_to_both"), &[]), 0);
    mono_to_both.process(&mut left, &mut right);
    let mix: Vec<f32> = was_left.iter().zip(&was_right).map(|(l, r)| (l + r) * 0.5).collect();
    assert_eq!(left, mix, "the left is the mix");
    assert_eq!(right, mix, "and so is the right");

    let (mut mix_down, _) = processor_of(&the_effect(the_kind_of("ffrs_test_mix_down"), &[]), 0);
    let (mut left, mut right) = (was_left.clone(), was_right.clone());
    mix_down.process(&mut left, &mut right);
    let sum: Vec<f32> = was_left.iter().zip(&was_right).map(|(l, r)| l + r).collect();
    assert_eq!(left, sum);
    assert_eq!(right, sum, "both channels have the one output");
}

#[test]
fn a_plugin_that_remembers_gives_the_same_sound_in_blocks_of_any_size_and_each_channel_has_a_memory_of_its_own() {
    let _alone = the_installed_plugins_to_myself();
    let effect = the_effect(the_kind_of("ffrs_test_one_pole"), &[1000.0]);
    let (left_in, right_in) = (ramp(1024, 1.0), vec![0.25_f32; 1024]);

    let (mut whole, _) = processor_of(&effect, 0);
    let (mut whole_left, mut whole_right) = (left_in.clone(), right_in.clone());
    whole.process(&mut whole_left, &mut whole_right);

    let (mut pieces, _) = processor_of(&effect, 0);
    let (mut pieces_left, mut pieces_right) = (left_in.clone(), right_in.clone());
    for start in (0..1024).step_by(100) {
        let end = (start + 100).min(1024);
        pieces.process(&mut pieces_left[start..end], &mut pieces_right[start..end]);
    }
    assert_eq!(
        pieces_left, whole_left,
        "blocks of 100 are one block of 1024, on the left"
    );
    assert_eq!(pieces_right, whole_right, "and on the right");
    assert_ne!(whole_left, left_in, "(it does filter)");
    // A step on the right alone: the left is not touched by the right's memory.
    let (mut each, _) = processor_of(&effect, 0);
    let (mut left, mut right) = (vec![0.0_f32; 64], vec![1.0_f32; 64]);
    each.process(&mut left, &mut right);
    assert!(left.iter().all(|sample| *sample == 0.0), "{left:?}");
    assert!(right[0] > 0.0 && right[63] > right[0]);
}

#[test]
fn a_reset_makes_a_plugin_that_remembers_forget() {
    let _alone = the_installed_plugins_to_myself();
    let effect = the_effect(the_kind_of("ffrs_test_one_pole"), &[500.0]);
    let (mut processor, _) = processor_of(&effect, 0);
    let (mut first_left, mut first_right) = (vec![1.0_f32; 128], vec![1.0_f32; 128]);
    processor.process(&mut first_left, &mut first_right);
    let (mut second_left, mut second_right) = (vec![1.0_f32; 128], vec![1.0_f32; 128]);
    processor.process(&mut second_left, &mut second_right);
    assert_ne!(first_left, second_left, "it remembered");
    processor.reset();
    let (mut third_left, mut third_right) = (vec![1.0_f32; 128], vec![1.0_f32; 128]);
    processor.process(&mut third_left, &mut third_right);
    assert_eq!((third_left, third_right), (first_left, first_right), "and forgot");
}

// --- the values ---

#[test]
fn a_value_is_kept_to_the_range_the_plugin_says_though_a_person_can_type_one_past_it() {
    let _alone = the_installed_plugins_to_myself();
    let kind = the_kind_of("ffrs_test_gain");
    let (mut past_the_top, _) = processor_of(&the_effect(kind, &[10.0]), 0);
    let (mut left, mut right) = (vec![1.0_f32; 8], vec![1.0_f32; 8]);
    past_the_top.process(&mut left, &mut right);
    assert_eq!(left, vec![4.0; 8], "the gain goes from 0 to 4: 10 is 4");
    let (mut below, _) = processor_of(&the_effect(kind, &[-3.0]), 0);
    let (mut left, mut right) = (vec![1.0_f32; 8], vec![1.0_f32; 8]);
    below.process(&mut left, &mut right);
    assert_eq!(left, vec![0.0; 8]);
    let (mut not_a_number, _) = processor_of(&the_effect(kind, &[f32::NAN]), 0);
    let (mut left, mut right) = (vec![1.0_f32; 8], vec![1.0_f32; 8]);
    not_a_number.process(&mut left, &mut right);
    assert!(
        left.iter().all(|sample| sample.is_finite()),
        "a value that is not a number does not reach the speakers: {left:?}"
    );
}

#[test]
fn a_flag_is_one_or_zero_and_a_whole_number_is_given_as_it_is() {
    let _alone = the_installed_plugins_to_myself();
    let kind = the_kind_of("ffrs_test_switches");
    let mut effect = the_effect(kind, &[]);
    effect.values = vec![PropertyValue::Flag(true), PropertyValue::Number(3.0)];
    let (mut processor, _) = processor_of(&effect, 0);
    let (mut left, mut right) = (vec![0.5_f32; 4], vec![0.5_f32; 4]);
    processor.process(&mut left, &mut right);
    assert_eq!(left, vec![-1.5; 4], "inverted and times three");
    effect.values = vec![PropertyValue::Flag(false), PropertyValue::Number(3.0)];
    let (mut processor, _) = processor_of(&effect, 0);
    let (mut left, mut right) = (vec![0.5_f32; 4], vec![0.5_f32; 4]);
    processor.process(&mut left, &mut right);
    assert_eq!(left, vec![1.5; 4]);
}

#[test]
fn a_value_that_is_animated_is_what_the_keyframes_say_at_the_frame_each_stretch_of_32_samples_is_in() {
    let _alone = the_installed_plugins_to_myself();
    let kind = the_kind_of("ffrs_test_gain");
    let mut effect = the_effect(kind, &[1.0]);
    let gain = PropertyId::effect_value(kind, EffectInstanceId::FIRST, 0);
    let keyframe = |frame, value| Keyframe {
        frame,
        value: PropertyValue::Number(value),
    };
    // The gain goes from 0 at the frame 10 to 4 at the frame 40, on a clip that begins at the frame 10: it rises through 30 frames, which are 48000 samples.
    effect.animation =
        Animation::from_keyframes([(gain, Keyframes::from_unordered([keyframe(10, 0.0), keyframe(40, 4.0)]))]);
    let (mut processor, _) = processor_of(&effect, 10);

    let samples = 48_000;
    let (mut left, mut right) = (vec![1.0_f32; samples], vec![1.0_f32; samples]);
    // (In blocks of 512, as the preview asks.)
    for start in (0..samples).step_by(512) {
        let end = (start + 512).min(samples);
        processor.process(&mut left[start..end], &mut right[start..end]);
    }
    for index in [0, 31, 32, 1599, 1600, 1601, 24_000, 40_000, 47_999] {
        let stretch_begins_at = (index / 32) * 32;
        let frame = 10 + stretch_begins_at as u64 * 30 / 48_000;
        let expected = ((frame - 10) as f32 / 30.0 * 4.0).clamp(0.0, 4.0);
        assert!(
            (left[index] - expected).abs() < 1e-4,
            "sample {index}: the gain is {} and the keyframes say {expected} at frame {frame}",
            left[index]
        );
        assert_eq!(left[index], right[index], "both channels have the same values");
    }
    assert_eq!(left[0], 0.0);
    assert!(left[47_999] > 3.8, "{}", left[47_999]);
}

#[test]
fn a_value_that_is_not_animated_is_given_once_and_the_plugin_keeps_it() {
    let _alone = the_installed_plugins_to_myself();
    let (mut processor, _) = processor_of(&the_effect(the_kind_of("ffrs_test_gain"), &[2.0]), 0);
    for _ in 0..5 {
        let (mut left, mut right) = (vec![1.0_f32; 512], vec![1.0_f32; 512]);
        processor.process(&mut left, &mut right);
        assert!(left.iter().all(|sample| *sample == 2.0));
    }
}

// --- what a plugin gives ---

#[test]
fn sound_that_is_not_a_number_is_silence() {
    let _alone = the_installed_plugins_to_myself();
    let (mut processor, _) = processor_of(&the_effect(the_kind_of("ffrs_test_gain"), &[1.0]), 0);
    let mut left = vec![1.0, f32::NAN, f32::INFINITY, -f32::INFINITY, 0.5];
    let mut right = vec![f32::NAN, 1.0, 1.0, 1.0, 1.0];
    processor.process(&mut left, &mut right);
    assert_eq!(left, vec![1.0, 0.0, 0.0, 0.0, 0.5]);
    assert_eq!(right, vec![0.0, 1.0, 1.0, 1.0, 1.0]);
}

#[test]
fn a_block_of_nothing_changes_nothing_and_a_block_of_one_sample_works() {
    let _alone = the_installed_plugins_to_myself();
    let (mut processor, _) = processor_of(&the_effect(the_kind_of("ffrs_test_gain"), &[3.0]), 0);
    processor.process(&mut [], &mut []);
    let (mut left, mut right) = (vec![1.0_f32], vec![2.0_f32]);
    processor.process(&mut left, &mut right);
    assert_eq!((left, right), (vec![3.0], vec![6.0]));
}

// --- the host: who may be run ---

fn the_gain_effect() -> (LayerEffect, &'static InstalledPlugin) {
    let kind = the_kind_of("ffrs_test_gain");
    (the_effect(kind, &[0.5]), kind.installed_plugin().unwrap())
}

fn the_folder_of(plugin: &InstalledPlugin) -> PathBuf {
    folder_of(&plugin.library)
}

fn ask(host: &PluginHost, effect: &LayerEffect, notes: &Notes) -> Option<Box<dyn BlockProcessor>> {
    host.processor_for(effect, &a_clip_at(0, vec![effect.clone()]), thirty(), notes, true)
}

#[test]
fn a_folder_that_nobody_has_said_anything_about_is_wanted_once_and_the_sound_goes_through_unchanged() {
    let _alone = the_installed_plugins_to_myself();
    let (effect, plugin) = the_gain_effect();
    let host = PluginHost::new();
    let notes = Notes::default();
    assert!(
        ask(&host, &effect, &notes).is_none(),
        "not run: nobody has said it may be"
    );
    assert!(ask(&host, &effect, &notes).is_none());
    assert!(host.loaded_libraries().is_empty(), "and not loaded");
    let wanted = host.take_wanted();
    assert_eq!(
        wanted,
        [Wanted {
            folder: the_folder_of(plugin),
            libraries: vec![plugin.library.file_name().unwrap().to_string_lossy().into_owned()],
            plugins: vec![plugin.descriptor.name.to_string()]
        }],
        "the folder, once, with the library and the plugin that are wanted"
    );
    assert!(host.take_wanted().is_empty(), "and it is given once");
    assert!(notes.lock().unwrap().is_empty(), "(asking a person is not a note)");
}

#[test]
fn the_plugins_that_are_wanted_from_a_folder_are_listed_once_each() {
    let _alone = the_installed_plugins_to_myself();
    let swap = the_effect(the_kind_of("ffrs_test_swap"), &[]);
    let gain = the_effect(the_kind_of("ffrs_test_gain"), &[1.0]);
    let host = PluginHost::new();
    let notes = Notes::default();
    for effect in [&gain, &swap, &gain, &swap] {
        let _ = ask(&host, effect, &notes);
    }
    let wanted = host.take_wanted();
    assert_eq!(wanted.len(), 1, "one folder");
    assert_eq!(wanted[0].plugins.len(), 2, "{:?}", wanted[0].plugins);
}

#[test]
fn a_folder_that_was_approved_is_run_and_the_library_is_loaded_once_and_the_generation_changed() {
    let _alone = the_installed_plugins_to_myself();
    let (effect, plugin) = the_gain_effect();
    let host = PluginHost::new();
    let before = host.generation();
    host.approve_folder(&the_folder_of(plugin));
    assert_ne!(
        host.generation(),
        before,
        "what may be run changed: the sound is made again"
    );
    let notes = Notes::default();
    let (mut first, mut second) = (
        ask(&host, &effect, &notes).unwrap(),
        ask(&host, &effect, &notes).unwrap(),
    );
    assert_eq!(
        host.loaded_libraries().len(),
        1,
        "one library for two plugins made of it"
    );
    let (mut left, mut right) = (vec![1.0_f32; 4], vec![1.0_f32; 4]);
    first.process(&mut left, &mut right);
    second.process(&mut left, &mut right);
    assert_eq!(left, vec![0.25; 4], "both are run");
    assert!(host.take_wanted().is_empty());
}

#[test]
fn a_folder_that_was_declined_is_not_run_and_is_not_asked_about_again_and_what_was_wanted_is_forgotten() {
    let _alone = the_installed_plugins_to_myself();
    let (effect, plugin) = the_gain_effect();
    let host = PluginHost::new();
    let notes = Notes::default();
    let _ = ask(&host, &effect, &notes);
    host.decline_folder(&the_folder_of(plugin));
    assert!(host.take_wanted().is_empty(), "an answer is an answer");
    assert!(ask(&host, &effect, &notes).is_none());
    assert!(host.take_wanted().is_empty(), "and it is not asked about again");
    host.forget_what_was_declined();
    assert!(ask(&host, &effect, &notes).is_none());
    assert_eq!(host.take_wanted().len(), 1, "another project: asked again");
}

#[test]
fn every_folder_is_run_when_everything_is_allowed_and_the_approved_folders_can_be_set_all_at_once() {
    let _alone = the_installed_plugins_to_myself();
    let (effect, plugin) = the_gain_effect();
    let notes = Notes::default();
    let host = PluginHost::new();
    host.set_approved_folders([the_folder_of(plugin)]);
    assert!(ask(&host, &effect, &notes).is_some());
    let generation = host.generation();
    host.set_approved_folders([the_folder_of(plugin)]);
    assert_eq!(host.generation(), generation, "the same: nothing changed");
    host.set_approved_folders(Vec::<PathBuf>::new());
    assert!(ask(&host, &effect, &notes).is_none(), "replaced");
    host.allow_everything();
    assert!(ask(&host, &effect, &notes).is_some());
}

#[test]
fn a_plugin_that_is_not_installed_is_not_run_and_a_library_that_cannot_be_loaded_is_said_once() {
    let _alone = the_installed_plugins_to_myself();
    let (effect, _) = the_gain_effect();
    let host = PluginHost::new();
    host.allow_everything();
    let notes = Notes::default();
    set_installed_plugins(Vec::new());
    assert!(ask(&host, &effect, &notes).is_none(), "the plugin is gone");
    assert!(
        notes.lock().unwrap().is_empty(),
        "(a plugin that is not installed is no news)"
    );

    let mut broken = the_test_plugins().remove(0);
    broken.library = std::env::temp_dir().join("a_library_that_is_not_there.dll");
    set_installed_plugins(vec![broken]);
    let kind = EffectKind::installed_plugin_kinds()[0];
    let effect = the_effect(kind, &[1.0]);
    assert!(ask(&host, &effect, &notes).is_none());
    assert!(ask(&host, &effect, &notes).is_none());
    let notes = notes.lock().unwrap().clone();
    assert_eq!(notes.len(), 1, "said once: {notes:?}");
    assert!(
        notes[0].contains("could not be loaded") && notes[0].contains("unchanged"),
        "{notes:?}"
    );
}

#[test]
fn a_library_that_is_not_what_it_was_when_it_was_scanned_is_not_run_and_says_to_scan_again() {
    let _alone = the_installed_plugins_to_myself();
    let mut plugins = the_test_plugins();
    // The gain is installed as a plugin that the library has at another place, with a label that is another plugin's.
    let mut impostor = plugins.remove(0);
    // (The stereo swap.)
    let crate::project::PluginAddress::Ladspa { index, .. } = &mut impostor.address else {
        panic!("the test plugins are LADSPA ones");
    };
    *index = 2;
    set_installed_plugins(vec![impostor]);
    let kind = EffectKind::installed_plugin_kinds()[0];
    let effect = the_effect(kind, &[0.5]);
    let host = PluginHost::new();
    host.allow_everything();
    let notes = Notes::default();
    assert!(ask(&host, &effect, &notes).is_none());
    let said = notes.lock().unwrap().clone();
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("scan again"), "{said:?}");
}

#[test]
fn a_breadcrumb_is_written_before_a_library_is_loaded_and_taken_away_when_the_editor_ends_well() {
    let _alone = the_installed_plugins_to_myself();
    let (effect, plugin) = the_gain_effect();
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("in_use.json");
    let host = PluginHost::new();
    host.allow_everything();
    host.keep_a_breadcrumb(crate::plugin_scan::Breadcrumb::at(file.clone()));
    assert!(!file.exists());
    let _ = ask(&host, &effect, &Notes::default()).unwrap();
    let written = std::fs::read_to_string(&file).expect("the breadcrumb says which library is in use");
    let file_name = plugin.library.file_name().unwrap().to_string_lossy().into_owned();
    assert!(written.contains(&file_name), "{written}");
    host.ended_well();
    assert!(!file.exists(), "it ended well");
}

// --- the graph with plugins in it ---

/// A runner that doubles the sound of any plugin effect it is asked for, and counts the questions.
struct Doubler {
    asked: Mutex<usize>,
}

struct Double;

impl BlockProcessor for Double {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for sample in left.iter_mut().chain(right.iter_mut()) {
            *sample *= 2.0;
        }
    }

    fn reset(&mut self) {}
}

impl PluginRunner for Doubler {
    fn processor_for(
        &self,
        _: &LayerEffect,
        _: &SoundClipToExport,
        _: FrameRate,
        _: &Notes,
        _: bool,
    ) -> Option<Box<dyn BlockProcessor>> {
        *self.asked.lock().unwrap() += 1;
        Some(Box::new(Double))
    }
}

#[test]
fn the_insertions_of_a_graph_ask_the_runner_for_the_plugin_effects_of_a_clip_and_only_for_those() {
    let _alone = the_installed_plugins_to_myself();
    let installed = the_effect(the_kind_of("ffrs_test_gain"), &[1.0]);
    let swap = the_effect(the_kind_of("ffrs_test_swap"), &[]);
    // Only the gain is installed now: the swap is a plugin effect that nothing stands in for.
    set_installed_plugins(the_test_plugins().into_iter().take(1).collect());
    assert!(installed.kind.installed_plugin().is_some());
    assert!(
        swap.kind.installed_plugin().is_none(),
        "the swap is not in the list now"
    );

    let runner = Arc::new(Doubler { asked: Mutex::new(0) });
    let mut insertions = PluginInsertions::new(runner.clone(), thirty(), Notes::default(), true);
    let clip = a_clip_at(0, vec![installed, swap]);
    let request = ClipRequest {
        strip: 0,
        item: 0,
        clip: &clip,
    };
    let processors = insertions.for_clip(&request);
    assert_eq!(processors.len(), 1, "one of the two is a plugin that is installed");
    assert_eq!(*runner.asked.lock().unwrap(), 1, "and the runner was asked for it only");
    assert!(insertions.for_strip(0).is_empty());
}

// --- the whole graph, with the real test plugins and sound made in memory ---

use crate::media::export::{SoundGraphSpecification, SoundItemToExport, SoundStripToExport};
use crate::project::SpedUpClipSound;
use crate::sound_engine::{ClipFeed, ClipFeeds, SoundGraph, pre_roll_samples_of, sample_at_frame};

/// The sound of a source file that has, at sample number `n` of it, `1 + n / 100000` on the left and the same with the sign changed on the right: a different number at every
/// sample, so that a sample out of place cannot be taken for the right one.
struct SourceRamp {
    next: u64,
}

impl ClipFeed for SourceRamp {
    fn read(&mut self, left: &mut [f32], right: &mut [f32]) -> usize {
        for place in 0..left.len() {
            left[place] = 1.0 + (self.next + place as u64) as f32 / 100_000.0;
            right[place] = -left[place];
        }
        self.next += left.len() as u64;
        left.len()
    }
}

struct SourceRamps {
    frame_rate: FrameRate,
}

impl ClipFeeds for SourceRamps {
    fn open(&mut self, request: &ClipRequest<'_>) -> Box<dyn ClipFeed> {
        // (A feed has the sound from before the clip that the plugins ask for, as the feed of ffmpeg does.)
        Box::new(SourceRamp {
            next: sample_at_frame(self.frame_rate, request.clip.first_source_frame)
                - pre_roll_samples_of(self.frame_rate, request.clip) as u64,
        })
    }
}

/// The sound of a strip of the one clip `clip`, made by the graph in blocks of `block`, with `runner` making the plugins.
fn sound_of(
    clip: SoundClipToExport,
    runner: Arc<dyn PluginRunner>,
    is_for_the_preview: bool,
    block: usize,
) -> (Vec<f32>, Vec<f32>, Notes) {
    let notes = Notes::default();
    let specification = SoundGraphSpecification {
        frame_rate: thirty(),
        total_frame_count: clip.timeline_frame_count,
        strips: vec![SoundStripToExport {
            items: vec![SoundItemToExport::Clip(clip)],
        }],
        sped_up_clip_sound: SpedUpClipSound::MaintainPitch,
    };
    let mut graph = SoundGraph::new(
        specification,
        Box::new(SourceRamps { frame_rate: thirty() }),
        Box::new(PluginInsertions::new(
            runner,
            thirty(),
            Arc::clone(&notes),
            is_for_the_preview,
        )),
    );
    let (left, right) = graph.render_to_the_end(block);
    (left, right, notes)
}

fn a_host_that_runs_everything() -> Arc<PluginHost> {
    let host = Arc::new(PluginHost::new());
    host.allow_everything();
    host
}

fn the_clip(
    first_source_frame: u64,
    frame_count: u64,
    first_timeline_frame: u64,
    effects: Vec<LayerEffect>,
) -> SoundClipToExport {
    SoundClipToExport {
        source_input: 0,
        first_source_frame,
        end_source_frame: first_source_frame + frame_count,
        speed_factor: SpeedFactor::new(1).unwrap(),
        timeline_frame_count: frame_count,
        loudness: LoudnessToExport::UNCHANGED,
        first_timeline_frame,
        frames_of_the_clip_before: 0,
        effects,
    }
}

fn an_animated_gain(first: (u64, f32), last: (u64, f32)) -> LayerEffect {
    let kind = the_kind_of("ffrs_test_gain");
    let mut effect = the_effect(kind, &[1.0]);
    let gain = PropertyId::effect_value(kind, EffectInstanceId::FIRST, 0);
    let keyframe = |(frame, value): (u64, f32)| Keyframe {
        frame,
        value: PropertyValue::Number(value),
    };
    effect.animation =
        Animation::from_keyframes([(gain, Keyframes::from_unordered([keyframe(first), keyframe(last)]))]);
    effect
}

#[test]
fn a_gain_plugin_on_a_clip_in_the_graph_gives_the_arithmetic_exactly_for_blocks_of_any_size() {
    let _alone = the_installed_plugins_to_myself();
    let gain = the_effect(the_kind_of("ffrs_test_gain"), &[0.5]);
    let (plain_left, plain_right, _) = sound_of(
        the_clip(0, 45, 0, Vec::new()),
        a_host_that_runs_everything(),
        false,
        512,
    );
    assert_eq!(plain_left.len(), 45 * 1600);
    let half = |plain: &[f32]| plain.iter().map(|sample| sample * 0.5).collect::<Vec<f32>>();
    for block in [512, 100, 4096] {
        let (left, right, notes) = sound_of(
            the_clip(0, 45, 0, vec![gain.clone()]),
            a_host_that_runs_everything(),
            false,
            block,
        );
        assert_eq!(left, half(&plain_left), "blocks of {block}, left");
        assert_eq!(right, half(&plain_right), "blocks of {block}, right");
        assert!(notes.lock().unwrap().is_empty());
    }
    // And blocks of one sample, for a sound of a frame.
    let (plain_left, _, _) = sound_of(the_clip(0, 1, 0, Vec::new()), a_host_that_runs_everything(), false, 512);
    let (left, _, _) = sound_of(the_clip(0, 1, 0, vec![gain]), a_host_that_runs_everything(), false, 1);
    assert_eq!(left, half(&plain_left), "blocks of one sample");
}

#[test]
fn the_effects_of_a_clip_are_done_before_its_volume() {
    let _alone = the_installed_plugins_to_myself();
    let gain = the_effect(the_kind_of("ffrs_test_gain"), &[0.5]);
    let mut clip = the_clip(0, 3, 0, vec![gain]);
    clip.loudness = LoudnessToExport::Constant(0.25);
    let (left, _, _) = sound_of(clip, a_host_that_runs_everything(), false, 512);
    let (plain, _, _) = sound_of(the_clip(0, 3, 0, Vec::new()), a_host_that_runs_everything(), false, 512);
    assert_eq!(left.len(), plain.len());
    for (place, (made, was)) in left.iter().zip(&plain).enumerate() {
        assert!(
            (made - was * 0.125).abs() <= 1e-6 * was.abs(),
            "sample {place}: {made} and {was} * 0.125"
        );
    }
}

#[test]
fn a_seek_gives_the_same_samples_as_playing_through_where_the_plugin_has_no_memory_though_its_value_is_animated() {
    let _alone = the_installed_plugins_to_myself();
    // A clip from the timeline frame 10, for 90 frames, with a gain that rises from 0 at the frame 10 to 4 at the frame 100.
    let effect = an_animated_gain((10, 0.0), (100, 4.0));
    let (whole_left, whole_right, _) = sound_of(
        the_clip(0, 90, 10, vec![effect.clone()]),
        a_host_that_runs_everything(),
        false,
        512,
    );
    assert!(
        whole_left[0].abs() < 1e-6 && whole_left[whole_left.len() - 1] > 3.0,
        "it does rise"
    );
    // The same sound from the timeline frame 40: what the preview makes when it is started there (see `plan_sound`: the clip begins at the frame it is heard from, and the
    // frames of the keyframes are still those of the timeline).
    let (seeked_left, seeked_right, _) = sound_of(
        the_clip(30, 60, 40, vec![effect]),
        a_host_that_runs_everything(),
        false,
        512,
    );
    let from = sample_at_frame(thirty(), 30) as usize;
    assert_eq!(seeked_left.len(), whole_left.len() - from);
    assert_eq!(seeked_left, whole_left[from..], "the left, to the sample");
    assert_eq!(seeked_right, whole_right[from..], "and the right");
}

#[test]
fn the_preview_and_the_export_make_the_same_samples_when_the_plugin_keeps_up() {
    let _alone = the_installed_plugins_to_myself();
    let effect = an_animated_gain((0, 0.5), (60, 2.0));
    let (preview, _, _) = sound_of(
        the_clip(0, 60, 0, vec![effect.clone()]),
        a_host_that_runs_everything(),
        true,
        512,
    );
    let (export, _, _) = sound_of(
        the_clip(0, 60, 0, vec![effect]),
        a_host_that_runs_everything(),
        false,
        512,
    );
    assert_eq!(preview, export);
}

#[test]
fn the_effects_of_a_clip_are_asked_for_in_the_order_they_are_applied_in() {
    let _alone = the_installed_plugins_to_myself();
    struct Recording {
        asked_in_this_order: Mutex<Vec<u32>>,
    }
    impl PluginRunner for Recording {
        fn processor_for(
            &self,
            effect: &LayerEffect,
            _: &SoundClipToExport,
            _: FrameRate,
            _: &Notes,
            _: bool,
        ) -> Option<Box<dyn BlockProcessor>> {
            self.asked_in_this_order.lock().unwrap().push(effect.instance.number());
            None
        }
    }
    let kind = the_kind_of("ffrs_test_gain");
    let mut effects = Vec::new();
    for number in [7, 3, 9] {
        let mut effect = the_effect(kind, &[1.0]);
        effect.instance = EffectInstanceId::new(number);
        effects.push(effect);
    }
    let runner = Arc::new(Recording {
        asked_in_this_order: Mutex::new(Vec::new()),
    });
    let _ = sound_of(the_clip(0, 1, 0, effects), runner.clone(), false, 512);
    assert_eq!(*runner.asked_in_this_order.lock().unwrap(), [7, 3, 9]);
}

// --- a plugin that is too slow (the test plugin "counted" takes as long as its control says) ---

fn a_plugin_effect_of(
    label: &str,
    values: &[f32],
    for_the_preview: bool,
    patience: Option<f64>,
) -> (PluginEffect, Notes) {
    let kind = the_kind_of(label);
    let plugin = kind.installed_plugin().unwrap();
    let library = ladspa_host::Library::open(&plugin.library).unwrap();
    let instances = (0..Layout::of(plugin.audio_inputs, plugin.audio_outputs)
        .unwrap()
        .instances())
        .map(|_| {
            Box::new(super::LadspaCopy(
                library
                    .instantiate(plugin.address.ladspa_index().unwrap(), crate::sound_engine::SAMPLE_RATE)
                    .unwrap(),
            )) as Box<dyn super::PluginInstance>
        })
        .collect();
    let notes = Notes::default();
    let mut effect = PluginEffect::new(
        instances,
        plugin,
        the_effect(kind, values),
        thirty(),
        0,
        for_the_preview,
        Arc::clone(&notes),
    )
    .unwrap();
    if let Some(patience) = patience {
        effect = effect.with_patience(patience);
    }
    (effect, notes)
}

fn run_blocks(effect: &mut PluginEffect, count: usize) {
    for _ in 0..count {
        let (mut left, mut right) = (vec![0.25_f32; 512], vec![-0.25_f32; 512]);
        effect.process(&mut left, &mut right);
    }
}

#[test]
fn a_plugin_that_takes_longer_than_the_sound_for_the_patience_it_is_given_is_left_out_of_the_preview_and_said_once() {
    let _alone = the_installed_plugins_to_myself();
    // A block of 512 samples is 10.7 ms of sound, and this plugin takes 10 ms for each of the two channels.
    let (effect, notes) = a_plugin_effect_of("ffrs_test_counted", &[0.1], true, Some(0.05));
    let board = super::effect::ProblemBoard::default();
    let mut effect = effect.reporting_problems_on(Arc::clone(&board));
    run_blocks(&mut effect, 4);
    assert!(board.lock().unwrap().is_empty(), "nothing is wrong yet");
    assert!(!effect.is_left_out(), "four blocks are 43 ms of sound: not yet");
    assert!(notes.lock().unwrap().is_empty());
    run_blocks(&mut effect, 1);
    assert!(
        effect.is_left_out(),
        "five blocks are 53 ms of sound that took too long"
    );
    let said = notes.lock().unwrap().clone();
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(
        said[0].contains("too slow") && said[0].contains("0.05 seconds") && said[0].contains("unchanged"),
        "{said:?}"
    );
    // From then on the plugin is not run (a hundred blocks would take two seconds if it were) and the sound is as it was given.
    let began = std::time::Instant::now();
    let (mut left, mut right) = (vec![0.25_f32; 512], vec![-0.25_f32; 512]);
    for _ in 0..100 {
        effect.process(&mut left, &mut right);
    }
    assert!(began.elapsed() < Duration::from_millis(500), "{:?}", began.elapsed());
    assert_eq!((left[0], right[0]), (0.25, -0.25));
    assert_eq!(notes.lock().unwrap().len(), 1, "and it is not said again");
    let on_the_board: Vec<String> = board.lock().unwrap().values().cloned().collect();
    assert_eq!(on_the_board.len(), 1, "{on_the_board:?}");
    assert!(
        on_the_board[0].contains("too slow"),
        "the window can say why: {on_the_board:?}"
    );
}

#[test]
fn the_export_waits_for_a_plugin_that_is_too_slow_and_never_leaves_it_out() {
    let _alone = the_installed_plugins_to_myself();
    let (mut effect, notes) = a_plugin_effect_of("ffrs_test_counted", &[0.1], false, Some(0.05));
    run_blocks(&mut effect, 8);
    assert!(!effect.is_left_out());
    assert!(notes.lock().unwrap().is_empty());
}

#[test]
fn a_plugin_that_keeps_up_is_not_left_out_of_the_preview() {
    let _alone = the_installed_plugins_to_myself();
    let (mut effect, notes) = a_plugin_effect_of("ffrs_test_gain", &[1.0], true, Some(0.05));
    run_blocks(&mut effect, 300);
    assert!(!effect.is_left_out());
    assert!(notes.lock().unwrap().is_empty());
}

// --- what the mutation campaign found missing ---

fn assert_changed(host: &PluginHost, last: &mut u64, what: &str) {
    let now = host.generation();
    assert_ne!(now, *last, "{what} changes what may be run: the sound is made again");
    *last = now;
}

#[test]
fn every_change_of_what_may_be_run_changes_the_generation_and_a_repeat_or_a_taking_back_changes_nothing() {
    let host = PluginHost::new();
    let (a, b, c, d) = (
        PathBuf::from("/a"),
        PathBuf::from("/b"),
        PathBuf::from("/c"),
        PathBuf::from("/d"),
    );
    let mut last = host.generation();
    host.set_approved_folders([a.clone()]);
    assert_changed(&host, &mut last, "saying which folders were approved");
    host.set_approved_folders([a]);
    assert_eq!(host.generation(), last, "the same folders again: nothing changed");
    host.set_approved_folders([b]);
    assert_changed(&host, &mut last, "other folders");
    host.decline_folder(&c);
    assert_changed(&host, &mut last, "a no");
    host.forget_what_was_declined();
    assert_changed(&host, &mut last, "forgetting a no");
    host.forget_what_was_declined();
    assert_eq!(host.generation(), last, "nothing to forget: nothing changed");
    host.approve_folder(&c);
    assert_changed(&host, &mut last, "a yes");
    // A no that a yes took back is not there to be forgotten, and forgetting it makes no difference to the sound.
    host.decline_folder(&d);
    host.approve_folder(&d);
    last = host.generation();
    host.forget_what_was_declined();
    assert_eq!(host.generation(), last, "the no was taken back by the yes");
    host.allow_everything();
    assert_changed(&host, &mut last, "allowing everything");
}

#[test]
fn what_the_host_does_about_a_folder_is_approved_declined_or_not_asked_and_everything_allowed_approves_all() {
    let host = PluginHost::new();
    let (yes, no, nobody) = (PathBuf::from("/yes"), PathBuf::from("/no"), PathBuf::from("/nobody"));
    host.approve_folder(&yes);
    host.decline_folder(&no);
    assert_eq!(host.standing_of(&yes), FolderStanding::Approved);
    assert_eq!(host.standing_of(&no), FolderStanding::Declined);
    assert_eq!(host.standing_of(&nobody), FolderStanding::NotAsked);
    host.allow_everything();
    assert_eq!(host.standing_of(&nobody), FolderStanding::Approved);
    assert_eq!(host.standing_of(&no), FolderStanding::Approved);
}

#[test]
fn the_folders_above_a_plugin_library_are_looked_in_for_the_libraries_it_needs_until_a_person_says_not() {
    let host = PluginHost::new();
    assert_eq!(host.where_dependencies_are_looked_for(), (Vec::new(), true));
    host.look_for_dependencies_in(vec![PathBuf::from("/needed")], false);
    assert_eq!(
        host.where_dependencies_are_looked_for(),
        (vec![PathBuf::from("/needed")], false)
    );
}

#[test]
fn a_folder_that_was_wanted_and_is_then_approved_is_not_wanted_any_more() {
    let _alone = the_installed_plugins_to_myself();
    let (effect, plugin) = the_gain_effect();
    let host = PluginHost::new();
    let _ = ask(&host, &effect, &Notes::default());
    host.approve_folder(&the_folder_of(plugin));
    assert!(host.take_wanted().is_empty());
}

/// Copies of the test library, with the names `names`, in a folder of their own, and the plugin `plugin` as the plugin of each.
fn plugins_in_libraries_of_one_folder(
    folder: &std::path::Path,
    plugin: &InstalledPlugin,
    names: &[&str],
) -> Vec<InstalledPlugin> {
    names
        .iter()
        .map(|name| {
            let library = folder.join(name);
            std::fs::copy(&plugin.library, &library).unwrap();
            let mut copy = plugin.clone();
            copy.library = library;
            copy.descriptor.key = format!("ladspa:{name}:ffrs_test_gain").into();
            copy.descriptor.name = format!("gain of {name}").into();
            copy
        })
        .collect()
}

#[test]
fn two_libraries_of_one_folder_are_wanted_each_once_with_their_plugins_and_the_folder_once() {
    let _alone = the_installed_plugins_to_myself();
    let folder = tempfile::tempdir().unwrap();
    let plugins = plugins_in_libraries_of_one_folder(folder.path(), &the_test_plugins()[0], &["one.dll", "two.dll"]);
    set_installed_plugins(plugins);
    let host = PluginHost::new();
    let notes = Notes::default();
    for _ in 0..2 {
        for kind in EffectKind::installed_plugin_kinds() {
            let _ = ask(&host, &the_effect(kind, &[1.0]), &notes);
        }
    }
    let wanted = host.take_wanted();
    assert_eq!(wanted.len(), 1, "{wanted:?}");
    assert_eq!(wanted[0].libraries, ["one.dll", "two.dll"]);
    assert_eq!(wanted[0].plugins, ["gain of one.dll", "gain of two.dll"]);
}

#[test]
fn a_library_that_cannot_be_loaded_is_in_the_breadcrumb_all_the_same() {
    let _alone = the_installed_plugins_to_myself();
    let mut plugin = the_test_plugins().remove(0);
    let folder = tempfile::tempdir().unwrap();
    plugin.library = folder.path().join("nope.dll");
    set_installed_plugins(vec![plugin]);
    let effect = the_effect(EffectKind::installed_plugin_kinds()[0], &[1.0]);
    let host = PluginHost::new();
    host.allow_everything();
    let breadcrumb = folder.path().join("in_use.json");
    host.keep_a_breadcrumb(crate::plugin_scan::Breadcrumb::at(breadcrumb.clone()));

    assert!(ask(&host, &effect, &Notes::default()).is_none());

    let written = std::fs::read_to_string(&breadcrumb).expect("the library was written down before it was loaded");
    assert!(written.contains("nope.dll"), "{written}");
}

fn notes_when_the_plugin_is_changed_by(change: impl FnOnce(&mut InstalledPlugin)) -> Vec<String> {
    let mut plugin = the_test_plugins().remove(0);
    change(&mut plugin);
    set_installed_plugins(vec![plugin]);
    let effect = the_effect(EffectKind::installed_plugin_kinds()[0], &[1.0]);
    let host = PluginHost::new();
    host.allow_everything();
    let notes = Notes::default();
    assert!(
        ask(&host, &effect, &notes).is_none(),
        "a plugin that is not the one scanned is not run"
    );
    notes.lock().unwrap().clone()
}

#[test]
fn a_plugin_that_has_another_label_or_other_ports_than_the_one_that_was_scanned_is_not_run() {
    let _alone = the_installed_plugins_to_myself();
    for (what, said) in [
        (
            "another label",
            notes_when_the_plugin_is_changed_by(|plugin| {
                plugin.descriptor.key = "ladspa:good:ffrs_test_gain_of_another_time".into()
            }),
        ),
        (
            "other audio ports",
            notes_when_the_plugin_is_changed_by(|plugin| {
                plugin.audio_inputs = 2;
                plugin.audio_outputs = 2;
            }),
        ),
        (
            "a value on a port that is not a control",
            notes_when_the_plugin_is_changed_by(|plugin| plugin.value_handles = vec![1]),
        ),
    ] {
        assert_eq!(said.len(), 1, "{what}: {said:?}");
        assert!(said[0].contains("scan again"), "{what}: {said:?}");
    }
}

#[test]
fn a_plugin_that_fails_to_run_is_left_out_for_the_rest_of_the_play_and_said_once_and_the_sound_is_as_it_was() {
    let _alone = the_installed_plugins_to_myself();
    // The gain plugin, said to have two audio inputs and two outputs: it is made once and given two buffers where it has one port, which it refuses.
    let kind = the_kind_of("ffrs_test_gain");
    let mut plugin = kind.installed_plugin().unwrap().clone();
    plugin.audio_inputs = 2;
    plugin.audio_outputs = 2;
    let plugin: &'static InstalledPlugin = Box::leak(Box::new(plugin));
    let library = ladspa_host::Library::open(&plugin.library).unwrap();
    let instances: Vec<Box<dyn super::PluginInstance>> = vec![Box::new(super::LadspaCopy(
        library
            .instantiate(plugin.address.ladspa_index().unwrap(), crate::sound_engine::SAMPLE_RATE)
            .unwrap(),
    ))];
    let notes = Notes::default();
    let mut effect = PluginEffect::new(
        instances,
        plugin,
        the_effect(kind, &[0.5]),
        thirty(),
        0,
        false,
        Arc::clone(&notes),
    )
    .unwrap();

    run_blocks(&mut effect, 3);

    assert!(effect.is_left_out());
    let said = notes.lock().unwrap().clone();
    assert_eq!(said.len(), 1, "said once, though three blocks came: {said:?}");
    assert!(
        said[0].contains("could not be run") && said[0].contains("unchanged"),
        "{said:?}"
    );
    let (mut left, mut right) = (vec![0.25_f32; 8], vec![-0.25_f32; 8]);
    effect.process(&mut left, &mut right);
    assert_eq!((left[0], right[0]), (0.25, -0.25), "the sound goes through as it is");
}

#[test]
fn channels_of_different_lengths_are_made_as_far_as_the_shorter_goes() {
    let _alone = the_installed_plugins_to_myself();
    let (mut processor, _) = processor_of(&the_effect(the_kind_of("ffrs_test_gain"), &[2.0]), 0);
    let (mut left, mut right) = (vec![1.0_f32; 4], vec![1.0_f32; 2]);
    processor.process(&mut left, &mut right);
    assert_eq!(left, [2.0, 2.0, 1.0, 1.0], "only as far as the right channel goes");
    assert_eq!(right, [2.0, 2.0]);
}

#[test]
fn a_plugin_is_too_slow_after_two_seconds_as_the_design_says() {
    assert_eq!(SECONDS_TOO_SLOW, 2.0);
}

#[test]
fn the_frame_of_a_sample_is_exact_at_the_end_of_a_frame_whatever_the_blocks_are() {
    let _alone = the_installed_plugins_to_myself();
    let kind = the_kind_of("ffrs_test_gain");
    let mut effect = the_effect(kind, &[1.0]);
    let gain = PropertyId::effect_value(kind, EffectInstanceId::FIRST, 0);
    let keyframe = |frame, value| Keyframe {
        frame,
        value: PropertyValue::Number(value),
    };
    // The gain is the number of the frame.
    effect.animation =
        Animation::from_keyframes([(gain, Keyframes::from_unordered([keyframe(0, 0.0), keyframe(4, 4.0)]))]);
    let (mut processor, _) = processor_of(&effect, 0);

    // The first block ends one sample before the end of the frame 0 (1600 samples); the next begins with the last sample of it, and a look at the values is made there.
    let (mut left, mut right) = (vec![1.0_f32; 1599], vec![1.0_f32; 1599]);
    processor.process(&mut left, &mut right);
    let (mut left, mut right) = (vec![1.0_f32; 64], vec![1.0_f32; 64]);
    processor.process(&mut left, &mut right);

    assert_eq!(left[0], 0.0, "the sample 1599 is in the frame 0, where the gain is 0");
    assert_eq!(
        left[31], 0.0,
        "and so is the rest of the 32 that are looked at together"
    );
    assert_eq!(left[32], 1.0, "the next 32 begin at the sample 1631, in the frame 1");
}

// --- what the sound of a project needs from the folders ---

#[test]
fn the_folders_that_the_sound_of_a_project_needs_have_each_library_and_plugin_once_whatever_the_number_of_effects() {
    use crate::video_export::test_projects::{clip_location, project_with_video};
    let _alone = the_installed_plugins_to_myself();
    let gain = the_kind_of("ffrs_test_gain");
    let swap = the_kind_of("ffrs_test_swap");
    let mut project = project_with_video(320, 180, false);
    let clip = clip_location(&project, 0, 0);
    for kind in [swap, gain, gain, EffectKind::HUE_SHIFT] {
        project
            .add_effect(crate::project::AnimationTarget::Clip(clip), kind)
            .unwrap();
    }

    let needed = folders_the_sound_of_a_project_needs(&project);

    assert_eq!(needed.len(), 1, "{needed:?}");
    let library = the_test_plugins()[0]
        .library
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        needed[0].libraries,
        [library],
        "one library, though two plugins of it and three effects"
    );
    assert_eq!(
        needed[0].plugins,
        ["test plugin ffrs_test_gain", "test plugin ffrs_test_swap"],
        "each plugin once, in the order of their names"
    );
    assert_eq!(needed[0].folder, the_folder_of(&the_test_plugins()[0]));
}

// --- the search path of the system, where the editor can change it ---

#[cfg(windows)]
#[test]
fn the_folders_for_libraries_are_put_before_the_search_path_of_the_system_and_the_system_folders_stay_in_it() {
    let _alone = the_installed_plugins_to_myself();
    let (effect, plugin) = the_gain_effect();
    let folder = tempfile::tempdir().unwrap();
    let host = PluginHost::new();
    host.allow_everything();
    host.look_for_dependencies_in(vec![folder.path().to_owned()], false);

    assert!(ask(&host, &effect, &Notes::default()).is_some(), "{:?}", plugin.library);

    let path = std::env::var_os("PATH").unwrap();
    let entries: Vec<PathBuf> = std::env::split_paths(&path).collect();
    // (Other tests of this program put their own folders there at the same time, so what is looked at is what must always be there.)
    let system = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32");
    assert!(
        entries
            .iter()
            .any(|entry| entry.to_string_lossy().eq_ignore_ascii_case(&system.to_string_lossy())),
        "the folders of Windows are still in the search path: {entries:?}"
    );
}

#[test]
fn sound_more_than_a_thousand_times_as_loud_as_the_loudest_is_silence_and_said_once_and_a_thousand_is_let_through() {
    let _alone = the_installed_plugins_to_myself();
    let (mut processor, notes) = processor_of(&the_effect(the_kind_of("ffrs_test_gain"), &[4.0]), 0);
    let (mut left, mut right) = (vec![300.0_f32, 100.0, 250.0, -300.0], vec![1.0_f32; 4]);

    processor.process(&mut left, &mut right);

    assert_eq!(
        left,
        [0.0, 400.0, 1000.0, 0.0],
        "1200 and -1200 are silence; 400 and exactly 1000 are let through"
    );
    assert_eq!(right, [4.0; 4]);
    let said = notes.lock().unwrap().clone();
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("made silent") && said[0].contains("1000"), "{said:?}");

    let (mut left, mut right) = (vec![300.0_f32], vec![1.0_f32]);
    processor.process(&mut left, &mut right);
    assert_eq!(left, [0.0]);
    assert_eq!(notes.lock().unwrap().len(), 1, "said once, though it happened again");
}

// --- CLAP plugins (piece 7.8.4), with the real test plugins of `crates/clap-test-plugins` loaded in this process ---

/// The plugins of the CLAP test library, as the scan makes them effects.
fn the_clap_test_plugins() -> Vec<InstalledPlugin> {
    static SCANNED: std::sync::OnceLock<Vec<InstalledPlugin>> = std::sync::OnceLock::new();
    SCANNED
        .get_or_init(|| {
            let path = clap_test_plugins::library_file_for_tests();
            let scan = clap_host::scan::scan_library(&path, |_| {}).expect("the CLAP test library is scanned");
            scan.plugins
                .iter()
                .filter_map(|plugin| crate::plugin_scan::effect_of_a_clap_plugin(&path, plugin).ok())
                .collect()
        })
        .clone()
}

/// The kind of the CLAP test plugin `id`, with the list of what is installed made of the CLAP test plugins.
fn the_clap_kind_of(id: &str) -> EffectKind {
    set_installed_plugins(the_clap_test_plugins());
    let key = format!("clap:{id}");
    EffectKind::installed_plugin_kinds()
        .into_iter()
        .find(|kind| kind.key() == key)
        .unwrap_or_else(|| panic!("no CLAP plugin called {id}"))
}

fn the_clap_effect(id: &str, values: Vec<PropertyValue>) -> LayerEffect {
    LayerEffect {
        instance: EffectInstanceId::FIRST,
        kind: the_clap_kind_of(id),
        values,
        animation: Animation::default(),
        tables: Vec::new(),
    }
}

fn numbers(values: &[f32]) -> Vec<PropertyValue> {
    values.iter().map(|value| PropertyValue::Number(*value)).collect()
}

/// The two channels of a sound, and what the plugins said while it was made.
type SoundAndNotes = (Vec<f32>, Vec<f32>, Notes);

/// The sound of `frames` frames of the ramp, with the effect on it, in blocks of `block`; and the same with no effect.
fn with_and_without(effect: LayerEffect, frames: u64, block: usize) -> (SoundAndNotes, (Vec<f32>, Vec<f32>)) {
    let with = sound_of(
        the_clip(0, frames, 0, vec![effect]),
        a_host_that_runs_everything(),
        false,
        block,
    );
    let (left, right, _) = sound_of(
        the_clip(0, frames, 0, Vec::new()),
        a_host_that_runs_everything(),
        false,
        block,
    );
    (with, (left, right))
}

#[test]
fn a_clap_gain_plugin_on_a_clip_in_the_graph_gives_the_arithmetic_exactly_for_blocks_of_any_size() {
    let _alone = the_installed_plugins_to_myself();
    let half = |plain: &[f32]| plain.iter().map(|sample| sample * 0.5).collect::<Vec<f32>>();
    for block in [512, 100, 4096, 1] {
        let frames = if block == 1 { 1 } else { 45 };

        let ((left, right, notes), (plain_left, plain_right)) =
            with_and_without(the_clap_effect(clap_test_plugins::GAIN, numbers(&[0.5])), frames, block);

        assert_eq!(left, half(&plain_left), "blocks of {block}, left");
        assert_eq!(right, half(&plain_right), "blocks of {block}, right");
        assert!(notes.lock().unwrap().is_empty(), "{:?}", notes.lock().unwrap());
    }
}

#[test]
fn a_clap_value_that_is_animated_is_what_the_keyframes_say_at_the_frame_of_each_stretch_of_32_samples_and_the_same_for_any_blocks()
 {
    let _alone = the_installed_plugins_to_myself();
    let mut effect = the_clap_effect(clap_test_plugins::GAIN, numbers(&[1.0]));
    let gain = PropertyId::effect_value(effect.kind, EffectInstanceId::FIRST, 0);
    let keyframe = |frame, value| Keyframe {
        frame,
        value: PropertyValue::Number(value),
    };
    // From 0 at the frame 10 to 4 at the frame 40, on a clip that begins at the frame 10.
    effect.animation =
        Animation::from_keyframes([(gain, Keyframes::from_unordered([keyframe(10, 0.0), keyframe(40, 4.0)]))]);
    let samples = 48_000;
    let made = |block: usize| {
        let (mut processor, _) = processor_of(&effect, 10);
        let (mut left, mut right) = (vec![1.0_f32; samples], vec![1.0_f32; samples]);
        for start in (0..samples).step_by(block) {
            let end = (start + block).min(samples);
            processor.process(&mut left[start..end], &mut right[start..end]);
        }
        (left, right)
    };

    let (left, right) = made(512);

    for index in [0, 31, 32, 1599, 1600, 1601, 24_000, 40_000, 47_999] {
        let stretch_begins_at = (index / 32) * 32;
        let frame = 10 + stretch_begins_at as u64 * 30 / 48_000;
        let expected = ((frame - 10) as f32 / 30.0 * 4.0).clamp(0.0, 4.0);
        assert!(
            (left[index] - expected).abs() < 1e-4,
            "sample {index}: the gain is {} and the keyframes say {expected} at frame {frame}",
            left[index]
        );
        assert_eq!(left[index], right[index]);
    }
    // The places that the keyframes are looked at are the same whatever the blocks are (unlike a plugin that is run in pieces).
    for block in [13, 100, 1000, 4096] {
        assert_eq!(made(block), (left.clone(), right.clone()), "blocks of {block}");
    }
}

#[test]
fn a_clap_value_changes_at_the_sample_where_its_stretch_begins_and_not_before() {
    let _alone = the_installed_plugins_to_myself();
    let mut effect = the_clap_effect(clap_test_plugins::GAIN, numbers(&[1.0]));
    let gain = PropertyId::effect_value(effect.kind, EffectInstanceId::FIRST, 0);
    let keyframe = |frame, value| Keyframe {
        frame,
        value: PropertyValue::Number(value),
    };
    // 0 at the frame 0, 3 at the frame 3: a frame is 1600 samples, so the gain is 0 for 1600 samples, 1 for the next 1600...
    effect.animation =
        Animation::from_keyframes([(gain, Keyframes::from_unordered([keyframe(0, 0.0), keyframe(3, 3.0)]))]);
    let (mut processor, _) = processor_of(&effect, 0);
    let (mut left, mut right) = (vec![1.0_f32; 4800], vec![1.0_f32; 4800]);

    processor.process(&mut left, &mut right);

    assert_eq!(
        (left[1599], left[1600]),
        (0.0, 1.0),
        "the value is the next at the sample the frame begins"
    );
    assert_eq!((left[3199], left[3200]), (1.0, 2.0));
}

#[test]
fn a_clap_plugin_with_a_latency_is_made_up_for_in_the_graph_so_the_sound_is_where_it_was_for_any_blocks() {
    let _alone = the_installed_plugins_to_myself();
    let (mut processor, _) = processor_of(&the_clap_effect(clap_test_plugins::LATENCY, Vec::new()), 0);
    assert_eq!(
        processor.latency(),
        clap_test_plugins::LATENCY_SAMPLES as usize,
        "the effect says the latency of the plugin"
    );
    let (mut left, mut right) = (vec![1.0_f32; 8], vec![1.0_f32; 8]);
    processor.process(&mut left, &mut right);
    assert_eq!(
        left, [0.0; 8],
        "without the graph the sound is late: silence while the plugin fills up"
    );

    for block in [512, 100, 1, 4096] {
        let frames = if block == 1 { 1 } else { 45 };

        let ((left, right, notes), (plain_left, plain_right)) =
            with_and_without(the_clap_effect(clap_test_plugins::LATENCY, Vec::new()), frames, block);

        assert_eq!(left, plain_left, "blocks of {block}, left");
        assert_eq!(right, plain_right, "blocks of {block}, right");
        assert!(notes.lock().unwrap().is_empty(), "{:?}", notes.lock().unwrap());
    }
}

#[test]
fn a_clap_plugin_of_one_channel_is_made_twice_and_one_with_a_side_chain_is_given_silence_for_it() {
    let _alone = the_installed_plugins_to_myself();
    let ((left, right, _), (plain_left, plain_right)) =
        with_and_without(the_clap_effect(clap_test_plugins::MONO_HALF, Vec::new()), 3, 512);
    assert_eq!(left, plain_left.iter().map(|sample| sample * 0.5).collect::<Vec<_>>());
    assert_eq!(right, plain_right.iter().map(|sample| sample * 0.5).collect::<Vec<_>>());

    let ((left, right, _), (plain_left, plain_right)) =
        with_and_without(the_clap_effect(clap_test_plugins::SIDE_CHAIN, Vec::new()), 3, 512);
    assert_eq!((left, right), (plain_left, plain_right), "the main input plus silence");

    let ((left, right, _), (plain_left, plain_right)) =
        with_and_without(the_clap_effect(clap_test_plugins::SWAP, Vec::new()), 3, 512);
    assert_eq!(
        (left, right),
        (plain_right, plain_left),
        "the channels are given to each other"
    );
}

#[test]
fn a_clap_choice_is_given_as_the_number_of_its_option_and_a_whole_number_as_it_is() {
    let _alone = the_installed_plugins_to_myself();
    // `Steps` 2 and `Mode` option 1: the sound times (2 + 1) times (1 + 1).
    let values = vec![PropertyValue::Number(2.0), PropertyValue::Words("1".to_owned())];

    let ((left, _, _), (plain, _)) = with_and_without(the_clap_effect(clap_test_plugins::CHOICE, values), 3, 512);

    assert_eq!(left, plain.iter().map(|sample| sample * 6.0).collect::<Vec<_>>());
}

#[test]
fn a_clap_plugin_that_checks_its_threads_is_told_the_truth_when_it_runs_in_the_graph() {
    let _alone = the_installed_plugins_to_myself();

    let ((left, right, _), (plain_left, plain_right)) =
        with_and_without(the_clap_effect(clap_test_plugins::THREADS, Vec::new()), 3, 512);

    assert_eq!(
        (left, right),
        (plain_left, plain_right),
        "the sign is not changed: it was made, activated and told which thread is which as the contract says"
    );
}

#[test]
fn a_clap_plugin_that_asks_to_be_made_again_is_made_again_between_blocks_and_given_its_values_again() {
    let _alone = the_installed_plugins_to_myself();
    let half = |plain: &[f32]| plain.iter().map(|sample| sample * 0.5).collect::<Vec<f32>>();

    let ((left, right, notes), (plain_left, plain_right)) =
        with_and_without(the_clap_effect(clap_test_plugins::CALLBACKS, numbers(&[0.5])), 45, 100);

    assert_eq!(
        (left, right),
        (half(&plain_left), half(&plain_right)),
        "each time it is made again it is told its value again, so it is not heard to be made again"
    );
    let said = notes.lock().unwrap().clone();
    assert_eq!(
        said.len(),
        1,
        "it asks every time it is made, so the limit is reached and said once: {said:?}"
    );
    assert!(said[0].contains("more than 5 times"), "{said:?}");
}

#[test]
fn a_clap_plugin_that_says_process_failed_is_left_out_and_said_and_the_sound_is_as_it_was() {
    let _alone = the_installed_plugins_to_myself();
    // The gain, said to be the plugin that fails when it processes (and to have none of the gain's values).
    let mut plugin = the_clap_kind_of(clap_test_plugins::GAIN)
        .installed_plugin()
        .unwrap()
        .clone();
    plugin.descriptor.key = format!("clap:{}", clap_test_plugins::FAILS_TO_PROCESS).into();
    plugin.descriptor.parameters = std::borrow::Cow::Owned(Vec::new());
    plugin.address = crate::project::PluginAddress::Clap {
        id: clap_test_plugins::FAILS_TO_PROCESS.to_owned(),
    };
    plugin.value_handles = Vec::new();
    set_installed_plugins(vec![plugin]);
    let kind = EffectKind::installed_plugin_kinds()[0];
    let effect = LayerEffect {
        instance: EffectInstanceId::FIRST,
        kind,
        values: Vec::new(),
        animation: Animation::default(),
        tables: Vec::new(),
    };

    let ((left, right, notes), (plain_left, plain_right)) = with_and_without(effect, 3, 512);

    assert_eq!((left, right), (plain_left, plain_right));
    let said = notes.lock().unwrap().clone();
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(
        said[0].contains("could not be run") && said[0].contains("unchanged"),
        "{said:?}"
    );
}

#[test]
fn a_clap_plugin_that_is_not_the_one_that_was_scanned_is_not_run_and_says_to_scan_again() {
    let _alone = the_installed_plugins_to_myself();
    for (what, change) in [
        (
            "a value that the plugin has no parameter for",
            Box::new(|plugin: &mut InstalledPlugin| plugin.value_handles = vec![99])
                as Box<dyn Fn(&mut InstalledPlugin)>,
        ),
        (
            "one of two values that the plugin has no parameter for",
            Box::new(|plugin: &mut InstalledPlugin| plugin.value_handles = vec![1, 99]),
        ),
        (
            "other channels",
            Box::new(|plugin: &mut InstalledPlugin| {
                plugin.audio_inputs = 1;
                plugin.audio_outputs = 1;
            }),
        ),
    ] {
        let mut plugin = the_clap_kind_of(clap_test_plugins::GAIN)
            .installed_plugin()
            .unwrap()
            .clone();
        change(&mut plugin);
        set_installed_plugins(vec![plugin]);
        let effect = the_effect(EffectKind::installed_plugin_kinds()[0], &[1.0]);
        let host = PluginHost::new();
        host.allow_everything();
        let notes = Notes::default();

        assert!(ask(&host, &effect, &notes).is_none(), "{what}");

        let said = notes.lock().unwrap().clone();
        assert_eq!(said.len(), 1, "{what}: {said:?}");
        assert!(said[0].contains("scan again"), "{what}: {said:?}");
    }
}

#[test]
fn a_clap_library_is_loaded_only_from_a_folder_that_was_said_yes_to_and_is_in_the_breadcrumb_before_it_is() {
    let _alone = the_installed_plugins_to_myself();
    let effect = the_clap_effect(clap_test_plugins::GAIN, numbers(&[1.0]));
    let library = effect.kind.installed_plugin().unwrap().library.clone();
    let host = PluginHost::new();
    let notes = Notes::default();
    let folder = tempfile::tempdir().unwrap();
    let breadcrumb = folder.path().join("in_use.json");
    host.keep_a_breadcrumb(crate::plugin_scan::Breadcrumb::at(breadcrumb.clone()));

    assert!(
        ask(&host, &effect, &notes).is_none(),
        "nobody has said yes to the folder"
    );
    assert!(host.loaded_libraries().is_empty());
    assert!(!breadcrumb.exists(), "nothing was loaded, so nothing is written");
    let wanted = host.take_wanted();
    assert_eq!(wanted.len(), 1);
    assert_eq!(wanted[0].folder, folder_of(&library));

    host.approve_folder(&folder_of(&library));
    assert!(ask(&host, &effect, &notes).is_some());
    assert_eq!(host.loaded_libraries(), std::slice::from_ref(&library));
    let written = std::fs::read_to_string(&breadcrumb).expect("the library was written down before it was loaded");
    assert!(written.contains("clap_test_plugins"), "{written}");
}

#[test]
fn a_clap_library_that_cannot_be_loaded_is_said_and_the_sound_goes_through() {
    let _alone = the_installed_plugins_to_myself();
    let mut plugin = the_clap_kind_of(clap_test_plugins::GAIN)
        .installed_plugin()
        .unwrap()
        .clone();
    plugin.library = PathBuf::from("/nowhere/at/all/gone.clap");
    set_installed_plugins(vec![plugin]);
    let effect = the_effect(EffectKind::installed_plugin_kinds()[0], &[1.0]);
    let host = PluginHost::new();
    host.allow_everything();
    let notes = Notes::default();

    assert!(ask(&host, &effect, &notes).is_none());

    let said = notes.lock().unwrap().clone();
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(
        said[0].contains("could not be loaded") && said[0].contains("gone.clap"),
        "{said:?}"
    );
}

#[test]
fn a_clap_copy_that_asks_to_be_made_again_is_made_again_when_the_blocks_are_between() {
    use super::instance::{ClapCopy, ClapMaker, PluginInstance};
    let library = clap_host::Library::open(&clap_test_plugins::library_file_for_tests()).unwrap();
    let worker = clap_host::MainThread::start();
    let maker = || ClapMaker {
        worker: Arc::clone(&worker),
        library: library.clone(),
        id: clap_test_plugins::CALLBACKS.to_owned(),
    };
    let lives = Arc::new(Mutex::new(super::Lives::default()));
    let mut copy = ClapCopy::new(
        maker().make().unwrap(),
        maker(),
        super::Lives::is_alive(&lives, library.path(), 0),
    );
    assert_eq!(copy.between_blocks(), BetweenBlocks::default(), "nothing was asked for");
    let (mut input, mut output) = ([vec![1.0_f32; 8], vec![1.0; 8]], [vec![0.0_f32; 8], vec![0.0; 8]]);
    let [in_left, in_right] = &mut input;
    let [out_left, out_right] = &mut output;

    copy.run(&mut [in_left, in_right], &mut [out_left, out_right], 8)
        .unwrap();
    let done = copy.between_blocks();

    assert!(done.was_restarted, "the plugin asked in its first block");
    assert_eq!(done.note, None, "its latency is the same");
    assert_eq!(
        copy.between_blocks(),
        BetweenBlocks::default(),
        "the new copy has asked for nothing yet"
    );
}

#[test]
fn a_clap_plugin_that_asks_to_be_made_again_every_time_is_made_again_a_few_times_and_then_goes_on_as_it_is_and_it_is_said_once()
 {
    let _alone = the_installed_plugins_to_myself();
    let latency = clap_test_plugins::LATENCY_SAMPLES as usize;
    let most = super::instance::MOST_RESTARTS_OF_A_COPY;

    let ((left, _, notes), (plain, _)) =
        with_and_without(the_clap_effect(clap_test_plugins::LATENCY_CHANGES, Vec::new()), 45, 100);

    // Each time it was made again it began with nothing in its memory: a gap as long as its latency at the start of each of the blocks that it was made again before.
    for restart in 0..most {
        let from = restart * 100;
        assert_eq!(
            &left[from..from + latency],
            &vec![0.0; latency][..],
            "the gap of the restart number {restart}"
        );
        assert_eq!(
            &left[from + latency..from + 100],
            &plain[from + latency..from + 100],
            "and then the sound"
        );
    }
    assert_eq!(
        &left[most * 100..],
        &plain[most * 100..],
        "after the last time it goes on as it is"
    );
    let said = notes.lock().unwrap().clone();
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("more than 5 times"), "{said:?}");
}

#[test]
fn the_places_where_the_keyframes_are_looked_at_in_a_piece_are_its_start_and_every_32nd_sample_of_the_clip() {
    use super::places_to_look_in;
    assert_eq!(places_to_look_in(0, 100, true), [0, 32, 64, 96]);
    assert_eq!(
        places_to_look_in(10, 100, true),
        [0, 22, 54, 86],
        "the sample 32 of the clip is the place 22 of a piece that begins at 10"
    );
    assert_eq!(places_to_look_in(32, 64, true), [0, 32]);
    assert_eq!(places_to_look_in(31, 2, true), [0, 1]);
    assert_eq!(
        places_to_look_in(33, 31, true),
        [0],
        "no multiple of 32 within the piece but its start"
    );
    assert_eq!(
        places_to_look_in(0, 100, false),
        [0],
        "values that are not animated are looked at once"
    );
    assert_eq!(places_to_look_in(5, 1, true), [0]);
}

#[test]
fn what_is_wrong_with_a_plugin_is_on_the_board_for_the_window_until_it_is_wanted_again_and_nothing_is_wrong() {
    let _alone = the_installed_plugins_to_myself();
    let mut plugin = the_clap_kind_of(clap_test_plugins::GAIN)
        .installed_plugin()
        .unwrap()
        .clone();
    let good_library = plugin.library.clone();
    let key = plugin.descriptor.key.to_string();
    plugin.library = PathBuf::from("/nowhere/at/all/gone.clap");
    set_installed_plugins(vec![plugin.clone()]);
    let effect = the_effect(EffectKind::installed_plugin_kinds()[0], &[1.0]);
    let host = PluginHost::new();
    host.allow_everything();
    assert_eq!(host.problem_with(&key), None, "nothing has been wanted yet");

    assert!(ask(&host, &effect, &Notes::default()).is_none());

    let said = host
        .problem_with(&key)
        .expect("the library could not be loaded, and the board says so");
    assert!(
        said.contains("could not be loaded") && said.contains("gone.clap"),
        "{said}"
    );

    // The library is there now: the plugin is wanted again, and it works, and the board is clean.
    plugin.library = good_library;
    set_installed_plugins(vec![plugin]);
    let effect = the_effect(EffectKind::installed_plugin_kinds()[0], &[1.0]);
    assert!(ask(&host, &effect, &Notes::default()).is_some());
    assert_eq!(host.problem_with(&key), None);
}

#[test]
fn a_plugin_that_is_left_out_while_it_runs_says_why_on_the_board() {
    let _alone = the_installed_plugins_to_myself();
    let mut plugin = the_clap_kind_of(clap_test_plugins::GAIN)
        .installed_plugin()
        .unwrap()
        .clone();
    plugin.descriptor.key = format!("clap:{}", clap_test_plugins::FAILS_TO_PROCESS).into();
    plugin.descriptor.parameters = std::borrow::Cow::Owned(Vec::new());
    plugin.address = crate::project::PluginAddress::Clap {
        id: clap_test_plugins::FAILS_TO_PROCESS.to_owned(),
    };
    plugin.value_handles = Vec::new();
    let key = plugin.descriptor.key.to_string();
    set_installed_plugins(vec![plugin]);
    let effect = LayerEffect {
        instance: EffectInstanceId::FIRST,
        kind: EffectKind::installed_plugin_kinds()[0],
        values: Vec::new(),
        animation: Animation::default(),
        tables: Vec::new(),
    };
    let host = a_host_that_runs_everything();

    sound_of(
        the_clip(0, 3, 0, vec![effect]),
        Arc::clone(&host) as Arc<dyn PluginRunner>,
        false,
        512,
    );

    let said = host
        .problem_with(&key)
        .expect("it could not be run, and the board says so");
    assert!(said.contains("could not be run"), "{said}");
}

// --- sound from before a clip: a seek into a tail ---

/// The sound of the CLAP echo (a tail of 1000 samples) on the ramp, for a clip of `frames` frames that begins `first_source_frame` frames into the file, with the plugin asking
/// for `pre_roll_samples` of sound from before (what the scan says, unless a test says another).
fn the_echo_on(first_source_frame: u64, frames: u64, pre_roll_samples: usize, block: usize) -> Vec<f32> {
    let mut plugin = the_clap_kind_of(clap_test_plugins::TAIL)
        .installed_plugin()
        .unwrap()
        .clone();
    plugin.pre_roll_samples = pre_roll_samples;
    set_installed_plugins(vec![plugin]);
    let effect = the_effect(EffectKind::installed_plugin_kinds()[0], &[]);
    let mut clip = the_clip(first_source_frame, frames, 0, vec![effect]);
    clip.first_timeline_frame = 0;
    // (The clip begins at the start of its file, and the stretch that is made begins `first_source_frame` frames into it.)
    clip.frames_of_the_clip_before = first_source_frame;
    sound_of(clip, a_host_that_runs_everything(), false, block).0
}

#[test]
fn a_clap_plugin_with_a_tail_says_how_much_sound_from_before_a_clip_it_wants_and_one_that_says_nothing_wants_half_a_second()
 {
    let _alone = the_installed_plugins_to_myself();
    // The echo says a tail of 1000 samples.
    let echo = the_clap_kind_of(clap_test_plugins::TAIL)
        .installed_plugin()
        .unwrap()
        .pre_roll_samples;
    assert_eq!(echo, clap_test_plugins::TAIL_SAMPLES as usize);
    // The gain says nothing of a tail: half a second.
    let gain = the_clap_kind_of(clap_test_plugins::GAIN)
        .installed_plugin()
        .unwrap()
        .pre_roll_samples;
    assert_eq!(gain, 24_000);
    // A tail for ever is two seconds, and a tail past two seconds is two seconds.
    assert_eq!(crate::plugin_scan::pre_roll_of_a_clap_plugin(Some(u32::MAX)), 96_000);
    assert_eq!(crate::plugin_scan::pre_roll_of_a_clap_plugin(Some(500_000)), 96_000);
    assert_eq!(crate::plugin_scan::pre_roll_of_a_clap_plugin(Some(0)), 0);
    assert_eq!(crate::plugin_scan::pre_roll_of_a_clap_plugin(None), 24_000);
}

#[test]
fn a_clip_that_begins_in_the_middle_of_an_echo_gives_the_samples_of_playing_through_to_it_and_without_the_sound_from_before_it_does_not()
 {
    let _alone = the_installed_plugins_to_myself();
    let samples_a_frame = 1600;
    let whole = the_echo_on(0, 60, clap_test_plugins::TAIL_SAMPLES as usize, 512);
    let from_the_middle = |pre_roll: usize, block: usize| the_echo_on(30, 30, pre_roll, block);
    let second_half = &whole[30 * samples_a_frame..];

    for block in [512, 100, 4096] {
        let with = from_the_middle(clap_test_plugins::TAIL_SAMPLES as usize, block);
        assert_eq!(with.len(), second_half.len());
        // The echo of what came more than its tail ago (1000 samples, ten echoes, a thousandth) is not in what is heard; the sound from before is two frames, which is more.
        let worst = with
            .iter()
            .zip(second_half)
            .map(|(made, was)| (made - was).abs() / was.abs().max(1.0))
            .fold(0.0_f32, f32::max);
        assert!(worst < 1e-4, "blocks of {block}: the worst difference is {worst}");
    }

    // With no sound from before the clip the echo begins empty: the first hundred samples are the sound alone, and the difference is as big as the echo itself.
    let without = from_the_middle(0, 512);
    let worst_at_the_start = without[..100]
        .iter()
        .zip(&second_half[..100])
        .map(|(made, was)| (made - was).abs() / was.abs().max(1.0))
        .fold(0.0_f32, f32::max);
    assert!(
        worst_at_the_start > 0.2,
        "{worst_at_the_start}: the control shows that the test can tell"
    );
}

#[test]
fn the_keyframes_of_a_value_are_those_of_the_clip_and_the_sound_from_before_it_does_not_move_them() {
    let _alone = the_installed_plugins_to_myself();
    // The gain says no tail, so it asks for half a second (15 frames) of sound from before the clip; this clip begins 30 frames into its file, so it has them.
    let kind = the_clap_kind_of(clap_test_plugins::GAIN);
    assert_eq!(kind.installed_plugin().unwrap().pre_roll_samples, 24_000);
    let mut effect = the_clap_effect(clap_test_plugins::GAIN, numbers(&[1.0]));
    let gain = PropertyId::effect_value(kind, EffectInstanceId::FIRST, 0);
    let keyframe = |frame, value| Keyframe {
        frame,
        value: PropertyValue::Number(value),
    };
    // From 0 at the frame 10 to 4 at the frame 40, on a clip that begins at the frame 10.
    effect.animation =
        Animation::from_keyframes([(gain, Keyframes::from_unordered([keyframe(10, 0.0), keyframe(40, 4.0)]))]);
    let host = a_host_that_runs_everything();
    // (A stretch that begins 30 frames into a clip that begins at the start of its file: it has the 15 frames that the gain asks for.)
    let mut seeked = the_clip(30, 30, 10, vec![effect]);
    seeked.frames_of_the_clip_before = 30;
    let (made, _, _) = sound_of(seeked, Arc::clone(&host) as Arc<dyn PluginRunner>, false, 512);
    let (plain, _, _) = sound_of(the_clip(30, 30, 10, Vec::new()), host, false, 512);

    assert_eq!(made.len(), 30 * 1600);
    for index in [0_usize, 31, 32, 1_599, 1_600, 1_601, 24_000, 40_000, 47_999] {
        let stretch_begins_at = (index / 32) * 32;
        let frame = 10 + stretch_begins_at as u64 / 1600;
        let expected = ((frame - 10) as f32 / 30.0 * 4.0).clamp(0.0, 4.0) * plain[index];
        assert!(
            (made[index] - expected).abs() <= 1e-4 * plain[index].abs().max(1.0),
            "sample {index}: {} and the keyframes say {expected} at frame {frame}",
            made[index]
        );
    }
}

// --- plugins that cannot be alive together: a stream that is being stopped, and the one that takes its place (a seek) ---

/// The processor that `host` makes for the `single_instance` test plugin for the stream whose notes are `notes`, and what it gives for a block of ones: 1 if it was made when it was
/// the only one of its plugin alive, and -1 if another was alive.
fn a_single_instance_for(host: &PluginHost, notes: &Notes) -> Option<Box<dyn BlockProcessor>> {
    let effect = the_clap_effect(clap_test_plugins::SINGLE_INSTANCE, Vec::new());
    host.processor_for(&effect, &a_clip_at(0, vec![effect.clone()]), thirty(), notes, true)
}

fn what_it_gives(processor: &mut dyn BlockProcessor) -> f32 {
    let (mut left, mut right) = (vec![1.0_f32; 64], vec![1.0_f32; 64]);
    processor.process(&mut left, &mut right);
    left[63]
}

fn a_host_that_runs_everything_and_waits_no_longer_than(longest: Duration) -> PluginHost {
    let host = PluginHost::new().waiting_no_longer_than(longest);
    host.allow_everything();
    host
}

#[test]
fn a_plugin_made_for_a_stream_waits_for_the_plugins_of_the_stream_that_is_being_stopped_and_so_is_not_made_while_one_is_alive()
 {
    let _alone = the_installed_plugins_to_myself();
    let host = a_host_that_runs_everything_and_waits_no_longer_than(Duration::from_secs(10));
    let (old, new) = (Notes::default(), Notes::default());
    let mut earlier = a_single_instance_for(&host, &old).expect("the plugin of the stream that is stopped");
    assert_eq!(what_it_gives(&mut *earlier), 1.0, "alone");
    host.stream_is_stopping(&old);
    let began = std::time::Instant::now();
    let lets_go = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(250));
        drop(earlier);
    });

    let mut later = a_single_instance_for(&host, &new).expect("the plugin of the stream that takes its place");

    assert!(
        began.elapsed() >= Duration::from_millis(200),
        "it waited until the other was let go of: {:?}",
        began.elapsed()
    );
    assert_eq!(
        what_it_gives(&mut *later),
        1.0,
        "so it was the only one alive when it was made"
    );
    lets_go.join().unwrap();
}

#[test]
fn the_plugins_of_streams_that_are_not_being_stopped_are_not_waited_for_two_clips_are_heard_together() {
    let _alone = the_installed_plugins_to_myself();
    let host = a_host_that_runs_everything_and_waits_no_longer_than(Duration::from_secs(10));
    let (first, second) = (Notes::default(), Notes::default());
    let _earlier = a_single_instance_for(&host, &first).unwrap();
    let began = std::time::Instant::now();

    let mut later = a_single_instance_for(&host, &second).unwrap();

    assert!(began.elapsed() < Duration::from_millis(500), "{:?}", began.elapsed());
    assert_eq!(
        what_it_gives(&mut *later),
        -1.0,
        "it was made while another was alive (a plugin like this one cannot be helped then)"
    );
}

#[test]
fn a_stream_that_is_being_stopped_makes_no_more_plugins_and_the_sound_of_it_goes_through() {
    let _alone = the_installed_plugins_to_myself();
    let host = a_host_that_runs_everything_and_waits_no_longer_than(Duration::from_secs(10));
    let notes = Notes::default();
    host.stream_is_stopping(&notes);

    assert!(a_single_instance_for(&host, &notes).is_none());
    assert!(
        host.problem_with("clap:org.fastforwardrs.test.single_instance")
            .is_none(),
        "it is not a problem of the plugin"
    );
    let another = Notes::default();
    assert!(
        a_single_instance_for(&host, &another).is_some(),
        "a stream that is not being stopped has its plugins"
    );
}

#[test]
fn the_wait_for_a_plugin_that_is_not_let_go_of_ends_at_the_time_it_is_given() {
    let _alone = the_installed_plugins_to_myself();
    let host = a_host_that_runs_everything_and_waits_no_longer_than(Duration::from_millis(300));
    let (old, new) = (Notes::default(), Notes::default());
    let _held = a_single_instance_for(&host, &old).unwrap();
    host.stream_is_stopping(&old);
    let began = std::time::Instant::now();

    let later = a_single_instance_for(&host, &new);

    assert!(later.is_some(), "it is made all the same when the time is up");
    let waited = began.elapsed();
    assert!(
        waited >= Duration::from_millis(250) && waited < Duration::from_secs(5),
        "{waited:?}"
    );
}

#[test]
fn a_stream_that_is_gone_is_not_waited_for_even_if_it_was_said_to_be_stopping() {
    use super::Lives;
    let lives = Arc::new(Mutex::new(Lives::default()));
    let library = std::path::Path::new("/plugins/a.clap");
    let notes: Notes = Notes::default();
    let guard = Lives::is_alive(&lives, library, 1);
    lives.lock().unwrap().stream_is_stopping(1, &notes);
    assert!(
        lives
            .lock()
            .unwrap()
            .is_something_of_a_stopping_stream_alive(library, 2)
    );
    assert!(
        !lives
            .lock()
            .unwrap()
            .is_something_of_a_stopping_stream_alive(library, 1),
        "not for itself"
    );
    assert!(
        !lives
            .lock()
            .unwrap()
            .is_something_of_a_stopping_stream_alive(std::path::Path::new("/plugins/b.clap"), 2),
        "not for another library"
    );

    drop(notes);
    assert!(
        !lives
            .lock()
            .unwrap()
            .is_something_of_a_stopping_stream_alive(library, 2),
        "the stream is gone (what names it is not there any more)"
    );
    drop(guard);
}

#[test]
fn a_plugin_that_is_let_go_of_is_not_waited_for_even_if_its_stream_is_still_being_stopped() {
    use super::Lives;
    let lives = Arc::new(Mutex::new(Lives::default()));
    let library = std::path::Path::new("/plugins/a.clap");
    let notes: Notes = Notes::default();
    let (first, second) = (Lives::is_alive(&lives, library, 1), Lives::is_alive(&lives, library, 1));
    lives.lock().unwrap().stream_is_stopping(1, &notes);
    drop(first);
    assert!(
        lives
            .lock()
            .unwrap()
            .is_something_of_a_stopping_stream_alive(library, 2),
        "one of the two is still alive"
    );

    drop(second);

    assert!(
        !lives
            .lock()
            .unwrap()
            .is_something_of_a_stopping_stream_alive(library, 2),
        "both are let go of: there is nothing to wait for, though the stream is still said to be stopping"
    );
}

#[test]
fn a_plugin_whose_start_is_unreliable_says_so_once_in_the_sound_it_is_run_in_and_one_that_is_not_says_nothing() {
    let _alone = the_installed_plugins_to_myself();
    let mut plugin = the_clap_kind_of(clap_test_plugins::GAIN)
        .installed_plugin()
        .unwrap()
        .clone();
    plugin.start_warning = Some("its start depends on memory it did not write".to_owned());
    set_installed_plugins(vec![plugin]);
    let kind = EffectKind::installed_plugin_kinds()[0];
    let effect = || LayerEffect {
        instance: EffectInstanceId::FIRST,
        kind,
        values: kind.default_values(),
        animation: Animation::default(),
        tables: Vec::new(),
    };

    let ((_, _, notes), _) = with_and_without(effect(), 3, 512);

    let said = notes.lock().unwrap().clone();
    assert_eq!(said.len(), 1, "once, however many blocks are run: {said:?}");
    assert!(
        said[0].contains("unreliable at its start")
            && said[0].contains("its start depends on memory it did not write")
            && said[0].contains("\u{ab}gain\u{bb}"),
        "{said:?}"
    );

    // The plugin of the same kind that has no warning says nothing.
    let mut quiet = kind.installed_plugin().unwrap().clone();
    quiet.start_warning = None;
    set_installed_plugins(vec![quiet]);
    let ((_, _, notes), _) = with_and_without(effect(), 3, 512);
    assert!(notes.lock().unwrap().is_empty(), "{:?}", notes.lock().unwrap());
}

#[test]
fn the_bundled_folder_and_the_folders_in_it_are_trusted_and_no_other_folder_is_not_even_one_named_like_it() {
    let host = PluginHost::new();
    let root = tempfile::tempdir().unwrap();
    let bundled = root.path().join("plugins");
    std::fs::create_dir_all(bundled.join("reverbs").join("deeper")).unwrap();
    std::fs::create_dir_all(root.path().join("plugins_of_someone")).unwrap();
    std::fs::create_dir_all(root.path().join("other")).unwrap();
    let key = |path: &std::path::Path| crate::plugin_trust::folder_key(path);
    for folder in [&bundled, &bundled.join("reverbs"), &root.path().join("other")] {
        assert_eq!(
            host.standing_of(&key(folder)),
            FolderStanding::NotAsked,
            "before: {folder:?}"
        );
    }

    host.trust_bundled_folder(&bundled);

    assert_eq!(host.standing_of(&key(&bundled)), FolderStanding::Approved);
    assert_eq!(
        host.standing_of(&key(&bundled.join("reverbs"))),
        FolderStanding::Approved
    );
    assert_eq!(
        host.standing_of(&key(&bundled.join("reverbs").join("deeper"))),
        FolderStanding::Approved
    );
    assert_eq!(
        host.standing_of(&key(&root.path().join("plugins_of_someone"))),
        FolderStanding::NotAsked,
        "a folder whose name begins like the bundled one is another folder"
    );
    assert_eq!(
        host.standing_of(&key(&root.path().join("other"))),
        FolderStanding::NotAsked
    );
}
