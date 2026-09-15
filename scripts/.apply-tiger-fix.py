from pathlib import Path
root = Path('.')
def edit(file, old, new, count=1):
    p = root / file
    s = p.read_text()
    assert s.count(old) == count, (file, s.count(old), old[:80])
    p.write_text(s.replace(old, new))

f = 'crates/noon-geometry/src/morph/correspondence.rs'
p = root / f
s = p.read_text()
start = s.index('pub(super) fn interpolate(')
end = s.index('\nfn split(', start)
s = s[:start] + '''/// Prepared canonical cubic correspondence for a path whose fill topology can change.
///
/// Preparation is independent of playback time. Sampling preserves the same point,
/// contour and closure ordering used by ordinary path transforms; no endpoint fan
/// triangulation is assumed. The renderer can tessellate the current filled path
/// without mutating or retransmitting its immutable execution resource.
#[derive(Clone, Debug)]
pub struct PreparedPathInterpolation {
    contours: Vec<PreparedContourInterpolation>,
}

#[derive(Clone, Debug)]
struct PreparedContourInterpolation {
    curves: Vec<([Vec2; 4], [Vec2; 4])>,
    closed: bool,
}

impl PreparedPathInterpolation {
    pub fn new(source: &VectorPath, target: &VectorPath) -> Result<Self, MorphError> {
        if !source.is_finite() || !target.is_finite() {
            return Err(GeometryError::NonFinitePoint.into());
        }
        let contours = aligned_contours(source, target)?
            .into_iter()
            .map(|(source, target)| PreparedContourInterpolation {
                curves: source.curves.into_iter().zip(target.curves).collect(),
                closed: source.closed,
            })
            .collect();
        Ok(Self { contours })
    }

    pub fn interpolate(&self, progress: f32) -> Result<VectorPath, MorphError> {
        if !progress.is_finite() || !(0.0..=1.0).contains(&progress) {
            return Err(MorphError::PathAlignment(
                crate::PathProportionError::InvalidProportion(progress),
            ));
        }
        let mut path = VectorPath::new();
        for contour in &self.contours {
            for (index, (a, b)) in contour.curves.iter().enumerate() {
                let p: [Vec2; 4] = std::array::from_fn(|index| {
                    a[index] * (1.0 - progress) + b[index] * progress
                });
                if index == 0 {
                    path = path.move_to(p[0]);
                }
                path = path.cubic_to(p[1], p[2], p[3]);
            }
            if contour.closed {
                path = path.close();
            }
        }
        if !path.is_finite() {
            return Err(GeometryError::NonFinitePoint.into());
        }
        Ok(path)
    }
}

pub(super) fn interpolate(
    source: &VectorPath,
    target: &VectorPath,
    progress: f32,
) -> Result<VectorPath, MorphError> {
    PreparedPathInterpolation::new(source, target)?.interpolate(progress)
}
''' + s[end:]
p.write_text(s)
edit('crates/noon-geometry/src/morph.rs', 'mod correspondence;', 'mod correspondence;\npub use correspondence::PreparedPathInterpolation;')
f = 'crates/noon-compile/src/transform.rs'
p = root / f
s = p.read_text().replace('filled_morph_plan_is_safe', 'filled_morph_is_supported')
s = s.replace('Filled-path safety is style-dependent and was validated before', 'Filled-path support is style-dependent and was validated before')
s = s.replace('// Overflowed derived points and unsafe world-space filled topology retain', '// Overflowed derived points and unsupported world-space correspondence retain')
old = '''        .is_ok()
}

// Independent drivers'''
new = '''        .is_ok()
        // Complex/concave and changing-contour fills have no valid retained fan.
        // They retain the SAME ordered endpoint resource and progress channel;
        // the renderer samples/tessellates only that affected path locally.
        || noon_geometry::PreparedPathInterpolation::new(source, target).is_ok()
}

// Independent drivers'''
assert s.count(old) == 1
s = s.replace(old, new)
p.write_text(s)
f = 'crates/noon-compile/tests/generic_transform.rs'
p = root / f
s = p.read_text().replace('fn unsafe_filled_path_transform_is_rejected_before_runtime()', 'fn self_intersecting_filled_path_transform_retains_ordered_endpoints()')
s = s.replace('''    assert!(matches!(
        CompiledScene::compile_objects(source_objects, &source_tracks),
        Err(CompileError::UnsafeFilledPathTransform(_))
    ));''', '''    assert!(CompiledScene::compile_objects(source_objects, &source_tracks).is_ok());''')
p.write_text(s)
edit('crates/noon-compile/src/transform.rs', '''        assert_eq!(
            compile_path_pair(
                visible_fill,
                visible_fill,
                Transform2D::IDENTITY,
                Transform2D::IDENTITY,
                source,
                target,
            ),
            Err(TransformCompileFailure::UnsafeFilledPath)
        );''', '''        assert!(compile_path_pair(
            visible_fill,
            visible_fill,
            Transform2D::IDENTITY,
            Transform2D::IDENTITY,
            source,
            target,
        ).is_ok(), "open fills use the same implicit closure as ordinary tessellation");''')
f = 'crates/noon-render-wgpu/src/lib.rs'
edit(f, 'mod reveal;', 'mod reveal;\nmod sampled_morph;')
edit(f, '    last_used: u64,\n}', '    last_used: u64,\n    sampled: Option<sampled_morph::SampledPathMesh>,\n}')
edit(f, '    painter_order_indices: Vec<u32>,', '    painter_order_indices: Vec<u32>,\n    painter_order_positions: Vec<Option<usize>>,')
edit(f, '    path_mesh_lookup: HashMap<PathMeshKey, Vec<usize>>,', '    path_mesh_lookup: HashMap<PathMeshKey, Vec<usize>>,\n    sampled_path_mesh_lookup: HashMap<sampled_morph::SampledMeshOwner, usize>,')
edit(f, '''            resident: None,
            last_used,
        });''', '''            resident: None,
            last_used,
            sampled: None,
        });''')
edit(f, 'self.cache_path_mesh(path, object.style, frame.render_transform(object_index))', '''self.cache_path_mesh_at_progress(
                path, object.style, frame.render_transform(object_index),
                sampled_morph::SampledMeshOwner::Stable(object.id), frame.morph(object_index),
            )''', count=2)
edit(f, '''                let cache_index = match self.cache_path_mesh(
                    path,
                    object.style,
                    frame.render_transform(object_index),
                )''', '''                let cache_index = match self.cache_path_mesh_at_progress(
                    path,
                    object.style,
                    frame.render_transform(object_index),
                    sampled_morph::SampledMeshOwner::Stable(object.id),
                    frame.morph(object_index),
                )''')
edit(f, '''                    && geometry_matches
                    && reveal_head_available''', '''                    && geometry_matches
                    && cache.sampled.as_ref().is_none_or(|sampled| {
                        sampled.owner == sampled_morph::SampledMeshOwner::Stable(object.id)
                            && sampled.progress_bits == frame.morph(object_index).to_bits()
                    })
                    && reveal_head_available''')
edit(f, '''        for object_index in replacement_indices {
            let replacement''', '''        let mut replacement_chunks = std::collections::BTreeSet::new();
        for object_index in replacement_indices {
            let replacement''')
edit(f, '''            path_indices_repacked += replacement.indices_repacked;
        }
        if path_vertices_repacked > 0 || path_indices_repacked > 0 {
            self.rebuild_ordered_render_batches();
            self.rebuild_mega_render_batches();
            self.rebuild_render_order_chunks(None);
        }''', '''            path_indices_repacked += replacement.indices_repacked;
            let position = if self.painter_order_installed {
                self.painter_order_positions.get(object_index).copied().flatten()
            } else {
                Some(object_index)
            };
            if let Some(position) = position {
                replacement_chunks.insert(position / Self::RENDER_ORDER_CHUNK_SIZE);
            }
        }
        for chunk in replacement_chunks {
            let start = chunk * Self::RENDER_ORDER_CHUNK_SIZE;
            self.rebuild_render_order_chunks(Some(start..start + Self::RENDER_ORDER_CHUNK_SIZE));
        }''')
edit(f, '''    pub(crate) fn cached_path_mesh(
        &mut self,
        path: &VectorPath,
        style: Style,
        transform: Transform2D,
    ) -> Result<(&TessellatedPath, bool), noon_geometry::GeometryError> {
        let (index, cache_miss) = self.cache_path_mesh(path, style, transform)?;''', '''    pub(crate) fn cached_path_mesh(
        &mut self,
        path: &VectorPath,
        style: Style,
        transform: Transform2D,
        owner: sampled_morph::SampledMeshOwner,
        progress: f32,
    ) -> Result<(&TessellatedPath, bool), noon_geometry::GeometryError> {
        let (index, cache_miss) = self.cache_path_mesh_at_progress(path, style, transform, owner, progress)?;''')
edit(f, '''            let stroke_transform =
                path_stroke_transform_key(object.style, frame.render_transform(object_index));''', '''            if let Some(&index) = self.sampled_path_mesh_lookup.get(
                &sampled_morph::SampledMeshOwner::Stable(object.id),
            ) {
                keep[index] = true;
            }
            let stroke_transform =
                path_stroke_transform_key(object.style, frame.render_transform(object_index));''')
edit(f, '''        self.path_mesh_lookup.clear();
        for (old_index, entry) in old_cache.into_iter().enumerate() {''', '''        self.path_mesh_lookup.clear();
        self.sampled_path_mesh_lookup.clear();
        for (old_index, entry) in old_cache.into_iter().enumerate() {''')
edit(f, '''            self.path_mesh_cache.push(entry);
            self.path_mesh_lookup
                .entry(key)
                .or_default()
                .push(new_index);''', '''            if let Some(sampled) = &entry.sampled {
                self.sampled_path_mesh_lookup.insert(sampled.owner, new_index);
            } else {
                self.path_mesh_lookup.entry(key).or_default().push(new_index);
            }
            self.path_mesh_cache.push(entry);''')
f = 'crates/noon-render-wgpu/src/render_order.rs'
edit(f, '''        self.painter_order_indices.extend_from_slice(order);
        self.painter_order_installed = true;''', '''        self.painter_order_indices.extend_from_slice(order);
        self.painter_order_positions.clear();
        self.painter_order_positions.resize(frame.objects.len(), None);
        for (position, &index) in order.iter().enumerate() {
            self.painter_order_positions[index as usize] = Some(position);
        }
        self.painter_order_installed = true;''')
edit(f, '''        self.painter_order_indices.clear();
        self.painter_order_installed = false;''', '''        self.painter_order_indices.clear();
        self.painter_order_positions.clear();
        self.painter_order_installed = false;''')
edit(f, '''        let new_end = range.end.min(order.len());
        self.painter_order_indices.splice(''', '''        let new_end = range.end.min(order.len());
        self.painter_order_positions.resize(frame.objects.len(), None);
        for &index in &self.painter_order_indices[range.start.min(old_end)..old_end] {
            if let Some(position) = self.painter_order_positions.get_mut(index as usize) {
                *position = None;
            }
        }
        for (offset, &index) in order[range.start..new_end].iter().enumerate() {
            self.painter_order_positions[index as usize] = Some(range.start + offset);
        }
        self.painter_order_indices.splice(''')
edit(f, '''                    .cached_path_mesh(path, state.style, render_transform)''', '''                    .cached_path_mesh(
                        path, state.style, render_transform,
                        crate::sampled_morph::SampledMeshOwner::Derived {
                            anchor: object.anchor_object_index(), occurrence,
                        },
                        state.morph,
                    )''')
edit(f, '''                let mesh = crate::tessellate_path_mesh(path, state.style, render_transform)''', '''                let mesh = crate::sampled_morph::tessellate_path_at_progress(
                    path, state.style, render_transform, state.morph,
                )''')
f = 'crates/noon-render-wgpu/src/path_residency.rs'
edit(f, '''            let (index, _) = self.cache_path_mesh(path, request.style, request.transform)?;''', '''            let (index, _) = match self.cache_path_mesh(path, request.style, request.transform) {
                Ok(cached) => cached,
                Err(_) if sampled_morph::prepare_sampled_path(path, request.style).is_ok() => continue,
                Err(error) => return Err(error),
            };''')
edit(f, '''                let (index, cache_miss) =
                    self.cache_path_mesh(path, request.style, request.transform)?;''', '''                let (index, cache_miss) = match self.cache_path_mesh(path, request.style, request.transform) {
                    Ok(cached) => cached,
                    Err(_) if sampled_morph::prepare_sampled_path(path, request.style).is_ok() => return Ok(()),
                    Err(error) => return Err(error),
                };''')
f = 'crates/noon-render-wgpu/src/mega_mesh.rs'
edit(f, '''            .iter()
            .map(|batch| {
                batch.instance_range.end == batch.instance_range.start + 1''', '''            .iter()
            .enumerate()
            .map(|(index, batch)| {
                self.path_mesh_cache[self.path_batch_cache_indices[index]].sampled.is_none()
                    && batch.instance_range.end == batch.instance_range.start + 1''')
p = root / 'web/python/examples/manim_compatible_svg_tiger_morph.py'
s = p.read_text()
start = s.index('        # Complex filled paths intentionally')
s = s[:start] + '''        # Preserve the authored paint throughout both real path transforms.
        # Complex fills are sampled/tessellated by the shared Rust renderer, not
        # replaced with outlines or hidden until an instantaneous endpoint repaint.
        tiger_return = tiger.copy()

        self.add(tiger)
        self.wait(0.5)
        self.play(Transform(tiger, rocket), run_time=1.8)
        self.wait(0.75)
        self.play(Transform(tiger, tiger_return), run_time=1.8)
        self.wait(0.85)
'''
p.write_text(s)
p = root / 'docs/architecture.md'
s = p.read_text()
anchor = 'Installed immutable geometry may carry one-shot preparation hints'
assert s.count(anchor) == 1
s = s.replace(anchor, 'A filled path whose changing shape has no safe time-invariant triangulation uses a renderer-local sampled-mesh specialization. The same immutable ordered endpoint resource and scalar progress remain authoritative; canonical cubic correspondence is cached once on first use. Only the affected presentation row is sampled and tessellated, using ordinary fill rules. One disposable mesh per stable row or identity-free derived occurrence is reused across progress values, and local arena ranges and painter chunks are updated without rebuilding unrelated state. Such a mesh is never installed as an immutable resident prefix or shared between independently progressing occurrences. Safe fan and stroke-only morphs retain GPU endpoint interpolation, and an unchanged progress/style/geometry sample performs no tessellation. This correctness path has work proportional to the changed path complexity, not the total scene; it must not become a frontend per-frame path loop or a sequence of new cross-worker geometry resources.\n\n' + anchor)
p.write_text(s)
p = root / 'crates/noon/src/family_transform_renderer_publication_tests.rs'
s = p.read_text()
assert 'complex_filled_family_round_trip_restores_padding_without_repainting' not in s
s += '''
#[test]
fn complex_filled_family_round_trip_restores_padding_without_repainting() {
    use noon_core::{Color, Vec2, VectorPath};
    let mut scene = Scene::new();
    let concave = VectorPath::new().move_to(Vec2::new(0.0, 0.0))
        .line_to(Vec2::new(8.0, 0.0)).line_to(Vec2::new(8.0, 8.0))
        .line_to(Vec2::new(6.0, 8.0)).line_to(Vec2::new(6.0, 2.0))
        .line_to(Vec2::new(2.0, 2.0)).line_to(Vec2::new(2.0, 8.0))
        .line_to(Vec2::new(0.0, 8.0)).close();
    let rectangle = VectorPath::new().move_to(Vec2::new(0.0, 0.0))
        .line_to(Vec2::new(8.0, 0.0)).line_to(Vec2::new(8.0, 8.0))
        .line_to(Vec2::new(0.0, 8.0)).close();
    let source_objects = (0..3).map(|_| scene.path(concave.clone(), Default::default()).unwrap())
        .collect::<Vec<_>>();
    let source_members = source_objects.iter().map(|object| object.into()).collect::<Vec<_>>();
    let source = scene.family(&source_members).unwrap();
    source.set_fill(Some(Color::WHITE), Some(1.0)).unwrap();
    let returned = source.copy_family().unwrap();
    let target_objects = (0..2).map(|_| scene.path(rectangle.clone(), Default::default()).unwrap())
        .collect::<Vec<_>>();
    let target_members = target_objects.iter().map(|object| object.into()).collect::<Vec<_>>();
    let target = scene.family(&target_members).unwrap();
    target.set_fill(Some(Color::WHITE), Some(1.0)).unwrap();
    scene.add_many(&[(&source).into()]).unwrap();
    let mut execution = scene.execution_session().unwrap();
    {
        let mut live = scene.live(&mut execution);
        let segment = family_transform(&mut live, &source, &target, 1.8);
        assert_eq!(live.segment_state(segment).timeline(), TimelineWakeState::Continuous);
        live.advance_segment_to(segment, 0.9).unwrap();
        finish(&mut live, segment);
    }
    drain(&mut execution);
    assert_eq!(scene.live(&mut execution).effective(&source_objects[1]).unwrap().appearance, 0.0);
    {
        let wait = scene.live(&mut execution).wait_segment(0.75).unwrap();
        finish(&mut scene.live(&mut execution), wait);
    }
    drain(&mut execution);
    {
        let mut live = scene.live(&mut execution);
        let segment = family_transform(&mut live, &source, returned.root(), 1.8);
        assert_eq!(live.segment_state(segment).timeline(), TimelineWakeState::Continuous);
        live.advance_segment_to(segment, segment.start_time() + segment.duration()*0.5).unwrap();
        let appearance = live.effective(&source_objects[1]).unwrap().appearance;
        assert!(appearance > 0.0 && appearance < 1.0, "padding must recover continuously: {appearance}");
        finish(&mut live, segment);
    }
    drain(&mut execution);
    {
        let wait = scene.live(&mut execution).wait_segment(0.85).unwrap();
        finish(&mut scene.live(&mut execution), wait);
    }
    drain(&mut execution);
    let mut live = scene.live(&mut execution);
    for object in &source_objects {
        assert_eq!(live.effective(object).unwrap().appearance, 1.0);
    }
    live.copy_family(&source).expect("returned filled family remains capturable after hold/drain");
}
'''
p.write_text(s)
p = root / 'scripts/playground-python-family-transform-smoke.mjs'
s = p.read_text()
s = s.replace('        returned.set_fill(opacity=0)\n        contracted.set_fill(opacity=0)\n', '        source.set_fill(WHITE, opacity=1)\n        returned.set_fill(WHITE, opacity=1)\n        contracted.set_fill(WHITE, opacity=1)\n')
s = s.replace('        self.play(source.animate.set_fill(opacity=0), run_time=0.35)\n', '')
s = s.replace('        source.set_fill(opacity=1)\n', '')
s = s.replace('        source.set_fill(opacity=0)\n        self.wait(0.35)\n', '')
s = s.replace('        self.play(source.animate.set_fill(opacity=1), run_time=0.35)\n', '        self.wait(0.85)\n')
s = s.replace('  await page.evaluate((pythonSource) => {', "  // Do not join the gallery's initial run and mistake its completion for ours.\n  await page.waitForFunction(() => !window.__noonExampleGallery.runInFlight &&\n    document.querySelector('#patch-status')?.dataset.state === 'applied', null, { timeout: 90000 });\n  await page.evaluate((pythonSource) => {")
s = s.replace("    editor.dispatchEvent(new Event('input', { bubbles: true }));", "    // This fixture explicitly invokes Run. Dispatching input also queues an\n    // automatic rerun, whose overlapping completion is not this test's subject.")
p.write_text(s)
print('Applied tiger filled-morph integration and regression coverage')
