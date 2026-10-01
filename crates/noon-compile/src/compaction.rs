//! Explicit reclamation of retired compiled-object rows and resource versions.
//!
//! This is a maintenance barrier, never part of an ordinary execution patch. It
//! deliberately relocates live execution rows, so its caller must rebuild every
//! row-indexed derived consumer under a new execution revision.

use std::collections::{BTreeMap, BTreeSet};

use noon_core::{FontResourceKey, GeometryRef, ObjectContentRef, TrackValues};

use crate::{
    CompiledChannelKey, CompiledScene, CompiledTrack, CompiledTrackLocator, TransformGeometryPlan,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompiledSceneCompactionError {
    /// These projections contain row indices that this bounded maintenance slice
    /// does not yet remap. Rejecting keeps their current execution projection
    /// authoritative instead of quietly dropping it.
    DerivedStatePresent,
}

impl std::fmt::Display for CompiledSceneCompactionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DerivedStatePresent => formatter.write_str(
                "retired-row compaction does not support retained graph, family, or numeric-text state",
            ),
        }
    }
}

impl std::error::Error for CompiledSceneCompactionError {}

/// Instrumentation for the explicitly scheduled retired-row maintenance barrier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompiledSceneCompactionStats {
    pub object_slots_before: usize,
    pub object_slots_after: usize,
    pub object_slots_reclaimed: usize,
    pub track_rows_reindexed: usize,
    pub resource_entries_reclaimed: usize,
}

impl CompiledScene {
    /// Pack live execution rows and release retired object-row history. Plans
    /// without untraversed derived owners also drop unreachable resource versions.
    ///
    /// This intentionally visits retained slots and live tracks. Runtime owners
    /// must use their explicit maintenance barrier to renew execution/frame
    /// revisions before exposing the relocated rows.
    pub fn compact_retired_object_slots(
        &mut self,
    ) -> Result<CompiledSceneCompactionStats, CompiledSceneCompactionError> {
        self.compact_retired_object_slots_with_content_roots(&[])
    }

    /// The runtime may hold effective content whose resource was superseded
    /// in the authored plan. Treat those versions as roots at this explicit
    /// barrier without making the compiled plan own the leases themselves.
    pub fn compact_retired_object_slots_with_content_roots(
        &mut self,
        retained_contents: &[ObjectContentRef],
    ) -> Result<CompiledSceneCompactionStats, CompiledSceneCompactionError> {
        let slots_before = self.objects.len();
        if self.retired_object_indices.is_empty() {
            return Ok(CompiledSceneCompactionStats {
                object_slots_before: slots_before,
                object_slots_after: slots_before,
                resource_entries_reclaimed: self.prune_unreferenced_resources(retained_contents),
                ..CompiledSceneCompactionStats::default()
            });
        }
        if !self.family_animation_plans.is_empty()
            || !self.family_animations.is_empty()
            || !self.graph_edge_dependencies.is_empty()
            || !self.numeric_text_drivers.is_empty()
        {
            return Err(CompiledSceneCompactionError::DerivedStatePresent);
        }

        let mut new_indices = vec![None; self.objects.len()];
        let mut objects = Vec::with_capacity(self.live_object_count);
        for (old_index, object) in self.objects.iter().enumerate() {
            if object.live {
                let new_index = u32::try_from(objects.len())
                    .expect("existing compiled object capacity fits u32");
                new_indices[old_index] = Some(new_index);
                objects.push(object.clone());
            }
        }

        let remap = |old_index: u32| {
            new_indices[old_index as usize]
                .expect("live compiled references never target retired rows")
        };
        let mut tracks = BTreeMap::<CompiledChannelKey, Vec<CompiledTrack>>::new();
        let mut track_locators = BTreeMap::new();
        let mut track_rows_reindexed = 0;
        for track in self.tracks.values().flatten() {
            let mut track = track.clone();
            track.object_index = remap(track.object_index);
            let channel = CompiledChannelKey::new(track.object_index, track.property);
            track_locators.insert(track.id, CompiledTrackLocator::from_track(&track));
            tracks.entry(channel).or_default().push(track);
            track_rows_reindexed += 1;
        }

        let family_order: Vec<u32> = self
            .family_order
            .iter()
            .map(|&index| remap(index))
            .collect();
        let painter_order: Vec<u32> = self
            .painter_order
            .iter()
            .map(|&index| remap(index))
            .collect();
        let mut family_ranks = vec![None; objects.len()];
        for (rank, &index) in family_order.iter().enumerate() {
            family_ranks[index as usize] = Some(rank as u32);
        }
        let mut painter_ranks = vec![None; objects.len()];
        for (rank, &index) in painter_order.iter().enumerate() {
            painter_ranks[index as usize] = Some(rank as u32);
        }
        let object_indices = objects
            .iter()
            .enumerate()
            .map(|(index, object)| (object.id, index as u32))
            .collect();

        self.objects = objects;
        self.live_object_count = self.objects.len();
        self.object_indices = object_indices;
        self.retired_object_indices.clear();
        self.tracks = tracks;
        self.track_locators = track_locators;
        self.family_order = family_order;
        self.family_ranks = family_ranks;
        self.painter_order = painter_order;
        self.painter_ranks = painter_ranks;
        let resource_entries_reclaimed = self.prune_unreferenced_resources(retained_contents);

        Ok(CompiledSceneCompactionStats {
            object_slots_before: slots_before,
            object_slots_after: self.objects.len(),
            object_slots_reclaimed: slots_before - self.objects.len(),
            track_rows_reindexed,
            resource_entries_reclaimed,
        })
    }

    /// Reclaim when every compiled resource owner is a live object, a track, or
    /// an explicit effective-content root. Graph-derived rows and numeric-text
    /// tokens are explicit roots below; family plans still retain their complete
    /// closure until their dependencies can be traversed safely.
    fn prune_unreferenced_resources(&mut self, retained_contents: &[ObjectContentRef]) -> usize {
        if !self.family_animation_plans.is_empty()
            || !self.family_animations.is_empty()
            || (self.resources.images.is_empty()
                && self.resources.texts.is_empty()
                && self.resources.fonts.is_empty()
                && self.resources.geometries.is_empty())
        {
            return 0;
        }

        let mut images = BTreeSet::new();
        let mut texts = BTreeSet::new();
        let mut fonts = BTreeSet::new();
        let mut font_keys = BTreeSet::new();
        let mut geometries = BTreeSet::new();
        let mut geometry_ids = BTreeSet::new();
        // A numeric-text declaration can select any authored token at runtime,
        // so all of its compiled token resources remain roots until the driver
        // is removed. Treat them as ordinary text content here to reuse the
        // same font and vector dependency traversal as live object content.
        let numeric_text_contents = self
            .numeric_text_drivers
            .iter()
            .flat_map(|driver| driver.token_resources.iter())
            .map(|(_, handle)| ObjectContentRef::Text(*handle))
            .collect::<Vec<_>>();
        for content in self
            .objects
            .iter()
            .filter(|object| object.live)
            .map(|object| &object.content)
            .chain(self.graph_authored_content.values())
            .chain(retained_contents.iter())
            .chain(numeric_text_contents.iter())
        {
            match content {
                ObjectContentRef::Image(image) => {
                    if !self.resources.images.contains_key(&image.resource()) {
                        return 0;
                    }
                    images.insert(image.resource());
                }
                ObjectContentRef::Text(handle) => {
                    let Some(resource) = self.resources.texts.get(handle) else {
                        return 0;
                    };
                    texts.insert(*handle);
                    for run in resource.runs.iter() {
                        let key = FontResourceKey::from_face(&run.font);
                        let Some(font) = self.resources.font_handles.get(&key).copied() else {
                            return 0;
                        };
                        if !self.resources.fonts.contains_key(&font) {
                            return 0;
                        }
                        fonts.insert(font);
                        font_keys.insert(key);
                    }
                    for item in resource.vector_items.iter() {
                        if !self.resources.geometries.contains_key(&item.geometry) {
                            return 0;
                        }
                        geometries.insert(item.geometry);
                        let Some(current) = self
                            .resources
                            .geometry_handles
                            .get(&item.geometry.id)
                            .copied()
                        else {
                            return 0;
                        };
                        if !self.resources.geometries.contains_key(&current) {
                            return 0;
                        }
                        geometry_ids.insert(item.geometry.id);
                        geometries.insert(current);
                    }
                }
                ObjectContentRef::Geometry(GeometryRef::External(id)) => {
                    let Some(handle) = self.resources.geometry_handles.get(id).copied() else {
                        return 0;
                    };
                    if !self.resources.geometries.contains_key(&handle) {
                        return 0;
                    }
                    geometry_ids.insert(*id);
                    geometries.insert(handle);
                }
                ObjectContentRef::Geometry(_) => {}
            }
        }

        // Ordinary property tracks carry no resource handles, but transform
        // endpoints and prepared morphs may name external geometry. Resolve
        // those IDs before pruning, just like live object content above.
        for track in self.tracks.values().flatten() {
            let geometry_refs: &[&GeometryRef] = match &track.values {
                TrackValues::Object { from, to } => &[&from.geometry, &to.geometry],
                TrackValues::PreparedMorph { geometry, .. } => &[geometry],
                _ => &[],
            };
            for geometry in geometry_refs.iter().copied().chain(
                track
                    .transform_geometry_plan
                    .as_ref()
                    .and_then(|plan| match plan {
                        TransformGeometryPlan::PathPair { geometry, .. } => Some(geometry.as_ref()),
                        _ => None,
                    }),
            ) {
                let GeometryRef::External(id) = geometry else {
                    continue;
                };
                let Some(handle) = self.resources.geometry_handles.get(id).copied() else {
                    return 0;
                };
                if !self.resources.geometries.contains_key(&handle) {
                    return 0;
                }
                geometry_ids.insert(*id);
                geometries.insert(handle);
            }
        }

        let before = self.resources.images.len()
            + self.resources.texts.len()
            + self.resources.fonts.len()
            + self.resources.geometries.len();
        self.resources
            .images
            .retain(|handle, _| images.contains(handle));
        self.resources
            .texts
            .retain(|handle, _| texts.contains(handle));
        self.resources
            .fonts
            .retain(|handle, _| fonts.contains(handle));
        self.resources
            .font_handles
            .retain(|key, _| font_keys.contains(key));
        self.resources
            .geometries
            .retain(|handle, _| geometries.contains(handle));
        self.resources
            .geometry_handles
            .retain(|id, _| geometry_ids.contains(id));
        before
            - self.resources.images.len()
            - self.resources.texts.len()
            - self.resources.fonts.len()
            - self.resources.geometries.len()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use noon_core::{
        CompositionTimeMap, FontFaceIdentity, FontResource, FontResourceHandle, FontResourceId,
        FontResourceKey, GeometryRef, GeometryResourceArena, GlyphRun, GraphEdgeId,
        ObjectContentRef, ObjectId, Property, RasterImageContentRef, RasterImageResource,
        RasterImageResourceHandle, RasterImageResourceId, RateFunction, Rect, SemanticImageContent,
        Style, TextAffineTransform, TextDirection, TextRenderItem, TextResource,
        TextResourceHandle, TextResourceId, TextSourceKind, TextVectorItem, TextVectorStyle,
        TrackDefinition, TrackId, TrackTiming, TrackValues, Transform2D, TransformTrackEndpoint,
        Vec2, VectorPath,
    };

    use crate::{
        CompiledGraphDependencyDefinition, CompiledGraphDependencyKind, CompiledObject,
        CompiledScene, ExecutionPatch,
    };

    fn circle(id: u64) -> CompiledObject {
        CompiledObject::new(
            ObjectId::new(id),
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        )
    }

    #[test]
    fn static_compaction_prunes_superseded_resources_without_retired_rows() {
        let mut compiled =
            CompiledScene::compile_objects(vec![circle(1), circle(2), circle(3)], &[]).unwrap();
        let mut source_geometries = GeometryResourceArena::new();
        let kept_geometry =
            source_geometries.insert_path(VectorPath::new().move_to(Vec2::ZERO).line_to(Vec2::ONE));
        let obsolete_geometry = source_geometries.insert_path(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(2.0, 2.0)),
        );
        for handle in [kept_geometry, obsolete_geometry] {
            compiled
                .resources
                .geometries
                .insert(handle, source_geometries.get(handle).unwrap().clone());
            compiled
                .resources
                .geometry_handles
                .insert(handle.id, handle);
        }

        let kept_font = FontResourceHandle {
            arena: 1,
            id: FontResourceId::new(1),
            version: 0,
        };
        let obsolete_font = FontResourceHandle {
            arena: 1,
            id: FontResourceId::new(2),
            version: 0,
        };
        let face = FontFaceIdentity {
            family: Arc::from("Test"),
            face_key: Arc::from("kept-face"),
            face_index: 0,
            variation_key: Arc::from(""),
        };
        let kept_font_key = FontResourceKey::from_face(&face);
        for (handle, key) in [
            (kept_font, kept_font_key.clone()),
            (
                obsolete_font,
                FontResourceKey {
                    face_key: Arc::from("obsolete-face"),
                    face_index: 0,
                },
            ),
        ] {
            compiled.resources.font_handles.insert(key.clone(), handle);
            compiled.resources.fonts.insert(
                handle,
                Arc::new(FontResource {
                    key,
                    data: Arc::from([0_u8]),
                }),
            );
        }

        let kept_text = TextResourceHandle {
            arena: 1,
            id: TextResourceId::new(1),
            version: 0,
        };
        let obsolete_text = TextResourceHandle {
            arena: 1,
            id: TextResourceId::new(2),
            version: 0,
        };
        let text = TextResource {
            source: Arc::from(""),
            kind: TextSourceKind::Plain,
            runs: Arc::from([GlyphRun {
                font: face,
                variations: Arc::from([]),
                font_size: 12.0,
                direction: TextDirection::LeftToRight,
                fill: None,
                stroke: None,
                transform: TextAffineTransform::IDENTITY,
                glyphs: Arc::from([]),
            }]),
            vector_items: Arc::from([TextVectorItem {
                geometry: kept_geometry,
                transform: TextAffineTransform::IDENTITY,
                style: TextVectorStyle::default(),
                source_span: None,
                semantic_key: None,
            }]),
            render_items: Arc::from([TextRenderItem::GlyphRun(0), TextRenderItem::Vector(0)]),
            parts: Arc::from([]),
            bounds: Rect::new(Vec2::ZERO, Vec2::ONE),
            baseline: 0.0,
            layout_artifact: None,
        };
        compiled
            .resources
            .texts
            .insert(kept_text, Arc::new(text.clone()));
        compiled
            .resources
            .texts
            .insert(obsolete_text, Arc::new(text));
        compiled.objects[0].content = ObjectContentRef::Text(kept_text);
        compiled.objects[0].text_bounds = Some(Rect::new(Vec2::ZERO, Vec2::ONE));

        let kept_image = RasterImageResourceHandle {
            arena: 1,
            id: RasterImageResourceId::new(1),
            version: 0,
        };
        let obsolete_image = RasterImageResourceHandle {
            arena: 1,
            id: RasterImageResourceId::new(2),
            version: 0,
        };
        let image = Arc::new(RasterImageResource::from_rgba8(1, 1, [255, 0, 0, 255]).unwrap());
        compiled.resources.images.insert(kept_image, image.clone());
        compiled
            .resources
            .images
            .insert(obsolete_image, image.clone());
        compiled.objects[1].content = ObjectContentRef::Image(
            RasterImageContentRef::from_resource(SemanticImageContent::new(kept_image), &image),
        );
        compiled.objects[2].content =
            ObjectContentRef::Geometry(GeometryRef::External(kept_geometry.id));

        let retained_image = ObjectContentRef::Image(RasterImageContentRef::from_resource(
            SemanticImageContent::new(obsolete_image),
            &image,
        ));
        let stats = compiled
            .compact_retired_object_slots_with_content_roots(&[retained_image])
            .unwrap();
        assert_eq!(stats.object_slots_reclaimed, 0);
        assert_eq!(stats.resource_entries_reclaimed, 3);
        assert_eq!(compiled.resources.images.len(), 2);
        assert_eq!(compiled.resources.texts.len(), 1);
        assert_eq!(compiled.resources.fonts.len(), 1);
        assert_eq!(compiled.resources.geometries.len(), 1);
        assert_eq!(
            compiled.resources.geometry_handles.get(&kept_geometry.id),
            Some(&kept_geometry)
        );
        assert!(!compiled
            .resources
            .geometry_handles
            .contains_key(&obsolete_geometry.id));
        assert_eq!(
            compiled
                .compact_retired_object_slots()
                .unwrap()
                .resource_entries_reclaimed,
            1
        );
        assert_eq!(compiled.resources.images.len(), 1);
        assert_eq!(
            compiled
                .compact_retired_object_slots()
                .unwrap()
                .resource_entries_reclaimed,
            0
        );
    }

    #[test]
    fn ordinary_property_track_does_not_pin_unrelated_resource_history() {
        let mut compiled = CompiledScene::compile_objects(
            vec![circle(1)],
            &[TrackDefinition {
                id: TrackId::new(1),
                object: ObjectId::new(1),
                property: Property::Position,
                values: TrackValues::Vec2 {
                    from: Vec2::ZERO,
                    to: Vec2::ONE,
                },
                timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
                time_map: CompositionTimeMap::identity(),
            }],
        )
        .unwrap();
        let mut source = GeometryResourceArena::new();
        let handle = source.insert_path(VectorPath::new().move_to(Vec2::ZERO));
        compiled
            .resources
            .geometries
            .insert(handle, source.get(handle).unwrap().clone());
        compiled
            .resources
            .geometry_handles
            .insert(handle.id, handle);
        let stats = compiled.compact_retired_object_slots().unwrap();
        assert_eq!(stats.resource_entries_reclaimed, 1);
        assert!(compiled.resources.geometries.is_empty());
        assert!(compiled.resources.geometry_handles.is_empty());
    }

    #[test]
    fn numeric_text_tokens_root_their_text_while_unrelated_history_is_pruned() {
        let mut compiled = CompiledScene::compile_objects(vec![circle(1)], &[]).unwrap();
        let retained = TextResourceHandle {
            arena: 1,
            id: TextResourceId::new(1),
            version: 0,
        };
        let obsolete = TextResourceHandle {
            arena: 1,
            id: TextResourceId::new(2),
            version: 0,
        };
        let text = Arc::new(TextResource {
            source: Arc::from("token"),
            kind: TextSourceKind::Plain,
            runs: Arc::from([]),
            vector_items: Arc::from([]),
            render_items: Arc::from([]),
            parts: Arc::from([]),
            bounds: Rect::new(Vec2::ZERO, Vec2::ZERO),
            baseline: 0.0,
            layout_artifact: None,
        });
        compiled.resources.texts.insert(retained, text.clone());
        compiled.resources.texts.insert(obsolete, text);
        compiled
            .numeric_text_drivers
            .push(crate::CompiledNumericTextDriver {
                signal: noon_core::SignalId::new(1),
                object_index: 0,
                format: noon_core::DecimalFormat::default(),
                font_size: 12.0,
                point_to_scene_scale: 1.0,
                token_resources: Arc::from([(Arc::from("token"), retained)]),
            });

        let stats = compiled.compact_retired_object_slots().unwrap();

        assert_eq!(stats.object_slots_reclaimed, 0);
        assert_eq!(stats.resource_entries_reclaimed, 1);
        assert!(compiled.resources.texts.contains_key(&retained));
        assert!(!compiled.resources.texts.contains_key(&obsolete));
    }

    #[test]
    fn graph_derived_row_keeps_its_authored_geometry_root_while_pruning_history() {
        let mut compiled =
            CompiledScene::compile_objects(vec![circle(1), circle(2), circle(3)], &[]).unwrap();
        let mut source = GeometryResourceArena::new();
        let kept = source.insert_path(VectorPath::new().move_to(Vec2::ZERO).line_to(Vec2::ONE));
        let obsolete = source.insert_path(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(2.0, 2.0)),
        );
        for handle in [kept, obsolete] {
            compiled
                .resources
                .geometries
                .insert(handle, source.get(handle).unwrap().clone());
            compiled
                .resources
                .geometry_handles
                .insert(handle.id, handle);
        }
        compiled
            .apply_execution_patch(&ExecutionPatch::SetContent {
                object: ObjectId::new(3),
                content: GeometryRef::External(kept.id).into(),
                text_bounds: None,
            })
            .unwrap();
        let owner = ObjectId::new(100);
        compiled
            .apply_execution_patch(&ExecutionPatch::SetGraphDependencies {
                owner,
                dependencies: vec![CompiledGraphDependencyDefinition {
                    edge: GraphEdgeId::new(10),
                    start_vertex: ObjectId::new(1),
                    end_vertex: ObjectId::new(2),
                    line: ObjectId::new(3),
                    kind: CompiledGraphDependencyKind::Line,
                }],
            })
            .unwrap();
        let stats = compiled.compact_retired_object_slots().unwrap();
        assert_eq!(stats.resource_entries_reclaimed, 1);
        assert!(compiled.resources.geometries.contains_key(&kept));
        assert!(!compiled.resources.geometries.contains_key(&obsolete));
        compiled
            .apply_execution_patch(&ExecutionPatch::SetGraphDependencies {
                owner,
                dependencies: Vec::new(),
            })
            .unwrap();
        let row = compiled.object_index(ObjectId::new(3)).unwrap();
        assert_eq!(
            compiled.objects[row as usize].content,
            GeometryRef::External(kept.id).into()
        );
    }

    #[test]
    fn transform_track_keeps_only_its_external_geometry_dependency() {
        let mut source = GeometryResourceArena::new();
        let kept = source.insert_path(VectorPath::new().move_to(Vec2::ZERO));
        let obsolete = source.insert_path(VectorPath::new().move_to(Vec2::ONE));
        let endpoint = TransformTrackEndpoint::new(GeometryRef::External(kept.id));
        let mut compiled = CompiledScene::compile_objects(
            vec![circle(1)],
            &[TrackDefinition {
                id: TrackId::new(1),
                object: ObjectId::new(1),
                property: Property::Transform,
                values: TrackValues::Object {
                    from: endpoint.clone(),
                    to: endpoint,
                },
                timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
                time_map: CompositionTimeMap::identity(),
            }],
        )
        .unwrap();
        for handle in [kept, obsolete] {
            compiled
                .resources
                .geometries
                .insert(handle, source.get(handle).unwrap().clone());
            compiled
                .resources
                .geometry_handles
                .insert(handle.id, handle);
        }

        let stats = compiled.compact_retired_object_slots().unwrap();
        assert_eq!(stats.resource_entries_reclaimed, 1);
        assert!(compiled.resources.geometries.contains_key(&kept));
        assert!(!compiled.resources.geometries.contains_key(&obsolete));
    }
}
