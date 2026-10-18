// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Saying yes to the severe question: by holding down for five seconds.
//!
//! *Load it* is not a button that is clicked: a click, a double click, a key that repeats, a hand that slips and the Space bar that presses whatever has the focus must not be
//! able to say yes to running a program that nobody has looked at. So it has to be **held down, with the mouse on it or with the Enter key, for [`SECONDS_TO_HOLD`] without a
//! break**, while it fills up where the person can see it; **letting go starts it over**; and **the Space bar does nothing at all**. This is the arithmetic of that, with the
//! time given to it (so that it can be tested without waiting), and the window draws it (`application::dialogs`).

/// How long *Load it* has to be held down, in seconds.
pub const SECONDS_TO_HOLD: f64 = 5.0;

/// A break in what is told to it of longer than this is taken to be a letting go, whatever it says (a window that was not drawn for a while, because it was behind another,
/// is not a hand that has stayed).
const LONGEST_SILENCE_IN_SECONDS: f64 = 0.5;

/// How far the holding has got.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HoldToConfirm {
    /// When the hold that is going on began, and when it was last told of.
    holding: Option<(f64, f64)>,
    confirmed: bool,
}

impl HoldToConfirm {
    /// Tells it that at time `now` (in seconds, from any start, going forward) the button is `held` (the primary mouse button is down that went down on it, or the Enter key is
    /// down). Gives how far it has got, from 0 to 1.
    pub fn update(&mut self, now: f64, held: bool) -> f64 {
        if self.confirmed {
            return 1.0;
        }
        if !held {
            self.holding = None;
            return 0.0;
        }
        // (A hold that was not told of for too long is over, and this is a new one.)
        let since = match self.holding {
            Some((since, last)) if now - last <= LONGEST_SILENCE_IN_SECONDS => since,
            _ => now,
        };
        self.holding = Some((since, now));
        let progress = ((now - since) / SECONDS_TO_HOLD).clamp(0.0, 1.0);
        if progress >= 1.0 {
            self.confirmed = true;
        }
        progress
    }

    /// Whether it has been held for long enough. (Once it has, it stays so.)
    pub fn is_confirmed(&self) -> bool {
        self.confirmed
    }

    /// How far it has got now, from 0 to 1, for drawing it.
    pub fn progress_at(&self, now: f64) -> f64 {
        if self.confirmed {
            return 1.0;
        }
        match self.holding {
            Some((since, last)) if now - last <= LONGEST_SILENCE_IN_SECONDS => {
                ((now - since) / SECONDS_TO_HOLD).clamp(0.0, 1.0)
            }
            _ => 0.0,
        }
    }
}
