// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the picture engine is asked to draw for each frame of an export.
//!
//! A frame of the export is described by **the very function that describes a frame of the preview** ([`describe_frame`], of `frame_scene`), from a
//! [`PreviewContent`] built the way the window builds one: the layers of the section the frame is in, each where its keyframes say it is *at that frame*, with the
//! colours its effects make *at that frame*, and with the picture it has *at that frame*. So the export draws what the preview shows, by construction (EN-03), and
//! an animated value is exact at every frame of the finished video.
//!
//! The pictures a frame is made of come from three places: the next picture of each run of video ([`PicturesOfTheRuns`]), the picture files (decoded once, before
//! the first frame), and the text (drawn when a frame first needs it). [`FrameDescriber::describe`] sends the engine the pictures it does not have yet, and says which
//! of the pictures it has the frame does not use: **the engine holds the pictures of the frame it is asked for, and not more**, so a long video does not pile up
//! pictures on the graphics card.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;

use thiserror::Error;

use super::pictures::{PicturesOfTheRuns, RunPictureProblem, RunStreamStarter};
use super::plan::ExportPlan;
use crate::engine::{FrameColour, FrameDescription, PictureData, PictureId};
use crate::frame_scene::{
    PreviewContent, SentPictures, TextPictures, VideoInThePicture, colour_of_a_picture_file, describe_frame,
    groups_of_a_section, layers_of_a_section, symbol_of_a_section,
};
use crate::project::{SourceMediaIdentifier, TimelineFrameIndex};
use crate::rgba_picture::RgbaPicture;

/// The number that tells the ids of the pictures that are made from files from those of an earlier project (the window changes it when it lets go of what it knows
/// of its files; an export is of one project and never does).
const GENERATION_OF_THE_PICTURES_OF_FILES: u64 = 0;

/// What a layer is that has no picture: nothing. (None does: a video always has the picture of its frame, a picture file is decoded before the first frame, and a text
/// that cannot be drawn is left out.)
const COLOUR_OF_A_LAYER_WITHOUT_A_PICTURE: [u8; 4] = [0, 0, 0, 0];

#[derive(Debug, Error)]
pub enum FrameProblem {
    #[error(transparent)]
    Picture(#[from] RunPictureProblem),
    #[error("frame {0} is not a frame of the project")]
    NotOnTheTimeline(u64),
    /// The plan has a video in a section and no run of video for it at the frame: a mistake of the program, not of the project.
    #[error("the plan has no run of video for a video that is seen at frame {0}")]
    NoRunForAVideo(u64),
}

/// How a describer makes its frames, besides what the plan says of the project: the sizes that the picture is placed and the frame is made at, and the colour that the frame is put
/// together in. **The export's frames are made as the plan says** ([`Self::of_the_export`]); a lead-up's are made as the preview's are (EN-14: a state is of the size and the colour of the
/// frames that made it, so the frames that make it for the preview have to be those).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HowTheFramesAreDescribed {
    /// The size of the picture that the layers are placed on and that the text is drawn for, in pixels.
    pub picture_size: (u32, u32),
    /// How many pixels across and down the frames are made at.
    pub render_size: (u32, u32),
    /// What the frames are put together in and encoded as.
    pub colour: FrameColour,
}

impl HowTheFramesAreDescribed {
    /// The way the export describes its frames: at the size of its picture, in the colour of its plan.
    pub fn of_the_export(plan: &ExportPlan) -> Self {
        Self {
            picture_size: plan.picture_size(),
            render_size: plan.picture_size(),
            colour: plan.colour,
        }
    }
}

/// A frame to ask the engine to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct DescribedFrame {
    pub description: FrameDescription,
    /// The pictures that the engine was sent for earlier frames and that this one does not use: let go of them before this frame is asked for. (If a later frame
    /// uses one of them again it is sent again; the engine is asked for one frame at a time and in order, so none of the frames before this one is waiting for them.)
    pub forget: Vec<PictureId>,
}

/// Describes the frames of an export, one after another. See the module documentation.
pub struct FrameDescriber<'plan, Starter: RunStreamStarter> {
    plan: &'plan ExportPlan,
    pictures: PicturesOfTheRuns<Starter>,
    /// The picture files, decoded as big as the biggest place any layer of them has.
    images: HashMap<SourceMediaIdentifier, RgbaPicture>,
    text: Box<dyn TextPictures + 'plan>,
    /// The pictures that the engine has been sent and has not been told to let go of.
    sent: HashSet<PictureId>,
    how: HowTheFramesAreDescribed,
}

impl<'plan, Starter: RunStreamStarter> FrameDescriber<'plan, Starter> {
    pub fn new(
        plan: &'plan ExportPlan,
        pictures: PicturesOfTheRuns<Starter>,
        images: HashMap<SourceMediaIdentifier, RgbaPicture>,
        text: Box<dyn TextPictures + 'plan>,
    ) -> Self {
        Self {
            plan,
            pictures,
            images,
            text,
            sent: HashSet::new(),
            how: HowTheFramesAreDescribed::of_the_export(plan),
        }
    }

    /// The same describer, making its frames as `how` says and not as the plan does (a lead-up's frames are made as the preview's are).
    #[must_use]
    pub fn described_as(mut self, how: HowTheFramesAreDescribed) -> Self {
        self.how = how;
        self
    }

    /// How many pictures of video were made by repeating the last because a file had fewer than the project was told.
    pub fn pictures_held(&self) -> u64 {
        self.pictures.pictures_held()
    }

    /// Says that the engine has none of the pictures it was sent (it was given a new graphics device): the next frame sends the ones it uses again.
    pub fn forget_what_was_sent(&mut self) {
        self.sent.clear();
    }

    /// The frame `timeline_frame` of the export. **The frames are described in order**, a frame after a frame (the pictures of the runs of video come in order):
    /// a frame may be described again, but not one that is before the one described last. Pictures that the engine does not have are sent with `send` before this
    /// returns. Waits for the picture of a video if ffmpeg has not decoded it yet, looking at `cancelled`.
    pub fn describe(
        &mut self,
        timeline_frame: u64,
        cancelled: &AtomicBool,
        send: &mut dyn FnMut(PictureId, PictureData),
    ) -> Result<DescribedFrame, FrameProblem> {
        let plan = self.plan;
        let frame = TimelineFrameIndex::new(timeline_frame);
        let section = plan
            .pictures
            .section_at(frame)
            .ok_or(FrameProblem::NotOnTheTimeline(timeline_frame))?;
        self.pictures.start_the_runs_that_begin_soon(timeline_frame)?;

        // The picture of each video of the frame, taken before anything is built from them: they are what the content borrows.
        let mut pictures_of_the_videos = HashMap::new();
        for (layer, _) in section.video_layers() {
            let run = plan
                .run_of(layer.clip, timeline_frame)
                .ok_or(FrameProblem::NoRunForAVideo(timeline_frame))?;
            pictures_of_the_videos.insert(layer.clip, self.pictures.picture(run, timeline_frame, cancelled)?);
        }

        // (The layers and the symbol are made from the section by the very functions that the window makes them with.)
        let content = PreviewContent {
            picture_size: Some(self.how.picture_size),
            layers: layers_of_a_section(
                section,
                frame,
                |layer, _| {
                    VideoInThePicture::Picture(
                        pictures_of_the_videos
                            .get(&layer.clip)
                            .expect("a picture was taken for every video"),
                    )
                },
                |source| {
                    (
                        self.images.get(&source),
                        colour_of_a_picture_file(
                            &plan.images.get(&source).map(|image| image.colour).unwrap_or_default(),
                        ),
                    )
                },
            ),
            groups: groups_of_a_section(section, frame),
            symbol: symbol_of_a_section(section),
            indicator_layout: plan.symbol_layout,
            selected_clip: None,
        };

        let mut supply = SentPictures {
            sent: &mut self.sent,
            text_renderer: &mut *self.text,
            send,
        };
        let description = describe_frame(
            &content,
            self.how.render_size,
            GENERATION_OF_THE_PICTURES_OF_FILES,
            COLOUR_OF_A_LAYER_WITHOUT_A_PICTURE,
            self.how.colour,
            &mut supply,
        );

        let used: HashSet<PictureId> = description.pictures_used().into_iter().collect();
        let mut forget: Vec<PictureId> = self.sent.difference(&used).copied().collect();
        forget.sort_unstable();
        self.sent.retain(|id| used.contains(id));
        Ok(DescribedFrame { description, forget })
    }
}

#[cfg(test)]
mod tests;
