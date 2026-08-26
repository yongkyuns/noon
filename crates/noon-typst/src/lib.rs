//! Typst-backed text/math layout for Noon.
//!
//! This crate is intentionally separate from `noon-core`: Typst is one concrete
//! layout backend, while the semantic core retains a backend-neutral text resource
//! contract. The initial bridge emits deterministic shrink-wrapped SVG plus layout
//! identity. Renderer integration can later walk Typst frames directly and normalize
//! glyph/vector items without changing the public resource model.

use std::{fmt, sync::Arc};

use noon_core::{
    TextLayoutArtifact, TextLayoutBackend, TextLayoutBackendKind, TextSourceKind, Vec2,
};
use typst_as_lib::TypstEngine;
use typst_layout::PagedDocument;
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

/// First-stage Typst compilation result.
///
/// SVG is an integration artifact, not the eventual retained representation. The
/// renderer should consume normalized `TextResource` glyph/vector data once direct
/// frame extraction lands.
#[derive(Clone, Debug, PartialEq)]
pub struct TypstSvgArtifact {
    pub mode: TypstMode,
    pub source: Arc<str>,
    pub prepared_source: Arc<str>,
    pub svg: Arc<str>,
    pub size_points: Vec2,
    pub layout: TextLayoutArtifact,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypstBackendError {
    Compile(Arc<str>),
    EmptyDocument,
    MultiPage { pages: usize },
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
        }
    }
}

impl std::error::Error for TypstBackendError {}

/// Compile Typst markup/math using bundled Typst fonts and return deterministic SVG.
///
/// No system font scan, filesystem resolver, remote package resolver, or external
/// executable is involved. This keeps browser/native behavior reproducible.
pub fn compile_typst(source: &str, mode: TypstMode) -> Result<TypstSvgArtifact, TypstBackendError> {
    let prepared_source = prepare_source(source, mode);
    let engine = TypstEngine::builder()
        .main_file(prepared_source.as_str())
        .fonts(typst_assets::fonts())
        .build();

    let compiled = engine.compile::<PagedDocument>();
    let document = compiled
        .output
        .map_err(|error| TypstBackendError::Compile(Arc::from(error.to_string())))?;

    let pages = document.pages();
    let page = match pages {
        [] => return Err(TypstBackendError::EmptyDocument),
        [page] => page,
        pages => return Err(TypstBackendError::MultiPage { pages: pages.len() }),
    };

    let size = page.frame.size();
    let svg = svg(page, &SvgOptions::default());
    let artifact_fingerprint = fingerprint_hex(svg.as_bytes());

    Ok(TypstSvgArtifact {
        mode,
        source: Arc::from(source),
        prepared_source: Arc::from(prepared_source),
        svg: Arc::from(svg),
        size_points: Vec2::new(size.x.to_pt() as f32, size.y.to_pt() as f32),
        layout: TextLayoutArtifact {
            backend: TextLayoutBackend {
                kind: TextLayoutBackendKind::Typst,
                version: Arc::from(TYPST_BACKEND_VERSION),
            },
            template_fingerprint: Arc::from(fingerprint_hex(TEMPLATE_VERSION.as_bytes())),
            artifact_fingerprint: Arc::from(artifact_fingerprint),
            backend_payload_key: None,
        },
    })
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
    fn compilation_is_deterministic_for_identical_input() {
        let first = compile_typst("$ x^2 $", TypstMode::Markup).unwrap();
        let second = compile_typst("$ x^2 $", TypstMode::Markup).unwrap();
        assert_eq!(first.svg, second.svg);
        assert_eq!(
            first.layout.artifact_fingerprint,
            second.layout.artifact_fingerprint
        );
    }

    #[test]
    fn template_and_source_fingerprints_are_stable() {
        assert_eq!(fingerprint_hex(b"noon"), fingerprint_hex(b"noon"));
        assert_ne!(fingerprint_hex(b"noon"), fingerprint_hex(b"Noon"));
        assert!(prepare_source("x", TypstMode::Math).starts_with(TEMPLATE_PREFIX));
    }
}
