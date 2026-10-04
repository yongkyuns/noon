//! Deterministic pointwise matrix target preparation for Manim-compatible ApplyMatrix.
use crate::{
    path_editing::{world_path, PreparedPathEdits},
    semantic_mobject::{authoring_render_f64, authoring_xy_f64},
    AuthoringError, Mobject, MobjectFamily, Scene, UnsupportedAuthoringOperation,
};
use noon_core::{PathCommand, SemanticObjectRole, Vec2, VectorPath};

#[derive(Clone, Copy, Debug, PartialEq)]
struct PlanarMatrix {
    xx: f64,
    xy: f64,
    yx: f64,
    yy: f64,
}

impl PlanarMatrix {
    fn parse(values: &[f64], rows: usize, columns: usize) -> Result<Self, AuthoringError> {
        let expected = rows
            .checked_mul(columns)
            .ok_or(AuthoringError::Unsupported(
                UnsupportedAuthoringOperation::ApplyMatrixDimensions,
            ))?;
        if values.len() != expected || !matches!((rows, columns), (2, 2) | (3, 3)) {
            return Err(AuthoringError::Unsupported(
                UnsupportedAuthoringOperation::ApplyMatrixDimensions,
            ));
        }
        let mut checked = Vec::with_capacity(values.len());
        for (index, value) in values.iter().copied().enumerate() {
            checked.push(authoring_render_f64(&format!("matrix[{index}]"), value)?);
        }
        // Noon currently authors only z=0 geometry/about-points. A 3x3
        // matrix is admissible exactly when it does not map XY into Z. The
        // third column and bottom-right coefficient cannot affect z=0 input.
        if rows == 3 && (checked[6].abs() > 1e-12 || checked[7].abs() > 1e-12) {
            return Err(AuthoringError::Unsupported(
                UnsupportedAuthoringOperation::ApplyMatrixNonPlanar,
            ));
        }
        Ok(Self {
            xx: checked[0],
            xy: checked[1],
            yx: checked[columns],
            yy: checked[columns + 1],
        })
    }

    fn transform_point(self, point: Vec2, about: (f64, f64)) -> Result<Vec2, AuthoringError> {
        let (x, y) = self.transform_point_f64((f64::from(point.x), f64::from(point.y)), about)?;
        Ok(Vec2::new(x as f32, y as f32))
    }

    fn transform_point_f64(
        self,
        point: (f64, f64),
        about: (f64, f64),
    ) -> Result<(f64, f64), AuthoringError> {
        let dx = point.0 - about.0;
        let dy = point.1 - about.1;
        let x = authoring_render_f64(
            "ApplyMatrix result.x",
            about.0 + self.xx * dx + self.xy * dy,
        )?;
        let y = authoring_render_f64(
            "ApplyMatrix result.y",
            about.1 + self.yx * dx + self.yy * dy,
        )?;
        Ok((x, y))
    }
}

pub(crate) fn transform_planar_point_about(
    values: &[f64],
    rows: usize,
    columns: usize,
    point: (f64, f64),
    about: (f64, f64),
) -> Result<(f64, f64), AuthoringError> {
    let matrix = PlanarMatrix::parse(values, rows, columns)?;
    let point = authoring_xy_f64(point.0, point.1)?;
    let about = authoring_xy_f64(about.0, about.1)?;
    matrix.transform_point_f64((point.x, point.y), (about.x, about.y))
}

fn transform_path(
    path: &VectorPath,
    matrix: PlanarMatrix,
    about: (f64, f64),
) -> Result<VectorPath, AuthoringError> {
    let mut result = VectorPath::new();
    for command in path.commands() {
        result = match *command {
            PathCommand::MoveTo { to } => result.move_to(matrix.transform_point(to, about)?),
            PathCommand::LineTo { to } => result.line_to(matrix.transform_point(to, about)?),
            PathCommand::QuadraticTo { control, to } => result.quadratic_to(
                matrix.transform_point(control, about)?,
                matrix.transform_point(to, about)?,
            ),
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => result.cubic_to(
                matrix.transform_point(control1, about)?,
                matrix.transform_point(control2, about)?,
                matrix.transform_point(to, about)?,
            ),
            PathCommand::Close => result.close(),
        };
    }
    if let Some(target) = path.morph_target() {
        result = result.with_morph_target(transform_path(target, matrix, about)?);
    }
    Ok(result)
}

pub(crate) fn prepare_apply_matrix(
    store: &noon_core::SemanticStore,
    object: noon_core::SemanticNodeId,
    captured: noon_core::SemanticObjectState,
    values: &[f64],
    rows: usize,
    columns: usize,
    about: (f64, f64),
) -> Result<Option<PreparedPathEdits>, AuthoringError> {
    let matrix = PlanarMatrix::parse(values, rows, columns)?;
    let about = authoring_xy_f64(about.0, about.1)?;
    let about = (about.x, about.y);
    let path = world_path(store, &captured).map_err(|error| match error {
        AuthoringError::Unsupported(UnsupportedAuthoringOperation::PathQueryContent) => {
            AuthoringError::Unsupported(UnsupportedAuthoringOperation::ApplyMatrixContent)
        }
        error => error,
    })?;
    let transformed = transform_path(&path, matrix, about)?;
    if transformed == path {
        return Ok(None);
    }
    PreparedPathEdits::prepare(store, [(object, captured, transformed)]).map(Some)
}

/// Prepare a pointwise matrix for every unique leaf under one semantic family.
/// Arrow component families are rejected: their typed authoring operation must
/// transform public endpoints and rebuild the tip, rather than shear a triangle.
pub(crate) fn prepare_apply_matrix_family(
    store: &noon_core::SemanticStore,
    family: &MobjectFamily,
    values: &[f64],
    rows: usize,
    columns: usize,
    about: (f64, f64),
) -> Result<Option<PreparedPathEdits>, AuthoringError> {
    // Validate through the borrowed store passed by the caller. Re-entering
    // `MobjectFamily::validate` would borrow its RefCell again and panic when
    // preparation is already inside a scene publication borrow.
    store
        .semantic_family_checked(family.node_id())
        .map_err(AuthoringError::from)?;
    let matrix = PlanarMatrix::parse(values, rows, columns)?;
    let about = authoring_xy_f64(about.0, about.1)?;
    let about = (about.x, about.y);
    let leaves = store.ordered_leaf_nodes(family.node_id())?;
    let mut replacements = Vec::with_capacity(leaves.len());
    for node in leaves {
        let captured = store.semantic_object_state_checked(node)?.clone();
        if matches!(
            captured.role(),
            SemanticObjectRole::ArrowShaft(_)
                | SemanticObjectRole::ArrowEndTip
                | SemanticObjectRole::ArrowStartTip
        ) {
            return Err(AuthoringError::Unsupported(
                UnsupportedAuthoringOperation::ApplyMatrixContent,
            ));
        }
        let path = world_path(store, &captured).map_err(|error| match error {
            AuthoringError::Unsupported(UnsupportedAuthoringOperation::PathQueryContent) => {
                AuthoringError::Unsupported(UnsupportedAuthoringOperation::ApplyMatrixContent)
            }
            error => error,
        })?;
        replacements.push((node, captured, transform_path(&path, matrix, about)?));
    }
    let prepared = PreparedPathEdits::prepare(store, replacements)?;
    Ok(prepared.has_changes().then_some(prepared))
}

impl Mobject {
    /// Apply a Manim-compatible pointwise matrix to this geometric object's world path.
    ///
    /// The transformed path is published through the same immutable path-replacement
    /// transaction used by ordinary point editing. Object identity, paint and priority
    /// are retained while the previous world affine is baked into the replacement path.
    pub fn apply_matrix(
        &mut self,
        values: &[f64],
        rows: usize,
        columns: usize,
        about_x: f64,
        about_y: f64,
    ) -> Result<(), AuthoringError> {
        self.validate()?;
        let captured = self.state()?;
        let store = std::rc::Rc::clone(self.integration_store());
        let prepared = {
            let store = store.borrow();
            prepare_apply_matrix(
                &store,
                self.node_id(),
                captured,
                values,
                rows,
                columns,
                (about_x, about_y),
            )?
        };
        let Some(prepared) = prepared else {
            return Ok(());
        };
        let mut store_mut = store.borrow_mut();
        prepared.publish(&mut store_mut, |store, transaction| {
            transaction
                .apply(store)
                .map(|_| ())
                .map_err(AuthoringError::from)
        })
    }
}

impl MobjectFamily {
    /// Apply one pointwise matrix to every path-backed leaf through one
    /// immutable-resource replacement scope. Arrow-containing families fail
    /// closed; their semantic endpoints must be transformed through Arrow.
    pub fn apply_matrix(
        &self,
        values: &[f64],
        rows: usize,
        columns: usize,
        about_x: f64,
        about_y: f64,
    ) -> Result<(), AuthoringError> {
        self.validate()?;
        let store = std::rc::Rc::clone(self.integration_store());
        let prepared = {
            let borrowed = store.borrow();
            prepare_apply_matrix_family(&borrowed, self, values, rows, columns, (about_x, about_y))?
        };
        if let Some(prepared) = prepared {
            prepared.publish(&mut store.borrow_mut(), |store, transaction| {
                transaction
                    .apply(store)
                    .map(|_| ())
                    .map_err(AuthoringError::from)
            })?;
        }
        Ok(())
    }
}

impl Scene {
    /// Apply a pointwise matrix to all path-backed leaves of a family in one
    /// authored or live publication. Arrow families are intentionally rejected;
    /// transform their semantic endpoints and construct a fresh Arrow target.
    pub fn apply_matrix_to_family(
        &mut self,
        family: &MobjectFamily,
        values: &[f64],
        rows: usize,
        columns: usize,
        about_x: f64,
        about_y: f64,
    ) -> Result<(), AuthoringError> {
        if !std::rc::Rc::ptr_eq(family.integration_store(), self.integration_store()) {
            return Err(AuthoringError::ForeignStore);
        }
        family.validate()?;
        self.with_semantic_publication(|store, publish| {
            let Some(prepared) = prepare_apply_matrix_family(
                store,
                family,
                values,
                rows,
                columns,
                (about_x, about_y),
            )?
            else {
                return Ok(());
            };
            prepared.publish(store, publish).map(|_| ())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Scene;
    use noon_core::{AnimationOptions, GeometryResource, RateFunction, StoredGeometry};

    #[test]
    fn vector_matrix_queries_retain_semantic_precision_until_geometry_lowering() {
        let x = 1.0 + 2.0_f64.powi(-30);
        let transformed =
            transform_planar_point_about(&[2.0, 0.0, 0.0, 3.0], 2, 2, (x, -x), (0.0, 0.0)).unwrap();
        assert_eq!(transformed, (2.0 * x, -3.0 * x));
        assert_ne!(transformed.0, 2.0);
    }

    fn path_for(object: &Mobject) -> VectorPath {
        let state = object.state().unwrap();
        let store = object.integration_store().borrow();
        match state.content.geometry().unwrap() {
            StoredGeometry::Resource(handle) => {
                match store.geometry_resources().get(handle).unwrap() {
                    GeometryResource::VectorPath(path) => path.as_ref().clone(),
                    GeometryResource::Mesh(_) => {
                        panic!("2D ApplyMatrix fixture resolved to a mesh")
                    }
                }
            }
            _ => panic!("ApplyMatrix target must be path-backed"),
        }
    }

    #[test]
    fn shear_bakes_analytic_rectangle_into_world_path() {
        let mut scene = Scene::new();
        let mut rectangle = scene.rectangle(2.0, 2.0).unwrap();
        rectangle.move_to(1.0, 0.0).unwrap();
        rectangle
            .apply_matrix(&[1.0, 1.0, 0.0, 1.0], 2, 2, 0.0, 0.0)
            .unwrap();
        let state = rectangle.state().unwrap();
        assert_eq!(state.transform.translation.x, 0.0);
        assert_eq!(state.transform.translation.y, 0.0);
        assert_eq!(state.transform.scale.x, 1.0);
        assert_eq!(state.transform.scale.y, 1.0);
        assert_eq!(
            state
                .transform
                .planar_rotation()
                .expect("planar orientation"),
            0.0
        );
        let path = path_for(&rectangle);
        let points: Vec<Vec2> = path
            .commands()
            .iter()
            .filter_map(|command| match command {
                PathCommand::MoveTo { to } | PathCommand::LineTo { to } => Some(*to),
                _ => None,
            })
            .collect();
        assert!(points.contains(&Vec2::new(-1.0, -1.0)));
        assert!(points.contains(&Vec2::new(1.0, -1.0)));
        assert!(points.contains(&Vec2::new(3.0, 1.0)));
        assert!(points.contains(&Vec2::new(1.0, 1.0)));
    }

    #[test]
    fn matrix_about_point_transforms_bezier_controls_and_anchors() {
        let mut scene = Scene::new();
        let mut path = scene
            .path(
                VectorPath::new().move_to(Vec2::new(1.0, 0.0)).cubic_to(
                    Vec2::new(2.0, 0.0),
                    Vec2::new(2.0, 1.0),
                    Vec2::new(1.0, 1.0),
                ),
                Default::default(),
            )
            .unwrap();
        path.apply_matrix(&[0.0, -1.0, 1.0, 0.0], 2, 2, 1.0, 0.0)
            .unwrap();
        assert_eq!(
            path_for(&path).commands(),
            &[
                PathCommand::MoveTo {
                    to: Vec2::new(1.0, 0.0)
                },
                PathCommand::CubicTo {
                    control1: Vec2::new(1.0, 1.0),
                    control2: Vec2::new(0.0, 1.0),
                    to: Vec2::new(0.0, 0.0),
                },
            ]
        );
    }

    #[test]
    fn planar_three_by_three_matches_two_by_two_about_point() {
        let mut scene = Scene::new();
        let base = VectorPath::new()
            .move_to(Vec2::new(1.0, 0.0))
            .line_to(Vec2::new(2.0, 0.0));
        let mut two = scene.path(base.clone(), Default::default()).unwrap();
        let mut three = scene.path(base, Default::default()).unwrap();
        two.apply_matrix(&[0.0, -1.0, 1.0, 0.0], 2, 2, 1.0, 0.0)
            .unwrap();
        three
            .apply_matrix(
                &[0.0, -1.0, 7.0, 1.0, 0.0, -3.0, 0.0, 0.0, 2.0],
                3,
                3,
                1.0,
                0.0,
            )
            .unwrap();
        assert_eq!(path_for(&two), path_for(&three));
    }

    #[test]
    fn apply_matrix_transform_direct_seek_matches_forward_playback_and_completion() {
        let mut scene = Scene::new();
        let source = scene.rectangle(2.0, 2.0).unwrap();
        scene.add(&source).unwrap();
        let mut target = source.target_editor().unwrap();
        target
            .apply_matrix(&[1.0, 0.5, 0.0, 1.0], 2, 2, 0.0, 0.0)
            .unwrap();
        let animation = scene
            .declare_transform_to(
                &source,
                &target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();

        let mut direct = scene.execution_session().unwrap();
        let mut forward = scene.execution_session().unwrap();
        {
            let store = scene.integration_store().borrow();
            direct
                .activate_animation(&store, animation.node_id(), AnimationOptions::new())
                .unwrap();
            forward
                .activate_animation(&store, animation.node_id(), AnimationOptions::new())
                .unwrap();
        }
        direct.seek(1.0).unwrap();
        forward.advance_to(0.5).unwrap();
        forward.advance_to(1.0).unwrap();
        assert_eq!(direct.frame(), forward.frame());
        direct.seek(2.0).unwrap();
        forward.advance_to(2.0).unwrap();
        assert_eq!(direct.frame(), forward.frame());

        let mut completion_session = scene.execution_session().unwrap();
        {
            let mut live = scene.live(&mut completion_session);
            let segment = live.play_animation(&animation).unwrap();
            live.advance_segment_to(segment, segment.end_time())
                .unwrap();
            live.complete_segment(segment).unwrap();
        }
        assert_eq!(
            source.state().unwrap().content,
            target.state().unwrap().content
        );
        assert_eq!(
            source.state().unwrap().transform,
            target.state().unwrap().transform
        );
    }

    #[test]
    fn identity_matrix_is_a_resource_noop_and_invalid_shapes_fail_closed() {
        let mut scene = Scene::new();
        let mut path = scene
            .path(
                VectorPath::new()
                    .move_to(Vec2::ZERO)
                    .line_to(Vec2::new(2.0, 0.0)),
                Default::default(),
            )
            .unwrap();
        let before = path.state().unwrap();
        let resources = path.integration_store().borrow().geometry_resources().len();
        path.apply_matrix(&[1.0, 0.0, 0.0, 1.0], 2, 2, 0.0, 0.0)
            .unwrap();
        assert_eq!(path.state().unwrap(), before);
        assert_eq!(
            path.integration_store().borrow().geometry_resources().len(),
            resources
        );

        assert_eq!(
            path.apply_matrix(&[1.0; 6], 2, 3, 0.0, 0.0),
            Err(AuthoringError::Unsupported(
                UnsupportedAuthoringOperation::ApplyMatrixDimensions
            ))
        );
        assert!(matches!(
            path.apply_matrix(&[1.0, f64::NAN, 0.0, 1.0], 2, 2, 0.0, 0.0),
            Err(AuthoringError::InvalidRenderNumber { value, .. }) if value.is_nan()
        ));
        assert_eq!(
            path.apply_matrix(
                &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0],
                3,
                3,
                0.0,
                0.0
            ),
            Err(AuthoringError::Unsupported(
                UnsupportedAuthoringOperation::ApplyMatrixNonPlanar
            ))
        );
    }

    #[test]
    fn family_matrix_replaces_all_path_leaves_atomically() {
        let mut scene = Scene::new();
        let first = scene
            .path(
                VectorPath::new()
                    .move_to(Vec2::ZERO)
                    .line_to(Vec2::new(1.0, 0.0)),
                Default::default(),
            )
            .unwrap();
        let second = scene
            .path(
                VectorPath::new()
                    .move_to(Vec2::new(0.0, 1.0))
                    .line_to(Vec2::new(1.0, 1.0)),
                Default::default(),
            )
            .unwrap();
        let family = MobjectFamily::create(
            std::rc::Rc::clone(scene.integration_store()),
            &[(&first).into(), (&second).into()],
        )
        .unwrap();

        scene
            .apply_matrix_to_family(&family, &[1.0, 1.0, 0.0, 1.0], 2, 2, 0.0, 0.0)
            .unwrap();
        let first_path = path_for(&first);
        let second_path = path_for(&second);
        assert_eq!(
            first_path.commands()[1],
            PathCommand::LineTo {
                to: Vec2::new(1.0, 0.0)
            }
        );
        assert_eq!(
            second_path.commands()[0],
            PathCommand::MoveTo {
                to: Vec2::new(1.0, 1.0)
            }
        );
        assert_eq!(
            second_path.commands()[1],
            PathCommand::LineTo {
                to: Vec2::new(2.0, 1.0)
            }
        );
    }

    #[test]
    fn family_matrix_validation_and_arrow_policy_fail_before_resource_admission() {
        let mut scene = Scene::new();
        let line = scene
            .path(
                VectorPath::new()
                    .move_to(Vec2::ZERO)
                    .line_to(Vec2::new(1.0, 0.0)),
                Default::default(),
            )
            .unwrap();
        let family = MobjectFamily::create(
            std::rc::Rc::clone(scene.integration_store()),
            &[(&line).into()],
        )
        .unwrap();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        assert!(scene
            .apply_matrix_to_family(&family, &[1.0, f64::NAN, 0.0, 1.0], 2, 2, 0.0, 0.0)
            .is_err());
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
            resources
        );

        let arrow = scene
            .manim_arrow(crate::ManimArrowOptions::vector(2.0, 1.0).unwrap())
            .unwrap();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        assert_eq!(
            scene.apply_matrix_to_family(arrow.family(), &[1.0, 1.0, 0.0, 1.0], 2, 2, 0.0, 0.0),
            Err(AuthoringError::Unsupported(
                UnsupportedAuthoringOperation::ApplyMatrixContent
            ))
        );
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
            resources
        );
    }
}
