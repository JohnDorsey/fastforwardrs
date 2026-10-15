// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The control that says for how long something that is shown for a while (a text object, a picture)
//! is shown.

use egui::{DragValue, Ui};

use crate::project::FrameRate;

/// The "Shown for" label and the number of seconds beside it. If the number is changed, this gives
/// how many frames that is. `duration_in_frames` is how long the thing is shown for now.
pub fn shown_for(ui: &mut Ui, frame_rate: FrameRate, duration_in_frames: u64) -> Option<u64> {
    let mut changed_to = None;
    ui.horizontal(|ui| {
        let label = ui.label("Shown for");
        let mut seconds = frame_rate.seconds_at_frame(duration_in_frames);
        if ui
            .add(
                DragValue::new(&mut seconds)
                    .range(0.0..=3600.0)
                    .speed(0.05)
                    .max_decimals(2)
                    .suffix(" s"),
            )
            .labelled_by(label.id)
            .changed()
        {
            changed_to = Some(frame_rate.nearest_frame_to_seconds(seconds).max(1));
        }
    });
    changed_to
}
