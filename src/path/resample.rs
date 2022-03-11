use crate::geom::{point, Point, Vector};

use nannou::lyon::path as lyon;
use nannou::lyon::{
    lyon_algorithms::walk::Pattern,
    path::{
        builder::PathBuilder,
        geom::{CubicBezierSegment, QuadraticBezierSegment},
        path::Builder,
        EndpointId, PathEvent,
    },
};

/// Walk along path and insert line segments along the given distances
pub fn resample_along_path<Iter>(path: Iter, start: f32, pattern: &mut dyn Pattern) -> lyon::Path
where
    Iter: Iterator<Item = PathEvent>,
{
    let mut walker = PathWalker::new(start, pattern);
    for evt in path {
        walker.path_event(evt);
        if walker.done {
            break;
        }
    }
    walker.build()
}

/// A helper struct to walk along a flattened path using a builder API.
pub struct PathWalker<'l> {
    prev: Point,
    advancement: f32,
    leftover: f32,
    next_distance: f32,
    first: Point,
    need_moveto: bool,
    done: bool,
    builder: Builder,

    pattern: &'l mut dyn Pattern,
}

impl<'l> PathWalker<'l> {
    pub fn new(start: f32, pattern: &'l mut dyn Pattern) -> PathWalker<'l> {
        let start = f32::max(start, 0.0);
        PathWalker {
            prev: point(0.0, 0.0),
            first: point(0.0, 0.0),
            advancement: 0.0,
            leftover: 0.0,
            next_distance: start,
            need_moveto: true,
            done: false,
            builder: Builder::new(),
            pattern,
        }
    }
    pub fn build(self) -> lyon::Path {
        self.builder.build()
    }
}

impl<'l> PathBuilder for PathWalker<'l> {
    fn begin(&mut self, to: Point) -> EndpointId {
        self.builder.begin(to);
        self.need_moveto = false;
        self.first = to;
        self.prev = to;

        if let Some(distance) = self.pattern.begin(self.next_distance) {
            self.next_distance = distance;
        } else {
            self.done = true;
        }

        EndpointId::INVALID
    }

    fn line_to(&mut self, to: Point) -> EndpointId {
        debug_assert!(!self.need_moveto);

        let v = to - self.prev;
        let d = v.length();

        if d < 1e-5 {
            return EndpointId::INVALID;
        }

        let tangent = v / d;

        let mut distance = self.leftover + d;
        while distance >= self.next_distance {
            let position = self.prev + tangent * (self.next_distance - self.leftover);
            self.prev = position;
            self.leftover = 0.0;
            self.advancement += self.next_distance;
            distance -= self.next_distance;

            if let Some(distance) = self.pattern.next(position, tangent, self.advancement) {
                self.next_distance = distance;
                self.builder.line_to(position);
            } else {
                self.done = true;
                return EndpointId::INVALID;
            }
        }

        self.prev = to;
        self.leftover = distance;

        EndpointId::INVALID
    }

    fn end(&mut self, close: bool) {
        self.builder.end(close);
        if close {
            let first = self.first;
            self.line_to(first);
            self.need_moveto = true;
        }
    }

    fn quadratic_bezier_to(&mut self, ctrl: Point, to: Point) -> EndpointId {
        let curve = QuadraticBezierSegment {
            from: self.prev,
            ctrl,
            to,
        };
        curve.for_each_flattened(0.01, &mut |p| {
            self.line_to(p);
        });

        EndpointId::INVALID
    }

    fn cubic_bezier_to(&mut self, ctrl1: Point, ctrl2: Point, to: Point) -> EndpointId {
        let curve = CubicBezierSegment {
            from: self.prev,
            ctrl1,
            ctrl2,
            to,
        };
        curve.for_each_flattened(0.01, &mut |p| {
            self.line_to(p);
        });

        EndpointId::INVALID
    }
}

pub struct ResamplePattern<'l, Cb> {
    /// The function to call at each step.
    pub callback: Cb,
    /// Array of distances along the path to insert line segments
    pub intervals: &'l [f32],
    /// The index of the next interval in the sequence.
    pub index: usize,
}

impl<'l, Cb> Pattern for ResamplePattern<'l, Cb>
where
    Cb: FnMut(Point, Vector, f32) -> bool,
{
    #[inline]
    fn next(&mut self, position: Point, tangent: Vector, distance: f32) -> Option<f32> {
        if !(self.callback)(position, tangent, distance) {
            return None;
        }
        let idx = self.index % self.intervals.len();
        self.index += 1;
        Some(self.intervals[idx])
    }
}
