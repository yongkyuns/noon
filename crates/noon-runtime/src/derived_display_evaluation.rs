use noon_compile::TransformGeometryPlan;
use noon_core::{
    mapped_continuous_progress, CompositionTimeMap, ObjectContentRef, Property, TrackTiming,
    TrackValues,
};

use crate::frame::FrameRowState;
use crate::{DerivedDisplayObject, DerivedDisplayObjectState};

/// One identity-free runtime channel for a transient derived display occurrence.
///
/// This execution data intentionally carries no `TrackId`, `ObjectId`, semantic node,
/// or stable slot. The compiler has already prepared any geometry plan required by
/// Morph evaluation.
#[derive(Clone, Debug, PartialEq)]
pub struct DerivedDisplayAnimationTrack {
    pub property: Property,
    pub values: TrackValues,
    pub timing: TrackTiming,
    pub time_map: CompositionTimeMap,
    pub transform_geometry_plan: Option<TransformGeometryPlan>,
}

/// One plan-local visual occurrence anchored only for painter placement.
#[derive(Clone, Debug, PartialEq)]
pub struct DerivedDisplayAnimationOccurrence {
    pub anchor_object_index: u32,
    pub occurrence_index: u32,
    pub base: DerivedDisplayObjectState,
    pub tracks: Vec<DerivedDisplayAnimationTrack>,
}

/// Renderer-publication overlay evaluated independently of stable `FrameState.objects`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DerivedDisplayAnimationPlan {
    occurrences: Vec<DerivedDisplayAnimationOccurrence>,
}

impl DerivedDisplayAnimationPlan {
    pub fn new(
        occurrences: Vec<DerivedDisplayAnimationOccurrence>,
    ) -> Result<Self, DerivedDisplayEvaluationError> {
        let mut seen = std::collections::BTreeSet::new();
        for occurrence in &occurrences {
            if !seen.insert(occurrence.occurrence_index) {
                return Err(DerivedDisplayEvaluationError::DuplicateOccurrence(
                    occurrence.occurrence_index,
                ));
            }
            if occurrence.tracks.is_empty() {
                return Err(DerivedDisplayEvaluationError::EmptyOccurrence(
                    occurrence.occurrence_index,
                ));
            }
            for track in &occurrence.tracks {
                track.time_map.validate().map_err(|_| {
                    DerivedDisplayEvaluationError::InvalidTimeMap {
                        occurrence_index: occurrence.occurrence_index,
                        property: track.property,
                    }
                })?;
            }
        }
        Ok(Self { occurrences })
    }

    pub fn occurrences(&self) -> &[DerivedDisplayAnimationOccurrence] {
        &self.occurrences
    }

    pub fn is_empty(&self) -> bool {
        self.occurrences.is_empty()
    }

    /// Evaluate one coherent derived display overlay.
    ///
    /// A mapped occurrence does not exist before its first channel begins. Once begun,
    /// it remains at its mapped endpoint after the root interval. The owning session
    /// decides when that retained effective presentation state is superseded; this
    /// evaluator never turns it into semantic identity or stable execution storage.
    pub fn evaluate(
        &self,
        time: f64,
    ) -> Result<Vec<DerivedDisplayObject>, DerivedDisplayEvaluationError> {
        if !time.is_finite() {
            return Err(DerivedDisplayEvaluationError::InvalidTime(time));
        }
        let mut objects = Vec::with_capacity(self.occurrences.len());
        for occurrence in &self.occurrences {
            let first = &occurrence.tracks[0];
            if mapped_continuous_progress(first.timing, &first.time_map, time).is_none() {
                continue;
            }

            let base_content = occurrence.base.content.clone();
            let mut row = row_from_derived(&occurrence.base);
            for track in &occurrence.tracks {
                let Some(progress) =
                    mapped_continuous_progress(track.timing, &track.time_map, time)
                else {
                    continue;
                };
                apply_derived_track(
                    &mut row,
                    &base_content,
                    track,
                    progress,
                    occurrence.occurrence_index,
                )?;
            }
            objects.push(DerivedDisplayObject::new(
                occurrence.anchor_object_index,
                occurrence.occurrence_index,
                derived_from_row(occurrence.base.text_bounds, base_content, row),
            ));
        }
        Ok(objects)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DerivedDisplayEvaluationError {
    InvalidTime(f64),
    DuplicateOccurrence(u32),
    EmptyOccurrence(u32),
    InvalidTimeMap {
        occurrence_index: u32,
        property: Property,
    },
    UnsupportedProperty {
        occurrence_index: u32,
        property: Property,
    },
    InvalidValueKind {
        occurrence_index: u32,
        property: Property,
    },
    MissingGeometryPlan {
        occurrence_index: u32,
        property: Property,
    },
}

impl std::fmt::Display for DerivedDisplayEvaluationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::InvalidTime(time) => write!(formatter, "invalid derived display time {time}"),
            Self::DuplicateOccurrence(index) => {
                write!(formatter, "derived display occurrence {index} is duplicated")
            }
            Self::EmptyOccurrence(index) => {
                write!(formatter, "derived display occurrence {index} has no channels")
            }
            Self::InvalidTimeMap {
                occurrence_index,
                property,
            } => write!(
                formatter,
                "derived display occurrence {occurrence_index} has invalid {property:?} time map"
            ),
            Self::UnsupportedProperty {
                occurrence_index,
                property,
            } => write!(
                formatter,
                "derived display occurrence {occurrence_index} does not support {property:?} evaluation"
            ),
            Self::InvalidValueKind {
                occurrence_index,
                property,
            } => write!(
                formatter,
                "derived display occurrence {occurrence_index} has invalid values for {property:?}"
            ),
            Self::MissingGeometryPlan {
                occurrence_index,
                property,
            } => write!(
                formatter,
                "derived display occurrence {occurrence_index} lacks the compiled {property:?} geometry plan"
            ),
        }
    }
}

impl std::error::Error for DerivedDisplayEvaluationError {}

fn row_from_derived(base: &DerivedDisplayObjectState) -> FrameRowState {
    FrameRowState {
        z_index: base.z_index,
        transform: base.transform,
        style: base.style,
        appearance: base.appearance,
        presence: base.presence,
        reveal: base.reveal,
        morph: base.morph,
        content_override: None,
        render_geometry: base.render_geometry.clone(),
        render_transform: base.render_transform,
    }
}

fn derived_from_row(
    text_bounds: Option<noon_core::Rect>,
    base_content: ObjectContentRef,
    row: FrameRowState,
) -> DerivedDisplayObjectState {
    DerivedDisplayObjectState {
        z_index: row.z_index,
        content: row.content_override.unwrap_or(base_content),
        text_bounds,
        transform: row.transform,
        style: row.style,
        appearance: row.appearance,
        presence: row.presence,
        reveal: row.reveal,
        morph: row.morph,
        render_geometry: row.render_geometry,
        render_transform: row.render_transform,
    }
}

fn apply_derived_track(
    row: &mut FrameRowState,
    base_content: &ObjectContentRef,
    track: &DerivedDisplayAnimationTrack,
    progress: f32,
    occurrence_index: u32,
) -> Result<(), DerivedDisplayEvaluationError> {
    match track.property {
        Property::Morph => {
            let plan = track.transform_geometry_plan.as_ref().ok_or(
                DerivedDisplayEvaluationError::MissingGeometryPlan {
                    occurrence_index,
                    property: track.property,
                },
            )?;
            crate::apply_prepared_morph_values(
                &mut row.as_mut(base_content),
                &track.values,
                plan,
                progress,
            )
            .ok_or(DerivedDisplayEvaluationError::InvalidValueKind {
                occurrence_index,
                property: track.property,
            })?;
            Ok(())
        }
        Property::Position
        | Property::Rotation
        | Property::Scale
        | Property::Fill
        | Property::Stroke
        | Property::StrokeWidth
        | Property::Opacity
        | Property::Appearance
        | Property::Reveal => {
            let value = crate::interpolate_track_values(&track.values, progress).ok_or(
                DerivedDisplayEvaluationError::InvalidValueKind {
                    occurrence_index,
                    property: track.property,
                },
            )?;
            crate::apply_evaluated_value(
                &mut row.as_mut(base_content),
                track.property,
                value,
                false,
            );
            Ok(())
        }
        Property::Presence | Property::ZIndex | Property::Transform => {
            Err(DerivedDisplayEvaluationError::UnsupportedProperty {
                occurrence_index,
                property: track.property,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use noon_core::{Color, GeometryRef, RateFunction, Style, Transform2D, Vec2, VectorPath};

    use super::*;

    fn base(geometry: GeometryRef) -> DerivedDisplayObjectState {
        DerivedDisplayObjectState {
            z_index: 0.0,
            content: ObjectContentRef::Geometry(geometry),
            text_bounds: None,
            transform: Transform2D::IDENTITY,
            style: Style::default(),
            appearance: 1.0,
            presence: true,
            reveal: 1.0,
            morph: 0.0,
            render_geometry: None,
            render_transform: None,
        }
    }

    fn track(property: Property, values: TrackValues) -> DerivedDisplayAnimationTrack {
        DerivedDisplayAnimationTrack {
            property,
            values,
            timing: TrackTiming::new(1.0, 2.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
            transform_geometry_plan: None,
        }
    }

    #[test]
    fn derived_copy_is_absent_before_start_and_matches_endpoints() {
        let occurrence = DerivedDisplayAnimationOccurrence {
            anchor_object_index: 3,
            occurrence_index: 7,
            base: base(GeometryRef::circle(1.0)),
            tracks: vec![
                track(
                    Property::Position,
                    TrackValues::Vec2 {
                        from: Vec2::ZERO,
                        to: Vec2::new(10.0, 0.0),
                    },
                ),
                track(
                    Property::Appearance,
                    TrackValues::Scalar { from: 0.0, to: 1.0 },
                ),
            ],
        };
        let plan = DerivedDisplayAnimationPlan::new(vec![occurrence]).unwrap();
        assert!(plan.evaluate(0.5).unwrap().is_empty());

        let start = plan.evaluate(1.0).unwrap();
        assert_eq!(start.len(), 1);
        assert_eq!(start[0].anchor_object_index(), 3);
        assert_eq!(start[0].occurrence_index(), 7);
        assert_eq!(start[0].state().transform.translation, Vec2::ZERO);
        assert_eq!(start[0].state().appearance, 0.0);

        let middle = plan.evaluate(2.0).unwrap();
        assert_eq!(middle[0].state().transform.translation, Vec2::new(5.0, 0.0));
        assert_eq!(middle[0].state().appearance, 0.5);

        let end = plan.evaluate(3.0).unwrap();
        assert_eq!(end[0].state().transform.translation, Vec2::new(10.0, 0.0));
        assert_eq!(end[0].state().appearance, 1.0);
        assert_eq!(plan.evaluate(9.0).unwrap(), end);
    }

    #[test]
    fn nested_time_map_controls_occurrence_lifetime_and_progress() {
        let mut mapped = track(
            Property::Position,
            TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(8.0, 0.0),
            },
        );
        mapped.timing = TrackTiming::new(0.0, 4.0, RateFunction::Linear);
        mapped.time_map =
            CompositionTimeMap::from_steps(vec![noon_core::CompositionTimeMapStep::new(
                0.5,
                0.5,
                RateFunction::Linear,
            )]);
        let occurrence = DerivedDisplayAnimationOccurrence {
            anchor_object_index: 0,
            occurrence_index: 1,
            base: base(GeometryRef::circle(1.0)),
            tracks: vec![mapped],
        };
        let plan = DerivedDisplayAnimationPlan::new(vec![occurrence]).unwrap();
        assert!(plan.evaluate(1.0).unwrap().is_empty());
        assert_eq!(
            plan.evaluate(3.0).unwrap()[0].state().transform.translation,
            Vec2::new(4.0, 0.0)
        );
        assert_eq!(
            plan.evaluate(4.0).unwrap()[0].state().transform.translation,
            Vec2::new(8.0, 0.0)
        );
    }

    #[test]
    fn prepared_morph_uses_compiled_geometry_plan_without_stable_identity() {
        let source = VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.0))
            .line_to(Vec2::new(1.0, 0.0));
        let target = VectorPath::new()
            .move_to(Vec2::new(0.0, -1.0))
            .line_to(Vec2::new(0.0, 1.0));
        let geometry = GeometryRef::path(source.with_morph_target(target));
        let prepared = Arc::new(geometry.clone());
        let occurrence = DerivedDisplayAnimationOccurrence {
            anchor_object_index: 0,
            occurrence_index: 2,
            base: {
                let mut state = base(geometry.clone());
                state.style = Style {
                    stroke: Some(Color::WHITE),
                    fill: None,
                    ..Style::default()
                };
                state
            },
            tracks: vec![DerivedDisplayAnimationTrack {
                property: Property::Morph,
                values: TrackValues::PreparedMorph {
                    from: 0.0,
                    to: 1.0,
                    geometry,
                    render_transform: None,
                },
                timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
                time_map: CompositionTimeMap::identity(),
                transform_geometry_plan: Some(TransformGeometryPlan::PathPair {
                    geometry: prepared,
                    render_transform: None,
                }),
            }],
        };
        let plan = DerivedDisplayAnimationPlan::new(vec![occurrence]).unwrap();
        let middle = plan.evaluate(1.0).unwrap();
        assert_eq!(middle[0].state().morph, 0.5);
        assert!(middle[0].state().render_geometry.is_some());
    }
}
