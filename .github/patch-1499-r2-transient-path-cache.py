from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    if old not in text:
        raise SystemExit(f"missing expected block in {path}: {old[:140]!r}")
    p.write_text(text.replace(old, new, 1))


# Expose only the existing cache lookup result to renderer sibling modules. The
# cache and its key/tessellation semantics remain owned by FramePreparer.
lib = "crates/noon-render-wgpu/src/lib.rs"
replace_once(
    lib,
    '''        self.path_mesh_lookup.entry(key).or_default().push(index);\n        Ok((index, true))\n    }\n\n    fn next_path_mesh_use(&mut self) -> u64 {\n''',
    '''        self.path_mesh_lookup.entry(key).or_default().push(index);\n        Ok((index, true))\n    }\n\n    pub(crate) fn path_mesh_with_cache(\n        &mut self,\n        path: &VectorPath,\n        style: Style,\n        transform: Transform2D,\n    ) -> Result<(&TessellatedPath, bool), noon_geometry::GeometryError> {\n        let (index, cache_miss) = self.cache_path_mesh(path, style, transform)?;\n        Ok((&self.path_mesh_cache[index].mesh, cache_miss))\n    }\n\n    fn next_path_mesh_use(&mut self) -> u64 {\n''',
)

render = "crates/noon-render-wgpu/src/render_order.rs"
replace_once(
    render,
    '''    pub path_batches: Vec<crate::PathBatch>,\n    pub slots: Vec<PreparedDerivedDisplaySlot>,\n''',
    '''    pub path_batches: Vec<crate::PathBatch>,\n    /// Path meshes tessellated by this preparation rather than reused from the\n    /// retained renderer's existing path-mesh cache.\n    pub path_geometry_cache_misses: usize,\n    pub slots: Vec<PreparedDerivedDisplaySlot>,\n''',
)
replace_once(
    render,
    '''pub fn prepare_derived_display(\n    publication: &noon_runtime::RendererPublication<'_>,\n) -> Result<PreparedDerivedDisplay, DerivedDisplayRenderError> {\n    prepare_derived_display_inner(publication, None)\n}\n\n/// Prepare only derived occurrences whose real source anchor participates in this\n/// viewport projection. Stable and derived painter ordering still comes from the\n/// authoritative publication order rather than candidate order.\npub fn prepare_derived_display_visible(\n    publication: &noon_runtime::RendererPublication<'_>,\n    visible_object_indices: &[usize],\n) -> Result<PreparedDerivedDisplay, DerivedDisplayRenderError> {\n    let visible = visible_object_indices\n        .iter()\n        .copied()\n        .collect::<std::collections::HashSet<_>>();\n    prepare_derived_display_inner(publication, Some(&visible))\n}\n\nfn prepare_derived_display_inner(\n    publication: &noon_runtime::RendererPublication<'_>,\n    visible: Option<&std::collections::HashSet<usize>>,\n) -> Result<PreparedDerivedDisplay, DerivedDisplayRenderError> {\n''',
    '''pub fn prepare_derived_display(\n    publication: &noon_runtime::RendererPublication<'_>,\n) -> Result<PreparedDerivedDisplay, DerivedDisplayRenderError> {\n    prepare_derived_display_inner(publication, None, None)\n}\n\n/// Prepare only derived occurrences whose real source anchor participates in this\n/// viewport projection. Stable and derived painter ordering still comes from the\n/// authoritative publication order rather than candidate order.\npub fn prepare_derived_display_visible(\n    publication: &noon_runtime::RendererPublication<'_>,\n    visible_object_indices: &[usize],\n) -> Result<PreparedDerivedDisplay, DerivedDisplayRenderError> {\n    let visible = visible_object_indices\n        .iter()\n        .copied()\n        .collect::<std::collections::HashSet<_>>();\n    prepare_derived_display_inner(publication, Some(&visible), None)\n}\n\npub(crate) fn prepare_derived_display_visible_cached(\n    publication: &noon_runtime::RendererPublication<'_>,\n    visible_object_indices: &[usize],\n    path_cache: &mut FramePreparer,\n) -> Result<PreparedDerivedDisplay, DerivedDisplayRenderError> {\n    let visible = visible_object_indices\n        .iter()\n        .copied()\n        .collect::<std::collections::HashSet<_>>();\n    prepare_derived_display_inner(publication, Some(&visible), Some(path_cache))\n}\n\nfn prepare_derived_display_inner(\n    publication: &noon_runtime::RendererPublication<'_>,\n    visible: Option<&std::collections::HashSet<usize>>,\n    mut path_cache: Option<&mut FramePreparer>,\n) -> Result<PreparedDerivedDisplay, DerivedDisplayRenderError> {\n''',
)
replace_once(
    render,
    '''        seen_anchors.insert(object_index);\n        for &object in objects {\n            pack_derived_display_object(object, &mut prepared)?;\n            prepared.painter_items.push(DisplayPainterItem::Derived {\n''',
    '''        seen_anchors.insert(object_index);\n        for &object in objects {\n            pack_derived_display_object(object, &mut prepared, path_cache.as_deref_mut())?;\n            prepared.painter_items.push(DisplayPainterItem::Derived {\n''',
)
replace_once(
    render,
    '''fn pack_derived_display_object(\n    object: &noon_runtime::DerivedDisplayObject,\n    prepared: &mut PreparedDerivedDisplay,\n) -> Result<(), DerivedDisplayRenderError> {\n''',
    '''fn pack_derived_display_object(\n    object: &noon_runtime::DerivedDisplayObject,\n    prepared: &mut PreparedDerivedDisplay,\n    path_cache: Option<&mut FramePreparer>,\n) -> Result<(), DerivedDisplayRenderError> {\n''',
)
replace_once(
    render,
    '''        noon_core::GeometryRef::VectorPath(path) => {\n            let mesh = crate::tessellate_path_mesh(path, state.style, render_transform)\n                .map_err(|_| DerivedDisplayRenderError::UnsupportedGeometry(occurrence))?;\n            let vertex_start = u32::try_from(prepared.path_vertices.len())\n''',
    '''        noon_core::GeometryRef::VectorPath(path) => {\n            let uncached_mesh;\n            let (mesh, cache_miss) = if let Some(path_cache) = path_cache {\n                path_cache\n                    .path_mesh_with_cache(path, state.style, render_transform)\n                    .map_err(|_| DerivedDisplayRenderError::UnsupportedGeometry(occurrence))?\n            } else {\n                uncached_mesh = crate::tessellate_path_mesh(path, state.style, render_transform)\n                    .map_err(|_| DerivedDisplayRenderError::UnsupportedGeometry(occurrence))?;\n                (&uncached_mesh, true)\n            };\n            prepared.path_geometry_cache_misses += usize::from(cache_miss);\n            let vertex_start = u32::try_from(prepared.path_vertices.len())\n''',
)
replace_once(
    render,
    '''        assert_eq!(prepared.path_batches.len(), 1);\n        assert_eq!(prepared.paths[0].path_params, [1.0, 0.25]);\n''',
    '''        assert_eq!(prepared.path_batches.len(), 1);\n        assert_eq!(prepared.path_geometry_cache_misses, 1);\n        assert_eq!(prepared.paths[0].path_params, [1.0, 0.25]);\n''',
)
replace_once(
    render,
    '''    #[test]\n    fn painter_anchor_requires_source_z_index_and_stable_order_membership() {\n''',
    '''    #[test]\n    fn cached_transient_path_reuses_mesh_without_dirtying_stable_path() {\n        let stable_path = noon_core::VectorPath::new()\n            .move_to(Vec2::new(-0.8, -0.6))\n            .line_to(Vec2::new(0.8, -0.6))\n            .line_to(Vec2::new(0.0, 0.8))\n            .close();\n        let transient_path = noon_core::VectorPath::new()\n            .move_to(Vec2::new(-0.4, -0.4))\n            .line_to(Vec2::new(0.4, -0.4))\n            .line_to(Vec2::new(0.4, 0.4))\n            .line_to(Vec2::new(-0.4, 0.4))\n            .close();\n        let mut runtime = runtime(vec![GeometryRef::path(stable_path)]);\n        let mut transient_state = state(GeometryRef::path(transient_path));\n        transient_state.style.fill = Some(noon_core::Color::WHITE);\n        transient_state.style.stroke = None;\n        let derived = [DerivedDisplayObject::new(0, 5, transient_state)];\n        let publication = runtime\n            .take_renderer_publication()\n            .with_derived_display_objects(&derived)\n            .unwrap();\n\n        let mut preparer = FramePreparer::new();\n        preparer.set_painter_order(publication.frame(), publication.painter_order());\n        {\n            let stable = preparer.prepare(publication.frame());\n            assert_eq!(stable.stats.geometry_cache_misses, 1);\n        }\n        let stable_vertices = preparer.path_vertices.clone();\n        let stable_indices = preparer.path_indices.clone();\n\n        let first = prepare_derived_display_visible_cached(&publication, &[0], &mut preparer)\n            .expect("first transient path preparation");\n        assert_eq!(first.path_geometry_cache_misses, 1);\n        let second = prepare_derived_display_visible_cached(&publication, &[0], &mut preparer)\n            .expect("second transient path preparation");\n        assert_eq!(second.path_geometry_cache_misses, 0);\n        assert_eq!(first.path_vertices, second.path_vertices);\n        assert_eq!(first.path_indices, second.path_indices);\n\n        let stable = preparer.prepare_incremental(publication.frame(), &FrameChanges::default());\n        assert_eq!(stable.stats.geometry_cache_misses, 0);\n        assert_eq!(stable.stats.path_vertices_repacked, 0);\n        assert_eq!(stable.stats.path_indices_repacked, 0);\n        assert_eq!(stable.stats.dirty_instance_count, 0);\n        assert!(!stable.path_geometry_dirty);\n        assert_eq!(stable.path_vertices, stable_vertices);\n        assert_eq!(stable.path_indices, stable_indices);\n    }\n\n    #[test]\n    fn painter_anchor_requires_source_z_index_and_stable_order_membership() {\n''',
)
replace_once(
    render,
    '''    use noon_runtime::{DerivedDisplayObject, DerivedDisplayObjectState, SceneInstance};\n''',
    '''    use noon_runtime::{\n        DerivedDisplayObject, DerivedDisplayObjectState, FrameChanges, SceneInstance,\n    };\n''',
)

# Route direct retained hosts through the existing retained preparer's path cache.
retained = "crates/noon-render-wgpu/src/gpu/retained_text.rs"
replace_once(
    retained,
    '''    pub const fn visibility_stats(&self) -> RetainedVisibilityProjectionStats {\n        self.visibility_stats\n    }\n\n    #[allow(clippy::too_many_arguments)]\n''',
    '''    pub const fn visibility_stats(&self) -> RetainedVisibilityProjectionStats {\n        self.visibility_stats\n    }\n\n    /// Prepare identity-free transient presentation through the same path-mesh\n    /// cache owned by retained geometry preparation. The returned rows remain\n    /// disposable and do not enter stable renderer slots or identity tables.\n    pub fn prepare_transient_presentations_visible(\n        &mut self,\n        publication: &RendererPublication<'_>,\n        visible_object_indices: &[usize],\n    ) -> Result<crate::PreparedDerivedDisplay, crate::DerivedDisplayRenderError> {\n        crate::prepare_derived_display_visible_cached(\n            publication,\n            visible_object_indices,\n            &mut self.geometry,\n        )\n    }\n\n    #[allow(clippy::too_many_arguments)]\n''',
)

native = "crates/noon-native/src/lib.rs"
replace_once(
    native,
    '''use noon_render_wgpu::{\n    prepare_derived_display_visible, Camera2D, GpuRenderer, RetainedFramePreparer,\n    RetainedTextGpuState,\n};\n''',
    '''use noon_render_wgpu::{Camera2D, GpuRenderer, RetainedFramePreparer, RetainedTextGpuState};\n''',
)
replace_once(
    native,
    '''        let derived = prepare_derived_display_visible(&publication, visibility.object_indices())\n            .map_err(|error| NativeHostError::Gpu(error.to_string()))?;\n        let prepared = gpu\n            .preparer\n''',
    '''        let derived = gpu\n            .preparer\n            .prepare_transient_presentations_visible(&publication, visibility.object_indices())\n            .map_err(|error| NativeHostError::Gpu(error.to_string()))?;\n        let prepared = gpu\n            .preparer\n''',
)

web = "crates/noon-web/src/execution_canvas.rs"
replace_once(
    web,
    '''    use noon_render_wgpu::{\n        prepare_derived_display_visible, Camera2D, GpuRenderer, RetainedFramePreparer,\n        RetainedTextGpuState,\n    };\n''',
    '''    use noon_render_wgpu::{Camera2D, GpuRenderer, RetainedFramePreparer, RetainedTextGpuState};\n''',
)
replace_once(
    web,
    '''                let derived =\n                    prepare_derived_display_visible(&publication, visibility.object_indices())\n                        .map_err(js_error)?;\n                let prepared = self\n                    .direct_preparer\n''',
    '''                let derived = self\n                    .direct_preparer\n                    .prepare_transient_presentations_visible(\n                        &publication,\n                        visibility.object_indices(),\n                    )\n                    .map_err(js_error)?;\n                let prepared = self\n                    .direct_preparer\n''',
)
