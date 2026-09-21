//! Browser control-port normalization. The collector supplies a source lifetime,
//! actual DOM pointer ID and immutable logical viewport coordinates. Only one
//! source is projected into the session's current unkeyed native pointer signals.
//! This is not a picking/capture owner or a historical presented-frame cache.

use noon::integration::{
    NativePointerInputPublication, NativePointerInputToken, PointerFrameError,
    PointerFrameSnapshot, PointerFrameView,
};
use noon::{ExecutionSession, LiveContinuation, LiveProgram};

/// Only the operations needed by browser pointer normalization. A live program
/// preserves its continuation barriers instead of exposing mutable session state.
pub(crate) trait BrowserPointerTarget {
    fn session(&self) -> &ExecutionSession;
    fn configure_pointer(
        &mut self,
        pointer: NativePointerId,
        view: u64,
    ) -> Result<NativePointerInputToken, String>;
    fn pointer_token(&self) -> Result<NativePointerInputToken, String>;
    fn submit_pointer(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<NativePointerInputPublication, String>;
}

/// Frame incompatibility is recoverable; malformed input and session/callback
/// transaction failures remain errors. Hosts must not classify arbitrary strings.
#[derive(Debug)]
pub(crate) enum BrowserPointerAdmissionError {
    Frame(PointerFrameError),
    Input(String),
}

impl std::fmt::Display for BrowserPointerAdmissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Frame(error) => error.fmt(f),
            Self::Input(error) => f.write_str(error),
        }
    }
}
impl From<String> for BrowserPointerAdmissionError {
    fn from(error: String) -> Self {
        Self::Input(error)
    }
}
impl From<&str> for BrowserPointerAdmissionError {
    fn from(error: &str) -> Self {
        Self::Input(error.into())
    }
}
impl From<PointerFrameError> for BrowserPointerAdmissionError {
    fn from(error: PointerFrameError) -> Self {
        Self::Frame(error)
    }
}

/// Worker processing-time normalization. Collection-time receipt transport and
/// removal of this unassociated entry are owned by #846. Direct hosts cannot use
/// this entry for positional input; their entry requires an actual retained frame.
pub(crate) fn submit_browser_pointer_input(
    target: &mut (impl BrowserPointerTarget + ?Sized),
    binding: &mut Option<BrowserPointerBinding>,
    next_sequence: &mut u64,
    wire: BrowserPointerInput,
) -> Result<(), String> {
    submit_pointer(target, binding, next_sequence, wire, None).map_err(|e| e.to_string())
}

#[cfg(any(all(feature = "renderer", target_arch = "wasm32"), test))]
pub(crate) fn submit_presented_browser_pointer_input(
    target: &mut (impl BrowserPointerTarget + ?Sized),
    binding: &mut Option<BrowserPointerBinding>,
    next_sequence: &mut u64,
    wire: BrowserPointerInput,
    frame: &PointerFrameSnapshot,
) -> Result<(), BrowserPointerAdmissionError> {
    submit_pointer(target, binding, next_sequence, wire, Some(frame))
}

/// Cancel the current gesture through the ordinary binding/sequence/program gates.
/// DOM cancellation and rejected input retire the source; a surface transition
/// keeps it available for a subsequent physical release, but never restores the
/// cancelled gesture. No positional receipt or synthetic release is involved.
#[cfg(any(all(feature = "renderer", target_arch = "wasm32"), test))]
pub(crate) fn cancel_browser_pointer_input(
    target: &mut (impl BrowserPointerTarget + ?Sized),
    binding: &mut Option<BrowserPointerBinding>,
    next_sequence: &mut u64,
    retire_source: bool,
) -> Result<(), String> {
    let Some(contact) = binding.as_mut().filter(|contact| !contact.retired) else {
        return Ok(());
    };
    let token = target.pointer_token()?;
    if token.pointer() != contact.pointer || token.context().view_revision != contact.view_revision
    {
        return Err("browser pointer session binding has been replaced".into());
    }
    let sequence = *next_sequence;
    let next = sequence
        .checked_add(1)
        .ok_or("native input event sequence exhausted")?;
    target.submit_pointer(
        &token,
        NativePointerInput::new(
            sequence,
            token.pointer(),
            token.context(),
            NativeInputModifiers::default(),
            NativePointerInputKind::Cancel(NativePointerCancellation::Cancelled),
        ),
    )?;
    *next_sequence = next;
    contact.retired = retire_source;
    Ok(())
}

impl BrowserPointerTarget for ExecutionSession {
    fn session(&self) -> &ExecutionSession {
        self
    }
    fn configure_pointer(
        &mut self,
        pointer: NativePointerId,
        view: u64,
    ) -> Result<NativePointerInputToken, String> {
        self.configure_native_pointer_input(pointer, view)
            .map_err(|e| e.to_string())
    }
    fn pointer_token(&self) -> Result<NativePointerInputToken, String> {
        self.native_pointer_input_token().map_err(|e| e.to_string())
    }
    fn submit_pointer(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<NativePointerInputPublication, String> {
        self.submit_native_pointer_input(token, input)
            .map_err(|e| e.to_string())
    }
}

impl<C: LiveContinuation> BrowserPointerTarget for LiveProgram<C>
where
    C::Error: std::fmt::Display,
{
    fn session(&self) -> &ExecutionSession {
        self.session()
    }
    fn configure_pointer(
        &mut self,
        pointer: NativePointerId,
        view: u64,
    ) -> Result<NativePointerInputToken, String> {
        self.configure_native_pointer_input(pointer, view)
            .map_err(|e| e.to_string())
    }
    fn pointer_token(&self) -> Result<NativePointerInputToken, String> {
        self.native_pointer_input_token().map_err(|e| e.to_string())
    }
    fn submit_pointer(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<NativePointerInputPublication, String> {
        self.submit_native_pointer_input(token, input)
            .map_err(|e| e.to_string())
    }
}
use noon_core::{
    Camera2DState, NativeInputModifiers, NativePointerCancellation, NativePointerId,
    NativePointerInput, NativePointerInputKind, NativePointerPosition, Vec2,
};
use serde::Deserialize;

const MAX_JS_INTEGER: u64 = (1_u64 << 53) - 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BrowserPointerBinding {
    pointer: NativePointerId,
    view_revision: u64,
    viewport: Vec2,
    retired: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BrowserPointerInput {
    pub(crate) kind: BrowserPointerKind,
    pub(crate) source_id: u64,
    pub(crate) pointer_id: i32,
    pub(crate) surface_x: Option<f32>,
    pub(crate) surface_y: Option<f32>,
    pub(crate) viewport_width: Option<f32>,
    pub(crate) viewport_height: Option<f32>,
    pub(crate) button: Option<u8>,
    pub(crate) view_revision: u64,
    #[serde(default)]
    pub(crate) shift: bool,
    #[serde(default)]
    pub(crate) control: bool,
    #[serde(default)]
    pub(crate) alt: bool,
    #[serde(default)]
    pub(crate) meta: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BrowserPointerKind {
    Move,
    Press,
    Release,
    Cancel,
    FocusLost,
    CaptureLost,
}

impl BrowserPointerInput {
    pub(crate) fn cancellation(self) -> Option<NativePointerCancellation> {
        match self.kind {
            BrowserPointerKind::Cancel => Some(NativePointerCancellation::Cancelled),
            BrowserPointerKind::FocusLost => Some(NativePointerCancellation::FocusLost),
            BrowserPointerKind::CaptureLost => Some(NativePointerCancellation::CaptureLost),
            _ => None,
        }
    }

    pub(crate) fn pointer(self) -> Result<NativePointerId, String> {
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

    /// Pure shape/source validation, including when no frame has been presented.
    /// Missing display state must not relax a retired source or manufacture an edge.
    pub(crate) fn needs_binding(
        self,
        binding: Option<BrowserPointerBinding>,
    ) -> Result<bool, String> {
        let pointer = self.pointer()?;
        let coordinates = self.coordinates()?;
        let needs_binding = match binding {
            None => true,
            Some(binding) => {
                if pointer.source < binding.pointer.source {
                    return Err("browser pointer source has been retired".into());
                }
                if pointer.source == binding.pointer.source {
                    if pointer != binding.pointer
                        || self.view_revision != binding.view_revision
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
                self.kind,
                BrowserPointerKind::Move | BrowserPointerKind::Press
            )
        {
            return Err("browser pointer release/cancellation requires its existing source".into());
        }
        Ok(needs_binding)
    }

    pub(crate) fn coordinates(self) -> Result<Option<(Vec2, Vec2)>, String> {
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
            BrowserPointerKind::Press | BrowserPointerKind::Release if self.button.is_none() => {
                return Err("browser pointer edge is missing its button".into())
            }
            BrowserPointerKind::Move if self.button.is_some() => {
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

fn submit_pointer(
    target: &mut (impl BrowserPointerTarget + ?Sized),
    binding: &mut Option<BrowserPointerBinding>,
    next_sequence: &mut u64,
    wire: BrowserPointerInput,
    presented: Option<&PointerFrameSnapshot>,
) -> Result<(), BrowserPointerAdmissionError> {
    // Validate the entire wire record before even configuring a source. A bad
    // press, overflowed coordinate or exhausted sequence must not clear buttons.
    let pointer = wire.pointer()?;
    let coordinates = wire.coordinates()?;
    let sequence = *next_sequence;
    let next = sequence
        .checked_add(1)
        .ok_or("native input event sequence exhausted")?;
    let needs_binding = wire.needs_binding(*binding)?;
    if let Some((surface, viewport)) = coordinates {
        // Direct hosts validate the displayed publication before configuration;
        // resetting held signals may itself change execution. Worker association
        // still belongs to #846's collection-time receipt transport, not this
        // current-execution fallback. It cannot be called by the direct entry.
        let camera = target
            .session()
            .camera()
            .map_err(|error| error.to_string())?;
        if let Some(frame) = presented {
            let view = PointerFrameView::new(wire.view_revision, viewport, camera)?;
            frame.validate_current(target.session(), view)?;
            frame.position(surface)?;
        } else {
            position(surface, viewport, camera)?;
        }
        if needs_binding {
            target
                .configure_pointer(pointer, wire.view_revision)
                .map_err(|error| error.to_string())?;
            *binding = Some(BrowserPointerBinding {
                pointer,
                view_revision: wire.view_revision,
                viewport,
                retired: false,
            });
        }
    }
    let token = target.pointer_token().map_err(|error| error.to_string())?;
    if token.pointer() != pointer || token.context().view_revision != wire.view_revision {
        return Err("browser pointer session binding has been replaced".into());
    }
    let kind = if let Some(reason) = wire.cancellation() {
        NativePointerInputKind::Cancel(reason)
    } else {
        let (surface, viewport) = coordinates.expect("validated positional record");
        let camera = target
            .session()
            .camera()
            .map_err(|error| error.to_string())?;
        let position = if let Some(frame) = presented {
            // Revalidate after configuration. Never retag an occurrence to the
            // publication or camera produced by clearing an older source.
            let view = PointerFrameView::new(wire.view_revision, viewport, camera)?;
            frame.input_token(target.session(), view)?;
            frame.position(surface)?
        } else {
            position(surface, viewport, camera)?
        };
        match wire.kind {
            BrowserPointerKind::Move => NativePointerInputKind::Move(position),
            BrowserPointerKind::Press => NativePointerInputKind::Press {
                position,
                button: wire.button.expect("validated edge"),
            },
            BrowserPointerKind::Release => NativePointerInputKind::Release {
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
    target
        .submit_pointer(&token, input)
        .map_err(|error| error.to_string())?;
    *next_sequence = next;
    if wire.cancellation().is_some() {
        binding.as_mut().expect("configured source").retired = true;
    }
    Ok(())
}

/// Validate DOM numeric identities before narrowing at the JavaScript ABI.
/// This function is shared by direct-WASM qualification and the scalar binding.
#[cfg(any(all(feature = "renderer", target_arch = "wasm32"), test))]
pub(crate) fn dom_integer(value: f64, min: f64, max: f64) -> Result<f64, String> {
    if !value.is_finite() || value.fract() != 0.0 || value < min || value > max {
        return Err("DOM pointer identity/button must be an exact in-range integer".into());
    }
    Ok(value)
}

#[cfg(any(all(feature = "renderer", target_arch = "wasm32"), test))]
impl BrowserPointerKind {
    pub(crate) fn from_name(name: &str) -> Result<Self, String> {
        match name {
            "move" => Ok(Self::Move),
            "press" => Ok(Self::Press),
            "release" => Ok(Self::Release),
            "cancel" => Ok(Self::Cancel),
            "focus_lost" => Ok(Self::FocusLost),
            "capture_lost" => Ok(Self::CaptureLost),
            _ => Err("unknown browser pointer occurrence kind".into()),
        }
    }
}
