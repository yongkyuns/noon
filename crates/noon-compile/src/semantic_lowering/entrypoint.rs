use noon_core::{
    Camera2DState, ComputeProgram, ExecutionRevision, FrameEpoch, ObjectId, PublicationContext,
    ReactiveProgram, SemanticNodeId, SemanticObjectRole, SemanticStore,
};

use crate::CompiledScene;

use super::{
    lower_semantic_host_callbacks, lower_semantic_reactive_projection_for_roots,
    SemanticCompiledSceneError, SemanticExecutionIndex, SemanticExecutionProjection,
    SemanticHostCallbackPlan, SemanticLoweringError, SemanticReactiveLoweringError,
    SemanticReactiveProjection,
};

/// One typed compiler handoff from the authoritative semantic scene into Noon's
/// existing execution representations.
///
/// This is a composition boundary, not a second runtime scene model: object/timeline
/// storage remains `CompiledScene`, native reactive execution remains the existing
/// compute VM, and durable runtime identity remains owned by `ExecutionSlotTable`.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticExecutionLoweringOutput {
    compiled: CompiledScene,
    reactive: SemanticReactiveProjection,
    compute: ComputeProgram,
    host_callbacks: SemanticHostCallbackPlan,
    camera_object: Option<ObjectId>,
    publication: PublicationContext,
}

impl SemanticExecutionLoweringOutput {
    pub fn compiled(&self) -> &CompiledScene {
        &self.compiled
    }

    pub fn reactive(&self) -> &SemanticReactiveProjection {
        &self.reactive
    }

    pub fn compute(&self) -> &ComputeProgram {
        &self.compute
    }

    pub fn host_callbacks(&self) -> &SemanticHostCallbackPlan {
        &self.host_callbacks
    }

    /// Publication context of the exact semantic snapshot from which this handoff
    /// was derived. Initial lowering publishes execution revision/frame epoch zero;
    /// later runtime publications advance those domains independently.
    pub const fn publication_context(&self) -> PublicationContext {
        self.publication
    }

    /// Execution identity of the unique semantic object carrying the canonical 2D
    /// camera role, if authored. The camera value itself remains derived from this
    /// object's effective runtime geometry/transform rather than duplicated here.
    pub const fn camera_object(&self) -> Option<ObjectId> {
        self.camera_object
    }

    /// Compatibility decomposition retained while callers migrate to consuming the
    /// complete canonical execution handoff.
    pub fn into_parts(self) -> (CompiledScene, SemanticReactiveProjection) {
        (self.compiled, self.reactive)
    }

    pub fn into_execution_parts(
        self,
    ) -> (CompiledScene, SemanticReactiveProjection, ComputeProgram) {
        (self.compiled, self.reactive, self.compute)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticExecutionLoweringError {
    Object(SemanticLoweringError),
    Reactive(SemanticReactiveLoweringError),
    Compiled(SemanticCompiledSceneError),
    MultipleCameraObjects {
        first: SemanticNodeId,
        second: SemanticNodeId,
    },
    InvalidCameraObject {
        node: SemanticNodeId,
    },
}

impl From<SemanticLoweringError> for SemanticExecutionLoweringError {
    fn from(value: SemanticLoweringError) -> Self {
        Self::Object(value)
    }
}

impl From<SemanticReactiveLoweringError> for SemanticExecutionLoweringError {
    fn from(value: SemanticReactiveLoweringError) -> Self {
        Self::Reactive(value)
    }
}

impl From<SemanticCompiledSceneError> for SemanticExecutionLoweringError {
    fn from(value: SemanticCompiledSceneError) -> Self {
        Self::Compiled(value)
    }
}

impl std::fmt::Display for SemanticExecutionLoweringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Object(error) => write!(formatter, "semantic object lowering failed: {error}"),
            Self::Reactive(error) => {
                write!(formatter, "semantic reactive lowering failed: {error}")
            }
            Self::Compiled(error) => {
                write!(formatter, "compiled execution lowering failed: {error}")
            }
            Self::MultipleCameraObjects { first, second } => write!(
                formatter,
                "semantic scene has multiple 2D camera objects: {}:{} and {}:{}",
                first.slot(),
                first.generation(),
                second.slot(),
                second.generation()
            ),
            Self::InvalidCameraObject { node } => write!(
                formatter,
                "semantic 2D camera object {}:{} is not a valid rectangle frame",
                node.slot(),
                node.generation()
            ),
        }
    }
}

impl std::error::Error for SemanticExecutionLoweringError {}

/// Canonical A1.6 initial-scene lowering entry point.
///
/// Object values and active native-reactive bindings are lowered from the same
/// semantic snapshot. The reactive graph is validated against the already-lowered
/// execution object/timeline domain and lowered into the existing compute VM. The
/// identity index is staged and published only after all downstream lowering
/// succeeds, including compute-program construction and camera-role validation, so
/// an invalid execution handoff cannot leave a partially admitted
/// semantic-to-execution mapping.
///
/// Authored animation declarations remain semantic intent until an explicit
/// animation activation/composition root is supplied to its dedicated lowering
/// channel; detached declarations are not implicitly scheduled by initial-scene
/// lowering.
pub fn lower_semantic_execution(
    store: &SemanticStore,
    index: &mut SemanticExecutionIndex,
) -> Result<SemanticExecutionLoweringOutput, SemanticExecutionLoweringError> {
    let roots = store.scene_roots().collect::<Vec<_>>();
    let mut staged_index = index.clone();
    let projection = staged_index.lower_scene(store)?;
    finish_semantic_execution(store, &roots, index, staged_index, projection)
}

/// Canonical initial lowering scoped to one semantic scene family.
///
/// Uses the same object/reactive/compute/camera handoff as [`lower_semantic_execution`].
/// Only the selected family's reachable leaves and their signal dependencies enter
/// execution; other attached or detached scene families remain untouched. This is
/// initial lowering, not a live membership or runtime publication operation.
/// `root` must be a live family ID from `store`; language wrappers must additionally
/// validate store ownership before passing a store-local ID across their boundary.
pub fn lower_semantic_execution_root(
    store: &SemanticStore,
    root: SemanticNodeId,
    index: &mut SemanticExecutionIndex,
) -> Result<SemanticExecutionLoweringOutput, SemanticExecutionLoweringError> {
    let mut staged_index = index.clone();
    let projection = staged_index.lower_root(store, root)?;
    finish_semantic_execution(store, &[root], index, staged_index, projection)
}

fn finish_semantic_execution(
    store: &SemanticStore,
    roots: &[SemanticNodeId],
    index: &mut SemanticExecutionIndex,
    staged_index: SemanticExecutionIndex,
    projection: SemanticExecutionProjection,
) -> Result<SemanticExecutionLoweringOutput, SemanticExecutionLoweringError> {
    let camera = semantic_camera_object(store, &projection)?;
    let reactive = lower_semantic_reactive_projection_for_roots(store, &projection, roots)?;
    let host_callbacks = lower_semantic_host_callbacks(store, roots);
    let compiled =
        CompiledScene::from_semantic_projection_after_reactive_lowering(&projection, store)?;
    let camera_object = validate_camera_object(camera, &compiled)?;
    let program = ReactiveProgram::compile_for_execution_domain(
        compiled
            .objects()
            .iter()
            .filter(|object| object.live)
            .map(|object| object.id),
        compiled.tracks_iter().map(|track| {
            let object = compiled
                .object_id_at_slot(track.object_index)
                .expect("compiled timeline track must reference a live object slot");
            (object, track.property)
        }),
        reactive.graph(),
    )
    .map_err(SemanticReactiveLoweringError::from)?;
    let compute = program
        .into_compute()
        .map_err(SemanticReactiveLoweringError::from)?;

    *index = staged_index;
    Ok(SemanticExecutionLoweringOutput {
        compiled,
        reactive,
        compute,
        host_callbacks,
        camera_object,
        publication: PublicationContext::new(
            store.scene_revision(),
            ExecutionRevision::default(),
            FrameEpoch::default(),
        ),
    })
}

fn semantic_camera_object(
    store: &SemanticStore,
    projection: &SemanticExecutionProjection,
) -> Result<Option<(SemanticNodeId, ObjectId)>, SemanticExecutionLoweringError> {
    let mut camera = None;
    for object in projection.objects() {
        let state = store
            .node(object.semantic_id)
            .and_then(|node| node.semantic_object_state())
            .expect("semantic projection object must retain authored object state");
        if state.role() != SemanticObjectRole::Camera2D {
            continue;
        }
        if let Some((first, _)) = camera {
            return Err(SemanticExecutionLoweringError::MultipleCameraObjects {
                first,
                second: object.semantic_id,
            });
        }
        camera = Some((object.semantic_id, object.execution_id));
    }
    Ok(camera)
}

fn validate_camera_object(
    camera: Option<(SemanticNodeId, ObjectId)>,
    compiled: &CompiledScene,
) -> Result<Option<ObjectId>, SemanticExecutionLoweringError> {
    let Some((node, object_id)) = camera else {
        return Ok(None);
    };
    let object = compiled
        .objects()
        .iter()
        .find(|object| object.live && object.id == object_id)
        .expect("semantic camera object must exist in its compiled projection");
    object
        .geometry()
        .and_then(|geometry| Camera2DState::from_frame_object(geometry, object.base_transform))
        .ok_or(SemanticExecutionLoweringError::InvalidCameraObject { node })?;
    Ok(Some(object_id))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use noon_core::{
        FontFaceIdentity, FontResourceArena, FontResourceLookup, GeometryResourceArena,
        GeometryResourceLookup, GlyphRun, Property, ReactiveValue, Rect,
        SemanticMutationTransaction, SemanticNodeCreation, SemanticObjectProperty,
        SemanticObjectRole, SemanticObjectState, SemanticStore, SemanticVec3, StoredGeometry,
        TextAffineTransform, TextDirection, TextRenderItem, TextResource, TextResourceLookup,
        TextSourceKind, TextVectorItem, TextVectorStyle, Vec2, VectorPath,
    };

    use super::*;

    fn circle(radius: f32) -> SemanticObjectState {
        SemanticObjectState::new(StoredGeometry::Circle { radius })
    }

    #[test]
    fn root_scoped_input_lowers_without_painter_membership_and_isolates_other_roots() {
        let mut store = SemanticStore::new();
        let selected = store.insert_family();
        let other = store.insert_family();
        let selected_signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
        let other_signal = store.insert_semantic_input_signal(2.0_f64).unwrap();
        let detached = store.insert_semantic_input_signal(3.0_f64).unwrap();
        let mut scope = SemanticMutationTransaction::new();
        scope
            .scope_signal(selected, selected_signal)
            .scope_signal(other, other_signal);
        scope.apply(&mut store).unwrap();

        let lowered =
            lower_semantic_execution_root(&store, selected, &mut SemanticExecutionIndex::new())
                .unwrap();
        assert_eq!(lowered.compiled().objects().len(), 0);
        assert!(lowered
            .reactive()
            .execution_signal_id(selected_signal)
            .is_some());
        assert!(lowered
            .reactive()
            .execution_signal_id(other_signal)
            .is_none());
        assert!(lowered.reactive().execution_signal_id(detached).is_none());

        let mut transaction = SemanticMutationTransaction::new();
        let pending = transaction.create_node(SemanticNodeCreation::input_signal(4.0_f64).unwrap());
        transaction.scope_signal(selected, pending);
        transaction.apply(&mut store).unwrap();
    }

    fn camera(center: Vec2, height: f32) -> SemanticObjectState {
        let mut state = SemanticObjectState::new(StoredGeometry::Rectangle {
            size: Vec2::new(height * 16.0 / 9.0, height),
        });
        state.transform.translation = SemanticVec3::new(center.x as f64, center.y as f64, 0.0);
        state.set_role(SemanticObjectRole::Camera2D);
        state
    }

    #[test]
    fn scoped_root_preserves_order_aliases_and_ignores_unrelated_scene_state() {
        let mut store = SemanticStore::new();
        let selected = store.insert_family();
        let nested = store.insert_family();
        let sibling = store.insert_family();
        let first = store.insert_semantic_object(circle(1.0));
        let second = store.insert_semantic_object(circle(2.0));
        let mut invalid_state = circle(1.0);
        invalid_state.transform.translation.x = f64::NAN;
        let invalid = store.insert_semantic_object(invalid_state);
        let signal = store.insert_semantic_input_signal(0.4_f64).unwrap();
        store
            .bind_semantic_signal(signal, second, SemanticObjectProperty::ObjectOpacity)
            .unwrap();
        store.add_semantic_family_member(selected, first).unwrap();
        store.add_semantic_family_member(selected, nested).unwrap();
        store.add_semantic_family_member(nested, second).unwrap();
        store.add_semantic_family_member(nested, first).unwrap();
        store.add_semantic_family_member(sibling, invalid).unwrap();
        store.attach_to_scene(sibling).unwrap();
        let roots = store.scene_roots().collect::<Vec<_>>();
        let revision = store.scene_revision();
        let mut index = SemanticExecutionIndex::new();

        let lowered = lower_semantic_execution_root(&store, selected, &mut index).unwrap();

        assert_eq!(
            lowered
                .compiled()
                .objects()
                .iter()
                .map(|object| object.id)
                .collect::<Vec<_>>(),
            [
                index.execution_object_id(first).unwrap(),
                index.execution_object_id(second).unwrap()
            ]
        );
        assert_eq!(index.len(), 2);
        assert_eq!(index.execution_object_id(invalid), None);
        assert_eq!(lowered.reactive().signal_count(), 1);
        assert_eq!(lowered.compute().signal_count(), 1);
        assert_eq!(store.scene_roots().collect::<Vec<_>>(), roots);
        assert_eq!(store.scene_revision(), revision);
        assert!(!store.node(selected).unwrap().is_scene_owned());
        // The existing all-roots entry still observes (and rejects) the unrelated
        // invalid attached family; scoped lowering does not rewrite membership.
        assert!(lower_semantic_execution(&store, &mut SemanticExecutionIndex::new()).is_err());
    }

    #[test]
    fn scoped_root_validates_family_identity_and_rejects_stale_generation() {
        let mut store = SemanticStore::new();
        let object = store.insert_semantic_object(circle(1.0));
        let stale = store.insert_family();
        store.remove_node(stale).unwrap();
        let replacement = store.insert_family();
        assert_eq!(replacement.slot(), stale.slot());
        assert_ne!(replacement.generation(), stale.generation());
        let mut index = SemanticExecutionIndex::new();
        for (root, expected) in [
            (object, noon_core::SemanticStoreError::NotFamily(object)),
            (stale, noon_core::SemanticStoreError::UnknownNode(stale)),
        ] {
            assert_eq!(
                lower_semantic_execution_root(&store, root, &mut index),
                Err(SemanticExecutionLoweringError::Object(
                    SemanticLoweringError::Store(expected)
                ))
            );
            assert!(index.is_empty());
        }
        let empty = lower_semantic_execution_root(&store, replacement, &mut index).unwrap();
        assert!(empty.compiled().objects().is_empty());
        assert!(index.is_empty());
    }

    #[test]
    fn scoped_root_failure_keeps_preexisting_execution_index_unchanged() {
        let mut store = SemanticStore::new();
        let first_root = store.insert_family();
        let first = store.insert_semantic_object(circle(1.0));
        store.add_semantic_family_member(first_root, first).unwrap();
        let second_root = store.insert_family();
        let second = store.insert_semantic_object(circle(2.0));
        store
            .add_semantic_family_member(second_root, second)
            .unwrap();
        let unlowerable = store.insert_semantic_input_signal(f64::MAX).unwrap();
        store
            .bind_semantic_signal(unlowerable, second, SemanticObjectProperty::ObjectOpacity)
            .unwrap();
        let mut index = SemanticExecutionIndex::new();
        lower_semantic_execution_root(&store, first_root, &mut index).unwrap();
        let first_id = index.execution_object_id(first).unwrap();

        assert!(matches!(
            lower_semantic_execution_root(&store, second_root, &mut index),
            Err(SemanticExecutionLoweringError::Reactive(_))
        ));
        assert_eq!(index.len(), 1);
        assert_eq!(index.execution_object_id(first), Some(first_id));
        assert_eq!(index.execution_object_id(second), None);
    }

    #[test]
    fn canonical_entry_lowers_object_values_and_reactivity_into_existing_compute_vm() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(0.4_f64).unwrap();
        let object = store.insert_semantic_object(circle(2.0));
        store.attach_to_scene(object).unwrap();
        store
            .bind_semantic_signal(signal, object, SemanticObjectProperty::ObjectOpacity)
            .unwrap();

        let mut index = SemanticExecutionIndex::new();
        let lowered = lower_semantic_execution(&store, &mut index).unwrap();
        let execution_id = index.execution_object_id(object).unwrap();
        let execution_signal = lowered.reactive().execution_signal_id(signal).unwrap();

        assert_eq!(lowered.compiled().objects().len(), 1);
        assert_eq!(lowered.compiled().objects()[0].id, execution_id);
        assert_eq!(lowered.camera_object(), None);
        assert_eq!(
            lowered.publication_context().scene_revision(),
            store.scene_revision()
        );
        assert_eq!(
            lowered.publication_context().execution_revision(),
            ExecutionRevision::default()
        );
        assert_eq!(
            lowered.publication_context().frame_epoch(),
            FrameEpoch::default()
        );
        assert_eq!(lowered.reactive().signal_count(), 1);
        assert_eq!(lowered.compute().signal_count(), 1);
        assert_eq!(
            lowered.reactive().graph().bindings()[0].object,
            execution_id
        );
        assert_eq!(
            lowered.reactive().graph().bindings()[0].property,
            Property::Opacity
        );
        assert_eq!(
            lowered.reactive().graph().bindings()[0].signal,
            execution_signal
        );

        let mut compute = lowered.compute().clone().instantiate();
        let update = compute.set_input(execution_signal, 0.7_f32).unwrap();
        assert_eq!(update.affected_objects(), vec![execution_id]);
        assert_eq!(
            update.property_changes(),
            &[noon_core::ReactivePropertyChange {
                object: execution_id,
                property: Property::Opacity,
                value: ReactiveValue::Scalar(0.7),
            }]
        );
    }

    #[test]
    fn canonical_camera_role_lowers_to_existing_execution_object_identity() {
        let mut store = SemanticStore::new();
        let camera = store.insert_semantic_object(camera(Vec2::new(2.0, -1.0), 6.0));
        store.attach_to_scene(camera).unwrap();

        let mut index = SemanticExecutionIndex::new();
        let lowered = lower_semantic_execution(&store, &mut index).unwrap();

        assert_eq!(lowered.camera_object(), index.execution_object_id(camera));
        let execution_id = lowered.camera_object().unwrap();
        let compiled = lowered
            .compiled()
            .objects()
            .iter()
            .find(|object| object.id == execution_id)
            .unwrap();
        assert_eq!(
            compiled.geometry().and_then(|geometry| {
                Camera2DState::from_frame_object(geometry, compiled.base_transform)
            }),
            Some(Camera2DState {
                center: Vec2::new(2.0, -1.0),
                height: 6.0,
            })
        );
    }

    #[test]
    fn multiple_semantic_camera_roles_fail_without_publishing_identity() {
        let mut store = SemanticStore::new();
        let first = store.insert_semantic_object(camera(Vec2::ZERO, 8.0));
        let second = store.insert_semantic_object(camera(Vec2::new(1.0, 0.0), 6.0));
        store.attach_to_scene(first).unwrap();
        store.attach_to_scene(second).unwrap();

        let mut index = SemanticExecutionIndex::new();
        assert!(matches!(
            lower_semantic_execution(&store, &mut index),
            Err(SemanticExecutionLoweringError::MultipleCameraObjects {
                first: actual_first,
                second: actual_second,
            }) if actual_first == first && actual_second == second
        ));
        assert!(index.is_empty());
    }

    #[test]
    fn non_frame_semantic_camera_fails_without_publishing_identity() {
        let mut store = SemanticStore::new();
        let mut state = circle(1.0);
        state.set_role(SemanticObjectRole::Camera2D);
        let camera = store.insert_semantic_object(state);
        store.attach_to_scene(camera).unwrap();

        let mut index = SemanticExecutionIndex::new();
        assert!(matches!(
            lower_semantic_execution(&store, &mut index),
            Err(SemanticExecutionLoweringError::InvalidCameraObject { node }) if node == camera
        ));
        assert!(index.is_empty());
    }

    #[test]
    fn reactive_failure_does_not_publish_staged_execution_identity() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(0.4_f64).unwrap();
        let object = store.insert_semantic_object(circle(1.0));
        store.attach_to_scene(object).unwrap();
        store
            .bind_semantic_signal(signal, object, SemanticObjectProperty::FillOpacity)
            .unwrap();

        let mut index = SemanticExecutionIndex::new();
        assert!(matches!(
            lower_semantic_execution(&store, &mut index),
            Err(SemanticExecutionLoweringError::Reactive(
                SemanticReactiveLoweringError::UnsupportedProperty {
                    target,
                    property: SemanticObjectProperty::FillOpacity,
                }
            )) if target == object
        ));
        assert!(index.is_empty());
    }

    #[test]
    fn text_payload_retains_only_its_dependency_closed_resources() {
        let mut store = SemanticStore::new();
        let selected_face = FontFaceIdentity {
            family: Arc::from("Selected Sans"),
            face_key: Arc::from("selected-sans-v1"),
            face_index: 0,
            variation_key: Arc::from(""),
        };
        let unrelated_face = FontFaceIdentity {
            family: Arc::from("Unrelated Sans"),
            face_key: Arc::from("unrelated-sans-v1"),
            face_index: 0,
            variation_key: Arc::from(""),
        };
        let mut fonts = FontResourceArena::new();
        fonts
            .intern_face(&selected_face, Arc::<[u8]>::from([1_u8, 2, 3]))
            .unwrap();
        fonts
            .intern_face(&unrelated_face, Arc::<[u8]>::from([4_u8, 5, 6]))
            .unwrap();
        let mut geometries = GeometryResourceArena::new();
        let selected_vector = geometries.insert_path(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(3.0, 1.0)),
        );
        let unrelated_vector = geometries.insert_path(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(99.0, 99.0)),
        );
        let handle = store
            .import_text_resource(
                TextResource {
                    source: Arc::from("hello"),
                    kind: TextSourceKind::Plain,
                    runs: Arc::from([GlyphRun {
                        font: selected_face.clone(),
                        variations: Arc::from([]),
                        font_size: 24.0,
                        direction: TextDirection::LeftToRight,
                        fill: None,
                        stroke: None,
                        transform: TextAffineTransform::IDENTITY,
                        glyphs: Arc::from([]),
                    }]),
                    vector_items: Arc::from([TextVectorItem {
                        geometry: selected_vector,
                        transform: TextAffineTransform::IDENTITY,
                        style: TextVectorStyle::default(),
                        source_span: None,
                        semantic_key: None,
                    }]),
                    render_items: Arc::from([
                        TextRenderItem::GlyphRun(0),
                        TextRenderItem::Vector(0),
                    ]),
                    parts: Arc::from([]),
                    bounds: Rect::new(Vec2::ZERO, Vec2::new(3.0, 1.0)),
                    baseline: 0.0,
                    layout_artifact: None,
                },
                &fonts,
                &geometries,
            )
            .unwrap();
        let _unrelated = store
            .import_text_resource(
                TextResource {
                    source: Arc::from("detached"),
                    kind: TextSourceKind::Plain,
                    runs: Arc::from([GlyphRun {
                        font: unrelated_face,
                        variations: Arc::from([]),
                        font_size: 24.0,
                        direction: TextDirection::LeftToRight,
                        fill: None,
                        stroke: None,
                        transform: TextAffineTransform::IDENTITY,
                        glyphs: Arc::from([]),
                    }]),
                    vector_items: Arc::from([TextVectorItem {
                        geometry: unrelated_vector,
                        transform: TextAffineTransform::IDENTITY,
                        style: TextVectorStyle::default(),
                        source_span: None,
                        semantic_key: None,
                    }]),
                    render_items: Arc::from([
                        TextRenderItem::GlyphRun(0),
                        TextRenderItem::Vector(0),
                    ]),
                    parts: Arc::from([]),
                    bounds: Rect::new(Vec2::ZERO, Vec2::ONE),
                    baseline: 0.0,
                    layout_artifact: None,
                },
                &fonts,
                &geometries,
            )
            .unwrap();
        let object = store.insert_semantic_object(SemanticObjectState::new(handle));
        store.attach_to_scene(object).unwrap();

        let mut index = SemanticExecutionIndex::new();
        let lowered = lower_semantic_execution(&store, &mut index).unwrap();
        let execution = index.execution_object_id(object).unwrap();
        assert_eq!(lowered.compiled().object_index(execution), Some(0));
        assert_eq!(lowered.compiled().objects()[0].text(), Some(handle));
        let resources = lowered.compiled().resources();
        assert_eq!(resources.text_count(), 1);
        assert_eq!(resources.font_count(), 1);
        assert_eq!(resources.geometry_count(), 1);
        let retained_text = TextResourceLookup::get(resources, handle).unwrap();
        let retained_vector = retained_text.vector_items[0].geometry;
        assert!(GeometryResourceLookup::get(resources, retained_vector).is_some());
        assert_eq!(
            GeometryResourceLookup::current_handle(resources, retained_vector.id),
            Some(retained_vector)
        );
        let retained_font = FontResourceLookup::handle_for_face(resources, &selected_face).unwrap();
        assert!(FontResourceLookup::get(resources, retained_font).is_some());
        assert_eq!(
            lowered.compiled().objects()[0].text_bounds,
            Some(Rect::new(Vec2::ZERO, Vec2::new(3.0, 1.0)))
        );
    }
}
