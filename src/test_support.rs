// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Helpers shared by the unit tests in several modules.

use std::borrow::Cow;

use crate::project::{
    AnimationTarget, Clip, ClipId, EffectDescriptor, EffectDomain, EffectInstanceId, EffectKind, Effects,
    ParameterDescriptor, Project, PropertyId, PropertyValue, RackDestination, TextClip, TextObject, TimelineFrameIndex,
    Track, TrackId, UnknownFields, properties_of_the_text_effect, values_of_a_text_object,
};

/// The property that is the one value of the hue shift when the hue shift is the first effect of the chain it is in, which is what it is
/// when it is the only effect, as it is in nearly every test (see [`EffectInstanceId::FIRST`]).
pub const HUE_SHIFT_AS_THE_FIRST_EFFECT: PropertyId =
    PropertyId::effect_value(EffectKind::HUE_SHIFT, EffectInstanceId::FIRST, 0);

/// What a new text says, its font, its size and its colour, as the properties that they are since phase 9: the values `text`, `font`, `size` and `colour` of the text effect that is
/// the first of the chain of a text clip that is new (and of every clip that was cut from it or copied). A text was given these as properties of the clip itself before, and the tests that
/// went through them go through these.
pub const TEXT_OF_A_TEXT: PropertyId = PropertyId::effect_value(EffectKind::TEXT, EffectInstanceId::FIRST, 0);
pub const FONT_OF_A_TEXT: PropertyId = PropertyId::effect_value(EffectKind::TEXT, EffectInstanceId::FIRST, 1);
pub const SIZE_OF_A_TEXT: PropertyId = PropertyId::effect_value(EffectKind::TEXT, EffectInstanceId::FIRST, 2);
pub const COLOUR_OF_A_TEXT: PropertyId = PropertyId::effect_value(EffectKind::TEXT, EffectInstanceId::FIRST, 3);

/// The effects of a text clip that says what `object` says: the one effect that it is made of (see [`crate::project::TextClip::content`]), the text effect, which is the first of the chain
/// and has the words, the font, the size and the colour of `object` for its values. (Where `object` is on the picture is the clip's: see [`a_text_clip_that_says`].)
pub fn the_effects_of_a_text_that_says(object: &TextObject) -> Effects {
    let mut effects = Effects::none();
    let content = effects.add(EffectKind::TEXT);
    assert_eq!(
        content,
        EffectInstanceId::FIRST,
        "a chain that is new has no effect yet"
    );
    for (property, value) in properties_of_the_text_effect(content)
        .into_iter()
        .zip(values_of_a_text_object(object))
    {
        assert!(effects.set_value(property, &value), "the text effect has this value");
    }
    effects
}

/// A text clip that shows for `frames` frames, and the effects it has, which say what `object` says: where `object` is on the picture is what the clip has for its place.
pub fn a_text_clip_that_says(object: &TextObject, frames: u64) -> (Clip, Effects) {
    let clip = TextClip::new(frames, EffectInstanceId::FIRST)
        .expect("it lasts a frame at least")
        .with_placement(object.placement());
    (Clip::Text(clip), the_effects_of_a_text_that_says(object))
}

/// The same track, and every text on it that has no effect yet has the text effect, saying what a new text object says (a text that was made by the code of a test as a clip alone, with
/// the content that a new text clip has, is made whole by this).
pub fn with_the_text_effect_on_every_text(track: Track) -> Track {
    let effects: Vec<Effects> = track
        .entries()
        .iter()
        .map(|entry| match entry.clip() {
            Clip::Text(_) if entry.effects().is_empty() => the_effects_of_a_text_that_says(&TextObject::default()),
            _ => entry.effects().clone(),
        })
        .collect();
    track.with_effects_on_its_clips(effects)
}

/// The descriptor of an effect that the program does not have, as a file would bring it: the key `key`, the version 1.0, a picture effect that
/// changes each pixel by itself, and these values, `(id, what it begins as)`, each named by its id, with a label that is its id with a
/// capital letter, and no range.
pub fn the_descriptor_of_an_effect_the_program_does_not_have(
    key: &str,
    values: &[(&str, PropertyValue)],
) -> EffectDescriptor {
    EffectDescriptor {
        shader: None,
        key: Cow::Owned(key.to_owned()),
        version: (1, 0),
        name: Cow::Owned(key.split_once(':').map_or(key, |(_, id)| id).replace('_', " ")),
        description: Cow::Borrowed("Made up for a test"),
        category: Cow::Borrowed(""),
        domain: EffectDomain::Picture,
        changes_each_pixel_by_itself: true,
        parameters: Cow::Owned(
            values
                .iter()
                .map(|(id, default)| ParameterDescriptor {
                    id: Cow::Owned((*id).to_owned()),
                    name: Cow::Owned((*id).replace('_', " ")),
                    label: Cow::Owned(
                        id.chars()
                            .next()
                            .map(|first| first.to_uppercase().chain(id.chars().skip(1)).collect())
                            .unwrap_or_default(),
                    ),
                    default: default.clone(),
                    range: None,
                    unit: Cow::Borrowed(""),
                    decimals: None,
                    unknown_fields: UnknownFields::none(),
                    scale: crate::project::ValueScale::Linear,
                    control: crate::project::ParameterControl::Automatic,
                })
                .collect(),
        ),
        unknown_fields: UnknownFields::none(),
    }
}

/// The kind of an effect that the program does not have and that uses a resource: `test:uses_a_table`, with one value, `table`, which is the name of a resource of the
/// kind `cube` (words, with the control that wants a resource; it begins as nothing).
pub fn a_kind_of_effect_that_uses_a_resource() -> EffectKind {
    let mut descriptor = the_descriptor_of_an_effect_the_program_does_not_have(
        "test:uses_a_table",
        &[("table", PropertyValue::Words(String::new()))],
    );
    descriptor.parameters.to_mut()[0].control = crate::project::ParameterControl::Resource(Cow::Borrowed("cube"));
    EffectKind::from_snapshot(descriptor).expect("the descriptor is one that can be used")
}

/// The kind of an effect that the program does not have (see [`the_descriptor_of_an_effect_the_program_does_not_have`]), made as a file's
/// snapshot makes one.
pub fn a_kind_of_effect_the_program_does_not_have(key: &str, values: &[(&str, PropertyValue)]) -> EffectKind {
    EffectKind::from_snapshot(the_descriptor_of_an_effect_the_program_does_not_have(key, values))
        .expect("the descriptor is one that can be used")
}

/// Every property that a clip or a track has of its own, and, as the sample of the values of effects, the value of the hue shift that is
/// the first effect of its chain: for the tests that go through every property there is.
pub fn properties_of_every_kind() -> Vec<PropertyId> {
    PropertyId::OWN
        .into_iter()
        .chain([HUE_SHIFT_AS_THE_FIRST_EFFECT])
        .collect()
}

/// Puts the hue shift effect on `target` (a clip or a track), turned by `degrees` for what it is when it is not animated, and gives the name
/// it has in the chain of `target`. (The effect is another one when `target` has a hue shift already.)
pub fn put_a_hue_shift_on(project: &mut Project, target: AnimationTarget, degrees: f32) -> EffectInstanceId {
    let instance = project
        .add_effect(target, EffectKind::HUE_SHIFT)
        .expect("it is something that can have the effect");
    project
        .set_property_at(
            target,
            PropertyId::effect_value(EffectKind::HUE_SHIFT, instance, 0),
            PropertyValue::Number(degrees),
            TimelineFrameIndex::new(0),
        )
        .expect("the effect was just put on");
    instance
}

/// Puts an effect of `kind` on `target` (a clip or a track), with `values` for what its values are when they are not animated (one for each of the values the effect has,
/// in their order), and gives the name it has in the chain of `target`.
pub fn put_an_effect_on(
    project: &mut Project,
    target: AnimationTarget,
    kind: EffectKind,
    values: &[PropertyValue],
) -> EffectInstanceId {
    let instance = project
        .add_effect(target, kind)
        .expect("it is something that can have the effect");
    for (slot, value) in values.iter().enumerate() {
        project
            .set_property_at(
                target,
                PropertyId::effect_value(kind, instance, u8::try_from(slot).expect("few values")),
                value.clone(),
                TimelineFrameIndex::new(0),
            )
            .expect("the effect was just put on");
    }
    instance
}

/// Puts a keyframe at `timeline_frame` that makes `property` of `target` have `value` there. (The property is animated from
/// then on, if it was not.)
pub fn make_a_keyframe(
    project: &mut Project,
    target: AnimationTarget,
    property: PropertyId,
    timeline_frame: u64,
    value: PropertyValue,
) {
    let frame = TimelineFrameIndex::new(timeline_frame);
    project
        .add_keyframe(target, property, frame)
        .expect("the frame is in what the keyframe is for");
    project
        .set_property_at(target, property, value, frame)
        .expect("a keyframe was just made there");
}

/// Puts a keyframe crate on `track` that lasts from `first_frame` of the timeline to `last_frame` (both are in it), with a rack in it of
/// `property` of `target`, which has a keyframe at each of those frames (with the value the property has there). Gives the crate.
///
/// (A crate with nothing in it does not stay, and one lasts exactly from its first keyframe to its last: this makes one as the editor does,
/// by taking a rack out of a drawer onto the track. The `target` has to be one that can have keyframes at both frames.)
pub fn make_a_crate(
    project: &mut Project,
    track: TrackId,
    first_frame: u64,
    last_frame: u64,
    target: AnimationTarget,
    property: PropertyId,
) -> ClipId {
    for frame in [first_frame, last_frame] {
        project
            .add_keyframe(target, property, TimelineFrameIndex::new(frame))
            .expect("the frame is in what the keyframe is for");
    }
    let rack = project
        .located_racks()
        .find(|located| {
            located.owner == target && located.rack.target() == target && located.rack.property() == property
        })
        .map(|located| located.rack.id())
        .expect("the keyframes were just put in a rack of the drawer of the target");
    project
        .move_racks(
            &[rack],
            RackDestination::NewCrate {
                track,
                from_frame: first_frame,
                until_frame: last_frame + 1,
            },
            0,
            None,
        )
        .expect("the track is empty from the first frame to the last");
    project
        .track(track)
        .expect("the track is there")
        .placements()
        .find(|placement| placement.first_timeline_frame.value() == first_frame)
        .expect("the crate begins at the first frame")
        .id
}

/// How many racks the crate of [`project_with_a_crate_that_has_a_lot_of_racks`] has.
pub const RACKS_IN_THE_CRATE: usize = 7;

/// A project with a track that has a text object, and in front of it a track that has a keyframe crate with seven racks in it
/// (which do not fit in it, on tracks of the height that a new one has). Gives the project and the track of the crate.
pub fn project_with_a_crate_that_has_a_lot_of_racks() -> (Project, TrackId) {
    use crate::project::TextObject;

    let mut project = Project::empty();
    let text_track = project.tracks()[0].id();
    let (text, _) = project
        .insert_text(text_track, TimelineFrameIndex::new(0), TextObject::default(), 90)
        .unwrap();
    let crate_track = project.add_track_in_front();
    let properties_of_a_text = [
        PropertyId::Position,
        PropertyId::Size,
        PropertyId::Rotation,
        TEXT_OF_A_TEXT,
        FONT_OF_A_TEXT,
        SIZE_OF_A_TEXT,
        COLOUR_OF_A_TEXT,
    ];
    assert_eq!(properties_of_a_text.len(), RACKS_IN_THE_CRATE);
    // (A crate lasts from its first keyframe to its last: the first rack makes it, from the first frame of the text to the last.)
    let crate_clip = make_a_crate(
        &mut project,
        crate_track,
        0,
        89,
        AnimationTarget::Clip(text),
        properties_of_a_text[0],
    );
    for property in &properties_of_a_text[1..] {
        let property = *property;
        let made = project
            .add_keyframe(AnimationTarget::Clip(text), property, TimelineFrameIndex::new(10))
            .unwrap()
            .unwrap();
        project
            .move_racks(
                &[made.rack],
                RackDestination::Drawer(AnimationTarget::Clip(crate_clip)),
                0,
                None,
            )
            .unwrap();
    }
    (project, crate_track)
}

/// Runs `check` on every one of `cases` on several threads at once (as many as the computer has, eight at the most) and gives what each
/// check gave, in the order of the cases.
///
/// For a test that tries many cases that have nothing to do with one another: it takes as long as the slowest of its threads and not as long
/// as all the cases together, and what it tries is what it was. (The suite of tests runs many tests at once, which keeps every thread of the
/// computer busy for most of the time it takes; but a test of half a minute is still going on after all the others are done, with all but
/// one of the threads of the computer idle, and that is what this is for.) A check that panics (an assertion that does not hold) stops the
/// others from taking more cases, and the panic comes out of this function with its own message.
pub fn check_every_case_at_once<Case: Sync, Output: Send>(
    cases: &[Case],
    check: impl Fn(&Case) -> Output + Sync,
) -> Vec<Output> {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(8)
        .min(cases.len().max(1));
    let next_case = AtomicUsize::new(0);
    let a_check_panicked = AtomicBool::new(false);
    let outputs: Mutex<Vec<Option<Output>>> = Mutex::new((0..cases.len()).map(|_| None).collect());
    let panics: Vec<Box<dyn std::any::Any + Send>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    while !a_check_panicked.load(Ordering::Relaxed) {
                        let index = next_case.fetch_add(1, Ordering::Relaxed);
                        let case = cases.get(index)?;
                        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(case))) {
                            Ok(output) => outputs.lock().expect("no check holds the lock")[index] = Some(output),
                            Err(panic) => {
                                a_check_panicked.store(true, Ordering::Relaxed);
                                return Some(panic);
                            }
                        }
                    }
                    None
                })
            })
            .collect();
        handles
            .into_iter()
            .filter_map(|handle| {
                handle
                    .join()
                    .expect("a thread that catches the panics of its checks does not panic")
            })
            .collect()
    });
    if let Some(panic) = panics.into_iter().next() {
        std::panic::resume_unwind(panic);
    }
    outputs
        .into_inner()
        .expect("no check holds the lock")
        .into_iter()
        .map(|output| output.expect("every case was checked"))
        .collect()
}

/// A tiny deterministic random number generator, so that randomised tests are reproducible
/// without adding a dependency. (Marsaglia's xorshift64.)
pub struct DeterministicRandomNumbers(u64);

impl DeterministicRandomNumbers {
    /// Different seeds give different sequences; any seed is fine (zero is replaced).
    pub fn with_seed(seed: u64) -> Self {
        let scrambled = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
        Self(if scrambled == 0 { 1 } else { scrambled })
    }

    /// A number from 0 up to but not including `upper_bound`.
    pub fn next_below(&mut self, upper_bound: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % upper_bound
    }
}

/// The list of the plugins that are installed (see `project::set_installed_plugins`; the shaders that are installed, `project::set_installed_shaders`, are kept with it: the plugin manager
/// finds both) is a list for the whole program, and the tests run at the same time. **Tests that put
/// plugins in it hold [`the_installed_plugins_to_myself`], tests whose menus or rules would be different with plugins in it hold [`the_installed_plugins_in_peace`]:** any
/// number of the second kind run together, and never with the first. (A CI run found the lack of this: a test that says a sound has no plus button, which is true only with
/// no sound plugin installed, ran while another test had a plugin installed.)
static THE_INSTALLED_PLUGINS: std::sync::RwLock<()> = std::sync::RwLock::new(());

thread_local! {
    /// Whether this thread holds the list to itself, so that the windows it opens do not wait for themselves.
    static HOLDS_THE_LIST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether this thread holds the list of installed plugins to itself (a test that changes the list must: see [`the_installed_plugins_to_myself`]).
pub fn this_thread_holds_the_list_of_installed_plugins() -> bool {
    HOLDS_THE_LIST.with(std::cell::Cell::get)
}

/// What [`the_installed_plugins_to_myself`] gives: the list is empty while it is held, and is emptied again when it is let go of.
pub struct TheInstalledPluginsToMyself {
    _guard: std::sync::RwLockWriteGuard<'static, ()>,
}

impl Drop for TheInstalledPluginsToMyself {
    fn drop(&mut self) {
        crate::project::set_installed_plugins(Vec::new());
        crate::project::set_installed_shaders(Vec::new());
        HOLDS_THE_LIST.with(|holds| holds.set(false));
    }
}

/// What [`the_installed_plugins_in_peace`] gives (nothing, where the thread holds the list itself).
pub struct TheInstalledPluginsInPeace {
    _guard: Option<std::sync::RwLockReadGuard<'static, ()>>,
}

/// Held by every test that puts plugins in the list of the installed ones: the list is emptied when it is taken and when it is let go of, and no other test sees it in
/// between.
pub fn the_installed_plugins_to_myself() -> TheInstalledPluginsToMyself {
    let guard = THE_INSTALLED_PLUGINS
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    HOLDS_THE_LIST.with(|holds| holds.set(true));
    crate::project::set_installed_plugins(Vec::new());
    crate::project::set_installed_shaders(Vec::new());
    TheInstalledPluginsToMyself { _guard: guard }
}

/// Held by every test whose menus or rules would be different if a plugin were installed (a sound has an effect to be given only then): no test that installs one runs while
/// it is held, and the list is empty.
pub fn the_installed_plugins_in_peace() -> TheInstalledPluginsInPeace {
    if HOLDS_THE_LIST.with(std::cell::Cell::get) {
        return TheInstalledPluginsInPeace { _guard: None };
    }
    TheInstalledPluginsInPeace {
        _guard: Some(
            THE_INSTALLED_PLUGINS
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        ),
    }
}

/// The plugins of the library written for the tests (`crates/ladspa-test-plugins`) that can be effects, as the scan makes them (the library is scanned in this process, which
/// is what the scan program is for with libraries that are not known to be good: this one is). They are not installed: see `project::set_installed_plugins`.
pub fn the_plugins_of_the_test_library() -> Vec<crate::project::InstalledPlugin> {
    static SCANNED: std::sync::OnceLock<Vec<crate::project::InstalledPlugin>> = std::sync::OnceLock::new();
    SCANNED
        .get_or_init(|| {
            let path = ladspa_test_plugins::library_file_for_tests();
            let scan = ladspa_host::scan_library(&path, |_| {}).expect("the test library is scanned");
            scan.plugins
                .iter()
                .filter_map(|plugin| crate::plugin_scan::effect_of_a_plugin(&path, plugin).ok())
                .collect()
        })
        .clone()
}

/// A value of a sound plugin for [`an_installed_sound_plugin`]: its id, what it begins as, and its range if it has one.
pub type ValueOfAPlugin<'a> = (&'a str, f32, Option<(f32, f32)>);

/// A sound plugin that is installed, as the scan makes one (see `plugin_scan::effects`): the key `key`, the version 1.0, a sound effect in the category `category` with these
/// values, each a number that begins as the one given and has the range given, if one is; one audio input and one output; the library `library`.
pub fn an_installed_sound_plugin(
    key: &str,
    category: &str,
    values: &[ValueOfAPlugin<'_>],
) -> crate::project::InstalledPlugin {
    let mut descriptor = the_descriptor_of_an_effect_the_program_does_not_have(
        key,
        &values
            .iter()
            .map(|(id, default, _)| (*id, PropertyValue::Number(*default)))
            .collect::<Vec<_>>(),
    );
    descriptor.domain = EffectDomain::Sound;
    descriptor.changes_each_pixel_by_itself = false;
    descriptor.category = Cow::Owned(category.to_owned());
    for (parameter, (_, _, range)) in descriptor.parameters.to_mut().iter_mut().zip(values) {
        parameter.range = *range;
    }
    crate::project::InstalledPlugin {
        descriptor,
        library: std::path::PathBuf::from(format!("/plugins/{}.so", key.replace(':', "_"))),
        address: crate::project::PluginAddress::Ladspa {
            index: 0,
            unique_id: 900,
        },
        audio_inputs: 1,
        audio_outputs: 1,
        value_handles: (0..values.len() as u32).collect(),
        pre_roll_samples: 0,
        start_warning: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_test_in_peace_never_sees_a_plugin_that_a_test_installed_not_while_it_runs_and_not_after() {
        let (tell_main, installed) = std::sync::mpsc::channel();
        let installer = std::thread::spawn(move || {
            let _alone = the_installed_plugins_to_myself();
            crate::project::set_installed_plugins(vec![an_installed_sound_plugin("ladspa:peace:peace", "Peace", &[])]);
            tell_main.send(()).unwrap();
            // (Long enough for the other thread to be waiting.)
            std::thread::sleep(std::time::Duration::from_millis(200));
            assert_eq!(
                crate::project::EffectKind::installed_plugin_kinds().len(),
                1,
                "it still has it"
            );
        });
        installed.recv().unwrap();

        let _in_peace = the_installed_plugins_in_peace();

        assert!(
            crate::project::EffectKind::installed_plugin_kinds().is_empty(),
            "the plugin was taken out when its test was done, and not seen before"
        );
        installer.join().unwrap();
    }

    #[test]
    fn a_test_that_holds_the_list_can_open_what_wants_it_in_peace_and_many_in_peace_run_together() {
        {
            let _alone = the_installed_plugins_to_myself();
            let _and_in_peace = the_installed_plugins_in_peace();
        }
        let first = the_installed_plugins_in_peace();
        let second = the_installed_plugins_in_peace();
        drop((first, second));
    }

    #[test]
    fn the_same_seed_gives_the_same_sequence_and_different_seeds_differ() {
        let sequence = |seed| {
            let mut random = DeterministicRandomNumbers::with_seed(seed);
            (0..20).map(|_| random.next_below(1000)).collect::<Vec<_>>()
        };
        assert_eq!(sequence(7), sequence(7));
        assert_ne!(sequence(7), sequence(8));
    }

    #[test]
    fn numbers_stay_below_the_bound_and_cover_the_range() {
        let mut random = DeterministicRandomNumbers::with_seed(1);
        let mut seen = [false; 5];
        for _ in 0..200 {
            let number = random.next_below(5);
            assert!(number < 5);
            seen[number as usize] = true;
        }
        assert!(seen.iter().all(|&was_seen| was_seen), "{seen:?}");
    }

    #[test]
    fn a_seed_of_zero_still_works() {
        let mut random = DeterministicRandomNumbers::with_seed(0);
        assert!((0..50).map(|_| random.next_below(1000)).any(|number| number != 0));
    }

    #[test]
    fn every_case_is_checked_once_and_what_the_checks_gave_is_in_the_order_of_the_cases() {
        let checked = std::sync::atomic::AtomicUsize::new(0);
        let cases: Vec<u64> = (0..500).collect();

        let outputs = check_every_case_at_once(&cases, |case| {
            checked.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            case * 2
        });

        assert_eq!(checked.load(std::sync::atomic::Ordering::Relaxed), 500);
        assert_eq!(outputs, (0..500).map(|case| case * 2).collect::<Vec<u64>>());
    }

    #[test]
    fn no_cases_give_nothing() {
        let outputs: Vec<u8> = check_every_case_at_once(&Vec::<u8>::new(), |case| *case);
        assert!(outputs.is_empty());
    }

    #[test]
    #[should_panic(expected = "case 37 is not what it should be")]
    fn a_check_that_panics_makes_the_whole_panic_with_its_own_message() {
        let cases: Vec<u64> = (0..100).collect();
        check_every_case_at_once(&cases, |case| {
            assert_ne!(*case, 37, "case {case} is not what it should be");
        });
    }

    #[test]
    fn the_cases_are_checked_on_more_than_one_thread_where_the_computer_has_more_than_one() {
        if std::thread::available_parallelism().map_or(1, std::num::NonZero::get) < 2 {
            return;
        }
        let cases: Vec<u8> = vec![0; 32];
        let threads_used = check_every_case_at_once(&cases, |_| {
            // (Long enough that the thread that has taken a case has not taken them all by the time the others have begun.)
            std::thread::sleep(std::time::Duration::from_millis(20));
            std::thread::current().id()
        });
        let distinct: std::collections::HashSet<_> = threads_used.into_iter().collect();
        assert!(
            distinct.len() >= 2,
            "{} thread(s) checked 32 cases that each took 20 ms",
            distinct.len()
        );
    }
}
