use std::collections::{HashMap, VecDeque};

/// One point in the coordinate space sampled by the implicit-function planner.
///
/// The planner keeps f64 coordinates until the later retained-path conversion so
/// adaptive topology and zero refinement are not constrained by renderer precision.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct IsolinePoint {
    pub x: f64,
    pub y: f64,
}

impl IsolinePoint {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    fn midpoint(self, other: Self) -> Self {
        Self::new((self.x + other.x) * 0.5, (self.y + other.y) * 0.5)
    }

    fn lerp(self, other: Self, t: f64) -> Self {
        Self::new(
            self.x * (1.0 - t) + other.x * t,
            self.y * (1.0 - t) + other.y * t,
        )
    }
}

/// Axis-aligned coordinate-space domain for one isoline query.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IsolineBounds {
    pub min: IsolinePoint,
    pub max: IsolinePoint,
}

impl IsolineBounds {
    pub const fn new(min: IsolinePoint, max: IsolinePoint) -> Self {
        Self { min, max }
    }
}

/// Deterministic adaptive-contouring options.
///
/// Defaults follow ManimCE v0.21 `ImplicitFunction`: five mandatory quadtree
/// levels and a 1,500-quad budget. When `tolerance` is absent, each component is
/// one-thousandth of the corresponding domain extent, matching the pinned
/// `isosurfaces` oracle used by Manim.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IsolineOptions {
    pub min_depth: u32,
    pub max_quads: usize,
    pub tolerance: Option<IsolinePoint>,
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

/// Geometry-only output from adaptive isoline planning.
///
/// Curves are deterministic coordinate-space polylines. A closed contour repeats
/// its first point as its final point. Later B5 integration owns Axes mapping,
/// retained `VectorPath` construction and optional B2 smoothing.
#[derive(Clone, Debug, PartialEq)]
pub struct IsolinePlan {
    pub curves: Vec<Vec<IsolinePoint>>,
    pub leaf_quads: usize,
    pub sampled_points: usize,
    pub triangles: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IsolineError {
    InvalidBounds,
    InvalidTolerance,
    MinimumDepthBudgetOverflow { min_depth: u32 },
    LeafBudgetOverflow,
}

impl std::fmt::Display for IsolineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBounds => {
                formatter.write_str("isoline bounds must be finite and strictly increasing")
            }
            Self::InvalidTolerance => {
                formatter.write_str("isoline tolerance components must be finite and positive")
            }
            Self::MinimumDepthBudgetOverflow { min_depth } => write!(
                formatter,
                "isoline minimum depth {min_depth} exceeds the addressable quadtree budget"
            ),
            Self::LeafBudgetOverflow => {
                formatter.write_str("isoline adaptive leaf count overflowed")
            }
        }
    }
}

impl std::error::Error for IsolineError {}

/// Plan the zero contour of `function` over `bounds`.
///
/// This is deliberately renderer- and frontend-independent. Function evaluation
/// occurs only while this planner runs; the resulting curves contain no callback
/// or runtime authority.
pub fn plan_isoline<F>(
    function: F,
    bounds: IsolineBounds,
    options: IsolineOptions,
) -> Result<IsolinePlan, IsolineError>
where
    F: FnMut(IsolinePoint) -> f64,
{
    validate_bounds(bounds)?;
    let tolerance = effective_tolerance(bounds, options)?;
    let mandatory_leaves =
        4usize
            .checked_pow(options.min_depth)
            .ok_or(IsolineError::MinimumDepthBudgetOverflow {
                min_depth: options.min_depth,
            })?;
    let max_leaves = mandatory_leaves.max(options.max_quads);

    let mut sampler = Sampler::new(function);
    let (cells, leaf_quads) = build_tree(
        &mut sampler,
        bounds,
        options.min_depth,
        max_leaves,
        tolerance,
    )?;
    let mut triangulator = Triangulator::new(&cells, &mut sampler, tolerance);
    triangulator.triangulate();
    let triangles = triangulator.triangles.len();
    let curves = triangulator.trace();

    Ok(IsolinePlan {
        curves,
        leaf_quads,
        sampled_points: sampler.evaluations,
        triangles,
    })
}

fn validate_bounds(bounds: IsolineBounds) -> Result<(), IsolineError> {
    let values = [bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y];
    if values.iter().any(|value| !value.is_finite())
        || bounds.min.x >= bounds.max.x
        || bounds.min.y >= bounds.max.y
    {
        return Err(IsolineError::InvalidBounds);
    }
    Ok(())
}

fn effective_tolerance(
    bounds: IsolineBounds,
    options: IsolineOptions,
) -> Result<IsolinePoint, IsolineError> {
    let tolerance = options.tolerance.unwrap_or_else(|| {
        IsolinePoint::new(
            (bounds.max.x - bounds.min.x) / 1000.0,
            (bounds.max.y - bounds.min.y) / 1000.0,
        )
    });
    if !tolerance.x.is_finite()
        || !tolerance.y.is_finite()
        || tolerance.x <= 0.0
        || tolerance.y <= 0.0
    {
        return Err(IsolineError::InvalidTolerance);
    }
    Ok(tolerance)
}

#[derive(Clone, Copy, Debug)]
struct Sample {
    point: IsolinePoint,
    value: f64,
}

struct Sampler<F> {
    function: F,
    evaluations: usize,
}

impl<F> Sampler<F>
where
    F: FnMut(IsolinePoint) -> f64,
{
    fn new(function: F) -> Self {
        Self {
            function,
            evaluations: 0,
        }
    }

    fn sample(&mut self, point: IsolinePoint) -> Sample {
        self.evaluations = self.evaluations.saturating_add(1);
        Sample {
            point,
            value: (self.function)(point),
        }
    }

    fn midpoint(&mut self, first: Sample, second: Sample) -> Sample {
        self.sample(first.point.midpoint(second.point))
    }

    fn intersect_zero(&mut self, first: Sample, second: Sample) -> Sample {
        let denominator = first.value - second.value;
        let first_weight = -second.value / denominator;
        let second_weight = first.value / denominator;
        self.sample(IsolinePoint::new(
            first_weight * first.point.x + second_weight * second.point.x,
            first_weight * first.point.y + second_weight * second.point.y,
        ))
    }
}

#[derive(Clone, Debug)]
struct Cell {
    vertices: [Sample; 4],
    depth: u32,
    children: Option<[usize; 4]>,
}

fn build_tree<F>(
    sampler: &mut Sampler<F>,
    bounds: IsolineBounds,
    min_depth: u32,
    max_leaves: usize,
    tolerance: IsolinePoint,
) -> Result<(Vec<Cell>, usize), IsolineError>
where
    F: FnMut(IsolinePoint) -> f64,
{
    let root = make_cell(sampler, bounds.min, bounds.max, 0);
    let mut cells = vec![root];
    let mut queue = VecDeque::from([0usize]);
    let mut leaf_count = 1usize;

    while !queue.is_empty() && leaf_count < max_leaves {
        let cell_index = queue
            .pop_front()
            .expect("non-empty adaptive contour queue has a front cell");
        if cells[cell_index].depth < min_depth
            || should_descend_deep_cell(&cells[cell_index], tolerance)
        {
            let children = split_cell(sampler, &mut cells, cell_index);
            queue.extend(children);
            leaf_count = leaf_count
                .checked_add(3)
                .ok_or(IsolineError::LeafBudgetOverflow)?;
        }
    }

    Ok((cells, leaf_count))
}

fn make_cell<F>(sampler: &mut Sampler<F>, min: IsolinePoint, max: IsolinePoint, depth: u32) -> Cell
where
    F: FnMut(IsolinePoint) -> f64,
{
    Cell {
        vertices: [
            sampler.sample(min),
            sampler.sample(IsolinePoint::new(max.x, min.y)),
            sampler.sample(IsolinePoint::new(min.x, max.y)),
            sampler.sample(max),
        ],
        depth,
        children: None,
    }
}

fn split_cell<F>(sampler: &mut Sampler<F>, cells: &mut Vec<Cell>, cell_index: usize) -> [usize; 4]
where
    F: FnMut(IsolinePoint) -> f64,
{
    debug_assert!(cells[cell_index].children.is_none());
    let vertices = cells[cell_index].vertices;
    let depth = cells[cell_index].depth + 1;
    let lower = vertices[0].point;
    let upper = vertices[3].point;
    let mut children = [0usize; 4];

    for (direction, corner) in vertices.into_iter().enumerate() {
        let min = lower.midpoint(corner.point);
        let max = upper.midpoint(corner.point);
        children[direction] = cells.len();
        cells.push(make_cell(sampler, min, max, depth));
    }
    cells[cell_index].children = Some(children);
    children
}

fn should_descend_deep_cell(cell: &Cell, tolerance: IsolinePoint) -> bool {
    let lower = cell.vertices[0].point;
    let upper = cell.vertices[3].point;
    if upper.x - lower.x < 10.0 * tolerance.x && upper.y - lower.y < 10.0 * tolerance.y {
        return false;
    }

    if cell.vertices.iter().all(|vertex| vertex.value.is_nan()) {
        return false;
    }
    if cell.vertices.iter().any(|vertex| vertex.value.is_nan()) {
        return true;
    }

    let reference = numeric_sign(cell.vertices[0].value);
    cell.vertices[1..]
        .iter()
        .any(|vertex| numeric_sign(vertex.value) != reference)
}

fn numeric_sign(value: f64) -> i8 {
    if value > 0.0 {
        1
    } else if value < 0.0 {
        -1
    } else {
        0
    }
}

#[derive(Clone, Copy, Debug)]
struct Triangle {
    vertices: [Sample; 3],
    next: Option<usize>,
    next_point: Option<Sample>,
    previous: Option<usize>,
    visited: bool,
}

impl Triangle {
    fn new(vertices: [Sample; 3]) -> Self {
        Self {
            vertices,
            next: None,
            next_point: None,
            previous: None,
            visited: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct HangingEdgeKey {
    x: u64,
    y: u64,
}

impl HangingEdgeKey {
    fn from_endpoints(first: IsolinePoint, second: IsolinePoint) -> Self {
        Self {
            x: (first.x + second.x).to_bits(),
            y: (first.y + second.y).to_bits(),
        }
    }
}

struct Triangulator<'a, F> {
    cells: &'a [Cell],
    sampler: &'a mut Sampler<F>,
    tolerance: IsolinePoint,
    triangles: Vec<Triangle>,
    hanging_next: HashMap<HangingEdgeKey, usize>,
}

impl<'a, F> Triangulator<'a, F>
where
    F: FnMut(IsolinePoint) -> f64,
{
    fn new(cells: &'a [Cell], sampler: &'a mut Sampler<F>, tolerance: IsolinePoint) -> Self {
        Self {
            cells,
            sampler,
            tolerance,
            triangles: Vec::new(),
            hanging_next: HashMap::new(),
        }
    }

    fn triangulate(&mut self) {
        self.triangulate_inside(0);
    }

    fn triangulate_inside(&mut self, cell: usize) {
        let Some(children) = self.cells[cell].children else {
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
            (None, None) => {
                let left_face = self.face_dual(left);
                let right_face = self.face_dual(right);
                if self.cells[left].depth < self.cells[right].depth {
                    let vertices = self.cells[right].vertices;
                    let edge = self.edge_dual(vertices[2], vertices[0]);
                    self.add_four_triangles(four_triangles(
                        vertices[2],
                        right_face,
                        vertices[0],
                        left_face,
                        edge,
                    ));
                } else {
                    let vertices = self.cells[left].vertices;
                    let edge = self.edge_dual(vertices[3], vertices[1]);
                    self.add_four_triangles(four_triangles(
                        vertices[3],
                        right_face,
                        vertices[1],
                        left_face,
                        edge,
                    ));
                }
            }
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
            (None, None) => {
                let bottom_face = self.face_dual(bottom);
                let top_face = self.face_dual(top);
                if self.cells[bottom].depth < self.cells[top].depth {
                    let vertices = self.cells[top].vertices;
                    let edge = self.edge_dual(vertices[0], vertices[1]);
                    self.add_four_triangles(four_triangles(
                        vertices[0],
                        top_face,
                        vertices[1],
                        bottom_face,
                        edge,
                    ));
                } else {
                    let vertices = self.cells[bottom].vertices;
                    let edge = self.edge_dual(vertices[2], vertices[3]);
                    self.add_four_triangles(four_triangles(
                        vertices[2],
                        top_face,
                        vertices[3],
                        bottom_face,
                        edge,
                    ));
                }
            }
        }
    }

    fn face_dual(&mut self, cell: usize) -> Sample {
        let vertices = self.cells[cell].vertices;
        self.sampler.midpoint(vertices[0], vertices[3])
    }

    fn edge_dual(&mut self, first: Sample, second: Sample) -> Sample {
        if (first.value > 0.0) != (second.value > 0.0) {
            return self.sampler.midpoint(first, second);
        }

        let near_first = first.point.lerp(second.point, 0.01);
        let near_second = first.point.lerp(second.point, 0.99);
        let first_probe = self.sampler.sample(near_first).value;
        let second_probe = self.sampler.sample(near_second).value;
        if (first_probe > 0.0) == (second_probe > 0.0) {
            self.sampler.midpoint(first, second)
        } else {
            self.sampler.intersect_zero(
                Sample {
                    point: first.point,
                    value: first_probe,
                },
                Sample {
                    point: second.point,
                    value: second_probe,
                },
            )
        }
    }

    fn add_four_triangles(&mut self, triangles: [Triangle; 4]) {
        let base = self.triangles.len();
        self.triangles.extend(triangles);
        for index in 0..4 {
            self.next_sandwich(base + index, base + (index + 1) % 4, base + (index + 2) % 4);
        }
    }

    fn next_sandwich(&mut self, previous: usize, current: usize, next: usize) {
        let vertices = self.triangles[current].vertices;
        let x = vertices[0];
        let y = vertices[1];
        let center = vertices[2];

        if center.value > 0.0 && y.value <= 0.0 {
            self.set_next(current, next, center, y);
        }
        if x.value > 0.0 && center.value <= 0.0 {
            self.set_next(current, previous, x, center);
        }

        let key = HangingEdgeKey::from_endpoints(x.point, y.point);
        if y.value > 0.0 && x.value <= 0.0 {
            if let Some(waiting) = self.hanging_next.remove(&key) {
                self.set_next(current, waiting, y, x);
            } else {
                self.hanging_next.insert(key, current);
            }
        } else if y.value <= 0.0 && x.value > 0.0 {
            if let Some(waiting) = self.hanging_next.remove(&key) {
                self.set_next(waiting, current, x, y);
            } else {
                self.hanging_next.insert(key, current);
            }
        }
    }

    fn set_next(&mut self, from: usize, to: usize, positive: Sample, negative: Sample) {
        if !(positive.value > 0.0 && negative.value <= 0.0) {
            return;
        }
        let (intersection, valid) =
            binary_search_zero(self.sampler, positive, negative, self.tolerance);
        if !valid {
            return;
        }

        self.triangles[from].next_point = Some(intersection);
        self.triangles[from].next = Some(to);
        self.triangles[to].previous = Some(from);
    }

    fn trace(&mut self) -> Vec<Vec<IsolinePoint>> {
        let mut curves = Vec::new();
        for start in 0..self.triangles.len() {
            if self.triangles[start].visited || self.triangles[start].next.is_none() {
                continue;
            }
            let curve = self.trace_from(start);
            if !curve.is_empty() {
                curves.push(curve);
            }
        }
        curves
    }

    fn trace_from(&mut self, start: usize) -> Vec<IsolinePoint> {
        let mut current = start;
        let mut closed = false;

        for _ in 0..=self.triangles.len() {
            let Some(previous) = self.triangles[current].previous else {
                break;
            };
            current = previous;
            if current == start {
                closed = true;
                break;
            }
        }

        let mut curve = Vec::new();
        while !self.triangles[current].visited {
            if let Some(point) = self.triangles[current].next_point {
                curve.push(point.point);
            }
            self.triangles[current].visited = true;
            let Some(next) = self.triangles[current].next else {
                break;
            };
            current = next;
        }

        if closed && !curve.is_empty() {
            curve.push(curve[0]);
        }
        curve
    }
}

fn four_triangles(a: Sample, b: Sample, c: Sample, d: Sample, center: Sample) -> [Triangle; 4] {
    [
        Triangle::new([a, b, center]),
        Triangle::new([b, c, center]),
        Triangle::new([c, d, center]),
        Triangle::new([d, a, center]),
    ]
}

fn binary_search_zero<F>(
    sampler: &mut Sampler<F>,
    mut positive: Sample,
    mut negative: Sample,
    tolerance: IsolinePoint,
) -> (Sample, bool)
where
    F: FnMut(IsolinePoint) -> f64,
{
    for _ in 0..2048 {
        if (negative.point.x - positive.point.x).abs() < tolerance.x
            && (negative.point.y - positive.point.y).abs() < tolerance.y
        {
            let intersection = sampler.intersect_zero(positive, negative);
            let valid = intersection.value == 0.0
                || (same_numeric_sign(
                    intersection.value - positive.value,
                    negative.value - intersection.value,
                ) && intersection.value < 1e200);
            return (intersection, valid);
        }

        let midpoint = sampler.midpoint(positive, negative);
        if midpoint.value == 0.0 {
            return (midpoint, true);
        }
        if (midpoint.value > 0.0) == (positive.value > 0.0) {
            positive = midpoint;
        } else {
            negative = midpoint;
        }
    }

    let intersection = sampler.intersect_zero(positive, negative);
    (intersection, false)
}

fn same_numeric_sign(first: f64, second: f64) -> bool {
    if first.is_nan() || second.is_nan() {
        return false;
    }
    numeric_sign(first) == numeric_sign(second)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> IsolineBounds {
        IsolineBounds::new(IsolinePoint::new(-2.0, -2.0), IsolinePoint::new(2.0, 2.0))
    }

    #[test]
    fn minimum_depth_takes_precedence_over_smaller_budget() {
        let plan = plan_isoline(
            |_| 1.0,
            bounds(),
            IsolineOptions {
                min_depth: 2,
                max_quads: 1,
                tolerance: None,
            },
        )
        .unwrap();

        assert_eq!(plan.leaf_quads, 16);
        assert!(plan.curves.is_empty());
    }

    #[test]
    fn adaptive_budget_matches_breadth_first_leaf_overshoot() {
        let plan = plan_isoline(
            |point| point.x,
            IsolineBounds::new(IsolinePoint::new(-1.0, -1.0), IsolinePoint::new(1.0, 1.0)),
            IsolineOptions {
                min_depth: 1,
                max_quads: 8,
                tolerance: Some(IsolinePoint::new(1e-4, 1e-4)),
            },
        )
        .unwrap();

        // Four mandatory leaves, then two breadth-first splits add three leaves each.
        assert_eq!(plan.leaf_quads, 10);
    }

    #[test]
    fn undefined_regions_stop_only_after_defined_boundary_isolated() {
        let all_undefined = plan_isoline(
            |_| f64::NAN,
            bounds(),
            IsolineOptions {
                min_depth: 0,
                max_quads: 64,
                tolerance: Some(IsolinePoint::new(1e-3, 1e-3)),
            },
        )
        .unwrap();
        assert_eq!(all_undefined.leaf_quads, 1);
        assert!(all_undefined.curves.is_empty());

        let mixed = plan_isoline(
            |point| if point.x < 0.0 { f64::NAN } else { 1.0 },
            bounds(),
            IsolineOptions {
                min_depth: 0,
                max_quads: 64,
                tolerance: Some(IsolinePoint::new(1e-3, 1e-3)),
            },
        )
        .unwrap();
        assert!(mixed.leaf_quads > 1);
        assert!(mixed.sampled_points > all_undefined.sampled_points);
    }

    #[test]
    fn zero_refinement_rejects_vertical_asymptote() {
        let mut sampler = Sampler::new(|point: IsolinePoint| 1.0 / point.x);
        let positive = sampler.sample(IsolinePoint::new(1.0, 0.0));
        let negative = sampler.sample(IsolinePoint::new(-1.0, 0.0));
        let (_, valid) = binary_search_zero(
            &mut sampler,
            positive,
            negative,
            IsolinePoint::new(1e-6, 1e-6),
        );

        assert!(!valid);
    }

    #[test]
    fn circle_is_deterministic_and_explicitly_closed() {
        let options = IsolineOptions {
            min_depth: 4,
            max_quads: 600,
            tolerance: None,
        };
        let circle = |point: IsolinePoint| point.x * point.x + point.y * point.y - 1.0;
        let first = plan_isoline(circle, bounds(), options).unwrap();
        let second = plan_isoline(circle, bounds(), options).unwrap();

        assert_eq!(first, second);
        let closed = first
            .curves
            .iter()
            .filter(|curve| curve.len() > 2 && curve.first() == curve.last())
            .count();
        assert_eq!(closed, 1);
    }

    #[test]
    fn invalid_domain_and_tolerance_fail_before_sampling() {
        assert_eq!(
            plan_isoline(
                |_| panic!("invalid bounds must fail before sampling"),
                IsolineBounds::new(IsolinePoint::new(1.0, 0.0), IsolinePoint::new(0.0, 1.0)),
                IsolineOptions::default(),
            ),
            Err(IsolineError::InvalidBounds)
        );
        assert_eq!(
            plan_isoline(
                |_| panic!("invalid tolerance must fail before sampling"),
                bounds(),
                IsolineOptions {
                    tolerance: Some(IsolinePoint::new(0.0, 1e-3)),
                    ..IsolineOptions::default()
                },
            ),
            Err(IsolineError::InvalidTolerance)
        );
    }
}
