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
    pub(crate) has_composed_effects: bool,
    pub(crate) has_owned_geometry: bool,
}

impl FamilyMethodTarget {
    pub(super) fn capture(
        live: &LiveSession<'_>,
        family: &MobjectFamily,
        methods: &[Method],
        composite_opacity: bool,
    ) -> Result<Self, LiveSessionError> {
        live.require_family(family)?;
        let store = live.integration_store().borrow();
        let unsupported = || {
            LiveSessionError::Mobject(
                "owned family animate requires groups of planar vector shapes, plain text, or MathTex without per-object effects, composite opacity, or table/graph metadata".into(),
            )
        };
        let mut states = BTreeMap::new();
        let mut styles = BTreeMap::new();
        let mut has_composed_effects = false;
        let mut has_owned_geometry = false;
        for id in store
            .ordered_authoring_nodes(family.node_id())
            .map_err(AuthoringError::from)?
        {
            let node = store.node(id).expect("validated member");
            if matches!(node.kind(), SemanticNodeKind::Family(_)) {
                let Some(style) = method_group_style(&store, id, composite_opacity) else {
                    return Err(unsupported());
                };
                has_composed_effects |= !node.effect_ids().is_empty();
                if let Some(state) = node.semantic_object_state() {
                    if !matches!(state.content, SemanticObjectContent::Geometry(_)) {
                        return Err(unsupported());
                    }
                    has_owned_geometry = true;
                } else {
                    styles.insert(id, style.clone());
                    continue;
                }
            } else if !matches!(node.kind(), SemanticNodeKind::AuthoringObject)
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
        if has_owned_geometry && (has_composed_effects || composite_opacity) {
            return Err(unsupported());
        }
        if (has_composed_effects || has_owned_geometry)
            && methods
                .iter()
                .any(|method| matches!(method, Method::SetFill(_) | Method::SetColor(_)))
        {
            return Err(LiveSessionError::Mobject(
                "groups with composed effects or their own geometry support only shift, uniform scale, and planar rotation"
                    .into(),
            ));
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
        Ok(Self {
            states,
            styles,
            has_composed_effects,
            has_owned_geometry,
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
            |_, id, style| {
                *style = self
                    .styles
                    .get(&id)
                    .cloned()
                    .ok_or(AuthoringError::MissingCopySource(id))?;
                Ok(())
            },
            // Motion drives only the original drawable leaves. Their composed
            // effect still reads those same leaves through its existing scope.
            |_, _, _, _| Ok(()),
        )?;
        Ok(copied[&source])
    }
}

fn method_group_style(
    store: &SemanticStore,
    id: SemanticNodeId,
    allow_opacity: bool,
) -> Option<&SemanticStyle> {
    let node = store.node(id)?;
    let style = node.family_style()?;
    ((allow_opacity || style.object_opacity == 1.0 && !store.has_family_appearance_target(id))
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
            SemanticObjectContent::Text(text) => {
                store.text_resources().get(text).is_some_and(|resource| {
                    matches!(
                        resource.kind,
                        TextSourceKind::Plain | TextSourceKind::MathTex
                    )
                })
            }
            _ => crate::path_queries::authored_content_requires_boundary(store, state.content)
                .is_ok_and(|required| !required),
        }
}

pub(crate) fn is_ordinary_method_family(store: &SemanticStore, root: SemanticNodeId) -> bool {
    is_method_family(store, root, MethodFamilyAppearance::Ordinary)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MethodFamilyAppearance {
    Ordinary,
    ComposedEffects,
    RootOpacity,
    OwnedGeometry,
}

impl MethodFamilyAppearance {
    pub(crate) fn for_motion(has_composed_effects: bool) -> Self {
        if has_composed_effects {
            Self::ComposedEffects
        } else {
            Self::Ordinary
        }
    }
}

pub(crate) fn is_method_family(
    store: &SemanticStore,
    root: SemanticNodeId,
    appearance: MethodFamilyAppearance,
) -> bool {
    store.ordered_authoring_nodes(root).is_ok_and(|nodes| {
        nodes.into_iter().all(|id| {
            let node = store.node(id).expect("validated authoring node");
            if matches!(node.kind(), SemanticNodeKind::Family(_)) {
                node.semantic_object_state().is_none_or(|state| {
                    appearance == MethodFamilyAppearance::OwnedGeometry
                        && matches!(state.content, SemanticObjectContent::Geometry(_))
                        && is_method_leaf(store, state)
                }) && (node.effect_ids().is_empty()
                    || matches!(
                        appearance,
                        MethodFamilyAppearance::ComposedEffects
                            | MethodFamilyAppearance::RootOpacity
                    ))
                    && method_group_style(
                        store,
                        id,
                        appearance == MethodFamilyAppearance::RootOpacity,
                    )
                    .is_some()
            } else {
                node.effect_ids().is_empty()
                    && node.member_count() == 0
                    && node
                        .semantic_object_state()
                        .is_some_and(|state| is_method_leaf(store, state))
            }
        })
    })
}

pub(crate) fn contains_family_geometry(
    store: &SemanticStore,
    root: SemanticNodeId,
) -> Result<bool, AuthoringError> {
    Ok(store.ordered_authoring_nodes(root)?.into_iter().any(|id| {
        store.node(id).is_some_and(|node| {
            matches!(node.kind(), SemanticNodeKind::Family(_))
                && node.semantic_object_state().is_some()
        })
    }))
}

/// The copied host endpoint must keep the same drawing and member correspondence.
/// Fixed-topology lowering checks the actual node and alias pairs before publication.
pub(crate) fn supports_geometry_method_target(
    store: &SemanticStore,
    source: SemanticNodeId,
    target: SemanticNodeId,
) -> Result<bool, AuthoringError> {
    if ![source, target]
        .into_iter()
        .all(|id| is_method_family(store, id, MethodFamilyAppearance::OwnedGeometry))
    {
        return Ok(false);
    }

    let source_leaves = store.ordered_leaf_nodes(source)?;
    let target_leaves = store.ordered_leaf_nodes(target)?;
    if source_leaves.len() != target_leaves.len() {
        return Ok(false);
    }
    for (source, target) in source_leaves.into_iter().zip(target_leaves) {
        let mut previous = store.semantic_object_state_checked(source)?.clone();
        let next = store.semantic_object_state_checked(target)?;
        let old_scale = previous.transform.scale;
        let new_scale = next.transform.scale;
        // Check proportional scale without dividing by a potentially zero axis.
        let x = old_scale.x * new_scale.y;
        let y = old_scale.y * new_scale.x;
        if (x - y).abs() > f32::EPSILON as f64 * x.abs().max(y.abs())
            || (old_scale.x == 0.0 && new_scale.x != 0.0)
            || (old_scale.y == 0.0 && new_scale.y != 0.0)
        {
            return Ok(false);
        }
        previous.transform.translation.x = next.transform.translation.x;
        previous.transform.translation.y = next.transform.translation.y;
        previous.transform.scale.x = new_scale.x;
        previous.transform.scale.y = new_scale.y;
        previous.transform.orientation = next.transform.orientation;
        if !next.matches_authored_state_of(&previous) {
            return Ok(false);
        }
    }
    Ok(true)
}
