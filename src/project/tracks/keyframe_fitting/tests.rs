use super::WhichAreLost::{SpreadThroughThem, TheLatest};
use super::*;
use crate::test_support::DeterministicRandomNumbers;

/// Where keyframes that want `wanted` are put when none is lost: it says so if one is.
fn put_at(wanted: &[u64], length: Option<u64>) -> Vec<u64> {
    frames_for_keyframes_wanted_at(wanted, length, TheLatest)
        .into_iter()
        .map(|frame| frame.expect("none of them is lost"))
        .collect()
}

/// The places of the keyframes that are not lost.
fn places_kept(put: &[Option<u64>]) -> Vec<usize> {
    put.iter()
        .enumerate()
        .filter_map(|(place, frame)| frame.map(|_| place))
        .collect()
}

// --- keyframes that are not in each other's way ---

#[test]
fn keyframes_that_want_frames_of_their_own_within_the_length_are_put_exactly_there() {
    assert_eq!(put_at(&[0, 5, 6, 40], Some(41)), vec![0, 5, 6, 40]);
    assert_eq!(put_at(&[0, 5, 6, 40], None), vec![0, 5, 6, 40]);
    assert_eq!(put_at(&[7], Some(8)), vec![7]);
    assert_eq!(put_at(&[], Some(8)), Vec::<u64>::new());
    assert_eq!(put_at(&[], None), Vec::<u64>::new());
}

// --- a crowd ---

#[test]
fn keyframes_that_want_one_frame_are_spread_out_round_it() {
    assert_eq!(put_at(&[3, 3, 3, 3], Some(10)), vec![1, 2, 3, 4]);
    assert_eq!(put_at(&[5, 5, 5], Some(10)), vec![4, 5, 6]);
    assert_eq!(
        put_at(&[5, 5, 5], None),
        vec![4, 5, 6],
        "a room with no end is the same"
    );
}

#[test]
fn two_that_want_one_frame_are_one_either_side_of_it_and_the_earlier_gets_the_frame_before_it_if_there_is_a_choice() {
    // (Half a frame has to go one way or the other, and it is always the same way.)
    assert_eq!(put_at(&[3, 3], Some(10)), vec![2, 3]);
    assert_eq!(put_at(&[8, 8], Some(10)), vec![7, 8]);
}

#[test]
fn a_crowd_at_the_start_goes_up_from_it_and_a_crowd_at_the_end_comes_back_from_it() {
    assert_eq!(put_at(&[0, 0, 0], Some(10)), vec![0, 1, 2]);
    assert_eq!(
        put_at(&[0, 0, 0], None),
        vec![0, 1, 2],
        "there is no frame before the first"
    );
    assert_eq!(put_at(&[9, 9, 9], Some(10)), vec![7, 8, 9]);
}

#[test]
fn keyframes_that_want_frames_past_the_end_are_put_at_the_last_frames_in_their_order() {
    assert_eq!(put_at(&[20, 30, 40], Some(10)), vec![7, 8, 9]);
    assert_eq!(
        put_at(&[2, 30, 40], Some(10)),
        vec![2, 8, 9],
        "those that are within the length stay"
    );
    assert_eq!(put_at(&[2, 9, 10], Some(10)), vec![2, 8, 9]);
}

#[test]
fn a_crowd_moves_the_keyframes_next_to_it_only_as_much_as_it_needs_and_not_the_others() {
    // The two that want 3 have the one that wants 4 beside them, and the one that wants 10 is not in the way of anything.
    assert_eq!(put_at(&[3, 3, 4, 10], Some(20)), vec![2, 3, 4, 10]);
    // Two crowds, far apart, are each spread out by themselves.
    assert_eq!(put_at(&[3, 3, 3, 15, 15, 15], Some(30)), vec![2, 3, 4, 14, 15, 16]);
}

#[test]
fn keyframes_that_are_not_in_a_crowd_are_not_moved_when_the_crowd_can_be_spread_the_other_way() {
    // The three that want 42, 42 and 43 are next to the two that want 40 and 41, and the cheapest is to move the later ones.
    assert_eq!(put_at(&[40, 41, 42, 42, 43], Some(100)), vec![40, 41, 42, 43, 44]);
    assert_eq!(put_at(&[2, 4, 5, 6, 6], Some(9)), vec![2, 4, 5, 6, 7]);
    assert_eq!(put_at(&[6, 6, 7, 7, 10], Some(10)), vec![5, 6, 7, 8, 9]);
}

#[test]
fn as_many_keyframes_as_frames_are_at_every_frame_whatever_they_want() {
    assert_eq!(put_at(&[5, 5, 5, 5], Some(4)), vec![0, 1, 2, 3]);
    assert_eq!(put_at(&[0, 0, 0, 0], Some(4)), vec![0, 1, 2, 3]);
    assert_eq!(put_at(&[0, 1, 1, 3], Some(4)), vec![0, 1, 2, 3]);
    assert_eq!(put_at(&[0], Some(1)), vec![0]);
}

// --- too many keyframes ---

#[test]
fn when_there_are_more_keyframes_than_frames_the_latest_are_lost_if_the_end_is_what_goes() {
    let put = frames_for_keyframes_wanted_at(&[0, 5, 9, 12, 30], Some(3), TheLatest);
    assert_eq!(put, vec![Some(0), Some(1), Some(2), None, None]);
    let put = frames_for_keyframes_wanted_at(&[1, 2, 3, 4], Some(1), TheLatest);
    assert_eq!(put, vec![Some(0), None, None, None]);
    assert_eq!(
        frames_for_keyframes_wanted_at(&[1, 2], Some(0), TheLatest),
        vec![None, None]
    );
}

#[test]
fn when_there_are_more_keyframes_than_frames_and_all_of_it_stays_the_ones_kept_are_spread_through_the_keyframes() {
    let kept = |count: usize, room: u64| {
        let wanted: Vec<u64> = (0..count as u64).collect();
        places_kept(&frames_for_keyframes_wanted_at(&wanted, Some(room), SpreadThroughThem))
    };
    assert_eq!(kept(10, 4), vec![0, 3, 6, 9]);
    assert_eq!(kept(10, 6), vec![0, 2, 4, 5, 7, 9]);
    assert_eq!(kept(10, 2), vec![0, 9], "the first and the last");
    assert_eq!(kept(10, 1), vec![0], "when there is room for one only, the first is it");
    assert_eq!(kept(10, 0), Vec::<usize>::new());
    assert_eq!(
        kept(10, 9),
        vec![0, 1, 2, 3, 5, 6, 7, 8, 9],
        "the one that is lost is nearest the middle"
    );
    assert_eq!(kept(3, 3), vec![0, 1, 2], "all of them, if there is room for all");
    assert_eq!(kept(3, 50), vec![0, 1, 2]);
}

#[test]
fn the_keyframes_kept_from_a_crowd_that_is_too_big_are_put_at_every_frame() {
    let put = frames_for_keyframes_wanted_at(&[0, 1, 2, 3, 4, 5, 6, 7], Some(4), SpreadThroughThem);
    assert_eq!(put, vec![Some(0), None, Some(1), None, None, Some(2), None, Some(3)]);
}

// --- whatever they want ---

/// Found by looking at what a clip does to its keyframes when it is sped up: a keyframe that is lost only if there are more keyframes
/// than frames, whichever frames they want, and the others are in their order, each at a frame of its own within the length, and not
/// further from what they wanted than there are of them.
#[test]
fn whatever_the_keyframes_want_those_kept_have_frames_of_their_own_in_order_and_near_what_they_wanted() {
    let mut random = DeterministicRandomNumbers::with_seed(8);
    let (mut crowded, mut too_many, mut with_no_end, mut already_fine) = (0, 0, 0, 0);
    for round in 0..4000 {
        let count = random.next_below(13) as usize;
        let length = match random.next_below(4) {
            0 => None,
            _ => Some(random.next_below(22)),
        };
        let mut wanted: Vec<u64> = (0..count).map(|_| random.next_below(26)).collect();
        wanted.sort_unstable();
        let which_are_lost = if random.next_below(2) == 0 {
            TheLatest
        } else {
            SpreadThroughThem
        };
        let context = format!("round {round}: {wanted:?} in {length:?} frames, {which_are_lost:?}");

        let put = frames_for_keyframes_wanted_at(&wanted, length, which_are_lost);

        assert_eq!(put.len(), count, "{context}");
        let kept = places_kept(&put);
        let room = length.map_or(count, |length| count.min(usize::try_from(length).unwrap()));
        assert_eq!(
            kept.len(),
            room,
            "one is lost for every one there is no frame for, and no more: {context}"
        );
        let frames: Vec<u64> = put.iter().flatten().copied().collect();
        assert!(
            frames.windows(2).all(|pair| pair[0] < pair[1]),
            "in the order they were in, each at a frame of its own: {put:?}, {context}"
        );
        if let Some(length) = length {
            assert!(frames.iter().all(|frame| *frame < length), "{put:?}, {context}");
        }
        for (place, frame) in put.iter().enumerate() {
            let Some(frame) = frame else { continue };
            let wanted_at_most = length.map_or(wanted[place], |length| wanted[place].min(length.saturating_sub(1)));
            assert!(
                frame.abs_diff(wanted_at_most) < kept.len() as u64,
                "keyframe {place} is put at {frame}, which is too far from {wanted_at_most}: {put:?}, {context}"
            );
        }
        let want_frames_of_their_own = wanted.windows(2).all(|pair| pair[0] < pair[1])
            && length.is_none_or(|length| wanted.iter().all(|frame| *frame < length));
        if want_frames_of_their_own {
            already_fine += 1;
            assert_eq!(
                frames, wanted,
                "those that are not in each other's way stay where they are: {context}"
            );
        }
        if which_are_lost == TheLatest {
            assert_eq!(
                kept,
                (0..room).collect::<Vec<_>>(),
                "the earliest are the ones kept: {context}"
            );
        } else if room >= 2 && count > room {
            assert_eq!(
                (kept.first(), kept.last()),
                (Some(&0), Some(&(count - 1))),
                "the first and the last are kept: {context}"
            );
        }
        crowded += usize::from(wanted.windows(2).any(|pair| pair[0] == pair[1]));
        too_many += usize::from(count > room);
        with_no_end += usize::from(length.is_none());
    }
    assert!(
        crowded > 1000,
        "many of the rounds have keyframes that want one frame: {crowded}"
    );
    assert!(
        too_many > 300,
        "many of the rounds have more keyframes than frames: {too_many}"
    );
    assert!(with_no_end > 500 && already_fine > 300, "{with_no_end}, {already_fine}");
}

// --- as little as can be ---

/// Calls `visit` with every way of choosing `count` of the frames from `from` up to (not including) `to`, in order.
fn for_each_way_of_choosing(from: u64, to: u64, count: usize, chosen: &mut Vec<u64>, visit: &mut impl FnMut(&[u64])) {
    if count == 0 {
        visit(chosen);
        return;
    }
    for frame in from..to {
        chosen.push(frame);
        for_each_way_of_choosing(frame + 1, to, count - 1, chosen, visit);
        chosen.pop();
    }
}

/// How far the keyframes are put from where they want to be, all the distances added up (the last frame is as far as they can
/// want to be).
fn moved_by(wanted: &[u64], length: Option<u64>, put: &[u64]) -> u64 {
    wanted
        .iter()
        .zip(put)
        .map(|(wanted, put)| {
            length
                .map_or(*wanted, |length| (*wanted).min(length - 1))
                .abs_diff(*put)
        })
        .sum()
}

/// The least that any way of giving each of the keyframes a frame of its own, in order, moves them by altogether: found by trying
/// every way there is.
fn the_least_they_can_be_moved_by(wanted: &[u64], length: Option<u64>) -> u64 {
    let frames_to_choose_from =
        length.unwrap_or_else(|| wanted.iter().max().map_or(0, |last| last + wanted.len() as u64 + 1));
    let mut least = u64::MAX;
    for_each_way_of_choosing(0, frames_to_choose_from, wanted.len(), &mut Vec::new(), &mut |way| {
        least = least.min(moved_by(wanted, length, way));
    });
    least
}

/// The first version of this spread a crowd out by averaging two ways of making room for it (pushing the crowd later, and pushing
/// it earlier), which looked fair, and trying every way there is found that in about one case in thirty it moved keyframes that
/// were not in the crowd by more than it had to. Whatever they want, no way of putting the keyframes at frames of their own moves
/// them less altogether than the way they are put.
#[test]
fn no_way_of_giving_keyframes_frames_of_their_own_moves_them_less_than_the_way_they_are_put_for_every_small_case() {
    let mut cases = 0;
    for count in 0..=5usize {
        // Every way for `count` keyframes to want frames from 0 to 6, none wanting a frame before the one before it.
        let mut wanted_by_each_way: Vec<Vec<u64>> = Vec::new();
        for_each_non_decreasing(0, 6, count, &mut Vec::new(), &mut |wanted| {
            wanted_by_each_way.push(wanted.to_vec())
        });
        for wanted in &wanted_by_each_way {
            for length in std::iter::once(None).chain((count as u64..=8).map(Some)) {
                let put = put_at(wanted, length);
                assert_eq!(
                    moved_by(wanted, length, &put),
                    the_least_they_can_be_moved_by(wanted, length),
                    "{wanted:?} in {length:?} frames are put at {put:?}"
                );
                cases += 1;
            }
        }
    }
    assert!(cases > 4000, "{cases}");
}

/// Calls `visit` with every way of choosing `count` numbers from `from` up to and including `to`, each at least as high as the one
/// before it.
fn for_each_non_decreasing(from: u64, to: u64, count: usize, chosen: &mut Vec<u64>, visit: &mut impl FnMut(&[u64])) {
    if count == 0 {
        visit(chosen);
        return;
    }
    for number in from..=to {
        chosen.push(number);
        for_each_non_decreasing(number, to, count - 1, chosen, visit);
        chosen.pop();
    }
}

#[test]
fn no_way_of_giving_keyframes_frames_of_their_own_moves_them_less_than_the_way_they_are_put_for_random_larger_cases() {
    let mut random = DeterministicRandomNumbers::with_seed(21);
    let mut crowded = 0;
    for round in 0..1500 {
        let count = 1 + random.next_below(7) as usize;
        let length = match random.next_below(4) {
            0 => None,
            _ => Some(count as u64 + random.next_below(6)),
        };
        let mut wanted: Vec<u64> = (0..count).map(|_| random.next_below(16)).collect();
        wanted.sort_unstable();
        let put = put_at(&wanted, length);
        assert_eq!(
            moved_by(&wanted, length, &put),
            the_least_they_can_be_moved_by(&wanted, length),
            "round {round}: {wanted:?} in {length:?} frames are put at {put:?}"
        );
        crowded += usize::from(wanted.windows(2).any(|pair| pair[0] == pair[1]));
    }
    assert!(crowded > 400, "many of the rounds have a crowd: {crowded}");
}

#[test]
fn putting_keyframes_that_have_been_put_already_changes_nothing() {
    let mut random = DeterministicRandomNumbers::with_seed(3);
    for round in 0..500 {
        let count = random.next_below(9) as usize;
        let length = Some(count as u64 + random.next_below(10));
        let mut wanted: Vec<u64> = (0..count).map(|_| random.next_below(20)).collect();
        wanted.sort_unstable();
        let put = put_at(&wanted, length);
        assert_eq!(
            put_at(&put, length),
            put,
            "round {round}: {wanted:?} in {length:?} frames"
        );
    }
}
