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

mod animating;
mod animation;
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

pub use animating::AnimationTarget;
pub use animation::{
    Animation, Keyframe, Keyframes, PROPERTIES_OF_A_PLACEMENT, PROPERTIES_OF_A_TEXT_OBJECT, PropertyId, PropertyValue,
    RemovedKeyframe, ValueKind, placement_value, text_value, with_placement_value, with_text_value,
};
pub use frame_index::{SourceFrameIndex, TimelineFrameIndex};
pub use frame_rate::{FrameRate, InvalidFrameRate};
pub use settings::{
    AnySetting, ProjectSettings, SETTINGS, Setting, SettingControlKind, SettingValue, Settings, SpedUpClipSound,
};
pub use source_media::{
    SourceAudioProperties, SourceImageProperties, SourceMedia, SourceMediaIdentifier, SourceMediaKind, SourceMediaLink,
    SourceMediaProperties, SourceVideoProperties,
};
pub use speed_factor::{SpeedFactor, ZeroSpeedFactor};
pub use speed_indicator_layout::SpeedIndicatorLayout;
pub use timecode::format_timecode;
pub use tracks::{
    Angle, CanvasArea, Clip, ClipEditError, ClipId, ClipKind, ClipPlacement, ClipToArrange, Corner, EmptyClip,
    EmptyImageClip, EmptyTextClip, FontSize, Gain, ImageClip, InsertError, MediaClip, PictureDimensions, PictureFit,
    PixelBox, Placement, PointOnThePicture, Room, SplitError, SplitOutcome, TextClip, TextColour, TextObject, Track,
    TrackEntry, TrackHeight, TrackId, fit_inside_picture, fitted_placement,
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
    /// The frame rate the project was given, or `None` while it follows its first video (and has the
    /// rate of [`FRAME_RATE_WITHOUT_A_VIDEO`] until there is one).
    frame_rate: Option<FrameRate>,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ImportError {
    #[error("the project already has a source file number {identifier}")]
    IdentifierInUse { identifier: SourceMediaIdentifier },
}

/// Where [`Project::move_clips`] is to put a clip: on which track (the one it is on, or another), and at
/// which frame of the timeline it is to start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipMove {
    pub clip: ClipId,
    pub to_track: TrackId,
    pub start: TimelineFrameIndex,
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
    #[error("{track} is the only track, and a project always has a track")]
    CannotDeleteTheOnlyTrack { track: TrackId },
    #[error("{property:?} is not a property of a track")]
    NotAPropertyOfTheTrack { track: TrackId, property: PropertyId },
    #[error(transparent)]
    Clip(#[from] ClipEditError),
}

/// How many seconds a picture is shown for when it is put on a track, until it is changed.
pub const SECONDS_A_NEW_PICTURE_IS_SHOWN_FOR: f64 = 5.0;

impl Project {
    /// A project with no files and one empty track. It follows every global setting.
    pub fn empty() -> Self {
        let mut project = Self {
            sources: Vec::new(),
            tracks: Vec::new(),
            next_track_number: 0,
            speed_indicator_layout: SpeedIndicatorLayout::DEFAULT,
            settings: ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS,
            frame_rate: None,
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
                let Some(identifier) = entry.clip().source() else {
                    continue;
                };
                let source = sources.iter().find(|source| source.identifier == identifier).ok_or(
                    InconsistentProject::ClipUsesUnknownSource {
                        track: track.id(),
                        identifier,
                    },
                )?;
                let wanted_kind = match entry.clip() {
                    Clip::Video(_) => SourceMediaKind::Video,
                    Clip::Audio(_) => SourceMediaKind::Audio,
                    Clip::Image(_) => SourceMediaKind::Image,
                    Clip::Text(_) => continue,
                };
                if source.kind() != wanted_kind {
                    return Err(InconsistentProject::ClipUsesSourceOfTheWrongKind {
                        track: track.id(),
                        identifier: source.identifier,
                    });
                }
            }
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
            frame_rate: None,
        })
    }

    /// The same project with this frame rate chosen for it, or with none chosen (`None`), so that it
    /// follows its first video. (Nothing in it is counted again: see [`Self::set_frame_rate`] for that.)
    #[must_use]
    pub fn with_frame_rate(mut self, frame_rate: Option<FrameRate>) -> Self {
        self.frame_rate = frame_rate;
        self
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
    pub fn placement_of(&self, clip: ClipId) -> Option<ClipPlacement<'_>> {
        self.track_holding(clip)?.placement_of(clip)
    }

    /// The track that has `clip` on it now: not necessarily the one it was made on, because a clip keeps
    /// its id when it is moved to another track.
    pub fn track_holding(&self, clip: ClipId) -> Option<&Track> {
        self.track_position_holding(clip).map(|position| &self.tracks[position])
    }

    /// Where, in the list of tracks (0 is the front), the track that has `clip` on it is.
    pub fn track_position_holding(&self, clip: ClipId) -> Option<usize> {
        let has_it = |track: &Track| track.index_of(clip).is_some();
        // Nearly always still on the track it was made on, which is looked in first.
        match self.track_position(clip.origin()) {
            Some(position) if has_it(&self.tracks[position]) => Some(position),
            _ => self.tracks.iter().position(has_it),
        }
    }

    pub fn speed_indicator_layout(&self) -> SpeedIndicatorLayout {
        self.speed_indicator_layout
    }

    pub fn set_speed_indicator_layout(&mut self, layout: SpeedIndicatorLayout) {
        self.speed_indicator_layout = layout;
    }

    /// The frame rate of the finished video, and of everything on the timeline: the one that was
    /// chosen for the project, or if none was, that of its first video (or [`FRAME_RATE_WITHOUT_A_VIDEO`]
    /// while it has none). Videos of other rates are made to run at it.
    pub fn frame_rate(&self) -> FrameRate {
        self.frame_rate.unwrap_or_else(|| {
            self.sources
                .iter()
                .find_map(|source| source.properties.video().map(|video| video.frame_rate))
                .unwrap_or(FRAME_RATE_WITHOUT_A_VIDEO)
        })
    }

    /// The frame rate that was chosen for the project, if one was (the project file keeps only that).
    pub fn chosen_frame_rate(&self) -> Option<FrameRate> {
        self.frame_rate
    }

    /// Makes `frame_rate` the rate of the project, and counts everything on the timeline again at it, so
    /// that everything lasts as long as it did and starts when it did, to the nearest frame (see
    /// [`Track::retimed`]). The project keeps the rate from now on, whatever videos come.
    pub fn set_frame_rate(&mut self, frame_rate: FrameRate) {
        let before = self.frame_rate();
        self.frame_rate = Some(frame_rate);
        if frame_rate != before {
            for track in &mut self.tracks {
                *track = track.retimed(before, frame_rate);
            }
        }
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

    /// Takes a track, and everything on it, out of the project. The files its clips were made from
    /// stay part of the project. A project always has a track, so the last one cannot be deleted.
    pub fn delete_track(&mut self, track: TrackId) -> Result<(), EditError> {
        let position = self.track_position(track).ok_or(EditError::UnknownTrack { track })?;
        if self.tracks.len() == 1 {
            return Err(EditError::CannotDeleteTheOnlyTrack { track });
        }
        self.tracks.remove(position);
        Ok(())
    }

    /// Moves a track to `position` in the list of tracks (0 is the front): it goes in front of the tracks that are
    /// from that place on and behind those before it, and the other tracks keep their order. A `position` that
    /// is past the last place is the last place.
    pub fn move_track(&mut self, track: TrackId, position: usize) -> Result<(), EditError> {
        let from = self.track_position(track).ok_or(EditError::UnknownTrack { track })?;
        let moved = self.tracks.remove(from);
        self.tracks.insert(position.min(self.tracks.len()), moved);
        Ok(())
    }

    /// Adds a file to the project and puts all of it on a track, starting at `at_frame`: on the
    /// front-most track that has nothing on it, or on a new track in front of all of them.
    ///
    /// A video that does not run at the frame rate of the project is made to: it lasts as long as it
    /// does in time, which is as many frames as it comes to at the project's rate. The first video of a
    /// project that has nothing on its tracks (and no rate chosen for it) sets that rate; in a project
    /// that has something on them, which was laid out at the rate it has, the rate stays what it is.
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
        let has_a_video = self
            .sources
            .iter()
            .any(|existing| existing.properties.video().is_some());
        if source.properties.video().is_some() && !has_a_video && !self.has_no_clips() {
            self.frame_rate = Some(self.frame_rate());
        }

        let (track_id, created_track) = match self.tracks.iter().find(|track| track.is_empty()) {
            Some(empty_track) => (empty_track.id(), false),
            None => (self.add_track_in_front(), true),
        };
        let identifier = source.identifier;
        let kind = source.kind();
        let properties = source.properties;
        self.sources.push(source);
        let frame_count = match kind {
            // A picture is shown for a while, as long as that is said to be (it has no length of its own).
            SourceMediaKind::Image => self
                .frame_rate()
                .nearest_frame_to_seconds(SECONDS_A_NEW_PICTURE_IS_SHOWN_FOR)
                .max(1),
            SourceMediaKind::Video | SourceMediaKind::Audio => properties.frame_count_at(self.frame_rate()),
        };

        let clip = match kind {
            SourceMediaKind::Image => ImageClip::new(identifier, frame_count).ok().map(Clip::Image),
            SourceMediaKind::Video | SourceMediaKind::Audio => MediaClip::new(
                identifier,
                SourceFrameIndex::new(0),
                SourceFrameIndex::new(frame_count),
                SpeedFactor::NORMAL,
            )
            .ok()
            .map(|media| {
                if kind == SourceMediaKind::Video {
                    Clip::Video(media)
                } else {
                    Clip::Audio(media)
                }
            }),
        };
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
    /// The new file must be the same kind of file. A video may run at another frame rate than the old
    /// one: the clips are stored as frames of the project, which is as much as to say as stretches of
    /// time. Its length may differ; the returned [`RelinkOutcome`] reports by how much (in frames of the
    /// project) so that the caller can tell the user.
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
                expected: source.properties.kind().description(),
                chosen: properties.kind().description(),
            });
        }
        let outcome = RelinkOutcome {
            frame_count_before: source.properties.frame_count_at(frame_rate),
            frame_count_after: properties.frame_count_at(frame_rate),
        };
        source.link = link;
        source.properties = properties;
        // A project that follows its first video must not follow it to another frame rate by this: the
        // edits are laid out at the rate it had.
        if self.frame_rate() != frame_rate {
            self.frame_rate = Some(frame_rate);
        }
        Ok(outcome)
    }

    /// Makes the project what `earlier` is, which is a copy of the project from before some edits
    /// (or from after some that were undone), as undoing and redoing do. Two things are left as
    /// they are now, because they are not what was edited:
    ///
    /// * the settings of the project, and
    /// * where each source file is and what is known of it, for the files that `earlier` has too:
    ///   a file may have been found, or chosen again, since the copy was made, and putting the old
    ///   place back would lose it.
    pub fn restore(&mut self, mut earlier: Project) {
        for source in &mut earlier.sources {
            if let Some(current) = self.source(source.identifier) {
                source.link = current.link.clone();
                source.properties = current.properties;
            }
        }
        earlier.settings = self.settings;
        *self = earlier;
    }

    fn track_mut(&mut self, track: TrackId) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|candidate| candidate.id() == track)
    }

    fn track_mut_or_error(&mut self, track: TrackId) -> Result<&mut Track, EditError> {
        self.track_mut(track).ok_or(EditError::UnknownTrack { track })
    }

    fn track_holding_mut_or_error(&mut self, clip: ClipId) -> Result<&mut Track, EditError> {
        let position = self
            .track_position_holding(clip)
            .ok_or(ClipEditError::UnknownClip { clip })?;
        Ok(&mut self.tracks[position])
    }

    /// Cuts the clips that are at `timeline_frame`, each in two, there. (A track that has nothing
    /// there, or where a clip already starts there, is left alone.) With `only` given, only those
    /// clips are cut, and only those of them that are at that frame. Returns what was cut.
    pub fn split_at(
        &mut self,
        timeline_frame: TimelineFrameIndex,
        only: Option<&[ClipId]>,
    ) -> Vec<(TrackId, SplitOutcome)> {
        let mut cut = Vec::new();
        for track in &mut self.tracks {
            if let Some(only) = only {
                let is_one_of_them = track
                    .placement_covering(timeline_frame)
                    .is_some_and(|placement| only.contains(&placement.id));
                if !is_one_of_them {
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
    pub fn set_clip_speed(&mut self, clip: ClipId, speed: SpeedFactor) -> Result<(), EditError> {
        Ok(self.track_holding_mut_or_error(clip)?.set_clip_speed(clip, speed)?)
    }

    pub fn set_clip_gain(&mut self, clip: ClipId, gain: Gain) -> Result<(), EditError> {
        Ok(self.track_holding_mut_or_error(clip)?.set_clip_gain(clip, gain)?)
    }

    pub fn set_clip_muted(&mut self, clip: ClipId, muted: bool) -> Result<(), EditError> {
        Ok(self.track_holding_mut_or_error(clip)?.set_clip_muted(clip, muted)?)
    }

    /// Moves clips, each to start at the frame given for it on the track given for it, which may be the track
    /// it is on or another: it is the same clip on the other track, and has the id it had. Nothing else moves
    /// (a clip that touched one that is moved away stays where it was, with a gap now before it), and how long
    /// a clip lasts is counted by the speed of the track it ends up on. A clip named twice goes where it is
    /// last told to.
    ///
    /// Either all of them move or, if one of them cannot (a clip or a track is not there, or clips would be
    /// on top of one another), none does. The clips all leave before any arrives, so that clips can swap
    /// places, or one take the place another leaves.
    pub fn move_clips(&mut self, moves: &[ClipMove]) -> Result<(), EditError> {
        /// A clip to move: where it is now (as a place in the list of tracks), where it goes, and where it starts.
        struct Departure {
            clip: ClipId,
            from: usize,
            to: usize,
            start: u64,
        }
        let mut departures: Vec<Departure> = Vec::with_capacity(moves.len());
        for requested in moves {
            let from = self
                .track_position_holding(requested.clip)
                .ok_or(ClipEditError::UnknownClip { clip: requested.clip })?;
            let to = self.track_position(requested.to_track).ok_or(EditError::UnknownTrack {
                track: requested.to_track,
            })?;
            let departure = Departure {
                clip: requested.clip,
                from,
                to,
                start: requested.start.value(),
            };
            match departures.iter_mut().find(|existing| existing.clip == requested.clip) {
                Some(existing) => *existing = departure,
                None => departures.push(departure),
            }
        }

        let mut affected: Vec<usize> = departures
            .iter()
            .flat_map(|departure| [departure.from, departure.to])
            .collect();
        affected.sort_unstable();
        affected.dedup();
        let mut arranged: Vec<(usize, Track)> = Vec::with_capacity(affected.len());
        for position in affected {
            // What the track has when it is done: the clips of it that stay, where they are, and the clips that arrive.
            let mut clips: Vec<ClipToArrange> = self.tracks[position]
                .placements()
                .filter(|placement| departures.iter().all(|departure| departure.clip != placement.id))
                .map(|placement| ClipToArrange {
                    id: placement.id,
                    clip: placement.clip.clone(),
                    animation: placement.animation.clone(),
                    start: placement.first_timeline_frame.value(),
                    frames_it_lasted_before: placement.timeline_frame_count(),
                })
                .collect();
            for departure in departures.iter().filter(|departure| departure.to == position) {
                let leaving = self.tracks[departure.from]
                    .placement_of(departure.clip)
                    .expect("the clip is on the track it was found on");
                clips.push(ClipToArrange {
                    id: departure.clip,
                    clip: leaving.clip.clone(),
                    animation: leaving.animation.clone(),
                    start: departure.start,
                    frames_it_lasted_before: leaving.timeline_frame_count(),
                });
            }
            let mut track = self.tracks[position].clone();
            track.arrange_clips(clips)?;
            arranged.push((position, track));
        }
        for (position, track) in arranged {
            self.tracks[position] = track;
        }
        Ok(())
    }

    /// Takes a clip off its track. See [`Track::delete_clip`] for what moves.
    pub fn delete_clip(&mut self, clip: ClipId) -> Result<(), EditError> {
        Ok(self.track_holding_mut_or_error(clip)?.delete_clip(clip)?)
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

    /// Makes the row of a track on the timeline taller or shorter.
    pub fn set_track_height(&mut self, track: TrackId, height: TrackHeight) -> Result<(), EditError> {
        self.track_mut_or_error(track)?.set_height(height);
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
    ) -> Option<(ClipId, u64)> {
        let (clip, duration) = self
            .track_mut(track)?
            .insert_text_at(timeline_frame, object, wanted_frames)?;
        Some((clip, duration))
    }

    pub fn set_text_object(&mut self, clip: ClipId, object: TextObject) -> Result<(), EditError> {
        Ok(self.track_holding_mut_or_error(clip)?.set_text_object(clip, object)?)
    }

    /// Puts a video or a text clip somewhere on the picture: where, how big and how far turned.
    pub fn set_clip_placement(&mut self, clip: ClipId, placement: Placement) -> Result<(), EditError> {
        Ok(self
            .track_holding_mut_or_error(clip)?
            .set_clip_placement(clip, placement)?)
    }

    /// Makes a text clip or a picture last `frames` frames, or as long as there is room for if that
    /// is less. Returns how long it lasts now.
    pub fn set_clip_duration(&mut self, clip: ClipId, frames: u64) -> Result<u64, EditError> {
        Ok(self.track_holding_mut_or_error(clip)?.set_clip_duration(clip, frames)?)
    }
}

impl Default for Project {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod animating_tests;
#[cfg(test)]
mod tests;
