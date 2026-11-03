// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Draws shaders (ISF files) to PNG files, on a graphics adapter of this computer, so that a shader can be **looked at**: what it makes at a time, with values of its inputs, and how
//! long a frame of it takes on that adapter. It draws the way a clip does (the shader is read, compiled, guarded and run by the picture engine), with nothing else in the frame.
//!
//! ```text
//! cargo run --release --example shader_pictures -- --out <folder> [options] <shader file or folder>...
//! cargo run --release --example shader_pictures -- --list                    (which graphics adapters there are, with numbers)
//!
//!   --adapter NUMBER   draw on that adapter (the default is the first that is not a processor)
//!   --software         draw on the software adapter (a processor does it: slow, and what the tests use)
//!   --size WxH         the size of the pictures, 640x360 unless it is said
//!   --times 0,1.5,3    the times, in seconds, of the clip (TIME): a picture of each shader at each
//!   --values a=1,b=2   values of inputs, by their names: a number, true or false, #rrggbb for a colour, across/down for a pair, the number of an option; several sets,
//!                      separated by ;, are several pictures of each shader at each time (`a=1;a=2`)
//!   --picture FILE     the picture that a filter works on (a PNG or a JPEG), in place of a scene made for the purpose
//!   --measure N        draw each picture N more times and say how long a frame takes, on the average
//!   --columns N        how many pictures are side by side in sheet.png (as many as there are times, unless it is said)
//!   --wgsl             write the WGSL that the graphics library is given beside the pictures
//! ```
//!
//! `sheet.png` in the folder has all of the pictures (the times side by side, the sets of values one under the other).

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use fastforwardrs::engine::{AdapterChoice, EngineDevice};
use fastforwardrs::shader_files::{PicturesAsked, write_the_pictures_asked_for};

const HELP: &str = "Draws shaders (ISF files) to PNG files, on a graphics adapter of this computer.
\n
\n  --out FOLDER       where the pictures go (needed)
\n  --list             say which graphics adapters there are, with numbers, and stop
\n  --adapter NUMBER   draw on that adapter (the default is the first that is not a processor)
\n  --software         draw on the software adapter (a processor does it: slow)
\n  --size WxH         the size of the pictures, 640x360 unless it is said
\n  --times 0,1.5,3    the times, in seconds, of the clip: a picture of each shader at each (0 unless it is said)
\n  --values a=1,b=2   values of inputs by their names; several sets separated by ; are several pictures
\n  --picture FILE     the picture that a filter works on, in place of a scene made for the purpose
\n  --measure N        draw each picture N more times and say how long a frame takes, on the average
\n  --columns N        how many pictures are side by side in sheet.png (as many as there are times, unless it is said)
\n  --wgsl             write the WGSL that the graphics library is given beside the pictures
\n  --help             this
\n
\nAfter the options: the shader files (.fs, .isf), or folders of them.";

/// The options that take a value after them.
const OPTIONS_WITH_A_VALUE: [&str; 8] = [
    "--out",
    "--adapter",
    "--size",
    "--times",
    "--values",
    "--picture",
    "--measure",
    "--columns",
];

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        println!("{HELP}");
        return ExitCode::SUCCESS;
    }
    let value_of = |name: &str| {
        arguments
            .iter()
            .position(|argument| argument == name)
            .and_then(|place| arguments.get(place + 1))
            .cloned()
    };

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::PRIMARY));
    if arguments.iter().any(|argument| argument == "--list") {
        for (number, adapter) in adapters.iter().enumerate() {
            let information = adapter.get_info();
            println!(
                "{number}: {} ({:?}, {:?})",
                information.name, information.backend, information.device_type
            );
        }
        return ExitCode::SUCCESS;
    }

    // The shader files: every argument that is not an option or the value of one.
    let mut shaders = Vec::new();
    let mut at = 0;
    while at < arguments.len() {
        let argument = &arguments[at];
        if OPTIONS_WITH_A_VALUE.contains(&argument.as_str()) {
            at += 2;
        } else {
            if !argument.starts_with("--") {
                shaders.push(PathBuf::from(argument));
            }
            at += 1;
        }
    }
    let (Some(folder), false) = (value_of("--out"), shaders.is_empty()) else {
        eprintln!("--out and at least one shader are needed (--help says how)");
        return ExitCode::FAILURE;
    };

    let size = match value_of("--size").as_deref().unwrap_or("640x360").split_once('x') {
        Some((width, height)) => match (width.parse::<u32>(), height.parse::<u32>()) {
            (Ok(width), Ok(height)) if width > 0 && height > 0 => (width, height),
            _ => {
                eprintln!("--size is a width and a height like 640x360");
                return ExitCode::FAILURE;
            }
        },
        None => {
            eprintln!("--size is a width and a height like 640x360");
            return ExitCode::FAILURE;
        }
    };
    let mut times = Vec::new();
    for time in value_of("--times").as_deref().unwrap_or("0").split(',') {
        match time.trim().parse::<f32>() {
            Ok(time) => times.push(time),
            Err(_) => {
                eprintln!("--times is a list of numbers of seconds like 0,1.5,3, not {time:?}");
                return ExitCode::FAILURE;
            }
        }
    }
    let mut sets_of_values = Vec::new();
    for set in value_of("--values").unwrap_or_default().split(';') {
        let mut values = Vec::new();
        for part in set.split(',').filter(|part| !part.trim().is_empty()) {
            match part.split_once('=') {
                Some((id, text)) => values.push((id.trim().to_owned(), text.trim().to_owned())),
                None => {
                    eprintln!("a value is written as name=value, not {part:?}");
                    return ExitCode::FAILURE;
                }
            }
        }
        sets_of_values.push(values);
    }
    let more_times_to_measure = match value_of("--measure") {
        Some(number) => match number.parse::<usize>() {
            Ok(number) => number,
            Err(_) => {
                eprintln!("--measure is a number of frames, not {number:?}");
                return ExitCode::FAILURE;
            }
        },
        None => 0,
    };

    let sheet_columns = match value_of("--columns") {
        Some(number) => match number.parse::<usize>() {
            Ok(number) if number > 0 => Some(number),
            _ => {
                eprintln!("--columns is a number of pictures above nothing, not {number:?}");
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };

    let choice = if arguments.iter().any(|argument| argument == "--software") {
        AdapterChoice::SoftwareOnly
    } else {
        let chosen = match value_of("--adapter") {
            Some(number) => number.parse::<usize>().ok().and_then(|number| adapters.get(number)),
            None => adapters
                .iter()
                .find(|adapter| adapter.get_info().device_type != wgpu::DeviceType::Cpu),
        };
        let Some(chosen) = chosen else {
            eprintln!("there is no such graphics adapter (--list says which there are)");
            return ExitCode::FAILURE;
        };
        AdapterChoice::Window(Arc::new(chosen.clone()))
    };
    let device = match EngineDevice::create(&choice) {
        Ok(device) => device,
        Err(problem) => {
            eprintln!("no device could be made: {problem}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "drawing on {} ({:?})",
        device.adapter_information.name, device.adapter_information.device_type
    );

    let asked = PicturesAsked {
        shaders,
        folder: PathBuf::from(folder),
        size,
        times,
        sets_of_values,
        picture: value_of("--picture").map(PathBuf::from),
        write_the_wgsl: arguments.iter().any(|argument| argument == "--wgsl"),
        more_times_to_measure,
        sheet_columns,
    };
    match write_the_pictures_asked_for(&device, &asked, &mut |line| println!("{line}")) {
        Ok(0) => ExitCode::SUCCESS,
        Ok(failed) => {
            eprintln!("{failed} pictures could not be drawn");
            ExitCode::FAILURE
        }
        Err(problem) => {
            eprintln!("{problem}");
            ExitCode::FAILURE
        }
    }
}
