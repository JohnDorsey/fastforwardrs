// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Getting a finished picture back from the graphics card as bytes.
//!
//! The picture is copied into a buffer that the CPU can read, in the same batch of work as the drawing; then the CPU waits (not for longer than
//! it is told to) for the card to be done, and takes the bytes. The card wants the rows of the copy to start at multiples of 256 bytes, so the
//! rows are padded and have to be taken out of the padding.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use thiserror::Error;

use super::recycler::Recycler;

/// How many bytes a row of a copy from a texture takes up when each row has `row_bytes` bytes of picture in it.
pub fn padded_bytes_per_row(row_bytes: u32) -> u32 {
    row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
}

/// The rows of a copy with the padding taken out: `rows` rows of `row_bytes` bytes each, from rows that are `padded_row_bytes` apart.
pub fn without_padding(padded: &[u8], padded_row_bytes: usize, row_bytes: usize, rows: usize) -> Vec<u8> {
    let mut unpadded = Vec::with_capacity(row_bytes * rows);
    for row in 0..rows {
        let start = row * padded_row_bytes;
        unpadded.extend_from_slice(&padded[start..start + row_bytes]);
    }
    unpadded
}

#[derive(Debug, Error)]
pub enum ReadbackProblem {
    #[error("the graphics card did not finish in time")]
    TimedOut,
    #[error("{0}")]
    Failed(String),
}

/// A copy of a texture that has been asked for, and has not been taken yet.
pub struct PendingReadback {
    buffer: wgpu::Buffer,
    size_in_bytes: u64,
    padded_row_bytes: u32,
    row_bytes: u32,
    rows: u32,
}

/// The buffers that pictures are copied into, kept from one frame to the next.
#[derive(Default)]
pub struct ReadbackBuffers {
    idle: Recycler<u64, wgpu::Buffer>,
}

impl ReadbackBuffers {
    /// Asks for `texture` (of a format with four bytes to the pixel, which is all the engine reads back) to be copied to a buffer, as part of the
    /// work that `encoder` is making. Give the result to [`Self::collect`] once the work has been submitted.
    pub fn record_copy(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
    ) -> PendingReadback {
        let row_bytes = texture.width() * 4;
        let padded_row_bytes = padded_bytes_per_row(row_bytes);
        let rows = texture.height();
        let size_in_bytes = u64::from(padded_row_bytes) * u64::from(rows);
        let buffer = self.idle.take(&size_in_bytes).unwrap_or_else(|| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("engine read-back buffer"),
                size: size_in_bytes,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            })
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row_bytes),
                    rows_per_image: Some(rows),
                },
            },
            wgpu::Extent3d {
                width: texture.width(),
                height: rows,
                depth_or_array_layers: 1,
            },
        );
        PendingReadback {
            buffer,
            size_in_bytes,
            padded_row_bytes,
            row_bytes,
            rows,
        }
    }

    /// The pixels of a copy that was asked for and whose work has been submitted, once the card has done it: waits until `deadline` for that.
    pub fn collect(
        &mut self,
        device: &wgpu::Device,
        pending: PendingReadback,
        deadline: Instant,
    ) -> Result<Vec<u8>, ReadbackProblem> {
        let slice = pending.buffer.slice(..);
        let (sender, receiver) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            // (Nobody is listening if the wait was given up, and that is all right.)
            let _ = sender.send(result);
        });
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(remaining),
            }) {
                Ok(_) => {}
                Err(wgpu::PollError::Timeout) => return Err(ReadbackProblem::TimedOut),
                Err(other) => return Err(ReadbackProblem::Failed(other.to_string())),
            }
            match receiver.try_recv() {
                Ok(Ok(())) => break,
                Ok(Err(error)) => return Err(ReadbackProblem::Failed(error.to_string())),
                Err(mpsc::TryRecvError::Empty) => {
                    if Instant::now() >= deadline {
                        return Err(ReadbackProblem::TimedOut);
                    }
                    std::thread::sleep(Duration::from_micros(200));
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(ReadbackProblem::Failed(
                        "the card stopped before it gave the pixels".to_owned(),
                    ));
                }
            }
        }
        let pixels = {
            let view = slice
                .get_mapped_range()
                .map_err(|error| ReadbackProblem::Failed(error.to_string()))?;
            without_padding(
                &view,
                pending.padded_row_bytes as usize,
                pending.row_bytes as usize,
                pending.rows as usize,
            )
        };
        pending.buffer.unmap();
        self.idle.give(pending.size_in_bytes, pending.buffer);
        Ok(pixels)
    }

    /// Lets go of the buffers that are waiting, except the `most_kept` that were given back last.
    pub fn end_frame(&mut self, most_kept: usize) {
        // (Every buffer counts as 1 here, so the budget is how many are kept.)
        self.idle.trim(most_kept as u64, |_| 1);
    }

    /// How many buffers are waiting.
    #[cfg(test)]
    pub fn idle_count(&self) -> usize {
        self.idle.idle_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_padded_to_multiples_of_256_bytes() {
        assert_eq!(padded_bytes_per_row(4), 256);
        assert_eq!(padded_bytes_per_row(256), 256);
        assert_eq!(padded_bytes_per_row(257), 512);
        assert_eq!(padded_bytes_per_row(1280 * 4), 5120);
        assert_eq!(padded_bytes_per_row(1000 * 4), 4096);
    }

    #[test]
    fn the_padding_is_taken_out_of_every_row_and_only_the_padding() {
        // Three rows of 5 bytes each, 8 bytes apart: the bytes 100.. are padding.
        let mut padded = Vec::new();
        for row in 0..3u8 {
            padded.extend((0..5).map(|byte| row * 10 + byte));
            padded.extend([100, 101, 102]);
        }
        assert_eq!(
            without_padding(&padded, 8, 5, 3),
            vec![0, 1, 2, 3, 4, 10, 11, 12, 13, 14, 20, 21, 22, 23, 24]
        );
    }

    #[test]
    fn rows_that_need_no_padding_come_out_as_they_went_in() {
        let bytes: Vec<u8> = (0..24).collect();
        assert_eq!(without_padding(&bytes, 8, 8, 3), bytes);
    }

    #[test]
    fn the_last_row_does_not_need_the_padding_after_it() {
        // The buffer of a copy has padding after every row but the last one may stop at the end of the picture's bytes.
        let bytes: Vec<u8> = (0..13).collect();
        assert_eq!(without_padding(&bytes, 8, 5, 2), vec![0, 1, 2, 3, 4, 8, 9, 10, 11, 12]);
    }
}
