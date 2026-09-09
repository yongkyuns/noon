use serde::{Deserialize, Serialize};

use crate::{
    object_state::{validate_geometry, validate_style, validate_transform},
    CompositionTimeMap, CompositionTimeMapError, ObjectId, ObjectStateError, ObjectStateField,
    TrackId, Vec2,
};
use crate::{GeometryRef, Style, Transform2D};

/// Language-neutral animation rate functions shared by every authoring frontend.
///
/// The Manim-compatible variants reproduce Manim Community's deterministic
/// built-ins without requiring a Python callback during playback. Noon's previous
/// cubic easing remains available as an explicit low-level compatibility option.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateFunction {
    Linear,
    #[default]
    Smooth,
    RushInto,
    RushFrom,
    ThereAndBack,
    EaseInOutCubic,
    /// Hold the source value at normalized progress 0, then switch to the target.
    StepStart,
    /// Hold the source value until normalized progress reaches 1, then switch.
    StepEnd,
}

impl RateFunction {
    /// Evaluate a normalized animation progress value.
    pub fn evaluate(self, progress: f32) -> f32 {
        let progress = progress.clamp(0.0, 1.0);
        match self {
            Self::Linear => progress,
            Self::Smooth => manim_smooth(progress),
            Self::RushInto => 2.0 * manim_smooth(progress / 2.0),
            Self::RushFrom => 2.0 * manim_smooth(progress / 2.0 + 0.5) - 1.0,
            Self::ThereAndBack => {
                let mirrored = if progress < 0.5 {
                    2.0 * progress
                } else {
                    2.0 * (1.0 - progress)
                };
                manim_smooth(mirrored)
            }
            Self::EaseInOutCubic => {
                if progress < 0.5 {
                    4.0 * progress * progress * progress
                } else {
                    1.0 - (-2.0 * progress + 2.0).powi(3) / 2.0
                }
            }
            Self::StepStart => {
                if progress <= 0.0 {
                    0.0
                } else {
                    1.0
                }
            }
            Self::StepEnd => {
                if progress < 1.0 {
                    0.0
                } else {
                    1.0
                }
            }
        }
    }
}

fn manim_smooth(progress: f32) -> f32 {
    const INFLECTION: f32 = 10.0;
    let error = sigmoid(-INFLECTION / 2.0);
    ((sigmoid(INFLECTION * (progress - 0.5)) - error) / (1.0 - 2.0 * error)).clamp(0.0, 1.0)
}

fn sigmoid(value: f32) -> f32 {
    1.0 / (1.0 + (-value).exp())
}

pub type Easing = RateFunction;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Property {
    Presence,
    Transform,
    Position,
    Rotation,
    Scale,
    Fill,
    Stroke,
    StrokeWidth,
    Opacity,
    Appearance,
    Reveal,
    Morph,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueKind {
    Bool,
    Scalar,
    Vec2,
    Color,
    Object,
}

impl Property {
    pub const fn value_kind(self) -> ValueKind {
        match self {
            Self::Presence => ValueKind::Bool,
            Self::Transform => ValueKind::Object,
            Self::Fill | Self::Stroke => ValueKind::Color,
            Self::Position | Self::Scale => ValueKind::Vec2,
            Self::Rotation
            | Self::StrokeWidth
            | Self::Opacity
            | Self::Appearance
            | Self::Reveal
            | Self::Morph => ValueKind::Scalar,
        }
    }

    pub const fn is_instant(self) -> bool {
        matches!(self, Self::Presence)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackValueEndpoint {
    From,
    To,
}

/// An exact renderer-independent endpoint for an execution transform track.
///
/// Semantic lowering resolves authored object references into this value before
/// runtime evaluation. It intentionally carries no authored object identity.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransformTrackEndpoint {
    pub geometry: GeometryRef,
    pub transform: Transform2D,
    pub style: Style,
}

impl TransformTrackEndpoint {
    pub fn new(geometry: GeometryRef) -> Self {
        Self {
            geometry,
            transform: Transform2D::default(),
            style: Style::default(),
        }
    }
}

impl std::fmt::Display for TrackValueEndpoint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::From => "from",
            Self::To => "to",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackValues {
    Bool {
        from: bool,
        to: bool,
    },
    Scalar {
        from: f32,
        to: f32,
    },
    Vec2 {
        from: Vec2,
        to: Vec2,
    },
    Color {
        from: Option<crate::Color>,
        to: Option<crate::Color>,
    },
    /// Renderer-independent geometry prepared by lowering for a scalar morph channel.
    ///
    /// This is execution data, not an authored object endpoint: semantic transform,
    /// style, identity, and completion remain in their owning channels and store.
    PreparedMorph {
        from: f32,
        to: f32,
        geometry: crate::GeometryRef,
        render_transform: Option<crate::Transform2D>,
    },
    Object {
        from: TransformTrackEndpoint,
        to: TransformTrackEndpoint,
    },
}

impl TrackValues {
    pub const fn value_kind(&self) -> ValueKind {
        match self {
            Self::Bool { .. } => ValueKind::Bool,
            Self::Scalar { .. } => ValueKind::Scalar,
            Self::Vec2 { .. } => ValueKind::Vec2,
            Self::Color { .. } => ValueKind::Color,
            Self::PreparedMorph { .. } => ValueKind::Scalar,
            Self::Object { .. } => ValueKind::Object,
        }
    }

    fn validate_numeric_values(
        &self,
        object: ObjectId,
        property: Property,
    ) -> Result<(), TimelineError> {
        match self {
            Self::Scalar { from, to } if !from.is_finite() || !to.is_finite() => {
                Err(TimelineError::InvalidScalarValues {
                    property,
                    from: *from,
                    to: *to,
                })
            }
            Self::Scalar { from, to }
                if property == Property::StrokeWidth && (*from < 0.0 || *to < 0.0) =>
            {
                Err(TimelineError::InvalidStrokeWidthValues {
                    from: *from,
                    to: *to,
                })
            }
            Self::Vec2 { from, to }
                if !from.x.is_finite()
                    || !from.y.is_finite()
                    || !to.x.is_finite()
                    || !to.y.is_finite() =>
            {
                Err(TimelineError::InvalidVec2Values {
                    property,
                    from: *from,
                    to: *to,
                })
            }
            Self::Color { from, to } => {
                for (endpoint, color) in [
                    (TrackValueEndpoint::From, from),
                    (TrackValueEndpoint::To, to),
                ] {
                    if color.as_ref().is_some_and(|color| {
                        !color.red.is_finite()
                            || !color.green.is_finite()
                            || !color.blue.is_finite()
                            || !color.alpha.is_finite()
                    }) {
                        return Err(TimelineError::InvalidColorValue { property, endpoint });
                    }
                }
                Ok(())
            }
            Self::PreparedMorph {
                from,
                to,
                geometry,
                render_transform,
            } => {
                if !from.is_finite() || !to.is_finite() {
                    return Err(TimelineError::InvalidScalarValues {
                        property,
                        from: *from,
                        to: *to,
                    });
                }
                validate_geometry(object, geometry).map_err(|error| {
                    invalid_object_track_value(property, TrackValueEndpoint::From, error)
                })?;
                if let Some(transform) = render_transform {
                    validate_transform(object, *transform).map_err(|error| {
                        invalid_object_track_value(property, TrackValueEndpoint::From, error)
                    })?;
                }
                Ok(())
            }
            Self::Object { from, to } => {
                validate_object_track_value(object, property, TrackValueEndpoint::From, from)?;
                validate_object_track_value(object, property, TrackValueEndpoint::To, to)
            }
            _ => Ok(()),
        }
    }
}

fn validate_object_track_value(
    object: ObjectId,
    property: Property,
    endpoint: TrackValueEndpoint,
    snapshot: &TransformTrackEndpoint,
) -> Result<(), TimelineError> {
    validate_geometry(object, &snapshot.geometry)
        .map_err(|error| invalid_object_track_value(property, endpoint, error))?;
    validate_transform(object, snapshot.transform)
        .map_err(|error| invalid_object_track_value(property, endpoint, error))?;
    validate_style(object, snapshot.style)
        .map_err(|error| invalid_object_track_value(property, endpoint, error))
}

fn invalid_object_track_value(
    property: Property,
    endpoint: TrackValueEndpoint,
    error: ObjectStateError,
) -> TimelineError {
    let ObjectStateError { field, .. } = error;
    TimelineError::InvalidObjectValue {
        property,
        endpoint,
        field,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackTiming {
    pub start_time: f64,
    pub duration: f64,
    pub easing: RateFunction,
}

impl TrackTiming {
    pub const fn new(start_time: f64, duration: f64, easing: RateFunction) -> Self {
        Self {
            start_time,
            duration,
            easing,
        }
    }

    pub const fn instant(start_time: f64) -> Self {
        Self::new(start_time, 0.0, RateFunction::Linear)
    }

    pub const fn is_instant(self) -> bool {
        self.duration == 0.0
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackDefinition {
    pub id: TrackId,
    pub object: ObjectId,
    pub property: Property,
    pub values: TrackValues,
    pub timing: TrackTiming,
    #[serde(default, skip_serializing_if = "CompositionTimeMap::is_identity")]
    pub time_map: CompositionTimeMap,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TimelineError {
    InvalidStartTime(f64),
    InvalidDuration(f64),
    InvalidInstantDuration {
        property: Property,
        duration: f64,
    },
    ValueTypeMismatch {
        property: Property,
        expected: ValueKind,
        actual: ValueKind,
    },
    PreparedMorphPropertyMismatch(Property),
    InvalidScalarValues {
        property: Property,
        from: f32,
        to: f32,
    },
    InvalidStrokeWidthValues {
        from: f32,
        to: f32,
    },
    InvalidVec2Values {
        property: Property,
        from: Vec2,
        to: Vec2,
    },
    InvalidColorValue {
        property: Property,
        endpoint: TrackValueEndpoint,
    },
    InvalidObjectValue {
        property: Property,
        endpoint: TrackValueEndpoint,
        field: ObjectStateField,
    },
    InvalidCompositionTimeMap(CompositionTimeMapError),
    InstantTrackCannotUseTimeMap(Property),
}

impl std::fmt::Display for TimelineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidStartTime(value) => write!(formatter, "invalid start time {value}"),
            Self::InvalidDuration(value) => write!(formatter, "invalid duration {value}"),
            Self::InvalidInstantDuration { property, duration } => write!(
                formatter,
                "instant {property:?} track requires zero duration, got {duration}"
            ),
            Self::ValueTypeMismatch {
                property,
                expected,
                actual,
            } => write!(
                formatter,
                "value type mismatch for {property:?}: expected {expected:?}, got {actual:?}"
            ),
            Self::PreparedMorphPropertyMismatch(property) => write!(
                formatter,
                "prepared morph execution data cannot drive {property:?}"
            ),
            Self::InvalidScalarValues { property, from, to } => write!(
                formatter,
                "non-finite scalar values for {property:?}: from={from}, to={to}"
            ),
            Self::InvalidStrokeWidthValues { from, to } => write!(
                formatter,
                "stroke width values must be non-negative: from={from}, to={to}"
            ),
            Self::InvalidVec2Values { property, from, to } => write!(
                formatter,
                "non-finite vector values for {property:?}: from=({}, {}), to=({}, {})",
                from.x, from.y, to.x, to.y
            ),
            Self::InvalidColorValue { property, endpoint } => write!(
                formatter,
                "non-finite color in {endpoint} value for {property:?}"
            ),
            Self::InvalidObjectValue {
                property,
                endpoint,
                field,
            } => write!(
                formatter,
                "non-finite {field} state in {endpoint} object value for {property:?}"
            ),
            Self::InvalidCompositionTimeMap(error) => error.fmt(formatter),
            Self::InstantTrackCannotUseTimeMap(property) => write!(
                formatter,
                "instant {property:?} tracks cannot carry a composition time map"
            ),
        }
    }
}

impl std::error::Error for TimelineError {}

pub(crate) fn validate_track_timing(
    property: Property,
    timing: TrackTiming,
) -> Result<(), TimelineError> {
    validate_track_time_fields(timing)?;
    if property.is_instant() {
        if timing.duration != 0.0 {
            return Err(TimelineError::InvalidInstantDuration {
                property,
                duration: timing.duration,
            });
        }
    } else if timing.duration < 0.0 {
        return Err(TimelineError::InvalidDuration(timing.duration));
    }
    Ok(())
}

pub(crate) fn validate_continuous_track_timing(timing: TrackTiming) -> Result<(), TimelineError> {
    validate_track_time_fields(timing)?;
    if timing.duration < 0.0 {
        return Err(TimelineError::InvalidDuration(timing.duration));
    }
    Ok(())
}

fn validate_track_time_fields(timing: TrackTiming) -> Result<(), TimelineError> {
    if !timing.start_time.is_finite() {
        return Err(TimelineError::InvalidStartTime(timing.start_time));
    }
    if !timing.duration.is_finite() {
        return Err(TimelineError::InvalidDuration(timing.duration));
    }
    Ok(())
}

pub fn validate_track_definition(track: &TrackDefinition) -> Result<(), TimelineError> {
    if track.property.is_instant() && !track.time_map.is_identity() {
        validate_continuous_track_timing(track.timing)?;
    } else {
        validate_track_timing(track.property, track.timing)?;
    }
    let expected = track.property.value_kind();
    let actual = track.values.value_kind();
    if expected != actual {
        return Err(TimelineError::ValueTypeMismatch {
            property: track.property,
            expected,
            actual,
        });
    }
    if matches!(&track.values, TrackValues::PreparedMorph { .. })
        && track.property != Property::Morph
    {
        return Err(TimelineError::PreparedMorphPropertyMismatch(track.property));
    }
    track
        .values
        .validate_numeric_values(track.object, track.property)?;
    if track.timing.is_instant() && !track.property.is_instant() && !track.time_map.is_identity() {
        return Err(TimelineError::InstantTrackCannotUseTimeMap(track.property));
    }
    track
        .time_map
        .validate()
        .map_err(TimelineError::InvalidCompositionTimeMap)?;
    if track.property.is_instant() && !track.time_map.is_identity() {
        track
            .time_map
            .monotone_event_alpha()
            .map_err(TimelineError::InvalidCompositionTimeMap)?;
    }
    Ok(())
}

/// Resolve authored timing into the timing consumed by the execution scheduler.
///
/// A mapped discrete track carries its composition root interval during semantic
/// lowering. It is compiled to one instant boundary so playback and direct seek
/// use the same event scheduler and cannot replay a reversing time-map event.
pub fn resolve_track_timing(track: &TrackDefinition) -> Result<TrackTiming, TimelineError> {
    validate_track_definition(track)?;
    if !track.property.is_instant() || track.time_map.is_identity() {
        return Ok(track.timing);
    }
    let alpha = track
        .time_map
        .monotone_event_alpha()
        .map_err(TimelineError::InvalidCompositionTimeMap)?;
    Ok(TrackTiming::instant(
        track.timing.start_time + track.timing.duration * alpha,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompositionTimeMapStep, GeometryRef, Style};

    fn timing() -> TrackTiming {
        TrackTiming::new(1.0, 2.0, RateFunction::Linear)
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-6,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn manim_rate_functions_have_exact_endpoints_and_reference_values() {
        assert_eq!(RateFunction::Linear.evaluate(0.0), 0.0);
        assert_eq!(RateFunction::Linear.evaluate(1.0), 1.0);
        assert_eq!(RateFunction::Smooth.evaluate(0.0), 0.0);
        assert_eq!(RateFunction::Smooth.evaluate(1.0), 1.0);
        assert_close(RateFunction::Smooth.evaluate(0.25), 0.07010372);
        assert_close(RateFunction::Smooth.evaluate(0.5), 0.5);
        assert_close(RateFunction::Smooth.evaluate(0.75), 0.9298963);
        assert_close(
            RateFunction::RushInto.evaluate(0.5),
            2.0 * RateFunction::Smooth.evaluate(0.25),
        );
        assert_close(
            RateFunction::RushFrom.evaluate(0.5),
            2.0 * RateFunction::Smooth.evaluate(0.75) - 1.0,
        );
        assert_eq!(RateFunction::ThereAndBack.evaluate(0.0), 0.0);
        assert_eq!(RateFunction::ThereAndBack.evaluate(0.5), 1.0);
        assert_eq!(RateFunction::ThereAndBack.evaluate(1.0), 0.0);
        assert_eq!(RateFunction::StepStart.evaluate(0.0), 0.0);
        assert_eq!(RateFunction::StepStart.evaluate(f32::EPSILON), 1.0);
        assert_eq!(RateFunction::StepStart.evaluate(1.0), 1.0);
        assert_eq!(RateFunction::StepEnd.evaluate(0.0), 0.0);
        assert_eq!(RateFunction::StepEnd.evaluate(1.0 - f32::EPSILON), 0.0);
        assert_eq!(RateFunction::StepEnd.evaluate(1.0), 1.0);
    }

    #[test]
    fn rate_functions_clamp_normalized_input() {
        assert_eq!(RateFunction::Linear.evaluate(-1.0), 0.0);
        assert_eq!(RateFunction::Linear.evaluate(2.0), 1.0);
        assert_eq!(RateFunction::Smooth.evaluate(-1.0), 0.0);
        assert_eq!(RateFunction::Smooth.evaluate(2.0), 1.0);
    }

    #[test]
    fn legacy_easing_name_is_a_source_compatible_alias() {
        assert_eq!(Easing::Linear, RateFunction::Linear);
        assert_eq!(Easing::EaseInOutCubic, RateFunction::EaseInOutCubic);
    }

    #[test]
    fn mapped_presence_resolves_to_its_nested_monotone_boundary() {
        let track = TrackDefinition {
            id: TrackId::new(0),
            object: ObjectId::new(1),
            property: Property::Presence,
            values: TrackValues::Bool {
                from: false,
                to: true,
            },
            timing: TrackTiming::new(4.0, 6.0, RateFunction::Linear),
            time_map: CompositionTimeMap::from_steps(vec![
                CompositionTimeMapStep::new(0.25, 0.5, RateFunction::Linear),
                CompositionTimeMapStep::new(0.5, 0.5, RateFunction::Linear),
            ]),
        };
        assert_eq!(resolve_track_timing(&track), Ok(TrackTiming::instant(7.0)));

        let mut zero_root = track.clone();
        zero_root.timing = TrackTiming::new(2.0, 0.0, RateFunction::Linear);
        assert_eq!(
            resolve_track_timing(&zero_root),
            Ok(TrackTiming::instant(2.0))
        );
    }

    #[test]
    fn mapped_presence_rejects_reversing_rate_during_validation() {
        let track = TrackDefinition {
            id: TrackId::new(0),
            object: ObjectId::new(1),
            property: Property::Presence,
            values: TrackValues::Bool {
                from: false,
                to: true,
            },
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            time_map: CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
                0.0,
                1.0,
                RateFunction::ThereAndBack,
            )]),
        };
        let error = validate_track_definition(&track)
            .expect_err("a reversing parent has no single presence boundary");
        assert!(matches!(
            error,
            TimelineError::InvalidCompositionTimeMap(
                CompositionTimeMapError::UnsupportedDiscreteRate { .. }
            )
        ));
    }

    #[test]
    fn stroke_width_is_a_non_negative_scalar_timeline_property() {
        let valid = TrackDefinition {
            id: TrackId::new(0),
            object: ObjectId::new(4),
            property: Property::StrokeWidth,
            values: TrackValues::Scalar { from: 1.0, to: 3.0 },
            timing: timing(),
            time_map: CompositionTimeMap::identity(),
        };
        validate_track_definition(&valid).expect("valid stroke-width track");

        let mut invalid = valid.clone();
        invalid.values = TrackValues::Scalar {
            from: 3.0,
            to: -1.0,
        };
        let error = validate_track_definition(&invalid)
            .expect_err("negative stroke width must fail typed-track validation");
        assert_eq!(
            error,
            TimelineError::InvalidStrokeWidthValues {
                from: 3.0,
                to: -1.0,
            }
        );
        validate_track_definition(&valid).expect("rejection does not mutate the typed track");
    }

    #[test]
    fn non_finite_fill_color_is_rejected_by_track_validation() {
        let invalid = crate::Color {
            red: f32::NAN,
            ..crate::Color::RED
        };
        let invalid_track = TrackDefinition {
            id: TrackId::new(0),
            object: ObjectId::new(1),
            property: Property::Fill,
            values: TrackValues::Color {
                from: Some(crate::Color::BLUE),
                to: Some(invalid),
            },
            timing: timing(),
            time_map: CompositionTimeMap::identity(),
        };
        assert_eq!(
            validate_track_definition(&invalid_track),
            Err(TimelineError::InvalidColorValue {
                property: Property::Fill,
                endpoint: TrackValueEndpoint::To,
            })
        );

        let valid_track = TrackDefinition {
            values: TrackValues::Color {
                from: None,
                to: Some(crate::Color::RED),
            },
            ..invalid_track
        };
        assert!(validate_track_definition(&valid_track).is_ok());
    }

    fn track(property: Property, values: TrackValues, timing: TrackTiming) -> TrackDefinition {
        TrackDefinition {
            id: TrackId::new(7),
            object: ObjectId::new(3),
            property,
            values,
            timing,
            time_map: CompositionTimeMap::identity(),
        }
    }

    #[test]
    fn typed_properties_accept_matching_values_and_reject_wrong_kinds() {
        for property in [Property::Position, Property::Scale] {
            let values = TrackValues::Vec2 {
                from: Vec2::ONE,
                to: Vec2::new(2.0, 0.5),
            };
            validate_track_definition(&track(property, values, timing())).unwrap();
            assert_eq!(
                validate_track_definition(&track(
                    property,
                    TrackValues::Scalar { from: 0.0, to: 1.0 },
                    timing()
                )),
                Err(TimelineError::ValueTypeMismatch {
                    property,
                    expected: ValueKind::Vec2,
                    actual: ValueKind::Scalar
                })
            );
        }
        for property in [
            Property::Opacity,
            Property::Rotation,
            Property::Appearance,
            Property::Reveal,
            Property::Morph,
            Property::StrokeWidth,
        ] {
            let values = TrackValues::Scalar { from: 0.0, to: 1.0 };
            validate_track_definition(&track(property, values.clone(), timing())).unwrap();
            validate_track_definition(&track(property, values, TrackTiming::instant(1.0))).unwrap();
            assert_eq!(
                validate_track_definition(&track(
                    property,
                    TrackValues::Vec2 {
                        from: Vec2::ZERO,
                        to: Vec2::ONE
                    },
                    timing()
                )),
                Err(TimelineError::ValueTypeMismatch {
                    property,
                    expected: ValueKind::Scalar,
                    actual: ValueKind::Vec2
                })
            );
        }
    }

    #[test]
    fn presence_requires_an_instant_event_without_a_composition_map() {
        let mut event = track(
            Property::Presence,
            TrackValues::Bool {
                from: false,
                to: true,
            },
            TrackTiming::instant(1.25),
        );
        assert_eq!(resolve_track_timing(&event), Ok(TrackTiming::instant(1.25)));
        event.timing.duration = 0.5;
        assert_eq!(
            validate_track_definition(&event),
            Err(TimelineError::InvalidInstantDuration {
                property: Property::Presence,
                duration: 0.5,
            })
        );
    }

    #[test]
    fn continuous_composition_map_is_validated_without_changing_track_identity() {
        let mut value = track(
            Property::Position,
            TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::ONE,
            },
            timing(),
        );
        value.time_map = CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
            0.25,
            0.5,
            RateFunction::Smooth,
        )]);
        let before = value.clone();
        assert_eq!(resolve_track_timing(&value), Ok(timing()));
        assert_eq!(value, before);
        value.timing = TrackTiming::instant(1.0);
        assert_eq!(
            validate_track_definition(&value),
            Err(TimelineError::InstantTrackCannotUseTimeMap(
                Property::Position
            ))
        );
    }

    #[test]
    fn invalid_timing_is_rejected() {
        let mut value = track(
            Property::Position,
            TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::ONE,
            },
            timing(),
        );
        for duration in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            value.timing.duration = duration;
            assert!(matches!(
                validate_track_definition(&value),
                Err(TimelineError::InvalidDuration(_))
            ));
        }
        value.timing.duration = 1.0;
        for start in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            value.timing.start_time = start;
            assert!(matches!(
                validate_track_definition(&value),
                Err(TimelineError::InvalidStartTime(_))
            ));
        }
    }

    #[test]
    fn non_finite_scalar_and_vector_endpoints_are_rejected() {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for (from, to) in [(invalid, 1.0), (0.0, invalid)] {
                assert!(matches!(
                    validate_track_definition(&track(
                        Property::Opacity,
                        TrackValues::Scalar { from, to },
                        timing()
                    )),
                    Err(TimelineError::InvalidScalarValues {
                        property: Property::Opacity,
                        ..
                    })
                ));
            }
            for invalid in [Vec2::new(invalid, 0.0), Vec2::new(0.0, invalid)] {
                for (from, to) in [(invalid, Vec2::ONE), (Vec2::ZERO, invalid)] {
                    assert!(matches!(
                        validate_track_definition(&track(
                            Property::Position,
                            TrackValues::Vec2 { from, to },
                            timing()
                        )),
                        Err(TimelineError::InvalidVec2Values {
                            property: Property::Position,
                            ..
                        })
                    ));
                }
            }
        }
    }

    #[test]
    fn transform_endpoints_validate_geometry_transform_and_style_on_both_sides() {
        let valid = TransformTrackEndpoint {
            geometry: GeometryRef::circle(1.0),
            transform: Transform2D::IDENTITY,
            style: Style::default(),
        };
        let mut bad_geometry = valid.clone();
        bad_geometry.geometry = GeometryRef::circle(f32::NAN);
        let mut bad_transform = valid.clone();
        bad_transform.transform.rotation = f32::INFINITY;
        let mut bad_style = valid.clone();
        bad_style.style.opacity = f32::NAN;
        for (field, invalid) in [
            (ObjectStateField::Geometry, bad_geometry),
            (ObjectStateField::Transform, bad_transform),
            (ObjectStateField::Style, bad_style),
        ] {
            for (endpoint, from, to) in [
                (TrackValueEndpoint::From, invalid.clone(), valid.clone()),
                (TrackValueEndpoint::To, valid.clone(), invalid),
            ] {
                assert_eq!(
                    validate_track_definition(&track(
                        Property::Transform,
                        TrackValues::Object { from, to },
                        timing()
                    )),
                    Err(TimelineError::InvalidObjectValue {
                        property: Property::Transform,
                        endpoint,
                        field
                    })
                );
            }
        }
        validate_track_definition(&track(
            Property::Transform,
            TrackValues::Object {
                from: valid.clone(),
                to: valid,
            },
            timing(),
        ))
        .unwrap();
    }
}
