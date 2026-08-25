//! Browser-facing persistent Noon runtime.
//!
//! The core player is ordinary Rust so its state transitions are testable on
//! native CI. A thin wasm-bindgen wrapper exposes the same semantics to JavaScript.

#![forbid(unsafe_code)]

mod clock;

pub use clock::*;

use noon_compile::{CompileError, CompilePatchError, CompiledScene};
use std::collections::{BTreeMap, BTreeSet};

use noon_core::{
    preflight_transaction, MutationTransaction, ObjectId, PatchError, SceneDefinition, ScenePatch,
};
use noon_ir::{decode_patch_batch, decode_scene, encode_scene, IrError};
use noon_runtime::{
    EvaluationError, ExecutionDelta, ExecutionTransactionError, FrameChanges, FrameState,
    SlottedSceneInstance,
};

#[derive(Debug)]
pub enum PlayerError {
    Ir(IrError),
    Compile(CompileError),
    Patch(PatchError),
    CompilePatch(CompilePatchError),
    ExecutionTransaction(ExecutionTransactionError),
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
            Self::ExecutionTransaction(error) => {
                write!(formatter, "execution transaction failed: {error}")
            }
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

impl From<ExecutionTransactionError> for PlayerError {
    fn from(value: ExecutionTransactionError) -> Self {
        Self::ExecutionTransaction(value)
    }
}

impl From<EvaluationError> for PlayerError {
    fn from(value: EvaluationError) -> Self {
        Self::Evaluation(value)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerTransactionStats {
    pub mutations: usize,
    pub semantic_scene_clones: usize,
    pub runtime_rebuilds: usize,
}

#[derive(Clone, Debug)]
pub struct ScenePlayer {
    definition: SceneDefinition,
    instance: SlottedSceneInstance,
    next_sequence: u64,
    last_transaction_stats: PlayerTransactionStats,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReconcileOutcome {
    Incremental { patch_count: usize },
    Rebuilt { patch_count: usize },
    Replaced,
}

impl ScenePlayer {
    pub fn from_scene_json(json: &str) -> Result<Self, PlayerError> {
        let definition = decode_scene(json)?;
        let compiled = CompiledScene::compile(&definition)?;
        Ok(Self {
            definition,
            instance: SlottedSceneInstance::new(compiled),
            next_sequence: 0,
            last_transaction_stats: PlayerTransactionStats::default(),
        })
    }

    pub fn seek(&mut self, time: f64) -> Result<&FrameState, PlayerError> {
        Ok(self.instance.seek(time)?)
    }

    pub fn advance_to(&mut self, time: f64) -> Result<&FrameState, PlayerError> {
        Ok(self.instance.advance_to(time)?)
    }

    pub fn take_frame_changes(&mut self) -> FrameChanges {
        self.instance.take_frame_changes()
    }

    /// Apply host-callback mutations without consuming the interactive patch sequence.
    pub(crate) fn apply_host_patch_batch_json(
        &mut self,
        json: &str,
    ) -> Result<&FrameState, PlayerError> {
        let batch = decode_patch_batch(json)?;
        self.apply_patches_transactionally(&batch.patches)?;
        Ok(self.instance.frame())
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
        let mut instance = SlottedSceneInstance::new(compiled);
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
        let transaction = MutationTransaction::from_mutations(patches.iter().cloned());
        preflight_transaction(&self.definition, &transaction)?;
        self.instance.apply_transaction(&transaction)?;
        for patch in patches {
            self.definition
                .apply_patch(patch.clone())
                .expect("semantic transaction was fully preflighted");
        }
        self.last_transaction_stats = PlayerTransactionStats {
            mutations: patches.len(),
            semantic_scene_clones: 0,
            runtime_rebuilds: 0,
        };
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

    pub const fn last_transaction_stats(&self) -> PlayerTransactionStats {
        self.last_transaction_stats
    }

    pub fn last_execution_delta(&self) -> &ExecutionDelta {
        self.instance.last_execution_delta()
    }

    pub fn object_count(&self) -> usize {
        self.instance.live_object_count()
    }

    pub(crate) fn live_frame_indices(&self) -> Vec<usize> {
        self.instance.live_frame_indices()
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
    use std::{
        cell::{Cell, RefCell},
        collections::VecDeque,
        rc::Rc,
    };

    use noon_core::{
        Color, Easing, GeometryRef, Property, SceneDefinition, Style, TrackTiming, Transform2D,
        Vec2, VectorPath,
    };
    use noon_ir::encode_scene;
    use noon_render_wgpu::{Camera2D, FramePreparer, GpuRenderer};
    use wasm_bindgen::prelude::*;
    use web_sys::HtmlCanvasElement;

    use super::{PlaybackClock, ReconcileOutcome, ScenePlayer};

    const GPU_QUERY_BYTES: u64 = 16;
    const GPU_PROFILE_SLOT_COUNT: usize = 4;
    const GPU_PROFILE_SAMPLE_CAPACITY: usize = 512;

    #[derive(Debug)]
    struct WebDisplaySource;

    impl wgpu::rwh::HasDisplayHandle for WebDisplaySource {
        fn display_handle(&self) -> Result<wgpu::rwh::DisplayHandle<'_>, wgpu::rwh::HandleError> {
            Ok(wgpu::rwh::DisplayHandle::web())
        }
    }

    #[derive(Clone, Copy)]
    struct GpuSampleToken {
        slot: usize,
        generation: u64,
    }

    struct GpuTimingSlot {
        query_set: wgpu::QuerySet,
        resolve_buffer: wgpu::Buffer,
        readback_buffer: wgpu::Buffer,
        pending: Rc<Cell<bool>>,
    }

    #[derive(Default)]
    struct GpuTimingState {
        generation: u64,
        total_samples: usize,
        dropped_samples: usize,
        failed_samples: usize,
        samples_ms: VecDeque<f64>,
    }

    impl GpuTimingState {
        fn reset(&mut self) {
            self.generation = self.generation.wrapping_add(1);
            self.total_samples = 0;
            self.dropped_samples = 0;
            self.failed_samples = 0;
            self.samples_ms.clear();
        }

        fn record(&mut self, milliseconds: f64) {
            self.total_samples += 1;
            if self.samples_ms.len() == GPU_PROFILE_SAMPLE_CAPACITY {
                self.samples_ms.pop_front();
            }
            self.samples_ms.push_back(milliseconds);
        }

        fn percentile(&self, percentile: f64) -> Option<f64> {
            if self.samples_ms.is_empty() {
                return None;
            }
            let mut sorted = self.samples_ms.iter().copied().collect::<Vec<_>>();
            sorted.sort_by(f64::total_cmp);
            let rank = (percentile * sorted.len() as f64).ceil() as usize;
            Some(sorted[rank.saturating_sub(1).min(sorted.len() - 1)])
        }
    }

    struct GpuFrameProfiler {
        enabled: bool,
        timestamp_period_ns: f64,
        next_slot: usize,
        slots: Vec<GpuTimingSlot>,
        state: Rc<RefCell<GpuTimingState>>,
    }

    impl GpuFrameProfiler {
        fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
            let slots = (0..GPU_PROFILE_SLOT_COUNT)
                .map(|_| GpuTimingSlot {
                    query_set: device.create_query_set(&wgpu::QuerySetDescriptor {
                        label: Some("Noon GPU frame timestamps"),
                        ty: wgpu::QueryType::Timestamp,
                        count: 2,
                    }),
                    resolve_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("Noon GPU timestamp resolve buffer"),
                        size: GPU_QUERY_BYTES,
                        usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                        mapped_at_creation: false,
                    }),
                    readback_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("Noon GPU timestamp readback buffer"),
                        size: GPU_QUERY_BYTES,
                        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                        mapped_at_creation: false,
                    }),
                    pending: Rc::new(Cell::new(false)),
                })
                .collect();
            Self {
                enabled: false,
                timestamp_period_ns: f64::from(queue.get_timestamp_period()),
                next_slot: 0,
                slots,
                state: Rc::new(RefCell::new(GpuTimingState::default())),
            }
        }

        fn set_enabled(&mut self, enabled: bool) {
            if enabled && !self.enabled {
                self.reset();
            }
            self.enabled = enabled;
        }

        fn reset(&mut self) {
            self.state.borrow_mut().reset();
        }

        fn begin_sample(&mut self) -> Option<GpuSampleToken> {
            if !self.enabled {
                return None;
            }
            let generation = self.state.borrow().generation;
            for offset in 0..self.slots.len() {
                let index = (self.next_slot + offset) % self.slots.len();
                if !self.slots[index].pending.replace(true) {
                    self.next_slot = (index + 1) % self.slots.len();
                    return Some(GpuSampleToken {
                        slot: index,
                        generation,
                    });
                }
            }
            self.state.borrow_mut().dropped_samples += 1;
            None
        }

        fn query_set(&self, sample: GpuSampleToken) -> &wgpu::QuerySet {
            &self.slots[sample.slot].query_set
        }

        fn finish_sample(&mut self, encoder: &mut wgpu::CommandEncoder, sample: GpuSampleToken) {
            let slot = &self.slots[sample.slot];
            encoder.resolve_query_set(&slot.query_set, 0..2, &slot.resolve_buffer, 0);
            encoder.copy_buffer_to_buffer(
                &slot.resolve_buffer,
                0,
                &slot.readback_buffer,
                0,
                GPU_QUERY_BYTES,
            );

            let readback = slot.readback_buffer.clone();
            let pending = Rc::clone(&slot.pending);
            let state = Rc::clone(&self.state);
            let timestamp_period_ns = self.timestamp_period_ns;
            encoder.map_buffer_on_submit(
                &slot.readback_buffer,
                wgpu::MapMode::Read,
                ..,
                move |result| {
                    if result.is_ok() {
                        let bytes = readback.slice(..).get_mapped_range();
                        let beginning = u64::from_le_bytes(
                            bytes[0..8].try_into().expect("timestamp is eight bytes"),
                        );
                        let end = u64::from_le_bytes(
                            bytes[8..16].try_into().expect("timestamp is eight bytes"),
                        );
                        drop(bytes);
                        readback.unmap();
                        let mut state = state.borrow_mut();
                        if state.generation == sample.generation {
                            if let Some(ticks) = end.checked_sub(beginning) {
                                state.record(ticks as f64 * timestamp_period_ns / 1_000_000.0);
                            } else {
                                state.failed_samples += 1;
                            }
                        }
                    } else {
                        // wgpu records the requested map range before the backend
                        // completes the async operation. A failed map therefore
                        // still needs an explicit unmap before this ring slot can
                        // safely be reused by a later GPU submission.
                        readback.unmap();
                        let mut state = state.borrow_mut();
                        if state.generation == sample.generation {
                            state.failed_samples += 1;
                        }
                    }
                    pending.set(false);
                },
            );
        }
    }

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

    #[wasm_bindgen(js_name = NoonCanvasPlayer)]
    pub struct WasmCanvasPlayer {
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        backend: wgpu::Backend,
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
        last_geometry_cache_misses: usize,
        last_cpu_frame_ms: f64,
        last_runtime_evaluation_ms: f64,
        last_frame_prepare_ms: f64,
        last_upload_ms: f64,
        last_encode_submit_ms: f64,
        gpu_profiler: Option<GpuFrameProfiler>,
    }

    #[wasm_bindgen(js_class = NoonCanvasPlayer)]
    impl WasmCanvasPlayer {
        #[wasm_bindgen(js_name = create)]
        pub async fn create(
            canvas: HtmlCanvasElement,
            scene_json: &str,
            loop_duration_seconds: f64,
        ) -> Result<WasmCanvasPlayer, JsValue> {
            let player = ScenePlayer::from_scene_json(scene_json).map_err(js_error)?;
            let clock = PlaybackClock::looping(loop_duration_seconds).map_err(js_error)?;

            let mut instance_descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
            instance_descriptor.backends = wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL;
            instance_descriptor.display = Some(Box::new(WebDisplaySource));
            let instance =
                wgpu::util::new_instance_with_webgpu_detection(instance_descriptor).await;
            let surface = create_surface(&instance, &canvas)?;
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    force_fallback_adapter: false,
                    compatible_surface: Some(&surface),
                })
                .await
                .map_err(js_error)?;
            let backend = adapter.get_info().backend;
            let timestamp_queries_supported =
                adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
            let required_features = if timestamp_queries_supported {
                wgpu::Features::TIMESTAMP_QUERY
            } else {
                wgpu::Features::empty()
            };
            let required_limits = if backend == wgpu::Backend::Gl {
                wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())
            } else {
                wgpu::Limits::default()
            };
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("Noon browser GPU device"),
                    required_features,
                    required_limits,
                    ..Default::default()
                })
                .await
                .map_err(js_error)?;

            let width = canvas.width().max(1);
            let height = canvas.height().max(1);
            let config = surface
                .get_default_config(&adapter, width, height)
                .ok_or_else(|| js_message("GPU adapter cannot present to this canvas"))?;
            surface.configure(&device, &config);
            let renderer = GpuRenderer::new(&device, config.format);
            let gpu_profiler =
                timestamp_queries_supported.then(|| GpuFrameProfiler::new(&device, &queue));

            let mut result = Self {
                instance,
                surface,
                device,
                queue,
                backend,
                canvas,
                config,
                drawable: true,
                player,
                clock,
                preparer: FramePreparer::new(),
                renderer,
                camera_center: Vec2::ZERO,
                camera_height: noon_core::DEFAULT_FRAME_HEIGHT,
                clear_color: wgpu::Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
                last_draw_calls: 0,
                last_instances_drawn: 0,
                last_bytes_uploaded: 0,
                last_geometry_cache_misses: 0,
                last_cpu_frame_ms: f64::NAN,
                last_runtime_evaluation_ms: f64::NAN,
                last_frame_prepare_ms: f64::NAN,
                last_upload_ms: f64::NAN,
                last_encode_submit_ms: f64::NAN,
                gpu_profiler,
            };
            result.update_camera()?;
            Ok(result)
        }

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

        #[wasm_bindgen(js_name = renderFrame)]
        pub fn render_frame(&mut self, timestamp_ms: f64) -> Result<bool, JsValue> {
            let frame_started_ms = performance_now_ms();
            let scene_time = self.clock.scene_time(timestamp_ms).map_err(js_error)?;
            let runtime_started_ms = performance_now_ms();
            self.player.advance_to(scene_time).map_err(js_error)?;
            self.last_runtime_evaluation_ms = elapsed_ms(runtime_started_ms);
            let presented = self.render_current_frame()?;
            self.last_cpu_frame_ms = elapsed_ms(frame_started_ms);
            Ok(presented)
        }

        #[wasm_bindgen(js_name = applyPatchBatch)]
        pub fn apply_patch_batch(&mut self, json: &str) -> Result<(), JsValue> {
            self.player.apply_patch_batch_json(json).map_err(js_error)?;
            Ok(())
        }

        #[wasm_bindgen(js_name = replaceScene)]
        pub fn replace_scene(&mut self, json: &str) -> Result<(), JsValue> {
            self.player.replace_scene_json(json).map_err(js_error)?;
            Ok(())
        }

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

        #[wasm_bindgen(js_name = rendererBackend)]
        pub fn renderer_backend(&self) -> String {
            match self.backend {
                wgpu::Backend::BrowserWebGpu => "WebGPU".to_owned(),
                wgpu::Backend::Gl => "WebGL2".to_owned(),
                other => format!("{other:?}"),
            }
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

        #[wasm_bindgen(js_name = lastGeometryCacheMisses)]
        pub fn last_geometry_cache_misses(&self) -> usize {
            self.last_geometry_cache_misses
        }

        #[wasm_bindgen(js_name = lastCpuFrameMs)]
        pub fn last_cpu_frame_ms(&self) -> f64 {
            self.last_cpu_frame_ms
        }

        #[wasm_bindgen(js_name = lastRuntimeEvaluationMs)]
        pub fn last_runtime_evaluation_ms(&self) -> f64 {
            self.last_runtime_evaluation_ms
        }

        #[wasm_bindgen(js_name = lastFramePrepareMs)]
        pub fn last_frame_prepare_ms(&self) -> f64 {
            self.last_frame_prepare_ms
        }

        #[wasm_bindgen(js_name = lastUploadMs)]
        pub fn last_upload_ms(&self) -> f64 {
            self.last_upload_ms
        }

        #[wasm_bindgen(js_name = lastEncodeSubmitMs)]
        pub fn last_encode_submit_ms(&self) -> f64 {
            self.last_encode_submit_ms
        }

        #[wasm_bindgen(js_name = gpuProfilingSupported)]
        pub fn gpu_profiling_supported(&self) -> bool {
            self.gpu_profiler.is_some()
        }

        #[wasm_bindgen(js_name = setGpuProfilingEnabled)]
        pub fn set_gpu_profiling_enabled(&mut self, enabled: bool) -> bool {
            if let Some(profiler) = &mut self.gpu_profiler {
                profiler.set_enabled(enabled);
                true
            } else {
                false
            }
        }

        #[wasm_bindgen(js_name = resetGpuProfiling)]
        pub fn reset_gpu_profiling(&mut self) {
            if let Some(profiler) = &mut self.gpu_profiler {
                profiler.reset();
            }
        }

        #[wasm_bindgen(js_name = gpuProfiledFrameCount)]
        pub fn gpu_profiled_frame_count(&self) -> usize {
            self.gpu_profiler
                .as_ref()
                .map_or(0, |profiler| profiler.state.borrow().total_samples)
        }

        #[wasm_bindgen(js_name = gpuDroppedSampleCount)]
        pub fn gpu_dropped_sample_count(&self) -> usize {
            self.gpu_profiler
                .as_ref()
                .map_or(0, |profiler| profiler.state.borrow().dropped_samples)
        }

        #[wasm_bindgen(js_name = gpuFailedSampleCount)]
        pub fn gpu_failed_sample_count(&self) -> usize {
            self.gpu_profiler
                .as_ref()
                .map_or(0, |profiler| profiler.state.borrow().failed_samples)
        }

        #[wasm_bindgen(js_name = lastGpuRenderMs)]
        pub fn last_gpu_render_ms(&self) -> f64 {
            self.gpu_profiler
                .as_ref()
                .and_then(|profiler| profiler.state.borrow().samples_ms.back().copied())
                .unwrap_or(f64::NAN)
        }

        #[wasm_bindgen(js_name = gpuRenderP50Ms)]
        pub fn gpu_render_p50_ms(&self) -> f64 {
            self.gpu_render_percentile(0.50)
        }

        #[wasm_bindgen(js_name = gpuRenderP95Ms)]
        pub fn gpu_render_p95_ms(&self) -> f64 {
            self.gpu_render_percentile(0.95)
        }
    }

    impl WasmCanvasPlayer {
        fn gpu_render_percentile(&self, percentile: f64) -> f64 {
            self.gpu_profiler
                .as_ref()
                .and_then(|profiler| profiler.state.borrow().percentile(percentile))
                .unwrap_or(f64::NAN)
        }

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
            self.renderer.set_viewport(
                &self.device,
                &self.queue,
                self.config.width,
                self.config.height,
            );
            self.renderer.set_camera(&self.queue, camera);
            Ok(())
        }

        fn render_current_frame(&mut self) -> Result<bool, JsValue> {
            if !self.drawable {
                return Ok(false);
            }

            let prepare_started_ms = performance_now_ms();
            let changes = self.player.take_frame_changes();
            let prepared = self
                .preparer
                .prepare_incremental(self.player.frame(), &changes);
            self.last_frame_prepare_ms = elapsed_ms(prepare_started_ms);
            self.last_geometry_cache_misses = prepared.stats.geometry_cache_misses;
            let upload_started_ms = performance_now_ms();
            let upload = self.renderer.upload(&self.device, &self.queue, &prepared);
            self.last_upload_ms = elapsed_ms(upload_started_ms);
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
                        return Err(js_message(
                            "GPU backend rejected the canvas surface texture",
                        ));
                    }
                };
            let view = surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let encode_started_ms = performance_now_ms();
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Noon browser frame"),
                });
            let gpu_sample = self
                .gpu_profiler
                .as_mut()
                .and_then(GpuFrameProfiler::begin_sample);
            let draw = if let Some(sample) = gpu_sample {
                let query_set = self
                    .gpu_profiler
                    .as_ref()
                    .expect("GPU profiler exists for an active sample")
                    .query_set(sample);
                self.renderer.encode_profiled(
                    &mut encoder,
                    &view,
                    &prepared,
                    self.clear_color,
                    query_set,
                )
            } else {
                self.renderer
                    .encode(&mut encoder, &view, &prepared, self.clear_color)
            };
            if let Some(sample) = gpu_sample {
                self.gpu_profiler
                    .as_mut()
                    .expect("GPU profiler exists for an active sample")
                    .finish_sample(&mut encoder, sample);
            }
            self.queue.submit(Some(encoder.finish()));
            self.last_encode_submit_ms = elapsed_ms(encode_started_ms);
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
        let path = scene.add(GeometryRef::path(
            VectorPath::new()
                .move_to(Vec2::new(-0.8, -0.2))
                .cubic_to(
                    Vec2::new(-0.8, 0.55),
                    Vec2::new(0.0, 0.85),
                    Vec2::new(0.0, 0.2),
                )
                .cubic_to(
                    Vec2::new(0.0, 0.85),
                    Vec2::new(0.8, 0.55),
                    Vec2::new(0.8, -0.2),
                )
                .cubic_to(
                    Vec2::new(0.65, -0.8),
                    Vec2::new(-0.65, -0.8),
                    Vec2::new(-0.8, -0.2),
                )
                .close(),
        ));

        scene.object_mut(circle).expect("circle exists").style = Style {
            fill: Some(Color::rgb(0.98, 0.38, 0.36)),
            stroke: Some(Color::WHITE),
            stroke_width: 0.04,
            stroke_width_mode: Default::default(),
            opacity: 1.0,
            stroke_join: noon_core::StrokeJoin::Round,
            stroke_cap: noon_core::StrokeCap::Round,
        };
        scene.object_mut(rectangle).expect("rectangle exists").style = Style {
            fill: Some(Color::rgb(0.27, 0.65, 0.96)),
            stroke: Some(Color::WHITE),
            stroke_width: 0.04,
            stroke_width_mode: Default::default(),
            opacity: 1.0,
            stroke_join: noon_core::StrokeJoin::Round,
            stroke_cap: noon_core::StrokeCap::Round,
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
            stroke_width_mode: Default::default(),
            opacity: 1.0,
            stroke_join: noon_core::StrokeJoin::Round,
            stroke_cap: noon_core::StrokeCap::Round,
        };
        scene.object_mut(line).expect("line exists").transform = Transform2D {
            translation: Vec2::new(0.0, -1.55),
            rotation: -0.35,
            ..Transform2D::IDENTITY
        };
        scene.object_mut(path).expect("path exists").style = Style {
            fill: Some(Color::rgb(0.62, 0.38, 0.96)),
            stroke: Some(Color::WHITE),
            stroke_width: 0.06,
            stroke_width_mode: Default::default(),
            opacity: 0.95,
            stroke_join: noon_core::StrokeJoin::Round,
            stroke_cap: noon_core::StrokeCap::Round,
        };
        scene.object_mut(path).expect("path exists").transform = Transform2D {
            translation: Vec2::new(0.0, 1.45),
            scale: Vec2::new(0.75, 0.75),
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

    fn performance_now_ms() -> f64 {
        web_sys::window()
            .and_then(|window| window.performance())
            .map_or(f64::NAN, |performance| performance.now())
    }

    fn elapsed_ms(start_ms: f64) -> f64 {
        let end_ms = performance_now_ms();
        if start_ms.is_finite() && end_ms.is_finite() {
            (end_ms - start_ms).max(0.0)
        } else {
            f64::NAN
        }
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
    use noon_core::{
        Easing, GeometryRef, ObjectDefinition, ObjectId, ObjectSnapshot, Property, ScenePatch,
        StrokeCap, StrokeJoin, Style, TrackDefinition, TrackId, TrackTiming, TrackValues,
        Transform2D, Vec2,
    };
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
                        stroke_join: noon_core::StrokeJoin::Round,
                        stroke_cap: noon_core::StrokeCap::Round,
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

    fn grid_scene(columns: usize, rows: usize) -> SceneDefinition {
        let mut scene = SceneDefinition::new();
        for row in 0..rows {
            for column in 0..columns {
                let x = column as f32 * 0.1;
                let y = row as f32 * 0.1;
                let object = scene.add(GeometryRef::circle(0.05));
                scene
                    .object_mut(object)
                    .expect("grid object exists")
                    .transform
                    .translation = Vec2::new(x, y);
                scene
                    .animate_position(
                        object,
                        Vec2::new(x, y),
                        Vec2::new(x * 0.8 - y * 0.1, y * 0.8 + x * 0.1),
                        TrackTiming::new(0.0, 3.0, Easing::EaseInOutCubic),
                    )
                    .expect("grid track must be valid");
            }
        }
        scene
    }

    #[test]
    fn dense_grid_edit_stays_incremental_and_preserves_playhead() {
        let initial = grid_scene(18, 10);
        let json = encode_scene(&initial).expect("initial grid must serialize");
        let mut player = ScenePlayer::from_scene_json(&json).expect("grid must load");
        player.seek(1.75).expect("seek must succeed");

        let desired = grid_scene(20, 10);
        let json = encode_scene(&desired).expect("expanded grid must serialize");
        let outcome = player
            .reconcile_scene_json(&json)
            .expect("grid reconciliation must succeed");

        let ReconcileOutcome::Incremental { patch_count } = outcome else {
            panic!("dense structural edit must stay incremental: {outcome:?}");
        };
        assert!(
            patch_count > 180,
            "grid edit should contain many semantic changes"
        );
        assert_eq!(player.object_count(), 200);
        assert_eq!(player.frame().time, 1.75);
        assert_eq!(player.next_sequence(), 0);
    }

    #[test]
    fn no_op_scene_rerun_remains_incremental_without_mutation() {
        let mut player = player();
        player.seek(0.625).expect("seek must succeed");
        let json = player.scene_json().expect("scene must serialize");
        assert_eq!(
            player
                .reconcile_scene_json(&json)
                .expect("no-op reconcile must succeed"),
            ReconcileOutcome::Incremental { patch_count: 0 }
        );
        assert_eq!(player.frame().time, 0.625);
    }

    #[test]
    fn join_and_cap_only_edit_uses_value_only_reconciliation() {
        let mut player = player();
        let mut desired = SceneDefinition::new();
        let object = desired.add(GeometryRef::circle(1.0));
        desired.object_mut(object).expect("object exists").style = Style {
            stroke_join: StrokeJoin::Bevel,
            stroke_cap: StrokeCap::Square,
            ..Style::default()
        };
        let json = encode_scene(&desired).expect("scene must serialize");

        assert_eq!(
            player
                .reconcile_scene_json(&json)
                .expect("style reconcile must succeed"),
            ReconcileOutcome::Incremental { patch_count: 1 }
        );
        assert_eq!(
            player.frame().objects[0].style.stroke_join,
            StrokeJoin::Bevel
        );
        assert_eq!(
            player.frame().objects[0].style.stroke_cap,
            StrokeCap::Square
        );
    }

    fn transform_scene(target_x: f32) -> SceneDefinition {
        let object = ObjectDefinition::new(ObjectId::new(0), GeometryRef::circle(1.0));
        let from = ObjectSnapshot::new(GeometryRef::circle(1.0));
        let mut to = ObjectSnapshot::new(GeometryRef::circle(1.0));
        to.transform.translation = Vec2::new(target_x, 0.0);
        let track = TrackDefinition {
            id: TrackId::new(0),
            object: ObjectId::new(0),
            property: Property::Transform,
            values: TrackValues::Object { from, to },
            timing: TrackTiming::new(0.0, 2.0, Easing::Linear),
            time_map: noon_core::CompositionTimeMap::identity(),
        };
        SceneDefinition::from_parts(vec![object], vec![track]).expect("transform scene is valid")
    }

    #[test]
    fn generic_transform_target_edit_is_detected_by_rust_reconciliation() {
        let initial = transform_scene(1.0);
        let json = encode_scene(&initial).expect("scene must serialize");
        let mut player = ScenePlayer::from_scene_json(&json).expect("scene must load");
        player.seek(0.5).expect("seek must succeed");

        let desired = transform_scene(3.0);
        let json = encode_scene(&desired).expect("scene must serialize");
        assert_eq!(
            player
                .reconcile_scene_json(&json)
                .expect("transform edit must reconcile"),
            ReconcileOutcome::Incremental { patch_count: 1 }
        );
        assert_eq!(player.frame().time, 0.5);
        assert!((player.frame().objects[0].transform.translation.x - 0.75).abs() < 1.0e-6);
    }

    #[test]
    fn hundred_thousand_object_remove_is_atomic_local_and_bounded() {
        let mut scene = SceneDefinition::new();
        for _ in 0..100_000 {
            scene.add(GeometryRef::circle(1.0));
        }
        let json = encode_scene(&scene).expect("large scene serializes");
        let mut player = ScenePlayer::from_scene_json(&json).expect("large scene loads");
        let retained_before = player
            .instance
            .slot_for_object(ObjectId::new(99_999))
            .expect("retained slot exists");
        let batch = PatchBatch::new(0, vec![ScenePatch::RemoveObject(ObjectId::new(10))]);
        let json = encode_patch_batch(&batch).expect("batch serializes");

        player
            .apply_patch_batch_json(&json)
            .expect("local removal succeeds");

        assert_eq!(player.object_count(), 99_999);
        assert_eq!(
            player.instance.slot_for_object(ObjectId::new(99_999)),
            Some(retained_before)
        );
        assert_eq!(player.last_transaction_stats().semantic_scene_clones, 0);
        assert_eq!(player.last_transaction_stats().runtime_rebuilds, 0);
        assert_eq!(player.last_execution_delta().slots().len(), 1);
        let runtime = player.instance.scene_instance().last_patch_stats();
        assert_eq!(runtime.object_slots_retired, 1);
        assert_eq!(runtime.full_group_rebuilds, 0);
        assert_eq!(runtime.full_seeks, 0);
    }

    #[test]
    fn compile_only_failure_keeps_browser_scene_and_frame_atomic() {
        let mut player = player();
        let before_scene = player.scene_json().expect("scene serializes");
        let before_frame = player.frame().clone();
        let from = ObjectSnapshot::new(GeometryRef::circle(1.0));
        let to = ObjectSnapshot::new(GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)));
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
                ScenePatch::AddTrack(TrackDefinition {
                    id: TrackId::new(50),
                    object: ObjectId::new(0),
                    property: Property::Transform,
                    values: TrackValues::Object { from, to },
                    timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
                    time_map: noon_core::CompositionTimeMap::identity(),
                }),
            ],
        );
        let json = encode_patch_batch(&batch).expect("batch serializes");

        assert!(matches!(
            player.apply_patch_batch_json(&json),
            Err(PlayerError::ExecutionTransaction(
                ExecutionTransactionError::Compile(
                    CompilePatchError::UnsupportedTransformGeometry(_)
                )
            ))
        ));
        assert_eq!(player.scene_json().expect("scene serializes"), before_scene);
        assert_eq!(player.frame(), &before_frame);
        assert_eq!(player.next_sequence(), 0);
    }
}
