// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which shaders are usable: the ones that compile and that this version can run, once each, with the reason for the others.

use super::*;
use crate::isf::descriptor_of;
use crate::project::EffectKind;

const COLOR_CONTROLS: &str = include_str!("../fixtures/color_controls.fs");
const PLASMA: &str = include_str!("../fixtures/plasma.fs");
const WIPE: &str = include_str!("../fixtures/wipe_transition.fs");
const BLUR: &str = include_str!("../fixtures/gaussian_blur.fs");
const OLD_STYLE: &str = include_str!("../fixtures/old_style.fs");
const POSTERIZE: &str = include_str!("../fixtures/posterize.fs");
const VIGNETTE: &str = include_str!("../fixtures/vignette.fs");
const BAD_SYNTAX: &str = include_str!("../fixtures/bad_syntax.fs");

fn descriptor_from(name: &str, source: &str) -> EffectDescriptor {
    descriptor_of(&read(source).unwrap(), name, source)
}

#[test]
fn the_filters_that_compile_are_usable_and_say_nothing_against_themselves() {
    for (name, source) in [
        ("color_controls.fs", COLOR_CONTROLS),
        ("vignette.fs", VIGNETTE),
        ("posterize.fs", POSTERIZE),
        ("old_style.fs", OLD_STYLE),
    ] {
        let descriptor = descriptor_from(name, source);
        assert!(is_usable(&descriptor), "{name}: {:?}", why_not_usable(&descriptor));
        assert_eq!(why_not_usable(&descriptor), None);
    }
}

#[test]
fn what_this_version_cannot_run_yet_is_not_usable_and_says_why() {
    let reason = |name: &str, source: &str| {
        let descriptor = descriptor_from(name, source);
        assert!(!is_usable(&descriptor), "{name}");
        why_not_usable(&descriptor).unwrap_or_else(|| panic!("{name} says nothing"))
    };
    // (Rewritten on purpose in phase 6: a generator is run now, so the example of what is not is the transition alone.)
    assert!(reason("wipe.fs", WIPE).contains("transition"));
    // (Rewritten on purpose in phase 6.4: a shader with passes of its own is run now, and since 6.4.2 so is one with a buffer that lasts: see the tests that say so.)
    let two_writers = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ], \"PASSES\": [ { \"TARGET\": \"a\" }, { \"TARGET\": \"a\" }, {} ] }*/
void main() { gl_FragColor = IMG_THIS_PIXEL(a); }
";
    assert!(reason("two_writers.fs", two_writers).contains("more than one pass"));
    let into_a_buffer_last = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ], \"PASSES\": [ { \"TARGET\": \"a\" } ] }*/
void main() { gl_FragColor = IMG_THIS_PIXEL(inputImage); }
";
    assert!(reason("into_a_buffer_last.fs", into_a_buffer_last).contains("last pass draws into a buffer"));
    let bad_size = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ], \"PASSES\": [ { \"TARGET\": \"a\", \"WIDTH\": \"$WIDTH /\" }, {} ] }*/
void main() { gl_FragColor = IMG_THIS_PIXEL(a); }
";
    assert!(reason("bad_size.fs", bad_size).contains("the WIDTH of pass number 1"));
    let other_image = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"maskImage\", \"TYPE\": \"image\" } ] }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(inputImage); }\n";
    assert!(reason("mask.fs", other_image).contains("maskImage"));
    let imported = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ], \"IMPORTED\": { \"tex\": { \"PATH\": \"a.png\" } } }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(inputImage); }\n";
    assert!(reason("imported.fs", imported).contains("IMPORTED"));
}

#[test]
fn a_shader_that_does_not_compile_is_not_usable_and_says_where() {
    let descriptor = descriptor_from("bad.fs", BAD_SYNTAX);

    assert!(!is_usable(&descriptor));
    let reason = why_not_usable(&descriptor).unwrap();
    assert!(reason.starts_with("line 12, column"), "{reason}");
}

#[test]
fn a_header_that_cannot_be_read_is_told_by_the_reason_too_if_a_project_kept_such_a_text() {
    // A project file that was edited by hand can carry any text as a shader.
    let mut descriptor = descriptor_from("color_controls.fs", COLOR_CONTROLS);
    descriptor.key = crate::isf::key_of("hand_made.fs", "void main() {}").into();
    descriptor.shader.as_mut().unwrap().text = "void main() {}".into();

    assert!(!is_usable(&descriptor));
    assert!(why_not_usable(&descriptor).unwrap().contains("no header"));
}

#[test]
fn a_text_that_is_not_the_one_its_key_was_made_from_is_not_run_and_does_not_take_the_place_of_the_one_that_is() {
    let good = descriptor_from("same_key.fs", COLOR_CONTROLS);
    // The same key with another text in it, as a file that was changed by hand has.
    let mut changed = good.clone();
    changed.shader.as_mut().unwrap().text = format!(
        "{COLOR_CONTROLS}
// changed"
    )
    .into();

    // (Asked in the unlucky order: the changed one first.)
    assert!(!is_usable(&changed));
    assert!(why_not_usable(&changed).unwrap().contains("changed by hand"));
    assert!(is_usable(&good), "the good text is still run, as itself");
    assert_eq!(
        &*crate::engine::published_effect_shader(&good.key).unwrap(),
        compiled_shader_of(&good).unwrap().as_ref().as_ref().unwrap().wgsl,
        "and it is the good one that is on the shelf"
    );
    assert!(!is_usable(&changed), "and the changed one is still not run");
}

#[test]
fn each_shader_is_compiled_once_and_what_it_made_is_kept() {
    let descriptor = descriptor_from("once.fs", &format!("{COLOR_CONTROLS}\n// once"));
    let first = compiled_shader_of(&descriptor).unwrap();
    let second = compiled_shader_of(&descriptor).unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    // And another text, even of one name, is another shader.
    let other = descriptor_from("once.fs", &format!("{COLOR_CONTROLS}\n// twice"));
    assert!(!Arc::ptr_eq(&first, &compiled_shader_of(&other).unwrap()));
}

#[test]
fn what_compiled_is_on_the_shelf_of_the_engine_under_the_key_and_what_did_not_is_not() {
    let good = descriptor_from("shelf_good.fs", COLOR_CONTROLS);
    let compiled = compiled_shader_of(&good).unwrap();
    let wgsl = compiled.as_ref().as_ref().unwrap().wgsl.clone();
    assert_eq!(&*crate::engine::published_effect_shader(&good.key).unwrap(), wgsl);

    let bad = descriptor_from("shelf_bad.fs", BAD_SYNTAX);
    assert!(compiled_shader_of(&bad).is_some());
    assert!(crate::engine::published_effect_shader(&bad.key).is_none());
}

#[test]
fn a_shader_with_passes_of_its_own_is_usable_and_the_engine_is_given_what_it_is_made_of_with_it() {
    let blur = descriptor_from("shelf_blur.fs", BLUR);
    assert!(is_usable(&blur), "{:?}", why_not_usable(&blur));
    assert_eq!(why_not_usable(&blur), None);
    assert!(EffectKind::from_snapshot(blur.clone()).unwrap().is_installed());

    let (wgsl, structure) =
        crate::engine::published_effect_shader_with_structure(&blur.key).expect("it is on the shelf");
    assert_eq!(
        &*wgsl,
        compiled_shader_of(&blur).unwrap().as_ref().as_ref().unwrap().wgsl
    );
    assert_eq!(structure.passes.len(), 2);
    assert_eq!(structure.buffers.len(), 1);
    assert_eq!(structure.pass_index_number, Some(3));
    // A shader of one pass is on the shelf with the structure of one pass, which still is not the structure of a shader of phase 1 (it has a number for the pass).
    let plain = descriptor_from("shelf_plain.fs", COLOR_CONTROLS);
    assert!(is_usable(&plain));
    let (_, plain_structure) = crate::engine::published_effect_shader_with_structure(&plain.key).unwrap();
    assert_eq!(plain_structure.passes.len(), 1);
    assert_eq!(plain_structure.pass_index_number, Some(3));
}

#[test]
fn an_effect_that_is_no_shader_is_none_of_this_hosts_business() {
    assert!(compiled_shader_of(EffectKind::HUE_SHIFT.descriptor()).is_none());
    assert!(!is_usable(EffectKind::HUE_SHIFT.descriptor()));
    assert_eq!(why_not_usable(EffectKind::HUE_SHIFT.descriptor()), None);
}

#[test]
fn the_kind_of_effect_a_shader_is_is_installed_if_the_shader_is_usable() {
    let usable = EffectKind::from_snapshot(descriptor_from("kind_usable.fs", COLOR_CONTROLS)).unwrap();
    assert!(usable.is_installed());
    assert_eq!(usable.shader_source().unwrap().text, COLOR_CONTROLS);
    assert_eq!(usable.domain(), crate::project::EffectDomain::Picture);

    // (Rewritten on purpose in phase 6: a generator is run now, and the kind that cannot be run is a transition.)
    let unusable = EffectKind::from_snapshot(descriptor_from("kind_wipe.fs", WIPE)).unwrap();
    assert!(!unusable.is_installed(), "a transition cannot be run yet");
    assert!(!usable.is_a_generator());
    let generator = EffectKind::from_snapshot(descriptor_from("kind_plasma.fs", PLASMA)).unwrap();
    assert!(
        generator.is_installed() && generator.is_a_generator(),
        "a generator is run"
    );
    // The kinds of the program and of plugins are as they were.
    assert!(EffectKind::HUE_SHIFT.is_installed());
    assert!(EffectKind::HUE_SHIFT.shader_source().is_none());
}

#[test]
fn a_shader_with_a_buffer_that_lasts_is_usable_and_says_that_it_depends_on_earlier_frames_and_one_without_does_not() {
    let lasting = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ], \"PASSES\": [ { \"TARGET\": \"memory\", \"PERSISTENT\": true }, {} ] }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(memory); }\n";
    let lasting = descriptor_from("lasting.fs", lasting);
    assert!(is_usable(&lasting), "{:?}", why_not_usable(&lasting));
    assert!(depends_on_earlier_frames(&lasting));
    for (name, source) in [
        ("blur.fs", BLUR),
        ("color_controls.fs", COLOR_CONTROLS),
        ("plasma.fs", PLASMA),
    ] {
        let descriptor = descriptor_from(name, source);
        assert!(is_usable(&descriptor));
        assert!(!depends_on_earlier_frames(&descriptor), "{name}");
    }
    // Neither a shader that is not usable nor an effect that is no shader depends on anything.
    assert!(!depends_on_earlier_frames(&descriptor_from("wipe.fs", WIPE)));
    assert!(!depends_on_earlier_frames(EffectKind::HUE_SHIFT.descriptor()));
}
