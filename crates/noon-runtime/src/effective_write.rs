use noon_compile::ExecutionPatch;
use noon_core::{Color, ObjectId, Property, Style, Transform2D, Vec2};

use crate::{apply_evaluated_value, release_render_transform, EvaluatedValue, FrameRowMut};

/// One transient effective driver write, not a persistent semantic or plan edit.
///
/// Component writes preserve all other values in the prepared row, including
/// timeline/reactive values evaluated since the driver read the published frame.
/// Whole Transform/Style writes remain explicit whole-domain assignments for
/// callers that actually own those domains. Mixing both forms follows supplied
/// order, never an implicit priority based on the size of the write.
///
/// This vocabulary describes write scope, not an effect lifetime or a grant of
/// ownership. Acquisition, conflict handling and release remain the caller's
/// responsibility; preparing a batch does not bypass publication/replay guards.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EffectivePropertyWrite<I = ObjectId> {
    Presence { object: I, presence: bool },
    Transform { object: I, transform: Transform2D },
    Style { object: I, style: Style },
    Translation { object: I, translation: Vec2 },
    Rotation { object: I, rotation: f32 },
    Scale { object: I, scale: Vec2 },
    Fill { object: I, fill: Option<Color> },
    Stroke { object: I, stroke: Option<Color> },
    StrokeWidth { object: I, stroke_width: f32 },
    Opacity { object: I, opacity: f32 },
}

impl<I: Copy> EffectivePropertyWrite<I> {
    pub const fn object(self) -> I {
        match self {
            Self::Presence { object, .. }
            | Self::Transform { object, .. }
            | Self::Style { object, .. }
            | Self::Translation { object, .. }
            | Self::Rotation { object, .. }
            | Self::Scale { object, .. }
            | Self::Fill { object, .. }
            | Self::Stroke { object, .. }
            | Self::StrokeWidth { object, .. }
            | Self::Opacity { object, .. } => object,
        }
    }

    /// Resolve identity at the semantic-to-execution boundary without translating
    /// or duplicating the effective write vocabulary.
    pub fn map_object<J>(self, map: impl FnOnce(I) -> J) -> EffectivePropertyWrite<J> {
        let object = map(self.object());
        match self {
            Self::Presence { presence, .. } => {
                EffectivePropertyWrite::Presence { object, presence }
            }
            Self::Transform { transform, .. } => {
                EffectivePropertyWrite::Transform { object, transform }
            }
            Self::Style { style, .. } => EffectivePropertyWrite::Style { object, style },
            Self::Translation { translation, .. } => EffectivePropertyWrite::Translation {
                object,
                translation,
            },
            Self::Rotation { rotation, .. } => {
                EffectivePropertyWrite::Rotation { object, rotation }
            }
            Self::Scale { scale, .. } => EffectivePropertyWrite::Scale { object, scale },
            Self::Fill { fill, .. } => EffectivePropertyWrite::Fill { object, fill },
            Self::Stroke { stroke, .. } => EffectivePropertyWrite::Stroke { object, stroke },
            Self::StrokeWidth { stroke_width, .. } => EffectivePropertyWrite::StrokeWidth {
                object,
                stroke_width,
            },
            Self::Opacity { opacity, .. } => EffectivePropertyWrite::Opacity { object, opacity },
        }
    }
}

impl EffectivePropertyWrite {
    /// Reuse the compiler's existing identity/numeric validators. Defaults fill
    /// only the irrelevant validation fields; these patches are never committed
    /// to the execution plan or used to assign a whole effective row.
    pub(crate) fn as_execution_patch(self) -> ExecutionPatch {
        match self {
            Self::Presence { object, .. } => ExecutionPatch::SetTransform {
                object,
                transform: Transform2D::IDENTITY,
            },
            Self::Transform { object, transform } => {
                ExecutionPatch::SetTransform { object, transform }
            }
            Self::Style { object, style } => ExecutionPatch::SetStyle { object, style },
            Self::Translation {
                object,
                translation,
            } => ExecutionPatch::SetTransform {
                object,
                transform: Transform2D {
                    translation,
                    ..Transform2D::IDENTITY
                },
            },
            Self::Rotation { object, rotation } => ExecutionPatch::SetTransform {
                object,
                transform: Transform2D {
                    rotation,
                    ..Transform2D::IDENTITY
                },
            },
            Self::Scale { object, scale } => ExecutionPatch::SetTransform {
                object,
                transform: Transform2D {
                    scale,
                    ..Transform2D::IDENTITY
                },
            },
            Self::Fill { object, fill } => ExecutionPatch::SetStyle {
                object,
                style: Style {
                    fill,
                    ..Style::default()
                },
            },
            Self::Stroke { object, stroke } => ExecutionPatch::SetStyle {
                object,
                style: Style {
                    stroke,
                    ..Style::default()
                },
            },
            Self::StrokeWidth {
                object,
                stroke_width,
            } => ExecutionPatch::SetStyle {
                object,
                style: Style {
                    stroke_width,
                    ..Style::default()
                },
            },
            Self::Opacity { object, opacity } => ExecutionPatch::SetStyle {
                object,
                style: Style {
                    opacity,
                    ..Style::default()
                },
            },
        }
    }
}

pub(crate) fn apply_effective_property_to_row(
    mut row: FrameRowMut<'_>,
    write: EffectivePropertyWrite,
) {
    let (property, value) = match write {
        EffectivePropertyWrite::Presence { presence, .. } => {
            *row.presence = presence;
            return;
        }
        EffectivePropertyWrite::Transform { transform, .. } => {
            release_render_transform(row.render_geometry, row.render_transform, *row.transform);
            *row.transform = transform;
            return;
        }
        EffectivePropertyWrite::Style { style, .. } => {
            *row.style = style;
            return;
        }
        EffectivePropertyWrite::Translation { translation, .. } => {
            (Property::Position, EvaluatedValue::Vec2(translation))
        }
        EffectivePropertyWrite::Rotation { rotation, .. } => {
            (Property::Rotation, EvaluatedValue::Scalar(rotation))
        }
        EffectivePropertyWrite::Scale { scale, .. } => {
            (Property::Scale, EvaluatedValue::Vec2(scale))
        }
        EffectivePropertyWrite::Fill { fill, .. } => (Property::Fill, EvaluatedValue::Color(fill)),
        EffectivePropertyWrite::Stroke { stroke, .. } => {
            (Property::Stroke, EvaluatedValue::Color(stroke))
        }
        EffectivePropertyWrite::StrokeWidth { stroke_width, .. } => {
            (Property::StrokeWidth, EvaluatedValue::Scalar(stroke_width))
        }
        EffectivePropertyWrite::Opacity { opacity, .. } => {
            (Property::Opacity, EvaluatedValue::Scalar(opacity))
        }
    };
    // Use exactly the normal evaluator's channel assignment and prepared-morph
    // render-frame release. No parallel transform/style application semantics.
    apply_evaluated_value(&mut row, property, value, false);
}

#[cfg(test)]
mod tests;
