use std::collections::{HashMap, HashSet};

use noon_core::{
    Color, ObjectId, SemanticMutationImpact, SemanticMutationTransactionResult, SemanticNodeId,
    SemanticObjectContent, SemanticObjectState, SemanticPaint, SemanticPresentation, SemanticStore,
    SemanticStoreError, StoredGeometry, StrokeCap, StrokeJoin, Style, Transform2D, Vec2,
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
    /// object. Detached or never-lowered nodes are absent until an AddNode impact or
    /// scene lowering observes them.
    pub fn execution_object_id(&self, semantic_id: SemanticNodeId) -> Option<ObjectId> {
        self.object_ids.get(&semantic_id).copied()
    }

    /// Apply committed A1.5 mutation impacts to the identity index without scanning
    /// unrelated semantic nodes.
    ///
    /// Object creation installs only that newly allocated target object identity.
    /// Structural removal deletes exactly the identities reported by the semantic
    /// transaction's reverse-reference cleanup. Property/content/subscription and
    /// family-order impacts do not change identity and therefore require no index
    /// mutation.
    pub fn apply_transaction_result(
        &mut self,
        store: &SemanticStore,
        result: &SemanticMutationTransactionResult,
    ) {
        self.apply_impacts(store, result.impacts());
    }

    pub fn apply_impacts(&mut self, store: &SemanticStore, impacts: &[SemanticMutationImpact]) {
        for impact in impacts {
            match *impact {
                SemanticMutationImpact::NodeAdded { node } => {
                    if store
                        .node(node)
                        .and_then(|node| node.semantic_object_state())
                        .is_some()
                    {
                        self.ensure_object(node);
                    }
                }
                SemanticMutationImpact::NodeRemoved { node } => {
                    self.object_ids.remove(&node);
                }
                SemanticMutationImpact::SignalValue { .. }
                | SemanticMutationImpact::ObjectProperty { .. }
                | SemanticMutationImpact::ObjectContent { .. }
                | SemanticMutationImpact::Subscription { .. }
                | SemanticMutationImpact::FamilyMemberAdded { .. }
                | SemanticMutationImpact::FamilyMemberRemoved { .. }
                | SemanticMutationImpact::FamilyMemberReordered { .. }
                | SemanticMutationImpact::AnimationAdded { .. } => {}
            }
        }
    }

    /// Lower the current authoritative semantic scene to the typed execution handoff.
    ///
    /// Top-level scene order and family depth-first order come from `SemanticStore`.
    /// Shared/aliased leaves are emitted once at their first visible occurrence.
    /// Mixed geometry/text content remains represented by authoritative
    /// `SemanticObjectContent`, including versioned resource handles. High-precision
    /// transform/style values are lowered explicitly into the compact values already
    /// consumed by the current compiled/runtime path.
    ///
    /// All target-object and compact-value validation completes before the identity
    /// index is mutated, so a stale/state-less object, unsupported paint resource, or
    /// non-representable f64 value cannot leave a partially updated execution map.
    pub fn lower_scene<'a>(
        &mut self,
        store: &'a SemanticStore,
    ) -> Result<SemanticExecutionProjection<'a>, SemanticLoweringError> {
        let mut pending = Vec::new();
        let mut seen = HashSet::new();

        for root in store.scene_roots() {
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
                let base_transform =
                    lower_transform(state).map_err(|error| SemanticLoweringError::ObjectValue {
                        object: semantic_id,
                        error,
                    })?;
                let base_style =
                    lower_style(state).map_err(|error| SemanticLoweringError::ObjectValue {
                        object: semantic_id,
                        error,
                    })?;
                validate_content(state.content).map_err(|error| {
                    SemanticLoweringError::ObjectValue {
                        object: semantic_id,
                        error,
                    }
                })?;
                pending.push((semantic_id, state, base_transform, base_style));
            }
        }

        let objects = pending
            .into_iter()
            .map(
                |(semantic_id, state, base_transform, base_style)| SemanticExecutionObject {
                    semantic_id,
                    execution_id: self.ensure_object(semantic_id),
                    state,
                    base_transform,
                    base_style,
                },
            )
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

/// Borrowed typed handoff produced at the Semantic Scene -> execution boundary.
///
/// This is intentionally not another runtime scene/plan model: stable compiled and
/// runtime slots remain owned by `CompiledScene`/`ExecutionSlotTable`. The borrowed
/// semantic payload preserves full authored state while `base_transform` and
/// `base_style` make the current compact execution-value lowering explicit. The next
/// A1.6 slot migration can consume this handoff without routing mixed content through
/// `GeometryRef`, `RetainedObjectDefinition`, or `RetainedCompiledScene`.
#[derive(Debug)]
pub struct SemanticExecutionProjection<'a> {
    objects: Vec<SemanticExecutionObject<'a>>,
}

impl<'a> SemanticExecutionProjection<'a> {
    pub fn objects(&self) -> &[SemanticExecutionObject<'a>] {
        &self.objects
    }

    pub fn len(&self) -> usize {
        self.objects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SemanticExecutionObject<'a> {
    /// Authoritative scene-global semantic identity.
    pub semantic_id: SemanticNodeId,
    /// Temporary key accepted by the existing compiled/runtime object domain.
    pub execution_id: ObjectId,
    /// Authoritative mixed semantic payload. This remains borrowed so lowering never
    /// turns the compact execution representation back into authored truth.
    pub state: &'a SemanticObjectState,
    /// Explicit compact 2D execution transform derived from `state.transform`.
    pub base_transform: Transform2D,
    /// Explicit compact renderer-facing style derived from `state.style`.
    pub base_style: Style,
}

impl SemanticExecutionObject<'_> {
    pub const fn content(&self) -> SemanticObjectContent {
        self.state.content
    }

    pub const fn presentation(&self) -> SemanticPresentation {
        self.state.presentation()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticExecutionValueField {
    Translation,
    Scale,
    RotationZ,
    FillColor,
    FillOpacity,
    StrokeColor,
    StrokeOpacity,
    StrokeWidth,
    ObjectOpacity,
    Geometry,
}

impl std::fmt::Display for SemanticExecutionValueField {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Translation => "translation",
            Self::Scale => "scale",
            Self::RotationZ => "rotation_z",
            Self::FillColor => "fill_color",
            Self::FillOpacity => "fill_opacity",
            Self::StrokeColor => "stroke_color",
            Self::StrokeOpacity => "stroke_opacity",
            Self::StrokeWidth => "stroke_width",
            Self::ObjectOpacity => "object_opacity",
            Self::Geometry => "geometry",
        };
        formatter.write_str(name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticExecutionValueError {
    NonFinite(SemanticExecutionValueField),
    OutOfF32Range(SemanticExecutionValueField),
    UnsupportedPaintResource(u64),
}

impl std::fmt::Display for SemanticExecutionValueError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFinite(field) => write!(
                formatter,
                "semantic execution lowering requires finite {field}"
            ),
            Self::OutOfF32Range(field) => write!(
                formatter,
                "semantic execution lowering cannot represent {field} as f32"
            ),
            Self::UnsupportedPaintResource(resource) => write!(
                formatter,
                "semantic paint resource {resource} has no compact renderer representation yet"
            ),
        }
    }
}

impl std::error::Error for SemanticExecutionValueError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticLoweringError {
    Store(SemanticStoreError),
    /// A visible object leaf came from a migration-only legacy/state-less path
    /// instead of carrying target `SemanticObjectState` directly.
    MissingSemanticObjectState(SemanticNodeId),
    ObjectValue {
        object: SemanticNodeId,
        error: SemanticExecutionValueError,
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
            Self::ObjectValue { object, error } => write!(
                formatter,
                "semantic execution lowering failed for object {}:{}: {error}",
                object.slot(),
                object.generation()
            ),
        }
    }
}

impl std::error::Error for SemanticLoweringError {}

fn lower_transform(
    state: &SemanticObjectState,
) -> Result<Transform2D, SemanticExecutionValueError> {
    if !state.transform.translation.is_finite() {
        return Err(SemanticExecutionValueError::NonFinite(
            SemanticExecutionValueField::Translation,
        ));
    }
    if !state.transform.scale.is_finite() {
        return Err(SemanticExecutionValueError::NonFinite(
            SemanticExecutionValueField::Scale,
        ));
    }

    Ok(Transform2D {
        translation: Vec2::new(
            lower_f32(
                state.transform.translation.x,
                SemanticExecutionValueField::Translation,
            )?,
            lower_f32(
                state.transform.translation.y,
                SemanticExecutionValueField::Translation,
            )?,
        ),
        rotation: lower_f32(
            state.transform.rotation_z,
            SemanticExecutionValueField::RotationZ,
        )?,
        scale: Vec2::new(
            lower_f32(
                state.transform.scale.x,
                SemanticExecutionValueField::Scale,
            )?,
            lower_f32(
                state.transform.scale.y,
                SemanticExecutionValueField::Scale,
            )?,
        ),
    })
}

fn lower_style(state: &SemanticObjectState) -> Result<Style, SemanticExecutionValueError> {
    let fill = lower_paint(
        state.style.fill.as_ref(),
        state.style.fill_opacity,
        SemanticExecutionValueField::FillColor,
        SemanticExecutionValueField::FillOpacity,
    )?;
    let stroke = lower_paint(
        state.style.stroke.as_ref(),
        state.style.stroke_opacity,
        SemanticExecutionValueField::StrokeColor,
        SemanticExecutionValueField::StrokeOpacity,
    )?;

    Ok(Style {
        fill,
        stroke,
        stroke_width: lower_f32(
            state.style.stroke_width,
            SemanticExecutionValueField::StrokeWidth,
        )?,
        stroke_width_mode: state.style.stroke_width_mode,
        // Semantic style intentionally does not invent join/cap values yet. Use the
        // current renderer defaults at this explicit execution boundary.
        stroke_join: StrokeJoin::Round,
        stroke_cap: StrokeCap::Round,
        opacity: lower_f32(
            state.style.object_opacity,
            SemanticExecutionValueField::ObjectOpacity,
        )?,
    })
}

fn lower_paint(
    paint: Option<&SemanticPaint>,
    local_opacity: f64,
    color_field: SemanticExecutionValueField,
    opacity_field: SemanticExecutionValueField,
) -> Result<Option<Color>, SemanticExecutionValueError> {
    if !local_opacity.is_finite() {
        return Err(SemanticExecutionValueError::NonFinite(opacity_field));
    }

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
                return Err(SemanticExecutionValueError::NonFinite(color_field));
            }
            let mut color = *color;
            color.alpha = lower_f32(f64::from(color.alpha) * local_opacity, opacity_field)?;
            Ok(Some(color))
        }
        SemanticPaint::Resource(resource) => Err(
            SemanticExecutionValueError::UnsupportedPaintResource(*resource),
        ),
    }
}

fn validate_content(content: SemanticObjectContent) -> Result<(), SemanticExecutionValueError> {
    let finite = match content {
        SemanticObjectContent::Geometry(geometry) => match geometry {
            StoredGeometry::Circle { radius } => radius.is_finite(),
            StoredGeometry::Rectangle { size } => size.x.is_finite() && size.y.is_finite(),
            StoredGeometry::Line { start, end } => {
                start.x.is_finite()
                    && start.y.is_finite()
                    && end.x.is_finite()
                    && end.y.is_finite()
            }
            StoredGeometry::Resource(_) => true,
        },
        SemanticObjectContent::Text(_) => true,
    };
    if finite {
        Ok(())
    } else {
        Err(SemanticExecutionValueError::NonFinite(
            SemanticExecutionValueField::Geometry,
        ))
    }
}

fn lower_f32(
    value: f64,
    field: SemanticExecutionValueField,
) -> Result<f32, SemanticExecutionValueError> {
    if !value.is_finite() {
        return Err(SemanticExecutionValueError::NonFinite(field));
    }
    if value.abs() > f64::from(f32::MAX) {
        return Err(SemanticExecutionValueError::OutOfF32Range(field));
    }
    Ok(value as f32)
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
        SemanticStore, SemanticVec3, StoredGeometry, TextResourceHandle, TextResourceId,
    };

    use super::*;

    fn circle(radius: f32) -> SemanticObjectState {
        SemanticObjectState::new(StoredGeometry::Circle { radius })
    }

    fn text(id: u64) -> SemanticObjectState {
        SemanticObjectState::new(TextResourceHandle {
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
    fn lower_scene_preserves_mixed_semantic_content_and_family_order() {
        let mut store = SemanticStore::new();
        let mut geometry_state = circle(2.0);
        geometry_state.transform.translation = SemanticVec3::new(3.25, -1.5, 8.0);
        geometry_state.transform.scale = SemanticVec3::new(2.0, 0.5, 4.0);
        geometry_state.transform.rotation_z = 0.25;
        geometry_state.style.stroke = Some(SemanticPaint::Solid(Color::rgba(
            0.2, 0.4, 0.6, 0.5,
        )));
        geometry_state.style.stroke_opacity = 0.4;
        geometry_state.style.stroke_width = 2.5;
        geometry_state.style.object_opacity = 0.75;
        geometry_state.set_z_index(9);

        let geometry = store.insert_semantic_object(geometry_state);
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
            lowered.objects()[0].content(),
            SemanticObjectContent::Geometry(StoredGeometry::Circle { radius: 2.0 })
        ));
        assert!(matches!(
            lowered.objects()[1].content(),
            SemanticObjectContent::Text(_)
        ));
        assert_eq!(
            lowered.objects()[0].base_transform,
            Transform2D {
                translation: Vec2::new(3.25, -1.5),
                rotation: 0.25,
                scale: Vec2::new(2.0, 0.5),
            }
        );
        assert_eq!(lowered.objects()[0].base_style.stroke_width, 2.5);
        assert_eq!(lowered.objects()[0].base_style.opacity, 0.75);
        assert_eq!(lowered.objects()[0].base_style.stroke.unwrap().alpha, 0.2);
        assert_eq!(lowered.objects()[0].presentation().z_index, 9);
        assert_eq!(index.len(), 2);
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
        assert_eq!(lowered.objects()[0].base_transform.rotation, 0.5);
        assert!(matches!(
            lowered.objects()[0].content(),
            SemanticObjectContent::Geometry(StoredGeometry::Circle { radius: 3.0 })
        ));
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
    fn node_added_and_removed_impacts_update_only_that_identity() {
        let mut store = SemanticStore::new();
        let mut index = SemanticExecutionIndex::new();

        let mut add = SemanticMutationTransaction::new();
        add.add_node(SemanticNodeCreation::object(circle(1.0)));
        let result = add.apply(&mut store).unwrap();
        let [SemanticMutationImpact::NodeAdded { node }] = result.impacts() else {
            panic!("expected one node-added impact");
        };
        index.apply_transaction_result(&store, &result);
        let old_id = index.execution_object_id(*node).unwrap();
        assert_eq!(index.len(), 1);

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
        assert_ne!(new_id, old_id);
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

    #[test]
    fn compact_value_failure_is_atomic_with_identity_index() {
        let mut store = SemanticStore::new();
        let valid = attach(&mut store, circle(1.0));
        let mut invalid = circle(2.0);
        invalid.transform.translation.x = f64::from(f32::MAX) * 2.0;
        let invalid = attach(&mut store, invalid);

        let mut index = SemanticExecutionIndex::new();
        assert_eq!(
            index.lower_scene(&store).unwrap_err(),
            SemanticLoweringError::ObjectValue {
                object: invalid,
                error: SemanticExecutionValueError::OutOfF32Range(
                    SemanticExecutionValueField::Translation
                ),
            }
        );
        assert!(index.is_empty());
        assert_eq!(index.execution_object_id(valid), None);
    }

    #[test]
    fn unsupported_semantic_paint_fails_before_identity_commit() {
        let mut store = SemanticStore::new();
        let mut state = circle(1.0);
        state.style.fill = Some(SemanticPaint::Resource(77));
        let object = attach(&mut store, state);

        let mut index = SemanticExecutionIndex::new();
        assert_eq!(
            index.lower_scene(&store).unwrap_err(),
            SemanticLoweringError::ObjectValue {
                object,
                error: SemanticExecutionValueError::UnsupportedPaintResource(77),
            }
        );
        assert!(index.is_empty());
    }

    #[test]
    fn semantic_paint_opacity_lowers_without_collapsing_object_opacity() {
        let mut store = SemanticStore::new();
        let mut state = circle(1.0);
        state.style.fill = Some(SemanticPaint::Solid(Color::rgba(0.1, 0.2, 0.3, 0.5)));
        state.style.fill_opacity = 0.4;
        state.style.object_opacity = 0.25;
        let object = attach(&mut store, state);

        let mut index = SemanticExecutionIndex::new();
        let lowered = index.lower_scene(&store).unwrap();
        let object = lowered
            .objects()
            .iter()
            .find(|lowered| lowered.semantic_id == object)
            .unwrap();

        assert_eq!(object.base_style.fill.unwrap().alpha, 0.2);
        assert_eq!(object.base_style.opacity, 0.25);
    }

    #[test]
    fn non_finite_inline_geometry_fails_closed() {
        let mut store = SemanticStore::new();
        let object = attach(
            &mut store,
            SemanticObjectState::new(StoredGeometry::Circle { radius: f32::NAN }),
        );

        let mut index = SemanticExecutionIndex::new();
        assert_eq!(
            index.lower_scene(&store).unwrap_err(),
            SemanticLoweringError::ObjectValue {
                object,
                error: SemanticExecutionValueError::NonFinite(
                    SemanticExecutionValueField::Geometry
                ),
            }
        );
        assert!(index.is_empty());
    }
}
