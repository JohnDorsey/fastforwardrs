// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The block graph itself.

use super::feed::{ClipFeed, ClipFeeds, ClipRequest};
use super::gain::GainCurve;
use super::insertions::{BlockProcessor, BusRequest, Insertions};
use super::position::{pre_roll_samples_of, sample_at_frame};
use crate::composition::tail_samples_of_a_chain;
use crate::media::export::{SoundGraphSpecification, SoundItemToExport};

/// The sound of a project, made a block at a time (see the top of this module's parent for what it does). It is given the specification once, and asked for the sound from
/// its start to its end, in blocks of whatever size is wanted: the sound is the same for any sizes.
pub struct SoundGraph {
    specification: SoundGraphSpecification,
    feeds: Box<dyn ClipFeeds + Send>,
    insertions: Box<dyn Insertions + Send>,
    strips: Vec<Strip>,
    total_samples: u64,
    /// How many samples at the start are made and not heard (see `SoundGraphSpecification::lead_in_frames`).
    lead_in_samples: u64,
    position: u64,
    scratch: Scratch,
    /// Where the sound of one item is made before it is added to what is there (items that ring on overlap the ones after them).
    work: Scratch,
}

/// The sound of one strip while a block is made, before it is added to the others.
#[derive(Default)]
struct Scratch {
    left: Vec<f32>,
    right: Vec<f32>,
}

impl Scratch {
    /// Makes room for `count` samples, and makes the first `count` of them silent.
    fn silent_for(&mut self, count: usize) {
        if self.left.len() < count {
            self.left.resize(count, 0.0);
            self.right.resize(count, 0.0);
        }
        self.left[..count].fill(0.0);
        self.right[..count].fill(0.0);
    }
}

struct Strip {
    items: Vec<Item>,
    /// The first item that has not ended yet.
    current: usize,
    processors: Vec<Box<dyn BlockProcessor>>,
    /// The instances of the effects of the track that clips share (phase 12): each hears the items from its first to its last, put together.
    buses: Vec<Bus>,
    /// For each item, which bus it is heard by, if one.
    bus_of_item: Vec<Option<usize>>,
    /// How loud the track is, after its effects (counted from the first sample of the sound), and where it is muted, in samples: each from a sample up to a sample (not including it).
    gain: GainCurve,
    muted: Vec<(u64, u64)>,
}

/// One instance of the effects of a track that hears several items together: what the effects are, from the first item to the last, once they are made.
struct Bus {
    first_item: usize,
    last_item: usize,
    /// The first sample of the first item, and the sample just after the last that it makes: the last item has rung on, and the effects of the bus after it.
    start: u64,
    end: u64,
    state: BusState,
    /// The sound of the items, put together, while a block is made, before the effects are given it.
    input: Scratch,
}

enum BusState {
    /// Not heard yet: its effects are made when it is.
    Waiting,
    Running(RunningBus),
    /// Heard to its end: its effects are let go of.
    Done,
}

struct RunningBus {
    processors: Vec<Box<dyn BlockProcessor>>,
    /// How many samples ahead of the sound that comes out the sound that goes in is made: the latency of the effects (see [`BlockProcessor::latency`]), which they are made up for by.
    ahead: u64,
}

struct Item {
    /// The first sample of the item, and the sample just after its last.
    start: u64,
    end: u64,
    /// How many samples its effects go on making sound after it (phase 12: the tails of its own effects, added up).
    tail: u64,
    kind: ItemKind,
}

impl Item {
    /// The sample just after the last that the item makes: its effects have rung on.
    fn rings_until(&self) -> u64 {
        self.end + self.tail
    }
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
    /// How many samples of sound from before the clip the feed begins with (see `pre_roll_frames_of`).
    pre_roll_samples: usize,
}

impl Live {
    /// The feed begins with sound from before the clip, for the effects that remember: it is given to the effects and what comes out is thrown away.
    fn give_the_effects_the_sound_from_before_the_clip(&mut self) {
        let mut left_to_give = self.pre_roll_samples;
        if left_to_give == 0 {
            return;
        }
        for processor in &mut self.processors {
            processor.about_to_be_given_sound_from_before_the_clip(left_to_give);
        }
        let (mut left, mut right) = (
            vec![0.0_f32; MOST_PRE_ROLL_AT_ONCE],
            vec![0.0_f32; MOST_PRE_ROLL_AT_ONCE],
        );
        while left_to_give > 0 {
            let length = left_to_give.min(MOST_PRE_ROLL_AT_ONCE);
            let (left, right) = (&mut left[..length], &mut right[..length]);
            // (The feed has the sound from before the clip; if it has less, what it does not have is silence.)
            left.fill(0.0);
            right.fill(0.0);
            if !self.feed_has_ended {
                let given = self.feed.read(left, right);
                self.feed_has_ended = given < length;
            }
            for processor in &mut self.processors {
                processor.process(left, right);
            }
            left_to_give -= length;
        }
    }

    /// Gives the effects, before the first sample of the clip is made, as many samples of the feed as their latency in all, and throws away what comes out: the sound that comes out
    /// of them from then on is the sound of the clip, where it is. (The effects are given silence after the end of the feed, so the last samples of the clip come out too.)
    fn make_up_for_the_latency(&mut self) {
        if std::mem::replace(&mut self.has_made_up_for_the_latency, true) {
            return;
        }
        self.give_the_effects_the_sound_from_before_the_clip();
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

/// The most samples of sound from before a clip that are given to its effects in one block.
const MOST_PRE_ROLL_AT_ONCE: usize = 4096;

/// The most samples of lead-in that are made, and thrown away, in one block.
const MOST_LEAD_IN_AT_ONCE: usize = 4096;

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
                let items: Vec<Item> = strip
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
                        let (kind, tail) = match item {
                            SoundItemToExport::Silence { .. } => (ItemKind::Silence, 0),
                            SoundItemToExport::Clip(clip) => (
                                ItemKind::Clip {
                                    gain: GainCurve::new(&clip.loudness, frame_rate),
                                    live: None,
                                },
                                tail_samples_of_a_chain(&clip.effects),
                            ),
                        };
                        Item { start, end, tail, kind }
                    })
                    .collect();
                let mut bus_of_item = vec![None; items.len()];
                let buses = strip
                    .buses
                    .iter()
                    .enumerate()
                    .map(|(bus_number, bus)| {
                        assert!(
                            bus.first_item <= bus.last_item && bus.last_item < items.len(),
                            "a bus hears items that are there"
                        );
                        for heard_by in &mut bus_of_item[bus.first_item..=bus.last_item] {
                            assert!(heard_by.is_none(), "an item is heard by one bus at most");
                            *heard_by = Some(bus_number);
                        }
                        let rings_on_until = items[bus.first_item..=bus.last_item]
                            .iter()
                            .map(Item::rings_until)
                            .max()
                            .unwrap_or(0);
                        Bus {
                            first_item: bus.first_item,
                            last_item: bus.last_item,
                            start: items[bus.first_item].start,
                            end: rings_on_until + tail_samples_of_a_chain(&bus.effects),
                            state: BusState::Waiting,
                            input: Scratch::default(),
                        }
                    })
                    .collect();
                Strip {
                    items,
                    current: 0,
                    processors: insertions.for_strip(strip_number),
                    buses,
                    bus_of_item,
                    gain: GainCurve::new(&strip.loudness, frame_rate),
                    muted: strip
                        .muted
                        .iter()
                        .map(|&(first, end)| (sample_at_frame(frame_rate, first), sample_at_frame(frame_rate, end)))
                        .collect(),
                }
            })
            .collect();
        Self {
            total_samples: sample_at_frame(frame_rate, specification.total_frame_count),
            lead_in_samples: sample_at_frame(frame_rate, specification.lead_in_frames),
            specification,
            feeds,
            insertions,
            strips,
            position: 0,
            scratch: Scratch::default(),
            work: Scratch::default(),
        }
    }

    /// How many samples the sound has in all (not those at the start that are made and not heard).
    pub fn total_samples(&self) -> u64 {
        self.total_samples - self.lead_in_samples
    }

    /// How many samples of the sound have been made so far (not those at the start that are made and not heard).
    pub fn position(&self) -> u64 {
        self.position.saturating_sub(self.lead_in_samples)
    }

    /// Makes the next samples of the sound into `left` and `right` (as long as each other, or the shorter is used) and says how many: all that fit, until the sound ends,
    /// and then none. (The first call makes the lead-in first, and throws it away: see [`SoundGraphSpecification::lead_in_frames`].)
    pub fn render(&mut self, left: &mut [f32], right: &mut [f32]) -> usize {
        if self.position < self.lead_in_samples {
            let (mut left_to_throw_away, mut right_to_throw_away) =
                (vec![0.0_f32; MOST_LEAD_IN_AT_ONCE], vec![0.0_f32; MOST_LEAD_IN_AT_ONCE]);
            while self.position < self.lead_in_samples {
                let length = ((self.lead_in_samples - self.position) as usize).min(MOST_LEAD_IN_AT_ONCE);
                self.render_a_block(&mut left_to_throw_away[..length], &mut right_to_throw_away[..length]);
            }
        }
        self.render_a_block(left, right)
    }

    /// [`Self::render`] for the samples that come next, wherever they are.
    fn render_a_block(&mut self, left: &mut [f32], right: &mut [f32]) -> usize {
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
        for strip_number in 0..self.strips.len() {
            scratch.silent_for(count);
            self.render_strip(strip_number, from, to, &mut scratch);
            let strip = &mut self.strips[strip_number];
            for processor in &mut strip.processors {
                processor.process(&mut scratch.left[..count], &mut scratch.right[..count]);
            }
            // (The volume and the mute of the track come after its effects: what they remember goes on while it is muted.)
            strip
                .gain
                .apply(from, &mut scratch.left[..count], &mut scratch.right[..count]);
            for &(first, end) in &strip.muted {
                let (first, end) = (first.max(from), end.min(to));
                if first < end {
                    let (first, end) = ((first - from) as usize, (end - from) as usize);
                    scratch.left[first..end].fill(0.0);
                    scratch.right[first..end].fill(0.0);
                }
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
            work,
            ..
        } = self;
        let strip = &mut strips[strip_number];
        while strip.current < strip.items.len() && strip.items[strip.current].rings_until() <= from {
            strip.current += 1;
        }
        // The instances of the effects of the track that are first heard in this block are made, and given the sound that makes up for their latency.
        for bus_number in 0..strip.buses.len() {
            let bus = &strip.buses[bus_number];
            if matches!(bus.state, BusState::Waiting) && bus.start < to && from < bus.end {
                let specification_of_the_bus = &specification.strips[strip_number].buses[bus_number];
                let first_clip = (bus.first_item..=bus.last_item).find_map(|place| {
                    match &specification.strips[strip_number].items[place] {
                        SoundItemToExport::Clip(clip) => Some(clip),
                        SoundItemToExport::Silence { .. } => None,
                    }
                });
                let processors = match first_clip {
                    Some(first_clip) => insertions.for_bus(&BusRequest {
                        strip: strip_number,
                        bus: bus_number,
                        first_clip,
                        effects: &specification_of_the_bus.effects,
                    }),
                    None => Vec::new(),
                };
                let ahead: usize = processors.iter().map(|processor| processor.latency()).sum();
                let mut running = RunningBus {
                    processors,
                    ahead: ahead as u64,
                };
                let first = from.max(bus.start);
                if ahead > 0 {
                    // (What the effects are given before they make the first sample that is heard: the sound that comes out of them from then on is where it is.)
                    let mut input = Scratch::default();
                    input.silent_for(ahead);
                    render_the_items_of_a_bus(
                        strip_number,
                        &mut strip.items,
                        (bus.first_item, bus.last_item),
                        specification,
                        feeds,
                        insertions,
                        (first, first + ahead as u64),
                        (&mut input, work),
                    );
                    for processor in &mut running.processors {
                        processor.process(&mut input.left[..ahead], &mut input.right[..ahead]);
                    }
                }
                strip.buses[bus_number].state = BusState::Running(running);
            }
        }
        // The items that no instance hears, made where they are.
        let mut number = strip.current;
        while number < strip.items.len() && strip.items[number].start < to {
            let item = &mut strip.items[number];
            let (first, last) = (item.start.max(from), item.rings_until().min(to));
            if strip.bus_of_item[number].is_none() && first < last {
                let offset = (first - from) as usize;
                let length = (last - first) as usize;
                render_the_item(
                    strip_number,
                    number,
                    item,
                    specification,
                    feeds,
                    insertions,
                    (first, last),
                    work,
                    &mut scratch.left[offset..offset + length],
                    &mut scratch.right[offset..offset + length],
                );
            }
            number += 1;
        }
        // The items that an instance hears: put together, given to the effects, and added.
        for bus_number in 0..strip.buses.len() {
            let (first_item, last_item, start, end) = {
                let bus = &strip.buses[bus_number];
                (bus.first_item, bus.last_item, bus.start, bus.end)
            };
            let (first, last) = (from.max(start), to.min(end));
            if first < last {
                let length = (last - first) as usize;
                let mut input = std::mem::take(&mut strip.buses[bus_number].input);
                if let BusState::Running(running) = &mut strip.buses[bus_number].state {
                    input.silent_for(length);
                    // (The sound that goes in is `ahead` samples later than the sound that comes out is: see `RunningBus`.)
                    render_the_items_of_a_bus(
                        strip_number,
                        &mut strip.items,
                        (first_item, last_item),
                        specification,
                        feeds,
                        insertions,
                        (first + running.ahead, last + running.ahead),
                        (&mut input, work),
                    );
                    for processor in &mut running.processors {
                        processor.process(&mut input.left[..length], &mut input.right[..length]);
                    }
                    let offset = (first - from) as usize;
                    for (sum, sample) in scratch.left[offset..offset + length]
                        .iter_mut()
                        .zip(&input.left[..length])
                    {
                        *sum += *sample;
                    }
                    for (sum, sample) in scratch.right[offset..offset + length]
                        .iter_mut()
                        .zip(&input.right[..length])
                    {
                        *sum += *sample;
                    }
                }
                strip.buses[bus_number].input = input;
            }
            if to >= end {
                strip.buses[bus_number].state = BusState::Done;
            }
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

/// Makes the sound of item `number` of strip `strip_number` from sample `first` to sample `last` (not including it), which is in the item **or in the time its effects ring on after
/// it**, and **adds** it to `left` and `right` (which are as long as that: what is in them is the sound of other items, which ring on over this one). A silence makes nothing. `work`
/// is where it is made before it is added.
#[allow(clippy::too_many_arguments)]
fn render_the_item(
    strip_number: usize,
    number: usize,
    item: &mut Item,
    specification: &SoundGraphSpecification,
    feeds: &mut Box<dyn ClipFeeds + Send>,
    insertions: &mut Box<dyn Insertions + Send>,
    (first, last): (u64, u64),
    work: &mut Scratch,
    left: &mut [f32],
    right: &mut [f32],
) {
    let (item_start, item_end, rings_until) = (item.start, item.end, item.rings_until());
    let ItemKind::Clip { gain, live: slot } = &mut item.kind else {
        return;
    };
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
        pre_roll_samples: pre_roll_samples_of(specification.frame_rate, clip),
    });
    live.make_up_for_the_latency();
    let length = (last - first) as usize;
    work.silent_for(length);
    // (The first part is the clip itself, which its feed gives; what comes after it, while its effects ring on, is the effects given silence.)
    let of_the_clip = (item_end.saturating_sub(first)).min(last - first) as usize;
    if of_the_clip > 0 && !live.feed_has_ended {
        let given = live
            .feed
            .read(&mut work.left[..of_the_clip], &mut work.right[..of_the_clip]);
        // (What the feed did not give stays silent: the clip is as long as its place on the timeline whatever the file has.)
        live.feed_has_ended = given < of_the_clip;
    }
    for processor in &mut live.processors {
        processor.process(&mut work.left[..length], &mut work.right[..length]);
    }
    gain.apply(first - item_start, &mut work.left[..length], &mut work.right[..length]);
    for (sum, sample) in left.iter_mut().zip(&work.left[..length]) {
        *sum += *sample;
    }
    for (sum, sample) in right.iter_mut().zip(&work.right[..length]) {
        *sum += *sample;
    }
    if last == rings_until {
        *slot = None;
    }
}

/// Makes the sound that the items from `items.0` to `items.1` of strip `strip_number` have from sample `range.0` to sample `range.1` (not including it), put together, into the
/// start of `input`, which is silent when it is given and has room for that many samples.
#[allow(clippy::too_many_arguments)]
fn render_the_items_of_a_bus(
    strip_number: usize,
    items: &mut [Item],
    (first_item, last_item): (usize, usize),
    specification: &SoundGraphSpecification,
    feeds: &mut Box<dyn ClipFeeds + Send>,
    insertions: &mut Box<dyn Insertions + Send>,
    (from, to): (u64, u64),
    (input, work): (&mut Scratch, &mut Scratch),
) {
    for (number, item) in items.iter_mut().enumerate().take(last_item + 1).skip(first_item) {
        let (first, last) = (item.start.max(from), item.rings_until().min(to));
        if first >= last {
            continue;
        }
        let offset = (first - from) as usize;
        let length = (last - first) as usize;
        render_the_item(
            strip_number,
            number,
            item,
            specification,
            feeds,
            insertions,
            (first, last),
            work,
            &mut input.left[offset..offset + length],
            &mut input.right[offset..offset + length],
        );
    }
}
