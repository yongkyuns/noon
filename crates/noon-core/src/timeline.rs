use serde::{Deserialize, Serialize};

use crate::{
    patch::{validate_geometry, validate_style, validate_transform},
    CompositionTimeMap, CompositionTimeMapError, ObjectId, ObjectSnapshot, ObjectStateField,
    PatchError, SceneDefinition, TrackId, Vec2,
};

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
            Self::Rotation | Self::Opacity | Self::Appearance | Self::Reveal | Self::Morph => {
                ValueKind::Scalar
            }
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
    Object {
        from: ObjectSnapshot,
        to: ObjectSnapshot,
    },
}

impl TrackValues {
    pub const fn value_kind(&self) -> ValueKind {
        match self {
            Self::Bool { .. } => ValueKind::Bool,
            Self::Scalar { .. } => ValueKind::Scalar,
            Self::Vec2 { .. } => ValueKind::Vec2,
            Self::Color { .. } => ValueKind::Color,
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
    snapshot: &ObjectSnapshot,
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
    error: PatchError,
) -> TimelineError {
    let PatchError::InvalidObjectState { field, .. } = error else {
        unreachable!("object-state validator returned a non-object validation error")
    };
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
    UnknownObject(ObjectId),
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
    InvalidScalarValues {
        property: Property,
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
    TrackIdExhausted,
}

impl std::fmt::Display for TimelineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownObject(id) => write!(formatter, "unknown object id {}", id.get()),
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
            Self::InvalidScalarValues { property, from, to } => write!(
                formatter,
                "non-finite scalar values for {property:?}: from={from}, to={to}"
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
            Self::TrackIdExhausted => formatter.write_str("Noon track ID space exhausted"),
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
    validate_track_timing(track.property, track.timing)?;
    let expected = track.property.value_kind();
    let actual = track.values.value_kind();
    if expected != actual {
        return Err(TimelineError::ValueTypeMismatch {
            property: track.property,
            expected,
            actual,
        });
    }
    track
        .values
        .validate_numeric_values(track.object, track.property)?;
    if track.timing.is_instant() && !track.time_map.is_identity() {
        return Err(TimelineError::InstantTrackCannotUseTimeMap(track.property));
    }
    track
        .time_map
        .validate()
        .map_err(TimelineError::InvalidCompositionTimeMap)
}

impl SceneDefinition {
    pub fn add_track(
        &mut self,
        object: ObjectId,
        property: Property,
        values: TrackValues,
        timing: TrackTiming,
    ) -> Result<TrackId, TimelineError> {
        self.add_track_with_time_map(
            object,
            property,
            values,
            timing,
            CompositionTimeMap::identity(),
        )
    }

    pub fn add_track_with_time_map(
        &mut self,
        object: ObjectId,
        property: Property,
        values: TrackValues,
        timing: TrackTiming,
        time_map: CompositionTimeMap,
    ) -> Result<TrackId, TimelineError> {
        if self.object(object).is_none() {
            return Err(TimelineError::UnknownObject(object));
        }
        let id = TrackId::new(self.next_track_id);
        let track = TrackDefinition {
            id,
            object,
            property,
            values,
            timing,
            time_map,
        };
        validate_track_definition(&track)?;
        self.next_track_id = self
            .next_track_id
            .checked_add(1)
            .ok_or(TimelineError::TrackIdExhausted)?;
        self.tracks.push(track);
        Ok(id)
    }

    pub fn set_presence_at(
        &mut self,
        object: ObjectId,
        from: bool,
        to: bool,
        time: f64,
    ) -> Result<TrackId, TimelineError> {
        self.add_track(
            object,
            Property::Presence,
            TrackValues::Bool { from, to },
            TrackTiming::instant(time),
        )
    }

    pub fn animate_transform(
        &mut self,
        object: ObjectId,
        from: ObjectSnapshot,
        to: ObjectSnapshot,
        timing: TrackTiming,
    ) -> Result<TrackId, TimelineError> {
        self.add_track(
            object,
            Property::Transform,
            TrackValues::Object { from, to },
            timing,
        )
    }

    pub fn animate_position(
        &mut self,
        object: ObjectId,
        from: Vec2,
        to: Vec2,
        timing: TrackTiming,
    ) -> Result<TrackId, TimelineError> {
        self.add_track(
            object,
            Property::Position,
            TrackValues::Vec2 { from, to },
            timing,
        )
    }

    pub fn animate_scale(
        &mut self,
        object: ObjectId,
        from: Vec2,
        to: Vec2,
        timing: TrackTiming,
    ) -> Result<TrackId, TimelineError> {
        self.add_track(
            object,
            Property::Scale,
            TrackValues::Vec2 { from, to },
            timing,
        )
    }

    pub fn animate_scalar(
        &mut self,
        object: ObjectId,
        property: Property,
        from: f32,
        to: f32,
        timing: TrackTiming,
    ) -> Result<TrackId, TimelineError> {
        self.add_track(object, property, TrackValues::Scalar { from, to }, timing)
    }

    pub fn animate_appearance(
        &mut self,
        object: ObjectId,
        from: f32,
        to: f32,
        timing: TrackTiming,
    ) -> Result<TrackId, TimelineError> {
        self.animate_scalar(object, Property::Appearance, from, to, timing)
    }

    pub fn animate_reveal(
        &mut self,
        object: ObjectId,
        from: f32,
        to: f32,
        timing: TrackTiming,
    ) -> Result<TrackId, TimelineError> {
        self.animate_scalar(object, Property::Reveal, from, to, timing)
    }

    pub fn animate_morph(
        &mut self,
        object: ObjectId,
        from: f32,
        to: f32,
        timing: TrackTiming,
    ) -> Result<TrackId, TimelineError> {
        self.animate_scalar(object, Property::Morph, from, to, timing)
    }

    pub fn tracks(&self) -> &[TrackDefinition] {
        &self.tracks
    }
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
    fn track_ids_and_order_are_deterministic() {
        let mut first = SceneDefinition::new();
        let mut second = SceneDefinition::new();
        let first_object = first.add(GeometryRef::circle(1.0));
        let second_object = second.add(GeometryRef::circle(1.0));
        let first_position = first
            .animate_position(first_object, Vec2::ZERO, Vec2::ONE, timing())
            .expect("valid track");
        let first_opacity = first
            .animate_scalar(first_object, Property::Opacity, 1.0, 0.0, timing())
            .expect("valid track");
        let second_position = second
            .animate_position(second_object, Vec2::ZERO, Vec2::ONE, timing())
            .expect("valid track");
        let second_opacity = second
            .animate_scalar(second_object, Property::Opacity, 1.0, 0.0, timing())
            .expect("valid track");
        assert_eq!(first_position, TrackId::new(0));
        assert_eq!(first_opacity, TrackId::new(1));
        assert_eq!(first_position, second_position);
        assert_eq!(first_opacity, second_opacity);
        assert_eq!(first.tracks(), second.tracks());
    }

    #[test]
    fn composed_track_carries_validated_time_map() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let map = CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
            0.25,
            0.5,
            RateFunction::Smooth,
        )]);
        scene
            .add_track_with_time_map(
                object,
                Property::Position,
                TrackValues::Vec2 {
                    from: Vec2::ZERO,
                    to: Vec2::ONE,
                },
                TrackTiming::new(0.0, 2.0, RateFunction::Linear),
                map.clone(),
            )
            .unwrap();
        assert_eq!(scene.tracks()[0].time_map, map);
    }

    #[test]
    fn presence_is_a_zero_duration_bool_event() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let track = scene
            .set_presence_at(object, false, true, 1.25)
            .expect("valid presence event");
        assert_eq!(track, TrackId::new(0));
        assert_eq!(scene.tracks()[0].property, Property::Presence);
        assert_eq!(
            scene.tracks()[0].values,
            TrackValues::Bool {
                from: false,
                to: true
            }
        );
        assert_eq!(scene.tracks()[0].timing, TrackTiming::instant(1.25));
        assert!(scene.tracks()[0].time_map.is_identity());
    }

    #[test]
    fn presence_requires_zero_duration_and_other_tracks_allow_instant_assignments() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        assert!(matches!(
            scene.add_track(
                object,
                Property::Presence,
                TrackValues::Bool {
                    from: true,
                    to: false
                },
                TrackTiming::new(1.0, 0.5, RateFunction::Linear),
            ),
            Err(TimelineError::InvalidInstantDuration {
                property: Property::Presence,
                ..
            })
        ));
        let track = scene
            .animate_scalar(
                object,
                Property::Opacity,
                1.0,
                0.0,
                TrackTiming::instant(1.0),
            )
            .expect("ordinary properties may be assigned at an exact timestamp");
        assert_eq!(track, TrackId::new(0));
    }

    #[test]
    fn scale_is_a_vec2_timeline_property() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let track = scene
            .animate_scale(object, Vec2::ONE, Vec2::new(2.0, 0.5), timing())
            .expect("valid scale track");
        assert_eq!(track, TrackId::new(0));
        assert_eq!(scene.tracks()[0].property, Property::Scale);
        assert_eq!(
            scene.tracks()[0].values,
            TrackValues::Vec2 {
                from: Vec2::ONE,
                to: Vec2::new(2.0, 0.5)
            }
        );
    }

    #[test]
    fn appearance_is_a_distinct_scalar_timeline_property() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let track = scene
            .animate_appearance(object, 0.0, 1.0, timing())
            .expect("valid appearance track");
        assert_eq!(track, TrackId::new(0));
        assert_eq!(scene.tracks()[0].property, Property::Appearance);
        assert_eq!(
            scene.tracks()[0].values,
            TrackValues::Scalar { from: 0.0, to: 1.0 }
        );
    }

    #[test]
    fn reveal_is_a_scalar_timeline_property() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::path(
            crate::VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::ONE),
        ));
        let track = scene
            .animate_reveal(object, 0.0, 1.0, timing())
            .expect("valid reveal track");
        assert_eq!(track, TrackId::new(0));
        assert_eq!(scene.tracks()[0].property, Property::Reveal);
        assert_eq!(
            scene.tracks()[0].values,
            TrackValues::Scalar { from: 0.0, to: 1.0 }
        );
    }

    #[test]
    fn morph_is_a_distinct_scalar_timeline_property() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::path(
            crate::VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::ONE),
        ));
        scene
            .animate_morph(object, 0.0, 1.0, timing())
            .expect("valid morph track");
        assert_eq!(scene.tracks()[0].property, Property::Morph);
        assert_eq!(
            scene.tracks()[0].values,
            TrackValues::Scalar { from: 0.0, to: 1.0 }
        );
    }

    #[test]
    fn unknown_objects_are_rejected() {
        let mut scene = SceneDefinition::new();
        let error = scene
            .animate_position(ObjectId::new(99), Vec2::ZERO, Vec2::ONE, timing())
            .expect_err("unknown object must fail");
        assert_eq!(error, TimelineError::UnknownObject(ObjectId::new(99)));
    }

    #[test]
    fn invalid_timing_is_rejected() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        for duration in [-1.0, f64::NAN, f64::INFINITY] {
            let error = scene
                .animate_position(
                    object,
                    Vec2::ZERO,
                    Vec2::ONE,
                    TrackTiming::new(0.0, duration, RateFunction::Linear),
                )
                .expect_err("invalid duration must fail");
            assert!(matches!(error, TimelineError::InvalidDuration(_)));
        }
        let error = scene
            .animate_position(
                object,
                Vec2::ZERO,
                Vec2::ONE,
                TrackTiming::new(f64::NAN, 1.0, RateFunction::Linear),
            )
            .expect_err("invalid start time must fail");
        assert!(matches!(error, TimelineError::InvalidStartTime(_)));
    }

    #[test]
    fn non_finite_scalar_values_are_rejected_without_consuming_track_ids() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));

        for (from, to) in [
            (f32::NAN, 1.0),
            (0.0, f32::INFINITY),
            (f32::NEG_INFINITY, 1.0),
        ] {
            assert!(matches!(
                scene.animate_scalar(object, Property::Opacity, from, to, timing()),
                Err(TimelineError::InvalidScalarValues {
                    property: Property::Opacity,
                    ..
                })
            ));
        }
        assert!(scene.tracks().is_empty());
        assert_eq!(
            scene
                .animate_scalar(object, Property::Opacity, 1.0, 0.0, timing())
                .unwrap(),
            TrackId::new(0)
        );
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

    #[test]
    fn non_finite_position_values_are_rejected_without_consuming_track_ids() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));

        for (from, to) in [
            (Vec2::new(f32::NAN, 0.0), Vec2::ONE),
            (Vec2::ZERO, Vec2::new(1.0, f32::INFINITY)),
            (Vec2::new(0.0, f32::NEG_INFINITY), Vec2::ONE),
        ] {
            assert!(matches!(
                scene.animate_position(object, from, to, timing()),
                Err(TimelineError::InvalidVec2Values {
                    property: Property::Position,
                    ..
                })
            ));
        }
        assert!(scene.tracks().is_empty());
        assert_eq!(
            scene
                .animate_position(object, Vec2::ZERO, Vec2::ONE, timing())
                .unwrap(),
            TrackId::new(0)
        );
    }

    #[test]
    fn non_finite_transform_snapshots_are_rejected_without_consuming_track_ids() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let valid = ObjectSnapshot::new(GeometryRef::circle(1.0));

        let mut invalid_geometry = valid.clone();
        invalid_geometry.geometry = GeometryRef::circle(f32::NAN);
        let mut invalid_transform = valid.clone();
        invalid_transform.transform.rotation = f32::INFINITY;
        let mut invalid_style = valid.clone();
        invalid_style.style = Style {
            opacity: f32::NAN,
            ..Style::default()
        };

        for (field, invalid) in [
            (ObjectStateField::Geometry, invalid_geometry),
            (ObjectStateField::Transform, invalid_transform),
            (ObjectStateField::Style, invalid_style),
        ] {
            for (endpoint, from, to) in [
                (TrackValueEndpoint::From, invalid.clone(), valid.clone()),
                (TrackValueEndpoint::To, valid.clone(), invalid.clone()),
            ] {
                assert_eq!(
                    scene
                        .animate_transform(object, from, to, timing())
                        .expect_err("non-finite transform endpoint must fail"),
                    TimelineError::InvalidObjectValue {
                        property: Property::Transform,
                        endpoint,
                        field,
                    }
                );
            }
        }

        assert!(scene.tracks().is_empty());
        assert_eq!(
            scene
                .animate_transform(object, valid.clone(), valid, timing())
                .expect("finite transform track must remain valid"),
            TrackId::new(0)
        );
    }

    #[test]
    fn value_type_mismatches_are_rejected() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let error = scene
            .add_track(
                object,
                Property::Position,
                TrackValues::Scalar { from: 0.0, to: 1.0 },
                timing(),
            )
            .expect_err("position requires Vec2 values");
        assert_eq!(
            error,
            TimelineError::ValueTypeMismatch {
                property: Property::Position,
                expected: ValueKind::Vec2,
                actual: ValueKind::Scalar,
            }
        );
    }
}
