// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Measures the picture engine on a graphics adapter of this computer, and says how many frames to the second it makes of four layers with two blurs and a
//! colour grade on them, and what each kind of pass costs. The gate of phase 1 is that the whole stack makes 60 frames to the second at 1080p on the integrated
//! graphics; this is how to find out.
//!
//! ```text
//! cargo run --release --example engine_benchmark                    (the integrated graphics, if there is one, at 1080p)
//! cargo run --release --example engine_benchmark -- --list          (which graphics adapters there are, with numbers)
//! cargo run --release --example engine_benchmark -- --adapter 1     (that one)
//! cargo run --release --example engine_benchmark -- --software      (a processor does it: slow, and not the gate)
//! cargo run --release --example engine_benchmark -- --size 1280x720 --frames 300
//! ```
//!
//! It draws with shaders that are part of the program and nothing else: there is nothing in it that could keep a graphics card busy for long.

use std::process::ExitCode;
use std::sync::Arc;

use fastforwardrs::engine::AdapterChoice;
use fastforwardrs::engine::benchmark::{BenchmarkSettings, run};

const HELP: &str = "Measures the picture engine on a graphics adapter of this computer.
\n
\n  --list             say which graphics adapters there are, with numbers, and stop
\n  --adapter NUMBER   use that adapter (the default is the integrated graphics, if there is any, and otherwise the first that is not a processor)
\n  --software         use the software adapter (a processor does the drawing: slow, and not the gate)
\n  --size WxH         the size of the frames, 1920x1080 unless it is said
\n  --frames N         how many frames of each scene are timed, 120 unless it is said
\n  --help             this";

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let value_of = |name: &str| {
        arguments
            .iter()
            .position(|argument| argument == name)
            .and_then(|place| arguments.get(place + 1))
            .cloned()
    };
    if arguments
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        println!("{HELP}");
        return ExitCode::SUCCESS;
    }

    let mut settings = BenchmarkSettings::for_the_gate();
    if let Some(size) = value_of("--size") {
        let parts: Vec<&str> = size.split('x').collect();
        match parts.as_slice() {
            [width, height] if width.parse::<u32>().is_ok() && height.parse::<u32>().is_ok() => {
                settings.frame_size = (width.parse().unwrap(), height.parse().unwrap());
            }
            _ => {
                eprintln!("--size is a width and a height like 1920x1080, not {size:?}");
                return ExitCode::FAILURE;
            }
        }
    }
    if let Some(frames) = value_of("--frames") {
        match frames.parse::<usize>() {
            Ok(frames) if frames > 0 => settings.frames_of_each_scene = frames,
            _ => {
                eprintln!("--frames is a number of frames above nothing, not {frames:?}");
                return ExitCode::FAILURE;
            }
        }
    }

    let adapter = if arguments.iter().any(|argument| argument == "--software") {
        AdapterChoice::SoftwareOnly
    } else {
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
        let chosen = match value_of("--adapter") {
            Some(number) => number.parse::<usize>().ok().and_then(|number| adapters.get(number)),
            // The integrated graphics if there are any (that is the gate), and otherwise the first that is not a processor.
            None => adapters
                .iter()
                .find(|adapter| adapter.get_info().device_type == wgpu::DeviceType::IntegratedGpu)
                .or_else(|| {
                    adapters
                        .iter()
                        .find(|adapter| adapter.get_info().device_type != wgpu::DeviceType::Cpu)
                }),
        };
        let Some(chosen) = chosen else {
            eprintln!("there is no such graphics adapter (--list says which there are)");
            return ExitCode::FAILURE;
        };
        AdapterChoice::Window(Arc::new(chosen.clone()))
    };

    match run(&adapter, &settings) {
        Ok(report) => {
            println!("{}", report.describe());
            ExitCode::SUCCESS
        }
        Err(problem) => {
            eprintln!("the benchmark did not run: {problem}");
            ExitCode::FAILURE
        }
    }
}
