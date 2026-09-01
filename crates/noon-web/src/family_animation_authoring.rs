#[cfg(target_arch = "wasm32")]
mod wasm {
    use noon_core::{
        FamilyAnimationLeafBinding, FamilyAnimationMode, FamilyAnimationRequest,
        FamilyAnimationSpec, ObjectId, RateFunction, SemanticNodeId,
    };
    use wasm_bindgen::prelude::*;

    use crate::{
        WasmAuthoringFamilyHandle, WasmAuthoringFamilyLayout,
        WasmAuthoringFamilyMemberHandle, WasmAuthoringMobjectHandle,
        WasmRetainedNativeTextAuthoringHandle,
    };

    const MAX_JS_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

    fn js_error(message: impl ToString) -> JsValue {
        JsValue::from_str(&message.to_string())
    }

    fn object_id(value: f64) -> Result<ObjectId, JsValue> {
        if !value.is_finite()
            || value < 0.0
            || value > MAX_JS_SAFE_INTEGER
            || value.fract() != 0.0
        {
            return Err(JsValue::from_str(
                "family animation object ID must be a non-negative JavaScript-safe integer",
            ));
        }
        Ok(ObjectId::new(value as u64))
    }

    fn animation_mode(value: &str) -> Result<FamilyAnimationMode, JsValue> {
        match value {
            "reveal" => Ok(FamilyAnimationMode::Reveal),
            _ => Err(JsValue::from_str(&format!(
                "unsupported family animation mode {value:?}; public authoring currently supports reveal"
            ))),
        }
    }

    fn rate_function(value: &str) -> Result<RateFunction, JsValue> {
        RateFunction::from_semantic_id(value).ok_or_else(|| {
            JsValue::from_str(&format!(
                "unsupported family animation rate function semantic ID {value:?}"
            ))
        })
    }

    /// Rust-owned builder for one canonical semantic-family animation request.
    ///
    /// The embedded family-layout session is used only as an authoritative ordered-leaf
    /// validator. Frontends report each already-materialized leaf and final ObjectId;
    /// they never serialize or reconstruct semantic family order themselves.
    #[wasm_bindgen]
    pub struct WasmFamilyAnimationRequestSession {
        target: SemanticNodeId,
        layout: WasmAuthoringFamilyLayout,
        spec: FamilyAnimationSpec,
        bindings: Vec<FamilyAnimationLeafBinding>,
    }

    #[wasm_bindgen]
    impl WasmAuthoringFamilyHandle {
        #[wasm_bindgen(js_name = familyAnimationRequest)]
        #[allow(clippy::too_many_arguments)]
        pub fn family_animation_request(
            &self,
            mode: &str,
            start_time: f64,
            duration: f64,
            lag_ratio: f64,
            rate_function_id: &str,
            reverse_rate_function: bool,
            reverse_member_order: bool,
        ) -> Result<WasmFamilyAnimationRequestSession, JsValue> {
            let target = SemanticNodeId::new(self.semantic_slot(), self.semantic_generation());
            let spec = FamilyAnimationSpec::new(
                animation_mode(mode)?,
                start_time,
                duration,
                lag_ratio,
                rate_function(rate_function_id)?,
                reverse_rate_function,
                reverse_member_order,
            )
            .map_err(js_error)?;
            Ok(WasmFamilyAnimationRequestSession {
                target,
                layout: self.layout_session()?,
                spec,
                bindings: Vec::new(),
            })
        }
    }

    #[wasm_bindgen]
    impl WasmFamilyAnimationRequestSession {
        /// Bind one ordinary semantic Mobject leaf in authoritative family order.
        #[wasm_bindgen(js_name = bindMobject)]
        pub fn bind_mobject(
            &mut self,
            member: &WasmAuthoringMobjectHandle,
            final_object_id: f64,
        ) -> Result<(), JsValue> {
            self.layout.include_mobject(member)?;
            let semantic_leaf = SemanticNodeId::new(
                member.semantic_slot()?,
                member.semantic_generation()?,
            );
            self.bindings.push(FamilyAnimationLeafBinding::new(
                semantic_leaf,
                object_id(final_object_id)?,
            ));
            Ok(())
        }

        /// Bind one retained native Text leaf without exposing its shaped members.
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
            self.bindings.push(FamilyAnimationLeafBinding::new(
                semantic_leaf,
                object_id(final_object_id)?,
            ));
            Ok(())
        }

        /// Finish only after every authoritative semantic leaf has been accepted.
        #[wasm_bindgen(js_name = finishJson)]
        pub fn finish_json(&self) -> Result<String, JsValue> {
            // Every public layout query first checks that all authoritative leaves were
            // consumed. Use width only as a zero-allocation completion gate; bounds are
            // intentionally absent from the canonical family-animation request.
            let _ = self.layout.width()?;
            let request =
                FamilyAnimationRequest::new(self.target, self.bindings.clone(), self.spec)
                    .map_err(js_error)?;
            serde_json::to_string(&request).map_err(js_error)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;
