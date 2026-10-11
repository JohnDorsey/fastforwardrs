//! Project files of versions 1 and 2, which are still read. They had one list of videos and a
//! single video track; they become a project with one track holding all of the clips.
//!
//! (Version 1 is the same as 2 but has no `settings`, which means that the project follows every
//! global setting.)

use serde::Deserialize;

use super::document::{ProjectSettingsDocument, SpeedIndicatorDocument};
use crate::project::{FrameRate, SourceFrameIndex, SourceMediaIdentifier, SpeedFactor};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LegacyProjectDocument {
    // Read and checked before this (see `FileHeader`); named here so that they are not "unknown".
    #[serde(rename = "file_format")]
    pub _file_format: String,
    #[serde(rename = "file_format_version")]
    pub _file_format_version: u32,
    pub source_videos: Vec<LegacySourceVideoDocument>,
    pub video_track: LegacyVideoTrackDocument,
    pub speed_indicator: SpeedIndicatorDocument,
    #[serde(default)]
    pub settings: Option<ProjectSettingsDocument>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LegacySourceVideoDocument {
    pub identifier: SourceMediaIdentifier,
    pub absolute_path: String,
    pub relative_path: Option<String>,
    pub frame_rate: FrameRate,
    pub total_frame_count: u64,
    pub display_width_in_pixels: u32,
    pub display_height_in_pixels: u32,
    pub has_audio: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LegacyVideoTrackDocument {
    pub clips: Vec<LegacyClipDocument>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LegacyClipDocument {
    pub source_video_identifier: SourceMediaIdentifier,
    pub first_source_frame: SourceFrameIndex,
    pub end_source_frame: SourceFrameIndex,
    pub speed_factor: SpeedFactor,
}
