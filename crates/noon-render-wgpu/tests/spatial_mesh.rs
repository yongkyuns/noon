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
    let camera_id = store.insert_semantic_object(camera);

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
        light.style = opaque_style(Color::RED);
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
            "zero/near-zero interpolated normal falls back to the base color: {zero_normal_pixel:?}"
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
        assert!(matches!(
            renderer.prepare_spatial(&device, &queue, &stroke_publication),
            Err(SpatialPrepareError::MeshStroke(_))
        ));
        assert!(matches!(
            renderer.prepare_spatial(&device, &queue, &stroke_publication),
            Err(SpatialPrepareError::MeshStroke(_))
        ));
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
