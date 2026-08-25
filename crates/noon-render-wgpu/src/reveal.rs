use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use noon_core::{GeometryRef, PathCommand, VectorPath};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AnalyticRevealKey {
    Circle(u32),
    Rectangle(u32, u32),
    VectorPath(u64),
}

pub(crate) fn analytic_reveal_key(geometry: &GeometryRef) -> Option<AnalyticRevealKey> {
    match geometry {
        GeometryRef::Circle { radius } => Some(AnalyticRevealKey::Circle(radius.to_bits())),
        GeometryRef::Rectangle { size } => Some(AnalyticRevealKey::Rectangle(
            size.x.to_bits(),
            size.y.to_bits(),
        )),
        GeometryRef::VectorPath(path) => Some(AnalyticRevealKey::VectorPath(path_key(path))),
        GeometryRef::Line { .. } | GeometryRef::External(_) => None,
    }
}

pub(crate) fn temporary_reveal_path(
    geometry: &GeometryRef,
    reveal: f32,
) -> Option<(AnalyticRevealKey, VectorPath)> {
    if reveal >= 1.0 {
        return None;
    }
    let key = analytic_reveal_key(geometry)?;
    let path = noon_geometry::canonical_outline_path(geometry)?;
    let partial = noon_geometry::pointwise_partial_path(&path, 0.0, reveal.clamp(0.0, 1.0));
    Some((key, partial))
}

fn path_key(path: &VectorPath) -> u64 {
    let mut hasher = DefaultHasher::new();
    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                0u8.hash(&mut hasher);
                hash_vec(to, &mut hasher);
            }
            PathCommand::LineTo { to } => {
                1u8.hash(&mut hasher);
                hash_vec(to, &mut hasher);
            }
            PathCommand::QuadraticTo { control, to } => {
                2u8.hash(&mut hasher);
                hash_vec(control, &mut hasher);
                hash_vec(to, &mut hasher);
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                3u8.hash(&mut hasher);
                hash_vec(control1, &mut hasher);
                hash_vec(control2, &mut hasher);
                hash_vec(to, &mut hasher);
            }
            PathCommand::Close => 4u8.hash(&mut hasher),
        }
    }
    hasher.finish()
}

fn hash_vec(value: noon_core::Vec2, hasher: &mut DefaultHasher) {
    value.x.to_bits().hash(hasher);
    value.y.to_bits().hash(hasher);
}
