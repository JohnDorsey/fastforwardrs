// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The block graph itself.

use super::feed::{ClipFeed, ClipFeeds, ClipRequest};
use super::gain::GainCurve;
use super::insertions::{BlockProcessor, Insertions};
use super::position::sample_at_frame;
use crate::media::export::{SoundGraphSpecification, SoundItemToExport};

/// The sound of a project, made a block at a time (see the top of this module's parent for what it does). It is given the specification once, and asked for the sound from
/// its start to its end, in blocks of whatever size is wanted: the sound is the same for any sizes.
pub struct SoundGraph {
    specification: SoundGraphSpecification,
    feeds: Box<dyn ClipFeeds + Send>,
    insertions: Box<dyn Insertions + Send>,
    strips: Vec<Strip>,
    total_samples: u64,
    position: u64,
    scratch: Scratch,
}

/// The sound of one strip while a block is made, before it is added to the others.
#[derive(Default)]
struct Scratch {
    left: Vec<f32>,
    right: Vec<f32>,
}

struct Strip {
    items: Vec<Item>,
    /// The first item that has not ended yet.
    current: usize,
    processors: Vec<Box<dyn BlockProcessor>>,
}

struct Item {
    /// The first sample of the item, and the sample just after its last.
    start: u64,
    end: u64,
    kind: ItemKind,
}

enum ItemKind {
    Silence,
    Clip {
        gain: GainCurve,
        /// What the clip is while it is heard: its feed and its effects. Opened when the clip is first heard, and let go of when it has ended.
        live: Option<Live>,
    },
}

struct Live {
    feed: Box<dyn ClipFeed>,
    processors: Vec<Box<dyn BlockProcessor>>,
    /// Whether the feed has given all its sound already.
    feed_has_ended: bool,
    /// Whether the effects have been given the samples that make up for their latency (see [`BlockProcessor::latency`]).
    has_made_up_for_the_latency: bool,
}

impl Live {
    /// Gives the effects, before the first sample of the clip is made, as many samples of the feed as their latency in all, and throws away what comes out: the sound that comes out
    /// of them from then on is the sound of the clip, where it is. (The effects are given silence after the end of the feed, so the last samples of the clip come out too.)
    fn make_up_for_the_latency(&mut self) {
        if std::mem::replace(&mut self.has_made_up_for_the_latency, true) {
            return;
        }
        let latency: usize = self.processors.iter().map(|processor| processor.latency()).sum();
        if latency == 0 {
            return;
        }
        let (mut left, mut right) = (vec![0.0_f32; latency], vec![0.0_f32; latency]);
        if !self.feed_has_ended {
            let given = self.feed.read(&mut left, &mut right);
            self.feed_has_ended = given < latency;
        }
        for processor in &mut self.processors {
            processor.process(&mut left, &mut right);
        }
    }
}

impl SoundGraph {
    pub fn new(
        specification: SoundGraphSpecification,
        feeds: Box<dyn ClipFeeds + Send>,
        mut insertions: Box<dyn Insertions + Send>,
    ) -> Self {
        let frame_rate = specification.frame_rate;
        let strips = specification
            .strips
            .iter()
            .enumerate()
            .map(|(strip_number, strip)| {
                let mut frames_before = 0_u64;
                let items = strip
                    .items
                    .iter()
                    .map(|item| {
                        let frames = match item {
                            SoundItemToExport::Silence { frame_count } => *frame_count,
                            SoundItemToExport::Clip(clip) => clip.timeline_frame_count,
                        };
                        let start = sample_at_frame(frame_rate, frames_before);
                        frames_before += frames;
                        let end = sample_at_frame(frame_rate, frames_before);
                        let kind = match item {
                            SoundItemToExport::Silence { .. } => ItemKind::Silence,
                            SoundItemToExport::Clip(clip) => ItemKind::Clip {
                                gain: GainCurve::new(&clip.loudness, frame_rate),
                                live: None,
                            },
                        };
                        Item { start, end, kind }
                    })
                    .collect();
                Strip {
                    items,
                    current: 0,
                    processors: insertions.for_strip(strip_number),
                }
            })
            .collect();
        Self {
            total_samples: sample_at_frame(frame_rate, specification.total_frame_count),
            specification,
            feeds,
            insertions,
            strips,
            position: 0,
            scratch: Scratch::default(),
        }
    }

    /// How many samples the sound has in all.
    pub fn total_samples(&self) -> u64 {
        self.total_samples
    }

    /// How many samples have been made so far.
    pub fn position(&self) -> u64 {
        self.position
    }

    /// Makes the next samples of the sound into `left` and `right` (as long as each other, or the shorter is used) and says how many: all that fit, until the sound ends,
    /// and then none.
    pub fn render(&mut self, left: &mut [f32], right: &mut [f32]) -> usize {
        let wanted = left.len().min(right.len());
        self.feeds.about_to_render(self.position);
        let count = (self.total_samples - self.position).min(wanted as u64) as usize;
        left[..count].fill(0.0);
        right[..count].fill(0.0);
        if count == 0 {
            return 0;
        }
        let (from, to) = (self.position, self.position + count as u64);
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.left.resize(count.max(scratch.left.len()), 0.0);
        scratch.right.resize(count.max(scratch.right.len()), 0.0);
        for strip_number in 0..self.strips.len() {
            scratch.left[..count].fill(0.0);
            scratch.right[..count].fill(0.0);
            self.render_strip(strip_number, from, to, &mut scratch);
            for processor in &mut self.strips[strip_number].processors {
                processor.process(&mut scratch.left[..count], &mut scratch.right[..count]);
            }
            for (sum, sample) in left[..count].iter_mut().zip(&scratch.left[..count]) {
                *sum += *sample;
            }
            for (sum, sample) in right[..count].iter_mut().zip(&scratch.right[..count]) {
                *sum += *sample;
            }
        }
        self.scratch = scratch;
        self.position = to;
        count
    }

    /// Makes the sound of strip `strip_number` from sample `from` to sample `to` (not including it) into the start of `scratch`, which is silent when it is given.
    fn render_strip(&mut self, strip_number: usize, from: u64, to: u64, scratch: &mut Scratch) {
        let Self {
            specification,
            feeds,
            insertions,
            strips,
            ..
        } = self;
        let strip = &mut strips[strip_number];
        while strip.current < strip.items.len() && strip.items[strip.current].end <= from {
            strip.current += 1;
        }
        let mut number = strip.current;
        while number < strip.items.len() && strip.items[number].start < to {
            let item = &mut strip.items[number];
            let (first, last) = (item.start.max(from), item.end.min(to));
            if let ItemKind::Clip { gain, live: slot } = &mut item.kind {
                let SoundItemToExport::Clip(clip) = &specification.strips[strip_number].items[number] else {
                    unreachable!("an item that is a clip here is a clip in the specification");
                };
                let request = ClipRequest {
                    strip: strip_number,
                    item: number,
                    clip,
                };
                let live = slot.get_or_insert_with(|| Live {
                    feed: feeds.open(&request),
                    processors: insertions.for_clip(&request),
                    feed_has_ended: false,
                    has_made_up_for_the_latency: false,
                });
                live.make_up_for_the_latency();
                let offset = (first - from) as usize;
                let length = (last - first) as usize;
                let left = &mut scratch.left[offset..offset + length];
                let right = &mut scratch.right[offset..offset + length];
                if !live.feed_has_ended {
                    let given = live.feed.read(left, right);
                    // (What the feed did not give stays silent: the clip is as long as its place on the timeline whatever the file has.)
                    live.feed_has_ended = given < length;
                }
                for processor in &mut live.processors {
                    processor.process(left, right);
                }
                gain.apply(first - item.start, left, right);
                if last == item.end {
                    *slot = None;
                }
            }
            number += 1;
        }
    }

    /// Makes all the sound that is left, in blocks of `block_samples`, and gives it as two buffers. (For tests, and for whatever wants all of it at once.)
    pub fn render_to_the_end(&mut self, block_samples: usize) -> (Vec<f32>, Vec<f32>) {
        let (mut left, mut right) = (Vec::new(), Vec::new());
        let mut block_left = vec![0.0; block_samples];
        let mut block_right = vec![0.0; block_samples];
        loop {
            let count = self.render(&mut block_left, &mut block_right);
            if count == 0 {
                return (left, right);
            }
            left.extend_from_slice(&block_left[..count]);
            right.extend_from_slice(&block_right[..count]);
        }
    }
}
