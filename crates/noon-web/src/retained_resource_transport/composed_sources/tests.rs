use super::*;

fn handle(version: u64) -> TransportComposedSourceHandle {
    TransportComposedSourceHandle {
        session: 73,
        owner: ObjectId::new((4 << 32) | 19),
        version,
    }
}
fn source(generation: u64) -> ComposedSource {
    ComposedSource::new(
        [ObjectId::new(generation << 32 | 1), ObjectId::new(2)],
        None,
    )
    .unwrap()
}
fn empty() -> RetainedResourceBundle {
    RetainedResourceBundle::capture(
        [],
        &TextResourceArena::new(),
        &GeometryResourceArena::new(),
        &FontResourceArena::new(),
    )
    .unwrap()
}

#[test]
fn cbor_source_plan_preserves_full_generations_and_resolves_one_shared_arc() {
    let mut plan = source(7);
    let parent = ObjectId::new((9 << 32) | 20);
    plan = ComposedSource::new(plan.members().iter().copied(), Some(parent)).unwrap();
    let bundle = empty()
        .with_composed_sources([(handle(11), &plan)])
        .unwrap();
    assert!(!bundle.is_empty());
    assert_eq!(bundle.composed_source_count(), 1);
    let bytes = bundle.encode_binary().unwrap();
    let installed = RetainedResourceBundle::decode_binary(&bytes)
        .unwrap()
        .install()
        .unwrap();
    let first = installed.composed_source(handle(11)).unwrap();
    let second = installed.composed_source(handle(11)).unwrap();
    assert!(Arc::ptr_eq(first, second));
    assert_eq!(first.as_ref(), &plan);
    for wrong in [
        TransportComposedSourceHandle {
            session: 74,
            ..handle(11)
        },
        TransportComposedSourceHandle {
            owner: ObjectId::new((5 << 32) | 19),
            ..handle(11)
        },
        handle(12),
    ] {
        assert!(
            installed.composed_source(wrong).is_none(),
            "exact resource provenance"
        );
    }
}

#[test]
fn raw_install_rejects_malformed_source_payloads_before_installation() {
    let owner = handle(0).owner;
    let input = ObjectId::new(1);
    for (members, parent) in [
        (vec![], None),
        (vec![input, input], None),
        (vec![owner], None),
        (vec![input], Some(owner)),
        (vec![input], Some(input)),
    ] {
        let mut bundle = empty();
        bundle.composed_sources.push(TransportComposedSourceEntry {
            handle: handle(0),
            members,
            parent_scope: parent,
        });
        let bytes = bundle.encode_binary().unwrap();
        assert!(matches!(
            RetainedResourceBundle::decode_binary(&bytes).unwrap().install(),
            Err(RetainedResourceTransportError::InvalidComposedSource { handle: rejected, .. }) if rejected == handle(0)
        ));
    }
}

#[test]
fn immutable_handle_cannot_be_rebound_by_capture_or_staged_additions() {
    let original = source(0);
    let replacement = source(1);
    let bundle = empty()
        .with_composed_sources([(handle(0), &original)])
        .unwrap();
    assert!(matches!(
        bundle.clone().with_composed_sources([(handle(0), &replacement)]),
        Err(RetainedResourceTransportError::DuplicateComposedSource(rejected)) if rejected == handle(0)
    ));
    let installed = bundle.clone().install().unwrap();
    let rebound = empty()
        .with_composed_sources([(handle(0), &replacement)])
        .unwrap();
    assert!(matches!(
        installed.prepare_additions(rebound),
        Err(RetainedResourceTransportError::DuplicateComposedSource(rejected)) if rejected == handle(0)
    ));
    let mut duplicate = bundle;
    duplicate
        .composed_sources
        .push(duplicate.composed_sources[0].clone());
    assert!(matches!(
        duplicate.install(),
        Err(RetainedResourceTransportError::DuplicateComposedSource(_))
    ));
    assert_eq!(
        installed.composed_source(handle(0)).unwrap().as_ref(),
        &original
    );
}

#[test]
fn preparing_then_dropping_additions_preserves_existing_resources_and_retry() {
    let original = source(0);
    let replacement = source(1);
    let mut installed = empty()
        .with_composed_sources([(handle(0), &original)])
        .unwrap()
        .install()
        .unwrap();
    let old_arc = Arc::downgrade(installed.composed_source(handle(0)).unwrap());
    let next = empty()
        .with_composed_sources([(handle(1), &replacement)])
        .unwrap();
    let prepared = installed.prepare_additions(next.clone()).unwrap();
    assert_eq!(installed.composed_source_count(), 1);
    assert!(installed.composed_source(handle(1)).is_none());
    drop(prepared);
    assert!(old_arc.upgrade().is_some());
    let prepared = installed.prepare_additions(next).unwrap();
    installed.commit_additions(prepared);
    assert_eq!(installed.composed_source_count(), 2);
    installed.retire(&RetainedResourceRetirements {
        composed_sources: vec![handle(0)],
        ..Default::default()
    });
    assert!(old_arc.upgrade().is_none());
    assert_eq!(
        installed.composed_source(handle(1)).unwrap().as_ref(),
        &replacement
    );
}

#[test]
fn repeated_plan_replacement_retires_payloads_inventory_and_arena_layers() {
    let mut installed = empty().install().unwrap();
    let mut previous = None;
    for version in 0..256 {
        let plan = source(version);
        let mut additions = empty()
            .with_composed_sources([(handle(version), &plan)])
            .unwrap();
        let mut producer = installed.inventory();
        additions.retain_additions(&mut producer);
        assert_eq!(additions.composed_source_count(), 1);
        let mut duplicate = additions.clone();
        duplicate.retain_additions(&mut producer);
        assert!(duplicate.is_empty());
        let prepared = installed.prepare_additions(additions).unwrap();
        installed.commit_additions(prepared);
        if version > 0 {
            let retired = RetainedResourceRetirements {
                composed_sources: vec![handle(version - 1)],
                ..Default::default()
            };
            producer.forget_root_retirements(&retired);
            installed.retire(&retired);
        }
        assert_eq!(producer.composed_sources.len(), 1);
        if let Some(previous) = previous.take() {
            assert!(
                std::sync::Weak::upgrade(&previous).is_none(),
                "retired source payload released"
            );
        }
        previous = Some(Arc::downgrade(
            installed.composed_source(handle(version)).unwrap(),
        ));
        assert_eq!(installed.composed_source_count(), 1);
        assert_eq!(installed.inventory.composed_sources.len(), 1);
        assert!(
            installed.additions.is_empty(),
            "source plans do not retain arena layers"
        );
    }
    installed.retire(&RetainedResourceRetirements {
        composed_sources: vec![handle(255)],
        ..Default::default()
    });
    assert_eq!(installed.composed_source_count(), 0);
    assert!(installed.inventory.composed_sources.is_empty());
    assert!(installed.additions.is_empty());
    assert!(previous.unwrap().upgrade().is_none());
}

#[test]
fn source_payload_and_inventory_have_one_owner_with_mixed_physical_resources() {
    let plan = source(0);
    let mut images = noon_core::RasterImageResourceArena::new();
    let image = images.intern_rgba8(1, 1, vec![15, 41, 87, 255]).unwrap();
    let bundle = empty()
        .with_composed_sources([(handle(0), &plan)])
        .unwrap()
        .with_images([image], &images)
        .unwrap();
    let mut installed = empty().install().unwrap();
    let prepared = installed.prepare_additions(bundle).unwrap();
    installed.commit_additions(prepared);
    assert_eq!(installed.image_count(), 1);
    assert_eq!(installed.additions.len(), 1);
    assert!(installed.additions[0].composed_sources.is_empty());
    assert!(installed.additions[0].inventory.composed_sources.is_empty());
    let weak = Arc::downgrade(installed.composed_source(handle(0)).unwrap());
    installed.retire(&RetainedResourceRetirements {
        composed_sources: vec![handle(0)],
        ..Default::default()
    });
    assert!(weak.upgrade().is_none());
    assert_eq!(
        installed.image_count(),
        1,
        "unrelated physical resource remains resident"
    );
    installed.retire(&RetainedResourceRetirements {
        images: vec![image.into()],
        ..Default::default()
    });
    assert_eq!(installed.image_count(), 0);
    assert_eq!(installed.inventory.counts(), [0; 5]);
}

#[test]
fn resource_protocol_rejects_previous_version_and_duplicate_source_retirements() {
    let mut old = empty();
    old.protocol_version = 14;
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(&old, &mut bytes).unwrap();
    assert!(matches!(
        RetainedResourceBundle::decode_binary(&bytes),
        Err(RetainedResourceTransportError::UnsupportedVersion(14))
    ));
    let retired = RetainedResourceRetirements {
        composed_sources: vec![handle(0), handle(0)],
        ..Default::default()
    };
    assert!(!retired.is_empty());
    assert_eq!(
        retired.validate(),
        Err(RetainedResourceTransportError::DuplicateRetiredComposedSource)
    );
}
