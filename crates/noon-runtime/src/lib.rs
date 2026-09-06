//! Deterministic renderer-free evaluation of compiled Noon scenes.

#![forbid(unsafe_code)]

mod execution_slots;
mod prepared_frame;
mod reactive;
mod renderer_publication;
mod spatial_index;

pub use execution_slots::*;
pub use prepared_frame::*;
pub use reactive::*;
pub use renderer_publication::*;
pub use spatial_index::*;

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use noon_compile::{
    CompilePatchError, CompiledChannelKey, CompiledScene, CompiledTrack, ExecutionPatch,
    TransformGeometryPlan,
};
use noon_core::PublicationContext;
use noon_core::{
    Color, GeometryRef, ObjectId, ObjectSnapshot, PathCommand, Property, ScenePatch,
    StrokeWidthMode, Style, TrackDefinition, TrackValues, Transform2D, Vec2, VectorPath,
};
use noon_core::{ObjectContentRef, TextResourceHandle};

#[derive(Clone, Debug, PartialEq)]
pub struct FrameObjectState {
    pub id: ObjectId,
    pub content: ObjectContentRef,
    pub text_bounds: Option<noon_core::Rect>,
    pub transform: Transform2D,
    pub style: Style,
    pub appearance: f32,
}

impl FrameObjectState {
    pub fn geometry(&self) -> Option<&GeometryRef> {
        self.content.geometry()
    }

    pub const fn text(&self) -> Option<TextResourceHandle> {
        self.content.text()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FrameState {
    pub time: f64,
    pub objects: Vec<FrameObjectState>,
    pub presences: Vec<bool>,
    pub reveals: Vec<f32>,
    pub morphs: Vec<f32>,
    pub render_geometries: Vec<Option<Arc<GeometryRef>>>,
    /// Derived geometry coordinate frame; absent means semantic object transform.
    pub render_transforms: Vec<Option<Transform2D>>,
}

impl FrameState {
    pub fn is_present(&self, object_index: usize) -> bool {
        self.presences[object_index]
    }

    pub fn appearance(&self, object_index: usize) -> f32 {
        self.objects[object_index].appearance
    }

    pub fn reveal(&self, object_index: usize) -> f32 {
        self.reveals[object_index]
    }

    pub fn morph(&self, object_index: usize) -> f32 {
        self.morphs[object_index]
    }

    pub fn render_transform(&self, object_index: usize) -> Transform2D {
        self.render_transforms[object_index].unwrap_or(self.objects[object_index].transform)
    }

    pub(crate) fn release_render_transform(&mut self, object_index: usize) -> bool {
        release_render_transform(
            &mut self.render_geometries[object_index],
            &mut self.render_transforms[object_index],
            self.objects[object_index].transform,
        )
    }

    pub fn render_geometry(&self, object_index: usize) -> Option<&GeometryRef> {
        self.render_geometries[object_index]
            .as_deref()
            .or_else(|| self.objects[object_index].geometry())
    }

    pub fn text(&self, object_index: usize) -> Option<TextResourceHandle> {
        self.objects[object_index].text()
    }
}

/// Copyable effective properties exposed to required host callbacks without
/// copying immutable geometry or text payloads out of the retained frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectiveObjectProperties {
    pub transform: Transform2D,
    pub style: Style,
    pub appearance: f32,
    pub presence: bool,
    pub reveal: f32,
    pub morph: f32,
    pub bounds: Option<noon_core::Rect>,
    bounds_basis: Option<EffectiveBoundsBasis>,
}

impl EffectiveObjectProperties {
    fn from_frame(
        frame: &FrameState,
        object_index: usize,
        bounds: Option<noon_core::Rect>,
    ) -> Self {
        Self {
            transform: frame.objects[object_index].transform,
            style: frame.objects[object_index].style,
            appearance: frame.objects[object_index].appearance,
            presence: frame.presences[object_index],
            reveal: frame.reveals[object_index],
            morph: frame.morphs[object_index],
            bounds,
            bounds_basis: EffectiveBoundsBasis::from_frame(frame, object_index),
        }
    }

    pub fn set_transform(&mut self, transform: Transform2D) {
        let previous = self.transform;
        self.transform = transform;
        if self.bounds_basis.is_some() {
            self.refresh_bounds();
        } else if previous.rotation == transform.rotation && previous.scale == transform.scale {
            let delta = transform.translation - previous.translation;
            self.bounds = self
                .bounds
                .map(|bounds| noon_core::Rect::new(bounds.min + delta, bounds.max + delta));
        } else {
            self.bounds = None;
        }
    }

    pub fn set_style(&mut self, style: Style) {
        let spatial_change = self.style.stroke.is_some() != style.stroke.is_some()
            || self.style.stroke_width != style.stroke_width
            || self.style.stroke_width_mode != style.stroke_width_mode
            || self.style.stroke_join != style.stroke_join
            || self.style.stroke_cap != style.stroke_cap;
        self.style = style;
        if self.bounds_basis.is_some() {
            self.refresh_bounds();
        } else if spatial_change {
            self.bounds = None;
        }
    }

    fn refresh_bounds(&mut self) {
        self.bounds = self
            .bounds_basis
            .and_then(|basis| basis.world_bounds(self.transform, self.style));
    }
}

#[cfg(test)]
mod effective_object_properties_tests {
    use noon_core::Rect;

    use super::*;

    fn cached_path_properties() -> EffectiveObjectProperties {
        EffectiveObjectProperties {
            transform: Transform2D::IDENTITY,
            style: Style::default(),
            appearance: 1.0,
            presence: true,
            reveal: 1.0,
            morph: 0.0,
            bounds: Some(Rect::new(Vec2::new(-1.0, -2.0), Vec2::new(3.0, 4.0))),
            bounds_basis: None,
        }
    }

    #[test]
    fn cached_path_bounds_follow_translation_and_survive_opacity_only_style() {
        let mut properties = cached_path_properties();
        properties.set_transform(Transform2D {
            translation: Vec2::new(5.0, -1.0),
            ..Transform2D::IDENTITY
        });
        assert_eq!(
            properties.bounds,
            Some(Rect::new(Vec2::new(4.0, -3.0), Vec2::new(8.0, 3.0)))
        );

        properties.set_style(Style {
            opacity: 0.25,
            ..properties.style
        });
        assert_eq!(
            properties.bounds,
            Some(Rect::new(Vec2::new(4.0, -3.0), Vec2::new(8.0, 3.0)))
        );
    }

    #[test]
    fn cached_path_bounds_are_invalidated_by_unreconstructable_spatial_changes() {
        let mut properties = cached_path_properties();
        properties.set_transform(Transform2D {
            rotation: 0.25,
            ..Transform2D::IDENTITY
        });
        assert_eq!(properties.bounds, None);

        let mut properties = cached_path_properties();
        properties.set_style(Style {
            stroke_width: properties.style.stroke_width + 1.0,
            ..properties.style
        });
        assert_eq!(properties.bounds, None);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum EffectiveBoundsBasis {
    Circle(f32),
    Rect(noon_core::Rect),
}

impl EffectiveBoundsBasis {
    fn from_frame(frame: &FrameState, object_index: usize) -> Option<Self> {
        let object = &frame.objects[object_index];
        Self::from_content(frame.render_geometry(object_index), object.text_bounds)
    }

    fn from_content(
        geometry: Option<&GeometryRef>,
        text_bounds: Option<noon_core::Rect>,
    ) -> Option<Self> {
        match geometry {
            Some(GeometryRef::Circle { radius }) => Some(Self::Circle(*radius)),
            Some(GeometryRef::Rectangle { size }) => {
                let half = *size * 0.5;
                Some(Self::Rect(noon_core::Rect::new(-half, half)))
            }
            Some(GeometryRef::Line { start, end }) => {
                noon_core::Rect::from_points([*start, *end]).map(Self::Rect)
            }
            Some(GeometryRef::VectorPath(_) | GeometryRef::External(_)) => None,
            None => text_bounds.map(Self::Rect),
        }
    }

    fn world_bounds(self, transform: Transform2D, style: Style) -> Option<noon_core::Rect> {
        let mut bounds = match self {
            Self::Circle(radius) => GeometryRef::circle(radius).world_bounds(transform)?,
            Self::Rect(local) => {
                let corners = [
                    local.min,
                    Vec2::new(local.min.x, local.max.y),
                    Vec2::new(local.max.x, local.min.y),
                    local.max,
                ];
                noon_core::Rect::from_points(corners.map(|point| transform.transform_point(point)))?
            }
        };
        if style.stroke.is_some() && style.stroke_width.is_finite() {
            let scale = transform.scale.x.abs().max(transform.scale.y.abs());
            let expansion = style.stroke_width.abs() * scale * 0.5;
            bounds.min.x -= expansion;
            bounds.min.y -= expansion;
            bounds.max.x += expansion;
            bounds.max.y += expansion;
        }
        (bounds.min.x.is_finite()
            && bounds.min.y.is_finite()
            && bounds.max.x.is_finite()
            && bounds.max.y.is_finite())
        .then_some(bounds)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FrameRowState {
    transform: Transform2D,
    style: Style,
    appearance: f32,
    presence: bool,
    reveal: f32,
    morph: f32,
    content_override: Option<ObjectContentRef>,
    render_geometry: Option<Arc<GeometryRef>>,
    render_transform: Option<Transform2D>,
}

impl FrameRowState {
    pub(crate) fn from_frame(frame: &FrameState, object_index: usize) -> Self {
        Self {
            transform: frame.objects[object_index].transform,
            style: frame.objects[object_index].style,
            appearance: frame.objects[object_index].appearance,
            presence: frame.presences[object_index],
            reveal: frame.reveals[object_index],
            morph: frame.morphs[object_index],
            content_override: None,
            render_geometry: frame.render_geometries[object_index].clone(),
            render_transform: frame.render_transforms[object_index],
        }
    }

    fn write_to_frame(self, frame: &mut FrameState, object_index: usize) {
        let object = &mut frame.objects[object_index];
        object.transform = self.transform;
        object.style = self.style;
        object.appearance = self.appearance;
        if let Some(content) = self.content_override {
            object.content = content;
        }
        frame.presences[object_index] = self.presence;
        frame.reveals[object_index] = self.reveal;
        frame.morphs[object_index] = self.morph;
        frame.render_geometries[object_index] = self.render_geometry;
        frame.render_transforms[object_index] = self.render_transform;
    }

    pub(crate) fn differs_from_frame(&self, frame: &FrameState, object_index: usize) -> bool {
        let object = &frame.objects[object_index];
        self.transform != object.transform
            || self.style != object.style
            || self.appearance != object.appearance
            || self.presence != frame.presences[object_index]
            || self.reveal != frame.reveals[object_index]
            || self.morph != frame.morphs[object_index]
            || self
                .content_override
                .as_ref()
                .is_some_and(|content| content != &object.content)
            || self.render_geometry != frame.render_geometries[object_index]
            || self.render_transform != frame.render_transforms[object_index]
    }

    fn properties(
        &self,
        bounds: Option<noon_core::Rect>,
        bounds_basis: Option<EffectiveBoundsBasis>,
    ) -> EffectiveObjectProperties {
        EffectiveObjectProperties {
            transform: self.transform,
            style: self.style,
            appearance: self.appearance,
            presence: self.presence,
            reveal: self.reveal,
            morph: self.morph,
            bounds,
            bounds_basis,
        }
    }

    fn spatially_differs_from_frame(&self, frame: &FrameState, object_index: usize) -> bool {
        let object = &frame.objects[object_index];
        self.transform != object.transform
            || self.style.stroke != object.style.stroke
            || self.style.stroke_width != object.style.stroke_width
            || self.content_override.is_some()
            || self.render_geometry != frame.render_geometries[object_index]
            || self.render_transform != frame.render_transforms[object_index]
    }

    fn as_mut<'a>(&'a mut self, base_content: &'a ObjectContentRef) -> FrameRowMut<'a> {
        FrameRowMut {
            content: FrameContentMut::Staged {
                base: base_content,
                content_override: &mut self.content_override,
            },
            transform: &mut self.transform,
            style: &mut self.style,
            appearance: &mut self.appearance,
            presence: &mut self.presence,
            reveal: &mut self.reveal,
            morph: &mut self.morph,
            render_geometry: &mut self.render_geometry,
            render_transform: &mut self.render_transform,
        }
    }
}

enum FrameContentMut<'a> {
    Direct(&'a mut ObjectContentRef),
    Staged {
        base: &'a ObjectContentRef,
        content_override: &'a mut Option<ObjectContentRef>,
    },
}

impl FrameContentMut<'_> {
    fn geometry_mut(&mut self) -> Option<&mut GeometryRef> {
        let content: &mut ObjectContentRef = match self {
            Self::Direct(content) => content,
            Self::Staged {
                base,
                content_override,
            } => (*content_override).get_or_insert_with(|| (**base).clone()),
        };
        let ObjectContentRef::Geometry(geometry) = content else {
            return None;
        };
        Some(geometry)
    }
}

struct FrameRowMut<'a> {
    content: FrameContentMut<'a>,
    transform: &'a mut Transform2D,
    style: &'a mut Style,
    appearance: &'a mut f32,
    presence: &'a mut bool,
    reveal: &'a mut f32,
    morph: &'a mut f32,
    render_geometry: &'a mut Option<Arc<GeometryRef>>,
    render_transform: &'a mut Option<Transform2D>,
}

fn frame_row_mut(frame: &mut FrameState, object_index: usize) -> FrameRowMut<'_> {
    let object = &mut frame.objects[object_index];
    FrameRowMut {
        content: FrameContentMut::Direct(&mut object.content),
        transform: &mut object.transform,
        style: &mut object.style,
        appearance: &mut object.appearance,
        presence: &mut frame.presences[object_index],
        reveal: &mut frame.reveals[object_index],
        morph: &mut frame.morphs[object_index],
        render_geometry: &mut frame.render_geometries[object_index],
        render_transform: &mut frame.render_transforms[object_index],
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrameChanges {
    all: bool,
    object_indices: Vec<usize>,
    added_indices: Vec<usize>,
    removed_indices: Vec<usize>,
}

impl FrameChanges {
    pub fn all() -> Self {
        Self {
            all: true,
            object_indices: Vec::new(),
            added_indices: Vec::new(),
            removed_indices: Vec::new(),
        }
    }

    pub fn objects(mut object_indices: Vec<usize>) -> Self {
        sort_dedup(&mut object_indices);
        Self {
            all: false,
            object_indices,
            added_indices: Vec::new(),
            removed_indices: Vec::new(),
        }
    }

    pub fn structural(mut added_indices: Vec<usize>, mut removed_indices: Vec<usize>) -> Self {
        sort_dedup(&mut added_indices);
        sort_dedup(&mut removed_indices);
        let mut object_indices = added_indices.clone();
        object_indices.extend_from_slice(&removed_indices);
        sort_dedup(&mut object_indices);
        Self {
            all: false,
            object_indices,
            added_indices,
            removed_indices,
        }
    }

    pub fn with_structure(
        mut object_indices: Vec<usize>,
        mut added_indices: Vec<usize>,
        mut removed_indices: Vec<usize>,
    ) -> Self {
        sort_dedup(&mut added_indices);
        sort_dedup(&mut removed_indices);
        object_indices.extend_from_slice(&added_indices);
        object_indices.extend_from_slice(&removed_indices);
        sort_dedup(&mut object_indices);
        Self {
            all: false,
            object_indices,
            added_indices,
            removed_indices,
        }
    }

    pub(crate) fn contains_object(&self, object_index: usize) -> bool {
        self.all || self.object_indices.binary_search(&object_index).is_ok()
    }

    pub(crate) fn remove_unchanged_object(&mut self, object_index: usize) {
        if self.all
            || self.added_indices.binary_search(&object_index).is_ok()
            || self.removed_indices.binary_search(&object_index).is_ok()
        {
            return;
        }
        if let Ok(position) = self.object_indices.binary_search(&object_index) {
            self.object_indices.remove(position);
        }
    }

    pub const fn is_all(&self) -> bool {
        self.all
    }

    pub fn object_indices(&self) -> &[usize] {
        &self.object_indices
    }

    pub fn added_indices(&self) -> &[usize] {
        &self.added_indices
    }

    pub fn removed_indices(&self) -> &[usize] {
        &self.removed_indices
    }

    pub const fn is_structural(&self) -> bool {
        !self.added_indices.is_empty() || !self.removed_indices.is_empty()
    }

    pub const fn is_empty(&self) -> bool {
        !self.all && self.object_indices.is_empty()
    }

    fn invalidate_all(&mut self) {
        self.all = true;
        self.object_indices.clear();
        self.added_indices.clear();
        self.removed_indices.clear();
    }

    fn insert(&mut self, object_index: usize) {
        insert_sorted_unique(&mut self.object_indices, object_index);
    }

    fn insert_added(&mut self, object_index: usize) {
        if self.all {
            return;
        }
        insert_sorted_unique(&mut self.object_indices, object_index);
        insert_sorted_unique(&mut self.added_indices, object_index);
    }

    fn insert_removed(&mut self, object_index: usize) {
        if self.all {
            return;
        }
        insert_sorted_unique(&mut self.object_indices, object_index);
        insert_sorted_unique(&mut self.removed_indices, object_index);
    }
}

fn sort_dedup(values: &mut Vec<usize>) {
    values.sort_unstable();
    values.dedup();
}

fn insert_sorted_unique(values: &mut Vec<usize>, value: usize) {
    match values.binary_search(&value) {
        Ok(_) => {}
        Err(position) => values.insert(position, value),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EvaluationStats {
    pub groups_evaluated: usize,
    pub tracks_advanced: usize,
    pub binary_search_steps: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EvaluationError {
    InvalidTime(f64),
    NonMonotonicPreparedAdvance { current: f64, requested: f64 },
    FrameEpochExhausted(noon_core::FrameEpoch),
    RequiredCallbackPending,
    RequiredCallbackBarrier,
    Reactive(noon_core::ReactiveError),
}

impl std::fmt::Display for EvaluationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTime(time) => write!(formatter, "invalid scene time {time}"),
            Self::NonMonotonicPreparedAdvance { current, requested } => write!(
                formatter,
                "required callback preparation cannot move backward from {current} to {requested}"
            ),
            Self::FrameEpochExhausted(epoch) => {
                write!(formatter, "frame epoch exhausted after {epoch:?}")
            }
            Self::RequiredCallbackPending => {
                formatter.write_str("a required callback publication is pending")
            }
            Self::RequiredCallbackBarrier => formatter
                .write_str("semantic host callbacks require callback-aware session advancement"),
            Self::Reactive(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for EvaluationError {}

#[derive(Clone, Debug)]
struct TrackGroup {
    channel: CompiledChannelKey,
    cursor: usize,
    mapped: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuntimePatchStats {
    pub channels_relowered: usize,
    pub scheduler_events_removed: usize,
    pub scheduler_events_inserted: usize,
    pub objects_recomputed: usize,
    pub groups_evaluated: usize,
    pub object_slots_appended: usize,
    pub object_slots_retired: usize,
    pub track_locators_removed: usize,
    pub full_group_rebuilds: usize,
    pub full_seeks: usize,
}

static NEXT_RUNTIME_IDENTITY: AtomicU64 = AtomicU64::new(1);

/// Process-local identity of one mutable runtime incarnation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RuntimeIdentity(u64);

impl RuntimeIdentity {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    fn fresh() -> Self {
        let raw = NEXT_RUNTIME_IDENTITY
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .expect("runtime identity space exhausted");
        Self(raw)
    }
}

#[derive(Debug)]
pub struct SceneInstance {
    identity: RuntimeIdentity,
    compiled: CompiledScene,
    frame: FrameState,
    groups: BTreeMap<CompiledChannelKey, TrackGroup>,
    timeline_scheduler: TimelineEventScheduler,
    last_stats: EvaluationStats,
    last_patch_stats: RuntimePatchStats,
    changes: FrameChanges,
    spatial_changes: FrameChanges,
    reactive: Option<ReactiveRuntime>,
    last_reactive_stats: ReactiveRuntimeStats,
    publication: PublicationContext,
    effective_driver_rows: BTreeSet<usize>,
}

impl Clone for SceneInstance {
    fn clone(&self) -> Self {
        Self {
            identity: RuntimeIdentity::fresh(),
            compiled: self.compiled.clone(),
            frame: self.frame.clone(),
            groups: self.groups.clone(),
            timeline_scheduler: self.timeline_scheduler.clone(),
            last_stats: self.last_stats,
            last_patch_stats: self.last_patch_stats,
            changes: self.changes.clone(),
            spatial_changes: self.spatial_changes.clone(),
            reactive: self.reactive.clone(),
            last_reactive_stats: self.last_reactive_stats,
            publication: self.publication,
            effective_driver_rows: self.effective_driver_rows.clone(),
        }
    }
}

impl SceneInstance {
    pub fn preflight_reconcilable_track_additions(
        &self,
        tracks: &[TrackDefinition],
    ) -> Result<(), CompilePatchError> {
        self.compiled.preflight_reconcilable_track_additions(tracks)
    }

    pub fn new(compiled: CompiledScene) -> Self {
        let frame = base_frame(&compiled, 0.0);
        let groups = build_groups(&compiled);
        let timeline_scheduler = TimelineEventScheduler::from_compiled(&compiled);
        let mut instance = Self {
            identity: RuntimeIdentity::fresh(),
            compiled,
            frame,
            groups,
            timeline_scheduler,
            last_stats: EvaluationStats::default(),
            last_patch_stats: RuntimePatchStats::default(),
            changes: FrameChanges::all(),
            spatial_changes: FrameChanges::all(),
            reactive: None,
            last_reactive_stats: ReactiveRuntimeStats::default(),
            publication: PublicationContext::default(),
            effective_driver_rows: BTreeSet::new(),
        };
        instance.seek_unchecked(0.0);
        instance
    }

    pub const fn runtime_identity(&self) -> RuntimeIdentity {
        self.identity
    }

    pub fn frame(&self) -> &FrameState {
        &self.frame
    }

    pub fn effective_properties_at(
        &self,
        object_index: usize,
        cached_bounds: Option<noon_core::Rect>,
    ) -> Option<EffectiveObjectProperties> {
        self.object_slot_is_live(object_index).then(|| {
            EffectiveObjectProperties::from_frame(&self.frame, object_index, cached_bounds)
        })
    }

    pub const fn last_stats(&self) -> EvaluationStats {
        self.last_stats
    }

    pub const fn last_patch_stats(&self) -> RuntimePatchStats {
        self.last_patch_stats
    }

    pub fn take_frame_changes(&mut self) -> FrameChanges {
        std::mem::take(&mut self.changes)
    }

    /// Borrow accumulated renderer invalidation without consuming it.
    ///
    /// This is intended for bounded publication diagnostics. Normal renderer
    /// preparation continues to consume changes through `take_renderer_publication`.
    pub const fn frame_changes(&self) -> &FrameChanges {
        &self.changes
    }

    /// Consume one renderer publication atomically with its accumulated changes.
    pub fn take_renderer_publication(&mut self) -> RendererPublication<'_> {
        let changes = self.take_frame_changes();
        RendererPublication::new(
            self.publication,
            &self.frame,
            changes,
            self.compiled.text_resources(),
            self.compiled.font_resources(),
            self.compiled.geometry_resources(),
        )
    }

    /// Consume derived spatial invalidation for the execution-session index owner.
    pub fn take_spatial_changes(&mut self) -> FrameChanges {
        std::mem::take(&mut self.spatial_changes)
    }

    pub(crate) fn mark_changed(&mut self, object_index: usize) {
        self.changes.insert(object_index);
        self.spatial_changes.insert(object_index);
    }

    pub(crate) fn mark_added(&mut self, object_index: usize) {
        self.changes.insert_added(object_index);
        self.spatial_changes.insert_added(object_index);
    }

    pub(crate) fn mark_removed(&mut self, object_index: usize) {
        self.changes.insert_removed(object_index);
        self.spatial_changes.insert_removed(object_index);
    }

    pub(crate) fn mark_all_changed(&mut self) {
        self.changes.invalidate_all();
        self.spatial_changes.invalidate_all();
    }

    pub fn contains_object(&self, id: ObjectId) -> bool {
        self.compiled.object_index(id).is_some()
    }

    pub fn frame_index_for_object(&self, id: ObjectId) -> Option<usize> {
        self.compiled.object_index(id).map(|index| index as usize)
    }

    pub fn object_has_effective_driver(&self, id: ObjectId) -> bool {
        self.frame_index_for_object(id)
            .is_some_and(|index| self.effective_driver_rows.contains(&index))
    }

    pub fn text_resources(&self) -> &impl noon_core::TextResourceLookup {
        self.compiled.text_resources()
    }

    pub fn font_resources(&self) -> &impl noon_core::FontResourceLookup {
        self.compiled.font_resources()
    }

    pub fn geometry_resources(&self) -> &impl noon_core::GeometryResourceLookup {
        self.compiled.geometry_resources()
    }

    pub fn object_slot_is_live(&self, object_index: usize) -> bool {
        let Ok(object_index) = u32::try_from(object_index) else {
            return false;
        };
        self.compiled.object_slot_is_live(object_index)
    }

    pub fn evaluate(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        if !time.is_finite() {
            return Err(EvaluationError::InvalidTime(time));
        }
        if time >= self.frame.time {
            self.advance_unchecked(time);
        } else {
            self.seek_unchecked(time);
            self.publish_effective_change();
        }
        Ok(&self.frame)
    }

    pub fn seek(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        if !time.is_finite() {
            return Err(EvaluationError::InvalidTime(time));
        }
        let previous_time = self.frame.time;
        self.seek_unchecked(time);
        if self.frame.time != previous_time {
            self.publish_effective_change();
        }
        Ok(&self.frame)
    }

    pub fn advance_to(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        if !time.is_finite() {
            return Err(EvaluationError::InvalidTime(time));
        }
        let previous_time = self.frame.time;
        if time < previous_time {
            self.seek_unchecked(time);
            if self.frame.time != previous_time {
                self.publish_effective_change();
            }
        } else {
            self.advance_unchecked(time);
        }
        Ok(&self.frame)
    }

    pub fn apply_patch(&mut self, patch: &ScenePatch) -> Result<&FrameState, CompilePatchError> {
        let patch = ExecutionPatch::decode(patch);
        self.apply_execution_patch(&patch)
    }

    pub fn apply_execution_patch(
        &mut self,
        patch: &ExecutionPatch,
    ) -> Result<&FrameState, CompilePatchError> {
        if !self.compiled.patch_changes_execution(patch) {
            self.last_patch_stats = RuntimePatchStats::default();
            return Ok(&self.frame);
        }
        self.apply_patch_unpublished(patch)?;
        self.publish_execution_change();
        Ok(&self.frame)
    }

    fn apply_patch_unpublished(
        &mut self,
        patch: &ExecutionPatch,
    ) -> Result<&FrameState, CompilePatchError> {
        self.last_patch_stats = RuntimePatchStats::default();
        if matches!(
            patch,
            ExecutionPatch::SetContent { .. }
                | ExecutionPatch::SetTransform { .. }
                | ExecutionPatch::SetStyle { .. }
        ) {
            self.apply_value_patch(patch)?;
            return Ok(&self.frame);
        }
        if matches!(
            patch,
            ExecutionPatch::AddTrack(_)
                | ExecutionPatch::ReplaceTrack(_)
                | ExecutionPatch::RemoveTrack(_)
                | ExecutionPatch::ReconcileTrack { .. }
        ) {
            self.apply_timeline_patch(patch)?;
            return Ok(&self.frame);
        }

        if matches!(
            patch,
            ExecutionPatch::CreateObject(_) | ExecutionPatch::RemoveObject(_)
        ) {
            self.apply_structural_patch(patch)?;
            return Ok(&self.frame);
        }

        unreachable!("all ExecutionPatch variants are handled above")
    }

    fn apply_structural_patch(&mut self, patch: &ExecutionPatch) -> Result<(), CompilePatchError> {
        let removed = match patch {
            ExecutionPatch::RemoveObject(object) => {
                let object_index = self
                    .compiled
                    .object_index(*object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                Some((object_index, self.compiled.object_channels(*object)))
            }
            ExecutionPatch::CreateObject(_) => None,
            _ => unreachable!("structural patch helper accepts only create/remove"),
        };

        let compiled_stats = self.compiled.apply_execution_patch_with_stats(patch)?;
        let mut patch_stats = RuntimePatchStats {
            object_slots_appended: compiled_stats.object_slots_appended,
            object_slots_retired: compiled_stats.object_slots_retired,
            track_locators_removed: compiled_stats.track_locators_removed,
            ..RuntimePatchStats::default()
        };

        match patch {
            ExecutionPatch::CreateObject(object) => {
                let object_index = self
                    .compiled
                    .object_index(object.id)
                    .expect("compiled create must expose its appended slot")
                    as usize;
                debug_assert_eq!(object_index, self.frame.objects.len());
                append_object_frame(&self.compiled, &mut self.frame, object_index);
                self.rebind_reactive_object(object.id, object_index);
                self.reapply_reactive_for_object(object_index);
                self.mark_added(object_index);
            }
            ExecutionPatch::RemoveObject(_) => {
                let (object_index, old_channels) = removed.expect("remove context captured above");
                for channel in old_channels {
                    let scheduler_stats = self.timeline_scheduler.relower_channel(channel, &[]);
                    patch_stats.channels_relowered += scheduler_stats.groups_relowered;
                    patch_stats.scheduler_events_removed += scheduler_stats.events_removed;
                    patch_stats.scheduler_events_inserted += scheduler_stats.events_inserted;
                    self.groups.remove(&channel);
                }
                let object_index = object_index as usize;
                self.frame.presences[object_index] = false;
                self.frame.render_geometries[object_index] = None;
                self.frame.render_transforms[object_index] = None;
                self.mark_removed(object_index);
            }
            _ => unreachable!("structural patch helper accepts only create/remove"),
        }

        self.last_stats = EvaluationStats::default();
        self.last_patch_stats = patch_stats;
        Ok(())
    }

    fn apply_timeline_patch(&mut self, patch: &ExecutionPatch) -> Result<(), CompilePatchError> {
        if let ExecutionPatch::ReconcileTrack { track, .. } = patch {
            let channel = self
                .compiled
                .channel_for_track(*track)
                .ok_or(CompilePatchError::UnknownTrack(*track))?;
            self.compiled.apply_execution_patch(patch)?;
            let mut evaluation = EvaluationStats::default();
            self.relower_object(
                channel.object_index as usize,
                self.frame.time,
                &mut evaluation,
            );
            self.reapply_reactive_for_object(channel.object_index as usize);
            self.last_stats = evaluation;
            self.last_patch_stats = RuntimePatchStats {
                objects_recomputed: 1,
                groups_evaluated: evaluation.groups_evaluated,
                ..RuntimePatchStats::default()
            };
            self.mark_changed(channel.object_index as usize);
            return Ok(());
        }
        let old_channel = match patch {
            ExecutionPatch::ReplaceTrack(track) => self.compiled.channel_for_track(track.id),
            ExecutionPatch::RemoveTrack(id) => self.compiled.channel_for_track(*id),
            ExecutionPatch::AddTrack(_) => None,
            ExecutionPatch::ReconcileTrack { .. } => unreachable!("handled above"),
            _ => unreachable!("timeline patch helper accepts only track mutations"),
        };
        self.compiled.apply_execution_patch(patch)?;
        let new_channel = match patch {
            ExecutionPatch::AddTrack(track) | ExecutionPatch::ReplaceTrack(track) => {
                self.compiled.channel_for_track(track.id)
            }
            ExecutionPatch::RemoveTrack(_) => None,
            ExecutionPatch::ReconcileTrack { .. } => unreachable!("handled above"),
            _ => unreachable!("timeline patch helper accepts only track mutations"),
        };

        let mut affected_channels = [None, None];
        push_unique_channel(&mut affected_channels, old_channel);
        push_unique_channel(&mut affected_channels, new_channel);
        let mut patch_stats = RuntimePatchStats::default();
        for channel in affected_channels.into_iter().flatten() {
            let tracks = self.compiled.channel_tracks(channel);
            let scheduler_stats = self.timeline_scheduler.relower_channel(channel, tracks);
            patch_stats.channels_relowered += scheduler_stats.groups_relowered;
            patch_stats.scheduler_events_removed += scheduler_stats.events_removed;
            patch_stats.scheduler_events_inserted += scheduler_stats.events_inserted;
            if tracks.is_empty() {
                self.groups.remove(&channel);
            } else {
                let mapped = tracks.iter().any(|track| !track.time_map.is_identity());
                self.groups
                    .entry(channel)
                    .and_modify(|group| group.mapped = mapped)
                    .or_insert(TrackGroup {
                        channel,
                        cursor: 0,
                        mapped,
                    });
            }
        }

        let mut affected_objects = [None, None];
        push_unique_object(
            &mut affected_objects,
            old_channel.map(|channel| channel.object_index as usize),
        );
        push_unique_object(
            &mut affected_objects,
            new_channel.map(|channel| channel.object_index as usize),
        );
        let mut evaluation = EvaluationStats::default();
        for object_index in affected_objects.into_iter().flatten() {
            self.relower_object(object_index, self.frame.time, &mut evaluation);
            self.reapply_reactive_for_object(object_index);
            self.mark_changed(object_index);
            patch_stats.objects_recomputed += 1;
        }
        patch_stats.groups_evaluated = evaluation.groups_evaluated;
        self.last_stats = evaluation;
        self.last_patch_stats = patch_stats;
        Ok(())
    }

    fn apply_value_patch(&mut self, patch: &ExecutionPatch) -> Result<(), CompilePatchError> {
        let object = match patch {
            ExecutionPatch::SetContent { object, .. }
            | ExecutionPatch::SetTransform { object, .. }
            | ExecutionPatch::SetStyle { object, .. } => *object,
            _ => unreachable!("value patch helper only accepts object-local property patches"),
        };
        let index = self
            .compiled
            .object_index(object)
            .ok_or(CompilePatchError::UnknownObject(object))? as usize;
        let before = self.frame.objects[index].clone();
        self.compiled.apply_execution_patch(patch)?;

        match patch {
            ExecutionPatch::SetContent {
                content,
                text_bounds,
                ..
            } => {
                self.frame.objects[index].content = content.clone();
                self.frame.objects[index].text_bounds = *text_bounds;
                // Host callbacks run after ordinary timeline/reactive evaluation for the frame.
                // Clearing a transient render override makes authored content authoritative for
                // this phase without rebuilding unrelated runtime slots.
                self.frame.render_geometries[index] = None;
                self.frame.render_transforms[index] = None;
            }
            ExecutionPatch::SetTransform { transform, .. } => {
                self.frame.release_render_transform(index);
                self.frame.objects[index].transform = *transform;
                self.reapply_properties(
                    index,
                    &[
                        Property::Transform,
                        Property::Position,
                        Property::Rotation,
                        Property::Scale,
                    ],
                );
            }
            ExecutionPatch::SetStyle { style, .. } => {
                self.frame.objects[index].style = *style;
                self.reapply_properties(
                    index,
                    &[
                        Property::Transform,
                        Property::Fill,
                        Property::Stroke,
                        Property::Opacity,
                    ],
                );
            }
            _ => unreachable!("value patch helper only accepts object-local property patches"),
        }
        self.reapply_reactive_for_object(index);
        if self.frame.objects[index] != before {
            self.mark_changed(index);
        }
        Ok(())
    }

    fn reapply_properties(&mut self, object_index: usize, properties: &[Property]) {
        let time = self.frame.time;
        let mut stats = EvaluationStats::default();
        for property in properties {
            let channel = CompiledChannelKey::new(object_index as u32, *property);
            let tracks = self.compiled.channel_tracks(channel);
            let object = &self.compiled.objects()[object_index];
            let Some(group) = self.groups.get_mut(&channel) else {
                continue;
            };
            group.cursor = upper_bound_start(tracks, time, &mut stats.binary_search_steps);
            apply_group(
                &self.compiled,
                &mut self.frame,
                tracks,
                group,
                time,
                object.base_transform,
                object.base_style,
            );
            stats.groups_evaluated += 1;
        }
        self.last_stats = stats;
    }

    fn relower_object(&mut self, object_index: usize, time: f64, stats: &mut EvaluationStats) {
        reset_object_frame(&self.compiled, &mut self.frame, object_index, time);
        for property in PROPERTY_ORDER {
            let channel = CompiledChannelKey::new(object_index as u32, property);
            let tracks = self.compiled.channel_tracks(channel);
            let object = &self.compiled.objects()[object_index];
            let Some(group) = self.groups.get_mut(&channel) else {
                continue;
            };
            group.cursor = upper_bound_start(tracks, time, &mut stats.binary_search_steps);
            apply_group(
                &self.compiled,
                &mut self.frame,
                tracks,
                group,
                time,
                object.base_transform,
                object.base_style,
            );
            stats.groups_evaluated += 1;
        }
    }

    fn seek_unchecked(&mut self, time: f64) {
        self.frame = base_frame(&self.compiled, time);
        self.effective_driver_rows.clear();
        self.mark_all_changed();
        let mut stats = EvaluationStats::default();

        for group in self.groups.values_mut() {
            let tracks = self.compiled.channel_tracks(group.channel);
            let object = &self.compiled.objects()[group.channel.object_index as usize];
            group.cursor = upper_bound_start(tracks, time, &mut stats.binary_search_steps);
            apply_group(
                &self.compiled,
                &mut self.frame,
                tracks,
                group,
                time,
                object.base_transform,
                object.base_style,
            );
            stats.groups_evaluated += 1;
        }
        self.timeline_scheduler.seek(time);

        self.reapply_reactive();
        self.last_stats = stats;
    }

    fn advance_unchecked(&mut self, time: f64) {
        if !self.effective_driver_rows.is_empty() {
            let prepared = self
                .prepare_advance_to(time)
                .expect("unchecked forward evaluation receives a valid monotonic time");
            let effective = self
                .prepare_effective_property_batch(&[])
                .expect("empty effective batch is valid");
            self.commit_prepared_frame(prepared, effective)
                .expect("immediate evaluation cannot stale its own prepared frame");
            return;
        }

        let previous_time = self.frame.time;
        self.frame.time = time;
        let requested_count = self.timeline_scheduler.advance(time);
        let mut stats = EvaluationStats::default();

        for request_index in 0..requested_count {
            let channel = self.timeline_scheduler.requested()[request_index];
            let tracks = self.compiled.channel_tracks(channel);
            let object = &self.compiled.objects()[channel.object_index as usize];
            let Some(group) = self.groups.get_mut(&channel) else {
                continue;
            };
            while group.cursor < tracks.len() && tracks[group.cursor].timing.start_time <= time {
                group.cursor += 1;
                stats.tracks_advanced += 1;
            }
            if apply_group(
                &self.compiled,
                &mut self.frame,
                tracks,
                group,
                time,
                object.base_transform,
                object.base_style,
            ) {
                self.mark_changed(channel.object_index as usize);
            }
            stats.groups_evaluated += 1;
        }

        self.last_stats = stats;
        if self.frame.time != previous_time {
            self.publish_effective_change();
        }
    }
}

fn base_frame(compiled: &CompiledScene, time: f64) -> FrameState {
    let appearances = initial_scalar_property(
        compiled,
        compiled.objects().len(),
        Property::Appearance,
        1.0,
    );
    let objects: Vec<_> = compiled
        .objects()
        .iter()
        .enumerate()
        .map(|(index, object)| FrameObjectState {
            id: object.id,
            content: object.content.clone(),
            text_bounds: object.text_bounds,
            transform: affine_base_at_time(compiled, index, object.base_transform, time),
            style: object.base_style,
            appearance: appearances[index],
        })
        .collect();
    let mut presences = initial_bool_property(compiled, objects.len(), Property::Presence, true);
    for (index, object) in compiled.objects().iter().enumerate() {
        if !object.live {
            presences[index] = false;
        }
    }
    FrameState {
        time,
        presences,
        reveals: initial_scalar_property(compiled, objects.len(), Property::Reveal, 1.0),
        morphs: initial_scalar_property(compiled, objects.len(), Property::Morph, 0.0),
        render_geometries: vec![None; objects.len()],
        render_transforms: vec![None; objects.len()],
        objects,
    }
}

fn initial_bool_property(
    compiled: &CompiledScene,
    object_count: usize,
    property: Property,
    default: bool,
) -> Vec<bool> {
    let mut values = vec![default; object_count];
    let mut initialized = vec![false; object_count];
    for track in compiled
        .tracks_iter()
        .filter(|track| track.property == property)
    {
        let index = track.object_index as usize;
        if initialized[index] {
            continue;
        }
        let TrackValues::Bool { from, .. } = &track.values else {
            unreachable!("compiled bool property must contain bool values");
        };
        values[index] = *from;
        initialized[index] = true;
    }
    values
}

fn initial_scalar_property(
    compiled: &CompiledScene,
    object_count: usize,
    property: Property,
    default: f32,
) -> Vec<f32> {
    let mut values = vec![default; object_count];
    let mut initialized = vec![false; object_count];
    for track in compiled
        .tracks_iter()
        .filter(|track| track.property == property)
    {
        let index = track.object_index as usize;
        if initialized[index] {
            continue;
        }
        let (TrackValues::Scalar { from, .. } | TrackValues::PreparedMorph { from, .. }) =
            &track.values
        else {
            unreachable!("compiled scalar property must contain scalar values");
        };
        values[index] = from.clamp(0.0, 1.0);
        initialized[index] = true;
    }
    values
}

const PROPERTY_ORDER: [Property; 11] = [
    Property::Presence,
    Property::Transform,
    Property::Position,
    Property::Rotation,
    Property::Scale,
    Property::Fill,
    Property::Stroke,
    Property::Opacity,
    Property::Appearance,
    Property::Reveal,
    Property::Morph,
];

fn build_groups(compiled: &CompiledScene) -> BTreeMap<CompiledChannelKey, TrackGroup> {
    let mut groups = BTreeMap::new();
    for channel in compiled.channels() {
        let channel_tracks = compiled.channel_tracks(channel);
        let mapped = channel_tracks
            .iter()
            .any(|track| !track.time_map.is_identity());
        groups.insert(
            channel,
            TrackGroup {
                channel,
                cursor: 0,
                mapped,
            },
        );
    }
    groups
}

fn push_unique_channel(
    slots: &mut [Option<CompiledChannelKey>; 2],
    channel: Option<CompiledChannelKey>,
) {
    let Some(channel) = channel else {
        return;
    };
    if slots.iter().flatten().any(|existing| *existing == channel) {
        return;
    }
    if let Some(slot) = slots.iter_mut().find(|slot| slot.is_none()) {
        *slot = Some(channel);
    }
}

fn push_unique_object(slots: &mut [Option<usize>; 2], object: Option<usize>) {
    let Some(object) = object else {
        return;
    };
    if slots.iter().flatten().any(|existing| *existing == object) {
        return;
    }
    if let Some(slot) = slots.iter_mut().find(|slot| slot.is_none()) {
        *slot = Some(object);
    }
}

fn append_object_frame(compiled: &CompiledScene, frame: &mut FrameState, object_index: usize) {
    debug_assert_eq!(object_index, frame.objects.len());
    let object = &compiled.objects()[object_index];
    debug_assert!(object.live);
    frame.objects.push(FrameObjectState {
        id: object.id,
        content: object.content.clone(),
        text_bounds: object.text_bounds,
        transform: affine_base_at_time(compiled, object_index, object.base_transform, frame.time),
        style: object.base_style,
        appearance: initial_channel_scalar(compiled, object_index, Property::Appearance, 1.0),
    });
    frame.presences.push(initial_channel_bool(
        compiled,
        object_index,
        Property::Presence,
        true,
    ));
    frame.reveals.push(initial_channel_scalar(
        compiled,
        object_index,
        Property::Reveal,
        1.0,
    ));
    frame.morphs.push(initial_channel_scalar(
        compiled,
        object_index,
        Property::Morph,
        0.0,
    ));
    frame.render_geometries.push(None);
    frame.render_transforms.push(None);
}

fn reset_object_frame(
    compiled: &CompiledScene,
    frame: &mut FrameState,
    object_index: usize,
    time: f64,
) {
    let object = &compiled.objects()[object_index];
    frame.objects[object_index] = FrameObjectState {
        id: object.id,
        content: object.content.clone(),
        text_bounds: object.text_bounds,
        transform: affine_base_at_time(compiled, object_index, object.base_transform, time),
        style: object.base_style,
        appearance: initial_channel_scalar(compiled, object_index, Property::Appearance, 1.0),
    };
    frame.presences[object_index] =
        initial_channel_bool(compiled, object_index, Property::Presence, true);
    frame.reveals[object_index] =
        initial_channel_scalar(compiled, object_index, Property::Reveal, 1.0);
    frame.morphs[object_index] =
        initial_channel_scalar(compiled, object_index, Property::Morph, 0.0);
    frame.render_geometries[object_index] = None;
    frame.render_transforms[object_index] = None;
}

fn initial_channel_bool(
    compiled: &CompiledScene,
    object_index: usize,
    property: Property,
    default: bool,
) -> bool {
    let channel = CompiledChannelKey::new(object_index as u32, property);
    let Some(track) = compiled.channel_tracks(channel).first() else {
        return default;
    };
    let TrackValues::Bool { from, .. } = &track.values else {
        unreachable!("compiled bool property must contain bool values");
    };
    *from
}

fn initial_channel_scalar(
    compiled: &CompiledScene,
    object_index: usize,
    property: Property,
    default: f32,
) -> f32 {
    let channel = CompiledChannelKey::new(object_index as u32, property);
    let Some(track) = compiled.channel_tracks(channel).first() else {
        return default;
    };
    let (TrackValues::Scalar { from, .. } | TrackValues::PreparedMorph { from, .. }) =
        &track.values
    else {
        unreachable!("compiled scalar property must contain scalar values");
    };
    from.clamp(0.0, 1.0)
}

fn affine_base_at_time(
    compiled: &CompiledScene,
    object_index: usize,
    mut transform: Transform2D,
    time: f64,
) -> Transform2D {
    for property in [Property::Position, Property::Rotation, Property::Scale] {
        let channel = CompiledChannelKey::new(object_index as u32, property);
        let tracks = compiled.channel_tracks(channel);
        let Some(track) = tracks.first() else {
            continue;
        };
        if tracks.last().is_some_and(|last| {
            last.reconciled && time >= last.timing.start_time + last.timing.duration
        }) {
            continue;
        }
        match (property, &track.values) {
            (Property::Position, TrackValues::Vec2 { from, .. }) => transform.translation = *from,
            (Property::Rotation, TrackValues::Scalar { from, .. }) => transform.rotation = *from,
            (Property::Scale, TrackValues::Vec2 { from, .. }) => transform.scale = *from,
            _ => unreachable!("validated affine track must carry matching values"),
        }
    }
    transform
}

fn upper_bound_start(tracks: &[CompiledTrack], time: f64, steps: &mut usize) -> usize {
    let mut low = 0;
    let mut high = tracks.len();
    while low < high {
        *steps += 1;
        let middle = low + (high - low) / 2;
        if tracks[middle].timing.start_time <= time {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    low
}

fn apply_group(
    compiled: &CompiledScene,
    frame: &mut FrameState,
    tracks: &[CompiledTrack],
    group: &TrackGroup,
    time: f64,
    base_transform: Transform2D,
    base_style: Style,
) -> bool {
    let object_index = group.channel.object_index as usize;
    apply_group_to_row(
        compiled,
        frame_row_mut(frame, object_index),
        tracks,
        group,
        time,
        base_transform,
        base_style,
    )
}

fn apply_group_to_row(
    compiled: &CompiledScene,
    mut row: FrameRowMut<'_>,
    tracks: &[CompiledTrack],
    group: &TrackGroup,
    time: f64,
    base_transform: Transform2D,
    base_style: Style,
) -> bool {
    if group.cursor == 0 {
        return false;
    }
    if group.channel.property == Property::Presence {
        let track = &tracks[group.cursor - 1];
        let TrackValues::Bool { to, .. } = &track.values else {
            unreachable!("compiled Presence track must contain bool values");
        };
        let changed = *row.presence != *to;
        *row.presence = *to;
        return changed;
    }

    let selected = if group.mapped {
        tracks[..group.cursor]
            .iter()
            .rev()
            .find_map(|track| mapped_track_progress(track, time).map(|progress| (track, progress)))
    } else {
        let track = &tracks[group.cursor - 1];
        Some((track, track_progress(track, time)))
    };
    let Some((track, progress)) = selected else {
        return false;
    };
    let preserve_render_frame =
        matches!(
            group.channel.property,
            Property::Position | Property::Rotation | Property::Scale
        ) && prepared_morph_owns_render_frame(compiled, group.channel.object_index, time);
    if track.reconciled && time >= track.timing.start_time + track.timing.duration {
        if group.channel.property == Property::Morph
            && matches!(track.values, TrackValues::PreparedMorph { .. })
        {
            let changed = *row.morph != 0.0
                || row.render_geometry.is_some()
                || row.render_transform.is_some();
            *row.morph = 0.0;
            *row.render_geometry = None;
            *row.render_transform = None;
            return changed;
        }
        let base = match group.channel.property {
            Property::Position => Some(EvaluatedValue::Vec2(base_transform.translation)),
            Property::Rotation => Some(EvaluatedValue::Scalar(base_transform.rotation)),
            Property::Scale => Some(EvaluatedValue::Vec2(base_transform.scale)),
            Property::Fill => Some(EvaluatedValue::Color(base_style.fill)),
            Property::Stroke => Some(EvaluatedValue::Color(base_style.stroke)),
            Property::Opacity => Some(EvaluatedValue::Scalar(base_style.opacity)),
            Property::Appearance | Property::Reveal => Some(EvaluatedValue::Scalar(1.0)),
            Property::Morph => Some(EvaluatedValue::Scalar(0.0)),
            Property::Presence | Property::Transform => None,
        };
        return base.is_some_and(|value| {
            apply_evaluated_value(
                &mut row,
                group.channel.property,
                value,
                preserve_render_frame,
            )
        });
    }

    if group.channel.property == Property::Transform {
        return apply_transform_track(&mut row, track, progress);
    }
    if group.channel.property == Property::Morph
        && matches!(track.values, TrackValues::PreparedMorph { .. })
    {
        return apply_prepared_morph_track(&mut row, track, progress);
    }
    let value = interpolate(track, progress);
    apply_evaluated_value(
        &mut row,
        group.channel.property,
        value,
        preserve_render_frame,
    )
}

fn apply_effective_property_to_row(row: FrameRowMut<'_>, write: EffectivePropertyWrite) {
    match write {
        EffectivePropertyWrite::Transform { transform, .. } => {
            release_render_transform(row.render_geometry, row.render_transform, *row.transform);
            *row.transform = transform;
        }
        EffectivePropertyWrite::Style { style, .. } => *row.style = style,
    }
}

fn apply_evaluated_value(
    row: &mut FrameRowMut<'_>,
    property: Property,
    value: EvaluatedValue,
    preserve_render_frame: bool,
) -> bool {
    match (property, value) {
        (Property::Appearance, EvaluatedValue::Scalar(value)) => {
            let value = value.clamp(0.0, 1.0);
            let changed = *row.appearance != value;
            *row.appearance = value;
            changed
        }
        (Property::Reveal, EvaluatedValue::Scalar(value)) => {
            let value = value.clamp(0.0, 1.0);
            let changed = *row.reveal != value;
            *row.reveal = value;
            changed
        }
        (Property::Morph, EvaluatedValue::Scalar(value)) => {
            let value = value.clamp(0.0, 1.0);
            let changed = *row.morph != value;
            *row.morph = value;
            changed
        }
        (Property::Position, EvaluatedValue::Vec2(value)) => {
            let render_changed = !preserve_render_frame
                && release_render_transform(
                    row.render_geometry,
                    row.render_transform,
                    *row.transform,
                );
            let changed = row.transform.translation != value;
            row.transform.translation = value;
            changed || render_changed
        }
        (Property::Rotation, EvaluatedValue::Scalar(value)) => {
            let render_changed = !preserve_render_frame
                && release_render_transform(
                    row.render_geometry,
                    row.render_transform,
                    *row.transform,
                );
            let changed = row.transform.rotation != value;
            row.transform.rotation = value;
            changed || render_changed
        }
        (Property::Scale, EvaluatedValue::Vec2(value)) => {
            let render_changed = !preserve_render_frame
                && release_render_transform(
                    row.render_geometry,
                    row.render_transform,
                    *row.transform,
                );
            let changed = row.transform.scale != value;
            row.transform.scale = value;
            changed || render_changed
        }
        (Property::Fill, EvaluatedValue::Color(value)) => {
            let changed = row.style.fill != value;
            row.style.fill = value;
            changed
        }
        (Property::Stroke, EvaluatedValue::Color(value)) => {
            let changed = row.style.stroke != value;
            row.style.stroke = value;
            changed
        }
        (Property::Opacity, EvaluatedValue::Scalar(value)) => {
            let changed = row.style.opacity != value;
            row.style.opacity = value;
            changed
        }
        _ => unreachable!("compiled track value type must match its property"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum EvaluatedValue {
    Scalar(f32),
    Vec2(Vec2),
    Color(Option<Color>),
}

// A prepared morph owns its fixed rendering frame alongside ordinary semantic
// affine channels. Preserve that frame during their evaluation instead of rebuilding
// both paths only to reinstall the same compiled resource in the final Morph channel.
fn prepared_morph_owns_render_frame(
    compiled: &CompiledScene,
    object_index: u32,
    time: f64,
) -> bool {
    let tracks = compiled.channel_tracks(CompiledChannelKey::new(object_index, Property::Morph));
    let cursor = tracks.partition_point(|track| track.timing.start_time <= time);
    cursor.checked_sub(1).is_some_and(|index| {
        let track = &tracks[index];
        matches!(track.values, TrackValues::PreparedMorph { .. })
            && !(track.reconciled && time >= track.timing.start_time + track.timing.duration)
            && mapped_track_progress(track, time).is_some()
    })
}

fn apply_prepared_morph_track(
    row: &mut FrameRowMut<'_>,
    track: &CompiledTrack,
    progress: f32,
) -> bool {
    let TrackValues::PreparedMorph { from, to, .. } = &track.values else {
        unreachable!("prepared Morph track must contain scalar endpoints");
    };
    let Some(TransformGeometryPlan::PathPair {
        geometry,
        render_transform,
    }) = track.transform_geometry_plan.as_ref()
    else {
        unreachable!("prepared Morph track must carry compiled endpoint geometry");
    };
    let morph = lerp(*from, *to, progress).clamp(0.0, 1.0);
    let mut changed = *row.morph != morph || *row.render_transform != *render_transform;
    *row.morph = morph;
    *row.render_transform = *render_transform;
    changed |= set_optional_geometry_if_changed(row.render_geometry, Some(geometry), true);
    changed
}

fn apply_transform_track(row: &mut FrameRowMut<'_>, track: &CompiledTrack, progress: f32) -> bool {
    let TrackValues::Object { from, to } = &track.values else {
        unreachable!("compiled Transform track must contain object snapshots");
    };
    let plan = track
        .transform_geometry_plan
        .as_ref()
        .expect("compiled Transform track must carry a geometry plan");
    let next_transform = if matches!(plan, TransformGeometryPlan::PointwiseRotation) {
        interpolate_pointwise_rotation_transform(from.transform, to.transform, progress)
    } else {
        interpolate_transform(from.transform, to.transform, progress)
    };
    let next_style = interpolate_style(from.style, to.style, progress);
    let fixed_endpoint = matches!(
        plan,
        TransformGeometryPlan::PathPair {
            render_transform: Some(_),
            ..
        }
    ) && (progress <= 0.0 || progress >= 1.0);
    let next_morph = if !fixed_endpoint && matches!(plan, TransformGeometryPlan::PathPair { .. }) {
        progress
    } else {
        0.0
    };
    let owned_render_geometry = match plan {
        TransformGeometryPlan::PathPair {
            geometry: prepared,
            render_transform: None,
        } if from.style.stroke_width_mode == StrokeWidthMode::ScreenSpace
            && to.style.stroke_width_mode == StrokeWidthMode::ScreenSpace =>
        {
            screen_space_path_pair_relative_to_current(prepared, from, to, next_transform)
                .map(Arc::new)
        }
        _ => None,
    };
    let mut next_render_geometry = if let Some(geometry) = owned_render_geometry.as_ref() {
        Some(geometry)
    } else {
        match plan {
            TransformGeometryPlan::PathPair { geometry, .. } => Some(geometry),
            _ => None,
        }
    };

    if fixed_endpoint {
        next_render_geometry = None;
    }
    let next_render_transform = match plan {
        TransformGeometryPlan::PathPair {
            render_transform, ..
        } if !fixed_endpoint => *render_transform,
        _ => None,
    };
    let transform_changed = *row.render_transform != next_render_transform;
    *row.render_transform = next_render_transform;

    let Some(geometry) = row.content.geometry_mut() else {
        unreachable!("compiler rejects geometry-only Transform tracks on text content");
    };
    let mut changed =
        transform_changed | apply_transform_geometry(geometry, plan, from, to, progress);
    if *row.transform != next_transform {
        *row.transform = next_transform;
        changed = true;
    }
    if *row.style != next_style {
        *row.style = next_style;
        changed = true;
    }
    if *row.morph != next_morph {
        *row.morph = next_morph;
        changed = true;
    }
    changed |= set_optional_geometry_if_changed(
        row.render_geometry,
        next_render_geometry,
        owned_render_geometry.is_none(),
    );
    changed
}

// Independent affine drivers take ownership in semantic coordinates. The common
// morph-only lane never enters this conversion, so its compiled pair stays shared.
fn release_render_transform(
    geometry: &mut Option<Arc<GeometryRef>>,
    render_transform: &mut Option<Transform2D>,
    semantic_transform: Transform2D,
) -> bool {
    let Some(render) = *render_transform else {
        return false;
    };
    let Some(GeometryRef::VectorPath(source)) = geometry.as_deref() else {
        return false;
    };
    let Some(target) = source.morph_target() else {
        return false;
    };
    let source = path_relative_to_current(source, render, semantic_transform)
        .expect("fixed morph plan guarantees invertible semantic scale until driver takeover");
    let target = path_relative_to_current(target, render, semantic_transform)
        .expect("fixed morph plan guarantees invertible semantic scale until driver takeover");
    *geometry = Some(Arc::new(GeometryRef::path(
        source.with_morph_target(target),
    )));
    *render_transform = None;
    true
}

fn screen_space_path_pair_relative_to_current(
    prepared: &GeometryRef,
    from: &ObjectSnapshot,
    to: &ObjectSnapshot,
    current: Transform2D,
) -> Option<GeometryRef> {
    let GeometryRef::VectorPath(source) = prepared else {
        return None;
    };
    let target = source.morph_target()?;
    let source = path_relative_to_current(source, from.transform, current)?;
    let target = path_relative_to_current(target, to.transform, current)?;
    Some(GeometryRef::path(source.with_morph_target(target)))
}

fn path_relative_to_current(
    path: &VectorPath,
    endpoint: Transform2D,
    current: Transform2D,
) -> Option<VectorPath> {
    let mut transformed = VectorPath::new();
    for command in path.commands() {
        transformed = match *command {
            PathCommand::MoveTo { to } => {
                transformed.move_to(point_relative_to_current(to, endpoint, current)?)
            }
            PathCommand::LineTo { to } => {
                transformed.line_to(point_relative_to_current(to, endpoint, current)?)
            }
            PathCommand::QuadraticTo { control, to } => transformed.quadratic_to(
                point_relative_to_current(control, endpoint, current)?,
                point_relative_to_current(to, endpoint, current)?,
            ),
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => transformed.cubic_to(
                point_relative_to_current(control1, endpoint, current)?,
                point_relative_to_current(control2, endpoint, current)?,
                point_relative_to_current(to, endpoint, current)?,
            ),
            PathCommand::Close => transformed.close(),
        };
    }
    Some(transformed)
}

fn point_relative_to_current(
    point: Vec2,
    endpoint: Transform2D,
    current: Transform2D,
) -> Option<Vec2> {
    const MIN_SCALE: f32 = 1.0e-7;
    if !current.scale.x.is_finite()
        || !current.scale.y.is_finite()
        || current.scale.x.abs() <= MIN_SCALE
        || current.scale.y.abs() <= MIN_SCALE
    {
        return None;
    }
    let world = endpoint.transform_point(point);
    let relative = (world - current.translation).rotate(-current.rotation);
    let result = Vec2::new(relative.x / current.scale.x, relative.y / current.scale.y);
    (result.x.is_finite() && result.y.is_finite()).then_some(result)
}

fn apply_transform_geometry(
    current: &mut GeometryRef,
    plan: &TransformGeometryPlan,
    from: &ObjectSnapshot,
    to: &ObjectSnapshot,
    progress: f32,
) -> bool {
    match plan {
        TransformGeometryPlan::Static | TransformGeometryPlan::PointwiseRotation => {
            set_geometry_if_changed(current, &from.geometry)
        }
        TransformGeometryPlan::Circle {
            from_radius,
            to_radius,
        } => {
            let next = lerp(*from_radius, *to_radius, progress);
            match current {
                GeometryRef::Circle { radius } if *radius == next => false,
                GeometryRef::Circle { radius } => {
                    *radius = next;
                    true
                }
                _ => {
                    *current = GeometryRef::circle(next);
                    true
                }
            }
        }
        TransformGeometryPlan::Rectangle { from_size, to_size } => {
            let next = interpolate_vec2(*from_size, *to_size, progress);
            match current {
                GeometryRef::Rectangle { size } if *size == next => false,
                GeometryRef::Rectangle { size } => {
                    *size = next;
                    true
                }
                _ => {
                    *current = GeometryRef::rectangle(next.x, next.y);
                    true
                }
            }
        }
        TransformGeometryPlan::Line {
            from_start,
            from_end,
            to_start,
            to_end,
        } => {
            let next_start = interpolate_vec2(*from_start, *to_start, progress);
            let next_end = interpolate_vec2(*from_end, *to_end, progress);
            match current {
                GeometryRef::Line { start, end } if *start == next_start && *end == next_end => {
                    false
                }
                GeometryRef::Line { start, end } => {
                    *start = next_start;
                    *end = next_end;
                    true
                }
                _ => {
                    *current = GeometryRef::line(next_start, next_end);
                    true
                }
            }
        }
        TransformGeometryPlan::PathPair { .. } => {
            let semantic_geometry = if progress >= 1.0 {
                &to.geometry
            } else {
                &from.geometry
            };
            set_geometry_if_changed(current, semantic_geometry)
        }
    }
}

fn interpolate_vec2(from: Vec2, to: Vec2, progress: f32) -> Vec2 {
    Vec2::new(lerp(from.x, to.x, progress), lerp(from.y, to.y, progress))
}

fn set_geometry_if_changed(current: &mut GeometryRef, next: &GeometryRef) -> bool {
    if current == next {
        return false;
    }
    current.clone_from(next);
    true
}

// Compiled resources must recover their registered identity after a temporary
// affine-driver conversion, even when the owned points happen to be identical.
// Only dynamically rebuilt, unregistered geometry may retain a content-equal Arc.
fn set_optional_geometry_if_changed(
    current: &mut Option<Arc<GeometryRef>>,
    next: Option<&Arc<GeometryRef>>,
    require_shared_identity: bool,
) -> bool {
    match next {
        Some(next)
            if current.as_ref().is_some_and(|current| {
                Arc::ptr_eq(current, next)
                    || (!require_shared_identity && current.as_ref() == next.as_ref())
            }) =>
        {
            false
        }
        Some(next) => {
            if let Some(current) = current.as_mut() {
                current.clone_from(next);
            } else {
                *current = Some(next.clone());
            }
            true
        }
        None if current.is_some() => {
            *current = None;
            true
        }
        None => false,
    }
}

fn interpolate_pointwise_rotation_transform(
    from: Transform2D,
    to: Transform2D,
    progress: f32,
) -> Transform2D {
    if progress <= 0.0 {
        return from;
    }
    if progress >= 1.0 {
        return to;
    }
    debug_assert_eq!(from.scale, to.scale);
    let inverse = 1.0 - progress;
    let cosine = inverse * from.rotation.cos() + progress * to.rotation.cos();
    let sine = inverse * from.rotation.sin() + progress * to.rotation.sin();
    let magnitude = cosine.hypot(sine);
    Transform2D {
        translation: interpolate_vec2(from.translation, to.translation, progress),
        rotation: sine.atan2(cosine),
        scale: Vec2::new(from.scale.x * magnitude, from.scale.y * magnitude),
    }
}

fn interpolate_transform(from: Transform2D, to: Transform2D, progress: f32) -> Transform2D {
    Transform2D {
        translation: Vec2::new(
            lerp(from.translation.x, to.translation.x, progress),
            lerp(from.translation.y, to.translation.y, progress),
        ),
        rotation: lerp(from.rotation, to.rotation, progress),
        scale: Vec2::new(
            lerp(from.scale.x, to.scale.x, progress),
            lerp(from.scale.y, to.scale.y, progress),
        ),
    }
}

fn interpolate_style(from: Style, to: Style, progress: f32) -> Style {
    Style {
        fill: interpolate_optional_color(from.fill, to.fill, progress),
        stroke: interpolate_optional_color(from.stroke, to.stroke, progress),
        stroke_width: lerp(from.stroke_width, to.stroke_width, progress),
        stroke_width_mode: if progress >= 1.0 {
            to.stroke_width_mode
        } else {
            from.stroke_width_mode
        },
        stroke_join: if progress >= 1.0 {
            to.stroke_join
        } else {
            from.stroke_join
        },
        stroke_cap: if progress >= 1.0 {
            to.stroke_cap
        } else {
            from.stroke_cap
        },
        opacity: lerp(from.opacity, to.opacity, progress),
    }
}

fn interpolate_optional_color(
    from: Option<Color>,
    to: Option<Color>,
    progress: f32,
) -> Option<Color> {
    if progress <= 0.0 {
        return from;
    }
    if progress >= 1.0 {
        return to;
    }
    match (from, to) {
        (None, None) => None,
        (Some(from), Some(to)) => Some(interpolate_color(from, to, progress)),
        (None, Some(to)) => Some(interpolate_color(
            Color::rgba(to.red, to.green, to.blue, 0.0),
            to,
            progress,
        )),
        (Some(from), None) => Some(interpolate_color(
            from,
            Color::rgba(from.red, from.green, from.blue, 0.0),
            progress,
        )),
    }
}

fn interpolate_color(from: Color, to: Color, progress: f32) -> Color {
    Color::rgba(
        lerp(from.red, to.red, progress),
        lerp(from.green, to.green, progress),
        lerp(from.blue, to.blue, progress),
        lerp(from.alpha, to.alpha, progress),
    )
}

fn track_progress(track: &CompiledTrack, time: f64) -> f32 {
    if track.timing.is_instant() {
        return 1.0;
    }
    let raw = ((time - track.timing.start_time) / track.timing.duration).clamp(0.0, 1.0) as f32;
    track.timing.easing.evaluate(raw)
}

fn mapped_track_progress(track: &CompiledTrack, time: f64) -> Option<f32> {
    if time < track.timing.start_time {
        return None;
    }
    if track.timing.is_instant() {
        return Some(1.0);
    }
    let end = track.timing.start_time + track.timing.duration;
    // Scene state is defined after animation finish/cleanup at the exact endpoint.
    // This intentionally settles reversing group rates to the authored target,
    // matching Manim's `finish()` semantics and Noon's deterministic seek model.
    if time >= end {
        return Some(1.0);
    }
    let raw = ((time - track.timing.start_time) / track.timing.duration).clamp(0.0, 1.0) as f32;
    if track.time_map.is_identity() {
        return Some(track.timing.easing.evaluate(raw));
    }
    let sample = track.time_map.evaluate(raw);
    sample
        .begun
        .then(|| track.timing.easing.evaluate(sample.alpha))
}

fn interpolate(track: &CompiledTrack, progress: f32) -> EvaluatedValue {
    match &track.values {
        TrackValues::Scalar { from, to } => EvaluatedValue::Scalar(lerp(*from, *to, progress)),
        TrackValues::Vec2 { from, to } => EvaluatedValue::Vec2(Vec2::new(
            lerp(from.x, to.x, progress),
            lerp(from.y, to.y, progress),
        )),
        TrackValues::Color { from, to } => {
            EvaluatedValue::Color(interpolate_optional_color(*from, *to, progress))
        }
        TrackValues::Bool { .. } => {
            unreachable!("Presence tracks are evaluated as discrete events")
        }
        TrackValues::Object { .. } => {
            unreachable!("Transform tracks are evaluated atomically")
        }
        TrackValues::PreparedMorph { .. } => {
            unreachable!("prepared Morph tracks install compiled geometry with scalar progress")
        }
    }
}

const fn lerp(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress
}

#[cfg(test)]
mod tests {
    use noon_compile::CompiledObject;
    use noon_compile::CompiledScene;
    use noon_core::TrackId;
    use noon_core::{
        Color, CompositionTimeMap, CompositionTimeMapStep, Easing, GeometryRef, Property,
        RateFunction, SceneDefinition, Style, TrackDefinition, TrackTiming,
    };

    use super::*;

    fn compile_linear_scene() -> CompiledScene {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .animate_position(
                object,
                Vec2::ZERO,
                Vec2::new(10.0, 0.0),
                TrackTiming::new(1.0, 2.0, Easing::Linear),
            )
            .expect("valid track");
        CompiledScene::compile(&scene).expect("scene must compile")
    }

    #[test]
    fn timeline_endpoints_and_midpoint_are_exact() {
        let mut instance = SceneInstance::new(compile_linear_scene());
        assert_eq!(
            instance.seek(0.0).expect("valid time").objects[0]
                .transform
                .translation,
            Vec2::ZERO
        );
        assert_eq!(
            instance.seek(1.0).expect("valid time").objects[0]
                .transform
                .translation,
            Vec2::ZERO
        );
        assert_eq!(
            instance.seek(2.0).expect("valid time").objects[0]
                .transform
                .translation,
            Vec2::new(5.0, 0.0)
        );
        assert_eq!(
            instance.seek(3.0).expect("valid time").objects[0]
                .transform
                .translation,
            Vec2::new(10.0, 0.0)
        );
    }

    #[test]
    fn scale_timeline_endpoints_and_midpoint_are_exact() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .animate_scale(
                object,
                Vec2::ONE,
                Vec2::new(3.0, 2.0),
                TrackTiming::new(1.0, 2.0, Easing::Linear),
            )
            .expect("valid scale track");
        let mut instance =
            SceneInstance::new(CompiledScene::compile(&scene).expect("scene must compile"));
        assert_eq!(
            instance.seek(1.0).expect("valid time").objects[0]
                .transform
                .scale,
            Vec2::ONE
        );
        assert_eq!(
            instance.seek(2.0).expect("valid time").objects[0]
                .transform
                .scale,
            Vec2::new(2.0, 1.5)
        );
        assert_eq!(
            instance.seek(3.0).expect("valid time").objects[0]
                .transform
                .scale,
            Vec2::new(3.0, 2.0)
        );
    }

    #[test]
    fn manim_smooth_rate_function_is_evaluated_by_runtime() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .animate_position(
                object,
                Vec2::ZERO,
                Vec2::new(10.0, 0.0),
                TrackTiming::new(0.0, 2.0, RateFunction::Smooth),
            )
            .expect("valid track");
        let mut instance =
            SceneInstance::new(CompiledScene::compile(&scene).expect("scene must compile"));
        let quarter = instance.seek(0.5).expect("valid time").objects[0]
            .transform
            .translation
            .x;
        assert!((quarter - 0.7010372).abs() < 1e-5);
        assert_eq!(
            instance.seek(1.0).expect("valid time").objects[0]
                .transform
                .translation
                .x,
            5.0
        );
    }

    #[test]
    fn nonlinear_composition_time_map_is_evaluated_before_leaf_rate() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .add_track_with_time_map(
                object,
                Property::Position,
                TrackValues::Vec2 {
                    from: Vec2::ZERO,
                    to: Vec2::new(10.0, 0.0),
                },
                TrackTiming::new(0.0, 2.0, RateFunction::Linear),
                CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
                    0.0,
                    1.0,
                    RateFunction::Smooth,
                )]),
            )
            .unwrap();
        let mut instance =
            SceneInstance::new(CompiledScene::compile(&scene).expect("scene must compile"));
        let quarter = instance.seek(0.5).unwrap().objects[0]
            .transform
            .translation
            .x;
        assert!((quarter - 0.7010372).abs() < 1e-5);
    }

    #[test]
    fn mapped_succession_selects_latest_virtual_child() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        for (from, to, start) in [(0.0, 10.0, 0.0), (10.0, 20.0, 0.5)] {
            scene
                .add_track_with_time_map(
                    object,
                    Property::Position,
                    TrackValues::Vec2 {
                        from: Vec2::new(from, 0.0),
                        to: Vec2::new(to, 0.0),
                    },
                    TrackTiming::new(0.0, 2.0, RateFunction::Linear),
                    CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
                        start,
                        0.5,
                        RateFunction::Linear,
                    )]),
                )
                .unwrap();
        }
        let mut instance =
            SceneInstance::new(CompiledScene::compile(&scene).expect("scene must compile"));
        assert_eq!(
            instance.seek(0.5).unwrap().objects[0]
                .transform
                .translation
                .x,
            5.0
        );
        assert_eq!(
            instance.seek(1.25).unwrap().objects[0]
                .transform
                .translation
                .x,
            12.5
        );
        assert_eq!(
            instance.seek(2.0).unwrap().objects[0]
                .transform
                .translation
                .x,
            20.0
        );
    }

    #[test]
    fn reversing_composition_reopens_earlier_child_then_settles_at_finish() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        for (from, to, start) in [(0.0, 10.0, 0.0), (10.0, 20.0, 0.5)] {
            scene
                .add_track_with_time_map(
                    object,
                    Property::Position,
                    TrackValues::Vec2 {
                        from: Vec2::new(from, 0.0),
                        to: Vec2::new(to, 0.0),
                    },
                    TrackTiming::new(0.0, 2.0, RateFunction::Linear),
                    CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
                        start,
                        0.5,
                        RateFunction::ThereAndBack,
                    )]),
                )
                .unwrap();
        }
        let mut instance =
            SceneInstance::new(CompiledScene::compile(&scene).expect("scene must compile"));
        assert_eq!(
            instance.seek(1.0).unwrap().objects[0]
                .transform
                .translation
                .x,
            20.0
        );
        let reopened = instance.seek(1.6).unwrap().objects[0]
            .transform
            .translation
            .x;
        assert!(reopened > 0.0 && reopened < 10.0);
        assert_eq!(
            instance.seek(2.0).unwrap().objects[0]
                .transform
                .translation
                .x,
            20.0
        );
    }

    #[test]
    fn presence_events_are_discrete_and_direct_seek_matches_forward_playback() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .set_presence_at(object, false, true, 1.0)
            .expect("valid create event");
        scene
            .set_presence_at(object, true, false, 3.0)
            .expect("valid remove event");
        let compiled = CompiledScene::compile(&scene).expect("scene must compile");
        let mut sequential = SceneInstance::new(compiled.clone());
        let mut direct = SceneInstance::new(compiled);
        assert!(!sequential.frame().is_present(0));
        sequential.advance_to(0.999).expect("valid time");
        assert!(!sequential.frame().is_present(0));
        sequential.advance_to(1.0).expect("valid time");
        assert!(sequential.frame().is_present(0));
        sequential.advance_to(2.0).expect("valid time");
        assert!(sequential.frame().is_present(0));
        sequential.advance_to(3.0).expect("valid time");
        assert!(!sequential.frame().is_present(0));
        direct.seek(3.0).expect("valid time");
        assert_eq!(sequential.frame(), direct.frame());
        direct.seek(2.0).expect("valid time");
        assert!(direct.frame().is_present(0));
        direct.seek(0.0).expect("valid time");
        assert!(!direct.frame().is_present(0));
    }

    #[test]
    fn nested_mapped_presence_has_one_forward_and_seek_boundary() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .add_track_with_time_map(
                object,
                Property::Presence,
                TrackValues::Bool {
                    from: false,
                    to: true,
                },
                TrackTiming::new(2.0, 4.0, RateFunction::Linear),
                CompositionTimeMap::from_steps(vec![
                    CompositionTimeMapStep::new(0.2, 0.6, RateFunction::Smooth),
                    CompositionTimeMapStep::new(0.5, 0.5, RateFunction::Linear),
                ]),
            )
            .unwrap();
        let compiled = CompiledScene::compile(&scene).unwrap();
        let boundary = compiled.tracks()[0].timing.start_time;
        assert!(boundary > 2.0 && boundary < 6.0);

        let mut forward = SceneInstance::new(compiled.clone());
        forward.advance_to(boundary - 1e-6).unwrap();
        assert!(!forward.frame().is_present(0));
        forward.advance_to(boundary).unwrap();
        assert!(forward.frame().is_present(0));
        assert_eq!(forward.last_timeline_scheduler_stats().events_crossed, 1);
        forward.advance_to(boundary + 0.25).unwrap();
        assert!(forward.frame().is_present(0));
        assert_eq!(forward.last_timeline_scheduler_stats().groups_requested, 0);

        let mut direct = SceneInstance::new(compiled);
        direct.seek(boundary).unwrap();
        assert_eq!(direct.frame(), forward.seek(boundary).unwrap());
        direct.seek(boundary - 1e-6).unwrap();
        assert!(!direct.frame().is_present(0));
    }

    #[test]
    fn reveal_endpoints_midpoint_and_prestart_state_are_deterministic() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::path(
            noon_core::VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(3.0, 4.0)),
        ));
        scene
            .animate_reveal(object, 0.0, 1.0, TrackTiming::new(1.0, 2.0, Easing::Linear))
            .expect("valid reveal track");
        let mut instance =
            SceneInstance::new(CompiledScene::compile(&scene).expect("scene must compile"));
        assert_eq!(instance.seek(0.0).expect("valid time").reveal(0), 0.0);
        assert_eq!(instance.seek(1.0).expect("valid time").reveal(0), 0.0);
        assert_eq!(instance.seek(2.0).expect("valid time").reveal(0), 0.5);
        assert_eq!(instance.seek(3.0).expect("valid time").reveal(0), 1.0);
    }

    #[test]
    fn appearance_and_semantic_opacity_are_independent() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .object_mut(object)
            .expect("object exists")
            .style
            .opacity = 0.4;
        scene
            .animate_appearance(object, 1.0, 0.0, TrackTiming::new(0.0, 2.0, Easing::Linear))
            .expect("valid appearance track");
        let mut instance =
            SceneInstance::new(CompiledScene::compile(&scene).expect("scene must compile"));
        let frame = instance.seek(1.0).expect("valid time");
        assert_eq!(frame.objects[0].style.opacity, 0.4);
        assert_eq!(frame.appearance(0), 0.5);
    }

    #[test]
    fn reveal_and_morph_progress_are_independent() {
        let source = noon_core::VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.0))
            .line_to(Vec2::new(1.0, 0.0));
        let target = noon_core::VectorPath::new()
            .move_to(Vec2::new(0.0, -1.0))
            .line_to(Vec2::new(0.0, 1.0));
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::path(source.with_morph_target(target)));
        scene
            .animate_reveal(object, 0.0, 1.0, TrackTiming::new(0.0, 2.0, Easing::Linear))
            .expect("valid reveal track");
        scene
            .animate_morph(object, 0.0, 1.0, TrackTiming::new(0.0, 4.0, Easing::Linear))
            .expect("valid morph track");
        let mut instance =
            SceneInstance::new(CompiledScene::compile(&scene).expect("scene must compile"));
        let frame = instance.seek(1.0).expect("valid time");
        assert_eq!(frame.reveal(0), 0.5);
        assert_eq!(frame.morph(0), 0.25);
    }

    #[test]
    fn backward_and_forward_seeks_are_deterministic() {
        let mut instance = SceneInstance::new(compile_linear_scene());
        let first = instance.seek(2.25).expect("valid time").objects[0].clone();
        instance.seek(3.0).expect("valid time");
        instance.seek(0.5).expect("valid time");
        let second = instance.seek(2.25).expect("valid time").objects[0].clone();
        assert_eq!(first, second);
    }

    #[test]
    fn sequential_stepping_matches_direct_seek() {
        let compiled = compile_linear_scene();
        let mut sequential = SceneInstance::new(compiled.clone());
        let mut direct = SceneInstance::new(compiled);
        for step in 1..=25 {
            sequential
                .advance_to(f64::from(step) * 0.1)
                .expect("valid time");
        }
        direct.seek(2.5).expect("valid time");
        assert_eq!(sequential.frame(), direct.frame());
    }

    #[test]
    fn timeline_publication_advances_only_frame_epoch_and_same_time_is_a_no_op() {
        let mut instance = SceneInstance::new(compile_linear_scene());
        let before = instance.publication_context();

        instance.advance_to(2.0).expect("valid time");
        let advanced = instance.publication_context();
        assert_eq!(advanced.scene_revision(), before.scene_revision());
        assert_eq!(advanced.execution_revision(), before.execution_revision());
        assert_eq!(
            advanced.frame_epoch(),
            before.frame_epoch().checked_next().unwrap()
        );

        instance.advance_to(2.0).expect("same time is valid");
        assert_eq!(instance.publication_context(), advanced);

        instance.seek(1.0).expect("backward seek is valid");
        let sought = instance.publication_context();
        assert_eq!(sought.scene_revision(), advanced.scene_revision());
        assert_eq!(sought.execution_revision(), advanced.execution_revision());
        assert_eq!(
            sought.frame_epoch(),
            advanced.frame_epoch().checked_next().unwrap()
        );
    }

    #[test]
    fn renderer_publication_binds_one_frame_context_and_consumes_its_changes() {
        let mut instance = SceneInstance::new(compile_linear_scene());
        let initial_context = instance.publication_context();
        {
            let publication = instance.take_renderer_publication();
            assert_eq!(publication.context(), initial_context);
            assert!(publication.changes().is_all());
            assert_eq!(publication.frame().time, 0.0);
            assert_eq!(publication.frame().objects.len(), 1);
        }
        assert!(instance.take_frame_changes().is_empty());

        instance.advance_to(2.0).expect("valid time");
        let advanced_context = instance.publication_context();
        let publication = instance.take_renderer_publication();
        assert_eq!(publication.context(), advanced_context);
        assert_eq!(publication.frame().time, 2.0);
        assert!(!publication.changes().is_empty());
        assert_ne!(
            publication.context().frame_epoch(),
            initial_context.frame_epoch()
        );
    }

    #[test]
    fn completed_history_is_not_rescanned_during_forward_steps() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        for index in 0..1_000 {
            let start = f64::from(index);
            let from = index as f32;
            scene
                .animate_position(
                    object,
                    Vec2::new(from, 0.0),
                    Vec2::new(from + 1.0, 0.0),
                    TrackTiming::new(start, 0.5, Easing::Linear),
                )
                .expect("valid track");
        }
        let compiled = CompiledScene::compile(&scene).expect("scene must compile");
        let mut instance = SceneInstance::new(compiled);
        instance.seek(999.25).expect("valid time");
        assert!(instance.last_stats().binary_search_steps < 20);
        instance.advance_to(999.30).expect("valid time");
        assert_eq!(instance.last_stats().tracks_advanced, 0);
        assert_eq!(instance.last_stats().binary_search_steps, 0);
        assert_eq!(instance.last_stats().groups_evaluated, 1);
    }

    #[test]
    fn scalar_properties_are_evaluated_without_renderer_state() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .animate_scalar(
                object,
                Property::Opacity,
                1.0,
                0.0,
                TrackTiming::new(0.0, 2.0, Easing::Linear),
            )
            .expect("valid track");
        let compiled = CompiledScene::compile(&scene).expect("scene must compile");
        let mut instance = SceneInstance::new(compiled);
        let opacity = instance.seek(1.0).expect("valid time").objects[0]
            .style
            .opacity;
        assert_eq!(opacity, 0.5);
    }

    #[test]
    fn non_finite_times_are_rejected() {
        let mut instance = SceneInstance::new(compile_linear_scene());
        assert!(matches!(
            instance.seek(f64::NAN),
            Err(EvaluationError::InvalidTime(_))
        ));
    }

    #[test]
    fn live_patch_matches_recompile_of_equivalent_definition() {
        let mut definition = SceneDefinition::new();
        let object = definition.add(GeometryRef::circle(1.0));
        let track_id = definition
            .animate_position(
                object,
                Vec2::ZERO,
                Vec2::new(4.0, 0.0),
                TrackTiming::new(0.0, 4.0, Easing::Linear),
            )
            .expect("valid track");
        let compiled = CompiledScene::compile(&definition).expect("scene must compile");
        let mut live = SceneInstance::new(compiled);
        live.seek(2.0).expect("valid time");
        let replacement = TrackDefinition {
            id: track_id,
            object,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(8.0, 2.0),
            },
            timing: TrackTiming::new(0.0, 4.0, Easing::Linear),
            time_map: CompositionTimeMap::identity(),
        };
        let track_patch = ScenePatch::ReplaceTrack(replacement);
        let style_patch = ScenePatch::SetStyle {
            object,
            style: Style {
                opacity: 0.75,
                stroke_join: noon_core::StrokeJoin::Round,
                stroke_cap: noon_core::StrokeCap::Round,
                ..Style::default()
            },
        };
        live.apply_patch(&track_patch).expect("valid patch");
        live.apply_patch(&style_patch).expect("valid patch");
        definition
            .apply_patch(track_patch)
            .expect("valid definition patch");
        definition
            .apply_patch(style_patch)
            .expect("valid definition patch");
        let expected_compiled =
            CompiledScene::compile(&definition).expect("scene must compile after patches");
        let mut expected = SceneInstance::new(expected_compiled);
        expected.seek(2.0).expect("valid time");
        assert_eq!(live.frame(), expected.frame());
    }

    #[test]
    fn adding_presence_event_live_reconciles_at_current_time() {
        let mut definition = SceneDefinition::new();
        let object = definition.add(GeometryRef::circle(1.0));
        let mut live =
            SceneInstance::new(CompiledScene::compile(&definition).expect("scene must compile"));
        live.seek(2.0).expect("valid time");
        assert!(live.frame().is_present(0));
        let presence = TrackDefinition {
            id: noon_core::TrackId::new(7),
            object,
            property: Property::Presence,
            values: TrackValues::Bool {
                from: true,
                to: false,
            },
            timing: TrackTiming::instant(1.0),
            time_map: CompositionTimeMap::identity(),
        };
        let patch = ScenePatch::AddTrack(presence);
        live.apply_patch(&patch).expect("presence patch must apply");
        definition
            .apply_patch(patch)
            .expect("definition patch must apply");
        let mut expected = SceneInstance::new(
            CompiledScene::compile(&definition).expect("scene must compile after patch"),
        );
        expected.seek(2.0).expect("valid time");
        assert_eq!(live.frame(), expected.frame());
        assert!(!live.frame().is_present(0));
    }

    #[test]
    fn removing_track_restores_base_property_at_current_time() {
        let mut definition = SceneDefinition::new();
        let object = definition.add(GeometryRef::circle(1.0));
        let track_id = definition
            .animate_scalar(
                object,
                Property::Opacity,
                1.0,
                0.0,
                TrackTiming::new(0.0, 2.0, Easing::Linear),
            )
            .expect("valid track");
        let compiled = CompiledScene::compile(&definition).expect("scene must compile");
        let mut instance = SceneInstance::new(compiled);
        instance.seek(1.0).expect("valid time");
        assert_eq!(instance.frame().objects[0].style.opacity, 0.5);
        instance
            .apply_patch(&ScenePatch::RemoveTrack(track_id))
            .expect("valid patch");
        assert_eq!(instance.frame().objects[0].style.opacity, 1.0);
    }

    #[test]
    fn fill_track_uses_optional_paint_appearance_and_local_style_dirtying() {
        let object = ObjectId::new(1);
        let track = TrackDefinition {
            id: TrackId::new(0),
            object,
            property: Property::Fill,
            values: TrackValues::Color {
                from: None,
                to: Some(Color::RED),
            },
            timing: TrackTiming::new(0.0, 2.0, Easing::Linear),
            time_map: CompositionTimeMap::identity(),
        };
        let compiled = CompiledScene::compile_objects(
            vec![CompiledObject::new(
                object,
                GeometryRef::circle(1.0),
                Transform2D::default(),
                Style {
                    fill: None,
                    ..Style::default()
                },
            )],
            &[track],
        )
        .expect("scene must compile");
        let mut instance = SceneInstance::new(compiled);
        instance.take_frame_changes();

        instance.advance_to(1.0).unwrap();
        assert_eq!(
            instance.frame().objects[0].style.fill,
            Some(Color {
                alpha: 0.5,
                ..Color::RED
            })
        );
        assert_eq!(instance.take_frame_changes().object_indices(), &[0]);
        instance.advance_to(1.0).unwrap();
        assert!(instance.take_frame_changes().is_empty());
        instance.advance_to(2.0).unwrap();
        assert_eq!(instance.frame().objects[0].style.fill, Some(Color::RED));
    }

    #[test]
    fn stroke_track_uses_optional_paint_appearance_and_local_style_dirtying() {
        let object = ObjectId::new(1);
        let track = TrackDefinition {
            id: TrackId::new(0),
            object,
            property: Property::Stroke,
            values: TrackValues::Color {
                from: None,
                to: Some(Color {
                    alpha: 0.4,
                    ..Color::BLUE
                }),
            },
            timing: TrackTiming::new(0.0, 2.0, Easing::Linear),
            time_map: CompositionTimeMap::identity(),
        };
        let compiled = CompiledScene::compile_objects(
            vec![CompiledObject::new(
                object,
                GeometryRef::circle(1.0),
                Transform2D::default(),
                Style {
                    stroke: None,
                    ..Style::default()
                },
            )],
            &[track],
        )
        .expect("scene must compile");
        let mut instance = SceneInstance::new(compiled);
        instance.take_frame_changes();

        instance.advance_to(1.0).unwrap();
        assert_eq!(
            instance.frame().objects[0].style.stroke,
            Some(Color {
                alpha: 0.2,
                ..Color::BLUE
            })
        );
        assert_eq!(instance.take_frame_changes().object_indices(), &[0]);
        instance.advance_to(1.0).unwrap();
        assert!(instance.take_frame_changes().is_empty());
        instance.advance_to(2.0).unwrap();
        assert_eq!(
            instance.frame().objects[0].style.stroke,
            Some(Color {
                alpha: 0.4,
                ..Color::BLUE
            })
        );
    }

    #[test]
    fn value_patch_updates_base_fields_without_overwriting_animated_values() {
        let mut definition = SceneDefinition::new();
        let object = definition.add(GeometryRef::circle(1.0));
        definition
            .animate_scalar(
                object,
                Property::Opacity,
                1.0,
                0.0,
                TrackTiming::new(0.0, 2.0, Easing::Linear),
            )
            .expect("valid track");
        let mut instance =
            SceneInstance::new(CompiledScene::compile(&definition).expect("scene must compile"));
        instance.seek(1.0).expect("valid time");
        instance
            .apply_patch(&ScenePatch::SetStyle {
                object,
                style: Style {
                    fill: Some(Color::rgb(0.2, 0.4, 0.8)),
                    opacity: 0.9,
                    stroke_join: noon_core::StrokeJoin::Round,
                    stroke_cap: noon_core::StrokeCap::Round,
                    ..Style::default()
                },
            })
            .expect("style patch must apply");
        assert_eq!(
            instance.frame().objects[0].style.fill,
            Some(Color::rgb(0.2, 0.4, 0.8))
        );
        assert_eq!(instance.frame().objects[0].style.opacity, 0.5);
        assert_eq!(instance.frame().time, 1.0);
    }

    #[test]
    fn frame_changes_are_consumed_and_static_steps_stay_clean() {
        let mut scene = SceneDefinition::new();
        scene.add(GeometryRef::circle(1.0));
        let mut instance =
            SceneInstance::new(CompiledScene::compile(&scene).expect("scene must compile"));
        assert!(instance.take_frame_changes().is_all());
        instance.advance_to(0.5).expect("valid time");
        assert!(instance.take_frame_changes().is_empty());
    }

    #[test]
    fn frame_changes_accumulate_animation_and_patches_until_consumed() {
        let mut scene = SceneDefinition::new();
        let animated = scene.add(GeometryRef::circle(1.0));
        let patched = scene.add(GeometryRef::rectangle(2.0, 1.0));
        scene
            .animate_position(
                animated,
                Vec2::ZERO,
                Vec2::new(10.0, 0.0),
                TrackTiming::new(0.0, 2.0, Easing::Linear),
            )
            .expect("valid track");
        let mut instance =
            SceneInstance::new(CompiledScene::compile(&scene).expect("scene must compile"));
        instance.take_frame_changes();
        instance.advance_to(0.5).expect("valid time");
        instance
            .apply_patch(&ScenePatch::SetStyle {
                object: patched,
                style: Style {
                    opacity: 0.5,
                    stroke_join: noon_core::StrokeJoin::Round,
                    stroke_cap: noon_core::StrokeCap::Round,
                    ..Style::default()
                },
            })
            .expect("valid patch");
        instance.advance_to(0.75).expect("valid time");
        assert_eq!(instance.take_frame_changes().object_indices(), &[0, 1]);
        assert!(instance.take_frame_changes().is_empty());
    }
}
