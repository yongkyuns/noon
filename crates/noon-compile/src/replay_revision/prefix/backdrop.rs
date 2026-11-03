//! Built-in BackdropBlur history with optional owned visibility.
use super::*;
use noon_core::{EffectDefinition, EffectTrackValue};

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{
        BackdropBlur, EffectRadius, EffectTrackEndBehavior, EffectUpdate, NoiseDissolve, Pixels,
        SemanticNodeId,
    };
    use std::sync::Arc;

    fn effect(attachment: u32, definition: impl Into<EffectDefinition>) -> crate::CompiledEffect {
        crate::CompiledEffect {
            attachment: SemanticNodeId::new(attachment, 1),
            definition: definition.into(),
        }
    }

    #[test]
    fn backdrop_history_requires_owned_visibility_and_typed_radius_tracks() {
        let id = SemanticNodeId::new(3, 1);
        let blur = EffectDefinition::from(
            BackdropBlur::new(EffectUpdate::default().radius(Pixels(4.0))).unwrap(),
        );
        let stack = CompiledEffectStack::new([crate::CompiledEffect {
            attachment: id,
            definition: blur.clone(),
        }])
        .unwrap();
        assert!(local_effect_stack(&stack));
        let noise = SemanticNodeId::new(2, 1);
        let mixed = CompiledEffectStack::new_with_visibility(
            [
                effect(1, BackdropBlur::default()),
                effect(2, NoiseDissolve::default()),
            ],
            Some(noise),
        )
        .unwrap();
        assert!(local_effect_stack(&mixed));
        let unowned_dissolve = CompiledEffectStack::new([
            effect(1, Blur::default()),
            effect(2, NoiseDissolve::default()),
        ])
        .unwrap();
        assert!(!local_effect_stack(&unowned_dissolve));

        let mut object = crate::CompiledObject::new(
            noon_core::ObjectId::new(0),
            noon_core::GeometryRef::circle(0.5),
            noon_core::Transform2D::IDENTITY,
            noon_core::Style::default(),
        );
        object.effects = Some(Arc::new(stack));
        let track = crate::CompiledTrack {
            id: noon_core::TrackId::new(0),
            object_index: 0,
            property: noon_core::Property::EffectRadius,
            values: noon_core::TrackValues::Effect {
                attachment: id,
                from: EffectTrackValue::Radius(EffectRadius::Pixels(4.0)),
                to: EffectTrackValue::Radius(EffectRadius::Pixels(12.0)),
                end_behavior: EffectTrackEndBehavior::Sample,
            },
            timing: noon_core::TrackTiming::new(0.0, 1.0, noon_core::RateFunction::Linear),
            time_map: noon_core::CompositionTimeMap::identity(),
            transform_geometry_plan: None,
            reconciled: false,
        };
        assert!(local_effect_track(&track, Some(&object)));
        let mut wrong_property = track.clone();
        wrong_property.property = noon_core::Property::EffectIntensity;
        wrong_property.values = noon_core::TrackValues::Effect {
            attachment: id,
            from: EffectTrackValue::Intensity(0.0),
            to: EffectTrackValue::Intensity(1.0),
            end_behavior: EffectTrackEndBehavior::Sample,
        };
        assert!(!local_effect_track(&wrong_property, Some(&object)));
        let mut wrong_attachment = track;
        if let noon_core::TrackValues::Effect { attachment, .. } = &mut wrong_attachment.values {
            *attachment = SemanticNodeId::new(4, 1);
        }
        assert!(!local_effect_track(&wrong_attachment, Some(&object)));
    }
}
