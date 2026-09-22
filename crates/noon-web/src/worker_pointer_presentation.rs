//! Collection-time worker association. Retains one issued shared snapshot, not
//! geometry/history. Consumption is not presentation, and receipt IDs never
//! reconstruct a frame. A newer issued delta conservatively retires its predecessor.
use serde::{Deserialize, Serialize};

const MAX_JS_INTEGER: u64 = (1_u64 << 53) - 1;

/// Logical viewport used to render the publication, transported at the real
/// worker boundary. Zero dimensions register an unavailable view.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointerPresentationView {
    pub revision: u64,
    pub width: f32,
    pub height: f32,
}
impl PointerPresentationView {
    pub(crate) fn validate(self) -> Result<(), String> {
        if self.revision > MAX_JS_INTEGER
            || !self.width.is_finite()
            || !self.height.is_finite()
            || self.width < 0.0
            || self.height < 0.0
        {
            return Err("invalid worker pointer view".into());
        }
        Ok(())
    }
    pub(crate) fn drawable(self) -> bool {
        self.width > 0.0 && self.height > 0.0
    }
}

/// Existing transport session/sequence plus the render host's successful
/// presentation counter. A receipt is immutable on every collected occurrence.
#[cfg(any(target_arch = "wasm32", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkerPointerReceipt {
    pub session: u32,
    pub sequence: u64,
    pub presentation: u64,
    pub view_revision: u64,
}
#[cfg(any(target_arch = "wasm32", test))]
impl WorkerPointerReceipt {
    pub(crate) fn validate(self) -> Result<(), String> {
        if self.sequence > MAX_JS_INTEGER
            || self.presentation == 0
            || self.presentation > MAX_JS_INTEGER
            || self.view_revision > MAX_JS_INTEGER
        {
            return Err("invalid worker pointer presentation receipt".into());
        }
        Ok(())
    }
}

#[cfg(any(target_arch = "wasm32", test))]
mod admission {
    use super::*;
    use crate::browser_pointer_input::{
        self, BrowserPointerAdmissionError, BrowserPointerBinding, BrowserPointerInput,
    };
    use noon::integration::{PointerFrameSnapshot, PointerFrameView};
    use noon::ExecutionSession;
    use noon_core::Vec2;

    #[derive(Clone, Copy, Debug)]
    struct RejectedSource {
        source: u64,
        pointer: i32,
        view: u64,
        viewport: Option<Vec2>,
    }

    pub(crate) struct IssuedFrame {
        session: u32,
        sequence: u64,
        frame: PointerFrameSnapshot,
    }

    #[derive(Default)]
    pub(crate) struct WorkerPointerPresentation {
        view: Option<PointerPresentationView>,
        issued: Option<IssuedFrame>,
        presented: Option<WorkerPointerReceipt>,
        retired_presentation: u64,
        rejected: Option<RejectedSource>,
        refresh_pending: bool,
    }
    impl WorkerPointerPresentation {
        pub(crate) fn view(&self) -> Option<PointerPresentationView> {
            self.view
        }

        pub(crate) fn set_view(
            &mut self,
            session: &mut ExecutionSession,
            binding: &mut Option<BrowserPointerBinding>,
            sequence: &mut u64,
            view: PointerPresentationView,
        ) -> Result<(), String> {
            view.validate()?;
            if self.view.is_some_and(|old| view.revision < old.revision) {
                return Err("worker pointer view has been retired".into());
            }
            if self.view == Some(view) {
                return Ok(());
            }
            if self.view.is_some_and(|old| old.revision == view.revision) {
                return Err("worker pointer view revision reused with different geometry".into());
            }
            // Cancel before changing admission metadata; callback/sequence failure
            // must not claim that the old gesture was retired.
            browser_pointer_input::cancel_browser_pointer_input(session, binding, sequence, true)?;
            if let Some(old) = self.presented {
                self.retired_presentation = old.presentation;
            }
            self.view = Some(view);
            self.issued = None;
            self.presented = None;
            self.refresh_pending = true;
            Ok(())
        }

        pub(crate) fn capture(
            &self,
            session: &ExecutionSession,
        ) -> Result<Option<PointerFrameSnapshot>, String> {
            self.view
                .filter(|view| view.drawable())
                .map(|view| {
                    let camera = session.camera().map_err(|e| e.to_string())?;
                    let view = PointerFrameView::new(
                        view.revision,
                        Vec2::new(view.width, view.height),
                        camera,
                    )
                    .map_err(|e| e.to_string())?;
                    session
                        .capture_pointer_frame(view)
                        .map_err(|e| e.to_string())
                })
                .transpose()
        }

        pub(crate) fn needs_delta(&self, session: &ExecutionSession) -> bool {
            self.refresh_pending
                || self.issued.as_ref().is_some_and(|issued| {
                    issued
                        .frame
                        .validate_current(session, issued.frame.view())
                        .is_err_and(|e| browser_pointer_input::recoverable_frame_error(&e))
                })
        }

        pub(crate) fn issue(
            &mut self,
            session: u32,
            sequence: u64,
            frame: Option<PointerFrameSnapshot>,
        ) {
            self.issued = frame.map(|frame| IssuedFrame {
                session,
                sequence,
                frame,
            });
            // Keep the last actual presentation for ordered surface invalidation.
            // Admission still requires its IDs to match this newly issued frame.
            self.refresh_pending = false;
        }

        pub(crate) fn note_presented(
            &mut self,
            receipt: WorkerPointerReceipt,
        ) -> Result<bool, String> {
            receipt.validate()?;
            let Some(issued) = self.issued.as_ref() else {
                return Ok(false);
            };
            if receipt.session != issued.session
                || receipt.sequence != issued.sequence
                || receipt.view_revision != issued.frame.view().revision()
                || receipt.presentation <= self.retired_presentation
                || self
                    .presented
                    .is_some_and(|old| receipt.presentation <= old.presentation && receipt != old)
            {
                return Ok(false);
            }
            self.presented = Some(receipt);
            Ok(true)
        }

        pub(crate) fn invalidate(
            &mut self,
            session: &mut ExecutionSession,
            binding: &mut Option<BrowserPointerBinding>,
            sequence: &mut u64,
            receipt: WorkerPointerReceipt,
        ) -> Result<bool, String> {
            receipt.validate()?;
            if self.presented != Some(receipt) {
                return Ok(false);
            }
            // The physical source can deliver its eventual real release, but the
            // gesture cannot cross an unavailable surface.
            browser_pointer_input::cancel_browser_pointer_input(session, binding, sequence, false)?;
            self.retired_presentation = receipt.presentation;
            self.presented = None;
            self.refresh_pending = true;
            Ok(true)
        }

        pub(crate) fn submit(
            &mut self,
            session: &mut ExecutionSession,
            binding: &mut Option<BrowserPointerBinding>,
            sequence: &mut u64,
            input: BrowserPointerInput,
            receipt: Option<WorkerPointerReceipt>,
        ) -> Result<bool, String> {
            input.pointer()?;
            let coordinates = input.coordinates()?;
            if let Some(receipt) = receipt {
                receipt.validate()?;
            }
            // A rejected asynchronous source can already have a bounded packet
            // tail in flight. Drop only that exact source, without another cancel
            // or acknowledging those occurrences. It cannot revive after repaint.
            if let Some(rejected) = self.rejected {
                if input.source_id < rejected.source {
                    return Err("worker pointer source has been retired".into());
                }
                if input.source_id == rejected.source {
                    if input.pointer_id != rejected.pointer
                        || input.view_revision != rejected.view
                        || coordinates
                            .is_some_and(|(_, viewport)| Some(viewport) != rejected.viewport)
                    {
                        return Err("rejected worker pointer identity/view does not match".into());
                    }
                    return Ok(false);
                }
            }
            input.needs_binding(*binding)?;
            sequence
                .checked_add(1)
                .ok_or("native input event sequence exhausted")?;
            if input.cancellation().is_some() {
                browser_pointer_input::submit_browser_pointer_input(
                    session, binding, sequence, input,
                )?;
                return Ok(true);
            }
            if let (Some(receipt), Some(issued)) = (receipt, self.issued.as_ref()) {
                if self.presented == Some(receipt)
                    && receipt.session == issued.session
                    && receipt.sequence == issued.sequence
                {
                    match browser_pointer_input::submit_presented_browser_pointer_input(
                        session,
                        binding,
                        sequence,
                        input,
                        &issued.frame,
                    ) {
                        Ok(()) => {
                            self.rejected = None;
                            return Ok(true);
                        }
                        Err(BrowserPointerAdmissionError::Frame(error))
                            if browser_pointer_input::recoverable_frame_error(&error) => {}
                        Err(error) => return Err(error.to_string()),
                    }
                }
            }
            browser_pointer_input::cancel_browser_pointer_input(session, binding, sequence, true)?;
            self.rejected = Some(RejectedSource {
                source: input.source_id,
                pointer: input.pointer_id,
                view: input.view_revision,
                viewport: coordinates.map(|(_, viewport)| viewport),
            });
            // Only produce work when the issued frame itself is obsolete. An old
            // occurrence must not cause a feedback loop of identical fresh deltas.
            if self.issued.is_none() {
                self.refresh_pending = self.view.is_some();
            }
            Ok(false)
        }
    }
}
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) use admission::WorkerPointerPresentation;
