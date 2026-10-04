//! Sparse fixed-orientation anchor evaluation on the existing effective frame.

use crate::SceneInstance;

impl SceneInstance {
    pub(super) fn flush_fixed_orientation_anchor_changes(&mut self) {
        let groups = std::mem::take(&mut self.pending_fixed_orientation_anchor_groups);
        for group in groups {
            let center = self.fixed_orientation_anchor_group_center(group);
            for member_index in 0..self.compiled.fixed_orientation_group_members(group).len() {
                let member = self.compiled.fixed_orientation_group_members(group)[member_index];
                self.set_fixed_orientation_center(member as usize, center);
            }
        }
        let rows = std::mem::take(&mut self.pending_fixed_orientation_anchor_rows);
        for row in rows {
            let center = self.fixed_orientation_row_center(row);
            self.set_fixed_orientation_center(row, center);
        }
    }

    pub(super) fn refresh_all_fixed_orientation_anchors(&mut self) {
        let groups = (0..self.compiled.fixed_orientation_group_count() as u32).collect::<Vec<_>>();
        for group in groups {
            let center = self.fixed_orientation_anchor_group_center(group);
            for member_index in 0..self.compiled.fixed_orientation_group_members(group).len() {
                let member = self.compiled.fixed_orientation_group_members(group)[member_index];
                self.set_fixed_orientation_center(member as usize, center);
            }
        }
        for row in 0..self.frame.objects.len() {
            if self
                .compiled
                .fixed_orientation_group_for_row(row as u32)
                .is_none()
                && self.frame.objects[row]
                    .spatial
                    .as_deref()
                    .is_some_and(|spatial| {
                        spatial.composition_domain
                            == noon_core::SemanticSpatialCompositionDomain::FixedOrientation
                    })
            {
                let center = self.fixed_orientation_row_center(row);
                self.set_fixed_orientation_center(row, center);
            }
        }
    }

    fn fixed_orientation_anchor_group_center(&self, group: u32) -> Option<noon_core::SemanticVec3> {
        let members = self.compiled.fixed_orientation_group_bounds_members(group);
        if members.is_empty() {
            return None;
        }
        self.world_bounds_center(members.iter().copied())
    }

    fn fixed_orientation_row_center(&self, row: usize) -> Option<noon_core::SemanticVec3> {
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
            let Some(local) = self.compiled.fixed_orientation_local_bounds(index) else {
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
