// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The numbers that the passes of a frame are given, as bytes in the layout the shaders read them in.
//!
//! **All the passes of a frame share one buffer**, each with a slot of its own at an offset that is a multiple of the card's alignment, and the
//! pass is told which slot with a *dynamic offset*. (On the software adapter of Windows, WARP, wgpu gives two draws that use different statically bound
//! uniform buffers the data of the last one; dynamic offsets are right there. See `docs/plugin-system-spike-findings.md`.) This file is the bytes
//! half of that, with no graphics card in it, so that it can be tested alone; the buffer on the card is in `uniform_arena`.
//!
//! The numbers of a slot are written in the order the shader's struct has them, by the type, and each is put where WGSL's rules for the `uniform`
//! address space put it: an `f32` at a multiple of 4, a `vec2<f32>` of 8, a `vec3<f32>` and a `vec4<f32>` of 16, a `mat3x3<f32>` as three columns of
//! 16 bytes. The layout of each struct is checked against what naga makes of the shader (see the tests of the passes).

/// The bytes of all the slots of a frame, one after another.
#[derive(Debug)]
pub struct UniformBytes {
    bytes: Vec<u8>,
    alignment: usize,
}

impl UniformBytes {
    /// A buffer whose slots start at multiples of `alignment`, which must be a power of two (it is the card's
    /// `min_uniform_buffer_offset_alignment`, 256 on every card the engine has met).
    pub fn new(alignment: usize) -> Self {
        assert!(alignment.is_power_of_two(), "{alignment} is not a power of two");
        Self {
            bytes: Vec::new(),
            alignment,
        }
    }

    /// Forgets every slot, to begin a new frame (what was allocated is kept).
    pub fn clear(&mut self) {
        self.bytes.clear();
    }

    /// Adds a slot, written by `fill`, and says where it starts and how many bytes of it are numbers (the binding of a pass says that, so that the
    /// card does not read past them).
    pub fn add_slot(&mut self, fill: impl FnOnce(&mut SlotWriter)) -> UniformSlot {
        let offset = self.bytes.len().next_multiple_of(self.alignment);
        self.bytes.resize(offset, 0);
        let mut writer = SlotWriter { bytes: Vec::new() };
        fill(&mut writer);
        let size = writer.bytes.len().next_multiple_of(16).max(16);
        writer.bytes.resize(size, 0);
        self.bytes.extend_from_slice(&writer.bytes);
        UniformSlot {
            offset: u32::try_from(offset).expect("the uniforms of a frame are far less than 4 GiB"),
            size: u32::try_from(size).expect("a slot is far less than 4 GiB"),
        }
    }

    /// Everything that has been added, to be written to the card in one go.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// How many bytes the slots take up with the room between them.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether there is no slot.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

/// Where a slot is in the buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UniformSlot {
    /// The dynamic offset of the slot: a multiple of the alignment.
    pub offset: u32,
    /// How many bytes of the slot are the numbers of the struct (a multiple of 16).
    pub size: u32,
}

/// Writes the numbers of one slot.
#[derive(Debug)]
pub struct SlotWriter {
    bytes: Vec<u8>,
}

impl SlotWriter {
    fn align_to(&mut self, alignment: usize) {
        let aligned = self.bytes.len().next_multiple_of(alignment);
        self.bytes.resize(aligned, 0);
    }

    /// A `f32`.
    pub fn f32(&mut self, value: f32) -> &mut Self {
        self.align_to(4);
        self.bytes.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// A `u32`.
    pub fn u32(&mut self, value: u32) -> &mut Self {
        self.align_to(4);
        self.bytes.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// An `i32`.
    pub fn i32(&mut self, value: i32) -> &mut Self {
        self.align_to(4);
        self.bytes.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// A `vec2<f32>`.
    pub fn vec2(&mut self, value: [f32; 2]) -> &mut Self {
        self.align_to(8);
        for number in value {
            self.bytes.extend_from_slice(&number.to_le_bytes());
        }
        self
    }

    /// A `vec3<f32>` (which takes 12 bytes, and the next member of a struct may use the last 4 of its 16 if it is a four-byte one).
    pub fn vec3(&mut self, value: [f32; 3]) -> &mut Self {
        self.align_to(16);
        for number in value {
            self.bytes.extend_from_slice(&number.to_le_bytes());
        }
        self
    }

    /// A `vec4<f32>`.
    pub fn vec4(&mut self, value: [f32; 4]) -> &mut Self {
        self.align_to(16);
        for number in value {
            self.bytes.extend_from_slice(&number.to_le_bytes());
        }
        self
    }

    /// A `mat3x3<f32>` from its rows as people write them (the red row of a colour matrix first); WGSL's matrices are lists of columns, so the
    /// numbers are written by columns, each column in 16 bytes.
    pub fn mat3x3_from_rows(&mut self, rows: [[f32; 3]; 3]) -> &mut Self {
        self.align_to(16);
        for column in 0..3 {
            for row in rows {
                self.bytes.extend_from_slice(&row[column].to_le_bytes());
            }
            self.bytes.extend_from_slice(&0.0_f32.to_le_bytes());
        }
        self
    }

    /// How many bytes have been written so far.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether nothing has been written yet.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn number_at(bytes: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    #[test]
    fn the_numbers_of_a_slot_go_where_wgsl_puts_the_members_of_a_struct() {
        let mut uniforms = UniformBytes::new(256);
        // struct { a: f32, b: vec2<f32>, c: f32, d: vec4<f32>, e: u32, f: i32, g: vec3<f32>, h: f32 }
        let slot = uniforms.add_slot(|writer| {
            writer
                .f32(1.0)
                .vec2([2.0, 3.0])
                .f32(4.0)
                .vec4([5.0, 6.0, 7.0, 8.0])
                .u32(9)
                .i32(-10)
                .vec3([11.0, 12.0, 13.0])
                .f32(14.0);
        });
        let bytes = uniforms.as_bytes();
        assert_eq!(slot.offset, 0);
        assert_eq!(number_at(bytes, 0), 1.0, "a: f32 at 0");
        assert_eq!(
            (number_at(bytes, 8), number_at(bytes, 12)),
            (2.0, 3.0),
            "b: vec2 at 8 (not 4)"
        );
        assert_eq!(number_at(bytes, 16), 4.0, "c: f32 at 16");
        assert_eq!(
            [
                number_at(bytes, 32),
                number_at(bytes, 36),
                number_at(bytes, 40),
                number_at(bytes, 44)
            ],
            [5.0, 6.0, 7.0, 8.0],
            "d: vec4 at 32 (not 20)"
        );
        assert_eq!(u32::from_le_bytes(bytes[48..52].try_into().unwrap()), 9, "e: u32 at 48");
        assert_eq!(
            i32::from_le_bytes(bytes[52..56].try_into().unwrap()),
            -10,
            "f: i32 at 52"
        );
        assert_eq!(
            [number_at(bytes, 64), number_at(bytes, 68), number_at(bytes, 72)],
            [11.0, 12.0, 13.0],
            "g: vec3 at 64 (not 56)"
        );
        assert_eq!(
            number_at(bytes, 76),
            14.0,
            "h: f32 in the room that the vec3 leaves, at 76"
        );
        assert_eq!(slot.size, 80);
    }

    #[test]
    fn a_matrix_is_written_by_columns_of_sixteen_bytes() {
        let mut uniforms = UniformBytes::new(256);
        let slot = uniforms.add_slot(|writer| {
            writer.mat3x3_from_rows([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]);
        });
        let bytes = uniforms.as_bytes();
        // The first column is the first number of each row.
        assert_eq!(
            [number_at(bytes, 0), number_at(bytes, 4), number_at(bytes, 8)],
            [1.0, 4.0, 7.0]
        );
        assert_eq!(
            [number_at(bytes, 16), number_at(bytes, 20), number_at(bytes, 24)],
            [2.0, 5.0, 8.0]
        );
        assert_eq!(
            [number_at(bytes, 32), number_at(bytes, 36), number_at(bytes, 40)],
            [3.0, 6.0, 9.0]
        );
        assert_eq!(slot.size, 48);
    }

    #[test]
    fn every_slot_starts_at_a_multiple_of_the_alignment_and_the_slots_do_not_overlap() {
        let mut uniforms = UniformBytes::new(256);
        let first = uniforms.add_slot(|writer| {
            writer.f32(1.0);
        });
        let second = uniforms.add_slot(|writer| {
            writer.vec4([2.0; 4]).vec4([3.0; 4]);
        });
        let third = uniforms.add_slot(|writer| {
            writer.f32(4.0);
        });
        assert_eq!((first.offset, second.offset, third.offset), (0, 256, 512));
        assert_eq!((first.size, second.size, third.size), (16, 32, 16));
        assert_eq!(number_at(uniforms.as_bytes(), 0), 1.0);
        assert_eq!(number_at(uniforms.as_bytes(), 256), 2.0);
        assert_eq!(number_at(uniforms.as_bytes(), 256 + 16), 3.0);
        assert_eq!(number_at(uniforms.as_bytes(), 512), 4.0);
        assert_eq!(uniforms.len(), 512 + 16);
    }

    #[test]
    fn a_smaller_alignment_packs_the_slots_closer() {
        let mut uniforms = UniformBytes::new(64);
        let first = uniforms.add_slot(|writer| {
            writer.f32(1.0);
        });
        let second = uniforms.add_slot(|writer| {
            writer.f32(2.0);
        });
        assert_eq!((first.offset, second.offset), (0, 64));
    }

    #[test]
    fn an_empty_slot_still_has_room_for_one_vector_so_that_it_can_be_bound() {
        let mut uniforms = UniformBytes::new(256);
        let slot = uniforms.add_slot(|_| {});
        assert_eq!(slot.size, 16);
        assert_eq!(uniforms.len(), 16);
    }

    #[test]
    fn clearing_begins_a_new_frame_from_the_start() {
        let mut uniforms = UniformBytes::new(256);
        uniforms.add_slot(|writer| {
            writer.f32(1.0);
        });
        uniforms.add_slot(|writer| {
            writer.f32(2.0);
        });
        assert!(!uniforms.is_empty());
        uniforms.clear();
        assert!(uniforms.is_empty());
        let slot = uniforms.add_slot(|writer| {
            writer.f32(3.0);
        });
        assert_eq!(slot.offset, 0);
        assert_eq!(number_at(uniforms.as_bytes(), 0), 3.0);
    }

    #[test]
    #[should_panic(expected = "not a power of two")]
    fn an_alignment_that_is_not_a_power_of_two_is_refused() {
        let _ = UniformBytes::new(100);
    }
}
