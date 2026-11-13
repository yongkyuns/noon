//! Root-only appends reuse the existing payload and coherent path publication.
use super::*;

impl MobjectFamily {
    /// Append only this root's coordinates, preserving identity and children.
    pub fn append_points_f64(&self, points: &[(f64, f64)]) -> Result<(), AuthoringError> {
        self.append_points_3d(&crate::path_editing::promote_raw_points_3d(points))
    }

    pub fn append_points(&self, points: &[Vec2]) -> Result<(), AuthoringError> {
        self.append_points_f64(
            &points
                .iter()
                .map(|point| (f64::from(point.x), f64::from(point.y)))
                .collect::<Vec<_>>(),
        )
    }
}

impl crate::Scene {
    /// Append family-root content through ordinary authored or live publication.
    pub fn append_family_points_f64(
        &mut self,
        family: &MobjectFamily,
        points: &[(f64, f64)],
    ) -> Result<(), AuthoringError> {
        self.append_family_points_3d(family, &crate::path_editing::promote_raw_points_3d(points))
    }
}

#[cfg(test)]
mod tests;
