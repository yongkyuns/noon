use std::ops::{Deref, DerefMut};

use noon_core::{SceneDefinition, DEFAULT_FRAME_HEIGHT, DEFAULT_FRAME_WIDTH};

use crate::legacy::{AuthoringError, Mobject, Rectangle, Scene};

/// Rust authoring facade for a scene with a shared semantic 2D camera frame.
///
/// The frame is an ordinary Noon mobject. Its motion is authored through the same
/// `Animate`/Transform timeline as every other object; converting the facade into a
/// `SceneDefinition` only assigns the shared camera role understood by the Rust
/// execution/render pipeline.
#[derive(Clone, Debug, PartialEq)]
pub struct MovingCameraScene {
    scene: Scene,
    frame: Mobject,
}

impl MovingCameraScene {
    pub fn new() -> Self {
        let mut scene = Scene::new();
        let frame =
            scene.add(Rectangle::new(DEFAULT_FRAME_WIDTH, DEFAULT_FRAME_HEIGHT).set_opacity(0.0));
        Self { scene, frame }
    }

    pub const fn camera_frame(&self) -> Mobject {
        self.frame
    }

    pub fn into_definition(self) -> SceneDefinition {
        let frame = self.frame;
        let mut definition = self.scene.into_definition();
        let assigned = definition.set_camera_object(frame.id());
        debug_assert!(assigned, "camera frame must belong to the authored scene");
        definition
    }

    pub fn try_into_definition(self) -> Result<SceneDefinition, AuthoringError> {
        let frame = self.frame;
        let mut definition = self.scene.into_definition();
        if !definition.set_camera_object(frame.id()) {
            return Err(AuthoringError::UnknownObject(frame.id()));
        }
        Ok(definition)
    }
}

impl Default for MovingCameraScene {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for MovingCameraScene {
    type Target = Scene;

    fn deref(&self) -> &Self::Target {
        &self.scene
    }
}

impl DerefMut for MovingCameraScene {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.scene
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{Property, Vec2, RIGHT};

    use super::*;

    #[test]
    fn rust_moving_camera_facade_uses_ordinary_transform_tracks() {
        let mut scene = MovingCameraScene::new();
        let frame = scene.camera_frame();
        scene
            .play(frame.animate().move_to(RIGHT * 3.0))
            .run_time(1.0)
            .unwrap();

        let definition = scene.into_definition();
        assert_eq!(definition.camera_object(), Some(frame.id()));
        let tracks = definition
            .tracks()
            .iter()
            .filter(|track| track.object == frame.id())
            .collect::<Vec<_>>();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].property, Property::Transform);
        let target = match &tracks[0].values {
            noon_core::TrackValues::Object { to, .. } => to,
            other => panic!("camera frame must use shared transform track, got {other:?}"),
        };
        assert_eq!(target.transform.translation, Vec2::new(3.0, 0.0));
        assert_eq!(target.style.opacity, 0.0);
    }
}
