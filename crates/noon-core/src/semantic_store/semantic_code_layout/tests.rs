use super::*;
use crate::{
    GeometryResource, PendingGeometryPublicationError, SceneRevision, SemanticLocalNodeToken,
    SemanticMutationTransaction, SemanticMutationTransactionError, SemanticNodeCreation,
    SemanticObjectContent, SemanticObjectState, SemanticStore, SemanticStyle,
    SemanticTransform2_5D, StoredGeometry, Vec2, VectorPath,
};

fn metrics() -> SemanticCodeLayoutMetrics {
    SemanticCodeLayoutMetrics {
        line_count: 3,
        first_baseline: 0.5,
        baseline_step: 0.4,
        row_bottom: -0.1,
        row_top: 0.2,
        body_left: -0.5,
        body_right: 1.5,
        gutter_left: -0.9,
    }
}

fn panel_path() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(-1.0, -1.0))
        .line_to(Vec2::new(1.0, -1.0))
        .line_to(Vec2::new(1.0, 1.0))
        .line_to(Vec2::new(-1.0, 1.0))
        .close()
}

struct PendingListing {
    root: SemanticLocalNodeToken,
    nodes: SemanticCodeLayoutNodes<SemanticLocalNodeToken>,
    panel_geometry: SemanticLocalResourceToken,
}

impl PendingListing {
    fn layout(&self, metrics: SemanticCodeLayoutMetrics) -> SemanticTransactionCodeLayout {
        SemanticTransactionCodeLayout::new(
            self.nodes.map_nodes(Into::into),
            metrics,
            self.panel_geometry,
        )
    }
}

fn stage_listing(tx: &mut SemanticMutationTransaction) -> PendingListing {
    let panel_geometry = tx.stage_geometry_path(panel_path()).unwrap();
    let panel = tx.create_node(SemanticNodeCreation::pending_path_object(
        panel_geometry,
        SemanticTransform2_5D::default(),
        SemanticStyle::default(),
        0.0,
        Default::default(),
    ));
    let background = tx.create_node(SemanticNodeCreation::family());
    let highlights = tx.create_node(SemanticNodeCreation::family());
    let code_lines = tx.create_node(SemanticNodeCreation::family());
    let line_numbers = tx.create_node(SemanticNodeCreation::family());
    let root = tx.create_node(SemanticNodeCreation::family());
    tx.add_member(background, panel);
    for member in [background, highlights, line_numbers, code_lines] {
        tx.add_member(root, member);
    }
    PendingListing {
        root,
        nodes: SemanticCodeLayoutNodes {
            panel,
            background,
            highlights,
            code_lines,
            line_numbers: Some(line_numbers),
        },
        panel_geometry,
    }
}

fn admit(store: &mut SemanticStore) -> (SemanticNodeId, SemanticCodeLayout) {
    let mut tx = SemanticMutationTransaction::new();
    let listing = stage_listing(&mut tx);
    tx.set_code_layout(listing.root, listing.layout(metrics()));
    let result = tx.apply(store).unwrap();
    let root = result.resolve(listing.root).unwrap();
    (root, store.semantic_code_layout(root).unwrap().unwrap())
}

fn snapshot(store: &SemanticStore) -> (usize, usize, SceneRevision) {
    (
        store.len(),
        store.geometry_resources().len(),
        store.scene_revision(),
    )
}

#[test]
fn pending_panel_and_named_dependencies_resolve_without_extra_resources() {
    let mut store = SemanticStore::new();
    let mut tx = SemanticMutationTransaction::new();
    let listing = stage_listing(&mut tx);
    tx.set_code_layout(listing.root, listing.layout(metrics()));
    let before = snapshot(&store);
    let prepared = tx.prepare(&mut store).unwrap();
    // The ordinary scoped path publication also performs a second preflight
    // after materializing resources, as native/live Code admission does.
    let result = prepared
        .with_pending_geometry_paths(|prepared| Ok::<_, ()>(prepared.commit()))
        .unwrap();
    let root = result.resolve(listing.root).unwrap();
    let retained = store.semantic_code_layout(root).unwrap().unwrap();
    assert_eq!(retained.metrics(), &metrics());
    assert_eq!(
        retained.nodes(),
        &listing
            .nodes
            .map_nodes(|node| result.resolve(node).unwrap())
    );
    assert_eq!(
        store
            .semantic_object_state_checked(retained.nodes().panel)
            .unwrap()
            .content,
        SemanticObjectContent::Geometry(StoredGeometry::Resource(retained.panel_geometry()))
    );
    assert_eq!(store.geometry_resources().len(), before.1 + 1);
    assert_eq!(store.len(), before.0 + 6);
}

#[test]
fn discarded_and_failed_publication_leave_no_layout_nodes_or_resources() {
    let mut store = SemanticStore::new();
    let before = snapshot(&store);
    let mut tx = SemanticMutationTransaction::new();
    let listing = stage_listing(&mut tx);
    tx.set_code_layout(listing.root, listing.layout(metrics()));
    drop(tx.prepare(&mut store).unwrap());
    assert_eq!(snapshot(&store), before);

    let mut tx = SemanticMutationTransaction::new();
    let listing = stage_listing(&mut tx);
    tx.set_code_layout(listing.root, listing.layout(metrics()));
    let result = tx
        .prepare(&mut store)
        .unwrap()
        .with_pending_geometry_paths(|_| Err::<(), _>("publication rejected"));
    assert!(matches!(
        result,
        Err(PendingGeometryPublicationError::Publication(_))
    ));
    assert_eq!(snapshot(&store), before);
    assert!(store.code_layouts.is_empty());
}

#[test]
fn invalid_metrics_and_dependency_types_are_atomic() {
    let invalid = [
        SemanticCodeLayoutMetrics {
            line_count: 0,
            ..metrics()
        },
        SemanticCodeLayoutMetrics {
            baseline_step: 0.0,
            ..metrics()
        },
        SemanticCodeLayoutMetrics {
            baseline_step: -1.0,
            ..metrics()
        },
        SemanticCodeLayoutMetrics {
            first_baseline: f64::NAN,
            ..metrics()
        },
        SemanticCodeLayoutMetrics {
            row_top: f64::INFINITY,
            ..metrics()
        },
        SemanticCodeLayoutMetrics {
            row_top: -0.1,
            ..metrics()
        },
        SemanticCodeLayoutMetrics {
            row_top: f64::MAX,
            row_bottom: -f64::MAX,
            ..metrics()
        },
        SemanticCodeLayoutMetrics {
            body_right: -0.6,
            ..metrics()
        },
        SemanticCodeLayoutMetrics {
            gutter_left: 0.0,
            ..metrics()
        },
        SemanticCodeLayoutMetrics {
            baseline_step: f64::MAX,
            ..metrics()
        },
        SemanticCodeLayoutMetrics {
            body_right: f64::MAX,
            gutter_left: -f64::MAX,
            ..metrics()
        },
    ];
    for metrics in invalid {
        let mut store = SemanticStore::new();
        let before = snapshot(&store);
        let mut tx = SemanticMutationTransaction::new();
        let listing = stage_listing(&mut tx);
        tx.set_code_layout(listing.root, listing.layout(metrics));
        assert!(matches!(
            tx.apply(&mut store),
            Err(SemanticMutationTransactionError::InvalidCodeLayout { .. })
        ));
        assert_eq!(snapshot(&store), before);
    }

    for invalid_role in 0..3 {
        let mut store = SemanticStore::new();
        let before = snapshot(&store);
        let mut tx = SemanticMutationTransaction::new();
        let listing = stage_listing(&mut tx);
        let mut nodes = listing.nodes.map_nodes(Into::into);
        let owner = match invalid_role {
            0 => listing.nodes.panel,
            1 => {
                nodes.highlights = listing.nodes.panel.into();
                listing.root
            }
            _ => {
                nodes.panel = listing.nodes.highlights.into();
                listing.root
            }
        };
        tx.set_code_layout(
            owner,
            SemanticTransactionCodeLayout::new(nodes, metrics(), listing.panel_geometry),
        );
        assert!(tx.apply(&mut store).is_err());
        assert_eq!(snapshot(&store), before);
    }
}

#[test]
fn all_empty_unnumbered_listing_can_have_zero_allocated_body_width() {
    let mut store = SemanticStore::new();
    let mut tx = SemanticMutationTransaction::new();
    let listing = stage_listing(&mut tx);
    let mut nodes = listing.nodes.map_nodes(Into::into);
    nodes.line_numbers = None;
    let metrics = SemanticCodeLayoutMetrics {
        body_left: 0.0,
        body_right: 0.0,
        gutter_left: 0.0,
        ..metrics()
    };
    tx.set_code_layout(
        listing.root,
        SemanticTransactionCodeLayout::new(nodes, metrics, listing.panel_geometry),
    );
    let result = tx.apply(&mut store).unwrap();
    let layout = store
        .semantic_code_layout(result.resolve(listing.root).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(layout.metrics(), &metrics);
    assert_eq!(layout.nodes().line_numbers, None);
}

#[test]
fn pending_guard_must_name_its_panel_and_belong_to_the_transaction() {
    let mut foreign = SemanticMutationTransaction::new();
    let foreign_token = foreign.stage_geometry_path(panel_path()).unwrap();
    for foreign_guard in [false, true] {
        let mut store = SemanticStore::new();
        let before = snapshot(&store);
        let mut tx = SemanticMutationTransaction::new();
        let listing = stage_listing(&mut tx);
        let guard = if foreign_guard {
            foreign_token
        } else {
            tx.stage_geometry_path(panel_path()).unwrap()
        };
        tx.set_code_layout(
            listing.root,
            SemanticTransactionCodeLayout::new(
                listing.nodes.map_nodes(Into::into),
                metrics(),
                guard,
            ),
        );
        assert!(tx.apply(&mut store).is_err());
        assert_eq!(snapshot(&store), before);
    }
}

#[test]
fn repeated_writes_use_overlay_and_equal_layout_is_a_noop() {
    let mut store = SemanticStore::new();
    let mut tx = SemanticMutationTransaction::new();
    let listing = stage_listing(&mut tx);
    let changed = SemanticCodeLayoutMetrics {
        first_baseline: 2.0,
        ..metrics()
    };
    tx.set_code_layout(listing.root, listing.layout(metrics()));
    tx.set_code_layout(listing.root, listing.layout(changed));
    tx.set_code_layout(listing.root, listing.layout(metrics()));
    let result = tx.apply(&mut store).unwrap();
    let root = result.resolve(listing.root).unwrap();
    let original = store.semantic_code_layout(root).unwrap().unwrap();
    assert_eq!(original.metrics(), &metrics());
    let changed = SemanticCodeLayout::new(*original.nodes(), changed, original.panel_geometry());
    let mut tx = SemanticMutationTransaction::new();
    tx.set_code_layout(root, changed);
    tx.set_code_layout(root, original.clone());
    tx.apply(&mut store).unwrap();
    assert_eq!(
        store.semantic_code_layout(root).unwrap(),
        Some(original.clone())
    );
    let before = snapshot(&store);
    let mut tx = SemanticMutationTransaction::new();
    tx.set_code_layout(root, original);
    assert!(tx.apply(&mut store).unwrap().impacts().is_empty());
    assert_eq!(snapshot(&store), before);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn required_node_retirement_cleans_owner_and_slot_reuse_does_not_revive_layout() {
    for dependency in 0..5 {
        let mut store = SemanticStore::new();
        let (root, layout) = admit(&mut store);
        let target = layout.referenced_nodes().nth(dependency).unwrap();
        let mut tx = SemanticMutationTransaction::new();
        tx.remove_node(target);
        tx.apply(&mut store).unwrap();
        assert!(store.node(root).is_none());
        assert!(!store.code_layouts.contains_key(&root));
        assert!(store.semantic_code_layout(root).is_err());
    }
    let mut store = SemanticStore::new();
    let (root, _) = admit(&mut store);
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_node(root);
    tx.apply(&mut store).unwrap();
    let mut tx = SemanticMutationTransaction::new();
    let replacement = tx.create_node(SemanticNodeCreation::family());
    let result = tx.apply(&mut store).unwrap();
    let replacement = result.resolve(replacement).unwrap();
    assert_ne!(replacement, root);
    assert_eq!(store.semantic_code_layout(replacement).unwrap(), None);
    assert!(store.semantic_code_layout(root).is_err());
}

#[test]
fn replacing_dependencies_releases_reverse_links_and_highlight_members_are_not_dependencies() {
    let mut store = SemanticStore::new();
    let (root, original) = admit(&mut store);
    let mut tx = SemanticMutationTransaction::new();
    let highlight = tx.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    tx.add_member(original.nodes().highlights, highlight);
    let result = tx.apply(&mut store).unwrap();
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_node(result.resolve(highlight).unwrap());
    tx.apply(&mut store).unwrap();
    assert_eq!(
        store.semantic_code_layout(root).unwrap(),
        Some(original.clone())
    );

    let old = original.nodes().highlights;
    let mut tx = SemanticMutationTransaction::new();
    let new = tx.create_node(SemanticNodeCreation::family());
    let mut nodes = original.nodes().map_nodes(Into::into);
    nodes.highlights = new.into();
    tx.remove_member(root, old);
    tx.add_member(root, new);
    tx.set_code_layout(
        root,
        SemanticTransactionCodeLayout::new(nodes, *original.metrics(), original.panel_geometry()),
    );
    let result = tx.apply(&mut store).unwrap();
    let new = result.resolve(new).unwrap();
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_node(old);
    tx.apply(&mut store).unwrap();
    assert!(store.node(root).is_some());
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_node(new);
    tx.apply(&mut store).unwrap();
    assert!(store.node(root).is_none());
}

#[test]
fn a_declaration_cannot_publish_a_dependency_removed_in_the_same_batch() {
    let mut store = SemanticStore::new();
    let before = snapshot(&store);
    let mut tx = SemanticMutationTransaction::new();
    let listing = stage_listing(&mut tx);
    tx.set_code_layout(listing.root, listing.layout(metrics()));
    tx.remove_node(listing.nodes.panel);
    assert!(tx.apply(&mut store).is_err());
    assert_eq!(snapshot(&store), before);

    let (root, layout) = admit(&mut store);
    let before = snapshot(&store);
    let mut tx = SemanticMutationTransaction::new();
    let new_owner = tx.create_node(SemanticNodeCreation::family());
    tx.set_code_layout(new_owner, layout.clone());
    tx.remove_node(layout.nodes().panel);
    assert!(tx.apply(&mut store).is_err());
    assert_eq!(snapshot(&store), before);
    assert!(store.node(root).is_some());
}

#[test]
fn guard_is_nonowning_and_remains_stale_after_panel_replacement_and_store_clone() {
    let mut store = SemanticStore::new();
    let (root, layout) = admit(&mut store);
    let original = layout.panel_geometry();
    let mut tx = SemanticMutationTransaction::new();
    tx.replace_content(layout.nodes().panel, StoredGeometry::Circle { radius: 1.0 });
    tx.apply(&mut store).unwrap();
    assert!(store.geometry_resources().get(original).is_none());
    assert_eq!(
        store
            .semantic_code_layout(root)
            .unwrap()
            .unwrap()
            .panel_geometry(),
        original
    );
    // Rehydrating/copying the guard is allowed; it never adopts replacement
    // geometry or pins the resource that the panel has ceased to own.
    let mut tx = SemanticMutationTransaction::new();
    let alias = tx.create_node(SemanticNodeCreation::family());
    tx.set_code_layout(alias, layout);
    tx.apply(&mut store).unwrap();
    assert_eq!(store.geometry_resources().len(), 0);
    let cloned = store.clone();
    let guard = cloned
        .semantic_code_layout(root)
        .unwrap()
        .unwrap()
        .panel_geometry();
    assert_ne!(guard.arena, original.arena);
    assert_eq!(guard.id, original.id);
    assert_eq!(guard.version, original.version);
    assert!(cloned.geometry_resources().get(guard).is_none());
}

#[test]
fn store_clone_remaps_panel_guard_and_shares_only_immutable_path_payload() {
    let mut store = SemanticStore::new();
    let (root, layout) = admit(&mut store);
    let cloned = store.clone();
    let copied = cloned.semantic_code_layout(root).unwrap().unwrap();
    assert_eq!(copied.nodes(), layout.nodes());
    assert_eq!(copied.metrics(), layout.metrics());
    assert_ne!(copied.panel_geometry().arena, layout.panel_geometry().arena);
    assert_eq!(
        cloned
            .semantic_object_state_checked(copied.nodes().panel)
            .unwrap()
            .content,
        SemanticObjectContent::Geometry(StoredGeometry::Resource(copied.panel_geometry()))
    );
    let GeometryResource::VectorPath(original) = store
        .geometry_resources()
        .get(layout.panel_geometry())
        .unwrap()
    else {
        panic!("expected ordinary path panel");
    };
    let GeometryResource::VectorPath(copy) = cloned
        .geometry_resources()
        .get(copied.panel_geometry())
        .unwrap()
    else {
        panic!("expected copied path panel");
    };
    assert!(std::sync::Arc::ptr_eq(original, copy));

    let before = snapshot(&store);
    let mut tx = SemanticMutationTransaction::new();
    tx.set_code_layout(
        root,
        SemanticCodeLayout::new(*layout.nodes(), *layout.metrics(), copied.panel_geometry()),
    );
    assert!(matches!(
        tx.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidCodeLayout { .. })
    ));
    assert_eq!(snapshot(&store), before);
}
