//! Direct Rust proof for compiler-ordered host callbacks on the canonical runtime.

use noon::{AnimationOptions, HostCallbackId, RateFunction, RustHostCallbackTable, Scene};

const SET_Y: HostCallbackId = HostCallbackId::new(1);
const SET_OPACITY: HostCallbackId = HostCallbackId::new(2);
const ACCUMULATE_DT: HostCallbackId = HostCallbackId::new(3);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let mut source = scene.circle(1.0)?;
    source.set_fill(1.0, 1.0, 1.0, 1.0)?;
    let mut drift = scene.circle(0.5)?;
    drift.set_fill(1.0, 1.0, 1.0, 1.0)?;
    drift.set_translation(-3.0, 0.0)?;
    scene.add(&source)?;
    scene.add(&drift)?;

    let mut target = source.target_editor()?;
    target.set_translation(2.0, 0.0)?;
    let animation = scene.declare_transform_to(
        &source,
        &target,
        AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear),
    )?;

    let mut callbacks = RustHostCallbackTable::new();
    callbacks.insert(SET_Y, |context| {
        let mut transform = context.target_state().transform;
        transform.translation.y = 1.0;
        context.set_target_transform(transform)
    })?;
    callbacks.insert(SET_OPACITY, |context| {
        // This reads the same overlay after SET_Y, proving ordered read-after-write.
        debug_assert_eq!(context.target_state().transform.translation.y, 1.0);
        let mut style = context.target_state().style;
        style.opacity = 0.5;
        context.set_target_style(style)
    })?;
    callbacks.insert(ACCUMULATE_DT, |context| {
        let mut transform = context.target_state().transform;
        transform.translation.y += context.delta_time() as f32;
        context.set_target_transform(transform)
    })?;

    {
        let mut store = scene.store().borrow_mut();
        callbacks.add_updater(&mut store, source.node_id(), SET_Y, 0.0, None)?;
        callbacks.add_updater(&mut store, source.node_id(), SET_OPACITY, 0.0, None)?;
        callbacks.add_updater(&mut store, drift.node_id(), ACCUMULATE_DT, 0.0, None)?;
    }

    // Activate only after callback occurrences and animation intent are authored,
    // then let the native realtime host drive every required phase while rendering.
    let mut session = scene.execution_session()?;
    {
        let mut live = scene.live(&mut session);
        live.play_animation(&animation)?;
    }
    noon_native::run_with_callbacks(session, callbacks)?;
    Ok(())
}
