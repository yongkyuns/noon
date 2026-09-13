//! SVG-specific adaptation at the optional browser authoring boundary.
//!
//! Parsing, compatibility defaults, geometry, styles and semantic publication all
//! stay in `noon`. This module only supplies existing store/family handles to that
//! shared Rust importer and rewraps authoritative SVG leaves for the host language.

use std::rc::Rc;

use js_sys::Array;
use wasm_bindgen::prelude::*;

use crate::{
    authoring_error::{js_error, AuthoringFailure},
    WasmAuthoringFamilyHandle, WasmAuthoringMobjectHandle, WasmAuthoringStore,
    WasmManimGeometryOptions,
};

const SVG_DEFAULT_TRANSPORT_TAG: &str = "noon.svg-default.v1";

impl From<noon::SvgAuthoringError> for AuthoringFailure {
    fn from(error: noon::SvgAuthoringError) -> Self {
        use noon::SvgAuthoringError as E;
        let message = error.to_string();
        match error {
            E::Xml(_) => Self::new("invalid_input", "svg.invalid_xml", message),
            E::Parse(_) => Self::new("invalid_input", "svg.parse", message),
            E::Unsupported(_) => Self::new("unsupported_operation", "svg.unsupported", message),
            E::InvalidTargetDimension { .. } => {
                Self::new("invalid_input", "svg.invalid_target_dimension", message)
            }
            E::Authoring(cause) => {
                let nested = AuthoringFailure::from(cause);
                Self {
                    category: nested.category,
                    code: "svg.authoring",
                    message,
                    cause: Some(Box::new(nested)),
                }
            }
            _ => Self::new("invalid_input", "svg.authoring", message),
        }
    }
}

#[wasm_bindgen]
impl WasmManimGeometryOptions {
    /// Build the private typed transport used by the existing SVG worker binding.
    ///
    /// Generic Manim `color`/`opacity` precedence is resolved by the thin Python
    /// adapter; this boundary carries only the five effective inherited SVG paint
    /// fields plus the ordinary optional width.
    #[wasm_bindgen(js_name = svgDefaultTransport)]
    pub fn svg_default_transport(
        width: Option<f64>,
        fill_color: Option<u32>,
        fill_opacity: Option<f64>,
        stroke_color: Option<u32>,
        stroke_opacity: Option<f64>,
        stroke_width: Option<f64>,
    ) -> JsValue {
        let transport = Array::new();
        transport.push(&JsValue::from_str(SVG_DEFAULT_TRANSPORT_TAG));
        push_optional_f64(&transport, width);
        push_optional_u32(&transport, fill_color);
        push_optional_f64(&transport, fill_opacity);
        push_optional_u32(&transport, stroke_color);
        push_optional_f64(&transport, stroke_opacity);
        push_optional_f64(&transport, stroke_width);
        transport.into()
    }
}

#[wasm_bindgen]
impl WasmAuthoringStore {
    /// Import static SVG source into this store as one retained semantic family.
    #[wasm_bindgen(js_name = createSvgFromString)]
    pub fn create_svg_from_string(
        &self,
        source: &str,
        should_center: bool,
        height: Option<f64>,
        width_or_defaults: JsValue,
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let request = parse_width_or_defaults(width_or_defaults)?;
        let options = noon::SvgImportOptions {
            should_center,
            height,
            width: request.width,
        };
        let family = match request.svg_default {
            Some(svg_default) => noon::MobjectFamily::from_svg_str_with_options_and_svg_default(
                Rc::clone(&self.semantics),
                source,
                options,
                svg_default,
            ),
            None => noon::MobjectFamily::from_svg_str_with_options(
                Rc::clone(&self.semantics),
                source,
                options,
            ),
        };
        family
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
impl WasmAuthoringFamilyHandle {
    /// Rewrap one authoritative direct object member without copying geometry.
    #[wasm_bindgen(js_name = memberMobject)]
    pub fn member_mobject(&self, index: usize) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        let family = self.semantic_family()?;
        let store = Rc::clone(family.integration_store());
        let member = {
            let borrowed = store.borrow();
            borrowed
                .semantic_family_members_checked(family.node_id())
                .map_err(js_error)?
                .get(index)
                .copied()
                .ok_or_else(|| JsValue::from_str("family member index is out of bounds"))?
        };
        noon::Mobject::from_node(store, member)
            .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(js_error)
    }
}

struct SvgImportRequest {
    width: Option<f64>,
    svg_default: Option<noon::SvgDefaultStyle>,
}

fn parse_width_or_defaults(value: JsValue) -> Result<SvgImportRequest, JsValue> {
    if value.is_null() || value.is_undefined() {
        return Ok(SvgImportRequest {
            width: None,
            svg_default: None,
        });
    }
    if let Some(width) = value.as_f64() {
        return Ok(SvgImportRequest {
            width: Some(width),
            svg_default: None,
        });
    }
    if !Array::is_array(&value) {
        return Err(invalid_svg_transport("SVG width/default transport is invalid"));
    }
    let transport = Array::from(&value);
    if transport.length() != 7
        || transport.get(0).as_string().as_deref() != Some(SVG_DEFAULT_TRANSPORT_TAG)
    {
        return Err(invalid_svg_transport("SVG default transport has an invalid envelope"));
    }
    Ok(SvgImportRequest {
        width: optional_f64(&transport, 1, "width")?,
        svg_default: Some(noon::SvgDefaultStyle {
            color: None,
            opacity: None,
            fill_color: optional_color(&transport, 2, "fill_color")?,
            fill_opacity: optional_f64(&transport, 3, "fill_opacity")?,
            stroke_color: optional_color(&transport, 4, "stroke_color")?,
            stroke_opacity: optional_f64(&transport, 5, "stroke_opacity")?,
            stroke_width: optional_f64(&transport, 6, "stroke_width")?,
        }),
    })
}

fn push_optional_f64(array: &Array, value: Option<f64>) {
    array.push(&value.map_or(JsValue::NULL, JsValue::from_f64));
}

fn push_optional_u32(array: &Array, value: Option<u32>) {
    array.push(&value.map_or(JsValue::NULL, |value| JsValue::from_f64(f64::from(value))));
}

fn optional_f64(array: &Array, index: u32, name: &str) -> Result<Option<f64>, JsValue> {
    let value = array.get(index);
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    value
        .as_f64()
        .map(Some)
        .ok_or_else(|| invalid_svg_transport(&format!("SVG default {name} must be numeric")))
}

fn optional_color(array: &Array, index: u32, name: &str) -> Result<Option<noon::Color>, JsValue> {
    let Some(value) = optional_f64(array, index, name)? else {
        return Ok(None);
    };
    if value.fract() != 0.0 || !(0.0..=f64::from(0xFF_FFFF_u32)).contains(&value) {
        return Err(invalid_svg_transport(&format!(
            "SVG default {name} must be a 24-bit RGB integer"
        )));
    }
    Ok(Some(noon::Color::from_hex(value as u32)))
}

fn invalid_svg_transport(message: &str) -> JsValue {
    js_error(AuthoringFailure::new(
        "invalid_input",
        "svg.invalid_default_transport",
        message,
    ))
}
