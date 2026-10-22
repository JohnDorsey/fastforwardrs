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
    FrameRate, InputColourOverride, OutputColourSpace, Project, Settings, SourceMediaIdentifier, SpeedIndicatorLayout,
    TimelineFrameIndex, fit_inside_picture,
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
    /// What the user says the colours of the video are, instead of what it says.
    pub colour: InputColourOverride,
}

/// A picture file that the picture is made from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageToExport {
    /// How big it is as it is shown.
    pub display_size_in_pixels: (u32, u32),
    /// The biggest box that a layer of it has on the finished picture, in pixels: it is decoded as big as that (and no bigger, as nothing of it is seen past that).
    pub largest_box_in_pixels: (u32, u32),
    /// What the user says the colours of the picture are, instead of sRGB.
    pub colour: InputColourOverride,
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

    /// This plan as a lead-up needs it (EN-14, the lead-up on a seek): **for the frames `frames` only, with the pictures made as the preview makes them**. The video of each run is cut to the
    /// frames (a run that is not seen at any of them is left out, and the one that is begins at the picture that is seen at the first of them), and it is delivered as the preview delivers
    /// it: as big as it can be inside `preview_picture_size`, which is the size of the picture of the plan from now on. So the pictures that the lead-up makes the states of the effects from are
    /// the size of the pictures that the preview makes the next frame from, and a state is of the size of the picture that it was made from.
    #[must_use]
    pub fn for_a_lead_up(&self, frames: std::ops::Range<u64>, preview_picture_size: (u32, u32)) -> Self {
        let runs = self
            .runs
            .iter()
            .filter_map(|run| {
                let first = run.first_timeline_frame.max(frames.start);
                let end = run.end_timeline_frame.min(frames.end);
                if first >= end {
                    return None;
                }
                let speed = u64::from(run.speed.value());
                let skipped = first - run.first_timeline_frame;
                let (display_width, display_height) = self
                    .videos
                    .get(&run.source)
                    .map_or(preview_picture_size, |video| video.display_size_in_pixels);
                let fit = fit_inside_picture(
                    display_width,
                    display_height,
                    preview_picture_size.0,
                    preview_picture_size.1,
                );
                Some(VideoRun {
                    first_source_frame: run.first_source_frame + skipped * speed,
                    // (The file may have fewer pictures than the clip was told it has: what is left of the run is never more than there was.)
                    source_frame_count: run
                        .source_frame_count
                        .saturating_sub(skipped * speed)
                        .min((end - first) * speed),
                    first_timeline_frame: first,
                    end_timeline_frame: end,
                    delivered_size_in_pixels: (fit.width_in_pixels, fit.height_in_pixels),
                    ..run.clone()
                })
            })
            .collect();
        Self {
            runs,
            picture_width_in_pixels: preview_picture_size.0,
            picture_height_in_pixels: preview_picture_size.1,
            ..self.clone()
        }
    }
}

/// Plans the export of `project`.
pub fn plan_export(project: &Project, settings: Settings) -> Result<ExportPlan, ExportPlanError> {
    plan_export_of_the_files_that_can_be_read(project, settings, |_| true)
}

/// Plans the export of `project` as it would be if only the files for which `can_be_read` says so could be read: a video from a file that cannot has no pictures (as it has none where its file has no
/// more). **What the window shows is planned so** (`plan_pictures` is given the same function), and a lead-up is made from this plan.
pub fn plan_export_of_the_files_that_can_be_read(
    project: &Project,
    settings: Settings,
    can_be_read: impl Fn(SourceMediaIdentifier) -> bool,
) -> Result<ExportPlan, ExportPlanError> {
    let pictures = plan_pictures(project, can_be_read);
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
                            colour: project.source_colour(video.source).unwrap_or_default(),
                        },
                    );
                }
            }
            LayerContent::Image { source } => {
                used_sources.insert(*source);
                image_sources.insert(*source);
            }
            LayerContent::Text(_) | LayerContent::Generator | LayerContent::Adjustment => {}
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
                    colour: project.source_colour(source).unwrap_or_default(),
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
