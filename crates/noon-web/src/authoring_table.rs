//! Thin WASM handles for retained shared Table families.

use crate::authoring_composite::entry_handle;

use crate::{authoring_error::js_error, WasmAuthoringFamilyHandle, WasmAuthoringMobjectHandle};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WasmTableOptions {
    pub(crate) options: noon::TableOptions,
}

#[wasm_bindgen]
impl WasmTableOptions {
    #[wasm_bindgen(constructor)]
    pub fn new(v_buff: f64, h_buff: f64, include_outer_lines: bool) -> Self {
        Self {
            options: noon::TableOptions {
                v_buff,
                h_buff,
                include_outer_lines,
                ..Default::default()
            },
        }
    }
}

#[wasm_bindgen]
pub struct WasmTableHandle {
    table: noon::Table,
}
impl WasmTableHandle {
    pub(crate) fn new(table: noon::Table) -> Self {
        Self { table }
    }
    pub(crate) fn table(&self) -> &noon::Table {
        &self.table
    }
}

/// Direct WASM renderer counterpart of the native and Python retained Table
/// example. It exercises native text, LaTeX math, and a family-valued entry
/// through the same Rust scene builder.
#[cfg(all(feature = "renderer", feature = "renderer-smoke"))]
#[wasm_bindgen(js_name = createTableRenderer)]
pub async fn create_table_renderer(
    canvas: web_sys::OffscreenCanvas,
    compiler: &mut crate::WasmLatexCompiler,
) -> Result<crate::WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::table::session(compiler).map_err(js_error)?;
    crate::WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}

#[wasm_bindgen]
impl WasmAuthoringFamilyHandle {
    #[wasm_bindgen(js_name = asTable)]
    pub fn as_table(&self) -> Result<WasmTableHandle, JsValue> {
        noon::Table::from_family(self.semantic_family()?)
            .map(WasmTableHandle::new)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
impl WasmTableHandle {
    #[wasm_bindgen(js_name = family)]
    pub fn family(&self) -> WasmAuthoringFamilyHandle {
        WasmAuthoringFamilyHandle::from_semantic_family(self.table.family().clone())
    }
    #[wasm_bindgen(js_name = entryFamily)]
    pub fn entry_family(&self) -> WasmAuthoringFamilyHandle {
        WasmAuthoringFamilyHandle::from_semantic_family(self.table.entry_family().clone())
    }
    #[wasm_bindgen(js_name = entries)]
    pub fn entries(&self) -> Result<js_sys::Array, JsValue> {
        let result = js_sys::Array::new();
        for entry in self.table.entries().map_err(js_error)? {
            result.push(&entry_handle(entry));
        }
        Ok(result)
    }
    #[wasm_bindgen(js_name = entryAt)]
    pub fn entry_at(&self, row: usize, column: usize) -> Result<JsValue, JsValue> {
        self.table
            .entry(row, column)
            .map(entry_handle)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = columns)]
    pub fn columns(&self) -> Result<js_sys::Array, JsValue> {
        let result = js_sys::Array::new();
        for column in self.table.columns().map_err(js_error)? {
            let values = js_sys::Array::new();
            for entry in column {
                values.push(&entry_handle(entry));
            }
            result.push(&values);
        }
        Ok(result)
    }
    #[wasm_bindgen(js_name = shape)]
    pub fn shape(&self) -> Result<js_sys::Array, JsValue> {
        let (rows, columns) = self.table.shape().map_err(js_error)?;
        let result = js_sys::Array::new();
        result.push(&JsValue::from_f64(rows as f64));
        result.push(&JsValue::from_f64(columns as f64));
        Ok(result)
    }
    #[wasm_bindgen(js_name = rowFamilies)]
    pub fn row_families(&self) -> Result<js_sys::Array, JsValue> {
        let result = js_sys::Array::new();
        for family in self.table.row_families().map_err(js_error)? {
            result.push(&WasmAuthoringFamilyHandle::from_semantic_family(family).into());
        }
        Ok(result)
    }
    #[wasm_bindgen(js_name = columnFamilies)]
    pub fn column_families(&self) -> Result<js_sys::Array, JsValue> {
        let result = js_sys::Array::new();
        for family in self.table.column_families().map_err(js_error)? {
            result.push(&WasmAuthoringFamilyHandle::from_semantic_family(family).into());
        }
        Ok(result)
    }
    #[wasm_bindgen(js_name = getCell)]
    pub fn get_cell(
        &self,
        row: usize,
        column: usize,
    ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        self.table
            .get_cell(row, column)
            .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(js_error)
    }
}
