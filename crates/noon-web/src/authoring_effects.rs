//! Optional language-boundary values, not a second effect store or validator.
use crate::AuthoringFailure;
use noon::effects::{EffectParameterError, EffectRadius, EffectUpdate, Glow, GlowSource};
use noon::Color;

pub(crate) fn effect_update(
    color: &[f64],
    radius: Option<f64>,
    pixels: bool,
    intensity: Option<f64>,
    source: Option<&str>,
) -> Result<EffectUpdate, AuthoringFailure> {
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
        _ => return Err(noon::AuthoringError::from(EffectParameterError::InvalidColor).into()),
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
    let update = EffectUpdate {
        color,
        radius: radius.map(|v| {
            if pixels {
                EffectRadius::Pixels(v)
            } else {
                EffectRadius::Scene(v)
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
    pub struct WasmEffectUpdate {
        pub(crate) value: EffectUpdate,
    }

    #[wasm_bindgen]
    impl WasmEffectUpdate {
        #[wasm_bindgen(constructor)]
        pub fn new(
            color: &[f64],
            radius: Option<f64>,
            pixels: bool,
            intensity: Option<f64>,
            source: Option<String>,
        ) -> Result<WasmEffectUpdate, JsValue> {
            super::effect_update(color, radius, pixels, intensity, source.as_deref())
                .map(|value| Self { value })
                .map_err(js_error)
        }
    }

    /// Immutable definition only; constructor allocates no scene/GPU identity.
    #[wasm_bindgen]
    pub struct WasmEffectDefinition {
        value: EffectDefinition,
    }
    impl WasmEffectDefinition {
        fn glow_value(&self) -> Result<Glow, JsValue> {
            self.value.as_glow().ok_or_else(|| {
                js_error(noon::AuthoringError::from(
                    EffectParameterError::UnsupportedParameter(noon_core::EffectParameter::Color),
                ))
            })
        }
        pub(crate) fn definition(&self) -> EffectDefinition {
            self.value
        }
    }

    #[wasm_bindgen]
    impl WasmEffectDefinition {
        #[wasm_bindgen(js_name = glow)]
        pub fn glow(update: &WasmEffectUpdate) -> Result<WasmEffectDefinition, JsValue> {
            Glow::new(update.value)
                .map(|value| Self {
                    value: value.into(),
                })
                .map_err(noon::AuthoringError::from)
                .map_err(js_error)
        }
        #[wasm_bindgen(js_name = blur)]
        pub fn blur(update: &WasmEffectUpdate) -> Result<WasmEffectDefinition, JsValue> {
            noon::effects::Blur::new(update.value)
                .map(|value| Self {
                    value: value.into(),
                })
                .map_err(noon::AuthoringError::from)
                .map_err(js_error)
        }
        #[wasm_bindgen(getter)]
        pub fn kind(&self) -> String {
            match self.value {
                EffectDefinition::Glow(_) => "glow",
                EffectDefinition::Blur(_) => "blur",
            }
            .into()
        }
        #[wasm_bindgen(getter)]
        pub fn red(&self) -> Result<f64, JsValue> {
            Ok(f64::from(self.glow_value()?.color().red))
        }
        #[wasm_bindgen(getter)]
        pub fn green(&self) -> Result<f64, JsValue> {
            Ok(f64::from(self.glow_value()?.color().green))
        }
        #[wasm_bindgen(getter)]
        pub fn blue(&self) -> Result<f64, JsValue> {
            Ok(f64::from(self.glow_value()?.color().blue))
        }
        #[wasm_bindgen(getter)]
        pub fn alpha(&self) -> Result<f64, JsValue> {
            Ok(f64::from(self.glow_value()?.color().alpha))
        }
        #[wasm_bindgen(getter)]
        pub fn radius(&self) -> f64 {
            self.value.radius().value()
        }
        #[wasm_bindgen(getter)]
        pub fn pixels(&self) -> bool {
            matches!(self.value.radius(), EffectRadius::Pixels(_))
        }
        #[wasm_bindgen(getter)]
        pub fn intensity(&self) -> Result<f64, JsValue> {
            Ok(self.glow_value()?.intensity())
        }
        #[wasm_bindgen(getter)]
        pub fn source(&self) -> Result<String, JsValue> {
            Ok(match self.glow_value()?.source() {
                GlowSource::Painted => "painted",
                GlowSource::Silhouette => "silhouette",
            }
            .into())
        }
    }

    #[wasm_bindgen]
    pub struct WasmEffectHandle {
        pub(crate) handle: EffectHandle,
    }

    #[wasm_bindgen]
    impl WasmEffectHandle {
        #[wasm_bindgen(js_name = authoredDefinition)]
        pub fn authored_definition(&self) -> Result<WasmEffectDefinition, JsValue> {
            let value = self.handle.authored_definition().map_err(js_error)?;
            Ok(WasmEffectDefinition { value })
        }
    }

    // An alias is the same Rust object identity, never a copied appearance.
    #[wasm_bindgen]
    impl WasmAuthoringMobjectHandle {
        #[wasm_bindgen(js_name = setGlow)]
        pub fn set_glow(&self, update: &WasmEffectUpdate) -> Result<(), JsValue> {
            self.semantic_mobject()
                .clone()
                .set_glow(update.value)
                .map_err(js_error)
        }
        #[wasm_bindgen(js_name = addEffect)]
        pub fn add_effect(
            &self,
            definition: &WasmEffectDefinition,
            name: &str,
        ) -> Result<(), JsValue> {
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
        pub fn set_effect(&self, name: &str, update: &WasmEffectUpdate) -> Result<(), JsValue> {
            self.semantic_mobject()
                .clone()
                .set_effect(name, update.value)
                .map_err(js_error)
        }
        #[wasm_bindgen(js_name = setEffectHandle)]
        pub fn set_effect_handle(
            &self,
            effect: &WasmEffectHandle,
            update: &WasmEffectUpdate,
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
