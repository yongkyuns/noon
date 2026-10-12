use super::*;

const LIMIT: u64 = 16 * 1024 * 1024;

fn pixel(x: u32, y: u32) -> [u8; 4] {
    let alpha = [0, 1, 64, 128, 254, 255][((x + 2 * y) % 6) as usize];
    [
        (x % 193) as u8,
        (y % 173) as u8,
        ((x + y) % 211) as u8,
        alpha,
    ]
}

fn mapped_fixture(
    layout: Rgba8ReadbackLayout,
    channels: PixelChannelOrder,
    rows: PixelRowOrder,
) -> (Vec<u8>, Vec<u8>) {
    let mut source = vec![0xdd; layout.buffer_len()];
    let mut expected = Vec::new();
    for y in 0..layout.height() {
        let source_y = match rows {
            PixelRowOrder::TopToBottom => y,
            PixelRowOrder::BottomToTop => layout.height() - 1 - y,
        };
        for x in 0..layout.width() {
            let rgba = pixel(x, y);
            expected.extend_from_slice(&rgba);
            let mut encoded = rgba;
            if channels == PixelChannelOrder::Bgra {
                encoded.swap(0, 2);
            }
            let offset =
                source_y as usize * layout.padded_bytes_per_row() as usize + x as usize * 4;
            source[offset..offset + 4].copy_from_slice(&encoded);
        }
    }
    (source, expected)
}

#[test]
fn dimensions_must_be_positive() {
    for (width, height) in [(0, 0), (0, 1), (1, 0)] {
        assert!(matches!(
            Rgba8ReadbackLayout::new(width, height, 256, LIMIT),
            Err(PixelReadbackError::InvalidDimensions)
        ));
    }
}

#[test]
fn alignment_is_explicit_and_validated() {
    for alignment in [0, 3, 255, u32::MAX] {
        assert!(matches!(
            Rgba8ReadbackLayout::new(17, 3, alignment, LIMIT),
            Err(PixelReadbackError::InvalidAlignment)
        ));
    }
    for alignment in [1, 4, 64, 256] {
        let layout = Rgba8ReadbackLayout::new(17, 3, alignment, LIMIT).unwrap();
        assert_eq!(layout.padded_bytes_per_row() % alignment, 0);
        assert!(layout.padded_bytes_per_row() >= layout.bytes_per_row());
    }
}

#[test]
fn widths_on_both_sides_of_copy_alignment_keep_exact_dimensions() {
    for (width, stride) in [
        (1, 256),
        (63, 256),
        (64, 256),
        (65, 512),
        (255, 1024),
        (256, 1024),
        (257, 1280),
        (1919, 7680),
        (1920, 7680),
        (1921, 7936),
    ] {
        let layout = Rgba8ReadbackLayout::new(width, 3, 256, LIMIT).unwrap();
        assert_eq!((layout.width(), layout.height()), (width, 3));
        assert_eq!(layout.bytes_per_row(), width * 4);
        assert_eq!(layout.padded_bytes_per_row(), stride);
        assert_eq!(layout.buffer_len(), stride as usize * 3);
        assert_eq!(layout.packed_len(), width as usize * 3 * 4);
        let (source, expected) =
            mapped_fixture(layout, PixelChannelOrder::Rgba, PixelRowOrder::TopToBottom);
        assert_eq!(
            layout
                .copy_rgba8(&source, PixelChannelOrder::Rgba, PixelRowOrder::TopToBottom)
                .unwrap(),
            expected
        );
    }
}

#[test]
fn budget_includes_padding_and_accepts_the_exact_limit() {
    let layout = Rgba8ReadbackLayout::new(65, 3, 256, 1536).unwrap();
    assert_eq!(layout.buffer_len(), 1536);
    for limit in [0, 65 * 3 * 4, 1535] {
        assert!(matches!(
            Rgba8ReadbackLayout::new(65, 3, 256, limit),
            Err(PixelReadbackError::BufferLimit {
                requested: 1536,
                ..
            })
        ));
    }
}

#[test]
fn row_and_address_space_overflows_fail_before_allocation() {
    for (width, height, alignment) in [
        (u32::MAX, 1, 1),
        (u32::MAX / 4, 1, 256),
        ((1 << 29) + 1, u32::MAX, 256),
    ] {
        assert!(matches!(
            Rgba8ReadbackLayout::new(width, height, alignment, u64::MAX),
            Err(PixelReadbackError::SizeOverflow)
        ));
    }
}

#[test]
fn channel_and_row_orders_normalize_non_square_images_without_color_conversion() {
    let layout = Rgba8ReadbackLayout::new(65, 7, 256, LIMIT).unwrap();
    for channels in [PixelChannelOrder::Rgba, PixelChannelOrder::Bgra] {
        for rows in [PixelRowOrder::TopToBottom, PixelRowOrder::BottomToTop] {
            let (source, expected) = mapped_fixture(layout, channels, rows);
            let original = source.clone();
            assert_eq!(
                layout.copy_rgba8(&source, channels, rows).unwrap(),
                expected
            );
            assert_eq!(source, original);
        }
    }
}

#[test]
fn one_pixel_and_single_rows_do_not_read_padding_as_pixels() {
    for (width, height) in [(1, 1), (1, 9), (17, 1)] {
        let layout = Rgba8ReadbackLayout::new(width, height, 256, LIMIT).unwrap();
        let (source, expected) =
            mapped_fixture(layout, PixelChannelOrder::Bgra, PixelRowOrder::BottomToTop);
        assert_eq!(
            layout
                .copy_rgba8(&source, PixelChannelOrder::Bgra, PixelRowOrder::BottomToTop)
                .unwrap(),
            expected
        );
    }
}

#[test]
fn source_length_errors_leave_reusable_destination_unchanged() {
    let layout = Rgba8ReadbackLayout::new(65, 3, 256, LIMIT).unwrap();
    for length in [0, layout.buffer_len() - 1, layout.buffer_len() + 1] {
        let mut destination = vec![0x7b; layout.packed_len()];
        assert!(matches!(
            layout.copy_rgba8_into(
                &vec![0; length],
                PixelChannelOrder::Rgba,
                PixelRowOrder::TopToBottom,
                &mut destination,
            ),
            Err(PixelReadbackError::SourceLength { .. })
        ));
        assert!(destination.iter().all(|&byte| byte == 0x7b));
    }
}

#[test]
fn destination_length_errors_do_not_partially_copy() {
    let layout = Rgba8ReadbackLayout::new(65, 3, 256, LIMIT).unwrap();
    let source = vec![1; layout.buffer_len()];
    for length in [0, layout.packed_len() - 1, layout.packed_len() + 1] {
        let mut destination = vec![0x7b; length];
        assert!(matches!(
            layout.copy_rgba8_into(
                &source,
                PixelChannelOrder::Rgba,
                PixelRowOrder::TopToBottom,
                &mut destination,
            ),
            Err(PixelReadbackError::DestinationLength { .. })
        ));
        assert!(destination.iter().all(|&byte| byte == 0x7b));
    }
}

#[test]
fn owned_pixels_survive_source_buffer_reuse() {
    let layout = Rgba8ReadbackLayout::new(65, 3, 256, LIMIT).unwrap();
    let (mut source, expected) =
        mapped_fixture(layout, PixelChannelOrder::Rgba, PixelRowOrder::TopToBottom);
    let pixels = layout
        .copy_rgba8(&source, PixelChannelOrder::Rgba, PixelRowOrder::TopToBottom)
        .unwrap();
    source.fill(0);
    assert_eq!(pixels, expected);
    let next = layout
        .copy_rgba8(&source, PixelChannelOrder::Rgba, PixelRowOrder::TopToBottom)
        .unwrap();
    assert!(next.iter().all(|&byte| byte == 0));
    assert_eq!(pixels, expected);
}

#[test]
fn caller_owned_buffer_can_be_reused_without_growth() {
    let layout = Rgba8ReadbackLayout::new(65, 3, 256, LIMIT).unwrap();
    let mut destination = vec![0; layout.packed_len()];
    let capacity = destination.capacity();
    let address = destination.as_ptr();
    for channels in [PixelChannelOrder::Rgba, PixelChannelOrder::Bgra] {
        let (source, expected) = mapped_fixture(layout, channels, PixelRowOrder::BottomToTop);
        layout
            .copy_rgba8_into(
                &source,
                channels,
                PixelRowOrder::BottomToTop,
                &mut destination,
            )
            .unwrap();
        assert_eq!(destination, expected);
        assert_eq!(destination.capacity(), capacity);
        assert_eq!(destination.as_ptr(), address);
    }
}

#[test]
fn alpha_and_rgb_under_zero_alpha_are_not_silently_modified() {
    let layout = Rgba8ReadbackLayout::new(2, 1, 1, LIMIT).unwrap();
    let source = [19, 7, 233, 0, 199, 122, 13, 128];
    assert_eq!(
        layout
            .copy_rgba8(&source, PixelChannelOrder::Rgba, PixelRowOrder::TopToBottom)
            .unwrap(),
        source
    );
}
