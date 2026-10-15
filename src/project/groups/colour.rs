// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The colour a group is drawn in, everywhere in the window: its outline on the timeline, the bar at the head of its box in the
//! properties panel, and the badges that say a clip, a track or another group is in it.

/// A colour for a group: red, green and blue, each from 0 to 255. (It is not see-through: a group's colour is a colour of its own, not
/// a way to see what is under it.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GroupColour {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl GroupColour {
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    /// The colours that new groups are given, one after another: eight that can be told apart (no two of them are less than 33 degrees
    /// apart in hue, and the ones that follow each other are far apart), away from the yellow that shows what is selected, the orange of
    /// the buttons of the keyframes and the red of the playhead (the least difference of hue from any of the three is 36 degrees: that
    /// band of the wheel of colours is left to them), and bright enough to be seen on the dark of the window (a contrast of 3.6 at the
    /// least with the colour of a track) while either black or white writing can be read on each (a contrast of 4.9 at the least).
    pub const PALETTE: [GroupColour; 8] = [
        Self::new(100, 125, 216),
        Self::new(75, 210, 84),
        Self::new(208, 100, 216),
        Self::new(75, 181, 210),
        Self::new(216, 100, 158),
        Self::new(143, 210, 75),
        Self::new(143, 100, 216),
        Self::new(75, 210, 163),
    ];

    /// How bright the colour is to the eye, from 0 (black) to 1 (white), as accessibility guidelines count it.
    pub fn relative_luminance(self) -> f64 {
        let linear = |channel: u8| {
            let value = f64::from(channel) / 255.0;
            if value <= 0.03928 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(self.red) + 0.7152 * linear(self.green) + 0.0722 * linear(self.blue)
    }

    /// Whether white writing, and not black, is what can be read best on this colour: the one of the two that is the further from
    /// it in brightness, as accessibility guidelines count it.
    pub fn writing_on_it_is_white(self) -> bool {
        let luminance = self.relative_luminance();
        // The contrast with white is (1 + 0.05) / (luminance + 0.05) and with black (luminance + 0.05) / (0 + 0.05).
        let with_white = 1.05 / (luminance + 0.05);
        let with_black = (luminance + 0.05) / 0.05;
        with_white > with_black
    }
}
