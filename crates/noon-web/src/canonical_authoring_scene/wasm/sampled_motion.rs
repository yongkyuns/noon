//! Bulk host input joins the existing ordinary composition builder.

use super::{parse_object_id, CanonicalAuthoringSceneContext, WasmAnimationCompositionBuilder};
use crate::authoring_error::js_error;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
impl WasmAnimationCompositionBuilder {
    /// Append consecutive single-target records in one real host crossing.
    /// Existing wrapper IDs are resolved only inside their canonical context.
    #[allow(clippy::too_many_arguments)]
    #[wasm_bindgen(js_name = appendSampledMotionBatch)]
    pub fn append_sampled_motion_batch(
        &mut self,
        context: &CanonicalAuthoringSceneContext,
        object_ids: Vec<String>,
        sample_ends: &[u32],
        phases: &[f64],
        centers_xyz: &[f64],
        rotation_offsets: &[f64],
        option_presence: &[u8],
        option_values: &[f64],
        rate_functions: Vec<String>,
        play_run_time: Option<f64>,
        play_rate_function: &str,
        play_lag_ratio: Option<f64>,
    ) -> Result<(), JsValue> {
        let ids = object_ids
            .iter()
            .map(|id| parse_object_id("SampledMotion object ID", id))
            .collect::<Result<Vec<_>, _>>()?;
        let children = super::super::sampled_motion::children(
            &context.inner,
            &ids,
            sample_ends,
            phases,
            centers_xyz,
            rotation_offsets,
            option_presence,
            option_values,
            &rate_functions,
            play_run_time,
            play_rate_function,
            play_lag_ratio,
        )
        .map_err(js_error)?;
        // A rejected final record cannot append an earlier valid record.
        self.children.extend(children);
        Ok(())
    }
}
