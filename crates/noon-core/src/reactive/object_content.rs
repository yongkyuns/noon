use crate::{GeometryRef, ObjectDefinition, ObjectId, Style, TextResourceHandle, Transform2D};
use crate::{
    SemanticNodeId, SemanticPresentation, SemanticSignalValueKind, SemanticStyle,
    SemanticTransform2_5D, StoredGeometry,
};

/// Target authored content carried by one semantic object.
///
/// Cheap analytic geometry stays inline through [`StoredGeometry`]. Heavy geometry
/// is represented only by the existing generation/version-safe resource handle,
/// and text is represented by its existing immutable resource handle. This type
/// deliberately contains no semantic node identity, legacy `ObjectId`, execution
/// slot, frontend identity, or renderer identity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SemanticObjectContent {
    Geometry(StoredGeometry),
    Text(TextResourceHandle),
}

impl SemanticObjectContent {
    pub const fn geometry(self) -> Option<StoredGeometry> {
        match self {
            Self::Geometry(geometry) => Some(geometry),
            Self::Text(_) => None,
        }
    }

    pub const fn text(self) -> Option<TextResourceHandle> {
        match self {
            Self::Geometry(_) => None,
            Self::Text(handle) => Some(handle),
        }
    }
}

impl From<StoredGeometry> for SemanticObjectContent {
    fn from(value: StoredGeometry) -> Self {
        Self::Geometry(value)
    }
}

impl From<TextResourceHandle> for SemanticObjectContent {
    fn from(value: TextResourceHandle) -> Self {
        Self::Text(value)
    }
}

/// Scene-level role carried by an ordinary semantic object.
///
/// Roles describe how shared semantic scene state interprets an object; they do
/// not create a second renderer/runtime object model. In particular, a 2D camera
/// remains an ordinary semantic frame object whose effective execution transform
/// determines the renderer-facing [`crate::Camera2DState`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SemanticObjectRole {
    #[default]
    Ordinary,
    Camera2D,
}

/// Stable authored object properties that may be driven by native-reactive signals.
///
/// These names describe semantic state, not execution slots or legacy timeline
/// properties. `z_index` is intentionally absent until integer signal/conversion
/// semantics are defined; content/paint replacement also remains a separate
/// mutation class rather than being forced into the scalar/vector signal model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SemanticObjectProperty {
    Presence,
    Translation,
    Scale,
    RotationZ,
    FillOpacity,
    StrokeOpacity,
    StrokeWidth,
    ObjectOpacity,
}

impl SemanticObjectProperty {
    pub const fn value_kind(self) -> SemanticSignalValueKind {
        match self {
            Self::Presence => SemanticSignalValueKind::Bool,
            Self::Translation | Self::Scale => SemanticSignalValueKind::Vec3,
            Self::RotationZ
            | Self::FillOpacity
            | Self::StrokeOpacity
            | Self::StrokeWidth
            | Self::ObjectOpacity => SemanticSignalValueKind::Scalar,
        }
    }
}

/// One authored native-reactive binding from a semantic signal to an object property.
///
/// The target object owns this declaration. Signal identity uses the same
/// scene-global generational [`SemanticNodeId`] as every semantic entity; lowering
/// may derive execution slots later but those are not authored identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemanticSignalBinding {
    signal: SemanticNodeId,
    property: SemanticObjectProperty,
}

impl SemanticSignalBinding {
    pub(crate) const fn new(signal: SemanticNodeId, property: SemanticObjectProperty) -> Self {
        Self { signal, property }
    }

    pub const fn signal(self) -> SemanticNodeId {
        self.signal
    }

    pub const fn property(self) -> SemanticObjectProperty {
        self.property
    }
}

/// One target authored presentation payload for a semantic object.
///
/// Identity and family/lifecycle relationships belong to `SemanticNode`; immutable
/// heavy content belongs to the existing resource arenas. This value owns the
/// mutable authored content reference, high-precision transform, semantic style,
/// painter metadata, scene role, and typed native-reactive property bindings that
/// frontends must share.
///
/// Bounds are intentionally absent. Layout-accurate and conservative bounds are
/// derived from content + transform + style (and may be cached as disposable
/// derived data), so they cannot drift into a second mutable authored truth.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticObjectState {
    pub content: SemanticObjectContent,
    pub transform: SemanticTransform2_5D,
    pub style: SemanticStyle,
    presentation: SemanticPresentation,
    role: SemanticObjectRole,
    signal_bindings: Vec<SemanticSignalBinding>,
}

impl SemanticObjectState {
    pub fn new(content: impl Into<SemanticObjectContent>) -> Self {
        Self {
            content: content.into(),
            transform: SemanticTransform2_5D::default(),
            style: SemanticStyle::default(),
            presentation: SemanticPresentation::default(),
            role: SemanticObjectRole::default(),
            signal_bindings: Vec::new(),
        }
    }

    pub const fn presentation(&self) -> SemanticPresentation {
        self.presentation
    }

    pub const fn z_index(&self) -> i32 {
        self.presentation.z_index
    }

    pub fn set_z_index(&mut self, z_index: i32) {
        self.presentation.z_index = z_index;
    }

    pub const fn insertion_order(&self) -> u64 {
        self.presentation.insertion_order
    }

    pub const fn role(&self) -> SemanticObjectRole {
        self.role
    }

    pub fn set_role(&mut self, role: SemanticObjectRole) {
        self.role = role;
    }

    pub fn signal_bindings(&self) -> &[SemanticSignalBinding] {
        &self.signal_bindings
    }

    pub(crate) fn signal_bindings_mut(&mut self) -> &mut Vec<SemanticSignalBinding> {
        &mut self.signal_bindings
    }

    /// Assign the stable painter-order tie break at semantic-store insertion.
    ///
    /// Frontends may author `z_index`, but insertion order belongs to the scene
    /// authority so independent wrappers cannot manufacture conflicting order.
    pub(crate) fn assign_insertion_order(&mut self, insertion_order: u64) {
        self.presentation.insertion_order = insertion_order;
    }
}

/// Renderer-independent lowered content referenced by one compiled object slot.
///
/// The execution plan and runtime carry this same payload so geometry and text
/// share identity, ordering, timeline evaluation, and incremental invalidation.
/// Heavy text stays behind its immutable resource handle.
#[derive(Clone, Debug, PartialEq)]
pub enum ObjectContentRef {
    Geometry(GeometryRef),
    Text(TextResourceHandle),
}

impl ObjectContentRef {
    pub fn geometry(&self) -> Option<&GeometryRef> {
        match self {
            Self::Geometry(geometry) => Some(geometry),
            Self::Text(_) => None,
        }
    }

    pub const fn text(&self) -> Option<TextResourceHandle> {
        match self {
            Self::Geometry(_) => None,
            Self::Text(handle) => Some(*handle),
        }
    }
}

impl From<GeometryRef> for ObjectContentRef {
    fn from(value: GeometryRef) -> Self {
        Self::Geometry(value)
    }
}

impl From<TextResourceHandle> for ObjectContentRef {
    fn from(value: TextResourceHandle) -> Self {
        Self::Text(value)
    }
}

/// Migration-only compiler input retained while pre-A1 callers still require
/// legacy `ObjectId`, `Transform2D`, and `Style` values. #959/A4 owns deletion of
/// this type; it is not a second target semantic object model.
#[derive(Clone, Debug, PartialEq)]
pub struct RetainedObjectDefinition {
    pub id: ObjectId,
    pub content: ObjectContentRef,
    pub transform: Transform2D,
    pub style: Style,
}

impl RetainedObjectDefinition {
    pub fn new(id: ObjectId, content: impl Into<ObjectContentRef>) -> Self {
        Self {
            id,
            content: content.into(),
            transform: Transform2D::default(),
            style: Style::default(),
        }
    }

    pub fn geometry(id: ObjectId, geometry: GeometryRef) -> Self {
        Self::new(id, geometry)
    }

    pub fn text(id: ObjectId, text: TextResourceHandle) -> Self {
        Self::new(id, text)
    }
}

impl From<&ObjectDefinition> for RetainedObjectDefinition {
    fn from(value: &ObjectDefinition) -> Self {
        Self {
            id: value.id,
            content: ObjectContentRef::Geometry(value.geometry.clone()),
            transform: value.transform,
            style: value.style,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GeometryResourceArena, SemanticVec3, TextResourceId, Vec2, VectorPath};

    #[test]
    fn semantic_object_state_uses_shared_high_precision_authoring_values() {
        let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 2.0 });
        state.transform.translation = SemanticVec3::new(0.7, -0.3, 4.5);
        state.transform.scale = SemanticVec3::new(1.25, 0.5, 2.0);
        state.style.object_opacity = 0.4;
        state.set_z_index(7);

        assert_eq!(
            state.content.geometry(),
            Some(StoredGeometry::Circle { radius: 2.0 })
        );
        assert_eq!(state.transform.translation.z, 4.5);
        assert_eq!(state.transform.scale.z, 2.0);
        assert_eq!(state.style.object_opacity, 0.4);
        assert_eq!(state.z_index(), 7);
        assert_eq!(state.insertion_order(), 0);
        assert_eq!(state.role(), SemanticObjectRole::Ordinary);
        assert!(state.signal_bindings().is_empty());
    }

    #[test]
    fn semantic_object_role_is_explicit_and_defaults_to_ordinary() {
        let mut state = SemanticObjectState::new(StoredGeometry::Rectangle {
            size: Vec2::new(14.0, 8.0),
        });
        assert_eq!(state.role(), SemanticObjectRole::Ordinary);
        state.set_role(SemanticObjectRole::Camera2D);
        assert_eq!(state.role(), SemanticObjectRole::Camera2D);
    }

    #[test]
    fn semantic_binding_properties_have_explicit_signal_value_kinds() {
        for property in [
            SemanticObjectProperty::Translation,
            SemanticObjectProperty::Scale,
        ] {
            assert_eq!(property.value_kind(), SemanticSignalValueKind::Vec3);
        }
        for property in [
            SemanticObjectProperty::RotationZ,
            SemanticObjectProperty::FillOpacity,
            SemanticObjectProperty::StrokeOpacity,
            SemanticObjectProperty::StrokeWidth,
            SemanticObjectProperty::ObjectOpacity,
        ] {
            assert_eq!(property.value_kind(), SemanticSignalValueKind::Scalar);
        }
        assert_eq!(
            SemanticObjectProperty::Presence.value_kind(),
            SemanticSignalValueKind::Bool
        );
    }

    #[test]
    fn semantic_content_keeps_heavy_geometry_handle_backed() {
        let mut arena = GeometryResourceArena::new();
        let path = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(1.0, 2.0));
        let handle = arena.insert_path(path);
        let state = SemanticObjectState::new(StoredGeometry::Resource(handle));

        assert_eq!(
            state.content.geometry(),
            Some(StoredGeometry::Resource(handle))
        );
        assert_eq!(arena.len(), 1);
        assert!(arena.get(handle).is_some());
    }

    #[test]
    fn semantic_text_content_uses_existing_versioned_resource_identity() {
        let handle = TextResourceHandle {
            arena: 0,
            id: TextResourceId::new(11),
            version: 4,
        };
        let state = SemanticObjectState::new(handle);

        assert_eq!(state.content.text(), Some(handle));
        assert_eq!(state.content.geometry(), None);
    }

    #[test]
    fn transform_and_style_edits_do_not_change_content_identity() {
        let mut arena = GeometryResourceArena::new();
        let handle = arena.insert_path(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(2.0, 0.0)),
        );
        let mut state = SemanticObjectState::new(StoredGeometry::Resource(handle));
        let before = state.content;

        state.transform.translation = SemanticVec3::new(1000.25, -2000.5, 3.0);
        state.style.stroke_width = 12.5;
        state.style.object_opacity = 0.25;
        state.set_z_index(-3);

        assert_eq!(state.content, before);
        assert_eq!(
            state.content.geometry(),
            Some(StoredGeometry::Resource(handle))
        );
        assert_eq!(arena.len(), 1);
    }

    #[test]
    fn legacy_geometry_converts_without_changing_semantics() {
        let mut legacy = ObjectDefinition::new(ObjectId::new(7), GeometryRef::circle(2.0));
        legacy.transform.translation.x = 3.0;
        legacy.style.opacity = 0.4;

        let retained = RetainedObjectDefinition::from(&legacy);
        assert_eq!(retained.id, legacy.id);
        assert_eq!(retained.content.geometry(), Some(&legacy.geometry));
        assert_eq!(retained.transform, legacy.transform);
        assert_eq!(retained.style, legacy.style);
        assert_eq!(retained.content.text(), None);
    }

    #[test]
    fn legacy_text_object_keeps_only_the_versioned_resource_handle() {
        let handle = TextResourceHandle {
            arena: 0,
            id: TextResourceId::new(11),
            version: 4,
        };
        let retained = RetainedObjectDefinition::text(ObjectId::new(3), handle);
        assert_eq!(retained.content.text(), Some(handle));
        assert_eq!(retained.content.geometry(), None);
    }
}
