// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The picture engine: makes the finished picture of the preview with the graphics card.
//!
//! It runs on **a thread of its own, with a graphics device of its own**, and is spoken to by messages: the window gives it a
//! [`FrameDescription`] (what is on the frame, where, and what is done to each picture; pictures themselves are sent apart and are
//! referred to by their [`PictureId`]s) and gets back a [`RenderedFrame`], plain RGBA bytes, which it shows as an ordinary texture. The engine knows
//! nothing of the project, the timeline or the window, and nothing in a message points into the program's data, so the same messages could go to another
//! process one day (EN-12). The design, with the reasons for it, is in `docs/engine-phase-1-design.md`.
//!
//! This is the **compatibility mode** (CM-20): it makes the pictures that the window drew before it had an engine, so that the two can be compared
//! and the engine proved; colours are mixed in gamma-encoded numbers, as they always were, and layers are blended premultiplied, as the window's own
//! renderer blends them. Linear light is a later, separate step.
//!
//! How the files divide the work:
//!
//! * the messages and what they carry: `scene` (what to draw), `frames` (what comes back), `messages` (the requests and events), and `handle`,
//!   which is the [`Engine`] the window holds, and `worker`, the thread behind it;
//! * the device and what is kept on it: `device`, `texture_pool` and `recycler` (what pools are made of), `uniform_bytes` (the numbers
//!   the passes are given), `readback`, and `frame_cache`;
//! * making a frame: `plan` (which passes, worked out without a card), `pipelines` and `shaders`, and `frame_renderer` (one frame, recorded and
//!   submitted).

mod device;
mod frame_cache;
mod frame_renderer;
mod frames;
mod handle;
mod messages;
mod pipelines;
mod plan;
mod readback;
mod recycler;
mod scene;
mod shaders;
#[cfg(test)]
mod testing;
mod texture_pool;
mod uniform_bytes;
mod worker;

#[cfg(test)]
mod engine_tests;

pub use device::{AdapterChoice, DeviceError, DeviceTrouble, EngineDevice};
pub use frame_cache::FrameCache;
pub use frame_renderer::{FrameRenderer, RenderProblem};
pub use frames::{FrameStatistics, RenderedFrame};
pub use handle::Engine;
pub use messages::{EngineEvent, EngineRequest, EngineSettings, WakeUp};
pub use plan::{FramePlan, PassPlan, plan_frame};
pub use scene::{
    COLOUR_MATRIX_KEY, CORNERS_OF_THE_WHOLE_PICTURE, EffectCall, FrameDescription, LayerDescription, LayerSource,
    PictureId,
};
pub use texture_pool::{PoolStatistics, TexturePool, TextureShape};
pub use uniform_bytes::{SlotWriter, UniformBytes, UniformSlot};
