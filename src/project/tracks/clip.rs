//! A clip: one thing on a track. It is a piece of a video, a piece of a sound file, or a text
//! object.

use super::media_clip::MediaClip;
use super::text_object::TextClip;
use crate::project::SpeedFactor;

/// What kind of thing a clip is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClipKind {
    Video,
    Audio,
    Text,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Clip {
    /// A piece of a video: its pictures, and its sound if it has any.
    Video(MediaClip),
    /// A piece of a sound file.
    Audio(MediaClip),
    Text(TextClip),
}

impl Clip {
    pub fn kind(&self) -> ClipKind {
        match self {
            Self::Video(_) => ClipKind::Video,
            Self::Audio(_) => ClipKind::Audio,
            Self::Text(_) => ClipKind::Text,
        }
    }

    /// The part of the clip that comes from a file, for a video or a sound clip.
    pub fn as_media(&self) -> Option<&MediaClip> {
        match self {
            Self::Video(media) | Self::Audio(media) => Some(media),
            Self::Text(_) => None,
        }
    }

    pub fn as_text(&self) -> Option<&TextClip> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }

    /// Whether the clip comes from a file, and so has a speed and can have its sound changed.
    /// Clips that do (and only those) are carried along when the clip before them changes its
    /// length: see [`super::Track`].
    pub fn is_media(&self) -> bool {
        self.as_media().is_some()
    }

    /// The speed the clip plays at when it is on a track that plays at `track_speed`: the two
    /// multiplied. A text clip has no speed, so this is normal speed for it.
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
            Self::Text(text) => text.duration_in_frames(),
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
            Self::Text(text) => text
                .split_after(frames_in)
                .map(|(first, second)| (Self::Text(first), Self::Text(second))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn each_kind_of_clip_says_what_it_is() {
        assert_eq!(Clip::Video(media(0, 10, 1)).kind(), ClipKind::Video);
        assert_eq!(Clip::Audio(media(0, 10, 1)).kind(), ClipKind::Audio);
        assert_eq!(text(5).kind(), ClipKind::Text);
        assert!(Clip::Video(media(0, 10, 1)).is_media() && Clip::Audio(media(0, 10, 1)).is_media());
        assert!(!text(5).is_media());
        assert!(text(5).as_text().is_some() && text(5).as_media().is_none());
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
    fn a_clip_cannot_be_split_at_its_start_or_end_or_beyond() {
        for clip in [Clip::Video(media(0, 100, 1)), text(100)] {
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
