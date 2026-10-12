//! Validate inset declarations against the final, transaction-local object overlay.
use super::*;

pub(super) fn validate(
    transaction: &SemanticMutationTransaction,
    preflight: &SemanticTransactionPreflight,
    store: &SemanticStore,
) -> Result<(), SemanticMutationTransactionError> {
    let removed = |node: SemanticTransactionNodeRef| match node {
        SemanticTransactionNodeRef::Existing(node) => preflight.removed_existing.contains(&node),
        SemanticTransactionNodeRef::Pending(token) => preflight.removed_pending.contains(&token),
    };
    let state = |node: SemanticTransactionNodeRef| {
        preflight.staged_objects.get(&node).or_else(|| {
            node.existing()
                .and_then(|node| store.semantic_object_state_checked(node).ok())
        })
    };
    let mut declarations = HashMap::new();
    let mut affected = HashMap::new();
    for (index, mutation) in transaction.mutations.iter().enumerate() {
        if let SemanticMutation::SetInset2DView {
            object,
            camera_frame,
            ..
        } = mutation
        {
            declarations.insert(*object, *camera_frame);
            affected.insert(*object, index);
        }
        if let SemanticMutation::AddNode {
            token,
            creation: SemanticNodeCreation::Object { state, .. },
        } = mutation
        {
            if matches!(state.role(), SemanticObjectRole::Inset2DView(_)) {
                affected.insert((*token).into(), index);
            }
        }
        if let SemanticMutation::ReplaceContent { object, .. } = mutation {
            affected.insert(*object, index);
            if let Some(camera) = object.existing() {
                for display in store.inset_displays_for_camera(camera) {
                    affected.insert(display.into(), index);
                }
            }
        }
    }
    for (display, index) in affected {
        if removed(display) {
            continue;
        }
        let camera = declarations.get(&display).copied().unwrap_or_else(|| {
            state(display).and_then(|state| match state.role() {
                SemanticObjectRole::Inset2DView(view) => Some(view.camera_frame.into()),
                _ => None,
            })
        });
        let Some(camera) = camera else {
            continue;
        };
        // Retiring an existing camera clears its indexed owners at commit.
        if removed(camera) && !declarations.contains_key(&display) {
            continue;
        }
        let rectangle = |node| {
            state(node).is_some_and(|state| {
                matches!(
                    state.content.geometry(),
                    Some(StoredGeometry::Rectangle { .. })
                )
            })
        };
        if display == camera || removed(camera) || !rectangle(display) || !rectangle(camera) {
            return Err(SemanticMutationTransactionError::InvalidNodeObjectState { index });
        }
    }
    Ok(())
}
