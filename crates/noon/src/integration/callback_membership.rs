//! Unpublished semantic declarations made during a required callback.

use crate::ManimGeometryOptions;
use noon_core::{
    GeometryRef, GeometryResourceArena, GeometryResourceLookup, ObjectContentRef,
    PreparedSemanticMutationTransaction, SemanticLocalNodeToken, SemanticMutationTransaction,
    SemanticNodeCreation, SemanticNodeId, SemanticSceneMembershipRequest, SemanticStore,
    SemanticTransactionNodeRef, Style, Transform2D, Vec2, VectorPath,
};

use super::{AuthoringFailure, CallbackPhaseToken, EffectiveSemanticPropertyWrite};

/// Bound the cost of re-preflighting one callback's accumulated transaction.
pub const MAX_CALLBACK_MEMBERSHIP_STAGES: u16 = 128;

pub fn callback_provisional_key(local: SemanticLocalNodeToken) -> String {
    format!("callback-local:{local:?}")
}

/// The callback's one unpublished transaction and its phase-local handles.
pub struct CallbackMembershipCollector {
    pub token: CallbackPhaseToken,
    transaction: SemanticMutationTransaction,
    provisional_objects: Vec<SemanticLocalNodeToken>,
    admitted_provisionals: Vec<SemanticLocalNodeToken>,
    stages: u16,
}

pub enum CallbackVisualContent {
    Geometry(GeometryRef),
    Path(VectorPath),
}

pub fn callback_line_path(points: Vec<[f32; 2]>, closed: bool) -> Result<VectorPath, &'static str> {
    if !(2..=4096).contains(&points.len()) {
        return Err("callback path must contain between 2 and 4096 points");
    }
    if points.iter().flatten().any(|value| !value.is_finite()) {
        return Err("callback path points must be finite");
    }
    let mut points = points.into_iter().map(|[x, y]| Vec2::new(x, y));
    let mut path = VectorPath::new().move_to(points.next().expect("length checked"));
    for point in points {
        path = path.line_to(point);
    }
    Ok(if closed { path.close() } else { path })
}

/// Add one provisional producer's complete visual state to a callback batch.
pub fn resolve_callback_provisional(
    collector: &mut CallbackMembershipCollector,
    store: &mut SemanticStore,
    target: SemanticNodeId,
    key: &str,
    writes: &mut Vec<EffectiveSemanticPropertyWrite>,
) -> Result<CallbackVisualContent, AuthoringFailure> {
    let (transform, style, content) = collector.callback_provisional_visual(store, key)?;
    writes.extend([
        EffectiveSemanticPropertyWrite::Transform {
            object: target,
            transform,
        },
        EffectiveSemanticPropertyWrite::Style {
            object: target,
            style,
        },
    ]);
    Ok(content)
}

/// Publish callback-produced analytic shapes together with the effective writes.
pub fn commit_inline_callback_content(
    session: &mut crate::ExecutionSession,
    batch: super::EffectivePropertyBatch,
    replacements: Vec<(SemanticNodeId, GeometryRef)>,
) -> Result<(), AuthoringFailure> {
    let replacements = replacements
        .into_iter()
        .map(|(target, geometry)| (target, ObjectContentRef::Geometry(geometry)))
        .collect();
    session.commit_required_callback_phase_with_owned_contents(batch, replacements)?;
    Ok(())
}

/// Publish a retained path using a reusable producer slot. The runtime's lease
/// owns the published resource. The scratch arena only supplies a fresh ID.
pub fn commit_callback_path(
    session: &mut crate::ExecutionSession,
    arena: &mut GeometryResourceArena,
    batch: super::EffectivePropertyBatch,
    target: SemanticNodeId,
    path: VectorPath,
) -> Result<(), AuthoringFailure> {
    let mut handle = arena.insert_path(path);
    // GeometryRef contains a bare ID. Recycle the producer slot until its ID
    // does not alias an authored path or another currently active lease.
    while session
        .geometry_resources()
        .current_handle(handle.id)
        .is_some()
    {
        let resource = arena
            .remove(handle.id)
            .expect("fresh callback geometry remains in its producer arena");
        handle = arena.insert(resource);
    }
    let result =
        session.commit_required_callback_phase_with_owned_geometry(batch, target, handle, arena);
    arena
        .remove(handle.id)
        .expect("fresh callback geometry remains in its producer arena");
    result.map(|_| ()).map_err(Into::into)
}

impl CallbackMembershipCollector {
    pub fn new(token: CallbackPhaseToken) -> Self {
        Self {
            token,
            transaction: SemanticMutationTransaction::new(),
            provisional_objects: Vec::new(),
            admitted_provisionals: Vec::new(),
            stages: 0,
        }
    }

    fn prepare<'a>(
        &mut self,
        store: &'a mut SemanticStore,
    ) -> Result<PreparedSemanticMutationTransaction<'a>, AuthoringFailure> {
        match std::mem::take(&mut self.transaction).prepare_recoverable(store) {
            Ok(prepared) => Ok(prepared),
            Err((transaction, error)) => {
                self.transaction = transaction;
                Err(error.into())
            }
        }
    }

    fn require_stage_capacity(&self) -> Result<(), AuthoringFailure> {
        if self.stages == MAX_CALLBACK_MEMBERSHIP_STAGES {
            return Err("callback membership staging exceeded its bounded operation limit".into());
        }
        Ok(())
    }

    fn require_local(&self, local: SemanticLocalNodeToken) -> Result<(), AuthoringFailure> {
        if !self.provisional_objects.contains(&local) {
            return Err("callback provisional geometry token is unknown or stale".into());
        }
        Ok(())
    }

    pub fn stage_membership(
        &mut self,
        store: &mut SemanticStore,
        root: SemanticNodeId,
        request: SemanticSceneMembershipRequest<'_>,
    ) -> Result<(), AuthoringFailure> {
        self.stage_membership_edit(store, |prepared| {
            noon_core::stage_prepared_semantic_scene_membership(prepared, root, request)
        })
    }

    pub fn stage_admission(
        &mut self,
        store: &mut SemanticStore,
        root: SemanticNodeId,
        members: &[SemanticTransactionNodeRef],
    ) -> Result<(), AuthoringFailure> {
        let locals = members
            .iter()
            .filter_map(|member| match member {
                SemanticTransactionNodeRef::Pending(local) => Some(*local),
                SemanticTransactionNodeRef::Existing(_) => None,
            })
            .collect::<Vec<_>>();
        if locals.is_empty()
            || locals.iter().any(|local| {
                !self.provisional_objects.contains(local)
                    || self.admitted_provisionals.contains(local)
            })
        {
            return Err(AuthoringFailure::new(
                "invalid_input",
                "callback.membership",
                "callback provisional geometry token is unknown, stale, or already admitted",
            ));
        }
        self.stage_membership_edit(store, |prepared| {
            noon_core::stage_prepared_semantic_scene_admission(prepared, root, members)
        })?;
        self.admitted_provisionals.extend(locals);
        Ok(())
    }

    fn stage_membership_edit<'a>(
        &mut self,
        store: &'a mut SemanticStore,
        stage: impl FnOnce(
            PreparedSemanticMutationTransaction<'a>,
        ) -> Result<
            PreparedSemanticMutationTransaction<'a>,
            noon_core::PreparedSemanticMembershipError<'a>,
        >,
    ) -> Result<(), AuthoringFailure> {
        self.require_stage_capacity()?;
        match stage(self.prepare(store)?) {
            Ok(prepared) => {
                self.transaction = prepared.into_transaction();
                self.stages += 1;
                Ok(())
            }
            Err(error) => {
                let (prepared, cause) = error.into_parts();
                self.transaction = prepared.into_transaction();
                Err(match cause {
                    noon_core::PreparedSemanticMembershipErrorKind::Operation(error) => {
                        error.into()
                    }
                    noon_core::PreparedSemanticMembershipErrorKind::Transaction(error) => {
                        error.into()
                    }
                })
            }
        }
    }

    pub fn root_keys(
        &mut self,
        store: &mut SemanticStore,
        root: SemanticNodeId,
    ) -> Result<Vec<String>, AuthoringFailure> {
        let prepared = self.prepare(store)?;
        let members = prepared.family_members(root);
        self.transaction = prepared.into_transaction();
        Ok(members
            .map_err(|error| AuthoringFailure::unclassified("callback.membership_read", &error))?
            .into_iter()
            .map(|member| match member {
                SemanticTransactionNodeRef::Existing(node) => {
                    format!("{}:{}", node.slot(), node.generation())
                }
                SemanticTransactionNodeRef::Pending(local) => callback_provisional_key(local),
            })
            .collect())
    }

    pub fn prepare_final<'a>(
        &mut self,
        store: &'a mut SemanticStore,
    ) -> Result<PreparedSemanticMutationTransaction<'a>, AuthoringFailure> {
        let unadmitted = self
            .provisional_objects
            .iter()
            .copied()
            .filter(|local| !self.admitted_provisionals.contains(local))
            .collect::<Vec<_>>();
        let prepared = self.prepare(store)?;
        if unadmitted.is_empty() {
            return Ok(prepared);
        }
        prepared
            .with_pending_object_update(|transaction| {
                for local in unadmitted {
                    transaction.remove_node(local);
                }
            })
            .map_err(|(prepared, error)| {
                self.transaction = prepared.into_transaction();
                error.into()
            })
    }

    pub fn resolve_admitted(
        self,
        result: &noon_core::SemanticMutationTransactionResult,
    ) -> Vec<(SemanticLocalNodeToken, SemanticNodeId)> {
        self.admitted_provisionals
            .into_iter()
            .map(|local| {
                let node = result
                    .resolve(local)
                    .expect("a committed callback provisional object must resolve its exact token");
                (local, node)
            })
            .collect()
    }

    pub fn stage_provisional_geometry(
        &mut self,
        store: &mut SemanticStore,
        options: ManimGeometryOptions,
    ) -> Result<SemanticLocalNodeToken, AuthoringFailure> {
        let inline_state = options.inline_state()?;
        self.require_stage_capacity()?;
        let prepared = self.prepare(store)?;
        // Reuse the same recoverable extension for inline and retained paths.
        let result = prepared.with_pending_resource_object(|transaction| match inline_state {
            Some(state) => Ok(transaction.create_node(SemanticNodeCreation::object(state))),
            None => options.stage_pending_path_object(transaction),
        });
        match result {
            Ok((prepared, local)) => {
                self.transaction = prepared.into_transaction();
                self.provisional_objects.push(local);
                self.stages += 1;
                Ok(local)
            }
            Err((prepared, error)) => {
                self.transaction = prepared.into_transaction();
                Err(match error {
                    noon_core::PendingResourceExtensionError::Extension(error) => {
                        AuthoringFailure::from(error)
                    }
                    noon_core::PendingResourceExtensionError::Preflight(error) => {
                        AuthoringFailure::from(error)
                    }
                })
            }
        }
    }

    fn read_callback_provisional<T>(
        &mut self,
        store: &mut SemanticStore,
        local: SemanticLocalNodeToken,
        read: impl FnOnce(&PreparedSemanticMutationTransaction<'_>) -> Result<T, AuthoringFailure>,
    ) -> Result<T, AuthoringFailure> {
        self.require_local(local)?;
        let prepared = self.prepare(store)?;
        let result = read(&prepared);
        self.transaction = prepared.into_transaction();
        result
    }

    fn stage_callback_provisional_update(
        &mut self,
        store: &mut SemanticStore,
        local: SemanticLocalNodeToken,
        update: impl FnOnce(&mut SemanticMutationTransaction),
    ) -> Result<(), AuthoringFailure> {
        self.require_local(local)?;
        self.require_stage_capacity()?;
        match self.prepare(store)?.with_pending_object_update(update) {
            Ok(prepared) => {
                self.transaction = prepared.into_transaction();
                self.stages += 1;
                Ok(())
            }
            Err((prepared, error)) => {
                self.transaction = prepared.into_transaction();
                Err(error.into())
            }
        }
    }

    /// Read one local provisional geometry object through the callback's prepared semantic
    /// transaction. Retained paths deliberately use the typed path getters
    /// below: no provisional durable object state is fabricated for them.
    pub fn callback_provisional_object_state(
        &mut self,
        store: &mut SemanticStore,
        local: SemanticLocalNodeToken,
    ) -> Result<noon_core::SemanticObjectState, AuthoringFailure> {
        self.read_callback_provisional(store, local, |prepared| {
            prepared.proposed_object_state(local).map_err(|error| {
                AuthoringFailure::unclassified("callback.provisional_geometry_read", &error)
            })
        })
    }

    /// Convert one unadmitted inline constructor result into the same compact
    /// visual values used by ordinary semantic lowering. This is a producer
    /// snapshot, not a request to commit its temporary semantic node.
    fn callback_provisional_visual(
        &mut self,
        store: &mut SemanticStore,
        key: &str,
    ) -> Result<(Transform2D, Style, CallbackVisualContent), AuthoringFailure> {
        use noon_core::{
            SemanticMutation, SemanticObjectContent, SemanticObjectRole,
            SemanticTransactionNodeRef, StoredGeometry,
        };

        if !self.admitted_provisionals.is_empty() {
            return Err(
                "callback visual producer cannot share an authored membership publication".into(),
            );
        }
        let local = self
            .provisional_objects
            .iter()
            .copied()
            .find(|local| callback_provisional_key(*local) == key)
            .ok_or("callback visual producer token is unknown or stale")?;
        // A producer may construct several temporary objects, but no authored
        // membership or existing-object edit can be silently discarded when
        // the temporary transaction is dropped after effective publication.
        let is_provisional =
            |node: &SemanticLocalNodeToken| self.provisional_objects.contains(node);
        if self
            .transaction
            .mutations()
            .iter()
            .any(|mutation| match mutation {
                SemanticMutation::AddNode { token, .. } => !is_provisional(token),
                SemanticMutation::SetProperty {
                    object: SemanticTransactionNodeRef::Pending(local),
                    ..
                }
                | SemanticMutation::ReplaceStyle {
                    object: SemanticTransactionNodeRef::Pending(local),
                    ..
                } => !is_provisional(local),
                _ => true,
            })
        {
            return Err("callback visual producer cannot discard authored edits".into());
        }
        // A retained path lives in the transaction's pending resource table,
        // not in `SemanticObjectState`. Read that payload directly so it stays
        // callback-local until the effective-content commit admits it.
        let pending_path = self.read_callback_provisional(store, local, |prepared| {
            Ok(prepared.pending_geometry_path(local).cloned())
        })?;
        let path = match pending_path {
            Ok(path) => Some(path),
            Err(noon_core::SemanticTransactionReadError::NotPendingGeometry(_)) => None,
            Err(error) => {
                return Err(AuthoringFailure::unclassified(
                    "callback.provisional_visual",
                    &error,
                ));
            }
        };
        if let Some(path) = path {
            let mut command_count = 0usize;
            let mut current = Some(&path);
            while let Some(candidate) = current {
                if candidate.commands().len() < 2 {
                    return Err(
                        "callback provisional path chain member requires at least 2 commands"
                            .into(),
                    );
                }
                command_count = command_count
                    .checked_add(candidate.commands().len())
                    .ok_or("callback path command count overflow")?;
                if command_count > 4096 {
                    return Err("callback provisional path exceeds 4096 commands".into());
                }
                current = candidate.morph_target();
            }
            let (transform, style) = self.callback_provisional_path_visual(store, local)?;
            return Ok((transform, style, CallbackVisualContent::Path(path)));
        }

        let state = self.callback_provisional_object_state(store, local)?;
        if state.role() != SemanticObjectRole::Ordinary
            || state.z_index() != 0.0
            || state.decimal_number().is_some()
            || state.bar_metadata().is_some()
            || state.text_presentation_baseline().is_some()
            || state.click_action().is_some()
            || !state.signal_bindings().is_empty()
        {
            return Err(
                "callback visual producer returned unsupported nonvisual declarations".into(),
            );
        }
        let geometry = match state.content {
            SemanticObjectContent::Geometry(StoredGeometry::Circle { radius }) => {
                GeometryRef::circle(radius)
            }
            SemanticObjectContent::Geometry(StoredGeometry::Rectangle { size }) => {
                GeometryRef::Rectangle { size }
            }
            SemanticObjectContent::Geometry(StoredGeometry::Line { start, end }) => {
                GeometryRef::Line { start, end }
            }
            _ => return Err("callback visual producer currently requires inline circle, rectangle, or line geometry".into()),
        };
        let (transform, style) =
            noon_compile::lower_semantic_visual_values(&state).map_err(|error| {
                AuthoringFailure::unclassified("callback.provisional_visual", &error)
            })?;
        Ok((transform, style, CallbackVisualContent::Geometry(geometry)))
    }

    fn callback_provisional_path_visual(
        &mut self,
        store: &mut SemanticStore,
        local: SemanticLocalNodeToken,
    ) -> Result<(Transform2D, Style), AuthoringFailure> {
        let (transform, style) = self.read_callback_provisional(store, local, |prepared| {
            Ok((
                prepared.pending_path_transform(local).map_err(|error| {
                    AuthoringFailure::unclassified("callback.provisional_visual", &error)
                })?,
                prepared.pending_path_style(local).map_err(|error| {
                    AuthoringFailure::unclassified("callback.provisional_visual", &error)
                })?,
            ))
        })?;
        // Reuse the compiler's canonical lowering and validation for transform
        // and style values. The inline placeholder has no durable identity and
        // is discarded immediately; the actual path remains the result content.
        let mut visual =
            noon_core::SemanticObjectState::new(noon_core::StoredGeometry::Circle { radius: 0.0 });
        visual.transform = transform.into();
        visual.style = style;
        noon_compile::lower_semantic_visual_values(&visual)
            .map_err(|error| AuthoringFailure::unclassified("callback.provisional_visual", &error))
    }

    fn callback_provisional_transform(
        &mut self,
        store: &mut SemanticStore,
        local: SemanticLocalNodeToken,
    ) -> Result<noon_core::SemanticTransform, AuthoringFailure> {
        self.read_callback_provisional(store, local, |prepared| {
            prepared
                .object_state(local)
                .map(|state| state.transform)
                .or_else(|_| prepared.pending_path_transform(local).map(Into::into))
                .map_err(|error| {
                    AuthoringFailure::unclassified("callback.provisional_geometry_read", &error)
                })
        })
    }

    fn callback_provisional_style(
        &mut self,
        store: &mut SemanticStore,
        local: SemanticLocalNodeToken,
    ) -> Result<noon_core::SemanticStyle, AuthoringFailure> {
        self.read_callback_provisional(store, local, |prepared| {
            prepared
                .object_state(local)
                .map(|state| state.style.clone())
                .or_else(|_| prepared.pending_path_style(local))
                .map_err(|error| {
                    AuthoringFailure::unclassified("callback.provisional_geometry_read", &error)
                })
        })
    }

    /// Shift a phase-local provisional geometry object through the transaction's normal
    /// authored property mutation. This boundary is construction-only: regular
    /// callback targets continue to use the existing batched effective rows.
    pub fn stage_provisional_shift(
        &mut self,
        store: &mut SemanticStore,
        local: SemanticLocalNodeToken,
        x: f64,
        y: f64,
    ) -> Result<(), AuthoringFailure> {
        if !x.is_finite() || !y.is_finite() {
            return Err(AuthoringFailure::new(
                "invalid_input",
                "callback.provisional_geometry",
                "callback provisional translation must be finite",
            ));
        }
        let mut translation = self
            .callback_provisional_transform(store, local)?
            .translation;
        translation.x += x;
        translation.y += y;
        self.stage_callback_provisional_update(store, local, move |transaction| {
            transaction.replace_pending_object_property(
                local,
                noon_core::SemanticObjectProperty::Translation,
                translation,
            );
        })
    }

    /// Replace the provisional object's authored fill in the shared pending
    /// transaction. This avoids an effective-property write for an object that
    /// has no execution slot until the callback commits.
    pub fn stage_provisional_fill(
        &mut self,
        store: &mut SemanticStore,
        local: SemanticLocalNodeToken,
        components: [f64; 4],
        opacity: Option<f64>,
    ) -> Result<(), AuthoringFailure> {
        let [red, green, blue, alpha] = components;
        if components
            .into_iter()
            .any(|component| !component.is_finite() || !(0.0..=1.0).contains(&component))
            || opacity.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        {
            return Err(AuthoringFailure::new(
                "invalid_input",
                "callback.provisional_geometry",
                "callback provisional fill must use finite normalized color and opacity",
            ));
        }
        let mut style = self.callback_provisional_style(store, local)?;
        style.fill = Some(noon_core::SemanticPaint::Solid(noon_core::Color::rgba(
            red as f32,
            green as f32,
            blue as f32,
            alpha as f32,
        )));
        if let Some(opacity) = opacity {
            style.fill_opacity = opacity;
        }
        self.stage_callback_provisional_update(store, local, move |transaction| {
            transaction.replace_pending_object_style(local, style);
        })
    }

    pub fn callback_provisional_center(
        &mut self,
        store: &mut SemanticStore,
        local: SemanticLocalNodeToken,
    ) -> Result<(f64, f64), AuthoringFailure> {
        self.read_callback_provisional(store, local, |prepared| {
            if let Ok(state) = prepared.object_state(local) {
                return Ok((state.transform.translation.x, state.transform.translation.y));
            }
            let transform = prepared.pending_path_transform(local).map_err(|error| {
                AuthoringFailure::unclassified("callback.provisional_geometry_read", &error)
            })?;
            let center = prepared
                .pending_geometry_local_bounds(local)
                .map_err(|error| {
                    AuthoringFailure::unclassified("callback.provisional_geometry_read", &error)
                })?
                .map(|bounds| {
                    transform.transform_xy(
                        (f64::from(bounds.min.x) + f64::from(bounds.max.x)) * 0.5,
                        (f64::from(bounds.min.y) + f64::from(bounds.max.y)) * 0.5,
                    )
                });
            Ok(center.unwrap_or((transform.translation.x, transform.translation.y)))
        })
    }
}
