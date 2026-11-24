//! A callout rotates its outline and label around a shared center.
//! Paired with `showcase_mounted_callout.py`.
use crate::{
    AnimationOptions, Color, ContinuationScene, ContinuationStep, LiveContinuation, LiveProgram,
    MobjectFamily, RateFunction, Scene, ScriptError, StyleUpdate, Text, Vec2,
};

const TILT: f64 = std::f64::consts::PI / 12.0;

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
                    self.callout
                        .animate()
                        .shift(Vec2::new(11.25, 0.0))
                        .rotate(TILT)
                } else {
                    self.callout.animate().scale(1.25).rotate(-2.0 * TILT)
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
    scene.shift_family(&callout, -9.0, 0.0)?;
    Ok(scene.into_live_program(MountedCallout { callout, stage: 0 })?)
}

#[cfg(all(test, feature = "prepared-prefix"))]
mod tests {
    use super::*;
    use noon_core::ObjectContentRef;

    #[test]
    fn callout_rewinds_across_its_first_and_mounted_rotation() {
        let mut player = program().unwrap().into_playback().unwrap();
        let mut callbacks = crate::RustHostCallbackTable::new();
        let snapshot = |player: &crate::PlaybackProgram<MountedCallout>| {
            let session = player.session().unwrap();
            let drawn = || {
                session
                    .painter_order()
                    .iter()
                    .map(|&index| &session.frame().objects[index as usize])
            };
            let outline = drawn().find(|row| row.style.fill == Some(Color::from_hex(0x183846)));
            let label = drawn().find(|row| {
                matches!(row.content, ObjectContentRef::Text(_))
                    && row.transform.translation.y > 0.0
            });
            match (outline, label) {
                (Some(outline), Some(label)) => Some([outline.clone(), label.clone()]),
                (None, None) => None,
                _ => panic!("the outline and label must enter together"),
            }
        };
        let mut checkpoints = Vec::new();
        let mut original_ids = None;
        let mut initial_label_scale = None;
        let tilt = TILT as f32;
        let entering_x = 2.25 + 0.625 * tilt.sin();
        let label_offset = Vec2::new(0.0, 0.8125);
        let entered_offset = label_offset.rotate(tilt);
        let final_offset = label_offset.rotate(-tilt) * 1.25;
        let entering_motion = entered_offset - label_offset;
        for (time, expected) in [
            (0.25, None),
            (
                0.75,
                Some((tilt * 0.25, 1.0, label_offset + entering_motion * 0.25)),
            ),
            (
                1.0,
                Some((tilt * 0.5, 1.0, label_offset + entering_motion * 0.5)),
            ),
            (1.75, Some((tilt, 1.0, entered_offset))),
            (
                2.5,
                Some((0.0, 1.125, (entered_offset + final_offset) * 0.5)),
            ),
            (4.0, Some((-tilt, 1.25, final_offset))),
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
            if let Some((angle, scale, offset)) = expected {
                let [outline, label] = rows.as_ref().expect("callout is present");
                if time < 2.0 {
                    let alpha = ((time - 0.5) as f32).min(1.0);
                    let x = -9.0 + (entering_x + 9.0) * alpha;
                    assert!((outline.transform.translation.x - x).abs() < 1e-6);
                }
                assert!((outline.transform.rotation - angle).abs() < 1e-6);
                assert!((label.transform.rotation - angle).abs() < 1e-6);
                assert_eq!(outline.transform.scale, Vec2::new(scale, scale));
                let label_scale = *initial_label_scale.get_or_insert(label.transform.scale.x);
                for axis in [label.transform.scale.x, label.transform.scale.y] {
                    assert!((axis - label_scale * scale).abs() < 1e-6);
                }
                let actual_offset = label.transform.translation - outline.transform.translation;
                assert!((actual_offset.x - offset.x).abs() < 1e-6);
                assert!((actual_offset.y - offset.y).abs() < 1e-6);
                let ids = [outline.id, label.id];
                assert_eq!(ids, *original_ids.get_or_insert(ids));
            } else {
                assert!(rows.is_none());
            }
            checkpoints.push((time, rows));
        }
        for (time, rows) in checkpoints.iter().rev().skip(1) {
            player.seek(*time).unwrap();
            assert_eq!(snapshot(&player), *rows);
        }
        let (time, rows) = &checkpoints[2];
        player.seek(*time).unwrap();
        assert_eq!(snapshot(&player), *rows);
    }
}
