//! Backend-neutral numeric text layout from immutable token resources.

use std::sync::Arc;

use crate::{
    format_decimal, DecimalFormat, GlyphRun, NumericFormatError, Rect, TextAffineTransform,
    TextPart, TextRenderItem, TextResource, TextResourceValidationError, TextSourceKind,
    TextSourceSpan, Vec2,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NumericTextPart {
    Digit,
    Sign,
    Minus,
    Comma,
    DecimalPoint,
    Ellipsis,
    Unit { superscript: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumericTextLayoutToken {
    pub tex: Arc<str>,
    pub span: TextSourceSpan,
    pub part: NumericTextPart,
}

impl NumericTextLayoutToken {
    pub fn tex(&self) -> &str {
        &self.tex
    }

    pub const fn span(&self) -> TextSourceSpan {
        self.span
    }

    pub const fn part(&self) -> NumericTextPart {
        self.part
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NumericTextResourceError {
    Format(NumericFormatError),
    InvalidSourceSpan,
    InvalidFontSize { bits: u32 },
    TokenCountMismatch,
    InvalidResource(TextResourceValidationError),
}

impl std::fmt::Display for NumericTextResourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Format(error) => error.fmt(formatter),
            Self::InvalidSourceSpan => formatter.write_str("invalid numeric text source span"),
            Self::InvalidFontSize { bits } => write!(
                formatter,
                "invalid numeric text font size {}",
                f32::from_bits(*bits)
            ),
            Self::TokenCountMismatch => {
                formatter.write_str("numeric token resources do not match the formatted value")
            }
            Self::InvalidResource(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for NumericTextResourceError {}

impl From<NumericFormatError> for NumericTextResourceError {
    fn from(value: NumericFormatError) -> Self {
        Self::Format(value)
    }
}

pub fn numeric_text_layout(
    value: f64,
    format: &DecimalFormat,
) -> Result<(Arc<str>, Vec<NumericTextLayoutToken>), NumericTextResourceError> {
    let source: Arc<str> = Arc::from(format_decimal(value, format)?);
    let mut numeric_end = source.len();
    if let Some(unit) = &format.unit {
        numeric_end = numeric_end
            .checked_sub(unit.len())
            .ok_or(NumericTextResourceError::InvalidSourceSpan)?;
    }
    let ellipsis_start = if format.show_ellipsis {
        numeric_end = numeric_end
            .checked_sub(3)
            .ok_or(NumericTextResourceError::InvalidSourceSpan)?;
        Some(numeric_end)
    } else {
        None
    };
    let mut tokens = Vec::new();
    for (start, character) in source[..numeric_end].char_indices() {
        let part = match character {
            '-' => NumericTextPart::Minus,
            '+' => NumericTextPart::Sign,
            ',' => NumericTextPart::Comma,
            '.' => NumericTextPart::DecimalPoint,
            '0'..='9' => NumericTextPart::Digit,
            _ => return Err(NumericTextResourceError::InvalidSourceSpan),
        };
        tokens.push(NumericTextLayoutToken {
            tex: Arc::from(character.to_string()),
            span: text_span(start, start + character.len_utf8())?,
            part,
        });
    }
    if let Some(start) = ellipsis_start {
        tokens.push(NumericTextLayoutToken {
            tex: Arc::from("\\dots"),
            span: text_span(start, start + 3)?,
            part: NumericTextPart::Ellipsis,
        });
    }
    if let Some(unit) = &format.unit {
        if !unit.is_empty() {
            tokens.push(NumericTextLayoutToken {
                tex: Arc::from(unit.as_str()),
                span: text_span(
                    numeric_end + usize::from(format.show_ellipsis) * 3,
                    source.len(),
                )?,
                part: NumericTextPart::Unit {
                    superscript: unit.starts_with('^'),
                },
            });
        }
    }
    Ok((source, tokens))
}

pub fn compose_numeric_text_resource(
    source: Arc<str>,
    tokens: &[NumericTextLayoutToken],
    children: &[&TextResource],
    font_size: f32,
    point_to_scene_scale: f32,
) -> Result<TextResource, NumericTextResourceError> {
    if tokens.len() != children.len() || tokens.is_empty() {
        return Err(NumericTextResourceError::TokenCountMismatch);
    }
    if !font_size.is_finite() || font_size <= 0.0 || !point_to_scene_scale.is_finite() {
        return Err(NumericTextResourceError::InvalidFontSize {
            bits: font_size.to_bits(),
        });
    }
    let scale = font_size * point_to_scene_scale;
    let scale_transform = TextAffineTransform {
        xx: scale,
        yy: scale,
        ..TextAffineTransform::IDENTITY
    };
    let digit_buff = 0.001 * font_size;
    let mut bounds = Vec::with_capacity(children.len());
    let mut transforms = Vec::with_capacity(children.len());
    let mut cursor = 0.0;
    for (index, child) in children.iter().enumerate() {
        let scaled = transform_rect(child.bounds, scale_transform);
        let placement = TextAffineTransform::translation(cursor - scaled.min.x, -scaled.min.y);
        let placed = transform_rect(scaled, placement);
        transforms.push(placement);
        bounds.push(placed);
        let gap = tokens.get(index + 1).map_or(0.0, |next| match next.part {
            NumericTextPart::Unit { .. } => 2.0 * digit_buff,
            _ => digit_buff,
        });
        cursor = placed.max.x + gap;
    }
    for (index, token) in tokens.iter().enumerate() {
        let height = bounds[index].height();
        let offset = match token.part {
            NumericTextPart::Minus if index + 1 < bounds.len() => {
                bounds[index + 1].height() / 2.0 - height
            }
            NumericTextPart::Comma => -height / 2.0,
            _ => 0.0,
        };
        if offset != 0.0 {
            transforms[index].ty += offset;
            bounds[index] =
                transform_rect(bounds[index], TextAffineTransform::translation(0.0, offset));
        }
    }
    let overall_top = bounds
        .iter()
        .map(|bound| bound.max.y)
        .fold(f32::NEG_INFINITY, f32::max);
    for (index, token) in tokens.iter().enumerate() {
        if matches!(token.part, NumericTextPart::Unit { superscript: true }) {
            let offset = overall_top - bounds[index].max.y;
            transforms[index].ty += offset;
            bounds[index] =
                transform_rect(bounds[index], TextAffineTransform::translation(0.0, offset));
        }
    }
    let overall = bounds
        .iter()
        .copied()
        .reduce(Rect::union)
        .ok_or(NumericTextResourceError::TokenCountMismatch)?;
    let recenter = TextAffineTransform::translation(-overall.center().x, -overall.center().y);
    let mut runs = Vec::<GlyphRun>::new();
    let mut vectors = Vec::new();
    let mut render_items = Vec::new();
    let mut parts = Vec::new();
    let mut cluster_ordinal = 0_u32;
    for ((token, child), placement) in tokens.iter().zip(children).zip(transforms) {
        let first_cluster = u32::try_from(runs.iter().map(|run| run.glyphs.len()).sum::<usize>())
            .map_err(|_| NumericTextResourceError::InvalidSourceSpan)?;
        let first_vector = u32::try_from(vectors.len())
            .map_err(|_| NumericTextResourceError::InvalidSourceSpan)?;
        let run_offset =
            u32::try_from(runs.len()).map_err(|_| NumericTextResourceError::InvalidSourceSpan)?;
        let vector_offset = u32::try_from(vectors.len())
            .map_err(|_| NumericTextResourceError::InvalidSourceSpan)?;
        let transform = scale_transform.then(placement).then(recenter);
        for run in child.runs.iter() {
            let mut run = run.clone();
            run.transform = run.transform.then(transform);
            for glyph in Arc::make_mut(&mut run.glyphs) {
                glyph.cluster.source_span = token.span;
                glyph.cluster.cluster_ordinal = cluster_ordinal;
                glyph.cluster.semantic_key = None;
                cluster_ordinal = cluster_ordinal
                    .checked_add(1)
                    .ok_or(NumericTextResourceError::InvalidSourceSpan)?;
            }
            runs.push(run);
        }
        for vector in child.vector_items.iter() {
            let mut vector = vector.clone();
            vector.transform = vector.transform.then(transform);
            vector.source_span = Some(token.span);
            vector.semantic_key = None;
            vectors.push(vector);
        }
        for item in child.render_items.iter().copied() {
            render_items.push(match item {
                TextRenderItem::GlyphRun(index) => TextRenderItem::GlyphRun(run_offset + index),
                TextRenderItem::Vector(index) => TextRenderItem::Vector(vector_offset + index),
            });
        }
        parts.push(TextPart {
            source_span: token.span,
            first_cluster,
            cluster_count: u32::try_from(child.glyph_count())
                .map_err(|_| NumericTextResourceError::InvalidSourceSpan)?,
            first_vector,
            vector_count: u32::try_from(child.vector_count())
                .map_err(|_| NumericTextResourceError::InvalidSourceSpan)?,
            semantic_key: None,
        });
    }
    let resource = TextResource {
        source,
        kind: TextSourceKind::MathTex,
        runs: runs.into(),
        vector_items: vectors.into(),
        render_items: render_items.into(),
        parts: parts.into(),
        bounds: Rect::new(
            overall.min - overall.center(),
            overall.max - overall.center(),
        ),
        baseline: 0.0,
        layout_artifact: None,
    };
    resource
        .validate()
        .map_err(NumericTextResourceError::InvalidResource)?;
    Ok(resource)
}

fn text_span(start: usize, end: usize) -> Result<TextSourceSpan, NumericTextResourceError> {
    Ok(TextSourceSpan::new(
        u32::try_from(start).map_err(|_| NumericTextResourceError::InvalidSourceSpan)?,
        u32::try_from(end).map_err(|_| NumericTextResourceError::InvalidSourceSpan)?,
    ))
}

fn transform_rect(rect: Rect, transform: TextAffineTransform) -> Rect {
    Rect::from_points([
        transform.transform_point(rect.min),
        transform.transform_point(Vec2::new(rect.min.x, rect.max.y)),
        transform.transform_point(Vec2::new(rect.max.x, rect.min.y)),
        transform.transform_point(rect.max),
    ])
    .expect("a rectangle has four corners")
}
