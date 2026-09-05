#[cfg(target_arch = "wasm32")]
mod wasm {
    use noon::{MathTypst, RetainedScene, Typst};
    use noon_core::Vec2;
    use noon_render_wgpu::{Camera2D, GpuRenderer, RetainedFramePreparer, RetainedTextGpuState};
    use noon_runtime::SceneInstance;
    use noon_text_render_wgpu::TextDeviceMetrics;
    use wasm_bindgen::prelude::*;
    use web_sys::OffscreenCanvas;

    const DEFAULT_CAMERA_HEIGHT: f32 = 8.0;
    const CLEAR_COLOR: wgpu::Color = wgpu::Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    #[derive(Debug)]
    struct WebDisplaySource;

    impl wgpu::rwh::HasDisplayHandle for WebDisplaySource {
        fn display_handle(&self) -> Result<wgpu::rwh::DisplayHandle<'_>, wgpu::rwh::HandleError> {
            Ok(wgpu::rwh::DisplayHandle::web())
        }
    }

    /// End-to-end browser proof for the retained Typst path.
    ///
    /// Source is compiled once while constructing the renderer. Every render then
    /// consumes only the retained scene/runtime/resources and the mixed GPU painter
    /// stream; Python/SVG/full-frame rerendering is not involved.
    #[wasm_bindgen(js_name = RetainedTypstCanvasRenderer)]
    pub struct WasmRetainedTypstCanvasRenderer {
        _instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        canvas: OffscreenCanvas,
        config: wgpu::SurfaceConfiguration,
        scene: RetainedScene,
        runtime: SceneInstance,
        preparer: RetainedFramePreparer,
        renderer: GpuRenderer,
        text_gpu: RetainedTextGpuState,
        camera_height: f32,
        last_draw_calls: usize,
        last_instances_drawn: usize,
        last_bytes_uploaded: usize,
    }

    #[wasm_bindgen(js_class = RetainedTypstCanvasRenderer)]
    impl WasmRetainedTypstCanvasRenderer {
        #[wasm_bindgen(js_name = create)]
        pub async fn create(
            canvas: OffscreenCanvas,
            typst_source: &str,
            math_typst_source: &str,
        ) -> Result<WasmRetainedTypstCanvasRenderer, JsValue> {
            let mut scene = RetainedScene::new();
            if !typst_source.is_empty() {
                scene
                    .add_typst(
                        Typst::new(typst_source)
                            .with_font_size(64.0)
                            .move_to(Vec2::new(0.0, 1.15)),
                    )
                    .map_err(js_error)?;
            }
            if !math_typst_source.is_empty() {
                scene
                    .add_math_typst(
                        MathTypst::new(math_typst_source)
                            .with_font_size(72.0)
                            .move_to(Vec2::new(0.0, -1.0)),
                    )
                    .map_err(js_error)?;
            }
            Self::from_scene(canvas, scene).await
        }

        /// Construct exactly one centered retained Typst object. This is used by
        /// raster-differential fixtures so the public font-size transform and
        /// source-language identity are tested without demo-specific positioning.
        #[wasm_bindgen(js_name = createSingle)]
        pub async fn create_single(
            canvas: OffscreenCanvas,
            source: &str,
            math: bool,
            font_size: f32,
        ) -> Result<WasmRetainedTypstCanvasRenderer, JsValue> {
            let mut scene = RetainedScene::new();
            if math {
                scene
                    .add_math_typst(MathTypst::new(source).with_font_size(font_size))
                    .map_err(js_error)?;
            } else {
                scene
                    .add_typst(Typst::new(source).with_font_size(font_size))
                    .map_err(js_error)?;
            }
            Self::from_scene(canvas, scene).await
        }

        pub fn render(&mut self) -> Result<(), JsValue> {
            let surface_texture = match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(texture)
                | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    return Ok(())
                }
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                    self.surface.configure(&self.device, &self.config);
                    return Ok(());
                }
                wgpu::CurrentSurfaceTexture::Validation => {
                    return Err(js_message("GPU rejected retained Typst surface texture"))
                }
            };

            let camera = self.renderer.camera();
            let metrics = TextDeviceMetrics::new(Vec2::new(
                self.config.width as f32 / camera.world_size.x,
                self.config.height as f32 / camera.world_size.y,
            ))
            .map_err(js_error)?;
            let changes = self.runtime.take_frame_changes();
            let prepared = self
                .preparer
                .prepare_with_changes(
                    &self.device,
                    &self.queue,
                    self.runtime.frame(),
                    &changes,
                    self.scene.texts(),
                    self.scene.fonts(),
                    self.scene.geometries(),
                    metrics,
                )
                .map_err(js_error)?;
            let upload = self.renderer.upload_retained(
                &self.device,
                &self.queue,
                &prepared,
                &mut self.text_gpu,
            );
            self.last_bytes_uploaded = upload
                .geometry
                .bytes_uploaded
                .saturating_add(upload.text.bytes_uploaded);

            let view = surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Noon retained Typst demo frame"),
                });
            let draw = self
                .renderer
                .encode_retained(&mut encoder, &view, &prepared, &self.text_gpu, CLEAR_COLOR)
                .map_err(js_error)?;
            self.queue.submit(Some(encoder.finish()));
            self.queue.present(surface_texture);
            self.last_draw_calls = draw
                .geometry
                .draw_calls
                .saturating_add(draw.text.draw_calls);
            self.last_instances_drawn = draw
                .geometry
                .instances_drawn
                .saturating_add(draw.text.instances_drawn);
            Ok(())
        }

        pub fn resize(&mut self, width: u32, height: u32) -> Result<(), JsValue> {
            if width == 0 || height == 0 {
                return Ok(());
            }
            self.canvas.set_width(width);
            self.canvas.set_height(height);
            if self.config.width != width || self.config.height != height {
                self.config.width = width;
                self.config.height = height;
                self.surface.configure(&self.device, &self.config);
                set_camera(
                    &mut self.renderer,
                    &self.device,
                    &self.queue,
                    width,
                    height,
                    self.camera_height,
                )?;
            }
            Ok(())
        }

        #[wasm_bindgen(js_name = objectCount)]
        pub fn object_count(&self) -> usize {
            self.scene.objects().len()
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

    impl WasmRetainedTypstCanvasRenderer {
        async fn from_scene(
            canvas: OffscreenCanvas,
            scene: RetainedScene,
        ) -> Result<WasmRetainedTypstCanvasRenderer, JsValue> {
            if scene.objects().is_empty() {
                return Err(js_message(
                    "retained Typst renderer requires at least one text object",
                ));
            }
            let compiled = scene.compile().map_err(js_error)?;
            let runtime = SceneInstance::new(compiled);

            let mut instance_descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
            instance_descriptor.backends = wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL;
            instance_descriptor.display = Some(Box::new(WebDisplaySource));
            let instance =
                wgpu::util::new_instance_with_webgpu_detection(instance_descriptor).await;
            let surface = instance
                .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas.clone()))
                .map_err(js_error)?;
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    force_fallback_adapter: false,
                    compatible_surface: Some(&surface),
                    apply_limit_buckets: false,
                })
                .await
                .map_err(js_error)?;
            let backend = adapter.get_info().backend;
            let required_limits = if backend == wgpu::Backend::Gl {
                wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())
            } else {
                wgpu::Limits::default()
            };
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("Noon retained Typst demo GPU device"),
                    required_features: wgpu::Features::empty(),
                    required_limits,
                    ..Default::default()
                })
                .await
                .map_err(js_error)?;

            let width = canvas.width().max(1);
            let height = canvas.height().max(1);
            let config = surface
                .get_default_config(&adapter, width, height)
                .ok_or_else(|| js_message("GPU adapter cannot present retained Typst demo"))?;
            surface.configure(&device, &config);

            let mut renderer = GpuRenderer::new(&device, config.format);
            let camera_height = DEFAULT_CAMERA_HEIGHT;
            set_camera(&mut renderer, &device, &queue, width, height, camera_height)?;
            let text_gpu = renderer.create_retained_text_state(&device, &queue);

            Ok(Self {
                _instance: instance,
                surface,
                device,
                queue,
                canvas,
                config,
                scene,
                runtime,
                preparer: RetainedFramePreparer::new(),
                renderer,
                text_gpu,
                camera_height,
                last_draw_calls: 0,
                last_instances_drawn: 0,
                last_bytes_uploaded: 0,
            })
        }
    }

    fn set_camera(
        renderer: &mut GpuRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        camera_height: f32,
    ) -> Result<(), JsValue> {
        let aspect = width as f32 / height as f32;
        let camera = Camera2D::new(Vec2::ZERO, Vec2::new(camera_height * aspect, camera_height))
            .map_err(js_error)?;
        renderer.set_viewport(device, queue, width, height);
        renderer.set_camera(queue, camera);
        Ok(())
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
