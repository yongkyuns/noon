mod layout;
use super::*;
use noon_core::{
    Bounds2D64, SemanticMutationTransaction, SemanticNodeCreation, SemanticObjectProperty,
    SemanticObjectState, SemanticStyle, SemanticTransform2_5D, StoredGeometry, TextResource,
};

type TextDependency = (
    noon_core::TextCompilationIdentity,
    TextResource,
    noon_core::FontResourceArena,
    noon_core::GeometryResourceArena,
);

#[derive(Clone, Copy)]
pub(super) struct TableShape {
    pub rows: usize,
    pub columns: usize,
}
pub(super) fn table_shape<T>(rows: &[Vec<T>]) -> Result<TableShape, TableAuthoringError> {
    let Some(first) = rows.first().filter(|row| !row.is_empty()) else {
        return Err(TableAuthoringError::EmptyTable);
    };
    for row in rows {
        if row.len() != first.len() {
            return Err(TableAuthoringError::RaggedRows {
                expected: first.len(),
                actual: row.len(),
            });
        }
    }
    Ok(TableShape {
        rows: rows.len(),
        columns: first.len(),
    })
}

fn normalize_labels<T>(labels: Option<Vec<T>>) -> Option<Vec<T>> {
    labels.filter(|labels| !labels.is_empty())
}

/// Preserve the pinned constructor's error precedence before publication.
fn validate_label_counts(
    shape: TableShape,
    rows: Option<usize>,
    columns: Option<usize>,
) -> Result<(), TableAuthoringError> {
    if let Some(actual) = rows.filter(|count| *count > shape.rows) {
        return Err(TableAuthoringError::InvalidLabelIndex {
            expected: shape.rows,
            actual,
        });
    }
    if let Some(actual) = columns.filter(|count| *count < shape.columns) {
        return Err(TableAuthoringError::InsufficientColumnLabels {
            expected: shape.columns,
            actual,
        });
    }
    if let Some(actual) = rows.filter(|count| *count < shape.rows) {
        return Err(TableAuthoringError::InvalidLabelIndex {
            expected: shape.rows,
            actual,
        });
    }
    if let Some(actual) = columns.filter(|count| *count > shape.columns) {
        return Err(TableAuthoringError::InvalidLabelIndex {
            expected: shape.columns,
            actual,
        });
    }
    Ok(())
}

pub(super) enum TablePublisher<'a, 'session> {
    Store(Rc<RefCell<noon_core::SemanticStore>>),
    Scene(&'a mut Scene),
    Live(&'a mut crate::LiveSession<'session>),
}
impl TablePublisher<'_, '_> {
    fn store(&self) -> Rc<RefCell<noon_core::SemanticStore>> {
        match self {
            Self::Store(store) => Rc::clone(store),
            Self::Scene(scene) => Rc::clone(scene.integration_store()),
            Self::Live(live) => Rc::clone(live.integration_store()),
        }
    }
    fn entry_state(&self, object: &Mobject) -> Result<SemanticObjectState, AuthoringError> {
        match self {
            Self::Store(_) => Ok(object.state()?),
            Self::Scene(scene) => Ok(scene.composite_entry_state(object)?),
            Self::Live(live) => Ok(live.composite_entry_state(object)?),
        }
    }
    fn composite_entries(
        &self,
        entries: &[MobjectTarget<'_>],
    ) -> Result<Vec<crate::composite_entry::CompositeEntry>, TableAuthoringError> {
        let store = self.store();
        crate::composite_entry::capture_entries_with_aliases(&store, entries, |object| {
            self.entry_state(object)
        })
        .map_err(Into::into)
    }
    fn publish_text<T>(
        self,
        operation: impl FnOnce(
            &mut noon_core::SemanticStore,
            &mut dyn FnMut(
                &mut noon_core::SemanticStore,
                SemanticMutationTransaction,
            )
                -> Result<noon_core::SemanticMutationTransactionResult, AuthoringError>,
        ) -> Result<T, TextAuthoringError>,
    ) -> Result<T, TableAuthoringError> {
        self.publish(|store, publish| Ok(operation(store, publish)))?
            .map_err(TableAuthoringError::Text)
    }
    fn publish<T>(
        self,
        operation: impl FnOnce(
            &mut noon_core::SemanticStore,
            &mut dyn FnMut(
                &mut noon_core::SemanticStore,
                SemanticMutationTransaction,
            )
                -> Result<noon_core::SemanticMutationTransactionResult, AuthoringError>,
        ) -> Result<T, AuthoringError>,
    ) -> Result<T, TableAuthoringError> {
        match self {
            Self::Store(store) => {
                let mut store = store.borrow_mut();
                let mut publish =
                    |store: &mut noon_core::SemanticStore, tx: SemanticMutationTransaction| {
                        tx.apply(store).map_err(AuthoringError::from)
                    };
                operation(&mut store, &mut publish).map_err(Into::into)
            }
            Self::Scene(scene) => scene
                .with_semantic_publication(operation)
                .map_err(Into::into),
            Self::Live(live) => {
                live.with_semantic_publication(operation)
                    .map_err(|error| match error {
                        crate::LiveSessionError::Authoring(error) => {
                            TableAuthoringError::Semantic(error)
                        }
                        crate::LiveSessionError::Publication(error) => {
                            TableAuthoringError::Semantic(AuthoringError::ExecutionPublication(
                                error,
                            ))
                        }
                        other => TableAuthoringError::LiveSession(other),
                    })
            }
        }
    }
}

/// A shaped text cell ready for the common resource batch and table publication.
///
/// Native text has no presentation baseline.  Math text retains the baseline
/// established at compilation, so later table placement cannot change its
/// receiver-owned font-size reference.
#[cfg(any(feature = "latex", feature = "native-text"))]
struct PreparedTextCell {
    dependency: TextDependency,
    transform: SemanticTransform2_5D,
    style: SemanticStyle,
    presentation_baseline: Option<noon_core::TextPresentationBaseline>,
}
#[cfg(feature = "latex")]
fn prepare_text(
    backend: &mut impl LatexBackend,
    source: String,
) -> Result<PreparedTextCell, TableAuthoringError> {
    let (identity, resource, fonts, geometry, transform, style, font_size) =
        crate::latex_authoring::prepare_math_tex(crate::MathTex::from_strings([source])?, backend)?
            .into_compiled_resource_parts_with_presentation();
    let presentation_baseline =
        crate::latex_authoring::latex_presentation_baseline(&resource, transform, font_size)?;
    Ok(PreparedTextCell {
        dependency: (identity, resource, fonts, geometry),
        transform,
        style,
        presentation_baseline: Some(presentation_baseline),
    })
}
#[cfg(feature = "native-text")]
fn prepare_native_text(source: String) -> Result<PreparedTextCell, TableAuthoringError> {
    let (identity, resource, fonts, geometry, transform, style) =
        crate::text_authoring::prepare_native_text(crate::Text::new(source))?
            .into_compiled_resource_parts_with_presentation();
    Ok(PreparedTextCell {
        dependency: (identity, resource, fonts, geometry),
        transform,
        style,
        presentation_baseline: None,
    })
}

fn line_style() -> SemanticStyle {
    SemanticStyle {
        fill: None,
        fill_opacity: 0.0,
        stroke: Some(noon_core::SemanticPaint::Solid(Color::WHITE)),
        stroke_opacity: 1.0,
        stroke_width: 0.04,
        background_stroke: Some(noon_core::SemanticPaint::Solid(Color::BLACK)),
        background_stroke_opacity: 1.0,
        background_stroke_width: 0.0,
        stroke_width_mode: noon_core::StrokeWidthMode::ScreenSpace,
        stroke_join: noon_core::StrokeJoin::Miter,
        stroke_cap: noon_core::StrokeCap::Butt,
        object_opacity: 1.0,
    }
}
fn table_line_style(options: TableOptions) -> SemanticStyle {
    SemanticStyle {
        stroke: Some(noon_core::SemanticPaint::Solid(options.line_stroke_color)),
        stroke_width: options.line_stroke_width,
        stroke_opacity: options.line_stroke_opacity,
        stroke_join: options.line_stroke_join,
        stroke_cap: options.line_stroke_cap,
        ..line_style()
    }
}

struct Publication {
    result: noon_core::SemanticMutationTransactionResult,
    root: noon_core::SemanticLocalNodeToken,
    entries: noon_core::SemanticLocalNodeToken,
    body_entries: noon_core::SemanticLocalNodeToken,
    painted_entries: noon_core::SemanticLocalNodeToken,
    lines: noon_core::SemanticLocalNodeToken,
    highlights: noon_core::SemanticLocalNodeToken,
    row_labels: Option<noon_core::SemanticLocalNodeToken>,
    column_labels: Option<noon_core::SemanticLocalNodeToken>,
    top_left_entry: Option<noon_core::SemanticLocalNodeToken>,
}
struct StagedTable {
    root: noon_core::SemanticLocalNodeToken,
    entries: noon_core::SemanticLocalNodeToken,
    body_entries: noon_core::SemanticLocalNodeToken,
    painted_entries: noon_core::SemanticLocalNodeToken,
    lines: noon_core::SemanticLocalNodeToken,
    highlights: noon_core::SemanticLocalNodeToken,
    row_labels: Option<noon_core::SemanticLocalNodeToken>,
    column_labels: Option<noon_core::SemanticLocalNodeToken>,
    top_left_entry: Option<noon_core::SemanticLocalNodeToken>,
}
impl StagedTable {
    fn published(self, result: noon_core::SemanticMutationTransactionResult) -> Publication {
        Publication {
            result,
            root: self.root,
            entries: self.entries,
            body_entries: self.body_entries,
            painted_entries: self.painted_entries,
            lines: self.lines,
            highlights: self.highlights,
            row_labels: self.row_labels,
            column_labels: self.column_labels,
            top_left_entry: self.top_left_entry,
        }
    }
}
fn make_table(
    store: Rc<RefCell<noon_core::SemanticStore>>,
    value: Publication,
) -> Result<Table, TableAuthoringError> {
    let resolve = |token| {
        value
            .result
            .resolve(token)
            .ok_or(TableAuthoringError::InvalidStructure)
    };
    Ok(Table {
        family: MobjectFamily::from_node(Rc::clone(&store), resolve(value.root)?)?,
        entry_family: MobjectFamily::from_node(Rc::clone(&store), resolve(value.entries)?)?,
        body_entries: MobjectFamily::from_node(Rc::clone(&store), resolve(value.body_entries)?)?,
        painted_entries: MobjectFamily::from_node(
            Rc::clone(&store),
            resolve(value.painted_entries)?,
        )?,
        line_family: MobjectFamily::from_node(Rc::clone(&store), resolve(value.lines)?)?,
        highlight_family: MobjectFamily::from_node(Rc::clone(&store), resolve(value.highlights)?)?,
        row_label_family: match value.row_labels {
            Some(token) => Some(MobjectFamily::from_node(
                Rc::clone(&store),
                resolve(token)?,
            )?),
            None => None,
        },
        column_label_family: match value.column_labels {
            Some(token) => Some(MobjectFamily::from_node(
                Rc::clone(&store),
                resolve(token)?,
            )?),
            None => None,
        },
        top_left_entry_family: match value.top_left_entry {
            Some(token) => Some(MobjectFamily::from_node(
                Rc::clone(&store),
                resolve(token)?,
            )?),
            None => None,
        },
    })
}
struct TableLabels<'a> {
    rows: Option<&'a [noon_core::SemanticNodeId]>,
    columns: Option<&'a [noon_core::SemanticNodeId]>,
    top_left_entry: Option<noon_core::SemanticNodeId>,
}
fn stage(
    tx: &mut SemanticMutationTransaction,
    entries: Vec<noon_core::SemanticTransactionNodeRef>,
    shape: TableShape,
    lines: Vec<SemanticObjectState>,
    labels: TableLabels<'_>,
    options: TableOptions,
    painted: Vec<noon_core::SemanticTransactionNodeRef>,
) -> Result<StagedTable, AuthoringError> {
    let entries_root = tx.create_node(SemanticNodeCreation::family());
    for group in entries.chunks(shape.columns) {
        let row = tx.create_node(SemanticNodeCreation::family());
        for node in super::queries::unique_last(group.to_vec()) {
            tx.add_member(row, node);
        }
        tx.add_member(entries_root, row);
    }
    let body_entries = tx.create_node(SemanticNodeCreation::family());
    for node in super::queries::unique_last(entries.clone()) {
        tx.add_member(body_entries, node);
    }
    let painted_entries = tx.create_node(SemanticNodeCreation::family());
    for node in painted {
        tx.add_member(painted_entries, node);
    }
    let lines_root = tx.create_node(SemanticNodeCreation::family());
    for state in lines {
        let node = tx.create_node(SemanticNodeCreation::object(state));
        tx.add_member(lines_root, node);
    }
    let highlights = tx.create_node(SemanticNodeCreation::family());
    let mut label_view = |items: Option<&[noon_core::SemanticNodeId]>| {
        items.map(|items| {
            let family = tx.create_node(SemanticNodeCreation::family());
            for item in super::queries::unique_last(items.to_vec()) {
                tx.add_member(family, item);
            }
            family
        })
    };
    let row_labels = label_view(labels.rows);
    let column_labels = label_view(labels.columns);
    let top_left_entry = if row_labels.is_some() && column_labels.is_some() {
        labels.top_left_entry.map(|item| {
            let family = tx.create_node(SemanticNodeCreation::family());
            tx.add_member(family, item);
            family
        })
    } else {
        None
    };
    let corner_placeholder =
        if row_labels.is_some() && column_labels.is_some() && labels.top_left_entry.is_none() {
            let path = tx
                .stage_geometry_path(crate::VectorPath::new())
                .map_err(AuthoringError::from)?;
            Some(tx.create_node(SemanticNodeCreation::pending_path_object(
                path,
                SemanticTransform2_5D::default(),
                SemanticStyle::default(),
                0.0,
                Default::default(),
            )))
        } else {
            None
        };
    let root = tx.create_node(SemanticNodeCreation::family());
    // Only the displayed flat entry projection paints. Named body/label views
    // remain ordinary shared aliases retained by the same table declaration.
    for member in [highlights, painted_entries, lines_root] {
        tx.add_member(root, member);
    }
    let structure = noon_core::SemanticTableStructure::new(
        entries
            .chunks(shape.columns)
            .map(|row| row.to_vec())
            .collect(),
        labels
            .rows
            .map(|items| items.iter().copied().map(Into::into).collect()),
        labels
            .columns
            .map(|items| items.iter().copied().map(Into::into).collect()),
        labels.top_left_entry.map(Into::into),
        corner_placeholder.map(Into::into),
    )
    .with_families(noon_core::SemanticTableFamilies {
        body_rows: entries_root.into(),
        body_entries: body_entries.into(),
        painted_entries: painted_entries.into(),
        row_labels: row_labels.map(Into::into),
        column_labels: column_labels.map(Into::into),
        corner: top_left_entry.map(Into::into),
    });
    tx.set_table_layout(
        root,
        noon_core::SemanticTransactionTableLayout::new(
            noon_core::SemanticTableLayout::new(
                options.h_buff,
                options.v_buff,
                options.label_buff,
                options.include_outer_lines,
            )
            .with_inner_lines(options.include_inner_lines)
            .with_line_style(
                options.line_stroke_color,
                options.line_stroke_width,
                options.line_stroke_opacity,
            )
            .with_line_geometry(options.line_stroke_join, options.line_stroke_cap)
            .with_cell_alignment(options.cell_alignment_x, options.cell_alignment_y),
            Some(structure),
        ),
    );
    Ok(StagedTable {
        root,
        entries: entries_root,
        body_entries,
        painted_entries,
        lines: lines_root,
        highlights,
        row_labels,
        column_labels,
        top_left_entry,
    })
}
fn commit_composite(
    publisher: TablePublisher<'_, '_>,
    entries: Vec<MobjectTarget<'_>>,
    shape: TableShape,
    rows: Option<Vec<MobjectTarget<'_>>>,
    columns: Option<Vec<MobjectTarget<'_>>>,
    top_left_entry: Option<MobjectTarget<'_>>,
    options: TableOptions,
) -> Result<Table, TableAuthoringError> {
    let rows = normalize_labels(rows);
    let columns = normalize_labels(columns);
    let options = options.validate()?;
    let store = publisher.store();
    for target in entries
        .iter()
        .chain(rows.iter().flatten())
        .chain(columns.iter().flatten())
        .chain(top_left_entry.iter())
    {
        target.require_store(&store)?;
    }
    validate_label_counts(
        shape,
        rows.as_ref().map(Vec::len),
        columns.as_ref().map(Vec::len),
    )?;

    let entry_count = entries.len();
    let row_count = rows.as_ref().map(Vec::len);
    let column_count = columns.as_ref().map(Vec::len);
    let mut targets = entries;
    targets.extend(rows.iter().flatten().copied());
    targets.extend(columns.iter().flatten().copied());
    targets.extend(top_left_entry);
    let captured = publisher.composite_entries(&targets)?;
    let body = captured[..entry_count]
        .iter()
        .map(layout::Entry::retained)
        .collect();
    let mut cursor = entry_count;
    let row_entries = row_count.map(|n| {
        let entries = captured[cursor..cursor + n]
            .iter()
            .map(layout::Entry::retained)
            .collect();
        cursor += n;
        entries
    });
    let column_entries = column_count.map(|n| {
        let entries = captured[cursor..cursor + n]
            .iter()
            .map(layout::Entry::retained)
            .collect();
        cursor += n;
        entries
    });
    let corner = top_left_entry.map(|_| layout::Entry::retained(&captured[cursor]));
    let published = publisher.publish(move |semantic, publish| {
        let mut tx = SemanticMutationTransaction::new();
        let value = layout::stage_layout(
            semantic,
            &mut tx,
            body,
            shape,
            layout::Labels {
                rows: row_entries,
                columns: column_entries,
                corner,
            },
            options,
        )?;
        Ok(value.published(publish(semantic, tx)?))
    })?;
    make_table(store, published)
}

pub(super) fn publish_existing_table(
    publisher: TablePublisher<'_, '_>,
    entries: Vec<Mobject>,
    shape: TableShape,
    rows: Option<Vec<Mobject>>,
    columns: Option<Vec<Mobject>>,
    options: TableOptions,
) -> Result<Table, TableAuthoringError> {
    let targets = entries.iter().map(Into::into).collect();
    let row_targets = rows
        .as_ref()
        .map(|items| items.iter().map(Into::into).collect());
    let column_targets = columns
        .as_ref()
        .map(|items| items.iter().map(Into::into).collect());
    commit_composite(
        publisher,
        targets,
        shape,
        row_targets,
        column_targets,
        None,
        options,
    )
}
pub(super) fn publish_target_table(
    publisher: TablePublisher<'_, '_>,
    entries: Vec<MobjectTarget<'_>>,
    shape: TableShape,
    rows: Option<Vec<Mobject>>,
    columns: Option<Vec<Mobject>>,
    options: TableOptions,
) -> Result<Table, TableAuthoringError> {
    let row_targets = rows
        .as_ref()
        .map(|items| items.iter().map(Into::into).collect());
    let column_targets = columns
        .as_ref()
        .map(|items| items.iter().map(Into::into).collect());
    commit_composite(
        publisher,
        entries,
        shape,
        row_targets,
        column_targets,
        None,
        options,
    )
}
pub(super) fn publish_target_table_with_composite_labels(
    publisher: TablePublisher<'_, '_>,
    entries: Vec<MobjectTarget<'_>>,
    shape: TableShape,
    rows: Option<Vec<MobjectTarget<'_>>>,
    columns: Option<Vec<MobjectTarget<'_>>>,
    options: TableOptions,
) -> Result<Table, TableAuthoringError> {
    commit_composite(publisher, entries, shape, rows, columns, None, options)
}
pub(super) fn publish_target_table_with_top_left_entry(
    publisher: TablePublisher<'_, '_>,
    entries: Vec<MobjectTarget<'_>>,
    shape: TableShape,
    rows: Option<Vec<Mobject>>,
    columns: Option<Vec<Mobject>>,
    top_left_entry: Option<Mobject>,
    options: TableOptions,
) -> Result<Table, TableAuthoringError> {
    let row_targets = rows
        .as_ref()
        .map(|items| items.iter().map(Into::into).collect());
    let column_targets = columns
        .as_ref()
        .map(|items| items.iter().map(Into::into).collect());
    let top_left_target = top_left_entry.as_ref().map(Into::into);
    commit_composite(
        publisher,
        entries,
        shape,
        row_targets,
        column_targets,
        top_left_target,
        options,
    )
}
pub(super) fn publish_target_table_with_composite_labels_and_top_left_entry(
    publisher: TablePublisher<'_, '_>,
    entries: Vec<MobjectTarget<'_>>,
    shape: TableShape,
    rows: Option<Vec<MobjectTarget<'_>>>,
    columns: Option<Vec<MobjectTarget<'_>>>,
    top_left_entry: Option<MobjectTarget<'_>>,
    options: TableOptions,
) -> Result<Table, TableAuthoringError> {
    commit_composite(
        publisher,
        entries,
        shape,
        rows,
        columns,
        top_left_entry,
        options,
    )
}
#[cfg(any(feature = "latex", feature = "native-text"))]
fn publish_prepared_text_table(
    publisher: TablePublisher<'_, '_>,
    prepared: Vec<PreparedTextCell>,
    shape: TableShape,
    rows: Option<Vec<MobjectTarget<'_>>>,
    columns: Option<Vec<MobjectTarget<'_>>>,
    top_left_entry: Option<MobjectTarget<'_>>,
    options: TableOptions,
) -> Result<Table, TableAuthoringError> {
    let rows = normalize_labels(rows);
    let columns = normalize_labels(columns);
    let options = options.validate()?;
    let store = publisher.store();
    for target in rows
        .iter()
        .flatten()
        .chain(columns.iter().flatten())
        .chain(top_left_entry.iter())
    {
        target.require_store(&store)?;
    }
    validate_label_counts(
        shape,
        rows.as_ref().map(Vec::len),
        columns.as_ref().map(Vec::len),
    )?;
    let row_count = rows.as_ref().map(Vec::len);
    let column_count = columns.as_ref().map(Vec::len);
    let mut label_targets = Vec::new();
    label_targets.extend(rows.iter().flatten().copied());
    label_targets.extend(columns.iter().flatten().copied());
    let top_left_index = top_left_entry.as_ref().map(|_| label_targets.len());
    label_targets.extend(top_left_entry.iter().copied());
    let captured = publisher.composite_entries(&label_targets)?;
    let mut cursor = 0;
    let row_entries = row_count.map(|n| {
        let items = captured[cursor..cursor + n]
            .iter()
            .map(layout::Entry::retained)
            .collect();
        cursor += n;
        items
    });
    let column_entries = column_count.map(|n| {
        let items = captured[cursor..cursor + n]
            .iter()
            .map(layout::Entry::retained)
            .collect();
        cursor += n;
        items
    });
    let corner = top_left_index.map(|index| layout::Entry::retained(&captured[index]));
    let dependencies = prepared
        .iter()
        .map(|item| item.dependency.clone())
        .collect();
    let published = publisher.publish_text(move |semantic, publish| {
        semantic.with_compiled_text_dependency_batch::<TextAuthoringError, _>(
            dependencies,
            |semantic, handles| {
                handles
                    .iter()
                    .map(|handle| {
                        semantic
                            .text_resources()
                            .get(*handle)
                            .cloned()
                            .ok_or(TextAuthoringError::MissingGeometryResource)
                    })
                    .collect()
            },
            |semantic, handles| {
                let mut tx = SemanticMutationTransaction::new();
                let body = prepared
                    .iter()
                    .zip(handles)
                    .map(|(item, handle)| {
                        let mut state = SemanticObjectState::new(*handle);
                        state.transform = item.transform.into();
                        state.style = item.style.clone();
                        if let Some(baseline) = item.presentation_baseline {
                            state.set_text_presentation_baseline(baseline);
                        }
                        let node = tx.create_node(SemanticNodeCreation::object(state.clone()));
                        layout::Entry::pending(node.into(), state)
                    })
                    .collect();
                let value = layout::stage_layout(
                    semantic,
                    &mut tx,
                    body,
                    shape,
                    layout::Labels {
                        rows: row_entries,
                        columns: column_entries,
                        corner,
                    },
                    options,
                )
                .map_err(TextAuthoringError::Semantic)?;
                Ok(value.published(publish(semantic, tx).map_err(TextAuthoringError::Semantic)?))
            },
        )
    })?;
    make_table(store, published)
}

#[cfg(feature = "latex")]
pub(super) fn publish_text_table(
    publisher: TablePublisher<'_, '_>,
    backend: &mut impl LatexBackend,
    values: Vec<String>,
    shape: TableShape,
    rows: Option<Vec<Mobject>>,
    columns: Option<Vec<Mobject>>,
    options: TableOptions,
) -> Result<Table, TableAuthoringError> {
    let options = options.validate()?;
    let prepared = values
        .into_iter()
        .map(|value| prepare_text(backend, value))
        .collect::<Result<Vec<_>, _>>()?;
    let row_targets = rows
        .as_ref()
        .map(|items| items.iter().map(Into::into).collect());
    let column_targets = columns
        .as_ref()
        .map(|items| items.iter().map(Into::into).collect());
    publish_prepared_text_table(
        publisher,
        prepared,
        shape,
        row_targets,
        column_targets,
        None,
        options,
    )
}

/// Publish ordinary `Table` strings through the native shaped-text pipeline.
///
/// A plain table deliberately has no LaTeX backend dependency: its cells keep
/// native text resources and all resource imports plus table topology commit
/// as one publication.
#[cfg(feature = "native-text")]
pub(super) fn publish_native_text_table(
    publisher: TablePublisher<'_, '_>,
    values: Vec<String>,
    shape: TableShape,
    rows: Option<Vec<Mobject>>,
    columns: Option<Vec<Mobject>>,
    options: TableOptions,
) -> Result<Table, TableAuthoringError> {
    let row_targets = rows
        .as_ref()
        .map(|items| items.iter().map(Into::into).collect());
    let column_targets = columns
        .as_ref()
        .map(|items| items.iter().map(Into::into).collect());
    publish_native_text_table_with_composite_labels(
        publisher,
        values,
        shape,
        row_targets,
        column_targets,
        None,
        options,
    )
}

#[cfg(feature = "native-text")]
pub(super) fn publish_native_text_table_with_composite_labels(
    publisher: TablePublisher<'_, '_>,
    values: Vec<String>,
    shape: TableShape,
    rows: Option<Vec<MobjectTarget<'_>>>,
    columns: Option<Vec<MobjectTarget<'_>>>,
    top_left_entry: Option<MobjectTarget<'_>>,
    options: TableOptions,
) -> Result<Table, TableAuthoringError> {
    let options = options.validate()?;
    let prepared = values
        .into_iter()
        .map(prepare_native_text)
        .collect::<Result<Vec<_>, _>>()?;
    publish_prepared_text_table(
        publisher,
        prepared,
        shape,
        rows,
        columns,
        top_left_entry,
        options,
    )
}
#[cfg(feature = "latex")]
#[allow(clippy::too_many_arguments)] // Numeric resource admission and topology publish one transaction.
pub(super) fn publish_numeric_table(
    publisher: TablePublisher<'_, '_>,
    backend: &mut impl LatexBackend,
    values: Vec<f64>,
    shape: TableShape,
    format: DecimalFormat,
    rows: Option<Vec<Mobject>>,
    columns: Option<Vec<Mobject>>,
    options: TableOptions,
) -> Result<Table, TableAuthoringError> {
    let rows = normalize_labels(rows);
    let columns = normalize_labels(columns);
    let options = options.validate()?;
    let prepared = values
        .into_iter()
        .map(|value| {
            crate::numeric_authoring::PreparedDecimalValue::prepare(
                backend,
                value,
                format.clone(),
                48.0,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let store = publisher.store();
    let row_count = rows.as_ref().map(Vec::len);
    let column_count = columns.as_ref().map(Vec::len);
    validate_label_counts(shape, row_count, column_count)?;
    let targets: Vec<_> = rows
        .iter()
        .flatten()
        .chain(columns.iter().flatten())
        .map(MobjectTarget::from)
        .collect();
    let captured = publisher.composite_entries(&targets)?;
    let mut cursor = 0;
    let row_entries = row_count.map(|n| {
        let items = captured[cursor..cursor + n]
            .iter()
            .map(layout::Entry::retained)
            .collect();
        cursor += n;
        items
    });
    let column_entries = column_count.map(|n| {
        captured[cursor..cursor + n]
            .iter()
            .map(layout::Entry::retained)
            .collect()
    });
    let published = publisher.publish_text(move |semantic, publish| {
        crate::numeric_authoring::PreparedDecimalValue::publish_batch::<TextAuthoringError, _>(
            semantic,
            prepared,
            |semantic, handles, prepared| {
                let mut tx = SemanticMutationTransaction::new();
                let body = prepared
                    .iter()
                    .zip(handles)
                    .map(|(item, handle)| {
                        let state = item.decimal_state(
                            semantic,
                            *handle,
                            SemanticTransform2_5D::default(),
                        )?;
                        let node = tx.create_node(SemanticNodeCreation::object(state.clone()));
                        Ok(layout::Entry::pending(node.into(), state))
                    })
                    .collect::<Result<Vec<_>, TextAuthoringError>>()?;
                let value = layout::stage_layout(
                    semantic,
                    &mut tx,
                    body,
                    shape,
                    layout::Labels {
                        rows: row_entries,
                        columns: column_entries,
                        corner: None,
                    },
                    options,
                )
                .map_err(TextAuthoringError::Semantic)?;
                Ok(value.published(publish(semantic, tx).map_err(TextAuthoringError::Semantic)?))
            },
        )
    })?;
    make_table(store, published)
}

pub(super) fn alias(
    store: &Rc<RefCell<noon_core::SemanticStore>>,
    items: &[TableEntry],
) -> Result<MobjectFamily, TableAuthoringError> {
    let targets: Vec<_> = items.iter().map(TableEntry::as_target).collect();
    Ok(MobjectFamily::create(Rc::clone(store), &targets)?)
}
fn selected_cell_bounds(
    publisher: &TablePublisher<'_, '_>,
    family: &MobjectFamily,
    options: TableOptions,
    row: usize,
    column: usize,
) -> Result<Bounds2D64, TableAuthoringError> {
    let store = family.integration_store();
    let grid = store
        .borrow()
        .semantic_table_layout(family.node_id())
        .map_err(AuthoringError::from)?
        .and_then(|layout| layout.shared_structure())
        .ok_or(TableAuthoringError::InvalidStructure)?;
    let row_nodes = super::queries::displayed_row(&grid, row)?;
    let column_nodes = super::queries::displayed_column(&grid, column)?;
    let bounds = |nodes: Vec<_>| -> Result<Option<Bounds2D64>, TableAuthoringError> {
        let mut result: Option<Bounds2D64> = None;
        for node in nodes {
            let handle = CompositeEntryHandle::from_node(Rc::clone(store), node)?;
            // Each entry is an observation. Repeated roots and overlapping
            // families must not be rejected by the admission overlap check.
            for entry in publisher.composite_entries(&[handle.as_target()])? {
                if let Some(next) = entry.boundary_bounds()? {
                    if let Some(bounds) = &mut result {
                        bounds.include(next.min_x, next.min_y);
                        bounds.include(next.max_x, next.max_y);
                    } else {
                        result = Some(next);
                    }
                }
            }
        }
        Ok(result)
    };
    // Manim's empty VMobject corner contributes no points to critical bounds.
    let (min_x, max_x) = bounds(column_nodes)?
        .map(|b| (b.min_x, b.max_x))
        .unwrap_or((0.0, 0.0));
    let (min_y, max_y) = bounds(row_nodes)?
        .map(|b| (b.min_y, b.max_y))
        .unwrap_or((0.0, 0.0));
    Ok(Bounds2D64 {
        min_x: min_x - options.h_buff * 0.5,
        max_x: max_x + options.h_buff * 0.5,
        min_y: min_y - options.v_buff * 0.5,
        max_y: max_y + options.v_buff * 0.5,
    })
}
pub(super) fn cell(
    family: &MobjectFamily,
    options: TableOptions,
    row: usize,
    column: usize,
) -> Result<Mobject, TableAuthoringError> {
    cell_in_publisher(
        TablePublisher::Store(Rc::clone(family.integration_store())),
        family,
        options,
        row,
        column,
    )
}
pub(super) fn cell_in_publisher(
    publisher: TablePublisher<'_, '_>,
    family: &MobjectFamily,
    options: TableOptions,
    row: usize,
    column: usize,
) -> Result<Mobject, TableAuthoringError> {
    let bounds = selected_cell_bounds(&publisher, family, options, row, column)?;
    let store = publisher.store();
    let result = publisher.publish(move |semantic, publish| {
        let mut transaction = SemanticMutationTransaction::new();
        let mut state = SemanticObjectState::new(StoredGeometry::Rectangle {
            size: noon_core::Vec2::new(bounds.width() as f32, bounds.height() as f32),
        });
        state.style = line_style();
        state.transform.translation.x = (bounds.min_x + bounds.max_x) * 0.5;
        state.transform.translation.y = (bounds.min_y + bounds.max_y) * 0.5;
        let object = transaction.create_node(SemanticNodeCreation::object(state));
        let result = publish(semantic, transaction)?;
        result
            .resolve(object)
            .ok_or(AuthoringError::UnresolvedCreatedNode(object))
    })?;
    Mobject::from_node(store, result).map_err(Into::into)
}
pub(super) fn highlight(
    family: &MobjectFamily,
    highlights: &MobjectFamily,
    options: TableOptions,
    row: usize,
    column: usize,
    color: Color,
    opacity: f64,
) -> Result<Mobject, TableAuthoringError> {
    highlight_in_publisher(
        TablePublisher::Store(Rc::clone(family.integration_store())),
        family,
        highlights,
        options,
        row,
        column,
        color,
        opacity,
    )
}

pub(super) fn highlighted_cell(
    family: &MobjectFamily,
    options: TableOptions,
    row: usize,
    column: usize,
    color: Color,
    opacity: f64,
) -> Result<Mobject, TableAuthoringError> {
    highlighted_cell_in_publisher(
        TablePublisher::Store(Rc::clone(family.integration_store())),
        family,
        options,
        row,
        column,
        color,
        opacity,
    )
}

pub(super) fn highlighted_cell_in_publisher(
    publisher: TablePublisher<'_, '_>,
    family: &MobjectFamily,
    options: TableOptions,
    row: usize,
    column: usize,
    color: Color,
    opacity: f64,
) -> Result<Mobject, TableAuthoringError> {
    highlight_with_membership(
        publisher, family, None, options, row, column, color, opacity,
    )
}

#[allow(clippy::too_many_arguments)] // One atomic rectangle creation plus retained highlight membership.
pub(super) fn highlight_in_publisher(
    publisher: TablePublisher<'_, '_>,
    family: &MobjectFamily,
    highlights: &MobjectFamily,
    options: TableOptions,
    row: usize,
    column: usize,
    color: Color,
    opacity: f64,
) -> Result<Mobject, TableAuthoringError> {
    highlight_with_membership(
        publisher,
        family,
        Some(highlights),
        options,
        row,
        column,
        color,
        opacity,
    )
}

#[allow(clippy::too_many_arguments)] // One atomic rectangle creation plus optional family membership.
fn highlight_with_membership(
    publisher: TablePublisher<'_, '_>,
    family: &MobjectFamily,
    highlights: Option<&MobjectFamily>,
    options: TableOptions,
    row: usize,
    column: usize,
    color: Color,
    opacity: f64,
) -> Result<Mobject, TableAuthoringError> {
    if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
        return Err(TableAuthoringError::InvalidOption {
            name: "highlight opacity",
            value: opacity,
        });
    }
    let bounds = selected_cell_bounds(&publisher, family, options, row, column)?;
    let style = SemanticStyle {
        fill: Some(noon_core::SemanticPaint::Solid(color)),
        fill_opacity: opacity,
        stroke: None,
        stroke_width: 0.0,
        ..Default::default()
    };
    if let Some(highlights) = highlights {
        highlights.validate()?;
        if !Rc::ptr_eq(family.integration_store(), highlights.integration_store()) {
            return Err(AuthoringError::ForeignStore.into());
        }
    }
    let store = publisher.store();
    let result = publisher.publish(move |semantic, publish| {
        let mut transaction = SemanticMutationTransaction::new();
        let mut state = SemanticObjectState::new(StoredGeometry::Rectangle {
            size: noon_core::Vec2::new(bounds.width() as f32, bounds.height() as f32),
        });
        state.style = style;
        state.transform.translation.x = (bounds.min_x + bounds.max_x) * 0.5;
        state.transform.translation.y = (bounds.min_y + bounds.max_y) * 0.5;
        let object = transaction.create_node(SemanticNodeCreation::object(state));
        if let Some(highlights) = highlights {
            transaction.add_member(highlights.node_id(), object);
            if let Some(first) = semantic
                .semantic_family_member_at_checked(highlights.node_id(), 0)
                .map_err(AuthoringError::from)?
            {
                transaction.reorder_member(highlights.node_id(), object, Some(first));
            }
        }
        let publication = publish(semantic, transaction)?;
        publication
            .resolve(object)
            .ok_or(AuthoringError::UnresolvedCreatedNode(object))
    })?;
    Mobject::from_node(store, result).map_err(Into::into)
}
