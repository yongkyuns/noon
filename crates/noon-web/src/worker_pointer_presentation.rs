//! Collection-time worker association. Retains constant-size shared snapshots,
//! never geometry/history. Consumption is not presentation, and receipt IDs never
//! reconstruct a frame. New input requires the latest coherent presentation;
//! an acquired drag retains its one mapping until release or invalidation.
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

/// One normalized view-navigation occurrence at the genuine worker boundary.
/// The receipt is captured when the DOM event arrives, never at delivery time.
#[cfg(any(target_arch = "wasm32", test))]
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkerInspectionScroll {
    pub view_revision: u64,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub surface_x: f32,
    pub surface_y: f32,
    pub delta_pixels: f64,
    pub presentation: Option<WorkerPointerReceipt>,
}

#[cfg(any(target_arch = "wasm32", test))]
mod admission {
    use super::*;
    use crate::browser_pointer_input::{
        self, BrowserPointerAdmissionError, BrowserPointerBinding, BrowserPointerInput,
        BrowserPointerKind, BrowserPointerTarget,
    };
    use noon::integration::PointerFrameSnapshot;
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

    /// The last presented mapping, pinned when a press acquires a drag. Motion
    /// and acknowledgements use independent worker channels: a later receipt
    /// can reach the engine before an occurrence collected against an older one.
    /// Keeping this one mapping avoids retaining presentation history.
    struct PresentedFrame {
        receipt: WorkerPointerReceipt,
        frame: PointerFrameSnapshot,
    }

    impl PresentedFrame {
        fn contains_drag_receipt(
            &self,
            receipt: WorkerPointerReceipt,
            latest: WorkerPointerReceipt,
        ) -> bool {
            receipt.session == self.receipt.session
                && receipt.view_revision == self.receipt.view_revision
                && latest.session == self.receipt.session
                && latest.view_revision == self.receipt.view_revision
                && receipt.presentation >= self.receipt.presentation
                && receipt.presentation <= latest.presentation
                && receipt.sequence >= self.receipt.sequence
                && receipt.sequence <= latest.sequence
                && (receipt.presentation != self.receipt.presentation || receipt == self.receipt)
                && (receipt.presentation != latest.presentation || receipt == latest)
        }
    }

    #[derive(Default)]
    pub(crate) struct WorkerPointerPresentation {
        view: Option<PointerPresentationView>,
        issued: Option<IssuedFrame>,
        presented: Option<WorkerPointerReceipt>,
        presented_frame: Option<PresentedFrame>,
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
            target: &mut (impl BrowserPointerTarget + ?Sized),
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
            browser_pointer_input::cancel_browser_pointer_input(target, binding, sequence, true)?;
            if let Some(old) = self.presented {
                self.retired_presentation = old.presentation;
            }
            self.view = Some(view);
            self.issued = None;
            self.presented = None;
            self.presented_frame = None;
            self.refresh_pending = true;
            Ok(())
        }

        pub(crate) fn capture(
            &self,
            session: &noon::ExecutionSession,
        ) -> Result<Option<PointerFrameSnapshot>, String> {
            self.view
                .filter(|view| view.drawable())
                .map(|view| {
                    let view = session
                        .inspection_pointer_view(view.revision, Vec2::new(view.width, view.height))
                        .map_err(|e| e.to_string())?;
                    session
                        .capture_pointer_frame(view)
                        .map_err(|e| e.to_string())
                })
                .transpose()
        }

        pub(crate) fn needs_delta(&self, session: &ExecutionSession) -> bool {
            // A registered DOM view is not semantic input interest. Ordinary
            // scenes must retain their quiet-wait presentation without emitting
            // receipt-only deltas on every external authored-time sample.
            // Enabling selection or adding a native route resumes freshness
            // through this same session-owned query. Admission still validates
            // every collected receipt; no stale frame is relabelled as current.
            self.refresh_pending
                || (session.has_native_pointer_subscribers()
                    && self.issued.as_ref().is_some_and(|issued| {
                        issued
                            .frame
                            .validate_current(session, issued.frame.view())
                            .is_err_and(|e| browser_pointer_input::recoverable_frame_error(&e))
                    }))
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
            session: &mut noon::ExecutionSession,
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
            // A delayed renderer receipt may still describe the held contact's
            // visible mapping. Refresh hover only if that frame is current;
            // observing an older frame must not retarget the current session.
            if issued
                .frame
                .validate_presentation(session, issued.frame.view())
                .is_ok()
            {
                session
                    .refresh_pointer_hover(&issued.frame, issued.frame.view())
                    .map_err(|error| error.to_string())?;
            }
            self.presented = Some(receipt);
            if !session.translation_drag_active() {
                self.presented_frame = Some(PresentedFrame {
                    receipt,
                    frame: issued.frame.clone(),
                });
            }
            Ok(true)
        }

        pub(crate) fn invalidate(
            &mut self,
            target: &mut (impl BrowserPointerTarget + ?Sized),
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
            browser_pointer_input::cancel_browser_pointer_input(target, binding, sequence, false)?;
            self.retired_presentation = receipt.presentation;
            self.presented = None;
            self.presented_frame = None;
            self.refresh_pending = true;
            Ok(true)
        }

        /// None rejects an obsolete/unpresented occurrence; Some(false) is an
        /// admitted exact no-op; Some(true) changes only the shared session view
        /// and any held native buttons. No host camera, clock or input queue.
        pub(crate) fn scroll(
            &mut self,
            target: &mut (impl BrowserPointerTarget + ?Sized),
            binding: &mut Option<BrowserPointerBinding>,
            input: WorkerInspectionScroll,
        ) -> Result<Option<bool>, String> {
            let registered = PointerPresentationView {
                revision: input.view_revision,
                width: input.viewport_width,
                height: input.viewport_height,
            };
            registered.validate()?;
            if !registered.drawable()
                || !input.surface_x.is_finite()
                || !input.surface_y.is_finite()
                || !input.delta_pixels.is_finite()
            {
                return Err(
                    "worker inspection requires a finite cursor, delta and drawable viewport"
                        .into(),
                );
            }
            if let Some(receipt) = input.presentation {
                receipt.validate()?;
            }
            let view = target
                .session()
                .inspection_pointer_view(
                    input.view_revision,
                    Vec2::new(input.viewport_width, input.viewport_height),
                )
                .map_err(|error| error.to_string())?;
            if self.view != Some(registered) {
                return Ok(None);
            }
            let Some(issued) = self.issued.as_ref() else {
                self.refresh_pending = true;
                return Ok(None);
            };
            // Request a fresh coherent view only when the issued frame itself
            // is obsolete. A stale packet must not trigger an endless repaint.
            match issued.frame.validate_current(target.session(), view) {
                Ok(()) => {}
                Err(error) if browser_pointer_input::recoverable_frame_error(&error) => {
                    self.refresh_pending = true;
                    return Ok(None);
                }
                Err(error) => return Err(error.to_string()),
            }
            let Some(receipt) = input.presentation else {
                return Ok(None);
            };
            if self.presented != Some(receipt)
                || receipt.session != issued.session
                || receipt.sequence != issued.sequence
                || receipt.view_revision != input.view_revision
            {
                return Ok(None);
            }
            let changed = target
                .scroll_inspection(
                    &issued.frame,
                    view,
                    Vec2::new(input.surface_x, input.surface_y),
                    input.delta_pixels,
                )
                .map_err(|error| error.to_string())?;
            if changed {
                // Every fallible operation is complete. The session has already
                // cancelled buttons; never synthesize another release/cancel.
                if let Some(contact) = *binding {
                    let (pointer, view, viewport) = contact.identity_and_view();
                    self.rejected = Some(RejectedSource {
                        source: pointer.source,
                        pointer: (pointer.pointer as u32).cast_signed(),
                        view,
                        viewport: Some(viewport),
                    });
                }
                BrowserPointerBinding::retire_after_inspection(binding);
                self.retired_presentation = receipt.presentation;
                self.presented = None;
                self.presented_frame = None;
                self.issued = None;
                self.refresh_pending = true;
            }
            Ok(Some(changed))
        }

        pub(crate) fn submit(
            &mut self,
            target: &mut (impl BrowserPointerTarget + ?Sized),
            binding: &mut Option<BrowserPointerBinding>,
            sequence: &mut u64,
            input: BrowserPointerInput,
            receipt: Option<WorkerPointerReceipt>,
        ) -> Result<bool, String> {
            let pointer = input.pointer()?;
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
            sequence
                .checked_add(1)
                .ok_or("native input event sequence exhausted")?;
            if target.session().translation_drag_active()
                && binding.as_ref().is_some_and(|contact| {
                    contact.changes_captured_contact(pointer, input.view_revision, coordinates)
                })
            {
                // A changed contact mapping cannot inherit the captured hit.
                // Cancel through the shared input lane before admitting a
                // replacement source; that source needs a fresh presentation.
                browser_pointer_input::cancel_browser_pointer_input(
                    target, binding, sequence, true,
                )?;
                if let Some(old) = self.presented {
                    self.retired_presentation = old.presentation;
                }
                self.presented = None;
                self.presented_frame = None;
                self.refresh_pending = true;
                return Ok(false);
            }
            let needs_binding = input.needs_binding(*binding)?;
            if input.cancellation().is_some() {
                browser_pointer_input::submit_browser_pointer_input(
                    target, binding, sequence, input,
                )?;
                self.presented_frame = None;
                return Ok(true);
            }
            let drag_active = target.session().translation_drag_active();
            let captured_continuation = !needs_binding
                && drag_active
                && matches!(
                    input.kind,
                    BrowserPointerKind::Move | BrowserPointerKind::Release
                );
            if captured_continuation {
                let captured_frame = receipt.and_then(|receipt| {
                    self.presented_frame.as_ref().and_then(|presented| {
                        self.presented
                            .filter(|latest| presented.contains_drag_receipt(receipt, *latest))
                            .map(|_| presented.frame.clone())
                    })
                });
                if let Some(frame) = captured_frame {
                    match browser_pointer_input::submit_captured_browser_pointer_input(
                        target, binding, sequence, input, &frame,
                    ) {
                        Ok(()) => {
                            self.rejected = None;
                            if !target.session().translation_drag_active() {
                                self.presented_frame = None;
                            }
                            return Ok(true);
                        }
                        Err(BrowserPointerAdmissionError::Frame(error))
                            if browser_pointer_input::recoverable_frame_error(&error) => {}
                        Err(error) => return Err(error.to_string()),
                    }
                }
            }
            // A captured gesture cannot fall back to a newer camera mapping or
            // ordinary picking after its acquired mapping fails validation.
            // A dormant collector must not configure a native input binding in
            // callback-only scenes. Keep callback admission strict for actual
            // subscribers and retire uninterested sources through the same
            // bounded rejection path (including their asynchronous packet tail).
            let interested = binding.is_some() || target.session().has_native_pointer_subscribers();
            if let (true, false, Some(receipt), Some(issued)) = (
                interested,
                captured_continuation,
                receipt,
                self.issued.as_ref(),
            ) {
                if self.presented == Some(receipt)
                    && receipt.session == issued.session
                    && receipt.sequence == issued.sequence
                {
                    match browser_pointer_input::submit_presented_browser_pointer_input(
                        target,
                        binding,
                        sequence,
                        input,
                        &issued.frame,
                    ) {
                        Ok(()) => {
                            self.rejected = None;
                            if !drag_active && target.session().translation_drag_active() {
                                // Acquisition alone pins a mapping. Later receipt
                                // IDs never authorize a new hit or replace it.
                                self.presented_frame = Some(PresentedFrame {
                                    receipt,
                                    frame: issued.frame.clone(),
                                });
                            }
                            return Ok(true);
                        }
                        Err(BrowserPointerAdmissionError::Frame(error))
                            if browser_pointer_input::recoverable_frame_error(&error) => {}
                        Err(error) => return Err(error.to_string()),
                    }
                }
            }
            browser_pointer_input::cancel_browser_pointer_input(target, binding, sequence, true)?;
            self.presented_frame = None;
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
