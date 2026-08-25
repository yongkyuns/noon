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
''',
    '''use noon_core::{
    semantic_path_bounds, Bounds2D64, Color, GeometryRef, ObjectSnapshot, SemanticPaint,
    SemanticStyle, SemanticVec3, Style, Vec2,
};

#[derive(Clone, Debug, PartialEq)]
pub struct FrontendMobjectHandle {
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
    "handle semantic style field",
)
style_start = text.index("    pub fn set_color(")
style_end = text.index("    pub fn become_handle(", style_start)
new_style_block = '''    pub fn set_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), String> {
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
        self.snapshot.style.opacity = self.semantic_style.object_opacity as f32;
    }

'''
text = text[:style_start] + new_style_block + text[style_end:]
text = replace_once(
    text,
    '''    pub fn become_handle(&mut self, other: &Self) {
        self.snapshot = other.snapshot.clone();
    }
''',
    '''    pub fn become_handle(&mut self, other: &Self) {
        self.snapshot = other.snapshot.clone();
        self.semantic_style = other.semantic_style.clone();
    }
''',
    "become semantic style copy",
)
text = replace_once(
    text,
    '''fn finite_f32(name: &str, value: f64) -> Result<f32, String> {
    if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
        return Err(format!("{name} must be a finite f32-compatible number"));
    }
    Ok(value as f32)
}

fn unit_alpha(name: &str, value: f64) -> Result<f32, String> {
    let value = finite_f32(name, value)?;
    if !(0.0..=1.0).contains(&value) {
        return Err(format!("{name} must be between 0 and 1"));
    }
    Ok(value)
}
''',
    '''fn finite_f32(name: &str, value: f64) -> Result<f32, String> {
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
''',
    "style precision helpers",
)
text = replace_once(
    text,
    '''        handle.set_fill_opacity(0.25).unwrap();
        handle.set_stroke_width(3.5).unwrap();
        handle.set_stroke_opacity(0.6).unwrap();
        assert!((handle.fill_opacity() - 0.25).abs() < 1e-6);
        assert!((handle.stroke_opacity() - 0.6).abs() < 1e-6);
        assert!((handle.snapshot().style.stroke_width - 3.5).abs() < 1e-6);

        handle.set_opacity(0.2).unwrap();
        assert!((handle.fill_opacity() - 0.2).abs() < 1e-6);
        assert!((handle.stroke_opacity() - 0.2).abs() < 1e-6);
''',
    '''        handle.set_fill_opacity(0.25).unwrap();
        handle.set_stroke_width(3.5).unwrap();
        handle.set_stroke_opacity(0.6).unwrap();
        assert_eq!(handle.fill_opacity(), 0.25);
        assert_eq!(handle.stroke_opacity(), 0.6);
        assert!((handle.snapshot().style.stroke_width - 3.5).abs() < 1e-6);
        assert!((handle.snapshot().style.stroke.unwrap().alpha - 0.6).abs() < 1e-6);

        handle.set_opacity(0.2).unwrap();
        assert_eq!(handle.fill_opacity(), 0.2);
        assert_eq!(handle.stroke_opacity(), 0.2);
''',
    "tight style precision assertions",
)
path.write_text(text)
