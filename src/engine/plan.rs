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
    /// Clears the picture that the layers are put together in to opaque black.
    ClearAccumulation,
    /// Turns the picture that was put together into the 8-bit frame.
    Output,
}

/// Every step of a frame, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FramePlan {
    pub passes: Vec<PassPlan>,
}

/// The steps that make the frame `description` describes.
pub fn plan_frame(_description: &FrameDescription) -> FramePlan {
    FramePlan {
        passes: vec![PassPlan::ClearAccumulation, PassPlan::Output],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_with_nothing_in_it_is_cleared_and_then_made_into_the_output() {
        let plan = plan_frame(&FrameDescription::black((64, 36)));
        assert_eq!(plan.passes, vec![PassPlan::ClearAccumulation, PassPlan::Output]);
    }
}
