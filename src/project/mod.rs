//! What a project *is*: the files it is made from, the tracks (in front-to-back order) with the
//! clips on them, where the fast-forward symbol goes, and the settings the project has chosen for
//! itself.
//!
//! Nothing in this module touches files, processes or the screen, so all of it can be tested
//! quickly and thoroughly.
//!
//! Positions on both the source files and the edited timeline are whole frame numbers
//! ([`SourceFrameIndex`], [`TimelineFrameIndex`]). Whole frames keep every calculation exact and
//! make the live preview show precisely the frames the export will contain.

mod frame_index;
mod frame_rate;
mod settings;
mod source_media;
mod speed_factor;
mod speed_indicator_layout;
mod timecode;
pub mod tracks;

use std::collections::HashSet;
use std::num::NonZeroU32;

use thiserror::Error;

pub use frame_index::{SourceFrameIndex, TimelineFrameIndex};
pub use frame_rate::{FrameRate, InvalidFrameRate};
pub use settings::{
    AnySetting, ProjectSettings, SETTINGS, Setting, SettingControlKind, SettingValue, Settings, SpedUpClipSound,
};
pub use source_media::{
    SourceAudioProperties, SourceMedia, SourceMediaIdentifier, SourceMediaKind, SourceMediaLink, SourceMediaProperties,
    SourceVideoProperties,
};
pub use speed_factor::{SpeedFactor, ZeroSpeedFactor};
pub use speed_indicator_layout::SpeedIndicatorLayout;
pub use timecode::format_timecode;
pub use tracks::{
    CanvasArea, Clip, ClipEditError, ClipId, ClipKind, ClipLocation, ClipPlacement, Corner, EmptyClip, EmptyTextClip,
    FontSize, Gain, InsertError, MediaClip, Room, SplitError, SplitOutcome, TextClip, TextColour, TextObject, Track,
    TrackEntry, TrackId,
};

/// The frame rate of a project that has no video to take one from.
pub const FRAME_RATE_WITHOUT_A_VIDEO: FrameRate =
    FrameRate::from_non_zero_whole_frames_per_second(NonZeroU32::new(30).unwrap());

/// The size of the picture of a project that has no video to take one from, in pixels.
pub const PICTURE_SIZE_WITHOUT_A_VIDEO: (u32, u32) = (1920, 1080);

#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    sources: Vec<SourceMedia>,
    /// The tracks, the one in front first.
    tracks: Vec<Track>,
    /// The number the next new track is given: it names the track (see [`TrackId`]) and, until it
    /// is given another one, its title.
    next_track_number: u32,
    speed_indicator_layout: SpeedIndicatorLayout,
    settings: ProjectSettings,
}

/// Returned when the parts offered to [`Project::from_parts`] contradict each other.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum InconsistentProject {
    #[error("source file number {identifier} appears more than once")]
    DuplicateSource { identifier: SourceMediaIdentifier },
    #[error("the {track} appears more than once")]
    DuplicateTrack { track: TrackId },
    #[error("a clip on {track} uses source file number {identifier}, but the project does not contain that file")]
    ClipUsesUnknownSource {
        track: TrackId,
        identifier: SourceMediaIdentifier,
    },
    #[error("a clip on {track} is made from source file number {identifier}, which is not the kind of file it needs")]
    ClipUsesSourceOfTheWrongKind {
        track: TrackId,
        identifier: SourceMediaIdentifier,
    },
    #[error(
        "this version of fastforwardrs needs every video in a project to have the same frame rate, but video {first_identifier} runs at {first_frame_rate} and video {other_identifier} at {other_frame_rate}"
    )]
    MixedFrameRates {
        first_identifier: SourceMediaIdentifier,
        first_frame_rate: FrameRate,
        other_identifier: SourceMediaIdentifier,
        other_frame_rate: FrameRate,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ImportError {
    #[error("the project already has a source file number {identifier}")]
    IdentifierInUse { identifier: SourceMediaIdentifier },
    #[error(
        "this project runs at {project_frame_rate} frames per second, but the video runs at {chosen_frame_rate}. All the videos of a project must run at the same frame rate"
    )]
    DifferentFrameRate {
        project_frame_rate: FrameRate,
        chosen_frame_rate: FrameRate,
    },
    #[error(
        "this project already has things on its tracks, laid out at {project_frame_rate} frames per second (because it has no video yet), but the video runs at {chosen_frame_rate}. Import the video first, and then the rest"
    )]
    ProjectAlreadyLaidOutAtAnotherFrameRate {
        project_frame_rate: FrameRate,
        chosen_frame_rate: FrameRate,
    },
}

/// Where an imported file went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportOutcome {
    pub track: TrackId,
    /// The clip made of the whole file, or `None` for a file with nothing in it.
    pub clip: Option<ClipId>,
    /// Whether the track was made for the file (there was no empty one to put it on).
    pub created_track: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum RelinkError {
    #[error("the project has no source file number {identifier}")]
    UnknownSource { identifier: SourceMediaIdentifier },
    #[error("the project's file number {identifier} is {expected}, but the chosen file is {chosen}")]
    DifferentKind {
        identifier: SourceMediaIdentifier,
        expected: &'static str,
        chosen: &'static str,
    },
    #[error(
        "the chosen video runs at {chosen_frame_rate} frames per second, but this project was edited on a video running at {project_frame_rate}. Choose the original video (or a copy with the same frame rate) so that the edits still line up"
    )]
    DifferentFrameRate {
        project_frame_rate: FrameRate,
        chosen_frame_rate: FrameRate,
    },
}

/// How a relinked file compares with the one it replaced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelinkOutcome {
    pub frame_count_before: u64,
    pub frame_count_after: u64,
}

/// Returned by the edits that name a track or a clip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum EditError {
    #[error("the project has no {track}")]
    UnknownTrack { track: TrackId },
    #[error(transparent)]
    Clip(#[from] ClipEditError),
}

fn kind_name(kind: SourceMediaKind) -> &'static str {
    match kind {
        SourceMediaKind::Video => "a video",
        SourceMediaKind::Audio => "a sound file",
    }
}

impl Project {
    /// A project with no files and one empty track. It follows every global setting.
    pub fn empty() -> Self {
        let mut project = Self {
            sources: Vec::new(),
            tracks: Vec::new(),
            next_track_number: 0,
            speed_indicator_layout: SpeedIndicatorLayout::DEFAULT,
            settings: ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS,
        };
        project.add_track_in_front();
        project
    }

    /// Assembles a project from its parts (for example when loading a project file), checking
    /// that they agree with each other. It follows every global setting; see [`Self::with_settings`].
    pub fn from_parts(
        sources: Vec<SourceMedia>,
        tracks: Vec<Track>,
        speed_indicator_layout: SpeedIndicatorLayout,
    ) -> Result<Self, InconsistentProject> {
        let mut seen_sources = HashSet::new();
        for source in &sources {
            if !seen_sources.insert(source.identifier) {
                return Err(InconsistentProject::DuplicateSource {
                    identifier: source.identifier,
                });
            }
        }
        let mut seen_tracks = HashSet::new();
        for track in &tracks {
            if !seen_tracks.insert(track.id()) {
                return Err(InconsistentProject::DuplicateTrack { track: track.id() });
            }
            for entry in track.entries() {
                let Some(media) = entry.clip().as_media() else {
                    continue;
                };
                let source = sources
                    .iter()
                    .find(|source| source.identifier == media.source())
                    .ok_or(InconsistentProject::ClipUsesUnknownSource {
                        track: track.id(),
                        identifier: media.source(),
                    })?;
                let wanted_kind = match entry.clip() {
                    Clip::Video(_) => SourceMediaKind::Video,
                    _ => SourceMediaKind::Audio,
                };
                if source.kind() != wanted_kind {
                    return Err(InconsistentProject::ClipUsesSourceOfTheWrongKind {
                        track: track.id(),
                        identifier: source.identifier,
                    });
                }
            }
        }
        let mut videos = sources.iter().filter_map(|source| {
            source
                .properties
                .video()
                .map(|video| (source.identifier, video.frame_rate))
        });
        if let Some((first_identifier, first_frame_rate)) = videos.next()
            && let Some((other_identifier, other_frame_rate)) =
                videos.find(|(_, frame_rate)| *frame_rate != first_frame_rate)
        {
            return Err(InconsistentProject::MixedFrameRates {
                first_identifier,
                first_frame_rate,
                other_identifier,
                other_frame_rate,
            });
        }
        let next_track_number = tracks
            .iter()
            .map(|track| track.id().value() + 1)
            .max()
            .unwrap_or(0)
            .max(u32::try_from(tracks.len()).unwrap_or(u32::MAX));
        Ok(Self {
            sources,
            tracks,
            next_track_number,
            speed_indicator_layout,
            settings: ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS,
        })
    }

    /// The same project with these settings of its own.
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

    pub fn sources(&self) -> &[SourceMedia] {
        &self.sources
    }

    pub fn source(&self, identifier: SourceMediaIdentifier) -> Option<&SourceMedia> {
        self.sources.iter().find(|source| source.identifier == identifier)
    }

    /// The identifier a newly imported file should get: one more than the largest in use.
    pub fn next_source_identifier(&self) -> SourceMediaIdentifier {
        let largest_in_use = self.sources.iter().map(|source| source.identifier.value()).max();
        SourceMediaIdentifier::new(largest_in_use.map_or(1, |largest| largest + 1))
    }

    /// The tracks, the one in front first.
    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn track(&self, track: TrackId) -> Option<&Track> {
        self.tracks.iter().find(|candidate| candidate.id() == track)
    }

    /// Where `track` is in the list of tracks (0 is the front).
    pub fn track_position(&self, track: TrackId) -> Option<usize> {
        self.tracks.iter().position(|candidate| candidate.id() == track)
    }

    /// Where a clip is, on its track.
    pub fn placement_of(&self, location: ClipLocation) -> Option<ClipPlacement<'_>> {
        self.track(location.track)?.placement_of(location.clip)
    }

    pub fn speed_indicator_layout(&self) -> SpeedIndicatorLayout {
        self.speed_indicator_layout
    }

    pub fn set_speed_indicator_layout(&mut self, layout: SpeedIndicatorLayout) {
        self.speed_indicator_layout = layout;
    }

    /// The frame rate of the finished video: the frame rate of the project's videos, or
    /// [`FRAME_RATE_WITHOUT_A_VIDEO`] while it has none.
    pub fn frame_rate(&self) -> FrameRate {
        self.sources
            .iter()
            .find_map(|source| source.properties.video().map(|video| video.frame_rate))
            .unwrap_or(FRAME_RATE_WITHOUT_A_VIDEO)
    }

    /// The size of the finished picture in pixels, as the first video shows it (or a standard size
    /// while the project has no video). Videos of another shape are fitted inside it.
    pub fn picture_size(&self) -> (u32, u32) {
        self.sources
            .iter()
            .find_map(|source| source.properties.video())
            .map_or(PICTURE_SIZE_WITHOUT_A_VIDEO, |video| {
                (video.display_width_in_pixels, video.display_height_in_pixels)
            })
    }

    /// How many frames the finished video lasts: up to the end of the last clip of any track.
    pub fn total_frame_count(&self) -> u64 {
        self.tracks.iter().map(Track::total_frame_count).max().unwrap_or(0)
    }

    /// Whether no track has a clip on it.
    pub fn has_no_clips(&self) -> bool {
        self.tracks.iter().all(Track::is_empty)
    }

    /// How many frames of the file `identifier` there are, counted at the project's frame rate.
    pub fn frame_count_of_source(&self, identifier: SourceMediaIdentifier) -> Option<u64> {
        Some(self.source(identifier)?.properties.frame_count_at(self.frame_rate()))
    }

    /// Adds an empty track in front of all the others and gives it the next title.
    pub fn add_track_in_front(&mut self) -> TrackId {
        let number = self.next_track_number;
        self.next_track_number += 1;
        let id = TrackId::new(number);
        self.tracks.insert(0, Track::new(id, format!("track {number}")));
        id
    }

    /// Adds a file to the project and puts all of it on a track, starting at `at_frame`: on the
    /// front-most track that has nothing on it, or on a new track in front of all of them.
    pub fn import_source(
        &mut self,
        source: SourceMedia,
        at_frame: TimelineFrameIndex,
    ) -> Result<ImportOutcome, ImportError> {
        if self.source(source.identifier).is_some() {
            return Err(ImportError::IdentifierInUse {
                identifier: source.identifier,
            });
        }
        if let Some(chosen) = source.properties.video().map(|video| video.frame_rate) {
            let project_frame_rate = self.frame_rate();
            let has_a_video = self
                .sources
                .iter()
                .any(|existing| existing.properties.video().is_some());
            if has_a_video && chosen != project_frame_rate {
                return Err(ImportError::DifferentFrameRate {
                    project_frame_rate,
                    chosen_frame_rate: chosen,
                });
            }
            if !has_a_video && chosen != project_frame_rate && !self.has_no_clips() {
                return Err(ImportError::ProjectAlreadyLaidOutAtAnotherFrameRate {
                    project_frame_rate,
                    chosen_frame_rate: chosen,
                });
            }
        }

        let (track_id, created_track) = match self.tracks.iter().find(|track| track.is_empty()) {
            Some(empty_track) => (empty_track.id(), false),
            None => (self.add_track_in_front(), true),
        };
        let identifier = source.identifier;
        let kind = source.kind();
        let frame_count = match source.properties.video() {
            // The project's frame rate is the video's own, once it has been added.
            Some(video) => video.total_frame_count,
            None => source.properties.frame_count_at(self.frame_rate()),
        };
        self.sources.push(source);

        let clip = MediaClip::new(
            identifier,
            SourceFrameIndex::new(0),
            SourceFrameIndex::new(frame_count),
            SpeedFactor::NORMAL,
        )
        .ok()
        .map(|media| match kind {
            SourceMediaKind::Video => Clip::Video(media),
            SourceMediaKind::Audio => Clip::Audio(media),
        });
        let clip_id = match clip {
            Some(clip) => self
                .track_mut(track_id)
                .and_then(|track| track.insert_clip_at(at_frame, clip).ok()),
            None => None,
        };
        Ok(ImportOutcome {
            track: track_id,
            clip: clip_id,
            created_track,
        })
    }

    /// Points a source file at a different file, for example after the original was moved. The
    /// clips are left exactly as they are: no edit is lost.
    ///
    /// The new file must be the same kind of file, and a video must run at the same frame rate as
    /// the old one, because the clips are stored as frame numbers. Its length may differ; the
    /// returned [`RelinkOutcome`] reports by how much so that the caller can tell the user.
    pub fn relink_source(
        &mut self,
        identifier: SourceMediaIdentifier,
        link: SourceMediaLink,
        properties: SourceMediaProperties,
    ) -> Result<RelinkOutcome, RelinkError> {
        let frame_rate = self.frame_rate();
        let source = self
            .sources
            .iter_mut()
            .find(|source| source.identifier == identifier)
            .ok_or(RelinkError::UnknownSource { identifier })?;
        if source.properties.kind() != properties.kind() {
            return Err(RelinkError::DifferentKind {
                identifier,
                expected: kind_name(source.properties.kind()),
                chosen: kind_name(properties.kind()),
            });
        }
        if let (Some(old), Some(new)) = (source.properties.video(), properties.video())
            && old.frame_rate != new.frame_rate
        {
            return Err(RelinkError::DifferentFrameRate {
                project_frame_rate: old.frame_rate,
                chosen_frame_rate: new.frame_rate,
            });
        }
        let outcome = RelinkOutcome {
            frame_count_before: source.properties.frame_count_at(frame_rate),
            frame_count_after: properties.frame_count_at(frame_rate),
        };
        source.link = link;
        source.properties = properties;
        Ok(outcome)
    }

    fn track_mut(&mut self, track: TrackId) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|candidate| candidate.id() == track)
    }

    fn track_mut_or_error(&mut self, track: TrackId) -> Result<&mut Track, EditError> {
        self.track_mut(track).ok_or(EditError::UnknownTrack { track })
    }

    /// Cuts the clips that are at `timeline_frame`, each in two, there. (A track that has nothing
    /// there, or where a clip already starts there, is left alone.) With `only` given, only that
    /// clip is cut, and only if it is at that frame. Returns what was cut.
    pub fn split_at(
        &mut self,
        timeline_frame: TimelineFrameIndex,
        only: Option<ClipLocation>,
    ) -> Vec<(TrackId, SplitOutcome)> {
        let mut cut = Vec::new();
        for track in &mut self.tracks {
            if let Some(only) = only {
                let is_the_one = track.id() == only.track
                    && track
                        .placement_covering(timeline_frame)
                        .is_some_and(|placement| placement.id == only.clip);
                if !is_the_one {
                    continue;
                }
            }
            if let Ok(outcome) = track.split_at(timeline_frame) {
                cut.push((track.id(), outcome));
            }
        }
        cut
    }

    /// Plays a video or sound clip at `speed` (on top of the speed of its track).
    pub fn set_clip_speed(&mut self, location: ClipLocation, speed: SpeedFactor) -> Result<(), EditError> {
        Ok(self
            .track_mut_or_error(location.track)?
            .set_clip_speed(location.clip, speed)?)
    }

    pub fn set_clip_gain(&mut self, location: ClipLocation, gain: Gain) -> Result<(), EditError> {
        Ok(self
            .track_mut_or_error(location.track)?
            .set_clip_gain(location.clip, gain)?)
    }

    pub fn set_clip_muted(&mut self, location: ClipLocation, muted: bool) -> Result<(), EditError> {
        Ok(self
            .track_mut_or_error(location.track)?
            .set_clip_muted(location.clip, muted)?)
    }

    /// Takes a clip off its track. See [`Track::delete_clip`] for what moves.
    pub fn delete_clip(&mut self, location: ClipLocation) -> Result<(), EditError> {
        Ok(self.track_mut_or_error(location.track)?.delete_clip(location.clip)?)
    }

    pub fn set_track_title(&mut self, track: TrackId, title: impl Into<String>) -> Result<(), EditError> {
        self.track_mut_or_error(track)?.set_title(title);
        Ok(())
    }

    pub fn set_track_gain(&mut self, track: TrackId, gain: Gain) -> Result<(), EditError> {
        self.track_mut_or_error(track)?.set_gain(gain);
        Ok(())
    }

    pub fn set_track_muted(&mut self, track: TrackId, muted: bool) -> Result<(), EditError> {
        self.track_mut_or_error(track)?.set_muted(muted);
        Ok(())
    }

    /// Makes everything on a track play `speed` times faster, on top of the clips' own speeds.
    pub fn set_track_speed(&mut self, track: TrackId, speed: SpeedFactor) -> Result<(), EditError> {
        self.track_mut_or_error(track)?.set_speed(speed);
        Ok(())
    }

    /// Adds a text object to a track at `timeline_frame`, lasting `wanted_frames` frames or as
    /// long as there is room for if that is less. Returns the new clip and how many frames it
    /// lasts, or `None` if there is no such track or a clip is at that frame.
    pub fn insert_text(
        &mut self,
        track: TrackId,
        timeline_frame: TimelineFrameIndex,
        object: TextObject,
        wanted_frames: u64,
    ) -> Option<(ClipLocation, u64)> {
        let (clip, duration) = self
            .track_mut(track)?
            .insert_text_at(timeline_frame, object, wanted_frames)?;
        Some((ClipLocation { track, clip }, duration))
    }

    pub fn set_text_object(&mut self, location: ClipLocation, object: TextObject) -> Result<(), EditError> {
        Ok(self
            .track_mut_or_error(location.track)?
            .set_text_object(location.clip, object)?)
    }

    /// Makes a text clip last `frames` frames, or as long as there is room for if that is less.
    /// Returns how long it lasts now.
    pub fn set_text_duration(&mut self, location: ClipLocation, frames: u64) -> Result<u64, EditError> {
        Ok(self
            .track_mut_or_error(location.track)?
            .set_text_duration(location.clip, frames)?)
    }
}

impl Default for Project {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests;
