use noon_core::{
    semantic_path_bounds, Bounds2D64, Color, GeometryRef, ObjectSnapshot, SemanticVec3, Vec2,
};

#[derive(Clone, Debug, PartialEq)]
pub struct FrontendMobjectHandle {
    snapshot: ObjectSnapshot,
}

impl FrontendMobjectHandle {
    pub fn from_snapshot(snapshot: ObjectSnapshot) -> Self {
        Self { snapshot }
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
        snapshot_layout_bounds(&self.snapshot)
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
                let translation = self.snapshot.transform.translation;
                (translation.x as f64, translation.y as f64)
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
        let offset = semantic_xy(x, y)?;
        self.snapshot.transform.translation += offset;
        Ok(())
    }

    pub fn move_to(&mut self, x: f64, y: f64) -> Result<(), String> {
        semantic_xy(x, y)?;
        let center = self.center();
        self.shift(x - center.0, y - center.1)
    }

    pub fn scale(&mut self, x: f64, y: f64) -> Result<(), String> {
        let x = finite_f32("scale.x", x)?;
        let y = finite_f32("scale.y", y)?;
        self.snapshot.transform.scale = self
            .snapshot
            .transform
            .scale
            .component_mul(Vec2::new(x, y));
        if !self.snapshot.transform.scale.x.is_finite() || !self.snapshot.transform.scale.y.is_finite()
        {
            return Err("scale result must be finite".to_owned());
        }
        Ok(())
    }

    pub fn rotate(&mut self, angle: f64) -> Result<(), String> {
        let angle = finite_f32("rotation", angle)?;
        self.snapshot.transform.rotation += angle;
        if !self.snapshot.transform.rotation.is_finite() {
            return Err("rotation result must be finite".to_owned());
        }
        Ok(())
    }

    pub fn set_color(&mut self, red: f64, green: f64, blue: f64, alpha: f64) -> Result<(), String> {
        let color = Color::rgba(
            finite_f32("color.red", red)?,
            finite_f32("color.green", green)?,
            finite_f32("color.blue", blue)?,
            finite_f32("color.alpha", alpha)?,
        );
        if self.snapshot.style.fill.is_some() {
            self.snapshot.style.fill = Some(color);
        }
        if self.snapshot.style.stroke.is_some() {
            self.snapshot.style.stroke = Some(color);
        }
        if self.snapshot.style.fill.is_none() && self.snapshot.style.stroke.is_none() {
            self.snapshot.style.fill = Some(color);
        }
        Ok(())
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
            if direction_x == 0.0 { 0.0 } else { target.0 - source.0 },
            if direction_y == 0.0 { 0.0 } else { target.1 - source.1 },
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
            if direction_x == 0.0 { 0.0 } else { point_x - source.0 },
            if direction_y == 0.0 { 0.0 } else { point_y - source.1 },
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
    if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
        return Err(format!("{name} must be a finite f32-compatible number"));
    }
    Ok(value as f32)
}

fn semantic_xy(x: f64, y: f64) -> Result<Vec2, String> {
    SemanticVec3::new(x, y, 0.0)
        .lower_xy_f32()
        .map_err(|error| error.to_string())
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

fn snapshot_layout_bounds(snapshot: &ObjectSnapshot) -> Option<Bounds2D64> {
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

    let transform = snapshot.transform;
    let sine = f64::from(transform.rotation).sin();
    let cosine = f64::from(transform.rotation).cos();
    let scale_x = f64::from(transform.scale.x);
    let scale_y = f64::from(transform.scale.y);
    let translation_x = f64::from(transform.translation.x);
    let translation_y = f64::from(transform.translation.y);
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

    #[wasm_bindgen(js_name = AuthoringMobjectHandle)]
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
        assert_eq!(handle.snapshot().transform.translation, Vec2::new(2.0, -1.0));
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
        let mut right = FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::rectangle(1.0, 1.0)));
        right.next_to_handle(&left, 1.0, 0.0, 0.25).unwrap();
        assert!((right.center().0 - 1.25).abs() < 1e-9);
        right.align_on_frame(1.0, 1.0, 0.5).unwrap();
        let bounds = right.layout_bounds().unwrap();
        assert!((bounds.max_x - (f64::from(noon_core::DEFAULT_FRAME_WIDTH) * 0.5 - 0.5)).abs() < 1e-6);
        assert!((bounds.max_y - (f64::from(noon_core::DEFAULT_FRAME_HEIGHT) * 0.5 - 0.5)).abs() < 1e-6);
    }

    #[test]
    fn json_round_trip_preserves_wire_snapshot() {
        let handle = FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::rectangle(2.0, 3.0)));
        let json = handle.snapshot_json().unwrap();
        let restored = FrontendMobjectHandle::from_json(&json).unwrap();
        assert_eq!(restored, handle);
    }
}
