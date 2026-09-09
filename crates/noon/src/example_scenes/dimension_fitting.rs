//! Paired with ordinary_dimension_fitting.py on native and direct Rust/WASM.
use crate::{
    ExecutionSession, LayoutAnchor,
    LayoutDimension::{Height, Width},
    Scene,
};

pub fn session() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let mut a = scene.rectangle(2.0, 1.0)?;
    let mut b = scene.square(1.0)?;
    a.set_fill(0.2, 0.4, 1.0, 1.0)?;
    b.set_fill(1.0, 0.8, 0.1, 1.0)?;
    a.set_stroke_width(0.0)?;
    b.set_stroke_width(0.0)?;
    let source = LayoutAnchor::from(&a);
    let target = LayoutAnchor::from(&b);
    source.rescale_to_fit(2.0, Height, false)?;
    source.match_dim_size(&target, Height, true)?;
    a.shift(-2.0, 0.0)?;
    b.shift(2.0, 0.0)?;
    let family = scene.family(&[(&a).into(), (&b).into(), (&a).into()])?;
    let group = LayoutAnchor::from(&family);
    group.rescale_to_fit(4.0, Width, false)?;
    scene
        .add_many(&[(&family).into()])
        .map_err(|error| error.to_string())?;
    let mut session = scene.execution_session().map_err(|e| e.to_string())?;
    {
        let mut live = scene.live(&mut session);
        live.match_dim_size(&source, &target, Width, true)
            .map_err(|e| e.to_string())?;
        live.rescale_to_fit(&group, 2.0, Height, true)
            .map_err(|e| e.to_string())?;
        assert!(
            (live.effective_layout(&a).map_err(|e| e.to_string())?.width
                - live.effective_layout(&b).map_err(|e| e.to_string())?.width)
                .abs()
                < 1e-6
        );
        assert!(
            (live
                .effective_family_layout(&family)
                .map_err(|e| e.to_string())?
                .height
                - 2.0)
                .abs()
                < 1e-6
        );
        let wait = live.wait_segment(0.2).map_err(|e| e.to_string())?;
        live.advance_segment_to(wait, wait.end_time())
            .map_err(|e| e.to_string())?;
        live.complete_segment(wait).map_err(|e| e.to_string())?;
    }
    Ok(session)
}
