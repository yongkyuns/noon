//! Browser bindings for the shared callback-local semantic transaction.

use super::*;
use noon_core::SemanticLocalNodeToken;

impl SemanticExecutionPlayer {
    fn require_provisional_phase(&self, token: CallbackPhaseToken) -> Result<(), AuthoringFailure> {
        let pending = self
            .pending_callback_phase
            .map(|(token, _)| token)
            .ok_or("callback provisional geometry has no player pending phase")?;
        if pending != token {
            return Err("callback provisional geometry token is stale".into());
        }
        if self
            .callback_membership_transaction
            .as_ref()
            .is_some_and(|collector| collector.token != token)
        {
            return Err("callback provisional geometry collector token is stale".into());
        }
        Ok(())
    }

    fn with_provisional_collector<T>(
        &mut self,
        token: CallbackPhaseToken,
        use_collector: impl FnOnce(
            &mut CallbackMembershipCollector,
            &mut noon_core::SemanticStore,
        ) -> Result<T, AuthoringFailure>,
    ) -> Result<T, AuthoringFailure> {
        self.require_provisional_phase(token)?;
        let collector = self
            .callback_membership_transaction
            .as_mut()
            .ok_or("callback provisional geometry is unknown")?;
        let mut store = self.source.scene().integration_store().borrow_mut();
        use_collector(collector, &mut store)
    }

    pub(crate) fn stage_required_callback_provisional_geometry(
        &mut self,
        token: CallbackPhaseToken,
        options: noon::ManimGeometryOptions,
    ) -> Result<SemanticLocalNodeToken, AuthoringFailure> {
        self.require_provisional_phase(token)?;
        let mut store = self.source.scene().integration_store().borrow_mut();
        if let Some(collector) = &mut self.callback_membership_transaction {
            return collector.stage_provisional_geometry(&mut store, options);
        }
        let mut collector = CallbackMembershipCollector::new(token);
        let local = collector.stage_provisional_geometry(&mut store, options)?;
        self.callback_membership_transaction = Some(collector);
        Ok(local)
    }

    #[cfg(test)]
    pub(crate) fn callback_provisional_object_state(
        &mut self,
        token: CallbackPhaseToken,
        local: SemanticLocalNodeToken,
    ) -> Result<noon_core::SemanticObjectState, AuthoringFailure> {
        self.with_provisional_collector(token, |collector, store| {
            collector.callback_provisional_object_state(store, local)
        })
    }

    pub(super) fn callback_provisional_visual(
        &mut self,
        token: CallbackPhaseToken,
        target: SemanticNodeId,
        key: &str,
        writes: &mut Vec<EffectiveSemanticPropertyWrite>,
    ) -> Result<CallbackContentResult, AuthoringFailure> {
        self.with_provisional_collector(token, |collector, store| {
            let content = resolve_callback_provisional(collector, store, target, key, writes)?;
            Ok(match content {
                CallbackVisualContent::Geometry(geometry) => {
                    CallbackContentResult::Geometry(geometry)
                }
                CallbackVisualContent::Path(path) => CallbackContentResult::Path(path),
            })
        })
    }

    pub(crate) fn stage_required_callback_provisional_shift(
        &mut self,
        token: CallbackPhaseToken,
        local: SemanticLocalNodeToken,
        x: f64,
        y: f64,
    ) -> Result<(), AuthoringFailure> {
        self.with_provisional_collector(token, |collector, store| {
            collector.stage_provisional_shift(store, local, x, y)
        })
    }

    pub(crate) fn stage_required_callback_provisional_fill(
        &mut self,
        token: CallbackPhaseToken,
        local: SemanticLocalNodeToken,
        components: [f64; 4],
        opacity: Option<f64>,
    ) -> Result<(), AuthoringFailure> {
        self.with_provisional_collector(token, |collector, store| {
            collector.stage_provisional_fill(store, local, components, opacity)
        })
    }

    pub(crate) fn callback_provisional_center(
        &mut self,
        token: CallbackPhaseToken,
        local: SemanticLocalNodeToken,
    ) -> Result<(f64, f64), AuthoringFailure> {
        self.with_provisional_collector(token, |collector, store| {
            collector.callback_provisional_center(store, local)
        })
    }
}
