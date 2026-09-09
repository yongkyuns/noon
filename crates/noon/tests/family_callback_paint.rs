use noon::integration::HostCallbackId;
use noon::{Color, FamilyCallbackPaintError, FamilyPaint, RustHostCallbackTable, Scene, Style};
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn unique_local_family_preparation_and_late_read_failure_are_atomic() {
    let scene = Scene::new();
    let a = scene.circle(0.5).unwrap();
    let b = scene.circle(0.5).unwrap();
    let nested = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let family = scene.family(&[(&nested).into(), (&a).into()]).unwrap();
    for _ in 0..2000 {
        scene.circle(0.1).unwrap();
    }
    let before = scene.revision();
    let mut reads = vec![];
    let changes = family
        .prepare_callback_paint(
            before,
            FamilyPaint::Fill {
                color: None,
                opacity: Some(0.4),
            },
            |node| {
                reads.push(node);
                Ok(Style::default())
            },
        )
        .unwrap();
    assert_eq!(reads, [a.node_id(), b.node_id()]);
    assert_eq!(changes.len(), 2);
    let before_style = a.state().unwrap().style;
    assert!(family
        .prepare_callback_paint(before, FamilyPaint::Opacity(0.7), |node| {
            if node == b.node_id() {
                Err(noon::ExecutionSessionCallbackError::UnknownObject(node))
            } else {
                Ok(Style::default())
            }
        })
        .is_err());
    assert_eq!(a.state().unwrap().style, before_style);
    assert_eq!(scene.revision(), before);
    let empty = scene.family(&[]).unwrap();
    assert!(matches!(
        empty.prepare_callback_paint(
            scene.revision(),
            FamilyPaint::Opacity(f64::NAN),
            |_| unreachable!()
        ),
        Err(FamilyCallbackPaintError::InvalidPaint(_))
    ));
    assert!(matches!(
        family.prepare_callback_paint(before, FamilyPaint::Opacity(0.2), |_| unreachable!()),
        Err(FamilyCallbackPaintError::StaleRevision { .. })
    ));
}

#[test]
fn caught_missing_leaf_and_foreign_family_leave_ordered_overlay_intact_then_retry() {
    let mut scene = Scene::new();
    let a = scene.circle(0.5).unwrap();
    let b = scene.circle(0.5).unwrap();
    let missing = scene.circle(0.5).unwrap();
    let nested = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let family = scene.family(&[(&nested).into(), (&a).into()]).unwrap();
    let invalid = scene.family(&[(&a).into(), (&missing).into()]).unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    let foreign_scene = Scene::new();
    let foreign_a = foreign_scene.circle(0.5).unwrap();
    let foreign = foreign_scene.family(&[(&foreign_a).into()]).unwrap();
    let ids = [a.node_id(), b.node_id()];
    let observed = Rc::new(RefCell::new(Vec::new()));
    let seen = observed.clone();
    let mut callbacks = RustHostCallbackTable::new();
    let id = HostCallbackId::new(71);
    callbacks
        .insert(id, move |context| -> Result<(), FamilyCallbackPaintError> {
            let mut prior = context.target_state().style;
            prior.fill = Some(Color::rgba(1.0, 0.0, 0.0, 0.25));
            prior.opacity = 0.7;
            context
                .set_target_style(prior)
                .map_err(FamilyCallbackPaintError::Callback)?;
            assert!(context
                .paint_family(&invalid, FamilyPaint::Opacity(0.1))
                .is_err());
            assert_eq!(context.target_state().style, prior);
            assert!(matches!(
                context.paint_family(&foreign, FamilyPaint::Opacity(0.1)),
                Err(FamilyCallbackPaintError::Authoring(
                    noon::AuthoringError::ForeignStore
                ))
            ));
            context.paint_family(&family, FamilyPaint::Color(Color::rgba(0.0, 0.4, 1.0, 0.9)))?;
            assert_eq!(context.target_state().style.fill.unwrap().alpha, 0.25);
            context.paint_family(
                &family,
                FamilyPaint::Fill {
                    color: None,
                    opacity: Some(0.6),
                },
            )?;
            context.paint_family(
                &family,
                FamilyPaint::Stroke {
                    color: Some(Color::WHITE),
                    width: Some(0.03),
                    opacity: Some(0.2),
                },
            )?;
            for node in ids {
                let state = context
                    .read_object(node)
                    .map_err(FamilyCallbackPaintError::Callback)?;
                assert!((state.style.fill.unwrap().alpha - 0.6).abs() < 1e-6);
                assert!((state.style.stroke.unwrap().alpha - 0.2).abs() < 1e-6);
                seen.borrow_mut().push(node);
            }
            assert_eq!(context.target_state().style.opacity, 0.7);
            Ok(())
        })
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
    let authored = [a.state().unwrap(), b.state().unwrap()];
    let mut session = scene.execution_session().unwrap();
    let publication = session.publication_context();
    callbacks.advance_to(&mut session, 0.0).unwrap();
    assert_eq!(*observed.borrow(), ids);
    assert_eq!(scene.revision(), revision);
    assert_eq!([a.state().unwrap(), b.state().unwrap()], authored);
    assert_eq!(
        session.publication_context().scene_revision(),
        publication.scene_revision()
    );
    assert_eq!(
        session.publication_context().execution_revision(),
        publication.execution_revision()
    );
    assert_ne!(
        session.publication_context().frame_epoch(),
        publication.frame_epoch()
    );
}
