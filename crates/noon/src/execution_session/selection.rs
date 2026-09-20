//! Opt-in primary-button fill selection at the existing atomic input boundary.
//!
//! This is transient session policy, not an authored trigger/action binding. It
//! retains one gesture and one semantic identity, never a scene or input queue.

use noon_core::{
    Color, GeometryRef, NativePointerInput, NativePointerInputKind, PublicationContext,
    SemanticNodeId, Transform2D,
};

use super::{
    ExecutionSession, ExecutionSessionInputError, NativePointerInputToken, PointerFillOutcome,
    PointerFillQuery,
};

/// One successfully admitted click. `None` is a background click, not a failed
/// pick. Both endpoints retain their occurrence-local coordinates and context.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerSelectionClick {
    target: Option<SemanticNodeId>,
    press: NativePointerInput,
    release: NativePointerInput,
}

impl PointerSelectionClick {
    pub const fn target(self) -> Option<SemanticNodeId> {
        self.target
    }
    pub const fn press(self) -> NativePointerInput {
        self.press
    }
    pub const fn release(self) -> NativePointerInput {
        self.release
    }
}

/// Read-only highlight geometry for the selected analytic fill at the current
/// effective publication. It is not a semantic object or renderer publication.
/// An interactive renderer must explicitly draw this as an overlay; ordinary
/// scene rendering/export does not consume it. No raster output is implied.
#[derive(Clone, Debug, PartialEq)]
pub struct PointerSelectionHighlight {
    pub target: SemanticNodeId,
    pub publication: PublicationContext,
    pub geometry: GeometryRef,
    pub transform: Transform2D,
}

/// The desired selection image, independent of semantic identity and publication.
///
/// Equality describes the same world-space overlay content. Hosts still handle
/// camera/surface invalidation and retain this value at their own delivery or
/// presentation boundary. Target identity and frame epoch are not image-change
/// signals. This is not authored scene content.
#[derive(Clone, Debug, PartialEq)]
pub struct PointerSelectionPresentation {
    pub geometry: GeometryRef,
    pub transform: Transform2D,
    pub color: Color,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PendingClick {
    press: NativePointerInput,
    outcome: PointerFillOutcome,
    last_sequence: u64,
    eligible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SelectedTarget {
    node: SemanticNodeId,
    publication: PublicationContext,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct PointerSelectionState {
    max_movement: Option<f32>,
    pending: Option<PendingClick>,
    selected: Option<SelectedTarget>,
    buttons: [u64; 4],
}

impl PointerSelectionState {
    pub(super) const fn enabled(&self) -> bool {
        self.max_movement.is_some()
    }

    /// A cloned/replaced runtime keeps configuration, never a physical gesture
    /// or selection belonging to the old runtime incarnation.
    pub(super) fn fresh(&self) -> Self {
        Self {
            max_movement: self.max_movement,
            ..Self::default()
        }
    }

    pub(super) fn cancel_press(&mut self) {
        self.pending = None;
        self.buttons = [0; 4];
    }

    pub(super) fn reset(&mut self) {
        *self = self.fresh();
    }
}

pub(super) struct PreparedPointerSelection {
    pub state: PointerSelectionState,
    pub query: Option<PointerFillQuery>,
    pub click: Option<PointerSelectionClick>,
    pub changed: bool,
}

fn compatible_revisions(left: PublicationContext, right: PublicationContext) -> bool {
    left.scene_revision() == right.scene_revision()
        && left.execution_revision() == right.execution_revision()
}

impl ExecutionSession {
    /// Enable transient selection of all supported visible analytic fills with
    /// primary button 0. The maximum radial excursion is measured in logical
    /// surface pixels, inclusively, independently of camera scale.
    ///
    /// Every ordered motion sample must reach `submit_native_pointer_input`.
    /// Crossing the tolerance, a sequence gap, another button edge, duplicate
    /// press, cancellation or view/revision change disqualifies the gesture.
    /// A background press/release clears selection; incompatible endpoints and
    /// unsupported fills do not. No DOM `click` event is needed or consumed.
    ///
    /// This is an explicit session/editor policy, not a scene-authored binding.
    /// Reconfiguration clears transient state without changing the authored
    /// frame, time, membership, painter order or replay history.
    pub fn enable_pointer_fill_selection(
        &mut self,
        max_movement: f32,
    ) -> Result<(), ExecutionSessionInputError> {
        self.ensure_direct_input_ingress_available()?;
        if !max_movement.is_finite() || max_movement < 0.0 {
            return Err(ExecutionSessionInputError::InvalidPointerClickTolerance);
        }
        self.pointer_selection = PointerSelectionState {
            max_movement: Some(max_movement),
            ..PointerSelectionState::default()
        };
        Ok(())
    }

    pub fn disable_pointer_fill_selection(&mut self) -> Result<(), ExecutionSessionInputError> {
        self.ensure_direct_input_ingress_available()?;
        self.pointer_selection = PointerSelectionState::default();
        Ok(())
    }

    /// Current generation-safe selection. Any semantic/execution revision change
    /// invalidates selection conservatively, even when unrelated to the target.
    /// A later reattachment cannot revive it. Ordinary effective frame advance
    /// is compatible; successful seek/backward evaluation clears session state.
    pub fn selected_pointer_target(&self) -> Option<SemanticNodeId> {
        let selected = self.pointer_selection.selected?;
        (compatible_revisions(selected.publication, self.publication_context())
            && self.semantic_object_is_reachable(selected.node))
        .then_some(selected.node)
    }

    /// Project only the selected row, without querying the spatial index,
    /// consuming renderer dirtiness or modifying authored/runtime geometry.
    /// Invisible or no-longer-supported effective geometry has no highlight.
    pub fn pointer_selection_highlight(&self) -> Option<PointerSelectionHighlight> {
        let target = self.selected_pointer_target()?;
        let object = self.execution_object_id(target)?;
        let index = self.runtime.frame_index_for_object(object)?;
        let frame = self.frame();
        let state = &frame.objects[index];
        if !frame.is_present(index)
            || state.appearance <= 0.0
            || state.style.opacity <= 0.0
            || !state.style.fill.is_some_and(|color| color.alpha > 0.0)
        {
            return None;
        }
        let transform = frame.render_transform(index);
        // At the analytic center, the same support/degeneracy policy as picking
        // decides whether a complete circle/rectangle outline is meaningful.
        if super::picking::analytic_fill_contains(frame, index, transform.translation) != Ok(true) {
            return None;
        }
        Some(PointerSelectionHighlight {
            target,
            publication: self.publication_context(),
            geometry: frame.render_geometry(index)?.clone(),
            transform,
        })
    }

    /// Observe the current desired interactive image using the shared tint policy.
    /// This reads only the selected effective row; it consumes no dirtiness and
    /// does not acknowledge input or renderer delivery. Native and direct WASM
    /// use this typed value; a worker serializes it only at its transport boundary.
    pub fn pointer_selection_presentation(&self) -> Option<PointerSelectionPresentation> {
        self.pointer_selection_highlight()
            .map(|highlight| PointerSelectionPresentation {
                geometry: highlight.geometry,
                transform: highlight.transform,
                color: Color::rgba(1.0, 1.0, 0.0, 0.35),
            })
    }

    /// Prepare on a small copied gesture record. Input evaluation remains the
    /// only fallible publication step; callers install this state only after it
    /// succeeds. Queries may synchronize the existing derived spatial index.
    pub(super) fn prepare_pointer_selection(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<PreparedPointerSelection, ExecutionSessionInputError> {
        let mut prepared = PreparedPointerSelection {
            state: self.pointer_selection,
            query: None,
            click: None,
            changed: false,
        };
        let Some(max_movement) = prepared.state.max_movement else {
            return Ok(prepared);
        };
        let previous_selection = self.selected_pointer_target();
        if previous_selection.is_none() {
            prepared.state.selected = None;
        }
        if let Some(pending) = prepared.state.pending.as_mut() {
            let context = pending.press.context();
            pending.eligible &= pending.press.pointer() == input.pointer()
                && context.view_revision == input.context().view_revision
                && compatible_revisions(context.publication, input.context().publication)
                && pending.last_sequence.checked_add(1) == Some(input.sequence());
            if let Some(position) = input.position() {
                let origin = pending
                    .press
                    .position()
                    .expect("press has coordinates")
                    .surface();
                let point = position.surface();
                // Widen BEFORE subtracting: finite f32 endpoints can overflow a
                // narrow subtraction or squared distance. The flag is sticky.
                let distance = (f64::from(point.x) - f64::from(origin.x))
                    .hypot(f64::from(point.y) - f64::from(origin.y));
                pending.eligible &= distance <= f64::from(max_movement);
            }
            pending.last_sequence = input.sequence();
        }
        match input.kind() {
            NativePointerInputKind::Press { button, .. } => {
                prepared.state.buttons[usize::from(button / 64)] |= 1_u64 << (button % 64);
            }
            NativePointerInputKind::Release { button, .. } => {
                prepared.state.buttons[usize::from(button / 64)] &= !(1_u64 << (button % 64));
            }
            _ => {}
        }
        match input.kind() {
            NativePointerInputKind::Press { button: 0, .. } => {
                if let Some(pending) = prepared.state.pending.as_mut() {
                    // A repeated down is not a new physical gesture.
                    pending.eligible = false;
                } else {
                    let query = self.pick_native_pointer_fill(token, input, |_| true)?;
                    let outcome = query.outcome();
                    prepared.state.pending = Some(PendingClick {
                        press: input,
                        outcome,
                        last_sequence: input.sequence(),
                        eligible: prepared.state.buttons == [1, 0, 0, 0]
                            && matches!(
                                outcome,
                                PointerFillOutcome::Hit(_) | PointerFillOutcome::Miss
                            ),
                    });
                    prepared.query = Some(query);
                }
            }
            NativePointerInputKind::Release { button: 0, .. } => {
                if let Some(pending) = prepared.state.pending.take().filter(|p| p.eligible) {
                    let query = self.pick_native_pointer_fill(token, input, |_| true)?;
                    if query.outcome() == pending.outcome {
                        let target = match query.outcome() {
                            PointerFillOutcome::Hit(target) => Some(target),
                            PointerFillOutcome::Miss => None,
                            _ => unreachable!("eligible press is a decided hit or miss"),
                        };
                        prepared.state.selected = target.map(|node| SelectedTarget {
                            node,
                            publication: query.publication(),
                        });
                        prepared.click = Some(PointerSelectionClick {
                            target,
                            press: pending.press,
                            release: input,
                        });
                        prepared.changed = target != previous_selection;
                    }
                    prepared.query = Some(query);
                }
            }
            NativePointerInputKind::Press { .. } | NativePointerInputKind::Release { .. } => {
                if let Some(pending) = prepared.state.pending.as_mut() {
                    pending.eligible = false;
                }
            }
            NativePointerInputKind::Cancel(_) => prepared.state.cancel_press(),
            NativePointerInputKind::Move(_) => {}
        }
        Ok(prepared)
    }
}

#[cfg(test)]
mod tests;
