//! Native VectorScene and LinearTransformationScene helpers over ordinary
//! semantic planes, Arrow families, and the shared family path-edit operation.

use crate::{
    AuthoringError, CoordinateAuthoringError, ManimArrow, ManimArrowOptions, ManimNumberPlane,
    ManimNumberPlaneOptions, MobjectFamily, MobjectTarget, Scene, DEFAULT_ARROW_STROKE_WIDTH,
};
use noon_core::{Color, SemanticPaint, SemanticStyle, DEFAULT_FRAME_WIDTH, GREEN, GREY, RED};
use std::rc::Rc;

/// Small native configuration for the retained LTS building blocks.
///
/// The two plane option sets are deliberately independent. Coordinate labels,
/// animation wrappers, and ghost-vector histories remain ordinary caller-owned
/// semantic objects rather than state on this helper.
#[derive(Clone, Debug)]
pub struct LinearTransformationOptions {
    pub background_plane: Option<ManimNumberPlaneOptions>,
    pub foreground_plane: Option<ManimNumberPlaneOptions>,
    pub show_basis_vectors: bool,
}

impl Default for LinearTransformationOptions {
    fn default() -> Self {
        let mut background = ManimNumberPlaneOptions::default();
        background.axis_style.stroke = Some(SemanticPaint::Solid(GREY));
        background.background_line_style =
            muted_plane_style(crate::integration::MANIM_CAIRO_LINE_WIDTH_MULTIPLE);
        background.faded_line_style = None;
        let frame_width = f64::from(DEFAULT_FRAME_WIDTH);
        let foreground = ManimNumberPlaneOptions {
            x_range: [-frame_width, frame_width, 1.0],
            y_range: [-frame_width, frame_width, 1.0],
            ..ManimNumberPlaneOptions::default()
        };
        Self {
            background_plane: Some(background),
            foreground_plane: Some(foreground),
            show_basis_vectors: true,
        }
    }
}

fn muted_plane_style(width: f64) -> SemanticStyle {
    SemanticStyle {
        fill: None,
        stroke: Some(SemanticPaint::Solid(GREY)),
        stroke_width: width,
        ..SemanticStyle::default()
    }
}

/// Manim LinearTransformationScene's default `apply_matrix` path arc for the
/// usual column-vector matrix. Rust owns the basis transforms and angle math.
pub fn linear_transformation_path_arc(
    values: &[f64],
    rows: usize,
    columns: usize,
) -> Result<f64, AuthoringError> {
    let right = crate::matrix_authoring::transform_planar_point_about(
        values,
        rows,
        columns,
        (1.0, 0.0),
        (0.0, 0.0),
    )?;
    let up = crate::matrix_authoring::transform_planar_point_about(
        values,
        rows,
        columns,
        (0.0, 1.0),
        (0.0, 0.0),
    )?;
    Ok((right.1.atan2(right.0) + up.1.atan2(up.0) - std::f64::consts::FRAC_PI_2) / 2.0)
}

/// Stable handles to the shared LTS parts. Family membership remains the sole
/// source of truth; this value caches no leaf list, geometry, or transform.
#[derive(Clone, Debug)]
pub struct LinearTransformationAuthoring {
    background_plane: Option<ManimNumberPlane>,
    foreground_plane: Option<ManimNumberPlane>,
    basis_vectors: Option<MobjectFamily>,
    basis_arrows: Vec<ManimArrow>,
    vectors: Vec<ManimArrow>,
}

impl LinearTransformationAuthoring {
    pub fn background_plane(&self) -> Option<&ManimNumberPlane> {
        self.background_plane.as_ref()
    }

    pub fn foreground_plane(&self) -> Option<&ManimNumberPlane> {
        self.foreground_plane.as_ref()
    }

    pub fn basis_vectors(&self) -> Option<&MobjectFamily> {
        self.basis_vectors.as_ref()
    }

    pub fn vectors(&self) -> &[ManimArrow] {
        &self.vectors
    }

    pub fn basis_arrows(&self) -> &[ManimArrow] {
        &self.basis_arrows
    }

    /// Add a tracked vector anchored at the origin, matching VectorScene's
    /// coordinate input. The regular Arrow constructor owns shaft and tip.
    pub fn add_vector(
        &mut self,
        scene: &mut Scene,
        x: f64,
        y: f64,
        color: Color,
    ) -> Result<&ManimArrow, CoordinateAuthoringError> {
        let vector = create_colored_vector(scene, x, y, color)?;
        if let Some(basis) = &self.basis_vectors {
            basis.add(MobjectTarget::from(vector.family()))?;
        } else {
            scene.add_many(&[MobjectTarget::from(vector.family())])?;
        }
        self.vectors.push(vector);
        Ok(self.vectors.last().expect("just appended vector"))
    }

    /// Apply a planar matrix to the foreground grid. Vector targets are
    /// constructed separately with [`Self::vector_matrix_target`] so arrow tips
    /// are regenerated rather than geometrically sheared.
    pub fn apply_matrix_to_plane(
        &self,
        scene: &mut Scene,
        values: &[f64],
        rows: usize,
        columns: usize,
    ) -> Result<(), AuthoringError> {
        let Some(plane) = &self.foreground_plane else {
            return Ok(());
        };
        scene.apply_matrix_to_family(plane.family(), values, rows, columns, 0.0, 0.0)
    }

    /// Build a detached Arrow target from a tracked origin vector. This keeps
    /// the Arrow semantic constructor responsible for regenerating its tip.
    pub fn vector_matrix_target(
        &self,
        scene: &Scene,
        index: usize,
        values: &[f64],
        rows: usize,
        columns: usize,
    ) -> Result<ManimArrow, AuthoringError> {
        let source = self
            .vectors
            .get(index)
            .ok_or(AuthoringError::NonFiniteObjectState)?;
        matrix_arrow_target(scene, source, values, rows, columns, (0.0, 0.0))
    }

    /// Build a detached target family for every basis and tracked vector.
    /// Each Arrow target is regenerated from transformed public endpoints so
    /// its tip geometry is recomputed by the shared Arrow constructor.
    pub fn vector_matrix_target_family(
        &self,
        scene: &Scene,
        values: &[f64],
        rows: usize,
        columns: usize,
        about: (f64, f64),
    ) -> Result<MobjectFamily, AuthoringError> {
        let targets = self
            .basis_arrows
            .iter()
            .chain(&self.vectors)
            .map(|source| matrix_arrow_target(scene, source, values, rows, columns, about))
            .collect::<Result<Vec<_>, _>>()?;
        let members = targets
            .iter()
            .map(|arrow| MobjectTarget::from(arrow.family()))
            .collect::<Vec<_>>();
        MobjectFamily::create(Rc::clone(scene.integration_store()), &members)
    }
}

fn matrix_arrow_target(
    scene: &Scene,
    source: &ManimArrow,
    values: &[f64],
    rows: usize,
    columns: usize,
    about: (f64, f64),
) -> Result<ManimArrow, AuthoringError> {
    let endpoints = source.manim_endpoints()?;
    let start = crate::matrix_authoring::transform_planar_point_about(
        values,
        rows,
        columns,
        endpoints.start,
        about,
    )?;
    let end = crate::matrix_authoring::transform_planar_point_about(
        values,
        rows,
        columns,
        endpoints.end,
        about,
    )?;
    let mut options = ManimArrowOptions::arrow(start.0, start.1, end.0, end.1)?;
    if endpoints.start.0.abs() <= 1e-6 && endpoints.start.1.abs() <= 1e-6 {
        options.set_buff(0.0)?;
    }
    let state = source.shaft().state()?;
    if let Some(SemanticPaint::Solid(color)) = state.style.stroke {
        options.set_color(
            f64::from(color.red),
            f64::from(color.green),
            f64::from(color.blue),
            f64::from(color.alpha),
        )?;
    }
    options.set_stroke_width(state.style.stroke_width)?;
    ManimArrow::create(Rc::clone(scene.integration_store()), options)
}

impl Scene {
    /// Construct and add a VectorScene-style NumberPlane using the shared
    /// retained coordinate constructor.
    pub fn add_vector_plane(
        &mut self,
        options: &ManimNumberPlaneOptions,
    ) -> Result<ManimNumberPlane, CoordinateAuthoringError> {
        let plane = self.number_plane(options)?;
        self.add_many(&[MobjectTarget::from(plane.family())])?;
        Ok(plane)
    }

    /// Construct and add a VectorScene-style origin vector. The native default
    /// is Manim's pure yellow, and Vector's zero buff is supplied by the typed
    /// Arrow option constructor.
    pub fn add_vector(&mut self, x: f64, y: f64) -> Result<ManimArrow, CoordinateAuthoringError> {
        self.add_colored_vector(x, y, Color::from_hex(0xFFFF00))
    }

    pub fn add_colored_vector(
        &mut self,
        x: f64,
        y: f64,
        color: Color,
    ) -> Result<ManimArrow, CoordinateAuthoringError> {
        let arrow = create_colored_vector(self, x, y, color)?;
        self.add_many(&[MobjectTarget::from(arrow.family())])?;
        Ok(arrow)
    }
}

fn create_colored_vector(
    scene: &mut Scene,
    x: f64,
    y: f64,
    color: Color,
) -> Result<ManimArrow, CoordinateAuthoringError> {
    let mut options = ManimArrowOptions::vector(x, y)?;
    options.set_color(
        f64::from(color.red),
        f64::from(color.green),
        f64::from(color.blue),
        f64::from(color.alpha),
    )?;
    Ok(scene.manim_arrow(options)?)
}

impl Scene {
    /// Compose the existing NumberPlane and Arrow constructors into the
    /// ordinary retained LTS setup. No scene/runtime state is introduced.
    pub fn linear_transformation_setup(
        &mut self,
        options: &LinearTransformationOptions,
    ) -> Result<LinearTransformationAuthoring, CoordinateAuthoringError> {
        // Validate every plane request before the first Scene publication, so
        // malformed later input cannot leave a partially-authored LTS setup.
        if let Some(background) = &options.background_plane {
            crate::coordinate_authoring::prepare_number_plane(background)?;
        }
        if let Some(foreground) = &options.foreground_plane {
            crate::coordinate_authoring::prepare_number_plane(foreground)?;
        }
        let background_plane = options
            .background_plane
            .as_ref()
            .map(|plane| self.add_vector_plane(plane))
            .transpose()?;
        let foreground_plane = options
            .foreground_plane
            .as_ref()
            .map(|plane| self.add_vector_plane(plane))
            .transpose()?;
        let (basis_vectors, basis_arrows) = if options.show_basis_vectors {
            let i_hat = add_basis_vector(self, 1.0, 0.0, RED)?;
            let j_hat = add_basis_vector(self, 0.0, 1.0, GREEN)?;
            let family = MobjectFamily::create(
                Rc::clone(self.integration_store()),
                &[
                    MobjectTarget::from(i_hat.family()),
                    MobjectTarget::from(j_hat.family()),
                ],
            )?;
            self.add_many(&[MobjectTarget::from(&family)])?;
            (Some(family), vec![i_hat, j_hat])
        } else {
            (None, Vec::new())
        };
        Ok(LinearTransformationAuthoring {
            background_plane,
            foreground_plane,
            basis_vectors,
            basis_arrows,
            vectors: Vec::new(),
        })
    }
}

fn add_basis_vector(
    scene: &mut Scene,
    x: f64,
    y: f64,
    color: Color,
) -> Result<ManimArrow, AuthoringError> {
    let mut options = ManimArrowOptions::vector(x, y)?;
    options.set_color(
        f64::from(color.red),
        f64::from(color.green),
        f64::from(color.blue),
        f64::from(color.alpha),
    )?;
    options.set_stroke_width(DEFAULT_ARROW_STROKE_WIDTH)?;
    let arrow = scene.manim_arrow(options)?;
    Ok(arrow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_transformation_defaults_compose_retained_planes_and_basis_arrows() {
        let mut scene = Scene::new();
        let setup = scene
            .linear_transformation_setup(&LinearTransformationOptions::default())
            .unwrap();
        assert!(setup.background_plane().is_some());
        assert!(setup.foreground_plane().is_some());
        let basis = setup.basis_vectors().unwrap();
        let store = scene.integration_store().borrow();
        let members = store
            .semantic_family_checked(basis.node_id())
            .unwrap()
            .members_iter()
            .count();
        assert_eq!(members, 2);
        let foreground = setup.foreground_plane().unwrap();
        assert_eq!(foreground.authored_frame().unwrap().x().unit_size(), 1.0);
        let background = setup.background_plane().unwrap();
        let state = background
            .x_axis()
            .unwrap()
            .shaft()
            .unwrap()
            .state()
            .unwrap();
        assert_eq!(state.style.stroke, Some(SemanticPaint::Solid(GREY)));
        // Manim NumberPlane's default axis width is 2 Cairo pixels (0.02
        // scene units after the facade's 0.01 conversion).
        assert_eq!(state.style.stroke_width, 0.02);
        let background_lines = background.background_lines().unwrap();
        let store_rc = background_lines.integration_store();
        let store = store_rc.borrow();
        let first_line = store
            .semantic_family_checked(background_lines.node_id())
            .unwrap()
            .members_iter()
            .next()
            .unwrap();
        let grid_state = store.semantic_object_state_checked(first_line).unwrap();
        assert_eq!(grid_state.style.stroke, Some(SemanticPaint::Solid(GREY)));
        assert!((grid_state.style.stroke_width - 0.01).abs() < 1e-12);
    }

    #[test]
    fn vector_scene_add_vector_uses_origin_and_pure_yellow_default() {
        let mut scene = Scene::new();
        let vector = scene.add_vector(2.0, 1.0).unwrap();
        let endpoints = vector.manim_endpoints().unwrap();
        assert!((endpoints.start.0).abs() < 1e-6 && (endpoints.start.1).abs() < 1e-6);
        assert!((endpoints.end.0 - 2.0).abs() < 1e-6);
        assert!((endpoints.end.1 - 1.0).abs() < 1e-6);
        let state = vector.shaft().state().unwrap();
        assert_eq!(
            state.style.stroke,
            Some(SemanticPaint::Solid(Color::from_hex(0xFFFF00)))
        );
    }

    #[test]
    fn invalid_lts_plane_request_does_not_publish_background_plane() {
        let mut scene = Scene::new();
        let mut invalid_foreground = ManimNumberPlaneOptions::default();
        invalid_foreground.x_range[0] = f64::NAN;
        let options = LinearTransformationOptions {
            background_plane: Some(ManimNumberPlaneOptions::default()),
            foreground_plane: Some(invalid_foreground),
            show_basis_vectors: true,
        };
        let revision = scene.revision();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        assert!(scene.linear_transformation_setup(&options).is_err());
        assert_eq!(scene.revision(), revision);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
            resources
        );
    }

    #[test]
    fn vector_matrix_target_rebuilds_arrow_tip_from_transformed_endpoint() {
        let mut scene = Scene::new();
        let mut setup = scene
            .linear_transformation_setup(&LinearTransformationOptions {
                background_plane: None,
                foreground_plane: None,
                show_basis_vectors: false,
            })
            .unwrap();
        setup.add_vector(&mut scene, 2.0, 1.0, RED).unwrap();
        let target = setup
            .vector_matrix_target(&scene, 0, &[1.0, 1.0, 0.0, 1.0], 2, 2)
            .unwrap();
        let (x, y) = target.manim_vector().unwrap();
        assert!((x - 3.0).abs() < 1e-5);
        assert!((y - 1.0).abs() < 1e-5);
        assert!(
            target.end_tip().state().unwrap().role() == noon_core::SemanticObjectRole::ArrowEndTip
        );
    }

    #[test]
    fn bad_vector_matrix_is_rejected_before_target_resources_are_created() {
        let mut scene = Scene::new();
        let mut setup = scene
            .linear_transformation_setup(&LinearTransformationOptions {
                background_plane: None,
                foreground_plane: None,
                show_basis_vectors: false,
            })
            .unwrap();
        setup.add_vector(&mut scene, 1.0, 0.0, RED).unwrap();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        assert!(setup
            .vector_matrix_target(&scene, 0, &[1.0, f64::NAN, 0.0, 1.0], 2, 2)
            .is_err());
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
            resources
        );
    }

    #[test]
    fn lts_default_path_arc_matches_manim_basis_angle_formula() {
        assert_eq!(
            linear_transformation_path_arc(&[0.0, 1.0, 1.0, 0.0], 2, 2).unwrap(),
            0.0
        );
        let shear = linear_transformation_path_arc(&[1.0, 1.0, 0.0, 1.0], 2, 2).unwrap();
        assert!((shear + std::f64::consts::FRAC_PI_8).abs() < 1e-12);
    }
}
