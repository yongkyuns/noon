//! Inert world endpoints join the existing ordinary composition builder.

use super::WasmAnimationCompositionBuilder;
use crate::{authoring_error::js_error, WasmAuthoringMobjectHandle};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
impl WasmAnimationCompositionBuilder {
    #[wasm_bindgen(js_name = appendWorldTransform)]
    pub fn append_world_transform(
        &mut self,
        target: &WasmAuthoringMobjectHandle,
        values: &[f64],
        run_time: f64,
        rate_function: &str,
    ) -> Result<(), JsValue> {
        let transform = crate::authoring_spatial::world_transform_from_values(values)?;
        let rate = noon_core::RateFunction::from_semantic_id(rate_function)
            .ok_or_else(|| js_error(format!("unsupported rate function {rate_function:?}")))?;
        self.children
            .push(super::super::OrdinaryCompositionChild::WorldTransform {
                target: target.semantic_mobject().clone(),
                transform,
                options: noon_core::AnimationOptions::new()
                    .run_time(run_time)
                    .rate_func(rate),
            });
        Ok(())
    }
}
