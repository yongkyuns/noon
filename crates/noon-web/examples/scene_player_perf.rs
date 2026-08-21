use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use noon_core::{GeometryRef, ObjectId, SceneDefinition, ScenePatch, Style, Transform2D, Vec2};
use noon_ir::{encode_patch_batch, encode_scene, PatchBatch};
use noon_web::ScenePlayer;

const DEFAULT_SIZES: [usize; 3] = [1_000, 10_000, 100_000];
const DEFAULT_WARMUPS: usize = 2;
const DEFAULT_SAMPLES: usize = 10;

#[derive(Clone, Copy)]
struct Config {
    warmups: usize,
    samples: usize,
}

#[derive(Clone, Copy)]
struct Timing {
    median: Duration,
    p95: Duration,
}

fn main() {
    let (config, sizes) = parse_args();
    println!(
        "Noon ScenePlayer benchmark ({} warmups, {} samples)",
        config.warmups, config.samples
    );
    println!();
    println!("| Objects | Scene JSON | Operation | Median | p95 | Replacement / operation |");
    println!("|---:|---:|---|---:|---:|---:|");

    for object_count in sizes {
        benchmark_size(object_count, config);
    }
}

fn benchmark_size(object_count: usize, config: Config) {
    let scene_json = build_scene_json(object_count);
    let total_iterations = config.warmups + config.samples;
    let target = ObjectId::new((object_count / 2) as u64);

    let load = measure(config, |_| {
        let player = ScenePlayer::from_scene_json(black_box(&scene_json))
            .expect("benchmark scene must load");
        black_box(player.object_count());
    });

    let mut replacement_player =
        ScenePlayer::from_scene_json(&scene_json).expect("benchmark replacement player must load");
    replacement_player.seek(2.0).expect("seek must succeed");
    let replacement = measure(config, |_| {
        replacement_player
            .replace_scene_json(black_box(&scene_json))
            .expect("scene replacement must succeed");
        black_box(replacement_player.object_count());
    });

    let reconciled_scene_json =
        [0.75, 1.0].map(|opacity| build_scene_json_with_opacity(object_count, opacity));
    let mut reconcile_player =
        ScenePlayer::from_scene_json(&scene_json).expect("benchmark reconcile player must load");
    reconcile_player.seek(2.0).expect("seek must succeed");
    let reconciliation = measure(config, |iteration| {
        let outcome = reconcile_player
            .reconcile_scene_json(black_box(&reconciled_scene_json[iteration % 2]))
            .expect("scene reconciliation must succeed");
        black_box(outcome);
    });

    let style_batches = (0..total_iterations)
        .map(|sequence| {
            encode_patch_batch(&PatchBatch::new(
                sequence as u64,
                vec![ScenePatch::SetStyle {
                    object: target,
                    style: Style {
                        opacity: if sequence % 2 == 0 { 0.75 } else { 1.0 },
                        stroke_join: noon_core::StrokeJoin::Round,
                        stroke_cap: noon_core::StrokeCap::Round,
                        ..Style::default()
                    },
                }],
            ))
            .expect("style batch must serialize")
        })
        .collect::<Vec<_>>();
    let mut style_player =
        ScenePlayer::from_scene_json(&scene_json).expect("benchmark style player must load");
    style_player.seek(2.0).expect("seek must succeed");
    let style_patch = measure(config, |iteration| {
        style_player
            .apply_patch_batch_json(black_box(&style_batches[iteration]))
            .expect("style patch must succeed");
        black_box(style_player.object_count());
    });

    let transform_batches = (0..total_iterations)
        .map(|sequence| {
            encode_patch_batch(&PatchBatch::new(
                sequence as u64,
                vec![ScenePatch::SetTransform {
                    object: target,
                    transform: Transform2D {
                        translation: Vec2::new(sequence as f32, -(sequence as f32)),
                        ..Transform2D::IDENTITY
                    },
                }],
            ))
            .expect("transform batch must serialize")
        })
        .collect::<Vec<_>>();
    let mut transform_player =
        ScenePlayer::from_scene_json(&scene_json).expect("benchmark transform player must load");
    transform_player.seek(2.0).expect("seek must succeed");
    let transform_patch = measure(config, |iteration| {
        transform_player
            .apply_patch_batch_json(black_box(&transform_batches[iteration]))
            .expect("transform patch must succeed");
        black_box(transform_player.object_count());
    });

    let scene_size = format_mib(scene_json.len());
    print_row(object_count, &scene_size, "initial load", load, replacement);
    print_row(
        object_count,
        &scene_size,
        "full replacement",
        replacement,
        replacement,
    );
    print_row(
        object_count,
        &scene_size,
        "one style reconciliation",
        reconciliation,
        replacement,
    );
    print_row(
        object_count,
        &scene_size,
        "one style patch",
        style_patch,
        replacement,
    );
    print_row(
        object_count,
        &scene_size,
        "one transform patch",
        transform_patch,
        replacement,
    );
}

fn build_scene_json(object_count: usize) -> String {
    build_scene_json_with_opacity(object_count, 1.0)
}

fn build_scene_json_with_opacity(object_count: usize, opacity: f32) -> String {
    assert!(object_count > 0, "benchmark sizes must be positive");
    let mut scene = SceneDefinition::new();
    for _ in 0..object_count {
        scene.add(GeometryRef::circle(0.5));
    }
    let target = ObjectId::new((object_count / 2) as u64);
    scene
        .object_mut(target)
        .expect("object must exist")
        .style
        .opacity = opacity;
    encode_scene(&scene).expect("benchmark scene must serialize")
}

fn measure(mut config: Config, mut operation: impl FnMut(usize)) -> Timing {
    config.warmups = config.warmups.min(usize::MAX - config.samples);
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
    }
}

fn percentile(sorted: &[Duration], percentile: f64) -> Duration {
    let rank = (percentile * sorted.len() as f64).ceil() as usize;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

fn print_row(
    object_count: usize,
    scene_size: &str,
    operation: &str,
    timing: Timing,
    replacement: Timing,
) {
    let relative = replacement.median.as_secs_f64() / timing.median.as_secs_f64();
    println!(
        "| {object_count} | {scene_size} | {operation} | {:.3} ms | {:.3} ms | {relative:.2}x |",
        milliseconds(timing.median),
        milliseconds(timing.p95),
    );
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn format_mib(bytes: usize) -> String {
    format!("{:.2} MiB", bytes as f64 / (1024.0 * 1024.0))
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
            "--warmups" => config.warmups = parse_count("warmups", args.next()),
            "--samples" => config.samples = parse_count("samples", args.next()),
            _ => sizes.push(parse_positive("object count", &argument)),
        }
    }
    if sizes.is_empty() {
        sizes.extend(DEFAULT_SIZES);
    }
    (config, sizes)
}

fn parse_count(name: &str, value: Option<String>) -> usize {
    let value = value.unwrap_or_else(|| panic!("--{name} requires a value"));
    parse_positive(name, &value)
}

fn parse_positive(name: &str, value: &str) -> usize {
    value
        .parse::<usize>()
        .ok()
        .filter(|parsed| *parsed > 0)
        .unwrap_or_else(|| panic!("{name} must be a positive integer, got {value}"))
}
