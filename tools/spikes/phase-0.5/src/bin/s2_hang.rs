//! S2: a shader that does not finish, on the SOFTWARE adapter only, in its own process (the driver script kills it by process id if it is still there after the time it is given).
//! Prints a timeline: when the work was submitted, when (if ever) the poll with a timeout came back, when the device-lost callback and the uncaptured-error handler fired,
//! and whether a device made afterwards (on a fresh instance) can draw.
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use spikes::gpu::{Gpu, VERTEX_WGSL, make_texture, read_rgba8};

const FRAGMENT: &str = r#"
struct U { iterations: u32, mode: u32, never: u32, pad: u32 }
@group(0) @binding(0) var<uniform> u: U;
@fragment fn main(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    var acc = p.x * 0.001;
    if (u.mode == 0u) {
        for (var i = 0u; i < u.iterations; i = i + 1u) {
            acc = fract(acc * 1.0001 + 0.37);
        }
    } else {
        loop {
            acc = fract(acc * 1.0001 + 0.37);
            if (u.never == 1u) { break; }
        }
    }
    return vec4<f32>(acc, 0.5, 0.25, 1.0);
}
"#;

static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
static EVENTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn note(text: impl Into<String>) {
    let line = format!(
        "[{:8.3} s] {}",
        START.get().unwrap().elapsed().as_secs_f64(),
        text.into()
    );
    println!("{line}");
    EVENTS.lock().unwrap().push(line);
}

fn make_gpu() -> Gpu {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()))
        .into_iter()
        .find(|adapter| adapter.get_info().device_type == wgpu::DeviceType::Cpu)
        .expect("there is no software adapter: refusing to use a real GPU");
    let info = adapter.get_info();
    assert_eq!(info.device_type, wgpu::DeviceType::Cpu);
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("s2"),
        ..Default::default()
    }))
    .expect("no device");
    device.set_device_lost_callback(|reason, message| note(format!("DEVICE LOST callback: {reason:?}: {message}")));
    device.on_uncaptured_error(Arc::new(|error| note(format!("uncaptured error: {error}"))));
    let _keep_instance_alive = Box::leak(Box::new(instance));
    Gpu {
        device,
        queue,
        info,
        lost: Default::default(),
    }
}

fn submit_draw(gpu: &Gpu, iterations: u32, mode: u32) -> wgpu::Texture {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let vertex = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(VERTEX_WGSL.into()),
    });
    let fragment = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(FRAGMENT.into()),
    });
    let layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &vertex,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &fragment,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let words: [u32; 4] = [iterations, mode, 0, 0];
    gpu.queue.write_buffer(&buffer, 0, bytemuck::bytes_of(&words));
    let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &buffer,
                offset: 0,
                size: std::num::NonZeroU64::new(16),
            }),
        }],
    });
    let target = make_texture(gpu, 64, 64, format);
    let view = target.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[0]);
        pass.draw(0..3, 0..1);
    }
    gpu.queue.submit([encoder.finish()]);
    target
}

fn main() {
    START.set(Instant::now()).unwrap();
    let mode_name = std::env::args()
        .nth(1)
        .expect("usage: s2_hang <finite N | infinite_loop | long_for> [watchdog seconds]");
    let (iterations, mode) = match mode_name.as_str() {
        "finite" => (std::env::args().nth(2).unwrap().parse::<u32>().unwrap(), 0),
        "long_for" => (u32::MAX, 0),
        "infinite_loop" => (0, 1),
        other => panic!("unknown mode {other}"),
    };
    let watchdog_seconds: f64 = std::env::args()
        .nth(if mode_name == "finite" { 3 } else { 2 })
        .map_or(8.0, |s| s.parse().unwrap());
    let gpu = make_gpu();
    note(format!(
        "adapter {} ({:?}); mode {mode_name}, iterations {iterations}, watchdog {watchdog_seconds} s",
        gpu.info.name, gpu.info.backend
    ));

    let started = Instant::now();
    let target = submit_draw(&gpu, iterations, mode);
    note("submitted");
    let result = gpu.device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_secs_f64(watchdog_seconds)),
    });
    note(format!(
        "poll(Wait with timeout {watchdog_seconds} s) returned {result:?} after {:.3} s",
        started.elapsed().as_secs_f64()
    ));
    if result.is_ok() {
        // It finished (or the device was lost and wgpu stopped waiting): can the picture be read?
        let reader = std::thread::spawn({
            let device = gpu.device.clone();
            let queue = gpu.queue.clone();
            let info = gpu.info.clone();
            let target = target.clone();
            move || {
                let gpu = Gpu {
                    device,
                    queue,
                    info,
                    lost: Default::default(),
                };
                read_rgba8(&gpu, &target).get_pixel(10, 10).0
            }
        });
        let read_started = Instant::now();
        while !reader.is_finished() && read_started.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(20));
        }
        if reader.is_finished() {
            note(format!(
                "read back pixel (10,10) = {:?}",
                reader.join().map_err(|_| "reader panicked")
            ));
        } else {
            note("read back did NOT return within 10 s");
        }
    } else {
        note("the watchdog fired: the draw is still running. Recovery attempt:");
        // 1. Is the device lost by itself shortly after (the operating system's own timeout)?
        let waited = Instant::now();
        while waited.elapsed() < Duration::from_secs(6)
            && EVENTS.lock().unwrap().iter().all(|line| !line.contains("DEVICE LOST"))
        {
            std::thread::sleep(Duration::from_millis(50));
        }
        let lost = EVENTS.lock().unwrap().iter().any(|line| line.contains("DEVICE LOST"));
        note(format!("device lost callback seen within 6 more seconds: {lost}"));
        // 2. Destroy the old device (in a thread: does it come back?).
        let destroyer = std::thread::spawn({
            let device = gpu.device.clone();
            move || device.destroy()
        });
        let destroy_started = Instant::now();
        while !destroyer.is_finished() && destroy_started.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(20));
        }
        note(format!(
            "device.destroy() returned: {} (after {:.3} s)",
            destroyer.is_finished(),
            destroy_started.elapsed().as_secs_f64()
        ));
        // 3. A fresh instance and device: can they draw?
        let fresh = std::thread::spawn(|| {
            let gpu = make_gpu();
            let target = submit_draw(&gpu, 1000, 0);
            let polled = gpu.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            });
            let pixel = read_rgba8(&gpu, &target).get_pixel(10, 10).0;
            (polled.is_ok(), pixel)
        });
        let fresh_started = Instant::now();
        while !fresh.is_finished() && fresh_started.elapsed() < Duration::from_secs(25) {
            std::thread::sleep(Duration::from_millis(20));
        }
        if fresh.is_finished() {
            note(format!(
                "a FRESH device drew: {:?} after {:.3} s",
                fresh.join().map_err(|_| "panicked"),
                fresh_started.elapsed().as_secs_f64()
            ));
        } else {
            note("a FRESH device did NOT finish a trivial draw within 25 s");
        }
    }
    if let Some(seconds) = std::env::var("S2_LINGER_SECONDS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
    {
        note(format!(
            "lingering {seconds} s so that the driver can measure what the process still uses"
        ));
        std::thread::sleep(Duration::from_secs(seconds));
    }
    note("done (exiting the process without waiting for anything)");
    std::process::exit(0);
}
