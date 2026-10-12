use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::semantic_animations::{normalize_text_reveal_options, normalize_text_write_options};
use super::semantic_declarations::{
    close_all_updater_registrations, close_first_updater_registration, insert_updater_registration,
    UpdaterRegistrationEditError,
};
use crate::semantic_store::SemanticRemoveNodeEffect;
use crate::{
    AnimationOptions, HostCallbackId, ManimCamera3DProfile, SemanticAffineLifecycleDirection,
    SemanticAffineLifecycleEndpoint, SemanticAnimationCompositionKind, SemanticAnimationState,
    SemanticBarMetadata, SemanticClickIndicate, SemanticDecimalNumber, SemanticFadeDirection,
    SemanticFadeEndpoint, SemanticFamilyTransformMode, SemanticNodeId, SemanticNodeKind,
    SemanticObjectContent, SemanticObjectProperty, SemanticObjectRole, SemanticObjectState,
    SemanticObjectTrackProperty, SemanticObjectTrackValues, SemanticScalarSignalHold,
    SemanticScalarSignalTimelineEntry, SemanticScalarSignalTrack, SemanticScalarSignalTrackError,
    SemanticSceneOperationError, SemanticSignalBinding, SemanticSignalError, SemanticSignalSource,
    SemanticSignalValue, SemanticSignalValueKind, SemanticSpatialCompositionDomain,
    SemanticSpatialMaterial, SemanticStore, SemanticStoreError, SemanticStyle, SemanticTableLayout,
    SemanticTransactionGraphDeclaration, SemanticTransactionGraphEdgeDependency, SemanticTransform,
    SemanticTransformInterpolation, SemanticUpdaterEndpointPolicy, SemanticUpdaterRegistration,
    SemanticVec3, StoredGeometry, TextPresentationBaseline, VectorPath,
};
use crate::{CompositionTimeMap, TrackTiming};

mod inset_view;
mod pending_resources;
mod prepared;
pub use prepared::{
    PendingGeometryPublicationError, PendingResourceExtensionError,
    PreparedSemanticMutationTransaction, SemanticTransactionReadError,
};

mod animation_addition;
use animation_addition::{commit_add_animation, preflight_transaction_animation};
pub use animation_addition::{SemanticTransactionAnimation, SemanticTransactionAnimationIntent};

mod family_edges;
use family_edges::FamilyEdgePreflight;

mod node_addition;
pub use node_addition::SemanticNodeCreation;
use node_addition::SemanticPendingPathObject;
use node_addition::{commit_add_node, preflight_add_node, validate_spatial_material_resource};

mod provisional;
use provisional::{
    conflicting_style_error, duplicate_mutation_error, invalid_style_error,
    non_finite_property_error, property_type_error, replace_object_binding,
    subscription_type_error,
};
use provisional::{next_transaction_id, TransactionNodeCatalog};
pub use provisional::{
    SemanticLocalNodeToken, SemanticLocalResourceToken, SemanticPendingNodeKind,
    SemanticTransactionNodeRef,
};

/// One mutation in the authoritative Semantic Scene transaction vocabulary.
///
/// Signal values, object properties, authored content, scene-node allocation,
/// family membership/order, reactive subscriptions, ordered updater registrations,
/// animation declarations, and structural deletion share the same transaction so
/// frontends, editors, and host integrations cannot invent subsystem-specific patch
/// paths. Dependency-expression rewiring remains authored declaration topology
/// rather than being conflated with a value update.
#[derive(Clone, Debug, PartialEq)]
pub enum SemanticMutation {
    UpdateEffect {
        effect: SemanticNodeId,
        update: crate::GlowUpdate,
    },
    SetSignal {
        signal: SemanticNodeId,
        value: SemanticSignalValue,
    },
    AddScalarSignalTrack {
        signal: SemanticNodeId,
        from: f64,
        to: f64,
        timing: TrackTiming,
        time_map: CompositionTimeMap,
    },
    SetScalarSignalAt {
        signal: SemanticNodeId,
        value: f64,
        time: f64,
    },
    SetProperty {
        object: SemanticTransactionNodeRef,
        property: SemanticObjectProperty,
        value: SemanticSignalValue,
    },
    /// Atomically replace the complete authored transform, including spatial orientation.
    SetObjectTransform {
        object: SemanticTransactionNodeRef,
        transform: SemanticTransform,
    },
    /// Replace or clear one object-owned click indication declaration.
    SetClickIndicate {
        object: SemanticTransactionNodeRef,
        binding: Option<SemanticClickIndicate>,
    },
    SetInset2DView {
        object: SemanticTransactionNodeRef,
        camera_frame: Option<SemanticTransactionNodeRef>,
        capture_own_display: bool,
    },
    SetSpatialCompositionDomain {
        object: SemanticTransactionNodeRef,
        domain: SemanticSpatialCompositionDomain,
        anchor_family: Option<SemanticTransactionNodeRef>,
    },
    /// Replace immutable native angular-driver intervals on one camera.
    SetCameraMotions {
        object: SemanticTransactionNodeRef,
        motions: std::sync::Arc<[crate::CameraAngularMotion]>,
    },
    /// Atomically replace a camera's Manim profile and its derived pose/lens.
    SetCameraProfile {
        object: SemanticTransactionNodeRef,
        profile: ManimCamera3DProfile,
        near: f64,
        far: f64,
    },
    ReplaceContent {
        object: SemanticTransactionNodeRef,
        content: SemanticObjectContent,
    },
    /// Replace retained DecimalNumber inputs together with a normal text-content
    /// mutation. This deliberately does not generalize object roles.
    ReplaceDecimalNumber {
        object: SemanticTransactionNodeRef,
        number: SemanticDecimalNumber,
    },
    ReplaceTextPresentationBaseline {
        object: SemanticTransactionNodeRef,
        baseline: Option<TextPresentationBaseline>,
    },
    /// Replace optional retained BarChart source metadata without changing the
    /// object's ordinary semantic role or visual state.
    SetBarMetadata {
        object: SemanticTransactionNodeRef,
        metadata: Option<Arc<SemanticBarMetadata>>,
    },
    SetZIndex {
        node: SemanticTransactionNodeRef,
        value: f64,
    },
    ReplaceStyle {
        object: SemanticTransactionNodeRef,
        style: SemanticStyle,
    },
    ChangeSubscription {
        object: SemanticTransactionNodeRef,
        property: SemanticObjectProperty,
        signal: Option<SemanticNodeId>,
    },
    AddUpdater {
        target: SemanticTransactionNodeRef,
        callback: HostCallbackId,
        active_from: f64,
        inactive_from: Option<f64>,
        endpoint_policy: SemanticUpdaterEndpointPolicy,
        position: Option<usize>,
    },
    RemoveUpdater {
        target: SemanticTransactionNodeRef,
        callback: HostCallbackId,
        inactive_from: f64,
    },
    ClearUpdaters {
        target: SemanticTransactionNodeRef,
        inactive_from: f64,
    },
    ScopeSignal {
        scope: SemanticTransactionNodeRef,
        signal: SemanticTransactionNodeRef,
    },
    /// Replace one family's ordered foreground declarations without changing
    /// display membership. A membership planner must author any corresponding
    /// family edits in this same transaction.
    SetForegroundMembers {
        scope: SemanticTransactionNodeRef,
        members: Vec<SemanticTransactionNodeRef>,
    },
    /// Attach the initial authored Graph/DiGraph topology to one family root.
    ///
    /// This is construction-time whole-declaration publication. Persistent graph
    /// edits use local graph mutations rather than replacing the whole topology.
    SetGraphDeclaration {
        scope: SemanticTransactionNodeRef,
        graph: SemanticTransactionGraphDeclaration,
    },
    SetTableLayout {
        scope: SemanticTransactionNodeRef,
        layout: SemanticTableLayout,
    },
    AddMember {
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    },
    RemoveMember {
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    },
    ReorderMember {
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
        before: Option<SemanticTransactionNodeRef>,
    },
    AddNode {
        token: SemanticLocalNodeToken,
        creation: SemanticNodeCreation,
    },
    AddAnimation {
        token: SemanticLocalNodeToken,
        animation: SemanticTransactionAnimation,
    },
    RemoveAnimation {
        animation: SemanticNodeId,
    },
    RemoveNode {
        node: SemanticTransactionNodeRef,
    },
}

impl SemanticMutation {
    fn node_references(&self) -> Vec<SemanticTransactionNodeRef> {
        match self {
            Self::SetZIndex { node: object, .. }
            | Self::SetProperty { object, .. }
            | Self::SetObjectTransform { object, .. }
            | Self::SetCameraProfile { object, .. }
            | Self::SetCameraMotions { object, .. }
            | Self::SetClickIndicate { object, .. }
            | Self::ReplaceContent { object, .. }
            | Self::ReplaceDecimalNumber { object, .. }
            | Self::ReplaceTextPresentationBaseline { object, .. }
            | Self::SetBarMetadata { object, .. }
            | Self::ReplaceStyle { object, .. }
            | Self::ChangeSubscription { object, .. } => vec![*object],
            Self::SetSpatialCompositionDomain {
                object,
                anchor_family,
                ..
            } => {
                let mut refs = vec![*object];
                refs.extend(*anchor_family);
                refs
            }
            Self::SetInset2DView {
                object,
                camera_frame,
                ..
            } => std::iter::once(*object)
                .chain(camera_frame.iter().copied())
                .collect(),
            Self::AddUpdater { target, .. }
            | Self::RemoveUpdater { target, .. }
            | Self::ClearUpdaters { target, .. } => vec![*target],
            Self::ScopeSignal { scope, signal } => vec![*scope, *signal],
            Self::SetForegroundMembers { scope, members } => std::iter::once(*scope)
                .chain(members.iter().copied())
                .collect(),
            Self::SetGraphDeclaration { scope, graph } => std::iter::once(*scope)
                .chain(graph.node_references())
                .collect(),
            Self::SetTableLayout { scope, .. } => vec![*scope],
            Self::AddMember { family, member } | Self::RemoveMember { family, member } => {
                vec![*family, *member]
            }
            Self::ReorderMember {
                family,
                member,
                before,
            } => {
                let mut references = vec![*family, *member];
                references.extend(*before);
                references
            }
            Self::RemoveNode { node } => vec![*node],
            Self::AddAnimation { animation, .. } => animation.intent().node_references().collect(),
            Self::AddNode {
                creation: SemanticNodeCreation::Effect { owner, .. },
                ..
            } => vec![*owner],
            Self::UpdateEffect { effect, .. } => vec![(*effect).into()],
            Self::SetSignal { .. }
            | Self::AddScalarSignalTrack { .. }
            | Self::SetScalarSignalAt { .. }
            | Self::AddNode { .. }
            | Self::RemoveAnimation { .. } => Vec::new(),
        }
    }

    fn references_any_pending(&self, removed: &HashSet<SemanticLocalNodeToken>) -> bool {
        self.node_references().into_iter().any(
            |node| matches!(node, SemanticTransactionNodeRef::Pending(token) if removed.contains(&token)),
        ) || matches!(self, Self::AddNode { token, .. } | Self::AddAnimation { token, .. } if removed.contains(token))
    }

    /// Existing semantic identity directly targeted by this mutation.
    ///
    /// Allocation mutations do not have an identity until commit and therefore
    /// return `None`. This keeps creation out of the existing-target conflict key
    /// space without reserving or manufacturing semantic identity before preflight.
    pub const fn target(&self) -> Option<SemanticNodeId> {
        match self {
            Self::UpdateEffect { effect, .. } => Some(*effect),
            Self::SetSignal { signal, .. } => Some(*signal),
            Self::AddScalarSignalTrack { signal, .. } => Some(*signal),
            Self::SetScalarSignalAt { signal, .. } => Some(*signal),
            Self::SetZIndex { node: object, .. }
            | Self::SetProperty { object, .. }
            | Self::SetObjectTransform { object, .. }
            | Self::SetClickIndicate { object, .. }
            | Self::ReplaceContent { object, .. }
            | Self::ReplaceDecimalNumber { object, .. }
            | Self::ReplaceTextPresentationBaseline { object, .. }
            | Self::SetBarMetadata { object, .. }
            | Self::ReplaceStyle { object, .. }
            | Self::SetInset2DView { object, .. }
            | Self::SetSpatialCompositionDomain { object, .. }
            | Self::SetCameraProfile { object, .. }
            | Self::SetCameraMotions { object, .. }
            | Self::ChangeSubscription { object, .. } => object.existing(),
            Self::AddUpdater { target, .. }
            | Self::RemoveUpdater { target, .. }
            | Self::ClearUpdaters { target, .. } => target.existing(),
            Self::ScopeSignal { scope, .. }
            | Self::SetForegroundMembers { scope, .. }
            | Self::SetGraphDeclaration { scope, .. } => scope.existing(),
            Self::SetTableLayout { scope, .. } => scope.existing(),
            Self::AddMember { family, .. }
            | Self::RemoveMember { family, .. }
            | Self::ReorderMember { family, .. } => family.existing(),
            Self::AddNode { .. } | Self::AddAnimation { .. } => None,
            Self::RemoveAnimation { animation } => Some(*animation),
            Self::RemoveNode { node } => node.existing(),
        }
    }

    const fn key(&self) -> Option<SemanticMutationKey> {
        match self {
            Self::UpdateEffect { .. } => None, // each explicit parameter has its own key
            Self::SetSignal { signal, .. } => Some(SemanticMutationKey::Signal(*signal)),
            Self::AddScalarSignalTrack { .. } | Self::SetScalarSignalAt { .. } => None,
            Self::SetProperty {
                object, property, ..
            } => Some(SemanticMutationKey::ObjectProperty {
                object: *object,
                property: *property,
            }),
            Self::SetObjectTransform { object, .. } => {
                Some(SemanticMutationKey::ObjectTransform(*object))
            }
            Self::SetClickIndicate { object, .. } => {
                Some(SemanticMutationKey::ClickIndicate(*object))
            }
            Self::ReplaceContent { object, .. } => {
                Some(SemanticMutationKey::ObjectContent(*object))
            }
            Self::SetBarMetadata { object, .. } => {
                Some(SemanticMutationKey::ObjectBarMetadata(*object))
            }
            Self::SetInset2DView { object, .. } => Some(SemanticMutationKey::ObjectRole(*object)),
            Self::SetSpatialCompositionDomain { object, .. } => {
                Some(SemanticMutationKey::SpatialCompositionDomain(*object))
            }
            Self::SetCameraMotions { object, .. } => {
                Some(SemanticMutationKey::CameraMotions(*object))
            }
            Self::SetCameraProfile { object, .. } => {
                Some(SemanticMutationKey::ObjectTransform(*object))
            }
            Self::ReplaceDecimalNumber { object, .. } => {
                Some(SemanticMutationKey::DecimalNumber(*object))
            }
            Self::ReplaceTextPresentationBaseline { object, .. } => {
                Some(SemanticMutationKey::TextPresentationBaseline(*object))
            }
            Self::SetZIndex { node, .. } => Some(SemanticMutationKey::ZIndex(*node)),
            Self::ReplaceStyle { object, .. } => Some(SemanticMutationKey::ObjectStyle(*object)),
            Self::ChangeSubscription {
                object, property, ..
            } => Some(SemanticMutationKey::Subscription {
                object: *object,
                property: *property,
            }),
            Self::AddUpdater { .. }
            | Self::RemoveUpdater { .. }
            | Self::ClearUpdaters { .. }
            | Self::ScopeSignal { .. }
            | Self::SetForegroundMembers { .. }
            | Self::SetGraphDeclaration { .. } => None,
            Self::SetTableLayout { .. } => None,
            Self::AddMember { family, member } | Self::RemoveMember { family, member } => {
                Some(SemanticMutationKey::FamilyEdge {
                    family: *family,
                    member: *member,
                })
            }
            Self::ReorderMember { family, member, .. } => Some(SemanticMutationKey::FamilyOrder {
                family: *family,
                member: *member,
            }),
            Self::AddNode { .. } | Self::AddAnimation { .. } => None,
            Self::RemoveAnimation { animation } => Some(SemanticMutationKey::NodeRemoval(
                SemanticTransactionNodeRef::Existing(*animation),
            )),
            Self::RemoveNode { node } => Some(SemanticMutationKey::NodeRemoval(*node)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum SemanticMutationKey {
    EffectParameter {
        effect: SemanticNodeId,
        parameter: crate::GlowParameter,
    },
    Signal(SemanticNodeId),
    ObjectProperty {
        object: SemanticTransactionNodeRef,
        property: SemanticObjectProperty,
    },
    ObjectTransform(SemanticTransactionNodeRef),
    CameraMotions(SemanticTransactionNodeRef),
    ObjectContent(SemanticTransactionNodeRef),
    ClickIndicate(SemanticTransactionNodeRef),
    ObjectBarMetadata(SemanticTransactionNodeRef),
    ObjectRole(SemanticTransactionNodeRef),
    SpatialCompositionDomain(SemanticTransactionNodeRef),
    DecimalNumber(SemanticTransactionNodeRef),
    TextPresentationBaseline(SemanticTransactionNodeRef),
    ObjectStyle(SemanticTransactionNodeRef),
    ZIndex(SemanticTransactionNodeRef),
    Subscription {
        object: SemanticTransactionNodeRef,
        property: SemanticObjectProperty,
    },
    FamilyEdge {
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    },
    FamilyOrder {
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    },
    NodeRemoval(SemanticTransactionNodeRef),
}

/// Locality classification emitted by committed semantic mutations.
///
/// Lowering/runtime consumers can use this without re-interpreting the mutation
/// payload. Structural cleanup emits impacts for every semantic declaration it
/// actually invalidates; no frontend- or subsystem-specific patch classification
/// is introduced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticMutationImpact {
    /// Structural attachment/order/lifetime change on one owner.
    EffectAttachment {
        owner: SemanticNodeId,
        effect: SemanticNodeId,
    },
    /// One changed parameter; attachment identity and order did not change.
    /// Explicit writes of an unchanged value participate in conflict detection
    /// but emit no dirty impact.
    EffectParameter {
        owner: SemanticNodeId,
        effect: SemanticNodeId,
        parameter: crate::GlowParameter,
    },
    ZIndex {
        node: SemanticNodeId,
    },
    SignalValue {
        signal: SemanticNodeId,
    },
    SignalTimeline {
        signal: SemanticNodeId,
    },
    ObjectProperty {
        object: SemanticNodeId,
        property: SemanticObjectProperty,
    },
    ObjectTransform {
        object: SemanticNodeId,
    },
    ObjectContent {
        object: SemanticNodeId,
    },
    ClickIndicate {
        object: SemanticNodeId,
    },
    /// Retained BarChart source metadata changed without changing render data.
    BarMetadata {
        object: SemanticNodeId,
    },
    ObjectRole {
        object: SemanticNodeId,
    },
    SpatialCompositionDomain {
        object: SemanticNodeId,
    },
    CameraProfile {
        object: SemanticNodeId,
    },
    CameraMotions {
        object: SemanticNodeId,
    },
    SpatialAnchorChanged {
        object: SemanticNodeId,
    },
    DecimalNumber {
        object: SemanticNodeId,
    },
    TextPresentationBaseline {
        object: SemanticNodeId,
    },
    ObjectStyle {
        object: SemanticNodeId,
    },
    Subscription {
        object: SemanticNodeId,
        property: SemanticObjectProperty,
    },
    UpdaterRegistrations {
        target: SemanticNodeId,
    },
    SignalScoped {
        scope: SemanticNodeId,
        signal: SemanticNodeId,
    },
    /// Declaration-only metadata. Execution membership and painter order are
    /// unchanged unless separate ordinary family impacts accompany it.
    ForegroundMembers {
        scope: SemanticNodeId,
    },
    /// Authored graph topology/dependency meaning changed on this family root.
    GraphDeclaration {
        scope: SemanticNodeId,
    },
    FamilyMemberAdded {
        family: SemanticNodeId,
        member: SemanticNodeId,
    },
    FamilyMemberRemoved {
        family: SemanticNodeId,
        member: SemanticNodeId,
    },
    FamilyMemberReordered {
        family: SemanticNodeId,
        member: SemanticNodeId,
        before: Option<SemanticNodeId>,
    },
    NodeAdded {
        node: SemanticNodeId,
    },
    AnimationAdded {
        animation: SemanticNodeId,
    },
    NodeRemoved {
        node: SemanticNodeId,
    },
}

#[derive(Debug, PartialEq)]
pub struct SemanticMutationTransaction {
    id: u32,
    next_token: u32,
    /// Raw immutable path payloads remain transaction-owned until the final
    /// resource/publication scope materializes them. They never enter a store
    /// arena merely because a callback constructed a provisional object.
    pending_resources: Option<Box<pending_resources::PendingResourceDeclarations>>,
    mutations: Vec<SemanticMutation>,
    // Prepared existing-handle membership stages are composed in callback
    // order. Their preflight overlay already validates each transition, while
    // the ordinary public transaction path continues to reject repeated keys.
    allow_repeated_membership_mutations: bool,
}

pub(super) struct SemanticTransactionPreflight {
    changed: Vec<bool>,
    staged_effects: HashMap<SemanticNodeId, crate::EffectDefinition>,
    animation_effect_snapshots:
        HashMap<SemanticLocalNodeToken, Box<crate::SemanticTransformEffectSnapshot>>,
    pending_effect_order: HashMap<SemanticTransactionNodeRef, Vec<SemanticLocalNodeToken>>,
    staged_family_z: HashMap<SemanticTransactionNodeRef, f64>,
    staged_objects: HashMap<SemanticTransactionNodeRef, SemanticObjectState>,
    staged_spatial_anchors: HashMap<SemanticTransactionNodeRef, SemanticTransactionNodeRef>,
    /// Final constructor fields for transaction-only retained paths. This is a
    /// narrow creation overlay, never a durable object-state substitute.
    staged_pending_paths: HashMap<SemanticLocalNodeToken, SemanticPendingPathObject>,
    staged_object_order: Vec<SemanticTransactionNodeRef>,
    staged_updaters: HashMap<SemanticTransactionNodeRef, Vec<SemanticUpdaterRegistration>>,
    family_edges: FamilyEdgePreflight,
    pending_creations: HashMap<SemanticLocalNodeToken, SemanticNodeCreation>,
    pending_animations: HashMap<SemanticLocalNodeToken, SemanticTransactionAnimation>,
    staged_signal_scope_additions: Vec<(SemanticTransactionNodeRef, SemanticTransactionNodeRef)>,
    staged_foreground: HashMap<SemanticTransactionNodeRef, Vec<SemanticTransactionNodeRef>>,
    removed_existing: HashSet<SemanticNodeId>,
    removed_pending: HashSet<SemanticLocalNodeToken>,
    /// Owners whose family anchor is cleared by this transaction's removal
    /// closure, staged before publication just like explicit object writes.
    spatial_anchor_cleared: Vec<SemanticNodeId>,
}

impl Default for SemanticMutationTransaction {
    fn default() -> Self {
        let id = next_transaction_id();
        Self {
            id,
            next_token: 0,
            pending_resources: None,
            mutations: Vec::new(),
            allow_repeated_membership_mutations: false,
        }
    }
}

impl SemanticMutationTransaction {
    /// Create an ordered named appearance attachment using the ordinary allocator.
    /// This initial semantic slice supports leaf owners only; renderer admission
    /// remains separately gated until effect lowering is available.
    pub fn create_effect(
        &mut self,
        owner: impl Into<SemanticTransactionNodeRef>,
        name: impl Into<std::sync::Arc<str>>,
        definition: impl Into<crate::EffectDefinition>,
    ) -> SemanticLocalNodeToken {
        self.create_node(SemanticNodeCreation::Effect {
            owner: owner.into(),
            name: name.into(),
            definition: definition.into(),
        })
    }

    pub fn update_effect(
        &mut self,
        effect: SemanticNodeId,
        update: crate::GlowUpdate,
    ) -> &mut Self {
        self.mutations
            .push(SemanticMutation::UpdateEffect { effect, update });
        self
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_signal(
        &mut self,
        signal: SemanticNodeId,
        value: impl Into<SemanticSignalValue>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::SetSignal {
            signal,
            value: value.into(),
        });
        self
    }

    pub fn add_scalar_signal_track(
        &mut self,
        signal: SemanticNodeId,
        from: f64,
        to: f64,
        timing: TrackTiming,
    ) -> &mut Self {
        self.add_scalar_signal_track_with_time_map(
            signal,
            from,
            to,
            timing,
            CompositionTimeMap::identity(),
        )
    }

    pub fn add_scalar_signal_track_with_time_map(
        &mut self,
        signal: SemanticNodeId,
        from: f64,
        to: f64,
        timing: TrackTiming,
        time_map: CompositionTimeMap,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::AddScalarSignalTrack {
            signal,
            from,
            to,
            timing,
            time_map,
        });
        self
    }

    /// Persist one scalar input value from an explicit authored time onward.
    pub fn set_scalar_signal_at(
        &mut self,
        signal: SemanticNodeId,
        value: f64,
        time: f64,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::SetScalarSignalAt {
            signal,
            value,
            time,
        });
        self
    }

    /// Set one node-owned authored object property in the same atomic mutation
    /// vocabulary as signal values.
    pub fn set_property(
        &mut self,
        object: impl Into<SemanticTransactionNodeRef>,
        property: SemanticObjectProperty,
        value: impl Into<SemanticSignalValue>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::SetProperty {
            object: object.into(),
            property,
            value: value.into(),
        });
        self
    }

    /// Replace one object's complete authored transform through the shared
    /// staged mutation path. Spatial orientation cannot be represented through
    /// the legacy scalar RotationZ property.
    pub fn set_object_transform(
        &mut self,
        object: impl Into<SemanticTransactionNodeRef>,
        transform: SemanticTransform,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::SetObjectTransform {
            object: object.into(),
            transform,
        });
        self
    }

    /// Replace a prior staged property write for one transaction-local object.
    ///
    /// Pending construction may receive several source-level transform updates
    /// before publication. Coalescing only this local-node declaration avoids
    /// weakening duplicate-write validation for durable scene identities.
    pub fn replace_pending_object_property(
        &mut self,
        object: SemanticLocalNodeToken,
        property: SemanticObjectProperty,
        value: impl Into<SemanticSignalValue>,
    ) -> &mut Self {
        let value = value.into();
        if let Some(SemanticMutation::SetProperty {
            value: staged_value,
            ..
        }) = self.mutations.iter_mut().rev().find(|mutation| {
            matches!(mutation, SemanticMutation::SetProperty {
                object: SemanticTransactionNodeRef::Pending(candidate),
                property: candidate_property,
                ..
            } if *candidate == object && *candidate_property == property)
        }) {
            *staged_value = value;
            return self;
        }
        self.set_property(object, property, value)
    }

    /// Replace a prior staged style write for one transaction-local object.
    /// This has the same narrow coalescing scope as
    /// [`Self::replace_pending_object_property`].
    pub fn replace_pending_object_style(
        &mut self,
        object: SemanticLocalNodeToken,
        style: SemanticStyle,
    ) -> &mut Self {
        if let Some(SemanticMutation::ReplaceStyle {
            style: staged_style,
            ..
        }) = self.mutations.iter_mut().rev().find(|mutation| {
            matches!(mutation, SemanticMutation::ReplaceStyle {
                object: SemanticTransactionNodeRef::Pending(candidate),
                ..
            } if *candidate == object)
        }) {
            *staged_style = style;
            return self;
        }
        self.replace_style(object, style)
    }

    /// Replace the one authored primary-click indication on an object. `None`
    /// clears the declaration without creating a second interaction registry.
    pub fn set_click_indicate(
        &mut self,
        object: impl Into<SemanticTransactionNodeRef>,
        binding: Option<SemanticClickIndicate>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::SetClickIndicate {
            object: object.into(),
            binding,
        });
        self
    }

    /// Bind ordinary rectangles as an inset display and camera in one publication.
    pub fn set_inset_2d_view(
        &mut self,
        object: impl Into<SemanticTransactionNodeRef>,
        camera_frame: impl Into<SemanticTransactionNodeRef>,
        capture_own_display: bool,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::SetInset2DView {
            object: object.into(),
            camera_frame: Some(camera_frame.into()),
            capture_own_display,
        });
        self
    }

    pub fn clear_inset_2d_view(
        &mut self,
        object: impl Into<SemanticTransactionNodeRef>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::SetInset2DView {
            object: object.into(),
            camera_frame: None,
            capture_own_display: false,
        });
        self
    }

    /// Change the camera-composition domain of one semantic object.
    pub fn set_spatial_composition_domain(
        &mut self,
        object: impl Into<SemanticTransactionNodeRef>,
        domain: SemanticSpatialCompositionDomain,
    ) -> &mut Self {
        self.mutations
            .push(SemanticMutation::SetSpatialCompositionDomain {
                object: object.into(),
                domain,
                anchor_family: None,
            });
        self
    }

    /// Set composition domain and an optional shared FixedOrientation anchor.
    /// The anchor must be the object itself or an ancestor family containing it.
    pub fn set_spatial_composition_domain_with_anchor(
        &mut self,
        object: impl Into<SemanticTransactionNodeRef>,
        domain: SemanticSpatialCompositionDomain,
        anchor_family: Option<SemanticNodeId>,
    ) -> &mut Self {
        self.set_spatial_composition_domain_with_anchor_ref(
            object,
            domain,
            anchor_family.map(SemanticTransactionNodeRef::Existing),
        )
    }

    /// Set composition domain with an optional existing or transaction-local
    /// FixedOrientation anchor. Pending anchors are resolved immediately before
    /// commit publication.
    pub fn set_spatial_composition_domain_with_anchor_ref(
        &mut self,
        object: impl Into<SemanticTransactionNodeRef>,
        domain: SemanticSpatialCompositionDomain,
        anchor_family: Option<SemanticTransactionNodeRef>,
    ) -> &mut Self {
        self.mutations
            .push(SemanticMutation::SetSpatialCompositionDomain {
                object: object.into(),
                domain,
                anchor_family,
            });
        self
    }

    /// Stage native camera-driver history without any host evaluation state.
    pub fn set_camera_motions(
        &mut self,
        object: impl Into<SemanticTransactionNodeRef>,
        motions: std::sync::Arc<[crate::CameraAngularMotion]>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::SetCameraMotions {
            object: object.into(),
            motions,
        });
        self
    }

    /// Stage a validated camera profile together with the pose and lens it derives.
    pub fn set_camera_profile(
        &mut self,
        object: impl Into<SemanticTransactionNodeRef>,
        profile: ManimCamera3DProfile,
        near: f64,
        far: f64,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::SetCameraProfile {
            object: object.into(),
            profile,
            near,
            far,
        });
        self
    }

    /// Replace only the authored content reference/value of one semantic object.
    pub fn replace_content(
        &mut self,
        object: impl Into<SemanticTransactionNodeRef>,
        content: impl Into<SemanticObjectContent>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::ReplaceContent {
            object: object.into(),
            content: content.into(),
        });
        self
    }

    /// Replace one object's typed numeric metadata. The mutation is valid only
    /// when the staged object content is text, so a bad late mutation rolls the
    /// complete transaction back before resources or scene state are committed.
    pub fn replace_decimal_number(
        &mut self,
        object: impl Into<SemanticTransactionNodeRef>,
        number: SemanticDecimalNumber,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::ReplaceDecimalNumber {
            object: object.into(),
            number,
        });
        self
    }

    /// Replace the optional BarChart source metadata for one ordinary object.
    ///
    /// The payload is pointer-sized and validated before publication. Passing
    /// `None` removes chart ownership while retaining the object's geometry,
    /// style, role, and identity.
    pub fn set_bar_metadata(
        &mut self,
        object: impl Into<SemanticTransactionNodeRef>,
        metadata: Option<Arc<SemanticBarMetadata>>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::SetBarMetadata {
            object: object.into(),
            metadata,
        });
        self
    }

    /// Replace receiver-owned presentation metadata atomically with related
    /// text content. It does not create renderer work by itself.
    pub fn replace_text_presentation_baseline(
        &mut self,
        object: impl Into<SemanticTransactionNodeRef>,
        baseline: Option<TextPresentationBaseline>,
    ) -> &mut Self {
        self.mutations
            .push(SemanticMutation::ReplaceTextPresentationBaseline {
                object: object.into(),
                baseline,
            });
        self
    }

    /// Replace the complete authored style of one semantic object.
    ///
    /// This carries paints and discrete stroke topology through the same atomic
    /// mutation vocabulary as scalar style properties. A transaction cannot mix
    /// full-style replacement with scalar style writes for the same object.
    pub fn replace_style(
        &mut self,
        object: impl Into<SemanticTransactionNodeRef>,
        style: SemanticStyle,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::ReplaceStyle {
            object: object.into(),
            style,
        });
        self
    }

    /// Set authored painter priority on an object or family root atomically.
    pub fn set_z_index(
        &mut self,
        node: impl Into<SemanticTransactionNodeRef>,
        value: f64,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::SetZIndex {
            node: node.into(),
            value,
        });
        self
    }

    /// Change the authored signal driver for one object property.
    pub fn change_subscription(
        &mut self,
        object: impl Into<SemanticTransactionNodeRef>,
        property: SemanticObjectProperty,
        signal: Option<SemanticNodeId>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::ChangeSubscription {
            object: object.into(),
            property,
            signal,
        });
        self
    }

    /// Register one ordered host-updater occurrence on an object or family.
    ///
    /// `active_from` is inclusive authored time. `position` indexes the occurrences
    /// active at that time; `None` appends after the last active occurrence.
    pub fn add_updater(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        callback: HostCallbackId,
        active_from: f64,
        position: Option<usize>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::AddUpdater {
            target: target.into(),
            callback,
            active_from,
            inactive_from: None,
            endpoint_policy: SemanticUpdaterEndpointPolicy::Exclusive,
            position,
        });
        self
    }

    /// Register one host-updater occurrence over a finite half-open interval.
    /// The start is inclusive and the end is exclusive.
    pub fn add_updater_interval(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        callback: HostCallbackId,
        active_from: f64,
        inactive_from: f64,
        position: Option<usize>,
    ) -> &mut Self {
        self.add_updater_interval_with_endpoint_policy(
            target,
            callback,
            active_from,
            inactive_from,
            SemanticUpdaterEndpointPolicy::Exclusive,
            position,
        )
    }

    /// Register one host-updater occurrence over a finite interval with explicit
    /// endpoint behavior. Ordinary updater APIs use `Exclusive`.
    pub fn add_updater_interval_with_endpoint_policy(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        callback: HostCallbackId,
        active_from: f64,
        inactive_from: f64,
        endpoint_policy: SemanticUpdaterEndpointPolicy,
        position: Option<usize>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::AddUpdater {
            target: target.into(),
            callback,
            active_from,
            inactive_from: Some(inactive_from),
            endpoint_policy,
            position,
        });
        self
    }

    /// Close the first occurrence of `callback` active at exclusive authored time.
    pub fn remove_updater(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        callback: HostCallbackId,
        inactive_from: f64,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::RemoveUpdater {
            target: target.into(),
            callback,
            inactive_from,
        });
        self
    }

    /// Close every updater occurrence active on a target at exclusive authored time.
    pub fn clear_updaters(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        inactive_from: f64,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::ClearUpdaters {
            target: target.into(),
            inactive_from,
        });
        self
    }

    /// Associate a signal with one family execution scope without changing
    /// painter membership. Repeated association is an exact no-op.
    pub fn scope_signal(
        &mut self,
        scope: impl Into<SemanticTransactionNodeRef>,
        signal: impl Into<SemanticTransactionNodeRef>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::ScopeSignal {
            scope: scope.into(),
            signal: signal.into(),
        });
        self
    }

    /// Replace one root's ordered foreground declarations atomically.
    ///
    /// Members must be distinct object/family references, may be provisional,
    /// and must survive this transaction. The scope itself cannot be a member.
    /// Equal declarations are an exact no-op. Display membership and ordering
    /// remain separate family edits, so this primitive is not `Scene.add_foreground`.
    pub fn set_foreground_members(
        &mut self,
        scope: impl Into<SemanticTransactionNodeRef>,
        members: impl IntoIterator<Item = impl Into<SemanticTransactionNodeRef>>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::SetForegroundMembers {
            scope: scope.into(),
            members: members.into_iter().map(Into::into).collect(),
        });
        self
    }

    /// Attach the initial authored Graph/DiGraph declaration to a family root.
    ///
    /// The declaration may reference nodes created by this transaction. It is
    /// validated against the final staged family membership and object content.
    pub fn set_graph_declaration(
        &mut self,
        scope: impl Into<SemanticTransactionNodeRef>,
        graph: SemanticTransactionGraphDeclaration,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::SetGraphDeclaration {
            scope: scope.into(),
            graph,
        });
        self
    }
    pub fn set_table_layout(
        &mut self,
        scope: impl Into<SemanticTransactionNodeRef>,
        layout: SemanticTableLayout,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::SetTableLayout {
            scope: scope.into(),
            layout,
        });
        self
    }

    /// Add one direct ordered family edge through the authoritative transaction.
    pub fn add_member(
        &mut self,
        family: impl Into<SemanticTransactionNodeRef>,
        member: impl Into<SemanticTransactionNodeRef>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::AddMember {
            family: family.into(),
            member: member.into(),
        });
        self
    }

    /// Remove one direct ordered family edge through the authoritative transaction.
    pub fn remove_member(
        &mut self,
        family: impl Into<SemanticTransactionNodeRef>,
        member: impl Into<SemanticTransactionNodeRef>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::RemoveMember {
            family: family.into(),
            member: member.into(),
        });
        self
    }

    /// Move one direct family member before another direct member, or to the tail.
    ///
    /// `before=None` means tail. Reordering preserves membership and parent edges;
    /// only the family's authoritative order is mutated.
    pub fn reorder_member(
        &mut self,
        family: impl Into<SemanticTransactionNodeRef>,
        member: impl Into<SemanticTransactionNodeRef>,
        before: Option<SemanticNodeId>,
    ) -> &mut Self {
        self.reorder_member_ref(family, member, before.map(Into::into))
    }

    /// Move a direct family member using existing or transaction-local identities.
    pub fn reorder_member_ref(
        &mut self,
        family: impl Into<SemanticTransactionNodeRef>,
        member: impl Into<SemanticTransactionNodeRef>,
        before: Option<SemanticTransactionNodeRef>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::ReorderMember {
            family: family.into(),
            member: member.into(),
            before,
        });
        self
    }

    /// Allocate one new detached semantic scene node after complete transaction
    /// preflight.
    ///
    /// Object/family creation uses the same scene-global generational allocator as
    /// all other semantic entities. The real identity is reported by `NodeAdded`;
    /// no provisional semantic ID is reserved before commit. Optional source
    /// identity is assigned atomically after terminal removals, allowing hot-reload
    /// replacement to transfer one stable source key from an old node to its new
    /// identity without creating a second identity model.
    pub fn add_node(&mut self, creation: SemanticNodeCreation) -> &mut Self {
        self.create_node(creation);
        self
    }

    /// Stage a node allocation and return its transaction-local reference token.
    pub fn create_node(&mut self, creation: SemanticNodeCreation) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations
            .push(SemanticMutation::AddNode { token, creation });
        token
    }

    /// Add one authored animation declaration after complete transaction preflight.
    ///
    /// The declaration references existing scene-global semantic identities. The
    /// newly allocated animation identity is reported by `AnimationAdded` in the
    /// transaction result, rather than allocating semantic identity before commit.
    pub fn add_animation(&mut self, state: SemanticAnimationState) -> &mut Self {
        self.create_animation(state);
        self
    }

    /// Stage an animation declaration and return its transaction-local node token.
    pub fn create_animation(&mut self, state: SemanticAnimationState) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::from_published(state),
        });
        token
    }

    /// Stage one exact object-channel animation under a new semantic animation identity.
    pub fn create_object_property_track(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        property: SemanticObjectTrackProperty,
        values: SemanticObjectTrackValues<SemanticTransactionNodeRef>,
        timing: TrackTiming,
        time_map: CompositionTimeMap,
    ) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::ObjectPropertyTrack {
                    target: target.into(),
                    property,
                    values,
                    timing,
                    time_map,
                },
                AnimationOptions::new(),
            ),
        });
        token
    }

    /// Add a transform declaration that may reference objects staged in this batch.
    pub fn add_transform_animation(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        target_state: impl Into<SemanticTransactionNodeRef>,
        options: AnimationOptions,
    ) -> &mut Self {
        self.create_transform_animation(target, target_state, options);
        self
    }

    /// Stage a transform declaration and return its transaction-local node token.
    pub fn create_transform_animation(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        target_state: impl Into<SemanticTransactionNodeRef>,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        self.create_transform_animation_with_interpolation(
            target,
            target_state,
            SemanticTransformInterpolation::Affine,
            false,
            options,
        )
    }

    pub fn create_move_along_path_animation(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        path: impl Into<SemanticTransactionNodeRef>,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::MoveAlongPath {
                    target: target.into(),
                    path: path.into(),
                },
                options,
            ),
        });
        token
    }

    /// Stage a transform declaration with an explicit geometry interpolation contract.
    pub fn create_transform_animation_with_interpolation(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        target_state: impl Into<SemanticTransactionNodeRef>,
        interpolation: SemanticTransformInterpolation,
        complete_priority: bool,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::TransformTo {
                    target: target.into(),
                    target_state: target_state.into(),
                    interpolation,

                    complete_priority,
                },
                options,
            ),
        });
        token
    }

    /// Stage a typed world-pose animation for one existing semantic object.
    pub fn create_world_transform_animation(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        transform: crate::SemanticWorldTransform3D,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::WorldTransformTo {
                    target: target.into(),
                    transform,
                },
                options,
            ),
        });
        token
    }

    /// Stage an animatable unwrapped Manim camera-profile endpoint.
    pub fn create_camera_profile_animation(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        profile: ManimCamera3DProfile,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::CameraProfileTo {
                    target: target.into(),
                    profile,
                },
                options,
            ),
        });
        token
    }

    /// Stage a family Transform without manufacturing semantic padding members.
    pub fn create_family_transform_animation(
        &mut self,
        source: impl Into<SemanticTransactionNodeRef>,
        target_state: impl Into<SemanticTransactionNodeRef>,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        self.create_family_transform_animation_with_mode(
            source,
            target_state,
            SemanticFamilyTransformMode::Structural,
            options,
        )
    }

    /// Stage activation-time normalized-shape correspondence for two semantic families.
    pub fn create_matching_family_transform_animation(
        &mut self,
        source: impl Into<SemanticTransactionNodeRef>,
        target_state: impl Into<SemanticTransactionNodeRef>,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        self.create_family_transform_animation_with_mode(
            source,
            target_state,
            SemanticFamilyTransformMode::MatchingShapes,
            options,
        )
    }

    /// Stage activation-time authored-source correspondence for retained text families.
    pub fn create_source_matching_family_transform_animation(
        &mut self,
        source: impl Into<SemanticTransactionNodeRef>,
        target_state: impl Into<SemanticTransactionNodeRef>,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        self.create_family_transform_animation_with_mode(
            source,
            target_state,
            SemanticFamilyTransformMode::MatchingSourceKeys,
            options,
        )
    }

    /// Stage a family Transform with an explicit semantic correspondence policy.
    pub fn create_family_transform_animation_with_mode(
        &mut self,
        source: impl Into<SemanticTransactionNodeRef>,
        target_state: impl Into<SemanticTransactionNodeRef>,
        mode: SemanticFamilyTransformMode,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::FamilyTransformTo {
                    source: source.into(),
                    target_state: target_state.into(),
                    mode,
                },
                options,
            ),
        });
        token
    }

    /// Stage a centered 2D angular-path rotation declaration.
    pub fn create_rotate_animation(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        angle: f64,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        self.create_rotate_animation_with_origin_constraint(target, angle, false, options)
    }

    /// Stage an angular path that optionally holds its activation-time world origin.
    pub fn create_rotate_animation_with_origin_constraint(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        angle: f64,
        hold_origin: bool,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::Rotate {
                    target: target.into(),
                    angle,
                    hold_origin,
                },
                options,
            ),
        });
        token
    }

    /// Stage one activation-relative restoring Indicate declaration.
    pub fn create_indicate_animation(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        scale_factor: f64,
        color: crate::Color,
        scale_center: crate::SemanticVec3,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::Indicate {
                    target: target.into(),
                    scale_factor,
                    color,
                    scale_center,
                },
                options,
            ),
        });
        token
    }

    /// Stage one exact-Line passing-flash declaration.
    pub fn create_passing_flash_animation(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        time_width: f64,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::PassingFlash {
                    target: target.into(),
                    time_width,
                },
                options,
            ),
        });
        token
    }

    /// Stage one activation-relative two-phase vector outline/fill declaration.
    pub fn create_draw_border_then_fill_animation(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        stroke_width: f64,
        stroke_color: Option<crate::Color>,
        phase_rate_function: crate::RateFunction,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::DrawBorderThenFill {
                    target: target.into(),
                    stroke_width,
                    stroke_color,
                    phase_rate_function,
                },
                options,
            ),
        });
        token
    }

    /// Stage one ordered member of a shared family subset display.
    pub fn create_subset_display_member_animation(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        index: usize,
        count: usize,
        mode: crate::SemanticSubsetDisplayMode,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::SubsetDisplayMember {
                    target: target.into(),
                    index,
                    count,
                    mode,
                },
                options,
            ),
        });
        token
    }

    /// Stage one forward retained-text Write declaration.
    pub fn create_text_write_animation(
        &mut self,
        target: SemanticNodeId,
        reverse_member_order: bool,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        self.create_text_glyph_animation(
            target,
            crate::FamilyAnimationMode::DrawBorderThenFill,
            reverse_member_order,
            None,
            normalize_text_write_options(reverse_member_order, options),
        )
    }

    /// Stage one retained-text leaf in a globally indexed family Write plan.
    pub fn create_family_text_write_member_animation(
        &mut self,
        target: SemanticNodeId,
        reverse_member_order: bool,
        family_member: crate::SemanticFamilyAnimationMember,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        self.create_family_animation_member(
            target,
            crate::FamilyAnimationMode::DrawBorderThenFill,
            reverse_member_order,
            family_member,
            normalize_text_write_options(reverse_member_order, options),
        )
    }

    /// Stage one retained-text Create/Uncreate through the shared Reveal mode.
    pub fn create_text_reveal_animation(
        &mut self,
        target: SemanticNodeId,
        reverse: bool,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        self.create_text_glyph_animation(
            target,
            crate::FamilyAnimationMode::Reveal,
            false,
            None,
            normalize_text_reveal_options(reverse, options),
        )
    }

    /// Stage one family Text Create/Uncreate leaf with authoritative family position.
    pub fn create_family_text_reveal_member_animation(
        &mut self,
        target: SemanticNodeId,
        reverse: bool,
        family_member: crate::SemanticFamilyAnimationMember,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        self.create_family_animation_member(
            target,
            crate::FamilyAnimationMode::Reveal,
            false,
            family_member,
            normalize_text_reveal_options(reverse, options),
        )
    }

    /// Stage one exact member of a retained family animation.
    ///
    /// The caller owns explicit timing, reversal, and lifecycle options. Unlike
    /// the Write/Create conveniences, this constructor applies no lifecycle
    /// defaults and keeps member-order reversal independent from local-rate
    /// reversal.
    pub fn create_family_animation_member(
        &mut self,
        target: SemanticNodeId,
        mode: crate::FamilyAnimationMode,
        reverse_member_order: bool,
        family_member: crate::SemanticFamilyAnimationMember,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        self.create_text_glyph_animation(
            target,
            mode,
            reverse_member_order,
            Some(family_member),
            options,
        )
    }

    fn create_text_glyph_animation(
        &mut self,
        target: SemanticNodeId,
        mode: crate::FamilyAnimationMode,
        reverse_member_order: bool,
        family_member: Option<crate::SemanticFamilyAnimationMember>,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::TextGlyph {
                    target: target.into(),
                    mode,
                    reverse_member_order,
                    family_member,
                },
                options,
            ),
        });
        token
    }

    /// Stage a single-leaf fade declaration and return its transaction-local token.
    pub fn create_fade_animation(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        direction: SemanticFadeDirection,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        self.create_fade_animation_with_endpoint(
            target,
            direction,
            SemanticFadeEndpoint::default(),
            options,
        )
    }

    /// Stage a Fade declaration with an activation-relative affine endpoint.
    pub fn create_fade_animation_with_endpoint(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        direction: SemanticFadeDirection,
        endpoint: SemanticFadeEndpoint,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::Fade {
                    target: target.into(),
                    direction,
                    endpoint,
                },
                options,
            ),
        });
        token
    }

    /// Stage one content-preserving affine lifecycle declaration.
    pub fn create_affine_lifecycle_animation(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        direction: SemanticAffineLifecycleDirection,
        endpoint: SemanticAffineLifecycleEndpoint,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::AffineLifecycle {
                    target: target.into(),
                    direction,
                    endpoint,
                },
                options,
            ),
        });
        token
    }

    /// Stage a single-leaf Create declaration and return its transaction-local token.
    pub fn create_create_animation(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::Create {
                    target: target.into(),
                },
                options,
            ),
        });
        token
    }

    /// Stage one timed structural admission declaration.
    pub fn create_add_animation(
        &mut self,
        target: impl Into<SemanticTransactionNodeRef>,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::Add {
                    target: target.into(),
                },
                options,
            ),
        });
        token
    }

    /// Stage one scalar input animation without allocating object identity.
    pub fn create_scalar_animation(
        &mut self,
        signal: SemanticNodeId,
        target: f64,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::SetScalar { signal, target },
                options,
            ),
        });
        token
    }

    /// Stage one targetless authored wait declaration.
    pub fn create_wait_animation(&mut self, duration: f64) -> SemanticLocalNodeToken {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::Wait,
                AnimationOptions::new().run_time(duration),
            ),
        });
        token
    }

    /// Stage an ordered animation composition whose children already exist or
    /// were staged earlier in this transaction.
    pub fn create_animation_composition<I, R>(
        &mut self,
        kind: SemanticAnimationCompositionKind,
        children: I,
        options: AnimationOptions,
    ) -> SemanticLocalNodeToken
    where
        I: IntoIterator<Item = R>,
        R: Into<SemanticTransactionNodeRef>,
    {
        let token = self.allocate_local_node_token();
        self.mutations.push(SemanticMutation::AddAnimation {
            token,
            animation: SemanticTransactionAnimation::new(
                SemanticTransactionAnimationIntent::Composition {
                    kind,
                    children: children.into_iter().map(Into::into).collect(),
                },
                options,
            ),
        });
        token
    }

    fn allocate_local_node_token(&mut self) -> SemanticLocalNodeToken {
        let ordinal = self.next_token;
        self.next_token = self
            .next_token
            .checked_add(1)
            .expect("Noon semantic transaction local-node space exhausted");
        SemanticLocalNodeToken::new(self.id, ordinal)
    }

    /// Delete one authored animation declaration through the same structural
    /// transaction path as node deletion.
    ///
    /// The target must be a live animation at the transaction boundary. Removing a
    /// child animation also removes parent compositions that can no longer remain
    /// valid; composition children are references and are not owned/deleted when a
    /// composition itself is removed.
    pub fn remove_animation(&mut self, animation: SemanticNodeId) -> &mut Self {
        self.mutations
            .push(SemanticMutation::RemoveAnimation { animation });
        self
    }

    /// Delete one semantic identity and atomically clean declarations that cannot
    /// remain valid without it.
    ///
    /// Signal bindings are unbound. Derived signals and animation declarations
    /// that reference the removed identity are themselves removed, recursively.
    /// Structural removals are terminal within a transaction: once the first
    /// structural removal is authored, no later non-removal mutation is accepted.
    /// This keeps complete preflight valid through commit without staging a second
    /// scene. Removing a pending node cancels its allocation and any staged edge or
    /// animation declaration that references it; those mutations are still fully
    /// preflighted so cancellation cannot hide an invalid family, kind, or value.
    pub fn remove_node(&mut self, node: impl Into<SemanticTransactionNodeRef>) -> &mut Self {
        self.mutations
            .push(SemanticMutation::RemoveNode { node: node.into() });
        self
    }

    pub fn mutations(&self) -> &[SemanticMutation] {
        &self.mutations
    }

    pub fn is_empty(&self) -> bool {
        self.mutations.is_empty()
    }

    /// Validate and reserve publication while holding the store exclusively.
    /// Dropping the returned proof discards the batch without changing the store.
    pub fn prepare(
        self,
        store: &mut SemanticStore,
    ) -> Result<PreparedSemanticMutationTransaction<'_>, SemanticMutationTransactionError> {
        PreparedSemanticMutationTransaction::new(self, store)
    }

    /// Validate while retaining this exact transaction when preflight rejects it.
    ///
    /// A callback collector can report one caught operation failure and continue
    /// staging its previously accepted operations without cloning transaction
    /// identity or local-node allocation state.
    pub fn prepare_recoverable(
        self,
        store: &mut SemanticStore,
    ) -> Result<PreparedSemanticMutationTransaction<'_>, (Self, SemanticMutationTransactionError)>
    {
        PreparedSemanticMutationTransaction::new_recoverable(self, store)
    }

    /// Preflight the complete transaction, then commit every changed mutation.
    pub fn apply(
        self,
        store: &mut SemanticStore,
    ) -> Result<SemanticMutationTransactionResult, SemanticMutationTransactionError> {
        store.set_last_mutation_writes(0);
        self.prepare(store)
            .map(PreparedSemanticMutationTransaction::commit)
    }

    fn preflight(
        &self,
        store: &SemanticStore,
    ) -> Result<SemanticTransactionPreflight, SemanticMutationTransactionError> {
        let catalog = TransactionNodeCatalog::new(self, store);
        let mut removed_nodes = HashSet::new();
        let mut removed_pending = HashSet::new();
        let mut removal_started = false;
        for (index, mutation) in self.mutations.iter().enumerate() {
            match mutation {
                SemanticMutation::RemoveAnimation { animation } => {
                    let Some(node) = store.node(*animation) else {
                        return Err(SemanticMutationTransactionError::UnknownAnimation {
                            index,
                            animation: *animation,
                        });
                    };
                    if !matches!(node.kind(), SemanticNodeKind::Animation(_)) {
                        return Err(SemanticMutationTransactionError::NotAnimation {
                            index,
                            animation: *animation,
                        });
                    }
                    removal_started = true;
                    removed_nodes.insert(*animation);
                }
                SemanticMutation::RemoveNode {
                    node: SemanticTransactionNodeRef::Existing(node),
                } => {
                    removal_started = true;
                    removed_nodes.insert(*node);
                }
                SemanticMutation::RemoveNode {
                    node: SemanticTransactionNodeRef::Pending(token),
                } => {
                    catalog.validate_pending((*token).into(), index)?;
                    removal_started = true;
                    removed_pending.insert(*token);
                }
                _ if removal_started => {
                    return Err(SemanticMutationTransactionError::MutationAfterRemove { index });
                }
                _ => {}
            }
        }
        // Pending animation declarations form an insertion-ordered DAG because
        // compositions can only reference earlier pending animations. Canceling
        // any pending dependency therefore invalidates its parent declarations
        // transitively in one forward pass.
        for mutation in &self.mutations {
            if let SemanticMutation::AddNode {
                token,
                creation:
                    SemanticNodeCreation::Effect {
                        owner: SemanticTransactionNodeRef::Pending(owner),
                        ..
                    },
            } = mutation
            {
                if removed_pending.contains(owner) {
                    removed_pending.insert(*token);
                }
            }
            let SemanticMutation::AddAnimation { token, animation } = mutation else {
                continue;
            };
            if animation.intent().node_references().any(|reference| {
                matches!(reference, SemanticTransactionNodeRef::Pending(dependency) if removed_pending.contains(&dependency))
            }) {
                removed_pending.insert(*token);
            }
        }
        let removed_nodes = store.semantic_removal_closure(&removed_nodes);
        let pending_creations = catalog.cloned_creations();
        let pending_animations = catalog.cloned_animations();
        let surviving_object_creations = pending_creations
            .iter()
            .filter(|(token, creation)| {
                !removed_pending.contains(token)
                    && matches!(
                        creation,
                        SemanticNodeCreation::Object { .. }
                            | SemanticNodeCreation::PendingPathObject { .. }
                    )
            })
            .count();
        let surviving_object_creations = u64::try_from(surviving_object_creations)
            .map_err(|_| SemanticMutationTransactionError::InsertionOrderExhausted)?;
        if store
            .next_insertion_order()
            .checked_add(surviving_object_creations)
            .is_none()
        {
            return Err(SemanticMutationTransactionError::InsertionOrderExhausted);
        }

        let mut targets = HashSet::with_capacity(self.mutations.len());
        let mut style_replacements = HashSet::new();
        let mut style_property_writes = HashSet::new();
        let mut object_transform_writes = HashSet::new();
        let mut object_transform_property_writes = HashSet::new();
        let mut changed = Vec::with_capacity(self.mutations.len());
        let mut family_edges = FamilyEdgePreflight::default();
        let mut pending_sources = HashSet::new();
        let mut staged_objects = HashMap::new();
        let mut staged_effects = HashMap::new();
        let mut effect_names = HashSet::new();
        let mut available_pending_owners = HashSet::new();
        let mut staged_pending_paths: HashMap<SemanticLocalNodeToken, SemanticPendingPathObject> =
            HashMap::new();
        let mut staged_family_z = HashMap::new();
        let mut staged_object_order = Vec::new();
        let mut spatial_anchor_checks = HashMap::new();
        let mut staged_spatial_anchors = HashMap::new();
        let mut staged_updaters =
            HashMap::<SemanticTransactionNodeRef, Vec<SemanticUpdaterRegistration>>::new();
        let mut staged_signal_timeline =
            HashMap::<SemanticNodeId, Vec<SemanticScalarSignalTimelineEntry>>::new();
        let mut staged_signal_scope_additions = Vec::new();
        let mut staged_signal_scope_membership = HashSet::new();
        let mut staged_foreground = HashMap::new();
        let mut staged_table_layouts = HashMap::new();
        let mut available_pending_animations = HashSet::new();

        for (index, mutation) in self.mutations.iter().enumerate() {
            for node in mutation.node_references() {
                catalog.validate_pending(node, index)?;
            }
            if !matches!(
                mutation,
                SemanticMutation::RemoveAnimation { .. } | SemanticMutation::RemoveNode { .. }
            ) {
                if let Some(target) = mutation.target() {
                    if removed_nodes.contains(&target) {
                        return Err(SemanticMutationTransactionError::TargetRemoved {
                            index,
                            target,
                        });
                    }
                }
            }
            if let SemanticMutation::ChangeSubscription {
                object,
                property,
                signal: Some(signal),
            } = mutation
            {
                if removed_nodes.contains(signal) {
                    return Err(match object {
                        SemanticTransactionNodeRef::Existing(object) => {
                            SemanticMutationTransactionError::SubscriptionUsesRemovedSignal {
                                index,
                                object: *object,
                                property: *property,
                                signal: *signal,
                            }
                        }
                        SemanticTransactionNodeRef::Pending(token) => {
                            SemanticMutationTransactionError::PendingSubscriptionUsesRemovedSignal {
                                index,
                                object: *token,
                                property: *property,
                                signal: *signal,
                            }
                        }
                    });
                }
            }
            if let SemanticMutation::AddMember { family, member }
            | SemanticMutation::RemoveMember { family, member } = mutation
            {
                if let SemanticTransactionNodeRef::Existing(member) = member {
                    if removed_nodes.contains(member) {
                        let SemanticTransactionNodeRef::Existing(family) = family else {
                            return Err(SemanticMutationTransactionError::PendingFamilyEdgeUsesRemovedNode {
                            index,
                            family: *family,
                            member: *member,
                        });
                        };
                        return Err(
                            SemanticMutationTransactionError::FamilyEdgeUsesRemovedNode {
                                index,
                                family: *family,
                                member: *member,
                            },
                        );
                    }
                }
            }
            if let SemanticMutation::ReorderMember {
                family,
                member,
                before,
            } = mutation
            {
                if let SemanticTransactionNodeRef::Existing(member) = member {
                    if removed_nodes.contains(member) {
                        let SemanticTransactionNodeRef::Existing(family) = family else {
                            return Err(SemanticMutationTransactionError::PendingFamilyOrderUsesRemovedNode {
                            index,
                            family: *family,
                            node: (*member).into(),
                        });
                        };
                        return Err(
                            SemanticMutationTransactionError::FamilyOrderUsesRemovedNode {
                                index,
                                family: *family,
                                node: *member,
                            },
                        );
                    }
                }
                if let Some(SemanticTransactionNodeRef::Existing(anchor)) = before {
                    if removed_nodes.contains(anchor) {
                        let SemanticTransactionNodeRef::Existing(family) = family else {
                            return Err(SemanticMutationTransactionError::PendingFamilyOrderUsesRemovedNode {
                                index,
                                family: *family,
                                node: (*anchor).into(),
                            });
                        };
                        return Err(
                            SemanticMutationTransactionError::FamilyOrderUsesRemovedNode {
                                index,
                                family: *family,
                                node: *anchor,
                            },
                        );
                    }
                }
            }
            if let SemanticMutation::AddAnimation { animation, .. } = mutation {
                for node in animation.intent().node_references() {
                    if let SemanticTransactionNodeRef::Existing(node) = node {
                        if removed_nodes.contains(&node) {
                            return Err(
                                SemanticMutationTransactionError::AnimationUsesRemovedNode {
                                    index,
                                    node,
                                },
                            );
                        }
                    }
                }
            }

            match mutation {
                SemanticMutation::ReplaceStyle { object, .. } => {
                    if style_property_writes.contains(object) {
                        return Err(conflicting_style_error(index, *object));
                    }
                    style_replacements.insert(*object);
                }
                SemanticMutation::SetObjectTransform { object, .. } => {
                    if object_transform_property_writes.contains(object) {
                        return Err(duplicate_mutation_error(
                            index,
                            SemanticMutationKey::ObjectTransform(*object),
                        ));
                    }
                    object_transform_writes.insert(*object);
                }
                SemanticMutation::SetProperty {
                    object, property, ..
                } if is_transform_property(*property) => {
                    if object_transform_writes.contains(object) {
                        return Err(duplicate_mutation_error(
                            index,
                            SemanticMutationKey::ObjectTransform(*object),
                        ));
                    }
                    object_transform_property_writes.insert(*object);
                }
                SemanticMutation::SetProperty {
                    object, property, ..
                } if is_style_property(*property) => {
                    if style_replacements.contains(object) {
                        return Err(conflicting_style_error(index, *object));
                    }
                    style_property_writes.insert(*object);
                }
                _ => {}
            }

            if let SemanticMutation::UpdateEffect { effect, update } = mutation {
                for parameter in update.parameters() {
                    let key = SemanticMutationKey::EffectParameter {
                        effect: *effect,
                        parameter,
                    };
                    if !targets.insert(key) {
                        return Err(duplicate_mutation_error(index, key));
                    }
                }
            }
            if let Some(key) = mutation.key() {
                let repeated_membership = matches!(
                    key,
                    SemanticMutationKey::FamilyEdge { .. }
                        | SemanticMutationKey::FamilyOrder { .. }
                );
                if !(self.allow_repeated_membership_mutations && repeated_membership)
                    && !targets.insert(key)
                {
                    return Err(duplicate_mutation_error(index, key));
                }
            }

            match mutation {
                SemanticMutation::SetSignal { signal, value } => {
                    let state = store.semantic_signal_state(*signal).map_err(|error| {
                        SemanticMutationTransactionError::Signal { index, error }
                    })?;
                    let SemanticSignalSource::Input(previous) = state.source() else {
                        return Err(SemanticMutationTransactionError::NotInputSignal {
                            index,
                            signal: *signal,
                        });
                    };
                    if state.native_input().is_some() {
                        return Err(SemanticMutationTransactionError::Signal {
                            index,
                            error: SemanticSignalError::NativeOwnedSignal { signal: *signal },
                        });
                    }
                    if !state.scalar_timeline().is_empty()
                        || staged_signal_timeline.contains_key(signal)
                    {
                        return Err(SemanticMutationTransactionError::Signal {
                            index,
                            error: SemanticSignalError::TimelineOwnedSignal { signal: *signal },
                        });
                    }
                    if !value.is_finite() {
                        return Err(SemanticMutationTransactionError::Signal {
                            index,
                            error: SemanticSignalError::NonFiniteValue,
                        });
                    }
                    let expected = state.value_kind();
                    let actual = value.value_kind();
                    if actual != expected {
                        return Err(SemanticMutationTransactionError::SignalTypeMismatch {
                            index,
                            signal: *signal,
                            expected,
                            actual,
                        });
                    }
                    changed.push(previous != value);
                }
                SemanticMutation::SetProperty {
                    object,
                    property,
                    value,
                } => {
                    if *property == SemanticObjectProperty::Presence {
                        return Err(SemanticMutationTransactionError::UnsupportedPropertyWrite {
                            index,
                            object: *object,
                            property: *property,
                        });
                    }
                    if !value.is_finite() {
                        return Err(non_finite_property_error(index, *object, *property));
                    }
                    let expected = property.value_kind();
                    let actual = value.value_kind();
                    if actual != expected {
                        return Err(property_type_error(
                            index, *object, *property, expected, actual,
                        ));
                    }
                    if let SemanticTransactionNodeRef::Pending(token) = object {
                        if let Some(path) = staged_pending_paths.get_mut(token) {
                            let did_change = path.property_value(*property) != *value;
                            if did_change {
                                path.apply_property(*property, value.clone());
                            }
                            changed.push(did_change);
                            continue;
                        }
                    }
                    let state = catalog.staged_object_state(
                        &mut staged_objects,
                        &mut staged_object_order,
                        *object,
                        index,
                    )?;
                    if *property == SemanticObjectProperty::RotationZ
                        && state.transform.planar_rotation().is_none()
                    {
                        return Err(
                            SemanticMutationTransactionError::SpatialOrientationForPlanarRotation {
                                index,
                                object: *object,
                            },
                        );
                    }
                    let did_change = object_property_value(state, *property) != *value;
                    if did_change {
                        apply_object_property(state, *property, value.clone());
                        if !state.spatial_declaration_is_valid() {
                            let error = if state.role() == SemanticObjectRole::PointLight3D {
                                SemanticMutationTransactionError::InvalidPointLightPose {
                                    index,
                                    object: *object,
                                }
                            } else if state.spatial_material() == SemanticSpatialMaterial::PointLit
                            {
                                SemanticMutationTransactionError::InvalidSpatialMaterialPose {
                                    index,
                                    object: *object,
                                }
                            } else {
                                SemanticMutationTransactionError::InvalidCameraPose {
                                    index,
                                    object: *object,
                                }
                            };
                            return Err(error);
                        }
                    }
                    changed.push(did_change);
                }
                SemanticMutation::SetObjectTransform { object, transform } => {
                    if !transform.is_valid() {
                        return Err(SemanticMutationTransactionError::InvalidObjectTransform {
                            index,
                            object: *object,
                        });
                    }
                    let state = catalog.staged_object_state(
                        &mut staged_objects,
                        &mut staged_object_order,
                        *object,
                        index,
                    )?;
                    if state.role() == SemanticObjectRole::Camera3D
                        && !state.camera_transform_is_valid(*transform)
                    {
                        return Err(SemanticMutationTransactionError::InvalidCameraPose {
                            index,
                            object: *object,
                        });
                    }
                    if state.role() == SemanticObjectRole::PointLight3D
                        && transform.scale != SemanticVec3::new(1.0, 1.0, 1.0)
                    {
                        return Err(SemanticMutationTransactionError::InvalidPointLightPose {
                            index,
                            object: *object,
                        });
                    }
                    if state.spatial_material() == SemanticSpatialMaterial::PointLit
                        && [transform.scale.x, transform.scale.y, transform.scale.z]
                            .into_iter()
                            .any(|scale| scale == 0.0)
                    {
                        return Err(
                            SemanticMutationTransactionError::InvalidSpatialMaterialPose {
                                index,
                                object: *object,
                            },
                        );
                    }
                    let did_change =
                        state.transform != *transform || state.camera_profile().is_some();
                    if did_change {
                        state.set_transform(*transform);
                    }
                    changed.push(did_change);
                }
                SemanticMutation::SetClickIndicate { object, binding } => {
                    let state = catalog.staged_object_state(
                        &mut staged_objects,
                        &mut staged_object_order,
                        *object,
                        index,
                    )?;
                    if binding.is_some_and(|binding| {
                        !binding.is_valid()
                            || !state.signal_bindings().is_empty()
                            || !matches!(
                                state.content.geometry(),
                                Some(
                                    StoredGeometry::Circle { .. }
                                        | StoredGeometry::Rectangle { .. }
                                )
                            )
                    }) {
                        return Err(SemanticMutationTransactionError::InvalidClickIndicate {
                            index,
                            object: *object,
                        });
                    }
                    let did_change = state.click_indicate() != *binding;
                    if did_change {
                        state.set_click_indicate(*binding);
                    }
                    changed.push(did_change);
                }
                SemanticMutation::ReplaceContent { object, content } => {
                    let state = catalog.staged_object_state(
                        &mut staged_objects,
                        &mut staged_object_order,
                        *object,
                        index,
                    )?;
                    validate_object_content_resource(store, *content, index)?;
                    validate_spatial_material_resource(
                        store,
                        state.spatial_material(),
                        *content,
                        index,
                    )?;
                    let did_change = state.content != *content;
                    if did_change {
                        state.content = *content;
                    }
                    changed.push(did_change);
                }
                SemanticMutation::SetBarMetadata { object, metadata } => {
                    if metadata
                        .as_ref()
                        .is_some_and(|metadata| !metadata.is_valid())
                    {
                        return Err(SemanticMutationTransactionError::InvalidBarMetadata {
                            index,
                            object: *object,
                        });
                    }
                    let state = catalog.staged_object_state(
                        &mut staged_objects,
                        &mut staged_object_order,
                        *object,
                        index,
                    )?;
                    let did_change = state.bar_metadata() != metadata.as_deref();
                    if did_change {
                        state.set_bar_metadata(metadata.clone());
                    }
                    changed.push(did_change);
                }
                SemanticMutation::SetInset2DView {
                    object,
                    camera_frame,
                    capture_own_display,
                } => {
                    if let Some(camera) = camera_frame {
                        catalog.ensure_object(*camera, index)?;
                        if camera == object {
                            return Err(SemanticMutationTransactionError::InvalidNodeObjectState {
                                index,
                            });
                        }
                    }
                    let state = catalog.staged_object_state(
                        &mut staged_objects,
                        &mut staged_object_order,
                        *object,
                        index,
                    )?;
                    if !matches!(
                        state.role(),
                        SemanticObjectRole::Ordinary | SemanticObjectRole::Inset2DView(_)
                    ) || object.existing().is_some_and(|object| {
                        !store
                            .semantic_graph_owners_for_invariant_target(object)
                            .is_empty()
                    }) {
                        return Err(SemanticMutationTransactionError::InvalidNodeObjectState {
                            index,
                        });
                    }
                    let current = match state.role() {
                        SemanticObjectRole::Inset2DView(view) => {
                            Some((view.camera_frame.into(), view.capture_own_display))
                        }
                        _ => None,
                    };
                    let requested = camera_frame.map(|camera| (camera, *capture_own_display));
                    changed.push(current != requested);
                    // Pending camera identities exist only inside the transaction.
                    // Commit resolves them before publishing the semantic role.
                    if let Some(camera) = camera_frame.and_then(|camera| camera.existing()) {
                        state.set_role(SemanticObjectRole::Inset2DView(
                            crate::SemanticInset2DViewRole::new(camera)
                                .capture_own_display(*capture_own_display),
                        ));
                    } else {
                        state.set_role(SemanticObjectRole::Ordinary);
                    }
                }
                SemanticMutation::SetSpatialCompositionDomain {
                    object,
                    domain,
                    anchor_family,
                } => {
                    if let SemanticTransactionNodeRef::Pending(token) = object {
                        if removed_pending.contains(token) {
                            changed.push(false);
                            continue;
                        }
                    }
                    if let Some(anchor) = anchor_family {
                        if let SemanticTransactionNodeRef::Pending(token) = anchor {
                            if removed_pending.contains(token) {
                                return Err(SemanticMutationTransactionError::UnknownPendingNode {
                                    index,
                                    token: *token,
                                });
                            }
                        }
                        catalog.ensure_authoring_node(*anchor, index)?;
                        catalog.ensure_object(*object, index)?;
                        if anchor != object {
                            catalog.ensure_family(*anchor, index)?;
                            spatial_anchor_checks.insert(*object, (index, *anchor, *object));
                        } else {
                            spatial_anchor_checks.remove(object);
                        }
                    } else {
                        spatial_anchor_checks.remove(object);
                    }
                    match (object, anchor_family) {
                        (
                            SemanticTransactionNodeRef::Existing(_),
                            Some(anchor @ SemanticTransactionNodeRef::Pending(_)),
                        ) => {
                            staged_spatial_anchors.insert(*object, *anchor);
                        }
                        _ => {
                            staged_spatial_anchors.remove(object);
                        }
                    }
                    let state = catalog.staged_object_state(
                        &mut staged_objects,
                        &mut staged_object_order,
                        *object,
                        index,
                    )?;
                    let did_change = state.spatial_composition_domain() != *domain
                        || anchor_family.is_some_and(|anchor| anchor.existing().is_none())
                        || state.spatial_anchor_family()
                            != anchor_family.and_then(|anchor| anchor.existing());
                    if did_change {
                        state
                            .set_spatial_composition_domain_with_anchor(
                                *domain,
                                anchor_family.and_then(|anchor| anchor.existing()),
                            )
                            .map_err(|_| {
                                SemanticMutationTransactionError::InvalidNodeObjectState { index }
                            })?;
                    }
                    changed.push(did_change);
                }
                SemanticMutation::SetCameraMotions { object, motions } => {
                    let state = catalog.staged_object_state(
                        &mut staged_objects,
                        &mut staged_object_order,
                        *object,
                        index,
                    )?;
                    let did_change = state.camera_motions() != motions.as_ref();
                    state
                        .set_camera_motions(std::sync::Arc::clone(motions))
                        .map_err(
                            |_| SemanticMutationTransactionError::InvalidNodeObjectState { index },
                        )?;
                    changed.push(did_change);
                }
                SemanticMutation::SetCameraProfile {
                    object,
                    profile,
                    near,
                    far,
                } => {
                    let state = catalog.staged_object_state(
                        &mut staged_objects,
                        &mut staged_object_order,
                        *object,
                        index,
                    )?;
                    if state.role() != SemanticObjectRole::Camera3D
                        || profile.camera(*near, *far).is_none()
                    {
                        return Err(SemanticMutationTransactionError::InvalidNodeObjectState {
                            index,
                        });
                    }
                    let current = (
                        state.camera_profile(),
                        state.transform,
                        state.camera_projection(),
                    );
                    state
                        .set_camera_profile(*profile, *near, *far)
                        .map_err(
                            |_| SemanticMutationTransactionError::InvalidNodeObjectState { index },
                        )?;
                    let did_change = current
                        != (
                            state.camera_profile(),
                            state.transform,
                            state.camera_projection(),
                        );
                    changed.push(did_change);
                }
                SemanticMutation::ReplaceDecimalNumber { object, number } => {
                    let state = catalog.staged_object_state(
                        &mut staged_objects,
                        &mut staged_object_order,
                        *object,
                        index,
                    )?;
                    if !number.is_valid() {
                        return Err(SemanticMutationTransactionError::InvalidNodeObjectState {
                            index,
                        });
                    }
                    let did_change = state.decimal_number() != Some(number);
                    if did_change {
                        state.set_decimal_number(Some(number.clone()));
                    }
                    changed.push(did_change);
                }
                SemanticMutation::ReplaceTextPresentationBaseline { object, baseline } => {
                    let state = catalog.staged_object_state(
                        &mut staged_objects,
                        &mut staged_object_order,
                        *object,
                        index,
                    )?;
                    let did_change = state.text_presentation_baseline() != *baseline;
                    if did_change {
                        match baseline {
                            Some(baseline) => state.set_text_presentation_baseline(*baseline),
                            None => state.clear_text_presentation_baseline(),
                        }
                    }
                    changed.push(did_change);
                }
                SemanticMutation::SetZIndex { node, value } => {
                    catalog.ensure_authoring_node(*node, index)?;
                    if !value.is_finite() {
                        return Err(SemanticMutationTransactionError::NonFiniteZIndex {
                            index,
                            node: *node,
                        });
                    }
                    if let SemanticTransactionNodeRef::Pending(token) = node {
                        if let Some(path) = staged_pending_paths.get_mut(token) {
                            let did_change = path.z_index != *value;
                            path.z_index = *value;
                            changed.push(did_change);
                            continue;
                        }
                    }
                    if let Some(previous) = catalog.family_z_index(*node) {
                        let previous = staged_family_z.entry(*node).or_insert(previous);
                        changed.push(*previous != *value);
                        *previous = *value;
                    } else {
                        let state = catalog.staged_object_state(
                            &mut staged_objects,
                            &mut staged_object_order,
                            *node,
                            index,
                        )?;
                        changed.push(state.z_index() != *value);
                        state.set_z_index(*value);
                    }
                }
                SemanticMutation::ReplaceStyle { object, style } => {
                    if !style.is_finite() {
                        return Err(invalid_style_error(index, *object));
                    }
                    if let SemanticTransactionNodeRef::Pending(token) = object {
                        if let Some(path) = staged_pending_paths.get_mut(token) {
                            let did_change = path.style != *style;
                            path.style = style.clone();
                            changed.push(did_change);
                            continue;
                        }
                    }
                    let state = catalog.staged_object_state(
                        &mut staged_objects,
                        &mut staged_object_order,
                        *object,
                        index,
                    )?;
                    let did_change = state.style != *style;
                    if did_change {
                        state.style = style.clone();
                    }
                    changed.push(did_change);
                }
                SemanticMutation::ChangeSubscription {
                    object,
                    property,
                    signal,
                } => {
                    let state = catalog.staged_object_state(
                        &mut staged_objects,
                        &mut staged_object_order,
                        *object,
                        index,
                    )?;
                    let existing = state
                        .signal_bindings()
                        .iter()
                        .find(|binding| binding.property() == *property)
                        .map(|binding| binding.signal());

                    if let Some(signal) = signal {
                        let actual =
                            store.semantic_signal_value_kind(*signal).map_err(|error| {
                                SemanticMutationTransactionError::Signal { index, error }
                            })?;
                        let expected = property.value_kind();
                        if actual != expected {
                            return Err(subscription_type_error(
                                index, *object, *property, *signal, expected, actual,
                            ));
                        }
                        let did_change = existing != Some(*signal);
                        if did_change {
                            replace_object_binding(state, *property, Some(*signal));
                        }
                        changed.push(did_change);
                    } else {
                        let did_change = existing.is_some();
                        if did_change {
                            replace_object_binding(state, *property, None);
                        }
                        changed.push(did_change);
                    }
                }
                SemanticMutation::AddUpdater {
                    target,
                    callback,
                    active_from,
                    inactive_from,
                    endpoint_policy,
                    position,
                } => {
                    catalog.ensure_authoring_node(*target, index)?;
                    let registration = SemanticUpdaterRegistration::with_endpoint_policy(
                        *callback,
                        *active_from,
                        *inactive_from,
                        *endpoint_policy,
                    )
                    .map_err(|_| invalid_updater_interval(index, *target))?;
                    let registrations = staged_updaters
                        .entry(*target)
                        .or_insert_with(|| catalog.updater_registrations(*target));
                    insert_updater_registration(registrations, registration, *position)
                        .map_err(|error| updater_edit_error(index, *target, error))?;
                    changed.push(true);
                }
                SemanticMutation::AddScalarSignalTrack {
                    signal,
                    from,
                    to,
                    timing,
                    time_map,
                } => {
                    if targets.contains(&SemanticMutationKey::Signal(*signal)) {
                        return Err(SemanticMutationTransactionError::Signal {
                            index,
                            error: SemanticSignalError::TimelineOwnedSignal { signal: *signal },
                        });
                    }
                    let track = SemanticScalarSignalTrack::new_with_time_map(
                        *signal,
                        *from,
                        *to,
                        *timing,
                        time_map.clone(),
                    );
                    let existing_last = store
                        .semantic_signal_state(*signal)
                        .ok()
                        .and_then(|state| state.scalar_timeline().last());
                    let timeline = staged_signal_timeline.entry(*signal).or_default();
                    let previous = timeline.last().or(existing_last);
                    store
                        .validate_semantic_scalar_signal_track_after(&track, previous)
                        .map_err(|error| SemanticMutationTransactionError::SignalTrack {
                            index,
                            error,
                        })?;
                    timeline.push(SemanticScalarSignalTimelineEntry::Track(track));
                    changed.push(true);
                }
                SemanticMutation::SetScalarSignalAt {
                    signal,
                    value,
                    time,
                } => {
                    if targets.contains(&SemanticMutationKey::Signal(*signal)) {
                        return Err(SemanticMutationTransactionError::Signal {
                            index,
                            error: SemanticSignalError::TimelineOwnedSignal { signal: *signal },
                        });
                    }
                    let hold = SemanticScalarSignalHold::new(*signal, *value, *time);
                    let existing_last = store
                        .semantic_signal_state(*signal)
                        .ok()
                        .and_then(|state| state.scalar_timeline().last());
                    let timeline = staged_signal_timeline.entry(*signal).or_default();
                    let previous = timeline.last().or(existing_last);
                    store
                        .validate_semantic_scalar_signal_entry_after(
                            &SemanticScalarSignalTimelineEntry::Hold(hold),
                            previous,
                        )
                        .map_err(|error| SemanticMutationTransactionError::SignalTrack {
                            index,
                            error,
                        })?;
                    timeline.push(SemanticScalarSignalTimelineEntry::Hold(hold));
                    changed.push(true);
                }
                SemanticMutation::RemoveUpdater {
                    target,
                    callback,
                    inactive_from,
                } => {
                    catalog.ensure_authoring_node(*target, index)?;
                    validate_updater_boundary(index, *target, *inactive_from)?;
                    let registrations = staged_updaters
                        .entry(*target)
                        .or_insert_with(|| catalog.updater_registrations(*target));
                    let did_change =
                        close_first_updater_registration(registrations, *callback, *inactive_from)
                            .map_err(|error| updater_edit_error(index, *target, error))?;
                    changed.push(did_change);
                }
                SemanticMutation::ClearUpdaters {
                    target,
                    inactive_from,
                } => {
                    catalog.ensure_authoring_node(*target, index)?;
                    validate_updater_boundary(index, *target, *inactive_from)?;
                    let registrations = staged_updaters
                        .entry(*target)
                        .or_insert_with(|| catalog.updater_registrations(*target));
                    let did_change = close_all_updater_registrations(registrations, *inactive_from)
                        .map_err(|error| updater_edit_error(index, *target, error))?;
                    changed.push(did_change);
                }
                SemanticMutation::ScopeSignal { scope, signal } => {
                    catalog.ensure_family(*scope, index)?;
                    catalog.ensure_signal(*signal, index)?;
                    if matches!(signal, SemanticTransactionNodeRef::Existing(id) if removed_nodes.contains(id))
                    {
                        return Err(
                            SemanticMutationTransactionError::SignalScopeUsesRemovedNode {
                                index,
                                scope: *scope,
                                signal: *signal,
                            },
                        );
                    }
                    let pair = (*scope, *signal);
                    let already_scoped = staged_signal_scope_membership.contains(&pair)
                        || matches!(
                            pair,
                            (
                                SemanticTransactionNodeRef::Existing(scope),
                                SemanticTransactionNodeRef::Existing(signal)
                            ) if store.is_semantic_signal_scoped(scope, signal)
                        );
                    let did_change = !already_scoped;
                    if did_change {
                        staged_signal_scope_membership.insert(pair);
                        staged_signal_scope_additions.push(pair);
                    }
                    changed.push(did_change);
                }
                SemanticMutation::SetForegroundMembers { scope, members } => {
                    catalog.ensure_family(*scope, index)?;
                    if staged_foreground.contains_key(scope) {
                        return Err(SemanticMutationTransactionError::DuplicateForegroundScope {
                            index,
                            scope: *scope,
                        });
                    }
                    let mut seen = HashSet::with_capacity(members.len());
                    for &member in members {
                        catalog.ensure_authoring_node(member, index)?;
                        if member == *scope || !seen.insert(member) {
                            return Err(
                                SemanticMutationTransactionError::InvalidForegroundMember {
                                    index,
                                    scope: *scope,
                                    member,
                                },
                            );
                        }
                        let removed = match member {
                            SemanticTransactionNodeRef::Existing(id) => removed_nodes.contains(&id),
                            SemanticTransactionNodeRef::Pending(token) => {
                                removed_pending.contains(&token)
                            }
                        };
                        if removed {
                            return Err(
                                SemanticMutationTransactionError::ForegroundUsesRemovedNode {
                                    index,
                                    scope: *scope,
                                    member,
                                },
                            );
                        }
                    }
                    let unchanged = match scope {
                        SemanticTransactionNodeRef::Existing(id) => store
                            .node(*id)
                            .expect("validated family")
                            .foreground_members()
                            .iter()
                            .copied()
                            .map(SemanticTransactionNodeRef::from)
                            .eq(members.iter().copied()),
                        SemanticTransactionNodeRef::Pending(_) => members.is_empty(),
                    };
                    staged_foreground.insert(*scope, members.clone());
                    changed.push(!unchanged);
                }
                SemanticMutation::SetGraphDeclaration { .. } => {
                    changed.push(true);
                }
                SemanticMutation::SetTableLayout { scope, layout } => {
                    catalog.ensure_family(*scope, index)?;
                    if !layout.is_valid() {
                        return Err(SemanticMutationTransactionError::InvalidTableLayout {
                            index,
                            scope: *scope,
                        });
                    }
                    let previous = match staged_table_layouts.get(scope).copied() {
                        Some(previous) => Some(previous),
                        None => match scope {
                            SemanticTransactionNodeRef::Existing(scope) => {
                                store.semantic_table_layout(*scope).map_err(|error| {
                                    SemanticMutationTransactionError::Node { index, error }
                                })?
                            }
                            SemanticTransactionNodeRef::Pending(_) => None,
                        },
                    };
                    changed.push(previous != Some(*layout));
                    staged_table_layouts.insert(*scope, *layout);
                }
                SemanticMutation::AddMember { family, member } => {
                    changed.push(family_edges.add(&catalog, *family, *member, index)?);
                }
                SemanticMutation::RemoveMember { family, member } => {
                    changed.push(family_edges.remove(&catalog, *family, *member, index)?);
                }
                SemanticMutation::ReorderMember {
                    family,
                    member,
                    before,
                } => {
                    changed.push(family_edges.reorder(&catalog, *family, *member, *before, index)?);
                }
                SemanticMutation::UpdateEffect { effect, update } => {
                    let state = store
                        .semantic_effect_state(*effect)
                        .map_err(|error| SemanticMutationTransactionError::Node { index, error })?;
                    let previous = staged_effects
                        .get(effect)
                        .copied()
                        .unwrap_or_else(|| state.definition());
                    let next = previous.update(*update).map_err(|error| {
                        SemanticMutationTransactionError::EffectParameter {
                            index,
                            effect: *effect,
                            error,
                        }
                    })?;
                    changed.push(next != previous);
                    staged_effects.insert(*effect, next);
                }
                SemanticMutation::AddNode { token, creation } => {
                    if let SemanticNodeCreation::Effect { owner, name, .. } = creation {
                        catalog.ensure_object(*owner, index)?;
                        if name.is_empty() {
                            return Err(
                                SemanticMutationTransactionError::InvalidEffectAttachment {
                                    index,
                                    reason: "effect name is empty",
                                },
                            );
                        }
                        match owner {
                            SemanticTransactionNodeRef::Existing(owner) => {
                                if removed_nodes.contains(owner) {
                                    return Err(SemanticMutationTransactionError::NodeCreationUsesRemovedNode { index, node: *owner });
                                }
                                if !removed_pending.contains(token)
                                    && store
                                        .effect_by_name(*owner, name)
                                        .map_err(|error| SemanticMutationTransactionError::Node {
                                            index,
                                            error,
                                        })?
                                        .is_some_and(|effect| !removed_nodes.contains(&effect))
                                {
                                    return Err(
                                        SemanticMutationTransactionError::InvalidEffectAttachment {
                                            index,
                                            reason: "duplicate effect name",
                                        },
                                    );
                                }
                            }
                            SemanticTransactionNodeRef::Pending(owner) => {
                                if !available_pending_owners.contains(owner) {
                                    return Err(
                                        SemanticMutationTransactionError::InvalidEffectAttachment {
                                            index,
                                            reason: "effect owner must precede its attachment",
                                        },
                                    );
                                }
                            }
                        }
                        if !removed_pending.contains(token)
                            && !effect_names.insert((*owner, name.clone()))
                        {
                            return Err(
                                SemanticMutationTransactionError::InvalidEffectAttachment {
                                    index,
                                    reason: "duplicate effect name in transaction",
                                },
                            );
                        }
                    }
                    available_pending_owners.insert(*token);
                    preflight_add_node(
                        store,
                        creation,
                        &removed_nodes,
                        &mut pending_sources,
                        !removed_pending.contains(token),
                        index,
                    )?;
                    match creation {
                        SemanticNodeCreation::Object { state, .. } => {
                            staged_objects
                                .entry((*token).into())
                                .or_insert_with(|| (**state).clone());
                        }
                        SemanticNodeCreation::PendingPathObject { state, .. } => {
                            let resource = state.resource();
                            // A canceled node has no final resource. Its payload
                            // may already have been discarded by scoped materialization
                            // before the transaction is preflighted again.
                            if !removed_pending.contains(token)
                                && (!resource.belongs_to(self.id)
                                    || self.pending_geometry_path(resource).is_none())
                            {
                                return Err(
                                    SemanticMutationTransactionError::UnknownPendingGeometryResource {
                                        index,
                                        resource,
                                    },
                                );
                            }
                            // Keep canceled declarations in this temporary
                            // overlay until earlier ordered writes have been
                            // validated. They are removed from the final
                            // materialization set after the pass.
                            staged_pending_paths
                                .entry(*token)
                                .or_insert_with(|| state.clone());
                        }
                        SemanticNodeCreation::Family { .. }
                        | SemanticNodeCreation::Signal { .. }
                        | SemanticNodeCreation::Effect { .. } => {}
                    }
                    changed.push(!removed_pending.contains(token));
                }
                SemanticMutation::AddAnimation { token, animation } => {
                    preflight_transaction_animation(
                        &catalog,
                        *token,
                        animation,
                        &mut available_pending_animations,
                        &mut staged_objects,
                        &mut staged_object_order,
                        &family_edges,
                        index,
                    )?;
                    changed.push(!removed_pending.contains(token));
                }
                SemanticMutation::RemoveAnimation { .. } => {
                    changed.push(true);
                }
                SemanticMutation::RemoveNode { node } => match node {
                    SemanticTransactionNodeRef::Existing(node) => {
                        if store.node(*node).is_none() {
                            return Err(SemanticMutationTransactionError::Node {
                                index,
                                error: SemanticStoreError::UnknownNode(*node),
                            });
                        }
                        changed.push(true);
                    }
                    SemanticTransactionNodeRef::Pending(_) => changed.push(false),
                },
            }
        }

        for (index, anchor, object) in spatial_anchor_checks.into_values() {
            if !family_edges.contains_ancestor(&catalog, anchor, object) {
                return Err(SemanticMutationTransactionError::InvalidNodeObjectState { index });
            }
        }

        for (mutation, changed) in self.mutations.iter().zip(&mut changed) {
            if mutation.references_any_pending(&removed_pending) {
                *changed = false;
            }
        }
        let mut spatial_anchor_cleared = Vec::new();
        let mut ordered_removed = removed_nodes.iter().copied().collect::<Vec<_>>();
        ordered_removed.sort_unstable();
        for removed in ordered_removed {
            for owner in store.spatial_anchor_owners_for_target(removed) {
                if removed_nodes.contains(&owner) || spatial_anchor_cleared.contains(&owner) {
                    continue;
                }
                let Some(mut state) = store.semantic_object_state_checked(owner).ok().cloned()
                else {
                    continue;
                };
                state
                    .set_spatial_composition_domain_with_anchor(
                        SemanticSpatialCompositionDomain::FixedOrientation,
                        None,
                    )
                    .expect("clearing a valid removed anchor retains a valid domain");
                let node = SemanticTransactionNodeRef::Existing(owner);
                staged_objects.insert(node, state);
                if !staged_object_order.contains(&node) {
                    staged_object_order.push(node);
                }
                spatial_anchor_cleared.push(owner);
            }
        }
        staged_objects.retain(|node, _| {
            !matches!(node, SemanticTransactionNodeRef::Pending(token) if removed_pending.contains(token))
        });
        // Only affected/provisional objects are staged; this remains local to
        // the transaction and runs before any semantic resource publication.
        for (object, state) in &staged_objects {
            if !state.world_path_style_is_supported(store.geometry_resources()) {
                return Err(
                    SemanticMutationTransactionError::UnsupportedWorldPathStyle { object: *object },
                );
            }
            if state.content.image().is_some()
                && (!crate::SemanticImageContent::supports_style(&state.style)
                    || state
                        .signal_bindings()
                        .iter()
                        .any(|binding| binding.property() == SemanticObjectProperty::StrokeWidth))
            {
                return Err(SemanticMutationTransactionError::UnsupportedImageStyle {
                    object: *object,
                });
            }
            if state.click_indicate().is_some_and(|binding| {
                !binding.is_valid()
                    || !state.signal_bindings().is_empty()
                    || !matches!(
                        state.content.geometry(),
                        Some(StoredGeometry::Circle { .. } | StoredGeometry::Rectangle { .. })
                    )
            }) {
                return Err(SemanticMutationTransactionError::InvalidClickIndicate {
                    index: self
                        .mutations
                        .iter()
                        .position(|mutation| {
                            matches!(mutation, SemanticMutation::SetClickIndicate { object: target, .. } if target == object)
                        })
                        .unwrap_or(0),
                    object: *object,
                });
            }
        }
        staged_family_z.retain(|node, _| !matches!(node, SemanticTransactionNodeRef::Pending(token) if removed_pending.contains(token)));
        staged_pending_paths.retain(|token, _| !removed_pending.contains(token));
        let preflight = SemanticTransactionPreflight {
            animation_effect_snapshots: HashMap::new(),
            pending_effect_order: HashMap::new(),
            staged_effects,
            staged_family_z,
            changed,
            staged_objects,
            staged_spatial_anchors,
            staged_pending_paths,
            staged_object_order,
            staged_updaters,
            family_edges,
            staged_signal_scope_additions,
            staged_foreground,
            pending_creations,
            pending_animations,
            removed_existing: removed_nodes,
            removed_pending,
            spatial_anchor_cleared,
        };
        inset_view::validate(self, &preflight, store)?;
        // A graph declaration describes the complete final topology/binding
        // overlay.  Replacing an existing declaration is the only supported
        // way for a transaction to change graph-owned membership; this keeps
        // the invariant explicit while still allowing bounded graph edits.
        let mut staged_graph_scopes = HashSet::new();
        for (index, mutation) in self.mutations.iter().enumerate() {
            if let SemanticMutation::SetGraphDeclaration { scope, graph } = mutation {
                validate_graph_declaration(
                    &preflight,
                    store,
                    &catalog,
                    &mut staged_graph_scopes,
                    *scope,
                    graph,
                    index,
                )?;
            }
        }

        let replaced_graph_scopes = self
            .mutations
            .iter()
            .filter_map(|mutation| match mutation {
                SemanticMutation::SetGraphDeclaration {
                    scope: SemanticTransactionNodeRef::Existing(scope),
                    ..
                } => Some(*scope),
                _ => None,
            })
            .collect::<HashSet<_>>();

        for (index, mutation) in self.mutations.iter().enumerate() {
            if !preflight.changed[index] {
                continue;
            }
            match mutation {
                SemanticMutation::AddMember { family, .. }
                | SemanticMutation::RemoveMember { family, .. } => {
                    let SemanticTransactionNodeRef::Existing(family) = family else {
                        continue;
                    };
                    for scope in store.semantic_graph_owners_for_invariant_target(*family) {
                        if preflight.removed_existing.contains(&scope) {
                            continue;
                        }
                        if replaced_graph_scopes.contains(&scope) {
                            continue;
                        }
                        return Err(SemanticMutationTransactionError::InvalidGraphDeclaration {
                            index,
                            scope: scope.into(),
                            reason: "generic family membership cannot mutate an existing graph root or edge family",
                        });
                    }
                }
                SemanticMutation::ReplaceContent { object, .. } => {
                    let SemanticTransactionNodeRef::Existing(object) = object else {
                        continue;
                    };
                    let final_state = preflight
                        .staged_objects
                        .get(&SemanticTransactionNodeRef::Existing(*object))
                        .or_else(|| store.semantic_object_state_checked(*object).ok());
                    for scope in store.semantic_graph_owners_for_invariant_target(*object) {
                        if preflight.removed_existing.contains(&scope) {
                            continue;
                        }
                        let Some(graph) =
                            store.semantic_graph_declaration(scope).map_err(|error| {
                                SemanticMutationTransactionError::Node { index, error }
                            })?
                        else {
                            continue;
                        };
                        if graph.edge_for_line_node(*object).is_some()
                            && !matches!(
                                final_state.and_then(|state| state.content.geometry()),
                                Some(StoredGeometry::Line { .. })
                            )
                        {
                            return Err(
                                SemanticMutationTransactionError::InvalidGraphDeclaration {
                                    index,
                                    scope: scope.into(),
                                    reason: "designated graph edge dependency must remain an analytic Line",
                                },
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(preflight)
    }
}

fn validate_graph_declaration(
    preflight: &SemanticTransactionPreflight,
    store: &SemanticStore,
    catalog: &TransactionNodeCatalog<'_>,
    staged_scopes: &mut HashSet<SemanticTransactionNodeRef>,
    scope: SemanticTransactionNodeRef,
    graph: &SemanticTransactionGraphDeclaration,
    index: usize,
) -> Result<(), SemanticMutationTransactionError> {
    let invalid = |reason| SemanticMutationTransactionError::InvalidGraphDeclaration {
        index,
        scope,
        reason,
    };
    catalog.ensure_family(scope, index)?;
    if !staged_scopes.insert(scope) {
        return Err(SemanticMutationTransactionError::DuplicateGraphDeclaration { index, scope });
    }
    let removed = |node: SemanticTransactionNodeRef| match node {
        SemanticTransactionNodeRef::Existing(node) => preflight.removed_existing.contains(&node),
        SemanticTransactionNodeRef::Pending(token) => preflight.removed_pending.contains(&token),
    };
    let object_state = |node: SemanticTransactionNodeRef| -> Option<&SemanticObjectState> {
        preflight.staged_objects.get(&node).or_else(|| {
            node.existing()
                .and_then(|id| store.semantic_object_state_checked(id).ok())
        })
    };

    let root_members = preflight
        .family_edges
        .members_for_read(store, scope)
        .into_iter()
        .collect::<HashSet<_>>();

    let mut vertices_by_id = HashMap::with_capacity(graph.vertices().len());
    let mut semantic_objects = HashSet::new();
    for &(vertex_id, vertex) in graph.vertices() {
        if !graph.topology().contains_vertex(vertex_id)
            || vertices_by_id.insert(vertex_id, vertex).is_some()
        {
            return Err(invalid(
                "semantic vertex bindings must name each topology vertex exactly once",
            ));
        }
        catalog.ensure_object(vertex, index)?;
        if removed(vertex) || !semantic_objects.insert(vertex) {
            return Err(invalid("vertices must be distinct live semantic objects"));
        }
        if !root_members.contains(&vertex) {
            return Err(invalid(
                "every graph vertex must be a direct graph-root member",
            ));
        }
    }
    if vertices_by_id.len() != graph.topology().vertices().count()
        || graph
            .topology()
            .vertices()
            .any(|vertex| !vertices_by_id.contains_key(&vertex))
    {
        return Err(invalid(
            "semantic vertex bindings must cover the complete topology",
        ));
    }

    let mut edge_ids = HashSet::with_capacity(graph.edges().len());
    let mut edge_families = HashSet::with_capacity(graph.edges().len());
    for binding in graph.edges().iter().copied() {
        let Some(edge) = graph.topology().edge(binding.id()) else {
            return Err(invalid(
                "semantic edge binding names an unknown topology edge",
            ));
        };
        if !edge_ids.insert(binding.id()) {
            return Err(invalid("semantic edge bindings must be unique"));
        }

        catalog.ensure_family(binding.family(), index)?;
        catalog.ensure_object(binding.line(), index)?;
        if removed(binding.family()) || removed(binding.line()) {
            return Err(invalid("graph declarations cannot reference removed nodes"));
        }
        if binding.family() == scope || !edge_families.insert(binding.family()) {
            return Err(invalid(
                "graph edge families must be distinct from the graph root",
            ));
        }
        if !semantic_objects.insert(binding.line()) {
            return Err(invalid(
                "graph vertex and edge Line identities must be distinct",
            ));
        }
        if !root_members.contains(&binding.family()) {
            return Err(invalid(
                "every graph edge family must be a direct graph-root member",
            ));
        }
        if !preflight
            .family_edges
            .contains(catalog, binding.family(), binding.line())
        {
            return Err(invalid(
                "graph edge Line must be a direct edge-family member",
            ));
        }
        let line_state = object_state(binding.line());
        if !matches!(
            line_state.and_then(|state| state.content.geometry()),
            Some(StoredGeometry::Line { .. })
        ) {
            return Err(invalid(
                "graph edge dependency component must be an analytic Line",
            ));
        }

        let edge_members = preflight
            .family_edges
            .members_for_read(store, binding.family())
            .into_iter()
            .collect::<HashSet<_>>();
        match (edge.directed, binding.dependency()) {
            (false, SemanticTransactionGraphEdgeDependency::Line) => {
                if edge_members.len() != 1 {
                    return Err(invalid(
                        "undirected graph edge family must contain exactly its designated Line",
                    ));
                }
            }
            (
                true,
                SemanticTransactionGraphEdgeDependency::Arrow {
                    end_tip,
                    start_tip,
                    policy,
                },
            ) => {
                if !policy.is_valid() {
                    return Err(invalid(
                        "graph Arrow endpoint policy must be finite and nonnegative",
                    ));
                }
                if !matches!(
                    line_state.map(SemanticObjectState::role),
                    Some(SemanticObjectRole::ArrowShaft(_))
                ) {
                    return Err(invalid(
                        "directed graph edge Line must retain the shared Arrow shaft role",
                    ));
                }

                let mut expected_members = HashSet::with_capacity(3);
                expected_members.insert(binding.line());
                for (tip, expected_role) in [
                    (Some(end_tip), SemanticObjectRole::ArrowEndTip),
                    (start_tip, SemanticObjectRole::ArrowStartTip),
                ] {
                    let Some(tip) = tip else {
                        continue;
                    };
                    catalog.ensure_object(tip, index)?;
                    if removed(tip) || !semantic_objects.insert(tip) {
                        return Err(invalid(
                            "graph Arrow tip identities must be distinct live semantic objects",
                        ));
                    }
                    if !matches!(object_state(tip), Some(state) if state.role() == expected_role) {
                        return Err(invalid(
                            "graph Arrow tip dependency must reference the matching shared Arrow role",
                        ));
                    }
                    if !edge_members.contains(&tip) {
                        return Err(invalid(
                            "graph Arrow tip dependency must be a direct edge-family member",
                        ));
                    }
                    expected_members.insert(tip);
                }
                if edge_members != expected_members {
                    return Err(invalid(
                        "directed graph edge family must contain exactly its shaft and declared tips",
                    ));
                }
            }
            (false, SemanticTransactionGraphEdgeDependency::Arrow { .. }) => {
                return Err(invalid(
                    "undirected graph edges must use Line endpoint dependencies",
                ));
            }
            (true, SemanticTransactionGraphEdgeDependency::Line) => {
                return Err(invalid(
                    "directed graph edges must use shared Arrow endpoint dependencies",
                ));
            }
        }

        if !vertices_by_id.contains_key(&edge.start) || !vertices_by_id.contains_key(&edge.end) {
            return Err(invalid(
                "every graph edge endpoint must resolve through the declared vertex bindings",
            ));
        }
    }
    if edge_ids.len() != graph.topology().edges().count()
        || graph
            .topology()
            .edges()
            .any(|edge| !edge_ids.contains(&edge.id))
    {
        return Err(invalid(
            "semantic edge bindings must cover the complete topology",
        ));
    }

    if root_members.len() != vertices_by_id.len() + edge_families.len()
        || !vertices_by_id
            .values()
            .copied()
            .chain(edge_families.iter().copied())
            .all(|member| root_members.contains(&member))
    {
        return Err(invalid(
            "graph root direct membership must contain exactly its vertices and edge families",
        ));
    }
    Ok(())
}

fn object_property_value(
    state: &SemanticObjectState,
    property: SemanticObjectProperty,
) -> SemanticSignalValue {
    match property {
        SemanticObjectProperty::Presence => {
            unreachable!("presence is currently authored only through a typed signal binding")
        }
        SemanticObjectProperty::Translation => {
            SemanticSignalValue::Vec3(state.transform.translation)
        }
        SemanticObjectProperty::Scale => SemanticSignalValue::Vec3(state.transform.scale),
        SemanticObjectProperty::RotationZ => {
            SemanticSignalValue::Scalar(state.transform.planar_rotation().unwrap_or(f64::NAN))
        }
        SemanticObjectProperty::FillOpacity => {
            SemanticSignalValue::Scalar(state.style.fill_opacity)
        }
        SemanticObjectProperty::StrokeOpacity => {
            SemanticSignalValue::Scalar(state.style.stroke_opacity)
        }
        SemanticObjectProperty::StrokeWidth => {
            SemanticSignalValue::Scalar(state.style.stroke_width)
        }
        SemanticObjectProperty::ObjectOpacity => {
            SemanticSignalValue::Scalar(state.style.object_opacity)
        }
    }
}

fn validate_updater_boundary(
    index: usize,
    target: SemanticTransactionNodeRef,
    time: f64,
) -> Result<(), SemanticMutationTransactionError> {
    if !time.is_finite() || time < 0.0 {
        return Err(invalid_updater_interval(index, target));
    }
    Ok(())
}

fn invalid_updater_interval(
    index: usize,
    target: SemanticTransactionNodeRef,
) -> SemanticMutationTransactionError {
    SemanticMutationTransactionError::InvalidUpdaterActivation { index, target }
}

fn updater_edit_error(
    index: usize,
    target: SemanticTransactionNodeRef,
    error: UpdaterRegistrationEditError,
) -> SemanticMutationTransactionError {
    match error {
        UpdaterRegistrationEditError::InvalidActivationInterval => {
            invalid_updater_interval(index, target)
        }
        UpdaterRegistrationEditError::PositionOutOfBounds { position, active } => {
            SemanticMutationTransactionError::UpdaterPositionOutOfBounds {
                index,
                target,
                position,
                active,
            }
        }
    }
}

fn set_object_property(
    store: &mut SemanticStore,
    object: SemanticNodeId,
    property: SemanticObjectProperty,
    value: SemanticSignalValue,
) {
    let state = store
        .node_mut(object)
        .and_then(|node| node.semantic_object_state_mut())
        .expect("preflighted semantic object must remain valid while transaction owns the store");

    apply_object_property(state, property, value);
}

fn apply_object_property(
    state: &mut SemanticObjectState,
    property: SemanticObjectProperty,
    value: SemanticSignalValue,
) {
    match (property, value) {
        (SemanticObjectProperty::Presence, _) => {
            unreachable!("presence property writes are rejected during transaction preflight")
        }
        (SemanticObjectProperty::Translation, SemanticSignalValue::Vec3(value)) => {
            let mut transform = state.transform;
            transform.translation = value;
            state.set_transform(transform);
        }
        (SemanticObjectProperty::Scale, SemanticSignalValue::Vec3(value)) => {
            let mut transform = state.transform;
            transform.scale = value;
            state.set_transform(transform);
        }
        (SemanticObjectProperty::RotationZ, SemanticSignalValue::Scalar(value)) => {
            let mut transform = state.transform;
            transform.orientation = crate::SemanticOrientation::Planar(value);
            state.set_transform(transform);
        }
        (SemanticObjectProperty::FillOpacity, SemanticSignalValue::Scalar(value)) => {
            state.style.fill_opacity = value;
        }
        (SemanticObjectProperty::StrokeOpacity, SemanticSignalValue::Scalar(value)) => {
            state.style.stroke_opacity = value;
        }
        (SemanticObjectProperty::StrokeWidth, SemanticSignalValue::Scalar(value)) => {
            state.style.stroke_width = value;
        }
        (SemanticObjectProperty::ObjectOpacity, SemanticSignalValue::Scalar(value)) => {
            state.style.object_opacity = value;
        }
        _ => {
            unreachable!("semantic property value kind was validated during transaction preflight")
        }
    }
}

pub(super) fn validate_object_content_resource(
    store: &SemanticStore,
    content: SemanticObjectContent,
    index: usize,
) -> Result<(), SemanticMutationTransactionError> {
    match content {
        SemanticObjectContent::Geometry(geometry) => {
            if !geometry.is_finite() {
                return Err(SemanticMutationTransactionError::InvalidObjectContent { index });
            }
            if let StoredGeometry::Resource(resource) = geometry {
                if store.geometry_resources().get(resource).is_none() {
                    return Err(SemanticMutationTransactionError::InvalidGeometryResource {
                        index,
                        resource,
                    });
                }
            }
        }
        SemanticObjectContent::Image(content) => {
            let resource = content.resource();
            if store.raster_image_resources().get(resource).is_none() {
                return Err(SemanticMutationTransactionError::InvalidImageResource {
                    index,
                    resource,
                });
            }
        }
        SemanticObjectContent::Text(resource) => {
            if store.text_resources().get(resource).is_none() {
                return Err(SemanticMutationTransactionError::InvalidTextResource {
                    index,
                    resource,
                });
            }
        }
    }
    Ok(())
}

fn set_object_style(store: &mut SemanticStore, object: SemanticNodeId, style: SemanticStyle) {
    store
        .node_mut(object)
        .and_then(|node| node.semantic_object_state_mut())
        .expect("preflighted semantic object must remain valid while transaction owns the store")
        .style = style;
}

const fn is_transform_property(property: SemanticObjectProperty) -> bool {
    matches!(
        property,
        SemanticObjectProperty::Translation
            | SemanticObjectProperty::Scale
            | SemanticObjectProperty::RotationZ
    )
}

fn is_style_property(property: SemanticObjectProperty) -> bool {
    matches!(
        property,
        SemanticObjectProperty::FillOpacity
            | SemanticObjectProperty::StrokeOpacity
            | SemanticObjectProperty::StrokeWidth
            | SemanticObjectProperty::ObjectOpacity
    )
}

fn set_object_subscription(
    store: &mut SemanticStore,
    object: SemanticNodeId,
    property: SemanticObjectProperty,
    signal: Option<SemanticNodeId>,
) {
    let authored_order = store.next_authored_updater_order(object);
    store.unregister_semantic_references_for_owner(object);
    let bindings = store
        .node_mut(object)
        .and_then(|node| node.semantic_object_state_mut())
        .expect("preflighted semantic object must remain valid while transaction owns the store")
        .signal_bindings_mut();
    let position = bindings
        .iter()
        .position(|binding| binding.property() == property);

    match (position, signal) {
        (Some(position), Some(signal)) => {
            let order = bindings[position].authored_order();
            let mut binding = SemanticSignalBinding::new(signal, property);
            binding.set_authored_order(order);
            bindings[position] = binding;
        }
        (None, Some(signal)) => {
            let mut binding = SemanticSignalBinding::new(signal, property);
            binding.set_authored_order(authored_order);
            bindings.push(binding);
        }
        (Some(position), None) => {
            bindings.remove(position);
        }
        (None, None) => {
            unreachable!("unchanged missing subscription is filtered during transaction preflight")
        }
    }
    store.register_semantic_references_for_owner(object);
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SemanticMutationTransactionResult {
    impacts: Vec<SemanticMutationImpact>,
    committed_nodes: HashMap<SemanticLocalNodeToken, SemanticNodeId>,
}

impl SemanticMutationTransactionResult {
    pub fn impacts(&self) -> &[SemanticMutationImpact] {
        &self.impacts
    }

    /// Resolve one token from this committed transaction to its real semantic ID.
    /// Canceled or foreign tokens return `None`.
    pub fn resolve(&self, token: SemanticLocalNodeToken) -> Option<SemanticNodeId> {
        self.committed_nodes.get(&token).copied()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticMutationTransactionError {
    DuplicateEffectParameter {
        index: usize,
        effect: SemanticNodeId,
        parameter: crate::GlowParameter,
    },
    InvalidEffectAttachment {
        index: usize,
        reason: &'static str,
    },
    EffectParameter {
        index: usize,
        effect: SemanticNodeId,
        error: crate::GlowParameterError,
    },
    /// A transaction-local vector path contains a non-finite coordinate.
    InvalidPendingGeometryPath,
    /// A callback/resource batch exceeded its bounded pending path working set.
    PendingGeometryLimitExceeded,
    /// Transaction-local resource-token allocation cannot wrap and reuse an
    /// escaped provisional reference.
    LocalResourceTokenExhausted,
    /// A staged content reference does not belong to this transaction or its
    /// payload was not retained through the final materialization scope.
    UnknownPendingGeometryResource {
        index: usize,
        resource: SemanticLocalResourceToken,
    },
    DuplicateGraphDeclaration {
        index: usize,
        scope: SemanticTransactionNodeRef,
    },
    InvalidGraphDeclaration {
        index: usize,
        scope: SemanticTransactionNodeRef,
        reason: &'static str,
    },
    InvalidTableLayout {
        index: usize,
        scope: SemanticTransactionNodeRef,
    },
    DuplicateForegroundScope {
        index: usize,
        scope: SemanticTransactionNodeRef,
    },
    InvalidForegroundMember {
        index: usize,
        scope: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    },
    ForegroundUsesRemovedNode {
        index: usize,
        scope: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    },
    NonFiniteZIndex {
        index: usize,
        node: SemanticTransactionNodeRef,
    },
    DuplicateZIndex {
        index: usize,
        node: SemanticNodeId,
    },
    SceneRevisionExhausted,
    InsertionOrderExhausted,
    PendingNodeFromDifferentTransaction {
        index: usize,
        token: SemanticLocalNodeToken,
    },
    UnknownPendingNode {
        index: usize,
        token: SemanticLocalNodeToken,
    },
    PendingNodeKindMismatch {
        index: usize,
        token: SemanticLocalNodeToken,
        expected: SemanticPendingNodeKind,
    },
    PendingAnimationForwardReference {
        index: usize,
        animation: SemanticLocalNodeToken,
    },
    DuplicatePendingMutation {
        index: usize,
        node: SemanticLocalNodeToken,
    },
    ConflictingPendingStyleMutation {
        index: usize,
        object: SemanticLocalNodeToken,
    },
    PendingFamilyCycle {
        index: usize,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    },
    PendingNotFamilyMember {
        index: usize,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    },
    PendingSubscriptionUsesRemovedSignal {
        index: usize,
        object: SemanticLocalNodeToken,
        property: SemanticObjectProperty,
        signal: SemanticNodeId,
    },
    PendingFamilyEdgeUsesRemovedNode {
        index: usize,
        family: SemanticTransactionNodeRef,
        member: SemanticNodeId,
    },
    PendingFamilyOrderUsesRemovedNode {
        index: usize,
        family: SemanticTransactionNodeRef,
        node: SemanticTransactionNodeRef,
    },
    PendingNonFinitePropertyValue {
        index: usize,
        object: SemanticLocalNodeToken,
        property: SemanticObjectProperty,
    },
    PendingPropertyTypeMismatch {
        index: usize,
        object: SemanticLocalNodeToken,
        property: SemanticObjectProperty,
        expected: SemanticSignalValueKind,
        actual: SemanticSignalValueKind,
    },
    UnsupportedPropertyWrite {
        index: usize,
        object: SemanticTransactionNodeRef,
        property: SemanticObjectProperty,
    },
    InvalidPendingStyle {
        index: usize,
        object: SemanticLocalNodeToken,
    },
    PendingSubscriptionTypeMismatch {
        index: usize,
        object: SemanticLocalNodeToken,
        property: SemanticObjectProperty,
        signal: SemanticNodeId,
        expected: SemanticSignalValueKind,
        actual: SemanticSignalValueKind,
    },
    SamePendingAnimationTargetAndTargetState {
        index: usize,
        node: SemanticTransactionNodeRef,
    },
    DuplicateTarget {
        index: usize,
        target: SemanticNodeId,
    },
    DuplicateProperty {
        index: usize,
        object: SemanticNodeId,
        property: SemanticObjectProperty,
    },
    DuplicateContent {
        index: usize,
        object: SemanticNodeId,
    },
    DuplicateClickIndicate {
        index: usize,
        object: SemanticNodeId,
    },
    DuplicateBarMetadata {
        index: usize,
        object: SemanticNodeId,
    },
    DuplicateStyle {
        index: usize,
        object: SemanticNodeId,
    },
    ConflictingStyleMutation {
        index: usize,
        object: SemanticNodeId,
    },
    DuplicateSubscription {
        index: usize,
        object: SemanticNodeId,
        property: SemanticObjectProperty,
    },
    DuplicateFamilyEdge {
        index: usize,
        family: SemanticNodeId,
        member: SemanticNodeId,
    },
    DuplicateFamilyOrder {
        index: usize,
        family: SemanticNodeId,
        member: SemanticNodeId,
    },
    DuplicateNodeRemoval {
        index: usize,
        node: SemanticNodeId,
    },
    MutationAfterRemove {
        index: usize,
    },
    TargetRemoved {
        index: usize,
        target: SemanticNodeId,
    },
    SubscriptionUsesRemovedSignal {
        index: usize,
        object: SemanticNodeId,
        property: SemanticObjectProperty,
        signal: SemanticNodeId,
    },
    FamilyEdgeUsesRemovedNode {
        index: usize,
        family: SemanticNodeId,
        member: SemanticNodeId,
    },
    FamilyOrderUsesRemovedNode {
        index: usize,
        family: SemanticNodeId,
        node: SemanticNodeId,
    },
    NodeCreationUsesRemovedNode {
        index: usize,
        node: SemanticNodeId,
    },
    SignalScopeUsesRemovedNode {
        index: usize,
        scope: SemanticTransactionNodeRef,
        signal: SemanticTransactionNodeRef,
    },
    AnimationUsesRemovedNode {
        index: usize,
        node: SemanticNodeId,
    },
    InvalidNodeObjectState {
        index: usize,
    },
    InvalidObjectContent {
        index: usize,
    },
    NodeCreationBindingTypeMismatch {
        index: usize,
        signal: SemanticNodeId,
        expected: SemanticSignalValueKind,
        actual: SemanticSignalValueKind,
    },
    Signal {
        index: usize,
        error: SemanticSignalError,
    },
    SignalTrack {
        index: usize,
        error: SemanticScalarSignalTrackError,
    },
    NotInputSignal {
        index: usize,
        signal: SemanticNodeId,
    },
    SignalTypeMismatch {
        index: usize,
        signal: SemanticNodeId,
        expected: SemanticSignalValueKind,
        actual: SemanticSignalValueKind,
    },
    Object {
        index: usize,
        error: SemanticSceneOperationError,
    },
    Family {
        index: usize,
        error: SemanticSceneOperationError,
    },
    AnimationTarget {
        index: usize,
        error: SemanticSceneOperationError,
    },
    UnknownAnimation {
        index: usize,
        animation: SemanticNodeId,
    },
    NotAnimation {
        index: usize,
        animation: SemanticNodeId,
    },
    EmptyAnimationComposition {
        index: usize,
    },
    SameAnimationTargetAndTargetState {
        index: usize,
        node: SemanticNodeId,
    },
    InvalidAnimationRunTime {
        index: usize,
    },
    InvalidAnimationAngle {
        index: usize,
    },
    InvalidIndicateEndpoint {
        index: usize,
    },
    InvalidDrawBorderThenFillOutline {
        index: usize,
    },
    InvalidPassingFlash {
        index: usize,
    },
    InvalidSubsetDisplayMember {
        index: usize,
    },
    InvalidTextWriteTarget {
        index: usize,
    },
    InvalidFadeEndpoint {
        index: usize,
    },
    InvalidAffineLifecycleEndpoint {
        index: usize,
    },
    InvalidAnimationLagRatio {
        index: usize,
    },
    InvalidAnimationPathArc {
        index: usize,
    },
    InvalidObjectPropertyTrack {
        index: usize,
    },
    NonFinitePropertyValue {
        index: usize,
        object: SemanticNodeId,
        property: SemanticObjectProperty,
    },
    SpatialOrientationForPlanarRotation {
        index: usize,
        object: SemanticTransactionNodeRef,
    },
    InvalidObjectTransform {
        index: usize,
        object: SemanticTransactionNodeRef,
    },
    InvalidCameraPose {
        index: usize,
        object: SemanticTransactionNodeRef,
    },
    InvalidPointLightPose {
        index: usize,
        object: SemanticTransactionNodeRef,
    },
    InvalidSpatialMaterialPose {
        index: usize,
        object: SemanticTransactionNodeRef,
    },
    InvalidSpatialMaterialResource {
        index: usize,
    },
    InvalidStyle {
        index: usize,
        object: SemanticNodeId,
    },
    InvalidBarMetadata {
        index: usize,
        object: SemanticTransactionNodeRef,
    },
    InvalidClickIndicate {
        index: usize,
        object: SemanticTransactionNodeRef,
    },
    InvalidGeometryResource {
        index: usize,
        resource: crate::GeometryResourceHandle,
    },
    UnsupportedImageStyle {
        object: SemanticTransactionNodeRef,
    },
    UnsupportedWorldPathStyle {
        object: SemanticTransactionNodeRef,
    },
    UnsupportedImageAnimation {
        index: usize,
    },
    InvalidImageResource {
        index: usize,
        resource: crate::RasterImageResourceHandle,
    },
    InvalidTextResource {
        index: usize,
        resource: crate::TextResourceHandle,
    },
    InvalidUpdaterActivation {
        index: usize,
        target: SemanticTransactionNodeRef,
    },
    UpdaterPositionOutOfBounds {
        index: usize,
        target: SemanticTransactionNodeRef,
        position: usize,
        active: usize,
    },
    PropertyTypeMismatch {
        index: usize,
        object: SemanticNodeId,
        property: SemanticObjectProperty,
        expected: SemanticSignalValueKind,
        actual: SemanticSignalValueKind,
    },
    SubscriptionTypeMismatch {
        index: usize,
        object: SemanticNodeId,
        property: SemanticObjectProperty,
        signal: SemanticNodeId,
        expected: SemanticSignalValueKind,
        actual: SemanticSignalValueKind,
    },
    Node {
        index: usize,
        error: SemanticStoreError,
    },
}

impl std::fmt::Display for SemanticMutationTransactionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateEffectParameter { index, effect, parameter } => write!(formatter, "mutation {index}: duplicate write to effect {effect:?} parameter {parameter:?}"),
            Self::InvalidEffectAttachment { index, reason } => write!(formatter, "mutation {index}: {reason}"),
            Self::EffectParameter { index, effect, error } => write!(formatter, "mutation {index}: effect {effect:?}: {error}"),
            Self::InvalidPendingGeometryPath => {
                formatter.write_str("pending geometry path contains non-finite coordinates")
            }
            Self::PendingGeometryLimitExceeded => {
                formatter.write_str("pending geometry working set exceeds the transaction limit")
            }
            Self::LocalResourceTokenExhausted => {
                formatter.write_str("transaction-local resource tokens exhausted")
            }
            Self::UnknownPendingGeometryResource { index, resource } => write!(
                formatter,
                "semantic transaction mutation {index} names unknown pending geometry resource {resource:?}"
            ),
            Self::InvalidTableLayout { index, scope } => write!(
                formatter,
                "semantic transaction mutation {index} has invalid Table layout for {scope:?}"
            ),
            Self::DuplicateGraphDeclaration { index, scope } => write!(
                formatter,
                "semantic transaction mutation {index} repeats or replaces Graph declarations for {scope:?}"
            ),
            Self::InvalidGraphDeclaration {
                index,
                scope,
                reason,
            } => write!(
                formatter,
                "semantic transaction mutation {index} has invalid Graph declaration for {scope:?}: {reason}"
            ),
            Self::DuplicateForegroundScope { index, scope } => write!(
                formatter,
                "semantic transaction mutation {index} repeats foreground declarations for {scope:?}"
            ),
            Self::InvalidForegroundMember {
                index,
                scope,
                member,
            } => write!(
                formatter,
                "semantic transaction mutation {index} has duplicate or self foreground reference {member:?} under {scope:?}"
            ),
            Self::ForegroundUsesRemovedNode {
                index,
                scope,
                member,
            } => write!(
                formatter,
                "semantic transaction mutation {index} declares removed foreground member {member:?} under {scope:?}"
            ),
            Self::SceneRevisionExhausted => {
                write!(formatter, "Noon scene revision space exhausted")
            }
            Self::InsertionOrderExhausted => {
                write!(formatter, "Noon semantic insertion-order space exhausted")
            }
            Self::SignalTrack { index, error } => write!(
                formatter,
                "semantic transaction mutation {index} has invalid signal track: {error}"
            ),
            Self::PendingNodeFromDifferentTransaction { index, token } => write!(
                formatter,
                "semantic transaction mutation {index} uses pending node {token:?} from another transaction"
            ),
            Self::UnknownPendingNode { index, token } => write!(
                formatter,
                "semantic transaction mutation {index} uses unknown pending node {token:?}"
            ),
            Self::PendingNodeKindMismatch {
                index,
                token,
                expected,
            } => write!(
                formatter,
                "semantic transaction mutation {index} requires pending node {token:?} to be {expected:?}"
            ),
            Self::PendingAnimationForwardReference { index, animation } => write!(
                formatter,
                "semantic transaction mutation {index} references pending animation {animation:?} before its declaration"
            ),
            Self::DuplicatePendingMutation { index, node } => write!(
                formatter,
                "semantic transaction mutation {index} repeats a mutation target involving pending node {node:?}"
            ),
            Self::ConflictingPendingStyleMutation { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} mixes full and scalar style mutation on pending object {object:?}"
            ),
            Self::PendingFamilyCycle {
                index,
                family,
                member,
            } => write!(
                formatter,
                "semantic transaction mutation {index} creates a family cycle {family:?} -> {member:?}"
            ),
            Self::PendingNotFamilyMember {
                index,
                family,
                member,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot reorder non-member {member:?} in family {family:?}"
            ),
            Self::PendingSubscriptionUsesRemovedSignal {
                index,
                object,
                property,
                signal,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot bind removed signal {signal:?} to {property:?} on pending object {object:?}"
            ),
            Self::PendingFamilyEdgeUsesRemovedNode {
                index,
                family,
                member,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot use removed node {member:?} in pending family edge for {family:?}"
            ),
            Self::PendingFamilyOrderUsesRemovedNode {
                index,
                family,
                node,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot use removed node {node:?} in pending family order for {family:?}"
            ),
            Self::PendingNonFinitePropertyValue {
                index,
                object,
                property,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot set {property:?} on pending object {object:?} to a non-finite value"
            ),
            Self::PendingPropertyTypeMismatch {
                index,
                object,
                property,
                expected,
                actual,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot set {property:?} on pending object {object:?} requiring {expected} to {actual}"
            ),
            Self::UnsupportedPropertyWrite {
                index,
                object,
                property,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot directly set {property:?} on {object:?}; this property currently requires a typed signal binding"
            ),
            Self::InvalidPendingStyle { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} cannot set a non-finite style on pending object {object:?}"
            ),
            Self::PendingSubscriptionTypeMismatch {
                index,
                object,
                property,
                signal,
                expected,
                actual,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot bind {actual} signal {signal:?} to {property:?} on pending object {object:?} requiring {expected}"
            ),
            Self::SamePendingAnimationTargetAndTargetState { index, node } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add an animation with identical pending target and target state {node:?}"
            ),
            Self::DuplicateTarget { index, target } => write!(
                formatter,
                "semantic transaction mutation {index} repeats target {}:{}",
                target.slot(),
                target.generation()
            ),
            Self::DuplicateProperty {
                index,
                object,
                property,
            } => write!(
                formatter,
                "semantic transaction mutation {index} repeats property {:?} on object {}:{}",
                property,
                object.slot(),
                object.generation()
            ),
            Self::DuplicateContent { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} repeats content replacement on object {}:{}",
                object.slot(),
                object.generation()
            ),
            Self::DuplicateClickIndicate { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} repeats click Indicate on object {}:{}",
                object.slot(),
                object.generation()
            ),
            Self::DuplicateBarMetadata { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} repeats BarChart metadata replacement on object {}:{}",
                object.slot(),
                object.generation()
            ),
            Self::NonFiniteZIndex { index, node } => write!(
                formatter,
                "mutation {index} has non-finite z-index for {node:?}"
            ),
            Self::DuplicateZIndex { index, node } => write!(
                formatter,
                "mutation {index} duplicates z-index for {node:?}"
            ),
            Self::DuplicateStyle { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} repeats style replacement on object {}:{}",
                object.slot(),
                object.generation()
            ),
            Self::ConflictingStyleMutation { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} mixes full-style replacement with scalar style mutation on object {}:{}",
                object.slot(),
                object.generation()
            ),
            Self::DuplicateSubscription {
                index,
                object,
                property,
            } => write!(
                formatter,
                "semantic transaction mutation {index} repeats subscription for property {:?} on object {}:{}",
                property,
                object.slot(),
                object.generation()
            ),
            Self::DuplicateFamilyEdge {
                index,
                family,
                member,
            } => write!(
                formatter,
                "semantic transaction mutation {index} repeats family edge {}:{} -> {}:{}",
                family.slot(),
                family.generation(),
                member.slot(),
                member.generation()
            ),
            Self::DuplicateFamilyOrder {
                index,
                family,
                member,
            } => write!(
                formatter,
                "semantic transaction mutation {index} repeats family reorder for member {}:{} in family {}:{}",
                member.slot(),
                member.generation(),
                family.slot(),
                family.generation()
            ),
            Self::DuplicateNodeRemoval { index, node } => write!(
                formatter,
                "semantic transaction mutation {index} repeats removal of node {}:{}",
                node.slot(),
                node.generation()
            ),
            Self::MutationAfterRemove { index } => write!(
                formatter,
                "semantic transaction mutation {index} follows structural removal; structural removals must be terminal"
            ),
            Self::TargetRemoved { index, target } => write!(
                formatter,
                "semantic transaction mutation {index} targets node {}:{} that is removed by the same transaction",
                target.slot(),
                target.generation()
            ),
            Self::SubscriptionUsesRemovedSignal {
                index,
                object,
                property,
                signal,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot bind signal {}:{} scheduled for removal to property {:?} on object {}:{}",
                signal.slot(),
                signal.generation(),
                property,
                object.slot(),
                object.generation()
            ),
            Self::FamilyEdgeUsesRemovedNode {
                index,
                family,
                member,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot change family edge {}:{} -> {}:{} because the member is removed by the same transaction",
                family.slot(),
                family.generation(),
                member.slot(),
                member.generation()
            ),
            Self::FamilyOrderUsesRemovedNode {
                index,
                family,
                node,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot reorder family {}:{} using node {}:{} because that node is removed by the same transaction",
                family.slot(),
                family.generation(),
                node.slot(),
                node.generation()
            ),
            Self::NodeCreationUsesRemovedNode { index, node } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add a node referencing semantic node {}:{} because that node is removed by the same transaction",
                node.slot(),
                node.generation()
            ),
            Self::SignalScopeUsesRemovedNode {
                index,
                scope,
                signal,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot scope removed signal {signal:?} under {scope:?}"
            ),
            Self::AnimationUsesRemovedNode { index, node } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add an animation referencing node {}:{} because that node is removed by the same transaction",
                node.slot(),
                node.generation()
            ),
            Self::InvalidObjectContent { index } => write!(
                formatter,
                "semantic transaction mutation {index}: object geometry contains non-finite values"
            ),
            Self::InvalidNodeObjectState { index } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add an object with non-finite authored transform/style values"
            ),
            Self::InvalidUpdaterActivation { index, target } => write!(
                formatter,
                "semantic transaction mutation {index} has an invalid updater activation interval for {target:?}"
            ),
            Self::UpdaterPositionOutOfBounds {
                index,
                target,
                position,
                active,
            } => write!(
                formatter,
                "semantic transaction mutation {index} inserts updater at position {position} on {target:?}, but only {active} registrations are active"
            ),
            Self::NodeCreationBindingTypeMismatch {
                index,
                signal,
                expected,
                actual,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add an object binding {actual} signal {}:{} to a property requiring {expected}",
                signal.slot(),
                signal.generation()
            ),
            Self::Signal { index, error } => {
                write!(formatter, "semantic transaction mutation {index}: {error}")
            }
            Self::NotInputSignal { index, signal } => write!(
                formatter,
                "semantic transaction mutation {index} cannot SetSignal on derived signal {}:{}",
                signal.slot(),
                signal.generation()
            ),
            Self::SignalTypeMismatch {
                index,
                signal,
                expected,
                actual,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot set signal {}:{} of kind {expected} to {actual}",
                signal.slot(),
                signal.generation()
            ),
            Self::Object { index, error } | Self::Family { index, error } => {
                write!(formatter, "semantic transaction mutation {index}: {error}")
            }
            Self::AnimationTarget { index, error } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add animation: {error}"
            ),
            Self::UnknownAnimation { index, animation } => write!(
                formatter,
                "semantic transaction mutation {index}: unknown semantic animation {}:{}",
                animation.slot(),
                animation.generation()
            ),
            Self::NotAnimation { index, animation } => write!(
                formatter,
                "semantic transaction mutation {index}: semantic node {}:{} is not an animation",
                animation.slot(),
                animation.generation()
            ),
            Self::EmptyAnimationComposition { index } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add an empty animation composition"
            ),
            Self::SameAnimationTargetAndTargetState { index, node } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add animation with identical target and target-state node {}:{}",
                node.slot(),
                node.generation()
            ),
            Self::InvalidAnimationRunTime { index } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add animation with non-finite or non-positive run_time"
            ),
            Self::InvalidAnimationAngle { index } => write!(
                formatter,
                "semantic mutation {index} has a non-finite rotation angle"
            ),
            Self::InvalidIndicateEndpoint { index } => write!(
                formatter,
                "semantic mutation {index} has invalid Indicate scale, color, or center"
            ),
            Self::InvalidDrawBorderThenFillOutline { index } => write!(
                formatter,
                "semantic mutation {index} has an invalid DrawBorderThenFill outline"
            ),
            Self::InvalidPassingFlash { index } => write!(
                formatter,
                "semantic mutation {index} has an invalid PassingFlash target or width"
            ),
            Self::InvalidSubsetDisplayMember { index } => write!(
                formatter,
                "semantic mutation {index} has an invalid subset display member index/count"
            ),
            Self::InvalidTextWriteTarget { index } => write!(
                formatter,
                "semantic mutation {index} requires one retained text object"
            ),
            Self::InvalidFadeEndpoint { index } => write!(
                formatter,
                "semantic mutation {index} has an invalid Fade affine endpoint"
            ),
            Self::InvalidAffineLifecycleEndpoint { index } => write!(
                formatter,
                "semantic mutation {index} has an invalid affine lifecycle endpoint"
            ),
            Self::InvalidAnimationLagRatio { index } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add animation with non-finite or negative lag_ratio"
            ),
            Self::InvalidAnimationPathArc { index } => write!(
                formatter,
                "semantic transaction mutation {index} cannot add animation with non-finite path_arc"
            ),
            Self::InvalidObjectPropertyTrack { index } => write!(
                formatter,
                "semantic transaction mutation {index} has an invalid exact object property track"
            ),
            Self::NonFinitePropertyValue {
                index,
                object,
                property,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot set property {:?} on object {}:{} to a non-finite value",
                property,
                object.slot(),
                object.generation()
            ),
            Self::SpatialOrientationForPlanarRotation { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} cannot apply planar RotationZ to spatially oriented object {object:?}"
            ),
            Self::InvalidObjectTransform { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} cannot assign invalid transform to object {object:?}"
            ),
            Self::InvalidCameraPose { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} cannot assign a non-unit-scale or invalid pose to Camera3D object {object:?}"
            ),
            Self::InvalidPointLightPose { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} cannot assign a non-unit-scale or invalid pose to PointLight3D object {object:?}"
            ),
            Self::InvalidSpatialMaterialPose { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} cannot assign a singular pose to PointLit object {object:?}"
            ),
            Self::InvalidSpatialMaterialResource { index } => write!(
                formatter,
                "semantic transaction mutation {index} requires a CairoSurface mesh with retained appearance metadata"
            ),
            Self::InvalidStyle { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} cannot replace style on object {}:{} with non-finite authored values",
                object.slot(),
                object.generation()
            ),
            Self::InvalidBarMetadata { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} cannot assign invalid BarChart metadata to object {object:?}"
            ),
            Self::InvalidClickIndicate { index, object } => write!(
                formatter,
                "semantic transaction mutation {index} cannot assign click Indicate to unsupported or invalid object {object:?}"
            ),
            Self::InvalidGeometryResource { index, resource } => write!(
                formatter,
                "semantic transaction mutation {index} references unavailable geometry resource {:?}",
                resource
            ),
            Self::UnsupportedImageStyle { object } => write!(
                formatter,
                "image {object:?} supports alpha and affine edits, not recoloring, strokes or patterns",
            ),
            Self::UnsupportedWorldPathStyle { object } => write!(
                formatter,
                "World path {object:?} requires opaque or disabled fill/stroke paint; partial transparency is unsupported",
            ),
            Self::UnsupportedImageAnimation { index } => write!(
                formatter,
                "semantic transaction mutation {index} cannot apply vector/color animation to an image",
            ),
            Self::InvalidImageResource { index, resource } => write!(
                formatter,
                "semantic transaction mutation {index} references unavailable raster image resource {:?}",
                resource
            ),
            Self::InvalidTextResource { index, resource } => write!(
                formatter,
                "semantic transaction mutation {index} references unavailable text resource {:?}",
                resource
            ),
            Self::PropertyTypeMismatch {
                index,
                object,
                property,
                expected,
                actual,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot set property {:?} on object {}:{} of kind {expected} to {actual}",
                property,
                object.slot(),
                object.generation()
            ),
            Self::SubscriptionTypeMismatch {
                index,
                object,
                property,
                signal,
                expected,
                actual,
            } => write!(
                formatter,
                "semantic transaction mutation {index} cannot bind {actual} signal {}:{} to property {:?} on object {}:{} requiring {expected}",
                signal.slot(),
                signal.generation(),
                property,
                object.slot(),
                object.generation()
            ),
            Self::Node { index, error } => {
                write!(formatter, "semantic transaction mutation {index}: {error}")
            }
        }
    }
}

impl std::error::Error for SemanticMutationTransactionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Signal { error, .. } => Some(error),
            Self::SignalTrack { error, .. } => Some(error),
            Self::Object { error, .. }
            | Self::Family { error, .. }
            | Self::AnimationTarget { error, .. } => Some(error),
            Self::Node { error, .. } => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod base_tests;

#[cfg(test)]
mod content_tests;

#[cfg(test)]
mod bar_metadata_tests;

#[cfg(test)]
mod style_tests;

#[cfg(test)]
mod subscription_tests;

#[cfg(test)]
mod family_edge_tests;

#[cfg(test)]
mod reorder_member_tests;

#[cfg(test)]
mod add_node_tests;

#[cfg(test)]
mod add_animation_tests;

#[cfg(test)]
mod remove_animation_tests;

#[cfg(test)]
mod remove_node_tests;

#[cfg(test)]
mod prepared_tests;

#[cfg(test)]
mod provisional_tests;

#[cfg(test)]
mod signal_scope_tests;

#[cfg(test)]
mod z_index_tests;

#[cfg(test)]
mod foreground_tests;

#[cfg(test)]
mod graph_tests;

#[cfg(test)]
mod click_indicate_tests;
