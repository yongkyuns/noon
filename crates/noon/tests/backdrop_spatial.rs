//! Typed native publication of the execution-owned prefix geometry query.
//! This does not qualify backdrop pixels or promote the unfinished effect API.
use noon::{
    effects::{BackdropBlur, EffectUpdate, Pixels},
    Rect, Scene, Vec2,
};
use noon_core::{ObjectId, SemanticMutationTransaction};
use noon_runtime::{PainterPrefixGeometryQueryError as Error, SpatialIndexUpdateStats};
fn window() -> Rect {
    Rect::new(Vec2::new(-2.0, -2.0), Vec2::new(2.0, 2.0))
}

#[test]
fn backdrop_queries_borrow_the_synchronized_session_index_and_follow_local_edits() {
    let mut scene = Scene::new();
    let earlier = scene.circle(0.3).unwrap();
    let panel = scene.square(1.0).unwrap();
    let later = scene.circle(0.3).unwrap();
    let mut far = scene.circle(0.3).unwrap();
    far.shift(40.0, 0.0).unwrap();
    scene
        .add_many(&[
            (&earlier).into(),
            (&panel).into(),
            (&later).into(),
            (&far).into(),
        ])
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let anchor = session.execution_object_id(panel.node_id()).unwrap();
    let earlier_id = session.execution_object_id(earlier.node_id()).unwrap();
    let later_id = session.execution_object_id(later.node_id()).unwrap();
    assert_eq!(
        session
            .take_renderer_publication()
            .query_painter_prefix_geometry(window(), anchor),
        Err(Error::MissingSpatialIndex)
    );
    scene
        .live(&mut session)
        .add_effect(
            &panel,
            BackdropBlur::new(EffectUpdate::default().radius(Pixels(3.0))).unwrap(),
            "glass",
        )
        .unwrap();
    {
        let publication = session.take_renderer_publication();
        let query = publication
            .query_painter_prefix_geometry(window(), anchor)
            .unwrap();
        assert_eq!(query.objects, [earlier_id]);
        assert_eq!(query.stats.results, 1);
        assert_eq!(query.stats.full_scan_fallbacks, 0);
        assert_eq!(
            publication.query_painter_prefix_geometry(window(), ObjectId::new(u64::MAX)),
            Err(Error::InvalidAnchor(ObjectId::new(u64::MAX)))
        );
        assert_eq!(
            publication.query_painter_prefix_geometry(
                Rect::new(Vec2::new(f32::NAN, 0.0), Vec2::new(2.0, 2.0)),
                anchor
            ),
            Err(Error::InvalidBounds)
        );
    }
    let mut tx = SemanticMutationTransaction::new();
    tx.set_z_index(later.node_id(), -1.0);
    scene.live(&mut session).apply(tx).unwrap();
    assert_eq!(
        session
            .take_renderer_publication()
            .query_painter_prefix_geometry(window(), anchor)
            .unwrap()
            .objects,
        [later_id, earlier_id]
    );
    scene.live(&mut session).shift(&earlier, 20.0, 0.0).unwrap();
    assert_eq!(
        session
            .take_renderer_publication()
            .query_painter_prefix_geometry(window(), anchor)
            .unwrap()
            .objects,
        [later_id]
    );
    assert_eq!(session.last_spatial_update_stats().full_rebuilds, 0);
    assert_eq!(session.last_spatial_update_stats().leaves_upserted, 1);
    session.take_renderer_publication();
    assert_eq!(
        session.last_spatial_update_stats(),
        SpatialIndexUpdateStats::default()
    );
    scene
        .live(&mut session)
        .remove_effect(&panel, "glass")
        .unwrap();
    assert!(session
        .take_renderer_publication()
        .spatial_index()
        .is_none());
}

#[test]
fn composed_backdrop_queries_require_its_first_physical_member_anchor() {
    let mut scene = Scene::new();
    let earlier = scene.circle(0.2).unwrap();
    let a = scene.circle(0.3).unwrap();
    let b = scene.circle(0.3).unwrap();
    let group = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    scene
        .add_effect(&group, BackdropBlur::default(), "glass")
        .unwrap();
    scene
        .add_many(&[(&earlier).into(), (&group).into()])
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let owner = session.execution_object_id(group.node_id()).unwrap();
    let anchor = session.execution_object_id(a.node_id()).unwrap();
    let earlier = session.execution_object_id(earlier.node_id()).unwrap();
    let publication = session.take_renderer_publication();
    assert_eq!(
        publication.query_painter_prefix_geometry(window(), owner),
        Err(Error::InvalidAnchor(owner))
    );
    assert_eq!(
        publication
            .query_painter_prefix_geometry(window(), anchor)
            .unwrap()
            .objects,
        [earlier]
    );
}

#[test]
fn native_live_backdrop_capability_failure_rolls_back_semantic_and_execution_publication() {
    let mut scene = Scene::new();
    let source = scene.circle(0.3).unwrap();
    let panel = scene.square(1.0).unwrap();
    scene
        .add_many(&[(&source).into(), (&panel).into()])
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    scene
        .live(&mut session)
        .add_effect(
            &panel,
            BackdropBlur::new(EffectUpdate::default().radius(Pixels(2.0))).unwrap(),
            "glass",
        )
        .unwrap();
    let store = scene.integration_store();
    let before_frame = session.frame().clone();
    let before_context = session.publication_context();
    let before_transform = store
        .borrow()
        .semantic_object_state_checked(source.node_id())
        .unwrap()
        .transform;
    let before_revision = store.borrow().scene_revision();
    let mut bad = SemanticMutationTransaction::new();
    bad.set_object_transform(
        source.node_id(),
        noon_core::SemanticWorldTransform3D::IDENTITY.into(),
    );
    let error = scene.live(&mut session).apply(bad).unwrap_err();
    assert!(format!("{error:?}").contains("UnsupportedBackdropInput"));
    assert_eq!(store.borrow().scene_revision(), before_revision);
    assert_eq!(
        store
            .borrow()
            .semantic_object_state_checked(source.node_id())
            .unwrap()
            .transform,
        before_transform
    );
    assert_eq!(session.frame(), &before_frame);
    assert_eq!(session.publication_context(), before_context);
    // The same spatial role is valid after the target, when both edits publish
    // together through the ordinary shared semantic transaction.
    let mut good = SemanticMutationTransaction::new();
    good.set_object_transform(
        source.node_id(),
        noon_core::SemanticWorldTransform3D::IDENTITY.into(),
    );
    good.set_z_index(source.node_id(), 2.0);
    scene.live(&mut session).apply(good).unwrap();
    let before_frame = session.frame().clone();
    let before_context = session.publication_context();
    let before_revision = store.borrow().scene_revision();
    let mut bad = SemanticMutationTransaction::new();
    bad.set_z_index(source.node_id(), -1.0);
    assert!(scene.live(&mut session).apply(bad).is_err());
    assert_eq!(session.frame(), &before_frame);
    assert_eq!(session.publication_context(), before_context);
    assert_eq!(store.borrow().scene_revision(), before_revision);
}
