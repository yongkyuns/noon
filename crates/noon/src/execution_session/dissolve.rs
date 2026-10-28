//! Visibility enrollment belongs to the ordinary atomic activation transaction.
use super::{AnimationAdmissions, ExecutionSession, ExecutionSessionAnimationError};
use noon_compile::{
    PreparedSemanticAnimationScheduleProjection, PreparedSemanticScheduledAnimationPayload,
};
use noon_core::{
    AnimationOptions, DissolveDirection, NoiseDissolve, PreparedSemanticMutationTransaction,
    SemanticNodeId, SemanticStore, SemanticTransactionNodeRef,
};
use std::collections::HashSet;

/// Membership must see the same explicit play overrides as schedule lowering.
/// Only lifecycle flags are previewed here; timing stays with the common plan.
pub(super) fn lifecycle_overrides(
    request: &super::SemanticCompositionRequest,
    play: AnimationOptions,
) -> Option<super::SemanticCompositionRequest> {
    use super::SemanticCompositionRequest as Request;
    if play.introducer.is_none() && play.remover.is_none() {
        return None;
    }
    match request {
        Request::Dissolve {
            target,
            direction,
            style,
            options,
        } => Some(Request::Dissolve {
            target: *target,
            direction: *direction,
            style: *style,
            options: AnimationOptions {
                introducer: play.introducer.or(options.introducer),
                remover: play.remover.or(options.remover),
                ..*options
            },
        }),
        Request::Composition {
            kind,
            children,
            options,
        } => {
            let mut changed = false;
            let children = children
                .iter()
                .map(|child| {
                    if let Some(child) = lifecycle_overrides(child, play) {
                        changed = true;
                        child
                    } else {
                        child.clone()
                    }
                })
                .collect();
            changed.then_some(Request::Composition {
                kind: *kind,
                children,
                options: *options,
            })
        }
        _ => None,
    }
}

impl ExecutionSession {
    pub(crate) fn activate_animation_in_scene(
        &mut self,
        store: &mut SemanticStore,
        scene_root: SemanticNodeId,
        animation: SemanticNodeId,
        play_options: AnimationOptions,
    ) -> Result<crate::ExecutionSegment, ExecutionSessionAnimationError> {
        self.require_animation_declaration_context(store)?;
        let prepared = noon_core::SemanticMutationTransaction::new()
            .prepare(store)
            .map_err(|error| {
                ExecutionSessionAnimationError::InvalidComposition(error.to_string())
            })?;
        let schedule = noon_compile::lower_prepared_semantic_animation_schedule(
            &prepared,
            &self.execution_index,
            animation,
            self.frame().time,
            play_options,
        )?;
        let mut admitted = AnimationAdmissions::default();
        let mut has_dissolve = false;
        for leaf in schedule.leaves() {
            let PreparedSemanticScheduledAnimationPayload::Dissolve { direction, .. } =
                leaf.payload
            else {
                continue;
            };
            has_dissolve = true;
            let owner = leaf
                .target
                .existing()
                .expect("published declaration target");
            self.stage_dissolve_membership(
                prepared.store(),
                scene_root,
                owner,
                direction,
                AnimationOptions::new()
                    .introducer(leaf.options.introducer)
                    .remover(leaf.options.remover),
                &mut admitted,
            )?;
        }
        let mut declaration = prepared.into_transaction();
        if !has_dissolve {
            return self.declare_and_activate_prepared_animation(
                store,
                declaration,
                animation,
                play_options,
                None,
            );
        }
        super::stage_animation_admissions(store, scene_root, &admitted.ordered, &mut declaration)?;
        self.declare_and_activate_prepared_animation(
            store,
            declaration,
            animation,
            play_options,
            Some(super::PreparedAnimationLifecycle::Composition {
                root: scene_root,
                admits: !admitted.is_empty(),
                family_colors: Vec::new(),
                family_styles: Vec::new(),
                removals: Vec::new(),
                callback_intervals: Vec::new(),
            }),
        )
    }

    pub(super) fn stage_dissolve_membership(
        &self,
        store: &SemanticStore,
        root: SemanticNodeId,
        target: SemanticNodeId,
        direction: DissolveDirection,
        options: AnimationOptions,
        admitted: &mut AnimationAdmissions,
    ) -> Result<(), ExecutionSessionAnimationError> {
        let invalid =
            |message: &str| ExecutionSessionAnimationError::InvalidComposition(message.into());
        let node = store
            .node(target)
            .filter(|node| node.can_own_members())
            .ok_or_else(|| invalid("Dissolve requires an object or family"))?;
        if !self.reachability.is_execution_root(root) {
            return Err(invalid("Dissolve requires this session's scene root"));
        }
        if self.lifecycle_target_has_pending_updaters(store, target) {
            return Err(invalid(
                "Dissolve does not support an active host updater on its scope",
            ));
        }
        let present = self.reachability.is_reachable(target);
        if !present {
            if !options
                .introducer
                .unwrap_or(direction == DissolveDirection::In)
                || node.is_scene_owned()
                || !node.parents().is_empty()
            {
                return Err(invalid("Dissolve introduction requires a detached scope"));
            }
            admitted.insert(target.into());
        }
        if options
            .remover
            .unwrap_or(direction == DissolveDirection::Out)
            && present
            && node.parents() != [root]
        {
            return Err(invalid(
                "removing Dissolve requires one direct scene member",
            ));
        }
        Ok(())
    }

    pub(super) fn missing_dissolve_attachments(
        &self,
        prepared: &PreparedSemanticMutationTransaction<'_>,
        schedule: &PreparedSemanticAnimationScheduleProjection,
    ) -> Result<Vec<(SemanticTransactionNodeRef, NoiseDissolve)>, ExecutionSessionAnimationError>
    {
        let mut missing = Vec::new();
        let mut seen = HashSet::new();
        for leaf in schedule.leaves() {
            let PreparedSemanticScheduledAnimationPayload::Dissolve { direction, style } =
                leaf.payload
            else {
                continue;
            };
            let snapshot = prepared
                .animation_effect_snapshot(leaf.target)
                .map_err(|error| {
                    ExecutionSessionAnimationError::InvalidComposition(error.to_string())
                })?;
            if !snapshot.iter().any(|effect| effect.name.is_visibility())
                && seen.insert(leaf.target)
            {
                missing.push((
                    leaf.target,
                    style
                        .resolve(NoiseDissolve::default())
                        .with_progress(match direction {
                            DissolveDirection::In => 1.0,
                            DissolveDirection::Out => 0.0,
                        })
                        .expect("finite dissolve start"),
                ));
            }
        }
        Ok(missing)
    }
}
