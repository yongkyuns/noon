//! Numeric constructors share the current canonical live player.
use super::CanonicalAuthoringSceneContext;
use crate::{authoring_error::js_error, WasmDecimalNumberHandle};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = liveCreateDecimalNumber)]
    #[allow(clippy::too_many_arguments)]
    pub fn live_create_decimal_number(
        &mut self,
        value: f64,
        decimal_places: u32,
        include_sign: bool,
        group_with_commas: bool,
        show_ellipsis: bool,
        unit: Option<String>,
        font_size: f64,
        compiler: &mut crate::WasmLatexCompiler,
    ) -> Result<WasmDecimalNumberHandle, JsValue> {
        let font_size = crate::authoring_mobject::text_authoring_f32("font size", font_size)
            .map_err(js_error)?;
        self.inner
            .active_live_player()
            .map_err(js_error)?
            .live_create_decimal_number(
                compiler,
                value,
                noon::DecimalFormat {
                    decimal_places,
                    include_sign,
                    group_with_commas,
                    show_ellipsis,
                    unit,
                },
                font_size,
            )
            .map(WasmDecimalNumberHandle::from_number)
            .map_err(js_error)
    }
}
