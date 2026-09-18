from pathlib import Path
import runpy

runpy.run_path('.github/c2-native-pointer-adapter.py')

def rep(s, old, new):
    assert s.count(old) == 1, (old[:100], s.count(old))
    return s.replace(old, new, 1)

p = Path('crates/noon-native/src/execution_source.rs')
s = p.read_text()
s = rep(s, 'LiveProgramStatus, NativePointerInputToken,', 'LiveProgramStatus,')
s = 'use noon::integration::NativePointerInputToken;\n' + s
s = s.replace('    #[cfg(test)]\n    fn session(&self)', '    fn session(&self)')
p.write_text(s)

p = Path('crates/noon/src/live_program.rs')
s = p.read_text()
s = rep(s, '    pub fn native_pointer_input_token(', '    /// Capture the current session context without exposing mutable execution authority.\n    pub fn native_pointer_input_token(')
s = rep(s, '    pub fn submit_native_pointer_input(', '    /// Publish one occurrence and refresh an outstanding endpoint receipt after success.\n    pub fn submit_native_pointer_input(')
s += '\n#[cfg(test)]\nmod pointer_input_tests;\n'
p.write_text(s)

p = Path('crates/noon/src/execution_session/input.rs')
s = p.read_text()
s = rep(s, 'impl ExecutionSession {', '''impl ExecutionSession {
    /// Whether any native reactive route observes this collector's pointer vocabulary.
    ///
    /// This bounded lookup inspects lowered routes, not objects or authored scene
    /// graphs. It is an adapter-interest query, not admission: explicit typed
    /// callers still deliver unbound records through the normal session contract.
    /// Future interaction consumers must extend input interest at the same owner.
    pub fn has_native_pointer_subscribers(&self) -> bool {
        !self.reactive_projection.native_state_targets(&NativeStateSource::PointerPosition).is_empty()
            || (0..=u8::MAX).any(|button| {
                !self.reactive_projection.native_state_targets(&NativeStateSource::PointerButton { button }).is_empty()
                    || !self.reactive_projection.native_event_targets(&NativeEventSource::PointerDown { button }).is_empty()
                    || !self.reactive_projection.native_event_targets(&NativeEventSource::PointerUp { button }).is_empty()
            })
    }
''')
p.write_text(s)

p = Path('crates/noon-native/src/lib.rs')
s = p.read_text()
a = s.index('    fn configure_pointer_input(')
b = s.index('    fn realtime_clock_for_timeline(', a)
s = s[:a] + s[b:]
a = s.index('use noon::integration::{')
b = s.index('};', a) + 2
s = s[:a] + 'use noon::integration::{RendererPublication, TimelineWakeState};' + s[b:]
s = rep(s, 'use winit::dpi::{PhysicalPosition, PhysicalSize};', 'use winit::dpi::PhysicalSize;')
s = rep(s, 'use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};', 'use winit::keyboard::{KeyCode, PhysicalKey};')
s = rep(s, 'const NATIVE_MOUSE_POINTER: NativePointerId = NativePointerId { source: 1, pointer: 0 };\n\n', '')
s = rep(s, 'mod execution_source;', 'mod execution_source;\nmod pointer_input;')
s = rep(s, '''    pointer_view_revision: u64,
    last_pointer_surface: Option<Vec2>,
    modifiers: ModifiersState,''', '    pointer: pointer_input::PointerCollector,')
s = rep(s, '''            pointer_view_revision: 0,
            last_pointer_surface: None,
            modifiers: ModifiersState::empty(),''', '            pointer: pointer_input::PointerCollector::default(),')
s = rep(s, '''            if let Err(error) = self.configure_pointer_input() {
                self.fail(event_loop, error);
                return;
            }
''', '')
s = rep(s, '''            WindowEvent::ScaleFactorChanged { .. } => {
                if let Err(error) = self.rebind_pointer_view() {
                    self.fail(event_loop, error);
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Err(error) = self.dispatch_pointer_position(&window, position) {
                    self.fail(event_loop, error);
                }
            }
            WindowEvent::Focused(false) => {
                if let Err(error) = self.cancel_pointer(NativePointerCancellation::FocusLost) {
                    self.fail(event_loop, error);
                }
            }
''', '''            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Err(error) = self.pointer_scale_changed(window.inner_size(), scale_factor) {
                    self.fail(event_loop, error);
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.pointer.modifiers = modifiers.state();
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Err(error) = self.dispatch_pointer_position(
                    position, window.inner_size(), window.scale_factor(),
                ) {
                    self.fail(event_loop, error);
                }
            }
            WindowEvent::CursorLeft { .. } => {
                // This shell does not promise cross-platform OS capture. Leaving
                // the surface explicitly cancels, rather than risking a stuck drag.
                if let Err(error) = self.pointer_left() {
                    self.fail(event_loop, error);
                }
            }
            WindowEvent::Focused(false) => {
                if let Err(error) = self.pointer_focus_lost() {
                    self.fail(event_loop, error);
                }
            }
''')
s = rep(s, 'self.dispatch_pointer_button(&window, button, state)', 'self.dispatch_pointer_button(button, state, window.inner_size(), window.scale_factor())')
s = rep(s, '    fn resumed(&mut self, event_loop: &ActiveEventLoop) {', '''    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.pointer_left() {
            self.fail(event_loop, error);
        }
        self.gpu = None;
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {''')
p.write_text(s)

for p in Path('crates/noon/src').rglob('*.rs'):
    s = p.read_text()
    marker = 'fn ensure_direct_input_ingress_available('
    if marker in s:
        a = s.index(marker)
        print(p, s[a:a+850])
print('Native adapter assembled with lifecycle and LiveProgram regression modules.')
