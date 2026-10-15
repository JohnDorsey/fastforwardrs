// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The textures that the passes of a frame draw into and read from, kept from one frame to the next so that they are not made again for each.
//!
//! A frame needs a few textures of the size of the frame (the picture being put together, the finished picture) and of the size of each layer's
//! picture (what an effect makes of it). They are asked for by their *shape* (size, format, what they are used for), used for the frame and given
//! back, and the next frame that wants the same shape gets the same texture.

use super::recycler::Recycler;

/// What kind of texture: two textures of the same shape can stand in for each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureShape {
    pub width_in_pixels: u32,
    pub height_in_pixels: u32,
    pub format: wgpu::TextureFormat,
    pub usage: wgpu::TextureUsages,
}

impl TextureShape {
    /// How many bytes a texture of this shape takes up on the card (not counting what the driver adds).
    pub fn byte_count(&self) -> u64 {
        let bytes_per_pixel = self.format.block_copy_size(None).unwrap_or(4);
        u64::from(self.width_in_pixels) * u64::from(self.height_in_pixels) * u64::from(bytes_per_pixel)
    }
}

/// How the pool has been used so far.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PoolStatistics {
    /// How many textures were made.
    pub made: u64,
    /// How many times a texture that was given back was used again instead of making one.
    pub reused: u64,
}

/// See the module documentation.
#[derive(Default)]
pub struct TexturePool {
    recycler: Recycler<TextureShape, wgpu::Texture>,
    statistics: PoolStatistics,
}

impl TexturePool {
    /// A texture of this shape: one that was given back, or a new one.
    pub fn acquire(&mut self, device: &wgpu::Device, shape: TextureShape) -> wgpu::Texture {
        if let Some(texture) = self.recycler.take(&shape) {
            self.statistics.reused += 1;
            return texture;
        }
        self.statistics.made += 1;
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("engine texture of the pool"),
            size: wgpu::Extent3d {
                width: shape.width_in_pixels,
                height: shape.height_in_pixels,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: shape.format,
            usage: shape.usage,
            view_formats: &[],
        })
    }

    /// Gives back a texture of this shape that is not used any more.
    pub fn release(&mut self, shape: TextureShape, texture: wgpu::Texture) {
        self.recycler.give(shape, texture);
    }

    /// Lets go of the textures that were not used for the longest time until the idle ones take up no more than `idle_budget_in_bytes`.
    pub fn end_frame(&mut self, idle_budget_in_bytes: u64) {
        self.recycler.trim(idle_budget_in_bytes, TextureShape::byte_count);
    }

    pub fn statistics(&self) -> PoolStatistics {
        self.statistics
    }

    /// How many textures are given back and waiting.
    pub fn idle_count(&self) -> usize {
        self.recycler.idle_count()
    }

    /// Lets go of every texture that is waiting (the device is going away, or memory is short).
    pub fn clear(&mut self) {
        self.recycler.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_texture_takes_up_as_many_bytes_as_its_pixels_come_to() {
        let shape = |format| TextureShape {
            width_in_pixels: 100,
            height_in_pixels: 10,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
        };
        assert_eq!(shape(wgpu::TextureFormat::Rgba8Unorm).byte_count(), 4000);
        assert_eq!(shape(wgpu::TextureFormat::Rgba16Float).byte_count(), 8000);
        assert_eq!(shape(wgpu::TextureFormat::R8Unorm).byte_count(), 1000);
    }
}
