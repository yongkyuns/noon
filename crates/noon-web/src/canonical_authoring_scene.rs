use std::collections::BTreeMap;

use noon_core::{
    FamilyAnimationRequest, ObjectId, ObjectSnapshot, Style, TrackDefinition, Vec2,
};
use noon_ir::{ObjectSpec, ObjectSpecContent, SceneSpec, TextSpec};

use crate::{
    materialize_retained_tracks, RetainedTextAuthoringSpec, RetainedTextBackendSpec,
    RetainedTrackAuthoringSpec,
};

/// Per-scene canonical authoring state.
///
/// Geometry and source-level text are appended to one Rust-owned object vector in
/// authoring order. Legacy geometry documents and retained-text sidecars may still
/// be projected for compatibility, but they are not inputs to canonical SceneSpec
/// production.
#[derive(Clone, Debug, Default)]
pub struct CanonicalAuthoringSceneContext {
    objects: Vec<ObjectSpec>,
    positions: BTreeMap<ObjectId, usize>,
    retained_scale_factors: BTreeMap<ObjectId, Vec2>,
}

impl CanonicalAuthoringSceneContext {
    pub fn bind_geometry(
        &mut self,
        id: ObjectId,
        snapshot: ObjectSnapshot,
    ) -> Result<(), String> {
        let mut object = ObjectSpec::geometry(id, snapshot.geometry);
        object.transform = snapshot.transform;
        object.style = snapshot.style;
        self.insert(id, object, None)
    }

    pub fn update_geometry(
        &mut self,
        id: ObjectId,
        snapshot: ObjectSnapshot,
    ) -> Result<(), String> {
        let position = self.position(id)?;
        if !matches!(self.objects[position].content, ObjectSpecContent::Geometry(_)) {
            return Err(format!("canonical object {} is not geometry-backed", id.get()));
        }
        let mut object = ObjectSpec::geometry(id, snapshot.geometry);
        object.transform = snapshot.transform;
        object.style = snapshot.style;
        self.objects[position] = object;
        Ok(())
    }

    pub fn bind_text(
        &mut self,
        id: ObjectId,
        text: RetainedTextAuthoringSpec,
    ) -> Result<(), String> {
        let scale_factor = retained_scale_factor(&text);
        let object = canonical_text_object(id, text)?;
        self.insert(id, object, Some(scale_factor))
    }

    pub fn update_text(
        &mut self,
        id: ObjectId,
        text: RetainedTextAuthoringSpec,
    ) -> Result<(), String> {
        let position = self.position(id)?;
        if !matches!(self.objects[position].content, ObjectSpecContent::Text(_)) {
            return Err(format!("canonical object {} is not text-backed", id.get()));
        }
        let scale_factor = retained_scale_factor(&text);
        self.objects[position] = canonical_text_object(id, text)?;
        self.retained_scale_factors.insert(id, scale_factor);
        Ok(())
    }

    pub const fn checkpoint(&self) -> usize {
        self.objects.len()
    }

    pub fn restore(&mut self, checkpoint: usize) -> Result<(), String> {
        if checkpoint > self.objects.len() {
            return Err(format!(
                "canonical authoring checkpoint {checkpoint} exceeds object count {}",
                self.objects.len()
            ));
        }
        while self.objects.len() > checkpoint {
            let object = self.objects.pop().expect("length checked above");
            self.positions.remove(&object.id);
            self.retained_scale_factors.remove(&object.id);
        }
        Ok(())
    }

    pub fn finalize(
        &self,
        geometry_tracks: Vec<TrackDefinition>,
        retained_tracks: Vec<RetainedTrackAuthoringSpec>,
        family_animations: Vec<FamilyAnimationRequest>,
        camera_object: Option<ObjectId>,
    ) -> Result<SceneSpec, String> {
        let tracks = materialize_retained_tracks(
            &geometry_tracks,
            retained_tracks,
            &self.retained_scale_factors,
        )
        .map_err(|error| error.to_string())?;
        let mut spec =
            SceneSpec::new(self.objects.clone(), tracks).map_err(|error| error.to_string())?;
        spec.family_animations = family_animations;
        spec.camera_object = camera_object;
        spec.validate().map_err(|error| error.to_string())?;
        Ok(spec)
    }

    fn insert(
        &mut self,
        id: ObjectId,
        object: ObjectSpec,
        retained_scale_factor: Option<Vec2>,
    ) -> Result<(), String> {
        if self.positions.contains_key(&id) {
            return Err(format!("canonical object {} is already bound", id.get()));
        }
        let position = self.objects.len();
        self.objects.push(object);
        self.positions.insert(id, position);
        if let Some(scale_factor) = retained_scale_factor {
            self.retained_scale_factors.insert(id, scale_factor);
        }
        Ok(())
    }

    fn position(&self, id: ObjectId) -> Result<usize, String> {
        self.positions
            .get(&id)
            .copied()
            .ok_or_else(|| format!("unknown canonical object {}", id.get()))
    }
}

fn retained_scale_factor(text: &RetainedTextAuthoringSpec) -> Vec2 {
    let factor = match &text.backend {
        RetainedTextBackendSpec::Native { .. } => noon::NATIVE_POINT_TO_SCENE_SCALE,
        RetainedTextBackendSpec::Typst { .. } => {
            text.font_size * noon::SCALE_FACTOR_PER_FONT_POINT
        }
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

    #[wasm_bindgen(js_name = CanonicalAuthoringSceneContext)]
    pub struct WasmCanonicalAuthoringSceneContext {
        inner: CanonicalAuthoringSceneContext,
    }

    #[wasm_bindgen(js_class = CanonicalAuthoringSceneContext)]
    impl WasmCanonicalAuthoringSceneContext {
        #[wasm_bindgen(constructor)]
        pub fn new() -> Self {
            Self {
                inner: CanonicalAuthoringSceneContext::default(),
            }
        }

        #[wasm_bindgen(js_name = bindGeometry)]
        pub fn bind_geometry(&mut self, object_id: &str, snapshot_json: &str) -> Result<(), JsValue> {
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
            self.inner
                .restore(checkpoint as usize)
                .map_err(js_error)
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

    fn native_text(source: &str) -> RetainedTextAuthoringSpec {
        RetainedTextAuthoringSpec::native(source, "DejaVu Sans Mono", 48.0, 0.5).unwrap()
    }

    #[test]
    fn mixed_bind_events_define_the_canonical_object_stream_directly() {
        let mut context = CanonicalAuthoringSceneContext::default();
        context
            .bind_geometry(ObjectId::new(0), ObjectSnapshot::new(GeometryRef::circle(0.5)))
            .unwrap();
        context.bind_text(ObjectId::new(1), native_text("A")).unwrap();
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
            spec.objects.iter().map(|object| object.id).collect::<Vec<_>>(),
            vec![ObjectId::new(0), ObjectId::new(1), ObjectId::new(2)]
        );
        let ObjectSpecContent::Text(text) = &spec.objects[1].content else {
            panic!("middle object must be source-level text");
        };
        assert_eq!(text.kind, TextSpecKind::Plain);
        assert_eq!(text.source, "A");
    }

    #[test]
    fn updates_preserve_slots_and_checkpoint_restore_reclaims_failed_binds() {
        let mut context = CanonicalAuthoringSceneContext::default();
        let first = ObjectId::new(0);
        context
            .bind_geometry(first, ObjectSnapshot::new(GeometryRef::circle(0.5)))
            .unwrap();
        let checkpoint = context.checkpoint();
        context.bind_text(ObjectId::new(1), native_text("temporary")).unwrap();
        context.restore(checkpoint).unwrap();

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
        let mut context = CanonicalAuthoringSceneContext::default();
        let id = ObjectId::new(7);
        context.bind_text(id, native_text("stable")).unwrap();
        let error = context
            .update_geometry(id, ObjectSnapshot::new(GeometryRef::circle(1.0)))
            .unwrap_err();
        assert!(error.contains("not geometry-backed"));
    }
}
