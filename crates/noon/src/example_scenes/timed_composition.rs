//! Nested timed membership shared by native Rust, direct WASM and Python examples.

use std::rc::Rc;

use crate::{
    AnimationCompositionRequest as Request, AnimationOptions, Color, ContinuationStep,
    LiveContinuation, LiveProgram, LiveSession, Mobject, RateFunction, Scene,
    SemanticAnimationCompositionKind as Kind,
};

pub struct TimedComposition {
    squares: [Mobject; 3],
    stage: u8,
}

impl LiveContinuation for TimedComposition {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {
                let add = AnimationOptions::new().run_time(0.2);
                let nested = vec![
                    Request::Wait { duration: 0.2 },
                    Request::Add {
                        target: &self.squares[1],
                        options: add,
                    },
                    Request::Wait { duration: 0.2 },
                ];
                let children = vec![
                    Request::Add {
                        target: &self.squares[0],
                        options: add,
                    },
                    Request::Composition {
                        kind: Kind::Sequence,
                        children: nested,
                        options: AnimationOptions::new().rate_func(RateFunction::Linear),
                    },
                    Request::Add {
                        target: &self.squares[2],
                        options: add,
                    },
                ];
                let request = Request::Composition {
                    kind: Kind::Sequence,
                    children,
                    options: AnimationOptions::new().rate_func(RateFunction::Smooth),
                };
                let segment = live
                    .declare_and_activate_composition(&request, AnimationOptions::new())
                    .map_err(|error| error.to_string())?;
                self.stage = 1;
                Ok(ContinuationStep::Await(segment))
            }
            1 => {
                for square in &self.squares {
                    if !live.contains(square).map_err(|error| error.to_string())? {
                        return Err("timed Add did not preserve its semantic target".into());
                    }
                }
                self.stage = 2;
                live.wait_segment(0.25)
                    .map(ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            2 => {
                self.stage = 3;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("timed composition resumed after completion".into()),
        }
    }
}

pub fn program() -> Result<LiveProgram<TimedComposition>, String> {
    let scene = Scene::new();
    let mut squares = [
        Mobject::manim_square(Rc::clone(scene.store()), 1.0)?,
        Mobject::manim_square(Rc::clone(scene.store()), 1.0)?,
        Mobject::manim_square(Rc::clone(scene.store()), 1.0)?,
    ];
    for (square, x) in squares.iter_mut().zip([-2.0, 0.0, 2.0]) {
        square.set_translation(x, 0.0)?;
        square.set_fill(
            f64::from(Color::BLUE.red),
            f64::from(Color::BLUE.green),
            f64::from(Color::BLUE.blue),
            0.7,
        )?;
    }
    scene
        .into_live_program(TimedComposition { squares, stage: 0 })
        .map_err(|error| error.to_string())
}
