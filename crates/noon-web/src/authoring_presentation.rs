//! Optional language handles for immutable shared Rust presentation values.

use crate::AuthoringFailure;
use noon::{Color, PresentationError, Theme};

impl From<PresentationError> for AuthoringFailure {
    fn from(error: PresentationError) -> Self {
        match error {
            PresentationError::UnknownName { .. } => {
                Self::new("invalid_input", "presentation.unknown_name", error)
            }
            PresentationError::CategoricalIndexOutOfRange { .. } => {
                Self::new("index_error", "presentation.category_out_of_range", error)
            }
        }
    }
}

fn categorical(theme: Theme, index: f64) -> Result<Color, AuthoringFailure> {
    // Keep the JS number intact until checked: an integer WASM argument would
    // truncate fractions or wrap large values before the shared palette lookup.
    if !index.is_finite() || index.fract() != 0.0 || !(0.0..=f64::from(u32::MAX)).contains(&index) {
        return Err(AuthoringFailure::new(
            "invalid_input",
            "presentation.invalid_category_index",
            "categorical color index must be a nonnegative integer representable by this host",
        ));
    }
    theme.categorical(index as usize).map_err(Into::into)
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use crate::{authoring_error::js_error, WasmManimColor};
    use noon::{ColorRole, SpacingRole, TextRole, TextStyle};
    use wasm_bindgen::prelude::*;

    /// A value only: no scene, renderer, identity, or ambient theme state.
    #[wasm_bindgen]
    pub struct WasmTheme {
        value: Theme,
    }

    #[wasm_bindgen]
    impl WasmTheme {
        #[wasm_bindgen(constructor)]
        pub fn new(preset: &str) -> Result<Self, JsValue> {
            preset.parse().map(|value| Self { value }).map_err(js_error)
        }

        pub fn color(&self, role: &str) -> Result<WasmManimColor, JsValue> {
            let role = role.parse::<ColorRole>().map_err(js_error)?;
            Ok(WasmManimColor::from_color(self.value.color(role)))
        }

        pub fn spacing(&self, role: &str) -> Result<f64, JsValue> {
            let role = role.parse::<SpacingRole>().map_err(js_error)?;
            Ok(self.value.spacing(role))
        }

        pub fn categorical(&self, index: f64) -> Result<WasmManimColor, JsValue> {
            super::categorical(self.value, index)
                .map(WasmManimColor::from_color)
                .map_err(js_error)
        }

        #[wasm_bindgen(getter, js_name = categoricalCount)]
        pub fn categorical_count(&self) -> usize {
            self.value.categorical_count()
        }

        #[wasm_bindgen(js_name = textStyle)]
        pub fn text_style(&self, role: &str) -> Result<WasmPresentationTextStyle, JsValue> {
            let role = role.parse::<TextRole>().map_err(js_error)?;
            Ok(WasmPresentationTextStyle {
                value: self.value.text_style(role),
            })
        }
    }

    /// Immutable defaults forwarded into the ordinary shared Text constructor.
    #[wasm_bindgen]
    pub struct WasmPresentationTextStyle {
        value: TextStyle,
    }

    #[wasm_bindgen]
    impl WasmPresentationTextStyle {
        #[wasm_bindgen(getter, js_name = fontFamily)]
        pub fn font_family(&self) -> String {
            self.value.font_family().into()
        }

        #[wasm_bindgen(getter, js_name = fontSize)]
        pub fn font_size(&self) -> f64 {
            f64::from(self.value.font_size())
        }

        #[wasm_bindgen(getter, js_name = lineSpacing)]
        pub fn line_spacing(&self) -> f64 {
            f64::from(self.value.line_spacing())
        }

        #[wasm_bindgen(getter)]
        pub fn color(&self) -> WasmManimColor {
            WasmManimColor::from_color(self.value.color())
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_boundary_rejects_lossy_numbers_before_shared_lookup() {
        for index in [-1.0, 0.5, 4_294_967_296.0, f64::NAN, f64::INFINITY] {
            let error = categorical(Theme::dark(), index).unwrap_err();
            assert_eq!(error.category, "invalid_input");
            assert_eq!(error.code, "presentation.invalid_category_index");
        }
        assert_eq!(
            categorical(Theme::dark(), 0.0).unwrap(),
            Theme::dark().categorical(0).unwrap()
        );
        let error =
            categorical(Theme::dark(), Theme::dark().categorical_count() as f64).unwrap_err();
        assert_eq!(error.category, "index_error");
        assert_eq!(error.code, "presentation.category_out_of_range");
    }

    #[test]
    fn shared_name_error_is_preserved_at_the_optional_boundary() {
        let shared = "not-a-theme".parse::<Theme>().unwrap_err();
        let message = shared.to_string();
        let boundary = AuthoringFailure::from(shared);
        assert_eq!(boundary.category, "invalid_input");
        assert_eq!(boundary.code, "presentation.unknown_name");
        assert_eq!(boundary.message, message);
    }
}
