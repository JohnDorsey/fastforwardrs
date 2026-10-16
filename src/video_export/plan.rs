// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Working out what an export has to do, from a project.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use super::runs::{VideoRun, find_the_runs};
use crate::composition::{LayerContent, PicturePlan, plan_pictures, plan_sound};
use crate::engine::FrameColour;
use crate::frame_scene::colour_of_the_export;
use crate::media::export::{SoundGraphSpecification, sound_graph_specification, sources_of_sound};
use crate::project::{
    FrameRate, OutputColourSpace, Project, Settings, SourceMediaIdentifier, SpeedIndicatorLayout, TimelineFrameIndex,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ExportPlanError {
    #[error("there is nothing to export: the project has nothing on its tracks")]
    NothingToExport,
}

impl ExportPlanError {
    /// What a message that says it is headed with.
    pub fn heading(self) -> &'static str {
        match self {
            Self::NothingToExport => "There is nothing to export",
        }
    }
}

/// A video file that the picture is made from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoToExport {
    /// How big its picture is as it is shown.
    pub display_size_in_pixels: (u32, u32),
    /// How many pictures a second it has.
    pub frame_rate: FrameRate,
}

/// A picture file that the picture is made from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageToExport {
    /// How big it is as it is shown.
    pub display_size_in_pixels: (u32, u32),
    /// The biggest box that a layer of it has on the finished picture, in pixels: it is decoded as big as that (and no bigger, as nothing of it is seen past that).
    pub largest_box_in_pixels: (u32, u32),
}

/// The sound of the finished video, as the encoder is given it.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundToExport {
    /// The files the sound is made from, which are the first inputs of the encoder, in this order (`specification` refers to them by their place in this list).
    pub source_files: Vec<SourceMediaIdentifier>,
    pub specification: SoundGraphSpecification,
}

/// Everything an export is going to do, worked out in advance.
#[derive(Clone, Debug)]
pub struct ExportPlan {
    /// The picture at every frame: the sections, with what the keyframes of every layer say left to be worked out at each frame (the export makes every frame as it is,
    /// and does not cut the animation into stretches).
    pub pictures: PicturePlan,
    /// The size of the finished picture, in pixels (the project's picture size made even: video encoders for most formats can only make pictures with an even
    /// width and height).
    pub picture_width_in_pixels: u32,
    pub picture_height_in_pixels: u32,
    pub frame_rate: FrameRate,
    /// How many frames the finished video will have; used to show progress.
    pub total_frame_count: u64,
    /// How many frames of the finished video are missing the picture of a video, only because the video file has fewer pictures than its clips need. That only
    /// happens when the project was pointed at a shorter video than the one it was edited on (which keeps every edit).
    pub frames_missing_a_picture: u64,
    /// The source files that anything is made from: a video, a picture, a sound. The locations the export is given are in this order.
    pub source_files: Vec<SourceMediaIdentifier>,
    /// The stretches of video, each of which is a decode process of its own, in the order they begin.
    pub runs: Vec<VideoRun>,
    /// What is known of the video files that are used.
    pub videos: BTreeMap<SourceMediaIdentifier, VideoToExport>,
    /// What is known of the picture files that are used.
    pub images: BTreeMap<SourceMediaIdentifier, ImageToExport>,
    /// The sound, if the project has any.
    pub sound: Option<SoundToExport>,
    pub has_audio: bool,
    /// Where the fast-forward symbol goes (the same symbol in every sped-up stretch).
    pub symbol_layout: SpeedIndicatorLayout,
    /// What every frame is put together in, and what it is encoded as (from the settings: the working colour space and the output colour space).
    pub colour: FrameColour,
    /// What the numbers of the finished video are: it says what the matrix, the primaries and the transfer of the file are (see `output_colour_for`).
    pub output_colour_space: OutputColourSpace,
}

impl ExportPlan {
    /// The size of the finished picture, in pixels.
    pub fn picture_size(&self) -> (u32, u32) {
        (self.picture_width_in_pixels, self.picture_height_in_pixels)
    }

    /// The run of the video of `clip` that has `timeline_frame` in it, if there is one.
    pub fn run_of(&self, clip: crate::project::ClipId, timeline_frame: u64) -> Option<usize> {
        self.runs.iter().position(|run| {
            run.clip == clip && run.first_timeline_frame <= timeline_frame && timeline_frame < run.end_timeline_frame
        })
    }
}

/// Plans the export of `project`.
pub fn plan_export(project: &Project, settings: Settings) -> Result<ExportPlan, ExportPlanError> {
    let pictures = plan_pictures(project, |_| true);
    if pictures.total_frame_count == 0 {
        return Err(ExportPlanError::NothingToExport);
    }
    let sound = plan_sound(project, TimelineFrameIndex::new(0), |_| true);

    let (picture_width, picture_height) = pictures.picture_size;
    let picture_size = (even(picture_width), even(picture_height));

    let sound_sources = sources_of_sound(&sound);
    let mut used_sources: BTreeSet<SourceMediaIdentifier> = sound_sources.clone();
    let mut videos = BTreeMap::new();
    let mut image_sources = BTreeSet::new();
    for layer in pictures.sections.iter().flat_map(|section| &section.layers) {
        match &layer.content {
            LayerContent::Video(video) => {
                used_sources.insert(video.source);
                if let Some(properties) = project
                    .source(video.source)
                    .and_then(|source| source.properties.video())
                {
                    videos.insert(
                        video.source,
                        VideoToExport {
                            display_size_in_pixels: (
                                properties.display_width_in_pixels,
                                properties.display_height_in_pixels,
                            ),
                            frame_rate: properties.frame_rate,
                        },
                    );
                }
            }
            LayerContent::Image { source } => {
                used_sources.insert(*source);
                image_sources.insert(*source);
            }
            LayerContent::Text(_) => {}
        }
    }
    let runs = find_the_runs(&pictures, picture_size);
    let images = image_sources
        .into_iter()
        .filter_map(|source| {
            let display_size_in_pixels = project.source(source)?.properties.display_size_in_pixels()?;
            Some((
                source,
                ImageToExport {
                    display_size_in_pixels,
                    largest_box_in_pixels: super::runs::largest_box_of_an_image(&pictures, source, picture_size),
                },
            ))
        })
        .collect();

    let sound_files: Vec<SourceMediaIdentifier> = sound_sources.into_iter().collect();
    let has_audio = !sound.strips.is_empty();
    let sound = has_audio.then(|| SoundToExport {
        specification: sound_graph_specification(&sound, &sound_files, settings.sped_up_clip_sound),
        source_files: sound_files,
    });
    Ok(ExportPlan {
        picture_width_in_pixels: picture_size.0,
        picture_height_in_pixels: picture_size.1,
        frame_rate: pictures.frame_rate,
        total_frame_count: pictures.total_frame_count,
        frames_missing_a_picture: pictures.frames_missing_a_picture,
        source_files: used_sources.into_iter().collect(),
        runs,
        videos,
        images,
        sound,
        has_audio,
        symbol_layout: project.speed_indicator_layout(),
        colour: colour_of_the_export(settings),
        output_colour_space: settings.output_colour_space,
        pictures,
    })
}

/// The largest even number that is not above `size`, and at least 2. Video encoders for most formats can only make pictures with an even width and height.
fn even(size: u32) -> u32 {
    (size & !1).max(2)
}

#[cfg(test)]
mod tests;
