//! Sparse spatial anchor evaluation on the existing effective frame.

use crate::SceneInstance;

impl SceneInstance {
    pub(super) fn flush_spatial_anchor_changes(&mut self) {
        let groups = std::mem::take(&mut self.pending_spatial_anchor_groups);
        for group in groups {
            let center = self.spatial_anchor_group_center(group);
            for member_index in 0..self.compiled.spatial_anchor_group_members(group).len() {
                let member = self.compiled.spatial_anchor_group_members(group)[member_index];
                self.set_fixed_orientation_center(member as usize, center);
            }
            let bounds = self.cairo_path_group_bounds(group);
            for member in self
                .compiled
                .spatial_anchor_cairo_path_members(group)
                .to_vec()
            {
                self.set_cairo_path_world_family_bounds(member as usize, bounds);
            }
        }
        let rows = std::mem::take(&mut self.pending_spatial_anchor_rows);
        for row in rows {
            if self
                .frame
                .objects
                .get(row)
                .and_then(|o| o.spatial.as_deref())
                .is_some_and(|s| s.material == noon_core::SemanticSpatialMaterial::CairoPath)
            {
                let bounds = self.cairo_path_bounds_for_rows(std::iter::once(row as u32));
                self.set_cairo_path_world_family_bounds(row, bounds);
            } else {
                let center = self.spatial_anchor_row_center(row);
                self.set_fixed_orientation_center(row, center);
            }
        }
    }

    pub(super) fn refresh_all_spatial_anchors(&mut self) {
        let groups = (0..self.compiled.spatial_anchor_group_count() as u32).collect::<Vec<_>>();
        for group in groups {
            let center = self.spatial_anchor_group_center(group);
            for member_index in 0..self.compiled.spatial_anchor_group_members(group).len() {
                let member = self.compiled.spatial_anchor_group_members(group)[member_index];
                self.set_fixed_orientation_center(member as usize, center);
            }
            let bounds = self.cairo_path_group_bounds(group);
            for member in self
                .compiled
                .spatial_anchor_cairo_path_members(group)
                .to_vec()
            {
                self.set_cairo_path_world_family_bounds(member as usize, bounds);
            }
        }
        for row in 0..self.frame.objects.len() {
            if self.frame.objects[row].spatial.as_deref().is_some_and(|s| {
                s.material == noon_core::SemanticSpatialMaterial::CairoPath
                    && s.cairo_path_appearance
                        .as_deref()
                        .is_some_and(|a| a.gradient_direction.is_some())
                    && self
                        .compiled
                        .spatial_anchor_group_for_row(row as u32)
                        .is_none()
            }) {
                let bounds = self.cairo_path_bounds_for_rows(std::iter::once(row as u32));
                self.set_cairo_path_world_family_bounds(row, bounds);
            }
            if self
                .compiled
                .spatial_anchor_group_for_row(row as u32)
                .is_none()
                && self.frame.objects[row]
                    .spatial
                    .as_deref()
                    .is_some_and(|spatial| {
                        spatial.composition_domain
                            == noon_core::SemanticSpatialCompositionDomain::FixedOrientation
                    })
            {
                let center = self.spatial_anchor_row_center(row);
                self.set_fixed_orientation_center(row, center);
            }
        }
    }

    fn spatial_anchor_group_center(&self, group: u32) -> Option<noon_core::SemanticVec3> {
        let members = self.compiled.spatial_anchor_group_bounds_members(group);
        if members.is_empty() {
            return None;
        }
        self.world_bounds_center(members.iter().copied())
    }

    fn cairo_path_group_bounds(&self, group: u32) -> Option<noon_compile::CompiledWorldBounds3D64> {
        self.cairo_path_bounds_for_rows(
            self.compiled
                .spatial_anchor_group_bounds_members(group)
                .iter()
                .copied(),
        )
    }

    fn cairo_path_bounds_for_rows(
        &self,
        rows: impl IntoIterator<Item = u32>,
    ) -> Option<noon_compile::CompiledWorldBounds3D64> {
        let mut min = noon_core::SemanticVec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut max =
            noon_core::SemanticVec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        let mut found = false;
        for row in rows {
            let Some(points) = self.compiled.cairo_path_points(row) else {
                continue;
            };
            let Some(object) = self.frame.objects.get(row as usize) else {
                continue;
            };
            let world = object.world_transform().or_else(|| {
                noon_core::SemanticWorldTransform3D::from_2_5d(noon_core::SemanticTransform2_5D {
                    translation: noon_core::SemanticVec3::new(
                        f64::from(object.transform.translation.x),
                        f64::from(object.transform.translation.y),
                        0.0,
                    ),
                    scale: noon_core::SemanticVec3::new(
                        f64::from(object.transform.scale.x),
                        f64::from(object.transform.scale.y),
                        1.0,
                    ),
                    rotation_z: f64::from(object.transform.rotation),
                })
            });
            let Some(world) = world else {
                continue;
            };
            for point in points {
                let Some(p) = world.transform_point(*point) else {
                    return None;
                };
                min.x = min.x.min(p.x);
                min.y = min.y.min(p.y);
                min.z = min.z.min(p.z);
                max.x = max.x.max(p.x);
                max.y = max.y.max(p.y);
                max.z = max.z.max(p.z);
                found = true;
            }
        }
        found.then_some(noon_compile::CompiledWorldBounds3D64 { min, max })
    }

    fn set_cairo_path_world_family_bounds(
        &mut self,
        row: usize,
        bounds: Option<noon_compile::CompiledWorldBounds3D64>,
    ) {
        let Some(appearance) = self
            .frame
            .objects
            .get_mut(row)
            .and_then(|o| o.spatial.as_deref_mut())
            .filter(|s| s.material == noon_core::SemanticSpatialMaterial::CairoPath)
            .and_then(|s| s.cairo_path_appearance.as_deref_mut())
        else {
            return;
        };
        if appearance.world_family_bounds == bounds {
            return;
        }
        appearance.world_family_bounds = bounds;
        self.changes.insert(row);
        self.spatial_changes.insert(row);
    }

    fn spatial_anchor_row_center(&self, row: usize) -> Option<noon_core::SemanticVec3> {
        self.world_bounds_center(std::iter::once(u32::try_from(row).ok()?))
    }

    fn world_bounds_center(
        &self,
        rows: impl IntoIterator<Item = u32>,
    ) -> Option<noon_core::SemanticVec3> {
        let mut minimum = noon_core::SemanticVec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut maximum =
            noon_core::SemanticVec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        let mut found = false;
        for index in rows {
            if !self.compiled.object_index_is_live(index) {
                continue;
            }
            let Some(local) = self.compiled.spatial_anchor_local_bounds(index) else {
                continue;
            };
            let object = self.frame.objects.get(index as usize)?;
            let world = object.world_transform().or_else(|| {
                noon_core::SemanticWorldTransform3D::from_2_5d(noon_core::SemanticTransform2_5D {
                    translation: noon_core::SemanticVec3::new(
                        f64::from(object.transform.translation.x),
                        f64::from(object.transform.translation.y),
                        0.0,
                    ),
                    scale: noon_core::SemanticVec3::new(
                        f64::from(object.transform.scale.x),
                        f64::from(object.transform.scale.y),
                        1.0,
                    ),
                    rotation_z: f64::from(object.transform.rotation),
                })
            })?;
            let corners = [
                (local.min_x, local.min_y),
                (local.min_x, local.max_y),
                (local.max_x, local.min_y),
                (local.max_x, local.max_y),
            ];
            for corner in corners {
                let point =
                    world.transform_point(noon_core::SemanticVec3::new(corner.0, corner.1, 0.0))?;
                minimum.x = minimum.x.min(point.x);
                minimum.y = minimum.y.min(point.y);
                minimum.z = minimum.z.min(point.z);
                maximum.x = maximum.x.max(point.x);
                maximum.y = maximum.y.max(point.y);
                maximum.z = maximum.z.max(point.z);
                found = true;
            }
        }
        found.then(|| {
            noon_core::SemanticVec3::new(
                minimum.x * 0.5 + maximum.x * 0.5,
                minimum.y * 0.5 + maximum.y * 0.5,
                minimum.z * 0.5 + maximum.z * 0.5,
            )
        })
    }

    fn set_fixed_orientation_center(
        &mut self,
        object_index: usize,
        center: Option<noon_core::SemanticVec3>,
    ) {
        let Some(spatial) = self
            .frame
            .objects
            .get_mut(object_index)
            .and_then(|object| object.spatial.as_deref_mut())
        else {
            return;
        };
        if spatial.composition_domain
            != noon_core::SemanticSpatialCompositionDomain::FixedOrientation
            || spatial.fixed_orientation_center == center
        {
            return;
        }
        spatial.fixed_orientation_center = center;
        self.changes.insert(object_index);
        self.spatial_changes.insert(object_index);
    }
}
