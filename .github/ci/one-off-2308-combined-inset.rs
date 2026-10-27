
#[test]
fn historical_inset_rejection_precedes_zero_component_budgets() {
    let mut source = source(false);
    source.apply_execution_patch(&inset(true)).unwrap();
    source.advance_to(1.5).unwrap();
    source.apply_execution_patch(&inset(false)).unwrap();
    source.advance_to(2.0).unwrap();
    let before = source.frame().clone();
    let publication = source.publication_context();
    for _ in 0..2 {
        assert!(matches!(
            source.create_prefix_projection_with_object_budget(0, 0, 0),
            Err(PrefixError::Replay(ReplayError::UnsupportedDomain))
        ));
        assert_eq!(source.frame(), &before);
        assert_eq!(source.publication_context(), publication);
    }
    source.seal_replay().unwrap();
    source.seek(1.25).unwrap();
    assert_eq!(source.inset_2d_views().count(), 1);
    source.seek(1.75).unwrap();
    assert_eq!(source.inset_2d_views().count(), 0);
}
