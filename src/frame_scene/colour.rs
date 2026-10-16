// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the colour of a frame is, worked out from the settings (phase 3): **one place for the preview and the export**, so that they cannot disagree about what the
//! pictures are put together in. They differ in what the finished frame is encoded as, which is what the two settings are for: the display transform says what the screen
//! shows, and the output space says what the file's numbers are.

use crate::colour_space::StandardPrimaries;
use crate::engine::{FrameColour, OutputEncoding, PictureColour, WorkingSpace};
use crate::project::{InputColourOverride, Settings};

/// What the codes of the red, green and blue of a picture file stand for: sRGB, with the primaries and the curve that the user says instead (a picture has no matrix and no
/// range, and what is said of them is not used).
pub fn colour_of_a_picture_file(said: &InputColourOverride) -> PictureColour {
    PictureColour {
        primaries: said.primaries.unwrap_or(PictureColour::SRGB.primaries),
        curve: said.curve.unwrap_or(PictureColour::SRGB.curve),
    }
}

/// What the layers of every frame are put together in: light, in the primaries of the project's working colour space.
pub fn working_space_of(settings: Settings) -> WorkingSpace {
    WorkingSpace::Linear(settings.working_colour_space.primaries())
}

/// The colour of a frame of the preview: put together in the working space, and encoded for the display (the primaries of the screen are taken to be Rec.709's, the curve is
/// the display transform's).
pub fn colour_of_the_preview(settings: Settings) -> FrameColour {
    FrameColour {
        working: working_space_of(settings),
        output: OutputEncoding {
            primaries: StandardPrimaries::Rec709,
            curve: settings.display_transform.transfer_function(),
        },
    }
}

/// The colour of a frame of the export: put together in the working space, and encoded as the output space says (which, for "as the preview shows it", is as the display
/// transform encodes).
pub fn colour_of_the_export(settings: Settings) -> FrameColour {
    FrameColour {
        working: working_space_of(settings),
        output: OutputEncoding {
            primaries: settings.output_colour_space.primaries(),
            curve: settings
                .output_colour_space
                .transfer_function(settings.display_transform),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colour_space::TransferFunction;
    use crate::project::{DisplayTransform, OutputColourSpace, WorkingColourSpace};

    #[test]
    fn a_picture_file_is_srgb_unless_the_user_says_otherwise_and_what_is_said_of_a_matrix_or_a_range_is_not_used() {
        use crate::colour_space::{VideoMatrix, VideoRange};
        assert_eq!(
            colour_of_a_picture_file(&InputColourOverride::AUTOMATIC),
            PictureColour::SRGB
        );
        let matrix_and_range_only = InputColourOverride {
            matrix: Some(VideoMatrix::Bt2020),
            range: Some(VideoRange::Full),
            ..InputColourOverride::AUTOMATIC
        };
        assert_eq!(colour_of_a_picture_file(&matrix_and_range_only), PictureColour::SRGB);
        let both = InputColourOverride {
            primaries: Some(StandardPrimaries::DisplayP3),
            curve: Some(TransferFunction::Linear),
            ..InputColourOverride::AUTOMATIC
        };
        assert_eq!(
            colour_of_a_picture_file(&both),
            PictureColour {
                primaries: StandardPrimaries::DisplayP3,
                curve: TransferFunction::Linear
            }
        );
        let only_primaries = InputColourOverride {
            primaries: Some(StandardPrimaries::Rec2020),
            ..InputColourOverride::AUTOMATIC
        };
        assert_eq!(colour_of_a_picture_file(&only_primaries).curve, TransferFunction::Srgb);
        assert_eq!(
            colour_of_a_picture_file(&only_primaries).primaries,
            StandardPrimaries::Rec2020
        );
    }

    #[test]
    fn with_the_default_settings_a_frame_is_light_in_rec709_and_the_preview_and_the_export_encode_it_alike() {
        let preview = colour_of_the_preview(Settings::DEFAULT);
        let export = colour_of_the_export(Settings::DEFAULT);
        assert_eq!(preview.working, WorkingSpace::Linear(StandardPrimaries::Rec709));
        assert_eq!(preview.output, OutputEncoding::SRGB);
        assert_eq!(
            export, preview,
            "\"as the preview shows it\": the file's numbers are the preview's"
        );
    }

    #[test]
    fn the_working_space_is_the_same_for_the_preview_and_the_export_whatever_else_is_set() {
        for working in WorkingColourSpace::ALL {
            for display in DisplayTransform::ALL {
                for output in OutputColourSpace::ALL {
                    let settings = Settings {
                        working_colour_space: working,
                        display_transform: display,
                        output_colour_space: output,
                        ..Settings::DEFAULT
                    };
                    assert_eq!(
                        colour_of_the_preview(settings).working,
                        colour_of_the_export(settings).working,
                        "{settings:?}"
                    );
                    assert_eq!(
                        colour_of_the_preview(settings).working,
                        WorkingSpace::Linear(working.primaries())
                    );
                }
            }
        }
    }

    #[test]
    fn the_preview_follows_the_display_and_the_export_follows_the_output_space_and_only_one_of_the_three_follows_the_display()
     {
        for display in DisplayTransform::ALL {
            let settings = Settings {
                display_transform: display,
                ..Settings::DEFAULT
            };
            assert_eq!(
                colour_of_the_preview(settings).output.curve,
                display.transfer_function()
            );
            // The default output space is "as the preview shows it".
            assert_eq!(colour_of_the_export(settings).output.curve, display.transfer_function());
            let bt1886 = Settings {
                output_colour_space: OutputColourSpace::Rec709Bt1886,
                ..settings
            };
            assert_eq!(colour_of_the_export(bt1886).output.curve, TransferFunction::Gamma24);
            assert_eq!(
                colour_of_the_preview(bt1886).output.curve,
                display.transfer_function(),
                "the preview does not follow the output space"
            );
            let srgb = Settings {
                output_colour_space: OutputColourSpace::Srgb,
                ..settings
            };
            assert_eq!(colour_of_the_export(srgb).output.curve, TransferFunction::Srgb);
        }
    }
}
