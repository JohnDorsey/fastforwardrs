//! Everything the user can ask for. The parts of the screen only describe what was asked (by
//! pushing a [`Command`]); one place afterwards decides what to do about each, so that no drawing
//! code ever has to change the project while it is still being drawn from.

use crate::project::{
    ClipLocation, Gain, ProjectSettings, Settings, SourceMediaIdentifier, SpeedFactor, SpeedIndicatorLayout,
    TextObject, TimelineFrameIndex, TrackId,
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

    /// Make this clip the selected one (by clicking it).
    SelectClip(ClipLocation),

    /// Cut the clips under this frame in two (with the S key): only `only` if that is given, and
    /// otherwise every clip there is under the frame, on every track.
    SplitAt {
        frame: TimelineFrameIndex,
        only: Option<ClipLocation>,
    },

    /// Take a clip off its track (with the Delete key).
    DeleteClip(ClipLocation),

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
    /// A text clip was made to last this many frames.
    SetTextDuration {
        clip: ClipLocation,
        frames: u64,
    },

    /// Move or resize the fast-forward symbol.
    SetSpeedIndicatorLayout(SpeedIndicatorLayout),
}
