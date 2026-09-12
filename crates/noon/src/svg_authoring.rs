//! Static SVG import lowered into ordinary retained Noon vector geometry.
//!
//! Parsing and normalization happen entirely at author time. Supported leaves are
//! published as the same immutable `VectorPath` resources and semantic family
//! identities used by native geometry; no SVG-specific runtime or renderer state
//! exists below this module.

use crate::{AuthoringError, MobjectFamily, Scene};
use noon_core::{
    semantic_path_bounds, Color, SemanticMutationTransaction, SemanticNodeCreation,
    SemanticObjectState, SemanticPaint, SemanticStore, SemanticStyle, SemanticTransform2_5D,
    SemanticVec3, SourceIdentity, StoredGeometry, StrokeCap, StrokeJoin, StrokeWidthMode, Vec2,
    VectorPath,
};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

const MANIM_DEFAULT_STROKE_WIDTH_SENTINEL: f32 = 0.000_001;
const MANIM_CAIRO_LINE_WIDTH_MULTIPLE: f64 = 0.01;
const SVG_SOURCE_IDENTITY_PREFIX: &str = "noon.svg.v1";

/// Positioning applied after SVG parsing.
///
/// The default mirrors Manim's `SVGMobject` placement contract: flip SVG's Y axis,
/// center the imported point bounds, and scale the aggregate height to two scene
/// units. Set both `height` and `width` to `None` to retain parsed SVG dimensions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvgImportOptions {
    pub should_center: bool,
    pub height: Option<f64>,
    pub width: Option<f64>,
}

impl Default for SvgImportOptions {
    fn default() -> Self {
        Self {
            should_center: true,
            height: Some(2.0),
            width: None,
        }
    }
}

/// SVG behavior that this bounded importer refuses rather than approximating.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SvgUnsupportedFeature {
    Element(String),
    VectorEffect,
    GroupOpacity,
    BlendMode,
    Isolation,
    ClipPath,
    Mask,
    Filter,
    Image,
    Text,
    GradientPaint,
    PatternPaint,
    EvenOddFillRule,
    StrokeDash,
    StrokeMiterClip,
    StrokeMiterLimit,
    StrokeFirstPaintOrder,
    ShapeRendering,
}

impl std::fmt::Display for SvgUnsupportedFeature {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Element(name) => write!(formatter, "unsupported SVG element <{name}>"),
            Self::VectorEffect => formatter.write_str("SVG vector-effect is not supported"),
            Self::GroupOpacity => formatter.write_str("SVG group opacity is not supported"),
            Self::BlendMode => formatter.write_str("SVG blend modes are not supported"),
            Self::Isolation => formatter.write_str("SVG isolation is not supported"),
            Self::ClipPath => formatter.write_str("SVG clip paths are not supported"),
            Self::Mask => formatter.write_str("SVG masks are not supported"),
            Self::Filter => formatter.write_str("SVG filters are not supported"),
            Self::Image => {
                formatter.write_str("SVG image nodes are not supported in vector import")
            }
            Self::Text => formatter.write_str("SVG text nodes are not supported in vector import"),
            Self::GradientPaint => formatter.write_str("SVG gradient paints are not supported"),
            Self::PatternPaint => formatter.write_str("SVG pattern paints are not supported"),
            Self::EvenOddFillRule => formatter.write_str("SVG even-odd fill is not supported"),
            Self::StrokeDash => formatter.write_str("SVG dashed strokes are not supported"),
            Self::StrokeMiterClip => formatter.write_str("SVG miter-clip joins are not supported"),
            Self::StrokeMiterLimit => {
                formatter.write_str("non-default SVG stroke miter limits are not supported")
            }
            Self::StrokeFirstPaintOrder => {
                formatter.write_str("SVG stroke-before-fill paint order is not supported")
            }
            Self::ShapeRendering => {
                formatter.write_str("non-default SVG shape-rendering is not supported")
            }
        }
    }
}

impl std::error::Error for SvgUnsupportedFeature {}

/// Deterministic failure from static SVG preparation or semantic publication.
#[derive(Debug)]
#[non_exhaustive]
pub enum SvgAuthoringError {
    Xml(usvg::roxmltree::Error),
    Parse(usvg::Error),
    Unsupported(SvgUnsupportedFeature),
    InvalidTargetDimension { name: &'static str, value: f64 },
    InvalidImportKey,
    Authoring(AuthoringError),
}

impl std::fmt::Display for SvgAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Xml(error) => error.fmt(formatter),
            Self::Parse(error) => error.fmt(formatter),
            Self::Unsupported(error) => error.fmt(formatter),
            Self::InvalidTargetDimension { name, .. } => {
                write!(formatter, "SVG target {name} must be finite and positive")
            }
            Self::InvalidImportKey => formatter.write_str(
                "SVG import key must contain at least one non-whitespace character",
            ),
            Self::Authoring(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for SvgAuthoringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Xml(error) => Some(error),
            Self::Parse(error) => Some(error),
            Self::Unsupported(error) => Some(error),
            Self::Authoring(error) => Some(error),
            Self::InvalidTargetDimension { .. } | Self::InvalidImportKey => None,
        }
    }
}

impl From<AuthoringError> for SvgAuthoringError {
    fn from(error: AuthoringError) -> Self {
        Self::Authoring(error)
    }
}

impl From<noon_core::GeometryResourceError> for SvgAuthoringError {
    fn from(error: noon_core::GeometryResourceError) -> Self {
        Self::Authoring(AuthoringError::GeometryResource(error))
    }
}

struct PreparedSvgLeaf {
    path: VectorPath,
    style: SemanticStyle,
    explicit_id: Option<String>,
    identity_locator: String,
}

struct PreparedSvg {
    leaves: Vec<PreparedSvgLeaf>,
    transform: SemanticTransform2_5D,
}

impl MobjectFamily {
    /// Parse one static SVG string directly into one shared semantic store.
    ///
    /// This is the store-scoped form used by integration frontends. It publishes
    /// no scene/root identity of its own; only the imported family, leaves, and
    /// immutable geometry resources enter the supplied arena.
    pub fn from_svg_str(
        store: Rc<RefCell<SemanticStore>>,
        source: &str,
    ) -> Result<Self, SvgAuthoringError> {
        Self::from_svg_str_with_options(store, source, SvgImportOptions::default())
    }

    /// Parse one static SVG string into a supplied semantic store with explicit
    /// post-import placement options.
    pub fn from_svg_str_with_options(
        store_rc: Rc<RefCell<SemanticStore>>,
        source: &str,
        options: SvgImportOptions,
    ) -> Result<Self, SvgAuthoringError> {
        Self::from_svg_str_internal(store_rc, source, options, None)
    }

    /// Parse one SVG using a caller-stable import key for root/leaf reconciliation.
    ///
    /// The key namespaces the existing semantic `SourceIdentity` mechanism. It is
    /// intentionally explicit: two ordinary imports of the same SVG may coexist,
    /// while callers that need source re-execution or hot-reload matching can opt
    /// into stable root/leaf source keys without creating another identity model.
    pub fn from_svg_str_with_import_key(
        store: Rc<RefCell<SemanticStore>>,
        source: &str,
        import_key: &str,
    ) -> Result<Self, SvgAuthoringError> {
        Self::from_svg_str_with_options_and_import_key(
            store,
            source,
            SvgImportOptions::default(),
            import_key,
        )
    }

    /// Parse one SVG with explicit placement and caller-stable import identity.
    pub fn from_svg_str_with_options_and_import_key(
        store_rc: Rc<RefCell<SemanticStore>>,
        source: &str,
        options: SvgImportOptions,
        import_key: &str,
    ) -> Result<Self, SvgAuthoringError> {
        validate_import_key(import_key)?;
        Self::from_svg_str_internal(store_rc, source, options, Some(import_key))
    }

    fn from_svg_str_internal(
        store_rc: Rc<RefCell<SemanticStore>>,
        source: &str,
        options: SvgImportOptions,
        import_key: Option<&str>,
    ) -> Result<Self, SvgAuthoringError> {
        let prepared = prepare_svg(source, options)?;
        let mut paths = Vec::with_capacity(prepared.leaves.len());
        let mut metadata = Vec::with_capacity(prepared.leaves.len());
        for leaf in prepared.leaves {
            paths.push(leaf.path);
            metadata.push((leaf.style, leaf.identity_locator));
        }

        let family_id = {
            let mut store = store_rc.borrow_mut();
            store.with_geometry_paths(paths, |store, handles| {
                let mut transaction = SemanticMutationTransaction::new();
                let family_creation =
                    with_svg_source_identity(SemanticNodeCreation::family(), import_key, "root");
                let family = transaction.create_node(family_creation);
                for (handle, (style, identity_locator)) in
                    handles.iter().copied().zip(metadata)
                {
                    let mut state = SemanticObjectState::new(StoredGeometry::Resource(handle));
                    state.transform = prepared.transform;
                    state.style = style;
                    let creation = with_svg_source_identity(
                        SemanticNodeCreation::object(state),
                        import_key,
                        &identity_locator,
                    );
                    let object = transaction.create_node(creation);
                    transaction.add_member(family, object);
                }
                let result = transaction
                    .apply(store)
                    .map_err(AuthoringError::from)
                    .map_err(SvgAuthoringError::from)?;
                result.resolve(family).ok_or(SvgAuthoringError::Authoring(
                    AuthoringError::UnresolvedCreatedNode(family),
                ))
            })?
        };

        Self::from_node(store_rc, family_id).map_err(SvgAuthoringError::from)
    }
}

impl Scene {
    /// Parse one static SVG string into a detached semantic family.
    ///
    /// Supported content is normalized before one atomic semantic publication.
    /// The returned family and its leaves are ordinary Noon identities backed by
    /// immutable geometry resources; later transforms/styles never reparse SVG.
    pub fn svg_from_str(&self, source: &str) -> Result<MobjectFamily, SvgAuthoringError> {
        MobjectFamily::from_svg_str(Rc::clone(self.integration_store()), source)
    }

    /// Parse one static SVG string with explicit post-import placement options.
    pub fn svg_from_str_with_options(
        &self,
        source: &str,
        options: SvgImportOptions,
    ) -> Result<MobjectFamily, SvgAuthoringError> {
        MobjectFamily::from_svg_str_with_options(
            Rc::clone(self.integration_store()),
            source,
            options,
        )
    }

    /// Parse one SVG with a stable caller-owned import key.
    pub fn svg_from_str_with_import_key(
        &self,
        source: &str,
        import_key: &str,
    ) -> Result<MobjectFamily, SvgAuthoringError> {
        MobjectFamily::from_svg_str_with_import_key(
            Rc::clone(self.integration_store()),
            source,
            import_key,
        )
    }

    /// Parse one SVG with explicit placement and stable caller-owned import key.
    pub fn svg_from_str_with_options_and_import_key(
        &self,
        source: &str,
        options: SvgImportOptions,
        import_key: &str,
    ) -> Result<MobjectFamily, SvgAuthoringError> {
        MobjectFamily::from_svg_str_with_options_and_import_key(
            Rc::clone(self.integration_store()),
            source,
            options,
            import_key,
        )
    }
}

fn with_svg_source_identity(
    creation: SemanticNodeCreation,
    import_key: Option<&str>,
    locator: &str,
) -> SemanticNodeCreation {
    match import_key {
        Some(import_key) => creation.with_source_identity(svg_source_identity(import_key, locator)),
        None => creation,
    }
}

fn svg_source_identity(import_key: &str, locator: &str) -> SourceIdentity {
    SourceIdentity::ExplicitKey(format!(
        "{SVG_SOURCE_IDENTITY_PREFIX}:{}:{import_key}:{}:{locator}",
        import_key.len(),
        locator.len(),
    ))
}

fn validate_import_key(import_key: &str) -> Result<(), SvgAuthoringError> {
    if import_key.trim().is_empty() {
        Err(SvgAuthoringError::InvalidImportKey)
    } else {
        Ok(())
    }
}

fn parse_xml(source: &str) -> Result<usvg::roxmltree::Document<'_>, SvgAuthoringError> {
    usvg::roxmltree::Document::parse_with_options(
        source,
        usvg::roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .map_err(SvgAuthoringError::Xml)
}

fn with_manim_svg_defaults(source: &str, document: &usvg::roxmltree::Document<'_>) -> String {
    let root = document.root_element();
    if root.tag_name().name() != "svg" || root.has_attribute("stroke-width") {
        return source.to_owned();
    }

    // `usvg` intentionally discards a stroke whose width resolves to zero, which
    // can also discard an otherwise fill-less path. Manim still retains that path
    // as a submobject. Use a tiny inherited parse-only sentinel so normalization
    // keeps the geometry, then lower that sentinel back to Manim's zero fallback.
    let root_start = root.range().start;
    let after_angle = root_start.saturating_add(1);
    let Some(name_end) = source[after_angle..]
        .find(|character: char| character.is_ascii_whitespace() || matches!(character, '>' | '/'))
        .map(|offset| after_angle + offset)
    else {
        return source.to_owned();
    };
    let mut normalized = String::with_capacity(source.len() + 24);
    normalized.push_str(&source[..name_end]);
    normalized.push_str(" stroke-width=\"0.000001\"");
    normalized.push_str(&source[name_end..]);
    normalized
}

fn prepare_svg(source: &str, options: SvgImportOptions) -> Result<PreparedSvg, SvgAuthoringError> {
    validate_target_dimension("height", options.height)?;
    validate_target_dimension("width", options.width)?;

    let document = parse_xml(source)?;
    reject_source_features(&document)?;
    let normalized_source = with_manim_svg_defaults(source, &document);
    let normalized_document = parse_xml(&normalized_source)?;

    let tree = usvg::Tree::from_xmltree(&normalized_document, &usvg::Options::default())
        .map_err(SvgAuthoringError::Parse)?;
    let mut leaves = Vec::new();
    collect_group(tree.root(), &mut Vec::new(), &mut leaves)?;
    resolve_leaf_identity_locators(&mut leaves);
    let bounds = aggregate_path_bounds(&leaves);
    let transform = placement_transform(bounds, options);
    Ok(PreparedSvg { leaves, transform })
}

fn validate_target_dimension(
    name: &'static str,
    value: Option<f64>,
) -> Result<(), SvgAuthoringError> {
    if let Some(value) = value {
        if !value.is_finite() || value <= 0.0 {
            return Err(SvgAuthoringError::InvalidTargetDimension { name, value });
        }
    }
    Ok(())
}

fn reject_source_features(
    document: &usvg::roxmltree::Document<'_>,
) -> Result<(), SvgAuthoringError> {
    for node in document.descendants().filter(|node| node.is_element()) {
        let name = node.tag_name().name();
        if is_unsupported_element(name) {
            return Err(SvgAuthoringError::Unsupported(
                SvgUnsupportedFeature::Element(name.to_owned()),
            ));
        }
        for attribute in node.attributes() {
            if attribute.name() == "vector-effect" && attribute.value().trim() != "none" {
                return Err(SvgAuthoringError::Unsupported(
                    SvgUnsupportedFeature::VectorEffect,
                ));
            }
            if attribute.name() == "style"
                && css_declares_non_none(attribute.value(), "vector-effect")
            {
                return Err(SvgAuthoringError::Unsupported(
                    SvgUnsupportedFeature::VectorEffect,
                ));
            }
        }
        if name == "style"
            && node
                .text()
                .is_some_and(|text| text.to_ascii_lowercase().contains("vector-effect"))
        {
            return Err(SvgAuthoringError::Unsupported(
                SvgUnsupportedFeature::VectorEffect,
            ));
        }
    }
    Ok(())
}

fn is_unsupported_element(name: &str) -> bool {
    matches!(
        name,
        "image"
            | "text"
            | "tspan"
            | "textPath"
            | "linearGradient"
            | "radialGradient"
            | "pattern"
            | "clipPath"
            | "mask"
            | "filter"
            | "marker"
            | "foreignObject"
            | "script"
            | "animate"
            | "animateMotion"
            | "animateTransform"
            | "set"
            | "feBlend"
            | "feColorMatrix"
            | "feComponentTransfer"
            | "feComposite"
            | "feConvolveMatrix"
            | "feDiffuseLighting"
            | "feDisplacementMap"
            | "feDistantLight"
            | "feDropShadow"
            | "feFlood"
            | "feFuncA"
            | "feFuncB"
            | "feFuncG"
            | "feFuncR"
            | "feGaussianBlur"
            | "feImage"
            | "feMerge"
            | "feMergeNode"
            | "feMorphology"
            | "feOffset"
            | "fePointLight"
            | "feSpecularLighting"
            | "feSpotLight"
            | "feTile"
            | "feTurbulence"
    )
}

fn css_declares_non_none(style: &str, property: &str) -> bool {
    style.split(';').any(|declaration| {
        declaration.split_once(':').is_some_and(|(name, value)| {
            name.trim().eq_ignore_ascii_case(property) && !value.trim().eq_ignore_ascii_case("none")
        })
    })
}

fn collect_group(
    group: &usvg::Group,
    tree_path: &mut Vec<usize>,
    leaves: &mut Vec<PreparedSvgLeaf>,
) -> Result<(), SvgAuthoringError> {
    if group.opacity().get() != 1.0 {
        return Err(SvgAuthoringError::Unsupported(
            SvgUnsupportedFeature::GroupOpacity,
        ));
    }
    if group.blend_mode() != usvg::BlendMode::Normal {
        return Err(SvgAuthoringError::Unsupported(
            SvgUnsupportedFeature::BlendMode,
        ));
    }
    if group.isolate() {
        return Err(SvgAuthoringError::Unsupported(
            SvgUnsupportedFeature::Isolation,
        ));
    }
    if group.clip_path().is_some() {
        return Err(SvgAuthoringError::Unsupported(
            SvgUnsupportedFeature::ClipPath,
        ));
    }
    if group.mask().is_some() {
        return Err(SvgAuthoringError::Unsupported(SvgUnsupportedFeature::Mask));
    }
    if !group.filters().is_empty() {
        return Err(SvgAuthoringError::Unsupported(
            SvgUnsupportedFeature::Filter,
        ));
    }

    for (child_index, node) in group.children().iter().enumerate() {
        tree_path.push(child_index);
        match node {
            usvg::Node::Group(group) => collect_group(group, tree_path, leaves)?,
            usvg::Node::Path(path) => {
                if path.is_visible() {
                    leaves.push(prepare_path(path, tree_locator(tree_path))?);
                }
            }
            usvg::Node::Image(_) => {
                return Err(SvgAuthoringError::Unsupported(SvgUnsupportedFeature::Image));
            }
            usvg::Node::Text(_) => {
                return Err(SvgAuthoringError::Unsupported(SvgUnsupportedFeature::Text));
            }
        }
        tree_path.pop();
    }
    Ok(())
}

fn tree_locator(tree_path: &[usize]) -> String {
    let mut locator = String::from("tree");
    for index in tree_path {
        locator.push('/');
        locator.push_str(&index.to_string());
    }
    locator
}

fn explicit_id_locator(id: &str) -> String {
    format!("element:{}:{id}", id.len())
}

fn resolve_leaf_identity_locators(leaves: &mut [PreparedSvgLeaf]) {
    let mut id_counts = HashMap::<String, usize>::new();
    for leaf in leaves.iter() {
        if let Some(id) = &leaf.explicit_id {
            *id_counts.entry(id.clone()).or_default() += 1;
        }
    }
    for leaf in leaves.iter_mut() {
        if let Some(id) = &leaf.explicit_id {
            if id_counts.get(id).copied() == Some(1) {
                leaf.identity_locator = explicit_id_locator(id);
            }
        }
    }
}

fn prepare_path(
    path: &usvg::Path,
    identity_locator: String,
) -> Result<PreparedSvgLeaf, SvgAuthoringError> {
    if path.paint_order() == usvg::PaintOrder::StrokeAndFill
        && path.fill().is_some()
        && path.stroke().is_some()
    {
        return Err(SvgAuthoringError::Unsupported(
            SvgUnsupportedFeature::StrokeFirstPaintOrder,
        ));
    }
    if path.rendering_mode() != usvg::ShapeRendering::GeometricPrecision {
        return Err(SvgAuthoringError::Unsupported(
            SvgUnsupportedFeature::ShapeRendering,
        ));
    }

    let style = prepare_style(path)?;
    let explicit_id = (!path.id().is_empty()).then(|| path.id().to_owned());
    let path = prepare_vector_path(path)?;
    Ok(PreparedSvgLeaf {
        path,
        style,
        explicit_id,
        identity_locator,
    })
}

fn prepare_style(path: &usvg::Path) -> Result<SemanticStyle, SvgAuthoringError> {
    let (fill, fill_opacity) = match path.fill() {
        Some(fill) => {
            if fill.rule() == usvg::FillRule::EvenOdd {
                return Err(SvgAuthoringError::Unsupported(
                    SvgUnsupportedFeature::EvenOddFillRule,
                ));
            }
            (
                Some(SemanticPaint::Solid(solid_paint(fill.paint())?)),
                f64::from(fill.opacity().get()),
            )
        }
        None => (None, 1.0),
    };

    let (stroke, stroke_opacity, stroke_width, stroke_join, stroke_cap) = match path.stroke() {
        Some(stroke) => {
            if stroke.dasharray().is_some() {
                return Err(SvgAuthoringError::Unsupported(
                    SvgUnsupportedFeature::StrokeDash,
                ));
            }
            let join = match stroke.linejoin() {
                usvg::LineJoin::Round => StrokeJoin::Round,
                usvg::LineJoin::Bevel => StrokeJoin::Bevel,
                usvg::LineJoin::Miter => {
                    if (stroke.miterlimit().get() - 4.0).abs() > 1e-6 {
                        return Err(SvgAuthoringError::Unsupported(
                            SvgUnsupportedFeature::StrokeMiterLimit,
                        ));
                    }
                    StrokeJoin::Miter
                }
                usvg::LineJoin::MiterClip => {
                    return Err(SvgAuthoringError::Unsupported(
                        SvgUnsupportedFeature::StrokeMiterClip,
                    ));
                }
            };
            let cap = match stroke.linecap() {
                usvg::LineCap::Round => StrokeCap::Round,
                usvg::LineCap::Butt => StrokeCap::Butt,
                usvg::LineCap::Square => StrokeCap::Square,
            };
            let resolved_width = stroke.width().get();
            let width = if resolved_width == MANIM_DEFAULT_STROKE_WIDTH_SENTINEL {
                0.0
            } else {
                f64::from(resolved_width) * MANIM_CAIRO_LINE_WIDTH_MULTIPLE
            };
            (
                Some(SemanticPaint::Solid(solid_paint(stroke.paint())?)),
                f64::from(stroke.opacity().get()),
                width,
                join,
                cap,
            )
        }
        None => (None, 1.0, 0.0, StrokeJoin::Round, StrokeCap::Round),
    };

    Ok(SemanticStyle {
        fill,
        fill_opacity,
        stroke,
        stroke_opacity,
        stroke_width,
        // Manim point transforms do not mutate authored stroke width. Keeping
        // stroke width invariant under the common post-import affine preserves
        // that compatibility behavior while the path itself remains retained.
        stroke_width_mode: StrokeWidthMode::ScreenSpace,
        stroke_join,
        stroke_cap,
        object_opacity: 1.0,
    })
}

fn solid_paint(paint: &usvg::Paint) -> Result<Color, SvgAuthoringError> {
    match paint {
        usvg::Paint::Color(color) => Ok(Color::rgb(
            f32::from(color.red) / 255.0,
            f32::from(color.green) / 255.0,
            f32::from(color.blue) / 255.0,
        )),
        usvg::Paint::LinearGradient(_) | usvg::Paint::RadialGradient(_) => Err(
            SvgAuthoringError::Unsupported(SvgUnsupportedFeature::GradientPaint),
        ),
        usvg::Paint::Pattern(_) => Err(SvgAuthoringError::Unsupported(
            SvgUnsupportedFeature::PatternPaint,
        )),
    }
}

fn prepare_vector_path(path: &usvg::Path) -> Result<VectorPath, SvgAuthoringError> {
    use usvg::tiny_skia_path::PathSegment;

    let transform = path.abs_transform();
    let mut result = VectorPath::new();
    for segment in path.data().segments() {
        result = match segment {
            PathSegment::MoveTo(point) => result.move_to(svg_point(point, transform)),
            PathSegment::LineTo(point) => result.line_to(svg_point(point, transform)),
            PathSegment::QuadTo(control, point) => {
                result.quadratic_to(svg_point(control, transform), svg_point(point, transform))
            }
            PathSegment::CubicTo(control1, control2, point) => result.cubic_to(
                svg_point(control1, transform),
                svg_point(control2, transform),
                svg_point(point, transform),
            ),
            PathSegment::Close => result.close(),
        };
    }
    if !result.is_finite() {
        return Err(SvgAuthoringError::Authoring(
            AuthoringError::NonFiniteGeometry,
        ));
    }
    Ok(result)
}

fn svg_point(mut point: usvg::tiny_skia_path::Point, transform: usvg::Transform) -> Vec2 {
    transform.map_point(&mut point);
    // SVG canvas coordinates grow down; Manim/Noon scene coordinates grow up.
    Vec2::new(point.x, -point.y)
}

fn aggregate_path_bounds(leaves: &[PreparedSvgLeaf]) -> Option<noon_core::Bounds2D64> {
    let mut aggregate: Option<noon_core::Bounds2D64> = None;
    for leaf in leaves {
        let Some(bounds) = semantic_path_bounds(&leaf.path, 0.0).layout else {
            continue;
        };
        if let Some(aggregate) = &mut aggregate {
            aggregate.include(bounds.min_x, bounds.min_y);
            aggregate.include(bounds.max_x, bounds.max_y);
        } else {
            aggregate = Some(bounds);
        }
    }
    aggregate
}

fn placement_transform(
    bounds: Option<noon_core::Bounds2D64>,
    options: SvgImportOptions,
) -> SemanticTransform2_5D {
    let Some(bounds) = bounds else {
        return SemanticTransform2_5D::default();
    };
    let center_x = (bounds.min_x + bounds.max_x) * 0.5;
    let center_y = (bounds.min_y + bounds.max_y) * 0.5;
    let mut scale = 1.0;
    if let Some(height) = options.height {
        if bounds.height() > 0.0 {
            scale *= height / bounds.height();
        }
    }
    if let Some(width) = options.width {
        let current_width = bounds.width() * scale.abs();
        if current_width > 0.0 {
            scale *= width / current_width;
        }
    }

    let translation = if options.should_center {
        SemanticVec3::new(-center_x * scale, -center_y * scale, 0.0)
    } else {
        SemanticVec3::new(center_x * (1.0 - scale), center_y * (1.0 - scale), 0.0)
    };
    SemanticTransform2_5D {
        translation,
        scale: SemanticVec3::new(scale, scale, 1.0),
        rotation_z: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_options() -> SvgImportOptions {
        SvgImportOptions {
            should_center: false,
            height: None,
            width: None,
        }
    }

    fn source_identity_for(store: &SemanticStore, node: noon_core::SemanticNodeId) -> SourceIdentity {
        store
            .node(node)
            .and_then(|node| node.source_identity())
            .cloned()
            .expect("identity-aware SVG node must carry source identity")
    }

    #[test]
    fn static_svg_paths_publish_as_one_retained_family_transaction() {
        let scene = Scene::new();
        let before = scene.revision();
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10">
            <rect x="0" y="0" width="10" height="10" fill="#ff0000"/>
            <path d="M 10 0 L 20 0 L 20 10 Z" fill="#00ff00"/>
        </svg>"##;

        let family = scene.svg_from_str(svg).unwrap();
        assert_eq!(
            scene.revision(),
            before.checked_next().expect("one SVG publication revision")
        );
        let store = scene.integration_store().borrow();
        let members = store
            .semantic_family_members_checked(family.node_id())
            .unwrap();
        assert_eq!(members.len(), 2);
        drop(store);
        let bounds = family.layout_bounds().unwrap().unwrap();
        assert!((bounds.height() - 2.0).abs() < 1e-5);
        assert!((bounds.min_x + bounds.max_x).abs() < 1e-5);
        assert!((bounds.min_y + bounds.max_y).abs() < 1e-5);
    }

    #[test]
    fn store_scoped_svg_import_publishes_no_extra_scene_root() {
        let store = Rc::new(RefCell::new(SemanticStore::new()));
        let before = store.borrow().scene_revision();
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
            <rect x="0" y="0" width="10" height="10" fill="#ff0000"/>
        </svg>"##;

        let family = MobjectFamily::from_svg_str(Rc::clone(&store), svg).unwrap();
        assert_eq!(
            store.borrow().scene_revision(),
            before.checked_next().expect("one SVG publication revision")
        );
        let members = store
            .borrow()
            .semantic_family_members_checked(family.node_id())
            .unwrap();
        assert_eq!(members.len(), 1);
    }

    #[test]
    fn explicit_svg_ids_are_stable_source_identity_across_reordering() {
        let first_store = Rc::new(RefCell::new(SemanticStore::new()));
        let second_store = Rc::new(RefCell::new(SemanticStore::new()));
        let first = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10">
            <rect id="left" x="0" y="0" width="10" height="10" fill="#ff0000"/>
            <path id="right" d="M 10 0 L 20 0 L 20 10 Z" fill="#00ff00"/>
        </svg>"##;
        let reordered = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10">
            <path id="right" d="M 10 0 L 20 0 L 20 10 Z" fill="#00ff00"/>
            <rect id="left" x="0" y="0" width="10" height="10" fill="#ff0000"/>
        </svg>"##;

        let first_family = MobjectFamily::from_svg_str_with_import_key(
            Rc::clone(&first_store),
            first,
            "icons/status.svg",
        )
        .unwrap();
        let second_family = MobjectFamily::from_svg_str_with_import_key(
            Rc::clone(&second_store),
            reordered,
            "icons/status.svg",
        )
        .unwrap();

        let identities = |store: &SemanticStore, family: &MobjectFamily| {
            let mut identities = store
                .semantic_family_members_checked(family.node_id())
                .unwrap()
                .iter()
                .map(|member| source_identity_for(store, *member))
                .collect::<Vec<_>>();
            identities.sort_by(|left, right| format!("{left:?}").cmp(&format!("{right:?}")));
            identities
        };

        let first_borrowed = first_store.borrow();
        let second_borrowed = second_store.borrow();
        assert_eq!(
            source_identity_for(&first_borrowed, first_family.node_id()),
            svg_source_identity("icons/status.svg", "root")
        );
        assert_eq!(
            source_identity_for(&second_borrowed, second_family.node_id()),
            svg_source_identity("icons/status.svg", "root")
        );
        assert_eq!(
            identities(&first_borrowed, &first_family),
            identities(&second_borrowed, &second_family)
        );
        assert!(identities(&first_borrowed, &first_family).contains(&svg_source_identity(
            "icons/status.svg",
            &explicit_id_locator("left")
        )));
        assert!(identities(&first_borrowed, &first_family).contains(&svg_source_identity(
            "icons/status.svg",
            &explicit_id_locator("right")
        )));
    }

    #[test]
    fn anonymous_svg_leaves_use_deterministic_normalized_tree_identity() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10">
            <g transform="translate(1 0)">
                <rect x="0" y="0" width="5" height="5" fill="#ff0000"/>
                <path d="M 5 0 L 10 0 L 10 5 Z" fill="#00ff00"/>
            </g>
        </svg>"##;
        let identities_for_fresh_store = || {
            let store = Rc::new(RefCell::new(SemanticStore::new()));
            let family = MobjectFamily::from_svg_str_with_import_key(
                Rc::clone(&store),
                svg,
                "anonymous.svg",
            )
            .unwrap();
            let borrowed = store.borrow();
            borrowed
                .semantic_family_members_checked(family.node_id())
                .unwrap()
                .iter()
                .map(|member| source_identity_for(&borrowed, *member))
                .collect::<Vec<_>>()
        };

        let first = identities_for_fresh_store();
        let second = identities_for_fresh_store();
        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
        assert_ne!(first[0], first[1]);
    }

    #[test]
    fn duplicate_import_key_fails_atomically() {
        let scene = Scene::new();
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
            <rect id="box" width="10" height="10" fill="#ff0000"/>
        </svg>"##;
        scene
            .svg_from_str_with_import_key(svg, "shared/icon.svg")
            .unwrap();
        let before = scene.revision();

        assert!(matches!(
            scene.svg_from_str_with_import_key(svg, "shared/icon.svg"),
            Err(SvgAuthoringError::Authoring(_))
        ));
        assert_eq!(scene.revision(), before);
    }

    #[test]
    fn ordinary_duplicate_imports_remain_allowed_without_import_key() {
        let scene = Scene::new();
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
            <rect id="box" width="10" height="10" fill="#ff0000"/>
        </svg>"##;

        scene.svg_from_str(svg).unwrap();
        scene.svg_from_str(svg).unwrap();
    }

    #[test]
    fn empty_import_key_is_rejected_before_publication() {
        let scene = Scene::new();
        let before = scene.revision();
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0 L1 0"/></svg>"##;

        assert!(matches!(
            scene.svg_from_str_with_import_key(svg, "   "),
            Err(SvgAuthoringError::InvalidImportKey)
        ));
        assert_eq!(scene.revision(), before);
    }

    #[test]
    fn svg_transform_and_y_flip_are_baked_into_retained_path() {
        let scene = Scene::new();
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
            <path d="M 1 2 L 4 2" transform="translate(3 5)" fill="none" stroke="#112233"/>
        </svg>"##;
        let family = scene.svg_from_str_with_options(svg, raw_options()).unwrap();
        let store = scene.integration_store().borrow();
        let member = store
            .semantic_family_members_checked(family.node_id())
            .unwrap()[0];
        drop(store);
        let object =
            crate::Mobject::from_node(Rc::clone(scene.integration_store()), member).unwrap();
        let query = object.path_query().unwrap();
        assert_eq!(query.start().unwrap(), (4.0, -7.0));
        assert_eq!(query.end().unwrap(), (7.0, -7.0));
    }

    #[test]
    fn inherited_solid_style_reaches_semantic_leaf() {
        let scene = Scene::new();
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
            <g fill="#123456" fill-opacity="0.25" stroke="#abcdef" stroke-opacity="0.5" stroke-width="2">
                <path d="M 0 0 L 10 0 L 10 10 Z"/>
            </g>
        </svg>"##;
        let family = scene.svg_from_str_with_options(svg, raw_options()).unwrap();
        let store = scene.integration_store().borrow();
        let member = store
            .semantic_family_members_checked(family.node_id())
            .unwrap()[0];
        let state = store.semantic_object_state_checked(member).unwrap();
        assert_eq!(
            state.style.fill,
            Some(SemanticPaint::Solid(Color::from_hex(0x123456)))
        );
        assert_eq!(
            state.style.stroke,
            Some(SemanticPaint::Solid(Color::from_hex(0xABCDEF)))
        );
        assert!((state.style.fill_opacity - 0.25).abs() < 1e-6);
        assert!((state.style.stroke_opacity - 0.5).abs() < 1e-6);
        assert!((state.style.stroke_width - 0.02).abs() < 1e-6);
    }

    #[test]
    fn manim_default_stroke_width_is_zero_and_explicit_width_wins() {
        let scene = Scene::new();
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10">
            <rect x="0" y="0" width="10" height="10" fill="#ffffff" stroke="#ff0000"/>
            <rect x="10" y="0" width="10" height="10" fill="#ffffff" stroke="#00ff00" stroke-width="3"/>
        </svg>"##;
        let family = scene.svg_from_str_with_options(svg, raw_options()).unwrap();
        let store = scene.integration_store().borrow();
        let members = store
            .semantic_family_members_checked(family.node_id())
            .unwrap();
        assert_eq!(members.len(), 2);
        let defaulted = store.semantic_object_state_checked(members[0]).unwrap();
        let explicit = store.semantic_object_state_checked(members[1]).unwrap();
        assert_eq!(defaulted.style.stroke_width, 0.0);
        assert!((explicit.style.stroke_width - 0.03).abs() < 1e-6);
    }

    #[test]
    fn unsupported_svg_fails_before_semantic_publication() {
        let scene = Scene::new();
        let before = scene.revision();
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
            <defs><linearGradient id="g"><stop offset="0" stop-color="red"/></linearGradient></defs>
            <rect width="10" height="10" fill="url(#g)"/>
        </svg>"##;

        assert!(matches!(
            scene.svg_from_str(svg),
            Err(SvgAuthoringError::Unsupported(
                SvgUnsupportedFeature::Element(ref name)
            )) if name == "linearGradient"
        ));
        assert_eq!(scene.revision(), before);
    }

    #[test]
    fn group_compositing_is_rejected_instead_of_flattened() {
        let scene = Scene::new();
        let before = scene.revision();
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
            <g opacity="0.5"><rect width="10" height="10" fill="red"/></g>
        </svg>"##;
        assert!(matches!(
            scene.svg_from_str(svg),
            Err(SvgAuthoringError::Unsupported(
                SvgUnsupportedFeature::GroupOpacity
            ))
        ));
        assert_eq!(scene.revision(), before);
    }
}
