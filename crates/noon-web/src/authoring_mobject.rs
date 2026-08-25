use noon_core::{
    semantic_path_bounds, Bounds2D64, Color, GeometryRef, ObjectSnapshot, SemanticPaint,
    SemanticStyle, SemanticTransform2_5D, SemanticVec3, Style, Vec2,
};

#[derive(Clone, Debug, PartialEq)]
pub struct FrontendMobjectHandle {
    snapshot: ObjectSnapshot,
    semantic_transform: SemanticTransform2_5D,
    semantic_style: SemanticStyle,
}

impl FrontendMobjectHandle {
    pub fn from_snapshot(snapshot: ObjectSnapshot) -> Self {
        let transform = snapshot.transform;
        let semantic_transform = SemanticTransform2_5D {
            translation: SemanticVec3::new(
                f64::from(transform.translation.x),
                f64::from(transform.translation.y),
                0.0,
            ),
            scale: SemanticVec3::new(
                f64::from(transform.scale.x),
                f64::from(transform.scale.y),
                1.0,
            ),
            rotation_z: f64::from(transform.rotation),
        };
        let semantic_style = authoring_style_from_legacy(snapshot.style);
        Self {
            snapshot,
            semantic_transform,
            semantic_style,
        }
    }

    pub fn from_json(snapshot_json: &str) -> Result<Self, String> {
        serde_json::from_str(snapshot_json)
            .map(Self::from_snapshot)
            .map_err(|error| format!("invalid mobject snapshot: {error}"))
    }

    pub fn snapshot(&self) -> &ObjectSnapshot {
        &self.snapshot
    }

    pub fn snapshot_json(&self) -> Result<String, String> {
        serde_json::to_string(&self.snapshot)
            .map_err(|error| format!("unable to serialize mobject snapshot: {error}"))
    }

    pub fn replace_json(&mut self, snapshot_json: &str) -> Result<(), String> {
        *self = Self::from_json(snapshot_json)?;
        Ok(())
    }

    pub fn layout_bounds(&self) -> Option<Bounds2D64> {
        snapshot_layout_bounds(&self.snapshot, self.semantic_transform)
    }

    pub fn center(&self) -> (f64, f64) {
        self.layout_bounds()
            .map(|bounds| {
                (
                    (bounds.min_x + bounds.max_x) * 0.5,
                    (bounds.min_y + bounds.max_y) * 0.5,
                )
            })
            .unwrap_or_else(|| {
                let translation = self.semantic_transform.translation;
                (translation.x, translation.y)
            })
    }

    pub fn width(&self) -> f64 {
        self.layout_bounds().map_or(0.0, Bounds2D64::width)
    }

    pub fn height(&self) -> f64 {
        self.layout_bounds().map_or(0.0, Bounds2D64::height)
    }

    pub fn critical_point(&self, direction_x: f64, direction_y: f64) -> (f64, f64) {
        let Some(bounds) = self.layout_bounds() else {
            return self.center();
        };
        let center = self.center();
        (
            if direction_x < 0.0 {
                bounds.min_x
            } else if direction_x > 0.0 {
                bounds.max_x
            } else {
                center.0
            },
            if direction_y < 0.0 {
                bounds.min_y
            } else if direction_y > 0.0 {
                bounds.max_y
            } else {
                center.1
            },
        )
    }

    pub fn shift(&mut self, x: f64, y: f64) -> Result<(), String> {
        let offset = semantic_xy_f64(x, y)?;
        let translation = SemanticVec3::new(
            self.semantic_transform.translation.x + offset.x,
            self.semantic_transform.translation.y + offset.y,
            self.semantic_transform.translation.z,
        );
        translation
            .lower_xy_f32()
            .map_err(|error| error.to_string())?;
        self.semantic_transform.translation = translation;
        self.sync_legacy_transform()
    }

    pub fn move_to(&mut self, x: f64, y: f64) -> Result<(), String> {
        semantic_xy(x, y)?;
        let center = self.center();
        self.shift(x - center.0, y - center.1)
    }

    pub fn scale(&mut self, x: f64, y: f64) -> Result<(), String> {
        let x = render_f64("scale.x", x)?;
        let y = render_f64("scale.y", y)?;
        let scale = SemanticVec3::new(
            self.semantic_transform.scale.x * x,
            self.semantic_transform.scale.y * y,
            self.semantic_transform.scale.z,
        );
        scale.lower_xy_f32().map_err(|error| error.to_string())?;
        self.semantic_transform.scale = scale;
        self.sync_legacy_transform()
    }

    pub fn rotate(&mut self, angle: f64) -> Result<(), String> {
        let angle = render_f64("rotation", angle)?;
        let rotation = self.semantic_transform.rotation_z + angle;
        finite_f32("rotation result", rotation)?;
        self.semantic_transform.rotation_z = rotation;
        self.sync_legacy_transform()
    }

    pub fn rotate_about_point(
        &mut self,
        angle: f64,
        point_x: f64,
        point_y: f64,
    ) -> Result<(), String> {
        let angle = render_f64("rotation", angle)?;
        let pivot = semantic_xy_f64(point_x, point_y)?;
        let rotation = self.semantic_transform.rotation_z + angle;
        finite_f32("rotation result", rotation)?;

        let translation = self.semantic_transform.translation;
        let relative_x = translation.x - pivot.x;
        let relative_y = translation.y - pivot.y;
        let cosine = angle.cos();
        let sine = angle.sin();
        let next_translation = SemanticVec3::new(
            pivot.x + relative_x * cosine - relative_y * sine,
            pivot.y + relative_x * sine + relative_y * cosine,
            translation.z,
        );
        next_translation
            .lower_xy_f32()
            .map_err(|error| error.to_string())?;
        self.semantic_transform.translation = next_translation;
        self.semantic_transform.rotation_z = rotation;
        self.sync_legacy_transform()
    }

    pub fn set_color(&mut self, red: f64, green: f64, blue: f64, alpha: f64) -> Result<(), String> {
        let color = opaque_color("color", red, green, blue)?;
        let opacity = unit_opacity("color.alpha", alpha)?;
        let had_fill = self.semantic_style.fill.is_some();
        let had_stroke = self.semantic_style.stroke.is_some();
        if had_fill {
            self.semantic_style.fill = Some(SemanticPaint::Solid(color));
            self.semantic_style.fill_opacity = opacity;
        }
        if had_stroke {
            self.semantic_style.stroke = Some(SemanticPaint::Solid(color));
            self.semantic_style.stroke_opacity = opacity;
        }
        if !had_fill && !had_stroke {
            self.semantic_style.fill = Some(SemanticPaint::Solid(color));
            self.semantic_style.fill_opacity = opacity;
        }
        self.sync_legacy_style();
        Ok(())
    }

    pub fn disable_fill(&mut self) {
        self.semantic_style.fill = None;
        self.sync_legacy_style();
    }

    pub fn set_fill_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), String> {
        let color = opaque_color("fill", red, green, blue)?;
        let requested_opacity = unit_opacity("fill.alpha", alpha)?;
        if self.semantic_style.fill.is_none() {
            self.semantic_style.fill_opacity = requested_opacity;
        }
        self.semantic_style.fill = Some(SemanticPaint::Solid(color));
        self.sync_legacy_style();
        Ok(())
    }

    pub fn set_fill_opacity(&mut self, opacity: f64) -> Result<(), String> {
        let opacity = unit_opacity("fill opacity", opacity)?;
        if self.semantic_style.fill.is_none() {
            self.semantic_style.fill = Some(SemanticPaint::Solid(Color::WHITE));
        }
        self.semantic_style.fill_opacity = opacity;
        self.sync_legacy_style();
        Ok(())
    }

    pub fn fill_opacity(&self) -> f64 {
        if self.semantic_style.fill.is_some() {
            self.semantic_style.fill_opacity
        } else {
            0.0
        }
    }

    pub fn disable_stroke(&mut self) {
        self.semantic_style.stroke = None;
        self.sync_legacy_style();
    }

    pub fn set_stroke_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), String> {
        let color = opaque_color("stroke", red, green, blue)?;
        let requested_opacity = unit_opacity("stroke.alpha", alpha)?;
        if self.semantic_style.stroke.is_none() {
            self.semantic_style.stroke_opacity = requested_opacity;
        }
        self.semantic_style.stroke = Some(SemanticPaint::Solid(color));
        self.sync_legacy_style();
        Ok(())
    }

    pub fn set_stroke_width(&mut self, width: f64) -> Result<(), String> {
        let width = render_f64("stroke width", width)?;
        if width < 0.0 {
            return Err("stroke width must be non-negative".to_owned());
        }
        self.semantic_style.stroke_width = width;
        if self.semantic_style.stroke.is_none() {
            self.semantic_style.stroke = Some(SemanticPaint::Solid(Color::WHITE));
            self.semantic_style.stroke_opacity = 1.0;
        }
        self.sync_legacy_style();
        Ok(())
    }

    pub fn set_stroke_opacity(&mut self, opacity: f64) -> Result<(), String> {
        let opacity = unit_opacity("stroke opacity", opacity)?;
        if self.semantic_style.stroke.is_none() {
            self.semantic_style.stroke = Some(SemanticPaint::Solid(Color::WHITE));
        }
        self.semantic_style.stroke_opacity = opacity;
        self.sync_legacy_style();
        Ok(())
    }

    pub fn stroke_opacity(&self) -> f64 {
        if self.semantic_style.stroke.is_some() {
            self.semantic_style.stroke_opacity
        } else {
            0.0
        }
    }

    pub fn set_opacity(&mut self, opacity: f64) -> Result<(), String> {
        let opacity = unit_opacity("opacity", opacity)?;
        if self.semantic_style.fill.is_some() {
            self.semantic_style.fill_opacity = opacity;
        }
        if self.semantic_style.stroke.is_some() {
            self.semantic_style.stroke_opacity = opacity;
        }
        self.sync_legacy_style();
        Ok(())
    }

    fn sync_legacy_transform(&mut self) -> Result<(), String> {
        self.snapshot.transform.translation = self
            .semantic_transform
            .translation
            .lower_xy_f32()
            .map_err(|error| error.to_string())?;
        self.snapshot.transform.scale = self
            .semantic_transform
            .scale
            .lower_xy_f32()
            .map_err(|error| error.to_string())?;
        self.snapshot.transform.rotation =
            finite_f32("rotation", self.semantic_transform.rotation_z)?;
        Ok(())
    }

    fn sync_legacy_style(&mut self) {
        self.snapshot.style.fill = legacy_solid_color(
            self.semantic_style.fill.as_ref(),
            self.semantic_style.fill_opacity,
        );
        self.snapshot.style.stroke = legacy_solid_color(
            self.semantic_style.stroke.as_ref(),
            self.semantic_style.stroke_opacity,
        );
        self.snapshot.style.stroke_width = self.semantic_style.stroke_width as f32;
        self.snapshot.style.stroke_width_mode = self.semantic_style.stroke_width_mode;
        self.snapshot.style.opacity = self.semantic_style.object_opacity as f32;
    }

    pub fn become_handle(&mut self, other: &Self) {
        self.snapshot = other.snapshot.clone();
        self.semantic_transform = other.semantic_transform;
        self.semantic_style = other.semantic_style.clone();
    }

    pub fn replace_handle(
        &mut self,
        other: &Self,
        dim_to_match: u32,
        stretch: bool,
    ) -> Result<(), String> {
        if dim_to_match > 1 {
            return Err(
                "replace currently supports width (0) or height (1) in the 2D authoring model"
                    .to_owned(),
            );
        }
        let target_center = other.center();
        let source_width = self.width();
        let source_height = self.height();
        let target_width = other.width();
        let target_height = other.height();

        if stretch {
            if source_width == 0.0 || source_height == 0.0 {
                return Err("cannot stretch-replace an object with zero width or height".to_owned());
            }
            self.scale(target_width / source_width, target_height / source_height)?;
        } else {
            let (source_length, target_length) = if dim_to_match == 0 {
                (source_width, target_width)
            } else {
                (source_height, target_height)
            };
            if source_length == 0.0 {
                return Err("cannot replace along a zero-length dimension".to_owned());
            }
            let factor = target_length / source_length;
            self.scale(factor, factor)?;
        }
        self.move_to(target_center.0, target_center.1)
    }

    pub fn next_to_handle(
        &mut self,
        other: &Self,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
    ) -> Result<(), String> {
        let (axis_x, axis_y) = normalized_direction(direction_x, direction_y)?;
        let source = self.critical_point(-axis_x, -axis_y);
        let target = other.critical_point(axis_x, axis_y);
        self.shift(
            target.0 - source.0 + axis_x * buff,
            target.1 - source.1 + axis_y * buff,
        )
    }

    pub fn next_to_point(
        &mut self,
        point_x: f64,
        point_y: f64,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
    ) -> Result<(), String> {
        semantic_xy(point_x, point_y)?;
        let (axis_x, axis_y) = normalized_direction(direction_x, direction_y)?;
        let source = self.critical_point(-axis_x, -axis_y);
        self.shift(
            point_x - source.0 + axis_x * buff,
            point_y - source.1 + axis_y * buff,
        )
    }

    pub fn align_to_handle(
        &mut self,
        other: &Self,
        direction_x: f64,
        direction_y: f64,
    ) -> Result<(), String> {
        finite_f32("direction.x", direction_x)?;
        finite_f32("direction.y", direction_y)?;
        let source = self.critical_point(direction_x, direction_y);
        let target = other.critical_point(direction_x, direction_y);
        self.shift(
            if direction_x == 0.0 {
                0.0
            } else {
                target.0 - source.0
            },
            if direction_y == 0.0 {
                0.0
            } else {
                target.1 - source.1
            },
        )
    }

    pub fn align_to_point(
        &mut self,
        point_x: f64,
        point_y: f64,
        direction_x: f64,
        direction_y: f64,
    ) -> Result<(), String> {
        semantic_xy(point_x, point_y)?;
        let source = self.critical_point(direction_x, direction_y);
        self.shift(
            if direction_x == 0.0 {
                0.0
            } else {
                point_x - source.0
            },
            if direction_y == 0.0 {
                0.0
            } else {
                point_y - source.1
            },
        )
    }

    pub fn align_on_frame(
        &mut self,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
    ) -> Result<(), String> {
        finite_f32("direction.x", direction_x)?;
        finite_f32("direction.y", direction_y)?;
        let point = self.critical_point(direction_x, direction_y);
        let mut shift_x = 0.0;
        let mut shift_y = 0.0;
        if direction_x != 0.0 {
            let target = direction_x.signum() * f64::from(noon_core::DEFAULT_FRAME_WIDTH) * 0.5;
            shift_x = target - point.0 - direction_x * buff;
        }
        if direction_y != 0.0 {
            let target = direction_y.signum() * f64::from(noon_core::DEFAULT_FRAME_HEIGHT) * 0.5;
            shift_y = target - point.1 - direction_y * buff;
        }
        self.shift(shift_x, shift_y)
    }
}

fn finite_f32(name: &str, value: f64) -> Result<f32, String> {
    render_f64(name, value).map(|value| value as f32)
}

fn render_f64(name: &str, value: f64) -> Result<f64, String> {
    if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
        return Err(format!("{name} must be a finite f32-compatible number"));
    }
    Ok(value)
}

fn unit_opacity(name: &str, value: f64) -> Result<f64, String> {
    let value = render_f64(name, value)?;
    if !(0.0..=1.0).contains(&value) {
        return Err(format!("{name} must be between 0 and 1"));
    }
    Ok(value)
}

fn opaque_color(name: &str, red: f64, green: f64, blue: f64) -> Result<Color, String> {
    Ok(Color::rgba(
        finite_f32(&format!("{name}.red"), red)?,
        finite_f32(&format!("{name}.green"), green)?,
        finite_f32(&format!("{name}.blue"), blue)?,
        1.0,
    ))
}

fn authoring_style_from_legacy(style: Style) -> SemanticStyle {
    let mut semantic = SemanticStyle::from_legacy(style);
    if let Some(SemanticPaint::Solid(color)) = &mut semantic.fill {
        semantic.fill_opacity = f64::from(color.alpha);
        color.alpha = 1.0;
    }
    if let Some(SemanticPaint::Solid(color)) = &mut semantic.stroke {
        semantic.stroke_opacity = f64::from(color.alpha);
        color.alpha = 1.0;
    }
    semantic
}

fn legacy_solid_color(paint: Option<&SemanticPaint>, opacity: f64) -> Option<Color> {
    let SemanticPaint::Solid(color) = paint? else {
        return None;
    };
    Some(Color {
        alpha: opacity as f32,
        ..*color
    })
}

fn semantic_xy(x: f64, y: f64) -> Result<Vec2, String> {
    semantic_xy_f64(x, y)?
        .lower_xy_f32()
        .map_err(|error| error.to_string())
}

fn semantic_xy_f64(x: f64, y: f64) -> Result<SemanticVec3, String> {
    let value = SemanticVec3::new(x, y, 0.0);
    value.lower_xy_f32().map_err(|error| error.to_string())?;
    Ok(value)
}

fn normalized_direction(x: f64, y: f64) -> Result<(f64, f64), String> {
    if !x.is_finite() || !y.is_finite() {
        return Err("direction must be finite".to_owned());
    }
    let length = x.hypot(y);
    if length == 0.0 {
        return Err("direction must be non-zero".to_owned());
    }
    Ok((x / length, y / length))
}

fn snapshot_layout_bounds(
    snapshot: &ObjectSnapshot,
    transform: SemanticTransform2_5D,
) -> Option<Bounds2D64> {
    let local = match &snapshot.geometry {
        GeometryRef::Circle { radius } => Bounds2D64 {
            min_x: -f64::from(*radius),
            min_y: -f64::from(*radius),
            max_x: f64::from(*radius),
            max_y: f64::from(*radius),
        },
        GeometryRef::Rectangle { size } => Bounds2D64 {
            min_x: -f64::from(size.x) * 0.5,
            min_y: -f64::from(size.y) * 0.5,
            max_x: f64::from(size.x) * 0.5,
            max_y: f64::from(size.y) * 0.5,
        },
        GeometryRef::Line { start, end } => Bounds2D64 {
            min_x: f64::from(start.x.min(end.x)),
            min_y: f64::from(start.y.min(end.y)),
            max_x: f64::from(start.x.max(end.x)),
            max_y: f64::from(start.y.max(end.y)),
        },
        GeometryRef::VectorPath(path) => semantic_path_bounds(path, 0.0).layout?,
        GeometryRef::External(_) => return None,
    };

    let sine = transform.rotation_z.sin();
    let cosine = transform.rotation_z.cos();
    let scale_x = transform.scale.x;
    let scale_y = transform.scale.y;
    let translation_x = transform.translation.x;
    let translation_y = transform.translation.y;
    let corners = [
        (local.min_x, local.min_y),
        (local.min_x, local.max_y),
        (local.max_x, local.min_y),
        (local.max_x, local.max_y),
    ];
    let mut world: Option<Bounds2D64> = None;
    for (x, y) in corners {
        let x = x * scale_x;
        let y = y * scale_y;
        let point_x = x * cosine - y * sine + translation_x;
        let point_y = x * sine + y * cosine + translation_y;
        if let Some(bounds) = &mut world {
            bounds.include(point_x, point_y);
        } else {
            world = Some(Bounds2D64::point(point_x, point_y));
        }
    }
    world
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::FrontendMobjectHandle;

    fn js_error(error: String) -> JsValue {
        JsValue::from_str(&error)
    }

    #[wasm_bindgen]
    pub struct WasmAuthoringMobjectHandle(FrontendMobjectHandle);

    #[wasm_bindgen]
    impl WasmAuthoringMobjectHandle {
        #[wasm_bindgen(constructor)]
        pub fn new(snapshot_json: &str) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            FrontendMobjectHandle::from_json(snapshot_json)
                .map(WasmAuthoringMobjectHandle)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = cloneHandle)]
        pub fn clone_handle(&self) -> WasmAuthoringMobjectHandle {
            WasmAuthoringMobjectHandle(self.0.clone())
        }

        #[wasm_bindgen(js_name = snapshotJson)]
        pub fn snapshot_json(&self) -> Result<String, JsValue> {
            self.0.snapshot_json().map_err(js_error)
        }

        #[wasm_bindgen(js_name = replaceSnapshotJson)]
        pub fn replace_snapshot_json(&mut self, snapshot_json: &str) -> Result<(), JsValue> {
            self.0.replace_json(snapshot_json).map_err(js_error)
        }

        #[wasm_bindgen(getter, js_name = centerX)]
        pub fn center_x(&self) -> f64 {
            self.0.center().0
        }

        #[wasm_bindgen(getter, js_name = centerY)]
        pub fn center_y(&self) -> f64 {
            self.0.center().1
        }

        #[wasm_bindgen(getter)]
        pub fn width(&self) -> f64 {
            self.0.width()
        }

        #[wasm_bindgen(getter)]
        pub fn height(&self) -> f64 {
            self.0.height()
        }

        #[wasm_bindgen(js_name = criticalX)]
        pub fn critical_x(&self, direction_x: f64, direction_y: f64) -> f64 {
            self.0.critical_point(direction_x, direction_y).0
        }

        #[wasm_bindgen(js_name = criticalY)]
        pub fn critical_y(&self, direction_x: f64, direction_y: f64) -> f64 {
            self.0.critical_point(direction_x, direction_y).1
        }

        pub fn shift(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
            self.0.shift(x, y).map_err(js_error)
        }

        #[wasm_bindgen(js_name = moveTo)]
        pub fn move_to(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
            self.0.move_to(x, y).map_err(js_error)
        }

        pub fn scale(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
            self.0.scale(x, y).map_err(js_error)
        }

        pub fn rotate(&mut self, angle: f64) -> Result<(), JsValue> {
            self.0.rotate(angle).map_err(js_error)
        }

        #[wasm_bindgen(js_name = rotateAboutPoint)]
        pub fn rotate_about_point(
            &mut self,
            angle: f64,
            point_x: f64,
            point_y: f64,
        ) -> Result<(), JsValue> {
            self.0
                .rotate_about_point(angle, point_x, point_y)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setColor)]
        pub fn set_color(
            &mut self,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
        ) -> Result<(), JsValue> {
            self.0.set_color(red, green, blue, alpha).map_err(js_error)
        }

        #[wasm_bindgen(js_name = disableFill)]
        pub fn disable_fill(&mut self) {
            self.0.disable_fill();
        }

        #[wasm_bindgen(js_name = setFillColor)]
        pub fn set_fill_color(
            &mut self,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
        ) -> Result<(), JsValue> {
            self.0
                .set_fill_color(red, green, blue, alpha)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setFillOpacity)]
        pub fn set_fill_opacity(&mut self, opacity: f64) -> Result<(), JsValue> {
            self.0.set_fill_opacity(opacity).map_err(js_error)
        }

        #[wasm_bindgen(getter, js_name = fillOpacity)]
        pub fn fill_opacity(&self) -> f64 {
            self.0.fill_opacity()
        }

        #[wasm_bindgen(js_name = disableStroke)]
        pub fn disable_stroke(&mut self) {
            self.0.disable_stroke();
        }

        #[wasm_bindgen(js_name = setStrokeColor)]
        pub fn set_stroke_color(
            &mut self,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
        ) -> Result<(), JsValue> {
            self.0
                .set_stroke_color(red, green, blue, alpha)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setStrokeWidth)]
        pub fn set_stroke_width(&mut self, width: f64) -> Result<(), JsValue> {
            self.0.set_stroke_width(width).map_err(js_error)
        }

        #[wasm_bindgen(js_name = setStrokeOpacity)]
        pub fn set_stroke_opacity(&mut self, opacity: f64) -> Result<(), JsValue> {
            self.0.set_stroke_opacity(opacity).map_err(js_error)
        }

        #[wasm_bindgen(getter, js_name = strokeOpacity)]
        pub fn stroke_opacity(&self) -> f64 {
            self.0.stroke_opacity()
        }

        #[wasm_bindgen(js_name = setOpacity)]
        pub fn set_opacity(&mut self, opacity: f64) -> Result<(), JsValue> {
            self.0.set_opacity(opacity).map_err(js_error)
        }

        #[wasm_bindgen(js_name = becomeHandle)]
        pub fn become_handle(&mut self, other: &WasmAuthoringMobjectHandle) {
            self.0.become_handle(&other.0);
        }

        #[wasm_bindgen(js_name = replaceHandle)]
        pub fn replace_handle(
            &mut self,
            other: &WasmAuthoringMobjectHandle,
            dim_to_match: u32,
            stretch: bool,
        ) -> Result<(), JsValue> {
            self.0
                .replace_handle(&other.0, dim_to_match, stretch)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = nextToHandle)]
        pub fn next_to_handle(
            &mut self,
            other: &WasmAuthoringMobjectHandle,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
        ) -> Result<(), JsValue> {
            self.0
                .next_to_handle(&other.0, direction_x, direction_y, buff)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = nextToPoint)]
        pub fn next_to_point(
            &mut self,
            point_x: f64,
            point_y: f64,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
        ) -> Result<(), JsValue> {
            self.0
                .next_to_point(point_x, point_y, direction_x, direction_y, buff)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = alignToHandle)]
        pub fn align_to_handle(
            &mut self,
            other: &WasmAuthoringMobjectHandle,
            direction_x: f64,
            direction_y: f64,
        ) -> Result<(), JsValue> {
            self.0
                .align_to_handle(&other.0, direction_x, direction_y)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = alignToPoint)]
        pub fn align_to_point(
            &mut self,
            point_x: f64,
            point_y: f64,
            direction_x: f64,
            direction_y: f64,
        ) -> Result<(), JsValue> {
            self.0
                .align_to_point(point_x, point_y, direction_x, direction_y)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = alignOnFrame)]
        pub fn align_on_frame(
            &mut self,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
        ) -> Result<(), JsValue> {
            self.0
                .align_on_frame(direction_x, direction_y, buff)
                .map_err(js_error)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, ObjectSnapshot, Transform2D, VectorPath};

    use super::*;

    fn snapshot(geometry: GeometryRef) -> ObjectSnapshot {
        ObjectSnapshot {
            geometry,
            transform: Transform2D::default(),
            style: noon_core::Style::default(),
        }
    }

    #[test]
    fn handle_mutations_keep_state_in_shared_rust_semantics() {
        let mut handle = FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::circle(1.0)));
        handle.shift(2.0, -1.0).unwrap();
        handle.scale(1.5, 0.5).unwrap();
        assert_eq!(handle.center(), (2.0, -1.0));
        assert_eq!(handle.width(), 3.0);
        assert_eq!(handle.height(), 1.0);
        assert_eq!(
            handle.snapshot().transform.translation,
            Vec2::new(2.0, -1.0)
        );
    }

    #[test]
    fn authoring_transform_keeps_f64_precision_until_render_lowering() {
        let mut handle =
            FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::rectangle(2.0, 1.0)));
        handle.shift(0.7, 0.3).unwrap();
        assert_eq!(handle.semantic_transform.translation.x, 0.7);
        assert_eq!(handle.semantic_transform.translation.y, 0.3);
        assert!((handle.critical_point(-1.0, 0.0).0 + 0.3).abs() < 1e-12);
        assert!((handle.critical_point(0.0, 1.0).1 - 0.8).abs() < 1e-12);
        assert_ne!(f64::from(handle.snapshot().transform.translation.x), 0.7);

        handle.scale(1.1, 0.9).unwrap();
        handle.rotate(0.2).unwrap();
        assert_eq!(handle.semantic_transform.scale.x, 1.1);
        assert_eq!(handle.semantic_transform.scale.y, 0.9);
        assert_eq!(handle.semantic_transform.rotation_z, 0.2);
    }

    #[test]
    fn pivoted_rotation_preserves_offset_line_center() {
        let mut handle = FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::line(
            Vec2::ZERO,
            Vec2::new(1.0, 0.0),
        )));
        handle.shift(2.0, 0.0).unwrap();
        let pivot = handle.center();
        assert!((pivot.0 - 2.5).abs() < 1e-12);
        assert!(pivot.1.abs() < 1e-12);
        handle
            .rotate_about_point(std::f64::consts::FRAC_PI_2, pivot.0, pivot.1)
            .unwrap();
        let center = handle.center();
        assert!((center.0 - 2.5).abs() < 1e-9);
        assert!(center.1.abs() < 1e-9);
        assert!((handle.semantic_transform.translation.x - 2.5).abs() < 1e-12);
        assert!((handle.semantic_transform.translation.y + 0.5).abs() < 1e-12);
        assert!((handle.semantic_transform.rotation_z - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    }

    #[test]
    fn vector_path_layout_uses_extrema_not_control_hull() {
        let path = VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.0))
            .quadratic_to(Vec2::new(0.0, 2.0), Vec2::new(1.0, 0.0));
        let handle = FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::path(path)));
        let bounds = handle.layout_bounds().unwrap();
        assert!((bounds.min_x + 1.0).abs() < 1e-9);
        assert!((bounds.max_x - 1.0).abs() < 1e-9);
        assert!(bounds.min_y.abs() < 1e-9);
        assert!((bounds.max_y - 1.0).abs() < 1e-9);
        assert!((handle.height() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn layout_operations_are_shared_and_deterministic() {
        let left = FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::circle(0.5)));
        let mut right =
            FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::rectangle(1.0, 1.0)));
        right.next_to_handle(&left, 1.0, 0.0, 0.25).unwrap();
        assert!((right.center().0 - 1.25).abs() < 1e-9);
        right.align_on_frame(1.0, 1.0, 0.5).unwrap();
        let bounds = right.layout_bounds().unwrap();
        assert!(
            (bounds.max_x - (f64::from(noon_core::DEFAULT_FRAME_WIDTH) * 0.5 - 0.5)).abs() < 1e-6
        );
        assert!(
            (bounds.max_y - (f64::from(noon_core::DEFAULT_FRAME_HEIGHT) * 0.5 - 0.5)).abs() < 1e-6
        );
    }

    #[test]
    fn shared_style_mutations_preserve_independent_channels() {
        let mut value = snapshot(GeometryRef::circle(1.0));
        value.style.fill = Some(Color::rgba(1.0, 0.0, 0.0, 0.4));
        value.style.stroke = Some(Color::rgba(0.0, 0.0, 1.0, 0.7));
        let mut handle = FrontendMobjectHandle::from_snapshot(value);

        handle.set_fill_color(0.0, 1.0, 0.0, 1.0).unwrap();
        assert!((handle.fill_opacity() - 0.4).abs() < 1e-6);
        handle.set_fill_opacity(0.25).unwrap();
        handle.set_stroke_width(3.5).unwrap();
        handle.set_stroke_opacity(0.6).unwrap();
        assert_eq!(handle.fill_opacity(), 0.25);
        assert_eq!(handle.stroke_opacity(), 0.6);
        assert!((handle.snapshot().style.stroke_width - 3.5).abs() < 1e-6);
        assert!((handle.snapshot().style.stroke.unwrap().alpha - 0.6).abs() < 1e-6);

        handle.set_opacity(0.2).unwrap();
        assert_eq!(handle.fill_opacity(), 0.2);
        assert_eq!(handle.stroke_opacity(), 0.2);
        handle.disable_fill();
        assert_eq!(handle.fill_opacity(), 0.0);
    }

    #[test]
    fn become_and_replace_keep_state_inside_shared_handle() {
        let mut source = FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::circle(0.5)));
        source.shift(-2.0, 0.5).unwrap();
        let mut target =
            FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::rectangle(2.0, 1.0)));
        target.shift(1.0, -0.25).unwrap();

        source.become_handle(&target);
        assert_eq!(source.snapshot(), target.snapshot());

        let mut replacement =
            FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::circle(0.25)));
        replacement.replace_handle(&target, 0, false).unwrap();
        assert!((replacement.width() - 2.0).abs() < 1e-6);
        assert!((replacement.height() - 2.0).abs() < 1e-6);
        assert!((replacement.center().0 - 1.0).abs() < 1e-6);
        assert!((replacement.center().1 + 0.25).abs() < 1e-6);

        let mut stretched =
            FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::circle(0.25)));
        stretched.replace_handle(&target, 0, true).unwrap();
        assert!((stretched.width() - 2.0).abs() < 1e-6);
        assert!((stretched.height() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn json_round_trip_preserves_wire_snapshot() {
        let handle =
            FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::rectangle(2.0, 3.0)));
        let json = handle.snapshot_json().unwrap();
        let restored = FrontendMobjectHandle::from_json(&json).unwrap();
        assert_eq!(restored, handle);
    }
}
