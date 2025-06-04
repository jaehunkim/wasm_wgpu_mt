use std::time::Duration;
use wasm_bindgen_futures::spawn_local;
use wasm_bindgen_test::wasm_bindgen_test;
use wasm_thread as thread;
use web_sys::console;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

pub async fn run_wgpu_worker(
    job_request_rx: flume::Receiver<u32>,
    job_result_tx: flume::Sender<u32>,
) {
    console::log_1(&"run: WebGPU function called".into());

    let instance = wgpu::Instance::default();
    console::log_1(&format!("run: instance: {:?}", instance).into());

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: None,
        })
        .await
        .unwrap();

    console::log_1(&format!("run: adapter: {:?}", adapter).into());

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

    console::log_1(&format!("run: device: {:?}", device).into());

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

    console::log_1(&"run: job_request_rx on receiver_state".into());
    let requested_num = job_request_rx.recv().unwrap();
    console::log_1(&format!("run: job_request_rx returned {:?}", requested_num).into());

    // Create staging buffer
    let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: std::mem::size_of::<u32>() as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    // Get input data
    let mut input_data = [0u32; 2];
    input_data[0] = requested_num;
    input_data[1] = requested_num;

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
    let (sender, receiver) = flume::bounded(1);
    output_slice.map_async(wgpu::MapMode::Read, move |v| sender.send(v).unwrap());

    let mut result = 0;
    if let Ok(_) = receiver.recv_async().await {
        let data = output_slice.get_mapped_range();
        let result_arr = bytemuck::cast_slice::<u8, u32>(&data);
        result = result_arr[0];
        console::log_1(&format!("run: result: {:?}", result).into());
    }

    job_result_tx.send(result).unwrap();
}

#[wasm_bindgen_test]
pub async fn test_run_runner() {
    let (job_request_tx, job_request_rx) = flume::bounded::<u32>(1);
    let (job_result_tx, job_result_rx) = flume::bounded::<u32>(1);
    let (all_job_done_tx, all_job_done_rx) = flume::bounded::<()>(1);

    thread::spawn(move || {
        spawn_local(async move {
            run_wgpu_worker(job_request_rx, job_result_tx).await;
        });
    });

    thread::spawn(move || {
        // below blocking should be called from worker thread, not main thread
        mock_sync_prove_fn(job_request_tx, job_result_rx);
        all_job_done_tx.send(()).unwrap();
    });

    all_job_done_rx.recv_async().await.unwrap();
    wasm_thread::terminate_all_workers();
}

fn mock_sync_prove_fn(job_request_tx: flume::Sender<u32>, job_result_rx: flume::Receiver<u32>) {
    thread::sleep(Duration::from_millis(1000));
    job_request_tx.send(1).unwrap();

    let result = job_result_rx.recv().unwrap();
    console::log_1(&format!("result: {:?}", result).into());
}
