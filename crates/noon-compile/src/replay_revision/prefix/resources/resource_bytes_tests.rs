use super::*;
use noon_core::{
    FontResource, FontResourceHandle, FontResourceId, GeometryResource, GeometryResourceArena,
    RasterImageResource, RasterImageResourceHandle, RasterImageResourceId, Rect, TextResource,
    TextResourceHandle, TextResourceId, TextSourceKind, Vec2, VectorPath,
};
use std::sync::Arc;

fn image_resource(
    handle: u64,
    width: u32,
) -> (RasterImageResourceHandle, Arc<RasterImageResource>) {
    (
        RasterImageResourceHandle {
            arena: 1,
            id: RasterImageResourceId::new(handle),
            version: 0,
        },
        Arc::new(
            RasterImageResource::from_rgba8(width, 1, vec![0; width as usize * 4]).unwrap(),
        ),
    )
}

#[test]
fn empty_projection_has_no_resource_charge() {
    assert_eq!(CompiledResources::default().prefix_resource_bytes(), Some(0));
}

#[test]
fn payload_size_not_entry_count_controls_resource_byte_charge() {
    let (handle, small) = image_resource(0, 1);
    let (_, large) = image_resource(0, 1024);
    let mut resources = CompiledResources::default();
    resources.images.insert(handle, small.clone());
    assert_eq!(resources.prefix_payload_count(), 1);
    assert_eq!(resources.prefix_resource_bytes(), Some(small.retained_bytes()));
    resources.images.insert(handle, large.clone());
    assert_eq!(resources.prefix_payload_count(), 1);
    assert_eq!(resources.prefix_resource_bytes(), Some(large.retained_bytes()));
    assert_eq!(large.retained_bytes() - small.retained_bytes(), 4092);
}

#[test]
fn image_font_and_geometry_payloads_sum_without_alias_map_double_counting() {
    let (image_handle, image) = image_resource(0, 4);
    let font_handle = FontResourceHandle {
        arena: 1,
        id: FontResourceId::new(0),
        version: 0,
    };
    let font = Arc::new(FontResource {
        key: FontResourceKey {
            face_key: Arc::from("test-face"),
            face_index: 0,
        },
        data: Arc::from([1_u8, 2, 3, 4]),
    });
    let path = GeometryResource::VectorPath(Arc::new(
        VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(1.0, 0.0)),
    ));
    let expected = image.retained_bytes() + font.retained_bytes() + path.retained_bytes();
    let mut arena = GeometryResourceArena::new();
    let geometry_handle = arena.insert(path);
    let mut resources = CompiledResources::default();
    resources.images.insert(image_handle, image);
    resources.fonts.insert(font_handle, font.clone());
    resources.font_handles.insert(font.key.clone(), font_handle);
    resources
        .capture_geometry_from_arena(&arena, geometry_handle)
        .unwrap();
    assert_eq!(resources.prefix_payload_count(), 3);
    assert_eq!(resources.prefix_resource_bytes(), Some(expected));
    resources.font_handles.clear();
    resources.geometry_handles.clear();
    assert_eq!(resources.prefix_resource_bytes(), Some(expected));
}

#[test]
fn repeated_handle_does_not_recharge_but_distinct_handles_are_conservative() {
    let (first, image) = image_resource(0, 4);
    let (second, _) = image_resource(1, 4);
    let mut resources = CompiledResources::default();
    resources.images.insert(first, image.clone());
    let mut duplicate = CompiledResources::default();
    duplicate.images.insert(first, image.clone());
    resources.extend_prefix_knowledge(&duplicate);
    assert_eq!(resources.prefix_resource_bytes(), Some(image.retained_bytes()));
    resources.images.insert(second, image.clone());
    assert_eq!(resources.prefix_resource_bytes(), Some(2 * image.retained_bytes()));
}

#[test]
fn text_resource_payload_is_included() {
    let text = Arc::new(TextResource {
        source: Arc::from("retained source"),
        kind: TextSourceKind::Plain,
        runs: Arc::from([]),
        vector_items: Arc::from([]),
        render_items: Arc::from([]),
        parts: Arc::from([]),
        bounds: Rect::new(Vec2::ZERO, Vec2::ONE),
        baseline: 0.0,
        layout_artifact: None,
    });
    let mut resources = CompiledResources::default();
    let handle = TextResourceHandle {
        arena: 1,
        id: TextResourceId::new(0),
        version: 0,
    };
    resources.texts.insert(handle, text.clone());
    assert_eq!(resources.prefix_resource_bytes(), Some(text.retained_bytes()));
    assert_eq!(
        text.retained_bytes(),
        std::mem::size_of::<TextResource>() + text.source.len()
    );
}
