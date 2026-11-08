//! Candidate-only flat family endpoints; semantic identity stays in admission.
use super::*;
use crate::{family_affine::FamilyAffine, AuthoringError, ManimRotationPivot, StyleUpdate};
use noon_core::{
    SemanticLocalNodeToken, SemanticMutationTransaction, SemanticNodeId, SemanticNodeKind,
    SemanticObjectRole, SemanticObjectState, SemanticStore, SemanticStyle,
};
use std::{collections::BTreeMap, rc::Rc};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FamilyMethodTarget {
    states: BTreeMap<SemanticNodeId, SemanticObjectState>,
    pub(crate) style: SemanticStyle,
    pub(crate) color: Color,
}

impl FamilyMethodTarget {
    pub(super) fn capture(
        live: &LiveSession<'_>,
        family: &MobjectFamily,
        methods: &[Method],
    ) -> Result<Self, LiveSessionError> {
        live.require_family(family)?;
        let store = live.integration_store().borrow();
        let root = store.node(family.node_id()).expect("validated family");
        let unsupported = || {
            LiveSessionError::Mobject(
            "owned family animate requires a flat vector family without content-owning roots, effects, composite opacity, or table/graph metadata".into(),
        )
        };
        if root.semantic_object_state().is_some()
            || !root.effect_ids().is_empty()
            || root
                .family_style()
                .is_some_and(|style| style.object_opacity != 1.0)
            || store.has_family_appearance_target(family.node_id())
            || root.graph_declaration().is_some()
            || store
                .semantic_table_layout(family.node_id())
                .map_err(AuthoringError::from)?
                .is_some()
        {
            return Err(unsupported());
        }
        let mut states = BTreeMap::new();
        for id in root.members_iter() {
            let node = store.node(id).expect("validated member");
            if !matches!(node.kind(), SemanticNodeKind::AuthoringObject)
                || node.members_iter().next().is_some()
                || !node.effect_ids().is_empty()
            {
                return Err(unsupported());
            }
            let object = Mobject::from_node(Rc::clone(live.integration_store()), id)?;
            if live.requires_content_boundary((&object).into())? {
                return Err(unsupported());
            }
            let state = live.capture_mobject_state(&object)?;
            if state.role() != SemanticObjectRole::Ordinary
                || state.spatial_properties().is_some()
                || state.transform.as_planar().is_none()
            {
                return Err(unsupported());
            }
            states.insert(id, state);
        }
        let mut style = root.family_style().expect("family style").clone();
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
                edit_color(
                    &mut style,
                    color.red.into(),
                    color.green.into(),
                    color.blue.into(),
                    color.alpha.into(),
                )?;
            }
            let fill = if let Method::SetFill(color) = method {
                let update = StyleUpdate {
                    fill_color: Some(*color),
                    fill_opacity: Some(f64::from(color.alpha)),
                    ..StyleUpdate::default()
                };
                update.apply(&mut style)?;
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
        let color = style
            .family_color()
            .unwrap_or_else(|| root.family_fallback_color().expect("family fallback"));
        Ok(Self {
            states,
            style,
            color,
        })
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
            |_, _, style| {
                *style = self.style.clone();
                Ok(())
            },
            |_, _, _, _| Ok(()), // Admission rejects all attachment-bearing targets.
        )?;
        Ok(copied[&source])
    }
}
