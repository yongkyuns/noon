use super::*;
use crate::semantic_mobject::transformed_rect_layout_bounds;
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

struct GridLayout {
    widths: Vec<f64>,
    heights: Vec<f64>,
    x_centers: Vec<f64>,
    y_centers: Vec<f64>,
    row_offset: usize,
    column_offset: usize,
    options: TableOptions,
}
struct GridLabels<'a> {
    row_count: Option<usize>,
    row_bounds: Option<&'a [Bounds2D64]>,
    column_count: Option<usize>,
    column_bounds: Option<&'a [Bounds2D64]>,
    top_left_bounds: Option<Bounds2D64>,
}
impl GridLayout {
    fn measure(
        entry_bounds: &[Bounds2D64],
        shape: TableShape,
        labels: GridLabels<'_>,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError> {
        let GridLabels {
            row_count,
            row_bounds,
            column_count,
            column_bounds,
            top_left_bounds,
        } = labels;
        if top_left_bounds.is_some() && (row_count.is_none() || column_count.is_none()) {
            return Err(TableAuthoringError::TopLeftRequiresLabels);
        }
        let row_offset = usize::from(column_count.is_some());
        let column_offset = usize::from(row_count.is_some());
        let mut widths: Vec<f64> = vec![0.0; shape.columns + column_offset];
        let mut heights: Vec<f64> = vec![0.0; shape.rows + row_offset];
        for (index, bounds) in entry_bounds.iter().enumerate() {
            widths[index % shape.columns + column_offset] =
                widths[index % shape.columns + column_offset].max(bounds.width());
            heights[index / shape.columns + row_offset] =
                heights[index / shape.columns + row_offset].max(bounds.height());
        }
        if let (Some(count), Some(bounds)) = (row_count, row_bounds) {
            if count != shape.rows {
                return Err(TableAuthoringError::InvalidLabels {
                    expected: shape.rows,
                    actual: count,
                });
            }
            for (index, bound) in bounds.iter().enumerate() {
                widths[0] = widths[0].max(bound.width());
                heights[index + row_offset] = heights[index + row_offset].max(bound.height());
            }
        }
        if let (Some(count), Some(bounds)) = (column_count, column_bounds) {
            if count != shape.columns {
                return Err(TableAuthoringError::InvalidLabels {
                    expected: shape.columns,
                    actual: count,
                });
            }
            for (index, bound) in bounds.iter().enumerate() {
                heights[0] = heights[0].max(bound.height());
                widths[index + column_offset] = widths[index + column_offset].max(bound.width());
            }
        }
        if let Some(bounds) = top_left_bounds {
            widths[0] = widths[0].max(bounds.width());
            heights[0] = heights[0].max(bounds.height());
        }
        let x_centers = Self::centers(&widths, options.h_buff, false);
        let y_centers = Self::centers(&heights, options.v_buff, true);
        Ok(Self {
            widths,
            heights,
            x_centers,
            y_centers,
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
        let xs = &self.x_centers;
        let ys = &self.y_centers;
        let min_x = xs[0] - self.widths[0] / 2.0 - self.options.h_buff / 2.0;
        let max_x =
            xs.last().unwrap() + self.widths.last().unwrap() / 2.0 + self.options.h_buff / 2.0;
        let max_y = ys[0] + self.heights[0] / 2.0 + self.options.v_buff / 2.0;
        let min_y =
            ys.last().unwrap() - self.heights.last().unwrap() / 2.0 - self.options.v_buff / 2.0;
        let mut output = Vec::new();
        for row in 0..=ys.len() {
            let is_outer = row == 0 || row == ys.len();
            if (is_outer && self.options.include_outer_lines)
                || (!is_outer && self.options.include_inner_lines)
            {
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
                s.style = table_line_style(self.options);
                output.push(s)
            }
        }
        for column in 0..=xs.len() {
            let is_outer = column == 0 || column == xs.len();
            if (is_outer && self.options.include_outer_lines)
                || (!is_outer && self.options.include_inner_lines)
            {
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
                s.style = table_line_style(self.options);
                output.push(s)
            }
        }
        output
    }
    /// Return the Manim-aligned center for an object's measured bounds in one
    /// cell. Positive and negative components select the corresponding edge;
    /// zero centers the object, matching Mobject.get_critical_point semantics.
    fn aligned_center(&self, bounds: Bounds2D64, row: usize, column: usize) -> (f64, f64) {
        let x_slack = (self.widths[column] - bounds.width()).max(0.0) * 0.5;
        let y_slack = (self.heights[row] - bounds.height()).max(0.0) * 0.5;
        let x_offset = if self.options.cell_alignment_x > 0.0 {
            x_slack
        } else if self.options.cell_alignment_x < 0.0 {
            -x_slack
        } else {
            0.0
        };
        let y_offset = if self.options.cell_alignment_y > 0.0 {
            y_slack
        } else if self.options.cell_alignment_y < 0.0 {
            -y_slack
        } else {
            0.0
        };
        (
            self.x_centers[column] + x_offset,
            self.y_centers[row] + y_offset,
        )
    }
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
    lines: noon_core::SemanticLocalNodeToken,
    highlights: noon_core::SemanticLocalNodeToken,
    row_labels: Option<noon_core::SemanticLocalNodeToken>,
    column_labels: Option<noon_core::SemanticLocalNodeToken>,
    top_left_entry: Option<noon_core::SemanticLocalNodeToken>,
}
struct StagedTable {
    root: noon_core::SemanticLocalNodeToken,
    entries: noon_core::SemanticLocalNodeToken,
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
) -> Result<StagedTable, AuthoringError> {
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
    let row_labels = labels.rows.map(|items| {
        let family = tx.create_node(SemanticNodeCreation::family());
        for item in items {
            tx.add_member(family, *item);
        }
        family
    });
    let column_labels = labels.columns.map(|items| {
        let family = tx.create_node(SemanticNodeCreation::family());
        for item in items {
            tx.add_member(family, *item);
        }
        family
    });
    let top_left_entry = labels
        .top_left_entry
        .filter(|_| row_labels.is_some() && column_labels.is_some())
        .map(|item| {
            let family = tx.create_node(SemanticNodeCreation::family());
            tx.add_member(family, item);
            family
        });
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
    // Highlights paint behind table cells and lines by authoritative family
    // order. Individual highlight rectangles retain ordinary z-index zero.
    for member in [highlights, entries_root, lines_root] {
        tx.add_member(root, member);
    }
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
            Some(noon_core::SemanticTableStructure::new(
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
            )),
        ),
    );
    if let Some(member) = row_labels {
        tx.add_member(root, member);
    }
    if let Some(member) = column_labels {
        tx.add_member(root, member);
    }
    if let Some(member) = top_left_entry {
        tx.add_member(root, member);
    }
    Ok(StagedTable {
        root,
        entries: entries_root,
        lines: lines_root,
        highlights,
        row_labels,
        column_labels,
        top_left_entry,
    })
}
#[cfg(feature = "latex")]
fn preflight(
    entries: &[Mobject],
    shape: TableShape,
    rows: Option<&[Mobject]>,
    columns: Option<&[Mobject]>,
    top_left_entry: Option<&Mobject>,
    store: &Rc<RefCell<noon_core::SemanticStore>>,
) -> Result<(), TableAuthoringError> {
    let mut seen = std::collections::BTreeSet::new();
    for object in entries
        .iter()
        .chain(rows.into_iter().flatten())
        .chain(columns.into_iter().flatten())
        .chain(top_left_entry)
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
#[cfg(feature = "latex")]
fn authored_bounds(objects: &[Mobject]) -> Result<Vec<Bounds2D64>, TableAuthoringError> {
    objects
        .iter()
        .map(|object| {
            object
                .layout_bounds()?
                .ok_or(TableAuthoringError::InvalidStructure)
        })
        .collect()
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
    if rows
        .as_ref()
        .is_some_and(|labels| labels.len() != shape.rows)
    {
        return Err(TableAuthoringError::InvalidLabels {
            expected: shape.rows,
            actual: rows.as_ref().unwrap().len(),
        });
    }
    if columns
        .as_ref()
        .is_some_and(|labels| labels.len() != shape.columns)
    {
        return Err(TableAuthoringError::InvalidLabels {
            expected: shape.columns,
            actual: columns.as_ref().unwrap().len(),
        });
    }

    let entry_count = entries.len();
    let row_count = rows.as_ref().map(Vec::len);
    let column_count = columns.as_ref().map(Vec::len);
    let mut all_targets = entries;
    all_targets.extend(rows.iter().flatten().copied());
    all_targets.extend(columns.iter().flatten().copied());
    let top_left_index = top_left_entry.as_ref().map(|_| all_targets.len());
    all_targets.extend(top_left_entry.iter().copied());

    // Capture all supplied roots together so overlapping entries/labels are
    // rejected and every family-valued label retains its complete leaf set.
    let captured = publisher.composite_entries(&all_targets)?;
    let entries = captured[..entry_count].to_vec();
    let mut cursor = entry_count;
    let row_entries = row_count.map(|count| {
        let result = captured[cursor..cursor + count].to_vec();
        cursor += count;
        result
    });
    let column_entries = column_count.map(|count| {
        let result = captured[cursor..cursor + count].to_vec();
        cursor += count;
        result
    });
    let top_left_entry = top_left_index.map(|index| captured[index].clone());
    let entry_bounds = entries
        .iter()
        .map(|entry| entry.bounds()?.ok_or(TableAuthoringError::InvalidStructure))
        .collect::<Result<Vec<_>, _>>()?;
    let row_bounds = row_entries
        .as_ref()
        .map(|items| {
            items
                .iter()
                .map(|entry| entry.bounds()?.ok_or(TableAuthoringError::InvalidStructure))
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let column_bounds = column_entries
        .as_ref()
        .map(|items| {
            items
                .iter()
                .map(|entry| entry.bounds()?.ok_or(TableAuthoringError::InvalidStructure))
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let top_left_bounds = top_left_entry
        .as_ref()
        .filter(|_| row_count.is_some() && column_count.is_some())
        .map(|entry| entry.bounds()?.ok_or(TableAuthoringError::InvalidStructure))
        .transpose()?;
    let layout = GridLayout::measure(
        &entry_bounds,
        shape,
        GridLabels {
            row_count,
            row_bounds: row_bounds.as_deref(),
            column_count,
            column_bounds: column_bounds.as_deref(),
            top_left_bounds,
        },
        options,
    )?;
    let mut translations = entries
        .iter()
        .zip(&entry_bounds)
        .enumerate()
        .flat_map(|(index, (entry, bounds))| {
            let (x, y) = layout.aligned_center(
                *bounds,
                index / shape.columns + layout.row_offset,
                index % shape.columns + layout.column_offset,
            );
            let dx = x - (bounds.min_x + bounds.max_x) * 0.5;
            let dy = y - (bounds.min_y + bounds.max_y) * 0.5;
            entry.leaves().iter().map(move |(leaf, state)| {
                let mut translation = state.transform.translation;
                translation.x += dx;
                translation.y += dy;
                (leaf.node_id(), translation)
            })
        })
        .collect::<Vec<_>>();
    if let (Some(labels), Some(bounds)) = (row_entries.as_deref(), row_bounds.as_deref()) {
        for (index, (label, bounds)) in labels.iter().zip(bounds).enumerate() {
            let (x, y) = layout.aligned_center(*bounds, index + layout.row_offset, 0);
            let dx = x - (bounds.min_x + bounds.max_x) * 0.5;
            let dy = y - (bounds.min_y + bounds.max_y) * 0.5;
            translations.extend(label.leaves().iter().map(|(leaf, state)| {
                let mut translation = state.transform.translation;
                translation.x += dx;
                translation.y += dy;
                (leaf.node_id(), translation)
            }));
        }
    }
    if let (Some(entry), Some(bounds)) = (top_left_entry.as_ref(), top_left_bounds) {
        let (x, y) = layout.aligned_center(bounds, 0, 0);
        let dx = x - (bounds.min_x + bounds.max_x) * 0.5;
        let dy = y - (bounds.min_y + bounds.max_y) * 0.5;
        translations.extend(entry.leaves().iter().map(|(leaf, state)| {
            let mut translation = state.transform.translation;
            translation.x += dx;
            translation.y += dy;
            (leaf.node_id(), translation)
        }));
    }
    if let (Some(labels), Some(bounds)) = (column_entries.as_deref(), column_bounds.as_deref()) {
        for (index, (label, bounds)) in labels.iter().zip(bounds).enumerate() {
            let (x, y) = layout.aligned_center(*bounds, 0, index + layout.column_offset);
            let dx = x - (bounds.min_x + bounds.max_x) * 0.5;
            let dy = y - (bounds.min_y + bounds.max_y) * 0.5;
            translations.extend(label.leaves().iter().map(|(leaf, state)| {
                let mut translation = state.transform.translation;
                translation.x += dx;
                translation.y += dy;
                (leaf.node_id(), translation)
            }));
        }
    }
    let row_label_roots = row_entries
        .as_ref()
        .map(|items| items.iter().map(|item| item.root()).collect::<Vec<_>>());
    let column_label_roots = column_entries
        .as_ref()
        .map(|items| items.iter().map(|item| item.root()).collect::<Vec<_>>());
    let top_left_root = top_left_entry.as_ref().map(|item| item.root());
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
            TableLabels {
                rows: row_label_roots.as_deref(),
                columns: column_label_roots.as_deref(),
                top_left_entry: top_left_root,
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
    if rows.as_ref().is_some_and(|items| items.len() != shape.rows) {
        return Err(TableAuthoringError::InvalidLabels {
            expected: shape.rows,
            actual: rows.as_ref().unwrap().len(),
        });
    }
    if columns
        .as_ref()
        .is_some_and(|items| items.len() != shape.columns)
    {
        return Err(TableAuthoringError::InvalidLabels {
            expected: shape.columns,
            actual: columns.as_ref().unwrap().len(),
        });
    }
    let row_count = rows.as_ref().map(Vec::len);
    let column_count = columns.as_ref().map(Vec::len);
    let mut label_targets = Vec::new();
    label_targets.extend(rows.iter().flatten().copied());
    label_targets.extend(columns.iter().flatten().copied());
    let top_left_index = top_left_entry.as_ref().map(|_| label_targets.len());
    label_targets.extend(top_left_entry.iter().copied());
    // Capture all supplied labels together so repeated roots or overlapping
    // families fail before text resources or table topology are published.
    let captured = publisher.composite_entries(&label_targets)?;
    let mut cursor = 0;
    let row_entries = row_count.map(|count| {
        let result = captured[cursor..cursor + count].to_vec();
        cursor += count;
        result
    });
    let column_entries = column_count.map(|count| {
        let result = captured[cursor..cursor + count].to_vec();
        cursor += count;
        result
    });
    let top_left_entry = top_left_index.map(|index| captured[index].clone());
    let mut transforms: Vec<_> = prepared.iter().map(|item| item.transform).collect();
    let entry_bounds: Vec<_> = prepared
        .iter()
        .zip(&transforms)
        .map(|(item, transform)| {
            transformed_rect_layout_bounds(item.dependency.1.bounds, *transform)
        })
        .collect();
    let row_bounds = row_entries
        .as_ref()
        .map(|items| {
            items
                .iter()
                .map(|entry| entry.bounds()?.ok_or(TableAuthoringError::InvalidStructure))
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let column_bounds = column_entries
        .as_ref()
        .map(|items| {
            items
                .iter()
                .map(|entry| entry.bounds()?.ok_or(TableAuthoringError::InvalidStructure))
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let top_left_bounds = top_left_entry
        .as_ref()
        .filter(|_| row_count.is_some() && column_count.is_some())
        .map(|entry| entry.bounds()?.ok_or(TableAuthoringError::InvalidStructure))
        .transpose()?;
    let layout = GridLayout::measure(
        &entry_bounds,
        shape,
        GridLabels {
            row_count,
            row_bounds: row_bounds.as_deref(),
            column_count,
            column_bounds: column_bounds.as_deref(),
            top_left_bounds,
        },
        options,
    )?;
    for (index, (bound, transform)) in entry_bounds.iter().zip(&mut transforms).enumerate() {
        let (x, y) = layout.aligned_center(
            *bound,
            index / shape.columns + layout.row_offset,
            index % shape.columns + layout.column_offset,
        );
        transform.translation.x += x - (bound.min_x + bound.max_x) * 0.5;
        transform.translation.y += y - (bound.min_y + bound.max_y) * 0.5;
    }
    let mut label_translations = Vec::new();
    if let (Some(labels), Some(bounds)) = (row_entries.as_deref(), row_bounds.as_deref()) {
        for (index, (label, bounds)) in labels.iter().zip(bounds).enumerate() {
            let (x, y) = layout.aligned_center(*bounds, index + layout.row_offset, 0);
            let dx = x - (bounds.min_x + bounds.max_x) * 0.5;
            let dy = y - (bounds.min_y + bounds.max_y) * 0.5;
            label_translations.extend(label.leaves().iter().map(|(leaf, state)| {
                let mut translation = state.transform.translation;
                translation.x += dx;
                translation.y += dy;
                (leaf.node_id(), translation)
            }));
        }
    }
    if let (Some(labels), Some(bounds)) = (column_entries.as_deref(), column_bounds.as_deref()) {
        for (index, (label, bounds)) in labels.iter().zip(bounds).enumerate() {
            let (x, y) = layout.aligned_center(*bounds, 0, index + layout.column_offset);
            let dx = x - (bounds.min_x + bounds.max_x) * 0.5;
            let dy = y - (bounds.min_y + bounds.max_y) * 0.5;
            label_translations.extend(label.leaves().iter().map(|(leaf, state)| {
                let mut translation = state.transform.translation;
                translation.x += dx;
                translation.y += dy;
                (leaf.node_id(), translation)
            }));
        }
    }
    if let (Some(entry), Some(bounds)) = (top_left_entry.as_ref(), top_left_bounds) {
        let (x, y) = layout.aligned_center(bounds, 0, 0);
        let dx = x - (bounds.min_x + bounds.max_x) * 0.5;
        let dy = y - (bounds.min_y + bounds.max_y) * 0.5;
        label_translations.extend(entry.leaves().iter().map(|(leaf, state)| {
            let mut translation = state.transform.translation;
            translation.x += dx;
            translation.y += dy;
            (leaf.node_id(), translation)
        }));
    }
    let row_roots = row_entries
        .as_ref()
        .map(|items| items.iter().map(|item| item.root()).collect::<Vec<_>>());
    let column_roots = column_entries
        .as_ref()
        .map(|items| items.iter().map(|item| item.root()).collect::<Vec<_>>());
    let top_left_root = top_left_entry.as_ref().map(|item| item.root());
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
                for (node, translation) in label_translations {
                    tx.set_property(node, SemanticObjectProperty::Translation, translation);
                }
                let leaves = prepared
                    .iter()
                    .zip(handles)
                    .zip(transforms)
                    .map(|((item, handle), transform)| {
                        let mut state = SemanticObjectState::new(*handle);
                        state.transform = transform.into();
                        state.style = item.style.clone();
                        if let Some(baseline) = item.presentation_baseline {
                            state.set_text_presentation_baseline(baseline);
                        }
                        Ok(tx.create_node(SemanticNodeCreation::object(state)).into())
                    })
                    .collect::<Result<Vec<_>, TextAuthoringError>>()?;
                let value = stage(
                    &mut tx,
                    leaves,
                    shape,
                    lines,
                    TableLabels {
                        rows: row_roots.as_deref(),
                        columns: column_roots.as_deref(),
                        top_left_entry: top_left_root,
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
    preflight(
        &[],
        shape,
        rows.as_deref(),
        columns.as_deref(),
        None,
        &store,
    )?;
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
                            .map(|resource| {
                                transformed_rect_layout_bounds(
                                    resource.bounds,
                                    SemanticTransform2_5D::default(),
                                )
                            })
                            .ok_or(TextAuthoringError::MissingGeometryResource)
                    })
                    .collect::<Result<_, _>>()?;
                let layout = GridLayout::measure(
                    &bounds,
                    shape,
                    GridLabels {
                        row_count: rows.as_ref().map(Vec::len),
                        row_bounds: row_bounds.as_deref(),
                        column_count: columns.as_ref().map(Vec::len),
                        column_bounds: column_bounds.as_deref(),
                        top_left_bounds: None,
                    },
                    options,
                )
                .expect("validated nonempty table shape and label counts produce a grid");
                let mut tx = SemanticMutationTransaction::new();
                let leaves = prepared
                    .iter()
                    .zip(handles)
                    .zip(bounds)
                    .enumerate()
                    .map(|(index, ((item, handle), bound))| {
                        let mut transform = SemanticTransform2_5D::default();
                        let (x, y) = layout.aligned_center(
                            bound,
                            index / shape.columns + layout.row_offset,
                            index % shape.columns + layout.column_offset,
                        );
                        transform.translation.x = x - (bound.min_x + bound.max_x) * 0.5;
                        transform.translation.y = y - (bound.min_y + bound.max_y) * 0.5;
                        let state = item.decimal_state(semantic, *handle, transform)?;
                        Ok(tx.create_node(SemanticNodeCreation::object(state)))
                    })
                    .collect::<Result<Vec<_>, TextAuthoringError>>()?
                    .into_iter()
                    .map(Into::into)
                    .collect();
                let row_roots = rows
                    .as_deref()
                    .map(|items| items.iter().map(Mobject::node_id).collect::<Vec<_>>());
                let column_roots = columns
                    .as_deref()
                    .map(|items| items.iter().map(Mobject::node_id).collect::<Vec<_>>());
                let value = stage(
                    &mut tx,
                    leaves,
                    shape,
                    layout.line_states(),
                    TableLabels {
                        rows: row_roots.as_deref(),
                        columns: column_roots.as_deref(),
                        top_left_entry: None,
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
                if let Some(next) = entry.bounds()? {
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
