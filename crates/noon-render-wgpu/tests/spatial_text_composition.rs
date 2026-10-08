use noon_compile::{lower_semantic_execution, ExecutionPatch, SemanticExecutionIndex};
use noon_core::{
    Color, CompositionTimeMap, Property, RateFunction, SemanticMutationTransaction,
    SemanticObjectRole, SemanticObjectState, SemanticPaint, SemanticProjection3D,
    SemanticSpatialCompositionDomain, SemanticStore, SemanticStyle, SemanticTransform,
    SemanticVec3, SemanticWorldTransform3D, StoredGeometry, TrackDefinition, TrackId, TrackTiming,
    TrackValues, Vec2, WorldTransformTrackEndpoint,
};
use noon_render_wgpu::text::TextDeviceMetrics;
use noon_render_wgpu::{Camera2D, FramePreparer, GpuRenderer, RetainedFramePreparer};
use noon_runtime::SceneInstance;
use noon_typst::{compile_typst_resource, TypstMode};

const WIDTH: u32 = 192;
const HEIGHT: u32 = 128;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

struct Target {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    readback: wgpu::Buffer,
}

impl Target {
    fn new(device: &wgpu::Device) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("spatial text composition target"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("spatial text composition readback"),
            size: u64::from(WIDTH * HEIGHT * 4),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            texture,
            view,
            readback,
        }
    }

    fn read(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mut encoder: wgpu::CommandEncoder,
    ) -> Vec<u8> {
        encoder.copy_texture_to_buffer(
            self.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(WIDTH * 4),
                    rows_per_image: Some(HEIGHT),
                },
            },
            wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        self.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        receiver.recv().unwrap().unwrap();
        let bytes = self.readback.slice(..).get_mapped_range().unwrap().to_vec();
        self.readback.unmap();
        bytes
    }
}

fn attach(store: &mut SemanticStore, state: SemanticObjectState) -> noon_core::SemanticNodeId {
    let id = store.insert_semantic_object(state);
    store.attach_semantic_object(id).unwrap();
    id
}

fn fitted_text(
    handle: noon_core::TextResourceHandle,
    bounds: noon_core::Rect,
    center: SemanticVec3,
    size: [f64; 2],
) -> SemanticObjectState {
    let mut state = SemanticObjectState::new(handle);
    let scale_x = size[0] / f64::from(bounds.width());
    let scale_y = size[1] / f64::from(bounds.height());
    let local_center = bounds.center();
    state.transform.translation = SemanticVec3::new(
        center.x - f64::from(local_center.x) * scale_x,
        center.y - f64::from(local_center.y) * scale_y,
        center.z,
    );
    state.transform.scale = SemanticVec3::new(scale_x, scale_y, 1.0);
    state
}

fn pixel(image: &[u8], x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * WIDTH + x) * 4) as usize;
    image[offset..offset + 4].try_into().unwrap()
}

fn non_black_pixels(image: &[u8], x0: u32, x1: u32, y0: u32, y1: u32) -> usize {
    (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| (x, y)))
        .filter(|&(x, y)| pixel(image, x, y)[..3].iter().any(|&channel| channel > 24))
        .count()
}

fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &Target,
    renderer: &mut GpuRenderer,
    retained: &mut RetainedFramePreparer,
    text_state: &mut noon_render_wgpu::RetainedTextGpuState,
    runtime: &mut SceneInstance,
) -> (Vec<u8>, noon_render_wgpu::SpatialUploadStats) {
    let publication = runtime.take_renderer_publication();
    let fixed_centers: Vec<_> = publication
        .frame()
        .objects
        .iter()
        .filter_map(|object| object.spatial.as_deref())
        .filter(|spatial| {
            spatial.composition_domain == SemanticSpatialCompositionDomain::FixedOrientation
        })
        .map(|spatial| spatial.fixed_orientation_center)
        .collect();
    assert!(
        fixed_centers.len() >= 2,
        "both fixed-orientation family rows are published"
    );
    assert!(fixed_centers.iter().all(Option::is_some));
    assert!(fixed_centers
        .iter()
        .all(|center| *center == fixed_centers[0]));
    let spatial = renderer
        .prepare_spatial(device, queue, &publication)
        .unwrap();
    let visible: Vec<_> = (0..publication.frame().objects.len()).collect();
    retained
        .prepare_transient_presentations_visible(&publication, &visible)
        .unwrap();
    let prepared = retained
        .prepare_planned_publication_visible(
            device,
            &publication,
            &visible,
            TextDeviceMetrics::uniform(24.0).unwrap(),
        )
        .unwrap();
    renderer.upload_retained(device, queue, &prepared, text_state);
    let mut encoder = device.create_command_encoder(&Default::default());
    let draw_stats = renderer
        .encode_retained(
            &mut encoder,
            &target.view,
            &prepared,
            text_state,
            wgpu::Color::BLACK,
            None,
        )
        .unwrap();
    assert!(
        draw_stats.text.draw_calls > 0,
        "FixedFrame text issues real glyph draws"
    );
    (target.read(device, queue, encoder), spatial)
}

#[test]
fn retained_world_paths_preserve_direct_edge_coverage_with_and_without_hud() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let Ok(adapter) = instance.request_adapter(&Default::default()).await else {
            eprintln!("skipping spatial edge-coverage regression: no adapter is available");
            return;
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let target = Target::new(&device);
        let mut store = SemanticStore::new();
        let mut camera = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.0 });
        camera.set_role(SemanticObjectRole::Camera3D);
        camera
            .set_camera_projection(Some(SemanticProjection3D::Orthographic {
                height: 8.0,
                near: 0.1,
                far: 30.0,
            }))
            .unwrap();
        camera.transform.translation = SemanticVec3::new(0.0, 0.0, 8.0);
        attach(&mut store, camera);

        // The subpixel diagonal makes losing multisampling observable. The HUD
        // occupies a separate region, so it cannot legitimately change this path.
        let mut line = SemanticObjectState::new(StoredGeometry::Line {
            start: Vec2::new(-3.0, -1.13),
            end: Vec2::new(3.0, 1.37),
        });
        line.transform.orientation =
            noon_core::SemanticOrientation::Spatial(noon_core::SemanticRotation3D::IDENTITY);
        line.style.fill = None;
        line.style.stroke = Some(SemanticPaint::Solid(Color::WHITE));
        line.style.stroke_width = 0.075;
        attach(&mut store, line);

        let text = compile_typst_resource("HUD", TypstMode::Markup).unwrap();
        let bounds = text.resource.bounds;
        let handle = store
            .import_text_resource(text.resource, &text.fonts, &text.geometry)
            .unwrap();
        let mut hud = fitted_text(
            handle,
            bounds,
            SemanticVec3::new(-3.0, -3.0, 0.0),
            [1.0, 0.3],
        );
        hud.set_spatial_composition_domain(SemanticSpatialCompositionDomain::FixedFrame)
            .unwrap();
        let mut index = SemanticExecutionIndex::new();
        let mut images = Vec::new();
        for (retained_pass, with_hud) in [(false, false), (true, false), (true, true)] {
            if with_hud {
                attach(&mut store, hud.clone());
            }
            let (compiled, _) = lower_semantic_execution(&store, &mut index)
                .unwrap()
                .into_parts();
            let mut runtime = SceneInstance::new(compiled);
            let publication = runtime.take_renderer_publication();
            let mut renderer = GpuRenderer::new(&device, &queue, FORMAT);
            renderer.set_viewport(&device, &queue, WIDTH, HEIGHT);
            renderer.set_camera(
                &queue,
                Camera2D::new(Vec2::ZERO, Vec2::new(12.0, 8.0)).unwrap(),
            );
            let spatial = renderer
                .prepare_spatial(&device, &queue, &publication)
                .unwrap();
            assert_eq!(
                spatial.resident_instances, 1,
                "the fixture draws a World path"
            );
            if !retained_pass {
                let mut preparer = FramePreparer::new();
                let prepared = preparer.prepare(publication.frame());
                renderer.upload(&device, &queue, &prepared);
                let mut encoder = device.create_command_encoder(&Default::default());
                renderer.encode(&mut encoder, &target.view, &prepared, wgpu::Color::BLACK);
                images.push(target.read(&device, &queue, encoder));
                continue;
            }
            let mut retained = RetainedFramePreparer::new();
            let visible: Vec<_> = (0..publication.frame().objects.len()).collect();
            retained
                .prepare_transient_presentations_visible(&publication, &visible)
                .unwrap();
            let prepared = retained
                .prepare_planned_publication_visible(
                    &device,
                    &publication,
                    &visible,
                    TextDeviceMetrics::uniform(16.0).unwrap(),
                )
                .unwrap();
            let mut text_state = renderer.create_retained_text_state(&device, &queue);
            renderer.upload_retained(&device, &queue, &prepared, &mut text_state);
            let mut encoder = device.create_command_encoder(&Default::default());
            let stats = renderer
                .encode_retained(
                    &mut encoder,
                    &target.view,
                    &prepared,
                    &text_state,
                    wgpu::Color::BLACK,
                    None,
                )
                .unwrap();
            assert_eq!(stats.text.draw_calls > 0, with_hud);
            images.push(target.read(&device, &queue, encoder));
        }
        assert!(non_black_pixels(&images[0], 40, 152, 40, 88) > 40);
        assert!(non_black_pixels(&images[2], 32, 64, 104, 120) > 5);
        for retained_image in &images[1..] {
            let changed_world_pixels = (40..88)
                .flat_map(|y| (40..152).map(move |x| (x, y)))
                .filter(|&(x, y)| pixel(&images[0], x, y) != pixel(retained_image, x, y))
                .count();
            assert_eq!(
                changed_world_pixels, 0,
                "retained World paths must preserve direct edge coverage"
            );
        }
    });
}

#[test]
fn cairo_butt_lines_preserve_fractional_and_rotated_pixel_area() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let adapter = match instance.request_adapter(&Default::default()).await {
            Ok(adapter) => adapter,
            Err(wgpu::RequestAdapterError::NotFound { .. }) => {
                eprintln!("skipping Cairo line coverage: no adapter is available");
                return;
            }
            Err(error) => panic!("Cairo line adapter request failed: {error}"),
        };
        eprintln!("Cairo line coverage adapter: {:?}", adapter.get_info());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let target = Target::new(&device);
        for authoring_width in [12.0, 6.0] {
            // Backdrops: 0 = black; 1 = Cairo line; 2 = unlit face; 3 = Cairo face;
            // 4 overlays a fixed-orientation path; 5 crosses an opaque depth plane.
            for (width, offset, angle, backdrop) in [
                (0.25, 0.0, 0.0, 0),
                (0.5, 0.5, 0.0, 0),
                (1.35, 0.0, 0.0, 0),
                (1.35, 0.25, 0.0, 0),
                (1.35, 0.5, 0.0, 0),
                (1.35, 0.75, 0.0, 0),
                (0.25, 0.5, std::f64::consts::FRAC_PI_4, 0),
                (1.35, 0.5, std::f64::consts::FRAC_PI_4, 0),
                (1.35, 0.0, 0.0, 1),
                (1.35, 0.0, 0.0, 2),
                (1.35, 0.0, 0.0, 3),
                (1.35, 0.0, 0.0, 4),
                (1.35, 0.0, 0.0, 5),
            ] {
                let mut store = SemanticStore::new();
                let mut camera = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.0 });
                camera.set_role(SemanticObjectRole::Camera3D);
                camera
                    .set_camera_projection(Some(SemanticProjection3D::Orthographic {
                        height: 8.0,
                        near: 0.1,
                        far: 30.0,
                    }))
                    .unwrap();
                camera.transform.translation = SemanticVec3::new(0.0, 0.0, 8.0);
                attach(&mut store, camera);
                // Keep the single-curve World-UP lighting response below byte
                // precision so this fixture measures coverage independently.
                let mut light = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.0 });
                light.set_role(SemanticObjectRole::PointLight3D);
                light.transform.translation.z = 1000.0;
                attach(&mut store, light);
                let mut line = SemanticObjectState::new(StoredGeometry::Line {
                    start: Vec2::new(-1.5, 0.0),
                    end: Vec2::new(1.5, 0.0),
                });
                line.transform = SemanticWorldTransform3D::new(
                    SemanticVec3::new(0.0, offset / 16.0, 0.0),
                    noon_core::SemanticRotation3D::from_axis_angle(
                        if backdrop == 5 {
                            SemanticVec3::new(0.0, 1.0, 0.0)
                        } else {
                            SemanticVec3::new(0.0, 0.0, 1.0)
                        },
                        if backdrop == 5 {
                            std::f64::consts::FRAC_PI_6
                        } else {
                            angle
                        },
                    )
                    .unwrap(),
                    SemanticVec3::new(1.0, 1.0, 1.0),
                )
                .unwrap()
                .into();
                line.style.fill = None;
                line.style.stroke = Some(SemanticPaint::Solid(Color::WHITE));
                line.style.stroke_width = width / 16.0;
                line.style.stroke_width_mode = noon_core::StrokeWidthMode::ScreenSpace;
                line.style.stroke_cap = noon_core::StrokeCap::Butt;
                line.set_spatial_material(noon_core::SemanticSpatialMaterial::CairoPath);
                line.set_cairo_path_appearance(Default::default()).unwrap();
                attach(&mut store, line.clone());
                if backdrop == 1 {
                    // A farther opaque red stroke is submitted after the nearer
                    // white line. Partial white coverage must retain its red
                    // backdrop, independently of submission order.
                    line.transform.translation.z = -0.5;
                    line.style.stroke = Some(SemanticPaint::Solid(Color::rgba(1.0, 0.0, 0.0, 1.0)));
                    line.style.stroke_width = (width + 2.0) / 16.0;
                    attach(&mut store, line);
                }
                if backdrop >= 2 {
                    let mut mesh = noon_core::MeshResource::new(
                        vec![
                            SemanticVec3::new(-2.0, -1.0, 0.0),
                            SemanticVec3::new(2.0, -1.0, 0.0),
                            SemanticVec3::new(2.0, 1.0, 0.0),
                            SemanticVec3::new(-2.0, 1.0, 0.0),
                        ],
                        None,
                        vec![0, 1, 3, 1, 2, 3],
                    )
                    .unwrap();
                    if matches!(backdrop, 3 | 4) {
                        mesh = mesh
                            .with_cairo_appearance(noon_core::CairoSurfaceAppearance {
                                p0: SemanticVec3::new(-2.0, -1.0, 0.0),
                                p6: SemanticVec3::new(2.0, 1.0, 0.0),
                                span_p3_p0: SemanticVec3::new(4.0, 0.0, 0.0),
                                span_p12_p0: SemanticVec3::new(0.0, 2.0, 0.0),
                                span_p9_p6: SemanticVec3::new(-4.0, 0.0, 0.0),
                                span_p3_p6: SemanticVec3::new(0.0, -2.0, 0.0),
                                boundary_controls: None,
                            })
                            .unwrap();
                    }
                    let handle = store.insert_geometry_mesh(mesh);
                    let mut face = SemanticObjectState::new(StoredGeometry::Resource(handle));
                    // The tilted line crosses this plane at pixel-column boundary 89.
                    // All MSAA samples in pixel 88 are nearer, and all in 89 farther.
                    face.transform.translation.z = if backdrop == 5 {
                        (7.0 / 16.0) * std::f64::consts::FRAC_PI_6.tan()
                    } else {
                        -0.5
                    };
                    face.style.fill = Some(SemanticPaint::Solid(Color::rgba(1.0, 0.0, 0.0, 1.0)));
                    face.style.stroke = None;
                    if matches!(backdrop, 3 | 4) {
                        face.set_spatial_material(noon_core::SemanticSpatialMaterial::CairoSurface);
                    }
                    attach(&mut store, face);
                }
                if backdrop == 4 {
                    let mut hud = SemanticObjectState::new(StoredGeometry::Rectangle {
                        size: Vec2::new(1.0, 1.0),
                    });
                    hud.transform.orientation = noon_core::SemanticOrientation::Spatial(
                        noon_core::SemanticRotation3D::IDENTITY,
                    );
                    hud.style.fill = Some(SemanticPaint::Solid(Color::rgba(0.0, 1.0, 0.0, 1.0)));
                    hud.style.stroke = None;
                    hud.set_spatial_composition_domain(
                        SemanticSpatialCompositionDomain::FixedOrientation,
                    )
                    .unwrap();
                    attach(&mut store, hud);
                }
                let (compiled, _) =
                    lower_semantic_execution(&store, &mut SemanticExecutionIndex::new())
                        .unwrap()
                        .into_parts();
                let mut runtime = SceneInstance::new(compiled);
                let mut renderer = GpuRenderer::new(&device, &queue, FORMAT);
                renderer.set_viewport(&device, &queue, WIDTH, HEIGHT);
                renderer.set_camera(
                    &queue,
                    Camera2D::new(Vec2::ZERO, Vec2::new(authoring_width, 8.0)).unwrap(),
                );
                let mut retained = RetainedFramePreparer::new();
                let mut text_state = renderer.create_retained_text_state(&device, &queue);
                let mut previous = None;
                for pass in 0..2 {
                    let publication = runtime.take_renderer_publication();
                    let spatial = renderer
                        .prepare_spatial(&device, &queue, &publication)
                        .unwrap();
                    let visible: Vec<_> = (0..publication.frame().objects.len()).collect();
                    retained
                        .prepare_transient_presentations_visible(&publication, &visible)
                        .unwrap();
                    let prepared = retained
                        .prepare_planned_publication_visible(
                            &device,
                            &publication,
                            &visible,
                            TextDeviceMetrics::uniform(16.0).unwrap(),
                        )
                        .unwrap();
                    renderer.upload_retained(&device, &queue, &prepared, &mut text_state);
                    let mut encoder = device.create_command_encoder(&Default::default());
                    renderer
                        .encode_retained(
                            &mut encoder,
                            &target.view,
                            &prepared,
                            &text_state,
                            wgpu::Color::BLACK,
                            None,
                        )
                        .unwrap();
                    let image = target.read(&device, &queue, encoder);
                    let area: f64 = image
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|p| f64::from(p[1]) / 255.0)
                        .sum();
                    // Doubling the authoring frame's horizontal pixel scale makes
                    // a 45-degree stroke's perpendicular width sqrt(2.5) larger.
                    let width_factor = if authoring_width == 6.0 && angle != 0.0 {
                        2.5_f64.sqrt()
                    } else {
                        1.0
                    };
                    let expected = 48.0 * width * width_factor;
                    eprintln!("Cairo line frame_width={authoring_width} width={width} offset={offset} angle={angle} backdrop={backdrop} area={area} expected={expected}");
                    // Cairo face lighting contributes green to the backdrop.
                    // The other cases isolate the white line's pixel area.
                    if backdrop < 3 {
                        assert!((area - expected).abs() < 1.0, "{area} versus {expected}");
                    }
                    if width == 1.35 && offset == 0.0 && angle == 0.0 && backdrop < 3 {
                        assert!((i16::from(pixel(&image, 96, 63)[1]) - 172).abs() <= 1);
                        assert!((i16::from(pixel(&image, 96, 64)[1]) - 172).abs() <= 1);
                    }
                    if backdrop != 0 && backdrop < 4 {
                        assert_eq!(pixel(&image, 96, 63)[0], 255, "backdrop survives the edge");
                    }
                    if backdrop == 3 {
                        let base = i16::from(pixel(&image, 96, 60)[1]);
                        let expected = 172 + base * 83 / 255;
                        assert!((i16::from(pixel(&image, 96, 63)[1]) - expected).abs() <= 1);
                    }
                    if backdrop == 4 {
                        assert_eq!(
                            pixel(&image, 96, 63),
                            [0, 255, 0, 255],
                            "fixed-orientation painter content remains above world draws"
                        );
                    }
                    if backdrop == 5 {
                        let near = pixel(&image, 88, 63);
                        let far = pixel(&image, 89, 63);
                        eprintln!("Cairo sloped depth near={near:?} far={far:?}");
                        assert!(
                            (i16::from(near[1]) - 172).abs() <= 1,
                            "near stroke remains visible: {near:?}"
                        );
                        assert_eq!(far, [255, 0, 0, 255], "far stroke is occluded");
                    }
                    if pass == 1 {
                        assert_eq!(
                            spatial.bytes_uploaded(),
                            0,
                            "clean frame reuses all resources"
                        );
                        assert_eq!(spatial.rows_visited, 0);
                        assert_eq!(previous.as_ref(), Some(&image));
                    }
                    previous = Some(image);
                }
            }
        }
    });
}

#[test]
fn retained_spatial_paths_and_text_share_camera_domains_and_cached_geometry() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let Ok(adapter) = instance.request_adapter(&Default::default()).await else {
            eprintln!(
                "skipping spatial text composition GPU qualification: no adapter is available"
            );
            return;
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let target = Target::new(&device);

        let mut store = SemanticStore::new();
        let markup =
            compile_typst_resource("#text(fill: red)[World label]", TypstMode::Markup).unwrap();
        let markup_bounds = markup.resource.bounds;
        let markup_handle = store
            .import_text_resource(markup.resource, &markup.fonts, &markup.geometry)
            .unwrap();
        let math = compile_typst_resource("frac(x, 2)", TypstMode::Math).unwrap();
        assert!(
            !math.resource.vector_items.is_empty(),
            "MathTypst fixture exercises vector decorations"
        );
        let math_handle = store
            .import_text_resource(math.resource, &math.fonts, &math.geometry)
            .unwrap();

        let mut camera = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.0 });
        camera.set_role(SemanticObjectRole::Camera3D);
        camera
            .set_camera_projection(Some(SemanticProjection3D::Orthographic {
                height: 8.0,
                near: 0.1,
                far: 30.0,
            }))
            .unwrap();
        camera.transform.translation = SemanticVec3::new(0.0, 0.0, 8.0);
        let camera_id = attach(&mut store, camera);

        // An opaque spatial vector path anchors the scene. Text glyph outlines
        // and MathTypst vector decorations use this same retained spatial pass.
        let path = noon_core::VectorPath::new()
            .move_to(Vec2::new(-3.0, -3.0))
            .line_to(Vec2::new(3.0, -3.0))
            .line_to(Vec2::new(3.0, 3.0))
            .line_to(Vec2::new(-3.0, 3.0))
            .close();
        let geometry = store.insert_geometry_path(path).unwrap();
        let mut path_state = SemanticObjectState::new(StoredGeometry::Resource(geometry));
        path_state.style = SemanticStyle {
            fill: Some(SemanticPaint::Solid(Color::rgba(0.08, 0.12, 0.22, 1.0))),
            fill_opacity: 1.0,
            stroke: None,
            stroke_opacity: 1.0,
            stroke_width: 0.0,
            object_opacity: 1.0,
            ..SemanticStyle::default()
        };
        path_state.transform.translation = SemanticVec3::new(0.0, 0.0, 0.0);
        path_state.transform.orientation =
            noon_core::SemanticOrientation::Spatial(noon_core::SemanticRotation3D::IDENTITY);
        attach(&mut store, path_state);

        // The later, farther copy must not cover the first opaque path. This
        // exercises the shared spatial depth attachment with vector geometry.
        let mut far_path = SemanticObjectState::new(StoredGeometry::Resource(geometry));
        far_path.style = SemanticStyle {
            fill: Some(SemanticPaint::Solid(Color::rgba(0.0, 0.0, 1.0, 1.0))),
            fill_opacity: 1.0,
            stroke: None,
            stroke_opacity: 1.0,
            stroke_width: 0.0,
            object_opacity: 1.0,
            ..SemanticStyle::default()
        };
        far_path.transform.translation = SemanticVec3::new(0.0, 0.0, -1.0);
        far_path.transform.orientation =
            noon_core::SemanticOrientation::Spatial(noon_core::SemanticRotation3D::IDENTITY);
        far_path.set_z_index(10.0);
        attach(&mut store, far_path);

        let mut world_label = SemanticObjectState::new(markup_handle);
        world_label.transform = SemanticTransform {
            translation: SemanticVec3::new(-3.2, 2.6, 0.0),
            orientation: noon_core::SemanticOrientation::Spatial(
                noon_core::SemanticRotation3D::IDENTITY,
            ),
            ..SemanticTransform::default()
        };
        attach(&mut store, world_label);

        let mut math_label = SemanticObjectState::new(math_handle);
        math_label.transform = SemanticTransform {
            translation: SemanticVec3::new(2.2, 2.6, 0.0),
            orientation: noon_core::SemanticOrientation::Spatial(
                noon_core::SemanticRotation3D::IDENTITY,
            ),
            ..SemanticTransform::default()
        };
        attach(&mut store, math_label);

        // Two descendants share one FixedOrientation bounds center. The runtime
        // publishes that exact f64 center to the renderer for each row.
        let family = store.insert_family();
        store.attach_to_scene(family).unwrap();
        let mut left = fitted_text(
            markup_handle,
            markup_bounds,
            SemanticVec3::new(-1.0, -2.0, 0.3),
            [1.2, 0.24],
        );
        left.style.object_opacity = 0.5;
        let left_id = store.insert_semantic_object(left);
        store.add_semantic_family_member(family, left_id).unwrap();
        let mut right = fitted_text(
            markup_handle,
            markup_bounds,
            SemanticVec3::new(1.0, -2.0, -0.3),
            [1.2, 0.24],
        );
        right.style.object_opacity = 0.5;
        let right_id = store.insert_semantic_object(right);
        store.add_semantic_family_member(family, right_id).unwrap();
        let mut domain = SemanticMutationTransaction::new();
        for child in [left_id, right_id] {
            domain.set_spatial_composition_domain_with_anchor(
                child,
                SemanticSpatialCompositionDomain::FixedOrientation,
                Some(family),
            );
        }
        domain.apply(&mut store).unwrap();

        // A real FixedFrame label is retained in the existing planar pass.
        // Keep its measured bounds outside the spatial paths, so comparing
        // this region tests the HUD rather than the changing world behind it.
        let mut hud = fitted_text(
            markup_handle,
            markup_bounds,
            SemanticVec3::new(-3.4, -3.4, 0.0),
            [1.1, 0.22],
        );
        hud.set_spatial_composition_domain(SemanticSpatialCompositionDomain::FixedFrame)
            .unwrap();
        attach(&mut store, hud);

        let mut index = SemanticExecutionIndex::new();
        let (mut compiled, _) = lower_semantic_execution(&store, &mut index)
            .unwrap()
            .into_parts();
        let camera_object = index.execution_object_id(camera_id).unwrap();
        let camera_from = SemanticWorldTransform3D::new(
            SemanticVec3::new(0.0, 0.0, 8.0),
            noon_core::SemanticRotation3D::IDENTITY,
            SemanticVec3::new(1.0, 1.0, 1.0),
        )
        .unwrap();
        let camera_to = SemanticWorldTransform3D::new(
            SemanticVec3::new(0.45, 0.15, 8.0),
            noon_core::SemanticRotation3D::IDENTITY,
            SemanticVec3::new(1.0, 1.0, 1.0),
        )
        .unwrap();
        compiled
            .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
                id: TrackId::new(0),
                object: camera_object,
                property: Property::WorldTransform,
                values: TrackValues::WorldTransform {
                    from: WorldTransformTrackEndpoint::from_world(camera_from),
                    to: WorldTransformTrackEndpoint::from_world(camera_to),
                },
                timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
                time_map: CompositionTimeMap::identity(),
            }))
            .unwrap();
        let mut runtime = SceneInstance::new(compiled);

        let mut renderer = GpuRenderer::new(&device, &queue, FORMAT);
        renderer.set_viewport(&device, &queue, WIDTH, HEIGHT);
        renderer.set_camera(
            &queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 8.0)).unwrap(),
        );
        let mut retained = RetainedFramePreparer::new();
        let mut text_state = renderer.create_retained_text_state(&device, &queue);
        let (before, initial_upload) = render(
            &device,
            &queue,
            &target,
            &mut renderer,
            &mut retained,
            &mut text_state,
            &mut runtime,
        );
        assert!(
            non_black_pixels(&before, 0, WIDTH, 0, HEIGHT) > 400,
            "world paths and spatial labels are visible"
        );
        assert!(
            non_black_pixels(&before, 8, 80, 5, 40) > 10,
            "world Typst glyph label is visible"
        );
        assert!(
            non_black_pixels(&before, 120, 188, 5, 40) > 10,
            "MathTypst glyph/vector label is visible"
        );
        assert!(
            non_black_pixels(&before, 1, 30, 115, 123) > 5,
            "FixedFrame label uses the retained planar pass"
        );
        let center = pixel(&before, WIDTH / 2, HEIGHT / 2);
        assert!(
            center[2] > center[0] && center[2] < 100,
            "near opaque path wins against the later far blue path: {center:?}"
        );
        let blended_fixed = (42..HEIGHT)
            .flat_map(|y| (42..150).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let pixel = pixel(&before, x, y);
                pixel[0] > 40 && pixel[0] < 230 && pixel[0] > pixel[1].saturating_mul(2)
            })
            .count();
        assert!(
            blended_fixed > 8,
            "semi-transparent FixedOrientation glyphs blend over opaque world paths"
        );
        assert!(
            initial_upload.geometry_bytes > 0,
            "spatial path and glyph topology is uploaded once"
        );

        runtime.advance_to(0.5).unwrap();
        let (after, camera_upload) = render(
            &device,
            &queue,
            &target,
            &mut renderer,
            &mut retained,
            &mut text_state,
            &mut runtime,
        );
        assert_eq!(
            camera_upload.geometry_bytes, 0,
            "camera-only motion reuses vector and glyph topology"
        );
        assert_eq!(
            camera_upload.instance_bytes, 0,
            "camera-only motion changes only the compact camera uniform"
        );
        assert_eq!(
            camera_upload.camera_bytes, 64,
            "camera motion updates the existing 3D camera matrix"
        );
        assert_ne!(before, after, "world content responds to camera movement");
        assert_eq!(
            (115..123)
                .flat_map(|y| (1..30).map(move |x| (x, y)))
                .filter(|&(x, y)| pixel(&before, x, y) != pixel(&after, x, y))
                .count(),
            0,
            "FixedFrame label pixels remain camera-independent",
        );
    });
}
