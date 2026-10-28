// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The block graph itself.

use super::SAMPLE_RATE;
use std::collections::HashMap;

use super::feed::{ClipFeed, ClipFeeds, ClipRequest, SilentFeed};
use super::gain::GainCurve;
use super::insertions::{BlockProcessor, BusRequest, GroupRequest, Insertions};
use super::music::{TimedNote, events_of, give_the_notes_of_the_block, put_in_order};
use super::position::{pre_roll_samples_of, sample_at_frame};
use crate::composition::{NoteConsumer, TailDestination, TailRoute, tail_samples_of_a_chain};
use crate::media::export::{SoundGraphSpecification, SoundItemToExport};

/// The sound of a project, made a block at a time (see the top of this module's parent for what it does). It is given the specification once, and asked for the sound from its
/// start to its end, in blocks of whatever size is wanted: the sound is the same for any sizes.
///
/// **How a block is made** (phase 12, piece 12.3): the sound is **pulled** from the end. The sum of all the sound asks for what goes into it: the clips and instances of a track's effects that
/// are sent to it, and the groups whose parent it is. A group asks for what its members make in the same way, added together, and gives it to its effects. A group's effects can delay
/// the sound ([`BlockProcessor::latency`]): what a group asks of its members is the sound that many samples later than the sound it gives, so that what comes out is where it is.
pub struct SoundGraph {
    specification: SoundGraphSpecification,
    feeds: Box<dyn ClipFeeds + Send>,
    insertions: Box<dyn Insertions + Send>,
    strips: Vec<Strip>,
    /// The clip groups and track groups that have sound effects: each hears what its members make, added together.
    groups: Vec<GroupNode>,
    total_samples: u64,
    /// How many samples at the start are made and not heard (see `SoundGraphSpecification::lead_in_frames`).
    lead_in_samples: u64,
    position: u64,
    /// Buffers that are not in use, so that the sound of a block is made without asking for room for it.
    spare: Vec<Scratch>,
    /// Where the sound of one item is made before it is added to what is there (items that ring on overlap the ones after them).
    work: Scratch,
}

/// The sound of one strip, or of the members of a group, while a block is made, before it is added to the others.
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

    /// Adds the first `count` samples of `other` to the first `count` of these, from `offset` on.
    fn add_at(&mut self, offset: usize, other: &Scratch, count: usize) {
        for (sum, sample) in self.left[offset..offset + count].iter_mut().zip(&other.left[..count]) {
            *sum += *sample;
        }
        for (sum, sample) in self.right[offset..offset + count].iter_mut().zip(&other.right[..count]) {
            *sum += *sample;
        }
    }
}

/// From the sample `from` on, the sound of something that rings goes to `to`: a group, or the sum of all the sound (`Some(None)`), or nowhere (`None`).
#[derive(Clone, Copy)]
struct Segment {
    from: u64,
    to: Option<Option<usize>>,
}

/// Where the sound of an item or of an instance of a track's effects goes, when it does not all go to one place (phase 12, piece 12.3d: the tail policies): stretch after stretch, and over
/// the edge fade after the beginning of a stretch the sound goes to both the one before it and this one, less and less to the one before and more and more to this.
struct Routes {
    segments: Vec<Segment>,
    /// How many samples the crossfade between two stretches lasts.
    fade: u64,
}

impl Routes {
    fn new(
        start: u64,
        sent_to: Option<usize>,
        routes: &[TailRoute],
        frame_rate: crate::project::FrameRate,
        fade: u64,
    ) -> Self {
        let mut segments = vec![Segment {
            from: start,
            to: Some(sent_to),
        }];
        segments.extend(routes.iter().map(|route| Segment {
            from: sample_at_frame(frame_rate, route.from_frame),
            to: match route.to {
                TailDestination::Group(group) => Some(Some(group)),
                TailDestination::Sum => Some(None),
                TailDestination::Cut => None,
            },
        }));
        Self { segments, fade }
    }

    /// Whether any of the sound goes to `destination`.
    fn goes_to(&self, destination: Option<usize>) -> bool {
        self.segments.iter().any(|segment| segment.to == Some(destination))
    }

    /// The sample at which the sound is cut off, if it is.
    fn is_cut_at(&self) -> Option<u64> {
        self.segments
            .iter()
            .find(|segment| segment.to.is_none())
            .map(|segment| segment.from)
    }

    /// How much of the sound at sample `at` goes to `destination`: 0 where it does not go, 1 where it all does, and between over the fade at the edge of a stretch.
    fn weight(&self, destination: Option<usize>, at: u64) -> f32 {
        let mut total = 0.0;
        for (number, segment) in self.segments.iter().enumerate() {
            if segment.to != Some(destination) || at < segment.from {
                continue;
            }
            let coming_in = if number == 0 || self.fade == 0 || at >= segment.from + self.fade {
                1.0
            } else {
                (at - segment.from) as f32 / self.fade as f32
            };
            let going_out = match self.segments.get(number + 1) {
                Some(next) if at >= next.from => {
                    if self.fade == 0 || at >= next.from + self.fade {
                        0.0
                    } else {
                        1.0 - (at - next.from) as f32 / self.fade as f32
                    }
                }
                _ => 1.0,
            };
            total += coming_in * going_out;
        }
        total
    }

    /// The sample after the last at which any of the sound goes anywhere: where the last stretch that is a cut has faded out.
    fn the_end_of_what_goes_anywhere(&self) -> Option<u64> {
        self.is_cut_at().map(|at| at + self.fade)
    }
}

/// What an item or an instance with routes has made, which is kept until every place it goes to has taken what it takes of it: the sound from sample `first` on.
#[derive(Default)]
struct Cache {
    first: u64,
    left: Vec<f32>,
    right: Vec<f32>,
}

impl Cache {
    /// The sample after the last one that is kept (the first, if none is).
    fn end(&self) -> u64 {
        self.first + self.left.len() as u64
    }

    fn append(&mut self, first: u64, left: &[f32], right: &[f32]) {
        if self.left.is_empty() {
            self.first = first;
        }
        assert_eq!(self.end(), first, "what is made is made in order");
        self.left.extend_from_slice(left);
        self.right.extend_from_slice(right);
    }

    /// Lets go of what is before `sample`.
    fn drop_before(&mut self, sample: u64) {
        let count = (sample.saturating_sub(self.first) as usize).min(self.left.len());
        if count > 0 {
            self.left.drain(..count);
            self.right.drain(..count);
            self.first += count as u64;
        }
    }

    /// Adds what goes to `destination` according to `routes` of the samples from `from` up to `to` to the start of `part`.
    fn add_what_goes_to(
        &self,
        routes: &Routes,
        destination: Option<usize>,
        (from, to): (u64, u64),
        part: &mut Scratch,
    ) {
        for at in from.max(self.first)..to.min(self.end()) {
            let weight = routes.weight(destination, at);
            if weight > 0.0 {
                let (place, kept) = ((at - from) as usize, (at - self.first) as usize);
                part.left[place] += self.left[kept] * weight;
                part.right[place] += self.right[kept] * weight;
            }
        }
    }
}

/// An item or an instance whose sound does not all go to one place.
struct Routed {
    routes: Routes,
    cache: Cache,
}

impl Routed {
    /// The sample from which what is kept is not looked at again: nothing before `block_start` is, because everything that asks for sound asks for it from the start of the block on at the earliest.
    fn is_not_needed_before(&mut self, block_start: u64) {
        self.cache.drop_before(block_start);
    }
}

struct Strip {
    items: Vec<Item>,
    /// The first item that has not ended yet at the start of the block that is being made.
    current: usize,
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
    /// Where what it makes goes: the group that the clips it hears are sent to, if they are (all of them are sent to the same place), or the sum of all the sound.
    sent_to: Option<usize>,
    /// The first sample of the first item, and the sample just after the last that it makes: the last item has rung on, and the effects of the bus after it.
    start: u64,
    end: u64,
    state: BusState,
    /// The sound of the items, put together, while a block is made, before the effects are given it.
    input: Scratch,
    /// Where what it makes goes, if it does not all go to `sent_to`.
    routed: Option<Routed>,
    /// The notes and controllers that its first synthesizer plays (phase 13: those of the MIDI clips that it hears and that the plan says it plays), in samples from the start of the sound.
    events: Vec<TimedNote>,
}

enum BusState {
    /// Not heard yet: its effects are made when it is.
    Waiting,
    Running(RunningEffects),
    /// Heard to its end: its effects are let go of.
    Done,
}

/// The effects of an instance that is being heard (of a bus, or of a group).
struct RunningEffects {
    processors: Vec<Box<dyn BlockProcessor>>,
    /// How many samples ahead of the sound that comes out the sound that goes in is made: the latency of the effects (see [`BlockProcessor::latency`]), which they are made up for by.
    ahead: u64,
}

/// A clip group or a track group that has sound effects: what its members make, added together, goes through one instance of its effects, which is made when the first of them is
/// first heard and let go of when the last has rung on and the effects have rung on after it.
struct GroupNode {
    /// The group that holds this one and has sound effects too.
    parent: Option<usize>,
    /// The first sample of the first thing it hears, and the sample just after the last that it makes: everything it hears has rung on, and its effects after it. (`start == end`: it
    /// hears nothing.)
    start: u64,
    end: u64,
    state: BusState,
    /// The clip that is first heard of those it hears (its strip and its item), which is where its effects begin (see [`GroupRequest`]).
    first_clip: Option<(usize, usize)>,
    /// The notes and controllers that its first synthesizer plays (see `Bus::events`).
    events: Vec<TimedNote>,
}

struct Item {
    /// The first sample of the item, and the sample just after its last.
    start: u64,
    end: u64,
    /// Where the sound of the item goes (see `SoundClipToExport::sent_to`).
    sent_to: Option<usize>,
    /// How many samples its effects go on making sound after it (phase 12: the tails of its own effects, added up).
    tail: u64,
    kind: ItemKind,
    /// Where what it makes goes, if it does not all go to `sent_to`.
    routed: Option<Routed>,
    /// For a MIDI clip whose own effects have the synthesizer that plays its notes: the notes and controllers, in samples from the start of the sound. Empty for the others.
    events: Vec<TimedNote>,
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
    fn make_up_for_the_latency(&mut self, events: &[TimedNote], item_start: u64) {
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
        // (The samples that make up for the latency are the sound from the start of the clip on: the notes are those of the same stretch.)
        give_the_notes_of_the_block(&mut self.processors, events, item_start, latency);
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
        insertions: Box<dyn Insertions + Send>,
    ) -> Self {
        let frame_rate = specification.frame_rate;
        let group_count = specification.groups.len();
        let fade = u64::from(specification.edge_fade_milliseconds) * u64::from(SAMPLE_RATE) / 1000;
        let strips: Vec<Strip> = specification
            .strips
            .iter()
            .map(|strip| {
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
                        let (kind, mut tail, sent_to, routes) = match item {
                            SoundItemToExport::Silence { .. } => (ItemKind::Silence, 0, None, &[][..]),
                            SoundItemToExport::Clip(clip) => {
                                assert!(
                                    clip.sent_to.is_none_or(|group| group < group_count),
                                    "a clip is sent to a group that is there"
                                );
                                assert!(
                                    clip.tail_routes.iter().all(|route| match route.to {
                                        TailDestination::Group(group) => group < group_count,
                                        TailDestination::Sum | TailDestination::Cut => true,
                                    }),
                                    "the ring of a clip goes to a group that is there"
                                );
                                (
                                    ItemKind::Clip {
                                        gain: GainCurve::new(&clip.loudness, frame_rate),
                                        live: None,
                                    },
                                    tail_samples_of_a_chain(&clip.effects),
                                    clip.sent_to,
                                    &clip.tail_routes[..],
                                )
                            }
                        };
                        let routed = (!routes.is_empty()).then(|| Routed {
                            routes: Routes::new(start, sent_to, routes, frame_rate, fade),
                            cache: Cache::default(),
                        });
                        // (A ring that is cut ends where the cut has faded out: its effects are not kept for the rest of their tail.)
                        if let Some(cut_off) = routed
                            .as_ref()
                            .and_then(|routed| routed.routes.the_end_of_what_goes_anywhere())
                        {
                            tail = tail.min(cut_off.saturating_sub(end));
                        }
                        Item {
                            start,
                            end,
                            sent_to,
                            tail,
                            kind,
                            routed,
                            events: Vec::new(),
                        }
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
                        let sent_to = bus.sent_to;
                        assert!(
                            sent_to.is_none_or(|group| group < group_count),
                            "a bus is sent to a group that is there"
                        );
                        let start = items[bus.first_item].start;
                        let mut end = rings_on_until + tail_samples_of_a_chain(&bus.effects);
                        let routed = (!bus.tail_routes.is_empty()).then(|| Routed {
                            routes: Routes::new(start, sent_to, &bus.tail_routes, frame_rate, fade),
                            cache: Cache::default(),
                        });
                        if let Some(cut_off) = routed
                            .as_ref()
                            .and_then(|routed| routed.routes.the_end_of_what_goes_anywhere())
                        {
                            end = end.min(cut_off);
                        }
                        Bus {
                            first_item: bus.first_item,
                            last_item: bus.last_item,
                            sent_to,
                            start,
                            end,
                            state: BusState::Waiting,
                            input: Scratch::default(),
                            routed,
                            events: Vec::new(),
                        }
                    })
                    .collect();
                Strip {
                    items,
                    current: 0,
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
        let mut strips = strips;
        let mut groups = groups_of(&specification, &strips);
        give_the_synthesizers_their_notes(&specification, &mut strips, &mut groups);
        Self {
            total_samples: sample_at_frame(frame_rate, specification.total_frame_count),
            lead_in_samples: sample_at_frame(frame_rate, specification.lead_in_frames),
            specification,
            feeds,
            insertions,
            strips,
            groups,
            position: 0,
            spare: Vec::new(),
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
        // (What has rung on before this block is not looked at again. Sound that is asked for ahead of the block, for the effects that delay it, is later than that.)
        for strip in &mut self.strips {
            while strip.current < strip.items.len() && strip.items[strip.current].rings_until() <= from {
                strip.current += 1;
            }
        }
        let mut mix = self.take_a_buffer(count);
        self.render_what_goes_to(None, (from, to), &mut mix);
        for (sum, sample) in left[..count].iter_mut().zip(&mix.left[..count]) {
            *sum += *sample;
        }
        for (sum, sample) in right[..count].iter_mut().zip(&mix.right[..count]) {
            *sum += *sample;
        }
        self.give_back(mix);
        // (What the items and instances with routes made is not asked for from before the end of this block again.)
        for strip in &mut self.strips {
            for item in &mut strip.items {
                if let Some(routed) = &mut item.routed {
                    routed.is_not_needed_before(to);
                }
            }
            for bus in &mut strip.buses {
                if let Some(routed) = &mut bus.routed {
                    routed.is_not_needed_before(to);
                }
            }
        }
        self.position = to;
        count
    }

    /// A buffer with room for `count` samples, silent.
    fn take_a_buffer(&mut self, count: usize) -> Scratch {
        let mut buffer = self.spare.pop().unwrap_or_default();
        buffer.silent_for(count);
        buffer
    }

    fn give_back(&mut self, buffer: Scratch) {
        self.spare.push(buffer);
    }

    /// Makes the sound that goes to `destination` (a group, or `None`: the sum of all the sound) from sample `window.0` to sample `window.1` (not including it) and **adds** it to the
    /// start of `out`: what the tracks send to it (each track's volume and mutes are done to what it sends, after its effects), and what the groups that are in it make.
    fn render_what_goes_to(&mut self, destination: Option<usize>, window: (u64, u64), out: &mut Scratch) {
        let length = (window.1 - window.0) as usize;
        for strip_number in 0..self.strips.len() {
            let mut part = self.take_a_buffer(length);
            if self.render_strip_part(strip_number, destination, window, &mut part) {
                let strip = &mut self.strips[strip_number];
                // (The volume and the mute of the track come after its effects: what they remember goes on while it is muted.)
                strip
                    .gain
                    .apply(window.0, &mut part.left[..length], &mut part.right[..length]);
                for &(first, end) in &strip.muted {
                    let (first, end) = (first.max(window.0), end.min(window.1));
                    if first < end {
                        let (first, end) = ((first - window.0) as usize, (end - window.0) as usize);
                        part.left[first..end].fill(0.0);
                        part.right[first..end].fill(0.0);
                    }
                }
                out.add_at(0, &part, length);
            }
            self.give_back(part);
        }
        for group_number in 0..self.groups.len() {
            if self.groups[group_number].parent == destination {
                self.render_group(group_number, window, out);
            }
        }
    }

    /// Makes the sound that the group `number` gives from sample `window.0` to sample `window.1` and **adds** it to the start of `out`. Its effects are made when the first of what it
    /// hears is first wanted, and let go of when the last has rung on and they have rung on after it.
    fn render_group(&mut self, number: usize, window: (u64, u64), out: &mut Scratch) {
        let (from, to) = window;
        let (start, end) = (self.groups[number].start, self.groups[number].end);
        if matches!(self.groups[number].state, BusState::Done) {
            return;
        }
        if matches!(self.groups[number].state, BusState::Waiting) && start < to && from < end {
            self.make_the_effects_of_the_group(number, from.max(start));
        }
        let (first, last) = (from.max(start), to.min(end));
        if first < last
            && let BusState::Running(running) = &self.groups[number].state
        {
            let length = (last - first) as usize;
            let ahead = running.ahead;
            // (The sound that goes in is `ahead` samples later than the sound that comes out is.)
            let mut input = self.take_a_buffer(length);
            self.render_what_goes_to(Some(number), (first + ahead, last + ahead), &mut input);
            let events = std::mem::take(&mut self.groups[number].events);
            if let BusState::Running(running) = &mut self.groups[number].state {
                give_the_notes_of_the_block(&mut running.processors, &events, first + ahead, length);
                for processor in &mut running.processors {
                    processor.process(&mut input.left[..length], &mut input.right[..length]);
                }
            }
            self.groups[number].events = events;
            out.add_at((first - from) as usize, &input, length);
            self.give_back(input);
        }
        if to >= end {
            self.groups[number].state = BusState::Done;
        }
    }

    /// Makes the effects of the group `number`, whose first sample is `first`, and gives them the sound that makes up for their latency: the sound that comes out of them from then on is
    /// where it is.
    fn make_the_effects_of_the_group(&mut self, number: usize, first: u64) {
        let processors = {
            let Self {
                specification,
                insertions,
                groups,
                ..
            } = self;
            let first_clip =
                groups[number]
                    .first_clip
                    .and_then(|(strip, item)| match &specification.strips[strip].items[item] {
                        SoundItemToExport::Clip(clip) => Some(clip),
                        SoundItemToExport::Silence { .. } => None,
                    });
            match first_clip {
                Some(first_clip) => insertions.for_group(&GroupRequest {
                    group: number,
                    first_clip,
                    effects: &specification.groups[number].effects,
                }),
                None => Vec::new(),
            }
        };
        let ahead: usize = processors.iter().map(|processor| processor.latency()).sum();
        let mut running = RunningEffects {
            processors,
            ahead: ahead as u64,
        };
        if ahead > 0 {
            // (What the effects are given before they make the first sample that is heard: the sound that comes out of them from then on is where it is.)
            let mut input = self.take_a_buffer(ahead);
            self.render_what_goes_to(Some(number), (first, first + ahead as u64), &mut input);
            give_the_notes_of_the_block(&mut running.processors, &self.groups[number].events, first, ahead);
            for processor in &mut running.processors {
                processor.process(&mut input.left[..ahead], &mut input.right[..ahead]);
            }
            self.give_back(input);
        }
        self.groups[number].state = BusState::Running(running);
    }

    /// Makes the sound of the items of strip `strip_number` that go to `destination` from sample `window.0` to sample `window.1` (not including it) into the start of `part`, which is
    /// silent when it is given, and says whether there was any.
    fn render_strip_part(
        &mut self,
        strip_number: usize,
        destination: Option<usize>,
        (from, to): (u64, u64),
        part: &mut Scratch,
    ) -> bool {
        let Self {
            specification,
            feeds,
            insertions,
            strips,
            work,
            spare,
            ..
        } = self;
        let strip = &mut strips[strip_number];
        let mut made_any = false;
        let mut everything = Everything {
            specification,
            feeds,
            insertions,
            work,
            spare,
        };
        // The instances of the effects of the track that are first heard in this block are made, and given the sound that makes up for their latency.
        for bus_number in 0..strip.buses.len() {
            let bus = &strip.buses[bus_number];
            if bus.routed.is_none()
                && bus.sent_to == destination
                && matches!(bus.state, BusState::Waiting)
                && bus.start < to
                && from < bus.end
            {
                start_the_bus(strip, strip_number, bus_number, from.max(bus.start), &mut everything);
            }
        }
        // The items that no instance hears, made where they are.
        let mut number = strip.current;
        while number < strip.items.len() && strip.items[number].start < to {
            let item = &mut strip.items[number];
            if strip.bus_of_item[number].is_none() {
                if item.routed.is_none() {
                    let (first, last) = (item.start.max(from), item.rings_until().min(to));
                    if item.sent_to == destination && first < last {
                        let offset = (first - from) as usize;
                        let length = (last - first) as usize;
                        render_the_item(
                            strip_number,
                            number,
                            item,
                            everything.specification,
                            everything.feeds,
                            everything.insertions,
                            (first, last),
                            everything.work,
                            &mut part.left[offset..offset + length],
                            &mut part.right[offset..offset + length],
                        );
                        made_any = true;
                    }
                } else if item
                    .routed
                    .as_ref()
                    .is_some_and(|routed| routed.routes.goes_to(destination))
                {
                    // An item whose sound goes to several places is made once, in order, as far as anyone asks, and what is made is kept for each place to take its share.
                    let rendered_to = item.routed.as_ref().map_or(0, |routed| routed.cache.end());
                    let (first, last) = (item.start.max(rendered_to), item.rings_until().min(to));
                    if first < last {
                        let length = (last - first) as usize;
                        let mut made = everything.spare.pop().unwrap_or_default();
                        made.silent_for(length);
                        render_the_item(
                            strip_number,
                            number,
                            item,
                            everything.specification,
                            everything.feeds,
                            everything.insertions,
                            (first, last),
                            everything.work,
                            &mut made.left[..length],
                            &mut made.right[..length],
                        );
                        if let Some(routed) = &mut item.routed {
                            routed.cache.append(first, &made.left[..length], &made.right[..length]);
                        }
                        everything.spare.push(made);
                    }
                    if let Some(routed) = &item.routed {
                        routed
                            .cache
                            .add_what_goes_to(&routed.routes, destination, (from, to), part);
                        made_any = true;
                    }
                }
            }
            number += 1;
        }
        // The items that an instance hears: put together, given to the effects, and added.
        for bus_number in 0..strip.buses.len() {
            let (start, end, sent_to) = {
                let bus = &strip.buses[bus_number];
                (bus.start, bus.end, bus.sent_to)
            };
            if strip.buses[bus_number].routed.is_none() {
                if sent_to != destination {
                    continue;
                }
                let (first, last) = (from.max(start), to.min(end));
                if first < last {
                    let length = (last - first) as usize;
                    let offset = (first - from) as usize;
                    let (left, right) = (
                        &mut part.left[offset..offset + length],
                        &mut part.right[offset..offset + length],
                    );
                    if run_the_bus(
                        strip,
                        strip_number,
                        bus_number,
                        (first, last),
                        &mut everything,
                        left,
                        right,
                    ) {
                        made_any = true;
                    }
                }
                if to >= end {
                    strip.buses[bus_number].state = BusState::Done;
                }
            } else if strip.buses[bus_number]
                .routed
                .as_ref()
                .is_some_and(|routed| routed.routes.goes_to(destination))
            {
                // An instance whose sound goes to several places is made once, in order, as far as anyone asks, and what is made is kept for each place to take its share.
                let rendered_to = strip.buses[bus_number]
                    .routed
                    .as_ref()
                    .map_or(0, |routed| routed.cache.end());
                let (first, last) = (start.max(rendered_to), end.min(to));
                if first < last {
                    if matches!(strip.buses[bus_number].state, BusState::Waiting) {
                        start_the_bus(strip, strip_number, bus_number, first, &mut everything);
                    }
                    let length = (last - first) as usize;
                    let mut made = everything.spare.pop().unwrap_or_default();
                    made.silent_for(length);
                    run_the_bus(
                        strip,
                        strip_number,
                        bus_number,
                        (first, last),
                        &mut everything,
                        &mut made.left[..length],
                        &mut made.right[..length],
                    );
                    if let Some(routed) = &mut strip.buses[bus_number].routed {
                        routed.cache.append(first, &made.left[..length], &made.right[..length]);
                    }
                    everything.spare.push(made);
                    if last >= end {
                        strip.buses[bus_number].state = BusState::Done;
                    }
                }
                if let Some(routed) = &strip.buses[bus_number].routed {
                    routed
                        .cache
                        .add_what_goes_to(&routed.routes, destination, (from, to), part);
                    made_any = true;
                }
            }
        }
        made_any
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

/// What making sound needs that the strips do not have.
struct Everything<'graph> {
    specification: &'graph SoundGraphSpecification,
    feeds: &'graph mut Box<dyn ClipFeeds + Send>,
    insertions: &'graph mut Box<dyn Insertions + Send>,
    work: &'graph mut Scratch,
    spare: &'graph mut Vec<Scratch>,
}

/// Makes the effects of bus `bus_number` of `strip` (strip number `strip_number`), whose first sound is at sample `first`, and gives them the sound that makes up for their latency: the
/// sound that comes out of them from then on is where it is.
fn start_the_bus(
    strip: &mut Strip,
    strip_number: usize,
    bus_number: usize,
    first: u64,
    everything: &mut Everything<'_>,
) {
    let bus = &strip.buses[bus_number];
    let specification_of_the_bus = &everything.specification.strips[strip_number].buses[bus_number];
    let first_clip = (bus.first_item..=bus.last_item).find_map(|place| {
        match &everything.specification.strips[strip_number].items[place] {
            SoundItemToExport::Clip(clip) => Some(clip),
            SoundItemToExport::Silence { .. } => None,
        }
    });
    let processors = match first_clip {
        Some(first_clip) => everything.insertions.for_bus(&BusRequest {
            strip: strip_number,
            bus: bus_number,
            first_clip,
            effects: &specification_of_the_bus.effects,
        }),
        None => Vec::new(),
    };
    let ahead: usize = processors.iter().map(|processor| processor.latency()).sum();
    let mut running = RunningEffects {
        processors,
        ahead: ahead as u64,
    };
    if ahead > 0 {
        // (What the effects are given before they make the first sample that is heard: the sound that comes out of them from then on is where it is.)
        let mut input = everything.spare.pop().unwrap_or_default();
        input.silent_for(ahead);
        render_the_items_of_a_bus(
            strip_number,
            &mut strip.items,
            (bus.first_item, bus.last_item),
            everything.specification,
            everything.feeds,
            everything.insertions,
            (first, first + ahead as u64),
            (&mut input, everything.work),
        );
        give_the_notes_of_the_block(&mut running.processors, &strip.buses[bus_number].events, first, ahead);
        for processor in &mut running.processors {
            processor.process(&mut input.left[..ahead], &mut input.right[..ahead]);
        }
        everything.spare.push(input);
    }
    strip.buses[bus_number].state = BusState::Running(running);
}

/// Makes the sound of bus `bus_number` of `strip` from sample `window.0` to sample `window.1` (not including it), its items put together and given to its effects, and **adds** it to
/// `left` and `right`. Says whether the instance was there to make any (it has been made, and not let go of).
fn run_the_bus(
    strip: &mut Strip,
    strip_number: usize,
    bus_number: usize,
    (first, last): (u64, u64),
    everything: &mut Everything<'_>,
    left: &mut [f32],
    right: &mut [f32],
) -> bool {
    let (first_item, last_item) = (strip.buses[bus_number].first_item, strip.buses[bus_number].last_item);
    let length = (last - first) as usize;
    let mut input = std::mem::take(&mut strip.buses[bus_number].input);
    let events = std::mem::take(&mut strip.buses[bus_number].events);
    let mut made_any = false;
    if let BusState::Running(running) = &mut strip.buses[bus_number].state {
        input.silent_for(length);
        // (The sound that goes in is `ahead` samples later than the sound that comes out is: see `RunningEffects`.)
        render_the_items_of_a_bus(
            strip_number,
            &mut strip.items,
            (first_item, last_item),
            everything.specification,
            everything.feeds,
            everything.insertions,
            (first + running.ahead, last + running.ahead),
            (&mut input, everything.work),
        );
        give_the_notes_of_the_block(&mut running.processors, &events, first + running.ahead, length);
        for processor in &mut running.processors {
            processor.process(&mut input.left[..length], &mut input.right[..length]);
        }
        for (sum, sample) in left.iter_mut().zip(&input.left[..length]) {
            *sum += *sample;
        }
        for (sum, sample) in right.iter_mut().zip(&input.right[..length]) {
            *sum += *sample;
        }
        made_any = true;
    }
    strip.buses[bus_number].input = input;
    strip.buses[bus_number].events = events;
    made_any
}

/// Where the events of the music of a MIDI clip go: to the synthesizer among its own effects, to the one among the effects of the instance of its track that hears it, or to the one among those of
/// a group (see `NoteConsumer`).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum WhoPlays {
    Item(usize, usize),
    Bus(usize, usize),
    Group(usize),
}

/// Makes the list of events that the synthesizer of each chain plays, in samples from the start of the sound, from the music of the MIDI clips that the plan says it plays (phase 13, piece 13.4):
/// every consumer has its own list in order of time, with the notes numbered from 0.
fn give_the_synthesizers_their_notes(
    specification: &SoundGraphSpecification,
    strips: &mut [Strip],
    groups: &mut [GroupNode],
) {
    let mut collected: HashMap<WhoPlays, (Vec<TimedNote>, u32)> = HashMap::new();
    for (strip_number, strip) in specification.strips.iter().enumerate() {
        for (item_number, item) in strip.items.iter().enumerate() {
            let SoundItemToExport::Clip(clip) = item else {
                continue;
            };
            let Some(music) = &clip.music else {
                continue;
            };
            let (start, length) = {
                let built = &strips[strip_number].items[item_number];
                (built.start, built.end - built.start)
            };
            let who = match music.consumer {
                NoteConsumer::Clip => Some(WhoPlays::Item(strip_number, item_number)),
                NoteConsumer::Bus => strips[strip_number].bus_of_item[item_number]
                    .map(|bus_number| WhoPlays::Bus(strip_number, bus_number)),
                NoteConsumer::Group(group) => {
                    assert!(
                        group < groups.len(),
                        "a clip's notes are played by a group that is there"
                    );
                    Some(WhoPlays::Group(group))
                }
                NoteConsumer::Nobody => None,
            };
            if let Some(who) = who {
                let (events, next_id) = collected.entry(who).or_insert_with(|| (Vec::new(), 0));
                events.extend(events_of(music, start, length, next_id));
            }
        }
    }
    for (who, (mut events, _)) in collected {
        put_in_order(&mut events);
        match who {
            WhoPlays::Item(strip, item) => strips[strip].items[item].events = events,
            WhoPlays::Bus(strip, bus) => strips[strip].buses[bus].events = events,
            WhoPlays::Group(group) => groups[group].events = events,
        }
    }
}

/// The groups of the specification as the graph has them: where each hears from and until, and the first clip it hears.
fn groups_of(specification: &SoundGraphSpecification, strips: &[Strip]) -> Vec<GroupNode> {
    let count = specification.groups.len();
    for group in &specification.groups {
        assert!(
            group.parent.is_none_or(|parent| parent < count),
            "a group is in a group that is there"
        );
    }
    // How many groups are round each, so that what is inside is worked out before what holds it.
    let depth_of = |group: usize| {
        let (mut depth, mut at) = (0_usize, group);
        while let Some(parent) = specification.groups[at].parent {
            depth += 1;
            assert!(depth <= count, "a group is not in itself");
            at = parent;
        }
        depth
    };
    let mut innermost_first: Vec<usize> = (0..count).collect();
    innermost_first.sort_by_key(|&group| std::cmp::Reverse(depth_of(group)));

    // From where to where what is sent to each is heard, and the first clip.
    let mut extents: Vec<Option<(u64, u64)>> = vec![None; count];
    let mut first_clips: Vec<Option<(usize, usize)>> = vec![None; count];
    let widen = |extent: &mut Option<(u64, u64)>, start: u64, end: u64| {
        *extent = Some(extent.map_or((start, end), |(first, last)| (first.min(start), last.max(end))));
    };
    for (strip_number, strip) in strips.iter().enumerate() {
        for (item_number, item) in strip.items.iter().enumerate() {
            if strip.bus_of_item[item_number].is_none()
                && let ItemKind::Clip { .. } = &item.kind
            {
                widen_by_where_the_sound_goes(
                    &mut extents,
                    item.routed.as_ref(),
                    item.sent_to,
                    item.start,
                    item.rings_until(),
                    &widen,
                );
            }
            let ItemKind::Clip { .. } = &item.kind else {
                continue;
            };
            // (It is the first clip of the groups its sound goes to, and of the groups round those.)
            let routed_here = item.routed.as_ref().or_else(|| {
                strip.bus_of_item[item_number].and_then(|bus_number| strip.buses[bus_number].routed.as_ref())
            });
            let destinations = item.sent_to.into_iter().chain(
                routed_here
                    .into_iter()
                    .flat_map(|routed| routed.routes.segments.iter().filter_map(|segment| segment.to.flatten())),
            );
            for mut group in destinations {
                loop {
                    let is_earlier =
                        first_clips[group].is_none_or(|(strip, other)| item.start < strips[strip].items[other].start);
                    if is_earlier {
                        first_clips[group] = Some((strip_number, item_number));
                    }
                    match specification.groups[group].parent {
                        Some(parent) => group = parent,
                        None => break,
                    }
                }
            }
        }
        for bus in &strip.buses {
            widen_by_where_the_sound_goes(
                &mut extents,
                bus.routed.as_ref(),
                bus.sent_to,
                bus.start,
                bus.end,
                &widen,
            );
        }
    }
    let mut nodes: Vec<GroupNode> = (0..count)
        .map(|group| GroupNode {
            parent: specification.groups[group].parent,
            start: 0,
            end: 0,
            state: BusState::Waiting,
            first_clip: first_clips[group],
            events: Vec::new(),
        })
        .collect();
    for group in innermost_first {
        let (start, end) = extents[group].unwrap_or((0, 0));
        let end = if extents[group].is_some() {
            end + tail_samples_of_a_chain(&specification.groups[group].effects)
        } else {
            0
        };
        nodes[group].start = start;
        nodes[group].end = end;
        if extents[group].is_some()
            && let Some(parent) = specification.groups[group].parent
        {
            widen(&mut extents[parent], start, end);
        }
    }
    nodes
}

/// Widens the extents of the groups that something that makes sound from sample `start` to sample `end` sends it to: the whole of it to the group it is sent to, or each stretch of it
/// to the group it goes to, over the fade after it.
fn widen_by_where_the_sound_goes(
    extents: &mut [Option<(u64, u64)>],
    routed: Option<&Routed>,
    sent_to: Option<usize>,
    start: u64,
    end: u64,
    widen: &impl Fn(&mut Option<(u64, u64)>, u64, u64),
) {
    let Some(routed) = routed else {
        if let Some(group) = sent_to {
            widen(&mut extents[group], start, end);
        }
        return;
    };
    let segments = &routed.routes.segments;
    for (number, segment) in segments.iter().enumerate() {
        let Some(Some(group)) = segment.to else {
            continue;
        };
        let stretch_ends = segments
            .get(number + 1)
            .map_or(end, |next| (next.from + routed.routes.fade).min(end));
        if segment.from.max(start) < stretch_ends {
            widen(&mut extents[group], segment.from.max(start), stretch_ends);
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
    let is_music = clip.music.is_some();
    let live = slot.get_or_insert_with(|| Live {
        // (A MIDI clip has no file and no sound from before it: what is heard of it is its synthesizer's.)
        feed: if is_music {
            Box::new(SilentFeed)
        } else {
            feeds.open(&request)
        },
        processors: insertions.for_clip(&request),
        feed_has_ended: false,
        has_made_up_for_the_latency: false,
        pre_roll_samples: if is_music {
            0
        } else {
            pre_roll_samples_of(specification.frame_rate, clip)
        },
    });
    live.make_up_for_the_latency(&item.events, item_start);
    let length = (last - first) as usize;
    // (The effects are given their sound ahead by what they delay it: the notes that the sound has `first + latency` samples in are the ones for the synthesizer among them.)
    let latency: usize = live.processors.iter().map(|processor| processor.latency()).sum();
    give_the_notes_of_the_block(&mut live.processors, &item.events, first + latency as u64, length);
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
