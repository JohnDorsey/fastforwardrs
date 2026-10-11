//! Everything made so far of the thumbnails of one video, and working out which finer ones to make next.
//!
//! The picture of every second of a video is made once, for the whole of it, when the video is
//! opened. The finer ones (see [`ThumbnailKey`]) are made when they are wanted, for the stretch of
//! the video that is on screen, and only that: an hour of video has hundreds of thousands of them.

use std::collections::{BTreeSet, HashMap};

use super::thumbnails::{
    CompressedPicture, MOST_FINER_THUMBNAILS_AT_ONCE, ThumbnailKey, ThumbnailRange, ThumbnailStrip,
};

/// Holes of this many pictures or fewer between two wanted ones do not split a request in two: it
/// costs less to make the few pictures between than to start ffmpeg again.
const BIGGEST_HOLE_NOT_WORTH_SPLITTING_AT: usize = 2;

/// The thumbnails of one video: a picture for every second, and finer ones for the parts of it that
/// have been looked at closely.
#[derive(Clone, Debug, Default)]
pub struct ThumbnailLibrary {
    every_second: ThumbnailStrip,
    finer: HashMap<ThumbnailKey, CompressedPicture>,
}

impl ThumbnailLibrary {
    /// A library with room for the pictures of `seconds_of_video` seconds, none of them made yet.
    pub fn with_room_for(seconds_of_video: usize) -> Self {
        Self {
            every_second: ThumbnailStrip::with_room_for(seconds_of_video),
            finer: HashMap::new(),
        }
    }

    /// The picture of every second.
    pub fn every_second(&self) -> &ThumbnailStrip {
        &self.every_second
    }

    /// Stores a picture. (One for a second beyond the end of the video is ignored.)
    pub fn insert(&mut self, key: ThumbnailKey, picture: CompressedPicture) {
        if key.level == 0 {
            self.every_second.insert(key.index, picture);
        } else {
            self.finer.insert(key, picture);
        }
    }

    /// The picture `key` names, if it has been made.
    pub fn get(&self, key: ThumbnailKey) -> Option<&CompressedPicture> {
        if key.level == 0 {
            self.every_second.picture_for_second(key.index)
        } else {
            self.finer.get(&key)
        }
    }

    /// The picture `key` names if it has been made, and if not the one of the next coarser level that
    /// is nearest to the same moment, and so on down to the one of a second: what to show until the
    /// picture that was asked for is made. `None` if not even that has been.
    pub fn nearest_made(&self, key: ThumbnailKey) -> Option<(ThumbnailKey, &CompressedPicture)> {
        let seconds = key.seconds();
        (0..=key.level)
            .rev()
            .map(|level| ThumbnailKey::nearest(level, seconds))
            .find_map(|candidate| Some((candidate, self.get(candidate)?)))
    }

    /// How many of the finer pictures there are.
    pub fn finer_count(&self) -> usize {
        self.finer.len()
    }

    /// Lets go of the finer pictures that are not among `keep`, to make room.
    pub fn keep_only_these_finer_pictures(&mut self, keep: &BTreeSet<ThumbnailKey>) {
        self.finer.retain(|key, _| keep.contains(key));
    }
}

/// The runs of pictures to ask ffmpeg for so as to have all of `wanted`, leaving out those that
/// `already_have` says there are (or have been asked for). Pictures of the same level that are
/// nearly next to one another go in one run, and no run is longer than
/// [`MOST_FINER_THUMBNAILS_AT_ONCE`]. In order of level and then of place.
pub fn plan_finer_thumbnails(
    wanted: &BTreeSet<ThumbnailKey>,
    already_have: impl Fn(ThumbnailKey) -> bool,
) -> Vec<ThumbnailRange> {
    let mut ranges: Vec<ThumbnailRange> = Vec::new();
    for key in wanted
        .iter()
        .copied()
        .filter(|key| key.level > 0 && !already_have(*key))
    {
        if let Some(last) = ranges.last_mut()
            && last.level == key.level
            && let Some(count) = last.count
            && key.index < last.first_index + count + BIGGEST_HOLE_NOT_WORTH_SPLITTING_AT + 1
            && key.index + 1 - last.first_index <= MOST_FINER_THUMBNAILS_AT_ONCE
        {
            last.count = Some(key.index + 1 - last.first_index);
        } else {
            ranges.push(ThumbnailRange {
                level: key.level,
                first_index: key.index,
                count: Some(1),
            });
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::PictureSize;

    fn key(level: u8, index: usize) -> ThumbnailKey {
        ThumbnailKey { level, index }
    }

    fn picture(shade: u8) -> CompressedPicture {
        CompressedPicture::from_rgb_pixels(
            &[shade; 4 * 4 * 3],
            PictureSize {
                width_in_pixels: 4,
                height_in_pixels: 4,
            },
        )
        .unwrap()
    }

    fn wanted(keys: &[(u8, usize)]) -> BTreeSet<ThumbnailKey> {
        keys.iter().map(|&(level, index)| key(level, index)).collect()
    }

    // --- the library ---

    #[test]
    fn the_pictures_of_every_second_and_the_finer_ones_are_stored_and_found_by_their_keys() {
        let mut library = ThumbnailLibrary::with_room_for(10);
        library.insert(key(0, 3), picture(10));
        library.insert(key(2, 13), picture(20));

        assert_eq!(library.get(key(0, 3)), Some(&picture(10)));
        assert_eq!(library.get(key(2, 13)), Some(&picture(20)));
        assert_eq!(library.get(key(2, 3)), None, "another picture of another level");
        assert_eq!(library.get(key(0, 4)), None);
        assert_eq!(
            library.every_second().made_count(),
            1,
            "only the one of a second is in the strip"
        );
        assert_eq!(library.finer_count(), 1);
        assert_eq!(library.every_second().picture_for_second(3), Some(&picture(10)));
    }

    #[test]
    fn the_nearest_picture_made_is_the_one_asked_for_or_else_the_next_coarser_one_that_there_is() {
        let mut library = ThumbnailLibrary::with_room_for(10);
        library.insert(key(0, 2), picture(1)); // 2.0 s
        library.insert(key(1, 5), picture(2)); // 2.5 s
        library.insert(key(3, 20), picture(3)); // 2.5 s
        let nearest = |key: ThumbnailKey| library.nearest_made(key).map(|(found, _)| found);

        assert_eq!(
            nearest(key(3, 20)),
            Some(key(3, 20)),
            "the one asked for, when it is there"
        );
        assert_eq!(
            nearest(key(3, 21)),
            Some(key(1, 5)),
            "2.625 s: the nearest of the half seconds is 2.5 s, though going down a level at a time would pass 3 s"
        );
        assert_eq!(
            nearest(key(2, 10)),
            Some(key(1, 5)),
            "2.5 s at four a second: the one of two a second"
        );
        assert_eq!(
            nearest(key(2, 9)),
            Some(key(1, 5)),
            "2.25 s: 2.5 s is the nearest of the half seconds"
        );
        assert_eq!(
            nearest(key(1, 3)),
            Some(key(0, 2)),
            "1.5 s: 2 s is what there is of the whole seconds"
        );
        assert_eq!(nearest(key(0, 7)), None, "nothing at 7 s yet");
        assert_eq!(nearest(key(5, 300)), None);
    }

    #[test]
    fn finer_pictures_can_be_let_go_of_except_those_to_keep_and_the_ones_of_every_second_stay() {
        let mut library = ThumbnailLibrary::with_room_for(10);
        library.insert(key(0, 1), picture(1));
        library.insert(key(1, 3), picture(2));
        library.insert(key(2, 9), picture(3));
        library.insert(key(2, 10), picture(4));

        library.keep_only_these_finer_pictures(&wanted(&[(2, 10), (1, 99)]));

        assert_eq!(library.finer_count(), 1);
        assert!(library.get(key(2, 10)).is_some());
        assert!(library.get(key(2, 9)).is_none());
        assert!(
            library.get(key(0, 1)).is_some(),
            "the pictures of every second are never let go of"
        );
    }

    // --- planning ---

    fn ranges(keys: &[(u8, usize)], have: &[(u8, usize)]) -> Vec<(u8, usize, usize)> {
        let have: BTreeSet<ThumbnailKey> = have.iter().map(|&(level, index)| key(level, index)).collect();
        plan_finer_thumbnails(&wanted(keys), |key| have.contains(&key))
            .into_iter()
            .map(|range| (range.level, range.first_index, range.count.unwrap()))
            .collect()
    }

    #[test]
    fn pictures_that_are_next_to_one_another_are_asked_for_in_one_run() {
        assert_eq!(ranges(&[(2, 10), (2, 11), (2, 12), (2, 13)], &[]), vec![(2, 10, 4)]);
    }

    #[test]
    fn a_small_hole_between_wanted_pictures_is_made_too_and_a_big_one_splits_the_run() {
        assert_eq!(ranges(&[(3, 10), (3, 12)], &[]), vec![(3, 10, 3)], "one between");
        assert_eq!(ranges(&[(3, 10), (3, 13)], &[]), vec![(3, 10, 4)], "two between");
        assert_eq!(
            ranges(&[(3, 10), (3, 14)], &[]),
            vec![(3, 10, 1), (3, 14, 1)],
            "three between: not worth it"
        );
    }

    #[test]
    fn pictures_of_different_levels_are_in_different_runs_and_come_in_the_order_of_level_and_place() {
        assert_eq!(
            ranges(&[(3, 40), (2, 20), (2, 21), (1, 5)], &[]),
            vec![(1, 5, 1), (2, 20, 2), (3, 40, 1)]
        );
    }

    #[test]
    fn what_there_already_is_is_not_asked_for_again_and_the_pictures_of_every_second_are_not_asked_for_at_all() {
        assert_eq!(
            ranges(&[(2, 10), (2, 11), (2, 12)], &[(2, 10), (2, 11)]),
            vec![(2, 12, 1)]
        );
        assert_eq!(ranges(&[(2, 10)], &[(2, 10)]), vec![]);
        assert_eq!(
            ranges(&[(0, 3), (0, 4)], &[]),
            vec![],
            "those are made for the whole video, at the start"
        );
    }

    #[test]
    fn a_run_is_never_longer_than_the_most_there_is_to_make_at_once() {
        let keys: Vec<(u8, usize)> = (100..300).map(|index| (4, index)).collect();
        let runs = ranges(&keys, &[]);
        assert!(
            runs.iter().all(|&(_, _, count)| count <= MOST_FINER_THUMBNAILS_AT_ONCE),
            "{runs:?}"
        );
        assert_eq!(
            runs.iter().map(|&(_, _, count)| count).sum::<usize>(),
            200,
            "and they are all asked for"
        );
        for pair in runs.windows(2) {
            assert_eq!(
                pair[0].1 + pair[0].2,
                pair[1].1,
                "one run after another with nothing left out"
            );
        }
    }

    #[test]
    fn nothing_wanted_is_nothing_to_ask_for() {
        assert_eq!(ranges(&[], &[]), vec![]);
    }
}
