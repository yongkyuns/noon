//! A diagram built from ordinary shapes, Text, families, and Transform.
//!
//! Pair: `web/python/examples/ordinary_diagram_composition.py`.
//! Relative arrangement places the two labeled nodes. A common family motion
//! carries the connection with them; an explicit second target moves one node
//! and extends its connection in the same animation. No attachment updater or
//! diagram-specific layout object is needed for this scripted motion.

use crate::{
    AnimationOptions, AuthoringError, Color, ExecutionSession, ManimArrow, ManimArrowOptions,
    Mobject, MobjectFamily, RateFunction, Scene, Text,
};

pub const DURATION: f64 = 3.0;

/// Build a finite, seekable execution session, initially positioned at time zero.
pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let (imu, imu_box) = labeled_node(&mut scene, "IMU", Color::BLUE)?;
        let (ekf, ekf_box) = labeled_node(&mut scene, "EKF", Color::ORANGE)?;
        let nodes = scene.family(&[(&imu).into(), (&ekf).into()])?;
        nodes.arrange(1.0, 0.0, 1.2, true)?;

        // Use the shapes' side centers, so the label's bounds cannot choose
        // a corner or move the connection's endpoint.
        let connection = connect(&mut scene, &imu_box, &ekf_box)?;
        let assembly = scene.family(&[(&nodes).into(), connection.family().into()])?;

        // Copies retain the labels and the same nested family topology.
        let shifted = assembly.copy_family()?;
        shifted.root().shift(0.0, 0.75)?;

        let extended_nodes = nodes.copy_family()?;
        extended_nodes.root().shift(0.0, 0.75)?;
        extended_nodes.family(&ekf)?.shift(1.0, 0.0)?;
        let extended_connection = connect(
            &mut scene,
            &extended_nodes.mobject(&imu_box)?,
            &extended_nodes.mobject(&ekf_box)?,
        )?;
        let extended = scene.family(&[
            extended_nodes.root().into(),
            extended_connection.family().into(),
        ])?;

        scene.add_many(&[(&assembly).into()])?;
        let mut execution = scene.execution_session()?;

        // Author ordinary live segments and complete their normal barriers.
        // Both targets describe a coherent assembly, including shaft and tip.
        for target in [shifted.root(), &extended] {
            let mut live = scene.live(&mut execution);
            let hold = live.wait_segment(0.25)?;
            live.advance_segment_to(hold, hold.end_time())?;
            live.complete_segment(hold)?;
            let motion = live.declare_and_activate_family_transform_to(
                &assembly,
                target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )?;
            live.advance_segment_to(motion, motion.end_time())?;
            live.complete_segment(motion)?;
        }
        {
            let mut live = scene.live(&mut execution);
            let hold = live.wait_segment(0.5)?;
            live.advance_segment_to(hold, hold.end_time())?;
            live.complete_segment(hold)?;
        }
        execution.seek(0.0)?;
        Ok(execution)
    };
    build().map_err(|error| error.to_string())
}

fn labeled_node(
    scene: &mut Scene,
    label: &str,
    color: Color,
) -> Result<(MobjectFamily, Mobject), Box<dyn std::error::Error>> {
    let mut shape = scene.rectangle(1.6, 0.8)?;
    let (r, g, b) = (
        f64::from(color.red),
        f64::from(color.green),
        f64::from(color.blue),
    );
    shape.set_fill(r, g, b, 0.2)?;
    shape.set_stroke_color(r, g, b, 1.0)?;
    // Three Manim stroke-width units are 0.03 semantic scene units.
    shape.set_stroke_width(0.03)?;
    let mut text = scene.text(Text::new(label).with_font_size(24.0))?;
    text.manim_move_to_handle(&shape, 0.0, 0.0, 1.0, 1.0)?;
    let family = scene.family(&[(&shape).into(), (&text).into()])?;
    Ok((family, shape))
}

fn connect(
    scene: &mut Scene,
    left: &Mobject,
    right: &Mobject,
) -> Result<ManimArrow, AuthoringError> {
    let start = left.critical_point(1.0, 0.0)?;
    let end = right.critical_point(-1.0, 0.0)?;
    let mut options = ManimArrowOptions::arrow(start.0, start.1, end.0, end.1)?;
    options.set_buff(0.0)?;
    options.set_tip_length(0.2)?;
    options.set_stroke_width(0.03)?;
    options.set_color(1.0, 1.0, 1.0, 1.0)?;
    scene.manim_arrow(options)
}
