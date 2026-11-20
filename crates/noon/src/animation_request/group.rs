//! Candidate-only family endpoints; semantic identity stays in admission.
use super::*;
use crate::{family_affine::FamilyAffine, AuthoringError, ManimRotationPivot, StyleUpdate};
use noon_core::{
    SemanticLocalNodeToken, SemanticMutationTransaction, SemanticNode, SemanticNodeId,
    SemanticNodeKind, SemanticObjectContent, SemanticObjectRole, SemanticObjectState,
    SemanticStore, SemanticStyle, TextSourceKind,
};
use std::{collections::BTreeMap, rc::Rc};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FamilyMethodTarget {
    states: BTreeMap<SemanticNodeId, SemanticObjectState>,
    pub(crate) styles: BTreeMap<SemanticNodeId, SemanticStyle>,
    pub(crate) appearance: MethodFamilyAppearance,
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
                "owned family animate requires planar vector shapes, plain text, or MathTex with supported appearance".into(),
            )
        };
        let mut states = BTreeMap::new();
        let mut styles = BTreeMap::new();
        let mut has_composed_effects = false;
        let mut has_owned_geometry = false;
        let mut has_retained_layout = false;
        let nodes = store
            .ordered_authoring_nodes(family.node_id())
            .map_err(AuthoringError::from)?;
        let graph_rows = graph_component_rows(&store, family.node_id(), &nodes)?;
        for id in nodes {
            let node = store.node(id).expect("validated member");
            if matches!(node.kind(), SemanticNodeKind::Family(_)) {
                let Some(style) = method_group_style(&store, id, composite_opacity, true) else {
                    return Err(unsupported());
                };
                has_retained_layout |= node.graph_declaration().is_some()
                    || store
                        .semantic_table_layout(id)
                        .map_err(AuthoringError::from)?
                        .is_some();
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
            let graph_component = graph_rows.contains(&id);
            let state = live.capture_mobject_state(&object, graph_component)?;
            if !is_method_leaf(&store, &state, graph_component) {
                return Err(unsupported());
            }
            states.insert(id, state);
        }
        if has_owned_geometry && (has_composed_effects || composite_opacity) {
            return Err(unsupported());
        }
        if has_retained_layout
            && (has_owned_geometry
                || has_composed_effects
                || composite_opacity
                || methods
                    .iter()
                    .any(|method| !matches!(method, Method::Shift(_))))
        {
            return Err(LiveSessionError::Mobject(
                "table and graph family animation supports planar shift with unchanged layout"
                    .into(),
            ));
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
            appearance: if has_owned_geometry {
                MethodFamilyAppearance::OwnedGeometry
            } else if has_retained_layout {
                MethodFamilyAppearance::FixedLayout
            } else {
                MethodFamilyAppearance::for_motion(has_composed_effects)
            },
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
                if let Some(state) = self.states.get(&id) {
                    Ok(state.clone())
                } else if self.appearance == MethodFamilyAppearance::FixedLayout {
                    // A Table copy also carries query-only references outside its painted family.
                    Ok(store.semantic_object_state_checked(id)?.clone())
                } else {
                    Err(AuthoringError::MissingCopySource(id))
                }
            },
            |_, id, style| {
                if let Some(prepared) = self.styles.get(&id) {
                    *style = prepared.clone();
                } else if self.appearance != MethodFamilyAppearance::FixedLayout {
                    return Err(AuthoringError::MissingCopySource(id));
                }
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
    allow_layout: bool,
) -> Option<&SemanticStyle> {
    let node = store.node(id)?;
    let style = node.family_style()?;
    ((allow_opacity || style.object_opacity == 1.0 && !store.has_family_appearance_target(id))
        && (allow_layout
            || node.graph_declaration().is_none()
                && store
                    .semantic_table_layout(id)
                    .is_ok_and(|layout| layout.is_none())))
    .then_some(style)
}

fn graph_component_rows(
    store: &SemanticStore,
    root: SemanticNodeId,
    nodes: &[SemanticNodeId],
) -> Result<std::collections::HashSet<SemanticNodeId>, AuthoringError> {
    if has_graph_members(store, nodes) {
        crate::family_copy::graph_dependency_copy_rows(store, &[root], None)
    } else {
        Ok(std::collections::HashSet::new())
    }
}

pub(crate) fn has_graph_members(store: &SemanticStore, nodes: &[SemanticNodeId]) -> bool {
    nodes.iter().any(|&node| {
        store
            .node(node)
            .and_then(SemanticNode::graph_declaration)
            .is_some()
    })
}

fn is_method_leaf(
    store: &SemanticStore,
    state: &SemanticObjectState,
    graph_component: bool,
) -> bool {
    (state.role() == SemanticObjectRole::Ordinary
        || (graph_component
            && matches!(
                state.role(),
                SemanticObjectRole::ArrowShaft(_)
                    | SemanticObjectRole::ArrowStartTip
                    | SemanticObjectRole::ArrowEndTip
            )))
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MethodFamilyAppearance {
    Ordinary,
    ComposedEffects,
    RootOpacity,
    OwnedGeometry,
    FixedLayout,
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
    let Ok(nodes) = store.ordered_authoring_nodes(root) else {
        return false;
    };
    let graph_rows = if appearance == MethodFamilyAppearance::FixedLayout {
        let Ok(rows) = graph_component_rows(store, root, &nodes) else {
            return false;
        };
        rows
    } else {
        std::collections::HashSet::new()
    };
    nodes.into_iter().all(|id| {
        let node = store.node(id).expect("validated authoring node");
        if matches!(node.kind(), SemanticNodeKind::Family(_)) {
            node.semantic_object_state().is_none_or(|state| {
                appearance == MethodFamilyAppearance::OwnedGeometry
                    && matches!(state.content, SemanticObjectContent::Geometry(_))
                    && is_method_leaf(store, state, graph_rows.contains(&id))
            }) && (node.effect_ids().is_empty()
                || matches!(
                    appearance,
                    MethodFamilyAppearance::ComposedEffects | MethodFamilyAppearance::RootOpacity
                ))
                && method_group_style(
                    store,
                    id,
                    appearance == MethodFamilyAppearance::RootOpacity,
                    appearance == MethodFamilyAppearance::FixedLayout,
                )
                .is_some()
        } else {
            node.effect_ids().is_empty()
                && node.member_count() == 0
                && node
                    .semantic_object_state()
                    .is_some_and(|state| is_method_leaf(store, state, graph_rows.contains(&id)))
        }
    })
}

pub(crate) fn fixed_family_method_appearance(
    store: &SemanticStore,
    root: SemanticNodeId,
) -> Result<Option<MethodFamilyAppearance>, AuthoringError> {
    let mut appearance = None;
    for id in store.ordered_authoring_nodes(root)? {
        let node = store.node(id).expect("validated authoring node");
        if matches!(node.kind(), SemanticNodeKind::Family(_)) {
            if node.graph_declaration().is_some()
                || store
                    .semantic_table_layout(id)
                    .map_err(AuthoringError::from)?
                    .is_some()
            {
                return Ok(Some(MethodFamilyAppearance::FixedLayout));
            }
            if node.semantic_object_state().is_some() {
                appearance = Some(MethodFamilyAppearance::OwnedGeometry);
            }
        }
    }
    Ok(appearance)
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

pub(crate) fn supports_layout_shift_target(
    store: &SemanticStore,
    source: SemanticNodeId,
    target: SemanticNodeId,
) -> Result<bool, AuthoringError> {
    if ![source, target]
        .into_iter()
        .all(|id| is_method_family(store, id, MethodFamilyAppearance::FixedLayout))
    {
        return Ok(false);
    }
    for (source, target) in store.ordered_family_root_pairs(source, target)? {
        let layout = |id| {
            store.semantic_table_layout(id).map(|layout| {
                // The original table keeps its own view and entry identities.
                layout.map(|layout| layout.with_structure(None))
            })
        };
        if layout(source).map_err(AuthoringError::from)?
            != layout(target).map_err(AuthoringError::from)?
        {
            return Ok(false);
        }
        let source = store.node(source).expect("validated family pair");
        let target = store.node(target).expect("validated family pair");
        if source.graph_declaration().map(|graph| graph.topology())
            != target.graph_declaration().map(|graph| graph.topology())
            || !matching_method_family_paint(source, target, false)
        {
            return Ok(false);
        }
    }
    Ok(paired_planar_shift(store, source, target)?.is_some())
}

pub(crate) fn matching_method_family_paint(
    source: &SemanticNode,
    target: &SemanticNode,
    allow_opacity_change: bool,
) -> bool {
    let mut style = source
        .family_style()
        .expect("validated family style")
        .clone();
    let target_style = target.family_style().expect("validated family style");
    if allow_opacity_change {
        style.object_opacity = target_style.object_opacity;
    }
    let paint = |node: &SemanticNode| {
        node.presentation()
            .map(|value| (value.draw_enabled, value.z_index))
    };
    style == *target_style
        && paint(source) == paint(target)
        && source.family_fallback_color() == target.family_fallback_color()
        && source.family_background_stroke_color() == target.family_background_stroke_color()
        && source.family_background_stroke_width() == target.family_background_stroke_width()
        && source.family_background_stroke_opacity() == target.family_background_stroke_opacity()
        && source.sheen_direction() == target.sheen_direction()
}

/// Return whether an unchanged family endpoint moves or stays in place.
pub(crate) fn paired_planar_shift(
    store: &SemanticStore,
    source: SemanticNodeId,
    target: SemanticNodeId,
) -> Result<Option<bool>, AuthoringError> {
    let mut shifted = false;
    let mut shared_shift = [(f64::NEG_INFINITY, f64::INFINITY); 2];
    for (source, target) in store.ordered_family_leaf_pairs(source, target)? {
        let mut original = store.semantic_object_state_checked(source)?.clone();
        let copied = store.semantic_object_state_checked(target)?;
        let from = original.transform.translation;
        let to = copied.transform.translation;
        for (range, (start, end)) in shared_shift
            .iter_mut()
            .zip([(from.x, to.x), (from.y, to.y)])
        {
            let shift = end - start;
            // Copied endpoints round the same shift independently at each member's position.
            let tolerance = 4.0 * f64::EPSILON * (start.abs() + end.abs());
            *range = (
                range.0.max(shift - tolerance),
                range.1.min(shift + tolerance),
            );
            if range.0 > range.1 {
                return Ok(None);
            }
            shifted |= start != end;
        }
        original.transform.translation.x = to.x;
        original.transform.translation.y = to.y;
        if !copied.matches_authored_state_of(&original) {
            return Ok(None);
        }
    }
    Ok(Some(shifted))
}
