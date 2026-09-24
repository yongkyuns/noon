//! Ordinary authoring facade for an independently driven restoring action.

use super::{IndicateOptions, LiveSession, LiveSessionError};
use crate::Mobject;
use noon_core::{AnimationOptions, SemanticVec3};
use noon_runtime::PropertyAnimationToken;

impl LiveSession<'_> {
    /// Start a restoring Indicate without appending to or advancing authored time.
    ///
    /// Capture the same effective layout center used by ordinary live Indicate,
    /// then use the same shared semantic lowerer. The returned token identifies
    /// Runtime's operation, not a blocking play segment. No callback or source
    /// continuation is installed. None means the requested action changes nothing.
    pub fn start_indicate_effect(
        &mut self,
        target: &Mobject,
        indication: IndicateOptions,
        options: AnimationOptions,
    ) -> Result<Option<PropertyAnimationToken>, LiveSessionError> {
        let layout = self.effective_layout(target)?;
        self.session
            .start_indicate_effect(
                &mut self.store.borrow_mut(),
                target.node_id(),
                SemanticVec3::new(layout.center.0, layout.center.1, 0.0),
                indication,
                options,
            )
            .map_err(Into::into)
    }
}
