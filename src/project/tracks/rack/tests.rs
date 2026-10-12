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
    assert_eq!(first.rack(), rack_id(0));
    assert_eq!(rack.keyframe(first).unwrap().frame, 50);
    assert_eq!(rack.keyframe(second).unwrap().frame, 10);
    assert_eq!(rack.id_of(rack.at(50).unwrap()), first);
}

#[test]
fn setting_a_keyframe_where_there_is_one_changes_its_value_and_keeps_its_name() {
    let mut rack = rack_of(&[(10, 1.0)]);
    let before = rack.id_of(rack.at(10).unwrap());
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
    let name_of_the_middle = rack.id_of(rack.at(20).unwrap());
    let removed = rack.remove(name_of_the_middle).unwrap();
    assert_eq!((removed.frame, removed.value), (20, db(2.0)));
    assert_eq!(rack.remove(name_of_the_middle), None, "it is gone");
    assert_eq!(rack.remove_at(10).unwrap().value, db(1.0));
    assert_eq!(rack.remove_at(10), None);
    assert_eq!(frames_of(&rack), vec![30]);
}

#[test]
fn a_keyframe_of_another_rack_is_not_found_here() {
    let mut rack = rack_of(&[(10, 1.0)]);
    let number = rack.at(10).unwrap().number();
    let in_another_rack = KeyframeId::new(rack_id(9), number);
    assert_eq!(rack.keyframe(in_another_rack), None);
    assert_eq!(rack.remove(in_another_rack), None);
    assert_eq!(rack.len(), 1);
}

#[test]
fn the_curve_of_a_rack_is_its_keyframes_without_their_names() {
    let rack = rack_of(&[(10, 1.0), (20, 3.0)]);
    let curve = rack.curve();
    assert_eq!(curve.len(), 2);
    assert_eq!(curve.value_at(15), Some(db(2.0)));
    assert_eq!(curve.value_at(0), Some(db(1.0)));
}

// --- moving keyframes ---

#[test]
fn keyframes_are_moved_to_the_frames_given_for_them_and_keep_their_names() {
    let mut rack = rack_of(&[(10, 1.0), (20, 2.0), (30, 3.0)]);
    let ids: Vec<KeyframeId> = rack.keyframes().iter().map(|keyframe| rack.id_of(keyframe)).collect();

    rack.move_keyframes(&[(ids[0], 12), (ids[1], 22)]).unwrap();

    assert_eq!(frames_of(&rack), vec![12, 22, 30]);
    assert_eq!(rack.keyframe(ids[0]).unwrap().frame, 12);
    assert_eq!(rack.keyframe(ids[1]).unwrap().value, db(2.0), "still its own value");
}

#[test]
fn a_keyframe_moved_past_another_goes_after_it_and_the_rack_is_in_order_again() {
    let mut rack = rack_of(&[(10, 1.0), (20, 2.0)]);
    let first = rack.id_of(rack.at(10).unwrap());
    rack.move_keyframes(&[(first, 25)]).unwrap();
    assert_eq!(frames_of(&rack), vec![20, 25]);
    assert_eq!(decibels_at(&rack, 25), 1.0);
}

#[test]
fn keyframes_that_would_be_at_one_frame_are_not_moved_and_nothing_is_changed() {
    let mut rack = rack_of(&[(10, 1.0), (20, 2.0), (30, 3.0)]);
    let before = rack.clone();
    let first = rack.id_of(rack.at(10).unwrap());
    let second = rack.id_of(rack.at(20).unwrap());

    assert_eq!(
        rack.move_keyframes(&[(first, 30)]),
        Err(RackEditError::FramesWouldCoincide),
        "on one that stays"
    );
    assert_eq!(
        rack.move_keyframes(&[(first, 40), (second, 40)]),
        Err(RackEditError::FramesWouldCoincide),
        "on one that moves"
    );
    assert_eq!(rack, before);
}

#[test]
fn keyframes_may_swap_places_because_only_where_they_end_up_counts() {
    let mut rack = rack_of(&[(10, 1.0), (20, 2.0)]);
    let first = rack.id_of(rack.at(10).unwrap());
    let second = rack.id_of(rack.at(20).unwrap());
    rack.move_keyframes(&[(first, 20), (second, 10)]).unwrap();
    assert_eq!(decibels_at(&rack, 10), 2.0);
    assert_eq!(decibels_at(&rack, 20), 1.0);
}

#[test]
fn a_keyframe_that_the_rack_does_not_have_stops_the_whole_move() {
    let mut rack = rack_of(&[(10, 1.0), (20, 2.0)]);
    let before = rack.clone();
    let first = rack.id_of(rack.at(10).unwrap());
    let nobody = KeyframeId::new(rack_id(0), 99);
    assert_eq!(
        rack.move_keyframes(&[(first, 11), (nobody, 12)]),
        Err(RackEditError::NoSuchKeyframe)
    );
    assert_eq!(rack, before);
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
fn a_rack_shortened_keeps_the_keyframes_it_still_has_a_frame_for_and_one_at_its_end_if_it_is_given_a_value() {
    let rack = rack_of(&[(5, 1.0), (20, 2.0), (30, 3.0)]);
    assert_eq!(frames_of(&rack.truncated_to(25, None)), vec![5, 20]);
    let with_the_end = rack.truncated_to(25, Some(db(2.5)));
    assert_eq!(frames_of(&with_the_end), vec![5, 20, 24]);
    assert_eq!(decibels_at(&with_the_end, 24), 2.5);
    assert!(
        rack.truncated_to(0, Some(db(1.0))).is_empty(),
        "no frames, no keyframes"
    );
    assert_eq!(
        rack.truncated_to(100, None),
        rack,
        "longer than it needs to be changes nothing"
    );
}

#[test]
fn a_rack_stretched_has_each_keyframe_as_far_through_as_it_was_and_those_that_land_together_are_one() {
    let rack = rack_of(&[(0, 1.0), (50, 2.0), (99, 3.0)]);
    let doubled = rack.rescaled(100, 200);
    assert_eq!(frames_of(&doubled), vec![0, 100, 198]);
    let squeezed = rack_of(&[(10, 1.0), (11, 2.0), (12, 3.0)]).rescaled(100, 10);
    assert_eq!(frames_of(&squeezed), vec![1], "the later is kept");
    assert_eq!(decibels_at(&squeezed, 1), 3.0);
    assert!(rack.rescaled(100, 0).is_empty());
    assert_eq!(
        rack.rescaled(0, 50),
        rack,
        "a rack of something that had no length is not stretched"
    );
}

#[test]
fn nothing_of_a_rack_is_kept_at_or_past_the_frame_the_end_of_what_holds_it_is() {
    let rack = rack_of(&[(0, 1.0), (99, 3.0)]);
    let squeezed = rack.rescaled(100, 10);
    assert_eq!(frames_of(&squeezed), vec![0, 9]);
}

#[test]
fn the_frames_of_a_rack_can_be_counted_again_and_those_that_land_together_are_one_the_later_kept() {
    let rack = rack_of(&[(10, 1.0), (20, 2.0), (30, 3.0)]);
    assert_eq!(frames_of(&rack.with_frames_mapped(|frame| frame * 2)), vec![20, 40, 60]);
    let together = rack.with_frames_mapped(|_| 7);
    assert_eq!(frames_of(&together), vec![7]);
    assert_eq!(decibels_at(&together, 7), 3.0);
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
    let name_in_the_original = rack.id_of(rack.at(10).unwrap());
    let name_in_the_copy = copy.id_of(copy.at(10).unwrap());
    assert_ne!(
        name_in_the_original, name_in_the_copy,
        "they are told apart by the rack they are in"
    );
    assert_eq!(rack.clone().retargeted(other).target(), other);
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

/// What a rack does when what holds it is stretched, shortened or cut is what the keyframes of a property do (see
/// [`Keyframes`]): they are the same arithmetic written twice, one for the keyframes that are worked out from and one for the
/// keyframes that are kept and moved, and this is what keeps them from drifting apart.
#[test]
fn stretching_shortening_and_cutting_a_rack_do_to_its_curve_what_they_do_to_the_keyframes_of_a_property() {
    for seed in 1..=300u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed);
        let rack = random_rack(&mut random, 200);
        let curve = rack.curve();

        let from_length = 1 + random.next_below(300);
        let to_length = random.next_below(300);
        assert_eq!(
            rack.rescaled(from_length, to_length).curve(),
            curve.rescaled(from_length, to_length),
            "seed {seed}: {from_length} frames made {to_length}"
        );

        let length = random.next_below(250);
        let curve_beyond_the_end = curve.iter().any(|keyframe| keyframe.frame >= length);
        let value_at_the_end = length
            .checked_sub(1)
            .and_then(|last| curve.value_at(last))
            .filter(|_| curve_beyond_the_end && length > 0);
        assert_eq!(
            rack.truncated_to(length, value_at_the_end).curve(),
            curve.truncated_to(length),
            "seed {seed}: cut off at {length}"
        );

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
