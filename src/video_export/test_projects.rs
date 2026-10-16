// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Projects for the tests of the export, which have the files of the videos, pictures and sounds that they are made of only on paper (there is nothing to read).

use std::path::PathBuf;

use crate::project::{
    ClipId, FrameRate, Project, SourceAudioProperties, SourceImageProperties, SourceMedia, SourceMediaIdentifier,
    SourceMediaLink, SourceVideoProperties, SpeedFactor, TimelineFrameIndex,
};

pub(crate) fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

pub(crate) fn speed(factor: u32) -> SpeedFactor {
    SpeedFactor::new(factor).unwrap()
}

pub(crate) fn video_source(identifier: u32, width: u32, height: u32, frames: u64, has_audio: bool) -> SourceMedia {
    video_source_at(
        identifier,
        width,
        height,
        frames,
        FrameRate::new(30, 1).unwrap(),
        has_audio,
    )
}

/// Like [`video_source`], for a video that has `frame_rate` pictures a second.
pub(crate) fn video_source_at(
    identifier: u32,
    width: u32,
    height: u32,
    frames: u64,
    frame_rate: FrameRate,
    has_audio: bool,
) -> SourceMedia {
    SourceMedia::video(
        SourceMediaIdentifier::new(identifier),
        SourceMediaLink::from_absolute_path(PathBuf::from(format!("/videos/{identifier}.mp4"))),
        SourceVideoProperties {
            frame_rate,
            total_frame_count: frames,
            display_width_in_pixels: width,
            display_height_in_pixels: height,
            has_audio,
        },
    )
}

pub(crate) fn picture_source(identifier: u32, width: u32, height: u32) -> SourceMedia {
    SourceMedia::image(
        SourceMediaIdentifier::new(identifier),
        SourceMediaLink::from_absolute_path(PathBuf::from(format!("/pictures/{identifier}.png"))),
        SourceImageProperties {
            display_width_in_pixels: width,
            display_height_in_pixels: height,
        },
    )
}

pub(crate) fn audio_source(identifier: u32, seconds: u64) -> SourceMedia {
    SourceMedia::audio(
        SourceMediaIdentifier::new(identifier),
        SourceMediaLink::from_absolute_path(PathBuf::from(format!("/sounds/{identifier}.mp3"))),
        SourceAudioProperties {
            duration_in_microseconds: seconds * 1_000_000,
        },
    )
}

/// A project of one video of 300 frames (ten seconds), with its clip on the first track.
pub(crate) fn project_with_video(width: u32, height: u32, has_audio: bool) -> Project {
    let mut project = Project::empty();
    project
        .import_source(video_source(1, width, height, 300, has_audio), frame(0))
        .unwrap();
    project
}

pub(crate) fn clip_location(project: &Project, track_position: usize, clip_number: usize) -> ClipId {
    project.tracks()[track_position].entries()[clip_number].id()
}
