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
    Clear {
        object: &'a Mobject,
    },
    SmoothCorners {
        object: &'a Mobject,
        points: &'a [Vec2],
    },
    AnchorMode {
        object: &'a Mobject,
        smooth: bool,
    },
    Corners {
        object: &'a Mobject,
        points: &'a [Vec2],
    },
    Line {
        object: &'a Mobject,
        point: Vec2,
    },
    Close {
        object: &'a Mobject,
    },
    Reverse {
        object: &'a Mobject,
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
    SmoothCurve {
        object: &'a Mobject,
        handle2: Option<Vec2>,
        anchor: Vec2,
    },
    CubicCurve {
        object: &'a Mobject,
        anchor1: Vec2,
        handle1: Vec2,
        handle2: Vec2,
        anchor2: Vec2,
    },
    CubicCurves {
        object: &'a Mobject,
        curves: &'a [[Vec2; 4]],
    },
    SetCubicCurves {
        object: &'a Mobject,
        curves: &'a [[Vec2; 4]],
    },
    AppendVectorized {
        object: &'a Mobject,
        source: &'a Mobject,
    },
    AppendCorners {
        object: &'a Mobject,
        points: &'a [Vec2],
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
    FamilyAnchorMode {
        family: &'a MobjectFamily,
        smooth: bool,
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
    // Ordinary in-place object edits share one capture/prepare/publish path.
    // Cross-object partial selection and family edits have their own coherent
    // capture requirements, but still publish through the same Rust authority.
    let (object, operation) = match edit {
        BorrowedPathEdit::Clear { object } => (object, PathEdit::Clear),
        BorrowedPathEdit::SmoothCorners { object, points } => {
            (object, PathEdit::SmoothCorners(points))
        }
        BorrowedPathEdit::AnchorMode { object, smooth } => (object, PathEdit::AnchorMode(smooth)),
        BorrowedPathEdit::Corners { object, points } => (object, PathEdit::Corners(points)),
        BorrowedPathEdit::Line { object, point } => (object, PathEdit::Line(point)),
        BorrowedPathEdit::Close { object } => (object, PathEdit::Close),
        BorrowedPathEdit::Reverse { object } => (object, PathEdit::Reverse),
        BorrowedPathEdit::Start { object, point } => (object, PathEdit::Start(point)),
        BorrowedPathEdit::Quadratic {
            object,
            control,
            anchor,
        } => (object, PathEdit::Quadratic(control, anchor)),
        BorrowedPathEdit::Cubic {
            object,
            control1,
            control2,
            anchor,
        } => (object, PathEdit::Cubic(control1, control2, anchor)),
        BorrowedPathEdit::SmoothCurve {
            object,
            handle2,
            anchor,
        } => (object, PathEdit::SmoothCurve { handle2, anchor }),
        BorrowedPathEdit::CubicCurve {
            object,
            anchor1,
            handle1,
            handle2,
            anchor2,
        } => (
            object,
            PathEdit::CubicCurve(anchor1, handle1, handle2, anchor2),
        ),
        BorrowedPathEdit::CubicCurves { object, curves } => (object, PathEdit::CubicCurves(curves)),
        BorrowedPathEdit::SetCubicCurves { object, curves } => {
            (object, PathEdit::SetCubicCurves(curves))
        }
        BorrowedPathEdit::AppendCorners { object, points } => {
            (object, PathEdit::AppendCorners(points))
        }
        BorrowedPathEdit::AppendVectorized { object, source } => {
            return path_editing::publish_running_append_vectorized_mobject(
                store, root, execution, object, source,
            );
        }
        BorrowedPathEdit::Subdivide { object, additional } => {
            (object, PathEdit::Subdivide(additional))
        }
        BorrowedPathEdit::Partial {
            object,
            source,
            a,
            b,
        } => {
            return path_editing::publish_running_pointwise_partial(
                store, root, execution, object, source, a, b,
            );
        }
        BorrowedPathEdit::FamilyAnchorMode { family, smooth } => {
            return path_editing::publish_running_family_anchor_mode(
                store, root, execution, family, smooth,
            );
        }
    };
    path_editing::publish_running_object_edit(store, root, execution, object, operation)
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
        assert!(matches!(
            publish_borrowed_path_edit(
                &store,
                root,
                &mut execution,
                BorrowedPathEdit::AnchorMode {
                    object: &foreign,
                    smooth: true,
                },
            ),
            Err(AuthoringError::ForeignStore)
        ));
        let mut other_scene = Scene::new();
        let other = other_scene.square(1.0).unwrap();
        let foreign_family = other_scene
            .family(&[MobjectTarget::Object(&other)])
            .unwrap();
        assert!(matches!(
            publish_borrowed_path_edit(
                &store,
                root,
                &mut execution,
                BorrowedPathEdit::FamilyAnchorMode {
                    family: &foreign_family,
                    smooth: true,
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
            BorrowedPathEdit::Corners {
                object: &object,
                points: &points,
            },
        )
        .unwrap();
        assert_eq!(object.path_query().unwrap().curve_count(), 2);
        for smooth in [true, false] {
            publish_borrowed_path_edit(
                &store,
                root,
                &mut execution,
                BorrowedPathEdit::AnchorMode {
                    object: &object,
                    smooth,
                },
            )
            .unwrap();
            publish_borrowed_path_edit(
                &store,
                root,
                &mut execution,
                BorrowedPathEdit::FamilyAnchorMode {
                    family: &family,
                    smooth,
                },
            )
            .unwrap();
        }
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
            BorrowedPathEdit::Line {
                object: &object,
                point: Vec2::new(-1.0, -1.0),
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
            BorrowedPathEdit::Close { object: &object },
        )
        .unwrap();
        publish_borrowed_path_edit(
            &store,
            root,
            &mut execution,
            BorrowedPathEdit::Reverse { object: &object },
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
