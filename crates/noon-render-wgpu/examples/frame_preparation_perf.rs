use std::{
    hint::black_box,
    mem::size_of,
    time::{Duration, Instant},
};

use noon_core::{GeometryRef, ObjectId, Style, Transform2D, Vec2};
use noon_render_wgpu::{CircleInstance, FramePreparer, GpuRenderer};
use noon_runtime::{FrameChanges, FrameObjectState, FrameState};

const DEFAULT_SIZES: [usize; 3] = [1_000, 10_000, 100_000];
const DEFAULT_WARMUPS: usize = 10;
const DEFAULT_SAMPLES: usize = 100;

#[derive(Clone, Copy)]
struct Config {
    warmups: usize,
    samples: usize,
}

#[derive(Clone, Copy)]
struct Timing {
    median: Duration,
    p95: Duration,
    p99: Duration,
}

fn main() {
    let (config, sizes) = parse_args();
    println!(
        "Noon frame preparation benchmark ({} warmups, {} samples)",
        config.warmups, config.samples
    );
    println!();
    println!(
        "| Objects | Operation | Median | p95 | p99 | Repacked | Upload bytes | Full / operation |"
    );
    println!("|---:|---|---:|---:|---:|---:|---:|---:|");
    for object_count in sizes {
        benchmark_size(object_count, config);
    }
    benchmark_gpu_commands(config);
}

/// Attribute CPU-side command construction and submission on a real adapter.
/// GPU completion is deliberately excluded: Queue::submit measures host enqueue
/// cost, while GPU timestamps or explicit completion waits are a separate metric.
fn benchmark_gpu_commands(config: Config) {
    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        println!("\nGPU command measurements skipped: no WebGPU adapter available.");
        return;
    };
    let adapter_info = adapter.get_info();
    let Ok((device, queue)) = pollster::block_on(adapter.request_device(&Default::default()))
    else {
        println!("\nGPU command measurements skipped: adapter device request failed.");
        return;
    };

    println!(
        "\nGPU command measurements ({:?} {:?}; {} warmups, {} samples)",
        adapter_info.backend, adapter_info.device_type, config.warmups, config.samples
    );
    println!(
        "| Objects | Operation | Median | p95 | p99 | Upload bytes | Draw calls | Instances |"
    );
    println!("|---:|---|---:|---:|---:|---:|---:|---:|");
    for object_count in [10_000, 100] {
        benchmark_gpu_size(&device, &queue, object_count, config);
    }
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
}

fn benchmark_gpu_size(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    object_count: usize,
    config: Config,
) {
    let mut frame = build_gpu_frame(object_count);
    let changes = FrameChanges::objects(vec![object_count / 2]);
    let target = object_count / 2;
    let base_x = frame.objects[target].transform.translation.x;
    let mut preparer = FramePreparer::new();
    let mut renderer = GpuRenderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_viewport(device, queue, 256, 256);
    {
        let initial = preparer.prepare(&frame);
        renderer.upload(device, queue, &initial);
    }
    let target_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Noon command benchmark target"),
        size: wgpu::Extent3d {
            width: 256,
            height: 256,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target_texture.create_view(&Default::default());

    let mut update_times = Vec::with_capacity(config.samples);
    let mut encode_times = Vec::with_capacity(config.samples);
    let mut submit_times = Vec::with_capacity(config.samples);
    let mut upload_bytes = 0;
    let mut draws = noon_render_wgpu::DrawStats::default();
    let total = config.warmups + config.samples;
    for iteration in 0..total {
        frame.objects[target].transform.translation.x = base_x + iteration as f32 * 0.001;
        let update_started = Instant::now();
        let prepared = preparer.prepare_incremental(&frame, &changes);
        let upload = renderer.upload(device, queue, &prepared);
        let update_elapsed = update_started.elapsed();
        let encode_started = Instant::now();
        let command_buffer = {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Noon command benchmark encoder"),
            });
            let encoded = renderer.encode(&mut encoder, &view, &prepared, wgpu::Color::BLACK);
            if iteration >= config.warmups {
                upload_bytes = upload.bytes_uploaded;
                draws = encoded;
            }
            encoder.finish()
        };
        let encode_elapsed = encode_started.elapsed();
        let submit_started = Instant::now();
        queue.submit(Some(command_buffer));
        let submit_elapsed = submit_started.elapsed();
        if iteration >= config.warmups {
            update_times.push(update_elapsed);
            encode_times.push(encode_elapsed);
            submit_times.push(submit_elapsed);
        }
    }

    print_gpu_row(
        object_count,
        "transform prepare + upload",
        summarize(&update_times),
        upload_bytes,
        draws,
    );
    print_gpu_row(
        object_count,
        "command encode + finish",
        summarize(&encode_times),
        upload_bytes,
        draws,
    );
    print_gpu_row(
        object_count,
        "queue.submit host call",
        summarize(&submit_times),
        upload_bytes,
        draws,
    );
}

fn build_gpu_frame(object_count: usize) -> FrameState {
    let mut frame = build_frame(object_count);
    let columns = (object_count as f64).sqrt().ceil() as usize;
    let spacing = 2.0 / columns as f32;
    for (index, object) in frame.objects.iter_mut().enumerate() {
        let column = index % columns;
        let row = index / columns;
        object.transform.translation = Vec2::new(
            -1.0 + (column as f32 + 0.5) * spacing,
            -1.0 + (row as f32 + 0.5) * spacing,
        );
        object.content = noon_core::ObjectContentRef::Geometry(GeometryRef::circle(spacing * 0.2));
    }
    frame
}

fn summarize(durations: &[Duration]) -> Timing {
    let mut sorted = durations.to_vec();
    sorted.sort_unstable();
    Timing {
        median: percentile(&sorted, 0.50),
        p95: percentile(&sorted, 0.95),
        p99: percentile(&sorted, 0.99),
    }
}

fn print_gpu_row(
    object_count: usize,
    operation: &str,
    timing: Timing,
    upload_bytes: usize,
    draws: noon_render_wgpu::DrawStats,
) {
    println!(
        "| {object_count} | {operation} | {:.6} ms | {:.6} ms | {:.6} ms | {upload_bytes} | {} | {} |",
        milliseconds(timing.median),
        milliseconds(timing.p95),
        milliseconds(timing.p99),
        draws.draw_calls,
        draws.instances_drawn,
    );
}

fn benchmark_size(object_count: usize, config: Config) {
    let mut frame = build_frame(object_count);

    let mut full_preparer = FramePreparer::new();
    full_preparer.prepare(&frame);
    let full = measure(config, |_| {
        let prepared = full_preparer.prepare(black_box(&frame));
        black_box(prepared.stats.instances_repacked);
    });

    let mut ordered_preparer = FramePreparer::new();
    // A precomputed permutation models the execution plan's ordering input.
    let mut painter_order = (0..object_count as u32).collect::<Vec<_>>();
    painter_order.sort_by_key(|index| index % 7);
    ordered_preparer.set_painter_order(&frame, &painter_order);
    ordered_preparer.prepare(&frame);
    let explicit_order = measure(config, |_| {
        let prepared = ordered_preparer.prepare(black_box(&frame));
        black_box(prepared.stats.instances_repacked);
    });

    let mut static_preparer = FramePreparer::new();
    static_preparer.prepare(&frame);
    let static_changes = FrameChanges::default();
    let unchanged = measure(config, |_| {
        let prepared = static_preparer.prepare_incremental(black_box(&frame), &static_changes);
        black_box(prepared.stats.instances_repacked);
    });

    let mut dirty_preparer = FramePreparer::new();
    dirty_preparer.prepare(&frame);
    let target = object_count / 2;
    let dirty_changes = FrameChanges::objects(vec![target]);
    let one_changed = measure(config, |iteration| {
        frame.objects[target].transform.translation.x = iteration as f32;
        let prepared = dirty_preparer.prepare_incremental(black_box(&frame), &dirty_changes);
        black_box(prepared.stats.instances_repacked);
    });

    print_row(
        object_count,
        "full rebuild / default order",
        full,
        object_count,
        object_count * size_of::<CircleInstance>(),
        full,
    );
    print_row(
        object_count,
        "full rebuild / explicit z order",
        explicit_order,
        object_count,
        object_count * size_of::<CircleInstance>(),
        full,
    );
    print_row(object_count, "unchanged", unchanged, 0, 0, full);
    print_row(
        object_count,
        "one changed",
        one_changed,
        1,
        size_of::<CircleInstance>(),
        full,
    );
}

fn build_frame(object_count: usize) -> FrameState {
    assert!(object_count > 0, "benchmark sizes must be positive");
    FrameState {
        family_animations: Vec::new(),
        family_animation_plan_indices: Vec::new(),
        time: 0.0,
        objects: (0..object_count)
            .map(|index| FrameObjectState {
                z_index: 0.0,
                id: ObjectId::new(index as u64),
                content: noon_core::ObjectContentRef::Geometry(GeometryRef::circle(0.5)),
                text_bounds: None,
                transform: Transform2D {
                    translation: Vec2::new(index as f32, 0.0),
                    ..Transform2D::IDENTITY
                },
                style: Style::default(),
                appearance: 1.0,
            })
            .collect(),
        presences: vec![true; object_count],
        reveals: vec![1.0; object_count],
        morphs: vec![0.0; object_count],
        render_geometries: vec![None; object_count],
        render_transforms: vec![None; object_count],
    }
}

fn measure(config: Config, mut operation: impl FnMut(usize)) -> Timing {
    for iteration in 0..config.warmups {
        operation(iteration);
    }
    let mut durations = Vec::with_capacity(config.samples);
    for sample in 0..config.samples {
        let started = Instant::now();
        operation(config.warmups + sample);
        durations.push(started.elapsed());
    }
    durations.sort_unstable();
    Timing {
        median: percentile(&durations, 0.50),
        p95: percentile(&durations, 0.95),
        p99: percentile(&durations, 0.99),
    }
}

fn percentile(sorted: &[Duration], percentile: f64) -> Duration {
    let rank = (percentile * sorted.len() as f64).ceil() as usize;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

fn print_row(
    object_count: usize,
    operation: &str,
    timing: Timing,
    repacked: usize,
    upload_bytes: usize,
    full: Timing,
) {
    let speedup = full.median.as_secs_f64() / timing.median.as_secs_f64();
    println!(
        "| {object_count} | {operation} | {:.6} ms | {:.6} ms | {:.6} ms | {repacked} | {upload_bytes} | {speedup:.1}x |",
        milliseconds(timing.median),
        milliseconds(timing.p95),
        milliseconds(timing.p99),
    );
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn parse_args() -> (Config, Vec<usize>) {
    let mut config = Config {
        warmups: DEFAULT_WARMUPS,
        samples: DEFAULT_SAMPLES,
    };
    let mut sizes = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--warmups" => config.warmups = parse_positive("warmups", args.next()),
            "--samples" => config.samples = parse_positive("samples", args.next()),
            _ => sizes.push(
                argument
                    .parse()
                    .ok()
                    .filter(|value| *value > 0)
                    .unwrap_or_else(|| panic!("object count must be positive, got {argument}")),
            ),
        }
    }
    if sizes.is_empty() {
        sizes.extend(DEFAULT_SIZES);
    }
    (config, sizes)
}

fn parse_positive(name: &str, value: Option<String>) -> usize {
    let value = value.unwrap_or_else(|| panic!("--{name} requires a value"));
    value
        .parse()
        .ok()
        .filter(|parsed| *parsed > 0)
        .unwrap_or_else(|| panic!("{name} must be a positive integer, got {value}"))
}
