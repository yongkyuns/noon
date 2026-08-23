use noon_core::{GeometryRef, VectorPath};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AnalyticRevealKey {
    Rectangle(u32, u32),
}

pub(crate) fn analytic_reveal_key(geometry: &GeometryRef) -> Option<AnalyticRevealKey> {
    match geometry {
        GeometryRef::Circle { .. } => None,
        GeometryRef::Rectangle { size } => Some(AnalyticRevealKey::Rectangle(
            size.x.to_bits(),
            size.y.to_bits(),
        )),
        GeometryRef::Line { .. } | GeometryRef::VectorPath(_) | GeometryRef::External(_) => None,
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
    Some((key, path))
}
