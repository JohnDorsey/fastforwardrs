// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What shaders that depend on earlier frames keep from one frame to the next: the buffers of theirs that last (phase 6.4.2; see `docs/engine-phase-6-4-design.md`, K8 to K11).
//!
//! **A state is two textures for each buffer that lasts, `latest` and `older`**, kept under the identity that the call gives ([`EffectState`](super::EffectState)), and the frame that
//! the state was made at. What a call does with it depends on its own frame `f` and on the frame `n` the state was made at:
//!
//! * `f = n + 1`: **go on**. The call reads `latest` as the state before the frame and draws into `older`; afterwards they change places.
//! * `f = n`: **the same frame again** (a repaint, or a frame asked for twice). The call reads `older`, which is what the state was before frame `n`, and draws into `latest`: it makes
//!   the same picture, and does not advance.
//! * anything else, or no state yet: **start clear**. The call reads `older`, which is cleared, and draws into `latest`.
//!
//! Both of the last two leave `older` as the state before the frame. So the memory is two textures for each buffer, whatever the number of frames.

use std::collections::HashMap;

use super::effect_shaders::texture_format_of;
use super::shader_structure::TargetFormat;

/// The most bytes of textures that all the states together may take. (When there are more, the one used least recently goes first; its effect starts clear at its next frame.)
pub const MOST_BYTES_OF_STATES: u64 = 256 * 1024 * 1024;

/// How many frames can be made without a state being used before it is let go of.
pub const FRAMES_BEFORE_A_STATE_IS_LET_GO_OF: u64 = 600;

/// What a call does with its state (see the module's documentation).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Continuation {
    GoOn,
    SameFrame,
    StartClear,
}

/// What a call at frame `frame` does with a state that was made at frame `kept_at` (`None`: there is none).
pub fn continuation(kept_at: Option<u64>, frame: u64) -> Continuation {
    match kept_at {
        Some(kept) if kept.checked_add(1) == Some(frame) => Continuation::GoOn,
        Some(kept) if kept == frame => Continuation::SameFrame,
        _ => Continuation::StartClear,
    }
}

/// What a buffer that lasts is: how big, and in what format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferWanted {
    pub size: (u32, u32),
    pub format: TargetFormat,
}

impl BufferWanted {
    fn bytes(self) -> u64 {
        let pixel = texture_format_of(self.format).block_copy_size(None).unwrap_or(8);
        u64::from(self.size.0) * u64::from(self.size.1) * u64::from(pixel)
    }
}

struct PersistentBuffer {
    wanted: BufferWanted,
    latest: wgpu::Texture,
    older: wgpu::Texture,
}

struct State {
    /// The frame the state was made at; `None` before the first frame that used it was made.
    frame: Option<u64>,
    /// One for each buffer of the shader: `Some` for those that last.
    buffers: Vec<Option<PersistentBuffer>>,
    /// The number of frames made when it was last used.
    last_used: u64,
}

impl State {
    fn bytes(&self) -> u64 {
        self.buffers
            .iter()
            .flatten()
            .map(|buffer| 2 * buffer.wanted.bytes())
            .sum()
    }
}

/// What a call is given to run its passes with: where each buffer that lasts is read from and drawn into, and what has to be cleared first.
pub struct BuffersOfACall {
    pub continuation: Continuation,
    /// For each buffer of the shader, `Some` if it lasts: the state before the frame, which a pass reads before the pass that draws the buffer.
    pub base: Vec<Option<wgpu::TextureView>>,
    /// For each buffer of the shader, `Some` if it lasts: what the pass that draws the buffer draws into.
    pub target: Vec<Option<wgpu::TextureView>>,
    /// What is to be cleared before the first pass (the state before the frame, when the call starts clear).
    pub to_clear: Vec<wgpu::TextureView>,
}

/// The states of all the shaders that depend on earlier frames.
#[derive(Default)]
pub struct StatesOfEffects {
    states: HashMap<u64, State>,
    /// How many frames were made, which says how long ago a state was used.
    frames_made: u64,
}

fn texture_of(device: &wgpu::Device, wanted: BufferWanted, label: &str) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: wanted.size.0,
            height: wanted.size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: texture_format_of(wanted.format),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn view_of(texture: &wgpu::Texture) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

impl StatesOfEffects {
    /// How many states there are.
    pub fn len(&self) -> usize {
        self.states.len()
    }

    /// How many bytes of textures all the states take.
    pub fn bytes(&self) -> u64 {
        self.states.values().map(State::bytes).sum()
    }

    /// What the call at frame `frame` that goes on with the state `id` is given. `wanted` has one place for each buffer of the shader: `Some` for each that lasts, with what it is
    /// wanted as for this call (its size is worked out from the call's numbers, so it can change; a buffer that changes size, or a state that did not have it, starts clear).
    /// **Nothing is kept until [`Self::commit`]**: a frame that is not made is [`Self::abandon`]ed.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        id: u64,
        frame: u64,
        wanted: &[Option<BufferWanted>],
    ) -> BuffersOfACall {
        let state = self.states.entry(id).or_insert_with(|| State {
            frame: None,
            buffers: Vec::new(),
            last_used: 0,
        });
        state.last_used = self.frames_made;
        let mut continuation = continuation(state.frame, frame);
        // (A buffer that is not what it was, or that there was none of, has nothing to go on from: the whole call starts clear.)
        state.buffers.resize_with(wanted.len(), || None);
        for (place, want) in wanted.iter().enumerate() {
            let kept = state.buffers[place].as_ref().map(|buffer| buffer.wanted);
            if kept != *want {
                continuation = Continuation::StartClear;
                state.buffers[place] = want.map(|wanted| PersistentBuffer {
                    wanted,
                    latest: texture_of(device, wanted, "engine: a buffer that lasts (latest)"),
                    older: texture_of(device, wanted, "engine: a buffer that lasts (older)"),
                });
            }
        }
        let mut base = Vec::with_capacity(wanted.len());
        let mut target = Vec::with_capacity(wanted.len());
        let mut to_clear = Vec::new();
        for buffer in &state.buffers {
            match buffer {
                None => {
                    base.push(None);
                    target.push(None);
                }
                Some(buffer) => {
                    let (read, written) = if continuation == Continuation::GoOn {
                        (&buffer.latest, &buffer.older)
                    } else {
                        (&buffer.older, &buffer.latest)
                    };
                    if continuation == Continuation::StartClear {
                        to_clear.push(view_of(&buffer.older));
                    }
                    base.push(Some(view_of(read)));
                    target.push(Some(view_of(written)));
                }
            }
        }
        BuffersOfACall {
            continuation,
            base,
            target,
            to_clear,
        }
    }

    /// The frame `frame` was made with the state `id`, as [`Self::prepare`] said: from now on the state is of that frame.
    pub fn commit(&mut self, id: u64, frame: u64, continuation: Continuation) {
        let Some(state) = self.states.get_mut(&id) else { return };
        if continuation == Continuation::GoOn {
            for buffer in state.buffers.iter_mut().flatten() {
                std::mem::swap(&mut buffer.latest, &mut buffer.older);
            }
        }
        state.frame = Some(frame);
    }

    /// The frame was not made: the state is let go of, and its effect starts clear at its next frame.
    pub fn abandon(&mut self, id: u64) {
        self.states.remove(&id);
    }

    /// A frame has been made: the states that have not been used for a long time are let go of, and so, the one used longest ago first, are as many as it takes to be within the
    /// most bytes there may be.
    pub fn end_frame(&mut self) {
        self.end_frame_within(MOST_BYTES_OF_STATES);
    }

    /// The same, within `most_bytes` and not the most there may be.
    pub fn end_frame_within(&mut self, most_bytes: u64) {
        self.frames_made += 1;
        let now = self.frames_made;
        self.states
            .retain(|_, state| now.saturating_sub(state.last_used) <= FRAMES_BEFORE_A_STATE_IS_LET_GO_OF);
        while self.bytes() > most_bytes {
            let Some(oldest) = self
                .states
                .iter()
                .min_by_key(|(id, state)| (state.last_used, **id))
                .map(|(id, _)| *id)
            else {
                break;
            };
            self.states.remove(&oldest);
        }
    }
}

#[cfg(test)]
mod tests;
