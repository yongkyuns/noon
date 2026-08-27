#![forbid(unsafe_code)]

//! Backend-neutral text compilation and compiled-artifact caching for Noon.
//!
//! This crate sits between authoring/source backends and the retained runtime. It
//! deliberately knows nothing about Typst frames, LaTeX intermediates, native shaping
//! internals, renderer atlases, or frontend language bindings. Concrete backends compile
//! a deterministic [`TextCompileKey`] into one normalized [`CompiledTextArtifact`].

use std::{collections::HashMap, sync::Arc};

use noon_core::{
    FontResourceArena, GeometryResourceArena, TextLayoutBackend, TextResource,
    TextResourceValidationError, TextSourceKind,
};

pub const DEFAULT_TEXT_COMPILE_CACHE_MAX_ENTRIES: usize = 1_024;
pub const DEFAULT_TEXT_COMPILE_CACHE_MAX_RETAINED_BYTES: usize = 128 * 1024 * 1024;

/// Complete content identity for one backend compilation.
///
/// Presentation-only state such as object transform, color, opacity, z-order, reveal,
/// and morph progress intentionally has no place in this key. Backend adapters must put
/// every input that can change normalized layout/output into one of the fingerprint
/// fields instead of hiding it in process-global state.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextCompileKey {
    pub source_kind: TextSourceKind,
    pub source: Arc<str>,
    pub backend: TextLayoutBackend,
    pub template_fingerprint: Arc<str>,
    pub layout_fingerprint: Arc<str>,
    pub font_fingerprint: Arc<str>,
    pub config_fingerprint: Arc<str>,
}

impl TextCompileKey {
    pub fn new(
        source_kind: TextSourceKind,
        source: impl Into<Arc<str>>,
        backend: TextLayoutBackend,
    ) -> Self {
        Self {
            source_kind,
            source: source.into(),
            backend,
            template_fingerprint: Arc::from(""),
            layout_fingerprint: Arc::from(""),
            font_fingerprint: Arc::from(""),
            config_fingerprint: Arc::from(""),
        }
    }

    pub fn with_template_fingerprint(mut self, value: impl Into<Arc<str>>) -> Self {
        self.template_fingerprint = value.into();
        self
    }

    pub fn with_layout_fingerprint(mut self, value: impl Into<Arc<str>>) -> Self {
        self.layout_fingerprint = value.into();
        self
    }

    pub fn with_font_fingerprint(mut self, value: impl Into<Arc<str>>) -> Self {
        self.font_fingerprint = value.into();
        self
    }

    pub fn with_config_fingerprint(mut self, value: impl Into<Arc<str>>) -> Self {
        self.config_fingerprint = value.into();
        self
    }
}

/// Normalized immutable result shared by native Text, Typst, and real LaTeX backends.
///
/// Backend-owned compiler intermediates are intentionally absent. Geometry and exact
/// font bytes remain dependency arenas alongside the backend-neutral `TextResource`.
#[derive(Clone, Debug)]
pub struct CompiledTextArtifact {
    pub resource: Arc<TextResource>,
    pub geometry: Arc<GeometryResourceArena>,
    pub fonts: Arc<FontResourceArena>,
    pub artifact_fingerprint: Arc<str>,
    retained_bytes: usize,
}

impl CompiledTextArtifact {
    pub fn new(
        resource: TextResource,
        geometry: GeometryResourceArena,
        fonts: FontResourceArena,
        artifact_fingerprint: impl Into<Arc<str>>,
    ) -> Result<Self, TextResourceValidationError> {
        resource.validate()?;
        let artifact_fingerprint = artifact_fingerprint.into();
        let retained_bytes = resource
            .retained_bytes()
            .saturating_add(geometry.stats().retained_bytes)
            .saturating_add(fonts.stats().retained_bytes)
            .saturating_add(artifact_fingerprint.len());
        Ok(Self {
            resource: Arc::new(resource),
            geometry: Arc::new(geometry),
            fonts: Arc::new(fonts),
            artifact_fingerprint,
            retained_bytes,
        })
    }

    pub const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
}

/// Concrete backend boundary.
///
/// The associated error keeps backend diagnostics typed. Failed compiles are returned
/// directly and are never inserted into [`TextCompileCache`]. A later registry can
/// dispatch among implementations of this trait without changing cache/resource shape.
pub trait TextCompiler {
    type Error;

    fn compile(&self, key: &TextCompileKey) -> Result<CompiledTextArtifact, Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextCompileCacheLimits {
    pub max_entries: usize,
    pub max_retained_bytes: usize,
}

impl TextCompileCacheLimits {
    pub const fn new(max_entries: usize, max_retained_bytes: usize) -> Self {
        Self {
            max_entries,
            max_retained_bytes,
        }
    }
}

pub const DEFAULT_TEXT_COMPILE_CACHE_LIMITS: TextCompileCacheLimits = TextCompileCacheLimits::new(
    DEFAULT_TEXT_COMPILE_CACHE_MAX_ENTRIES,
    DEFAULT_TEXT_COMPILE_CACHE_MAX_RETAINED_BYTES,
);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextCompileCacheStats {
    pub resident_entries: usize,
    pub retained_bytes: usize,
    pub hits: u64,
    pub misses: u64,
    pub compilations: u64,
    pub compile_errors: u64,
    pub insertions: u64,
    pub evictions: u64,
    pub rejected_admissions: u64,
}

#[derive(Clone, Debug)]
struct CacheEntry {
    artifact: Arc<CompiledTextArtifact>,
    last_access: u64,
}

/// Bounded deterministic LRU cache for immutable compiled text artifacts.
///
/// LRU recency uses a strictly increasing access sequence, so eviction does not depend
/// on `HashMap` iteration order. Oversized artifacts are returned to the caller but are
/// not admitted, preventing one compile from blowing through the configured residency
/// ceiling. Cache limits may be tightened at runtime and take effect immediately.
#[derive(Clone, Debug)]
pub struct TextCompileCache {
    entries: HashMap<TextCompileKey, CacheEntry>,
    limits: TextCompileCacheLimits,
    retained_bytes: usize,
    next_access: u64,
    hits: u64,
    misses: u64,
    compilations: u64,
    compile_errors: u64,
    insertions: u64,
    evictions: u64,
    rejected_admissions: u64,
}

impl TextCompileCache {
    pub fn new() -> Self {
        Self::with_limits(DEFAULT_TEXT_COMPILE_CACHE_LIMITS)
    }

    pub fn with_limits(limits: TextCompileCacheLimits) -> Self {
        Self {
            entries: HashMap::new(),
            limits,
            retained_bytes: 0,
            next_access: 0,
            hits: 0,
            misses: 0,
            compilations: 0,
            compile_errors: 0,
            insertions: 0,
            evictions: 0,
            rejected_admissions: 0,
        }
    }

    pub const fn limits(&self) -> TextCompileCacheLimits {
        self.limits
    }

    pub fn set_limits(&mut self, limits: TextCompileCacheLimits) {
        self.limits = limits;
        self.evict_to_limits();
    }

    pub fn get(&mut self, key: &TextCompileKey) -> Option<Arc<CompiledTextArtifact>> {
        let access = self.next_access();
        let entry = self.entries.get_mut(key)?;
        entry.last_access = access;
        self.hits = self.hits.saturating_add(1);
        Some(Arc::clone(&entry.artifact))
    }

    pub fn get_or_compile<C: TextCompiler>(
        &mut self,
        compiler: &C,
        key: TextCompileKey,
    ) -> Result<Arc<CompiledTextArtifact>, C::Error> {
        if let Some(artifact) = self.get(&key) {
            return Ok(artifact);
        }

        self.misses = self.misses.saturating_add(1);
        let artifact = match compiler.compile(&key) {
            Ok(artifact) => artifact,
            Err(error) => {
                self.compile_errors = self.compile_errors.saturating_add(1);
                return Err(error);
            }
        };
        self.compilations = self.compilations.saturating_add(1);
        let artifact = Arc::new(artifact);
        self.admit(key, Arc::clone(&artifact));
        Ok(artifact)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.retained_bytes = 0;
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn stats(&self) -> TextCompileCacheStats {
        TextCompileCacheStats {
            resident_entries: self.entries.len(),
            retained_bytes: self.retained_bytes,
            hits: self.hits,
            misses: self.misses,
            compilations: self.compilations,
            compile_errors: self.compile_errors,
            insertions: self.insertions,
            evictions: self.evictions,
            rejected_admissions: self.rejected_admissions,
        }
    }

    fn admit(&mut self, key: TextCompileKey, artifact: Arc<CompiledTextArtifact>) {
        let bytes = artifact.retained_bytes();
        if self.limits.max_entries == 0 || bytes > self.limits.max_retained_bytes {
            self.rejected_admissions = self.rejected_admissions.saturating_add(1);
            return;
        }

        if let Some(previous) = self.entries.remove(&key) {
            self.retained_bytes = self
                .retained_bytes
                .saturating_sub(previous.artifact.retained_bytes());
        }
        let access = self.next_access();
        self.retained_bytes = self.retained_bytes.saturating_add(bytes);
        self.entries.insert(
            key,
            CacheEntry {
                artifact,
                last_access: access,
            },
        );
        self.insertions = self.insertions.saturating_add(1);
        self.evict_to_limits();
    }

    fn evict_to_limits(&mut self) {
        while self.entries.len() > self.limits.max_entries
            || self.retained_bytes > self.limits.max_retained_bytes
        {
            let victim = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_access)
                .map(|(key, _)| key.clone())
                .expect("over-limit text compile cache must contain an eviction candidate");
            let removed = self
                .entries
                .remove(&victim)
                .expect("selected text compile cache victim must still exist");
            self.retained_bytes = self
                .retained_bytes
                .saturating_sub(removed.artifact.retained_bytes());
            self.evictions = self.evictions.saturating_add(1);
        }
    }

    fn next_access(&mut self) -> u64 {
        let access = self.next_access;
        self.next_access = self
            .next_access
            .checked_add(1)
            .expect("text compile cache access sequence exhausted");
        access
    }
}

impl Default for TextCompileCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use noon_core::{Rect, TextLayoutBackendKind, TextResource, Vec2};

    use super::*;

    fn backend() -> TextLayoutBackend {
        TextLayoutBackend {
            kind: TextLayoutBackendKind::NativeText,
            version: Arc::from("test-v1"),
        }
    }

    fn key(source: &str) -> TextCompileKey {
        TextCompileKey::new(TextSourceKind::Plain, source, backend())
            .with_template_fingerprint("template-v1")
            .with_layout_fingerprint("layout-v1")
            .with_font_fingerprint("font-v1")
            .with_config_fingerprint("config-v1")
    }

    fn artifact(source: &str) -> CompiledTextArtifact {
        CompiledTextArtifact::new(
            TextResource {
                source: Arc::from(source),
                kind: TextSourceKind::Plain,
                runs: Arc::from([]),
                vector_items: Arc::from([]),
                render_items: Arc::from([]),
                parts: Arc::from([]),
                bounds: Rect::new(Vec2::ZERO, Vec2::ZERO),
                baseline: 0.0,
                layout_artifact: None,
            },
            GeometryResourceArena::new(),
            FontResourceArena::new(),
            format!("artifact:{source}"),
        )
        .unwrap()
    }

    #[derive(Default)]
    struct CountingCompiler {
        calls: Cell<u64>,
    }

    impl TextCompiler for CountingCompiler {
        type Error = &'static str;

        fn compile(&self, key: &TextCompileKey) -> Result<CompiledTextArtifact, Self::Error> {
            self.calls.set(self.calls.get() + 1);
            Ok(artifact(&key.source))
        }
    }

    struct FailingCompiler {
        calls: Cell<u64>,
    }

    impl TextCompiler for FailingCompiler {
        type Error = &'static str;

        fn compile(&self, _key: &TextCompileKey) -> Result<CompiledTextArtifact, Self::Error> {
            self.calls.set(self.calls.get() + 1);
            Err("compile failed")
        }
    }

    #[test]
    fn identical_content_compiles_once_and_reuses_the_same_artifact() {
        let compiler = CountingCompiler::default();
        let mut cache = TextCompileCache::new();
        let compile_key = key("hello");

        let first = cache
            .get_or_compile(&compiler, compile_key.clone())
            .unwrap();
        let second = cache.get_or_compile(&compiler, compile_key).unwrap();

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(compiler.calls.get(), 1);
        assert_eq!(cache.stats().misses, 1);
        assert_eq!(cache.stats().hits, 1);
        assert_eq!(cache.stats().resident_entries, 1);
    }

    #[test]
    fn layout_affecting_identity_fields_do_not_alias() {
        let base = key("hello");
        assert_ne!(
            base,
            base.clone().with_template_fingerprint("different-template")
        );
        assert_ne!(base, base.clone().with_font_fingerprint("different-font"));
        assert_ne!(base, key("goodbye"));
    }

    #[test]
    fn lru_eviction_preserves_the_recent_working_set() {
        let compiler = CountingCompiler::default();
        let mut cache = TextCompileCache::with_limits(TextCompileCacheLimits::new(2, usize::MAX));
        let a = key("a");
        let b = key("b");
        let c = key("c");

        cache.get_or_compile(&compiler, a.clone()).unwrap();
        cache.get_or_compile(&compiler, b.clone()).unwrap();
        assert!(cache.get(&a).is_some());
        cache.get_or_compile(&compiler, c).unwrap();

        assert!(cache.get(&a).is_some());
        assert!(cache.get(&b).is_none());
        cache.get_or_compile(&compiler, b).unwrap();
        assert_eq!(compiler.calls.get(), 4);
        assert!(cache.stats().evictions >= 2);
    }

    #[test]
    fn oversized_artifacts_are_returned_without_becoming_resident() {
        let compiler = CountingCompiler::default();
        let mut cache = TextCompileCache::with_limits(TextCompileCacheLimits::new(8, 1));
        let compile_key = key("too-large");

        cache
            .get_or_compile(&compiler, compile_key.clone())
            .unwrap();
        cache.get_or_compile(&compiler, compile_key).unwrap();

        assert_eq!(compiler.calls.get(), 2);
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.stats().rejected_admissions, 2);
    }

    #[test]
    fn failed_compiles_never_poison_the_cache() {
        let compiler = FailingCompiler {
            calls: Cell::new(0),
        };
        let mut cache = TextCompileCache::new();
        let compile_key = key("bad");

        assert!(matches!(
            cache.get_or_compile(&compiler, compile_key.clone()),
            Err("compile failed")
        ));
        assert!(matches!(
            cache.get_or_compile(&compiler, compile_key),
            Err("compile failed")
        ));
        assert_eq!(compiler.calls.get(), 2);
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.stats().compile_errors, 2);
    }

    #[test]
    fn tightening_limits_evicts_immediately() {
        let compiler = CountingCompiler::default();
        let mut cache = TextCompileCache::new();
        cache.get_or_compile(&compiler, key("a")).unwrap();
        cache.get_or_compile(&compiler, key("b")).unwrap();

        cache.set_limits(TextCompileCacheLimits::new(1, usize::MAX));
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.stats().evictions, 1);
    }
}
