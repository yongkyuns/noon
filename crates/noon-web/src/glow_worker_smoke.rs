//! Explicit browser-worker qualification fixtures; excluded from production builds.
//! These use ordinary compiler/runtime tracks and the real transport encoder.
//! They do not enable or stand in for public Scene/Python activation.
use std::sync::Arc;

use noon_compile::{CompiledEffect, CompiledObject, CompiledScene, ExecutionPatch};
use noon_core::{
    Camera2DState, Color, EffectUpdate, FontResourceArena, FrameEpoch, GeometryRef,
    GeometryResourceArena, Glow, GlowSource, ObjectId, Pixels, Property, PublicationContext,
    RateFunction, SemanticNodeId, StrokeWidthMode, Style, TextResourceArena, TrackDefinition,
    TrackId, TrackTiming, TrackValues, Transform2D, Vec2, VectorPath,
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

#[derive(Clone, Copy)]
enum SourceGeometry {
    Analytic,
    OutlinedCircle,
    StrokedPath,
    TransparentSilhouettePath,
}

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
    source_geometry: SourceGeometry,
) -> SceneInstance {
    let style = |color, opacity| Style {
        fill: Some(color),
        stroke: None,
        opacity,
        ..Style::default()
    };
    let (geometry, source_style) = match source_geometry {
        SourceGeometry::Analytic => (
            if rectangle {
                GeometryRef::rectangle(1.1, 0.6)
            } else {
                GeometryRef::circle(0.65)
            },
            style(
                Color::rgba(0.7, 0.3, 0.1, if silhouette { 0.0 } else { 0.43 }),
                0.47,
            ),
        ),
        SourceGeometry::OutlinedCircle => (
            GeometryRef::circle(0.65),
            Style {
                fill: Some(Color::rgba(0.7, 0.3, 0.1, 0.24)),
                stroke: Some(Color::rgba(0.9, 0.6, 0.2, 0.82)),
                stroke_width: 0.06,
                stroke_width_mode: StrokeWidthMode::ScreenSpace,
                opacity: 1.0,
                ..Style::default()
            },
        ),
        SourceGeometry::StrokedPath | SourceGeometry::TransparentSilhouettePath => {
            let transparent = matches!(source_geometry, SourceGeometry::TransparentSilhouettePath);
            (
                GeometryRef::VectorPath(
                    VectorPath::new().move_to(Vec2::new(-0.7, -0.35)).cubic_to(
                        Vec2::new(-0.5, 0.7),
                        Vec2::new(0.45, -0.65),
                        Vec2::new(0.7, 0.35),
                    ),
                ),
                Style {
                    fill: None,
                    stroke: Some(Color::rgba(
                        0.9,
                        0.55,
                        0.2,
                        if transparent { 0.0 } else { 0.76 },
                    )),
                    stroke_width: 0.07,
                    stroke_width_mode: StrokeWidthMode::ScreenSpace,
                    opacity: 1.0,
                    ..Style::default()
                },
            )
        }
    };
    let mut source = CompiledObject::new(
        ObjectId::new(2),
        geometry,
        Transform2D {
            translation: Vec2::new(if offscreen { -4.75 } else { 0.173 }, -0.217),
            rotation: if offscreen { 0.0 } else { 0.37 },
            scale: if offscreen {
                Vec2::new(1.0, 1.0)
            } else {
                Vec2::new(-1.2, 0.8)
            },
        },
        source_style,
    );
    let effect = CompiledEffect {
        attachment: SemanticNodeId::new(900, u32::MAX),
        definition: (Glow::new(
            EffectUpdate::default()
                .radius(Pixels(3.25))
                .intensity(2.4)
                .color(Color::rgba(0.95, 0.35, 0.8, 0.8))
                .source(if silhouette {
                    GlowSource::Silhouette
                } else {
                    GlowSource::Painted
                }),
        )
        .unwrap())
        .into(),
    };
    source.effect = Some(Arc::new(effect));
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
        EffectUpdate::default().intensity(0.0)
    } else {
        EffectUpdate::default()
            .radius(Pixels(4.75))
            .intensity(0.7)
            .color(Color::rgba(0.15, 0.85, 0.25, 0.5))
    };
    for (i, (property, values)) in effect
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
    if matches!(
        source_geometry,
        SourceGeometry::OutlinedCircle
            | SourceGeometry::StrokedPath
            | SourceGeometry::TransparentSilhouettePath
    ) {
        result
            .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
                id: TrackId::new(5),
                object: ObjectId::new(2),
                property: Property::Reveal,
                values: TrackValues::Scalar {
                    from: 0.15,
                    to: 1.0,
                },
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
        row.effect = None;
    }
    if index == 1 {
        value.objects[1].style.opacity = 1.0;
    }
    if matches!(stage, 2 | 3 | 5) {
        let silhouette = stage == 3
            && frame.objects[1]
                .effect
                .as_ref()
                .unwrap()
                .definition
                .as_glow()
                .unwrap()
                .source()
                == GlowSource::Silhouette;
        let white_with_alpha = |paint: Option<Color>| {
            paint
                .map(|color| Color::rgba(1.0, 1.0, 1.0, if silhouette { 1.0 } else { color.alpha }))
        };
        let style = &mut value.objects[index].style;
        style.fill = white_with_alpha(style.fill);
        style.stroke = white_with_alpha(style.stroke);
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
    for (i, (name, rectangle, offscreen, silhouette, intensity_only, source_geometry)) in [
        (
            "circle-motion",
            false,
            false,
            false,
            false,
            SourceGeometry::Analytic,
        ),
        (
            "rectangle-motion",
            true,
            false,
            false,
            false,
            SourceGeometry::Analytic,
        ),
        (
            "offscreen-circle",
            false,
            true,
            false,
            false,
            SourceGeometry::Analytic,
        ),
        (
            "offscreen-silhouette",
            true,
            true,
            true,
            false,
            SourceGeometry::Analytic,
        ),
        (
            "intensity-to-zero",
            false,
            false,
            false,
            true,
            SourceGeometry::Analytic,
        ),
        (
            "outlined-circle-reveal",
            false,
            false,
            false,
            false,
            SourceGeometry::OutlinedCircle,
        ),
        (
            "cubic-path-reveal",
            false,
            false,
            false,
            false,
            SourceGeometry::StrokedPath,
        ),
        (
            "cubic-path-silhouette-reveal",
            false,
            false,
            true,
            false,
            SourceGeometry::TransparentSilhouettePath,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let mut runtime = runtime(
            rectangle,
            offscreen,
            silhouette,
            intensity_only,
            source_geometry,
        );
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
            let effect = frame.objects[1].effect.as_ref().unwrap().definition;
            let color = effect.as_glow().unwrap().color();
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
                sigma: effect.radius().value(),
                intensity: effect.as_glow().unwrap().intensity(),
                tint: [color.red, color.green, color.blue, color.alpha],
                opacity: frame.objects[1].style.opacity,
                offscreen,
            });
        }
        for id in if intensity_only {
            vec![2]
        } else if matches!(source_geometry, SourceGeometry::Analytic) {
            vec![1, 2, 3, 4]
        } else {
            vec![1, 2, 3, 4, 5]
        } {
            runtime
                .apply_execution_patch(&ExecutionPatch::RemoveTrack(TrackId::new(id)))
                .unwrap();
        }
        let mut effect = **runtime.frame().objects[1].effect.as_ref().unwrap();
        effect.definition = EffectUpdate::default()
            .intensity(0.0)
            .apply_to(effect.definition)
            .unwrap();
        runtime
            .apply_execution_patch(&ExecutionPatch::SetEffect {
                object: ObjectId::new(2),
                effect: Arc::new(effect),
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
            row.effect = None;
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
        assert_eq!(fixture.cases.len(), 8);
        assert_eq!(
            fixture.cases[5..]
                .iter()
                .map(|case| case.name)
                .collect::<Vec<_>>(),
            [
                "outlined-circle-reveal",
                "cubic-path-reveal",
                "cubic-path-silhouette-reveal"
            ]
        );
        for case in fixture.cases {
            let mut installed =
                crate::InstalledRetainedExecutionMirror::from_bundle_bytes(&fixture.resources)
                    .unwrap();
            assert_eq!(case.frames.len(), 5);
            let first = &case.frames[0].delta;
            assert!(first.snapshot);
            for (sample_index, sample) in case.frames.iter().enumerate() {
                assert_eq!(
                    sample.delta.protocol_version,
                    crate::RETAINED_EXECUTION_TRANSPORT_VERSION
                );
                let envelope =
                    serde_json::from_value(serde_json::to_value(&sample.delta).unwrap()).unwrap();
                installed.apply(envelope).unwrap();
                let frame = installed.frame().unwrap();
                let row = &frame.objects[1];
                let wire_row = sample
                    .delta
                    .objects
                    .iter()
                    .find(|row| row.order == 1)
                    .expect("animated source row is transported");
                assert_eq!(wire_row.effect.unwrap().attachment.generation(), u32::MAX);
                assert_eq!(
                    match wire_row.effect.unwrap().definition {
                        crate::retained_execution_transport::TransportEffectDefinition::Glow {
                            intensity,
                            ..
                        } => intensity,
                        _ => panic!("expected Glow"),
                    },
                    sample.intensity
                );
                if case.name.ends_with("-reveal") {
                    let expected_reveal =
                        0.15 + 0.85 * [0.0, 0.37, 0.8, 1.0, 0.0][sample_index] as f32;
                    assert!((frame.reveal(1) - expected_reveal).abs() < 1e-6);
                    assert!(frame.render_geometries[1].is_none());
                }
                if case.name.starts_with("cubic-path-") {
                    assert!(matches!(
                        &row.content,
                        noon_core::ObjectContentRef::Geometry(GeometryRef::VectorPath(path))
                            if path.has_drawable_segments()
                    ));
                }
                if case.name == "cubic-path-reveal" && sample_index == 1 {
                    assert!((row.transform.translation.x - 0.173 - 0.037).abs() < 1e-6);
                    assert!((frame.reveal(1) - (0.15 + 0.85 * 0.37)).abs() < 1e-6);
                    assert!((match wire_row.effect.unwrap().definition { crate::retained_execution_transport::TransportEffectDefinition::Glow { intensity, .. } => intensity, _ => panic!("expected Glow") } - 1.771).abs() < 1e-6);
                }
            }
            if case.name == "outlined-circle-reveal" {
                let style = &case.frames[0].references[2].objects[1].style;
                assert_eq!(style.fill.unwrap().alpha, 0.24);
                assert_eq!(style.stroke.unwrap().alpha, 0.82);
                assert_eq!(style.stroke.unwrap().red, 1.0);
            }
            if case.name == "cubic-path-reveal" {
                let source_alpha = &case.frames[0].references[2].objects[1].style;
                let mask = &case.frames[0].references[3].objects[1].style;
                assert_eq!(source_alpha.stroke.unwrap().alpha, 0.76);
                assert_eq!(mask.stroke.unwrap().alpha, 0.76);
                assert_eq!(mask.stroke.unwrap().green, 1.0);
            }
            if case.name == "cubic-path-silhouette-reveal" {
                let source_alpha = &case.frames[0].references[2].objects[1].style;
                let mask = &case.frames[0].references[3].objects[1].style;
                assert_eq!(source_alpha.stroke.unwrap().alpha, 0.0);
                assert_eq!(mask.stroke.unwrap().alpha, 1.0);
            }
            assert_eq!(case.frames[0].intensity, case.frames[4].intensity);
            let neutral =
                serde_json::from_value(serde_json::to_value(&case.neutral).unwrap()).unwrap();
            installed.apply(neutral).unwrap();
            assert!(installed.frame().unwrap().objects[1]
                .effect
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
