from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    if old not in text:
        raise SystemExit(f"missing anchor: {label}")
    return text.replace(old, new, 1)


lib_path = Path("crates/noon-render-wgpu/src/lib.rs")
lib = lib_path.read_text()

lib = replace_once(
    lib,
    """    pub path_dirty_ranges: &'a [Range<usize>],\n    pub mega_path_instance_dirty_ranges: &'a [Range<usize>],\n""",
    """    pub path_dirty_ranges: &'a [Range<usize>],\n    /// Dirty packed path-geometry vertex ranges. Incremental path replacement\n    /// writes only these ranges instead of rewriting the full mesh arena.\n    pub path_vertex_dirty_ranges: &'a [Range<usize>],\n    /// Dirty packed path-geometry index ranges.\n    pub path_index_dirty_ranges: &'a [Range<usize>],\n    pub mega_path_instance_dirty_ranges: &'a [Range<usize>],\n""",
    "prepared geometry dirty ranges",
)

lib = replace_once(
    lib,
    """    path_dirty_ranges: Vec<Range<usize>>,\n    mega_path_instance_dirty_ranges: Vec<Range<usize>>,\n""",
    """    path_dirty_ranges: Vec<Range<usize>>,\n    path_vertex_dirty_ranges: Vec<Range<usize>>,\n    path_index_dirty_ranges: Vec<Range<usize>>,\n    // Incremental geometry edits never compact the arena: released chunks are\n    // first-fit reused, while full rebuilds are the explicit compaction barrier.\n    path_vertex_free_ranges: Vec<Range<u32>>,\n    path_index_free_ranges: Vec<Range<u32>>,\n    mega_path_instance_dirty_ranges: Vec<Range<usize>>,\n""",
    "preparer path arena fields",
)

old_gate = """        if !self.initialized\n            || changes.is_all()\n            || self.slots.len() != frame.objects.len()\n            || !changes\n                .object_indices()\n                .iter()\n                .all(|&index| self.slot_matches(frame, index))\n        {\n            return self.rebuild(frame);\n        }\n\n        self.clear_dirty_ranges();\n        let mut instances_repacked = 0;\n"""
new_gate = """        if !self.initialized || changes.is_all() || self.slots.len() != frame.objects.len() {\n            return self.rebuild(frame);\n        }\n\n        let replacement_indices = changes\n            .object_indices()\n            .iter()\n            .copied()\n            .filter(|&index| !self.slot_matches(frame, index))\n            .collect::<Vec<_>>();\n        if !replacement_indices\n            .iter()\n            .all(|&index| self.can_replace_unique_path_geometry(frame, index))\n        {\n            return self.rebuild(frame);\n        }\n\n        self.clear_dirty_ranges();\n        let mut geometry_cache_misses = 0;\n        let mut path_vertices_repacked = 0;\n        let mut path_indices_repacked = 0;\n        for object_index in replacement_indices {\n            let replacement = self\n                .replace_unique_path_geometry(frame, object_index)\n                .expect("preflighted unique path replacement must tessellate");\n            geometry_cache_misses += usize::from(replacement.cache_miss);\n            path_vertices_repacked += replacement.vertices_repacked;\n            path_indices_repacked += replacement.indices_repacked;\n        }\n        if path_vertices_repacked > 0 || path_indices_repacked > 0 {\n            self.rebuild_ordered_render_batches();\n            self.rebuild_mega_path_draws();\n        }\n\n        let mut instances_repacked = 0;\n"""
lib = replace_once(lib, old_gate, new_gate, "incremental replacement gate")

lib = replace_once(
    lib,
    """        self.prepared_frame(frame.time, 0, instances_repacked, 0, 0, 0)\n    }\n\n    fn rebuild<'a>(&'a mut self, frame: &FrameState) -> PreparedFrame<'a> {\n""",
    """        self.prepared_frame(\n            frame.time,\n            0,\n            instances_repacked,\n            geometry_cache_misses,\n            path_vertices_repacked,\n            path_indices_repacked,\n        )\n    }\n\n    fn can_replace_unique_path_geometry(&self, frame: &FrameState, object_index: usize) -> bool {\n        let Some(object) = frame.objects.get(object_index) else {\n            return false;\n        };\n        if !frame.is_present(object_index) {\n            return false;\n        }\n        let Some(PreparedSlot::Path {\n            index,\n            batch,\n            analytic_reveal: None,\n            reveal_head,\n        }) = self.slots.get(object_index)\n        else {\n            return false;\n        };\n        let Some(path_batch) = self.path_batches.get(*batch) else {\n            return false;\n        };\n        if path_batch.instance_range.end != path_batch.instance_range.start + 1\n            || self.path_ids.get(*index) != Some(&object.id)\n            || !matches!(frame.render_geometry(object_index), GeometryRef::VectorPath(_))\n        {\n            return false;\n        }\n        reveal_head.is_some()\n            || !should_create_path_reveal_head(object, frame.reveal(object_index))\n    }\n\n    fn replace_unique_path_geometry(\n        &mut self,\n        frame: &FrameState,\n        object_index: usize,\n    ) -> Result<PathReplacementStats, noon_geometry::GeometryError> {\n        let object = &frame.objects[object_index];\n        let GeometryRef::VectorPath(path) = frame.render_geometry(object_index) else {\n            unreachable!("unique path replacement preflight requires vector geometry");\n        };\n        let PreparedSlot::Path { batch, .. } = self.slots[object_index] else {\n            unreachable!("unique path replacement preflight requires a path slot");\n        };\n        let (cache_index, cache_miss) = self.cache_path_mesh(path, object.style)?;\n        let mesh = &self.path_mesh_cache[cache_index].mesh;\n        let packed_vertices = mesh\n            .vertices\n            .iter()\n            .map(|vertex| PathVertex {\n                position: [vertex.position.x, vertex.position.y],\n                target_position: [vertex.target_position.x, vertex.target_position.y],\n                surface: pack_path_surface(vertex.surface, vertex.path_progress),\n            })\n            .collect::<Vec<_>>();\n        let local_indices = mesh.indices.clone();\n\n        let old_vertex_range = self.path_batch_vertex_ranges[batch].clone();\n        let old_index_range = self.path_batches[batch].index_range.clone();\n        let vertex_range = allocate_replacement_range(\n            old_vertex_range,\n            packed_vertices.len(),\n            &mut self.path_vertex_free_ranges,\n            self.path_vertices.len(),\n        );\n        let index_range = allocate_replacement_range(\n            old_index_range,\n            local_indices.len(),\n            &mut self.path_index_free_ranges,\n            self.path_indices.len(),\n        );\n\n        let vertex_range_usize = range_usize_u32(&vertex_range);\n        if self.path_vertices.len() < vertex_range_usize.end {\n            self.path_vertices\n                .resize(vertex_range_usize.end, PathVertex::zeroed());\n        }\n        self.path_vertices[vertex_range_usize.clone()].copy_from_slice(&packed_vertices);\n        push_dirty_range(&mut self.path_vertex_dirty_ranges, vertex_range_usize);\n\n        let index_range_usize = range_usize_u32(&index_range);\n        if self.path_indices.len() < index_range_usize.end {\n            self.path_indices.resize(index_range_usize.end, 0);\n        }\n        let vertex_start = vertex_range.start;\n        for (target, local) in self.path_indices[index_range_usize.clone()]\n            .iter_mut()\n            .zip(local_indices.iter().copied())\n        {\n            *target = local\n                .checked_add(vertex_start)\n                .expect("path index exceeds renderer limits");\n        }\n        push_dirty_range(&mut self.path_index_dirty_ranges, index_range_usize);\n\n        self.path_batch_vertex_ranges[batch] = vertex_range;\n        self.path_batches[batch].index_range = index_range;\n        self.path_batch_cache_indices[batch] = cache_index;\n        self.path_geometry_dirty = true;\n        Ok(PathReplacementStats {\n            cache_miss,\n            vertices_repacked: packed_vertices.len(),\n            indices_repacked: local_indices.len(),\n        })\n    }\n\n    fn rebuild<'a>(&'a mut self, frame: &FrameState) -> PreparedFrame<'a> {\n""",
    "unique replacement methods",
)

lib = replace_once(
    lib,
    """        self.path_batches.clear();\n        self.path_batch_vertex_ranges.clear();\n        self.render_batches.clear();\n""",
    """        self.path_batches.clear();\n        self.path_batch_vertex_ranges.clear();\n        self.path_vertex_free_ranges.clear();\n        self.path_index_free_ranges.clear();\n        self.render_batches.clear();\n""",
    "full rebuild compaction barrier",
)

lib = replace_once(
    lib,
    """            self.path_vertices = next_vertices;\n            self.path_indices = next_indices;\n            self.packed_path_mesh_cache_generation = self.path_mesh_cache_generation;\n            repacked\n""",
    """            self.path_vertices = next_vertices;\n            self.path_indices = next_indices;\n            if !self.path_vertices.is_empty() {\n                self.path_vertex_dirty_ranges.push(0..self.path_vertices.len());\n            }\n            if !self.path_indices.is_empty() {\n                self.path_index_dirty_ranges.push(0..self.path_indices.len());\n            }\n            self.packed_path_mesh_cache_generation = self.path_mesh_cache_generation;\n            repacked\n""",
    "full geometry dirty ranges",
)

lib = replace_once(
    lib,
    """            path_dirty_ranges: &self.path_dirty_ranges,\n            mega_path_instance_dirty_ranges: &self.mega_path_instance_dirty_ranges,\n""",
    """            path_dirty_ranges: &self.path_dirty_ranges,\n            path_vertex_dirty_ranges: &self.path_vertex_dirty_ranges,\n            path_index_dirty_ranges: &self.path_index_dirty_ranges,\n            mega_path_instance_dirty_ranges: &self.mega_path_instance_dirty_ranges,\n""",
    "prepared frame arena dirty slices",
)

lib = replace_once(
    lib,
    """        self.path_dirty_ranges.clear();\n        self.mega_path_instance_dirty_ranges.clear();\n""",
    """        self.path_dirty_ranges.clear();\n        self.path_vertex_dirty_ranges.clear();\n        self.path_index_dirty_ranges.clear();\n        self.mega_path_instance_dirty_ranges.clear();\n""",
    "clear arena dirty ranges",
)

lib = replace_once(
    lib,
    """    fn capacities(&self) -> [usize; 25] {\n""",
    """    fn capacities(&self) -> [usize; 29] {\n""",
    "capacity width",
)
lib = replace_once(
    lib,
    """            self.path_dirty_ranges.capacity(),\n        ]\n""",
    """            self.path_dirty_ranges.capacity(),\n            self.path_vertex_dirty_ranges.capacity(),\n            self.path_index_dirty_ranges.capacity(),\n            self.path_vertex_free_ranges.capacity(),\n            self.path_index_free_ranges.capacity(),\n        ]\n""",
    "arena capacities",
)

marker = """#[derive(Debug)]\nstruct PathGroup {\n"""
insert = """#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]\nstruct PathReplacementStats {\n    cache_miss: bool,\n    vertices_repacked: usize,\n    indices_repacked: usize,\n}\n\n"""
lib = replace_once(lib, marker, insert + marker, "replacement stats")

helper_anchor = """fn path_mesh_key(\n"""
helpers = """fn range_usize_u32(range: &Range<u32>) -> Range<usize> {\n    range.start as usize..range.end as usize\n}\n\nfn allocate_replacement_range(\n    old: Range<u32>,\n    required_len: usize,\n    free_ranges: &mut Vec<Range<u32>>,\n    arena_len: usize,\n) -> Range<u32> {\n    let required = u32::try_from(required_len).expect("path arena range exceeds renderer limits");\n    let old_len = old.end.saturating_sub(old.start);\n    if required == 0 {\n        insert_free_range(free_ranges, old);\n        return 0..0;\n    }\n    if required <= old_len {\n        let used = old.start..old.start + required;\n        insert_free_range(free_ranges, used.end..old.end);\n        return used;\n    }\n\n    insert_free_range(free_ranges, old);\n    if let Some(index) = free_ranges\n        .iter()\n        .position(|range| range.end.saturating_sub(range.start) >= required)\n    {\n        let start = free_ranges[index].start;\n        let used = start..start + required;\n        free_ranges[index].start += required;\n        if free_ranges[index].is_empty() {\n            free_ranges.remove(index);\n        }\n        return used;\n    }\n\n    let start = u32::try_from(arena_len).expect("path arena exceeds renderer limits");\n    start..start + required\n}\n\nfn insert_free_range(free_ranges: &mut Vec<Range<u32>>, range: Range<u32>) {\n    if range.is_empty() {\n        return;\n    }\n    free_ranges.push(range);\n    free_ranges.sort_unstable_by_key(|range| range.start);\n    let mut write = 0usize;\n    for read in 0..free_ranges.len() {\n        if write > 0 && free_ranges[write - 1].end >= free_ranges[read].start {\n            free_ranges[write - 1].end = free_ranges[write - 1].end.max(free_ranges[read].end);\n        } else {\n            free_ranges[write] = free_ranges[read].clone();\n            write += 1;\n        }\n    }\n    free_ranges.truncate(write);\n}\n\n"""
lib = replace_once(lib, helper_anchor, helpers + helper_anchor, "path arena helpers")

# Add bounded replacement regression tests before the existing mega painter-boundary test.
test_anchor = """    #[test]\n    fn mega_paths_never_coalesce_across_an_analytic_painter_boundary() {\n"""
test = r'''    #[test]
    fn unique_path_replacement_dirties_only_its_geometry_chunks() {
        const OBJECT_COUNT: usize = 1_000;
        const REPLACED: usize = 500;
        let objects = (0..OBJECT_COUNT)
            .map(|index| {
                let y = index as f32 * 0.002;
                let mut state = object(
                    index as u64,
                    GeometryRef::path(
                        VectorPath::new()
                            .move_to(Vec2::new(-0.5, y))
                            .line_to(Vec2::new(0.5, y)),
                    ),
                );
                state.style.fill = None;
                state.style.stroke = Some(Color::WHITE);
                state.style.stroke_width = 0.01;
                state
            })
            .collect();
        let mut frame = frame(objects);
        let mut preparer = FramePreparer::new();
        preparer.prepare(&frame);
        let first_vertex_range = preparer.path_batch_vertex_ranges[0].clone();
        let last_vertex_range = preparer.path_batch_vertex_ranges[OBJECT_COUNT - 1].clone();
        let first_index_range = preparer.path_batches[0].index_range.clone();
        let last_index_range = preparer.path_batches[OBJECT_COUNT - 1].index_range.clone();
        let original_vertex_count = preparer.path_vertices.len();
        let original_index_count = preparer.path_indices.len();

        frame.objects[REPLACED].geometry = GeometryRef::path(
            VectorPath::new()
                .move_to(Vec2::new(-0.5, 1.0))
                .cubic_to(
                    Vec2::new(-0.5, 2.0),
                    Vec2::new(0.5, 0.0),
                    Vec2::new(0.5, 1.0),
                )
                .cubic_to(
                    Vec2::new(0.5, 2.0),
                    Vec2::new(-0.5, 0.0),
                    Vec2::new(-0.5, 1.0),
                ),
        );
        let prepared =
            preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![REPLACED]));

        assert_eq!(preparer.path_batch_vertex_ranges[0], first_vertex_range);
        assert_eq!(
            preparer.path_batch_vertex_ranges[OBJECT_COUNT - 1],
            last_vertex_range
        );
        assert_eq!(preparer.path_batches[0].index_range, first_index_range);
        assert_eq!(
            preparer.path_batches[OBJECT_COUNT - 1].index_range,
            last_index_range
        );
        assert_eq!(prepared.path_vertex_dirty_ranges.len(), 1);
        assert_eq!(prepared.path_index_dirty_ranges.len(), 1);
        assert!(prepared.stats.path_vertices_repacked > 0);
        assert!(prepared.stats.path_indices_repacked > 0);
        assert!(prepared.stats.path_vertices_repacked < original_vertex_count);
        assert!(prepared.stats.path_indices_repacked < original_index_count);
        assert!(prepared.path_geometry_dirty);
        assert_eq!(prepared.stats.geometry_cache_misses, 1);
        assert_eq!(prepared.stats.mega_path_count, OBJECT_COUNT);
    }

    #[test]
    fn path_arena_reuses_released_chunks_without_shifting_other_batches() {
        let make_path = |id: u64, y: f32| {
            let mut state = object(
                id,
                GeometryRef::path(
                    VectorPath::new()
                        .move_to(Vec2::new(-0.5, y))
                        .line_to(Vec2::new(0.5, y)),
                ),
            );
            state.style.fill = None;
            state.style.stroke = Some(Color::WHITE);
            state.style.stroke_width = 0.02;
            state
        };
        let mut frame = frame(vec![make_path(1, 0.0), make_path(2, 0.5), make_path(3, 1.0)]);
        let mut preparer = FramePreparer::new();
        preparer.prepare(&frame);

        frame.objects[1].geometry = GeometryRef::path(
            VectorPath::new()
                .move_to(Vec2::new(-1.0, 0.5))
                .cubic_to(
                    Vec2::new(-0.5, 1.5),
                    Vec2::new(0.5, -0.5),
                    Vec2::new(1.0, 0.5),
                ),
        );
        preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![1]));
        assert!(!preparer.path_vertex_free_ranges.is_empty());
        assert!(!preparer.path_index_free_ranges.is_empty());
        let arena_vertices_after_growth = preparer.path_vertices.len();
        let arena_indices_after_growth = preparer.path_indices.len();

        frame.objects[0].geometry = GeometryRef::path(
            VectorPath::new()
                .move_to(Vec2::new(-0.25, 0.0))
                .line_to(Vec2::new(0.25, 0.0)),
        );
        preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));

        assert_eq!(preparer.path_vertices.len(), arena_vertices_after_growth);
        assert_eq!(preparer.path_indices.len(), arena_indices_after_growth);
    }

'''
lib = replace_once(lib, test_anchor, test + test_anchor, "bounded replacement tests")

lib_path.write_text(lib)

# GPU uploads path geometry through dirty ranges instead of full-buffer writes.
gpu_path = Path("crates/noon-render-wgpu/src/gpu.rs")
gpu = gpu_path.read_text()
gpu = replace_once(
    gpu,
    """        ) + upload_full_if(\n            queue,\n            &self.path_vertex_buffer,\n            prepared.path_vertices,\n            prepared.path_geometry_dirty || path_vertex_reallocated,\n        ) + upload_full_if(\n            queue,\n            &self.path_index_buffer,\n            prepared.path_indices,\n            prepared.path_geometry_dirty || path_index_reallocated,\n        ) + upload_dirty(\n""",
    """        ) + upload_dirty(\n            queue,\n            &self.path_vertex_buffer,\n            prepared.path_vertices,\n            prepared.path_vertex_dirty_ranges,\n            path_vertex_reallocated,\n        ) + upload_dirty(\n            queue,\n            &self.path_index_buffer,\n            prepared.path_indices,\n            prepared.path_index_dirty_ranges,\n            path_index_reallocated,\n        ) + upload_dirty(\n""",
    "bounded path geometry GPU upload",
)
gpu_path.write_text(gpu)
