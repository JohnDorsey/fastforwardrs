// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Helpers shared by the unit tests in several modules.

use crate::project::{
    AnimationTarget, ClipId, EffectKind, Project, PropertyId, PropertyValue, RackDestination, TimelineFrameIndex,
    TrackId,
};

/// Puts the hue shift effect on `target` (a clip or a track), turned by `degrees` for what it is when it is not animated.
pub fn put_a_hue_shift_on(project: &mut Project, target: AnimationTarget, degrees: f32) {
    project
        .add_effect(target, EffectKind::HueShift)
        .expect("it is something that can have the effect");
    project
        .set_property_at(
            target,
            PropertyId::HueShift,
            PropertyValue::Number(degrees),
            TimelineFrameIndex::new(0),
        )
        .expect("the effect was just put on");
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
        PropertyId::Text,
        PropertyId::FontFamily,
        PropertyId::FontSize,
        PropertyId::Colour,
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

#[cfg(test)]
mod tests {
    use super::*;

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
