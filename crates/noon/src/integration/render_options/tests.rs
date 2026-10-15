use super::*;

#[test]
fn pinned_manim_presets_and_default_are_shared_values() {
    for (quality, width, height, fps) in [
        ("l", 854, 480, 15),
        ("m", 1280, 720, 30),
        ("h", 1920, 1080, 60),
        ("p", 2560, 1440, 60),
        ("k", 3840, 2160, 60),
    ] {
        let resolved = RenderOptionInputs {
            quality: Some(quality),
            ..Default::default()
        }
        .resolve()
        .unwrap();
        assert_eq!(
            (resolved.pixel_width, resolved.pixel_height),
            (width, height)
        );
        assert_eq!(resolved.frame_rate, FrameRate::new(fps, 1).unwrap());
        assert_eq!(resolved.format, RenderFormat::Mp4);
    }
    let default = RenderOptionInputs::default().resolve().unwrap();
    let high = RenderOptionInputs {
        quality: Some("high_quality"),
        ..Default::default()
    }
    .resolve()
    .unwrap();
    assert_eq!(default, high);
}

#[test]
fn explicit_dimensions_and_rate_independently_override_quality() {
    let dimensions = RenderOptionInputs {
        quality: Some("l"),
        resolution: Some("1920,1080"),
        ..Default::default()
    }
    .resolve()
    .unwrap();
    assert_eq!(
        (dimensions.pixel_width, dimensions.pixel_height),
        (1920, 1080)
    );
    assert_eq!(dimensions.frame_rate, FrameRate::new(15, 1).unwrap());
    let fps = RenderOptionInputs {
        quality: Some("l"),
        frame_rate: Some("60000/1001"),
        ..Default::default()
    }
    .resolve()
    .unwrap();
    assert_eq!((fps.pixel_width, fps.pixel_height), (854, 480));
    assert_eq!(fps.frame_rate, FrameRate::new(60000, 1001).unwrap());
    let partial = RenderOptionInputs {
        pixel_height: Some(360),
        ..Default::default()
    }
    .resolve()
    .unwrap();
    assert_eq!((partial.pixel_width, partial.pixel_height), (1920, 360));
}

#[test]
fn numeric_rates_remain_exact_and_do_not_guess_ntsc() {
    for (source, p, q) in [
        ("60", 60, 1),
        ("60.0", 60, 1),
        ("29.97", 2997, 100),
        ("59.94", 2997, 50),
        ("60000/1001", 60000, 1001),
        ("+6e1", 60, 1),
        (".5", 1, 2),
        ("15.", 15, 1),
        ("2.997E1", 2997, 100),
        (" 120 / 2 ", 60, 1),
        ("4294967296/2", 2147483648, 1),
        ("1e-2", 1, 100),
        ("60.000000000000000000000000000000000000000000", 60, 1),
    ] {
        assert_eq!(
            parse_render_frame_rate(source).unwrap(),
            FrameRate::new(p, q).unwrap(),
            "{source}"
        );
    }
    assert_ne!(
        parse_render_frame_rate("29.97").unwrap(),
        FrameRate::new(30000, 1001).unwrap()
    );
}

#[test]
fn bad_rates_fail_instead_of_rounding_clamping_or_allocating_unbounded_input() {
    for source in [
        "",
        " ",
        "0",
        "0/1",
        "1/0",
        "NaN",
        "inf",
        "-1",
        "-2/-2",
        "1/2/3",
        "1.2.3",
        ".",
        "e1",
        "1e",
        "1e1e2",
        "1_000",
        "1 e2",
        "4294967296",
        "1/4294967296",
        "1e9999",
        "1e-9999",
        "1e38",
    ] {
        assert_eq!(
            parse_render_frame_rate(source),
            Err(RenderOptionsError::FrameRate),
            "{source}"
        );
    }
    assert!(parse_render_frame_rate(&"9".repeat(129)).is_err());
}

#[test]
fn resolutions_are_not_silently_rounded_or_resized() {
    for source in [
        "",
        "65",
        "65x33",
        "65,33,1",
        "0,33",
        "-1,33",
        "65.0,33",
        "4294967296,33",
    ] {
        assert!(RenderOptionInputs {
            resolution: Some(source),
            ..Default::default()
        }
        .resolve()
        .is_err());
    }
    assert_eq!(
        RenderOptionInputs {
            resolution: Some("65,33"),
            pixel_width: Some(65),
            ..Default::default()
        }
        .resolve(),
        Err(RenderOptionsError::ConflictingResolution)
    );
    let odd = RenderOptionInputs {
        resolution: Some(" 65, 33 "),
        format: Some("png"),
        ..Default::default()
    }
    .resolve()
    .unwrap();
    assert_eq!((odd.pixel_width, odd.pixel_height), (65, 33));
    assert_eq!(odd.format.name(), "png");
}

#[test]
fn unsupported_presets_and_formats_never_select_a_substitute() {
    for quality in ["", "1080p", "H", "low"] {
        assert_eq!(
            RenderOptionInputs {
                quality: Some(quality),
                ..Default::default()
            }
            .resolve(),
            Err(RenderOptionsError::Quality)
        );
    }
    for format in ["", "gif", "webm", "mov", "MP4"] {
        assert_eq!(
            RenderOptionInputs {
                format: Some(format),
                ..Default::default()
            }
            .resolve(),
            Err(RenderOptionsError::UnsupportedFormat)
        );
    }
}
