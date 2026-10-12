use std::{cell::Cell, rc::Rc};

use wasm_bindgen::{prelude::*, JsCast};
use web_sys::{OffscreenCanvas, WebGl2RenderingContext};

/// Platform-owned WebGL context-loss state shared by canvas renderers.
pub(crate) struct WebGlContextLifecycle {
    context_lost: Rc<Cell<bool>>,
    recovery_pending: Rc<Cell<bool>>,
    canvas: OffscreenCanvas,
    loss_listener: Option<Closure<dyn FnMut(web_sys::Event)>>,
    restore_listener: Option<Closure<dyn FnMut(web_sys::Event)>>,
}

impl WebGlContextLifecycle {
    pub(crate) fn install(
        canvas: &OffscreenCanvas,
        backend: wgpu::Backend,
    ) -> Result<Self, JsValue> {
        let context_lost = Rc::new(Cell::new(false));
        let recovery_pending = Rc::new(Cell::new(false));
        if backend != wgpu::Backend::Gl {
            return Ok(Self {
                context_lost,
                recovery_pending,
                canvas: canvas.clone(),
                loss_listener: None,
                restore_listener: None,
            });
        }

        let lost_state = Rc::clone(&context_lost);
        let loss_listener = Closure::wrap(Box::new(move |event: web_sys::Event| {
            event.prevent_default();
            lost_state.set(true);
        }) as Box<dyn FnMut(web_sys::Event)>);
        canvas.add_event_listener_with_callback(
            "webglcontextlost",
            loss_listener.as_ref().unchecked_ref(),
        )?;

        let restored_lost_state = Rc::clone(&context_lost);
        let restored_pending = Rc::clone(&recovery_pending);
        let restore_listener = Closure::wrap(Box::new(move |_event: web_sys::Event| {
            if restored_lost_state.replace(false) {
                restored_pending.set(true);
            }
        }) as Box<dyn FnMut(web_sys::Event)>);
        if let Err(error) = canvas.add_event_listener_with_callback(
            "webglcontextrestored",
            restore_listener.as_ref().unchecked_ref(),
        ) {
            let _ = canvas.remove_event_listener_with_callback(
                "webglcontextlost",
                loss_listener.as_ref().unchecked_ref(),
            );
            return Err(error);
        }

        Ok(Self {
            context_lost,
            recovery_pending,
            canvas: canvas.clone(),
            loss_listener: Some(loss_listener),
            restore_listener: Some(restore_listener),
        })
    }

    pub(crate) fn is_lost(&self) -> bool {
        self.context_lost.get()
    }

    pub(crate) fn recovery_pending(&self) -> bool {
        self.recovery_pending.get()
    }

    pub(crate) fn mark_recovery_pending(&self) {
        self.recovery_pending.set(true);
    }

    pub(crate) fn mark_lost(&self) {
        self.context_lost.set(true);
    }

    pub(crate) fn finish_recovery(&self) {
        self.recovery_pending.set(false);
    }
}

impl Drop for WebGlContextLifecycle {
    fn drop(&mut self) {
        if let Some(listener) = self.loss_listener.as_ref() {
            let _ = self.canvas.remove_event_listener_with_callback(
                "webglcontextlost",
                listener.as_ref().unchecked_ref(),
            );
        }
        if let Some(listener) = self.restore_listener.as_ref() {
            let _ = self.canvas.remove_event_listener_with_callback(
                "webglcontextrestored",
                listener.as_ref().unchecked_ref(),
            );
        }
    }
}

/// Query an already selected WebGL backend before wgpu/glow probes the context.
/// Callers must not use this before backend selection: `get_context("webgl2")`
/// would claim an otherwise unused canvas and prevent a WebGPU renderer.
pub(crate) fn webgl_context_is_lost(canvas: &OffscreenCanvas) -> Result<bool, JsValue> {
    let Some(context) = canvas.get_context("webgl2")? else {
        return Ok(false);
    };
    let context = context
        .dyn_into::<WebGl2RenderingContext>()
        .map_err(|_| JsValue::from_str("OffscreenCanvas returned a non-WebGL2 context"))?;
    Ok(context.is_context_lost())
}

pub(crate) fn ensure_webgl_context_available(canvas: &OffscreenCanvas) -> Result<(), JsValue> {
    if webgl_context_is_lost(canvas)? {
        return Err(JsValue::from_str(
            "cannot create a renderer while the WebGL2 context is lost",
        ));
    }
    Ok(())
}
