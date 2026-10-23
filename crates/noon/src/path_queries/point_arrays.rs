//! Shape-aware recursive arrays reuse semantic membership and coherent queries.
use super::*;
use noon_core::PointArrayShape;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PointArrayObservation {
    points: Vec<SemanticVec3>,
    shape: PointArrayShape,
}

impl PointArrayObservation {
    pub fn points(&self) -> &[SemanticVec3] {
        &self.points
    }
    pub fn shape(&self) -> &PointArrayShape {
        &self.shape
    }
    pub fn into_parts(self) -> (Vec<SemanticVec3>, PointArrayShape) {
        (self.points, self.shape)
    }

    fn from_query(query: PathQuery) -> Result<Self, AuthoringError> {
        Ok(Self {
            points: query.points_3d()?,
            shape: query.array_shape,
        })
    }

    pub(super) fn into_points(self) -> Vec<SemanticVec3> {
        self.points
    }
}

fn from_node(
    store: &SemanticStore,
    root: SemanticNodeId,
    query: &mut impl FnMut(SemanticNodeId) -> Result<PathQuery, AuthoringError>,
) -> Result<PointArrayObservation, AuthoringError> {
    struct Frame {
        shape: PointArrayShape,
        children: std::vec::IntoIter<SemanticNodeId>,
    }
    let mut arrays = Vec::new();
    let mut enter = |id| -> Result<Frame, AuthoringError> {
        let (object, children) = point_reduction_node(store, id)?;
        let own = match object {
            Some(id) => PointArrayObservation::from_query(query(id)?)?,
            None => PointArrayObservation::default(),
        };
        arrays.push(own.points);
        Ok(Frame {
            shape: own.shape,
            children: children.into_iter(),
        })
    };
    let mut stack = vec![enter(root)?];
    let shape = loop {
        if let Some(child) = stack
            .last_mut()
            .expect("active array frame")
            .children
            .next()
        {
            stack.push(enter(child)?);
            continue;
        }
        let finished = stack.pop().expect("active array frame").shape;
        if let Some(parent) = stack.last_mut() {
            parent.shape = parent.shape.concatenated(&finished)?;
        } else {
            break finished;
        }
    };
    // Shape validation follows recursive append order, but coordinate buffers
    // are flattened once in pre-order. Deep families do not recurse or copy
    // each descendant array again at every ancestor.
    Ok(PointArrayObservation {
        shape,
        points: arrays.into_iter().flatten().collect(),
    })
}

impl MobjectTarget<'_> {
    /// Merge raw family arrays in recursive order. Zero-row arrays still
    /// participate in rank and non-concatenating dimension validation.
    pub fn point_array(&self) -> Result<PointArrayObservation, AuthoringError> {
        self.validate()?;
        let store = self.integration_store().borrow();
        from_node(&store, self.node_id(), &mut |id| {
            let state = store.semantic_object_state_checked(id)?;
            let transform = state
                .transform
                .as_planar()
                .ok_or(AuthoringError::NonFiniteObjectState)?;
            prepare_content(&store, state.content, transform)
        })
    }
}

pub fn effective_point_array(
    store: &Rc<RefCell<SemanticStore>>,
    execution: &ExecutionSession,
    target: MobjectTarget<'_>,
) -> Result<PointArrayObservation, AuthoringError> {
    if !Rc::ptr_eq(store, target.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    target.validate()?;
    execution
        .require_published_store(&store.borrow())
        .map_err(AuthoringError::from)?;
    let semantics = store.borrow();
    from_node(&semantics, target.node_id(), &mut |id| {
        family_point_path_query(store, execution, id)
    })
}
