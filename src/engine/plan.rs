// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a frame is made of, worked out from its description before anything is drawn: which passes, in which order.
//!
//! It is a function of the description alone (no graphics card in it), so that what the engine will do for a frame can be looked at and tested
//! without one: that a layer with no effects is put in its place straight from its picture, that the first thing drawn clears the picture, and so on.

use super::FrameDescription;

/// One step of making a frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PassPlan {
    /// Clears the picture that the layers are put together in to opaque black, when there are no layers to draw over it.
    ClearAccumulation,
    /// Clears the picture that the layers are put together in to opaque black and then puts the layers in their places on it, back to front. `layers`
    /// are the numbers of the layers in the description.
    PlaceLayers { layers: Vec<usize> },
    /// Turns the picture that was put together into the 8-bit frame.
    Output,
}

/// Every step of a frame, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FramePlan {
    pub passes: Vec<PassPlan>,
}

/// The steps that make the frame `description` describes.
pub fn plan_frame(description: &FrameDescription) -> FramePlan {
    let mut passes = Vec::new();
    if description.layers.is_empty() {
        passes.push(PassPlan::ClearAccumulation);
    } else {
        passes.push(PassPlan::PlaceLayers {
            layers: (0..description.layers.len()).collect(),
        });
    }
    passes.push(PassPlan::Output);
    FramePlan { passes }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{CORNERS_OF_THE_WHOLE_PICTURE, LayerDescription, LayerSource, PictureId};

    fn layer(picture: u64) -> LayerDescription {
        LayerDescription {
            source: LayerSource::Picture(PictureId(picture)),
            corners: CORNERS_OF_THE_WHOLE_PICTURE,
            effects: Vec::new(),
        }
    }

    #[test]
    fn a_frame_with_nothing_in_it_is_cleared_and_then_made_into_the_output() {
        let plan = plan_frame(&FrameDescription::black((64, 36)));
        assert_eq!(plan.passes, vec![PassPlan::ClearAccumulation, PassPlan::Output]);
    }

    #[test]
    fn layers_are_put_in_their_places_back_to_front_in_one_pass_that_also_clears_and_then_the_output_is_made() {
        let mut description = FrameDescription::black((64, 36));
        description.layers = vec![layer(1), layer(2), layer(3)];
        let plan = plan_frame(&description);
        assert_eq!(
            plan.passes,
            vec![PassPlan::PlaceLayers { layers: vec![0, 1, 2] }, PassPlan::Output]
        );
    }
}
