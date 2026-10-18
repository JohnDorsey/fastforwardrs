// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Getting the feeds of the clips ready before they are heard.
//!
//! A feed is a process (an ffmpeg that runs for one clip), and a process takes tens of milliseconds to start and to find the sound in its file. The graph asks for a feed at the
//! moment its clip is first heard, which is too late: the block would have to wait. So the feeds are got ready **ahead**: [`LookAheadFeeds`] is told where the graph is about to
//! make sound from ([`ClipFeeds::about_to_render`]), and starts the feeds of the clips that begin within the next [`SECONDS_OF_LOOK_AHEAD`], earliest first, with at most
//! [`MOST_FEEDS_OPEN`] open at once (a feed that is not being read waits: its process stops when its pipe is full, and costs a little memory and no work).

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::SAMPLE_RATE;
use super::feed::{ClipFeed, ClipFeeds, ClipRequest};
use super::position::sample_at_frame;
use crate::media::export::{SoundGraphSpecification, SoundItemToExport};

/// How far ahead of the sound that is being made the feeds of the clips are started, in seconds.
pub const SECONDS_OF_LOOK_AHEAD: f64 = 2.0;

/// How many feeds are open at once, at most, when they are started ahead. (A clip whose turn has come is always opened, whatever the number is.)
pub const MOST_FEEDS_OPEN: usize = 12;

/// What makes the feed of a clip: for the sound of a file, a process that cuts the clip out of it.
pub trait ClipSpawner: Send {
    fn spawn(&mut self, request: &ClipRequest<'_>) -> Box<dyn ClipFeed>;
}

/// A feed that is counted while it is open.
struct Counted {
    feed: Box<dyn ClipFeed>,
    open: Arc<AtomicUsize>,
}

impl ClipFeed for Counted {
    fn read(&mut self, left: &mut [f32], right: &mut [f32]) -> usize {
        self.feed.read(left, right)
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        self.open.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A clip that will be heard, and where.
struct Upcoming {
    first_sample: u64,
    strip: usize,
    item: usize,
}

/// The feeds of the clips of a specification, got ready ahead of when they are needed.
pub struct LookAheadFeeds<Spawner: ClipSpawner> {
    specification: SoundGraphSpecification,
    spawner: Spawner,
    /// The clips in the order they are first heard, with those that have been started or opened left out of the front as they are.
    upcoming: Vec<Upcoming>,
    started: std::collections::HashMap<(usize, usize), Counted>,
    /// The clips that have been started or opened.
    done_with: HashSet<(usize, usize)>,
    open: Arc<AtomicUsize>,
    look_ahead_in_samples: u64,
    most_open: usize,
}

impl<Spawner: ClipSpawner> LookAheadFeeds<Spawner> {
    pub fn new(specification: SoundGraphSpecification, spawner: Spawner) -> Self {
        Self::with_limits(specification, spawner, SECONDS_OF_LOOK_AHEAD, MOST_FEEDS_OPEN)
    }

    pub fn with_limits(
        specification: SoundGraphSpecification,
        spawner: Spawner,
        seconds_of_look_ahead: f64,
        most_open: usize,
    ) -> Self {
        let frame_rate = specification.frame_rate;
        let mut upcoming = Vec::new();
        for (strip, items) in specification.strips.iter().enumerate() {
            let mut frames = 0;
            for (item, content) in items.items.iter().enumerate() {
                let length = match content {
                    SoundItemToExport::Silence { frame_count } => *frame_count,
                    SoundItemToExport::Clip(clip) => {
                        upcoming.push(Upcoming {
                            first_sample: sample_at_frame(frame_rate, frames),
                            strip,
                            item,
                        });
                        clip.timeline_frame_count
                    }
                };
                frames += length;
            }
        }
        upcoming.sort_by_key(|clip| (clip.first_sample, clip.strip, clip.item));
        Self {
            specification,
            spawner,
            upcoming,
            started: std::collections::HashMap::new(),
            done_with: HashSet::new(),
            open: Arc::default(),
            look_ahead_in_samples: (seconds_of_look_ahead * f64::from(SAMPLE_RATE)) as u64,
            most_open,
        }
    }

    /// How many feeds are open now: started and waiting, or being read.
    pub fn open_count(&self) -> usize {
        self.open.load(Ordering::SeqCst)
    }

    fn spawn(&mut self, strip: usize, item: usize) -> Counted {
        let SoundItemToExport::Clip(clip) = &self.specification.strips[strip].items[item] else {
            unreachable!("only clips are upcoming");
        };
        let feed = self.spawner.spawn(&ClipRequest { strip, item, clip });
        self.open.fetch_add(1, Ordering::SeqCst);
        Counted {
            feed,
            open: Arc::clone(&self.open),
        }
    }
}

impl<Spawner: ClipSpawner> ClipFeeds for LookAheadFeeds<Spawner> {
    fn open(&mut self, request: &ClipRequest<'_>) -> Box<dyn ClipFeed> {
        let key = (request.strip, request.item);
        self.done_with.insert(key);
        match self.started.remove(&key) {
            Some(counted) => Box::new(counted),
            None => Box::new(self.spawn(request.strip, request.item)),
        }
    }

    fn about_to_render(&mut self, first_sample: u64) {
        let until = first_sample.saturating_add(self.look_ahead_in_samples);
        // (The clips that have been opened or started are no longer in front.)
        let mut next = 0;
        while next < self.upcoming.len() && self.open_count() < self.most_open {
            let clip = &self.upcoming[next];
            if clip.first_sample >= until {
                break;
            }
            let key = (clip.strip, clip.item);
            if !self.done_with.contains(&key) && !self.started.contains_key(&key) {
                let counted = self.spawn(key.0, key.1);
                self.started.insert(key, counted);
            }
            next += 1;
        }
        // Forget the clips at the front that are done with, so that the list gets shorter as the sound goes on.
        let finished_at_the_front = self
            .upcoming
            .iter()
            .take_while(|clip| {
                let key = (clip.strip, clip.item);
                self.done_with.contains(&key) || self.started.contains_key(&key)
            })
            .count();
        self.upcoming.drain(..finished_at_the_front);
    }
}
