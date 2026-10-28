//! Child coverage opacity survives capture without depending on glyph paint alpha.
use super::*;
use noon_compile::ExecutionPatch;
use noon_core::{ObjectContentRef, TextResourceArena};
fn check(source: &PreparedSource, index: u32, expected: f32) {
    let mut count = 0;
    for draw in &source.draws {
        if let SourceDraw::Glyph {
            item: PreparedTextItem::GlyphBatch { object_index, .. },
            bytes,
        } = draw
        {
            if *object_index != index {
                continue;
            }
            for offset in (bytes.start as usize..bytes.end as usize)
                .step_by(std::mem::size_of::<GlyphQuadInstance>())
            {
                let quad = bytemuck::pod_read_unaligned::<GlyphQuadInstance>(
                    &source.bytes[offset..offset + std::mem::size_of::<GlyphQuadInstance>()],
                );
                assert!(
                    (quad.color[3] - expected).abs() <= 1e-7,
                    "capture glyph alpha {}, expected {expected}",
                    quad.color[3]
                );
                count += 1;
            }
        }
    }
    assert!(count > 0, "the real prepared glyph lane must be exercised");
}
#[test]
fn grouped_glyph_coverage_preserves_child_opacity_independently_of_zero_paint_alpha() {
    let artifact =
        noon_typst::compile_typst_resource("Noon", noon_typst::TypstMode::Markup).unwrap();
    let mut texts = TextResourceArena::new();
    let handle = texts.insert(artifact.resource).unwrap();
    let mut resources = CompiledResources::default();
    let bounds = resources
        .capture_text_from_arenas(
            &texts,
            &artifact.fonts,
            &noon_core::GeometryResourceArena::new(),
            handle,
        )
        .unwrap();
    let rows = (0..2)
        .map(|i| {
            let mut row = CompiledObject::new(
                ObjectId::new(i + 1),
                ObjectContentRef::Text(handle),
                Transform2D::IDENTITY,
                Style {
                    fill: Some(Color::WHITE),
                    stroke: None,
                    opacity: 0.4,
                    ..Style::default()
                },
            );
            row.text_bounds = Some(bounds);
            row
        })
        .collect();
    let mut compiled = CompiledScene::compile_objects(rows, &[]).unwrap();
    compiled.merge_prepared_resources(resources);
    let mut runtime = SceneInstance::new(compiled);
    let (device, _queue) = wgpu::Device::noop(&Default::default());
    let mut preparer = RetainedFramePreparer::new();
    let alignment = device.limits().min_uniform_buffer_offset_alignment as usize;
    for (opacity, alpha) in [(0.7, 0.4), (0.25, 0.0), (0.0, 0.6), (0.65, 1.0), (0.7, 0.4)] {
        runtime
            .apply_execution_patch(&ExecutionPatch::SetStyle {
                object: ObjectId::new(2),
                style: Style {
                    fill: Some(Color::rgba(0.9, 0.7, 0.1, alpha)),
                    stroke: None,
                    opacity,
                    ..Style::default()
                },
                fill_paint: None,
            })
            .unwrap();
        let publication = runtime.take_renderer_publication();
        let prepared = preparer
            .prepare_planned_publication_visible(
                &device,
                &publication,
                &[0, 1],
                TextDeviceMetrics::uniform(20.0).unwrap(),
            )
            .unwrap();
        let members = [(ObjectId::new(1), 1.0), (ObjectId::new(2), 1.0)];
        let painted = PreparedSource::prepare(&prepared, &members, false, alignment).unwrap();
        let original = PreparedSource::prepare(&prepared, &members, true, alignment).unwrap();
        check(&painted, 0, 0.4);
        check(&original, 0, 0.4);
        check(&painted, 1, opacity * alpha);
        check(&original, 1, opacity);
        if opacity > 0.0 {
            let leaf = [(ObjectId::new(2), opacity)];
            let painted = PreparedSource::prepare(&prepared, &leaf, false, alignment).unwrap();
            let original = PreparedSource::prepare(&prepared, &leaf, true, alignment).unwrap();
            check(&painted, 1, alpha);
            check(&original, 1, 1.0);
        }
    }
}
