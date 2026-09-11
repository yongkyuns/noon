use crate::{GeometryRef, TextResourceHandle};
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
            Self::Geometry(content) => Some(content),
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

/// Immutable authored policy needed by one Arrow shaft dependency.
///
/// The visible shaft width may be capped by current Arrow length, so retaining
/// only `SemanticStyle::stroke_width` loses the constructor value required by a
/// later Manim-compatible resize. This payload remains Semantic Scene state; the
/// runtime and renderer continue to consume only ordinary geometry/style values.
#[derive(Clone, Copy, Debug)]
pub struct SemanticArrowShaftRole {
    initial_stroke_width: f64,
    max_stroke_width_to_length_ratio: f64,
}

impl SemanticArrowShaftRole {
    pub const fn new(initial_stroke_width: f64, max_stroke_width_to_length_ratio: f64) -> Self {
        Self {
            initial_stroke_width,
            max_stroke_width_to_length_ratio,
        }
    }

    pub const fn initial_stroke_width(self) -> f64 {
        self.initial_stroke_width
    }

    pub const fn max_stroke_width_to_length_ratio(self) -> f64 {
        self.max_stroke_width_to_length_ratio
    }

    pub fn is_valid(self) -> bool {
        self.initial_stroke_width.is_finite()
            && self.initial_stroke_width >= 0.0
            && self.max_stroke_width_to_length_ratio.is_finite()
            && self.max_stroke_width_to_length_ratio >= 0.0
    }

    fn canonical_bits(value: f64) -> u64 {
        if value == 0.0 {
            0
        } else {
            value.to_bits()
        }
    }
}

impl PartialEq for SemanticArrowShaftRole {
    fn eq(&self, other: &Self) -> bool {
        Self::canonical_bits(self.initial_stroke_width)
            == Self::canonical_bits(other.initial_stroke_width)
            && Self::canonical_bits(self.max_stroke_width_to_length_ratio)
                == Self::canonical_bits(other.max_stroke_width_to_length_ratio)
    }
}

impl Eq for SemanticArrowShaftRole {}

impl std::hash::Hash for SemanticArrowShaftRole {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        Self::canonical_bits(self.initial_stroke_width).hash(state);
        Self::canonical_bits(self.max_stroke_width_to_length_ratio).hash(state);
    }
}

/// Scene-level role carried by an ordinary semantic object.
///
/// Roles describe how shared semantic scene state interprets an object; they do
/// not create a second renderer/runtime object model. In particular, a 2D camera
/// remains an ordinary semantic frame object whose effective execution transform
/// determines the renderer-facing [`crate::Camera2DState`]. Arrow component roles
/// retain only the dependency information required to keep shaft/tip mutations
/// coherent; rendering still sees ordinary Line/path leaves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SemanticObjectRole {
    #[default]
    Ordinary,
    Camera2D,
    ArrowShaft(SemanticArrowShaftRole),
    ArrowEndTip,
    ArrowStartTip,
}

impl SemanticObjectRole {
    pub fn is_valid(self) -> bool {
        match self {
            Self::ArrowShaft(policy) => policy.is_valid(),
            Self::Ordinary | Self::Camera2D | Self::ArrowEndTip | Self::ArrowStartTip => true,
        }
    }
}

/// Stable authored object properties that may be driven by native-reactive signals.
///
/// These names describe semantic state, not execution slots or legacy timeline
/// properties. Painter priority uses the discrete semantic ordering transaction;
/// content/paint replacement also remains a separate
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

    pub const fn z_index(&self) -> f64 {
        self.presentation.z_index
    }

    pub fn set_z_index(&mut self, z_index: impl Into<f64>) {
        self.presentation.z_index = z_index.into();
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
        assert_eq!(state.z_index(), 7.0);
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
    fn arrow_role_retains_uncapped_stroke_policy_without_affecting_style() {
        let mut state = SemanticObjectState::new(StoredGeometry::Line {
            start: Vec2::ZERO,
            end: Vec2::new(0.2, 0.0),
        });
        state.style.stroke_width = 0.01;
        let policy = SemanticArrowShaftRole::new(0.06, 0.05);
        state.set_role(SemanticObjectRole::ArrowShaft(policy));

        assert_eq!(state.style.stroke_width, 0.01);
        assert_eq!(state.role(), SemanticObjectRole::ArrowShaft(policy));
        assert_eq!(policy.initial_stroke_width(), 0.06);
        assert_eq!(policy.max_stroke_width_to_length_ratio(), 0.05);
        assert!(state.role().is_valid());

        let invalid = SemanticArrowShaftRole::new(f64::NAN, 0.05);
        assert_eq!(invalid, invalid);
        assert!(!invalid.is_valid());
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
    fn text_content_keeps_only_the_versioned_resource_handle() {
        let handle = TextResourceHandle {
            arena: 0,
            id: TextResourceId::new(11),
            version: 4,
        };
        let retained = ObjectContentRef::Text(handle);
        assert_eq!(retained.text(), Some(handle));
        assert_eq!(retained.geometry(), None);
    }
}
