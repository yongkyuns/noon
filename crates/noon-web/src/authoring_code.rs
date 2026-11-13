//! Language handles for Rust-owned code highlighting, layout and retained text.

use crate::authoring_error::js_error;
use crate::{
    WasmAuthoringFamilyHandle, WasmAuthoringMobjectHandle, WasmAuthoringStore, WasmNativeFontFace,
};
use noon::integration::code_input;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WasmCodeOptions {
    pub(crate) options: noon::CodeOptions,
}

#[wasm_bindgen]
impl WasmCodeOptions {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        language: Option<String>,
        source_path: Option<String>,
        formatter_style: String,
        tab_width: f64,
        add_line_numbers: bool,
        line_numbers_from: f64,
        background: &str,
    ) -> Result<Self, JsValue> {
        code_input::options(
            language,
            source_path,
            formatter_style,
            tab_width,
            add_line_numbers,
            line_numbers_from,
            background,
        )
        .map(|options| Self { options })
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = setParagraph)]
    pub fn set_paragraph(
        &mut self,
        font: String,
        font_size: f64,
        line_spacing: f64,
        disable_ligatures: bool,
        font_face: Option<WasmNativeFontFace>,
    ) -> Result<(), JsValue> {
        self.options.paragraph_config = code_input::paragraph(
            font,
            font_size,
            line_spacing,
            disable_ligatures,
            font_face.map(|input| input.face),
        )
        .map_err(js_error)?;
        Ok(())
    }

    #[wasm_bindgen(js_name = setBackgroundGeometry)]
    pub fn set_background_geometry(&mut self, buff: f64, corner_radius: f64) {
        self.options.background_config.buff = buff;
        self.options.background_config.corner_radius = corner_radius;
    }

    #[wasm_bindgen(js_name = setBackgroundOpacity)]
    pub fn set_background_opacity(&mut self, opacity: f64) {
        self.options.background_config.fill_opacity = opacity;
    }

    #[wasm_bindgen(js_name = setBackgroundFill)]
    pub fn set_background_fill(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), JsValue> {
        self.options.background_config.fill_color =
            Some(code_input::color(red, green, blue, alpha).map_err(js_error)?);
        Ok(())
    }

    #[wasm_bindgen(js_name = setBackgroundStroke)]
    #[allow(clippy::too_many_arguments)]
    pub fn set_background_stroke(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        width: f64,
        opacity: f64,
    ) -> Result<(), JsValue> {
        self.options.background_config.stroke_color =
            code_input::color(red, green, blue, alpha).map_err(js_error)?;
        self.options.background_config.stroke_width = width;
        self.options.background_config.stroke_opacity = opacity;
        Ok(())
    }
}

#[wasm_bindgen]
pub struct WasmCodeHandle {
    code: noon::Code,
}

impl WasmCodeHandle {
    pub(crate) fn new(code: noon::Code) -> Self {
        Self { code }
    }
}

#[wasm_bindgen]
impl WasmAuthoringStore {
    #[wasm_bindgen(js_name = createCode)]
    pub fn create_code(
        &self,
        source: &str,
        options: WasmCodeOptions,
    ) -> Result<WasmCodeHandle, JsValue> {
        noon::Code::in_store(self.semantics.clone(), source, options.options)
            .map(WasmCodeHandle::new)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
impl WasmAuthoringFamilyHandle {
    #[wasm_bindgen(js_name = asCode)]
    pub fn as_code(&self) -> Result<WasmCodeHandle, JsValue> {
        noon::Code::from_family(self.semantic_family()?)
            .map(WasmCodeHandle::new)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
impl WasmCodeHandle {
    pub fn family(&self) -> WasmAuthoringFamilyHandle {
        WasmAuthoringFamilyHandle::from_semantic_family(self.code.family().clone())
    }

    #[wasm_bindgen(js_name = codeLines)]
    pub fn code_lines(&self) -> WasmAuthoringFamilyHandle {
        WasmAuthoringFamilyHandle::from_semantic_family(self.code.code_lines().clone())
    }

    #[wasm_bindgen(js_name = lineNumbers)]
    pub fn line_numbers(&self) -> Option<WasmAuthoringFamilyHandle> {
        self.code
            .line_numbers()
            .cloned()
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
    }

    pub fn background(&self) -> WasmAuthoringFamilyHandle {
        WasmAuthoringFamilyHandle::from_semantic_family(self.code.background().clone())
    }

    #[wasm_bindgen(js_name = lineAt)]
    pub fn line_at(&self, index: usize) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        self.code
            .line(index)
            .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = lineSource)]
    pub fn line_source(&self, index: usize) -> Result<String, JsValue> {
        self.code
            .line_source(index)
            .map(|source| source.to_string())
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = lineSources)]
    pub fn line_sources(&self) -> Result<Vec<String>, JsValue> {
        self.code
            .line_sources()
            .map(|sources| sources.iter().map(ToString::to_string).collect())
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = lineNumberSources)]
    pub fn line_number_sources(&self) -> Result<Vec<String>, JsValue> {
        self.code
            .line_number_sources()
            .map(|sources| sources.iter().map(ToString::to_string).collect())
            .map_err(js_error)
    }
}

#[wasm_bindgen(js_name = noonCodeStylesList)]
pub fn code_styles_list() -> Vec<String> {
    noon::Code::get_styles_list()
}

#[wasm_bindgen(js_name = noonCodeLanguagesList)]
pub fn code_languages_list() -> Vec<String> {
    noon::Code::get_languages_list()
}

/// The paired Rust example runs in the same WASM context as its renderer.
#[cfg(all(feature = "renderer", feature = "renderer-smoke"))]
#[wasm_bindgen(js_name = createCodeRenderer)]
pub async fn create_code_renderer(
    canvas: web_sys::OffscreenCanvas,
) -> Result<crate::WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::code_block::session().map_err(js_error)?;
    crate::WasmExecutionCanvasRenderer::create_from_scene_execution(canvas, session).await
}
