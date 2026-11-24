//! Common language binding input for retained LaTeX parts.

use super::{authoring_render_f64, family_color, AuthoringFailure};
use crate::{Color, LatexBackend, LatexParts, MathTex, Scene, Tex, TextAuthoringError};
use noon_core::SemanticStore;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone)]
pub enum LatexInput {
    Text(Tex),
    Math(MathTex),
}

impl LatexInput {
    pub fn new(
        source: &str,
        math: bool,
        font_size: f64,
        color: &[f64],
        opacity: f64,
    ) -> Result<Self, AuthoringFailure> {
        let (size, color, opacity) = presentation(font_size, color, opacity)?;
        let text = if math {
            Self::Math(MathTex::new(source)?)
        } else {
            Self::Text(Tex::new(source)?)
        };
        Ok(text.with_presentation(size, color, opacity))
    }

    pub fn from_strings(
        strings: Vec<String>,
        math: bool,
        font_size: f64,
        color: &[f64],
        opacity: f64,
    ) -> Result<Self, AuthoringFailure> {
        let (size, color, opacity) = presentation(font_size, color, opacity)?;
        let text = if math {
            Self::Math(MathTex::from_strings(strings)?)
        } else {
            Self::Text(Tex::from_strings(strings)?)
        };
        Ok(text.with_presentation(size, color, opacity))
    }

    fn with_presentation(self, size: f32, color: Color, opacity: f32) -> Self {
        match self {
            Self::Text(text) => {
                Self::Text(text.with_font_size(size).color(color).set_opacity(opacity))
            }
            Self::Math(text) => {
                Self::Math(text.with_font_size(size).color(color).set_opacity(opacity))
            }
        }
    }

    pub fn with_isolated_substrings(
        self,
        substrings: &[String],
    ) -> Result<Self, TextAuthoringError> {
        match self {
            Self::Text(text) => text.with_isolated_substrings(substrings).map(Self::Text),
            Self::Math(text) => text.with_isolated_substrings(substrings).map(Self::Math),
        }
    }

    pub fn publish_detached(
        self,
        store: Rc<RefCell<SemanticStore>>,
        backend: &mut impl LatexBackend,
    ) -> Result<LatexParts, TextAuthoringError> {
        match self {
            Self::Text(text) => LatexParts::from_tex(store, text, backend),
            Self::Math(text) => LatexParts::from_math_tex(store, text, backend),
        }
    }

    pub fn publish(
        self,
        scene: &mut Scene,
        backend: &mut impl LatexBackend,
    ) -> Result<LatexParts, TextAuthoringError> {
        match self {
            Self::Text(text) => scene.tex_parts(text, backend),
            Self::Math(text) => scene.math_tex_parts(text, backend),
        }
    }
}

fn presentation(
    font_size: f64,
    color: &[f64],
    opacity: f64,
) -> Result<(f32, Color, f32), AuthoringFailure> {
    let size = authoring_render_f64("font size", font_size)? as f32;
    let opacity = authoring_render_f64("opacity", opacity)? as f32;
    if !color.len().is_multiple_of(4) {
        return Err(AuthoringFailure::new(
            "invalid_input",
            "gradient.invalid_components",
            "gradient colors require RGBA components",
        ));
    }
    let mut colors = color.as_chunks::<4>().0.iter().map(|rgba| {
        family_color(true, rgba[0], rgba[1], rgba[2], rgba[3])
            .map(|color| color.expect("enabled color"))
            .map_err(|error| {
                AuthoringFailure::new("invalid_input", "gradient.invalid_color", error)
            })
    });
    if colors.len() == 1 {
        return Ok((size, colors.next().expect("one color")?, opacity));
    }
    colors.try_for_each(|value| value.map(|_| ()))?;
    Err("One LaTeX color is required".into())
}

pub fn latex_member_colors(values: &[f64]) -> Result<Vec<Option<Color>>, AuthoringFailure> {
    if !values.len().is_multiple_of(4) {
        return Err("member colors require complete RGBA tuples".into());
    }
    values
        .as_chunks::<4>()
        .0
        .iter()
        .map(|rgba| {
            if rgba[0].is_nan() {
                return Ok(None);
            }
            Ok(Some(Color::rgba(
                authoring_render_f64("member red", rgba[0])? as f32,
                authoring_render_f64("member green", rgba[1])? as f32,
                authoring_render_f64("member blue", rgba[2])? as f32,
                authoring_render_f64("member alpha", rgba[3])? as f32,
            )))
        })
        .collect()
}
