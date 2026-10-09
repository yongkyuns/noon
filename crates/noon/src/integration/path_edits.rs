//! Coherent persistent path edits for hosts that borrow an execution session.
//!
//! Ordinary authoring uses Scene. This typed operation only transfers one
//! transient host request to the existing Rust prepare/publish authority; it
//! owns no store, executor, cache, persistent state, or replacement facade.
use crate::{
    path_editing::{self, PathEdit},
    AuthoringError, ExecutionSession, Mobject, MobjectFamily,
};
use noon_core::{SemanticNodeId, SemanticStore, Vec2};
use std::{cell::RefCell, rc::Rc};

/// One typed path mutation on an existing externally owned execution session.
///
/// All variants use the same coherent effective-state capture, validation,
/// scoped immutable-resource rollback and semantic publication as Scene.
pub enum BorrowedPathEdit<'a> {
    SmoothCorners {
        object: &'a Mobject,
        points: &'a [Vec2],
    },
    Start {
        object: &'a Mobject,
        point: Vec2,
    },
    Quadratic {
        object: &'a Mobject,
        control: Vec2,
        anchor: Vec2,
    },
    Cubic {
        object: &'a Mobject,
        control1: Vec2,
        control2: Vec2,
        anchor: Vec2,
    },
    Subdivide {
        object: &'a Mobject,
        additional: usize,
    },
    Partial {
        object: &'a Mobject,
        source: &'a Mobject,
        a: f64,
        b: f64,
    },
    FamilyJagged {
        family: &'a MobjectFamily,
    },
}

/// Publish one host-originated edit against the supplied borrowed execution.
///
/// This is a low-level integration operation, not the ordinary public
/// authoring surface. Scene remains the durable control owner for normal apps.
pub fn publish_borrowed_path_edit(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    edit: BorrowedPathEdit<'_>,
) -> Result<(), AuthoringError> {
    match edit {
        BorrowedPathEdit::SmoothCorners { object, points } => {
            path_editing::publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::SmoothCorners(points),
            )
        }
        BorrowedPathEdit::Start { object, point } => path_editing::publish_running_object_edit(
            store,
            root,
            execution,
            object,
            PathEdit::Start(point),
        ),
        BorrowedPathEdit::Quadratic {
            object,
            control,
            anchor,
        } => path_editing::publish_running_object_edit(
            store,
            root,
            execution,
            object,
            PathEdit::Quadratic(control, anchor),
        ),
        BorrowedPathEdit::Cubic {
            object,
            control1,
            control2,
            anchor,
        } => path_editing::publish_running_object_edit(
            store,
            root,
            execution,
            object,
            PathEdit::Cubic(control1, control2, anchor),
        ),
        BorrowedPathEdit::Subdivide { object, additional } => {
            path_editing::publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::Subdivide(additional),
            )
        }
        BorrowedPathEdit::Partial {
            object,
            source,
            a,
            b,
        } => path_editing::publish_running_pointwise_partial(
            store, root, execution, object, source, a, b,
        ),
        BorrowedPathEdit::FamilyJagged { family } => {
            path_editing::publish_running_family_anchor_mode(store, root, execution, family, false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MobjectTarget, Scene};

    #[test]
    fn borrowed_edits_use_same_atomic_publication_as_scene() {
        let mut scene = Scene::new();
        let object = scene.square(2.0).unwrap();
        let source = scene.line((0.0, 0.0), (4.0, 0.0)).unwrap();
        scene.add(&object).unwrap();
        scene.add(&source).unwrap();
        let family = scene.family(&[MobjectTarget::Object(&object)]).unwrap();
        let mut execution = scene.execution_session().unwrap();
        let store = Rc::clone(scene.integration_store());
        let root = scene.root();
        let revision = scene.revision();
        let before = object.state().unwrap();
        let resources = store.borrow().geometry_resources().len();

        let invalid = [Vec2::ZERO, Vec2::new(f32::NAN, 0.0)];
        assert!(publish_borrowed_path_edit(
            &store,
            root,
            &mut execution,
            BorrowedPathEdit::SmoothCorners {
                object: &object,
                points: &invalid
            },
        )
        .is_err());
        let foreign = Scene::new().square(2.0).unwrap();
        assert!(matches!(
            publish_borrowed_path_edit(
                &store,
                root,
                &mut execution,
                BorrowedPathEdit::Start {
                    object: &foreign,
                    point: Vec2::ZERO
                },
            ),
            Err(AuthoringError::ForeignStore)
        ));
        assert_eq!(scene.revision(), revision);
        assert_eq!(object.state().unwrap(), before);
        assert_eq!(store.borrow().geometry_resources().len(), resources);

        let points = [
            Vec2::new(-1.0, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 0.0),
        ];
        publish_borrowed_path_edit(
            &store,
            root,
            &mut execution,
            BorrowedPathEdit::SmoothCorners {
                object: &object,
                points: &points,
            },
        )
        .unwrap();
        assert_eq!(object.path_query().unwrap().curve_count(), 2);
        publish_borrowed_path_edit(
            &store,
            root,
            &mut execution,
            BorrowedPathEdit::FamilyJagged { family: &family },
        )
        .unwrap();
        publish_borrowed_path_edit(
            &store,
            root,
            &mut execution,
            BorrowedPathEdit::Start {
                object: &object,
                point: Vec2::new(-2.0, -1.0),
            },
        )
        .unwrap();
        publish_borrowed_path_edit(
            &store,
            root,
            &mut execution,
            BorrowedPathEdit::Quadratic {
                object: &object,
                control: Vec2::new(-1.0, 1.0),
                anchor: Vec2::new(0.0, 0.0),
            },
        )
        .unwrap();
        publish_borrowed_path_edit(
            &store,
            root,
            &mut execution,
            BorrowedPathEdit::Cubic {
                object: &object,
                control1: Vec2::new(1.0, 2.0),
                control2: Vec2::new(2.0, 2.0),
                anchor: Vec2::new(3.0, 0.0),
            },
        )
        .unwrap();
        publish_borrowed_path_edit(
            &store,
            root,
            &mut execution,
            BorrowedPathEdit::Subdivide {
                object: &object,
                additional: 1,
            },
        )
        .unwrap();
        publish_borrowed_path_edit(
            &store,
            root,
            &mut execution,
            BorrowedPathEdit::Partial {
                object: &object,
                source: &source,
                a: 0.25,
                b: 0.75,
            },
        )
        .unwrap();
        assert_eq!(object.path_query().unwrap().start().unwrap(), (1.0, 0.0));
        assert_eq!(object.path_query().unwrap().end().unwrap(), (3.0, 0.0));
        assert!(scene.revision().get() > revision.get());
    }
}
