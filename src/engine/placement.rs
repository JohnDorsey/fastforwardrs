// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The numbers that the pass that puts a layer in its place is given (`shaders/place.wgsl`), made from the layer's description and written in the
//! order and at the places the shader's struct has them.

use super::{LayerDescription, LayerSource, SlotWriter, WorkingSpace};
use crate::colour_space::{StandardPrimaries, TransferFunction, matrix};

/// Where a layer is on a frame and what its picture is multiplied by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacementUniforms {
    /// The four corners, top left first, in pixels of the frame.
    pub corners_in_pixels: [(f32, f32); 4],
    /// What every pixel of the layer's picture is multiplied by (premultiplied).
    pub tint: [f32; 4],
    pub frame_size_in_pixels: (u32, u32),
}

impl PlacementUniforms {
    /// The numbers for `layer` on a frame of `frame_size_in_pixels`: each corner is its fractions of the frame times the frame's size, **worked out
    /// the way the window works out the corners of the same layer** (a fraction times a size in `f32`), which is what makes the same pixels be covered.
    pub fn of_layer(layer: &LayerDescription, frame_size_in_pixels: (u32, u32)) -> Self {
        let (width, height) = (frame_size_in_pixels.0 as f32, frame_size_in_pixels.1 as f32);
        Self {
            corners_in_pixels: layer.corners.map(|(across, down)| (across * width, down * height)),
            tint: match &layer.source {
                // (A group's picture is a picture: it is made of the layers of the group and is drawn as it is.)
                LayerSource::Picture(_) | LayerSource::Group(_) => [1.0, 1.0, 1.0, 1.0],
                LayerSource::Solid(rgba) => premultiplied_tint(*rgba),
            },
            frame_size_in_pixels,
        }
    }

    /// Like [`Self::of_layer`], for a frame that is put together in `working`: **the colour of a layer that is one colour is made the numbers of the working space**
    /// (its sRGB codes are decoded to light, in the primaries of the working space, and then multiplied by how opaque the layer is), as a picture's are when it is sent.
    /// In the compatibility mode it is [`Self::of_layer`].
    pub fn of_layer_in(layer: &LayerDescription, frame_size_in_pixels: (u32, u32), working: WorkingSpace) -> Self {
        let mut uniforms = Self::of_layer(layer, frame_size_in_pixels);
        if let (LayerSource::Solid(rgba), WorkingSpace::Linear(primaries)) = (&layer.source, working) {
            uniforms.tint = premultiplied_tint_in_light(*rgba, primaries);
        }
        uniforms
    }

    /// Writes the numbers as the shader's struct has them: the first two corners, the last two, the tint, the size of the frame.
    pub fn write(&self, writer: &mut SlotWriter) {
        let [top_left, top_right, bottom_right, bottom_left] = self.corners_in_pixels;
        writer
            .vec4([top_left.0, top_left.1, top_right.0, top_right.1])
            .vec4([bottom_right.0, bottom_right.1, bottom_left.0, bottom_left.1])
            .vec4(self.tint)
            .vec2([self.frame_size_in_pixels.0 as f32, self.frame_size_in_pixels.1 as f32]);
    }
}

/// What a layer that is one colour multiplies its one white pixel by: the colour, premultiplied by how opaque it is.
pub fn premultiplied_tint([red, green, blue, opacity]: [u8; 4]) -> [f32; 4] {
    let opacity = f32::from(opacity) / 255.0;
    [
        f32::from(red) / 255.0 * opacity,
        f32::from(green) / 255.0 * opacity,
        f32::from(blue) / 255.0 * opacity,
        opacity,
    ]
}

/// What a layer that is one colour multiplies its one white pixel by in a linear working space: the sRGB codes of the colour made light, in the primaries of the working
/// space, and premultiplied by how opaque it is. (Worked out in `f64` with the reference curve and matrices.)
pub fn premultiplied_tint_in_light([red, green, blue, opacity]: [u8; 4], working: StandardPrimaries) -> [f32; 4] {
    let opacity = f64::from(opacity) / 255.0;
    let light = [red, green, blue].map(|code| TransferFunction::Srgb.decode_to_light(f64::from(code) / 255.0));
    let in_working_space = matrix::apply(&StandardPrimaries::Rec709.conversion_to(working), light);
    [
        (in_working_space[0] * opacity) as f32,
        (in_working_space[1] * opacity) as f32,
        (in_working_space[2] * opacity) as f32,
        opacity as f32,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::pipelines::PLACEMENT_UNIFORM_BYTES;
    use crate::engine::{CORNERS_OF_THE_WHOLE_PICTURE, PictureId, UniformBytes, shaders};

    fn number_at(bytes: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    /// The members of the uniform struct `variable` of the shader `source`, with where naga says each is, and how big the struct is.
    fn members_of_the_uniform(source: &str, variable: &str) -> (Vec<(String, u32)>, u32) {
        let module = naga::front::wgsl::parse_str(source).unwrap();
        let (_, global) = module
            .global_variables
            .iter()
            .find(|(_, global)| global.name.as_deref() == Some(variable))
            .unwrap_or_else(|| panic!("no variable called {variable}"));
        assert_eq!(global.space, naga::AddressSpace::Uniform);
        let naga::TypeInner::Struct { members, span } = &module.types[global.ty].inner else {
            panic!("{variable} is not a struct")
        };
        (
            members
                .iter()
                .map(|member| (member.name.clone().unwrap(), member.offset))
                .collect(),
            *span,
        )
    }

    #[test]
    fn the_shader_has_the_members_where_the_writer_puts_the_numbers_and_the_struct_is_as_big_as_the_pipeline_says() {
        let (members, size) = members_of_the_uniform(shaders::PLACE, "placement");
        assert_eq!(
            members,
            vec![
                ("corners_top".to_owned(), 0),
                ("corners_bottom".to_owned(), 16),
                ("tint".to_owned(), 32),
                ("frame_size".to_owned(), 48),
            ]
        );
        assert_eq!(u64::from(size), PLACEMENT_UNIFORM_BYTES);

        let uniforms = PlacementUniforms {
            corners_in_pixels: [(1.0, 2.0), (3.0, 4.0), (5.0, 6.0), (7.0, 8.0)],
            tint: [9.0, 10.0, 11.0, 12.0],
            frame_size_in_pixels: (13, 14),
        };
        let mut bytes = UniformBytes::new(256);
        let slot = bytes.add_slot(|writer| uniforms.write(writer));
        assert_eq!(
            u64::from(slot.size),
            PLACEMENT_UNIFORM_BYTES,
            "the slot is the size of the struct, which the pipeline's binding says"
        );
        let bytes = bytes.as_bytes();
        let offset_of = |name: &str| members.iter().find(|(member, _)| member == name).unwrap().1 as usize;
        let at = |name: &str| {
            (0..4)
                .map(|k| number_at(bytes, offset_of(name) + 4 * k))
                .collect::<Vec<f32>>()
        };
        assert_eq!(at("corners_top"), vec![1.0, 2.0, 3.0, 4.0], "top left, then top right");
        assert_eq!(
            at("corners_bottom"),
            vec![5.0, 6.0, 7.0, 8.0],
            "bottom right, then bottom left"
        );
        assert_eq!(at("tint"), vec![9.0, 10.0, 11.0, 12.0]);
        assert_eq!(&at("frame_size")[..2], &[13.0, 14.0]);
    }

    #[test]
    fn the_corners_are_fractions_times_the_size_of_the_frame_and_a_picture_is_multiplied_by_white() {
        let layer = LayerDescription {
            source: LayerSource::Picture(PictureId(1)),
            corners: [(0.25, 0.5), (0.75, 0.5), (0.75, 1.0), (0.25, 1.0)],
            effects: Vec::new(),
        };
        let uniforms = PlacementUniforms::of_layer(&layer, (200, 100));
        assert_eq!(
            uniforms.corners_in_pixels,
            [(50.0, 50.0), (150.0, 50.0), (150.0, 100.0), (50.0, 100.0)]
        );
        assert_eq!(uniforms.tint, [1.0; 4]);
        assert_eq!(uniforms.frame_size_in_pixels, (200, 100));
    }

    #[test]
    fn a_layer_that_is_a_colour_is_tinted_with_the_colour_premultiplied() {
        let layer = LayerDescription {
            source: LayerSource::Solid([255, 0, 51, 255]),
            corners: CORNERS_OF_THE_WHOLE_PICTURE,
            effects: Vec::new(),
        };
        assert_eq!(PlacementUniforms::of_layer(&layer, (10, 10)).tint, [1.0, 0.0, 0.2, 1.0]);
    }

    #[test]
    fn a_translucent_colour_is_multiplied_by_how_opaque_it_is_and_a_clear_one_is_nothing() {
        assert_eq!(premultiplied_tint([255, 255, 255, 0]), [0.0, 0.0, 0.0, 0.0]);
        let half = premultiplied_tint([200, 100, 0, 51]);
        assert!((half[3] - 0.2).abs() < 1e-6);
        assert!((half[0] - 200.0 / 255.0 * 0.2).abs() < 1e-6);
        assert!((half[1] - 100.0 / 255.0 * 0.2).abs() < 1e-6);
        assert_eq!(half[2], 0.0);
    }
}
