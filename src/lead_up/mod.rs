// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The lead-up on a seek (EN-14, option 1; the design is `docs/engine-phase-6-6-design.md`, section 4): **the frames before the frame that the preview shows, described one after another so
//! that the engine can make them for what they do to the states of the effects that depend on earlier frames.**
//!
//! After a jump the state that such an effect goes on from is not the state of the frame before. The frames before the frame that is shown are the pictures it should have gone through, and they
//! are the project's, so they are described by the very describer that describes the frames of an export ([`FrameDescriber`], frames in order, one decode process for each stretch of video), made
//! to describe them as the preview does: at the size the preview makes its frames at, in the colour it makes them in, with videos delivered as big as the preview delivers them and by its way of
//! scaling, and with the picture files that the window has decoded already. That is what makes a state that the lead-up makes the state that the preview goes on from: **a state is of the
//! size of the picture that made it** (an effect on a layer is run on the picture of the layer), so the pictures of the lead-up have the size of the preview's.
//!
//! The decoding is slow and waits for ffmpeg, so it is done by a thread of its own ([`start_a_lead_up`]), which gives the engine what it describes through a channel that holds only a few things
//! (the thread waits when the engine is behind, so nothing piles up). **The pictures it sends have ids of their own** ([`PictureId::in_the_lead_up`]): none is ever one that the preview sent, so the
//! lead-up can send and let go of its pictures without touching the preview's.

use std::collections::HashMap;
use std::io;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::engine::{FRAMES_AFTER_A_STATE_IS_NOT_EXACT, LeadUpItem, PictureData, PictureId};
use crate::frame_scene::TextPictures;
use crate::media::{
    FfmpegToolchain, FrameStream, FrameStreamError, FrameStreamRequest, MediaLocation, NativeYuv420p,
    QUICK_SCALING_OF_THE_PREVIEW,
};
use crate::playback::VideoReading;
use crate::project::SourceMediaIdentifier;
use crate::rgba_picture::RgbaPicture;
use crate::video_export::{
    ExportPlan, FrameDescriber, FrameProblem, HowTheFramesAreDescribed, PicturesOfTheRuns, RunPictureProblem,
    RunStreamStarter,
};

/// The most frames that a lead-up has: **the number of frames after which a state that was made from nothing is as good as one that was made from all the frames before** (the engine goes on saying
/// that a picture is not exact for this many frames after a state that was not exact).
pub const MOST_FRAMES_OF_A_LEAD_UP: u64 = FRAMES_AFTER_A_STATE_IS_NOT_EXACT;

/// How many things the thread that describes the frames may have given the engine and not had taken, before it waits.
const MOST_ITEMS_AHEAD: usize = 8;

/// How long the thread waits to give the engine something when the channel is full, before it looks at whether it was cancelled.
const HOW_LONG_TO_WAIT_FOR_THE_ENGINE: Duration = Duration::from_millis(1);

/// What starts the decode process of a run of video for a lead-up: **as the preview starts one for a video** (the pictures delivered as big as the run says, which the plan of a lead-up has as the preview
/// delivers them, and scaled as quickly as the preview scales them), so that the pictures that the lead-up makes the states from are the preview's. It starts none if ffmpeg was not found (a project
/// that has no video does not need it).
#[derive(Clone)]
pub struct PreviewRunStreamStarter {
    toolchain: Option<Arc<FfmpegToolchain>>,
}

impl PreviewRunStreamStarter {
    pub fn new(toolchain: Option<Arc<FfmpegToolchain>>) -> Self {
        Self { toolchain }
    }
}

impl RunStreamStarter for PreviewRunStreamStarter {
    type Stream = FrameStream;

    fn start(&self, request: &FrameStreamRequest, delivered_size: (u32, u32)) -> Result<FrameStream, FrameStreamError> {
        let Some(toolchain) = &self.toolchain else {
            return Err(FrameStreamError::CouldNotStart(io::Error::other(
                "ffmpeg was not found",
            )));
        };
        FrameStream::start_delivering(
            toolchain,
            request,
            delivered_size,
            QUICK_SCALING_OF_THE_PREVIEW,
            Arc::new(|| {}),
        )
    }
}

/// The frames that a lead-up to the frame `frame` has, when the effects of that frame that have a state have gone on for at most `most_frames_a_state_has_gone_on_for` frames of their own clocks
/// (see `FrameDescription::most_frames_a_state_has_gone_on_for`): as many before it as that, but no more than [`MOST_FRAMES_OF_A_LEAD_UP`] and none before the first of the project. A state that has gone
/// on for few frames is made from all of them; one that has gone on for more is made from the last [`MOST_FRAMES_OF_A_LEAD_UP`].
pub fn frames_of_the_lead_up_to(frame: u64, most_frames_a_state_has_gone_on_for: u64) -> Range<u64> {
    let length = most_frames_a_state_has_gone_on_for.min(MOST_FRAMES_OF_A_LEAD_UP);
    frame.saturating_sub(length)..frame
}

/// Everything the thread that describes the frames of a lead-up needs, which it is given because it has no window to ask.
pub struct WhatALeadUpIsMadeFrom<Starter> {
    /// The plan of the project as the window shows it (see `plan_export_of_the_files_that_can_be_read`), which the thread cuts to its frames.
    pub plan: ExportPlan,
    /// The frames of the timeline that are described, the first first.
    pub frames: Range<u64>,
    /// The size of the picture that the layers are placed on, the size of the frames, and their colour: those of the frames that the preview makes (see [`HowTheFramesAreDescribed`]).
    pub how: HowTheFramesAreDescribed,
    /// The size of the pictures that the preview decodes a video or a picture file for (`playback::preview_picture_size`), which a video of the lead-up is delivered inside as well.
    pub preview_picture_size: (u32, u32),
    /// Where the files of the videos are.
    pub locations: HashMap<SourceMediaIdentifier, MediaLocation>,
    /// For each video that can be given to the engine in planes, what the planes stand for.
    pub planes: HashMap<SourceMediaIdentifier, NativeYuv420p>,
    /// How the colours of each video are read (as the preview reads them).
    pub readings: HashMap<SourceMediaIdentifier, VideoReading>,
    /// The picture files, as the window has decoded them.
    pub images: HashMap<SourceMediaIdentifier, RgbaPicture>,
    /// What starts a decode process for a run of a video.
    pub starter: Starter,
    /// Makes what draws the text. (It is made on the thread that describes the frames: the fonts of the computer take a moment to be found, and are found by the first text that is seen.)
    pub text: Box<dyn FnOnce() -> Box<dyn TextPictures> + Send>,
}

/// The thread that describes the frames of a lead-up, while it goes on. Dropping it cancels the lead-up: the thread notices within a moment and ends, letting go of what it has (the decode processes
/// among it); it is not waited for.
pub struct RunningLeadUp {
    cancelled: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl RunningLeadUp {
    /// Tells the thread to stop. It does within a moment, and says nothing of it.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Whether the thread has ended.
    pub fn is_over(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Waits for the thread to end (for the tests: the window never waits for it).
    #[cfg(test)]
    pub fn wait_until_it_is_over(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for RunningLeadUp {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// A lead-up that has been started: what the engine is to be given to make it from, and the thread that makes that.
pub struct StartedLeadUp {
    pub items: Receiver<LeadUpItem>,
    pub running: RunningLeadUp,
}

/// Starts the thread that describes the frames of the lead-up `made_from` says, and gives the end of the channel that the engine takes them from (`Engine::start_the_lead_up`).
pub fn start_a_lead_up<Starter>(made_from: WhatALeadUpIsMadeFrom<Starter>) -> StartedLeadUp
where
    Starter: RunStreamStarter + Send + 'static,
{
    let (sender, items) = mpsc::sync_channel(MOST_ITEMS_AHEAD);
    let cancelled = Arc::new(AtomicBool::new(false));
    let for_the_thread = Arc::clone(&cancelled);
    let thread = thread::Builder::new()
        .name("lead-up".to_owned())
        .spawn(move || run(made_from, &sender, &for_the_thread))
        .ok();
    StartedLeadUp {
        items,
        running: RunningLeadUp { cancelled, thread },
    }
}

/// Why the thread stopped before the end.
enum Ending {
    /// It was cancelled, or the engine let go of the lead-up: nothing is said.
    Stopped,
    Failed(String),
}

impl From<RunPictureProblem> for Ending {
    fn from(problem: RunPictureProblem) -> Self {
        match problem {
            RunPictureProblem::Cancelled => Self::Stopped,
            other => Self::Failed(other.to_string()),
        }
    }
}

impl From<FrameProblem> for Ending {
    fn from(problem: FrameProblem) -> Self {
        match problem {
            FrameProblem::Picture(picture) => picture.into(),
            other => Self::Failed(other.to_string()),
        }
    }
}

/// The body of the thread: describes the frames and says how it ended. (A mistake of the program, a panic, ends it with a word like any other ending: the engine waits for an end.)
fn run<Starter: RunStreamStarter>(
    made_from: WhatALeadUpIsMadeFrom<Starter>,
    sender: &SyncSender<LeadUpItem>,
    cancelled: &AtomicBool,
) {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        describe_the_frames(made_from, sender, cancelled)
    }))
    .unwrap_or_else(|panic| {
        let words = panic
            .downcast_ref::<&str>()
            .map(|words| (*words).to_owned())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "it did not say what".to_owned());
        Err(Ending::Failed(format!(
            "the lead-up stopped because of a mistake in the program: {words}"
        )))
    });
    let last_thing_said = match outcome {
        Ok(()) => LeadUpItem::End,
        Err(Ending::Failed(problem)) => LeadUpItem::Failed(problem),
        Err(Ending::Stopped) => return,
    };
    let _ = give(sender, last_thing_said, cancelled);
}

/// Gives `item` to the engine, waiting for room for it, unless the lead-up is cancelled or the engine has let go of it (the end of the channel that it took from is gone).
fn give(sender: &SyncSender<LeadUpItem>, item: LeadUpItem, cancelled: &AtomicBool) -> Result<(), Ending> {
    let mut item = item;
    loop {
        match sender.try_send(item) {
            Ok(()) => return Ok(()),
            Err(TrySendError::Disconnected(_)) => return Err(Ending::Stopped),
            Err(TrySendError::Full(not_taken)) => {
                if cancelled.load(Ordering::SeqCst) {
                    return Err(Ending::Stopped);
                }
                item = not_taken;
                thread::sleep(HOW_LONG_TO_WAIT_FOR_THE_ENGINE);
            }
        }
    }
}

/// Describes the frames, one after another, and gives the engine each with the pictures it needs first and the pictures it lets go of after.
fn describe_the_frames<Starter: RunStreamStarter>(
    made_from: WhatALeadUpIsMadeFrom<Starter>,
    sender: &SyncSender<LeadUpItem>,
    cancelled: &AtomicBool,
) -> Result<(), Ending> {
    let WhatALeadUpIsMadeFrom {
        plan,
        frames,
        how,
        preview_picture_size,
        locations,
        planes,
        readings,
        images,
        starter,
        text,
    } = made_from;
    let plan = plan.for_a_lead_up(frames.clone(), preview_picture_size);
    let pictures = PicturesOfTheRuns::new(starter, &plan, &locations, &planes)?.with_readings(&readings);
    let mut describer = FrameDescriber::new(&plan, pictures, images, text()).described_as(how);
    for frame in frames {
        if cancelled.load(Ordering::SeqCst) {
            return Err(Ending::Stopped);
        }
        let mut pictures_to_send: Vec<(PictureId, PictureData)> = Vec::new();
        let described = describer.describe(frame, cancelled, &mut |id, data| pictures_to_send.push((id, data)))?;
        for (id, data) in pictures_to_send {
            give(
                sender,
                LeadUpItem::Picture {
                    id: id.in_the_lead_up(),
                    data: Arc::new(data),
                },
                cancelled,
            )?;
        }
        if !described.forget.is_empty() {
            let forget = described.forget.into_iter().map(PictureId::in_the_lead_up).collect();
            give(sender, LeadUpItem::Forget(forget), cancelled)?;
        }
        let description = described
            .description
            .with_the_pictures_numbered_by(PictureId::in_the_lead_up);
        give(sender, LeadUpItem::Frame(description), cancelled)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
