//! A label follows the outline drawn by its own group.
//! Paired with `showcase_mounted_callout.py`.
use crate::{
    AnimationOptions, Color, ContinuationScene, ContinuationStep, LiveContinuation, LiveProgram,
    MobjectFamily, RateFunction, Scene, ScriptError, StyleUpdate, Text, Vec2,
};

pub struct MountedCallout {
    callout: MobjectFamily,
    stage: u8,
}

impl LiveContinuation for MountedCallout {
    const EFFECTS: crate::ContinuationEffects = crate::ContinuationEffects::SceneOnly;
    type Error = ScriptError;

    fn resume(&mut self, scene: &mut ContinuationScene) -> Result<ContinuationStep, ScriptError> {
        let mut live = scene.live();
        let segment = match self.stage {
            0 | 2 => live.wait_segment(0.5)?,
            4 => live.wait_segment(1.0)?,
            1 | 3 => {
                let target = if self.stage == 1 {
                    self.callout.animate().shift(Vec2::new(4.5, 0.0))
                } else {
                    self.callout.animate().scale(1.25)
                };
                crate::animation_request::activate(
                    &mut live,
                    target.into(),
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear),
                )?
            }
            _ => return Ok(ContinuationStep::Finished),
        };
        self.stage += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program() -> Result<LiveProgram<MountedCallout>, ScriptError> {
    let mut scene = Scene::new();
    let ink = Color::from_hex(0x9bf4e2);
    let rail_color = Color::from_hex(0x415874);
    let rail = scene.rectangle(4.5, 0.035)?;
    scene.shift(&rail, Vec2::new(0.0, -0.75))?;
    scene.set_fill(&rail, rail_color, 1.0)?;
    scene.disable_stroke(&rail)?;
    scene.add(&rail)?;
    for (x, name) in [(-2.25, "Sample A"), (2.25, "Sample B")] {
        let marker = scene.circle(0.12)?;
        scene.shift(&marker, Vec2::new(x, -0.75))?;
        scene.set_fill(&marker, ink, 1.0)?;
        scene.disable_stroke(&marker)?;
        let text = scene.text(Text::new(name).with_font_size(27.0).color(ink))?;
        scene.shift(&text, Vec2::new(x, -1.25))?;
        scene.add_many(&[(&marker).into(), (&text).into()])?;
    }
    let label = scene.text(Text::new("Inspect").with_font_size(40.0).color(ink))?;
    scene.shift(&label, Vec2::new(0.0, 0.8125))?;
    let callout = scene.family_with_presentation(&[(&label).into()], 0.0, ink, true)?;
    let outline = noon_core::VectorPath::from_manim_corners_f64(&[
        (-1.5, 0.25),
        (-1.5, 1.25),
        (-1.375, 1.375),
        (1.375, 1.375),
        (1.5, 1.25),
        (1.5, 0.25),
        (0.25, 0.25),
        (0.0, -0.125),
        (-0.25, 0.25),
        (-1.5, 0.25),
    ]);
    scene.set_family_points_3d(&callout, outline.manim_points().expect("authored controls"))?;
    scene.set_family_own_style(
        &callout,
        StyleUpdate {
            fill_color: Some(Color::from_hex(0x183846)),
            fill_opacity: Some(1.0),
            stroke_color: Some(ink),
            stroke_width: Some(0.06),
            stroke_opacity: Some(1.0),
            ..Default::default()
        },
    )?;
    scene.shift_family(&callout, -2.25, 0.0)?;
    scene.add_many(&[(&callout).into()])?;
    Ok(scene.into_live_program(MountedCallout { callout, stage: 0 })?)
}

#[cfg(all(test, feature = "prepared-prefix"))]
mod tests {
    use super::*;
    use noon_core::ObjectContentRef;

    #[test]
    fn mounted_callout_rewinds_the_same_outline_and_label() {
        let mut player = program().unwrap().into_playback().unwrap();
        let mut callbacks = crate::RustHostCallbackTable::new();
        let snapshot = |player: &crate::PlaybackProgram<MountedCallout>| {
            let frame = player.session().unwrap().frame();
            let outline = frame
                .objects
                .iter()
                .find(|row| row.style.fill == Some(Color::from_hex(0x183846)))
                .unwrap();
            let label = frame
                .objects
                .iter()
                .find(|row| {
                    matches!(row.content, ObjectContentRef::Text(_))
                        && row.transform.translation.y > 0.0
                })
                .unwrap();
            [outline.clone(), label.clone()]
        };
        let mut checkpoints = Vec::new();
        let mut original_ids = None;
        let mut label_scale = None;
        for (time, x, scale) in [
            (0.0, -2.25, 1.0),
            (1.0, 0.0, 1.0),
            (1.75, 2.25, 1.0),
            (2.5, 2.25, 1.125),
            (4.0, 2.25, 1.25),
        ] {
            for _ in 0..8 {
                player.prepare_ready(&mut callbacks).unwrap();
                player.advance_to(&mut callbacks, time).unwrap();
                if !player.buffering() {
                    break;
                }
            }
            assert!(!player.buffering());
            assert_eq!(player.session().unwrap().frame().time, time);
            let rows = snapshot(&player);
            for row in &rows {
                assert_eq!(row.transform.translation.x, x);
            }
            assert_eq!(rows[0].transform.scale, Vec2::new(scale, scale));
            let initial_label_scale = *label_scale.get_or_insert(rows[1].transform.scale.x);
            let expected_label_scale = initial_label_scale * scale;
            for axis in [rows[1].transform.scale.x, rows[1].transform.scale.y] {
                assert!((axis - expected_label_scale).abs() < 1e-6);
            }
            let y = |index: usize| rows[index].transform.translation.y;
            assert!((y(1) - y(0) - 0.8125 * scale).abs() < 1e-6);
            let ids = [rows[0].id, rows[1].id];
            assert_eq!(ids, *original_ids.get_or_insert(ids));
            checkpoints.push((time, rows));
        }
        for (time, rows) in checkpoints.into_iter().rev().skip(1) {
            player.seek(time).unwrap();
            assert_eq!(snapshot(&player), rows);
        }
    }
}
