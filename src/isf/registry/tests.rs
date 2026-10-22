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
    assert!(reason("plasma.fs", PLASMA).contains("generator"));
    assert!(reason("wipe.fs", WIPE).contains("transition"));
    assert!(reason("blur.fs", BLUR).contains("passes of its own"));
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

    let unusable = EffectKind::from_snapshot(descriptor_from("kind_plasma.fs", PLASMA)).unwrap();
    assert!(!unusable.is_installed(), "a generator cannot be run yet");
    // The kinds of the program and of plugins are as they were.
    assert!(EffectKind::HUE_SHIFT.is_installed());
    assert!(EffectKind::HUE_SHIFT.shader_source().is_none());
}
