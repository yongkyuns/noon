//! Per-member RGBA interpolation is authoring behavior, not renderer gradient paint.
use crate::{
    semantic_mobject::{authoring_render_f64, edit_color},
    AuthoringError, Bounds2D64, Color, Mobject, MobjectFamily,
};
use noon_core::{SemanticMutationTransaction, SemanticStyle};

fn validate_colors(colors: &[Color]) -> Result<(), AuthoringError> {
    if colors.is_empty() {
        return Err(AuthoringError::EmptyColorGradient);
    }
    for color in colors {
        edit_color(
            &mut SemanticStyle::default(),
            color.red.into(),
            color.green.into(),
            color.blue.into(),
            color.alpha.into(),
        )?;
    }
    Ok(())
}

fn gradient_color(colors: &[Color], index: usize, count: usize) -> Color {
    if count <= 1 || colors.len() == 1 {
        return colors[0];
    }
    let position = index as f64 * (colors.len() - 1) as f64 / (count - 1) as f64;
    let lower = (position.floor() as usize).min(colors.len() - 1);
    let upper = (lower + 1).min(colors.len() - 1);
    let fraction = position - lower as f64;
    let blend = |a: f32, b: f32| (f64::from(a) + (f64::from(b) - f64::from(a)) * fraction) as f32;
    let (a, b) = (colors[lower], colors[upper]);
    Color::rgba(
        blend(a.red, b.red),
        blend(a.green, b.green),
        blend(a.blue, b.blue),
        blend(a.alpha, b.alpha),
    )
}

pub(crate) fn validate_radial_gradient_inputs(
    center: (f64, f64, f64),
    radius: f64,
    inner: Color,
    outer: Color,
) -> Result<(), AuthoringError> {
    authoring_render_f64("center.x", center.0)?;
    authoring_render_f64("center.y", center.1)?;
    authoring_render_f64("center.z", center.2)?;
    authoring_render_f64("radius", radius)?;
    if radius <= 0.0 {
        return Err(AuthoringError::NonPositiveNumber {
            name: "radius".into(),
            value: radius,
        });
    }
    validate_colors(&[inner, outer])
}

pub(crate) fn radial_gradient_color(
    distance: f64,
    radius: f64,
    inner: Color,
    outer: Color,
) -> Color {
    // Pinned Manim clips only the upper end of t; valid positive radii therefore
    // interpolate in RGBA and leave members beyond the radius at the outer color.
    let t = (distance / radius).min(1.0);
    let blend = |a: f32, b: f32| (f64::from(a) + (f64::from(b) - f64::from(a)) * t) as f32;
    Color::rgba(
        blend(inner.red, outer.red),
        blend(inner.green, outer.green),
        blend(inner.blue, outer.blue),
        blend(inner.alpha, outer.alpha),
    )
}

pub(crate) fn radial_distance(member_center: (f64, f64), center: (f64, f64, f64)) -> f64 {
    (member_center.0 - center.0)
        .hypot(member_center.1 - center.1)
        .hypot(center.2)
}

fn bounds_center(bounds: Bounds2D64) -> (f64, f64) {
    (
        (bounds.min_x + bounds.max_x) * 0.5,
        (bounds.min_y + bounds.max_y) * 0.5,
    )
}

/// Evenly sample reference colors, including both endpoints when count > 1.
/// RGB interpolation follows pinned Manim's color-gradient utility.
/// Empty reference colors reject even when the requested output is empty.
pub fn color_gradient(colors: &[Color], count: usize) -> Result<Vec<Color>, AuthoringError> {
    validate_colors(colors)?;
    Ok((0..count)
        .map(|index| gradient_color(colors, index, count))
        .collect())
}

impl Mobject {
    /// A single semantic leaf takes the first reference color.
    pub fn set_color_by_gradient(&mut self, colors: &[Color]) -> Result<(), AuthoringError> {
        validate_colors(colors)?;
        let color = colors[0];
        self.set_color(
            color.red.into(),
            color.green.into(),
            color.blue.into(),
            color.alpha.into(),
        )
    }

    /// Apply a Manim-style radial color to this point-bearing object's own paint.
    /// A point-free object is unchanged, as it is omitted by
    /// `family_members_with_points()` in ManimCE.
    pub fn set_color_by_radial_gradient(
        &mut self,
        center: (f64, f64, f64),
        radius: f64,
        inner: Color,
        outer: Color,
    ) -> Result<(), AuthoringError> {
        validate_radial_gradient_inputs(center, radius, inner, outer)?;
        if !self.has_fade_to_points()? {
            return Ok(());
        }
        let bounds = self
            .layout_bounds()?
            .ok_or(AuthoringError::NonFiniteObjectState)?;
        let color = radial_gradient_color(
            radial_distance(bounds_center(bounds), center),
            radius,
            inner,
            outer,
        );
        let mut state = self.state()?;
        edit_color(
            &mut state.style,
            color.red.into(),
            color.green.into(),
            color.blue.into(),
            color.alpha.into(),
        )?;
        self.commit_state(state)
    }
}

impl MobjectFamily {
    /// Color each unique leaf in Manim's last-occurrence family order atomically.
    /// Geometry/resources and existing paint opacity remain unchanged.
    pub fn set_color_by_gradient(&self, colors: &[Color]) -> Result<(), AuthoringError> {
        self.gradient_transaction(colors)?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    pub(crate) fn gradient_transaction(
        &self,
        colors: &[Color],
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        validate_colors(colors)?;
        self.style_transaction_last_occurrence(|index, count, style| {
            let color = gradient_color(colors, index, count);
            edit_color(
                style,
                color.red.into(),
                color.green.into(),
                color.blue.into(),
                color.alpha.into(),
            )
        })
    }

    /// Stage a radial paint update for point-bearing unique leaves in Manim's
    /// last-occurrence family order. Existing content and opacity are preserved.
    pub(crate) fn radial_gradient_transaction(
        &self,
        center: (f64, f64, f64),
        radius: f64,
        inner: Color,
        outer: Color,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        validate_radial_gradient_inputs(center, radius, inner, outer)?;
        self.validate()?;
        let leaves = self
            .integration_store()
            .borrow()
            .ordered_last_occurrence_leaf_nodes(self.node_id())
            .map_err(AuthoringError::from)?;
        let mut transaction = SemanticMutationTransaction::new();
        for leaf in leaves {
            let object = Mobject::from_node(std::rc::Rc::clone(self.integration_store()), leaf)?;
            if !object.has_fade_to_points()? {
                continue;
            }
            let bounds = object
                .content_layout_bounds()?
                .ok_or(AuthoringError::NonFiniteObjectState)?;
            let color = radial_gradient_color(
                radial_distance(bounds_center(bounds), center),
                radius,
                inner,
                outer,
            );
            let mut style = object.state()?.style;
            edit_color(
                &mut style,
                color.red.into(),
                color.green.into(),
                color.blue.into(),
                color.alpha.into(),
            )?;
            transaction.replace_style(leaf, style);
        }
        Ok(transaction)
    }

    /// Recolor point-bearing members by their distance from a shared center.
    pub fn set_color_by_radial_gradient(
        &self,
        center: (f64, f64, f64),
        radius: f64,
        inner: Color,
        outer: Color,
    ) -> Result<(), AuthoringError> {
        self.radial_gradient_transaction(center, radius, inner, outer)?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MobjectTarget, Scene};
    use noon_core::{SemanticPaint, VectorPath};

    fn fill_color(object: &Mobject) -> Color {
        match object.state().unwrap().style.fill.unwrap() {
            SemanticPaint::Solid(color) => color,
            SemanticPaint::LinearGradient(_)
            | SemanticPaint::Image(_)
            | SemanticPaint::Resource(_) => {
                panic!("radial family paint must stay solid")
            }
        }
    }

    fn assert_rgb(actual: Color, red: f32, green: f32, blue: f32) {
        assert!((actual.red - red).abs() < 1e-6, "{actual:?}");
        assert!((actual.green - green).abs() < 1e-6, "{actual:?}");
        assert!((actual.blue - blue).abs() < 1e-6, "{actual:?}");
    }

    #[test]
    fn radial_gradient_uses_each_point_bearing_leaf_center_and_skips_empty_leaves() {
        let mut scene = Scene::new();
        let mut near = scene.square(1.0).unwrap();
        near.shift(-1.0, 0.0).unwrap();
        near.set_fill_opacity(0.3).unwrap();
        near.set_stroke_opacity(0.6).unwrap();
        let center = scene.circle(0.5).unwrap();
        let mut far = scene.square(1.0).unwrap();
        far.shift(3.0, 0.0).unwrap();
        let point_free_style = SemanticStyle {
            fill: Some(SemanticPaint::Solid(Color::rgb(0.0, 1.0, 0.0))),
            ..SemanticStyle::default()
        };
        let point_free = scene.path(VectorPath::new(), point_free_style).unwrap();

        let nested = scene
            .family(&[
                MobjectTarget::from(&point_free),
                MobjectTarget::from(&center),
                MobjectTarget::from(&far),
            ])
            .unwrap();
        let family = scene
            .family(&[MobjectTarget::from(&near), MobjectTarget::from(&nested)])
            .unwrap();
        family
            .set_color_by_radial_gradient(
                (0.0, 0.0, 0.0),
                2.0,
                Color::rgb(1.0, 0.0, 0.0),
                Color::rgb(0.0, 0.0, 1.0),
            )
            .unwrap();

        assert_rgb(fill_color(&near), 0.5, 0.0, 0.5);
        assert_rgb(fill_color(&center), 1.0, 0.0, 0.0);
        assert_rgb(fill_color(&far), 0.0, 0.0, 1.0);
        assert_rgb(fill_color(&point_free), 0.0, 1.0, 0.0);
        assert_eq!(near.state().unwrap().style.fill_opacity, 0.3);
        assert_eq!(near.state().unwrap().style.stroke_opacity, 0.6);
    }

    #[test]
    fn radial_gradient_rejects_zero_radius_without_mutating_and_direct_empty_is_noop() {
        let mut scene = Scene::new();
        let mut square = scene.square(1.0).unwrap();
        let before = fill_color(&square);
        assert!(square
            .set_color_by_radial_gradient(
                (0.0, 0.0, 0.0),
                0.0,
                Color::rgb(1.0, 0.0, 0.0),
                Color::rgb(0.0, 0.0, 1.0),
            )
            .is_err());
        assert_eq!(fill_color(&square), before);

        let empty = scene
            .path(
                VectorPath::new(),
                SemanticStyle {
                    fill: Some(SemanticPaint::Solid(Color::rgb(0.0, 1.0, 0.0))),
                    ..SemanticStyle::default()
                },
            )
            .unwrap();
        let before = fill_color(&empty);
        empty
            .clone()
            .set_color_by_radial_gradient(
                (0.0, 0.0, 0.0),
                1.0,
                Color::rgb(1.0, 0.0, 0.0),
                Color::rgb(0.0, 0.0, 1.0),
            )
            .unwrap();
        assert_eq!(fill_color(&empty), before);
    }

    #[test]
    fn radial_gradient_measures_planar_members_from_three_dimensional_centers() {
        let mut scene = Scene::new();
        let mut square = scene.square(1.0).unwrap();
        square
            .set_color_by_radial_gradient(
                (0.0, 0.0, 1.0),
                2.0,
                Color::rgb(1.0, 0.0, 0.0),
                Color::rgb(0.0, 0.0, 1.0),
            )
            .unwrap();

        assert_rgb(fill_color(&square), 0.5, 0.0, 0.5);
        assert_eq!(radial_distance((0.0, 0.0), (0.0, 0.0, 1.0)), 1.0);
        assert!(validate_radial_gradient_inputs(
            (0.0, 0.0, f64::NAN),
            2.0,
            Color::WHITE,
            Color::BLACK,
        )
        .is_err());
    }

    #[test]
    fn live_radial_gradient_uses_effective_member_centers() {
        let mut scene = Scene::new();
        let mut near = scene.square(1.0).unwrap();
        near.shift(-1.0, 0.0).unwrap();
        let center = scene.circle(0.5).unwrap();
        let mut far = scene.square(1.0).unwrap();
        far.shift(3.0, 0.0).unwrap();
        let family = scene
            .family(&[
                MobjectTarget::from(&near),
                MobjectTarget::from(&center),
                MobjectTarget::from(&far),
            ])
            .unwrap();
        for member in [&near, &center, &far] {
            scene.add(member).unwrap();
        }
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        live.shift(&center, 1.0, 0.0).unwrap();
        live.set_family_color_by_radial_gradient(
            &family,
            (0.0, 0.0, 0.0),
            2.0,
            Color::rgb(1.0, 0.0, 0.0),
            Color::rgb(0.0, 0.0, 1.0),
        )
        .unwrap();
        assert_rgb(fill_color(&center), 0.5, 0.0, 0.5);
    }
}
