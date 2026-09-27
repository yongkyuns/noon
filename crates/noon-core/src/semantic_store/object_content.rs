use crate::{GeometryRef, SemanticImageContent, TextResourceHandle};
use crate::{
    SemanticNodeId, SemanticPresentation, SemanticSignalValueKind, SemanticStyle,
    SemanticTransform2_5D, StoredGeometry,
};
use std::sync::Arc;

/// Receiver-owned baseline for Manim-compatible text presentation queries.
///
/// Content replacement deliberately preserves this declaration: current ink
/// bounds may change, while the receiver's initial font size and height remain
/// the scale reference.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextPresentationBaseline {
    pub initial_font_size: f64,
    pub initial_height: f64,
}

impl TextPresentationBaseline {
    pub fn new(initial_font_size: f64, initial_height: f64) -> Option<Self> {
        (initial_font_size.is_finite()
            && initial_font_size > 0.0
            && initial_height.is_finite()
            && initial_height >= 0.0)
            .then_some(Self {
                initial_font_size,
                initial_height,
            })
    }
}

mod coordinate_role;
pub use coordinate_role::SemanticNumberLineRole;
mod function_plot_role;
pub use function_plot_role::SemanticFunctionPlotRole;
mod decimal_number;
pub use decimal_number::{SemanticDecimalNumber, SemanticNumericTextBinding};

/// Authored numeric input and constructor paint for an ordinary chart rectangle.
///
/// This is optional object metadata rather than a [`SemanticObjectRole`] variant:
/// bar values cannot be recovered from geometry after user transforms/styles, but
/// they must not enlarge the role carried by every ordinary semantic object.
#[derive(Clone, Copy, Debug)]
pub struct SemanticBarMetadata {
    pub value: f64,
    pub original_color: crate::Color,
    pub width: f64,
    pub fill_opacity: f64,
    pub stroke_width: f64,
}

impl PartialEq for SemanticBarMetadata {
    fn eq(&self, other: &Self) -> bool {
        self.bits() == other.bits()
    }
}
impl Eq for SemanticBarMetadata {}
impl std::hash::Hash for SemanticBarMetadata {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.bits().hash(state);
    }
}
impl SemanticBarMetadata {
    fn bits(&self) -> [u64; 8] {
        [
            self.value,
            self.width,
            self.fill_opacity,
            self.stroke_width,
            f64::from(self.original_color.red),
            f64::from(self.original_color.green),
            f64::from(self.original_color.blue),
            f64::from(self.original_color.alpha),
        ]
        .map(|v| if v == 0.0 { 0 } else { v.to_bits() })
    }
    pub fn is_valid(&self) -> bool {
        self.value.is_finite()
            && self.width.is_finite()
            && self.width > 0.0
            && self.fill_opacity.is_finite()
            && (0.0..=1.0).contains(&self.fill_opacity)
            && self.stroke_width.is_finite()
            && self.stroke_width >= 0.0
            && [
                self.original_color.red,
                self.original_color.green,
                self.original_color.blue,
                self.original_color.alpha,
            ]
            .into_iter()
            .all(f32::is_finite)
    }
}

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
    Image(SemanticImageContent),
}

impl SemanticObjectContent {
    pub const fn geometry(self) -> Option<StoredGeometry> {
        match self {
            Self::Geometry(content) => Some(content),
            Self::Text(_) | Self::Image(_) => None,
        }
    }

    pub const fn image(self) -> Option<SemanticImageContent> {
        match self {
            Self::Image(content) => Some(content),
            Self::Geometry(_) | Self::Text(_) => None,
        }
    }

    pub const fn text(self) -> Option<TextResourceHandle> {
        match self {
            Self::Geometry(_) | Self::Image(_) => None,
            Self::Text(handle) => Some(handle),
        }
    }
}

impl From<SemanticImageContent> for SemanticObjectContent {
    fn from(value: SemanticImageContent) -> Self {
        Self::Image(value)
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

/// Authoritative pairing for one retained 2D inset view.
///
/// The display remains an ordinary semantic rectangle. This role only identifies
/// the ordinary frame whose effective runtime transform supplies the inset camera
/// and whether the display may recursively capture itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemanticInset2DViewRole {
    pub camera_frame: SemanticNodeId,
    pub capture_own_display: bool,
}

impl SemanticInset2DViewRole {
    pub const fn new(camera_frame: SemanticNodeId) -> Self {
        Self {
            camera_frame,
            capture_own_display: false,
        }
    }

    pub const fn capture_own_display(mut self, capture: bool) -> Self {
        self.capture_own_display = capture;
        self
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
    Inset2DView(SemanticInset2DViewRole),
    ArrowShaft(SemanticArrowShaftRole),
    ArrowEndTip,
    ArrowStartTip,
    /// Scalar range of an ordinary coordinate shaft; no renderer specialization.
    NumberLine(SemanticNumberLineRole),
    /// Parameter interval of an ordinary sampled graph path.
    FunctionPlot(SemanticFunctionPlotRole),
    /// Identifies an ordinary retained rectangle in a Manim SampleSpace horizontal partition.
    SampleSpaceHorizontalPart,
    /// Identifies an ordinary retained rectangle in a Manim SampleSpace vertical partition.
    SampleSpaceVerticalPart,
}

impl SemanticObjectRole {
    pub fn is_valid(self) -> bool {
        match self {
            Self::ArrowShaft(policy) => policy.is_valid(),
            Self::NumberLine(range) => range.is_valid(),
            Self::FunctionPlot(range) => range.is_valid(),
            Self::Ordinary
            | Self::Camera2D
            | Self::Inset2DView(_)
            | Self::ArrowEndTip
            | Self::ArrowStartTip
            | Self::SampleSpaceHorizontalPart
            | Self::SampleSpaceVerticalPart => true,
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
    // Numeric inputs are only present on DecimalNumber objects. Keep ordinary
    // semantic objects to one optional pointer rather than embedding the
    // metadata payload in every authored object state.
    decimal_number: Option<Arc<SemanticDecimalNumber>>,
    text_presentation_baseline: Option<TextPresentationBaseline>,
    /// Optional, pointer-sized BarChart source metadata. The immutable payload
    /// is allocated only for retained chart bars, so ordinary scene objects do
    /// not carry a BarChart-sized role variant.
    bar_metadata: Option<Arc<SemanticBarMetadata>>,
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
            decimal_number: None,
            text_presentation_baseline: None,
            bar_metadata: None,
            signal_bindings: Vec::new(),
        }
    }

    /// Return receiver-owned state with target visual state.
    ///
    /// Persistent `become` keeps painter provenance, role, BarChart metadata,
    /// bindings and identity on the receiver while copying the target's content,
    /// transform and style.
    /// Keep this as an exhaustive struct literal: adding a new authored-state field
    /// must fail to compile until its ownership is explicitly classified here.
    pub fn with_visual_state_from(&self, target: &Self) -> Self {
        Self {
            content: target.content,
            transform: target.transform,
            style: target.style.clone(),
            presentation: self.presentation,
            role: self.role,
            // Manim become replaces geometry and paint, not the receiver's
            // number or formatting inputs. Explicit set_value changes those.
            decimal_number: self.decimal_number.clone(),
            text_presentation_baseline: self.text_presentation_baseline,
            bar_metadata: self.bar_metadata.clone(),
            signal_bindings: self.signal_bindings.clone(),
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

    pub const fn text_presentation_baseline(&self) -> Option<TextPresentationBaseline> {
        self.text_presentation_baseline
    }

    pub fn set_text_presentation_baseline(&mut self, baseline: TextPresentationBaseline) {
        self.text_presentation_baseline = Some(baseline);
    }

    pub fn clear_text_presentation_baseline(&mut self) {
        self.text_presentation_baseline = None;
    }

    /// Receiver-owned DecimalNumber inputs, retained across visual replacement.
    /// A subsequent set_value reconstructs text from these authored inputs.
    pub fn decimal_number(&self) -> Option<&SemanticDecimalNumber> {
        self.decimal_number.as_deref()
    }

    pub fn set_decimal_number(&mut self, value: Option<SemanticDecimalNumber>) {
        self.decimal_number = value.map(Arc::new);
    }

    /// Authored BarChart source values for an ordinary retained rectangle.
    pub fn bar_metadata(&self) -> Option<&SemanticBarMetadata> {
        self.bar_metadata.as_deref()
    }

    /// Assign BarChart metadata through a semantic transaction when the object
    /// is published. This setter exists for detached construction only.
    pub fn set_bar_metadata(&mut self, metadata: Option<Arc<SemanticBarMetadata>>) {
        self.bar_metadata = metadata;
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
    Image(crate::RasterImageContentRef),
}

impl ObjectContentRef {
    pub const fn image(&self) -> Option<crate::RasterImageContentRef> {
        match self {
            Self::Image(image) => Some(*image),
            Self::Geometry(_) | Self::Text(_) => None,
        }
    }

    pub fn geometry(&self) -> Option<&GeometryRef> {
        match self {
            Self::Geometry(geometry) => Some(geometry),
            Self::Text(_) | Self::Image(_) => None,
        }
    }

    pub const fn text(&self) -> Option<TextResourceHandle> {
        match self {
            Self::Geometry(_) | Self::Image(_) => None,
            Self::Text(handle) => Some(*handle),
        }
    }
}

impl From<crate::RasterImageContentRef> for ObjectContentRef {
    fn from(value: crate::RasterImageContentRef) -> Self {
        Self::Image(value)
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
    use crate::{Color, GeometryResourceArena, SemanticVec3, TextResourceId, Vec2, VectorPath};

    #[test]
    fn numeric_metadata_is_an_optional_shared_pointer() {
        assert_eq!(
            std::mem::size_of::<Option<Arc<SemanticDecimalNumber>>>(),
            std::mem::size_of::<Arc<SemanticDecimalNumber>>()
        );
    }

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
    fn visual_state_copy_preserves_receiver_owned_metadata() {
        let mut receiver = SemanticObjectState::new(StoredGeometry::Rectangle {
            size: Vec2::new(1.0, 1.0),
        });
        receiver.set_z_index(7.0);
        receiver.assign_insertion_order(11);
        receiver.set_role(SemanticObjectRole::Camera2D);
        let receiver_baseline = TextPresentationBaseline::new(36.0, 1.25).unwrap();
        receiver.set_text_presentation_baseline(receiver_baseline);
        receiver.set_bar_metadata(Some(Arc::new(SemanticBarMetadata {
            value: 3.0,
            original_color: Color::RED,
            width: 0.6,
            fill_opacity: 0.7,
            stroke_width: 3.0,
        })));
        let mut target = SemanticObjectState::new(StoredGeometry::Circle { radius: 2.0 });
        target.transform.translation = SemanticVec3::new(3.0, -2.0, 0.0);
        target.style.object_opacity = 0.25;
        target.set_z_index(-4.0);
        target.assign_insertion_order(99);
        target.set_role(SemanticObjectRole::ArrowEndTip);
        target.set_text_presentation_baseline(TextPresentationBaseline::new(72.0, 3.0).unwrap());
        target.set_bar_metadata(Some(Arc::new(SemanticBarMetadata {
            value: 9.0,
            original_color: Color::BLUE,
            width: 0.4,
            fill_opacity: 0.5,
            stroke_width: 1.0,
        })));

        let copied = receiver.with_visual_state_from(&target);

        assert_eq!(copied.content, target.content);
        assert_eq!(copied.transform, target.transform);
        assert_eq!(copied.style, target.style);
        assert_eq!(copied.presentation(), receiver.presentation());
        assert_eq!(copied.role(), receiver.role());
        assert_eq!(copied.text_presentation_baseline(), Some(receiver_baseline));
        assert_eq!(copied.bar_metadata(), receiver.bar_metadata());
        assert_eq!(copied.signal_bindings(), receiver.signal_bindings());
    }

    #[test]
    fn optional_bar_metadata_is_pointer_sized_and_does_not_enlarge_object_roles() {
        use std::mem::size_of;

        assert_eq!(
            size_of::<Option<Arc<SemanticBarMetadata>>>(),
            size_of::<usize>()
        );
        // NumberLine already carries three f64 values inline. The role budget
        // is that existing payload plus its aligned discriminant; optional bar
        // metadata must not increase the footprint of every ordinary object.
        assert!(
            size_of::<SemanticObjectRole>()
                <= size_of::<SemanticNumberLineRole>() + std::mem::align_of::<SemanticObjectRole>()
        );
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
