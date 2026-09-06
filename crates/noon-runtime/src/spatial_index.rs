use std::collections::{BTreeMap, BTreeSet};

use noon_core::{GeometryRef, ObjectId, Rect, Vec2};

use crate::{ExecutionSlotId, FrameState};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpatialIndexConfig {
    pub cell_size: f32,
    pub max_cells_per_object: usize,
    pub max_cells_per_query: usize,
}

impl Default for SpatialIndexConfig {
    fn default() -> Self {
        Self {
            cell_size: 2.0,
            max_cells_per_object: 256,
            max_cells_per_query: 4_096,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpatialIndexUpdateStats {
    pub full_rebuilds: usize,
    pub leaves_upserted: usize,
    pub leaves_removed: usize,
    pub cells_inserted: usize,
    pub cells_removed: usize,
}

impl SpatialIndexUpdateStats {
    pub fn merge_from(&mut self, other: Self) {
        self.full_rebuilds += other.full_rebuilds;
        self.leaves_upserted += other.leaves_upserted;
        self.leaves_removed += other.leaves_removed;
        self.cells_inserted += other.cells_inserted;
        self.cells_removed += other.cells_removed;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpatialQueryStats {
    pub cells_visited: usize,
    pub candidates_tested: usize,
    pub results: usize,
    pub full_scan_fallbacks: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpatialQueryResult {
    slots: Vec<ExecutionSlotId>,
    stats: SpatialQueryStats,
}

impl SpatialQueryResult {
    pub fn slots(&self) -> &[ExecutionSlotId] {
        &self.slots
    }

    pub const fn stats(&self) -> SpatialQueryStats {
        self.stats
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CellKey {
    x: i32,
    y: i32,
}

#[derive(Clone, Debug, PartialEq)]
struct SpatialEntry {
    object: ObjectId,
    bounds: Rect,
    painter_order: u64,
    cells: Vec<CellKey>,
    global: bool,
}

#[derive(Clone, Debug)]
pub struct ExecutionSpatialIndex {
    config: SpatialIndexConfig,
    cells: BTreeMap<CellKey, Vec<ExecutionSlotId>>,
    global_slots: BTreeSet<ExecutionSlotId>,
    entries: BTreeMap<ExecutionSlotId, SpatialEntry>,
    object_slots: BTreeMap<ObjectId, ExecutionSlotId>,
}

impl Default for ExecutionSpatialIndex {
    fn default() -> Self {
        Self::new(SpatialIndexConfig::default())
    }
}

impl ExecutionSpatialIndex {
    pub fn new(config: SpatialIndexConfig) -> Self {
        assert!(
            config.cell_size.is_finite() && config.cell_size > 0.0,
            "spatial index cell size must be finite and positive"
        );
        assert!(
            config.max_cells_per_object > 0 && config.max_cells_per_query > 0,
            "spatial index cell limits must be non-zero"
        );
        Self {
            config,
            cells: BTreeMap::new(),
            global_slots: BTreeSet::new(),
            entries: BTreeMap::new(),
            object_slots: BTreeMap::new(),
        }
    }

    pub const fn config(&self) -> SpatialIndexConfig {
        self.config
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains_slot(&self, slot: ExecutionSlotId) -> bool {
        self.entries.contains_key(&slot)
    }

    pub fn bounds_for_slot(&self, slot: ExecutionSlotId) -> Option<Rect> {
        self.entries.get(&slot).map(|entry| entry.bounds)
    }

    pub fn rebuild(
        &mut self,
        frame: &FrameState,
        live_slots: impl IntoIterator<Item = (ExecutionSlotId, usize)>,
    ) -> SpatialIndexUpdateStats {
        self.cells.clear();
        self.global_slots.clear();
        self.entries.clear();
        self.object_slots.clear();
        let mut stats = SpatialIndexUpdateStats {
            full_rebuilds: 1,
            ..SpatialIndexUpdateStats::default()
        };
        for (slot, frame_index) in live_slots {
            stats.merge_from(self.upsert_frame_slot(frame, slot, frame_index, frame_index as u64));
        }
        stats
    }

    pub fn upsert_frame_slot(
        &mut self,
        frame: &FrameState,
        slot: ExecutionSlotId,
        frame_index: usize,
        painter_order: u64,
    ) -> SpatialIndexUpdateStats {
        let Some(object) = frame.objects.get(frame_index) else {
            return SpatialIndexUpdateStats::default();
        };
        if !frame.presences.get(frame_index).copied().unwrap_or(false)
            || object.appearance <= 0.0
            || object.style.opacity <= 0.0
        {
            return self.remove_object(object.id);
        }
        let Some(bounds) = frame_object_conservative_bounds(frame, frame_index) else {
            return self.remove_object(object.id);
        };
        self.upsert_bounds(slot, object.id, bounds, painter_order)
    }

    pub fn upsert_bounds(
        &mut self,
        slot: ExecutionSlotId,
        object: ObjectId,
        bounds: Rect,
        painter_order: u64,
    ) -> SpatialIndexUpdateStats {
        if !rect_is_finite(bounds) {
            return SpatialIndexUpdateStats::default();
        }

        if let Some(existing_slot) = self.object_slots.get(&object).copied() {
            if existing_slot != slot {
                self.remove_slot(existing_slot);
            }
        }

        let coverage = self.object_coverage(bounds);
        let (cells, global) = match coverage {
            Coverage::Cells(cells) => (cells, false),
            Coverage::Global => (Vec::new(), true),
        };
        let replacement = SpatialEntry {
            object,
            bounds,
            painter_order,
            cells,
            global,
        };
        if self.entries.get(&slot) == Some(&replacement) {
            return SpatialIndexUpdateStats::default();
        }

        let mut stats = self.remove_slot(slot);
        if replacement.global {
            self.global_slots.insert(slot);
        } else {
            for cell in &replacement.cells {
                let slots = self.cells.entry(*cell).or_default();
                insert_sorted_unique(slots, slot);
                stats.cells_inserted += 1;
            }
        }
        self.object_slots.insert(object, slot);
        self.entries.insert(slot, replacement);
        stats.leaves_upserted += 1;
        stats
    }

    pub fn remove_object(&mut self, object: ObjectId) -> SpatialIndexUpdateStats {
        let Some(slot) = self.object_slots.get(&object).copied() else {
            return SpatialIndexUpdateStats::default();
        };
        self.remove_slot(slot)
    }

    pub fn remove_slot(&mut self, slot: ExecutionSlotId) -> SpatialIndexUpdateStats {
        let Some(entry) = self.entries.remove(&slot) else {
            return SpatialIndexUpdateStats::default();
        };
        self.object_slots.remove(&entry.object);
        let mut stats = SpatialIndexUpdateStats {
            leaves_removed: 1,
            ..SpatialIndexUpdateStats::default()
        };
        if entry.global {
            self.global_slots.remove(&slot);
        } else {
            for cell in entry.cells {
                let remove_cell = if let Some(slots) = self.cells.get_mut(&cell) {
                    if let Ok(index) = slots.binary_search(&slot) {
                        slots.remove(index);
                        stats.cells_removed += 1;
                    }
                    slots.is_empty()
                } else {
                    false
                };
                if remove_cell {
                    self.cells.remove(&cell);
                }
            }
        }
        stats
    }

    pub fn query_rect(&self, bounds: Rect) -> SpatialQueryResult {
        self.query(bounds, false)
    }

    pub fn hit_test(&self, point: Vec2) -> SpatialQueryResult {
        self.query(Rect::new(point, point), true)
    }

    fn query(&self, bounds: Rect, topmost_first: bool) -> SpatialQueryResult {
        if !rect_is_finite(bounds) {
            return SpatialQueryResult::default();
        }
        let mut stats = SpatialQueryStats::default();
        let mut candidates = BTreeSet::new();
        candidates.extend(self.global_slots.iter().copied());

        match self.query_coverage(bounds) {
            Coverage::Cells(cells) => {
                stats.cells_visited = cells.len();
                for cell in cells {
                    if let Some(slots) = self.cells.get(&cell) {
                        candidates.extend(slots.iter().copied());
                    }
                }
            }
            Coverage::Global => {
                stats.full_scan_fallbacks = 1;
                candidates.extend(self.entries.keys().copied());
            }
        }

        let mut slots = Vec::new();
        for slot in candidates {
            let Some(entry) = self.entries.get(&slot) else {
                continue;
            };
            stats.candidates_tested += 1;
            if rects_intersect(entry.bounds, bounds) {
                slots.push(slot);
            }
        }
        slots.sort_by(|left, right| {
            let left_entry = &self.entries[left];
            let right_entry = &self.entries[right];
            left_entry
                .painter_order
                .cmp(&right_entry.painter_order)
                .then_with(|| left.cmp(right))
        });
        if topmost_first {
            slots.reverse();
        }
        stats.results = slots.len();
        SpatialQueryResult { slots, stats }
    }

    fn object_coverage(&self, bounds: Rect) -> Coverage {
        self.coverage(bounds, self.config.max_cells_per_object)
    }

    fn query_coverage(&self, bounds: Rect) -> Coverage {
        self.coverage(bounds, self.config.max_cells_per_query)
    }

    fn coverage(&self, bounds: Rect, max_cells: usize) -> Coverage {
        let Some(min_x) = cell_coordinate(bounds.min.x, self.config.cell_size) else {
            return Coverage::Global;
        };
        let Some(max_x) = cell_coordinate(bounds.max.x, self.config.cell_size) else {
            return Coverage::Global;
        };
        let Some(min_y) = cell_coordinate(bounds.min.y, self.config.cell_size) else {
            return Coverage::Global;
        };
        let Some(max_y) = cell_coordinate(bounds.max.y, self.config.cell_size) else {
            return Coverage::Global;
        };
        let width = i64::from(max_x) - i64::from(min_x) + 1;
        let height = i64::from(max_y) - i64::from(min_y) + 1;
        if width <= 0
            || height <= 0
            || width
                .checked_mul(height)
                .is_none_or(|count| count as u128 > max_cells as u128)
        {
            return Coverage::Global;
        }
        let mut cells = Vec::with_capacity((width * height) as usize);
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                cells.push(CellKey { x, y });
            }
        }
        Coverage::Cells(cells)
    }
}

#[derive(Clone, Debug)]
enum Coverage {
    Cells(Vec<CellKey>),
    Global,
}

pub fn frame_object_conservative_bounds(frame: &FrameState, object_index: usize) -> Option<Rect> {
    let object = frame.objects.get(object_index)?;
    effective_object_conservative_bounds(
        frame.render_geometry(object_index),
        object.text_bounds,
        frame.render_transform(object_index),
        object.style,
    )
}

pub(crate) fn effective_object_conservative_bounds(
    geometry: Option<&GeometryRef>,
    text_bounds: Option<Rect>,
    render_transform: noon_core::Transform2D,
    style: noon_core::Style,
) -> Option<Rect> {
    let mut world = match geometry {
        Some(GeometryRef::Circle { radius }) => circle_world_bounds(*radius, render_transform),
        Some(geometry) => transform_rect(geometry_local_bounds(geometry)?, render_transform),
        None => transform_rect(text_bounds?, render_transform),
    };
    if style.stroke.is_some() && style.stroke_width.is_finite() {
        let scale = render_transform
            .scale
            .x
            .abs()
            .max(render_transform.scale.y.abs());
        let expansion = style.stroke_width.abs() * scale * 0.5;
        world.min.x -= expansion;
        world.min.y -= expansion;
        world.max.x += expansion;
        world.max.y += expansion;
    }
    rect_is_finite(world).then_some(world)
}

fn circle_world_bounds(radius: f32, transform: noon_core::Transform2D) -> Rect {
    let radius = radius.abs();
    let scaled_x = radius * transform.scale.x;
    let scaled_y = radius * transform.scale.y;
    let (sin, cos) = transform.rotation.sin_cos();
    let half = Vec2::new(
        (scaled_x * cos).hypot(scaled_y * sin),
        (scaled_x * sin).hypot(scaled_y * cos),
    );
    Rect::new(transform.translation - half, transform.translation + half)
}

fn geometry_local_bounds(geometry: &GeometryRef) -> Option<Rect> {
    match geometry {
        GeometryRef::Circle { radius } => {
            let radius = radius.abs();
            Some(Rect::new(
                Vec2::new(-radius, -radius),
                Vec2::new(radius, radius),
            ))
        }
        GeometryRef::Rectangle { size } => {
            let half = Vec2::new(size.x.abs() * 0.5, size.y.abs() * 0.5);
            Some(Rect::new(-half, half))
        }
        GeometryRef::Line { start, end } => Rect::from_points([*start, *end]),
        GeometryRef::VectorPath(path) => {
            let source = path.conservative_bounds();
            let target = path
                .morph_target()
                .and_then(|target| target.conservative_bounds());
            match (source, target) {
                (Some(source), Some(target)) => Some(source.union(target)),
                (source, target) => source.or(target),
            }
        }
        GeometryRef::External(_) => None,
    }
}

fn transform_rect(bounds: Rect, transform: noon_core::Transform2D) -> Rect {
    let corners = [
        bounds.min,
        Vec2::new(bounds.max.x, bounds.min.y),
        bounds.max,
        Vec2::new(bounds.min.x, bounds.max.y),
    ];
    Rect::from_points(
        corners
            .into_iter()
            .map(|point| transform.transform_point(point)),
    )
    .expect("rectangle has four corners")
}

fn rects_intersect(left: Rect, right: Rect) -> bool {
    left.min.x <= right.max.x
        && left.max.x >= right.min.x
        && left.min.y <= right.max.y
        && left.max.y >= right.min.y
}

fn rect_is_finite(bounds: Rect) -> bool {
    bounds.min.x.is_finite()
        && bounds.min.y.is_finite()
        && bounds.max.x.is_finite()
        && bounds.max.y.is_finite()
        && bounds.min.x <= bounds.max.x
        && bounds.min.y <= bounds.max.y
}

fn cell_coordinate(value: f32, cell_size: f32) -> Option<i32> {
    if !value.is_finite() {
        return None;
    }
    let value = (value / cell_size).floor();
    if value < i32::MIN as f32 || value > i32::MAX as f32 {
        None
    } else {
        Some(value as i32)
    }
}

fn insert_sorted_unique(slots: &mut Vec<ExecutionSlotId>, slot: ExecutionSlotId) {
    if let Err(index) = slots.binary_search(&slot) {
        slots.insert(index, slot);
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        GeometryRef, ObjectContentRef, ObjectId, Rect, Style, TextResourceHandle, TextResourceId,
        Transform2D, Vec2,
    };

    use super::*;

    fn single_object_frame(
        geometry: GeometryRef,
        transform: Transform2D,
        style: Style,
    ) -> FrameState {
        FrameState {
            time: 0.0,
            objects: vec![crate::FrameObjectState {
                id: ObjectId::new(0),
                content: ObjectContentRef::Geometry(geometry),
                text_bounds: None,
                transform,
                style,
                appearance: 1.0,
            }],
            presences: vec![true],
            reveals: vec![1.0],
            morphs: vec![0.0],
            render_geometries: vec![None],
            render_transforms: vec![None],
        }
    }

    #[test]
    fn text_resource_bounds_follow_the_effective_object_transform() {
        let local = Rect::new(Vec2::new(-1.0, -0.5), Vec2::new(2.0, 1.5));
        let frame = FrameState {
            time: 0.0,
            objects: vec![crate::FrameObjectState {
                id: ObjectId::new(0),
                content: ObjectContentRef::Text(TextResourceHandle {
                    arena: 0,
                    id: TextResourceId::new(3),
                    version: 0,
                }),
                text_bounds: Some(local),
                transform: Transform2D {
                    translation: Vec2::new(3.0, 4.0),
                    rotation: 0.0,
                    scale: Vec2::new(2.0, 0.5),
                },
                style: Style::default(),
                appearance: 1.0,
            }],
            presences: vec![true],
            reveals: vec![1.0],
            morphs: vec![0.0],
            render_geometries: vec![None],
            render_transforms: vec![None],
        };

        assert_eq!(
            frame_object_conservative_bounds(&frame, 0),
            Some(Rect::new(Vec2::new(1.0, 3.75), Vec2::new(7.0, 4.75)))
        );
    }

    #[test]
    fn rotated_nonuniform_circle_uses_analytic_spatial_bounds() {
        let transform = Transform2D {
            translation: Vec2::new(3.0, -2.0),
            rotation: std::f32::consts::FRAC_PI_4,
            scale: Vec2::new(2.0, 1.0),
        };
        let frame = single_object_frame(GeometryRef::circle(1.0), transform, Style::default());
        let bounds = frame_object_conservative_bounds(&frame, 0).expect("circle has bounds");
        let half = 2.5_f32.sqrt();

        assert!((bounds.min.x - (3.0 - half)).abs() < 1e-5);
        assert!((bounds.max.x - (3.0 + half)).abs() < 1e-5);
        assert!((bounds.min.y - (-2.0 - half)).abs() < 1e-5);
        assert!((bounds.max.y - (-2.0 + half)).abs() < 1e-5);
    }

    #[test]
    fn circle_spatial_bounds_keep_stroke_conservative_under_reflection() {
        let transform = Transform2D {
            translation: Vec2::new(-4.0, 5.0),
            rotation: 0.3,
            scale: Vec2::new(-3.0, 0.5),
        };
        let style = Style {
            stroke: Some(noon_core::Color::WHITE),
            stroke_width: 0.4,
            ..Style::default()
        };
        let frame = single_object_frame(GeometryRef::circle(1.5), transform, style);
        let bounds = frame_object_conservative_bounds(&frame, 0).expect("circle has bounds");
        let (sin, cos) = transform.rotation.sin_cos();
        let scaled_x = 1.5 * transform.scale.x;
        let scaled_y = 1.5 * transform.scale.y;
        let half = Vec2::new(
            (scaled_x * cos).hypot(scaled_y * sin),
            (scaled_x * sin).hypot(scaled_y * cos),
        );
        let stroke_expansion = 0.4 * 3.0 * 0.5;

        assert!((bounds.min.x - (-4.0 - half.x - stroke_expansion)).abs() < 1e-5);
        assert!((bounds.max.x - (-4.0 + half.x + stroke_expansion)).abs() < 1e-5);
        assert!((bounds.min.y - (5.0 - half.y - stroke_expansion)).abs() < 1e-5);
        assert!((bounds.max.y - (5.0 + half.y + stroke_expansion)).abs() < 1e-5);
    }

    #[test]
    fn point_query_in_hundred_thousand_objects_does_not_scan_scene() {
        let mut index = ExecutionSpatialIndex::default();
        let columns = 400usize;
        for object_index in 0..100_000usize {
            let x = (object_index % columns) as f32 * 3.0;
            let y = (object_index / columns) as f32 * 3.0;
            index.upsert_bounds(
                ExecutionSlotId::new(object_index as u32, 0),
                ObjectId::new(object_index as u64),
                Rect::new(Vec2::new(x, y), Vec2::new(x + 1.0, y + 1.0)),
                object_index as u64,
            );
        }

        let target = 50_123usize;
        let x = (target % columns) as f32 * 3.0 + 0.5;
        let y = (target / columns) as f32 * 3.0 + 0.5;
        let result = index.hit_test(Vec2::new(x, y));
        assert_eq!(result.slots(), &[ExecutionSlotId::new(target as u32, 0)]);
        assert_eq!(result.stats().cells_visited, 1);
        assert!(result.stats().candidates_tested < 8);
        assert_eq!(result.stats().full_scan_fallbacks, 0);
    }

    #[test]
    fn moving_one_leaf_updates_only_its_cells() {
        let mut index = ExecutionSpatialIndex::default();
        let slot = ExecutionSlotId::new(7, 0);
        index.upsert_bounds(
            slot,
            ObjectId::new(7),
            Rect::new(Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0)),
            7,
        );
        let stats = index.upsert_bounds(
            slot,
            ObjectId::new(7),
            Rect::new(Vec2::new(12.0, 12.0), Vec2::new(13.0, 13.0)),
            7,
        );
        assert_eq!(stats.full_rebuilds, 0);
        assert_eq!(stats.leaves_upserted, 1);
        assert_eq!(stats.leaves_removed, 1);
        assert!(index.hit_test(Vec2::new(0.5, 0.5)).slots().is_empty());
        assert_eq!(index.hit_test(Vec2::new(12.5, 12.5)).slots(), &[slot]);
    }

    #[test]
    fn hit_test_returns_topmost_painter_candidate_first() {
        let mut index = ExecutionSpatialIndex::default();
        let bounds = Rect::new(Vec2::new(-1.0, -1.0), Vec2::new(1.0, 1.0));
        let lower = ExecutionSlotId::new(2, 0);
        let upper = ExecutionSlotId::new(9, 0);
        index.upsert_bounds(lower, ObjectId::new(2), bounds, 2);
        index.upsert_bounds(upper, ObjectId::new(9), bounds, 9);
        assert_eq!(index.hit_test(Vec2::ZERO).slots(), &[upper, lower]);
        assert_eq!(index.query_rect(bounds).slots(), &[lower, upper]);
    }

    #[test]
    fn viewport_query_visits_only_intersecting_grid_cells() {
        let mut index = ExecutionSpatialIndex::default();
        for object_index in 0..10_000usize {
            let x = (object_index % 100) as f32 * 4.0;
            let y = (object_index / 100) as f32 * 4.0;
            index.upsert_bounds(
                ExecutionSlotId::new(object_index as u32, 0),
                ObjectId::new(object_index as u64),
                Rect::new(Vec2::new(x, y), Vec2::new(x + 1.0, y + 1.0)),
                object_index as u64,
            );
        }
        let result = index.query_rect(Rect::new(Vec2::new(39.0, 39.0), Vec2::new(61.0, 61.0)));
        assert!(result.stats().cells_visited < 200);
        assert!(result.stats().candidates_tested < 100);
        assert!(!result.slots().is_empty());
    }
}
