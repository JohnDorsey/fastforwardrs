// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::project::tracks::ids::{ClipId, TrackId};
use crate::test_support::DeterministicRandomNumbers;

fn track() -> TrackId {
    TrackId::new(0)
}

fn rack_id(number: u32) -> RackId {
    RackId::new(track(), number)
}

fn the_clip() -> AnimationTarget {
    AnimationTarget::Clip(ClipId::new(track(), 0))
}

fn db(decibels: f32) -> PropertyValue {
    PropertyValue::Number(decibels)
}

/// A rack of the loudness of a clip with these keyframes.
fn rack_of(frames_and_decibels: &[(u64, f32)]) -> Rack {
    let mut rack = Rack::new(rack_id(0), the_clip(), PropertyId::Gain);
    for (frame, decibels) in frames_and_decibels {
        rack.set(*frame, db(*decibels)).unwrap();
    }
    rack
}

fn frames_of(rack: &Rack) -> Vec<u64> {
    rack.keyframes().iter().map(|keyframe| keyframe.frame).collect()
}

fn decibels_at(rack: &Rack, frame: u64) -> f32 {
    match rack.at(frame).map(|keyframe| &keyframe.value) {
        Some(PropertyValue::Number(decibels)) => *decibels,
        other => panic!("no number at {frame}: {other:?}"),
    }
}

// --- making and unmaking keyframes ---

#[test]
fn a_new_rack_has_no_keyframes_and_says_what_it_is_for() {
    let rack = Rack::new(rack_id(3), the_clip(), PropertyId::Gain);
    assert!(rack.is_empty());
    assert_eq!(rack.len(), 0);
    assert_eq!((rack.first_frame(), rack.last_frame()), (None, None));
    assert_eq!(rack.id(), rack_id(3));
    assert_eq!(rack.target(), the_clip());
    assert_eq!(rack.property(), PropertyId::Gain);
}

#[test]
fn keyframes_are_kept_in_the_order_of_their_frames_whatever_order_they_are_made_in() {
    let rack = rack_of(&[(30, -3.0), (10, 1.0), (20, 2.0)]);
    assert_eq!(frames_of(&rack), vec![10, 20, 30]);
    assert_eq!((rack.first_frame(), rack.last_frame()), (Some(10), Some(30)));
    assert_eq!(rack.len(), 3);
}

#[test]
fn each_keyframe_has_a_name_of_its_own_that_is_not_its_frame() {
    let mut rack = Rack::new(rack_id(0), the_clip(), PropertyId::Gain);
    let first = rack.set(50, db(0.0)).unwrap();
    let second = rack.set(10, db(1.0)).unwrap();
    assert_ne!(first, second);
    assert_eq!(first.origin(), rack_id(0), "the rack it was made in");
    assert_eq!(rack.keyframe(first).unwrap().frame, 50);
    assert_eq!(rack.keyframe(second).unwrap().frame, 10);
    assert_eq!(rack.at(50).unwrap().id(), first);
}

#[test]
fn setting_a_keyframe_where_there_is_one_changes_its_value_and_keeps_its_name() {
    let mut rack = rack_of(&[(10, 1.0)]);
    let before = rack.at(10).unwrap().id();
    let after = rack.set(10, db(-9.0)).unwrap();
    assert_eq!(before, after);
    assert_eq!(rack.len(), 1);
    assert_eq!(decibels_at(&rack, 10), -9.0);
}

#[test]
fn a_value_of_the_wrong_kind_is_refused_and_changes_nothing() {
    let mut rack = rack_of(&[(10, 1.0)]);
    let before = rack.clone();
    assert_eq!(
        rack.set(20, PropertyValue::Flag(true)),
        Err(RackEditError::NotTheKindOfValueThePropertyHas)
    );
    assert_eq!(
        rack.set(10, PropertyValue::Pair(0.5, 0.5)),
        Err(RackEditError::NotTheKindOfValueThePropertyHas)
    );
    assert_eq!(rack, before);
}

#[test]
fn a_keyframe_can_be_taken_away_by_its_frame_or_by_its_name_and_not_if_it_is_not_there() {
    let mut rack = rack_of(&[(10, 1.0), (20, 2.0), (30, 3.0)]);
    let name_of_the_middle = rack.at(20).unwrap().id();
    let removed = rack.remove(name_of_the_middle).unwrap();
    assert_eq!((removed.frame, removed.value), (20, db(2.0)));
    assert_eq!(rack.remove(name_of_the_middle), None, "it is gone");
    assert_eq!(rack.remove_at(10).unwrap().value, db(1.0));
    assert_eq!(rack.remove_at(10), None);
    assert_eq!(frames_of(&rack), vec![30]);
}

#[test]
fn a_keyframe_that_is_not_in_the_rack_is_not_found_in_it_and_one_made_in_another_rack_is_another_keyframe() {
    let mut rack = rack_of(&[(10, 1.0)]);
    let number = rack.at(10).unwrap().id().number();
    let made_in_another_rack = KeyframeId::new(rack_id(9), number);
    assert_eq!(rack.keyframe(made_in_another_rack), None);
    assert_eq!(rack.remove(made_in_another_rack), None);
    assert_eq!(rack.len(), 1);
}

#[test]
fn a_keyframe_made_in_another_rack_is_put_in_with_its_name_and_a_keyframe_made_after_that_has_a_name_of_its_own() {
    let mut other = Rack::new(rack_id(5), the_clip(), PropertyId::Gain);
    let travelling_name = other.set(40, db(7.0)).unwrap();
    let travelling = other.remove(travelling_name).unwrap();
    let mut rack = rack_of(&[(10, 1.0)]);

    rack.put_in(travelling).unwrap();

    assert_eq!(frames_of(&rack), vec![10, 40]);
    assert_eq!(
        rack.keyframe(travelling_name).unwrap().value,
        db(7.0),
        "found by the name it had"
    );
    assert_eq!(
        travelling_name.origin(),
        rack_id(5),
        "it was made in the other rack still"
    );
    let made_here = rack.set(50, db(0.0)).unwrap();
    assert_eq!(made_here.origin(), rack_id(0));
    assert_ne!(made_here, travelling_name);
    assert_eq!(rack.len(), 3);
}

#[test]
fn a_keyframe_is_not_put_in_a_rack_that_has_it_or_has_one_at_its_frame_or_has_another_kind_of_value() {
    let mut rack = rack_of(&[(10, 1.0), (20, 2.0)]);
    let before = rack.clone();
    let mut other = Rack::new(rack_id(5), the_clip(), PropertyId::Gain);
    let id = other.set(20, db(7.0)).unwrap();
    let at_a_frame_that_is_taken = other.remove(id).unwrap();
    assert_eq!(
        rack.put_in(at_a_frame_that_is_taken),
        Err(RackEditError::FramesWouldCoincide)
    );

    let own = rack.at(10).unwrap().clone();
    assert_eq!(rack.put_in(own), Err(RackEditError::AlreadyHasThatKeyframe));

    let mut of_another_property = Rack::new(rack_id(6), the_clip(), PropertyId::Muted);
    let flag = of_another_property.set(30, PropertyValue::Flag(true)).unwrap();
    let wrong_kind = of_another_property.remove(flag).unwrap();
    assert_eq!(
        rack.put_in(wrong_kind),
        Err(RackEditError::NotTheKindOfValueThePropertyHas)
    );
    assert_eq!(rack, before, "nothing was changed");
}

#[test]
fn the_curve_of_a_rack_is_its_keyframes_without_their_names() {
    let rack = rack_of(&[(10, 1.0), (20, 3.0)]);
    let curve = rack.curve();
    assert_eq!(curve.len(), 2);
    assert_eq!(curve.value_at(15), Some(db(2.0)));
    assert_eq!(curve.value_at(0), Some(db(1.0)));
}

// --- cutting, shortening, stretching ---

#[test]
fn cutting_a_rack_gives_the_keyframes_before_the_cut_to_the_first_and_those_from_it_on_to_the_second_counted_from_the_cut()
 {
    let rack = rack_of(&[(5, 1.0), (20, 2.0), (30, 3.0)]);
    let second_id = rack_id(7);
    let other_target = AnimationTarget::Clip(ClipId::new(track(), 5));

    let (first, second) = rack.split_at(20, None, None, (second_id, other_target));

    assert_eq!(frames_of(&first), vec![5]);
    assert_eq!(frames_of(&second), vec![0, 10]);
    assert_eq!((first.id(), first.target()), (rack_id(0), the_clip()));
    assert_eq!((second.id(), second.target()), (second_id, other_target));
    assert_eq!(decibels_at(&second, 0), 2.0);
}

#[test]
fn the_keyframes_of_the_two_parts_of_a_cut_rack_keep_the_names_they_had_and_those_made_by_the_cut_have_new_ones() {
    let rack = rack_of(&[(5, 1.0), (20, 2.0), (30, 3.0)]);
    let names: Vec<KeyframeId> = rack.keyframes().iter().map(RackKeyframe::id).collect();
    let target = (rack_id(7), the_clip());

    let (first, second) = rack.split_at(10, Some(db(1.5)), Some(db(1.7)), target);

    assert_eq!(first.at(5).unwrap().id(), names[0]);
    assert_eq!(second.at(10).unwrap().id(), names[1], "20 is 10 after the cut");
    assert_eq!(second.at(20).unwrap().id(), names[2]);
    let added_before_the_cut = first.at(9).unwrap().id();
    let added_after_the_cut = second.at(0).unwrap().id();
    assert_eq!(added_before_the_cut.origin(), rack_id(0), "made by the first part");
    assert_eq!(added_after_the_cut.origin(), rack_id(7), "made by the second part");
    for new in [added_before_the_cut, added_after_the_cut] {
        assert!(!names.contains(&new), "{new} is a keyframe that was not there before");
    }
}

#[test]
fn a_rack_with_its_frames_counted_again_keeps_the_keyframes_with_their_names() {
    let rack = rack_of(&[(10, 1.0), (20, 2.0), (30, 3.0)]);
    let names: Vec<KeyframeId> = rack.keyframes().iter().map(RackKeyframe::id).collect();
    let moved = rack.with_frames_mapped(|frame| frame + 5);
    let moved_names: Vec<KeyframeId> = moved.keyframes().iter().map(RackKeyframe::id).collect();
    assert_eq!(moved_names, names);
    let together = rack.with_frames_mapped(|frame| frame / 100);
    assert_eq!(
        together.keyframes().iter().map(RackKeyframe::id).collect::<Vec<_>>(),
        names,
        "all of them, each with the name it had, in the order it was in"
    );
}

#[test]
fn a_cut_adds_a_keyframe_at_the_edge_of_a_part_only_when_it_is_asked_to_and_not_where_there_is_one() {
    let rack = rack_of(&[(5, 1.0), (20, 2.0)]);
    let target = (rack_id(1), the_clip());

    let (first, second) = rack.split_at(20, Some(db(1.5)), Some(db(2.0)), target);
    assert_eq!(frames_of(&first), vec![5, 19]);
    assert_eq!(decibels_at(&first, 19), 1.5);
    assert_eq!(
        frames_of(&second),
        vec![0],
        "there was a keyframe at the first frame of the second already"
    );
    assert_eq!(decibels_at(&second, 0), 2.0, "and it is not replaced");

    let (none_added_first, none_added_second) = rack.split_at(10, None, None, target);
    assert_eq!(frames_of(&none_added_first), vec![5]);
    assert_eq!(frames_of(&none_added_second), vec![10]);
}

#[test]
fn nothing_is_added_before_the_first_frame_of_the_rack_by_a_cut_at_its_start() {
    let rack = rack_of(&[(5, 1.0)]);
    let (first, second) = rack.split_at(0, Some(db(9.0)), None, (rack_id(1), the_clip()));
    assert!(first.is_empty(), "no frame before the cut to put it at");
    assert_eq!(frames_of(&second), vec![5]);
}

#[test]
fn a_rack_shortened_keeps_the_keyframes_that_it_still_has_frames_for_where_they_are() {
    let rack = rack_of(&[(5, 1.0), (20, 2.0), (30, 3.0)]);
    assert_eq!(
        rack.truncated_to(31),
        rack,
        "not shortened enough to reach a keyframe changes nothing"
    );
    assert_eq!(rack.truncated_to(100), rack);
    let shortened = rack.truncated_to(25);
    assert_eq!(
        frames_of(&shortened)[..2],
        [5, 20],
        "those before the end are where they were"
    );
}

/// The bug of this round: the keyframes of a clip that is made shorter were cut off, though the clip had room for them.
#[test]
fn a_rack_shortened_brings_the_keyframes_that_are_cut_off_in_to_the_last_frames_and_loses_none_while_there_is_room() {
    let rack = rack_of(&[(5, 1.0), (20, 2.0), (30, 3.0), (40, 4.0)]);
    let names: Vec<KeyframeId> = rack.keyframes().iter().map(RackKeyframe::id).collect();

    let shortened = rack.truncated_to(25);

    assert_eq!(
        frames_of(&shortened),
        vec![5, 20, 23, 24],
        "the two that were cut off are against the end now"
    );
    let names_now: Vec<KeyframeId> = shortened.keyframes().iter().map(RackKeyframe::id).collect();
    assert_eq!(names_now, names, "they are the same keyframes, in the same order");
    assert_eq!(
        (decibels_at(&shortened, 23), decibels_at(&shortened, 24)),
        (3.0, 4.0),
        "with the values they had"
    );
}

#[test]
fn a_rack_made_so_short_that_it_has_more_keyframes_than_frames_loses_the_latest_and_has_a_keyframe_at_every_frame() {
    let rack = rack_of(&[(0, 1.0), (5, 2.0), (9, 3.0), (12, 4.0), (30, 5.0)]);
    let names: Vec<KeyframeId> = rack.keyframes().iter().map(RackKeyframe::id).collect();

    let shortened = rack.truncated_to(3);

    assert_eq!(frames_of(&shortened), vec![0, 1, 2]);
    let names_now: Vec<KeyframeId> = shortened.keyframes().iter().map(RackKeyframe::id).collect();
    assert_eq!(names_now, names[..3], "the first three are kept");
    assert!(rack.truncated_to(0).is_empty(), "no frames, no keyframes");
}

#[test]
fn a_rack_stretched_has_each_keyframe_as_far_through_as_it_was() {
    let rack = rack_of(&[(0, 1.0), (50, 2.0), (99, 3.0)]);
    assert_eq!(frames_of(&rack.rescaled(100, 200)), vec![0, 100, 198]);
    assert_eq!(
        frames_of(&rack.rescaled(100, 50)),
        vec![0, 25, 49],
        "made shorter, the last is at the last frame"
    );
    assert_eq!(rack.rescaled(100, 100), rack);
    assert!(rack.rescaled(100, 0).is_empty());
    assert_eq!(
        rack.rescaled(0, 50),
        rack,
        "a rack of something that had no length is not stretched"
    );
}

/// The bug of this round: keyframes that were near one another were merged into one by a clip being made shorter, though there
/// was room for all of them.
#[test]
fn keyframes_that_would_land_on_one_frame_when_a_rack_is_shortened_are_put_at_frames_of_their_own_beside_it() {
    let rack = rack_of(&[(10, 1.0), (11, 2.0), (12, 3.0)]);
    let names: Vec<KeyframeId> = rack.keyframes().iter().map(RackKeyframe::id).collect();

    let squeezed = rack.rescaled(100, 10); // all three would be at frame 1

    assert_eq!(
        frames_of(&squeezed),
        vec![0, 1, 2],
        "spread out round the frame they wanted"
    );
    assert_eq!(
        squeezed.keyframes().iter().map(RackKeyframe::id).collect::<Vec<_>>(),
        names,
        "none is lost"
    );
    assert_eq!(
        (
            decibels_at(&squeezed, 0),
            decibels_at(&squeezed, 1),
            decibels_at(&squeezed, 2)
        ),
        (1.0, 2.0, 3.0),
        "each with its value"
    );
}

#[test]
fn a_rack_made_much_shorter_keeps_every_keyframe_as_long_as_the_frames_are_as_many_as_the_keyframes() {
    let rack = rack_of(&(0..10).map(|frame| (frame * 3, frame as f32)).collect::<Vec<_>>());
    let squeezed = rack.rescaled(30, 10);
    assert_eq!(squeezed.len(), 10);
    assert_eq!(
        frames_of(&squeezed),
        (0..10).collect::<Vec<u64>>(),
        "every one of the ten frames"
    );
    let one_frame_short = rack.rescaled(30, 9);
    assert_eq!(
        one_frame_short.len(),
        9,
        "and with a frame less, one is lost and the rest have a frame each"
    );
    assert_eq!(frames_of(&one_frame_short), (0..9).collect::<Vec<u64>>());
}

#[test]
fn a_rack_made_so_short_that_the_keyframes_do_not_all_fit_loses_every_so_many_of_them_and_keeps_the_first_and_the_last()
{
    let rack = rack_of(&(0..10).map(|frame| (frame * 10 + 5, frame as f32)).collect::<Vec<_>>());

    let squeezed = rack.rescaled(100, 4);

    assert_eq!(frames_of(&squeezed), vec![0, 1, 2, 3]);
    let values: Vec<f32> = (0..4).map(|frame| decibels_at(&squeezed, frame)).collect();
    assert_eq!(
        values,
        vec![0.0, 3.0, 6.0, 9.0],
        "the first, the last and two between them"
    );
}

#[test]
fn a_rack_made_one_frame_long_keeps_its_first_keyframe_at_the_frame() {
    let rack = rack_of(&[(5, 1.0), (50, 2.0)]);
    let squeezed = rack.rescaled(100, 1);
    assert_eq!(frames_of(&squeezed), vec![0]);
    assert_eq!(decibels_at(&squeezed, 0), 1.0);
}

#[test]
fn nothing_of_a_rack_is_put_at_or_past_the_frame_the_end_of_what_holds_it_is() {
    let rack = rack_of(&[(0, 1.0), (99, 3.0)]);
    let squeezed = rack.rescaled(100, 10);
    assert_eq!(frames_of(&squeezed), vec![0, 9]);
}

#[test]
fn the_frames_of_a_rack_can_be_counted_again_and_those_that_would_land_together_are_put_beside_one_another() {
    let rack = rack_of(&[(10, 1.0), (20, 2.0), (30, 3.0)]);
    assert_eq!(frames_of(&rack.with_frames_mapped(|frame| frame * 2)), vec![20, 40, 60]);
    let together = rack.with_frames_mapped(|_| 7);
    assert_eq!(frames_of(&together), vec![6, 7, 8], "round the frame they all land on");
    assert_eq!(
        (
            decibels_at(&together, 6),
            decibels_at(&together, 7),
            decibels_at(&together, 8)
        ),
        (1.0, 2.0, 3.0),
        "in the order they were in"
    );
    let at_the_start = rack.with_frames_mapped(|_| 0);
    assert_eq!(
        frames_of(&at_the_start),
        vec![0, 1, 2],
        "none goes before the first frame"
    );
}

#[test]
fn a_copy_of_a_rack_has_a_name_and_a_target_of_its_own_and_the_same_keyframes() {
    let rack = rack_of(&[(10, 1.0), (20, 2.0)]);
    let other = AnimationTarget::Clip(ClipId::new(track(), 4));
    let copy = rack.copy_as(rack_id(8), other);
    assert_eq!(
        (copy.id(), copy.target(), copy.property()),
        (rack_id(8), other, PropertyId::Gain)
    );
    assert_eq!(frames_of(&copy), frames_of(&rack));
    assert_eq!(copy.curve(), rack.curve());
    let name_in_the_original = rack.at(10).unwrap().id();
    let name_in_the_copy = copy.at(10).unwrap().id();
    assert_ne!(
        name_in_the_original, name_in_the_copy,
        "the keyframes of the copy are new ones: they are not the keyframes of the original"
    );
    assert_eq!(name_in_the_copy.origin(), rack_id(8), "made in the copy");
}

#[test]
fn a_keyframe_made_in_a_copy_of_a_rack_has_a_name_that_none_of_its_keyframes_has() {
    let rack = rack_of(&[(10, 1.0), (20, 2.0)]);
    let mut copy = rack.copy_as(rack_id(8), the_clip());
    let names_of_the_copied: Vec<KeyframeId> = copy.keyframes().iter().map(RackKeyframe::id).collect();

    let new = copy.set(30, db(3.0)).unwrap();

    assert!(!names_of_the_copied.contains(&new));
    assert_eq!(copy.len(), 3);
}

// --- the same as what the keyframes of a property do ---

/// A rack with some keyframes at random frames below `limit`.
fn random_rack(random: &mut DeterministicRandomNumbers, limit: u64) -> Rack {
    let mut rack = Rack::new(rack_id(0), the_clip(), PropertyId::Gain);
    for _ in 0..1 + random.next_below(6) {
        rack.set(random.next_below(limit), db(random.next_below(400) as f32 / 8.0 - 20.0))
            .unwrap();
    }
    rack
}

/// What a rack does when what holds it is cut is what the keyframes of a property do (see [`Keyframes`]): they are the same
/// arithmetic written twice, one for the keyframes that are worked out from and one for the keyframes that are kept and moved,
/// and this is what keeps them from drifting apart.
#[test]
fn cutting_a_rack_does_to_its_curve_what_cutting_the_keyframes_of_a_property_does() {
    for seed in 1..=300u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed);
        let rack = random_rack(&mut random, 200);
        let curve = rack.curve();

        let frames_in = 1 + random.next_below(250);
        let (first_curve, second_curve) = curve.split_at(frames_in);
        let cut = curve
            .iter()
            .position(|keyframe| keyframe.frame >= frames_in)
            .unwrap_or(curve.len());
        let value_before = (cut < curve.len()).then(|| curve.value_at(frames_in - 1)).flatten();
        let value_after = (cut > 0).then(|| curve.value_at(frames_in)).flatten();
        let (first, second) = rack.split_at(frames_in, value_before, value_after, (rack_id(1), the_clip()));
        assert_eq!(
            first.curve(),
            first_curve,
            "seed {seed}: the first part of a cut at {frames_in}"
        );
        assert_eq!(
            second.curve(),
            second_curve,
            "seed {seed}: the second part of a cut at {frames_in}"
        );
    }
}
