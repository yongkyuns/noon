from pathlib import Path

path = Path("crates/noon-render-wgpu/src/lib.rs")
text = path.read_text()


def replace_once(old: str, new: str) -> None:
    global text
    if new in text:
        return
    if old not in text:
        raise SystemExit(f"expected source fragment not found:\n{old[:240]}")
    text = text.replace(old, new, 1)


replace_once(
    "use noon_core::{Color, GeometryRef, ObjectId, Style, Transform2D, VectorPath};\n",
    "use noon_core::{Color, GeometryRef, ObjectId, PathCommand, Style, Transform2D, VectorPath};\n",
)
replace_once(
    "use std::ops::Range;\n",
    "use std::{\n    collections::{hash_map::DefaultHasher, HashMap},\n    hash::{Hash, Hasher},\n    ops::Range,\n};\n",
)
replace_once(
    "#[derive(Clone, Debug)]\nstruct CachedPathMesh {\n    path: VectorPath,\n    stroke_width_bits: u32,\n    mesh: TessellatedPath,\n}\n",
    "#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]\nstruct PathMeshKey {\n    path_hash: u64,\n    stroke_width_bits: u32,\n}\n\n#[derive(Clone, Debug)]\nstruct CachedPathMesh {\n    path: VectorPath,\n    stroke_width_bits: u32,\n    mesh: TessellatedPath,\n}\n",
)
replace_once(
    "    path_batch_cache_indices: Vec<usize>,\n    path_mesh_cache: Vec<CachedPathMesh>,\n",
    "    path_batch_cache_indices: Vec<usize>,\n    path_mesh_cache: Vec<CachedPathMesh>,\n    path_mesh_lookup: HashMap<PathMeshKey, Vec<usize>>,\n",
)
replace_once(
    "        self.prepared_frame(frame.time, 0, instances_repacked, 0)\n",
    "        normalize_dirty_ranges(&mut self.circle_dirty_ranges);\n        normalize_dirty_ranges(&mut self.rectangle_dirty_ranges);\n        normalize_dirty_ranges(&mut self.line_dirty_ranges);\n        normalize_dirty_ranges(&mut self.path_dirty_ranges);\n\n        self.prepared_frame(frame.time, 0, instances_repacked, 0)\n",
)
replace_once(
    "        let mut path_groups = Vec::<PathGroup>::new();\n        let mut geometry_cache_misses = 0;\n",
    "        let mut path_groups = Vec::<PathGroup>::new();\n        let mut path_group_lookup = HashMap::<usize, usize>::new();\n        let mut geometry_cache_misses = 0;\n",
)
replace_once(
    "                    let batch = path_groups\n                        .iter()\n                        .position(|group| group.cache_index == cache_index)\n                        .unwrap_or_else(|| {\n                            path_groups.push(PathGroup {\n                                cache_index,\n                                ids: Vec::new(),\n                                instances: Vec::new(),\n                            });\n                            path_groups.len() - 1\n                        });\n",
    "                    let batch = match path_group_lookup.get(&cache_index).copied() {\n                        Some(batch) => batch,\n                        None => {\n                            let batch = path_groups.len();\n                            path_groups.push(PathGroup {\n                                cache_index,\n                                ids: Vec::new(),\n                                instances: Vec::new(),\n                            });\n                            path_group_lookup.insert(cache_index, batch);\n                            batch\n                        }\n                    };\n",
)
replace_once(
    "    fn capacities(&self) -> [usize; 19] {\n",
    "    fn capacities(&self) -> [usize; 20] {\n",
)
replace_once(
    "            self.path_batch_cache_indices.capacity(),\n            self.path_mesh_cache.capacity(),\n            self.unsupported.capacity(),\n",
    "            self.path_batch_cache_indices.capacity(),\n            self.path_mesh_cache.capacity(),\n            self.path_mesh_lookup.capacity(),\n            self.unsupported.capacity(),\n",
)
replace_once(
    "    fn cache_path_mesh(\n        &mut self,\n        path: &VectorPath,\n        stroke_width: f32,\n    ) -> Result<(usize, bool), noon_geometry::GeometryError> {\n        let stroke_width_bits = stroke_width.to_bits();\n        if let Some(index) = self\n            .path_mesh_cache\n            .iter()\n            .position(|entry| entry.path == *path && entry.stroke_width_bits == stroke_width_bits)\n        {\n            return Ok((index, false));\n        }\n        let mesh = noon_geometry::tessellate(path, stroke_width)?;\n        self.path_mesh_cache.push(CachedPathMesh {\n            path: path.clone(),\n            stroke_width_bits,\n            mesh,\n        });\n        Ok((self.path_mesh_cache.len() - 1, true))\n    }\n",
    "    fn cache_path_mesh(\n        &mut self,\n        path: &VectorPath,\n        stroke_width: f32,\n    ) -> Result<(usize, bool), noon_geometry::GeometryError> {\n        let stroke_width_bits = stroke_width.to_bits();\n        let key = path_mesh_key(path, stroke_width_bits);\n        if let Some(candidates) = self.path_mesh_lookup.get(&key) {\n            if let Some(index) = candidates.iter().copied().find(|&index| {\n                let entry = &self.path_mesh_cache[index];\n                entry.path == *path && entry.stroke_width_bits == stroke_width_bits\n            }) {\n                return Ok((index, false));\n            }\n        }\n\n        let mesh = noon_geometry::tessellate(path, stroke_width)?;\n        let index = self.path_mesh_cache.len();\n        self.path_mesh_cache.push(CachedPathMesh {\n            path: path.clone(),\n            stroke_width_bits,\n            mesh,\n        });\n        self.path_mesh_lookup.entry(key).or_default().push(index);\n        Ok((index, true))\n    }\n",
)
replace_once(
    "fn pack_circle(object: &FrameObjectState) -> CircleInstance {\n",
    "fn path_mesh_key(path: &VectorPath, stroke_width_bits: u32) -> PathMeshKey {\n    let mut hasher = DefaultHasher::new();\n    hash_vector_path(path, &mut hasher);\n    PathMeshKey {\n        path_hash: hasher.finish(),\n        stroke_width_bits,\n    }\n}\n\nfn hash_vector_path(path: &VectorPath, hasher: &mut impl Hasher) {\n    path.commands().len().hash(hasher);\n    for command in path.commands() {\n        match *command {\n            PathCommand::MoveTo { to } => {\n                0_u8.hash(hasher);\n                hash_vec2(to, hasher);\n            }\n            PathCommand::LineTo { to } => {\n                1_u8.hash(hasher);\n                hash_vec2(to, hasher);\n            }\n            PathCommand::QuadraticTo { control, to } => {\n                2_u8.hash(hasher);\n                hash_vec2(control, hasher);\n                hash_vec2(to, hasher);\n            }\n            PathCommand::CubicTo {\n                control1,\n                control2,\n                to,\n            } => {\n                3_u8.hash(hasher);\n                hash_vec2(control1, hasher);\n                hash_vec2(control2, hasher);\n                hash_vec2(to, hasher);\n            }\n            PathCommand::Close => 4_u8.hash(hasher),\n        }\n    }\n    match path.morph_target() {\n        Some(target) => {\n            1_u8.hash(hasher);\n            hash_vector_path(target, hasher);\n        }\n        None => 0_u8.hash(hasher),\n    }\n}\n\nfn hash_vec2(value: noon_core::Vec2, hasher: &mut impl Hasher) {\n    value.x.to_bits().hash(hasher);\n    value.y.to_bits().hash(hasher);\n}\n\nfn pack_circle(object: &FrameObjectState) -> CircleInstance {\n",
)
replace_once(
    "fn dirty_len(ranges: &[Range<usize>]) -> usize {\n",
    "fn normalize_dirty_ranges(ranges: &mut Vec<Range<usize>>) {\n    if ranges.len() < 2 {\n        return;\n    }\n    ranges.sort_unstable_by_key(|range| range.start);\n    let mut write = 0;\n    for read in 1..ranges.len() {\n        if ranges[read].start <= ranges[write].end {\n            ranges[write].end = ranges[write].end.max(ranges[read].end);\n        } else {\n            write += 1;\n            ranges[write] = ranges[read].clone();\n        }\n    }\n    ranges.truncate(write + 1);\n}\n\nfn dirty_len(ranges: &[Range<usize>]) -> usize {\n",
)

stress_test = '''    fn stress_morph_geometry(variant: usize) -> GeometryRef {\n        let scale = 0.8 + variant as f32 * 0.03;\n        let target = VectorPath::new()\n            .move_to(Vec2::new(0.0, scale))\n            .line_to(Vec2::new(scale, 0.0))\n            .line_to(Vec2::new(0.0, -scale))\n            .line_to(Vec2::new(-scale, 0.0))\n            .close();\n        GeometryRef::path(curved_path().with_morph_target(target))\n    }\n\n    #[test]\n    fn six_hundred_morphs_reuse_twelve_meshes_and_coalesce_uploads() {\n        const OBJECT_COUNT: usize = 600;\n        const VARIANT_COUNT: usize = 12;\n        let geometries: Vec<_> = (0..VARIANT_COUNT).map(stress_morph_geometry).collect();\n        let objects = (0..OBJECT_COUNT)\n            .map(|index| {\n                let mut state = object(index as u64, geometries[index % VARIANT_COUNT].clone());\n                state.style.stroke = Some(Color::WHITE);\n                state.style.stroke_width = 0.02;\n                state\n            })\n            .collect();\n        let mut frame = frame(objects);\n        let mut preparer = FramePreparer::new();\n\n        let prepared = preparer.prepare(&frame);\n        assert_eq!(prepared.stats.instance_count, OBJECT_COUNT);\n        assert_eq!(prepared.stats.geometry_cache_misses, VARIANT_COUNT);\n        assert_eq!(prepared.stats.batch_count, VARIANT_COUNT);\n        assert_eq!(prepared.path_batches.len(), VARIANT_COUNT);\n        assert_eq!(prepared.paths.len(), OBJECT_COUNT);\n        assert_eq!(preparer.cached_path_mesh_count(), VARIANT_COUNT);\n\n        frame.morphs.fill(0.5);\n        let changes = FrameChanges::objects((0..OBJECT_COUNT).collect());\n        let prepared = preparer.prepare_incremental(&frame, &changes);\n\n        assert_eq!(prepared.stats.geometry_cache_misses, 0);\n        assert_eq!(prepared.stats.instances_repacked, OBJECT_COUNT);\n        assert_eq!(prepared.stats.dirty_instance_count, OBJECT_COUNT);\n        assert!(!prepared.path_geometry_dirty);\n        assert_eq!(prepared.path_dirty_ranges, &[0..OBJECT_COUNT]);\n        assert_eq!(preparer.cached_path_mesh_count(), VARIANT_COUNT);\n    }\n\n'''
marker = "    #[test]\n    fn one_hundred_thousand_circles_form_one_batch() {\n"
if "fn six_hundred_morphs_reuse_twelve_meshes_and_coalesce_uploads()" not in text:
    if marker not in text:
        raise SystemExit("stress-test insertion marker not found")
    text = text.replace(marker, stress_test + marker, 1)

path.write_text(text)
