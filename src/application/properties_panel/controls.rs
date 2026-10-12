//! The controls that the properties of more than one kind of thing have: the sliders for speed and
//! loudness, and the rows they are put in.

use egui::{Slider, SliderClamping, Ui};

use crate::project::{Gain, SpeedFactor};

/// The largest speed the sliders offer. (A project file may hold a larger one.)
pub const LARGEST_SPEED_FACTOR_ON_THE_SLIDER: u32 = 1000;

/// How wide the sliders are, in points, if there is room, and how narrow they are made if there is not.
const SLIDER_WIDTH: f32 = 150.0;
const NARROWEST_SLIDER: f32 = 40.0;

/// How much room the label before a slider and the number after it take up, with the space between them, in points. (What
/// is left is for the slider.)
const ROOM_FOR_THE_LABEL_AND_THE_NUMBER: f32 = 112.0;

/// The speed slider, which goes from 1 to 1000 times, with the same room for each tenfold. Gives the
/// new speed if it was changed.
pub(super) fn speed_slider(ui: &mut Ui, speed_factor: SpeedFactor) -> Option<SpeedFactor> {
    let mut speed = speed_factor.value();
    let label = ui.label("Speed");
    let changed = ui
        .add(
            Slider::new(&mut speed, 1..=LARGEST_SPEED_FACTOR_ON_THE_SLIDER)
                .logarithmic(true)
                .clamping(SliderClamping::Always)
                .suffix("x")
                .smallest_positive(1.0),
        )
        .labelled_by(label.id)
        .changed();
    if changed { SpeedFactor::new(speed) } else { None }
}

/// How many digits after the decimal point the loudness slider shows (and works in).
const DECIMALS_OF_THE_GAIN_SLIDER: usize = 1;

/// The loudness slider, from quieter by 30 dB to louder by 30 dB, with no change in the middle. Gives
/// the new gain if it was changed.
pub(super) fn gain_slider(ui: &mut Ui, gain: Gain) -> Option<Gain> {
    // The slider rounds what it is given to what it shows, and gives that back as a change at once when it is not that already.
    // A gain that keyframes make is hardly ever a whole number of tenths of a decibel, so what is given is rounded first.
    let tenth_of_a_decibel_scale = 10f32.powi(DECIMALS_OF_THE_GAIN_SLIDER as i32);
    let mut decibels = (gain.decibels() * tenth_of_a_decibel_scale).round() / tenth_of_a_decibel_scale;
    let label = ui.label("Gain");
    let changed = ui
        .add(
            Slider::new(&mut decibels, Gain::SMALLEST_DECIBELS..=Gain::LARGEST_DECIBELS)
                .clamping(SliderClamping::Always)
                .suffix(" dB")
                .fixed_decimals(DECIMALS_OF_THE_GAIN_SLIDER),
        )
        .labelled_by(label.id)
        .changed();
    if changed {
        Some(Gain::from_decibels(decibels))
    } else {
        None
    }
}

/// Puts a label and a slider side by side with the sliders lined up.
///
/// A row that has less room than the sliders are wide is made with a shorter slider, so that nothing is wider than the row.
pub(super) fn labelled_row<R>(ui: &mut Ui, add_row: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.spacing_mut().slider_width =
            (ui.available_width() - ROOM_FOR_THE_LABEL_AND_THE_NUMBER).clamp(NARROWEST_SLIDER, SLIDER_WIDTH);
        add_row(ui)
    })
    .inner
}
