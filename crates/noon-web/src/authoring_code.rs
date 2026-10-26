//! Language handles for Rust-owned code highlighting, layout and retained text.

use crate::authoring_error::{js_error, AuthoringFailure};
use crate::{
    WasmAuthoringFamilyHandle, WasmAuthoringMobjectHandle, WasmAuthoringStore, WasmNativeFontFace,
};
use std::ops::Range;
use wasm_bindgen::prelude::*;

fn invalid_option(name: &'static str, value: f64) -> JsValue {
    js_error(noon::CodeAuthoringError::InvalidOption {
        name,
        value: value.to_string(),
    })
}

fn finite_f32(name: &'static str, value: f64) -> Result<f32, JsValue> {
    if !value.is_finite() || !(value as f32).is_finite() {
        return Err(invalid_option(name, value));
    }
    Ok(value as f32)
}

fn color(red: f64, green: f64, blue: f64, alpha: f64) -> Result<noon::Color, JsValue> {
    for value in [red, green, blue, alpha] {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(invalid_option("color component", value));
        }
    }
    Ok(noon::Color::rgba(
        red as f32,
        green as f32,
        blue as f32,
        alpha as f32,
    ))
}

pub(crate) fn line_index(name: &'static str, value: &JsValue) -> Result<usize, JsValue> {
    let value = value.as_f64().ok_or_else(|| {
        js_error(noon::CodeAuthoringError::InvalidOption {
            name,
            value: "expected a nonnegative integer".into(),
        })
    })?;
    // Keep the language-boundary value until checked. A usize parameter would
    // silently truncate fractions, wrap large integers, and coerce booleans.
    if !value.is_finite() || value.fract() != 0.0 || !(0.0..=f64::from(u32::MAX)).contains(&value) {
        return Err(invalid_option(name, value));
    }
    Ok(value as usize)
}

pub(crate) fn line_range(start: &JsValue, stop: &JsValue) -> Result<Range<usize>, JsValue> {
    let start = line_index("start", start)?;
    let stop = if stop.is_null() || stop.is_undefined() {
        (start as u32)
            .checked_add(1)
            .map(|stop| stop as usize)
            .ok_or_else(|| invalid_option("stop", start as f64 + 1.0))?
    } else {
        line_index("stop", stop)?
    };
    Ok(start..stop)
}

/// Optional language arguments over the shared native line-region defaults.
#[wasm_bindgen]
pub struct WasmCodeHighlightOptions {
    pub(crate) options: noon::CodeHighlightOptions,
}

#[wasm_bindgen]
impl WasmCodeHighlightOptions {
    #[wasm_bindgen(constructor)]
    pub fn new(include_line_numbers: bool) -> Self {
        Self {
            options: noon::CodeHighlightOptions {
                include_line_numbers,
                ..Default::default()
            },
        }
    }

    #[wasm_bindgen(js_name = setPadding)]
    pub fn set_padding(&mut self, padding: f64) {
        self.options.padding = padding;
    }

    #[wasm_bindgen(js_name = setFillOpacity)]
    pub fn set_fill_opacity(&mut self, opacity: f64) {
        self.options.fill_opacity = opacity;
    }

    #[wasm_bindgen(js_name = setColor)]
    pub fn set_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), JsValue> {
        self.options.color = color(red, green, blue, alpha)?;
        Ok(())
    }
}

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
        // Preserve JS numbers until checked: integer wasm-bindgen parameters
        // would wrap or truncate invalid inputs before Rust can reject them.
        if !tab_width.is_finite()
            || tab_width.fract() != 0.0
            || !(0.0..=f64::from(u32::MAX)).contains(&tab_width)
        {
            return Err(invalid_option("tab_width", tab_width));
        }
        if !line_numbers_from.is_finite()
            || line_numbers_from.fract() != 0.0
            || !(f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&line_numbers_from)
        {
            return Err(invalid_option("line_numbers_from", line_numbers_from));
        }
        let background = match background {
            "rectangle" => noon::CodeBackground::Rectangle,
            "window" => noon::CodeBackground::Window,
            _ => {
                return Err(js_error(AuthoringFailure::new(
                    "invalid_input",
                    "code.invalid_background",
                    "Code background must be 'rectangle' or 'window'",
                )))
            }
        };
        Ok(Self {
            options: noon::CodeOptions {
                language,
                source_path,
                formatter_style,
                tab_width: tab_width as usize,
                add_line_numbers,
                line_numbers_from: line_numbers_from as i32,
                background,
                ..Default::default()
            },
        })
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
        self.options.paragraph_config = noon::CodeParagraphConfig {
            font,
            font_size: finite_f32("font_size", font_size)?,
            line_spacing: finite_f32("line_spacing", line_spacing)?,
            disable_ligatures,
            font_face: font_face.map(|input| input.face),
        };
        Ok(())
    }

    #[wasm_bindgen(js_name = setBackgroundGeometry)]
    pub fn set_background_geometry(&mut self, buff: f64, corner_radius: f64) {
        self.options.background_config.buff = buff;
        self.options.background_config.corner_radius = corner_radius;
    }

    #[wasm_bindgen(js_name = setLineNumberLayout)]
    pub fn set_line_number_layout(
        &mut self,
        font_size: Option<f64>,
        min_digits: &JsValue,
    ) -> Result<(), JsValue> {
        let min_digits = line_index("line_number_config min_digits", min_digits)?;
        if min_digits > 32 {
            return Err(invalid_option(
                "line_number_config min_digits (0..=32)",
                min_digits as f64,
            ));
        }
        let font_size = font_size
            .map(|size| finite_f32("line_number_config font_size", size))
            .transpose()?;
        self.options.line_number_config.font_size = font_size;
        self.options.line_number_config.min_digits = min_digits;
        Ok(())
    }

    #[wasm_bindgen(js_name = setLineNumberColor)]
    pub fn set_line_number_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), JsValue> {
        self.options.line_number_config.color = Some(color(red, green, blue, alpha)?);
        Ok(())
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
        self.options.background_config.fill_color = Some(color(red, green, blue, alpha)?);
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
        self.options.background_config.stroke_color = color(red, green, blue, alpha)?;
        self.options.background_config.stroke_width = width;
        self.options.background_config.stroke_opacity = opacity;
        Ok(())
    }

    #[wasm_bindgen(js_name = setReservedSource)]
    pub fn set_reserved_source(&mut self, source: String) {
        self.options.reserved_source = Some(source);
    }

    #[wasm_bindgen(js_name = setSyntaxColor)]
    pub fn set_syntax_color(
        &mut self,
        category: &str,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), JsValue> {
        let color = color(red, green, blue, alpha)?;
        let selected = match category {
            "foreground" => &mut self.options.syntax_colors.foreground,
            "comment" => &mut self.options.syntax_colors.comment,
            "keyword" => &mut self.options.syntax_colors.keyword,
            "number" => &mut self.options.syntax_colors.number,
            "string" => &mut self.options.syntax_colors.string,
            "operator" => &mut self.options.syntax_colors.operator,
            _ => {
                return Err(js_error(noon::CodeAuthoringError::InvalidOption {
                    name: "syntax_colors category",
                    value: category.to_owned(),
                }))
            }
        };
        *selected = Some(color);
        Ok(())
    }

    #[wasm_bindgen(js_name = setTokenColor)]
    pub fn set_token_color(
        &mut self,
        token: String,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), JsValue> {
        if !noon_core::is_valid_code_token_selector(&token) {
            return Err(js_error(noon::CodeAuthoringError::InvalidOption {
                name: "token_colors selector",
                value: token,
            }));
        }
        self.options
            .token_colors
            .insert(token, color(red, green, blue, alpha)?);
        Ok(())
    }
}

#[wasm_bindgen]
pub struct WasmCodeHandle {
    pub(crate) code: noon::Code,
}

#[wasm_bindgen]
pub struct WasmCodeLineReplacement {
    pub(crate) replacement: noon::CodeLineReplacement,
}

impl WasmCodeLineReplacement {
    pub(crate) fn new(replacement: noon::CodeLineReplacement) -> Self {
        Self { replacement }
    }
}

#[wasm_bindgen]
impl WasmCodeLineReplacement {
    pub fn previous(&self) -> WasmAuthoringMobjectHandle {
        WasmAuthoringMobjectHandle::from_semantic_mobject(self.replacement.previous.clone())
    }

    pub fn current(&self) -> WasmAuthoringMobjectHandle {
        WasmAuthoringMobjectHandle::from_semantic_mobject(self.replacement.current.clone())
    }
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

    pub fn highlights(&self) -> WasmAuthoringFamilyHandle {
        WasmAuthoringFamilyHandle::from_semantic_family(self.code.highlights().clone())
    }

    #[wasm_bindgen(js_name = replaceLine)]
    pub fn replace_line(
        &self,
        index: JsValue,
        source: &str,
    ) -> Result<WasmCodeLineReplacement, JsValue> {
        self.code
            .replace_line(line_index("index", &index)?, source)
            .map(WasmCodeLineReplacement::new)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = getHighlightedLines)]
    pub fn get_highlighted_lines(
        &self,
        start: JsValue,
        stop: JsValue,
        options: WasmCodeHighlightOptions,
    ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        self.code
            .get_highlighted_lines(line_range(&start, &stop)?, options.options)
            .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = highlightLines)]
    pub fn highlight_lines(
        &self,
        start: JsValue,
        stop: JsValue,
        options: WasmCodeHighlightOptions,
    ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        self.code
            .highlight_lines(line_range(&start, &stop)?, options.options)
            .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(js_error)
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
    crate::WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}
