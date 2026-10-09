//! Submission compaction of adjacent instances and bounded disjoint geometry.
//! No instance relocation, mesh duplication, or persistent draw-order cache.
use noon_core::{Rect, Vec2};

use super::retained_text::RetainedRenderItem;
use crate::{OrderedRenderBatch, PackedTransform, PreparedFrame, RenderPrimitive};

const WINDOW: usize = 32;

pub(super) struct GeometryDrawBatches {
    batches: [Option<(OrderedRenderBatch, std::ops::Range<u32>)>; WINDOW],
    bounds: [Option<Rect>; WINDOW],
    count: usize,
}

impl GeometryDrawBatches {
    pub(super) fn collect<'a>(
        first: &OrderedRenderBatch,
        rest: &mut std::iter::Peekable<std::slice::Iter<'a, RetainedRenderItem>>,
        prepared: &PreparedFrame<'_>,
        pixel: Vec2,
        enabled: bool,
    ) -> Self {
        let mut first = first.clone();
        if enabled {
            // This prefix keeps the existing primitive and instance order, so
            // overlapping and polygon-covered paths need no bounds test. Mixed
            // items still stop the prefix; exclusions/insets disable collection.
            while let Some(RetainedRenderItem::Geometry { batch, .. }) = rest.peek() {
                let mut next = batch.clone();
                if let (
                    RenderPrimitive::Path { batch: first_path },
                    RenderPrimitive::Path { batch: next_path },
                ) = (first.primitive, next.primitive)
                {
                    let first_path = &prepared.path_batches[first_path];
                    let next_path = &prepared.path_batches[next_path];
                    // Structural appends can alias the same resident geometry
                    // through different batch IDs. Only identical indices and
                    // pipeline classification permit sharing the first draw.
                    if first_path.index_range == next_path.index_range
                        && first_path.polygon_coverage == next_path.polygon_coverage
                    {
                        next.primitive = first.primitive;
                    }
                }
                if !first.merge_adjacent(&next) {
                    break;
                }
                rest.next();
            }
        }
        let mut result = Self {
            batches: std::array::from_fn(|_| None),
            bounds: [None; WINDOW],
            count: 0,
        };
        result.batches[0] = Some((first.clone(), 0..0));
        let Some(bounds) = enabled
            .then(|| geometry_bounds(prepared, &first, pixel))
            .flatten()
        else {
            return result;
        };
        result.batches[0] = Some((first.clone(), path_indices(prepared, &first)));
        result.bounds[0] = Some(bounds);
        result.count = 1;
        while result.count < WINDOW {
            let Some(RetainedRenderItem::Geometry { batch, .. }) = rest.peek() else {
                break;
            };
            let Some(bounds) = geometry_bounds(prepared, batch, pixel) else {
                break;
            };
            if !result.push(batch, bounds, path_indices(prepared, batch)) {
                break;
            }
            rest.next();
        }
        result
    }

    fn push(
        &mut self,
        batch: &OrderedRenderBatch,
        bounds: Rect,
        indices: std::ops::Range<u32>,
    ) -> bool {
        if self.count == WINDOW
            || self.bounds[..self.count]
                .iter()
                .flatten()
                .any(|prior| overlaps(*prior, bounds))
        {
            return false;
        }
        let slot = self.batches.iter_mut().find(|slot| {
            slot.as_ref().is_some_and(|(prior, prior_indices)| {
                (prior.primitive == batch.primitive
                    || matches!(
                        (prior.primitive, batch.primitive),
                        (RenderPrimitive::Path { .. }, RenderPrimitive::Path { .. })
                    ))
                    && *prior_indices == indices
                    && prior.instance_range.end == batch.instance_range.start
            })
        });
        if let Some(Some((prior, _))) = slot {
            prior.instance_range.end = batch.instance_range.end;
        } else {
            *self
                .batches
                .iter_mut()
                .find(|slot| slot.is_none())
                .expect("bounded window") = Some((batch.clone(), indices));
        }
        self.bounds[self.count] = Some(bounds);
        self.count += 1;
        true
    }

    pub(super) fn batches(&self) -> impl Iterator<Item = &OrderedRenderBatch> {
        self.batches.iter().flatten().map(|(batch, _)| batch)
    }
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.min.x <= b.max.x && b.min.x <= a.max.x && a.min.y <= b.max.y && b.min.y <= a.max.y
}

fn path_indices(prepared: &PreparedFrame<'_>, batch: &OrderedRenderBatch) -> std::ops::Range<u32> {
    match batch.primitive {
        RenderPrimitive::Path { batch } => prepared.path_batches[batch].index_range.clone(),
        _ => 0..0,
    }
}

fn geometry_bounds(
    prepared: &PreparedFrame<'_>,
    batch: &OrderedRenderBatch,
    pixel: Vec2,
) -> Option<Rect> {
    if batch.instance_range.len() != 1 {
        return None;
    }
    let index = batch.instance_range.start as usize;
    let (half_size, transform, style, outlined) = match batch.primitive {
        RenderPrimitive::Circle => {
            let instance = &prepared.circles[index];
            if !instance.radius.is_finite() {
                return None;
            }
            let radius = instance.radius.abs().max(0.000001);
            (
                Vec2::new(radius, radius),
                instance.transform,
                instance.style,
                instance.style.stroke_enabled & 1 != 0
                    || (instance.padding[0] < 1.0 && instance.style.fill_enabled & 1 != 0),
            )
        }
        RenderPrimitive::Rectangle => {
            let instance = &prepared.rectangles[index];
            (
                Vec2::new(instance.size[0].abs() * 0.5, instance.size[1].abs() * 0.5),
                instance.transform,
                instance.style,
                instance.style.stroke_enabled & 1 != 0,
            )
        }
        _ => return path_bounds(prepared, batch, pixel),
    };
    if !style.stroke_width.is_finite() || !pixel.x.is_finite() || !pixel.y.is_finite() {
        return None;
    }
    let half_width = if outlined {
        style.stroke_width.max(0.0) * 0.5
    } else {
        0.0
    };
    let outline = if style.stroke_enabled & 2 != 0 {
        Vec2::new(
            half_width / transform.scale[0].abs().max(0.000001),
            half_width / transform.scale[1].abs().max(0.000001),
        )
    } else {
        Vec2::new(half_width, half_width)
    };
    let extent = half_size + outline;
    let bounds = Rect::new(Vec2::ZERO - extent, extent);
    // Each analytic quad axis adds at most one pixel in world length.
    // Two maximum-axis pixels conservatively cover their rotated sum, including
    // nonuniform/mirrored scales and screen-space stroke padding.
    let fringe = pixel.x.max(pixel.y) * 2.0;
    transformed_bounds(bounds, [bounds; 2], transform, Vec2::new(fringe, fringe))
}

fn path_bounds(
    prepared: &PreparedFrame<'_>,
    batch: &OrderedRenderBatch,
    pixel: Vec2,
) -> Option<Rect> {
    let RenderPrimitive::Path { batch: path } = batch.primitive else {
        return None;
    };
    if batch.instance_range.len() != 1 || prepared.path_batches[path].polygon_coverage {
        return None;
    }
    let mesh = &prepared.path_mesh_cache[prepared.path_batch_cache_indices[path]];
    if mesh.sampled.is_some() {
        return None;
    }
    let [source, target] = mesh.bounds?;
    let instance = &prepared.paths[batch.instance_range.start as usize];
    let progress = instance.path_params[1].clamp(0.0, 1.0);
    let bounds = Rect::new(
        source.min * (1.0 - progress) + target.min * progress,
        source.max * (1.0 - progress) + target.max * progress,
    );
    transformed_bounds(bounds, [source, target], instance.transform, pixel)
}

fn transformed_bounds(
    bounds: Rect,
    [source, target]: [Rect; 2],
    transform: PackedTransform,
    pixel: Vec2,
) -> Option<Rect> {
    // WGSL gives a finite sin/cos accuracy guarantee only inside [-pi, pi].
    // Keep the ordinary painter path outside that range (including NaN).
    if !transform.rotation.is_finite() || transform.rotation.abs() > std::f32::consts::PI {
        return None;
    }
    let (sin, cos) = transform.rotation.sin_cos();
    let mut min = Vec2::new(f32::INFINITY, f32::INFINITY);
    let mut max = Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for x in [bounds.min.x, bounds.max.x] {
        for y in [bounds.min.y, bounds.max.y] {
            let x = x * transform.scale[0];
            let y = y * transform.scale[1];
            let x_world = cos * x - sin * y + transform.translation[0];
            let y_world = sin * x + cos * y + transform.translation[1];
            if !x_world.is_finite() || !y_world.is_finite() {
                return None;
            }
            min.x = min.x.min(x_world);
            min.y = min.y.min(y_world);
            max.x = max.x.max(x_world);
            max.y = max.y.max(y_world);
        }
    }
    // Cover raster samples, WGSL's 2^-11 sin/cos error, and interpolation /
    // transform rounding, including large coordinates cancelled by translation.
    // https://www.w3.org/TR/WGSL/#floating-point-accuracy
    let magnitude = [source.min.x, source.max.x, target.min.x, target.max.x]
        .into_iter()
        .map(f32::abs)
        .fold(0.0_f32, f32::max)
        * transform.scale[0].abs()
        + [source.min.y, source.max.y, target.min.y, target.max.y]
            .into_iter()
            .map(f32::abs)
            .fold(0.0_f32, f32::max)
            * transform.scale[1].abs();
    let pad = Vec2::new(
        pixel.x + magnitude * 0.001 + transform.translation[0].abs() * 0.00001,
        pixel.y + magnitude * 0.001 + transform.translation[1].abs() * 0.00001,
    );
    let result = Rect::new(min - pad, max + pad);
    (pad.x.is_finite()
        && pad.y.is_finite()
        && result.min.x.is_finite()
        && result.min.y.is_finite()
        && result.max.x.is_finite()
        && result.max.y.is_finite())
    .then_some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn batch(mesh: usize, instance: u32) -> OrderedRenderBatch {
        OrderedRenderBatch {
            primitive: RenderPrimitive::Path { batch: mesh },
            instance_range: instance..instance + 1,
        }
    }
    fn bounds(x: f32) -> Rect {
        Rect::new(Vec2::new(x, 0.0), Vec2::new(x + 0.5, 0.5))
    }
    fn empty() -> GeometryDrawBatches {
        GeometryDrawBatches {
            batches: std::array::from_fn(|_| None),
            bounds: [None; WINDOW],
            count: 0,
        }
    }

    #[test]
    fn adjacent_overlapping_instances_keep_order_and_stop_at_text_or_gaps() {
        use noon_core::{GeometryRef, ObjectId, VectorPath};
        use noon_runtime::FrameChanges;
        let polygon = VectorPath::new()
            .move_to(Vec2::new(-0.5, -0.5))
            .line_to(Vec2::new(0.5, -0.5))
            .line_to(Vec2::new(0.5, 0.5))
            .line_to(Vec2::new(-0.5, 0.5))
            .close();
        for (geometry, primitive) in [
            (GeometryRef::circle(0.5), RenderPrimitive::Circle),
            (GeometryRef::rectangle(1.0, 1.0), RenderPrimitive::Rectangle),
            (
                GeometryRef::line(Vec2::ZERO, Vec2::new(1.0, 0.0)),
                RenderPrimitive::Line,
            ),
            (
                GeometryRef::path(polygon),
                RenderPrimitive::Path { batch: 0 },
            ),
        ] {
            let frame = crate::tests::frame(
                (0..601)
                    .map(|index| {
                        let mut object = crate::tests::object(index, geometry.clone());
                        object.style.stroke = None;
                        object.style.stroke_width = 0.0;
                        object.style.opacity = 0.5;
                        object
                    })
                    .collect(),
            );
            let mut preparer = crate::FramePreparer::new();
            preparer.prepare(&crate::tests::frame(vec![frame.objects[0].clone()]));
            let prepared = preparer.prepare_incremental(
                &frame,
                &FrameChanges::structural((1..601).collect(), Vec::new()),
            );
            assert_eq!(prepared.stats.full_rebuilds, 0);
            assert_eq!(prepared.stats.path_vertices_repacked, 0);
            assert_eq!(prepared.stats.path_indices_repacked, 0);
            if matches!(primitive, RenderPrimitive::Path { .. }) {
                assert_eq!(prepared.path_batches.len(), 601);
                assert!(prepared.path_batches[0].polygon_coverage);
                assert!(prepared.path_batches.iter().all(|batch| {
                    batch.index_range == prepared.path_batches[0].index_range
                        && batch.polygon_coverage
                }));
            }
            let first = OrderedRenderBatch {
                primitive,
                instance_range: 0..1,
            };
            let item = |index| RetainedRenderItem::Geometry {
                object_id: ObjectId::new(u64::from(index)),
                batch: OrderedRenderBatch {
                    primitive: match primitive {
                        RenderPrimitive::Path { .. } => RenderPrimitive::Path {
                            batch: index as usize,
                        },
                        other => other,
                    },
                    instance_range: index..index + 1,
                },
            };
            let glyph = RetainedRenderItem::Glyph {
                object_id: ObjectId::new(999),
                object_index: 601,
                run_index: 0,
            };
            let mut items: Vec<_> = (1..600).map(item).collect();
            items.push(glyph);
            items.push(item(600));
            let mut rest = items.iter().peekable();
            let collected = GeometryDrawBatches::collect(
                &first,
                &mut rest,
                &prepared,
                Vec2::new(0.01, 0.01),
                true,
            );
            let batches: Vec<_> = collected.batches().collect();
            assert_eq!(batches.len(), 1, "600 overlapping {primitive:?} instances");
            assert_eq!(batches[0].instance_range, 0..600);
            assert_eq!(
                rest.count(),
                2,
                "text and the later instance remain ordered"
            );

            // Exclusions/images/insets disable collection at the existing caller.
            let mut rest = items.iter().peekable();
            let uncollected = GeometryDrawBatches::collect(
                &first,
                &mut rest,
                &prepared,
                Vec2::new(0.01, 0.01),
                false,
            );
            assert_eq!(uncollected.batches().next().unwrap(), &first);
            assert_eq!(rest.count(), items.len());

            let gap = [item(2)];
            let mut rest = gap.iter().peekable();
            let collected = GeometryDrawBatches::collect(
                &first,
                &mut rest,
                &prepared,
                Vec2::new(0.01, 0.01),
                true,
            );
            assert_eq!(collected.batches().next().unwrap(), &first);
            assert_eq!(rest.count(), 1, "an absent instance must stay absent");

            if matches!(primitive, RenderPrimitive::Path { .. }) {
                let original = prepared.path_batches;
                let mut different_indices = original.to_vec();
                different_indices[1].index_range.end += 1;
                let mut different_pipeline = original.to_vec();
                different_pipeline[1].polygon_coverage = false;
                let mut prepared = prepared;
                for aliases in [&different_indices[..], &different_pipeline[..]] {
                    prepared.path_batches = aliases;
                    let next = [item(1)];
                    let mut rest = next.iter().peekable();
                    let collected = GeometryDrawBatches::collect(
                        &first,
                        &mut rest,
                        &prepared,
                        Vec2::new(0.01, 0.01),
                        true,
                    );
                    assert_eq!(collected.batches().next().unwrap(), &first);
                    assert_eq!(
                        rest.count(),
                        1,
                        "a different index range or pipeline cannot alias"
                    );
                }
            }
        }
    }

    #[test]
    fn disjoint_analytic_primitives_keep_their_separate_instance_arrays() {
        let mut group = empty();
        for index in 0..WINDOW {
            let primitive = if index % 2 == 0 {
                RenderPrimitive::Circle
            } else {
                RenderPrimitive::Rectangle
            };
            let instance = (index / 2) as u32;
            assert!(group.push(
                &OrderedRenderBatch {
                    primitive,
                    instance_range: instance..instance + 1
                },
                bounds(index as f32),
                0..0,
            ));
        }
        let batches: Vec<_> = group.batches().collect();
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].primitive, RenderPrimitive::Circle);
        assert_eq!(batches[1].primitive, RenderPrimitive::Rectangle);
        assert!(batches.iter().all(|batch| batch.instance_range == (0..16)));
    }

    #[test]
    fn disjoint_alternating_meshes_share_existing_contiguous_instances() {
        let mut group = empty();
        for index in 0..WINDOW {
            assert!(group.push(
                &batch(index % 2, (index / 2) as u32),
                bounds(index as f32),
                (index % 2) as u32..(index % 2 + 1) as u32
            ));
        }
        let batches: Vec<_> = group.batches().collect();
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].instance_range, 0..16);
        assert_eq!(batches[1].instance_range, 0..16);
        assert!(
            !group.push(&batch(0, 16), bounds(40.0), 0..1),
            "work stays bounded"
        );
    }
    #[test]
    fn overlap_touching_and_instance_gaps_never_merge() {
        let mut group = empty();
        assert!(group.push(&batch(0, 0), bounds(0.0), 0..1));
        assert!(!group.push(&batch(1, 0), bounds(0.25), 0..1));
        assert!(!group.push(&batch(1, 0), bounds(0.5), 0..1));
        assert!(group.push(&batch(0, 2), bounds(2.0), 0..1));
        assert_eq!(group.batches().count(), 2, "absent instance stays absent");
        assert!(group.push(&batch(7, 3), bounds(4.0), 0..1));
        assert_eq!(
            group.batches().count(),
            2,
            "shared index range merges across batch IDs"
        );
        assert!(group.push(&batch(7, 4), bounds(6.0), 3..4));
        assert_eq!(
            group.batches().count(),
            3,
            "different index ranges remain separate"
        );
    }
    #[test]
    fn bounds_cover_stroked_morph_vertices_with_mirrored_rotated_transform() {
        use noon_core::{
            Color, GeometryRef, ObjectContentRef, ObjectId, Style, Transform2D, VectorPath,
        };
        use noon_runtime::{FrameObjectState, FrameState};
        let path = VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.0))
            .line_to(Vec2::new(1.0, 0.0))
            .with_morph_target(
                VectorPath::new()
                    .move_to(Vec2::new(0.0, -2.0))
                    .line_to(Vec2::new(0.0, 2.0)),
            );
        let transform = Transform2D {
            translation: Vec2::new(3.0, -4.0),
            scale: Vec2::new(-2.0, 0.5),
            rotation: 0.7,
        };
        let mut frame = FrameState {
            time: 0.0,
            objects: vec![FrameObjectState {
                spatial: None,
                id: ObjectId::new(1),
                z_index: 0.0,
                content: ObjectContentRef::Geometry(GeometryRef::path(path)),
                text_bounds: None,
                transform,
                style: Style {
                    fill: None,
                    stroke: Some(Color::WHITE),
                    stroke_width: 0.2,
                    ..Style::default()
                },
                appearance: 1.0,
            }],
            presences: vec![true],
            reveals: vec![0.5],
            morphs: vec![0.5],
            render_geometries: vec![None],
            render_transforms: vec![None],
            family_animations: vec![],
            family_animation_plan_indices: vec![],
        };
        let mut preparer = crate::FramePreparer::new();
        preparer.prepare(&frame);
        let first = batch(0, 0);
        let (sin, cos) = transform.rotation.sin_cos();
        for progress in [0.0, 0.25, 0.5, 0.75, 1.0] {
            frame.morphs[0] = progress;
            let prepared =
                preparer.prepare_incremental(&frame, &noon_runtime::FrameChanges::objects(vec![0]));
            assert_eq!(prepared.stats.geometry_cache_misses, 0);
            let box_world = path_bounds(&prepared, &first, Vec2::new(0.01, 0.02)).unwrap();
            for vertex in prepared.path_vertices {
                let x = (vertex.position[0] * (1.0 - progress)
                    + vertex.target_position[0] * progress)
                    * transform.scale.x;
                let y = (vertex.position[1] * (1.0 - progress)
                    + vertex.target_position[1] * progress)
                    * transform.scale.y;
                let world = Vec2::new(cos * x - sin * y, sin * x + cos * y) + transform.translation;
                assert!(world.x > box_world.min.x && world.x < box_world.max.x);
                assert!(world.y > box_world.min.y && world.y < box_world.max.y);
            }
        }
        let prepared =
            preparer.prepare_incremental(&frame, &noon_runtime::FrameChanges::objects(vec![]));
        let glyph = RetainedRenderItem::Glyph {
            object_id: ObjectId::new(2),
            object_index: 0,
            run_index: 0,
        };
        let items = [glyph];
        let mut rest = items.iter().peekable();
        assert_eq!(
            GeometryDrawBatches::collect(&first, &mut rest, &prepared, Vec2::new(0.01, 0.02), true)
                .batches()
                .count(),
            1
        );
        assert!(
            rest.peek().is_some(),
            "text remains an untouched painter-order barrier"
        );
    }
}
