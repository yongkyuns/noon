//! Identity-free, bounded selection presentation at the genuine worker boundary.
//!
//! Each retained publication carries the complete optional overlay. Absence clears
//! it; it is never a scene row, resource, painter anchor, or selected-target owner.

#[cfg(all(feature = "renderer", any(target_arch = "wasm32", test)))]
use noon_core::Vec2;
use noon_core::{Color, GeometryRef, Transform2D};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SelectionOverlayGeometry {
    Circle { radius: f32 },
    Rectangle { width: f32, height: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionOverlayPresentation {
    pub geometry: SelectionOverlayGeometry,
    pub transform: Transform2D,
    pub color: Color,
}

impl SelectionOverlayPresentation {
    pub(crate) fn from_presentation(
        presentation: &noon::integration::PointerSelectionPresentation,
    ) -> Result<Self, crate::RetainedFamilyExecutionTransportError> {
        let geometry = match &presentation.geometry {
            GeometryRef::Circle { radius } => SelectionOverlayGeometry::Circle { radius: *radius },
            GeometryRef::Rectangle { size } => SelectionOverlayGeometry::Rectangle {
                width: size.x,
                height: size.y,
            },
            _ => return Err(crate::RetainedFamilyExecutionTransportError::InvalidSelectionOverlay),
        };
        let value = Self {
            geometry,
            transform: presentation.transform,
            color: presentation.color,
        };
        value.validate()?;
        Ok(value)
    }

    pub(crate) fn validate(&self) -> Result<(), crate::RetainedFamilyExecutionTransportError> {
        let positive = |value: f32| value.is_finite() && value > 0.0;
        let geometry_valid = match self.geometry {
            SelectionOverlayGeometry::Circle { radius } => positive(radius),
            SelectionOverlayGeometry::Rectangle { width, height } => {
                positive(width) && positive(height)
            }
        };
        let transform = self.transform;
        if ![
            self.color.red,
            self.color.green,
            self.color.blue,
            self.color.alpha,
        ]
        .into_iter()
        .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
            || !geometry_valid
            || ![
                transform.translation.x,
                transform.translation.y,
                transform.scale.x,
                transform.scale.y,
                transform.rotation,
            ]
            .into_iter()
            .all(f32::is_finite)
            || transform.scale.x == 0.0
            || transform.scale.y == 0.0
        {
            return Err(crate::RetainedFamilyExecutionTransportError::InvalidSelectionOverlay);
        }
        Ok(())
    }

    #[cfg(all(feature = "renderer", any(target_arch = "wasm32", test)))]
    pub(crate) fn prepare(
        &self,
    ) -> Result<noon_render_wgpu::AnalyticOverlay, noon_render_wgpu::OverlayPrepareError> {
        let geometry = match self.geometry {
            SelectionOverlayGeometry::Circle { radius } => GeometryRef::Circle { radius },
            SelectionOverlayGeometry::Rectangle { width, height } => GeometryRef::Rectangle {
                size: Vec2::new(width, height),
            },
        };
        noon_render_wgpu::AnalyticOverlay::new(&geometry, self.transform, self.color)
    }
}

#[cfg(test)]
mod tests;
