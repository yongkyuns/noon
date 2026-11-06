use super::*;
use crate::replay_revision::SavedRow;
use crate::{CompiledObject, CompiledReplayRevision};
use noon_core::{
    DRect, DVec2, FontFaceIdentity, FontResourceArena, GeometryResourceArena, GlyphRun,
    ObjectContentRef, ObjectId, Style, TextAffineTransform, TextDirection, TextResource,
    TextResourceArena, TextSourceKind, Transform2D,
};
use std::sync::Arc;

#[test]
fn historical_latex_text_prefix_retains_its_font_closure() {
    let face = FontFaceIdentity {
        family: Arc::from("Test math font"),
        face_key: Arc::from("test-math-font"),
        face_index: 0,
        variation_key: Arc::from(""),
    };
    let mut fonts = FontResourceArena::new();
    let font_handle = fonts
        .intern_face(&face, Arc::<[u8]>::from([3_u8, 1, 4]))
        .unwrap();
    let font = fonts.get_shared(font_handle).unwrap();
    let weak_font = Arc::downgrade(&font);
    drop(font);

    let mut texts = TextResourceArena::new();
    let text_handle = texts
        .insert(TextResource {
            source: Arc::from("x²"),
            kind: TextSourceKind::MathTex,
            runs: Arc::from([GlyphRun {
                font: face,
                outline_profile: noon_core::GlyphOutlineProfile::Font,
                variations: Arc::from([]),
                font_size: 12.0,
                direction: TextDirection::LeftToRight,
                fill: None,
                stroke: None,
                transform: TextAffineTransform::IDENTITY,
                glyphs: Arc::from([]),
            }]),
            vector_items: Arc::from([]),
            render_items: Arc::from([noon_core::TextRenderItem::GlyphRun(0)]),
            parts: Arc::from([]),
            bounds: DRect::new(DVec2::ZERO, DVec2::ONE),
            baseline: 0.0,
            layout_artifact: None,
        })
        .unwrap();

    let mut scene = CompiledScene::compile_objects(Vec::new(), &[]).unwrap();
    let mut prepared = CompiledResources::default();
    prepared
        .capture_text_from_arenas(&texts, &fonts, &GeometryResourceArena::new(), text_handle)
        .unwrap();
    scene.merge_prepared_resources(prepared);
    let row = CompiledObject::new(
        ObjectId::new(0),
        ObjectContentRef::Text(text_handle),
        Transform2D::IDENTITY,
        Style::default(),
    );
    let revision = CompiledReplayRevision {
        rows: vec![SavedRow {
            index: 0,
            object: Some(row),
            order: None,
        }],
        inset_2d_view: None,
        channels: Vec::new(),
        tracks: Vec::new(),
        camera_motion_payloads: 0,
        path_motion_commands: 0,
        geometry_payload_bytes: 0,
        effect_payload_bytes: 0,
        composed_payload_bytes: 0,
    };

    drop((fonts, texts));
    let delta = scene
        .prefix_resource_delta(&[revision], &CompiledResources::default())
        .unwrap();
    assert_eq!(delta.text_count(), 1);
    assert_eq!(delta.font_count(), 1);
    assert_eq!(delta.prefix_payload_count(), 2);
    assert!(weak_font.upgrade().is_some());
    drop(delta);
    drop(scene);
    assert!(weak_font.upgrade().is_none());
}
