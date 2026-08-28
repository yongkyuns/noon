#[cfg(any(target_arch = "wasm32", test))]
const MANIM_DEFAULT_CAMERA_HEIGHT: f32 = 8.0;

#[cfg(target_arch = "wasm32")]
const MANIM_DEFAULT_CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::mem;

    use noon_core::Vec2;
    use noon_render_wgpu::{Camera2D, FramePreparer, GpuRenderer};
    use noon_runtime::FrameChanges;
    use wasm_bindgen::prelude::*;
    use web_sys::OffscreenCanvas;

    use crate::{ExecutionFrameMirror, TransportApplyOutcome};

    use super::{MANIM_DEFAULT_CAMERA_HEIGHT, MANIM_DEFAULT_CLEAR_COLOR};

    #[derive(Debug)]
    struct WebDisplaySource;

    impl wgpu::rwh::HasDisplayHandle for WebDisplaySource {
        fn display_handle(&self) -> Result<wgpu::rwh::DisplayHandle<'_>, wgpu::rwh::HandleError> {
            Ok(wgpu::rwh::DisplayHandle::web())
        }
    }

    struct InitializedGpu {
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        backend: wgpu::Backend,
        config: wgpu::SurfaceConfiguration,
    }

    #[wasm_bindgen(js_name = ExecutionCanvasRenderer)]
    pub struct WasmExecutionCanvasRenderer {
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        backend: wgpu::Backend,
        canvas: OffscreenCanvas,
        config: wgpu::SurfaceConfiguration,
        drawable: bool,
        mirror: ExecutionFrameMirror,
        pending_changes: FrameChanges,
        preparer: FramePreparer,
        renderer: GpuRenderer,
        camera_center: Vec2,
        camera_height: f32,
        clear_color: wgpu::Color,
        last_draw_calls: usize,
        last_instances_drawn: usize,
        last_bytes_uploaded: usize,
        last_geometry_cache_misses: usize,
    }

    #[wasm_bindgen(js_class = ExecutionCanvasRenderer)]
    impl WasmExecutionCanvasRenderer {
        #[wasm_bindgen(js_name = create)]
        pub async fn create(
            canvas: OffscreenCanvas,
            initial_delta_json: &str,
        ) -> Result<WasmExecutionCanvasRenderer, JsValue> {
            let mut mirror = ExecutionFrameMirror::default();
            let (outcome, pending_changes) =
                mirror.apply_json(initial_delta_json).map_err(js_error)?;
            if outcome != TransportApplyOutcome::Applied || !pending_changes.is_all() {
                return Err(js_message(
                    "execution renderer must start from an applied transport snapshot",
                ));
            }
            let camera = mirror.camera();
            let width = canvas.width().max(1);
            let height = canvas.height().max(1);
            let InitializedGpu {
                instance,
                surface,
                device,
                queue,
                backend,
                config,
            } = initialize_gpu(&canvas, width, height).await?;
            let renderer = GpuRenderer::new(&device, config.format);

            let mut result = Self {
                instance,
                surface,
                device,
                queue,
                backend,
                canvas,
                config,
                drawable: true,
                mirror,
                pending_changes,
                preparer: FramePreparer::new(),
                renderer,
                camera_center: camera.center,
                camera_height: camera.height,
                clear_color: MANIM_DEFAULT_CLEAR_COLOR,
                last_draw_calls: 0,
                last_instances_drawn: 0,
                last_bytes_uploaded: 0,
                last_geometry_cache_misses: 0,
            };
            result.update_camera()?;
            Ok(result)
        }

        #[wasm_bindgen(js_name = applyDeltaJson)]
        pub fn apply_delta_json(&mut self, json: &str) -> Result<bool, JsValue> {
            if !self.pending_changes.is_empty() {
                return Err(js_message(
                    "render worker must present the applied execution delta before accepting another",
                ));
            }
            let (outcome, changes) = self.mirror.apply_json(json).map_err(js_error)?;
            match outcome {
                TransportApplyOutcome::Applied => {
                    let camera = self.mirror.camera();
                    if camera.center != self.camera_center || camera.height != self.camera_height {
                        self.camera_center = camera.center;
                        self.camera_height = camera.height;
                        self.update_camera()?;
                    }
                    self.pending_changes = changes;
                    Ok(true)
                }
                TransportApplyOutcome::DroppedStale => Ok(false),
            }
        }

        pub fn render(&mut self) -> Result<bool, JsValue> {
            if !self.drawable || self.pending_changes.is_empty() {
                return Ok(false);
            }

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
                            "GPU backend rejected the OffscreenCanvas surface texture",
                        ));
                    }
                };

            let changes = mem::take(&mut self.pending_changes);
            let frame = self
                .mirror
                .frame()
                .ok_or_else(|| js_message("execution renderer has no frame snapshot"))?;
            let prepared = self.preparer.prepare_incremental(frame, &changes);
            self.last_geometry_cache_misses = prepared.stats.geometry_cache_misses;
            let upload = self.renderer.upload(&self.device, &self.queue, &prepared);
            self.last_bytes_uploaded = upload.bytes_uploaded;

            let view = surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Noon execution render worker frame"),
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

        /// Manual camera control remains as a low-level host API. Semantic scene
        /// deltas automatically overwrite it with their shared Rust camera state.
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

        #[wasm_bindgen(js_name = rendererBackend)]
        pub fn renderer_backend(&self) -> String {
            match self.backend {
                wgpu::Backend::BrowserWebGpu => "WebGPU".to_owned(),
                wgpu::Backend::Gl => "WebGL2".to_owned(),
                other => format!("{other:?}"),
            }
        }

        pub fn time(&self) -> f64 {
            self.mirror.frame().map_or(0.0, |frame| frame.time)
        }

        #[wasm_bindgen(js_name = objectCount)]
        pub fn object_count(&self) -> usize {
            self.mirror.live_object_count()
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
    }

    impl WasmExecutionCanvasRenderer {
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
    }

    async fn initialize_gpu(
        canvas: &OffscreenCanvas,
        width: u32,
        height: u32,
    ) -> Result<InitializedGpu, JsValue> {
        let webgpu_error = if wgpu::util::is_browser_webgpu_supported().await {
            match initialize_webgpu(canvas, width, height).await {
                Ok(initialized) => return Ok(initialized),
                Err(error) => Some(error),
            }
        } else {
            None
        };

        initialize_webgl(canvas, width, height)
            .await
            .map_err(|webgl_error| {
                if let Some(webgpu_error) = webgpu_error {
                    js_message(&format!(
                        "WebGPU initialization failed: {webgpu_error}; WebGL2 fallback failed: {webgl_error}"
                    ))
                } else {
                    js_message(&format!("WebGL2 initialization failed: {webgl_error}"))
                }
            })
    }

    async fn initialize_webgpu(
        canvas: &OffscreenCanvas,
        width: u32,
        height: u32,
    ) -> Result<InitializedGpu, String> {
        let instance = create_instance(wgpu::Backends::BROWSER_WEBGPU);
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await
            .map_err(|error| error.to_string())?;
        let backend = adapter.get_info().backend;
        let (device, queue) = request_device(&adapter, backend).await?;

        // Do not claim a canvas context until WebGPU has a usable device. Once a
        // canvas is bound to WebGPU, browsers cannot reliably create WebGL2 from
        // that same canvas for fallback.
        let surface = create_surface(&instance, canvas).map_err(js_value_message)?;
        let config = default_surface_config(&surface, &adapter, width, height)?;
        surface.configure(&device, &config);
        Ok(InitializedGpu {
            instance,
            surface,
            device,
            queue,
            backend,
            config,
        })
    }

    async fn initialize_webgl(
        canvas: &OffscreenCanvas,
        width: u32,
        height: u32,
    ) -> Result<InitializedGpu, String> {
        let instance = create_instance(wgpu::Backends::GL);
        let surface = create_surface(&instance, canvas).map_err(js_value_message)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .map_err(|error| error.to_string())?;
        let backend = adapter.get_info().backend;
        let (device, queue) = request_device(&adapter, backend).await?;
        let config = default_surface_config(&surface, &adapter, width, height)?;
        surface.configure(&device, &config);
        Ok(InitializedGpu {
            instance,
            surface,
            device,
            queue,
            backend,
            config,
        })
    }

    fn create_instance(backends: wgpu::Backends) -> wgpu::Instance {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = backends;
        descriptor.display = Some(Box::new(WebDisplaySource));
        wgpu::Instance::new(descriptor)
    }

    async fn request_device(
        adapter: &wgpu::Adapter,
        backend: wgpu::Backend,
    ) -> Result<(wgpu::Device, wgpu::Queue), String> {
        let required_limits = if backend == wgpu::Backend::Gl {
            wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())
        } else {
            wgpu::Limits::default()
        };
        adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Noon execution render worker GPU device"),
                required_features: wgpu::Features::empty(),
                required_limits,
                ..Default::default()
            })
            .await
            .map_err(|error| error.to_string())
    }

    fn default_surface_config(
        surface: &wgpu::Surface<'_>,
        adapter: &wgpu::Adapter,
        width: u32,
        height: u32,
    ) -> Result<wgpu::SurfaceConfiguration, String> {
        surface
            .get_default_config(adapter, width, height)
            .ok_or_else(|| "GPU adapter cannot present to this OffscreenCanvas".to_owned())
    }

    fn create_surface(
        instance: &wgpu::Instance,
        canvas: &OffscreenCanvas,
    ) -> Result<wgpu::Surface<'static>, JsValue> {
        instance
            .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas.clone()))
            .map_err(js_error)
    }

    fn js_value_message(value: JsValue) -> String {
        value.as_string().unwrap_or_else(|| format!("{value:?}"))
    }

    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }

    fn js_message(message: &str) -> JsValue {
        JsValue::from_str(message)
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use super::MANIM_DEFAULT_CAMERA_HEIGHT;

    #[test]
    fn default_camera_matches_manim_frame_height() {
        assert_eq!(MANIM_DEFAULT_CAMERA_HEIGHT, 8.0);
    }
}
