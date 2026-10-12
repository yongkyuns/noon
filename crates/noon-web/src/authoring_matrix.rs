//! Thin WASM handles for shared retained Matrix families.

use crate::authoring_composite::entry_handle;

use crate::{authoring_error::js_error, WasmAuthoringFamilyHandle, WasmAuthoringMobjectHandle};
use wasm_bindgen::prelude::*;

/// Shared Manim LinearTransformationScene default for matrix-induced arcs.
#[wasm_bindgen(js_name = noonLinearTransformationPathArc)]
pub fn linear_transformation_path_arc(
    values: Vec<f64>,
    rows: u32,
    columns: u32,
) -> Result<f64, JsValue> {
    noon::linear_transformation_path_arc(&values, rows as usize, columns as usize).map_err(js_error)
}

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
impl WasmAuthoringFamilyHandle {
    /// Apply a matrix to the detached target family using the shared retained
    /// path-replacement transaction. Running scenes use `liveApplyMatrixFamily`.
    #[wasm_bindgen(js_name = applyMatrix)]
    pub fn apply_matrix(
        &self,
        values: Vec<f64>,
        rows: u32,
        columns: u32,
        about_x: f64,
        about_y: f64,
    ) -> Result<(), JsValue> {
        self.semantic_family()?
            .apply_matrix(&values, rows as usize, columns as usize, about_x, about_y)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = asMatrix)]
    pub fn as_matrix(&self) -> Result<WasmMatrixHandle, JsValue> {
        self.semantic_family()
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
    /// Read column roots without publishing temporary alias families.
    pub fn columns(&self) -> Result<js_sys::Array, JsValue> {
        let result = js_sys::Array::new();
        for column in self.matrix.columns().map_err(js_error)? {
            let entries = js_sys::Array::new();
            for entry in column {
                entries.push(&entry_handle(entry));
            }
            result.push(&entries);
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
