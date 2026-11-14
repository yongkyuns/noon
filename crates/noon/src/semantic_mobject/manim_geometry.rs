//! Typed constructors for specialized Manim-compatible geometry.
use super::*;

impl ManimGeometryOptions {
    /// Closed vertex groups, preserving Manim's path ordering and BLUE default.
    pub fn polygram(groups: &[Vec<(f64, f64)>]) -> Result<Self, AuthoringError> {
        Ok(Self::new(
            GeometryRef::path(crate::polygon_authoring::polygram_path(groups)?),
            manim_style(Color::BLUE),
        ))
    }

    pub fn polygon(vertices: &[(f64, f64)]) -> Result<Self, AuthoringError> {
        Self::polygram(&[vertices.to_vec()])
    }

    pub fn regular_polygon(
        n: u32,
        radius: f64,
        start_angle: Option<f64>,
    ) -> Result<Self, AuthoringError> {
        Self::regular_polygram(n, 1, radius, start_angle)
    }

    pub fn regular_polygram(
        n: u32,
        density: u32,
        radius: f64,
        start_angle: Option<f64>,
    ) -> Result<Self, AuthoringError> {
        let (path, resolved_start_angle) =
            crate::polygon_authoring::regular_polygram_path(n, density, radius, start_angle)?;
        let mut options = Self::new(GeometryRef::path(path), manim_style(Color::BLUE));
        options.resolved_start_angle = Some(resolved_start_angle);
        Ok(options)
    }

    pub fn star(
        n: u32,
        outer_radius: f64,
        inner_radius: Option<f64>,
        density: u32,
        start_angle: Option<f64>,
    ) -> Result<Self, AuthoringError> {
        let (path, resolved_start_angle) = crate::polygon_authoring::star_path(
            n,
            outer_radius,
            inner_radius,
            density,
            start_angle,
        )?;
        let mut options = Self::new(GeometryRef::path(path), manim_style(Color::BLUE));
        options.resolved_start_angle = Some(resolved_start_angle);
        Ok(options)
    }

    pub fn dot(x: f64, y: f64, radius: f64) -> Result<Self, AuthoringError> {
        let mut style = manim_style(Color::WHITE);
        style.fill_opacity = 1.0;
        style.stroke_width = 0.0;
        let mut options = Self::new(GeometryRef::circle(positive_f32("radius", radius)?), style);
        options.set_translation(x, y)?;
        Ok(options)
    }

    pub fn triangle() -> Result<Self, AuthoringError> {
        Self::regular_polygon(3, 1.0, None)
    }

    pub fn elbow(width: f64, angle: f64) -> Result<Self, AuthoringError> {
        let path = crate::elbow_authoring::manim_elbow_path(
            finite_f32("width", width)?,
            finite_f32("angle", angle)?,
        )
        .map_err(AuthoringError::from)?;
        Ok(Self::new(
            GeometryRef::path(path),
            manim_style(Color::WHITE),
        ))
    }

    pub fn rounded_rectangle(
        width: f64,
        height: f64,
        corner_radius: f64,
    ) -> Result<Self, AuthoringError> {
        let radius = finite_f32("corner_radius", corner_radius)?;
        let path = crate::rounded_rectangle_authoring::manim_rounded_rectangle_path(
            positive_f32("width", width)?,
            positive_f32("height", height)?,
            [radius; 4],
        )
        .map_err(AuthoringError::from)?;
        Ok(Self::new(
            GeometryRef::path(path),
            manim_style(Color::WHITE),
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn annular_sector(
        inner_radius: f64,
        outer_radius: f64,
        angle: f64,
        start_angle: f64,
        num_components: u32,
        center_x: f64,
        center_y: f64,
    ) -> Result<Self, AuthoringError> {
        let geometry = crate::sector_authoring::annular_sector_geometry(
            inner_radius,
            outer_radius,
            angle,
            start_angle,
            num_components,
            center_x,
            center_y,
        )?;
        Ok(Self::new(geometry, manim_filled_path_style()))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn sector(
        radius: f64,
        angle: f64,
        start_angle: f64,
        num_components: u32,
        center_x: f64,
        center_y: f64,
    ) -> Result<Self, AuthoringError> {
        let geometry = crate::sector_authoring::sector_geometry(
            radius,
            angle,
            start_angle,
            num_components,
            center_x,
            center_y,
        )?;
        Ok(Self::new(geometry, manim_filled_path_style()))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn annulus(
        inner_radius: f64,
        outer_radius: f64,
        num_components: u32,
        center_x: f64,
        center_y: f64,
    ) -> Result<Self, AuthoringError> {
        let geometry = crate::sector_authoring::annulus_geometry(
            inner_radius,
            outer_radius,
            num_components,
            center_x,
            center_y,
        )?;
        Ok(Self::new(geometry, manim_filled_path_style()))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn dashed_line(
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
        dash_length: f64,
        dashed_ratio: f64,
    ) -> Result<Self, AuthoringError> {
        let geometry = crate::dashed_line_authoring::dashed_line_geometry(
            start_x,
            start_y,
            end_x,
            end_y,
            dash_length,
            dashed_ratio,
        )?;
        Ok(Self::new(geometry, manim_style(Color::WHITE)))
    }

    pub fn underline(bounds: Bounds2D64, buff: f64) -> Result<Self, AuthoringError> {
        for (name, value) in [
            ("bounds.min_x", bounds.min_x),
            ("bounds.min_y", bounds.min_y),
            ("bounds.max_x", bounds.max_x),
            ("bounds.max_y", bounds.max_y),
        ] {
            authoring_render_f64(name, value)?;
        }
        if bounds.max_x < bounds.min_x || bounds.max_y < bounds.min_y {
            return Err(AuthoringError::UnorderedBounds(bounds));
        }
        let buff = authoring_render_f64("buff", buff)?;
        let half_width = bounds.width() * 0.5;
        let mut options = Self::new(
            GeometryRef::line(
                Vec2::new(finite_f32("underline.start.x", -half_width)?, 0.0),
                Vec2::new(finite_f32("underline.end.x", half_width)?, 0.0),
            ),
            manim_style(Color::WHITE),
        );
        options.set_translation((bounds.min_x + bounds.max_x) * 0.5, bounds.min_y - buff)?;
        Ok(options)
    }
}

impl crate::MobjectTarget<'_> {
    /// Observe authored bounds for a detached shape matcher.
    pub fn shape_matcher_bounds(self) -> Result<Bounds2D64, AuthoringError> {
        match self {
            Self::Object(object) => Ok(match object.layout_bounds()? {
                Some(bounds) => bounds,
                None => {
                    let translation = object.state()?.transform.translation;
                    Bounds2D64::point(translation.x, translation.y)
                }
            }),
            Self::Family(family) => Ok(family
                .layout()?
                .bounds()
                .unwrap_or_else(|| Bounds2D64::point(0.0, 0.0))),
        }
    }
}

fn observed_layout_bounds(layout: crate::EffectiveMobjectLayout) -> Bounds2D64 {
    let ((x, y), width, height) = (layout.center, layout.width, layout.height);
    Bounds2D64 {
        min_x: x - width * 0.5,
        max_x: x + width * 0.5,
        min_y: y - height * 0.5,
        max_y: y + height * 0.5,
    }
}

impl crate::Scene {
    /// Capture one bounds value for all parts of a shape matcher.
    pub fn shape_matcher_bounds(
        &self,
        target: crate::MobjectTarget<'_>,
    ) -> Result<Bounds2D64, AuthoringError> {
        use crate::MobjectTarget;
        target.require_store(self.integration_store())?;
        target.validate()?;
        self.require_effective_capture()?;
        if let Some(execution) = self.running_execution() {
            match target {
                MobjectTarget::Object(object)
                    if execution.semantic_object_is_reachable(object.node_id()) =>
                {
                    return self.effective_layout(object).map(observed_layout_bounds);
                }
                MobjectTarget::Family(family) => {
                    return self
                        .effective_family_layout(family)
                        .map(observed_layout_bounds);
                }
                MobjectTarget::Object(_) => {}
            }
        }
        target.shape_matcher_bounds()
    }

    /// Prepare an Underline from this object's current layout.
    pub fn underline_options(
        &self,
        target: &Mobject,
        buff: f64,
    ) -> Result<ManimGeometryOptions, AuthoringError> {
        self.require_object(target)?;
        self.require_effective_capture()?;
        if self
            .running_execution()
            .is_some_and(|execution| execution.semantic_object_is_reachable(target.node_id()))
        {
            // Retain the authored bounds requirement for empty targets.
            target
                .layout_bounds()?
                .ok_or(AuthoringError::MissingLayoutBounds(target.node_id()))?;
            return ManimGeometryOptions::underline(
                observed_layout_bounds(self.effective_layout(target)?),
                buff,
            );
        }
        target.underline_options(buff)
    }
}

impl Mobject {
    pub fn manim_dot(
        store: Rc<RefCell<SemanticStore>>,
        x: f64,
        y: f64,
        radius: f64,
    ) -> Result<Self, AuthoringError> {
        Self::from_manim_geometry(store, ManimGeometryOptions::dot(x, y, radius)?)
    }

    pub fn manim_triangle(store: Rc<RefCell<SemanticStore>>) -> Result<Self, AuthoringError> {
        Self::from_manim_geometry(store, ManimGeometryOptions::triangle()?)
    }

    pub fn manim_elbow(
        store: Rc<RefCell<SemanticStore>>,
        width: f64,
        angle: f64,
    ) -> Result<Self, AuthoringError> {
        Self::from_manim_geometry(store, ManimGeometryOptions::elbow(width, angle)?)
    }

    pub fn manim_rounded_rectangle(
        store: Rc<RefCell<SemanticStore>>,
        width: f64,
        height: f64,
        corner_radius: f64,
    ) -> Result<Self, AuthoringError> {
        Self::from_manim_geometry(
            store,
            ManimGeometryOptions::rounded_rectangle(width, height, corner_radius)?,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn manim_annular_sector(
        store: Rc<RefCell<SemanticStore>>,
        inner_radius: f64,
        outer_radius: f64,
        angle: f64,
        start_angle: f64,
        num_components: u32,
        center_x: f64,
        center_y: f64,
    ) -> Result<Self, AuthoringError> {
        Self::from_manim_geometry(
            store,
            ManimGeometryOptions::annular_sector(
                inner_radius,
                outer_radius,
                angle,
                start_angle,
                num_components,
                center_x,
                center_y,
            )?,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn manim_sector(
        store: Rc<RefCell<SemanticStore>>,
        radius: f64,
        angle: f64,
        start_angle: f64,
        num_components: u32,
        center_x: f64,
        center_y: f64,
    ) -> Result<Self, AuthoringError> {
        Self::from_manim_geometry(
            store,
            ManimGeometryOptions::sector(
                radius,
                angle,
                start_angle,
                num_components,
                center_x,
                center_y,
            )?,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn manim_annulus(
        store: Rc<RefCell<SemanticStore>>,
        inner_radius: f64,
        outer_radius: f64,
        num_components: u32,
        center_x: f64,
        center_y: f64,
    ) -> Result<Self, AuthoringError> {
        Self::from_manim_geometry(
            store,
            ManimGeometryOptions::annulus(
                inner_radius,
                outer_radius,
                num_components,
                center_x,
                center_y,
            )?,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn manim_dashed_line(
        store: Rc<RefCell<SemanticStore>>,
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
        dash_length: f64,
        dashed_ratio: f64,
    ) -> Result<Self, AuthoringError> {
        Self::from_manim_geometry(
            store,
            ManimGeometryOptions::dashed_line(
                start_x,
                start_y,
                end_x,
                end_y,
                dash_length,
                dashed_ratio,
            )?,
        )
    }

    pub fn manim_underline(target: &Self, buff: f64) -> Result<Self, AuthoringError> {
        Self::from_manim_geometry(
            Rc::clone(target.integration_store()),
            target.underline_options(buff)?,
        )
    }

    /// Prepare an Underline from detached authored layout.
    pub fn underline_options(&self, buff: f64) -> Result<ManimGeometryOptions, AuthoringError> {
        self.validate()?;
        let bounds = self
            .layout_bounds()?
            .ok_or(AuthoringError::MissingLayoutBounds(self.node_id()))?;
        ManimGeometryOptions::underline(bounds, buff)
    }
}

fn manim_filled_path_style() -> SemanticStyle {
    let mut style = manim_style(Color::WHITE);
    style.fill_opacity = 1.0;
    style.stroke_width = 0.0;
    style
}
