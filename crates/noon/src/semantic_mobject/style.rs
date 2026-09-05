//! Shared semantic paint and stroke edits.
use super::*;
impl Mobject {
    pub fn set_stroke_width_mode(&mut self, mode: &str) -> Result<(), String> {
        let mut state = self.state()?;
        state.style.stroke_width_mode = match mode {
            "scale_with_object" => StrokeWidthMode::ScaleWithObject,
            "screen_space" => StrokeWidthMode::ScreenSpace,
            _ => return Err("stroke_width_mode must be scale_with_object or screen_space".into()),
        };
        self.commit_state(state)
    }
    pub fn set_stroke_join(&mut self, join: &str) -> Result<(), String> {
        let mut state = self.state()?;
        state.style.stroke_join = match join {
            "round" => StrokeJoin::Round,
            "miter" => StrokeJoin::Miter,
            "bevel" => StrokeJoin::Bevel,
            _ => return Err("stroke_join must be round, miter, or bevel".into()),
        };
        self.commit_state(state)
    }
    pub fn set_stroke_cap(&mut self, cap: &str) -> Result<(), String> {
        let mut state = self.state()?;
        state.style.stroke_cap = match cap {
            "round" => StrokeCap::Round,
            "butt" => StrokeCap::Butt,
            "square" => StrokeCap::Square,
            _ => return Err("stroke_cap must be round, butt, or square".into()),
        };
        self.commit_state(state)
    }
    pub fn set_object_opacity(&mut self, opacity: f64) -> Result<(), String> {
        let mut state = self.state()?;
        state.style.object_opacity = unit_opacity("opacity", opacity)?;
        self.commit_state(state)
    }
    pub fn set_color(&mut self, red: f64, green: f64, blue: f64, alpha: f64) -> Result<(), String> {
        self.validate()?;
        let mut state = self.state()?;
        let color = opaque_color("color", red, green, blue)?;
        let opacity = unit_opacity("color.alpha", alpha)?;
        let had_fill = state.style.fill.is_some();
        let had_stroke = state.style.stroke.is_some();
        if had_fill {
            state.style.fill = Some(SemanticPaint::Solid(color));
            state.style.fill_opacity = opacity;
        }
        if had_stroke {
            state.style.stroke = Some(SemanticPaint::Solid(color));
            state.style.stroke_opacity = opacity;
        }
        if !had_fill && !had_stroke {
            state.style.fill = Some(SemanticPaint::Solid(color));
            state.style.fill_opacity = opacity;
        }
        self.commit_state(state)
    }
    pub fn disable_fill(&mut self) -> Result<(), String> {
        self.validate()?;
        let mut state = self.state()?;
        state.style.fill = None;
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
        let color = opaque_color("fill", red, green, blue)?;
        let requested_opacity = unit_opacity("fill.alpha", alpha)?;
        if state.style.fill.is_none() {
            state.style.fill_opacity = requested_opacity;
        }
        state.style.fill = Some(SemanticPaint::Solid(color));
        self.commit_state(state)
    }
    pub fn set_fill_opacity(&mut self, opacity: f64) -> Result<(), String> {
        self.validate()?;
        let mut state = self.state()?;
        let opacity = unit_opacity("fill opacity", opacity)?;
        if state.style.fill.is_none() {
            state.style.fill = Some(SemanticPaint::Solid(Color::WHITE));
        }
        state.style.fill_opacity = opacity;
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
        let color = opaque_color("fill", red, green, blue)?;
        let opacity = unit_opacity("fill opacity", opacity)?;
        state.style.fill = Some(SemanticPaint::Solid(color));
        state.style.fill_opacity = opacity;
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
        state.style.stroke = None;
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
        let color = opaque_color("stroke", red, green, blue)?;
        let requested_opacity = unit_opacity("stroke.alpha", alpha)?;
        if state.style.stroke.is_none() {
            state.style.stroke_opacity = requested_opacity;
        }
        state.style.stroke = Some(SemanticPaint::Solid(color));
        self.commit_state(state)
    }
    pub fn set_stroke_width(&mut self, width: f64) -> Result<(), String> {
        self.validate()?;
        let mut state = self.state()?;
        let width = authoring_render_f64("stroke width", width)?;
        if width < 0.0 {
            return Err("stroke width must be non-negative".to_owned());
        }
        state.style.stroke_width = width;
        if state.style.stroke.is_none() {
            state.style.stroke = Some(SemanticPaint::Solid(Color::WHITE));
            state.style.stroke_opacity = 1.0;
        }
        self.commit_state(state)
    }
    pub fn set_stroke_opacity(&mut self, opacity: f64) -> Result<(), String> {
        self.validate()?;
        let mut state = self.state()?;
        let opacity = unit_opacity("stroke opacity", opacity)?;
        if state.style.stroke.is_none() {
            state.style.stroke = Some(SemanticPaint::Solid(Color::WHITE));
        }
        state.style.stroke_opacity = opacity;
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
        let opacity = unit_opacity("opacity", opacity)?;
        if state.style.fill.is_some() {
            state.style.fill_opacity = opacity;
        }
        if state.style.stroke.is_some() {
            state.style.stroke_opacity = opacity;
        }
        self.commit_state(state)
    }
}
