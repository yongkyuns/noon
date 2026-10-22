use super::*;

#[test]
fn prefix_admission_capacity_preflight_preserves_batch_frame_and_active_effect() {
    let mut source = Producer::new();
    let old = source.object;
    let (mut publisher, mut reader) = source.bootstrap();
    reader.seek(0.5).unwrap();
    let prepared = reader.prepare_bound_click_indicate(old).unwrap().unwrap();
    reader.start_transient_animation(prepared).unwrap();
    reader.advance_interactions(10.0).unwrap();
    reader.advance_interactions(10.125).unwrap();
    source.delete();
    append(&mut reader, source.publish(&mut publisher));
    let new = source.replacement();
    let batch = source.publish(&mut publisher);
    let frame = reader.frame().clone();
    let history = reader.replay_stats();
    let context = reader.take_renderer_publication().context();
    let frontier = reader.frontier();
    let required = batch.required_object_capacity();

    for _ in 0..4 {
        assert_eq!(batch.new_object_ids().len(), 1);
        assert_eq!(batch.new_object_ids().collect::<Vec<_>>(), vec![new]);
        assert_eq!(
            reader.preflight_append(&batch),
            Err(PrefixError::CapacityRequired {
                object_slots: required,
            })
        );
        assert_eq!(reader.frame(), &frame);
        assert_eq!(reader.replay_stats(), history);
        assert_eq!(reader.take_renderer_publication().context(), context);
        assert_eq!(reader.frontier(), frontier);
        assert!(reader.interactions_active());
    }

    reader.reserve_object_capacity(required).unwrap();
    for _ in 0..4 {
        assert_eq!(reader.preflight_append(&batch), Ok(batch.stats()));
        assert_eq!(reader.frame(), &frame);
        assert_eq!(reader.replay_stats(), history);
        assert_eq!(reader.take_renderer_publication().context(), context);
        assert_eq!(reader.frontier(), frontier);
        absent(&mut reader, new);
    }
    let expected = batch.stats();
    assert_eq!(reader.append(batch).unwrap(), expected);
    assert_eq!(reader.frame(), &frame);
    assert_eq!(reader.take_renderer_publication().context(), context);
    assert!(reader.interactions_active());
    reader.advance_interactions(10.5).unwrap();
    reader.advance_to(2.5).unwrap();
    exercise(&mut reader, new, B, 20.0);
}

#[test]
fn prefix_admission_out_of_order_batch_can_be_delivered_after_its_predecessor() {
    let mut source = Producer::new();
    let (mut publisher, mut reader) = source.bootstrap();
    source.delete();
    let first = source.publish(&mut publisher);
    assert_eq!(first.new_object_ids().len(), 0);
    let new = source.replacement();
    let second = source.publish(&mut publisher);
    reader
        .reserve_object_capacity(second.required_object_capacity())
        .unwrap();
    let frame = reader.frame().clone();
    let history = reader.replay_stats();
    let context = reader.take_renderer_publication().context();
    let rejected = reader.preflight_append(&second);
    assert_eq!(rejected, Err(PrefixError::UnexpectedBatch));
    assert_eq!(reader.frame(), &frame);
    assert_eq!(reader.replay_stats(), history);
    assert_eq!(reader.take_renderer_publication().context(), context);
    assert_eq!(reader.frontier(), 1.0);

    reader.preflight_append(&first).unwrap();
    reader.append(first).unwrap();
    reader.preflight_append(&second).unwrap();
    // A successful preflight does not freeze playback or reserve the reader.
    reader.seek(0.5).unwrap();
    reader.append(second).unwrap();
    assert_eq!(reader.frontier(), 3.0);
    reader.seek(2.5).unwrap();
    exercise(&mut reader, new, B, 20.0);
}

#[test]
fn prefix_admission_wrong_reader_rejection_keeps_batch_for_its_own_stream() {
    let mut left = Producer::new();
    let mut right = Producer::new();
    let (_, mut left_reader) = left.bootstrap();
    let (mut publisher, mut right_reader) = right.bootstrap();
    let batch = right.publish(&mut publisher);
    let frame = left_reader.frame().clone();
    let history = left_reader.replay_stats();
    let context = left_reader.take_renderer_publication().context();
    for _ in 0..4 {
        let rejected = left_reader.preflight_append(&batch);
        assert_eq!(rejected, Err(PrefixError::UnexpectedBatch));
        assert_eq!(left_reader.frame(), &frame);
        assert_eq!(left_reader.replay_stats(), history);
        assert_eq!(left_reader.take_renderer_publication().context(), context);
        assert_eq!(left_reader.frontier(), 1.0);
    }
    right_reader.preflight_append(&batch).unwrap();
    right_reader.append(batch).unwrap();
    assert_eq!(right_reader.frontier(), 2.0);
    assert_eq!(left_reader.frontier(), 1.0);
}
