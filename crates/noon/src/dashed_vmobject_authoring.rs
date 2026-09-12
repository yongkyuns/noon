//! Shared ManimCE-compatible dashing for retained path-like Mobjects.
//!
//! Dashes are ordinary retained vector subpaths. Construction copies the source's
//! semantic style/priority into a fresh object and bakes the observed world path
//! into immutable geometry; no renderer dash primitive or frontend segmentation
//! participates in the result.

use crate::{AuthoringError, Mobject};
use noon_core::{SemanticObjectState, SemanticStore, StoredGeometry, VectorPath};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DashedVMobjectOptions {
    pub num_dashes: i32,
    pub dashed_ratio: f64,
    pub dash_offset: f64,
    pub equal_lengths: bool,
}

impl Default for DashedVMobjectOptions {
    fn default() -> Self {
        Self {
            num_dashes: 15,
            dashed_ratio: 0.5,
            dash_offset: 0.0,
            equal_lengths: true,
        }
    }
}

pub(crate) fn prepare_dashed_vmobject(
    store: &SemanticStore,
    state: &SemanticObjectState,
    options: DashedVMobjectOptions,
) -> Result<(SemanticObjectState, VectorPath), AuthoringError> {
    let dashed_ratio =
        crate::semantic_mobject::authoring_render_f64("dashed_ratio", options.dashed_ratio)?;
    if !(0.0..=1.0).contains(&dashed_ratio) {
        return Err(AuthoringError::InvalidOpacity {
            name: "dashed_ratio".to_owned(),
            value: dashed_ratio,
        });
    }
    let dash_offset =
        crate::semantic_mobject::authoring_render_f64("dash_offset", options.dash_offset)?;
    let num_dashes = options.num_dashes.max(0) as usize;
    if num_dashes == 0 {
        return Ok((
            crate::path_editing::path_replacement_state(state.clone())?,
            VectorPath::new(),
        ));
    }

    let world_path = crate::path_editing::world_path(store, state)?;
    let closed = world_path.endpoints().is_some_and(|(start, end)| {
        crate::path_queries::points_coincide(
            (f64::from(start.x), f64::from(start.y)),
            (f64::from(end.x), f64::from(end.y)),
        )
    });
    let path = noon_geometry::dashed_path(
        &world_path,
        closed,
        num_dashes,
        dashed_ratio,
        dash_offset,
        options.equal_lengths,
    )
    .map_err(AuthoringError::PathQuery)?;
    Ok((
        crate::path_editing::path_replacement_state(state.clone())?,
        path,
    ))
}

impl Mobject {
    /// Create a fresh retained dashed copy of this path-like object.
    ///
    /// The source is never mutated. The result preserves semantic style and
    /// priority while receiving a new identity and immutable vector-path content.
    pub fn dashed_vmobject(&self, options: DashedVMobjectOptions) -> Result<Self, AuthoringError> {
        let state = self.state()?;
        let mut store = self.integration_store().borrow_mut();
        let (mut state, path) = prepare_dashed_vmobject(&store, &state, options)?;
        let result = store.with_geometry_path(path, |store, handle| {
            state.content = StoredGeometry::Resource(handle).into();
            crate::path_editing::subcurve_creation(state)
                .apply(store)
                .map_err(AuthoringError::from)
        })?;
        let id = crate::path_editing::created_subcurve_id(&result);
        drop(store);
        Self::from_node(std::rc::Rc::clone(self.integration_store()), id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Scene;

    #[test]
    fn dashed_vmobject_preserves_source_state_and_creates_retained_subpaths() {
        let scene = Scene::new();
        let source = scene.circle(2.0).unwrap();
        let before = source.state().unwrap();

        let dashed = source
            .dashed_vmobject(DashedVMobjectOptions {
                num_dashes: 8,
                dashed_ratio: 0.5,
                ..Default::default()
            })
            .unwrap();

        assert_eq!(source.state().unwrap(), before);
        let dashed_state = dashed.state().unwrap();
        assert_eq!(dashed_state.style, before.style);
        assert_eq!(dashed.path_query().unwrap().subpaths().len(), 8);
        assert_ne!(dashed.node_id(), source.node_id());
    }

    #[test]
    fn nonpositive_dash_count_creates_an_empty_dashed_copy() {
        let scene = Scene::new();
        let source = scene.circle(1.0).unwrap();
        let dashed = source
            .dashed_vmobject(DashedVMobjectOptions {
                num_dashes: -3,
                ..Default::default()
            })
            .unwrap();
        assert!(matches!(
            dashed.state().unwrap().content,
            noon_core::SemanticObjectContent::Geometry(StoredGeometry::Resource(_))
        ));
    }

    #[test]
    fn invalid_parameters_fail_before_resource_or_identity_publication() {
        let scene = Scene::new();
        let source = scene.circle(1.0).unwrap();
        let revision = scene.revision();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats();

        for options in [
            DashedVMobjectOptions {
                dashed_ratio: -0.1,
                ..Default::default()
            },
            DashedVMobjectOptions {
                dashed_ratio: 1.1,
                ..Default::default()
            },
            DashedVMobjectOptions {
                dash_offset: f64::NAN,
                ..Default::default()
            },
        ] {
            assert!(source.dashed_vmobject(options).is_err());
            assert_eq!(scene.revision(), revision);
            assert_eq!(
                scene
                    .integration_store()
                    .borrow()
                    .geometry_resources()
                    .stats(),
                resources
            );
        }
    }

    #[test]
    fn live_dashing_captures_the_current_effective_world_path() {
        let mut scene = Scene::new();
        let source = scene.circle(2.0).unwrap();
        scene.add(&source).unwrap();
        let mut session = scene.execution_session().unwrap();

        scene.live(&mut session).shift(&source, 2.0, 0.0).unwrap();
        let dashed = scene
            .live(&mut session)
            .dashed_vmobject(
                &source,
                DashedVMobjectOptions {
                    num_dashes: 4,
                    dashed_ratio: 0.5,
                    ..Default::default()
                },
            )
            .unwrap();

        let start = dashed.path_query().unwrap().start().unwrap();
        assert!((start.0 - 4.0).abs() < 1e-5);
        assert!(start.1.abs() < 1e-5);
    }
}
