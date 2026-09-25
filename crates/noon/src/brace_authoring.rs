//! Retained Brace composites with one authoritative semantic family transaction.
//!
//! A Brace remains ordinary vector geometry.  `BraceLabel` owns only the
//! relationship between that geometry and an existing object-or-family label;
//! no renderer state or frontend layout cache participates in the operation.

use crate::{
    family_authoring::FamilyTranslation,
    family_layout::RelativePlacement,
    geometry_authoring::{prepare_brace_geometry, PreparedBraceGeometry},
    AuthoringError, LayoutAnchor, LiveSession, LiveSessionError, ManimGeometryOptions,
    ManimNextToArgs, Mobject, MobjectFamily, Scene,
};
use noon_core::{
    SemanticMutationTransaction, SemanticMutationTransactionResult, SemanticNodeCreation,
    SemanticNodeId, SemanticStore,
};
use std::{cell::RefCell, rc::Rc};

/// Typed retained Brace construction parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BraceOptions {
    pub direction: (f64, f64),
    pub buff: f64,
    pub sharpness: f64,
    /// Distance between the Brace tip and its label.
    pub label_buff: f64,
}

impl Default for BraceOptions {
    fn default() -> Self {
        Self {
            direction: (0.0, -1.0),
            buff: 0.2,
            sharpness: 2.0,
            label_buff: f64::from(crate::DEFAULT_MOBJECT_TO_MOBJECT_BUFFER),
        }
    }
}

/// One retained Brace object and the immutable options used to construct it.
#[derive(Clone, Debug)]
pub struct Brace {
    object: Mobject,
    options: BraceOptions,
}

impl Brace {
    /// Construct one detached Brace through the Scene's normal publication route.
    pub fn new(
        scene: &mut Scene,
        target: &LayoutAnchor,
        options: BraceOptions,
    ) -> Result<Self, AuthoringError> {
        require_anchor_store(scene.integration_store(), target)?;
        let object = scene.geometry(ManimGeometryOptions::brace(
            target,
            options.direction,
            options.buff,
            options.sharpness,
        )?)?;
        Ok(Self { object, options })
    }

    pub fn object(&self) -> &Mobject {
        &self.object
    }

    pub const fn options(&self) -> BraceOptions {
        self.options
    }
}

/// A Brace plus an existing native object or semantic family label.
///
/// Labels are represented by `LayoutAnchor`, so a multi-part `MathTex` family
/// remains a family throughout placement and replacement.
#[derive(Clone, Debug)]
pub struct BraceLabel {
    family: MobjectFamily,
    brace: Brace,
    label: LayoutAnchor,
    options: BraceOptions,
}

/// A `BraceLabel` whose label is ordinary retained native text.
#[derive(Clone, Debug)]
pub struct BraceText {
    inner: BraceLabel,
}

impl BraceLabel {
    /// Atomically create the Brace, position the existing label, and publish the
    /// composite family. The caller may pass either an object or a semantic
    /// family via `LayoutAnchor`.
    pub fn new(
        scene: &mut Scene,
        target: &LayoutAnchor,
        label: LayoutAnchor,
        options: BraceOptions,
    ) -> Result<Self, AuthoringError> {
        let store = Rc::clone(scene.integration_store());
        let prepared = PreparedComposite::new(&store, target, label.clone(), options)?;
        let (brace, family) = scene.with_semantic_publication(|store, publish| {
            publish_composite(store, &prepared, None, publish)
        })?;
        Self::from_committed(store, brace, family, label, options)
    }

    /// Live equivalent of [`BraceLabel::new`], retaining the caller's existing
    /// execution ownership and publication revision.
    pub fn new_live(
        live: &mut LiveSession<'_>,
        target: &LayoutAnchor,
        label: LayoutAnchor,
        options: BraceOptions,
    ) -> Result<Self, LiveSessionError> {
        let store = Rc::clone(live.integration_store());
        let geometry = live.prepare_brace_geometry(target, &label, options)?;
        let prepared = PreparedComposite::with_geometry(label.clone(), options, geometry)?;
        let (brace, family) = live.with_semantic_publication(|store, publish| {
            publish_composite(store, &prepared, None, publish)
        })?;
        Self::from_committed(store, brace, family, label, options).map_err(Into::into)
    }

    /// Rebind a copied composite using its ordinary family members.
    pub fn from_family(
        family: MobjectFamily,
        options: BraceOptions,
    ) -> Result<Self, AuthoringError> {
        let store = Rc::clone(family.integration_store());
        let members = store
            .borrow()
            .semantic_family_members_checked(family.node_id())?;
        let [brace, label] = members.as_slice() else {
            return Err(AuthoringError::NonFiniteGeometry);
        };
        let brace = Mobject::from_node(Rc::clone(&store), *brace)?;
        // Verify the canonical tip query needed by subsequent label placement.
        if brace.path_query()?.start_anchors().len() <= 7 {
            return Err(AuthoringError::NonFiniteGeometry);
        }
        let label = LayoutAnchor::from_node(store, *label);
        label.layout()?;
        Ok(Self {
            family,
            brace: Brace {
                object: brace,
                options,
            },
            label,
            options,
        })
    }

    pub fn family(&self) -> &MobjectFamily {
        &self.family
    }

    pub fn brace(&self) -> &Brace {
        &self.brace
    }

    pub fn label(&self) -> &LayoutAnchor {
        &self.label
    }

    /// Replace only the Brace while preserving the current label identity.
    pub fn shift_brace(
        &mut self,
        scene: &mut Scene,
        target: &LayoutAnchor,
    ) -> Result<(), AuthoringError> {
        let store = Rc::clone(scene.integration_store());
        let prepared = PreparedComposite::new(&store, target, self.label.clone(), self.options)?;
        let old_brace = self.brace.object.node_id();
        let (brace, _) = scene.with_semantic_publication(|store, publish| {
            publish_composite(
                store,
                &prepared,
                Some((self.family.node_id(), old_brace, None)),
                publish,
            )
        })?;
        self.brace = Brace {
            object: Mobject::from_node(store, brace)?,
            options: self.options,
        };
        Ok(())
    }

    /// Replace the label and its placement in the same transaction as family
    /// membership. A failed publication leaves both the old composite and the
    /// supplied replacement label unchanged.
    pub fn change_label(
        &mut self,
        scene: &mut Scene,
        label: LayoutAnchor,
    ) -> Result<(), AuthoringError> {
        let store = Rc::clone(scene.integration_store());
        let prepared = PreparedLabelPlacement::for_existing_brace(
            &store,
            self.brace.object(),
            label.clone(),
            self.options,
        )?;
        let old_label = self.label.resolve()?;
        scene.with_semantic_publication(|store, publish| {
            publish_label_replacement(store, &prepared, self.family.node_id(), old_label, publish)
        })?;
        self.label = label;
        Ok(())
    }

    /// Replace the Brace and label together through one semantic transaction.
    pub fn change_brace_label(
        &mut self,
        scene: &mut Scene,
        target: &LayoutAnchor,
        label: LayoutAnchor,
    ) -> Result<(), AuthoringError> {
        let store = Rc::clone(scene.integration_store());
        let prepared = PreparedComposite::new(&store, target, label.clone(), self.options)?;
        let old_label = self.label.resolve()?;
        let old_brace = self.brace.object.node_id();
        let (brace, _) = scene.with_semantic_publication(|store, publish| {
            publish_composite(
                store,
                &prepared,
                Some((self.family.node_id(), old_brace, Some(old_label))),
                publish,
            )
        })?;
        self.brace = Brace {
            object: Mobject::from_node(Rc::clone(&store), brace)?,
            options: self.options,
        };
        self.label = label;
        Ok(())
    }

    pub fn shift_brace_live(
        &mut self,
        live: &mut LiveSession<'_>,
        target: &LayoutAnchor,
    ) -> Result<(), LiveSessionError> {
        let store = Rc::clone(live.integration_store());
        let geometry = live.prepare_brace_geometry(target, &self.label, self.options)?;
        let prepared =
            PreparedComposite::with_geometry(self.label.clone(), self.options, geometry)?;
        let old_brace = self.brace.object.node_id();
        let (brace, _) = live.with_semantic_publication(|store, publish| {
            publish_composite(
                store,
                &prepared,
                Some((self.family.node_id(), old_brace, None)),
                publish,
            )
        })?;
        self.brace = Brace {
            object: Mobject::from_node(store, brace)?,
            options: self.options,
        };
        Ok(())
    }

    pub fn change_label_live(
        &mut self,
        live: &mut LiveSession<'_>,
        label: LayoutAnchor,
    ) -> Result<(), LiveSessionError> {
        live.require_brace_label_placement(&label)?;
        live.require_brace_label_placement(&LayoutAnchor::from(self.brace.object()))?;
        let store = Rc::clone(live.integration_store());
        let prepared = PreparedLabelPlacement::for_existing_brace(
            &store,
            self.brace.object(),
            label.clone(),
            self.options,
        )?;
        let old_label = self.label.resolve()?;
        live.with_semantic_publication(|store, publish| {
            publish_label_replacement(store, &prepared, self.family.node_id(), old_label, publish)
        })?;
        self.label = label;
        Ok(())
    }

    pub fn change_brace_label_live(
        &mut self,
        live: &mut LiveSession<'_>,
        target: &LayoutAnchor,
        label: LayoutAnchor,
    ) -> Result<(), LiveSessionError> {
        let store = Rc::clone(live.integration_store());
        let geometry = live.prepare_brace_geometry(target, &label, self.options)?;
        let prepared = PreparedComposite::with_geometry(label.clone(), self.options, geometry)?;
        let old_label = self.label.resolve()?;
        let old_brace = self.brace.object.node_id();
        let (brace, _) = live.with_semantic_publication(|store, publish| {
            publish_composite(
                store,
                &prepared,
                Some((self.family.node_id(), old_brace, Some(old_label))),
                publish,
            )
        })?;
        self.brace = Brace {
            object: Mobject::from_node(Rc::clone(&store), brace)?,
            options: self.options,
        };
        self.label = label;
        Ok(())
    }

    fn from_committed(
        store: Rc<RefCell<SemanticStore>>,
        brace: SemanticNodeId,
        family: SemanticNodeId,
        label: LayoutAnchor,
        options: BraceOptions,
    ) -> Result<Self, AuthoringError> {
        Ok(Self {
            family: MobjectFamily::from_node(Rc::clone(&store), family)?,
            brace: Brace {
                object: Mobject::from_node(store, brace)?,
                options,
            },
            label,
            options,
        })
    }
}

impl BraceText {
    /// Construct native text first, then atomically attach and place it with the
    /// Brace family. Text resource shaping remains native and renderer-neutral.
    #[cfg(feature = "native-text")]
    pub fn new(
        scene: &mut Scene,
        target: &LayoutAnchor,
        text: impl Into<crate::Text>,
        options: BraceOptions,
    ) -> Result<Self, crate::TextAuthoringError> {
        let label = scene.text(text)?;
        BraceLabel::new(scene, target, LayoutAnchor::from(&label), options)
            .map(|inner| Self { inner })
            .map_err(crate::TextAuthoringError::Semantic)
    }

    pub fn inner(&self) -> &BraceLabel {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut BraceLabel {
        &mut self.inner
    }
}

struct PreparedComposite {
    brace: PreparedBraceGeometry,
    label: SemanticNodeId,
    translation: FamilyTranslation,
}

struct PreparedLabelPlacement {
    label: SemanticNodeId,
    translation: FamilyTranslation,
}

impl PreparedLabelPlacement {
    fn for_existing_brace(
        store: &Rc<RefCell<SemanticStore>>,
        brace: &Mobject,
        label: LayoutAnchor,
        options: BraceOptions,
    ) -> Result<Self, AuthoringError> {
        if !Rc::ptr_eq(store, brace.integration_store()) {
            return Err(AuthoringError::ForeignStore);
        }
        require_anchor_store(store, &label)?;
        let anchors = brace.path_query()?.start_anchors();
        let tip = *anchors.get(7).ok_or(AuthoringError::NonFiniteGeometry)?;
        let center = brace.center()?;
        let outward = (tip.0 - center.0, tip.1 - center.1);
        let length = outward.0.hypot(outward.1);
        if length == 0.0 || !length.is_finite() {
            return Err(AuthoringError::NonFiniteGeometry);
        }
        let direction = ((outward.0 / length).round(), (outward.1 / length).round());
        let layout = label.layout()?;
        let (x, y) = RelativePlacement::Next(ManimNextToArgs {
            direction,
            buff: options.label_buff,
            aligned_edge: (0.0, 0.0),
            mask: (1.0, 1.0),
        })
        .delta::<AuthoringError>(layout.boundary_bounds(), |_, _| Ok(tip))?;
        Ok(Self {
            label: label.resolve()?,
            translation: FamilyTranslation::from_members(layout.leaves().to_vec(), x, y)?,
        })
    }
}

impl PreparedComposite {
    fn new(
        store: &Rc<RefCell<SemanticStore>>,
        target: &LayoutAnchor,
        label: LayoutAnchor,
        options: BraceOptions,
    ) -> Result<Self, AuthoringError> {
        require_anchor_store(store, target)?;
        require_anchor_store(store, &label)?;
        let brace =
            prepare_brace_geometry(target, options.direction, options.buff, options.sharpness)?;
        Self::with_geometry(label, options, brace)
    }

    fn with_geometry(
        label: LayoutAnchor,
        options: BraceOptions,
        brace: PreparedBraceGeometry,
    ) -> Result<Self, AuthoringError> {
        let label_node = label.resolve()?;
        let layout = label.layout()?;
        let direction = (brace.direction.0.round(), brace.direction.1.round());
        let (x, y) = RelativePlacement::Next(ManimNextToArgs {
            direction,
            buff: options.label_buff,
            aligned_edge: (0.0, 0.0),
            mask: (1.0, 1.0),
        })
        .delta::<AuthoringError>(layout.boundary_bounds(), |_, _| Ok(brace.tip))?;
        Ok(Self {
            brace,
            label: label_node,
            translation: FamilyTranslation::from_members(layout.leaves().to_vec(), x, y)?,
        })
    }
}

fn publish_composite(
    store: &mut SemanticStore,
    prepared: &PreparedComposite,
    replacement: Option<(SemanticNodeId, SemanticNodeId, Option<SemanticNodeId>)>,
    publish: &mut dyn FnMut(
        &mut SemanticStore,
        SemanticMutationTransaction,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError>,
) -> Result<(SemanticNodeId, SemanticNodeId), AuthoringError> {
    prepared
        .brace
        .options
        .clone()
        .with_state(store, |store, state| {
            let mut transaction = SemanticMutationTransaction::new();
            let brace = transaction.create_node(SemanticNodeCreation::object(state));
            prepared
                .translation
                .clone()
                .stage(&mut transaction, store)?;
            let (existing_family, created_family) =
                if let Some((family, old_brace, old_label)) = replacement {
                    transaction.remove_member(family, old_brace);
                    if let Some(old_label) = old_label {
                        transaction.remove_member(family, old_label);
                    }
                    transaction.add_member(family, brace);
                    transaction.add_member(family, prepared.label);
                    (Some(family), None)
                } else {
                    let family = transaction.create_node(SemanticNodeCreation::family());
                    transaction.add_member(family, brace);
                    transaction.add_member(family, prepared.label);
                    (None, Some(family))
                };
            let result = publish(store, transaction)?;
            let brace = result
                .resolve(brace)
                .ok_or(AuthoringError::UnresolvedCreatedNode(brace))?;
            let family = match created_family {
                Some(family) => result
                    .resolve(family)
                    .ok_or(AuthoringError::UnresolvedCreatedNode(family))?,
                None => existing_family.expect("replacement supplies an existing family"),
            };
            Ok((brace, family))
        })
}

fn publish_label_replacement(
    store: &mut SemanticStore,
    prepared: &PreparedLabelPlacement,
    family: SemanticNodeId,
    old_label: SemanticNodeId,
    publish: &mut dyn FnMut(
        &mut SemanticStore,
        SemanticMutationTransaction,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError>,
) -> Result<(), AuthoringError> {
    let mut transaction = SemanticMutationTransaction::new();
    prepared
        .translation
        .clone()
        .stage(&mut transaction, store)?;
    transaction.remove_member(family, old_label);
    transaction.add_member(family, prepared.label);
    publish(store, transaction).map(|_| ())
}

fn require_anchor_store(
    store: &Rc<RefCell<SemanticStore>>,
    anchor: &LayoutAnchor,
) -> Result<(), AuthoringError> {
    if !Rc::ptr_eq(store, anchor.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    anchor.resolve().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MobjectTarget, DOWN};

    fn members(family: &MobjectFamily) -> Vec<SemanticNodeId> {
        family
            .integration_store()
            .borrow()
            .semantic_family_checked(family.node_id())
            .unwrap()
            .members()
            .to_vec()
    }

    #[test]
    fn label_replacement_keeps_brace_identity_and_family_topology() {
        let mut scene = Scene::new();
        let target = scene.square(2.0).unwrap();
        let first = scene.square(0.4).unwrap();
        let mut composite = BraceLabel::new(
            &mut scene,
            &LayoutAnchor::from(&target),
            LayoutAnchor::from(&first),
            BraceOptions::default(),
        )
        .unwrap();
        let tip = composite
            .brace()
            .object()
            .path_query()
            .unwrap()
            .start_anchors()[7];
        let first_bounds = first.layout_bounds().unwrap().unwrap();
        assert!(
            tip.0.abs() < 0.001,
            "the brace tip lies below the target center"
        );
        assert!(((first_bounds.min_x + first_bounds.max_x) * 0.5 - tip.0).abs() < 0.001);
        assert!((first_bounds.max_y - (tip.1 - 0.25)).abs() < 0.001);
        let brace = composite.brace().object().node_id();
        let replacement = scene.square(0.6).unwrap();

        composite
            .change_label(&mut scene, LayoutAnchor::from(&replacement))
            .unwrap();

        assert_eq!(composite.brace().object().node_id(), brace);
        assert_eq!(composite.label().resolve().unwrap(), replacement.node_id());
        let replacement_bounds = replacement.layout_bounds().unwrap().unwrap();
        assert!(
            ((replacement_bounds.min_x + replacement_bounds.max_x) * 0.5 - tip.0).abs() < 0.001
        );
        assert!((replacement_bounds.max_y - (tip.1 - 0.25)).abs() < 0.001);
        let members = members(composite.family());
        assert_eq!(members, vec![brace, replacement.node_id()]);
    }

    #[test]
    fn replacement_failure_preserves_membership_and_new_label_position() {
        let mut scene = Scene::new();
        let target = scene.square(2.0).unwrap();
        let first = scene.square(0.4).unwrap();
        let mut composite = BraceLabel::new(
            &mut scene,
            &LayoutAnchor::from(&target),
            LayoutAnchor::from(&first),
            BraceOptions::default(),
        )
        .unwrap();
        let before_members = members(composite.family());
        let mut foreign_scene = Scene::new();
        let foreign = foreign_scene.square(0.5).unwrap();

        assert!(composite
            .change_label(&mut scene, LayoutAnchor::from(&foreign))
            .is_err());
        assert_eq!(members(composite.family()), before_members);
        assert_eq!(composite.label().resolve().unwrap(), first.node_id());
    }

    #[test]
    fn family_label_is_placed_and_replaced_as_one_anchor() {
        let mut scene = Scene::new();
        let target = scene.square(2.0).unwrap();
        let left = scene.square(0.2).unwrap();
        let right = scene.square(0.2).unwrap();
        let label = scene
            .family(&[MobjectTarget::Object(&left), MobjectTarget::Object(&right)])
            .unwrap();
        let mut composite = BraceLabel::new(
            &mut scene,
            &LayoutAnchor::from(&target),
            LayoutAnchor::from(&label),
            BraceOptions {
                direction: (DOWN.x as f64, DOWN.y as f64),
                ..Default::default()
            },
        )
        .unwrap();
        let replacement_target = scene.rectangle(3.0, 1.0).unwrap();
        composite
            .shift_brace(&mut scene, &LayoutAnchor::from(&replacement_target))
            .unwrap();
        assert_eq!(members(composite.family()).len(), 2);
        assert_eq!(composite.label().resolve().unwrap(), label.node_id());
    }
    #[test]
    fn live_brace_observes_reactive_target_without_rewriting_it() {
        let mut scene = Scene::new();
        let target = scene.square(2.0).unwrap();
        let label = scene.square(0.4).unwrap();
        let pointer = scene.pointer_position_signal().unwrap();
        scene.bind_native_translation(&target, &pointer).unwrap();
        scene.add(&target).unwrap();
        let mut session = scene.execution_session().unwrap();
        session
            .set_native_state_input(
                noon_core::NativeStateSource::PointerPosition,
                noon_core::NativeInputValue::Vec2(noon_core::Vec2::new(3.0, 1.0)),
            )
            .unwrap();
        let authored_target = target.state().unwrap();
        let mut live = scene.live(&mut session);
        let composite = BraceLabel::new_live(
            &mut live,
            &LayoutAnchor::from(&target),
            LayoutAnchor::from(&label),
            BraceOptions::default(),
        )
        .unwrap();
        let bounds = composite.brace().object().layout_bounds().unwrap().unwrap();
        assert!(((bounds.min_x + bounds.max_x) * 0.5 - 3.0).abs() < 0.001);
        assert!((bounds.max_y + 0.2).abs() < 0.001);
        assert_eq!(target.state().unwrap(), authored_target);
    }
}
