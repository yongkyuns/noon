//! Callback-local provisional geometry construction and typed reads.
//!
//! This is intentionally separate from ordinary callback effective-property
//! batching: the methods here operate only on phase-bound local declarations
//! that have no execution slot until the callback's final publication.

use super::*;

impl SemanticExecutionPlayer {
    /// Create one provisional inline or retained-path object in the exact pending callback
    /// transaction.
    ///
    /// The returned local token has no store identity and is valid only while
    /// this callback token remains pending. It permits typed wrapper code to
    /// observe the prepared declaration before the callback's one final
    /// semantic/effective publication. Retained paths keep their raw payload in
    /// this collector until that publication's scoped resource-admission suffix.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn stage_required_callback_provisional_geometry(
        &mut self,
        expected_token: CallbackPhaseToken,
        options: noon::ManimGeometryOptions,
    ) -> Result<noon_core::SemanticLocalNodeToken, AuthoringFailure> {
        let inline_state = options.inline_state().map_err(AuthoringFailure::from)?;
        let token = self
            .pending_callback_phase
            .map(|(token, _)| token)
            .ok_or("callback provisional geometry has no player pending phase")?;
        if token != expected_token {
            return Err("callback provisional geometry token is stale".into());
        }
        let semantics = self
            .semantics
            .clone()
            .ok_or("callback provisional geometry requires a live semantic store")?;
        let collector = self.callback_membership_transaction.take();
        if collector
            .as_ref()
            .is_some_and(|existing| existing.token != token)
        {
            self.callback_membership_transaction = collector;
            return Err("callback provisional geometry collector token is stale".into());
        }
        let (mut transaction, mut provisional_objects, admitted_provisionals, stages) = collector
            .map(|existing| {
                (
                    existing.transaction,
                    existing.provisional_objects,
                    existing.admitted_provisionals,
                    existing.stages,
                )
            })
            .unwrap_or_else(|| {
                (
                    SemanticMutationTransaction::new(),
                    Vec::new(),
                    Vec::new(),
                    0,
                )
            });
        if stages == MAX_CALLBACK_MEMBERSHIP_STAGES {
            self.callback_membership_transaction = Some(CallbackMembershipCollector {
                token,
                transaction,
                provisional_objects,
                admitted_provisionals,
                stages,
            });
            return Err("callback membership staging exceeded its bounded operation limit".into());
        }

        let mut store = semantics.borrow_mut();
        let prepared = match transaction.prepare_recoverable(&mut store) {
            Ok(prepared) => prepared,
            Err((transaction, error)) => {
                self.callback_membership_transaction = Some(CallbackMembershipCollector {
                    token,
                    transaction,
                    provisional_objects,
                    admitted_provisionals,
                    stages,
                });
                return Err(AuthoringFailure::from(error));
            }
        };
        // Append creation through the prepared transaction's recovery path, so
        // a caught construction failure restores the exact prior callback
        // prefix rather than retaining an orphan local-node mutation or raw path.
        let (prepared, local) = if let Some(state) = inline_state {
            let mut local = None;
            let prepared = match prepared.with_pending_object_update(|transaction| {
                local =
                    Some(transaction.create_node(noon_core::SemanticNodeCreation::object(state)));
            }) {
                Ok(prepared) => prepared,
                Err((prepared, error)) => {
                    transaction = (*prepared).into_transaction();
                    self.callback_membership_transaction = Some(CallbackMembershipCollector {
                        token,
                        transaction,
                        provisional_objects,
                        admitted_provisionals,
                        stages,
                    });
                    return Err(AuthoringFailure::from(error));
                }
            };
            (
                prepared,
                local.expect("creation closure returns its local node token"),
            )
        } else {
            match prepared.with_pending_resource_object(|transaction| {
                options.stage_pending_path_object(transaction)
            }) {
                Ok((prepared, local)) => (prepared, local),
                Err((prepared, error)) => {
                    transaction = (*prepared).into_transaction();
                    self.callback_membership_transaction = Some(CallbackMembershipCollector {
                        token,
                        transaction,
                        provisional_objects,
                        admitted_provisionals,
                        stages,
                    });
                    return Err(match error {
                        noon_core::PendingResourceExtensionError::Extension(error) => {
                            AuthoringFailure::from(error)
                        }
                        noon_core::PendingResourceExtensionError::Preflight(error) => {
                            AuthoringFailure::from(error)
                        }
                    });
                }
            }
        };
        transaction = prepared.into_transaction();
        provisional_objects.push(local);
        self.callback_membership_transaction = Some(CallbackMembershipCollector {
            token,
            transaction,
            provisional_objects,
            admitted_provisionals,
            stages: stages + 1,
        });
        Ok(local)
    }

    /// Read one phase-bound provisional declaration through its prepared
    /// transaction, then restore the collector unchanged. This is used only for
    /// construction-time fields; ordinary callback effective writes remain in
    /// their existing row batch rather than crossing this boundary per scalar.
    #[cfg(any(target_arch = "wasm32", test))]
    fn read_callback_provisional<T>(
        &mut self,
        expected_token: CallbackPhaseToken,
        local: noon_core::SemanticLocalNodeToken,
        read: impl FnOnce(
            &noon_core::PreparedSemanticMutationTransaction<'_>,
        ) -> Result<T, AuthoringFailure>,
    ) -> Result<T, AuthoringFailure> {
        let token = self
            .pending_callback_phase
            .map(|(token, _)| token)
            .ok_or("callback provisional geometry has no player pending phase")?;
        if token != expected_token {
            return Err("callback provisional geometry token is stale".into());
        }
        let semantics = self
            .semantics
            .clone()
            .ok_or("callback provisional geometry requires a live semantic store")?;
        let Some(collector) = self.callback_membership_transaction.take() else {
            return Err("callback provisional geometry is unknown".into());
        };
        if collector.token != token || !collector.provisional_objects.contains(&local) {
            self.callback_membership_transaction = Some(collector);
            return Err("callback provisional geometry token is unknown or stale".into());
        }
        let mut store = semantics.borrow_mut();
        let prepared = match collector.transaction.prepare_recoverable(&mut store) {
            Ok(prepared) => prepared,
            Err((transaction, error)) => {
                self.callback_membership_transaction = Some(CallbackMembershipCollector {
                    token,
                    transaction,
                    provisional_objects: collector.provisional_objects,
                    admitted_provisionals: collector.admitted_provisionals,
                    stages: collector.stages,
                });
                return Err(AuthoringFailure::from(error));
            }
        };
        let value = read(&prepared);
        let transaction = prepared.into_transaction();
        self.callback_membership_transaction = Some(CallbackMembershipCollector {
            token,
            transaction,
            provisional_objects: collector.provisional_objects,
            admitted_provisionals: collector.admitted_provisionals,
            stages: collector.stages,
        });
        value
    }

    /// Read one local provisional geometry object through the callback's prepared semantic
    /// transaction. Retained paths deliberately use the typed path getters
    /// below: no provisional durable object state is fabricated for them.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn callback_provisional_object_state(
        &mut self,
        expected_token: CallbackPhaseToken,
        local: noon_core::SemanticLocalNodeToken,
    ) -> Result<noon_core::SemanticObjectState, AuthoringFailure> {
        self.read_callback_provisional(expected_token, local, |prepared| {
            prepared.proposed_object_state(local).map_err(|error| {
                AuthoringFailure::unclassified("callback.provisional_geometry_read", &error)
            })
        })
    }

    /// Convert one unadmitted inline constructor result into the same compact
    /// visual values used by ordinary semantic lowering. This is a producer
    /// snapshot, not a request to commit its temporary semantic node.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(super) fn callback_provisional_visual(
        &mut self,
        token: CallbackPhaseToken,
        key: &str,
    ) -> Result<(Transform2D, Style, CallbackContentResult), AuthoringFailure> {
        use noon_core::{
            SemanticMutation, SemanticObjectContent, SemanticObjectRole,
            SemanticTransactionNodeRef, StoredGeometry,
        };

        let collector = self
            .callback_membership_transaction
            .as_ref()
            .ok_or("callback visual producer has no provisional declaration")?;
        if collector.token != token || !collector.admitted_provisionals.is_empty() {
            return Err(
                "callback visual producer cannot share an authored membership publication".into(),
            );
        }
        let local = collector
            .provisional_objects
            .iter()
            .copied()
            .find(|local| callback_provisional_key(*local) == key)
            .ok_or("callback visual producer token is unknown or stale")?;
        // A producer may construct several temporary objects, but no authored
        // membership or existing-object edit can be silently discarded when
        // the temporary transaction is dropped after effective publication.
        let is_provisional =
            |node: &noon_core::SemanticLocalNodeToken| collector.provisional_objects.contains(node);
        if collector
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
        let pending_path = self.read_callback_provisional(token, local, |prepared| {
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
            let (transform, style) = self.callback_provisional_path_visual(token, local)?;
            return Ok((transform, style, CallbackContentResult::Path(path)));
        }

        let state = self.callback_provisional_object_state(token, local)?;
        if state.role() != SemanticObjectRole::Ordinary
            || state.z_index() != 0.0
            || state.decimal_number().is_some()
            || state.bar_metadata().is_some()
            || state.text_presentation_baseline().is_some()
            || state.click_indicate().is_some()
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
        Ok((transform, style, CallbackContentResult::Geometry(geometry)))
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn callback_provisional_path_visual(
        &mut self,
        token: CallbackPhaseToken,
        local: noon_core::SemanticLocalNodeToken,
    ) -> Result<(Transform2D, Style), AuthoringFailure> {
        let (transform, style) = self.read_callback_provisional(token, local, |prepared| {
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
        visual.transform = transform;
        visual.style = style;
        noon_compile::lower_semantic_visual_values(&visual)
            .map_err(|error| AuthoringFailure::unclassified("callback.provisional_visual", &error))
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn callback_provisional_transform(
        &mut self,
        expected_token: CallbackPhaseToken,
        local: noon_core::SemanticLocalNodeToken,
    ) -> Result<noon_core::SemanticTransform2_5D, AuthoringFailure> {
        self.read_callback_provisional(expected_token, local, |prepared| {
            prepared
                .object_state(local)
                .map(|state| state.transform)
                .or_else(|_| prepared.pending_path_transform(local))
                .map_err(|error| {
                    AuthoringFailure::unclassified("callback.provisional_geometry_read", &error)
                })
        })
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn callback_provisional_style(
        &mut self,
        expected_token: CallbackPhaseToken,
        local: noon_core::SemanticLocalNodeToken,
    ) -> Result<noon_core::SemanticStyle, AuthoringFailure> {
        self.read_callback_provisional(expected_token, local, |prepared| {
            prepared
                .object_state(local)
                .map(|state| state.style.clone())
                .or_else(|_| prepared.pending_path_style(local))
                .map_err(|error| {
                    AuthoringFailure::unclassified("callback.provisional_geometry_read", &error)
                })
        })
    }

    /// Append ordinary authored mutations for one phase-local object while
    /// retaining the collector's preceding proof if the candidate fails.
    #[cfg(any(target_arch = "wasm32", test))]
    fn stage_callback_provisional_update(
        &mut self,
        expected_token: CallbackPhaseToken,
        local: noon_core::SemanticLocalNodeToken,
        update: impl FnOnce(&mut SemanticMutationTransaction),
    ) -> Result<(), AuthoringFailure> {
        let token = self
            .pending_callback_phase
            .map(|(token, _)| token)
            .ok_or("callback provisional geometry has no player pending phase")?;
        if token != expected_token {
            return Err("callback provisional geometry token is stale".into());
        }
        let semantics = self
            .semantics
            .clone()
            .ok_or("callback provisional geometry requires a live semantic store")?;
        let Some(mut collector) = self.callback_membership_transaction.take() else {
            return Err("callback provisional geometry is unknown".into());
        };
        if collector.token != token || !collector.provisional_objects.contains(&local) {
            self.callback_membership_transaction = Some(collector);
            return Err("callback provisional geometry token is unknown or stale".into());
        }
        if collector.stages == MAX_CALLBACK_MEMBERSHIP_STAGES {
            self.callback_membership_transaction = Some(collector);
            return Err("callback membership staging exceeded its bounded operation limit".into());
        }
        let transaction = std::mem::take(&mut collector.transaction);
        let mut store = semantics.borrow_mut();
        let prepared = match transaction.prepare_recoverable(&mut store) {
            Ok(prepared) => prepared,
            Err((transaction, error)) => {
                collector.transaction = transaction;
                self.callback_membership_transaction = Some(collector);
                return Err(AuthoringFailure::from(error));
            }
        };
        match prepared.with_pending_object_update(update) {
            Ok(prepared) => {
                collector.transaction = prepared.into_transaction();
                collector.stages += 1;
                self.callback_membership_transaction = Some(collector);
                Ok(())
            }
            Err((prepared, error)) => {
                collector.transaction = prepared.into_transaction();
                self.callback_membership_transaction = Some(collector);
                Err(AuthoringFailure::from(error))
            }
        }
    }

    /// Shift a phase-local provisional geometry object through the transaction's normal
    /// authored property mutation. This boundary is construction-only: regular
    /// callback targets continue to use the existing batched effective rows.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn stage_required_callback_provisional_shift(
        &mut self,
        expected_token: CallbackPhaseToken,
        local: noon_core::SemanticLocalNodeToken,
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
            .callback_provisional_transform(expected_token, local)?
            .translation;
        translation.x += x;
        translation.y += y;
        self.stage_callback_provisional_update(expected_token, local, move |transaction| {
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
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn stage_required_callback_provisional_fill(
        &mut self,
        expected_token: CallbackPhaseToken,
        local: noon_core::SemanticLocalNodeToken,
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
        let mut style = self.callback_provisional_style(expected_token, local)?;
        style.fill = Some(noon_core::SemanticPaint::Solid(noon_core::Color::rgba(
            red as f32,
            green as f32,
            blue as f32,
            alpha as f32,
        )));
        if let Some(opacity) = opacity {
            style.fill_opacity = opacity;
        }
        self.stage_callback_provisional_update(expected_token, local, move |transaction| {
            transaction.replace_pending_object_style(local, style);
        })
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn callback_provisional_center(
        &mut self,
        expected_token: CallbackPhaseToken,
        local: noon_core::SemanticLocalNodeToken,
    ) -> Result<(f64, f64), AuthoringFailure> {
        self.read_callback_provisional(expected_token, local, |prepared| {
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
