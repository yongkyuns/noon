use std::collections::{HashMap, HashSet};

use noon_core::{
    Color, ObjectId, SemanticMutationImpact, SemanticMutationTransactionResult, SemanticNodeId,
    SemanticNodeKind, SemanticObjectContent, SemanticObjectState, SemanticPaint,
    SemanticPresentation, SemanticSignalBinding, SemanticStore, SemanticStoreError, Style,
    Transform2D,
};

/// Compiler-owned identity bridge from authoritative semantic nodes to the existing
/// object-key domain consumed by `CompiledScene` and runtime execution slots.
///
/// Semantic identity remains authoritative. The `ObjectId` values stored here are
/// derived compatibility keys only; they are not written back into `SemanticStore`
/// and must not become frontend/authoring identity. #959/A4 owns deletion of this
/// bridge once the compiled/runtime path accepts semantic identities directly.
///
/// The index deliberately does not allocate a second slot domain. A compatibility
/// key is a one-to-one encoding of the semantic node's generational identity, while
/// the existing compiler/runtime remains responsible for dense/stable execution
/// slots.
#[derive(Clone, Debug, Default)]
pub struct SemanticExecutionIndex {
    object_ids: HashMap<SemanticNodeId, ObjectId>,
}

impl SemanticExecutionIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.object_ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.object_ids.is_empty()
    }

    /// Return the existing execution compatibility key for one indexed semantic
    /// object. Detached or never-lowered nodes are absent until reachability or scene
    /// lowering observes them.
    pub fn execution_object_id(&self, semantic_id: SemanticNodeId) -> Option<ObjectId> {
        self.object_ids.get(&semantic_id).copied()
    }

    /// Install identities for objects newly admitted by incremental reachability.
    pub fn apply_reachability_update(
        &mut self,
        update: &super::SemanticExecutionReachabilityUpdate,
    ) {
        for node in update.entered_objects() {
            self.ensure_object(*node);
        }
    }

    /// Apply committed A1.5 mutation impacts to the identity index without scanning
    /// unrelated semantic nodes.
    ///
    /// Detached object creation does not install an execution identity. A transaction
    /// that also admits the object installs it through [`Self::apply_reachability_update`]
    /// after commit. Structural removal deletes exactly the identities reported by the
    /// semantic transaction's reverse-reference cleanup. Property/content/subscription
    /// and family-order impacts do not change identity and therefore require no index
    /// mutation.
    pub fn apply_transaction_result(
        &mut self,
        store: &SemanticStore,
        result: &SemanticMutationTransactionResult,
    ) {
        self.apply_impacts(store, result.impacts());
    }

    pub fn apply_impacts(&mut self, _store: &SemanticStore, impacts: &[SemanticMutationImpact]) {
        for impact in impacts {
            match *impact {
                SemanticMutationImpact::NodeAdded { .. } => {}
                SemanticMutationImpact::NodeRemoved { node } => {
                    self.object_ids.remove(&node);
                }
                SemanticMutationImpact::SignalValue { .. }
                | SemanticMutationImpact::SignalTimeline { .. }
                | SemanticMutationImpact::ObjectProperty { .. }
                | SemanticMutationImpact::ObjectContent { .. }
                | SemanticMutationImpact::ObjectStyle { .. }
                | SemanticMutationImpact::Subscription { .. }
                | SemanticMutationImpact::UpdaterRegistrations { .. }
                | SemanticMutationImpact::SignalScoped { .. }
                | SemanticMutationImpact::FamilyMemberAdded { .. }
                | SemanticMutationImpact::FamilyMemberRemoved { .. }
                | SemanticMutationImpact::FamilyMemberReordered { .. }
                | SemanticMutationImpact::AnimationAdded { .. } => {}
            }
        }
    }

    /// Lower the authoritative semantic scene to the compiler/runtime value domain.
    ///
    /// Top-level scene order and family depth-first order come from `SemanticStore`.
    /// Shared/aliased leaves are emitted once at their first visible occurrence.
    /// Mixed content remains in the target `SemanticObjectContent` handle domain;
    /// high-precision transform/style values are explicitly compacted to the current
    /// f32/2D execution values. Authored native-reactive property bindings remain
    /// semantic-identity declarations for the later execution-slot lowering step.
    /// No migration-era retained-content or dense retained scene mirror participates
    /// in this boundary.
    ///
    /// Every visible object is validated and value-lowered before the identity index
    /// is mutated, so one late lowering failure cannot leave a partially updated
    /// semantic-to-execution mapping.
    pub fn lower_scene(
        &mut self,
        store: &SemanticStore,
    ) -> Result<SemanticExecutionProjection, SemanticLoweringError> {
        self.lower_roots(store, store.scene_roots())
    }

    /// Lower one semantic family as an isolated initial scene root.
    ///
    /// The family may be detached. Membership and ordering are read from the same
    /// store without attaching/detaching roots, cloning the store, or visiting other
    /// scene families. The caller must retain the originating store with `root`:
    /// a bare semantic ID is store-local, not a cross-store identity token.
    pub fn lower_root(
        &mut self,
        store: &SemanticStore,
        root: SemanticNodeId,
    ) -> Result<SemanticExecutionProjection, SemanticLoweringError> {
        let node = store
            .node(root)
            .ok_or(SemanticStoreError::UnknownNode(root))?;
        if !matches!(node.kind(), SemanticNodeKind::Family) {
            return Err(SemanticStoreError::NotFamily(root).into());
        }
        self.lower_roots(store, std::iter::once(root))
    }

    fn lower_roots(
        &mut self,
        store: &SemanticStore,
        roots: impl IntoIterator<Item = SemanticNodeId>,
    ) -> Result<SemanticExecutionProjection, SemanticLoweringError> {
        let mut pending = Vec::new();
        let mut seen = HashSet::new();

        for root in roots {
            for semantic_id in store.ordered_leaf_nodes(root)? {
                if !seen.insert(semantic_id) {
                    continue;
                }
                let state = store
                    .node(semantic_id)
                    .and_then(|node| node.semantic_object_state())
                    .ok_or(SemanticLoweringError::MissingSemanticObjectState(
                        semantic_id,
                    ))?;
                pending.push((semantic_id, lower_object_state(semantic_id, state)?));
            }
        }

        let objects = pending
            .into_iter()
            .map(|(semantic_id, state)| SemanticExecutionObject {
                semantic_id,
                execution_id: self.ensure_object(semantic_id),
                content: state.content,
                base_transform: state.base_transform,
                base_style: state.base_style,
                presentation: state.presentation,
                signal_bindings: state.signal_bindings,
            })
            .collect();

        Ok(SemanticExecutionProjection { objects })
    }

    fn ensure_object(&mut self, semantic_id: SemanticNodeId) -> ObjectId {
        *self
            .object_ids
            .entry(semantic_id)
            .or_insert_with(|| compatibility_object_id(semantic_id))
    }
}

/// Typed compiler handoff produced at the authoritative semantic -> execution
/// boundary.
///
/// This is not another runtime slot model. Objects own compact execution-facing
/// values, while stable/tombstoned slot allocation remains the responsibility of
/// the existing `CompiledScene` / `ExecutionSlotTable` path.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticExecutionProjection {
    objects: Vec<SemanticExecutionObject>,
}

impl SemanticExecutionProjection {
    pub fn objects(&self) -> &[SemanticExecutionObject] {
        &self.objects
    }

    pub fn len(&self) -> usize {
        self.objects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
}

/// One execution-facing object lowered from authoritative semantic state.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticExecutionObject {
    /// Authoritative scene-global semantic identity.
    pub semantic_id: SemanticNodeId,
    /// Temporary key accepted by the existing compiled/runtime object domain.
    pub execution_id: ObjectId,
    /// Target mixed content/resource handle, without a retained compatibility copy.
    pub content: SemanticObjectContent,
    /// Current compact 2D/f32 execution transform.
    pub base_transform: Transform2D,
    /// Current compact solid-paint execution style.
    pub base_style: Style,
    /// Stable painter-order metadata remains independent from transform/style.
    pub presentation: SemanticPresentation,
    /// Ordered authored signal drivers. Signal identity remains semantic here; the
    /// runtime consumer maps it to native reactive slots and dirty closure later.
    pub signal_bindings: Vec<SemanticSignalBinding>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticExecutionField {
    Translation,
    Scale,
    RotationZ,
    FillPaint,
    FillOpacity,
    StrokePaint,
    StrokeOpacity,
    StrokeWidth,
    ObjectOpacity,
}

impl std::fmt::Display for SemanticExecutionField {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Translation => "translation",
            Self::Scale => "scale",
            Self::RotationZ => "rotation_z",
            Self::FillPaint => "fill_paint",
            Self::FillOpacity => "fill_opacity",
            Self::StrokePaint => "stroke_paint",
            Self::StrokeOpacity => "stroke_opacity",
            Self::StrokeWidth => "stroke_width",
            Self::ObjectOpacity => "object_opacity",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticLoweringError {
    Store(SemanticStoreError),
    /// A visible object leaf came from a migration-only legacy/state-less path
    /// instead of carrying target `SemanticObjectState` directly.
    MissingSemanticObjectState(SemanticNodeId),
    NonFiniteValue {
        node: SemanticNodeId,
        field: SemanticExecutionField,
    },
    ValueOutOfRange {
        node: SemanticNodeId,
        field: SemanticExecutionField,
    },
    UnsupportedPaintResource {
        node: SemanticNodeId,
        field: SemanticExecutionField,
        resource: u64,
    },
}

impl From<SemanticStoreError> for SemanticLoweringError {
    fn from(value: SemanticStoreError) -> Self {
        Self::Store(value)
    }
}

impl std::fmt::Display for SemanticLoweringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(error) => error.fmt(formatter),
            Self::MissingSemanticObjectState(id) => write!(
                formatter,
                "semantic execution lowering requires target object state for visible node {}:{}",
                id.slot(),
                id.generation()
            ),
            Self::NonFiniteValue { node, field } => write!(
                formatter,
                "semantic object {}:{} contains non-finite {field} state",
                node.slot(),
                node.generation()
            ),
            Self::ValueOutOfRange { node, field } => write!(
                formatter,
                "semantic object {}:{} {field} cannot lower to the current f32 execution domain",
                node.slot(),
                node.generation()
            ),
            Self::UnsupportedPaintResource {
                node,
                field,
                resource,
            } => write!(
                formatter,
                "semantic object {}:{} {field} resource {resource} is not supported by the current solid-paint execution backend",
                node.slot(),
                node.generation()
            ),
        }
    }
}

impl std::error::Error for SemanticLoweringError {}

#[derive(Clone, Debug, PartialEq)]
struct LoweredObjectState {
    content: SemanticObjectContent,
    base_transform: Transform2D,
    base_style: Style,
    presentation: SemanticPresentation,
    signal_bindings: Vec<SemanticSignalBinding>,
}

fn lower_object_state(
    semantic_id: SemanticNodeId,
    state: &SemanticObjectState,
) -> Result<LoweredObjectState, SemanticLoweringError> {
    Ok(LoweredObjectState {
        content: state.content,
        base_transform: lower_semantic_transform(semantic_id, state)?,
        base_style: lower_semantic_style(semantic_id, state)?,
        presentation: state.presentation(),
        signal_bindings: state.signal_bindings().to_vec(),
    })
}

pub(super) fn lower_semantic_transform(
    node: SemanticNodeId,
    state: &SemanticObjectState,
) -> Result<Transform2D, SemanticLoweringError> {
    lower_semantic_transform_value(state).map_err(|error| error.with_node(node))
}

/// Value-lowering failure before a transaction-local node has a permanent semantic ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticExecutionValueError {
    NonFiniteValue {
        field: SemanticExecutionField,
    },
    ValueOutOfRange {
        field: SemanticExecutionField,
    },
    UnsupportedPaintResource {
        field: SemanticExecutionField,
        resource: u64,
    },
}

impl std::fmt::Display for SemanticExecutionValueError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteValue { field } => write!(formatter, "non-finite {field} state"),
            Self::ValueOutOfRange { field } => {
                write!(
                    formatter,
                    "{field} state is outside the f32 execution domain"
                )
            }
            Self::UnsupportedPaintResource { field, resource } => {
                write!(formatter, "unsupported {field} paint resource {resource}")
            }
        }
    }
}

impl std::error::Error for SemanticExecutionValueError {}

impl SemanticExecutionValueError {
    pub(super) fn with_node(self, node: SemanticNodeId) -> SemanticLoweringError {
        match self {
            Self::NonFiniteValue { field } => SemanticLoweringError::NonFiniteValue { node, field },
            Self::ValueOutOfRange { field } => {
                SemanticLoweringError::ValueOutOfRange { node, field }
            }
            Self::UnsupportedPaintResource { field, resource } => {
                SemanticLoweringError::UnsupportedPaintResource {
                    node,
                    field,
                    resource,
                }
            }
        }
    }
}

pub(crate) fn lower_semantic_transform_value(
    state: &SemanticObjectState,
) -> Result<Transform2D, SemanticExecutionValueError> {
    Ok(Transform2D {
        translation: lower_vector_xy(
            SemanticExecutionField::Translation,
            state.transform.translation,
        )?,
        scale: lower_vector_xy(SemanticExecutionField::Scale, state.transform.scale)?,
        rotation: lower_scalar_f32(
            SemanticExecutionField::RotationZ,
            state.transform.rotation_z,
        )?,
    })
}

fn lower_vector_xy(
    field: SemanticExecutionField,
    value: noon_core::SemanticVec3,
) -> Result<noon_core::Vec2, SemanticExecutionValueError> {
    value.lower_xy_f32().map_err(|error| match error {
        noon_core::SemanticLoweringError::NonFiniteVector(_) => {
            SemanticExecutionValueError::NonFiniteValue { field }
        }
        noon_core::SemanticLoweringError::CoordinateOutOfRange(_) => {
            SemanticExecutionValueError::ValueOutOfRange { field }
        }
    })
}

fn lower_scalar_f32(
    field: SemanticExecutionField,
    value: f64,
) -> Result<f32, SemanticExecutionValueError> {
    if !value.is_finite() {
        return Err(SemanticExecutionValueError::NonFiniteValue { field });
    }
    if value.abs() > f32::MAX as f64 {
        return Err(SemanticExecutionValueError::ValueOutOfRange { field });
    }
    Ok(value as f32)
}

pub(super) fn lower_semantic_style(
    node: SemanticNodeId,
    state: &SemanticObjectState,
) -> Result<Style, SemanticLoweringError> {
    lower_semantic_style_value(state).map_err(|error| error.with_node(node))
}

pub(crate) fn lower_semantic_style_value(
    state: &SemanticObjectState,
) -> Result<Style, SemanticExecutionValueError> {
    let fill = lower_paint(
        SemanticExecutionField::FillPaint,
        SemanticExecutionField::FillOpacity,
        state.style.fill.as_ref(),
        state.style.fill_opacity,
    )?;
    let stroke = lower_paint(
        SemanticExecutionField::StrokePaint,
        SemanticExecutionField::StrokeOpacity,
        state.style.stroke.as_ref(),
        state.style.stroke_opacity,
    )?;
    let stroke_width = lower_scalar_f32(
        SemanticExecutionField::StrokeWidth,
        state.style.stroke_width,
    )?;
    let opacity = lower_scalar_f32(
        SemanticExecutionField::ObjectOpacity,
        state.style.object_opacity,
    )?;

    Ok(Style {
        fill,
        stroke,
        stroke_width,
        stroke_width_mode: state.style.stroke_width_mode,
        stroke_join: state.style.stroke_join,
        stroke_cap: state.style.stroke_cap,
        opacity,
    })
}

fn lower_paint(
    paint_field: SemanticExecutionField,
    opacity_field: SemanticExecutionField,
    paint: Option<&SemanticPaint>,
    opacity: f64,
) -> Result<Option<Color>, SemanticExecutionValueError> {
    // Opacity is authored state even when paint is absent; validate it so lowering
    // never hides invalid semantic values behind a currently disabled paint.
    let opacity = lower_scalar_f32(opacity_field, opacity)? as f64;
    let Some(paint) = paint else {
        return Ok(None);
    };

    match paint {
        SemanticPaint::Solid(color) => {
            if !color.red.is_finite()
                || !color.green.is_finite()
                || !color.blue.is_finite()
                || !color.alpha.is_finite()
            {
                return Err(SemanticExecutionValueError::NonFiniteValue { field: paint_field });
            }
            let mut color = *color;
            color.alpha = lower_scalar_f32(opacity_field, f64::from(color.alpha) * opacity)?;
            Ok(Some(color))
        }
        SemanticPaint::Resource(resource) => {
            Err(SemanticExecutionValueError::UnsupportedPaintResource {
                field: paint_field,
                resource: *resource,
            })
        }
    }
}

/// One-to-one compatibility encoding for the target semantic object domain.
///
/// `SemanticNodeId` already owns generation-safe identity. Packing its two u32
/// components into the legacy u64 wrapper avoids introducing an allocator or a
/// second lifetime while the existing compiler/runtime still accepts `ObjectId`.
fn compatibility_object_id(id: SemanticNodeId) -> ObjectId {
    let raw = (u64::from(id.generation()) << 32) | u64::from(id.slot());
    ObjectId::new(raw)
}

#[cfg(test)]
mod tests {
    use noon_core::{
        Color, SemanticMutationImpact, SemanticMutationTransaction, SemanticNodeCreation,
        SemanticObjectContent, SemanticObjectProperty, SemanticObjectState, SemanticPaint,
        SemanticStore, SemanticVec3, StoredGeometry, TextResourceHandle, TextResourceId, Vec2,
    };

    use super::*;

    fn circle(radius: f32) -> SemanticObjectState {
        SemanticObjectState::new(StoredGeometry::Circle { radius })
    }

    fn text(id: u64) -> SemanticObjectState {
        SemanticObjectState::new(TextResourceHandle {
            arena: 0,
            id: TextResourceId::new(id),
            version: 0,
        })
    }

    fn attach(store: &mut SemanticStore, state: SemanticObjectState) -> SemanticNodeId {
        let id = store.insert_semantic_object(state);
        store.attach_to_scene(id).unwrap();
        id
    }

    #[test]
    fn lower_scene_preserves_mixed_content_and_family_order() {
        let mut store = SemanticStore::new();
        let geometry = store.insert_semantic_object(circle(2.0));
        let text = store.insert_semantic_object(text(7));
        let family = store.insert_family();
        store.add_member(family, geometry).unwrap();
        store.add_member(family, text).unwrap();
        store.attach_to_scene(family).unwrap();

        let mut index = SemanticExecutionIndex::new();
        let lowered = index.lower_scene(&store).unwrap();

        assert_eq!(
            lowered
                .objects()
                .iter()
                .map(|object| object.semantic_id)
                .collect::<Vec<_>>(),
            vec![geometry, text]
        );
        assert!(matches!(
            lowered.objects()[0].content,
            SemanticObjectContent::Geometry(StoredGeometry::Circle { radius: 2.0 })
        ));
        assert!(matches!(
            lowered.objects()[1].content,
            SemanticObjectContent::Text(_)
        ));
        assert_eq!(index.len(), 2);
    }

    #[test]
    fn lower_scene_compacts_transform_style_and_preserves_presentation() {
        let mut store = SemanticStore::new();
        let mut state = circle(2.0);
        state.transform.translation = SemanticVec3::new(4.5, -3.25, 12.0);
        state.transform.scale = SemanticVec3::new(2.0, 0.5, 7.0);
        state.transform.rotation_z = 0.75;
        state.style.fill = Some(SemanticPaint::Solid(Color::rgba(0.2, 0.4, 0.6, 0.8)));
        state.style.fill_opacity = 0.25;
        state.style.stroke_width = 3.5;
        state.style.stroke_join = noon_core::StrokeJoin::Bevel;
        state.style.stroke_cap = noon_core::StrokeCap::Square;
        state.style.object_opacity = 0.6;
        state.set_z_index(9);
        let object = attach(&mut store, state);

        let mut index = SemanticExecutionIndex::new();
        let lowered = index.lower_scene(&store).unwrap();
        let object_state = &lowered.objects()[0];

        assert_eq!(object_state.semantic_id, object);
        assert_eq!(
            object_state.base_transform.translation,
            Vec2::new(4.5, -3.25)
        );
        assert_eq!(object_state.base_transform.scale, Vec2::new(2.0, 0.5));
        assert_eq!(object_state.base_transform.rotation, 0.75);
        let fill = object_state.base_style.fill.unwrap();
        assert_eq!((fill.red, fill.green, fill.blue), (0.2, 0.4, 0.6));
        assert!((fill.alpha - 0.2).abs() < 1e-6);
        assert_eq!(object_state.base_style.stroke_width, 3.5);
        assert_eq!(
            object_state.base_style.stroke_join,
            noon_core::StrokeJoin::Bevel
        );
        assert_eq!(
            object_state.base_style.stroke_cap,
            noon_core::StrokeCap::Square
        );
        assert_eq!(object_state.base_style.opacity, 0.6);
        assert_eq!(object_state.presentation.z_index, 9);
        assert_eq!(object_state.presentation.insertion_order, 0);
    }

    #[test]
    fn lower_scene_preserves_ordered_native_reactive_bindings() {
        let mut store = SemanticStore::new();
        let opacity_signal = store.insert_semantic_input_signal(0.4_f64).unwrap();
        let translation_signal = store
            .insert_semantic_input_signal(SemanticVec3::new(3.0, 4.0, 5.0))
            .unwrap();
        let object = attach(&mut store, circle(1.0));
        store
            .bind_semantic_signal(
                opacity_signal,
                object,
                SemanticObjectProperty::ObjectOpacity,
            )
            .unwrap();
        store
            .bind_semantic_signal(
                translation_signal,
                object,
                SemanticObjectProperty::Translation,
            )
            .unwrap();

        let mut index = SemanticExecutionIndex::new();
        let lowered = index.lower_scene(&store).unwrap();
        let bindings = &lowered.objects()[0].signal_bindings;

        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].signal(), opacity_signal);
        assert_eq!(
            bindings[0].property(),
            SemanticObjectProperty::ObjectOpacity
        );
        assert_eq!(bindings[1].signal(), translation_signal);
        assert_eq!(bindings[1].property(), SemanticObjectProperty::Translation);
    }

    #[test]
    fn aliases_across_scene_roots_emit_one_execution_object() {
        let mut store = SemanticStore::new();
        let shared = store.insert_semantic_object(circle(1.0));
        let first = store.insert_family();
        let second = store.insert_family();
        store.add_member(first, shared).unwrap();
        store.add_member(second, shared).unwrap();
        store.attach_to_scene(first).unwrap();
        store.attach_to_scene(second).unwrap();

        let mut index = SemanticExecutionIndex::new();
        let lowered = index.lower_scene(&store).unwrap();

        assert_eq!(lowered.len(), 1);
        assert_eq!(lowered.objects()[0].semantic_id, shared);
    }

    #[test]
    fn object_mutation_impacts_preserve_execution_identity() {
        let mut store = SemanticStore::new();
        let object = attach(&mut store, circle(1.0));
        let mut index = SemanticExecutionIndex::new();
        let before = index.lower_scene(&store).unwrap().objects()[0].execution_id;

        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .set_property(object, SemanticObjectProperty::RotationZ, 0.5_f64)
            .replace_content(object, StoredGeometry::Circle { radius: 3.0 });
        let result = transaction.apply(&mut store).unwrap();
        index.apply_transaction_result(&store, &result);

        let lowered = index.lower_scene(&store).unwrap();
        let after = lowered.objects()[0].execution_id;
        assert_eq!(after, before);
        assert!(matches!(
            lowered.objects()[0].content,
            SemanticObjectContent::Geometry(StoredGeometry::Circle { radius: 3.0 })
        ));
        assert_eq!(lowered.objects()[0].base_transform.rotation, 0.5);
        assert_eq!(index.execution_object_id(object), Some(before));
    }

    #[test]
    fn family_reorder_changes_projection_order_without_identity_churn() {
        let mut store = SemanticStore::new();
        let first = store.insert_semantic_object(circle(1.0));
        let second = store.insert_semantic_object(circle(2.0));
        let third = store.insert_semantic_object(circle(3.0));
        let family = store.insert_family();
        for member in [first, second, third] {
            store.add_member(family, member).unwrap();
        }
        store.attach_to_scene(family).unwrap();

        let mut index = SemanticExecutionIndex::new();
        let initial = index
            .lower_scene(&store)
            .unwrap()
            .objects()
            .iter()
            .map(|object| (object.semantic_id, object.execution_id))
            .collect::<Vec<_>>();

        let mut transaction = SemanticMutationTransaction::new();
        transaction.reorder_member(family, third, Some(first));
        let result = transaction.apply(&mut store).unwrap();
        index.apply_transaction_result(&store, &result);

        let reordered = index
            .lower_scene(&store)
            .unwrap()
            .objects()
            .iter()
            .map(|object| (object.semantic_id, object.execution_id))
            .collect::<Vec<_>>();
        assert_eq!(
            reordered.iter().map(|entry| entry.0).collect::<Vec<_>>(),
            vec![third, first, second]
        );
        for (semantic_id, execution_id) in initial {
            assert_eq!(index.execution_object_id(semantic_id), Some(execution_id));
        }
    }

    #[test]
    fn detached_node_addition_does_not_allocate_an_execution_identity() {
        let mut store = SemanticStore::new();
        let mut index = SemanticExecutionIndex::new();

        let mut add = SemanticMutationTransaction::new();
        add.add_node(SemanticNodeCreation::object(circle(1.0)));
        let result = add.apply(&mut store).unwrap();
        let [SemanticMutationImpact::NodeAdded { node }] = result.impacts() else {
            panic!("expected one node-added impact");
        };
        index.apply_transaction_result(&store, &result);
        assert_eq!(index.execution_object_id(*node), None);
        assert!(index.is_empty());

        let old_node = *node;
        let mut remove = SemanticMutationTransaction::new();
        remove.remove_node(old_node);
        let result = remove.apply(&mut store).unwrap();
        index.apply_transaction_result(&store, &result);
        assert_eq!(index.execution_object_id(old_node), None);
        assert!(index.is_empty());

        let replacement = store.insert_semantic_object(circle(2.0));
        assert_eq!(replacement.slot(), old_node.slot());
        assert_ne!(replacement.generation(), old_node.generation());
        store.attach_to_scene(replacement).unwrap();
        let new_id = index.lower_scene(&store).unwrap().objects()[0].execution_id;
        assert_eq!(index.execution_object_id(replacement), Some(new_id));
    }

    #[test]
    fn unsupported_paint_fails_without_partial_identity_update() {
        let mut store = SemanticStore::new();
        attach(&mut store, circle(1.0));
        let mut invalid = circle(2.0);
        invalid.style.fill = Some(SemanticPaint::Resource(42));
        let invalid = attach(&mut store, invalid);

        let mut index = SemanticExecutionIndex::new();
        assert_eq!(
            index.lower_scene(&store).unwrap_err(),
            SemanticLoweringError::UnsupportedPaintResource {
                node: invalid,
                field: SemanticExecutionField::FillPaint,
                resource: 42,
            }
        );
        assert!(index.is_empty());
    }

    #[test]
    fn out_of_range_transform_fails_without_partial_identity_update() {
        let mut store = SemanticStore::new();
        let mut invalid = circle(1.0);
        invalid.transform.translation = SemanticVec3::new(f64::MAX, 0.0, 0.0);
        let invalid = attach(&mut store, invalid);

        let mut index = SemanticExecutionIndex::new();
        assert_eq!(
            index.lower_scene(&store).unwrap_err(),
            SemanticLoweringError::ValueOutOfRange {
                node: invalid,
                field: SemanticExecutionField::Translation,
            }
        );
        assert!(index.is_empty());
    }

    #[test]
    fn lowering_failure_does_not_partially_update_identity_index() {
        let mut store = SemanticStore::new();
        attach(&mut store, circle(1.0));
        let state_less = store.insert_authoring_object();
        store.attach_to_scene(state_less).unwrap();

        let mut index = SemanticExecutionIndex::new();
        assert_eq!(
            index.lower_scene(&store).unwrap_err(),
            SemanticLoweringError::MissingSemanticObjectState(state_less)
        );
        assert!(index.is_empty());
    }
}
