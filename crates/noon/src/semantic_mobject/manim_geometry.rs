//! Typed constructors for specialized Manim-compatible geometry.
use super::*;

impl ManimGeometryOptions {
    pub fn dot(x: f64, y: f64, radius: f64) -> Result<Self, String> {
        let mut style = manim_style(Color::WHITE);
        style.fill_opacity = 1.0;
        style.stroke_width = 0.0;
        let mut options = Self::new(GeometryRef::circle(positive_f32("radius", radius)?), style);
        options.set_translation(x, y)?;
        Ok(options)
    }

    pub fn triangle() -> Result<Self, String> {
        Ok(Self::new(
            GeometryRef::path(crate::geometry_authoring::manim_triangle_path()),
            manim_style(Color::BLUE),
        ))
    }

    pub fn elbow(width: f64, angle: f64) -> Result<Self, String> {
        let path = crate::elbow_authoring::manim_elbow_path(
            finite_f32("width", width)?,
            finite_f32("angle", angle)?,
        )
        .map_err(|error| error.to_string())?;
        Ok(Self::new(
            GeometryRef::path(path),
            manim_style(Color::WHITE),
        ))
    }

    pub fn rounded_rectangle(width: f64, height: f64, corner_radius: f64) -> Result<Self, String> {
        let radius = finite_f32("corner_radius", corner_radius)?;
        let path = crate::rounded_rectangle_authoring::manim_rounded_rectangle_path(
            positive_f32("width", width)?,
            positive_f32("height", height)?,
            [radius; 4],
        )
        .map_err(|error| error.to_string())?;
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
    ) -> Result<Self, String> {
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
    ) -> Result<Self, String> {
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
    ) -> Result<Self, String> {
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

    pub fn underline(bounds: Bounds2D64, buff: f64) -> Result<Self, String> {
        for (name, value) in [
            ("bounds.min_x", bounds.min_x),
            ("bounds.min_y", bounds.min_y),
            ("bounds.max_x", bounds.max_x),
            ("bounds.max_y", bounds.max_y),
        ] {
            authoring_render_f64(name, value)?;
        }
        if bounds.max_x < bounds.min_x || bounds.max_y < bounds.min_y {
            return Err("underline bounds must be ordered".into());
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

impl Mobject {
    pub fn manim_dot(
        store: Rc<RefCell<SemanticStore>>,
        x: f64,
        y: f64,
        radius: f64,
    ) -> Result<Self, String> {
        Self::from_manim_geometry(store, ManimGeometryOptions::dot(x, y, radius)?)
    }

    pub fn manim_triangle(store: Rc<RefCell<SemanticStore>>) -> Result<Self, String> {
        Self::from_manim_geometry(store, ManimGeometryOptions::triangle()?)
    }

    pub fn manim_elbow(
        store: Rc<RefCell<SemanticStore>>,
        width: f64,
        angle: f64,
    ) -> Result<Self, String> {
        Self::from_manim_geometry(store, ManimGeometryOptions::elbow(width, angle)?)
    }

    pub fn manim_rounded_rectangle(
        store: Rc<RefCell<SemanticStore>>,
        width: f64,
        height: f64,
        corner_radius: f64,
    ) -> Result<Self, String> {
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
    ) -> Result<Self, String> {
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
    ) -> Result<Self, String> {
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
    ) -> Result<Self, String> {
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
    ) -> Result<Self, String> {
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

    pub fn manim_underline(target: &Self, buff: f64) -> Result<Self, String> {
        target.validate().map_err(|error| error.to_string())?;
        let bounds = target
            .layout_bounds()?
            .ok_or("underline target has no finite bounds")?;
        Self::from_manim_geometry(
            Rc::clone(target.store()),
            ManimGeometryOptions::underline(bounds, buff)?,
        )
    }
}

fn manim_filled_path_style() -> SemanticStyle {
    let mut style = manim_style(Color::WHITE);
    style.fill_opacity = 1.0;
    style.stroke_width = 0.0;
    style
}
