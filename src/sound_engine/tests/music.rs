// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! MIDI clips in the block graph (phase 13, piece 13.4b; docs/engine-phase-13-midi-design.md, 13.4): the notes of a clip are given to the first synthesizer of the chain that the plan says plays
//! them (the clip's own, its track's bus, or a group's), at the sample they are at, **made up for what the effects before and after the synthesizer delay the sound by**; a MIDI clip has no feed.
//!
//! The synthesizer here is made up: it makes a sound of 1.0 from a note on to its note off, **at the sample of the note** and delayed by a latency of its own, so that a note that is a sample
//! too early or too late is heard.

use std::collections::VecDeque;
use std::sync::Mutex;

use super::groups::Scales;
use super::*;
use crate::composition::{Music, NoteConsumer, PlannedControl, PlannedNote};
use crate::media::export::SoundGroupToExport;
use crate::midi_file::MidiControlKind;
use crate::sound_engine::{NoteEvent, NoteEventKind, TakesTheNotes};

/// What a synthesizer was given in each block: the events, at their samples of the block.
type Log = Arc<Mutex<Vec<Vec<NoteEvent>>>>;

/// Makes 1.0 (the velocity over 127, so 1.0 for 127) from a note on to its note off, on top of what it is given, **delayed by `latency` samples**, and records what it is given.
struct Synth {
    latency: usize,
    level: f32,
    queue: VecDeque<f32>,
    pending: Vec<NoteEvent>,
    log: Log,
}

impl Synth {
    fn boxed(latency: usize, log: &Log) -> Box<dyn BlockProcessor> {
        Box::new(Self {
            latency,
            level: 0.0,
            queue: VecDeque::new(),
            pending: Vec::new(),
            log: Arc::clone(log),
        })
    }
}

impl BlockProcessor for Synth {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let mut next = 0;
        for place in 0..left.len() {
            while next < self.pending.len() && self.pending[next].time as usize <= place {
                match self.pending[next].what {
                    NoteEventKind::On { velocity, .. } => self.level = velocity,
                    NoteEventKind::Off { .. } | NoteEventKind::AllOff => self.level = 0.0,
                    NoteEventKind::Controller { .. } | NoteEventKind::PitchBend { .. } => {}
                }
                next += 1;
            }
            self.queue.push_back(self.level);
            let made = if self.queue.len() > self.latency {
                self.queue.pop_front().unwrap_or(0.0)
            } else {
                0.0
            };
            left[place] += made;
            right[place] += made;
        }
        self.pending.clear();
    }

    fn reset(&mut self) {}

    fn latency(&self) -> usize {
        self.latency
    }

    fn plays_notes(&self) -> bool {
        true
    }

    fn give_notes(&mut self, notes: &[NoteEvent]) {
        self.pending = notes.to_vec();
        self.log.lock().unwrap().push(notes.to_vec());
    }
}

/// Gives what it is given back `latency` samples later (and silence before).
struct Delays {
    queue: VecDeque<(f32, f32)>,
    latency: usize,
}

impl Delays {
    fn boxed(latency: usize) -> Box<dyn BlockProcessor> {
        Box::new(Self {
            queue: std::iter::repeat_n((0.0, 0.0), latency).collect(),
            latency,
        })
    }
}

impl BlockProcessor for Delays {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            self.queue.push_back((*l, *r));
            (*l, *r) = self.queue.pop_front().unwrap_or((0.0, 0.0));
        }
    }

    fn reset(&mut self) {}

    fn latency(&self) -> usize {
        self.latency
    }
}

/// A music clip of `frames` frames (30 a second, 1600 samples each) with these notes, each `(start, end)` in samples, played by `consumer`.
fn music_clip(frames: u64, notes: &[(u64, u64)], consumer: NoteConsumer) -> SoundItemToExport {
    let SoundItemToExport::Clip(mut clip) = clip(frames, LoudnessToExport::UNCHANGED) else {
        unreachable!("a clip");
    };
    clip.music = Some(Box::new(Music {
        notes: notes
            .iter()
            .map(|&(start, end)| PlannedNote {
                start,
                end,
                channel: 0,
                key: 60,
                velocity: 127,
            })
            .collect(),
        controls: Vec::new(),
        consumer,
    }));
    SoundItemToExport::Clip(clip)
}

/// What each place that makes the chains of effects makes: for a clip, for a bus and for a group. Counts the chains made.
struct Chains {
    for_clip: Box<dyn FnMut() -> Vec<Box<dyn BlockProcessor>> + Send>,
    for_bus: Box<dyn FnMut() -> Vec<Box<dyn BlockProcessor>> + Send>,
    for_group: Box<dyn FnMut() -> Vec<Box<dyn BlockProcessor>> + Send>,
    made: Arc<AtomicUsize>,
}

impl Chains {
    fn clips(for_clip: impl FnMut() -> Vec<Box<dyn BlockProcessor>> + Send + 'static) -> Self {
        Self {
            for_clip: Box::new(for_clip),
            for_bus: Box::new(Vec::new),
            for_group: Box::new(Vec::new),
            made: Arc::default(),
        }
    }

    fn buses(for_bus: impl FnMut() -> Vec<Box<dyn BlockProcessor>> + Send + 'static) -> Self {
        Self {
            for_clip: Box::new(Vec::new),
            for_bus: Box::new(for_bus),
            for_group: Box::new(Vec::new),
            made: Arc::default(),
        }
    }

    fn groups(for_group: impl FnMut() -> Vec<Box<dyn BlockProcessor>> + Send + 'static) -> Self {
        Self {
            for_clip: Box::new(Vec::new),
            for_bus: Box::new(Vec::new),
            for_group: Box::new(for_group),
            made: Arc::default(),
        }
    }
}

impl Insertions for Chains {
    fn for_clip(&mut self, _request: &ClipRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
        let made = (self.for_clip)();
        self.made.fetch_add(made.len(), Ordering::SeqCst);
        made
    }

    fn for_bus(&mut self, _request: &BusRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
        let made = (self.for_bus)();
        self.made.fetch_add(made.len(), Ordering::SeqCst);
        made
    }

    fn for_group(&mut self, _request: &GroupRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
        let made = (self.for_group)();
        self.made.fetch_add(made.len(), Ordering::SeqCst);
        made
    }
}

fn the_log() -> Log {
    Log::default()
}

/// What the synthesizer should make, over `samples` samples, for notes that begin and end where they say, counted from the sample `from` (1.0 inside a note, 0.0 outside).
fn what_notes_make(samples: usize, from: usize, notes: &[(u64, u64)]) -> Vec<f32> {
    let mut made = vec![0.0; samples];
    for &(start, end) in notes {
        let end = (from + end as usize).min(samples);
        if let Some(inside) = made.get_mut(from + start as usize..end) {
            inside.fill(1.0);
        }
    }
    made
}

const NOTES: [(u64, u64); 3] = [(0, 800), (4800, 6400), (7000, 7001)];

fn heard(specification: SoundGraphSpecification, chains: Chains, block: usize) -> Vec<f32> {
    let mut graph = SoundGraph::new(specification, Box::new(Ramps::default()), Box::new(chains));
    graph.render_to_the_end(block).0
}

#[test]
fn the_synthesizer_among_the_effects_of_a_clip_plays_its_notes_at_their_samples_whatever_the_size_of_the_block() {
    for block in [1, 7, 64, 480, 1600, 4096, 100_000] {
        let log = the_log();
        let chain = Arc::clone(&log);
        let specification = specification_with_buses(
            thirty(),
            10,
            vec![(vec![music_clip(10, &NOTES, NoteConsumer::Clip)], Vec::new())],
        );

        let sound = heard(
            specification,
            Chains::clips(move || vec![Synth::boxed(0, &chain)]),
            block,
        );

        assert_eq!(sound.len(), 16_000);
        assert_eq!(sound, what_notes_make(16_000, 0, &NOTES), "blocks of {block}");
    }
}

#[test]
fn a_music_clip_that_is_later_has_its_notes_counted_from_where_it_starts() {
    let log = the_log();
    let chain = Arc::clone(&log);
    let specification = specification_with_buses(
        thirty(),
        20,
        vec![(
            vec![
                silence(10),
                music_clip(10, &[(0, 800), (4800, 6400)], NoteConsumer::Clip),
            ],
            Vec::new(),
        )],
    );

    let sound = heard(
        specification,
        Chains::clips(move || vec![Synth::boxed(0, &chain)]),
        1000,
    );

    assert_eq!(sound, what_notes_make(32_000, 16_000, &[(0, 800), (4800, 6400)]));
}

#[test]
fn a_music_clip_has_no_feed_and_its_sound_is_silence_where_no_synthesizer_plays_it() {
    let ramps = Ramps::default();
    let specification = specification_with_buses(
        thirty(),
        10,
        vec![(vec![music_clip(10, &NOTES, NoteConsumer::Nobody)], Vec::new())],
    );
    let mut graph = SoundGraph::new(specification, Box::new(ramps.clone()), Box::new(NoInsertions));

    let (left, right) = graph.render_to_the_end(512);

    assert_eq!(ramps.opened.load(Ordering::SeqCst), 0, "no feed is opened for it");
    assert_eq!((left, right), (vec![0.0; 16_000], vec![0.0; 16_000]));
}

#[test]
fn a_music_clip_gives_no_notes_to_anyone_where_the_plan_says_nobody_plays_them() {
    let log = the_log();
    let chain = Arc::clone(&log);
    let specification = specification_with_buses(
        thirty(),
        10,
        vec![(vec![music_clip(10, &NOTES, NoteConsumer::Nobody)], Vec::new())],
    );

    let sound = heard(specification, Chains::clips(move || vec![Synth::boxed(0, &chain)]), 512);

    assert_eq!(sound, vec![0.0; 16_000]);
    assert!(log.lock().unwrap().iter().all(Vec::is_empty), "no event was given");
}

#[test]
fn the_notes_are_made_up_for_what_the_effects_before_and_after_the_synthesizer_delay_the_sound_by() {
    // (Each chain: what is before the synthesizer, its own latency, and what is after it.)
    let chains: [(usize, usize, usize); 6] = [
        (0, 0, 0),
        (0, 0, 480),
        (300, 0, 0),
        (300, 200, 100),
        (0, 333, 0),
        (1200, 17, 5000),
    ];
    for (before, own, after) in chains {
        for block in [64, 500, 1600, 9999] {
            let log = the_log();
            let chain = Arc::clone(&log);
            let specification = specification_with_buses(
                thirty(),
                10,
                vec![(vec![music_clip(10, &NOTES, NoteConsumer::Clip)], Vec::new())],
            );

            let sound = heard(
                specification,
                Chains::clips(move || {
                    let mut processors = Vec::new();
                    if before > 0 {
                        processors.push(Delays::boxed(before));
                    }
                    processors.push(Synth::boxed(own, &chain));
                    if after > 0 {
                        processors.push(Delays::boxed(after));
                    }
                    processors
                }),
                block,
            );

            assert_eq!(
                sound,
                what_notes_make(16_000, 0, &NOTES),
                "a delay of {before} before the synthesizer, one of {own} in it and one of {after} after it, blocks of {block}"
            );
        }
    }
}

#[test]
fn the_synthesizer_among_the_effects_of_a_bus_plays_the_notes_of_all_the_clips_that_it_hears_and_is_one_instance() {
    for latency in [0, 240] {
        let log = the_log();
        let chain = Arc::clone(&log);
        let chains = Chains::buses(move || vec![Synth::boxed(latency, &chain)]);
        let made = Arc::clone(&chains.made);
        let specification = specification_with_buses(
            thirty(),
            40,
            vec![(
                vec![
                    music_clip(10, &[(0, 800), (4800, 6400)], NoteConsumer::Bus),
                    music_clip(10, &[(100, 200)], NoteConsumer::Bus),
                    silence(10),
                    music_clip(10, &[(0, 1600)], NoteConsumer::Bus),
                ],
                vec![(0, 3)],
            )],
        );

        let sound = heard(specification, chains, 777);

        // (Four items of ten frames, the third a silence: the clips begin at samples 0, 16000 and 48000.)
        let mut wanted = what_notes_make(64_000, 0, &[(0, 800), (4800, 6400)]);
        for (place, value) in what_notes_make(64_000, 16_000, &[(100, 200)]).into_iter().enumerate() {
            wanted[place] += value;
        }
        for (place, value) in what_notes_make(64_000, 48_000, &[(0, 1600)]).into_iter().enumerate() {
            wanted[place] += value;
        }
        assert_eq!(sound, wanted, "a latency of {latency}");
        assert_eq!(made.load(Ordering::SeqCst), 1, "one instance for the run: {latency}");
    }
}

#[test]
fn the_synthesizer_among_the_effects_of_a_group_plays_the_notes_of_the_clips_of_different_tracks_that_are_sent_to_it() {
    for latency in [0, 160] {
        let log = the_log();
        let chain = Arc::clone(&log);
        let sent_to_the_group = |mut item: SoundItemToExport| {
            if let SoundItemToExport::Clip(clip) = &mut item {
                clip.sent_to = Some(0);
            }
            item
        };
        let mut specification = specification_with_buses(
            thirty(),
            20,
            vec![
                (
                    vec![sent_to_the_group(music_clip(10, &[(0, 800)], NoteConsumer::Group(0)))],
                    Vec::new(),
                ),
                (
                    vec![
                        silence(5),
                        sent_to_the_group(music_clip(10, &[(0, 400), (3200, 3600)], NoteConsumer::Group(0))),
                    ],
                    Vec::new(),
                ),
            ],
        );
        specification.groups = vec![SoundGroupToExport {
            effects: Vec::new(),
            parent: None,
        }];

        let sound = heard(
            specification,
            Chains::groups(move || vec![Synth::boxed(latency, &chain)]),
            640,
        );

        let mut wanted = what_notes_make(32_000, 0, &[(0, 800)]);
        for (place, value) in what_notes_make(32_000, 8_000, &[(0, 400), (3200, 3600)])
            .into_iter()
            .enumerate()
        {
            wanted[place] += value;
        }
        assert_eq!(sound, wanted, "a latency of {latency}");
    }
}

#[test]
fn only_the_first_synthesizer_of_a_chain_is_given_notes() {
    let (first, second) = (the_log(), the_log());
    let (chain_first, chain_second) = (Arc::clone(&first), Arc::clone(&second));
    let specification = specification_with_buses(
        thirty(),
        10,
        vec![(vec![music_clip(10, &NOTES, NoteConsumer::Clip)], Vec::new())],
    );

    let sound = heard(
        specification,
        Chains::clips(move || vec![Synth::boxed(0, &chain_first), Synth::boxed(0, &chain_second)]),
        1000,
    );

    assert_eq!(sound, what_notes_make(16_000, 0, &NOTES), "only the first makes sound");
    assert!(first.lock().unwrap().iter().any(|notes| !notes.is_empty()));
    assert!(
        second.lock().unwrap().iter().all(Vec::is_empty),
        "the second is given none"
    );
}

#[test]
fn a_stand_in_for_a_synthesizer_that_is_not_there_takes_the_notes_and_the_next_synthesizer_is_given_none() {
    let log = the_log();
    let chain = Arc::clone(&log);
    let specification = specification_with_buses(
        thirty(),
        10,
        vec![(vec![music_clip(10, &NOTES, NoteConsumer::Clip)], Vec::new())],
    );

    let sound = heard(
        specification,
        Chains::clips(move || vec![Box::new(TakesTheNotes), Synth::boxed(0, &chain)]),
        1000,
    );

    assert_eq!(sound, vec![0.0; 16_000]);
    assert!(log.lock().unwrap().iter().all(Vec::is_empty));
}

#[test]
fn an_effect_that_does_not_play_notes_is_not_given_any_and_does_its_work_on_what_the_synthesizer_makes() {
    let log = the_log();
    let chain = Arc::clone(&log);
    let specification = specification_with_buses(
        thirty(),
        10,
        vec![(vec![music_clip(10, &NOTES, NoteConsumer::Clip)], Vec::new())],
    );

    let sound = heard(
        specification,
        Chains::clips(move || vec![Box::new(Scales(0.25)), Synth::boxed(0, &chain), Box::new(Scales(0.5))]),
        1000,
    );

    // (Before the synthesizer an effect has nothing to scale; after it, it halves what the synthesizer makes.)
    let wanted: Vec<f32> = what_notes_make(16_000, 0, &NOTES)
        .into_iter()
        .map(|made| made * 0.5)
        .collect();
    assert_eq!(sound, wanted);
}

#[test]
fn the_volume_of_a_music_clip_is_done_to_what_its_synthesizer_makes_and_the_events_of_a_pedal_and_a_wheel_are_given() {
    let log = the_log();
    let chain = Arc::clone(&log);
    let SoundItemToExport::Clip(mut clip) = music_clip(10, &[(0, 1600)], NoteConsumer::Clip) else {
        unreachable!("a clip");
    };
    clip.loudness = LoudnessToExport::Constant(0.5);
    if let Some(music) = &mut clip.music {
        music.controls = vec![
            PlannedControl {
                at: 100,
                channel: 2,
                what: MidiControlKind::Controller { number: 64, value: 127 },
            },
            PlannedControl {
                at: 100,
                channel: 3,
                what: MidiControlKind::PitchBend { value: 9000 },
            },
        ];
    }
    let specification = specification_with_buses(thirty(), 10, vec![(vec![SoundItemToExport::Clip(clip)], Vec::new())]);

    let sound = heard(
        specification,
        Chains::clips(move || vec![Synth::boxed(0, &chain)]),
        100_000,
    );

    assert_eq!(
        sound,
        what_notes_make(16_000, 0, &[(0, 1600)])
            .into_iter()
            .map(|made| made * 0.5)
            .collect::<Vec<_>>()
    );
    let given: Vec<NoteEvent> = log.lock().unwrap().iter().flatten().copied().collect();
    assert_eq!(
        given.len(),
        4,
        "the note on, the note off, the pedal and the wheel: {given:?}"
    );
    assert!(given.iter().any(|event| matches!(
        event.what,
        NoteEventKind::Controller {
            channel: 2,
            number: 64,
            value: 127
        }
    ) && event.time == 100));
    assert!(given.iter().any(|event| matches!(
        event.what,
        NoteEventKind::PitchBend {
            channel: 3,
            value: 9000
        }
    )));
}

#[test]
fn the_notes_of_the_clips_of_one_synthesizer_are_numbered_so_that_each_note_off_ends_its_own_note() {
    let log = the_log();
    let chain = Arc::clone(&log);
    let specification = specification_with_buses(
        thirty(),
        20,
        vec![(
            vec![
                music_clip(10, &[(0, 400), (200, 600)], NoteConsumer::Bus),
                music_clip(10, &[(0, 400)], NoteConsumer::Bus),
            ],
            vec![(0, 1)],
        )],
    );

    heard(
        specification,
        Chains::buses(move || vec![Synth::boxed(0, &chain)]),
        100_000,
    );

    let given: Vec<NoteEvent> = log.lock().unwrap().iter().flatten().copied().collect();
    let mut ons = Vec::new();
    let mut offs = Vec::new();
    for event in given {
        match event.what {
            NoteEventKind::On { id, .. } => ons.push(id),
            NoteEventKind::Off { id, .. } => offs.push(id),
            _ => {}
        }
    }
    ons.sort_unstable();
    offs.sort_unstable();
    assert_eq!(ons, vec![0, 1, 2], "three notes, three numbers, in one list");
    assert_eq!(offs, ons, "each note off has the number of its note on");
}

#[test]
fn the_notes_of_the_second_of_two_buses_are_played_by_the_second_and_not_the_first() {
    let log = the_log();
    let chain = Arc::clone(&log);
    let specification = specification_with_buses(
        thirty(),
        30,
        vec![(
            vec![
                music_clip(10, &[(0, 800)], NoteConsumer::Bus),
                silence(10),
                music_clip(10, &[(100, 900)], NoteConsumer::Bus),
            ],
            vec![(0, 0), (2, 2)],
        )],
    );

    let sound = heard(specification, Chains::buses(move || vec![Synth::boxed(0, &chain)]), 700);

    let mut wanted = what_notes_make(48_000, 0, &[(0, 800)]);
    for (place, value) in what_notes_make(48_000, 32_000, &[(100, 900)]).into_iter().enumerate() {
        wanted[place] += value;
    }
    assert_eq!(sound, wanted);
}

#[test]
fn the_notes_sent_to_the_second_of_two_groups_are_played_by_the_second_and_not_the_first() {
    let log = the_log();
    let chain = Arc::clone(&log);
    let mut clip = music_clip(10, &[(0, 800), (4800, 6400)], NoteConsumer::Group(1));
    if let SoundItemToExport::Clip(clip) = &mut clip {
        clip.sent_to = Some(1);
    }
    let mut specification = specification_with_buses(thirty(), 10, vec![(vec![clip], Vec::new())]);
    specification.groups = vec![
        SoundGroupToExport {
            effects: Vec::new(),
            parent: None,
        },
        SoundGroupToExport {
            effects: Vec::new(),
            parent: None,
        },
    ];

    let sound = heard(
        specification,
        Chains::groups(move || vec![Synth::boxed(0, &chain)]),
        700,
    );

    assert_eq!(sound, what_notes_make(16_000, 0, &[(0, 800), (4800, 6400)]));
}

#[test]
fn a_note_that_the_plan_leaves_going_past_the_clip_is_ended_where_the_clip_ends_even_when_the_clip_is_not_the_first() {
    let log = the_log();
    let chain = Arc::clone(&log);
    // (A bus that hears a silence, the clip, and a silence: the note would sound into the last if it were not ended.)
    let specification = specification_with_buses(
        thirty(),
        30,
        vec![(
            vec![
                silence(10),
                music_clip(10, &[(15_000, 20_000)], NoteConsumer::Bus),
                silence(10),
            ],
            vec![(0, 2)],
        )],
    );

    let sound = heard(
        specification,
        Chains::buses(move || vec![Synth::boxed(0, &chain)]),
        1000,
    );

    assert_eq!(sound, what_notes_make(48_000, 16_000, &[(15_000, 16_000)]));
}

/// Notes whether it was told that it would be given sound from before the clip.
struct Remembers {
    told: Arc<AtomicUsize>,
}

impl BlockProcessor for Remembers {
    fn process(&mut self, _left: &mut [f32], _right: &mut [f32]) {}

    fn reset(&mut self) {}

    fn about_to_be_given_sound_from_before_the_clip(&mut self, _samples: usize) {
        self.told.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn a_music_clip_that_begins_in_the_middle_of_its_file_is_given_no_sound_from_before_it_for_its_effects_to_remember() {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    let log = the_log();
    let chain = Arc::clone(&log);
    let told = Arc::new(AtomicUsize::new(0));
    let told_by_the_chain = Arc::clone(&told);
    // (The effect of the clip asks for sound from before it, and the clip begins 10 frames into the file: a clip of sound would have 4 frames of it given to its effects.)
    let SoundItemToExport::Clip(mut clip) = clip_that_has_sound_from_before(10, 10, 6000, 1) else {
        unreachable!("a clip");
    };
    clip.music = Some(Box::new(Music {
        notes: vec![PlannedNote {
            start: 0,
            end: 800,
            channel: 0,
            key: 60,
            velocity: 127,
        }],
        controls: Vec::new(),
        consumer: NoteConsumer::Clip,
    }));
    let specification = specification_with_buses(thirty(), 10, vec![(vec![SoundItemToExport::Clip(clip)], Vec::new())]);

    let sound = heard(
        specification,
        Chains::clips(move || {
            vec![
                Synth::boxed(240, &chain),
                Box::new(Remembers {
                    told: Arc::clone(&told_by_the_chain),
                }),
            ]
        }),
        1000,
    );

    assert_eq!(sound, what_notes_make(16_000, 0, &[(0, 800)]));
    assert_eq!(
        told.load(Ordering::SeqCst),
        0,
        "nothing was told that sound from before comes"
    );
}
