//! Browser-facing persistent Noon runtime.
//!
//! The core player is ordinary Rust so its state transitions are testable on
//! native CI. A thin wasm-bindgen wrapper exposes the same semantics to JavaScript.

#![forbid(unsafe_code)]

mod clock;

pub use clock::*;

use noon_compile::{CompileError, CompilePatchError, CompiledScene};
use std::collections::{BTreeMap, BTreeSet};

use noon_core::{ObjectId, PatchError, SceneDefinition, ScenePatch};
use noon_ir::{decode_patch_batch, decode_scene, encode_scene, IrError};
use noon_runtime::{EvaluationError, FrameState, SceneInstance};

#[derive(Debug)]
pub enum PlayerError {
    Ir(IrError),
    Compile(CompileError),
    Patch(PatchError),
    CompilePatch(CompilePatchError),
    Evaluation(EvaluationError),
    Sequence { expected: u64, actual: u64 },
    SequenceExhausted,
}

impl std::fmt::Display for PlayerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ir(error) => write!(formatter, "{error}"),
            Self::Compile(error) => write!(formatter, "scene compilation failed: {error}"),
            Self::Patch(error) => write!(formatter, "scene patch failed: {error}"),
            Self::CompilePatch(error) => write!(formatter, "runtime patch failed: {error}"),
            Self::Evaluation(error) => write!(formatter, "scene evaluation failed: {error}"),
            Self::Sequence { expected, actual } => {
                write!(
                    formatter,
                    "expected patch sequence {expected}, got {actual}"
                )
            }
            Self::SequenceExhausted => formatter.write_str("patch sequence space exhausted"),
        }
    }
}

impl std::error::Error for PlayerError {}

impl From<IrError> for PlayerError {
    fn from(value: IrError) -> Self {
        Self::Ir(value)
    }
}

impl From<CompileError> for PlayerError {
    fn from(value: CompileError) -> Self {
        Self::Compile(value)
    }
}

impl From<PatchError> for PlayerError {
    fn from(value: PatchError) -> Self {
        Self::Patch(value)
    }
}

impl From<CompilePatchError> for PlayerError {
    fn from(value: CompilePatchError) -> Self {
        Self::CompilePatch(value)
    }
}

impl From<EvaluationError> for PlayerError {
    fn from(value: EvaluationError) -> Self {
        Self::Evaluation(value)
    }
}

#[derive(Clone, Debug)]
pub struct ScenePlayer {
    definition: SceneDefinition,
    instance: SceneInstance,
    next_sequence: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReconcileOutcome {
    Incremental { patch_count: usize },
    Replaced,
}

impl ScenePlayer {
    pub fn from_scene_json(json: &str) -> Result<Self, PlayerError> {
        let definition = decode_scene(json)?;
        let compiled = CompiledScene::compile(&definition)?;
        Ok(Self {
            definition,
            instance: SceneInstance::new(compiled),
            next_sequence: 0,
        })
    }

    pub fn seek(&mut self, time: f64) -> Result<&FrameState, PlayerError> {
        Ok(self.instance.seek(time)?)
    }

    pub fn apply_patch_batch_json(&mut self, json: &str) -> Result<&FrameState, PlayerError> {
        let batch = decode_patch_batch(json)?;
        if batch.sequence != self.next_sequence {
            return Err(PlayerError::Sequence {
                expected: self.next_sequence,
                actual: batch.sequence,
            });
        }

        let next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(PlayerError::SequenceExhausted)?;
        self.apply_patches_transactionally(&batch.patches)?;
        self.next_sequence = next_sequence;
        Ok(self.instance.frame())
    }

    pub fn replace_scene_json(&mut self, json: &str) -> Result<&FrameState, PlayerError> {
        let definition = decode_scene(json)?;
        let compiled = CompiledScene::compile(&definition)?;
        let playhead = self.instance.frame().time;
        let mut instance = SceneInstance::new(compiled);
        instance.seek(playhead)?;

        self.definition = definition;
        self.instance = instance;
        self.next_sequence = 0;
        Ok(self.instance.frame())
    }

    pub fn reconcile_scene_json(&mut self, json: &str) -> Result<ReconcileOutcome, PlayerError> {
        let desired = decode_scene(json)?;
        let Some(patches) = scene_diff(&self.definition, &desired) else {
            self.replace_scene_json(json)?;
            return Ok(ReconcileOutcome::Replaced);
        };
        let patch_count = patches.len();
        self.apply_patches_transactionally(&patches)?;
        self.next_sequence = 0;
        Ok(ReconcileOutcome::Incremental { patch_count })
    }

    fn apply_patches_transactionally(&mut self, patches: &[ScenePatch]) -> Result<(), PlayerError> {
        if patches.iter().all(is_value_patch) {
            for patch in patches {
                let object = value_patch_object(patch);
                if self.definition.object(object).is_none() {
                    return Err(PlayerError::Patch(PatchError::UnknownObject(object)));
                }
                if !self.instance.contains_object(object) {
                    return Err(PlayerError::CompilePatch(CompilePatchError::UnknownObject(
                        object,
                    )));
                }
            }
            for patch in patches {
                self.definition
                    .apply_patch(patch.clone())
                    .expect("value patch was preflighted against the scene definition");
                self.instance
                    .apply_patch(patch)
                    .expect("value patch was preflighted against the compiled scene");
            }
            return Ok(());
        }

        let mut definition = self.definition.clone();
        let mut instance = self.instance.clone();
        for patch in patches {
            definition.apply_patch(patch.clone())?;
            instance.apply_patch(patch)?;
        }
        self.definition = definition;
        self.instance = instance;
        Ok(())
    }

    pub fn scene_json(&self) -> Result<String, PlayerError> {
        Ok(encode_scene(&self.definition)?)
    }

    pub fn frame(&self) -> &FrameState {
        self.instance.frame()
    }

    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    pub fn object_count(&self) -> usize {
        self.instance.frame().objects.len()
    }
}

fn is_value_patch(patch: &ScenePatch) -> bool {
    matches!(
        patch,
        ScenePatch::SetTransform { .. } | ScenePatch::SetStyle { .. }
    )
}

fn value_patch_object(patch: &ScenePatch) -> ObjectId {
    match patch {
        ScenePatch::SetTransform { object, .. } | ScenePatch::SetStyle { object, .. } => *object,
        _ => unreachable!("value patch helper only accepts transform or style patches"),
    }
}

fn scene_diff(current: &SceneDefinition, desired: &SceneDefinition) -> Option<Vec<ScenePatch>> {
    let current_objects = current
        .objects()
        .iter()
        .map(|object| (object.id, object))
        .collect::<BTreeMap<_, _>>();
    let desired_objects = desired
        .objects()
        .iter()
        .map(|object| (object.id, object))
        .collect::<BTreeMap<_, _>>();
    if !append_compatible(
        current.objects().iter().map(|object| object.id),
        desired.objects().iter().map(|object| object.id),
    ) {
        return None;
    }
    for object in desired.objects() {
        if let Some(existing) = current_objects.get(&object.id) {
            if existing.geometry != object.geometry {
                return None;
            }
        }
    }

    let current_tracks = current
        .tracks()
        .iter()
        .map(|track| (track.id, track))
        .collect::<BTreeMap<_, _>>();
    let desired_tracks = desired
        .tracks()
        .iter()
        .map(|track| (track.id, track))
        .collect::<BTreeMap<_, _>>();
    if !append_compatible(
        current.tracks().iter().map(|track| track.id),
        desired.tracks().iter().map(|track| track.id),
    ) {
        return None;
    }

    let removed_objects = current_objects
        .keys()
        .filter(|id| !desired_objects.contains_key(id))
        .copied()
        .collect::<BTreeSet<ObjectId>>();
    let mut patches = Vec::new();
    for (id, track) in &current_tracks {
        if !desired_tracks.contains_key(id) && !removed_objects.contains(&track.object) {
            patches.push(ScenePatch::RemoveTrack(*id));
        }
    }
    for id in &removed_objects {
        patches.push(ScenePatch::RemoveObject(*id));
    }
    for object in desired.objects() {
        let id = object.id;
        match current_objects.get(&id) {
            Some(existing) => {
                if existing.transform != object.transform {
                    patches.push(ScenePatch::SetTransform {
                        object: id,
                        transform: object.transform,
                    });
                }
                if existing.style != object.style {
                    patches.push(ScenePatch::SetStyle {
                        object: id,
                        style: object.style,
                    });
                }
            }
            None => patches.push(ScenePatch::CreateObject(object.clone())),
        }
    }
    for track in desired.tracks() {
        match current_tracks.get(&track.id) {
            Some(existing) if **existing != *track => {
                patches.push(ScenePatch::ReplaceTrack(track.clone()));
            }
            None => patches.push(ScenePatch::AddTrack(track.clone())),
            _ => {}
        }
    }
    Some(patches)
}

fn append_compatible<Id: Copy + Ord>(
    current: impl Iterator<Item = Id>,
    desired: impl Iterator<Item = Id>,
) -> bool {
    let current = current.collect::<Vec<_>>();
    let desired = desired.collect::<Vec<_>>();
    let current_set = current.iter().copied().collect::<BTreeSet<_>>();
    let desired_set = desired.iter().copied().collect::<BTreeSet<_>>();
    let retained = current
        .into_iter()
        .filter(|id| desired_set.contains(id))
        .collect::<Vec<_>>();
    let desired_existing = desired
        .iter()
        .copied()
        .filter(|id| current_set.contains(id))
        .collect::<Vec<_>>();
    retained == desired_existing && desired.iter().take(retained.len()).copied().eq(retained)
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use noon_core::{
        Color, Easing, GeometryRef, Property, SceneDefinition, Style, TrackTiming, Transform2D,
        Vec2,
    };
    use noon_ir::encode_scene;
    use noon_render_wgpu::{Camera2D, FramePreparer, GpuRenderer};
    use wasm_bindgen::prelude::*;
    use web_sys::HtmlCanvasElement;

    use super::{PlaybackClock, ReconcileOutcome, ScenePlayer};

    #[wasm_bindgen(js_name = ScenePlayer)]
    pub struct WasmScenePlayer {
        inner: ScenePlayer,
    }

    #[wasm_bindgen(js_class = ScenePlayer)]
    impl WasmScenePlayer {
        #[wasm_bindgen(constructor)]
        pub fn new(scene_json: &str) -> Result<WasmScenePlayer, JsValue> {
            Ok(Self {
                inner: ScenePlayer::from_scene_json(scene_json).map_err(js_error)?,
            })
        }

        pub fn seek(&mut self, time: f64) -> Result<(), JsValue> {
            self.inner.seek(time).map_err(js_error)?;
            Ok(())
        }

        pub fn apply_patch_batch(&mut self, json: &str) -> Result<(), JsValue> {
            self.inner.apply_patch_batch_json(json).map_err(js_error)?;
            Ok(())
        }

        #[wasm_bindgen(js_name = replaceScene)]
        pub fn replace_scene(&mut self, json: &str) -> Result<(), JsValue> {
            self.inner.replace_scene_json(json).map_err(js_error)?;
            Ok(())
        }

        #[wasm_bindgen(js_name = reconcileScene)]
        pub fn reconcile_scene(&mut self, json: &str) -> Result<bool, JsValue> {
            Ok(matches!(
                self.inner.reconcile_scene_json(json).map_err(js_error)?,
                ReconcileOutcome::Incremental { .. }
            ))
        }

        pub fn time(&self) -> f64 {
            self.inner.frame().time
        }

        pub fn object_count(&self) -> usize {
            self.inner.object_count()
        }

        pub fn next_sequence(&self) -> u64 {
            self.inner.next_sequence()
        }

        pub fn scene_json(&self) -> Result<String, JsValue> {
            self.inner.scene_json().map_err(js_error)
        }
    }

    /// Persistent browser player that connects the deterministic runtime to a WebGPU canvas.
    #[wasm_bindgen(js_name = NoonCanvasPlayer)]
    pub struct WasmCanvasPlayer {
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        canvas: HtmlCanvasElement,
        config: wgpu::SurfaceConfiguration,
        drawable: bool,
        player: ScenePlayer,
        clock: PlaybackClock,
        preparer: FramePreparer,
        renderer: GpuRenderer,
        camera_center: Vec2,
        camera_height: f32,
        clear_color: wgpu::Color,
        last_draw_calls: usize,
        last_instances_drawn: usize,
        last_bytes_uploaded: usize,
    }

    #[wasm_bindgen(js_class = NoonCanvasPlayer)]
    impl WasmCanvasPlayer {
        /// Creates the GPU device and canvas surface. JavaScript calls this as an async factory.
        #[wasm_bindgen(js_name = create)]
        pub async fn create(
            canvas: HtmlCanvasElement,
            scene_json: &str,
            loop_duration_seconds: f64,
        ) -> Result<WasmCanvasPlayer, JsValue> {
            let player = ScenePlayer::from_scene_json(scene_json).map_err(js_error)?;
            let clock = PlaybackClock::looping(loop_duration_seconds).map_err(js_error)?;

            let mut instance_descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
            instance_descriptor.backends = wgpu::Backends::BROWSER_WEBGPU;
            let instance = wgpu::Instance::new(instance_descriptor);
            let surface = create_surface(&instance, &canvas)?;
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    force_fallback_adapter: false,
                    compatible_surface: Some(&surface),
                })
                .await
                .map_err(js_error)?;
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("Noon WebGPU device"),
                    ..Default::default()
                })
                .await
                .map_err(js_error)?;

            let width = canvas.width().max(1);
            let height = canvas.height().max(1);
            let config = surface
                .get_default_config(&adapter, width, height)
                .ok_or_else(|| js_message("WebGPU adapter cannot present to this canvas"))?;
            surface.configure(&device, &config);
            let renderer = GpuRenderer::new(&device, config.format);

            let mut result = Self {
                instance,
                surface,
                device,
                queue,
                canvas,
                config,
                drawable: true,
                player,
                clock,
                preparer: FramePreparer::new(),
                renderer,
                camera_center: Vec2::ZERO,
                camera_height: 6.0,
                clear_color: wgpu::Color {
                    r: 0.035,
                    g: 0.047,
                    b: 0.075,
                    a: 1.0,
                },
                last_draw_calls: 0,
                last_instances_drawn: 0,
                last_bytes_uploaded: 0,
            };
            result.update_camera()?;
            Ok(result)
        }

        /// Resizes the physical canvas backing store. Zero-sized canvases are simply skipped.
        #[wasm_bindgen(js_name = resize)]
        pub fn resize(&mut self, width: u32, height: u32) -> Result<(), JsValue> {
            self.canvas.set_width(width);
            self.canvas.set_height(height);
            self.drawable = width > 0 && height > 0;
            if !self.drawable {
                return Ok(());
            }

            if self.config.width != width || self.config.height != height {
                self.config.width = width;
                self.config.height = height;
                self.surface.configure(&self.device, &self.config);
                self.update_camera()?;
            }
            Ok(())
        }

        /// Sets a center and vertical world span while preserving canvas aspect ratio.
        #[wasm_bindgen(js_name = setCamera)]
        pub fn set_camera(
            &mut self,
            center_x: f32,
            center_y: f32,
            world_height: f32,
        ) -> Result<(), JsValue> {
            let center = Vec2::new(center_x, center_y);
            Camera2D::new(center, Vec2::new(world_height, world_height)).map_err(js_error)?;
            self.camera_center = center;
            self.camera_height = world_height;
            self.update_camera()
        }

        /// Advances from a monotonic `requestAnimationFrame` timestamp and presents one frame.
        #[wasm_bindgen(js_name = renderFrame)]
        pub fn render_frame(&mut self, timestamp_ms: f64) -> Result<bool, JsValue> {
            let scene_time = self.clock.scene_time(timestamp_ms).map_err(js_error)?;
            self.player.seek(scene_time).map_err(js_error)?;
            self.render_current_frame()
        }

        #[wasm_bindgen(js_name = applyPatchBatch)]
        pub fn apply_patch_batch(&mut self, json: &str) -> Result<(), JsValue> {
            self.player.apply_patch_batch_json(json).map_err(js_error)?;
            Ok(())
        }

        /// Atomically replaces semantic scene state while retaining the GPU and playback clock.
        #[wasm_bindgen(js_name = replaceScene)]
        pub fn replace_scene(&mut self, json: &str) -> Result<(), JsValue> {
            self.player.replace_scene_json(json).map_err(js_error)?;
            Ok(())
        }

        /// Reconciles compatible semantic state and falls back to atomic replacement.
        #[wasm_bindgen(js_name = reconcileScene)]
        pub fn reconcile_scene(&mut self, json: &str) -> Result<bool, JsValue> {
            Ok(matches!(
                self.player.reconcile_scene_json(json).map_err(js_error)?,
                ReconcileOutcome::Incremental { .. }
            ))
        }

        #[wasm_bindgen(js_name = resetClock)]
        pub fn reset_clock(&mut self) {
            self.clock.reset();
        }

        pub fn time(&self) -> f64 {
            self.player.frame().time
        }

        #[wasm_bindgen(js_name = objectCount)]
        pub fn object_count(&self) -> usize {
            self.player.object_count()
        }

        #[wasm_bindgen(js_name = nextSequence)]
        pub fn next_sequence(&self) -> u64 {
            self.player.next_sequence()
        }

        #[wasm_bindgen(js_name = lastDrawCalls)]
        pub fn last_draw_calls(&self) -> usize {
            self.last_draw_calls
        }

        #[wasm_bindgen(js_name = lastInstancesDrawn)]
        pub fn last_instances_drawn(&self) -> usize {
            self.last_instances_drawn
        }

        #[wasm_bindgen(js_name = lastBytesUploaded)]
        pub fn last_bytes_uploaded(&self) -> usize {
            self.last_bytes_uploaded
        }
    }

    impl WasmCanvasPlayer {
        fn update_camera(&mut self) -> Result<(), JsValue> {
            if !self.drawable {
                return Ok(());
            }
            let aspect = self.config.width as f32 / self.config.height as f32;
            let camera = Camera2D::new(
                self.camera_center,
                Vec2::new(self.camera_height * aspect, self.camera_height),
            )
            .map_err(js_error)?;
            self.renderer
                .set_viewport(&self.queue, self.config.width, self.config.height);
            self.renderer.set_camera(&self.queue, camera);
            Ok(())
        }

        fn render_current_frame(&mut self) -> Result<bool, JsValue> {
            if !self.drawable {
                return Ok(false);
            }

            let prepared = self.preparer.prepare(self.player.frame());
            let upload = self.renderer.upload(&self.device, &self.queue, &prepared);
            self.last_bytes_uploaded = upload.bytes_uploaded;

            let (surface_texture, reconfigure_after_present) =
                match self.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(texture) => (texture, false),
                    wgpu::CurrentSurfaceTexture::Suboptimal(texture) => (texture, true),
                    wgpu::CurrentSurfaceTexture::Timeout
                    | wgpu::CurrentSurfaceTexture::Occluded => return Ok(false),
                    wgpu::CurrentSurfaceTexture::Outdated => {
                        self.surface.configure(&self.device, &self.config);
                        return Ok(false);
                    }
                    wgpu::CurrentSurfaceTexture::Lost => {
                        self.surface = create_surface(&self.instance, &self.canvas)?;
                        self.surface.configure(&self.device, &self.config);
                        return Ok(false);
                    }
                    wgpu::CurrentSurfaceTexture::Validation => {
                        return Err(js_message("WebGPU rejected the canvas surface texture"));
                    }
                };
            let view = surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Noon browser frame"),
                });
            let draw = self
                .renderer
                .encode(&mut encoder, &view, &prepared, self.clear_color);
            self.queue.submit(Some(encoder.finish()));
            surface_texture.present();

            self.last_draw_calls = draw.draw_calls;
            self.last_instances_drawn = draw.instances_drawn;
            if reconfigure_after_present {
                self.surface.configure(&self.device, &self.config);
            }
            Ok(true)
        }
    }

    #[wasm_bindgen(js_name = demoSceneJson)]
    pub fn demo_scene_json() -> Result<String, JsValue> {
        let mut scene = SceneDefinition::new();
        let circle = scene.add(GeometryRef::circle(0.65));
        let rectangle = scene.add(GeometryRef::rectangle(1.5, 0.9));
        let line = scene.add(GeometryRef::line(Vec2::new(-1.2, 0.0), Vec2::new(1.2, 0.0)));

        scene.object_mut(circle).expect("circle exists").style = Style {
            fill: Some(Color::rgb(0.98, 0.38, 0.36)),
            stroke: Some(Color::WHITE),
            stroke_width: 0.04,
            opacity: 1.0,
        };
        scene.object_mut(rectangle).expect("rectangle exists").style = Style {
            fill: Some(Color::rgb(0.27, 0.65, 0.96)),
            stroke: Some(Color::WHITE),
            stroke_width: 0.04,
            opacity: 1.0,
        };
        scene
            .object_mut(rectangle)
            .expect("rectangle exists")
            .transform = Transform2D {
            rotation: -0.7,
            ..Transform2D::IDENTITY
        };
        scene.object_mut(line).expect("line exists").style = Style {
            fill: None,
            stroke: Some(Color::rgb(0.30, 0.88, 0.57)),
            stroke_width: 0.10,
            opacity: 1.0,
        };
        scene.object_mut(line).expect("line exists").transform = Transform2D {
            translation: Vec2::new(0.0, -1.55),
            rotation: -0.35,
            ..Transform2D::IDENTITY
        };

        let timing = TrackTiming::new(0.0, 4.0, Easing::EaseInOutCubic);
        scene
            .animate_position(circle, Vec2::new(-2.1, 0.8), Vec2::new(2.1, -0.8), timing)
            .map_err(js_error)?;
        scene
            .animate_position(
                rectangle,
                Vec2::new(2.1, 0.8),
                Vec2::new(-2.1, -0.8),
                timing,
            )
            .map_err(js_error)?;
        scene
            .animate_scalar(
                rectangle,
                Property::Rotation,
                -0.7,
                std::f32::consts::TAU - 0.7,
                timing,
            )
            .map_err(js_error)?;
        scene
            .animate_scalar(
                line,
                Property::Rotation,
                -0.35,
                std::f32::consts::TAU - 0.35,
                timing,
            )
            .map_err(js_error)?;
        encode_scene(&scene).map_err(js_error)
    }

    fn create_surface(
        instance: &wgpu::Instance,
        canvas: &HtmlCanvasElement,
    ) -> Result<wgpu::Surface<'static>, JsValue> {
        instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(js_error)
    }

    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }

    fn js_message(message: &str) -> JsValue {
        JsValue::from_str(message)
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, ObjectId, ScenePatch, Style, Transform2D, Vec2};
    use noon_ir::{encode_patch_batch, encode_scene, PatchBatch};

    use super::*;

    fn player() -> ScenePlayer {
        let mut scene = SceneDefinition::new();
        scene.add(GeometryRef::circle(1.0));
        let json = encode_scene(&scene).expect("scene must serialize");
        ScenePlayer::from_scene_json(&json).expect("player must load")
    }

    #[test]
    fn player_loads_and_seeks_without_reexecuting_frontend_code() {
        let mut player = player();
        player.seek(3.25).expect("seek must succeed");
        assert_eq!(player.frame().time, 3.25);
        assert_eq!(player.object_count(), 1);
    }

    #[test]
    fn ordered_patch_batch_preserves_playhead_and_advances_sequence() {
        let mut player = player();
        player.seek(2.0).expect("seek must succeed");
        let batch = PatchBatch::new(
            0,
            vec![ScenePatch::SetTransform {
                object: ObjectId::new(0),
                transform: Transform2D {
                    translation: Vec2::new(5.0, -2.0),
                    ..Transform2D::IDENTITY
                },
            }],
        );
        let json = encode_patch_batch(&batch).expect("batch must serialize");

        player
            .apply_patch_batch_json(&json)
            .expect("patch batch must apply");

        assert_eq!(player.frame().time, 2.0);
        assert_eq!(
            player.frame().objects[0].transform.translation,
            Vec2::new(5.0, -2.0)
        );
        assert_eq!(player.next_sequence(), 1);
    }

    #[test]
    fn scene_replacement_preserves_playhead_and_restarts_patch_sequence() {
        let mut player = player();
        player.seek(2.5).expect("seek must succeed");
        let patch =
            encode_patch_batch(&PatchBatch::new(0, Vec::new())).expect("batch must serialize");
        player
            .apply_patch_batch_json(&patch)
            .expect("patch batch must apply");

        let mut replacement = SceneDefinition::new();
        replacement.add(GeometryRef::circle(0.5));
        replacement.add(GeometryRef::rectangle(2.0, 1.0));
        let json = encode_scene(&replacement).expect("scene must serialize");

        player
            .replace_scene_json(&json)
            .expect("replacement must apply");

        assert_eq!(player.frame().time, 2.5);
        assert_eq!(player.object_count(), 2);
        assert_eq!(player.next_sequence(), 0);
    }

    #[test]
    fn invalid_scene_replacement_is_transactional() {
        let mut player = player();
        player.seek(1.25).expect("seek must succeed");
        let patch =
            encode_patch_batch(&PatchBatch::new(0, Vec::new())).expect("batch must serialize");
        player
            .apply_patch_batch_json(&patch)
            .expect("patch batch must apply");
        let before_scene = player.scene_json().expect("scene must serialize");
        let before_frame = player.frame().clone();

        assert!(player.replace_scene_json(r#"{"version":99}"#).is_err());

        assert_eq!(
            player.scene_json().expect("scene must serialize"),
            before_scene
        );
        assert_eq!(player.frame(), &before_frame);
        assert_eq!(player.next_sequence(), 1);
    }

    #[test]
    fn compatible_scene_reconciliation_applies_minimal_patch() {
        let mut player = player();
        player.seek(1.5).expect("seek must succeed");
        let mut desired = SceneDefinition::new();
        let object = desired.add(GeometryRef::circle(1.0));
        desired
            .object_mut(object)
            .expect("object must exist")
            .style
            .opacity = 0.4;
        let json = encode_scene(&desired).expect("scene must serialize");

        let outcome = player
            .reconcile_scene_json(&json)
            .expect("reconciliation must succeed");

        assert_eq!(outcome, ReconcileOutcome::Incremental { patch_count: 1 });
        assert_eq!(player.frame().time, 1.5);
        assert_eq!(player.frame().objects[0].style.opacity, 0.4);
        assert_eq!(player.next_sequence(), 0);
    }

    #[test]
    fn incompatible_geometry_reconciliation_falls_back_to_replacement() {
        let mut player = player();
        player.seek(0.75).expect("seek must succeed");
        let mut desired = SceneDefinition::new();
        desired.add(GeometryRef::rectangle(2.0, 1.0));
        let json = encode_scene(&desired).expect("scene must serialize");

        let outcome = player
            .reconcile_scene_json(&json)
            .expect("replacement fallback must succeed");

        assert_eq!(outcome, ReconcileOutcome::Replaced);
        assert_eq!(player.frame().time, 0.75);
        assert_eq!(
            player.frame().objects[0].geometry,
            GeometryRef::rectangle(2.0, 1.0)
        );
    }

    #[test]
    fn reordered_scene_reconciliation_falls_back_to_preserve_draw_order() {
        let mut current = SceneDefinition::new();
        current.add(GeometryRef::circle(1.0));
        current.add(GeometryRef::rectangle(1.0, 1.0));
        let json = encode_scene(&current).expect("scene must serialize");
        let mut player = ScenePlayer::from_scene_json(&json).expect("scene must load");
        let desired = SceneDefinition::from_parts(
            vec![
                noon_core::ObjectDefinition::new(
                    ObjectId::new(1),
                    GeometryRef::rectangle(1.0, 1.0),
                ),
                noon_core::ObjectDefinition::new(ObjectId::new(0), GeometryRef::circle(1.0)),
            ],
            Vec::new(),
        )
        .expect("scene must be valid");
        let json = encode_scene(&desired).expect("scene must serialize");

        assert_eq!(
            player
                .reconcile_scene_json(&json)
                .expect("fallback must succeed"),
            ReconcileOutcome::Replaced
        );
        assert_eq!(player.frame().objects[0].id, ObjectId::new(1));
    }

    #[test]
    fn patch_batch_is_transactional_when_later_patch_fails() {
        let mut player = player();
        let before_scene = player.scene_json().expect("scene must serialize");
        let before_frame = player.frame().clone();
        let batch = PatchBatch::new(
            0,
            vec![
                ScenePatch::SetStyle {
                    object: ObjectId::new(0),
                    style: Style {
                        opacity: 0.25,
                        ..Style::default()
                    },
                },
                ScenePatch::SetTransform {
                    object: ObjectId::new(999),
                    transform: Transform2D::IDENTITY,
                },
            ],
        );
        let json = encode_patch_batch(&batch).expect("batch must serialize");

        assert!(player.apply_patch_batch_json(&json).is_err());
        assert_eq!(
            player.scene_json().expect("scene must serialize"),
            before_scene
        );
        assert_eq!(player.frame(), &before_frame);
        assert_eq!(player.next_sequence(), 0);
    }

    #[test]
    fn out_of_order_patch_batch_is_rejected_without_mutation() {
        let mut player = player();
        let before = player.scene_json().expect("scene must serialize");
        let json =
            encode_patch_batch(&PatchBatch::new(3, Vec::new())).expect("batch must serialize");

        assert!(matches!(
            player.apply_patch_batch_json(&json),
            Err(PlayerError::Sequence {
                expected: 0,
                actual: 3
            })
        ));
        assert_eq!(player.scene_json().expect("scene must serialize"), before);
        assert_eq!(player.next_sequence(), 0);
    }

    #[test]
    fn javascript_shaped_style_batch_applies_transactionally() {
        let mut player = player();
        player.seek(1.75).expect("seek must succeed");
        let json = r#"{
            "version": 1,
            "sequence": 0,
            "patches": [{
                "set_style": {
                    "object": 0,
                    "style": {
                        "fill": {"red": 1.0, "green": 0.75, "blue": 0.2, "alpha": 1.0},
                        "stroke": null,
                        "stroke_width": 0.04,
                        "opacity": 0.8
                    }
                }
            }]
        }"#;

        player
            .apply_patch_batch_json(json)
            .expect("JavaScript-shaped batch must apply");

        assert_eq!(player.frame().time, 1.75);
        assert_eq!(player.frame().objects[0].style.opacity, 0.8);
        assert_eq!(
            player.frame().objects[0].style.fill,
            Some(noon_core::Color::rgba(1.0, 0.75, 0.2, 1.0))
        );
        assert_eq!(player.next_sequence(), 1);
    }
}
