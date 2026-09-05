//! Direct Rust proof for the generic shared authoring path, without serialization.
use noon::{
    AnimationOptions, RateFunction, Scene, SemanticAnimationIntent, SemanticAnimationState,
    SemanticMutationImpact, SemanticMutationTransaction, Vec2,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let mut circle = scene.circle(1.0)?;
    circle.shift(2.0, -1.0)?;
    circle.set_fill(0.0, 1.0, 0.0, 0.5)?;
    assert_eq!(circle.center()?, (2.0, -1.0));
    assert_eq!(circle.width()?, 2.0);
    assert_eq!(circle.fill_opacity()?, 0.5);
    scene.add(&circle)?;

    let mut target = circle.target_editor()?;
    target.shift(4.0, 0.0)?;
    target.manim_scale(2.0, 2.0)?;
    let options = AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear);
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_animation(SemanticAnimationState::new(
        SemanticAnimationIntent::TransformTo {
            target: circle.node_id(),
            target_state: target.node_id(),
        },
        options,
    ));
    let result = transaction.apply(&mut scene.store().borrow_mut())?;
    let [SemanticMutationImpact::AnimationAdded { animation }] = result.impacts() else {
        unreachable!()
    };
    let mut session = scene.execution_session()?;
    session.activate_animation_segment(&scene.store().borrow(), *animation, options)?;
    session.seek(0.5)?;
    assert_eq!(session.frame().objects.len(), 1);
    assert_eq!(
        session.frame().objects[0].transform.translation,
        Vec2::new(4.0, -1.0)
    );
    assert_eq!(
        session.frame().objects[0].transform.scale,
        Vec2::new(1.5, 1.5)
    );
    session.seek(1.0)?;
    assert_eq!(
        session.frame().objects[0].transform.translation,
        Vec2::new(6.0, -1.0)
    );
    assert_eq!(circle.center()?, (2.0, -1.0));
    Ok(())
}
