//! Scene-owned Brace geometry and label publication.
use super::wasm::CanonicalAuthoringSceneContext;
use crate::{
    authoring_error::js_error, authoring_mobject::WasmLayoutAnchor, WasmBraceLabelHandle,
    WasmManimGeometryOptions,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = braceGeometryOptions)]
    pub fn brace_geometry_options(
        &mut self,
        target: &WasmLayoutAnchor,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
        sharpness: f64,
    ) -> Result<WasmManimGeometryOptions, JsValue> {
        self.inner
            .local_authoring_scene()
            .map_err(js_error)?
            .brace_geometry_options(
                &target.anchor,
                noon::BraceOptions {
                    direction: (direction_x, direction_y),
                    buff,
                    sharpness,
                    ..Default::default()
                },
            )
            .map(WasmManimGeometryOptions::from_options)
            .map_err(|error| {
                js_error(crate::AuthoringFailure::unclassified(
                    "brace.prepare",
                    &error,
                ))
            })
    }

    /// Create the retained Brace/label relationship through the owning Scene.
    #[wasm_bindgen(js_name = createBraceLabel)]
    pub fn create_brace_label(
        &mut self,
        target: &WasmLayoutAnchor,
        label: &WasmLayoutAnchor,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
        sharpness: f64,
        label_buff: f64,
    ) -> Result<WasmBraceLabelHandle, JsValue> {
        noon::BraceLabel::new(
            self.inner.local_authoring_scene().map_err(js_error)?,
            &target.anchor,
            label.anchor.clone(),
            noon::BraceOptions {
                direction: (direction_x, direction_y),
                buff,
                sharpness,
                label_buff,
            },
        )
        .map(|inner| WasmBraceLabelHandle { inner })
        .map_err(|error| {
            js_error(crate::AuthoringFailure::unclassified(
                "brace.create",
                &error,
            ))
        })
    }

    #[wasm_bindgen(js_name = shiftBraceLabel)]
    pub fn shift_brace_label(
        &mut self,
        brace: &mut WasmBraceLabelHandle,
        target: &WasmLayoutAnchor,
    ) -> Result<(), JsValue> {
        brace
            .inner
            .shift_brace(
                self.inner.local_authoring_scene().map_err(js_error)?,
                &target.anchor,
            )
            .map_err(|error| js_error(crate::AuthoringFailure::unclassified("brace.shift", &error)))
    }

    #[wasm_bindgen(js_name = replaceBraceLabel)]
    pub fn replace_brace_label(
        &mut self,
        brace: &mut WasmBraceLabelHandle,
        label: &WasmLayoutAnchor,
    ) -> Result<(), JsValue> {
        brace
            .inner
            .change_label(
                self.inner.local_authoring_scene().map_err(js_error)?,
                label.anchor.clone(),
            )
            .map_err(|error| js_error(crate::AuthoringFailure::unclassified("brace.label", &error)))
    }

    #[wasm_bindgen(js_name = changeBraceLabel)]
    pub fn change_brace_label(
        &mut self,
        brace: &mut WasmBraceLabelHandle,
        target: &WasmLayoutAnchor,
        label: &WasmLayoutAnchor,
    ) -> Result<(), JsValue> {
        let code = if self.inner.requires_live_mutation_routing() {
            "brace.change"
        } else {
            "brace.replace"
        };
        brace
            .inner
            .change_brace_label(
                self.inner.local_authoring_scene().map_err(js_error)?,
                &target.anchor,
                label.anchor.clone(),
            )
            .map_err(|error| js_error(crate::AuthoringFailure::unclassified(code, &error)))
    }
}
