// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The lead-up on a seek, as the engine's thread makes it (EN-14, option 1; `docs/engine-phase-6-6-design.md`, section 4).
//!
//! After a jump the state that an effect that depends on earlier frames goes on from is not the state of the frame before. A lead-up is **the frames before the frame that is shown, made for
//! what they do to those states** and for nothing else: a thread of the program describes them one after another (it has the pictures of the videos in order, which only it can have),
//! and gives them to the engine through a channel; the engine makes each as a frame of the preview (so the rules of the checkpoints are the preview's, and the states are the ones that the next
//! frame of the preview goes on from) and takes no pixels of it. When the lead-up ends the engine says so, and the window asks for the frame that is shown again.
//!
//! **The preview comes first.** While a lead-up is going on the thread does not wait for the next of it: it does what the window has asked of it, and only when nothing has been asked
//! does it take the next thing of the lead-up (waiting a moment for it if it is not there yet, which is the thread that describes the frames being at work). A lead-up that is let go
//! of (cancelled, or replaced by another) takes the pictures that it sent with it; the pictures have ids of their own ([`PictureId::in_the_lead_up`]), so none that the preview sent is let go of.
//!
//! **A chase** is a lead-up for a seek while the project plays (`EngineRequest::StartTheChase`). The preview goes on making frames, so the states that its frames go on from cannot be the ones that
//! the lead-up makes: the chase's frames go through a store of states **of its own**, one frame at a time, and after each the engine looks whether its states are of the same frame as the ones that
//! the last frame of the preview used. When they are, they go over to the preview's (exact), and that is the end of it: the chase has caught up with the picture. The frames of a chase go on to
//! the end of the project, and the chase ends by catching up, or fails.

use std::collections::HashSet;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

use super::{Next, Worker};
use crate::engine::effect_states::CatchingUp;
use crate::engine::frame_renderer::RenderProblem;
use crate::engine::messages::{EngineEvent, LeadUpItem};
use crate::engine::{FrameDescription, PictureData, PictureId};

/// How long the thread waits for the next thing of a lead-up before it looks at what the window has asked of it.
const HOW_LONG_TO_WAIT_FOR_THE_NEXT_THING_OF_THE_LEAD_UP: Duration = Duration::from_millis(2);

/// The lead-up that is going on.
pub(super) struct LeadUp {
    generation: u64,
    items: Receiver<LeadUpItem>,
    /// The pictures that the lead-up sent and has not let go of.
    pictures: HashSet<PictureId>,
    /// How many frames of it have been made.
    frames_made: u64,
    /// Whether it is a chase: its frames go through states of their own, and it ends when they have caught up with the preview's.
    is_a_chase: bool,
}

impl Worker<'_> {
    /// Whether a lead-up is going on.
    pub(super) fn has_a_lead_up(&self) -> bool {
        self.lead_up.is_some()
    }

    /// Begins the lead-up `generation`, which brings what it is made of through `items`. One that is going on is let go of first.
    pub(super) fn start_the_lead_up(&mut self, generation: u64, items: Receiver<LeadUpItem>) {
        self.begin_a_lead_up(generation, items, false);
    }

    /// Begins the lead-up `generation` as a chase (see the module's documentation). One that is going on is let go of first.
    pub(super) fn start_the_chase(&mut self, generation: u64, items: Receiver<LeadUpItem>) {
        self.begin_a_lead_up(generation, items, true);
    }

    fn begin_a_lead_up(&mut self, generation: u64, items: Receiver<LeadUpItem>, is_a_chase: bool) {
        self.let_go_of_the_lead_up();
        self.lead_up = Some(LeadUp {
            generation,
            items,
            pictures: HashSet::new(),
            frames_made: 0,
            is_a_chase,
        });
    }

    /// Lets go of the lead-up `generation`, if that is the one that is going on, and of the pictures that it sent. (Nothing is said of it: the window that cancelled it knows.)
    pub(super) fn cancel_the_lead_up(&mut self, generation: u64) {
        if self
            .lead_up
            .as_ref()
            .is_some_and(|lead_up| lead_up.generation == generation)
        {
            self.let_go_of_the_lead_up();
        }
    }

    /// Lets go of the lead-up that is going on, if one is: the thread that makes it finds that nobody is listening, and the pictures that it sent are let go of.
    pub(super) fn let_go_of_the_lead_up(&mut self) {
        if let Some(lead_up) = self.lead_up.take() {
            self.let_go_of_what_it_has(lead_up.pictures);
        }
    }

    /// Lets go of what a lead-up that has ended has: the pictures that it sent, and the states that a chase made and did not give to the preview.
    fn let_go_of_what_it_has(&mut self, pictures: HashSet<PictureId>) {
        self.renderer.forget_pictures(pictures);
        self.renderer.let_go_of_what_a_chase_made();
    }

    /// Does the next thing of the lead-up (a picture, a frame, the end), waiting a moment for it if it is not there yet.
    pub(super) fn make_a_step_of_the_lead_up(&mut self) -> Next {
        let Some(lead_up) = &self.lead_up else {
            return Next::GoOn;
        };
        match lead_up
            .items
            .recv_timeout(HOW_LONG_TO_WAIT_FOR_THE_NEXT_THING_OF_THE_LEAD_UP)
        {
            Ok(LeadUpItem::Picture { id, data }) => self.take_a_picture_of_the_lead_up(id, &data),
            Ok(LeadUpItem::Forget(ids)) => {
                if let Some(lead_up) = &mut self.lead_up {
                    for id in &ids {
                        lead_up.pictures.remove(id);
                    }
                }
                self.renderer.forget_pictures(ids);
                Next::GoOn
            }
            Ok(LeadUpItem::Frame(description)) => self.make_a_frame_of_the_lead_up(&description),
            Ok(LeadUpItem::End) => self.the_frames_of_the_lead_up_are_over(),
            Ok(LeadUpItem::Failed(problem)) => self.the_lead_up_stops(&problem),
            Err(RecvTimeoutError::Timeout) => Next::GoOn,
            Err(RecvTimeoutError::Disconnected) => {
                self.the_lead_up_stops("the thread that describes its frames ended before it was done")
            }
        }
    }

    fn take_a_picture_of_the_lead_up(&mut self, id: PictureId, data: &PictureData) -> Next {
        match self.attempt_and_again_on_a_new_device_if_it_was_lost(|renderer| renderer.upload_picture(id, data)) {
            Err(stop) => Next::Stop(stop),
            Ok(Ok(())) => {
                if let Some(lead_up) = &mut self.lead_up {
                    lead_up.pictures.insert(id);
                }
                Next::GoOn
            }
            Ok(Err(problem)) => self.the_lead_up_stops(&format!("a picture of it could not be used: {problem}")),
        }
    }

    /// Makes the frame that `description` describes, for the states of its effects.
    fn make_a_frame_of_the_lead_up(&mut self, description: &FrameDescription) -> Next {
        let is_a_chase = self.lead_up.as_ref().is_some_and(|lead_up| lead_up.is_a_chase);
        let made = if is_a_chase {
            self.renderer
                .render_for_a_chase(description, self.settings.frame_timeout)
        } else {
            self.renderer.render(description, self.settings.frame_timeout)
        };
        match made {
            Ok(_) => {
                if let Some(lead_up) = &mut self.lead_up {
                    lead_up.frames_made += 1;
                }
                if is_a_chase {
                    return self.look_whether_the_chase_has_caught_up();
                }
                Next::GoOn
            }
            Err(RenderProblem::PicturesMissing(ids)) => self.the_lead_up_stops(&format!(
                "{} of the pictures that a frame of it is made of were not there",
                ids.len()
            )),
            Err(problem) => {
                // (What the frame had in it that the engine did not write, written down before the renderer may be thrown away: a frame of a lead-up that did not finish is a frame that
                // did not finish, whatever it was for.)
                let effects_of_strangers = self.renderer.shader_effects_in(description);
                let stopped = self.the_lead_up_stops(&format!("a frame of it could not be made: {problem}"));
                if !matches!(stopped, Next::GoOn) {
                    return stopped;
                }
                self.after_a_failed_frame(0, &problem, effects_of_strangers, Vec::new())
            }
        }
    }

    /// After a frame of a chase: if the states that it made have caught up with the preview's, they go over to it, and the chase is done; if they never will, the chase fails.
    fn look_whether_the_chase_has_caught_up(&mut self) -> Next {
        match self.renderer.take_over_what_the_chase_has_caught_up_with() {
            CatchingUp::NotYet => Next::GoOn,
            CatchingUp::TakenOver(_) => self.the_lead_up_is_whole(),
            CatchingUp::NeverWill(why) => {
                self.the_lead_up_stops(&format!("it cannot catch up with the picture: {why}"))
            }
        }
    }

    /// The thread that describes the frames says there are no more. A lead-up is whole then; a chase is not (it ends by catching up with the picture, and it did not).
    fn the_frames_of_the_lead_up_are_over(&mut self) -> Next {
        if self.lead_up.as_ref().is_some_and(|lead_up| lead_up.is_a_chase) {
            return self.the_lead_up_stops("its frames were over before it had caught up with the picture");
        }
        self.the_lead_up_is_whole()
    }

    /// The lead-up has had all its frames: says so, and lets go of what it has (the pictures that it sent).
    fn the_lead_up_is_whole(&mut self) -> Next {
        let Some(lead_up) = self.lead_up.take() else {
            return Next::GoOn;
        };
        self.let_go_of_what_it_has(lead_up.pictures);
        self.say(EngineEvent::LeadUpDone {
            generation: lead_up.generation,
            frames_made: lead_up.frames_made,
        })
    }

    /// The lead-up stops before its end, for this reason: says so, and lets go of the pictures that it sent. (The states are as the frames so far made them.)
    pub(super) fn the_lead_up_stops(&mut self, problem: &str) -> Next {
        let Some(lead_up) = self.lead_up.take() else {
            return Next::GoOn;
        };
        self.let_go_of_what_it_has(lead_up.pictures);
        self.say(EngineEvent::LeadUpFailed {
            generation: lead_up.generation,
            problem: problem.to_owned(),
        })
    }
}

#[cfg(test)]
mod chase_tests;
#[cfg(test)]
mod tests;
