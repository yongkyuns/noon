//! Player forwarding for the view color profile operation.

impl super::SemanticExecutionPlayer {
    pub(crate) fn live_set_view_color_profile(
        &mut self,
        camera: &noon::Mobject,
        profile: noon_core::ViewColorProfile,
    ) -> Result<(), crate::AuthoringFailure> {
        self.with_live_session(|live| live.set_view_color_profile(camera, profile))
    }
}
