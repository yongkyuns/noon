from pathlib import Path

# Anchored edits against e042b560. No shader or acceptance tolerance changes.
def replace(path, old, new):
    p=Path(path); s=p.read_text()
    if s.count(old)!=1: raise RuntimeError(f'{path}: expected one anchor, got {s.count(old)}')
    p.write_text(s.replace(old,new))

mod='crates/noon-render-wgpu/src/gpu/mod.rs'
replace(mod, '    camera: Camera2D,\n    viewport_size: [u32; 2],\n',
'''    camera: Camera2D,
    viewport_size: [u32; 2],
    // Logical presentation dimensions may differ from integer backing pixels.
    // This is view metadata, not an effect or semantic scene authority.
    logical_viewport_size: Vec2,
''')
replace(mod, '            viewport_size,\n            target_format,',
'''            viewport_size,
            logical_viewport_size: Vec2::new(viewport_size[0] as f32, viewport_size[1] as f32),
            target_format,''')
replace(mod, '        let viewport_size = [width.max(1), height.max(1)];\n',
'''        let viewport_size = [width.max(1), height.max(1)];
        self.logical_viewport_size = Vec2::new(viewport_size[0] as f32, viewport_size[1] as f32);
''')
replace(mod, '    fn write_camera_uniform(&self, queue: &wgpu::Queue) {',
'''    /// Set physical backing pixels together with the logical view used by the
    /// camera and pointer projection. Browser CSS bounds need not have exactly
    /// the backing aspect after borders, fractional layout and DPR rounding.
    /// Validation precedes any GPU or renderer mutation. Ordinary set_viewport
    /// restores the default one-logical-unit-per-backing-pixel mapping.
    pub fn set_viewport_with_logical_size(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        logical_size: Vec2,
    ) -> Result<(), CameraError> {
        Camera2D::new(Vec2::ZERO, logical_size)?;
        self.set_viewport(device, queue, width, height);
        self.logical_viewport_size = logical_size;
        Ok(())
    }

    fn write_camera_uniform(&self, queue: &wgpu::Queue) {''')
proj='crates/noon-render-wgpu/src/gpu/glow_filter.rs'
replace(proj, '''    ) -> Result<Self, GlowPrepareError> {
        if viewport.contains(&0)''', '''    ) -> Result<Self, GlowPrepareError> {
        Self::projected_analytic_in_view(
            geometry,
            transform,
            camera,
            viewport,
            noon_core::Vec2::new(viewport[0] as f32, viewport[1] as f32),
        )
    }

    /// Validate the uniform *logical* view separately from its raster sampling.
    /// Bounds and Gaussian support still use the exact physical output pixels.
    pub(super) fn projected_analytic_in_view(
        geometry: &noon_core::GeometryRef,
        transform: noon_core::Transform2D,
        camera: super::Camera2D,
        viewport: [u32; 2],
        logical_viewport: noon_core::Vec2,
    ) -> Result<Self, GlowPrepareError> {
        if viewport.contains(&0)''')
replace(proj, '                camera.world_size.y,\n                transform.translation.x,',
'''                camera.world_size.y,
                logical_viewport.x,
                logical_viewport.y,
                transform.translation.x,''')
replace(proj, '            || camera.world_size.y <= 0.0\n            || transform.scale.x == 0.0',
'''            || camera.world_size.y <= 0.0
            || logical_viewport.x <= 0.0
            || logical_viewport.y <= 0.0
            || transform.scale.x == 0.0''')
replace(proj, '''        let disparity = (x_pixels_per_world - y_pixels_per_world).abs()
            / x_pixels_per_world.max(y_pixels_per_world);''',
'''        let logical_x_per_world = f64::from(logical_viewport.x) / f64::from(camera.world_size.x);
        let logical_y_per_world = f64::from(logical_viewport.y) / f64::from(camera.world_size.y);
        let disparity = (logical_x_per_world - logical_y_per_world).abs()
            / logical_x_per_world.max(logical_y_per_world);''')
ag='crates/noon-render-wgpu/src/gpu/analytic_glow.rs'
replace(ag, '''        let bounds = GlowPixelBounds::projected_analytic(
            &effective.geometry(),
            transform,
            self.camera,
            self.viewport_size,
        )?;''', '''        let bounds = GlowPixelBounds::projected_analytic_in_view(
            &effective.geometry(),
            transform,
            self.camera,
            self.viewport_size,
            self.logical_viewport_size,
        )?;''')
ret='crates/noon-web/src/retained_execution_canvas.rs'
replace(ret, '''            let aspect = self.pointer_view.map_or(
                self.config.width as f32 / self.config.height as f32,
                |view| view.width / view.height,
            );''', '''            let logical = self.pointer_view.map_or(
                Vec2::new(self.config.width as f32, self.config.height as f32),
                |view| Vec2::new(view.width, view.height),
            );
            let aspect = logical.x / logical.y;''')
for web in (ret, 'crates/noon-web/src/execution_canvas.rs'):
    replace(web, '''            self.renderer.set_viewport(
                &self.device,
                &self.queue,
                self.config.width,
                self.config.height,
            );
            self.renderer.set_camera(&self.queue, camera);''', '''            self.renderer
                .set_viewport_with_logical_size(
                    &self.device,
                    &self.queue,
                    self.config.width,
                    self.config.height,
                    logical,
                )
                .map_err(js_error)?;
            self.renderer.set_camera(&self.queue, camera);''')
p=Path('crates/noon-render-wgpu/src/gpu/glow_filter/tests.rs')
p.write_text(p.read_text()+'''

#[test]
fn logical_projection_preserves_fractional_and_decorated_backing_pixels() {
    use noon_core::{GeometryRef, Transform2D, Vec2};
    // Same inputs used by the browser: CSS/pointer view and independently
    // rounded backing dimensions, including the shipped one-pixel border.
    for (logical, viewport) in [
        (Vec2::new(704.0, 396.0), [702, 394]),
        (Vec2::new(704.0, 396.0), [1404, 788]),
        (Vec2::new(513.25, 288.703125), [511, 287]),
        (Vec2::new(513.25, 288.703125), [639, 359]),
    ] {
        let camera = super::super::Camera2D::new(
            Vec2::ZERO,
            Vec2::new(8.0 * logical.x / logical.y, 8.0),
        ).unwrap();
        let bounds = GlowPixelBounds::projected_analytic_in_view(
            &GeometryRef::circle(1.0), Transform2D::IDENTITY, camera, viewport, logical,
        ).unwrap();
        let half = [
            f64::from(viewport[0]) / f64::from(camera.world_size.x),
            f64::from(viewport[1]) / f64::from(camera.world_size.y),
        ];
        assert_eq!(bounds.min, [f64::from(viewport[0]) * 0.5 - half[0], f64::from(viewport[1]) * 0.5 - half[1]]);
        assert_eq!(bounds.max, [f64::from(viewport[0]) * 0.5 + half[0], f64::from(viewport[1]) * 0.5 + half[1]]);
        // The old public physical-only contract remains strict: no arbitrary
        // nonuniform logical camera has been admitted by loosening a tolerance.
        assert_eq!(GlowPixelBounds::projected_analytic(
            &GeometryRef::circle(1.0), Transform2D::IDENTITY, camera, viewport,
        ), Err(GlowPrepareError::InvalidProjection));
    }
}

#[test]
fn logical_projection_rejects_invalid_or_nonuniform_logical_views() {
    use noon_core::{GeometryRef, Transform2D, Vec2};
    let camera = super::super::Camera2D::new(Vec2::ZERO, Vec2::new(20.0, 10.0)).unwrap();
    for logical in [Vec2::new(100.0, 100.0), Vec2::ZERO, Vec2::new(f32::NAN, 100.0), Vec2::new(200.0, f32::INFINITY)] {
        assert_eq!(GlowPixelBounds::projected_analytic_in_view(
            &GeometryRef::circle(1.0), Transform2D::IDENTITY, camera, [200,100], logical,
        ), Err(GlowPrepareError::InvalidProjection));
    }
}
''')
p=Path('crates/noon-render-wgpu/src/gpu/analytic_glow/tests.rs')
p.write_text(p.read_text()+'''

#[cfg(feature = "ci-noop")]
#[test]
fn logical_viewport_validation_is_atomic_and_physical_viewport_restores_default() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_viewport_with_logical_size(&device, &queue, 702, 394, Vec2::new(704.0, 396.0)).unwrap();
    assert_eq!(renderer.logical_viewport_size, Vec2::new(704.0,396.0));
    for invalid in [Vec2::ZERO, Vec2::new(f32::NAN,10.0), Vec2::new(10.0,-1.0)] {
        assert!(renderer.set_viewport_with_logical_size(&device, &queue, 20, 20, invalid).is_err());
        assert_eq!(renderer.viewport_size(), [702,394]);
        assert_eq!(renderer.logical_viewport_size, Vec2::new(704.0,396.0));
    }
    renderer.set_viewport(&device, &queue, 702, 394);
    assert_eq!(renderer.logical_viewport_size, Vec2::new(702.0,394.0));
    assert!(renderer.analytic_glows.is_none());
}
''')
raster='crates/noon/tests/glow_scene_raster.rs'
replace(raster, '''        let mut renderer = GpuRenderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(device, queue, SIZE[0], SIZE[1]);
        renderer.set_camera(
            queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
        );''', '''        Self::with_logical_viewport(device, queue, Vec2::new(80.0, 60.0))
    }
    fn with_logical_viewport(device: &wgpu::Device, queue: &wgpu::Queue, logical: Vec2) -> Self {
        let mut renderer = GpuRenderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport_with_logical_size(device, queue, SIZE[0], SIZE[1], logical).unwrap();
        renderer.set_camera(
            queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(6.0 * logical.x / logical.y, 6.0)).unwrap(),
        );''')
anchor='    eprintln!("PUBLIC_SCENE_GLOW_PASS frames=14 references=5 max_error={max_error} neutral_exact=true removal_exact=true reattach_exact=true");'
replace(raster,anchor,anchor+'''
    for logical in [Vec2::new(704.0,396.0), Vec2::new(513.25,288.703125)] {
        let (scene, source, animation) = self::scene(true);
        let (plain_scene, _, plain_animation) = self::scene(false);
        let mut session = scene.execution_session().unwrap();
        let mut plain = plain_scene.execution_session().unwrap();
        let segment = scene.live(&mut session).play_animation(&animation).unwrap();
        let plain_segment = plain_scene.live(&mut plain).play_animation(&plain_animation).unwrap();
        let mut surface = Surface::with_logical_viewport(&device, &queue, logical);
        let mut reference = Surface::with_logical_viewport(&device, &queue, logical);
        let mut last_plain = Vec::new();
        for time in [0.0, 0.5, 1.0] {
            scene.live(&mut session).advance_segment_to(segment,time).unwrap();
            plain_scene.live(&mut plain).advance_segment_to(plain_segment,time).unwrap();
            let actual = surface.render(&device,&queue,&mut session);
            last_plain = reference.render(&device,&queue,&mut plain);
            let oracle = expected(&last_plain,time);
            assert!(error(&actual,&oracle) <= 2, "logical viewport {logical:?}: pixel error {}", error(&actual,&oracle));
            assert!(error(&oracle,&last_plain) > 2, "missing-halo negative control");
        }
        scene.live(&mut session).complete_segment(segment).unwrap();
        scene.live(&mut session).set_glow(&source, GlowUpdate::default().intensity(0.0)).unwrap();
        assert_eq!(surface.render(&device,&queue,&mut session),last_plain);
        scene.live(&mut session).remove_glow(&source).unwrap();
        assert_eq!(surface.render(&device,&queue,&mut session),last_plain);
        eprintln!("LOGICAL_VIEWPORT_GLOW_PASS logical={logical:?} max_error<=2 neutral_exact=true removal_exact=true");
    }
''')
