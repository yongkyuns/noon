//! Thin WASM handles for shared retained Matrix families.

use crate::{authoring_error::js_error, WasmAuthoringFamilyHandle, WasmAuthoringMobjectHandle};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WasmMatrixOptions {
    pub(crate) options: noon::MatrixOptions,
}
#[wasm_bindgen]
impl WasmMatrixOptions {
    #[wasm_bindgen(constructor)]
    pub fn new(
        v_buff: f64,
        h_buff: f64,
        bracket_h_buff: f64,
        bracket_v_buff: f64,
        stretch_brackets: bool,
    ) -> Self {
        Self {
            options: noon::MatrixOptions {
                v_buff,
                h_buff,
                bracket_h_buff,
                bracket_v_buff,
                stretch_brackets,
            },
        }
    }
}

#[wasm_bindgen]
pub struct WasmMatrixHandle {
    matrix: noon::Matrix,
}
impl WasmMatrixHandle {
    pub(crate) fn new(matrix: noon::Matrix) -> Self {
        Self { matrix }
    }
}

#[wasm_bindgen]
impl WasmMatrixHandle {
    #[wasm_bindgen(js_name = family)]
    pub fn family(&self) -> WasmAuthoringFamilyHandle {
        WasmAuthoringFamilyHandle::from_semantic_family(self.matrix.family().clone())
    }
    #[wasm_bindgen(js_name = entryFamily)]
    pub fn entry_family(&self) -> WasmAuthoringFamilyHandle {
        WasmAuthoringFamilyHandle::from_semantic_family(self.matrix.entry_family().clone())
    }
    #[wasm_bindgen(js_name = entries)]
    pub fn entries(&self) -> Result<js_sys::Array, JsValue> {
        let result = js_sys::Array::new();
        for entry in self.matrix.entries().map_err(js_error)? {
            result.push(&WasmAuthoringMobjectHandle::from_semantic_mobject(entry).into());
        }
        Ok(result)
    }
    #[wasm_bindgen(js_name = leftBracket)]
    pub fn left_bracket(&self) -> WasmAuthoringMobjectHandle {
        WasmAuthoringMobjectHandle::from_semantic_mobject(self.matrix.left_bracket().clone())
    }
    #[wasm_bindgen(js_name = rightBracket)]
    pub fn right_bracket(&self) -> WasmAuthoringMobjectHandle {
        WasmAuthoringMobjectHandle::from_semantic_mobject(self.matrix.right_bracket().clone())
    }
    #[wasm_bindgen(js_name = shape)]
    pub fn shape(&self) -> Result<js_sys::Array, JsValue> {
        let (rows, columns) = self.matrix.shape().map_err(js_error)?;
        let result = js_sys::Array::new();
        result.push(&JsValue::from_f64(rows as f64));
        result.push(&JsValue::from_f64(columns as f64));
        Ok(result)
    }
    #[wasm_bindgen(js_name = rowFamilies)]
    pub fn row_families(&self) -> Result<js_sys::Array, JsValue> {
        let result = js_sys::Array::new();
        for family in self.matrix.row_families().map_err(js_error)? {
            result.push(&WasmAuthoringFamilyHandle::from_semantic_family(family).into());
        }
        Ok(result)
    }
    #[wasm_bindgen(js_name = columnFamilies)]
    pub fn column_families(&self) -> Result<js_sys::Array, JsValue> {
        let result = js_sys::Array::new();
        for family in self.matrix.column_families().map_err(js_error)? {
            result.push(&WasmAuthoringFamilyHandle::from_semantic_family(family).into());
        }
        Ok(result)
    }
}
