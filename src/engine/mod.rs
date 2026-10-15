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

mod byte_lru;
mod device;
mod effect_shaders;
mod frame_cache;
mod frame_renderer;
mod frames;
mod guard;
mod handle;
mod messages;
mod picture_cache;
mod pictures;
mod pipelines;
mod placement;
mod plan;
mod readback;
mod recycler;
mod replacement;
mod scene;
mod shaders;
#[cfg(test)]
pub(crate) mod testing;
mod texture_pool;
mod uniform_arena;
mod uniform_bytes;
mod worker;
mod yuv;

#[cfg(test)]
mod effect_tests;
#[cfg(test)]
mod engine_tests;
#[cfg(test)]
mod foreign_effect_tests;
#[cfg(test)]
mod guard_tests;
#[cfg(test)]
mod hang_tests;
#[cfg(test)]
pub(crate) mod oracle_tests;
#[cfg(test)]
mod picture_tests;
#[cfg(test)]
mod yuv_tests;

pub use device::{AdapterChoice, DeviceError, DeviceTrouble, EngineDevice};
pub use effect_shaders::{FOREIGN_EFFECT_UNIFORM_BYTES, MOST_NUMBERS_OF_A_FOREIGN_EFFECT};
pub use frame_cache::FrameCache;
pub use frame_renderer::{FrameRenderer, RenderProblem};
pub use frames::{FrameStatistics, RenderedFrame};
pub use guard::{DEFAULT_MOST_PASSES_OF_A_LOOP, GuardedShader, ShaderProblem, cap_loops, guard_wgsl};
pub use handle::{EffectShaderReport, Engine};
pub use messages::{EngineEvent, EngineRequest, EngineSettings, WakeUp};
pub use pictures::{PictureData, PicturePixels, PictureProblem};
pub use plan::{EffectsThatRun, FramePlan, KindOfEffect, PassPlan, plan_frame};
pub use scene::{
    COLOUR_MATRIX_KEY, CORNERS_OF_THE_WHOLE_PICTURE, EffectCall, FrameDescription, LayerDescription, LayerSource,
    PictureId,
};
pub use texture_pool::{PoolStatistics, TexturePool, TextureShape};
pub use uniform_bytes::{SlotWriter, UniformBytes, UniformSlot};
pub use yuv::{YuvColour, YuvConversion, YuvMatrix, YuvRange, chroma_size, reference_rgba as rgba_of_yuv420p_planes};
