//! Explicit browser-worker qualification fixtures; excluded from production builds.
//! These use ordinary compiler/runtime tracks and the real transport encoder.
//! They do not enable or stand in for public Scene/Python activation.
use std::sync::Arc;

use noon_compile::{CompiledGlow, CompiledObject, CompiledScene, ExecutionPatch};
use noon_core::{
    Camera2DState, Color, FontResourceArena, FrameEpoch, GeometryRef, GeometryResourceArena, Glow,
    GlowSource, GlowUpdate, ObjectId, Pixels, Property, PublicationContext, RateFunction,
    SemanticNodeId, Style, TextResourceArena, TrackDefinition, TrackId, TrackTiming, TrackValues,
    Transform2D, Vec2,
};
use noon_runtime::{FrameState, SceneInstance};
use serde::Serialize;

use crate::{
    RetainedExecutionDeltaEncoder, RetainedExecutionDeltaEnvelope, RetainedResourceBundle,
};

const WIDTH: u32 = 160;
const HEIGHT: u32 = 120;
const PAD: u32 = 24; // even: preserve the analytic derivative-quad grid
const VIEW_HEIGHT: f32 = 6.0;

#[derive(Serialize)]
struct Fixture {
    width: u32,
    height: u32,
    padding: u32,
    resources: Vec<u8>,
    cases: Vec<Case>,
}
#[derive(Serialize)]
struct Case {
    name: &'static str,
    frames: Vec<Sample>,
    neutral: RetainedExecutionDeltaEnvelope,
    ordinary: RetainedExecutionDeltaEnvelope,
    removed: RetainedExecutionDeltaEnvelope,
}
#[derive(Serialize)]
struct Sample {
    delta: RetainedExecutionDeltaEnvelope,
    // Ordinary reference renders: back, source RGB, source alpha, mask, front RGB, front alpha.
    references: Vec<RetainedExecutionDeltaEnvelope>,
    sigma: f64,
    intensity: f64,
    tint: [f32; 4],
    opacity: f32,
    offscreen: bool,
}

fn camera(extended: bool) -> Camera2DState {
    Camera2DState {
        center: Vec2::ZERO,
        height: if extended {
            VIEW_HEIGHT * (HEIGHT + 2 * PAD) as f32 / HEIGHT as f32
        } else {
            VIEW_HEIGHT
        },
    }
}

fn runtime(
    rectangle: bool,
    offscreen: bool,
    silhouette: bool,
    intensity_only: bool,
) -> SceneInstance {
    let style = |color, opacity| Style {
        fill: Some(color),
        stroke: None,
        opacity,
        ..Style::default()
    };
    let mut source = CompiledObject::new(
        ObjectId::new(2),
        if rectangle {
            GeometryRef::rectangle(1.1, 0.6)
        } else {
            GeometryRef::circle(0.65)
        },
        Transform2D {
            translation: Vec2::new(if offscreen { -4.75 } else { 0.173 }, -0.217),
            rotation: if offscreen { 0.0 } else { 0.37 },
            scale: if offscreen {
                Vec2::new(1.0, 1.0)
            } else {
                Vec2::new(-1.2, 0.8)
            },
        },
        style(
            Color::rgba(0.7, 0.3, 0.1, if silhouette { 0.0 } else { 0.43 }),
            0.47,
        ),
    );
    let glow = CompiledGlow {
        attachment: SemanticNodeId::new(900, u32::MAX),
        definition: Glow::new(
            GlowUpdate::default()
                .radius(Pixels(3.25))
                .intensity(2.4)
                .color(Color::rgba(0.95, 0.35, 0.8, 0.8))
                .source(if silhouette {
                    GlowSource::Silhouette
                } else {
                    GlowSource::Painted
                }),
        )
        .unwrap(),
    };
    source.glow = Some(Arc::new(glow));
    let objects = vec![
        CompiledObject::new(
            ObjectId::new(1),
            GeometryRef::circle(2.4),
            Transform2D {
                translation: Vec2::new(-1.0, 0.0),
                ..Transform2D::IDENTITY
            },
            style(Color::rgba(0.14, 0.28, 0.55, 0.7), 1.0),
        ),
        source,
        CompiledObject::new(
            ObjectId::new(3),
            GeometryRef::circle(0.3),
            Transform2D {
                translation: Vec2::new(if offscreen { -3.8 } else { 0.55 }, 0.25),
                ..Transform2D::IDENTITY
            },
            style(Color::rgba(0.1, 0.95, 0.4, 1.0), 1.0),
        ),
    ];
    let mut result = SceneInstance::new(CompiledScene::compile_objects(objects, &[]).unwrap());
    if !intensity_only {
        let from = result.frame().objects[1].transform.translation;
        result
            .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
                id: TrackId::new(1),
                object: ObjectId::new(2),
                property: Property::Position,
                values: TrackValues::Vec2 {
                    from,
                    to: from + Vec2::new(0.1, 0.12),
                },
                timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
                time_map: Default::default(),
            }))
            .unwrap();
    }
    let update = if intensity_only {
        GlowUpdate::default().intensity(0.0)
    } else {
        GlowUpdate::default()
            .radius(Pixels(4.75))
            .intensity(0.7)
            .color(Color::rgba(0.15, 0.85, 0.25, 0.5))
    };
    for (i, (property, values)) in glow
        .parameter_channels(update)
        .unwrap()
        .into_iter()
        .enumerate()
    {
        result
            .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
                id: TrackId::new(i as u64 + 2),
                object: ObjectId::new(2),
                property,
                values,
                timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
                time_map: Default::default(),
            }))
            .unwrap();
    }
    result
}

fn reference(frame: &FrameState, stage: usize) -> FrameState {
    let mut value = frame.clone();
    value.presences.fill(false);
    let index = match stage {
        0 => 0,
        1..=3 => 1,
        _ => 2,
    };
    value.presences[index] = true;
    for row in &mut value.objects {
        row.glow = None;
    }
    if index == 1 {
        value.objects[1].style.opacity = 1.0;
    }
    if matches!(stage, 2 | 3 | 5) {
        let mut alpha = value.objects[index].style.fill.unwrap().alpha;
        if stage == 3
            && frame.objects[1].glow.as_ref().unwrap().definition.source() == GlowSource::Silhouette
        {
            alpha = 1.0;
        }
        value.objects[index].style.fill = Some(Color::rgba(1.0, 1.0, 1.0, alpha));
    }
    value
}

fn fixture() -> Fixture {
    let resources = RetainedResourceBundle::capture(
        [],
        &TextResourceArena::new(),
        &GeometryResourceArena::new(),
        &FontResourceArena::new(),
    )
    .unwrap()
    .encode_binary()
    .unwrap();
    let mut reference_encoder = RetainedExecutionDeltaEncoder::new(700);
    let mut reference_epoch = 0;
    let mut cases = Vec::new();
    for (i, (name, rectangle, offscreen, silhouette, intensity_only)) in [
        ("circle-motion", false, false, false, false),
        ("rectangle-motion", true, false, false, false),
        ("offscreen-circle", false, true, false, false),
        ("offscreen-silhouette", true, true, true, false),
        ("intensity-to-zero", false, false, false, true),
    ]
    .into_iter()
    .enumerate()
    {
        let mut runtime = runtime(rectangle, offscreen, silhouette, intensity_only);
        let mut encoder = RetainedExecutionDeltaEncoder::new(10 + i as u32);
        let mut frames = Vec::new();
        for (index, time) in [0.0, 0.37, 0.8, 1.0, 0.0].into_iter().enumerate() {
            runtime.evaluate(time).unwrap();
            let publication = runtime.take_renderer_publication();
            let frame = publication.frame();
            let delta = if index == 0 {
                encoder
                    .encode_snapshot_with_context(frame, camera(false), publication.context())
                    .unwrap()
            } else {
                encoder
                    .encode_incremental_with_painter_order_and_context(
                        frame,
                        publication.changes(),
                        camera(false),
                        publication.context(),
                        publication.painter_order(),
                    )
                    .unwrap()
                    .unwrap()
            };
            let glow = frame.objects[1].glow.as_ref().unwrap().definition;
            let color = glow.color();
            let references = (0..6)
                .map(|stage| {
                    reference_epoch += 1;
                    reference_encoder
                        .encode_snapshot_with_context(
                            &reference(frame, stage),
                            camera(true),
                            PublicationContext::default()
                                .with_frame_epoch(FrameEpoch::new(reference_epoch)),
                        )
                        .unwrap()
                })
                .collect();
            frames.push(Sample {
                delta,
                references,
                sigma: glow.radius().value(),
                intensity: glow.intensity(),
                tint: [color.red, color.green, color.blue, color.alpha],
                opacity: frame.objects[1].style.opacity,
                offscreen,
            });
        }
        for id in if intensity_only {
            vec![2]
        } else {
            vec![1, 2, 3, 4]
        } {
            runtime
                .apply_execution_patch(&ExecutionPatch::RemoveTrack(TrackId::new(id)))
                .unwrap();
        }
        let mut glow = **runtime.frame().objects[1].glow.as_ref().unwrap();
        glow.definition = GlowUpdate::default()
            .intensity(0.0)
            .apply_to(glow.definition)
            .unwrap();
        runtime
            .apply_execution_patch(&ExecutionPatch::SetGlow {
                object: ObjectId::new(2),
                glow: Arc::new(glow),
            })
            .unwrap();
        let publication = runtime.take_renderer_publication();
        let neutral = encoder
            .encode_incremental_with_painter_order_and_context(
                publication.frame(),
                publication.changes(),
                camera(false),
                publication.context(),
                publication.painter_order(),
            )
            .unwrap()
            .unwrap();
        let mut ordinary_frame = publication.frame().clone();
        for row in &mut ordinary_frame.objects {
            row.glow = None;
        }
        let ordinary = RetainedExecutionDeltaEncoder::new(900 + i as u32)
            .encode_snapshot_with_context(&ordinary_frame, camera(false), publication.context())
            .unwrap();
        runtime
            .apply_execution_patch(&ExecutionPatch::RemoveObject(ObjectId::new(2)))
            .unwrap();
        let publication = runtime.take_renderer_publication();
        let removed = encoder
            .encode_incremental_with_painter_order_and_context(
                publication.frame(),
                publication.changes(),
                camera(false),
                publication.context(),
                publication.painter_order(),
            )
            .unwrap()
            .unwrap();
        cases.push(Case {
            name,
            frames,
            neutral,
            ordinary,
            removed,
        });
    }
    Fixture {
        width: WIDTH,
        height: HEIGHT,
        padding: PAD,
        resources,
        cases,
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(js_name = glowWorkerSmokeFixture)]
pub fn glow_worker_smoke_fixture() -> String {
    serde_json::to_string(&fixture()).expect("finite qualification fixture")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_uses_real_runtime_epochs_and_transport_admission() {
        let fixture = fixture();
        assert_eq!(fixture.cases.len(), 5);
        for case in fixture.cases {
            let mut installed =
                crate::InstalledRetainedExecutionMirror::from_bundle_bytes(&fixture.resources)
                    .unwrap();
            assert_eq!(case.frames.len(), 5);
            let first = &case.frames[0].delta;
            assert!(first.snapshot);
            for sample in &case.frames {
                assert_eq!(
                    sample.delta.protocol_version,
                    crate::RETAINED_EXECUTION_TRANSPORT_VERSION
                );
                let envelope =
                    serde_json::from_value(serde_json::to_value(&sample.delta).unwrap()).unwrap();
                installed.apply(envelope).unwrap();
                let row = &installed.frame().unwrap().objects[1];
                assert_eq!(row.glow.as_ref().unwrap().attachment.generation(), u32::MAX);
                assert_eq!(
                    row.glow.as_ref().unwrap().definition.intensity(),
                    sample.intensity
                );
            }
            assert_eq!(case.frames[0].intensity, case.frames[4].intensity);
            let neutral =
                serde_json::from_value(serde_json::to_value(&case.neutral).unwrap()).unwrap();
            installed.apply(neutral).unwrap();
            assert!(installed.frame().unwrap().objects[1]
                .glow
                .as_ref()
                .unwrap()
                .definition
                .is_neutral());
            let removed =
                serde_json::from_value(serde_json::to_value(&case.removed).unwrap()).unwrap();
            installed.apply(removed).unwrap();
            assert!(!installed.frame().unwrap().is_present(1));
        }
    }
}
