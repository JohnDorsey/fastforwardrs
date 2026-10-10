//! The exact shape of a project file on disk.
//!
//! These types are deliberately separate from the editing model in [`crate::project`]: the file
//! format has to stay readable by future versions, while the model is free to change. Converting
//! between the two is where files from the outside world get validated.

use serde::{Deserialize, Serialize};

use crate::project::{FrameRate, SourceFrameIndex, SourceVideoIdentifier, SpeedFactor};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProjectDocument {
    /// Always [`super::FILE_FORMAT_NAME`]; lets us recognise (and refuse) other JSON files.
    pub file_format: String,
    pub file_format_version: u32,
    pub source_videos: Vec<SourceVideoDocument>,
    pub video_track: VideoTrackDocument,
    pub speed_indicator: SpeedIndicatorDocument,
    /// What the project has set for itself. Left out when it follows every global setting, and in
    /// files from before there were settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<ProjectSettingsDocument>,
}

/// The settings a project has set for itself. A setting that follows the global one is not in the
/// file at all, which is what "not saved" means for it.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProjectSettingsDocument {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autosave: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mute_sped_up_clips: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceVideoDocument {
    pub identifier: SourceVideoIdentifier,
    pub absolute_path: String,
    /// Relative to the folder that holds the project file. Always written with `/` separators so
    /// that it means the same thing on every operating system.
    pub relative_path: Option<String>,
    pub frame_rate: FrameRate,
    pub total_frame_count: u64,
    pub display_width_in_pixels: u32,
    pub display_height_in_pixels: u32,
    pub has_audio: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct VideoTrackDocument {
    pub clips: Vec<ClipDocument>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ClipDocument {
    pub source_video_identifier: SourceVideoIdentifier,
    pub first_source_frame: SourceFrameIndex,
    /// The frame just after the clip's last frame.
    pub end_source_frame: SourceFrameIndex,
    pub speed_factor: SpeedFactor,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SpeedIndicatorDocument {
    pub left_fraction_of_frame_width: f32,
    pub top_fraction_of_frame_height: f32,
    pub height_fraction_of_frame_height: f32,
}
