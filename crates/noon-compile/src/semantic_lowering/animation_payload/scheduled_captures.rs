//! Candidate-local activation captures for non-overlapping prepared channels.
use std::collections::HashMap;

use noon_core::{
    continuous_time_map_interval, ObjectId, Property, SemanticObjectContent,
    SemanticTransactionNodeRef, TrackValues,
};

use super::super::PreparedSemanticScheduledAnimationLeaf;
use super::affine::{driver_key, EffectiveAnimationProperties, SemanticAnimationCompletion};
use super::prepared_composition::PreparedSemanticAnimationTrack;

#[derive(Default)]
pub(super) struct ScheduledCaptures {
    base: HashMap<ObjectId, EffectiveAnimationProperties>,
    completed: HashMap<(u64, u8), (Property, TrackValues, bool)>,
    latest_tracks: HashMap<(u64, u8), usize>,
    recorded_tracks: usize,
}

impl ScheduledCaptures {
    pub fn get(&self, object: ObjectId) -> Option<EffectiveAnimationProperties> {
        let mut value = self.base.get(&object)?.clone();
        for slot in 0..=driver_key(object, Property::GlowIntensity).1 {
            let Some((property, values, at_end)) = self.completed.get(&(object.get(), slot)) else {
                continue;
            };
            apply_effective_track_endpoint(&mut value, *property, values, *at_end)?;
        }
        Some(value)
    }

    pub fn insert(&mut self, object: ObjectId, value: EffectiveAnimationProperties) {
        self.base.insert(object, value);
    }

    pub fn begin_leaf(
        &mut self,
        leaf: &PreparedSemanticScheduledAnimationLeaf,
        driven: &mut HashMap<(u64, u8), SemanticTransactionNodeRef>,
        tracks: &[PreparedSemanticAnimationTrack],
        intervals: &HashMap<SemanticTransactionNodeRef, (f64, f64)>,
        allow_sequential_capture: bool,
    ) {
        for (index, track) in tracks.iter().enumerate().skip(self.recorded_tracks) {
            self.latest_tracks
                .insert(driver_key(track.execution_object_id, track.property), index);
        }
        self.recorded_tracks = tracks.len();
        if !allow_sequential_capture {
            return;
        }
        let Some((start, _)) = intervals.get(&leaf.animation) else {
            return;
        };
        let object = leaf.execution_object_id;
        // Existing ordinary prepared lowering does not yet substitute effective
        // content into a later leaf's semantic source object. Keep its established
        // fail-closed guard for both geometry-changing channels. The separate
        // activation-time matching helper below can consume exact Morph completion
        // content without widening ordinary TransformTo semantics.
        if self
            .latest_tracks
            .contains_key(&driver_key(object, Property::Morph))
            || self
                .latest_tracks
                .contains_key(&driver_key(object, Property::Transform))
        {
            return;
        }
        for slot in 0..=driver_key(object, Property::GlowIntensity).1 {
            let key = (object.get(), slot);
            let Some(owner) = driven.get(&key).copied() else {
                continue;
            };
            let Some((_, end)) = intervals.get(&owner) else {
                continue;
            };
            let end = self
                .latest_tracks
                .get(&key)
                .map(|index| &tracks[*index])
                .filter(|track| track.property == Property::ZIndex)
                .map_or(*end, |track| track.timing.start_time);
            // Adjacent normalized intervals can differ by a few rounding bits
            // after mapping back to root seconds (e.g. 2.5 * (0.2 + 0.4)
            // versus 2.5 * 0.6). Do not turn that arithmetic noise into a
            // simultaneous-driver conflict; meaningful overlaps still reject.
            let rounding = 4.0 * f64::EPSILON * end.abs().max(start.abs()).max(f64::MIN_POSITIVE);
            if end - *start > rounding {
                continue;
            }
            if let Some(index) = self.latest_tracks.get(&key) {
                let track = &tracks[*index];
                if track.animation == owner {
                    let alpha = track.values.terminal_progress(track.timing);
                    if alpha != 0.0 && alpha != 1.0 {
                        continue;
                    }
                    self.completed
                        .insert(key, (track.property, track.values.clone(), alpha == 1.0));
                }
            }
            driven.remove(&key);
        }
    }
}

/// Derive the exact effective affine/style endpoint visible at `activation_start`
/// from already-lowered non-overlapping prepared tracks.
///
/// This is intentionally endpoint-only. A source with an active overlapping driver,
/// a non-endpoint easing result, or an opaque `Property::Transform` fails closed.
pub(super) fn completed_effective_properties_before(
    mut value: EffectiveAnimationProperties,
    object: ObjectId,
    activation_start: f64,
    tracks: &[PreparedSemanticAnimationTrack],
) -> Option<EffectiveAnimationProperties> {
    for track in tracks
        .iter()
        .filter(|track| track.execution_object_id == object)
    {
        let (start, end) = continuous_time_map_interval(track.timing, &track.time_map).ok()?;
        match track_relation_to_activation(start, end, activation_start) {
            TrackActivationRelation::After => continue,
            TrackActivationRelation::Overlapping => return None,
            TrackActivationRelation::Completed => {}
        }
        let alpha = track.values.terminal_progress(track.timing);
        if alpha != 0.0 && alpha != 1.0 {
            return None;
        }
        if track.property == Property::Transform {
            return None;
        }
        apply_effective_track_endpoint(&mut value, track.property, &track.values, alpha == 1.0)?;
    }
    Some(value)
}

/// Derive the semantic content endpoint visible at `activation_start`.
///
/// Prepared Morph channels retain the exact authored target content in their
/// completion contract. That lets a later matching-shape activation observe a prior
/// Succession child's completed geometry without sampling renderer state or decoding
/// a temporary morph resource. Opaque combined Transform tracks remain fail-closed.
pub(super) fn completed_content_before(
    mut content: SemanticObjectContent,
    object: ObjectId,
    activation_start: f64,
    tracks: &[PreparedSemanticAnimationTrack],
) -> Option<SemanticObjectContent> {
    for track in tracks.iter().filter(|track| {
        track.execution_object_id == object
            && matches!(track.property, Property::Morph | Property::Transform)
    }) {
        let (start, end) = continuous_time_map_interval(track.timing, &track.time_map).ok()?;
        match track_relation_to_activation(start, end, activation_start) {
            TrackActivationRelation::After => continue,
            TrackActivationRelation::Overlapping => return None,
            TrackActivationRelation::Completed => {}
        }
        let alpha = track.timing.terminal_progress();
        if alpha == 0.0 {
            continue;
        }
        if alpha != 1.0 {
            return None;
        }
        match (track.property, &track.completion) {
            (Property::Morph, SemanticAnimationCompletion::ContentMorph { content: target }) => {
                content = *target;
            }
            (Property::Transform, _) | (Property::Morph, _) => return None,
            _ => unreachable!("filtered to content-changing prepared tracks"),
        }
    }
    Some(content)
}

fn apply_effective_track_endpoint(
    value: &mut EffectiveAnimationProperties,
    property: Property,
    values: &TrackValues,
    at_end: bool,
) -> Option<()> {
    match (property, values) {
        (
            Property::GlowColor | Property::GlowRadius | Property::GlowIntensity,
            TrackValues::Glow {
                attachment,
                from,
                to,
                ..
            },
        ) => {
            let glow = value.glow.as_mut()?;
            if glow.attachment != *attachment {
                return None;
            }
            let endpoint = if at_end { *to } else { *from };
            glow.definition = endpoint.update().apply_to(glow.definition).ok()?;
        }
        (Property::ZIndex, TrackValues::ZIndex { from, to }) => {
            value.z_index = if at_end { *to } else { *from };
        }
        (
            Property::Position,
            TrackValues::Vec2 { from, to } | TrackValues::ArcVec2 { from, to, .. },
        ) => value.transform.translation = if at_end { *to } else { *from },
        (
            Property::Position,
            TrackValues::PathVec2 {
                path,
                path_transform,
                target_center_offset,
            },
        ) => {
            let (from, to) = noon_geometry::drawable_endpoints(path)?;
            let point = if at_end { to } else { from };
            value.transform.translation =
                path_transform.transform_point(point) - *target_center_offset;
        }
        (Property::Scale, TrackValues::Vec2 { from, to }) => {
            value.transform.scale = if at_end { *to } else { *from };
        }
        (Property::Rotation, TrackValues::Scalar { from, to }) => {
            value.transform.rotation = if at_end { *to } else { *from };
        }
        (Property::Fill, TrackValues::Color { from, to }) => {
            value.fill_gradient = None;
            value.fill_image = None;
            value.style.fill = if at_end { *to } else { *from };
        }
        (Property::Fill, TrackValues::Paint { transition }) => {
            let (paint, opacity) = transition.sample(if at_end { 1.0 } else { 0.0 }).ok()?;
            match paint {
                Some(noon_core::SemanticPaint::LinearGradient(gradient)) => {
                    value.fill_gradient = Some(gradient);
                    value.fill_image = None;
                    value.style.fill = Some(noon_core::Color::rgba(1.0, 1.0, 1.0, opacity as f32));
                }
                Some(noon_core::SemanticPaint::Image(image)) => {
                    value.fill_gradient = None;
                    value.fill_image = Some(image);
                    value.style.fill = Some(noon_core::Color::rgba(1.0, 1.0, 1.0, opacity as f32));
                }
                Some(noon_core::SemanticPaint::Solid(mut color)) => {
                    value.fill_gradient = None;
                    value.fill_image = None;
                    color.alpha = (f64::from(color.alpha) * opacity) as f32;
                    value.style.fill = Some(color);
                }
                None => {
                    value.fill_gradient = None;
                    value.fill_image = None;
                    value.style.fill = None;
                }
                _ => return None,
            }
        }
        (Property::Stroke, TrackValues::Color { from, to }) => {
            value.style.stroke = if at_end { *to } else { *from };
        }
        (Property::StrokeWidth, TrackValues::Scalar { from, to }) => {
            value.style.stroke_width = if at_end { *to } else { *from };
        }
        (Property::Opacity, TrackValues::Scalar { from, to }) => {
            value.style.opacity = if at_end { *to } else { *from };
        }
        (Property::Appearance, TrackValues::Scalar { from, to }) => {
            value.appearance = if at_end { *to } else { *from };
        }
        (Property::Reveal, TrackValues::Scalar { from, to }) => {
            value.reveal = if at_end { *to } else { *from };
        }
        (Property::WorldTransform, TrackValues::WorldTransform { from, to }) => {
            value.world_transform = Some((if at_end { *to } else { *from }).world()?);
        }
        (
            Property::CameraProfile,
            TrackValues::CameraProfile {
                from,
                to,
                near,
                far,
            },
        ) => {
            let profile = if at_end { *to } else { *from };
            let camera = profile.camera(*near, *far)?;
            value.camera_profile = Some((profile, *near, *far));
            value.world_transform = Some(noon_core::SemanticWorldTransform3D::new(
                camera.position,
                camera.orientation,
                noon_core::SemanticVec3::new(1.0, 1.0, 1.0),
            )?);
        }
        (Property::Morph, TrackValues::PreparedMorph { .. }) | (Property::Presence, _) => {}
        (Property::Transform, _) => return None,
        _ => return None,
    }
    Some(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrackActivationRelation {
    Completed,
    Overlapping,
    After,
}

fn track_relation_to_activation(start: f64, end: f64, activation: f64) -> TrackActivationRelation {
    let rounding = 4.0
        * f64::EPSILON
        * start
            .abs()
            .max(end.abs())
            .max(activation.abs())
            .max(f64::MIN_POSITIVE);
    if end - activation <= rounding {
        TrackActivationRelation::Completed
    } else if start - activation > rounding {
        TrackActivationRelation::After
    } else {
        TrackActivationRelation::Overlapping
    }
}

pub(super) fn known_intervals<'a>(
    leaves: impl Iterator<Item = &'a PreparedSemanticScheduledAnimationLeaf>,
) -> HashMap<SemanticTransactionNodeRef, (f64, f64)> {
    leaves
        .filter_map(|leaf| {
            continuous_time_map_interval(leaf.timing, &leaf.time_map)
                .ok()
                .map(|interval| (leaf.animation, interval))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompiledGlow, PreparedSemanticScheduledAnimationPayload};
    use noon_core::{
        Color, CompositionTimeMap, CompositionTimeMapStep, Glow, GlowTrackEndBehavior,
        GlowTrackValue, GlowUpdate, Pixels, RateFunction, ResolvedAnimationOptions, SemanticNodeId,
        SemanticTransformInterpolation, Style, TrackTiming, Transform2D, Vec2,
    };

    #[test]
    fn sequential_capture_uses_channel_terminal_behavior() {
        let object = ObjectId::new(1);
        let attachment = SemanticNodeId::new(3, 7);
        let first_animation = SemanticTransactionNodeRef::Existing(SemanticNodeId::new(10, 0));
        let next_animation = SemanticTransactionNodeRef::Existing(SemanticNodeId::new(11, 0));
        let target = SemanticTransactionNodeRef::Existing(SemanticNodeId::new(1, 0));
        let captured_intensity = 0.613456789012345;
        let peak = 2.0;
        let source = EffectiveAnimationProperties {
            fill_gradient: None,
            fill_image: None,
            glow: Some(CompiledGlow {
                attachment,
                definition: Glow::new(
                    GlowUpdate::default()
                        .color(Color::BLUE)
                        .radius(Pixels(3.25))
                        .intensity(0.35),
                )
                .unwrap(),
            }),
            z_index: 4.5,
            transform: Transform2D {
                translation: Vec2::new(2.0, -1.0),
                ..Transform2D::IDENTITY
            },
            style: Style::default(),
            appearance: 0.75,
            reveal: 1.0,
            world_transform: None,
            camera_profile: None,
        };
        let leaf = PreparedSemanticScheduledAnimationLeaf {
            finish_time_map: CompositionTimeMap::identity(),
            animation: next_animation,
            target,
            execution_object_id: object,
            payload: PreparedSemanticScheduledAnimationPayload::TransformTo {
                target_state: target,
                interpolation: SemanticTransformInterpolation::Affine,
                complete_priority: false,
            },
            timing: TrackTiming::new(0.0, 4.0, RateFunction::Linear),
            time_map: CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
                0.5,
                0.25,
                RateFunction::Linear,
            )]),
            options: ResolvedAnimationOptions {
                run_time: 1.0,
                rate_func: RateFunction::Linear,
                lag_ratio: 0.0,
                path_arc: 0.0,
                reverse_rate_function: false,
                remover: false,
                introducer: false,
            },
        };
        let intervals =
            HashMap::from([(first_animation, (0.0, 2.0)), (next_animation, (2.0, 3.0))]);
        for (end_behavior, rate, expected_intensity) in [
            (
                GlowTrackEndBehavior::RestoreSource,
                RateFunction::Linear,
                captured_intensity,
            ),
            (GlowTrackEndBehavior::Sample, RateFunction::Linear, peak),
            // This ordinary terminal sample is exactly one in f32, but slightly
            // below one in f64. Endpoint capture keeps its existing contract.
            (GlowTrackEndBehavior::Sample, RateFunction::EaseInSine, peak),
        ] {
            let track = PreparedSemanticAnimationTrack {
                animation: first_animation,
                target,
                execution_object_id: object,
                property: Property::GlowIntensity,
                completion: SemanticAnimationCompletion::Release,
                values: TrackValues::Glow {
                    attachment,
                    from: GlowTrackValue::Intensity(captured_intensity),
                    to: GlowTrackValue::Intensity(peak),
                    end_behavior,
                },
                timing: TrackTiming::new(0.0, 4.0, rate),
                time_map: CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
                    0.0,
                    0.5,
                    RateFunction::Linear,
                )]),
            };
            let mut expected = source.clone();
            let glow = expected.glow.as_mut().unwrap();
            glow.definition = GlowUpdate::default()
                .intensity(expected_intensity)
                .apply_to(glow.definition)
                .unwrap();
            let tracks = [track];
            let mut captures = ScheduledCaptures::default();
            captures.insert(object, source.clone());
            let mut driven =
                HashMap::from([(driver_key(object, Property::GlowIntensity), first_animation)]);

            captures.begin_leaf(&leaf, &mut driven, &tracks, &intervals, true);
            assert_eq!(captures.get(object), Some(expected.clone()));
            assert!(driven.is_empty(), "completed child releases its channel");
            assert_eq!(
                completed_effective_properties_before(source.clone(), object, 2.0, &tracks),
                Some(expected),
                "activation correspondence and ordinary sequential captures agree"
            );
            assert_eq!(
                completed_effective_properties_before(source.clone(), object, 1.0, &tracks),
                None,
                "an overlapping driver remains unavailable for endpoint capture"
            );
        }
    }
}
