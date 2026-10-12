//! Speakers for tests of the sound effects: they make no sound, and keep a log of what they were asked to play
//! and what was done with them, which the test looks at.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use super::{EffectSpeakers, EffectSpeakersOpener};

/// What happened to the speakers made by [`speakers_that_only_keep_a_log`].
#[derive(Default)]
pub struct SpeakersLog {
    sounds: RefCell<Vec<Arc<[f32]>>>,
    times_opened: Cell<usize>,
    /// Whether the first speakers that are opened have stopped working from the start.
    first_ones_are_broken: Cell<bool>,
    /// Whether opening the speakers fails.
    cannot_be_opened: Cell<bool>,
}

impl SpeakersLog {
    /// The sounds that were played, in the order they were started.
    pub fn sounds_played(&self) -> Vec<Arc<[f32]>> {
        self.sounds.borrow().clone()
    }

    pub fn count_of_sounds_played(&self) -> usize {
        self.sounds.borrow().len()
    }

    /// How many times the speakers were opened.
    pub fn times_opened(&self) -> usize {
        self.times_opened.get()
    }

    /// Makes the first speakers that are opened from now on unusable from the start (the ones opened after them are
    /// fine), as speakers whose device was unplugged are.
    pub fn break_the_first_speakers_opened(&self) {
        self.first_ones_are_broken.set(true);
    }

    /// Makes opening the speakers fail (or succeed again).
    pub fn make_opening_fail(&self, fails: bool) {
        self.cannot_be_opened.set(fails);
    }
}

struct LoggingSpeakers {
    log: Rc<SpeakersLog>,
    sample_rate: u32,
    is_broken: bool,
}

impl EffectSpeakers for LoggingSpeakers {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn play(&mut self, sound: Arc<[f32]>) {
        self.log.sounds.borrow_mut().push(sound);
    }

    fn is_working(&self) -> bool {
        !self.is_broken
    }
}

/// The way to open speakers that play at `sample_rate` and make no sound, and the log of what is done with them.
pub fn speakers_that_only_keep_a_log(sample_rate: u32) -> (EffectSpeakersOpener, Rc<SpeakersLog>) {
    let log = Rc::new(SpeakersLog::default());
    let opener: EffectSpeakersOpener = {
        let log = Rc::clone(&log);
        Box::new(move || {
            if log.cannot_be_opened.get() {
                return Err("no thank you".to_owned());
            }
            log.times_opened.set(log.times_opened.get() + 1);
            let is_the_first = log.times_opened.get() == 1;
            Ok(Box::new(LoggingSpeakers {
                log: Rc::clone(&log),
                sample_rate,
                is_broken: is_the_first && log.first_ones_are_broken.get(),
            }))
        })
    };
    (opener, log)
}
