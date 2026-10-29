use std::collections::{BTreeMap, BTreeSet, HashSet};

use noon_core::{
    continuous_time_map_interval, resolve_track_timing, validate_style, validate_track_definition,
    validate_transform, ObjectContentRef, ObjectId, Property, SemanticClickIndicate,
    TrackDefinition, TrackId, TrackValues,
};

use crate::{
    compile_patch_error, compile_transform_geometry_plan, map_object_state_error,
    validate_compiled_object, validate_execution_content, validate_execution_content_resource,
    CompilePatchError, CompiledChannelKey, CompiledScene, CompiledTrack,
    CompiledTransactionPreflightStats, ExecutionMutationTransaction, ExecutionPatch,
};

#[derive(Clone, Copy, Debug)]
struct TrackShadow {
    id: TrackId,
    object_index: u32,
    property: Property,
    attachment: Option<noon_core::SemanticNodeId>,
    start_time: f64,
    duration: f64,
    reconciliation_end: f64,
    authored_duration: f64,
    reconciled: bool,
    presence: Option<(bool, bool)>,
    continuous_paint: bool,
}

impl TrackShadow {
    fn from_compiled(track: &CompiledTrack) -> Self {
        let (start_time, end_time) =
            effective_track_interval(track.property, track.timing, &track.time_map);
        Self {
            id: track.id,
            object_index: track.object_index,
            property: track.property,
            attachment: track.channel().attachment(),
            start_time,
            duration: end_time - start_time,
            reconciliation_end: track.timing.start_time + track.timing.duration,
            authored_duration: track.timing.duration,
            reconciled: track.reconciled,
            presence: presence_endpoints(track.property, &track.values),
            continuous_paint: matches!(track.values, TrackValues::Paint { .. }),
        }
    }

    fn channel(self) -> CompiledChannelKey {
        match self.attachment {
            Some(attachment) => {
                CompiledChannelKey::effect(self.object_index, self.property, attachment)
            }
            None => CompiledChannelKey::new(self.object_index, self.property),
        }
    }

    fn from_definition(track: &TrackDefinition, object_index: u32) -> Self {
        let timing = resolve_track_timing(track).expect("track was validated before preflight");
        let (start_time, end_time) =
            effective_track_interval(track.property, timing, &track.time_map);
        Self {
            id: track.id,
            object_index,
            property: track.property,
            attachment: CompiledChannelKey::for_values(object_index, track.property, &track.values)
                .attachment(),
            start_time,
            duration: end_time - start_time,
            reconciliation_end: timing.start_time + timing.duration,
            authored_duration: timing.duration,
            reconciled: false,
            presence: presence_endpoints(track.property, &track.values),
            continuous_paint: matches!(track.values, TrackValues::Paint { .. }),
        }
    }
}

fn effective_track_interval(
    property: Property,
    timing: noon_core::TrackTiming,
    time_map: &noon_core::CompositionTimeMap,
) -> (f64, f64) {
    if property.is_instant() {
        return (timing.start_time, timing.start_time + timing.duration);
    }
    continuous_time_map_interval(timing, time_map)
        .expect("validated track has a valid composition time map")
}

enum ObjectOverlay {
    Present {
        effects: Option<std::sync::Arc<crate::CompiledEffectStack>>,
        click_indicate: Option<SemanticClickIndicate>,
        click_indicate_target: bool,
        index: u32,
        is_text: bool,
        continuous_fill: bool,
        spatial: Option<Box<crate::CompiledSpatialState>>,
    },
    Removed {
        index: u32,
    },
}

/// A transaction-local sparse overlay. It reads untouched identity and channel
/// metadata from the compiled scene and stores only identities changed by this
/// transaction; payload vectors are never cloned for validation.
struct PreflightOverlay {
    objects: BTreeMap<ObjectId, ObjectOverlay>,
    paint_definitions: BTreeMap<u32, bool>,
    view_color_profiles: BTreeMap<u32, Option<noon_core::ViewColorProfile>>,
    tracks: BTreeMap<TrackId, Option<TrackShadow>>,
    channels: BTreeMap<CompiledChannelKey, BTreeSet<TrackId>>,
    seen_objects: BTreeSet<ObjectId>,
    seen_tracks: BTreeSet<TrackId>,
    next_object_index: usize,
    track_metadata_visits: usize,
}

impl PreflightOverlay {
    fn new(scene: &CompiledScene) -> Self {
        Self {
            objects: BTreeMap::new(),
            paint_definitions: BTreeMap::new(),
            view_color_profiles: BTreeMap::new(),
            tracks: BTreeMap::new(),
            channels: BTreeMap::new(),
            seen_objects: BTreeSet::new(),
            seen_tracks: BTreeSet::new(),
            next_object_index: scene.objects.len(),
            track_metadata_visits: 0,
        }
    }

    fn object_index(&mut self, scene: &CompiledScene, id: ObjectId) -> Option<u32> {
        match self.objects.get(&id) {
            Some(ObjectOverlay::Present { index, .. }) => Some(*index),
            Some(ObjectOverlay::Removed { .. }) => None,
            None => {
                let index = scene.object_indices.get(&id).copied();
                if index.is_some() {
                    self.seen_objects.insert(id);
                }
                index
            }
        }
    }

    fn effects(
        &mut self,
        scene: &CompiledScene,
        id: ObjectId,
    ) -> Option<std::sync::Arc<crate::CompiledEffectStack>> {
        match self.objects.get(&id) {
            Some(ObjectOverlay::Present { effects, .. }) => effects.clone(),
            Some(ObjectOverlay::Removed { .. }) => None,
            None => {
                let index = scene.object_indices.get(&id).copied()?;
                self.seen_objects.insert(id);
                scene.objects[index as usize].effects.clone()
            }
        }
    }

    fn track_effect(
        &mut self,
        scene: &CompiledScene,
        track: &TrackDefinition,
    ) -> Option<crate::CompiledEffect> {
        let TrackValues::Effect { attachment, .. } = track.values else {
            return None;
        };
        self.effects(scene, track.object)
            .and_then(|stack| stack.get(attachment))
    }

    fn set_effects(
        &mut self,
        scene: &CompiledScene,
        id: ObjectId,
        effects: Option<std::sync::Arc<crate::CompiledEffectStack>>,
    ) {
        if let Some(ObjectOverlay::Present {
            effects: current, ..
        }) = self.objects.get_mut(&id)
        {
            *current = effects;
            return;
        }
        if let Some(index) = scene.object_indices.get(&id).copied() {
            self.seen_objects.insert(id);
            self.objects.insert(
                id,
                ObjectOverlay::Present {
                    effects,
                    click_indicate: scene.objects[index as usize]
                        .click_indicate
                        .as_deref()
                        .copied(),
                    click_indicate_target: crate::is_click_indicate_target(
                        &scene.objects[index as usize].content,
                    ),
                    index,
                    is_text: scene.objects[index as usize].text().is_some(),
                    continuous_fill: crate::continuous_fill_content_supported(
                        &scene.objects[index as usize].content,
                    ),
                    spatial: scene.objects[index as usize].spatial.clone(),
                },
            );
        }
    }

    fn spatial(
        &mut self,
        scene: &CompiledScene,
        id: ObjectId,
    ) -> Option<crate::CompiledSpatialState> {
        match self.objects.get(&id) {
            Some(ObjectOverlay::Present { spatial, .. }) => spatial.as_deref().cloned(),
            Some(ObjectOverlay::Removed { .. }) => None,
            None => {
                let index = scene.object_indices.get(&id).copied()?;
                self.seen_objects.insert(id);
                scene.objects[index as usize].spatial.as_deref().cloned()
            }
        }
    }

    fn set_spatial(
        &mut self,
        scene: &CompiledScene,
        id: ObjectId,
        spatial: Option<crate::CompiledSpatialState>,
    ) {
        if let Some(ObjectOverlay::Present {
            spatial: current, ..
        }) = self.objects.get_mut(&id)
        {
            *current = spatial.map(Box::new);
            return;
        }
        if let Some(index) = scene.object_indices.get(&id).copied() {
            self.seen_objects.insert(id);
            self.objects.insert(
                id,
                ObjectOverlay::Present {
                    effects: scene.objects[index as usize].effects.clone(),
                    click_indicate: scene.objects[index as usize]
                        .click_indicate
                        .as_deref()
                        .copied(),
                    click_indicate_target: crate::is_click_indicate_target(
                        &scene.objects[index as usize].content,
                    ),
                    index,
                    is_text: scene.objects[index as usize].text().is_some(),
                    continuous_fill: crate::continuous_fill_content_supported(
                        &scene.objects[index as usize].content,
                    ),
                    spatial: spatial.map(Box::new),
                },
            );
        }
    }

    fn object_is_text(&mut self, scene: &CompiledScene, id: ObjectId) -> Option<bool> {
        match self.objects.get(&id) {
            Some(ObjectOverlay::Present { is_text, .. }) => Some(*is_text),
            Some(ObjectOverlay::Removed { .. }) => None,
            None => {
                let index = scene.object_indices.get(&id).copied()?;
                self.seen_objects.insert(id);
                Some(scene.objects[index as usize].text().is_some())
            }
        }
    }

    fn click_indicate(
        &mut self,
        scene: &CompiledScene,
        id: ObjectId,
    ) -> Option<(bool, Option<SemanticClickIndicate>)> {
        match self.objects.get(&id) {
            Some(ObjectOverlay::Present {
                click_indicate,
                click_indicate_target,
                ..
            }) => Some((*click_indicate_target, *click_indicate)),
            Some(ObjectOverlay::Removed { .. }) => None,
            None => {
                let index = scene.object_indices.get(&id).copied()?;
                self.seen_objects.insert(id);
                let row = &scene.objects[index as usize];
                Some((
                    crate::is_click_indicate_target(&row.content),
                    row.click_indicate.as_deref().copied(),
                ))
            }
        }
    }

    fn set_click_indicate(
        &mut self,
        scene: &CompiledScene,
        id: ObjectId,
        binding: Option<SemanticClickIndicate>,
    ) {
        if let Some(ObjectOverlay::Present { click_indicate, .. }) = self.objects.get_mut(&id) {
            *click_indicate = binding;
            return;
        }
        if let Some(index) = scene.object_indices.get(&id).copied() {
            self.seen_objects.insert(id);
            let row = &scene.objects[index as usize];
            self.objects.insert(
                id,
                ObjectOverlay::Present {
                    effects: row.effects.clone(),
                    continuous_fill: crate::continuous_fill_content_supported(&row.content),
                    click_indicate: binding,
                    click_indicate_target: crate::is_click_indicate_target(&row.content),
                    index,
                    is_text: row.text().is_some(),
                    spatial: row.spatial.clone(),
                },
            );
        }
    }

    fn supports_continuous_fill(&self, scene: &CompiledScene, id: ObjectId) -> bool {
        match self.objects.get(&id) {
            Some(ObjectOverlay::Present {
                continuous_fill, ..
            }) => *continuous_fill,
            Some(ObjectOverlay::Removed { .. }) => false,
            None => scene.object_indices.get(&id).is_some_and(|&index| {
                crate::continuous_fill_content_supported(&scene.objects[index as usize].content)
            }),
        }
    }

    fn track(&mut self, scene: &CompiledScene, id: TrackId) -> Option<TrackShadow> {
        self.track_metadata_visits += 1;
        match self.tracks.get(&id).copied() {
            Some(track) => track,
            None => {
                let track = scene.track(id).map(TrackShadow::from_compiled);
                if track.is_some() {
                    self.seen_tracks.insert(id);
                }
                track
            }
        }
    }

    fn set_track(&mut self, id: TrackId, replacement: Option<TrackShadow>) {
        if let Some(Some(previous)) = self.tracks.get(&id) {
            let channel = previous.channel();
            if let Some(ids) = self.channels.get_mut(&channel) {
                ids.remove(&id);
                if ids.is_empty() {
                    self.channels.remove(&channel);
                }
            }
        }
        if let Some(track) = replacement {
            self.channels.entry(track.channel()).or_default().insert(id);
        }
        self.tracks.insert(id, replacement);
    }

    fn remove_object_tracks(&mut self, scene: &CompiledScene, object_index: u32) {
        for channel in scene.channels_for_object_index(object_index) {
            for track in scene.channel_tracks(channel) {
                self.track_metadata_visits += 1;
                self.seen_tracks.insert(track.id);
                // An earlier replacement may have moved this base track to a
                // different live object. Remove its current projection, never
                // blindly its historical owner.
                match self.tracks.get(&track.id) {
                    Some(Some(current)) if current.object_index != object_index => {}
                    _ => {
                        self.set_track(track.id, None);
                    }
                }
            }
        }
        let first = CompiledChannelKey::new(object_index, Property::Presence);
        let staged = self
            .channels
            .range(first..)
            .take_while(|(channel, _)| channel.object_index == object_index)
            .flat_map(|(_, ids)| ids.iter().copied())
            .collect::<Vec<_>>();
        for id in staged {
            self.track_metadata_visits += 1;
            self.set_track(id, None);
        }
    }

    fn channel(&mut self, scene: &CompiledScene, channel: CompiledChannelKey) -> Vec<TrackShadow> {
        let mut tracks = Vec::new();
        let mut base_ids = BTreeSet::new();
        for track in scene.channel_tracks(channel) {
            self.track_metadata_visits += 1;
            self.seen_tracks.insert(track.id);
            base_ids.insert(track.id);
            match self.tracks.get(&track.id).copied() {
                Some(Some(shadow)) if shadow.channel() == channel => tracks.push(shadow),
                Some(Some(_)) => {}
                Some(None) => {}
                None => tracks.push(TrackShadow::from_compiled(track)),
            }
        }
        for id in self.channels.get(&channel).into_iter().flatten() {
            self.track_metadata_visits += 1;
            if base_ids.contains(id) {
                continue;
            }
            tracks.push(self.tracks[id].expect("indexed staged channel contains a live track"));
        }
        tracks
    }

    fn require_paint_source(
        &mut self,
        scene: &CompiledScene,
        index: u32,
        supported: bool,
    ) -> Result<(), CompilePatchError> {
        let painted = self
            .paint_definitions
            .get(&index)
            .copied()
            .unwrap_or_else(|| {
                scene
                    .objects
                    .get(index as usize)
                    .is_some_and(|object| object.fill_paint.is_some())
            });
        if !supported
            && (painted
                || self
                    .channel(scene, CompiledChannelKey::new(index, Property::Fill))
                    .iter()
                    .any(|track| track.continuous_paint))
        {
            return Err(CompilePatchError::InvalidTrack(
                noon_core::TimelineError::InvalidPaintValues,
            ));
        }
        Ok(())
    }
}

pub(super) fn preflight_transaction(
    scene: &CompiledScene,
    transaction: &ExecutionMutationTransaction,
) -> Result<CompiledTransactionPreflightStats, CompilePatchError> {
    preflight_transaction_with_resources(scene, transaction, &crate::CompiledResources::default())
}

pub(super) fn preflight_transaction_with_resources(
    scene: &CompiledScene,
    transaction: &ExecutionMutationTransaction,
    additions: &crate::CompiledResources,
) -> Result<CompiledTransactionPreflightStats, CompilePatchError> {
    scene
        .resources
        .preflight_merge(additions)
        .map_err(CompilePatchError::Resource)?;
    let mut overlay = PreflightOverlay::new(scene);

    for patch in transaction.mutations() {
        match patch {
            ExecutionPatch::CreateObject(object) => {
                if overlay.object_index(scene, object.id).is_some() {
                    return Err(CompilePatchError::DuplicateObject(object.id));
                }
                validate_compiled_object(object)?;
                crate::validate_execution_paint_resources(
                    &scene.resources,
                    Some(additions),
                    object.fill_paint.as_deref(),
                )?;
                validate_execution_content_resource(
                    &scene.resources,
                    Some(additions),
                    object.id,
                    &object.content,
                    object.text_bounds,
                )?;
                let index = match overlay.objects.get(&object.id) {
                    Some(ObjectOverlay::Removed { index }) => *index,
                    _ => match scene.retired_object_indices.get(&object.id).copied() {
                        Some(index) => index,
                        None => {
                            let index = u32::try_from(overlay.next_object_index).map_err(|_| {
                                CompilePatchError::TooManyObjects(overlay.next_object_index)
                            })?;
                            overlay.next_object_index += 1;
                            index
                        }
                    },
                };
                overlay
                    .paint_definitions
                    .insert(index, object.fill_paint.is_some());
                // Ordinary creations need no view-metadata shadow allocation.
                // An explicit None still masks a former camera at a reused slot.
                if object.view_color_profile.is_some()
                    || scene
                        .objects
                        .get(index as usize)
                        .is_some_and(|row| row.view_color_profile.is_some())
                    || overlay.view_color_profiles.contains_key(&index)
                {
                    overlay
                        .view_color_profiles
                        .insert(index, object.view_color_profile);
                }
                overlay.objects.insert(
                    object.id,
                    ObjectOverlay::Present {
                        effects: object.effects.clone(),
                        click_indicate: object.click_indicate.as_deref().copied(),
                        click_indicate_target: crate::is_click_indicate_target(&object.content),
                        index,
                        is_text: object.text().is_some(),
                        continuous_fill: crate::continuous_fill_content_supported(&object.content),
                        spatial: object.spatial.clone(),
                    },
                );
            }
            ExecutionPatch::RemoveObject(id) => {
                let index = overlay
                    .object_index(scene, *id)
                    .ok_or(CompilePatchError::UnknownObject(*id))?;
                overlay
                    .objects
                    .insert(*id, ObjectOverlay::Removed { index });
                overlay.remove_object_tracks(scene, index);
                overlay.paint_definitions.insert(index, false);
            }
            ExecutionPatch::ReorderObject { object, before } => {
                if overlay.object_index(scene, *object).is_none() {
                    return Err(CompilePatchError::UnknownObject(*object));
                }
                if let Some(before) = before {
                    if overlay.object_index(scene, *before).is_none() {
                        return Err(CompilePatchError::UnknownObject(*before));
                    }
                }
            }
            ExecutionPatch::SetContent {
                object,
                content,
                text_bounds,
            } => {
                let index = overlay
                    .object_index(scene, *object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                validate_execution_content(*object, content, *text_bounds)?;
                let (_, click_indicate) = overlay
                    .click_indicate(scene, *object)
                    .expect("validated live object");
                crate::validate_click_indicate_binding(
                    *object,
                    crate::is_click_indicate_target(content),
                    click_indicate.as_ref(),
                )?;
                validate_execution_content_resource(
                    &scene.resources,
                    Some(additions),
                    *object,
                    content,
                    *text_bounds,
                )?;
                for property in [Property::Transform, Property::Morph] {
                    if let Some(track) = overlay
                        .channel(scene, CompiledChannelKey::new(index, property))
                        .iter()
                        .find(|track| !track.reconciled)
                    {
                        return Err(CompilePatchError::ContentReplacementHasGeometryDriver {
                            object: *object,
                            track: track.id,
                            property,
                        });
                    }
                }
                let spatial = overlay.spatial(scene, *object);
                let effects = overlay.effects(scene, *object);
                overlay.require_paint_source(
                    scene,
                    index,
                    crate::continuous_fill_content_supported(content) && spatial.is_none(),
                )?;
                overlay.objects.insert(
                    *object,
                    ObjectOverlay::Present {
                        effects,
                        click_indicate,
                        click_indicate_target: crate::is_click_indicate_target(content),
                        index,
                        is_text: matches!(content, ObjectContentRef::Text(_)),
                        continuous_fill: crate::continuous_fill_content_supported(content),
                        spatial: spatial.map(Box::new),
                    },
                );
            }
            ExecutionPatch::SetTransform { object, transform } => {
                if overlay.object_index(scene, *object).is_none() {
                    return Err(CompilePatchError::UnknownObject(*object));
                }
                if overlay.spatial(scene, *object).is_some() {
                    return Err(CompilePatchError::InvalidObjectState {
                        object: *object,
                        field: noon_core::ObjectStateField::Transform,
                    });
                }
                validate_transform(*object, *transform).map_err(map_object_state_error)?;
            }
            ExecutionPatch::SetSemanticTransform { object, transform } => {
                overlay
                    .object_index(scene, *object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                let spatial = overlay.spatial(scene, *object);
                let Some((_, next_spatial)) =
                    super::lower_semantic_transform_patch(*object, spatial.as_ref(), *transform)
                else {
                    return Err(CompilePatchError::InvalidObjectState {
                        object: *object,
                        field: noon_core::ObjectStateField::Transform,
                    });
                };
                let index = overlay
                    .object_index(scene, *object)
                    .expect("validated object");
                overlay.require_paint_source(scene, index, next_spatial.is_none())?;
                overlay.set_spatial(scene, *object, next_spatial);
            }
            ExecutionPatch::SetSpatialState {
                object,
                base_transform,
                spatial,
            } => {
                let index = overlay
                    .object_index(scene, *object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                validate_transform(*object, *base_transform).map_err(map_object_state_error)?;
                if !crate::valid_compiled_spatial(spatial.as_ref()) {
                    return Err(CompilePatchError::InvalidObjectState {
                        object: *object,
                        field: noon_core::ObjectStateField::Transform,
                    });
                }
                overlay.require_paint_source(scene, index, spatial.is_none())?;
                let object_is_mesh = scene.compiled_object_is_mesh(index as usize);
                if spatial.as_ref().is_some_and(|spatial| {
                    (spatial.draw_kind == crate::CompiledSpatialDrawKind::Mesh) != object_is_mesh
                }) || (object_is_mesh && spatial.is_none())
                {
                    return Err(CompilePatchError::InvalidObjectState {
                        object: *object,
                        field: noon_core::ObjectStateField::Transform,
                    });
                }
                const CHANNELS: [noon_core::Property; 15] = [
                    noon_core::Property::Presence,
                    noon_core::Property::ZIndex,
                    noon_core::Property::Transform,
                    noon_core::Property::WorldTransform,
                    noon_core::Property::CameraProfile,
                    noon_core::Property::Position,
                    noon_core::Property::Rotation,
                    noon_core::Property::Scale,
                    noon_core::Property::Fill,
                    noon_core::Property::Stroke,
                    noon_core::Property::StrokeWidth,
                    noon_core::Property::Opacity,
                    noon_core::Property::Appearance,
                    noon_core::Property::Reveal,
                    noon_core::Property::Morph,
                ];
                if CHANNELS.into_iter().any(|property| {
                    scene
                        .channel_tracks(CompiledChannelKey::new(index, property))
                        .iter()
                        .any(|track| {
                            !crate::valid_compiled_track_for_spatial(
                                track.property,
                                &track.values,
                                spatial.as_ref(),
                            )
                        })
                }) {
                    return Err(CompilePatchError::InvalidObjectState {
                        object: *object,
                        field: noon_core::ObjectStateField::Transform,
                    });
                }
                overlay.set_spatial(scene, *object, spatial.clone());
            }
            ExecutionPatch::SetDrawEnabled { object, .. } => {
                if overlay.object_index(scene, *object).is_none() {
                    return Err(CompilePatchError::UnknownObject(*object));
                }
            }
            ExecutionPatch::SetClickIndicate { object, binding } => {
                let (analytic_target, _) = overlay
                    .click_indicate(scene, *object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                crate::validate_click_indicate_binding(*object, analytic_target, binding.as_ref())?;
                overlay.set_click_indicate(scene, *object, *binding);
            }
            ExecutionPatch::SetZIndex { object, value } => {
                if overlay.object_index(scene, *object).is_none() {
                    return Err(CompilePatchError::UnknownObject(*object));
                }
                super::validate_z_index(*object, *value)?;
            }
            ExecutionPatch::SetEffects { object, effects } => {
                overlay
                    .object_index(scene, *object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                let previous = overlay.effects(scene, *object);
                crate::execution_patch::validate_effect_values(
                    *object,
                    previous.as_deref(),
                    effects,
                )?;
                overlay.set_effects(scene, *object, Some(std::sync::Arc::clone(effects)));
            }
            ExecutionPatch::SetEffectStack {
                object,
                expected,
                effects,
            } => {
                let index = overlay
                    .object_index(scene, *object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                let previous = overlay.effects(scene, *object);
                crate::effect_attachment::validate_change(
                    *object,
                    previous.as_deref(),
                    expected,
                    effects.as_deref(),
                )?;
                // Respect staged adds/replacements as well as base tracks. A
                // retired generation cannot retain drivers or block its successor.
                for attachment in expected {
                    if effects
                        .as_ref()
                        .is_some_and(|stack| stack.get(*attachment).is_some())
                    {
                        continue;
                    }
                    let retired = previous
                        .as_ref()
                        .and_then(|stack| stack.get(*attachment))
                        .expect("validated previous attachment");
                    for property in retired.parameter_properties() {
                        for track in overlay.channel(
                            scene,
                            CompiledChannelKey::effect(index, property, *attachment),
                        ) {
                            overlay.set_track(track.id, None);
                        }
                    }
                }
                overlay.set_effects(scene, *object, effects.clone());
            }
            ExecutionPatch::SetViewColorProfile { object, profile } => {
                let index = overlay
                    .object_index(scene, *object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                let current = overlay
                    .view_color_profiles
                    .get(&index)
                    .copied()
                    .unwrap_or_else(|| {
                        scene
                            .objects
                            .get(index as usize)
                            .and_then(|row| row.view_color_profile)
                    });
                if current.is_none() {
                    return Err(CompilePatchError::InvalidObjectState {
                        object: *object,
                        field: noon_core::ObjectStateField::ViewColorProfile,
                    });
                }
                overlay.view_color_profiles.insert(index, Some(*profile));
            }
            ExecutionPatch::SetStyle {
                object,
                style,
                fill_paint,
            } => {
                let index = overlay
                    .object_index(scene, *object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                validate_style(*object, *style).map_err(map_object_state_error)?;
                crate::execution_patch::validate_fill_paint(*object, fill_paint.as_deref())?;
                crate::validate_execution_paint_resources(
                    &scene.resources,
                    Some(additions),
                    fill_paint.as_deref(),
                )?;
                let supported = overlay.supports_continuous_fill(scene, *object)
                    && overlay.spatial(scene, *object).is_none();
                if fill_paint.is_some() && !supported {
                    return Err(CompilePatchError::InvalidObjectState {
                        object: *object,
                        field: noon_core::ObjectStateField::Style,
                    });
                }
                overlay
                    .paint_definitions
                    .insert(index, fill_paint.is_some());
            }
            ExecutionPatch::SetInset2DView { object, view } => {
                if overlay.object_index(scene, *object).is_none() {
                    return Err(CompilePatchError::UnknownObject(*object));
                }
                if view.is_some_and(|view| view.camera_frame == *object) {
                    return Err(CompilePatchError::InvalidInset2DView(*object));
                }
            }
            ExecutionPatch::SetGraphDependencies {
                owner,
                dependencies,
            } => {
                let mut seen = HashSet::with_capacity(dependencies.len());
                for dependency in dependencies {
                    if !seen.insert(dependency.edge) {
                        return Err(CompilePatchError::DuplicateGraphDependency {
                            owner: *owner,
                            edge: dependency.edge,
                        });
                    }
                    let mut require_object = |object| {
                        overlay
                            .object_index(scene, object)
                            .ok_or(CompilePatchError::UnknownObject(object))
                    };
                    require_object(dependency.start_vertex)?;
                    require_object(dependency.end_vertex)?;
                    require_object(dependency.line)?;
                    if let crate::CompiledGraphDependencyKind::Arrow {
                        end_tip, start_tip, ..
                    } = dependency.kind
                    {
                        require_object(end_tip)?;
                        if let Some(start_tip) = start_tip {
                            require_object(start_tip)?;
                        }
                    }
                }
                let count = scene
                    .graph_edge_dependencies
                    .len()
                    .checked_add(dependencies.len())
                    .ok_or(CompilePatchError::TooManyGraphDependencies(usize::MAX))?;
                if count != 0 && u32::try_from(count - 1).is_err() {
                    return Err(CompilePatchError::TooManyGraphDependencies(count));
                }
            }
            ExecutionPatch::SetSpatialAnchorGroupBoundsMembers { members, .. } => {
                for member in members {
                    if overlay.object_index(scene, *member).is_none() {
                        return Err(CompilePatchError::UnknownObject(*member));
                    }
                }
            }
            ExecutionPatch::AddTrack(track) => {
                if overlay.track(scene, track.id).is_some() {
                    return Err(CompilePatchError::DuplicateTrack(track.id));
                }
                let object_index = overlay
                    .object_index(scene, track.object)
                    .ok_or(CompilePatchError::UnknownObject(track.object))?;
                validate_track(track)?;
                crate::validate_execution_paint_track_resources(
                    &scene.resources,
                    Some(additions),
                    &track.values,
                )?;
                if matches!(track.values, TrackValues::Effect { .. }) {
                    crate::validate_effect_track_attachment(
                        track,
                        overlay.track_effect(scene, track),
                    )
                    .map_err(CompilePatchError::InvalidTrack)?;
                }
                let spatial = overlay.spatial(scene, track.object);
                if matches!(track.values, TrackValues::Paint { .. }) {
                    let supported = overlay.supports_continuous_fill(scene, track.object);
                    crate::validate_paint_track_profile(track, supported, spatial.is_some())
                        .map_err(CompilePatchError::InvalidTrack)?;
                }
                if !crate::valid_track_for_spatial(track, spatial.as_ref()) {
                    return Err(CompilePatchError::InvalidTrack(
                        noon_core::TimelineError::InvalidWorldTransformValues,
                    ));
                }
                reject_geometry_track_on_text(scene, &mut overlay, track)?;
                let shadow = TrackShadow::from_definition(track, object_index);
                overlay.set_track(track.id, Some(shadow));
                validate_camera_track_overlap(scene, &mut overlay, object_index)?;
                if shadow.property == Property::Presence {
                    validate_presence_channel(scene, &mut overlay, object_index)?;
                }
            }
            ExecutionPatch::AddFamilyAnimation(animation) => {
                if overlay.object_index(scene, animation.target).is_none() {
                    return Err(CompilePatchError::UnknownObject(animation.target));
                }
                animation
                    .spec
                    .validate()
                    .map_err(|_| CompilePatchError::InvalidFamilyAnimation)?;
                animation
                    .time_map
                    .validate()
                    .map_err(|_| CompilePatchError::InvalidFamilyAnimation)?;
                if animation.plan.leaf_for_object(animation.target).is_none() {
                    return Err(CompilePatchError::InvalidFamilyAnimation);
                }
            }
            ExecutionPatch::ReplaceTrack(track) => {
                let old = overlay
                    .track(scene, track.id)
                    .ok_or(CompilePatchError::UnknownTrack(track.id))?;
                let object_index = overlay
                    .object_index(scene, track.object)
                    .ok_or(CompilePatchError::UnknownObject(track.object))?;
                validate_track(track)?;
                crate::validate_execution_paint_track_resources(
                    &scene.resources,
                    Some(additions),
                    &track.values,
                )?;
                if matches!(track.values, TrackValues::Effect { .. }) {
                    crate::validate_effect_track_attachment(
                        track,
                        overlay.track_effect(scene, track),
                    )
                    .map_err(CompilePatchError::InvalidTrack)?;
                }
                let spatial = overlay.spatial(scene, track.object);
                if matches!(track.values, TrackValues::Paint { .. }) {
                    let supported = overlay.supports_continuous_fill(scene, track.object);
                    crate::validate_paint_track_profile(track, supported, spatial.is_some())
                        .map_err(CompilePatchError::InvalidTrack)?;
                }
                if !crate::valid_track_for_spatial(track, spatial.as_ref()) {
                    return Err(CompilePatchError::InvalidTrack(
                        noon_core::TimelineError::InvalidWorldTransformValues,
                    ));
                }
                reject_geometry_track_on_text(scene, &mut overlay, track)?;
                let replacement = TrackShadow::from_definition(track, object_index);
                overlay.set_track(track.id, Some(replacement));
                validate_camera_track_overlap(scene, &mut overlay, object_index)?;
                if old.property == Property::Presence {
                    validate_presence_channel(scene, &mut overlay, old.object_index)?;
                }
                if replacement.property == Property::Presence
                    && (old.property != Property::Presence
                        || old.object_index != replacement.object_index)
                {
                    validate_presence_channel(scene, &mut overlay, replacement.object_index)?;
                }
            }
            ExecutionPatch::RemoveTrack(id) => {
                let removed = overlay
                    .track(scene, *id)
                    .ok_or(CompilePatchError::UnknownTrack(*id))?;
                overlay.set_track(*id, None);
                if removed.property == Property::Presence {
                    validate_presence_channel(scene, &mut overlay, removed.object_index)?;
                }
            }
            ExecutionPatch::ReconcileTrack {
                track,
                object,
                property,
                end_time,
            } => {
                let mut reconciled = overlay
                    .track(scene, *track)
                    .ok_or(CompilePatchError::UnknownTrack(*track))?;
                if reconciled.reconciled {
                    return Err(CompilePatchError::TrackAlreadyReconciled(*track));
                }
                let object_index = overlay
                    .object_index(scene, *object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                let actual_end = reconciled.reconciliation_end;
                if reconciled.object_index != object_index
                    || reconciled.property != *property
                    || actual_end.total_cmp(end_time) != std::cmp::Ordering::Equal
                {
                    return Err(CompilePatchError::TrackReconciliationMismatch(*track));
                }
                if (reconciled.authored_duration <= 0.0 && reconciled.property != Property::ZIndex)
                    || !matches!(
                        reconciled.property,
                        Property::Position
                            | Property::Rotation
                            | Property::Scale
                            | Property::Fill
                            | Property::Stroke
                            | Property::StrokeWidth
                            | Property::Opacity
                            | Property::Appearance
                            | Property::Reveal
                            | Property::Morph
                            | Property::WorldTransform
                            | Property::CameraProfile
                            | Property::ZIndex
                            | Property::EffectColor
                            | Property::EffectRadius
                            | Property::EffectIntensity
                            | Property::EffectThreshold
                            | Property::EffectOffset
                            | Property::EffectProgress
                            | Property::EffectSoftness
                            | Property::EffectSize
                            | Property::EffectShader(_)
                            | Property::EffectSeed
                    )
                {
                    return Err(CompilePatchError::UnsupportedTrackReconciliation(*track));
                }
                for other in overlay.channel(scene, reconciled.channel()) {
                    if other.id == *track {
                        continue;
                    }
                    let other_end = other.start_time + other.duration;
                    let reconciled_end = reconciled.start_time + reconciled.duration;
                    if reconciled.start_time < other_end && other.start_time < reconciled_end {
                        return Err(CompilePatchError::OverlappingTrackReconciliation {
                            track: *track,
                            other: other.id,
                        });
                    }
                }
                reconciled.reconciled = true;
                overlay.set_track(*track, Some(reconciled));
            }
        }
    }

    Ok(CompiledTransactionPreflightStats {
        composed: scene.preflight_composed_mutations(transaction.mutations())?,
        objects_indexed: overlay.seen_objects.len(),
        tracks_indexed: overlay.seen_tracks.len(),
        track_metadata_visits: overlay.track_metadata_visits,
        mutations_preflighted: transaction.mutations().len(),
        staged_compiled_scene_clones: 0,
    })
}

fn validate_camera_track_overlap(
    scene: &CompiledScene,
    overlay: &mut PreflightOverlay,
    object_index: u32,
) -> Result<(), CompilePatchError> {
    let profiles = overlay.channel(
        scene,
        CompiledChannelKey::new(object_index, Property::CameraProfile),
    );
    let worlds = overlay.channel(
        scene,
        CompiledChannelKey::new(object_index, Property::WorldTransform),
    );
    for profile in &profiles {
        for world in &worlds {
            if profile.start_time < world.start_time + world.duration
                && world.start_time < profile.start_time + profile.duration
            {
                return Err(CompilePatchError::OverlappingTrackReconciliation {
                    track: profile.id,
                    other: world.id,
                });
            }
        }
    }
    Ok(())
}

fn reject_geometry_track_on_text(
    scene: &CompiledScene,
    overlay: &mut PreflightOverlay,
    track: &TrackDefinition,
) -> Result<(), CompilePatchError> {
    if matches!(track.property, Property::Transform | Property::Morph)
        && overlay.object_is_text(scene, track.object) == Some(true)
    {
        return Err(CompilePatchError::GeometryTrackTargetsText {
            track: track.id,
            property: track.property,
        });
    }
    Ok(())
}

fn validate_track(track: &TrackDefinition) -> Result<(), CompilePatchError> {
    validate_track_definition(track).map_err(CompilePatchError::InvalidTrack)?;
    compile_transform_geometry_plan(track).map_err(|error| compile_patch_error(track.id, error))?;
    Ok(())
}

fn presence_endpoints(property: Property, values: &TrackValues) -> Option<(bool, bool)> {
    if property != Property::Presence {
        return None;
    }
    let TrackValues::Bool { from, to } = values else {
        unreachable!("validated Presence track must contain bool values");
    };
    Some((*from, *to))
}

fn validate_presence_channel(
    scene: &CompiledScene,
    overlay: &mut PreflightOverlay,
    object_index: u32,
) -> Result<(), CompilePatchError> {
    let mut tracks = overlay.channel(
        scene,
        CompiledChannelKey::new(object_index, Property::Presence),
    );
    tracks.sort_by(|left, right| {
        left.start_time
            .total_cmp(&right.start_time)
            .then_with(|| left.id.cmp(&right.id))
    });
    for pair in tracks.windows(2) {
        if pair[0].presence.expect("presence channel").1
            != pair[1].presence.expect("presence channel").0
        {
            return Err(CompilePatchError::DiscontinuousPresence {
                previous: pair[0].id,
                next: pair[1].id,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
