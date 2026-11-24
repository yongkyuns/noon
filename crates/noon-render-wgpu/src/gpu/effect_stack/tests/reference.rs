//! Independent specification evaluator: direct two-dimensional convolution,
//! transparent-border bilinear translation, and encoded premultiplied source-over.
//! No production sampler, kernel, uniform, interpolation or pass planner is used.
use noon_core::{EffectDefinition, EffectOffset, EffectRadius, GlowSource};

fn scalar(value: EffectRadius, height: u32, world: f64) -> f64 {
    match value {
        EffectRadius::Pixels(v) => v,
        EffectRadius::Scene(v) => v * f64::from(height) / world,
    }
}
fn quantize(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn convolve(source: &[u8], size: [u32; 2], sigma: f64) -> Vec<[f64; 4]> {
    if sigma == 0.0 {
        return source
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| std::array::from_fn(|c| f64::from(pixel[c]) / 255.0))
            .collect();
    }
    let radius = (3.0 * sigma).ceil() as i32;
    let diameter = (2 * radius + 1) as usize;
    let kernel: Vec<_> = (-radius..=radius)
        .flat_map(|dy| {
            (-radius..=radius)
                .map(move |dx| (-f64::from(dx * dx + dy * dy) / (2.0 * sigma * sigma)).exp())
        })
        .collect();
    let mass: f64 = kernel.iter().sum();
    let mut output = vec![[0.0; 4]; (size[0] * size[1]) as usize];
    for y in 0..size[1] as i32 {
        for x in 0..size[0] as i32 {
            let pixel = &mut output[(y * size[0] as i32 + x) as usize];
            for yy in (y - radius).max(0)..=(y + radius).min(size[1] as i32 - 1) {
                for xx in (x - radius).max(0)..=(x + radius).min(size[0] as i32 - 1) {
                    let weight = kernel
                        [((yy - y + radius) as usize) * diameter + (xx - x + radius) as usize]
                        / mass;
                    let at = ((yy * size[0] as i32 + xx) * 4) as usize;
                    for c in 0..4 {
                        pixel[c] += f64::from(source[at + c]) / 255.0 * weight;
                    }
                }
            }
        }
    }
    output
}

fn alpha_sample(image: &[[f64; 4]], size: [u32; 2], point: [f64; 2]) -> f64 {
    let [x, y] = point;
    let left = x.floor() as i32;
    let top = y.floor() as i32;
    let fraction = [x - f64::from(left), y - f64::from(top)];
    let mut value = 0.0;
    for dy in 0..2 {
        for dx in 0..2 {
            let (xx, yy) = (left + dx, top + dy);
            if xx < 0 || yy < 0 || xx >= size[0] as i32 || yy >= size[1] as i32 {
                continue;
            }
            let weight = (if dx == 0 {
                1.0 - fraction[0]
            } else {
                fraction[0]
            }) * (if dy == 0 {
                1.0 - fraction[1]
            } else {
                fraction[1]
            });
            // Shadow explicitly translates the quantized RGBA Gaussian result.
            value +=
                f64::from(quantize(image[(yy * size[0] as i32 + xx) as usize][3])) / 255.0 * weight;
        }
    }
    value
}
fn neutral(effect: &EffectDefinition, height: u32, world: f64) -> bool {
    match effect {
        EffectDefinition::Bloom(_) => {
            panic!("view Bloom is outside the compatibility capture oracle")
        }
        EffectDefinition::Blur(v) => scalar(v.radius(), height, world) == 0.0,
        EffectDefinition::BackdropBlur(v) => scalar(v.radius(), height, world) == 0.0,
        EffectDefinition::Glow(v) => {
            v.radius().value() == 0.0 || v.intensity() == 0.0 || v.color().alpha == 0.0
        }
        EffectDefinition::Shadow(v) => v.color().alpha == 0.0,
        EffectDefinition::NoiseDissolve(v) => v.is_neutral(),
        EffectDefinition::Pixelate(v) => scalar(v.size(), height, world) <= 1.0,
        EffectDefinition::Shader(_) => false,
    }
}

pub(super) fn evaluate(
    source: &[u8],
    mask: &[u8],
    size: [u32; 2],
    effects: &[EffectDefinition],
    opacity: f64,
    height: u32,
    world: f64,
) -> Vec<u8> {
    let last = effects
        .iter()
        .rposition(|effect| !neutral(effect, height, world));
    let mut input = source.to_vec();
    for (index, effect) in effects.iter().enumerate() {
        if neutral(effect, height, world) {
            continue;
        }
        let stage_opacity = if Some(index) == last { opacity } else { 1.0 };
        if let EffectDefinition::Pixelate(definition) = effect {
            input = crate::gpu::sample_filter::tests::cell_reference(
                &input,
                scalar(definition.size(), height, world),
                [0.0; 2],
                stage_opacity,
                size,
            );
            continue;
        }
        if let EffectDefinition::NoiseDissolve(definition) = effect {
            input = crate::gpu::sample_filter::tests::reference(
                &input,
                crate::SampleParameters {
                    shader: None,
                    definition: (*definition).into(),
                    output_height: height,
                    world_view_height: world,
                    source_bounds: crate::EffectPixelBounds {
                        min: [0.0; 2],
                        max: size.map(f64::from),
                    },
                    scope_opacity: stage_opacity as f32,
                    scratch_budget_bytes: u64::MAX,
                },
                size,
            );
            continue;
        }
        let sigma = scalar(effect.radius().unwrap(), height, world);
        let blurred = convolve(
            match effect {
                EffectDefinition::Glow(v) if v.source() == GlowSource::Silhouette => mask,
                _ => &input,
            },
            size,
            sigma,
        );
        input = input
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .flat_map(|(at, pixel)| {
                let source: [f64; 4] = std::array::from_fn(|c| f64::from(pixel[c]) / 255.0);
                let value: [f64; 4] = match effect {
                    EffectDefinition::Shader(_) => {
                        panic!("custom programs require a separate independent reference")
                    }
                    EffectDefinition::Bloom(_) => {
                        unreachable!("view Bloom rejected before compatibility evaluation")
                    }
                    EffectDefinition::Blur(_) | EffectDefinition::BackdropBlur(_) => blurred[at],
                    EffectDefinition::NoiseDissolve(_) | EffectDefinition::Pixelate(_) => {
                        unreachable!("mask stage was evaluated above")
                    }
                    EffectDefinition::Glow(v) => {
                        let color = v.color();
                        let h = (v.intensity() * f64::from(color.alpha) * blurred[at][3]).min(1.0);
                        let tint = [
                            f64::from(color.red),
                            f64::from(color.green),
                            f64::from(color.blue),
                            1.0,
                        ];
                        std::array::from_fn(|c| source[c] + (1.0 - source[3]) * tint[c] * h)
                    }
                    EffectDefinition::Shadow(v) => {
                        let offset = match v.offset() {
                            EffectOffset::Pixels(v) => v,
                            EffectOffset::Scene(v) => v.map(|x| x * f64::from(height) / world),
                        };
                        let point = [
                            (at % size[0] as usize) as f64 - offset[0],
                            (at / size[0] as usize) as f64 + offset[1],
                        ];
                        let color = v.color();
                        let alpha = f64::from(color.alpha) * alpha_sample(&blurred, size, point);
                        let tint = [
                            f64::from(color.red),
                            f64::from(color.green),
                            f64::from(color.blue),
                            1.0,
                        ];
                        std::array::from_fn(|c| source[c] + (1.0 - source[3]) * tint[c] * alpha)
                    }
                };
                value.map(|v| quantize(v * stage_opacity))
            })
            .collect();
    }
    input
}

#[test]
fn literal_hard_shadow_and_single_cell_kernel_do_not_depend_on_the_gpu() {
    use noon_core::{Color, EffectUpdate, Pixels, Shadow};
    let source = [128, 0, 0, 128, 0, 0, 0, 0];
    let effect = Shadow::new(
        EffectUpdate::default()
            .radius(Pixels(0.0))
            .offset(EffectOffset::Pixels([1.0, 0.0]))
            .color(Color::rgba(0.0, 0.0, 1.0, 0.5)),
    )
    .unwrap();
    assert_eq!(
        evaluate(&source, &source, [2, 1], &[effect.into()], 1.0, 10, 10.0),
        vec![128, 0, 0, 128, 0, 0, 64, 64]
    );
    let impulse = convolve(&[255, 0, 0, 255], [1, 1], 1.0);
    let normalization: f64 = (-3..=3)
        .flat_map(|y| (-3..=3).map(move |x| (-f64::from(x * x + y * y) / 2.0).exp()))
        .sum();
    assert!((impulse[0][0] - 1.0 / normalization).abs() < 1e-15);
    assert_eq!(impulse[0][1], 0.0);
}
