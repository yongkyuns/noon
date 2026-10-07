//! Optional language-boundary values, not a second effect store or validator.
use crate::AuthoringFailure;
use noon::effects::{Glow, GlowParameterError, GlowRadius, GlowSource, GlowUpdate};
use noon::Color;

pub(crate) fn glow_update(
    color: &[f64],
    radius: Option<f64>,
    pixels: bool,
    intensity: Option<f64>,
    source: Option<&str>,
) -> Result<GlowUpdate, AuthoringFailure> {
    let color = match color {
        [] => None,
        [r, g, b, a]
            if color
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v)) =>
        {
            // Check before narrowing so an out-of-range f64 cannot round into
            // an accepted f32. The complete schema is validated in shared Rust.
            Some(Color::rgba(*r as f32, *g as f32, *b as f32, *a as f32))
        }
        _ => return Err(noon::AuthoringError::from(GlowParameterError::InvalidColor).into()),
    };
    let source = match source {
        None => None,
        Some("painted") => Some(GlowSource::Painted),
        Some("silhouette") => Some(GlowSource::Silhouette),
        Some(_) => {
            return Err(AuthoringFailure::new(
                "invalid_input",
                "effect.invalid_source",
                "glow source must be painted or silhouette",
            ))
        }
    };
    if pixels && radius.is_none() {
        return Err(AuthoringFailure::new(
            "invalid_input",
            "effect.radius_unit_without_value",
            "a radius unit requires an explicit radius",
        ));
    }
    let update = GlowUpdate {
        color,
        radius: radius.map(|v| {
            if pixels {
                GlowRadius::Pixels(v)
            } else {
                GlowRadius::Scene(v)
            }
        }),
        intensity,
        source,
    };
    Glow::new(update).map_err(noon::AuthoringError::from)?;
    Ok(update)
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use crate::{authoring_error::js_error, WasmAuthoringMobjectHandle};
    use noon::effects::{EffectDefinition, EffectHandle};
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    pub struct WasmGlowUpdate {
        pub(crate) value: GlowUpdate,
    }

    #[wasm_bindgen]
    impl WasmGlowUpdate {
        #[wasm_bindgen(constructor)]
        pub fn new(
            color: &[f64],
            radius: Option<f64>,
            pixels: bool,
            intensity: Option<f64>,
            source: Option<String>,
        ) -> Result<WasmGlowUpdate, JsValue> {
            super::glow_update(color, radius, pixels, intensity, source.as_deref())
                .map(|value| Self { value })
                .map_err(js_error)
        }
    }

    /// Immutable definition only; constructor allocates no scene/GPU identity.
    #[wasm_bindgen]
    pub struct WasmGlow {
        value: Glow,
    }
    impl WasmGlow {
        pub(crate) fn definition(&self) -> EffectDefinition {
            self.value.into()
        }
    }

    #[wasm_bindgen]
    impl WasmGlow {
        #[wasm_bindgen(constructor)]
        pub fn new(update: &WasmGlowUpdate) -> Result<WasmGlow, JsValue> {
            Glow::new(update.value)
                .map(|value| Self { value })
                .map_err(noon::AuthoringError::from)
                .map_err(js_error)
        }
        #[wasm_bindgen(getter)]
        pub fn red(&self) -> f64 {
            f64::from(self.value.color().red)
        }
        #[wasm_bindgen(getter)]
        pub fn green(&self) -> f64 {
            f64::from(self.value.color().green)
        }
        #[wasm_bindgen(getter)]
        pub fn blue(&self) -> f64 {
            f64::from(self.value.color().blue)
        }
        #[wasm_bindgen(getter)]
        pub fn alpha(&self) -> f64 {
            f64::from(self.value.color().alpha)
        }
        #[wasm_bindgen(getter)]
        pub fn radius(&self) -> f64 {
            self.value.radius().value()
        }
        #[wasm_bindgen(getter)]
        pub fn pixels(&self) -> bool {
            matches!(self.value.radius(), GlowRadius::Pixels(_))
        }
        #[wasm_bindgen(getter)]
        pub fn intensity(&self) -> f64 {
            self.value.intensity()
        }
        #[wasm_bindgen(getter)]
        pub fn source(&self) -> String {
            match self.value.source() {
                GlowSource::Painted => "painted",
                GlowSource::Silhouette => "silhouette",
            }
            .into()
        }
    }

    #[wasm_bindgen]
    pub struct WasmEffectHandle {
        pub(crate) handle: EffectHandle,
    }

    #[wasm_bindgen]
    impl WasmEffectHandle {
        #[wasm_bindgen(js_name = authoredDefinition)]
        pub fn authored_definition(&self) -> Result<WasmGlow, JsValue> {
            let EffectDefinition::Glow(value) =
                self.handle.authored_definition().map_err(js_error)?;
            Ok(WasmGlow { value })
        }
    }

    // An alias is the same Rust object identity, never a copied appearance.
    #[wasm_bindgen]
    impl WasmAuthoringMobjectHandle {
        #[wasm_bindgen(js_name = setGlow)]
        pub fn set_glow(&self, update: &WasmGlowUpdate) -> Result<(), JsValue> {
            self.semantic_mobject()
                .clone()
                .set_glow(update.value)
                .map_err(js_error)
        }
        #[wasm_bindgen(js_name = addEffect)]
        pub fn add_effect(&self, definition: &WasmGlow, name: &str) -> Result<(), JsValue> {
            self.semantic_mobject()
                .clone()
                .add_effect(definition.definition(), name)
                .map_err(js_error)
        }
        #[wasm_bindgen(js_name = getEffect)]
        pub fn get_effect(&self, name: &str) -> Result<WasmEffectHandle, JsValue> {
            self.semantic_mobject()
                .get_effect(name)
                .map(|handle| WasmEffectHandle { handle })
                .map_err(js_error)
        }
        #[wasm_bindgen(js_name = setEffect)]
        pub fn set_effect(&self, name: &str, update: &WasmGlowUpdate) -> Result<(), JsValue> {
            self.semantic_mobject()
                .clone()
                .set_effect(name, update.value)
                .map_err(js_error)
        }
        #[wasm_bindgen(js_name = setEffectHandle)]
        pub fn set_effect_handle(
            &self,
            effect: &WasmEffectHandle,
            update: &WasmGlowUpdate,
        ) -> Result<(), JsValue> {
            self.semantic_mobject()
                .clone()
                .set_effect(&effect.handle, update.value)
                .map_err(js_error)
        }
        #[wasm_bindgen(js_name = removeEffect)]
        pub fn remove_effect(&self, name: &str) -> Result<(), JsValue> {
            self.semantic_mobject()
                .clone()
                .remove_effect(name)
                .map_err(js_error)
        }
        #[wasm_bindgen(js_name = removeEffectHandle)]
        pub fn remove_effect_handle(&self, effect: &WasmEffectHandle) -> Result<(), JsValue> {
            self.semantic_mobject()
                .clone()
                .remove_effect(&effect.handle)
                .map_err(js_error)
        }
        #[wasm_bindgen(js_name = removeGlow)]
        pub fn remove_glow(&self) -> Result<(), JsValue> {
            self.semantic_mobject()
                .clone()
                .remove_glow()
                .map_err(js_error)
        }
    }
}
#[cfg(target_arch = "wasm32")]
pub use wasm::*;
#[cfg(test)]
mod tests;
