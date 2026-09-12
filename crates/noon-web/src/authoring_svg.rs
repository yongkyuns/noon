//! SVG-specific adaptation at the optional browser authoring boundary.
//!
//! Parsing, compatibility defaults, geometry, styles and semantic publication all
//! stay in `noon`. This module only supplies existing store/family handles to that
//! shared Rust importer and rewraps authoritative SVG leaves for the host language.

use std::rc::Rc;

use wasm_bindgen::prelude::*;

use crate::{
    authoring_error::{js_error, AuthoringFailure},
    WasmAuthoringFamilyHandle, WasmAuthoringMobjectHandle, WasmAuthoringStore,
};

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
impl WasmAuthoringStore {
    /// Import static SVG source into this store as one retained semantic family.
    #[wasm_bindgen(js_name = createSvgFromString)]
    pub fn create_svg_from_string(
        &self,
        source: &str,
        should_center: bool,
        height: Option<f64>,
        width: Option<f64>,
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        noon::MobjectFamily::from_svg_str_with_options(
            Rc::clone(&self.semantics),
            source,
            noon::SvgImportOptions {
                should_center,
                height,
                width,
            },
        )
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
