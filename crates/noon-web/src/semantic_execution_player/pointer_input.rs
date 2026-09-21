//! Browser control-port normalization. The collector supplies a source lifetime,
//! actual DOM pointer ID and immutable logical viewport coordinates. Only one
//! source is projected into the session's current unkeyed native pointer signals.
//! This is not a picking/capture owner or a historical presented-frame cache.

use super::SemanticExecutionPlayer;
use noon_core::{
    Camera2DState, NativeInputModifiers, NativePointerCancellation, NativePointerId,
    NativePointerInput, NativePointerInputKind, NativePointerPosition, Vec2,
};
use serde::Deserialize;

const MAX_JS_INTEGER: u64 = (1_u64 << 53) - 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct BrowserPointerBinding {
    pointer: NativePointerId,
    view_revision: u64,
    viewport: Vec2,
    retired: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BrowserPointerInputWire {
    kind: BrowserPointerKindWire,
    source_id: u64,
    pointer_id: i32,
    surface_x: Option<f32>,
    surface_y: Option<f32>,
    viewport_width: Option<f32>,
    viewport_height: Option<f32>,
    button: Option<u8>,
    view_revision: u64,
    #[serde(default)]
    shift: bool,
    #[serde(default)]
    control: bool,
    #[serde(default)]
    alt: bool,
    #[serde(default)]
    meta: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BrowserPointerKindWire {
    Move,
    Press,
    Release,
    Cancel,
    FocusLost,
    CaptureLost,
}

impl BrowserPointerInputWire {
    fn cancellation(self) -> Option<NativePointerCancellation> {
        match self.kind {
            BrowserPointerKindWire::Cancel => Some(NativePointerCancellation::Cancelled),
            BrowserPointerKindWire::FocusLost => Some(NativePointerCancellation::FocusLost),
            BrowserPointerKindWire::CaptureLost => Some(NativePointerCancellation::CaptureLost),
            _ => None,
        }
    }

    fn pointer(self) -> Result<NativePointerId, String> {
        if self.source_id == 0
            || self.source_id > MAX_JS_INTEGER
            || self.view_revision > MAX_JS_INTEGER
        {
            return Err(
                "browser pointer source/view must fit the exact JavaScript integer domain".into(),
            );
        }
        if self.pointer_id == -1 {
            return Err("browser pointer ID -1 does not identify a pointing device".into());
        }
        // DOM pointerId is a signed long. Preserve all its bits (including other
        // negative IDs) without merging contacts or using a semantic identity.
        Ok(NativePointerId {
            source: self.source_id,
            pointer: u64::from(self.pointer_id.cast_unsigned()),
        })
    }

    fn coordinates(self) -> Result<Option<(Vec2, Vec2)>, String> {
        if self.cancellation().is_some() {
            if self.surface_x.is_some()
                || self.surface_y.is_some()
                || self.viewport_width.is_some()
                || self.viewport_height.is_some()
                || self.button.is_some()
            {
                return Err(
                    "browser pointer cancellation must not contain position or button data".into(),
                );
            }
            return Ok(None);
        }
        match self.kind {
            BrowserPointerKindWire::Press | BrowserPointerKindWire::Release
                if self.button.is_none() =>
            {
                return Err("browser pointer edge is missing its button".into())
            }
            BrowserPointerKindWire::Move if self.button.is_some() => {
                return Err("browser pointer motion must not contain an edge button".into())
            }
            _ => {}
        }
        let surface = Vec2::new(
            self.surface_x.ok_or("browser pointer x is missing")?,
            self.surface_y.ok_or("browser pointer y is missing")?,
        );
        let viewport = Vec2::new(
            self.viewport_width
                .ok_or("browser pointer width is missing")?,
            self.viewport_height
                .ok_or("browser pointer height is missing")?,
        );
        if !surface.x.is_finite()
            || !surface.y.is_finite()
            || !viewport.x.is_finite()
            || !viewport.y.is_finite()
            || viewport.x <= 0.0
            || viewport.y <= 0.0
        {
            return Err("browser pointer coordinates and positive viewport must be finite".into());
        }
        Ok(Some((surface, viewport)))
    }
}

fn position(
    surface: Vec2,
    viewport: Vec2,
    camera: Camera2DState,
) -> Result<NativePointerPosition, String> {
    let scene = Vec2::new(
        camera.center.x
            + (surface.x / viewport.x - 0.5) * camera.height * (viewport.x / viewport.y),
        camera.center.y + (0.5 - surface.y / viewport.y) * camera.height,
    );
    NativePointerPosition::new(scene, surface).map_err(|error| error.to_string())
}

impl SemanticExecutionPlayer {
    pub(super) fn submit_browser_pointer_input(
        &mut self,
        wire: BrowserPointerInputWire,
    ) -> Result<(), String> {
        // Validate the entire wire record before even configuring a source. A bad
        // press, overflowed coordinate or exhausted sequence must not clear buttons.
        let pointer = wire.pointer()?;
        let coordinates = wire.coordinates()?;
        let sequence = self.next_native_event_sequence;
        let next = sequence
            .checked_add(1)
            .ok_or("native input event sequence exhausted")?;
        let needs_binding = match self.browser_pointer_binding {
            None => true,
            Some(binding) => {
                if pointer.source < binding.pointer.source {
                    return Err("browser pointer source has been retired".into());
                }
                if pointer.source == binding.pointer.source {
                    if pointer != binding.pointer
                        || wire.view_revision != binding.view_revision
                        || binding.retired
                    {
                        return Err(
                            "browser pointer identity/view does not match its live source".into(),
                        );
                    }
                    if coordinates.is_some_and(|(_, viewport)| viewport != binding.viewport) {
                        return Err(
                            "browser pointer viewport changed without a new source/view".into()
                        );
                    }
                    false
                } else {
                    true
                }
            }
        };
        if needs_binding
            && !matches!(
                wire.kind,
                BrowserPointerKindWire::Move | BrowserPointerKindWire::Press
            )
        {
            return Err("browser pointer release/cancellation requires its existing source".into());
        }
        if let Some((surface, viewport)) = coordinates {
            // Catch arithmetic overflow before binding changes as well as raw NaN.
            position(
                surface,
                viewport,
                self.session.camera().map_err(|error| error.to_string())?,
            )?;
            if needs_binding {
                self.session
                    .configure_native_pointer_input(pointer, wire.view_revision)
                    .map_err(|error| error.to_string())?;
                self.browser_pointer_binding = Some(BrowserPointerBinding {
                    pointer,
                    view_revision: wire.view_revision,
                    viewport,
                    retired: false,
                });
            }
        }
        let token = self
            .session
            .native_pointer_input_token()
            .map_err(|error| error.to_string())?;
        if token.pointer() != pointer || token.context().view_revision != wire.view_revision {
            return Err("browser pointer session binding has been replaced".into());
        }
        let kind = if let Some(reason) = wire.cancellation() {
            NativePointerInputKind::Cancel(reason)
        } else {
            let (surface, viewport) = coordinates.expect("validated positional record");
            // Configuration can reset a button-driven camera. Use the effective
            // camera after that reset for both the coordinates and session token.
            let position = position(
                surface,
                viewport,
                self.session.camera().map_err(|error| error.to_string())?,
            )?;
            match wire.kind {
                BrowserPointerKindWire::Move => NativePointerInputKind::Move(position),
                BrowserPointerKindWire::Press => NativePointerInputKind::Press {
                    position,
                    button: wire.button.expect("validated edge"),
                },
                BrowserPointerKindWire::Release => NativePointerInputKind::Release {
                    position,
                    button: wire.button.expect("validated edge"),
                },
                _ => unreachable!("cancellation handled above"),
            }
        };
        let input = NativePointerInput::new(
            sequence,
            pointer,
            token.context(),
            NativeInputModifiers {
                shift: wire.shift,
                control: wire.control,
                alt: wire.alt,
                meta: wire.meta,
            },
            kind,
        );
        self.session
            .submit_native_pointer_input(&token, input)
            .map_err(|error| error.to_string())?;
        self.next_native_event_sequence = next;
        if wire.cancellation().is_some() {
            self.browser_pointer_binding
                .as_mut()
                .expect("configured source")
                .retired = true;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod selection_tests;
