//! Browser-owned compilation task shared by direct and worker render hosts.
//! Requests own device work; polling never borrows or advances a scene.
use noon_render_wgpu::{
    EffectProgramPrepareError, EffectProgramStats, GpuRenderer, PreparedEffectPrograms,
};
use noon_runtime::RendererPublication;
use std::{cell::RefCell, rc::Rc};

type Pending = Rc<RefCell<Option<Result<PreparedEffectPrograms, EffectProgramPrepareError>>>>;

#[derive(Default)]
pub(crate) struct EffectProgramTask {
    pending: Option<Pending>,
    pub(crate) pipeline_compiles: u64,
}

impl EffectProgramTask {
    pub(crate) fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(crate) fn cancel(&mut self) {
        self.pending = None;
    }

    pub(crate) fn record(&mut self, work: EffectProgramStats) {
        self.pipeline_compiles = self
            .pipeline_compiles
            .saturating_add(work.pipeline_compiles as u64);
    }

    pub(crate) fn poll_modules(
        &mut self,
        renderer: &mut GpuRenderer,
        device: &wgpu::Device,
        programs: &[noon_core::WgslEffectProgram],
    ) -> Result<Option<PreparedEffectPrograms>, EffectProgramPrepareError> {
        let completed = self
            .pending
            .as_ref()
            .and_then(|pending| pending.borrow_mut().take());
        if self.pending.is_some() && completed.is_none() {
            return Ok(None);
        }
        self.pending = None;
        let completed = completed.transpose()?;
        if let Some(prepared) = &completed {
            self.record(prepared.stats());
            renderer
                .cache_effect_programs(device, prepared)
                .map_err(EffectProgramPrepareError::Profile)?;
        }
        match renderer
            .effect_module_preparation(device, programs)
            .try_ready()
        {
            Ok(ready) => Ok(Some(ready)),
            Err(request) => {
                let pending = Rc::new(RefCell::new(None));
                let result = Rc::clone(&pending);
                wasm_bindgen_futures::spawn_local(async move {
                    *result.borrow_mut() = Some(request.prepare().await);
                });
                self.pending = Some(pending);
                Ok(None)
            }
        }
    }

    /// False retains the caller's pending frame and dirty changes. A completed
    /// task may belong to an older publication: reuse its immutable programs,
    /// then replan the current delta instead of installing stale row bindings.
    pub(crate) fn poll(
        &mut self,
        renderer: &mut GpuRenderer,
        device: &wgpu::Device,
        publication: &RendererPublication<'_>,
    ) -> Result<bool, EffectProgramPrepareError> {
        let completed = if let Some(pending) = &self.pending {
            let Some(completed) = pending.borrow_mut().take() else {
                return Ok(false);
            };
            Some(completed)
        } else {
            None
        };
        // Clear even on failure, so a later corrected publication can retry.
        self.pending = None;
        let completed = completed.transpose()?;
        if let Some(completed) = &completed {
            self.record(completed.stats());
            renderer
                .cache_effect_programs(device, completed)
                .map_err(EffectProgramPrepareError::Profile)?;
        }
        if renderer.effect_programs_current(device, publication.context()) {
            return Ok(true);
        }
        match renderer
            .effect_program_preparation(device, publication)
            .map_err(EffectProgramPrepareError::Profile)?
            .try_ready()
        {
            Ok(prepared) => {
                renderer
                    .commit_effect_programs(device, publication, prepared)
                    .map_err(EffectProgramPrepareError::Profile)?;
                Ok(true)
            }
            Err(preparation) => {
                let pending = Rc::new(RefCell::new(None));
                let result = Rc::clone(&pending);
                wasm_bindgen_futures::spawn_local(async move {
                    *result.borrow_mut() = Some(preparation.prepare().await);
                });
                self.pending = Some(pending);
                Ok(false)
            }
        }
    }
}

/// Opaque owned device work at the genuine worker boundary. The renderer and
/// semantic owner are never borrowed while the compilation future is pending.
#[wasm_bindgen::prelude::wasm_bindgen]
pub struct WasmPreparedEffectPrograms {
    pub(crate) prepared: PreparedEffectPrograms,
}

#[wasm_bindgen::prelude::wasm_bindgen]
impl WasmPreparedEffectPrograms {}
