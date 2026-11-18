//! Candidate-only family endpoints; semantic identity stays in admission.
use super::*;
use crate::{family_affine::FamilyAffine, AuthoringError, ManimRotationPivot, StyleUpdate};
use noon_core::{
    SemanticLocalNodeToken, SemanticMutationTransaction, SemanticNodeId, SemanticNodeKind,
    SemanticObjectContent, SemanticObjectRole, SemanticObjectState, SemanticStore, SemanticStyle,
    TextSourceKind,
};
use std::{collections::BTreeMap, rc::Rc};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FamilyMethodTarget {
    states: BTreeMap<SemanticNodeId, SemanticObjectState>,
    pub(crate) styles: BTreeMap<SemanticNodeId, SemanticStyle>,
}

impl FamilyMethodTarget {
    pub(super) fn capture(
        live: &LiveSession<'_>,
        family: &MobjectFamily,
        methods: &[Method],
    ) -> Result<Self, LiveSessionError> {
        live.require_family(family)?;
        let store = live.integration_store().borrow();
        let unsupported = || {
            LiveSessionError::Mobject(
                "owned family animate requires groups of planar vector shapes or plain text without content-owning roots, effects, composite opacity, or table/graph metadata".into(),
            )
        };
        let mut states = BTreeMap::new();
        let mut styles = BTreeMap::new();
        for id in store
            .ordered_authoring_nodes(family.node_id())
            .map_err(AuthoringError::from)?
        {
            let node = store.node(id).expect("validated member");
            if matches!(node.kind(), SemanticNodeKind::Family(_)) {
                let Some(style) = ordinary_group_style(&store, id) else {
                    return Err(unsupported());
                };
                styles.insert(id, style.clone());
                continue;
            }
            if !matches!(node.kind(), SemanticNodeKind::AuthoringObject)
                || node.member_count() != 0
                || !node.effect_ids().is_empty()
            {
                return Err(unsupported());
            }
            let object = Mobject::from_node(Rc::clone(live.integration_store()), id)?;
            let state = live.capture_mobject_state(&object)?;
            if !is_method_leaf(&store, &state) {
                return Err(unsupported());
            }
            states.insert(id, state);
        }
        for method in methods {
            let mut bounds = None;
            if matches!(method, Method::Scale(_) | Method::Rotate(_)) {
                for state in states.values() {
                    crate::family_layout::union_bounds(
                        &mut bounds,
                        crate::semantic_mobject::boundary_for_content(
                            &store,
                            state.content,
                            state
                                .transform
                                .as_planar()
                                .expect("validated planar target"),
                        )?,
                    );
                }
            }
            let center = crate::family_layout::bounds_critical_point(bounds, 0.0, 0.0);
            if let Method::SetColor(color) = method {
                for style in styles.values_mut() {
                    edit_color(
                        style,
                        color.red.into(),
                        color.green.into(),
                        color.blue.into(),
                        color.alpha.into(),
                    )?;
                }
            }
            let fill = if let Method::SetFill(color) = method {
                let update = StyleUpdate {
                    fill_color: Some(*color),
                    fill_opacity: Some(f64::from(color.alpha)),
                    ..StyleUpdate::default()
                };
                for style in styles.values_mut() {
                    update.apply(style)?;
                }
                Some(update)
            } else {
                None
            };
            // Validate even an empty family; a no-op must not hide invalid input.
            match method {
                Method::Shift(delta) => {
                    authoring_render_f64("shift.x", f64::from(delta.x))?;
                    authoring_render_f64("shift.y", f64::from(delta.y))?;
                }
                Method::Scale(value) | Method::Rotate(value) => {
                    authoring_render_f64("family affine", *value)?;
                }
                Method::SetFill(_) | Method::SetColor(_) => {}
            }
            for state in states.values_mut() {
                match method {
                    Method::Shift(delta) => {
                        state.transform.translation.x = authoring_render_f64(
                            "translation.x",
                            state.transform.translation.x + f64::from(delta.x),
                        )?;
                        state.transform.translation.y = authoring_render_f64(
                            "translation.y",
                            state.transform.translation.y + f64::from(delta.y),
                        )?;
                    }
                    Method::SetColor(color) => edit_color(
                        &mut state.style,
                        color.red.into(),
                        color.green.into(),
                        color.blue.into(),
                        color.alpha.into(),
                    )?,
                    Method::SetFill(_) => fill.expect("fill edit").apply(&mut state.style)?,
                    Method::Scale(factor) => {
                        FamilyAffine::Scale(*factor, *factor, ManimRotationPivot::Center)
                            .apply_to_state(&store, state, center, 0.0)?
                    }
                    Method::Rotate(angle) => {
                        FamilyAffine::Rotate(*angle, ManimRotationPivot::Center)
                            .apply_to_state(&store, state, center, 0.0)?
                    }
                }
            }
        }
        Ok(Self { states, styles })
    }

    pub(crate) fn stage(
        &self,
        store: &SemanticStore,
        source: SemanticNodeId,
        transaction: &mut SemanticMutationTransaction,
    ) -> Result<SemanticLocalNodeToken, AuthoringError> {
        let copied = crate::family_copy::stage_semantic_copy(
            store,
            source,
            &[],
            crate::family_copy::CopyRootMembership::Preserve,
            transaction,
            |id, _| {
                self.states
                    .get(&id)
                    .cloned()
                    .ok_or(AuthoringError::MissingCopySource(id))
            },
            |_, id, style| {
                *style = self
                    .styles
                    .get(&id)
                    .cloned()
                    .ok_or(AuthoringError::MissingCopySource(id))?;
                Ok(())
            },
            |_, _, _, _| Ok(()), // Admission rejects all attachment-bearing targets.
        )?;
        Ok(copied[&source])
    }
}

fn ordinary_group_style(store: &SemanticStore, id: SemanticNodeId) -> Option<&SemanticStyle> {
    let node = store.node(id)?;
    let style = node.family_style()?;
    (node.semantic_object_state().is_none()
        && node.effect_ids().is_empty()
        && style.object_opacity == 1.0
        && !store.has_family_appearance_target(id)
        && node.graph_declaration().is_none()
        && store
            .semantic_table_layout(id)
            .is_ok_and(|layout| layout.is_none()))
    .then_some(style)
}

fn is_method_leaf(store: &SemanticStore, state: &SemanticObjectState) -> bool {
    state.role() == SemanticObjectRole::Ordinary
        && state.spatial_properties().is_none()
        && state.transform.as_planar().is_some()
        && match state.content {
            SemanticObjectContent::Text(text) => store
                .text_resources()
                .get(text)
                .is_some_and(|resource| resource.kind == TextSourceKind::Plain),
            _ => crate::path_queries::authored_content_requires_boundary(store, state.content)
                .is_ok_and(|required| !required),
        }
}

pub(crate) fn is_ordinary_method_family(store: &SemanticStore, root: SemanticNodeId) -> bool {
    store.ordered_authoring_nodes(root).is_ok_and(|nodes| {
        nodes.into_iter().all(|id| {
            let node = store.node(id).expect("validated authoring node");
            if matches!(node.kind(), SemanticNodeKind::Family(_)) {
                ordinary_group_style(store, id).is_some()
            } else {
                node.member_count() == 0
                    && node.effect_ids().is_empty()
                    && node
                        .semantic_object_state()
                        .is_some_and(|state| is_method_leaf(store, state))
            }
        })
    })
}
