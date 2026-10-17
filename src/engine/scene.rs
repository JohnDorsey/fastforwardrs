// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the engine is asked to draw: a **self-contained, serialisable description of one frame**.
//!
//! It has no pointers into the program's data and no types of the window: sizes, layers (back to front), where each one is, and what is done to
//! each one's picture. Pictures are not in it, only their [`PictureId`]s; they are sent to the engine separately (as `PictureData`). That
//! is what makes the interface *message-shaped* (EN-12): the same description could be written down and sent to another process.
//!
//! A description has a [`fingerprint`](FrameDescription::fingerprint), which is also the key of the cache of finished frames: it is made of all that
//! decides what the frame looks like, so an edit that changes a frame changes its key and nothing has to be thrown away by hand.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use super::colour::FrameColour;

/// The key of the effect that mixes colours with a 3 by 3 matrix: the nine numbers are the rows, one after the other (how much of the red, green
/// and blue the new red is made of, then the new green, then the new blue). The mixed colour is cut off at 0 and 1. **Three more numbers may follow the nine**: offsets
/// that are added to the mixed colour, in the codes of the display-referred domain, before it is cut off (brightness and contrast need them).
pub const COLOUR_MATRIX_KEY: &str = "engine:colour_matrix";

/// The key of the effect that makes the light of a picture brighter or darker by whole and part stops: one number, the stops (the light is multiplied by two to the stops;
/// the opacity is not changed). It is an effect of the scene-linear domain.
pub const EXPOSURE_KEY: &str = "engine:exposure";

/// The key of the effect that makes a picture more or less see-through: one number, from 0 (all see-through) to 1 (as it is), by which the picture, colours and opacity
/// together, is multiplied. It is an effect of the scene-linear domain.
pub const OPACITY_KEY: &str = "engine:opacity";

/// The corners of a layer that is all of the picture: top left, top right, bottom right, bottom left, as fractions of the picture.
pub const CORNERS_OF_THE_WHOLE_PICTURE: [(f32, f32); 4] = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];

/// Which picture: the same id is always the same pixels (a decoded picture's serial number, or a number made from what a text picture is made from),
/// so a picture that the engine has is never sent again, and one that is not the same never has the same id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PictureId(pub u64);

/// What a layer is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LayerSource {
    /// A picture the engine has been sent.
    Picture(PictureId),
    /// One colour all over (red, green, blue and how opaque, not multiplied by it).
    Solid([u8; 4]),
}

/// One thing that is done to the picture of a layer, by the key of the effect and the numbers it is given.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectCall {
    pub key: String,
    pub values: Vec<f32>,
}

impl EffectCall {
    /// The effect that mixes colours with `rows` (see [`COLOUR_MATRIX_KEY`]).
    pub fn colour_matrix(rows: [[f32; 3]; 3]) -> Self {
        Self {
            key: COLOUR_MATRIX_KEY.to_owned(),
            values: rows.into_iter().flatten().collect(),
        }
    }

    /// The effect that mixes colours with `rows` and then adds `offsets` (see [`COLOUR_MATRIX_KEY`]).
    pub fn colour_matrix_with_offsets(rows: [[f32; 3]; 3], offsets: [f32; 3]) -> Self {
        Self {
            key: COLOUR_MATRIX_KEY.to_owned(),
            values: rows.into_iter().flatten().chain(offsets).collect(),
        }
    }

    /// The effect that makes the light of a picture brighter or darker by `stops` (see [`EXPOSURE_KEY`]).
    pub fn exposure(stops: f32) -> Self {
        Self {
            key: EXPOSURE_KEY.to_owned(),
            values: vec![stops],
        }
    }

    /// The effect that makes a picture `opacity` as opaque as it is (see [`OPACITY_KEY`]).
    pub fn opacity(opacity: f32) -> Self {
        Self {
            key: OPACITY_KEY.to_owned(),
            values: vec![opacity],
        }
    }
}

/// A picture, or a colour, somewhere on the frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayerDescription {
    pub source: LayerSource,
    /// Where the layer is: its four corners in the order top left, top right, bottom right, bottom left (of the picture, before it is turned: where they
    /// are once it is turned), as fractions of the frame's width and height. They can be outside the frame.
    pub corners: [(f32, f32); 4],
    /// What is done to the picture of the layer, one after another, **before** it is put in its place.
    pub effects: Vec<EffectCall>,
}

/// One frame of the finished picture: the layers over black.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrameDescription {
    /// How many pixels across and down the frame is made at.
    pub render_size: (u32, u32),
    /// The layers, the one at the back first.
    pub layers: Vec<LayerDescription>,
    /// What the layers are put together in, and what the finished frame is encoded as. **The compatibility mode (codes, nothing converted) when it is not said**, which is
    /// what every description made before phase 3 has, and what the tests of the engine against the old drawing run in.
    #[serde(default)]
    pub colour: FrameColour,
}

impl FrameDescription {
    /// A frame of nothing but black.
    pub fn black(render_size: (u32, u32)) -> Self {
        Self {
            render_size,
            layers: Vec::new(),
            colour: FrameColour::default(),
        }
    }

    /// What tells this description from every other one that looks different: equal descriptions have the same fingerprint, and a difference in
    /// any number, any layer, any key or any size is a different one (as far as a 64-bit hash goes). It is only the same from one run to the next
    /// of the same program, and is not for keeping in a file.
    pub fn fingerprint(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.render_size.hash(&mut hasher);
        self.hash_the_layers(&mut hasher);
        hasher.finish()
    }

    /// Like [`Self::fingerprint`], but not made of the size the frame is made at: it is the same for two descriptions of what is shown that are made at different
    /// sizes, and it is not for two that show something different. (It is what the quality of the preview goes by, which changes the size and not the picture.)
    pub fn fingerprint_of_what_is_shown(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash_the_layers(&mut hasher);
        hasher.finish()
    }

    fn hash_the_layers(&self, hasher: &mut DefaultHasher) {
        // (The colour is a part of what is shown: a frame that is put together in another space or encoded as another is another picture.)
        self.colour.hash(hasher);
        self.layers.len().hash(hasher);
        for layer in &self.layers {
            layer.source.hash(hasher);
            for (across, down) in layer.corners {
                across.to_bits().hash(hasher);
                down.to_bits().hash(hasher);
            }
            layer.effects.len().hash(hasher);
            for effect in &layer.effects {
                effect.key.hash(hasher);
                effect.values.len().hash(hasher);
                for value in &effect.values {
                    value.to_bits().hash(hasher);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(picture: u64) -> LayerDescription {
        LayerDescription {
            source: LayerSource::Picture(PictureId(picture)),
            corners: CORNERS_OF_THE_WHOLE_PICTURE,
            effects: Vec::new(),
        }
    }

    fn description() -> FrameDescription {
        FrameDescription {
            colour: Default::default(),
            render_size: (640, 360),
            layers: vec![
                layer(1),
                LayerDescription {
                    source: LayerSource::Solid([10, 20, 30, 255]),
                    corners: [(0.1, 0.2), (0.6, 0.2), (0.6, 0.7), (0.1, 0.7)],
                    effects: vec![EffectCall::colour_matrix([
                        [0.5, 0.25, 0.25],
                        [0.0, 1.0, 0.0],
                        [0.0, 0.0, 1.0],
                    ])],
                },
            ],
        }
    }

    #[test]
    fn a_description_written_as_json_and_read_back_is_the_same_and_has_the_same_fingerprint() {
        let original = description();
        let text = serde_json::to_string(&original).unwrap();
        let read_back: FrameDescription = serde_json::from_str(&text).unwrap();
        assert_eq!(read_back, original);
        assert_eq!(read_back.fingerprint(), original.fingerprint());
    }

    #[test]
    fn equal_descriptions_have_the_same_fingerprint() {
        assert_eq!(description().fingerprint(), description().fingerprint());
    }

    /// Everything that decides what a frame looks like is part of its fingerprint: one change at a time, each one is a different frame.
    #[test]
    fn every_part_of_a_description_changes_its_fingerprint() {
        let original = description();
        let mut changes: Vec<(&str, FrameDescription)> = Vec::new();
        let mut changed = |name, change: &dyn Fn(&mut FrameDescription)| {
            let mut copy = original.clone();
            change(&mut copy);
            changes.push((name, copy));
        };
        changed("render width", &|d| d.render_size.0 += 1);
        changed("render height", &|d| d.render_size.1 += 1);
        changed("a layer is left out", &|d| {
            d.layers.pop();
        });
        changed("a layer is added", &|d| d.layers.push(layer(9)));
        changed("the layers are in the other order", &|d| d.layers.reverse());
        changed("another picture", &|d| {
            d.layers[0].source = LayerSource::Picture(PictureId(2))
        });
        changed("a picture and a colour", &|d| {
            d.layers[0].source = LayerSource::Solid([0, 0, 0, 255])
        });
        changed("another colour", &|d| {
            d.layers[1].source = LayerSource::Solid([10, 20, 31, 255])
        });
        changed("a corner moved by a thousandth", &|d| d.layers[1].corners[2].0 += 0.001);
        changed("a corner moved by a tiny amount", &|d| {
            d.layers[1].corners[3].1 = f32::from_bits(d.layers[1].corners[3].1.to_bits() + 1)
        });
        changed("an effect's key", &|d| {
            d.layers[1].effects[0].key = "engine:other".to_owned()
        });
        changed("an effect's number", &|d| d.layers[1].effects[0].values[4] = 0.99);
        changed("an effect has a number less", &|d| {
            d.layers[1].effects[0].values.pop();
        });
        changed("an effect is added", &|d| {
            d.layers[0].effects.push(EffectCall::colour_matrix([[1.0; 3]; 3]))
        });
        changed("an effect is left out", &|d| d.layers[1].effects.clear());
        for (name, other) in &changes {
            assert_ne!(other.fingerprint(), original.fingerprint(), "{name}");
        }
        // And no two of the changes are the same frame either.
        let mut fingerprints: Vec<u64> = changes.iter().map(|(_, other)| other.fingerprint()).collect();
        fingerprints.sort_unstable();
        fingerprints.dedup();
        assert_eq!(fingerprints.len(), changes.len());
    }

    #[test]
    fn what_is_shown_has_a_fingerprint_that_is_not_made_of_the_size_and_is_made_of_everything_else() {
        let original = description();
        let mut bigger = original.clone();
        bigger.render_size = (1280, 720);
        assert_ne!(bigger.fingerprint(), original.fingerprint());
        assert_eq!(
            bigger.fingerprint_of_what_is_shown(),
            original.fingerprint_of_what_is_shown(),
            "the same layers made at another size are the same picture"
        );
        let mut moved = original.clone();
        moved.layers[1].corners[0].0 += 0.001;
        assert_ne!(
            moved.fingerprint_of_what_is_shown(),
            original.fingerprint_of_what_is_shown()
        );
        let mut other_picture = original.clone();
        other_picture.layers[0].source = LayerSource::Picture(PictureId(2));
        assert_ne!(
            other_picture.fingerprint_of_what_is_shown(),
            original.fingerprint_of_what_is_shown()
        );
        let mut other_numbers = original.clone();
        other_numbers.layers[1].effects[0].values[0] = 0.75;
        assert_ne!(
            other_numbers.fingerprint_of_what_is_shown(),
            original.fingerprint_of_what_is_shown()
        );
        assert_ne!(
            FrameDescription::black((8, 8)).fingerprint_of_what_is_shown(),
            original.fingerprint_of_what_is_shown()
        );
    }

    #[test]
    fn the_numbers_of_a_layer_are_not_mixed_up_with_those_of_the_next_one() {
        // Two layers with these numbers must not hash like one layer with the numbers of both.
        let mut first = FrameDescription::black((8, 8));
        first.layers.push(LayerDescription {
            source: LayerSource::Solid([1, 2, 3, 4]),
            corners: CORNERS_OF_THE_WHOLE_PICTURE,
            effects: vec![EffectCall {
                key: "a".to_owned(),
                values: vec![1.0, 2.0],
            }],
        });
        let mut second = FrameDescription::black((8, 8));
        second.layers.push(LayerDescription {
            source: LayerSource::Solid([1, 2, 3, 4]),
            corners: CORNERS_OF_THE_WHOLE_PICTURE,
            effects: vec![
                EffectCall {
                    key: "a".to_owned(),
                    values: vec![1.0],
                },
                EffectCall {
                    key: String::new(),
                    values: vec![2.0],
                },
            ],
        });
        assert_ne!(first.fingerprint(), second.fingerprint());
    }

    #[test]
    fn the_colour_matrix_call_has_the_nine_numbers_row_by_row_under_its_key() {
        let call = EffectCall::colour_matrix([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]);
        assert_eq!(call.key, "engine:colour_matrix");
        assert_eq!(call.values, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    }

    #[test]
    fn a_black_frame_has_no_layers() {
        let frame = FrameDescription::black((16, 9));
        assert_eq!(frame.render_size, (16, 9));
        assert!(frame.layers.is_empty());
    }
}
