use std::collections::{HashMap, VecDeque};

use noon_core::{Vec2, VectorPath};

use crate::{smooth_cubic_path_from_subpaths, PathSmoothingError};

/// ManimCE v0.21-compatible adaptive isoline configuration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IsolineOptions {
    pub min_depth: usize,
    pub max_quads: usize,
    pub tolerance: Option<[f64; 2]>,
}

impl Default for IsolineOptions {
    fn default() -> Self {
        Self {
            min_depth: 5,
            max_quads: 1_500,
            tolerance: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum IsolineError {
    InvalidBounds { min: [f64; 2], max: [f64; 2] },
    InvalidTolerance([f64; 2]),
    MinimumDepthOverflow(usize),
    CoordinateOverflow { x: f64, y: f64 },
    Smoothing(PathSmoothingError),
}

impl std::fmt::Display for IsolineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBounds { min, max } => write!(
                formatter,
                "isoline bounds must be finite and strictly increasing: min={min:?}, max={max:?}"
            ),
            Self::InvalidTolerance(tolerance) => write!(
                formatter,
                "isoline tolerance must contain two finite positive values: {tolerance:?}"
            ),
            Self::MinimumDepthOverflow(depth) => {
                write!(formatter, "isoline minimum depth {depth} exceeds the cell budget range")
            }
            Self::CoordinateOverflow { x, y } => write!(
                formatter,
                "isoline point cannot be represented as retained f32 coordinates: ({x}, {y})"
            ),
            Self::Smoothing(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for IsolineError {}

impl From<PathSmoothingError> for IsolineError {
    fn from(value: PathSmoothingError) -> Self {
        Self::Smoothing(value)
    }
}

/// Trace `field(x, y) = 0` using the adaptive 2D topology used by the
/// `isosurfaces` v0.1.1 dependency consumed by ManimCE v0.21.
///
/// Field values may be finite, infinite, or NaN. NaN has semantic meaning to
/// the subdivision policy: fully undefined cells stop, while a cell crossing
/// between defined and undefined samples is refined. Returned geometry itself
/// is always finite.
pub fn trace_isoline<F>(
    min: [f64; 2],
    max: [f64; 2],
    options: IsolineOptions,
    mut field: F,
) -> Result<Vec<Vec<Vec2>>, IsolineError>
where
    F: FnMut(f64, f64) -> f64,
{
    validate_bounds(min, max)?;
    let tolerance = resolve_tolerance(min, max, options.tolerance)?;
    let cells = build_tree(min, max, options.min_depth, options.max_quads, tolerance, &mut field)?;
    let triangles = Triangulator::new(&cells, &mut field, tolerance).triangulate();
    let curves = trace_curves(triangles);
    curves
        .into_iter()
        .map(|curve| curve.into_iter().map(Point2d::lower).collect())
        .collect()
}

/// Lower a deterministic isoline directly to ordinary retained `VectorPath`
/// geometry. Smoothing uses Noon's shared ManimCE v0.21 cubic-handle solver.
pub fn isoline_vector_path<F>(
    min: [f64; 2],
    max: [f64; 2],
    options: IsolineOptions,
    use_smoothing: bool,
    field: F,
) -> Result<VectorPath, IsolineError>
where
    F: FnMut(f64, f64) -> f64,
{
    let curves = trace_isoline(min, max, options, field)?;
    if use_smoothing {
        return Ok(smooth_cubic_path_from_subpaths(&curves)?);
    }

    let mut path = VectorPath::new();
    for curve in curves {
        let Some((&first, rest)) = curve.split_first() else {
            continue;
        };
        path = path.move_to(first);
        for &point in rest {
            path = path.line_to(point);
        }
    }
    Ok(path)
}

fn validate_bounds(min: [f64; 2], max: [f64; 2]) -> Result<(), IsolineError> {
    if min.into_iter().all(f64::is_finite)
        && max.into_iter().all(f64::is_finite)
        && min[0] < max[0]
        && min[1] < max[1]
    {
        Ok(())
    } else {
        Err(IsolineError::InvalidBounds { min, max })
    }
}

fn resolve_tolerance(
    min: [f64; 2],
    max: [f64; 2],
    requested: Option<[f64; 2]>,
) -> Result<[f64; 2], IsolineError> {
    let tolerance = requested.unwrap_or([(max[0] - min[0]) / 1000.0, (max[1] - min[1]) / 1000.0]);
    if tolerance.into_iter().all(|value| value.is_finite() && value > 0.0) {
        Ok(tolerance)
    } else {
        Err(IsolineError::InvalidTolerance(tolerance))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Point2d {
    x: f64,
    y: f64,
}

impl Point2d {
    const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    fn midpoint(self, other: Self) -> Self {
        Self::new((self.x + other.x) * 0.5, (self.y + other.y) * 0.5)
    }

    fn lerp(self, other: Self, alpha: f64) -> Self {
        Self::new(
            self.x * (1.0 - alpha) + other.x * alpha,
            self.y * (1.0 - alpha) + other.y * alpha,
        )
    }

    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }

    fn abs_difference(self, other: Self) -> Self {
        Self::new((self.x - other.x).abs(), (self.y - other.y).abs())
    }

    fn lower(self) -> Result<Vec2, IsolineError> {
        if !self.x.is_finite()
            || !self.y.is_finite()
            || self.x.abs() > f64::from(f32::MAX)
            || self.y.abs() > f64::from(f32::MAX)
        {
            return Err(IsolineError::CoordinateOverflow {
                x: self.x,
                y: self.y,
            });
        }
        Ok(Vec2::new(self.x as f32, self.y as f32))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ValuedPoint {
    position: Point2d,
    value: f64,
}

impl ValuedPoint {
    fn evaluate<F>(position: Point2d, field: &mut F) -> Self
    where
        F: FnMut(f64, f64) -> f64,
    {
        Self {
            position,
            value: field(position.x, position.y),
        }
    }

    fn midpoint<F>(first: Self, second: Self, field: &mut F) -> Self
    where
        F: FnMut(f64, f64) -> f64,
    {
        Self::evaluate(first.position.midpoint(second.position), field)
    }

    fn intersect_zero<F>(first: Self, second: Self, field: &mut F) -> Self
    where
        F: FnMut(f64, f64) -> f64,
    {
        let denominator = first.value - second.value;
        let first_weight = -second.value / denominator;
        let second_weight = first.value / denominator;
        let position = Point2d::new(
            first_weight * first.position.x + second_weight * second.position.x,
            first_weight * first.position.y + second_weight * second.position.y,
        );
        Self::evaluate(position, field)
    }
}

#[derive(Clone, Debug)]
struct Cell {
    vertices: [ValuedPoint; 4],
    depth: usize,
    children: Option<[usize; 4]>,
}

fn cell_vertices<F>(min: Point2d, max: Point2d, field: &mut F) -> [ValuedPoint; 4]
where
    F: FnMut(f64, f64) -> f64,
{
    [
        ValuedPoint::evaluate(min, field),
        ValuedPoint::evaluate(Point2d::new(max.x, min.y), field),
        ValuedPoint::evaluate(Point2d::new(min.x, max.y), field),
        ValuedPoint::evaluate(max, field),
    ]
}

fn build_tree<F>(
    min: [f64; 2],
    max: [f64; 2],
    min_depth: usize,
    max_quads: usize,
    tolerance: [f64; 2],
    field: &mut F,
) -> Result<Vec<Cell>, IsolineError>
where
    F: FnMut(f64, f64) -> f64,
{
    let minimum_required = 4usize
        .checked_pow(u32::try_from(min_depth).map_err(|_| IsolineError::MinimumDepthOverflow(min_depth))?)
        .ok_or(IsolineError::MinimumDepthOverflow(min_depth))?;
    let max_cells = max_quads.max(minimum_required);
    let min = Point2d::new(min[0], min[1]);
    let max = Point2d::new(max[0], max[1]);
    let root = Cell {
        vertices: cell_vertices(min, max, field),
        depth: 0,
        children: None,
    };
    let mut cells = vec![root];
    let mut queue = VecDeque::from([0usize]);
    let mut leaf_count = 1usize;

    while let Some(index) = queue.pop_front() {
        if leaf_count >= max_cells {
            break;
        }
        let should_descend = cells[index].depth < min_depth
            || should_descend_deep_cell(&cells[index], tolerance);
        if !should_descend {
            continue;
        }

        let parent_min = cells[index].vertices[0].position;
        let parent_max = cells[index].vertices[3].position;
        let parent_vertices = cells[index].vertices;
        let child_depth = cells[index].depth + 1;
        let first_child = cells.len();
        let mut child_indices = [0usize; 4];
        for (child_direction, vertex) in parent_vertices.into_iter().enumerate() {
            let child_min = parent_min.midpoint(vertex.position);
            let child_max = parent_max.midpoint(vertex.position);
            let child_index = cells.len();
            cells.push(Cell {
                vertices: cell_vertices(child_min, child_max, field),
                depth: child_depth,
                children: None,
            });
            child_indices[child_direction] = child_index;
        }
        debug_assert_eq!(child_indices, [first_child, first_child + 1, first_child + 2, first_child + 3]);
        cells[index].children = Some(child_indices);
        queue.extend(child_indices);
        leaf_count += 3;
    }
    Ok(cells)
}

fn should_descend_deep_cell(cell: &Cell, tolerance: [f64; 2]) -> bool {
    let size = cell.vertices[3]
        .position
        .abs_difference(cell.vertices[0].position);
    if size.x < 10.0 * tolerance[0] && size.y < 10.0 * tolerance[1] {
        return false;
    }

    let all_nan = cell.vertices.iter().all(|vertex| vertex.value.is_nan());
    if all_nan {
        return false;
    }
    if cell.vertices.iter().any(|vertex| vertex.value.is_nan()) {
        return true;
    }

    let first_sign = numpy_sign(cell.vertices[0].value);
    cell.vertices[1..]
        .iter()
        .any(|vertex| !same_numpy_sign(numpy_sign(vertex.value), first_sign))
}

fn numpy_sign(value: f64) -> f64 {
    if value.is_nan() {
        f64::NAN
    } else if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        0.0
    }
}

fn same_numpy_sign(first: f64, second: f64) -> bool {
    !first.is_nan() && !second.is_nan() && first == second
}

fn binary_search_zero<F>(
    mut positive_or_first: ValuedPoint,
    mut negative_or_second: ValuedPoint,
    field: &mut F,
    tolerance: [f64; 2],
) -> (ValuedPoint, bool)
where
    F: FnMut(f64, f64) -> f64,
{
    loop {
        let difference = negative_or_second
            .position
            .abs_difference(positive_or_first.position);
        if difference.x < tolerance[0] && difference.y < tolerance[1] {
            let point = ValuedPoint::intersect_zero(positive_or_first, negative_or_second, field);
            let monotone = same_numpy_sign(
                numpy_sign(point.value - positive_or_first.value),
                numpy_sign(negative_or_second.value - point.value),
            );
            // `isosurfaces` v0.1.1 spells this last check as
            // `np.abs(pt.val < 1e200)`, i.e. a boolean test rather than abs(value).
            let is_zero = point.value == 0.0 || (monotone && point.value < 1.0e200);
            return (point, is_zero);
        }

        let midpoint = ValuedPoint::midpoint(positive_or_first, negative_or_second, field);
        if midpoint.value == 0.0 {
            return (midpoint, true);
        }
        if (midpoint.value > 0.0) == (positive_or_first.value > 0.0) {
            positive_or_first = midpoint;
        } else {
            negative_or_second = midpoint;
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Triangle {
    vertices: [ValuedPoint; 3],
    next: Option<usize>,
    next_bisect_point: Option<ValuedPoint>,
    previous: Option<usize>,
    visited: bool,
}

impl Triangle {
    fn new(vertices: [ValuedPoint; 3]) -> Self {
        Self {
            vertices,
            next: None,
            next_bisect_point: None,
            previous: None,
            visited: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct HangingEdgeKey([u64; 2]);

struct Triangulator<'a, F> {
    cells: &'a [Cell],
    field: &'a mut F,
    tolerance: [f64; 2],
    triangles: Vec<Triangle>,
    hanging_next: HashMap<HangingEdgeKey, usize>,
}

impl<'a, F> Triangulator<'a, F>
where
    F: FnMut(f64, f64) -> f64,
{
    fn new(cells: &'a [Cell], field: &'a mut F, tolerance: [f64; 2]) -> Self {
        Self {
            cells,
            field,
            tolerance,
            triangles: Vec::new(),
            hanging_next: HashMap::new(),
        }
    }

    fn triangulate(mut self) -> Vec<Triangle> {
        self.triangulate_inside(0);
        self.triangles
    }

    fn triangulate_inside(&mut self, cell_index: usize) {
        let Some(children) = self.cells[cell_index].children else {
            return;
        };
        for child in children {
            self.triangulate_inside(child);
        }
        self.triangulate_crossing_row(children[0], children[1]);
        self.triangulate_crossing_row(children[2], children[3]);
        self.triangulate_crossing_col(children[0], children[2]);
        self.triangulate_crossing_col(children[1], children[3]);
    }

    fn triangulate_crossing_row(&mut self, left: usize, right: usize) {
        match (self.cells[left].children, self.cells[right].children) {
            (Some(left_children), Some(right_children)) => {
                self.triangulate_crossing_row(left_children[1], right_children[0]);
                self.triangulate_crossing_row(left_children[3], right_children[2]);
            }
            (Some(left_children), None) => {
                self.triangulate_crossing_row(left_children[1], right);
                self.triangulate_crossing_row(left_children[3], right);
            }
            (None, Some(right_children)) => {
                self.triangulate_crossing_row(left, right_children[0]);
                self.triangulate_crossing_row(left, right_children[2]);
            }
            (None, None) => self.add_row_connection(left, right),
        }
    }

    fn triangulate_crossing_col(&mut self, bottom: usize, top: usize) {
        match (self.cells[bottom].children, self.cells[top].children) {
            (Some(bottom_children), Some(top_children)) => {
                self.triangulate_crossing_col(bottom_children[2], top_children[0]);
                self.triangulate_crossing_col(bottom_children[3], top_children[1]);
            }
            (Some(bottom_children), None) => {
                self.triangulate_crossing_col(bottom_children[2], top);
                self.triangulate_crossing_col(bottom_children[3], top);
            }
            (None, Some(top_children)) => {
                self.triangulate_crossing_col(bottom, top_children[0]);
                self.triangulate_crossing_col(bottom, top_children[1]);
            }
            (None, None) => self.add_col_connection(bottom, top),
        }
    }

    fn add_row_connection(&mut self, left: usize, right: usize) {
        let left_cell = &self.cells[left];
        let right_cell = &self.cells[right];
        let left_dual = ValuedPoint::midpoint(left_cell.vertices[0], left_cell.vertices[3], self.field);
        let right_dual = ValuedPoint::midpoint(right_cell.vertices[0], right_cell.vertices[3], self.field);
        let four = if left_cell.depth < right_cell.depth {
            let edge = self.edge_dual(right_cell.vertices[2], right_cell.vertices[0]);
            four_triangles(right_cell.vertices[2], right_dual, right_cell.vertices[0], left_dual, edge)
        } else {
            let edge = self.edge_dual(left_cell.vertices[3], left_cell.vertices[1]);
            four_triangles(left_cell.vertices[3], right_dual, left_cell.vertices[1], left_dual, edge)
        };
        self.add_four_triangles(four);
    }

    fn add_col_connection(&mut self, bottom: usize, top: usize) {
        let bottom_cell = &self.cells[bottom];
        let top_cell = &self.cells[top];
        let bottom_dual = ValuedPoint::midpoint(bottom_cell.vertices[0], bottom_cell.vertices[3], self.field);
        let top_dual = ValuedPoint::midpoint(top_cell.vertices[0], top_cell.vertices[3], self.field);
        let four = if bottom_cell.depth < top_cell.depth {
            let edge = self.edge_dual(top_cell.vertices[0], top_cell.vertices[1]);
            four_triangles(top_cell.vertices[0], top_dual, top_cell.vertices[1], bottom_dual, edge)
        } else {
            let edge = self.edge_dual(bottom_cell.vertices[2], bottom_cell.vertices[3]);
            four_triangles(bottom_cell.vertices[2], top_dual, bottom_cell.vertices[3], bottom_dual, edge)
        };
        self.add_four_triangles(four);
    }

    fn edge_dual(&mut self, first: ValuedPoint, second: ValuedPoint) -> ValuedPoint {
        if (first.value > 0.0) != (second.value > 0.0) {
            return ValuedPoint::midpoint(first, second, self.field);
        }

        const DELTA: f64 = 0.01;
        let first_near = first.position.lerp(second.position, DELTA);
        let second_near = first.position.lerp(second.position, 1.0 - DELTA);
        let first_delta = (self.field)(first_near.x, first_near.y);
        let second_delta = (self.field)(second_near.x, second_near.y);
        if (first_delta > 0.0) == (second_delta > 0.0) {
            ValuedPoint::midpoint(first, second, self.field)
        } else {
            ValuedPoint::intersect_zero(
                ValuedPoint {
                    position: first.position,
                    value: first_delta,
                },
                ValuedPoint {
                    position: second.position,
                    value: second_delta,
                },
                self.field,
            )
        }
    }

    fn add_four_triangles(&mut self, vertices: [[ValuedPoint; 3]; 4]) {
        let base = self.triangles.len();
        for triangle in vertices {
            self.triangles.push(Triangle::new(triangle));
        }
        let indices = [base, base + 1, base + 2, base + 3];
        for index in 0..4 {
            self.next_sandwich_triangles(
                indices[index],
                indices[(index + 1) % 4],
                indices[(index + 2) % 4],
            );
        }
    }

    fn set_next(
        &mut self,
        first_triangle: usize,
        second_triangle: usize,
        positive: ValuedPoint,
        negative: ValuedPoint,
    ) {
        if !(positive.value > 0.0 && negative.value <= 0.0) {
            return;
        }
        let (intersection, is_zero) =
            binary_search_zero(positive, negative, self.field, self.tolerance);
        if !is_zero {
            return;
        }
        self.triangles[first_triangle].next_bisect_point = Some(intersection);
        self.triangles[first_triangle].next = Some(second_triangle);
        self.triangles[second_triangle].previous = Some(first_triangle);
    }

    fn next_sandwich_triangles(&mut self, first: usize, middle: usize, third: usize) {
        let center = self.triangles[middle].vertices[2];
        let x = self.triangles[middle].vertices[0];
        let y = self.triangles[middle].vertices[1];

        if center.value > 0.0 && y.value <= 0.0 {
            self.set_next(middle, third, center, y);
        }
        if x.value > 0.0 && center.value <= 0.0 {
            self.set_next(middle, first, x, center);
        }

        let sum = x.position.add(y.position);
        let key = HangingEdgeKey([sum.x.to_bits(), sum.y.to_bits()]);
        if y.value > 0.0 && x.value <= 0.0 {
            if let Some(other) = self.hanging_next.remove(&key) {
                self.set_next(middle, other, y, x);
            } else {
                self.hanging_next.insert(key, middle);
            }
        } else if y.value <= 0.0 && x.value > 0.0 {
            if let Some(other) = self.hanging_next.remove(&key) {
                self.set_next(other, middle, x, y);
            } else {
                self.hanging_next.insert(key, middle);
            }
        }
    }
}

fn four_triangles(
    first: ValuedPoint,
    second: ValuedPoint,
    third: ValuedPoint,
    fourth: ValuedPoint,
    center: ValuedPoint,
) -> [[ValuedPoint; 3]; 4] {
    [
        [first, second, center],
        [second, third, center],
        [third, fourth, center],
        [fourth, first, center],
    ]
}

fn trace_curves(mut triangles: Vec<Triangle>) -> Vec<Vec<Point2d>> {
    let mut curves = Vec::new();
    for start in 0..triangles.len() {
        if triangles[start].visited || triangles[start].next.is_none() {
            continue;
        }

        let mut triangle = start;
        let mut closed_loop = false;
        while let Some(previous) = triangles[triangle].previous {
            triangle = previous;
            if triangle == start {
                closed_loop = true;
                break;
            }
        }

        let mut curve = Vec::new();
        loop {
            if triangles[triangle].visited {
                break;
            }
            if let Some(point) = triangles[triangle].next_bisect_point {
                curve.push(point.position);
            }
            triangles[triangle].visited = true;
            let Some(next) = triangles[triangle].next else {
                break;
            };
            triangle = next;
        }
        if closed_loop && !curve.is_empty() {
            curve.push(curve[0]);
        }
        curves.push(curve);
    }
    curves
}

#[cfg(test)]
mod tests {
    use noon_core::PathCommand;

    use super::*;

    fn count_leaves(cells: &[Cell]) -> usize {
        cells.iter().filter(|cell| cell.children.is_none()).count()
    }

    #[test]
    fn minimum_depth_takes_precedence_over_requested_quad_budget() {
        let mut field = |x: f64, y: f64| x + y;
        let cells = build_tree([-1.0, -1.0], [1.0, 1.0], 2, 1, [0.002, 0.002], &mut field)
            .unwrap();
        assert_eq!(count_leaves(&cells), 16);
    }

    #[test]
    fn simple_vertical_zero_traces_one_deterministic_curve() {
        let options = IsolineOptions {
            min_depth: 3,
            max_quads: 64,
            tolerance: None,
        };
        let first = trace_isoline([-1.0, -1.0], [1.0, 1.0], options, |x, _| x).unwrap();
        let second = trace_isoline([-1.0, -1.0], [1.0, 1.0], options, |x, _| x).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 1);
        assert!(first[0].len() >= 2);
        assert!(first[0].iter().all(|point| point.x.abs() <= 1.0e-6));
    }

    #[test]
    fn circle_is_closed_and_default_smoothing_produces_cubics() {
        let options = IsolineOptions {
            min_depth: 4,
            max_quads: 512,
            tolerance: None,
        };
        let curves = trace_isoline([-1.5, -1.5], [1.5, 1.5], options, |x, y| {
            x * x + y * y - 1.0
        })
        .unwrap();
        assert_eq!(curves.len(), 1);
        let curve = &curves[0];
        assert!(curve.len() > 8);
        assert!((curve[0] - *curve.last().unwrap()).length() <= 1.0e-6);

        let path = isoline_vector_path(
            [-1.5, -1.5],
            [1.5, 1.5],
            options,
            true,
            |x, y| x * x + y * y - 1.0,
        )
        .unwrap();
        assert!(path
            .commands()
            .iter()
            .any(|command| matches!(command, PathCommand::CubicTo { .. })));
    }

    #[test]
    fn fully_nan_cell_stops_while_defined_boundary_descends() {
        let mut all_nan = |_x: f64, _y: f64| f64::NAN;
        let cells = build_tree([-1.0, -1.0], [1.0, 1.0], 0, 64, [0.002, 0.002], &mut all_nan)
            .unwrap();
        assert_eq!(count_leaves(&cells), 1);

        let mut mixed = |x: f64, _y: f64| if x < 0.0 { f64::NAN } else { x };
        let cells = build_tree([-1.0, -1.0], [1.0, 1.0], 0, 64, [0.002, 0.002], &mut mixed)
            .unwrap();
        assert!(count_leaves(&cells) > 1);
    }

    #[test]
    fn asymptote_is_not_reported_as_a_zero_curve() {
        let curves = trace_isoline(
            [-1.0, -1.0],
            [1.0, 1.0],
            IsolineOptions {
                min_depth: 4,
                max_quads: 256,
                tolerance: None,
            },
            |x, _| 1.0 / x,
        )
        .unwrap();
        assert!(curves.is_empty());
    }
}
