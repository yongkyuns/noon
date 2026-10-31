//! One thousand authored attach/remove cycles: 100 dots, ten repetitions.
//! Ordinary live source; no arbitrary seek or physical-performance claim.
use crate::{
    effects::{EffectUpdate, Pixels},
    ContinuationStep, LiveContinuation, LiveProgram, LiveSession, Mobject,
};

pub struct GlowChurnScene {
    dots: Vec<Mobject>,
    stage: u8,
    last_boundary: f64,
}

impl LiveContinuation for GlowChurnScene {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let duration = match self.stage {
            0 => 1.0 / 60.0,
            1..=20 => {
                for dot in &self.dots {
                    if self.stage % 2 == 1 {
                        live.set_glow(
                            dot,
                            EffectUpdate::default().radius(Pixels(3.0)).intensity(1.0),
                        )
                        .map_err(message)?;
                    } else {
                        live.remove_glow(dot).map_err(message)?;
                    }
                }
                26.0 / 60.0
            }
            21 => 10.0 - self.last_boundary,
            22 => return Ok(ContinuationStep::Finished),
            _ => return Err("glow churn resumed after completion".into()),
        };
        let segment = live.wait_segment(duration).map_err(message)?;
        self.last_boundary = segment.end_time();
        self.stage += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program() -> Result<LiveProgram<GlowChurnScene>, String> {
    let (scene, dots) = super::glow_profile::grid(0)?;
    scene
        .into_live_program(GlowChurnScene {
            dots: dots.into_iter().take(100).collect(),
            stage: 0,
            last_boundary: 0.0,
        })
        .map_err(message)
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};
    use std::collections::HashSet;

    #[test]
    fn source_creates_one_thousand_distinct_attachments_and_restores_the_grid() {
        let mut source = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        let ordinary = source.session().frame().objects.clone();
        let mut attachments = HashSet::new();
        source.resume().unwrap();
        for phase in 0..=21 {
            let time = if phase == 21 {
                10.0
            } else {
                (1.0 + phase as f64 * 26.0) / 60.0
            };
            match source.drive_to(&mut callbacks, time + 1e-8).unwrap() {
                LiveProgramStatus::PublicationPending(expected) => {
                    let context = source.take_renderer_publication().context();
                    assert_eq!(context, expected);
                    source.admit_publication(context).unwrap();
                }
                LiveProgramStatus::ReadyToResume => {}
                other => panic!("unexpected source phase {phase}: {other:?}"),
            }
            let status = source.resume().unwrap();
            let rows = &source.session().frame().objects;
            assert_eq!(rows.len(), ordinary.len());
            let mut active = 0;
            for (row, before) in rows.iter().zip(&ordinary) {
                assert_eq!(row.geometry(), before.geometry());
                assert_eq!(row.transform, before.transform);
                assert_eq!(row.style, before.style);
                if let Some(stack) = &row.effects {
                    active += 1;
                    assert_eq!(stack.len(), 1);
                    attachments.insert(stack.iter().next().unwrap().attachment);
                }
            }
            assert_eq!(active, if phase < 20 && phase % 2 == 0 { 100 } else { 0 });
            assert_eq!(status == LiveProgramStatus::Finished, phase == 21);
        }
        assert_eq!(attachments.len(), 1000);
        assert_eq!(source.session().frame().time, 10.0);
    }
}
