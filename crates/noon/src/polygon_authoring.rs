//! ManimCE v0.21 polygon and star geometry over the canonical retained path.
use crate::{semantic_mobject::authoring_render_f64, AuthoringError};
use noon_core::{Vec2, VectorPath};
use std::f64::consts::{FRAC_PI_2, TAU};

pub(crate) fn polygram_path(groups: &[Vec<(f64, f64)>]) -> Result<VectorPath, AuthoringError> {
    let mut path = VectorPath::new();
    for group in groups {
        if group.is_empty() {
            return Err(AuthoringError::InvalidPolygonOptions(
                "vertex groups must not be empty",
            ));
        }
        for (index, &(x, y)) in group.iter().enumerate() {
            let point = Vec2::new(
                authoring_render_f64("vertex.x", x)? as f32,
                authoring_render_f64("vertex.y", y)? as f32,
            );
            path = if index == 0 {
                path.move_to(point)
            } else {
                path.line_to(point)
            };
        }
        // Preserve upstream's explicit final corner, including a zero-length
        // edge for a single/repeated vertex; Close alone elides that segment.
        let (x, y) = group[0];
        path = path.line_to(Vec2::new(x as f32, y as f32)).close();
    }
    Ok(path)
}

fn vertices(n: u32, radius: f64, start: f64) -> Result<Vec<(f64, f64)>, AuthoringError> {
    if n == 0 {
        return Err(AuthoringError::InvalidPolygonOptions(
            "number of vertices must be positive",
        ));
    }
    authoring_render_f64("radius", radius)?;
    authoring_render_f64("start_angle", start)?;
    Ok((0..n)
        .map(|i| {
            let angle = start + TAU * f64::from(i) / f64::from(n);
            (radius * angle.cos(), radius * angle.sin())
        })
        .collect())
}

#[cfg(test)]
pub(crate) fn regular_polygram_path(
    n: u32,
    density: u32,
    radius: f64,
    start: Option<f64>,
) -> Result<VectorPath, AuthoringError> {
    regular_polygram_path_and_start_angle(n, density, radius, start).map(|(path, _)| path)
}

pub(crate) fn regular_polygram_path_and_start_angle(
    n: u32,
    density: u32,
    radius: f64,
    start: Option<f64>,
) -> Result<(VectorPath, f64), AuthoringError> {
    if n == 0 || density == 0 {
        return Err(AuthoringError::InvalidPolygonOptions(
            "vertex count and density must be positive",
        ));
    }
    let (mut a, mut b) = (n, density);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    let groups = a;
    let count = n / groups;
    let step = density / groups;
    let start = start.unwrap_or(if count.is_multiple_of(2) {
        0.0
    } else {
        FRAC_PI_2
    });
    let mut paths = Vec::new();
    for group in 0..groups {
        let points = vertices(count, radius, start + TAU * f64::from(group) / f64::from(n))?;
        paths.push(
            (0..count)
                .map(|i| points[((u64::from(i) * u64::from(step)) % u64::from(count)) as usize])
                .collect(),
        );
    }
    Ok((polygram_path(&paths)?, start))
}

#[cfg(test)]
pub(crate) fn star_path(
    n: u32,
    outer_radius: f64,
    inner_radius: Option<f64>,
    density: u32,
    start: Option<f64>,
) -> Result<VectorPath, AuthoringError> {
    star_path_and_start_angle(n, outer_radius, inner_radius, density, start).map(|(path, _)| path)
}

pub(crate) fn star_path_and_start_angle(
    n: u32,
    outer_radius: f64,
    inner_radius: Option<f64>,
    density: u32,
    start: Option<f64>,
) -> Result<(VectorPath, f64), AuthoringError> {
    if n == 0 {
        return Err(AuthoringError::InvalidPolygonOptions(
            "number of star points must be positive",
        ));
    }
    authoring_render_f64("outer_radius", outer_radius)?;
    let half_step = TAU / (2.0 * f64::from(n));
    let inner = match inner_radius {
        Some(radius) => radius,
        None => {
            if density == 0 || f64::from(density) >= f64::from(n) / 2.0 {
                return Err(AuthoringError::InvalidPolygonOptions(
                    "density must be positive and less than half the star point count",
                ));
            }
            let outer_angle = TAU * f64::from(density) / f64::from(n);
            let inverse_x = 1.0 - half_step.tan() * (outer_angle.cos() - 1.0) / outer_angle.sin();
            outer_radius / (half_step.cos() * inverse_x)
        }
    };
    let start = start.unwrap_or(if n.is_multiple_of(2) { 0.0 } else { FRAC_PI_2 });
    let outer = vertices(n, outer_radius, start)?;
    let inner = vertices(n, inner, start + half_step)?;
    Ok((
        polygram_path(&[outer
            .into_iter()
            .zip(inner)
            .flat_map(|(a, b)| [a, b])
            .collect()])?,
        start,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ManimGeometryOptions, Scene};

    #[test]
    fn regular_defaults_and_density_preserve_closed_vertex_groups() {
        let mut scene = Scene::new();
        let odd = scene
            .geometry(ManimGeometryOptions::regular_polygon(3, 1.0, None).unwrap())
            .unwrap();
        let anchors = odd.path_query().unwrap().start_anchors();
        assert_eq!(anchors.len(), 3);
        assert!((anchors[0].1 - 1.0).abs() < 1e-6);
        assert!(anchors[0].0.abs() < 1e-6);
        let even = scene
            .geometry(ManimGeometryOptions::regular_polygon(4, 2.0, None).unwrap())
            .unwrap();
        assert_eq!(even.path_query().unwrap().start_anchors()[0], (2.0, 0.0));
        let compound = scene
            .geometry(ManimGeometryOptions::regular_polygram(6, 2, 1.0, None).unwrap())
            .unwrap();
        assert_eq!(compound.path_query().unwrap().subpaths().len(), 2);
        assert_eq!(compound.path_query().unwrap().start_anchors().len(), 6);
        let groups = compound.path_query().unwrap().vertex_groups();
        assert_eq!(groups.iter().map(Vec::len).collect::<Vec<_>>(), vec![3, 3]);
        assert_eq!(
            ManimGeometryOptions::regular_polygram(6, 2, 1.0, None)
                .unwrap()
                .resolved_start_angle(),
            Some(FRAC_PI_2),
        );
        assert_eq!(
            ManimGeometryOptions::regular_polygram(6, 1, 1.0, Some(0.3))
                .unwrap()
                .resolved_start_angle(),
            Some(0.3),
        );
    }

    #[test]
    fn star_uses_upstream_intersection_radius_and_explicit_radius_override() {
        let mut scene = Scene::new();
        let options = ManimGeometryOptions::star(5, 1.0, None, 2, Some(FRAC_PI_2)).unwrap();
        assert_eq!(options.resolved_start_angle(), Some(FRAC_PI_2));
        let star = scene.geometry(options).unwrap();
        let anchors = star.path_query().unwrap().start_anchors();
        assert_eq!(anchors.len(), 10);
        assert!((anchors[1].0.hypot(anchors[1].1) - 0.38196601125).abs() < 1e-6);
        assert!(ManimGeometryOptions::star(4, 1.0, None, 2, None).is_err());
        assert!(ManimGeometryOptions::star(4, 1.0, Some(0.5), 2, None).is_ok());
    }

    #[test]
    fn invalid_polygon_inputs_fail_before_geometry_options_are_built() {
        assert!(polygram_path(&[vec![]]).is_err());
        assert!(polygram_path(&[vec![(f64::NAN, 0.0)]]).is_err());
        assert!(regular_polygram_path(0, 1, 1.0, None).is_err());
        assert!(regular_polygram_path(5, 0, 1.0, None).is_err());
        assert!(star_path(5, 1.0, Some(f64::INFINITY), 2, None).is_err());
    }

    #[test]
    fn repeated_and_single_vertices_keep_their_explicit_closing_edges() {
        let mut scene = Scene::new();
        let single = scene
            .geometry(ManimGeometryOptions::polygon(&[(1.0, 2.0)]).unwrap())
            .unwrap();
        assert_eq!(
            single.path_query().unwrap().vertex_groups(),
            vec![vec![(1.0, 2.0)]]
        );
        let repeated = scene
            .geometry(
                ManimGeometryOptions::polygon(&[(0.0, 0.0), (1.0, 0.0), (0.0, 0.0), (0.0, 1.0)])
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(
            repeated
                .path_query()
                .unwrap()
                .vertex_groups()
                .iter()
                .map(Vec::len)
                .collect::<Vec<_>>(),
            vec![2, 2]
        );
    }
}
