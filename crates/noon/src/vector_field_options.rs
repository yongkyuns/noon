//! Inert sampled field shared by foreign-language authoring hosts.

use crate::vector_field_authoring::{arrow_options, ColorPolicy};
use crate::{
    manim_default_vector_field_ranges_2d, ArrowVectorFieldAuthoringError as Error, Color,
    ManimArrowOptions, ManimArrowVectorField, VectorFieldAxisRange, VectorFieldPoint,
    VectorFieldRanges2D,
};
use noon_core::SemanticStore;
use noon_geometry::{
    default_vector_field_length, plan_static_arrow_vector_field, prepare_vector_field_sample,
};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Debug, Default)]
enum Coloring {
    #[default]
    Default,
    Single(Color),
    Gradient {
        colors: Vec<Color>,
        min: f64,
        max: f64,
        values: Option<Vec<Option<f64>>>,
    },
}

/// A sampled 2D vector field awaiting publication through a Scene.
#[derive(Clone, Debug)]
pub struct ArrowVectorFieldOptions {
    points: Vec<VectorFieldPoint>,
    vectors: Vec<Option<VectorFieldPoint>>,
    display_lengths: Option<Vec<Option<f64>>>,
    coloring: Coloring,
}

impl ArrowVectorFieldOptions {
    pub fn new(
        x: Option<VectorFieldAxisRange>,
        y: Option<VectorFieldAxisRange>,
        custom_length: bool,
    ) -> Result<Self, Error> {
        let defaults = manim_default_vector_field_ranges_2d();
        let ranges = VectorFieldRanges2D::new(x.unwrap_or(defaults.x), y.unwrap_or(defaults.y));
        let grid = plan_static_arrow_vector_field(|_| VectorFieldPoint::ZERO, ranges)?;
        let points = grid
            .samples
            .into_iter()
            .map(|sample| sample.point)
            .collect::<Vec<_>>();
        let count = points.len();
        Ok(Self {
            points,
            vectors: vec![None; count],
            display_lengths: custom_length.then(|| vec![None; count]),
            coloring: Coloring::Default,
        })
    }

    pub fn sample_count(&self) -> usize {
        self.points.len()
    }

    pub fn sample_point(&self, index: usize) -> Result<VectorFieldPoint, Error> {
        self.points
            .get(index)
            .copied()
            .ok_or(Error::SampleOutOfRange(index))
    }

    pub fn set_vector(&mut self, index: usize, x: f64, y: f64) -> Result<(), Error> {
        set_sample(&mut self.vectors, index, VectorFieldPoint::new(x, y))
    }

    pub fn set_display_length(&mut self, index: usize, value: f64) -> Result<(), Error> {
        set_sample(
            self.display_lengths.as_mut().ok_or(Error::DefaultLength)?,
            index,
            value,
        )
    }

    pub fn set_field_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), Error> {
        self.coloring = Coloring::Single(color(red, green, blue, alpha)?);
        Ok(())
    }

    pub fn set_color_gradient(&mut self, min: f64, max: f64, custom_scheme: bool) {
        self.coloring = Coloring::Gradient {
            colors: Vec::new(),
            min,
            max,
            values: custom_scheme.then(|| vec![None; self.sample_count()]),
        };
    }

    pub fn add_color_gradient_stop(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), Error> {
        let value = color(red, green, blue, alpha)?;
        let Coloring::Gradient { colors, .. } = &mut self.coloring else {
            return Err(Error::NotGradient);
        };
        colors.push(value);
        Ok(())
    }

    pub fn set_color_value(&mut self, index: usize, value: f64) -> Result<(), Error> {
        let Coloring::Gradient {
            values: Some(values),
            ..
        } = &mut self.coloring
        else {
            return Err(Error::DefaultColorScheme);
        };
        set_sample(values, index, value)
    }

    /// Publish a detached field when there is no active authoring Scene.
    #[doc(hidden)]
    pub fn publish_detached(
        self,
        store: Rc<RefCell<SemanticStore>>,
    ) -> Result<ManimArrowVectorField, Error> {
        ManimArrowVectorField::publish(store, self.prepare()?)
    }

    pub(crate) fn prepare(self) -> Result<Vec<ManimArrowOptions>, Error> {
        let vectors = self
            .vectors
            .into_iter()
            .enumerate()
            .map(|(index, vector)| vector.ok_or(Error::MissingVector(index)))
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(lengths) = &self.display_lengths {
            for (index, (vector, length)) in vectors.iter().zip(lengths).enumerate() {
                if vector.length() != 0.0 && length.is_none() {
                    return Err(Error::MissingLength(index));
                }
            }
        }
        if let Coloring::Gradient {
            values: Some(values),
            ..
        } = &self.coloring
        {
            for (index, value) in values.iter().enumerate() {
                if value.is_none() {
                    return Err(Error::MissingColorValue(index));
                }
            }
        }

        let samples = self
            .points
            .into_iter()
            .zip(vectors)
            .enumerate()
            .map(|(index, (point, vector))| {
                prepare_vector_field_sample(point, vector, index, |norm| {
                    self.display_lengths.as_ref().map_or_else(
                        || default_vector_field_length(norm),
                        |lengths| lengths[index].expect("nonzero vector lengths validated above"),
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        match self.coloring {
            Coloring::Default => arrow_options(&samples, ColorPolicy::Default),
            Coloring::Single(color) => arrow_options(&samples, ColorPolicy::Single(color)),
            Coloring::Gradient {
                colors,
                min,
                max,
                values,
            } => {
                let mut values = values.map(Vec::into_iter);
                let custom = values.is_some();
                let mut scheme = |_| {
                    values
                        .as_mut()
                        .and_then(Iterator::next)
                        .flatten()
                        .expect("sample color values validated above")
                };
                arrow_options(
                    &samples,
                    ColorPolicy::Gradient {
                        colors: &colors,
                        min,
                        max,
                        scheme: if custom { Some(&mut scheme) } else { None },
                    },
                )
            }
        }
    }
}

fn set_sample<T>(slots: &mut [Option<T>], index: usize, value: T) -> Result<(), Error> {
    *slots.get_mut(index).ok_or(Error::SampleOutOfRange(index))? = Some(value);
    Ok(())
}

fn color(red: f64, green: f64, blue: f64, alpha: f64) -> Result<Color, Error> {
    if [red, green, blue, alpha]
        .iter()
        .any(|value| !value.is_finite())
    {
        return Err(Error::InvalidColor);
    }
    Ok(Color::rgba(
        red as f32,
        green as f32,
        blue as f32,
        alpha as f32,
    ))
}
