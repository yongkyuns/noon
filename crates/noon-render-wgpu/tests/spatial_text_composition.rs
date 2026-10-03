use noon_compile::{lower_semantic_execution, ExecutionPatch, SemanticExecutionIndex};
use noon_core::{
    Color, CompositionTimeMap, Property, RateFunction, SemanticMutationTransaction,
    SemanticObjectRole, SemanticObjectState, SemanticPaint, SemanticProjection3D,
    SemanticSpatialCompositionDomain, SemanticStore, SemanticStyle, SemanticTransform,
    SemanticVec3, SemanticWorldTransform3D, StoredGeometry, TrackDefinition, TrackId, TrackTiming,
    TrackValues, Vec2, WorldTransformTrackEndpoint,
};
use noon_render_wgpu::text::TextDeviceMetrics;
use noon_render_wgpu::{Camera2D, GpuRenderer, RetainedFramePreparer};
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
        let bytes = self.readback.slice(..).get_mapped_range().to_vec();
        self.readback.unmap();
        bytes
    }
}

fn attach(store: &mut SemanticStore, state: SemanticObjectState) -> noon_core::SemanticNodeId {
    let id = store.insert_semantic_object(state);
    store.attach_semantic_object(id).unwrap();
    id
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
    renderer
        .encode_retained(
            &mut encoder,
            &target.view,
            &prepared,
            text_state,
            wgpu::Color::BLACK,
            None,
        )
        .unwrap();
    (target.read(device, queue, encoder), spatial)
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
        far_path.set_z_index(10.0);
        attach(&mut store, far_path);

        let mut world_label = SemanticObjectState::new(markup_handle);
        world_label.transform = SemanticTransform {
            translation: SemanticVec3::new(-3.2, 2.6, 0.0),
            ..SemanticTransform::default()
        };
        attach(&mut store, world_label);

        let mut math_label = SemanticObjectState::new(math_handle);
        math_label.transform = SemanticTransform {
            translation: SemanticVec3::new(2.2, 2.6, 0.0),
            ..SemanticTransform::default()
        };
        attach(&mut store, math_label);

        // Two descendants share one FixedOrientation bounds center. The runtime
        // publishes that exact f64 center to the renderer for each row.
        let family = store.insert_family();
        store.attach_to_scene(family).unwrap();
        let mut left = SemanticObjectState::new(markup_handle);
        left.transform.translation = SemanticVec3::new(-1.0, -2.0, 0.3);
        left.style.object_opacity = 0.5;
        let left_id = store.insert_semantic_object(left);
        store.add_semantic_family_member(family, left_id).unwrap();
        let mut right = SemanticObjectState::new(markup_handle);
        right.transform.translation = SemanticVec3::new(1.0, -2.0, -0.3);
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
        let mut hud = SemanticObjectState::new(markup_handle);
        hud.transform.translation = SemanticVec3::new(-3.4, -3.4, 0.0);
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
            non_black_pixels(&before, 10, 90, 92, 126) > 10,
            "FixedFrame label uses the retained planar pass"
        );
        let center = pixel(&before, WIDTH / 2, HEIGHT / 2);
        assert!(
            center[2] > center[0] && center[2] < 100,
            "near opaque path wins against the later far blue path: {center:?}"
        );
        let blended_fixed = (42..150)
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
            (0..HEIGHT)
                .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
                .filter(|&(x, y)| x > 0 && x < 86 && y > 92)
                .filter(|&(x, y)| pixel(&before, x, y) != pixel(&after, x, y))
                .count(),
            0,
            "FixedFrame label pixels remain camera-independent",
        );
    });
}
