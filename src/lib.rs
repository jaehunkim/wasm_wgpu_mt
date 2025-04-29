use js_sys::{Atomics, Int32Array};
use wasm_bindgen::prelude::*;
use web_sys::console;

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

    let request_flag = Int32Array::new(sender_sab);
    let response_flag = Int32Array::new(receiver_sab);

    let input_data_bytes = js_sys::Uint32Array::new(input_data_sab);
    let output_data_bytes = js_sys::Uint32Array::new(output_data_sab);

    loop {
        console::log_1(&"run: Atomics.wait on receiver_state".into());
        let outcome = Atomics::wait(&request_flag, 0, 0).unwrap();
        console::log_1(&format!("run: Atomics.wait returned {:?}", outcome).into());

        // Create staging buffer
        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: std::mem::size_of::<u32>() as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Get input data
        let mut input_data = [0u32; 2];
        input_data_bytes.copy_to(&mut input_data);

        console::log_1(&format!("run: input_data: {:?}", input_data).into());

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
            console::log_1(&format!("run: result: {:?}", result).into());
            output_data_bytes.copy_from(&result);
        }

        Atomics::store(&request_flag, 0, 0).unwrap();

        // Reset receiver state and notify sender
        Atomics::store(&response_flag, 0, 1).unwrap();
        Atomics::notify(&response_flag, 0).unwrap();
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

    let input_data_bytes = js_sys::Uint32Array::new(input_data_sab);
    let output_data_bytes = js_sys::Uint32Array::new(output_data_sab);
    let request_flag = Int32Array::new(sender_sab);
    let response_flag = Int32Array::new(receiver_sab);

    // Copy input data to SharedArrayBuffer
    input_data_bytes.set_index(0, input1);
    input_data_bytes.set_index(1, input2);

    // 1) Write value to send to GPU worker
    Atomics::store(&request_flag, 0, 1).unwrap();
    // 2) Wake up worker with notify
    Atomics::notify(&request_flag, 0).unwrap();

    console::log_1(&"process_data: Atomics.wait on receiver_state".into());
    let outcome = Atomics::wait(&response_flag, 0, 0).unwrap();
    console::log_1(&format!("process_data: Atomics.wait returned {:?}", outcome).into());

    // Read result
    let result = output_data_bytes.get_index(0);
    console::log_1(&format!("process_data: result: {:?}", result).into());

    // Reset receiver state
    Atomics::store(&response_flag, 0, 0).expect("Failed to reset receiver state");
}
