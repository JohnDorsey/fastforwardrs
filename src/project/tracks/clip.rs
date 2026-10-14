//! A clip: one thing on a track. It is a piece of a video, a piece of a sound file, a picture, a
//! text object or a keyframe crate.

use super::crate_clip::CrateClip;
use super::image_clip::ImageClip;
use super::media_clip::MediaClip;
use super::placement::Placement;
use super::text_object::TextClip;
use crate::project::{FrameRate, SourceMediaIdentifier, SpeedFactor};

/// What kind of thing a clip is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClipKind {
    Video,
    Audio,
    Image,
    Text,
    /// A keyframe crate: nothing to see or hear, only racks of keyframes.
    Crate,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Clip {
    /// A piece of a video: its pictures, and its sound if it has any.
    Video(MediaClip),
    /// A piece of a sound file.
    Audio(MediaClip),
    /// A picture, shown for a while.
    Image(ImageClip),
    Text(TextClip),
    /// A keyframe crate: a stretch of the track that holds racks of keyframes, and nothing else.
    Crate(CrateClip),
}

impl Clip {
    pub fn kind(&self) -> ClipKind {
        match self {
            Self::Video(_) => ClipKind::Video,
            Self::Audio(_) => ClipKind::Audio,
            Self::Image(_) => ClipKind::Image,
            Self::Text(_) => ClipKind::Text,
            Self::Crate(_) => ClipKind::Crate,
        }
    }

    /// The part of the clip that comes from a file and plays, for a video or a sound clip.
    pub fn as_media(&self) -> Option<&MediaClip> {
        match self {
            Self::Video(media) | Self::Audio(media) => Some(media),
            Self::Image(_) | Self::Text(_) | Self::Crate(_) => None,
        }
    }

    pub fn as_text(&self) -> Option<&TextClip> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_image(&self) -> Option<&ImageClip> {
        match self {
            Self::Image(image) => Some(image),
            _ => None,
        }
    }

    /// The file the clip is made from, if it is made from one: a video, a sound or a picture.
    pub fn source(&self) -> Option<SourceMediaIdentifier> {
        match self {
            Self::Video(media) | Self::Audio(media) => Some(media.source()),
            Self::Image(image) => Some(image.source()),
            Self::Text(_) | Self::Crate(_) => None,
        }
    }

    /// Where the clip is on the picture, if it says. A text clip always does; a video clip does
    /// once it has been moved, resized or turned (before that it is fitted inside the picture);
    /// a sound clip is not on the picture at all.
    pub fn placement(&self) -> Option<Placement> {
        match self {
            Self::Video(media) => media.placement(),
            Self::Image(image) => image.placement(),
            Self::Audio(_) | Self::Crate(_) => None,
            Self::Text(text) => Some(text.object.placement()),
        }
    }

    /// The same clip, put at `placement` on the picture. `None` for a sound clip, which is not on it.
    #[must_use]
    pub fn with_placement(&self, placement: Placement) -> Option<Clip> {
        match self {
            Self::Video(media) => Some(Self::Video(media.with_placement(Some(placement)))),
            Self::Image(image) => Some(Self::Image(image.with_placement(Some(placement)))),
            Self::Audio(_) | Self::Crate(_) => None,
            Self::Text(text) => {
                let mut moved = text.clone();
                moved.object = text.object.with_placement(placement);
                Some(Self::Text(moved))
            }
        }
    }

    /// How many frames the clip is shown for, if that is something it says itself (a text object
    /// and a picture). A video or a sound lasts as long as the part of its file it is made from, and a keyframe crate from its
    /// first keyframe to its last.
    pub fn duration_in_frames_it_says(&self) -> Option<u64> {
        match self {
            Self::Text(text) => Some(text.duration_in_frames()),
            Self::Image(image) => Some(image.duration_in_frames()),
            Self::Video(_) | Self::Audio(_) | Self::Crate(_) => None,
        }
    }

    /// The same clip shown for `duration_in_frames` frames. `None` for a clip that does not say how
    /// long it lasts, and for no time at all.
    #[must_use]
    pub fn with_duration_in_frames_it_says(&self, duration_in_frames: u64) -> Option<Clip> {
        match self {
            Self::Text(text) => text.with_duration_in_frames(duration_in_frames).map(Self::Text),
            Self::Image(image) => image.with_duration_in_frames(duration_in_frames).map(Self::Image),
            Self::Video(_) | Self::Audio(_) | Self::Crate(_) => None,
        }
    }

    /// The clip as it is when its project runs at the frame rate `to` and not `from`: it lasts as long
    /// as it did, counted again at the new rate (a picture or a text object for at least one frame).
    #[must_use]
    pub fn retimed(&self, from: FrameRate, to: FrameRate) -> Clip {
        match self {
            Self::Video(media) => Self::Video(media.retimed(from, to)),
            Self::Audio(media) => Self::Audio(media.retimed(from, to)),
            Self::Image(_) | Self::Text(_) => {
                let duration = self.duration_in_frames_it_says().unwrap_or(1);
                let retimed = from.convert_frame_count_to(duration, to).max(1);
                self.with_duration_in_frames_it_says(retimed)
                    .unwrap_or_else(|| self.clone())
            }
            // (A crate lasts as long as its keyframes need, which is worked out again from them, once they are counted again: see
            // `Project::fit_every_crate`.)
            Self::Crate(crate_clip) => Self::Crate(crate_clip.retimed(from, to)),
        }
    }

    /// Whether the clip comes from a file, and so has a speed and can have its sound changed.
    /// Clips that do (and only those) are carried along when the clip before them changes its
    /// length: see [`super::Track`].
    pub fn is_media(&self) -> bool {
        self.as_media().is_some()
    }

    /// The speed the clip plays at when it is on a track that plays at `track_speed`: the two
    /// multiplied. A text clip or a picture has no speed, so this is normal speed for it.
    pub fn effective_speed(&self, track_speed: SpeedFactor) -> SpeedFactor {
        match self.as_media() {
            Some(media) => media.speed().times(track_speed),
            None => SpeedFactor::NORMAL,
        }
    }

    /// How many frames of the timeline the clip occupies on a track that plays at `track_speed`.
    pub fn timeline_frame_count(&self, track_speed: SpeedFactor) -> u64 {
        match self {
            Self::Video(media) | Self::Audio(media) => media.timeline_frame_count(self.effective_speed(track_speed)),
            Self::Image(image) => image.duration_in_frames(),
            Self::Text(text) => text.duration_in_frames(),
            Self::Crate(crate_clip) => crate_clip.duration_in_frames(),
        }
    }

    /// Cuts the clip in two, so that the second half starts `frames_in` frames after the clip's
    /// first frame. Both halves keep everything else about the clip, and together last exactly
    /// as long as the clip did. `None` if that is not strictly inside the clip.
    pub fn split_after(&self, frames_in: u64, track_speed: SpeedFactor) -> Option<(Clip, Clip)> {
        match self {
            Self::Video(media) | Self::Audio(media) => {
                if frames_in == 0 || frames_in >= self.timeline_frame_count(track_speed) {
                    return None;
                }
                let split_source_frame =
                    media.source_frame_at_timeline_offset(self.effective_speed(track_speed), frames_in)?;
                let (first, second) = media.split_before_source_frame(split_source_frame)?;
                let (make_first, make_second) = match self {
                    Self::Video(_) => (Self::Video(first), Self::Video(second)),
                    _ => (Self::Audio(first), Self::Audio(second)),
                };
                Some((make_first, make_second))
            }
            Self::Image(image) => image
                .split_after(frames_in)
                .map(|(first, second)| (Self::Image(first), Self::Image(second))),
            Self::Text(text) => text
                .split_after(frames_in)
                .map(|(first, second)| (Self::Text(first), Self::Text(second))),
            Self::Crate(crate_clip) => crate_clip
                .split_after(frames_in)
                .map(|(first, second)| (Self::Crate(first), Self::Crate(second))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::tracks::placement::{Angle, CanvasArea};
    use crate::project::tracks::text_object::TextObject;
    use crate::project::{SourceFrameIndex, SourceMediaIdentifier};

    fn speed(factor: u32) -> SpeedFactor {
        SpeedFactor::new(factor).unwrap()
    }

    fn media(first: u64, end: u64, factor: u32) -> MediaClip {
        MediaClip::new(
            SourceMediaIdentifier::new(1),
            SourceFrameIndex::new(first),
            SourceFrameIndex::new(end),
            speed(factor),
        )
        .unwrap()
    }

    fn text(duration: u64) -> Clip {
        Clip::Text(TextClip::new(TextObject::default(), duration).unwrap())
    }

    const PICTURE_FILE: SourceMediaIdentifier = SourceMediaIdentifier::new(4);

    fn picture(duration: u64) -> Clip {
        Clip::Image(ImageClip::new(PICTURE_FILE, duration).unwrap())
    }

    #[test]
    fn each_kind_of_clip_says_what_it_is() {
        assert_eq!(Clip::Video(media(0, 10, 1)).kind(), ClipKind::Video);
        assert_eq!(Clip::Audio(media(0, 10, 1)).kind(), ClipKind::Audio);
        assert_eq!(text(5).kind(), ClipKind::Text);
        assert!(Clip::Video(media(0, 10, 1)).is_media() && Clip::Audio(media(0, 10, 1)).is_media());
        assert!(!text(5).is_media());
        assert!(text(5).as_text().is_some() && text(5).as_media().is_none());
    }

    fn somewhere() -> Placement {
        Placement::new(CanvasArea::new(0.2, 0.3, 0.4, 0.3), Angle::from_degrees(20.0))
    }

    #[test]
    fn a_text_clip_is_always_somewhere_on_the_picture_a_video_only_once_it_was_placed_and_a_sound_never_is() {
        assert_eq!(text(5).placement(), Some(TextObject::default().placement()));
        assert_eq!(Clip::Video(media(0, 10, 1)).placement(), None);
        assert_eq!(
            Clip::Video(media(0, 10, 1).with_placement(Some(somewhere()))).placement(),
            Some(somewhere())
        );
        assert_eq!(Clip::Audio(media(0, 10, 1)).placement(), None);
    }

    #[test]
    fn a_clip_that_is_on_the_picture_can_be_put_somewhere_else_and_keeps_all_that_it_is_otherwise() {
        let video = Clip::Video(media(100, 300, 4));
        let placed = video.with_placement(somewhere()).unwrap();
        assert_eq!(placed.placement(), Some(somewhere()));
        assert_eq!(placed.kind(), ClipKind::Video);
        assert_eq!(
            placed.timeline_frame_count(speed(1)),
            video.timeline_frame_count(speed(1))
        );

        let words = Clip::Text(
            TextClip::new(
                TextObject {
                    text: "Hello".to_owned(),
                    ..TextObject::default()
                },
                90,
            )
            .unwrap(),
        );
        let moved = words.with_placement(somewhere()).unwrap();
        assert_eq!(moved.placement(), Some(somewhere()));
        assert_eq!(moved.as_text().unwrap().object.text, "Hello");
        assert_eq!(moved.timeline_frame_count(speed(1)), 90);
    }

    #[test]
    fn a_sound_clip_cannot_be_put_anywhere_on_the_picture() {
        assert_eq!(Clip::Audio(media(0, 10, 1)).with_placement(somewhere()), None);
    }

    #[test]
    fn a_media_clip_plays_at_its_own_speed_times_the_tracks() {
        let clip = Clip::Video(media(0, 600, 3));
        assert_eq!(clip.effective_speed(speed(1)), speed(3));
        assert_eq!(clip.effective_speed(speed(2)), speed(6));
        assert_eq!(clip.timeline_frame_count(speed(1)), 200);
        assert_eq!(clip.timeline_frame_count(speed(2)), 100);
    }

    #[test]
    fn a_text_clip_lasts_exactly_as_long_as_it_says_whatever_the_speed_of_its_track() {
        let clip = text(90);
        for track_speed in [1, 2, 7, 1000] {
            assert_eq!(clip.timeline_frame_count(speed(track_speed)), 90);
            assert_eq!(clip.effective_speed(speed(track_speed)), SpeedFactor::NORMAL);
        }
    }

    #[test]
    fn splitting_a_media_clip_after_some_frames_cuts_it_at_the_source_frame_shown_there() {
        // Frame 7 of a clip at 4x shows source frame 100 + 7 * 4 = 128.
        let (first, second) = Clip::Video(media(100, 300, 4)).split_after(7, speed(1)).unwrap();
        assert_eq!(first, Clip::Video(media(100, 128, 4)));
        assert_eq!(second, Clip::Video(media(128, 300, 4)));
        // A sound clip stays a sound clip.
        let (first, _) = Clip::Audio(media(0, 100, 1)).split_after(10, speed(1)).unwrap();
        assert_eq!(first.kind(), ClipKind::Audio);
    }

    #[test]
    fn splitting_on_a_track_with_its_own_speed_uses_the_effective_speed() {
        // On a 2x track the clip's 3x makes 6x: frame 5 shows source frame 30.
        let (first, second) = Clip::Video(media(0, 600, 3)).split_after(5, speed(2)).unwrap();
        assert_eq!(first, Clip::Video(media(0, 30, 3)));
        assert_eq!(second, Clip::Video(media(30, 600, 3)));
    }

    #[test]
    fn the_halves_of_a_split_clip_last_as_long_together_as_the_clip_did() {
        for clip_speed in [1u32, 2, 3, 5, 7, 16] {
            for track_speed in [1u32, 2, 3] {
                let clip = Clip::Video(media(10, 10 + 101, clip_speed));
                let total = clip.timeline_frame_count(speed(track_speed));
                for frames_in in 1..total {
                    let (first, second) = clip.split_after(frames_in, speed(track_speed)).unwrap();
                    assert_eq!(
                        first.timeline_frame_count(speed(track_speed))
                            + second.timeline_frame_count(speed(track_speed)),
                        total,
                        "{clip_speed}x clip on a {track_speed}x track cut after {frames_in} frames"
                    );
                    assert_eq!(first.timeline_frame_count(speed(track_speed)), frames_in);
                }
            }
        }
    }

    #[test]
    fn splitting_a_text_clip_splits_its_duration() {
        let (first, second) = text(90).split_after(30, speed(5)).unwrap();
        assert_eq!(first.timeline_frame_count(speed(1)), 30);
        assert_eq!(second.timeline_frame_count(speed(1)), 60);
    }

    #[test]
    fn a_picture_is_a_kind_of_clip_that_comes_from_a_file_but_does_not_play_it() {
        let clip = picture(60);
        assert_eq!(clip.kind(), ClipKind::Image);
        assert_eq!(clip.source(), Some(PICTURE_FILE));
        assert!(clip.as_image().is_some() && clip.as_media().is_none() && clip.as_text().is_none());
        assert!(
            !clip.is_media(),
            "it has no speed and no sound, and what is after it does not move with it"
        );
        assert_eq!(
            Clip::Video(media(0, 10, 1)).source(),
            Some(SourceMediaIdentifier::new(1))
        );
        assert_eq!(
            Clip::Audio(media(0, 10, 1)).source(),
            Some(SourceMediaIdentifier::new(1))
        );
        assert_eq!(text(5).source(), None, "text is not made from a file");
    }

    #[test]
    fn a_picture_lasts_exactly_as_long_as_it_says_whatever_the_speed_of_its_track() {
        let clip = picture(90);
        for track_speed in [1, 2, 7, 1000] {
            assert_eq!(clip.timeline_frame_count(speed(track_speed)), 90);
            assert_eq!(clip.effective_speed(speed(track_speed)), SpeedFactor::NORMAL);
        }
    }

    #[test]
    fn splitting_a_picture_splits_its_duration_and_keeps_where_it_is() {
        let placed = Clip::Image(
            ImageClip::new(PICTURE_FILE, 90)
                .unwrap()
                .with_placement(Some(somewhere())),
        );
        let (first, second) = placed.split_after(30, speed(5)).unwrap();
        assert_eq!(first.timeline_frame_count(speed(1)), 30);
        assert_eq!(second.timeline_frame_count(speed(1)), 60);
        assert_eq!(
            (first.placement(), second.placement()),
            (Some(somewhere()), Some(somewhere()))
        );
        assert_eq!((first.kind(), second.kind()), (ClipKind::Image, ClipKind::Image));
    }

    #[test]
    fn a_picture_is_on_the_picture_and_can_be_put_somewhere() {
        assert_eq!(
            picture(30).placement(),
            None,
            "fitted inside the picture until it is put somewhere"
        );
        let placed = picture(30).with_placement(somewhere()).unwrap();
        assert_eq!(placed.placement(), Some(somewhere()));
        assert_eq!(placed.timeline_frame_count(speed(1)), 30);
        assert_eq!(placed.source(), Some(PICTURE_FILE));
    }

    #[test]
    fn text_and_pictures_say_how_long_they_last_and_can_be_made_to_last_another_time_but_videos_and_sounds_do_not() {
        for clip in [text(40), picture(40)] {
            assert_eq!(clip.duration_in_frames_it_says(), Some(40));
            let longer = clip.with_duration_in_frames_it_says(100).unwrap();
            assert_eq!(longer.duration_in_frames_it_says(), Some(100));
            assert_eq!(longer.kind(), clip.kind());
            assert_eq!(clip.with_duration_in_frames_it_says(0), None, "not for no time");
        }
        for clip in [
            Clip::Video(media(0, 10, 1)),
            Clip::Audio(media(0, 10, 1)),
            Clip::Crate(super::super::CrateClip::new(10).unwrap()),
        ] {
            assert_eq!(clip.duration_in_frames_it_says(), None);
            assert_eq!(clip.with_duration_in_frames_it_says(5), None);
        }
    }

    #[test]
    fn a_clip_that_says_how_long_it_lasts_is_counted_again_at_another_frame_rate_for_as_long_in_time_and_a_frame_at_the_least()
     {
        let rate = |frames_per_second| FrameRate::from_whole_frames_per_second(frames_per_second).unwrap();
        let crate_lasting = |frames| Clip::Crate(super::super::CrateClip::new(frames).unwrap());
        let of_each_kind = |frames: u64| [text(frames), picture(frames), crate_lasting(frames)];

        for ((before, at_60), at_15) in of_each_kind(31).into_iter().zip(of_each_kind(62)).zip(of_each_kind(16)) {
            assert_eq!(before.retimed(rate(30), rate(60)), at_60, "{:?}", before.kind());
            assert_eq!(before.retimed(rate(30), rate(15)), at_15, "{:?}", before.kind());
        }
        for before in of_each_kind(1) {
            assert_eq!(
                before.retimed(rate(30), rate(12)).timeline_frame_count(speed(1)),
                1,
                "{:?} lasts a frame, though that would be no time at the new rate",
                before.kind()
            );
        }
    }

    #[test]
    fn a_clip_cannot_be_split_at_its_start_or_end_or_beyond() {
        for clip in [Clip::Video(media(0, 100, 1)), text(100), picture(100)] {
            for frames_in in [0, 100, 101, 5000] {
                assert_eq!(
                    clip.split_after(frames_in, speed(1)),
                    None,
                    "{:?} after {frames_in}",
                    clip.kind()
                );
            }
        }
    }
}
