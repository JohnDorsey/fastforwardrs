//! Everything the user can ask for. The parts of the screen only describe what was asked (by
//! pushing a [`Command`]); one place afterwards decides what to do about each, so that no drawing
//! code ever has to change the project while it is still being drawn from.

use crate::project::{SpeedFactor, SpeedIndicatorLayout, TimelineFrameIndex};

/// Something the user has asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    // The File menu.
    LoadProject,
    SaveProject,
    NewProject,
    ImportVideo,
    ExportVideo,

    // The play buttons.
    Pause,
    Play,
    Stop,

    /// Move the current frame to a place (by dragging the slider along the bottom, or clicking
    /// the ruler above the video track).
    SeekTo(TimelineFrameIndex),

    /// Move the current frame this many frames on (or, if negative, back), with the arrow keys.
    MoveCurrentFrameBy(i64),

    /// Cut the clip under this frame in two (with the S key).
    SplitAt(TimelineFrameIndex),

    /// Play a clip at a different speed.
    SetClipSpeed {
        clip_index: usize,
        speed_factor: SpeedFactor,
    },

    /// Move or resize the fast-forward symbol.
    SetSpeedIndicatorLayout(SpeedIndicatorLayout),
}
