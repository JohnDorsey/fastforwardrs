// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The stretches of video of an export, each of which is one decode process.
//!
//! The plan of the picture cuts the timeline into sections wherever the layers change, so a video that has a caption over it for a moment, or a picture-in-picture
//! clip that starts in the middle of it, is in more than one section. Decoding it afresh at each boundary would start a process of ffmpeg, and seek, a hundred times
//! for a video with a hundred captions: a **run** is the stretch of a clip's video that goes on from section to section (the pictures right after the pictures of
//! the section before), and one decode process reads all of it.

use std::collections::HashMap;

use crate::composition::{Layer, LayerContent, PicturePlan, PictureSection, VisibleVideo};
use crate::project::{ClipId, SourceMediaIdentifier, SpeedFactor, TimelineFrameIndex};

/// One clip's video over a stretch of the timeline that is made of whole sections, read by one decode process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoRun {
    pub clip: ClipId,
    pub source: SourceMediaIdentifier,
    pub speed: SpeedFactor,
    /// The source frame that the first picture of the run is of.
    pub first_source_frame: u64,
    /// How many source frames are read, starting at `first_source_frame`; with a speed of `N` only every `N`th of them is a picture of the run.
    pub source_frame_count: u64,
    /// The first frame of the timeline that the run is seen at.
    pub first_timeline_frame: u64,
    /// The frame just after the last frame of the timeline that the run is seen at.
    pub end_timeline_frame: u64,
    /// The size every picture of the run is delivered at, in pixels: **as big as the layer needs it, made by ffmpeg** (with its own scaling, which is better than
    /// the engine's at making a picture much smaller, and is what the export always did): the box that the layer fills on the finished picture, or the largest of
    /// its boxes if keyframes change its size. A video that fills the picture is delivered as big as the picture, so the engine puts it on the frame 1 to 1.
    pub delivered_size_in_pixels: (u32, u32),
}

impl VideoRun {
    /// How many pictures the run has: one for each frame of the timeline that it is seen at.
    pub fn picture_count(&self) -> u64 {
        self.end_timeline_frame - self.first_timeline_frame
    }

    /// The source frame that the picture `number` of the run (counted from 0) is of.
    pub fn source_frame_of_picture(&self, number: u64) -> u64 {
        self.first_source_frame + number * u64::from(self.speed.value())
    }
}

/// A run while it is being found: the layer that is the clip's video in its first section, and where its source frames end so far.
struct RunSoFar<'a> {
    run: VideoRun,
    layer: &'a Layer,
    end_source_frame: u64,
}

/// The runs of the video of `pictures`, in the order they begin. `picture_size` is the size of the finished picture in pixels.
pub fn find_the_runs(pictures: &PicturePlan, picture_size: (u32, u32)) -> Vec<VideoRun> {
    let mut found: Vec<RunSoFar<'_>> = Vec::new();
    // The latest run of each clip, which the clip's video in a later section may go on.
    let mut latest_of_each_clip: HashMap<ClipId, usize> = HashMap::new();
    for section in &pictures.sections {
        // (The videos of the masks too, with their source frames counted from the first frame of this section, as those of its own layers are: a mask is one stretch of its track's
        // picture that the section may be a piece of.)
        for (layer, video) in videos_of_a_section_from_its_first_frame(section) {
            let video = &video;
            let goes_on = latest_of_each_clip.get(&layer.clip).copied().filter(|&index| {
                let so_far = &found[index];
                // (It was seen in the section before this one: the run ends where this section begins. A clip that was hidden for a while is not read on from where it
                // was: the process that reads it has no pictures to give for the frames in between.)
                so_far.run.end_timeline_frame == section.first_timeline_frame
                    && so_far.run.source == video.source
                    && so_far.run.speed == video.speed
                    && video.first_source_frame == so_far.end_source_frame_of_whole_pictures()
            });
            let index = match goes_on {
                Some(index) => {
                    let so_far = &mut found[index];
                    so_far.run.end_timeline_frame = section.end_timeline_frame;
                    so_far.end_source_frame = video.end_source_frame;
                    index
                }
                None => {
                    found.push(RunSoFar {
                        run: VideoRun {
                            clip: layer.clip,
                            source: video.source,
                            speed: video.speed,
                            first_source_frame: video.first_source_frame,
                            source_frame_count: 0,
                            first_timeline_frame: section.first_timeline_frame,
                            end_timeline_frame: section.end_timeline_frame,
                            delivered_size_in_pixels: (1, 1),
                        },
                        layer,
                        end_source_frame: video.end_source_frame,
                    });
                    found.len() - 1
                }
            };
            latest_of_each_clip.insert(layer.clip, index);
        }
    }
    found
        .into_iter()
        .map(|so_far| {
            let first = so_far.run.first_timeline_frame;
            let end = so_far.run.end_timeline_frame;
            VideoRun {
                source_frame_count: so_far.end_source_frame - so_far.run.first_source_frame,
                delivered_size_in_pixels: largest_box_over(so_far.layer, first..end, picture_size),
                ..so_far.run
            }
        })
        .collect()
}

/// The videos of the layers of `section` and of its masks, each with its layer, with the source frames of each counted from the first frame of `section`, which a layer of a mask has
/// counted from the first frame of the stretch of its own track that it is in (a clip that is in the picture and in a mask is one video: it comes once).
fn videos_of_a_section_from_its_first_frame(section: &PictureSection) -> Vec<(&Layer, VisibleVideo)> {
    let mut found: Vec<(&Layer, VisibleVideo)> = section.video_layers().map(|(layer, video)| (layer, *video)).collect();
    for (in_section, layer, video) in section.video_layers_of_the_masks() {
        if found.iter().any(|(already, _)| already.clip == layer.clip) {
            continue;
        }
        let behind = section.first_timeline_frame - in_section.first_timeline_frame;
        let speed = u64::from(video.speed.value());
        let first_source_frame = video.first_source_frame + behind * speed;
        found.push((
            layer,
            VisibleVideo {
                first_source_frame,
                end_source_frame: (first_source_frame + section.timeline_frame_count() * speed)
                    .min(video.end_source_frame),
                ..*video
            },
        ));
    }
    found
}

impl RunSoFar<'_> {
    /// The source frame that the picture after the last one of the run so far is of: where a section that goes on with the run begins.
    fn end_source_frame_of_whole_pictures(&self) -> u64 {
        self.run.first_source_frame
            + (self.run.end_timeline_frame - self.run.first_timeline_frame) * u64::from(self.run.speed.value())
    }
}

/// The largest box, in pixels (width, height), that `layer` fills on a finished picture of `picture_size` pixels at the frames `frames`: the box that the layer has, or
/// if keyframes move or resize it the largest of the boxes it has at those frames.
fn largest_box_over(layer: &Layer, frames: std::ops::Range<u64>, picture_size: (u32, u32)) -> (u32, u32) {
    let box_at = |layer: &Layer, frame: Option<u64>| {
        let placement = match frame {
            Some(frame) => layer.placement_at(TimelineFrameIndex::new(frame)),
            None => layer.placement,
        };
        let pixels = placement.area.in_pixels(picture_size);
        (pixels.width, pixels.height)
    };
    if !layer.is_moved_by_keyframes() {
        return box_at(layer, None);
    }
    frames
        .map(|frame| box_at(layer, Some(frame)))
        .fold((1, 1), |(widest, tallest), (width, height)| {
            (widest.max(width), tallest.max(height))
        })
}

/// The largest box, in pixels (width, height), that any layer of the picture file `source` fills on a finished picture of `picture_size` pixels.
pub(super) fn largest_box_of_an_image(
    pictures: &PicturePlan,
    source: SourceMediaIdentifier,
    picture_size: (u32, u32),
) -> (u32, u32) {
    pictures
        .sections
        .iter()
        .flat_map(|section| section.all_layers().into_iter().map(move |layer| (section, layer)))
        .filter(|(_, layer)| matches!(&layer.content, LayerContent::Image { source: found } if *found == source))
        .map(|(section, layer)| {
            largest_box_over(
                layer,
                section.first_timeline_frame..section.end_timeline_frame,
                picture_size,
            )
        })
        .fold((1, 1), |(widest, tallest), (width, height)| {
            (widest.max(width), tallest.max(height))
        })
}

#[cfg(test)]
mod tests;
