use noon_core::Vec2;
use noon_render_wgpu::{Camera2D, SecondaryViewport, SecondaryViewportError};

#[test]
fn secondary_viewport_is_a_bounded_composition_descriptor() {
    let camera = Camera2D::new(Vec2::new(2.0, -1.0), Vec2::new(4.0, 3.0)).unwrap();
    let viewport = SecondaryViewport::new(camera, [640, 360, 320, 180], [1280, 720]).unwrap();
    assert_eq!(viewport.camera, camera);
    assert_eq!(viewport.destination, [640, 360, 320, 180]);
}

#[test]
fn secondary_viewport_rejects_empty_overflowing_and_out_of_bounds_destinations() {
    let camera = Camera2D::DEFAULT;
    assert_eq!(
        SecondaryViewport::new(camera, [0, 0, 0, 10], [100, 100]).unwrap_err(),
        SecondaryViewportError::EmptyDestination
    );
    assert_eq!(
        SecondaryViewport::new(camera, [90, 0, 11, 10], [100, 100]).unwrap_err(),
        SecondaryViewportError::DestinationOutOfBounds
    );
    assert_eq!(
        SecondaryViewport::new(camera, [u32::MAX, 0, 2, 1], [u32::MAX, 1]).unwrap_err(),
        SecondaryViewportError::DestinationOutOfBounds
    );
}
