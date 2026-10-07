use super::{CompiledScene, CompiledSpatialState};
use noon_core::{
    GeometryRef, GeometryResource, GeometryResourceHandle, GeometryResourceLookup,
    SemanticSpatialMaterial, SemanticVec3,
};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(super) struct CairoPathPointResourceCache {
    // Keep the point buffer in a Vec: a dead Weak<slice> retains its allocation,
    // whereas this memo retains only the Arc header after the last user retires.
    entries: HashMap<GeometryResourceHandle, std::sync::Weak<Vec<SemanticVec3>>>,
    next_prune_at: usize,
}

impl Default for CairoPathPointResourceCache {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            next_prune_at: 64,
        }
    }
}

impl PartialEq for CairoPathPointResourceCache {
    fn eq(&self, _other: &Self) -> bool {
        // Weak memoization is an execution optimization, not compiled scene state.
        true
    }
}

impl std::ops::Deref for CairoPathPointResourceCache {
    type Target = HashMap<GeometryResourceHandle, std::sync::Weak<Vec<SemanticVec3>>>;
    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

impl std::ops::DerefMut for CairoPathPointResourceCache {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entries
    }
}

impl CairoPathPointResourceCache {
    fn insert_live_version(
        &mut self,
        handle: GeometryResourceHandle,
        points: &Arc<Vec<SemanticVec3>>,
    ) {
        self.entries.insert(handle, Arc::downgrade(points));
        if self.entries.len() >= self.next_prune_at {
            self.entries.retain(|_, points| points.strong_count() > 0);
            self.next_prune_at = self.entries.len().saturating_mul(2).max(64);
        }
    }
}

fn convex_hull(mut points: Vec<noon_core::Vec2>) -> Vec<noon_core::Vec2> {
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then_with(|| a.y.total_cmp(&b.y)));
    points.dedup_by(|a, b| a.x == b.x && a.y == b.y);
    if points.len() <= 2 {
        return points;
    }
    let cross = |o: noon_core::Vec2, a: noon_core::Vec2, b: noon_core::Vec2| {
        let ax = f64::from(a.x) - f64::from(o.x);
        let ay = f64::from(a.y) - f64::from(o.y);
        let bx = f64::from(b.x) - f64::from(o.x);
        let by = f64::from(b.y) - f64::from(o.y);
        ax * by - ay * bx
    };
    let mut lower = Vec::new();
    for point in points.iter().copied() {
        while lower.len() >= 2
            && cross(lower[lower.len() - 2], lower[lower.len() - 1], point) <= 0.0
        {
            lower.pop();
        }
        lower.push(point);
    }
    let mut upper = Vec::new();
    for point in points.iter().copied().rev() {
        while upper.len() >= 2
            && cross(upper[upper.len() - 2], upper[upper.len() - 1], point) <= 0.0
        {
            upper.pop();
        }
        upper.push(point);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Cached local planar bounds widened to f64 for world-space family-anchor
/// calculations. Source paths currently provide f32 coordinates, but all pose
/// composition and bounds accumulation remains in the semantic precision lane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompiledLocalBounds2D64 {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CompiledWorldBounds3D64 {
    pub min: SemanticVec3,
    pub max: SemanticVec3,
}

impl CompiledWorldBounds3D64 {
    pub fn is_valid(self) -> bool {
        [
            self.min.x, self.min.y, self.min.z, self.max.x, self.max.y, self.max.z,
        ]
        .into_iter()
        .all(f64::is_finite)
            && self.min.x <= self.max.x
            && self.min.y <= self.max.y
            && self.min.z <= self.max.z
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CompiledCairoPathAppearance {
    pub sheen_factor: f64,
    pub gradient_direction: Option<SemanticVec3>,
    pub world_family_bounds: Option<CompiledWorldBounds3D64>,
}

impl CompiledCairoPathAppearance {
    pub fn is_valid(&self) -> bool {
        self.sheen_factor.is_finite()
            && (0.0..=1.0).contains(&self.sheen_factor)
            && self.gradient_direction.is_none_or(|v| {
                v.x.is_finite()
                    && v.y.is_finite()
                    && v.z.is_finite()
                    && v.x.abs().max(v.y.abs()).max(v.z.abs()) > 0.0
            })
            && self
                .world_family_bounds
                .is_none_or(|bounds| bounds.is_valid())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct CompiledSpatialAnchorGroup {
    /// Rows whose renderer-facing fixed-orientation center is updated.
    member_indices: Vec<u32>,
    cairo_path_member_indices: Vec<u32>,
    /// All family leaves that contribute geometry bounds, whether or not they
    /// carry FixedOrientation metadata themselves.
    bounds_indices: Vec<u32>,
}

impl CompiledScene {
    pub(super) fn rebuild_spatial_anchor_groups(&mut self) {
        self.spatial_anchor_groups.clear();
        self.spatial_anchor_group_indices.clear();
        self.spatial_anchor_row_groups.clear();
        self.spatial_anchor_bounds_row_groups.clear();
        self.spatial_anchor_local_bounds.clear();
        self.cairo_path_control_points.clear();
        for index in 0..self.objects.len() {
            if !self.objects[index].live {
                continue;
            }
            let Some(spatial) = self.objects[index].spatial.as_deref() else {
                continue;
            };
            let row_index = index as u32;
            let fixed_member = spatial.composition_domain
                == noon_core::SemanticSpatialCompositionDomain::FixedOrientation;
            let cairo_member = spatial.material == SemanticSpatialMaterial::CairoPath
                && spatial
                    .cairo_path_appearance
                    .as_ref()
                    .is_some_and(|a| a.gradient_direction.is_some());
            if !fixed_member && !cairo_member {
                continue;
            }
            if fixed_member {
                let local_bounds = self.spatial_anchor_local_bounds_for(index);
                self.spatial_anchor_local_bounds
                    .insert(row_index, local_bounds);
            }
            let Some(anchor_family) = spatial.spatial_anchor_family else {
                continue;
            };
            let group_index = if let Some(group_index) = self
                .spatial_anchor_group_indices
                .get(&anchor_family)
                .copied()
            {
                group_index
            } else {
                let group_index = self.spatial_anchor_groups.len() as u32;
                self.spatial_anchor_groups.push(CompiledSpatialAnchorGroup {
                    member_indices: Vec::new(),
                    cairo_path_member_indices: Vec::new(),
                    bounds_indices: Vec::new(),
                });
                self.spatial_anchor_group_indices
                    .insert(anchor_family, group_index);
                group_index
            };
            let group = &mut self.spatial_anchor_groups[group_index as usize];
            if fixed_member {
                group.member_indices.push(row_index);
            }
            if cairo_member {
                group.cairo_path_member_indices.push(row_index);
            }
            self.spatial_anchor_row_groups
                .insert(row_index, group_index);
        }
        // Prepare unanchored gradients after group membership has been rebuilt.
        let gradient_rows = self
            .objects
            .iter()
            .enumerate()
            .filter_map(|(i, object)| {
                object
                    .spatial
                    .as_deref()
                    .filter(|s| s.material == SemanticSpatialMaterial::CairoPath)
                    .and_then(|s| s.cairo_path_appearance.as_ref())
                    .filter(|a| a.gradient_direction.is_some())
                    .map(|_| i as u32)
            })
            .collect::<Vec<_>>();
        for row in gradient_rows {
            self.refresh_cairo_path_control_points(row);
        }
    }

    pub(super) fn spatial_anchor_local_bounds_for(
        &self,
        object_index: usize,
    ) -> Option<CompiledLocalBounds2D64> {
        let object = self.objects.get(object_index)?;
        let bounds = object
            .content
            .geometry()
            .and_then(|geometry| match geometry {
                GeometryRef::External(id) => self
                    .resources
                    .current_handle(*id)
                    .and_then(|handle| GeometryResourceLookup::get(&self.resources, handle))
                    .and_then(|resource| match resource {
                        GeometryResource::VectorPath(path) => path.conservative_bounds(),
                        GeometryResource::Mesh(_) => None,
                    }),
                geometry => geometry.local_bounds(),
            })
            .or(object.text_bounds)?;
        Some(CompiledLocalBounds2D64 {
            min_x: f64::from(bounds.min.x),
            min_y: f64::from(bounds.min.y),
            max_x: f64::from(bounds.max.x),
            max_y: f64::from(bounds.max.y),
        })
    }

    pub(super) fn update_spatial_anchor_group(
        &mut self,
        object_index: u32,
        previous: Option<&CompiledSpatialState>,
        next: Option<&CompiledSpatialState>,
    ) {
        let group_anchor = |spatial: &CompiledSpatialState| {
            (spatial.composition_domain
                == noon_core::SemanticSpatialCompositionDomain::FixedOrientation
                || (spatial.material == SemanticSpatialMaterial::CairoPath
                    && spatial
                        .cairo_path_appearance
                        .as_ref()
                        .is_some_and(|a| a.gradient_direction.is_some())))
            .then_some(spatial.spatial_anchor_family)
            .flatten()
        };
        let previous_anchor = previous.and_then(group_anchor);
        let next_anchor = next.and_then(group_anchor);
        let previous_cairo = previous.is_some_and(|s| {
            s.material == SemanticSpatialMaterial::CairoPath
                && s.cairo_path_appearance
                    .as_ref()
                    .is_some_and(|a| a.gradient_direction.is_some())
        });
        let next_cairo = next.is_some_and(|s| {
            s.material == SemanticSpatialMaterial::CairoPath
                && s.cairo_path_appearance
                    .as_ref()
                    .is_some_and(|a| a.gradient_direction.is_some())
        });
        let previous_fixed = previous.is_some_and(|s| {
            s.composition_domain == noon_core::SemanticSpatialCompositionDomain::FixedOrientation
        });
        let next_fixed = next.is_some_and(|s| {
            s.composition_domain == noon_core::SemanticSpatialCompositionDomain::FixedOrientation
        });
        if previous_anchor != next_anchor {
            if let Some(previous_anchor) = previous_anchor {
                if let Some(group_index) = self
                    .spatial_anchor_group_indices
                    .get(&previous_anchor)
                    .copied()
                {
                    self.spatial_anchor_groups[group_index as usize]
                        .member_indices
                        .retain(|member| *member != object_index);
                    self.spatial_anchor_groups[group_index as usize]
                        .cairo_path_member_indices
                        .retain(|member| *member != object_index);
                }
            }
            self.spatial_anchor_row_groups.remove(&object_index);
            if let Some(next_anchor) = next_anchor {
                let group_index = if let Some(group_index) =
                    self.spatial_anchor_group_indices.get(&next_anchor).copied()
                {
                    group_index
                } else {
                    let group_index = self.spatial_anchor_groups.len() as u32;
                    self.spatial_anchor_groups.push(CompiledSpatialAnchorGroup {
                        member_indices: Vec::new(),
                        cairo_path_member_indices: Vec::new(),
                        bounds_indices: Vec::new(),
                    });
                    self.spatial_anchor_group_indices
                        .insert(next_anchor, group_index);
                    group_index
                };
                let members = if next_fixed {
                    &mut self.spatial_anchor_groups[group_index as usize].member_indices
                } else {
                    &mut self.spatial_anchor_groups[group_index as usize].cairo_path_member_indices
                };
                if !members.contains(&object_index) {
                    members.push(object_index);
                    members.sort_unstable();
                }
                self.spatial_anchor_row_groups
                    .insert(object_index, group_index);
            }
        }
        if let Some(group_index) = next_anchor
            .filter(|_| previous_anchor == next_anchor)
            .and_then(|anchor| self.spatial_anchor_group_for_anchor(anchor))
        {
            let group = &mut self.spatial_anchor_groups[group_index as usize];
            for (was_member, is_member, members) in [
                (previous_fixed, next_fixed, &mut group.member_indices),
                (
                    previous_cairo,
                    next_cairo,
                    &mut group.cairo_path_member_indices,
                ),
            ] {
                if was_member && !is_member {
                    members.retain(|member| *member != object_index);
                }
                if is_member && !members.contains(&object_index) {
                    members.push(object_index);
                    members.sort_unstable();
                }
            }
        }
        if next_fixed
            || self
                .spatial_anchor_bounds_row_groups
                .contains_key(&object_index)
        {
            let local_bounds = self.spatial_anchor_local_bounds_for(object_index as usize);
            self.spatial_anchor_local_bounds
                .insert(object_index, local_bounds);
        } else {
            self.spatial_anchor_local_bounds.remove(&object_index);
        }
    }

    pub fn spatial_anchor_group_for_row(&self, object_index: u32) -> Option<u32> {
        self.spatial_anchor_row_groups.get(&object_index).copied()
    }

    pub fn spatial_anchor_groups_for_row(&self, object_index: u32) -> &[u32] {
        self.spatial_anchor_bounds_row_groups
            .get(&object_index)
            .map(Vec::as_slice)
            .unwrap_or_else(|| {
                if let Some(group) = self.spatial_anchor_row_groups.get(&object_index) {
                    std::slice::from_ref(group)
                } else {
                    &[]
                }
            })
    }

    pub fn spatial_anchor_group_count(&self) -> usize {
        self.spatial_anchor_groups.len()
    }

    pub(crate) fn spatial_anchor_families(&self) -> Vec<noon_core::SemanticNodeId> {
        let mut anchors = self
            .spatial_anchor_group_indices
            .keys()
            .copied()
            .collect::<Vec<_>>();
        anchors.sort_unstable();
        anchors
    }

    pub fn spatial_anchor_group_members(&self, group_index: u32) -> &[u32] {
        self.spatial_anchor_groups
            .get(group_index as usize)
            .map_or(&[], |group| group.member_indices.as_slice())
    }

    pub fn spatial_anchor_cairo_path_members(&self, group_index: u32) -> &[u32] {
        self.spatial_anchor_groups
            .get(group_index as usize)
            .map_or(&[], |group| group.cairo_path_member_indices.as_slice())
    }

    pub fn cairo_path_points(&self, row: u32) -> Option<&[SemanticVec3]> {
        self.cairo_path_control_points
            .get(&row)
            .map(|points| points.as_slice())
    }

    pub fn spatial_anchor_group_bounds_members(&self, group_index: u32) -> &[u32] {
        self.spatial_anchor_groups
            .get(group_index as usize)
            .map_or(&[], |group| group.bounds_indices.as_slice())
    }

    pub(crate) fn set_spatial_anchor_group_bounds_members(
        &mut self,
        anchor_family: noon_core::SemanticNodeId,
        members: &[u32],
    ) {
        let Some(group_index) = self
            .spatial_anchor_group_indices
            .get(&anchor_family)
            .copied()
        else {
            return;
        };
        let old_rows = self.spatial_anchor_groups[group_index as usize]
            .bounds_indices
            .clone();
        for row in old_rows {
            let remove = if let Some(groups) = self.spatial_anchor_bounds_row_groups.get_mut(&row) {
                groups.retain(|group| *group != group_index);
                groups.is_empty()
            } else {
                false
            };
            if remove {
                self.spatial_anchor_bounds_row_groups.remove(&row);
            }
            self.refresh_cairo_path_control_points(row);
        }
        let mut rows = members.to_vec();
        rows.sort_unstable();
        rows.dedup();
        if let Some(group) = self.spatial_anchor_groups.get_mut(group_index as usize) {
            group.bounds_indices = rows.clone();
        }
        for row in rows {
            let groups = self
                .spatial_anchor_bounds_row_groups
                .entry(row)
                .or_default();
            if !groups.contains(&group_index) {
                groups.push(group_index);
                groups.sort_unstable();
            }
            let bounds = self.spatial_anchor_local_bounds_for(row as usize);
            self.spatial_anchor_local_bounds.insert(row, bounds);
            self.refresh_cairo_path_control_points(row);
        }
    }

    pub(super) fn refresh_cairo_path_control_points(&mut self, row: u32) {
        self.cairo_path_control_points.remove(&row);
        if !self.object_index_is_live(row) {
            return;
        }
        let own_gradient = self
            .objects
            .get(row as usize)
            .and_then(|o| o.spatial.as_deref())
            .filter(|s| s.material == SemanticSpatialMaterial::CairoPath)
            .and_then(|s| s.cairo_path_appearance.as_ref())
            .is_some_and(|a| a.gradient_direction.is_some());
        let needed = own_gradient
            || self
                .spatial_anchor_bounds_row_groups
                .get(&row)
                .is_some_and(|groups| {
                    groups
                        .iter()
                        .any(|g| !self.spatial_anchor_cairo_path_members(*g).is_empty())
                });
        if !needed {
            self.cairo_path_control_points.remove(&row);
            return;
        }
        let Some(geometry) = self
            .objects
            .get(row as usize)
            .and_then(|o| o.content.geometry())
        else {
            return;
        };
        let external_handle = match &geometry {
            GeometryRef::External(id) => self.resources.current_handle(*id),
            _ => None,
        };
        let points: Option<Vec<noon_core::Vec2>> = match &geometry {
            GeometryRef::External(_) => {
                let Some(handle) = external_handle else {
                    return;
                };
                if let Some(points) = self.cairo_path_points_by_resource.get(&handle) {
                    if let Some(points) = points.upgrade() {
                        self.cairo_path_control_points.insert(row, points);
                        return;
                    }
                }
                let Some(GeometryResource::VectorPath(path)) = self.resources.get(handle) else {
                    return;
                };
                Some(noon_geometry::cairo_path_control_points(path))
            }
            GeometryRef::VectorPath(path) => Some(noon_geometry::cairo_path_control_points(path)),
            _ => noon_geometry::canonical_outline_path(geometry)
                .map(|path| noon_geometry::cairo_path_control_points(&path)),
        };
        let Some(points) = points.filter(|p| !p.is_empty()) else {
            return;
        };
        let points = convex_hull(points);
        let points: Arc<Vec<SemanticVec3>> = points
            .into_iter()
            .map(|p| SemanticVec3::new(f64::from(p.x), f64::from(p.y), 0.0))
            .collect::<Vec<_>>()
            .into();
        if let Some(handle) = external_handle {
            self.cairo_path_points_by_resource
                .insert_live_version(handle, &points);
        }
        self.cairo_path_control_points.insert(row, points);
    }

    pub fn spatial_anchor_group_for_anchor(
        &self,
        anchor_family: noon_core::SemanticNodeId,
    ) -> Option<u32> {
        self.spatial_anchor_group_indices
            .get(&anchor_family)
            .copied()
    }

    pub fn spatial_anchor_local_bounds(
        &self,
        object_index: u32,
    ) -> Option<CompiledLocalBounds2D64> {
        self.spatial_anchor_local_bounds
            .get(&object_index)
            .copied()
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{
        Color, GeometryId, GeometryResourceHandle, ObjectContentRef, ObjectId,
        SemanticSpatialCompositionDomain, SemanticSpatialMaterial, SemanticWorldTransform3D, Style,
        TextResourceHandle, TextResourceId, Transform2D, Vec2, VectorPath,
    };

    fn handle(version: u64) -> GeometryResourceHandle {
        GeometryResourceHandle {
            arena: 7,
            id: GeometryId::new(9),
            version,
        }
    }

    fn cairo_state() -> CompiledSpatialState {
        CompiledSpatialState {
            world: SemanticWorldTransform3D::IDENTITY,
            camera_projection: None,
            camera_profile: None,
            camera_motions: None,
            material: SemanticSpatialMaterial::CairoPath,
            point_light: false,
            composition_domain: SemanticSpatialCompositionDomain::World,
            draw_kind: super::super::CompiledSpatialDrawKind::Planar,
            spatial_anchor_family: None,
            fixed_orientation_center: None,
            cairo_path_appearance: Some(Box::new(CompiledCairoPathAppearance {
                sheen_factor: 0.2,
                gradient_direction: Some(SemanticVec3::new(0.0, 1.0, 0.0)),
                world_family_bounds: None,
            })),
        }
    }

    #[test]
    fn cairo_control_cache_shares_live_versions_and_retires_replaced_or_removed_paths() {
        let object = super::super::CompiledObject::new(
            ObjectId::new(4),
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style {
                fill: Some(Color::WHITE),
                ..Style::default()
            },
        );
        let second_object = super::super::CompiledObject::new(
            ObjectId::new(5),
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        );
        let mut scene = CompiledScene::compile_objects(vec![object, second_object], &[]).unwrap();
        let id = GeometryId::new(9);
        let first = handle(1);
        let second = handle(2);
        let path = |end| {
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(end, 0.0))
        };
        scene
            .resources
            .geometries
            .insert(first, GeometryResource::VectorPath(Arc::new(path(1.0))));
        scene.resources.geometry_handles.insert(id, first);
        scene.objects[0].content = GeometryRef::External(id).into();
        scene.objects[0].spatial = Some(Box::new(cairo_state()));
        scene.refresh_cairo_path_control_points(0);
        scene.objects[1].content = GeometryRef::External(id).into();
        scene.objects[1].spatial = Some(Box::new(cairo_state()));
        scene.refresh_cairo_path_control_points(1);
        assert_eq!(scene.cairo_path_points(0).unwrap().len(), 2);
        let first_weak = scene
            .cairo_path_points_by_resource
            .get(&first)
            .unwrap()
            .clone();
        let shared = first_weak.upgrade().unwrap();
        assert_eq!(Arc::strong_count(&shared), 3); // Two row caches plus this observation.

        scene
            .resources
            .geometries
            .insert(second, GeometryResource::VectorPath(Arc::new(path(3.0))));
        scene.resources.geometry_handles.insert(id, second);
        scene.refresh_cairo_path_control_points(0);
        assert!(first_weak.upgrade().is_some()); // Row 1 still uses the old version.
        scene.refresh_cairo_path_control_points(1);
        assert!(first_weak.upgrade().is_some()); // A held snapshot still owns this version.
        assert_eq!(shared[1].x, 1.0);
        drop(shared);
        assert!(first_weak.upgrade().is_none());
        assert_eq!(scene.cairo_path_points(0).unwrap()[1].x, 3.0);
        let second_weak = scene
            .cairo_path_points_by_resource
            .get(&second)
            .unwrap()
            .clone();

        scene.objects[0].content = ObjectContentRef::Text(TextResourceHandle {
            arena: 1,
            id: TextResourceId::new(1),
            version: 1,
        });
        scene.refresh_cairo_path_control_points(0);
        assert!(scene.cairo_path_points(0).is_none());
        assert!(second_weak.upgrade().is_some());
        scene.objects[1].content = ObjectContentRef::Text(TextResourceHandle {
            arena: 1,
            id: TextResourceId::new(2),
            version: 1,
        });
        scene.refresh_cairo_path_control_points(1);
        assert!(scene.cairo_path_points(1).is_none());
        assert!(second_weak.upgrade().is_none());
    }

    #[test]
    fn cairo_control_cache_prunes_amortized_without_repeated_full_scans() {
        let mut cache = CairoPathPointResourceCache::default();
        let live_points: Vec<_> = (0..64)
            .map(|index| {
                let points: Arc<Vec<SemanticVec3>> =
                    vec![SemanticVec3::new(index as f64, 0.0, 0.0)].into();
                cache.insert_live_version(handle(index), &points);
                points
            })
            .collect();
        assert_eq!(cache.len(), 64);
        assert_eq!(cache.next_prune_at, 128);

        for index in 64..128 {
            let points: Arc<Vec<SemanticVec3>> =
                vec![SemanticVec3::new(index as f64, 0.0, 0.0)].into();
            cache.insert_live_version(handle(index), &points);
        }
        assert_eq!(
            cache.len(),
            65,
            "the prune preserves 64 held versions and the current insertion"
        );
        assert_eq!(cache.next_prune_at, 130);

        let next: Arc<Vec<SemanticVec3>> = vec![SemanticVec3::ZERO].into();
        cache.insert_live_version(handle(128), &next);
        assert_eq!(cache.len(), 66, "the next insert must not rescan the table");
        assert_eq!(cache.next_prune_at, 130);
        assert_eq!(live_points.len(), 64);
    }

    #[test]
    fn cairo_control_hull_keeps_finite_extreme_coordinates_ordered() {
        let limit = f32::MAX;
        let hull = convex_hull(vec![
            noon_core::Vec2::new(-limit, -limit),
            noon_core::Vec2::new(limit, -limit),
            noon_core::Vec2::new(limit, limit),
            noon_core::Vec2::new(-limit, limit),
            noon_core::Vec2::ZERO,
        ]);
        assert_eq!(hull.len(), 4);
        assert_eq!(hull[0], noon_core::Vec2::new(-limit, -limit));
        assert_eq!(hull[1], noon_core::Vec2::new(limit, -limit));
        assert_eq!(hull[2], noon_core::Vec2::new(limit, limit));
        assert_eq!(hull[3], noon_core::Vec2::new(-limit, limit));
    }
}
