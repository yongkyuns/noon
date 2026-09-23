//! Occurrence-local pointer data for the common Rust input seam (#69).
//!
//! This is input data, not a second dispatcher or interaction runtime. The
//! execution session owns admission, ordering, publication validation and C5
//! gesture policy. Platform adapters construct these values without selecting
//! scene objects. No platform, renderer or language-host dependency is required.

use crate::{
    NativeEventOccurrence, NativeEventSource, NativeInputRuntimeError, NativeInputValue,
    NativeStateSource, NativeStateUpdate, PublicationContext, Vec2,
};

/// A pointer identity within an input source's lifetime.
///
/// A collector supplies `source`; it must change when that source is replaced.
/// `pointer` is local to that source, so two devices may use the same pointer ID.
/// These are input identities, never semantic object IDs or frame revisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NativePointerId {
    pub source: u64,
    pub pointer: u64,
}

/// Keyboard modifiers observed at the occurrence, not a later sampled value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeInputModifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub meta: bool,
}

/// The publication and view used by the collector to locate a pointer.
///
/// `view_revision` is scoped to the input session and changes on coordinate-map
/// changes, including resize/device scale. It is not a `FrameEpoch`. The session
/// must also validate its own runtime incarnation at admission; equality of these
/// fields alone does not prove that two different sessions are interchangeable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativePointerContext {
    pub publication: PublicationContext,
    pub view_revision: u64,
}

/// Finite scene and surface coordinates captured together for one occurrence.
///
/// `scene` uses scene/world coordinates. `surface` uses logical pixels relative
/// to the content viewport, with origin at the top left and positive y downward
/// (CSS pixels in a browser; physical pixels divided by scale factor natively).
/// Gesture tolerances use `surface`, not camera-dependent scene units. Positions
/// outside the viewport are valid: a captured pointer can leave the surface.
///
/// The adapter must derive both positions from the view identified by the input
/// context. This type validates numerical data, not that geometric association.
/// Fields are private and there is no unchecked deserialization constructor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativePointerPosition {
    scene: Vec2,
    surface: Vec2,
}

impl NativePointerPosition {
    pub fn new(scene: Vec2, surface: Vec2) -> Result<Self, NativeInputRuntimeError> {
        if !NativeInputValue::Vec2(scene).is_finite()
            || !NativeInputValue::Vec2(surface).is_finite()
        {
            return Err(NativeInputRuntimeError::NonFiniteValue);
        }
        Ok(Self { scene, surface })
    }

    pub const fn scene(self) -> Vec2 {
        self.scene
    }

    pub const fn surface(self) -> Vec2 {
        self.surface
    }
}

/// Finite logical-pixel wheel displacement captured with its occurrence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeWheelDelta(Vec2);
impl NativeWheelDelta {
    pub fn new(value: Vec2) -> Result<Self, NativeInputRuntimeError> {
        if !NativeInputValue::Vec2(value).is_finite() {
            return Err(NativeInputRuntimeError::NonFiniteValue);
        }
        Ok(Self(value))
    }
    pub const fn value(self) -> Vec2 {
        self.0
    }
}

/// Cancellation is not a successful release and must not synthesize a click.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativePointerCancellation {
    Cancelled,
    CaptureLost,
    CaptureFailed,
    FocusLost,
}

/// Pointer occurrences before C5 target selection and gesture interpretation.
///
/// Buttons use the existing native-input button code: 0 primary, 1 auxiliary,
/// 2 secondary; other codes are preserved for the embedding host. These codes
/// are not a DOM `buttons` bit mask. Cancellation needs no invented position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NativePointerInputKind {
    Wheel {
        position: NativePointerPosition,
        delta: NativeWheelDelta,
    },
    Move(NativePointerPosition),
    Press {
        position: NativePointerPosition,
        button: u8,
    },
    Release {
        position: NativePointerPosition,
        button: u8,
    },
    Cancel(NativePointerCancellation),
}

/// An immutable pointer occurrence with its own position and input context.
///
/// `sequence` is the existing native input ingress sequence, independent of
/// authored time. The session, not this data constructor, enforces increasing
/// order and bounded admission. Move records must not be coalesced across a
/// gesture unless the consumer preserves its required motion evidence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativePointerInput {
    sequence: u64,
    pointer: NativePointerId,
    context: NativePointerContext,
    modifiers: NativeInputModifiers,
    kind: NativePointerInputKind,
}

impl NativePointerInput {
    pub const fn new(
        sequence: u64,
        pointer: NativePointerId,
        context: NativePointerContext,
        modifiers: NativeInputModifiers,
        kind: NativePointerInputKind,
    ) -> Self {
        Self {
            sequence,
            pointer,
            context,
            modifiers,
            kind,
        }
    }

    pub const fn sequence(self) -> u64 {
        self.sequence
    }

    pub const fn pointer(self) -> NativePointerId {
        self.pointer
    }

    pub const fn context(self) -> NativePointerContext {
        self.context
    }

    pub const fn modifiers(self) -> NativeInputModifiers {
        self.modifiers
    }

    pub const fn kind(self) -> NativePointerInputKind {
        self.kind
    }

    pub const fn position(self) -> Option<NativePointerPosition> {
        match self.kind {
            NativePointerInputKind::Move(position)
            | NativePointerInputKind::Press { position, .. }
            | NativePointerInputKind::Release { position, .. }
            | NativePointerInputKind::Wheel { position, .. } => Some(position),
            NativePointerInputKind::Cancel(_) => None,
        }
    }

    /// Project one explicitly chosen pointer onto existing single-pointer signals.
    ///
    /// A session must choose the source/pointer driving the existing unkeyed
    /// `PointerPosition`/`PointerButton` signals; never merge all pointers into
    /// those signals implicitly. These values and `button_event()` are parts of
    /// one admitted occurrence, not instructions to publish them separately.
    /// Cancellation remains an explicit occurrence for session policy to handle.
    pub fn state_updates(self) -> impl Iterator<Item = NativeStateUpdate> {
        let position = self.position().map(|position| NativeStateUpdate {
            source: NativeStateSource::PointerPosition,
            value: NativeInputValue::Vec2(position.scene()),
        });
        let button = match self.kind {
            NativePointerInputKind::Press { button, .. } => Some((button, true)),
            NativePointerInputKind::Release { button, .. } => Some((button, false)),
            NativePointerInputKind::Move(_)
            | NativePointerInputKind::Cancel(_)
            | NativePointerInputKind::Wheel { .. } => None,
        }
        .map(|(button, pressed)| NativeStateUpdate {
            source: NativeStateSource::PointerButton { button },
            value: NativeInputValue::Bool(pressed),
        });
        let wheel = match self.kind {
            NativePointerInputKind::Wheel { delta, .. } => Some(NativeStateUpdate {
                source: NativeStateSource::WheelDelta,
                value: NativeInputValue::Vec2(delta.value()),
            }),
            _ => None,
        };
        [position, button, wheel].into_iter().flatten()
    }

    /// Preserve existing discrete button-event subscriptions and ingress sequence.
    ///
    /// Move and cancellation are not button events. Consumers must still handle
    /// the original `kind()`; this projection is not the interaction dispatcher.
    pub fn button_event(self) -> Option<NativeEventOccurrence> {
        let source = match self.kind {
            NativePointerInputKind::Wheel { .. } => NativeEventSource::Wheel,
            NativePointerInputKind::Press { button, .. } => {
                NativeEventSource::PointerDown { button }
            }
            NativePointerInputKind::Release { button, .. } => {
                NativeEventSource::PointerUp { button }
            }
            NativePointerInputKind::Move(_) | NativePointerInputKind::Cancel(_) => return None,
        };
        Some(NativeEventOccurrence::new(self.sequence, source))
    }
}

#[cfg(test)]
mod tests;
