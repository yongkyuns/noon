from pathlib import Path

ROOT = Path('.')


def replace_once(path: str, old: str, new: str) -> None:
    target = ROOT / path
    text = target.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f'{path}: expected one anchor, found {count}: {old[:80]!r}')
    target.write_text(text.replace(old, new, 1))


def append_before(path: str, anchor: str, addition: str) -> None:
    replace_once(path, anchor, addition + anchor)


# Renderer visibility: retain a reusable painter-ordered draw list containing only
# execution slots returned by the spatial index.
replace_once(
    'crates/noon-render-wgpu/src/render_order.rs',
    '''#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderedRenderBatch {
    pub primitive: RenderPrimitive,
    pub instance_range: Range<u32>,
}
''',
    '''#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderedRenderBatch {
    pub primitive: RenderPrimitive,
    pub instance_range: Range<u32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderVisibilityStats {
    pub requested_slots: usize,
    pub renderable_slots: usize,
    pub batch_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderVisibility {
    frame_indices: Vec<usize>,
    batches: Vec<OrderedRenderBatch>,
    stats: RenderVisibilityStats,
}

impl RenderVisibility {
    pub fn batches(&self) -> &[OrderedRenderBatch] {
        &self.batches
    }

    pub const fn stats(&self) -> RenderVisibilityStats {
        self.stats
    }
}
''',
)
replace_once(
    'crates/noon-render-wgpu/src/render_order.rs',
    '''    pub fn clear_render_order_keys(&mut self) {
        if !self.render_order_keys.is_empty() {
            self.render_order_keys.clear();
            self.initialized = false;
        }
    }

''',
    '''    pub fn clear_render_order_keys(&mut self) {
        if !self.render_order_keys.is_empty() {
            self.render_order_keys.clear();
            self.initialized = false;
        }
    }

    /// Rebuild a retained draw-selection list from already-indexed frame slots.
    ///
    /// Work is proportional to the requested/visible slots rather than total scene
    /// size. Packed instance/mesh storage remains unchanged; this only changes the
    /// draw indirection consumed by the GPU encoder.
    pub fn update_render_visibility(
        &self,
        visibility: &mut RenderVisibility,
        frame_indices: &[usize],
    ) {
        visibility.frame_indices.clear();
        visibility
            .frame_indices
            .extend(frame_indices.iter().copied().filter(|&index| index < self.slots.len()));
        if self.render_order_keys.len() == self.slots.len() {
            visibility
                .frame_indices
                .sort_by_key(|&index| self.render_order_keys[index]);
        } else {
            visibility.frame_indices.sort_unstable();
        }
        visibility.frame_indices.dedup();

        visibility.batches.clear();
        let mut renderable_slots = 0;
        for &index in &visibility.frame_indices {
            let slot = self.slots[index];
            if !matches!(slot, PreparedSlot::Absent | PreparedSlot::Unsupported(_)) {
                renderable_slots += 1;
            }
            push_slot_batches(&mut visibility.batches, slot);
        }
        visibility.stats = RenderVisibilityStats {
            requested_slots: frame_indices.len(),
            renderable_slots,
            batch_count: visibility.batches.len(),
        };
    }

''',
)
append_before(
    'crates/noon-render-wgpu/src/render_order.rs',
    '''    #[test]
    fn key_count_must_match_scene() {''',
    '''    #[test]
    fn visibility_selection_touches_only_requested_slots_in_large_scene() {
        let objects = (0..100_000usize)
            .map(|index| object(index as u64, GeometryRef::circle(0.4)))
            .collect::<Vec<_>>();
        let frame = frame(objects);
        let mut preparer = FramePreparer::new();
        let _ = preparer.prepare(&frame);
        let mut visibility = RenderVisibility::default();
        preparer.update_render_visibility(&mut visibility, &[10, 50_000, 99_999]);

        assert_eq!(visibility.stats().requested_slots, 3);
        assert_eq!(visibility.stats().renderable_slots, 3);
        assert_eq!(visibility.batches().len(), 3);
        assert_eq!(
            visibility
                .batches()
                .iter()
                .map(|batch| batch.instance_range.len())
                .sum::<usize>(),
            3
        );
        assert_eq!(visibility.batches()[0].instance_range, 10..11);
        assert_eq!(visibility.batches()[1].instance_range, 50_000..50_001);
        assert_eq!(visibility.batches()[2].instance_range, 99_999..100_000);
    }

''',
)

# A cheap second borrow of prepared arrays is useful after the first borrow has been
# uploaded and visibility has been rebuilt.
replace_once(
    'crates/noon-render-wgpu/src/lib.rs',
    '''    pub fn prepare<'a>(&'a mut self, frame: &FrameState) -> PreparedFrame<'a> {
        self.rebuild(frame)
    }

''',
    '''    pub fn prepare<'a>(&'a mut self, frame: &FrameState) -> PreparedFrame<'a> {
        self.rebuild(frame)
    }

    /// Borrow the already-prepared packed frame without doing preparation work.
    /// Useful for camera-only visibility changes after GPU upload has completed.
    pub fn prepared_view(&self, time: f64) -> PreparedFrame<'_> {
        self.prepared_frame(time, 0, 0, 0, 0, 0, 0, 0, 0)
    }

''',
)

# GPU encoder: accept a visibility draw list and derive MSAA only from selected paths.
replace_once(
    'crates/noon-render-wgpu/src/gpu.rs',
    'use noon_core::Vec2;\n',
    'use noon_core::{Rect, Vec2};\n',
)
replace_once(
    'crates/noon-render-wgpu/src/gpu.rs',
    '''    CircleInstance, LineInstance, PathBatch, PathInstance, PathVertex, PreparedFrame,
    RectangleInstance, RenderPrimitive,
''',
    '''    CircleInstance, LineInstance, OrderedRenderBatch, PathBatch, PathInstance, PathVertex,
    PreparedFrame, RectangleInstance, RenderPrimitive, RenderVisibility,
''',
)
replace_once(
    'crates/noon-render-wgpu/src/gpu.rs',
    '''    fn uniform(self, viewport_size: [u32; 2]) -> CameraUniform {
        CameraUniform {
''',
    '''    pub fn world_bounds(self) -> Rect {
        let half = self.world_size * 0.5;
        Rect::new(self.center - half, self.center + half)
    }

    /// Convert backing-store pixel coordinates to world coordinates.
    pub fn screen_to_world(self, viewport_size: [u32; 2], point: Vec2) -> Vec2 {
        let width = viewport_size[0].max(1) as f32;
        let height = viewport_size[1].max(1) as f32;
        Vec2::new(
            self.center.x + (point.x / width - 0.5) * self.world_size.x,
            self.center.y + (0.5 - point.y / height) * self.world_size.y,
        )
    }

    fn uniform(self, viewport_size: [u32; 2]) -> CameraUniform {
        CameraUniform {
''',
)
replace_once(
    'crates/noon-render-wgpu/src/gpu.rs',
    '''    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        prepared: &PreparedFrame<'_>,
        clear_color: wgpu::Color,
    ) -> DrawStats {
        self.encode_inner(encoder, view, prepared, clear_color, None)
    }

''',
    '''    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        prepared: &PreparedFrame<'_>,
        clear_color: wgpu::Color,
    ) -> DrawStats {
        self.encode_inner(
            encoder,
            view,
            prepared,
            prepared.render_batches,
            clear_color,
            None,
        )
    }

    pub fn encode_visible(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        prepared: &PreparedFrame<'_>,
        visibility: &RenderVisibility,
        clear_color: wgpu::Color,
    ) -> DrawStats {
        self.encode_inner(
            encoder,
            view,
            prepared,
            visibility.batches(),
            clear_color,
            None,
        )
    }

''',
)
replace_once(
    'crates/noon-render-wgpu/src/gpu.rs',
    '''    pub fn encode_profiled(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        prepared: &PreparedFrame<'_>,
        clear_color: wgpu::Color,
        query_set: &wgpu::QuerySet,
    ) -> DrawStats {
        self.encode_inner(encoder, view, prepared, clear_color, Some(query_set))
    }

''',
    '''    pub fn encode_profiled(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        prepared: &PreparedFrame<'_>,
        clear_color: wgpu::Color,
        query_set: &wgpu::QuerySet,
    ) -> DrawStats {
        self.encode_inner(
            encoder,
            view,
            prepared,
            prepared.render_batches,
            clear_color,
            Some(query_set),
        )
    }

    pub fn encode_profiled_visible(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        prepared: &PreparedFrame<'_>,
        visibility: &RenderVisibility,
        clear_color: wgpu::Color,
        query_set: &wgpu::QuerySet,
    ) -> DrawStats {
        self.encode_inner(
            encoder,
            view,
            prepared,
            visibility.batches(),
            clear_color,
            Some(query_set),
        )
    }

''',
)
replace_once(
    'crates/noon-render-wgpu/src/gpu.rs',
    '''    fn encode_inner(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        prepared: &PreparedFrame<'_>,
        clear_color: wgpu::Color,
        query_set: Option<&wgpu::QuerySet>,
    ) -> DrawStats {
        let sample_count = ordered_render_sample_count(prepared.path_batches);
''',
    '''    fn encode_inner(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        prepared: &PreparedFrame<'_>,
        render_batches: &[OrderedRenderBatch],
        clear_color: wgpu::Color,
        query_set: Option<&wgpu::QuerySet>,
    ) -> DrawStats {
        let sample_count = ordered_render_sample_count(render_batches);
''',
)
replace_once(
    'crates/noon-render-wgpu/src/gpu.rs',
    '            return self.draw_ordered(&mut pass, prepared, true);\n',
    '            return self.draw_ordered(&mut pass, prepared, render_batches, true);\n',
)
replace_once(
    'crates/noon-render-wgpu/src/gpu.rs',
    '        self.draw_ordered(&mut pass, prepared, false)\n',
    '        self.draw_ordered(&mut pass, prepared, render_batches, false)\n',
)
replace_once(
    'crates/noon-render-wgpu/src/gpu.rs',
    '''    fn draw_ordered<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        prepared: &PreparedFrame<'_>,
        single_sample_analytics: bool,
    ) -> DrawStats {
''',
    '''    fn draw_ordered<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        prepared: &PreparedFrame<'_>,
        render_batches: &[OrderedRenderBatch],
        single_sample_analytics: bool,
    ) -> DrawStats {
''',
)
replace_once(
    'crates/noon-render-wgpu/src/gpu.rs',
    '        for batch in prepared.render_batches {\n',
    '        for batch in render_batches {\n',
)
replace_once(
    'crates/noon-render-wgpu/src/gpu.rs',
    '''fn ordered_render_sample_count(path_batches: &[PathBatch]) -> u32 {
    if path_batches
        .iter()
        .any(|batch| !batch.index_range.is_empty())
    {
        PATH_SAMPLE_COUNT
    } else {
        1
    }
}
''',
    '''fn ordered_render_sample_count(render_batches: &[OrderedRenderBatch]) -> u32 {
    if render_batches.iter().any(|batch| {
        matches!(
            batch.primitive,
            RenderPrimitive::Path { .. } | RenderPrimitive::MegaPath { .. }
        )
    }) {
        PATH_SAMPLE_COUNT
    } else {
        1
    }
}
''',
)

# Transport mirror helpers allow the render worker to maintain the exact same retained
# index without asking the engine worker or scanning semantic objects.
replace_once(
    'crates/noon-web/src/execution_transport.rs',
    '''use noon_runtime::{
    ExecutionSlotError, ExecutionSlotId, FrameChanges, FrameObjectState, FrameState,
};
''',
    '''use noon_runtime::{
    ExecutionSlotError, ExecutionSlotId, ExecutionSpatialIndex, FrameChanges, FrameObjectState,
    FrameState, SpatialIndexUpdateStats,
};
''',
)
replace_once(
    'crates/noon-web/src/execution_transport.rs',
    '''impl From<ExecutionSlotId> for TransportSlotId {
    fn from(value: ExecutionSlotId) -> Self {
        Self {
            slot: value.slot(),
            generation: value.generation(),
        }
    }
}
''',
    '''impl From<ExecutionSlotId> for TransportSlotId {
    fn from(value: ExecutionSlotId) -> Self {
        Self {
            slot: value.slot(),
            generation: value.generation(),
        }
    }
}

impl From<TransportSlotId> for ExecutionSlotId {
    fn from(value: TransportSlotId) -> Self {
        ExecutionSlotId::new(value.slot, value.generation)
    }
}
''',
)
replace_once(
    'crates/noon-web/src/execution_transport.rs',
    '''    pub fn live_object_count(&self) -> usize {
        self.slot_indices.len()
    }

''',
    '''    pub fn live_object_count(&self) -> usize {
        self.slot_indices.len()
    }

    pub fn execution_slot_for_frame_index(&self, frame_index: usize) -> Option<ExecutionSlotId> {
        let slot = *self.slots.get(frame_index)?;
        self.slot_indices
            .contains_key(&slot)
            .then_some(ExecutionSlotId::from(slot))
    }

    pub fn frame_index_for_execution_slot(&self, slot: ExecutionSlotId) -> Option<usize> {
        self.slot_indices.get(&TransportSlotId::from(slot)).copied()
    }

    fn retained_execution_slot_for_frame_index(
        &self,
        frame_index: usize,
    ) -> Option<ExecutionSlotId> {
        self.slots
            .get(frame_index)
            .copied()
            .map(ExecutionSlotId::from)
    }

    pub fn live_frame_slots(&self) -> Vec<(ExecutionSlotId, usize)> {
        let mut slots = self
            .slot_indices
            .iter()
            .map(|(&slot, &index)| (ExecutionSlotId::from(slot), index))
            .collect::<Vec<_>>();
        slots.sort_unstable_by_key(|(_, index)| *index);
        slots
    }

    pub fn sync_spatial_index(
        &self,
        index: &mut ExecutionSpatialIndex,
        changes: &FrameChanges,
    ) -> SpatialIndexUpdateStats {
        let Some(frame) = self.frame() else {
            return SpatialIndexUpdateStats::default();
        };
        if changes.is_all() {
            return index.rebuild(frame, self.live_frame_slots());
        }

        let mut stats = SpatialIndexUpdateStats::default();
        for &frame_index in changes.removed_indices() {
            if let Some(slot) = self.retained_execution_slot_for_frame_index(frame_index) {
                stats.merge_from(index.remove_slot(slot));
            }
        }
        for &frame_index in changes.object_indices() {
            if changes.removed_indices().binary_search(&frame_index).is_ok() {
                continue;
            }
            let Some(slot) = self.execution_slot_for_frame_index(frame_index) else {
                continue;
            };
            stats.merge_from(index.upsert_frame_slot(
                frame,
                slot,
                frame_index,
                frame_index as u64,
            ));
        }
        stats
    }

''',
)

# Render-worker side: retained culling index, camera-only redraws, and metrics.
replace_once(
    'crates/noon-web/src/execution_canvas.rs',
    '''    use noon_render_wgpu::{Camera2D, FramePreparer, GpuRenderer};
    use noon_runtime::FrameChanges;
''',
    '''    use noon_render_wgpu::{Camera2D, FramePreparer, GpuRenderer, RenderVisibility};
    use noon_runtime::{ExecutionSpatialIndex, FrameChanges};
''',
)
replace_once(
    'crates/noon-web/src/execution_canvas.rs',
    '''        pending_changes: FrameChanges,
        preparer: FramePreparer,
        renderer: GpuRenderer,
''',
    '''        pending_changes: FrameChanges,
        spatial_index: ExecutionSpatialIndex,
        preparer: FramePreparer,
        visibility: RenderVisibility,
        renderer: GpuRenderer,
        view_dirty: bool,
''',
)
replace_once(
    'crates/noon-web/src/execution_canvas.rs',
    '''        last_geometry_cache_misses: usize,
''',
    '''        last_geometry_cache_misses: usize,
        last_visible_objects: usize,
        last_spatial_candidates_tested: usize,
        last_spatial_cells_visited: usize,
        last_spatial_full_scan_fallbacks: usize,
''',
)
replace_once(
    'crates/noon-web/src/execution_canvas.rs',
    '''            let renderer = GpuRenderer::new(&device, config.format);

            let mut result = Self {
''',
    '''            let renderer = GpuRenderer::new(&device, config.format);
            let mut spatial_index = ExecutionSpatialIndex::default();
            mirror.sync_spatial_index(&mut spatial_index, &pending_changes);

            let mut result = Self {
''',
)
replace_once(
    'crates/noon-web/src/execution_canvas.rs',
    '''                pending_changes,
                preparer: FramePreparer::new(),
                renderer,
''',
    '''                pending_changes,
                spatial_index,
                preparer: FramePreparer::new(),
                visibility: RenderVisibility::default(),
                renderer,
                view_dirty: true,
''',
)
replace_once(
    'crates/noon-web/src/execution_canvas.rs',
    '''                last_geometry_cache_misses: 0,
''',
    '''                last_geometry_cache_misses: 0,
                last_visible_objects: 0,
                last_spatial_candidates_tested: 0,
                last_spatial_cells_visited: 0,
                last_spatial_full_scan_fallbacks: 0,
''',
)
replace_once(
    'crates/noon-web/src/execution_canvas.rs',
    '''                TransportApplyOutcome::Applied => {
                    self.pending_changes = changes;
                    Ok(true)
                }
''',
    '''                TransportApplyOutcome::Applied => {
                    self.mirror
                        .sync_spatial_index(&mut self.spatial_index, &changes);
                    self.pending_changes = changes;
                    Ok(true)
                }
''',
)
replace_once(
    'crates/noon-web/src/execution_canvas.rs',
    '''        pub fn render(&mut self) -> Result<bool, JsValue> {
            if !self.drawable || self.pending_changes.is_empty() {
                return Ok(false);
            }
''',
    '''        pub fn render(&mut self) -> Result<bool, JsValue> {
            if !self.drawable || (self.pending_changes.is_empty() && !self.view_dirty) {
                return Ok(false);
            }
''',
)
replace_once(
    'crates/noon-web/src/execution_canvas.rs',
    '''            let prepared = self.preparer.prepare_incremental(frame, &changes);
            self.last_geometry_cache_misses = prepared.stats.geometry_cache_misses;
            let upload = self.renderer.upload(&self.device, &self.queue, &prepared);
            self.last_bytes_uploaded = upload.bytes_uploaded;

            let view = surface_texture
''',
    '''            let frame_time = frame.time;
            {
                let prepared = self.preparer.prepare_incremental(frame, &changes);
                self.last_geometry_cache_misses = prepared.stats.geometry_cache_misses;
                let upload = self.renderer.upload(&self.device, &self.queue, &prepared);
                self.last_bytes_uploaded = upload.bytes_uploaded;
            }

            let query = self.spatial_index.query_rect(self.renderer.camera().world_bounds());
            let query_stats = query.stats();
            let visible_indices = query
                .slots()
                .iter()
                .filter_map(|&slot| self.mirror.frame_index_for_execution_slot(slot))
                .collect::<Vec<_>>();
            self.preparer
                .update_render_visibility(&mut self.visibility, &visible_indices);
            self.last_visible_objects = self.visibility.stats().renderable_slots;
            self.last_spatial_candidates_tested = query_stats.candidates_tested;
            self.last_spatial_cells_visited = query_stats.cells_visited;
            self.last_spatial_full_scan_fallbacks = query_stats.full_scan_fallbacks;
            let prepared = self.preparer.prepared_view(frame_time);

            let view = surface_texture
''',
)
replace_once(
    'crates/noon-web/src/execution_canvas.rs',
    '''            let draw = self
                .renderer
                .encode(&mut encoder, &view, &prepared, self.clear_color);
''',
    '''            let draw = self.renderer.encode_visible(
                &mut encoder,
                &view,
                &prepared,
                &self.visibility,
                self.clear_color,
            );
''',
)
replace_once(
    'crates/noon-web/src/execution_canvas.rs',
    '''            self.last_draw_calls = draw.draw_calls;
            self.last_instances_drawn = draw.instances_drawn;
''',
    '''            self.last_draw_calls = draw.draw_calls;
            self.last_instances_drawn = draw.instances_drawn;
            self.view_dirty = false;
''',
)
replace_once(
    'crates/noon-web/src/execution_canvas.rs',
    '''                self.update_camera()?;
            }
            Ok(())
''',
    '''                self.update_camera()?;
                self.view_dirty = true;
            }
            Ok(())
''',
)
replace_once(
    'crates/noon-web/src/execution_canvas.rs',
    '''            self.camera_center = center;
            self.camera_height = world_height;
            self.update_camera()
''',
    '''            self.camera_center = center;
            self.camera_height = world_height;
            self.update_camera()?;
            self.view_dirty = true;
            Ok(())
''',
)
replace_once(
    'crates/noon-web/src/execution_canvas.rs',
    '''        #[wasm_bindgen(js_name = lastGeometryCacheMisses)]
        pub fn last_geometry_cache_misses(&self) -> usize {
            self.last_geometry_cache_misses
        }
''',
    '''        #[wasm_bindgen(js_name = lastGeometryCacheMisses)]
        pub fn last_geometry_cache_misses(&self) -> usize {
            self.last_geometry_cache_misses
        }

        #[wasm_bindgen(js_name = lastVisibleObjects)]
        pub fn last_visible_objects(&self) -> usize {
            self.last_visible_objects
        }

        #[wasm_bindgen(js_name = lastSpatialCandidatesTested)]
        pub fn last_spatial_candidates_tested(&self) -> usize {
            self.last_spatial_candidates_tested
        }

        #[wasm_bindgen(js_name = lastSpatialCellsVisited)]
        pub fn last_spatial_cells_visited(&self) -> usize {
            self.last_spatial_cells_visited
        }

        #[wasm_bindgen(js_name = lastSpatialFullScanFallbacks)]
        pub fn last_spatial_full_scan_fallbacks(&self) -> usize {
            self.last_spatial_full_scan_fallbacks
        }
''',
)

# Direct browser canvas: same retained query drives draw selection, plus native hit APIs.
replace_once(
    'crates/noon-web/src/legacy.rs',
    '''    use noon_render_wgpu::{Camera2D, FramePreparer, GpuRenderer};
''',
    '''    use noon_render_wgpu::{Camera2D, FramePreparer, GpuRenderer, RenderVisibility};
''',
)
replace_once(
    'crates/noon-web/src/legacy.rs',
    '''        pub fn scene_json(&self) -> Result<String, JsValue> {
            self.inner.scene_json().map_err(js_error)
        }
    }

    #[wasm_bindgen(js_name = NoonCanvasPlayer)]
''',
    '''        pub fn scene_json(&self) -> Result<String, JsValue> {
            self.inner.scene_json().map_err(js_error)
        }

        #[wasm_bindgen(js_name = hitTestWorldJson)]
        pub fn hit_test_world_json(&self, x: f32, y: f32) -> Result<String, JsValue> {
            spatial_query_json(&self.inner, self.inner.hit_test(Vec2::new(x, y)))
        }
    }

    #[wasm_bindgen(js_name = NoonCanvasPlayer)]
''',
)
replace_once(
    'crates/noon-web/src/legacy.rs',
    '''        preparer: FramePreparer,
        renderer: GpuRenderer,
''',
    '''        preparer: FramePreparer,
        visibility: RenderVisibility,
        renderer: GpuRenderer,
''',
)
# This anchor occurs once in the WasmCanvasPlayer field block after prior replacement.
replace_once(
    'crates/noon-web/src/legacy.rs',
    '''        last_geometry_cache_misses: usize,
        last_cpu_frame_ms: f64,
''',
    '''        last_geometry_cache_misses: usize,
        last_visible_objects: usize,
        last_spatial_candidates_tested: usize,
        last_spatial_cells_visited: usize,
        last_spatial_full_scan_fallbacks: usize,
        last_cpu_frame_ms: f64,
''',
)
replace_once(
    'crates/noon-web/src/legacy.rs',
    '''                preparer: FramePreparer::new(),
                renderer,
''',
    '''                preparer: FramePreparer::new(),
                visibility: RenderVisibility::default(),
                renderer,
''',
)
replace_once(
    'crates/noon-web/src/legacy.rs',
    '''                last_geometry_cache_misses: 0,
                last_cpu_frame_ms: f64::NAN,
''',
    '''                last_geometry_cache_misses: 0,
                last_visible_objects: 0,
                last_spatial_candidates_tested: 0,
                last_spatial_cells_visited: 0,
                last_spatial_full_scan_fallbacks: 0,
                last_cpu_frame_ms: f64::NAN,
''',
)
replace_once(
    'crates/noon-web/src/legacy.rs',
    '''        #[wasm_bindgen(js_name = lastGeometryCacheMisses)]
        pub fn last_geometry_cache_misses(&self) -> usize {
            self.last_geometry_cache_misses
        }

        #[wasm_bindgen(js_name = lastCpuFrameMs)]
''',
    '''        #[wasm_bindgen(js_name = lastGeometryCacheMisses)]
        pub fn last_geometry_cache_misses(&self) -> usize {
            self.last_geometry_cache_misses
        }

        #[wasm_bindgen(js_name = lastVisibleObjects)]
        pub fn last_visible_objects(&self) -> usize {
            self.last_visible_objects
        }

        #[wasm_bindgen(js_name = lastSpatialCandidatesTested)]
        pub fn last_spatial_candidates_tested(&self) -> usize {
            self.last_spatial_candidates_tested
        }

        #[wasm_bindgen(js_name = lastSpatialCellsVisited)]
        pub fn last_spatial_cells_visited(&self) -> usize {
            self.last_spatial_cells_visited
        }

        #[wasm_bindgen(js_name = lastSpatialFullScanFallbacks)]
        pub fn last_spatial_full_scan_fallbacks(&self) -> usize {
            self.last_spatial_full_scan_fallbacks
        }

        #[wasm_bindgen(js_name = hitTestWorldJson)]
        pub fn hit_test_world_json(&self, x: f32, y: f32) -> Result<String, JsValue> {
            spatial_query_json(&self.player, self.player.hit_test(Vec2::new(x, y)))
        }

        #[wasm_bindgen(js_name = hitTestCanvasJson)]
        pub fn hit_test_canvas_json(&self, x: f32, y: f32) -> Result<String, JsValue> {
            let point = self
                .renderer
                .camera()
                .screen_to_world(self.renderer.viewport_size(), Vec2::new(x, y));
            spatial_query_json(&self.player, self.player.hit_test(point))
        }

        #[wasm_bindgen(js_name = lastCpuFrameMs)]
''',
)
replace_once(
    'crates/noon-web/src/legacy.rs',
    '''            let prepare_started_ms = performance_now_ms();
            let changes = self.player.take_frame_changes();
            let prepared = self
                .preparer
                .prepare_incremental(self.player.frame(), &changes);
            self.last_frame_prepare_ms = elapsed_ms(prepare_started_ms);
            self.last_geometry_cache_misses = prepared.stats.geometry_cache_misses;
            let upload_started_ms = performance_now_ms();
            let upload = self.renderer.upload(&self.device, &self.queue, &prepared);
            self.last_upload_ms = elapsed_ms(upload_started_ms);
            self.last_bytes_uploaded = upload.bytes_uploaded;

            let (surface_texture, reconfigure_after_present) =
''',
    '''            let query = self.player.query_viewport(self.renderer.camera().world_bounds());
            let query_stats = query.stats();
            let visible_indices = query
                .slots()
                .iter()
                .filter_map(|&slot| self.player.frame_index_for_execution_slot(slot))
                .collect::<Vec<_>>();
            self.last_spatial_candidates_tested = query_stats.candidates_tested;
            self.last_spatial_cells_visited = query_stats.cells_visited;
            self.last_spatial_full_scan_fallbacks = query_stats.full_scan_fallbacks;

            let prepare_started_ms = performance_now_ms();
            let changes = self.player.take_frame_changes();
            let frame_time = self.player.frame().time;
            {
                let prepared = self
                    .preparer
                    .prepare_incremental(self.player.frame(), &changes);
                self.last_geometry_cache_misses = prepared.stats.geometry_cache_misses;
                let upload_started_ms = performance_now_ms();
                let upload = self.renderer.upload(&self.device, &self.queue, &prepared);
                self.last_upload_ms = elapsed_ms(upload_started_ms);
                self.last_bytes_uploaded = upload.bytes_uploaded;
            }
            self.preparer
                .update_render_visibility(&mut self.visibility, &visible_indices);
            self.last_visible_objects = self.visibility.stats().renderable_slots;
            let prepared = self.preparer.prepared_view(frame_time);
            self.last_frame_prepare_ms = elapsed_ms(prepare_started_ms);

            let (surface_texture, reconfigure_after_present) =
''',
)
replace_once(
    'crates/noon-web/src/legacy.rs',
    '''                self.renderer.encode_profiled(
                    &mut encoder,
                    &view,
                    &prepared,
                    self.clear_color,
                    query_set,
                )
            } else {
                self.renderer
                    .encode(&mut encoder, &view, &prepared, self.clear_color)
            };
''',
    '''                self.renderer.encode_profiled_visible(
                    &mut encoder,
                    &view,
                    &prepared,
                    &self.visibility,
                    self.clear_color,
                    query_set,
                )
            } else {
                self.renderer.encode_visible(
                    &mut encoder,
                    &view,
                    &prepared,
                    &self.visibility,
                    self.clear_color,
                )
            };
''',
)
# Helper inside wasm module, before performance timing helpers.
append_before(
    'crates/noon-web/src/legacy.rs',
    '''    fn performance_now_ms() -> f64 {''',
    '''    fn spatial_query_json(
        player: &ScenePlayer,
        result: noon_runtime::SpatialQueryResult,
    ) -> Result<String, JsValue> {
        let objects = result
            .slots()
            .iter()
            .filter_map(|&slot| player.frame_index_for_execution_slot(slot))
            .map(|index| player.frame().objects[index].id.get().to_string())
            .collect::<Vec<_>>();
        let stats = result.stats();
        serde_json::to_string(&serde_json::json!({
            "objects": objects,
            "cellsVisited": stats.cells_visited,
            "candidatesTested": stats.candidates_tested,
            "fullScanFallbacks": stats.full_scan_fallbacks,
        }))
        .map_err(js_error)
    }

''',
)

# Camera-only render-worker viewport changes should produce a present even without a
# new execution delta.
replace_once(
    'web/execution-render-worker.js',
    '''      case "resize":
        width = normalizedDimension(message.width);
        height = normalizedDimension(message.height);
        renderer?.resize(width, height);
        return;
''',
    '''      case "resize":
        width = normalizedDimension(message.width);
        height = normalizedDimension(message.height);
        renderer?.resize(width, height);
        if (renderer !== null) {
          needsPresent = true;
          tryPresent();
        }
        return;
''',
)
replace_once(
    'web/execution-render-worker.js',
    '''    geometryCacheMisses: renderer.lastGeometryCacheMisses(),
    presentedFrames,
''',
    '''    geometryCacheMisses: renderer.lastGeometryCacheMisses(),
    visibleObjects: renderer.lastVisibleObjects(),
    spatialCandidatesTested: renderer.lastSpatialCandidatesTested(),
    spatialCellsVisited: renderer.lastSpatialCellsVisited(),
    spatialFullScanFallbacks: renderer.lastSpatialFullScanFallbacks(),
    presentedFrames,
''',
)

# Add the missing brute-force correctness corpus for #66 acceptance.
append_before(
    'crates/noon-runtime/src/spatial_index.rs',
    '''    #[test]
    fn viewport_query_visits_only_intersecting_grid_cells() {''',
    '''    #[test]
    fn indexed_queries_match_brute_force_reference_corpus() {
        let mut index = ExecutionSpatialIndex::default();
        let mut entries = Vec::new();
        for object_index in 0..2_000usize {
            let x = ((object_index * 37) % 211) as f32 * 0.73 - 70.0;
            let y = ((object_index * 83) % 197) as f32 * 0.61 - 55.0;
            let width = 0.2 + (object_index % 9) as f32 * 0.17;
            let height = 0.2 + (object_index % 7) as f32 * 0.19;
            let bounds = if object_index % 173 == 0 {
                Rect::new(Vec2::new(x - 20.0, y - 20.0), Vec2::new(x + 20.0, y + 20.0))
            } else {
                Rect::new(Vec2::new(x, y), Vec2::new(x + width, y + height))
            };
            let slot = ExecutionSlotId::new(object_index as u32, 0);
            index.upsert_bounds(
                slot,
                ObjectId::new(object_index as u64),
                bounds,
                object_index as u64,
            );
            entries.push((slot, bounds, object_index as u64));
        }

        for query_index in 0..120usize {
            let x = ((query_index * 29) % 113) as f32 * 1.7 - 70.0;
            let y = ((query_index * 47) % 109) as f32 * 1.3 - 55.0;
            let query = Rect::new(Vec2::new(x, y), Vec2::new(x + 8.5, y + 6.5));
            let mut expected = entries
                .iter()
                .filter(|(_, bounds, _)| rects_intersect(*bounds, query))
                .map(|(slot, _, painter)| (*slot, *painter))
                .collect::<Vec<_>>();
            expected.sort_by_key(|(_, painter)| *painter);
            let expected = expected.into_iter().map(|(slot, _)| slot).collect::<Vec<_>>();
            assert_eq!(index.query_rect(query).slots(), expected);

            let point = Vec2::new(x + 1.25, y + 2.75);
            let point_bounds = Rect::new(point, point);
            let mut expected_hits = entries
                .iter()
                .filter(|(_, bounds, _)| rects_intersect(*bounds, point_bounds))
                .map(|(slot, _, painter)| (*slot, *painter))
                .collect::<Vec<_>>();
            expected_hits.sort_by_key(|(_, painter)| std::cmp::Reverse(*painter));
            let expected_hits = expected_hits
                .into_iter()
                .map(|(slot, _)| slot)
                .collect::<Vec<_>>();
            assert_eq!(index.hit_test(point).slots(), expected_hits);
        }
    }

''',
)

# Transport-level retained-index sync regression.
append_before(
    'crates/noon-web/src/execution_transport.rs',
    '''    #[test]
    fn structural_incremental_delta_preserves_surviving_frame_rows() {''',
    '''    #[test]
    fn render_mirror_spatial_index_refits_only_delta_slots() {
        let mut scene = noon_core::SceneDefinition::new();
        let left = scene.add(GeometryRef::circle(0.5));
        let right = scene.add(GeometryRef::circle(0.5));
        scene.object_mut(left).unwrap().transform.translation = noon_core::Vec2::new(-4.0, 0.0);
        scene.object_mut(right).unwrap().transform.translation = noon_core::Vec2::new(4.0, 0.0);
        let scene_json = noon_ir::encode_scene(&scene).unwrap();
        let mut engine = EngineScenePlayer::new(&scene_json, 4.0, 9).unwrap();
        let initial: ExecutionDeltaEnvelope =
            serde_json::from_str(&engine.initial_delta_json().unwrap()).unwrap();
        let mut mirror = ExecutionFrameMirror::default();
        let (_, initial_changes) = mirror.apply(initial).unwrap();
        let mut index = ExecutionSpatialIndex::default();
        let initial_stats = mirror.sync_spatial_index(&mut index, &initial_changes);
        assert_eq!(initial_stats.full_rebuilds, 1);
        assert_eq!(index.len(), 2);

        let slot = index.hit_test(noon_core::Vec2::new(4.0, 0.0)).slots()[0];
        let frame_index = mirror.frame_index_for_execution_slot(slot).unwrap();
        let object = mirror.frame().unwrap().objects[frame_index].id;
        let patch = noon_ir::PatchBatch {
            sequence: 0,
            patches: vec![noon_core::ScenePatch::SetTransform {
                object,
                transform: noon_core::Transform2D {
                    translation: noon_core::Vec2::new(7.0, 0.0),
                    ..noon_core::Transform2D::IDENTITY
                },
            }],
        };
        let delta = engine
            .apply_patch_batch_delta_json(&noon_ir::encode_patch_batch(&patch).unwrap())
            .unwrap()
            .unwrap();
        let delta: ExecutionDeltaEnvelope = serde_json::from_str(&delta).unwrap();
        let (_, changes) = mirror.apply(delta).unwrap();
        let stats = mirror.sync_spatial_index(&mut index, &changes);
        assert_eq!(stats.full_rebuilds, 0);
        assert_eq!(stats.leaves_upserted, 1);
        assert_eq!(index.hit_test(noon_core::Vec2::new(7.0, 0.0)).slots(), &[slot]);
        assert!(index.hit_test(noon_core::Vec2::new(4.0, 0.0)).slots().is_empty());
    }

''',
)

# Documentation closes the remaining #66 architecture contract.
replace_once(
    'docs/spatial-index.md',
    '''## Next #66 slice

Renderer viewport culling should consume `ScenePlayer::query_viewport` / the same
execution index and filter ordered draw submission without repacking unrelated GPU
instance or path geometry. Native hover/selection can consume `hit_test` directly.
''',
    '''## Renderer and browser integration

The renderer consumes retained viewport query results as stable execution slots and
builds a reusable `RenderVisibility` draw indirection in `O(visible)` work. Camera
changes do not repack instance buffers or path geometry, and offscreen vector paths
no longer force multisampling or draw submission. The execution render worker keeps
its own mirror-side `ExecutionSpatialIndex`, incrementally synchronized from transport
`FrameChanges`, so culling never round-trips through the Python/engine worker.

`ScenePlayer` remains the authoritative hit-test API. WASM `ScenePlayer` exposes
world-coordinate hit results, while direct `NoonCanvasPlayer` also converts backing-
store pixel coordinates through `Camera2D::screen_to_world`. Results keep topmost-first
painter ordering and include candidate/cell/fallback counters for editor observability.

Correctness now includes a deterministic indexed-vs-brute-force point/viewport corpus.
Combined with the 100,000-object locality regressions, this completes #66's retained
hit-testing and viewport-culling contract.
''',
)

print('applied I1 renderer culling implementation')
