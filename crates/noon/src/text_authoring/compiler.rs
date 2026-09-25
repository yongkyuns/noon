//! Backend-neutral compiled-text cache used by every Rust text authoring path.
//!
//! The cache owns only normalized immutable artifacts. Semantic nodes continue to
//! own presentation and resource handles, so a cache hit never couples node
//! identity, transforms, or object style.

use super::*;
#[cfg(feature = "native-text")]
use noon_text::shaping::{NativeTextCompiler, NativeTextOptions};
#[cfg(feature = "typst")]
use noon_typst::{compile_typst_resource, compile_typst_resource_with_fonts};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
};

const MAX_ENTRIES: usize = 128;
const MAX_RETAINED_BYTES: usize = 32 * 1024 * 1024;
const NATIVE_BACKEND_VERSION: &str = "noon-native-swash-0.2.10-v2";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct TextCompileKey(Arc<[u8]>, Arc<[Arc<[u8]>]>);

#[derive(Clone, Debug)]
pub(crate) struct CompiledTextArtifact {
    pub resource: Arc<TextResource>,
    pub fonts: Arc<FontResourceArena>,
    pub geometry: Arc<GeometryResourceArena>,
}

impl CompiledTextArtifact {
    fn retained_bytes(&self) -> usize {
        self.resource
            .retained_bytes()
            .saturating_add(self.fonts.stats().retained_bytes)
            .saturating_add(self.geometry.stats().retained_bytes)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextCompilerDiagnostics {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub successful_compiles: u64,
    pub failed_compiles: u64,
    pub evictions: u64,
    /// Aggregate native wall-clock compile time. `None` on WASM, where this
    /// authoring layer has no portable monotonic clock and must not report a
    /// fabricated zero-duration measurement.
    pub compile_nanos: Option<u128>,
    pub entries: usize,
    pub retained_bytes: usize,
}

#[derive(Clone, Debug)]
struct CacheEntry {
    artifact: CompiledTextArtifact,
    retained_bytes: usize,
}

#[derive(Default)]
struct TextCompilerRegistry {
    entries: BTreeMap<TextCompileKey, CacheEntry>,
    lru: VecDeque<TextCompileKey>,
    diagnostics: TextCompilerDiagnostics,
}

impl TextCompilerRegistry {
    fn lookup(&mut self, key: &TextCompileKey) -> Option<CompiledTextArtifact> {
        if let Some(artifact) = self.entries.get(key).map(|entry| entry.artifact.clone()) {
            self.diagnostics.cache_hits += 1;
            self.touch(key);
            Some(artifact)
        } else {
            self.diagnostics.cache_misses += 1;
            None
        }
    }

    fn finish(
        &mut self,
        key: TextCompileKey,
        result: Result<CompiledTextArtifact, TextAuthoringError>,
        #[cfg(not(target_arch = "wasm32"))] elapsed_nanos: u128,
    ) -> Result<CompiledTextArtifact, TextAuthoringError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.diagnostics.compile_nanos = Some(
                self.diagnostics
                    .compile_nanos
                    .unwrap_or(0)
                    .saturating_add(elapsed_nanos),
            );
        }
        let artifact = match result {
            Ok(artifact) => artifact,
            Err(error) => {
                self.diagnostics.failed_compiles += 1;
                return Err(error);
            }
        };
        self.diagnostics.successful_compiles += 1;
        let retained_bytes = artifact
            .retained_bytes()
            .saturating_add(key.0.len())
            .saturating_add(key.1.iter().map(|font| font.len()).sum::<usize>());
        if let Some(previous) = self.entries.insert(
            key.clone(),
            CacheEntry {
                artifact: artifact.clone(),
                retained_bytes,
            },
        ) {
            self.diagnostics.retained_bytes = self
                .diagnostics
                .retained_bytes
                .saturating_sub(previous.retained_bytes);
        }
        self.diagnostics.retained_bytes = self
            .diagnostics
            .retained_bytes
            .saturating_add(retained_bytes);
        self.touch(&key);
        self.evict();
        self.diagnostics.entries = self.entries.len();
        Ok(artifact)
    }

    fn touch(&mut self, key: &TextCompileKey) {
        if let Some(index) = self.lru.iter().position(|candidate| candidate == key) {
            self.lru.remove(index);
        }
        self.lru.push_back(key.clone());
    }

    fn evict(&mut self) {
        while self.entries.len() > MAX_ENTRIES
            || self.diagnostics.retained_bytes > MAX_RETAINED_BYTES
        {
            let Some(key) = self.lru.pop_front() else {
                break;
            };
            if let Some(entry) = self.entries.remove(&key) {
                self.diagnostics.retained_bytes = self
                    .diagnostics
                    .retained_bytes
                    .saturating_sub(entry.retained_bytes);
                self.diagnostics.evictions += 1;
            }
        }
    }
}

fn compile_cached(
    key: TextCompileKey,
    compile: impl FnOnce() -> Result<CompiledTextArtifact, TextAuthoringError>,
) -> Result<CompiledTextArtifact, TextAuthoringError> {
    if let Some(artifact) = REGISTRY.with(|registry| registry.borrow_mut().lookup(&key)) {
        return Ok(artifact);
    }
    #[cfg(not(target_arch = "wasm32"))]
    let started = Instant::now();
    let result = compile();
    REGISTRY.with(|registry| {
        registry.borrow_mut().finish(
            key,
            result,
            #[cfg(not(target_arch = "wasm32"))]
            started.elapsed().as_nanos(),
        )
    })
}

thread_local! { static REGISTRY: RefCell<TextCompilerRegistry> = RefCell::new(TextCompilerRegistry::default()); }

pub fn text_compiler_diagnostics() -> TextCompilerDiagnostics {
    REGISTRY.with(|registry| registry.borrow().diagnostics)
}

#[cfg(test)]
pub(crate) fn clear_text_compiler_cache() {
    REGISTRY.with(|registry| *registry.borrow_mut() = TextCompilerRegistry::default());
}

#[cfg(feature = "native-text")]
pub(crate) fn compile_native(text: &Text) -> Result<Arc<CompiledTextArtifact>, TextAuthoringError> {
    text.validate()?;
    let key = native_key(text)?;
    let artifact = compile_cached(key, || {
        let font = match &text.font_face {
            Some(font) => font.clone(),
            None => bundled_native_font(text.font_family.as_ref())?,
        };
        let mut options = NativeTextOptions::new(text.font_size);
        options.line_spacing = text.line_spacing;
        // Object color is semantic presentation. It must not affect shaping or cache identity.
        options.fill = None;
        let mut compiler = NativeTextCompiler::new();
        let mut artifact = if text.markup {
            markup::compile(text, &font, &options, &mut compiler)?
        } else {
            compiler.compile_plain(text.source.as_ref(), &font, &options)?
        };
        text.apply_source_fills(&mut artifact.resource)?;
        Ok(CompiledTextArtifact {
            resource: Arc::new(artifact.resource),
            fonts: Arc::new(artifact.fonts),
            geometry: Arc::new(GeometryResourceArena::new()),
        })
    })?;
    Ok(Arc::new(artifact))
}

#[cfg(feature = "typst")]
pub(super) fn compile_typst(
    text: &TypstSpec,
    mode: TypstMode,
) -> Result<Arc<CompiledTextArtifact>, TextAuthoringError> {
    let key = typst_key(text, mode);
    let artifact = compile_cached(key, || {
        let artifact = match &text.fonts {
            Some(fonts) => {
                compile_typst_resource_with_fonts(text.source.as_ref(), mode, fonts.iter())?
            }
            None => compile_typst_resource(text.source.as_ref(), mode)?,
        };
        Ok(CompiledTextArtifact {
            resource: Arc::new(artifact.resource),
            fonts: Arc::new(artifact.fonts),
            geometry: Arc::new(artifact.geometry),
        })
    })?;
    Ok(Arc::new(artifact))
}

fn push_bytes(bytes: &mut Vec<u8>, value: &[u8]) {
    bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
    bytes.extend_from_slice(value);
}
fn push_text(bytes: &mut Vec<u8>, value: &str) {
    push_bytes(bytes, value.as_bytes());
}
fn push_color(bytes: &mut Vec<u8>, color: Color) {
    for value in [color.red, color.green, color.blue, color.alpha] {
        bytes.extend_from_slice(&value.to_bits().to_le_bytes());
    }
}

#[cfg(feature = "native-text")]
fn native_key(text: &Text) -> Result<TextCompileKey, TextAuthoringError> {
    let mut bytes = Vec::new();
    push_text(&mut bytes, "native");
    push_text(&mut bytes, NATIVE_BACKEND_VERSION);
    push_text(&mut bytes, text.source.as_ref());
    bytes.push(text.markup as u8);
    push_text(&mut bytes, text.font_family.as_ref());
    bytes.extend_from_slice(&text.font_size.to_bits().to_le_bytes());
    bytes.extend_from_slice(&text.line_spacing.to_bits().to_le_bytes());
    if let Some(face) = &text.font_face {
        bytes.push(1);
        push_text(&mut bytes, face.family.as_ref());
        bytes.extend_from_slice(&face.face_index.to_le_bytes());
        // NativeFontFace computes this immutable content identity once. The
        // Arc payload stored beside the descriptor remains the collision guard.
        push_text(&mut bytes, face.face_key.as_ref());
    } else {
        bytes.push(0);
    }
    bytes.extend_from_slice(&(text.source_fills.len() as u64).to_le_bytes());
    for fill in &text.source_fills {
        bytes.extend_from_slice(&fill.source_span.start.to_le_bytes());
        bytes.extend_from_slice(&fill.source_span.end.to_le_bytes());
        push_color(&mut bytes, fill.color);
    }
    bytes.extend_from_slice(&(text.text2color.len() as u64).to_le_bytes());
    for (selector, color) in &text.text2color {
        push_text(&mut bytes, selector.as_ref());
        push_color(&mut bytes, *color);
    }
    let fonts: Arc<[Arc<[u8]>]> = text
        .font_face
        .as_ref()
        .map(|face| vec![face.data.clone()].into())
        .unwrap_or_else(|| Arc::from([]));
    Ok(TextCompileKey(bytes.into(), fonts))
}

#[cfg(feature = "native-text")]
pub(crate) fn native_identity(
    text: &Text,
) -> Result<noon_core::TextCompilationIdentity, TextAuthoringError> {
    let key = native_key(text)?;
    Ok(noon_core::TextCompilationIdentity {
        descriptor: key.0,
        font_contents: key.1,
    })
}

#[cfg(feature = "typst")]
fn typst_key(text: &TypstSpec, mode: TypstMode) -> TextCompileKey {
    let mut bytes = Vec::new();
    push_text(&mut bytes, "typst");
    push_text(&mut bytes, noon_typst::TYPST_BACKEND_VERSION);
    push_text(&mut bytes, noon_typst::TYPST_TEMPLATE_VERSION);
    bytes.push(match mode {
        TypstMode::Markup => 0,
        TypstMode::Math => 1,
    });
    push_text(&mut bytes, text.source.as_ref());
    match &text.fonts {
        Some(fonts) => {
            bytes.push(1);
            bytes.extend_from_slice(&(fonts.len() as u64).to_le_bytes());
            for font in fonts.iter() {
                // Typst has no stable face key before loading the bytes. This
                // compact FNV descriptor is always paired with the full Arc
                // payload in TextCompileKey, so equality remains exact.
                bytes.extend_from_slice(&typst_fingerprint(font.as_ref()).to_le_bytes());
            }
        }
        None => bytes.push(0),
    }
    let font_contents = text.fonts.clone().unwrap_or_else(|| Arc::from([]));
    TextCompileKey(bytes.into(), font_contents)
}

#[cfg(feature = "typst")]
fn typst_fingerprint(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

#[cfg(feature = "typst")]
pub(super) fn typst_identity(
    text: &TypstSpec,
    mode: TypstMode,
) -> noon_core::TextCompilationIdentity {
    let key = typst_key(text, mode);
    noon_core::TextCompilationIdentity {
        descriptor: key.0,
        font_contents: key.1,
    }
}

#[cfg(all(test, feature = "native-text", feature = "bundled-fonts"))]
mod tests {
    use super::*;

    #[test]
    fn presentation_style_reuses_one_native_compilation() {
        clear_text_compiler_cache();
        let first = Text::new("shared").color(noon_core::RED);
        let second = Text::new("shared")
            .color(noon_core::BLUE)
            .shift(noon_core::Vec2::new(3.0, 2.0));
        first.compile_artifact().unwrap();
        second.compile_artifact().unwrap();
        let diagnostics = text_compiler_diagnostics();
        assert_eq!(diagnostics.successful_compiles, 1);
        assert_eq!(diagnostics.cache_hits, 1);
    }

    #[test]
    fn output_affecting_native_inputs_have_distinct_identity() {
        clear_text_compiler_cache();
        Text::new("shared")
            .with_font_size(24.0)
            .compile_artifact()
            .unwrap();
        Text::new("shared")
            .with_font_size(25.0)
            .compile_artifact()
            .unwrap();
        assert_eq!(text_compiler_diagnostics().successful_compiles, 2);
    }

    #[test]
    fn failed_compiles_are_not_cached() {
        clear_text_compiler_cache();
        let invalid = Text::new("shared").with_font("not an installed Noon font");
        assert!(invalid.compile_artifact().is_err());
        assert!(invalid.compile_artifact().is_err());
        let diagnostics = text_compiler_diagnostics();
        assert_eq!(diagnostics.entries, 0);
        assert_eq!(diagnostics.failed_compiles, 2);
    }

    #[test]
    fn compiler_callbacks_reenter_without_borrowing_the_registry() {
        clear_text_compiler_cache();
        let template = (*Text::new("template").compile_artifact().unwrap()).clone();
        let key = TextCompileKey(Arc::from(&b"reentrant"[..]), Arc::from([]));
        let result = compile_cached(key.clone(), || {
            let diagnostics = text_compiler_diagnostics();
            assert_eq!(diagnostics.entries, 1);
            Text::new("nested compiler work").compile_artifact()?;
            let nested = compile_cached(key.clone(), || Ok(template.clone()))?;
            assert_eq!(nested.resource.source.as_ref(), "template");
            Ok(template)
        })
        .unwrap();
        assert_eq!(result.resource.source.as_ref(), "template");
        let diagnostics = text_compiler_diagnostics();
        assert_eq!(
            diagnostics.entries, 3,
            "same-key reentry replaces one entry"
        );
        assert_eq!(diagnostics.successful_compiles, 4);
    }

    #[test]
    fn cache_eviction_is_bounded() {
        clear_text_compiler_cache();
        for index in 0..=MAX_ENTRIES {
            Text::new(format!("entry-{index}"))
                .compile_artifact()
                .unwrap();
        }
        let diagnostics = text_compiler_diagnostics();
        assert!(diagnostics.entries <= MAX_ENTRIES);
        assert!(diagnostics.evictions >= 1);
        assert!(diagnostics.retained_bytes <= MAX_RETAINED_BYTES);
    }
}

#[cfg(feature = "latex")]
pub(crate) fn compile_latex(
    text: &crate::latex_authoring::LatexSpec,
    document: &str,
    backend_identity: Arc<str>,
    backend: &mut impl crate::LatexBackend,
) -> Result<Arc<CompiledTextArtifact>, TextAuthoringError> {
    let key = latex_key(document, &backend_identity, text.document.kind());
    let artifact = compile_cached(key, || {
        let dvi = backend
            .compile(document)
            .map_err(|error| TextAuthoringError::LatexBackend(Arc::from(error)))?;
        let names =
            noon_text::latex::required_dvi_fonts(&dvi, noon_text::latex::DviLimits::default())?;
        let mut fonts = Vec::new();
        fonts.try_reserve_exact(names.len())?;
        for name in names {
            fonts.push(
                backend
                    .font(&name)
                    .map_err(|error| TextAuthoringError::LatexBackend(Arc::from(error)))?,
            );
        }
        let backend_identity = noon_core::TextLayoutArtifact {
            backend: noon_core::TextLayoutBackend {
                kind: noon_core::TextLayoutBackendKind::Latex,
                version: backend_identity,
            },
            template_fingerprint: Arc::from(noon_text::latex_document::LATEX_TEMPLATE_VERSION),
            artifact_fingerprint: Arc::from(document),
            backend_payload_key: None,
        };
        let mut artifact = noon_text::latex::normalize_dvi(
            text.document.source(),
            text.document.kind(),
            backend_identity,
            &dvi,
            &fonts,
            noon_text::latex::DviLimits::default(),
        )?;
        recenter_latex_artifact(&mut artifact);
        Ok(CompiledTextArtifact {
            resource: Arc::new(artifact.resource),
            fonts: Arc::new(artifact.fonts),
            geometry: Arc::new(artifact.geometries),
        })
    })?;
    Ok(Arc::new(artifact))
}

#[cfg(feature = "latex")]
pub(crate) fn latex_identity(
    document: &str,
    backend_identity: &str,
    kind: noon_core::TextSourceKind,
) -> noon_core::TextCompilationIdentity {
    let key = latex_key(document, backend_identity, kind);
    noon_core::TextCompilationIdentity {
        descriptor: key.0,
        font_contents: key.1,
    }
}

#[cfg(feature = "latex")]
fn latex_key(
    document: &str,
    backend_identity: &str,
    kind: noon_core::TextSourceKind,
) -> TextCompileKey {
    let mut bytes = Vec::new();
    push_text(&mut bytes, "latex");
    push_text(&mut bytes, noon_text::latex::LATEX_DVI_BACKEND_VERSION);
    push_text(
        &mut bytes,
        noon_text::latex_document::LATEX_TEMPLATE_VERSION,
    );
    push_text(&mut bytes, backend_identity);
    push_text(&mut bytes, &format!("{kind:?}"));
    push_text(&mut bytes, document);
    TextCompileKey(bytes.into(), Arc::from([]))
}

#[cfg(feature = "latex")]
fn recenter_latex_artifact(artifact: &mut noon_text::latex::LatexDviArtifact) {
    let center = artifact.resource.bounds.center();
    let recenter = noon_core::TextAffineTransform::translation(-center.x, -center.y);
    for run in Arc::make_mut(&mut artifact.resource.runs) {
        run.transform = run.transform.then(recenter);
    }
    for vector in Arc::make_mut(&mut artifact.resource.vector_items) {
        vector.transform = vector.transform.then(recenter);
    }
    artifact.resource.bounds = noon_core::Rect::new(
        artifact.resource.bounds.min - center,
        artifact.resource.bounds.max - center,
    );
}
