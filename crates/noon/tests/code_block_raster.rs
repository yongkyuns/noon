//! Actual retained-renderer qualification for native Code blocks. This checks
//! Noon's implemented profile; it is not a pinned Manim raster oracle.
#![cfg(all(feature = "code", feature = "bundled-fonts", feature = "image-decode"))]

use noon::{Code, CodeBackground, CodeOptions, CodeParagraphConfig, Scene};

mod raster_support;
use raster_support::{Raster, SIZE, WORLD_SIZE};

fn retain(name: &str, pixels: &[u8], width: u32, height: u32) {
    if let Some(directory) = std::env::var_os("NOON_CODE_PROOF_DIR") {
        std::fs::create_dir_all(&directory).unwrap();
        image::save_buffer(
            std::path::Path::new(&directory).join(format!("{name}.png")),
            pixels,
            width,
            height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}

fn assert_pixels_equal(actual: &[u8], expected: &[u8], label: &str) {
    assert_eq!(actual.len(), expected.len());
    if actual == expected {
        return;
    }
    let pairs = || {
        actual
            .as_chunks::<4>()
            .0
            .iter()
            .zip(expected.as_chunks::<4>().0)
    };
    let count = pairs()
        .filter(|(actual, expected)| actual != expected)
        .count();
    let first = pairs()
        .enumerate()
        .find(|(_, (actual, expected))| actual != expected);
    panic!("{label}: {count} changed pixels; first mismatch {first:?}");
}

fn assert_same_marker_region(before: &[u8], after: &[u8], size: u32) {
    let x = ((3.2 / WORLD_SIZE + 0.5) * size as f32) as usize;
    let y = ((0.5 - 3.2 / WORLD_SIZE) * size as f32) as usize;
    for row in y - 8..=y + 8 {
        for column in x - 8..=x + 8 {
            let offset = (row * size as usize + column) * 4;
            assert_eq!(&before[offset..offset + 4], &after[offset..offset + 4]);
        }
    }
}

#[test]
#[ignore = "requires software Vulkan; run with --ignored --nocapture"]
fn highlighted_code_retains_pixels_and_obeys_live_family_edits() {
    let mut scene = Scene::new();
    let mut marker = scene.square(0.6).unwrap();
    marker.set_fill(0.0, 0.7, 1.0, 1.0).unwrap();
    marker.set_stroke_width(0.0).unwrap();
    marker.set_translation(3.2, 3.2).unwrap();
    scene.add(&marker).unwrap();
    let code = Code::new(
        &mut scene,
        "def twice(x):\n    return x * 2",
        CodeOptions {
            language: Some("python".into()),
            formatter_style: "monokai".into(),
            background: CodeBackground::Window,
            paragraph_config: CodeParagraphConfig {
                font_size: 28.0,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    let line_handles: Vec<_> = (0..2)
        .map(|index| code.line(index).unwrap().state().unwrap().content)
        .collect();
    let marker_state = marker.state().unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut raster = pollster::block_on(Raster::new());
    let marker_only = raster.capture(&session.take_renderer_publication());

    scene
        .live(&mut session)
        .add_many(&[code.family().into()])
        .unwrap();
    let original = raster.capture(&session.take_renderer_publication());
    assert!(
        original != marker_only,
        "Code must render through retained text and geometry"
    );
    assert_same_marker_region(&marker_only, &original, SIZE);
    let text_bounds = code.code_lines().layout_bounds().unwrap().unwrap();
    let mut colorful = [0usize; 3];
    for (index, [red, green, blue, _]) in original.as_chunks::<4>().0.iter().enumerate() {
        let x = ((index % SIZE as usize) as f64 + 0.5) / f64::from(SIZE) * f64::from(WORLD_SIZE)
            - f64::from(WORLD_SIZE) * 0.5;
        let y = f64::from(WORLD_SIZE) * 0.5
            - ((index / SIZE as usize) as f64 + 0.5) / f64::from(SIZE) * f64::from(WORLD_SIZE);
        if x < text_bounds.min_x
            || x > text_bounds.max_x
            || y < text_bounds.min_y
            || y > text_bounds.max_y
        {
            continue;
        }
        if *red > 130 && u16::from(*red) > u16::from(*green) + 35 {
            colorful[0] += 1;
        }
        if *green > 130 && u16::from(*green) > u16::from(*red) + 30 {
            colorful[1] += 1;
        }
        if *blue > 130 && u16::from(*blue) > u16::from(*red) + 35 {
            colorful[2] += 1;
        }
    }
    assert!(
        colorful.into_iter().all(|count| count > 3),
        "syntax colors must survive actual rasterization: {colorful:?}"
    );
    let repeated = raster.capture(&session.take_renderer_publication());
    assert_pixels_equal(&repeated, &original, "a static redraw must be pixel-stable");
    assert_eq!(raster.last_scene_upload_bytes, 0);
    assert_eq!(raster.last_scene_repacked, 0);
    retain("code-original", &original, SIZE, SIZE);

    scene
        .live(&mut session)
        .shift_family(code.family(), 0.5, -0.5)
        .unwrap();
    let shifted = raster.capture(&session.take_renderer_publication());
    assert!(
        shifted != original,
        "ordinary live family translation must move Code pixels"
    );
    assert_same_marker_region(&original, &shifted, SIZE);
    assert_eq!(marker.state().unwrap(), marker_state);
    for (index, handle) in line_handles.iter().enumerate() {
        assert_eq!(&code.line(index).unwrap().state().unwrap().content, handle);
    }
    retain("code-shifted", &shifted, SIZE, SIZE);

    scene
        .live(&mut session)
        .set_family_opacity(code.family(), 0.0)
        .unwrap();
    let hidden = raster.capture(&session.take_renderer_publication());
    retain("code-hidden", &hidden, SIZE, SIZE);
    retain("code-marker-only", &marker_only, SIZE, SIZE);
    assert_pixels_equal(
        &hidden,
        &marker_only,
        "zero family opacity must hide background, numbers, and intrinsic glyph colors",
    );
    scene
        .live(&mut session)
        .set_family_opacity(code.family(), 1.0)
        .unwrap();
    let restored = raster.capture(&session.take_renderer_publication());
    assert_pixels_equal(
        &restored,
        &shifted,
        "restoring opacity must retain syntax paint and the live transform",
    );
    assert_pixels_equal(
        &raster.capture(&session.take_renderer_publication()),
        &restored,
        "restored frame must settle",
    );
    assert_eq!(raster.last_scene_upload_bytes, 0);
    eprintln!("Code retained raster: syntax_pixels={colorful:?}, size={SIZE}x{SIZE}, static_uploads=0, family_hide_restore=exact");
}

#[test]
#[ignore = "requires software Vulkan; run with --ignored --nocapture"]
fn paired_code_example_renders_python_and_cuda_in_the_native_engine() {
    let mut session = noon::example_scenes::code_block::session().unwrap();
    let mut raster = pollster::block_on(Raster::with_dimensions(1280, 720));
    let pixels = raster.capture(&session.take_renderer_publication());
    let changed = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[..3] != [0, 0, 0])
        .count();
    assert!(
        changed > 20_000,
        "the paired scene must contain substantial visible text and panels"
    );
    assert_pixels_equal(
        &raster.capture(&session.take_renderer_publication()),
        &pixels,
        "paired scene must settle",
    );
    assert_eq!(raster.last_scene_upload_bytes, 0);
    retain("code-blocks-native", &pixels, 1280, 720);
    eprintln!(
        "Code paired native raster: changed_pixels={changed}, size=1280x720, repeated_frame=exact"
    );
}
