//! Shared semantic paint and stroke edits.
use super::*;
use noon_core::Style;

pub(crate) fn paint_color(paint: Option<&SemanticPaint>) -> Result<Option<Color>, AuthoringError> {
    match paint {
        Some(SemanticPaint::Solid(color)) => {
            Ok(Some(Color::rgb(color.red, color.green, color.blue)))
        }
        Some(
            SemanticPaint::LinearGradient(_)
            | SemanticPaint::RadialGradient(_)
            | SemanticPaint::Noise(_)
            | SemanticPaint::Hatch(_)
            | SemanticPaint::Image(_),
        ) => Err(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::ResourcePaintColorQuery,
        )),
        None => Ok(None),
    }
}

pub(super) fn manim_color_from_semantic(style: &SemanticStyle) -> Result<Color, AuthoringError> {
    let fill_visible = match style.fill.as_ref() {
        Some(SemanticPaint::Solid(color)) => f64::from(color.alpha) * style.fill_opacity > 0.0,
        Some(
            SemanticPaint::LinearGradient(_)
            | SemanticPaint::RadialGradient(_)
            | SemanticPaint::Noise(_)
            | SemanticPaint::Hatch(_)
            | SemanticPaint::Image(_),
        ) => style.fill_opacity > 0.0,
        None => false,
    };
    paint_color(if fill_visible {
        style.fill.as_ref()
    } else {
        style.stroke.as_ref()
    })
    .map(|color| color.unwrap_or(Color::WHITE))
}

/// Observable paint alpha is the solid color alpha times its authored multiplier.
/// A continuous paint reports its independent multiplier; intrinsic stop alpha
/// varies spatially. Image paints likewise report their independent multiplier.
fn manim_paint_opacity(paint: Option<&SemanticPaint>, opacity: f64) -> Result<f64, AuthoringError> {
    match paint {
        Some(SemanticPaint::Solid(color)) => Ok(f64::from(color.alpha) * opacity),
        Some(
            SemanticPaint::LinearGradient(_)
            | SemanticPaint::RadialGradient(_)
            | SemanticPaint::Noise(_)
            | SemanticPaint::Hatch(_)
            | SemanticPaint::Image(_),
        ) => Ok(opacity),
        None => Ok(0.0),
    }
}

pub(crate) fn opaque_paint_color(color: Color) -> Color {
    Color::rgb(color.red, color.green, color.blue)
}

pub(crate) fn manim_color_from_effective(style: &Style) -> Color {
    style
        .fill
        .filter(|color| color.alpha > 0.0)
        .or(style.stroke)
        .map(opaque_paint_color)
        .unwrap_or(Color::WHITE)
}

pub(crate) trait PaintStyleEdit {
    fn set_emission(&mut self, emission: noon_core::Emission);
    fn has_fill(&self) -> bool;
    fn has_stroke(&self) -> bool;
    fn has_background_stroke(&self) -> bool;
    fn manim_color(&self) -> Result<Color, AuthoringError>;
    fn set_fill_color(&mut self, color: Color, opacity_when_enabled: f64);
    fn set_stroke_color(&mut self, color: Color, opacity_when_enabled: f64);
    fn set_fill_opacity(&mut self, opacity: f64);
    fn disable_fill(&mut self);
    fn disable_stroke(&mut self);
    fn set_stroke_opacity(&mut self, opacity: f64);
    fn scale_fill_opacity(&mut self, factor: f64);
    fn scale_stroke_opacity(&mut self, factor: f64);
    fn scale_background_stroke_opacity(&mut self, factor: f64);
    fn set_stroke_width(&mut self, width: f64);
    fn set_background_stroke_color(&mut self, color: Color, opacity_when_enabled: f64);
    fn set_background_stroke_opacity(&mut self, opacity: f64);
    fn set_background_stroke_width(&mut self, width: f64);
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
        Some(
            SemanticPaint::LinearGradient(_)
            | SemanticPaint::RadialGradient(_)
            | SemanticPaint::Noise(_)
            | SemanticPaint::Hatch(_)
            | SemanticPaint::Image(_),
        ) => *opacity,
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
    fn set_emission(&mut self, emission: noon_core::Emission) {
        self.emission = emission;
    }

    fn has_fill(&self) -> bool {
        self.fill.is_some()
    }

    fn has_stroke(&self) -> bool {
        self.stroke.is_some()
    }

    fn has_background_stroke(&self) -> bool {
        self.background_stroke.is_some()
    }

    fn manim_color(&self) -> Result<Color, AuthoringError> {
        manim_color_from_semantic(self)
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
    fn disable_fill(&mut self) {
        self.fill = None;
    }
    fn disable_stroke(&mut self) {
        self.stroke = None;
    }
    fn set_stroke_opacity(&mut self, opacity: f64) {
        set_paint_opacity(&mut self.stroke, &mut self.stroke_opacity, opacity);
    }
    fn scale_fill_opacity(&mut self, factor: f64) {
        self.fill_opacity *= factor;
    }
    fn scale_stroke_opacity(&mut self, factor: f64) {
        self.stroke_opacity *= factor;
    }
    fn scale_background_stroke_opacity(&mut self, factor: f64) {
        self.background_stroke_opacity *= factor;
    }
    fn set_stroke_width(&mut self, width: f64) {
        self.stroke_width = width;
    }

    fn set_background_stroke_color(&mut self, color: Color, opacity_when_enabled: f64) {
        replace_paint_color(
            &mut self.background_stroke,
            &mut self.background_stroke_opacity,
            color,
            opacity_when_enabled,
        );
    }

    fn set_background_stroke_opacity(&mut self, opacity: f64) {
        set_paint_opacity(
            &mut self.background_stroke,
            &mut self.background_stroke_opacity,
            opacity,
        );
    }

    fn set_background_stroke_width(&mut self, width: f64) {
        self.background_stroke_width = width;
    }
}

impl PaintStyleEdit for Style {
    fn set_emission(&mut self, emission: noon_core::Emission) {
        self.emission = emission;
    }

    fn has_fill(&self) -> bool {
        self.fill.is_some()
    }

    fn has_stroke(&self) -> bool {
        self.stroke.is_some()
    }

    fn has_background_stroke(&self) -> bool {
        self.background_stroke.is_some()
    }

    fn manim_color(&self) -> Result<Color, AuthoringError> {
        Ok(self
            .fill
            .filter(|color| color.alpha > 0.0)
            .or(self.stroke)
            .map(opaque_paint_color)
            .unwrap_or(Color::WHITE))
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
    fn disable_fill(&mut self) {
        self.fill = None;
    }
    fn disable_stroke(&mut self) {
        self.stroke = None;
    }
    fn set_stroke_opacity(&mut self, opacity: f64) {
        self.stroke = Some(Color {
            alpha: opacity as f32,
            ..self.stroke.unwrap_or(Color::WHITE)
        });
    }
    fn scale_fill_opacity(&mut self, factor: f64) {
        if let Some(fill) = &mut self.fill {
            fill.alpha *= factor as f32;
        }
    }
    fn scale_stroke_opacity(&mut self, factor: f64) {
        if let Some(stroke) = &mut self.stroke {
            stroke.alpha *= factor as f32;
        }
    }
    fn scale_background_stroke_opacity(&mut self, factor: f64) {
        if let Some(stroke) = &mut self.background_stroke {
            stroke.alpha *= factor as f32;
        }
    }
    fn set_stroke_width(&mut self, width: f64) {
        self.stroke_width = width as f32;
    }

    fn set_background_stroke_color(&mut self, color: Color, opacity_when_enabled: f64) {
        let alpha = self
            .background_stroke
            .map_or(opacity_when_enabled as f32, |stroke| stroke.alpha);
        self.background_stroke = Some(Color { alpha, ..color });
    }

    fn set_background_stroke_opacity(&mut self, opacity: f64) {
        self.background_stroke = Some(Color {
            alpha: opacity as f32,
            ..self.background_stroke.unwrap_or(Color::BLACK)
        });
    }

    fn set_background_stroke_width(&mut self, width: f64) {
        self.background_stroke_width = width as f32;
    }
}

pub(crate) fn edit_object_opacity(
    style: &mut SemanticStyle,
    opacity: f64,
) -> Result<(), AuthoringError> {
    style.object_opacity = unit_opacity("opacity", opacity)?;
    Ok(())
}

pub(crate) fn edit_color<S: PaintStyleEdit>(
    style: &mut S,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) -> Result<(), AuthoringError> {
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

pub(crate) fn edit_disable_fill<S: PaintStyleEdit>(style: &mut S) {
    style.disable_fill();
}

pub(crate) fn edit_fill_color<S: PaintStyleEdit>(
    style: &mut S,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) -> Result<(), AuthoringError> {
    let color = opaque_color("fill", red, green, blue)?;
    let requested_opacity = unit_opacity("fill.alpha", alpha)?;
    style.set_fill_color(color, requested_opacity);
    Ok(())
}

pub(crate) fn edit_fill_opacity<S: PaintStyleEdit>(
    style: &mut S,
    opacity: f64,
) -> Result<(), AuthoringError> {
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
) -> Result<(), AuthoringError> {
    let color = opaque_color("fill", red, green, blue)?;
    let opacity = unit_opacity("fill opacity", opacity)?;
    style.set_fill_color(color, opacity);
    style.set_fill_opacity(opacity);
    Ok(())
}

pub(crate) fn edit_manim_opacity<S: PaintStyleEdit>(
    style: &mut S,
    opacity: f64,
) -> Result<(), AuthoringError> {
    let opacity = unit_opacity("opacity", opacity)?;
    if style.has_fill() {
        style.set_fill_opacity(opacity);
    }
    if style.has_stroke() {
        style.set_stroke_opacity(opacity);
    }
    if style.has_background_stroke() {
        style.set_background_stroke_opacity(opacity);
    }
    Ok(())
}

pub(crate) fn edit_fade<S: PaintStyleEdit>(
    style: &mut S,
    darkness: f64,
) -> Result<(), AuthoringError> {
    edit_fade_occurrences(style, darkness, 1)
}

/// Interpolate the current Manim color toward a target and apply it to enabled paint.
/// This follows ManimCE's `Mobject.fade_to` path: `get_color`, color interpolation,
/// then `set_color` on the object itself.
pub(crate) fn edit_fade_to<S: PaintStyleEdit>(
    style: &mut S,
    target: Color,
    amount: f64,
) -> Result<(), AuthoringError> {
    validate_fade_to_inputs(target, amount)?;
    // VMobject.set_color changes fill/stroke color without changing their opacity.
    let source = style.manim_color()?;
    let channel = |from: f32, to: f32| f64::from(from) + (f64::from(to) - f64::from(from)) * amount;
    let color = opaque_color(
        "fade_to",
        channel(source.red, target.red),
        channel(source.green, target.green),
        channel(source.blue, target.blue),
    )?;
    edit_color(
        style,
        f64::from(color.red),
        f64::from(color.green),
        f64::from(color.blue),
        1.0,
    )
}

pub(crate) fn validate_fade_to_inputs(target: Color, amount: f64) -> Result<(), AuthoringError> {
    if !amount.is_finite() {
        return Err(AuthoringError::NonFiniteObjectState);
    }
    opaque_color(
        "fade_to target",
        f64::from(target.red),
        f64::from(target.green),
        f64::from(target.blue),
    )?;
    Ok(())
}

pub(crate) fn edit_fade_to_occurrences<S: PaintStyleEdit>(
    style: &mut S,
    target: Color,
    amount: f64,
    mut occurrences: u64,
) -> Result<(), AuthoringError> {
    if occurrences == 1 {
        return edit_fade_to(style, target, amount);
    }
    validate_fade_to_inputs(target, amount)?;
    if occurrences == 0 {
        return Ok(());
    }
    let source = style.manim_color()?;
    // Compose the repeated affine blend in at most 64 steps. Keep the target
    // weight separately: 1 - (1 - amount)^n loses tiny nonzero blends.
    let (mut source_weight, mut target_weight) = (1.0, 0.0);
    if source != opaque_paint_color(target) {
        let (mut power, mut bias) = (1.0 - amount, amount);
        while occurrences > 0 {
            if occurrences & 1 != 0 {
                source_weight *= power;
                target_weight = target_weight * power + bias;
            }
            bias *= 1.0 + power;
            power *= power;
            occurrences >>= 1;
        }
    }
    let channel =
        |from: f32, to: f32| f64::from(from) * source_weight + f64::from(to) * target_weight;
    edit_color(
        style,
        channel(source.red, target.red),
        channel(source.green, target.green),
        channel(source.blue, target.blue),
        1.0,
    )
}

pub(crate) fn fade_factor(darkness: f64, mut occurrences: u64) -> Result<f64, AuthoringError> {
    let mut power = 1.0 - unit_opacity("darkness", darkness)?;
    // Preserve tiny nonzero factors without subtracting the result from one.
    // Bounded exponentiation also avoids expanding a shared family DAG.
    let mut factor = 1.0;
    while occurrences > 0 {
        if occurrences & 1 != 0 {
            factor *= power;
        }
        power *= power;
        occurrences >>= 1;
    }
    Ok(factor)
}

pub(crate) fn edit_fade_occurrences<S: PaintStyleEdit>(
    style: &mut S,
    darkness: f64,
    occurrences: u64,
) -> Result<(), AuthoringError> {
    let factor = fade_factor(darkness, occurrences)?;
    if style.has_fill() {
        style.scale_fill_opacity(factor);
    }
    if style.has_stroke() {
        style.scale_stroke_opacity(factor);
    }
    if style.has_background_stroke() {
        style.scale_background_stroke_opacity(factor);
    }
    Ok(())
}

pub(crate) fn edit_disable_stroke<S: PaintStyleEdit>(style: &mut S) {
    style.disable_stroke();
}

pub(crate) fn edit_stroke_color<S: PaintStyleEdit>(
    style: &mut S,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) -> Result<(), AuthoringError> {
    let color = opaque_color("stroke", red, green, blue)?;
    let requested_opacity = unit_opacity("stroke.alpha", alpha)?;
    style.set_stroke_color(color, requested_opacity);
    Ok(())
}

pub(crate) fn edit_stroke_opacity<S: PaintStyleEdit>(
    style: &mut S,
    opacity: f64,
) -> Result<(), AuthoringError> {
    let opacity = unit_opacity("stroke opacity", opacity)?;
    style.set_stroke_opacity(opacity);
    Ok(())
}

pub(crate) fn edit_stroke(
    style: &mut SemanticStyle,
    red: f64,
    green: f64,
    blue: f64,
    opacity: f64,
) -> Result<(), AuthoringError> {
    let color = opaque_color("stroke", red, green, blue)?;
    let opacity = unit_opacity("stroke opacity", opacity)?;
    style.stroke = Some(SemanticPaint::Solid(color));
    style.stroke_opacity = opacity;
    Ok(())
}

pub(crate) fn edit_stroke_width<S: PaintStyleEdit>(
    style: &mut S,
    width: f64,
) -> Result<(), AuthoringError> {
    let width = authoring_render_f64("stroke width", width)?;
    if width < 0.0 {
        return Err(AuthoringError::NegativeStrokeWidth(width));
    }
    style.set_stroke_width(width);
    if !style.has_stroke() {
        style.set_stroke_color(Color::WHITE, 1.0);
    }
    Ok(())
}

pub(crate) fn edit_background_stroke_color<S: PaintStyleEdit>(
    style: &mut S,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) -> Result<(), AuthoringError> {
    let color = opaque_color("background stroke", red, green, blue)?;
    let opacity = unit_opacity("background stroke.alpha", alpha)?;
    style.set_background_stroke_color(color, opacity);
    Ok(())
}

pub(crate) fn edit_background_stroke_opacity<S: PaintStyleEdit>(
    style: &mut S,
    opacity: f64,
) -> Result<(), AuthoringError> {
    let opacity = unit_opacity("background stroke opacity", opacity)?;
    style.set_background_stroke_opacity(opacity);
    Ok(())
}

pub(crate) fn edit_background_stroke_width<S: PaintStyleEdit>(
    style: &mut S,
    width: f64,
) -> Result<(), AuthoringError> {
    let width = authoring_render_f64("background stroke width", width)?;
    if width < 0.0 {
        return Err(AuthoringError::NegativeStrokeWidth(width));
    }
    style.set_background_stroke_width(width);
    if !style.has_background_stroke() {
        style.set_background_stroke_color(Color::BLACK, 1.0);
    }
    Ok(())
}

pub(super) fn parse_stroke_width_mode(mode: &str) -> Result<StrokeWidthMode, AuthoringError> {
    match mode {
        "scale_with_object" => Ok(StrokeWidthMode::ScaleWithObject),
        "screen_space" => Ok(StrokeWidthMode::ScreenSpace),
        _ => Err(AuthoringError::InvalidStrokeWidthMode(mode.to_owned())),
    }
}

pub(super) fn parse_stroke_join(join: &str) -> Result<StrokeJoin, AuthoringError> {
    match join {
        "round" => Ok(StrokeJoin::Round),
        "miter" => Ok(StrokeJoin::Miter),
        "bevel" => Ok(StrokeJoin::Bevel),
        _ => Err(AuthoringError::InvalidStrokeJoin(join.to_owned())),
    }
}

pub(crate) fn parse_stroke_cap(cap: &str) -> Result<StrokeCap, AuthoringError> {
    match cap {
        "auto" => Ok(StrokeCap::Auto),
        "round" => Ok(StrokeCap::Round),
        "butt" => Ok(StrokeCap::Butt),
        "square" => Ok(StrokeCap::Square),
        _ => Err(AuthoringError::InvalidStrokeCap(cap.to_owned())),
    }
}

pub(crate) fn prepare_fill_gradient(
    object: &Mobject,
    gradient: std::sync::Arc<noon_core::LinearGradient>,
    opacity: Option<f64>,
) -> Result<SemanticObjectState, AuthoringError> {
    object.validate()?;
    let state = object.state()?;
    prepare_fill_paint_value(
        &object.integration_store().borrow(),
        object.node_id(),
        state,
        SemanticPaint::LinearGradient(gradient),
        opacity,
    )
}

pub(crate) fn prepare_fill_paint_value(
    store: &SemanticStore,
    node: SemanticNodeId,
    mut state: SemanticObjectState,
    paint: SemanticPaint,
    opacity: Option<f64>,
) -> Result<SemanticObjectState, AuthoringError> {
    if state.style.fill.is_none() {
        state.style.fill_opacity = 1.0;
    }
    state.style.fill = Some(paint);
    if let Some(opacity) = opacity {
        edit_fill_opacity(&mut state.style, opacity)?;
    }
    let unsupported = || {
        AuthoringError::Unsupported(crate::UnsupportedAuthoringOperation::ContinuousPaintProfile)
    };
    noon_compile::lower_semantic_fill(node, &state, store).map_err(|_| unsupported())?;
    Ok(state)
}

impl Mobject {
    /// Set immutable stripes or crosshatching in object-local scene units.
    pub fn set_fill_hatch(
        &mut self,
        paint: impl Into<std::sync::Arc<noon_core::HatchPaint>>,
        opacity: Option<f64>,
    ) -> Result<(), AuthoringError> {
        self.validate()?;
        let state = prepare_fill_paint_value(
            &self.integration_store().borrow(),
            self.node_id(),
            self.state()?,
            SemanticPaint::Hatch(paint.into()),
            opacity,
        )?;
        self.commit_state(state)
    }

    /// Set a seeded local noise fill through the shared style publication.
    pub fn set_fill_noise(
        &mut self,
        paint: impl Into<std::sync::Arc<noon_core::NoisePaint>>,
        opacity: Option<f64>,
    ) -> Result<(), AuthoringError> {
        self.validate()?;
        let state = prepare_fill_paint_value(
            &self.integration_store().borrow(),
            self.node_id(),
            self.state()?,
            SemanticPaint::Noise(paint.into()),
            opacity,
        )?;
        self.commit_state(state)
    }

    /// Set an elliptical radial fill through the ordinary shared style operation.
    pub fn set_fill_radial_gradient(
        &mut self,
        gradient: impl Into<std::sync::Arc<noon_core::RadialGradient>>,
        opacity: Option<f64>,
    ) -> Result<(), AuthoringError> {
        self.validate()?;
        let state = prepare_fill_paint_value(
            &self.integration_store().borrow(),
            self.node_id(),
            self.state()?,
            SemanticPaint::RadialGradient(gradient.into()),
            opacity,
        )?;
        self.commit_state(state)
    }

    /// Set one immutable object-local gradient through the ordinary style transaction.
    pub fn set_fill_gradient(
        &mut self,
        gradient: impl Into<std::sync::Arc<noon_core::LinearGradient>>,
        opacity: Option<f64>,
    ) -> Result<(), AuthoringError> {
        let state = prepare_fill_gradient(self, gradient.into(), opacity)?;
        self.commit_state(state)
    }
    /// RGB paint observation; opacity is queried independently.
    pub fn fill_color(&self) -> Result<Option<Color>, AuthoringError> {
        paint_color(self.state()?.style.fill.as_ref())
    }

    pub fn stroke_color(&self) -> Result<Option<Color>, AuthoringError> {
        paint_color(self.state()?.style.stroke.as_ref())
    }

    pub fn background_stroke_color(&self) -> Result<Option<Color>, AuthoringError> {
        let style = self.state()?.style;
        Ok(paint_color(style.background_stroke.as_ref())?.or(Some(Color::BLACK)))
    }

    /// Stored stroke width in scene units, independent of object scale.
    pub fn stroke_width(&self) -> Result<f64, AuthoringError> {
        Ok(self.state()?.style.stroke_width)
    }

    pub fn background_stroke_width(&self) -> Result<f64, AuthoringError> {
        Ok(self.state()?.style.background_stroke_width)
    }

    pub fn set_stroke_width_mode(&mut self, mode: &str) -> Result<(), AuthoringError> {
        let mut state = self.state()?;
        state.style.stroke_width_mode = parse_stroke_width_mode(mode)?;
        self.commit_state(state)
    }
    pub fn set_stroke_join(&mut self, join: &str) -> Result<(), AuthoringError> {
        let mut state = self.state()?;
        state.style.stroke_join = parse_stroke_join(join)?;
        self.commit_state(state)
    }
    pub fn set_stroke_cap(&mut self, cap: &str) -> Result<(), AuthoringError> {
        let mut state = self.state()?;
        state.style.stroke_cap = parse_stroke_cap(cap)?;
        self.commit_state(state)
    }
    pub fn set_object_opacity(&mut self, opacity: f64) -> Result<(), AuthoringError> {
        let mut state = self.state()?;
        edit_object_opacity(&mut state.style, opacity)?;
        self.commit_state(state)
    }
    pub fn set_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), AuthoringError> {
        self.validate()?;
        let mut state = self.state()?;
        edit_color(&mut state.style, red, green, blue, alpha)?;
        self.commit_state(state)
    }

    /// Match the donor's authored Manim RGB while preserving enabled paint opacity.
    pub fn match_color(&mut self, donor: &Mobject) -> Result<(), AuthoringError> {
        let color = donor.manim_color()?;
        self.set_color(color.red.into(), color.green.into(), color.blue.into(), 1.0)
    }
    pub fn disable_fill(&mut self) -> Result<(), AuthoringError> {
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
    ) -> Result<(), AuthoringError> {
        self.validate()?;
        let mut state = self.state()?;
        edit_fill_color(&mut state.style, red, green, blue, alpha)?;
        self.commit_state(state)
    }
    pub fn set_fill_opacity(&mut self, opacity: f64) -> Result<(), AuthoringError> {
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
    ) -> Result<(), AuthoringError> {
        self.validate()?;
        let mut state = self.state()?;
        edit_fill(&mut state.style, red, green, blue, opacity)?;
        self.commit_state(state)
    }
    pub fn fill_opacity(&self) -> Result<f64, AuthoringError> {
        let state = self.state()?;
        manim_paint_opacity(state.style.fill.as_ref(), state.style.fill_opacity)
    }
    pub fn disable_stroke(&mut self) -> Result<(), AuthoringError> {
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
    ) -> Result<(), AuthoringError> {
        self.validate()?;
        let mut state = self.state()?;
        edit_stroke_color(&mut state.style, red, green, blue, alpha)?;
        self.commit_state(state)
    }
    pub fn set_stroke_width(&mut self, width: f64) -> Result<(), AuthoringError> {
        self.validate()?;
        let mut state = self.state()?;
        edit_stroke_width(&mut state.style, width)?;
        self.commit_state(state)
    }
    pub fn set_stroke_opacity(&mut self, opacity: f64) -> Result<(), AuthoringError> {
        self.validate()?;
        let mut state = self.state()?;
        edit_stroke_opacity(&mut state.style, opacity)?;
        self.commit_state(state)
    }
    pub fn stroke_opacity(&self) -> Result<f64, AuthoringError> {
        let state = self.state()?;
        manim_paint_opacity(state.style.stroke.as_ref(), state.style.stroke_opacity)
    }
    pub fn background_stroke_opacity(&self) -> Result<f64, AuthoringError> {
        let state = self.state()?;
        if state.style.background_stroke.is_none() {
            return Ok(1.0);
        }
        manim_paint_opacity(
            state.style.background_stroke.as_ref(),
            state.style.background_stroke_opacity,
        )
    }
    /// Set checked colored emission for a linear-HDR material. This changes
    /// radiance before association; it does not change alpha or create a halo.
    pub fn set_emission(&mut self, strength: f64) -> Result<(), AuthoringError> {
        let emission = noon_core::Emission::new(strength).map_err(AuthoringError::Emission)?;
        let mut state = self.state()?;
        state.style.emission = emission;
        self.commit_state(state)
    }

    pub fn emission(&self) -> Result<f64, AuthoringError> {
        Ok(self.state()?.style.emission.strength())
    }

    pub fn set_opacity(&mut self, opacity: f64) -> Result<(), AuthoringError> {
        self.validate()?;
        let mut state = self.state()?;
        edit_manim_opacity(&mut state.style, opacity)?;
        self.commit_state(state)
    }

    /// Multiply enabled fill/stroke alpha while preserving whole-object opacity.
    pub fn fade(&mut self, darkness: f64) -> Result<(), AuthoringError> {
        let mut state = self.state()?;
        edit_fade(&mut state.style, darkness)?;
        self.commit_state(state)
    }

    /// Interpolate this object's Manim color toward `target` by `amount`.
    pub fn fade_to(&mut self, target: Color, amount: f64) -> Result<(), AuthoringError> {
        self.validate()?;
        if !self.has_fade_to_points()? {
            return Ok(());
        }
        let mut state = self.state()?;
        edit_fade_to(&mut state.style, target, amount)?;
        self.commit_state(state)
    }

    pub(crate) fn has_fade_to_points(&self) -> Result<bool, AuthoringError> {
        let state = self.state()?;
        match state.content {
            SemanticObjectContent::Image(_) => Ok(false),
            SemanticObjectContent::Text(_) => Ok(true),
            SemanticObjectContent::Geometry(StoredGeometry::Resource(handle)) => {
                let store = self.integration_store().borrow();
                match store
                    .geometry_resources()
                    .get(handle)
                    .ok_or(AuthoringError::MissingGeometryResource(handle))?
                {
                    GeometryResource::Mesh(_) => Ok(true),
                    GeometryResource::VectorPath(path) => {
                        Ok(crate::path_queries::PathQuery::prepare(
                            path,
                            noon_core::SemanticTransform2_5D::default(),
                        )?
                        .point_count()
                            > 0)
                    }
                }
            }
            // Analytic primitives retain control points even when degenerate.
            SemanticObjectContent::Geometry(_) => Ok(true),
        }
    }
}

#[cfg(test)]
mod opacity_tests {
    use super::*;

    fn image_style() -> (crate::Scene, SemanticStyle) {
        let mut scene = crate::Scene::new();
        let mut object = scene.circle(1.0).unwrap();
        object
            .set_fill_image(
                crate::ImagePaintOptions::rgba8(1, 1, vec![128, 64, 0, 64]).unwrap(),
                Some(1.0),
            )
            .unwrap();
        object.disable_stroke().unwrap();
        let style = object.state().unwrap().style;
        (scene, style)
    }

    #[test]
    fn stroke_cap_parser_accepts_manim_auto_and_explicit_caps() {
        assert_eq!(parse_stroke_cap("auto").unwrap(), StrokeCap::Auto);
        assert_eq!(parse_stroke_cap("round").unwrap(), StrokeCap::Round);
        assert_eq!(parse_stroke_cap("butt").unwrap(), StrokeCap::Butt);
        assert_eq!(parse_stroke_cap("square").unwrap(), StrokeCap::Square);
        assert!(parse_stroke_cap("invalid").is_err());
    }

    #[test]
    fn repeated_fade_to_preserves_small_blends_and_bounds_shared_alias_work() {
        let mut style = Style {
            fill: Some(Color::rgb(1.0, 0.0, 0.0)),
            ..Default::default()
        };
        edit_fade_to_occurrences(&mut style, Color::rgb(0.0, 0.0, 1.0), 1e-20, 2).unwrap();
        let blue = style.fill.unwrap().blue;
        assert!(blue > 1.9e-20 && blue < 2.1e-20);
        edit_fade_to_occurrences(&mut style, Color::rgb(0.0, 0.0, 1.0), 0.5, u64::MAX).unwrap();
        assert_eq!(style.fill, Some(Color::rgb(0.0, 0.0, 1.0)));
        edit_fade_to_occurrences(&mut style, Color::rgb(0.0, 0.0, 1.0), 3.0, u64::MAX).unwrap();
        assert_eq!(style.fill, Some(Color::rgb(0.0, 0.0, 1.0)));
    }

    #[test]
    fn fade_to_point_filter_preserves_empty_paths_and_unfinished_anchors() {
        let mut scene = crate::Scene::new();
        for path in [
            noon_core::VectorPath::new(),
            noon_core::VectorPath::new().close(),
            noon_core::VectorPath::new().move_to(noon_core::Vec2::ZERO),
            noon_core::VectorPath::new()
                .move_to(noon_core::Vec2::ZERO)
                .close(),
            noon_core::VectorPath::new().line_to(noon_core::Vec2::ZERO),
            noon_core::VectorPath::new()
                .move_to(noon_core::Vec2::ZERO)
                .line_to(noon_core::Vec2::ZERO),
        ] {
            let object = scene
                .geometry(crate::ManimGeometryOptions::path(path).unwrap())
                .unwrap();
            assert_eq!(
                object.has_fade_to_points().unwrap(),
                object.local_path_query().unwrap().point_count() > 0
            );
        }
        assert!(scene.square(0.5).unwrap().has_fade_to_points().unwrap());
    }

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

        let (_scene, mut resource) = image_style();
        let original = resource.fill.clone();
        edit_manim_opacity(&mut resource, 0.4).unwrap();
        assert_eq!(resource.fill, original);
        assert_eq!(resource.fill_opacity, 0.4);
        assert_eq!(resource.stroke, None);
    }

    #[test]
    fn paint_opacity_observes_intrinsic_alpha_and_disabled_paints() {
        let solid = SemanticPaint::Solid(Color::rgba(0.2, 0.4, 0.8, 0.5));
        assert_eq!(manim_paint_opacity(Some(&solid), 0.25).unwrap(), 0.125);
        assert_eq!(manim_paint_opacity(Some(&solid), 1.0).unwrap(), 0.5);
        assert_eq!(manim_paint_opacity(None, 1.0).unwrap(), 0.0);
        let (_scene, image) = image_style();
        assert_eq!(manim_paint_opacity(image.fill.as_ref(), 0.5).unwrap(), 0.5);
    }
}
