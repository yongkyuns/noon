use noon_compile::{
    lower_semantic_execution, lower_semantic_execution_root,
    lower_semantic_execution_root_with_animation_root, ExecutionPatch, SemanticExecutionIndex,
};
use noon_core::{
    AnimationOptions, Color, CompositionTimeMap, MeshResource, Property, RateFunction,
    SemanticAnimationCompositionKind, SemanticMutationTransaction, SemanticObjectRole,
    SemanticObjectState, SemanticObjectTrackProperty, SemanticObjectTrackValues, SemanticPaint,
    SemanticProjection3D, SemanticSpatialMaterial, SemanticStore, SemanticStyle, SemanticTransform,
    SemanticVec3, SemanticWorldTransform3D, StoredGeometry, TrackDefinition, TrackId, TrackTiming,
    TrackValues, WorldTransformTrackEndpoint,
};
use noon_render_wgpu::text::TextDeviceMetrics;
use noon_render_wgpu::{
    Camera2D, FramePreparer, GpuRenderer, RetainedFramePreparer, SpatialPrepareError,
};
use noon_runtime::SceneInstance;

const WIDTH: u32 = 128;
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
            label: Some("spatial mesh qualification target"),
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
            label: Some("spatial mesh readback"),
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
                sender.send(result).unwrap();
            });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        receiver.recv().unwrap().unwrap();
        let bytes = self.readback.slice(..).get_mapped_range().unwrap().to_vec();
        self.readback.unmap();
        bytes
    }
}

fn mesh() -> MeshResource {
    MeshResource::new(
        vec![
            SemanticVec3::new(-1.0, -1.0, 0.0),
            SemanticVec3::new(1.0, -1.0, 0.0),
            SemanticVec3::new(0.0, 1.0, 0.0),
        ],
        None,
        vec![0, 1, 2],
    )
    .unwrap()
}

fn projection(kind: bool) -> SemanticProjection3D {
    if kind {
        SemanticProjection3D::Orthographic {
            height: 6.0,
            near: 0.1,
            far: 30.0,
        }
    } else {
        SemanticProjection3D::Perspective {
            vertical_fov_radians: 1.0,
            near: 0.1,
            far: 30.0,
        }
    }
}

fn opaque_style(color: Color) -> SemanticStyle {
    SemanticStyle {
        fill: Some(SemanticPaint::Solid(color)),
        fill_opacity: 1.0,
        stroke: None,
        stroke_opacity: 1.0,
        stroke_width: 0.0,
        object_opacity: 1.0,
        ..SemanticStyle::default()
    }
}

fn attach(store: &mut SemanticStore, state: SemanticObjectState) -> noon_core::SemanticNodeId {
    let node = store.insert_semantic_object(state);
    store.attach_semantic_object(node).unwrap();
    node
}

fn build_scene(
    with_overlay: bool,
    projection_kind: bool,
    huge_vertex: bool,
) -> (
    SceneInstance,
    SemanticExecutionIndex,
    noon_core::SemanticNodeId,
    noon_core::SemanticNodeId,
    noon_core::SemanticNodeId,
) {
    let mut store = SemanticStore::new();
    let payload = if huge_vertex {
        MeshResource::new(
            vec![
                SemanticVec3::new(f64::MAX, 0.0, 0.0),
                SemanticVec3::new(1.0, 0.0, 0.0),
                SemanticVec3::new(0.0, 1.0, 0.0),
            ],
            None,
            vec![0, 1, 2],
        )
        .unwrap()
    } else {
        mesh()
    };
    let handle = store.insert_geometry_mesh(payload);

    let mut camera = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.0 });
    camera.set_role(SemanticObjectRole::Camera3D);
    camera
        .set_camera_projection(Some(projection(projection_kind)))
        .unwrap();
    camera.transform = SemanticTransform {
        translation: SemanticVec3::new(0.0, 0.0, 5.0),
        ..SemanticTransform::default()
    };
    let camera_id = attach(&mut store, camera);

    // The near red mesh is deliberately earlier in painter order than the far
    // blue mesh. Depth must decide the winner when the far mesh is drawn last.
    let mut near = SemanticObjectState::new(StoredGeometry::Resource(handle));
    near.style = opaque_style(Color::RED);
    near.transform.translation = SemanticVec3::new(0.0, 0.0, 1.0);
    near.set_z_index(-5.0);
    let near_id = attach(&mut store, near);

    let mut far = SemanticObjectState::new(StoredGeometry::Resource(handle));
    far.style = opaque_style(Color::BLUE);
    far.transform.translation = SemanticVec3::ZERO;
    far.set_z_index(5.0);
    let far_id = attach(&mut store, far);

    if with_overlay {
        let mut overlay = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.42 });
        overlay.transform.translation = SemanticVec3::new(4.0, 0.0, 0.0);
        overlay.style = opaque_style(Color::GREEN);
        attach(&mut store, overlay);
    }

    let mut index = SemanticExecutionIndex::new();
    let (mut compiled, _) = lower_semantic_execution(&store, &mut index)
        .unwrap()
        .into_parts();
    let near_object = index.execution_object_id(near_id).unwrap();
    let camera_object = index.execution_object_id(camera_id).unwrap();
    let start_near = SemanticWorldTransform3D::new(
        SemanticVec3::new(0.0, 0.0, 1.0),
        noon_core::SemanticRotation3D::IDENTITY,
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .unwrap();
    let end_near = SemanticWorldTransform3D::new(
        SemanticVec3::new(0.0, 0.0, -1.0),
        noon_core::SemanticRotation3D::IDENTITY,
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .unwrap();
    let start_camera = SemanticWorldTransform3D::new(
        SemanticVec3::new(0.0, 0.0, 5.0),
        noon_core::SemanticRotation3D::IDENTITY,
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .unwrap();
    let end_camera = SemanticWorldTransform3D::new(
        SemanticVec3::new(0.25, 0.0, 5.0),
        noon_core::SemanticRotation3D::IDENTITY,
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .unwrap();
    for (id, object, from, to) in [
        (0, near_object, start_near, end_near),
        (1, camera_object, start_camera, end_camera),
    ] {
        compiled
            .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
                id: TrackId::new(id),
                object,
                property: Property::WorldTransform,
                values: TrackValues::WorldTransform {
                    from: WorldTransformTrackEndpoint::from_world(from),
                    to: WorldTransformTrackEndpoint::from_world(to),
                },
                timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
                time_map: CompositionTimeMap::identity(),
            }))
            .unwrap();
    }
    (
        SceneInstance::new(compiled),
        index,
        near_id,
        camera_id,
        far_id,
    )
}

fn build_many_mesh_scene(count: usize) -> SceneInstance {
    let mut store = SemanticStore::new();
    let handle = store.insert_geometry_mesh(mesh());
    let mut camera = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.0 });
    camera.set_role(SemanticObjectRole::Camera3D);
    camera
        .set_camera_projection(Some(projection(true)))
        .unwrap();
    camera.transform.translation = SemanticVec3::new(0.0, 0.0, 5.0);
    attach(&mut store, camera);

    let mut animated_node = None;
    for index in 0..count {
        let mut object = SemanticObjectState::new(StoredGeometry::Resource(handle));
        object.style = opaque_style(Color::RED);
        let column = index % 30;
        let row = index / 30;
        object.transform.translation =
            SemanticVec3::new(column as f64 * 0.15 - 2.175, row as f64 * 0.15 - 1.425, 0.0);
        let node = attach(&mut store, object);
        if index == 0 {
            animated_node = Some(node);
        }
    }

    let mut index = SemanticExecutionIndex::new();
    let (mut compiled, _) = lower_semantic_execution(&store, &mut index)
        .unwrap()
        .into_parts();
    let object = index
        .execution_object_id(animated_node.expect("non-empty mesh fixture"))
        .unwrap();
    let from = SemanticWorldTransform3D::new(
        SemanticVec3::new(-2.175, -1.425, 0.0),
        noon_core::SemanticRotation3D::IDENTITY,
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .unwrap();
    let to = SemanticWorldTransform3D::new(
        SemanticVec3::new(-2.075, -1.425, 0.0),
        noon_core::SemanticRotation3D::IDENTITY,
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .unwrap();
    compiled
        .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
            id: TrackId::new(0),
            object,
            property: Property::WorldTransform,
            values: TrackValues::WorldTransform {
                from: WorldTransformTrackEndpoint::from_world(from),
                to: WorldTransformTrackEndpoint::from_world(to),
            },
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        }))
        .unwrap();
    SceneInstance::new(compiled)
}

fn build_lighting_scene(
    material: SemanticSpatialMaterial,
    light_count: usize,
    animate_light: bool,
) -> SceneInstance {
    build_lighting_scene_with_normal(
        material,
        light_count,
        animate_light,
        SemanticVec3::new(0.0, 0.0, 1.0),
        Color::BLACK,
    )
}

fn build_lighting_scene_with_normal(
    material: SemanticSpatialMaterial,
    light_count: usize,
    animate_light: bool,
    normal: SemanticVec3,
    surface_color: Color,
) -> SceneInstance {
    build_lighting_scene_with_normals(
        material,
        light_count,
        animate_light,
        vec![normal; 3],
        surface_color,
    )
}

fn build_lighting_scene_with_normals(
    material: SemanticSpatialMaterial,
    light_count: usize,
    animate_light: bool,
    normals: Vec<SemanticVec3>,
    surface_color: Color,
) -> SceneInstance {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let payload = MeshResource::new(
        vec![
            SemanticVec3::new(-1.0, -1.0, 0.0),
            SemanticVec3::new(1.0, -1.0, 0.0),
            SemanticVec3::new(0.0, 1.0, 0.0),
        ],
        Some(normals),
        vec![0, 1, 2],
    )
    .unwrap();
    let handle = store.insert_geometry_mesh(payload);

    let mut camera = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.0 });
    camera.set_role(SemanticObjectRole::Camera3D);
    camera
        .set_camera_projection(Some(SemanticProjection3D::Orthographic {
            height: 4.0,
            near: 0.1,
            far: 30.0,
        }))
        .unwrap();
    camera.transform.translation.z = 5.0;
    let camera_id = attach(&mut store, camera);
    store.add_semantic_family_member(root, camera_id).unwrap();

    let mut surface = SemanticObjectState::new(StoredGeometry::Resource(handle));
    surface.style = opaque_style(surface_color);
    surface.set_spatial_material(material);
    let surface_id = store.insert_semantic_object(surface);
    store.add_semantic_family_member(root, surface_id).unwrap();

    let mut light_id = None;
    for _ in 0..light_count {
        let mut light = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.0 });
        light.set_role(SemanticObjectRole::PointLight3D);
        light.transform.translation.z = 2.0;
        light.style = opaque_style(Color::rgba(1.0, 0.0, 0.0, 1.0));
        let id = store.insert_semantic_object(light);
        store.add_semantic_family_member(root, id).unwrap();
        light_id.get_or_insert(id);
    }

    let mut index = SemanticExecutionIndex::new();
    if animate_light {
        let light_id = light_id.expect("animated light fixture includes its point light");
        let world_pose = |x| {
            SemanticWorldTransform3D::new(
                SemanticVec3::new(x, 0.0, 2.0),
                noon_core::SemanticRotation3D::IDENTITY,
                SemanticVec3::new(1.0, 1.0, 1.0),
            )
            .unwrap()
        };
        let mut transaction = SemanticMutationTransaction::new();
        let track = transaction.create_object_property_track(
            light_id,
            SemanticObjectTrackProperty::WorldTransform,
            SemanticObjectTrackValues::WorldTransform {
                from: world_pose(0.0),
                to: world_pose(2.0),
            },
            TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            CompositionTimeMap::identity(),
        );
        let animation_root = transaction.create_animation_composition(
            SemanticAnimationCompositionKind::Parallel,
            [track],
            AnimationOptions::new(),
        );
        let committed = transaction.apply(&mut store).unwrap();
        let animation_root = committed.resolve(animation_root).unwrap();
        SceneInstance::from_semantic_execution(
            lower_semantic_execution_root_with_animation_root(
                &store,
                root,
                &mut index,
                animation_root,
            )
            .unwrap(),
        )
    } else {
        SceneInstance::from_semantic_execution(
            lower_semantic_execution_root(&store, root, &mut index).unwrap(),
        )
    }
}

fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut GpuRenderer,
    frame_preparer: &mut FramePreparer,
    scene: &mut SceneInstance,
    target: &Target,
) -> Result<(noon_render_wgpu::SpatialUploadStats, Vec<u8>), SpatialPrepareError> {
    let publication = scene.take_renderer_publication();
    let stats = renderer.prepare_spatial(device, queue, &publication)?;
    let retry = renderer.prepare_spatial(device, queue, &publication)?;
    assert_eq!(
        retry.bytes_uploaded(),
        0,
        "same publication retry is idempotent"
    );
    assert_eq!(retry.rows_visited, 0);
    let prepared = frame_preparer.prepare_incremental(publication.frame(), publication.changes());
    renderer.upload(device, queue, &prepared);
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer.encode(&mut encoder, &target.view, &prepared, wgpu::Color::BLACK);
    Ok((stats, target.read(device, queue, encoder)))
}

fn pixel(bytes: &[u8], x: u32, y: u32) -> [u8; 4] {
    let start = ((y * WIDTH + x) * 4) as usize;
    bytes[start..start + 4].try_into().unwrap()
}

fn cairo_surface_scene(shared_unlit: bool) -> SceneInstance {
    let mut store = SemanticStore::new();
    let payload = MeshResource::new(
        vec![
            SemanticVec3::new(-1., -1., 0.),
            SemanticVec3::new(1., -1., 0.),
            SemanticVec3::new(1., 1., 0.),
            SemanticVec3::new(-1., 1., 0.),
        ],
        None,
        vec![0, 1, 3, 1, 2, 3],
    )
    .unwrap()
    .with_cairo_appearance(noon_core::CairoSurfaceAppearance {
        p0: SemanticVec3::new(-1., -1., 0.),
        p6: SemanticVec3::new(1., 1., 0.),
        span_p3_p0: SemanticVec3::new(2., 0., 0.),
        span_p12_p0: SemanticVec3::new(0., 2., 0.),
        span_p9_p6: SemanticVec3::new(-2., 0., 0.),
        span_p3_p6: SemanticVec3::new(0., -2., 0.),
    })
    .unwrap();
    let handle = store.insert_geometry_mesh(payload);
    let mut camera = SemanticObjectState::new(StoredGeometry::Circle { radius: 0. });
    camera.set_role(SemanticObjectRole::Camera3D);
    camera
        .set_camera_projection(Some(SemanticProjection3D::Orthographic {
            height: 4.,
            near: 0.1,
            far: 30.,
        }))
        .unwrap();
    camera.transform.translation.z = 5.;
    let camera_node = attach(&mut store, camera);
    let mut surface = SemanticObjectState::new(StoredGeometry::Resource(handle));
    surface.style = opaque_style(Color::rgba(0.8, 0.8, 0.8, 1.));
    surface.set_spatial_material(SemanticSpatialMaterial::CairoSurface);
    surface.set_surface_uv_cell(Some([0, 0]));
    if shared_unlit {
        surface.transform.scale = SemanticVec3::new(0.45, 0.45, 1.);
        surface.transform.translation.x = -0.75;
    }
    attach(&mut store, surface);
    if shared_unlit {
        let mut unlit = SemanticObjectState::new(StoredGeometry::Resource(handle));
        unlit.style = opaque_style(Color::rgba(0.8, 0.8, 0.8, 1.));
        unlit.transform.scale = SemanticVec3::new(0.45, 0.45, 1.);
        unlit.transform.translation.x = 0.75;
        attach(&mut store, unlit);
    }
    let mut light = SemanticObjectState::new(StoredGeometry::Circle { radius: 0. });
    light.set_role(SemanticObjectRole::PointLight3D);
    // Cairo's white scalar response deliberately ignores this red native light.
    light.style = opaque_style(Color::RED);
    light.transform.translation = SemanticVec3::new(-1., -1., 1.);
    let light_node = attach(&mut store, light);
    let mut index = SemanticExecutionIndex::new();
    let (mut compiled, _) = lower_semantic_execution(&store, &mut index)
        .unwrap()
        .into_parts();
    let pose = |x| {
        SemanticWorldTransform3D::new(
            SemanticVec3::new(x, x, 1.),
            noon_core::SemanticRotation3D::IDENTITY,
            SemanticVec3::new(1., 1., 1.),
        )
        .unwrap()
    };
    compiled
        .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
            id: TrackId::new(0),
            object: index.execution_object_id(light_node).unwrap(),
            property: Property::WorldTransform,
            values: TrackValues::WorldTransform {
                from: WorldTransformTrackEndpoint::from_world(pose(-1.)),
                to: WorldTransformTrackEndpoint::from_world(pose(1.)),
            },
            timing: TrackTiming::new(0., 1., RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        }))
        .unwrap();
    let camera_pose = |x| {
        SemanticWorldTransform3D::new(
            SemanticVec3::new(x, 0., 5.),
            noon_core::SemanticRotation3D::IDENTITY,
            SemanticVec3::new(1., 1., 1.),
        )
        .unwrap()
    };
    compiled
        .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
            id: TrackId::new(1),
            object: index.execution_object_id(camera_node).unwrap(),
            property: Property::WorldTransform,
            values: TrackValues::WorldTransform {
                from: WorldTransformTrackEndpoint::from_world(camera_pose(0.)),
                to: WorldTransformTrackEndpoint::from_world(camera_pose(0.5)),
            },
            timing: TrackTiming::new(1., 1., RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        }))
        .unwrap();
    SceneInstance::new(compiled)
}

#[test]
fn cairo_surface_retains_projected_clamped_gradient_and_only_updates_light() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let Ok(adapter) = instance.request_adapter(&Default::default()).await else {
            eprintln!("skipping Cairo Surface GPU qualification: no adapter is available");
            return;
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let target = Target::new(&device);
        let mut renderer = GpuRenderer::new(&device, &queue, FORMAT);
        renderer.set_viewport(&device, &queue, WIDTH, HEIGHT);
        let mut preparer = FramePreparer::new();
        let mut scene = cairo_surface_scene(false);
        let (initial, pixels) = render(
            &device,
            &queue,
            &mut renderer,
            &mut preparer,
            &mut scene,
            &target,
        )
        .unwrap();
        assert_eq!(initial.resident_meshes, 1);
        assert_eq!(initial.geometry_bytes, 4 * 24 + 6 * 4 + 96);
        // Independent analytic stops: clamp(.8 + .5) = 1; .8 + .5/27.
        // At the center their midpoint is .909259..., not clamp(1.059259...).
        let center = pixel(&pixels, WIDTH / 2, HEIGHT / 2);
        for channel in &center[..3] {
            assert!(
                (230..=234).contains(channel),
                "Cairo clamped-stop gradient: {center:?}"
            );
        }
        let first = pixel(&pixels, 40, 87);
        let last = pixel(&pixels, 87, 40);
        assert!(
            first[0] > last[0] + 25,
            "projected gradient follows p0 to p6: {first:?}, {last:?}"
        );
        scene.advance_to(1.).unwrap();
        let (moved, end_pixels) = render(
            &device,
            &queue,
            &mut renderer,
            &mut preparer,
            &mut scene,
            &target,
        )
        .unwrap();
        assert_eq!(moved.geometry_bytes, 0);
        assert_eq!(moved.instance_bytes, 0);
        assert_eq!(moved.camera_bytes, 0);
        assert_eq!(moved.light_bytes, 32);
        assert!(pixel(&end_pixels, 40, 87)[0] + 25 < pixel(&end_pixels, 87, 40)[0]);
        scene.advance_to(2.).unwrap();
        let (camera_moved, camera_pixels) = render(
            &device,
            &queue,
            &mut renderer,
            &mut preparer,
            &mut scene,
            &target,
        )
        .unwrap();
        assert_eq!(camera_moved.geometry_bytes, 0);
        assert_eq!(camera_moved.instance_bytes, 0);
        assert_eq!(camera_moved.camera_bytes, 64);
        assert_eq!(camera_moved.light_bytes, 0);
        assert_eq!(
            pixel(&camera_pixels, 48, 64),
            pixel(&end_pixels, 64, 64),
            "camera motion projects the same retained shading gradient"
        );
        scene.seek(0.).unwrap();
        let (rewound, replay) = render(
            &device,
            &queue,
            &mut renderer,
            &mut preparer,
            &mut scene,
            &target,
        )
        .unwrap();
        assert_eq!(rewound.geometry_bytes, 0);
        assert_eq!(
            pixels, replay,
            "deterministic seek restores the endpoint gradient"
        );

        let mut shared = cairo_surface_scene(true);
        let mut shared_renderer = GpuRenderer::new(&device, &queue, FORMAT);
        shared_renderer.set_viewport(&device, &queue, WIDTH, HEIGHT);
        let (stats, pixels) = render(
            &device,
            &queue,
            &mut shared_renderer,
            &mut preparer,
            &mut shared,
            &target,
        )
        .unwrap();
        assert_eq!(
            stats.resident_meshes, 1,
            "different materials retain one shared topology"
        );
        assert_eq!(stats.resident_instances, 2);
        let unlit = pixel(&pixels, 88, 64);
        assert_eq!(
            unlit,
            [204, 204, 204, 255],
            "Cairo pipeline must not shade the Unlit instance"
        );
        assert_ne!(pixel(&pixels, 40, 64), unlit);
    });
}

fn cairo_path_scene() -> SceneInstance {
    let mut store = SemanticStore::new();
    let mut camera = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.0 });
    camera.set_role(SemanticObjectRole::Camera3D);
    camera
        .set_camera_projection(Some(SemanticProjection3D::Orthographic {
            height: 4.0,
            near: 0.1,
            far: 30.0,
        }))
        .unwrap();
    camera.transform.translation.z = 5.0;
    let camera_node = attach(&mut store, camera);
    let mut shaft = SemanticObjectState::new(StoredGeometry::Line {
        start: noon_core::Vec2::new(-1.0, 0.0),
        end: noon_core::Vec2::new(1.0, 0.0),
    });
    shaft.style = SemanticStyle {
        fill: None,
        stroke: Some(SemanticPaint::Solid(Color::rgba(0.4, 0.4, 0.4, 1.0))),
        stroke_width: 0.25,
        stroke_width_mode: noon_core::StrokeWidthMode::ScreenSpace,
        ..SemanticStyle::default()
    };
    shaft.transform = SemanticWorldTransform3D::new(
        SemanticVec3::ZERO,
        noon_core::SemanticRotation3D::from_axis_angle(
            SemanticVec3::new(1.0, 0.0, 0.0),
            std::f64::consts::FRAC_PI_2,
        )
        .unwrap(),
        SemanticVec3::new(1.0, 1.0, 1.0),
    )
    .unwrap()
    .into();
    shaft.set_spatial_material(SemanticSpatialMaterial::CairoPath);
    shaft
        .set_cairo_path_appearance(noon_core::SemanticCairoPathAppearance {
            sheen_factor: 0.2,
            gradient_direction: Some(SemanticVec3::new(1.0, 0.0, 0.0)),
        })
        .unwrap();
    attach(&mut store, shaft);
    let mut light = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.0 });
    light.set_role(SemanticObjectRole::PointLight3D);
    light.style = opaque_style(Color::RED);
    light.transform.translation = SemanticVec3::new(-1.0, 1.0, 0.0);
    let light_node = attach(&mut store, light);
    let mut index = SemanticExecutionIndex::new();
    let (mut compiled, _) = lower_semantic_execution(&store, &mut index)
        .unwrap()
        .into_parts();
    for (track, node, start, from, to) in [
        (
            0,
            light_node,
            0.0,
            SemanticVec3::new(-1.0, 1.0, 0.0),
            SemanticVec3::new(-1.0, -1.0, 0.0),
        ),
        (
            1,
            camera_node,
            1.0,
            SemanticVec3::new(0.0, 0.0, 5.0),
            SemanticVec3::new(0.5, 0.0, 5.0),
        ),
    ] {
        let pose = |translation| {
            SemanticWorldTransform3D::new(
                translation,
                noon_core::SemanticRotation3D::IDENTITY,
                SemanticVec3::new(1.0, 1.0, 1.0),
            )
            .unwrap()
        };
        compiled
            .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
                id: TrackId::new(track),
                object: index.execution_object_id(node).unwrap(),
                property: Property::WorldTransform,
                values: TrackValues::WorldTransform {
                    from: WorldTransformTrackEndpoint::from_world(pose(from)),
                    to: WorldTransformTrackEndpoint::from_world(pose(to)),
                },
                timing: TrackTiming::new(start, 1.0, RateFunction::Linear),
                time_map: CompositionTimeMap::identity(),
            }))
            .unwrap();
    }
    SceneInstance::new(compiled)
}

#[test]
fn cairo_path_world_up_shading_and_sheen_reuse_geometry_across_light_camera_and_seek() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let Ok(adapter) = instance.request_adapter(&Default::default()).await else {
            eprintln!("skipping Cairo path GPU qualification: no adapter is available");
            return;
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let target = Target::new(&device);
        let mut renderer = GpuRenderer::new(&device, &queue, FORMAT);
        renderer.set_viewport(&device, &queue, WIDTH, HEIGHT);
        let mut preparer = FramePreparer::new();
        let mut scene = cairo_path_scene();
        let (initial, pixels) = render(
            &device,
            &queue,
            &mut renderer,
            &mut preparer,
            &mut scene,
            &target,
        )
        .unwrap();
        assert_eq!(initial.resident_instances, 1);
        assert!(initial.geometry_bytes > 0);
        // A single-curve Cairo path keeps world UP after the X rotation. Its
        // independently derived stops are .4 + .5 = .9 and clamp(.6 + .5) = 1.
        let center = pixel(&pixels, 64, 64);
        assert!(
            (241..=244).contains(&center[0]),
            "world-UP clamped sheen: {center:?}"
        );
        assert!(pixel(&pixels, 88, 64)[0] > pixel(&pixels, 40, 64)[0]);
        scene.advance_to(1.0).unwrap();
        let (moved, moved_pixels) = render(
            &device,
            &queue,
            &mut renderer,
            &mut preparer,
            &mut scene,
            &target,
        )
        .unwrap();
        assert_eq!(moved.geometry_bytes, 0);
        assert_eq!(moved.instance_bytes, 0);
        assert_eq!(moved.camera_bytes, 0);
        assert_eq!(moved.light_bytes, 32);
        // Negative illumination uses half of the signed cubic response:
        // -.25, giving .15/.35 stops and .25 at the center.
        let center = pixel(&moved_pixels, 64, 64);
        assert!(
            (62..=66).contains(&center[0]),
            "negative Cairo response: {center:?}"
        );
        scene.advance_to(2.0).unwrap();
        let (camera, camera_pixels) = render(
            &device,
            &queue,
            &mut renderer,
            &mut preparer,
            &mut scene,
            &target,
        )
        .unwrap();
        assert_eq!(camera.geometry_bytes, 0);
        assert_eq!(camera.instance_bytes, 0);
        assert_eq!(camera.camera_bytes, 64);
        assert_eq!(camera.light_bytes, 0);
        assert_eq!(pixel(&camera_pixels, 48, 64), pixel(&moved_pixels, 64, 64));
        scene.seek(0.0).unwrap();
        let (rewound, replay) = render(
            &device,
            &queue,
            &mut renderer,
            &mut preparer,
            &mut scene,
            &target,
        )
        .unwrap();
        assert_eq!(rewound.geometry_bytes, 0);
        assert_eq!(pixels, replay);
    });
}

#[test]
fn world_screen_stroke_keeps_width_across_perspective_distance_and_object_scale() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let Ok(adapter) = instance.request_adapter(&Default::default()).await else {
            eprintln!("skipping screen-stroke GPU qualification: no adapter is available");
            return;
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let mut store = SemanticStore::new();
        let mut camera = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.0 });
        camera.set_role(SemanticObjectRole::Camera3D);
        camera
            .set_camera_projection(Some(SemanticProjection3D::Perspective {
                vertical_fov_radians: 1.0,
                near: 0.1,
                far: 30.0,
            }))
            .unwrap();
        camera.transform.translation.z = 5.0;
        let camera_id = attach(&mut store, camera);
        let mut line = SemanticObjectState::new(StoredGeometry::Line {
            start: noon_core::Vec2::new(-1.0, 0.0),
            end: noon_core::Vec2::new(1.0, 0.0),
        });
        line.style = SemanticStyle {
            fill: None,
            stroke: Some(SemanticPaint::Solid(Color::WHITE)),
            stroke_width: 0.25,
            stroke_width_mode: noon_core::StrokeWidthMode::ScreenSpace,
            ..SemanticStyle::default()
        };
        line.transform = SemanticWorldTransform3D::new(
            SemanticVec3::ZERO,
            noon_core::SemanticRotation3D::IDENTITY,
            SemanticVec3::new(2.0, 0.01, 1.0),
        )
        .unwrap()
        .into();
        let line_id = attach(&mut store, line);
        let mut circle = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.65 });
        circle.style = SemanticStyle {
            fill: None,
            stroke: Some(SemanticPaint::Solid(Color::WHITE)),
            stroke_width: 0.25,
            stroke_width_mode: noon_core::StrokeWidthMode::ScreenSpace,
            ..SemanticStyle::default()
        };
        circle.transform = SemanticWorldTransform3D::new(
            SemanticVec3::new(-1.3, 1.35, 0.0),
            noon_core::SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 1.0, 0.0), 0.45)
                .unwrap(),
            SemanticVec3::new(1.1, 0.7, 1.0),
        )
        .unwrap()
        .into();
        let circle_id = attach(&mut store, circle);
        let path_handle = store
            .insert_geometry_path(
                noon_core::VectorPath::new()
                    .move_to(noon_core::Vec2::new(-1.0, -0.5))
                    .cubic_to(
                        noon_core::Vec2::new(-0.4, 1.0),
                        noon_core::Vec2::new(0.3, -1.0),
                        noon_core::Vec2::new(1.0, -0.5),
                    ),
            )
            .unwrap();
        let mut path = SemanticObjectState::new(StoredGeometry::Resource(path_handle));
        path.style = SemanticStyle {
            fill: None,
            stroke: Some(SemanticPaint::Solid(Color::WHITE)),
            stroke_width: 0.25,
            stroke_width_mode: noon_core::StrokeWidthMode::ScreenSpace,
            stroke_join: noon_core::StrokeJoin::Bevel,
            stroke_cap: noon_core::StrokeCap::Square,
            ..SemanticStyle::default()
        };
        path.transform = SemanticWorldTransform3D::new(
            SemanticVec3::new(1.25, -1.2, 0.0),
            noon_core::SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 1.0, 0.0), 0.35)
                .unwrap(),
            SemanticVec3::new(1.1, 0.7, 1.0),
        )
        .unwrap()
        .into();
        let path_id = attach(&mut store, path);
        let corner_handle = store
            .insert_geometry_path(
                noon_core::VectorPath::new()
                    .move_to(noon_core::Vec2::new(-0.55, -0.4))
                    .line_to(noon_core::Vec2::new(0.0, -0.4))
                    .line_to(noon_core::Vec2::new(0.0, 0.55)),
            )
            .unwrap();
        let mut corner = SemanticObjectState::new(StoredGeometry::Resource(corner_handle));
        corner.style = SemanticStyle {
            fill: None,
            stroke: Some(SemanticPaint::Solid(Color::WHITE)),
            stroke_width: 0.25,
            stroke_width_mode: noon_core::StrokeWidthMode::ScreenSpace,
            stroke_join: noon_core::StrokeJoin::Miter,
            stroke_cap: noon_core::StrokeCap::Square,
            ..SemanticStyle::default()
        };
        corner.transform = SemanticWorldTransform3D::new(
            SemanticVec3::new(-1.6, -1.2, 0.0),
            noon_core::SemanticRotation3D::from_axis_angle(SemanticVec3::new(0.0, 1.0, 0.0), 0.7)
                .unwrap(),
            SemanticVec3::new(1.5, 0.7, 1.0),
        )
        .unwrap()
        .into();
        let corner_id = attach(&mut store, corner);
        let mut index = SemanticExecutionIndex::new();
        let (mut compiled, _) = lower_semantic_execution(&store, &mut index)
            .unwrap()
            .into_parts();
        compiled
            .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
                id: TrackId::new(0),
                object: index.execution_object_id(camera_id).unwrap(),
                property: Property::WorldTransform,
                values: TrackValues::WorldTransform {
                    from: WorldTransformTrackEndpoint::from_world(
                        SemanticWorldTransform3D::new(
                            SemanticVec3::new(0.0, 0.0, 5.0),
                            noon_core::SemanticRotation3D::IDENTITY,
                            SemanticVec3::new(1.0, 1.0, 1.0),
                        )
                        .unwrap(),
                    ),
                    to: WorldTransformTrackEndpoint::from_world(
                        SemanticWorldTransform3D::new(
                            SemanticVec3::new(0.0, 0.0, 10.0),
                            noon_core::SemanticRotation3D::IDENTITY,
                            SemanticVec3::new(1.0, 1.0, 1.0),
                        )
                        .unwrap(),
                    ),
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
            Camera2D::new(noon_core::Vec2::ZERO, noon_core::Vec2::new(8.0, 8.0)).unwrap(),
        );
        let target = Target::new(&device);
        let mut preparer = FramePreparer::new();
        let mut counts = Vec::new();
        let mut circle_widths = Vec::new();
        let mut circle_pixel_counts = Vec::new();
        let mut circle_rasters = Vec::new();
        let mut path_pixel_counts = Vec::new();
        let mut corner_pixel_counts = Vec::new();
        let mut corner_rasters = Vec::new();
        for (index, time) in [0.0, 0.5, 1.0, 0.0].into_iter().enumerate() {
            runtime.seek(time).unwrap();
            let (uploads, pixels) = render(
                &device,
                &queue,
                &mut renderer,
                &mut preparer,
                &mut runtime,
                &target,
            )
            .unwrap();
            let width = (0..HEIGHT)
                .filter(|&y| pixel(&pixels, WIDTH / 2, y)[0] > 128)
                .count();
            assert_eq!(width, 4, "screen width must remain 0.25 frame units, despite perspective and 0.01 object Y scale at {time}");
            let circle_pixels: Vec<_> = (8..60)
                .flat_map(|y| (5..60).map(move |x| (x, y)))
                .filter(|&(x, y)| pixel(&pixels, x, y)[0] > 128)
                .collect();
            assert!(
                circle_pixels.len() > 30,
                "tilted Circle stroke disappeared at {time}"
            );
            let path_pixels = (72..108)
                .flat_map(|y| (58..104).map(move |x| (x, y)))
                .filter(|&(x, y)| pixel(&pixels, x, y)[0] > 128)
                .count();
            assert!(
                path_pixels > 12,
                "curved World path stroke disappeared at {time}"
            );
            path_pixel_counts.push(path_pixels);
            let corner_pixels = (65..94)
                .flat_map(|y| (28..60).map(move |x| (x, y)))
                .filter(|&(x, y)| pixel(&pixels, x, y)[0] > 128)
                .count();
            assert!(
                corner_pixels > 16,
                "projected miter path vanished at {time}"
            );
            corner_pixel_counts.push(corner_pixels);
            corner_rasters.push(pixels.clone());
            if index == 0 {
                let horizontal_width = (38..48)
                    .map(|x| (76..90).filter(|&y| pixel(&pixels, x, y)[0] > 128).count())
                    .max()
                    .unwrap();
                let vertical_width = (72..79)
                    .map(|y| (42..56).filter(|&x| pixel(&pixels, x, y)[0] > 128).count())
                    .max()
                    .unwrap();
                assert!(
                    (3..=5).contains(&horizontal_width),
                    "projected horizontal segment width {horizontal_width}px"
                );
                assert!(
                    (3..=5).contains(&vertical_width),
                    "projected vertical segment width {vertical_width}px"
                );
            }
            let min_x = circle_pixels.iter().map(|(x, _)| *x).min().unwrap();
            let max_x = circle_pixels.iter().map(|(x, _)| *x).max().unwrap();
            let min_y = circle_pixels.iter().map(|(_, y)| *y).min().unwrap();
            let max_y = circle_pixels.iter().map(|(_, y)| *y).max().unwrap();
            assert!(max_x - min_x >= 8 && max_y - min_y >= 8);
            let center_x = (min_x + max_x) / 2;
            let center_y = (min_y + max_y) / 2;
            assert!(
                pixel(&pixels, center_x, center_y)[0] < 32,
                "Circle stroke must preserve its inner silhouette at {time}"
            );
            let left_edge = (5..center_x).find(|x| pixel(&pixels, *x, center_y)[0] > 128);
            let right_edge = (center_x..60)
                .rev()
                .find(|x| pixel(&pixels, *x, center_y)[0] > 128);
            let top_edge = (8..center_y).find(|y| pixel(&pixels, center_x, *y)[0] > 128);
            let bottom_edge = (center_y..60)
                .rev()
                .find(|y| pixel(&pixels, center_x, *y)[0] > 128);
            assert!(
                left_edge.is_some()
                    && right_edge.is_some()
                    && top_edge.is_some()
                    && bottom_edge.is_some(),
                "Circle stroke must cover all four outer cardinal silhouettes at {time}"
            );
            let left_width = (left_edge.unwrap()..=center_x)
                .take_while(|x| pixel(&pixels, *x, center_y)[0] > 128)
                .count();
            let top_width = (top_edge.unwrap()..=center_y)
                .take_while(|y| pixel(&pixels, center_x, *y)[0] > 128)
                .count();
            assert!(
                (3..=5).contains(&left_width),
                "left cardinal stroke thickness {left_width} at {time}"
            );
            assert!(
                (3..=5).contains(&top_width),
                "top cardinal stroke thickness {top_width} at {time}"
            );
            circle_widths.push((left_width, top_width));
            circle_pixel_counts.push(circle_pixels.len());
            circle_rasters.push(pixels.clone());
            counts.push(
                (0..WIDTH)
                    .filter(|&x| pixel(&pixels, x, HEIGHT / 2)[0] > 128)
                    .count(),
            );
            if index > 0 {
                assert_eq!(
                    uploads.geometry_bytes, 0,
                    "camera frames reuse stroke topology"
                );
                assert_eq!(
                    uploads.instance_bytes, 0,
                    "camera frames do not rewrite object instances"
                );
            }
        }
        assert!(
            counts[0] > counts[1] && counts[1] > counts[2],
            "perspective still changes centerline length"
        );
        assert_eq!(
            counts[0], counts[3],
            "backward seek restores the same projected extent"
        );
        assert!(
            circle_widths
                .iter()
                .take(3)
                .all(|(left, top)| left.abs_diff(circle_widths[0].0) <= 1
                    && top.abs_diff(circle_widths[0].1) <= 1),
            "screen stroke thickness remains stable across perspective distances: {circle_widths:?}"
        );
        assert_eq!(circle_widths[0], circle_widths[3]);
        assert_eq!(
            circle_rasters[0], circle_rasters[3],
            "Circle raster exactly replays after backward seek"
        );
        assert_eq!(
            corner_rasters[0], corner_rasters[3],
            "projected corner joins exactly replay after backward seek"
        );
        assert_eq!(corner_pixel_counts[0], corner_pixel_counts[3]);
        assert!(index.execution_object_id(corner_id).is_some());
        let line_object = index.execution_object_id(line_id).unwrap();
        let circle_object = index.execution_object_id(circle_id).unwrap();
        let style = noon_core::Style {
            stroke_width: 0.5,
            ..runtime
                .frame()
                .objects
                .iter()
                .find(|object| object.id == line_object)
                .unwrap()
                .style
        };
        runtime
            .apply_execution_patch(&ExecutionPatch::SetStyle {
                object: line_object,
                style,
            })
            .unwrap();
        let (uploads, pixels) = render(
            &device,
            &queue,
            &mut renderer,
            &mut preparer,
            &mut runtime,
            &target,
        )
        .unwrap();
        assert_eq!(
            uploads.geometry_bytes, 0,
            "width edits reuse the unit stroke topology"
        );
        assert!(
            uploads.instance_bytes > 0,
            "width edits update the affected instance"
        );
        assert_eq!(
            (0..HEIGHT)
                .filter(|&y| pixel(&pixels, WIDTH / 2, y)[0] > 128)
                .count(),
            8
        );
        let circle_frame = runtime.frame();
        let circle_style = noon_core::Style {
            stroke_width: 0.5,
            ..circle_frame
                .objects
                .iter()
                .find(|object| object.id == circle_object)
                .unwrap()
                .style
        };
        runtime
            .apply_execution_patch(&ExecutionPatch::SetStyle {
                object: circle_object,
                style: circle_style,
            })
            .unwrap();
        let (uploads, pixels) = render(
            &device,
            &queue,
            &mut renderer,
            &mut preparer,
            &mut runtime,
            &target,
        )
        .unwrap();
        assert_eq!(
            uploads.geometry_bytes, 0,
            "Circle width edits reuse retained topology"
        );
        assert!(
            uploads.instance_bytes > 0,
            "Circle width edits update its instance"
        );
        let widened = (8..60)
            .flat_map(|y| (5..60).map(move |x| (x, y)))
            .filter(|&(x, y)| pixel(&pixels, x, y)[0] > 128)
            .count();
        assert!(
            widened > circle_pixel_counts[3] + 20,
            "Circle width edit visibly thickens the retained stroke: {widened} vs {}",
            circle_pixel_counts[3]
        );

        let path_object = index.execution_object_id(path_id).unwrap();
        let path_style = noon_core::Style {
            stroke_width: 0.5,
            ..runtime
                .frame()
                .objects
                .iter()
                .find(|object| object.id == path_object)
                .unwrap()
                .style
        };
        runtime
            .apply_execution_patch(&ExecutionPatch::SetStyle {
                object: path_object,
                style: path_style,
            })
            .unwrap();
        let (uploads, pixels) = render(
            &device,
            &queue,
            &mut renderer,
            &mut preparer,
            &mut runtime,
            &target,
        )
        .unwrap();
        assert_eq!(
            uploads.geometry_bytes, 0,
            "path width edits reuse retained curve/join topology"
        );
        assert!(
            uploads.instance_bytes > 0,
            "path width edits update the instance"
        );
        let widened_path = (72..108)
            .flat_map(|y| (58..104).map(move |x| (x, y)))
            .filter(|&(x, y)| pixel(&pixels, x, y)[0] > 128)
            .count();
        assert!(
            widened_path > path_pixel_counts[3],
            "screen width edit thickens the retained path: {widened_path} vs {}",
            path_pixel_counts[3]
        );
    });
}

#[test]
fn spatial_renderer_rejects_an_older_publication_after_advancing() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let Ok(adapter) = instance.request_adapter(&Default::default()).await else {
            eprintln!("skipping spatial mesh GPU qualification: no adapter is available");
            return;
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let (mut scene, _, _, _, _) = build_scene(false, false, false);
        let mut older_scene = scene.clone();
        let older_publication = older_scene.take_renderer_publication();
        let mut renderer = GpuRenderer::new(&device, &queue, FORMAT);
        renderer
            .prepare_spatial(&device, &queue, &older_publication)
            .unwrap();
        scene.seek(0.5).unwrap();
        let newer_publication = scene.take_renderer_publication();
        renderer
            .prepare_spatial(&device, &queue, &newer_publication)
            .unwrap();
        assert_eq!(
            renderer.prepare_spatial(&device, &queue, &older_publication),
            Err(SpatialPrepareError::StalePublication)
        );
    });
}

#[test]
fn semantic_meshes_use_depth_projection_shared_residency_local_updates_and_atomic_validation() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let Ok(adapter) = instance.request_adapter(&Default::default()).await else {
            eprintln!("skipping spatial mesh GPU qualification: no adapter is available");
            return;
        };
        eprintln!("spatial mesh adapter: {:?}", adapter.get_info());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let target = Target::new(&device);
        let mut renderer = GpuRenderer::new(&device, &queue, FORMAT);
        renderer.set_viewport(&device, &queue, WIDTH, HEIGHT);
        renderer.set_camera(
            &queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(10.0, 10.0)).unwrap(),
        );
        let mut frame_preparer = FramePreparer::new();
        let (mut runtime, index, near_id, _, far_id) = build_scene(true, false, false);

        let (initial, pixels) = render(
            &device,
            &queue,
            &mut renderer,
            &mut frame_preparer,
            &mut runtime,
            &target,
        )
        .unwrap();
        assert_eq!(
            initial.resident_meshes, 1,
            "two objects sharing one resource allocate one mesh"
        );
        assert_eq!(initial.resident_instances, 2);
        assert!(initial.geometry_bytes > 0);
        assert_eq!(initial.instance_bytes, 2 * 128);
        assert_eq!(initial.camera_bytes, 64);
        let center = pixel(&pixels, WIDTH / 2, HEIGHT / 2);
        assert!(
            center[0] > 220
                && u16::from(center[0]) > u16::from(center[1]) * 2
                && u16::from(center[0]) > u16::from(center[2]) * 2,
            "front red triangle must win although blue is later in painter order: {center:?}"
        );
        let overlay = pixel(&pixels, 116, HEIGHT / 2);
        assert!(
            u16::from(overlay[1]) * 10 > u16::from(overlay[0]) * 13
                && u16::from(overlay[1]) * 10 > u16::from(overlay[2]) * 14,
            "2D overlay composes over the spatial scene on the same target: {overlay:?}"
        );

        let (clean, _) = render(
            &device,
            &queue,
            &mut renderer,
            &mut frame_preparer,
            &mut runtime,
            &target,
        )
        .unwrap();
        assert_eq!(clean.bytes_uploaded(), 0);
        assert_eq!(clean.rows_visited, 0);

        runtime.advance_to(0.5).unwrap();
        let (moved, pixels) = render(
            &device,
            &queue,
            &mut renderer,
            &mut frame_preparer,
            &mut runtime,
            &target,
        )
        .unwrap();
        assert_eq!(
            moved.geometry_bytes, 0,
            "world motion reuses immutable mesh buffers"
        );
        assert_eq!(
            moved.instance_bytes, 128,
            "one changed world transform writes one instance"
        );
        assert_eq!(
            moved.camera_bytes, 64,
            "camera motion updates one projection uniform"
        );
        let center = pixel(&pixels, WIDTH / 2, HEIGHT / 2);
        assert!(
            center[0] > 220
                && u16::from(center[0]) > u16::from(center[1]) * 2
                && u16::from(center[0]) > u16::from(center[2]) * 2,
            "intermediate front pose remains red: {center:?}"
        );

        runtime.seek(2.0).unwrap();
        let (ended, pixels) = render(
            &device,
            &queue,
            &mut renderer,
            &mut frame_preparer,
            &mut runtime,
            &target,
        )
        .unwrap();
        assert_eq!(ended.geometry_bytes, 0);
        let center = pixel(&pixels, WIDTH / 2, HEIGHT / 2);
        assert!(
            center[2] > 220 && u16::from(center[2]) > u16::from(center[0]) * 2,
            "far blue triangle becomes visible after the animated red mesh moves behind it: {center:?}"
        );

        // Orthographic projection uses the same retained target and shared mesh
        // payload, with no vertex re-upload when only the camera declaration differs.
        let (mut ortho_scene, _, _, _, _) = build_scene(true, true, false);
        let mut ortho_renderer = GpuRenderer::new(&device, &queue, FORMAT);
        ortho_renderer.set_viewport(&device, &queue, WIDTH, HEIGHT);
        ortho_renderer.set_camera(
            &queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(10.0, 10.0)).unwrap(),
        );
        let (ortho, ortho_pixels) = render(
            &device,
            &queue,
            &mut ortho_renderer,
            &mut frame_preparer,
            &mut ortho_scene,
            &target,
        )
        .unwrap();
        assert_eq!(
            ortho.geometry_bytes, initial.geometry_bytes,
            "orthographic camera uses the same mesh payload size"
        );
        assert_eq!(ortho.resident_meshes, 1);
        assert!(pixel(&ortho_pixels, WIDTH / 2, HEIGHT / 2)[0] > 220);

        let (mut invalid_scene, invalid_index, invalid_near, _, invalid_far) =
            build_scene(false, false, true);
        let mut invalid_renderer = GpuRenderer::new(&device, &queue, FORMAT);
        invalid_renderer.set_viewport(&device, &queue, WIDTH, HEIGHT);
        let publication = invalid_scene.take_renderer_publication();
        assert_eq!(
            invalid_renderer.prepare_spatial(&device, &queue, &publication),
            Err(SpatialPrepareError::UnrepresentableVertex)
        );
        assert_eq!(
            invalid_renderer.prepare_spatial(&device, &queue, &publication),
            Err(SpatialPrepareError::UnrepresentableVertex),
            "retry must revalidate the rejected frame without publishing partial residency"
        );
        drop(publication);
        for node in [invalid_near, invalid_far] {
            let object = invalid_index.execution_object_id(node).unwrap();
            invalid_scene
                .apply_execution_patch(&ExecutionPatch::RemoveObject(object))
                .unwrap();
        }
        let camera_only_publication = invalid_scene.take_renderer_publication();
        let after_rejection = invalid_renderer
            .prepare_spatial(&device, &queue, &camera_only_publication)
            .unwrap();
        assert_eq!(
            after_rejection.bytes_uploaded(),
            0,
            "removing rejected meshes admits a camera-only frame without uploads"
        );
        assert_eq!(after_rejection.resident_meshes, 0);
        assert_eq!(after_rejection.resident_instances, 0);

        for node in [near_id, far_id] {
            let object = index.execution_object_id(node).unwrap();
            runtime
                .apply_execution_patch(&ExecutionPatch::RemoveObject(object))
                .unwrap();
            let publication = runtime.take_renderer_publication();
            let retired = renderer
                .prepare_spatial(&device, &queue, &publication)
                .unwrap();
            if node == near_id {
                assert_eq!(
                    retired.resident_meshes, 1,
                    "shared mesh stays resident while one object still uses it"
                );
                assert_eq!(retired.resident_instances, 1);
            } else {
                assert_eq!(
                    retired.resident_meshes, 0,
                    "last user removal retires the mesh allocation"
                );
                assert_eq!(retired.resident_instances, 0);
            }
        }
    });
}

#[test]
fn native_publication_preparation_skips_spatial_rows_in_retained_vector_stream() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let Ok(adapter) = instance.request_adapter(&Default::default()).await else {
            eprintln!(
                "skipping retained/spatial integration qualification: no adapter is available"
            );
            return;
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let target = Target::new(&device);
        let mut renderer = GpuRenderer::new(&device, &queue, FORMAT);
        renderer.set_viewport(&device, &queue, WIDTH, HEIGHT);
        renderer.set_camera(
            &queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(10.0, 10.0)).unwrap(),
        );
        let mut text_state = renderer.create_retained_text_state(&device, &queue);
        let mut retained = RetainedFramePreparer::new();
        let (mut runtime, _, _, _, _) = build_scene(true, false, false);
        let publication = runtime.take_renderer_publication();
        let spatial = renderer
            .prepare_spatial(&device, &queue, &publication)
            .unwrap();
        assert_eq!(spatial.resident_meshes, 1);

        // This is the native/direct-WASM host preparation order. A visible list
        // containing every row ensures the retained pass itself must classify
        // and bypass the camera and mesh declarations.
        let visible: Vec<_> = (0..publication.frame().objects.len()).collect();
        let transient = retained
            .prepare_transient_presentations_visible(&publication, &visible)
            .unwrap();
        let prepared = retained
            .prepare_planned_publication_visible(
                &device,
                &publication,
                &visible,
                TextDeviceMetrics::uniform(1.0).unwrap(),
            )
            .unwrap();
        renderer.upload_retained(&device, &queue, &prepared, &mut text_state);
        if !transient.slots.is_empty() {
            renderer.upload_derived(&device, &queue, &transient);
        }

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
        let pixels = target.read(&device, &queue, encoder);
        let center = pixel(&pixels, WIDTH / 2, HEIGHT / 2);
        assert!(
            center[0] > 220 && u16::from(center[0]) > u16::from(center[2]) * 2,
            "spatial mesh draws after the retained pass prepares the mixed frame: {center:?}"
        );

        let retained_before_motion = retained.incremental_stats();
        runtime.advance_to(0.5).unwrap();
        let publication = runtime.take_renderer_publication();
        let spatial = renderer
            .prepare_spatial(&device, &queue, &publication)
            .unwrap();
        assert_eq!(spatial.resident_meshes, 1);
        assert_eq!(spatial.geometry_bytes, 0);

        let visible: Vec<_> = (0..publication.frame().objects.len()).collect();
        let transient = retained
            .prepare_transient_presentations_visible(&publication, &visible)
            .unwrap();
        let prepared = retained
            .prepare_planned_publication_visible(
                &device,
                &publication,
                &visible,
                TextDeviceMetrics::uniform(1.0).unwrap(),
            )
            .unwrap();
        assert_eq!(prepared.geometry_stats().full_rebuilds, 0);
        let upload = renderer.upload_retained(&device, &queue, &prepared, &mut text_state);
        assert_eq!(upload.geometry.bytes_uploaded, 0);
        if !transient.slots.is_empty() {
            renderer.upload_derived(&device, &queue, &transient);
        }

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
        let pixels = target.read(&device, &queue, encoder);
        let center = pixel(&pixels, WIDTH / 2, HEIGHT / 2);
        assert!(
            center[0] > 220 && u16::from(center[0]) > u16::from(center[2]) * 2,
            "mixed frame remains drawable after spatial-only motion: {center:?}"
        );
        assert_eq!(
            retained.incremental_stats().scratch_rebuilds,
            retained_before_motion.scratch_rebuilds,
            "world/camera-only motion must not rebuild the retained planar scratch frame"
        );
    });
}

#[test]
fn point_lit_mesh_uses_cubic_normal_response_and_light_only_updates() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let Ok(adapter) = instance.request_adapter(&Default::default()).await else {
            eprintln!("skipping point-light GPU qualification: no adapter is available");
            return;
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let target = Target::new(&device);
        let mut renderer = GpuRenderer::new(&device, &queue, FORMAT);
        renderer.set_viewport(&device, &queue, WIDTH, HEIGHT);
        let mut frame_preparer = FramePreparer::new();
        let mut lit = build_lighting_scene(SemanticSpatialMaterial::PointLit, 1, true);

        let (initial, pixels) = render(
            &device,
            &queue,
            &mut renderer,
            &mut frame_preparer,
            &mut lit,
            &target,
        )
        .unwrap();
        assert_eq!(initial.resident_meshes, 1);
        assert_eq!(initial.resident_instances, 1);
        assert_eq!(initial.light_bytes, 32);
        let lit_center = pixel(&pixels, WIDTH / 2, HEIGHT / 2);
        assert!(
            (110..=145).contains(&lit_center[0]) && lit_center[1] < 4 && lit_center[2] < 4,
            "front-facing +Z normal receives the cubic positive response: {lit_center:?}"
        );

        lit.advance_to(0.5).unwrap();
        let (moved, pixels) = render(
            &device,
            &queue,
            &mut renderer,
            &mut frame_preparer,
            &mut lit,
            &target,
        )
        .unwrap();
        assert_eq!(
            moved.rows_visited, 1,
            "only the animated light row is visited"
        );
        assert_eq!(
            moved.light_bytes, 32,
            "the changed point-light uniform is uploaded"
        );
        assert_eq!(
            moved.instance_bytes, 0,
            "light motion does not rewrite mesh instances"
        );
        assert_eq!(
            moved.geometry_bytes, 0,
            "light motion preserves mesh residency"
        );
        assert_eq!(moved.camera_bytes, 0);
        let moved_center = pixel(&pixels, WIDTH / 2, HEIGHT / 2);
        assert!(
            moved_center[0] + 25 < lit_center[0],
            "moving the light off the face normal lowers the cubic response: {moved_center:?}"
        );

        let mut unlit_renderer = GpuRenderer::new(&device, &queue, FORMAT);
        unlit_renderer.set_viewport(&device, &queue, WIDTH, HEIGHT);
        let mut unlit_preparer = FramePreparer::new();
        let mut unlit = build_lighting_scene(SemanticSpatialMaterial::Unlit, 1, false);
        let (_, pixels) = render(
            &device,
            &queue,
            &mut unlit_renderer,
            &mut unlit_preparer,
            &mut unlit,
            &target,
        )
        .unwrap();
        assert_eq!(
            pixel(&pixels, WIDTH / 2, HEIGHT / 2),
            [0, 0, 0, 255],
            "Unlit remains byte-identical when a light is present"
        );

        let mut reverse_normal_renderer = GpuRenderer::new(&device, &queue, FORMAT);
        reverse_normal_renderer.set_viewport(&device, &queue, WIDTH, HEIGHT);
        let mut reverse_normal_preparer = FramePreparer::new();
        let mut reverse_normal = build_lighting_scene_with_normal(
            SemanticSpatialMaterial::PointLit,
            1,
            false,
            SemanticVec3::new(0.0, 0.0, -1.0),
            Color::WHITE,
        );
        let (_, pixels) = render(
            &device,
            &queue,
            &mut reverse_normal_renderer,
            &mut reverse_normal_preparer,
            &mut reverse_normal,
            &target,
        )
        .unwrap();
        let reverse_center = pixel(&pixels, WIDTH / 2, HEIGHT / 2);
        assert!(
            (185..=200).contains(&reverse_center[0])
                && reverse_center[1] > 250
                && reverse_center[2] > 250,
            "negative dot contribution is half-strength (-0.25 at -1): {reverse_center:?}"
        );

        // At the center sample the two lower +Z normals and upper -Z normal
        // interpolate to zero. The fragment shader must take the unlit
        // fallback instead of normalizing zero and producing NaNs.
        let zero_normal_color = Color::rgba(0.0, 0.0, 1.0, 1.0);
        let mut zero_normal_renderer = GpuRenderer::new(&device, &queue, FORMAT);
        zero_normal_renderer.set_viewport(&device, &queue, WIDTH, HEIGHT);
        let mut zero_normal_preparer = FramePreparer::new();
        let mut zero_normal = build_lighting_scene_with_normals(
            SemanticSpatialMaterial::PointLit,
            1,
            false,
            vec![
                SemanticVec3::new(0.0, 0.0, 1.0),
                SemanticVec3::new(0.0, 0.0, 1.0),
                SemanticVec3::new(0.0, 0.0, -1.0),
            ],
            zero_normal_color,
        );
        // The pixel center at (64,64) is world y=-1/64 for this four-unit
        // orthographic viewport. Align the zero-normal interpolation line with
        // that sample rather than testing a nonzero neighboring normal.
        let row = zero_normal
            .frame()
            .objects
            .iter()
            .find(|row| {
                row.content
                    .geometry()
                    .is_some_and(|geometry| matches!(geometry, noon_core::GeometryRef::External(_)))
            })
            .unwrap();
        let object = row.id;
        let mut pose = row.world_transform().unwrap();
        pose.translation.y = -1.0 / 64.0;
        zero_normal
            .apply_execution_patch(&ExecutionPatch::SetSemanticTransform {
                object,
                transform: pose.into(),
            })
            .unwrap();
        let (_, pixels) = render(
            &device,
            &queue,
            &mut zero_normal_renderer,
            &mut zero_normal_preparer,
            &mut zero_normal,
            &target,
        )
        .unwrap();
        let zero_normal_pixel = pixel(&pixels, WIDTH / 2, HEIGHT / 2);
        assert!(
            zero_normal_pixel[0] <= 2
                && zero_normal_pixel[1] <= 2
                && zero_normal_pixel[2] == 255
                && zero_normal_pixel[3] == 255,
            "zero interpolated normal falls back to the base color: {zero_normal_pixel:?}"
        );

        let mut missing_light = build_lighting_scene(SemanticSpatialMaterial::PointLit, 0, false);
        let publication = missing_light.take_renderer_publication();
        let mut missing_renderer = GpuRenderer::new(&device, &queue, FORMAT);
        assert_eq!(
            missing_renderer.prepare_spatial(&device, &queue, &publication),
            Err(SpatialPrepareError::MissingPointLight)
        );

        let mut multiple_lights = build_lighting_scene(SemanticSpatialMaterial::Unlit, 2, false);
        let publication = multiple_lights.take_renderer_publication();
        let mut multiple_renderer = GpuRenderer::new(&device, &queue, FORMAT);
        assert_eq!(
            multiple_renderer.prepare_spatial(&device, &queue, &publication),
            Err(SpatialPrepareError::MultiplePointLights)
        );
    });
}

#[test]
fn mesh_fill_and_stroke_changes_preserve_residency_atomically() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let Ok(adapter) = instance.request_adapter(&Default::default()).await else {
            eprintln!("skipping spatial mesh GPU qualification: no adapter is available");
            return;
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let mut renderer = GpuRenderer::new(&device, &queue, FORMAT);
        renderer.set_viewport(&device, &queue, WIDTH, HEIGHT);
        let (mut scene, index, near_node, _, _) = build_scene(false, false, false);
        let near_object = index.execution_object_id(near_node).unwrap();
        let original_style = scene
            .frame()
            .objects
            .iter()
            .find(|row| row.id == near_object)
            .unwrap()
            .style;

        let initial_publication = scene.take_renderer_publication();
        let initial = renderer
            .prepare_spatial(&device, &queue, &initial_publication)
            .unwrap();
        assert_eq!(initial.resident_meshes, 1);
        assert_eq!(initial.resident_instances, 2);
        drop(initial_publication);

        let mut no_fill = original_style;
        no_fill.fill = None;
        scene
            .apply_execution_patch(&ExecutionPatch::SetStyle {
                object: near_object,
                style: no_fill,
            })
            .unwrap();
        let no_fill_publication = scene.take_renderer_publication();
        let no_fill_stats = renderer
            .prepare_spatial(&device, &queue, &no_fill_publication)
            .unwrap();
        assert_eq!(
            no_fill_stats.resident_meshes, 1,
            "the other user retains the mesh"
        );
        assert_eq!(
            no_fill_stats.resident_instances, 1,
            "fill-less mesh has no draw row"
        );
        assert_eq!(no_fill_stats.geometry_bytes, 0);
        drop(no_fill_publication);

        scene
            .apply_execution_patch(&ExecutionPatch::SetStyle {
                object: near_object,
                style: original_style,
            })
            .unwrap();
        let restored_publication = scene.take_renderer_publication();
        let restored = renderer
            .prepare_spatial(&device, &queue, &restored_publication)
            .unwrap();
        assert_eq!(restored.resident_meshes, 1);
        assert_eq!(restored.resident_instances, 2);
        assert_eq!(
            restored.geometry_bytes, 0,
            "restoring fill reuses the mesh retained by the other instance"
        );
        drop(restored_publication);

        let mut stroked = original_style;
        stroked.stroke = Some(Color::WHITE);
        stroked.stroke_width = 1.0;
        scene
            .apply_execution_patch(&ExecutionPatch::SetStyle {
                object: near_object,
                style: stroked,
            })
            .unwrap();
        let stroke_publication = scene.take_renderer_publication();
        let stroke_stats = renderer
            .prepare_spatial(&device, &queue, &stroke_publication)
            .unwrap();
        assert_eq!(stroke_stats.rows_visited, 1);
        assert!(
            stroke_stats.geometry_bytes > 0,
            "boundary geometry is derived once"
        );
        assert_eq!(stroke_stats.resident_meshes, 1);
        let retry = renderer
            .prepare_spatial(&device, &queue, &stroke_publication)
            .unwrap();
        assert_eq!(retry.bytes_uploaded(), 0);
        drop(stroke_publication);

        scene
            .apply_execution_patch(&ExecutionPatch::SetStyle {
                object: near_object,
                style: original_style,
            })
            .unwrap();
        let valid_retry = scene.take_renderer_publication();
        let retry_stats = renderer
            .prepare_spatial(&device, &queue, &valid_retry)
            .unwrap();
        assert_eq!(retry_stats.resident_meshes, 1);
        assert_eq!(retry_stats.resident_instances, 2);
        assert_eq!(retry_stats.geometry_bytes, 0);
    });
}

#[test]
fn many_shared_meshes_upload_only_the_changed_instance_and_batch_one_draw() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let Ok(adapter) = instance.request_adapter(&Default::default()).await else {
            eprintln!("skipping spatial mesh locality qualification: no adapter is available");
            return;
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let target = Target::new(&device);
        let mut renderer = GpuRenderer::new(&device, &queue, FORMAT);
        renderer.set_viewport(&device, &queue, WIDTH, HEIGHT);
        let mut frame_preparer = FramePreparer::new();
        let mut runtime = build_many_mesh_scene(600);

        let initial_publication = runtime.take_renderer_publication();
        let initial = renderer
            .prepare_spatial(&device, &queue, &initial_publication)
            .unwrap();
        assert_eq!(initial.resident_meshes, 1);
        assert_eq!(initial.resident_instances, 600);
        assert!(initial.geometry_bytes > 0);
        drop(initial_publication);

        let clean_publication = runtime.take_renderer_publication();
        let clean = renderer
            .prepare_spatial(&device, &queue, &clean_publication)
            .unwrap();
        assert_eq!(clean.bytes_uploaded(), 0);
        assert_eq!(clean.rows_visited, 0);
        drop(clean_publication);

        runtime.advance_to(0.5).unwrap();
        let moved_publication = runtime.take_renderer_publication();
        let moved = renderer
            .prepare_spatial(&device, &queue, &moved_publication)
            .unwrap();
        assert_eq!(moved.rows_visited, 1, "only the animated row is staged");
        assert_eq!(moved.geometry_bytes, 0, "mesh buffers remain resident");
        assert_eq!(
            moved.instance_bytes, 128,
            "one 128-byte instance is written"
        );
        assert_eq!(
            moved.camera_bytes, 0,
            "the unchanged camera uniform is retained"
        );
        assert_eq!(moved.resident_meshes, 1);
        assert_eq!(moved.resident_instances, 600);

        let prepared = frame_preparer
            .prepare_incremental(moved_publication.frame(), moved_publication.changes());
        renderer.upload(&device, &queue, &prepared);
        let mut encoder = device.create_command_encoder(&Default::default());
        let draw = renderer.encode(&mut encoder, &target.view, &prepared, wgpu::Color::BLACK);
        target.read(&device, &queue, encoder);
        assert_eq!(
            draw.draw_calls, 1,
            "contiguous shared-mesh instances batch together"
        );
        assert_eq!(draw.instances_drawn, 600);
    });
}

use noon_core::Vec2;

#[test]
fn translucent_faces_sort_after_opaque_geometry_and_reorder_on_pose_changes() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let Ok(adapter) = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
        else {
            eprintln!("skipping spatial face qualification: no adapter is available");
            return;
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let mut renderer = GpuRenderer::new(&device, &queue, FORMAT);
        renderer.set_viewport(&device, &queue, WIDTH, HEIGHT);
        let target = Target::new(&device);
        let mut preparer = FramePreparer::new();
        let (mut scene, index, near, _, far) = build_scene(false, false, false);
        for node in [near, far] {
            let object = index.execution_object_id(node).unwrap();
            let mut style = scene
                .frame()
                .objects
                .iter()
                .find(|row| row.id == object)
                .unwrap()
                .style;
            style.opacity = 0.5;
            scene
                .apply_execution_patch(&ExecutionPatch::SetStyle { object, style })
                .unwrap();
        }
        scene.seek(0.0).unwrap();
        let (initial, pixels) = render(
            &device,
            &queue,
            &mut renderer,
            &mut preparer,
            &mut scene,
            &target,
        )
        .unwrap();
        let first = pixel(&pixels, WIDTH / 2, HEIGHT / 2);
        assert!(
            first[0] > first[2],
            "near red face must blend after far blue: {first:?}"
        );
        assert_eq!(initial.resident_meshes, 1);
        scene.seek(1.0).unwrap();
        let (moved, pixels) = render(
            &device,
            &queue,
            &mut renderer,
            &mut preparer,
            &mut scene,
            &target,
        )
        .unwrap();
        let second = pixel(&pixels, WIDTH / 2, HEIGHT / 2);
        assert!(
            second[2] > second[0],
            "blue must blend after red moved behind it: {second:?}"
        );
        assert_eq!(moved.geometry_bytes, 0);
        scene.seek(0.0).unwrap();
        let (_, replayed) = render(
            &device,
            &queue,
            &mut renderer,
            &mut preparer,
            &mut scene,
            &target,
        )
        .unwrap();
        assert_eq!(pixel(&replayed, WIDTH / 2, HEIGHT / 2), first);
        let settled = scene.take_renderer_publication();
        assert_eq!(
            renderer
                .prepare_spatial(&device, &queue, &settled)
                .unwrap()
                .bytes_uploaded(),
            0
        );
    });
}
