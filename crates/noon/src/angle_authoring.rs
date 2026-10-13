//! Planar Angle/RightAngle construction shares the canonical Arc/path operations.
use crate::{
    semantic_mobject::authoring_render_f64, AuthoringError, ManimGeometryOptions, Mobject,
};
use noon_core::{SemanticAngleRole, SemanticObjectRole, Vec2, VectorPath};
use std::f64::consts::TAU;

#[derive(Clone, Copy, Debug)]
pub struct ManimAngleOptions {
    pub radius: Option<f64>,
    pub quadrant: (f64, f64),
    pub other_angle: bool,
    pub elbow: bool,
    pub num_components: u32,
}
impl Default for ManimAngleOptions {
    fn default() -> Self {
        Self {
            radius: None,
            quadrant: (1.0, 1.0),
            other_angle: false,
            elbow: false,
            num_components: 9,
        }
    }
}

impl ManimGeometryOptions {
    /// Endpoints may be authored or observed from one coherent live frame.
    pub fn angle_from_endpoints(
        first: [(f64, f64); 2],
        second: [(f64, f64); 2],
        options: ManimAngleOptions,
    ) -> Result<Self, AuthoringError> {
        for point in first.into_iter().chain(second) {
            authoring_render_f64("line.x", point.0)?;
            authoring_render_f64("line.y", point.1)?;
        }
        for value in [options.quadrant.0, options.quadrant.1] {
            authoring_render_f64("quadrant", value)?;
        }
        if let Some(radius) = options.radius {
            authoring_render_f64("radius", radius)?;
        }
        let r = (first[1].0 - first[0].0, first[1].1 - first[0].1);
        let s = (second[1].0 - second[0].0, second[1].1 - second[0].1);
        let cross = |a: (f64, f64), b: (f64, f64)| a.0 * b.1 - a.1 * b.0;
        let determinant = cross(r, s);
        if determinant == 0.0 {
            // Pinned Manim issue #1930 makes parallel/collinear XY angles empty.
            let mut empty = Self::path(VectorPath::new())?;
            empty.set_semantic_role(SemanticObjectRole::Angle(SemanticAngleRole::new(Some(0.0))));
            return Ok(empty);
        }
        let offset = (second[0].0 - first[0].0, second[0].1 - first[0].1);
        let t = cross(offset, s) / determinant;
        let center = (first[0].0 + r.0 * t, first[0].1 + r.1 * t);
        authoring_render_f64("intersection.x", center.0)?;
        authoring_render_f64("intersection.y", center.1)?;
        let radius = options.radius.unwrap_or_else(|| {
            let a = first[usize::from(options.quadrant.0 == 1.0)];
            let b = second[usize::from(options.quadrant.1 == 1.0)];
            let distance = (a.0 - center.0)
                .hypot(a.1 - center.1)
                .min((b.0 - center.0).hypot(b.1 - center.1));
            if distance < 0.6 {
                (2.0 / 3.0) * distance
            } else {
                0.4
            }
        });
        let unit = |v: (f64, f64)| {
            let length = v.0.hypot(v.1);
            (v.0 / length, v.1 / length)
        };
        let (u, v) = (unit(r), unit(s));
        let a = (
            options.quadrant.0 * radius * u.0,
            options.quadrant.0 * radius * u.1,
        );
        let b = (
            options.quadrant.1 * radius * v.0,
            options.quadrant.1 * radius * v.1,
        );
        let mut geometry;
        let value;
        if options.elbow {
            let point = |delta: (f64, f64)| -> Result<Vec2, AuthoringError> {
                Ok(Vec2::new(
                    authoring_render_f64("elbow.x", center.0 + delta.0)? as f32,
                    authoring_render_f64("elbow.y", center.1 + delta.1)? as f32,
                ))
            };
            geometry = Self::path(
                VectorPath::new()
                    .move_to(point(a)?)
                    .line_to(point((a.0 + b.0, a.1 + b.1))?)
                    .line_to(point(b)?),
            )?;
            value = None;
        } else {
            let start = a.1.atan2(a.0);
            let end = b.1.atan2(b.0);
            let sweep = if options.other_angle {
                if end < start {
                    end - start
                } else {
                    end - start - TAU
                }
            } else if end > start {
                end - start
            } else {
                TAU - (start - end)
            };
            geometry = Self::arc(
                radius,
                start,
                sweep,
                options.num_components,
                center.0,
                center.1,
            )?;
            value = Some(sweep);
        }
        geometry.set_semantic_role(SemanticObjectRole::Angle(SemanticAngleRole::new(value)));
        Ok(geometry)
    }
}

impl Mobject {
    /// Nominal constructor value, as in Manim; visual mutations do not rewrite it.
    pub fn angle_value(&self, degrees: bool) -> Result<f64, AuthoringError> {
        let SemanticObjectRole::Angle(role) = self.state()?.role() else {
            return Err(AuthoringError::MissingAngleValue);
        };
        let radians = role.radians().ok_or(AuthoringError::MissingAngleValue)?;
        Ok(if degrees {
            radians.to_degrees()
        } else {
            radians
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Scene;
    use std::f64::consts::{FRAC_PI_2, PI};
    const HORIZONTAL: [(f64, f64); 2] = [(-1.0, 0.0), (1.0, 0.0)];
    const VERTICAL: [(f64, f64); 2] = [(0.0, -1.0), (0.0, 1.0)];

    #[test]
    fn quadrant_and_other_angle_observe_pinned_sweeps_and_nominal_values() {
        let mut scene = Scene::new();
        for (quadrant, other, expected) in [
            ((1.0, 1.0), false, FRAC_PI_2),
            ((1.0, 1.0), true, -3.0 * FRAC_PI_2),
            ((-1.0, 1.0), false, 3.0 * FRAC_PI_2),
        ] {
            let options = ManimAngleOptions {
                quadrant,
                other_angle: other,
                ..Default::default()
            };
            let mut angle = scene
                .geometry(
                    ManimGeometryOptions::angle_from_endpoints(HORIZONTAL, VERTICAL, options)
                        .unwrap(),
                )
                .unwrap();
            assert!((angle.angle_value(false).unwrap() - expected).abs() < 1e-12);
            assert!((angle.angle_value(true).unwrap() - expected * 180.0 / PI).abs() < 1e-12);
            angle.shift(2.0, 1.0).unwrap();
            assert_eq!(angle.angle_value(false).unwrap(), expected);
        }
    }

    #[test]
    fn default_radius_uses_short_selected_endpoint_and_elbow_has_two_segments() {
        let mut scene = Scene::new();
        let short = [(-1.0, 0.0), (0.3, 0.0)];
        let arc = scene
            .geometry(
                ManimGeometryOptions::angle_from_endpoints(short, VERTICAL, Default::default())
                    .unwrap(),
            )
            .unwrap();
        let start = arc.path_query().unwrap().start().unwrap();
        assert!((start.0 - 0.2).abs() < 1e-6);
        let elbow = scene
            .geometry(
                ManimGeometryOptions::angle_from_endpoints(
                    HORIZONTAL,
                    VERTICAL,
                    ManimAngleOptions {
                        elbow: true,
                        ..Default::default()
                    },
                )
                .unwrap(),
            )
            .unwrap();
        let path = elbow.path_query().unwrap();
        assert_eq!(path.curve_count(), 2);
        assert_eq!(
            elbow.angle_value(false),
            Err(AuthoringError::MissingAngleValue)
        );
        let end = path.end().unwrap();
        assert!(end.0.abs() < 1e-6 && (end.1 - 0.4).abs() < 1e-6);
    }

    #[test]
    fn parallel_xy_lines_are_empty_with_zero_value_and_invalid_inputs_fail() {
        let mut scene = Scene::new();
        let empty = scene
            .geometry(
                ManimGeometryOptions::angle_from_endpoints(
                    HORIZONTAL,
                    [(-1.0, 1.0), (1.0, 1.0)],
                    Default::default(),
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(empty.path_query().unwrap().curve_count(), 0);
        assert_eq!(empty.angle_value(false).unwrap(), 0.0);
        assert!(ManimGeometryOptions::angle_from_endpoints(
            HORIZONTAL,
            VERTICAL,
            ManimAngleOptions {
                radius: Some(f64::NAN),
                ..Default::default()
            }
        )
        .is_err());
        assert!(ManimGeometryOptions::angle_from_endpoints(
            HORIZONTAL,
            VERTICAL,
            ManimAngleOptions {
                num_components: 1,
                ..Default::default()
            }
        )
        .is_err());
    }
}
