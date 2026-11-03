// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Drawing a shader file (ISF) to a picture that can be looked at.
//!
//! It is how a shader is **seen**: what the tests of the program's own shaders draw with (on the software adapter), and what `examples/shader_pictures.rs` writes PNG files with, on any
//! graphics adapter of the computer, so that whoever writes a shader can look at what it makes, at a time and with values of its inputs, and say how long a frame of it takes on a
//! real card. A shader is read, compiled, guarded, installed and drawn **the way a clip draws it** (`engine::guard`, `isf::compile`): what is seen is what the program makes.
//!
//! A value of an input is given as text, and read as the kind of value the input has: a number, `true` or `false`, a colour as `#rrggbb` or `#rrggbbaa`, a pair as `across/down`, or the
//! number of the option of a choice.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::engine::{
    CORNERS_OF_THE_WHOLE_PICTURE, DEFAULT_MOST_PASSES_OF_A_LOOP, EffectCall, EffectState, EngineDevice,
    FrameDescription, FrameRenderer, LayerDescription, LayerSource, PictureData, PictureId, RenderedFrame, guard_wgsl,
};
use crate::isf::{compile_in_a_worker, descriptor_of, numbers_of_the_values, read};
use crate::project::{EffectKind, PropertyValue, TextColour};

/// How many frames a second the pictures are drawn as if at: what a shader that remembers the frames before it is run through.
pub const FRAMES_A_SECOND: f32 = 30.0;

/// How long a frame is waited for.
const LONGEST_WAIT_FOR_A_FRAME: Duration = Duration::from_secs(120);

/// A scene with a sky, a sun, mountains, hills and shapes of bright colours with hard edges: something for a filter to be looked at on, which a gradient is not. The bytes of its pixels, four
/// to a pixel (red, green, blue, and 255), the top row first.
pub fn a_scene_to_look_at(width: u32, height: u32) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    let (width_f, height_f) = (width as f32, height as f32);
    for down in 0..height {
        for across in 0..width {
            let (x, y) = (across as f32 / width_f, down as f32 / height_f);
            // The sky: deep blue at the top, orange at the horizon.
            let sky_mix = (y / 0.62).clamp(0.0, 1.0).powf(1.6);
            let mut colour = [0.10 + 0.90 * sky_mix, 0.18 + 0.50 * sky_mix, 0.55 - 0.25 * sky_mix];
            // The sun.
            let sun_distance = ((x - 0.68) * width_f / height_f).hypot(y - 0.40);
            if sun_distance < 0.07 {
                colour = [1.0, 0.95, 0.70];
            } else if sun_distance < 0.16 {
                let glow = 1.0 - (sun_distance - 0.07) / 0.09;
                for (channel, light) in colour.iter_mut().zip([1.0, 0.8, 0.4]) {
                    *channel += (light - *channel) * glow * glow * 0.6;
                }
            }
            // The mountains, far and near.
            let far = 0.52 + 0.06 * (x * 9.0).sin() + 0.03 * (x * 23.0 + 1.0).sin();
            let near = 0.62 + 0.05 * (x * 6.0 + 2.0).sin() + 0.02 * (x * 31.0).sin();
            if y > far {
                colour = [0.32, 0.22, 0.45];
            }
            if y > near {
                colour = [0.12, 0.30, 0.22];
            }
            // A field of squares in the lower part.
            if y > 0.78 {
                let square = ((across / 24) + (down / 24)).is_multiple_of(2);
                colour = if square { [0.92, 0.90, 0.80] } else { [0.15, 0.15, 0.20] };
            }
            // Shapes with hard edges and saturated colours.
            let shapes = [
                ((0.18, 0.30), 0.06, [0.95, 0.12, 0.12]),
                ((0.30, 0.22), 0.045, [0.98, 0.90, 0.10]),
                ((0.42, 0.34), 0.05, [0.10, 0.85, 0.90]),
                ((0.08, 0.50), 0.04, [0.90, 0.15, 0.85]),
            ];
            for ((centre_x, centre_y), radius, shape_colour) in shapes {
                if ((x - centre_x) * width_f / height_f).hypot(y - centre_y) < radius {
                    colour = shape_colour;
                }
            }
            // Two bars like lines of print, one white and one black.
            if (0.86..0.88).contains(&x) && (0.15..0.60).contains(&y) {
                colour = [1.0, 1.0, 1.0];
            }
            if (0.90..0.92).contains(&x) && (0.25..0.70).contains(&y) {
                colour = [0.05, 0.05, 0.05];
            }
            rgba.extend(colour.map(|channel| (channel.clamp(0.0, 1.0) * 255.0 + 0.5) as u8));
            rgba.push(255);
        }
    }
    rgba
}

/// What to draw: the shader (its file name and its text), the size, the time, the values it is given in place of the ones it begins as, and the picture a filter works on.
pub struct ShaderToDraw<'a> {
    pub file_name: &'a str,
    pub text: &'a str,
    pub size: (u32, u32),
    pub time: f32,
    /// `(id of the input, its value as text)`: see the top of the file.
    pub values: &'a [(&'a str, &'a str)],
    pub picture: &'a PictureData,
}

/// The value that the text `text` is for a value that begins as `begins_as`.
fn value_of_the_text(begins_as: &PropertyValue, text: &str) -> Result<PropertyValue, String> {
    let wrong = || format!("{text:?} is not a value of the kind that this input has");
    Ok(match begins_as {
        PropertyValue::Number(_) => PropertyValue::Number(text.trim().parse().map_err(|_| wrong())?),
        PropertyValue::Flag(_) => PropertyValue::Flag(text.trim().parse().map_err(|_| wrong())?),
        PropertyValue::Words(_) => PropertyValue::Words(text.trim().to_owned()),
        PropertyValue::Pair(..) => {
            let (across, down) = text.split_once('/').ok_or_else(wrong)?;
            PropertyValue::Pair(
                across.trim().parse().map_err(|_| wrong())?,
                down.trim().parse().map_err(|_| wrong())?,
            )
        }
        PropertyValue::Colour(_) => {
            let digits = text.trim().trim_start_matches('#');
            let channel =
                |at: usize| u8::from_str_radix(digits.get(at..at + 2).ok_or_else(wrong)?, 16).map_err(|_| wrong());
            let opacity = if digits.len() == 8 { channel(6)? } else { 255 };
            PropertyValue::Colour(TextColour::new(channel(0)?, channel(2)?, channel(4)?, opacity))
        }
    })
}

/// A shader that was read, compiled, guarded and installed on a renderer, ready to be drawn at any time and with any values.
pub struct ShaderOnARenderer {
    key: String,
    number: u64,
    kind: EffectKind,
    remembers_earlier_frames: bool,
}

impl ShaderOnARenderer {
    /// Reads, compiles, guards and installs the shader `text` of the file `file_name` on `renderer`, under the number `number` (which tells it from the other shaders the renderer
    /// has). What went wrong is told in words, with the stage it got to.
    pub fn install(renderer: &mut FrameRenderer, number: u64, file_name: &str, text: &str) -> Result<Self, String> {
        let file = read(text).map_err(|problem| format!("read: {problem}"))?;
        let kind =
            EffectKind::from_snapshot(descriptor_of(&file, file_name, text)).map_err(|why| format!("read: {why}"))?;
        let compiled = compile_in_a_worker(&file).map_err(|problems| {
            format!(
                "compile: {}",
                problems.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")
            )
        })?;
        let structure = compiled.structure.clone().map_err(|problems| {
            format!(
                "structure: {}",
                problems.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")
            )
        })?;
        structure.check().map_err(|why| format!("structure: {why}"))?;
        let key = format!("pictures:{number}");
        let guarded = guard_wgsl(&compiled.wgsl, DEFAULT_MOST_PASSES_OF_A_LOOP)
            .map_err(|problem| format!("install: {problem}"))?;
        renderer
            .install_effect_with_structure(&key, &guarded, &structure)
            .map_err(|problem| format!("install: {problem}"))?;
        Ok(Self {
            key,
            number,
            kind,
            remembers_earlier_frames: structure.depends_on_earlier_frames(),
        })
    }

    /// The kind of effect that the shader is (its values, and what they begin as).
    pub fn kind(&self) -> EffectKind {
        self.kind
    }

    /// The values the shader is given: what it begins as, with `changed` (an id and a value as text) in place of those that it names.
    pub fn values_with(&self, changed: &[(&str, &str)]) -> Result<Vec<PropertyValue>, String> {
        let mut values = self.kind.default_values();
        for (id, text) in changed {
            let slot = self
                .kind
                .slot_of_the_value_with_id(id)
                .ok_or_else(|| format!("value: the shader has no input called {id}"))?;
            values[slot] = value_of_the_text(&values[slot], text).map_err(|why| format!("value {id}: {why}"))?;
        }
        Ok(values)
    }

    /// The frame at `time` seconds, drawn with `values`. **A shader that remembers the frames before it is drawn from its first frame to the one at the time**, at
    /// [`FRAMES_A_SECOND`].
    pub fn draw(
        &self,
        renderer: &mut FrameRenderer,
        size: (u32, u32),
        time: f32,
        values: &[PropertyValue],
        picture: &PictureData,
    ) -> Result<RenderedFrame, String> {
        renderer
            .upload_picture(PictureId(1), picture)
            .map_err(|problem| format!("draw: the picture could not be sent: {problem}"))?;
        let target_frame = (time * FRAMES_A_SECOND).round().max(0.0) as u64;
        let first_frame = if self.remembers_earlier_frames { 0 } else { target_frame };
        let mut last = None;
        for frame_number in first_frame..=target_frame {
            let time_of_the_frame = if frame_number == target_frame {
                time
            } else {
                frame_number as f32 / FRAMES_A_SECOND
            };
            let mut numbers = vec![time_of_the_frame, 1.0 / FRAMES_A_SECOND, frame_number as f32, 0.0];
            numbers.extend(numbers_of_the_values(values));
            let effect = EffectCall {
                key: self.key.clone(),
                values: numbers,
                table: None,
                state: self.remembers_earlier_frames.then_some(EffectState {
                    id: self.number + 1,
                    frame: frame_number,
                }),
                images: Vec::new(),
                masks: Vec::new(),
            };
            let mut description = FrameDescription::black(size);
            description.layers = vec![LayerDescription {
                source: LayerSource::Picture(PictureId(1)),
                corners: CORNERS_OF_THE_WHOLE_PICTURE,
                effects: vec![effect],
            }];
            let (frame, statistics) = renderer
                .render(&description, LONGEST_WAIT_FOR_A_FRAME)
                .map_err(|problem| format!("draw: {problem}"))?;
            if statistics.effects_not_run != 0 {
                return Err(format!(
                    "draw: the engine did not run the effect ({})",
                    statistics.effects_not_run
                ));
            }
            last = Some(frame);
        }
        last.ok_or_else(|| "draw: no frame was drawn".to_owned())
    }
}

/// The picture that the shader makes: installed on `renderer` and drawn once (see [`ShaderOnARenderer`]).
pub fn picture_of_a_shader(
    renderer: &mut FrameRenderer,
    number: u64,
    shader: &ShaderToDraw<'_>,
) -> Result<RenderedFrame, String> {
    let installed = ShaderOnARenderer::install(renderer, number, shader.file_name, shader.text)?;
    let values = installed.values_with(shader.values)?;
    installed.draw(renderer, shader.size, shader.time, &values, shader.picture)
}

/// `frame` as a PNG file.
pub fn write_as_png(frame: &RenderedFrame, path: &Path) -> Result<(), String> {
    image::save_buffer(
        path,
        &frame.rgba_pixels,
        frame.width_in_pixels,
        frame.height_in_pixels,
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|error| format!("{}: {error}", path.display()))
}

/// All of `frames`, `across` of them to a row, as one PNG file. The frames are all of one size.
pub fn write_a_sheet(frames: &[RenderedFrame], across: usize, path: &Path) -> Result<(), String> {
    let Some(first) = frames.first() else {
        return Ok(());
    };
    let across = across.max(1);
    let rows = frames.len().div_ceil(across);
    let (width, height) = (first.width_in_pixels as usize, first.height_in_pixels as usize);
    let sheet_width = width * across;
    let mut rgba = vec![0_u8; sheet_width * height * rows * 4];
    for (number, frame) in frames.iter().enumerate() {
        let (column, row) = (number % across, number / across);
        for line in 0..height {
            let from = line * width * 4;
            let to = ((row * height + line) * sheet_width + column * width) * 4;
            rgba[to..to + width * 4].copy_from_slice(&frame.rgba_pixels[from..from + width * 4]);
        }
    }
    image::save_buffer(
        path,
        &rgba,
        sheet_width as u32,
        (height * rows) as u32,
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|error| format!("{}: {error}", path.display()))
}

/// The shader files that `named` names: a file is itself, and a folder is the files of it that end in `.fs` or `.isf`, by name.
pub fn shader_files_named(named: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for path in named {
        if path.is_dir() {
            let mut inside: Vec<PathBuf> = std::fs::read_dir(path)
                .map_err(|error| format!("{}: {error}", path.display()))?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|file| {
                    file.extension()
                        .is_some_and(|ending| ending.eq_ignore_ascii_case("fs") || ending.eq_ignore_ascii_case("isf"))
                })
                .collect();
            inside.sort();
            files.extend(inside);
        } else {
            files.push(path.clone());
        }
    }
    Ok(files)
}

/// What the command that writes pictures of shaders was asked to do (`examples/shader_pictures.rs`).
pub struct PicturesAsked {
    /// The shaders: files, or folders whose `.fs` and `.isf` files are all drawn.
    pub shaders: Vec<PathBuf>,
    /// Where the pictures are written.
    pub folder: PathBuf,
    pub size: (u32, u32),
    pub times: Vec<f32>,
    /// Sets of values (`id=value`), each a picture of each shader at each time; one empty set draws the shader as it begins.
    pub sets_of_values: Vec<Vec<(String, String)>>,
    /// A picture for a filter to work on (a PNG or a JPEG), in place of [`a_scene_to_look_at`].
    pub picture: Option<PathBuf>,
    /// Write the WGSL that the graphics library is given beside the pictures (before and after the guard puts counters in the loops).
    pub write_the_wgsl: bool,
    /// How many more times each picture is drawn, to say how long a frame takes (nothing for a shader that remembers frames).
    pub more_times_to_measure: usize,
    /// How many pictures are side by side in `sheet.png`: as many as there are times if it is not said.
    pub sheet_columns: Option<usize>,
}

/// Writes the pictures that `asked` says, on `device`, with a line for each to `say`; and `sheet.png` with all of them (the times side by side, the sets of values one under the other,
/// unless the columns of the sheet are said).
/// Gives how many pictures could not be drawn.
pub fn write_the_pictures_asked_for(
    device: &EngineDevice,
    asked: &PicturesAsked,
    say: &mut dyn FnMut(&str),
) -> Result<usize, String> {
    std::fs::create_dir_all(&asked.folder).map_err(|error| format!("{}: {error}", asked.folder.display()))?;
    let picture = match &asked.picture {
        Some(path) => {
            let decoded = image::open(path)
                .map_err(|error| format!("{}: {error}", path.display()))?
                .resize_exact(asked.size.0, asked.size.1, image::imageops::FilterType::Triangle)
                .to_rgba8();
            PictureData::rgba8(asked.size.0, asked.size.1, decoded.into_raw()).map_err(|problem| problem.to_string())?
        }
        None => PictureData::rgba8(
            asked.size.0,
            asked.size.1,
            a_scene_to_look_at(asked.size.0, asked.size.1),
        )
        .map_err(|problem| problem.to_string())?,
    };
    let sets: Vec<Vec<(String, String)>> = if asked.sets_of_values.is_empty() {
        vec![Vec::new()]
    } else {
        asked.sets_of_values.clone()
    };
    let mut drawn = Vec::new();
    let mut failed = 0;
    for (number, path) in shader_files_named(&asked.shaders)?.iter().enumerate() {
        let file_name = path
            .file_name()
            .map_or_else(|| "shader.fs".to_owned(), |name| name.to_string_lossy().into_owned());
        let stem = path
            .file_stem()
            .map_or_else(|| "shader".to_owned(), |name| name.to_string_lossy().replace(' ', "_"));
        let text = std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
        if asked.write_the_wgsl
            && let Ok(file) = read(&text)
            && let Ok(compiled) = compile_in_a_worker(&file)
        {
            std::fs::write(asked.folder.join(format!("{stem}.wgsl")), &compiled.wgsl)
                .map_err(|error| error.to_string())?;
            if let Ok(guarded) = guard_wgsl(&compiled.wgsl, DEFAULT_MOST_PASSES_OF_A_LOOP) {
                std::fs::write(asked.folder.join(format!("{stem}_guarded.wgsl")), &guarded.wgsl)
                    .map_err(|error| error.to_string())?;
            }
        }
        // (A renderer for each shader, so that none is held up by what another left.)
        let mut renderer = FrameRenderer::new(device.clone());
        let started = Instant::now();
        let installed = match ShaderOnARenderer::install(&mut renderer, number as u64, &file_name, &text) {
            Ok(installed) => installed,
            Err(why) => {
                say(&format!("FAIL {file_name} | {why}"));
                failed += asked.times.len() * sets.len();
                continue;
            }
        };
        say(&format!(
            "{file_name} installed in {} ms",
            started.elapsed().as_millis()
        ));
        for (set_number, set) in sets.iter().enumerate() {
            let changed: Vec<(&str, &str)> = set.iter().map(|(id, text)| (id.as_str(), text.as_str())).collect();
            let values = match installed.values_with(&changed) {
                Ok(values) => values,
                Err(why) => {
                    say(&format!("FAIL {file_name} values {set_number} | {why}"));
                    failed += asked.times.len();
                    continue;
                }
            };
            for time in &asked.times {
                let started = Instant::now();
                match installed.draw(&mut renderer, asked.size, *time, &values, &picture) {
                    Ok(frame) => {
                        let first = started.elapsed();
                        let name = format!("{stem}_v{set_number}_t{time}.png");
                        write_as_png(&frame, &asked.folder.join(&name))?;
                        let mut timing = format!("{} ms", first.as_millis());
                        if asked.more_times_to_measure > 0 && !installed.remembers_earlier_frames {
                            let started = Instant::now();
                            for _ in 0..asked.more_times_to_measure {
                                installed.draw(&mut renderer, asked.size, *time, &values, &picture)?;
                            }
                            timing = format!(
                                "{timing}, then {:.2} ms a frame (the mean of {})",
                                started.elapsed().as_secs_f64() * 1000.0 / asked.more_times_to_measure as f64,
                                asked.more_times_to_measure
                            );
                        }
                        say(&format!(
                            "ok   {file_name} values {set_number} at {time} | {timing} | {name}"
                        ));
                        drawn.push(frame);
                    }
                    Err(why) => {
                        say(&format!("FAIL {file_name} values {set_number} at {time} | {why}"));
                        failed += 1;
                    }
                }
            }
        }
    }
    write_a_sheet(
        &drawn,
        asked.sheet_columns.unwrap_or(asked.times.len()),
        &asked.folder.join("sheet.png"),
    )?;
    Ok(failed)
}

#[cfg(test)]
mod tests;
