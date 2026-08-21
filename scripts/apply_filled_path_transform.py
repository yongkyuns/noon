from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    if old not in text:
        raise SystemExit(f"expected text missing from {path}: {old[:160]!r}")
    file.write_text(text.replace(old, new, 1))


# --- Geometry: bounded filled-morph plan -----------------------------------
morph = Path("crates/noon-geometry/src/morph.rs")
text = morph.read_text()
marker = "\n#[cfg(test)]\nmod tests {"
if marker not in text:
    raise SystemExit("morph test marker missing")
fill_plan = r'''

const FILL_AREA_EPSILON: f32 = 1.0e-5;

#[derive(Clone, Debug, PartialEq)]
pub struct FilledMorphPlan {
    pub contour: MorphContourPlan,
    pub source_center: Vec2,
    pub target_center: Vec2,
    /// Triangle indices over boundary points followed by one center vertex.
    pub indices: Vec<u32>,
}

impl FilledMorphPlan {
    pub fn vertex_count(&self) -> usize {
        self.contour.source_points.len() + 1
    }

    pub fn interpolate_vertices(&self, progress: f32) -> Vec<Vec2> {
        let progress = if progress.is_finite() {
            progress.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mut vertices = self
            .contour
            .source_points
            .iter()
            .zip(&self.contour.target_points)
            .map(|(source, target)| lerp_vec2(*source, *target, progress))
            .collect::<Vec<_>>();
        vertices.push(lerp_vec2(
            self.source_center,
            self.target_center,
            progress,
        ));
        vertices
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum FilledMorphError {
    Morph(MorphError),
    RequiresSingleClosedContour,
    DegenerateArea { side: MorphSide },
    SelfIntersecting { side: MorphSide },
    NoStableFanTriangulation,
}

impl std::fmt::Display for FilledMorphError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Morph(error) => write!(formatter, "filled morph planning failed: {error}"),
            Self::RequiresSingleClosedContour => formatter.write_str(
                "filled path Transform currently requires exactly one closed contour",
            ),
            Self::DegenerateArea { side } => {
                write!(formatter, "filled morph {side:?} contour has degenerate area")
            }
            Self::SelfIntersecting { side } => {
                write!(formatter, "filled morph {side:?} contour self-intersects")
            }
            Self::NoStableFanTriangulation => formatter.write_str(
                "filled morph has no stable center-fan triangulation over the full interpolation interval",
            ),
        }
    }
}

impl std::error::Error for FilledMorphError {}

impl From<MorphError> for FilledMorphError {
    fn from(value: MorphError) -> Self {
        Self::Morph(value)
    }
}

/// Plans the deliberately bounded first filled-path Transform topology.
///
/// The supported class is one simple closed contour whose source and target
/// are both star-shaped around their area centroids, with every center-fan
/// triangle retaining positive orientation for the complete linear morph.
/// This gives one provably fixed triangle topology and rejects cases that would
/// require per-frame triangulation or could invert triangles during playback.
pub fn plan_filled_morph(
    source: &VectorPath,
    target: &VectorPath,
    options: MorphOptions,
) -> Result<FilledMorphPlan, FilledMorphError> {
    let plan = plan_morph(source, target, options)?;
    if plan.contours.len() != 1 || !plan.contours[0].closed {
        return Err(FilledMorphError::RequiresSingleClosedContour);
    }
    let mut contour = plan.contours.into_iter().next().expect("one contour validated");

    canonicalize_ccw(&mut contour.source_points, MorphSide::Source)?;
    canonicalize_ccw(&mut contour.target_points, MorphSide::Target)?;
    contour.target_points = align_closed_contour_preserving_winding(
        &contour.source_points,
        &contour.target_points,
    );

    if !polygon_is_simple(&contour.source_points) {
        return Err(FilledMorphError::SelfIntersecting {
            side: MorphSide::Source,
        });
    }
    if !polygon_is_simple(&contour.target_points) {
        return Err(FilledMorphError::SelfIntersecting {
            side: MorphSide::Target,
        });
    }

    let source_center = polygon_centroid(&contour.source_points).ok_or(
        FilledMorphError::DegenerateArea {
            side: MorphSide::Source,
        },
    )?;
    let target_center = polygon_centroid(&contour.target_points).ok_or(
        FilledMorphError::DegenerateArea {
            side: MorphSide::Target,
        },
    )?;

    let count = contour.source_points.len();
    if count < 3 {
        return Err(FilledMorphError::NoStableFanTriangulation);
    }
    for index in 0..count {
        let next = (index + 1) % count;
        let minimum = minimum_triangle_orientation_over_interval(
            source_center,
            target_center,
            contour.source_points[index],
            contour.target_points[index],
            contour.source_points[next],
            contour.target_points[next],
        );
        if !minimum.is_finite() || minimum <= FILL_AREA_EPSILON as f64 {
            return Err(FilledMorphError::NoStableFanTriangulation);
        }
    }

    let center = u32::try_from(count).map_err(|_| FilledMorphError::NoStableFanTriangulation)?;
    let mut indices = Vec::with_capacity(count * 3);
    for index in 0..count {
        let next = (index + 1) % count;
        indices.extend([
            center,
            u32::try_from(index).map_err(|_| FilledMorphError::NoStableFanTriangulation)?,
            u32::try_from(next).map_err(|_| FilledMorphError::NoStableFanTriangulation)?,
        ]);
    }

    Ok(FilledMorphPlan {
        contour,
        source_center,
        target_center,
        indices,
    })
}

fn canonicalize_ccw(points: &mut Vec<Vec2>, side: MorphSide) -> Result<(), FilledMorphError> {
    let area = signed_polygon_area(points);
    if !area.is_finite() || area.abs() <= FILL_AREA_EPSILON {
        return Err(FilledMorphError::DegenerateArea { side });
    }
    if area < 0.0 {
        points.reverse();
    }
    Ok(())
}

fn signed_polygon_area(points: &[Vec2]) -> f32 {
    if points.len() < 3 {
        return 0.0;
    }
    0.5 * (0..points.len())
        .map(|index| {
            let next = (index + 1) % points.len();
            points[index].x * points[next].y - points[next].x * points[index].y
        })
        .sum::<f32>()
}

fn polygon_centroid(points: &[Vec2]) -> Option<Vec2> {
    let twice_area = 2.0 * signed_polygon_area(points);
    if !twice_area.is_finite() || twice_area.abs() <= 2.0 * FILL_AREA_EPSILON {
        return None;
    }
    let mut x = 0.0_f32;
    let mut y = 0.0_f32;
    for index in 0..points.len() {
        let next = (index + 1) % points.len();
        let cross = points[index].x * points[next].y - points[next].x * points[index].y;
        x += (points[index].x + points[next].x) * cross;
        y += (points[index].y + points[next].y) * cross;
    }
    let denominator = 3.0 * twice_area;
    let centroid = Vec2::new(x / denominator, y / denominator);
    (centroid.x.is_finite() && centroid.y.is_finite()).then_some(centroid)
}

fn align_closed_contour_preserving_winding(source: &[Vec2], target: &[Vec2]) -> Vec<Vec2> {
    debug_assert_eq!(source.len(), target.len());
    let count = source.len();
    let mut best_cost = f64::INFINITY;
    let mut best_shift = 0;
    for shift in 0..count {
        let cost = (0..count)
            .map(|index| squared_distance(source[index], target[(index + shift) % count]) as f64)
            .sum::<f64>();
        if cost < best_cost {
            best_cost = cost;
            best_shift = shift;
        }
    }
    (0..count)
        .map(|index| target[(index + best_shift) % count])
        .collect()
}

fn polygon_is_simple(points: &[Vec2]) -> bool {
    if points.len() < 3 {
        return false;
    }
    let count = points.len();
    for first in 0..count {
        let first_next = (first + 1) % count;
        if distance(points[first], points[first_next]) <= DEGENERATE_LENGTH_EPSILON {
            return false;
        }
        for second in first + 1..count {
            let second_next = (second + 1) % count;
            if first == second
                || first_next == second
                || second_next == first
                || (first == 0 && second_next == 0)
            {
                continue;
            }
            if segments_intersect(
                points[first],
                points[first_next],
                points[second],
                points[second_next],
            ) {
                return false;
            }
        }
    }
    true
}

fn segments_intersect(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    let ab_c = orientation(a, b, c);
    let ab_d = orientation(a, b, d);
    let cd_a = orientation(c, d, a);
    let cd_b = orientation(c, d, b);
    let epsilon = FILL_AREA_EPSILON;

    if ((ab_c > epsilon && ab_d < -epsilon) || (ab_c < -epsilon && ab_d > epsilon))
        && ((cd_a > epsilon && cd_b < -epsilon) || (cd_a < -epsilon && cd_b > epsilon))
    {
        return true;
    }
    (ab_c.abs() <= epsilon && point_on_segment(c, a, b))
        || (ab_d.abs() <= epsilon && point_on_segment(d, a, b))
        || (cd_a.abs() <= epsilon && point_on_segment(a, c, d))
        || (cd_b.abs() <= epsilon && point_on_segment(b, c, d))
}

fn orientation(a: Vec2, b: Vec2, c: Vec2) -> f32 {
    cross(
        Vec2::new(b.x - a.x, b.y - a.y),
        Vec2::new(c.x - a.x, c.y - a.y),
    )
}

fn point_on_segment(point: Vec2, start: Vec2, end: Vec2) -> bool {
    let epsilon = FILL_AREA_EPSILON;
    point.x >= start.x.min(end.x) - epsilon
        && point.x <= start.x.max(end.x) + epsilon
        && point.y >= start.y.min(end.y) - epsilon
        && point.y <= start.y.max(end.y) + epsilon
}

fn minimum_triangle_orientation_over_interval(
    source_center: Vec2,
    target_center: Vec2,
    source_a: Vec2,
    target_a: Vec2,
    source_b: Vec2,
    target_b: Vec2,
) -> f64 {
    let a0 = Vec2::new(source_a.x - source_center.x, source_a.y - source_center.y);
    let b0 = Vec2::new(source_b.x - source_center.x, source_b.y - source_center.y);
    let center_delta = Vec2::new(
        target_center.x - source_center.x,
        target_center.y - source_center.y,
    );
    let da = Vec2::new(
        target_a.x - source_a.x - center_delta.x,
        target_a.y - source_a.y - center_delta.y,
    );
    let db = Vec2::new(
        target_b.x - source_b.x - center_delta.x,
        target_b.y - source_b.y - center_delta.y,
    );
    let c0 = cross(a0, b0) as f64;
    let c1 = (cross(da, b0) + cross(a0, db)) as f64;
    let c2 = cross(da, db) as f64;
    let evaluate = |time: f64| c0 + c1 * time + c2 * time * time;
    let mut minimum = evaluate(0.0).min(evaluate(1.0));
    if c2 > 0.0 {
        let critical = -c1 / (2.0 * c2);
        if (0.0..1.0).contains(&critical) {
            minimum = minimum.min(evaluate(critical));
        }
    }
    minimum
}
'''
text = text.replace(marker, fill_plan + marker, 1)
morph.write_text(text)

# --- Geometry tessellation: emit fixed fill topology when requested --------
replace_once(
    "crates/noon-geometry/src/tessellation.rs",
    '''pub fn tessellate_styled(\n    path: &VectorPath,\n    stroke_width: f32,\n    stroke_join: StrokeJoin,\n    stroke_cap: StrokeCap,\n) -> Result<TessellatedPath, GeometryError> {\n    if !stroke_width.is_finite() || stroke_width < 0.0 {\n''',
    '''pub fn tessellate_styled(\n    path: &VectorPath,\n    stroke_width: f32,\n    stroke_join: StrokeJoin,\n    stroke_cap: StrokeCap,\n) -> Result<TessellatedPath, GeometryError> {\n    tessellate_styled_with_fill(path, stroke_width, stroke_join, stroke_cap, true)\n}\n\npub fn tessellate_styled_with_fill(\n    path: &VectorPath,\n    stroke_width: f32,\n    stroke_join: StrokeJoin,\n    stroke_cap: StrokeCap,\n    fill_enabled: bool,\n) -> Result<TessellatedPath, GeometryError> {\n    if !stroke_width.is_finite() || stroke_width < 0.0 {\n''',
)
replace_once(
    "crates/noon-geometry/src/tessellation.rs",
    '''        return tessellate_morph_path(path, target, stroke_width, stroke_join, stroke_cap);\n''',
    '''        return tessellate_morph_path(\n            path,\n            target,\n            stroke_width,\n            stroke_join,\n            stroke_cap,\n            fill_enabled,\n        );\n''',
)

# Wrap Lyon static fill tessellation in the explicit fill policy.
tess = Path("crates/noon-geometry/src/tessellation.rs")
text = tess.read_text()
start = text.index("    FillTessellator::new()")
end = text.index("\n\n    if stroke_width > 0.0 {", start)
fill_block = text[start:end]
indented = "\n".join("    " + line if line else line for line in fill_block.splitlines())
text = text[:start] + "    if fill_enabled {\n" + indented + "\n    }" + text[end:]
tess.write_text(text)

replace_once(
    "crates/noon-geometry/src/tessellation.rs",
    '''fn tessellate_morph_path(\n    source: &VectorPath,\n    target: &VectorPath,\n    stroke_width: f32,\n    stroke_join: StrokeJoin,\n    stroke_cap: StrokeCap,\n) -> Result<TessellatedPath, GeometryError> {\n    if stroke_width == 0.0 {\n        return Ok(TessellatedPath {\n            morphing: true,\n            ..TessellatedPath::default()\n        });\n    }\n    let plan = crate::plan_morph(source, target, crate::MorphOptions::DEFAULT)\n        .map_err(|error| GeometryError::Tessellation(format!("morph planning failed: {error}")))?;\n    let total_points = plan.point_count();\n    let mut vertices = Vec::new();\n    let mut indices = Vec::new();\n''',
    '''fn tessellate_morph_path(\n    source: &VectorPath,\n    target: &VectorPath,\n    stroke_width: f32,\n    stroke_join: StrokeJoin,\n    stroke_cap: StrokeCap,\n    fill_enabled: bool,\n) -> Result<TessellatedPath, GeometryError> {\n    let mut vertices = Vec::new();\n    let mut indices = Vec::new();\n\n    if fill_enabled {\n        let fill = crate::plan_filled_morph(source, target, crate::MorphOptions::DEFAULT)\n            .map_err(|error| {\n                GeometryError::Tessellation(format!("filled morph planning failed: {error}"))\n            })?;\n        let vertex_start = u32::try_from(vertices.len())\n            .map_err(|_| GeometryError::Tessellation("filled morph vertex count overflow".into()))?;\n        for (source_point, target_point) in fill\n            .contour\n            .source_points\n            .iter()\n            .zip(&fill.contour.target_points)\n        {\n            vertices.push(MeshVertex {\n                position: *source_point,\n                target_position: *target_point,\n                surface: PathSurface::Fill,\n                path_distance: 0.0,\n                path_progress: 1.0,\n            });\n        }\n        vertices.push(MeshVertex {\n            position: fill.source_center,\n            target_position: fill.target_center,\n            surface: PathSurface::Fill,\n            path_distance: 0.0,\n            path_progress: 1.0,\n        });\n        indices.extend(fill.indices.iter().map(|index| {\n            index\n                .checked_add(vertex_start)\n                .expect("filled morph index overflow validated by vertex count")\n        }));\n    }\n\n    if stroke_width == 0.0 {\n        let bounds = morph_mesh_bounds(&vertices);\n        return Ok(TessellatedPath {\n            vertices,\n            indices,\n            bounds,\n            stroke_length: 0.0,\n            morphing: true,\n        });\n    }\n\n    let plan = crate::plan_morph(source, target, crate::MorphOptions::DEFAULT)\n        .map_err(|error| GeometryError::Tessellation(format!("morph planning failed: {error}")))?;\n    let total_points = plan.point_count();\n''',
)

# --- Compiler: validate fill topology before producing a PathPair -----------
replace_once(
    "crates/noon-compile/Cargo.toml",
    '''[dependencies]\nnoon-core = { path = "../noon-core" }\n''',
    '''[dependencies]\nnoon-core = { path = "../noon-core" }\nnoon-geometry = { path = "../noon-geometry" }\n''',
)
replace_once(
    "crates/noon-compile/src/lib.rs",
    '''    UnsupportedTransformGeometry(TrackId),\n    PathTransformRequiresRetessellation(TrackId),\n''',
    '''    UnsupportedTransformGeometry(TrackId),\n    PathTransformRequiresRetessellation(TrackId),\n    UnsafeFilledPathTransform(TrackId),\n''',
)
replace_once(
    "crates/noon-compile/src/lib.rs",
    '''            Self::PathTransformRequiresRetessellation(id) => write!(\n                formatter,\n                "transform track {} changes path fill topology or stroke width",\n                id.get()\n            ),\n''',
    '''            Self::PathTransformRequiresRetessellation(id) => write!(\n                formatter,\n                "transform track {} changes path fill presence, stroke topology, or stroke width",\n                id.get()\n            ),\n            Self::UnsafeFilledPathTransform(id) => write!(\n                formatter,\n                "transform track {} uses filled path geometry without a stable fixed triangulation",\n                id.get()\n            ),\n''',
)
# CompilePatchError has the same variant list/display; replace the remaining occurrences.
compile = Path("crates/noon-compile/src/lib.rs")
text = compile.read_text()
needle = "    PathTransformRequiresRetessellation(TrackId),\n"
pos = text.find(needle, text.find("pub enum CompilePatchError"))
if pos < 0:
    raise SystemExit("CompilePatchError variant marker missing")
text = text[: pos + len(needle)] + "    UnsafeFilledPathTransform(TrackId),\n" + text[pos + len(needle) :]
needle_display = '''            Self::PathTransformRequiresRetessellation(id) => write!(\n                formatter,\n                "transform track {} changes path fill topology or stroke width",\n                id.get()\n            ),\n'''
pos = text.find(needle_display, text.find("impl std::fmt::Display for CompilePatchError"))
if pos < 0:
    raise SystemExit("CompilePatchError display marker missing")
replacement = '''            Self::PathTransformRequiresRetessellation(id) => write!(\n                formatter,\n                "transform track {} changes path fill presence, stroke topology, or stroke width",\n                id.get()\n            ),\n            Self::UnsafeFilledPathTransform(id) => write!(\n                formatter,\n                "transform track {} uses filled path geometry without a stable fixed triangulation",\n                id.get()\n            ),\n'''
text = text[:pos] + replacement + text[pos + len(needle_display) :]
compile.write_text(text)

replace_once(
    "crates/noon-compile/src/lib.rs",
    '''enum TransformCompileFailure {\n    UnsupportedGeometry,\n    RequiresRetessellation,\n}\n''',
    '''enum TransformCompileFailure {\n    UnsupportedGeometry,\n    RequiresRetessellation,\n    UnsafeFilledPath,\n}\n''',
)
replace_once(
    "crates/noon-compile/src/lib.rs",
    '''        (GeometryRef::VectorPath(source), GeometryRef::VectorPath(target)) => {\n            if from.style.fill.is_some() || to.style.fill.is_some() {\n                return Err(TransformCompileFailure::RequiresRetessellation);\n            }\n            TransformGeometryPlan::PathPair(GeometryRef::path(\n                source.clone().with_morph_target(target.clone()),\n            ))\n        }\n''',
    '''        (GeometryRef::VectorPath(source), GeometryRef::VectorPath(target)) => {\n            if from.style.fill.is_some() != to.style.fill.is_some() {\n                return Err(TransformCompileFailure::RequiresRetessellation);\n            }\n            if from.style.fill.is_some()\n                && noon_geometry::plan_filled_morph(\n                    source,\n                    target,\n                    noon_geometry::MorphOptions::DEFAULT,\n                )\n                .is_err()\n            {\n                return Err(TransformCompileFailure::UnsafeFilledPath);\n            }\n            TransformGeometryPlan::PathPair(GeometryRef::path(\n                source.clone().with_morph_target(target.clone()),\n            ))\n        }\n''',
)
replace_once(
    "crates/noon-compile/src/lib.rs",
    '''        TransformCompileFailure::RequiresRetessellation => {\n            CompileError::PathTransformRequiresRetessellation(id)\n        }\n''',
    '''        TransformCompileFailure::RequiresRetessellation => {\n            CompileError::PathTransformRequiresRetessellation(id)\n        }\n        TransformCompileFailure::UnsafeFilledPath => CompileError::UnsafeFilledPathTransform(id),\n''',
)
replace_once(
    "crates/noon-compile/src/lib.rs",
    '''        TransformCompileFailure::RequiresRetessellation => {\n            CompilePatchError::PathTransformRequiresRetessellation(id)\n        }\n''',
    '''        TransformCompileFailure::RequiresRetessellation => {\n            CompilePatchError::PathTransformRequiresRetessellation(id)\n        }\n        TransformCompileFailure::UnsafeFilledPath => {\n            CompilePatchError::UnsafeFilledPathTransform(id)\n        }\n''',
)

# --- Renderer: fill participation is part of mesh-cache identity ------------
replace_once(
    "crates/noon-render-wgpu/src/lib.rs",
    '''struct PathMeshKey {\n    path_hash: u64,\n    stroke_width_bits: u32,\n    stroke_join: StrokeJoin,\n    stroke_cap: StrokeCap,\n}\n''',
    '''struct PathMeshKey {\n    path_hash: u64,\n    stroke_width_bits: u32,\n    stroke_join: StrokeJoin,\n    stroke_cap: StrokeCap,\n    fill_enabled: bool,\n}\n''',
)
replace_once(
    "crates/noon-render-wgpu/src/lib.rs",
    '''struct CachedPathMesh {\n    path: VectorPath,\n    stroke_width_bits: u32,\n    stroke_join: StrokeJoin,\n    stroke_cap: StrokeCap,\n    mesh: TessellatedPath,\n}\n''',
    '''struct CachedPathMesh {\n    path: VectorPath,\n    stroke_width_bits: u32,\n    stroke_join: StrokeJoin,\n    stroke_cap: StrokeCap,\n    fill_enabled: bool,\n    mesh: TessellatedPath,\n}\n''',
)
replace_once(
    "crates/noon-render-wgpu/src/lib.rs",
    '''                    && cache.stroke_width_bits == object.style.stroke_width.to_bits()\n                    && cache.stroke_join == object.style.stroke_join\n                    && cache.stroke_cap == object.style.stroke_cap\n''',
    '''                    && cache.stroke_width_bits == object.style.stroke_width.to_bits()\n                    && cache.stroke_join == object.style.stroke_join\n                    && cache.stroke_cap == object.style.stroke_cap\n                    && cache.fill_enabled == object.style.fill.is_some()\n''',
)
replace_once(
    "crates/noon-render-wgpu/src/lib.rs",
    '''        let stroke_width_bits = style.stroke_width.to_bits();\n        let key = path_mesh_key(path, stroke_width_bits, style.stroke_join, style.stroke_cap);\n''',
    '''        let stroke_width_bits = style.stroke_width.to_bits();\n        let fill_enabled = style.fill.is_some();\n        let key = path_mesh_key(\n            path,\n            stroke_width_bits,\n            style.stroke_join,\n            style.stroke_cap,\n            fill_enabled,\n        );\n''',
)
replace_once(
    "crates/noon-render-wgpu/src/lib.rs",
    '''                    && entry.stroke_width_bits == stroke_width_bits\n                    && entry.stroke_join == style.stroke_join\n                    && entry.stroke_cap == style.stroke_cap\n''',
    '''                    && entry.stroke_width_bits == stroke_width_bits\n                    && entry.stroke_join == style.stroke_join\n                    && entry.stroke_cap == style.stroke_cap\n                    && entry.fill_enabled == fill_enabled\n''',
)
replace_once(
    "crates/noon-render-wgpu/src/lib.rs",
    '''        let mesh = noon_geometry::tessellate_styled(\n            path,\n            style.stroke_width,\n            style.stroke_join,\n            style.stroke_cap,\n        )?;\n''',
    '''        let mesh = noon_geometry::tessellate_styled_with_fill(\n            path,\n            style.stroke_width,\n            style.stroke_join,\n            style.stroke_cap,\n            fill_enabled,\n        )?;\n''',
)
replace_once(
    "crates/noon-render-wgpu/src/lib.rs",
    '''            stroke_width_bits,\n            stroke_join: style.stroke_join,\n            stroke_cap: style.stroke_cap,\n            mesh,\n''',
    '''            stroke_width_bits,\n            stroke_join: style.stroke_join,\n            stroke_cap: style.stroke_cap,\n            fill_enabled,\n            mesh,\n''',
)
replace_once(
    "crates/noon-render-wgpu/src/lib.rs",
    '''fn path_mesh_key(\n    path: &VectorPath,\n    stroke_width_bits: u32,\n    stroke_join: StrokeJoin,\n    stroke_cap: StrokeCap,\n) -> PathMeshKey {\n''',
    '''fn path_mesh_key(\n    path: &VectorPath,\n    stroke_width_bits: u32,\n    stroke_join: StrokeJoin,\n    stroke_cap: StrokeCap,\n    fill_enabled: bool,\n) -> PathMeshKey {\n''',
)
replace_once(
    "crates/noon-render-wgpu/src/lib.rs",
    '''        stroke_width_bits,\n        stroke_join,\n        stroke_cap,\n    }\n}\n''',
    '''        stroke_width_bits,\n        stroke_join,\n        stroke_cap,\n        fill_enabled,\n    }\n}\n''',
)

# --- Geometry verification --------------------------------------------------
Path("crates/noon-geometry/tests/filled_morph.rs").write_text(r'''use noon_core::{StrokeCap, StrokeJoin, Vec2, VectorPath};
use noon_geometry::{
    plan_filled_morph, tessellate_styled_with_fill, FilledMorphError, MorphOptions, PathSurface,
};

fn rounded_loop() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(0.0, 1.6))
        .cubic_to(Vec2::new(0.95, 1.6), Vec2::new(1.6, 0.95), Vec2::new(1.6, 0.0))
        .cubic_to(Vec2::new(1.6, -0.95), Vec2::new(0.95, -1.6), Vec2::new(0.0, -1.6))
        .cubic_to(Vec2::new(-0.95, -1.6), Vec2::new(-1.6, -0.95), Vec2::new(-1.6, 0.0))
        .cubic_to(Vec2::new(-1.6, 0.95), Vec2::new(-0.95, 1.6), Vec2::new(0.0, 1.6))
        .close()
}

fn star() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(0.0, 2.0))
        .line_to(Vec2::new(0.47, 0.65))
        .line_to(Vec2::new(1.9, 0.62))
        .line_to(Vec2::new(0.76, -0.25))
        .line_to(Vec2::new(1.18, -1.62))
        .line_to(Vec2::new(0.0, -0.82))
        .line_to(Vec2::new(-1.18, -1.62))
        .line_to(Vec2::new(-0.76, -0.25))
        .line_to(Vec2::new(-1.9, 0.62))
        .line_to(Vec2::new(-0.47, 0.65))
        .close()
}

fn triangle_area(a: Vec2, b: Vec2, c: Vec2) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

#[test]
fn rounded_loop_to_concave_star_has_stable_fill_topology() {
    let plan = plan_filled_morph(&rounded_loop(), &star(), MorphOptions::DEFAULT)
        .expect("regular star is star-shaped around its centroid");
    assert_eq!(plan.indices.len(), plan.contour.source_points.len() * 3);
    assert_eq!(plan.vertex_count(), plan.contour.source_points.len() + 1);

    for progress in [0.0, 0.125, 0.25, 0.5, 0.75, 0.875, 1.0] {
        let vertices = plan.interpolate_vertices(progress);
        for triangle in plan.indices.chunks_exact(3) {
            let a = vertices[triangle[0] as usize];
            let b = vertices[triangle[1] as usize];
            let c = vertices[triangle[2] as usize];
            assert!(triangle_area(a, b, c) > 1.0e-5, "triangle inverted at {progress}");
        }
    }
}

#[test]
fn filled_morph_tessellation_contains_fill_and_stroke_with_one_topology() {
    let source = rounded_loop().with_morph_target(star());
    let mesh = tessellate_styled_with_fill(
        &source,
        0.12,
        StrokeJoin::Round,
        StrokeCap::Round,
        true,
    )
    .expect("safe filled morph must tessellate");

    assert!(mesh.morphing);
    assert!(mesh.vertices.iter().any(|vertex| vertex.surface == PathSurface::Fill));
    assert!(mesh.vertices.iter().any(|vertex| vertex.surface == PathSurface::Stroke));
    assert!(mesh
        .vertices
        .iter()
        .any(|vertex| vertex.position != vertex.target_position));
    assert!(mesh.indices.iter().all(|index| (*index as usize) < mesh.vertices.len()));
}

#[test]
fn self_intersecting_target_is_rejected() {
    let bow_tie = VectorPath::new()
        .move_to(Vec2::new(-1.0, -1.0))
        .line_to(Vec2::new(1.0, 1.0))
        .line_to(Vec2::new(-1.0, 1.0))
        .line_to(Vec2::new(1.0, -1.0))
        .close();
    assert!(matches!(
        plan_filled_morph(&rounded_loop(), &bow_tie, MorphOptions::DEFAULT),
        Err(FilledMorphError::SelfIntersecting { .. })
            | Err(FilledMorphError::DegenerateArea { .. })
            | Err(FilledMorphError::NoStableFanTriangulation)
    ));
}

#[test]
fn open_or_multi_contour_fill_is_rejected() {
    let open = VectorPath::new()
        .move_to(Vec2::new(-1.0, 0.0))
        .line_to(Vec2::new(1.0, 0.0))
        .line_to(Vec2::new(0.0, 1.0));
    assert!(matches!(
        plan_filled_morph(&open, &open, MorphOptions::DEFAULT),
        Err(FilledMorphError::RequiresSingleClosedContour)
    ));
}
''')

# --- Compiler verification --------------------------------------------------
compile = Path("crates/noon-compile/src/lib.rs")
text = compile.read_text()
marker = "\n    #[test]\n    fn object_ids_resolve_to_dense_indices()"
if marker not in text:
    raise SystemExit("compiler test insertion marker missing")
compiler_tests = r'''

    fn filled_loop() -> noon_core::VectorPath {
        noon_core::VectorPath::new()
            .move_to(Vec2::new(0.0, 1.5))
            .cubic_to(Vec2::new(1.0, 1.5), Vec2::new(1.5, 1.0), Vec2::new(1.5, 0.0))
            .cubic_to(Vec2::new(1.5, -1.0), Vec2::new(1.0, -1.5), Vec2::new(0.0, -1.5))
            .cubic_to(Vec2::new(-1.0, -1.5), Vec2::new(-1.5, -1.0), Vec2::new(-1.5, 0.0))
            .cubic_to(Vec2::new(-1.5, 1.0), Vec2::new(-1.0, 1.5), Vec2::new(0.0, 1.5))
            .close()
    }

    fn filled_star() -> noon_core::VectorPath {
        noon_core::VectorPath::new()
            .move_to(Vec2::new(0.0, 1.9))
            .line_to(Vec2::new(0.45, 0.62))
            .line_to(Vec2::new(1.8, 0.58))
            .line_to(Vec2::new(0.72, -0.24))
            .line_to(Vec2::new(1.12, -1.54))
            .line_to(Vec2::new(0.0, -0.78))
            .line_to(Vec2::new(-1.12, -1.54))
            .line_to(Vec2::new(-0.72, -0.24))
            .line_to(Vec2::new(-1.8, 0.58))
            .line_to(Vec2::new(-0.45, 0.62))
            .close()
    }

    #[test]
    fn safe_filled_path_transform_compiles_to_fixed_path_pair() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::path(filled_loop()));
        let mut from = noon_core::ObjectSnapshot::new(GeometryRef::path(filled_loop()));
        let mut to = noon_core::ObjectSnapshot::new(GeometryRef::path(filled_star()));
        from.style.fill = Some(noon_core::Color::WHITE);
        to.style.fill = Some(noon_core::Color::BLACK);
        scene
            .add_track(TrackDefinition {
                id: noon_core::TrackId::new(0),
                object,
                property: Property::Transform,
                values: TrackValues::Object { from, to },
                timing: TrackTiming::new(0.0, 2.0, Easing::Linear),
            })
            .expect("safe filled Transform track must be valid");

        let compiled = CompiledScene::compile(&scene).expect("safe filled path must compile");
        assert!(matches!(
            compiled.tracks()[0].transform_geometry_plan,
            Some(TransformGeometryPlan::PathPair(_))
        ));
    }

    #[test]
    fn filled_path_transform_rejects_fill_presence_change() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::path(filled_loop()));
        let from = noon_core::ObjectSnapshot::new(GeometryRef::path(filled_loop()));
        let mut to = noon_core::ObjectSnapshot::new(GeometryRef::path(filled_star()));
        to.style.fill = None;
        let mut from = from;
        from.style.fill = Some(noon_core::Color::WHITE);
        scene
            .add_track(TrackDefinition {
                id: noon_core::TrackId::new(0),
                object,
                property: Property::Transform,
                values: TrackValues::Object { from, to },
                timing: TrackTiming::new(0.0, 2.0, Easing::Linear),
            })
            .expect("semantic track is valid before compilation");
        assert!(matches!(
            CompiledScene::compile(&scene),
            Err(CompileError::PathTransformRequiresRetessellation(_))
        ));
    }
'''
text = text.replace(marker, compiler_tests + marker, 1)
compile.write_text(text)

# --- Renderer verification --------------------------------------------------
renderer = Path("crates/noon-render-wgpu/src/lib.rs")
text = renderer.read_text()
marker = "\n    #[test]\n    fn packed_instance_layout_is_stable()"
if marker not in text:
    raise SystemExit("renderer test insertion marker missing")
renderer_test = r'''

    #[test]
    fn filled_morph_reuses_geometry_after_cold_prepare() {
        let source = curved_path();
        let target = VectorPath::new()
            .move_to(Vec2::new(0.0, 1.3))
            .line_to(Vec2::new(0.38, 0.42))
            .line_to(Vec2::new(1.2, 0.4))
            .line_to(Vec2::new(0.5, -0.18))
            .line_to(Vec2::new(0.74, -1.05))
            .line_to(Vec2::new(0.0, -0.52))
            .line_to(Vec2::new(-0.74, -1.05))
            .line_to(Vec2::new(-0.5, -0.18))
            .line_to(Vec2::new(-1.2, 0.4))
            .line_to(Vec2::new(-0.38, 0.42))
            .close();
        let geometry = GeometryRef::path(source.with_morph_target(target));
        let mut path = object(7, geometry.clone());
        path.style.fill = Some(Color::WHITE);
        path.style.stroke = Some(Color::BLACK);
        path.style.stroke_width = 0.08;
        let mut initial = frame(vec![path.clone()]);
        initial.render_geometries[0] = Some(geometry.clone());
        let mut preparer = FramePreparer::new();

        let cold = preparer.prepare(&initial);
        assert_eq!(cold.stats.geometry_cache_misses, 1);
        assert!(cold.path_vertices.iter().any(|vertex| vertex.surface & 1 == 0));
        let vertices = cold.path_vertices.to_vec();
        let indices = cold.path_indices.to_vec();

        let mut advanced = initial.clone();
        advanced.morphs[0] = 0.5;
        let changes = FrameChanges::objects(vec![0]);
        let steady = preparer.prepare_incremental(&advanced, &changes);
        assert_eq!(steady.stats.geometry_cache_misses, 0);
        assert!(!steady.path_geometry_dirty);
        assert_eq!(steady.path_vertices, vertices);
        assert_eq!(steady.path_indices, indices);
        assert_eq!(steady.path_dirty_ranges, &[0..1]);
    }
'''
text = text.replace(marker, renderer_test + marker, 1)
renderer.write_text(text)

# --- Browser demo -----------------------------------------------------------
Path("web/python/examples/filled_path_transform.py").write_text(r'''from noon import Color, Scene, Transform, VectorPath

scene = Scene()

source = (
    VectorPath()
    .move_to((0.0, 1.65))
    .cubic_to((0.95, 1.65), (1.65, 0.95), (1.65, 0.0))
    .cubic_to((1.65, -0.95), (0.95, -1.65), (0.0, -1.65))
    .cubic_to((-0.95, -1.65), (-1.65, -0.95), (-1.65, 0.0))
    .cubic_to((-1.65, 0.95), (-0.95, 1.65), (0.0, 1.65))
    .close()
)

target = (
    VectorPath()
    .move_to((0.0, 2.0))
    .line_to((0.47, 0.65))
    .line_to((1.9, 0.62))
    .line_to((0.76, -0.25))
    .line_to((1.18, -1.62))
    .line_to((0.0, -0.82))
    .line_to((-1.18, -1.62))
    .line_to((-0.76, -0.25))
    .line_to((-1.9, 0.62))
    .line_to((-0.47, 0.65))
    .close()
)

shape = scene.path(
    source,
    fill=Color(0.18, 0.62, 0.96),
    stroke=Color(0.96, 0.96, 1.0),
    stroke_width=0.08,
    key="filled-morph",
)
target_shape = scene.path(
    target,
    fill=Color(0.78, 0.32, 0.94),
    stroke=Color(0.96, 0.96, 1.0),
    stroke_width=0.08,
)
scene.remove(target_shape)

scene.play(
    Transform(shape, target_shape, key="filled-morph.transform"),
    duration=4.0,
    easing="ease_in_out_cubic",
)
result = scene
''')

# The detached target API already supports Mobjects; expose the new example.
main = Path("web/main.js")
text = main.read_text()
anchor = '''  {\n    name: "Path morph / Transform",\n    path: "./python/examples/path_morph_transform.py",\n'''
position = text.find(anchor)
if position < 0:
    raise SystemExit("path morph gallery anchor missing")
entry = '''  {\n    name: "Filled path Transform",\n    path: "./python/examples/filled_path_transform.py",\n    summary:\n      "A filled rounded loop morphs into a concave star using one validated fixed triangle topology.",\n    features: "Transform · fixed fill topology · fill + stroke",\n  },\n'''
text = text[:position] + entry + text[position:]
main.write_text(text)

# Add the example to the Python execution test list.
replace_once(
    "web/python/test_examples.py",
    '''    "path_morph_transform.py",\n    "morph_stress_test.py",\n''',
    '''    "path_morph_transform.py",\n    "filled_path_transform.py",\n    "morph_stress_test.py",\n''',
)
