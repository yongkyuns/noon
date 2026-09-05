use noon_core::FamilyAnimationMode;

/// Failure while selecting and realizing one active retained family operation.
///
/// The renderer chooses the operation once from the runtime state attached to the
/// immutable family plan. Browser/WASM callers stay operation-agnostic, and every
/// concrete preparation path keeps its own content-specific validation.
#[derive(Clone, Debug, PartialEq)]
pub enum RetainedFamilyAnimationPrepareError {
    Retained(RetainedPrepareError),
    Reveal(RetainedFamilyPrepareError),
    DrawBorderThenFill(RetainedFamilyDrawBorderPrepareError),
    InconsistentModes {
        first_object: ObjectId,
        first_mode: FamilyAnimationMode,
        object: ObjectId,
        mode: FamilyAnimationMode,
    },
}

impl std::fmt::Display for RetainedFamilyAnimationPrepareError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retained(error) => error.fmt(formatter),
            Self::Reveal(error) => error.fmt(formatter),
            Self::DrawBorderThenFill(error) => error.fmt(formatter),
            Self::InconsistentModes {
                first_object,
                first_mode,
                object,
                mode,
            } => write!(
                formatter,
                "retained family plan resolved inconsistent active modes: object {} is {first_mode:?}, object {} is {mode:?}",
                first_object.get(),
                object.get()
            ),
        }
    }
}

impl std::error::Error for RetainedFamilyAnimationPrepareError {}

impl From<RetainedPrepareError> for RetainedFamilyAnimationPrepareError {
    fn from(value: RetainedPrepareError) -> Self {
        Self::Retained(value)
    }
}

impl From<RetainedFamilyPrepareError> for RetainedFamilyAnimationPrepareError {
    fn from(value: RetainedFamilyPrepareError) -> Self {
        Self::Reveal(value)
    }
}

impl From<RetainedFamilyDrawBorderPrepareError> for RetainedFamilyAnimationPrepareError {
    fn from(value: RetainedFamilyDrawBorderPrepareError) -> Self {
        Self::DrawBorderThenFill(value)
    }
}

impl RetainedFramePreparer {
    /// Prepare one retained family frame without exposing operation selection to the caller.
    ///
    /// Outside an active family interval there is no family state to realize, so this
    /// deliberately falls through to ordinary retained preparation. During an active
    /// interval every planned leaf must agree on one operation mode; a mixed mode fails
    /// closed rather than rendering different semantics for members of one global plan.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_family_animation<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &RetainedFamilyFrame<'_>,
        plan: &RetainedFamilyAnimationPlan,
        texts: &(impl TextResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
        geometries: &(impl GeometryResourceLookup + ?Sized),
        metrics: TextDeviceMetrics,
    ) -> Result<PreparedRetainedGpuFrame<'a>, RetainedFamilyAnimationPrepareError> {
        let changes = FrameChanges::all();
        self.prepare_family_animation_with_changes(
            device, queue, frame, plan, &changes, texts, fonts, geometries, metrics,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare_family_animation_with_changes<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &RetainedFamilyFrame<'_>,
        plan: &RetainedFamilyAnimationPlan,
        changes: &FrameChanges,
        texts: &(impl TextResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
        geometries: &(impl GeometryResourceLookup + ?Sized),
        metrics: TextDeviceMetrics,
    ) -> Result<PreparedRetainedGpuFrame<'a>, RetainedFamilyAnimationPrepareError> {
        match active_family_mode(frame, plan)? {
            Some(FamilyAnimationMode::Reveal) => self
                .prepare_family_with_changes(
                    device, queue, frame, plan, changes, texts, fonts, geometries, metrics,
                )
                .map_err(Into::into),
            Some(FamilyAnimationMode::DrawBorderThenFill) => self
                .prepare_family_draw_border_then_fill_with_changes(
                    device, queue, frame, plan, changes, texts, fonts, geometries, metrics,
                )
                .map_err(Into::into),
            None => self
                .prepare_with_changes(
                    device,
                    queue,
                    frame.retained,
                    changes,
                    texts,
                    fonts,
                    geometries,
                    metrics,
                )
                .map_err(Into::into),
        }
    }
}

fn active_family_mode(
    frame: &RetainedFamilyFrame<'_>,
    plan: &RetainedFamilyAnimationPlan,
) -> Result<Option<FamilyAnimationMode>, RetainedFamilyAnimationPrepareError> {
    let mut selected: Option<(ObjectId, FamilyAnimationMode)> = None;

    for (object_index, object) in frame.retained.objects.iter().enumerate() {
        if plan.leaf_for_object(object.id).is_none() {
            continue;
        }
        let Some(state) = frame.family_animation(object_index) else {
            continue;
        };
        match selected {
            None => selected = Some((object.id, state.mode)),
            Some((_, mode)) if mode == state.mode => {}
            Some((first_object, first_mode)) => {
                return Err(RetainedFamilyAnimationPrepareError::InconsistentModes {
                    first_object,
                    first_mode,
                    object: object.id,
                    mode: state.mode,
                });
            }
        }
    }

    Ok(selected.map(|(_, mode)| mode))
}

#[cfg(test)]
mod operation_selection_tests {
    use noon_core::{
        FamilyAnimationState, GeometryRef, ObjectContentRef, RateFunction,
        RetainedFamilyAnimationPlanBuilder, RetainedObjectDefinition, SemanticStore, Style,
        TextResourceArena, Transform2D,
    };
    use noon_runtime::{FrameObjectState, FrameState};

    use super::*;

    fn state(mode: FamilyAnimationMode) -> FamilyAnimationState {
        FamilyAnimationState {
            mode,
            overall_progress: 0.5,
            lag_ratio: 0.0,
            rate_function: RateFunction::Linear,
            reverse_rate_function: false,
            reverse_member_order: false,
        }
    }

    fn fixture() -> (
        RetainedFamilyAnimationPlan,
        FrameState,
        Vec<Option<FamilyAnimationState>>,
    ) {
        let mut semantics = SemanticStore::new();
        let first = semantics.insert_authoring_object();
        let second = semantics.insert_authoring_object();
        let family = semantics.insert_family();
        semantics.add_member(family, first).unwrap();
        semantics.add_member(family, second).unwrap();

        let first_object =
            RetainedObjectDefinition::geometry(ObjectId::new(10), GeometryRef::circle(1.0));
        let second_object =
            RetainedObjectDefinition::geometry(ObjectId::new(11), GeometryRef::circle(1.0));
        let texts = TextResourceArena::new();
        let mut builder = RetainedFamilyAnimationPlanBuilder::begin(&semantics, family).unwrap();
        builder.accept_leaf(first, &first_object, &texts).unwrap();
        builder.accept_leaf(second, &second_object, &texts).unwrap();
        let plan = builder.finish().unwrap();

        let frame = FrameState {
            time: 1.0,
            objects: vec![
                FrameObjectState {
                    id: ObjectId::new(10),
                    content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
                    text_bounds: None,
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                },
                FrameObjectState {
                    id: ObjectId::new(11),
                    content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
                    text_bounds: None,
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                },
                FrameObjectState {
                    id: ObjectId::new(99),
                    content: ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
                    text_bounds: None,
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                },
            ],
            presences: vec![true, true, true],
            reveals: vec![1.0, 1.0, 1.0],
            morphs: vec![0.0, 0.0, 0.0],
            render_geometries: vec![None, None, None],
            render_transforms: vec![None, None, None],
        };
        (
            plan,
            frame,
            vec![
                Some(state(FamilyAnimationMode::Reveal)),
                Some(state(FamilyAnimationMode::Reveal)),
                Some(state(FamilyAnimationMode::DrawBorderThenFill)),
            ],
        )
    }

    #[test]
    fn operation_selection_uses_only_active_members_of_the_plan() {
        let (plan, retained, states) = fixture();
        let frame = RetainedFamilyFrame {
            retained: &retained,
            family_animations: &states,
        };
        assert_eq!(
            active_family_mode(&frame, &plan).unwrap(),
            Some(FamilyAnimationMode::Reveal)
        );
    }

    #[test]
    fn operation_selection_falls_back_when_family_interval_is_inactive() {
        let (plan, retained, mut states) = fixture();
        states[0] = None;
        states[1] = None;
        let frame = RetainedFamilyFrame {
            retained: &retained,
            family_animations: &states,
        };
        assert_eq!(active_family_mode(&frame, &plan).unwrap(), None);
    }

    #[test]
    fn one_global_plan_cannot_mix_active_operation_modes() {
        let (plan, retained, mut states) = fixture();
        states[1] = Some(state(FamilyAnimationMode::DrawBorderThenFill));
        let frame = RetainedFamilyFrame {
            retained: &retained,
            family_animations: &states,
        };
        assert_eq!(
            active_family_mode(&frame, &plan).unwrap_err(),
            RetainedFamilyAnimationPrepareError::InconsistentModes {
                first_object: ObjectId::new(10),
                first_mode: FamilyAnimationMode::Reveal,
                object: ObjectId::new(11),
                mode: FamilyAnimationMode::DrawBorderThenFill,
            }
        );
    }
    #[test]
    fn family_reveal_keeps_fixed_morph_frame_and_warm_geometry() {
        let (plan, mut retained, mut states) = fixture();
        states[1] = None;
        states[2] = None;
        let source = noon_core::VectorPath::new()
            .move_to(noon_core::Vec2::new(-1.0, 0.0))
            .line_to(noon_core::Vec2::new(1.0, 0.0));
        let target = noon_core::VectorPath::new()
            .move_to(noon_core::Vec2::new(0.0, -1.0))
            .line_to(noon_core::Vec2::new(0.0, 1.0));
        retained.render_geometries[0] = Some(std::sync::Arc::new(GeometryRef::path(
            source.with_morph_target(target),
        )));
        retained.render_transforms[0] = Some(Transform2D::IDENTITY);
        retained.objects[0].transform = Transform2D {
            translation: noon_core::Vec2::new(3.0, -2.0),
            rotation: 0.7,
            scale: noon_core::Vec2::new(2.0, 0.5),
        };
        retained.objects[0].style.fill = None;
        retained.objects[0].style.stroke = Some(noon_core::Color::WHITE);
        retained.objects[0].style.stroke_width = 0.1;
        retained.objects[0].style.stroke_width_mode = noon_core::StrokeWidthMode::ScreenSpace;
        retained.morphs[0] = 0.3;
        let texts = TextResourceArena::new();
        let fonts = FontResourceArena::new();
        let geometries = GeometryResourceArena::new();
        let metrics = TextDeviceMetrics::uniform(100.0).unwrap();
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedFramePreparer::new();
        let mut renderer = GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        let mut text_state = renderer.create_retained_text_state(&device, &queue);
        let frame = RetainedFamilyFrame {
            retained: &retained,
            family_animations: &states,
        };
        let first = preparer
            .prepare_family_animation_with_changes(
                &device,
                &queue,
                &frame,
                &plan,
                &FrameChanges::all(),
                &texts,
                &fonts,
                &geometries,
                metrics,
            )
            .unwrap();
        assert_eq!(
            first.geometry.paths[0].transform,
            Transform2D::IDENTITY.into()
        );
        assert_no_unused_mega_streams(&first);
        let cold_upload = renderer.upload_retained(&device, &queue, &first, &mut text_state);
        assert!(cold_upload.geometry.bytes_uploaded > 0);

        retained.morphs[0] = 0.6;
        states[0].as_mut().unwrap().overall_progress = 0.7;
        let frame = RetainedFamilyFrame {
            retained: &retained,
            family_animations: &states,
        };
        let warm = preparer
            .prepare_family_animation_with_changes(
                &device,
                &queue,
                &frame,
                &plan,
                &FrameChanges::objects(vec![0]),
                &texts,
                &fonts,
                &geometries,
                metrics,
            )
            .unwrap();
        assert_eq!(
            warm.geometry.paths[0].transform,
            Transform2D::IDENTITY.into()
        );
        assert_eq!(warm.geometry_stats().geometry_cache_misses, 0);
        assert_eq!(warm.geometry_stats().path_vertices_repacked, 0);
        assert_eq!(warm.geometry_stats().path_indices_repacked, 0);
        assert_no_unused_mega_streams(&warm);
        let upload = renderer.upload_retained(&device, &queue, &warm, &mut text_state);
        let geometry = &warm.geometry;
        let expected = std::mem::size_of_val(geometry.circles)
            + std::mem::size_of_val(geometry.rectangles)
            + std::mem::size_of_val(geometry.lines)
            + std::mem::size_of_val(geometry.paths);
        assert_eq!(upload.geometry.bytes_uploaded, expected);
        assert_eq!(upload.geometry.buffer_reallocations, 0);

        // A same-slot structural replacement must retain its structural marker
        // through the family baseline. Reusing the old scratch/source identity
        // would draw object 99 instead of the replacement object 100.
        retained.objects[2].id = ObjectId::new(100);
        let frame = RetainedFamilyFrame {
            retained: &retained,
            family_animations: &states,
        };
        let replacement = preparer
            .prepare_family_animation_with_changes(
                &device,
                &queue,
                &frame,
                &plan,
                &FrameChanges::structural(vec![2], vec![2]),
                &texts,
                &fonts,
                &geometries,
                metrics,
            )
            .unwrap();
        assert_eq!(replacement.geometry_stats().full_rebuilds, 1);
        assert!(replacement
            .render_items
            .iter()
            .any(|item| item.object_id() == ObjectId::new(100)));
        assert!(!replacement
            .render_items
            .iter()
            .any(|item| item.object_id() == ObjectId::new(99)));
    }

    fn assert_no_unused_mega_streams(frame: &PreparedRetainedGpuFrame<'_>) {
        assert!(frame.geometry.mega_path_indices.is_empty());
        assert!(frame.geometry.mega_path_vertex_instances.is_empty());
        assert!(frame.geometry.mega_path_batches.is_empty());
        assert!(frame.geometry.mega_path_instance_dirty_ranges.is_empty());
        assert!(frame.geometry.mega_path_index_dirty_ranges.is_empty());
        assert_eq!(frame.geometry_stats().mega_path_count, 0);
        assert_eq!(
            frame.geometry_stats().mega_path_instance_vertices_repacked,
            0
        );
        assert!(frame.render_items.iter().all(|item| !matches!(
            item,
            RetainedRenderItem::Geometry {
                batch: OrderedRenderBatch {
                    primitive: RenderPrimitive::MegaPath { .. },
                    ..
                },
                ..
            }
        )));
    }
    #[test]
    fn compiled_morph_budget_keeps_inactive_phase_meshes_warm() {
        const PER_PHASE: usize = 300;
        let (_, mut retained, _) = fixture();
        let mut template = retained.objects[0].clone();
        template.style.fill = None;
        template.style.stroke = Some(noon_core::Color::WHITE);
        template.style.stroke_width = 0.02;
        retained.objects = (0..PER_PHASE)
            .map(|index| {
                let mut object = template.clone();
                object.id = ObjectId::new(index as u64);
                object
            })
            .collect();
        retained.presences = vec![true; PER_PHASE];
        retained.reveals = vec![1.0; PER_PHASE];
        retained.morphs = vec![0.3; PER_PHASE];
        retained.render_transforms = vec![Some(Transform2D::IDENTITY); PER_PHASE];
        let resources: Vec<_> = (0..2 * PER_PHASE)
            .map(|index| {
                let x = index as f32;
                std::sync::Arc::new(GeometryRef::path(
                    noon_core::VectorPath::new()
                        .move_to(noon_core::Vec2::new(x, 0.0))
                        .line_to(noon_core::Vec2::new(x + 0.5, 0.5)),
                ))
            })
            .collect();
        let texts = TextResourceArena::new();
        let fonts = FontResourceArena::new();
        let geometries = GeometryResourceArena::new();
        let metrics = TextDeviceMetrics::uniform(100.0).unwrap();
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedFramePreparer::new();
        preparer.set_scene_path_mesh_cache_budget(resources.len(), 0);
        for (visit, phase) in [0, 0, 1, 1, 0, 0, 1].into_iter().enumerate() {
            retained.render_geometries = resources[phase * PER_PHASE..(phase + 1) * PER_PHASE]
                .iter()
                .cloned()
                .map(Some)
                .collect();
            let prepared = preparer
                .prepare_with_changes(
                    &device,
                    &queue,
                    &retained,
                    &FrameChanges::all(),
                    &texts,
                    &fonts,
                    &geometries,
                    metrics,
                )
                .unwrap();
            assert_eq!(
                prepared.geometry_stats().geometry_cache_misses,
                if visit == 0 || visit == 2 {
                    PER_PHASE
                } else {
                    0
                }
            );
        }
        assert_eq!(preparer.geometry.cached_path_mesh_count(), 2 * PER_PHASE);

        // Installing a smaller resource set resets the allowance; visible paths
        // remain pinned while now-stale meshes return to the bounded cache policy.
        preparer.set_scene_path_mesh_cache_budget(0, 0);
        preparer
            .prepare_with_changes(
                &device,
                &queue,
                &retained,
                &FrameChanges::all(),
                &texts,
                &fonts,
                &geometries,
                metrics,
            )
            .unwrap();
        assert_eq!(preparer.geometry.cached_path_mesh_count(), PER_PHASE);
        assert_eq!(
            preparer.geometry.path_mesh_cache_limit(),
            crate::DEFAULT_PATH_MESH_CACHE_LIMIT
        );
    }
    #[test]
    fn preload_replacement_resets_prepared_state_and_failure_preserves_installation() {
        let (_, retained, _) = fixture();
        let texts = TextResourceArena::new();
        let fonts = FontResourceArena::new();
        let geometries = GeometryResourceArena::new();
        let metrics = TextDeviceMetrics::uniform(100.0).unwrap();
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedFramePreparer::new();
        let mut renderer = GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        preparer.set_scene_path_mesh_cache_budget(10, 400);
        let path = GeometryRef::path(
            noon_core::VectorPath::new()
                .move_to(noon_core::Vec2::ZERO)
                .line_to(noon_core::Vec2::ONE),
        );
        let request = crate::PathMeshPreload {
            geometry: &path,
            style: retained.objects[0].style,
            transform: Transform2D::IDENTITY,
        };
        preparer
            .preload_path_meshes(&device, &queue, &mut renderer, &[request])
            .unwrap();
        let generation = preparer
            .prepare_with_changes(
                &device,
                &queue,
                &retained,
                &FrameChanges::all(),
                &texts,
                &fonts,
                &geometries,
                metrics,
            )
            .unwrap()
            .text_generation;
        let prefix = preparer.geometry.path_vertices.clone();
        let cache_count = preparer.geometry.cached_path_mesh_count();
        let invalid = GeometryRef::path(noon_core::VectorPath::new().line_to(noon_core::Vec2::ONE));
        let bad = crate::PathMeshPreload {
            geometry: &invalid,
            ..request
        };
        assert!(preparer
            .preload_path_meshes(&device, &queue, &mut renderer, &[request, bad])
            .is_err());
        assert_eq!(preparer.geometry.path_vertices, prefix);
        assert_eq!(preparer.geometry.cached_path_mesh_count(), cache_count);
        assert_eq!(preparer.text_generation, generation);
        assert!(preparer.prepared_generation_ready);
        let reused = preparer
            .prepare_with_changes(
                &device,
                &queue,
                &retained,
                &FrameChanges::default(),
                &texts,
                &fonts,
                &geometries,
                metrics,
            )
            .unwrap();
        assert_eq!(reused.text_generation, generation);

        preparer
            .preload_path_meshes(&device, &queue, &mut renderer, &[request, request])
            .unwrap();
        assert_eq!(preparer.geometry.cached_path_mesh_count(), 1);
        assert_eq!(preparer.geometry.path_mesh_cache_limit(), 410);
        let rebuilt = preparer
            .prepare_with_changes(
                &device,
                &queue,
                &retained,
                &FrameChanges::default(),
                &texts,
                &fonts,
                &geometries,
                metrics,
            )
            .unwrap();
        assert_eq!(rebuilt.geometry_stats().full_rebuilds, 1);
        // This fixture is geometry-only. Reinstalling path residency rebuilds
        // geometry, but it must not manufacture a text upload generation.
        assert_eq!(rebuilt.text_generation, generation);
        assert!(rebuilt.geometry_only);
        assert_eq!(
            rebuilt.geometry_stats().instance_count,
            retained.objects.len()
        );
        let empty = preparer
            .preload_path_meshes(&device, &queue, &mut renderer, &[])
            .unwrap();
        assert_eq!(empty.upload.bytes_uploaded, 0);
        assert_eq!(preparer.geometry.cached_path_mesh_count(), 0);
        assert_eq!(preparer.geometry.resident_vertex_count, 0);
        assert_eq!(preparer.geometry.resident_index_count, 0);
    }
    #[test]
    fn oversized_preload_preserves_previous_cpu_and_gpu_installation() {
        let (_, mut retained, _) = fixture();
        retained.objects.truncate(1);
        retained.objects[0].style.fill = None;
        retained.objects[0].style.stroke = Some(noon_core::Color::WHITE);
        retained.objects[0].style.stroke_width = 0.02;
        retained.presences = vec![true];
        retained.reveals = vec![1.0];
        retained.morphs = vec![0.0];
        retained.render_transforms = vec![Some(Transform2D::IDENTITY)];
        let paths: Vec<_> = (0..1024)
            .map(|i| {
                let x = i as f32;
                GeometryRef::path(
                    noon_core::VectorPath::new()
                        .move_to(noon_core::Vec2::new(x, 0.0))
                        .line_to(noon_core::Vec2::new(x + 0.5, 0.5)),
                )
            })
            .collect();
        let requests: Vec<_> = paths
            .iter()
            .map(|geometry| crate::PathMeshPreload {
                geometry,
                style: retained.objects[0].style,
                transform: Transform2D::IDENTITY,
            })
            .collect();
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor {
            required_limits: wgpu::Limits {
                max_buffer_size: 65536,
                ..Default::default()
            },
            ..Default::default()
        });
        let mut renderer = GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        let mut preparer = RetainedFramePreparer::new();
        preparer
            .preload_path_meshes(&device, &queue, &mut renderer, &requests[..2])
            .unwrap();
        let texts = TextResourceArena::new();
        let fonts = FontResourceArena::new();
        let geometries = GeometryResourceArena::new();
        let metrics = TextDeviceMetrics::uniform(100.0).unwrap();
        retained.render_geometries = vec![Some(std::sync::Arc::new(paths[0].clone()))];
        let first = preparer
            .prepare_with_changes(
                &device,
                &queue,
                &retained,
                &FrameChanges::all(),
                &texts,
                &fonts,
                &geometries,
                metrics,
            )
            .unwrap();
        renderer.upload(&device, &queue, &first.geometry);
        let vertices = preparer.geometry.path_vertices.clone();
        let indices = preparer.geometry.path_indices.clone();
        let generation = preparer.text_generation;
        let error = preparer
            .preload_path_meshes(&device, &queue, &mut renderer, &requests)
            .expect_err("replacement must exceed the 64 KiB device buffer limit");
        assert!(
            matches!(error, crate::PathMeshPreloadError::Upload(
                crate::PathPreloadUploadError::BufferLimit { requested, limit, .. }
            ) if requested as u64 > limit && limit == 65536),
            "unexpected preload rejection: {error:?}"
        );
        assert_eq!(preparer.text_generation, generation);
        assert_eq!(preparer.geometry.cached_path_mesh_count(), 2);
        for path in &paths[..2] {
            retained.render_geometries = vec![Some(std::sync::Arc::new(path.clone()))];
            let prepared = preparer
                .prepare_with_changes(
                    &device,
                    &queue,
                    &retained,
                    &FrameChanges::all(),
                    &texts,
                    &fonts,
                    &geometries,
                    metrics,
                )
                .unwrap();
            assert_eq!(prepared.geometry_stats().geometry_cache_misses, 0);
            assert_eq!(prepared.geometry.path_vertices, vertices);
            assert_eq!(prepared.geometry.path_indices, indices);
            let mut writes = Vec::new();
            renderer.upload_with_trace(&device, &queue, &prepared.geometry, &mut writes);
            assert!(writes
                .iter()
                .all(|w| w.buffer != "path_vertex" && w.buffer != "path_index"));
            assert!(prepared.geometry_only);
            assert!(prepared.render_items.is_empty());
        }
    }
}
