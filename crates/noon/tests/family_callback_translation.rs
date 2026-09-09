use noon::integration::HostCallbackId;
use noon::{
    ExecutionSessionCallbackError, FamilyCallbackPaintError, FamilyCallbackTranslationError, Rect,
    RustHostCallbackTable, Scene, Transform2D, Vec2,
};
use std::{cell::RefCell, error::Error, rc::Rc};

#[test]
fn selection_is_unique_local_and_all_inputs_are_prepared_before_returning_changes() {
    let scene = Scene::new();
    let a = scene.circle(0.5).unwrap();
    let b = scene.circle(0.5).unwrap();
    let nested = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let family = scene.family(&[(&a).into(), (&nested).into()]).unwrap();
    let empty = scene.family(&[]).unwrap();
    for _ in 0..2000 {
        scene.circle(0.1).unwrap();
    }
    let before = scene.revision();
    let authored = [a.state().unwrap(), b.state().unwrap()];
    let bounds = Some(Rect::new(Vec2::new(-1.0, -1.0), Vec2::new(1.0, 1.0)));
    let mut reads = vec![];
    let changes = family
        .prepare_callback_translation(before, 1.0, -2.0, |node| {
            reads.push(node);
            Ok((Transform2D::default(), bounds))
        })
        .unwrap();
    assert_eq!(reads, [a.node_id(), b.node_id()]);
    assert_eq!(changes.iter().map(|c| c.node).collect::<Vec<_>>(), reads);
    for change in &changes {
        assert_eq!(change.transform.translation, Vec2::new(1.0, -2.0));
        assert_eq!(change.bounds.unwrap().min, Vec2::new(0.0, -3.0));
    }
    for (x, y) in [
        (f64::NAN, 0.0),
        (0.0, f64::INFINITY),
        (f64::NEG_INFINITY, 0.0),
        (f64::MAX, 0.0),
    ] {
        for selection in [&family, &empty] {
            let error = selection
                .prepare_callback_translation(before, x, y, |_| {
                    unreachable!("invalid delta must not read")
                })
                .unwrap_err();
            assert!(matches!(
                error,
                FamilyCallbackTranslationError::Translation(_)
            ));
            assert!(error
                .source()
                .unwrap()
                .is::<noon_core::SemanticLoweringError>());
        }
    }
    let late = family
        .prepare_callback_translation(before, 1.0, 0.0, |node| {
            if node == b.node_id() {
                Err(ExecutionSessionCallbackError::UnknownObject(node))
            } else {
                Ok((Transform2D::default(), bounds))
            }
        })
        .unwrap_err();
    assert!(
        matches!(late, FamilyCallbackTranslationError::Family(FamilyCallbackPaintError::Callback(
        ExecutionSessionCallbackError::UnknownObject(node))) if node==b.node_id())
    );
    // A finite requested delta can overflow only at the last effective row.
    assert!(matches!(
        family.prepare_callback_translation(before, f64::from(f32::MAX), 0.0, |node| {
            let mut transform = Transform2D::default();
            if node == b.node_id() {
                transform.translation.x = f32::MAX;
            }
            Ok((transform, None))
        }),
        Err(FamilyCallbackTranslationError::Translation(_))
    ));
    assert!(family
        .prepare_callback_translation(before, 0.0, 0.0, |_| Ok((Transform2D::default(), bounds)))
        .unwrap()
        .is_empty());
    assert_eq!(scene.revision(), before);
    assert_eq!([a.state().unwrap(), b.state().unwrap()], authored);
    scene.circle(0.1).unwrap();
    assert!(matches!(
        family.prepare_callback_translation(before, 1.0, 0.0, |_| unreachable!()),
        Err(FamilyCallbackTranslationError::Family(
            FamilyCallbackPaintError::StaleRevision { .. }
        ))
    ));
}

#[test]
fn native_callback_preserves_prior_overlay_and_late_failures_then_retries_once_per_leaf() {
    let mut scene = Scene::new();
    let a = scene.circle(0.5).unwrap();
    let b = scene.circle(0.5).unwrap();
    let missing = scene.circle(0.5).unwrap();
    let untouched = scene.circle(0.5).unwrap();
    let nested = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let family = scene.family(&[(&a).into(), (&nested).into()]).unwrap();
    let invalid = scene.family(&[(&a).into(), (&missing).into()]).unwrap();
    let foreign_scene = Scene::new();
    let foreign = foreign_scene.family(&[]).unwrap();
    scene
        .add_many(&[(&a).into(), (&b).into(), (&untouched).into()])
        .unwrap();
    let observed = Rc::new(RefCell::new(Vec::new()));
    let calls = observed.clone();
    let mut callbacks = RustHostCallbackTable::new();
    let id = HostCallbackId::new(73);
    callbacks
        .insert(
            id,
            move |context| -> Result<(), FamilyCallbackTranslationError> {
                let mut prior = context.target_state().transform;
                prior.translation = Vec2::new(3.0, 2.0);
                context
                    .set_target_transform(prior)
                    .map_err(FamilyCallbackPaintError::Callback)?;
                let prior = *context.target_state();
                assert!(matches!(
                    context.shift_family(&invalid, 1.0, 0.0),
                    Err(FamilyCallbackTranslationError::Family(
                        FamilyCallbackPaintError::Callback(_)
                    ))
                ));
                assert_eq!(*context.target_state(), prior);
                assert!(matches!(
                    context.shift_family(&foreign, 1.0, 0.0),
                    Err(FamilyCallbackTranslationError::Family(
                        FamilyCallbackPaintError::Authoring(noon::AuthoringError::ForeignStore)
                    ))
                ));
                assert!(context.shift_family(&family, f64::NAN, 0.0).is_err());
                assert_eq!(*context.target_state(), prior);
                context.shift_family(&family, 1.0, 0.0)?;
                assert_eq!(
                    context.target_state().transform.translation,
                    Vec2::new(4.0, 2.0)
                );
                let second = context
                    .read_object(b.node_id())
                    .map_err(FamilyCallbackPaintError::Callback)?;
                assert_eq!(second.transform.translation, Vec2::new(1.0, 0.0));
                // A late numeric error also leaves the preceding successful operation intact.
                let mut too_large = second.transform;
                too_large.translation.x = f32::MAX;
                context
                    .set_transform(b.node_id(), too_large)
                    .map_err(FamilyCallbackPaintError::Callback)?;
                let first = *context.target_state();
                assert!(context
                    .shift_family(&family, f64::from(f32::MAX), 0.0)
                    .is_err());
                assert_eq!(*context.target_state(), first);
                assert_eq!(
                    context.read_object(b.node_id()).unwrap().transform,
                    too_large
                );
                context
                    .set_transform(b.node_id(), second.transform)
                    .map_err(FamilyCallbackPaintError::Callback)?;
                context.shift_family(&family, -1.0, 0.0)?;
                assert_eq!(context.target_state().transform, prior.transform);
                assert_eq!(
                    context
                        .read_object(b.node_id())
                        .unwrap()
                        .transform
                        .translation,
                    Vec2::ZERO
                );
                calls.borrow_mut().push(context.time());
                Ok(())
            },
        )
        .unwrap();
    callbacks
        .add_updater(
            &mut scene.integration_store().borrow_mut(),
            a.node_id(),
            id,
            0.0,
            None,
        )
        .unwrap();
    let revision = scene.revision();
    let authored = a.state().unwrap();
    let mut session = scene.execution_session().unwrap();
    let untouched_before = session.frame().objects[2].clone();
    let geometry = session
        .frame()
        .objects
        .iter()
        .map(|o| o.geometry().cloned())
        .collect::<Vec<_>>();
    callbacks.advance_to(&mut session, 0.0).unwrap();
    assert_eq!(*observed.borrow(), [0.0]);
    assert_eq!(
        session.frame().objects[0].transform.translation,
        Vec2::new(3.0, 2.0)
    );
    assert_eq!(session.frame().objects[1].transform.translation, Vec2::ZERO);
    assert_eq!(session.frame().objects[2], untouched_before);
    assert_eq!(
        session
            .frame()
            .objects
            .iter()
            .map(|o| o.geometry().cloned())
            .collect::<Vec<_>>(),
        geometry
    );
    assert_eq!(scene.revision(), revision);
    assert_eq!(a.state().unwrap(), authored);
}
