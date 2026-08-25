from pathlib import Path


def replace_once(text: str, before: str, after: str, label: str) -> str:
    count = text.count(before)
    if count != 1:
        raise SystemExit(f"{label}: expected one match, found {count}")
    return text.replace(before, after, 1)


path = Path("crates/noon-web/src/authoring_mobject.rs")
text = path.read_text()

text = replace_once(
    text,
    '''use noon_core::{
    semantic_path_bounds, Bounds2D64, Color, GeometryRef, ObjectSnapshot, SemanticPaint,
    SemanticStyle, SemanticVec3, Style, Vec2,
};
''',
    '''use noon_core::{
    semantic_path_bounds, Bounds2D64, Color, GeometryRef, ObjectSnapshot, SemanticPaint,
    SemanticStyle, SemanticTransform2_5D, SemanticVec3, Style, Vec2,
};
''',
    "semantic transform import",
)

text = replace_once(
    text,
    '''pub struct FrontendMobjectHandle {
    snapshot: ObjectSnapshot,
    semantic_style: SemanticStyle,
}

impl FrontendMobjectHandle {
    pub fn from_snapshot(snapshot: ObjectSnapshot) -> Self {
        let semantic_style = authoring_style_from_legacy(snapshot.style);
        Self {
            snapshot,
            semantic_style,
        }
    }
''',
    '''pub struct FrontendMobjectHandle {
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
''',
    "semantic transform field",
)

text = replace_once(
    text,
    '''    pub fn layout_bounds(&self) -> Option<Bounds2D64> {
        snapshot_layout_bounds(&self.snapshot)
    }
''',
    '''    pub fn layout_bounds(&self) -> Option<Bounds2D64> {
        snapshot_layout_bounds(&self.snapshot, self.semantic_transform)
    }
''',
    "semantic layout transform",
)

text = replace_once(
    text,
    '''            .unwrap_or_else(|| {
                let translation = self.snapshot.transform.translation;
                (translation.x as f64, translation.y as f64)
            })
''',
    '''            .unwrap_or_else(|| {
                let translation = self.semantic_transform.translation;
                (translation.x, translation.y)
            })
''',
    "semantic center fallback",
)

text = replace_once(
    text,
    '''    pub fn shift(&mut self, x: f64, y: f64) -> Result<(), String> {
        let offset = semantic_xy(x, y)?;
        self.snapshot.transform.translation += offset;
        Ok(())
    }
''',
    '''    pub fn shift(&mut self, x: f64, y: f64) -> Result<(), String> {
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
''',
    "semantic shift",
)

text = replace_once(
    text,
    '''    pub fn scale(&mut self, x: f64, y: f64) -> Result<(), String> {
        let x = finite_f32("scale.x", x)?;
        let y = finite_f32("scale.y", y)?;
        self.snapshot.transform.scale =
            self.snapshot.transform.scale.component_mul(Vec2::new(x, y));
        if !self.snapshot.transform.scale.x.is_finite()
            || !self.snapshot.transform.scale.y.is_finite()
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
''',
    '''    pub fn scale(&mut self, x: f64, y: f64) -> Result<(), String> {
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
''',
    "semantic scale and rotation",
)

text = replace_once(
    text,
    '''    fn sync_legacy_style(&mut self) {
''',
    '''    fn sync_legacy_transform(&mut self) -> Result<(), String> {
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
''',
    "legacy transform sync",
)

text = replace_once(
    text,
    '''    pub fn become_handle(&mut self, other: &Self) {
        self.snapshot = other.snapshot.clone();
        self.semantic_style = other.semantic_style.clone();
    }
''',
    '''    pub fn become_handle(&mut self, other: &Self) {
        self.snapshot = other.snapshot.clone();
        self.semantic_transform = other.semantic_transform;
        self.semantic_style = other.semantic_style.clone();
    }
''',
    "become semantic transform copy",
)

text = replace_once(
    text,
    '''fn semantic_xy(x: f64, y: f64) -> Result<Vec2, String> {
    SemanticVec3::new(x, y, 0.0)
        .lower_xy_f32()
        .map_err(|error| error.to_string())
}
''',
    '''fn semantic_xy(x: f64, y: f64) -> Result<Vec2, String> {
    semantic_xy_f64(x, y)?
        .lower_xy_f32()
        .map_err(|error| error.to_string())
}

fn semantic_xy_f64(x: f64, y: f64) -> Result<SemanticVec3, String> {
    let value = SemanticVec3::new(x, y, 0.0);
    value
        .lower_xy_f32()
        .map_err(|error| error.to_string())?;
    Ok(value)
}
''',
    "semantic xy precision helper",
)

text = replace_once(
    text,
    '''fn snapshot_layout_bounds(snapshot: &ObjectSnapshot) -> Option<Bounds2D64> {
''',
    '''fn snapshot_layout_bounds(
    snapshot: &ObjectSnapshot,
    transform: SemanticTransform2_5D,
) -> Option<Bounds2D64> {
''',
    "semantic bounds signature",
)

text = replace_once(
    text,
    '''    let transform = snapshot.transform;
    let sine = f64::from(transform.rotation).sin();
    let cosine = f64::from(transform.rotation).cos();
    let scale_x = f64::from(transform.scale.x);
    let scale_y = f64::from(transform.scale.y);
    let translation_x = f64::from(transform.translation.x);
    let translation_y = f64::from(transform.translation.y);
''',
    '''    let sine = transform.rotation_z.sin();
    let cosine = transform.rotation_z.cos();
    let scale_x = transform.scale.x;
    let scale_y = transform.scale.y;
    let translation_x = transform.translation.x;
    let translation_y = transform.translation.y;
''',
    "semantic bounds values",
)

text = replace_once(
    text,
    '''    #[test]
    fn vector_path_layout_uses_extrema_not_control_hull() {
''',
    '''    #[test]
    fn authoring_transform_keeps_f64_precision_until_render_lowering() {
        let mut handle = FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::rectangle(2.0, 1.0)));
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
    fn vector_path_layout_uses_extrema_not_control_hull() {
''',
    "semantic transform precision test",
)

path.write_text(text)
