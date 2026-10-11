//! Everything the user can ask for. The parts of the screen only describe what was asked (by
//! pushing a [`Command`]); one place afterwards decides what to do about each, so that no drawing
//! code ever has to change the project while it is still being drawn from.

use super::history::EditKey;
use super::selection::SelectionChange;
use crate::project::{
    ClipLocation, Gain, Placement, ProjectSettings, Settings, SourceMediaIdentifier, SpeedFactor, SpeedIndicatorLayout,
    TextObject, TimelineFrameIndex, TrackHeight, TrackId,
};

/// Something the user has asked for.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    // The File menu.
    LoadProject,
    SaveProject,
    NewProject,
    /// Choose a video or a sound file and put all of it on the timeline, at the current frame.
    ImportFile,
    ExportVideo,

    // The Edit menu.
    /// Go back to the project as it was before the latest edit.
    Undo,
    /// Do again the edit that was last undone.
    Redo,
    /// Select no clip.
    Deselect,
    ShowProjectSettings,
    ShowGlobalSettings,

    /// The project's own settings were changed in the project settings window.
    SetProjectSettings(ProjectSettings),
    /// The global settings were changed in the global settings window.
    SetGlobalSettings(Settings),

    // The play buttons.
    Pause,
    Play,
    Stop,

    /// Move the current frame to a place (by dragging the slider along the bottom, or clicking
    /// the ruler above the tracks).
    SeekTo(TimelineFrameIndex),

    /// Move the current frame this many frames on (or, if negative, back), with the arrow keys.
    MoveCurrentFrameBy(i64),

    /// Change which clips are selected (by clicking one, or drawing a rectangle round some):
    /// select `clips` and nothing else, or as well as what is selected, or the other way round for
    /// each of them, as `change` says.
    Select {
        clips: Vec<ClipLocation>,
        change: SelectionChange,
    },

    /// Cut the clips under this frame in two (with the S key): only those in `only` if that is
    /// given, and otherwise every clip there is under the frame, on every track.
    SplitAt {
        frame: TimelineFrameIndex,
        only: Option<Vec<ClipLocation>>,
    },

    /// Take clips off their tracks (with the Delete key).
    DeleteClips(Vec<ClipLocation>),

    /// Move clips along their tracks, by dragging them: each to start at the frame given for it.
    MoveClips(Vec<(ClipLocation, TimelineFrameIndex)>),

    // The settings of a clip, from the box that opens above it.
    SetClipSpeed {
        clip: ClipLocation,
        speed_factor: SpeedFactor,
    },
    SetClipGain {
        clip: ClipLocation,
        gain: Gain,
    },
    SetClipMuted {
        clip: ClipLocation,
        muted: bool,
    },
    /// A video or text clip was moved, resized or turned on the preview.
    SetClipPlacement {
        clip: ClipLocation,
        placement: Placement,
    },
    /// Choose again the file a clip is made from, because it cannot be found.
    RelinkSource(SourceMediaIdentifier),

    // The tracks.
    /// Add an empty track in front of the others.
    AddTrack,
    SetTrackTitle {
        track: TrackId,
        title: String,
    },
    SetTrackSpeed {
        track: TrackId,
        speed_factor: SpeedFactor,
    },
    SetTrackGain {
        track: TrackId,
        gain: Gain,
    },
    SetTrackMuted {
        track: TrackId,
        muted: bool,
    },
    /// Take a track, and everything on it, out of the project (from the menu of its title box).
    DeleteTrack(TrackId),
    /// The row of a track was made taller or shorter, by dragging the boundary under its title box.
    SetTrackHeight {
        track: TrackId,
        height: TrackHeight,
    },

    // Text objects.
    /// Put a new text object on a track, starting at a frame, and open the window to write it in.
    NewTextObject {
        track: TrackId,
        frame: TimelineFrameIndex,
    },
    /// Open the window that edits a text object.
    EditTextObject(ClipLocation),
    /// A text object was changed: what it says, how it looks, or where it is on the picture.
    SetTextObject {
        clip: ClipLocation,
        object: TextObject,
    },
    /// A text clip or a picture was made to last this many frames.
    SetClipDuration {
        clip: ClipLocation,
        frames: u64,
    },

    /// Move or resize the fast-forward symbol.
    SetSpeedIndicatorLayout(SpeedIndicatorLayout),
}

impl Command {
    /// Whether doing this can change the project, so that it may be an edit that can be undone.
    /// (Everything else leaves the project alone, or changes what is not part of what is edited:
    /// the settings, the selection, which frame is current.) Written out for every command, with
    /// no catch-all, so that a command that is added has to be put on one side or the other.
    pub fn may_change_the_project(&self) -> bool {
        match self {
            Self::SplitAt { .. }
            | Self::DeleteClips(_)
            | Self::MoveClips(_)
            | Self::SetClipSpeed { .. }
            | Self::SetClipGain { .. }
            | Self::SetClipMuted { .. }
            | Self::SetClipPlacement { .. }
            | Self::AddTrack
            | Self::DeleteTrack(_)
            | Self::SetTrackTitle { .. }
            | Self::SetTrackSpeed { .. }
            | Self::SetTrackGain { .. }
            | Self::SetTrackMuted { .. }
            | Self::SetTrackHeight { .. }
            | Self::NewTextObject { .. }
            | Self::SetTextObject { .. }
            | Self::SetClipDuration { .. }
            | Self::SetSpeedIndicatorLayout(_) => true,

            Self::LoadProject
            | Self::SaveProject
            | Self::NewProject
            | Self::ImportFile
            | Self::ExportVideo
            | Self::Undo
            | Self::Redo
            | Self::Deselect
            | Self::ShowProjectSettings
            | Self::ShowGlobalSettings
            | Self::SetProjectSettings(_)
            | Self::SetGlobalSettings(_)
            | Self::Pause
            | Self::Play
            | Self::Stop
            | Self::SeekTo(_)
            | Self::MoveCurrentFrameBy(_)
            | Self::Select { .. }
            | Self::RelinkSource(_)
            | Self::EditTextObject(_) => false,
        }
    }

    /// What this command is about, if it is the kind of edit that a gesture makes over and over (a
    /// slider dragged, a title typed): edits about the same thing, one after another, are one step
    /// of the history. `None` for an edit that is to be a step of its own, and for what is no edit.
    pub fn gesture_key(&self) -> Option<EditKey> {
        match self {
            Self::MoveClips(starts) => {
                let mut clips: Vec<ClipLocation> = starts.iter().map(|(clip, _)| *clip).collect();
                clips.sort();
                Some(EditKey::about(&("the place of", clips)))
            }
            Self::SetClipSpeed { clip, .. } => Some(EditKey::about(&("the speed of", clip))),
            Self::SetClipGain { clip, .. } => Some(EditKey::about(&("the gain of", clip))),
            Self::SetClipPlacement { clip, .. } => Some(EditKey::about(&("the place on the picture of", clip))),
            Self::SetTrackTitle { track, .. } => Some(EditKey::about(&("the title of", track))),
            Self::SetTrackSpeed { track, .. } => Some(EditKey::about(&("the speed of", track))),
            Self::SetTrackGain { track, .. } => Some(EditKey::about(&("the gain of", track))),
            Self::SetTrackHeight { track, .. } => Some(EditKey::about(&("the height of", track))),
            Self::SetTextObject { clip, .. } => Some(EditKey::about(&("the text object of", clip))),
            Self::SetClipDuration { clip, .. } => Some(EditKey::about(&("how long it lasts of", clip))),
            Self::SetSpeedIndicatorLayout(_) => Some(EditKey::about(&"where the fast-forward symbol is")),
            _ => None,
        }
    }
}

/// What the edits among `commands` are about, together: `Some` if there are edits and every one is
/// the kind that a gesture makes over and over, so that the same gesture, in the next frame, has the
/// same key. (The commands of one frame are one step of the history, and are joined to the step
/// before if that was about the same.)
pub fn gesture_key_of_the_edits_among(commands: &[Command]) -> Option<EditKey> {
    let mut keys = Vec::new();
    for command in commands.iter().filter(|command| command.may_change_the_project()) {
        keys.push(command.gesture_key()?);
    }
    if keys.is_empty() {
        return None;
    }
    keys.sort();
    keys.dedup();
    Some(EditKey::about(&keys))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Angle, CanvasArea, ClipId, TextObject};

    fn clip(number: u32) -> ClipLocation {
        ClipLocation {
            track: TrackId::new(0),
            clip: ClipId::new(number),
        }
    }

    fn speed_of(clip_number: u32, factor: u32) -> Command {
        Command::SetClipSpeed {
            clip: clip(clip_number),
            speed_factor: SpeedFactor::new(factor).unwrap(),
        }
    }

    #[test]
    fn what_changes_the_project_and_what_does_not_is_told_apart() {
        for edit in [
            Command::AddTrack,
            Command::DeleteTrack(TrackId::new(0)),
            Command::DeleteClips(vec![clip(0)]),
            Command::SplitAt {
                frame: TimelineFrameIndex::new(3),
                only: None,
            },
            speed_of(0, 2),
            Command::SetTrackHeight {
                track: TrackId::new(0),
                height: TrackHeight::DEFAULT,
            },
            Command::SetTextObject {
                clip: clip(0),
                object: TextObject::default(),
            },
        ] {
            assert!(edit.may_change_the_project(), "{edit:?}");
        }
        for not_an_edit in [
            Command::Undo,
            Command::Redo,
            Command::Deselect,
            Command::Play,
            Command::SeekTo(TimelineFrameIndex::new(3)),
            Command::Select {
                clips: vec![clip(0)],
                change: SelectionChange::Replace,
            },
            Command::SetProjectSettings(ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS),
            Command::EditTextObject(clip(0)),
        ] {
            assert!(!not_an_edit.may_change_the_project(), "{not_an_edit:?}");
        }
    }

    #[test]
    fn the_same_gesture_has_the_same_key_in_every_frame_and_different_things_have_different_keys() {
        assert_eq!(speed_of(0, 2).gesture_key(), speed_of(0, 50).gesture_key());
        assert_ne!(
            speed_of(0, 2).gesture_key(),
            speed_of(1, 2).gesture_key(),
            "another clip"
        );
        assert_ne!(
            speed_of(0, 2).gesture_key(),
            Command::SetClipGain {
                clip: clip(0),
                gain: Gain::UNCHANGED
            }
            .gesture_key(),
            "the speed and the gain of the same clip"
        );
        assert!(speed_of(0, 2).gesture_key().is_some());
    }

    #[test]
    fn dragging_clips_is_one_gesture_for_those_clips_wherever_they_have_got_to() {
        let moved = |clips: &[(u32, u64)]| {
            Command::MoveClips(
                clips
                    .iter()
                    .map(|&(number, start)| (clip(number), TimelineFrameIndex::new(start)))
                    .collect(),
            )
        };
        assert!(moved(&[(0, 10)]).may_change_the_project());
        assert!(moved(&[(0, 10)]).gesture_key().is_some());
        assert_eq!(moved(&[(0, 10)]).gesture_key(), moved(&[(0, 99)]).gesture_key());
        assert_eq!(
            moved(&[(0, 10), (1, 50)]).gesture_key(),
            moved(&[(1, 70), (0, 30)]).gesture_key(),
            "the same clips in any order"
        );
        assert_ne!(
            moved(&[(0, 10)]).gesture_key(),
            moved(&[(1, 10)]).gesture_key(),
            "other clips"
        );
        assert_ne!(
            moved(&[(0, 10)]).gesture_key(),
            moved(&[(0, 10), (1, 50)]).gesture_key(),
            "more clips"
        );
    }

    #[test]
    fn moving_resizing_or_turning_a_clip_on_the_preview_is_one_gesture_per_clip() {
        let placed = |clip_number: u32, degrees: f32| Command::SetClipPlacement {
            clip: clip(clip_number),
            placement: Placement::new(CanvasArea::DEFAULT, Angle::from_degrees(degrees)),
        };
        assert!(placed(0, 10.0).may_change_the_project());
        assert!(placed(0, 10.0).gesture_key().is_some());
        assert_eq!(placed(0, 10.0).gesture_key(), placed(0, 80.0).gesture_key());
        assert_ne!(
            placed(0, 10.0).gesture_key(),
            placed(1, 10.0).gesture_key(),
            "another clip"
        );
        assert_ne!(
            placed(0, 10.0).gesture_key(),
            Command::SetTextObject {
                clip: clip(0),
                object: TextObject::default()
            }
            .gesture_key(),
            "what the text says is another gesture than where it is"
        );
    }

    #[test]
    fn dragging_the_height_of_a_track_is_one_gesture_per_track() {
        let height_of = |track: u32, points: f32| Command::SetTrackHeight {
            track: TrackId::new(track),
            height: TrackHeight::from_points(points),
        };
        assert!(height_of(0, 50.0).gesture_key().is_some());
        assert_eq!(height_of(0, 50.0).gesture_key(), height_of(0, 90.0).gesture_key());
        assert_ne!(height_of(0, 50.0).gesture_key(), height_of(1, 50.0).gesture_key());
        assert_ne!(
            height_of(0, 50.0).gesture_key(),
            Command::SetTrackGain {
                track: TrackId::new(0),
                gain: Gain::UNCHANGED
            }
            .gesture_key()
        );
    }

    #[test]
    fn a_cut_or_a_delete_or_a_new_track_is_never_joined_to_another_edit() {
        for edit in [
            Command::AddTrack,
            Command::DeleteTrack(TrackId::new(0)),
            Command::DeleteClips(vec![clip(0)]),
            Command::SplitAt {
                frame: TimelineFrameIndex::new(3),
                only: None,
            },
            Command::SetClipMuted {
                clip: clip(0),
                muted: true,
            },
        ] {
            assert_eq!(edit.gesture_key(), None, "{edit:?}");
        }
    }

    #[test]
    fn the_key_of_a_frame_is_that_of_all_its_edits_together_and_is_none_if_one_is_never_joined() {
        let one_clip = [speed_of(0, 2)];
        let two_clips = [speed_of(0, 2), speed_of(1, 2)];
        assert!(gesture_key_of_the_edits_among(&one_clip).is_some());
        assert_eq!(
            gesture_key_of_the_edits_among(&two_clips),
            gesture_key_of_the_edits_among(&[speed_of(1, 9), speed_of(0, 9)]),
            "the same clips in any order and at any speed"
        );
        assert_ne!(
            gesture_key_of_the_edits_among(&one_clip),
            gesture_key_of_the_edits_among(&two_clips)
        );
        assert_eq!(
            gesture_key_of_the_edits_among(&[speed_of(0, 2), Command::AddTrack]),
            None,
            "one edit that is never joined is enough"
        );
        assert_eq!(
            gesture_key_of_the_edits_among(&[Command::Play, Command::SeekTo(TimelineFrameIndex::new(1))]),
            None,
            "no edits at all"
        );
        assert_eq!(
            gesture_key_of_the_edits_among(&[Command::Play, speed_of(0, 2)]),
            gesture_key_of_the_edits_among(&one_clip),
            "what is not an edit does not count"
        );
    }
}
