use serde::{Deserialize, Serialize};

use crate::{ObjectId, ObjectSnapshot, SceneDefinition, TrackId, Vec2};

/// Language-neutral animation rate functions shared by every authoring frontend.
///
/// The Manim-compatible variants reproduce Manim Community's deterministic
/// built-ins without requiring a Python callback during playback. The existing
/// Noon cubic easing remains available as an explicit low-level option while
/// timeline tracks are migrated from [`Easing`] to this semantic type.
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
}

impl RateFunction {
    /// Evaluate a normalized animation progress value.
    ///
    /// Input is clamped to `[0, 1]`, matching the interval behavior of the
    /// corresponding Manim built-ins used here.
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Easing {
    #[default]
    Linear,
    EaseInOutCubic,
}

impl From<Easing> for RateFunction {
    fn from(value: Easing) -> Self {
        match value {
            Easing::Linear => Self::Linear,
            Easing::EaseInOutCubic => Self::EaseInOutCubic,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Property {
    Presence,
    Transform,
    Position,
    Rotation,
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
    Object,
}

impl Property {
    pub const fn value_kind(self) -> ValueKind {
        match self {
            Self::Presence => ValueKind::Bool,
            Self::Transform => ValueKind::Object,
            Self::Position => ValueKind::Vec2,
            Self::Rotation | Self::Opacity | Self::Appearance | Self::Reveal | Self::Morph => {
                ValueKind::Scalar
            }
        }
    }

    pub const fn is_instant(self) -> bool {
        matches!(self, Self::Presence)
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
            Self::Object { .. } => ValueKind::Object,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackTiming {
    pub start_time: f64,
    pub duration: f64,
    pub easing: Easing,
}

impl TrackTiming {
    pub const fn new(start_time: f64, duration: f64, easing: Easing) -> Self {
        Self {
            start_time,
            duration,
            easing,
        }
    }

    pub const fn instant(start_time: f64) -> Self {
        Self::new(start_time, 0.0, Easing::Linear)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackDefinition {
    pub id: TrackId,
    pub object: ObjectId,
    pub property: Property,
    pub values: TrackValues,
    pub timing: TrackTiming,
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
            Self::TrackIdExhausted => formatter.write_str("Noon track ID space exhausted"),
        }
    }
}

impl std::error::Error for TimelineError {}

pub(crate) fn validate_track_timing(
    property: Property,
    timing: TrackTiming,
) -> Result<(), TimelineError> {
    if !timing.start_time.is_finite() {
        return Err(TimelineError::InvalidStartTime(timing.start_time));
    }
    if !timing.duration.is_finite() {
        return Err(TimelineError::InvalidDuration(timing.duration));
    }
    if property.is_instant() {
        if timing.duration != 0.0 {
            return Err(TimelineError::InvalidInstantDuration {
                property,
                duration: timing.duration,
            });
        }
    } else if timing.duration <= 0.0 {
        return Err(TimelineError::InvalidDuration(timing.duration));
    }
    Ok(())
}

impl SceneDefinition {
    pub fn add_track(
        &mut self,
        object: ObjectId,
        property: Property,
        values: TrackValues,
        timing: TrackTiming,
    ) -> Result<TrackId, TimelineError> {
        if self.object(object).is_none() {
            return Err(TimelineError::UnknownObject(object));
        }
        validate_track_timing(property, timing)?;

        let expected = property.value_kind();
        let actual = values.value_kind();
        if expected != actual {
            return Err(TimelineError::ValueTypeMismatch {
                property,
                expected,
                actual,
            });
        }

        let id = TrackId::new(self.next_track_id);
        self.next_track_id = self
            .next_track_id
            .checked_add(1)
            .ok_or(TimelineError::TrackIdExhausted)?;
        self.tracks.push(TrackDefinition {
            id,
            object,
            property,
            values,
            timing,
        });
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
    use crate::GeometryRef;

    fn timing() -> TrackTiming {
        TrackTiming::new(1.0, 2.0, Easing::Linear)
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
    }

    #[test]
    fn rate_functions_clamp_normalized_input() {
        assert_eq!(RateFunction::Linear.evaluate(-1.0), 0.0);
        assert_eq!(RateFunction::Linear.evaluate(2.0), 1.0);
        assert_eq!(RateFunction::Smooth.evaluate(-1.0), 0.0);
        assert_eq!(RateFunction::Smooth.evaluate(2.0), 1.0);
    }

    #[test]
    fn legacy_easing_maps_to_shared_rate_function() {
        assert_eq!(RateFunction::from(Easing::Linear), RateFunction::Linear);
        assert_eq!(
            RateFunction::from(Easing::EaseInOutCubic),
            RateFunction::EaseInOutCubic
        );
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
    }

    #[test]
    fn presence_rejects_nonzero_duration_and_other_tracks_reject_zero_duration() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));

        assert!(matches!(
            scene.add_track(
                object,
                Property::Presence,
                TrackValues::Bool {
                    from: true,
                    to: false,
                },
                TrackTiming::new(1.0, 0.5, Easing::Linear),
            ),
            Err(TimelineError::InvalidInstantDuration {
                property: Property::Presence,
                ..
            })
        ));
        assert!(matches!(
            scene.animate_scalar(
                object,
                Property::Opacity,
                1.0,
                0.0,
                TrackTiming::instant(1.0),
            ),
            Err(TimelineError::InvalidDuration(0.0))
        ));
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

        for duration in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let error = scene
                .animate_position(
                    object,
                    Vec2::ZERO,
                    Vec2::ONE,
                    TrackTiming::new(0.0, duration, Easing::Linear),
                )
                .expect_err("invalid duration must fail");
            assert!(matches!(error, TimelineError::InvalidDuration(_)));
        }

        let error = scene
            .animate_position(
                object,
                Vec2::ZERO,
                Vec2::ONE,
                TrackTiming::new(f64::NAN, 1.0, Easing::Linear),
            )
            .expect_err("invalid start time must fail");
        assert!(matches!(error, TimelineError::InvalidStartTime(_)));
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
