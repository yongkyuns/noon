//! Shared object/family become preparation and atomic semantic state replacement.
use std::{collections::HashMap, rc::Rc};

use crate::{
    semantic_mobject::{
        layout_for_content, scale_state_about_center, stage_state_changes, state_center,
        validate_content,
    },
    AuthoringError, Mobject, MobjectFamily,
};
use noon_core::{Bounds2D64, SemanticMutationTransaction, SemanticObjectState, SemanticStore};

/// Pair through the same topology/alias contract as ordinary family Transform.
/// All captures and fitting succeed before a caller can publish any edit.
pub(crate) fn prepare_family_become<E: From<AuthoringError>>(
    source: &MobjectFamily,
    target: &MobjectFamily,
    options: ManimBecomeOptions,
    mut capture: impl FnMut(&Mobject) -> Result<SemanticObjectState, E>,
) -> Result<crate::path_editing::PreparedPathEdits, E> {
    if !Rc::ptr_eq(source.integration_store(), target.integration_store()) {
        return Err(AuthoringError::ForeignStore.into());
    }
    source.validate()?;
    target.validate()?;
    let pairs = match source
        .integration_store()
        .borrow()
        .ordered_family_leaf_pairs(source.node_id(), target.node_id())
    {
        Ok(pairs) => pairs,
        Err(noon_core::SemanticFamilyPairingError::Empty) => {
            return crate::path_editing::PreparedPathEdits::prepare(
                &source.integration_store().borrow(),
                Vec::new(),
            )
            .map_err(E::from)
        }
        Err(error) => return Err(AuthoringError::from(error).into()),
    };
    let mut sources = Vec::with_capacity(pairs.len());
    let mut targets = Vec::with_capacity(pairs.len());
    for &(source_id, target_id) in &pairs {
        sources.push(capture(&Mobject::from_node(
            Rc::clone(source.integration_store()),
            source_id,
        )?)?);
        targets.push(capture(&Mobject::from_node(
            Rc::clone(source.integration_store()),
            target_id,
        )?)?);
    }
    let store = source.integration_store().borrow();
    let targets = prepare_become_states(&store, &sources, targets, options)?;
    let mut transaction = SemanticMutationTransaction::new();
    let mut replacements = Vec::new();
    let mut staged = HashMap::<noon_core::SemanticNodeId, SemanticObjectState>::new();
    for ((source, target_id), (mut target, path)) in pairs.into_iter().zip(targets) {
        // Plain Manim become reads a shared target after preceding leaf writes.
        // Matching options copy the target first, so those reads stay captured.
        if options == ManimBecomeOptions::default() {
            if let Some(previous_write) = staged.get(&target_id) {
                target = previous_write.clone();
            }
            staged.insert(source, target.clone());
        }
        let previous = store
            .semantic_object_state_checked(source)
            .map_err(AuthoringError::from)?;
        if let Some(path) = path {
            replacements.push((source, target, path));
        } else {
            stage_state_changes(&mut transaction, source, previous, &target);
        }
    }
    Ok(
        crate::path_editing::PreparedPathEdits::prepare(&store, replacements)?
            .with_transaction(transaction),
    )
}

impl MobjectFamily {
    /// Replace family presentation while preserving member and alias identities.
    /// Uses authored state; live execution uses `LiveSession::become_family`.
    /// Different topology is rejected by the shared family pairing contract.
    pub fn become_family(
        &self,
        target: &Self,
        options: ManimBecomeOptions,
    ) -> Result<(), AuthoringError> {
        prepare_family_become(self, target, options, Mobject::state)?.publish(
            &mut self.integration_store().borrow_mut(),
            |store, transaction| {
                transaction
                    .apply(store)
                    .map(|_| ())
                    .map_err(AuthoringError::from)
            },
        )
    }
}

/// Dimension matching applies height then width; stretch overrides both.
/// Center matching runs last, after the target dimensions have been resolved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ManimBecomeOptions {
    pub match_height: bool,
    pub match_width: bool,
    pub match_center: bool,
    pub stretch: bool,
}

pub(crate) fn prepare_become(
    store: &SemanticStore,
    node: noon_core::SemanticNodeId,
    source: &SemanticObjectState,
    target: SemanticObjectState,
    options: ManimBecomeOptions,
) -> Result<crate::path_editing::PreparedPathEdits, AuthoringError> {
    let (target, path) =
        prepare_become_states(store, std::slice::from_ref(source), vec![target], options)?
            .pop()
            .expect("one target state");
    let mut transaction = SemanticMutationTransaction::new();
    let replacements = if let Some(path) = path {
        vec![(node, target, path)]
    } else {
        stage_state_changes(
            &mut transaction,
            node,
            store.semantic_object_state_checked(node)?,
            &target,
        );
        Vec::new()
    };
    Ok(
        crate::path_editing::PreparedPathEdits::prepare(store, replacements)?
            .with_transaction(transaction),
    )
}

/// Resolve matching once over aggregate family bounds, then transform each
/// captured target leaf exactly once. Captures are transient preparation data.
pub(crate) fn prepare_become_states(
    store: &SemanticStore,
    source: &[SemanticObjectState],
    targets: Vec<SemanticObjectState>,
    options: ManimBecomeOptions,
) -> Result<Vec<(SemanticObjectState, Option<noon_core::VectorPath>)>, AuthoringError> {
    fn bounds(
        store: &SemanticStore,
        states: &[SemanticObjectState],
    ) -> Result<Option<Bounds2D64>, AuthoringError> {
        let mut total: Option<Bounds2D64> = None;
        for state in states {
            validate_content(store, state.content)?;
            if let Some(bounds) = layout_for_content(store, state.content, state.transform)? {
                if let Some(total) = &mut total {
                    total.include(bounds.min_x, bounds.min_y);
                    total.include(bounds.max_x, bounds.max_y);
                } else {
                    total = Some(bounds);
                }
            }
        }
        Ok(total)
    }
    fn center(
        store: &SemanticStore,
        states: &[SemanticObjectState],
    ) -> Result<(f64, f64), AuthoringError> {
        let mut boundary = None;
        for state in states {
            crate::family_layout::union_bounds(
                &mut boundary,
                crate::semantic_mobject::boundary_for_content(
                    store,
                    state.content,
                    state.transform,
                )?,
            );
        }
        if let Some(bounds) = boundary {
            return Ok((
                (bounds.min_x + bounds.max_x) * 0.5,
                (bounds.min_y + bounds.max_y) * 0.5,
            ));
        }
        if let [state] = states {
            return state_center(store, state);
        }
        Ok((0.0, 0.0))
    }
    let source_bounds = bounds(store, source)?;
    let target_bounds = bounds(store, &targets)?;
    let source_width = source_bounds.map_or(0.0, |b| b.width());
    let source_height = source_bounds.map_or(0.0, |b| b.height());
    let target_width = target_bounds.map_or(0.0, |b| b.width());
    let target_height = target_bounds.map_or(0.0, |b| b.height());
    let (mut x, mut y) = (1.0, 1.0);
    if options.stretch {
        if target_width == 0.0 || target_height == 0.0 {
            return Err(AuthoringError::ZeroStretchTarget);
        }
        x = source_width / target_width;
        y = source_height / target_height;
    } else {
        if options.match_height {
            if target_height == 0.0 {
                return Err(AuthoringError::ZeroMatchHeight);
            }
            x = source_height / target_height;
            y = x;
        }
        if options.match_width {
            let scaled_width = target_width * x;
            if scaled_width == 0.0 {
                return Err(AuthoringError::ZeroMatchWidth);
            }
            let factor = source_width / scaled_width;
            x *= factor;
            y *= factor;
        }
    }
    let target_center = center(store, &targets)?;
    let destination = if options.match_center {
        center(store, source)?
    } else {
        target_center
    };
    let mut result = Vec::with_capacity(targets.len());
    for mut target in targets {
        let Ok((local_x, local_y)) =
            crate::dimension_fit::world_scale_factors(target.transform.rotation_z, x, y)
        else {
            let path = crate::family_affine::world_scaled_path(
                store,
                &target,
                x,
                y,
                target_center,
                destination,
            )?;
            result.push((target, Some(path)));
            continue;
        };
        let old_center = state_center(store, &target)?;
        let next_center = (
            destination.0 + (old_center.0 - target_center.0) * x,
            destination.1 + (old_center.1 - target_center.1) * y,
        );
        scale_state_about_center(store, &mut target, local_x, local_y, next_center)?;
        result.push((target, None));
    }
    Ok(result)
}
