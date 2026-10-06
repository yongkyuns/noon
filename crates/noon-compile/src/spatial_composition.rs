use super::{CompiledScene, CompiledSpatialState};
use noon_core::{
    GeometryRef, GeometryResource, GeometryResourceHandle, GeometryResourceLookup,
    SemanticSpatialMaterial, SemanticVec3,
};
use std::sync::Arc;

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
        if previous_anchor == next_anchor {
            if previous_fixed && !next_fixed {
                self.spatial_anchor_groups
                    .iter_mut()
                    .for_each(|g| g.member_indices.retain(|m| *m != object_index));
            }
            if previous_cairo && !next_cairo {
                self.spatial_anchor_groups
                    .iter_mut()
                    .for_each(|g| g.cairo_path_member_indices.retain(|m| *m != object_index));
            }
            if next_fixed && next_anchor.is_some() {
                if let Some(g) = self.spatial_anchor_group_for_anchor(next_anchor.unwrap()) {
                    let v = &mut self.spatial_anchor_groups[g as usize].member_indices;
                    if !v.contains(&object_index) {
                        v.push(object_index);
                        v.sort_unstable();
                    }
                }
            }
            if next_cairo && next_anchor.is_some() {
                if let Some(g) = self.spatial_anchor_group_for_anchor(next_anchor.unwrap()) {
                    let v = &mut self.spatial_anchor_groups[g as usize].cairo_path_member_indices;
                    if !v.contains(&object_index) {
                        v.push(object_index);
                        v.sort_unstable();
                    }
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

    pub(crate) fn cairo_path_points(&self, row: u32) -> Option<&[SemanticVec3]> {
        self.cairo_path_control_points.get(&row).map(AsRef::as_ref)
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
                    self.cairo_path_control_points.insert(row, points.clone());
                    return;
                }
                let Some(GeometryResource::VectorPath(path)) = self.resources.get(handle) else {
                    return;
                };
                Some(noon_geometry::cairo_path_control_points(path))
            }
            GeometryRef::VectorPath(path) => Some(noon_geometry::cairo_path_control_points(path)),
            _ => noon_geometry::canonical_outline_path(&geometry)
                .map(|path| noon_geometry::cairo_path_control_points(&path)),
        };
        let Some(points) = points.filter(|p| !p.is_empty()) else {
            return;
        };
        let points: Arc<[SemanticVec3]> = points
            .into_iter()
            .map(|p| SemanticVec3::new(f64::from(p.x), f64::from(p.y), 0.0))
            .collect::<Vec<_>>()
            .into();
        if let Some(handle) = external_handle {
            self.cairo_path_points_by_resource
                .insert(handle, points.clone());
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
