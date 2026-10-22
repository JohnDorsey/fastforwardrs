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
    // (Rewritten on purpose in phase 6.6: a transition is run now. A shader that reads one of the two pictures of a transition and not the other is not one, and says so.)
    let only_start = "/*{ \"INPUTS\": [ { \"NAME\": \"startImage\", \"TYPE\": \"image\" } ] }*/
void main() { gl_FragColor = IMG_THIS_PIXEL(startImage); }
";
    assert!(reason("only_start.fs", only_start).contains("startImage"));
    assert!(reason("only_start.fs", only_start).contains("endImage"));
    let only_end = "/*{ \"INPUTS\": [ { \"NAME\": \"endImage\", \"TYPE\": \"image\" } ] }*/
void main() { gl_FragColor = IMG_THIS_PIXEL(endImage); }
";
    assert!(reason("only_end.fs", only_end).contains("startImage"));
    let all_three = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"startImage\", \"TYPE\": \"image\" }, { \"NAME\": \"endImage\", \"TYPE\": \"image\" } ] }*/
void main() { gl_FragColor = IMG_THIS_PIXEL(inputImage); }
";
    assert!(reason("all_three.fs", all_three).contains("inputImage"));
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
    // (Rewritten on purpose in phase 6.4.4: a shader that brings an image is run now if the project has the image; one given none says that it does not have it.)
    let imported = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ], \"IMPORTED\": { \"tex\": { \"PATH\": \"a.png\" } } }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(inputImage); }\n";
    assert!(reason("imported.fs", imported).contains("an image of its own, tex, which the project does not have"));
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

    // (Rewritten on purpose in phase 6.6: a transition is run now, and the kind that cannot be run is one that does not compile.)
    let unusable = EffectKind::from_snapshot(descriptor_from("kind_bad.fs", BAD_SYNTAX)).unwrap();
    assert!(!unusable.is_installed(), "a shader that does not compile cannot be run");
    assert!(!usable.is_a_generator());
    assert!(!usable.is_a_transition());
    let transition = EffectKind::from_snapshot(descriptor_from("kind_wipe.fs", WIPE)).unwrap();
    assert!(
        transition.is_installed() && transition.is_a_transition() && !transition.is_a_generator(),
        "a transition is run"
    );
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
    // Neither a shader that is not usable nor an effect that is no shader depends on anything. (Rewritten on purpose in phase 6.6: the example of a shader that is not usable is one that does not compile.)
    assert!(!depends_on_earlier_frames(&descriptor_from("bad.fs", BAD_SYNTAX)));
    assert!(!depends_on_earlier_frames(EffectKind::HUE_SHIFT.descriptor()));
}

/// A PNG of 2 by 2 pixels.
fn a_png() -> Vec<u8> {
    crate::rgba_picture::RgbaPicture {
        width_in_pixels: 2,
        height_in_pixels: 2,
        rgba_pixels: [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 9, 9, 9, 255].to_vec(),
    }
    .to_png_bytes()
    .unwrap()
}

const WITH_TWO_IMAGES: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ], \"IMPORTED\": { \"noise\": { \"PATH\": \"n.png\" }, \"ramp\": { \"PATH\": \"r.png\" } } }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(noise) + IMG_THIS_PIXEL(ramp); }\n";

fn with_images(name: &str, images: Vec<crate::project::ShaderImage>) -> EffectDescriptor {
    crate::isf::descriptor_with_images_of(&read(WITH_TWO_IMAGES).unwrap(), name, WITH_TWO_IMAGES, images)
}

#[test]
fn a_shader_that_brings_images_is_usable_when_it_has_all_of_them_in_the_order_of_its_header_and_each_is_a_picture() {
    use crate::project::ShaderImage;
    let both = || vec![ShaderImage::new("noise", a_png()), ShaderImage::new("ramp", a_png())];
    let descriptor = with_images("brings_both.fs", both());
    assert!(is_usable(&descriptor), "{:?}", why_not_usable(&descriptor));
    let (_, structure) = crate::engine::published_effect_shader_with_structure(&descriptor.key).unwrap();
    assert_eq!(structure.imported_images(), 2);

    let said = |descriptor: &EffectDescriptor| {
        assert!(!is_usable(descriptor));
        why_not_usable(descriptor).unwrap()
    };
    let none = said(&with_images("brings_none.fs", Vec::new()));
    assert!(
        none.contains("an image of its own, noise, which the project does not have"),
        "{none}"
    );
    let one = said(&with_images("brings_one.fs", vec![ShaderImage::new("noise", a_png())]));
    assert!(
        one.contains("an image of its own, ramp, which the project does not have"),
        "{one}"
    );
    let wrong_order = said(&with_images(
        "brings_them_the_other_way.fs",
        vec![ShaderImage::new("ramp", a_png()), ShaderImage::new("noise", a_png())],
    ));
    assert!(wrong_order.contains("ramp where noise is expected"), "{wrong_order}");
    let not_a_picture = said(&with_images(
        "brings_text.fs",
        vec![
            ShaderImage::new("noise", a_png()),
            ShaderImage::new("ramp", b"not a picture".to_vec()),
        ],
    ));
    assert!(
        not_a_picture.contains("the image ramp of this shader cannot be used"),
        "{not_a_picture}"
    );
    let too_many = said(&with_images(
        "brings_three.fs",
        vec![
            ShaderImage::new("noise", a_png()),
            ShaderImage::new("ramp", a_png()),
            ShaderImage::new("extra", a_png()),
        ],
    ));
    assert!(
        too_many.contains("brings 3 images and its header lists 2"),
        "{too_many}"
    );
}

#[test]
fn the_same_shader_with_other_images_is_another_kind_and_both_are_run_each_with_its_own() {
    use crate::project::ShaderImage;
    let mut other_png = a_png();
    // (Another picture: the pixels of the first are changed, and the file is made again.)
    other_png = crate::rgba_picture::RgbaPicture {
        width_in_pixels: 2,
        height_in_pixels: 2,
        rgba_pixels: [1, 2, 3, 255].repeat(4),
    }
    .to_png_bytes()
    .map(|bytes| if bytes == other_png { Vec::new() } else { bytes })
    .unwrap();
    let first = with_images(
        "same_name.fs",
        vec![ShaderImage::new("noise", a_png()), ShaderImage::new("ramp", a_png())],
    );
    let second = with_images(
        "same_name.fs",
        vec![ShaderImage::new("noise", other_png), ShaderImage::new("ramp", a_png())],
    );
    assert_ne!(first.key, second.key);
    assert!(is_usable(&first) && is_usable(&second));
    // And a descriptor that was changed by hand so that its images are not the ones its key was made from is not run.
    let mut forged = first.clone();
    forged.shader.as_mut().unwrap().images = second.shader.as_ref().unwrap().images.clone();
    assert!(!is_usable(&forged));
    assert!(
        why_not_usable(&forged)
            .unwrap()
            .contains("not the one that its key was made from")
    );
}

#[test]
fn a_transition_is_usable_and_is_on_the_shelf_of_the_engine_as_a_shader_that_reads_what_is_behind_the_layer() {
    let wipe = descriptor_from("shelf_wipe.fs", WIPE);
    assert!(is_usable(&wipe), "{:?}", why_not_usable(&wipe));
    assert_eq!(why_not_usable(&wipe), None);
    assert_eq!(shape_of(&wipe), Some(Shape::Transition));
    let (_, structure) = crate::engine::published_effect_shader_with_structure(&wipe.key).expect("it is on the shelf");
    assert!(structure.reads_the_backdrop());
    assert_eq!(
        structure.images,
        vec![crate::engine::ImageSlot::Backdrop, crate::engine::ImageSlot::Picture]
    );
    // A filter is on the shelf as one that does not.
    let filter = descriptor_from("shelf_filter_for_the_wipe.fs", COLOR_CONTROLS);
    assert!(is_usable(&filter));
    let (_, structure) = crate::engine::published_effect_shader_with_structure(&filter.key).unwrap();
    assert!(!structure.reads_the_backdrop());
}

#[test]
fn a_transition_is_for_the_clips_that_are_seen_and_for_groups_and_not_for_a_track_or_an_adjustment_clip() {
    use crate::project::{ClipKind, GroupKind};
    let transition = EffectKind::from_snapshot(descriptor_from("rules_wipe.fs", WIPE)).unwrap();
    assert!(transition.is_a_transition());
    for seen in [ClipKind::Video, ClipKind::Image, ClipKind::Text, ClipKind::Generator] {
        assert!(transition.applies_to_a_clip_of_kind(seen), "{seen:?}");
    }
    // (An adjustment clip has no picture of its own for a transition to bring over what is behind it, and a sound is not seen.)
    for not_for in [ClipKind::Adjustment, ClipKind::Audio] {
        assert!(!transition.applies_to_a_clip_of_kind(not_for), "{not_for:?}");
    }
    assert!(
        !transition.applies_to_a_track(),
        "a track has no picture of its own: it is the clips on it"
    );
    // (A tag has it done to each of its members, and a group, which is a picture of its own, has it done to that.)
    for group in [GroupKind::Tag, GroupKind::ClipGroup, GroupKind::TrackGroup] {
        assert!(transition.applies_to_a_group_of_kind(group), "{group:?}");
    }

    // A filter is for all that it was for before.
    let filter = EffectKind::from_snapshot(descriptor_from("rules_filter.fs", COLOR_CONTROLS)).unwrap();
    assert!(!filter.is_a_transition());
    for kind in [
        ClipKind::Video,
        ClipKind::Image,
        ClipKind::Text,
        ClipKind::Generator,
        ClipKind::Adjustment,
    ] {
        assert!(filter.applies_to_a_clip_of_kind(kind), "{kind:?}");
    }
    assert!(!filter.applies_to_a_clip_of_kind(ClipKind::Audio));
    assert!(filter.applies_to_a_track());
    // And the effects of the program are as they were.
    assert!(!EffectKind::HUE_SHIFT.is_a_transition());
    assert!(EffectKind::HUE_SHIFT.applies_to_a_track());
    assert!(EffectKind::HUE_SHIFT.applies_to_a_clip_of_kind(ClipKind::Adjustment));
}
