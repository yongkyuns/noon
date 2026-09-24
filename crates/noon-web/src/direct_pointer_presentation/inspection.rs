//! Direct DOM scroll admission against the last successfully presented view.
//! Only receipt lifecycle belongs here; all zoom mathematics and cancellation
//! are owned by ExecutionSession / LiveProgram.
use super::*;

impl DirectPointerPresentation {
    /// None rejects a stale/unpresented occurrence without replay; Some(false)
    /// is an admitted exact no-op; Some(true) changed the session view.
    pub(crate) fn scroll(
        &mut self,
        target: &mut (impl BrowserPointerTarget + ?Sized),
        binding: &mut Option<BrowserPointerBinding>,
        revision: u64,
        viewport: Vec2,
        surface: Vec2,
        delta_pixels: f64,
    ) -> Result<Option<bool>, String> {
        if revision > (1_u64 << 53) - 1
            || !delta_pixels.is_finite()
            || !surface.x.is_finite()
            || !surface.y.is_finite()
        {
            return Err(
                "inspection scroll coordinates, delta and view revision must be finite/exact"
                    .into(),
            );
        }
        let view = target
            .session()
            .inspection_pointer_view(revision, viewport)
            .map_err(|error| error.to_string())?;
        let Some(frame) = self.presented.as_ref() else {
            self.refresh_pending = true;
            return Ok(None);
        };
        // Compare with host-registered geometry as well as the offered record.
        // The DOM cannot authorize an old view by resending its old dimensions.
        if self.revision != revision || self.view != Some(viewport) {
            self.refresh_pending = true;
            return Ok(None);
        }
        match frame.validate_current(target.session(), view) {
            Ok(()) => {}
            Err(error) if recoverable_frame_error(&error) => {
                self.refresh_pending = true;
                return Ok(None);
            }
            Err(error) => return Err(error.to_string()),
        }
        let changed = target.scroll_inspection(frame, view, surface, delta_pixels)?;
        if changed {
            BrowserPointerBinding::retire_after_inspection(binding);
            self.invalidate();
        }
        Ok(Some(changed))
    }
}
