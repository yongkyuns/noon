from pathlib import Path

session = Path("crates/noon/src/execution_session.rs")
text = session.read_text()
old = '''                Err(error) => {
                    let is_flat_family = |family: SemanticNodeId| {
                        store
                            .semantic_family_members_checked(family)
                            .is_ok_and(|members| {
                                members.iter().all(|member| {
                                    store.node(*member).is_some_and(|node| {
                                        matches!(
                                            node.kind(),
                                            noon_core::SemanticNodeKind::AuthoringObject
                                        )
                                    })
                                })
                            })
                    };
                    if !is_flat_family(*source) || !is_flat_family(*target_state) {
                        return Err(ExecutionSessionAnimationError::InvalidComposition(
                            error.to_string(),
                        ));
                    }
                    Ok(declaration.create_family_transform_animation(
                        *source,
                        *target_state,
                        *options,
                    ))
                }
'''
new = '''                Err(error) => {
                    let is_family = |family: SemanticNodeId| {
                        store.semantic_family_members_checked(family).is_ok()
                    };
                    if !is_family(*source) || !is_family(*target_state) {
                        return Err(ExecutionSessionAnimationError::InvalidComposition(
                            error.to_string(),
                        ));
                    }
                    Ok(declaration.create_family_transform_animation(
                        *source,
                        *target_state,
                        *options,
                    ))
                }
'''
if old not in text:
    raise SystemExit("nested family staging guard anchor not found")
session.write_text(text.replace(old, new, 1))

web = Path("crates/noon-web/src/canonical_authoring_scene.rs")
text = web.read_text()
old = '        let invalid_nested = context.scene.family(&[(&left_target).into()]).unwrap();\n'
new = '        let invalid_nested = context.scene.family(&[]).unwrap();\n'
if old not in text:
    raise SystemExit("canonical nested rejection fixture anchor not found")
web.write_text(text.replace(old, new, 1))

tests = Path("crates/noon/src/execution_session/family_transform_tests.rs")
text = tests.read_text()
marker = "fn nested_expansion_session()"
if marker not in text:
    text += r'''

fn nested_expansion_session() -> (ExecutionSession, ExecutionSegment) {
    let mut store = SemanticStore::new();
    let a = object(&mut store, 0.0);
    let b = object(&mut store, 2.0);
    let c = object(&mut store, 4.0);
    let nested = family(&mut store, &[b, c]);
    let source = family(&mut store, &[a, nested]);
    let d = object(&mut store, 10.0);
    let e = object(&mut store, 12.0);
    let f = object(&mut store, 14.0);
    let target = family(&mut store, &[d, e, f]);

    let mut session = ExecutionSession::from_semantic_root(&store, source).unwrap();
    let request = SemanticCompositionRequest::FamilyTransformTo {
        source,
        target_state: target,
        options: AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear),
    };
    let segment = session
        .declare_and_activate_composition(&mut store, source, &request, AnimationOptions::new())
        .unwrap();
    (session, segment)
}

#[test]
fn nested_unequal_family_transform_uses_recursive_identity_free_correspondence() {
    let mut store = SemanticStore::new();
    let a = object(&mut store, 0.0);
    let b = object(&mut store, 2.0);
    let c = object(&mut store, 4.0);
    let nested = family(&mut store, &[b, c]);
    let source = family(&mut store, &[a, nested]);
    let d = object(&mut store, 10.0);
    let e = object(&mut store, 12.0);
    let f = object(&mut store, 14.0);
    let target = family(&mut store, &[d, e, f]);
    let source_members = store
        .semantic_family_members_checked(source)
        .unwrap()
        .to_vec();
    let nested_members = store
        .semantic_family_members_checked(nested)
        .unwrap()
        .to_vec();

    let mut session = ExecutionSession::from_semantic_root(&store, source).unwrap();
    let request = SemanticCompositionRequest::FamilyTransformTo {
        source,
        target_state: target,
        options: AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear),
    };
    let segment = session
        .declare_and_activate_composition(&mut store, source, &request, AnimationOptions::new())
        .unwrap();

    session.advance_segment_to(segment, 0.5).unwrap();
    let transient = session
        .take_renderer_publication()
        .transient_presentations()
        .to_vec();
    assert_eq!(transient.len(), 1);
    assert_eq!(transient[0].anchor_object_index(), 0);
    assert!(transient[0].state().appearance > 0.0);
    assert!(transient[0].state().appearance < 1.0);
    assert_eq!(
        store.semantic_family_members_checked(source).unwrap(),
        source_members
    );
    assert_eq!(
        store.semantic_family_members_checked(nested).unwrap(),
        nested_members
    );

    session.advance_segment_to(segment, 1.0).unwrap();
    session.complete_segment(&mut store, segment).unwrap();
    let endpoint = session.take_renderer_publication();
    assert_eq!(endpoint.transient_presentations().len(), 1);
    assert_eq!(endpoint.transient_presentations()[0].state().appearance, 1.0);
    let retirement = session.take_renderer_publication();
    assert!(retirement.transient_presentations().is_empty());
    assert!(retirement.changes().requires_presentation_redraw());
    assert!(!retirement.changes().is_all());
    assert!(retirement.changes().object_indices().is_empty());
}

#[test]
fn nested_unequal_family_transform_direct_seek_matches_forward_playback() {
    let (mut forward, forward_segment) = nested_expansion_session();
    forward.advance_segment_to(forward_segment, 0.25).unwrap();
    forward.take_renderer_publication();
    forward.advance_segment_to(forward_segment, 0.5).unwrap();
    let forward_frame = forward.frame().clone();
    let forward_transient = forward
        .take_renderer_publication()
        .transient_presentations()
        .to_vec();

    let (mut direct, _direct_segment) = nested_expansion_session();
    direct.seek(0.5).unwrap();
    let direct_frame = direct.frame().clone();
    let direct_transient = direct
        .take_renderer_publication()
        .transient_presentations()
        .to_vec();

    assert_eq!(forward_frame, direct_frame);
    assert_eq!(forward_transient, direct_transient);
    assert_eq!(forward_transient.len(), 1);
    assert_eq!(forward_transient[0].anchor_object_index(), 0);
}
'''
    tests.write_text(text)
