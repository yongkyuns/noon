use super::*;
#[cfg(feature = "native-text")]
use crate::LayoutAnchor;
use crate::{ImageMobjectOptions, Scene};

#[test]
fn boundary_profile_uses_only_the_addressed_content_and_does_not_materialize_resources() {
    let mut scene = Scene::new();
    let a = scene.circle(0.2).unwrap();
    let b = scene.square(0.3).unwrap();
    let family = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let image = scene
        .image(ImageMobjectOptions::rgba8(1, 1, vec![255; 4]).unwrap())
        .unwrap();
    let mixed = scene.family(&[(&family).into(), (&image).into()]).unwrap();
    let before = scene.revision();
    let resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();
    assert!(!MobjectTarget::from(&family)
        .requires_content_boundary()
        .unwrap());
    assert!(MobjectTarget::from(&image)
        .requires_content_boundary()
        .unwrap());
    assert!(MobjectTarget::from(&mixed)
        .requires_content_boundary()
        .unwrap());
    let mut read = Vec::new();
    assert!(!requires_content_boundary(
        &scene.integration_store().borrow(),
        family.node_id(),
        |id, _| {
            read.push(id);
            Ok(false)
        }
    )
    .unwrap());
    assert_eq!(read, [a.node_id(), b.node_id()]);
    assert_eq!(scene.revision(), before);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len(),
        resources
    );
}

#[test]
fn effective_boundary_profile_tracks_content_replacement_and_keeps_queries_read_only() {
    let mut scene = Scene::new();
    let target = scene.circle(0.4).unwrap();
    let geometry = scene.square(0.3).unwrap();
    let image = scene
        .image(ImageMobjectOptions::rgba8(1, 1, vec![255; 4]).unwrap())
        .unwrap();
    let family = scene.family(&[(&target).into()]).unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    let mut execution = scene.execution_session().unwrap();
    assert!(!scene
        .live(&mut execution)
        .requires_content_boundary((&family).into())
        .unwrap());
    // Replacing content preserves the target's style. Image admission must
    // reject a stroked geometry style before changing either authority.
    let publication = execution.publication_context();
    let revision = scene.revision();
    let state = target.state().unwrap();
    assert!(scene
        .live(&mut execution)
        .replace_content(&target, &image)
        .is_err());
    assert_eq!(execution.publication_context(), publication);
    assert_eq!(scene.revision(), revision);
    assert_eq!(target.state().unwrap(), state);
    assert!(!scene
        .live(&mut execution)
        .requires_content_boundary((&family).into())
        .unwrap());
    {
        let mut live = scene.live(&mut execution);
        live.set_style(
            &target,
            crate::StyleUpdate {
                fill_color: Some(crate::Color::WHITE),
                fill_opacity: Some(1.),
                stroke_width: Some(0.),
                ..Default::default()
            },
        )
        .unwrap();
        live.disable_stroke(&target).unwrap();
    }
    for source in [&image, &geometry, &image] {
        let mut live = scene.live(&mut execution);
        live.replace_content(&target, source).unwrap();
        let expected = MobjectTarget::from(source)
            .requires_content_boundary()
            .unwrap();
        assert_eq!(
            live.requires_content_boundary((&family).into()).unwrap(),
            expected
        );
        assert_eq!(
            live.requires_content_boundary(source.into()).unwrap(),
            expected
        );
    }
    let before = execution.publication_context();
    let foreign = Scene::new().circle(0.5).unwrap();
    assert!(scene
        .live(&mut execution)
        .requires_content_boundary((&foreign).into())
        .is_err());
    assert_eq!(execution.publication_context(), before);
    let _out_of_band = scene.circle(1.).unwrap();
    assert!(scene
        .live(&mut execution)
        .requires_content_boundary((&family).into())
        .is_err());
    assert_eq!(execution.publication_context(), before);
}

#[cfg(feature = "native-text")]
#[test]
fn generic_table_entries_use_retained_text_layout_before_and_after_live_placement() {
    let mut scene = Scene::new();
    let table = crate::Table::from_rows(&mut scene, [["a", "b"], ["c", "d"]]).unwrap();
    let entry = table.get_entry(0, 0).unwrap();
    let target = entry.as_target();
    let anchor = match target {
        MobjectTarget::Object(object) => LayoutAnchor::from(object),
        MobjectTarget::Family(family) => LayoutAnchor::from(family),
    };
    assert!(target.requires_content_boundary().unwrap());
    assert_eq!(target.get_num_points().unwrap(), 0);
    let authored = anchor.critical_point_3d(SemanticVec3::ZERO).unwrap();
    scene.add_many(&[table.family().into()]).unwrap();
    let mut execution = scene.execution_session().unwrap();
    let before = execution.publication_context();
    let mut live = scene.live(&mut execution);
    assert!(live.requires_content_boundary(target).unwrap());
    assert_eq!(
        live.layout_critical_point_3d(&anchor, SemanticVec3::ZERO)
            .unwrap(),
        authored
    );
    live.shift_family(table.family(), 1.5, -0.5).unwrap();
    assert!(live.requires_content_boundary(target).unwrap());
    let after = live
        .layout_critical_point_3d(&anchor, SemanticVec3::ZERO)
        .unwrap();
    assert!((after.x - authored.x - 1.5).abs() < 1e-6);
    assert!((after.y - authored.y + 0.5).abs() < 1e-6);
    assert_ne!(execution.publication_context(), before);
}

#[cfg(feature = "native-text")]
#[test]
fn boundary_profile_tracks_text_content_without_host_type_or_identity_changes() {
    let mut scene = Scene::new();
    let target = scene.square(0.3).unwrap();
    let text = scene.text("a").unwrap();
    scene.add(&target).unwrap();
    let id = target.node_id();
    let mut execution = scene.execution_session().unwrap();
    let mut live = scene.live(&mut execution);
    assert!(!live.requires_content_boundary((&target).into()).unwrap());
    live.replace_content(&target, &text).unwrap();
    assert!(live.requires_content_boundary((&target).into()).unwrap());
    assert_eq!(target.node_id(), id);
    assert_eq!(target.get_num_points().unwrap(), 0);
}
