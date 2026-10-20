//! Own-point cardinality observes shared content without traversing descendants.
use super::{effective_geometry_path, PathQuery};
use crate::{AuthoringError, ExecutionSession, Mobject, MobjectTarget};
use noon_core::{SemanticObjectContent, SemanticStore, SemanticTransform2_5D};
use std::{cell::RefCell, rc::Rc};

impl MobjectTarget<'_> {
    /// Count only this root's owned Manim points, excluding all descendants.
    pub fn get_num_points(&self) -> Result<usize, AuthoringError> {
        match self {
            Self::Object(object) => object.get_num_points(),
            Self::Family(family) => {
                family.validate()?;
                let has_content = family
                    .integration_store()
                    .borrow()
                    .semantic_family_checked(family.node_id())?
                    .semantic_object_state()
                    .is_some();
                if has_content {
                    Mobject::from_node(Rc::clone(family.integration_store()), family.node_id())?
                        .get_num_points()
                } else {
                    Ok(0)
                }
            }
        }
    }
}

impl Mobject {
    /// Count this object's owned point buffer rather than its descendants.
    /// ImageMobject owns a four-corner quad; a Text root owns no glyph points.
    pub fn get_num_points(&self) -> Result<usize, AuthoringError> {
        match self.state()?.content {
            SemanticObjectContent::Image(_) => Ok(4),
            SemanticObjectContent::Text(_) => Ok(0),
            SemanticObjectContent::Geometry(_) => {
                self.local_path_query().map(|query| query.point_count())
            }
        }
    }
}

/// Count this object's own points from one coherent effective publication.
/// Geometry retains the same morph/reveal observation as a live path query;
/// image quads and text roots do not require a vector-path conversion.
pub fn effective_mobject_point_count(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    object: &Mobject,
) -> Result<usize, AuthoringError> {
    if !Rc::ptr_eq(store, object.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    object.validate()?;
    let store = store.borrow();
    let observed = execution
        .effective_semantic_object(&store, object.node_id())
        .map_err(AuthoringError::from)?;
    match observed.object.content {
        noon_core::ObjectContentRef::Image(_) => Ok(4),
        noon_core::ObjectContentRef::Text(_) => Ok(0),
        noon_core::ObjectContentRef::Geometry(_) => {
            let path = effective_geometry_path(&observed)?;
            PathQuery::prepare(&path, SemanticTransform2_5D::default())
                .map(|query| query.point_count())
        }
    }
}
