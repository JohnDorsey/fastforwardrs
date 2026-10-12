//! Fitting keyframes into the frames of what holds them, when it gets shorter or its frames are counted again.
//!
//! Keyframes want to be where they were in relation to what holds them: as far through a clip as they were when the clip is sped
//! up, at the moment of time they were at when the frame rate changes, where they were when the clip is cut short. But two of them
//! cannot be at one frame, and none can be past the end, and a keyframe that is lost is work that is lost. So when what they want
//! does not fit, they are put at frames of their own, as near to what they want as can be: empty frames are what is given up. A
//! keyframe is lost only when there are more keyframes than there are frames to put them at.
//!
//! This is the arithmetic of that, on frames alone, so that it can be tested thoroughly without any of what has keyframes.

/// Which keyframes are lost when there are more of them than there are frames to put them at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WhichAreLost {
    /// The latest ones: what holds the keyframes is cut short, and the end of it is what goes.
    TheLatest,
    /// Ones chosen so that those that are kept are as evenly spread through the keyframes as can be, with the first and the last
    /// of them among those kept: what holds the keyframes lasts less time but all of it is still there, so what is kept is a
    /// coarser version of the whole. (When there is only one frame the first keyframe is kept.)
    SpreadThroughThem,
}

/// The frame that each of some keyframes is put at, or `None` for each that is lost, when the keyframes want to be at the frames
/// `wanted` (one for each, in the order of the keyframes: no keyframe wants to be earlier than the one before it) and there are
/// `length` frames to put them at, from 0 (or as many as are wanted, if that is `None`: what holds them has no end).
///
/// The frames put are in the order of the keyframes, each at a frame of its own, none at or after `length`, and the keyframes are
/// moved from the frames they want as little as can be altogether. So keyframes that want frames of their own within `length` are
/// put exactly there, and a crowd of keyframes that want one frame is spread out round it (or back from the end, if it wants the
/// end), which moves the keyframes next to it only as much as that needs.
///
/// If `length` is less than the number of keyframes, the ones that `which_are_lost` says are lost, so that the others have a
/// frame each.
pub(super) fn frames_for_keyframes_wanted_at(
    wanted: &[u64],
    length: Option<u64>,
    which_are_lost: WhichAreLost,
) -> Vec<Option<u64>> {
    let kept = places_of_the_keyframes_kept(wanted.len(), length, which_are_lost);
    let wanted_by_the_kept: Vec<u64> = kept.iter().map(|&place| wanted[place]).collect();
    let mut put_at = vec![None; wanted.len()];
    for (place, frame) in kept
        .into_iter()
        .zip(spread_out_to_frames_of_their_own(&wanted_by_the_kept, length))
    {
        put_at[place] = Some(frame);
    }
    put_at
}

/// The places, in the order of `count` keyframes, of those that are not lost when there are `length` frames to put them at.
fn places_of_the_keyframes_kept(count: usize, length: Option<u64>, which_are_lost: WhichAreLost) -> Vec<usize> {
    let room = length
        .map(|length| usize::try_from(length).unwrap_or(usize::MAX))
        .filter(|room| *room < count);
    let Some(room) = room else {
        return (0..count).collect();
    };
    match (which_are_lost, room) {
        (_, 0) => Vec::new(),
        (WhichAreLost::TheLatest, room) => (0..room).collect(),
        (WhichAreLost::SpreadThroughThem, 1) => vec![0],
        (WhichAreLost::SpreadThroughThem, room) => {
            // The nth of the places kept is as far along the keyframes as the nth of `room` frames is along the frames (to the
            // nearest place, a half going up), which puts the first of them at the first and the last at the last.
            let (count, steps) = (count as u128, (room - 1) as u128);
            (0..=steps)
                .map(|nth| usize::try_from((2 * nth * (count - 1) + steps) / (2 * steps)).unwrap_or(usize::MAX))
                .collect()
        }
    }
}

/// Where keyframes that want to be at `wanted` are put when each is to have a frame of its own, and there are `length` frames of
/// them (as many as are needed, if `None`), which are at least as many as the keyframes.
///
/// The keyframes are moved as little as can be altogether (the numbers of frames they are each moved by, added up): a crowd that
/// wants one frame is spread out round it, which moves those next to it only as much as that needs, and where there are two ways
/// of spreading a crowd out that are as good, the crowd gets the extra frame before the one it wants.
fn spread_out_to_frames_of_their_own(wanted: &[u64], length: Option<u64>) -> Vec<u64> {
    let count = wanted.len();
    if count == 0 {
        return Vec::new();
    }
    debug_assert!(
        length.is_none_or(|length| length >= count as u64),
        "there is a frame for each"
    );
    let signed = |number: u64| i128::from(number);
    let place = |place: usize| i128::try_from(place).unwrap_or(i128::MAX);
    // What each wants less its place in the order: keyframes that are each at a frame after the one before it have numbers that do
    // not go down, and the others have one that is lower than the one before it. So the keyframes are put at frames of their own by
    // finding the numbers that do not go down that are nearest to these.
    let wanted_less_the_place: Vec<i128> = wanted
        .iter()
        .enumerate()
        .map(|(at, &frame)| signed(frame) - place(at))
        .collect();
    // The first cannot be before the first frame, and the last cannot be after the last frame: which, less its place, is the
    // number that the first can be at the most. (A keyframe that wants a frame past the end is put where it would be if it wanted
    // the last frame, because of this: it is not necessary to say so.)
    let highest_number = length.map(|length| signed(length) - place(count));
    nearest_numbers_that_do_not_go_down(&wanted_less_the_place)
        .into_iter()
        .enumerate()
        .map(|(at, number)| {
            let number = number.max(0);
            let number = highest_number.map_or(number, |highest| number.min(highest));
            u64::try_from(number + place(at)).unwrap_or(u64::MAX)
        })
        .collect()
}

/// Numbers that do not go down, one for each of `numbers`, which are as near to them as can be altogether (the distances added up).
/// Where there is more than one way, the lower numbers.
///
/// Neighbours are put at one number, as a group, when the number of the group before them is higher than theirs ("pool adjacent
/// violators"): the number of a group is the one in the middle of the numbers it is made of (the lower, if there are two), which
/// is the one that they are all as near to as can be altogether.
fn nearest_numbers_that_do_not_go_down(numbers: &[i128]) -> Vec<i128> {
    fn middle(group: &[i128]) -> i128 {
        group[(group.len() - 1) / 2]
    }
    // Each group has the numbers it is made of, the lowest first.
    let mut groups: Vec<Vec<i128>> = Vec::new();
    for &number in numbers {
        let mut group = vec![number];
        while let Some(mut before) = groups.pop_if(|before| middle(before) > middle(&group)) {
            before.extend(group);
            before.sort_unstable();
            group = before;
        }
        groups.push(group);
    }
    groups
        .iter()
        .flat_map(|group| std::iter::repeat_n(middle(group), group.len()))
        .collect()
}

#[cfg(test)]
mod tests;
