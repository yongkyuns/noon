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
        crate::composite_entry::capture_entries_with(&store, entries, |object| {
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

struct PreparedText {
    dependency: TextDependency,
    transform: SemanticTransform2_5D,
    style: SemanticStyle,
    font_size: f64,
}
#[cfg(feature = "native-text")]
struct PreparedNativeText {
    dependency: TextDependency,
    transform: SemanticTransform2_5D,
    style: SemanticStyle,
}
fn prepare_text(
    backend: &mut impl LatexBackend,
    source: String,
) -> Result<PreparedText, TableAuthoringError> {
    let (identity, resource, fonts, geometry, transform, style, font_size) =
        crate::latex_authoring::prepare_math_tex(crate::MathTex::from_strings([source])?, backend)?
            .into_compiled_resource_parts_with_presentation();
    Ok(PreparedText {
        dependency: (identity, resource, fonts, geometry),
        transform,
        style,
        font_size,
    })
}
#[cfg(feature = "native-text")]
fn prepare_native_text(source: String) -> Result<PreparedNativeText, TableAuthoringError> {
    let (identity, resource, fonts, geometry, transform, style) =
        crate::text_authoring::prepare_native_text(crate::Text::new(source))?
            .into_compiled_resource_parts_with_presentation();
    Ok(PreparedNativeText {
        dependency: (identity, resource, fonts, geometry),
        transform,
        style,
    })
}
fn text_bounds(resource: &TextResource, transform: SemanticTransform2_5D) -> Bounds2D64 {
    let mut result = Bounds2D64::point(0.0, 0.0);
    for (index, point) in [
        resource.bounds.min,
        noon_core::Vec2::new(resource.bounds.min.x, resource.bounds.max.y),
        noon_core::Vec2::new(resource.bounds.max.x, resource.bounds.min.y),
        resource.bounds.max,
    ]
    .into_iter()
    .enumerate()
    {
        let x = f64::from(point.x) * transform.scale.x;
        let y = f64::from(point.y) * transform.scale.y;
        let (sin, cos) = transform.rotation_z.sin_cos();
        let (x, y) = (
            x * cos - y * sin + transform.translation.x,
            x * sin + y * cos + transform.translation.y,
        );
        if index == 0 {
            result = Bounds2D64::point(x, y);
        } else {
            result.include(x, y);
        }
    }
    result
}

struct GridLayout {
    widths: Vec<f64>,
    heights: Vec<f64>,
    row_offset: usize,
    column_offset: usize,
    options: TableOptions,
}
impl GridLayout {
    fn measure(
        entry_bounds: &[Bounds2D64],
        shape: TableShape,
        row_labels: Option<&[Mobject]>,
        row_bounds: Option<&[Bounds2D64]>,
        column_labels: Option<&[Mobject]>,
        column_bounds: Option<&[Bounds2D64]>,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError> {
        let row_offset = usize::from(column_labels.is_some());
        let column_offset = usize::from(row_labels.is_some());
        let mut widths: Vec<f64> = vec![0.0; shape.columns + column_offset];
        let mut heights: Vec<f64> = vec![0.0; shape.rows + row_offset];
        for (index, bounds) in entry_bounds.iter().enumerate() {
            widths[index % shape.columns + column_offset] =
                widths[index % shape.columns + column_offset].max(bounds.width());
            heights[index / shape.columns + row_offset] =
                heights[index / shape.columns + row_offset].max(bounds.height());
        }
        if let (Some(labels), Some(bounds)) = (row_labels, row_bounds) {
            if labels.len() != shape.rows {
                return Err(TableAuthoringError::InvalidLabels {
                    expected: shape.rows,
                    actual: labels.len(),
                });
            }
            for (index, bound) in bounds.iter().enumerate() {
                widths[0] = widths[0].max(bound.width());
                heights[index + row_offset] = heights[index + row_offset].max(bound.height());
            }
        }
        if let (Some(labels), Some(bounds)) = (column_labels, column_bounds) {
            if labels.len() != shape.columns {
                return Err(TableAuthoringError::InvalidLabels {
                    expected: shape.columns,
                    actual: labels.len(),
                });
            }
            for (index, bound) in bounds.iter().enumerate() {
                heights[0] = heights[0].max(bound.height());
                widths[index + column_offset] = widths[index + column_offset].max(bound.width());
            }
        }
        Ok(Self {
            widths,
            heights,
            row_offset,
            column_offset,
            options,
        })
    }
    fn centers(values: &[f64], gap: f64, inverted: bool) -> Vec<f64> {
        let total = values.iter().sum::<f64>() + gap * values.len().saturating_sub(1) as f64;
        let mut cursor = if inverted { total / 2.0 } else { -total / 2.0 };
        values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let value = if inverted {
                    cursor - *v / 2.0
                } else {
                    cursor + *v / 2.0
                };
                if inverted {
                    cursor -= *v + if i + 1 == values.len() { 0.0 } else { gap }
                } else {
                    cursor += *v + if i + 1 == values.len() { 0.0 } else { gap }
                };
                value
            })
            .collect()
    }
    fn line_states(&self) -> Vec<SemanticObjectState> {
        let xs = Self::centers(&self.widths, self.options.h_buff, false);
        let ys = Self::centers(&self.heights, self.options.v_buff, true);
        let min_x = xs[0] - self.widths[0] / 2.0 - self.options.h_buff / 2.0;
        let max_x =
            xs.last().unwrap() + self.widths.last().unwrap() / 2.0 + self.options.h_buff / 2.0;
        let max_y = ys[0] + self.heights[0] / 2.0 + self.options.v_buff / 2.0;
        let min_y =
            ys.last().unwrap() - self.heights.last().unwrap() / 2.0 - self.options.v_buff / 2.0;
        let mut output = Vec::new();
        for row in 0..=ys.len() {
            if self.options.include_outer_lines || (row > 0 && row < ys.len()) {
                let y = if row == 0 {
                    max_y
                } else if row == ys.len() {
                    min_y
                } else {
                    ys[row - 1] - self.heights[row - 1] / 2.0 - self.options.v_buff / 2.0
                };
                let mut s = SemanticObjectState::new(noon_core::StoredGeometry::Line {
                    start: noon_core::Vec2::new(min_x as f32, y as f32),
                    end: noon_core::Vec2::new(max_x as f32, y as f32),
                });
                s.style = line_style();
                output.push(s)
            }
        }
        for column in 0..=xs.len() {
            if self.options.include_outer_lines || (column > 0 && column < xs.len()) {
                let x = if column == 0 {
                    min_x
                } else if column == xs.len() {
                    max_x
                } else {
                    xs[column - 1] + self.widths[column - 1] / 2.0 + self.options.h_buff / 2.0
                };
                let mut s = SemanticObjectState::new(noon_core::StoredGeometry::Line {
                    start: noon_core::Vec2::new(x as f32, min_y as f32),
                    end: noon_core::Vec2::new(x as f32, max_y as f32),
                });
                s.style = line_style();
                output.push(s)
            }
        }
        output
    }
}
fn line_style() -> SemanticStyle {
    SemanticStyle {
        fill: None,
        fill_opacity: 0.0,
        stroke: Some(noon_core::SemanticPaint::Solid(Color::WHITE)),
        stroke_opacity: 1.0,
        stroke_width: 0.04,
        stroke_width_mode: noon_core::StrokeWidthMode::ScreenSpace,
        stroke_join: noon_core::StrokeJoin::Miter,
        stroke_cap: noon_core::StrokeCap::Butt,
        object_opacity: 1.0,
    }
}

struct Publication {
    result: noon_core::SemanticMutationTransactionResult,
    root: noon_core::SemanticLocalNodeToken,
    entries: noon_core::SemanticLocalNodeToken,
    lines: noon_core::SemanticLocalNodeToken,
    highlights: noon_core::SemanticLocalNodeToken,
    row_labels: Option<noon_core::SemanticLocalNodeToken>,
    column_labels: Option<noon_core::SemanticLocalNodeToken>,
}
struct StagedTable {
    root: noon_core::SemanticLocalNodeToken,
    entries: noon_core::SemanticLocalNodeToken,
    lines: noon_core::SemanticLocalNodeToken,
    highlights: noon_core::SemanticLocalNodeToken,
    row_labels: Option<noon_core::SemanticLocalNodeToken>,
    column_labels: Option<noon_core::SemanticLocalNodeToken>,
}
impl StagedTable {
    fn published(self, result: noon_core::SemanticMutationTransactionResult) -> Publication {
        Publication {
            result,
            root: self.root,
            entries: self.entries,
            lines: self.lines,
            highlights: self.highlights,
            row_labels: self.row_labels,
            column_labels: self.column_labels,
        }
    }
}
fn make_table(
    store: Rc<RefCell<noon_core::SemanticStore>>,
    value: Publication,
    options: TableOptions,
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
        options,
    })
}
fn stage(
    tx: &mut SemanticMutationTransaction,
    entries: Vec<noon_core::SemanticTransactionNodeRef>,
    shape: TableShape,
    lines: Vec<SemanticObjectState>,
    rows: Option<&[Mobject]>,
    columns: Option<&[Mobject]>,
) -> StagedTable {
    let entries_root = tx.create_node(SemanticNodeCreation::family());
    for group in entries.chunks(shape.columns) {
        let row = tx.create_node(SemanticNodeCreation::family());
        for node in group {
            tx.add_member(row, *node);
        }
        tx.add_member(entries_root, row);
    }
    let lines_root = tx.create_node(SemanticNodeCreation::family());
    for state in lines {
        let node = tx.create_node(SemanticNodeCreation::object(state));
        tx.add_member(lines_root, node);
    }
    let highlights = tx.create_node(SemanticNodeCreation::family());
    let row_labels = rows.map(|items| {
        let family = tx.create_node(SemanticNodeCreation::family());
        for item in items {
            tx.add_member(family, item.node_id());
        }
        family
    });
    let column_labels = columns.map(|items| {
        let family = tx.create_node(SemanticNodeCreation::family());
        for item in items {
            tx.add_member(family, item.node_id());
        }
        family
    });
    let root = tx.create_node(SemanticNodeCreation::family());
    for member in [entries_root, lines_root, highlights] {
        tx.add_member(root, member);
    }
    if let Some(member) = row_labels {
        tx.add_member(root, member);
    }
    if let Some(member) = column_labels {
        tx.add_member(root, member);
    }
    StagedTable {
        root,
        entries: entries_root,
        lines: lines_root,
        highlights,
        row_labels,
        column_labels,
    }
}
fn preflight(
    entries: &[Mobject],
    shape: TableShape,
    rows: Option<&[Mobject]>,
    columns: Option<&[Mobject]>,
    store: &Rc<RefCell<noon_core::SemanticStore>>,
) -> Result<(), TableAuthoringError> {
    let mut seen = std::collections::BTreeSet::new();
    for object in entries
        .iter()
        .chain(rows.into_iter().flatten())
        .chain(columns.into_iter().flatten())
    {
        if !Rc::ptr_eq(object.integration_store(), store) {
            return Err(AuthoringError::ForeignStore.into());
        }
        object.validate()?;
        if !seen.insert(object.node_id()) {
            return Err(TableAuthoringError::DuplicateEntry);
        }
    }
    if rows.is_some_and(|items| items.len() != shape.rows) {
        return Err(TableAuthoringError::InvalidLabels {
            expected: shape.rows,
            actual: rows.unwrap().len(),
        });
    }
    if columns.is_some_and(|items| items.len() != shape.columns) {
        return Err(TableAuthoringError::InvalidLabels {
            expected: shape.columns,
            actual: columns.unwrap().len(),
        });
    }
    Ok(())
}
fn authored_bounds(objects: &[Mobject]) -> Result<Vec<Bounds2D64>, TableAuthoringError> {
    objects
        .iter()
        .map(|object| {
            object
                .layout_bounds()?
                .ok_or(TableAuthoringError::InvalidStructure)
                .map_err(Into::into)
        })
        .collect()
}
fn bounds_at_states(
    objects: &[Mobject],
    states: &[SemanticObjectState],
) -> Result<Vec<Bounds2D64>, TableAuthoringError> {
    objects
        .iter()
        .zip(states)
        .map(|(object, state)| {
            crate::semantic_mobject::layout_for_content(
                &object.integration_store().borrow(),
                state.content,
                state.transform,
            )?
            .ok_or(TableAuthoringError::InvalidStructure)
        })
        .collect()
}
fn commit_composite(
    publisher: TablePublisher<'_, '_>,
    entries: Vec<MobjectTarget<'_>>,
    shape: TableShape,
    rows: Option<Vec<Mobject>>,
    columns: Option<Vec<Mobject>>,
    options: TableOptions,
) -> Result<Table, TableAuthoringError> {
    let options = options.validate()?;
    let store = publisher.store();
    preflight(&[], shape, rows.as_deref(), columns.as_deref(), &store)?;
    let entries = publisher.composite_entries(&entries)?;
    let entry_bounds = entries
        .iter()
        .map(|entry| entry.bounds()?.ok_or(TableAuthoringError::InvalidStructure))
        .collect::<Result<Vec<_>, _>>()?;
    for label in rows.iter().flatten().chain(columns.iter().flatten()) {
        if entries
            .iter()
            .flat_map(|entry| entry.leaves())
            .any(|(leaf, _)| leaf.node_id() == label.node_id())
        {
            return Err(TableAuthoringError::DuplicateEntry);
        }
    }
    let row_states: Option<Vec<SemanticObjectState>> = rows
        .as_deref()
        .map(|items| {
            items
                .iter()
                .map(|entry| publisher.entry_state(entry))
                .collect()
        })
        .transpose()?;
    let column_states: Option<Vec<SemanticObjectState>> = columns
        .as_deref()
        .map(|items| {
            items
                .iter()
                .map(|entry| publisher.entry_state(entry))
                .collect()
        })
        .transpose()?;
    let row_bounds = rows
        .as_deref()
        .zip(row_states.as_deref())
        .map(|(items, states)| bounds_at_states(items, states))
        .transpose()?;
    let column_bounds = columns
        .as_deref()
        .zip(column_states.as_deref())
        .map(|(items, states)| bounds_at_states(items, states))
        .transpose()?;
    let layout = GridLayout::measure(
        &entry_bounds,
        shape,
        rows.as_deref(),
        row_bounds.as_deref(),
        columns.as_deref(),
        column_bounds.as_deref(),
        options,
    )?;
    let xs = GridLayout::centers(&layout.widths, options.h_buff, false);
    let ys = GridLayout::centers(&layout.heights, options.v_buff, true);
    let mut translations = entries
        .iter()
        .zip(&entry_bounds)
        .enumerate()
        .flat_map(|(index, (entry, bounds))| {
            let dx = xs[index % shape.columns + layout.column_offset]
                - (bounds.min_x + bounds.max_x) * 0.5;
            let dy =
                ys[index / shape.columns + layout.row_offset] - (bounds.min_y + bounds.max_y) * 0.5;
            entry.leaves().iter().map(move |(leaf, state)| {
                let mut translation = state.transform.translation;
                translation.x += dx;
                translation.y += dy;
                (leaf.node_id(), translation)
            })
        })
        .collect::<Vec<_>>();
    if let (Some(labels), Some(bounds), Some(states)) = (
        rows.as_deref(),
        row_bounds.as_deref(),
        row_states.as_deref(),
    ) {
        translations.extend(labels.iter().zip(bounds).zip(states).enumerate().map(
            |(index, ((label, bounds), state))| {
                let mut translation = state.transform.translation;
                translation.x += xs[0] - (bounds.min_x + bounds.max_x) * 0.5;
                translation.y +=
                    ys[index + layout.row_offset] - (bounds.min_y + bounds.max_y) * 0.5;
                (label.node_id(), translation)
            },
        ));
    }
    if let (Some(labels), Some(bounds), Some(states)) = (
        columns.as_deref(),
        column_bounds.as_deref(),
        column_states.as_deref(),
    ) {
        translations.extend(labels.iter().zip(bounds).zip(states).enumerate().map(
            |(index, ((label, bounds), state))| {
                let mut translation = state.transform.translation;
                translation.x +=
                    xs[index + layout.column_offset] - (bounds.min_x + bounds.max_x) * 0.5;
                translation.y += ys[0] - (bounds.min_y + bounds.max_y) * 0.5;
                (label.node_id(), translation)
            },
        ));
    }
    let lines = layout.line_states();
    let published = publisher.publish(move |semantic, publish| {
        let mut tx = SemanticMutationTransaction::new();
        for (node, translation) in translations {
            tx.set_property(node, SemanticObjectProperty::Translation, translation);
        }
        let value = stage(
            &mut tx,
            entries.iter().map(|item| item.root().into()).collect(),
            shape,
            lines,
            rows.as_deref(),
            columns.as_deref(),
        );
        Ok(value.published(publish(semantic, tx)?))
    })?;
    make_table(store, published, options)
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
    commit_composite(publisher, targets, shape, rows, columns, options)
}
pub(super) fn publish_target_table(
    publisher: TablePublisher<'_, '_>,
    entries: Vec<MobjectTarget<'_>>,
    shape: TableShape,
    rows: Option<Vec<Mobject>>,
    columns: Option<Vec<Mobject>>,
    options: TableOptions,
) -> Result<Table, TableAuthoringError> {
    commit_composite(publisher, entries, shape, rows, columns, options)
}
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
    let mut transforms: Vec<_> = prepared.iter().map(|item| item.transform).collect();
    let resources: Vec<_> = prepared.iter().map(|item| &item.dependency.1).collect();
    let entry_bounds: Vec<_> = resources
        .iter()
        .zip(&transforms)
        .map(|(r, t)| text_bounds(r, *t))
        .collect();
    let store = publisher.store();
    preflight(&[], shape, rows.as_deref(), columns.as_deref(), &store)?;
    let row_bounds = rows.as_deref().map(authored_bounds).transpose()?;
    let column_bounds = columns.as_deref().map(authored_bounds).transpose()?;
    let layout = GridLayout::measure(
        &entry_bounds,
        shape,
        rows.as_deref(),
        row_bounds.as_deref(),
        columns.as_deref(),
        column_bounds.as_deref(),
        options,
    )?;
    let xs = GridLayout::centers(&layout.widths, options.h_buff, false);
    let ys = GridLayout::centers(&layout.heights, options.v_buff, true);
    for (index, (bound, transform)) in entry_bounds.iter().zip(&mut transforms).enumerate() {
        transform.translation.x +=
            xs[index % shape.columns + layout.column_offset] - (bound.min_x + bound.max_x) * 0.5;
        transform.translation.y +=
            ys[index / shape.columns + layout.row_offset] - (bound.min_y + bound.max_y) * 0.5;
    }
    let dependencies = prepared
        .iter()
        .map(|item| item.dependency.clone())
        .collect();
    let lines = layout.line_states();
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
                let leaves = prepared
                    .iter()
                    .zip(handles)
                    .zip(transforms)
                    .map(|((item, handle), transform)| {
                        let mut state = SemanticObjectState::new(*handle);
                        state.transform = transform;
                        state.style = item.style.clone();
                        state.set_text_presentation_baseline(
                            crate::latex_authoring::latex_presentation_baseline(
                                &item.dependency.1,
                                transform,
                                item.font_size,
                            )?,
                        );
                        Ok(tx.create_node(SemanticNodeCreation::object(state)).into())
                    })
                    .collect::<Result<Vec<_>, TextAuthoringError>>()?;
                let value = stage(
                    &mut tx,
                    leaves,
                    shape,
                    lines,
                    rows.as_deref(),
                    columns.as_deref(),
                );
                Ok(value.published(publish(semantic, tx).map_err(TextAuthoringError::Semantic)?))
            },
        )
    })?;
    make_table(store, published, options)
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
    let options = options.validate()?;
    let prepared = values
        .into_iter()
        .map(prepare_native_text)
        .collect::<Result<Vec<_>, _>>()?;
    let mut transforms: Vec<_> = prepared.iter().map(|item| item.transform).collect();
    let entry_bounds: Vec<_> = prepared
        .iter()
        .zip(&transforms)
        .map(|(item, transform)| text_bounds(&item.dependency.1, *transform))
        .collect();
    let store = publisher.store();
    preflight(&[], shape, rows.as_deref(), columns.as_deref(), &store)?;
    let row_bounds = rows.as_deref().map(authored_bounds).transpose()?;
    let column_bounds = columns.as_deref().map(authored_bounds).transpose()?;
    let layout = GridLayout::measure(
        &entry_bounds,
        shape,
        rows.as_deref(),
        row_bounds.as_deref(),
        columns.as_deref(),
        column_bounds.as_deref(),
        options,
    )?;
    let xs = GridLayout::centers(&layout.widths, options.h_buff, false);
    let ys = GridLayout::centers(&layout.heights, options.v_buff, true);
    for (index, (bound, transform)) in entry_bounds.iter().zip(&mut transforms).enumerate() {
        transform.translation.x +=
            xs[index % shape.columns + layout.column_offset] - (bound.min_x + bound.max_x) * 0.5;
        transform.translation.y +=
            ys[index / shape.columns + layout.row_offset] - (bound.min_y + bound.max_y) * 0.5;
    }
    let dependencies = prepared
        .iter()
        .map(|item| item.dependency.clone())
        .collect();
    let lines = layout.line_states();
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
                let leaves = prepared
                    .iter()
                    .zip(handles)
                    .zip(transforms)
                    .map(|((item, handle), transform)| {
                        tx.create_node(SemanticNodeCreation::object(
                            crate::text_authoring::semantic_text_state(
                                *handle,
                                transform,
                                item.style.clone(),
                            ),
                        ))
                    })
                    .map(Into::into)
                    .collect();
                let value = stage(
                    &mut tx,
                    leaves,
                    shape,
                    lines,
                    rows.as_deref(),
                    columns.as_deref(),
                );
                Ok(value.published(publish(semantic, tx).map_err(TextAuthoringError::Semantic)?))
            },
        )
    })?;
    make_table(store, published, options)
}
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
    preflight(&[], shape, rows.as_deref(), columns.as_deref(), &store)?;
    let row_bounds = rows.as_deref().map(authored_bounds).transpose()?;
    let column_bounds = columns.as_deref().map(authored_bounds).transpose()?;
    let published = publisher.publish_text(move |semantic, publish| {
        crate::numeric_authoring::PreparedDecimalValue::publish_batch::<TextAuthoringError, _>(
            semantic,
            prepared,
            |semantic, handles, prepared| {
                let bounds: Vec<_> = handles
                    .iter()
                    .map(|handle| {
                        semantic
                            .text_resources()
                            .get(*handle)
                            .map(|resource| text_bounds(resource, SemanticTransform2_5D::default()))
                            .ok_or(TextAuthoringError::MissingGeometryResource)
                    })
                    .collect::<Result<_, _>>()?;
                let layout = GridLayout::measure(
                    &bounds,
                    shape,
                    rows.as_deref(),
                    row_bounds.as_deref(),
                    columns.as_deref(),
                    column_bounds.as_deref(),
                    options,
                )
                .expect("validated nonempty table shape and label counts produce a grid");
                let xs = GridLayout::centers(&layout.widths, options.h_buff, false);
                let ys = GridLayout::centers(&layout.heights, options.v_buff, true);
                let mut tx = SemanticMutationTransaction::new();
                let leaves = prepared
                    .iter()
                    .zip(handles)
                    .zip(bounds)
                    .enumerate()
                    .map(|(index, ((item, handle), bound))| {
                        let mut transform = SemanticTransform2_5D::default();
                        transform.translation.x = xs[index % shape.columns + layout.column_offset]
                            - (bound.min_x + bound.max_x) * 0.5;
                        transform.translation.y = ys[index / shape.columns + layout.row_offset]
                            - (bound.min_y + bound.max_y) * 0.5;
                        let state = item.decimal_state(semantic, *handle, transform)?;
                        Ok(tx.create_node(SemanticNodeCreation::object(state)))
                    })
                    .collect::<Result<Vec<_>, TextAuthoringError>>()?
                    .into_iter()
                    .map(Into::into)
                    .collect();
                let value = stage(
                    &mut tx,
                    leaves,
                    shape,
                    layout.line_states(),
                    rows.as_deref(),
                    columns.as_deref(),
                );
                Ok(value.published(publish(semantic, tx).map_err(TextAuthoringError::Semantic)?))
            },
        )
    })?;
    make_table(store, published, options)
}

pub(super) fn shape_from_family(
    family: &MobjectFamily,
) -> Result<(usize, usize), TableAuthoringError> {
    let rows = rows(family)?;
    let columns = rows.first().map_or(0, Vec::len);
    if columns == 0 || rows.iter().any(|row| row.len() != columns) {
        return Err(TableAuthoringError::InvalidStructure);
    }
    Ok((rows.len(), columns))
}
pub(super) fn entries(family: &MobjectFamily) -> Result<Vec<TableEntry>, TableAuthoringError> {
    Ok(rows(family)?.into_iter().flatten().collect())
}
pub(super) fn rows(family: &MobjectFamily) -> Result<Vec<Vec<TableEntry>>, TableAuthoringError> {
    let store = Rc::clone(family.integration_store());
    family.validate()?;
    let row_nodes = store
        .borrow()
        .semantic_family_members_checked(family.node_id())
        .map_err(AuthoringError::from)?;
    row_nodes
        .into_iter()
        .map(|row| {
            store
                .borrow()
                .semantic_family_members_checked(row)
                .map_err(AuthoringError::from)?
                .into_iter()
                .map(|node| {
                    crate::CompositeEntryHandle::from_node(Rc::clone(&store), node)
                        .map_err(Into::into)
                })
                .collect()
        })
        .collect()
}
pub(super) fn columns(family: &MobjectFamily) -> Result<Vec<Vec<TableEntry>>, TableAuthoringError> {
    let rows = rows(family)?;
    let columns = shape_from_rows(&rows)?;
    Ok((0..columns)
        .map(|column| rows.iter().map(|row| row[column].clone()).collect())
        .collect())
}
fn shape_from_rows<T>(rows: &[Vec<T>]) -> Result<usize, TableAuthoringError> {
    let Some(first) = rows.first().filter(|row| !row.is_empty()) else {
        return Err(TableAuthoringError::InvalidStructure);
    };
    if rows.iter().any(|row| row.len() != first.len()) {
        return Err(TableAuthoringError::InvalidStructure);
    }
    Ok(first.len())
}
fn alias(
    store: &Rc<RefCell<noon_core::SemanticStore>>,
    items: &[TableEntry],
) -> Result<MobjectFamily, TableAuthoringError> {
    let targets: Vec<_> = items.iter().map(TableEntry::as_target).collect();
    Ok(MobjectFamily::create(Rc::clone(store), &targets)?)
}
pub(super) fn row_families(
    family: &MobjectFamily,
) -> Result<Vec<MobjectFamily>, TableAuthoringError> {
    let store = Rc::clone(family.integration_store());
    rows(family)?
        .iter()
        .map(|items| alias(&store, items))
        .collect()
}
pub(super) fn column_families(
    family: &MobjectFamily,
) -> Result<Vec<MobjectFamily>, TableAuthoringError> {
    let store = Rc::clone(family.integration_store());
    columns(family)?
        .iter()
        .map(|items| alias(&store, items))
        .collect()
}
fn retained_cell_bounds(
    bounds: &[Bounds2D64],
    shape: TableShape,
    options: TableOptions,
    row: usize,
    column: usize,
) -> Result<Bounds2D64, TableAuthoringError> {
    if row >= shape.rows || column >= shape.columns {
        return Err(TableAuthoringError::InvalidStructure);
    }
    let column_bounds = (0..shape.rows).map(|r| bounds[r * shape.columns + column]);
    let row_bounds = (0..shape.columns).map(|c| bounds[row * shape.columns + c]);
    let (min_x, max_x) = column_bounds.fold((f64::INFINITY, f64::NEG_INFINITY), |value, bound| {
        (value.0.min(bound.min_x), value.1.max(bound.max_x))
    });
    let (min_y, max_y) = row_bounds.fold((f64::INFINITY, f64::NEG_INFINITY), |value, bound| {
        (value.0.min(bound.min_y), value.1.max(bound.max_y))
    });
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
    let values = rows(family)?;
    let shape = TableShape {
        rows: values.len(),
        columns: shape_from_rows(&values)?,
    };
    let targets: Vec<_> = values.iter().flatten().map(TableEntry::as_target).collect();
    let entries = publisher.composite_entries(&targets)?;
    let bounds = entries
        .iter()
        .map(|entry| entry.bounds()?.ok_or(TableAuthoringError::InvalidStructure))
        .collect::<Result<Vec<_>, TableAuthoringError>>()?;
    let bounds = retained_cell_bounds(&bounds, shape, options, row, column)?;
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
    let values = rows(family)?;
    let shape = TableShape {
        rows: values.len(),
        columns: shape_from_rows(&values)?,
    };
    let targets: Vec<_> = values.iter().flatten().map(TableEntry::as_target).collect();
    let entries = publisher.composite_entries(&targets)?;
    let bounds = entries
        .iter()
        .map(|entry| entry.bounds()?.ok_or(TableAuthoringError::InvalidStructure))
        .collect::<Result<Vec<_>, TableAuthoringError>>()?;
    let bounds = retained_cell_bounds(&bounds, shape, options, row, column)?;
    let mut style = SemanticStyle::default();
    style.fill = Some(noon_core::SemanticPaint::Solid(color));
    style.fill_opacity = opacity;
    style.stroke = None;
    style.stroke_width = 0.0;
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
        state.set_z_index(-1.0);
        let object = transaction.create_node(SemanticNodeCreation::object(state));
        if let Some(highlights) = highlights {
            transaction.add_member(highlights.node_id(), object);
        }
        let publication = publish(semantic, transaction)?;
        publication
            .resolve(object)
            .ok_or(AuthoringError::UnresolvedCreatedNode(object))
    })?;
    Mobject::from_node(store, result).map_err(Into::into)
}
