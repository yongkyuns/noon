from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one replacement, found {count}")
    p.write_text(text.replace(old, new, 1))


path = "crates/noon-web/src/authoring_mobject.rs"
replace_once(
    path,
    "    semantic_path_bounds, Bounds2D64, Color, GeometryRef, ObjectSnapshot, SemanticPaint,\n    SemanticStyle, SemanticTransform2_5D, SemanticVec3, Style, Vec2,\n",
    "    Bounds2D64, Color, GeometryRef, ObjectSnapshot, PathCommand, SemanticPaint, SemanticStyle,\n    SemanticTransform2_5D, SemanticVec3, Style, Vec2, VectorPath,\n",
)

old_bounds = '''fn snapshot_layout_bounds(
    snapshot: &ObjectSnapshot,
    transform: SemanticTransform2_5D,
) -> Option<Bounds2D64> {
    let local = match &snapshot.geometry {
        GeometryRef::Circle { radius } => Bounds2D64 {
            min_x: -f64::from(*radius),
            min_y: -f64::from(*radius),
            max_x: f64::from(*radius),
            max_y: f64::from(*radius),
        },
        GeometryRef::Rectangle { size } => Bounds2D64 {
            min_x: -f64::from(size.x) * 0.5,
            min_y: -f64::from(size.y) * 0.5,
            max_x: f64::from(size.x) * 0.5,
            max_y: f64::from(size.y) * 0.5,
        },
        GeometryRef::Line { start, end } => Bounds2D64 {
            min_x: f64::from(start.x.min(end.x)),
            min_y: f64::from(start.y.min(end.y)),
            max_x: f64::from(start.x.max(end.x)),
            max_y: f64::from(start.y.max(end.y)),
        },
        GeometryRef::VectorPath(path) => semantic_path_bounds(path, 0.0).layout?,
        GeometryRef::External(_) => return None,
    };

    let sine = transform.rotation_z.sin();
    let cosine = transform.rotation_z.cos();
    let scale_x = transform.scale.x;
    let scale_y = transform.scale.y;
    let translation_x = transform.translation.x;
    let translation_y = transform.translation.y;
    let corners = [
        (local.min_x, local.min_y),
        (local.min_x, local.max_y),
        (local.max_x, local.min_y),
        (local.max_x, local.max_y),
    ];
    let mut world: Option<Bounds2D64> = None;
    for (x, y) in corners {
        let x = x * scale_x;
        let y = y * scale_y;
        let point_x = x * cosine - y * sine + translation_x;
        let point_y = x * sine + y * cosine + translation_y;
        if let Some(bounds) = &mut world {
            bounds.include(point_x, point_y);
        } else {
            world = Some(Bounds2D64::point(point_x, point_y));
        }
    }
    world
}
'''
new_bounds = '''fn include_layout_point(bounds: &mut Option<Bounds2D64>, point: (f64, f64)) {
    if let Some(bounds) = bounds {
        bounds.include(point.0, point.1);
    } else {
        *bounds = Some(Bounds2D64::point(point.0, point.1));
    }
}

fn transform_layout_point(
    transform: SemanticTransform2_5D,
    point: Vec2,
) -> (f64, f64) {
    let x = f64::from(point.x) * transform.scale.x;
    let y = f64::from(point.y) * transform.scale.y;
    let sine = transform.rotation_z.sin();
    let cosine = transform.rotation_z.cos();
    (
        x * cosine - y * sine + transform.translation.x,
        x * sine + y * cosine + transform.translation.y,
    )
}

fn quadratic_layout_point(
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    t: f64,
) -> (f64, f64) {
    let u = 1.0 - t;
    (
        u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0,
        u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1,
    )
}

fn cubic_layout_point(
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
    t: f64,
) -> (f64, f64) {
    let u = 1.0 - t;
    (
        u * u * u * p0.0
            + 3.0 * u * u * t * p1.0
            + 3.0 * u * t * t * p2.0
            + t * t * t * p3.0,
        u * u * u * p0.1
            + 3.0 * u * u * t * p1.1
            + 3.0 * u * t * t * p2.1
            + t * t * t * p3.1,
    )
}

fn cubic_layout_derivative_roots(p0: f64, p1: f64, p2: f64, p3: f64) -> Vec<f64> {
    let a = -p0 + 3.0 * p1 - 3.0 * p2 + p3;
    let b = 2.0 * (p0 - 2.0 * p1 + p2);
    let c = p1 - p0;
    let epsilon = 1.0e-14;
    if a.abs() <= epsilon {
        if b.abs() <= epsilon {
            return Vec::new();
        }
        return vec![-c / b];
    }
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return Vec::new();
    }
    let root = discriminant.sqrt();
    let mut roots = vec![(-b + root) / (2.0 * a)];
    if root > epsilon {
        roots.push((-b - root) / (2.0 * a));
    }
    roots
}

fn transformed_path_layout_bounds(
    path: &VectorPath,
    transform: SemanticTransform2_5D,
) -> Option<Bounds2D64> {
    let mut bounds = None;
    let mut current = None;
    let mut subpath_start = None;

    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                let point = transform_layout_point(transform, to);
                include_layout_point(&mut bounds, point);
                current = Some(point);
                subpath_start = Some(point);
            }
            PathCommand::LineTo { to } => {
                let end = transform_layout_point(transform, to);
                if let Some(start) = current {
                    include_layout_point(&mut bounds, start);
                }
                include_layout_point(&mut bounds, end);
                current = Some(end);
            }
            PathCommand::QuadraticTo { control, to } => {
                let end = transform_layout_point(transform, to);
                let Some(start) = current else {
                    include_layout_point(&mut bounds, end);
                    current = Some(end);
                    continue;
                };
                let control = transform_layout_point(transform, control);
                include_layout_point(&mut bounds, start);
                include_layout_point(&mut bounds, end);
                for axis in 0..2 {
                    let (p0, p1, p2) = if axis == 0 {
                        (start.0, control.0, end.0)
                    } else {
                        (start.1, control.1, end.1)
                    };
                    let denominator = p0 - 2.0 * p1 + p2;
                    if denominator.abs() <= 1.0e-14 {
                        continue;
                    }
                    let t = (p0 - p1) / denominator;
                    if (0.0..1.0).contains(&t) {
                        include_layout_point(
                            &mut bounds,
                            quadratic_layout_point(start, control, end, t),
                        );
                    }
                }
                current = Some(end);
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                let end = transform_layout_point(transform, to);
                let Some(start) = current else {
                    include_layout_point(&mut bounds, end);
                    current = Some(end);
                    continue;
                };
                let control1 = transform_layout_point(transform, control1);
                let control2 = transform_layout_point(transform, control2);
                include_layout_point(&mut bounds, start);
                include_layout_point(&mut bounds, end);
                let mut roots = cubic_layout_derivative_roots(
                    start.0, control1.0, control2.0, end.0,
                );
                roots.extend(cubic_layout_derivative_roots(
                    start.1, control1.1, control2.1, end.1,
                ));
                for t in roots {
                    if (0.0..1.0).contains(&t) {
                        include_layout_point(
                            &mut bounds,
                            cubic_layout_point(start, control1, control2, end, t),
                        );
                    }
                }
                current = Some(end);
            }
            PathCommand::Close => {
                if let Some(end) = current {
                    include_layout_point(&mut bounds, end);
                }
                if let Some(start) = subpath_start {
                    include_layout_point(&mut bounds, start);
                    current = Some(start);
                }
            }
        }
    }
    bounds
}

fn snapshot_layout_bounds(
    snapshot: &ObjectSnapshot,
    transform: SemanticTransform2_5D,
) -> Option<Bounds2D64> {
    match &snapshot.geometry {
        GeometryRef::Circle { radius } => {
            let radius = f64::from(*radius);
            let sine = transform.rotation_z.sin();
            let cosine = transform.rotation_z.cos();
            let half_width = radius
                * (transform.scale.x * cosine).hypot(transform.scale.y * sine);
            let half_height = radius
                * (transform.scale.x * sine).hypot(transform.scale.y * cosine);
            Some(Bounds2D64 {
                min_x: transform.translation.x - half_width,
                min_y: transform.translation.y - half_height,
                max_x: transform.translation.x + half_width,
                max_y: transform.translation.y + half_height,
            })
        }
        GeometryRef::Rectangle { size } => {
            let half_x = f64::from(size.x) * 0.5;
            let half_y = f64::from(size.y) * 0.5;
            let mut bounds = None;
            for (x, y) in [
                (-half_x, -half_y),
                (-half_x, half_y),
                (half_x, -half_y),
                (half_x, half_y),
            ] {
                let sine = transform.rotation_z.sin();
                let cosine = transform.rotation_z.cos();
                let x = x * transform.scale.x;
                let y = y * transform.scale.y;
                include_layout_point(
                    &mut bounds,
                    (
                        x * cosine - y * sine + transform.translation.x,
                        x * sine + y * cosine + transform.translation.y,
                    ),
                );
            }
            bounds
        }
        GeometryRef::Line { start, end } => {
            let mut bounds = None;
            include_layout_point(&mut bounds, transform_layout_point(transform, *start));
            include_layout_point(&mut bounds, transform_layout_point(transform, *end));
            bounds
        }
        GeometryRef::VectorPath(path) => transformed_path_layout_bounds(path, transform),
        GeometryRef::External(_) => None,
    }
}
'''
replace_once(path, old_bounds, new_bounds)

replace_once(
    path,
    '''    #[test]
    fn layout_operations_are_shared_and_deterministic() {
''',
    '''    #[test]
    fn transformed_layout_bounds_match_manim_world_extrema() {
        let mut ellipse =
            FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::circle(1.0)));
        ellipse.scale(2.0, 1.0).unwrap();
        ellipse.rotate(std::f64::consts::FRAC_PI_4).unwrap();
        assert!((ellipse.width() - 10.0_f64.sqrt()).abs() < 1e-12);
        assert!((ellipse.height() - 10.0_f64.sqrt()).abs() < 1e-12);

        let mut diagonal = FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::line(
            Vec2::ZERO,
            Vec2::new(1.0, 1.0),
        )));
        diagonal.rotate(std::f64::consts::FRAC_PI_4).unwrap();
        assert!(diagonal.width().abs() < 1e-12);
        assert!((diagonal.height() - 2.0_f64.sqrt()).abs() < 1e-12);

        let path = VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.0))
            .quadratic_to(Vec2::new(0.0, 2.0), Vec2::new(1.0, 0.0));
        let mut curve = FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::path(path)));
        curve.rotate(std::f64::consts::FRAC_PI_4).unwrap();
        let expected = 9.0 * 2.0_f64.sqrt() / 8.0;
        assert!((curve.width() - expected).abs() < 1e-12);
        assert!((curve.height() - expected).abs() < 1e-12);
    }

    #[test]
    fn layout_operations_are_shared_and_deterministic() {
''',
)

path = "web/python/_manim_semantic_handles.py"
replace_once(
    path,
    '''def _layout_bounds(value: _base.Mobject) -> tuple[_base.Vec2, _base.Vec2] | None:
    """Use the one Manim compatibility bounds contract for every object owner.

    `_manim_phase_b` installs `_base._bounds` before this module is installed. It
    evaluates transformed quadratic/cubic extrema and analytic primitive extents in
    world space. Detached semantic handles remain authoritative for object state; we
    materialize only their current snapshot for this compatibility query instead of
    using the handle's older transformed-local-AABB shortcuts.
    """

    return _base._bounds(value._current_raw())


def _layout_center(value: _base.Mobject) -> _base.Vec2:
    raw = value._current_raw()
    bounds = _base._bounds(raw)
    if bounds is not None:
        return (bounds[0] + bounds[1]) * 0.5
    translation = raw.transform["translation"]
    return _base.Vec2(float(translation["x"]), float(translation["y"]))
''',
    '''def _layout_bounds(value: _base.Mobject) -> tuple[_base.Vec2, _base.Vec2] | None:
    """Read exact world-space layout bounds from a detached shared handle."""

    handle = _handle_for(value)
    if handle is None:
        return _base._bounds(value._current_raw())
    return (
        _base.Vec2(
            float(handle.criticalX(-1.0, 0.0)),
            float(handle.criticalY(0.0, -1.0)),
        ),
        _base.Vec2(
            float(handle.criticalX(1.0, 0.0)),
            float(handle.criticalY(0.0, 1.0)),
        ),
    )


def _layout_center(value: _base.Mobject) -> _base.Vec2:
    handle = _handle_for(value)
    if handle is None:
        raw = value._current_raw()
        bounds = _base._bounds(raw)
        if bounds is not None:
            return (bounds[0] + bounds[1]) * 0.5
        translation = raw.transform["translation"]
        return _base.Vec2(float(translation["x"]), float(translation["y"]))
    return _base.Vec2(float(handle.centerX), float(handle.centerY))
''',
)
replace_once(
    path,
    '''def _width(self: _base.Mobject) -> float:
    bounds = (
        _layout_bounds(self)
        if _handle_for(self) is not None
        else _base._bounds(self._current_raw())
    )
    return 0.0 if bounds is None else bounds[1].x - bounds[0].x


def _height(self: _base.Mobject) -> float:
    bounds = (
        _layout_bounds(self)
        if _handle_for(self) is not None
        else _base._bounds(self._current_raw())
    )
    return 0.0 if bounds is None else bounds[1].y - bounds[0].y
''',
    '''def _width(self: _base.Mobject) -> float:
    handle = _handle_for(self)
    if handle is not None:
        return float(handle.width)
    bounds = _base._bounds(self._current_raw())
    return 0.0 if bounds is None else bounds[1].x - bounds[0].x


def _height(self: _base.Mobject) -> float:
    handle = _handle_for(self)
    if handle is not None:
        return float(handle.height)
    bounds = _base._bounds(self._current_raw())
    return 0.0 if bounds is None else bounds[1].y - bounds[0].y
''',
)
replace_once(
    path,
    '''def _critical(value: _base.Mobject, direction: _base.Vec2) -> _base.Vec2:
    if _handle_for(value) is not None:
        bounds = _layout_bounds(value)
        if bounds is None:
            return _layout_center(value)
        minimum, maximum = bounds
        center = (minimum + maximum) * 0.5
        return _base.Vec2(
            minimum.x if direction.x < 0 else maximum.x if direction.x > 0 else center.x,
            minimum.y if direction.y < 0 else maximum.y if direction.y > 0 else center.y,
        )
    return _base._critical(value._current_raw(), direction)
''',
    '''def _critical(value: _base.Mobject, direction: _base.Vec2) -> _base.Vec2:
    handle = _handle_for(value)
    if handle is not None:
        return _base.Vec2(
            float(handle.criticalX(direction.x, direction.y)),
            float(handle.criticalY(direction.x, direction.y)),
        )
    return _base._critical(value._current_raw(), direction)
''',
)
replace_once(
    path,
    '''    point = _critical(self, direction)
    shift_x = 0.0
    shift_y = 0.0
    if direction.x != 0.0:
        target_x = direction.x.__class__(_base.DEFAULT_FRAME_WIDTH / 2.0)
        target_x = (1.0 if direction.x > 0.0 else -1.0) * float(target_x)
        shift_x = target_x - point.x - direction.x * float(buff)
    if direction.y != 0.0:
        target_y = direction.y.__class__(_base.DEFAULT_FRAME_HEIGHT / 2.0)
        target_y = (1.0 if direction.y > 0.0 else -1.0) * float(target_y)
        shift_y = target_y - point.y - direction.y * float(buff)
    handle.shift(shift_x, shift_y)
    return self
''',
    '''    handle.alignOnFrame(direction.x, direction.y, float(buff))
    return self
''',
)

path = "web/python/test_manim_semantic_handle_layout_bounds.py"
replace_once(
    path,
    '''            import _manim_semantic_handles as handles
''',
    '''            import _manim_semantic_handles as handles
            import noon as _base
''',
)
replace_once(
    path,
    '''            class FakeHandle:
                # Deliberately omit center/width/height/critical/nextTo/align shortcuts.
                # The adapter must obtain layout from the shared exact bounds contract.
                def __init__(self, snapshot_json):
                    self.snapshot = json.loads(snapshot_json)

                def snapshotJson(self):
                    return json.dumps(self.snapshot, separators=(",", ":"))

                def replaceSnapshotJson(self, snapshot_json):
                    self.snapshot = json.loads(snapshot_json)

                def cloneHandle(self):
                    return FakeHandle(self.snapshotJson())

                @property
                def centerX(self):
                    return float(self.snapshot["transform"]["translation"]["x"])

                @property
                def centerY(self):
                    return float(self.snapshot["transform"]["translation"]["y"])
''',
    '''            class FakeHandle:
                def __init__(self, snapshot_json):
                    self.snapshot = json.loads(snapshot_json)
                    self.snapshot_requests = 0

                def snapshotJson(self):
                    self.snapshot_requests += 1
                    return json.dumps(self.snapshot, separators=(",", ":"))

                def replaceSnapshotJson(self, snapshot_json):
                    self.snapshot = json.loads(snapshot_json)

                def cloneHandle(self):
                    return FakeHandle(json.dumps(self.snapshot, separators=(",", ":")))

                def _bounds(self):
                    raw = _base._ir.Mobject(
                        geometry=self.snapshot["geometry"],
                        transform=self.snapshot["transform"],
                        style=self.snapshot["style"],
                    )
                    return _base._bounds(raw)

                @property
                def centerX(self):
                    bounds = self._bounds()
                    return (bounds[0].x + bounds[1].x) * 0.5

                @property
                def centerY(self):
                    bounds = self._bounds()
                    return (bounds[0].y + bounds[1].y) * 0.5

                @property
                def width(self):
                    bounds = self._bounds()
                    return bounds[1].x - bounds[0].x

                @property
                def height(self):
                    bounds = self._bounds()
                    return bounds[1].y - bounds[0].y

                def criticalX(self, direction_x, direction_y):
                    bounds = self._bounds()
                    center = (bounds[0].x + bounds[1].x) * 0.5
                    return bounds[0].x if direction_x < 0 else bounds[1].x if direction_x > 0 else center

                def criticalY(self, direction_x, direction_y):
                    bounds = self._bounds()
                    center = (bounds[0].y + bounds[1].y) * 0.5
                    return bounds[0].y if direction_y < 0 else bounds[1].y if direction_y > 0 else center
''',
)
replace_once(
    path,
    '''                    self.snapshot["transform"]["rotation"] += float(angle)


            handles._create_handle = FakeHandle
''',
    '''                    self.snapshot["transform"]["rotation"] += float(angle)

                def alignOnFrame(self, direction_x, direction_y, buff):
                    point_x = self.criticalX(direction_x, direction_y)
                    point_y = self.criticalY(direction_x, direction_y)
                    shift_x = 0.0
                    shift_y = 0.0
                    if direction_x != 0.0:
                        target_x = math.copysign(_base.DEFAULT_FRAME_WIDTH * 0.5, direction_x)
                        shift_x = target_x - point_x - direction_x * float(buff)
                    if direction_y != 0.0:
                        target_y = math.copysign(_base.DEFAULT_FRAME_HEIGHT * 0.5, direction_y)
                        shift_y = target_y - point_y - direction_y * float(buff)
                    self.shift(shift_x, shift_y)


            handles._create_handle = FakeHandle
''',
)
replace_once(
    path,
    '''            ellipse = Circle(1.0).scale((2.0, 1.0)).rotate(PI / 4.0)
            expected_ellipse_extent = math.sqrt(10.0)
            assert abs(ellipse.width - expected_ellipse_extent) < 1e-12
            assert abs(ellipse.height - expected_ellipse_extent) < 1e-12
''',
    '''            ellipse = Circle(1.0).scale((2.0, 1.0)).rotate(PI / 4.0)
            expected_ellipse_extent = math.sqrt(10.0)
            ellipse._semantic_handle.snapshot_requests = 0
            assert abs(ellipse.width - expected_ellipse_extent) < 1e-12
            assert abs(ellipse.height - expected_ellipse_extent) < 1e-12
            assert abs(ellipse.get_center().x) < 1e-12
            assert abs(ellipse.get_critical_point(RIGHT).x - expected_ellipse_extent * 0.5) < 1e-12
            assert ellipse._semantic_handle.snapshot_requests == 0
''',
)
replace_once(
    path,
    '''            square = Square(1.0).next_to(curve, RIGHT, buff=0.5)
''',
    '''            curve._semantic_handle.snapshot_requests = 0
            square = Square(1.0).next_to(curve, RIGHT, buff=0.5)
''',
)
replace_once(
    path,
    '''            assert abs(gap - 0.5) < 1e-12

            moved = curve.copy().move_to((3.0, -2.0))
''',
    '''            assert abs(gap - 0.5) < 1e-12
            assert curve._semantic_handle.snapshot_requests == 0

            moved = curve.copy().move_to((3.0, -2.0))
''',
)

path = "compat/semantic-ownership-v1.json"
replace_once(
    path,
    '''    {
      "id": "mobject.layout-query",
      "surface": "Mobject.center/width/height/critical_point",
      "classification": "python-semantic-duplicate",
      "owner": {"language": "python", "path": "web/python/_manim_semantic_handles.py", "symbol": "_layout_bounds/_layout_center/_width/_height/_critical"},
      "shared_owner": {"language": "rust", "path": "crates/noon-web/src/authoring_mobject.rs", "symbol": "FrontendMobjectHandle::center/width/height/critical_point"},
      "reason": "Python materializes handle snapshots and evaluates the compatibility bounds contract locally instead of using the shared query result.",
      "replacement": "Finish #62 bounds integration and route these queries directly to shared semantic handles.",
      "migration_issue": "#61"
    },
''',
    '''    {
      "id": "mobject.layout-query",
      "surface": "Detached and animate-target Mobject.center/width/height/critical_point",
      "classification": "shared-rust",
      "owner": {"language": "rust", "path": "crates/noon-web/src/authoring_mobject.rs", "symbol": "FrontendMobjectHandle::center/width/height/critical_point"},
      "adapters": [{"language": "python", "path": "web/python/_manim_semantic_handles.py", "symbol": "_layout_bounds/_layout_center/_width/_height/_critical"}]
    },
    {
      "id": "mobject.scene-layout-query",
      "surface": "Scene-bound Mobject.center/width/height/critical_point",
      "classification": "python-semantic-duplicate",
      "owner": {"language": "python", "path": "web/python/_manim_semantic_handles.py", "symbol": "_layout_bounds/_layout_center/_width/_height/_critical fallbacks"},
      "shared_owner": {"language": "rust", "path": "crates/noon-web/src/authoring_mobject.rs", "symbol": "FrontendMobjectHandle::center/width/height/critical_point"},
      "reason": "Scene-bound wrappers do not yet retain a stable shared semantic handle, so layout queries still evaluate their scene snapshot in Python.",
      "replacement": "Bind scene-owned wrappers to stable shared handles and reuse the same Rust layout query path.",
      "migration_issue": "#61"
    },
''',
)
