use bevy_ecs::prelude::*;
use lyon::{builder::WithSvg, iterator::PathIterator, PathEvent};
use nannou::lyon::path as lyon;
use nannou::lyon::{
    algorithms::length::approximate_length,
    lyon_algorithms::walk::walk_along_path,
    lyon_algorithms::walk::RepeatedPattern,
    path::{iterator::Flattened, path::Iter},
};

// use nannou::lyon::math::Transform;
use crate::Transform;

use crate::{point, Interpolate, Point, Size, EPS_LOW};

/// Global path component that can be rendered to screen without any transformation
#[derive(Debug, Clone, Component)]
pub struct PixelPath(pub(crate) Path);

/// Data type for representing a vectorized 2D path.
///
/// [Path] is a thin wrapper around [lyon::Path] and mainly adds [Interpolate]
/// and exposes a few convenience methods. By allowing any arbitrary path to
/// be interpolated, [animate](crate::animate) can transform from one shape
/// to another shape through [Animation](crate::Animation).
#[derive(Debug, Clone, Component)]
pub struct Path {
    /// Lyon path
    pub(crate) raw: lyon::Path,
    /// Boolean to indicate whether the path is closed shape or not.
    pub(crate) closed: bool,
}

impl Path {
    pub fn new(path: lyon::Path, closed: bool) -> Self {
        Self { raw: path, closed }
    }
    /// Expose `Lyon`'s [SVG path builder](WithSvg)
    pub fn svg_builder() -> WithSvg<lyon::path::Builder> {
        lyon::Path::svg_builder()
    }
    /// Expose `Lyon`'s [path builder](lyon::path::Builder)
    pub fn builder() -> lyon::path::Builder {
        lyon::path::Builder::new()
    }
    /// Flatten the current path into series of linear line segments
    /// with given tolerance.
    ///
    /// Note that the returned path is an iterator.
    pub fn flattened(&self, tolerance: f32) -> Flattened<Iter> {
        self.raw.iter().flattened(tolerance)
    }

    /// Provides rough size of the path
    pub fn size(&self) -> Size {
        let mut max = point(-1.0e5, -1.0e5);
        let mut min = point(1.0e5, 1.0e5);
        for e in self.flattened(EPS_LOW) {
            match e {
                PathEvent::Line { from, to } => {
                    // Probably only need to compare with destination, i.e. `to`
                    // Remove later
                    min = min.min(from);
                    max = max.max(from);
                    min = min.min(to);
                    max = max.max(to);
                }
                _ => (),
            }
        }
        Size::from_points(&vec![min, max])
    }

    /// Apply given scale to the path.
    pub fn scale(&self, x: f32, y: f32) -> Self {
        Self::new(
            self.raw
                .clone()
                .transformed(&nannou::lyon::math::Transform::scale(x, y)),
            self.closed,
        )
    }

    pub fn transform(&self, transform: &Transform) -> Self {
        Self::new(self.raw.clone().transformed(&transform.0), self.closed)
    }
}

impl Interpolate for Path {
    fn interp(&self, other: &Self, progress: f32) -> Self {
        let tol = EPS_LOW;
        // let progress = progress.min(1.0).max(0.0);

        if progress <= 0.00001 {
            self.clone()
        } else if progress >= 0.99999 {
            other.clone()
        } else {
            // 1. Calculate the length of initial and final paths (1 and 2)
            // 2. Iterate through each path and construct normalized distance array
            // 3. Combine normalized distances from both paths into a single array
            // 4. Walk through each path and fill-in missing points to make sizes equal
            // 5. Interpolate each point between initial and final path
            // 6. Construct Path with above points as line segments
            let segments_src = get_segments(&self);
            let segments_dst = get_segments(&other);

            let mut interpolated = Vec::new();
            for (src, dst) in segments_src.iter().zip(segments_dst.iter()) {
                interpolated.push(interp_segment(src, dst, progress, tol, self.closed));
            }
            // Path::new(interpolated.get(0).unwrap().raw.clone(), self.closed)
            if segments_src.len() > segments_dst.len() {
                for src in segments_src.iter().skip(segments_dst.len()) {
                    interpolated.push(interp_segment(
                        src,
                        segments_dst.last().unwrap(),
                        progress,
                        tol,
                        self.closed,
                    ));
                }
            } else if segments_src.len() < segments_dst.len() {
                for dst in segments_dst.iter().skip(segments_src.len()) {
                    interpolated.push(interp_segment(
                        segments_src.last().unwrap(),
                        dst,
                        progress,
                        tol,
                        self.closed,
                    ));
                }
            }

            merge_segments(&interpolated)
        }
    }
}

fn interp_segment(
    source: &Path,
    destination: &Path,
    progress: f32,
    tolerance: f32,
    closed: bool,
) -> Path {
    let src_len = get_line_lengths(source.flattened(tolerance));
    let dst_len = get_line_lengths(destination.flattened(tolerance));

    let mut builder = Path::svg_builder();
    if src_len.len() > 1 && dst_len.len() > 1 {
        let normalized = normalized_distances(&src_len, &dst_len);

        let src_max_len = *src_len.last().unwrap();
        let dst_max_len = *dst_len.last().unwrap();

        let p1 = points_from_path(source.flattened(tolerance), &normalized, src_max_len);
        let p2 = points_from_path(destination.flattened(tolerance), &normalized, dst_max_len);

        p1.iter().zip(p2.iter()).for_each(|(&p1, p2)| {
            builder.line_to(p1.interp(p2, progress));
        });

        if closed {
            builder.close();
        }
    }
    Path::new(builder.build(), closed)
}

fn merge_segments(paths: &[Path]) -> Path {
    let mut builder = Path::builder();
    let mut closed = true;
    for path in paths {
        closed = path.closed;
        builder.concatenate(&vec![path.raw.as_slice()]);
    }
    Path::new(builder.build(), closed)
}

fn get_segments(path: &Path) -> Vec<Path> {
    let mut segments = Vec::new();
    let mut path_iter = path.raw.iter();
    while let Some(segment) = get_segment(&mut path_iter) {
        segments.push(segment);
    }
    segments
}

fn get_segment(path_iter: &mut Iter) -> Option<Path> {
    let mut builder = Path::builder();
    let mut count = 0;
    while let Some(event) = path_iter.next() {
        builder.path_event(event);
        count += 1;
        if let PathEvent::End { .. } = event {
            break;
        }
    }
    if count > 0 {
        Some(Path::new(builder.build(), true))
    } else {
        None
    }
}

fn points_from_path(
    path: Flattened<Iter>,
    normalized_len: &[f32],
    total_length: f32,
) -> Vec<Point> {
    // Compute the delta distance between each point
    let lengths: Vec<f32> = normalized_len
        .iter()
        .zip(normalized_len.iter().skip(1))
        .map(|(a, b)| b - a)
        .map(|val| val * total_length)
        .collect();

    let mut points = Vec::new();
    let mut pattern = RepeatedPattern {
        callback: &mut |position, _t, _d| {
            points.push(position);
            true
        },
        intervals: &lengths,
        index: 0,
    };

    walk_along_path(path.into_iter(), 0.0, &mut pattern);
    points
}

fn get_line_lengths(flattened: Flattened<Iter>) -> Vec<f32> {
    let mut p = flattened
        .filter(|e| matches!(e, PathEvent::Line { .. }))
        .scan(0.0, |d, event| {
            match event {
                PathEvent::Line { from, to } => {
                    *d += (to - from).length();
                }
                _ => (),
            };
            Some(*d)
        })
        .collect::<Vec<f32>>();
    p.insert(0, 0.0); // This is needed to correct the initial point being shifted by 1 index
    p
}

// Combine two vectors which are both monotonically increasing by normalized ordering
fn normalized_distances(v1: &[f32], v2: &[f32]) -> Vec<f32> {
    let mut combined = Vec::new();

    let s1 = *v1.last().unwrap();
    let s2 = *v2.last().unwrap();

    let mut v2_iter = v2.iter().peekable();
    for val1 in v1.into_iter() {
        while let Some(val2) = v2_iter.peek() {
            if **val2 / s2 < val1 / s1 {
                combined.push(**val2 / s2);
                v2_iter.next();
            } else {
                break;
            }
        }
        combined.push(val1 / s1);
    }

    combined
}

pub trait PathComponent {
    fn path(size: &Size) -> Path;
}

pub trait MeasureLength {
    fn approximate_length(&self, tolerance: f32) -> f32;
}

impl MeasureLength for Path {
    fn approximate_length(&self, tolerance: f32) -> f32 {
        approximate_length(self.raw.iter(), tolerance)
    }
}

pub trait GetPartial: MeasureLength {
    fn upto(&self, ratio: f32, tolerance: f32) -> Path;
}

impl GetPartial for Path {
    fn upto(&self, ratio: f32, tolerance: f32) -> Path {
        if ratio >= 1.0 {
            self.clone()
        } else {
            let ratio = ratio.max(0.0);
            let full_length = self.approximate_length(tolerance);
            let stop_at = ratio * full_length;

            let mut builder = Path::svg_builder();
            let mut length = 0.0;

            for e in self.raw.iter().flattened(tolerance) {
                if length > stop_at {
                    break;
                }
                match e {
                    PathEvent::Begin { at } => {
                        builder.move_to(at);
                    }
                    PathEvent::Line { from, to } => {
                        let seg_length = (to - from).length();
                        let new_length = length + seg_length;
                        if new_length > stop_at {
                            let seg_ratio = 1.0 - (new_length - stop_at) / seg_length;
                            builder.line_to(from.lerp(to, seg_ratio));
                            break;
                        } else {
                            length = new_length;
                            builder.line_to(to);
                        }
                    }
                    PathEvent::End { .. } => {
                        builder.close();
                    }
                    _ => (),
                }
            }
            Self::new(builder.build(), self.closed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // use nannou::geom::rect::Rect;
    // use nannou::lyon::path::builder::PathBuilder;
    use nannou::lyon::math::{point, Point};
    use nannou::prelude::*;

    #[test]
    fn partial_path() {
        let win_rect = Rect::from_w_h(640.0, 480.0);
        let text = text("Hello").font_size(128).left_justify().build(win_rect);
        let mut builder = Path::builder();
        for e in text.path_events() {
            builder.path_event(e);
        }
        builder.close();
        let path = Path::new(builder.build(), true);
        let partial_path = path.upto(0.5, 0.01);

        println!("length = {}", partial_path.approximate_length(0.01));
    }

    #[test]
    fn flatten() {
        let mut builder = Path::svg_builder();
        builder.move_to(point(0.0, 0.0));
        builder.line_to(point(10.0, 0.0));
        builder.close();
        let path = Path::new(builder.build(), true).upto(0.5, 0.01);
        for e in path.raw.iter().flattened(0.01) {
            match e {
                PathEvent::Begin { .. } => {}
                PathEvent::Line { from, to } => {
                    println!("from:({},{}), to:({},{})", from.x, from.y, to.x, to.y);
                }
                PathEvent::End { .. } => {}
                _ => (),
            }
        }
    }

    #[test]
    fn iter_check() {
        let arr = [1, 2, 3, 4, 5];
        let out = arr
            .iter()
            .zip(arr.iter().skip(1))
            .scan(0, |val, a| {
                *val += a.0 + a.1;
                Some(*val)
            })
            .collect::<Vec<i32>>();

        dbg!(out);
    }

    #[test]
    fn length() {
        use nannou::lyon::algorithms::length::approximate_length;

        let mut builder = Path::svg_builder();
        builder.move_to(point(0.0, 0.0));
        builder.line_to(point(10.0, 0.0));
        builder.quadratic_bezier_to(point(15.0, 5.0), point(20.0, 0.0));
        builder.close();

        let path = Path::new(builder.build(), true);
        let l = approximate_length(path.raw.iter(), 0.01);
        let l2 = path.approximate_length(0.01);

        println!("{}, {}", l, l2);
    }

    #[test]
    fn check_vector_ordering() {
        let v1 = vec![0.0, 0.3, 0.6, 0.8, 1.0];
        let v2 = vec![0.2, 0.5, 0.55, 0.8, 2.0];

        let out = normalized_distances(&v1, &v2);
        assert_eq!(*out, vec![0.0, 0.1, 0.25, 0.275, 0.3, 0.4, 0.6, 0.8, 1.0]);
    }

    #[test]
    fn check_walk() {
        let mut builder = Path::builder();
        builder.begin(point(5.0, 5.0));
        builder.line_to(point(5.0, 10.0));
        builder.line_to(point(10.0, 10.0));
        builder.line_to(point(10.0, 5.0));
        builder.end(true);
        let path = builder.build();

        let pts = vec![0.0, 2.0, 2.5, 5.0, 10.0, 20.0];
        let pts: Vec<f32> = pts
            .iter()
            .zip(pts.iter().skip(1))
            .map(|(a, b)| b - a)
            .collect();

        let mut pattern = RepeatedPattern {
            callback: &mut |position: Point, _t, d| {
                println!("d = {}, x = {}, y = {}", d, position.x, position.y);
                true
            },
            intervals: &pts,
            index: 0,
        };

        walk_along_path(path.iter(), 0.0, &mut pattern);
    }
    #[test]
    fn path_size() {
        let mut builder = Path::svg_builder();
        builder.move_to(point(-100.0, 200.0));
        builder.line_to(point(100.0, 300.0));
        builder.line_to(point(-300.0, 300.0));
        builder.close();
        let size = Path::new(builder.build(), true).size();
        assert_eq!(400.0, size.width);
        assert_eq!(100.0, size.height);
    }

    #[test]
    fn path_evaluation() {
        let mut builder = Path::svg_builder();
        builder.move_to(point(-100.0, 0.0));
        builder.line_to(point(-100.0, 100.0));
        builder.line_to(point(0.0, 100.0));
        builder.line_to(point(0.0, 0.0));
        builder.close();
        builder.move_to(point(100.0, 0.0));
        builder.line_to(point(100.0, 100.0));
        builder.line_to(point(200.0, 100.0));
        builder.move_to(point(200.0, 0.0));
        builder.line_to(point(200.0, 200.0));
        builder.line_to(point(300.0, 200.0));
        builder.close();
        let p = builder.build();

        // let mut builder = Path::builder();
        // builder.begin(point(-100.0, 0.0));
        // builder.line_to(point(-100.0, 100.0));
        // builder.line_to(point(0.0, 100.0));
        // builder.line_to(point(0.0, 0.0));
        // builder.close();
        // builder.begin(point(100.0, 0.0));
        // builder.line_to(point(100.0, 100.0));
        // builder.line_to(point(200.0, 100.0));
        // builder.close();
        // builder.begin(point(200.0, 0.0));
        // builder.line_to(point(200.0, 200.0));
        // builder.line_to(point(300.0, 200.0));
        // builder.close();
        // let p = builder.build();

        // dbg!(&p);
        for e in p.iter() {
            dbg!(&e);
        }
    }

    #[test]
    fn circle() {
        use nannou::lyon::math::{Angle, Vector};
        let mut builder = Path::svg_builder();

        let radius = 3.0;
        let sweep_angle = Angle::radians(-TAU);
        let x_rotation = Angle::radians(0.0);
        let center = point(0.0, 0.0);
        let start = point(radius, 0.0);
        let radii = Vector::new(radius, radius);

        builder.move_to(start);
        builder.arc(center, radii, sweep_angle, x_rotation);
        builder.close();

        // let mut path = Path::new(builder.build()).upto(0.5, 0.01);
        for e in builder.build().iter() {
            match e {
                PathEvent::Begin { at } => {
                    println!("Begin -> at:({},{})", at.x, at.y);
                }
                PathEvent::Line { from, to } => {
                    println!(
                        "Line -> from:({},{}), to:({},{})",
                        from.x, from.y, to.x, to.y
                    );
                }
                PathEvent::Quadratic { from, to, .. } => {
                    println!(
                        "Quadratic -> from:({},{}), to:({},{})",
                        from.x, from.y, to.x, to.y
                    );
                }
                PathEvent::Cubic { from, to, .. } => {
                    println!(
                        "Cubic -> from:({},{}), to:({},{})",
                        from.x, from.y, to.x, to.y
                    );
                }
                PathEvent::End { .. } => {
                    println!("End");
                }
            }
        }
    }
}
