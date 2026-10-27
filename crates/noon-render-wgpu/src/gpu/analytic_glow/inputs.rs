//! Borrow already filtered child images as ordinary parent-capture draws.
use super::super::source_capture::{FilteredCapture, SourceReplacement};
use super::*;

impl GpuRenderer {
    pub(in crate::gpu) fn retained_input_is_ready(&self, index: usize, object: ObjectId) -> bool {
        self.retained_input(index, object).is_some_and(|scope| {
            !scope.invalidated
                && scope.camera == self.camera
                && scope.viewport == self.viewport_size
        })
    }

    fn retained_input(&self, index: usize, object: ObjectId) -> Option<&ResidentEffect> {
        let packed = self.retained_effects.packed_capture_owner(index, object)?;
        let cache = self.effect_captures.as_deref()?;
        cache
            .scopes
            .get(cache.owners.get(&packed)?)
            .filter(|scope| scope.enabled)
    }

    fn filtered_input(
        &self,
        index: usize,
        object: ObjectId,
    ) -> Result<Option<FilteredCapture>, EffectPrepareError> {
        let Some(scope) = self.retained_input(index, object) else {
            return Ok(None);
        };
        if scope.camera != self.camera || scope.viewport != self.viewport_size {
            return Err(EffectPrepareError::PublicationMismatch);
        }
        let texture = scope
            .filter
            .output()
            .ok_or(EffectPrepareError::PublicationMismatch)?
            .clone();
        let origin = scope.tile.origin;
        let size = scope.tile.size;
        let x = |pixel: f64| {
            (f64::from(self.camera.center.x)
                + (pixel - f64::from(self.viewport_size[0]) * 0.5)
                    * f64::from(self.camera.world_size.x)
                    / f64::from(self.viewport_size[0])) as f32
        };
        let y = |pixel: f64| {
            (f64::from(self.camera.center.y)
                - (pixel - f64::from(self.viewport_size[1]) * 0.5)
                    * f64::from(self.camera.world_size.y)
                    / f64::from(self.viewport_size[1])) as f32
        };
        let bounds = noon_core::Rect::new(
            Vec2::new(
                x(scope.contribution_bounds.min[0]),
                y(scope.contribution_bounds.max[1]),
            ),
            Vec2::new(
                x(scope.contribution_bounds.max[0]),
                y(scope.contribution_bounds.min[1]),
            ),
        );
        Ok(Some(FilteredCapture {
            object,
            texture,
            revision: scope.output_revision,
            origin,
            size,
            bounds,
        }))
    }

    pub(super) fn prepare_composed_replacements(
        &self,
        prepared: &super::super::retained_text::PreparedRetainedGpuFrame<'_>,
        owner: usize,
        members: &[ObjectId],
    ) -> Result<Vec<SourceReplacement>, EffectPrepareError> {
        let mut replacements = Vec::new();
        let mut at = 0;
        while at < members.len() {
            let object = members[at];
            let index = prepared
                .capture_index(object)
                .ok_or(EffectPrepareError::PublicationMismatch)?;
            let candidate = self
                .retained_effects
                .composed_candidates(owner, index)
                .filter(|(index, owner, _, _, _)| self.retained_input(*index, *owner).is_some())
                .min_by_key(|(_, _, _, _, depth)| *depth);
            let (capture, count) = if let Some((index, owner, count, first, _)) = candidate {
                if first != object || count > members.len() - at {
                    return Err(EffectPrepareError::PublicationMismatch);
                }
                (self.filtered_input(index, owner)?, count)
            } else {
                (self.filtered_input(index, object)?, 1)
            };
            if let Some(capture) = capture {
                replacements.push(SourceReplacement {
                    members: at..at + count,
                    capture,
                });
            }
            at += count;
        }
        Ok(replacements)
    }
}
