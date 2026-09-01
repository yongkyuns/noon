use std::collections::{HashMap, VecDeque};

use noon_core::{Vec2, VectorPath};
use noon_geometry::{smooth_cubic_path_from_subpaths, PathSmoothingError};

use crate::CoordinateSystemError;

type Point = [f64; 2];

/// Deterministic ManimCE v0.21-compatible 2D implicit contour authoring plan.
///
/// Manim delegates this surface to the MIT-licensed `isosurfaces` package, whose
/// isoline implementation uses an adaptive quadtree and a multiresolution
/// simplicial triangulation rather than uniform marching squares. This is a Rust
/// behavioral port of that 2D authoring algorithm: the caller supplies only the
/// scalar evaluator, while subdivision, dual placement, zero refinement, curve
/// tracing, smoothing, and retained path construction remain engine-owned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImplicitFunctionPlan {
    x_range: [f64; 2],
    y_range: [f64; 2],
    min_depth: usize,
    max_quads: usize,
    use_smoothing: bool,
}

impl ImplicitFunctionPlan {
    pub fn new(
        x_range: [f64; 2],
        y_range: [f64; 2],
        min_depth: usize,
        max_quads: usize,
        use_smoothing: bool,
    ) -> Result<Self, ImplicitFunctionAuthoringError> {
        validate_range("x", x_range)?;
        validate_range("y", y_range)?;
        let minimum_cells = 4_usize
            .checked_pow(
                min_depth
                    .try_into()
                    .map_err(|_| ImplicitFunctionAuthoringError::DepthOverflow(min_depth))?,
            )
            .ok_or(ImplicitFunctionAuthoringError::DepthOverflow(min_depth))?;
        let _ = minimum_cells.max(max_quads);
        let tolerance = [
            (x_range[1] - x_range[0]) / 1000.0,
            (y_range[1] - y_range[0]) / 1000.0,
        ];
        if tolerance
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0)
        {
            return Err(ImplicitFunctionAuthoringError::InvalidTolerance(tolerance));
        }
        Ok(Self {
            x_range,
            y_range,
            min_depth,
            max_quads,
            use_smoothing,
        })
    }

    pub const fn x_range(self) -> [f64; 2] {
        self.x_range
    }

    pub const fn y_range(self) -> [f64; 2] {
        self.y_range
    }

    pub const fn min_depth(self) -> usize {
        self.min_depth
    }

    pub const fn max_quads(self) -> usize {
        self.max_quads
    }

    pub const fn use_smoothing(self) -> bool {
        self.use_smoothing
    }

    /// Extract logical 2D contour curves. NaN and infinity are intentionally
    /// preserved as scalar-field values because upstream uses them to distinguish
    /// undefined regions and reject asymptotes during zero refinement.
    pub fn curves_with_evaluator<F>(
        self,
        mut evaluator: F,
    ) -> Result<Vec<Vec<Point>>, ImplicitFunctionAuthoringError>
    where
        F: FnMut(f64, f64) -> Result<f64, ImplicitFunctionAuthoringError>,
    {
        let pmin = [self.x_range[0], self.y_range[0]];
        let pmax = [self.x_range[1], self.y_range[1]];
        let tolerance = [(pmax[0] - pmin[0]) / 1000.0, (pmax[1] - pmin[1]) / 1000.0];
        let cells = build_tree(
            &mut evaluator,
            pmin,
            pmax,
            self.min_depth,
            self.max_quads,
            tolerance,
        )?;
        let triangles = Triangulator::new(&cells, &mut evaluator, tolerance).triangulate()?;
        trace_curves(triangles)
    }

    /// Compile a direct scene-space `ImplicitFunction` into immutable retained path
    /// geometry. The evaluator is consumed during authoring and never survives in
    /// the returned resource.
    pub fn vector_path_with_evaluator<F>(
        self,
        evaluator: F,
    ) -> Result<VectorPath, ImplicitFunctionAuthoringError>
    where
        F: FnMut(f64, f64) -> Result<f64, ImplicitFunctionAuthoringError>,
    {
        let curves = self.curves_with_evaluator(evaluator)?;
        point_curves_to_vector_path(&curves, self.use_smoothing, |point| {
            checked_vec2(point[0], point[1])
        })
    }

    /// Compile the same logical contours through a coordinate-system mapper. This
    /// is the shared substrate for `Axes.plot_implicit_curve`, including current
    /// retained axis transforms.
    pub fn vector_path_with_evaluator_and_mapper<F, M>(
        self,
        evaluator: F,
        mut mapper: M,
    ) -> Result<VectorPath, ImplicitFunctionAuthoringError>
    where
        F: FnMut(f64, f64) -> Result<f64, ImplicitFunctionAuthoringError>,
        M: FnMut(f64, f64) -> Result<Vec2, CoordinateSystemError>,
    {
        let curves = self.curves_with_evaluator(evaluator)?;
        point_curves_to_vector_path(&curves, self.use_smoothing, |point| {
            Ok(mapper(point[0], point[1])?)
        })
    }
}

fn validate_range(
    axis: &'static str,
    range: [f64; 2],
) -> Result<(), ImplicitFunctionAuthoringError> {
    if !range[0].is_finite() || !range[1].is_finite() || range[1] <= range[0] {
        return Err(ImplicitFunctionAuthoringError::InvalidRange { axis, range });
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ValuedPoint {
    pos: Point,
    val: f64,
}

impl ValuedPoint {
    fn evaluated<F>(pos: Point, evaluator: &mut F) -> Result<Self, ImplicitFunctionAuthoringError>
    where
        F: FnMut(f64, f64) -> Result<f64, ImplicitFunctionAuthoringError>,
    {
        Ok(Self {
            pos,
            val: evaluator(pos[0], pos[1])?,
        })
    }

    fn midpoint<F>(
        first: Self,
        second: Self,
        evaluator: &mut F,
    ) -> Result<Self, ImplicitFunctionAuthoringError>
    where
        F: FnMut(f64, f64) -> Result<f64, ImplicitFunctionAuthoringError>,
    {
        Self::evaluated(point_midpoint(first.pos, second.pos), evaluator)
    }

    fn intersect_zero<F>(
        first: Self,
        second: Self,
        evaluator: &mut F,
    ) -> Result<Self, ImplicitFunctionAuthoringError>
    where
        F: FnMut(f64, f64) -> Result<f64, ImplicitFunctionAuthoringError>,
    {
        let denominator = first.val - second.val;
        let first_weight = -second.val / denominator;
        let second_weight = first.val / denominator;
        let pos = point_add(
            point_scale(first.pos, first_weight),
            point_scale(second.pos, second_weight),
        );
        Self::evaluated(pos, evaluator)
    }
}

#[derive(Clone, Debug)]
struct Cell {
    depth: usize,
    vertices: [ValuedPoint; 4],
    children: Option<[usize; 4]>,
}

fn build_tree<F>(
    evaluator: &mut F,
    pmin: Point,
    pmax: Point,
    min_depth: usize,
    max_quads: usize,
    tolerance: Point,
) -> Result<Vec<Cell>, ImplicitFunctionAuthoringError>
where
    F: FnMut(f64, f64) -> Result<f64, ImplicitFunctionAuthoringError>,
{
    let minimum_cells = 4_usize
        .checked_pow(
            min_depth
                .try_into()
                .map_err(|_| ImplicitFunctionAuthoringError::DepthOverflow(min_depth))?,
        )
        .ok_or(ImplicitFunctionAuthoringError::DepthOverflow(min_depth))?;
    let max_cells = minimum_cells.max(max_quads);
    let root_vertices = vertices_from_extremes(pmin, pmax, evaluator)?;
    let mut cells = vec![Cell {
        depth: 0,
        vertices: root_vertices,
        children: None,
    }];
    let mut queue = VecDeque::from([0_usize]);
    let mut leaf_count = 1_usize;

    while let Some(cell_id) = queue.pop_front() {
        if leaf_count >= max_cells {
            break;
        }
        let should_descend = cells[cell_id].depth < min_depth
            || should_descend_deep_cell(&cells[cell_id], tolerance);
        if !should_descend {
            continue;
        }

        let parent_vertices = cells[cell_id].vertices;
        let child_depth = cells[cell_id]
            .depth
            .checked_add(1)
            .ok_or(ImplicitFunctionAuthoringError::DepthOverflow(min_depth))?;
        let mut child_ids = [0_usize; 4];
        for (child_direction, vertex) in parent_vertices.iter().copied().enumerate() {
            let child_min = point_midpoint(parent_vertices[0].pos, vertex.pos);
            let child_max = point_midpoint(parent_vertices[3].pos, vertex.pos);
            let vertices = vertices_from_extremes(child_min, child_max, evaluator)?;
            let child_id = cells.len();
            cells.push(Cell {
                depth: child_depth,
                vertices,
                children: None,
            });
            child_ids[child_direction] = child_id;
            queue.push_back(child_id);
        }
        cells[cell_id].children = Some(child_ids);
        leaf_count = leaf_count
            .checked_add(3)
            .ok_or(ImplicitFunctionAuthoringError::CellBudgetOverflow)?;
    }
    Ok(cells)
}

fn vertices_from_extremes<F>(
    pmin: Point,
    pmax: Point,
    evaluator: &mut F,
) -> Result<[ValuedPoint; 4], ImplicitFunctionAuthoringError>
where
    F: FnMut(f64, f64) -> Result<f64, ImplicitFunctionAuthoringError>,
{
    let width = point_sub(pmax, pmin);
    let mut vertices = [
        ValuedPoint {
            pos: [0.0; 2],
            val: 0.0,
        },
        ValuedPoint {
            pos: [0.0; 2],
            val: 0.0,
        },
        ValuedPoint {
            pos: [0.0; 2],
            val: 0.0,
        },
        ValuedPoint {
            pos: [0.0; 2],
            val: 0.0,
        },
    ];
    for (index, slot) in vertices.iter_mut().enumerate() {
        let pos = [
            pmin[0] + f64::from((index & 1) as u8) * width[0],
            pmin[1] + f64::from(((index >> 1) & 1) as u8) * width[1],
        ];
        *slot = ValuedPoint::evaluated(pos, evaluator)?;
    }
    Ok(vertices)
}

fn should_descend_deep_cell(cell: &Cell, tolerance: Point) -> bool {
    let size = point_sub(cell.vertices[3].pos, cell.vertices[0].pos);
    if size[0] < 10.0 * tolerance[0] && size[1] < 10.0 * tolerance[1] {
        return false;
    }
    if cell.vertices.iter().all(|vertex| vertex.val.is_nan()) {
        return false;
    }
    if cell.vertices.iter().any(|vertex| vertex.val.is_nan()) {
        return true;
    }
    let reference = numpy_sign(cell.vertices[0].val);
    cell.vertices[1..]
        .iter()
        .any(|vertex| numpy_sign(vertex.val) != reference)
}

#[derive(Clone, Debug)]
struct Triangle {
    vertices: [ValuedPoint; 3],
    next: Option<usize>,
    next_bisect_point: Option<ValuedPoint>,
    prev: Option<usize>,
    visited: bool,
}

impl Triangle {
    const fn new(vertices: [ValuedPoint; 3]) -> Self {
        Self {
            vertices,
            next: None,
            next_bisect_point: None,
            prev: None,
            visited: false,
        }
    }
}

struct Triangulator<'a, F> {
    cells: &'a [Cell],
    evaluator: &'a mut F,
    tolerance: Point,
    triangles: Vec<Triangle>,
    hanging_next: HashMap<[u64; 2], usize>,
}

impl<'a, F> Triangulator<'a, F>
where
    F: FnMut(f64, f64) -> Result<f64, ImplicitFunctionAuthoringError>,
{
    fn new(cells: &'a [Cell], evaluator: &'a mut F, tolerance: Point) -> Self {
        Self {
            cells,
            evaluator,
            tolerance,
            triangles: Vec::new(),
            hanging_next: HashMap::new(),
        }
    }

    fn triangulate(mut self) -> Result<Vec<Triangle>, ImplicitFunctionAuthoringError> {
        self.triangulate_inside(0)?;
        Ok(self.triangles)
    }

    fn triangulate_inside(&mut self, cell_id: usize) -> Result<(), ImplicitFunctionAuthoringError> {
        let Some(children) = self.cells[cell_id].children else {
            return Ok(());
        };
        for child in children {
            self.triangulate_inside(child)?;
        }
        self.triangulate_crossing_row(children[0], children[1])?;
        self.triangulate_crossing_row(children[2], children[3])?;
        self.triangulate_crossing_col(children[0], children[2])?;
        self.triangulate_crossing_col(children[1], children[3])?;
        Ok(())
    }

    fn triangulate_crossing_row(
        &mut self,
        left: usize,
        right: usize,
    ) -> Result<(), ImplicitFunctionAuthoringError> {
        match (self.cells[left].children, self.cells[right].children) {
            (Some(a), Some(b)) => {
                self.triangulate_crossing_row(a[1], b[0])?;
                self.triangulate_crossing_row(a[3], b[2])?;
            }
            (Some(a), None) => {
                self.triangulate_crossing_row(a[1], right)?;
                self.triangulate_crossing_row(a[3], right)?;
            }
            (None, Some(b)) => {
                self.triangulate_crossing_row(left, b[0])?;
                self.triangulate_crossing_row(left, b[2])?;
            }
            (None, None) => {
                let face_left = self.face_dual(left)?;
                let face_right = self.face_dual(right)?;
                let left_cell = &self.cells[left];
                let right_cell = &self.cells[right];
                let (a, c, edge) = if left_cell.depth < right_cell.depth {
                    let a = right_cell.vertices[2];
                    let c = right_cell.vertices[0];
                    let edge = self.edge_dual(a, c)?;
                    (a, c, edge)
                } else {
                    let a = left_cell.vertices[3];
                    let c = left_cell.vertices[1];
                    let edge = self.edge_dual(a, c)?;
                    (a, c, edge)
                };
                self.add_four_triangles(four_triangles(a, face_right, c, face_left, edge))?;
            }
        }
        Ok(())
    }

    fn triangulate_crossing_col(
        &mut self,
        bottom: usize,
        top: usize,
    ) -> Result<(), ImplicitFunctionAuthoringError> {
        match (self.cells[bottom].children, self.cells[top].children) {
            (Some(a), Some(b)) => {
                self.triangulate_crossing_col(a[2], b[0])?;
                self.triangulate_crossing_col(a[3], b[1])?;
            }
            (Some(a), None) => {
                self.triangulate_crossing_col(a[2], top)?;
                self.triangulate_crossing_col(a[3], top)?;
            }
            (None, Some(b)) => {
                self.triangulate_crossing_col(bottom, b[0])?;
                self.triangulate_crossing_col(bottom, b[1])?;
            }
            (None, None) => {
                let face_bottom = self.face_dual(bottom)?;
                let face_top = self.face_dual(top)?;
                let bottom_cell = &self.cells[bottom];
                let top_cell = &self.cells[top];
                let (a, c, edge) = if bottom_cell.depth < top_cell.depth {
                    let a = top_cell.vertices[0];
                    let c = top_cell.vertices[1];
                    let edge = self.edge_dual(a, c)?;
                    (a, c, edge)
                } else {
                    let a = bottom_cell.vertices[2];
                    let c = bottom_cell.vertices[3];
                    let edge = self.edge_dual(a, c)?;
                    (a, c, edge)
                };
                self.add_four_triangles(four_triangles(a, face_top, c, face_bottom, edge))?;
            }
        }
        Ok(())
    }

    fn face_dual(&mut self, cell_id: usize) -> Result<ValuedPoint, ImplicitFunctionAuthoringError> {
        let vertices = self.cells[cell_id].vertices;
        ValuedPoint::midpoint(vertices[0], vertices[3], self.evaluator)
    }

    fn edge_dual(
        &mut self,
        first: ValuedPoint,
        second: ValuedPoint,
    ) -> Result<ValuedPoint, ImplicitFunctionAuthoringError> {
        if (first.val > 0.0) != (second.val > 0.0) {
            return ValuedPoint::midpoint(first, second, self.evaluator);
        }

        const DT: f64 = 0.01;
        let near_first = point_add(
            point_scale(first.pos, 1.0 - DT),
            point_scale(second.pos, DT),
        );
        let near_second = point_add(
            point_scale(first.pos, DT),
            point_scale(second.pos, 1.0 - DT),
        );
        let first_delta = (self.evaluator)(near_first[0], near_first[1])?;
        let second_delta = (self.evaluator)(near_second[0], near_second[1])?;
        if (first_delta > 0.0) == (second_delta > 0.0) {
            ValuedPoint::midpoint(first, second, self.evaluator)
        } else {
            ValuedPoint::intersect_zero(
                ValuedPoint {
                    pos: first.pos,
                    val: first_delta,
                },
                ValuedPoint {
                    pos: second.pos,
                    val: second_delta,
                },
                self.evaluator,
            )
        }
    }

    fn add_four_triangles(
        &mut self,
        triangles: [[ValuedPoint; 3]; 4],
    ) -> Result<(), ImplicitFunctionAuthoringError> {
        let base = self.triangles.len();
        self.triangles
            .extend(triangles.into_iter().map(Triangle::new));
        for index in 0..4 {
            self.next_sandwich_triangles(
                base + index,
                base + (index + 1) % 4,
                base + (index + 2) % 4,
            )?;
        }
        Ok(())
    }

    fn next_sandwich_triangles(
        &mut self,
        previous: usize,
        current: usize,
        next: usize,
    ) -> Result<(), ImplicitFunctionAuthoringError> {
        let vertices = self.triangles[current].vertices;
        let center = vertices[2];
        let x = vertices[0];
        let y = vertices[1];

        if center.val > 0.0 && y.val <= 0.0 {
            self.set_next(current, next, center, y)?;
        }
        if x.val > 0.0 && center.val <= 0.0 {
            self.set_next(current, previous, x, center)?;
        }

        let key = point_sum_key(x.pos, y.pos);
        if y.val > 0.0 && x.val <= 0.0 {
            if let Some(waiting) = self.hanging_next.remove(&key) {
                self.set_next(current, waiting, y, x)?;
            } else {
                self.hanging_next.insert(key, current);
            }
        } else if y.val <= 0.0 && x.val > 0.0 {
            if let Some(waiting) = self.hanging_next.remove(&key) {
                self.set_next(waiting, current, x, y)?;
            } else {
                self.hanging_next.insert(key, current);
            }
        }
        Ok(())
    }

    fn set_next(
        &mut self,
        first_triangle: usize,
        second_triangle: usize,
        positive: ValuedPoint,
        negative: ValuedPoint,
    ) -> Result<(), ImplicitFunctionAuthoringError> {
        if !(positive.val > 0.0 && negative.val <= 0.0) {
            return Ok(());
        }
        let (intersection, is_zero) =
            binary_search_zero(positive, negative, self.evaluator, self.tolerance)?;
        if !is_zero {
            return Ok(());
        }
        self.triangles[first_triangle].next_bisect_point = Some(intersection);
        self.triangles[first_triangle].next = Some(second_triangle);
        self.triangles[second_triangle].prev = Some(first_triangle);
        Ok(())
    }
}

fn four_triangles(
    a: ValuedPoint,
    b: ValuedPoint,
    c: ValuedPoint,
    d: ValuedPoint,
    center: ValuedPoint,
) -> [[ValuedPoint; 3]; 4] {
    [
        [a, b, center],
        [b, c, center],
        [c, d, center],
        [d, a, center],
    ]
}

fn binary_search_zero<F>(
    mut positive: ValuedPoint,
    mut negative: ValuedPoint,
    evaluator: &mut F,
    tolerance: Point,
) -> Result<(ValuedPoint, bool), ImplicitFunctionAuthoringError>
where
    F: FnMut(f64, f64) -> Result<f64, ImplicitFunctionAuthoringError>,
{
    loop {
        let delta = point_sub(negative.pos, positive.pos);
        if delta[0].abs() < tolerance[0] && delta[1].abs() < tolerance[1] {
            let point = ValuedPoint::intersect_zero(positive, negative, evaluator)?;
            // Preserve the upstream asymptote guard exactly, including its boolean
            // comparison behavior (`np.abs(pt.val < 1e200)`).
            let monotonic = same_numpy_sign(point.val - positive.val, negative.val - point.val);
            let is_zero = point.val == 0.0 || (monotonic && point.val < 1.0e200);
            return Ok((point, is_zero));
        }

        let midpoint = ValuedPoint::midpoint(positive, negative, evaluator)?;
        if midpoint.val == 0.0 {
            return Ok((midpoint, true));
        }
        if (midpoint.val > 0.0) == (positive.val > 0.0) {
            positive = midpoint;
        } else {
            negative = midpoint;
        }
    }
}

fn trace_curves(
    mut triangles: Vec<Triangle>,
) -> Result<Vec<Vec<Point>>, ImplicitFunctionAuthoringError> {
    let mut curves = Vec::new();
    for start in 0..triangles.len() {
        if triangles[start].visited || triangles[start].next.is_none() {
            continue;
        }

        let mut triangle = start;
        let mut closed_loop = false;
        let mut backwards_steps = 0_usize;
        while let Some(previous) = triangles[triangle].prev {
            triangle = previous;
            backwards_steps += 1;
            if triangle == start {
                closed_loop = true;
                break;
            }
            if backwards_steps > triangles.len() {
                return Err(ImplicitFunctionAuthoringError::TopologyCycle);
            }
        }

        let mut active = Vec::new();
        let mut forward_steps = 0_usize;
        loop {
            if triangles[triangle].visited {
                break;
            }
            if let Some(point) = triangles[triangle].next_bisect_point {
                active.push(point.pos);
            }
            triangles[triangle].visited = true;
            forward_steps += 1;
            if forward_steps > triangles.len() {
                return Err(ImplicitFunctionAuthoringError::TopologyCycle);
            }
            let Some(next) = triangles[triangle].next else {
                break;
            };
            triangle = next;
        }
        if closed_loop {
            if let Some(first) = active.first().copied() {
                active.push(first);
            }
        }
        curves.push(active);
    }
    Ok(curves)
}

fn point_curves_to_vector_path<M>(
    curves: &[Vec<Point>],
    use_smoothing: bool,
    mut mapper: M,
) -> Result<VectorPath, ImplicitFunctionAuthoringError>
where
    M: FnMut(Point) -> Result<Vec2, ImplicitFunctionAuthoringError>,
{
    let mut subpaths = Vec::with_capacity(curves.len());
    for curve in curves.iter().filter(|curve| !curve.is_empty()) {
        let mut points = Vec::with_capacity(curve.len());
        for &point in curve {
            let mapped = mapper(point)?;
            if !mapped.x.is_finite() || !mapped.y.is_finite() {
                return Err(ImplicitFunctionAuthoringError::NonFiniteContourPoint(point));
            }
            points.push(mapped);
        }
        subpaths.push(points);
    }

    if use_smoothing {
        Ok(smooth_cubic_path_from_subpaths(&subpaths)?)
    } else {
        let mut path = VectorPath::new();
        for points in &subpaths {
            let Some((&first, rest)) = points.split_first() else {
                continue;
            };
            path = path.move_to(first);
            for &point in rest {
                path = path.line_to(point);
            }
        }
        Ok(path)
    }
}

fn checked_vec2(x: f64, y: f64) -> Result<Vec2, ImplicitFunctionAuthoringError> {
    let point = Vec2::new(x as f32, y as f32);
    if point.x.is_finite() && point.y.is_finite() {
        Ok(point)
    } else {
        Err(ImplicitFunctionAuthoringError::NonFiniteContourPoint([
            x, y,
        ]))
    }
}

fn numpy_sign(value: f64) -> Option<i8> {
    if value.is_nan() {
        None
    } else if value > 0.0 {
        Some(1)
    } else if value < 0.0 {
        Some(-1)
    } else {
        Some(0)
    }
}

fn same_numpy_sign(first: f64, second: f64) -> bool {
    match (numpy_sign(first), numpy_sign(second)) {
        (Some(first), Some(second)) => first == second,
        _ => false,
    }
}

fn point_midpoint(first: Point, second: Point) -> Point {
    [(first[0] + second[0]) * 0.5, (first[1] + second[1]) * 0.5]
}

fn point_add(first: Point, second: Point) -> Point {
    [first[0] + second[0], first[1] + second[1]]
}

fn point_sub(first: Point, second: Point) -> Point {
    [first[0] - second[0], first[1] - second[1]]
}

fn point_scale(point: Point, scalar: f64) -> Point {
    [point[0] * scalar, point[1] * scalar]
}

fn point_sum_key(first: Point, second: Point) -> [u64; 2] {
    [
        (first[0] + second[0]).to_bits(),
        (first[1] + second[1]).to_bits(),
    ]
}

#[derive(Clone, Debug, PartialEq)]
pub enum ImplicitFunctionAuthoringError {
    InvalidRange { axis: &'static str, range: [f64; 2] },
    InvalidTolerance(Point),
    DepthOverflow(usize),
    CellBudgetOverflow,
    Callback(String),
    NonFiniteContourPoint(Point),
    TopologyCycle,
    Coordinates(CoordinateSystemError),
    Smoothing(PathSmoothingError),
}

impl std::fmt::Display for ImplicitFunctionAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRange { axis, range } => write!(
                formatter,
                "ImplicitFunction {axis}_range must contain finite increasing bounds, got {range:?}"
            ),
            Self::InvalidTolerance(tolerance) => write!(
                formatter,
                "ImplicitFunction contour tolerance must be finite and positive, got {tolerance:?}"
            ),
            Self::DepthOverflow(depth) => {
                write!(
                    formatter,
                    "ImplicitFunction min_depth is too large: {depth}"
                )
            }
            Self::CellBudgetOverflow => {
                formatter.write_str("ImplicitFunction quadtree cell budget overflow")
            }
            Self::Callback(error) => write!(formatter, "ImplicitFunction callback failed: {error}"),
            Self::NonFiniteContourPoint(point) => write!(
                formatter,
                "ImplicitFunction produced a non-finite contour point: {point:?}"
            ),
            Self::TopologyCycle => {
                formatter.write_str("ImplicitFunction contour topology contains an invalid cycle")
            }
            Self::Coordinates(error) => error.fmt(formatter),
            Self::Smoothing(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ImplicitFunctionAuthoringError {}

impl From<CoordinateSystemError> for ImplicitFunctionAuthoringError {
    fn from(value: CoordinateSystemError) -> Self {
        Self::Coordinates(value)
    }
}

impl From<PathSmoothingError> for ImplicitFunctionAuthoringError {
    fn from(value: PathSmoothingError) -> Self {
        Self::Smoothing(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::PathCommand;

    fn evaluate<F>(
        function: F,
    ) -> impl FnMut(f64, f64) -> Result<f64, ImplicitFunctionAuthoringError>
    where
        F: Fn(f64, f64) -> f64,
    {
        move |x, y| Ok(function(x, y))
    }

    #[test]
    fn closed_circle_contour_is_traced_as_a_closed_subpath() {
        let plan = ImplicitFunctionPlan::new([-2.0, 2.0], [-2.0, 2.0], 4, 1500, false).unwrap();
        let curves = plan
            .curves_with_evaluator(evaluate(|x, y| x * x + y * y - 1.0))
            .unwrap();
        assert_eq!(curves.len(), 1);
        assert!(curves[0].len() > 8);
        assert_eq!(curves[0].first(), curves[0].last());
        for point in &curves[0] {
            assert!((point[0] * point[0] + point[1] * point[1] - 1.0).abs() < 0.02);
        }
    }

    #[test]
    fn smoothing_reuses_the_shared_cubic_path_substrate() {
        let plan = ImplicitFunctionPlan::new([-2.0, 2.0], [-2.0, 2.0], 3, 500, true).unwrap();
        let path = plan
            .vector_path_with_evaluator(evaluate(|x, y| x * x + y * y - 1.0))
            .unwrap();
        assert!(matches!(
            path.commands().first(),
            Some(PathCommand::MoveTo { .. })
        ));
        assert!(path
            .commands()
            .iter()
            .skip(1)
            .any(|command| matches!(command, PathCommand::CubicTo { .. })));
    }

    #[test]
    fn nan_only_regions_do_not_force_unbounded_subdivision() {
        let plan = ImplicitFunctionPlan::new([-1.0, 1.0], [-1.0, 1.0], 2, 200, false).unwrap();
        let curves = plan
            .curves_with_evaluator(evaluate(|_, _| f64::NAN))
            .unwrap();
        assert!(curves.is_empty());
    }

    #[test]
    fn minimum_depth_takes_precedence_over_smaller_quad_budget() {
        let plan = ImplicitFunctionPlan::new([-1.0, 1.0], [-1.0, 1.0], 3, 1, false).unwrap();
        let curves = plan.curves_with_evaluator(evaluate(|x, _| x)).unwrap();
        assert!(!curves.is_empty());
    }

    #[test]
    fn invalid_ranges_are_rejected_before_callback_evaluation() {
        assert!(matches!(
            ImplicitFunctionPlan::new([1.0, -1.0], [-1.0, 1.0], 5, 1500, true),
            Err(ImplicitFunctionAuthoringError::InvalidRange { axis: "x", .. })
        ));
    }
}
