// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The files a project is made from (videos, sound files and pictures), and how the project finds
//! them again.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{FrameRate, InputColourOverride};

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

impl SourceVideoProperties {
    /// How many frames of a project that runs at `frame_rate` the video lasts: its own pictures counted
    /// again at that rate, which is how many pictures it has once it is made to run at it (as it is,
    /// in the preview and in the export, whatever rate it was made at).
    pub fn frame_count_at(&self, frame_rate: FrameRate) -> u64 {
        self.frame_rate
            .convert_frame_count_to(self.total_frame_count, frame_rate)
    }
}

/// Where a source video file lives. The project does not embed the video, unless the user chose to include it in the
/// project file (see [`Self::included_as`]); it remembers the place in two ways so that it can usually find the file again: the
/// absolute path, and a path relative to the folder holding the project file (which still works if the project and the video were
/// moved together).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMediaLink {
    pub absolute_path: PathBuf,
    /// Relative to the folder containing the project file. `None` until the project has been
    /// saved, or when no relative path exists (for example the video is on another drive).
    pub relative_path: Option<PathBuf>,
    /// The name of the entry of the project file (an archive, which is a `.ffrs` file) that has the file in it, if the user chose to
    /// include the file there. The file is then in the project file, and the two paths above are where it was when it was
    /// included: they are kept as they were, so that the file can be taken out of the project file and put back there.
    pub included_as: Option<String>,
}

impl SourceMediaLink {
    /// A link that only knows the absolute path (a video that was just chosen, in a project that
    /// has not been saved yet).
    pub fn from_absolute_path(absolute_path: PathBuf) -> Self {
        Self {
            absolute_path,
            relative_path: None,
            included_as: None,
        }
    }

    /// Whether the file is included in the project file.
    pub fn is_included(&self) -> bool {
        self.included_as.is_some()
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

/// What the project needs to remember about a picture (a photograph, a logo) so that it can still
/// lay out the picture while the file itself is missing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceImageProperties {
    /// The size of the picture as it is shown: after the file's own turning (if it says it is on
    /// its side) and the shape of its pixels have been applied.
    pub display_width_in_pixels: u32,
    pub display_height_in_pixels: u32,
}

/// What kind of file a source is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SourceMediaKind {
    Video,
    Audio,
    Image,
}

impl SourceMediaKind {
    /// What a file of this kind is called, with its "a", for saying in a message.
    pub fn description(self) -> &'static str {
        match self {
            Self::Video => "a video",
            Self::Audio => "a sound file",
            Self::Image => "a picture",
        }
    }
}

/// What a project knows about a source file besides where it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceMediaProperties {
    Video(SourceVideoProperties),
    Audio(SourceAudioProperties),
    Image(SourceImageProperties),
}

impl SourceMediaProperties {
    pub fn kind(&self) -> SourceMediaKind {
        match self {
            Self::Video(_) => SourceMediaKind::Video,
            Self::Audio(_) => SourceMediaKind::Audio,
            Self::Image(_) => SourceMediaKind::Image,
        }
    }

    /// The properties of the video, if the file is a video.
    pub fn video(&self) -> Option<&SourceVideoProperties> {
        match self {
            Self::Video(video) => Some(video),
            Self::Audio(_) | Self::Image(_) => None,
        }
    }

    /// How big the picture of the file is as it is shown, for a file that has one (a video, or an
    /// image).
    pub fn display_size_in_pixels(&self) -> Option<(u32, u32)> {
        match self {
            Self::Video(video) => Some((video.display_width_in_pixels, video.display_height_in_pixels)),
            Self::Image(image) => Some((image.display_width_in_pixels, image.display_height_in_pixels)),
            Self::Audio(_) => None,
        }
    }

    /// Whether the file has sound to play: a sound file always has, a video may, a picture never.
    pub fn has_sound(&self) -> bool {
        match self {
            Self::Video(video) => video.has_audio,
            Self::Audio(_) => true,
            Self::Image(_) => false,
        }
    }

    /// How many frames the file has, counted at the project's `frame_rate`: a video made to run at
    /// the project's rate, a sound as long as that, and a picture is a single one.
    pub fn frame_count_at(&self, frame_rate: FrameRate) -> u64 {
        match self {
            Self::Video(video) => video.frame_count_at(frame_rate),
            Self::Audio(audio) => audio.frame_count_at(frame_rate),
            Self::Image(_) => 1,
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
    /// What the user says the colours of the file are, instead of what the file says (all automatic until they say).
    pub colour: InputColourOverride,
}

impl SourceMedia {
    pub fn video(identifier: SourceMediaIdentifier, link: SourceMediaLink, properties: SourceVideoProperties) -> Self {
        Self {
            identifier,
            link,
            properties: SourceMediaProperties::Video(properties),
            colour: InputColourOverride::AUTOMATIC,
        }
    }

    pub fn audio(identifier: SourceMediaIdentifier, link: SourceMediaLink, properties: SourceAudioProperties) -> Self {
        Self {
            identifier,
            link,
            properties: SourceMediaProperties::Audio(properties),
            colour: InputColourOverride::AUTOMATIC,
        }
    }

    pub fn image(identifier: SourceMediaIdentifier, link: SourceMediaLink, properties: SourceImageProperties) -> Self {
        Self {
            identifier,
            link,
            properties: SourceMediaProperties::Image(properties),
            colour: InputColourOverride::AUTOMATIC,
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
            included_as: None,
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
    fn a_link_made_from_an_absolute_path_has_no_relative_path_yet_and_is_not_included() {
        let link = SourceMediaLink::from_absolute_path(PathBuf::from("/videos/holiday.mp4"));
        assert_eq!(link.relative_path, None);
        assert_eq!(link.included_as, None);
        assert!(!link.is_included());
    }

    #[test]
    fn a_link_of_a_file_that_is_included_is_one_and_still_has_the_places_the_file_was_at() {
        let included = SourceMediaLink {
            included_as: Some("resources/1-holiday.mp4".to_owned()),
            ..link(Some("../videos/holiday.mp4"))
        };

        assert!(included.is_included());
        assert_eq!(
            included.candidate_paths(Some(Path::new("/projects/summer"))),
            link(Some("../videos/holiday.mp4")).candidate_paths(Some(Path::new("/projects/summer"))),
            "where it was is what it was: what is done with those places is for the one that reads the project to say"
        );
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
    fn a_video_has_as_many_frames_as_it_lasts_at_the_rate_asked_for_and_a_sound_has_sound_a_video_only_if_it_says_so() {
        let video_properties = SourceVideoProperties {
            frame_rate: FrameRate::from_whole_frames_per_second(25).unwrap(),
            total_frame_count: 500,
            display_width_in_pixels: 640,
            display_height_in_pixels: 360,
            has_audio: false,
        };
        let video = SourceMediaProperties::Video(video_properties);
        let thirty = FrameRate::from_whole_frames_per_second(30).unwrap();
        // The video's own 500 pictures are 20 seconds, which is 600 frames of a project at 30.
        assert_eq!(
            video.frame_count_at(FrameRate::from_whole_frames_per_second(25).unwrap()),
            500
        );
        assert_eq!(video.frame_count_at(thirty), 600);
        assert_eq!(
            video.frame_count_at(FrameRate::from_whole_frames_per_second(20).unwrap()),
            400
        );
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

    #[test]
    fn a_picture_has_a_size_and_no_sound_and_counts_as_one_frame_whatever_the_frame_rate() {
        let picture = SourceMediaProperties::Image(SourceImageProperties {
            display_width_in_pixels: 800,
            display_height_in_pixels: 600,
        });
        assert_eq!(picture.kind(), SourceMediaKind::Image);
        assert_eq!(picture.display_size_in_pixels(), Some((800, 600)));
        assert!(picture.video().is_none());
        assert!(!picture.has_sound());
        for frames_per_second in [24, 30, 60] {
            assert_eq!(
                picture.frame_count_at(FrameRate::from_whole_frames_per_second(frames_per_second).unwrap()),
                1
            );
        }
        let sound = SourceMediaProperties::Audio(SourceAudioProperties {
            duration_in_microseconds: 1,
        });
        assert_eq!(sound.display_size_in_pixels(), None, "a sound has no picture");
    }

    #[test]
    fn the_display_size_of_a_video_is_the_size_of_its_picture_after_turning_and_pixel_shape() {
        let video = SourceMediaProperties::Video(SourceVideoProperties {
            frame_rate: FrameRate::from_whole_frames_per_second(25).unwrap(),
            total_frame_count: 5,
            display_width_in_pixels: 720,
            display_height_in_pixels: 1280,
            has_audio: false,
        });
        assert_eq!(video.display_size_in_pixels(), Some((720, 1280)));
    }

    #[test]
    fn each_kind_of_file_is_described_in_words_with_its_article() {
        assert_eq!(SourceMediaKind::Video.description(), "a video");
        assert_eq!(SourceMediaKind::Audio.description(), "a sound file");
        assert_eq!(SourceMediaKind::Image.description(), "a picture");
    }

    #[test]
    fn a_picture_made_into_a_source_is_a_picture() {
        let source = SourceMedia::image(
            SourceMediaIdentifier::new(5),
            SourceMediaLink::from_absolute_path(PathBuf::from("/pictures/logo.png")),
            SourceImageProperties {
                display_width_in_pixels: 64,
                display_height_in_pixels: 64,
            },
        );
        assert_eq!(source.kind(), SourceMediaKind::Image);
    }
}
