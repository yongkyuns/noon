//! Paired with ordinary_z_index.py on native and direct Rust/WASM.
use crate::{ExecutionSession, Scene};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut options = crate::ManimGeometryOptions::square(2.)?;
        options.set_z_index(1.)?;
        let mut a = scene.geometry(options.clone())?;
        a.set_fill(1., 68. / 255., 102. / 255., 1.)?;
        a.set_stroke_width(0.)?;
        a.shift(-0.6, -0.3)?;
        let mut b = scene.geometry(options)?;
        b.set_fill(68. / 255., 136. / 255., 1., 1.)?;
        b.set_stroke_width(0.)?;
        b.shift(0.6, -0.3)?;
        let mut c = scene.circle(1.)?;
        c.set_fill(68. / 255., 1., 136. / 255., 1.)?;
        c.set_stroke_width(0.)?;
        c.shift(0., 0.5)?;
        let nested = scene.family_with_z_index(&[(&a).into(), (&b).into()], 1.)?;
        let family = scene.family_with_z_index(&[(&a).into(), (&nested).into()], -3.)?;
        let copied = family.copy_family()?;
        assert_eq!(copied.root().z_index()?, -3.);
        assert_eq!(copied.mobject(&a)?.z_index()?, 1.);
        scene.add_many(&[(&family).into(), (&c).into()])?;
        let mut session = scene.execution_session()?;
        let mut live = scene.live(&mut session);
        live.set_z_index(&(&c).into(), 2.25, true)?;
        live.set_z_index(&(&a).into(), 3.5, true)?;
        live.set_z_index(&(&family).into(), 0.5, false)?;
        assert_eq!(a.z_index()?, 3.5);
        assert_eq!(b.z_index()?, 1.);
        let wait = live.wait_segment(0.2)?;
        live.advance_segment_to(wait, wait.end_time())?;
        live.complete_segment(wait)?;
        Ok(session)
    };
    build().map_err(|error| error.to_string())
}
