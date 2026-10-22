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
//! **Two working spaces** (`colour`, CM-20): the **compatibility mode** (`WorkingSpace::DisplayReferred`) makes the pictures that the window drew before it had an engine,
//! so that the two can be compared and the engine proved: colours are mixed in gamma-encoded numbers, as they always were, and layers are blended premultiplied, as the
//! window's own renderer blends them. **Linear light** (`WorkingSpace::Linear`, phase 3) is the one that the program uses: the numbers are light in the primaries of the
//! project's working space, pictures are decoded to it when they are sent, and the finished frame is encoded for the display or the file at the end.
//!
//! How the files divide the work:
//!
//! * the messages and what they carry: `scene` (what to draw), `frames` (what comes back), `messages` (the requests and events), and `handle`,
//!   which is the [`Engine`] the window holds, and `worker`, the thread behind it;
//! * the device and what is kept on it: `device`, `texture_pool` and `recycler` (what pools are made of), `uniform_bytes` (the numbers
//!   the passes are given), `readback`, and `frame_cache`;
//! * making a frame: `plan` (which passes, worked out without a card), `pipelines` and `shaders`, and `frame_renderer` (one frame, recorded and
//!   submitted).

pub mod benchmark;
mod byte_lru;
mod colour;
mod device;
mod effect_shaders;
mod frame_cache;
mod frame_renderer;
mod frames;
mod guard;
mod handle;
mod messages;
mod own_effects;
mod picture_cache;
mod pictures;
mod pipelines;
mod placement;
mod plan;
mod quality;
mod readback;
mod recycler;
mod replacement;
mod scene;
mod shader_steps;
mod shader_structure;
mod shaders;
mod tables;
#[cfg(test)]
pub(crate) mod testing;
mod texture_pool;
mod uniform_arena;
mod uniform_bytes;
mod worker;
mod yuv;

#[cfg(test)]
mod blur_and_crop_tests;
#[cfg(test)]
mod chroma_tests;
#[cfg(test)]
mod colour_tests;
#[cfg(test)]
mod domain_tests;
#[cfg(test)]
mod effect_tests;
#[cfg(test)]
mod encoded_planes_tests;
#[cfg(test)]
mod engine_tests;
#[cfg(test)]
mod export_frames_tests;
#[cfg(test)]
mod foreign_effect_tests;
#[cfg(test)]
mod gain_tests;
#[cfg(test)]
mod group_tests;
#[cfg(test)]
mod guard_tests;
#[cfg(test)]
mod hang_tests;
#[cfg(test)]
mod isf_pass_tests;
#[cfg(test)]
mod linear_light_tests;
#[cfg(test)]
mod lut_tests;
#[cfg(test)]
mod multi_pass_tests;
#[cfg(test)]
pub(crate) mod oracle_tests;
#[cfg(test)]
mod picture_tests;
#[cfg(test)]
mod planes_out_tests;
#[cfg(test)]
mod yuv_tests;

pub use colour::{
    ColourDomain, DisplayDomainConversion, FrameColour, IngestConversion, LutSpaceConversion, MixingParts,
    OutputConversion, OutputEncoding, PictureColour, WorkingSpace,
};
pub use device::{
    AdapterChoice, DeviceError, DeviceTrouble, EngineDevice, TESTS_MAY_USE_ANY_ADAPTER_VARIABLE,
    there_is_a_software_adapter,
};
pub use effect_shaders::{
    FOREIGN_EFFECT_UNIFORM_BYTES, MOST_NUMBERS_OF_A_FOREIGN_EFFECT, NUMBERS_THE_ENGINE_ADDS, publish_effect_shader,
    publish_effect_shader_with, published_effect_shader, published_effect_shader_with_structure,
};
pub use frame_cache::FrameCache;
pub use frame_renderer::{FinishedFrame, FrameOutput, FrameRenderer, PendingFrame, RenderProblem};
pub use frames::{FrameStatistics, RenderedFrame, RenderedPlanes};
pub use guard::{DEFAULT_MOST_PASSES_OF_A_LOOP, GuardedShader, ShaderProblem, cap_loops, guard_wgsl};
pub use handle::{EffectShaderReport, Engine};
pub use messages::{EngineEvent, EngineRequest, EngineSettings, WakeUp};
pub use pictures::{PictureData, PicturePixels, PictureProblem};
pub use plan::{
    CallDomain, EffectStep, EffectToRun, EffectsThatRun, FramePlan, KindOfEffect, NUMBERS_OF_A_COLOUR_MATRIX, PassPlan,
    blur_changes_nothing, crop_changes_nothing, exposure_changes_nothing, lut_changes_nothing, opacity_changes_nothing,
    plan_frame, steps_of_a_run_of_effects,
};
pub use quality::{PreviewScale, QualityGovernor, QualityRules};
pub use scene::{
    BLUR_KEY, COLOUR_MATRIX_KEY, CORNERS_OF_THE_WHOLE_PICTURE, CROP_KEY, EXPOSURE_KEY, EffectCall, FlatLayer,
    FlatLayers, FrameDescription, GroupDescription, LUT_KEY, LayerDescription, LayerSource, LutInterpolation, LutSpace,
    OPACITY_KEY, PictureId, TableId,
};
pub use shader_structure::{
    BufferSpec, ImageSlot, ImageSource, LARGEST_SIDE_OF_A_BUFFER, MOST_IMAGES_OF_A_SHADER, MOST_PASSES_OF_A_SHADER,
    MathFunction, Operator, PassFacts, PassSpec, ShaderStructure, SizeExpression, TargetFormat, size_from,
};
pub use tables::{MOST_TABLES_ON_THE_CARD, publish_table, published_table};
pub use texture_pool::{PoolStatistics, TexturePool, TextureShape};
pub use uniform_bytes::{SlotWriter, UniformBytes, UniformSlot};
pub use yuv::{
    ChromaReconstruction, RgbToYuvConversion, YuvColour, YuvConversion, YuvMatrix, YuvRange, chroma_size,
    reference_rgba as rgba_of_yuv420p_planes, reference_rgba_smooth as smooth_rgba_of_yuv420p_planes,
    reference_yuv420p as yuv420p_planes_of_rgba,
};
