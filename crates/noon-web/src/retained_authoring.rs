use std::collections::HashSet;

#[cfg(any(target_arch = "wasm32", test))]
use noon_core::Rect;
use noon_core::{Color, ObjectId, Transform2D, Vec2, WHITE};
use serde::{Deserialize, Serialize};

/// Resource-aware Python/browser authoring channel.
///
/// This channel deliberately carries source-level retained text definitions rather
/// than shaped glyphs, font bytes, SVG, or placeholder geometry. The receiving
/// runtime selects the backend compiler and installs its normalized resources into
/// the ordinary retained arenas.
pub const RETAINED_AUTHORING_CHANNEL: &str = "noon.authoring.retained";
pub const RETAINED_AUTHORING_VERSION: u32 = 2;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RetainedTextBackendSpec {
    Native {
        font_family: String,
        line_spacing: f32,
    },
    Typst {
        math: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetainedTextAuthoringSpec {
    pub source: String,
    pub backend: RetainedTextBackendSpec,
    pub font_size: f32,
    #[serde(default)]
    pub transform: Transform2D,
    #[serde(default = "default_white")]
    pub color: Color,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
}

/// Source-compatible Rust alias retained while the browser sidecar migrates from
/// its original Typst-only shape to the backend-neutral text wire.
pub type RetainedTypstAuthoringSpec = RetainedTextAuthoringSpec;

impl RetainedTextAuthoringSpec {
    /// Construct a Typst/MathTypst source definition.
    pub fn new(source: impl Into<String>, math: bool, font_size: f32) -> Result<Self, String> {
        let spec = Self {
            source: source.into(),
            backend: RetainedTextBackendSpec::Typst { math },
            font_size,
            transform: Transform2D::default(),
            color: WHITE,
            opacity: 1.0,
        };
        spec.validate()?;
        Ok(spec)
    }

    /// Construct a native plain-text definition. Font lookup and shaping remain
    /// Rust-owned; the wire carries only deterministic source-level policy.
    pub fn native(
        source: impl Into<String>,
        font_family: impl Into<String>,
        font_size: f32,
        line_spacing: f32,
    ) -> Result<Self, String> {
        let spec = Self {
            source: source.into(),
            backend: RetainedTextBackendSpec::Native {
                font_family: font_family.into(),
                line_spacing,
            },
            font_size,
            transform: Transform2D::default(),
            color: WHITE,
            opacity: 1.0,
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.font_size.is_finite() || self.font_size <= 0.0 {
            return Err("retained text font_size must be finite and positive".to_owned());
        }
        match &self.backend {
            RetainedTextBackendSpec::Native {
                font_family,
                line_spacing,
            } => {
                if font_family.trim().is_empty() {
                    return Err("retained native text font_family must not be empty".to_owned());
                }
                if !line_spacing.is_finite() || *line_spacing < -1.0 {
                    return Err(
                        "retained native text line_spacing must be -1 or greater than -1"
                            .to_owned(),
                    );
                }
            }
            RetainedTextBackendSpec::Typst { .. } => {
                if self.source.is_empty() {
                    return Err("retained Typst source must not be empty".to_owned());
                }
            }
        }
        if !self.opacity.is_finite() || !(0.0..=1.0).contains(&self.opacity) {
            return Err("retained text opacity must be finite and between 0 and 1".to_owned());
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
            return Err("retained text transform/color must be finite".to_owned());
        }
        Ok(())
    }

    pub fn shift(&mut self, offset: Vec2) -> Result<(), String> {
        if !offset.x.is_finite() || !offset.y.is_finite() {
            return Err("retained text shift must be finite".to_owned());
        }
        self.transform.translation += offset;
        Ok(())
    }

    pub fn move_to(&mut self, point: Vec2) -> Result<(), String> {
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err("retained text position must be finite".to_owned());
        }
        self.transform.translation = point;
        Ok(())
    }

    pub fn scale(&mut self, factor: f32) -> Result<(), String> {
        if !factor.is_finite() || factor <= 0.0 {
            return Err("retained text scale factor must be finite and positive".to_owned());
        }
        self.transform.scale = self
            .transform
            .scale
            .component_mul(Vec2::new(factor, factor));
        Ok(())
    }

    pub fn rotate(&mut self, angle: f32) -> Result<(), String> {
        if !angle.is_finite() {
            return Err("retained text rotation must be finite".to_owned());
        }
        self.transform.rotation += angle;
        Ok(())
    }

    pub fn set_opacity(&mut self, opacity: f32) -> Result<(), String> {
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return Err("retained text opacity must be finite and between 0 and 1".to_owned());
        }
        self.opacity = opacity;
        Ok(())
    }

    pub fn set_color(&mut self, color: Color) -> Result<(), String> {
        if [color.red, color.green, color.blue, color.alpha]
            .iter()
            .any(|value| !value.is_finite())
        {
            return Err("retained text color must be finite".to_owned());
        }
        self.color = color;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetainedAuthoringTextObject {
    /// Stable semantic identity, independent of renderer-local resource slots.
    pub object: ObjectId,
    /// Global painter order shared with ordinary geometry.
    pub order: u32,
    pub text: RetainedTextAuthoringSpec,
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

#[cfg(any(target_arch = "wasm32", test))]
fn native_text_intrinsic_bounds(spec: &RetainedTextAuthoringSpec) -> Result<Rect, String> {
    let RetainedTextBackendSpec::Native {
        font_family,
        line_spacing,
    } = &spec.backend
    else {
        return Err("native text bounds require the native backend".to_owned());
    };

    let mut scene = noon::RetainedScene::new();
    scene
        .add_text(
            noon::Text::new(spec.source.clone())
                .with_font(font_family.clone())
                .with_font_size(spec.font_size)
                .with_line_spacing(*line_spacing),
        )
        .map_err(|error| error.to_string())?;
    let object = scene
        .objects()
        .first()
        .ok_or_else(|| "native text measurement produced no retained object".to_owned())?;
    let handle = object
        .content
        .text()
        .ok_or_else(|| "native text measurement produced no text resource".to_owned())?;
    let resource = scene
        .texts()
        .get(handle)
        .ok_or_else(|| "native text measurement lost its text resource".to_owned())?;
    let scale = object.transform.scale;
    Ok(Rect::new(
        Vec2::new(
            resource.bounds.min.x * scale.x,
            resource.bounds.min.y * scale.y,
        ),
        Vec2::new(
            resource.bounds.max.x * scale.x,
            resource.bounds.max.y * scale.y,
        ),
    ))
}

#[cfg(any(target_arch = "wasm32", test))]
fn transformed_bounds(bounds: Rect, transform: Transform2D) -> Rect {
    Rect::from_points([
        transform.transform_point(bounds.min),
        transform.transform_point(Vec2::new(bounds.min.x, bounds.max.y)),
        transform.transform_point(Vec2::new(bounds.max.x, bounds.min.y)),
        transform.transform_point(bounds.max),
    ])
    .expect("four transformed text bounds corners are never empty")
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use wasm_bindgen::prelude::*;

    /// Rust-owned semantic handle used by thin Python/JS Typst wrappers.
    #[wasm_bindgen(js_name = RetainedTypstAuthoringHandle)]
    pub struct WasmRetainedTypstAuthoringHandle {
        inner: RetainedTextAuthoringSpec,
    }

    #[wasm_bindgen(js_class = RetainedTypstAuthoringHandle)]
    impl WasmRetainedTypstAuthoringHandle {
        #[wasm_bindgen(constructor)]
        pub fn new(source: &str, math: bool, font_size: f32) -> Result<Self, JsValue> {
            Ok(Self {
                inner: RetainedTextAuthoringSpec::new(source, math, font_size).map_err(js_error)?,
            })
        }

        #[wasm_bindgen(getter)]
        pub fn source(&self) -> String {
            self.inner.source.clone()
        }

        #[wasm_bindgen(getter)]
        pub fn math(&self) -> bool {
            match &self.inner.backend {
                RetainedTextBackendSpec::Typst { math } => *math,
                RetainedTextBackendSpec::Native { .. } => unreachable!("Typst handle backend"),
            }
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

    /// Rust-owned source handle for native plain text. Font discovery, shaping,
    /// exact font bytes, glyphs, and atlas state never cross into Python. The
    /// canonical native compiler is also used once at construction to cache only
    /// the scene-space layout bounds required by authoring-time placement queries.
    #[wasm_bindgen(js_name = RetainedNativeTextAuthoringHandle)]
    pub struct WasmRetainedNativeTextAuthoringHandle {
        inner: RetainedTextAuthoringSpec,
        intrinsic_bounds: Rect,
    }

    impl WasmRetainedNativeTextAuthoringHandle {
        fn bounds(&self) -> Rect {
            transformed_bounds(self.intrinsic_bounds, self.inner.transform)
        }
    }

    #[wasm_bindgen(js_class = RetainedNativeTextAuthoringHandle)]
    impl WasmRetainedNativeTextAuthoringHandle {
        #[wasm_bindgen(constructor)]
        pub fn new(
            source: &str,
            font_family: &str,
            font_size: f32,
            line_spacing: f32,
        ) -> Result<Self, JsValue> {
            let inner =
                RetainedTextAuthoringSpec::native(source, font_family, font_size, line_spacing)
                    .map_err(js_error)?;
            let intrinsic_bounds = native_text_intrinsic_bounds(&inner).map_err(js_error)?;
            Ok(Self {
                inner,
                intrinsic_bounds,
            })
        }

        #[wasm_bindgen(getter)]
        pub fn source(&self) -> String {
            self.inner.source.clone()
        }

        #[wasm_bindgen(getter, js_name = fontFamily)]
        pub fn font_family(&self) -> String {
            match &self.inner.backend {
                RetainedTextBackendSpec::Native { font_family, .. } => font_family.clone(),
                RetainedTextBackendSpec::Typst { .. } => unreachable!("native text handle backend"),
            }
        }

        #[wasm_bindgen(getter, js_name = lineSpacing)]
        pub fn line_spacing(&self) -> f32 {
            match &self.inner.backend {
                RetainedTextBackendSpec::Native { line_spacing, .. } => *line_spacing,
                RetainedTextBackendSpec::Typst { .. } => unreachable!("native text handle backend"),
            }
        }

        #[wasm_bindgen(getter, js_name = fontSize)]
        pub fn font_size(&self) -> f32 {
            self.inner.font_size
        }

        #[wasm_bindgen(getter, js_name = centerX)]
        pub fn center_x(&self) -> f64 {
            f64::from(self.bounds().center().x)
        }

        #[wasm_bindgen(getter, js_name = centerY)]
        pub fn center_y(&self) -> f64 {
            f64::from(self.bounds().center().y)
        }

        #[wasm_bindgen(getter)]
        pub fn width(&self) -> f64 {
            f64::from(self.bounds().width())
        }

        #[wasm_bindgen(getter)]
        pub fn height(&self) -> f64 {
            f64::from(self.bounds().height())
        }

        #[wasm_bindgen(js_name = criticalX)]
        pub fn critical_x(&self, direction_x: f64, direction_y: f64) -> f64 {
            let point = self
                .bounds()
                .critical_point(Vec2::new(direction_x as f32, direction_y as f32));
            f64::from(point.x)
        }

        #[wasm_bindgen(js_name = criticalY)]
        pub fn critical_y(&self, direction_x: f64, direction_y: f64) -> f64 {
            let point = self
                .bounds()
                .critical_point(Vec2::new(direction_x as f32, direction_y as f32));
            f64::from(point.y)
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
    fn backend_neutral_wire_stays_source_level() {
        let mut native =
            RetainedTextAuthoringSpec::native("Native Noon", "DejaVu Sans Mono", 48.0, -1.0)
                .unwrap();
        native.shift(Vec2::new(1.0, -2.0)).unwrap();
        let typst = RetainedTextAuthoringSpec::new("*Hello* from _Typst!_", false, 96.0).unwrap();
        for spec in [native, typst] {
            let json = serde_json::to_string(&spec).unwrap();
            assert!(!json.contains("glyph"));
            assert!(!json.contains("font_bytes"));
            assert!(!json.contains("svg"));
            assert!(!json.contains("geometry"));
            let round_trip: RetainedTextAuthoringSpec = serde_json::from_str(&json).unwrap();
            assert_eq!(round_trip, spec);
        }
    }

    #[test]
    fn native_layout_bounds_reuse_canonical_text_authoring() {
        let spec = RetainedTextAuthoringSpec::native(
            "Native Noon",
            noon::DEFAULT_NATIVE_TEXT_FONT_FAMILY,
            48.0,
            -1.0,
        )
        .unwrap();
        let bounds = native_text_intrinsic_bounds(&spec).unwrap();
        assert!(bounds.width() > 0.0);
        assert!(bounds.height() > 0.0);
        assert!(bounds.center().x.abs() < 1.0e-5);
        assert!(bounds.center().y.abs() < 1.0e-5);
    }

    #[test]
    fn retained_layout_transform_produces_world_space_aabb() {
        let bounds = Rect::new(Vec2::new(-1.0, -0.5), Vec2::new(1.0, 0.5));
        let transformed = transformed_bounds(
            bounds,
            Transform2D {
                translation: Vec2::new(3.0, -2.0),
                scale: Vec2::new(2.0, 1.0),
                rotation: std::f32::consts::FRAC_PI_2,
            },
        );
        assert!((transformed.center().x - 3.0).abs() < 1.0e-6);
        assert!((transformed.center().y + 2.0).abs() < 1.0e-6);
        assert!((transformed.width() - 1.0).abs() < 1.0e-5);
        assert!((transformed.height() - 4.0).abs() < 1.0e-5);
    }

    #[test]
    fn backend_identity_is_explicit_on_wire() {
        let native =
            RetainedTextAuthoringSpec::native("Noon", "DejaVu Sans Mono", 48.0, 0.3).unwrap();
        assert!(matches!(
            native.backend,
            RetainedTextBackendSpec::Native {
                ref font_family,
                line_spacing: 0.3,
            } if font_family == "DejaVu Sans Mono"
        ));
        let math =
            RetainedTextAuthoringSpec::new("sum_(k=1)^n k = (n(n + 1)) / 2", true, 72.0).unwrap();
        assert!(matches!(
            math.backend,
            RetainedTextBackendSpec::Typst { math: true }
        ));
    }

    #[test]
    fn document_preserves_semantic_identity_global_order_and_backends() {
        let document = RetainedAuthoringDocument::new(vec![
            RetainedAuthoringTextObject {
                object: ObjectId::new(9),
                order: 1,
                text: RetainedTextAuthoringSpec::native("B", "DejaVu Sans Mono", 48.0, -1.0)
                    .unwrap(),
            },
            RetainedAuthoringTextObject {
                object: ObjectId::new(4),
                order: 0,
                text: RetainedTextAuthoringSpec::new("A", false, 48.0).unwrap(),
            },
        ])
        .unwrap();
        assert_eq!(document.protocol_version, 2);
        let round_trip =
            RetainedAuthoringDocument::from_json(&document.to_json().unwrap()).unwrap();
        assert_eq!(round_trip.objects[0].object, ObjectId::new(9));
        assert_eq!(round_trip.objects[0].order, 1);
        assert!(matches!(
            round_trip.objects[0].text.backend,
            RetainedTextBackendSpec::Native { .. }
        ));
        assert_eq!(round_trip.objects[1].object, ObjectId::new(4));
        assert_eq!(round_trip.objects[1].order, 0);
        assert!(matches!(
            round_trip.objects[1].text.backend,
            RetainedTextBackendSpec::Typst { math: false }
        ));
    }

    #[test]
    fn duplicate_identity_or_order_is_rejected() {
        let spec = RetainedTextAuthoringSpec::new("A", false, 48.0).unwrap();
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
