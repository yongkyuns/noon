//! Typed native translation-drag ingress without frontend object selection.
//!
//! Run with `cargo run -p noon-native --example native_translation_drag`.
//! A real platform adapter performs the same token refresh and forwards each
//! normalized occurrence from its pointer collector.

use noon::{
    integration::{
        NativeInputModifiers, NativePointerId, NativePointerInput, NativePointerInputKind,
        NativePointerPosition,
    },
    ContinuationStep, LiveContinuation, LiveSession, Scene, SemanticVec3, Vec2,
};

const POINTER: NativePointerId = NativePointerId {
    source: 1,
    pointer: 0,
};

struct Finish;

impl LiveContinuation for Finish {
    type Error = std::convert::Infallible;

    fn resume(&mut self, _: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        Ok(ContinuationStep::Finished)
    }
}

fn position(x: f32) -> NativePointerPosition {
    NativePointerPosition::new(Vec2::new(x, 0.0), Vec2::new(x * 100.0, 100.0))
        .expect("example positions are finite")
}

fn input(
    token: &noon::integration::NativePointerInputToken,
    sequence: u64,
    kind: NativePointerInputKind,
) -> NativePointerInput {
    NativePointerInput::new(
        sequence,
        token.pointer(),
        token.context(),
        NativeInputModifiers::default(),
        kind,
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let circle = scene.circle(0.5)?;
    scene.add(&circle)?;

    let mut program = scene.into_live_program(Finish)?;
    program.set_translation_drag_targets([&circle])?;
    program.resume()?;

    let token = program.configure_native_pointer_input(POINTER, 0)?;
    program.submit_translation_drag_input(
        &token,
        input(
            &token,
            1,
            NativePointerInputKind::Press {
                position: position(0.0),
                button: 0,
            },
        ),
    )?;

    let token = program.native_pointer_input_token()?;
    program.submit_translation_drag_input(
        &token,
        input(&token, 2, NativePointerInputKind::Move(position(2.0))),
    )?;

    let token = program.native_pointer_input_token()?;
    let release = program.submit_translation_drag_input(
        &token,
        input(
            &token,
            3,
            NativePointerInputKind::Release {
                position: position(2.0),
                button: 0,
            },
        ),
    )?;
    assert!(release.undo.is_some());
    assert_eq!(
        circle.state()?.transform.translation,
        SemanticVec3::new(2.0, 0.0, 0.0)
    );
    println!("translation drag committed at x = 2");
    Ok(())
}
