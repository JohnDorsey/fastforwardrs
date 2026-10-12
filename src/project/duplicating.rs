//! Copying a clip, or a track, to the side of itself, with what is in its drawers.

use super::tracks::{ClipId, TrackId};
use super::{ClipEditError, EditError, Project, TimelineFrameIndex};

/// Which side of a clip its copy is put on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipSide {
    /// Before it: the copy ends where the clip starts.
    Left,
    /// After it: the copy starts where the clip ends.
    Right,
}

/// Which side of a track its copy is put on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackSide {
    /// In front of it.
    Above,
    /// Behind it.
    Below,
}

/// `title` without the "copy" at its end that a copy has been given, with the number after it if it has one: "captions copy 2"
/// is "captions", and a title that does not end like that is as it is.
fn without_the_copy_ending(title: &str) -> &str {
    let number_starts_at = title.trim_end_matches(|letter: char| letter.is_ascii_digit()).len();
    let (before_the_number, number) = title.split_at(number_starts_at);
    let before_the_number = if number.is_empty() {
        Some(title)
    } else {
        before_the_number.strip_suffix(' ')
    };
    before_the_number
        .and_then(|words| words.strip_suffix(" copy"))
        .unwrap_or(title)
}

impl Project {
    /// The frame a copy of `clip` on `side` of it would start at, and the position of the track it would be on, if there is
    /// room for it there.
    fn where_a_copy_of_a_clip_would_go(
        &self,
        clip: ClipId,
        side: ClipSide,
    ) -> Result<(usize, TimelineFrameIndex), EditError> {
        let position = self
            .track_position_holding(clip)
            .ok_or(ClipEditError::UnknownClip { clip })?;
        let track = &self.tracks[position];
        let placement = track.placement_of(clip).ok_or(ClipEditError::UnknownClip { clip })?;
        let length = placement.timeline_frame_count();
        let start = match side {
            ClipSide::Right => Some(placement.end_timeline_frame.value()),
            ClipSide::Left => placement.first_timeline_frame.value().checked_sub(length),
        }
        .map(TimelineFrameIndex::new);
        match start {
            Some(start) if track.start_of_the_free_stretch_that_has_room_for(start, length).is_ok() => {
                Ok((position, start))
            }
            _ => Err(EditError::NoRoomForACopy { clip }),
        }
    }

    /// Whether there is room on the track for a copy of `clip` right next to it, on `side` of it: nothing else is at any of
    /// the frames the copy would be at, and it would not start before the start of the timeline.
    pub fn room_for_a_copy_of(&self, clip: ClipId, side: ClipSide) -> bool {
        self.where_a_copy_of_a_clip_would_go(clip, side).is_ok()
    }

    /// Puts a copy of `clip` on the same track, right next to it on `side` of it, and gives the copy's name. Everything about
    /// the clip is copied, and so is what is in its drawer: each rack is made again, of the same thing as before except that
    /// the keyframes of the clip itself are of the copy. So what the clip does to anything when it is there, the copy does
    /// when it is there. (The racks kept in other drawers that animate the clip are not copied.) Nothing is changed if there
    /// is no room for the copy.
    pub fn duplicate_clip(&mut self, clip: ClipId, side: ClipSide) -> Result<ClipId, EditError> {
        let (position, start) = self.where_a_copy_of_a_clip_would_go(clip, side)?;
        self.tracks[position]
            .insert_a_copy_of_the_clip_at(clip, start)
            .ok_or(EditError::NoRoomForACopy { clip })
    }

    /// The title a copy of a track that is titled `title` is given, which no track has: the title with "copy" after it, and
    /// if a track has that title already, a number after that ("captions copy", "captions copy 2", "captions copy 3"). The
    /// copy of a copy is a copy of what it is a copy of ("captions copy 2" is copied as "captions copy 3", and not as
    /// "captions copy 2 copy"). (A track that is called by just a number, as the tracks that are made are, has "copy" after it
    /// like any other: "track 4 copy".)
    fn title_for_a_copy_of(&self, title: &str) -> String {
        let base = without_the_copy_ending(title);
        let first_try = if base.is_empty() {
            "copy".to_owned()
        } else {
            format!("{base} copy")
        };
        let is_taken = |candidate: &str| self.tracks.iter().any(|track| track.title() == candidate);
        if !is_taken(&first_try) {
            return first_try;
        }
        (2..)
            .map(|number| format!("{first_try} {number}"))
            .find(|candidate| !is_taken(candidate))
            .expect("there are not as many tracks as there are numbers")
    }

    /// Puts a copy of the track `track` right in front of it (`Above`) or right behind it (`Below`) and gives the copy's
    /// name. It has everything the track has: its loudness, speed, mute switch and height, and a copy of each clip with a copy
    /// of each rack in the drawer of the clip and in the drawer of the track. Those racks are of the same things as before,
    /// except that what animated the track animates the copy and what animated one of its clips animates the copy of that
    /// clip. Its title is the track's with "copy" after it (see `title_for_a_copy_of`), so that the tracks can be told apart
    /// in the words that say what a rack of keyframes animates, and by anyone looking for the copy.
    pub fn duplicate_track(&mut self, track: TrackId, side: TrackSide) -> Result<TrackId, EditError> {
        let position = self.track_position(track).ok_or(EditError::UnknownTrack { track })?;
        let title = self.title_for_a_copy_of(self.tracks[position].title());
        let id = TrackId::new(self.next_track_number);
        self.next_track_number += 1;
        let copy = self.tracks[position].copy_as(id, title);
        let place = match side {
            TrackSide::Above => position,
            TrackSide::Below => position + 1,
        };
        self.tracks.insert(place, copy);
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_copy_ending_of_a_title_is_taken_off_with_the_number_that_follows_it() {
        assert_eq!(without_the_copy_ending("captions"), "captions");
        assert_eq!(without_the_copy_ending("captions copy"), "captions");
        assert_eq!(without_the_copy_ending("captions copy 2"), "captions");
        assert_eq!(without_the_copy_ending("captions copy 12"), "captions");
        assert_eq!(without_the_copy_ending("track 1"), "track 1", "a number is not a copy");
        assert_eq!(
            without_the_copy_ending("copy 2"),
            "copy 2",
            "nothing before the word is not a copy of anything"
        );
        assert_eq!(
            without_the_copy_ending("captions copy2"),
            "captions copy2",
            "the number has a space before it"
        );
        assert_eq!(without_the_copy_ending("recopy"), "recopy", "only the word copy");
        assert_eq!(without_the_copy_ending("copy"), "copy");
        assert_eq!(without_the_copy_ending(""), "");
        assert_eq!(without_the_copy_ending("a copy copy"), "a copy", "one ending at a time");
    }
}
