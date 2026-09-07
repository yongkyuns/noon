//! Typed constructors for specialized Manim-compatible geometry.
use super::*;

impl Mobject {
    pub fn manim_dot(
        store: Rc<RefCell<SemanticStore>>,
        x: f64,
        y: f64,
        radius: f64,
    ) -> Result<Self, String> {
        let radius = positive_f32("radius", radius)?;
        let point = authoring_xy_f64(x, y)?;
        let transform = SemanticTransform2_5D {
            translation: point,
            ..Default::default()
        };
        let mut style = manim_style(Color::WHITE);
        style.fill_opacity = 1.0;
        style.stroke_width = 0.0;
        Self::from_geometry_state(store, GeometryRef::circle(radius), transform, style)
    }

    pub fn manim_triangle(store: Rc<RefCell<SemanticStore>>) -> Result<Self, String> {
        Self::from_geometry(
            store,
            GeometryRef::path(crate::geometry_authoring::manim_triangle_path()),
            manim_style(Color::BLUE),
        )
    }

    pub fn manim_elbow(
        store: Rc<RefCell<SemanticStore>>,
        width: f64,
        angle: f64,
    ) -> Result<Self, String> {
        let path = crate::elbow_authoring::manim_elbow_path(
            finite_f32("width", width)?,
            finite_f32("angle", angle)?,
        )
        .map_err(|error| error.to_string())?;
        Self::from_geometry(store, GeometryRef::path(path), manim_style(Color::WHITE))
    }

    pub fn manim_rounded_rectangle(
        store: Rc<RefCell<SemanticStore>>,
        width: f64,
        height: f64,
        corner_radius: f64,
    ) -> Result<Self, String> {
        let radius = finite_f32("corner_radius", corner_radius)?;
        let path = crate::rounded_rectangle_authoring::manim_rounded_rectangle_path(
            positive_f32("width", width)?,
            positive_f32("height", height)?,
            [radius; 4],
        )
        .map_err(|error| error.to_string())?;
        Self::from_geometry(store, GeometryRef::path(path), manim_style(Color::WHITE))
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
    ) -> Result<Self, String> {
        let geometry = crate::sector_authoring::annular_sector_geometry(
            inner_radius,
            outer_radius,
            angle,
            start_angle,
            num_components,
            center_x,
            center_y,
        )?;
        Self::from_geometry(store, geometry, manim_filled_path_style())
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
    ) -> Result<Self, String> {
        let geometry = crate::sector_authoring::sector_geometry(
            radius,
            angle,
            start_angle,
            num_components,
            center_x,
            center_y,
        )?;
        Self::from_geometry(store, geometry, manim_filled_path_style())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn manim_annulus(
        store: Rc<RefCell<SemanticStore>>,
        inner_radius: f64,
        outer_radius: f64,
        num_components: u32,
        center_x: f64,
        center_y: f64,
    ) -> Result<Self, String> {
        let geometry = crate::sector_authoring::annulus_geometry(
            inner_radius,
            outer_radius,
            num_components,
            center_x,
            center_y,
        )?;
        Self::from_geometry(store, geometry, manim_filled_path_style())
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
    ) -> Result<Self, String> {
        let geometry = crate::dashed_line_authoring::dashed_line_geometry(
            start_x,
            start_y,
            end_x,
            end_y,
            dash_length,
            dashed_ratio,
        )?;
        Self::from_geometry(store, geometry, manim_style(Color::WHITE))
    }

    pub fn manim_underline(target: &Self, buff: f64) -> Result<Self, String> {
        target.validate()?;
        let bounds = target
            .layout_bounds()?
            .ok_or("underline target has no finite bounds")?;
        let buff = authoring_render_f64("buff", buff)?;
        let center_x = (bounds.min_x + bounds.max_x) * 0.5;
        let half_width = (bounds.max_x - bounds.min_x) * 0.5;
        let y = bounds.min_y - buff;
        let geometry = GeometryRef::line(
            Vec2::new(
                finite_f32("underline.start.x", center_x - half_width)?,
                finite_f32("underline.start.y", y)?,
            ),
            Vec2::new(
                finite_f32("underline.end.x", center_x + half_width)?,
                finite_f32("underline.end.y", y)?,
            ),
        );
        Self::from_geometry(
            Rc::clone(target.store()),
            geometry,
            manim_style(Color::WHITE),
        )
    }
}

fn manim_filled_path_style() -> SemanticStyle {
    let mut style = manim_style(Color::WHITE);
    style.fill_opacity = 1.0;
    style.stroke_width = 0.0;
    style
}
