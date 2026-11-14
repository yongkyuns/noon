//! Inert constructor values forwarding directly to shared Rust validation.
use crate::engine_error;
use pyo3::prelude::*;
#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
#[derive(Clone)]
pub struct GeometryOptions {
    pub(crate) options: noon::ManimGeometryOptions,
}
impl GeometryOptions {
    pub(crate) fn from_options(options: noon::ManimGeometryOptions) -> Self {
        Self { options }
    }
}
#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub(crate) struct BooleanOperands(pub(crate) Vec<noon::Mobject>);

#[pymethods]
impl BooleanOperands {
    fn push(&mut self, object: &crate::mobject::MobjectHandle) {
        self.0.push(object.handle.clone());
    }

    fn free(&mut self) {
        self.0.clear();
    }
}

pub(crate) fn boolean_operation(name: &str) -> PyResult<noon::BooleanOperation> {
    use noon::BooleanOperation;
    match name {
        "Union" => Ok(BooleanOperation::Union),
        "Intersection" => Ok(BooleanOperation::Intersection),
        "Difference" => Ok(BooleanOperation::Difference),
        "Exclusion" => Ok(BooleanOperation::Exclusion),
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            "unknown boolean operation",
        )),
    }
}

#[pymethods]
impl GeometryOptions {
    #[staticmethod]
    #[pyo3(name = "booleanOperands")]
    fn boolean_operands() -> BooleanOperands {
        BooleanOperands(Vec::new())
    }

    #[staticmethod]
    #[pyo3(name = "booleanGeometry")]
    fn boolean_geometry(name: &str, operands: &BooleanOperands) -> PyResult<Self> {
        noon::ManimGeometryOptions::boolean_geometry(boolean_operation(name)?, &operands.0)
            .map(Self::from_options)
            .map_err(engine_error)
    }

    #[staticmethod]
    #[pyo3(name="polygram", signature=(coordinates,lengths))]
    #[allow(clippy::too_many_arguments)]
    pub fn polygram(coordinates: Vec<f64>, lengths: Vec<u32>) -> Result<Self, PyErr> {
        if !coordinates.len().is_multiple_of(2)
            || lengths.iter().map(|&n| u64::from(n)).sum::<u64>() != (coordinates.len() / 2) as u64
        {
            return Err(engine_error(noon::AuthoringError::InvalidPointCoordinates(
                coordinates.len(),
            )));
        }
        let mut offset = 0;
        let groups: Vec<_> = lengths
            .iter()
            .map(|&length| {
                let end = offset + length as usize * 2;
                let group = coordinates[offset..end]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|xy| (xy[0], xy[1]))
                    .collect();
                offset = end;
                group
            })
            .collect();
        noon::ManimGeometryOptions::polygram(&groups)
            .map(Self::from_options)
            .map_err(engine_error)
    }
    #[staticmethod]
    #[pyo3(name="star", signature=(n,outer_radius,inner_radius,density,start_angle))]
    #[allow(clippy::too_many_arguments)]
    pub fn star(
        n: u32,
        outer_radius: f64,
        inner_radius: Option<f64>,
        density: u32,
        start_angle: Option<f64>,
    ) -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::star(n, outer_radius, inner_radius, density, start_angle)
            .map(Self::from_options)
            .map_err(engine_error)
    }
    #[staticmethod]
    #[pyo3(name="angle", signature=(coordinates,radius,quadrant_x,quadrant_y,other_angle,elbow,num_components))]
    #[allow(clippy::too_many_arguments)]
    pub fn angle(
        coordinates: Vec<f64>,
        radius: Option<f64>,
        quadrant_x: f64,
        quadrant_y: f64,
        other_angle: bool,
        elbow: bool,
        num_components: u32,
    ) -> Result<Self, PyErr> {
        if coordinates.len() != 8 {
            return Err(engine_error(noon::AuthoringError::InvalidAngleOptions(
                "Angle requires two XY line endpoint pairs",
            )));
        }
        let first = [
            (coordinates[0], coordinates[1]),
            (coordinates[2], coordinates[3]),
        ];
        let second = [
            (coordinates[4], coordinates[5]),
            (coordinates[6], coordinates[7]),
        ];
        let options = noon::ManimAngleOptions {
            radius,
            quadrant: (quadrant_x, quadrant_y),
            other_angle,
            elbow,
            num_components,
        };
        noon::ManimGeometryOptions::angle_from_endpoints(first, second, options)
            .map(Self::from_options)
            .map_err(engine_error)
    }
    #[staticmethod]
    #[pyo3(name="crossLine", signature=(index,scale_factor))]
    #[allow(clippy::too_many_arguments)]
    pub fn cross_line(index: u32, scale_factor: f64) -> Result<Self, PyErr> {
        let [first, second] =
            noon::ManimGeometryOptions::cross_lines(None, scale_factor).map_err(engine_error)?;
        match index {
            0 => Ok(Self::from_options(first)),
            1 => Ok(Self::from_options(second)),
            _ => Err(engine_error("Cross line index must be 0 or 1")),
        }
    }
    #[staticmethod]
    #[pyo3(name="elbow", signature=(width,angle))]
    #[allow(clippy::too_many_arguments)]
    pub fn elbow(width: f64, angle: f64) -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::elbow(width, angle)
            .map(Self::from_options)
            .map_err(engine_error)
    }
    #[staticmethod]
    #[pyo3(name="roundedRectangle", signature=(width,height,corner_radius))]
    #[allow(clippy::too_many_arguments)]
    pub fn rounded_rectangle(width: f64, height: f64, corner_radius: f64) -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::rounded_rectangle(width, height, corner_radius)
            .map(Self::from_options)
            .map_err(engine_error)
    }
    #[staticmethod]
    #[pyo3(name="annularSector", signature=(inner_radius,outer_radius,angle,start_angle,num_components,center_x,center_y))]
    #[allow(clippy::too_many_arguments)]
    pub fn annular_sector(
        inner_radius: f64,
        outer_radius: f64,
        angle: f64,
        start_angle: f64,
        num_components: u32,
        center_x: f64,
        center_y: f64,
    ) -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::annular_sector(
            inner_radius,
            outer_radius,
            angle,
            start_angle,
            num_components,
            center_x,
            center_y,
        )
        .map(Self::from_options)
        .map_err(engine_error)
    }
    #[staticmethod]
    #[pyo3(name="sector", signature=(radius,angle,start_angle,num_components,center_x,center_y))]
    #[allow(clippy::too_many_arguments)]
    pub fn sector(
        radius: f64,
        angle: f64,
        start_angle: f64,
        num_components: u32,
        center_x: f64,
        center_y: f64,
    ) -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::sector(
            radius,
            angle,
            start_angle,
            num_components,
            center_x,
            center_y,
        )
        .map(Self::from_options)
        .map_err(engine_error)
    }
    #[staticmethod]
    #[pyo3(name="annulus", signature=(inner_radius,outer_radius,num_components,center_x,center_y))]
    #[allow(clippy::too_many_arguments)]
    pub fn annulus(
        inner_radius: f64,
        outer_radius: f64,
        num_components: u32,
        center_x: f64,
        center_y: f64,
    ) -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::annulus(
            inner_radius,
            outer_radius,
            num_components,
            center_x,
            center_y,
        )
        .map(Self::from_options)
        .map_err(engine_error)
    }
    #[staticmethod]
    #[pyo3(name="dashedLine", signature=(start_x,start_y,end_x,end_y,dash_length,dashed_ratio))]
    #[allow(clippy::too_many_arguments)]
    pub fn dashed_line(
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
        dash_length: f64,
        dashed_ratio: f64,
    ) -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::dashed_line(
            start_x,
            start_y,
            end_x,
            end_y,
            dash_length,
            dashed_ratio,
        )
        .map(Self::from_options)
        .map_err(engine_error)
    }
    #[staticmethod]
    #[pyo3(name="arc", signature=(radius,start_angle,angle,num_components,center_x,center_y))]
    #[allow(clippy::too_many_arguments)]
    pub fn arc(
        radius: f64,
        start_angle: f64,
        angle: f64,
        num_components: u32,
        center_x: f64,
        center_y: f64,
    ) -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::arc(
            radius,
            start_angle,
            angle,
            num_components,
            center_x,
            center_y,
        )
        .map(Self::from_options)
        .map_err(engine_error)
    }
    #[staticmethod]
    #[pyo3(name="arcBetweenPoints", signature=(start_x,start_y,end_x,end_y,angle,radius,num_components))]
    #[allow(clippy::too_many_arguments)]
    pub fn arc_between_points(
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
        angle: f64,
        radius: Option<f64>,
        num_components: u32,
    ) -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::arc_between_points(
            start_x,
            start_y,
            end_x,
            end_y,
            angle,
            radius,
            num_components,
        )
        .map(Self::from_options)
        .map_err(engine_error)
    }
    #[getter(explicitAngleRadius)]
    fn explicit_angle_radius(&self) -> Option<f64> {
        self.options.explicit_angle_radius()
    }
    #[staticmethod]
    #[pyo3(name="arcBetweenPointsMetadata",signature=(start_x,start_y,end_x,end_y,angle,radius))]
    fn arc_between_points_metadata(
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
        angle: f64,
        radius: Option<f64>,
    ) -> PyResult<ArcMetadata> {
        let (radius, angle) = noon::ManimGeometryOptions::arc_between_points_metadata(
            start_x, start_y, end_x, end_y, angle, radius,
        )
        .map_err(engine_error)?;
        Ok(ArcMetadata { radius, angle })
    }

    #[staticmethod]
    #[pyo3(name="regularPolygram",signature=(n,density,radius,start_angle))]
    fn regular_polygram(
        n: u32,
        density: u32,
        radius: f64,
        start_angle: Option<f64>,
    ) -> PyResult<Self> {
        noon::ManimGeometryOptions::regular_polygram(n, density, radius, start_angle)
            .map(Self::from_options)
            .map_err(engine_error)
    }
    #[getter(resolvedStartAngle)]
    fn resolved_start_angle(&self) -> Option<f64> {
        self.options.resolved_start_angle()
    }

    #[pyo3(name = "setDrawEnabled")]
    fn set_draw_enabled(&mut self, enabled: bool) {
        self.options.set_draw_enabled(enabled);
    }
    #[staticmethod]
    #[pyo3(name = "svgDefaultTransport", signature = (width, fill_color, fill_opacity, stroke_color, stroke_opacity, stroke_width))]
    fn svg_default_transport(
        width: Option<f64>,
        fill_color: Option<u32>,
        fill_opacity: Option<f64>,
        stroke_color: Option<u32>,
        stroke_opacity: Option<f64>,
        stroke_width: Option<f64>,
    ) -> PyResult<crate::svg::SvgDefaults> {
        crate::svg::defaults(
            width,
            fill_color,
            fill_opacity,
            stroke_color,
            stroke_opacity,
            stroke_width,
        )
    }
    #[staticmethod]
    #[pyo3(name = "braceBetweenPoints")]
    #[allow(clippy::too_many_arguments)]
    fn brace_between_points(
        first_x: f64,
        first_y: f64,
        second_x: f64,
        second_y: f64,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
        sharpness: f64,
    ) -> PyResult<Self> {
        noon::ManimGeometryOptions::brace_between_points(
            (first_x, first_y),
            (second_x, second_y),
            (direction_x, direction_y),
            buff,
            sharpness,
        )
        .map(Self::from_options)
        .map_err(engine_error)
    }

    #[pyo3(name = "setSheenDirection")]
    fn set_sheen_direction(&mut self, x: f64, y: f64, z: f64, dimensions: u8) -> PyResult<()> {
        self.options
            .set_sheen_direction(x, y, z, dimensions)
            .map_err(engine_error)
    }

    #[pyo3(name = "setZIndex")]
    pub fn set_z_index(&mut self, value: f64) -> Result<(), PyErr> {
        self.options.set_z_index(value).map_err(engine_error)
    }

    #[pyo3(name = "setTranslation")]
    pub fn set_translation(&mut self, x: f64, y: f64) -> Result<(), PyErr> {
        self.options.set_translation(x, y).map_err(engine_error)
    }

    #[pyo3(name = "setScale")]
    pub fn set_scale(&mut self, x: f64, y: f64) -> Result<(), PyErr> {
        self.options.set_scale(x, y).map_err(engine_error)
    }

    #[pyo3(name = "scaleBy")]
    pub fn scale_by(&mut self, x: f64, y: f64) -> Result<(), PyErr> {
        self.options.scale_by(x, y).map_err(engine_error)
    }

    #[pyo3(name = "setRotation")]
    pub fn set_rotation(&mut self, angle: f64) -> Result<(), PyErr> {
        self.options.set_rotation(angle).map_err(engine_error)
    }

    #[pyo3(name = "setColor")]
    pub fn set_color(&mut self, red: f64, green: f64, blue: f64, alpha: f64) -> Result<(), PyErr> {
        self.options
            .set_color(red, green, blue, alpha)
            .map_err(engine_error)
    }

    #[pyo3(name = "disableFill")]
    pub fn disable_fill(&mut self) {
        self.options.disable_fill();
    }

    #[pyo3(name = "setFill")]
    pub fn set_fill(&mut self, red: f64, green: f64, blue: f64, opacity: f64) -> Result<(), PyErr> {
        self.options
            .set_fill(red, green, blue, opacity)
            .map_err(engine_error)
    }

    #[pyo3(name = "setFillColor")]
    pub fn set_fill_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), PyErr> {
        self.options
            .set_fill_color(red, green, blue, alpha)
            .map_err(engine_error)
    }

    #[pyo3(name = "setFillOpacity")]
    pub fn set_fill_opacity(&mut self, opacity: f64) -> Result<(), PyErr> {
        self.options.set_fill_opacity(opacity).map_err(engine_error)
    }

    #[pyo3(name = "disableStroke")]
    pub fn disable_stroke(&mut self) {
        self.options.disable_stroke();
    }

    #[pyo3(name = "setStroke")]
    pub fn set_stroke(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        opacity: f64,
    ) -> Result<(), PyErr> {
        self.options
            .set_stroke(red, green, blue, opacity)
            .map_err(engine_error)
    }

    #[pyo3(name = "setStrokeColor")]
    pub fn set_stroke_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), PyErr> {
        self.options
            .set_stroke_color(red, green, blue, alpha)
            .map_err(engine_error)
    }

    #[pyo3(name = "setStrokeOpacity")]
    pub fn set_stroke_opacity(&mut self, opacity: f64) -> Result<(), PyErr> {
        self.options
            .set_stroke_opacity(opacity)
            .map_err(engine_error)
    }

    #[pyo3(name = "setStrokeWidth")]
    pub fn set_stroke_width(&mut self, width: f64) -> Result<(), PyErr> {
        self.options.set_stroke_width(width).map_err(engine_error)
    }

    #[pyo3(name = "setStrokeWidthMode")]
    pub fn set_stroke_width_mode(&mut self, mode: &str) -> Result<(), PyErr> {
        self.options
            .set_stroke_width_mode(mode)
            .map_err(engine_error)
    }

    #[pyo3(name = "setStrokeJoin")]
    pub fn set_stroke_join(&mut self, join: &str) -> Result<(), PyErr> {
        self.options.set_stroke_join(join).map_err(engine_error)
    }

    #[pyo3(name = "setStrokeCap")]
    pub fn set_stroke_cap(&mut self, cap: &str) -> Result<(), PyErr> {
        self.options.set_stroke_cap(cap).map_err(engine_error)
    }

    #[pyo3(name = "setObjectOpacity")]
    pub fn set_object_opacity(&mut self, opacity: f64) -> Result<(), PyErr> {
        self.options
            .set_object_opacity(opacity)
            .map_err(engine_error)
    }

    #[staticmethod]
    pub fn path(mut path: PyRefMut<'_, crate::vector_path::AuthoringVectorPath>) -> PyResult<Self> {
        noon::ManimGeometryOptions::path(path.take()?)
            .map(Self::from_options)
            .map_err(engine_error)
    }
    #[staticmethod]
    #[pyo3(name = "emptyPath")]
    pub fn empty_path() -> PyResult<Self> {
        noon::ManimGeometryOptions::path(noon_core::VectorPath::new())
            .map(Self::from_options)
            .map_err(engine_error)
    }
    #[staticmethod]
    pub fn circle(radius: f64) -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::circle(radius)
            .map(Self::from_options)
            .map_err(engine_error)
    }
    #[staticmethod]
    pub fn ellipse(width: f64, height: f64) -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::ellipse(width, height)
            .map(Self::from_options)
            .map_err(engine_error)
    }
    #[staticmethod]
    pub fn square(side: f64) -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::square(side)
            .map(Self::from_options)
            .map_err(engine_error)
    }
    #[staticmethod]
    pub fn rectangle(width: f64, height: f64) -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::rectangle(width, height)
            .map(Self::from_options)
            .map_err(engine_error)
    }
    #[staticmethod]
    pub fn line(start_x: f64, start_y: f64, end_x: f64, end_y: f64) -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::line(start_x, start_y, end_x, end_y)
            .map(Self::from_options)
            .map_err(engine_error)
    }
    #[staticmethod]
    #[pyo3(name = "dot")]
    pub fn dot(x: f64, y: f64, radius: f64) -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::dot(x, y, radius)
            .map(Self::from_options)
            .map_err(engine_error)
    }
    #[staticmethod]
    #[pyo3(name = "triangle")]
    pub fn triangle() -> Result<Self, PyErr> {
        noon::ManimGeometryOptions::triangle()
            .map(Self::from_options)
            .map_err(engine_error)
    }
}

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub struct ArcMetadata {
    #[pyo3(get)]
    radius: f64,
    #[pyo3(get)]
    angle: f64,
}

#[pymethods]
impl ArcMetadata {
    fn free(&self) {}
}
