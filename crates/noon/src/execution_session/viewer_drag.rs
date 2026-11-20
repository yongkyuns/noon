//! Presentation-only dragging on a paused retained frame.
use noon_core::{NativePointerId, NativePointerInput, NativePointerInputKind, ObjectId, Vec2};

use super::{
    ExecutionSession, ExecutionSessionInputError, NativePointerInputToken, PointerFillOutcome,
};

#[derive(Clone, Copy, Debug, PartialEq)]
struct ViewerDrag {
    pointer: NativePointerId,
    object: ObjectId,
    press: Vec2,
    before: Vec2,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct ViewerDragState {
    active: Option<ViewerDrag>,
    undo: Option<(ObjectId, Vec2)>,
}

impl ViewerDragState {
    pub(super) fn is_active(&self) -> bool {
        self.active.is_some()
    }
}

pub(super) struct PreparedViewerDrag {
    active: Option<ViewerDrag>,
    undo: Option<(ObjectId, Vec2)>,
    position: Option<(ObjectId, Vec2)>,
}

impl ExecutionSession {
    pub(super) fn prepare_viewer_drag(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
        allow_drag: bool,
    ) -> Result<Option<PreparedViewerDrag>, ExecutionSessionInputError> {
        let viewer = self.translation_drag.viewer;
        if let Some(drag) = viewer.active.filter(|drag| drag.pointer == input.pointer()) {
            let mut next = PreparedViewerDrag {
                active: viewer.active,
                undo: viewer.undo,
                position: None,
            };
            match input.kind() {
                NativePointerInputKind::Move(point) => {
                    next.position = Some((drag.object, drag.before + (point.scene() - drag.press)));
                }
                NativePointerInputKind::Release { button: 0, .. } => {
                    let position = input.position().map_or_else(
                        || {
                            self.runtime
                                .effective_transform(drag.object)
                                .map(|t| t.translation)
                        },
                        |point| Some(drag.before + (point.scene() - drag.press)),
                    );
                    next.position = position.map(|position| (drag.object, position));
                    next.active = None;
                    if position.is_some_and(|position| position != drag.before) {
                        next.undo = Some((drag.object, drag.before));
                    }
                }
                NativePointerInputKind::Cancel(_) => {
                    next.position = Some((drag.object, drag.before));
                    next.active = None;
                }
                _ => {}
            }
            return Ok(Some(next));
        }

        if !allow_drag
            || viewer.active.is_some()
            || !self.runtime.has_viewer_drag_targets()
            || !matches!(
                input.kind(),
                NativePointerInputKind::Press { button: 0, .. }
            )
        {
            return Ok(None);
        }
        // Pick in painter order. Resolve the hit only against the currently
        // displayed revision, so future declarations cannot enable this press.
        let query = self.pick_native_pointer_fill(token, input, |_| true)?;
        let PointerFillOutcome::Hit(node) = query.outcome() else {
            return Ok(None);
        };
        let Some(object) = self.interaction_execution_object_id(node) else {
            return Ok(None);
        };
        if !self.runtime.can_viewer_drag(object) {
            return Ok(None);
        }
        let before = self
            .runtime
            .effective_transform(object)
            .expect("preflighted viewer drag target")
            .translation;
        Ok(Some(PreparedViewerDrag {
            active: Some(ViewerDrag {
                pointer: input.pointer(),
                object,
                press: input.position().expect("fill hit has a position").scene(),
                before,
            }),
            undo: viewer.undo,
            position: None,
        }))
    }

    pub(super) fn commit_viewer_drag(
        &mut self,
        drag: PreparedViewerDrag,
    ) -> Result<(), ExecutionSessionInputError> {
        if let Some((object, position)) = drag.position {
            self.publish_viewer_drag_position(object, position)?;
        }
        self.translation_drag.viewer = ViewerDragState {
            active: drag.active,
            undo: drag.undo,
        };
        Ok(())
    }

    fn publish_viewer_drag_position(
        &mut self,
        object: ObjectId,
        position: Vec2,
    ) -> Result<(), ExecutionSessionInputError> {
        self.runtime
            .set_viewer_translation(object, position)
            .map_err(ExecutionSessionInputError::Interaction)
    }

    #[cfg(feature = "prepared-prefix")]
    pub(super) fn can_undo_viewer_drag(&self) -> bool {
        let viewer = self.translation_drag.viewer;
        viewer.active.is_none() && viewer.undo.is_some()
    }

    #[cfg(feature = "prepared-prefix")]
    pub(super) fn undo_viewer_drag(&mut self) -> Result<bool, ExecutionSessionInputError> {
        if !self.can_undo_viewer_drag() {
            return Ok(false);
        }
        let (object, before) = self
            .translation_drag
            .viewer
            .undo
            .expect("undo is available");
        self.publish_viewer_drag_position(object, before)?;
        self.translation_drag.viewer.undo = None;
        Ok(true)
    }
}
