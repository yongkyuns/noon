from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one replacement, found {count}")
    file.write_text(text.replace(old, new, 1))


# The first prep pass introduces this shared state helper for authored point pivots.
# Make it available to the live semantic facade rather than duplicating the math.
replace_once(
    "crates/noon/src/semantic_mobject.rs",
    "fn scale_state_about_point(\n",
    "pub(crate) fn scale_state_about_point(\n",
)

replace_once(
    "crates/noon/src/live_session.rs",
    "    semantic_mobject::{authoring_render_f64, prepare_become_state, stage_state_changes},\n",
    "    semantic_mobject::{\n        authoring_render_f64, prepare_become_state, scale_state_about_center,\n        scale_state_about_point, stage_state_changes, state_center,\n    },\n",
)

replace_once(
    "crates/noon/src/live_session.rs",
    '''    /// Add a center-relative affine rotation through the shared live
    /// transaction. Pivot/layout rotation remains outside the bounded ordinary
    /// affine facade.
''',
    '''    /// Apply Manim's center-preserving scale through one coherent live publication.
    /// The raw [`Self::scale`] operation intentionally retains its origin-space affine contract.
    pub fn manim_scale(
        &mut self,
        mobject: &Mobject,
        x: f64,
        y: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.placement_authored_transform(mobject)?;
        let authored = self.authored(mobject)?;
        let mut next = authored.clone();
        let store = self.store.borrow();
        let center = state_center(&store, &next).map_err(LiveSessionError::from)?;
        scale_state_about_center(&store, &mut next, x, y, center)
            .map_err(LiveSessionError::from)?;
        drop(store);
        let mut transaction = SemanticMutationTransaction::new();
        stage_state_changes(&mut transaction, mobject.node_id(), &authored, &next);
        self.apply(transaction)
    }

    /// Apply uniform Manim scaling around one explicit world-space point.
    pub fn manim_scale_about_point(
        &mut self,
        mobject: &Mobject,
        factor: f64,
        point_x: f64,
        point_y: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.placement_authored_transform(mobject)?;
        let authored = self.authored(mobject)?;
        let mut next = authored.clone();
        scale_state_about_point(&mut next, factor, (point_x, point_y))
            .map_err(LiveSessionError::from)?;
        let mut transaction = SemanticMutationTransaction::new();
        stage_state_changes(&mut transaction, mobject.node_id(), &authored, &next);
        self.apply(transaction)
    }

    /// Resolve a Manim edge pivot from shared authored layout, then scale around it.
    pub fn manim_scale_about_edge(
        &mut self,
        mobject: &Mobject,
        factor: f64,
        direction_x: f64,
        direction_y: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.placement_authored_transform(mobject)?;
        let direction_x =
            authoring_render_f64("scale edge.x", direction_x).map_err(LiveSessionError::from)?;
        let direction_y =
            authoring_render_f64("scale edge.y", direction_y).map_err(LiveSessionError::from)?;
        let pivot = mobject
            .critical_point(direction_x, direction_y)
            .map_err(LiveSessionError::from)?;
        self.manim_scale_about_point(mobject, factor, pivot.0, pivot.1)
    }

    /// Add a center-relative affine rotation through the shared live
    /// transaction. Pivot/layout rotation remains outside the bounded ordinary
    /// affine facade.
''',
)

replace_once(
    "crates/noon-web/src/semantic_execution_player.rs",
    '''    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_scale(
        &mut self,
        mobject: &noon::Mobject,
        x: f64,
        y: f64,
    ) -> Result<(), AuthoringFailure> {
        let semantics = self
            .semantics
            .clone()
            .ok_or("execution player has no live semantic store")?;
        noon::LiveSession::new(
            &semantics,
            self.semantic_root
                .expect("live semantic store has one scene root"),
            &mut self.session,
        )
        .scale(mobject, x, y)
        .map(|_| ())
        .map_err(AuthoringFailure::from)
    }

''',
    '''    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_scale(
        &mut self,
        mobject: &noon::Mobject,
        x: f64,
        y: f64,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| live.manim_scale(mobject, x, y))
            .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_scale_about_point(
        &mut self,
        mobject: &noon::Mobject,
        factor: f64,
        point_x: f64,
        point_y: f64,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| {
            live.manim_scale_about_point(mobject, factor, point_x, point_y)
        })
        .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_scale_about_edge(
        &mut self,
        mobject: &noon::Mobject,
        factor: f64,
        direction_x: f64,
        direction_y: f64,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| {
            live.manim_scale_about_edge(mobject, factor, direction_x, direction_y)
        })
        .map(|_| ())
    }

''',
)

replace_once(
    "crates/noon-web/src/canonical_authoring_scene.rs",
    '''        #[wasm_bindgen(js_name = liveSetRotation)]
''',
    '''        #[wasm_bindgen(js_name = liveScaleAboutPoint)]
        pub fn live_scale_about_point(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            factor: f64,
            point_x: f64,
            point_y: f64,
        ) -> Result<(), JsValue> {
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_scale_about_point(handle.semantic_mobject(), factor, point_x, point_y)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveScaleAboutEdge)]
        pub fn live_scale_about_edge(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            factor: f64,
            direction_x: f64,
            direction_y: f64,
        ) -> Result<(), JsValue> {
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_scale_about_edge(handle.semantic_mobject(), factor, direction_x, direction_y)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveSetRotation)]
''',
)

replace_once(
    "web/python/_manim_semantic_handles.py",
    '''    context = _live_mutation_context(self)
    if context is not None:
        if about_point is not None or about_edge is not None:
            raise NotImplementedError(
                "canonical live affine scaling supports only scaling about the current center"
            )
        try:
            engine_call(context.liveScale, handle, value.x, value.y)
        except Exception as error:
            raise_engine_error(error)
        return self
    if about_point is not None:
''',
    '''    context = _live_mutation_context(self)
    if context is not None:
        try:
            if about_point is not None:
                if scalar is None:
                    raise TypeError("about_point requires a scalar scale_factor")
                pivot = _base._as_vec2(about_point)
                engine_call(context.liveScaleAboutPoint, handle, scalar, pivot.x, pivot.y)
            elif about_edge is not None:
                if scalar is None:
                    raise TypeError("about_edge requires a scalar scale_factor")
                edge = _base._as_vec2(about_edge)
                engine_call(context.liveScaleAboutEdge, handle, scalar, edge.x, edge.y)
            else:
                engine_call(context.liveScale, handle, value.x, value.y)
        except Exception as error:
            raise_engine_error(error)
        return self
    if about_point is not None:
''',
)

replace_once(
    "crates/noon/src/semantic_mobject/tests.rs",
    '''#[test]
fn no_op_edits_do_not_publish_and_invalid_compound_edits_roll_back() {
''',
    '''#[test]
fn live_manim_scale_matches_authored_pivots_and_rejects_invalid_pivots_atomically() {
    let scene = Scene::new();
    let path = || {
        VectorPath::new()
            .move_to(Vec2::new(1.0, -1.0))
            .line_to(Vec2::new(3.0, 1.0))
    };
    let centered = scene.path(path(), SemanticStyle::default()).unwrap();
    let point = scene.path(path(), SemanticStyle::default()).unwrap();
    let edge = scene.path(path(), SemanticStyle::default()).unwrap();
    scene.add(&centered).unwrap();
    scene.add(&point).unwrap();
    scene.add(&edge).unwrap();
    let mut session = scene.execution_session().unwrap();
    session.take_frame_changes();

    scene
        .live(&mut session)
        .manim_scale(&centered, 2.0, 2.0)
        .unwrap();
    assert_eq!(centered.center().unwrap(), (2.0, 0.0));
    assert!((centered.width().unwrap() - 4.0).abs() < 1.0e-9);
    assert_eq!(
        scene
            .live(&mut session)
            .effective_layout(&centered)
            .unwrap()
            .center,
        (2.0, 0.0)
    );

    scene
        .live(&mut session)
        .manim_scale_about_point(&point, 2.0, 0.0, 0.0)
        .unwrap();
    assert_eq!(point.center().unwrap(), (4.0, 0.0));

    let right = edge.critical_point(1.0, 0.0).unwrap();
    scene
        .live(&mut session)
        .manim_scale_about_edge(&edge, 2.0, 1.0, 0.0)
        .unwrap();
    assert_eq!(edge.critical_point(1.0, 0.0).unwrap(), right);
    assert_eq!(edge.center().unwrap(), (1.0, 0.0));

    session.take_frame_changes();
    let before_state = point.state().unwrap();
    let before_revision = scene.integration_store().borrow().scene_revision();
    let before_publication = session.publication_context();
    let before_frame = session.frame().clone();
    assert!(scene
        .live(&mut session)
        .manim_scale_about_point(&point, 2.0, f64::NAN, 0.0)
        .is_err());
    assert_eq!(point.state().unwrap(), before_state);
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        before_revision
    );
    assert_eq!(session.publication_context(), before_publication);
    assert_eq!(session.frame(), &before_frame);
    assert!(session.take_frame_changes().is_empty());
}

#[test]
fn no_op_edits_do_not_publish_and_invalid_compound_edits_roll_back() {
''',
)
