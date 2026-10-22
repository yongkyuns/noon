use super::*;
use noon::ExecutionSegment;

fn bootstrap(source: &mut Producer) -> (ExecutionSegment, PrefixPublisher, PrefixReader) {
    let boundary = source.session.wait_segment(1.0).unwrap();
    source.session.advance_to(boundary.end_time()).unwrap();
    let (publisher, reader) = source.session.create_prepared_prefix(boundary).unwrap();
    (boundary, publisher, reader)
}

fn finish(
    source: &Producer,
    boundary: ExecutionSegment,
    publisher: &mut PrefixPublisher,
) -> Result<PrefixBatch, PrefixError> {
    source.session.finish_prepared_prefix(boundary, publisher)
}

#[test]
fn terminal_requires_sealed_source_and_explicit_reader_adoption() {
    let mut source = Producer::new();
    let (boundary, mut publisher, mut reader) = bootstrap(&mut source);
    assert!(matches!(
        finish(&source, boundary, &mut publisher),
        Err(PrefixError::NotReady)
    ));
    assert!(!source.session.replay_is_sealed());
    reader.seek(0.5).unwrap();
    let displayed = reader.frame().clone();
    let context = reader.take_renderer_publication().context();
    let history = reader.replay_stats();
    source.session.seal_replay().unwrap();
    let expected = source.session.frame().clone();
    let terminal = finish(&source, boundary, &mut publisher).unwrap();
    assert!(terminal.is_terminal());
    assert_eq!(terminal.frontier(), 1.0);
    assert_eq!(terminal.stats().revisions, 0);
    assert_eq!(terminal.stats().payloads, 0);
    assert_eq!(terminal.new_object_ids().len(), 0);
    assert!(matches!(
        finish(&source, boundary, &mut publisher),
        Err(PrefixError::NotReady)
    ));
    assert!(matches!(
        source
            .session
            .publish_prepared_prefix(boundary, &mut publisher),
        Err(PrefixError::NotReady)
    ));
    drop(source);
    drop(publisher);

    assert_eq!(reader.end_time(), None);
    reader.preflight_append(&terminal).unwrap();
    assert_eq!(reader.end_time(), None);
    assert!(matches!(reader.seek(1.0), Err(PrefixError::OutsideRange { .. })));
    assert_eq!(reader.frame(), &displayed);
    assert_eq!(reader.replay_stats(), history);
    assert_eq!(reader.take_renderer_publication().context(), context);
    reader.append(terminal).unwrap();
    assert_eq!(reader.end_time(), Some(1.0));
    assert_eq!(reader.frame(), &displayed);
    assert_eq!(reader.replay_stats(), history);
    assert_eq!(reader.take_renderer_publication().context(), context);
    reader.advance_to(1.0).unwrap();
    assert_eq!(reader.frame(), &expected);
    reader.seek(0.5).unwrap();
    assert_eq!(reader.frame(), &displayed);
    reader.seek(1.0).unwrap();
    assert_eq!(reader.frame(), &expected);
}

#[test]
fn terminal_same_frontier_replacement_keeps_old_effect_until_explicit_evaluation() {
    let mut source = Producer::new();
    let old = source.object;
    let (boundary, mut publisher, mut reader) = bootstrap(&mut source);
    reader.seek(0.5).unwrap();
    let baseline = reader.frame().clone();
    let prepared = reader.prepare_bound_click_indicate(old).unwrap().unwrap();
    reader.start_transient_animation(prepared).unwrap();
    reader.advance_interactions(10.0).unwrap();
    reader.advance_interactions(10.125).unwrap();
    let displayed = reader.frame().clone();
    let history = reader.replay_stats();
    let context = reader.take_renderer_publication().context();
    source.delete();
    let new = source.replacement();
    source.session.seal_replay().unwrap();
    let expected = source.session.frame().clone();
    let terminal = finish(&source, boundary, &mut publisher).unwrap();
    assert_eq!(terminal.frontier(), reader.frontier());
    assert_eq!(terminal.stats().new_object_slots, 1);
    assert!(terminal.stats().revisions > 0);
    let required = terminal.required_object_capacity();
    assert_eq!(
        reader.preflight_append(&terminal),
        Err(PrefixError::CapacityRequired {
            object_slots: required,
        })
    );
    assert_eq!(reader.end_time(), None);
    assert_eq!(reader.frame(), &displayed);
    assert_eq!(reader.replay_stats(), history);
    assert_eq!(reader.take_renderer_publication().context(), context);
    assert!(reader.interactions_active());
    reader.reserve_object_capacity(required).unwrap();
    reader.preflight_append(&terminal).unwrap();
    reader.append(terminal).unwrap();
    assert_eq!(reader.end_time(), Some(1.0));
    assert_eq!(reader.frame(), &displayed);
    assert_eq!(reader.take_renderer_publication().context(), context);
    assert!(reader.interactions_active());
    absent(&mut reader, new);
    drop(source);
    drop(publisher);

    reader.advance_interactions(10.5).unwrap();
    assert_eq!(reader.frame(), &baseline);
    reader.advance_to(1.0).unwrap();
    assert_eq!(reader.frame(), &expected);
    absent(&mut reader, old);
    exercise(&mut reader, new, B, 20.0);
    reader.seek(0.5).unwrap();
    absent(&mut reader, new);
    exercise(&mut reader, old, A, 30.0);
}

#[test]
fn terminal_carries_the_unpublished_suffix_without_an_intermediate_snapshot() {
    let mut source = Producer::new();
    let old = source.object;
    let (_, mut publisher, mut reader) = bootstrap(&mut source);
    source.delete();
    let new = source.replacement();
    let last = source.session.wait_segment(1.0).unwrap();
    source.session.advance_to(last.end_time()).unwrap();
    source.session.seal_replay().unwrap();
    let expected = source.session.frame().clone();
    let terminal = finish(&source, last, &mut publisher).unwrap();
    assert!(terminal.is_terminal());
    assert_eq!(terminal.frontier(), 2.0);
    assert_eq!(terminal.new_object_ids().collect::<Vec<_>>(), vec![new]);
    let displayed = reader.frame().clone();
    append(&mut reader, terminal);
    assert_eq!(reader.frame(), &displayed);
    drop(source);
    drop(publisher);
    reader.seek(2.0).unwrap();
    assert_eq!(reader.frame(), &expected);
    absent(&mut reader, old);
    exercise(&mut reader, new, B, 10.0);
    reader.seek(0.5).unwrap();
    exercise(&mut reader, old, A, 20.0);
    reader.seek(2.0).unwrap();
    assert_eq!(reader.frame(), &expected);
}

#[test]
fn terminal_out_of_order_delivery_does_not_advertise_completion() {
    let mut source = Producer::new();
    let (_, mut publisher, mut reader) = bootstrap(&mut source);
    let boundary = source.session.wait_segment(1.0).unwrap();
    source.session.advance_to(boundary.end_time()).unwrap();
    let preceding = source
        .session
        .publish_prepared_prefix(boundary, &mut publisher)
        .unwrap();
    assert!(!preceding.is_terminal());
    source.session.seal_replay().unwrap();
    let terminal = finish(&source, boundary, &mut publisher).unwrap();
    let displayed = reader.frame().clone();
    let history = reader.replay_stats();
    let context = reader.take_renderer_publication().context();
    assert_eq!(
        reader.preflight_append(&terminal),
        Err(PrefixError::UnexpectedBatch)
    );
    assert_eq!(reader.frame(), &displayed);
    assert_eq!(reader.replay_stats(), history);
    assert_eq!(reader.take_renderer_publication().context(), context);
    assert_eq!(reader.end_time(), None);
    assert_eq!(reader.frontier(), 1.0);
    reader.append(preceding).unwrap();
    assert_eq!(reader.frontier(), 2.0);
    assert_eq!(reader.end_time(), None);
    assert!(matches!(reader.seek(2.0), Err(PrefixError::OutsideRange { .. })));
    reader.preflight_append(&terminal).unwrap();
    reader.append(terminal).unwrap();
    assert_eq!(reader.end_time(), Some(2.0));
    reader.seek(2.0).unwrap();
}

#[test]
fn terminal_foreign_stream_cannot_close_another_reader() {
    let mut left = Producer::new();
    let mut right = Producer::new();
    assert_eq!(left.object, right.object);
    let (_, _, mut left_reader) = bootstrap(&mut left);
    let (boundary, mut publisher, mut right_reader) = bootstrap(&mut right);
    right.session.seal_replay().unwrap();
    let terminal = finish(&right, boundary, &mut publisher).unwrap();
    let displayed = left_reader.frame().clone();
    let history = left_reader.replay_stats();
    let context = left_reader.take_renderer_publication().context();
    assert_eq!(
        left_reader.preflight_append(&terminal),
        Err(PrefixError::UnexpectedBatch)
    );
    assert_eq!(left_reader.end_time(), None);
    assert_eq!(left_reader.frame(), &displayed);
    assert_eq!(left_reader.replay_stats(), history);
    assert_eq!(left_reader.take_renderer_publication().context(), context);
    right_reader.append(terminal).unwrap();
    assert_eq!(right_reader.end_time(), Some(1.0));
    assert_eq!(left_reader.end_time(), None);
    right_reader.seek(1.0).unwrap();
    assert!(matches!(left_reader.seek(1.0), Err(PrefixError::OutsideRange { .. })));
}

#[test]
fn terminal_requires_the_actual_sealed_endpoint_and_keeps_rejected_cursor_usable() {
    let mut source = Producer::new();
    let (early, mut publisher, mut reader) = bootstrap(&mut source);
    let last = source.session.wait_segment(1.0).unwrap();
    source.session.advance_to(last.end_time()).unwrap();
    source.session.seal_replay().unwrap();
    assert!(matches!(
        finish(&source, early, &mut publisher),
        Err(PrefixError::NotReady)
    ));
    source.session.seek(1.0).unwrap();
    assert!(finish(&source, early, &mut publisher).is_err());
    assert!(finish(&source, last, &mut publisher).is_err());
    source.session.seek(2.0).unwrap();
    let terminal = finish(&source, last, &mut publisher).unwrap();
    reader.append(terminal).unwrap();
    assert_eq!(reader.end_time(), Some(2.0));
    reader.seek(2.0).unwrap();
}

#[test]
fn terminal_invalid_time_preserves_endpoint_effect_and_finished_metadata() {
    let mut source = Producer::new();
    let object = source.object;
    let (boundary, mut publisher, mut reader) = bootstrap(&mut source);
    source.session.seal_replay().unwrap();
    let terminal = finish(&source, boundary, &mut publisher).unwrap();
    reader.append(terminal).unwrap();
    reader.seek(1.0).unwrap();
    let baseline = reader.frame().clone();
    let prepared = reader.prepare_bound_click_indicate(object).unwrap().unwrap();
    reader.start_transient_animation(prepared).unwrap();
    reader.advance_interactions(10.0).unwrap();
    reader.advance_interactions(10.125).unwrap();
    let displayed = reader.frame().clone();
    let history = reader.replay_stats();
    let context = reader.take_renderer_publication().context();
    for time in [-1.0, 1.0001, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(matches!(
            reader.seek(time),
            Err(PrefixError::OutsideFinishedRange { .. })
        ));
        assert!(matches!(
            reader.advance_to(time),
            Err(PrefixError::OutsideFinishedRange { .. })
        ));
        assert_eq!(reader.end_time(), Some(1.0));
        assert_eq!(reader.frame(), &displayed);
        assert_eq!(reader.replay_stats(), history);
        assert_eq!(reader.take_renderer_publication().context(), context);
        assert!(reader.interactions_active());
    }
    reader.advance_to(1.0).unwrap();
    assert!(reader.interactions_active());
    reader.advance_interactions(10.5).unwrap();
    assert_eq!(reader.frame(), &baseline);
    assert!(!reader.interactions_active());
}

#[test]
fn terminal_replaced_source_scope_does_not_close_the_delivered_prefix() {
    let mut source = Producer::new();
    let (_, mut publisher, mut reader) = bootstrap(&mut source);
    source.session.discard_replay_retention();
    source
        .session
        .begin_replay_retention(ReplayLimits {
            revisions: 128,
            payloads: 2048,
        })
        .unwrap();
    let boundary = source.session.wait_segment(1.0).unwrap();
    source.session.advance_to(boundary.end_time()).unwrap();
    source.session.seal_replay().unwrap();
    assert!(matches!(
        finish(&source, boundary, &mut publisher),
        Err(PrefixError::SourceChanged)
    ));
    drop(source);
    assert_eq!(reader.end_time(), None);
    assert_eq!(reader.frontier(), 1.0);
    reader.seek(0.5).unwrap();
    assert!(matches!(reader.seek(1.0), Err(PrefixError::OutsideRange { .. })));
}
