// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

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
mod crates;
mod duplicating;
mod effect_editing;
mod effects;
mod frame_index;
mod frame_rate;
pub(crate) mod groups;
mod keyframe_copying;
mod plugin_libraries;
mod rack_editing;
mod rack_merging;
mod racks;
mod resource_editing;
mod resources;
mod settings;
mod source_colour;
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
    ValueKind, placement_value, text_value, with_placement_value, with_text_value,
};
pub use duplicating::{ClipSide, TrackSide};
pub use effects::{
    Badge, ChainEntry, ChainItem, ChainItemKey, ChoiceOption, Effect, EffectDescriptor, EffectDomain, EffectInstanceId,
    EffectKind, Effects, EffectsIter, HUE_SHIFT_KEY, ImportedPicture, InstalledPlugin, LutOperation, LutSpace,
    MOST_BYTES_OF_A_SHADER, MOST_BYTES_OF_AN_IMAGE_OF_A_SHADER, MOST_EFFECTS_ON_ONE_THING, MOST_IMAGES_A_SHADER_BRINGS,
    MOST_PIXELS_ACROSS_OF_AN_IMAGE_OF_A_SHADER, MOST_VALUES_OF_AN_EFFECT, ParameterControl, ParameterDescriptor,
    PictureOperation, PluginAddress, ShaderImage, ShaderLanguage, ShaderSource, UnknownFields, ValueScale,
    installed_plugin_for, installed_plugins, set_installed_plugins,
};
pub use frame_index::{SourceFrameIndex, TimelineFrameIndex};
pub use frame_rate::{FrameRate, InvalidFrameRate};
pub use groups::{
    AppliedEffect, ChainRow, ClipGroupBeforeTheDrag, ClipGroupEdge, EffectOwner, Group, GroupColour, GroupId,
    GroupKind, GroupProblem, Member, Rectangle,
};
pub use keyframe_copying::TimeDirection;
pub use plugin_libraries::{
    EmbeddedBuild, LinkedLibrary, MOST_BUILDS_OF_A_LIBRARY, MOST_BYTES_OF_A_BUILD, MOST_BYTES_OF_ALL_BUILDS,
    MOST_PLUGIN_LIBRARIES, PluginLibraries, PluginLibrary, PluginLibraryKind, PluginLibraryProblem,
    PluginLibrarySource, is_a_platform_name,
};
pub use rack_editing::{KeyframeDestination, KeyframeMove, RackDestination};
pub use racks::{
    DrawerExtent, FrameRoom, KeyframeLocation, LocatedKeyframe, LocatedRack, NewKeyframe, ResolvedAnimations,
};
pub use resources::{
    MOST_BYTES_OF_A_RESOURCE, MOST_RESOURCES, Resource, ResourceKind, ResourceProblem, Resources,
    why_not_the_name_of_a_resource,
};
pub use settings::{
    AnySetting, DisplayTransform, HowToKeepAResource, OutputColourSpace, ProjectFileKind, ProjectSettings,
    ResourceStorage, SETTINGS, Setting, SettingControlKind, SettingValue, Settings, SpedUpClipSound,
    WorkingColourSpace,
};
pub use source_colour::InputColourOverride;
pub use source_media::{
    SourceAudioProperties, SourceImageProperties, SourceMedia, SourceMediaIdentifier, SourceMediaKind, SourceMediaLink,
    SourceMediaProperties, SourceVideoProperties,
};
pub use speed_factor::{SpeedFactor, ZeroSpeedFactor};
pub use speed_indicator_layout::SpeedIndicatorLayout;
pub use timecode::format_timecode;
pub use tracks::{
    AdjustmentClip, Angle, CanvasArea, Clip, ClipEditError, ClipId, ClipKind, ClipPlacement, ClipToArrange, Corner,
    CrateClip, CutEdges, Drawer, EmptyAdjustmentClip, EmptyClip, EmptyCrate, EmptyGeneratorClip, EmptyImageClip,
    EmptyTextClip, FontSize, Gain, GeneratorClip, ImageClip, InsertError, KeyframeId, MediaClip, PictureDimensions,
    PictureFit, PixelBox, Placement, PointOnThePicture, Rack, RackCut, RackEditError, RackId, RackKeyframe, Room,
    SplitError, SplitOutcome, TextClip, TextColour, TextObject, Track, TrackEntry, TrackHeight, TrackId,
    fit_inside_picture, fitted_placement,
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
    /// The groups, in the order they were made in. What is in them is for [`groups`] to say, and the rules that they keep are there too.
    groups: Vec<Group>,
    /// The number the next new group is given: it names the group (see [`GroupId`]).
    next_group_number: u32,
    speed_indicator_layout: SpeedIndicatorLayout,
    settings: ProjectSettings,
    /// The files that the effects use (see [`Resources`]), in the order they were added in.
    resources: Resources,
    /// The libraries of native code that its sound effects come from (see [`PluginLibraries`]): linked or embedded. Nothing is loaded from them without a person's word.
    plugin_libraries: PluginLibraries,
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
    #[error("the racks of keyframes do not agree with what is in the project: {problems}")]
    RacksAreWrong { problems: String },
    #[error("the groups do not agree with what is in the project: {problem}")]
    GroupsAreWrong { problem: GroupProblem },
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
    #[error("{track} cannot have the {kind} effect")]
    NotAnEffectOfTheTrack { track: TrackId, kind: EffectKind },
    #[error(
        "{clip} is a generator clip, and what it makes is its first effect, which cannot be taken off, switched off, copied or moved"
    )]
    TheContentOfAGenerator { clip: ClipId },
    #[error("{target} does not have the {instance}")]
    DoesNotHaveThatEffect {
        target: AnimationTarget,
        instance: EffectInstanceId,
    },
    #[error(transparent)]
    Clip(#[from] ClipEditError),
    #[error("the project has no {rack}")]
    UnknownRack { rack: RackId },
    #[error(transparent)]
    Rack(#[from] RackEditError),
    #[error("the keyframes would not all be at frames that the drawer they are to be in has")]
    NoRoomForTheKeyframes,
    #[error(
        "{keyframe} cannot go to {rack}, which does not have the same property of the same thing as the rack it is in"
    )]
    KeyframeCannotGoToThatRack { keyframe: KeyframeId, rack: RackId },
    #[error("racks can only be merged when there are at least two of them")]
    TooFewRacksToMerge,
    #[error("racks can only be merged when they have the same property of the same thing")]
    RacksOfDifferentThingsCannotBeMerged,
    #[error("there is no room next to {clip} for a copy of it")]
    NoRoomForACopy { clip: ClipId },
    #[error(transparent)]
    Insert(#[from] InsertError),
    #[error(transparent)]
    EmptyCrate(#[from] EmptyCrate),
    #[error("there is no room for it there")]
    NoRoomThere,
    #[error("the project has no {group}")]
    UnknownGroup { group: GroupId },
    #[error("there is nothing to make a {kind} of")]
    NothingToMakeAGroupOf { kind: GroupKind },
    #[error("a clip group is made of at least two things")]
    TooFewToMakeAClipGroup,
    #[error("{member} is in {group} already")]
    AlreadyInTheGroup { group: GroupId, member: Member },
    #[error("{member} is not in {group}")]
    NotInTheGroup { group: GroupId, member: Member },
    #[error("{group} is not a clip group, and has no edges to move")]
    NotAClipGroup { group: GroupId },
    #[error("the edge would cut across {member}: part of it would be inside the group and part outside")]
    TheEdgeWouldCutAcross { member: Member },
    #[error("nothing would be left in {group}")]
    NothingWouldBeLeftInTheGroup { group: GroupId },
    /// An edit that would leave the groups breaking a rule (see [`GroupProblem`]) is not made.
    #[error("that would break a rule of groups: {0}")]
    BreaksTheRulesOfGroups(GroupProblem),
    #[error("that would delete every track, and a project always has a track")]
    WouldDeleteEveryTrack,
    #[error("{group} cannot have the {kind} effect")]
    NotAnEffectOfTheGroup { group: GroupId, kind: EffectKind },
    #[error("{group} does not have the {instance}")]
    TheGroupDoesNotHaveThatEffect { group: GroupId, instance: EffectInstanceId },
    #[error("{property:?} is not the value of an effect of {group}")]
    NotAPropertyOfTheGroup { group: GroupId, property: PropertyId },
    #[error("{item:?} is not in the chain of effects of {owner}")]
    NotInTheChain { owner: Member, item: ChainItemKey },
    #[error("a clip, a track or a group can have at most {limit} effects, and this one has them")]
    TooManyEffects { limit: usize },
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
            groups: Vec::new(),
            next_group_number: 0,
            speed_indicator_layout: SpeedIndicatorLayout::DEFAULT,
            settings: ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS,
            resources: Resources::default(),
            plugin_libraries: PluginLibraries::default(),
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
        Self::from_parts_and_groups(sources, tracks, speed_indicator_layout, Vec::new())
    }

    /// [`Self::from_parts`] with `groups` in the project, which are used as they are, in that order (the tracks, the clips and the racks
    /// are as they were made). It is an error if they break a rule (see [`GroupProblem`]): what is in them has to be in the project, and
    /// has to be in order, as it is checked here whatever the groups came from. The groups are put in before the keyframe crates are made
    /// as long as their keyframes need, because a crate that has no keyframes in it goes then, and is taken out of the groups it was in.
    pub fn from_parts_and_groups(
        sources: Vec<SourceMedia>,
        tracks: Vec<Track>,
        speed_indicator_layout: SpeedIndicatorLayout,
        groups: Vec<Group>,
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
                    Clip::Text(_) | Clip::Crate(_) | Clip::Adjustment(_) | Clip::Generator(_) => continue,
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
        let mut project = Self {
            sources,
            tracks,
            next_track_number,
            groups: Vec::new(),
            next_group_number: 0,
            speed_indicator_layout,
            settings: ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS,
            resources: Resources::default(),
            plugin_libraries: PluginLibraries::default(),
            frame_rate: None,
        }
        .with_groups(groups)?;
        // (A keyframe crate lasts exactly as long as its keyframes need: one that was saved lasting longer, or with nothing in it, is
        // made so.)
        project
            .fit_every_crate()
            .map_err(|error| InconsistentProject::RacksAreWrong {
                problems: format!("a keyframe crate has keyframes that it has no room for: {error}"),
            })?;
        let problems = project.problems_with_the_racks();
        if !problems.is_empty() {
            return Err(InconsistentProject::RacksAreWrong {
                problems: problems.join("; "),
            });
        }
        if let Some(problem) = project.first_problem_with_the_groups() {
            return Err(InconsistentProject::GroupsAreWrong { problem });
        }
        Ok(project)
    }

    /// The same project with `groups` in it, which are used as they are, in that order (the tracks, the clips and the racks are as they
    /// were made). It is an error if they break a rule (see [`GroupProblem`]): what is in them has to be in the project, and has to be in
    /// order, as it is checked here whatever the groups came from. (For a project that is finished, whose keyframe crates last as long as
    /// their keyframes need: a project is made from its parts, as when a project file is loaded, with [`Self::from_parts_and_groups`].)
    pub fn with_groups(mut self, groups: Vec<Group>) -> Result<Self, InconsistentProject> {
        self.next_group_number = groups
            .iter()
            .map(|group| group.id().value() + 1)
            .max()
            .unwrap_or(0)
            .max(u32::try_from(groups.len()).unwrap_or(u32::MAX));
        self.groups = groups;
        match self.first_problem_with_the_groups() {
            None => Ok(self),
            Some(problem) => Err(InconsistentProject::GroupsAreWrong { problem }),
        }
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
    ///
    /// (This does not look at the rules about groups: counting everything again rounds every start and length to a whole frame, and can put
    /// a clip a frame inside the rectangle of a clip group that it was a frame beside. Use [`Self::try_set_frame_rate`] where there can be
    /// groups.)
    pub fn set_frame_rate(&mut self, frame_rate: FrameRate) {
        let before = self.frame_rate();
        self.frame_rate = Some(frame_rate);
        if frame_rate != before {
            for track in &mut self.tracks {
                *track = track.retimed(before, frame_rate);
            }
            // (The keyframes of a crate are counted again with it, and it is made as long as they need: that only makes it shorter.)
            let fitted = self.fit_every_crate();
            debug_assert!(
                fitted.is_ok(),
                "counting a crate again does not make it need more room: {fitted:?}"
            );
        }
    }

    /// [`Self::set_frame_rate`], which is an error, and changes nothing, if counting everything again would break a rule of groups.
    pub fn try_set_frame_rate(&mut self, frame_rate: FrameRate) -> Result<(), EditError> {
        self.keeping_the_groups_valid(|project| {
            project.set_frame_rate(frame_rate);
            Ok(())
        })
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
    /// stay part of the project. A project always has a track, so the last one cannot be deleted. The track and its clips are taken out
    /// of the groups they are in, which are dropped if nothing is left in them.
    pub fn delete_track(&mut self, track: TrackId) -> Result<(), EditError> {
        self.keeping_the_groups_valid(|project| project.delete_track_without_looking_at_the_groups(track))
    }

    /// [`Self::delete_track`], without checking the rules about groups afterwards (for a caller that does it itself, after more edits).
    fn delete_track_without_looking_at_the_groups(&mut self, track: TrackId) -> Result<(), EditError> {
        let position = self.track_position(track).ok_or(EditError::UnknownTrack { track })?;
        if self.tracks.len() == 1 {
            return Err(EditError::CannotDeleteTheOnlyTrack { track });
        }
        let clips_that_go = racks::ids_of_the_clips_on(&self.tracks[position]);
        self.tracks.remove(position);
        // The racks in the drawers of the track and of its clips went with them. Those kept elsewhere that animated them
        // would animate nothing.
        self.remove_the_racks_that_animate(AnimationTarget::Track(track));
        for clip in &clips_that_go {
            self.remove_the_racks_that_animate(AnimationTarget::Clip(*clip));
        }
        for clip in clips_that_go {
            self.take_out_of_the_groups(Member::Clip(clip));
        }
        self.take_out_of_the_groups(Member::Track(track));
        Ok(())
    }

    /// Moves a track to `position` in the list of tracks (0 is the front): it goes in front of the tracks that are
    /// from that place on and behind those before it, and the other tracks keep their order. A `position` that
    /// is past the last place is the last place. It is an error, and nothing is changed, if that would break a rule of groups (a track
    /// that is not in a track group cannot go in the middle of it, and one that is cannot go out of it).
    pub fn move_track(&mut self, track: TrackId, position: usize) -> Result<(), EditError> {
        self.keeping_the_groups_valid(|project| {
            let from = project.track_position(track).ok_or(EditError::UnknownTrack { track })?;
            let moved = project.tracks.remove(from);
            project.tracks.insert(position.min(project.tracks.len()), moved);
            Ok(())
        })
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

        let empty_tracks: Vec<TrackId> = self
            .tracks
            .iter()
            .filter(|track| track.is_empty())
            .map(Track::id)
            .collect();
        if self.groups.is_empty() {
            return Ok(self.put_the_source_on_a_track(source, at_frame, empty_tracks.first().copied()));
        }
        // A track that has nothing on it can still be in the rectangle of a clip group, where only what is in the group may be: the first
        // that the file can go on without breaking a rule is the one it goes on, and if there is none it goes on a new track in front,
        // which is in no group.
        for empty_track in empty_tracks {
            let mut attempt = self.clone();
            let outcome = attempt.put_the_source_on_a_track(source.clone(), at_frame, Some(empty_track));
            if attempt.first_problem_with_the_groups().is_none() {
                *self = attempt;
                return Ok(outcome);
            }
        }
        Ok(self.put_the_source_on_a_track(source, at_frame, None))
    }

    /// Puts a source file in the project and all of it on the track `track`, which is empty, or on a new track in front if that is `None`.
    fn put_the_source_on_a_track(
        &mut self,
        source: SourceMedia,
        at_frame: TimelineFrameIndex,
        track: Option<TrackId>,
    ) -> ImportOutcome {
        let (track_id, created_track) = match track {
            Some(track) => (track, false),
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
        ImportOutcome {
            track: track_id,
            clip: clip_id,
            created_track,
        }
    }

    /// Points a source file at a different file, for example after the original was moved. The
    /// clips are left exactly as they are: no edit is lost.
    ///
    /// The new file must be the same kind of file. A video may run at another frame rate than the old
    /// one: the clips are stored as frames of the project, which is as much as to say as stretches of
    /// time. Its length may differ; the returned [`RelinkOutcome`] reports by how much (in frames of the
    /// project) so that the caller can tell the user.
    /// Changes only where the file `identifier` is, and whether it is included in the project file, to what `link` says, and not
    /// what the project knows of the file (how long it is, how big its picture is). `false` if there is no such source file.
    pub fn set_source_link(&mut self, identifier: SourceMediaIdentifier, link: SourceMediaLink) -> bool {
        match self.sources.iter_mut().find(|source| source.identifier == identifier) {
            Some(source) => {
                source.link = link;
                true
            }
            None => false,
        }
    }

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
    ///
    /// What is in the drawer of a clip is cut with it (see [`Track::split_at_with_edges`]), and the keyframes that are of
    /// the clip but are kept in other drawers go on being what the second half is animated by too: a copy of each of
    /// their racks, of the second half, is put beside it.
    pub fn split_at(
        &mut self,
        timeline_frame: TimelineFrameIndex,
        only: Option<&[ClipId]>,
    ) -> Vec<(TrackId, SplitOutcome)> {
        let mut cut = Vec::new();
        for position in 0..self.tracks.len() {
            if let Some(only) = only {
                let is_one_of_them = self.tracks[position]
                    .placement_covering(timeline_frame)
                    .is_some_and(|placement| only.contains(&placement.id));
                if !is_one_of_them {
                    continue;
                }
            }
            if let Some(outcome) = self.split_the_clip_at(position, timeline_frame) {
                cut.push((self.tracks[position].id(), outcome));
            }
        }
        cut
    }

    /// Cuts the clip that is at `timeline_frame` on the track at `position`, if there is one that is not cut there already.
    fn split_the_clip_at(&mut self, position: usize, timeline_frame: TimelineFrameIndex) -> Option<SplitOutcome> {
        let (clip, frames_in) = {
            let placement = self.tracks[position].placement_covering(timeline_frame)?;
            (
                placement.id,
                timeline_frame.value() - placement.first_timeline_frame.value(),
            )
        };
        // What the property is at the edges of the cut, by all the keyframes there are for it wherever they are kept.
        let edges: CutEdges = self.tracks[position]
            .racks_cut_by(clip, frames_in)
            .into_iter()
            .map(|cut| {
                let keyframes = self.keyframes_of(AnimationTarget::Clip(clip), cut.property);
                let before = cut
                    .keyframes_go_on_beyond
                    .then(|| keyframes.value_at(timeline_frame.value().saturating_sub(1)))
                    .flatten();
                let after = cut
                    .keyframes_come_from_before
                    .then(|| keyframes.value_at(timeline_frame.value()))
                    .flatten();
                (cut.rack, (before, after))
            })
            .collect();
        let outcome = self.tracks[position].split_at_with_edges(timeline_frame, &edges).ok()?;
        self.animate_the_second_half_as_the_first_half_is(outcome);
        // (Both halves are where the clip was, so the rectangles of its groups are what they were: nothing can be broken by this. The
        // second half has the badges of the first in its chain already, because it was cut from it.)
        self.put_the_copy_in_the_groups_of(Member::Clip(outcome.second_half), Member::Clip(outcome.first_half));
        // (A crate that is cut is two crates, each as long as the keyframes in it need, which is shorter than the half it was cut to
        // be where the keyframes do not go as far, and nothing at all for a half that has none.)
        let fitted = self.fit_the_crates(&[
            AnimationTarget::Clip(outcome.first_half),
            AnimationTarget::Clip(outcome.second_half),
        ]);
        debug_assert!(
            fitted.is_ok(),
            "the halves of a crate that is cut only get shorter: {fitted:?}"
        );
        Some(outcome)
    }

    /// The racks kept in other drawers than the clip's own that animate the first half of a clip that was cut (which is what
    /// they animated) have a copy put beside them that animates the second half, so that it has what the whole had.
    fn animate_the_second_half_as_the_first_half_is(&mut self, outcome: SplitOutcome) {
        let first = AnimationTarget::Clip(outcome.first_half);
        let second = AnimationTarget::Clip(outcome.second_half);
        let racks_elsewhere: Vec<(AnimationTarget, Rack)> = self
            .located_racks()
            .filter(|located| located.rack.target() == first && located.owner != first)
            .map(|located| (located.owner, located.rack.clone()))
            .collect();
        for (owner, rack) in racks_elsewhere {
            let position = match owner {
                AnimationTarget::Clip(clip) => self.track_position_holding(clip),
                AnimationTarget::Track(track) => self.track_position(track),
            };
            let Some(position) = position else {
                continue;
            };
            let copy = rack.copy_as(self.tracks[position].make_a_rack_id(), second);
            if let Some(drawer) = self.drawer_mut(owner) {
                drawer.push(copy);
            }
        }
    }

    /// Plays a video or sound clip at `speed` (on top of the speed of its track). It is an error, and nothing is changed, if that would
    /// break a rule of groups: a clip that lasts longer or shorter can have a clip group's rectangle take in another clip.
    pub fn set_clip_speed(&mut self, clip: ClipId, speed: SpeedFactor) -> Result<(), EditError> {
        self.keeping_the_groups_valid(|project| {
            Ok(project.track_holding_mut_or_error(clip)?.set_clip_speed(clip, speed)?)
        })
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
    /// on top of one another, or a rule of groups would be broken: a clip that is in a clip group cannot be moved so that the rectangle
    /// takes in a clip that is not in it, and one that is in none cannot be moved so that it is in the rectangle of one and is in it
    /// without being a member of it: it is put in the clip group whose rectangle it is moved into, see [`Self::move_clips_remembering`]),
    /// none does. The clips all leave before any arrives, so that clips can swap places, or one take the place another leaves.
    pub fn move_clips(&mut self, moves: &[ClipMove]) -> Result<(), EditError> {
        self.move_clips_remembering(moves, &[])
    }

    /// [`Self::move_clips`], without checking the rules about groups afterwards.
    fn move_clips_without_looking_at_the_groups(&mut self, moves: &[ClipMove]) -> Result<(), EditError> {
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
                    drawer: placement.drawer.clone(),
                    effects: placement.effects.clone(),
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
                    drawer: leaving.drawer.clone(),
                    effects: leaving.effects.clone(),
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

    /// Takes a clip off its track. See [`Track::delete_clip`] for what moves. What was in its drawer goes with it, and the
    /// racks kept elsewhere that animated it are taken away, because they would animate nothing. It is taken out of the groups it is in,
    /// which are dropped if nothing is left in them.
    pub fn delete_clip(&mut self, clip: ClipId) -> Result<(), EditError> {
        self.keeping_the_groups_valid(|project| project.delete_clip_without_looking_at_the_groups(clip))
    }

    /// [`Self::delete_clip`], without checking the rules about groups afterwards (for a caller that does it itself, after more edits).
    fn delete_clip_without_looking_at_the_groups(&mut self, clip: ClipId) -> Result<(), EditError> {
        self.track_holding_mut_or_error(clip)?.delete_clip(clip)?;
        self.remove_the_racks_that_animate(AnimationTarget::Clip(clip));
        self.take_out_of_the_groups(Member::Clip(clip));
        Ok(())
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

    /// Makes everything on a track play `speed` times faster, on top of the clips' own speeds. It is an error, and nothing is changed, if
    /// that would break a rule of groups (the clips last longer or shorter).
    pub fn set_track_speed(&mut self, track: TrackId, speed: SpeedFactor) -> Result<(), EditError> {
        self.keeping_the_groups_valid(|project| {
            project.track_mut_or_error(track)?.set_speed(speed);
            Ok(())
        })
    }

    /// Adds a text object to a track at `timeline_frame`, lasting `wanted_frames` frames or as
    /// long as there is room for if that is less. Returns the new clip and how many frames it
    /// lasts, or `None` if there is no such track or a clip is at that frame, or if the text would be in the rectangle of a clip group
    /// (where only what is in the group may be).
    pub fn insert_text(
        &mut self,
        track: TrackId,
        timeline_frame: TimelineFrameIndex,
        object: TextObject,
        wanted_frames: u64,
    ) -> Option<(ClipId, u64)> {
        self.try_insert_text(track, timeline_frame, object, wanted_frames).ok()
    }

    /// [`Self::insert_text`], which says why it could not: [`EditError::NoRoomThere`] if there is no such track or a clip is at that frame,
    /// and [`EditError::BreaksTheRulesOfGroups`] if the text would be in the rectangle of a clip group.
    pub fn try_insert_text(
        &mut self,
        track: TrackId,
        timeline_frame: TimelineFrameIndex,
        object: TextObject,
        wanted_frames: u64,
    ) -> Result<(ClipId, u64), EditError> {
        self.keeping_the_groups_valid(|project| {
            project
                .track_mut(track)
                .and_then(|track| track.insert_text_at(timeline_frame, object, wanted_frames))
                .ok_or(EditError::NoRoomThere)
        })
    }

    /// Adds an adjustment clip to a track at `timeline_frame`, lasting `wanted_frames` frames or as long as there is room for if that is less. Returns the new clip and how many
    /// frames it lasts, or why it could not: [`EditError::NoRoomThere`] if there is no such track or a clip is at that frame, and [`EditError::BreaksTheRulesOfGroups`] if it would
    /// be in the rectangle of a clip group (where only what is in the group may be).
    pub fn try_insert_adjustment_clip(
        &mut self,
        track: TrackId,
        timeline_frame: TimelineFrameIndex,
        wanted_frames: u64,
    ) -> Result<(ClipId, u64), EditError> {
        self.keeping_the_groups_valid(|project| {
            project
                .track_mut(track)
                .and_then(|track| track.insert_adjustment_clip_at(timeline_frame, wanted_frames))
                .ok_or(EditError::NoRoomThere)
        })
    }

    /// Adds a generator clip to a track at `timeline_frame`, lasting `wanted_frames` frames or as long as there is room for if that is less, whose content (what it makes, its
    /// first effect) is of the kind `content`. Returns the new clip and how many frames it lasts, or why it could not: [`EditError::NoRoomThere`] if there is no such track or a clip is
    /// at that frame, and [`EditError::BreaksTheRulesOfGroups`] if it would be in the rectangle of a clip group (where only what is in the group may be).
    pub fn try_insert_generator_clip(
        &mut self,
        track: TrackId,
        timeline_frame: TimelineFrameIndex,
        content: EffectKind,
        wanted_frames: u64,
    ) -> Result<(ClipId, u64), EditError> {
        self.keeping_the_groups_valid(|project| {
            project
                .track_mut(track)
                .and_then(|track| track.insert_generator_clip_at(timeline_frame, content, wanted_frames))
                .ok_or(EditError::NoRoomThere)
        })
    }

    /// The clip whose content, what it makes, is the effect named `instance` in the chain of `owner`, if `owner` is a generator clip and that is its content. **The content of a
    /// generator cannot be taken off, switched off, copied or moved.**
    pub fn the_clip_whose_content_is(&self, owner: Member, instance: EffectInstanceId) -> Option<ClipId> {
        let Member::Clip(clip) = owner else {
            return None;
        };
        let placement = self.placement_of(clip)?;
        (placement.clip.as_generator()?.content() == instance).then_some(clip)
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
    /// is less. Returns how long it lasts now. What is in its drawer is shortened with it (see [`Track::set_clip_duration`]). It is
    /// an error, and nothing is changed, if that would break a rule of groups (a clip that lasts longer can have a clip group's rectangle
    /// take in another clip).
    pub fn set_clip_duration(&mut self, clip: ClipId, frames: u64) -> Result<u64, EditError> {
        self.keeping_the_groups_valid(|project| {
            Ok(project
                .track_holding_mut_or_error(clip)?
                .set_clip_duration(clip, frames)?)
        })
    }
}

impl Default for Project {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod adjustment_clip_tests;
#[cfg(test)]
mod animating_tests;
#[cfg(test)]
mod crates_tests;
#[cfg(test)]
mod duplicating_tests;
#[cfg(test)]
mod effect_editing_tests;
#[cfg(test)]
mod generator_clip_tests;
#[cfg(test)]
mod keyframe_copying_tests;
#[cfg(test)]
mod keyframe_crossing_tests;
#[cfg(test)]
mod rack_editing_tests;
#[cfg(test)]
mod rack_merging_tests;
#[cfg(test)]
mod rack_ordering_tests;
#[cfg(test)]
mod racks_tests;
#[cfg(test)]
mod resource_editing_tests;
#[cfg(test)]
mod source_colour_tests;
#[cfg(test)]
mod tests;
