//! Scoped Position leases used by the session-owned native translation drag.

use noon_core::{ObjectId, Property, Vec2};

use crate::SceneInstance;

impl SceneInstance {
    pub fn translation_drag_active(&self, id: ObjectId) -> bool {
        self.frame_index_for_object(id)
            .is_some_and(|index| self.translation_drag_rows.contains_key(&index))
    }

    pub fn has_viewer_drag_targets(&self) -> bool {
        self.compiled.has_viewer_drag_targets()
    }

    /// Use the same Position ownership checks as the authored drag. Viewer
    /// gestures may reacquire an existing viewer-owned translation.
    pub fn can_viewer_drag(&self, id: ObjectId) -> bool {
        self.compiled.viewer_draggable(id)
            && self.active_family_animation_indices.is_empty()
            && !self.translation_drag_driver_conflicts(id)
            && self
                .effective_object(id)
                .is_some_and(|row| row.spatial.is_none())
    }

    pub fn translation_drag_conflicts(&self, id: ObjectId) -> bool {
        self.translation_drag_active(id) || self.translation_drag_driver_conflicts(id)
    }

    fn translation_drag_driver_conflicts(&self, id: ObjectId) -> bool {
        let Some(index) = self.frame_index_for_object(id) else {
            return true;
        };
        self.effective_driver_rows.contains(&index)
            || self.numeric_text.has_object_driver(index)
            || self
                .transient_animations
                .owns_property(id, Property::Position)
            || self
                .reactive
                .as_ref()
                .is_some_and(|reactive| reactive.owns_property(id, Property::Position))
            || self
                .compiled
                .object_channels(id)
                .iter()
                .any(|channel| matches!(channel.property, Property::Position | Property::Transform))
    }

    /// Commit a presentation-only Position write in a retained reader.
    pub fn set_viewer_translation(
        &mut self,
        id: ObjectId,
        translation: Vec2,
    ) -> Result<(), String> {
        if !self.replay_is_sealed() || !self.can_viewer_drag(id) {
            return Err(
                "viewer translation requires a retained frame and an available target".into(),
            );
        }
        self.publish_viewer_translations(&[crate::EffectivePropertyWrite::Translation {
            object: id,
            translation,
        }])?;
        if self.translation_drag_base(id) == Some(translation) {
            self.release_translation_drag(id);
        } else {
            self.adopt_translation_drag(id, translation);
        }
        Ok(())
    }

    /// Before forward play, release viewer leases and restore their base
    /// positions. Acquisition excludes all other Position drivers.
    pub fn clear_viewer_translations(&mut self) -> Result<(), String> {
        if self.translation_drag_rows.is_empty() {
            return Ok(());
        }
        if !self.replay_is_sealed() {
            return Err("viewer translation requires a retained frame".into());
        }
        let writes: Vec<_> = self
            .translation_drag_rows
            .keys()
            .map(|&index| {
                let object = &self.compiled.objects()[index];
                crate::EffectivePropertyWrite::Translation {
                    object: object.id,
                    translation: object.base_transform.translation,
                }
            })
            .collect();
        self.publish_viewer_translations(&writes)?;
        self.translation_drag_rows.clear();
        Ok(())
    }

    fn publish_viewer_translations(
        &mut self,
        writes: &[crate::EffectivePropertyWrite],
    ) -> Result<(), String> {
        let effective = self
            .prepare_effective_property_batch(writes)
            .map_err(|error| error.to_string())?;
        self.commit_transient_effective_properties(effective)
            .map_err(|error| error.to_string())?;
        self.flush_spatial_anchor_changes();
        Ok(())
    }

    pub fn adopt_translation_drag(&mut self, id: ObjectId, translation: Vec2) {
        let index = self
            .frame_index_for_object(id)
            .expect("preflighted pointer drag object remains live during input commit");
        self.effective_driver_rows.remove(&index);
        self.translation_drag_rows.insert(index, translation);
    }

    pub fn suspend_translation_drag(&mut self, id: ObjectId) -> Option<Vec2> {
        let index = self.frame_index_for_object(id)?;
        let translation = self.translation_drag_rows.remove(&index)?;
        self.effective_driver_rows.insert(index);
        Some(translation)
    }

    pub fn translation_drag_base(&self, id: ObjectId) -> Option<Vec2> {
        let index = self.frame_index_for_object(id)?;
        Some(
            self.compiled
                .objects()
                .get(index)?
                .base_transform
                .translation,
        )
    }

    pub fn clear_translation_drag_effective_driver(&mut self, id: ObjectId) {
        if let Some(index) = self.frame_index_for_object(id) {
            self.effective_driver_rows.remove(&index);
        }
    }

    pub fn restore_translation_drag(&mut self, id: ObjectId, translation: Vec2) {
        let index = self
            .frame_index_for_object(id)
            .expect("suspended pointer drag object remains live without structural publication");
        self.effective_driver_rows.remove(&index);
        self.translation_drag_rows.insert(index, translation);
    }

    pub fn release_translation_drag(&mut self, id: ObjectId) -> bool {
        self.frame_index_for_object(id)
            .is_some_and(|index| self.translation_drag_rows.remove(&index).is_some())
    }
}
