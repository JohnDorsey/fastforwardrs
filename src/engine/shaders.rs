// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The engine's own shaders, which are in files of their own (`shaders/*.wgsl`) and are part of the program.

#[cfg(test)]
/// The curves between codes and light, with no entry points: the passes that need them (the four that follow) have them **in front of their own text**, so that what
/// each of them is, to the card, is one text (`concat!` makes it when the program is built, and the tests that read a shader read the same text).
pub const COLOUR_CURVES: &str = include_str!("shaders/colour.wgsl");

/// The last pass of a frame: the finished picture as 8-bit.
pub const OUTPUT: &str = concat!(
    include_str!("shaders/colour.wgsl"),
    "
",
    include_str!("shaders/output.wgsl")
);

/// What is done once to a picture that was sent as bytes: it is made light, premultiplied and half-float.
pub const INGEST: &str = concat!(
    include_str!("shaders/colour.wgsl"),
    "
",
    include_str!("shaders/ingest.wgsl")
);

/// What is done once to a video picture that was sent in planes: it is made red, green and blue, and light.
pub const YUV: &str = concat!(
    include_str!("shaders/colour.wgsl"),
    "
",
    include_str!("shaders/yuv.wgsl")
);

/// The effect that mixes the colours of a layer's picture with a matrix.
pub const COLOUR_MATRIX: &str = concat!(
    include_str!("shaders/colour.wgsl"),
    "
",
    include_str!("shaders/colour_matrix.wgsl")
);

/// Putting a layer's picture in its place on the frame.
pub const PLACE: &str = include_str!("shaders/place.wgsl");

/// The export's pass that makes the brightness plane of a finished frame.
pub const RGB_TO_LUMA: &str = include_str!("shaders/rgb_to_luma.wgsl");

/// The export's pass that makes the two colour difference planes of a finished frame.
pub const RGB_TO_CHROMA: &str = include_str!("shaders/rgb_to_chroma.wgsl");

/// The vertex stage of the effects that others wrote (their shaders only have a fragment stage): one triangle that covers all of the picture.
pub const FULL_SCREEN_TRIANGLE: &str = include_str!("shaders/full_screen_triangle.wgsl");

/// Every shader of the engine, with the name of its file.
#[cfg(test)]
pub const ALL: &[(&str, &str)] = &[
    ("output.wgsl", OUTPUT),
    ("ingest.wgsl", INGEST),
    ("yuv.wgsl", YUV),
    ("colour_matrix.wgsl", COLOUR_MATRIX),
    ("place.wgsl", PLACE),
    ("rgb_to_luma.wgsl", RGB_TO_LUMA),
    ("rgb_to_chroma.wgsl", RGB_TO_CHROMA),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// A mistake in a shader would otherwise only be found when the first frame is drawn, on a computer that has a graphics card (and not at all on a
    /// computer that has none to run the tests on). naga is what wgpu checks shaders with, so a shader it accepts here is one wgpu accepts.
    #[test]
    fn every_shader_of_the_engine_parses_and_passes_validation() {
        for (name, source) in ALL {
            let module = naga::front::wgsl::parse_str(source)
                .unwrap_or_else(|error| panic!("{name}: {}", error.emit_to_string(source)));
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
                .validate(&module)
                .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        }
    }

    #[test]
    fn the_vertex_stage_of_the_effects_that_others_wrote_reads_and_passes_validation_and_is_a_vertex_stage_and_no_more()
    {
        let module = naga::front::wgsl::parse_str(FULL_SCREEN_TRIANGLE)
            .unwrap_or_else(|error| panic!("{}", error.emit_to_string(FULL_SCREEN_TRIANGLE)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
            .validate(&module)
            .unwrap();
        let entry_points: Vec<(&str, naga::ShaderStage)> = module
            .entry_points
            .iter()
            .map(|entry| (entry.name.as_str(), entry.stage))
            .collect();
        assert_eq!(entry_points, vec![("vs_main", naga::ShaderStage::Vertex)]);
        assert!(
            module.global_variables.is_empty(),
            "it uses nothing the effect's pass binds"
        );
    }

    #[test]
    fn every_shader_has_its_entry_points_by_the_names_the_pipelines_use() {
        for (name, source) in ALL {
            let module = naga::front::wgsl::parse_str(source).unwrap();
            let entry_points: Vec<(&str, naga::ShaderStage)> = module
                .entry_points
                .iter()
                .map(|entry| (entry.name.as_str(), entry.stage))
                .collect();
            assert!(
                entry_points.contains(&("vs_main", naga::ShaderStage::Vertex)),
                "{name}: {entry_points:?}"
            );
            assert!(
                entry_points.contains(&("fs_main", naga::ShaderStage::Fragment)),
                "{name}: {entry_points:?}"
            );
        }
    }
}
