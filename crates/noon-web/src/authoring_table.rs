//! Thin WASM handles for retained shared Table families.

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

/// Validated existing roots for one MobjectTable admission.
#[wasm_bindgen]
pub struct WasmMobjectTableRows {
    rows: Vec<Vec<noon::CompositeEntryHandle>>,
}

#[wasm_bindgen]
impl WasmMobjectTableRows {
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
        self.rows
            .last_mut()
            .ok_or_else(|| js_error("MobjectTable rows require beginRow before appendEntry"))?
            .push(noon::CompositeEntryHandle::Mobject(
                entry.semantic_mobject().clone(),
            ));
        Ok(())
    }
    #[wasm_bindgen(js_name = appendFamilyEntry)]
    pub fn append_family_entry(
        &mut self,
        entry: &WasmAuthoringFamilyHandle,
    ) -> Result<(), JsValue> {
        self.rows
            .last_mut()
            .ok_or_else(|| js_error("MobjectTable rows require beginRow before appendFamilyEntry"))?
            .push(noon::CompositeEntryHandle::Family(entry.semantic_family()?));
        Ok(())
    }
}
impl WasmMobjectTableRows {
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

fn entry_handle(entry: noon::CompositeEntryHandle) -> JsValue {
    match entry {
        noon::CompositeEntryHandle::Mobject(object) => {
            WasmAuthoringMobjectHandle::from_semantic_mobject(object).into()
        }
        noon::CompositeEntryHandle::Family(family) => {
            WasmAuthoringFamilyHandle::from_semantic_family(family).into()
        }
    }
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
