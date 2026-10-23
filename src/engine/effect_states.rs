// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What shaders that depend on earlier frames keep from one frame to the next: the buffers of theirs that last (phase 6.4.2; see `docs/engine-phase-6-4-design.md`, K8 to K11).
//!
//! **A state is two textures for each buffer that lasts, `latest` and `older`**, kept under the identity that the call gives ([`EffectState`](super::EffectState)), and the frame that
//! the state was made at. What a call does with it depends on its own frame `f` and on the frame `n` the state was made at:
//!
//! * `f = n + 1`: **go on**. The call reads `latest` as the state before the frame and draws into `older`; afterwards they change places.
//! * `f = n`: **the same frame again** (a repaint, or a frame asked for twice). The call reads `older`, which is what the state was before frame `n`, and draws into `latest`: it makes
//!   the same picture, and does not advance.
//! * anything else, or no state yet: **start clear**. The call reads `older`, which is cleared, and draws into `latest`.
//!
//! Both of the last two leave `older` as the state before the frame. So the memory is two textures for each buffer, whatever the number of frames.
//!
//! **Checkpoints (EN-14, option 3; `docs/engine-phase-6-6-design.md`, section 4).** A state is exact only for a frame that comes after the one it was made at. The preview jumps (the person moves the
//! playhead), and then the state it has is of a frame that is not before the frame it is asked for. So the preview **keeps a copy of the state every so many frames of the effect's own clock** (a
//! checkpoint, in a texture of its own, as many as a budget of memory allows, the one used longest ago going first), and a call at a frame that is not the next goes on from **the nearest state that is not
//! after the frame before it**: the state it has, or a checkpoint (copied over the state before the frame). **It says when it is not the state of the frame before** (`approximate`), and the state it makes
//! is said to be approximate for the next 300 frames, as are the checkpoints that are made from it. The export takes no checkpoint and restores none: it makes every frame in order, and is exact.

use std::collections::{BTreeMap, HashMap};

use super::effect_shaders::texture_format_of;
use super::shader_structure::TargetFormat;

/// The most bytes of textures that all the states together may take. (When there are more, the one used least recently goes first; its effect starts clear at its next frame.)
pub const MOST_BYTES_OF_STATES: u64 = 256 * 1024 * 1024;

/// How many frames can be made without a state being used before it is let go of.
pub const FRAMES_BEFORE_A_STATE_IS_LET_GO_OF: u64 = 600;

/// What a call does with its state (see the module's documentation).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Continuation {
    GoOn,
    SameFrame,
    StartClear,
}

/// What a call at frame `frame` does with a state that was made at frame `kept_at` (`None`: there is none).
pub fn continuation(kept_at: Option<u64>, frame: u64) -> Continuation {
    match kept_at {
        Some(kept) if kept.checked_add(1) == Some(frame) => Continuation::GoOn,
        Some(kept) if kept == frame => Continuation::SameFrame,
        _ => Continuation::StartClear,
    }
}

/// How many frames of its own clock a state goes on being said to be not exact after a frame that was made from a state that was not the one of the frame before: the length that the lead-up on a
/// seek is at most (EN-14).
pub const FRAMES_AFTER_A_STATE_IS_NOT_EXACT: u64 = 300;

/// How often checkpoints of the states are taken, and how much memory they may take (EN-14): the settings of the program, which the window gives the engine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RulesOfCheckpoints {
    /// A checkpoint is taken after a frame whose number, on the clock of its effect, is a multiple of this. 0: none is taken.
    pub interval_in_frames: u64,
    /// The most bytes of textures that all the checkpoints may take together; the one used longest ago goes first.
    pub memory_budget_in_bytes: u64,
}

impl RulesOfCheckpoints {
    /// No checkpoints: what the engine does until it is told otherwise.
    pub const NONE: Self = Self {
        interval_in_frames: 0,
        memory_budget_in_bytes: 0,
    };
}

/// Where a call at a frame of the preview goes on from, and whether that is exact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoesOnFrom {
    /// The same frame again: from the state before it, which the state still has (as for [`Continuation::SameFrame`]).
    TheSameFrameAgain,
    /// The state it has, which is of a frame before the one asked for (as for [`Continuation::GoOn`]); `exact` if it is of the frame before.
    TheState { exact: bool },
    /// The checkpoint of this frame, which is copied over the state before the frame (and the call reads that, as it does when it starts clear); `exact` if it is of the frame before.
    ACheckpoint { frame: u64, exact: bool },
    /// Nothing: the state before the frame is clear. `exact` only for the first frame of the effect, whose state before it is clear.
    Nothing { exact: bool },
}

/// Where a call at `frame` goes on from, when the state it has is of the frame `kept_at` (`None`: it has none) and there are checkpoints of the frames `checkpoints`: **the nearest state that is not
/// of a frame after the one before `frame`**, the state it has if it is as near as a checkpoint is. (The state before the first frame is clear, and that is exact.)
pub fn where_to_go_on_from(kept_at: Option<u64>, checkpoints: &[u64], frame: u64) -> GoesOnFrom {
    if kept_at == Some(frame) {
        return GoesOnFrom::TheSameFrameAgain;
    }
    let Some(before) = frame.checked_sub(1) else {
        return GoesOnFrom::Nothing { exact: true };
    };
    let live = kept_at.filter(|kept| *kept <= before);
    let checkpoint = checkpoints.iter().copied().filter(|made_at| *made_at <= before).max();
    match (live, checkpoint) {
        (Some(live), Some(made_at)) if made_at > live => GoesOnFrom::ACheckpoint {
            frame: made_at,
            exact: made_at == before,
        },
        (Some(live), _) => GoesOnFrom::TheState { exact: live == before },
        (None, Some(made_at)) => GoesOnFrom::ACheckpoint {
            frame: made_at,
            exact: made_at == before,
        },
        (None, None) => GoesOnFrom::Nothing { exact: false },
    }
}

/// What came of looking whether the states that a chase made have caught up with the ones of the preview (see [`StatesOfEffects::take_over_what_has_caught_up`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatchingUp {
    /// Not yet: a state that the last frame of the preview used is not in the chase's store, or is made at an earlier frame. Nothing was taken.
    NotYet,
    /// The states that the last frame of the preview used (as many as this; none if it used none) went over from the chase's store to the preview's, and are exact.
    TakenOver(usize),
    /// They never will, for this reason: a state of the chase is at a later frame than the preview's, or has other buffers. Nothing was taken.
    NeverWill(String),
}

/// What a buffer that lasts is: how big, and in what format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferWanted {
    pub size: (u32, u32),
    pub format: TargetFormat,
}

impl BufferWanted {
    fn bytes(self) -> u64 {
        let pixel = texture_format_of(self.format).block_copy_size(None).unwrap_or(8);
        u64::from(self.size.0) * u64::from(self.size.1) * u64::from(pixel)
    }
}

struct PersistentBuffer {
    wanted: BufferWanted,
    latest: wgpu::Texture,
    older: wgpu::Texture,
}

struct State {
    /// The frame the state was made at; `None` before the first frame that used it was made.
    frame: Option<u64>,
    /// One for each buffer of the shader: `Some` for those that last.
    buffers: Vec<Option<PersistentBuffer>>,
    /// The number of frames made when it was last used.
    last_used: u64,
    /// The frames before this one, of the effect's own clock, are made from a state that is not exact (see [`FRAMES_AFTER_A_STATE_IS_NOT_EXACT`]); `None` if it is.
    approximate_until: Option<u64>,
    /// What the frame that was prepared and not committed yet makes of the state.
    pending: Option<Pending>,
}

/// What a call that was prepared for the preview will leave when its frame is committed.
struct Pending {
    approximate_until: Option<u64>,
    /// The copy of the state that is taken after the frame, if one is.
    checkpoint: Option<PendingCheckpoint>,
}

struct PendingCheckpoint {
    frame: u64,
    /// For each buffer of the state: `Some` for those that last, with what it is and the texture that the copy goes in.
    buffers: Vec<Option<(BufferWanted, wgpu::Texture)>>,
    approximate_until: Option<u64>,
}

/// A copy of a state, taken after a frame.
struct Checkpoint {
    buffers: Vec<Option<(BufferWanted, wgpu::Texture)>>,
    approximate_until: Option<u64>,
    /// The number of frames made when it was taken or last gone on from.
    last_used: u64,
}

impl Checkpoint {
    fn bytes(&self) -> u64 {
        self.buffers.iter().flatten().map(|(wanted, _)| wanted.bytes()).sum()
    }

    /// Whether it is a copy of buffers that are what `wanted` says (the sizes can change, and then it is of no use).
    fn is_of(&self, wanted: &[Option<BufferWanted>]) -> bool {
        self.buffers.len() == wanted.len()
            && self
                .buffers
                .iter()
                .zip(wanted)
                .all(|(kept, want)| kept.as_ref().map(|(buffer, _)| *buffer) == *want)
    }
}

/// Whether the frames before `until` are said not to be exact at `frame`.
fn is_still_approximate(until: Option<u64>, frame: u64) -> bool {
    until.is_some_and(|until| frame < until)
}

impl State {
    /// Whether the buffers of this state and of `other` are the same: the same places that last, each of the same size and format.
    fn has_the_buffers_of(&self, other: &State) -> bool {
        self.buffers.len() == other.buffers.len()
            && self.buffers.iter().zip(&other.buffers).all(|(ours, theirs)| {
                ours.as_ref().map(|buffer| buffer.wanted) == theirs.as_ref().map(|buffer| buffer.wanted)
            })
    }

    fn bytes(&self) -> u64 {
        self.buffers
            .iter()
            .flatten()
            .map(|buffer| 2 * buffer.wanted.bytes())
            .sum()
    }
}

/// What a call is given to run its passes with: where each buffer that lasts is read from and drawn into, and what has to be cleared first.
pub struct BuffersOfACall {
    pub continuation: Continuation,
    /// For each buffer of the shader, `Some` if it lasts: the state before the frame, which a pass reads before the pass that draws the buffer.
    pub base: Vec<Option<wgpu::TextureView>>,
    /// For each buffer of the shader, `Some` if it lasts: what the pass that draws the buffer draws into.
    pub target: Vec<Option<wgpu::TextureView>>,
    /// What is to be cleared before the first pass (the state before the frame, when the call starts clear).
    pub to_clear: Vec<wgpu::TextureView>,
    /// What is to be copied before the first pass: a checkpoint (the first of each pair) over the state before the frame (the second), when the call goes on from a checkpoint.
    pub to_restore: Vec<(wgpu::Texture, wgpu::Texture)>,
    /// What is to be copied after the last pass: the state that the frame makes (the first of each pair) into the texture of a checkpoint (the second).
    pub to_checkpoint: Vec<(wgpu::Texture, wgpu::Texture)>,
    /// Whether the state that the frame goes on from is not the one of the frame before (the picture is not exact: EN-14).
    pub approximate: bool,
}

/// The states of all the shaders that depend on earlier frames.
#[derive(Default)]
pub struct StatesOfEffects {
    states: HashMap<u64, State>,
    /// How many frames were made, which says how long ago a state was used.
    frames_made: u64,
    /// The checkpoints of each state (by its identity), by the frame they were taken after. They are kept apart from the states: a state that is let go of for not being used takes none of
    /// its checkpoints with it (a jump back to it later is what they are for).
    checkpoints: HashMap<u64, BTreeMap<u64, Checkpoint>>,
    rules: RulesOfCheckpoints,
}

fn texture_of(device: &wgpu::Device, wanted: BufferWanted, label: &str) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: wanted.size.0,
            height: wanted.size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: texture_format_of(wanted.format),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn view_of(texture: &wgpu::Texture) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

impl StatesOfEffects {
    /// How many states there are.
    pub fn len(&self) -> usize {
        self.states.len()
    }

    /// How many bytes of textures all the states take.
    pub fn bytes(&self) -> u64 {
        self.states.values().map(State::bytes).sum()
    }

    /// How many checkpoints there are, and how many bytes of textures they take up.
    pub fn checkpoints_kept(&self) -> (usize, u64) {
        let all = self.checkpoints.values().flat_map(BTreeMap::values);
        (all.clone().count(), all.map(Checkpoint::bytes).sum())
    }

    /// From now on checkpoints are taken and kept as `rules` say (those that there are stay, within the new budget, unless none are to be taken).
    pub fn set_the_rules_of_checkpoints(&mut self, rules: RulesOfCheckpoints) {
        self.rules = rules;
        if rules.interval_in_frames == 0 {
            self.checkpoints.clear();
        } else {
            self.evict_the_checkpoints_that_are_over_the_budget();
        }
    }

    /// Lets go of every checkpoint: what they are copies of is not there any more (the project was edited, or loaded, or the colours of the frame changed).
    pub fn forget_the_checkpoints(&mut self) {
        self.checkpoints.clear();
    }

    /// What the call at frame `frame` that goes on with the state `id` is given. `wanted` has one place for each buffer of the shader: `Some` for each that lasts, with what it is
    /// wanted as for this call (its size is worked out from the call's numbers, so it can change; a buffer that changes size, or a state that did not have it, starts clear).
    /// **Nothing is kept until [`Self::commit`]**: a frame that is not made is [`Self::abandon`]ed.
    ///
    /// This is for a frame that is not of the preview (the export's, whose frames are made in order from the first: it takes no checkpoint and restores none).
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        id: u64,
        frame: u64,
        wanted: &[Option<BufferWanted>],
    ) -> BuffersOfACall {
        self.prepare_for(device, id, frame, wanted, false)
    }

    /// The same for a frame of the preview, which jumps: **if checkpoints are to be taken** (see [`Self::set_the_rules_of_checkpoints`]) it goes on from the nearest state that is not after the frame before
    /// it (see [`where_to_go_on_from`]), takes a checkpoint after a frame that is a multiple of the interval, and says whether the picture is not exact (`approximate`). If none are to be taken it is
    /// [`Self::prepare`].
    pub fn prepare_for_the_preview(
        &mut self,
        device: &wgpu::Device,
        id: u64,
        frame: u64,
        wanted: &[Option<BufferWanted>],
    ) -> BuffersOfACall {
        self.prepare_for(device, id, frame, wanted, true)
    }

    fn prepare_for(
        &mut self,
        device: &wgpu::Device,
        id: u64,
        frame: u64,
        wanted: &[Option<BufferWanted>],
        for_the_preview: bool,
    ) -> BuffersOfACall {
        let checkpoints_are_kept = for_the_preview && self.rules.interval_in_frames > 0;
        let state = self.states.entry(id).or_insert_with(|| State {
            frame: None,
            buffers: Vec::new(),
            last_used: 0,
            approximate_until: None,
            pending: None,
        });
        state.last_used = self.frames_made;
        let mut continuation = continuation(state.frame, frame);
        // (A buffer that is not what it was, or that there was none of, has nothing to go on from: the whole call starts clear.)
        state.buffers.resize_with(wanted.len(), || None);
        let mut buffers_changed = false;
        for (place, want) in wanted.iter().enumerate() {
            let kept = state.buffers[place].as_ref().map(|buffer| buffer.wanted);
            if kept != *want {
                continuation = Continuation::StartClear;
                buffers_changed = true;
                state.buffers[place] = want.map(|wanted| PersistentBuffer {
                    wanted,
                    latest: texture_of(device, wanted, "engine: a buffer that lasts (latest)"),
                    older: texture_of(device, wanted, "engine: a buffer that lasts (older)"),
                });
            }
        }
        // Where the call goes on from, if checkpoints are kept: the nearest state that is not after the frame before it.
        let mut restore_from: Option<u64> = None;
        let mut approximate = false;
        let mut approximate_until_after = state.approximate_until;
        if checkpoints_are_kept {
            let kept = if buffers_changed { None } else { state.frame };
            let usable: Vec<u64> = self.checkpoints.get(&id).map_or_else(Vec::new, |checkpoints| {
                checkpoints
                    .iter()
                    .filter(|(_, checkpoint)| checkpoint.is_of(wanted))
                    .map(|(made_at, _)| *made_at)
                    .collect()
            });
            let (exact, carried_until) = match where_to_go_on_from(kept, &usable, frame) {
                GoesOnFrom::TheSameFrameAgain => {
                    continuation = Continuation::SameFrame;
                    (true, state.approximate_until)
                }
                GoesOnFrom::TheState { exact } => {
                    continuation = Continuation::GoOn;
                    (exact, state.approximate_until)
                }
                GoesOnFrom::ACheckpoint { frame: made_at, exact } => {
                    continuation = Continuation::StartClear;
                    restore_from = Some(made_at);
                    let checkpoint = self
                        .checkpoints
                        .get_mut(&id)
                        .and_then(|checkpoints| checkpoints.get_mut(&made_at))
                        .expect("the checkpoint was found among those that there are");
                    checkpoint.last_used = self.frames_made;
                    (exact, checkpoint.approximate_until)
                }
                GoesOnFrom::Nothing { exact } => {
                    continuation = Continuation::StartClear;
                    (exact, None)
                }
            };
            approximate = !exact || is_still_approximate(carried_until, frame);
            approximate_until_after = if exact {
                carried_until.filter(|until| frame + 1 < *until)
            } else {
                Some(
                    carried_until
                        .unwrap_or(0)
                        .max(frame.saturating_add(FRAMES_AFTER_A_STATE_IS_NOT_EXACT)),
                )
            };
        }
        let checkpoint_here = &self.checkpoints.get(&id);
        let takes_a_checkpoint = checkpoints_are_kept
            && frame.is_multiple_of(self.rules.interval_in_frames)
            && !checkpoint_here.is_some_and(|checkpoints| checkpoints.contains_key(&frame));
        let mut base = Vec::with_capacity(wanted.len());
        let mut target = Vec::with_capacity(wanted.len());
        let mut to_clear = Vec::new();
        let mut to_restore = Vec::new();
        let mut to_checkpoint = Vec::new();
        let mut checkpoint_buffers = Vec::with_capacity(wanted.len());
        for (place, buffer) in state.buffers.iter().enumerate() {
            match buffer {
                None => {
                    base.push(None);
                    target.push(None);
                    checkpoint_buffers.push(None);
                }
                Some(buffer) => {
                    let (read, written) = if continuation == Continuation::GoOn {
                        (&buffer.latest, &buffer.older)
                    } else {
                        (&buffer.older, &buffer.latest)
                    };
                    if continuation == Continuation::StartClear {
                        let to_copy_over_it = restore_from.and_then(|made_at| {
                            self.checkpoints
                                .get(&id)
                                .and_then(|checkpoints| checkpoints.get(&made_at))
                                .and_then(|checkpoint| checkpoint.buffers[place].as_ref())
                                .map(|(_, texture)| texture.clone())
                        });
                        match to_copy_over_it {
                            Some(checkpoint_texture) => to_restore.push((checkpoint_texture, buffer.older.clone())),
                            None => to_clear.push(view_of(&buffer.older)),
                        }
                    }
                    base.push(Some(view_of(read)));
                    target.push(Some(view_of(written)));
                    if takes_a_checkpoint {
                        let copy = texture_of(device, buffer.wanted, "engine: a checkpoint of a state");
                        to_checkpoint.push((written.clone(), copy.clone()));
                        checkpoint_buffers.push(Some((buffer.wanted, copy)));
                    } else {
                        checkpoint_buffers.push(None);
                    }
                }
            }
        }
        state.pending = checkpoints_are_kept.then(|| Pending {
            approximate_until: approximate_until_after,
            checkpoint: takes_a_checkpoint.then(|| PendingCheckpoint {
                frame,
                buffers: checkpoint_buffers,
                approximate_until: approximate_until_after,
            }),
        });
        BuffersOfACall {
            continuation,
            base,
            target,
            to_clear,
            to_restore,
            to_checkpoint,
            approximate,
        }
    }

    /// **The chase** (EN-14, the lead-up while the project plays): `chasing` is a store of its own whose states were made from frames before the frame that the preview shows, one after another, to
    /// catch up with the preview's. When every state that **the last frame of the preview used** is in it, **at the same frame** as the preview's and with the same buffers, they are what the
    /// preview's would be if it had made all the frames before: they go over to this store, in place of the preview's, and are exact. (All together or not at all: an effect that is exact next to
    /// one that is not would be a picture that is half right.) What the last frame did not use stays where it is, and is let go of with `chasing`.
    ///
    /// A state of `chasing` is at the same frame as the preview's after the frame that makes it so, whichever frames the preview made in the meantime, because it advances by one frame at a time and
    /// this is asked after each: it can only pass the preview's frame if the preview went back, which is a jump and ends the chase.
    pub fn take_over_what_has_caught_up(&mut self, chasing: &mut StatesOfEffects) -> CatchingUp {
        let Some(last_frame) = self.frames_made.checked_sub(1) else {
            return CatchingUp::TakenOver(0);
        };
        let needed: Vec<u64> = self
            .states
            .iter()
            .filter(|(_, state)| state.last_used == last_frame)
            .map(|(id, _)| *id)
            .collect();
        let mut all_have_caught_up = true;
        for id in &needed {
            let ours = &self.states[id];
            let Some(theirs) = chasing.states.get(id) else {
                all_have_caught_up = false;
                continue;
            };
            if !ours.has_the_buffers_of(theirs) {
                return CatchingUp::NeverWill(format!(
                    "the state {id:x} of the chase has other buffers than the preview's"
                ));
            }
            match theirs.frame.cmp(&ours.frame) {
                std::cmp::Ordering::Less => all_have_caught_up = false,
                std::cmp::Ordering::Equal => {}
                std::cmp::Ordering::Greater => {
                    return CatchingUp::NeverWill(format!(
                        "the state {id:x} of the chase is at a later frame than the preview's"
                    ));
                }
            }
        }
        if !all_have_caught_up {
            return CatchingUp::NotYet;
        }
        for id in &needed {
            let Some(mut state) = chasing.states.remove(id) else {
                continue;
            };
            state.last_used = self.frames_made;
            state.approximate_until = None;
            state.pending = None;
            self.states.insert(*id, state);
        }
        CatchingUp::TakenOver(needed.len())
    }

    /// The frame `frame` was made with the state `id`, as [`Self::prepare`] said: from now on the state is of that frame (and the checkpoint that the frame took is kept).
    pub fn commit(&mut self, id: u64, frame: u64, continuation: Continuation) {
        let Some(state) = self.states.get_mut(&id) else { return };
        if continuation == Continuation::GoOn {
            for buffer in state.buffers.iter_mut().flatten() {
                std::mem::swap(&mut buffer.latest, &mut buffer.older);
            }
        }
        state.frame = Some(frame);
        if let Some(pending) = state.pending.take() {
            state.approximate_until = pending.approximate_until;
            if let Some(checkpoint) = pending.checkpoint {
                self.checkpoints.entry(id).or_default().insert(
                    checkpoint.frame,
                    Checkpoint {
                        buffers: checkpoint.buffers,
                        approximate_until: checkpoint.approximate_until,
                        last_used: self.frames_made,
                    },
                );
            }
        }
    }

    /// The frame was not made: the state is let go of, and its effect starts clear at its next frame. (The checkpoints of the frames before it are what they were.)
    pub fn abandon(&mut self, id: u64) {
        self.states.remove(&id);
    }

    /// A frame has been made: the states that have not been used for a long time are let go of, and so, the one used longest ago first, are as many as it takes to be within the
    /// most bytes there may be; and the checkpoints are within the memory they may take.
    pub fn end_frame(&mut self) {
        self.end_frame_within(MOST_BYTES_OF_STATES);
    }

    /// The same, within `most_bytes` and not the most there may be.
    pub fn end_frame_within(&mut self, most_bytes: u64) {
        self.frames_made += 1;
        let now = self.frames_made;
        self.states
            .retain(|_, state| now.saturating_sub(state.last_used) <= FRAMES_BEFORE_A_STATE_IS_LET_GO_OF);
        while self.bytes() > most_bytes {
            let Some(oldest) = self
                .states
                .iter()
                .min_by_key(|(id, state)| (state.last_used, **id))
                .map(|(id, _)| *id)
            else {
                break;
            };
            self.states.remove(&oldest);
        }
        self.evict_the_checkpoints_that_are_over_the_budget();
    }

    /// Lets go of the checkpoint that was used longest ago, again and again, until they take no more than the budget says.
    fn evict_the_checkpoints_that_are_over_the_budget(&mut self) {
        while self.checkpoints_kept().1 > self.rules.memory_budget_in_bytes {
            let Some((id, made_at)) = self
                .checkpoints
                .iter()
                .flat_map(|(id, checkpoints)| {
                    checkpoints
                        .iter()
                        .map(move |(made_at, checkpoint)| (checkpoint.last_used, *id, *made_at))
                })
                .min()
                .map(|(_, id, made_at)| (id, made_at))
            else {
                break;
            };
            if let Some(checkpoints) = self.checkpoints.get_mut(&id) {
                checkpoints.remove(&made_at);
                if checkpoints.is_empty() {
                    self.checkpoints.remove(&id);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
