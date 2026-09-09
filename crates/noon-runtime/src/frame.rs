//! Retained effective frame rows, observations and sparse frame changes.
use std::sync::Arc;

use noon_core::{
    FamilyAnimationState, GeometryRef, ObjectContentRef, ObjectId, Style, TextResourceHandle,
    Transform2D, Vec2,
};

use crate::release_render_transform;

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
    pub family_animations: Vec<Option<FamilyAnimationState>>,
    pub family_animation_plan_indices: Vec<Option<u32>>,
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
    pub(super) bounds_basis: Option<EffectiveBoundsBasis>,
}

impl EffectiveObjectProperties {
    pub(super) fn from_frame(
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
pub(super) enum EffectiveBoundsBasis {
    Circle(f32),
    Rect(noon_core::Rect),
}

impl EffectiveBoundsBasis {
    pub(super) fn from_frame(frame: &FrameState, object_index: usize) -> Option<Self> {
        let object = &frame.objects[object_index];
        Self::from_content(frame.render_geometry(object_index), object.text_bounds)
    }

    pub(super) fn from_content(
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

    pub(super) fn world_bounds(
        self,
        transform: Transform2D,
        style: Style,
    ) -> Option<noon_core::Rect> {
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
    pub(super) transform: Transform2D,
    pub(super) style: Style,
    pub(super) appearance: f32,
    pub(super) presence: bool,
    pub(super) reveal: f32,
    pub(super) morph: f32,
    pub(super) content_override: Option<ObjectContentRef>,
    pub(super) render_geometry: Option<Arc<GeometryRef>>,
    pub(super) render_transform: Option<Transform2D>,
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

    pub(super) fn write_to_frame(self, frame: &mut FrameState, object_index: usize) {
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

    pub(super) fn properties(
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

    pub(super) fn spatially_differs_from_frame(
        &self,
        frame: &FrameState,
        object_index: usize,
    ) -> bool {
        let object = &frame.objects[object_index];
        self.transform != object.transform
            || self.style.stroke != object.style.stroke
            || self.style.stroke_width != object.style.stroke_width
            || self.content_override.is_some()
            || self.render_geometry != frame.render_geometries[object_index]
            || self.render_transform != frame.render_transforms[object_index]
    }

    pub(super) fn as_mut<'a>(&'a mut self, base_content: &'a ObjectContentRef) -> FrameRowMut<'a> {
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

pub(super) enum FrameContentMut<'a> {
    Direct(&'a mut ObjectContentRef),
    Staged {
        base: &'a ObjectContentRef,
        content_override: &'a mut Option<ObjectContentRef>,
    },
}

impl FrameContentMut<'_> {
    pub(super) fn geometry_mut(&mut self) -> Option<&mut GeometryRef> {
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

pub(super) struct FrameRowMut<'a> {
    pub(super) content: FrameContentMut<'a>,
    pub(super) transform: &'a mut Transform2D,
    pub(super) style: &'a mut Style,
    pub(super) appearance: &'a mut f32,
    pub(super) presence: &'a mut bool,
    pub(super) reveal: &'a mut f32,
    pub(super) morph: &'a mut f32,
    pub(super) render_geometry: &'a mut Option<Arc<GeometryRef>>,
    pub(super) render_transform: &'a mut Option<Transform2D>,
}

pub(super) fn frame_row_mut(frame: &mut FrameState, object_index: usize) -> FrameRowMut<'_> {
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
    pub(super) all: bool,
    pub(super) object_indices: Vec<usize>,
    pub(super) added_indices: Vec<usize>,
    pub(super) removed_indices: Vec<usize>,
    pub(super) painter_order_range: Option<std::ops::Range<usize>>,
}

impl FrameChanges {
    pub fn all() -> Self {
        Self {
            all: true,
            object_indices: Vec::new(),
            added_indices: Vec::new(),
            removed_indices: Vec::new(),
            painter_order_range: None,
        }
    }

    pub fn objects(mut object_indices: Vec<usize>) -> Self {
        sort_dedup(&mut object_indices);
        Self {
            all: false,
            object_indices,
            added_indices: Vec::new(),
            removed_indices: Vec::new(),
            painter_order_range: None,
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
            painter_order_range: None,
        }
    }

    pub fn painter_order(range: std::ops::Range<usize>) -> Self {
        let mut changes = Self::default();
        changes.insert_painter_order_range(range);
        changes
    }

    /// Attach a locally bounded painter-order update to an existing object or
    /// structural change set.
    pub fn with_painter_order(mut self, range: std::ops::Range<usize>) -> Self {
        self.insert_painter_order_range(range);
        self
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
            painter_order_range: None,
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

    pub const fn has_painter_order_change(&self) -> bool {
        self.painter_order_range.is_some()
    }

    pub const fn is_empty(&self) -> bool {
        !self.all && self.object_indices.is_empty() && self.painter_order_range.is_none()
    }

    pub fn painter_order_range(&self) -> Option<std::ops::Range<usize>> {
        self.painter_order_range.clone()
    }

    pub(super) fn invalidate_all(&mut self) {
        self.all = true;
        self.object_indices.clear();
        self.added_indices.clear();
        self.removed_indices.clear();
        self.painter_order_range = None;
    }

    pub(super) fn insert(&mut self, object_index: usize) {
        insert_sorted_unique(&mut self.object_indices, object_index);
    }

    pub(super) fn insert_added(&mut self, object_index: usize) {
        if self.all {
            return;
        }
        insert_sorted_unique(&mut self.object_indices, object_index);
        insert_sorted_unique(&mut self.added_indices, object_index);
    }

    pub(super) fn insert_removed(&mut self, object_index: usize) {
        if self.all {
            return;
        }
        insert_sorted_unique(&mut self.object_indices, object_index);
        insert_sorted_unique(&mut self.removed_indices, object_index);
    }

    pub(super) fn insert_painter_order_range(&mut self, range: std::ops::Range<usize>) {
        if self.all || range.is_empty() {
            return;
        }
        self.painter_order_range = Some(match self.painter_order_range.take() {
            Some(existing) => existing.start.min(range.start)..existing.end.max(range.end),
            None => range,
        });
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
