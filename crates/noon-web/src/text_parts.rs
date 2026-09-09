//! Typed WASM projection of shared Rust text-part selections.
//!
//! The browser boundary exposes retained part values only. Source matching and
//! cluster/vector coverage remain Rust-owned in `TextResource`.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

use crate::{AuthoringFailure, WasmAuthoringMobjectHandle};

fn text_part_js_error(error: noon::TextPartAuthoringError) -> JsValue {
    let failure = match error {
        noon::TextPartAuthoringError::Authoring(cause) => AuthoringFailure::from(cause),
        noon::TextPartAuthoringError::NotText(_) => AuthoringFailure::new(
            "unsupported_operation",
            "text_parts.not_text",
            error,
        ),
        noon::TextPartAuthoringError::Query(noon::TextPartQueryError::InvalidSourceSpan) => {
            AuthoringFailure::new("invalid_input", "text_parts.invalid_source_span", error)
        }
        noon::TextPartAuthoringError::Query(noon::TextPartQueryError::NonContiguousClusters) => {
            AuthoringFailure::new(
                "unsupported_operation",
                "text_parts.non_contiguous_clusters",
                error,
            )
        }
        noon::TextPartAuthoringError::Query(noon::TextPartQueryError::NonContiguousVectors) => {
            AuthoringFailure::new(
                "unsupported_operation",
                "text_parts.non_contiguous_vectors",
                error,
            )
        }
    };
    crate::authoring_error::js_error(failure)
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
        self.part
            .semantic_key
            .as_deref()
            .map(ToOwned::to_owned)
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
}