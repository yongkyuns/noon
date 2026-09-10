use super::*;
use crate::path_editing::{path_is_unchanged, path_replacement_state, path_transaction, PathEdit};

impl LiveSession<'_> {
    pub(super) fn publish_path_edits(
        &mut self,
        prepared: crate::path_editing::PreparedPathEdits,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let mut store = self.store.borrow_mut();
        if prepared.creates_resources() {
            self.session
                .require_resource_creation_at_root(&store, self.root)?;
        }
        prepared.publish(&mut store, |store, transaction| {
            self.session
                .apply_semantic_transaction_at_root(store, self.root, transaction)
                .map_err(LiveSessionError::from)
        })
    }

    /// Copy a selected interval from one coherent effective publication.
    pub fn subcurve(
        &mut self,
        source: &Mobject,
        a: f64,
        b: f64,
    ) -> Result<Mobject, LiveSessionError> {
        self.require_mobject(source)?;
        self.session
            .require_resource_creation_at_root(&self.store.borrow(), self.root)?;
        let captured = self.capture_mobject_state(source)?;
        let mut store = self.store.borrow_mut();
        let (mut state, path) = crate::path_editing::prepare_subcurve(&store, &captured, a, b)?;
        let mut publish = |store: &mut noon_core::SemanticStore, state| {
            self.session
                .apply_semantic_transaction_at_root(
                    store,
                    self.root,
                    crate::path_editing::subcurve_creation(state),
                )
                .map_err(LiveSessionError::from)
        };
        let result = if let Some(path) = path {
            store.with_geometry_path(path, |store, handle| {
                state.content = noon_core::StoredGeometry::Resource(handle).into();
                publish(store, state)
            })?
        } else {
            publish(&mut store, state)?
        };
        let id = crate::path_editing::created_subcurve_id(&result);
        drop(store);
        Mobject::from_node(Rc::clone(self.store), id).map_err(LiveSessionError::from)
    }

    pub fn set_points_smoothly(
        &mut self,
        object: &Mobject,
        points: &[noon_core::Vec2],
    ) -> Result<(), LiveSessionError> {
        self.edit_path(object, PathEdit::SmoothCorners(points))
    }
    pub fn make_smooth(&mut self, object: &Mobject) -> Result<(), LiveSessionError> {
        self.edit_path(object, PathEdit::AnchorMode(true))
    }
    pub fn make_jagged(&mut self, object: &Mobject) -> Result<(), LiveSessionError> {
        self.edit_path(object, PathEdit::AnchorMode(false))
    }
    /// Replace persistent world-space corners at a coherent publication boundary.
    pub fn set_points_as_corners(
        &mut self,
        object: &Mobject,
        points: &[noon_core::Vec2],
    ) -> Result<(), LiveSessionError> {
        self.edit_path(object, PathEdit::Corners(points))
    }
    pub fn start_new_path(
        &mut self,
        object: &Mobject,
        point: noon_core::Vec2,
    ) -> Result<(), LiveSessionError> {
        self.edit_path(object, PathEdit::Start(point))
    }
    pub fn add_line_to(
        &mut self,
        object: &Mobject,
        point: noon_core::Vec2,
    ) -> Result<(), LiveSessionError> {
        self.edit_path(object, PathEdit::Line(point))
    }
    pub fn add_quadratic_bezier_curve_to(
        &mut self,
        object: &Mobject,
        control: noon_core::Vec2,
        anchor: noon_core::Vec2,
    ) -> Result<(), LiveSessionError> {
        self.edit_path(object, PathEdit::Quadratic(control, anchor))
    }
    pub fn add_cubic_bezier_curve_to(
        &mut self,
        object: &Mobject,
        c1: noon_core::Vec2,
        c2: noon_core::Vec2,
        anchor: noon_core::Vec2,
    ) -> Result<(), LiveSessionError> {
        self.edit_path(object, PathEdit::Cubic(c1, c2, anchor))
    }
    pub fn close_path(&mut self, object: &Mobject) -> Result<(), LiveSessionError> {
        self.edit_path(object, PathEdit::Close)
    }
    pub fn insert_n_curves(
        &mut self,
        object: &Mobject,
        additional: usize,
    ) -> Result<(), LiveSessionError> {
        self.edit_path(object, PathEdit::Subdivide(additional))
    }
    pub fn reverse_direction(&mut self, object: &Mobject) -> Result<(), LiveSessionError> {
        self.edit_path(object, PathEdit::Reverse)
    }
    /// Capture both operands from one coherent runtime publication; active
    /// render overrides use the normal capture rejection, never stale geometry.
    pub fn pointwise_become_partial(
        &mut self,
        object: &Mobject,
        source: &Mobject,
        a: f64,
        b: f64,
    ) -> Result<(), LiveSessionError> {
        self.require_mobject(source)?;
        let (a, b) = crate::path_editing::partial_interval(a, b)?;
        let source = self.capture_mobject_state(source)?;
        self.edit_path(
            object,
            PathEdit::Partial {
                source: &source,
                a,
                b,
            },
        )
    }
    fn edit_path(&mut self, object: &Mobject, edit: PathEdit<'_>) -> Result<(), LiveSessionError> {
        self.require_mobject(object)?;
        self.session
            .require_resource_creation_at_root(&self.store.borrow(), self.root)?;
        let captured = self.capture_mobject_state(object)?;
        let before = object.state()?;
        let after = path_replacement_state(captured.clone())?;
        let mut store = self.store.borrow_mut();
        let Some(path) = edit.prepare(&store, &captured)? else {
            return Ok(());
        };
        if path_is_unchanged(&store, &before, &after, &path) {
            return Ok(());
        }
        store.with_geometry_path(path, |store, handle| {
            self.session
                .apply_semantic_transaction_at_root(
                    store,
                    self.root,
                    path_transaction(object.node_id(), &before, after, handle),
                )
                .map(|_| ())
                .map_err(LiveSessionError::from)
        })
    }
}

impl From<noon_core::GeometryResourceError> for LiveSessionError {
    fn from(error: noon_core::GeometryResourceError) -> Self {
        crate::AuthoringError::from(error).into()
    }
}

impl crate::LiveSession<'_> {
    pub fn make_family_smooth(
        &mut self,
        family: &MobjectFamily,
    ) -> Result<(), crate::LiveSessionError> {
        self.change_family_anchor_mode(family, true)
    }
    pub fn make_family_jagged(
        &mut self,
        family: &MobjectFamily,
    ) -> Result<(), crate::LiveSessionError> {
        self.change_family_anchor_mode(family, false)
    }
    fn change_family_anchor_mode(
        &mut self,
        family: &MobjectFamily,
        smooth: bool,
    ) -> Result<(), crate::LiveSessionError> {
        self.require_family(family)?;
        self.session
            .require_resource_creation_at_root(&self.store.borrow(), self.root)?;
        let nodes = self
            .store
            .borrow()
            .ordered_leaf_nodes(family.node_id())
            .map_err(crate::AuthoringError::from)?;
        let states = nodes
            .into_iter()
            .map(|node| {
                let object = Mobject::from_node(std::rc::Rc::clone(self.store), node)?;
                Ok((node, self.capture_mobject_state(&object)?))
            })
            .collect::<Result<Vec<_>, crate::LiveSessionError>>()?;
        let mut store = self.store.borrow_mut();
        crate::path_smoothing::prepare_anchor_edits(&store, states, smooth)?.publish(
            &mut store,
            |store, transaction| {
                self.session
                    .apply_semantic_transaction_at_root(store, self.root, transaction)
                    .map(|_| ())
                    .map_err(crate::LiveSessionError::from)
            },
        )
    }
}
