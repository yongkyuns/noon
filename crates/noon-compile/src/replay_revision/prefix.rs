//! Experimental sparse lifecycle/content prefix transfer. Uses the existing compiler
//! revision representation; no second scene, evaluator, or resource registry.
use super::{CompiledReplayRevision, CompiledScene, CompiledTrack, SavedRow};
use crate::{CompiledEffectStack, CompiledObject};
use noon_core::{Property, TrackId, TrackValues};
use std::collections::BTreeMap;

#[cfg(test)]
mod authored_view;
#[cfg(test)]
mod backdrop;
mod family;
mod order;
mod resources;
mod storage;
use order::SparseOrder;

fn local_property(property: Property) -> bool {
    matches!(
        property,
        Property::Presence
            | Property::ZIndex
            | Property::Transform
            | Property::Position
            | Property::Rotation
            | Property::Scale
            | Property::Fill
            | Property::Stroke
            | Property::StrokeWidth
            | Property::Emission
            | Property::Opacity
            | Property::Appearance
            | Property::Reveal
            | Property::Morph
            | Property::EffectColor
            | Property::EffectRadius
            | Property::EffectIntensity
            | Property::EffectThreshold
            | Property::EffectProgress
            | Property::EffectSoftness
            | Property::EffectSeed
    )
}

/// The bounded built-in history profile: optional Blur, then optional Glow,
/// then optional stack-owned visibility NoiseDissolve. Arbitrary stacks and
/// shader programs remain outside this profile.
pub(super) fn local_effect_stack(stack: &CompiledEffectStack) -> bool {
    use noon_core::EffectDefinition::{BackdropBlur, Blur, Glow, NoiseDissolve};

    match (stack.definitions(), stack.visibility()) {
        ([], None) => true,
        ([BackdropBlur(_)], None) => true,
        ([Blur(_)], None) => true,
        ([Glow(_)], None) => true,
        ([Blur(_), Glow(_)], None) => true,
        ([NoiseDissolve(_)], Some(visibility))
        | ([Blur(_), NoiseDissolve(_)], Some(visibility))
        | ([Glow(_), NoiseDissolve(_)], Some(visibility))
        | ([Blur(_), Glow(_), NoiseDissolve(_)], Some(visibility)) => {
            stack.attachments().last() == Some(&visibility.attachment)
        }
        _ => false,
    }
}

fn local_track(track: Option<&CompiledTrack>) -> bool {
    track.is_none_or(|track| {
        local_property(track.property) && super::path_motion_command_count(&track.values) == 0
    })
}

fn camera_owner(object: &CompiledObject) -> bool {
    object.live
        && object.spatial.as_deref().is_some_and(|spatial| {
            spatial.camera_projection.is_some()
                && spatial.camera_profile.is_some()
                && !spatial.point_light
                && spatial.spatial_anchor_family.is_none()
                && spatial.fixed_orientation_center.is_none()
                && spatial.cairo_path_appearance.is_none()
        })
}

fn world_spatial_owner(object: &CompiledObject) -> bool {
    object.live
        && object.spatial.as_deref().is_some_and(|spatial| {
            spatial.camera_projection.is_none()
                && spatial.camera_profile.is_none()
                && spatial.camera_motions.is_none()
                && spatial.composition_domain == noon_core::SemanticSpatialCompositionDomain::World
                && spatial.spatial_anchor_family.is_none()
                && spatial.fixed_orientation_center.is_none()
                && spatial.cairo_path_appearance.is_none()
        })
}

fn spatial_history_owner(object: &CompiledObject) -> bool {
    camera_owner(object) || world_spatial_owner(object)
}

fn same_world_spatial_owner(before: &CompiledObject, after: &CompiledObject) -> bool {
    world_spatial_owner(before)
        && world_spatial_owner(after)
        && before
            .spatial
            .as_deref()
            .zip(after.spatial.as_deref())
            .is_some_and(|(before, after)| {
                before.material == after.material
                    && before.point_light == after.point_light
                    && before.draw_kind == after.draw_kind
            })
}

fn local_channel_for_objects(
    property: Property,
    before: &CompiledObject,
    after: &CompiledObject,
) -> bool {
    match property {
        Property::EffectThreshold => authored_bloom_owner(before) && authored_bloom_owner(after),
        property if local_property(property) => before.spatial.is_none() && after.spatial.is_none(),
        Property::WorldTransform => {
            (camera_owner(before) && camera_owner(after)) || same_world_spatial_owner(before, after)
        }
        Property::CameraProfile => camera_owner(before) && camera_owner(after),
        _ => false,
    }
}

fn local_track_for_object(track: Option<&CompiledTrack>, object: Option<&CompiledObject>) -> bool {
    track.is_none_or(|track| {
        let typed_spatial_track = matches!(
            track.property,
            Property::WorldTransform | Property::CameraProfile
        ) && object.is_some_and(|object| match track.property {
            Property::WorldTransform => spatial_history_owner(object),
            Property::CameraProfile => camera_owner(object),
            _ => false,
        }) && object.is_some_and(|object| {
            crate::valid_compiled_track_for_spatial(
                track.property,
                &track.values,
                object.spatial.as_deref(),
            )
        }) && match (&track.property, &track.values) {
            (Property::WorldTransform, TrackValues::WorldTransform { .. }) => true,
            (
                Property::CameraProfile,
                TrackValues::CameraProfile {
                    from,
                    to,
                    near,
                    far,
                },
            ) => from.camera(*near, *far).is_some() && to.camera(*near, *far).is_some(),
            _ => false,
        };
        (if object.is_some_and(|object| object.spatial.is_some()) {
            typed_spatial_track
        } else {
            local_property(track.property)
        }) && super::path_motion_command_count(&track.values) == 0
    })
}

fn local_effect_track(track: &CompiledTrack, object: Option<&CompiledObject>) -> bool {
    use noon_core::{EffectTrackValue, TrackValues};

    let property = track.property;
    if !matches!(
        property,
        Property::EffectColor
            | Property::EffectRadius
            | Property::EffectIntensity
            | Property::EffectThreshold
            | Property::EffectProgress
            | Property::EffectSoftness
            | Property::EffectSeed
    ) {
        return true;
    }
    let Some(object) = object else {
        return false;
    };
    let Some(stack) = object.effects.as_deref() else {
        return false;
    };
    if !local_effect_stack_for_object(object, stack) {
        return false;
    }
    let TrackValues::Effect {
        attachment,
        from,
        to,
        ..
    } = &track.values
    else {
        return false;
    };
    if from.property() != property || to.property() != property {
        return false;
    }
    let Some(effect) = stack.get(*attachment) else {
        return false;
    };
    match (property, from, to) {
        (Property::EffectColor, EffectTrackValue::Color(_), EffectTrackValue::Color(_)) => {
            effect.definition.as_glow().is_some()
        }
        (Property::EffectRadius, EffectTrackValue::Radius(_), EffectTrackValue::Radius(_)) => {
            effect.definition.as_blur().is_some()
                || effect.definition.as_glow().is_some()
                || effect.definition.as_backdrop_blur().is_some()
                || (authored_view_owner(object) && effect.definition.as_bloom().is_some())
        }
        (
            Property::EffectIntensity,
            EffectTrackValue::Intensity(_),
            EffectTrackValue::Intensity(_),
        ) => {
            effect.definition.as_glow().is_some()
                || (authored_view_owner(object) && effect.definition.as_bloom().is_some())
        }
        (
            Property::EffectThreshold,
            EffectTrackValue::Threshold(_),
            EffectTrackValue::Threshold(_),
        ) => authored_view_owner(object) && effect.definition.as_bloom().is_some(),
        (
            Property::EffectProgress | Property::EffectSoftness,
            EffectTrackValue::Progress(_) | EffectTrackValue::Softness(_),
            EffectTrackValue::Progress(_) | EffectTrackValue::Softness(_),
        ) => {
            stack
                .visibility()
                .is_some_and(|visibility| visibility.attachment == *attachment)
                && effect.definition.as_noise_dissolve().is_some()
        }
        (Property::EffectSeed, EffectTrackValue::Seed(_), EffectTrackValue::Seed(_)) => {
            stack
                .visibility()
                .is_some_and(|visibility| visibility.attachment == *attachment)
                && effect.definition.as_noise_dissolve().is_some()
        }
        _ => false,
    }
}

fn authored_view_owner(object: &CompiledObject) -> bool {
    object.live
        && object.spatial.is_none()
        && object.view_color_profile == Some(noon_core::ViewColorProfile::LinearHdr)
}

fn authored_bloom_owner(object: &CompiledObject) -> bool {
    authored_view_owner(object)
        && object.effects.as_deref().is_some_and(|stack| {
            matches!(stack.definitions(), [noon_core::EffectDefinition::Bloom(_)])
                && stack.visibility().is_none()
                && stack.attachments().len() == 1
        })
}

fn local_effect_stack_for_object(object: &CompiledObject, stack: &CompiledEffectStack) -> bool {
    use noon_core::{EffectDefinition::Bloom, EffectInputRole};

    if stack
        .definitions()
        .iter()
        .any(|definition| definition.input_role() == EffectInputRole::AuthoredView)
    {
        authored_bloom_owner(object) && matches!(stack.definitions(), [Bloom(_)])
    } else {
        local_effect_stack(stack)
    }
}

fn local_object(object: &CompiledObject, physical: &CompiledObject) -> bool {
    object.live
        && object.id == physical.id
        && ((object.spatial.is_none() && physical.spatial.is_none())
            || (camera_owner(object) && camera_owner(physical))
            || same_world_spatial_owner(object, physical))
        && object
            .effects
            .as_ref()
            .is_none_or(|stack| local_effect_stack_for_object(object, stack))
}

impl CompiledReplayRevision {
    fn local_prefix_metadata(&self) -> bool {
        self.inset_2d_view.is_none()
            && self.path_motion_commands == 0
            && self.rows.iter().filter(|row| row.order.is_some()).count() <= 1
    }

    /// Inset relations are retained by ordinary replay, but not yet presented
    /// by prepared readers. Bootstrap must inspect saved history as well as the
    /// current projection so a removed relation cannot disappear from admission.
    #[doc(hidden)]
    pub fn changes_inset_2d_view(&self) -> bool {
        self.inset_2d_view.is_some()
    }
}

impl CompiledScene {
    /// Necessary suffix-profile checks before advancing the canonical producer.
    /// Visits only saved payloads and their current counterparts, without cloning
    /// rows/tracks or reconstructing ordering. Complete extraction remains fallible.
    pub fn supports_prefix_revision(&self, old: &CompiledReplayRevision) -> bool {
        if !old.local_prefix_metadata() {
            return false;
        }
        if !old.channels.iter().all(|channel| {
            old.rows
                .iter()
                .find(|row| row.index == channel.object_index)
                .and_then(|row| row.object.as_ref())
                .zip(self.objects.get(channel.object_index as usize))
                .is_some_and(|(before, after)| {
                    local_channel_for_objects(channel.property, before, after)
                })
        }) {
            return false;
        }
        for saved in &old.rows {
            let Some(physical) = self.objects.get(saved.index as usize) else {
                return false;
            };
            for object in saved.object.iter().chain(physical.live.then_some(physical)) {
                if !local_object(object, physical) {
                    return false;
                }
            }
        }
        old.tracks.iter().all(|(id, before)| {
            let after = self.track(*id);
            let Some(index) = before
                .as_ref()
                .or(after)
                .map(|track| track.object_index as usize)
            else {
                return local_track(before.as_ref()) && local_track(after);
            };
            let physical = self.objects.get(index).filter(|object| object.live);
            let before_object = old
                .rows
                .iter()
                .find(|row| row.index as usize == index)
                .map_or(physical, |row| row.object.as_ref());
            let valid = local_track_for_object(before.as_ref(), before_object)
                && local_track_for_object(after, physical)
                && before
                    .as_ref()
                    .is_none_or(|track| local_effect_track(track, before_object))
                && after.is_none_or(|track| local_effect_track(track, physical));
            valid
        })
    }

    /// Recover the forward sides of a NEW suffix of inverse revisions without
    /// rewinding the producer or cloning its compiled scene/history.
    ///
    /// Reverse traversal keeps only touched rows/tracks in a scratch overlay.
    /// Capturing each revision from the latest scene directly would leak a later
    /// write into earlier timestamps when a row is changed more than once.
    ///
    /// Ordinary 2D lifecycle/content/property changes and typed profile history
    /// for camera and World-domain spatial owners are admitted. Family transforms,
    /// path-motion, anchored composition and Cairo path bounds require separate
    /// qualification.
    /// Append-only glyph plans travel through the existing immutable plan table.
    /// Validated 2D transform/morph plans retain their existing immutable geometry.
    /// Rows may carry the bounded ordered Blur/Glow profile with optional
    /// trailing visibility NoiseDissolve, or a standalone BackdropBlur with typed
    /// radius tracks. Authored-view Bloom history is limited to the actual linear-HDR
    /// camera owner and its typed radius, intensity and threshold channels. Other
    /// effect definitions remain outside this suffix profile.
    /// Ordering reconstruction visits only affected rows and their neighbours;
    /// it never clones/scans the full painter/family order.
    /// The caller must pass the complete, ordered suffix ending at this scene's
    /// current frontier and validate originating runtime/scope identity first.
    /// None leaves this scene unchanged and publishes nothing.
    pub fn forward_prefix_revisions(
        &self,
        inverse: &[&CompiledReplayRevision],
    ) -> Option<Vec<CompiledReplayRevision>> {
        let mut row_overlay = BTreeMap::new();
        let mut order = SparseOrder::new(self);
        let mut track_overlay: BTreeMap<TrackId, Option<CompiledTrack>> = BTreeMap::new();
        let mut output = Vec::with_capacity(inverse.len());
        for old in inverse.iter().rev() {
            // Empty revisions denote append-only glyph plans. Their immutable
            // table suffix is delivered separately; mapped starts gate activity.
            if !self.supports_prefix_revision(old) {
                return None;
            }
            let mut rows = Vec::with_capacity(old.rows.len());
            let mut after_objects = BTreeMap::new();
            for saved in &old.rows {
                let physical = self.objects.get(saved.index as usize)?;
                let current = row_overlay
                    .entry(saved.index)
                    .or_insert_with(|| physical.live.then(|| physical.clone()));
                for object in saved.object.iter().chain(current.iter()) {
                    if !local_object(object, physical) {
                        return None;
                    }
                }
                after_objects.insert(saved.index, current.clone());
                // Capture the actual opposite membership/order, then undo this
                // one local order change in the sparse scratch view.
                let forward_order = if let Some(before) = saved.order {
                    let after = order.successor(saved.index);
                    order.restore(saved.index, saved.object.is_some(), before)?;
                    Some(after)
                } else {
                    if saved.object.is_none() || current.is_none() {
                        return None;
                    }
                    None
                };
                rows.push(SavedRow {
                    index: saved.index,
                    object: current.clone(),
                    order: forward_order,
                });
                *current = saved.object.clone();
            }
            let mut tracks = Vec::with_capacity(old.tracks.len());
            for (id, before) in &old.tracks {
                let after = track_overlay
                    .entry(*id)
                    .or_insert_with(|| self.track(*id).cloned());
                let Some(index) = before
                    .as_ref()
                    .or(after.as_ref())
                    .map(|track| track.object_index as usize)
                else {
                    if !local_track(before.as_ref()) || !local_track(after.as_ref()) {
                        return None;
                    }
                    tracks.push((*id, after.clone()));
                    *after = before.clone();
                    continue;
                };
                let physical = self.objects.get(index)?;
                let before_object = old
                    .rows
                    .iter()
                    .find(|row| row.index as usize == index)
                    .and_then(|row| row.object.as_ref())
                    .or_else(|| physical.live.then_some(physical));
                if !local_track_for_object(before.as_ref(), before_object)
                    || !local_track_for_object(after.as_ref(), Some(physical))
                {
                    return None;
                }
                if let Some(index) = before
                    .as_ref()
                    .or(after.as_ref())
                    .map(|track| track.object_index)
                {
                    let physical = self
                        .objects
                        .get(index as usize)
                        .filter(|object| object.live);
                    let before_object = row_overlay.get(&index).map_or(physical, Option::as_ref);
                    let after_object = after_objects
                        .get(&index)
                        .map_or(before_object, Option::as_ref);
                    if before
                        .as_ref()
                        .is_some_and(|track| !local_effect_track(track, before_object))
                        || after
                            .as_ref()
                            .is_some_and(|track| !local_effect_track(track, after_object))
                    {
                        return None;
                    }
                }
                tracks.push((*id, after.clone()));
                *after = before.clone();
            }
            output.push(CompiledReplayRevision {
                rows,
                inset_2d_view: None,
                tracks,
                channels: old.channels.clone(),
                camera_motion_payloads: old.camera_motion_payloads,
                path_motion_commands: 0,
                geometry_payload_bytes: old.geometry_payload_bytes,
                effect_payload_bytes: old.effect_payload_bytes,
                composed_payload_bytes: old.composed_payload_bytes,
            });
        }
        output.reverse();
        Some(output)
    }
}

#[cfg(test)]
mod camera_prefix_tests {
    use super::*;
    use crate::{CompiledObject, CompiledSpatialDrawKind, CompiledSpatialState};
    use noon_core::{
        GeometryRef, ManimCamera3DProfile, SemanticSpatialCompositionDomain,
        SemanticSpatialMaterial, SemanticVec3, SemanticWorldTransform3D, Style, Transform2D,
        WorldTransformTrackEndpoint,
    };

    fn object(camera: bool, point_light: bool) -> CompiledObject {
        let mut object = CompiledObject::new(
            noon_core::ObjectId::new(0),
            GeometryRef::circle(0.0),
            Transform2D::IDENTITY,
            Style::default(),
        );
        let profile = ManimCamera3DProfile {
            phi: 0.6,
            theta: -1.2,
            gamma: 0.0,
            focal_distance: 5.0,
            zoom: 1.0,
            frame_height: 8.0,
            frame_center: SemanticVec3::ZERO,
        };
        let camera_pose = profile.camera(0.1, 100.0).unwrap();
        object.spatial = Some(Box::new(CompiledSpatialState {
            world: SemanticWorldTransform3D::new(
                camera_pose.position,
                camera_pose.orientation,
                SemanticVec3::new(1.0, 1.0, 1.0),
            )
            .unwrap(),
            camera_projection: camera.then(|| {
                profile
                    .camera(0.1, 100.0)
                    .expect("test camera profile is valid")
                    .projection
            }),
            camera_profile: camera.then_some(profile),
            camera_motions: None,
            material: SemanticSpatialMaterial::Unlit,
            point_light,
            composition_domain: SemanticSpatialCompositionDomain::World,
            draw_kind: CompiledSpatialDrawKind::Planar,
            spatial_anchor_family: None,
            fixed_orientation_center: None,
            cairo_path_appearance: None,
        }));
        object
    }

    #[test]
    fn spatial_suffix_predicate_admits_world_objects_without_broadening_camera_roles() {
        let camera = object(true, false);
        let mesh = object(false, false);
        let light = object(false, true);
        assert!(camera_owner(&camera));
        assert!(!camera_owner(&mesh));
        assert!(!camera_owner(&light));
        assert!(local_object(&camera, &camera));
        assert!(world_spatial_owner(&mesh));
        assert!(world_spatial_owner(&light));
        assert!(local_object(&mesh, &mesh));
        assert!(local_object(&light, &light));
        let mut changed_material = mesh.clone();
        changed_material.spatial.as_deref_mut().unwrap().material =
            SemanticSpatialMaterial::CairoSurface;
        assert!(!same_world_spatial_owner(&mesh, &changed_material));

        let mut anchored = mesh.clone();
        anchored.spatial.as_deref_mut().unwrap().composition_domain =
            SemanticSpatialCompositionDomain::FixedOrientation;
        assert!(!world_spatial_owner(&anchored));
        let mut cairo_path = mesh.clone();
        cairo_path
            .spatial
            .as_deref_mut()
            .unwrap()
            .cairo_path_appearance = Some(Box::new(crate::CompiledCairoPathAppearance {
            sheen_factor: 0.5,
            gradient_direction: None,
            world_family_bounds: None,
        }));
        assert!(!world_spatial_owner(&cairo_path));
        assert!(!local_object(&anchored, &anchored));
        assert!(!local_object(&cairo_path, &cairo_path));

        let camera_track = CompiledTrack {
            id: TrackId::new(1),
            object_index: 0,
            property: Property::WorldTransform,
            values: TrackValues::WorldTransform {
                from: WorldTransformTrackEndpoint::from_world(SemanticWorldTransform3D::IDENTITY),
                to: WorldTransformTrackEndpoint::from_world(SemanticWorldTransform3D::IDENTITY),
            },
            timing: noon_core::TrackTiming::new(0.0, 1.0, noon_core::RateFunction::Linear),
            time_map: noon_core::CompositionTimeMap::identity(),
            transform_geometry_plan: None,
            reconciled: false,
        };
        assert!(local_track_for_object(Some(&camera_track), Some(&camera)));
        assert!(local_track_for_object(Some(&camera_track), Some(&mesh)));
        assert!(local_track_for_object(Some(&camera_track), Some(&light)));

        let mut fill = camera_track.clone();
        fill.property = Property::Fill;
        fill.values = TrackValues::Color {
            from: Some(noon_core::Color::WHITE),
            to: Some(noon_core::Color::RED),
        };
        assert!(!local_track_for_object(Some(&fill), Some(&mesh)));
        assert!(!local_track_for_object(Some(&fill), Some(&light)));
        assert!(!local_channel_for_objects(Property::Fill, &light, &light));
        assert!(!local_track_for_object(
            Some(&camera_track),
            Some(&anchored)
        ));
        assert!(!local_track_for_object(
            Some(&camera_track),
            Some(&cairo_path)
        ));
    }

    #[test]
    fn camera_motion_entries_stay_charged_in_forward_revisions() {
        let id = noon_core::ObjectId::new(0);
        let mut camera = object(true, false);
        camera.id = id;
        let spatial = camera.spatial.as_deref().unwrap();
        let profile = spatial.camera_profile.unwrap();
        let first = noon_core::CameraAngularMotion::new(
            profile,
            noon_core::CameraRotationAxis::Theta,
            0.1,
            0.0,
            Some(0.5),
            0.1,
            100.0,
        )
        .unwrap();
        let second = noon_core::CameraAngularMotion::new(
            profile,
            noon_core::CameraRotationAxis::Theta,
            0.1,
            0.5,
            Some(1.0),
            0.1,
            100.0,
        )
        .unwrap();
        camera.spatial.as_deref_mut().unwrap().camera_motions = Some(std::sync::Arc::from([first]));
        let mut compiled = CompiledScene::compile_objects(vec![camera.clone()], &[]).unwrap();
        let mut next_spatial = camera.spatial.as_deref().unwrap().clone();
        next_spatial.camera_motions = Some(std::sync::Arc::from([first, second]));
        let patch = crate::ExecutionPatch::SetSpatialState {
            object: id,
            base_transform: camera.base_transform,
            spatial: Some(next_spatial),
        };
        let inverse = compiled.prepare_replay_revision(&patch).unwrap();
        compiled.apply_execution_patch(&patch).unwrap();

        assert_eq!(inverse.camera_motion_payloads, 3);
        assert!(compiled.supports_prefix_revision(&inverse));
        let forward = compiled.forward_prefix_revisions(&[&inverse]).unwrap();
        assert_eq!(forward[0].camera_motion_payloads, 3);
        assert_eq!(forward[0].retention_cost(), inverse.retention_cost());
    }
}

#[cfg(test)]
mod effect_profile_tests {
    use super::*;
    use crate::{CompiledEffect, CompiledObject};
    use noon_core::{
        Blur, CompositionTimeMap, EffectDefinition, EffectTrackEndBehavior, EffectTrackValue,
        GeometryRef, Glow, NoiseDissolve, ObjectId, RateFunction, SemanticNodeId, Style,
        TrackTiming, TrackValues, Transform2D,
    };
    use std::sync::Arc;

    fn effect_track(
        property: Property,
        attachment: SemanticNodeId,
        from: EffectTrackValue,
        to: EffectTrackValue,
    ) -> CompiledTrack {
        CompiledTrack {
            id: TrackId::new(0),
            object_index: 0,
            property,
            values: TrackValues::Effect {
                attachment,
                from,
                to,
                end_behavior: EffectTrackEndBehavior::Sample,
            },
            timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
            transform_geometry_plan: None,
            reconciled: false,
        }
    }

    #[test]
    fn effect_tracks_are_limited_to_the_matching_profile_attachment() {
        let blur = SemanticNodeId::new(7, 1);
        let glow = SemanticNodeId::new(8, 1);
        let visibility = SemanticNodeId::new(9, 1);
        let mut object = CompiledObject::new(
            ObjectId::new(0),
            GeometryRef::circle(0.5),
            Transform2D::IDENTITY,
            Style::default(),
        );
        object.effects = Some(Arc::new(
            CompiledEffectStack::new_with_visibility(
                [
                    CompiledEffect {
                        attachment: blur,
                        definition: EffectDefinition::Blur(Blur::default()),
                    },
                    CompiledEffect {
                        attachment: glow,
                        definition: EffectDefinition::Glow(Glow::default()),
                    },
                    CompiledEffect {
                        attachment: visibility,
                        definition: EffectDefinition::NoiseDissolve(NoiseDissolve::default()),
                    },
                ],
                Some(visibility),
            )
            .unwrap(),
        ));

        assert!(local_effect_track(
            &effect_track(
                Property::EffectRadius,
                blur,
                EffectTrackValue::Radius(Blur::default().radius()),
                EffectTrackValue::Radius(Blur::default().radius()),
            ),
            Some(&object),
        ));
        assert!(local_effect_track(
            &effect_track(
                Property::EffectIntensity,
                glow,
                EffectTrackValue::Intensity(0.0),
                EffectTrackValue::Intensity(1.0),
            ),
            Some(&object),
        ));
        assert!(local_effect_track(
            &effect_track(
                Property::EffectProgress,
                visibility,
                EffectTrackValue::Progress(0.0),
                EffectTrackValue::Progress(1.0),
            ),
            Some(&object),
        ));
        assert!(!local_effect_track(
            &effect_track(
                Property::EffectIntensity,
                blur,
                EffectTrackValue::Intensity(0.0),
                EffectTrackValue::Intensity(1.0),
            ),
            Some(&object),
        ));
        assert!(!local_effect_track(
            &effect_track(
                Property::EffectProgress,
                glow,
                EffectTrackValue::Progress(0.0),
                EffectTrackValue::Progress(1.0),
            ),
            Some(&object),
        ));
    }
}
