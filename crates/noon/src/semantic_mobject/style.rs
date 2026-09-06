//! Shared semantic paint and stroke edits.
use super::*;
use noon_core::Style;

pub(crate) trait PaintStyleEdit {
    fn has_fill(&self) -> bool;
    fn has_stroke(&self) -> bool;
    fn set_fill_color(&mut self, color: Color, opacity_when_enabled: f64);
    fn set_stroke_color(&mut self, color: Color, opacity_when_enabled: f64);
    fn set_fill_opacity(&mut self, opacity: f64);
}

impl PaintStyleEdit for SemanticStyle {
    fn has_fill(&self) -> bool {
        self.fill.is_some()
    }

    fn has_stroke(&self) -> bool {
        self.stroke.is_some()
    }

    fn set_fill_color(&mut self, color: Color, opacity_when_enabled: f64) {
        if self.fill.is_none() {
            self.fill_opacity = opacity_when_enabled;
        }
        self.fill = Some(SemanticPaint::Solid(color));
    }

    fn set_stroke_color(&mut self, color: Color, opacity_when_enabled: f64) {
        if self.stroke.is_none() {
            self.stroke_opacity = opacity_when_enabled;
        }
        self.stroke = Some(SemanticPaint::Solid(color));
    }

    fn set_fill_opacity(&mut self, opacity: f64) {
        if self.fill.is_none() {
            self.fill = Some(SemanticPaint::Solid(Color::WHITE));
        }
        self.fill_opacity = opacity;
    }
}

impl PaintStyleEdit for Style {
    fn has_fill(&self) -> bool {
        self.fill.is_some()
    }

    fn has_stroke(&self) -> bool {
        self.stroke.is_some()
    }

    fn set_fill_color(&mut self, color: Color, opacity_when_enabled: f64) {
        let alpha = self
            .fill
            .map_or(opacity_when_enabled as f32, |fill| fill.alpha);
        self.fill = Some(Color { alpha, ..color });
    }

    fn set_stroke_color(&mut self, color: Color, opacity_when_enabled: f64) {
        let alpha = self
            .stroke
            .map_or(opacity_when_enabled as f32, |stroke| stroke.alpha);
        self.stroke = Some(Color { alpha, ..color });
    }

    fn set_fill_opacity(&mut self, opacity: f64) {
        self.fill = Some(Color {
            alpha: opacity as f32,
            ..self.fill.unwrap_or(Color::WHITE)
        });
    }
}

pub(crate) fn edit_object_opacity(style: &mut SemanticStyle, opacity: f64) -> Result<(), String> {
    style.object_opacity = unit_opacity("opacity", opacity)?;
    Ok(())
}

pub(crate) fn edit_color<S: PaintStyleEdit>(
    style: &mut S,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) -> Result<(), String> {
    let color = opaque_color("color", red, green, blue)?;
    let requested_opacity = unit_opacity("color.alpha", alpha)?;
    let had_fill = style.has_fill();
    let had_stroke = style.has_stroke();
    if had_fill {
        style.set_fill_color(color, requested_opacity);
    }
    if had_stroke {
        style.set_stroke_color(color, requested_opacity);
    }
    if !had_fill && !had_stroke {
        style.set_fill_color(color, requested_opacity);
    }
    Ok(())
}

pub(crate) fn edit_disable_fill(style: &mut SemanticStyle) {
    style.fill = None;
}

pub(crate) fn edit_fill_color<S: PaintStyleEdit>(
    style: &mut S,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) -> Result<(), String> {
    let color = opaque_color("fill", red, green, blue)?;
    let requested_opacity = unit_opacity("fill.alpha", alpha)?;
    style.set_fill_color(color, requested_opacity);
    Ok(())
}

pub(crate) fn edit_fill_opacity<S: PaintStyleEdit>(
    style: &mut S,
    opacity: f64,
) -> Result<(), String> {
    let opacity = unit_opacity("fill opacity", opacity)?;
    style.set_fill_opacity(opacity);
    Ok(())
}

pub(crate) fn edit_fill<S: PaintStyleEdit>(
    style: &mut S,
    red: f64,
    green: f64,
    blue: f64,
    opacity: f64,
) -> Result<(), String> {
    let color = opaque_color("fill", red, green, blue)?;
    let opacity = unit_opacity("fill opacity", opacity)?;
    style.set_fill_color(color, opacity);
    style.set_fill_opacity(opacity);
    Ok(())
}

pub(crate) fn edit_manim_opacity(style: &mut SemanticStyle, opacity: f64) -> Result<(), String> {
    let opacity = unit_opacity("opacity", opacity)?;
    if style.fill.is_some() {
        style.fill_opacity = opacity;
    }
    if style.stroke.is_some() {
        style.stroke_opacity = opacity;
    }
    Ok(())
}

pub(crate) fn edit_disable_stroke(style: &mut SemanticStyle) {
    style.stroke = None;
}

pub(crate) fn edit_stroke_color<S: PaintStyleEdit>(
    style: &mut S,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) -> Result<(), String> {
    let color = opaque_color("stroke", red, green, blue)?;
    let requested_opacity = unit_opacity("stroke.alpha", alpha)?;
    style.set_stroke_color(color, requested_opacity);
    Ok(())
}

pub(crate) fn edit_stroke_opacity(style: &mut SemanticStyle, opacity: f64) -> Result<(), String> {
    let opacity = unit_opacity("stroke opacity", opacity)?;
    if style.stroke.is_none() {
        style.stroke = Some(SemanticPaint::Solid(Color::WHITE));
    }
    style.stroke_opacity = opacity;
    Ok(())
}

pub(crate) fn edit_stroke(
    style: &mut SemanticStyle,
    red: f64,
    green: f64,
    blue: f64,
    opacity: f64,
) -> Result<(), String> {
    let color = opaque_color("stroke", red, green, blue)?;
    let opacity = unit_opacity("stroke opacity", opacity)?;
    style.stroke = Some(SemanticPaint::Solid(color));
    style.stroke_opacity = opacity;
    Ok(())
}

pub(super) fn edit_stroke_width(style: &mut SemanticStyle, width: f64) -> Result<(), String> {
    let width = authoring_render_f64("stroke width", width)?;
    if width < 0.0 {
        return Err("stroke width must be non-negative".to_owned());
    }
    style.stroke_width = width;
    if style.stroke.is_none() {
        style.stroke = Some(SemanticPaint::Solid(Color::WHITE));
        style.stroke_opacity = 1.0;
    }
    Ok(())
}

pub(super) fn parse_stroke_width_mode(mode: &str) -> Result<StrokeWidthMode, String> {
    match mode {
        "scale_with_object" => Ok(StrokeWidthMode::ScaleWithObject),
        "screen_space" => Ok(StrokeWidthMode::ScreenSpace),
        _ => Err("stroke_width_mode must be scale_with_object or screen_space".into()),
    }
}

pub(super) fn parse_stroke_join(join: &str) -> Result<StrokeJoin, String> {
    match join {
        "round" => Ok(StrokeJoin::Round),
        "miter" => Ok(StrokeJoin::Miter),
        "bevel" => Ok(StrokeJoin::Bevel),
        _ => Err("stroke_join must be round, miter, or bevel".into()),
    }
}

pub(super) fn parse_stroke_cap(cap: &str) -> Result<StrokeCap, String> {
    match cap {
        "round" => Ok(StrokeCap::Round),
        "butt" => Ok(StrokeCap::Butt),
        "square" => Ok(StrokeCap::Square),
        _ => Err("stroke_cap must be round, butt, or square".into()),
    }
}

impl Mobject {
    pub fn set_stroke_width_mode(&mut self, mode: &str) -> Result<(), String> {
        let mut state = self.state()?;
        state.style.stroke_width_mode = parse_stroke_width_mode(mode)?;
        self.commit_state(state)
    }
    pub fn set_stroke_join(&mut self, join: &str) -> Result<(), String> {
        let mut state = self.state()?;
        state.style.stroke_join = parse_stroke_join(join)?;
        self.commit_state(state)
    }
    pub fn set_stroke_cap(&mut self, cap: &str) -> Result<(), String> {
        let mut state = self.state()?;
        state.style.stroke_cap = parse_stroke_cap(cap)?;
        self.commit_state(state)
    }
    pub fn set_object_opacity(&mut self, opacity: f64) -> Result<(), String> {
        let mut state = self.state()?;
        edit_object_opacity(&mut state.style, opacity)?;
        self.commit_state(state)
    }
    pub fn set_color(&mut self, red: f64, green: f64, blue: f64, alpha: f64) -> Result<(), String> {
        self.validate()?;
        let mut state = self.state()?;
        edit_color(&mut state.style, red, green, blue, alpha)?;
        self.commit_state(state)
    }
    pub fn disable_fill(&mut self) -> Result<(), String> {
        self.validate()?;
        let mut state = self.state()?;
        edit_disable_fill(&mut state.style);
        self.commit_state(state)
    }
    pub fn set_fill_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), String> {
        self.validate()?;
        let mut state = self.state()?;
        edit_fill_color(&mut state.style, red, green, blue, alpha)?;
        self.commit_state(state)
    }
    pub fn set_fill_opacity(&mut self, opacity: f64) -> Result<(), String> {
        self.validate()?;
        let mut state = self.state()?;
        edit_fill_opacity(&mut state.style, opacity)?;
        self.commit_state(state)
    }
    pub fn set_fill(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        opacity: f64,
    ) -> Result<(), String> {
        self.validate()?;
        let mut state = self.state()?;
        edit_fill(&mut state.style, red, green, blue, opacity)?;
        self.commit_state(state)
    }
    pub fn fill_opacity(&self) -> Result<f64, String> {
        let state = self.state()?;
        Ok(if state.style.fill.is_some() {
            state.style.fill_opacity
        } else {
            0.0
        })
    }
    pub fn disable_stroke(&mut self) -> Result<(), String> {
        self.validate()?;
        let mut state = self.state()?;
        edit_disable_stroke(&mut state.style);
        self.commit_state(state)
    }
    pub fn set_stroke_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), String> {
        self.validate()?;
        let mut state = self.state()?;
        edit_stroke_color(&mut state.style, red, green, blue, alpha)?;
        self.commit_state(state)
    }
    pub fn set_stroke_width(&mut self, width: f64) -> Result<(), String> {
        self.validate()?;
        let mut state = self.state()?;
        edit_stroke_width(&mut state.style, width)?;
        self.commit_state(state)
    }
    pub fn set_stroke_opacity(&mut self, opacity: f64) -> Result<(), String> {
        self.validate()?;
        let mut state = self.state()?;
        edit_stroke_opacity(&mut state.style, opacity)?;
        self.commit_state(state)
    }
    pub fn stroke_opacity(&self) -> Result<f64, String> {
        let state = self.state()?;
        Ok(if state.style.stroke.is_some() {
            state.style.stroke_opacity
        } else {
            0.0
        })
    }
    pub fn set_opacity(&mut self, opacity: f64) -> Result<(), String> {
        self.validate()?;
        let mut state = self.state()?;
        edit_manim_opacity(&mut state.style, opacity)?;
        self.commit_state(state)
    }
}
