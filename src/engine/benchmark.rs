// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The benchmark of the engine: how many frames to the second it makes of a stack that is typical of a project (four layers, two blurs and a colour grade by a
//! lookup), and what the kinds of pass cost, on the graphics adapter it is given.
//!
//! **What it measures.** Frames are made one after another, each one all the way to the pixels in the program's memory (the way the engine makes frames for the
//! window), and the time of each is the engine's own (`FrameStatistics::time_taken`: from taking the request to having the frame in hand, the read-back of the
//! pixels included). The scenes are a ladder: the same four pictures with one more thing done to them in each, so that what a pass costs is what the next scene
//! takes more than the one before it.
//!
//! **What it is for.** The gate of phase 1 (EN-07) is that the whole stack plays at **1080p and 60 frames to the second on the integrated graphics** of the
//! maintainer's computer. That is for the maintainer to measure, with the example (`cargo run --release --example engine_benchmark`), which uses the graphics
//! card; the tests use the software adapter, at a size that a processor can make, only to see that the benchmark works. (The numbers of the software adapter
//! say what a processor takes, not what a card does.)
//!
//! **What stands in for what.** The two blurs are effects that are shaders (one along the rows and one down the columns, 17 reads each, so a blur is two passes);
//! the lookup table of a colour grade is **not** here: a 3-D table is a resource that an effect of this first form does not have, and the pass that stands in for
//! it makes eight reads of the picture at places that the picture's own colour says, which is as many reads as a lookup in a table makes, without the table. Both
//! become the engine's own effects in phase 4.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use thiserror::Error;

use super::device::{AdapterChoice, DeviceError, EngineDevice};
use super::frame_renderer::{FrameRenderer, RenderProblem};
use super::guard::{DEFAULT_MOST_PASSES_OF_A_LOOP, guard_wgsl};
use super::pictures::PictureProblem;
use super::yuv::YuvColour;
use super::{
    EffectCall, EffectsThatRun, FrameDescription, LayerDescription, LayerSource, PictureData, PictureId, plan_frame,
};

/// The key of the blur that the benchmark gives the engine as a shader.
const BLUR_KEY: &str = "benchmark:blur";

/// The key of the pass that stands in for the lookup in a table.
const LOOKUP_KEY: &str = "benchmark:lookup";

/// What the shaders of the benchmark are given of the engine (see `effect_shaders`).
const THE_BINDINGS: &str = "
@group(0) @binding(0) var<uniform> numbers: array<vec4<f32>, 16>;
@group(0) @binding(1) var picture: texture_2d<f32>;
@group(0) @binding(2) var picture_sampler: sampler;
";

/// A blur along a line: the numbers are the step between two reads (across, down, in pixels), how many reads there are each side of the pixel, and how wide
/// the blur is (the sigma of the weights).
const BLUR: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(picture));
    let step = vec2<f32>(numbers[0].x, numbers[0].y);
    let reach = i32(numbers[0].z);
    let width = numbers[0].w;
    var sum = vec4<f32>(0.0);
    var weights = 0.0;
    for (var tap = -reach; tap <= reach; tap++) {
        let weight = exp(-0.5 * f32(tap * tap) / (width * width));
        sum = sum + weight * textureSample(picture, picture_sampler, (position.xy + step * f32(tap)) / size);
        weights = weights + weight;
    }
    return sum / weights;
}
";

/// Eight reads of the picture, at places that its own colour says: as many reads as a lookup in a 3-D table makes. The first number is how much of what is read
/// is mixed with the picture.
const LOOKUP: &str = "
@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(picture));
    let here = textureLoad(picture, vec2<i32>(position.xy), 0);
    var sum = vec4<f32>(0.0);
    for (var corner = 0; corner < 8; corner++) {
        let offset = vec2<f32>(f32(corner & 1) * 3.0 + f32((corner >> 1) & 1) * 5.0, f32((corner >> 2) & 1) * 7.0) + here.rg * 61.0;
        sum = sum + textureSample(picture, picture_sampler, (position.xy + offset) / size);
    }
    return mix(here, sum / 8.0, numbers[0].x);
}
";

/// How the benchmark is run.
#[derive(Clone, Debug)]
pub struct BenchmarkSettings {
    /// The size of the frames, and of the pictures that are made them from.
    pub frame_size: (u32, u32),
    /// How many frames of each scene are timed.
    pub frames_of_each_scene: usize,
    /// How many frames of each scene are made first and not timed (the card and its driver are not at their best at once).
    pub frames_to_leave_out_first: usize,
    /// How long a frame is waited for.
    pub frame_timeout: Duration,
}

impl BenchmarkSettings {
    /// What the gate of the phase is measured with: 1080p, 120 frames of each scene.
    pub fn for_the_gate() -> Self {
        Self {
            frame_size: (1920, 1080),
            frames_of_each_scene: 120,
            frames_to_leave_out_first: 10,
            frame_timeout: Duration::from_secs(60),
        }
    }
}

/// How one scene went.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneTimes {
    pub name: &'static str,
    /// How many passes of the card a frame of the scene takes.
    pub passes: usize,
    pub frames: usize,
    pub mean: Duration,
    pub fastest: Duration,
    pub slowest: Duration,
    /// The part of the mean that the engine spent waiting for the card to finish and for the pixels.
    pub mean_waiting_for_the_card: Duration,
}

impl SceneTimes {
    /// How many frames to the second the scene makes, going by the mean.
    pub fn frames_per_second(&self) -> f64 {
        1.0 / self.mean.as_secs_f64().max(1e-9)
    }
}

/// What the benchmark found.
#[derive(Clone, Debug, PartialEq)]
pub struct BenchmarkReport {
    pub adapter_name: String,
    pub backend: String,
    pub is_a_software_adapter: bool,
    pub frame_size: (u32, u32),
    pub frames_of_each_scene: usize,
    /// How long it took to send one video picture of the size of the frame to the card and have it ready, once.
    pub time_to_send_a_video_picture: Duration,
    pub scenes: Vec<SceneTimes>,
}

/// The frames to the second that the whole stack has to make at 1080p, for the gate of phase 1 (EN-07).
pub const FRAMES_TO_THE_SECOND_OF_THE_GATE: f64 = 60.0;

impl BenchmarkReport {
    /// The scene that is the whole stack: the last.
    pub fn whole_stack(&self) -> Option<&SceneTimes> {
        self.scenes.last()
    }

    /// The report as a table, in words, to be read by a person.
    pub fn describe(&self) -> String {
        let mut text = String::new();
        let milliseconds = |time: Duration| time.as_secs_f64() * 1000.0;
        let _ = writeln!(text, "Picture engine benchmark");
        let _ = writeln!(
            text,
            "  adapter: {} ({}{})",
            self.adapter_name,
            self.backend,
            if self.is_a_software_adapter {
                ", a software adapter: this is what a processor makes of it, not a graphics card"
            } else {
                ""
            }
        );
        let _ = writeln!(
            text,
            "  frames: {} by {} pixels, {} timed of each scene",
            self.frame_size.0, self.frame_size.1, self.frames_of_each_scene
        );
        let _ = writeln!(
            text,
            "  sending one video picture of that size to the card, once: {:.1} ms",
            milliseconds(self.time_to_send_a_video_picture)
        );
        let _ = writeln!(text);
        let _ = writeln!(
            text,
            "  {:<44} {:>6} {:>10} {:>9} {:>9} {:>9} {:>14}",
            "scene", "passes", "frames/s", "mean ms", "best ms", "worst ms", "waiting (ms)"
        );
        for scene in &self.scenes {
            let _ = writeln!(
                text,
                "  {:<44} {:>6} {:>10.1} {:>9.2} {:>9.2} {:>9.2} {:>14.2}",
                scene.name,
                scene.passes,
                scene.frames_per_second(),
                milliseconds(scene.mean),
                milliseconds(scene.fastest),
                milliseconds(scene.slowest),
                milliseconds(scene.mean_waiting_for_the_card)
            );
        }
        if let Some(whole) = self.whole_stack() {
            let _ = writeln!(text);
            let makes_it = whole.frames_per_second() >= FRAMES_TO_THE_SECOND_OF_THE_GATE;
            let _ = writeln!(
                text,
                "  the whole stack: {:.1} frames to the second; the gate is {:.0} at 1080p on the integrated graphics: {}",
                whole.frames_per_second(),
                FRAMES_TO_THE_SECOND_OF_THE_GATE,
                match (self.frame_size == (1920, 1080), self.is_a_software_adapter, makes_it) {
                    (false, _, _) => "not at that size, so not the gate",
                    (true, true, _) => "a software adapter is not the gate",
                    (true, false, true) => "MADE",
                    (true, false, false) => "NOT MADE",
                }
            );
        }
        text
    }
}

#[derive(Debug, Error)]
pub enum BenchmarkProblem {
    #[error("the benchmark could not get a graphics device: {0}")]
    NoDevice(#[from] DeviceError),
    #[error("a picture of the benchmark could not be made: {0}")]
    BadPicture(#[from] PictureProblem),
    #[error("the engine could not do what the benchmark asked of it: {0}")]
    Engine(#[from] RenderProblem),
    #[error("a shader of the benchmark was refused: {0}")]
    BadShader(String),
}

/// Runs the benchmark on `adapter`.
pub fn run(adapter: &AdapterChoice, settings: &BenchmarkSettings) -> Result<BenchmarkReport, BenchmarkProblem> {
    let device = EngineDevice::create(adapter)?;
    let adapter_name = device.adapter_information.name.clone();
    let backend = format!("{:?}", device.adapter_information.backend);
    let is_a_software_adapter = device.adapter_information.device_type == wgpu::DeviceType::Cpu;
    let mut renderer = FrameRenderer::new(device);
    for (key, stage) in [(BLUR_KEY, BLUR), (LOOKUP_KEY, LOOKUP)] {
        let source = format!("{THE_BINDINGS}\n{stage}");
        let shader = guard_wgsl(&source, DEFAULT_MOST_PASSES_OF_A_LOOP)
            .map_err(|problem| BenchmarkProblem::BadShader(problem.to_string()))?;
        renderer.install_effect(key, &shader)?;
    }

    let (width, height) = settings.frame_size;
    let pictures = pictures_of(width, height)?;
    let mut time_to_send_a_video_picture = Duration::ZERO;
    for (number, picture) in pictures.iter().enumerate() {
        let started = Instant::now();
        renderer.upload_picture(PictureId(number as u64 + 1), picture)?;
        let waited = renderer.device().device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(settings.frame_timeout),
        });
        if number == 0 {
            time_to_send_a_video_picture = started.elapsed();
            let _ = waited;
        }
    }

    let known = known_effects();
    let mut scenes = Vec::new();
    for scene in the_scenes() {
        let description = scene.description(settings.frame_size);
        let passes = plan_frame(&description, &known).passes_recorded();
        let mut times = Vec::with_capacity(settings.frames_of_each_scene);
        let mut waiting = Vec::with_capacity(settings.frames_of_each_scene);
        for number in 0..settings.frames_to_leave_out_first + settings.frames_of_each_scene {
            let (_, statistics) = renderer.render(&description, settings.frame_timeout)?;
            if number >= settings.frames_to_leave_out_first {
                times.push(statistics.time_taken);
                waiting.push(statistics.time_waiting_for_the_graphics_card);
            }
        }
        scenes.push(SceneTimes {
            name: scene.name,
            passes,
            frames: times.len(),
            mean: mean_of(&times),
            fastest: times.iter().min().copied().unwrap_or_default(),
            slowest: times.iter().max().copied().unwrap_or_default(),
            mean_waiting_for_the_card: mean_of(&waiting),
        });
    }
    Ok(BenchmarkReport {
        adapter_name,
        backend,
        is_a_software_adapter,
        frame_size: settings.frame_size,
        frames_of_each_scene: settings.frames_of_each_scene,
        time_to_send_a_video_picture,
        scenes,
    })
}

fn mean_of(times: &[Duration]) -> Duration {
    if times.is_empty() {
        return Duration::ZERO;
    }
    times.iter().sum::<Duration>() / times.len() as u32
}

/// The effects that the benchmark's frames are made with: the engine's own, and the two that are shaders.
fn known_effects() -> EffectsThatRun {
    let mut known = EffectsThatRun::built_in_only();
    known.add_shader(BLUR_KEY);
    known.add_shader(LOOKUP_KEY);
    known
}

/// What the benchmark makes frames of: two video pictures (in planes, as the engine takes them from ffmpeg), a picture of RGBA bytes, and a smaller one with
/// see-through parts, as a picture file or a text is.
fn pictures_of(width: u32, height: u32) -> Result<Vec<PictureData>, BenchmarkProblem> {
    let (chroma_width, chroma_height) = super::chroma_size(width, height);
    let video = |seed: u32| {
        PictureData::yuv420p8(
            width,
            height,
            numbers_like_a_picture(width as usize * height as usize, width as usize, seed),
            numbers_like_a_picture(
                chroma_width as usize * chroma_height as usize,
                chroma_width as usize,
                seed + 1,
            ),
            numbers_like_a_picture(
                chroma_width as usize * chroma_height as usize,
                chroma_width as usize,
                seed + 2,
            ),
            YuvColour::UNSPECIFIED,
        )
    };
    let rgba = |width: u32, height: u32, seed: u32, see_through: bool| {
        let mut bytes = numbers_like_a_picture(width as usize * height as usize * 4, width as usize * 4, seed);
        for pixel in bytes.as_chunks_mut::<4>().0 {
            pixel[3] = if see_through { pixel[3] | 0x40 } else { 255 };
        }
        PictureData::rgba8(width, height, bytes)
    };
    Ok(vec![
        video(1)?,
        video(4)?,
        rgba(width, height, 7, false)?,
        rgba((width / 3).max(1), (height / 3).max(1), 9, true)?,
    ])
}

/// `count` numbers that change smoothly along a row and from one row to the next, and have a little noise: what a picture is like, and not so smooth that a
/// card could be cleverer about it than it can be about a picture.
fn numbers_like_a_picture(count: usize, row_length: usize, seed: u32) -> Vec<u8> {
    let mut state = 0x9E37_79B9_u32.wrapping_mul(seed.max(1)) | 1;
    (0..count)
        .map(|place| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let across = place % row_length.max(1);
            let down = place / row_length.max(1);
            let smooth = (across * 255 / row_length.max(1)) as u32 / 2 + (down as u32 * 3 + seed * 17) % 128;
            (smooth + (state >> 28)).min(255) as u8
        })
        .collect()
}

/// One scene of the ladder.
struct Scene {
    name: &'static str,
    /// Which of the pictures are in it (from the back), and what is done to each.
    layers: Vec<SceneLayer>,
}

/// A layer of a scene: a picture, where it is, and what is done to it.
struct SceneLayer {
    picture: u64,
    corners: [(f32, f32); 4],
    effects: Vec<Effect>,
}

impl SceneLayer {
    fn new(picture: u64, corners: [(f32, f32); 4], effects: Vec<Effect>) -> Self {
        Self {
            picture,
            corners,
            effects,
        }
    }
}

#[derive(Clone, Copy)]
enum Effect {
    ColourMatrix,
    /// A blur along the rows and then down the columns.
    Blur,
    LookupStandIn,
}

impl Scene {
    fn description(&self, frame_size: (u32, u32)) -> FrameDescription {
        let mut description = FrameDescription::black(frame_size);
        for layer in &self.layers {
            description.layers.push(LayerDescription {
                source: LayerSource::Picture(PictureId(layer.picture)),
                corners: layer.corners,
                effects: layer.effects.iter().flat_map(|effect| calls_of(*effect)).collect(),
            });
        }
        description
    }
}

fn calls_of(effect: Effect) -> Vec<EffectCall> {
    let call = |key: &str, values: &[f32]| EffectCall {
        key: key.to_owned(),
        values: values.to_vec(),
    };
    match effect {
        Effect::ColourMatrix => vec![EffectCall::colour_matrix([
            [0.9, 0.2, 0.0],
            [0.1, 0.8, 0.1],
            [0.0, 0.2, 0.9],
        ])],
        Effect::Blur => vec![
            call(BLUR_KEY, &[1.0, 0.0, 8.0, 4.0]),
            call(BLUR_KEY, &[0.0, 1.0, 8.0, 4.0]),
        ],
        Effect::LookupStandIn => vec![call(LOOKUP_KEY, &[0.5])],
    }
}

/// The corners of a rectangle that is not turned, as fractions of the frame.
fn upright(left: f32, top: f32, width: f32, height: f32) -> [(f32, f32); 4] {
    [
        (left, top),
        (left + width, top),
        (left + width, top + height),
        (left, top + height),
    ]
}

/// The ladder: nothing, then one picture, then four, and then each kind of effect on one of them, and then everything.
fn the_scenes() -> Vec<Scene> {
    let whole = upright(0.0, 0.0, 1.0, 1.0);
    let four = |back: Vec<Effect>, second: Vec<Effect>, third: Vec<Effect>| {
        vec![
            SceneLayer::new(1, whole, back),
            SceneLayer::new(2, upright(0.1, 0.1, 0.8, 0.8), second),
            SceneLayer::new(3, upright(0.5, 0.05, 0.45, 0.45), third),
            SceneLayer::new(4, upright(0.05, 0.6, 0.3, 0.3), Vec::new()),
        ]
    };
    vec![
        Scene {
            name: "a black frame (the output and the read-back)",
            layers: Vec::new(),
        },
        Scene {
            name: "one picture of video",
            layers: vec![SceneLayer::new(1, whole, Vec::new())],
        },
        Scene {
            name: "four pictures",
            layers: four(Vec::new(), Vec::new(), Vec::new()),
        },
        Scene {
            name: "four pictures, a colour matrix",
            layers: four(Vec::new(), vec![Effect::ColourMatrix], Vec::new()),
        },
        Scene {
            name: "four pictures, one blur (two passes)",
            layers: four(vec![Effect::Blur], Vec::new(), Vec::new()),
        },
        Scene {
            name: "four pictures, the lookup stand-in",
            layers: four(Vec::new(), Vec::new(), vec![Effect::LookupStandIn]),
        },
        Scene {
            name: "the whole stack: two blurs, a matrix, a lookup",
            layers: four(
                vec![Effect::Blur],
                vec![Effect::ColourMatrix, Effect::LookupStandIn],
                vec![Effect::Blur],
            ),
        },
    ]
}

#[cfg(test)]
mod tests;
