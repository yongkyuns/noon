from pathlib import Path

def rep(s,a,b):
    assert s.count(a)==1,(a[:100],s.count(a))
    return s.replace(a,b,1)

p=Path("crates/noon/src/live_program.rs"); s=p.read_text()
s=rep(s, "use crate::execution_session::ExecutionViewportQuery;", "use crate::execution_session::{ExecutionViewportQuery, NativePointerInputPublication, NativePointerInputToken};")
s=rep(s,"""use noon_core::{
    NativeEventOccurrence, NativeInputValue, NativeStateSource, PublicationContext, Rect,
};""","""use noon_core::{
    NativeEventOccurrence, NativeInputValue, NativePointerId, NativePointerInput,
    NativeStateSource, PublicationContext, Rect,
};""")
s=rep(s,"""    ExecutionSegment, ExecutionSegmentAdvanceError, ExecutionSegmentState, ExecutionSession,
    ExecutionSessionInputError, LiveSession, RustHostCallbackError, RustHostCallbackTable, Scene,
};""","""    ExecutionSegment, ExecutionSegmentAdvanceError, ExecutionSegmentState, ExecutionSession,
    ExecutionSessionInputError, LiveSession, RustHostCallbackError, RustHostCallbackTable, Scene,
};""")
s=rep(s,"""    /// Deliver one normalized sampled native value without exposing mutable session authority.
    pub fn set_native_state_input(""","""    /// Configure the platform pointer projected into the existing unkeyed native signals.
    pub fn configure_native_pointer_input(
        &mut self,
        pointer: NativePointerId,
        view_revision: u64,
    ) -> Result<NativePointerInputToken, LiveProgramError<C::Error>> {
        self.ensure_host_input_available("configure contextual pointer input")?;
        let token = self.scene.owned_execution_mut()
            .configure_native_pointer_input(pointer, view_revision)
            .map_err(LiveProgramError::Input)?;
        self.refresh_pending_publication();
        Ok(token)
    }

    pub fn native_pointer_input_token(
        &self,
    ) -> Result<NativePointerInputToken, LiveProgramError<C::Error>> {
        self.ensure_host_input_available("capture contextual pointer input")?;
        self.session().native_pointer_input_token().map_err(LiveProgramError::Input)
    }

    pub fn submit_native_pointer_input(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<NativePointerInputPublication, LiveProgramError<C::Error>> {
        self.ensure_host_input_available("deliver contextual pointer input")?;
        let publication = self.scene.owned_execution_mut()
            .submit_native_pointer_input(token, input)
            .map_err(LiveProgramError::Input)?;
        self.refresh_pending_publication();
        Ok(publication)
    }

    /// Deliver one normalized sampled native value without exposing mutable session authority.
    pub fn set_native_state_input(""")
p.write_text(s)

p=Path("crates/noon-native/src/execution_source.rs"); s=p.read_text()
s=rep(s,"""use noon::{
    ExecutionSession, LiveContinuation, LiveProgram, LiveProgramStatus, RustHostCallbackTable,
};""","""use noon::{
    ExecutionSession, LiveContinuation, LiveProgram, LiveProgramStatus, NativePointerInputToken,
    RustHostCallbackTable,
};""")
s=rep(s,"""    Camera2DState, NativeEventOccurrence, NativeInputValue, NativeStateSource, PublicationContext,
    Rect,
};""","""    Camera2DState, NativeEventOccurrence, NativeInputValue, NativePointerId, NativePointerInput,
    NativeStateSource, PublicationContext, Rect,
};""")
s=rep(s,"""    fn set_native_state_input(
        &mut self,
        source: NativeStateSource,
        value: NativeInputValue,
    ) -> Result<(), NativeHostError>;""","""    fn configure_native_pointer_input(
        &mut self,
        pointer: NativePointerId,
        view_revision: u64,
    ) -> Result<NativePointerInputToken, NativeHostError>;
    fn native_pointer_input_token(&self) -> Result<NativePointerInputToken, NativeHostError>;
    fn submit_native_pointer_input(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<(), NativeHostError>;
    fn set_native_state_input(
        &mut self,
        source: NativeStateSource,
        value: NativeInputValue,
    ) -> Result<(), NativeHostError>;""")
anchor="""    fn set_native_state_input(
        &mut self,
        source: NativeStateSource,
        value: NativeInputValue,
    ) -> Result<(), NativeHostError> {
        self.session
            .set_native_state_input(source, value)
            .map(|_| ())
            .map_err(Into::into)
    }"""
s=rep(s,anchor,"""    fn configure_native_pointer_input(
        &mut self, pointer: NativePointerId, view_revision: u64,
    ) -> Result<NativePointerInputToken, NativeHostError> {
        self.session.configure_native_pointer_input(pointer, view_revision).map_err(Into::into)
    }
    fn native_pointer_input_token(&self) -> Result<NativePointerInputToken, NativeHostError> {
        self.session.native_pointer_input_token().map_err(Into::into)
    }
    fn submit_native_pointer_input(
        &mut self, token: &NativePointerInputToken, input: NativePointerInput,
    ) -> Result<(), NativeHostError> {
        self.session.submit_native_pointer_input(token, input).map(|_| ()).map_err(Into::into)
    }

"""+anchor)
anchor="""    fn set_native_state_input(
        &mut self,
        source: NativeStateSource,
        value: NativeInputValue,
    ) -> Result<(), NativeHostError> {
        self.program
            .set_native_state_input(source, value)
            .map(|_| ())
            .map_err(|error| NativeHostError::Program(error.to_string()))
    }"""
s=rep(s,anchor,"""    fn configure_native_pointer_input(
        &mut self, pointer: NativePointerId, view_revision: u64,
    ) -> Result<NativePointerInputToken, NativeHostError> {
        self.program.configure_native_pointer_input(pointer, view_revision)
            .map_err(|error| NativeHostError::Program(error.to_string()))
    }
    fn native_pointer_input_token(&self) -> Result<NativePointerInputToken, NativeHostError> {
        self.program.native_pointer_input_token()
            .map_err(|error| NativeHostError::Program(error.to_string()))
    }
    fn submit_native_pointer_input(
        &mut self, token: &NativePointerInputToken, input: NativePointerInput,
    ) -> Result<(), NativeHostError> {
        self.program.submit_native_pointer_input(token, input).map(|_| ())
            .map_err(|error| NativeHostError::Program(error.to_string()))
    }

"""+anchor)
p.write_text(s)

p=Path("crates/noon-native/src/lib.rs"); s=p.read_text()
s=rep(s,"use noon::integration::{RendererPublication, TimelineWakeState};","""use noon::integration::{
    NativeInputModifiers, NativePointerCancellation, NativePointerId, NativePointerInput,
    NativePointerInputKind, NativePointerPosition, RendererPublication, TimelineWakeState,
};""")
s=rep(s,"use winit::dpi::PhysicalSize;","use winit::dpi::{PhysicalPosition, PhysicalSize};")
s=rep(s,"use winit::keyboard::{KeyCode, PhysicalKey};","use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};")
s=rep(s,"const CLEAR_COLOR: wgpu::Color = wgpu::Color {","""const NATIVE_MOUSE_POINTER: NativePointerId = NativePointerId { source: 1, pointer: 0 };

const CLEAR_COLOR: wgpu::Color = wgpu::Color {""")
s=rep(s,"""    next_input_sequence: u64,
    force_full_redraw: bool,""","""    next_input_sequence: u64,
    pointer_view_revision: u64,
    last_pointer_surface: Option<Vec2>,
    modifiers: ModifiersState,
    force_full_redraw: bool,""")
s=rep(s,"""            next_input_sequence: 0,
            force_full_redraw: false,""","""            next_input_sequence: 0,
            pointer_view_revision: 0,
            last_pointer_surface: None,
            modifiers: ModifiersState::empty(),
            force_full_redraw: false,""")
old="""    fn dispatch_pointer_button(
        &mut self,
        button: MouseButton,
        state: ElementState,
    ) -> Result<(), NativeHostError> {
        let Some(button) = native_pointer_button(button) else {
            return Ok(());
        };
        let pressed = state == ElementState::Pressed;
        self.dispatch_state(
            NativeStateSource::PointerButton { button },
            NativeInputValue::Bool(pressed),
        )?;
        self.dispatch_event(if pressed {
            NativeEventSource::PointerDown { button }
        } else {
            NativeEventSource::PointerUp { button }
        })
    }"""
new="""    fn configure_pointer_input(&mut self) -> Result<(), NativeHostError> {
        self.execution.configure_native_pointer_input(NATIVE_MOUSE_POINTER, self.pointer_view_revision)?;
        Ok(())
    }

    fn rebind_pointer_view(&mut self) -> Result<(), NativeHostError> {
        self.pointer_view_revision = self.pointer_view_revision.checked_add(1).ok_or_else(|| {
            NativeHostError::Platform("native pointer view revision exhausted".to_owned())
        })?;
        self.configure_pointer_input()
    }

    fn current_modifiers(&self) -> NativeInputModifiers {
        NativeInputModifiers {
            shift: self.modifiers.shift_key(),
            control: self.modifiers.control_key(),
            alt: self.modifiers.alt_key(),
            meta: self.modifiers.super_key(),
        }
    }

    fn pointer_position(
        &self, surface: Vec2, logical_size: Vec2, camera: Camera2DState,
    ) -> Result<NativePointerPosition, NativeHostError> {
        if logical_size.x <= 0.0 || logical_size.y <= 0.0
            || !logical_size.x.is_finite() || !logical_size.y.is_finite() {
            return Err(NativeHostError::Platform(
                "native pointer viewport dimensions must be finite and positive".to_owned(),
            ));
        }
        let world_width = camera.height * (logical_size.x / logical_size.y);
        let scene = Vec2::new(
            camera.center.x + (surface.x / logical_size.x - 0.5) * world_width,
            camera.center.y + (0.5 - surface.y / logical_size.y) * camera.height,
        );
        NativePointerPosition::new(scene, surface)
            .map_err(|error| NativeHostError::Platform(error.to_string()))
    }

    fn dispatch_pointer_kind(&mut self, kind: NativePointerInputKind) -> Result<(), NativeHostError> {
        let sequence = self.next_input_sequence;
        let next = sequence.checked_add(1).ok_or_else(|| {
            NativeHostError::Platform("native input event sequence exhausted".to_owned())
        })?;
        let token = self.execution.native_pointer_input_token()?;
        let input = NativePointerInput::new(
            sequence, token.pointer(), token.context(), self.current_modifiers(), kind,
        );
        self.execution.submit_native_pointer_input(&token, input)?;
        self.next_input_sequence = next;
        Ok(())
    }

    fn dispatch_pointer_position(
        &mut self, window: &Window, physical: PhysicalPosition<f64>,
    ) -> Result<(), NativeHostError> {
        let surface = physical.to_logical::<f32>(window.scale_factor());
        let logical_size = window.inner_size().to_logical::<f32>(window.scale_factor());
        let surface = Vec2::new(surface.x, surface.y);
        let position = self.pointer_position(
            surface, Vec2::new(logical_size.width, logical_size.height), self.execution.camera()?,
        )?;
        self.last_pointer_surface = Some(surface);
        self.dispatch_pointer_kind(NativePointerInputKind::Move(position))
    }

    fn dispatch_pointer_button(
        &mut self, window: &Window, button: MouseButton, state: ElementState,
    ) -> Result<(), NativeHostError> {
        let Some(button) = native_pointer_button(button) else { return Ok(()); };
        let surface = self.last_pointer_surface.ok_or_else(|| {
            NativeHostError::Platform(
                "native pointer button arrived before a pointer position was observed".to_owned(),
            )
        })?;
        let logical_size = window.inner_size().to_logical::<f32>(window.scale_factor());
        let position = self.pointer_position(
            surface, Vec2::new(logical_size.width, logical_size.height), self.execution.camera()?,
        )?;
        self.dispatch_pointer_kind(if state == ElementState::Pressed {
            NativePointerInputKind::Press { position, button }
        } else {
            NativePointerInputKind::Release { position, button }
        })
    }

    fn cancel_pointer(&mut self, reason: NativePointerCancellation) -> Result<(), NativeHostError> {
        self.dispatch_pointer_kind(NativePointerInputKind::Cancel(reason))
    }"""
s=rep(s,old,new)
s=rep(s,"""            if let Err(error) = self.dispatch_viewport_size(&window, window.inner_size()) {
                self.fail(event_loop, error);
                return;
            }
            self.window = Some(window);""","""            if let Err(error) = self.dispatch_viewport_size(&window, window.inner_size()) {
                self.fail(event_loop, error);
                return;
            }
            if let Err(error) = self.configure_pointer_input() {
                self.fail(event_loop, error);
                return;
            }
            self.window = Some(window);""")
s=rep(s,"""                    self.force_full_redraw = true;
                }
            }
            WindowEvent::KeyboardInput""","""                    self.force_full_redraw = true;
                }
                if let Err(error) = self.rebind_pointer_view() {
                    self.fail(event_loop, error);
                    return;
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
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
            WindowEvent::KeyboardInput""")
s=rep(s,"""            WindowEvent::MouseInput { state, button, .. } => {
                if let Err(error) = self.dispatch_pointer_button(button, state) {""","""            WindowEvent::MouseInput { state, button, .. } => {
                if let Err(error) = self.dispatch_pointer_button(&window, button, state) {""")
p.write_text(s)
print("patched")
