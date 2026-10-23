//! Select existing point/content boundary operations from semantic content.
use super::*;

fn authored_content_requires_boundary(
    store: &SemanticStore,
    content: SemanticObjectContent,
) -> Result<bool, AuthoringError> {
    match content {
        SemanticObjectContent::Text(_) | SemanticObjectContent::Image(_) => Ok(true),
        SemanticObjectContent::Geometry(StoredGeometry::Resource(handle)) => {
            match store
                .geometry_resources()
                .get(handle)
                .ok_or(AuthoringError::MissingGeometryResource(handle))?
            {
                GeometryResource::VectorPath(_) => Ok(false),
                GeometryResource::Mesh(_) => Ok(true),
            }
        }
        SemanticObjectContent::Geometry(_) => Ok(false),
    }
}

fn requires_content_boundary(
    store: &SemanticStore,
    root: SemanticNodeId,
    mut read: impl FnMut(SemanticNodeId, SemanticObjectContent) -> Result<bool, AuthoringError>,
) -> Result<bool, AuthoringError> {
    for id in store.ordered_last_occurrence_authoring_nodes(root)? {
        if let Some(state) = store.node(id).and_then(|node| node.semantic_object_state()) {
            if read(id, state.content)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

impl MobjectTarget<'_> {
    /// Select retained-content layout when any addressed member has opaque
    /// text, image or mesh content. Geometry-only targets retain their exact
    /// raw-point/vector-anchor query semantics. No resources are materialized.
    pub fn requires_content_boundary(&self) -> Result<bool, AuthoringError> {
        self.validate()?;
        let store = self.integration_store().borrow();
        requires_content_boundary(&store, self.node_id(), |_, content| {
            authored_content_requires_boundary(&store, content)
        })
    }
}

/// Select the boundary operation from the same coherent content publication
/// that the subsequent layout/point query observes, including detached nodes.
pub fn effective_requires_content_boundary(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    target: MobjectTarget<'_>,
) -> Result<bool, AuthoringError> {
    target.require_store(store)?;
    target.validate()?;
    let semantics = store.borrow();
    execution.require_published_store(&semantics)?;
    requires_content_boundary(&semantics, target.node_id(), |id, authored| {
        if !execution.semantic_object_is_reachable(id) {
            return authored_content_requires_boundary(&semantics, authored);
        }
        let observed = execution.effective_semantic_object(&semantics, id)?;
        if let Some(geometry) = observed.render_geometry {
            return Ok(matches!(geometry, GeometryRef::External(_)));
        }
        Ok(match &observed.object.content {
            noon_core::ObjectContentRef::Text(_) | noon_core::ObjectContentRef::Image(_) => true,
            noon_core::ObjectContentRef::Geometry(geometry) => {
                matches!(geometry, GeometryRef::External(_))
            }
        })
    })
}

#[cfg(test)]
mod tests;
