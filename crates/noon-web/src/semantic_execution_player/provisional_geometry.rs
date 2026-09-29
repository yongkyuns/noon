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
