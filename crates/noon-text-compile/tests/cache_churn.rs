use std::{cell::Cell, sync::Arc};

use noon_core::{
    FontResourceArena, GeometryResourceArena, Rect, TextLayoutBackend, TextLayoutBackendKind,
    TextResource, TextSourceKind, Vec2,
};
use noon_text_compile::{
    CompiledTextArtifact, TextCompileCache, TextCompileCacheLimits, TextCompileKey, TextCompiler,
};

const WORKING_SET: usize = 4;
const CHURN_ITERATIONS: usize = 1_000;

fn backend() -> TextLayoutBackend {
    TextLayoutBackend {
        kind: TextLayoutBackendKind::NativeText,
        version: Arc::from("cache-churn-test-v1"),
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
    .expect("churn fixture must be a valid normalized text artifact")
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
fn bounded_working_set_reaches_a_stable_residency_plateau() {
    let compiler = CountingCompiler::default();
    let sample_bytes = artifact("0000").retained_bytes();
    let byte_limit = sample_bytes
        .checked_mul(WORKING_SET)
        .expect("test cache byte limit must fit usize");
    let mut cache =
        TextCompileCache::with_limits(TextCompileCacheLimits::new(WORKING_SET, byte_limit));
    let keys = [key("0000"), key("0001"), key("0002"), key("0003")];
    let mut plateau_bytes = None;

    for _ in 0..CHURN_ITERATIONS {
        for compile_key in &keys {
            cache
                .get_or_compile(&compiler, compile_key.clone())
                .expect("bounded working-set compile must succeed");
        }

        let stats = cache.stats();
        assert_eq!(stats.resident_entries, WORKING_SET);
        assert!(stats.retained_bytes <= byte_limit);
        match plateau_bytes {
            Some(expected) => assert_eq!(stats.retained_bytes, expected),
            None => plateau_bytes = Some(stats.retained_bytes),
        }
    }

    let stats = cache.stats();
    assert_eq!(compiler.calls.get(), WORKING_SET as u64);
    assert_eq!(stats.compilations, WORKING_SET as u64);
    assert_eq!(stats.misses, WORKING_SET as u64);
    assert_eq!(stats.hits, (WORKING_SET * (CHURN_ITERATIONS - 1)) as u64);
    assert_eq!(stats.insertions, WORKING_SET as u64);
    assert_eq!(stats.evictions, 0);
    assert_eq!(stats.rejected_admissions, 0);
}

#[test]
fn unique_source_churn_never_exceeds_entry_or_byte_budget() {
    let compiler = CountingCompiler::default();
    let sample_bytes = artifact("0000").retained_bytes();
    let byte_limit = sample_bytes
        .checked_mul(WORKING_SET)
        .expect("test cache byte limit must fit usize");
    let mut cache =
        TextCompileCache::with_limits(TextCompileCacheLimits::new(WORKING_SET, byte_limit));

    for index in 0..CHURN_ITERATIONS {
        let source = format!("{index:04}");
        cache
            .get_or_compile(&compiler, key(&source))
            .expect("unique churn compile must succeed");

        let stats = cache.stats();
        assert!(stats.resident_entries <= WORKING_SET);
        assert!(stats.retained_bytes <= byte_limit);
        if index + 1 >= WORKING_SET {
            assert_eq!(stats.resident_entries, WORKING_SET);
            assert_eq!(stats.retained_bytes, byte_limit);
        }
    }

    let stats = cache.stats();
    assert_eq!(compiler.calls.get(), CHURN_ITERATIONS as u64);
    assert_eq!(stats.compilations, CHURN_ITERATIONS as u64);
    assert_eq!(stats.misses, CHURN_ITERATIONS as u64);
    assert_eq!(stats.hits, 0);
    assert_eq!(stats.insertions, CHURN_ITERATIONS as u64);
    assert_eq!(stats.evictions, (CHURN_ITERATIONS - WORKING_SET) as u64);
    assert_eq!(stats.rejected_admissions, 0);
}

#[test]
fn repeated_compile_failures_do_not_accumulate_resident_state() {
    let compiler = FailingCompiler {
        calls: Cell::new(0),
    };
    let mut cache =
        TextCompileCache::with_limits(TextCompileCacheLimits::new(WORKING_SET, usize::MAX));

    for index in 0..CHURN_ITERATIONS {
        let source = format!("{index:04}");
        assert!(matches!(
            cache.get_or_compile(&compiler, key(&source)),
            Err("compile failed")
        ));
        let stats = cache.stats();
        assert_eq!(stats.resident_entries, 0);
        assert_eq!(stats.retained_bytes, 0);
    }

    let stats = cache.stats();
    assert_eq!(compiler.calls.get(), CHURN_ITERATIONS as u64);
    assert_eq!(stats.misses, CHURN_ITERATIONS as u64);
    assert_eq!(stats.compile_errors, CHURN_ITERATIONS as u64);
    assert_eq!(stats.compilations, 0);
    assert_eq!(stats.insertions, 0);
    assert_eq!(stats.evictions, 0);
    assert_eq!(stats.rejected_admissions, 0);
}
