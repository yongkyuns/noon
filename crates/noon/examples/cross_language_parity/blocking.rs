//! Diagnostic host for an ordinary source stack. Frames are output artifacts;
//! the canonical program remains the only scene/runtime throughout the run.
use std::{cell::RefCell, rc::Rc};

use noon::{
    diagnostics::execution_frame_value,
    integration::{BlockingBoundary, BlockingContinuation, BlockingSceneHost, BlockingSceneSetup},
    BlockingScene, LiveProgram, LiveProgramStatus, RustHostCallbackTable, Scene, ScriptError,
};
use serde_json::{json, Value};

struct DiagnosticHost {
    setup: RefCell<Option<BlockingSceneSetup>>,
    program: RefCell<Option<LiveProgram<BlockingContinuation>>>,
    times: Vec<f64>,
    frames: RefCell<Vec<Value>>,
}

impl DiagnosticHost {
    fn initialize(&self) -> Result<(), ScriptError> {
        if let Some(setup) = self.setup.borrow_mut().take() {
            *self.program.borrow_mut() = Some(setup.into_program()?);
        }
        Ok(())
    }
}

impl BlockingSceneHost for DiagnosticHost {
    fn with_scene(
        &self,
        operation: &mut dyn FnMut(
            &mut Scene,
            &mut RustHostCallbackTable,
        ) -> Result<(), ScriptError>,
    ) -> Result<(), ScriptError> {
        if let Some(setup) = self.setup.borrow_mut().as_mut() {
            return setup.with_scene(operation);
        }
        self.program
            .borrow_mut()
            .as_mut()
            .ok_or(ScriptError::Inactive)?
            .with_authoring_scene(operation)
    }

    fn wait(&self, boundary: BlockingBoundary) -> Result<(), ScriptError> {
        self.initialize()?;
        let mut owner = self.program.borrow_mut();
        let program = owner.as_mut().ok_or(ScriptError::Inactive)?;
        program.begin_boundary(boundary)?;
        // This offline diagnostic does not reenter the source or suspend its
        // thread. Only the real platform hosts service external callbacks here.
        loop {
            match program.status() {
                LiveProgramStatus::ReadyToResume => return Ok(()),
                LiveProgramStatus::PublicationPending(expected) => {
                    let actual = program.take_renderer_publication().context();
                    assert_eq!(actual, expected);
                    program
                        .admit_publication(actual)
                        .map_err(|error| ScriptError::Host(error.to_string()))?;
                }
                LiveProgramStatus::Awaiting(_) => {
                    let index = self.frames.borrow().len();
                    let time = *self.times.get(index).ok_or_else(|| {
                        ScriptError::Host(
                            "synchronous parity source exceeded its sample range".into(),
                        )
                    })?;
                    let status = program
                        .drive_to(&mut RustHostCallbackTable::new(), time)
                        .map_err(|error| ScriptError::Host(error.to_string()))?;
                    if matches!(status, LiveProgramStatus::Awaiting(_)) {
                        program.take_renderer_publication();
                        assert_eq!(program.session().frame().time, time);
                        self.frames
                            .borrow_mut()
                            .push(execution_frame_value(program.session()));
                    }
                    // At an endpoint let ordinary source code run first. The
                    // following boundary samples its post-play edits at that
                    // same authored time, matching the existing parity host.
                }
                state => {
                    return Err(ScriptError::Host(format!(
                        "unexpected parity phase: {state:?}"
                    )))
                }
            }
        }
    }
}

pub fn emit() -> Result<(), String> {
    let host = Rc::new(DiagnosticHost {
        setup: RefCell::new(Some(BlockingSceneSetup::new())),
        program: RefCell::new(None),
        times: (0..=8).map(|index| f64::from(index) * 0.125).collect(),
        frames: RefCell::new(Vec::new()),
    });
    let erased: Rc<dyn BlockingSceneHost> = host.clone();
    let scene = BlockingScene::for_host(Rc::downgrade(&erased));
    noon::example_scenes::blocking_authoring::construct(&scene)
        .map_err(|error| error.to_string())?;
    host.initialize().map_err(|error| error.to_string())?;
    {
        let mut owner = host.program.borrow_mut();
        let program = owner.as_mut().expect("source initialized its program");
        program
            .finish_authoring()
            .map_err(|error| error.to_string())?;
        assert_eq!(program.session().frame().time, *host.times.last().unwrap());
        host.frames
            .borrow_mut()
            .push(execution_frame_value(program.session()));
    }
    assert_eq!(host.frames.borrow().len(), host.times.len());
    println!(
        "{}",
        json!({
            "name": "ordinary_blocking_authoring",
            "times": host.times,
            "frames": *host.frames.borrow(),
        })
    );
    Ok(())
}
