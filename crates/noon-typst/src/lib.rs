//! Typst-backed text/math layout for Noon.
//!
//! Typst remains one concrete layout backend. This crate owns compilation and
//! normalization into `noon-core`'s backend-neutral retained text resources so the
//! renderer never needs to understand Typst frames or retain SVG markup.

use std::{fmt, sync::Arc};

use noon_core::{
    Color, FontFaceIdentity, GeometryResourceArena, GlyphRun, PositionedGlyph, Rect,
    TextAffineTransform, TextClusterIdentity, TextDirection, TextLayoutArtifact, TextLayoutBackend,
    TextLayoutBackendKind, TextPart, TextResource, TextResourceValidationError, TextSourceKind,
    TextSourceSpan, TextVectorItem, TextVectorStyle, Vec2, VectorPath,
};
use typst_as_lib::TypstEngine;
use typst_layout::PagedDocument;
use typst_library::{
    layout::{Frame, FrameItem, Point, Transform},
    text::TextItem,
    visualize::{CurveItem, Geometry, Paint, Shape},
};
use typst_svg::{svg, SvgOptions};

pub const TYPST_BACKEND_VERSION: &str = "0.15.1";
const TEMPLATE_VERSION: &str = "noon-typst-page-v1";
const TEMPLATE_PREFIX: &str = "#set page(width: auto, height: auto, margin: 0pt, fill: none)\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TypstMode {
    Markup,
    Math,
}

impl TypstMode {
    pub const fn source_kind(self) -> TextSourceKind {
        match self {
            Self::Markup => TextSourceKind::Typst,
            Self::Math => TextSourceKind::MathTypst,
        }
    }
}

/// SVG compatibility/debug result. SVG is never retained by the normalized text path.
#[derive(Clone, Debug, PartialEq)]
pub struct TypstSvgArtifact {
    pub mode: TypstMode,
    pub source: Arc<str>,
    pub prepared_source: Arc<str>,
    pub svg: Arc<str>,
    pub size_points: Vec2,
    pub layout: TextLayoutArtifact,
}

/// Direct retained result used by Noon rendering/animation integration.
///
/// `resource` contains shaped glyph runs and references into `geometry` for Typst
/// vector decorations such as fraction rules and authored shapes. No SVG string or
/// Typst frame is retained after this value is constructed.
#[derive(Clone, Debug)]
pub struct TypstResourceArtifact {
    pub mode: TypstMode,
    pub source: Arc<str>,
    pub prepared_source: Arc<str>,
    pub resource: TextResource,
    pub geometry: GeometryResourceArena,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypstBackendError {
    Compile(Arc<str>),
    EmptyDocument,
    MultiPage { pages: usize },
    SourceTooLarge,
    UnsupportedGradientOrTiling,
    UnsupportedImage,
    UnsupportedClip,
    InvalidResource(TextResourceValidationError),
}

impl fmt::Display for TypstBackendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compile(message) => write!(formatter, "Typst compilation failed: {message}"),
            Self::EmptyDocument => write!(formatter, "Typst produced no pages"),
            Self::MultiPage { pages } => write!(
                formatter,
                "Typst text/math resources must produce one page, got {pages}"
            ),
            Self::SourceTooLarge => {
                write!(formatter, "Typst source exceeds Noon's text span space")
            }
            Self::UnsupportedGradientOrTiling => write!(
                formatter,
                "Typst gradients/tilings are not yet representable by retained text styles"
            ),
            Self::UnsupportedImage => write!(
                formatter,
                "Typst image frame items are not yet representable by retained text resources"
            ),
            Self::UnsupportedClip => write!(
                formatter,
                "Typst clipped text groups are not yet representable by retained text resources"
            ),
            Self::InvalidResource(error) => {
                write!(formatter, "invalid normalized text resource: {error}")
            }
        }
    }
}

impl std::error::Error for TypstBackendError {}

/// Compile Typst markup/math using bundled Typst fonts and return deterministic SVG.
///
/// This stays available for compatibility, diagnostics, and raster-differential
/// fixtures. Production retained rendering should use [`compile_typst_resource`].
pub fn compile_typst(source: &str, mode: TypstMode) -> Result<TypstSvgArtifact, TypstBackendError> {
    let (document, prepared_source) = compile_document(source, mode)?;
    let page = one_page(&document)?;
    let size = page.frame.size();
    let svg = svg(page, &SvgOptions::default());

    Ok(TypstSvgArtifact {
        mode,
        source: Arc::from(source),
        prepared_source: Arc::from(prepared_source.as_str()),
        svg: Arc::from(svg.as_str()),
        size_points: Vec2::new(size.x.to_pt() as f32, size.y.to_pt() as f32),
        layout: layout_artifact(&prepared_source),
    })
}

/// Compile Typst directly into Noon's retained glyph/vector resource model.
pub fn compile_typst_resource(
    source: &str,
    mode: TypstMode,
) -> Result<TypstResourceArtifact, TypstBackendError> {
    let source_len = u32::try_from(source.len()).map_err(|_| TypstBackendError::SourceTooLarge)?;
    let (document, prepared_source) = compile_document(source, mode)?;
    let page = one_page(&document)?;
    let size = page.frame.size();
    let width = size.x.to_pt() as f32;
    let height = size.y.to_pt() as f32;
    let full_span = TextSourceSpan::new(0, source_len);

    let mut normalizer = FrameNormalizer {
        source,
        full_span,
        page_to_noon: TextAffineTransform {
            xx: 1.0,
            yx: 0.0,
            xy: 0.0,
            yy: -1.0,
            tx: -0.5 * width,
            ty: 0.5 * height,
        },
        runs: Vec::new(),
        vectors: Vec::new(),
        parts: Vec::new(),
        geometry: GeometryResourceArena::new(),
        clusters: 0,
    };
    normalizer.walk_frame(&page.frame, TextAffineTransform::IDENTITY, None)?;

    // The entire mobject is always one addressable part. Labeled Typst groups are
    // appended as narrower semantic parts during traversal.
    normalizer.parts.insert(
        0,
        TextPart {
            source_span: full_span,
            first_cluster: 0,
            cluster_count: normalizer.clusters,
            first_vector: 0,
            vector_count: normalizer.vectors.len() as u32,
            semantic_key: None,
        },
    );

    let resource = TextResource {
        source: Arc::from(source),
        kind: mode.source_kind(),
        runs: normalizer.runs.into(),
        vector_items: normalizer.vectors.into(),
        parts: normalizer.parts.into(),
        bounds: Rect::new(
            Vec2::new(-0.5 * width, -0.5 * height),
            Vec2::new(0.5 * width, 0.5 * height),
        ),
        baseline: 0.5 * height - page.frame.baseline().to_pt() as f32,
        layout_artifact: Some(layout_artifact(&prepared_source)),
    };
    resource
        .validate()
        .map_err(TypstBackendError::InvalidResource)?;

    Ok(TypstResourceArtifact {
        mode,
        source: Arc::from(source),
        prepared_source: Arc::from(prepared_source),
        resource,
        geometry: normalizer.geometry,
    })
}

fn compile_document(
    source: &str,
    mode: TypstMode,
) -> Result<(PagedDocument, String), TypstBackendError> {
    let prepared_source = prepare_source(source, mode);
    let engine = TypstEngine::builder()
        .main_file(prepared_source.as_str())
        .fonts(typst_assets::fonts())
        .build();

    let compiled = engine.compile::<PagedDocument>();
    let document = compiled
        .output
        .map_err(|error| TypstBackendError::Compile(Arc::from(error.to_string())))?;
    Ok((document, prepared_source))
}

fn one_page(document: &PagedDocument) -> Result<&typst_layout::Page, TypstBackendError> {
    match document.pages() {
        [] => Err(TypstBackendError::EmptyDocument),
        [page] => Ok(page),
        pages => Err(TypstBackendError::MultiPage { pages: pages.len() }),
    }
}

fn layout_artifact(prepared_source: &str) -> TextLayoutArtifact {
    let artifact_identity =
        format!("{TYPST_BACKEND_VERSION}\0{TEMPLATE_VERSION}\0{prepared_source}");
    TextLayoutArtifact {
        backend: TextLayoutBackend {
            kind: TextLayoutBackendKind::Typst,
            version: Arc::from(TYPST_BACKEND_VERSION),
        },
        template_fingerprint: Arc::from(fingerprint_hex(TEMPLATE_VERSION.as_bytes())),
        artifact_fingerprint: Arc::from(fingerprint_hex(artifact_identity.as_bytes())),
        backend_payload_key: None,
    }
}

struct FrameNormalizer<'a> {
    source: &'a str,
    full_span: TextSourceSpan,
    page_to_noon: TextAffineTransform,
    runs: Vec<GlyphRun>,
    vectors: Vec<TextVectorItem>,
    parts: Vec<TextPart>,
    geometry: GeometryResourceArena,
    clusters: u32,
}

impl FrameNormalizer<'_> {
    fn walk_frame(
        &mut self,
        frame: &Frame,
        state: TextAffineTransform,
        inherited_key: Option<Arc<str>>,
    ) -> Result<(), TypstBackendError> {
        for (position, item) in frame.items() {
            let item_state = point_translation(*position).then(state);
            match item {
                FrameItem::Group(group) => {
                    if group.clip.is_some() {
                        return Err(TypstBackendError::UnsupportedClip);
                    }
                    let key = group
                        .label
                        .map(|label| Arc::<str>::from(label.resolve()))
                        .or_else(|| inherited_key.clone());
                    let first_cluster = self.clusters;
                    let first_vector = self.vectors.len() as u32;
                    let group_state = typst_transform(group.transform).then(item_state);
                    self.walk_frame(&group.frame, group_state, key.clone())?;
                    if let Some(semantic_key) = key {
                        let cluster_count = self.clusters - first_cluster;
                        let vector_count = self.vectors.len() as u32 - first_vector;
                        if cluster_count != 0 || vector_count != 0 {
                            self.parts.push(TextPart {
                                source_span: self.full_span,
                                first_cluster,
                                cluster_count,
                                first_vector,
                                vector_count,
                                semantic_key: Some(semantic_key),
                            });
                        }
                    }
                }
                FrameItem::Text(text) => {
                    self.push_text(text, item_state, inherited_key.clone())?;
                }
                FrameItem::Shape(shape, _) => {
                    self.push_shape(shape, item_state, inherited_key.clone())?;
                }
                FrameItem::Image(_, _, _) => return Err(TypstBackendError::UnsupportedImage),
                FrameItem::Link(_, _) | FrameItem::Tag(_) => {}
            }
        }
        Ok(())
    }

    fn push_text(
        &mut self,
        text: &TextItem,
        state: TextAffineTransform,
        inherited_key: Option<Arc<str>>,
    ) -> Result<(), TypstBackendError> {
        let font = text.font.font();
        let family = font.info().family.clone();
        let face_key = font
            .post_script_name()
            .unwrap_or_else(|| format!("{}#{}", family, font.index()));
        let variation_key = format!("{:?}", text.font.variations());
        let fill = inherited_color(&text.fill)?;
        if let Some(stroke) = &text.stroke {
            // Ensure the retained path does not silently discard a paint mode even
            // though glyph stroke rendering is a later renderer task.
            let _ = solid_color(&stroke.paint)?;
        }

        let mut cursor = Vec2::ZERO;
        let font_size = text.size.to_pt() as f32;
        let mut glyphs = Vec::with_capacity(text.glyphs.len());
        for glyph in &text.glyphs {
            let offset = Vec2::new(
                glyph.x_offset.at(text.size).to_pt() as f32,
                glyph.y_offset.at(text.size).to_pt() as f32,
            );
            let advance = Vec2::new(
                glyph.x_advance.at(text.size).to_pt() as f32,
                glyph.y_advance.at(text.size).to_pt() as f32,
            );
            let origin = cursor + offset;
            let semantic_key = inherited_key.clone().or_else(|| {
                let slice = &text.text[glyph.range()];
                (!slice.is_empty()).then(|| Arc::<str>::from(slice))
            });

            // Exact glyph outlines remain lazy. Bounds are conservative layout bounds
            // and do not require extracting a path from the font.
            let x1 = origin.x.min(origin.x + advance.x);
            let mut x2 = origin.x.max(origin.x + advance.x);
            if (x2 - x1).abs() < f32::EPSILON {
                x2 = x1 + 0.5 * font_size;
            }
            let bounds = Rect::new(
                Vec2::new(x1, origin.y - 0.3 * font_size),
                Vec2::new(x2, origin.y + 0.9 * font_size),
            );
            glyphs.push(PositionedGlyph {
                glyph_id: u32::from(glyph.id),
                cluster: TextClusterIdentity {
                    // Typst's Span is backend-internal. Until the source-map bridge is
                    // added, stable glyph/label semantic keys plus ordinal identity are
                    // retained while the span conservatively addresses the full source.
                    source_span: self.full_span,
                    cluster_ordinal: self.clusters,
                    semantic_key,
                },
                origin,
                advance,
                bounds,
            });
            self.clusters += 1;
            cursor += advance;
        }

        // Typst renders glyph outlines in a Y-up font coordinate system while its
        // frames are Y-down. Conjugating that flip with the accumulated frame state
        // and final page-to-Noon flip preserves arbitrary group affine transforms.
        let font_y_up_to_frame = TextAffineTransform {
            yy: -1.0,
            ..TextAffineTransform::IDENTITY
        };
        let transform = font_y_up_to_frame.then(state).then(self.page_to_noon);

        self.runs.push(GlyphRun {
            font: FontFaceIdentity {
                family: Arc::from(family),
                face_key: Arc::from(face_key),
                face_index: font.index(),
                variation_key: Arc::from(variation_key),
            },
            font_size,
            direction: TextDirection::LeftToRight,
            fill,
            transform,
            glyphs: glyphs.into(),
        });
        Ok(())
    }

    fn push_shape(
        &mut self,
        shape: &Shape,
        state: TextAffineTransform,
        semantic_key: Option<Arc<str>>,
    ) -> Result<(), TypstBackendError> {
        let path = geometry_path(&shape.geometry);
        let handle = self.geometry.insert_path(path);
        let fill = shape
            .fill
            .as_ref()
            .map(inherited_color)
            .transpose()?
            .flatten();
        let (stroke, stroke_width) = match &shape.stroke {
            Some(stroke) => (
                inherited_color(&stroke.paint)?,
                stroke.thickness.to_pt() as f32,
            ),
            None => (None, 0.0),
        };
        self.vectors.push(TextVectorItem {
            geometry: handle,
            transform: state.then(self.page_to_noon),
            style: TextVectorStyle {
                fill,
                stroke,
                stroke_width,
            },
            source_span: None,
            semantic_key,
        });
        Ok(())
    }
}

fn point_translation(point: Point) -> TextAffineTransform {
    TextAffineTransform::translation(point.x.to_pt() as f32, point.y.to_pt() as f32)
}

fn typst_transform(transform: Transform) -> TextAffineTransform {
    TextAffineTransform {
        xx: transform.sx.get() as f32,
        yx: transform.ky.get() as f32,
        xy: transform.kx.get() as f32,
        yy: transform.sy.get() as f32,
        tx: transform.tx.to_pt() as f32,
        ty: transform.ty.to_pt() as f32,
    }
}

fn geometry_path(geometry: &Geometry) -> VectorPath {
    match geometry {
        Geometry::Line(end) => VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(point_vec(*end)),
        Geometry::Rect(size) => {
            let width = size.x.to_pt() as f32;
            let height = size.y.to_pt() as f32;
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(width, 0.0))
                .line_to(Vec2::new(width, height))
                .line_to(Vec2::new(0.0, height))
                .close()
        }
        Geometry::Curve(curve) => {
            let mut path = VectorPath::new();
            for item in &curve.0 {
                path = match item {
                    CurveItem::Move(point) => path.move_to(point_vec(*point)),
                    CurveItem::Line(point) => path.line_to(point_vec(*point)),
                    CurveItem::Cubic(control1, control2, point) => path.cubic_to(
                        point_vec(*control1),
                        point_vec(*control2),
                        point_vec(*point),
                    ),
                    CurveItem::Close => path.close(),
                };
            }
            path
        }
    }
}

fn point_vec(point: Point) -> Vec2 {
    Vec2::new(point.x.to_pt() as f32, point.y.to_pt() as f32)
}

fn inherited_color(paint: &Paint) -> Result<Option<Color>, TypstBackendError> {
    let color = solid_color(paint)?;
    // Typst's default text/shape paint is black. Treat it as inherited so the
    // owning Noon mobject can apply Manim's color semantics without recompilation.
    if color.red.abs() < f32::EPSILON
        && color.green.abs() < f32::EPSILON
        && color.blue.abs() < f32::EPSILON
        && (color.alpha - 1.0).abs() < f32::EPSILON
    {
        Ok(None)
    } else {
        Ok(Some(color))
    }
}

fn solid_color(paint: &Paint) -> Result<Color, TypstBackendError> {
    let Paint::Solid(color) = paint else {
        return Err(TypstBackendError::UnsupportedGradientOrTiling);
    };
    let (red, green, blue, alpha) = color.to_rgb().into_components();
    Ok(Color::rgba(red, green, blue, alpha))
}

pub fn prepare_source(source: &str, mode: TypstMode) -> String {
    let mut prepared = String::with_capacity(TEMPLATE_PREFIX.len() + source.len() + 8);
    prepared.push_str(TEMPLATE_PREFIX);
    match mode {
        TypstMode::Markup => prepared.push_str(source),
        TypstMode::Math => {
            prepared.push_str("$ ");
            prepared.push_str(source);
            prepared.push_str(" $");
        }
    }
    prepared.push('\n');
    prepared
}

/// Stable FNV-1a fingerprint. This is an identity/cache key, not a security hash.
fn fingerprint_hex(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::GeometryResource;

    #[test]
    fn markup_compiles_to_one_shrink_wrapped_svg() {
        let artifact = compile_typst("Hello, Noon!", TypstMode::Markup).unwrap();
        assert_eq!(artifact.mode, TypstMode::Markup);
        assert_eq!(artifact.layout.backend.kind, TextLayoutBackendKind::Typst);
        assert_eq!(
            artifact.layout.backend.version.as_ref(),
            TYPST_BACKEND_VERSION
        );
        assert!(artifact.svg.starts_with("<svg"));
        assert!(artifact.size_points.x > 0.0);
        assert!(artifact.size_points.y > 0.0);
    }

    #[test]
    fn math_compiles_without_latex_translation() {
        let source = "frac(x^2, 2)";
        let artifact = compile_typst(source, TypstMode::Math).unwrap();
        assert_eq!(artifact.source.as_ref(), source);
        assert!(artifact.prepared_source.contains("$ frac(x^2, 2) $"));
        assert_eq!(TypstMode::Math.source_kind(), TextSourceKind::MathTypst);
        assert_ne!(TypstMode::Math.source_kind(), TextSourceKind::MathTex);
    }

    #[test]
    fn direct_markup_normalization_retains_shaped_glyphs_without_svg() {
        let artifact = compile_typst_resource("Hello", TypstMode::Markup).unwrap();
        assert_eq!(artifact.resource.kind, TextSourceKind::Typst);
        assert!(artifact.resource.glyph_count() >= 5);
        assert!(!artifact.resource.runs.is_empty());
        assert!(artifact.resource.bounds.width() > 0.0);
        assert!(artifact.resource.bounds.height() > 0.0);
        assert_eq!(
            artifact
                .resource
                .layout_artifact
                .as_ref()
                .unwrap()
                .backend
                .kind,
            TextLayoutBackendKind::Typst
        );
    }

    #[test]
    fn direct_math_normalization_retains_vector_decorations() {
        let artifact = compile_typst_resource("frac(x, 2)", TypstMode::Math).unwrap();
        assert_eq!(artifact.resource.kind, TextSourceKind::MathTypst);
        assert!(artifact.resource.glyph_count() >= 2);
        assert!(artifact.resource.vector_count() >= 1);
        assert!(artifact.geometry.len() >= 1);
        for item in artifact.resource.vector_items.iter() {
            assert!(matches!(
                artifact.geometry.get(item.geometry),
                Some(GeometryResource::VectorPath(_))
            ));
        }
    }

    #[test]
    fn authored_typst_color_is_intrinsic_but_default_black_inherits() {
        let default = compile_typst_resource("Hello", TypstMode::Markup).unwrap();
        assert!(default.resource.runs.iter().all(|run| run.fill.is_none()));

        let styled = compile_typst_resource("#text(fill: red)[Hello]", TypstMode::Markup).unwrap();
        assert!(styled.resource.runs.iter().any(|run| run.fill.is_some()));
    }

    #[test]
    fn labeled_groups_become_stable_semantic_parts() {
        let artifact =
            compile_typst_resource("#box([#label(\"lhs\") x]) + y", TypstMode::Markup).unwrap();
        assert!(artifact
            .resource
            .parts
            .iter()
            .any(|part| part.semantic_key.as_deref() == Some("lhs")));
    }

    #[test]
    fn compilation_is_deterministic_for_identical_input() {
        let first = compile_typst_resource("$ x^2 $", TypstMode::Markup).unwrap();
        let second = compile_typst_resource("$ x^2 $", TypstMode::Markup).unwrap();
        assert_eq!(first.resource.runs, second.resource.runs);
        assert_eq!(
            first
                .resource
                .layout_artifact
                .as_ref()
                .unwrap()
                .artifact_fingerprint,
            second
                .resource
                .layout_artifact
                .as_ref()
                .unwrap()
                .artifact_fingerprint
        );
    }

    #[test]
    fn template_and_source_fingerprints_are_stable() {
        assert_eq!(fingerprint_hex(b"noon"), fingerprint_hex(b"noon"));
        assert_ne!(fingerprint_hex(b"noon"), fingerprint_hex(b"Noon"));
        assert!(prepare_source("x", TypstMode::Math).starts_with(TEMPLATE_PREFIX));
    }
}
