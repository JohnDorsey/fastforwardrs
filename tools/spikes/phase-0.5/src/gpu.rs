//! A device on a SOFTWARE adapter only (never the user's GPU), a fullscreen-triangle renderer and texture helpers. Throwaway spike code.
use std::sync::Arc;

pub const VERTEX_WGSL: &str = r#"
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}
"#;

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub info: wgpu::AdapterInfo,
    pub lost: Arc<std::sync::Mutex<Option<String>>>,
}

/// The one adapter of device type `Cpu` (on Windows: "Microsoft Basic Render Driver" = WARP, on Linux: lavapipe). Panics rather than use
/// anything else: the spikes include shaders that do not finish.
pub fn software_gpu() -> Gpu {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));
    let adapter = adapters
        .into_iter()
        .find(|adapter| adapter.get_info().device_type == wgpu::DeviceType::Cpu)
        .expect("there is no software adapter: the spikes refuse to use a real GPU");
    let info = adapter.get_info();
    assert_eq!(
        info.device_type,
        wgpu::DeviceType::Cpu,
        "safety check: only a software adapter may be opened"
    );
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("spike software device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        ..Default::default()
    }))
    .expect("the software adapter gave no device");
    let lost = Arc::new(std::sync::Mutex::new(None));
    let lost_for_callback = lost.clone();
    device.set_device_lost_callback(move |reason, message| {
        *lost_for_callback.lock().unwrap() = Some(format!("{reason:?}: {message}"));
    });
    Gpu {
        device,
        queue,
        info,
        lost,
    }
}

pub fn make_texture(gpu: &Gpu, width: u32, height: u32, format: wgpu::TextureFormat) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// Uploads RGBA8 rows exactly as given (so a caller who wants GL orientation flips the image first).
pub fn upload_rgba8(gpu: &Gpu, image: &image::RgbaImage) -> wgpu::Texture {
    let texture = make_texture(gpu, image.width(), image.height(), wgpu::TextureFormat::Rgba8Unorm);
    gpu.queue.write_texture(
        texture.as_image_copy(),
        image.as_raw(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * image.width()),
            rows_per_image: Some(image.height()),
        },
        wgpu::Extent3d {
            width: image.width(),
            height: image.height(),
            depth_or_array_layers: 1,
        },
    );
    texture
}

/// Reads an RGBA8 texture back, rows as stored.
pub fn read_rgba8(gpu: &Gpu, texture: &wgpu::Texture) -> image::RgbaImage {
    let (width, height) = (texture.width(), texture.height());
    let padded = (4 * width).div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(padded * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |result| result.expect("map failed"));
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("poll failed");
    let data = slice.get_mapped_range().expect("range");
    let mut pixels = Vec::with_capacity((4 * width * height) as usize);
    for row in 0..height {
        let start = (row * padded) as usize;
        pixels.extend_from_slice(&data[start..start + (4 * width) as usize]);
    }
    drop(data);
    buffer.unmap();
    image::RgbaImage::from_raw(width, height, pixels).unwrap()
}
