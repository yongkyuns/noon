use super::*;

fn pcm(sample_rate: u32, channels: u16, frames: usize) -> AudioResource {
    AudioResource::from_interleaved(
        sample_rate,
        channels,
        vec![0.25; frames * usize::from(channels)],
        DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET,
    )
    .unwrap()
}

#[test]
fn intrinsic_frames_and_duration_are_independent_of_channel_count() {
    for rate in [8_000, 44_100, 48_000, 192_000] {
        for channels in [1, 2] {
            let resource = pcm(rate, channels, rate as usize / 10);
            assert_eq!(resource.sample_rate(), rate);
            assert_eq!(resource.channels(), channels);
            assert_eq!(resource.frames(), rate as usize / 10);
            assert_eq!(resource.duration_seconds(), 0.1);
            assert_eq!(resource.pcm_bytes(), resource.samples().len() * 4);
        }
    }
}

#[test]
fn invalid_payloads_leave_the_arena_unchanged() {
    let mut arena = AudioResourceArena::new();
    let original = arena.insert(pcm(48_000, 2, 8)).unwrap();
    let snapshot = arena.get_shared(original).unwrap();
    let before = arena.stats();
    for rate in [0, 7_999, 192_001, u32::MAX] {
        assert_eq!(
            arena.insert_interleaved(rate, 2, [0.0, 0.0]),
            Err(AudioResourceError::UnsupportedSampleRate { sample_rate: rate })
        );
    }
    for channels in [0, 3, u16::MAX] {
        assert_eq!(
            arena.insert_interleaved(48_000, channels, [0.0, 0.0]),
            Err(AudioResourceError::UnsupportedChannelCount { channels })
        );
    }
    assert_eq!(
        arena.insert_interleaved(48_000, 1, []),
        Err(AudioResourceError::EmptyPcm)
    );
    assert_eq!(
        arena.insert_interleaved(48_000, 2, [0.0, 0.0, 0.0]),
        Err(AudioResourceError::IncompleteFrame {
            samples: 3,
            channels: 2
        })
    );
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            arena.insert_interleaved(48_000, 2, [0.0, invalid]),
            Err(AudioResourceError::NonFiniteSample { index: 1 })
        );
    }
    assert_eq!(arena.stats(), before);
    assert!(Arc::ptr_eq(&arena.get_shared(original).unwrap(), &snapshot));
    let next = arena.insert(pcm(48_000, 2, 8)).unwrap();
    assert_eq!(next.id.get(), original.id.get() + 1);
}

#[test]
fn resource_payload_limit_is_exact_and_does_not_normalize_samples() {
    let samples: Arc<[f32]> = [2.0, -2.0, 0.0, -0.0].into();
    assert_eq!(
        AudioResource::from_interleaved(48_000, 2, samples.clone(), 15),
        Err(AudioResourceError::PcmByteLimitExceeded {
            bytes: 16,
            limit: 15
        })
    );
    let resource = AudioResource::from_interleaved(48_000, 2, samples.clone(), 16).unwrap();
    assert_eq!(resource.samples(), samples.as_ref());
    assert!(std::ptr::eq(resource.samples(), samples.as_ref()));
}

#[test]
fn arena_budget_is_atomic_and_reclamation_releases_capacity() {
    let resource = pcm(48_000, 2, 2);
    let budget = resource.retained_bytes();
    let mut arena = AudioResourceArena::with_byte_budget(budget);
    let first = arena.insert(resource.clone()).unwrap();
    let before = arena.stats();
    assert_eq!(
        arena.insert(resource.clone()),
        Err(AudioResourceError::ByteBudgetExceeded {
            retained_bytes: budget,
            required_bytes: budget,
            budget,
        })
    );
    assert_eq!(arena.stats(), before);
    let pinned = arena.get_shared(first).unwrap();
    assert!(Arc::ptr_eq(&arena.remove(first).unwrap(), &pinned));
    assert_eq!(arena.stats(), AudioResourceStats::default());
    assert!(arena.get(first).is_none());
    assert_eq!(pinned.samples(), resource.samples());
    let second = arena.insert(resource).unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(second.id.get(), first.id.get() + 1);
    assert!(arena.remove(first).is_none());
    assert_eq!(arena.len(), 1);
}

#[test]
fn shared_admission_reuses_one_budget_charge_but_explicit_insert_remains_fresh() {
    let resource = pcm(8_000, 1, 4);
    let budget = resource.retained_bytes() * 2;
    let mut arena = AudioResourceArena::with_byte_budget(budget);
    let (shared, fresh) = arena.admit_shared(resource.clone()).unwrap();
    assert!(fresh);
    let before = arena.stats();
    assert_eq!(
        arena.admit_shared(resource.clone()).unwrap(),
        (shared, false)
    );
    assert_eq!(arena.stats(), before);

    let explicit = arena.insert(resource.clone()).unwrap();
    assert_ne!(explicit, shared);
    assert_eq!(arena.stats().live_resources, 2);
    assert_eq!(arena.stats().retained_bytes, budget);
    assert!(matches!(
        arena.insert(resource.clone()),
        Err(AudioResourceError::ByteBudgetExceeded { .. })
    ));
    assert_eq!(
        arena.admit_shared(resource.clone()).unwrap(),
        (shared, false)
    );
    arena.remove(explicit).unwrap();
    assert_eq!(arena.admit_shared(resource).unwrap(), (shared, false));
    assert_eq!(arena.stats(), before);
}

#[test]
fn shared_admission_distinguishes_format_and_allocations_and_retires_its_index() {
    let samples: Arc<[f32]> = vec![0.25; 8].into();
    let resource = AudioResource::from_interleaved(8_000, 1, samples.clone(), 64).unwrap();
    let mut arena = AudioResourceArena::new();
    let (first, _) = arena.admit_shared(resource.clone()).unwrap();
    for alternate in [
        AudioResource::from_interleaved(16_000, 1, samples.clone(), 64).unwrap(),
        AudioResource::from_interleaved(8_000, 2, samples, 64).unwrap(),
        pcm(8_000, 1, 8),
    ] {
        let (handle, fresh) = arena.admit_shared(alternate).unwrap();
        assert!(fresh);
        assert_ne!(handle, first);
    }
    arena.remove(first).unwrap();
    assert!(arena.get(first).is_none());
    let (next, fresh) = arena.admit_shared(resource.clone()).unwrap();
    assert!(fresh);
    assert_ne!(next, first);
    assert_eq!(arena.admit_shared(resource).unwrap(), (next, false));
    assert_eq!(arena.len(), 4);
}

#[test]
fn cloned_arena_reuses_payload_with_its_own_resource_provenance() {
    let resource = pcm(8_000, 1, 4);
    let mut original = AudioResourceArena::new();
    let (handle, _) = original.admit_shared(resource.clone()).unwrap();
    let mut cloned = original.clone();
    let (remapped, fresh) = cloned.admit_shared(resource.clone()).unwrap();
    assert!(!fresh);
    assert_ne!(handle.arena, remapped.arena);
    assert_eq!(handle.id, remapped.id);
    assert!(cloned.get(handle).is_none());
    assert!(Arc::ptr_eq(
        &original.get_shared(handle).unwrap(),
        &cloned.get_shared(remapped).unwrap(),
    ));
    original.remove(handle).unwrap();
    assert_eq!(cloned.admit_shared(resource).unwrap(), (remapped, false));
    assert_eq!(cloned.len(), 1);
}

#[test]
fn foreign_wrong_version_and_zero_handles_do_not_resolve_or_remove() {
    let mut arena = AudioResourceArena::new();
    let handle = arena.insert(pcm(8_000, 1, 1)).unwrap();
    let mut other = AudioResourceArena::new();
    let foreign = other.insert(pcm(8_000, 1, 1)).unwrap();
    let before = arena.stats();
    for invalid in [
        foreign,
        AudioResourceHandle { arena: 0, ..handle },
        AudioResourceHandle {
            version: 1,
            ..handle
        },
        AudioResourceHandle {
            id: AudioResourceId::new(u64::MAX),
            ..handle
        },
    ] {
        assert!(arena.get(invalid).is_none());
        assert!(arena.get_shared(invalid).is_none());
        assert!(arena.remove(invalid).is_none());
    }
    assert_eq!(arena.stats(), before);
    assert!(arena.get(handle).is_some());
}

#[test]
fn cloning_replaces_namespace_but_shares_immutable_payloads() {
    let mut original = AudioResourceArena::new();
    let handle = original.insert(pcm(48_000, 2, 4)).unwrap();
    let snapshot = original.get_shared(handle).unwrap();
    let mut cloned = original.clone();
    assert_ne!(cloned.namespace(), original.namespace());
    assert!(cloned.get(handle).is_none());
    let remapped = AudioResourceHandle {
        arena: cloned.namespace(),
        ..handle
    };
    assert!(original.get(remapped).is_none());
    assert!(Arc::ptr_eq(
        &snapshot,
        &cloned.get_shared(remapped).unwrap()
    ));
    original.remove(handle).unwrap();
    assert!(original.is_empty());
    assert_eq!(cloned.len(), 1);
    assert_eq!(cloned.get(remapped).unwrap().frames(), 4);
    cloned.remove(remapped).unwrap();
    assert!(cloned.is_empty());
    assert_eq!(snapshot.frames(), 4);
}

#[test]
fn exhaustion_and_tiny_budgets_do_not_consume_admission_state() {
    let resource = pcm(8_000, 1, 1);
    for budget in [0, resource.retained_bytes() - 1] {
        let mut arena = AudioResourceArena::with_byte_budget(budget);
        assert!(matches!(
            arena.insert(resource.clone()),
            Err(AudioResourceError::ByteBudgetExceeded { .. })
        ));
        assert_eq!(arena.stats(), AudioResourceStats::default());
        assert_eq!(arena.next_id, 0);
    }
    let mut arena = AudioResourceArena::new();
    arena.next_id = u64::MAX;
    assert_eq!(
        arena.insert(resource),
        Err(AudioResourceError::IdentityExhausted)
    );
    assert_eq!(arena.stats(), AudioResourceStats::default());
    assert_eq!(arena.next_id, u64::MAX);
}
