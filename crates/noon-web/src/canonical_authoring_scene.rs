use std::collections::BTreeMap;

use noon_core::{FamilyAnimationRequest, ObjectId, ObjectSnapshot, Style, TrackDefinition, Vec2};
use noon_ir::{ObjectSpec, SceneSpec, TextSpec};

use crate::{
    materialize_retained_tracks, RetainedTextAuthoringSpec, RetainedTextBackendSpec,
    RetainedTrackAuthoringSpec,
};

/// One scene family in the worker's shared semantic store.
/// Geometry bindings retain identity only. Source-level text remains a deletion-owned
/// export adapter (#959); it cannot enter geometry-only typed execution silently.
#[derive(Debug)]
pub struct CanonicalAuthoringScene {
    scene: noon::Scene,
    bindings: BTreeMap<ObjectId, noon_core::SemanticNodeId>,
    identities: BTreeMap<noon_core::SemanticNodeId, ObjectId>,
    text_adapters: BTreeMap<noon_core::SemanticNodeId, ObjectSpec>,
    retained_scale_factors: BTreeMap<ObjectId, Vec2>,
}

impl Default for CanonicalAuthoringScene {
    fn default() -> Self {
        Self::with_store(std::rc::Rc::new(std::cell::RefCell::new(
            noon_core::SemanticStore::new(),
        )))
    }
}

impl CanonicalAuthoringScene {
    pub fn with_store(
        semantics: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
    ) -> Self {
        let scene = noon::Scene::with_store(semantics);
        Self {
            scene,
            bindings: BTreeMap::new(),
            identities: BTreeMap::new(),
            text_adapters: BTreeMap::new(),
            retained_scale_factors: BTreeMap::new(),
        }
    }

    pub fn bind_mobject(&mut self, id: ObjectId, handle: &noon::Mobject) -> Result<(), String> {
        if !std::rc::Rc::ptr_eq(self.scene.store(), handle.store()) {
            return Err("mobject belongs to another authoring store".into());
        }
        handle.validate()?;
        self.bind_node(id, handle.node_id())
    }

    fn bind_node(&mut self, id: ObjectId, node: noon_core::SemanticNodeId) -> Result<(), String> {
        if self.bindings.contains_key(&id) || self.identities.contains_key(&node) {
            return Err(format!("canonical object {} is already bound", id.get()));
        }
        let mut transaction = noon_core::SemanticMutationTransaction::new();
        transaction.add_member(self.scene.root(), node);
        transaction
            .apply(&mut self.scene.store().borrow_mut())
            .map_err(|error| error.to_string())?;
        self.bindings.insert(id, node);
        self.identities.insert(node, id);
        Ok(())
    }

    /// Snapshot import is an explicit compatibility boundary, never the typed bind path.
    pub fn bind_geometry(&mut self, id: ObjectId, snapshot: ObjectSnapshot) -> Result<(), String> {
        if self.bindings.contains_key(&id) {
            return Err(format!("canonical object {} is already bound", id.get()));
        }
        let handle = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(self.scene.store()),
            snapshot,
        )?;
        self.bind_mobject(id, &handle)
    }

    pub fn update_geometry(
        &mut self,
        id: ObjectId,
        snapshot: ObjectSnapshot,
    ) -> Result<(), String> {
        let node = self.node(id)?;
        if self.text_adapters.contains_key(&node) {
            return Err(format!(
                "canonical object {} is not geometry-backed",
                id.get()
            ));
        }
        let mut handle = noon::Mobject::from_node(std::rc::Rc::clone(self.scene.store()), node)?;
        noon::legacy::replace_mobject_snapshot(&mut handle, snapshot)
    }

    pub fn bind_text(
        &mut self,
        id: ObjectId,
        text: RetainedTextAuthoringSpec,
    ) -> Result<(), String> {
        let scale_factor = retained_scale_factor(&text);
        let object = canonical_text_object(id, text)?;
        if self.bindings.contains_key(&id) {
            return Err(format!("canonical object {} is already bound", id.get()));
        }
        let node = self.scene.store().borrow_mut().insert_authoring_object();
        // The explicit retained-text export adapter still uses its historical
        // identity-only node; it is never admitted to typed geometry execution.
        self.scene
            .store()
            .borrow_mut()
            .add_member(self.scene.root(), node)
            .map_err(|e| e.to_string())?;
        self.bindings.insert(id, node);
        self.identities.insert(node, id);
        self.text_adapters.insert(node, object);
        self.retained_scale_factors.insert(id, scale_factor);
        Ok(())
    }

    pub fn update_text(
        &mut self,
        id: ObjectId,
        text: RetainedTextAuthoringSpec,
    ) -> Result<(), String> {
        let node = self.node(id)?;
        if !self.text_adapters.contains_key(&node) {
            return Err(format!("canonical object {} is not text-backed", id.get()));
        }
        let scale_factor = retained_scale_factor(&text);
        let object = canonical_text_object(id, text)?;
        self.text_adapters.insert(node, object);
        self.retained_scale_factors.insert(id, scale_factor);
        Ok(())
    }

    fn members(&self) -> Result<Vec<noon_core::SemanticNodeId>, String> {
        self.scene
            .store()
            .borrow()
            .node(self.scene.root())
            .map(|node| node.members().to_vec())
            .ok_or_else(|| "semantic scene root is no longer live".into())
    }

    pub fn checkpoint(&self) -> usize {
        self.bindings.len()
    }

    pub fn restore(&mut self, checkpoint: usize) -> Result<(), String> {
        let members = self.members()?;
        if checkpoint > members.len() {
            return Err(format!(
                "canonical authoring checkpoint {checkpoint} exceeds object count {}",
                members.len()
            ));
        }
        let removed = &members[checkpoint..];
        let mut transaction = noon_core::SemanticMutationTransaction::new();
        for node in removed {
            if !self.text_adapters.contains_key(node) {
                transaction.remove_member(self.scene.root(), *node);
            }
        }
        transaction
            .apply(&mut self.scene.store().borrow_mut())
            .map_err(|error| error.to_string())?;
        for node in removed {
            if self.text_adapters.contains_key(node) {
                self.scene
                    .store()
                    .borrow_mut()
                    .remove_member(self.scene.root(), *node)
                    .map_err(|e| e.to_string())?;
            }
        }
        self.bindings.retain(|id, node| {
            if removed.contains(node) {
                self.retained_scale_factors.remove(id);
                false
            } else {
                true
            }
        });
        for node in removed {
            self.text_adapters.remove(node);
            self.identities.remove(node);
        }
        Ok(())
    }

    pub fn lower_execution(&self) -> Result<noon::ExecutionSession, String> {
        if !self.text_adapters.is_empty() {
            return Err("retained text requires the explicit retained execution adapter".into());
        }
        self.scene
            .execution_session()
            .map_err(|error| error.to_string())
    }

    /// Derive the migration/export document from live semantic state at the boundary.
    pub fn finalize(
        &self,
        geometry_tracks: Vec<TrackDefinition>,
        retained_tracks: Vec<RetainedTrackAuthoringSpec>,
        family_animations: Vec<FamilyAnimationRequest>,
        camera_object: Option<ObjectId>,
    ) -> Result<SceneSpec, String> {
        let mut objects = Vec::with_capacity(self.identities.len());
        for node in self.members()? {
            if let Some(text) = self.text_adapters.get(&node) {
                objects.push(text.clone());
                continue;
            }
            let handle = noon::Mobject::from_node(std::rc::Rc::clone(self.scene.store()), node)?;
            let snapshot = noon::legacy::export_mobject_snapshot(&handle)?;
            let mut object = ObjectSpec::geometry(
                *self
                    .identities
                    .get(&node)
                    .ok_or("unbound semantic scene member")?,
                snapshot.geometry,
            );
            object.transform = snapshot.transform;
            object.style = snapshot.style;
            objects.push(object);
        }
        let tracks = materialize_retained_tracks(
            &geometry_tracks,
            retained_tracks,
            &self.retained_scale_factors,
        )
        .map_err(|error| error.to_string())?;
        let mut spec = SceneSpec::new(objects, tracks).map_err(|error| error.to_string())?;
        spec.family_animations = family_animations;
        spec.camera_object = camera_object;
        spec.validate().map_err(|error| error.to_string())?;
        Ok(spec)
    }

    fn node(&self, id: ObjectId) -> Result<noon_core::SemanticNodeId, String> {
        self.bindings
            .get(&id)
            .copied()
            .ok_or_else(|| format!("unknown canonical object {}", id.get()))
    }
}

fn retained_scale_factor(text: &RetainedTextAuthoringSpec) -> Vec2 {
    let factor = match &text.backend {
        RetainedTextBackendSpec::Native { .. } => noon::NATIVE_POINT_TO_SCENE_SCALE,
        RetainedTextBackendSpec::Typst { .. } => text.font_size * noon::SCALE_FACTOR_PER_FONT_POINT,
    };
    Vec2::new(factor, factor)
}

fn canonical_text_object(
    id: ObjectId,
    text: RetainedTextAuthoringSpec,
) -> Result<ObjectSpec, String> {
    text.validate()?;
    let RetainedTextAuthoringSpec {
        source,
        backend,
        font_size,
        transform,
        color,
        opacity,
    } = text;
    let text = match backend {
        RetainedTextBackendSpec::Native {
            font_family,
            line_spacing,
        } => TextSpec::native_plain(source, font_family, font_size, line_spacing),
        RetainedTextBackendSpec::Typst { math: false } => TextSpec::typst(source, font_size),
        RetainedTextBackendSpec::Typst { math: true } => TextSpec::math_typst(source, font_size),
    };
    let mut object = ObjectSpec::text(id, text);
    object.transform = transform;
    object.style = Style {
        fill: Some(color),
        stroke: None,
        stroke_width: 0.0,
        opacity,
        ..Style::default()
    };
    Ok(object)
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use serde::de::DeserializeOwned;
    use wasm_bindgen::prelude::*;

    use super::*;

    fn js_error(error: impl ToString) -> JsValue {
        JsValue::from_str(&error.to_string())
    }

    fn parse_json<T: DeserializeOwned>(label: &str, json: &str) -> Result<T, JsValue> {
        serde_json::from_str(json).map_err(|error| js_error(format!("invalid {label}: {error}")))
    }

    fn parse_object_id(label: &str, value: &str) -> Result<ObjectId, JsValue> {
        value
            .parse::<u64>()
            .map(ObjectId::new)
            .map_err(|error| js_error(format!("invalid {label} {value:?}: {error}")))
    }

    #[wasm_bindgen]
    pub struct CanonicalAuthoringSceneContext {
        inner: CanonicalAuthoringScene,
    }

    impl CanonicalAuthoringSceneContext {
        pub(crate) fn with_store(
            store: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        ) -> Self {
            Self {
                inner: CanonicalAuthoringScene::with_store(store),
            }
        }
    }

    #[wasm_bindgen]
    impl CanonicalAuthoringSceneContext {
        #[wasm_bindgen(js_name = bindMobject)]
        pub fn bind_mobject(
            &mut self,
            object_id: &str,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let id = parse_object_id("object ID", object_id)?;
            self.inner
                .bind_mobject(id, handle.semantic_mobject())
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = createExecutionPlayer)]
        pub fn create_execution_player(
            &self,
            duration: f64,
            session: u32,
        ) -> Result<crate::SemanticExecutionPlayer, JsValue> {
            let execution = self.inner.lower_execution().map_err(js_error)?;
            crate::SemanticExecutionPlayer::from_session(execution, duration, session)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = bindGeometry)]
        pub fn bind_geometry(
            &mut self,
            object_id: &str,
            snapshot_json: &str,
        ) -> Result<(), JsValue> {
            let id = parse_object_id("object ID", object_id)?;
            let snapshot = parse_json::<ObjectSnapshot>("geometry snapshot", snapshot_json)?;
            self.inner.bind_geometry(id, snapshot).map_err(js_error)
        }

        #[wasm_bindgen(js_name = updateGeometry)]
        pub fn update_geometry(
            &mut self,
            object_id: &str,
            snapshot_json: &str,
        ) -> Result<(), JsValue> {
            let id = parse_object_id("object ID", object_id)?;
            let snapshot = parse_json::<ObjectSnapshot>("geometry snapshot", snapshot_json)?;
            self.inner.update_geometry(id, snapshot).map_err(js_error)
        }

        #[wasm_bindgen(js_name = bindText)]
        pub fn bind_text(&mut self, object_id: &str, text_json: &str) -> Result<(), JsValue> {
            let id = parse_object_id("object ID", object_id)?;
            let text = parse_json::<RetainedTextAuthoringSpec>("retained text spec", text_json)?;
            self.inner.bind_text(id, text).map_err(js_error)
        }

        #[wasm_bindgen(js_name = updateText)]
        pub fn update_text(&mut self, object_id: &str, text_json: &str) -> Result<(), JsValue> {
            let id = parse_object_id("object ID", object_id)?;
            let text = parse_json::<RetainedTextAuthoringSpec>("retained text spec", text_json)?;
            self.inner.update_text(id, text).map_err(js_error)
        }

        pub fn checkpoint(&self) -> u32 {
            u32::try_from(self.inner.checkpoint()).expect("canonical object count fits u32")
        }

        pub fn restore(&mut self, checkpoint: u32) -> Result<(), JsValue> {
            self.inner.restore(checkpoint as usize).map_err(js_error)
        }

        #[wasm_bindgen(js_name = sceneSpecJson)]
        pub fn scene_spec_json(
            &self,
            geometry_tracks_json: &str,
            retained_tracks_json: &str,
            family_animations_json: &str,
            camera_object_id: &str,
        ) -> Result<String, JsValue> {
            let geometry_tracks =
                parse_json::<Vec<TrackDefinition>>("geometry tracks", geometry_tracks_json)?;
            let retained_tracks = parse_json::<Vec<RetainedTrackAuthoringSpec>>(
                "retained tracks",
                retained_tracks_json,
            )?;
            let family_animations = parse_json::<Vec<FamilyAnimationRequest>>(
                "family animations",
                family_animations_json,
            )?;
            let camera_object = if camera_object_id.is_empty() {
                None
            } else {
                Some(parse_object_id("camera object ID", camera_object_id)?)
            };
            let spec = self
                .inner
                .finalize(
                    geometry_tracks,
                    retained_tracks,
                    family_animations,
                    camera_object,
                )
                .map_err(js_error)?;
            serde_json::to_string(&spec).map_err(js_error)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, Transform2D};
    use noon_ir::{ObjectSpecContent, TextSpecKind};

    use super::*;

    #[test]
    fn typed_binding_shares_state_and_root_without_snapshot_synchronization() {
        use std::{cell::RefCell, rc::Rc};
        let store = Rc::new(RefCell::new(noon_core::SemanticStore::new()));
        let mut context = CanonicalAuthoringScene::with_store(Rc::clone(&store));
        let mut object = noon::Mobject::manim_circle(Rc::clone(&store), 1.0).unwrap();
        let id = object.node_id();
        context.bind_mobject(ObjectId::new(42), &object).unwrap();
        object.shift(2.0, -1.0).unwrap();
        let execution = context.lower_execution().unwrap();
        assert_eq!(
            execution.execution_object_id(id),
            Some(execution.frame().objects[0].id)
        );
        assert_eq!(
            execution.frame().objects[0].transform.translation,
            Vec2::new(2.0, -1.0)
        );
        let mut other = CanonicalAuthoringScene::with_store(Rc::clone(&store));
        assert!(other.lower_execution().unwrap().frame().objects.is_empty());
        other.bind_mobject(ObjectId::new(0), &object).unwrap();
        context.restore(0).unwrap();
        assert!(context
            .lower_execution()
            .unwrap()
            .frame()
            .objects
            .is_empty());
        assert_eq!(other.lower_execution().unwrap().frame().objects.len(), 1);
        context.bind_mobject(ObjectId::new(42), &object).unwrap();
        assert_eq!(
            context.lower_execution().unwrap().execution_object_id(id),
            execution.execution_object_id(id)
        );
    }

    #[test]
    fn typed_binding_rejects_cross_store_collisions_atomically() {
        let mut first = CanonicalAuthoringScene::default();
        let second = CanonicalAuthoringScene::default();
        let local = first.scene.circle(1.0).unwrap();
        let foreign = second.scene.circle(2.0).unwrap();
        assert_eq!(local.node_id(), foreign.node_id());
        let revision = first.scene.store().borrow().scene_revision();
        assert!(first.bind_mobject(ObjectId::new(0), &foreign).is_err());
        assert_eq!(first.checkpoint(), 0);
        assert_eq!(first.scene.store().borrow().scene_revision(), revision);
        first.bind_mobject(ObjectId::new(0), &local).unwrap();
    }

    fn native_text(source: &str) -> RetainedTextAuthoringSpec {
        RetainedTextAuthoringSpec::native(source, "DejaVu Sans Mono", 48.0, 0.5).unwrap()
    }

    #[test]
    fn mixed_bind_events_define_the_canonical_object_stream_directly() {
        let mut context = CanonicalAuthoringScene::default();
        context
            .bind_geometry(
                ObjectId::new(0),
                ObjectSnapshot::new(GeometryRef::circle(0.5)),
            )
            .unwrap();
        context
            .bind_text(ObjectId::new(1), native_text("A"))
            .unwrap();
        context
            .bind_geometry(
                ObjectId::new(2),
                ObjectSnapshot::new(GeometryRef::rectangle(1.0, 1.0)),
            )
            .unwrap();

        let spec = context
            .finalize(Vec::new(), Vec::new(), Vec::new(), None)
            .unwrap();
        assert_eq!(
            spec.objects
                .iter()
                .map(|object| object.id)
                .collect::<Vec<_>>(),
            vec![ObjectId::new(0), ObjectId::new(1), ObjectId::new(2)]
        );
        let ObjectSpecContent::Text(text) = &spec.objects[1].content else {
            panic!("middle object must be source-level text");
        };
        assert_eq!(text.kind, TextSpecKind::Plain);
        assert_eq!(text.source, "A");
    }

    #[test]
    fn updates_preserve_slots_and_append_checkpoint_restore_reclaims_failed_binds() {
        let mut context = CanonicalAuthoringScene::default();
        let first = ObjectId::new(0);
        context
            .bind_geometry(first, ObjectSnapshot::new(GeometryRef::circle(0.5)))
            .unwrap();
        let checkpoint = context.checkpoint();
        context
            .bind_text(ObjectId::new(1), native_text("temporary"))
            .unwrap();
        // Checkpoint rollback is intentionally append-only: an update to an
        // existing slot remains visible after the failed bind is reclaimed.
        context
            .update_geometry(first, ObjectSnapshot::new(GeometryRef::circle(0.75)))
            .unwrap();
        context.restore(checkpoint).unwrap();
        let exported = context
            .finalize(Vec::new(), Vec::new(), Vec::new(), None)
            .unwrap();
        let ObjectSpecContent::Geometry(geometry) = &exported.objects[0].content else {
            panic!("first object must remain geometry-backed");
        };
        assert_eq!(geometry, &GeometryRef::circle(0.75));

        let mut replacement = ObjectSnapshot::new(GeometryRef::circle(1.0));
        replacement.transform = Transform2D {
            translation: Vec2::new(2.0, -1.0),
            ..Transform2D::default()
        };
        context.update_geometry(first, replacement).unwrap();
        context
            .bind_geometry(
                ObjectId::new(1),
                ObjectSnapshot::new(GeometryRef::rectangle(2.0, 1.0)),
            )
            .unwrap();

        let spec = context
            .finalize(Vec::new(), Vec::new(), Vec::new(), Some(first))
            .unwrap();
        assert_eq!(spec.objects.len(), 2);
        assert_eq!(spec.objects[0].id, first);
        assert_eq!(spec.objects[0].transform.translation, Vec2::new(2.0, -1.0));
        assert_eq!(spec.camera_object, Some(first));
    }

    #[test]
    fn content_domain_cannot_change_after_binding() {
        let mut context = CanonicalAuthoringScene::default();
        let id = ObjectId::new(7);
        context.bind_text(id, native_text("stable")).unwrap();
        let error = context
            .update_geometry(id, ObjectSnapshot::new(GeometryRef::circle(1.0)))
            .unwrap_err();
        assert!(error.contains("not geometry-backed"));
    }
}
