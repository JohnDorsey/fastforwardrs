// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Making the finer thumbnails of one video, one run of ffmpeg at a time, for the pictures that the
//! timeline says it wants and does not have.
//!
//! The decisions are here, apart from the application, so that they can be tested without running
//! anything: which run to ask for next, that a picture is never asked for twice (some never come:
//! a video may end before it says it does), and how many are kept.

use std::collections::{BTreeSet, HashSet};

use super::thumbnail_library::{ThumbnailLibrary, plan_finer_thumbnails};
use super::thumbnails::{ThumbnailEvent, ThumbnailExtraction, ThumbnailKey, ThumbnailRange};

/// The work of making finer pictures of a video.
#[derive(Default)]
pub struct FinerThumbnails {
    /// The pictures that the timeline wanted in the last frame and did not have.
    wanted: BTreeSet<ThumbnailKey>,
    /// The pictures that have been asked for, so that they are not asked for again.
    asked_for: HashSet<ThumbnailKey>,
    /// The run of pictures that is being made now, if one is.
    running: Option<(ThumbnailRange, ThumbnailExtraction)>,
    /// How many runs have been started.
    runs_started: usize,
}

impl FinerThumbnails {
    /// Says which pictures the timeline wants now (and did not have), in place of what it wanted before.
    pub fn set_wanted(&mut self, wanted: BTreeSet<ThumbnailKey>) {
        self.wanted = wanted;
    }

    /// How many pictures the timeline wanted and did not have, in the last frame.
    pub fn wanted_count(&self) -> usize {
        self.wanted.len()
    }

    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// How many runs of ffmpeg have been started to make finer pictures.
    pub fn runs_started(&self) -> usize {
        self.runs_started
    }

    /// Puts the pictures that the running run has made so far into `library`, and notes when it has
    /// finished.
    pub fn collect_into(&mut self, library: &mut ThumbnailLibrary) {
        let Some((range, extraction)) = &self.running else {
            return;
        };
        let mut finished = false;
        while let Some(event) = extraction.try_next_event() {
            match event {
                ThumbnailEvent::Thumbnail { index, picture } => library.insert(
                    ThumbnailKey {
                        level: range.level,
                        index,
                    },
                    picture,
                ),
                ThumbnailEvent::Finished { .. } => {
                    finished = true;
                    break;
                }
            }
        }
        if finished {
            self.running = None;
        }
    }

    /// If `library` has more than `most` finer pictures, lets go of those that are not wanted now
    /// (and forgets that they were asked for, so that they are made again if they are wanted again).
    pub fn keep_within(&mut self, library: &mut ThumbnailLibrary, most: usize) {
        if library.finer_count() > most {
            library.keep_only_these_finer_pictures(&self.wanted);
            self.asked_for.retain(|key| self.wanted.contains(key));
        }
    }

    /// The next run of pictures to make, which is noted as asked for: the first of the runs that the
    /// wanted pictures that `library` does not have, and that have not been asked for, need. `None`
    /// if there is none.
    pub fn next_run(&mut self, library: &ThumbnailLibrary) -> Option<ThumbnailRange> {
        let plan = plan_finer_thumbnails(&self.wanted, |key| {
            library.get(key).is_some() || self.asked_for.contains(&key)
        });
        let range = plan.first().copied()?;
        self.asked_for.extend(range.keys());
        Some(range)
    }

    /// Notes that `extraction` is making the pictures of `range`.
    pub fn start(&mut self, range: ThumbnailRange, extraction: ThumbnailExtraction) {
        self.running = Some((range, extraction));
        self.runs_started += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::{CompressedPicture, PictureSize};

    fn key(level: u8, index: usize) -> ThumbnailKey {
        ThumbnailKey { level, index }
    }

    fn wanted(keys: &[(u8, usize)]) -> BTreeSet<ThumbnailKey> {
        keys.iter().map(|&(level, index)| key(level, index)).collect()
    }

    fn picture() -> CompressedPicture {
        CompressedPicture::from_rgb_pixels(
            &[7; 4 * 4 * 3],
            PictureSize {
                width_in_pixels: 4,
                height_in_pixels: 4,
            },
        )
        .unwrap()
    }

    fn first_index_and_count(range: Option<ThumbnailRange>) -> Option<(u8, usize, usize)> {
        range.map(|range| (range.level, range.first_index, range.count.unwrap()))
    }

    #[test]
    fn nothing_wanted_is_nothing_to_make() {
        let mut work = FinerThumbnails::default();
        assert_eq!(work.next_run(&ThumbnailLibrary::with_room_for(10)), None);
        assert_eq!(work.wanted_count(), 0);
    }

    #[test]
    fn the_wanted_pictures_that_there_are_not_are_asked_for_in_runs_and_never_asked_for_twice() {
        let mut work = FinerThumbnails::default();
        let library = ThumbnailLibrary::with_room_for(10);
        work.set_wanted(wanted(&[(2, 10), (2, 11), (2, 12), (3, 40)]));

        assert_eq!(first_index_and_count(work.next_run(&library)), Some((2, 10, 3)));
        // Whether or not they have come, the pictures of that run are not asked for again...
        assert_eq!(
            first_index_and_count(work.next_run(&library)),
            Some((3, 40, 1)),
            "the next run"
        );
        assert_eq!(work.next_run(&library), None, "and there is nothing left to ask for");
        assert_eq!(work.next_run(&library), None);
    }

    #[test]
    fn a_picture_that_never_comes_is_not_asked_for_again_either() {
        let mut work = FinerThumbnails::default();
        let library = ThumbnailLibrary::with_room_for(10);
        work.set_wanted(wanted(&[(4, 900)]));
        assert!(work.next_run(&library).is_some());
        // The run ended without it (the video was shorter than it said), and the timeline still wants it.
        work.set_wanted(wanted(&[(4, 900)]));
        assert_eq!(work.next_run(&library), None);
    }

    #[test]
    fn pictures_that_the_library_has_are_not_asked_for_and_the_ones_next_to_them_are() {
        let mut work = FinerThumbnails::default();
        let mut library = ThumbnailLibrary::with_room_for(10);
        library.insert(key(2, 10), picture());
        library.insert(key(2, 11), picture());
        work.set_wanted(wanted(&[(2, 10), (2, 11), (2, 12), (2, 13)]));

        assert_eq!(first_index_and_count(work.next_run(&library)), Some((2, 12, 2)));
    }

    #[test]
    fn what_is_wanted_is_what_was_said_last_and_not_everything_that_ever_was() {
        let mut work = FinerThumbnails::default();
        let library = ThumbnailLibrary::with_room_for(10);
        work.set_wanted(wanted(&[(2, 10)]));
        work.set_wanted(wanted(&[(2, 50)]));
        assert_eq!(work.wanted_count(), 1);
        assert_eq!(first_index_and_count(work.next_run(&library)), Some((2, 50, 1)));
    }

    #[test]
    fn too_many_pictures_are_let_go_of_except_the_wanted_ones_which_can_then_be_asked_for_again() {
        let mut work = FinerThumbnails::default();
        let mut library = ThumbnailLibrary::with_room_for(10);
        for index in 0..6 {
            library.insert(key(2, 100 + index), picture());
        }
        work.set_wanted(wanted(&[(2, 300), (2, 400)]));
        assert_eq!(
            first_index_and_count(work.next_run(&library)),
            Some((2, 300, 1)),
            "asked for"
        );
        assert_eq!(
            first_index_and_count(work.next_run(&library)),
            Some((2, 400, 1)),
            "and this"
        );
        work.set_wanted(wanted(&[(2, 100), (2, 105), (2, 400)]));

        work.keep_within(&mut library, 10);
        assert_eq!(library.finer_count(), 6, "not too many yet");

        work.keep_within(&mut library, 5);
        assert_eq!(
            library.finer_count(),
            2,
            "only the wanted ones that there were are left"
        );
        assert!(library.get(key(2, 100)).is_some() && library.get(key(2, 105)).is_some());
        // What is wanted and was asked for is still remembered as asked for, and what is not wanted any
        // more is forgotten, so that it is asked for again if it is wanted again.
        work.set_wanted(wanted(&[(2, 101), (2, 300), (2, 400)]));
        assert_eq!(first_index_and_count(work.next_run(&library)), Some((2, 101, 1)));
        assert_eq!(
            first_index_and_count(work.next_run(&library)),
            Some((2, 300, 1)),
            "300 was let go of"
        );
        assert_eq!(
            work.next_run(&library),
            None,
            "400 was wanted all along, so it is still remembered"
        );
    }

    #[test]
    fn collecting_with_nothing_running_does_nothing() {
        let mut work = FinerThumbnails::default();
        let mut library = ThumbnailLibrary::with_room_for(10);
        work.collect_into(&mut library);
        assert_eq!(library.finer_count(), 0);
        assert!(!work.is_running());
        assert_eq!(work.runs_started(), 0);
    }
}
