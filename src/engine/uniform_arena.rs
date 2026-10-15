// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The buffer on the card that the uniforms of all the passes of a frame are written to in one go (see [`super::UniformBytes`], which makes the bytes).

use super::UniformBytes;

/// The room the buffer starts with, in bytes: 64 slots of the usual 64 bytes, at 256 bytes apart this is 16 passes.
const SMALLEST_CAPACITY_IN_BYTES: u64 = 4096;

/// The buffer on the card. It grows (and the old buffer is let go of) when a frame needs more room than it has.
#[derive(Default)]
pub struct UniformArena {
    buffer: Option<wgpu::Buffer>,
    capacity_in_bytes: u64,
}

impl UniformArena {
    /// Writes `uniforms` to the buffer, making it bigger first if need be, and gives the buffer. `None` if there are no uniforms at all.
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        uniforms: &UniformBytes,
    ) -> Option<&wgpu::Buffer> {
        if uniforms.is_empty() {
            return None;
        }
        let needed = uniforms.len() as u64;
        if self.buffer.is_none() || needed > self.capacity_in_bytes {
            self.capacity_in_bytes = needed.next_power_of_two().max(SMALLEST_CAPACITY_IN_BYTES);
            self.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("engine uniforms of a frame"),
                size: self.capacity_in_bytes,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        let buffer = self
            .buffer
            .as_ref()
            .expect("the buffer was just made if there was none");
        queue.write_buffer(buffer, 0, uniforms.as_bytes());
        Some(buffer)
    }

    /// How many bytes the buffer has room for.
    #[cfg(test)]
    pub fn capacity_in_bytes(&self) -> u64 {
        self.capacity_in_bytes
    }
}
