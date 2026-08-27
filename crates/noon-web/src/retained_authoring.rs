use std::collections::HashSet;

use noon_core::{Color, ObjectId, Transform2D, Vec2, WHITE};
use serde::{Deserialize, Serialize};

/// Resource-aware Python/browser authoring channel.
///
/// This channel deliberately carries source-level retained text definitions rather
/// than shaped glyphs, font bytes, SVG, or placeholder geometry. The receiving
/// runtime compiles each definition once into the ordinary retained resource arenas.
pub const RETAINED_AUTHORING_CHANNEL: &str = "noon.authoring.retained";
pub const RETAINED_AUTHORING_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetainedTypstAuthoringSpec {
    pub source: String,
    pub math: bool,
    pub font_size: f32,
    #[serde(default)]
    pub transform: Transform2D,
    #[serde(default = "default_white")]
    pub color: Color,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
}

impl RetainedTypstAuthoringSpec {
    pub fn new(source: impl Into<String>, math: bool, font_size: f32) -> Result<Self, String> {
        let spec = Self {
            source: source.into(),
            math,
            font_size,
            transform: Transform2D::default(),
            color: WHITE,
            opacity: 1.0,
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.source.is_empty() {
            return Err("retained Typst source must not be empty".to_owned());
        }
        if !self.font_size.is_finite() || self.font_size <= 0.0 {
            return Err("retained Typst font_size must be finite and positive".to_owned());
        }
        if !self.opacity.is_finite() || !(0.0..=1.0).contains(&self.opacity) {
            return Err("retained Typst opacity must be finite and between 0 and 1".to_owned());
        }
        let transform = self.transform;
        let values = [
            transform.translation.x,
            transform.translation.y,
            transform.scale.x,
            transform.scale.y,
            transform.rotation,
            self.color.red,
            self.color.green,
            self.color.blue,
            self.color.alpha,
        ];
        if values.iter().any(|value| !value.is_finite()) {
            return Err("retained Typst transform/color must be finite".to_owned());
        }
        Ok(())
    }

    pub fn shift(&mut self, offset: Vec2) -> Result<(), String> {
        if !offset.x.is_finite() || !offset.y.is_finite() {
            return Err("retained Typst shift must be finite".to_owned());
        }
        self.transform.translation += offset;
        Ok(())
    }

    pub fn move_to(&mut self, point: Vec2) -> Result<(), String> {
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err("retained Typst position must be finite".to_owned());
        }
        self.transform.translation = point;
        Ok(())
    }

    pub fn scale(&mut self, factor: f32) -> Result<(), String> {
        if !factor.is_finite() || factor <= 0.0 {
            return Err("retained Typst scale factor must be finite and positive".to_owned());
        }
        self.transform.scale = self
            .transform
            .scale
            .component_mul(Vec2::new(factor, factor));
        Ok(())
    }

    pub fn rotate(&mut self, angle: f32) -> Result<(), String> {
        if !angle.is_finite() {
            return Err("retained Typst rotation must be finite".to_owned());
        }
        self.transform.rotation += angle;
        Ok(())
    }

    pub fn set_opacity(&mut self, opacity: f32) -> Result<(), String> {
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return Err("retained Typst opacity must be finite and between 0 and 1".to_owned());
        }
        self.opacity = opacity;
        Ok(())
    }

    pub fn set_color(&mut self, color: Color) -> Result<(), String> {
        if [color.red, color.green, color.blue, color.alpha]
            .iter()
            .any(|value| !value.is_finite())
        {
            return Err("retained Typst color must be finite".to_owned());
        }
        self.color = color;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetainedAuthoringTextObject {
    /// Stable semantic identity. V1 keeps this independent of renderer-local slots.
    pub object: ObjectId,
    /// Global painter order shared with ordinary geometry at the eventual mixed lowering boundary.
    pub order: u32,
    pub text: RetainedTypstAuthoringSpec,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetainedAuthoringDocument {
    pub channel: String,
    pub protocol_version: u32,
    pub objects: Vec<RetainedAuthoringTextObject>,
}

impl RetainedAuthoringDocument {
    pub fn new(objects: Vec<RetainedAuthoringTextObject>) -> Result<Self, String> {
        let document = Self {
            channel: RETAINED_AUTHORING_CHANNEL.to_owned(),
            protocol_version: RETAINED_AUTHORING_VERSION,
            objects,
        };
        document.validate()?;
        Ok(document)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.channel != RETAINED_AUTHORING_CHANNEL {
            return Err(format!(
                "invalid retained authoring channel {:?}",
                self.channel
            ));
        }
        if self.protocol_version != RETAINED_AUTHORING_VERSION {
            return Err(format!(
                "unsupported retained authoring protocol version {}",
                self.protocol_version
            ));
        }
        let mut objects = HashSet::with_capacity(self.objects.len());
        let mut orders = HashSet::with_capacity(self.objects.len());
        for object in &self.objects {
            if !objects.insert(object.object) {
                return Err(format!(
                    "duplicate retained authoring object {}",
                    object.object.get()
                ));
            }
            if !orders.insert(object.order) {
                return Err(format!(
                    "duplicate retained authoring painter order {}",
                    object.order
                ));
            }
            object.text.validate()?;
        }
        Ok(())
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        let document: Self = serde_json::from_str(json)
            .map_err(|error| format!("invalid retained authoring document: {error}"))?;
        document.validate()?;
        Ok(document)
    }

    pub fn to_json(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string(self)
            .map_err(|error| format!("unable to serialize retained authoring document: {error}"))
    }
}

const fn default_white() -> Color {
    WHITE
}

const fn default_opacity() -> f32 {
    1.0
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use wasm_bindgen::prelude::*;

    /// Rust-owned semantic handle used by thin Python/JS Typst wrappers.
    ///
    /// Authoring mutations update this handle directly. `specJson()` is only the
    /// cross-worker source definition; it never contains glyphs, font bytes, SVG,
    /// vectorized glyph outlines, or renderer-local atlas state.
    #[wasm_bindgen(js_name = RetainedTypstAuthoringHandle)]
    pub struct WasmRetainedTypstAuthoringHandle {
        inner: RetainedTypstAuthoringSpec,
    }

    #[wasm_bindgen(js_class = RetainedTypstAuthoringHandle)]
    impl WasmRetainedTypstAuthoringHandle {
        #[wasm_bindgen(constructor)]
        pub fn new(source: &str, math: bool, font_size: f32) -> Result<Self, JsValue> {
            Ok(Self {
                inner: RetainedTypstAuthoringSpec::new(source, math, font_size)
                    .map_err(js_error)?,
            })
        }

        #[wasm_bindgen(getter)]
        pub fn source(&self) -> String {
            self.inner.source.clone()
        }

        #[wasm_bindgen(getter)]
        pub fn math(&self) -> bool {
            self.inner.math
        }

        #[wasm_bindgen(getter, js_name = fontSize)]
        pub fn font_size(&self) -> f32 {
            self.inner.font_size
        }

        #[wasm_bindgen(js_name = shift)]
        pub fn shift(&mut self, x: f32, y: f32) -> Result<(), JsValue> {
            self.inner.shift(Vec2::new(x, y)).map_err(js_error)
        }

        #[wasm_bindgen(js_name = moveTo)]
        pub fn move_to(&mut self, x: f32, y: f32) -> Result<(), JsValue> {
            self.inner.move_to(Vec2::new(x, y)).map_err(js_error)
        }

        pub fn scale(&mut self, factor: f32) -> Result<(), JsValue> {
            self.inner.scale(factor).map_err(js_error)
        }

        pub fn rotate(&mut self, angle: f32) -> Result<(), JsValue> {
            self.inner.rotate(angle).map_err(js_error)
        }

        #[wasm_bindgen(js_name = setOpacity)]
        pub fn set_opacity(&mut self, opacity: f32) -> Result<(), JsValue> {
            self.inner.set_opacity(opacity).map_err(js_error)
        }

        #[wasm_bindgen(js_name = setColor)]
        pub fn set_color(
            &mut self,
            red: f32,
            green: f32,
            blue: f32,
            alpha: f32,
        ) -> Result<(), JsValue> {
            self.inner
                .set_color(Color::rgba(red, green, blue, alpha))
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = specJson)]
        pub fn spec_json(&self) -> Result<String, JsValue> {
            self.inner.validate().map_err(js_error)?;
            serde_json::to_string(&self.inner).map_err(js_error)
        }
    }

    #[wasm_bindgen(js_name = validateRetainedAuthoringDocumentJson)]
    pub fn validate_retained_authoring_document_json(json: &str) -> Result<(), JsValue> {
        RetainedAuthoringDocument::from_json(json)
            .map(|_| ())
            .map_err(js_error)
    }

    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typst_handle_wire_spec_stays_source_level() {
        let mut spec =
            RetainedTypstAuthoringSpec::new("*Hello* from _Typst!_", false, 96.0).unwrap();
        spec.shift(Vec2::new(1.0, -2.0)).unwrap();
        let json = serde_json::to_string(&spec).unwrap();
        assert!(json.contains("*Hello* from _Typst!_"));
        assert!(!json.contains("glyph"));
        assert!(!json.contains("font_bytes"));
        assert!(!json.contains("svg"));
        assert!(!json.contains("geometry"));
        let round_trip: RetainedTypstAuthoringSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(round_trip, spec);
    }

    #[test]
    fn math_typst_identity_is_explicit_on_wire() {
        let spec =
            RetainedTypstAuthoringSpec::new("sum_(k=1)^n k = (n(n + 1)) / 2", true, 72.0).unwrap();
        assert!(spec.math);
        assert_eq!(spec.font_size, 72.0);
    }

    #[test]
    fn document_preserves_semantic_identity_and_global_order() {
        let document = RetainedAuthoringDocument::new(vec![
            RetainedAuthoringTextObject {
                object: ObjectId::new(9),
                order: 1,
                text: RetainedTypstAuthoringSpec::new("B", false, 48.0).unwrap(),
            },
            RetainedAuthoringTextObject {
                object: ObjectId::new(4),
                order: 0,
                text: RetainedTypstAuthoringSpec::new("A", false, 48.0).unwrap(),
            },
        ])
        .unwrap();
        let round_trip =
            RetainedAuthoringDocument::from_json(&document.to_json().unwrap()).unwrap();
        assert_eq!(round_trip.objects[0].object, ObjectId::new(9));
        assert_eq!(round_trip.objects[0].order, 1);
        assert_eq!(round_trip.objects[1].object, ObjectId::new(4));
        assert_eq!(round_trip.objects[1].order, 0);
    }

    #[test]
    fn duplicate_identity_or_order_is_rejected() {
        let spec = RetainedTypstAuthoringSpec::new("A", false, 48.0).unwrap();
        assert!(RetainedAuthoringDocument::new(vec![
            RetainedAuthoringTextObject {
                object: ObjectId::new(1),
                order: 0,
                text: spec.clone(),
            },
            RetainedAuthoringTextObject {
                object: ObjectId::new(1),
                order: 1,
                text: spec.clone(),
            },
        ])
        .is_err());
        assert!(RetainedAuthoringDocument::new(vec![
            RetainedAuthoringTextObject {
                object: ObjectId::new(1),
                order: 0,
                text: spec.clone(),
            },
            RetainedAuthoringTextObject {
                object: ObjectId::new(2),
                order: 0,
                text: spec,
            },
        ])
        .is_err());
    }
}
