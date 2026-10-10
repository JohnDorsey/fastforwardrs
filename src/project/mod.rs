//! What a project *is*: the source video, the clips its video track has been split into, the
//! speed of each clip, where the fast-forward symbol goes, and all of the timeline arithmetic.
//!
//! Nothing in this module touches files, processes or the screen, so all of it can be tested
//! quickly and thoroughly.
//!
//! Positions on both the source video and the edited timeline are whole frame numbers
//! ([`SourceFrameIndex`], [`TimelineFrameIndex`]). Whole frames keep every calculation exact and
//! make the live preview show precisely the frames the export will contain.

mod clip;
mod frame_index;
mod frame_rate;
mod settings;
mod source_video;
mod speed_factor;
mod speed_indicator_layout;
mod timecode;
mod video_track;

use std::collections::HashSet;

use thiserror::Error;

pub use clip::{Clip, EmptyClip};
pub use frame_index::{SourceFrameIndex, TimelineFrameIndex};
pub use frame_rate::{FrameRate, InvalidFrameRate};
pub use settings::{BOOLEAN_SETTINGS, BooleanSetting, ProjectSettings, Settings};
pub use source_video::{SourceVideo, SourceVideoIdentifier, SourceVideoLink, SourceVideoProperties};
pub use speed_factor::{SpeedFactor, ZeroSpeedFactor};
pub use speed_indicator_layout::SpeedIndicatorLayout;
pub use timecode::format_timecode;
pub use video_track::{ClipPlacement, SplitError, SplitOutcome, TimelineLocation, UnknownClip, VideoTrack};

/// Everything the user has made: which video(s) it is made from and how they have been cut up
/// and sped up, and the settings the project has chosen for itself. This is exactly what a
/// project file stores.
#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    source_videos: Vec<SourceVideo>,
    video_track: VideoTrack,
    speed_indicator_layout: SpeedIndicatorLayout,
    settings: ProjectSettings,
}

/// Returned when the parts offered to [`Project::from_parts`] contradict each other.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum InconsistentProject {
    #[error("source video number {identifier} appears more than once")]
    DuplicateSourceVideo { identifier: SourceVideoIdentifier },
    #[error("a clip uses source video number {identifier}, but the project does not contain that video")]
    ClipUsesUnknownSourceVideo { identifier: SourceVideoIdentifier },
    #[error(
        "this version of fastforwardrs needs every source video in a project to have the same frame rate, but video {first_identifier} runs at {first_frame_rate} and video {other_identifier} at {other_frame_rate}"
    )]
    MixedFrameRates {
        first_identifier: SourceVideoIdentifier,
        first_frame_rate: FrameRate,
        other_identifier: SourceVideoIdentifier,
        other_frame_rate: FrameRate,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ImportError {
    #[error("this version of fastforwardrs supports only one source video per project")]
    ProjectAlreadyHasASourceVideo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum RelinkError {
    #[error("the project has no source video number {identifier}")]
    UnknownSourceVideo { identifier: SourceVideoIdentifier },
    #[error(
        "the chosen video runs at {chosen_frame_rate} frames per second, but this project was edited on a video running at {project_frame_rate}. Choose the original video (or a copy with the same frame rate) so that the edits still line up"
    )]
    DifferentFrameRate {
        project_frame_rate: FrameRate,
        chosen_frame_rate: FrameRate,
    },
}

/// How a relinked video compares with the one it replaced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelinkOutcome {
    pub total_frame_count_before: u64,
    pub total_frame_count_after: u64,
}

impl Project {
    /// A project with no video yet. It follows every global setting.
    pub fn empty() -> Self {
        Self {
            source_videos: Vec::new(),
            video_track: VideoTrack::default(),
            speed_indicator_layout: SpeedIndicatorLayout::DEFAULT,
            settings: ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS,
        }
    }

    /// Assembles a project from its parts (for example when loading a project file), checking
    /// that they agree with each other.
    pub fn from_parts(
        source_videos: Vec<SourceVideo>,
        video_track: VideoTrack,
        speed_indicator_layout: SpeedIndicatorLayout,
    ) -> Result<Self, InconsistentProject> {
        let mut seen_identifiers = HashSet::new();
        for source_video in &source_videos {
            if !seen_identifiers.insert(source_video.identifier) {
                return Err(InconsistentProject::DuplicateSourceVideo {
                    identifier: source_video.identifier,
                });
            }
        }
        for clip in video_track.clips() {
            if !seen_identifiers.contains(&clip.source_video_identifier()) {
                return Err(InconsistentProject::ClipUsesUnknownSourceVideo {
                    identifier: clip.source_video_identifier(),
                });
            }
        }
        if let Some((first_video, other_videos)) = source_videos.split_first() {
            let first_frame_rate = first_video.properties.frame_rate;
            if let Some(other_video) = other_videos
                .iter()
                .find(|video| video.properties.frame_rate != first_frame_rate)
            {
                return Err(InconsistentProject::MixedFrameRates {
                    first_identifier: first_video.identifier,
                    first_frame_rate,
                    other_identifier: other_video.identifier,
                    other_frame_rate: other_video.properties.frame_rate,
                });
            }
        }
        Ok(Self {
            source_videos,
            video_track,
            speed_indicator_layout,
            settings: ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS,
        })
    }

    /// The same project with these settings of its own (a project made by [`Self::from_parts`]
    /// follows all the global settings).
    #[must_use]
    pub fn with_settings(mut self, settings: ProjectSettings) -> Self {
        self.settings = settings;
        self
    }

    /// The settings this project has set for itself; the rest follow the global settings.
    pub fn settings(&self) -> ProjectSettings {
        self.settings
    }

    pub fn set_settings(&mut self, settings: ProjectSettings) {
        self.settings = settings;
    }

    pub fn source_videos(&self) -> &[SourceVideo] {
        &self.source_videos
    }

    pub fn source_video(&self, identifier: SourceVideoIdentifier) -> Option<&SourceVideo> {
        self.source_videos
            .iter()
            .find(|source_video| source_video.identifier == identifier)
    }

    /// The identifier a newly imported video should get: one more than the largest in use.
    pub fn next_source_video_identifier(&self) -> SourceVideoIdentifier {
        let largest_in_use = self
            .source_videos
            .iter()
            .map(|source_video| source_video.identifier.value())
            .max();
        SourceVideoIdentifier::new(largest_in_use.map_or(1, |largest| largest + 1))
    }

    pub fn video_track(&self) -> &VideoTrack {
        &self.video_track
    }

    pub fn speed_indicator_layout(&self) -> SpeedIndicatorLayout {
        self.speed_indicator_layout
    }

    pub fn set_speed_indicator_layout(&mut self, layout: SpeedIndicatorLayout) {
        self.speed_indicator_layout = layout;
    }

    /// The frame rate of the finished video: the frame rate of the source video(s). `None` while
    /// the project has no video.
    pub fn timeline_frame_rate(&self) -> Option<FrameRate> {
        self.source_videos
            .first()
            .map(|source_video| source_video.properties.frame_rate)
    }

    /// Adds a video to an empty project and lays it out as one clip covering the whole video.
    pub fn import_source_video(&mut self, source_video: SourceVideo) -> Result<(), ImportError> {
        if !self.source_videos.is_empty() {
            return Err(ImportError::ProjectAlreadyHasASourceVideo);
        }
        self.video_track =
            VideoTrack::with_single_clip_covering(source_video.identifier, source_video.properties.total_frame_count);
        self.source_videos.push(source_video);
        Ok(())
    }

    /// Cuts the video track at `timeline_frame`. See [`VideoTrack::split_at`].
    pub fn split_at(&mut self, timeline_frame: TimelineFrameIndex) -> Result<SplitOutcome, SplitError> {
        self.video_track.split_at(timeline_frame)
    }

    /// Plays the clip at `clip_index` at `speed_factor`. See [`VideoTrack::set_speed_factor`].
    pub fn set_clip_speed(&mut self, clip_index: usize, speed_factor: SpeedFactor) -> Result<(), UnknownClip> {
        self.video_track.set_speed_factor(clip_index, speed_factor)
    }

    /// Points a source video at a different file, for example after the original was moved. The
    /// clips are left exactly as they are: no edit is lost.
    ///
    /// The new file must run at the same frame rate as the old one, because the clips are stored
    /// as frame numbers. Its length may differ; the returned [`RelinkOutcome`] reports by how
    /// much so that the caller can tell the user.
    pub fn relink_source_video(
        &mut self,
        identifier: SourceVideoIdentifier,
        link: SourceVideoLink,
        properties: SourceVideoProperties,
    ) -> Result<RelinkOutcome, RelinkError> {
        let source_video = self
            .source_videos
            .iter_mut()
            .find(|source_video| source_video.identifier == identifier)
            .ok_or(RelinkError::UnknownSourceVideo { identifier })?;
        if source_video.properties.frame_rate != properties.frame_rate {
            return Err(RelinkError::DifferentFrameRate {
                project_frame_rate: source_video.properties.frame_rate,
                chosen_frame_rate: properties.frame_rate,
            });
        }
        let outcome = RelinkOutcome {
            total_frame_count_before: source_video.properties.total_frame_count,
            total_frame_count_after: properties.total_frame_count,
        };
        source_video.link = link;
        source_video.properties = properties;
        Ok(outcome)
    }
}

impl Default for Project {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn frame_rate(frames_per_second: u32) -> FrameRate {
        FrameRate::from_whole_frames_per_second(frames_per_second).unwrap()
    }

    fn source_video(identifier: u32, frames_per_second: u32, total_frame_count: u64) -> SourceVideo {
        SourceVideo {
            identifier: SourceVideoIdentifier::new(identifier),
            link: SourceVideoLink::from_absolute_path(PathBuf::from(format!("/videos/{identifier}.mp4"))),
            properties: SourceVideoProperties {
                frame_rate: frame_rate(frames_per_second),
                total_frame_count,
                display_width_in_pixels: 1280,
                display_height_in_pixels: 720,
                has_audio: true,
            },
        }
    }

    fn clip_of_video(identifier: u32, first: u64, end: u64) -> Clip {
        Clip::new(
            SourceVideoIdentifier::new(identifier),
            SourceFrameIndex::new(first),
            SourceFrameIndex::new(end),
            SpeedFactor::NORMAL,
        )
        .unwrap()
    }

    #[test]
    fn an_empty_project_has_no_video_no_frame_rate_and_no_timeline() {
        let project = Project::empty();
        assert!(project.source_videos().is_empty());
        assert_eq!(project.timeline_frame_rate(), None);
        assert_eq!(project.video_track().total_frame_count(), 0);
        assert_eq!(project.speed_indicator_layout(), SpeedIndicatorLayout::DEFAULT);
    }

    #[test]
    fn importing_a_video_lays_it_out_as_one_clip_covering_all_of_it() {
        let mut project = Project::empty();
        project.import_source_video(source_video(1, 30, 900)).unwrap();
        assert_eq!(project.timeline_frame_rate(), Some(frame_rate(30)));
        assert_eq!(project.video_track().clips(), &[clip_of_video(1, 0, 900)]);
        assert_eq!(project.video_track().total_frame_count(), 900);
    }

    #[test]
    fn a_second_video_cannot_be_imported_into_the_same_project() {
        let mut project = Project::empty();
        project.import_source_video(source_video(1, 30, 900)).unwrap();
        let before = project.clone();
        assert_eq!(
            project.import_source_video(source_video(2, 30, 100)),
            Err(ImportError::ProjectAlreadyHasASourceVideo)
        );
        assert_eq!(project, before);
    }

    #[test]
    fn the_next_identifier_follows_the_largest_one_in_use() {
        assert_eq!(
            Project::empty().next_source_video_identifier(),
            SourceVideoIdentifier::new(1)
        );
        let project = Project::from_parts(
            vec![source_video(3, 30, 10), source_video(7, 30, 10)],
            VideoTrack::default(),
            SpeedIndicatorLayout::DEFAULT,
        )
        .unwrap();
        assert_eq!(project.next_source_video_identifier(), SourceVideoIdentifier::new(8));
    }

    #[test]
    fn from_parts_accepts_a_consistent_project_with_several_videos() {
        let track = VideoTrack::new(vec![clip_of_video(1, 0, 100), clip_of_video(2, 0, 50)]);
        let project = Project::from_parts(
            vec![source_video(1, 30, 100), source_video(2, 30, 50)],
            track,
            SpeedIndicatorLayout::DEFAULT,
        )
        .unwrap();
        assert_eq!(project.source_videos().len(), 2);
        assert_eq!(project.video_track().total_frame_count(), 150);
        assert!(project.source_video(SourceVideoIdentifier::new(2)).is_some());
        assert!(project.source_video(SourceVideoIdentifier::new(3)).is_none());
    }

    #[test]
    fn from_parts_rejects_duplicate_identifiers() {
        let result = Project::from_parts(
            vec![source_video(1, 30, 100), source_video(1, 30, 50)],
            VideoTrack::default(),
            SpeedIndicatorLayout::DEFAULT,
        );
        assert_eq!(
            result,
            Err(InconsistentProject::DuplicateSourceVideo {
                identifier: SourceVideoIdentifier::new(1)
            })
        );
    }

    #[test]
    fn from_parts_rejects_clips_of_videos_the_project_does_not_have() {
        let result = Project::from_parts(
            vec![source_video(1, 30, 100)],
            VideoTrack::new(vec![clip_of_video(2, 0, 10)]),
            SpeedIndicatorLayout::DEFAULT,
        );
        assert_eq!(
            result,
            Err(InconsistentProject::ClipUsesUnknownSourceVideo {
                identifier: SourceVideoIdentifier::new(2)
            })
        );
    }

    #[test]
    fn from_parts_rejects_videos_with_different_frame_rates() {
        let result = Project::from_parts(
            vec![source_video(1, 30, 100), source_video(2, 25, 50)],
            VideoTrack::default(),
            SpeedIndicatorLayout::DEFAULT,
        );
        assert!(matches!(result, Err(InconsistentProject::MixedFrameRates { .. })));
    }

    #[test]
    fn splitting_and_speeding_up_go_through_to_the_video_track() {
        let mut project = Project::empty();
        project.import_source_video(source_video(1, 30, 300)).unwrap();
        project.split_at(TimelineFrameIndex::new(100)).unwrap();
        project.set_clip_speed(1, SpeedFactor::new(4).unwrap()).unwrap();
        assert_eq!(project.video_track().clips().len(), 2);
        assert_eq!(project.video_track().total_frame_count(), 100 + 50);
    }

    #[test]
    fn relinking_keeps_every_edit_and_replaces_the_link_and_properties() {
        let mut project = Project::empty();
        project.import_source_video(source_video(1, 30, 300)).unwrap();
        project.split_at(TimelineFrameIndex::new(100)).unwrap();
        project.set_clip_speed(1, SpeedFactor::new(4).unwrap()).unwrap();
        let track_before = project.video_track().clone();

        let new_link = SourceVideoLink {
            absolute_path: PathBuf::from("/elsewhere/copy.mp4"),
            relative_path: Some(PathBuf::from("../elsewhere/copy.mp4")),
        };
        let mut new_properties = source_video(1, 30, 302).properties;
        new_properties.display_width_in_pixels = 640;
        let outcome = project
            .relink_source_video(SourceVideoIdentifier::new(1), new_link.clone(), new_properties)
            .unwrap();

        assert_eq!(
            outcome,
            RelinkOutcome {
                total_frame_count_before: 300,
                total_frame_count_after: 302
            }
        );
        assert_eq!(
            project.video_track(),
            &track_before,
            "relinking must not touch the edits"
        );
        let relinked = project.source_video(SourceVideoIdentifier::new(1)).unwrap();
        assert_eq!(relinked.link, new_link);
        assert_eq!(relinked.properties, new_properties);
    }

    #[test]
    fn relinking_to_a_video_with_a_different_frame_rate_is_refused_and_changes_nothing() {
        let mut project = Project::empty();
        project.import_source_video(source_video(1, 30, 300)).unwrap();
        let before = project.clone();
        let result = project.relink_source_video(
            SourceVideoIdentifier::new(1),
            SourceVideoLink::from_absolute_path(PathBuf::from("/other.mp4")),
            source_video(1, 25, 300).properties,
        );
        assert_eq!(
            result,
            Err(RelinkError::DifferentFrameRate {
                project_frame_rate: frame_rate(30),
                chosen_frame_rate: frame_rate(25)
            })
        );
        assert_eq!(project, before);
    }

    #[test]
    fn relinking_an_unknown_video_is_an_error() {
        let mut project = Project::empty();
        let result = project.relink_source_video(
            SourceVideoIdentifier::new(5),
            SourceVideoLink::from_absolute_path(PathBuf::from("/other.mp4")),
            source_video(5, 30, 300).properties,
        );
        assert_eq!(
            result,
            Err(RelinkError::UnknownSourceVideo {
                identifier: SourceVideoIdentifier::new(5)
            })
        );
    }
}
