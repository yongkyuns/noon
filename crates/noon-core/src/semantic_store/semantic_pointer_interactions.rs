//! Authored pointer actions. Platform collectors never choose targets or policy.
use crate::{Color, SemanticNodeId, YELLOW};

/// A primary-button click on a supported analytic fill plays the shared restoring
/// Indicate animation. A second click while an animation owns the session's
/// completion barrier is consumed without queuing or compounding that animation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerIndicateOptions {
    pub scale_factor: f64,
    pub color: Color,
    pub duration: f64,
    pub max_movement: f32,
}
impl Default for PointerIndicateOptions {
    fn default() -> Self {
        Self {
            scale_factor: 1.2,
            color: YELLOW,
            duration: 0.65,
            max_movement: 6.0,
        }
    }
}

/// Exponential, cursor-anchored camera zoom from normalized logical-pixel wheel
/// deltas. Heights are scene units; positive wheel Y zooms out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerZoomOptions {
    pub sensitivity: f64,
    pub min_height: f64,
    pub max_height: f64,
}
impl Default for PointerZoomOptions {
    fn default() -> Self {
        Self {
            sensitivity: 0.002,
            min_height: 0.5,
            max_height: 64.0,
        }
    }
}

/// The ordinary camera and two scene-global input signals which drive it. These
/// signals are Runtime input state, not a platform-owned copy of the camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticPointerZoom<N = SemanticNodeId> {
    pub camera: N,
    pub center_signal: N,
    pub scale_signal: N,
    pub options: PointerZoomOptions,
}

/// One root-scoped authored binding. The initial bounded vocabulary deliberately
/// contains no host callable, event queue, animation scheduler or renderer data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticPointerInteractions<N = SemanticNodeId> {
    pub indicate: Option<PointerIndicateOptions>,
    pub zoom: Option<SemanticPointerZoom<N>>,
}
impl<N> Default for SemanticPointerInteractions<N> {
    fn default() -> Self {
        Self {
            indicate: None,
            zoom: None,
        }
    }
}
impl<N: Copy> SemanticPointerInteractions<N> {
    pub fn references(&self) -> Vec<N> {
        self.zoom.map_or_else(Vec::new, |z| {
            vec![z.camera, z.center_signal, z.scale_signal]
        })
    }
    pub fn map<M>(self, mut map: impl FnMut(N) -> M) -> SemanticPointerInteractions<M> {
        SemanticPointerInteractions {
            indicate: self.indicate,
            zoom: self.zoom.map(|z| SemanticPointerZoom {
                camera: map(z.camera),
                center_signal: map(z.center_signal),
                scale_signal: map(z.scale_signal),
                options: z.options,
            }),
        }
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if let Some(i) = self.indicate {
            if !i.scale_factor.is_finite()
                || i.scale_factor <= 0.0
                || i.scale_factor > f64::from(f32::MAX)
                || !i.duration.is_finite()
                || i.duration <= 0.0
                || !i.max_movement.is_finite()
                || i.max_movement < 0.0
                || ![i.color.red, i.color.green, i.color.blue, i.color.alpha]
                    .iter()
                    .all(|v| v.is_finite())
            {
                return Err("Indicate requires finite positive scale/duration, finite color and nonnegative click tolerance");
            }
        }
        if let Some(z) = self.zoom {
            let o = z.options;
            if !o.sensitivity.is_finite()
                || o.sensitivity <= 0.0
                || !o.min_height.is_finite()
                || o.min_height <= 0.0
                || !o.max_height.is_finite()
                || o.max_height < o.min_height
                || o.max_height > f64::from(f32::MAX)
            {
                return Err(
                    "zoom requires finite positive sensitivity and ordered positive camera heights",
                );
            }
        }
        Ok(())
    }
    pub const fn enabled(&self) -> bool {
        self.indicate.is_some() || self.zoom.is_some()
    }
}
