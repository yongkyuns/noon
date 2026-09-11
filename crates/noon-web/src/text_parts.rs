//! Typed WASM projection of shared Rust text-part selection and styling.
//!
//! The browser boundary exposes retained part values and selector/color intent only.
//! Source matching, selector interpretation, overlap validation, cluster coverage,
//! and immutable resource replacement remain Rust-owned.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

use crate::{AuthoringFailure, WasmAuthoringMobjectHandle};

fn query_failure(error: noon::TextPartQueryError) -> AuthoringFailure {
    match error {
        noon::TextPartQueryError::InvalidSourceSpan => {
            AuthoringFailure::new("invalid_input", "text_parts.invalid_source_span", error)
        }
        noon::TextPartQueryError::NonContiguousClusters => AuthoringFailure::new(
            "unsupported_operation",
            "text_parts.non_contiguous_clusters",
            error,
        ),
        noon::TextPartQueryError::NonContiguousVectors => AuthoringFailure::new(
            "unsupported_operation",
            "text_parts.non_contiguous_vectors",
            error,
        ),
    }
}

fn text_part_failure(error: noon::TextPartAuthoringError) -> AuthoringFailure {
    match error {
        noon::TextPartAuthoringError::Authoring(cause) => AuthoringFailure::from(cause),
        noon::TextPartAuthoringError::NotText(_) => {
            AuthoringFailure::new("unsupported_operation", "text_parts.not_text", error)
        }
        noon::TextPartAuthoringError::Query(cause) => query_failure(cause),
    }
}

fn text_part_js_error(error: noon::TextPartAuthoringError) -> JsValue {
    crate::authoring_error::js_error(text_part_failure(error))
}

fn text_style_failure(error: noon::TextStyleAuthoringError) -> AuthoringFailure {
    match error {
        noon::TextStyleAuthoringError::Selection(cause) => text_part_failure(cause),
        noon::TextStyleAuthoringError::UnsupportedSourceKind { .. } => AuthoringFailure::new(
            "unsupported_operation",
            "text_style.unsupported_source_kind",
            error,
        ),
        noon::TextStyleAuthoringError::InvalidSelector(_) => {
            AuthoringFailure::new("invalid_input", "text_style.invalid_selector", error)
        }
        noon::TextStyleAuthoringError::Style(noon_core::TextSourceStyleError::Query(cause)) => {
            query_failure(cause)
        }
        noon::TextStyleAuthoringError::Style(
            noon_core::TextSourceStyleError::AmbiguousFillOverlap { .. },
        ) => AuthoringFailure::new("invalid_input", "text_style.ambiguous_fill_overlap", error),
        noon::TextStyleAuthoringError::Style(noon_core::TextSourceStyleError::SplitsCluster {
            ..
        }) => AuthoringFailure::new("unsupported_operation", "text_style.splits_cluster", error),
        noon::TextStyleAuthoringError::Style(noon_core::TextSourceStyleError::InvalidResource(
            _,
        )) => AuthoringFailure::new("unclassified", "text_style.invalid_resource", error),
        noon::TextStyleAuthoringError::Import(_) => {
            AuthoringFailure::new("unclassified", "text_style.import", error)
        }
        noon::TextStyleAuthoringError::Font(_) => {
            AuthoringFailure::new("unclassified", "text_style.font", error)
        }
    }
}

fn text_style_js_error(error: noon::TextStyleAuthoringError) -> JsValue {
    crate::authoring_error::js_error(text_style_failure(error))
}

fn text_color(red: f64, green: f64, blue: f64, alpha: f64) -> Result<noon::Color, JsValue> {
    if ![red, green, blue, alpha]
        .iter()
        .all(|component| component.is_finite() && (0.0..=1.0).contains(component))
    {
        return Err(crate::authoring_error::js_error(AuthoringFailure::new(
            "invalid_input",
            "text_style.invalid_color",
            "text style color components must be finite and between zero and one",
        )));
    }
    Ok(noon::Color::rgba(
        red as f32,
        green as f32,
        blue as f32,
        alpha as f32,
    ))
}

/// One stable source part selected from a semantic text object.
#[wasm_bindgen]
pub struct WasmTextPart {
    part: noon::TextPart,
}

impl WasmTextPart {
    fn new(part: noon::TextPart) -> Self {
        Self { part }
    }
}

#[wasm_bindgen]
impl WasmTextPart {
    #[wasm_bindgen(getter, js_name = sourceStart)]
    pub fn source_start(&self) -> u32 {
        self.part.source_span.start
    }

    #[wasm_bindgen(getter, js_name = sourceEnd)]
    pub fn source_end(&self) -> u32 {
        self.part.source_span.end
    }

    #[wasm_bindgen(getter, js_name = firstCluster)]
    pub fn first_cluster(&self) -> u32 {
        self.part.first_cluster
    }

    #[wasm_bindgen(getter, js_name = clusterCount)]
    pub fn cluster_count(&self) -> u32 {
        self.part.cluster_count
    }

    #[wasm_bindgen(getter, js_name = firstVector)]
    pub fn first_vector(&self) -> u32 {
        self.part.first_vector
    }

    #[wasm_bindgen(getter, js_name = vectorCount)]
    pub fn vector_count(&self) -> u32 {
        self.part.vector_count
    }

    #[wasm_bindgen(getter, js_name = semanticKey)]
    pub fn semantic_key(&self) -> Option<String> {
        self.part.semantic_key.as_deref().map(ToOwned::to_owned)
    }
}

/// Bounded typed result for one substring query.
#[wasm_bindgen]
pub struct WasmTextPartList {
    parts: Vec<noon::TextPart>,
}

impl WasmTextPartList {
    fn new(parts: Vec<noon::TextPart>) -> Self {
        Self { parts }
    }
}

#[wasm_bindgen]
impl WasmTextPartList {
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> usize {
        self.parts.len()
    }

    pub fn item(&self, index: usize) -> Result<WasmTextPart, JsValue> {
        self.parts
            .get(index)
            .cloned()
            .map(WasmTextPart::new)
            .ok_or_else(|| JsValue::from_str("text part index is out of bounds"))
    }
}

/// Typed browser-side builder for one complete Manim `t2c` selector batch.
///
/// The builder stores only source selector strings and colors. Rust resolves every
/// selector and validates the complete overlap set before publishing a replacement
/// immutable text resource.
#[wasm_bindgen]
pub struct WasmTextSourceFillBatch {
    selectors: Vec<(String, noon::Color)>,
}

#[wasm_bindgen]
impl WasmTextSourceFillBatch {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            selectors: Vec::new(),
        }
    }

    #[wasm_bindgen(js_name = appendSelector)]
    pub fn append_selector(
        &mut self,
        selector: &str,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), JsValue> {
        let color = text_color(red, green, blue, alpha)?;
        self.selectors.push((selector.to_owned(), color));
        Ok(())
    }

    #[wasm_bindgen(getter)]
    pub fn length(&self) -> usize {
        self.selectors.len()
    }
}

impl Default for WasmTextSourceFillBatch {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl WasmAuthoringMobjectHandle {
    /// Select authored substring occurrences through the shared retained text resource.
    #[wasm_bindgen(js_name = textSourcePartsFor)]
    pub fn text_source_parts_for(&self, needle: &str) -> Result<WasmTextPartList, JsValue> {
        self.semantic_mobject()
            .text_source_parts_for(needle)
            .map(WasmTextPartList::new)
            .map_err(text_part_js_error)
    }

    /// Create an inert typed source-style batch beside this semantic handle.
    #[wasm_bindgen(js_name = textSourceFillBatch)]
    pub fn text_source_fill_batch(&self) -> WasmTextSourceFillBatch {
        WasmTextSourceFillBatch::new()
    }

    /// Apply one complete source-style batch through shared Rust semantics.
    #[wasm_bindgen(js_name = applyTextSourceFills)]
    pub fn apply_text_source_fills(&self, batch: &WasmTextSourceFillBatch) -> Result<(), JsValue> {
        let mut handle = self.semantic_mobject().clone();
        handle
            .set_text_fills_for_selectors(&batch.selectors)
            .map_err(text_style_js_error)
    }
}
