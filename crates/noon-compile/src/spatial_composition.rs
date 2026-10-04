use super::{CompiledScene, CompiledSpatialState};
use noon_core::{GeometryRef, GeometryResource, GeometryResourceLookup};

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

#[derive(Clone, Debug, PartialEq)]
pub(super) struct CompiledFixedOrientationGroup {
    /// Rows whose renderer-facing fixed-orientation center is updated.
    member_indices: Vec<u32>,
    /// All family leaves that contribute geometry bounds, whether or not they
    /// carry FixedOrientation metadata themselves.
    bounds_indices: Vec<u32>,
}

impl CompiledScene {
    pub(super) fn rebuild_fixed_orientation_groups(&mut self) {
        self.fixed_orientation_groups.clear();
        self.fixed_orientation_group_indices.clear();
        self.fixed_orientation_row_groups.clear();
        self.fixed_orientation_bounds_row_groups.clear();
        self.fixed_orientation_local_bounds.clear();
        for index in 0..self.objects.len() {
            if !self.objects[index].live {
                continue;
            }
            let spatial = self.objects[index].spatial.as_deref();
            if !spatial.is_some_and(|spatial| {
                spatial.composition_domain
                    == noon_core::SemanticSpatialCompositionDomain::FixedOrientation
            }) {
                continue;
            }
            let local_bounds = self.fixed_orientation_local_bounds_for(index);
            let row_index = index as u32;
            self.fixed_orientation_local_bounds
                .insert(row_index, local_bounds);
            let Some(anchor_family) =
                spatial.and_then(|spatial| spatial.fixed_orientation_anchor_family)
            else {
                continue;
            };
            let group_index = if let Some(group_index) = self
                .fixed_orientation_group_indices
                .get(&anchor_family)
                .copied()
            {
                group_index
            } else {
                let group_index = self.fixed_orientation_groups.len() as u32;
                self.fixed_orientation_groups
                    .push(CompiledFixedOrientationGroup {
                        member_indices: Vec::new(),
                        bounds_indices: Vec::new(),
                    });
                self.fixed_orientation_group_indices
                    .insert(anchor_family, group_index);
                group_index
            };
            self.fixed_orientation_groups[group_index as usize]
                .member_indices
                .push(row_index);
            self.fixed_orientation_row_groups
                .insert(row_index, group_index);
        }
    }

    pub(super) fn fixed_orientation_local_bounds_for(
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

    pub(super) fn update_fixed_orientation_group(
        &mut self,
        object_index: u32,
        previous: Option<&CompiledSpatialState>,
        next: Option<&CompiledSpatialState>,
    ) {
        let previous_anchor = previous
            .filter(|spatial| {
                spatial.composition_domain
                    == noon_core::SemanticSpatialCompositionDomain::FixedOrientation
            })
            .and_then(|spatial| spatial.fixed_orientation_anchor_family);
        let next_anchor = next
            .filter(|spatial| {
                spatial.composition_domain
                    == noon_core::SemanticSpatialCompositionDomain::FixedOrientation
            })
            .and_then(|spatial| spatial.fixed_orientation_anchor_family);
        if previous_anchor != next_anchor {
            if let Some(previous_anchor) = previous_anchor {
                if let Some(group_index) = self
                    .fixed_orientation_group_indices
                    .get(&previous_anchor)
                    .copied()
                {
                    self.fixed_orientation_groups[group_index as usize]
                        .member_indices
                        .retain(|member| *member != object_index);
                }
            }
            self.fixed_orientation_row_groups.remove(&object_index);
            if let Some(next_anchor) = next_anchor {
                let group_index = if let Some(group_index) = self
                    .fixed_orientation_group_indices
                    .get(&next_anchor)
                    .copied()
                {
                    group_index
                } else {
                    let group_index = self.fixed_orientation_groups.len() as u32;
                    self.fixed_orientation_groups
                        .push(CompiledFixedOrientationGroup {
                            member_indices: Vec::new(),
                            bounds_indices: Vec::new(),
                        });
                    self.fixed_orientation_group_indices
                        .insert(next_anchor, group_index);
                    group_index
                };
                let members =
                    &mut self.fixed_orientation_groups[group_index as usize].member_indices;
                if !members.contains(&object_index) {
                    members.push(object_index);
                    members.sort_unstable();
                }
                self.fixed_orientation_row_groups
                    .insert(object_index, group_index);
            }
        }
        if next.is_some_and(|spatial| {
            spatial.composition_domain
                == noon_core::SemanticSpatialCompositionDomain::FixedOrientation
        }) || self
            .fixed_orientation_bounds_row_groups
            .contains_key(&object_index)
        {
            let local_bounds = self.fixed_orientation_local_bounds_for(object_index as usize);
            self.fixed_orientation_local_bounds
                .insert(object_index, local_bounds);
        } else {
            self.fixed_orientation_local_bounds.remove(&object_index);
        }
    }

    pub fn fixed_orientation_group_for_row(&self, object_index: u32) -> Option<u32> {
        self.fixed_orientation_row_groups
            .get(&object_index)
            .copied()
    }

    pub fn fixed_orientation_groups_for_row(&self, object_index: u32) -> &[u32] {
        self.fixed_orientation_bounds_row_groups
            .get(&object_index)
            .map(Vec::as_slice)
            .unwrap_or_else(|| {
                if let Some(group) = self.fixed_orientation_row_groups.get(&object_index) {
                    std::slice::from_ref(group)
                } else {
                    &[]
                }
            })
    }

    pub fn fixed_orientation_group_count(&self) -> usize {
        self.fixed_orientation_groups.len()
    }

    pub(crate) fn fixed_orientation_anchor_families(&self) -> Vec<noon_core::SemanticNodeId> {
        let mut anchors = self
            .fixed_orientation_group_indices
            .keys()
            .copied()
            .collect::<Vec<_>>();
        anchors.sort_unstable();
        anchors
    }

    pub fn fixed_orientation_group_members(&self, group_index: u32) -> &[u32] {
        self.fixed_orientation_groups
            .get(group_index as usize)
            .map_or(&[], |group| group.member_indices.as_slice())
    }

    pub fn fixed_orientation_group_bounds_members(&self, group_index: u32) -> &[u32] {
        self.fixed_orientation_groups
            .get(group_index as usize)
            .map_or(&[], |group| group.bounds_indices.as_slice())
    }

    pub(crate) fn set_fixed_orientation_group_bounds_members(
        &mut self,
        anchor_family: noon_core::SemanticNodeId,
        members: &[u32],
    ) {
        let Some(group_index) = self
            .fixed_orientation_group_indices
            .get(&anchor_family)
            .copied()
        else {
            return;
        };
        let old_rows = self.fixed_orientation_groups[group_index as usize]
            .bounds_indices
            .clone();
        for row in old_rows {
            let remove =
                if let Some(groups) = self.fixed_orientation_bounds_row_groups.get_mut(&row) {
                    groups.retain(|group| *group != group_index);
                    groups.is_empty()
                } else {
                    false
                };
            if remove {
                self.fixed_orientation_bounds_row_groups.remove(&row);
            }
        }
        let mut rows = members.to_vec();
        rows.sort_unstable();
        rows.dedup();
        if let Some(group) = self.fixed_orientation_groups.get_mut(group_index as usize) {
            group.bounds_indices = rows.clone();
        }
        for row in rows {
            let groups = self
                .fixed_orientation_bounds_row_groups
                .entry(row)
                .or_default();
            if !groups.contains(&group_index) {
                groups.push(group_index);
                groups.sort_unstable();
            }
            let bounds = self.fixed_orientation_local_bounds_for(row as usize);
            self.fixed_orientation_local_bounds.insert(row, bounds);
        }
    }

    pub fn fixed_orientation_group_for_anchor(
        &self,
        anchor_family: noon_core::SemanticNodeId,
    ) -> Option<u32> {
        self.fixed_orientation_group_indices
            .get(&anchor_family)
            .copied()
    }

    pub fn fixed_orientation_local_bounds(
        &self,
        object_index: u32,
    ) -> Option<CompiledLocalBounds2D64> {
        self.fixed_orientation_local_bounds
            .get(&object_index)
            .copied()
            .flatten()
    }
}
