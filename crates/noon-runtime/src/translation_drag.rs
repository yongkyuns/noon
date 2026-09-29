//! Scoped Position leases used by the session-owned native translation drag.

use noon_core::{ObjectId, Property, Vec2};

use crate::SceneInstance;

impl SceneInstance {
    pub fn translation_drag_active(&self, id: ObjectId) -> bool {
        self.frame_index_for_object(id)
            .is_some_and(|index| self.translation_drag_rows.contains_key(&index))
    }

    pub fn translation_drag_conflicts(&self, id: ObjectId) -> bool {
        let Some(index) = self.frame_index_for_object(id) else {
            return true;
        };
        self.effective_driver_rows.contains(&index)
            || self.translation_drag_rows.contains_key(&index)
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
