//! Thin browser host adapter for the shared real-LaTeX authoring path.
use crate::{authoring_error::js_error, WasmAuthoringMobjectHandle, WasmAuthoringStore};
use js_sys::Uint8Array;
use noon::{DviFontResource, LatexBackend, LatexFormat, MathTex, Tex};
use std::{collections::BTreeMap, rc::Rc};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    pub type JsLatexBackend;
    #[wasm_bindgen(method, getter)]
    fn identity(this: &JsLatexBackend) -> String;
    #[wasm_bindgen(method, catch)]
    fn compile(this: &JsLatexBackend, document: &str) -> Result<JsLatexOutput, JsValue>;
    #[wasm_bindgen(method, catch)]
    fn font(this: &JsLatexBackend, name: &str) -> Result<JsLatexFont, JsValue>;
    pub type JsLatexOutput;
    #[wasm_bindgen(method, getter)]
    fn dvi(this: &JsLatexOutput) -> Uint8Array;
    pub type JsLatexFont;
    #[wasm_bindgen(method, getter)]
    fn tfm(this: &JsLatexFont) -> Uint8Array;
    #[wasm_bindgen(method, getter)]
    fn ttf(this: &JsLatexFont) -> Uint8Array;
}

/// Owns an explicitly prepared host. Neither construction nor rendering fetches
/// compiler assets; callers prepare the optional backend before creating this.
#[wasm_bindgen]
pub struct WasmLatexCompiler {
    backend: JsLatexBackend,
    identity: String,
    fonts: BTreeMap<String, DviFontResource>,
}

#[wasm_bindgen]
impl WasmLatexCompiler {
    #[wasm_bindgen(constructor)]
    pub fn new(backend: JsLatexBackend) -> Result<Self, JsValue> {
        let identity = backend.identity();
        if identity.is_empty() || identity.len() > 4096 {
            return Err(JsValue::from_str("Invalid LaTeX compiler identity"));
        }
        Ok(Self {
            backend,
            identity,
            fonts: BTreeMap::new(),
        })
    }
}

fn host_error(error: JsValue) -> String {
    error.as_string().unwrap_or_else(|| format!("{error:?}"))
}

impl LatexBackend for WasmLatexCompiler {
    fn identity(&self) -> &str {
        &self.identity
    }
    fn format(&self) -> LatexFormat {
        LatexFormat::Preloaded
    }
    fn compile(&mut self, document: &str) -> Result<Vec<u8>, String> {
        let dvi = self.backend.compile(document).map_err(host_error)?.dvi();
        if dvi.length() > 4 * 1024 * 1024 {
            return Err("LaTeX DVI exceeds 4 MiB".into());
        }
        Ok(dvi.to_vec())
    }
    fn font(&mut self, name: &str) -> Result<DviFontResource, String> {
        if let Some(font) = self.fonts.get(name) {
            return Ok(font.clone());
        }
        if self.fonts.len() >= 256 {
            return Err("LaTeX font resource limit exceeded".into());
        }
        let font = self.backend.font(name).map_err(host_error)?;
        let (tfm, ttf) = (font.tfm(), font.ttf());
        if tfm.length() > 1024 * 1024 || ttf.length() > 4 * 1024 * 1024 {
            return Err("LaTeX font payload exceeds limit".into());
        }
        let font = DviFontResource::bakoma(name, &self.identity, tfm.to_vec(), ttf.to_vec())
            .map_err(|error| error.to_string())?;
        self.fonts.insert(name.to_owned(), font.clone());
        Ok(font)
    }
}

pub(crate) enum AuthoredLatex {
    Text(Tex),
    Math(MathTex),
}

#[wasm_bindgen]
pub struct WasmLatexPartsHandle {
    parts: noon::LatexParts,
}

impl WasmLatexPartsHandle {
    pub(crate) fn new(parts: noon::LatexParts) -> Self {
        Self { parts }
    }

    pub(crate) fn semantic_parts(&self) -> &noon::LatexParts {
        &self.parts
    }
}

#[wasm_bindgen]
impl WasmLatexPartsHandle {
    #[wasm_bindgen(js_name = family)]
    pub fn family(&self) -> crate::WasmAuthoringFamilyHandle {
        crate::WasmAuthoringFamilyHandle::from_semantic_family(self.parts.family().clone())
    }

    #[wasm_bindgen(js_name = rebindFamily)]
    pub fn rebind_family(
        &self,
        family: &crate::WasmAuthoringFamilyHandle,
    ) -> Result<WasmLatexPartsHandle, JsValue> {
        self.parts
            .rebind_family(family.semantic_family()?)
            .map(WasmLatexPartsHandle::new)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = members)]
    pub fn members(&self) -> Result<js_sys::Array, JsValue> {
        let current = self.parts.current_members().map_err(js_error)?;
        let members = js_sys::Array::new_with_length(current.len() as u32);
        for (index, member) in current.iter().enumerate() {
            members.set(
                index as u32,
                WasmAuthoringMobjectHandle::from_semantic_mobject(member.clone()).into(),
            );
        }
        Ok(members)
    }

    #[wasm_bindgen(js_name = sourceMemberIndicesFor)]
    pub fn source_member_indices_for(&self, needle: &str) -> Result<Vec<u32>, JsValue> {
        self.parts
            .current_member_indices_for(needle)
            .map(|indices| indices.into_iter().map(|index| index as u32).collect())
            .map_err(js_error)
    }

    /// One RGBA tuple per current member. NaN in the first channel preserves
    /// that member. The complete vector is validated and published atomically.
    #[wasm_bindgen(js_name = setMemberColors)]
    pub fn set_member_colors(&self, values: &[f64]) -> Result<(), JsValue> {
        let colors = member_colors(values)?;
        self.parts
            .set_current_member_colors(&colors)
            .map_err(js_error)
    }

    #[wasm_bindgen(getter)]
    pub fn source(&self) -> String {
        self.parts.source().to_string()
    }

    #[wasm_bindgen(getter, js_name = fontSize)]
    pub fn font_size(&self) -> Result<f64, JsValue> {
        self.parts.current_font_size().map_err(js_error)
    }
}

pub(crate) fn member_colors(values: &[f64]) -> Result<Vec<Option<noon::Color>>, JsValue> {
    if values.len() % 4 != 0 {
        return Err(JsValue::from_str(
            "member colors require complete RGBA tuples",
        ));
    }
    values
        .chunks_exact(4)
        .map(|rgba| {
            if rgba[0].is_nan() {
                Ok(None)
            } else {
                Ok(Some(noon::Color::rgba(
                    crate::authoring_mobject::text_authoring_f32("member red", rgba[0])
                        .map_err(js_error)?,
                    crate::authoring_mobject::text_authoring_f32("member green", rgba[1])
                        .map_err(js_error)?,
                    crate::authoring_mobject::text_authoring_f32("member blue", rgba[2])
                        .map_err(js_error)?,
                    crate::authoring_mobject::text_authoring_f32("member alpha", rgba[3])
                        .map_err(js_error)?,
                )))
            }
        })
        .collect()
}

#[wasm_bindgen]
pub struct WasmLatexOptions {
    pub(crate) text: AuthoredLatex,
}

#[wasm_bindgen]
impl WasmLatexOptions {
    #[wasm_bindgen(constructor)]
    pub fn new(
        source: &str,
        math: bool,
        font_size: f64,
        color: &[f64],
        opacity: f64,
    ) -> Result<Self, JsValue> {
        let number = crate::authoring_mobject::text_authoring_f32;
        let font_size = number("font size", font_size).map_err(js_error)?;
        let opacity = number("opacity", opacity).map_err(js_error)?;
        let colors = crate::authoring_mobject::gradient_colors(color).map_err(js_error)?;
        let [color] = colors.as_slice() else {
            return Err(JsValue::from_str("One LaTeX color is required"));
        };
        let text = if math {
            AuthoredLatex::Math(
                MathTex::new(source)
                    .map_err(js_error)?
                    .with_font_size(font_size)
                    .color(*color)
                    .set_opacity(opacity),
            )
        } else {
            AuthoredLatex::Text(
                Tex::new(source)
                    .map_err(js_error)?
                    .with_font_size(font_size)
                    .color(*color)
                    .set_opacity(opacity),
            )
        };
        Ok(Self { text })
    }

    /// Construct from explicit Tex or MathTex string arguments. Argument
    /// normalization, separators, and isolated-part markers stay in noon-text.
    #[wasm_bindgen(js_name = fromStrings)]
    pub fn from_strings(
        strings: Vec<String>,
        math: bool,
        font_size: f64,
        color: &[f64],
        opacity: f64,
    ) -> Result<Self, JsValue> {
        let number = crate::authoring_mobject::text_authoring_f32;
        let font_size = number("font size", font_size).map_err(js_error)?;
        let opacity = number("opacity", opacity).map_err(js_error)?;
        let colors = crate::authoring_mobject::gradient_colors(color).map_err(js_error)?;
        let [color] = colors.as_slice() else {
            return Err(JsValue::from_str("One LaTeX color is required"));
        };
        let text = if math {
            AuthoredLatex::Math(
                MathTex::from_strings(strings)
                    .map_err(js_error)?
                    .with_font_size(font_size)
                    .color(*color)
                    .set_opacity(opacity),
            )
        } else {
            AuthoredLatex::Text(
                Tex::from_strings(strings)
                    .map_err(js_error)?
                    .with_font_size(font_size)
                    .color(*color)
                    .set_opacity(opacity),
            )
        };
        Ok(Self { text })
    }
}

#[wasm_bindgen]
impl WasmAuthoringStore {
    #[wasm_bindgen(js_name = createLatex)]
    pub fn create_latex(
        &self,
        options: WasmLatexOptions,
        compiler: &mut WasmLatexCompiler,
    ) -> Result<WasmLatexPartsHandle, JsValue> {
        let store = Rc::clone(&self.semantics);
        match options.text {
            AuthoredLatex::Text(text) => noon::LatexParts::from_tex(store, text, compiler),
            AuthoredLatex::Math(text) => noon::LatexParts::from_math_tex(store, text, compiler),
        }
        .map(WasmLatexPartsHandle::new)
        .map_err(js_error)
    }
}

#[cfg(all(feature = "renderer", feature = "renderer-smoke"))]
#[wasm_bindgen(js_name = createLatexTextRenderer)]
pub async fn create_latex_text_renderer(
    canvas: web_sys::OffscreenCanvas,
    compiler: &mut WasmLatexCompiler,
) -> Result<crate::WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::latex_text::session(compiler).map_err(js_error)?;
    crate::WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}

#[cfg(all(feature = "renderer", feature = "renderer-smoke"))]
#[wasm_bindgen(js_name = createNumericDecimalRenderer)]
pub async fn create_numeric_decimal_renderer(
    canvas: web_sys::OffscreenCanvas,
    compiler: &mut WasmLatexCompiler,
) -> Result<crate::WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::numeric_decimal::session(compiler).map_err(js_error)?;
    crate::WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}
