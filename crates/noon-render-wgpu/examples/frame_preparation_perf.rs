use std::{
    hint::black_box,
    mem::size_of,
    time::{Duration, Instant},
};

use noon_core::{
    GeometryRef, GeometryResourceArena, ObjectContentRef, ObjectId, Style, TextResourceArena,
    Transform2D, Vec2, VectorPath,
};
use noon_render_wgpu::text::TextDeviceMetrics;
use noon_render_wgpu::{
    Camera2D, CircleInstance, FramePreparer, GpuRenderer, RetainedFramePreparer,
};
use noon_runtime::{FrameChanges, FrameObjectState, FrameState};
use noon_typst::{compile_typst_resource, TypstMode};

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

#[derive(Clone, Copy, Default)]
struct CommandMetrics {
    initial_upload_bytes: usize,
    upload_bytes: usize,
    draw_calls: usize,
    instances: usize,
    bundle_rebuilds: usize,
}

struct CommandObservation {
    command_buffer: wgpu::CommandBuffer,
    update: Duration,
    encode: Duration,
    metrics: CommandMetrics,
}

struct CommandMeasurements {
    update: Timing,
    encode: Timing,
    submit: Timing,
    metrics: CommandMetrics,
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
    println!("| Objects | Operation | Median | p95 | p99 | Initial bytes | Dirty bytes | Draw calls | Instances | Bundle rebuilds / samples |");
    println!("|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|");
    for (object_count, case) in [
        (10_000, GeometryCase::Transform),
        (100_000, GeometryCase::Transform),
        (100, GeometryCase::Small),
        (1_024, GeometryCase::UniquePaths),
        (10_000, GeometryCase::Camera),
        (512, GeometryCase::Invalidation),
    ] {
        benchmark_gpu_geometry(&device, &queue, object_count, config, case);
    }
    benchmark_gpu_text(&device, &queue, config, false);
    benchmark_gpu_text(&device, &queue, config, true);
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
}

#[derive(Clone, Copy)]
enum GeometryCase {
    Transform,
    Small,
    UniquePaths,
    Camera,
    Invalidation,
}

fn benchmark_gpu_geometry(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    object_count: usize,
    config: Config,
    case: GeometryCase,
) {
    let target = object_count / 2;
    let mut frame = if matches!(case, GeometryCase::UniquePaths | GeometryCase::Invalidation) {
        build_path_frame(object_count)
    } else {
        build_gpu_frame(object_count)
    };
    let stable_content = frame.objects[target].content.clone();
    let replacement = ObjectContentRef::Geometry(polygon_path(
        target,
        96,
        2.0 / (object_count as f64).sqrt().ceil() as f32 * 0.28,
    ));
    let changes = if matches!(case, GeometryCase::Camera) {
        FrameChanges::default()
    } else {
        FrameChanges::objects(vec![target])
    };
    let base_x = frame.objects[target].transform.translation.x;
    let mut preparer = FramePreparer::new();
    let mut renderer = GpuRenderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_viewport(device, queue, 256, 256);
    let initial_upload_bytes = {
        let initial = preparer.prepare(&frame);
        renderer.upload(device, queue, &initial).bytes_uploaded
    };
    let initial_bundle_count = renderer.path_render_bundle_rebuilds();
    let mut previous_bundle_count = initial_bundle_count;
    let (_target_texture, view) = benchmark_target(device);
    let measurements = measure_command_frames(device, queue, config, |iteration| {
        let update_started = Instant::now();
        match case {
            GeometryCase::Camera => {
                let phase = iteration as f32 * 0.002;
                renderer.set_camera(
                    queue,
                    Camera2D {
                        center: Vec2::new(phase.sin() * 0.15, phase.cos() * 0.15),
                        world_size: Vec2::new(2.0, 2.0),
                    },
                );
            }
            GeometryCase::Invalidation => {
                frame.objects[target].content = if iteration % 2 == 0 {
                    replacement.clone()
                } else {
                    stable_content.clone()
                };
            }
            _ => {
                frame.objects[target].transform.translation.x = base_x + iteration as f32 * 0.001;
            }
        }
        let prepared = preparer.prepare_incremental(&frame, &changes);
        let upload = renderer.upload(device, queue, &prepared);
        let update = update_started.elapsed();
        let started = Instant::now();
        let mut encoder = device.create_command_encoder(&Default::default());
        let draws = renderer.encode(&mut encoder, &view, &prepared, wgpu::Color::BLACK);
        let command_buffer = encoder.finish();
        let bundle_count = renderer.path_render_bundle_rebuilds();
        let bundle_rebuilds = bundle_count.saturating_sub(previous_bundle_count);
        previous_bundle_count = bundle_count;
        CommandObservation {
            command_buffer,
            update,
            encode: started.elapsed(),
            metrics: CommandMetrics {
                initial_upload_bytes,
                upload_bytes: upload.bytes_uploaded,
                draw_calls: draws.draw_calls,
                instances: draws.instances_drawn,
                bundle_rebuilds,
            },
        }
    });
    assert_eq!(measurements.metrics.instances, object_count);
    let label = match case {
        GeometryCase::Transform => "transform prepare + upload",
        GeometryCase::Small => "small-scene transform prepare + upload",
        GeometryCase::UniquePaths => "unique-path transform prepare + upload",
        GeometryCase::Camera => "camera uniform update + clean frame prep",
        GeometryCase::Invalidation => "path topology/resource invalidation",
    };
    print_command_measurements(object_count, label, measurements);
}

fn benchmark_gpu_text(device: &wgpu::Device, queue: &wgpu::Queue, config: Config, mixed: bool) {
    let object_count = if mixed { 256 } else { 512 };
    let artifact = compile_typst_resource("Noon C7 benchmark", TypstMode::Markup).unwrap();
    let mut texts = TextResourceArena::new();
    let text = texts.insert(artifact.resource).unwrap();
    let fonts = artifact.fonts;
    let geometries = GeometryResourceArena::new();
    let metrics = TextDeviceMetrics::uniform(100.0).unwrap();
    let mut frame = build_retained_frame(object_count, text, mixed);
    let target = if mixed { 1 } else { object_count / 2 };
    let base_x = frame.objects[target].transform.translation.x;
    let changes = FrameChanges::objects(vec![target]);
    let mut preparer = RetainedFramePreparer::new();
    let mut renderer = GpuRenderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut text_state = renderer.create_retained_text_state(device, queue);
    renderer.set_viewport(device, queue, 256, 256);
    let initial_upload_bytes = {
        let initial = preparer
            .prepare_with_changes(
                device,
                &frame,
                &FrameChanges::all(),
                &texts,
                &fonts,
                &geometries,
                metrics,
            )
            .unwrap();
        assert_eq!(initial.stats.semantic_objects, object_count);
        renderer
            .upload_retained(device, queue, &initial, &mut text_state)
            .bytes_uploaded()
    };
    let initial_bundle_count = renderer.path_render_bundle_rebuilds();
    let mut previous_bundle_count = initial_bundle_count;
    let (_target_texture, view) = benchmark_target(device);
    let measurements = measure_command_frames(device, queue, config, |iteration| {
        frame.objects[target].transform.translation.x = base_x + iteration as f32 * 0.0005;
        let update_started = Instant::now();
        let prepared = preparer
            .prepare_with_changes(
                device,
                &frame,
                &changes,
                &texts,
                &fonts,
                &geometries,
                metrics,
            )
            .unwrap();
        let upload = renderer.upload_retained(device, queue, &prepared, &mut text_state);
        let update = update_started.elapsed();
        let started = Instant::now();
        let mut encoder = device.create_command_encoder(&Default::default());
        let draws = renderer
            .encode_retained(
                &mut encoder,
                &view,
                &prepared,
                &text_state,
                wgpu::Color::BLACK,
                None,
            )
            .unwrap();
        let command_buffer = encoder.finish();
        let bundle_count = renderer.path_render_bundle_rebuilds();
        let bundle_rebuilds = bundle_count.saturating_sub(previous_bundle_count);
        previous_bundle_count = bundle_count;
        CommandObservation {
            command_buffer,
            update,
            encode: started.elapsed(),
            metrics: CommandMetrics {
                initial_upload_bytes,
                upload_bytes: upload.bytes_uploaded(),
                draw_calls: draws.draw_calls(),
                instances: draws.instances_drawn(),
                bundle_rebuilds,
            },
        }
    });
    print_command_measurements(
        object_count,
        if mixed {
            "mixed painter-order prepare + upload"
        } else {
            "stable text prepare + upload"
        },
        measurements,
    );
}

fn build_retained_frame(
    object_count: usize,
    text: noon_core::TextResourceHandle,
    mixed: bool,
) -> FrameState {
    let mut frame = build_gpu_frame(object_count);
    for (index, object) in frame.objects.iter_mut().enumerate() {
        object.z_index = index as f64;
        object.content = if mixed && index % 2 == 0 {
            ObjectContentRef::Geometry(GeometryRef::circle(0.025))
        } else {
            ObjectContentRef::Text(text)
        };
    }
    frame
}

fn benchmark_target(device: &wgpu::Device) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
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
    let view = texture.create_view(&Default::default());
    (texture, view)
}

fn build_path_frame(object_count: usize) -> FrameState {
    let mut frame = build_gpu_frame(object_count);
    for (index, object) in frame.objects.iter_mut().enumerate() {
        object.content = ObjectContentRef::Geometry(unique_path(index));
    }
    frame
}

fn unique_path(index: usize) -> GeometryRef {
    let sides = 3 + index % 5;
    let columns = (1_024usize as f64).sqrt().ceil() as usize;
    polygon_path(index, sides, 2.0 / columns as f32 * 0.28)
}

fn polygon_path(index: usize, sides: usize, radius: f32) -> GeometryRef {
    let rotation = index as f32 * 0.013;
    let mut path = VectorPath::new();
    for point in 0..sides {
        let angle = rotation + std::f32::consts::TAU * point as f32 / sides as f32;
        let scale = radius * (1.0 + index as f32 * 0.000_001);
        let position = Vec2::new(scale * angle.cos(), scale * angle.sin());
        path = if point == 0 {
            path.move_to(position)
        } else {
            path.line_to(position)
        };
    }
    GeometryRef::path(path.close())
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

fn measure_command_frames(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    config: Config,
    mut encode_frame: impl FnMut(usize) -> CommandObservation,
) -> CommandMeasurements {
    let mut update = Vec::with_capacity(config.samples);
    let mut encode = Vec::with_capacity(config.samples);
    let mut submit = Vec::with_capacity(config.samples);
    let mut metrics = CommandMetrics::default();
    for iteration in 0..(config.warmups + config.samples) {
        let observation = encode_frame(iteration);
        let submit_started = Instant::now();
        queue.submit(Some(observation.command_buffer));
        let submit_elapsed = submit_started.elapsed();
        if iteration >= config.warmups {
            update.push(observation.update);
            encode.push(observation.encode);
            submit.push(submit_elapsed);
            metrics.initial_upload_bytes = observation.metrics.initial_upload_bytes;
            metrics.upload_bytes = observation.metrics.upload_bytes;
            metrics.draw_calls = observation.metrics.draw_calls;
            metrics.instances = observation.metrics.instances;
            metrics.bundle_rebuilds = metrics
                .bundle_rebuilds
                .saturating_add(observation.metrics.bundle_rebuilds);
        }
    }
    // Drain outside the timed regions to keep later cases from inheriting this
    // case's GPU backlog. These timings still exclude GPU execution.
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    CommandMeasurements {
        update: summarize(&update),
        encode: summarize(&encode),
        submit: summarize(&submit),
        metrics,
    }
}

fn print_command_measurements(
    object_count: usize,
    update_label: &str,
    measurements: CommandMeasurements,
) {
    for (label, timing) in [
        (update_label, measurements.update),
        ("command encode + finish", measurements.encode),
        ("queue.submit host call", measurements.submit),
    ] {
        println!(
            "| {object_count} | {label} | {:.6} ms | {:.6} ms | {:.6} ms | {} | {} | {} | {} | {} |",
            milliseconds(timing.median),
            milliseconds(timing.p95),
            milliseconds(timing.p99),
            measurements.metrics.initial_upload_bytes,
            measurements.metrics.upload_bytes,
            measurements.metrics.draw_calls,
            measurements.metrics.instances,
            measurements.metrics.bundle_rebuilds,
        );
    }
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
