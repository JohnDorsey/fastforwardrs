// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The picture engine as the preview uses it: the engine, what it has been sent, and the frame it made last, which is what the preview shows.
//!
//! Each time the window is drawn, [`EnginePreview::update`] takes what the engine has said since (a finished frame becomes the picture that is shown; a
//! picture that the engine does not have is sent again), describes the frame that the content of the preview makes ([`describe_frame_in_colour`]), sends the pictures
//! that the engine has not been sent, and, if the frame is not the one that was asked for last, asks for it. The engine makes only the newest frame it is asked
//! for, so a window that changes faster than the engine makes frames shows the latest frame that is done, a moment behind. Nothing here waits for the engine.
//!
//! **If the engine stops** (it could not make a device, or lost it, or its thread ended) the preview says so ([`EnginePreview::why_it_stopped`]) and the
//! window draws the preview the old way from then on. **If the engine replaces its graphics device** (it was lost, or a frame did not finish in time) it goes on,
//! and the preview keeps what the user is to be told ([`EnginePreview::take_what_the_user_is_to_be_told`]).
//!
//! **The lead-up on a seek** (EN-14, option 1; `docs/engine-phase-6-6-design.md`, section 4): when the checkbox *Generate lead-up on seek* is on and the picture on the screen is not exact (an effect
//! that depends on earlier frames went on from a state that is not the one of the frame before) **and has stayed as it is for a quarter of a second**, the preview says that it wants a lead-up
//! made ([`EnginePreview::the_lead_up_that_is_wanted`]); the window, which has the project, starts it ([`EnginePreview::start_the_lead_up`]); the engine makes the frames before the one on the screen
//! for the states of the effects, and says when it is done, and then the frame on the screen is asked for again and is exact. **Any other frame that is asked for cancels it**, before it is asked for.
//! A picture that keeps changing (a playhead that is dragged, a project that plays) has none made: a lead-up is for the frame that stays.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use egui::{Color32, ColorImage, Context, Painter, Rect, TextureHandle, pos2};

use super::PreviewContent;
use super::engine_view::{PictureSupply, PictureWanted, SentPictures, describe_frame_in_colour};
use crate::application::textures::PictureFilter;
use crate::engine::{
    AdapterChoice, Engine, EngineEvent, EngineSettings, FrameColour, FrameDescription, LeadUpItem, PictureId,
    QualityGovernor, QualityRules, RenderedFrame, RulesOfCheckpoints, WakeUp,
};
use crate::project::FrameRate;
use crate::text_rendering::LazyTextRenderer;

/// What the user is told when the engine replaced its graphics device and went on.
fn the_words_about_a_replaced_device(why: &str, effects_switched_off: &[String]) -> String {
    let mut words = format!(
        "The picture engine's graphics device had to be replaced: {why}.

The engine made itself a new one and goes on; a picture may have been late. Nothing of the project is changed."
    );
    if !effects_switched_off.is_empty() {
        words.push_str(&format!(
            "

The effects that were being drawn when it happened are not drawn again as long as the program runs, and the pictures are made without them: {}.",
            effects_switched_off.join(", ")
        ));
    }
    words
}

/// How many pictures are kept in the engine, at the most, before the ones that the last frames did not use are let go of. (A picture of a video is a new
/// one for every picture of the video that is shown, so without this they would pile up in the engine, and in the list of what it has been sent.)
const MOST_PICTURES_KEPT_WITHOUT_USE: usize = 48;

/// For how many of the last frames asked for a picture counts as used.
const FRAMES_A_PICTURE_STAYS_FOR: u64 = 4;

/// What size the engine is to make the frame at: the size the picture is shown at on the screen, in pixels of the screen, but no more than the picture of the
/// project is (a picture shown bigger than it is has its pixels made bigger by the window, which is how it is zoomed in on), and in the shape of that
/// picture. A picture that is shown smaller is made smaller (the engine has less to do for it, and the window has nothing to shrink).
pub fn render_size_for(
    shown_size_in_points: egui::Vec2,
    pixels_per_point: f32,
    picture_size: (u32, u32),
) -> (u32, u32) {
    let (width, height) = (picture_size.0.max(1) as f32, picture_size.1.max(1) as f32);
    let shown_width_in_pixels = (shown_size_in_points.x * pixels_per_point).max(1.0);
    let scale = (shown_width_in_pixels / width).min(1.0);
    (
        (width * scale).round().max(2.0) as u32,
        (height * scale).round().max(2.0) as u32,
    )
}

/// How many frames that were asked for and have not come are remembered, to tell how long the one that comes took.
const MOST_FRAMES_WAITED_FOR: usize = 64;

/// How the clock of the project goes, as far as the quality of the preview is concerned: while it goes the frames are made smaller if the card cannot keep up, and while
/// it is stopped a picture that is still is made whole (see `engine::quality`).
#[derive(Clone, Copy, Debug)]
pub struct HowTheClockGoes {
    /// Whether the project is playing.
    pub is_playing: bool,
    /// How long a frame of the project lasts.
    pub how_long_a_frame_lasts: Duration,
    /// What time it is.
    pub now: Instant,
}

impl HowTheClockGoes {
    /// How the clock of a project that has `frame_rate` goes at `now`, playing or not.
    pub fn new(is_playing: bool, frame_rate: FrameRate, now: Instant) -> Self {
        Self {
            is_playing,
            // (A frame rate is a number above nothing, so this is the time of a frame; if it were not, no time would be what the quality of the preview leaves alone.)
            how_long_a_frame_lasts: Duration::try_from_secs_f64(1.0 / frame_rate.frames_per_second())
                .unwrap_or(Duration::ZERO),
            now,
        }
    }
}

/// How long a picture has to stay as it is before a lead-up is wanted of it. (A playhead that is dragged or a project that plays changes the picture all the time, and a lead-up is for the frame
/// that stays: it is made while a person looks at it.)
const HOW_LONG_THE_PICTURE_STAYS_STILL_BEFORE_A_LEAD_UP: Duration = Duration::from_millis(250);

/// What the preview asks the window to make a lead-up of (see the module documentation).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LeadUpWanted {
    /// A number that tells this lead-up from every other that the preview has wanted; the engine says it when the lead-up ends.
    pub generation: u64,
    /// The size that the frame on the screen was made at, which the frames of the lead-up are made at too: **a state is of the size of the picture that made it**.
    pub render_size: (u32, u32),
    /// What the frame on the screen was put together in, which the frames of the lead-up are put together in too: a state is in the colour that made it.
    pub colour: FrameColour,
    /// The most frames of their own clocks that the effects of the frame on the screen that have a state have gone on for (see `FrameDescription::most_frames_a_state_has_gone_on_for`): how far
    /// back the frames that its states are made of go. The lead-up has as many frames before the frame on the screen, but no more than `lead_up::MOST_FRAMES_OF_A_LEAD_UP`.
    pub most_frames_a_state_has_gone_on_for: u64,
}

/// Where the preview is with a lead-up.
enum LeadUpOfThePreview {
    /// None is wanted or going on.
    None,
    /// The preview wants this one made for the frame with this fingerprint, and the window has not said whether it can.
    Wanted { wanted: LeadUpWanted, for_the_frame: u64 },
    /// The engine is making this one, from what the thread that `_keeps_alive` is describes. (Dropping it cancels the lead-up: the thread is let go of.)
    Going {
        generation: u64,
        for_the_frame: u64,
        _keeps_alive: Box<dyn Send>,
    },
}

/// What the preview knows of the frame it has just described that has to do with a lead-up.
struct FactsAboutTheFrame {
    fingerprint: u64,
    /// See [`LeadUpWanted::most_frames_a_state_has_gone_on_for`]; `None` if no effect of the frame has a state.
    most_frames_a_state_has_gone_on_for: Option<u64>,
    render_size: (u32, u32),
    colour: FrameColour,
}

/// The frame that is on the preview.
struct ShownFrame {
    texture: TextureHandle,
    /// The frame it is a picture of, which is kept so that it can be put on the graphics card again when it is to be put on the screen another way.
    frame: Arc<RenderedFrame>,
    /// How it was put on the screen.
    filter: PictureFilter,
    /// The serial number of the request it came from.
    serial: u64,
    /// Whether the engine said that the picture is not exact: an effect that depends on earlier frames went on from a state that is not the one of the frame before (EN-14).
    approximate: bool,
}

/// See the module documentation.
pub struct EnginePreview {
    engine: Engine,
    /// The pictures the engine has been sent and has not been told to let go of.
    sent: HashSet<PictureId>,
    /// Pictures that the engine said it could not use, which are not sent again.
    rejected: HashSet<PictureId>,
    /// For each picture sent, the number of the last frame that was asked for that used it.
    last_used_in: HashMap<PictureId, u64>,
    /// A number the window changes when it lets go of what it knows of its files (see [`describe_frame_in_colour`]).
    generation: u64,
    /// The fingerprint of the frame that was asked for last and the serial number of its request, so that the same frame is not asked for again.
    asked_for: Option<(u64, u64)>,
    /// How many frames were asked for.
    frames_asked_for: u64,
    /// How many of the frames that came were in the engine's cache of the frames it has made: asked for again, they came at once.
    frames_from_the_cache: u64,
    /// The serial number of the last request.
    last_serial: u64,
    /// Decides what part of the size of the picture the frames are made at.
    governor: QualityGovernor,
    /// When each of the frames that were asked for was asked for, the oldest first, so that how long the one that comes took can be told.
    asked_at: VecDeque<(u64, Instant)>,
    /// What was shown when the window was drawn last, not counting the size it is made at.
    what_was_shown: Option<u64>,
    /// The first serial number whose frame may be shown: the frames of requests before it are of what the window has let go of.
    first_serial_that_counts: u64,
    shown: Option<ShownFrame>,
    /// Why the engine stopped, if it has.
    stopped: Option<String>,
    /// The last frame that the engine could not make, and why, or the last picture that it could not use.
    last_problem: Option<String>,
    /// What the user is to be told that the engine has done and gone on after (a graphics device that it replaced), which the window takes when it can say it.
    to_tell_the_user: Vec<String>,
    /// What the frames are put together in and encoded as (from the settings, which the window gives before it draws: [`Self::set_the_colour`]). The compatibility mode
    /// until it is given.
    colour: FrameColour,
    /// How the engine was last told to take checkpoints of the states of effects that depend on earlier frames (it takes none until it is told).
    rules_of_checkpoints: RulesOfCheckpoints,
    /// What the project was when the window last said (see [`Self::the_project_is_now`]).
    state_of_the_project: Option<(u64, u64)>,
    /// Whether a lead-up is wanted of a picture that is not exact: the checkbox *Generate lead-up on seek* of the View menu, which the window says each time it draws.
    lead_ups_are_wanted: bool,
    lead_up: LeadUpOfThePreview,
    /// The fingerprint of the frame that has been on the screen since a lead-up was last made for it (or could not be): none is made again for it, which would be a loop when what the lead-up
    /// makes does not make the picture exact. It is forgotten when the picture is another.
    last_lead_up_was_for: Option<u64>,
    /// The fingerprint of the frame that was described last, and when it was first described as that.
    the_picture_has_been_as_it_is_since: Option<(u64, Instant)>,
    /// How many lead-ups have been wanted.
    lead_ups_wanted: u64,
}

impl EnginePreview {
    /// Starts the engine on `adapter` (this does not wait for it), which calls `wake_up` whenever it has something to say.
    pub fn start(adapter: AdapterChoice, wake_up: WakeUp) -> Self {
        Self::with_engine(Engine::start(EngineSettings::new(adapter), wake_up))
    }

    /// The preview of an engine that is already started.
    pub fn with_engine(engine: Engine) -> Self {
        Self {
            engine,
            sent: HashSet::new(),
            rejected: HashSet::new(),
            last_used_in: HashMap::new(),
            generation: 0,
            asked_for: None,
            frames_asked_for: 0,
            frames_from_the_cache: 0,
            last_serial: 0,
            governor: QualityGovernor::new(QualityRules::default()),
            asked_at: VecDeque::new(),
            what_was_shown: None,
            first_serial_that_counts: 0,
            shown: None,
            stopped: None,
            last_problem: None,
            to_tell_the_user: Vec::new(),
            colour: FrameColour::default(),
            rules_of_checkpoints: RulesOfCheckpoints::NONE,
            state_of_the_project: None,
            lead_ups_are_wanted: false,
            lead_up: LeadUpOfThePreview::None,
            last_lead_up_was_for: None,
            the_picture_has_been_as_it_is_since: None,
            lead_ups_wanted: 0,
        }
    }

    /// What the frames from now on are put together in and encoded as. (A frame of another colour is another frame: the pictures are made for the working space, and
    /// their ids say which, so that the ones that were sent for the colour before are not used. **What the states of effects hold is in the colour that it was made in, so the
    /// checkpoints are let go of.**)
    pub fn set_the_colour(&mut self, colour: FrameColour) {
        if colour != self.colour {
            self.engine.forget_the_checkpoints();
            self.cancel_the_lead_up();
        }
        self.colour = colour;
    }

    /// How the engine takes checkpoints of the states of effects that depend on earlier frames from now on (EN-14: the settings of the program). It is told when this is not what it
    /// was told last, and not otherwise.
    pub fn set_the_rules_of_checkpoints(&mut self, rules: RulesOfCheckpoints) {
        if rules != self.rules_of_checkpoints {
            self.rules_of_checkpoints = rules;
            self.engine.set_the_rules_of_checkpoints(rules);
        }
    }

    /// What the project is now, as a pair of numbers that differ whenever it does (another project, another edit, an undo or a redo): **when it is not what it was the checkpoints are
    /// let go of**, because they are copies of what the effects made of a project that is not the one on the screen. The window says this each time it is drawn.
    pub fn the_project_is_now(&mut self, state: (u64, u64)) {
        if self
            .state_of_the_project
            .replace(state)
            .is_some_and(|before| before != state)
        {
            self.engine.forget_the_checkpoints();
            self.cancel_the_lead_up();
        }
    }

    /// For the tests of what the window does with a lead-up that the preview wants: a preview that wants `wanted` for the frame with this fingerprint, as it does when the picture on the screen is not
    /// exact and has stayed so for long enough (which the tests of the preview itself bring about, with a scripted engine and the clock).
    #[cfg(test)]
    pub(crate) fn wanting_a_lead_up_for_tests(engine: Engine, wanted: LeadUpWanted, for_the_frame: u64) -> Self {
        let mut preview = Self::with_engine(engine);
        preview.lead_up = LeadUpOfThePreview::Wanted { wanted, for_the_frame };
        preview
    }

    /// Whether the picture that is shown is not exact: the engine said so of the frame (EN-14). The window says so over it while it is.
    pub fn the_picture_shown_is_not_exact(&self) -> bool {
        self.shown.as_ref().is_some_and(|shown| shown.approximate)
    }

    /// Whether the picture that is shown is the answer to the frame that was asked for last, and is not exact: the picture that a lead-up is for.
    fn the_picture_shown_is_the_one_asked_for_last_and_is_not_exact(&self) -> bool {
        self.shown
            .as_ref()
            .is_some_and(|shown| shown.approximate && self.asked_for.is_some_and(|(_, serial)| shown.serial == serial))
    }

    /// Says whether a lead-up is wanted of a picture that is not exact: the checkbox *Generate lead-up on seek* of the View menu. The window says it each time it draws; when it is turned off
    /// a lead-up that is going on or wanted is cancelled, and the picture goes on as it is (what it does with an inexact picture without a lead-up: the checkpoints).
    pub fn set_a_lead_up_is_wanted(&mut self, wanted: bool) {
        if !wanted {
            self.cancel_the_lead_up();
        }
        self.lead_ups_are_wanted = wanted;
    }

    /// The lead-up that the preview wants the window to start, if it wants one that nobody has answered: the window starts it with [`Self::start_the_lead_up`], or says that it cannot
    /// with [`Self::the_lead_up_cannot_be_made`]. (The frame that it is for is the one on the screen, which is the frame that the playhead is at.)
    pub fn the_lead_up_that_is_wanted(&self) -> Option<LeadUpWanted> {
        match &self.lead_up {
            LeadUpOfThePreview::Wanted { wanted, .. } => Some(*wanted),
            LeadUpOfThePreview::None | LeadUpOfThePreview::Going { .. } => None,
        }
    }

    /// Whether the engine is making a lead-up. (The line over the picture says so.)
    pub fn a_lead_up_is_being_made(&self) -> bool {
        matches!(self.lead_up, LeadUpOfThePreview::Going { .. })
    }

    /// Starts the lead-up `wanted` that [`Self::the_lead_up_that_is_wanted`] gave: the engine makes it from what comes through `items`, which a thread of the window describes, and
    /// `keeps_alive` is what keeps that thread (dropping it cancels the lead-up). If that is not the lead-up that is wanted any more nothing is started, and both are let go of.
    pub fn start_the_lead_up(
        &mut self,
        wanted: &LeadUpWanted,
        items: Receiver<LeadUpItem>,
        keeps_alive: impl Send + 'static,
    ) {
        let LeadUpOfThePreview::Wanted {
            wanted: asked_for,
            for_the_frame,
        } = &self.lead_up
        else {
            return;
        };
        if asked_for.generation != wanted.generation {
            return;
        }
        let for_the_frame = *for_the_frame;
        self.engine.start_the_lead_up(wanted.generation, items);
        self.lead_up = LeadUpOfThePreview::Going {
            generation: wanted.generation,
            for_the_frame,
            _keeps_alive: Box::new(keeps_alive),
        };
    }

    /// Says that the lead-up `wanted` cannot be made (`why`): there is none for the frame that is on the screen until the picture is another.
    pub fn the_lead_up_cannot_be_made(&mut self, wanted: &LeadUpWanted, why: String) {
        let LeadUpOfThePreview::Wanted {
            wanted: asked_for,
            for_the_frame,
        } = &self.lead_up
        else {
            return;
        };
        if asked_for.generation != wanted.generation {
            return;
        }
        self.last_lead_up_was_for = Some(*for_the_frame);
        self.lead_up = LeadUpOfThePreview::None;
        self.last_problem = Some(why);
    }

    /// Cancels the lead-up that is going on, or wanted: the engine is told, and the thread that makes it is let go of.
    fn cancel_the_lead_up(&mut self) {
        if let LeadUpOfThePreview::Going { generation, .. } =
            std::mem::replace(&mut self.lead_up, LeadUpOfThePreview::None)
        {
            self.engine.cancel_the_lead_up(generation);
        }
    }

    /// The engine said that the lead-up `generation` ended, with a `problem` if it did not go to its end. (One that is not the one going on is nothing.) If it was whole the frame on the screen is
    /// asked for again: the states are what the lead-up made them.
    fn the_lead_up_ended(&mut self, generation: u64, problem: Option<String>) {
        let LeadUpOfThePreview::Going {
            generation: going,
            for_the_frame,
            ..
        } = &self.lead_up
        else {
            return;
        };
        if *going != generation {
            return;
        }
        self.last_lead_up_was_for = Some(*for_the_frame);
        self.lead_up = LeadUpOfThePreview::None;
        match problem {
            None => self.asked_for = None,
            Some(problem) => self.last_problem = Some(format!("the lead-up could not be made: {problem}")),
        }
    }

    /// Looks at whether a lead-up is wanted of the frame that was just described (see the module documentation).
    fn note_whether_a_lead_up_is_wanted(&mut self, context: &Context, facts: &FactsAboutTheFrame, now: Instant) {
        let since = match self.the_picture_has_been_as_it_is_since {
            Some((fingerprint, since)) if fingerprint == facts.fingerprint => since,
            _ => {
                self.the_picture_has_been_as_it_is_since = Some((facts.fingerprint, now));
                now
            }
        };
        // (A lead-up that was made for a frame that was left since is not a reason not to make one when the picture comes back to it.)
        if self
            .last_lead_up_was_for
            .is_some_and(|fingerprint| fingerprint != facts.fingerprint)
        {
            self.last_lead_up_was_for = None;
        }
        let Some(most_frames_a_state_has_gone_on_for) = facts.most_frames_a_state_has_gone_on_for else {
            return;
        };
        let is_worth_it = self.lead_ups_are_wanted
            && matches!(self.lead_up, LeadUpOfThePreview::None)
            && most_frames_a_state_has_gone_on_for > 0
            && self.last_lead_up_was_for != Some(facts.fingerprint)
            && self.the_picture_shown_is_the_one_asked_for_last_and_is_not_exact();
        if !is_worth_it {
            return;
        }
        let has_been_still_for = now.saturating_duration_since(since);
        if has_been_still_for < HOW_LONG_THE_PICTURE_STAYS_STILL_BEFORE_A_LEAD_UP {
            // (The window is not drawn when nothing changes: it is asked to be, when the time is over.)
            context.request_repaint_after(HOW_LONG_THE_PICTURE_STAYS_STILL_BEFORE_A_LEAD_UP - has_been_still_for);
            return;
        }
        self.lead_ups_wanted += 1;
        self.lead_up = LeadUpOfThePreview::Wanted {
            wanted: LeadUpWanted {
                generation: self.lead_ups_wanted,
                render_size: facts.render_size,
                colour: facts.colour,
                most_frames_a_state_has_gone_on_for,
            },
            for_the_frame: facts.fingerprint,
        };
    }

    /// Why the engine has stopped, if it has: the preview is then to be drawn the old way.
    pub fn why_it_stopped(&self) -> Option<&str> {
        self.stopped.as_deref()
    }

    /// What the user is to be told that has not been told yet (the window says it when no question is being asked), oldest first: the engine replaced its
    /// graphics device and went on. Taking it empties it.
    pub fn take_what_the_user_is_to_be_told(&mut self) -> Vec<String> {
        std::mem::take(&mut self.to_tell_the_user)
    }

    /// The last problem the engine had that did not stop it: a frame it could not make, a picture it could not use.
    #[cfg(test)]
    pub fn last_problem(&self) -> Option<&str> {
        self.last_problem.as_deref()
    }

    /// How many frames have been asked for.
    #[cfg(test)]
    pub fn frames_asked_for(&self) -> u64 {
        self.frames_asked_for
    }

    /// How many of the frames that came were in the engine's cache of the frames it has made.
    #[cfg(test)]
    pub fn frames_from_the_cache(&self) -> u64 {
        self.frames_from_the_cache
    }

    /// The number that the window changes whenever it lets go of what it knows of its files.
    #[cfg(test)]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// How the frame that is shown is put on the screen.
    #[cfg(test)]
    pub fn filter_it_is_shown_with(&self) -> Option<PictureFilter> {
        self.shown.as_ref().map(|shown| shown.filter)
    }

    /// Whether the frame that is on the preview is the one that was asked for last (the engine has made it, and it has been taken).
    #[cfg(test)]
    pub fn has_shown_the_last_frame_asked_for(&self) -> bool {
        self.frames_asked_for > 0
            && self
                .shown
                .as_ref()
                .is_some_and(|shown| self.asked_for.is_some_and(|(_, serial)| shown.serial == serial))
    }

    /// Whether a frame of the engine is on the preview.
    #[cfg(test)]
    pub fn has_a_frame_to_show(&self) -> bool {
        self.shown.is_some()
    }

    /// The pixels and the size of the frame that is on the preview.
    #[cfg(test)]
    pub fn the_frame_shown(&self) -> Option<&RenderedFrame> {
        self.shown.as_ref().map(|shown| &*shown.frame)
    }

    /// How many pictures the engine has been sent and has not been told to let go of.
    #[cfg(test)]
    pub fn pictures_the_engine_has(&self) -> usize {
        self.sent.len()
    }

    /// How the window lets go of what it knows of its files (another project, or a file that was replaced): the pictures that come from files get other ids,
    /// the frame is asked for again, and the frames that are on their way, which are of the old ones, are not shown. (The frame that is shown stays until
    /// the new one comes, which is a moment: a picture that goes black in between would flash.)
    pub fn forget_what_is_known_of_the_files(&mut self) {
        self.generation += 1;
        self.asked_for = None;
        self.first_serial_that_counts = self.last_serial + 1;
        self.engine.forget_the_checkpoints();
        self.cancel_the_lead_up();
    }

    /// Takes what the engine has said, describes `content` as a frame made at `whole_size` (or the part of it that the quality of the preview says: see
    /// `engine::quality`), sends what the engine does not have, and asks for the frame if it is not the one asked for last. `filter` says how the frame is put on
    /// the screen, and `clock` how the clock of the project goes.
    pub fn update(
        &mut self,
        context: &Context,
        content: &PreviewContent<'_>,
        whole_size: (u32, u32),
        filter: PictureFilter,
        text_renderer: &mut LazyTextRenderer,
        clock: HowTheClockGoes,
    ) {
        self.take_what_the_engine_says(context, filter, clock);
        if self.stopped.is_some() {
            return;
        }
        if !self.engine.is_running() {
            self.stopped = Some("the picture engine's thread ended without saying why".to_owned());
            return;
        }
        // The frame is described at the size the quality of the preview has now; what is shown (apart from its size) tells the quality whether the picture is
        // being changed, and the quality may say that the size is another, which makes it be described again at that.
        let scale_before = self.governor.scale();
        let mut description = self.describe(content, scale_before.of_the_size(whole_size), text_renderer);
        let what_is_shown_now = description.fingerprint_of_what_is_shown();
        let what_is_shown_has_changed = self.what_was_shown != Some(what_is_shown_now);
        let scale = self
            .governor
            .the_window_is_drawn(clock.now, clock.is_playing, what_is_shown_has_changed);
        if scale != scale_before {
            description = self.describe(content, scale.of_the_size(whole_size), text_renderer);
        }
        self.what_was_shown = Some(description.fingerprint_of_what_is_shown());

        let fingerprint = description.fingerprint();
        let facts = FactsAboutTheFrame {
            fingerprint,
            most_frames_a_state_has_gone_on_for: description.most_frames_a_state_has_gone_on_for(),
            render_size: description.render_size,
            colour: description.colour,
        };
        if self.asked_for.map(|(asked, _)| asked) != Some(fingerprint) {
            // (A lead-up is for the frame that is on the screen. Another frame makes it of no use, and it is cancelled **before** the frame is asked for, so that the engine does not make the frame in
            // the middle of it. The same frame asked for again, which it is when a picture it needs was lost, is not another frame.)
            if !self.lead_up_is_for(fingerprint) {
                self.cancel_the_lead_up();
            }
            self.note_the_pictures_used(&description);
            let serial = self.engine.request_frame(description);
            self.asked_for = Some((fingerprint, serial));
            self.last_serial = serial;
            self.frames_asked_for += 1;
            self.asked_at.push_back((serial, clock.now));
            while self.asked_at.len() > MOST_FRAMES_WAITED_FOR {
                self.asked_at.pop_front();
            }
            self.let_go_of_the_pictures_that_are_not_used_any_more();
        }
        self.put_the_frame_on_the_screen_the_way_it_is_asked_for(context, filter);
        self.note_whether_a_lead_up_is_wanted(context, &facts, clock.now);
        // (A picture that is still is made whole after a while, and the window is not drawn when nothing changes: it is asked to be.)
        if let Some(wait) = self.governor.how_long_until_the_picture_is_made_whole(clock.now) {
            context.request_repaint_after(wait);
        }
    }

    /// Whether the lead-up that is going on or wanted is for the frame with this fingerprint.
    fn lead_up_is_for(&self, fingerprint: u64) -> bool {
        match &self.lead_up {
            LeadUpOfThePreview::None => false,
            LeadUpOfThePreview::Wanted { for_the_frame, .. } | LeadUpOfThePreview::Going { for_the_frame, .. } => {
                *for_the_frame == fingerprint
            }
        }
    }

    /// The frame that `content` is, made at `render_size`, with the pictures it needs sent to the engine if it does not have them.
    fn describe(
        &mut self,
        content: &PreviewContent<'_>,
        render_size: (u32, u32),
        text_renderer: &mut LazyTextRenderer,
    ) -> FrameDescription {
        let Self {
            engine,
            sent,
            rejected,
            generation,
            colour,
            ..
        } = self;
        let mut supply = SuppliesWhatIsNotRejected {
            inner: SentPictures {
                sent,
                text_renderer,
                send: |id, data| engine.upload_picture(id, Arc::new(data)),
            },
            rejected,
        };
        describe_frame_in_colour(content, render_size, *generation, *colour, &mut supply)
    }

    /// Paints the frame that is shown over `rectangle` (all of the picture, wherever it is on the screen, which may reach past what `painter` is clipped to).
    pub fn paint(&self, painter: &Painter, rectangle: Rect) {
        if let Some(shown) = &self.shown {
            painter.image(
                shown.texture.id(),
                rectangle,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    }

    /// Tells the engine's thread to stop (the preview is then to be drawn the old way).
    #[cfg(test)]
    pub fn stop(&self) {
        self.engine.stop();
    }

    fn take_what_the_engine_says(&mut self, context: &Context, filter: PictureFilter, clock: HowTheClockGoes) {
        while let Some(event) = self.engine.try_next_event() {
            match event {
                EngineEvent::Frame {
                    serial,
                    frame,
                    statistics,
                } => {
                    self.frames_from_the_cache += u64::from(statistics.came_from_the_cache);
                    self.note_how_long_the_frame_took(serial, statistics.came_from_the_cache, clock);
                    self.show(context, serial, frame, filter, statistics.approximate);
                }
                // (The preview does not ask for frames of the export.)
                EngineEvent::ExportFrame { .. } => {}
                EngineEvent::FrameFailed { problem, .. } => self.last_problem = Some(problem),
                EngineEvent::PicturesMissing { ids, .. } => {
                    // (They are sent again, and the frame is asked for again.)
                    for id in ids {
                        self.sent.remove(&id);
                        self.last_used_in.remove(&id);
                    }
                    self.asked_for = None;
                }
                EngineEvent::PictureRejected { id, problem } => {
                    self.sent.remove(&id);
                    self.last_used_in.remove(&id);
                    self.rejected.insert(id);
                    self.last_problem = Some(problem);
                    self.asked_for = None;
                }
                EngineEvent::EffectRejected { problem, .. } => self.last_problem = Some(problem),
                EngineEvent::LeadUpDone { generation, .. } => self.the_lead_up_ended(generation, None),
                EngineEvent::LeadUpFailed { generation, problem } => self.the_lead_up_ended(generation, Some(problem)),
                EngineEvent::DeviceReplaced {
                    why,
                    effects_switched_off,
                } => self.note_that_the_device_was_replaced(why, &effects_switched_off),
                EngineEvent::Stopped { problem } => self.stopped = Some(problem),
            }
        }
    }

    /// The engine replaced its graphics device (`why`), and does not run the effects `effects_switched_off` any more: the user is to be told (each time, in order),
    /// and the pictures are to be sent again.
    fn note_that_the_device_was_replaced(&mut self, why: String, effects_switched_off: &[String]) {
        // (The new device has none of the pictures: they are all sent again, and the frame is asked for again, which the engine may now make
        // without an effect that it does not trust any more.)
        self.sent.clear();
        self.last_used_in.clear();
        self.asked_for = None;
        // (The lead-up was on the old device: the engine has let go of it, and it is not going on.)
        self.cancel_the_lead_up();
        self.to_tell_the_user
            .push(the_words_about_a_replaced_device(&why, effects_switched_off));
        self.last_problem = Some(why);
    }

    /// Tells the quality of the preview how long the frame with this serial number took from being asked for to coming, if it is one that the card made. (A frame
    /// that came from the cache says nothing about what the card can do; and the frames before it that never came were passed over by the engine, which makes only the
    /// newest.)
    fn note_how_long_the_frame_took(&mut self, serial: u64, came_from_the_cache: bool, clock: HowTheClockGoes) {
        let asked_at = self
            .asked_at
            .iter()
            .find(|(asked, _)| *asked == serial)
            .map(|(_, asked_at)| *asked_at);
        self.asked_at.retain(|(asked, _)| *asked > serial);
        if let (Some(asked_at), false) = (asked_at, came_from_the_cache) {
            self.governor.a_frame_came(
                clock.now.saturating_duration_since(asked_at),
                clock.how_long_a_frame_lasts,
                clock.is_playing,
            );
        }
    }

    /// Makes the frame the one that is shown, unless a later one already is or it is of what has been let go of.
    fn show(
        &mut self,
        context: &Context,
        serial: u64,
        frame: Arc<RenderedFrame>,
        filter: PictureFilter,
        approximate: bool,
    ) {
        if serial < self.first_serial_that_counts || self.shown.as_ref().is_some_and(|shown| shown.serial > serial) {
            return;
        }
        let image = image_of(&frame);
        match &mut self.shown {
            Some(shown)
                if shown.frame.width_in_pixels == frame.width_in_pixels
                    && shown.frame.height_in_pixels == frame.height_in_pixels =>
            {
                shown.texture.set(image, filter.options());
                shown.frame = frame;
                shown.filter = filter;
                shown.serial = serial;
                shown.approximate = approximate;
            }
            _ => {
                self.shown = Some(ShownFrame {
                    texture: context.load_texture("the preview, made by the picture engine", image, filter.options()),
                    frame,
                    filter,
                    serial,
                    approximate,
                });
            }
        }
    }

    /// Puts the frame that is shown on the graphics card again if the way it is to be put on the screen is not the way it was.
    fn put_the_frame_on_the_screen_the_way_it_is_asked_for(&mut self, _context: &Context, filter: PictureFilter) {
        if let Some(shown) = &mut self.shown
            && shown.filter != filter
        {
            shown.texture.set(image_of(&shown.frame), filter.options());
            shown.filter = filter;
        }
    }

    fn note_the_pictures_used(&mut self, description: &FrameDescription) {
        let frame_number = self.frames_asked_for + 1;
        for id in description.pictures_used() {
            self.last_used_in.insert(id, frame_number);
        }
    }

    /// Lets go of the pictures that none of the last frames asked for used, when there are more than a few: the pictures of a video are new ones all the time.
    fn let_go_of_the_pictures_that_are_not_used_any_more(&mut self) {
        if self.sent.len() <= MOST_PICTURES_KEPT_WITHOUT_USE {
            return;
        }
        let this_frame = self.frames_asked_for;
        let not_used: Vec<PictureId> = self
            .sent
            .iter()
            .copied()
            .filter(|id| {
                self.last_used_in
                    .get(id)
                    .is_none_or(|last| last + FRAMES_A_PICTURE_STAYS_FOR < this_frame)
            })
            .collect();
        if not_used.is_empty() {
            return;
        }
        for id in &not_used {
            self.sent.remove(id);
            self.last_used_in.remove(id);
        }
        self.engine.forget_pictures(not_used);
    }
}

/// The picture of a frame as the window's graphics card takes it: it is all opaque, so what is premultiplied and what is not is the same.
fn image_of(frame: &RenderedFrame) -> ColorImage {
    ColorImage::from_rgba_premultiplied(
        [frame.width_in_pixels as usize, frame.height_in_pixels as usize],
        &frame.rgba_pixels,
    )
}

/// The supply of [`SentPictures`], but for the pictures that the engine said it could not use, which it says it cannot make again.
struct SuppliesWhatIsNotRejected<'a, Send: FnMut(PictureId, crate::engine::PictureData)> {
    inner: SentPictures<'a, Send>,
    rejected: &'a HashSet<PictureId>,
}

impl<Send: FnMut(PictureId, crate::engine::PictureData)> PictureSupply for SuppliesWhatIsNotRejected<'_, Send> {
    fn make_sure_the_engine_has(&mut self, wanted: &PictureWanted<'_>) -> bool {
        !self.rejected.contains(&wanted.id) && self.inner.make_sure_the_engine_has(wanted)
    }
}

#[cfg(test)]
mod tests;
