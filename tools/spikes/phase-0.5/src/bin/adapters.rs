//! Lists the adapters wgpu sees and says which are software ones (never opens a device).
fn main() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    for adapter in pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all())) {
        let info = adapter.get_info();
        println!(
            "{:?} | {} | {:?} | driver {} {}",
            info.backend, info.name, info.device_type, info.driver, info.driver_info
        );
    }
}
