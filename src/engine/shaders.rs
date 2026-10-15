// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The engine's own shaders, which are in files of their own (`shaders/*.wgsl`) and are part of the program.

/// The last pass of a frame: the finished picture as 8-bit.
pub const OUTPUT: &str = include_str!("shaders/output.wgsl");

/// What is done once to a picture that was sent as bytes: it is premultiplied and made half-float.
pub const INGEST: &str = include_str!("shaders/ingest.wgsl");

/// Putting a layer's picture in its place on the frame.
pub const PLACE: &str = include_str!("shaders/place.wgsl");

/// Every shader of the engine, with the name of its file.
#[cfg(test)]
pub const ALL: &[(&str, &str)] = &[("output.wgsl", OUTPUT), ("ingest.wgsl", INGEST), ("place.wgsl", PLACE)];

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
