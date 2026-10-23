// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What is said between the window and the engine's thread, both ways.

use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use super::device::AdapterChoice;
use super::effect_states::RulesOfCheckpoints;
use super::guard::{DEFAULT_MOST_PASSES_OF_A_LOOP, GuardedShader};
use super::shader_structure::ShaderStructure;
use super::{FrameDescription, FrameStatistics, PictureData, PictureId, RenderedFrame, RenderedPlanes, YuvColour};

/// What the engine calls when it has something to tell: the window's way of being asked to draw again, so that the engine's thread does not
/// have to be waited for. (It is called from the engine's thread.)
pub type WakeUp = Arc<dyn Fn() + Send + Sync>;

/// What the thread that describes the frames of a lead-up says to the engine, in the order that the engine is to do it in (EN-14, the lead-up on a seek; `docs/engine-phase-6-6-design.md`,
/// section 4).
#[derive(Debug)]
pub enum LeadUpItem {
    /// A picture that the frames after it are made of. **Its id is one that [`PictureId::in_the_lead_up`] made**, so that it is never the id of a picture that the preview sent.
    Picture { id: PictureId, data: Arc<PictureData> },
    /// Pictures that none of the frames after this one are made of: let go of them.
    Forget(Vec<PictureId>),
    /// The next frame of the lead-up. The engine makes it for what it does to the states of the effects that depend on earlier frames, and takes no pixels of it.
    Frame(FrameDescription),
    /// There are no more frames: the lead-up is whole. (The engine says [`EngineEvent::LeadUpDone`].)
    End,
    /// The frames could not be described any more (a video that cannot be read, a project that cannot be planned): the lead-up stops, and the states are as the frames so far made them.
    /// (The engine says [`EngineEvent::LeadUpFailed`].)
    Failed(String),
}

/// What the window asks of the engine.
#[derive(Debug)]
pub enum EngineRequest {
    /// Here is a picture, which frames can then refer to by `id`. (If the engine has a picture under that id already, this one replaces it: the
    /// window uses the same id only for the same pixels, so it does not send a picture again.) The bytes are shared, not copied.
    Upload { id: PictureId, data: Arc<PictureData> },
    /// Let go of these pictures.
    Forget(Vec<PictureId>),
    /// From now on the effect under `key` is run by this shader, which has been through the guard (an effect that was under that key is replaced).
    InstallEffect {
        key: String,
        shader: Arc<GuardedShader>,
        /// What the shader is made of besides its code (see [`ShaderStructure`]).
        structure: Arc<ShaderStructure>,
    },
    /// Make the frame `description` describes. If more requests have come by the time the engine looks, only the newest of them is made, and
    /// the others are not answered: the window only wants the latest.
    Render { serial: u64, description: FrameDescription },
    /// Make the frame `description` describes for the export, as the planes of a `yuv420p` picture made the way `colour` says. **Every such request is made, in the
    /// order it was asked in** (none is passed over for a newer one, and none is looked for in the cache of finished frames), and answered with an
    /// [`EngineEvent::ExportFrame`] (or, if it could not be made, with the same events a frame of the preview has), in that order. Pictures that the frame uses are
    /// sent before it, and the ones that none of the frames after it uses can be let go of after it: the requests are done in the order they come in.
    RenderForExport {
        serial: u64,
        description: FrameDescription,
        colour: YuvColour,
    },
    /// From now on the frames of the preview take checkpoints of the states of effects that depend on earlier frames, and go on from them after a jump, as `RulesOfCheckpoints` say
    /// (EN-14; `RulesOfCheckpoints::NONE` takes none, as the engine does until it is told). The export takes none.
    SetTheRulesOfCheckpoints(RulesOfCheckpoints),
    /// Let go of every checkpoint: what they are copies of is not there any more (the project was edited or loaded, or the colours of the frames changed).
    ForgetTheCheckpoints,
    /// Begin the lead-up `generation` (one that is going on is let go of first): **the frames that `items` brings are made, in the order they come in, for what they do to the states of the
    /// effects that depend on earlier frames, and no pixels are taken of them** (EN-14, option 1). The frames are of the frames *before* the one the preview shows, so that the state that the
    /// frame goes on from is the state of the frame before it. The engine takes what is waiting of what the window asks of it before it takes the next of the lead-up, so a frame of the preview
    /// is not kept waiting for the lead-up; and it says [`EngineEvent::LeadUpDone`] or [`EngineEvent::LeadUpFailed`] when the lead-up ends, and nothing when it is cancelled.
    StartTheLeadUp {
        generation: u64,
        items: Receiver<LeadUpItem>,
    },
    /// Begin the lead-up `generation` **as a chase** (the lead-up of a seek while the project plays; EN-14; `docs/engine-phase-6-6-design.md`, section 4): `items` brings the frames from before the frame
    /// that the preview shows **on to the end of the project**, in order, and the engine makes them for the states of the effects **in a store of states of its own**, so that the frames of the
    /// preview, which go on being made, are not touched. After each frame it looks whether those states are of the same frame as the ones that the last frame of the preview used; **when they are
    /// they go over to the preview's, which goes on from them exactly**, and the engine says [`EngineEvent::LeadUpDone`]. If the frames are over before that, or the states can never be the same
    /// (they are of a later frame, or of other sizes), it says [`EngineEvent::LeadUpFailed`]. Otherwise it is as [`Self::StartTheLeadUp`]: what the window asks comes first, and the chase goes on
    /// in the moments between.
    StartTheChase {
        generation: u64,
        items: Receiver<LeadUpItem>,
    },
    /// The lead-up `generation` (a chase too) is not wanted any more (the playhead moved, or the project was edited): it is let go of, and so are the pictures that it sent. (A lead-up that is not
    /// the one going on is not touched.)
    CancelTheLeadUp { generation: u64 },
    /// Stop.
    Stop,
}

/// What the engine tells the window.
#[derive(Debug)]
pub enum EngineEvent {
    /// The frame of the request with this serial number.
    Frame {
        serial: u64,
        frame: Arc<RenderedFrame>,
        statistics: FrameStatistics,
    },
    /// The frame of the export with this serial number, as planes.
    ExportFrame {
        serial: u64,
        planes: RenderedPlanes,
        statistics: FrameStatistics,
    },
    /// The frame of the request with this serial number could not be made. The engine goes on.
    FrameFailed { serial: u64, problem: String },
    /// The frame of the request with this serial number uses pictures that the engine does not have (they were not sent, or were let go of to
    /// make room, or the graphics device was replaced). Send them, and ask again.
    PicturesMissing { serial: u64, ids: Vec<PictureId> },
    /// A picture that was sent could not be used, and is not kept.
    PictureRejected { id: PictureId, problem: String },
    /// The shader that was given for the effect under `key` could not be used (the graphics library would not take it): the effect is not installed, and a
    /// frame that asks for it is made without it.
    EffectRejected { key: String, problem: String },
    /// The graphics device was replaced by a new one: it was lost, or a frame did not finish in time (`why` says which). The new device has none of the
    /// pictures the old one had: **send them again** (the engine would say so, a frame at a time, with `PicturesMissing`). Frames that were made are not kept.
    /// `effects_switched_off` are the keys of the effects that were in the frame that did not finish: they are not run again as long as the engine runs, and
    /// frames that ask for them are made without them.
    DeviceReplaced {
        why: String,
        effects_switched_off: Vec<String>,
    },
    /// The lead-up `generation` was made to its end: `frames_made` frames went through the engine, and the states of the effects are what they made them.
    LeadUpDone { generation: u64, frames_made: u64 },
    /// The lead-up `generation` stopped before its end, for this reason: the states of the effects are as the frames so far made them.
    LeadUpFailed { generation: u64, problem: String },
    /// The engine has stopped, and will make no more frames (it could not start, or could not go on, or was told to stop).
    Stopped { problem: String },
}

/// How the engine is set up.
#[derive(Clone, Debug)]
pub struct EngineSettings {
    pub adapter: AdapterChoice,
    /// How long the card gets to make a frame before the engine gives up on it.
    pub frame_timeout: Duration,
    /// How many finished frames are kept for when the same frame is asked for again.
    pub most_cached_frames: usize,
    /// How many bytes the kept frames take up at most.
    pub most_cached_frame_bytes: usize,
    /// How many passes a loop of a shader that was given to the engine is let make (see `guard`).
    pub most_passes_of_a_loop: u32,
    /// How many frames of the export may be on their way at once: while the card is at work on one, the next is recorded and the one before is read back. One is
    /// no overlap at all.
    pub export_frames_in_flight: usize,
    /// When the graphics device has had to be replaced this many times within `how_long_replacements_are_counted`, the engine stops instead (see `replacement`).
    pub replacements_that_stop_the_engine: usize,
    pub how_long_replacements_are_counted: Duration,
    /// For the tests: the thread does nothing, once it has its device, until this is opened, so that a test can have requests pile up.
    #[cfg(test)]
    pub(crate) hold_the_first_batch_until: Option<Arc<super::testing::Gate>>,
    /// For the tests: told about every device the engine makes, the first and each that replaces another, so that a test can do to it what a card does.
    #[cfg(test)]
    pub(crate) watch_the_devices: Option<super::testing::DeviceWatcher>,
}

impl EngineSettings {
    /// For a test that is **not about the watchdog**: the usual settings, but a card is given a minute for a frame, not two seconds. The tests run many at once on a computer that
    /// is busy with them, and a software adapter that is slow because the other tests have the processor is not a card that has stopped (found when a test of the window and one
    /// of the export each failed once in a full run, and the export did again under load). The tests of the watchdog itself use [`Self::new`].
    #[cfg(test)]
    pub(crate) fn patient(adapter: AdapterChoice) -> Self {
        Self {
            frame_timeout: Duration::from_secs(60),
            ..Self::new(adapter)
        }
    }

    /// The usual settings, with the device made from `adapter`.
    pub fn new(adapter: AdapterChoice) -> Self {
        Self {
            adapter,
            frame_timeout: Duration::from_secs(2),
            most_cached_frames: 32,
            most_cached_frame_bytes: 256 * 1024 * 1024,
            most_passes_of_a_loop: DEFAULT_MOST_PASSES_OF_A_LOOP,
            export_frames_in_flight: 2,
            replacements_that_stop_the_engine: 3,
            how_long_replacements_are_counted: Duration::from_secs(60),
            #[cfg(test)]
            hold_the_first_batch_until: None,
            #[cfg(test)]
            watch_the_devices: None,
        }
    }
}
