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

/// The key of the effect that blurs a picture with a Gaussian blur: one number, the radius (the standard deviation) in points of a picture 1080 high, which is that many
/// pixels in a picture of that height and in proportion in another. It is an effect of the scene-linear domain, and takes two passes of the card.
pub const BLUR_KEY: &str = "engine:blur";

/// The key of the effect that crops a picture: five numbers, the fractions of the picture's width and height that are cut off from the left, the top, the right and the
/// bottom, and the width of the soft edge as a fraction of the picture's height. What is outside becomes clear. It is an effect of the scene-linear domain.
pub const CROP_KEY: &str = "engine:crop";

/// The key of the effect that applies a look-up table (a `.cube` file) to the colours of a picture: four numbers, how much of the table's colours there are in the result (0 to
/// 1), the space the colours are taken to the table in (see [`LutSpace`]), the space they come out of it in, and how the table is looked up between its points (see
/// [`LutInterpolation`]); and the table, by the [`TableId`] that the call has. A call without a table does nothing. An effect of the display-referred domain where the two spaces are
/// the codes of a screen and the whole of the table is wanted, and of the scene-linear domain otherwise.
pub const LUT_KEY: &str = "engine:lut";

/// Which table a call uses: the table (a [`CubeTable`](crate::cube_table::CubeTable)) has been published to the engine under this number (see `tables`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TableId(pub u64);

/// The space that the colours of a picture are in when they go into a table, or come out of it (CM-11): **a number, in the call**.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LutSpace {
    /// The codes of a screen: sRGB codes of Rec.709 primaries. What a table made for pictures nearly always wants.
    Screen = 0,
    /// Rec.709 video: the primaries of Rec.709 and the power 2.4 of BT.1886.
    Video = 1,
    /// Linear light, in the primaries of the working space as they are.
    Linear = 2,
}

impl LutSpace {
    pub const ALL: [Self; 3] = [Self::Screen, Self::Video, Self::Linear];

    /// The number that a call has for it.
    pub fn number(self) -> f32 {
        f32::from(self as u8)
    }

    /// The space that a number of a call says, if it says one.
    pub fn from_number(number: f32) -> Option<Self> {
        Self::ALL.into_iter().find(|space| space.number() == number)
    }
}

/// How a table is looked up between its points: **a number, in the call**.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LutInterpolation {
    Tetrahedral = 0,
    Trilinear = 1,
}

impl LutInterpolation {
    pub const ALL: [Self; 2] = [Self::Tetrahedral, Self::Trilinear];

    pub fn number(self) -> f32 {
        f32::from(self as u8)
    }

    pub fn from_number(number: f32) -> Option<Self> {
        Self::ALL.into_iter().find(|way| way.number() == number)
    }
}

/// The corners of a layer that is all of the picture: top left, top right, bottom right, bottom left, as fractions of the picture.
pub const CORNERS_OF_THE_WHOLE_PICTURE: [(f32, f32); 4] = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];

/// Which picture: the same id is always the same pixels (a decoded picture's serial number, or a number made from what a text picture is made from),
/// so a picture that the engine has is never sent again, and one that is not the same never has the same id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PictureId(pub u64);

/// What a layer is made of.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LayerSource {
    /// A picture the engine has been sent.
    Picture(PictureId),
    /// One colour all over (red, green, blue and how opaque, not multiplied by it).
    Solid([u8; 4]),
    /// **A group** (phase 5): layers that are put together in a picture of their own, the size of the frame and clear where nothing is, which is the picture of this layer. What is
    /// done to the layer's picture (its effects) is done to **the composite of the layers of the group**, and the picture is then put in its place like any other.
    Group(GroupDescription),
}

/// The layers of a group: the one at the back first, as a frame's are. They may be groups too.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GroupDescription {
    pub layers: Vec<LayerDescription>,
}

impl LayerSource {
    /// What tells this source from every other: equal sources hash alike.
    fn hash_into(&self, hasher: &mut DefaultHasher) {
        match self {
            Self::Picture(id) => {
                0_u8.hash(hasher);
                id.hash(hasher);
            }
            Self::Solid(rgba) => {
                1_u8.hash(hasher);
                rgba.hash(hasher);
            }
            Self::Group(group) => {
                2_u8.hash(hasher);
                hash_layers(&group.layers, hasher);
            }
        }
    }
}

/// What a call of a shader that depends on earlier frames says of itself: **which state it goes on with** and **which frame of it this is** (phase 6.4.2; see `docs/engine-phase-6-4-design.md`, K8 and K9).
///
/// The engine keeps the buffers of such a shader that last from one frame to the next under `id`. A frame that is the one after the one the state was made at goes on from it; the same frame
/// again is made from the state as it was before that frame (so that it is the same picture); any other frame starts clear.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EffectState {
    /// What tells this effect on this clip (or group) from every other: the same number in every frame that the state goes on in.
    pub id: u64,
    /// The frame of the effect's own clock (frames since the clip, or the timeline, began); the first is 0.
    pub frame: u64,
}

/// One thing that is done to the picture of a layer, by the key of the effect and the numbers it is given.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectCall {
    pub key: String,
    pub values: Vec<f32>,
    /// The table that the effect uses, if it uses one (see [`LUT_KEY`]).
    #[serde(default)]
    pub table: Option<TableId>,
    /// Which state the effect goes on with, if it is a shader that depends on earlier frames (see [`EffectState`]); nobody else has one.
    #[serde(default)]
    pub state: Option<EffectState>,
    /// The pictures of the images that the shader brings with it (an ISF file's `IMPORTED`), in the order its file lists them (phase 6.4.4); none for any other effect. They are
    /// pictures that the engine is sent, as those of the layers are, **made so that the shader reads their codes as they are** (they are not converted to light).
    #[serde(default)]
    pub images: Vec<PictureId>,
}

impl EffectCall {
    /// The effect that mixes colours with `rows` (see [`COLOUR_MATRIX_KEY`]).
    pub fn colour_matrix(rows: [[f32; 3]; 3]) -> Self {
        Self {
            key: COLOUR_MATRIX_KEY.to_owned(),
            values: rows.into_iter().flatten().collect(),
            table: None,
            state: None,
            images: Vec::new(),
        }
    }

    /// The effect that mixes colours with `rows` and then adds `offsets` (see [`COLOUR_MATRIX_KEY`]).
    pub fn colour_matrix_with_offsets(rows: [[f32; 3]; 3], offsets: [f32; 3]) -> Self {
        Self {
            key: COLOUR_MATRIX_KEY.to_owned(),
            values: rows.into_iter().flatten().chain(offsets).collect(),
            table: None,
            state: None,
            images: Vec::new(),
        }
    }

    /// The effect that makes the light of a picture brighter or darker by `stops` (see [`EXPOSURE_KEY`]).
    pub fn exposure(stops: f32) -> Self {
        Self {
            key: EXPOSURE_KEY.to_owned(),
            values: vec![stops],
            table: None,
            state: None,
            images: Vec::new(),
        }
    }

    /// The effect that blurs a picture by `radius` points of a picture 1080 high (see [`BLUR_KEY`]).
    pub fn blur(radius: f32) -> Self {
        Self {
            key: BLUR_KEY.to_owned(),
            values: vec![radius],
            table: None,
            state: None,
            images: Vec::new(),
        }
    }

    /// The effect that cuts the fractions `left`, `top`, `right` and `bottom` off a picture, with an edge `feather` (a fraction of the picture's height) wide (see
    /// [`CROP_KEY`]).
    pub fn crop(left: f32, top: f32, right: f32, bottom: f32, feather: f32) -> Self {
        Self {
            key: CROP_KEY.to_owned(),
            values: vec![left, top, right, bottom, feather],
            table: None,
            state: None,
            images: Vec::new(),
        }
    }

    /// The effect that applies the table `table` (see [`LUT_KEY`]): `intensity` of it (0 to 1), taking the colours to it in `input` and out of it in `output`, looked up
    /// as `interpolation` says.
    pub fn lut(
        table: TableId,
        intensity: f32,
        input: LutSpace,
        output: LutSpace,
        interpolation: LutInterpolation,
    ) -> Self {
        Self {
            key: LUT_KEY.to_owned(),
            values: vec![intensity, input.number(), output.number(), interpolation.number()],
            table: Some(table),
            state: None,
            images: Vec::new(),
        }
    }

    /// The effect that makes a picture `opacity` as opaque as it is (see [`OPACITY_KEY`]).
    pub fn opacity(opacity: f32) -> Self {
        Self {
            key: OPACITY_KEY.to_owned(),
            values: vec![opacity],
            table: None,
            state: None,
            images: Vec::new(),
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

    /// Every layer of the frame and of the groups in it, numbered as the plan numbers them (see [`FlatLayers`]).
    pub fn flatten(&self) -> FlatLayers<'_> {
        let mut layers: Vec<FlatLayer<'_>> = Vec::new();
        let mut containers: Vec<std::ops::Range<usize>> = Vec::new();
        // (The layers of each container, the frame's first. A group found in a container is put at the end, so that it comes after the container it is in.)
        let mut pending: Vec<&[LayerDescription]> = vec![&self.layers];
        let mut next = 0;
        while next < pending.len() {
            let first = layers.len();
            for layer in pending[next] {
                let group = match &layer.source {
                    LayerSource::Group(group) => {
                        pending.push(&group.layers);
                        Some(pending.len() - 1)
                    }
                    LayerSource::Picture(_) | LayerSource::Solid(_) => None,
                };
                layers.push(FlatLayer {
                    layer,
                    container: next,
                    group,
                });
            }
            containers.push(first..layers.len());
            next += 1;
        }
        FlatLayers { layers, containers }
    }

    /// The pictures that the frame is made of, in the layers of the frame and of its groups, each once, in the order they are met in.
    pub fn pictures_used(&self) -> Vec<PictureId> {
        let mut used: Vec<PictureId> = Vec::new();
        for flat in self.flatten().layers {
            if let LayerSource::Picture(id) = flat.layer.source
                && !used.contains(&id)
            {
                used.push(id);
            }
            // (The images that the shaders of a layer bring with them are pictures of the frame too.)
            for id in flat
                .layer
                .effects
                .iter()
                .flat_map(|effect| effect.images.iter().copied())
            {
                if !used.contains(&id) {
                    used.push(id);
                }
            }
        }
        used
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

    /// Whether any effect in the frame goes on with a state it keeps from earlier frames (see [`EffectState`]): **such a frame is never the same as another that looks the same**, because
    /// what it shows depends on the frames before it, so it is not kept in the cache of finished frames and not taken from it.
    pub fn depends_on_earlier_frames(&self) -> bool {
        self.flatten()
            .layers
            .iter()
            .any(|flat| flat.layer.effects.iter().any(|effect| effect.state.is_some()))
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
        hash_layers(&self.layers, hasher);
    }
}

/// A frame's layers and those of the groups in it, in one list (the frame's first, then those of the first group that was found, and so on): **the number of a layer in the list is the
/// number that the plan and the renderer give it**. A group is a container with a number too (the frame is container 0): the containers are numbered in the order they are found in, so
/// **a container is always numbered after the one it is in**, and drawing them from the last to the first draws every group before the layer that is made of it.
#[derive(Debug)]
pub struct FlatLayers<'a> {
    pub layers: Vec<FlatLayer<'a>>,
    /// For each container, which of `layers` are its own.
    pub containers: Vec<std::ops::Range<usize>>,
}

/// One layer of a [`FlatLayers`].
#[derive(Clone, Copy, Debug)]
pub struct FlatLayer<'a> {
    pub layer: &'a LayerDescription,
    /// The container this layer is in (0 is the frame).
    pub container: usize,
    /// If the layer is a group, the container that its layers are.
    pub group: Option<usize>,
}

/// Hashes `layers` and everything in them, the layers of the groups among them too.
fn hash_layers(layers: &[LayerDescription], hasher: &mut DefaultHasher) {
    layers.len().hash(hasher);
    for layer in layers {
        layer.source.hash_into(hasher);
        for (across, down) in layer.corners {
            across.to_bits().hash(hasher);
            down.to_bits().hash(hasher);
        }
        layer.effects.len().hash(hasher);
        for effect in &layer.effects {
            effect.key.hash(hasher);
            effect.table.hash(hasher);
            effect.state.hash(hasher);
            effect.images.hash(hasher);
            effect.values.len().hash(hasher);
            for value in &effect.values {
                value.to_bits().hash(hasher);
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
        changed("an effect has a table", &|d| {
            d.layers[1].effects[0].table = Some(TableId(7))
        });
        changed("an effect has a state", &|d| {
            d.layers[1].effects[0].state = Some(EffectState { id: 1, frame: 0 })
        });
        changed("an effect has another state", &|d| {
            d.layers[0].effects.push(EffectCall {
                state: Some(EffectState { id: 2, frame: 0 }),
                ..EffectCall::colour_matrix([[1.0; 3]; 3])
            })
        });
        changed("an effect has another state than that", &|d| {
            d.layers[0].effects.push(EffectCall {
                state: Some(EffectState { id: 2, frame: 1 }),
                ..EffectCall::colour_matrix([[1.0; 3]; 3])
            })
        });
        changed("an effect has another table", &|d| {
            d.layers[0].effects.push(EffectCall::lut(
                TableId(8),
                1.0,
                LutSpace::Screen,
                LutSpace::Screen,
                LutInterpolation::Tetrahedral,
            ));
        });
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
    fn a_frame_depends_on_earlier_frames_if_any_effect_of_any_layer_of_it_or_of_a_group_in_it_has_a_state() {
        let stateful = || EffectCall {
            state: Some(EffectState { id: 3, frame: 5 }),
            ..EffectCall::exposure(1.0)
        };
        assert!(!description().depends_on_earlier_frames());
        assert!(!FrameDescription::black((8, 8)).depends_on_earlier_frames());
        let mut on_the_last = description();
        on_the_last.layers.last_mut().unwrap().effects.push(stateful());
        assert!(on_the_last.depends_on_earlier_frames(), "on the last of two layers");
        let mut on_the_first = description();
        on_the_first.layers[0].effects.push(stateful());
        assert!(on_the_first.depends_on_earlier_frames(), "on the first");
        // In a group of the frame.
        let mut in_a_group = description();
        in_a_group.layers.push(LayerDescription {
            source: LayerSource::Group(GroupDescription {
                layers: vec![LayerDescription {
                    effects: vec![stateful()],
                    ..layer(4)
                }],
            }),
            corners: CORNERS_OF_THE_WHOLE_PICTURE,
            effects: Vec::new(),
        });
        assert!(in_a_group.depends_on_earlier_frames(), "in a group");
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
                table: None,
                state: None,
                images: Vec::new(),
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
                    table: None,
                    state: None,
                    images: Vec::new(),
                },
                EffectCall {
                    key: String::new(),
                    values: vec![2.0],
                    table: None,
                    state: None,
                    images: Vec::new(),
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
