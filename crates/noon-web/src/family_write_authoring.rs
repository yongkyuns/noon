use noon_core::FamilyAnimationError;

const MANIM_WRITE_LONG_FAMILY_THRESHOLD: u32 = 15;
const MANIM_WRITE_SHORT_DURATION: f64 = 1.0;
const MANIM_WRITE_LONG_DURATION: f64 = 2.0;
const MANIM_WRITE_MAX_LAG_RATIO: f64 = 0.2;
const MANIM_WRITE_LAG_NUMERATOR: f64 = 4.0;

/// Resolve ManimCE v0.21 Write's family-length-dependent defaults without exposing
/// retained member identity to a frontend adapter.
fn write_duration(member_count: u32, override_duration: Option<f64>) -> f64 {
    override_duration.unwrap_or_else(|| {
        if member_count < MANIM_WRITE_LONG_FAMILY_THRESHOLD {
            MANIM_WRITE_SHORT_DURATION
        } else {
            MANIM_WRITE_LONG_DURATION
        }
    })
}

fn write_lag_ratio(member_count: u32, override_lag_ratio: Option<f64>) -> f64 {
    override_lag_ratio.unwrap_or_else(|| {
        let denominator = f64::from(member_count.max(1));
        (MANIM_WRITE_LAG_NUMERATOR / denominator).min(MANIM_WRITE_MAX_LAG_RATIO)
    })
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use noon_core::{
        plain_text_animation_members, FamilyAnimationLeafBinding, FamilyAnimationMode,
        FamilyAnimationRequest, FamilyAnimationSpec, ObjectId, RateFunction, SemanticNodeId,
    };
    use serde::Serialize;
    use wasm_bindgen::prelude::*;

    use crate::{
        WasmAuthoringFamilyHandle, WasmAuthoringFamilyLayout, WasmAuthoringFamilyMemberHandle,
        WasmAuthoringMobjectHandle, WasmRetainedNativeTextAuthoringHandle,
    };

    use super::{write_duration, write_lag_ratio, FamilyAnimationError};

    const MAX_JS_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

    fn js_error(message: impl ToString) -> JsValue {
        JsValue::from_str(&message.to_string())
    }

    fn object_id(value: f64) -> Result<ObjectId, JsValue> {
        if !value.is_finite() || value < 0.0 || value > MAX_JS_SAFE_INTEGER || value.fract() != 0.0
        {
            return Err(JsValue::from_str(
                "family Write object ID must be a non-negative JavaScript-safe integer",
            ));
        }
        Ok(ObjectId::new(value as u64))
    }

    fn rate_function(value: &str) -> Result<RateFunction, JsValue> {
        RateFunction::from_semantic_id(value).ok_or_else(|| {
            JsValue::from_str(&format!(
                "unsupported family Write rate function semantic ID {value:?}"
            ))
        })
    }

    fn native_text_animation_member_count(
        text: &WasmRetainedNativeTextAuthoringHandle,
    ) -> Result<u32, JsValue> {
        // Native Text already uses this canonical compiler for authoring bounds and
        // final retained materialization. Reusing it here keeps whitespace/ligature
        // semantics Rust-owned; no glyph identity crosses into Python/JavaScript.
        let mut scene = noon::RetainedScene::new();
        scene
            .add_text(
                noon::Text::new(text.source())
                    .with_font(text.font_family())
                    .with_font_size(text.font_size())
                    .with_line_spacing(text.line_spacing()),
            )
            .map_err(js_error)?;
        let object = scene
            .objects()
            .first()
            .ok_or_else(|| js_error("native Text Write measurement produced no object"))?;
        let handle = object
            .content
            .text()
            .ok_or_else(|| js_error("native Text Write measurement produced no text resource"))?;
        let resource = scene
            .texts()
            .get(handle)
            .ok_or_else(|| js_error("native Text Write measurement lost its text resource"))?;
        let count = plain_text_animation_members(resource)
            .map_err(js_error)?
            .len();
        u32::try_from(count).map_err(|_| js_error("native Text Write member count exceeds u32"))
    }

    #[derive(Serialize)]
    struct FamilyWriteRequestResult {
        request: FamilyAnimationRequest,
        run_time: f64,
        lag_ratio: f64,
    }

    /// Rust-owned builder for canonical Write/Unwrite family requests.
    ///
    /// Unlike the generic request session, timing may be omitted. The session counts
    /// concrete retained animation members while it validates authoritative semantic
    /// leaf order, then applies ManimCE v0.21's length-dependent Write defaults at
    /// `finishJson`. Frontends receive only the resolved timing and canonical request;
    /// rendered Text member identities remain inside Rust.
    #[wasm_bindgen]
    pub struct WasmFamilyWriteAnimationRequestSession {
        target: SemanticNodeId,
        layout: WasmAuthoringFamilyLayout,
        start_time: f64,
        duration_override: Option<f64>,
        lag_ratio_override: Option<f64>,
        rate_function: RateFunction,
        reverse_rate_function: bool,
        reverse_member_order: bool,
        bindings: Vec<FamilyAnimationLeafBinding>,
        member_count: u32,
    }

    #[wasm_bindgen]
    impl WasmAuthoringFamilyHandle {
        #[wasm_bindgen(js_name = familyWriteAnimationRequest)]
        #[allow(clippy::too_many_arguments)]
        pub fn family_write_animation_request(
            &self,
            start_time: f64,
            duration: Option<f64>,
            lag_ratio: Option<f64>,
            rate_function_id: &str,
            reverse_rate_function: bool,
            reverse_member_order: bool,
        ) -> Result<WasmFamilyWriteAnimationRequestSession, JsValue> {
            if !start_time.is_finite() || start_time < 0.0 {
                return Err(js_error(format!(
                    "invalid family Write start time {start_time}"
                )));
            }
            if let Some(duration) = duration {
                if !duration.is_finite() || duration <= 0.0 {
                    return Err(js_error(FamilyAnimationError::InvalidDuration(duration)));
                }
            }
            if let Some(lag_ratio) = lag_ratio {
                if !lag_ratio.is_finite() || lag_ratio < 0.0 {
                    return Err(js_error(FamilyAnimationError::InvalidLagRatio(lag_ratio)));
                }
            }

            let target = SemanticNodeId::new(self.semantic_slot(), self.semantic_generation());
            Ok(WasmFamilyWriteAnimationRequestSession {
                target,
                layout: self.layout_session()?,
                start_time,
                duration_override: duration,
                lag_ratio_override: lag_ratio,
                rate_function: rate_function(rate_function_id)?,
                reverse_rate_function,
                reverse_member_order,
                bindings: Vec::new(),
                member_count: 0,
            })
        }
    }

    #[wasm_bindgen]
    impl WasmFamilyWriteAnimationRequestSession {
        /// Bind one ordinary vector leaf. A retained geometry leaf contributes one
        /// Manim-visible family member; geometry DrawBorderThenFill realization can
        /// reuse this session once that renderer capability is enabled.
        #[wasm_bindgen(js_name = bindMobject)]
        pub fn bind_mobject(
            &mut self,
            member: &WasmAuthoringMobjectHandle,
            final_object_id: f64,
        ) -> Result<(), JsValue> {
            self.layout.include_mobject(member)?;
            let semantic_leaf =
                SemanticNodeId::new(member.semantic_slot()?, member.semantic_generation()?);
            let next_count = self
                .member_count
                .checked_add(1)
                .ok_or_else(|| js_error("family Write member count exceeds u32"))?;
            self.bindings.push(FamilyAnimationLeafBinding::new(
                semantic_leaf,
                object_id(final_object_id)?,
            ));
            self.member_count = next_count;
            Ok(())
        }

        /// Bind one native Text leaf. The canonical native compiler determines the
        /// rendered non-whitespace glyph cardinality, preserving ligature/cluster
        /// behavior without exposing glyph IDs to the frontend.
        #[wasm_bindgen(js_name = bindRetainedNativeText)]
        pub fn bind_retained_native_text(
            &mut self,
            member: &WasmAuthoringFamilyMemberHandle,
            text: &WasmRetainedNativeTextAuthoringHandle,
            final_object_id: f64,
        ) -> Result<(), JsValue> {
            self.layout.include_retained_native_text(member, text)?;
            let semantic_leaf =
                SemanticNodeId::new(member.semantic_slot(), member.semantic_generation());
            let local_count = native_text_animation_member_count(text)?;
            let next_count = self
                .member_count
                .checked_add(local_count)
                .ok_or_else(|| js_error("family Write member count exceeds u32"))?;
            self.bindings.push(FamilyAnimationLeafBinding::new(
                semantic_leaf,
                object_id(final_object_id)?,
            ));
            self.member_count = next_count;
            Ok(())
        }

        /// Finish after every authoritative semantic leaf has been accepted, resolve
        /// Manim's member-count-dependent defaults, and return one canonical request.
        #[wasm_bindgen(js_name = finishJson)]
        pub fn finish_json(&self) -> Result<String, JsValue> {
            // As in the generic family request session, any completed layout query is
            // used only as a zero-allocation completeness gate. Bounds are not part of
            // the animation request.
            let _ = self.layout.width()?;
            let run_time = write_duration(self.member_count, self.duration_override);
            let lag_ratio = write_lag_ratio(self.member_count, self.lag_ratio_override);
            let spec = FamilyAnimationSpec::new(
                FamilyAnimationMode::DrawBorderThenFill,
                self.start_time,
                run_time,
                lag_ratio,
                self.rate_function,
                self.reverse_rate_function,
                self.reverse_member_order,
            )
            .map_err(js_error)?;
            let request =
                FamilyAnimationRequest::new(self.target, self.bindings.clone(), spec)
                    .map_err(js_error)?;
            serde_json::to_string(&FamilyWriteRequestResult {
                request,
                run_time,
                lag_ratio,
            })
            .map_err(js_error)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_duration_matches_manim_family_length_threshold() {
        assert_eq!(write_duration(0, None), 1.0);
        assert_eq!(write_duration(14, None), 1.0);
        assert_eq!(write_duration(15, None), 2.0);
        assert_eq!(write_duration(200, None), 2.0);
        assert_eq!(write_duration(200, Some(3.5)), 3.5);
    }

    #[test]
    fn write_lag_ratio_matches_manim_length_formula() {
        assert_eq!(write_lag_ratio(0, None), 0.2);
        assert_eq!(write_lag_ratio(1, None), 0.2);
        assert_eq!(write_lag_ratio(14, None), 0.2);
        assert_eq!(write_lag_ratio(20, None), 0.2);
        assert_eq!(write_lag_ratio(21, None), 4.0 / 21.0);
        assert_eq!(write_lag_ratio(100, None), 0.04);
        assert_eq!(write_lag_ratio(100, Some(0.7)), 0.7);
    }
}
