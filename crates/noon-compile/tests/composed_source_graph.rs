//! The worker can borrow its installed and staged rows through this same view;
//! these tests deliberately have no CompiledScene or scene-cloning adapter.
use noon_compile::{
    validate_composed_sources, CompiledComposedSourceErrorKind as ErrorKind, ComposedDependencies,
    ComposedDependencyChanges, ComposedSourceGraphView, ComposedValidationStats,
};
use noon_core::{ComposedSource, ObjectId};
use std::{cell::Cell, collections::BTreeMap};

fn id(value: u64) -> ObjectId {
    ObjectId::new(value)
}
struct Row {
    source: Option<ComposedSource>,
    valid: bool,
    input_role: noon_core::EffectInputRole,
}
fn leaf() -> Row {
    Row {
        source: None,
        valid: true,
        input_role: noon_core::EffectInputRole::Painted,
    }
}
fn scope(members: &[u64], parent: Option<u64>) -> Row {
    Row {
        source: Some(ComposedSource::new(members.iter().copied().map(id), parent.map(id)).unwrap()),
        valid: true,
        input_role: noon_core::EffectInputRole::Painted,
    }
}
fn fixture() -> (
    BTreeMap<ObjectId, Row>,
    ComposedDependencies,
    BTreeMap<ObjectId, Option<usize>>,
) {
    let mut rows = (1..=20_000)
        .map(|value| (id(value), leaf()))
        .collect::<BTreeMap<_, _>>();
    rows.insert(id(90), scope(&[1, 2], Some(91)));
    rows.insert(id(91), scope(&[1, 2, 3], None));
    let mut dependencies = ComposedDependencies::default();
    for (owner, row) in &rows {
        if let Some(source) = &row.source {
            dependencies.replace(*owner, None, Some(source));
        }
    }
    let ranks = BTreeMap::from([(id(1), Some(0)), (id(2), Some(1)), (id(3), Some(2))]);
    (rows, dependencies, ranks)
}
struct View<'a> {
    rows: &'a BTreeMap<ObjectId, Row>,
    edits: &'a BTreeMap<ObjectId, Option<Row>>,
    ranks: &'a BTreeMap<ObjectId, Option<usize>>,
    dependencies: &'a ComposedDependencyChanges<'a>,
    reads: Cell<usize>,
}
impl View<'_> {
    fn row(&self, owner: ObjectId) -> Option<&Row> {
        self.reads.set(self.reads.get() + 1);
        self.edits
            .get(&owner)
            .map(Option::as_ref)
            .unwrap_or_else(|| self.rows.get(&owner))
    }
}
impl ComposedSourceGraphView for View<'_> {
    fn source(&self, owner: ObjectId) -> Option<&ComposedSource> {
        self.row(owner)?.source.as_ref()
    }
    fn owner_error(&self, owner: ObjectId) -> Option<ErrorKind> {
        (!self
            .row(owner)
            .is_some_and(|row| row.source.is_some() && row.valid))
        .then_some(ErrorKind::InvalidOwner)
    }
    fn input_valid(&self, input: ObjectId) -> Option<bool> {
        self.row(input).map(|row| row.source.is_none() && row.valid)
    }
    fn rank(&self, input: ObjectId) -> Option<usize> {
        self.ranks.get(&input).copied().flatten()
    }
    fn input_role(&self, owner: ObjectId) -> Option<noon_core::EffectInputRole> {
        self.row(owner).map(|row| row.input_role)
    }
    fn prefix_input_unsupported(&self, input: ObjectId) -> bool {
        self.row(input)
            .is_some_and(|row| row.source.is_none() && !row.valid)
    }
    fn prefix_conflict(&self) -> Option<(ObjectId, ObjectId)> {
        let changes = self.dependencies.prepare_prefix_admission(
            self,
            self.edits.keys().copied(),
            std::iter::empty(),
        );
        self.dependencies.prefix_conflict(&changes)
    }
    fn overlapping(&self, input: ObjectId) -> impl Iterator<Item = ObjectId> {
        self.dependencies.overlapping(input)
    }
}

#[test]
fn final_overlay_validates_new_generational_inputs_and_nested_sources_locally() {
    let (rows, dependencies, mut ranks) = fixture();
    let next = (7 << 32) | 1;
    let edits = BTreeMap::from([
        (id(1), None),
        (id(next), Some(leaf())),
        (id(90), Some(scope(&[next, 2], Some(91)))),
        (id(91), Some(scope(&[next, 2, 3], None))),
    ]);
    let mut staged = ComposedDependencyChanges::new(&dependencies);
    for owner in [90, 91] {
        staged.replace(
            id(owner),
            None,
            edits[&id(owner)].as_ref().unwrap().source.as_ref(),
        );
    }
    ranks.insert(id(1), None);
    ranks.insert(id(next), Some(0));
    let view = View {
        rows: &rows,
        edits: &edits,
        ranks: &ranks,
        dependencies: &staged,
        reads: Cell::new(0),
    };
    let affected = staged.affected_scopes(&view, [id(1), id(next)], [id(1), id(next)]);
    assert_eq!(affected.into_iter().collect::<Vec<_>>(), [id(90), id(91)]);
    let affected = staged.affected_scopes(&view, [id(next)], []);
    let stats = validate_composed_sources(&view, &affected).unwrap();
    assert_eq!(stats.scopes_validated, 2);
    assert_eq!(stats.members_examined, 5);
    assert!(
        view.reads.get() < 100,
        "local edit borrowed {} rows among {}",
        view.reads.get(),
        rows.len()
    );
    assert_eq!(
        dependencies.dependents(id(next)).count(),
        0,
        "preflight must not commit links"
    );
    assert_eq!(dependencies.dependents(id(1)).count(), 2);
}

#[test]
fn parameter_and_unrelated_rows_do_not_visit_input_lists() {
    let (rows, dependencies, ranks) = fixture();
    let edits = BTreeMap::new();
    let staged = ComposedDependencyChanges::new(&dependencies);
    let view = View {
        rows: &rows,
        edits: &edits,
        ranks: &ranks,
        dependencies: &staged,
        reads: Cell::new(0),
    };
    for (changed, painter) in [(vec![], vec![]), (vec![id(20_000)], vec![id(90), id(91)])] {
        let affected = staged.affected_scopes(&view, changed, painter);
        assert!(affected.is_empty());
        assert_eq!(
            validate_composed_sources(&view, &affected).unwrap(),
            ComposedValidationStats::default()
        );
    }
    assert_eq!(
        view.reads.get(),
        2,
        "only the unrelated changed row source and input role are queried"
    );
}

#[test]
fn parent_retirement_checks_unchanged_children_and_allows_corrected_retry() {
    let (rows, mut dependencies, ranks) = fixture();
    let mut edits = BTreeMap::from([(id(91), None)]);
    {
        let mut staged = ComposedDependencyChanges::new(&dependencies);
        staged.replace(id(91), rows[&id(91)].source.as_ref(), None);
        let view = View {
            rows: &rows,
            edits: &edits,
            ranks: &ranks,
            dependencies: &staged,
            reads: Cell::new(0),
        };
        let affected = staged.affected_scopes(&view, [], []);
        let error = validate_composed_sources(&view, &affected).unwrap_err();
        assert_eq!(error.owner, id(90));
        assert_eq!(error.kind, ErrorKind::InvalidParent(id(91)));
    }
    edits.insert(id(90), Some(scope(&[1, 2], None)));
    {
        let mut staged = ComposedDependencyChanges::new(&dependencies);
        for owner in [90, 91] {
            staged.replace(
                id(owner),
                rows[&id(owner)].source.as_ref(),
                edits[&id(owner)]
                    .as_ref()
                    .and_then(|row| row.source.as_ref()),
            );
        }
        let view = View {
            rows: &rows,
            edits: &edits,
            ranks: &ranks,
            dependencies: &staged,
            reads: Cell::new(0),
        };
        let affected = staged.affected_scopes(&view, [], []);
        assert_eq!(
            validate_composed_sources(&view, &affected)
                .unwrap()
                .scopes_validated,
            1
        );
    }
    for owner in [90, 91] {
        dependencies.replace(
            id(owner),
            rows[&id(owner)].source.as_ref(),
            edits[&id(owner)]
                .as_ref()
                .and_then(|row| row.source.as_ref()),
        );
    }
    assert_eq!(dependencies.owners().collect::<Vec<_>>(), [id(90)]);
    assert_eq!(dependencies.children(id(91)).count(), 0);
    assert_eq!(dependencies.dependents(id(3)).count(), 0);
}

#[test]
fn staged_input_order_generation_domain_and_scope_aliases_use_shared_typed_errors() {
    let (rows, dependencies, ranks) = fixture();
    let cases = [
        (scope(&[2, 1], Some(91)), ErrorKind::NonContiguousInput),
        (
            scope(&[(1 << 32) | 1, 2], Some(91)),
            ErrorKind::UnknownInput(id((1 << 32) | 1)),
        ),
        (scope(&[91], None), ErrorKind::NonDrawableInput(id(91))),
    ];
    for (row, expected) in cases {
        let edits = BTreeMap::from([(id(90), Some(row))]);
        let mut staged = ComposedDependencyChanges::new(&dependencies);
        staged.replace(
            id(90),
            rows[&id(90)].source.as_ref(),
            edits[&id(90)].as_ref().unwrap().source.as_ref(),
        );
        let mut ranks = ranks.clone();
        ranks.insert(id(91), Some(3));
        let view = View {
            rows: &rows,
            edits: &edits,
            ranks: &ranks,
            dependencies: &staged,
            reads: Cell::new(0),
        };
        let affected = staged.affected_scopes(&view, [], []);
        let error = validate_composed_sources(&view, &affected).unwrap_err();
        assert_eq!(error.owner, id(90));
        assert_eq!(error.kind, expected);
    }
    let edits = BTreeMap::from([(id(92), Some(scope(&[2, 3], Some(91))))]);
    let mut staged = ComposedDependencyChanges::new(&dependencies);
    staged.replace(
        id(92),
        None,
        edits[&id(92)].as_ref().unwrap().source.as_ref(),
    );
    let view = View {
        rows: &rows,
        edits: &edits,
        ranks: &ranks,
        dependencies: &staged,
        reads: Cell::new(0),
    };
    let affected = staged.affected_scopes(&view, [], []);
    let error = validate_composed_sources(&view, &affected).unwrap_err();
    assert_eq!(error.owner, id(92));
    assert_eq!(error.kind, ErrorKind::CrossingOrAliasedScopes);
}

#[test]
fn repeated_staging_replaces_final_reverse_links_and_removal_leaves_no_staged_owner() {
    let (rows, dependencies, _) = fixture();
    let next = scope(&[3], None);
    let mut staged = ComposedDependencyChanges::new(&dependencies);
    staged.replace(id(90), rows[&id(90)].source.as_ref(), next.source.as_ref());
    assert_eq!(staged.overlapping(id(1)).collect::<Vec<_>>(), [id(91)]);
    assert_eq!(
        staged.overlapping(id(3)).collect::<Vec<_>>(),
        [id(91), id(90)]
    );
    staged.replace(id(90), next.source.as_ref(), rows[&id(90)].source.as_ref());
    assert_eq!(
        staged.overlapping(id(1)).collect::<Vec<_>>(),
        [id(91), id(90)]
    );
    assert_eq!(staged.overlapping(id(3)).collect::<Vec<_>>(), [id(91)]);
    staged.replace(id(90), rows[&id(90)].source.as_ref(), None);
    assert_eq!(staged.overlapping(id(1)).collect::<Vec<_>>(), [id(91)]);
    assert_eq!(staged.overlapping(id(2)).collect::<Vec<_>>(), [id(91)]);
    assert_eq!(
        dependencies.children(id(91)).collect::<Vec<_>>(),
        [id(90)],
        "staging leaves installed links unchanged"
    );
}

#[test]
fn backdrop_role_changes_check_only_the_actual_isolation_closure_and_reject_missing_anchors() {
    use noon_core::EffectInputRole;
    let (rows, dependencies, mut ranks) = fixture();
    let mut input = leaf();
    input.input_role = EffectInputRole::PainterPrefix;
    let mut edits = BTreeMap::from([(id(1), Some(input))]);
    let staged = ComposedDependencyChanges::new(&dependencies);
    {
        let view = View {
            rows: &rows,
            edits: &edits,
            ranks: &ranks,
            dependencies: &staged,
            reads: Cell::new(0),
        };
        let affected = staged.affected_scopes(&view, [id(1)], []);
        let error = validate_composed_sources(&view, &affected).unwrap_err();
        assert!(matches!(error.kind, ErrorKind::NestedBackdrop(_)));
        assert!(view.reads.get() < 100);
    }
    let mut input = leaf();
    input.input_role = EffectInputRole::PainterPrefix;
    edits.clear();
    edits.insert(id(20_000), Some(input));
    // An existing row is insufficient: capture needs an actual painter anchor.
    {
        let view = View {
            rows: &rows,
            edits: &edits,
            ranks: &ranks,
            dependencies: &staged,
            reads: Cell::new(0),
        };
        let affected = staged.affected_scopes(&view, [id(20_000)], []);
        assert_eq!(
            validate_composed_sources(&view, &affected)
                .unwrap_err()
                .kind,
            ErrorKind::InvalidBackdropSource
        );
    }
    ranks.insert(id(20_000), Some(20_000));
    let view = View {
        rows: &rows,
        edits: &edits,
        ranks: &ranks,
        dependencies: &staged,
        reads: Cell::new(0),
    };
    let affected = staged.affected_scopes(&view, [id(20_000)], []);
    assert_eq!(
        validate_composed_sources(&view, &affected).unwrap(),
        ComposedValidationStats::default()
    );
    assert!(
        view.reads.get() < 20,
        "root prefix role check read {} of {} rows",
        view.reads.get(),
        rows.len()
    );
}

#[test]
fn borrowed_prefix_capability_validation_uses_only_the_staged_rows_and_canonical_ranks() {
    let (rows, dependencies, mut ranks) = fixture();
    let mut input = leaf();
    input.valid = false;
    let mut glass = leaf();
    glass.input_role = noon_core::EffectInputRole::PainterPrefix;
    let edits = BTreeMap::from([(id(1000), Some(input)), (id(20_000), Some(glass))]);
    let staged = ComposedDependencyChanges::new(&dependencies);
    ranks.insert(id(1000), Some(42));
    ranks.insert(id(20_000), Some(19_999));
    {
        let view = View {
            rows: &rows,
            edits: &edits,
            ranks: &ranks,
            dependencies: &staged,
            reads: Cell::new(0),
        };
        let affected = staged.affected_scopes(&view, [id(1000), id(20_000)], []);
        let error = validate_composed_sources(&view, &affected).unwrap_err();
        assert_eq!(error.owner, id(20_000));
        assert_eq!(error.kind, ErrorKind::UnsupportedBackdropInput(id(1000)));
        assert!(
            view.reads.get() < 100,
            "borrowed {} of {} rows",
            view.reads.get(),
            rows.len()
        );
    }
    ranks.insert(id(1000), Some(20_001));
    let view = View {
        rows: &rows,
        edits: &edits,
        ranks: &ranks,
        dependencies: &staged,
        reads: Cell::new(0),
    };
    let affected = staged.affected_scopes(&view, [id(1000), id(20_000)], []);
    validate_composed_sources(&view, &affected).unwrap();
    assert!(view.reads.get() < 100);
}
