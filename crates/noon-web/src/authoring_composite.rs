//! Language-boundary conversion for shared Matrix/Table entry handles.
//! Layout and publication stay in the shared Rust semantic operations.

use crate::{authoring_error::js_error, WasmAuthoringFamilyHandle, WasmAuthoringMobjectHandle};
use wasm_bindgen::prelude::*;

/// Retained object or family roots for Matrix and Table admission.
#[wasm_bindgen]
pub struct WasmCompositeRows {
    rows: Vec<Vec<noon::CompositeEntryHandle>>,
}

#[wasm_bindgen]
impl WasmCompositeRows {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self { rows: Vec::new() }
    }

    #[wasm_bindgen(js_name = beginRow)]
    pub fn begin_row(&mut self) {
        self.rows.push(Vec::new());
    }

    #[wasm_bindgen(js_name = appendEntry)]
    pub fn append_entry(&mut self, entry: &WasmAuthoringMobjectHandle) -> Result<(), JsValue> {
        let row = self
            .rows
            .last_mut()
            .ok_or_else(|| js_error("Composite rows require beginRow before appendEntry"))?;
        row.push(noon::CompositeEntryHandle::Mobject(
            entry.semantic_mobject().clone(),
        ));
        Ok(())
    }
    #[wasm_bindgen(js_name = appendFamilyEntry)]
    pub fn append_family_entry(
        &mut self,
        entry: &WasmAuthoringFamilyHandle,
    ) -> Result<(), JsValue> {
        let row = self
            .rows
            .last_mut()
            .ok_or_else(|| js_error("Composite rows require beginRow before appendFamilyEntry"))?;
        row.push(noon::CompositeEntryHandle::Family(entry.semantic_family()?));
        Ok(())
    }
}

impl WasmCompositeRows {
    pub(crate) fn targets(&self) -> Vec<Vec<noon::MobjectTarget<'_>>> {
        self.rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(noon::CompositeEntryHandle::as_target)
                    .collect()
            })
            .collect()
    }
}

pub(crate) fn entry_handle(entry: noon::CompositeEntryHandle) -> JsValue {
    match entry {
        noon::CompositeEntryHandle::Mobject(object) => {
            WasmAuthoringMobjectHandle::from_semantic_mobject(object).into()
        }
        noon::CompositeEntryHandle::Family(family) => {
            WasmAuthoringFamilyHandle::from_semantic_family(family).into()
        }
    }
}

pub(crate) fn text_rows(rows: js_sys::Array, kind: &str) -> Result<Vec<Vec<String>>, JsValue> {
    parse_rows(rows, kind, "strings", |entry| entry.as_string())
}

pub(crate) fn number_rows(rows: js_sys::Array, kind: &str) -> Result<Vec<Vec<f64>>, JsValue> {
    parse_rows(rows, kind, "finite numbers", |entry| {
        entry.as_f64().filter(|value| value.is_finite())
    })
}

fn parse_rows<T>(
    rows: js_sys::Array,
    kind: &str,
    entry_kind: &str,
    parse: impl Fn(JsValue) -> Option<T>,
) -> Result<Vec<Vec<T>>, JsValue> {
    rows.iter()
        .map(|row| {
            let row: js_sys::Array = row
                .dyn_into()
                .map_err(|_| js_error(format!("{kind} rows must be arrays")))?;
            row.iter()
                .map(|entry| {
                    parse(entry)
                        .ok_or_else(|| js_error(format!("{kind} entries must be {entry_kind}")))
                })
                .collect()
        })
        .collect()
}
