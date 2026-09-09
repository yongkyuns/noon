//! Compiler from Noon's semantic scene to specialized execution data.

#![forbid(unsafe_code)]

mod execution_patch;
mod semantic_lowering;
mod transaction_preflight;
mod transform;

use std::cmp::Ordering;
use std::{collections::BTreeMap, sync::Arc};

use noon_core::{continuous_time_map_interval, resolve_track_timing};
use noon_core::{
    validate_geometry, validate_style, validate_track_definition, validate_transform,
    CompositionTimeMap, GeometryRef, ObjectId, ObjectStateField, Property, Style, TimelineError,
    TrackDefinition, TrackId, TrackTiming, TrackValues, Transform2D,
};
use noon_core::{
    FontFaceIdentity, FontResource, FontResourceHandle, FontResourceKey, FontResourceLookup,
    GeometryId, GeometryResource, GeometryResourceHandle, GeometryResourceLookup, ObjectContentRef,
    Rect, RetainedFamilyAnimationPlan, SemanticStore, TextResource, TextResourceHandle,
    TextResourceLookup,
};
use transform::{compile_transform_geometry_plan, TransformCompileFailure};

pub use execution_patch::{ExecutionMutationTransaction, ExecutionPatch};
pub use semantic_lowering::*;
pub use transform::TransformGeometryPlan;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DynamicProperties {
    pub presence: bool,
    pub transform: bool,
    pub position: bool,
    pub rotation: bool,
    pub scale: bool,
    pub fill: bool,
    pub stroke: bool,
    pub stroke_width: bool,
    pub opacity: bool,
    pub appearance: bool,
    pub reveal: bool,
    pub morph: bool,
}

impl DynamicProperties {
    fn mark(&mut self, property: Property) {
        match property {
            Property::Presence => self.presence = true,
            Property::Transform => self.transform = true,
            Property::Position => self.position = true,
            Property::Rotation => self.rotation = true,
            Property::Scale => self.scale = true,
            Property::Fill => self.fill = true,
            Property::Stroke => self.stroke = true,
            Property::StrokeWidth => self.stroke_width = true,
            Property::Opacity => self.opacity = true,
            Property::Appearance => self.appearance = true,
            Property::Reveal => self.reveal = true,
            Property::Morph => self.morph = true,
        }
    }

    pub const fn any(self) -> bool {
        self.presence
            || self.transform
            || self.position
            || self.rotation
            || self.scale
            || self.fill
            || self.stroke
            || self.stroke_width
            || self.opacity
            || self.appearance
            || self.reveal
            || self.morph
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledObject {
    pub id: ObjectId,
    pub content: ObjectContentRef,
    /// Immutable local bounds for resource-backed text; geometry bounds remain derived.
    pub text_bounds: Option<Rect>,
    pub base_transform: Transform2D,
    pub base_style: Style,
    pub dynamic: DynamicProperties,
    /// Whether this stable compiled slot currently contains a live scene object.
    /// Removed objects leave tombstones so unrelated slot numbers never change.
    pub live: bool,
}

impl CompiledObject {
    pub fn new(
        id: ObjectId,
        content: impl Into<ObjectContentRef>,
        base_transform: Transform2D,
        base_style: Style,
    ) -> Self {
        Self {
            id,
            content: content.into(),
            text_bounds: None,
            base_transform,
            base_style,
            dynamic: DynamicProperties::default(),
            live: true,
        }
    }

    pub fn geometry(&self) -> Option<&GeometryRef> {
        self.content.geometry()
    }

    pub const fn text(&self) -> Option<TextResourceHandle> {
        self.content.text()
    }
}

/// Dependency-closed immutable resources retained by one compiled execution plan.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompiledResources {
    texts: BTreeMap<TextResourceHandle, Arc<TextResource>>,
    fonts: BTreeMap<FontResourceHandle, Arc<FontResource>>,
    font_handles: BTreeMap<FontResourceKey, FontResourceHandle>,
    geometries: BTreeMap<GeometryResourceHandle, GeometryResource>,
    geometry_handles: BTreeMap<GeometryId, GeometryResourceHandle>,
}

impl TextResourceLookup for CompiledResources {
    fn get(&self, handle: TextResourceHandle) -> Option<&TextResource> {
        self.texts.get(&handle).map(Arc::as_ref)
    }
}

impl FontResourceLookup for CompiledResources {
    fn handle_for_face(&self, face: &FontFaceIdentity) -> Option<FontResourceHandle> {
        self.font_handles
            .get(&FontResourceKey::from_face(face))
            .copied()
    }

    fn get(&self, handle: FontResourceHandle) -> Option<&FontResource> {
        self.fonts.get(&handle).map(Arc::as_ref)
    }
}

impl GeometryResourceLookup for CompiledResources {
    fn current_handle(&self, id: GeometryId) -> Option<GeometryResourceHandle> {
        self.geometry_handles.get(&id).copied()
    }

    fn get(&self, handle: GeometryResourceHandle) -> Option<&GeometryResource> {
        self.geometries.get(&handle)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompiledResourceError {
    MissingText(TextResourceHandle),
    MissingFont(FontResourceKey),
    MissingGeometry(GeometryResourceHandle),
}

impl std::fmt::Display for CompiledResourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingText(handle) => write!(
                formatter,
                "missing text resource {}@{}",
                handle.id.get(),
                handle.version
            ),
            Self::MissingFont(key) => write!(
                formatter,
                "missing font resource {}#{}",
                key.face_key, key.face_index
            ),
            Self::MissingGeometry(handle) => write!(
                formatter,
                "missing geometry resource {}@{}",
                handle.id.get(),
                handle.version
            ),
        }
    }
}

impl std::error::Error for CompiledResourceError {}

impl CompiledResources {
    pub fn text_count(&self) -> usize {
        self.texts.len()
    }

    pub fn font_count(&self) -> usize {
        self.fonts.len()
    }

    pub fn geometry_count(&self) -> usize {
        self.geometries.len()
    }

    /// Install an already-validated sparse dependency closure.
    ///
    /// Handles are immutable resource identities, so insertion is infallible after
    /// preparation has resolved every dependency against the owning semantic store.
    pub(crate) fn merge(&mut self, additions: Self) {
        self.texts.extend(additions.texts);
        self.fonts.extend(additions.fonts);
        self.font_handles.extend(additions.font_handles);
        self.geometries.extend(additions.geometries);
        self.geometry_handles.extend(additions.geometry_handles);
    }

    pub(crate) fn capture_text(
        &mut self,
        store: &SemanticStore,
        handle: TextResourceHandle,
    ) -> Result<Rect, CompiledResourceError> {
        if let Some(resource) = self.texts.get(&handle) {
            return Ok(resource.bounds);
        }
        let resource = store
            .text_resources()
            .get_shared(handle)
            .ok_or(CompiledResourceError::MissingText(handle))?;

        for run in resource.runs.iter() {
            let key = FontResourceKey::from_face(&run.font);
            let font_handle = store
                .font_resources()
                .handle_for_face(&run.font)
                .ok_or_else(|| CompiledResourceError::MissingFont(key.clone()))?;
            let font = store
                .font_resources()
                .get_shared(font_handle)
                .ok_or_else(|| CompiledResourceError::MissingFont(key.clone()))?;
            self.font_handles.insert(key, font_handle);
            self.fonts.insert(font_handle, font);
        }
        for item in resource.vector_items.iter() {
            let geometry = store
                .geometry_resources()
                .get(item.geometry)
                .cloned()
                .ok_or(CompiledResourceError::MissingGeometry(item.geometry))?;
            self.geometry_handles
                .insert(item.geometry.id, item.geometry);
            self.geometries.insert(item.geometry, geometry);
        }

        let bounds = resource.bounds;
        self.texts.insert(handle, resource);
        Ok(bounds)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledTrack {
    pub id: TrackId,
    pub object_index: u32,
    pub property: Property,
    pub values: TrackValues,
    pub timing: TrackTiming,
    pub time_map: CompositionTimeMap,
    pub transform_geometry_plan: Option<TransformGeometryPlan>,
    /// Completion reconciled this driver's endpoint into authored base state.
    /// Its payload remains available for deterministic evaluation before the end.
    pub reconciled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompiledChannelKey {
    pub object_index: u32,
    pub property: Property,
}

impl CompiledChannelKey {
    pub const fn new(object_index: u32, property: Property) -> Self {
        Self {
            object_index,
            property,
        }
    }
}

impl PartialOrd for CompiledChannelKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CompiledChannelKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.object_index
            .cmp(&other.object_index)
            .then_with(|| property_rank(self.property).cmp(&property_rank(other.property)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CompiledTrackLocator {
    object_index: u32,
    property: Property,
    start_time: f64,
    id: TrackId,
}

impl CompiledTrackLocator {
    fn from_track(track: &CompiledTrack) -> Self {
        Self {
            object_index: track.object_index,
            property: track.property,
            start_time: track.timing.start_time,
            id: track.id,
        }
    }
}

/// Instrumentation for one compiled-scene patch.
///
/// Timeline edits intentionally report dense-vector shifts separately from semantic work:
/// this slice removes full track payload clones and global dynamic sweeps, while the
/// remaining dense storage migration is tracked by #58.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompiledPatchStats {
    pub track_vector_clones: usize,
    pub presence_tracks_inspected: usize,
    pub dynamic_objects_recomputed: usize,
    pub dynamic_tracks_inspected: usize,
    /// Entries shifted inside an affected object/property channel only.
    pub dense_track_slots_shifted: usize,
    /// Global/unrelated track payload movement. This must remain zero for local edits.
    pub unrelated_track_slots_shifted: usize,
    pub object_slots_appended: usize,
    pub object_slots_reactivated: usize,
    pub object_slots_retired: usize,
    pub object_indices_rewritten: usize,
    pub track_object_indices_rewritten: usize,
    pub track_locators_removed: usize,
    pub tracks_reconciled: usize,
}

/// Lightweight validation accounting for an atomic compiled-scene transaction.
/// Existing geometry/track payloads are never cloned for staging.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompiledTransactionPreflightStats {
    /// Existing object identities resolved through the sparse transaction overlay.
    pub objects_indexed: usize,
    /// Existing track identities resolved through the sparse transaction overlay.
    pub tracks_indexed: usize,
    /// Track lookup/iteration work, including repeated visits to affected channels.
    pub track_metadata_visits: usize,
    pub mutations_preflighted: usize,
    pub staged_compiled_scene_clones: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledScene {
    // Stable slot storage. Removal tombstones a slot instead of shifting it; re-entry
    // of the same execution identity revives that row in place. A future compaction
    // generation may reclaim other retired capacity off the live-edit path.
    objects: Vec<CompiledObject>,
    live_object_count: usize,
    /// Tracks are segmented by stable execution channel. Mutating one channel never
    /// relocates payloads belonging to another channel. Each channel vector remains
    /// sorted by start time and TrackId for deterministic evaluation.
    tracks: BTreeMap<CompiledChannelKey, Vec<CompiledTrack>>,
    track_count: usize,
    object_indices: BTreeMap<ObjectId, u32>,
    retired_object_indices: BTreeMap<ObjectId, u32>,
    /// Live stable row indices in authoritative semantic painter order.
    painter_order: Vec<u32>,
    painter_ranks: Vec<Option<u32>>,
    track_locators: BTreeMap<TrackId, CompiledTrackLocator>,
    family_animation_plans: Vec<RetainedFamilyAnimationPlan>,
    family_animations: Vec<CompiledFamilyAnimationChannel>,
    resources: CompiledResources,
}

#[derive(Clone, Copy, Debug)]
pub struct CompiledTracks<'a> {
    channels: &'a BTreeMap<CompiledChannelKey, Vec<CompiledTrack>>,
}

impl<'a> CompiledTracks<'a> {
    pub fn iter(self) -> impl Iterator<Item = &'a CompiledTrack> + 'a {
        self.channels.values().flat_map(|tracks| tracks.iter())
    }

    pub fn len(self) -> usize {
        self.channels.values().map(Vec::len).sum()
    }

    pub fn is_empty(self) -> bool {
        self.channels.is_empty()
    }

    pub fn to_vec(self) -> Vec<CompiledTrack> {
        self.iter().cloned().collect()
    }
}

impl std::ops::Index<usize> for CompiledTracks<'_> {
    type Output = CompiledTrack;

    fn index(&self, index: usize) -> &Self::Output {
        self.channels
            .values()
            .flat_map(|tracks| tracks.iter())
            .nth(index)
            .expect("compiled track index out of bounds")
    }
}

impl PartialEq<Vec<CompiledTrack>> for CompiledTracks<'_> {
    fn eq(&self, other: &Vec<CompiledTrack>) -> bool {
        self.channels
            .values()
            .flat_map(|tracks| tracks.iter())
            .eq(other.iter())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CompileError {
    TooManyObjects(usize),
    DuplicateObject(ObjectId),
    UnknownObject(ObjectId),
    InvalidTrack(TimelineError),
    GeometryTrackTargetsText { track: TrackId, property: Property },
    DiscontinuousPresence { previous: TrackId, next: TrackId },
    UnsupportedTransformGeometry(TrackId),
    PathTransformRequiresRetessellation(TrackId),
    UnsafeFilledPathTransform(TrackId),
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyObjects(count) => {
                write!(formatter, "scene contains too many objects: {count}")
            }
            Self::UnknownObject(id) => {
                write!(formatter, "track references unknown object {}", id.get())
            }
            Self::DuplicateObject(id) => write!(formatter, "duplicate object id {}", id.get()),
            Self::InvalidTrack(error) => write!(formatter, "invalid track: {error}"),
            Self::GeometryTrackTargetsText { track, property } => write!(
                formatter,
                "geometry-only {property:?} track {} cannot target text content",
                track.get()
            ),
            Self::DiscontinuousPresence { previous, next } => write!(
                formatter,
                "presence track {} does not hand off continuously to track {}",
                previous.get(),
                next.get()
            ),
            Self::UnsupportedTransformGeometry(id) => write!(
                formatter,
                "transform track {} uses unsupported geometry interpolation",
                id.get()
            ),
            Self::PathTransformRequiresRetessellation(id) => write!(
                formatter,
                "transform track {} changes path fill presence, stroke topology, or stroke width",
                id.get()
            ),
            Self::UnsafeFilledPathTransform(id) => write!(
                formatter,
                "transform track {} uses filled path geometry without a stable fixed triangulation",
                id.get()
            ),
        }
    }
}

impl std::error::Error for CompileError {}

#[derive(Clone, Debug, PartialEq)]
pub enum CompilePatchError {
    TooManyObjects(usize),
    TooManyFamilyAnimations,
    InvalidFamilyAnimation,
    DuplicateObject(ObjectId),
    UnknownObject(ObjectId),
    DuplicateTrack(TrackId),
    UnknownTrack(TrackId),
    TrackAlreadyReconciled(TrackId),
    TrackReconciliationMismatch(TrackId),
    UnsupportedTrackReconciliation(TrackId),
    OverlappingTrackReconciliation {
        track: TrackId,
        other: TrackId,
    },
    InvalidObjectState {
        object: ObjectId,
        field: ObjectStateField,
    },
    InvalidTrack(TimelineError),
    GeometryTrackTargetsText {
        track: TrackId,
        property: Property,
    },
    ContentReplacementHasGeometryDriver {
        object: ObjectId,
        track: TrackId,
        property: Property,
    },
    TextBoundsMismatch {
        object: ObjectId,
        resource: TextResourceHandle,
    },
    InvalidContentBounds(ObjectId),
    Resource(CompiledResourceError),
    DiscontinuousPresence {
        previous: TrackId,
        next: TrackId,
    },
    UnsupportedTransformGeometry(TrackId),
    PathTransformRequiresRetessellation(TrackId),
    UnsafeFilledPathTransform(TrackId),
}

impl std::fmt::Display for CompilePatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyObjects(count) => {
                write!(formatter, "scene contains too many objects: {count}")
            }
            Self::TooManyFamilyAnimations => {
                formatter.write_str("scene contains too many family animation plans")
            }
            Self::InvalidFamilyAnimation => {
                formatter.write_str("invalid family animation plan, timing, or mapping")
            }
            Self::DuplicateObject(id) => write!(formatter, "duplicate object id {}", id.get()),
            Self::UnknownObject(id) => write!(formatter, "unknown object id {}", id.get()),
            Self::DuplicateTrack(id) => write!(formatter, "duplicate track id {}", id.get()),
            Self::UnknownTrack(id) => write!(formatter, "unknown track id {}", id.get()),
            Self::TrackAlreadyReconciled(id) => {
                write!(formatter, "track {} was already reconciled", id.get())
            }
            Self::TrackReconciliationMismatch(id) => write!(
                formatter,
                "track {} does not match its completion reconciliation metadata",
                id.get()
            ),
            Self::UnsupportedTrackReconciliation(id) => write!(
                formatter,
                "track {} cannot use affine completion reconciliation",
                id.get()
            ),
            Self::OverlappingTrackReconciliation { track, other } => write!(
                formatter,
                "track {} overlaps track {} on its completion channel",
                track.get(),
                other.get()
            ),
            Self::InvalidObjectState { object, field } => write!(
                formatter,
                "object {} contains non-finite {field} state",
                object.get()
            ),
            Self::InvalidTrack(error) => write!(formatter, "invalid track: {error}"),
            Self::GeometryTrackTargetsText { track, property } => write!(
                formatter,
                "geometry-only {property:?} track {} cannot target text content",
                track.get()
            ),
            Self::ContentReplacementHasGeometryDriver {
                object,
                track,
                property,
            } => write!(
                formatter,
                "content replacement for object {} is unsupported while geometry-only {property:?} track {} is installed",
                object.get(),
                track.get()
            ),
            Self::TextBoundsMismatch { object, resource } => write!(
                formatter,
                "content replacement for object {} carries bounds that differ from text resource {}@{}",
                object.get(),
                resource.id.get(),
                resource.version
            ),
            Self::InvalidContentBounds(object) => write!(
                formatter,
                "object {} content carries invalid or mismatched text bounds",
                object.get()
            ),
            Self::Resource(error) => write!(formatter, "content replacement resource failed: {error}"),
            Self::DiscontinuousPresence { previous, next } => write!(
                formatter,
                "presence track {} does not hand off continuously to track {}",
                previous.get(),
                next.get()
            ),
            Self::UnsupportedTransformGeometry(id) => write!(
                formatter,
                "transform track {} uses unsupported geometry interpolation",
                id.get()
            ),
            Self::PathTransformRequiresRetessellation(id) => write!(
                formatter,
                "transform track {} changes path fill presence, stroke topology, or stroke width",
                id.get()
            ),
            Self::UnsafeFilledPathTransform(id) => write!(
                formatter,
                "transform track {} uses filled path geometry without a stable fixed triangulation",
                id.get()
            ),
        }
    }
}

impl std::error::Error for CompilePatchError {}

impl CompiledScene {
    /// Validate the bounded affine completion policy for newly activated tracks.
    /// Existing and candidate tracks are inspected only in affected channels.
    /// Mapped composition leaves retain the root interval for completion, while
    /// overlap checks use their mapped active intervals. Completion still reconciles
    /// at the root endpoint, where runtime finish semantics settle every leaf to its
    /// authored target independently of the map's ordinary alpha-at-one sample.
    pub fn preflight_reconcilable_track_additions(
        &self,
        tracks: &[TrackDefinition],
    ) -> Result<(), CompilePatchError> {
        let mut candidates = BTreeMap::<CompiledChannelKey, Vec<&TrackDefinition>>::new();
        for track in tracks {
            validate_track_definition(track).map_err(CompilePatchError::InvalidTrack)?;
            if track.timing.duration <= 0.0
                || !matches!(
                    track.property,
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
                )
            {
                return Err(CompilePatchError::UnsupportedTrackReconciliation(track.id));
            }
            let object_index = self
                .object_index(track.object)
                .ok_or(CompilePatchError::UnknownObject(track.object))?;
            candidates
                .entry(CompiledChannelKey::new(object_index, track.property))
                .or_default()
                .push(track);
        }
        for (channel, candidate_tracks) in candidates {
            for (candidate_index, candidate) in candidate_tracks.iter().enumerate() {
                let (start, end) =
                    continuous_time_map_interval(candidate.timing, &candidate.time_map)
                        .expect("validated candidate track has a valid composition time map");
                for existing in self.channel_tracks(channel) {
                    let (existing_start, existing_end) =
                        continuous_time_map_interval(existing.timing, &existing.time_map)
                            .expect("compiled track has a valid composition time map");
                    if start < existing_end && existing_start < end {
                        return Err(CompilePatchError::OverlappingTrackReconciliation {
                            track: candidate.id,
                            other: existing.id,
                        });
                    }
                }
                for other in &candidate_tracks[..candidate_index] {
                    let (other_start, other_end) =
                        continuous_time_map_interval(other.timing, &other.time_map)
                            .expect("validated candidate track has a valid composition time map");
                    if start < other_end && other_start < end {
                        return Err(CompilePatchError::OverlappingTrackReconciliation {
                            track: candidate.id,
                            other: other.id,
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Compile geometry and text into the same stable execution-slot domain.
    pub fn compile_objects(
        source_objects: Vec<CompiledObject>,
        source_tracks: &[TrackDefinition],
    ) -> Result<Self, CompileError> {
        let mut object_indices = BTreeMap::new();
        let object_count = source_objects.len();
        let mut objects = Vec::with_capacity(object_count);

        for (index, mut object) in source_objects.into_iter().enumerate() {
            let index =
                u32::try_from(index).map_err(|_| CompileError::TooManyObjects(object_count))?;
            if object_indices.insert(object.id, index).is_some() {
                return Err(CompileError::DuplicateObject(object.id));
            }
            object.dynamic = DynamicProperties::default();
            object.live = true;
            objects.push(object);
        }

        let mut tracks = Vec::with_capacity(source_tracks.len());
        for track in source_tracks {
            let object_index = *object_indices
                .get(&track.object)
                .ok_or(CompileError::UnknownObject(track.object))?;
            validate_track_definition(track).map_err(CompileError::InvalidTrack)?;
            reject_geometry_track_on_text(&objects[object_index as usize], track).map_err(
                |(track, property)| CompileError::GeometryTrackTargetsText { track, property },
            )?;
            objects[object_index as usize].dynamic.mark(track.property);
            tracks.push(
                compile_track(track, object_index)
                    .map_err(|error| compile_error(track.id, error))?,
            );
        }
        sort_tracks(&mut tracks);
        validate_presence_chains(&tracks)
            .map_err(|(previous, next)| CompileError::DiscontinuousPresence { previous, next })?;
        let track_locators = tracks
            .iter()
            .map(|track| (track.id, CompiledTrackLocator::from_track(track)))
            .collect();
        let track_count = tracks.len();
        let mut tracks_by_channel = BTreeMap::<CompiledChannelKey, Vec<CompiledTrack>>::new();
        for track in tracks {
            tracks_by_channel
                .entry(CompiledChannelKey::new(track.object_index, track.property))
                .or_default()
                .push(track);
        }

        let live_object_count = objects.len();
        Ok(Self {
            painter_order: (0..objects.len() as u32).collect(),
            painter_ranks: (0..objects.len() as u32).map(Some).collect(),
            objects,
            live_object_count,
            tracks: tracks_by_channel,
            track_count,
            object_indices,
            retired_object_indices: BTreeMap::new(),
            track_locators,
            family_animation_plans: Vec::new(),
            family_animations: Vec::new(),
            resources: CompiledResources::default(),
        })
    }

    /// Stable compiled object slots. Retired slots remain in this slice and have `live == false`.
    pub fn objects(&self) -> &[CompiledObject] {
        &self.objects
    }

    pub const fn live_object_count(&self) -> usize {
        self.live_object_count
    }

    pub fn painter_order(&self) -> &[u32] {
        &self.painter_order
    }

    pub fn painter_position(&self, object: ObjectId) -> Option<usize> {
        let index = self.object_index(object)?;
        self.painter_rank(index).map(|rank| rank as usize)
    }

    pub fn painter_rank(&self, object_index: u32) -> Option<u32> {
        self.painter_ranks
            .get(object_index as usize)
            .copied()
            .flatten()
    }

    fn painter_reorder_changes(&self, object: ObjectId, before: Option<ObjectId>) -> bool {
        let Some(current) = self.painter_position(object) else {
            return true;
        };
        let destination = match before {
            Some(anchor) if anchor == object => return false,
            Some(anchor) => {
                let Some(anchor) = self.painter_position(anchor) else {
                    return true;
                };
                anchor.saturating_sub(usize::from(current < anchor))
            }
            None => self.painter_order.len().saturating_sub(1),
        };
        current != destination
    }

    pub const fn resources(&self) -> &CompiledResources {
        &self.resources
    }

    pub fn family_animation_plans(&self) -> &[RetainedFamilyAnimationPlan] {
        &self.family_animation_plans
    }

    pub fn family_animations(&self) -> &[CompiledFamilyAnimationChannel] {
        &self.family_animations
    }

    pub fn text_resources(&self) -> &impl TextResourceLookup {
        &self.resources
    }

    pub fn font_resources(&self) -> &impl FontResourceLookup {
        &self.resources
    }

    pub fn geometry_resources(&self) -> &impl GeometryResourceLookup {
        &self.resources
    }

    pub fn object_slot_is_live(&self, object_index: u32) -> bool {
        self.objects
            .get(object_index as usize)
            .is_some_and(|object| object.live)
    }

    pub fn object_id_at_slot(&self, object_index: u32) -> Option<ObjectId> {
        let object = self.objects.get(object_index as usize)?;
        object.live.then_some(object.id)
    }

    pub fn object_channels(&self, id: ObjectId) -> Vec<CompiledChannelKey> {
        let Some(object_index) = self.object_index(id) else {
            return Vec::new();
        };
        self.channels_for_object_index(object_index).collect()
    }

    pub fn track_object(&self, id: TrackId) -> Option<ObjectId> {
        let locator = self.track_locators.get(&id)?;
        self.object_id_at_slot(locator.object_index)
    }

    /// Snapshot tracks in deterministic runtime order. This compatibility accessor
    /// clones payloads; hot compiler/runtime paths should use `tracks_iter` or
    /// `channel_tracks` instead.
    pub fn tracks(&self) -> CompiledTracks<'_> {
        CompiledTracks {
            channels: &self.tracks,
        }
    }

    pub fn tracks_iter(&self) -> impl Iterator<Item = &CompiledTrack> {
        self.tracks.values().flat_map(|tracks| tracks.iter())
    }

    pub fn channels(&self) -> impl Iterator<Item = CompiledChannelKey> + '_ {
        self.tracks.keys().copied()
    }

    pub const fn track_count(&self) -> usize {
        self.track_count
    }

    pub fn track(&self, id: TrackId) -> Option<&CompiledTrack> {
        let locator = *self.track_locators.get(&id)?;
        let channel = CompiledChannelKey::new(locator.object_index, locator.property);
        let tracks = self.tracks.get(&channel)?;
        let position = tracks
            .binary_search_by(|track| compare_track_locator(track, locator))
            .ok()?;
        tracks.get(position)
    }

    pub fn object_index(&self, id: ObjectId) -> Option<u32> {
        self.object_indices.get(&id).copied()
    }

    pub fn channel_for_track(&self, id: TrackId) -> Option<CompiledChannelKey> {
        let locator = self.track_locators.get(&id)?;
        Some(CompiledChannelKey::new(
            locator.object_index,
            locator.property,
        ))
    }

    pub fn channel_tracks(&self, channel: CompiledChannelKey) -> &[CompiledTrack] {
        self.tracks.get(&channel).map_or(&[], Vec::as_slice)
    }

    pub fn has_channel(&self, channel: CompiledChannelKey) -> bool {
        self.tracks.contains_key(&channel)
    }

    fn channels_for_object_index(
        &self,
        object_index: u32,
    ) -> impl Iterator<Item = CompiledChannelKey> + '_ {
        let start = CompiledChannelKey::new(object_index, Property::Presence);
        let end = CompiledChannelKey::new(object_index, Property::Morph);
        self.tracks.range(start..=end).map(|(channel, _)| *channel)
    }

    /// Validate a mutation transaction using only stable identity/channel metadata.
    /// Incoming track payloads are validated individually, but existing compiled
    /// scene payloads are never cloned.
    pub fn preflight_execution_transaction(
        &self,
        transaction: &ExecutionMutationTransaction,
    ) -> Result<CompiledTransactionPreflightStats, CompilePatchError> {
        transaction_preflight::preflight_transaction(self, transaction)
    }

    pub fn preflight_execution_transaction_with_resources(
        &self,
        transaction: &ExecutionMutationTransaction,
        additions: &CompiledResources,
    ) -> Result<CompiledTransactionPreflightStats, CompilePatchError> {
        transaction_preflight::preflight_transaction_with_resources(self, transaction, additions)
    }

    pub fn merge_prepared_resources(&mut self, additions: CompiledResources) {
        self.resources.merge(additions);
    }

    /// Validate append-only structural capacity before transaction-local semantic
    /// identities are promoted. Removed rows remain tombstones, so only appended
    /// objects affect this bound.
    pub fn preflight_object_appends(
        &self,
        additional_objects: usize,
    ) -> Result<(), CompilePatchError> {
        let count = self
            .objects
            .len()
            .checked_add(additional_objects)
            .ok_or(CompilePatchError::TooManyObjects(usize::MAX))?;
        if count != 0 && u32::try_from(count - 1).is_err() {
            return Err(CompilePatchError::TooManyObjects(count));
        }
        Ok(())
    }

    pub fn apply_execution_patch(
        &mut self,
        patch: &ExecutionPatch,
    ) -> Result<(), CompilePatchError> {
        self.apply_execution_patch_with_stats(patch).map(|_| ())
    }

    /// Whether a preflighted patch changes the current executable projection.
    ///
    /// This compares only the addressed object/track. Callers must still preflight
    /// the complete transaction, including redundant writes, before skipping work.
    pub fn patch_changes_execution(&self, patch: &ExecutionPatch) -> bool {
        match patch {
            ExecutionPatch::SetContent {
                object,
                content,
                text_bounds,
            } => self.object_index(*object).is_none_or(|index| {
                let existing = &self.objects[index as usize];
                &existing.content != content || existing.text_bounds != *text_bounds
            }),
            ExecutionPatch::SetTransform { object, transform } => self
                .object_index(*object)
                .is_none_or(|index| self.objects[index as usize].base_transform != *transform),
            ExecutionPatch::SetStyle { object, style } => self
                .object_index(*object)
                .is_none_or(|index| self.objects[index as usize].base_style != *style),
            ExecutionPatch::ReplaceTrack(track) => self.track(track.id).is_none_or(|existing| {
                self.object_index(track.object) != Some(existing.object_index)
                    || existing.property != track.property
                    || existing.values != track.values
                    || existing.timing != track.timing
                    || existing.time_map != track.time_map
            }),
            ExecutionPatch::ReconcileTrack { track, .. } => self
                .track(*track)
                .is_none_or(|existing| !existing.reconciled),
            ExecutionPatch::ReorderObject { object, before } => {
                self.painter_reorder_changes(*object, *before)
            }
            ExecutionPatch::CreateObject(_)
            | ExecutionPatch::RemoveObject(_)
            | ExecutionPatch::AddTrack(_)
            | ExecutionPatch::AddFamilyAnimation(_)
            | ExecutionPatch::RemoveTrack(_) => true,
        }
    }

    pub fn apply_execution_patch_with_stats(
        &mut self,
        patch: &ExecutionPatch,
    ) -> Result<CompiledPatchStats, CompilePatchError> {
        let mut stats = CompiledPatchStats::default();
        match patch {
            ExecutionPatch::CreateObject(object) => {
                if self.object_indices.contains_key(&object.id) {
                    return Err(CompilePatchError::DuplicateObject(object.id));
                }
                validate_compiled_object(object)?;
                validate_execution_content_resource(
                    &self.resources,
                    None,
                    object.id,
                    &object.content,
                    object.text_bounds,
                )?;
                let mut object = object.clone();
                object.dynamic = DynamicProperties::default();
                object.live = true;
                if let Some(index) = self.retired_object_indices.remove(&object.id) {
                    self.objects[index as usize] = object;
                    self.object_indices
                        .insert(self.objects[index as usize].id, index);
                    self.live_object_count += 1;
                    self.painter_order.push(index);
                    self.painter_ranks[index as usize] = Some(self.painter_order.len() as u32 - 1);
                    stats.object_slots_reactivated = 1;
                    return Ok(stats);
                }
                let index = u32::try_from(self.objects.len())
                    .map_err(|_| CompilePatchError::TooManyObjects(self.objects.len()))?;
                self.object_indices.insert(object.id, index);
                self.objects.push(object);
                self.live_object_count += 1;
                self.painter_order.push(index);
                self.painter_ranks
                    .push(Some(self.painter_order.len() as u32 - 1));
                stats.object_slots_appended = 1;
            }
            ExecutionPatch::RemoveObject(id) => {
                let index = self
                    .object_index(*id)
                    .ok_or(CompilePatchError::UnknownObject(*id))?;
                let channels: Vec<_> = self.channels_for_object_index(index).collect();
                for channel in channels {
                    if let Some(removed) = self.tracks.remove(&channel) {
                        self.track_count -= removed.len();
                        for track in removed {
                            self.track_locators.remove(&track.id);
                            stats.track_locators_removed += 1;
                        }
                    }
                }

                let object = &mut self.objects[index as usize];
                debug_assert!(object.live);
                object.live = false;
                object.dynamic = DynamicProperties::default();
                self.object_indices.remove(id);
                self.retired_object_indices.insert(*id, index);
                self.live_object_count -= 1;
                let painter_position = self.painter_ranks[index as usize]
                    .take()
                    .expect("live object has one painter rank")
                    as usize;
                self.painter_order.remove(painter_position);
                for rank in painter_position..self.painter_order.len() {
                    self.painter_ranks[self.painter_order[rank] as usize] = Some(rank as u32);
                }
                stats.object_slots_retired = 1;
                // No unrelated object or track payload changes storage location.
                stats.object_indices_rewritten = 0;
                stats.track_object_indices_rewritten = 0;
                stats.unrelated_track_slots_shifted = 0;
            }
            ExecutionPatch::ReorderObject { object, before } => {
                let index = self
                    .object_index(*object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                let before_index = before
                    .map(|before| {
                        self.object_index(before)
                            .ok_or(CompilePatchError::UnknownObject(before))
                    })
                    .transpose()?;
                if before_index == Some(index) {
                    return Ok(stats);
                }
                let position = self
                    .painter_order
                    .iter()
                    .position(|candidate| *candidate == index)
                    .expect("live object has one painter-order entry");
                self.painter_order.remove(position);
                let destination = before_index
                    .and_then(|anchor| {
                        self.painter_order
                            .iter()
                            .position(|candidate| *candidate == anchor)
                    })
                    .unwrap_or(self.painter_order.len());
                self.painter_order.insert(destination, index);
                let first = position.min(destination);
                let last = position.max(destination).min(self.painter_order.len() - 1);
                for rank in first..=last {
                    self.painter_ranks[self.painter_order[rank] as usize] = Some(rank as u32);
                }
            }
            ExecutionPatch::SetContent {
                object,
                content,
                text_bounds,
            } => {
                let index = self
                    .object_index(*object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                validate_execution_content(*object, content, *text_bounds)?;
                validate_execution_content_resource(
                    &self.resources,
                    None,
                    *object,
                    content,
                    *text_bounds,
                )?;
                for property in [Property::Transform, Property::Morph] {
                    let channel = CompiledChannelKey::new(index, property);
                    if let Some(track) = self
                        .channel_tracks(channel)
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
                self.objects[index as usize].content = content.clone();
                self.objects[index as usize].text_bounds = *text_bounds;
            }
            ExecutionPatch::SetTransform { object, transform } => {
                let index = self
                    .object_index(*object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                validate_transform(*object, *transform).map_err(map_object_state_error)?;
                self.objects[index as usize].base_transform = *transform;
            }
            ExecutionPatch::SetStyle { object, style } => {
                let index = self
                    .object_index(*object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                validate_style(*object, *style).map_err(map_object_state_error)?;
                self.objects[index as usize].base_style = *style;
            }
            ExecutionPatch::AddTrack(track) => {
                if self.track_locators.contains_key(&track.id) {
                    return Err(CompilePatchError::DuplicateTrack(track.id));
                }
                let compiled = self.compile_patch_track(track)?;
                stats.presence_tracks_inspected +=
                    self.validate_presence_edit(None, Some(&compiled))?;
                let locator = CompiledTrackLocator::from_track(&compiled);
                let channel = CompiledChannelKey::new(locator.object_index, locator.property);
                let channel_tracks = self.tracks.entry(channel).or_default();
                let position = track_insertion_position(channel_tracks, &compiled);
                stats.dense_track_slots_shifted = channel_tracks.len().saturating_sub(position);
                stats.unrelated_track_slots_shifted = 0;
                channel_tracks.insert(position, compiled);
                self.track_count += 1;
                self.track_locators.insert(track.id, locator);
                self.objects[locator.object_index as usize]
                    .dynamic
                    .mark(locator.property);
            }
            ExecutionPatch::AddFamilyAnimation(animation) => {
                let object_index = self
                    .object_index(animation.target)
                    .ok_or(CompilePatchError::UnknownObject(animation.target))?;
                animation
                    .spec
                    .validate()
                    .map_err(|_| CompilePatchError::InvalidFamilyAnimation)?;
                animation
                    .time_map
                    .validate()
                    .map_err(|_| CompilePatchError::InvalidFamilyAnimation)?;
                let plan_index = u32::try_from(self.family_animation_plans.len())
                    .map_err(|_| CompilePatchError::TooManyFamilyAnimations)?;
                self.family_animation_plans.push(animation.plan.clone());
                self.family_animations.push(CompiledFamilyAnimationChannel {
                    target: animation.target,
                    object_index,
                    plan_index,
                    spec: animation.spec,
                    time_map: animation.time_map.clone(),
                });
            }
            ExecutionPatch::ReplaceTrack(track) => {
                let old_locator = self
                    .track_locators
                    .get(&track.id)
                    .copied()
                    .ok_or(CompilePatchError::UnknownTrack(track.id))?;
                let old_channel =
                    CompiledChannelKey::new(old_locator.object_index, old_locator.property);
                let old_position = self.track_position(old_locator);
                let compiled = self.compile_patch_track(track)?;
                stats.presence_tracks_inspected +=
                    self.validate_presence_edit(Some(track.id), Some(&compiled))?;

                let remove_channel = {
                    let old_tracks = self
                        .tracks
                        .get_mut(&old_channel)
                        .expect("track locator channel must exist");
                    stats.dense_track_slots_shifted +=
                        old_tracks.len().saturating_sub(old_position + 1);
                    old_tracks.remove(old_position);
                    old_tracks.is_empty()
                };
                if remove_channel {
                    self.tracks.remove(&old_channel);
                }

                let new_locator = CompiledTrackLocator::from_track(&compiled);
                let new_channel =
                    CompiledChannelKey::new(new_locator.object_index, new_locator.property);
                let new_tracks = self.tracks.entry(new_channel).or_default();
                let new_position = track_insertion_position(new_tracks, &compiled);
                stats.dense_track_slots_shifted += new_tracks.len().saturating_sub(new_position);
                stats.unrelated_track_slots_shifted = 0;
                new_tracks.insert(new_position, compiled);
                self.track_locators.insert(track.id, new_locator);
                self.recompute_dynamic_for_objects(
                    &[old_locator.object_index, new_locator.object_index],
                    &mut stats,
                );
            }
            ExecutionPatch::RemoveTrack(id) => {
                let old_locator = self
                    .track_locators
                    .get(id)
                    .copied()
                    .ok_or(CompilePatchError::UnknownTrack(*id))?;
                stats.presence_tracks_inspected += self.validate_presence_edit(Some(*id), None)?;
                let channel =
                    CompiledChannelKey::new(old_locator.object_index, old_locator.property);
                let position = self.track_position(old_locator);
                let remove_channel = {
                    let tracks = self
                        .tracks
                        .get_mut(&channel)
                        .expect("track locator channel must exist");
                    stats.dense_track_slots_shifted = tracks.len().saturating_sub(position + 1);
                    tracks.remove(position);
                    tracks.is_empty()
                };
                if remove_channel {
                    self.tracks.remove(&channel);
                }
                self.track_count -= 1;
                stats.unrelated_track_slots_shifted = 0;
                self.track_locators.remove(id);
                self.recompute_dynamic_for_objects(&[old_locator.object_index], &mut stats);
            }
            ExecutionPatch::ReconcileTrack {
                track,
                object,
                property,
                end_time,
            } => {
                let locator = self
                    .track_locators
                    .get(track)
                    .copied()
                    .ok_or(CompilePatchError::UnknownTrack(*track))?;
                let position = self.track_position(locator);
                let channel = CompiledChannelKey::new(locator.object_index, locator.property);
                let compiled = &self.tracks[&channel][position];
                if compiled.reconciled {
                    return Err(CompilePatchError::TrackAlreadyReconciled(*track));
                }
                let actual_object = self
                    .object_index(*object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                let actual_end = compiled.timing.start_time + compiled.timing.duration;
                if actual_object != locator.object_index
                    || *property != locator.property
                    || actual_end.total_cmp(end_time) != std::cmp::Ordering::Equal
                {
                    return Err(CompilePatchError::TrackReconciliationMismatch(*track));
                }
                if compiled.timing.duration <= 0.0
                    || !matches!(
                        compiled.property,
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
                    )
                {
                    return Err(CompilePatchError::UnsupportedTrackReconciliation(*track));
                }
                for other in &self.tracks[&channel] {
                    if other.id == *track {
                        continue;
                    }
                    let (compiled_start, compiled_end) =
                        continuous_time_map_interval(compiled.timing, &compiled.time_map)
                            .expect("compiled track has a valid composition time map");
                    let (other_start, other_end) =
                        continuous_time_map_interval(other.timing, &other.time_map)
                            .expect("compiled track has a valid composition time map");
                    if compiled_start < other_end && other_start < compiled_end {
                        return Err(CompilePatchError::OverlappingTrackReconciliation {
                            track: *track,
                            other: other.id,
                        });
                    }
                }
                let compiled = &mut self
                    .tracks
                    .get_mut(&channel)
                    .expect("track locator channel must exist")[position];
                compiled.reconciled = true;
                stats.tracks_reconciled = 1;
            }
        }
        Ok(stats)
    }

    fn compile_patch_track(
        &self,
        track: &TrackDefinition,
    ) -> Result<CompiledTrack, CompilePatchError> {
        let object_index = self
            .object_index(track.object)
            .ok_or(CompilePatchError::UnknownObject(track.object))?;
        validate_track_definition(track).map_err(CompilePatchError::InvalidTrack)?;
        reject_geometry_track_on_text(&self.objects[object_index as usize], track).map_err(
            |(track, property)| CompilePatchError::GeometryTrackTargetsText { track, property },
        )?;
        compile_track(track, object_index).map_err(|error| compile_patch_error(track.id, error))
    }

    fn track_position(&self, locator: CompiledTrackLocator) -> usize {
        let channel = CompiledChannelKey::new(locator.object_index, locator.property);
        self.channel_tracks(channel)
            .binary_search_by(|track| compare_track_locator(track, locator))
            .expect("track locator index must match channel-local sorted storage")
    }

    fn validate_presence_edit(
        &self,
        excluded: Option<TrackId>,
        candidate: Option<&CompiledTrack>,
    ) -> Result<usize, CompilePatchError> {
        let mut affected_objects = Vec::with_capacity(2);
        if let Some(id) = excluded {
            let locator = self
                .track_locators
                .get(&id)
                .copied()
                .expect("excluded track was resolved before presence validation");
            if locator.property == Property::Presence {
                affected_objects.push(locator.object_index);
            }
        }
        if let Some(track) = candidate {
            if track.property == Property::Presence
                && !affected_objects.contains(&track.object_index)
            {
                affected_objects.push(track.object_index);
            }
        }

        let mut inspected = 0;
        for object_index in affected_objects {
            let mut events = Vec::new();
            let presence_channel = CompiledChannelKey::new(object_index, Property::Presence);
            for track in self.channel_tracks(presence_channel) {
                if excluded == Some(track.id) {
                    continue;
                }
                let TrackValues::Bool { from, to } = track.values else {
                    unreachable!("validated Presence track must contain bool values");
                };
                events.push((track.timing.start_time, track.id, from, to));
                inspected += 1;
            }
            if let Some(track) = candidate.filter(|track| {
                track.object_index == object_index && track.property == Property::Presence
            }) {
                let TrackValues::Bool { from, to } = track.values else {
                    unreachable!("validated Presence track must contain bool values");
                };
                events.push((track.timing.start_time, track.id, from, to));
                inspected += 1;
            }
            events.sort_by(|left, right| {
                left.0
                    .total_cmp(&right.0)
                    .then_with(|| left.1.cmp(&right.1))
            });
            for pair in events.windows(2) {
                if pair[0].3 != pair[1].2 {
                    return Err(CompilePatchError::DiscontinuousPresence {
                        previous: pair[0].1,
                        next: pair[1].1,
                    });
                }
            }
        }
        Ok(inspected)
    }

    fn recompute_dynamic_for_objects(
        &mut self,
        object_indices: &[u32],
        stats: &mut CompiledPatchStats,
    ) {
        let mut unique = Vec::with_capacity(object_indices.len());
        for object_index in object_indices.iter().copied() {
            if !unique.contains(&object_index) {
                unique.push(object_index);
            }
        }
        for object_index in unique {
            let mut dynamic = DynamicProperties::default();
            let channels: Vec<_> = self.channels_for_object_index(object_index).collect();
            for channel in channels {
                for track in self.channel_tracks(channel) {
                    dynamic.mark(track.property);
                    stats.dynamic_tracks_inspected += 1;
                }
            }
            self.objects[object_index as usize].dynamic = dynamic;
            stats.dynamic_objects_recomputed += 1;
        }
    }
}

fn reject_geometry_track_on_text(
    object: &CompiledObject,
    track: &TrackDefinition,
) -> Result<(), (TrackId, Property)> {
    if object.text().is_some() && matches!(track.property, Property::Transform | Property::Morph) {
        return Err((track.id, track.property));
    }
    Ok(())
}

fn compile_track(
    track: &TrackDefinition,
    object_index: u32,
) -> Result<CompiledTrack, TransformCompileFailure> {
    let timing = resolve_track_timing(track).expect("track was validated before compilation");
    let time_map = if track.property.is_instant() {
        CompositionTimeMap::identity()
    } else {
        track.time_map.clone()
    };
    Ok(CompiledTrack {
        id: track.id,
        object_index,
        property: track.property,
        values: track.values.clone(),
        timing,
        time_map,
        transform_geometry_plan: compile_transform_geometry_plan(track)?,
        reconciled: false,
    })
}

fn compile_error(id: TrackId, error: TransformCompileFailure) -> CompileError {
    match error {
        TransformCompileFailure::UnsupportedGeometry => {
            CompileError::UnsupportedTransformGeometry(id)
        }
        TransformCompileFailure::RequiresRetessellation => {
            CompileError::PathTransformRequiresRetessellation(id)
        }
        TransformCompileFailure::UnsafeFilledPath => CompileError::UnsafeFilledPathTransform(id),
    }
}

fn compile_patch_error(id: TrackId, error: TransformCompileFailure) -> CompilePatchError {
    match error {
        TransformCompileFailure::UnsupportedGeometry => {
            CompilePatchError::UnsupportedTransformGeometry(id)
        }
        TransformCompileFailure::RequiresRetessellation => {
            CompilePatchError::PathTransformRequiresRetessellation(id)
        }
        TransformCompileFailure::UnsafeFilledPath => {
            CompilePatchError::UnsafeFilledPathTransform(id)
        }
    }
}

fn map_object_state_error(error: noon_core::ObjectStateError) -> CompilePatchError {
    CompilePatchError::InvalidObjectState {
        object: error.object,
        field: error.field,
    }
}

fn validate_execution_content(
    object: ObjectId,
    content: &ObjectContentRef,
    text_bounds: Option<Rect>,
) -> Result<(), CompilePatchError> {
    match content {
        ObjectContentRef::Geometry(geometry) => {
            if text_bounds.is_some() {
                return Err(CompilePatchError::InvalidContentBounds(object));
            }
            validate_geometry(object, geometry).map_err(map_object_state_error)
        }
        ObjectContentRef::Text(_) => {
            let valid = text_bounds.is_some_and(|bounds| {
                bounds.min.x.is_finite()
                    && bounds.min.y.is_finite()
                    && bounds.max.x.is_finite()
                    && bounds.max.y.is_finite()
                    && bounds.min.x <= bounds.max.x
                    && bounds.min.y <= bounds.max.y
            });
            valid
                .then_some(())
                .ok_or(CompilePatchError::InvalidContentBounds(object))
        }
    }
}

fn validate_execution_content_resource(
    resources: &CompiledResources,
    additions: Option<&CompiledResources>,
    object: ObjectId,
    content: &ObjectContentRef,
    text_bounds: Option<Rect>,
) -> Result<(), CompilePatchError> {
    let ObjectContentRef::Text(handle) = content else {
        return Ok(());
    };
    let resource = TextResourceLookup::get(resources, *handle)
        .or_else(|| additions.and_then(|resources| TextResourceLookup::get(resources, *handle)))
        .ok_or(CompilePatchError::Resource(
            CompiledResourceError::MissingText(*handle),
        ))?;
    if text_bounds != Some(resource.bounds) {
        return Err(CompilePatchError::TextBoundsMismatch {
            object,
            resource: *handle,
        });
    }
    Ok(())
}

fn validate_compiled_object(object: &CompiledObject) -> Result<(), CompilePatchError> {
    validate_execution_content(object.id, &object.content, object.text_bounds)?;
    validate_transform(object.id, object.base_transform).map_err(map_object_state_error)?;
    validate_style(object.id, object.base_style).map_err(map_object_state_error)
}

fn track_insertion_position(tracks: &[CompiledTrack], track: &CompiledTrack) -> usize {
    tracks
        .binary_search_by(|existing| compare_tracks(existing, track))
        .unwrap_or_else(|position| position)
}

fn compare_tracks(left: &CompiledTrack, right: &CompiledTrack) -> Ordering {
    left.object_index
        .cmp(&right.object_index)
        .then_with(|| property_rank(left.property).cmp(&property_rank(right.property)))
        .then_with(|| left.timing.start_time.total_cmp(&right.timing.start_time))
        .then_with(|| left.id.cmp(&right.id))
}

fn compare_track_locator(track: &CompiledTrack, locator: CompiledTrackLocator) -> Ordering {
    track
        .object_index
        .cmp(&locator.object_index)
        .then_with(|| property_rank(track.property).cmp(&property_rank(locator.property)))
        .then_with(|| track.timing.start_time.total_cmp(&locator.start_time))
        .then_with(|| track.id.cmp(&locator.id))
}

fn sort_tracks(tracks: &mut [CompiledTrack]) {
    tracks.sort_by(compare_tracks);
}

fn validate_presence_chains(tracks: &[CompiledTrack]) -> Result<(), (TrackId, TrackId)> {
    let mut previous: Option<(u32, TrackId, bool)> = None;
    for track in tracks
        .iter()
        .filter(|track| track.property == Property::Presence)
    {
        let TrackValues::Bool { from, to } = &track.values else {
            unreachable!("validated Presence track must contain bool values");
        };
        if let Some((object_index, previous_id, previous_to)) = previous {
            if object_index == track.object_index && previous_to != *from {
                return Err((previous_id, track.id));
            }
        }
        previous = Some((track.object_index, track.id, *to));
    }
    Ok(())
}

const fn property_rank(property: Property) -> u8 {
    match property {
        Property::Presence => 0,
        Property::Transform => 1,
        Property::Position => 2,
        Property::Rotation => 3,
        Property::Scale => 4,
        Property::Fill => 5,
        Property::Stroke => 6,
        Property::StrokeWidth => 7,
        Property::Opacity => 8,
        Property::Appearance => 9,
        Property::Reveal => 10,
        Property::Morph => 11,
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        CompositionTimeMap, CompositionTimeMapStep, Easing, GeometryRef, Property, RateFunction,
        TextResourceHandle, TextResourceId, TrackTiming, TrackValues, Vec2,
    };

    use super::*;

    fn filled_loop() -> noon_core::VectorPath {
        noon_core::VectorPath::new()
            .move_to(Vec2::new(0.0, 1.5))
            .cubic_to(
                Vec2::new(1.0, 1.5),
                Vec2::new(1.5, 1.0),
                Vec2::new(1.5, 0.0),
            )
            .cubic_to(
                Vec2::new(1.5, -1.0),
                Vec2::new(1.0, -1.5),
                Vec2::new(0.0, -1.5),
            )
            .cubic_to(
                Vec2::new(-1.0, -1.5),
                Vec2::new(-1.5, -1.0),
                Vec2::new(-1.5, 0.0),
            )
            .cubic_to(
                Vec2::new(-1.5, 1.0),
                Vec2::new(-1.0, 1.5),
                Vec2::new(0.0, 1.5),
            )
            .close()
    }

    fn filled_star() -> noon_core::VectorPath {
        noon_core::VectorPath::new()
            .move_to(Vec2::new(0.0, 1.9))
            .line_to(Vec2::new(0.45, 0.62))
            .line_to(Vec2::new(1.8, 0.58))
            .line_to(Vec2::new(0.72, -0.24))
            .line_to(Vec2::new(1.12, -1.54))
            .line_to(Vec2::new(0.0, -0.78))
            .line_to(Vec2::new(-1.12, -1.54))
            .line_to(Vec2::new(-0.72, -0.24))
            .line_to(Vec2::new(-1.8, 0.58))
            .line_to(Vec2::new(-0.45, 0.62))
            .close()
    }

    #[test]
    fn safe_filled_path_transform_compiles_to_fixed_path_pair() {
        let mut source_objects = Vec::new();
        let mut source_tracks = Vec::new();
        let object = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            object,
            GeometryRef::path(filled_loop()),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        let mut from = noon_core::TransformTrackEndpoint::new(GeometryRef::path(filled_loop()));
        let mut to = noon_core::TransformTrackEndpoint::new(GeometryRef::path(filled_star()));
        from.style.fill = Some(noon_core::Color::WHITE);
        to.style.fill = Some(noon_core::Color::BLACK);
        source_tracks.push(TrackDefinition {
            id: TrackId::new(source_tracks.len() as u64),
            object,
            property: Property::Transform,
            values: TrackValues::Object { from, to },
            timing: TrackTiming::new(0.0, 2.0, Easing::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        let compiled = CompiledScene::compile_objects(source_objects, &source_tracks)
            .expect("safe filled path must compile");
        assert!(matches!(
            compiled.tracks()[0].transform_geometry_plan,
            Some(TransformGeometryPlan::PathPair { .. })
        ));
    }

    #[test]
    fn filled_path_transform_rejects_fill_presence_change() {
        let mut source_objects = Vec::new();
        let mut source_tracks = Vec::new();
        let object = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            object,
            GeometryRef::path(filled_loop()),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        let from = noon_core::TransformTrackEndpoint::new(GeometryRef::path(filled_loop()));
        let mut to = noon_core::TransformTrackEndpoint::new(GeometryRef::path(filled_star()));
        to.style.fill = None;
        let mut from = from;
        from.style.fill = Some(noon_core::Color::WHITE);
        source_tracks.push(TrackDefinition {
            id: TrackId::new(source_tracks.len() as u64),
            object,
            property: Property::Transform,
            values: TrackValues::Object { from, to },
            timing: TrackTiming::new(0.0, 2.0, Easing::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        assert!(matches!(
            CompiledScene::compile_objects(source_objects, &source_tracks),
            Err(CompileError::PathTransformRequiresRetessellation(_))
        ));
    }

    #[test]
    fn object_ids_resolve_to_dense_indices() {
        let mut source_objects = Vec::new();
        let source_tracks = Vec::new();
        let circle = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            circle,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        let rectangle = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            rectangle,
            GeometryRef::rectangle(2.0, 3.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        let compiled = CompiledScene::compile_objects(source_objects, &source_tracks)
            .expect("scene must compile");
        assert_eq!(compiled.object_index(circle), Some(0));
        assert_eq!(compiled.object_index(rectangle), Some(1));
        assert_eq!(compiled.objects()[0].id, circle);
        assert_eq!(compiled.objects()[1].id, rectangle);
    }

    #[test]
    fn tracks_are_sorted_for_runtime_access() {
        let mut source_objects = Vec::new();
        let mut source_tracks = Vec::new();
        let object = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        source_tracks.push(TrackDefinition {
            id: TrackId::new(source_tracks.len() as u64),
            object,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::new(5.0, 0.0),
                to: Vec2::new(6.0, 0.0),
            },
            timing: TrackTiming::new(5.0, 1.0, Easing::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        source_tracks.push(TrackDefinition {
            id: TrackId::new(source_tracks.len() as u64),
            object,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::ONE,
            },
            timing: TrackTiming::new(1.0, 1.0, Easing::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        let compiled = CompiledScene::compile_objects(source_objects, &source_tracks)
            .expect("scene must compile");
        let starts: Vec<f64> = compiled
            .tracks()
            .iter()
            .map(|track| track.timing.start_time)
            .collect();
        assert_eq!(starts, vec![1.0, 5.0]);
    }

    #[test]
    fn composition_time_map_is_preserved_in_compiled_track() {
        let mut source_objects = Vec::new();
        let mut source_tracks = Vec::new();
        let object = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        let map = CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
            0.25,
            0.5,
            RateFunction::Smooth,
        )]);
        source_tracks.push(TrackDefinition {
            id: TrackId::new(source_tracks.len() as u64),
            object,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::ONE,
            },
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            time_map: map.clone(),
        });
        let compiled = CompiledScene::compile_objects(source_objects, &source_tracks).unwrap();
        assert_eq!(compiled.tracks()[0].time_map, map);
    }

    #[test]
    fn mapped_presence_compiles_to_one_scheduler_event() {
        let mut store = SemanticStore::new();
        let node = store.insert_semantic_object(noon_core::SemanticObjectState::new(
            noon_core::StoredGeometry::Circle { radius: 1.0 },
        ));
        store.attach_semantic_object(node).unwrap();
        let mut index = SemanticExecutionIndex::new();
        let (mut compiled, _) = lower_semantic_execution(&store, &mut index)
            .unwrap()
            .into_parts();
        let object = index.execution_object_id(node).unwrap();
        compiled
            .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
                id: TrackId::new(0),
                object,
                property: Property::Presence,
                values: TrackValues::Bool {
                    from: false,
                    to: true,
                },
                timing: TrackTiming::new(3.0, 4.0, RateFunction::Linear),
                time_map: CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
                    0.25,
                    0.5,
                    RateFunction::Linear,
                )]),
            }))
            .unwrap();
        let track = &compiled.tracks()[0];
        assert_eq!(track.timing, TrackTiming::instant(4.0));
        assert!(track.time_map.is_identity());
    }

    #[test]
    fn only_animated_properties_are_marked_dynamic() {
        let mut source_objects = Vec::new();
        let mut source_tracks = Vec::new();
        let animated = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            animated,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        let static_object = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            static_object,
            GeometryRef::rectangle(2.0, 2.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        source_tracks.push(TrackDefinition {
            id: TrackId::new(source_tracks.len() as u64),
            object: animated,
            property: Property::Opacity,
            values: TrackValues::Scalar { from: 1.0, to: 0.0 },
            timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        let compiled = CompiledScene::compile_objects(source_objects, &source_tracks)
            .expect("scene must compile");
        let animated_index = compiled.object_index(animated).expect("known object") as usize;
        let static_index = compiled.object_index(static_object).expect("known object") as usize;
        assert_eq!(
            compiled.objects()[animated_index].dynamic,
            DynamicProperties {
                presence: false,
                transform: false,
                position: false,
                rotation: false,
                scale: false,
                fill: false,
                stroke: false,
                stroke_width: false,
                opacity: true,
                appearance: false,
                reveal: false,
                morph: false,
            }
        );
        assert!(!compiled.objects()[static_index].dynamic.any());
    }

    #[test]
    fn scale_tracks_mark_only_scale_dynamic() {
        let mut source_objects = Vec::new();
        let mut source_tracks = Vec::new();
        let object = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        source_tracks.push(TrackDefinition {
            id: TrackId::new(source_tracks.len() as u64),
            object,
            property: Property::Scale,
            values: TrackValues::Vec2 {
                from: Vec2::ONE,
                to: Vec2::new(2.0, 0.5),
            },
            timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        let compiled = CompiledScene::compile_objects(source_objects, &source_tracks)
            .expect("scene must compile");
        assert_eq!(
            compiled.objects()[0].dynamic,
            DynamicProperties {
                scale: true,
                ..DynamicProperties::default()
            }
        );
    }

    #[test]
    fn stroke_width_tracks_mark_only_stroke_width_dynamic() {
        let object = ObjectId::new(7);
        let compiled = CompiledScene::compile_objects(
            vec![CompiledObject::new(
                object,
                GeometryRef::circle(1.0),
                Transform2D::IDENTITY,
                Style::default(),
            )],
            &[TrackDefinition {
                id: TrackId::new(0),
                object,
                property: Property::StrokeWidth,
                values: TrackValues::Scalar { from: 1.0, to: 2.0 },
                timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
                time_map: CompositionTimeMap::identity(),
            }],
        )
        .expect("scene must compile");
        assert_eq!(
            compiled.objects()[0].dynamic,
            DynamicProperties {
                stroke_width: true,
                ..DynamicProperties::default()
            }
        );
    }

    #[test]
    fn appearance_tracks_mark_only_appearance_dynamic() {
        let mut source_objects = Vec::new();
        let mut source_tracks = Vec::new();
        let object = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        source_tracks.push(TrackDefinition {
            id: TrackId::new(source_tracks.len() as u64),
            object,
            property: Property::Appearance,
            values: TrackValues::Scalar { from: 0.0, to: 1.0 },
            timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        let compiled = CompiledScene::compile_objects(source_objects, &source_tracks)
            .expect("scene must compile");
        assert_eq!(
            compiled.objects()[0].dynamic,
            DynamicProperties {
                appearance: true,
                ..DynamicProperties::default()
            }
        );
    }

    #[test]
    fn presence_tracks_mark_only_presence_dynamic() {
        let mut source_objects = Vec::new();
        let mut source_tracks = Vec::new();
        let object = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        source_tracks.push(TrackDefinition {
            id: TrackId::new(source_tracks.len() as u64),
            object,
            property: Property::Presence,
            values: TrackValues::Bool {
                from: false,
                to: true,
            },
            timing: TrackTiming::instant(2.0),
            time_map: CompositionTimeMap::identity(),
        });
        let compiled = CompiledScene::compile_objects(source_objects, &source_tracks)
            .expect("scene must compile");
        assert_eq!(
            compiled.objects()[0].dynamic,
            DynamicProperties {
                presence: true,
                ..DynamicProperties::default()
            }
        );
        assert_eq!(compiled.tracks()[0].timing.duration, 0.0);
    }

    #[test]
    fn continuous_presence_chain_compiles() {
        let mut source_objects = Vec::new();
        let mut source_tracks = Vec::new();
        let object = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        source_tracks.push(TrackDefinition {
            id: TrackId::new(source_tracks.len() as u64),
            object,
            property: Property::Presence,
            values: TrackValues::Bool {
                from: false,
                to: true,
            },
            timing: TrackTiming::instant(1.0),
            time_map: CompositionTimeMap::identity(),
        });
        source_tracks.push(TrackDefinition {
            id: TrackId::new(source_tracks.len() as u64),
            object,
            property: Property::Presence,
            values: TrackValues::Bool {
                from: true,
                to: false,
            },
            timing: TrackTiming::instant(2.0),
            time_map: CompositionTimeMap::identity(),
        });
        CompiledScene::compile_objects(source_objects, &source_tracks)
            .expect("continuous presence chain must compile");
    }

    #[test]
    fn discontinuous_presence_chain_is_rejected() {
        let mut source_objects = Vec::new();
        let mut source_tracks = Vec::new();
        let object = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        let previous = TrackId::new(source_tracks.len() as u64);
        source_tracks.push(TrackDefinition {
            id: previous,
            object,
            property: Property::Presence,
            values: TrackValues::Bool {
                from: false,
                to: true,
            },
            timing: TrackTiming::instant(1.0),
            time_map: CompositionTimeMap::identity(),
        });
        let next = TrackId::new(source_tracks.len() as u64);
        source_tracks.push(TrackDefinition {
            id: next,
            object,
            property: Property::Presence,
            values: TrackValues::Bool {
                from: false,
                to: true,
            },
            timing: TrackTiming::instant(2.0),
            time_map: CompositionTimeMap::identity(),
        });
        assert_eq!(
            CompiledScene::compile_objects(source_objects, &source_tracks),
            Err(CompileError::DiscontinuousPresence { previous, next })
        );
    }

    #[test]
    fn patch_rejects_discontinuous_presence_without_mutating_tracks() {
        let mut source_objects = Vec::new();
        let mut source_tracks = Vec::new();
        let object = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        let previous = TrackId::new(source_tracks.len() as u64);
        source_tracks.push(TrackDefinition {
            id: previous,
            object,
            property: Property::Presence,
            values: TrackValues::Bool {
                from: false,
                to: true,
            },
            timing: TrackTiming::instant(1.0),
            time_map: CompositionTimeMap::identity(),
        });
        let mut compiled = CompiledScene::compile_objects(source_objects, &source_tracks)
            .expect("scene must compile");
        let before = compiled.tracks().to_vec();
        let next = TrackId::new(9);
        let track = TrackDefinition {
            id: next,
            object,
            property: Property::Presence,
            values: TrackValues::Bool {
                from: false,
                to: true,
            },
            timing: TrackTiming::new(2.0, 0.0, Easing::Linear),
            time_map: CompositionTimeMap::identity(),
        };
        assert_eq!(
            compiled.apply_execution_patch(&ExecutionPatch::AddTrack(track)),
            Err(CompilePatchError::DiscontinuousPresence { previous, next })
        );
        assert_eq!(compiled.tracks(), before);
    }

    #[test]
    fn patch_rejects_removing_required_presence_handoff_without_mutating_tracks() {
        let mut source_objects = Vec::new();
        let mut source_tracks = Vec::new();
        let object = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        let first = TrackId::new(source_tracks.len() as u64);
        source_tracks.push(TrackDefinition {
            id: first,
            object,
            property: Property::Presence,
            values: TrackValues::Bool {
                from: false,
                to: true,
            },
            timing: TrackTiming::instant(1.0),
            time_map: CompositionTimeMap::identity(),
        });
        let middle = TrackId::new(source_tracks.len() as u64);
        source_tracks.push(TrackDefinition {
            id: middle,
            object,
            property: Property::Presence,
            values: TrackValues::Bool {
                from: true,
                to: false,
            },
            timing: TrackTiming::instant(2.0),
            time_map: CompositionTimeMap::identity(),
        });
        let last = TrackId::new(source_tracks.len() as u64);
        source_tracks.push(TrackDefinition {
            id: last,
            object,
            property: Property::Presence,
            values: TrackValues::Bool {
                from: false,
                to: true,
            },
            timing: TrackTiming::instant(3.0),
            time_map: CompositionTimeMap::identity(),
        });
        let mut compiled = CompiledScene::compile_objects(source_objects, &source_tracks)
            .expect("scene must compile");
        let before = compiled.tracks().to_vec();
        assert_eq!(
            compiled.apply_execution_patch(&ExecutionPatch::RemoveTrack(middle)),
            Err(CompilePatchError::DiscontinuousPresence {
                previous: first,
                next: last,
            })
        );
        assert_eq!(compiled.tracks(), before);
    }

    #[test]
    fn reveal_tracks_mark_only_reveal_dynamic() {
        let mut source_objects = Vec::new();
        let mut source_tracks = Vec::new();
        let object = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            object,
            GeometryRef::path(
                noon_core::VectorPath::new()
                    .move_to(Vec2::ZERO)
                    .line_to(Vec2::ONE),
            ),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        source_tracks.push(TrackDefinition {
            id: TrackId::new(source_tracks.len() as u64),
            object,
            property: Property::Reveal,
            values: TrackValues::Scalar { from: 0.0, to: 1.0 },
            timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        let compiled = CompiledScene::compile_objects(source_objects, &source_tracks)
            .expect("scene must compile");
        assert_eq!(
            compiled.objects()[0].dynamic,
            DynamicProperties {
                presence: false,
                transform: false,
                position: false,
                rotation: false,
                scale: false,
                fill: false,
                stroke: false,
                stroke_width: false,
                opacity: false,
                appearance: false,
                reveal: true,
                morph: false,
            }
        );
    }

    #[test]
    fn morph_tracks_mark_only_morph_dynamic() {
        let mut source_objects = Vec::new();
        let mut source_tracks = Vec::new();
        let object = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            object,
            GeometryRef::path(
                noon_core::VectorPath::new()
                    .move_to(Vec2::ZERO)
                    .line_to(Vec2::ONE),
            ),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        source_tracks.push(TrackDefinition {
            id: TrackId::new(source_tracks.len() as u64),
            object,
            property: Property::Morph,
            values: TrackValues::Scalar { from: 0.0, to: 1.0 },
            timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        let compiled = CompiledScene::compile_objects(source_objects, &source_tracks)
            .expect("scene must compile");
        assert!(compiled.objects()[0].dynamic.morph);
        assert!(!compiled.objects()[0].dynamic.reveal);
        assert!(!compiled.objects()[0].dynamic.appearance);
    }

    #[test]
    fn identical_input_compiles_identically() {
        fn build() -> CompiledScene {
            let mut source_objects = Vec::new();
            let mut source_tracks = Vec::new();
            let object = ObjectId::new(source_objects.len() as u64);
            source_objects.push(CompiledObject::new(
                object,
                GeometryRef::circle(2.0),
                Transform2D::IDENTITY,
                Style::default(),
            ));
            source_tracks.push(TrackDefinition {
                id: TrackId::new(source_tracks.len() as u64),
                object,
                property: Property::Position,
                values: TrackValues::Vec2 {
                    from: Vec2::ZERO,
                    to: Vec2::new(3.0, 4.0),
                },
                timing: TrackTiming::new(0.5, 2.0, Easing::EaseInOutCubic),
                time_map: CompositionTimeMap::identity(),
            });
            CompiledScene::compile_objects(source_objects, &source_tracks).unwrap()
        }
        assert_eq!(build(), build());
    }

    #[test]
    fn compiled_patches_preserve_dense_identity_and_dynamic_flags() {
        let mut source_objects = Vec::new();
        let source_tracks = Vec::new();
        let first = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            first,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        let second = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            second,
            GeometryRef::rectangle(2.0, 2.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        let mut compiled = CompiledScene::compile_objects(source_objects, &source_tracks)
            .expect("scene must compile");
        compiled
            .apply_execution_patch(&ExecutionPatch::CreateObject(CompiledObject::new(
                ObjectId::new(7),
                GeometryRef::circle(3.0),
                noon_core::Transform2D::IDENTITY,
                Style::default(),
            )))
            .expect("valid patch");
        assert_eq!(compiled.object_index(first), Some(0));
        assert_eq!(compiled.object_index(second), Some(1));
        assert_eq!(compiled.object_index(ObjectId::new(7)), Some(2));
        let track = TrackDefinition {
            id: TrackId::new(9),
            object: second,
            property: Property::Opacity,
            values: TrackValues::Scalar { from: 1.0, to: 0.0 },
            timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
            time_map: CompositionTimeMap::identity(),
        };
        compiled
            .apply_execution_patch(&ExecutionPatch::AddTrack(track))
            .expect("valid patch");
        assert!(compiled.objects()[1].dynamic.opacity);
    }

    #[test]
    fn large_add_track_patch_avoids_global_clone_and_dynamic_sweep() {
        let mut source_objects = Vec::new();
        let mut source_tracks = Vec::new();
        let mut objects = Vec::with_capacity(10_000);
        for index in 0..10_000u32 {
            let object = ObjectId::new(source_objects.len() as u64);
            source_objects.push(CompiledObject::new(
                object,
                GeometryRef::circle(1.0),
                Transform2D::IDENTITY,
                Style::default(),
            ));
            objects.push(object);
            source_tracks.push(TrackDefinition {
                id: TrackId::new(source_tracks.len() as u64),
                object,
                property: Property::Position,
                values: TrackValues::Vec2 {
                    from: Vec2::ZERO,
                    to: Vec2::ONE,
                },
                timing: TrackTiming::new(index as f64, 1.0, Easing::Linear),
                time_map: CompositionTimeMap::identity(),
            });
        }
        let mut compiled = CompiledScene::compile_objects(source_objects, &source_tracks)
            .expect("large scene must compile");
        let target = objects[5_000];
        let stats = compiled
            .apply_execution_patch_with_stats(&ExecutionPatch::AddTrack(TrackDefinition {
                id: TrackId::new(100_000),
                object: target,
                property: Property::Opacity,
                values: TrackValues::Scalar { from: 1.0, to: 0.0 },
                timing: TrackTiming::new(0.5, 1.0, Easing::Linear),
                time_map: CompositionTimeMap::identity(),
            }))
            .expect("local track add must compile");

        assert_eq!(stats.track_vector_clones, 0);
        assert_eq!(stats.presence_tracks_inspected, 0);
        assert_eq!(stats.dynamic_objects_recomputed, 0);
        assert_eq!(stats.dynamic_tracks_inspected, 0);
        assert_eq!(stats.dense_track_slots_shifted, 0);
        assert_eq!(stats.unrelated_track_slots_shifted, 0);
        let target_index = compiled.object_index(target).unwrap() as usize;
        assert!(compiled.objects()[target_index].dynamic.position);
        assert!(compiled.objects()[target_index].dynamic.opacity);
        assert!(compiled.objects()[0].dynamic.position);
    }

    #[test]
    fn unrelated_track_payload_address_survives_local_channel_edit() {
        let mut source_objects = Vec::new();
        let mut source_tracks = Vec::new();
        let mut objects = Vec::with_capacity(10_000);
        let mut track_ids = Vec::with_capacity(10_000);
        for index in 0..10_000u32 {
            let object = ObjectId::new(source_objects.len() as u64);
            source_objects.push(CompiledObject::new(
                object,
                GeometryRef::circle(1.0),
                Transform2D::IDENTITY,
                Style::default(),
            ));
            objects.push(object);
            let track_id = TrackId::new(source_tracks.len() as u64);
            track_ids.push(track_id);
            source_tracks.push(TrackDefinition {
                id: track_id,
                object,
                property: Property::Position,
                values: TrackValues::Vec2 {
                    from: Vec2::ZERO,
                    to: Vec2::ONE,
                },
                timing: TrackTiming::new(index as f64, 1.0, Easing::Linear),
                time_map: CompositionTimeMap::identity(),
            });
        }
        let mut compiled = CompiledScene::compile_objects(source_objects, &source_tracks).unwrap();
        let untouched = track_ids[9_999];
        let before = compiled.track(untouched).unwrap() as *const CompiledTrack as usize;

        let stats = compiled
            .apply_execution_patch_with_stats(&ExecutionPatch::AddTrack(TrackDefinition {
                id: TrackId::new(100_001),
                object: objects[5_000],
                property: Property::Opacity,
                values: TrackValues::Scalar { from: 1.0, to: 0.5 },
                timing: TrackTiming::new(0.25, 1.0, Easing::Linear),
                time_map: CompositionTimeMap::identity(),
            }))
            .unwrap();

        let after = compiled.track(untouched).unwrap() as *const CompiledTrack as usize;
        assert_eq!(before, after);
        assert_eq!(stats.unrelated_track_slots_shifted, 0);
        assert_eq!(compiled.track_count(), 10_001);
    }

    #[test]
    fn replace_track_recomputes_only_affected_object_channels() {
        let mut source_objects = Vec::new();
        let mut source_tracks = Vec::new();
        let first = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            first,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        let second = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            second,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        source_tracks.push(TrackDefinition {
            id: TrackId::new(source_tracks.len() as u64),
            object: first,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::ONE,
            },
            timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        source_tracks.push(TrackDefinition {
            id: TrackId::new(source_tracks.len() as u64),
            object: second,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::ONE,
            },
            timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        let replaced = TrackId::new(source_tracks.len() as u64);
        source_tracks.push(TrackDefinition {
            id: replaced,
            object: first,
            property: Property::Opacity,
            values: TrackValues::Scalar { from: 1.0, to: 0.0 },
            timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        let mut compiled = CompiledScene::compile_objects(source_objects, &source_tracks).unwrap();
        let stats = compiled
            .apply_execution_patch_with_stats(&ExecutionPatch::ReplaceTrack(TrackDefinition {
                id: replaced,
                object: second,
                property: Property::Opacity,
                values: TrackValues::Scalar {
                    from: 0.5,
                    to: 0.25,
                },
                timing: TrackTiming::new(2.0, 1.0, Easing::Linear),
                time_map: CompositionTimeMap::identity(),
            }))
            .unwrap();

        assert_eq!(stats.track_vector_clones, 0);
        assert_eq!(stats.dynamic_objects_recomputed, 2);
        assert_eq!(stats.dynamic_tracks_inspected, 3);
        assert!(
            !compiled.objects()[compiled.object_index(first).unwrap() as usize]
                .dynamic
                .opacity
        );
        assert!(
            compiled.objects()[compiled.object_index(second).unwrap() as usize]
                .dynamic
                .opacity
        );
    }

    #[test]
    fn presence_patch_validation_inspects_only_affected_chain() {
        let mut source_objects = Vec::new();
        let mut source_tracks = Vec::new();
        let target = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            target,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        let first = TrackId::new(source_tracks.len() as u64);
        source_tracks.push(TrackDefinition {
            id: first,
            object: target,
            property: Property::Presence,
            values: TrackValues::Bool {
                from: false,
                to: true,
            },
            timing: TrackTiming::instant(1.0),
            time_map: CompositionTimeMap::identity(),
        });
        for index in 0..5_000u32 {
            let object = ObjectId::new(source_objects.len() as u64);
            source_objects.push(CompiledObject::new(
                object,
                GeometryRef::circle(1.0),
                Transform2D::IDENTITY,
                Style::default(),
            ));
            source_tracks.push(TrackDefinition {
                id: TrackId::new(source_tracks.len() as u64),
                object,
                property: Property::Position,
                values: TrackValues::Vec2 {
                    from: Vec2::ZERO,
                    to: Vec2::ONE,
                },
                timing: TrackTiming::new(index as f64, 1.0, Easing::Linear),
                time_map: CompositionTimeMap::identity(),
            });
        }
        let mut compiled = CompiledScene::compile_objects(source_objects, &source_tracks).unwrap();
        let next = TrackId::new(50_000);
        let before = compiled.tracks().to_vec();
        let error = compiled
            .apply_execution_patch_with_stats(&ExecutionPatch::AddTrack(TrackDefinition {
                id: next,
                object: target,
                property: Property::Presence,
                values: TrackValues::Bool {
                    from: false,
                    to: true,
                },
                timing: TrackTiming::instant(2.0),
                time_map: CompositionTimeMap::identity(),
            }))
            .unwrap_err();
        assert_eq!(
            error,
            CompilePatchError::DiscontinuousPresence {
                previous: first,
                next,
            }
        );
        assert_eq!(compiled.tracks(), before);
    }

    #[test]
    fn removing_middle_object_keeps_compiled_slots_and_track_targets_stable() {
        let mut source_objects = Vec::new();
        let mut source_tracks = Vec::new();
        let mut objects = Vec::with_capacity(100_000);
        for _ in 0..100_000 {
            objects.push({
                let id = ObjectId::new(source_objects.len() as u64);
                source_objects.push(CompiledObject::new(
                    id,
                    GeometryRef::circle(1.0),
                    Transform2D::IDENTITY,
                    Style::default(),
                ));
                id
            });
        }
        let tracked = objects[99_999];
        let track = TrackId::new(source_tracks.len() as u64);
        source_tracks.push(TrackDefinition {
            id: track,
            object: tracked,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::ONE,
            },
            timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        let mut compiled = CompiledScene::compile_objects(source_objects, &source_tracks).unwrap();
        let later_slot = compiled.object_index(objects[50_001]).unwrap();
        let tracked_slot = compiled.object_index(tracked).unwrap();

        let stats = compiled
            .apply_execution_patch_with_stats(&ExecutionPatch::RemoveObject(objects[50_000]))
            .unwrap();

        assert_eq!(compiled.object_index(objects[50_000]), None);
        assert_eq!(compiled.object_index(objects[50_001]), Some(later_slot));
        assert_eq!(compiled.object_index(tracked), Some(tracked_slot));
        assert_eq!(
            compiled.channel_for_track(track).unwrap().object_index,
            tracked_slot
        );
        assert!(!compiled.object_slot_is_live(50_000));
        assert_eq!(compiled.objects().len(), 100_000);
        assert_eq!(compiled.live_object_count(), 99_999);
        assert_eq!(stats.object_slots_retired, 1);
        assert_eq!(stats.object_indices_rewritten, 0);
        assert_eq!(stats.track_object_indices_rewritten, 0);
        assert_eq!(stats.dynamic_objects_recomputed, 0);
        assert_eq!(stats.dynamic_tracks_inspected, 0);
    }

    #[test]
    fn transaction_preflight_rejects_late_compile_failure_without_scene_clone() {
        let mut source_objects = Vec::new();
        let source_tracks = Vec::new();
        let object = ObjectId::new(source_objects.len() as u64);
        source_objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        let compiled =
            CompiledScene::compile_objects(source_objects, &source_tracks).expect("valid scene");
        let from = noon_core::TransformTrackEndpoint::new(GeometryRef::circle(1.0));
        let to = noon_core::TransformTrackEndpoint::new(GeometryRef::line(
            Vec2::new(-1.0, 0.0),
            Vec2::new(1.0, 0.0),
        ));
        let transaction = ExecutionMutationTransaction::from_mutations([
            ExecutionPatch::SetTransform {
                object,
                transform: Transform2D {
                    translation: Vec2::new(2.0, 0.0),
                    ..Transform2D::IDENTITY
                },
            },
            ExecutionPatch::AddTrack(TrackDefinition {
                id: TrackId::new(50),
                object,
                property: Property::Transform,
                values: TrackValues::Object { from, to },
                timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
                time_map: CompositionTimeMap::identity(),
            }),
        ]);

        assert!(matches!(
            compiled.preflight_execution_transaction(&transaction),
            Err(CompilePatchError::UnsupportedTransformGeometry(_))
        ));
        assert_eq!(compiled.objects()[0].base_transform, Transform2D::IDENTITY);

        let valid = ExecutionMutationTransaction::from_mutations([ExecutionPatch::SetStyle {
            object,
            style: Style::default(),
        }]);
        let stats = compiled
            .preflight_execution_transaction(&valid)
            .expect("valid transaction preflights");
        assert_eq!(stats.mutations_preflighted, 1);
        assert_eq!(stats.staged_compiled_scene_clones, 0);
    }

    #[test]
    fn geometry_only_tracks_reject_text_before_compilation_mutates_state() {
        let text_id = ObjectId::new(20);
        let text = CompiledObject::new(
            text_id,
            TextResourceHandle {
                arena: 0,
                id: TextResourceId::new(7),
                version: 3,
            },
            Transform2D::IDENTITY,
            Style::default(),
        );
        let morph = TrackDefinition {
            id: TrackId::new(8),
            object: text_id,
            property: Property::Morph,
            values: TrackValues::Scalar { from: 0.0, to: 1.0 },
            timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        };

        assert_eq!(
            CompiledScene::compile_objects(vec![text], &[morph]),
            Err(CompileError::GeometryTrackTargetsText {
                track: TrackId::new(8),
                property: Property::Morph,
            })
        );
    }

    #[test]
    fn text_morph_patch_fails_before_compiled_state_changes() {
        let text_id = ObjectId::new(20);
        let text = CompiledObject::new(
            text_id,
            TextResourceHandle {
                arena: 0,
                id: TextResourceId::new(7),
                version: 3,
            },
            Transform2D::IDENTITY,
            Style::default(),
        );
        let mut compiled = CompiledScene::compile_objects(vec![text], &[]).unwrap();
        let morph = TrackDefinition {
            id: TrackId::new(8),
            object: text_id,
            property: Property::Morph,
            values: TrackValues::Scalar { from: 0.0, to: 1.0 },
            timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        };

        assert_eq!(
            compiled.apply_execution_patch(&ExecutionPatch::AddTrack(morph)),
            Err(CompilePatchError::GeometryTrackTargetsText {
                track: TrackId::new(8),
                property: Property::Morph,
            })
        );
        assert_eq!(compiled.track_count(), 0);
        assert!(!compiled.objects()[0].dynamic.any());
    }

    #[test]
    fn content_replacement_rejects_installed_geometry_driver_before_mutation() {
        let object = ObjectId::new(20);
        let morph = TrackDefinition {
            id: TrackId::new(8),
            object,
            property: Property::Morph,
            values: TrackValues::Scalar { from: 0.0, to: 1.0 },
            timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        };
        let compiled_object = CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        );
        let mut compiled = CompiledScene::compile_objects(vec![compiled_object], &[morph]).unwrap();
        let before = compiled.objects()[0].clone();

        let error = compiled
            .apply_execution_patch(&ExecutionPatch::SetContent {
                object,
                content: GeometryRef::circle(2.0).into(),
                text_bounds: None,
            })
            .unwrap_err();
        assert!(matches!(
            error,
            CompilePatchError::ContentReplacementHasGeometryDriver {
                object: rejected,
                track,
                property: Property::Morph,
            } if rejected == object && track == TrackId::new(8)
        ));
        assert_eq!(compiled.objects()[0], before);
    }

    #[test]
    fn typed_create_rejects_missing_text_resource_before_insertion() {
        let mut compiled = CompiledScene::compile_objects(Vec::new(), &[]).unwrap();
        let object = ObjectId::new(44);
        let resource = TextResourceHandle {
            arena: 7,
            id: TextResourceId::new(3),
            version: 2,
        };
        let mut text =
            CompiledObject::new(object, resource, Transform2D::IDENTITY, Style::default());
        text.text_bounds = Some(Rect::new(Vec2::ZERO, Vec2::ONE));
        let patch = ExecutionPatch::CreateObject(text);
        let transaction = ExecutionMutationTransaction::from_mutations([patch.clone()]);
        let expected = CompilePatchError::Resource(CompiledResourceError::MissingText(resource));

        assert_eq!(
            compiled.preflight_execution_transaction(&transaction),
            Err(expected.clone())
        );
        assert_eq!(compiled.apply_execution_patch(&patch), Err(expected));
        assert_eq!(compiled.object_index(object), None);
        assert_eq!(compiled.live_object_count(), 0);
    }

    #[test]
    fn typed_create_normalizes_runtime_derived_object_flags() {
        let mut compiled = CompiledScene::compile_objects(Vec::new(), &[]).unwrap();
        let object = ObjectId::new(45);
        let mut input = CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        );
        input.live = false;
        input.dynamic.morph = true;

        compiled
            .apply_execution_patch(&ExecutionPatch::CreateObject(input))
            .unwrap();

        let inserted = &compiled.objects()[compiled.object_index(object).unwrap() as usize];
        assert!(inserted.live);
        assert_eq!(inserted.dynamic, DynamicProperties::default());
        assert_eq!(compiled.live_object_count(), 1);
    }

    #[test]
    fn typed_create_reactivates_the_same_retired_row() {
        let returning = ObjectId::new(45);
        let later = ObjectId::new(46);
        let object = |id| {
            CompiledObject::new(
                id,
                GeometryRef::circle(1.0),
                Transform2D::IDENTITY,
                Style::default(),
            )
        };
        let mut compiled =
            CompiledScene::compile_objects(vec![object(returning), object(later)], &[]).unwrap();

        compiled
            .apply_execution_patch(&ExecutionPatch::RemoveObject(returning))
            .unwrap();
        let stats = compiled
            .apply_execution_patch_with_stats(&ExecutionPatch::CreateObject(object(returning)))
            .unwrap();

        assert_eq!(compiled.object_index(returning), Some(0));
        assert_eq!(compiled.object_index(later), Some(1));
        assert_eq!(compiled.objects().len(), 2);
        assert_eq!(compiled.live_object_count(), 2);
        assert_eq!(stats.object_slots_reactivated, 1);
        assert_eq!(stats.object_slots_appended, 0);
        assert_eq!(compiled.painter_order(), &[1, 0]);

        compiled
            .apply_execution_patch(&ExecutionPatch::ReorderObject {
                object: returning,
                before: Some(later),
            })
            .unwrap();
        assert_eq!(compiled.painter_order(), &[0, 1]);
        assert_eq!(compiled.object_index(returning), Some(0));
        assert_eq!(compiled.object_index(later), Some(1));
    }
}
