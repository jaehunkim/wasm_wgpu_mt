use futures_intrusive::channel::shared::{channel, Receiver, Sender};
use tokio::task::{spawn, spawn_blocking};

use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::*;
use wasm_mt::prelude::*;
use wasm_mt::utils::run_js;
use web_sys::console;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

const LENGTH: usize = 2;
type Data = [u32; LENGTH];

pub async fn run_with_channel(
    req_rx: Receiver<Data>,
    resp_tx: Sender<Data>,
    initialize_tx: Sender<()>,
) {
    console::log_1(&"run_with_channel: WebGPU function called".into());
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

    initialize_tx.send(()).await.unwrap();
    console::log_1(&"run_with_channel: waiting for input".into());
    while let Some(buf) = req_rx.receive().await {
        // Create staging buffer
        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: std::mem::size_of::<u32>() as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        console::log_1(&format!("run_with_channel: input_data: {:?}", buf).into());

        // Create command encoder
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        // Copy input data
        queue.write_buffer(&input_buffer, 0, bytemuck::cast_slice(&buf));

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
        console::log_1(&"run_with_channel: Waiting for GPU command buffer execution...".into());
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
            console::log_1(&format!("run_with_channel: result: {:?}", result).into());
            let mut result_buf: [u32; LENGTH] = [0, 0];
            result_buf[0] = result[0];
            resp_tx.send(result_buf).await.unwrap();
        }
    }
}

pub fn request_data_sync(data: Data, req_tx: Sender<Data>, resp_rx: Receiver<Data>) -> Data {
    console::log_1(&format!("request_data_sync: sending data").into());
    let _ = pollster::block_on(req_tx.send(data));
    console::log_1(&format!("request_data_sync: waiting for result").into());
    let result: Data = pollster::block_on(resp_rx.receive()).unwrap();
    console::log_1(&format!("request_data_sync: received result").into());
    result
}

#[wasm_bindgen_test]
pub async fn prove_only_in_rust() {
    // let href = run_js("return location.href;")
    //     .unwrap()
    //     .as_string()
    //     .unwrap();
    // console::log_1(&format!("prove_only_in_rust: href: {}", href).into());
    // let mt = WasmMt::new(&href).and_init().await.unwrap();
    // let _th = mt.thread().and_init().await.unwrap();

    let (runner_tx, runner_rx) = channel::<()>(1);
    let (req_tx, req_rx) = channel::<Data>(1);
    let (resp_tx, resp_rx) = channel::<Data>(1);
    let thread_id = std::thread::current().id();
    console::log_1(&format!("prove_only_in_rust: thread id: {:?}", thread_id).into());

    let _ = tokio::spawn(async move {
        let thread_id = std::thread::current().id();
        console::log_1(&format!("run_with_channel: thread id: {:?}", thread_id).into());
        console::log_1(&format!("run_with_channel: start").into());
        run_with_channel(req_rx, resp_tx, runner_tx).await;
    });

    let _ = runner_rx.receive().await;
    console::log_1(&format!("prove_only_in_rust: runner spawned").into());

    let mut data1: Data = [0; LENGTH];
    data1[0] = 1;
    data1[1] = 2;
    let result1 = request_data_sync(data1, req_tx, resp_rx);
    web_sys::console::log_1(&format!("prove_only_in_rust: {:?}", &result1[0]).into());

    console::log_1(&format!("prove_only_in_rust: end").into());
}
