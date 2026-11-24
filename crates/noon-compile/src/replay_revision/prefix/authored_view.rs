use super::*;
use noon_core::{
    Bloom, EffectRadius, EffectTrackEndBehavior, EffectTrackValue, EffectUpdate, GeometryRef,
    Pixels, SemanticNodeId, Style, Transform2D, ViewColorProfile,
};
use std::sync::Arc;

fn camera(profile: Option<ViewColorProfile>, effects: Arc<CompiledEffectStack>) -> CompiledObject {
    let mut object = CompiledObject::new(
        noon_core::ObjectId::new(0),
        GeometryRef::circle(0.5),
        Transform2D::IDENTITY,
        Style::default(),
    );
    object.view_color_profile = profile;
    object.effects = Some(effects);
    object
}

fn bloom_stack() -> Arc<CompiledEffectStack> {
    Arc::new(
        CompiledEffectStack::new([crate::CompiledEffect {
            attachment: SemanticNodeId::new(1, 1),
            definition: Bloom::new(
                EffectUpdate::default()
                    .radius(Pixels(4.0))
                    .threshold(0.15)
                    .intensity(0.0),
            )
            .unwrap()
            .into(),
        }])
        .unwrap(),
    )
}

fn track(property: Property, from: EffectTrackValue, to: EffectTrackValue) -> CompiledTrack {
    CompiledTrack {
        id: noon_core::TrackId::new(0),
        object_index: 0,
        property,
        values: TrackValues::Effect {
            attachment: SemanticNodeId::new(1, 1),
            from,
            to,
            end_behavior: EffectTrackEndBehavior::Sample,
        },
        timing: noon_core::TrackTiming::new(0.0, 1.0, noon_core::RateFunction::Linear),
        time_map: noon_core::CompositionTimeMap::identity(),
        transform_geometry_plan: None,
        reconciled: false,
    }
}

#[test]
fn bloom_history_is_typed_and_limited_to_the_linear_hdr_camera_owner() {
    let stack = bloom_stack();
    let owner = camera(Some(ViewColorProfile::LinearHdr), stack.clone());
    assert!(local_effect_stack_for_object(&owner, &stack));
    assert!(local_effect_track(
        &track(
            Property::EffectRadius,
            EffectTrackValue::Radius(EffectRadius::Pixels(4.0)),
            EffectTrackValue::Radius(EffectRadius::Pixels(8.0)),
        ),
        Some(&owner),
    ));
    assert!(local_effect_track(
        &track(
            Property::EffectThreshold,
            EffectTrackValue::Threshold(0.15),
            EffectTrackValue::Threshold(0.25),
        ),
        Some(&owner),
    ));
    assert!(!local_effect_track(
        &track(
            Property::EffectThreshold,
            EffectTrackValue::Intensity(0.0),
            EffectTrackValue::Intensity(1.0),
        ),
        Some(&owner),
    ));

    let ordinary = camera(None, stack.clone());
    let non_hdr = camera(Some(ViewColorProfile::Compatibility), stack.clone());
    assert!(!local_effect_stack_for_object(&ordinary, &stack));
    assert!(!local_effect_stack_for_object(&non_hdr, &stack));
    assert!(!local_effect_track(
        &track(
            Property::EffectIntensity,
            EffectTrackValue::Intensity(0.0),
            EffectTrackValue::Intensity(1.0),
        ),
        Some(&ordinary),
    ));
    assert!(!local_channel_for_objects(
        Property::EffectThreshold,
        &ordinary,
        &ordinary,
    ));
}
