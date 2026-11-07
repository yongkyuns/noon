//! Thin CPython coordinate inputs and immutable Rust frame observations.
use crate::{
    engine_error, family::FamilyHandle, geometry::GeometryOptions, mobject::MobjectHandle,
};
use noon::integration::{
    coordinate_failure, coordinate_math_failure, plot_data_points as data_points, plot_failure,
    AuthoringFailure,
};
use noon::{
    AxesFrame, CoordinateTicks, ManimAxes, ManimAxesOptions, ManimNumberLine,
    ManimNumberLineOptions, ManimThreeDAxes, ManimThreeDAxesFrame, ManimThreeDAxesOptions,
    NumberLineFrame, PlotSamplingOptions,
};
use noon::{ManimNumberPlaneOptions, SemanticPaint};
use noon::{ManimPolarPlaneOptions, PolarAzimuthDirection, PolarFrame};
use noon_core::SemanticNumberLineScale;
use pyo3::prelude::*;
use std::rc::Rc;
/// Inert input only. No family or shaft identity exists until `createCoordinates`.
#[pyclass(unsendable, module = "_noon_native")]
pub struct CoordinateOptions {
    pub(crate) request: CoordinateRequest,
}

#[derive(Clone)]
pub(crate) enum CoordinateRequest {
    NumberLine(ManimNumberLineOptions),
    Axes(ManimAxesOptions),
    NumberPlane(Box<noon::ManimNumberPlaneOptions>),
    PolarPlane(noon::ManimPolarPlaneOptions),
    ThreeDAxes(ManimThreeDAxesOptions),
}

fn coordinate_scale(kind: &str, parameter: f64) -> Result<SemanticNumberLineScale, PyErr> {
    let scale = match kind {
        "linear" => SemanticNumberLineScale::linear(parameter),
        "log" => SemanticNumberLineScale::log(parameter),
        _ => return Err(engine_error("unsupported coordinate scale kind")),
    };
    if !scale.is_valid() {
        return Err(engine_error("invalid coordinate scale parameter"));
    }
    Ok(scale)
}

fn range3(values: &[f64]) -> Result<[f64; 3], PyErr> {
    let [start, end, step] = values else {
        return Err(engine_error(AuthoringFailure::new(
            "invalid_input",
            "coordinate.range_shape",
            "coordinate range requires exactly three values",
        )));
    };
    let range = [*start, *end, *step];
    noon_geometry::validate_coordinate_range(range)
        .map_err(coordinate_math_failure)
        .map_err(engine_error)?;
    Ok(range)
}

impl CoordinateOptions {
    fn style_mut(&mut self) -> &mut noon::SemanticStyle {
        match &mut self.request {
            CoordinateRequest::NumberLine(options) => &mut options.style,
            CoordinateRequest::Axes(options) => &mut options.style,
            CoordinateRequest::NumberPlane(options) => &mut options.axis_style,
            CoordinateRequest::PolarPlane(options) => &mut options.axis_style,
            CoordinateRequest::ThreeDAxes(options) => &mut options.style,
        }
    }

    fn axis_overrides_mut(
        &mut self,
        axis: u32,
    ) -> Result<&mut noon::ManimThreeDAxisOverrides, PyErr> {
        let CoordinateRequest::ThreeDAxes(options) = &mut self.request else {
            return Err(engine_error("per-axis options require ThreeDAxes"));
        };
        options
            .axis_overrides
            .get_mut(axis as usize)
            .ok_or_else(|| engine_error("ThreeDAxes axis index must be 0, 1, or 2"))
    }
}

#[pymethods]
impl CoordinateOptions {
    #[pyo3(name = "numberLabels")]
    #[cfg(feature = "native-text")]
    #[staticmethod]
    pub fn number_labels(
        font: &str,
        font_size: f64,
        decimal_places: f64,
        buff: f64,
    ) -> Result<NumberLabelOptions, PyErr> {
        if !decimal_places.is_finite()
            || decimal_places.fract() != 0.0
            || !(0.0..=12.0).contains(&decimal_places)
        {
            return Err(label_failure(
                "number-label precision must be an integer between 0 and 12",
            ));
        }
        let font_size = noon::integration::authoring_render_f64("font size", font_size)
            .map(|v| v as f32)
            .map_err(engine_error)?;
        Ok(NumberLabelOptions {
            options: noon::plot_presentation::NumberLabelOptions {
                font: font.into(),
                font_size,
                decimal_places: decimal_places as u32,
                buff,
                ..Default::default()
            },
        })
    }
    #[pyo3(name = "polarPlane")]
    #[staticmethod]
    #[pyo3(signature=(radius_max,size,radius_step,azimuth_step,azimuth_offset,clockwise,faded_line_ratio))]
    pub fn polar_plane(
        radius_max: f64,
        size: Option<f64>,
        radius_step: f64,
        azimuth_step: Option<f64>,
        azimuth_offset: f64,
        clockwise: bool,
        faded_line_ratio: f64,
    ) -> Result<Self, PyErr> {
        if !faded_line_ratio.is_finite()
            || faded_line_ratio.fract() != 0.0
            || !(0.0..=u32::MAX as f64).contains(&faded_line_ratio)
        {
            return Err(engine_error(
                "faded_line_ratio must be a nonnegative integer",
            ));
        }
        Ok(Self {
            request: CoordinateRequest::PolarPlane(ManimPolarPlaneOptions {
                radius_max,
                size,
                radius_step,
                azimuth_step,
                azimuth_offset,
                azimuth_direction: if clockwise {
                    PolarAzimuthDirection::Clockwise
                } else {
                    PolarAzimuthDirection::Counterclockwise
                },
                faded_line_ratio: faded_line_ratio as u32,
                ..Default::default()
            }),
        })
    }

    #[pyo3(name = "setPolarLineStyle")]
    #[pyo3(signature=(faded,color,width,opacity))]
    pub fn set_polar_line_style(
        &mut self,
        faded: bool,
        color: Vec<f64>,
        width: Option<f64>,
        opacity: Option<f64>,
    ) -> Result<(), PyErr> {
        let CoordinateRequest::PolarPlane(options) = &mut self.request else {
            return Err(engine_error("grid style requires PolarPlane options"));
        };
        let style = if faded {
            options.faded_line_style_mut()
        } else {
            &mut options.background_line_style
        };
        if !color.is_empty() {
            let [r, g, b, a] = color.as_slice() else {
                return Err(engine_error("stroke color requires RGBA"));
            };
            style.stroke = noon::integration::family_color(true, *r, *g, *b, *a)
                .map_err(engine_error)?
                .map(SemanticPaint::Solid);
        }
        if let Some(value) = width {
            style.stroke_width = value;
        }
        if let Some(value) = opacity {
            style.stroke_opacity = value;
        }
        Ok(())
    }
    #[pyo3(name = "numberPlane")]
    #[staticmethod]
    #[pyo3(signature=(x_range,y_range,x_length,y_length,faded_line_ratio))]
    pub fn number_plane(
        x_range: Vec<f64>,
        y_range: Vec<f64>,
        x_length: Option<f64>,
        y_length: Option<f64>,
        faded_line_ratio: f64,
    ) -> Result<Self, PyErr> {
        let mut options = ManimNumberPlaneOptions::default();
        // Empty means omitted. Normalize two-value ranges in shared Rust input,
        // never in the Python facade's coordinate calculations.
        let range = |values: &[f64], default| match values {
            [] => Ok(default),
            [start, end] => Ok([*start, *end, 1.0]),
            [start, end, step] => Ok([*start, *end, *step]),
            _ => Err(engine_error(
                "NumberPlane range requires two or three values",
            )),
        };
        options.x_range = range(&x_range, options.x_range)?;
        options.y_range = range(&y_range, options.y_range)?;
        options.x_length = x_length;
        options.y_length = y_length;
        if !faded_line_ratio.is_finite()
            || faded_line_ratio.fract() != 0.0
            || !(0.0..=u32::MAX as f64).contains(&faded_line_ratio)
        {
            return Err(engine_error(
                "faded_line_ratio must be a nonnegative integer",
            ));
        }
        options.faded_line_ratio = faded_line_ratio as u32;
        Ok(Self {
            request: CoordinateRequest::NumberPlane(Box::new(options)),
        })
    }

    /// Partial style dictionary, supplied once before atomic construction.
    #[pyo3(name = "setPlaneLineStyle")]
    #[pyo3(signature=(faded,color,width,opacity))]
    pub fn set_plane_line_style(
        &mut self,
        faded: bool,
        color: Vec<f64>,
        width: Option<f64>,
        opacity: Option<f64>,
    ) -> Result<(), PyErr> {
        let CoordinateRequest::NumberPlane(options) = &mut self.request else {
            return Err(engine_error("grid style requires NumberPlane options"));
        };
        let style = if faded {
            options.faded_line_style_mut()
        } else {
            &mut options.background_line_style
        };
        if !color.is_empty() {
            let [r, g, b, a] = color.as_slice() else {
                return Err(engine_error("stroke color requires RGBA"));
            };
            style.stroke = noon::integration::family_color(true, *r, *g, *b, *a)
                .map_err(engine_error)?
                .map(SemanticPaint::Solid);
        }
        if let Some(value) = width {
            style.stroke_width = value;
        }
        if let Some(value) = opacity {
            style.stroke_opacity = value;
        }
        Ok(())
    }

    #[pyo3(name = "setPlaneAxisTicks")]
    pub fn set_plane_axis_ticks(
        &mut self,
        axis_index: u32,
        enabled: bool,
        tick_size: f64,
        exclude_origin: bool,
    ) -> Result<(), PyErr> {
        let CoordinateRequest::NumberPlane(options) = &mut self.request else {
            return Err(engine_error(
                "axis tick options require NumberPlane options",
            ));
        };
        let ticks = match axis_index {
            0 => &mut options.x_ticks,
            1 => &mut options.y_ticks,
            _ => return Err(engine_error("NumberPlane axis index must be 0 or 1")),
        };
        *ticks = CoordinateTicks {
            enabled,
            half_length: tick_size,
            exclude_origin,
            ..*ticks
        };
        Ok(())
    }

    #[pyo3(name = "setPlaneAxisTips")]
    pub fn set_plane_axis_tips(
        &mut self,
        axis_index: u32,
        enabled: bool,
        width: f64,
        height: f64,
    ) -> Result<(), PyErr> {
        let CoordinateRequest::NumberPlane(options) = &mut self.request else {
            return Err(engine_error("axis tip options require NumberPlane options"));
        };
        let (include_tip, tip_width, tip_height) = match axis_index {
            0 => (
                &mut options.x_include_tip,
                &mut options.x_tip_width,
                &mut options.x_tip_height,
            ),
            1 => (
                &mut options.y_include_tip,
                &mut options.y_tip_width,
                &mut options.y_tip_height,
            ),
            _ => return Err(engine_error("NumberPlane axis index must be 0 or 1")),
        };
        if !width.is_finite() || width <= 0.0 || !height.is_finite() || height <= 0.0 {
            return Err(engine_error(
                "NumberPlane tip width and height must be positive",
            ));
        }
        *include_tip = enabled;
        *tip_width = width;
        *tip_height = height;
        Ok(())
    }

    #[pyo3(name = "setPlaneAxisStyle")]
    #[pyo3(signature=(axis_index,color,width,stroke_opacity,opacity))]
    pub fn set_plane_axis_style(
        &mut self,
        axis_index: u32,
        color: Vec<f64>,
        width: Option<f64>,
        stroke_opacity: Option<f64>,
        opacity: Option<f64>,
    ) -> Result<(), PyErr> {
        let CoordinateRequest::NumberPlane(options) = &mut self.request else {
            return Err(engine_error("axis style requires NumberPlane options"));
        };
        let target = match axis_index {
            0 => &mut options.x_axis_style,
            1 => &mut options.y_axis_style,
            _ => return Err(engine_error("NumberPlane axis index must be 0 or 1")),
        };
        let mut style = target.as_ref().unwrap_or(&options.axis_style).clone();
        if !color.is_empty() {
            let [r, g, b, a] = color.as_slice() else {
                return Err(engine_error("axis stroke color requires RGBA"));
            };
            style.stroke = noon::integration::family_color(true, *r, *g, *b, *a)
                .map_err(engine_error)?
                .map(SemanticPaint::Solid);
        }
        if let Some(value) = width {
            style.stroke_width = value;
        }
        if let Some(value) = stroke_opacity {
            style.stroke_opacity = value;
        }
        if let Some(value) = opacity {
            style.object_opacity = value;
        }
        *target = Some(style);
        Ok(())
    }
    fn free(&self) {}

    #[pyo3(name = "numberLine")]
    #[staticmethod]
    #[pyo3(signature=(range,length,unit_size,rotation))]
    pub fn number_line(
        range: Vec<f64>,
        length: Option<f64>,
        unit_size: f64,
        rotation: f64,
    ) -> Result<Self, PyErr> {
        let mut options = ManimNumberLineOptions::from_range(if range.is_empty() {
            None
        } else {
            Some(range.as_slice())
        })
        .map_err(coordinate_failure)
        .map_err(engine_error)?;
        options.length = length;
        options.unit_size = unit_size;
        options.rotation = rotation;
        Ok(Self {
            request: CoordinateRequest::NumberLine(options),
        })
    }

    #[staticmethod]
    #[pyo3(signature=(x_range,y_range,x_length,y_length))]
    pub fn axes(
        x_range: Vec<f64>,
        y_range: Vec<f64>,
        x_length: Option<f64>,
        y_length: Option<f64>,
    ) -> Result<Self, PyErr> {
        let options = ManimAxesOptions::from_ranges(
            if x_range.is_empty() {
                None
            } else {
                Some(x_range.as_slice())
            },
            if y_range.is_empty() {
                None
            } else {
                Some(y_range.as_slice())
            },
            x_length,
            y_length,
        )
        .map_err(coordinate_failure)
        .map_err(engine_error)?;
        Ok(Self {
            request: CoordinateRequest::Axes(options),
        })
    }

    #[pyo3(name = "threeDAxes")]
    #[staticmethod]
    pub fn three_d_axes(
        x_range: Vec<f64>,
        y_range: Vec<f64>,
        z_range: Vec<f64>,
        x_length: f64,
        y_length: f64,
        z_length: f64,
    ) -> Result<Self, PyErr> {
        Ok(Self {
            request: CoordinateRequest::ThreeDAxes(ManimThreeDAxesOptions::new(
                range3(&x_range)?,
                range3(&y_range)?,
                range3(&z_range)?,
                x_length,
                y_length,
                z_length,
            )),
        })
    }

    #[pyo3(name = "setNumAxisPieces")]
    pub fn set_num_axis_pieces(&mut self, pieces: u32) -> Result<(), PyErr> {
        let CoordinateRequest::ThreeDAxes(options) = &mut self.request else {
            return Err(engine_error("axis pieces require ThreeDAxes"));
        };
        if !(1..=256).contains(&pieces) {
            return Err(engine_error(
                "ThreeDAxes num_axis_pieces must be between 1 and 256",
            ));
        }
        options.num_axis_pieces = pieces as usize;
        Ok(())
    }

    #[pyo3(name = "setAxisLightDirection")]
    pub fn set_axis_light_direction(&mut self, x: f64, y: f64, z: f64) -> Result<(), PyErr> {
        let CoordinateRequest::ThreeDAxes(options) = &mut self.request else {
            return Err(engine_error("axis light direction requires ThreeDAxes"));
        };
        let direction = noon::SemanticVec3::new(x, y, z);
        if !direction.is_finite()
            || direction
                .x
                .abs()
                .max(direction.y.abs())
                .max(direction.z.abs())
                == 0.0
        {
            return Err(engine_error(
                "ThreeDAxes light_source must be finite and nonzero",
            ));
        }
        options.light_source = direction;
        Ok(())
    }

    #[pyo3(name = "setShadeIn3D")]
    pub fn set_shade_in_3d(&mut self, enabled: bool) -> Result<(), PyErr> {
        let CoordinateRequest::ThreeDAxes(options) = &mut self.request else {
            return Err(engine_error("shade_in_3d requires ThreeDAxes"));
        };
        options.shade_in_3d = enabled;
        Ok(())
    }

    #[pyo3(name = "setTicks")]
    pub fn set_ticks(
        &mut self,
        enabled: bool,
        half_length: f64,
        exclude_origin: bool,
    ) -> Result<(), PyErr> {
        let ticks = match &mut self.request {
            CoordinateRequest::NumberLine(options) => &mut options.ticks,
            CoordinateRequest::Axes(options) => &mut options.ticks,
            CoordinateRequest::ThreeDAxes(options) => &mut options.ticks,
            CoordinateRequest::NumberPlane(_) | CoordinateRequest::PolarPlane(_) => {
                return Err(engine_error("plane ticks are not supported"));
            }
        };
        *ticks = CoordinateTicks {
            enabled,
            half_length,
            exclude_origin,
            ..*ticks
        };
        Ok(())
    }

    #[pyo3(name = "setTips")]
    pub fn set_tips(&mut self, enabled: bool) -> Result<(), PyErr> {
        match &mut self.request {
            CoordinateRequest::NumberLine(options) => options.include_tip = enabled,
            CoordinateRequest::Axes(options) => options.tips = enabled,
            CoordinateRequest::ThreeDAxes(options) => options.tips = enabled,
            _ => {
                return Err(engine_error(
                    "tips are configurable only for NumberLine, Axes, and ThreeDAxes",
                ));
            }
        }
        Ok(())
    }

    #[pyo3(name = "setNumberLineTipDimensions")]
    pub fn set_number_line_tip_dimensions(&mut self, width: f64, height: f64) -> Result<(), PyErr> {
        let CoordinateRequest::NumberLine(options) = &mut self.request else {
            return Err(engine_error("tip dimensions require a NumberLine"));
        };
        if !width.is_finite() || width <= 0.0 || !height.is_finite() || height <= 0.0 {
            return Err(engine_error(
                "NumberLine tip width and height must be positive",
            ));
        }
        options.tip_width = width;
        options.tip_height = height;
        Ok(())
    }

    #[pyo3(name = "setNumberLineScale")]
    pub fn set_number_line_scale(&mut self, kind: &str, parameter: f64) -> Result<(), PyErr> {
        let CoordinateRequest::NumberLine(options) = &mut self.request else {
            return Err(engine_error("scale settings require a NumberLine"));
        };
        options.scale = coordinate_scale(kind, parameter)?;
        Ok(())
    }

    #[pyo3(name = "setAxesScale")]
    pub fn set_axes_scale(&mut self, axis: u32, kind: &str, parameter: f64) -> Result<(), PyErr> {
        let CoordinateRequest::Axes(options) = &mut self.request else {
            return Err(engine_error("scale settings require Axes"));
        };
        let scale = coordinate_scale(kind, parameter)?;
        match axis {
            0 => options.x_scale = scale,
            1 => options.y_scale = scale,
            _ => return Err(engine_error("Axes axis index must be 0 or 1")),
        }
        Ok(())
    }

    #[pyo3(name = "setTipLength")]
    pub fn set_tip_length(&mut self, length: f64) -> Result<(), PyErr> {
        let CoordinateRequest::ThreeDAxes(options) = &mut self.request else {
            return Err(engine_error(
                "tip length is configurable only for ThreeDAxes",
            ));
        };
        options.tip_length = length;
        Ok(())
    }

    #[pyo3(name = "setAxisTicks")]
    pub fn set_axis_ticks(&mut self, axis: u32, enabled: bool) -> Result<(), PyErr> {
        self.axis_overrides_mut(axis)?.ticks_enabled = Some(enabled);
        Ok(())
    }

    #[pyo3(name = "setAxisTickSize")]
    pub fn set_axis_tick_size(&mut self, axis: u32, size: f64) -> Result<(), PyErr> {
        self.axis_overrides_mut(axis)?.tick_size = Some(size);
        Ok(())
    }

    #[pyo3(name = "setAxisExcludeOriginTick")]
    pub fn set_axis_exclude_origin_tick(&mut self, axis: u32, exclude: bool) -> Result<(), PyErr> {
        self.axis_overrides_mut(axis)?.exclude_origin_tick = Some(exclude);
        Ok(())
    }

    #[pyo3(name = "setAxisTips")]
    pub fn set_axis_tips(&mut self, axis: u32, enabled: bool) -> Result<(), PyErr> {
        self.axis_overrides_mut(axis)?.tips = Some(enabled);
        Ok(())
    }

    #[pyo3(name = "setAxisTipLength")]
    pub fn set_axis_tip_length(&mut self, axis: u32, length: f64) -> Result<(), PyErr> {
        self.axis_overrides_mut(axis)?.tip_length = Some(length);
        Ok(())
    }

    #[pyo3(name = "setAxisColor")]
    pub fn set_axis_color(
        &mut self,
        axis: u32,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), PyErr> {
        let color = noon::integration::family_color(true, red, green, blue, alpha)
            .map_err(|error| {
                engine_error(AuthoringFailure::new(
                    "invalid_input",
                    "coordinate.color",
                    error,
                ))
            })?
            .expect("enabled color");
        self.axis_overrides_mut(axis)?.color = Some(color);
        Ok(())
    }

    #[pyo3(name = "setAxisStrokeWidth")]
    pub fn set_axis_stroke_width(&mut self, axis: u32, width: f64) -> Result<(), PyErr> {
        self.axis_overrides_mut(axis)?.stroke_width = Some(width);
        Ok(())
    }

    #[pyo3(name = "setAxisStrokeOpacity")]
    pub fn set_axis_stroke_opacity(&mut self, axis: u32, opacity: f64) -> Result<(), PyErr> {
        self.axis_overrides_mut(axis)?.stroke_opacity = Some(opacity);
        Ok(())
    }

    #[pyo3(name = "setAxisOpacity")]
    pub fn set_axis_opacity(&mut self, axis: u32, opacity: f64) -> Result<(), PyErr> {
        self.axis_overrides_mut(axis)?.opacity = Some(opacity);
        Ok(())
    }

    #[pyo3(name = "setElongatedTicks")]
    pub fn set_elongated_ticks(&mut self, values: Vec<f64>, multiple: f64) -> Result<(), PyErr> {
        let CoordinateRequest::NumberLine(options) = &mut self.request else {
            return Err(engine_error("elongated ticks require a NumberLine"));
        };
        if values.len() > options.ticks.limit
            || values.iter().any(|x| !x.is_finite())
            || !multiple.is_finite()
            || multiple < 0.0
        {
            return Err(engine_error("invalid elongated ticks"));
        }
        options.numbers_with_elongated_ticks = values.to_vec();
        options.longer_tick_multiple = multiple;
        Ok(())
    }

    #[pyo3(name = "setColor")]
    pub fn set_color(&mut self, red: f64, green: f64, blue: f64, alpha: f64) -> Result<(), PyErr> {
        let color = noon::integration::family_color(true, red, green, blue, alpha)
            .map_err(|error| {
                engine_error(AuthoringFailure::new(
                    "invalid_input",
                    "coordinate.color",
                    error,
                ))
            })?
            .expect("enabled color");
        self.style_mut().stroke = Some(noon::SemanticPaint::Solid(color));
        Ok(())
    }

    #[pyo3(name = "setStrokeWidth")]
    pub fn set_stroke_width(&mut self, width: f64) {
        self.style_mut().stroke_width = width;
    }

    #[pyo3(name = "setStrokeOpacity")]
    pub fn set_stroke_opacity(&mut self, opacity: f64) {
        self.style_mut().stroke_opacity = opacity;
    }

    #[pyo3(name = "setObjectOpacity")]
    pub fn set_object_opacity(&mut self, opacity: f64) {
        self.style_mut().object_opacity = opacity;
    }
}

#[pyclass(unsendable, module = "_noon_native")]
pub struct NumberLineObservation {
    pub(crate) frame: NumberLineFrame,
}

#[pymethods]
impl NumberLineObservation {
    fn free(&self) {}

    pub fn range(&self) -> Vec<f64> {
        self.frame.range().to_vec()
    }

    pub fn start(&self) -> Vec<f64> {
        self.frame.start().to_vec()
    }

    pub fn end(&self) -> Vec<f64> {
        self.frame.end().to_vec()
    }

    pub fn vector(&self) -> Vec<f64> {
        self.frame.vector().to_vec()
    }

    pub fn length(&self) -> f64 {
        self.frame.length()
    }

    pub fn angle(&self) -> f64 {
        self.frame.angle()
    }

    pub fn slope(&self) -> f64 {
        self.frame.slope()
    }

    #[pyo3(name = "projectPoint")]
    pub fn project_point(&self, x: f64, y: f64) -> Result<Vec<f64>, PyErr> {
        self.frame
            .project_point([x, y])
            .map(|point| point.to_vec())
            .map_err(coordinate_math_failure)
            .map_err(engine_error)
    }

    #[pyo3(name = "tickValues")]
    pub fn tick_values(&self, include_tip: bool, exclude_origin: bool) -> Result<Vec<f64>, PyErr> {
        self.frame
            .tick_values(
                include_tip,
                exclude_origin,
                noon_geometry::DEFAULT_NUMBER_LINE_TICK_LIMIT,
            )
            .map_err(coordinate_math_failure)
            .map_err(engine_error)
    }

    #[pyo3(name = "tickSegment")]
    pub fn tick_segment(&self, number: f64, half_length: f64) -> Result<Vec<f64>, PyErr> {
        self.frame
            .tick_segment(number, half_length)
            .map(|points| points.into_iter().flatten().collect())
            .map_err(coordinate_math_failure)
            .map_err(engine_error)
    }

    #[pyo3(name = "unitSize")]
    pub fn unit_size(&self) -> f64 {
        self.frame.unit_size()
    }

    #[pyo3(name = "numberToPoint")]
    pub fn number_to_point(&self, value: f64) -> Result<Vec<f64>, PyErr> {
        self.frame
            .number_to_point(value)
            .map(|point| point.to_vec())
            .map_err(coordinate_math_failure)
            .map_err(engine_error)
    }

    #[pyo3(name = "pointToNumber")]
    pub fn point_to_number(&self, x: f64, y: f64) -> Result<f64, PyErr> {
        self.frame
            .point_to_number([x, y])
            .map_err(coordinate_math_failure)
            .map_err(engine_error)
    }
}

#[pyclass(unsendable, module = "_noon_native")]
pub struct AxesObservation {
    pub(crate) frame: AxesFrame,
}

#[pyclass(unsendable, module = "_noon_native")]
pub struct ThreeDAxesObservation {
    pub(crate) frame: ManimThreeDAxesFrame,
}

#[pymethods]
impl ThreeDAxesObservation {
    fn free(&self) {}

    #[pyo3(name = "coordsToPoint")]
    pub fn coords_to_point(&self, x: f64, y: f64, z: f64) -> Result<Vec<f64>, PyErr> {
        self.frame
            .c2p(x, y, z)
            .map(|point| vec![point.x, point.y, point.z])
            .ok_or_else(|| {
                engine_error(AuthoringFailure::new(
                    "invalid_input",
                    "coordinate.point",
                    "coordinates and result must be finite",
                ))
            })
    }

    #[pyo3(name = "pointToCoords")]
    pub fn point_to_coords(&self, x: f64, y: f64, z: f64) -> Result<Vec<f64>, PyErr> {
        self.frame
            .p2c(noon::SemanticVec3::new(x, y, z))
            .map(|coordinates| vec![coordinates.x, coordinates.y, coordinates.z])
            .ok_or_else(|| {
                engine_error(AuthoringFailure::new(
                    "invalid_input",
                    "coordinate.point",
                    "point and result must be finite",
                ))
            })
    }
}

#[pymethods]
impl AxesObservation {
    fn free(&self) {}

    #[pyo3(name = "coordsToPoint")]
    pub fn coords_to_point(&self, x: f64, y: f64) -> Result<Vec<f64>, PyErr> {
        self.frame
            .coords_to_point(x, y)
            .map(|point| point.to_vec())
            .map_err(coordinate_math_failure)
            .map_err(engine_error)
    }

    #[pyo3(name = "pointToCoords")]
    pub fn point_to_coords(&self, x: f64, y: f64) -> Result<Vec<f64>, PyErr> {
        self.frame
            .point_to_coords([x, y])
            .map(|point| point.to_vec())
            .map_err(coordinate_math_failure)
            .map_err(engine_error)
    }

    #[pyo3(name = "plotPlan")]
    #[pyo3(signature=(range,discontinuities,dt,max_samples))]
    pub fn plot_plan(
        &self,
        range: Vec<f64>,
        discontinuities: Vec<f64>,
        dt: Option<f64>,
        max_samples: Option<u32>,
    ) -> Result<crate::plotting::PlotSamplingPlan, PyErr> {
        let options = PlotSamplingOptions::axes(
            self.frame.x().range(),
            if range.is_empty() {
                None
            } else {
                Some(range.as_slice())
            },
        )
        .map_err(noon::PlotAuthoringError::from)
        .map_err(plot_failure)
        .map_err(engine_error)?;
        crate::plotting::PlotSamplingPlan::prepare(
            options,
            discontinuities,
            dt,
            max_samples,
            Some(self.frame),
        )
    }

    #[pyo3(name = "sampledPlot")]
    pub fn sampled_plot(&self, values: Vec<f64>) -> Result<GeometryOptions, PyErr> {
        noon::ManimGeometryOptions::axes_sampled_plot(
            self.frame,
            &data_points(&values).map_err(engine_error)?,
        )
        .map(GeometryOptions::from_options)
        .map_err(coordinate_failure)
        .map_err(engine_error)
    }

    /// Rust prepares the closed area path from this one captured frame; Python
    /// only applies normal VMobject style options to the returned inert request.
    #[pyo3(name = "area")]
    pub fn area(
        &self,
        graph: &MobjectHandle,
        range: Vec<f64>,
        bounded_graph: &MobjectHandle,
        has_bounded_graph: bool,
    ) -> Result<GeometryOptions, PyErr> {
        let graph = &graph.handle;
        let bounded = has_bounded_graph.then_some(&bounded_graph.handle);
        let graph_path = graph
            .path_query()
            .map_err(AuthoringFailure::from)
            .map_err(engine_error)?;
        let bounded_path = bounded
            .map(|object| object.path_query())
            .transpose()
            .map_err(AuthoringFailure::from)
            .map_err(engine_error)?;
        let range = match range.as_slice() {
            [] => None,
            [start, end] if start.is_finite() && end.is_finite() => Some([*start, *end]),
            _ => {
                return Err(engine_error(AuthoringFailure::new(
                    "invalid_input",
                    "area.range",
                    "area range requires two finite values",
                )));
            }
        };
        noon::ManimGeometryOptions::axes_area(
            self.frame,
            graph,
            &graph_path,
            range,
            bounded.zip(bounded_path.as_ref()),
        )
        .map(GeometryOptions::from_options)
        .map_err(coordinate_failure)
        .map_err(engine_error)
    }
}

pub(crate) fn create(
    store: &Rc<std::cell::RefCell<noon::integration::SemanticStore>>,
    candidate: &CoordinateOptions,
) -> PyResult<FamilyHandle> {
    let family = match &candidate.request {
        CoordinateRequest::NumberLine(options) => {
            ManimNumberLine::create(Rc::clone(store), options).map(|v| v.family().clone())
        }
        CoordinateRequest::Axes(options) => {
            ManimAxes::create(Rc::clone(store), options).map(|v| v.family().clone())
        }
        CoordinateRequest::NumberPlane(options) => {
            noon::ManimNumberPlane::create(Rc::clone(store), options).map(|v| v.family().clone())
        }
        CoordinateRequest::PolarPlane(options) => {
            noon::ManimPolarPlane::create(Rc::clone(store), options).map(|v| v.family().clone())
        }
        CoordinateRequest::ThreeDAxes(options) => {
            ManimThreeDAxes::create(Rc::clone(store), options).map(|v| v.family().clone())
        }
    };
    family
        .map(|family| FamilyHandle { family })
        .map_err(coordinate_failure)
        .map_err(engine_error)
}
pub(crate) fn live(
    context: &mut crate::context::Context,
    candidate: &CoordinateOptions,
) -> PyResult<FamilyHandle> {
    context.available()?;
    let Some(session) = context.execution.as_mut() else {
        return create(context.scene.integration_store(), candidate);
    };
    let mut live = context.scene.live(session);
    let family = match &candidate.request {
        CoordinateRequest::NumberLine(options) => {
            live.number_line(options).map(|v| v.family().clone())
        }
        CoordinateRequest::Axes(options) => live.axes(options).map(|v| v.family().clone()),
        CoordinateRequest::NumberPlane(options) => {
            live.number_plane(options).map(|v| v.family().clone())
        }
        CoordinateRequest::PolarPlane(options) => {
            live.polar_plane(options).map(|v| v.family().clone())
        }
        CoordinateRequest::ThreeDAxes(options) => {
            live.three_d_axes(options).map(|v| v.family().clone())
        }
    };
    family
        .map(|family| FamilyHandle { family })
        .map_err(coordinate_failure)
        .map_err(engine_error)
}
#[pyclass(unsendable, module = "_noon_native")]
pub struct PolarObservation {
    pub(crate) frame: PolarFrame,
}

#[pymethods]
impl PolarObservation {
    fn free(&self) {}
    #[pyo3(name = "polarToPoint")]
    pub fn polar_to_point(&self, radius: f64, azimuth: f64) -> Result<Vec<f64>, PyErr> {
        self.frame
            .polar_to_point(radius, azimuth)
            .map(|point| point.to_vec())
            .map_err(coordinate_math_failure)
            .map_err(engine_error)
    }
    #[pyo3(name = "pointToPolar")]
    pub fn point_to_polar(&self, x: f64, y: f64) -> Result<Vec<f64>, PyErr> {
        self.frame
            .point_to_polar([x, y])
            .map(|point| point.to_vec())
            .map_err(coordinate_math_failure)
            .map_err(engine_error)
    }
    #[pyo3(name = "axesFrame")]
    pub fn axes_frame(&self) -> AxesObservation {
        AxesObservation {
            frame: self.frame.axes(),
        }
    }
}

#[cfg(feature = "native-text")]
pub(crate) fn label_failure(error: impl std::fmt::Display) -> PyErr {
    engine_error(AuthoringFailure::new(
        "invalid_input",
        "plot.number_labels",
        error.to_string(),
    ))
}
#[cfg(feature = "native-text")]
#[pyclass(unsendable, module = "_noon_native")]
pub struct NumberLabelOptions {
    pub(crate) options: noon::plot_presentation::NumberLabelOptions,
}
#[cfg(feature = "native-text")]
#[pymethods]
impl NumberLabelOptions {
    fn free(&self) {}

    #[pyo3(name = "setTickRangeFlags")]
    pub fn set_tick_range_flags(&mut self, include_tip: bool, exclude_origin_tick: bool) {
        self.options.include_tip = include_tip;
        self.options.exclude_origin_tick = exclude_origin_tick;
    }

    #[pyo3(name = "setExcludedNumbers")]
    pub fn set_excluded_numbers(&mut self, values: Vec<f64>) -> Result<(), PyErr> {
        self.options
            .set_excluded_numbers(&values)
            .map_err(label_failure)
    }

    #[pyo3(name = "setDirection")]
    pub fn set_direction(&mut self, x: f64, y: f64) {
        self.options.direction = [x, y];
    }
    #[pyo3(name = "setExcludeZero")]
    pub fn set_exclude_zero(&mut self, exclude: bool) {
        self.options.exclude_zero = exclude;
    }
    #[pyo3(name = "setIncludeSign")]
    pub fn set_include_sign(&mut self, include: bool) {
        self.options.include_sign = include;
    }
    #[pyo3(name = "setGroupWithCommas")]
    pub fn set_group_with_commas(&mut self, group: bool) {
        self.options.group_with_commas = group;
    }
    #[pyo3(name = "setShowEllipsis")]
    pub fn set_show_ellipsis(&mut self, show: bool) {
        self.options.show_ellipsis = show;
    }
    #[pyo3(name = "setUnit")]
    pub fn set_unit(&mut self, unit: Option<String>) {
        self.options.unit = unit;
    }
    #[pyo3(name = "setColor")]
    pub fn set_color(&mut self, red: f64, green: f64, blue: f64, alpha: f64) -> Result<(), PyErr> {
        self.options.color = noon::integration::family_color(true, red, green, blue, alpha)
            .map_err(label_failure)?
            .expect("enabled label color");
        Ok(())
    }
}
