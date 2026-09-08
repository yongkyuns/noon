use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use noon::{DeclaredAnimation, Mobject, MobjectFamily, MobjectFamilyMember, Scene};
use noon_core::{
    AnimationOptions, Color, FamilyAnimationRequest, RateFunction,
    SemanticAnimationCompositionKind, SemanticAnimationIntent, SemanticFamilyAnimationMember,
    SemanticMutationTransaction, SemanticNodeId, SemanticObjectRole, SemanticObjectTrackProperty,
    SemanticObjectTrackValues, SemanticTransactionNodeRef, SemanticVec3, Style, Transform2D,
};
use noon_ir::{ObjectSpec, ObjectSpecContent, TextSpecKind, TextSpecOptions};
use noon_ir::{SceneSpec, SceneSpecError};

use crate::{ClockError, PlaybackClock, SemanticExecutionPlayer};

/// Shared semantic execution retained behind the canonical browser/WASM codec surface.
///
/// The source codec maps ordinary objects, exact tracks, and family requests into one
/// semantic scene before lowering. The worker observes only the normal clocked shared
/// execution delta and resource APIs.
struct OrdinarySemanticExecution {
    _scene: Scene,
    player: SemanticExecutionPlayer,
}

impl std::fmt::Debug for OrdinarySemanticExecution {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OrdinarySemanticExecution")
            .finish_non_exhaustive()
    }
}

fn import_ordinary_semantic_scene(
    objects: Vec<ObjectSpec>,
    tracks: Vec<noon_core::TrackDefinition>,
    family_requests: Vec<FamilyAnimationRequest>,
    camera_object: Option<noon_core::ObjectId>,
) -> Result<(Scene, Option<DeclaredAnimation>, f64), CanonicalRetainedEnginePlayerError> {
    validate_family_topologies(&objects, &family_requests)?;
    let origin = tracks
        .iter()
        .map(|track| track.timing.start_time)
        .chain(
            family_requests
                .iter()
                .map(|request| request.spec().start_time),
        )
        .fold(0.0_f64, f64::min);
    let mut scene = Scene::new();
    let mut external_objects = HashMap::with_capacity(objects.len());
    let mut ordered = Vec::with_capacity(objects.len());
    for object in objects {
        let external_id = object.id;
        let mobject = import_semantic_object(&scene, object, camera_object == Some(external_id))?;
        external_objects.insert(external_id, mobject.clone());
        ordered.push(mobject);
    }
    let members = ordered
        .iter()
        .map(MobjectFamilyMember::Mobject)
        .collect::<Vec<_>>();
    if !members.is_empty() {
        scene.add_many(&members).map_err(|error| {
            CanonicalRetainedEnginePlayerError::SemanticPlayer(error.to_string())
        })?;
    }
    let mut families = HashMap::new();
    for request in &family_requests {
        let external_family = request.target();
        let std::collections::hash_map::Entry::Vacant(entry) = families.entry(external_family)
        else {
            continue;
        };
        let members = request
            .bindings()
            .iter()
            .map(|binding| {
                external_objects.get(&binding.object).ok_or_else(|| {
                    CanonicalRetainedEnginePlayerError::SemanticPlayer(format!(
                        "family target {} references unknown object {}",
                        external_family.slot(),
                        binding.object.get()
                    ))
                })
            })
            .map(|member| member.map(noon::MobjectFamilyMember::Mobject))
            .collect::<Result<Vec<_>, _>>()?;
        let family = scene
            .family(&members)
            .map_err(CanonicalRetainedEnginePlayerError::SemanticPlayer)?;
        entry.insert(family);
    }
    let animation_root = import_semantic_animations(
        &scene,
        &external_objects,
        &families,
        tracks,
        family_requests,
        origin,
    )?;
    Ok((scene, animation_root, origin))
}

fn validate_family_topologies(
    objects: &[ObjectSpec],
    requests: &[FamilyAnimationRequest],
) -> Result<(), CanonicalRetainedEnginePlayerError> {
    let object_ids = objects
        .iter()
        .map(|object| object.id)
        .collect::<HashSet<_>>();
    let family_ids = requests
        .iter()
        .map(FamilyAnimationRequest::target)
        .collect::<HashSet<_>>();
    let mut topologies = HashMap::new();
    let mut objects_by_leaf = HashMap::new();
    let mut leaves_by_object = HashMap::new();
    for request in requests {
        request.validate().map_err(|error| {
            CanonicalRetainedEnginePlayerError::SemanticPlayer(error.to_string())
        })?;
        if request.bindings().is_empty() {
            return Err(CanonicalRetainedEnginePlayerError::SemanticPlayer(format!(
                "family target {} has no bound leaves",
                request.target().slot()
            )));
        }
        let topology = request
            .bindings()
            .iter()
            .map(|binding| (binding.semantic_leaf, binding.object))
            .collect::<Vec<_>>();
        for (leaf, object) in &topology {
            if family_ids.contains(leaf) {
                return Err(CanonicalRetainedEnginePlayerError::SemanticPlayer(format!(
                    "external semantic key {} is both a family and a leaf",
                    leaf.slot()
                )));
            }
            if !object_ids.contains(object) {
                return Err(CanonicalRetainedEnginePlayerError::SemanticPlayer(format!(
                    "family target {} references unknown object {}",
                    request.target().slot(),
                    object.get()
                )));
            }
            if let Some(existing) = objects_by_leaf.insert(*leaf, *object) {
                if existing != *object {
                    return Err(CanonicalRetainedEnginePlayerError::SemanticPlayer(format!(
                        "external leaf {} maps to inconsistent objects",
                        leaf.slot()
                    )));
                }
            }
            if let Some(existing) = leaves_by_object.insert(*object, *leaf) {
                if existing != *leaf {
                    return Err(CanonicalRetainedEnginePlayerError::SemanticPlayer(format!(
                        "object {} maps to inconsistent external leaves",
                        object.get()
                    )));
                }
            }
        }
        match topologies.entry(request.target()) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(topology);
            }
            std::collections::hash_map::Entry::Occupied(entry) if entry.get() != &topology => {
                return Err(CanonicalRetainedEnginePlayerError::SemanticPlayer(format!(
                    "family target {} has inconsistent ordered topology",
                    request.target().slot()
                )));
            }
            std::collections::hash_map::Entry::Occupied(_) => {}
        }
    }
    Ok(())
}

fn import_semantic_object(
    scene: &Scene,
    object: ObjectSpec,
    camera: bool,
) -> Result<Mobject, CanonicalRetainedEnginePlayerError> {
    let ObjectSpec {
        id,
        content,
        transform,
        style,
    } = object;
    if camera && !matches!(&content, ObjectSpecContent::Geometry(_)) {
        return Err(CanonicalRetainedEnginePlayerError::SemanticPlayer(format!(
            "camera object {} must contain geometry",
            id.get()
        )));
    }
    let mobject = match content {
        ObjectSpecContent::Geometry(geometry) => {
            let mut state = noon::semantic_object_state_from_compact(
                &mut scene.store().borrow_mut(),
                geometry,
                transform,
                style,
            )
            .map_err(CanonicalRetainedEnginePlayerError::SemanticPlayer)?;
            if camera {
                state.set_role(SemanticObjectRole::Camera2D);
            }
            Mobject::new(Rc::clone(scene.store()), state)
                .map_err(CanonicalRetainedEnginePlayerError::SemanticPlayer)?
        }
        ObjectSpecContent::Text(text) => {
            let color = canonical_text_color(id, transform, style)?;
            match text.kind {
                TextSpecKind::Plain => {
                    let (font_family, line_spacing) = match text.options {
                        TextSpecOptions::Default => {
                            (noon::DEFAULT_NATIVE_TEXT_FONT_FAMILY.to_owned(), -1.0)
                        }
                        TextSpecOptions::NativePlain {
                            font_family,
                            line_spacing,
                        } => (font_family, line_spacing),
                    };
                    scene
                        .text(
                            noon::Text::new(text.source)
                                .with_font(font_family)
                                .with_font_size(text.font_size)
                                .with_line_spacing(line_spacing)
                                .color(color)
                                .set_opacity(style.opacity)
                                .move_to(transform.translation)
                                .scale_xy(transform.scale)
                                .rotate(transform.rotation),
                        )
                        .map_err(|error| {
                            CanonicalRetainedEnginePlayerError::SemanticPlayer(error.to_string())
                        })?
                }
                TextSpecKind::Typst | TextSpecKind::MathTypst => {
                    if !matches!(text.options, TextSpecOptions::Default) {
                        return Err(CanonicalRetainedEnginePlayerError::SemanticPlayer(
                            "Typst objects require default source options".into(),
                        ));
                    }
                    if text.kind == TextSpecKind::MathTypst {
                        scene
                            .math_typst(
                                noon::MathTypst::new(text.source)
                                    .with_font_size(text.font_size)
                                    .color(color)
                                    .set_opacity(style.opacity)
                                    .move_to(transform.translation)
                                    .scale_xy(transform.scale)
                                    .rotate(transform.rotation),
                            )
                            .map_err(|error| {
                                CanonicalRetainedEnginePlayerError::SemanticPlayer(
                                    error.to_string(),
                                )
                            })?
                    } else {
                        scene
                            .typst(
                                noon::Typst::new(text.source)
                                    .with_font_size(text.font_size)
                                    .color(color)
                                    .set_opacity(style.opacity)
                                    .move_to(transform.translation)
                                    .scale_xy(transform.scale)
                                    .rotate(transform.rotation),
                            )
                            .map_err(|error| {
                                CanonicalRetainedEnginePlayerError::SemanticPlayer(
                                    error.to_string(),
                                )
                            })?
                    }
                }
                TextSpecKind::Markup | TextSpecKind::Tex | TextSpecKind::MathTex => {
                    return Err(CanonicalRetainedEnginePlayerError::SemanticPlayer(format!(
                        "text object {} uses unsupported source kind {:?}",
                        id.get(),
                        text.kind
                    )));
                }
            }
        }
    };
    Ok(mobject)
}

fn canonical_text_color(
    id: noon_core::ObjectId,
    transform: Transform2D,
    style: Style,
) -> Result<Color, CanonicalRetainedEnginePlayerError> {
    let Some(color) = style.fill else {
        return Err(invalid_semantic_import(format!(
            "text object {} has no fill color",
            id.get()
        )));
    };
    if style.stroke.is_some() {
        return Err(invalid_semantic_import(format!(
            "text object {} requests text stroke before canonical stroke lowering is available",
            id.get()
        )));
    }
    if !style.opacity.is_finite() || !(0.0..=1.0).contains(&style.opacity) {
        return Err(invalid_semantic_import(format!(
            "text object {} has invalid opacity {}",
            id.get(),
            style.opacity
        )));
    }
    let values = [
        transform.translation.x,
        transform.translation.y,
        transform.scale.x,
        transform.scale.y,
        transform.rotation,
        color.red,
        color.green,
        color.blue,
        color.alpha,
    ];
    if values.iter().any(|value| !value.is_finite()) {
        return Err(invalid_semantic_import(format!(
            "text object {} has non-finite transform/color state",
            id.get()
        )));
    }
    Ok(color)
}

fn invalid_semantic_import(error: impl std::fmt::Display) -> CanonicalRetainedEnginePlayerError {
    CanonicalRetainedEnginePlayerError::SemanticPlayer(format!(
        "invalid canonical scene input: {error}"
    ))
}

fn import_semantic_animations(
    scene: &Scene,
    objects: &HashMap<noon_core::ObjectId, Mobject>,
    families: &HashMap<SemanticNodeId, MobjectFamily>,
    tracks: Vec<noon_core::TrackDefinition>,
    family_requests: Vec<FamilyAnimationRequest>,
    origin: f64,
) -> Result<Option<DeclaredAnimation>, CanonicalRetainedEnginePlayerError> {
    if tracks.is_empty() && family_requests.is_empty() {
        return Ok(None);
    }
    let mut transaction = SemanticMutationTransaction::new();
    let mut children = Vec::with_capacity(tracks.len() + family_requests.len());
    for track in tracks {
        let target = objects.get(&track.object).ok_or_else(|| {
            CanonicalRetainedEnginePlayerError::SemanticPlayer(format!(
                "track targets unknown object {}",
                track.object.get()
            ))
        })?;
        let property = semantic_track_property(track.property);
        let values = semantic_track_values(scene, track.values)?;
        children.push(transaction.create_object_property_track(
            target.node_id(),
            property,
            values,
            track.timing,
            track.time_map,
        ));
    }
    for request in family_requests {
        let family = families.get(&request.target()).ok_or_else(|| {
            CanonicalRetainedEnginePlayerError::SemanticPlayer(format!(
                "family target {} was not imported",
                request.target().slot()
            ))
        })?;
        let spec = request.spec();
        let options = AnimationOptions::new()
            .run_time(spec.duration)
            .rate_func(spec.rate_function)
            .lag_ratio(spec.lag_ratio)
            .reverse_rate_function(spec.reverse_rate_function)
            .introducer(false)
            .remover(false);
        let members = request
            .bindings()
            .iter()
            .enumerate()
            .map(|(leaf_index, binding)| {
                let target = objects
                    .get(&binding.object)
                    .expect("validated family object is imported");
                transaction.create_family_animation_member(
                    target.node_id(),
                    spec.mode,
                    spec.reverse_member_order,
                    SemanticFamilyAnimationMember {
                        family: family.node_id(),
                        leaf_index,
                    },
                    options,
                )
            })
            .collect::<Vec<_>>();
        let request_root = transaction.create_animation_composition(
            SemanticAnimationCompositionKind::Parallel,
            members,
            AnimationOptions::new().rate_func(RateFunction::Linear),
        );
        let delay = spec.start_time - origin;
        if delay > 0.0 {
            let wait = transaction.create_wait_animation(delay);
            children.push(transaction.create_animation_composition(
                SemanticAnimationCompositionKind::Sequence,
                [wait, request_root],
                AnimationOptions::new().rate_func(RateFunction::Linear),
            ));
        } else {
            children.push(request_root);
        }
    }
    let result = transaction
        .apply(&mut scene.store().borrow_mut())
        .map_err(|error| CanonicalRetainedEnginePlayerError::SemanticPlayer(error.to_string()))?;
    let children = children
        .into_iter()
        .map(|child| {
            result
                .resolve(child)
                .expect("committed property track resolves")
        })
        .collect();
    scene
        .declare_animation(
            SemanticAnimationIntent::Composition {
                kind: SemanticAnimationCompositionKind::Parallel,
                children,
            },
            AnimationOptions::new(),
        )
        .map(Some)
        .map_err(CanonicalRetainedEnginePlayerError::SemanticPlayer)
}

fn semantic_track_property(property: noon_core::Property) -> SemanticObjectTrackProperty {
    match property {
        noon_core::Property::Presence => SemanticObjectTrackProperty::Presence,
        noon_core::Property::Transform => SemanticObjectTrackProperty::Transform,
        noon_core::Property::Position => SemanticObjectTrackProperty::Position,
        noon_core::Property::Rotation => SemanticObjectTrackProperty::Rotation,
        noon_core::Property::Scale => SemanticObjectTrackProperty::Scale,
        noon_core::Property::Fill => SemanticObjectTrackProperty::Fill,
        noon_core::Property::Stroke => SemanticObjectTrackProperty::Stroke,
        noon_core::Property::StrokeWidth => SemanticObjectTrackProperty::StrokeWidth,
        noon_core::Property::Opacity => SemanticObjectTrackProperty::Opacity,
        noon_core::Property::Appearance => SemanticObjectTrackProperty::Appearance,
        noon_core::Property::Reveal => SemanticObjectTrackProperty::Reveal,
        noon_core::Property::Morph => SemanticObjectTrackProperty::Morph,
    }
}

fn semantic_track_values(
    scene: &Scene,
    values: noon_core::TrackValues,
) -> Result<SemanticObjectTrackValues<SemanticTransactionNodeRef>, CanonicalRetainedEnginePlayerError>
{
    Ok(match values {
        noon_core::TrackValues::Bool { from, to } => SemanticObjectTrackValues::Bool { from, to },
        noon_core::TrackValues::Scalar { from, to } => SemanticObjectTrackValues::Scalar {
            from: f64::from(from),
            to: f64::from(to),
        },
        noon_core::TrackValues::Vec2 { from, to } => SemanticObjectTrackValues::Vec3 {
            from: SemanticVec3::from_vec2(from),
            to: SemanticVec3::from_vec2(to),
        },
        noon_core::TrackValues::Color { from, to } => SemanticObjectTrackValues::Color { from, to },
        noon_core::TrackValues::Object { from, to } => {
            let from_state = noon::semantic_object_state_from_compact(
                &mut scene.store().borrow_mut(),
                from.geometry,
                from.transform,
                from.style,
            )
            .map_err(CanonicalRetainedEnginePlayerError::SemanticPlayer)?;
            let from = Mobject::new(Rc::clone(scene.store()), from_state)
                .map_err(CanonicalRetainedEnginePlayerError::SemanticPlayer)?;
            let to_state = noon::semantic_object_state_from_compact(
                &mut scene.store().borrow_mut(),
                to.geometry,
                to.transform,
                to.style,
            )
            .map_err(CanonicalRetainedEnginePlayerError::SemanticPlayer)?;
            let to = Mobject::new(Rc::clone(scene.store()), to_state)
                .map_err(CanonicalRetainedEnginePlayerError::SemanticPlayer)?;
            SemanticObjectTrackValues::Object {
                from: from.node_id().into(),
                to: to.node_id().into(),
            }
        }
        noon_core::TrackValues::PreparedMorph { .. } => {
            return Err(CanonicalRetainedEnginePlayerError::SemanticPlayer(
                "prepared morph execution payloads are not valid semantic authoring input".into(),
            ));
        }
    })
}

/// Clocked shared semantic execution constructed at the canonical external codec boundary.
///
/// Ordinary objects, exact tracks, and family requests are imported into one typed scene.
/// The original encoded source is retained only for explicit roundtrip inspection.
#[derive(Debug)]
pub struct CanonicalRetainedEnginePlayer {
    player: OrdinarySemanticExecution,
    clock: PlaybackClock,
    scene_spec_json: String,
}

impl CanonicalRetainedEnginePlayer {
    pub fn new(
        scene_spec: SceneSpec,
        loop_duration_seconds: f64,
        session: u32,
    ) -> Result<Self, CanonicalRetainedEnginePlayerError> {
        let scene_spec_json = scene_spec.to_json()?;
        scene_spec.validate()?;
        let (scene, animation_root, origin) = import_ordinary_semantic_scene(
            scene_spec.objects,
            scene_spec.tracks,
            scene_spec.family_animations,
            scene_spec.camera_object,
        )?;
        let execution = match animation_root {
            Some(root) => scene
                .execution_session_with_animation_root_at(&root, origin)
                .map_err(CanonicalRetainedEnginePlayerError::SemanticPlayer)?,
            None => scene.execution_session().map_err(|error| {
                CanonicalRetainedEnginePlayerError::SemanticPlayer(error.to_string())
            })?,
        };
        let semantic_player =
            SemanticExecutionPlayer::from_session(execution, loop_duration_seconds, session)
                .map_err(CanonicalRetainedEnginePlayerError::SemanticPlayer)?;
        let player = OrdinarySemanticExecution {
            _scene: scene,
            player: semantic_player,
        };

        Ok(Self {
            player,
            clock: PlaybackClock::looping(loop_duration_seconds)?,
            scene_spec_json,
        })
    }

    pub fn from_json(
        scene_spec_json: &str,
        loop_duration_seconds: f64,
        session: u32,
    ) -> Result<Self, CanonicalRetainedEnginePlayerError> {
        Self::new(
            SceneSpec::from_json(scene_spec_json)?,
            loop_duration_seconds,
            session,
        )
    }

    pub fn scene_spec_json(&self) -> &str {
        &self.scene_spec_json
    }

    pub fn resource_bundle_bytes(&self) -> &[u8] {
        self.player.player.resource_bundle_slice()
    }

    pub fn initial_delta_json(&mut self) -> Result<String, CanonicalRetainedEnginePlayerError> {
        self.player
            .player
            .evaluate_delta_at(0.0)
            .map_err(CanonicalRetainedEnginePlayerError::SemanticPlayer)?
            .ok_or(CanonicalRetainedEnginePlayerError::MissingInitialSnapshot)
    }

    pub fn tick_delta_json(
        &mut self,
        timestamp_ms: f64,
    ) -> Result<Option<String>, CanonicalRetainedEnginePlayerError> {
        let scene_time = self.clock.scene_time(timestamp_ms)?;
        self.player
            .player
            .evaluate_delta_at(scene_time)
            .map_err(CanonicalRetainedEnginePlayerError::SemanticPlayer)
    }

    pub fn set_loop_duration(
        &mut self,
        duration: f64,
    ) -> Result<(), CanonicalRetainedEnginePlayerError> {
        self.clock.set_loop_duration(duration)?;
        Ok(())
    }

    pub fn pause(&mut self) {
        self.clock.pause();
    }

    pub fn resume(&mut self) {
        self.clock.resume();
    }

    pub fn seek_delta_json(
        &mut self,
        scene_time: f64,
    ) -> Result<Option<String>, CanonicalRetainedEnginePlayerError> {
        let mut clock = self.clock.clone();
        clock.seek(scene_time)?;
        let delta = self
            .player
            .player
            .evaluate_delta_at(scene_time)
            .map_err(CanonicalRetainedEnginePlayerError::SemanticPlayer)?;
        self.clock = clock;
        Ok(delta)
    }

    pub const fn is_playing(&self) -> bool {
        self.clock.is_playing()
    }

    pub fn time(&self) -> f64 {
        self.player.player.time()
    }
}

#[derive(Debug)]
pub enum CanonicalRetainedEnginePlayerError {
    SceneSpec(SceneSpecError),
    SemanticPlayer(String),
    MissingInitialSnapshot,
    Clock(ClockError),
    Json(serde_json::Error),
}

impl std::fmt::Display for CanonicalRetainedEnginePlayerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SceneSpec(error) => error.fmt(formatter),
            Self::SemanticPlayer(error) => formatter.write_str(error),
            Self::MissingInitialSnapshot => formatter
                .write_str("canonical retained execution did not emit its initial snapshot"),
            Self::Clock(error) => error.fmt(formatter),
            Self::Json(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for CanonicalRetainedEnginePlayerError {}

impl From<SceneSpecError> for CanonicalRetainedEnginePlayerError {
    fn from(value: SceneSpecError) -> Self {
        Self::SceneSpec(value)
    }
}

impl From<ClockError> for CanonicalRetainedEnginePlayerError {
    fn from(value: ClockError) -> Self {
        Self::Clock(value)
    }
}

impl From<serde_json::Error> for CanonicalRetainedEnginePlayerError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::CanonicalRetainedEnginePlayer;

    #[wasm_bindgen(js_name = CanonicalRetainedEngineScenePlayer)]
    pub struct WasmCanonicalRetainedEngineScenePlayer {
        inner: CanonicalRetainedEnginePlayer,
    }

    #[wasm_bindgen(js_class = CanonicalRetainedEngineScenePlayer)]
    impl WasmCanonicalRetainedEngineScenePlayer {
        #[wasm_bindgen(constructor)]
        pub fn new(
            scene_spec_json: &str,
            loop_duration_seconds: f64,
            session: u32,
        ) -> Result<Self, JsValue> {
            Ok(Self {
                inner: CanonicalRetainedEnginePlayer::from_json(
                    scene_spec_json,
                    loop_duration_seconds,
                    session,
                )
                .map_err(js_error)?,
            })
        }

        #[wasm_bindgen(js_name = resourceBundleBytes)]
        pub fn resource_bundle_bytes(&self) -> Vec<u8> {
            self.inner.resource_bundle_bytes().to_vec()
        }

        #[wasm_bindgen(js_name = initialDeltaJson)]
        pub fn initial_delta_json(&mut self) -> Result<String, JsValue> {
            self.inner.initial_delta_json().map_err(js_error)
        }

        #[wasm_bindgen(js_name = tickDeltaJson)]
        pub fn tick_delta_json(&mut self, timestamp_ms: f64) -> Result<Option<String>, JsValue> {
            self.inner.tick_delta_json(timestamp_ms).map_err(js_error)
        }

        #[wasm_bindgen(js_name = setLoopDurationSeconds)]
        pub fn set_loop_duration_seconds(&mut self, duration: f64) -> Result<(), JsValue> {
            self.inner.set_loop_duration(duration).map_err(js_error)
        }

        pub fn pause(&mut self) {
            self.inner.pause();
        }

        pub fn resume(&mut self) {
            self.inner.resume();
        }

        #[wasm_bindgen(js_name = seekDeltaJson)]
        pub fn seek_delta_json(&mut self, scene_time: f64) -> Result<Option<String>, JsValue> {
            self.inner.seek_delta_json(scene_time).map_err(js_error)
        }

        #[wasm_bindgen(js_name = isPlaying)]
        pub fn is_playing(&self) -> bool {
            self.inner.is_playing()
        }

        #[wasm_bindgen(js_name = sceneSpecJson)]
        pub fn scene_spec_json(&self) -> String {
            self.inner.scene_spec_json().to_owned()
        }

        pub fn time(&self) -> f64 {
            self.inner.time()
        }
    }

    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use noon_core::{
        CompositionTimeMap, FamilyAnimationLeafBinding, FamilyAnimationMode,
        FamilyAnimationRequest, FamilyAnimationSpec, ObjectId, Property, RateFunction,
        TrackDefinition, TrackId, TrackTiming, TrackValues, Vec2,
    };
    use serde_json::json;
    use std::rc::Rc;

    use crate::{
        canonical_authoring_scene::CanonicalAuthoringScene, InstalledRetainedExecutionMirror,
        RetainedExecutionDeltaEnvelope, RetainedFamilyExecutionDeltaEnvelope,
        RetainedTransportApplyOutcome,
    };

    use super::*;

    fn canonical_scene_json(
        source: &str,
        position: Option<(f64, f32)>,
    ) -> (String, ObjectId, ObjectId) {
        let scene = noon::Scene::new();
        let circle = scene.circle(0.25).unwrap();
        let text = scene.text(noon::Text::new(source)).unwrap();
        let mut context = CanonicalAuthoringScene::with_store(Rc::clone(scene.store()));
        let circle_id = ObjectId::new((1_u64 << 40) + 7);
        let text_id = ObjectId::new(1_u64 << 52);
        context.bind_mobject(circle_id, &circle).unwrap();
        context.bind_mobject(text_id, &text).unwrap();
        let tracks = position
            .map(|(duration, target_x)| vec![position_track(circle_id, duration, target_x)])
            .unwrap_or_default();
        let json = context
            .finalize(tracks, Vec::new(), None)
            .unwrap()
            .to_json()
            .unwrap();
        (json, text_id, circle_id)
    }

    fn position_track(object: ObjectId, duration: f64, target_x: f32) -> TrackDefinition {
        TrackDefinition {
            id: TrackId::new(0),
            object,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(target_x, 0.0),
            },
            timing: TrackTiming::new(0.0, duration, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        }
    }

    fn family_scene_json(
        additional: Option<(f64, f64, FamilyAnimationMode, bool)>,
    ) -> (String, ObjectId, ObjectId) {
        family_scene_json_with_start(1.0, additional)
    }

    fn family_scene_json_with_start(
        start_time: f64,
        additional: Option<(f64, f64, FamilyAnimationMode, bool)>,
    ) -> (String, ObjectId, ObjectId) {
        let scene = noon::Scene::new();
        let circle = scene.circle(0.25).unwrap();
        let text = scene.text(noon::Text::new("AB")).unwrap();
        let family = scene.family(&[(&text).into(), (&circle).into()]).unwrap();
        let mut context = CanonicalAuthoringScene::with_store(Rc::clone(scene.store()));
        let circle_id = ObjectId::new(1);
        let text_id = ObjectId::new(1_u64 << 52);
        context.bind_mobject(circle_id, &circle).unwrap();
        context.bind_mobject(text_id, &text).unwrap();
        let family_spec = FamilyAnimationSpec::new(
            FamilyAnimationMode::Reveal,
            start_time,
            2.0,
            1.0,
            RateFunction::Linear,
            false,
            false,
        )
        .unwrap();
        let binding = || {
            [
                FamilyAnimationLeafBinding::new(circle.node_id(), circle_id),
                FamilyAnimationLeafBinding::new(text.node_id(), text_id),
            ]
        };
        let mut requests = vec![FamilyAnimationRequest::from_semantic_bindings(
            &scene.store().borrow(),
            family.node_id(),
            family_spec,
            binding(),
        )
        .unwrap()];
        if let Some((start_time, duration, mode, inconsistent_topology)) = additional {
            let spec = FamilyAnimationSpec::new(
                mode,
                start_time,
                duration,
                1.0,
                RateFunction::Linear,
                false,
                false,
            )
            .unwrap();
            requests.push(if inconsistent_topology {
                FamilyAnimationRequest::new(family.node_id(), binding().to_vec(), spec).unwrap()
            } else {
                FamilyAnimationRequest::from_semantic_bindings(
                    &scene.store().borrow(),
                    family.node_id(),
                    spec,
                    binding(),
                )
                .unwrap()
            });
        }
        let json = context
            .finalize(vec![position_track(circle_id, 4.0, 4.0)], requests, None)
            .unwrap()
            .to_json()
            .unwrap();
        (json, text_id, circle_id)
    }

    fn family_object_indices(mirror: &InstalledRetainedExecutionMirror) -> (usize, usize) {
        let retained = mirror.frame().unwrap();
        let text_index = retained
            .objects
            .iter()
            .position(|object| object.content.text().is_some())
            .unwrap();
        let circle_index = retained
            .objects
            .iter()
            .position(|object| object.content.geometry().is_some())
            .unwrap();
        (text_index, circle_index)
    }

    fn assert_family_midpoint(mirror: &InstalledRetainedExecutionMirror) {
        let retained = mirror.frame().unwrap();
        let (text_index, circle_index) = family_object_indices(mirror);
        assert_eq!((circle_index, text_index), (0, 1));
        let family_frame = mirror.planned_family_frame().unwrap().unwrap();
        let text_plan_index = family_frame.family_plan_index(text_index).unwrap() as usize;
        let circle_plan_index = family_frame.family_plan_index(circle_index).unwrap() as usize;
        let text_plan = &mirror.family_plans()[text_plan_index];
        let circle_plan = &mirror.family_plans()[circle_plan_index];
        let text_span = text_plan.leaves()[0].span();
        let circle_span = circle_plan.leaves()[0].span();
        assert_eq!(text_span.object, retained.objects[text_index].id);
        assert_eq!((text_span.first_member, text_span.member_count), (0, 2));
        assert_eq!(text_plan.member_plan().total_member_count(), 3);
        assert_eq!(circle_span.object, retained.objects[circle_index].id);
        assert_eq!((circle_span.first_member, circle_span.member_count), (2, 1));
        assert_eq!(circle_plan.member_plan().total_member_count(), 3);

        let text = family_frame
            .planned_family_leaf(mirror.family_plans(), text_index)
            .unwrap()
            .unwrap();
        let circle = family_frame
            .planned_family_leaf(mirror.family_plans(), circle_index)
            .unwrap()
            .unwrap();
        assert_eq!(text.member_progress(0).unwrap(), 1.0);
        assert!((text.member_progress(1).unwrap() - 0.5).abs() < 1e-6);
        assert_eq!(circle.member_progress(0).unwrap(), 0.0);

        let circle = &retained.objects[circle_index];
        assert_eq!(circle.transform.translation, Vec2::new(2.0, 0.0));
    }

    #[test]
    fn canonical_engine_emits_mixed_snapshot_and_resources() {
        let (scene_spec_json, text_id, circle) = canonical_scene_json("Canonical engine", None);

        let mut engine =
            CanonicalRetainedEnginePlayer::from_json(&scene_spec_json, 2.0, 41).unwrap();
        let initial: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&engine.initial_delta_json().unwrap()).unwrap();

        assert!(initial.snapshot);
        assert_eq!(initial.objects.len(), 2);
        assert_ne!(initial.objects[0].object, circle);
        assert_ne!(initial.objects[1].object, text_id);
        assert_ne!(initial.objects[0].object, initial.objects[1].object);
        assert!(matches!(
            initial.objects[0].content,
            crate::TransportObjectContent::Geometry { .. }
        ));
        assert!(matches!(
            initial.objects[1].content,
            crate::TransportObjectContent::Text { .. }
        ));
        assert!(!engine.resource_bundle_bytes().is_empty());
        assert_eq!(engine.scene_spec_json(), scene_spec_json);
    }

    #[test]
    fn canonical_engine_playback_controls_keep_resources_and_session_stable() {
        let (scene_spec_json, _, _) = canonical_scene_json("controls", Some((2.0, 2.0)));
        let mut engine =
            CanonicalRetainedEnginePlayer::from_json(&scene_spec_json, 4.0, 37).unwrap();
        let bundle = engine.resource_bundle_bytes().to_vec();

        let initial: RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&engine.initial_delta_json().unwrap()).unwrap();
        engine.tick_delta_json(100.0).unwrap();
        engine.tick_delta_json(1_100.0).unwrap();
        assert_eq!(engine.time(), 1.0);

        engine.pause();
        assert!(!engine.is_playing());
        assert!(engine.tick_delta_json(5_100.0).unwrap().is_none());
        assert_eq!(engine.time(), 1.0);

        let rewind: RetainedExecutionDeltaEnvelope = serde_json::from_str(
            &engine
                .seek_delta_json(0.25)
                .unwrap()
                .expect("rewind snapshot"),
        )
        .unwrap();
        assert!(rewind.snapshot);
        assert_eq!(rewind.session, initial.session);
        assert_eq!(rewind.time, 0.25);
        assert_eq!(engine.resource_bundle_bytes(), bundle);
        assert!(engine.tick_delta_json(8_100.0).unwrap().is_none());
        assert_eq!(engine.time(), 0.25);

        engine.resume();
        assert!(engine.is_playing());
        assert!(engine.tick_delta_json(8_100.0).unwrap().is_none());
        engine.tick_delta_json(8_600.0).unwrap();
        assert_eq!(engine.time(), 0.75);
    }

    #[test]
    fn canonical_engine_imports_family_into_shared_execution_and_preserves_tracks() {
        let (scene_spec_json, _, _) = family_scene_json(None);
        let mut engine =
            CanonicalRetainedEnginePlayer::from_json(&scene_spec_json, 4.0, 51).unwrap();
        assert_eq!(engine.scene_spec_json(), scene_spec_json);

        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(engine.resource_bundle_bytes())
                .unwrap();
        let initial_json = engine.initial_delta_json().unwrap();
        let initial: RetainedFamilyExecutionDeltaEnvelope =
            serde_json::from_str(&initial_json).unwrap();
        assert!(initial.retained.snapshot);
        assert!(initial.family_plans.is_empty());
        assert!(!initial_json.contains("glyph"));
        let (outcome, changes) = mirror.apply_json(&initial_json).unwrap();
        assert_eq!(outcome, RetainedTransportApplyOutcome::Applied);
        assert!(changes.is_all());

        let midpoint_json = engine
            .seek_delta_json(2.0)
            .unwrap()
            .expect("family midpoint delta");
        let midpoint: RetainedFamilyExecutionDeltaEnvelope =
            serde_json::from_str(&midpoint_json).unwrap();
        assert!(!midpoint.retained.snapshot);
        assert_eq!(midpoint.family_plans.len(), 2);
        mirror.apply_json(&midpoint_json).unwrap();
        assert_family_midpoint(&mirror);
    }

    #[test]
    fn canonical_family_direct_seek_matches_forward_state() {
        let (scene_spec_json, _, _) = family_scene_json(None);

        let mut forward =
            CanonicalRetainedEnginePlayer::from_json(&scene_spec_json, 4.0, 61).unwrap();
        let mut forward_mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(forward.resource_bundle_bytes())
                .unwrap();
        forward_mirror
            .apply_json(&forward.initial_delta_json().unwrap())
            .unwrap();
        forward_mirror
            .apply_json(
                &forward
                    .seek_delta_json(2.0)
                    .unwrap()
                    .expect("forward midpoint delta"),
            )
            .unwrap();

        let mut direct =
            CanonicalRetainedEnginePlayer::from_json(&scene_spec_json, 4.0, 62).unwrap();
        let mut direct_mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(direct.resource_bundle_bytes())
                .unwrap();
        let direct_json = direct
            .seek_delta_json(2.0)
            .unwrap()
            .expect("direct midpoint snapshot");
        let direct_delta: RetainedFamilyExecutionDeltaEnvelope =
            serde_json::from_str(&direct_json).unwrap();
        assert!(direct_delta.retained.snapshot);
        direct_mirror.apply_json(&direct_json).unwrap();

        assert_family_midpoint(&forward_mirror);
        assert_family_midpoint(&direct_mirror);
        let forward_frame = forward_mirror.frame().unwrap();
        let direct_frame = direct_mirror.frame().unwrap();
        assert_eq!(
            crate::determinism::normalized_frame_value(forward_frame),
            crate::determinism::normalized_frame_value(direct_frame),
        );
        for (forward_object, direct_object) in
            forward_frame.objects.iter().zip(&direct_frame.objects)
        {
            assert_eq!(forward_object.text_bounds, direct_object.text_bounds);
            if let (Some(forward_text), Some(direct_text)) =
                (forward_object.text(), direct_object.text())
            {
                assert_ne!(forward_text.arena, direct_text.arena);
                assert_eq!(
                    forward_mirror.resources().texts().get(forward_text),
                    direct_mirror.resources().texts().get(direct_text),
                );
            }
        }
    }

    #[test]
    fn canonical_engine_runs_sequential_family_requests_with_exact_plan_identity() {
        let (scene_spec_json, _, _) = family_scene_json(Some((
            3.0,
            1.0,
            FamilyAnimationMode::DrawBorderThenFill,
            false,
        )));
        let mut engine =
            CanonicalRetainedEnginePlayer::from_json(&scene_spec_json, 5.0, 71).unwrap();
        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(engine.resource_bundle_bytes())
                .unwrap();

        let initial = engine.initial_delta_json().unwrap();
        let initial_delta: RetainedFamilyExecutionDeltaEnvelope =
            serde_json::from_str(&initial).unwrap();
        assert!(initial_delta.family_plans.is_empty());
        mirror.apply_json(&initial).unwrap();
        assert!(mirror.family_plans().is_empty());

        let first = engine
            .seek_delta_json(2.0)
            .unwrap()
            .expect("first family request state");
        mirror.apply_json(&first).unwrap();
        let (text_index, circle_index) = family_object_indices(&mirror);
        let first_frame = mirror.planned_family_frame().unwrap().unwrap();
        let first_text_plan = first_frame.family_plan_index(text_index).unwrap() as usize;
        let first_circle_plan = first_frame.family_plan_index(circle_index).unwrap() as usize;
        assert_ne!(first_text_plan, first_circle_plan);
        let text_target = mirror.family_plans()[first_text_plan]
            .member_plan()
            .target();
        let circle_target = mirror.family_plans()[first_circle_plan]
            .member_plan()
            .target();
        assert_family_midpoint(&mirror);

        let second = engine
            .seek_delta_json(3.5)
            .unwrap()
            .expect("second family request state");
        mirror.apply_json(&second).unwrap();
        let second_frame = mirror.planned_family_frame().unwrap().unwrap();
        let second_text_plan = second_frame.family_plan_index(text_index).unwrap() as usize;
        let second_circle_plan = second_frame.family_plan_index(circle_index).unwrap() as usize;
        assert_ne!(second_text_plan, second_circle_plan);
        assert_ne!(second_text_plan, first_text_plan);
        assert_ne!(second_circle_plan, first_circle_plan);
        assert_eq!(
            mirror.family_plans()[second_text_plan]
                .member_plan()
                .target(),
            text_target
        );
        assert_eq!(
            mirror.family_plans()[second_circle_plan]
                .member_plan()
                .target(),
            circle_target
        );
    }

    #[test]
    fn canonical_engine_rejects_overlapping_family_ownership_on_same_object() {
        let (scene_spec_json, _, _) =
            family_scene_json(Some((2.5, 1.0, FamilyAnimationMode::Reveal, false)));
        let error =
            CanonicalRetainedEnginePlayer::from_json(&scene_spec_json, 4.0, 72).unwrap_err();
        assert!(matches!(
            error,
            CanonicalRetainedEnginePlayerError::SemanticPlayer(message)
                if message.to_ascii_lowercase().contains("conflict")
        ));
    }

    #[test]
    fn canonical_engine_rejects_reused_family_key_with_different_order() {
        let (scene_spec_json, _, _) =
            family_scene_json(Some((3.0, 1.0, FamilyAnimationMode::Reveal, true)));
        let error =
            CanonicalRetainedEnginePlayer::from_json(&scene_spec_json, 4.0, 73).unwrap_err();
        assert!(matches!(
            error,
            CanonicalRetainedEnginePlayerError::SemanticPlayer(message)
                if message.contains("inconsistent ordered topology")
        ));
    }

    #[test]
    fn negative_family_start_is_in_progress_at_time_zero() {
        let (scene_spec_json, _, _) = family_scene_json_with_start(-1.0, None);
        let mut engine =
            CanonicalRetainedEnginePlayer::from_json(&scene_spec_json, 4.0, 74).unwrap();
        let initial = engine.initial_delta_json().unwrap();
        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(engine.resource_bundle_bytes())
                .unwrap();
        mirror.apply_json(&initial).unwrap();
        let family = mirror.family_frame().unwrap().unwrap();
        let state = family
            .family_animation(family_object_indices(&mirror).0)
            .unwrap();
        assert_eq!(state.overall_progress, 0.5);
    }

    #[test]
    fn canonical_engine_rejects_unsupported_scene_spec_version() {
        let invalid = json!({"version": 99, "objects": [], "tracks": []}).to_string();
        let error = CanonicalRetainedEnginePlayer::from_json(&invalid, 2.0, 47).unwrap_err();
        assert!(matches!(
            error,
            CanonicalRetainedEnginePlayerError::SceneSpec(SceneSpecError::UnsupportedVersion(99))
        ));
    }
}
