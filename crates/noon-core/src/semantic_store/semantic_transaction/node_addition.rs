use std::collections::HashSet;

use crate::{
    GeometryResource, GeometryResourceHandle, SemanticNodeId, SemanticObjectContent,
    SemanticObjectProperty, SemanticObjectRole, SemanticObjectState, SemanticSignalError,
    SemanticSignalState, SemanticSignalValue, SemanticSpatialMaterial, SemanticStore,
    SemanticStoreError, SemanticStyle, SemanticTransform2_5D, SourceIdentity, StoredGeometry,
};

use super::{
    validate_object_content_resource, SemanticLocalResourceToken, SemanticMutationTransactionError,
};

/// The transaction-only declaration for a path whose immutable payload has not
/// entered a resource arena yet.
///
/// This intentionally carries only the constructor fields shared with a normal
/// object. It is never a durable `SemanticObjectState`: final preparation admits
/// the retained path and replaces this declaration with an ordinary object before
/// semantic storage or runtime lowering can observe it.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticPendingPathObject {
    resource: SemanticLocalResourceToken,
    pub(crate) transform: SemanticTransform2_5D,
    pub(crate) style: SemanticStyle,
    pub(crate) z_index: f64,
    pub(crate) role: SemanticObjectRole,
}

impl SemanticPendingPathObject {
    pub(crate) const fn new(
        resource: SemanticLocalResourceToken,
        transform: SemanticTransform2_5D,
        style: SemanticStyle,
        z_index: f64,
        role: SemanticObjectRole,
    ) -> Self {
        Self {
            resource,
            transform,
            style,
            z_index,
            role,
        }
    }

    pub(crate) const fn resource(&self) -> SemanticLocalResourceToken {
        self.resource
    }

    pub(crate) fn materialize(self, resource: GeometryResourceHandle) -> SemanticObjectState {
        let mut state = SemanticObjectState::new(StoredGeometry::Resource(resource));
        state.transform = self.transform.into();
        state.style = self.style;
        state.set_z_index(self.z_index);
        state.set_role(self.role);
        state
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.transform.translation.is_finite()
            && self.transform.scale.is_finite()
            && self.transform.rotation_z.is_finite()
            && self.style.is_finite()
            && self.z_index.is_finite()
            && self.role.is_valid()
    }

    pub(crate) fn property_value(&self, property: SemanticObjectProperty) -> SemanticSignalValue {
        match property {
            SemanticObjectProperty::Translation => self.transform.translation.into(),
            SemanticObjectProperty::Scale => self.transform.scale.into(),
            SemanticObjectProperty::RotationZ => self.transform.rotation_z.into(),
            SemanticObjectProperty::FillOpacity => self.style.fill_opacity.into(),
            SemanticObjectProperty::StrokeOpacity => self.style.stroke_opacity.into(),
            SemanticObjectProperty::StrokeWidth => self.style.stroke_width.into(),
            SemanticObjectProperty::ObjectOpacity => self.style.object_opacity.into(),
            SemanticObjectProperty::Presence => unreachable!("presence is not an object value"),
        }
    }

    pub(crate) fn apply_property(
        &mut self,
        property: SemanticObjectProperty,
        value: SemanticSignalValue,
    ) {
        match (property, value) {
            (SemanticObjectProperty::Translation, SemanticSignalValue::Vec3(value)) => {
                self.transform.translation = value;
            }
            (SemanticObjectProperty::Scale, SemanticSignalValue::Vec3(value)) => {
                self.transform.scale = value;
            }
            (SemanticObjectProperty::RotationZ, SemanticSignalValue::Scalar(value)) => {
                self.transform.rotation_z = value;
            }
            (SemanticObjectProperty::FillOpacity, SemanticSignalValue::Scalar(value)) => {
                self.style.fill_opacity = value;
            }
            (SemanticObjectProperty::StrokeOpacity, SemanticSignalValue::Scalar(value)) => {
                self.style.stroke_opacity = value;
            }
            (SemanticObjectProperty::StrokeWidth, SemanticSignalValue::Scalar(value)) => {
                self.style.stroke_width = value;
            }
            (SemanticObjectProperty::ObjectOpacity, SemanticSignalValue::Scalar(value)) => {
                self.style.object_opacity = value;
            }
            _ => unreachable!("path property value kind was preflighted"),
        }
    }
}

/// Authored payload for allocating one new scene node through the semantic
/// mutation transaction.
///
/// Nodes created here start detached. Animation declarations keep their dedicated
/// `AddAnimation` mutation. Input signals share this creation vocabulary so a
/// signal and its root scope can publish in one transaction.
#[derive(Clone, Debug, PartialEq)]
pub enum SemanticNodeCreation {
    Object {
        state: Box<SemanticObjectState>,
        source_identity: Option<SourceIdentity>,
    },
    /// A transaction-only retained-path constructor. Its resource token has no
    /// durable handle and cannot be admitted through ordinary node APIs.
    PendingPathObject {
        state: SemanticPendingPathObject,
        source_identity: Option<SourceIdentity>,
    },
    Family {
        source_identity: Option<SourceIdentity>,
    },
    Signal {
        state: SemanticSignalState,
    },
}

impl SemanticNodeCreation {
    pub fn object(state: SemanticObjectState) -> Self {
        Self::Object {
            state: Box::new(state),
            source_identity: None,
        }
    }

    /// Build one transaction-local retained-path object. The resource token must
    /// have been issued by the same transaction through `stage_geometry_path`.
    pub fn pending_path_object(
        resource: SemanticLocalResourceToken,
        transform: SemanticTransform2_5D,
        style: SemanticStyle,
        z_index: f64,
        role: SemanticObjectRole,
    ) -> Self {
        Self::PendingPathObject {
            state: SemanticPendingPathObject::new(resource, transform, style, z_index, role),
            source_identity: None,
        }
    }

    pub const fn family() -> Self {
        Self::Family {
            source_identity: None,
        }
    }

    pub fn input_signal(
        value: impl Into<SemanticSignalValue>,
    ) -> Result<Self, SemanticSignalError> {
        Ok(Self::Signal {
            state: SemanticSignalState::input(value.into())?,
        })
    }

    pub fn native_input_signal(
        value: impl Into<SemanticSignalValue>,
        source: crate::SemanticNativeInputSource,
    ) -> Result<Self, SemanticSignalError> {
        Ok(Self::Signal {
            state: SemanticSignalState::input_with_native_source(value.into(), source)?,
        })
    }

    pub fn with_source_identity(mut self, source_identity: SourceIdentity) -> Self {
        match &mut self {
            Self::Object {
                source_identity: source,
                ..
            }
            | Self::PendingPathObject {
                source_identity: source,
                ..
            }
            | Self::Family {
                source_identity: source,
            } => *source = Some(source_identity),
            Self::Signal { .. } => {}
        }
        self
    }

    pub const fn source_identity(&self) -> Option<&SourceIdentity> {
        match self {
            Self::Object {
                source_identity, ..
            }
            | Self::PendingPathObject {
                source_identity, ..
            }
            | Self::Family { source_identity } => source_identity.as_ref(),
            Self::Signal { .. } => None,
        }
    }
}

pub(super) fn preflight_add_node(
    store: &SemanticStore,
    creation: &SemanticNodeCreation,
    removed_nodes: &HashSet<SemanticNodeId>,
    pending_sources: &mut HashSet<SourceIdentity>,
    validate_source_identity: bool,
    index: usize,
) -> Result<(), SemanticMutationTransactionError> {
    if let Some(source) = creation
        .source_identity()
        .filter(|_| validate_source_identity)
    {
        if let Some(existing) = store.node_for_source(source) {
            if !removed_nodes.contains(&existing) {
                return Err(SemanticMutationTransactionError::Node {
                    index,
                    error: SemanticStoreError::DuplicateSourceIdentity(source.clone()),
                });
            }
        }
        if !pending_sources.insert(source.clone()) {
            return Err(SemanticMutationTransactionError::Node {
                index,
                error: SemanticStoreError::DuplicateSourceIdentity(source.clone()),
            });
        }
    }

    match creation {
        SemanticNodeCreation::PendingPathObject { state, .. } => {
            // Retained-path construction is intentionally the ordinary-object
            // slice. Roles with extra content/binding invariants are admitted
            // only through their existing fully materialized constructors.
            if !state.is_valid() || !matches!(state.role, SemanticObjectRole::Ordinary) {
                return Err(SemanticMutationTransactionError::InvalidNodeObjectState { index });
            }
            return Ok(());
        }
        SemanticNodeCreation::Object { state, .. } => {
            if !state.transform.translation.is_finite()
                || !state.transform.scale.is_finite()
                || !state.transform.is_valid()
                || !state.style.is_finite()
                || !state.z_index().is_finite()
                || !state.role().is_valid()
                || !state.spatial_declaration_is_valid()
                || state
                    .decimal_number()
                    .is_some_and(|number| !number.is_valid())
            {
                return Err(SemanticMutationTransactionError::InvalidNodeObjectState { index });
            }
            validate_object_content_resource(store, state.content, index)?;
            validate_cairo_surface_resource(store, state.spatial_material(), state.content, index)?;

            if let SemanticObjectRole::Inset2DView(view) = state.role() {
                if !matches!(
                    state.content.geometry(),
                    Some(StoredGeometry::Rectangle { .. })
                ) || removed_nodes.contains(&view.camera_frame)
                    || !store
                        .semantic_object_state_checked(view.camera_frame)
                        .ok()
                        .is_some_and(|frame| {
                            matches!(
                                frame.content.geometry(),
                                Some(StoredGeometry::Rectangle { .. })
                            )
                        })
                {
                    return Err(SemanticMutationTransactionError::InvalidNodeObjectState { index });
                }
            }

            for binding in state.signal_bindings() {
                let signal = binding.signal();
                if removed_nodes.contains(&signal) {
                    return Err(
                        SemanticMutationTransactionError::NodeCreationUsesRemovedNode {
                            index,
                            node: signal,
                        },
                    );
                }
                let actual = store
                    .semantic_signal_value_kind(signal)
                    .map_err(|error| SemanticMutationTransactionError::Signal { index, error })?;
                let expected = binding.property().value_kind();
                if actual != expected {
                    return Err(
                        SemanticMutationTransactionError::NodeCreationBindingTypeMismatch {
                            index,
                            signal,
                            expected,
                            actual,
                        },
                    );
                }
            }
        }
        SemanticNodeCreation::Family { .. } | SemanticNodeCreation::Signal { .. } => {}
    }

    Ok(())
}

pub(super) fn validate_cairo_surface_resource(
    store: &SemanticStore,
    material: SemanticSpatialMaterial,
    content: SemanticObjectContent,
    index: usize,
) -> Result<(), SemanticMutationTransactionError> {
    if material != SemanticSpatialMaterial::CairoSurface {
        return Ok(());
    }
    let Some(StoredGeometry::Resource(handle)) = content.geometry() else {
        return Err(SemanticMutationTransactionError::InvalidSpatialMaterialResource { index });
    };
    let has_appearance = matches!(
        store.geometry_resources().get(handle),
        Some(GeometryResource::Mesh(mesh)) if mesh.is_single_face() && mesh.cairo_appearance().is_some()
    );
    if has_appearance {
        Ok(())
    } else {
        Err(SemanticMutationTransactionError::InvalidSpatialMaterialResource { index })
    }
}

pub(super) fn commit_add_node(
    store: &mut SemanticStore,
    creation: SemanticNodeCreation,
) -> (SemanticNodeId, Option<SourceIdentity>) {
    match creation {
        SemanticNodeCreation::Object {
            state,
            source_identity,
        } => (store.insert_semantic_object(*state), source_identity),
        SemanticNodeCreation::PendingPathObject { .. } => {
            unreachable!("prepared path declarations materialize before semantic commit")
        }
        SemanticNodeCreation::Family { source_identity } => {
            (store.insert_family(), source_identity)
        }
        SemanticNodeCreation::Signal { state } => (store.insert_semantic_signal_state(state), None),
    }
}
