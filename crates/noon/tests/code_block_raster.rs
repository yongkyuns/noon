//! Actual retained-renderer qualification for native Code blocks. This checks
//! Noon's implemented profile; it is not a pinned Manim raster oracle.
#![cfg(all(feature = "code", feature = "bundled-fonts", feature = "image-decode"))]

use noon::{Code, CodeBackground, CodeHighlightOptions, CodeOptions, CodeParagraphConfig, Scene};

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
        "def twice(x):\n\n    return x * 2",
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
    let line_handles: Vec<_> = (0..3)
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

    let focus = code
        .highlight_lines_in_live_session(
            &mut scene.live(&mut session),
            1..2,
            CodeHighlightOptions::default(),
        )
        .unwrap();
    let focused = raster.capture(&session.take_renderer_publication());
    let upper = code.line(0).unwrap().layout_bounds().unwrap().unwrap();
    let lower = code.line(2).unwrap().layout_bounds().unwrap().unwrap();
    let mut blank_row_pixels = 0;
    for (index, (before, after)) in original
        .as_chunks::<4>()
        .0
        .iter()
        .zip(focused.as_chunks::<4>().0)
        .enumerate()
    {
        if before == after {
            continue;
        }
        let y = f64::from(WORLD_SIZE) * 0.5
            - (index / SIZE as usize) as f64 / f64::from(SIZE) * f64::from(WORLD_SIZE);
        assert!(
            y < upper.min_y && y > lower.max_y,
            "blank-row highlight must stay between the neighboring text rows"
        );
        blank_row_pixels += 1;
    }
    assert!(
        blank_row_pixels > 100,
        "an empty text row must have a visible band"
    );
    assert_same_marker_region(&original, &focused, SIZE);
    retain("code-blank-row-highlight", &focused, SIZE, SIZE);
    scene
        .live(&mut session)
        .remove_family_members(code.highlights(), &[(&focus).into()])
        .unwrap();
    assert_pixels_equal(
        &raster.capture(&session.take_renderer_publication()),
        &original,
        "removing one highlight must restore the original listing exactly",
    );
    assert_eq!(code.line_sources().unwrap().len(), 3);

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
fn logical_row_example_preserves_glyphs_above_rotated_scaled_highlights() {
    let mut control = noon::example_scenes::code_line_highlight::session(false).unwrap();
    let mut highlighted = noon::example_scenes::code_line_highlight::session(true).unwrap();
    // A retained renderer belongs to one execution publication stream. Render
    // the independent control in its own context, just like the browser pair.
    let before = {
        let mut control_raster = pollster::block_on(Raster::with_dimensions(960, 540));
        control_raster.capture(&control.take_renderer_publication())
    };
    let mut raster = pollster::block_on(Raster::with_dimensions(960, 540));
    let after = raster.capture(&highlighted.take_renderer_publication());
    retain("code-line-control-native", &before, 960, 540);
    retain("code-line-highlights-native", &after, 960, 540);
    let before_pixels = before.as_chunks::<4>().0;
    let after_pixels = after.as_chunks::<4>().0;
    let mut colors = std::collections::HashMap::new();
    for pixel in before_pixels.iter().filter(|pixel| pixel[..3] != [0, 0, 0]) {
        *colors.entry(*pixel).or_insert(0usize) += 1;
    }
    let (panel, panel_pixels) = colors.into_iter().max_by_key(|(_, count)| *count).unwrap();
    assert!(panel_pixels > 2_000 && panel[..3].iter().all(|value| *value < 128));
    let background_blue_gain = before_pixels
        .iter()
        .zip(after_pixels)
        .filter(|(before, _)| **before == panel)
        .map(|(_, after)| after[2].saturating_sub(panel[2]))
        .max()
        .unwrap();
    assert!(
        background_blue_gain > 0,
        "yellow bands must alter the panel background"
    );
    let mut changed = 0;
    let mut glyph_pixels_above_band = 0;
    for (index, (before, after)) in before_pixels.iter().zip(after_pixels).enumerate() {
        changed += usize::from(before != after);
        let minimum = *before[..3].iter().min().unwrap();
        let maximum = *before[..3].iter().max().unwrap();
        if minimum < 150 || maximum - minimum > 16 {
            continue;
        }
        let (x, y) = ((index % 960) as i32, (index / 960) as i32);
        let near_band = (-2..=2).any(|dy| {
            (-2..=2).any(|dx| {
                let (nx, ny) = (x + dx, y + dy);
                if !(0..960).contains(&nx) || !(0..540).contains(&ny) {
                    return false;
                }
                let neighbor = ny as usize * 960 + nx as usize;
                before_pixels[neighbor] == panel && after_pixels[neighbor][2] > panel[2]
            })
        });
        if near_band {
            // SDF glyphs need not contain any fully opaque pixel at this size.
            // The default yellow's blue lies between the dark panel and these
            // bright glyphs. An underlay only increases background blue; an
            // overlay would decrease glyph blue instead.
            assert!(
                before[2].saturating_sub(after[2]) <= 2,
                "a highlight must remain below antialiased source glyphs"
            );
            glyph_pixels_above_band += 1;
        }
    }
    assert!(
        changed > 2_000,
        "both highlights must render over the opaque panel"
    );
    assert!(
        glyph_pixels_above_band > 20,
        "glyphs within the highlighted area must be sampled"
    );
    assert_pixels_equal(
        &raster.capture(&highlighted.take_renderer_publication()),
        &after,
        "the highlighted scene must settle",
    );
    assert_eq!(raster.last_scene_upload_bytes, 0);
    assert_eq!(raster.last_scene_repacked, 0);
    eprintln!("Code logical-row raster: changed={changed}, glyphs_above_band={glyph_pixels_above_band}, background_blue_gain={background_blue_gain}, static_uploads=0");
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
