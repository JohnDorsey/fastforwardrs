//! The files a project is made from (videos, and sound files), and how the project finds them again.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::FrameRate;

/// Tells the source files of one project apart. Clips refer to their file by this number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SourceMediaIdentifier(u32);

impl SourceMediaIdentifier {
    pub const fn new(number: u32) -> Self {
        Self(number)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

impl fmt::Display for SourceMediaIdentifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// What the project needs to remember about a source video so that it can still lay out the
/// timeline, count frames and show timecodes while the video file itself is missing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceVideoProperties {
    pub frame_rate: FrameRate,
    /// How many frames the video has in total.
    pub total_frame_count: u64,
    /// The size of the picture as it is shown: after the video's rotation and non-square pixel
    /// shape (if it has either) have been applied.
    pub display_width_in_pixels: u32,
    pub display_height_in_pixels: u32,
    pub has_audio: bool,
}

/// Where a source video file lives. The project never embeds the video; it remembers the place in
/// two ways so that it can usually find the file again: the absolute path, and a path relative to
/// the folder holding the project file (which still works if the project and the video were moved
/// together).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMediaLink {
    pub absolute_path: PathBuf,
    /// Relative to the folder containing the project file. `None` until the project has been
    /// saved, or when no relative path exists (for example the video is on another drive).
    pub relative_path: Option<PathBuf>,
}

impl SourceMediaLink {
    /// A link that only knows the absolute path (a video that was just chosen, in a project that
    /// has not been saved yet).
    pub fn from_absolute_path(absolute_path: PathBuf) -> Self {
        Self {
            absolute_path,
            relative_path: None,
        }
    }

    /// The places the video may be, best guess first: the absolute path, then the relative path
    /// resolved against `project_directory`.
    pub fn candidate_paths(&self, project_directory: Option<&Path>) -> Vec<PathBuf> {
        let mut candidates = vec![self.absolute_path.clone()];
        if let (Some(relative_path), Some(project_directory)) = (&self.relative_path, project_directory) {
            let resolved_relative_path = project_directory.join(relative_path);
            if resolved_relative_path != self.absolute_path {
                candidates.push(resolved_relative_path);
            }
        }
        candidates
    }
}

/// What the project needs to remember about a sound file so that it can still lay out the
/// timeline while the file itself is missing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceAudioProperties {
    /// How long the sound lasts, in whole microseconds.
    pub duration_in_microseconds: u64,
}

impl SourceAudioProperties {
    /// How many frames of a video that runs at `frame_rate` the sound lasts for, counting a part
    /// of a frame at the end as a frame.
    pub fn frame_count_at(&self, frame_rate: FrameRate) -> u64 {
        const MICROSECONDS_PER_SECOND: u128 = 1_000_000;
        let frames_times_microseconds = u128::from(self.duration_in_microseconds) * u128::from(frame_rate.numerator());
        let microseconds_per_frame_denominator = MICROSECONDS_PER_SECOND * u128::from(frame_rate.denominator());
        u64::try_from(frames_times_microseconds.div_ceil(microseconds_per_frame_denominator)).unwrap_or(u64::MAX)
    }
}

/// What kind of file a source is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SourceMediaKind {
    Video,
    Audio,
}

/// What a project knows about a source file besides where it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceMediaProperties {
    Video(SourceVideoProperties),
    Audio(SourceAudioProperties),
}

impl SourceMediaProperties {
    pub fn kind(&self) -> SourceMediaKind {
        match self {
            Self::Video(_) => SourceMediaKind::Video,
            Self::Audio(_) => SourceMediaKind::Audio,
        }
    }

    /// The properties of the picture, if the file is a video.
    pub fn video(&self) -> Option<&SourceVideoProperties> {
        match self {
            Self::Video(video) => Some(video),
            Self::Audio(_) => None,
        }
    }

    /// Whether the file has sound to play: a sound file always has, a video may.
    pub fn has_sound(&self) -> bool {
        match self {
            Self::Video(video) => video.has_audio,
            Self::Audio(_) => true,
        }
    }

    /// How many frames the file has, counted at the project's `frame_rate`. (A video always runs
    /// at the project's frame rate, so for it this is its own number of frames.)
    pub fn frame_count_at(&self, frame_rate: FrameRate) -> u64 {
        match self {
            Self::Video(video) => video.total_frame_count,
            Self::Audio(audio) => audio.frame_count_at(frame_rate),
        }
    }
}

/// A source file of a project: a link to a file on disk plus the properties it had when it was
/// imported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMedia {
    pub identifier: SourceMediaIdentifier,
    pub link: SourceMediaLink,
    pub properties: SourceMediaProperties,
}

impl SourceMedia {
    pub fn video(identifier: SourceMediaIdentifier, link: SourceMediaLink, properties: SourceVideoProperties) -> Self {
        Self {
            identifier,
            link,
            properties: SourceMediaProperties::Video(properties),
        }
    }

    pub fn audio(identifier: SourceMediaIdentifier, link: SourceMediaLink, properties: SourceAudioProperties) -> Self {
        Self {
            identifier,
            link,
            properties: SourceMediaProperties::Audio(properties),
        }
    }

    pub fn kind(&self) -> SourceMediaKind {
        self.properties.kind()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(relative_path: Option<&str>) -> SourceMediaLink {
        SourceMediaLink {
            absolute_path: PathBuf::from("/videos/holiday.mp4"),
            relative_path: relative_path.map(PathBuf::from),
        }
    }

    #[test]
    fn the_absolute_path_is_tried_before_the_relative_one() {
        let candidates = link(Some("../videos/holiday.mp4")).candidate_paths(Some(Path::new("/projects/summer")));
        assert_eq!(
            candidates,
            vec![
                PathBuf::from("/videos/holiday.mp4"),
                PathBuf::from("/projects/summer").join("../videos/holiday.mp4"),
            ]
        );
    }

    #[test]
    fn without_a_relative_path_or_project_folder_only_the_absolute_path_is_a_candidate() {
        let expected = vec![PathBuf::from("/videos/holiday.mp4")];
        assert_eq!(link(None).candidate_paths(Some(Path::new("/projects"))), expected);
        assert_eq!(link(Some("a.mp4")).candidate_paths(None), expected);
    }

    #[test]
    fn a_relative_path_that_resolves_to_the_absolute_path_is_not_listed_twice() {
        let mut link = link(Some("holiday.mp4"));
        link.absolute_path = PathBuf::from("/projects/summer").join("holiday.mp4");
        assert_eq!(link.candidate_paths(Some(Path::new("/projects/summer"))).len(), 1);
    }

    #[test]
    fn a_link_made_from_an_absolute_path_has_no_relative_path_yet() {
        let link = SourceMediaLink::from_absolute_path(PathBuf::from("/videos/holiday.mp4"));
        assert_eq!(link.relative_path, None);
    }

    #[test]
    fn the_length_of_a_sound_in_frames_rounds_a_part_of_a_frame_up() {
        let thirty = FrameRate::from_whole_frames_per_second(30).unwrap();
        let sound = |microseconds| SourceAudioProperties {
            duration_in_microseconds: microseconds,
        };
        assert_eq!(sound(1_000_000).frame_count_at(thirty), 30);
        assert_eq!(
            sound(1_000_001).frame_count_at(thirty),
            31,
            "a little over is another frame"
        );
        assert_eq!(sound(33_333).frame_count_at(thirty), 1);
        assert_eq!(sound(33_334).frame_count_at(thirty), 2);
        assert_eq!(sound(0).frame_count_at(thirty), 0);
        let ntsc = FrameRate::new(30_000, 1001).unwrap();
        assert_eq!(
            sound(10_000_000).frame_count_at(ntsc),
            300,
            "10 s at 29.97 fps is 299.7 frames"
        );
        assert_eq!(
            sound(u64::MAX).frame_count_at(FrameRate::new(u32::MAX, 1).unwrap()),
            u64::MAX,
            "an absurd length does not wrap round"
        );
    }

    #[test]
    fn a_video_has_the_number_of_frames_it_says_and_a_sound_has_sound_a_video_only_if_it_says_so() {
        let video_properties = SourceVideoProperties {
            frame_rate: FrameRate::from_whole_frames_per_second(25).unwrap(),
            total_frame_count: 500,
            display_width_in_pixels: 640,
            display_height_in_pixels: 360,
            has_audio: false,
        };
        let video = SourceMediaProperties::Video(video_properties);
        let thirty = FrameRate::from_whole_frames_per_second(30).unwrap();
        assert_eq!(video.frame_count_at(thirty), 500);
        assert!(!video.has_sound());
        assert!(
            SourceMediaProperties::Video(SourceVideoProperties {
                has_audio: true,
                ..video_properties
            })
            .has_sound()
        );
        let audio = SourceMediaProperties::Audio(SourceAudioProperties {
            duration_in_microseconds: 2_000_000,
        });
        assert!(audio.has_sound());
        assert_eq!(audio.frame_count_at(thirty), 60);
        assert_eq!(
            (video.kind(), audio.kind()),
            (SourceMediaKind::Video, SourceMediaKind::Audio)
        );
        assert!(video.video().is_some() && audio.video().is_none());
    }
}
