from pathlib import Path

ROOT = Path.cwd()
def replace(path, old, new, count=1):
    p = ROOT / path
    s = p.read_text()
    assert s.count(old) == count, (path, s.count(old), old)
    p.write_text(s.replace(old, new))

# Run only after the exact shared-session candidate has been prepared. This
# script is staging-only; the published branch contains ordinary Rust sources.
p = 'crates/noon/src/execution_session/inspection.rs'
replace(p, '    RevisionExhausted,', '    RevisionExhausted,\n    InvalidScroll,')
replace(p, '            Self::RevisionExhausted =>', '            Self::InvalidScroll => f.write_str("inspection scroll delta must be finite"),\n            Self::RevisionExhausted =>')
replace(p, '    /// Reset viewer navigation explicitly,', '''    /// Scroll by normalized logical pixels (positive means zoom out).
    ///
    /// The response curve is shared by platform adapters, not a host animation.
    /// Extreme finite deltas saturate before exponentiation; relative zoom limits
    /// and cursor anchoring remain those of `zoom_inspection_view`.
    pub fn scroll_inspection_view(
        &mut self,
        displayed: &PointerFrameSnapshot,
        current_view: PointerFrameView,
        surface: Vec2,
        delta_pixels: f64,
    ) -> Result<bool, InspectionNavigationError> {
        if !delta_pixels.is_finite() {
            return Err(InspectionNavigationError::InvalidScroll);
        }
        let factor = (delta_pixels / 500.0).clamp(-16.0, 16.0).exp();
        self.zoom_inspection_view(displayed, current_view, surface, factor)
    }

    /// Reset viewer navigation explicitly,''')

p = 'crates/noon/src/execution_session/input/presentation.rs'
replace(p, '''        if self.runtime != session.runtime_identity() {''', '''        self.validate_presentation(session, current_view)
    }

    /// Validate publication/view identity without admitting input or evaluating
    /// callbacks. Hosts use this after presentation; ordinary input still uses
    /// `validate_current`, which enforces its existing callback barriers first.
    pub fn validate_presentation(
        &self,
        session: &ExecutionSession,
        current_view: PointerFrameView,
    ) -> Result<(), PointerFrameError> {
        if self.runtime != session.runtime_identity() {''')

p = 'crates/noon/src/live_program.rs'
replace(p, 'use std::error::Error;', 'use std::error::Error;\n\nmod inspection;')
replace(p, '    Input(ExecutionSessionInputError),', '    Input(ExecutionSessionInputError),\n    Inspection(crate::InspectionNavigationError),')
replace(p, '            Self::Input(error) => error.fmt(formatter),', '            Self::Input(error) => error.fmt(formatter),\n            Self::Inspection(error) => error.fmt(formatter),')
replace(p, '            Self::Input(error) => Some(error),', '            Self::Input(error) => Some(error),\n            Self::Inspection(error) => Some(error),')

p = 'crates/noon-native/src/execution_source.rs'
replace(p, 'use noon::integration::NativePointerInputToken;', 'use noon::integration::{NativePointerInputToken, PointerFrameSnapshot, PointerFrameView};')
replace(p, '    Camera2DState, NativeEventOccurrence,', '    Camera2DState, Vec2, NativeEventOccurrence,')
replace(p, '    fn camera(&self) -> Result<Camera2DState, NativeHostError>;', '''    fn camera(&self) -> Result<Camera2DState, NativeHostError> {
        self.session().inspection_camera().map_err(Into::into)
    }
    fn scroll_inspection_view(
        &mut self,
        displayed: &PointerFrameSnapshot,
        current_view: PointerFrameView,
        surface: Vec2,
        delta_pixels: f64,
    ) -> Result<bool, NativeHostError>;

    /// Validate the displayed view as well as the authored publication before
    /// releasing a live continuation. A view-only edit need not change the latter.
    fn admit_presented_frame(&mut self, frame: &PointerFrameSnapshot) -> Result<(), NativeHostError> {
        frame.validate_presentation(self.session(), frame.view())?;
        self.admit_presented_publication(frame.publication())
    }''')
replace(p, '''    fn camera(&self) -> Result<Camera2DState, NativeHostError> {
        self.session.camera().map_err(Into::into)
    }''', '''    fn scroll_inspection_view(
        &mut self,
        displayed: &PointerFrameSnapshot,
        current_view: PointerFrameView,
        surface: Vec2,
        delta_pixels: f64,
    ) -> Result<bool, NativeHostError> {
        self.session.scroll_inspection_view(displayed, current_view, surface, delta_pixels).map_err(Into::into)
    }''')
replace(p, '''    fn camera(&self) -> Result<Camera2DState, NativeHostError> {
        self.program.session().camera().map_err(Into::into)
    }''', '''    fn scroll_inspection_view(
        &mut self,
        displayed: &PointerFrameSnapshot,
        current_view: PointerFrameView,
        surface: Vec2,
        delta_pixels: f64,
    ) -> Result<bool, NativeHostError> {
        self.program.scroll_inspection_view(displayed, current_view, surface, delta_pixels)
            .map_err(|error| NativeHostError::Program(error.to_string()))
    }''')

p = 'crates/noon-native/src/lib.rs'
replace(p, '    pub height: u32,', '''    pub height: u32,
    /// Opt in to session-owned cursor-anchored inspection zoom. Authored camera
    /// values and time remain unchanged; ordinary native viewports default off.
    pub inspection_zoom: bool,''')
replace(p, '            height: 540,', '            height: 540,\n            inspection_zoom: false,')
replace(p, '                height: 180,', '                height: 180,\n                inspection_zoom: false,', 2)
replace(p, '    Program(String),', '    Program(String),\n    PointerFrame(noon::integration::PointerFrameError),\n    Inspection(noon::InspectionNavigationError),')
replace(p, '            Self::Program(message) =>', '            Self::PointerFrame(error) => error.fmt(formatter),\n            Self::Inspection(error) => error.fmt(formatter),\n            Self::Program(message) =>')
replace(p, 'impl std::error::Error for NativeHostError {}', '''impl std::error::Error for NativeHostError {}

impl From<noon::integration::PointerFrameError> for NativeHostError {
    fn from(value: noon::integration::PointerFrameError) -> Self { Self::PointerFrame(value) }
}
impl From<noon::InspectionNavigationError> for NativeHostError {
    fn from(value: noon::InspectionNavigationError) -> Self { Self::Inspection(value) }
}''')
replace(p, '        self.pointer.presented = Some(pointer_frame);', '''        self.execution.admit_presented_frame(&pointer_frame)?;
        self.pointer.presented = Some(pointer_frame);''')
replace(p, '        self.execution.admit_presented_publication(presented)?;\n', '')
replace(p, '            WindowEvent::MouseInput { state, button, .. } => {', '''            WindowEvent::MouseWheel { delta, .. } => {
                if let Err(error) = self.dispatch_inspection_scroll(delta, window.inner_size(), window.scale_factor()) {
                    self.fail(event_loop, error);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {''')

p = 'crates/noon-native/src/pointer_input.rs'
replace(p, 'const WINDOW_CURSOR:', 'mod inspection;\n\nconst WINDOW_CURSOR:')
replace(p, '''        PointerFrameView::new(
            self.pointer.view_revision,
            logical,
            self.execution.camera()?,
        )''', '''        self.execution.session().inspection_pointer_view(self.pointer.view_revision, logical)''')
replace(p, '        if outcome == PointerDispatch::Admitted {', '''        if outcome == PointerDispatch::Admitted
            || (outcome == PointerDispatch::Unsubscribed && self.config.inspection_zoom) {''')
p = 'crates/noon-native/src/selection_overlay/tests.rs'
replace(p, '            height: SIZE.height,', '            height: SIZE.height,\n            inspection_zoom: false,')

p = '.github/workflows/native-host-smoke.yml'
replace(p, '      - name: Present click selection and clear without advancing native time', '''      - name: Present inspection zoom and view recovery without advancing native time
        run: |
          xvfb-run -a -s "-screen 0 1280x720x24" \\
            cargo test -p noon-native native_surface_smoke_presents_inspection_zoom_and_recovery \\
              -- --ignored --nocapture --test-threads=1

      - name: Present click selection and clear without advancing native time''')
