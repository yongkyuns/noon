//! Opt-in native primary-button selection under the existing input publication.
//!
//! This is session/tool state, not an authored trigger/action binding or another
//! input consumer queue. It retains one candidate gesture and one semantic target.

use noon_core::{
    GeometryRef, NativePointerInput, NativePointerInputKind, PublicationContext, Rect,
    SemanticNodeId,
};

use super::{
    ExecutionSession, ExecutionSessionInputError, NativePointerInputToken, PointerFillOutcome,
    PointerFillQuery,
};

/// One successfully admitted primary-button click. A background click has no target.
/// Both original occurrences are retained; a later sample cannot move the press.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativePointerClick {
    target: Option<SemanticNodeId>,
    press: NativePointerInput,
    release: NativePointerInput,
}
impl NativePointerClick {
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

/// A transient selection observation for an explicit interactive presentation host.
/// It is not a scene object or renderer row and is absent from ordinary exports.
/// Bounds are nominal analytic-fill bounds, not a stroke/antialias coverage promise.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativePointerSelection {
    target: SemanticNodeId,
    publication: PublicationContext,
    bounds: Option<Rect>,
}
impl NativePointerSelection {
    pub const fn target(self) -> SemanticNodeId {
        self.target
    }
    pub const fn publication(self) -> PublicationContext {
        self.publication
    }
    pub const fn bounds(self) -> Option<Rect> {
        self.bounds
    }
}

#[derive(Clone, Copy, Debug)]
struct Press {
    input: NativePointerInput,
    target: Option<SemanticNodeId>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct PointerSelectionState {
    tolerance: Option<f32>,
    buttons: [u64; 4],
    press: Option<Press>,
    selected: Option<SemanticNodeId>,
    scope: Option<PublicationContext>,
}
impl PointerSelectionState {
    pub(super) fn enabled(self) -> bool {
        self.tolerance.is_some()
    }
    pub(super) fn without_transients(self) -> Self {
        Self {
            tolerance: self.tolerance,
            ..Self::default()
        }
    }
    pub(super) fn cancel_gesture(&mut self) {
        self.buttons = [0; 4];
        self.press = None;
    }
    // Rejected motion can contain path evidence. Never turn a missing sample into
    // a click; preserve accepted button state, but disarm until a new clean press.
    pub(super) fn reject_occurrence(&mut self) {
        self.press = None;
    }
    fn same_scope(self, current: PublicationContext) -> bool {
        self.scope.is_some_and(|previous| {
            previous.scene_revision() == current.scene_revision()
                && previous.execution_revision() == current.execution_revision()
        })
    }
    fn has_button(self, button: u8) -> bool {
        self.buttons[usize::from(button / 64)] & (1_u64 << (button % 64)) != 0
    }
    fn set_button(&mut self, button: u8, down: bool) {
        let word = &mut self.buttons[usize::from(button / 64)];
        let mask = 1_u64 << (button % 64);
        if down {
            *word |= mask;
        } else {
            *word &= !mask;
        }
    }
    fn observe_motion(&mut self, input: NativePointerInput) {
        let Some(press) = self.press else {
            return;
        };
        let Some(position) = input.position() else {
            return;
        };
        let origin = press
            .input
            .position()
            .expect("only positional presses can arm selection")
            .surface();
        let point = position.surface();
        let dx = f64::from(point.x) - f64::from(origin.x);
        let dy = f64::from(point.y) - f64::from(origin.y);
        let tolerance = f64::from(self.tolerance.expect("enabled selection has a tolerance"));
        if dx.hypot(dy) > tolerance {
            self.press = None;
        }
    }
}

pub(super) struct PreparedPointerSelection {
    state: PointerSelectionState,
    pub(super) query: Option<PointerFillQuery>,
    pub(super) click: Option<NativePointerClick>,
}

impl ExecutionSession {
    /// Enable a transient primary-button selection tool for all eligible fills.
    ///
    /// The existing analytic fill policy is authoritative; undecidable coverage
    /// blocks selection instead of selecting through it. This is not an authored
    /// interaction binding. Disabled by default. Tolerance is a finite nonnegative
    /// distance in the same logical-surface pixels carried by the input adapter.
    /// Reconfiguration clears selection/gesture state without synthesizing input.
    pub fn configure_native_pointer_selection(
        &mut self,
        tolerance: f32,
    ) -> Result<(), ExecutionSessionInputError> {
        self.ensure_direct_input_ingress_available()?;
        if !tolerance.is_finite() || tolerance < 0.0 {
            return Err(ExecutionSessionInputError::InvalidSelectionTolerance);
        }
        self.clear_native_pointer_selection();
        self.pointer_selection.tolerance = Some(tolerance);
        Ok(())
    }

    /// Disable the tool and retire its transient state. Native signal routes remain.
    pub fn disable_native_pointer_selection(&mut self) -> Result<(), ExecutionSessionInputError> {
        self.ensure_direct_input_ingress_available()?;
        self.clear_native_pointer_selection();
        self.pointer_selection.tolerance = None;
        Ok(())
    }

    /// Read the current selected semantic leaf and its effective analytic bounds.
    /// This indexed observation neither refits the spatial index nor consumes
    /// renderer dirtiness. Structural/execution-revision changes conservatively
    /// retire tool state; frame-only animation continues to move the selected leaf.
    /// A host must combine this observation with the same effective publication.
    /// No historical displayed-frame association or rendered overlay is implied.
    pub fn native_pointer_selection(&self) -> Option<NativePointerSelection> {
        let current = self.publication_context();
        if !self.pointer_selection.same_scope(current) {
            return None;
        }
        let target = self.pointer_selection.selected?;
        let object = self.execution_index.execution_object_id(target)?;
        let index = self.runtime.frame_index_for_object(object)?;
        let frame = self.runtime.frame();
        if !frame.is_present(index) {
            return None;
        }
        let state = &frame.objects[index];
        let bounds = if frame.reveal(index) == 1.0
            && frame.morph(index) == 0.0
            && state.appearance > 0.0
            && state.style.opacity > 0.0
            && state.style.fill.is_some_and(|fill| fill.alpha > 0.0)
        {
            match frame.render_geometry(index) {
                Some(shape @ (GeometryRef::Circle { .. } | GeometryRef::Rectangle { .. })) => shape
                    .world_bounds(frame.render_transform(index))
                    .filter(|bounds| {
                        bounds.min.x.is_finite()
                            && bounds.min.y.is_finite()
                            && bounds.max.x.is_finite()
                            && bounds.max.y.is_finite()
                    }),
                _ => None,
            }
        } else {
            None
        };
        Some(NativePointerSelection {
            target,
            publication: current,
            bounds,
        })
    }

    pub(super) fn clear_native_pointer_selection(&mut self) {
        if self.pointer_selection.selected.is_some() {
            self.runtime.request_presentation_redraw();
        }
        self.pointer_selection = self.pointer_selection.without_transients();
    }

    /// Stage a small value copy before the existing atomic native input commit.
    /// Picking is done at the occurrence's pre-input publication; native reactive
    /// subscribers may subsequently move geometry, but cannot retarget this click.
    pub(super) fn prepare_native_pointer_selection(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<PreparedPointerSelection, ExecutionSessionInputError> {
        let mut state = self.pointer_selection;
        let mut query = None;
        let mut click = None;
        if state.enabled() {
            let current = self.publication_context();
            if !state.same_scope(current) {
                state = state.without_transients();
            }
            state.scope = Some(current);
            state.observe_motion(input);
            match input.kind() {
                NativePointerInputKind::Press { button, .. } => {
                    let clean_press = button == 0 && state.buttons == [0; 4];
                    state.press = None;
                    state.set_button(button, true);
                    if clean_press {
                        let picked = self.pick_native_pointer_fill(token, input, |_| true)?;
                        let target = match picked.outcome() {
                            PointerFillOutcome::Hit(target) => Some(Some(target)),
                            PointerFillOutcome::Miss => Some(None),
                            _ => None,
                        };
                        state.press = target.map(|target| Press { input, target });
                        query = Some(picked);
                    }
                }
                NativePointerInputKind::Release { button, .. } => {
                    let was_down = state.has_button(button);
                    state.set_button(button, false);
                    if button == 0 {
                        if let Some(press) = state.press.take().filter(|_| was_down) {
                            let picked = self.pick_native_pointer_fill(token, input, |_| true)?;
                            let same = match picked.outcome() {
                                PointerFillOutcome::Hit(target) => press.target == Some(target),
                                PointerFillOutcome::Miss => press.target.is_none(),
                                _ => false,
                            };
                            if same
                                && press.input.pointer() == input.pointer()
                                && press.input.context().view_revision
                                    == input.context().view_revision
                            {
                                state.selected = press.target;
                                click = Some(NativePointerClick {
                                    target: press.target,
                                    press: press.input,
                                    release: input,
                                });
                            }
                            query = Some(picked);
                        }
                    }
                }
                NativePointerInputKind::Move(_) => {}
                NativePointerInputKind::Cancel(_) => state.cancel_gesture(),
            }
        }
        Ok(PreparedPointerSelection {
            state,
            query,
            click,
        })
    }

    pub(super) fn commit_native_pointer_selection(
        &mut self,
        prepared: PreparedPointerSelection,
    ) -> bool {
        let changed = self.pointer_selection.selected != prepared.state.selected;
        self.pointer_selection = prepared.state;
        if changed {
            self.runtime.request_presentation_redraw();
        }
        changed
    }
}

#[cfg(test)]
mod tests;
