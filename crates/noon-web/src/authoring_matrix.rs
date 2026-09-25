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

/// A validated set of existing Mobject entries for one Matrix admission.
#[wasm_bindgen]
pub struct WasmMobjectMatrixRows {
    rows: Vec<Vec<noon::CompositeEntryHandle>>,
}

#[wasm_bindgen]
impl WasmMobjectMatrixRows {
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
            .ok_or_else(|| js_error("MobjectMatrix rows require beginRow before appendEntry"))?;
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
        let row = self.rows.last_mut().ok_or_else(|| {
            js_error("MobjectMatrix rows require beginRow before appendFamilyEntry")
        })?;
        row.push(noon::CompositeEntryHandle::Family(entry.semantic_family()?));
        Ok(())
    }
}

impl WasmMobjectMatrixRows {
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
impl WasmMatrixHandle {
    pub(crate) fn new(matrix: noon::Matrix) -> Self {
        Self { matrix }
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
    #[wasm_bindgen(js_name = asMatrix)]
    pub fn as_matrix(&self) -> Result<WasmMatrixHandle, JsValue> {
        self.semantic_family()
            .map_err(|error| error)
            .and_then(|family| noon::Matrix::from_family(family).map_err(js_error))
            .map(WasmMatrixHandle::new)
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
            result.push(&entry_handle(entry));
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

#[cfg(all(feature = "renderer", feature = "renderer-smoke"))]
#[wasm_bindgen(js_name = createMatrixRenderer)]
pub async fn create_matrix_renderer(
    canvas: web_sys::OffscreenCanvas,
    compiler: &mut crate::WasmLatexCompiler,
) -> Result<crate::WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::matrix::session(compiler).map_err(js_error)?;
    crate::WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}
