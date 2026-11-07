//! Optional language-boundary values, not a second effect store or validator.
#[cfg(any(test, target_arch = "wasm32"))]
use noon::effects::Glow;
#[cfg(target_arch = "wasm32")]
use noon::effects::{EffectParameterError, EffectRadius};
#[cfg(any(test, target_arch = "wasm32"))]
use noon::effects::{EffectUpdate, GlowSource};
#[cfg(any(test, target_arch = "wasm32"))]
use noon::Color;

#[cfg(target_arch = "wasm32")]
pub(crate) use noon::integration::effect_color;
pub(crate) use noon::integration::{effect_update, mask_parameters};

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
        #[wasm_bindgen(js_name = setThreshold)]
        pub fn set_threshold(&mut self, threshold: f64) -> Result<(), JsValue> {
            let update = self.value.clone().threshold(threshold);
            update
                .validate()
                .map_err(noon::AuthoringError::from)
                .map_err(js_error)?;
            self.value = update;
            Ok(())
        }
        #[wasm_bindgen(js_name = setSize)]
        pub fn set_size(&mut self, size: f64, pixels: bool) -> Result<(), JsValue> {
            let value = self.value.clone().size(if pixels {
                EffectRadius::Pixels(size)
            } else {
                EffectRadius::Scene(size)
            });
            value
                .validate()
                .map_err(noon::AuthoringError::from)
                .map_err(js_error)?;
            self.value = value;
            Ok(())
        }
        #[wasm_bindgen(constructor)]
        pub fn new(
            color: &[f64],
            radius: Option<f64>,
            pixels: bool,
            intensity: Option<f64>,
            source: Option<String>,
            offset: &[f64],
            offset_pixels: bool,
            progress: Option<f64>,
            softness: Option<f64>,
            seed: Option<f64>,
        ) -> Result<WasmEffectUpdate, JsValue> {
            super::effect_update(
                color,
                radius,
                pixels,
                intensity,
                source.as_deref(),
                offset,
                offset_pixels,
            )
            .and_then(|value| super::mask_parameters(value, progress, softness, seed))
            .map(|value| Self { value })
            .map_err(js_error)
        }
    }

    /// Immutable definition only; constructor allocates no scene/GPU identity.
    #[wasm_bindgen]
    pub struct WasmEffectDefinition {
        pub(super) value: EffectDefinition,
    }
    impl WasmEffectDefinition {
        fn pixelate_value(&self) -> Result<noon_core::Pixelate, JsValue> {
            self.value.as_pixelate().ok_or_else(|| {
                js_error(noon::AuthoringError::from(
                    EffectParameterError::UnsupportedParameter(noon_core::EffectParameter::Size),
                ))
            })
        }
        fn radius_value(&self) -> Result<EffectRadius, JsValue> {
            self.value.radius().ok_or_else(|| {
                js_error(noon::AuthoringError::from(
                    EffectParameterError::UnsupportedParameter(noon_core::EffectParameter::Radius),
                ))
            })
        }
        fn mask_value(&self) -> Result<noon_core::NoiseDissolve, JsValue> {
            self.value.as_noise_dissolve().ok_or_else(|| {
                js_error(noon::AuthoringError::from(
                    EffectParameterError::UnsupportedParameter(
                        noon_core::EffectParameter::Progress,
                    ),
                ))
            })
        }
        fn glow_value(&self) -> Result<Glow, JsValue> {
            self.value.as_glow().ok_or_else(|| {
                js_error(noon::AuthoringError::from(
                    EffectParameterError::UnsupportedParameter(noon_core::EffectParameter::Color),
                ))
            })
        }
        fn color_value(&self) -> Result<Color, JsValue> {
            self.value.color().ok_or_else(|| {
                js_error(noon::AuthoringError::from(
                    EffectParameterError::UnsupportedParameter(noon_core::EffectParameter::Color),
                ))
            })
        }
        fn shadow_value(&self) -> Result<noon::effects::Shadow, JsValue> {
            self.value.as_shadow().ok_or_else(|| {
                js_error(noon::AuthoringError::from(
                    EffectParameterError::UnsupportedParameter(noon_core::EffectParameter::Offset),
                ))
            })
        }
        pub(crate) fn definition(&self) -> EffectDefinition {
            self.value.clone()
        }
    }

    #[wasm_bindgen]
    impl WasmEffectDefinition {
        #[wasm_bindgen(js_name = bloom)]
        pub fn bloom(update: &WasmEffectUpdate) -> Result<WasmEffectDefinition, JsValue> {
            noon_core::Bloom::new(update.value.clone())
                .map(|value| Self {
                    value: value.into(),
                })
                .map_err(noon::AuthoringError::from)
                .map_err(js_error)
        }
        #[wasm_bindgen(getter)]
        pub fn threshold(&self) -> Result<f64, JsValue> {
            self.value
                .as_bloom()
                .map(|value| value.threshold())
                .ok_or_else(|| {
                    js_error(noon::AuthoringError::from(
                        EffectParameterError::UnsupportedParameter(
                            noon_core::EffectParameter::Threshold,
                        ),
                    ))
                })
        }
        #[wasm_bindgen(js_name = glow)]
        pub fn glow(update: &WasmEffectUpdate) -> Result<WasmEffectDefinition, JsValue> {
            Glow::new(update.value.clone())
                .map(|value| Self {
                    value: value.into(),
                })
                .map_err(noon::AuthoringError::from)
                .map_err(js_error)
        }
        #[wasm_bindgen(js_name = pixelate)]
        pub fn pixelate(update: &WasmEffectUpdate) -> Result<WasmEffectDefinition, JsValue> {
            noon_core::Pixelate::new(update.value.clone())
                .map(|value| Self {
                    value: value.into(),
                })
                .map_err(noon::AuthoringError::from)
                .map_err(js_error)
        }
        #[wasm_bindgen(getter)]
        pub fn size(&self) -> Result<f64, JsValue> {
            Ok(self.pixelate_value()?.size().value())
        }
        #[wasm_bindgen(getter, js_name = sizePixels)]
        pub fn size_pixels(&self) -> Result<bool, JsValue> {
            Ok(matches!(
                self.pixelate_value()?.size(),
                EffectRadius::Pixels(_)
            ))
        }
        #[wasm_bindgen(js_name = blur)]
        pub fn blur(update: &WasmEffectUpdate) -> Result<WasmEffectDefinition, JsValue> {
            noon::effects::Blur::new(update.value.clone())
                .map(|value| Self {
                    value: value.into(),
                })
                .map_err(noon::AuthoringError::from)
                .map_err(js_error)
        }
        #[wasm_bindgen(js_name = backdropBlur)]
        pub fn backdrop_blur(update: &WasmEffectUpdate) -> Result<WasmEffectDefinition, JsValue> {
            noon::effects::BackdropBlur::new(update.value.clone())
                .map(|value| Self {
                    value: value.into(),
                })
                .map_err(noon::AuthoringError::from)
                .map_err(js_error)
        }
        #[wasm_bindgen(js_name = shadow)]
        pub fn shadow(update: &WasmEffectUpdate) -> Result<WasmEffectDefinition, JsValue> {
            noon::effects::Shadow::new(update.value.clone())
                .map(|v| Self { value: v.into() })
                .map_err(noon::AuthoringError::from)
                .map_err(js_error)
        }
        #[wasm_bindgen(js_name=noiseDissolve)]
        pub fn noise_dissolve(update: &WasmEffectUpdate) -> Result<WasmEffectDefinition, JsValue> {
            update
                .value
                .apply_to(noon_core::NoiseDissolve::default())
                .map(|value| Self {
                    value: value.into(),
                })
                .map_err(noon::AuthoringError::from)
                .map_err(js_error)
        }
        #[wasm_bindgen(getter)]
        pub fn progress(&self) -> Result<f64, JsValue> {
            Ok(self.mask_value()?.progress())
        }
        #[wasm_bindgen(getter)]
        pub fn softness(&self) -> Result<f64, JsValue> {
            Ok(self.mask_value()?.softness())
        }
        #[wasm_bindgen(getter)]
        pub fn seed(&self) -> Result<u32, JsValue> {
            self.value
                .as_shader()
                .map_or_else(|| self.mask_value().map(|v| v.seed()), |v| Ok(v.seed()))
        }

        #[wasm_bindgen(getter, js_name = offsetX)]
        pub fn offset_x(&self) -> Result<f64, JsValue> {
            Ok(self.shadow_value()?.offset().value()[0])
        }
        #[wasm_bindgen(getter, js_name = offsetY)]
        pub fn offset_y(&self) -> Result<f64, JsValue> {
            Ok(self.shadow_value()?.offset().value()[1])
        }
        #[wasm_bindgen(getter, js_name = offsetPixels)]
        pub fn offset_pixels(&self) -> Result<bool, JsValue> {
            Ok(matches!(
                self.shadow_value()?.offset(),
                noon_core::EffectOffset::Pixels(_)
            ))
        }
        #[wasm_bindgen(getter)]
        pub fn kind(&self) -> String {
            match self.value {
                EffectDefinition::Bloom(_) => "bloom",
                EffectDefinition::Shader(_) => "shader",
                EffectDefinition::Pixelate(_) => "pixelate",
                EffectDefinition::Glow(_) => "glow",
                EffectDefinition::Blur(_) => "blur",
                EffectDefinition::BackdropBlur(_) => "backdrop_blur",
                EffectDefinition::Shadow(_) => "shadow",
                EffectDefinition::NoiseDissolve(_) => "noise_dissolve",
            }
            .into()
        }
        #[wasm_bindgen(getter)]
        pub fn red(&self) -> Result<f64, JsValue> {
            Ok(f64::from(self.color_value()?.red))
        }
        #[wasm_bindgen(getter)]
        pub fn green(&self) -> Result<f64, JsValue> {
            Ok(f64::from(self.color_value()?.green))
        }
        #[wasm_bindgen(getter)]
        pub fn blue(&self) -> Result<f64, JsValue> {
            Ok(f64::from(self.color_value()?.blue))
        }
        #[wasm_bindgen(getter)]
        pub fn alpha(&self) -> Result<f64, JsValue> {
            Ok(f64::from(self.color_value()?.alpha))
        }
        #[wasm_bindgen(getter)]
        pub fn radius(&self) -> Result<f64, JsValue> {
            Ok(self.radius_value()?.value())
        }
        #[wasm_bindgen(getter)]
        pub fn pixels(&self) -> Result<bool, JsValue> {
            Ok(matches!(self.radius_value()?, EffectRadius::Pixels(_)))
        }
        #[wasm_bindgen(getter)]
        pub fn intensity(&self) -> Result<f64, JsValue> {
            if let Some(bloom) = self.value.as_bloom() {
                return Ok(bloom.intensity());
            }
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
                .set_glow(update.value.clone())
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
                .set_effect(name, update.value.clone())
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
                .set_effect(&effect.handle, update.value.clone())
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
#[cfg(target_arch = "wasm32")]
mod family;
#[cfg(test)]
mod tests;

#[cfg(all(target_arch = "wasm32", feature = "wgsl-effects"))]
mod shader;
