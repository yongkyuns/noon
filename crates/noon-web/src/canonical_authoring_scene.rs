use std::collections::BTreeMap;

use noon_core::{
    Color, FamilyAnimationRequest, ObjectId, ObjectSnapshot, SemanticObjectState, SemanticPaint,
    SemanticStyle, SemanticTransform2_5D, Style, TextSourceKind, TrackDefinition, Transform2D,
    Vec2,
};
use noon_ir::{ObjectSpec, SceneSpec, TextSpec};
#[cfg(target_arch = "wasm32")]
use noon_ir::{ObjectSpecContent, TextSpecKind, TextSpecOptions};

use crate::{
    materialize_retained_tracks, RetainedTextAuthoringSpec, RetainedTextBackendSpec,
    RetainedTrackAuthoringSpec,
};

/// One scene family in the worker's shared semantic store.
/// Geometry bindings retain identity only. Source-level text remains a deletion-owned
/// export adapter (#959); it cannot enter geometry-only typed execution silently.
pub struct CanonicalAuthoringScene {
    scene: noon::Scene,
    bindings: BTreeMap<ObjectId, noon_core::SemanticNodeId>,
    identities: BTreeMap<noon_core::SemanticNodeId, ObjectId>,
    text_adapters: BTreeMap<noon_core::SemanticNodeId, ObjectSpec>,
    retained_scale_factors: BTreeMap<ObjectId, Vec2>,
    #[cfg(any(target_arch = "wasm32", test))]
    live_player: Option<crate::SemanticExecutionPlayer>,
    #[cfg(any(target_arch = "wasm32", test))]
    live_player_transferred: bool,
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
            #[cfg(any(target_arch = "wasm32", test))]
            live_player: None,
            #[cfg(any(target_arch = "wasm32", test))]
            live_player_transferred: false,
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

    #[cfg(any(target_arch = "wasm32", test))]
    fn live_player(
        &mut self,
        duration: f64,
    ) -> Result<&mut crate::SemanticExecutionPlayer, String> {
        if self.live_player_transferred {
            return Err("live execution session is running in the semantic engine".into());
        }
        if let Some(player) = self.live_player.as_mut() {
            player.set_loop_duration(duration)?;
        } else {
            let execution = self.lower_execution()?;
            self.live_player = Some(crate::SemanticExecutionPlayer::from_live_session(
                execution,
                std::rc::Rc::clone(self.scene.store()),
                self.scene.root(),
                duration,
                0,
            )?);
        }
        Ok(self.live_player.as_mut().expect("live player initialized"))
    }

    /// Begin an explicit authoring-run publication boundary.
    ///
    /// Renderer recovery returns its player to this context and therefore keeps
    /// its effective runtime. A subsequent Python run may mutate authored state
    /// directly before registration; only this boundary is allowed to discard a
    /// now-stale returned runtime and lower a fresh one on attach.
    #[cfg(any(target_arch = "wasm32", test))]
    fn prepare_execution_run(&mut self) -> Result<(), String> {
        if self.live_player_transferred {
            return Err("semantic execution session is running in the semantic engine".into());
        }
        if self
            .live_player
            .as_ref()
            .map(crate::SemanticExecutionPlayer::scene_revision)
            != Some(self.scene.store().borrow().scene_revision())
        {
            self.live_player = None;
        }
        Ok(())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn active_live_player(&mut self) -> Result<&mut crate::SemanticExecutionPlayer, String> {
        if self.live_player_transferred {
            return Err("live execution session is running in the semantic engine".into());
        }
        self.live_player
            .as_mut()
            .ok_or_else(|| "begin live execution before reading or mutating it".into())
    }

    /// Read only the live runtime's authored handoff duration.
    ///
    /// Static authoring has no live session, so its existing authored-duration
    /// projection remains the fallback at the Python export boundary.
    #[cfg(any(target_arch = "wasm32", test))]
    fn live_handoff_duration(&self) -> Option<f64> {
        self.live_player
            .as_ref()
            .and_then(crate::SemanticExecutionPlayer::live_handoff_duration)
    }

    /// Add replayable animation meaning before a live session is created.
    #[cfg(any(target_arch = "wasm32", test))]
    fn declare_live_transform_to(
        &self,
        source: &noon::Mobject,
        target: &noon::Mobject,
        options: noon_core::AnimationOptions,
    ) -> Result<noon::DeclaredAnimation, String> {
        if self.live_player.is_some() || self.live_player_transferred {
            return Err("declare live animations before beginning execution".into());
        }
        self.scene.declare_transform_to(source, target, options)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn live_add_mobject(&mut self, id: ObjectId, handle: &noon::Mobject) -> Result<(), String> {
        if !std::rc::Rc::ptr_eq(self.scene.store(), handle.store()) {
            return Err("mobject belongs to another authoring store".into());
        }
        handle.validate()?;
        let node = handle.node_id();
        let new_binding = match (self.bindings.get(&id), self.identities.get(&node)) {
            (None, None) => true,
            (Some(bound_node), Some(bound_id)) if *bound_node == node && *bound_id == id => false,
            _ => return Err(format!("canonical object {} is already bound", id.get())),
        };
        self.active_live_player()?.live_add(handle)?;
        if new_binding {
            self.bindings.insert(id, node);
            self.identities.insert(node, id);
        }
        Ok(())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn live_remove_mobject(&mut self, handle: &noon::Mobject) -> Result<(), String> {
        if !std::rc::Rc::ptr_eq(self.scene.store(), handle.store()) {
            return Err("mobject belongs to another authoring store".into());
        }
        let node = handle.node_id();
        let id = *self
            .identities
            .get(&node)
            .ok_or("live Mobject is not bound to this Scene")?;
        self.active_live_player()?.live_remove(handle)?;
        self.identities.remove(&node);
        self.bindings.remove(&id);
        Ok(())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn live_replace_content(
        &mut self,
        target: &noon::Mobject,
        source: &noon::Mobject,
    ) -> Result<(), String> {
        if !std::rc::Rc::ptr_eq(self.scene.store(), target.store())
            || !std::rc::Rc::ptr_eq(self.scene.store(), source.store())
        {
            return Err("mobject belongs to another authoring store".into());
        }
        self.active_live_player()?
            .live_replace_content(target, source)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn take_execution_player(
        &mut self,
        duration: f64,
        transport_session: u32,
    ) -> Result<crate::SemanticExecutionPlayer, String> {
        if let Some(player) = self.live_player.as_mut() {
            player.rebind_transport(duration, transport_session)?;
            let player = self.live_player.take().expect("live player initialized");
            self.live_player_transferred = true;
            return Ok(player);
        }
        if self.live_player_transferred {
            return Err("live execution session was already transferred".into());
        }
        let execution = self.lower_execution()?;
        let store = std::rc::Rc::clone(self.scene.store());
        let player = crate::SemanticExecutionPlayer::from_live_session(
            execution,
            store,
            self.scene.root(),
            duration,
            transport_session,
        )?;
        self.live_player_transferred = true;
        Ok(player)
    }

    /// Return a player after endpoint setup or renderer recovery. This preserves
    /// the one runtime so reattachment never lowers a parallel session.
    #[cfg(any(target_arch = "wasm32", test))]
    fn return_execution_player(
        &mut self,
        player: crate::SemanticExecutionPlayer,
    ) -> Result<(), String> {
        if !self.live_player_transferred || self.live_player.is_some() {
            return Err("semantic execution player is not leased by this context".into());
        }
        self.live_player = Some(player);
        self.live_player_transferred = false;
        Ok(())
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
            let state = handle.state()?;
            if state.content.text().is_some() {
                objects.push(canonical_native_text_export(
                    &self.scene.store().borrow(),
                    *self
                        .identities
                        .get(&node)
                        .ok_or("unbound semantic scene member")?,
                    &state,
                )?);
                continue;
            }
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

/// Reconstruct the legacy source document only when the normal semantic session
/// is unavailable (for example, callback/timeline execution). This #959 export
/// seam reads immutable content and presentation from the shared store; Python
/// wrappers never provide a parallel Text source or transform representation.
fn canonical_native_text_export(
    store: &noon_core::SemanticStore,
    id: ObjectId,
    state: &SemanticObjectState,
) -> Result<ObjectSpec, String> {
    let text = state
        .content
        .text()
        .ok_or("canonical text export requires text content")?;
    let resource = store
        .text_resources()
        .get(text)
        .ok_or("canonical text export references an unknown text resource")?;
    if resource.kind != TextSourceKind::Plain {
        return Err("canonical text export supports native plain text only".into());
    }
    let run = resource
        .runs
        .first()
        .ok_or("canonical native text resource has no shaped run")?;
    let mut object = ObjectSpec::text(
        id,
        TextSpec::native_plain(
            resource.source.as_ref(),
            run.font.family.as_ref(),
            run.font_size,
            native_line_spacing(resource)?,
        ),
    );
    object.transform = legacy_text_transform(state.transform)?;
    object.style = legacy_style(&state.style)?;
    Ok(object)
}

/// Derive the temporary #959 native-Text authoring codec from shared semantic
/// state. This is only consumed when the normal live session cannot run; Python
/// wrappers never retain a second text source or presentation model.
#[cfg(target_arch = "wasm32")]
pub(crate) fn canonical_native_text_authoring_spec(
    store: &noon_core::SemanticStore,
    state: &SemanticObjectState,
) -> Result<RetainedTextAuthoringSpec, String> {
    let object = canonical_native_text_export(store, ObjectId::new(0), state)?;
    let ObjectSpec {
        content: ObjectSpecContent::Text(text),
        transform,
        style,
        ..
    } = object
    else {
        return Err("canonical text export produced non-text content".into());
    };
    let TextSpec {
        kind: TextSpecKind::Plain,
        source,
        font_size,
        options:
            TextSpecOptions::NativePlain {
                font_family,
                line_spacing,
            },
    } = text
    else {
        return Err("canonical text export produced non-native content".into());
    };
    let mut spec = RetainedTextAuthoringSpec::native(source, font_family, font_size, line_spacing)?;
    spec.transform = transform;
    spec.color = style.fill.unwrap_or(noon_core::WHITE);
    spec.opacity = style.opacity;
    Ok(spec)
}

fn legacy_text_transform(transform: SemanticTransform2_5D) -> Result<Transform2D, String> {
    let point_scale = f64::from(noon::NATIVE_POINT_TO_SCENE_SCALE);
    Ok(Transform2D {
        translation: Vec2::new(
            legacy_f32("text translation x", transform.translation.x)?,
            legacy_f32("text translation y", transform.translation.y)?,
        ),
        scale: Vec2::new(
            legacy_f32("text scale x", transform.scale.x / point_scale)?,
            legacy_f32("text scale y", transform.scale.y / point_scale)?,
        ),
        rotation: legacy_f32("text rotation", transform.rotation_z)?,
    })
}

fn native_line_spacing(resource: &noon_core::TextResource) -> Result<f32, String> {
    let Some(first) = resource.runs.first() else {
        return Err("canonical native text resource has no shaped run".into());
    };
    if resource.runs.len() < 2 {
        // A single line has no observable line advance. Preserve Manim's ordinary
        // default spelling rather than manufacturing wrapper-side metadata.
        return Ok(-1.0);
    }
    let second = &resource.runs[1];
    let advance = first.transform.ty - second.transform.ty;
    let spacing = advance / first.font_size - 1.0;
    if !spacing.is_finite() || spacing < -1.0 {
        return Err("canonical native text resource has invalid line spacing".into());
    }
    Ok(spacing)
}

fn legacy_style(style: &SemanticStyle) -> Result<Style, String> {
    Ok(Style {
        fill: legacy_color(style.fill.as_ref(), style.fill_opacity)?,
        stroke: legacy_color(style.stroke.as_ref(), style.stroke_opacity)?,
        stroke_width: legacy_f32("text stroke width", style.stroke_width)?,
        stroke_width_mode: style.stroke_width_mode,
        stroke_join: style.stroke_join,
        stroke_cap: style.stroke_cap,
        opacity: legacy_f32("text object opacity", style.object_opacity)?,
    })
}

fn legacy_color(paint: Option<&SemanticPaint>, opacity: f64) -> Result<Option<Color>, String> {
    let Some(paint) = paint else {
        return Ok(None);
    };
    let SemanticPaint::Solid(color) = paint else {
        return Err("legacy text export does not support resource-backed paint".into());
    };
    Ok(Some(Color {
        alpha: legacy_f32("text paint opacity", f64::from(color.alpha) * opacity)?,
        ..*color
    }))
}

fn legacy_f32(name: &str, value: f64) -> Result<f32, String> {
    if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
        return Err(format!("{name} must be a finite f32-compatible number"));
    }
    Ok(value as f32)
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

    #[wasm_bindgen]
    pub struct WasmLiveMobjectState {
        state: noon::EffectiveMobjectState,
    }

    /// Opaque JS/Python wrapper over a replayable shared semantic declaration.
    #[wasm_bindgen]
    pub struct WasmDeclaredAnimationHandle {
        declaration: noon::DeclaredAnimation,
        store: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
    }

    #[wasm_bindgen]
    impl WasmLiveMobjectState {
        #[wasm_bindgen(getter, js_name = translationX)]
        pub fn translation_x(&self) -> f64 {
            self.state.transform.translation.x as f64
        }
        #[wasm_bindgen(getter, js_name = translationY)]
        pub fn translation_y(&self) -> f64 {
            self.state.transform.translation.y as f64
        }
    }

    impl WasmDeclaredAnimationHandle {
        fn declaration_in(
            &self,
            store: &std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        ) -> Result<&noon::DeclaredAnimation, JsValue> {
            if !std::rc::Rc::ptr_eq(&self.store, store) {
                return Err(js_error(
                    "animation and live execution context belong to different authoring stores",
                ));
            }
            Ok(&self.declaration)
        }
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
            &mut self,
            duration: f64,
            session: u32,
        ) -> Result<crate::SemanticExecutionPlayer, JsValue> {
            self.inner
                .take_execution_player(duration, session)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = beginLiveExecution)]
        pub fn begin_live_execution(&mut self, duration: f64) -> Result<(), JsValue> {
            self.inner
                .live_player(duration)
                .map(|_| ())
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveHandoffDuration)]
        pub fn live_handoff_duration(&self) -> Option<f64> {
            self.inner.live_handoff_duration()
        }

        #[wasm_bindgen(js_name = declareLiveTransformTo)]
        pub fn declare_live_transform_to(
            &mut self,
            source: &crate::WasmAuthoringMobjectHandle,
            target: &crate::WasmAuthoringMobjectHandle,
            run_time: f64,
            rate_function: &str,
        ) -> Result<WasmDeclaredAnimationHandle, JsValue> {
            source.id_in_store(self.inner.scene.store(), "animation declaration")?;
            target.id_in_store(self.inner.scene.store(), "animation declaration")?;
            let rate_function = noon_core::RateFunction::from_semantic_id(rate_function)
                .ok_or_else(|| {
                    js_error(format!(
                        "unsupported animation rate function semantic ID {rate_function:?}"
                    ))
                })?;
            let options = noon_core::AnimationOptions::new()
                .run_time(run_time)
                .rate_func(rate_function);
            let declaration = self
                .inner
                .declare_live_transform_to(
                    source.semantic_mobject(),
                    target.semantic_mobject(),
                    options,
                )
                .map_err(js_error)?;
            Ok(WasmDeclaredAnimationHandle {
                declaration,
                store: std::rc::Rc::clone(self.inner.scene.store()),
            })
        }

        #[wasm_bindgen(js_name = livePlayAnimation)]
        pub fn live_play_animation(
            &mut self,
            animation: &WasmDeclaredAnimationHandle,
        ) -> Result<f64, JsValue> {
            let declaration = animation.declaration_in(self.inner.scene.store())?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_play_animation(declaration)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveWait)]
        pub fn live_wait(&mut self, duration: f64) -> Result<f64, JsValue> {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_wait(duration)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveAdvanceSegmentTo)]
        pub fn live_advance_segment_to(&mut self, time: f64) -> Result<bool, JsValue> {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_advance_segment_to(time)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = prepareExecutionRun)]
        pub fn prepare_execution_run(&mut self) -> Result<(), JsValue> {
            self.inner.prepare_execution_run().map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveSetTranslation)]
        pub fn live_set_translation(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            x: f64,
            y: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_set_translation(handle.semantic_mobject(), x, y)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveAdd)]
        pub fn live_add(
            &mut self,
            object_id: &str,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let id = parse_object_id("object ID", object_id)?;
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .live_add_mobject(id, handle.semantic_mobject())
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveRemove)]
        pub fn live_remove(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .live_remove_mobject(handle.semantic_mobject())
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveReplaceContent)]
        pub fn live_replace_content(
            &mut self,
            target: &crate::WasmAuthoringMobjectHandle,
            source: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            target.id_in_store(self.inner.scene.store(), "live execution context")?;
            source.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .live_replace_content(target.semantic_mobject(), source.semantic_mobject())
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveShift)]
        pub fn live_shift(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            x: f64,
            y: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_shift(handle.semantic_mobject(), x, y)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveSetScale)]
        pub fn live_set_scale(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            x: f64,
            y: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_set_scale(handle.semantic_mobject(), x, y)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveSetRotation)]
        pub fn live_set_rotation(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            angle: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_set_rotation(handle.semantic_mobject(), angle)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveEffectiveMobject)]
        pub fn live_effective_mobject(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<WasmLiveMobjectState, JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            Ok(WasmLiveMobjectState {
                state: self
                    .inner
                    .active_live_player()
                    .map_err(js_error)?
                    .live_effective(handle.semantic_mobject())
                    .map_err(js_error)?,
            })
        }

        #[wasm_bindgen(js_name = returnExecutionPlayer)]
        pub fn return_execution_player(
            &mut self,
            player: crate::SemanticExecutionPlayer,
        ) -> Result<(), JsValue> {
            self.inner.return_execution_player(player).map_err(js_error)
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
    use noon_core::{AnimationOptions, GeometryRef, RateFunction, SemanticVec3, Transform2D};
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

    #[test]
    fn live_membership_uses_the_existing_session_and_registers_detached_handles() {
        let mut context = CanonicalAuthoringScene::default();
        let anchor = context.scene.circle(0.5).unwrap();
        let toggled = context.scene.circle(1.0).unwrap();
        let appended = context.scene.square(1.5).unwrap();
        context.bind_mobject(ObjectId::new(0), &anchor).unwrap();
        context.bind_mobject(ObjectId::new(1), &toggled).unwrap();
        let anchor_slot = context
            .live_player(1.0)
            .unwrap()
            .session_mut_for_test()
            .execution_slot_for_frame_index(0)
            .unwrap();

        context.live_remove_mobject(&toggled).unwrap();
        assert!(context
            .active_live_player()
            .unwrap()
            .live_effective(&toggled)
            .is_err());
        context
            .live_add_mobject(ObjectId::new(1), &toggled)
            .unwrap();
        context
            .live_add_mobject(ObjectId::new(2), &appended)
            .unwrap();
        context
            .active_live_player()
            .unwrap()
            .live_set_translation(&appended, 2.0, -1.0)
            .unwrap();

        assert_eq!(context.node(ObjectId::new(2)).unwrap(), appended.node_id());
        assert_eq!(
            context
                .active_live_player()
                .unwrap()
                .live_effective(&anchor)
                .unwrap()
                .transform
                .translation,
            Vec2::ZERO
        );
        assert_eq!(
            context
                .active_live_player()
                .unwrap()
                .live_effective(&appended)
                .unwrap()
                .transform
                .translation,
            Vec2::new(2.0, -1.0)
        );
        assert_eq!(
            context
                .active_live_player()
                .unwrap()
                .session_mut_for_test()
                .execution_slot_for_frame_index(0),
            Some(anchor_slot)
        );
    }

    #[test]
    fn live_content_switch_refreshes_handoff_resources_and_preserves_execution_identity() {
        let mut context = CanonicalAuthoringScene::default();
        let target = context.scene.circle(0.5).unwrap();
        let replacement = context.scene.text(noon::Text::new("replacement")).unwrap();
        context.bind_mobject(ObjectId::new(0), &target).unwrap();

        let (execution_id, slot) = {
            let player = context.live_player(1.0).unwrap();
            let bundle =
                crate::RetainedResourceBundle::decode_binary(&player.resource_bundle_bytes())
                    .unwrap();
            assert_eq!(bundle.text_count(), 0);
            let session = player.session_mut_for_test();
            (
                session.execution_object_id(target.node_id()).unwrap(),
                session.execution_slot_for_frame_index(0).unwrap(),
            )
        };

        context.live_replace_content(&target, &replacement).unwrap();
        {
            let player = context.active_live_player().unwrap();
            let session = player.session_mut_for_test();
            assert_eq!(
                session.execution_object_id(target.node_id()),
                Some(execution_id)
            );
            assert_eq!(session.execution_slot_for_frame_index(0), Some(slot));
            assert!(session.frame().objects[0].text().is_some());
        }

        let mut handed_off = context.take_execution_player(1.0, 29).unwrap();
        let bundle =
            crate::RetainedResourceBundle::decode_binary(&handed_off.resource_bundle_bytes())
                .unwrap();
        assert_eq!(bundle.text_count(), 1);
        let snapshot: crate::RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&handed_off.initial_delta_json().unwrap()).unwrap();
        assert_eq!(snapshot.objects[0].object, execution_id);
        assert!(matches!(
            snapshot.objects[0].content,
            crate::TransportObjectContent::Text { .. }
        ));
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
    fn native_semantic_text_exports_only_at_the_legacy_callback_boundary() {
        let mut context = CanonicalAuthoringScene::default();
        let mut label = context
            .scene
            .text(
                noon::Text::new("A\nB")
                    .with_font_size(36.0)
                    .with_line_spacing(0.5),
            )
            .unwrap();
        label.shift(2.0, -1.0).unwrap();
        context.bind_mobject(ObjectId::new(4), &label).unwrap();

        let spec = context
            .finalize(Vec::new(), Vec::new(), Vec::new(), None)
            .unwrap();
        let ObjectSpecContent::Text(text) = &spec.objects[0].content else {
            panic!("shared native Text must derive a legacy-boundary text spec");
        };
        assert_eq!(text.source, "A\nB");
        assert_eq!(text.font_size, 36.0);
        let noon_ir::TextSpecOptions::NativePlain { line_spacing, .. } = &text.options else {
            panic!("native Text export requires native options");
        };
        assert!((*line_spacing - 0.5).abs() < 1.0e-6);
        assert_eq!(spec.objects[0].transform.translation, Vec2::new(2.0, -1.0));
        assert_eq!(spec.objects[0].transform.scale, Vec2::ONE);
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

    #[test]
    fn live_runtime_survives_normal_execution_handoff_and_renderer_recovery() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(1.0).unwrap();
        let mut target = circle.target_editor().unwrap();
        target.set_translation(4.0, 0.0).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();
        let options = AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear);
        let animation = context
            .declare_live_transform_to(&circle, &target, options)
            .unwrap();
        let store = std::rc::Rc::clone(context.scene.store());

        {
            let player = context.live_player(2.0).unwrap();
            let end = player.live_play_animation(&animation).unwrap();
            assert_eq!(end, 2.0);
            assert!(player.live_wait(0.5).is_err());
            assert!(!player.live_advance_segment_to(1.0).unwrap());

            player.live_set_translation(&circle, 100.0, 0.0).unwrap();
            assert_eq!(
                store
                    .borrow()
                    .semantic_object_state_checked(circle.node_id())
                    .unwrap()
                    .transform
                    .translation,
                SemanticVec3::new(100.0, 0.0, 0.0)
            );
            assert_eq!(
                player
                    .live_effective(&circle)
                    .unwrap()
                    .transform
                    .translation
                    .x,
                2.0
            );
        }

        context.prepare_execution_run().unwrap();
        let mut handed_off = context.take_execution_player(2.0, 17).unwrap();
        assert_eq!(
            handed_off
                .live_effective(&circle)
                .unwrap()
                .transform
                .translation
                .x,
            2.0
        );
        let snapshot: crate::RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&handed_off.initial_delta_json().unwrap()).unwrap();
        assert_eq!(snapshot.session, 17);
        assert_eq!(snapshot.objects[0].transform.translation.x, 2.0);

        context.return_execution_player(handed_off).unwrap();
        let mut recovered = context.take_execution_player(2.0, 18).unwrap();
        assert_eq!(
            recovered
                .live_effective(&circle)
                .unwrap()
                .transform
                .translation
                .x,
            2.0
        );
        let recovery_snapshot: crate::RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&recovered.initial_delta_json().unwrap()).unwrap();
        assert_eq!(recovery_snapshot.session, 18);
        assert_eq!(recovery_snapshot.objects[0].transform.translation.x, 2.0);
        assert!(context.live_player(2.0).is_err());
    }

    #[test]
    fn live_handoff_duration_keeps_the_completed_segment_after_renderer_seek() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(1.0).unwrap();
        let mut target = circle.target_editor().unwrap();
        target.set_translation(4.0, 0.0).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();
        let animation = context
            .declare_live_transform_to(
                &circle,
                &target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();

        {
            let player = context.live_player(1.0).unwrap();
            assert_eq!(player.live_play_animation(&animation).unwrap(), 2.0);
            assert!(player.live_advance_segment_to(2.0).unwrap());
            assert_eq!(player.live_wait(0.25).unwrap(), 2.25);
            assert!(player.live_advance_segment_to(2.25).unwrap());
        }
        assert_eq!(context.live_handoff_duration(), Some(2.25));

        let duration = context.live_handoff_duration().unwrap();
        let mut handed_off = context.take_execution_player(duration, 17).unwrap();
        handed_off.seek_delta_json(0.5).unwrap();
        assert_eq!(handed_off.time(), 0.5);
        context.return_execution_player(handed_off).unwrap();

        // Presentation may scrub back, but the completed logical continuation
        // remains the authoritative handoff boundary for the next attachment.
        assert_eq!(context.live_handoff_duration(), Some(2.25));
        let duration = context.live_handoff_duration().unwrap();
        let mut recovered = context.take_execution_player(duration, 18).unwrap();
        recovered.seek_delta_json(2.0).unwrap();
        assert_eq!(recovered.time(), 2.0);
        assert_eq!(
            recovered
                .live_effective(&circle)
                .unwrap()
                .transform
                .translation
                .x,
            4.0
        );
    }

    #[test]
    fn new_authoring_run_refreshes_a_returned_runtime_after_direct_scene_edits() {
        let mut context = CanonicalAuthoringScene::default();
        let mut circle = context.scene.circle(1.0).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();

        let initial = context.take_execution_player(1.0, 17).unwrap();
        context.return_execution_player(initial).unwrap();

        // A direct authoring operation happens outside the returned runtime.
        // The next registration boundary lowers precisely one fresh runtime.
        circle.shift(3.0, -1.0).unwrap();
        context.prepare_execution_run().unwrap();
        assert!(context.live_player.is_none());

        let mut rerun = context.take_execution_player(1.0, 18).unwrap();
        let effective = rerun.live_effective(&circle).unwrap();
        assert_eq!(effective.transform.translation, Vec2::new(3.0, -1.0));
        let snapshot: crate::RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&rerun.initial_delta_json().unwrap()).unwrap();
        assert_eq!(snapshot.session, 18);
        assert_eq!(snapshot.objects[0].transform.translation.x, 3.0);
    }
}
