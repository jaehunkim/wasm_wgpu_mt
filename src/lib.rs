use js_sys::{Atomics, Int32Array};
use wasm_bindgen::prelude::*;
use web_sys::console;

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}

#[wasm_bindgen]
pub async fn run(
    input_data_sab: &js_sys::SharedArrayBuffer,
    output_data_sab: &js_sys::SharedArrayBuffer,
    receiver_sab: &js_sys::SharedArrayBuffer,
    sender_sab: &js_sys::SharedArrayBuffer,
) {
    console::log_1(&"run: WebGPU function called".into());
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init_with_level(log::Level::Warn).expect("Couldn't initialize logger");

    // want to log the address of the input_data_sab, output_data_sab, receiver_sab, sender_sab
    console::log_1(&format!("run: input_data_sab: {:?}", input_data_sab).into());
    console::log_1(&format!("run: output_data_sab: {:?}", output_data_sab).into());
    console::log_1(&format!("run: receiver_sab: {:?}", receiver_sab).into());
    console::log_1(&format!("run: sender_sab: {:?}", sender_sab).into());

    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: None,
        })
        .await
        .unwrap();

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Field Operations Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        )
        .await
        .unwrap();

    // WGSL shader code
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
    });

    // Create input buffer (2 u32s)
    let input_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (2 * std::mem::size_of::<u32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    // Create output buffer (1 u32)
    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: std::mem::size_of::<u32>() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    // Create staging buffer
    let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: std::mem::size_of::<u32>() as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    // Create bind group
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("main"),
        cache: None,
        compilation_options: Default::default(),
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output_buffer.as_entire_binding(),
            },
        ],
        label: None,
    });

    let receiver_state = Int32Array::new(receiver_sab);
    let sender_state = Int32Array::new(sender_sab);
    let input_data_bytes = js_sys::Uint32Array::new(input_data_sab);
    let output_data_bytes = js_sys::Uint32Array::new(output_data_sab);

    loop {
        console::log_1(&"run: Waiting for receiver".into());
        // Wait for receiver
        while Atomics::load(&receiver_state, 0).unwrap() == 0 {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        // Get input data
        let mut input_data = [0u32; 2];
        input_data_bytes.copy_to(&mut input_data);

        // Create command encoder
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        // Copy input data
        queue.write_buffer(&input_buffer, 0, bytemuck::cast_slice(&input_data));

        // Create compute pass
        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            compute_pass.set_pipeline(&compute_pipeline);
            compute_pass.set_bind_group(0, &bind_group, &[]);
            compute_pass.dispatch_workgroups(1, 1, 1);
        }

        encoder.copy_buffer_to_buffer(&output_buffer, 0, &staging_buffer, 0, staging_buffer.size());

        // Wait for the GPU to finish executing the command buffer
        console::log_1(&"run: Waiting for GPU command buffer execution...".into());
        let command_buffer = encoder.finish();
        queue.submit(std::iter::once(command_buffer));
        device.poll(wgpu::Maintain::Wait);

        // Read results
        let output_slice = staging_buffer.slice(..);
        let (sender, receiver) = futures_intrusive::channel::shared::oneshot_channel();
        output_slice.map_async(wgpu::MapMode::Read, move |v| sender.send(v).unwrap());

        if let Some(Ok(())) = receiver.receive().await {
            let data = output_slice.get_mapped_range();
            let result = bytemuck::cast_slice::<u8, u32>(&data);
            output_data_bytes.copy_from(&result);
        }

        // Reset receiver state and notify sender
        Atomics::store(&receiver_state, 0, 0).expect("Failed to store receiver state");
        Atomics::store(&sender_state, 0, 1).expect("Failed to store sender state");
        Atomics::notify(&sender_state, 0).expect("Failed to notify sender");
    }
}

#[wasm_bindgen]
pub fn process_data(
    input1: u32,
    input2: u32,
    input_data_sab: &js_sys::SharedArrayBuffer,
    output_data_sab: &js_sys::SharedArrayBuffer,
    receiver_sab: &js_sys::SharedArrayBuffer,
    sender_sab: &js_sys::SharedArrayBuffer,
) {
    console::log_1(&"process_data: Processing data in Rust".into());
    console::log_1(&format!("process_data: input1: {}, input2: {}", input1, input2).into());
    console::log_1(&format!("process_data: input_data_sab: {:?}", input_data_sab).into());
    console::log_1(&format!("process_data: output_data_sab: {:?}", output_data_sab).into());
    console::log_1(&format!("process_data: receiver_sab: {:?}", receiver_sab).into());
    console::log_1(&format!("process_data: sender_sab: {:?}", sender_sab).into());
    let input_data_bytes = js_sys::Uint32Array::new(input_data_sab);
    let output_data_bytes = js_sys::Uint32Array::new(output_data_sab);
    let receiver_state = Int32Array::new(receiver_sab);
    let sender_state = Int32Array::new(sender_sab);

    // 입력 데이터를 SharedArrayBuffer에 복사
    input_data_bytes.set_index(0, input1);
    input_data_bytes.set_index(1, input2);

    // sender에게 알림
    Atomics::store(&sender_state, 0, 1).expect("Failed to store sender state");
    Atomics::notify(&sender_state, 0).expect("Failed to notify sender");
    console::log_1(&"process_data: Notified sender".into());

    // receiver 대기
    console::log_1(&"process_data: Waiting for receiver".into());
    while Atomics::load(&receiver_state, 0).unwrap() == 0 {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    console::log_1(&"process_data: Receiver notified".into());
    // 결과 읽기
    let result = output_data_bytes.get_index(0);
    web_sys::console::log_1(&result.into());

    // receiver 상태 초기화
    Atomics::store(&receiver_state, 0, 0).expect("Failed to reset receiver state");
}
