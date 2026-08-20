use serde::{Deserialize, Serialize};

use crate::{ObjectId, ObjectSnapshot, SceneDefinition, TrackId, Vec2};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Easing {
    #[default]
    Linear,
    EaseInOutCubic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Property {
    Transform,
    Position,
    Rotation,
    Opacity,
    Reveal,
    Morph,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueKind {
    Scalar,
    Vec2,
    Object,
}

impl Property {
    pub const fn value_kind(self) -> ValueKind {
        match self {
            Self::Transform => ValueKind::Object,
            Self::Position => ValueKind::Vec2,
            Self::Rotation | Self::Opacity | Self::Reveal | Self::Morph => ValueKind::Scalar,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackValues {
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
        if !timing.start_time.is_finite() {
            return Err(TimelineError::InvalidStartTime(timing.start_time));
        }
        if !timing.duration.is_finite() || timing.duration <= 0.0 {
            return Err(TimelineError::InvalidDuration(timing.duration));
        }

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
