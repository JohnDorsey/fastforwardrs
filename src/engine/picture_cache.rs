// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The pictures that the engine has been sent, as they are kept on the graphics card: each one is a premultiplied half-float texture, ready to be
//! put in its place on a frame. Which are kept is decided by how recently each was used and how much room they take (see [`super::byte_lru`]).

use super::byte_lru::ByteLru;
use super::{PictureId, WorkingSpace};

/// How many bytes of pictures are kept on the card.
pub const DEFAULT_BUDGET_IN_BYTES: usize = 512 * 1024 * 1024;

/// A picture on the card.
pub struct GpuPicture {
    /// (A view keeps its texture alive, but nothing here should depend on that: the picture is the texture.) When a picture is let go of, its texture is what the
    /// renderer keeps to make the next picture in.
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width_in_pixels: u32,
    pub height_in_pixels: u32,
    /// The working space that the numbers of the texture are in: a frame that is put together in another one cannot use the picture.
    pub working: WorkingSpace,
}

/// See the module documentation.
pub struct PictureCache {
    pictures: ByteLru<PictureId, GpuPicture>,
}

impl PictureCache {
    pub fn new(budget_in_bytes: usize) -> Self {
        Self {
            pictures: ByteLru::new(None, budget_in_bytes),
        }
    }

    /// Keeps a picture, which takes up `size_in_bytes` on the card, and lets go of the ones used least recently while there is not room for all. Gives back the
    /// pictures that were let go of (the one that was kept under the same id, if there was one, and the new one itself if it is too big to keep).
    pub fn insert(&mut self, id: PictureId, picture: GpuPicture, size_in_bytes: usize) -> Vec<GpuPicture> {
        self.pictures
            .insert_giving_back_what_is_let_go(id, picture, size_in_bytes)
            .1
    }

    /// Counts these pictures as used just now (the ones that a frame is about to use are not the ones to let go of).
    pub fn touch(&mut self, ids: impl IntoIterator<Item = PictureId>) {
        for id in ids {
            self.pictures.touch(&id);
        }
    }

    pub fn get(&self, id: PictureId) -> Option<&GpuPicture> {
        self.pictures.peek(&id)
    }

    /// Of these pictures, the ones that are not kept, each once, in the order they were asked about.
    pub fn missing(&self, ids: impl IntoIterator<Item = PictureId>) -> Vec<PictureId> {
        let mut missing = Vec::new();
        for id in ids {
            if !self.pictures.contains(&id) && !missing.contains(&id) {
                missing.push(id);
            }
        }
        missing
    }

    /// Lets go of these pictures, and gives them back.
    pub fn forget(&mut self, ids: impl IntoIterator<Item = PictureId>) -> Vec<GpuPicture> {
        ids.into_iter().filter_map(|id| self.pictures.remove(&id)).collect()
    }

    pub fn len(&self) -> usize {
        self.pictures.len()
    }

    pub fn byte_count(&self) -> usize {
        self.pictures.byte_count()
    }
}
