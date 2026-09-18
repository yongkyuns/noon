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
const NATIVE_BACKEND_VERSION: &str = "noon-native-swash-0.2.10-v1";

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
    fn compile(
        &mut self,
        key: TextCompileKey,
        compile: impl FnOnce() -> Result<CompiledTextArtifact, TextAuthoringError>,
    ) -> Result<CompiledTextArtifact, TextAuthoringError> {
        if let Some(artifact) = self.entries.get(&key).map(|entry| entry.artifact.clone()) {
            self.diagnostics.cache_hits += 1;
            self.touch(&key);
            return Ok(artifact);
        }
        self.diagnostics.cache_misses += 1;
        #[cfg(not(target_arch = "wasm32"))]
        let started = Instant::now();
        let artifact = match compile() {
            Ok(artifact) => artifact,
            Err(error) => {
                self.diagnostics.failed_compiles += 1;
                #[cfg(not(target_arch = "wasm32"))]
                {
                    self.diagnostics.compile_nanos = Some(
                        self.diagnostics
                            .compile_nanos
                            .unwrap_or(0)
                            .saturating_add(started.elapsed().as_nanos()),
                    );
                }
                return Err(error);
            }
        };
        self.diagnostics.successful_compiles += 1;
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.diagnostics.compile_nanos = Some(
                self.diagnostics
                    .compile_nanos
                    .unwrap_or(0)
                    .saturating_add(started.elapsed().as_nanos()),
            );
        }
        let retained_bytes = artifact
            .retained_bytes()
            .saturating_add(key.0.len())
            .saturating_add(key.1.iter().map(|font| font.len()).sum::<usize>());
        self.diagnostics.retained_bytes = self
            .diagnostics
            .retained_bytes
            .saturating_add(retained_bytes);
        self.entries.insert(
            key.clone(),
            CacheEntry {
                artifact: artifact.clone(),
                retained_bytes,
            },
        );
        self.lru.push_back(key);
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
    let key = native_identity(text)?;
    let artifact = REGISTRY.with(|registry| {
        registry.borrow_mut().compile(key, || {
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
        })
    })?;
    Ok(artifact)
}

#[cfg(feature = "typst")]
pub(crate) fn compile_typst(
    text: &TypstSpec,
    mode: TypstMode,
) -> Result<Arc<CompiledTextArtifact>, TextAuthoringError> {
    let key = typst_key(text, mode);
    let artifact = REGISTRY.with(|registry| {
        registry.borrow_mut().compile(key, || {
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
        })
    })?;
    Ok(artifact)
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
pub(crate) fn typst_identity(
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
    fn cache_eviction_is_bounded() {
        clear_text_compiler_cache();
        for index in 0..=MAX_ENTRIES {
            Text::new(format!("entry-{index}"))
                .compile_artifact()
                .unwrap();
        }
        let diagnostics = text_compiler_diagnostics();
        assert_eq!(diagnostics.entries, MAX_ENTRIES);
        assert_eq!(diagnostics.evictions, 1);
    }
}
