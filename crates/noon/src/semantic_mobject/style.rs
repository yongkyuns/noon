//! Shared semantic paint and stroke edits.
use super::*;
use noon_core::Style;

pub(super) fn manim_color_from_semantic(style: &SemanticStyle) -> Result<Color, String> {
    let selected = style.stroke.as_ref().or(style.fill.as_ref());
    let opacity = if style.stroke.is_some() {
        style.stroke_opacity
    } else {
        style.fill_opacity
    };
    match selected {
        Some(SemanticPaint::Solid(_)) => Ok(solid_color_with_opacity(selected, opacity)
            .expect("selected solid paint produces one color")),
        Some(SemanticPaint::Resource(_)) => {
            Err("Manim color queries do not support resource paints".into())
        }
        None => Ok(Color::WHITE),
    }
}

/// Observable paint alpha is the solid color alpha times its authored multiplier.
/// Object-composite opacity is a separate domain. A resource paint has no single
/// scalar color alpha and cannot be represented by a Manim opacity getter.
fn manim_paint_opacity(paint: Option<&SemanticPaint>, opacity: f64) -> Result<f64, String> {
    match paint {
        Some(SemanticPaint::Solid(color)) => Ok(f64::from(color.alpha) * opacity),
        Some(SemanticPaint::Resource(_)) => {
            Err("Manim opacity queries do not support resource paints".into())
        }
        None => Ok(0.0),
    }
}

pub(crate) fn manim_color_from_effective(style: &Style) -> Color {
    style.stroke.or(style.fill).unwrap_or(Color::WHITE)
}

pub(crate) trait PaintStyleEdit {
    fn has_fill(&self) -> bool;
    fn has_stroke(&self) -> bool;
    fn set_fill_color(&mut self, color: Color, opacity_when_enabled: f64);
    fn set_stroke_color(&mut self, color: Color, opacity_when_enabled: f64);
    fn set_fill_opacity(&mut self, opacity: f64);
}

// A captured effective solid paint can carry alpha in its color as well as
// its semantic multiplier. Preserve their product on RGB edits; absolute
// Manim opacity writes normalize the intrinsic alpha before setting it.
fn replace_paint_color(
    paint: &mut Option<SemanticPaint>,
    opacity: &mut f64,
    color: Color,
    opacity_when_enabled: f64,
) {
    *opacity = match paint {
        None => opacity_when_enabled,
        Some(SemanticPaint::Solid(previous)) => f64::from(previous.alpha) * *opacity,
        Some(SemanticPaint::Resource(_)) => *opacity,
    };
    *paint = Some(SemanticPaint::Solid(color));
}

fn set_paint_opacity(paint: &mut Option<SemanticPaint>, multiplier: &mut f64, opacity: f64) {
    if let SemanticPaint::Solid(color) = paint.get_or_insert(SemanticPaint::Solid(Color::WHITE)) {
        color.alpha = 1.0;
    }
    *multiplier = opacity;
}

impl PaintStyleEdit for SemanticStyle {
    fn has_fill(&self) -> bool {
        self.fill.is_some()
    }

    fn has_stroke(&self) -> bool {
        self.stroke.is_some()
    }

    fn set_fill_color(&mut self, color: Color, opacity_when_enabled: f64) {
        replace_paint_color(
            &mut self.fill,
            &mut self.fill_opacity,
            color,
            opacity_when_enabled,
        );
    }

    fn set_stroke_color(&mut self, color: Color, opacity_when_enabled: f64) {
        replace_paint_color(
            &mut self.stroke,
            &mut self.stroke_opacity,
            color,
            opacity_when_enabled,
        );
    }

    fn set_fill_opacity(&mut self, opacity: f64) {
        set_paint_opacity(&mut self.fill, &mut self.fill_opacity, opacity);
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
        set_paint_opacity(&mut style.fill, &mut style.fill_opacity, opacity);
    }
    if style.stroke.is_some() {
        set_paint_opacity(&mut style.stroke, &mut style.stroke_opacity, opacity);
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
    set_paint_opacity(&mut style.stroke, &mut style.stroke_opacity, opacity);
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

pub(crate) fn edit_stroke_width(style: &mut SemanticStyle, width: f64) -> Result<(), String> {
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
        manim_paint_opacity(state.style.fill.as_ref(), state.style.fill_opacity)
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
        manim_paint_opacity(state.style.stroke.as_ref(), state.style.stroke_opacity)
    }
    pub fn set_opacity(&mut self, opacity: f64) -> Result<(), String> {
        self.validate()?;
        let mut state = self.state()?;
        edit_manim_opacity(&mut state.style, opacity)?;
        self.commit_state(state)
    }
}

#[cfg(test)]
mod opacity_tests {
    use super::*;

    #[test]
    fn authored_paint_edits_agree_with_effective_edits_for_intrinsic_alpha() {
        let mut authored = SemanticStyle {
            fill: Some(SemanticPaint::Solid(Color::rgba(0.1, 0.2, 0.3, 0.5))),
            fill_opacity: 0.25,
            stroke: Some(SemanticPaint::Solid(Color::rgba(0.4, 0.5, 0.6, 0.0))),
            stroke_opacity: 0.75,
            object_opacity: 0.3,
            ..SemanticStyle::default()
        };
        let mut absolute = authored.clone();
        edit_fill_opacity(&mut absolute, 0.6).unwrap();
        edit_stroke_opacity(&mut absolute, 0.4).unwrap();
        assert_eq!(
            manim_paint_opacity(absolute.fill.as_ref(), absolute.fill_opacity).unwrap(),
            0.6
        );
        assert_eq!(
            manim_paint_opacity(absolute.stroke.as_ref(), absolute.stroke_opacity).unwrap(),
            0.4
        );
        let mut combined = authored.clone();
        edit_manim_opacity(&mut combined, 0.2).unwrap();
        assert_eq!(
            manim_paint_opacity(combined.fill.as_ref(), combined.fill_opacity).unwrap(),
            0.2
        );
        assert_eq!(
            manim_paint_opacity(combined.stroke.as_ref(), combined.stroke_opacity).unwrap(),
            0.2
        );
        let mut effective = Style {
            fill: solid_color_with_opacity(authored.fill.as_ref(), authored.fill_opacity),
            stroke: solid_color_with_opacity(authored.stroke.as_ref(), authored.stroke_opacity),
            opacity: authored.object_opacity as f32,
            ..Style::default()
        };
        edit_color(&mut authored, 0.8, 0.4, 0.2, 0.9).unwrap();
        edit_color(&mut effective, 0.8, 0.4, 0.2, 0.9).unwrap();
        assert_eq!(
            solid_color_with_opacity(authored.fill.as_ref(), authored.fill_opacity),
            effective.fill
        );
        assert_eq!(
            solid_color_with_opacity(authored.stroke.as_ref(), authored.stroke_opacity),
            effective.stroke
        );
        assert_eq!(authored.fill_opacity, 0.125);
        assert_eq!(authored.stroke_opacity, 0.0);

        edit_fill_opacity(&mut authored, 0.6).unwrap();
        edit_fill_opacity(&mut effective, 0.6).unwrap();
        assert_eq!(
            solid_color_with_opacity(authored.fill.as_ref(), authored.fill_opacity),
            effective.fill
        );
        edit_stroke_opacity(&mut authored, 0.4).unwrap();
        assert_eq!(
            manim_paint_opacity(authored.stroke.as_ref(), authored.stroke_opacity).unwrap(),
            0.4
        );
        edit_manim_opacity(&mut authored, 0.2).unwrap();
        assert_eq!(
            manim_paint_opacity(authored.fill.as_ref(), authored.fill_opacity).unwrap(),
            0.2
        );
        assert_eq!(
            manim_paint_opacity(authored.stroke.as_ref(), authored.stroke_opacity).unwrap(),
            0.2
        );
        assert_eq!(authored.object_opacity, 0.3);

        let mut resource = SemanticStyle {
            fill: Some(SemanticPaint::Resource(7)),
            stroke: None,
            ..SemanticStyle::default()
        };
        edit_manim_opacity(&mut resource, 0.4).unwrap();
        assert_eq!(resource.fill, Some(SemanticPaint::Resource(7)));
        assert_eq!(resource.fill_opacity, 0.4);
        assert_eq!(resource.stroke, None);
    }

    #[test]
    fn paint_opacity_observes_intrinsic_alpha_and_disabled_paints() {
        let solid = SemanticPaint::Solid(Color::rgba(0.2, 0.4, 0.8, 0.5));
        assert_eq!(manim_paint_opacity(Some(&solid), 0.25).unwrap(), 0.125);
        assert_eq!(manim_paint_opacity(Some(&solid), 1.0).unwrap(), 0.5);
        assert_eq!(manim_paint_opacity(None, 1.0).unwrap(), 0.0);
        assert!(manim_paint_opacity(Some(&SemanticPaint::Resource(7)), 0.5)
            .unwrap_err()
            .contains("resource paints"));
    }
}
