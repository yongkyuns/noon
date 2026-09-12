//! Static SVG import lowered into ordinary retained Noon vector geometry.
//!
//! Parsing and normalization happen entirely at author time. Supported leaves are
//! published as the same immutable `VectorPath` resources and semantic family
//! identities used by native geometry; no SVG-specific runtime or renderer state
//! exists below this module.

use crate::{AuthoringError, MobjectFamily, Scene};
use noon_core::{
    semantic_path_bounds, Color, SemanticMutationTransaction, SemanticNodeCreation,
    SemanticObjectState, SemanticPaint, SemanticStyle, SemanticTransform2_5D, SemanticVec3,
    StoredGeometry, StrokeCap, StrokeJoin, StrokeWidthMode, Vec2, VectorPath,
};
use std::rc::Rc;

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
            Self::Image => formatter.write_str("SVG image nodes are not supported in vector import"),
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
            Self::InvalidTargetDimension { .. } => None,
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
}

struct PreparedSvg {
    leaves: Vec<PreparedSvgLeaf>,
    transform: SemanticTransform2_5D,
}

impl Scene {
    /// Parse one static SVG string into a detached semantic family.
    ///
    /// Supported content is normalized before one atomic semantic publication.
    /// The returned family and its leaves are ordinary Noon identities backed by
    /// immutable geometry resources; later transforms/styles never reparse SVG.
    pub fn svg_from_str(&self, source: &str) -> Result<MobjectFamily, SvgAuthoringError> {
        self.svg_from_str_with_options(source, SvgImportOptions::default())
    }

    /// Parse one static SVG string with explicit post-import placement options.
    pub fn svg_from_str_with_options(
        &self,
        source: &str,
        options: SvgImportOptions,
    ) -> Result<MobjectFamily, SvgAuthoringError> {
        let prepared = prepare_svg(source, options)?;
        let store_rc = Rc::clone(self.integration_store());
        let mut paths = Vec::with_capacity(prepared.leaves.len());
        let mut styles = Vec::with_capacity(prepared.leaves.len());
        for leaf in prepared.leaves {
            paths.push(leaf.path);
            styles.push(leaf.style);
        }

        let family_id = {
            let mut store = store_rc.borrow_mut();
            store.with_geometry_paths(paths, |store, handles| {
                let mut transaction = SemanticMutationTransaction::new();
                let family = transaction.create_node(SemanticNodeCreation::family());
                for (handle, style) in handles.iter().copied().zip(styles.into_iter()) {
                    let mut state = SemanticObjectState::new(StoredGeometry::Resource(handle));
                    state.transform = prepared.transform;
                    state.style = style;
                    let object = transaction.create_node(SemanticNodeCreation::object(state));
                    transaction.add_member(family, object);
                }
                let result = transaction
                    .apply(store)
                    .map_err(AuthoringError::from)
                    .map_err(SvgAuthoringError::from)?;
                result.resolve(family).ok_or_else(|| {
                    SvgAuthoringError::Authoring(AuthoringError::UnresolvedCreatedNode(family))
                })
            })?
        };

        MobjectFamily::from_node(store_rc, family_id).map_err(SvgAuthoringError::from)
    }
}

fn prepare_svg(source: &str, options: SvgImportOptions) -> Result<PreparedSvg, SvgAuthoringError> {
    validate_target_dimension("height", options.height)?;
    validate_target_dimension("width", options.width)?;

    let xml_options = usvg::roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let document = usvg::roxmltree::Document::parse_with_options(source, xml_options)
        .map_err(SvgAuthoringError::Xml)?;
    reject_source_features(&document)?;

    let tree = usvg::Tree::from_xmltree(&document, &usvg::Options::default())
        .map_err(SvgAuthoringError::Parse)?;
    let mut leaves = Vec::new();
    collect_group(tree.root(), &mut leaves)?;
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
        declaration
            .split_once(':')
            .is_some_and(|(name, value)| {
                name.trim().eq_ignore_ascii_case(property)
                    && !value.trim().eq_ignore_ascii_case("none")
            })
    })
}

fn collect_group(
    group: &usvg::Group,
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
        return Err(SvgAuthoringError::Unsupported(
            SvgUnsupportedFeature::Mask,
        ));
    }
    if !group.filters().is_empty() {
        return Err(SvgAuthoringError::Unsupported(
            SvgUnsupportedFeature::Filter,
        ));
    }

    for node in group.children() {
        match node {
            usvg::Node::Group(group) => collect_group(group, leaves)?,
            usvg::Node::Path(path) => {
                if path.is_visible() {
                    leaves.push(prepare_path(path)?);
                }
            }
            usvg::Node::Image(_) => {
                return Err(SvgAuthoringError::Unsupported(
                    SvgUnsupportedFeature::Image,
                ));
            }
            usvg::Node::Text(_) => {
                return Err(SvgAuthoringError::Unsupported(
                    SvgUnsupportedFeature::Text,
                ));
            }
        }
    }
    Ok(())
}

fn prepare_path(path: &usvg::Path) -> Result<PreparedSvgLeaf, SvgAuthoringError> {
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
    let path = prepare_vector_path(path)?;
    Ok(PreparedSvgLeaf { path, style })
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
            (
                Some(SemanticPaint::Solid(solid_paint(stroke.paint())?)),
                f64::from(stroke.opacity().get()),
                f64::from(stroke.width().get()),
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
            PathSegment::QuadTo(control, point) => result
                .quadratic_to(svg_point(control, transform), svg_point(point, transform)),
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

fn svg_point(
    mut point: usvg::tiny_skia_path::Point,
    transform: usvg::Transform,
) -> Vec2 {
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
        SemanticVec3::new(
            center_x * (1.0 - scale),
            center_y * (1.0 - scale),
            0.0,
        )
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

    #[test]
    fn static_svg_paths_publish_as_one_retained_family_transaction() {
        let scene = Scene::new();
        let before = scene.revision();
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10">
            <rect x="0" y="0" width="10" height="10" fill="#ff0000"/>
            <path d="M 10 0 L 20 0 L 20 10 Z" fill="#00ff00"/>
        </svg>"#;

        let family = scene.svg_from_str(svg).unwrap();
        assert_eq!(
            scene.revision(),
            before.checked_next().expect("one SVG publication revision")
        );
        let members = scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(family.node_id())
            .unwrap();
        assert_eq!(members.len(), 2);
        let bounds = family.layout_bounds().unwrap().unwrap();
        assert!((bounds.height() - 2.0).abs() < 1e-5);
        assert!((bounds.min_x + bounds.max_x).abs() < 1e-5);
        assert!((bounds.min_y + bounds.max_y).abs() < 1e-5);
    }

    #[test]
    fn svg_transform_and_y_flip_are_baked_into_retained_path() {
        let scene = Scene::new();
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
            <path d="M 1 2 L 4 2" transform="translate(3 5)" fill="none" stroke="#112233"/>
        </svg>"#;
        let family = scene
            .svg_from_str_with_options(svg, raw_options())
            .unwrap();
        let member = scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(family.node_id())
            .unwrap()[0];
        let object = crate::Mobject::from_node(Rc::clone(scene.integration_store()), member).unwrap();
        let path = object.path_query().unwrap().path().unwrap();
        let commands = path.commands();
        assert_eq!(commands[0], noon_core::PathCommand::MoveTo { to: Vec2::new(4.0, -7.0) });
        assert_eq!(commands[1], noon_core::PathCommand::LineTo { to: Vec2::new(7.0, -7.0) });
    }

    #[test]
    fn inherited_solid_style_reaches_semantic_leaf() {
        let scene = Scene::new();
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
            <g fill="#123456" fill-opacity="0.25" stroke="#abcdef" stroke-opacity="0.5" stroke-width="2">
                <path d="M 0 0 L 10 0 L 10 10 Z"/>
            </g>
        </svg>"#;
        let family = scene
            .svg_from_str_with_options(svg, raw_options())
            .unwrap();
        let store = scene.integration_store().borrow();
        let member = store.semantic_family_members_checked(family.node_id()).unwrap()[0];
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
        assert!((state.style.stroke_width - 2.0).abs() < 1e-6);
    }

    #[test]
    fn unsupported_svg_fails_before_semantic_publication() {
        let scene = Scene::new();
        let before = scene.revision();
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
            <defs><linearGradient id="g"><stop offset="0" stop-color="red"/></linearGradient></defs>
            <rect width="10" height="10" fill="url(#g)"/>
        </svg>"#;

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
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
            <g opacity="0.5"><rect width="10" height="10" fill="red"/></g>
        </svg>"#;
        assert!(matches!(
            scene.svg_from_str(svg),
            Err(SvgAuthoringError::Unsupported(
                SvgUnsupportedFeature::GroupOpacity
            ))
        ));
        assert_eq!(scene.revision(), before);
    }
}
