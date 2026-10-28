//! Atomic copies of an addressed authoritative semantic membership graph.
use crate::AuthoringError;
use crate::{ManimArrow, ManimArrowVectorField, Mobject, MobjectFamily, MobjectTarget};
use noon_core::{
    SemanticGraphEdgeDependency, SemanticLocalNodeToken, SemanticMutationTransaction,
    SemanticMutationTransactionResult, SemanticNodeCreation, SemanticNodeId, SemanticNodeKind,
    SemanticObjectState, SemanticStore, SemanticTransactionGraphDeclaration,
    SemanticTransactionGraphEdgeBinding,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashSet},
    rc::Rc,
};

/// A detached copied root and a derived mapping for host wrapper reconstruction.
/// The semantic store owns all copied nodes; dropping this lookup does not change them.
#[derive(Clone, Debug)]
pub struct SemanticCopy<R> {
    root: R,
    store: Rc<RefCell<SemanticStore>>,
    copied: BTreeMap<SemanticNodeId, SemanticNodeId>,
}

/// A copied family root with its shared identity lookup.
pub type FamilyCopy = SemanticCopy<MobjectFamily>;
/// A copied object root, including its own content and ordered descendants.
pub type MobjectCopy = SemanticCopy<Mobject>;

impl<R> SemanticCopy<R> {
    /// Change only the typed root projection; retain the canonical copy identity map.
    fn map_root<T>(self, map: impl FnOnce(R) -> T) -> SemanticCopy<T> {
        SemanticCopy {
            root: map(self.root),
            store: self.store,
            copied: self.copied,
        }
    }

    pub fn root(&self) -> &R {
        &self.root
    }

    pub fn mobject(&self, source: &Mobject) -> Result<Mobject, AuthoringError> {
        self.require_store(source.integration_store())?;
        source.validate()?;
        Mobject::from_node(Rc::clone(&self.store), self.copied_id(source.node_id())?)
    }

    pub fn family(&self, source: &MobjectFamily) -> Result<MobjectFamily, AuthoringError> {
        self.require_store(source.integration_store())?;
        source.validate()?;
        MobjectFamily::from_node(Rc::clone(&self.store), self.copied_id(source.node_id())?)
    }

    /// Rebind an Arrow aggregate to the semantic components created by this
    /// copy. This never recreates geometry: every component must already be in
    /// the authoritative copied-family map.
    pub fn rebind_manim_arrow(&self, source: &ManimArrow) -> Result<ManimArrow, AuthoringError> {
        Ok(ManimArrow::from_copied_components(
            self.family(source.family())?,
            self.mobject(source.shaft())?,
            self.mobject(source.end_tip())?,
            source
                .start_tip()
                .map(|tip| self.mobject(tip))
                .transpose()?,
        ))
    }

    /// Rebind a static ArrowVectorField aggregate using only its copied outer
    /// family and copied Arrow component identities.
    pub fn rebind_manim_arrow_vector_field(
        &self,
        source: &ManimArrowVectorField,
    ) -> Result<ManimArrowVectorField, AuthoringError> {
        let vectors = source
            .vectors()
            .iter()
            .map(|arrow| self.rebind_manim_arrow(arrow))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ManimArrowVectorField::from_copied_components(
            self.family(source.family())?,
            vectors,
        ))
    }

    /// Rebind one ArrowVectorField member as an independent Arrow aggregate.
    /// The selected vector family must be present in this copy; callers use
    /// this when copying one field member without its outer field family.
    pub fn rebind_manim_arrow_vector(
        &self,
        source: &ManimArrowVectorField,
        index: usize,
    ) -> Result<ManimArrow, AuthoringError> {
        let arrow = source
            .vectors()
            .get(index)
            .ok_or(AuthoringError::InvalidSubmobjectIndex {
                family: source.family().node_id(),
                index: index as isize,
            })?;
        self.rebind_manim_arrow(arrow)
    }

    fn require_store(&self, store: &Rc<RefCell<SemanticStore>>) -> Result<(), AuthoringError> {
        if Rc::ptr_eq(&self.store, store) {
            Ok(())
        } else {
            Err(AuthoringError::ForeignStore)
        }
    }

    fn copied_id(&self, source: SemanticNodeId) -> Result<SemanticNodeId, AuthoringError> {
        self.copied
            .get(&source)
            .copied()
            .ok_or(AuthoringError::MissingCopySource(source))
    }
}

pub(crate) struct PendingSemanticCopy {
    store: Rc<RefCell<SemanticStore>>,
    source_root: SemanticNodeId,
    copied: BTreeMap<SemanticNodeId, SemanticLocalNodeToken>,
}

impl PendingSemanticCopy {
    pub(crate) fn root_token(&self) -> SemanticLocalNodeToken {
        self.copied[&self.source_root]
    }

    pub(crate) fn resolve<R>(
        self,
        result: &SemanticMutationTransactionResult,
        from_node: fn(Rc<RefCell<SemanticStore>>, SemanticNodeId) -> Result<R, AuthoringError>,
    ) -> Result<SemanticCopy<R>, AuthoringError> {
        let copied: BTreeMap<_, _> = self
            .copied
            .into_iter()
            .map(|(source, token)| {
                (
                    source,
                    result
                        .resolve(token)
                        .expect("committed copied node resolves"),
                )
            })
            .collect();
        let root = from_node(Rc::clone(&self.store), copied[&self.source_root])?;
        Ok(SemanticCopy {
            root,
            store: self.store,
            copied,
        })
    }
}

impl SemanticCopy<Mobject> {
    /// Project this copied object's root into the common typed entry form.
    pub fn into_entry_copy(self) -> SemanticCopy<crate::CompositeEntryHandle> {
        self.map_root(crate::CompositeEntryHandle::Mobject)
    }
}
impl SemanticCopy<MobjectFamily> {
    /// Project this copied family's root into the common typed entry form.
    pub fn into_entry_copy(self) -> SemanticCopy<crate::CompositeEntryHandle> {
        self.map_root(crate::CompositeEntryHandle::Family)
    }
}

/// Prepare every copied node and edge before the caller publishes anything.
/// Shared aliases allocate once; traversal and temporary work stay root-local.
pub(crate) fn prepare_semantic_copy<E: From<AuthoringError>>(
    source: MobjectTarget<'_>,
    references: &[MobjectTarget<'_>],
    capture: impl FnMut(&Mobject, bool) -> Result<SemanticObjectState, E>,
    capture_family_style: impl FnMut(
        &SemanticStore,
        SemanticNodeId,
        &mut noon_core::SemanticStyle,
    ) -> Result<(), E>,
    copy_effects: impl FnMut(
        &SemanticStore,
        SemanticNodeId,
        SemanticLocalNodeToken,
        &mut SemanticMutationTransaction,
    ) -> Result<(), E>,
) -> Result<(SemanticMutationTransaction, PendingSemanticCopy), E> {
    prepare_semantic_copy_with_root_membership(
        source,
        references,
        CopyRootMembership::Preserve,
        capture,
        capture_family_style,
        copy_effects,
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CopyRootMembership {
    Preserve,
    Omit,
}

/// Copy the same graph while optionally omitting only its root's member edges.
/// Explicit metadata references still share one recursive identity mapping.
pub(crate) fn prepare_semantic_copy_with_root_membership<E: From<AuthoringError>>(
    source: MobjectTarget<'_>,
    references: &[MobjectTarget<'_>],
    root_membership: CopyRootMembership,
    mut capture: impl FnMut(&Mobject, bool) -> Result<SemanticObjectState, E>,
    mut capture_family_style: impl FnMut(
        &SemanticStore,
        SemanticNodeId,
        &mut noon_core::SemanticStyle,
    ) -> Result<(), E>,
    mut copy_effects: impl FnMut(
        &SemanticStore,
        SemanticNodeId,
        SemanticLocalNodeToken,
        &mut SemanticMutationTransaction,
    ) -> Result<(), E>,
) -> Result<(SemanticMutationTransaction, PendingSemanticCopy), E> {
    source.validate()?;
    let store = source.integration_store();
    let mut transaction = SemanticMutationTransaction::new();
    let mut copied = BTreeMap::new();
    let mut edges = Vec::new();
    let mut graph_declarations = Vec::new();
    let mut table_layouts = Vec::new();
    let mut inset_views = Vec::new();
    let mut queue = Vec::with_capacity(references.len() + 1);
    for target in references {
        queue.push(target.require_store(store)?);
    }
    queue.push(source.node_id());
    let omitted_root = (root_membership == CopyRootMembership::Omit).then_some(source.node_id());
    let graph_dependency_rows = graph_dependency_copy_rows(&store.borrow(), &queue, omitted_root)?;
    while let Some(id) = queue.pop() {
        if copied.contains_key(&id) {
            continue;
        }
        let (
            is_family,
            mut members,
            family_z,
            draw_enabled,
            family_color,
            mut family_style,
            family_background_stroke_color,
            family_background_stroke_width,
            family_background_stroke_opacity,
            family_sheen_direction,
            graph_declaration,
            updating_suspended,
        ) = {
            let store = store.borrow();
            let node = store.node(id).ok_or_else(|| {
                AuthoringError::from(noon_core::SemanticSceneOperationError::UnknownNode(id))
            })?;
            match node.kind() {
                SemanticNodeKind::Family(_) => (
                    true,
                    node.members_iter().collect::<Vec<_>>(),
                    Some(node.presentation().expect("family presentation").z_index),
                    node.presentation()
                        .expect("family presentation")
                        .draw_enabled,
                    // Visible paint may differ from the stored fallback. Copy the
                    // authored fallback itself; the complete style is copied below.
                    node.family_fallback_color(),
                    node.family_style().cloned(),
                    node.family_background_stroke_color(),
                    node.family_background_stroke_width(),
                    node.family_background_stroke_opacity(),
                    node.sheen_direction(),
                    node.graph_declaration().cloned(),
                    node.updating_suspended(),
                ),
                SemanticNodeKind::AuthoringObject => (
                    false,
                    node.members_iter().collect::<Vec<_>>(),
                    None,
                    node.presentation()
                        .expect("object presentation")
                        .draw_enabled,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    node.updating_suspended(),
                ),
                _ => {
                    return Err(AuthoringError::from(
                        noon_core::SemanticSceneOperationError::NotSemanticAuthoringNode(id),
                    )
                    .into());
                }
            }
        };
        if omitted_root == Some(id) {
            members.clear();
        }
        if let Some(graph) = graph_declaration {
            graph_declarations.push((id, graph));
        }
        if is_family {
            if let Some(layout) = store
                .borrow()
                .semantic_table_layout(id)
                .map_err(AuthoringError::from)?
            {
                queue.extend(layout.referenced_nodes());
                table_layouts.push((id, layout));
            }
        }
        queue.extend(members.iter().rev().copied());
        edges.push((id, members));
        let has_content = !is_family
            || store
                .borrow()
                .node(id)
                .is_some_and(|node| node.semantic_object_state().is_some());
        if !has_content {
            capture_family_style(
                &store.borrow(),
                id,
                family_style.as_mut().expect("family style"),
            )?;
        }
        let state = if has_content {
            let mobject = Mobject::from_node(Rc::clone(store), id)?;
            let mut state = capture(&mobject, graph_dependency_rows.contains(&id))?;
            if let noon_core::SemanticObjectRole::Inset2DView(view) = state.role() {
                inset_views.push((id, view));
                state.set_role(noon_core::SemanticObjectRole::Ordinary);
            }
            Some(state)
        } else {
            None
        };
        let creation = match (is_family, state) {
            (true, Some(state)) => SemanticNodeCreation::family_with_content_and_fallback(
                state,
                family_color.expect("family fallback was captured"),
            ),
            (true, None) => SemanticNodeCreation::family(),
            (false, Some(state)) => SemanticNodeCreation::object(state),
            (false, None) => unreachable!("validated object owns a payload"),
        };
        let pending = transaction.create_node(creation);
        transaction.set_draw_enabled(pending, draw_enabled);
        if updating_suspended {
            transaction.set_updating_suspended(pending, true);
        }
        copy_effects(&store.borrow(), id, pending, &mut transaction)?;
        if let Some(z) = family_z {
            transaction.set_z_index(pending, z);
        }
        if let Some(color) = family_color.filter(|_| !has_content) {
            transaction.set_family_color(pending, color);
        }
        if let (
            Some(style),
            Some(background_stroke_color),
            Some(background_stroke_width),
            Some(background_stroke_opacity),
        ) = (
            family_style.filter(|_| !has_content),
            family_background_stroke_color,
            family_background_stroke_width,
            family_background_stroke_opacity,
        ) {
            transaction.set_copied_family_style_with_background_stroke_update(
                pending,
                id,
                style,
                Some(background_stroke_color),
                Some(background_stroke_width),
                Some(background_stroke_opacity),
            );
        }
        if let Some(direction) = family_sheen_direction {
            transaction.set_sheen_direction(pending, direction);
        }
        copied.insert(id, pending);
    }
    for (parent, members) in edges {
        for member in members {
            transaction.add_member(copied[&parent], copied[&member]);
        }
    }
    for (display, view) in inset_views {
        let camera = copied
            .get(&view.camera_frame)
            .copied()
            .map(noon_core::SemanticTransactionNodeRef::from)
            .unwrap_or_else(|| view.camera_frame.into());
        transaction.set_inset_2d_view(copied[&display], camera, view.capture_own_display);
    }
    for (source_family, graph) in graph_declarations {
        let vertices = graph.vertices().map(|(id, source)| (id, copied[&source]));
        let edges = graph
            .edges()
            .map(|(edge, binding)| match binding.dependency() {
                SemanticGraphEdgeDependency::Line => SemanticTransactionGraphEdgeBinding::new(
                    edge.id,
                    copied[&binding.family()].into(),
                    copied[&binding.line()].into(),
                ),
                SemanticGraphEdgeDependency::Arrow {
                    end_tip,
                    start_tip,
                    policy,
                } => SemanticTransactionGraphEdgeBinding::new_arrow(
                    edge.id,
                    copied[&binding.family()].into(),
                    copied[&binding.line()].into(),
                    copied[&end_tip].into(),
                    start_tip.map(|tip| copied[&tip].into()),
                    policy,
                ),
            });
        transaction.set_graph_declaration(
            copied[&source_family],
            SemanticTransactionGraphDeclaration::new(graph.topology().clone(), vertices, edges),
        );
    }
    for (source_family, layout) in table_layouts {
        transaction.set_table_layout(
            copied[&source_family],
            noon_core::SemanticTransactionTableLayout::map_existing(layout, |node| {
                copied[&node].into()
            }),
        );
    }
    Ok((
        transaction,
        PendingSemanticCopy {
            store: Rc::clone(store),
            source_root: source.node_id(),
            copied,
        },
    ))
}

fn graph_dependency_copy_rows<E: From<AuthoringError>>(
    store: &SemanticStore,
    roots: &[SemanticNodeId],
    omitted_root: Option<SemanticNodeId>,
) -> Result<HashSet<SemanticNodeId>, E> {
    let mut rows = HashSet::new();
    let mut seen = HashSet::new();
    let mut stack = roots.to_vec();
    while let Some(node_id) = stack.pop() {
        if !seen.insert(node_id) {
            continue;
        }
        let node = store.node(node_id).ok_or_else(|| {
            AuthoringError::from(noon_core::SemanticSceneOperationError::UnknownNode(node_id))
        })?;
        if let Some(graph) = node.graph_declaration() {
            for (_, binding) in graph.edges() {
                rows.insert(binding.line());
                if let SemanticGraphEdgeDependency::Arrow {
                    end_tip, start_tip, ..
                } = binding.dependency()
                {
                    rows.insert(end_tip);
                    rows.extend(start_tip);
                }
            }
        }
        if omitted_root != Some(node_id) {
            stack.extend(node.members_iter());
        }
        if matches!(node.kind(), SemanticNodeKind::Family(_)) {
            if let Some(layout) = store
                .semantic_table_layout(node_id)
                .map_err(AuthoringError::from)?
            {
                stack.extend(layout.referenced_nodes());
            }
        }
    }
    Ok(rows)
}

impl MobjectFamily {
    /// Copy the complete authored family, preserving order and internal aliases.
    pub fn copy_family(&self) -> Result<FamilyCopy, AuthoringError> {
        self.copy_with_references(&[])
    }

    /// Copy a family and additional detached references in one transaction.
    /// References may represent saved states or other host metadata. They are
    /// copied once, including aliases into the family, but never added as members.
    pub fn copy_with_references(
        &self,
        references: &[MobjectTarget<'_>],
    ) -> Result<FamilyCopy, AuthoringError> {
        authored_copy(self.into(), references, MobjectFamily::from_node)
    }
}

impl Mobject {
    /// Copy this object's own content and ordered descendants in one transaction.
    /// Additional metadata references share the same once-only identity mapping.
    pub fn copy_with_references(
        &self,
        references: &[MobjectTarget<'_>],
    ) -> Result<MobjectCopy, AuthoringError> {
        authored_copy(self.into(), references, Mobject::from_node)
    }
}

fn authored_copy<R>(
    source: MobjectTarget<'_>,
    references: &[MobjectTarget<'_>],
    from_node: fn(Rc<RefCell<SemanticStore>>, SemanticNodeId) -> Result<R, AuthoringError>,
) -> Result<SemanticCopy<R>, AuthoringError> {
    let (transaction, pending) = prepare_semantic_copy(
        source,
        references,
        |mobject, _| mobject.state(),
        |_, _, _| Ok(()),
        |store, source, target, transaction| {
            store
                .copy_effects_into(source, target, transaction)
                .map_err(AuthoringError::from)
        },
    )?;
    let result = transaction.apply(&mut source.integration_store().borrow_mut())?;
    pending.resolve(&result, from_node)
}

#[cfg(test)]
mod owned_root_fallback_copy_tests {
    use super::*;
    use crate::{Color, MobjectFamily, Scene};

    #[test]
    fn prepared_family_copy_captures_effective_scope_opacity_without_crossing_live_barrier() {
        use crate::{AnimationCompositionRequest, AnimationOptions, RateFunction};
        for start in [0.512345678901, 1.0] {
            let mut scene = Scene::new();
            let leaf = scene.circle(0.4).unwrap();
            let family = scene.family(&[(&leaf).into()]).unwrap();
            family.set_object_opacity(start).unwrap();
            scene.add_many(&[(&family).into()]).unwrap();
            let mut session = scene.execution_session().unwrap();
            let target = scene.live(&mut session).copy_family(&family).unwrap();
            assert_eq!(target.root().own_style().unwrap().object_opacity, start);
            scene
                .live(&mut session)
                .set_family_object_opacity(target.root(), 0.25)
                .unwrap();
            let segment = scene
                .live(&mut session)
                .declare_and_activate_composition(
                    &AnimationCompositionRequest::FamilyTransformTo {
                        source: &family,
                        target_state: target.root(),
                        options: AnimationOptions::new()
                            .run_time(1.0)
                            .rate_func(RateFunction::Linear),
                    },
                    AnimationOptions::new(),
                )
                .unwrap();
            session.advance_segment_to(segment, 0.5).unwrap();
            let object = session.execution_object_id(family.node_id()).unwrap();
            let effective = session
                .frame()
                .objects
                .iter()
                .find(|row| row.id == object)
                .unwrap()
                .style
                .opacity;
            let before = session.publication_context();
            let count = scene.integration_store().borrow().len();
            assert!(
                scene.live(&mut session).copy_family(&family).is_err(),
                "copy publication retains the ordinary pending-completion barrier"
            );
            let (copy, pending) = prepare_semantic_copy::<AuthoringError>(
                (&family).into(),
                &[],
                |object, graph_row| {
                    crate::effective_capture::capture_mobject_state_with_graph_dependency(
                        scene.integration_store(),
                        &session,
                        object,
                        graph_row,
                    )
                },
                |store, source, style| {
                    crate::effective_capture::capture_family_style(store, &session, source, style)
                },
                |store, source, target, transaction| {
                    crate::effective_capture::stage_effect_copy(
                        store,
                        &session,
                        source,
                        target,
                        transaction,
                    )
                },
            )
            .unwrap();
            {
                let mut store = scene.integration_store().borrow_mut();
                let prepared = copy.prepare(&mut store).unwrap();
                assert_eq!(
                    prepared
                        .family_style(pending.root_token())
                        .unwrap()
                        .object_opacity,
                    f64::from(effective),
                    "copy preparation captures owned composite opacity"
                );
                let copy = prepared
                    .family_opacity_copy_provenance(pending.root_token())
                    .unwrap()
                    .unwrap();
                assert_eq!(copy.source(), family.node_id());
                assert_eq!(copy.copied_opacity(), f64::from(effective));
                assert!(!copy.writes_object_opacity());
            }
            assert_eq!(family.own_style().unwrap().object_opacity, start);
            assert_eq!(leaf.state().unwrap().style.object_opacity, 1.0);
            assert_eq!(session.publication_context(), before);
            assert_eq!(scene.integration_store().borrow().len(), count);
        }
    }

    fn fallback(family: &MobjectFamily) -> Color {
        let store = family.integration_store().borrow();
        match store.node(family.node_id()).unwrap().kind() {
            noon_core::SemanticNodeKind::Family(
                noon_core::SemanticFamilyState::ContentOwning { fallback_color },
            ) => *fallback_color,
            kind => panic!("expected an owned family root, got {kind:?}"),
        }
    }

    #[test]
    fn authored_and_live_copies_preserve_fallback_independently_from_visible_paint() {
        let mut scene = Scene::new();
        let child = scene.square(0.2).unwrap();
        let source = scene
            .family_with_presentation(&[(&child).into()], 0.0, Color::RED, true)
            .unwrap();
        source
            .set_points_f64(&[(0.0, 0.0), (0.1, 0.0), (0.2, 0.1), (0.3, 0.2)])
            .unwrap();
        source
            .own_object()
            .unwrap()
            .unwrap()
            .set_fill(0.0, 0.0, 1.0, 1.0)
            .unwrap();
        assert_eq!(fallback(&source), Color::RED);
        assert_ne!(
            scene
                .integration_store()
                .borrow()
                .node(source.node_id())
                .unwrap()
                .family_color(),
            Some(Color::RED)
        );
        let style = source.own_object().unwrap().unwrap().state().unwrap().style;
        let authored = source.copy_family().unwrap().root().clone();
        scene.add_many(&[(&source).into()]).unwrap();
        let mut session = scene.execution_session().unwrap();
        let live = scene
            .live(&mut session)
            .copy_family(&source)
            .unwrap()
            .root()
            .clone();
        for copied in [authored, live] {
            assert_eq!(fallback(&copied), Color::RED);
            assert_eq!(
                copied.own_object().unwrap().unwrap().state().unwrap().style,
                style
            );
            assert_eq!(copied.get_num_points().unwrap(), 4);
            assert_eq!(copied.member_count().unwrap(), 1);
        }
        assert_eq!(fallback(&source), Color::RED);
    }
}
