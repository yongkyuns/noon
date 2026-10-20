//! Retained Manim-compatible table families.
//!
//! Tables are ordinary semantic families.  Their entries, labels, grid lines,
//! and highlights keep normal [`Mobject`] identities; this module only owns the
//! admission, layout and topology convention.

mod admission;
#[cfg(all(test, feature = "diagnostics"))]
mod alias_tests;
mod queries;

use crate::{
    AuthoringError, Color, CompositeEntryHandle, Mobject, MobjectFamily, MobjectTarget, Scene,
    StrokeCap, StrokeJoin, TextAuthoringError,
};
#[cfg(feature = "latex")]
use crate::{DecimalFormat, LatexBackend, NumericAuthoringError};
use std::{cell::RefCell, ops::Deref, rc::Rc};

use admission::{
    publish_existing_table, publish_target_table, publish_target_table_with_composite_labels,
    publish_target_table_with_composite_labels_and_top_left_entry,
    publish_target_table_with_top_left_entry, table_shape, TablePublisher,
};
#[cfg(feature = "native-text")]
use admission::{publish_native_text_table, publish_native_text_table_with_composite_labels};

#[cfg(feature = "latex")]
use admission::{publish_numeric_table, publish_text_table};

pub const DEFAULT_TABLE_H_BUFF: f64 = 1.3;
pub const DEFAULT_TABLE_V_BUFF: f64 = 0.8;
pub const DEFAULT_TABLE_LABEL_BUFF: f64 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TableOptions {
    pub h_buff: f64,
    pub v_buff: f64,
    pub include_outer_lines: bool,
    pub include_inner_lines: bool,
    pub label_buff: f64,
    pub line_stroke_color: Color,
    /// Screen-space width; the Python facade maps ManimCE's width by 0.01.
    pub line_stroke_width: f64,
    /// Stroke opacity in the inclusive range [0, 1].
    pub line_stroke_opacity: f64,
    pub line_stroke_join: StrokeJoin,
    pub line_stroke_cap: StrokeCap,
    /// Manim arrange-in-grid cell alignment directions; magnitudes follow
    /// Mobject critical-point semantics and therefore affect only the sign.
    pub cell_alignment_x: f64,
    pub cell_alignment_y: f64,
}

impl Default for TableOptions {
    fn default() -> Self {
        Self {
            h_buff: DEFAULT_TABLE_H_BUFF,
            v_buff: DEFAULT_TABLE_V_BUFF,
            include_outer_lines: false,
            include_inner_lines: true,
            label_buff: DEFAULT_TABLE_LABEL_BUFF,
            line_stroke_color: Color::WHITE,
            line_stroke_width: 0.04,
            line_stroke_opacity: 1.0,
            line_stroke_join: StrokeJoin::Miter,
            line_stroke_cap: StrokeCap::Butt,
            cell_alignment_x: 0.0,
            cell_alignment_y: 0.0,
        }
    }
}

impl TableOptions {
    pub(crate) fn validate(self) -> Result<Self, TableAuthoringError> {
        for (name, value) in [
            ("h_buff", self.h_buff),
            ("v_buff", self.v_buff),
            ("label_buff", self.label_buff),
            ("line_config.stroke_width", self.line_stroke_width),
            ("line_config.stroke_opacity", self.line_stroke_opacity),
            (
                "line_config.color.red",
                f64::from(self.line_stroke_color.red),
            ),
            (
                "line_config.color.green",
                f64::from(self.line_stroke_color.green),
            ),
            (
                "line_config.color.blue",
                f64::from(self.line_stroke_color.blue),
            ),
            (
                "line_config.color.alpha",
                f64::from(self.line_stroke_color.alpha),
            ),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(TableAuthoringError::InvalidOption { name, value });
            }
        }
        for (name, value) in [
            (
                "arrange_in_grid_config.cell_alignment.x",
                self.cell_alignment_x,
            ),
            (
                "arrange_in_grid_config.cell_alignment.y",
                self.cell_alignment_y,
            ),
        ] {
            if !value.is_finite() {
                return Err(TableAuthoringError::InvalidOption { name, value });
            }
        }
        if self.line_stroke_opacity > 1.0
            || self.line_stroke_color.red > 1.0
            || self.line_stroke_color.green > 1.0
            || self.line_stroke_color.blue > 1.0
            || self.line_stroke_color.alpha > 1.0
        {
            return Err(TableAuthoringError::InvalidOption {
                name: "line_config opacity or color channel",
                value: self
                    .line_stroke_opacity
                    .max(f64::from(self.line_stroke_color.red))
                    .max(f64::from(self.line_stroke_color.green))
                    .max(f64::from(self.line_stroke_color.blue))
                    .max(f64::from(self.line_stroke_color.alpha)),
            });
        }
        Ok(self)
    }
}

#[derive(Debug)]
pub enum TableAuthoringError {
    EmptyTable,
    RaggedRows {
        expected: usize,
        actual: usize,
    },
    DuplicateEntry,
    InvalidLabels {
        expected: usize,
        actual: usize,
    },
    /// A row label mismatch or oversized column list, matching Manim's list lookup.
    InvalidLabelIndex {
        expected: usize,
        actual: usize,
    },
    /// A nonempty short column list cannot fit the displayed grid.
    InsufficientColumnLabels {
        expected: usize,
        actual: usize,
    },
    TopLeftRequiresLabels,
    InvalidOption {
        name: &'static str,
        value: f64,
    },
    InvalidStructure,
    InvalidEntryIndex,
    Text(TextAuthoringError),
    #[cfg(feature = "latex")]
    Numeric(NumericAuthoringError),
    Semantic(AuthoringError),
    LiveSession(crate::LiveSessionError),
}
impl std::fmt::Display for TableAuthoringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyTable => f.write_str("a Table requires at least one non-empty row"),
            Self::RaggedRows { expected, actual } => {
                write!(f, "table row has {actual} entries; expected {expected}")
            }
            Self::DuplicateEntry => f.write_str("a MobjectTable entry may occur only once"),
            Self::InvalidLabels { expected, actual } => {
                write!(f, "table label count is {actual}; expected {expected}")
            }
            Self::InvalidLabelIndex { .. } => f.write_str("list index out of range"),
            Self::InsufficientColumnLabels { .. } => {
                f.write_str("Too few rows and columns to fit all submobjetcs.")
            }
            Self::TopLeftRequiresLabels => {
                f.write_str("top_left_entry requires both row_labels and column_labels")
            }
            Self::InvalidOption { name, value } => write!(f, "invalid table {name}: {value}"),
            Self::InvalidEntryIndex => f.write_str("range object index out of range"),
            Self::InvalidStructure => f.write_str("semantic family is not a valid Table"),
            Self::Text(error) => error.fmt(f),
            #[cfg(feature = "latex")]
            Self::Numeric(error) => error.fmt(f),
            Self::Semantic(error) => error.fmt(f),
            Self::LiveSession(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for TableAuthoringError {}
impl From<AuthoringError> for TableAuthoringError {
    fn from(value: AuthoringError) -> Self {
        Self::Semantic(value)
    }
}
impl From<TextAuthoringError> for TableAuthoringError {
    fn from(value: TextAuthoringError) -> Self {
        Self::Text(value)
    }
}
#[cfg(feature = "latex")]
impl From<NumericAuthoringError> for TableAuthoringError {
    fn from(value: NumericAuthoringError) -> Self {
        Self::Numeric(value)
    }
}

/// A table root and the normal semantic families it owns.
#[derive(Clone, Debug)]
pub struct Table {
    family: MobjectFamily,
    entry_family: MobjectFamily,
    body_entries: MobjectFamily,
    painted_entries: MobjectFamily,
    line_family: MobjectFamily,
    highlight_family: MobjectFamily,
    row_label_family: Option<MobjectFamily>,
    column_label_family: Option<MobjectFamily>,
    top_left_entry_family: Option<MobjectFamily>,
}
/// One retained Table entry root. This common display handle preserves family
/// identity while layout operates on ordered descendant leaves.
pub type TableEntry = CompositeEntryHandle;
#[cfg(feature = "latex")]
#[derive(Clone, Debug)]
pub struct MathTable(Table);
#[cfg(feature = "latex")]
#[derive(Clone, Debug)]
pub struct IntegerTable(Table);
#[cfg(feature = "latex")]
#[derive(Clone, Debug)]
pub struct DecimalTable(Table);
#[derive(Clone, Debug)]
pub struct MobjectTable(Table);

impl Table {
    /// Rehydrate a retained table from its durable semantic-family topology.
    pub fn from_family(family: MobjectFamily) -> Result<Self, TableAuthoringError> {
        family.validate()?;
        let store = Rc::clone(family.integration_store());
        let members = store
            .borrow()
            .semantic_family_members_checked(family.node_id())
            .map_err(AuthoringError::from)?;
        let [highlights, painted_entries, lines] = members.as_slice() else {
            return Err(TableAuthoringError::InvalidStructure);
        };
        let layout = store
            .borrow()
            .semantic_table_layout(family.node_id())
            .map_err(AuthoringError::from)?
            .ok_or(TableAuthoringError::InvalidStructure)?;
        let views = layout
            .structure()
            .and_then(|grid| grid.families())
            .ok_or(TableAuthoringError::InvalidStructure)?;
        if views.painted_entries != *painted_entries {
            return Err(TableAuthoringError::InvalidStructure);
        }
        let view =
            |id| MobjectFamily::from_node(Rc::clone(&store), id).map_err(TableAuthoringError::from);
        let row_label_family = views.row_labels.map(view).transpose()?;
        let column_label_family = views.column_labels.map(view).transpose()?;
        let top_left_entry_family = views.corner.map(view).transpose()?;
        let entry_family = view(views.body_rows)?;
        let body_entries = view(views.body_entries)?;
        let painted_entries = view(views.painted_entries)?;
        let table = Self {
            family,
            line_family: MobjectFamily::from_node(Rc::clone(&store), *lines)?,
            highlight_family: MobjectFamily::from_node(Rc::clone(&store), *highlights)?,
            row_label_family,
            column_label_family,
            top_left_entry_family,
            entry_family,
            body_entries,
            painted_entries,
        };
        table.options()?;
        table.shape()?;
        Ok(table)
    }
    /// Read the table-owned buffers from the sparse semantic declaration.
    ///
    /// Handles can rehydrate or alias the same root, and direct family scaling
    /// updates this declaration atomically with its leaves.  Queries must never
    /// retain a stale wrapper-side copy.
    fn options(&self) -> Result<TableOptions, TableAuthoringError> {
        self.family
            .integration_store()
            .borrow()
            .semantic_table_layout(self.family.node_id())
            .map_err(AuthoringError::from)?
            .map(|layout| TableOptions {
                h_buff: layout.h_buff(),
                v_buff: layout.v_buff(),
                label_buff: layout.label_buff(),
                include_outer_lines: layout.include_outer_lines(),
                include_inner_lines: layout.include_inner_lines(),
                line_stroke_color: layout.line_stroke_color(),
                line_stroke_width: layout.line_stroke_width(),
                line_stroke_opacity: layout.line_stroke_opacity(),
                line_stroke_join: layout.line_stroke_join(),
                line_stroke_cap: layout.line_stroke_cap(),
                cell_alignment_x: layout.cell_alignment_x(),
                cell_alignment_y: layout.cell_alignment_y(),
            })
            .ok_or(TableAuthoringError::InvalidStructure)
    }
    #[cfg(feature = "native-text")]
    pub fn from_rows<I, J, S>(scene: &mut Scene, rows: I) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self::from_rows_with_options(scene, rows, TableOptions::default())
    }
    #[cfg(feature = "native-text")]
    pub fn from_rows_with_options<I, J, S>(
        scene: &mut Scene,
        rows: I,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let rows = collect_text_rows(rows);
        let shape = table_shape(&rows)?;
        publish_native_text_table(
            TablePublisher::Scene(scene),
            rows.into_iter().flatten().collect(),
            shape,
            None,
            None,
            options,
        )
    }
    /// Build a native-text table with retained object or family-valued labels.
    ///
    /// Labels keep their original semantic roots and are positioned with the
    /// same grid measurement used for the table entries.
    #[cfg(feature = "native-text")]
    pub fn from_rows_with_composite_labels<'a, I, J, S>(
        scene: &mut Scene,
        rows: I,
        row_labels: Option<Vec<MobjectTarget<'a>>>,
        column_labels: Option<Vec<MobjectTarget<'a>>>,
        top_left_entry: Option<MobjectTarget<'a>>,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let rows = collect_text_rows(rows);
        let shape = table_shape(&rows)?;
        publish_native_text_table_with_composite_labels(
            TablePublisher::Scene(scene),
            rows.into_iter().flatten().collect(),
            shape,
            row_labels,
            column_labels,
            top_left_entry,
            options,
        )
    }
    #[cfg(feature = "native-text")]
    pub fn from_rows_in_store<I, J, S>(
        store: Rc<RefCell<noon_core::SemanticStore>>,
        rows: I,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let rows = collect_text_rows(rows);
        let shape = table_shape(&rows)?;
        publish_native_text_table(
            TablePublisher::Store(store),
            rows.into_iter().flatten().collect(),
            shape,
            None,
            None,
            options,
        )
    }
    #[cfg(feature = "native-text")]
    pub fn from_rows_in_live_session<I, J, S>(
        live: &mut crate::LiveSession<'_>,
        rows: I,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let rows = collect_text_rows(rows);
        let shape = table_shape(&rows)?;
        publish_native_text_table(
            TablePublisher::Live(live),
            rows.into_iter().flatten().collect(),
            shape,
            None,
            None,
            options,
        )
    }
    /// Live-session counterpart of [`Self::from_rows_with_composite_labels`].
    #[cfg(feature = "native-text")]
    pub fn from_rows_with_composite_labels_in_live_session<'a, I, J, S>(
        live: &mut crate::LiveSession<'_>,
        rows: I,
        row_labels: Option<Vec<MobjectTarget<'a>>>,
        column_labels: Option<Vec<MobjectTarget<'a>>>,
        top_left_entry: Option<MobjectTarget<'a>>,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let rows = collect_text_rows(rows);
        let shape = table_shape(&rows)?;
        publish_native_text_table_with_composite_labels(
            TablePublisher::Live(live),
            rows.into_iter().flatten().collect(),
            shape,
            row_labels,
            column_labels,
            top_left_entry,
            options,
        )
    }
    pub fn family(&self) -> &MobjectFamily {
        &self.family
    }
    pub fn entry_family(&self) -> &MobjectFamily {
        &self.entry_family
    }
    pub fn line_family(&self) -> &MobjectFamily {
        &self.line_family
    }
    /// Return the retained horizontal grid lines in ManimCE's creation order.
    pub fn get_horizontal_lines(&self) -> Result<Vec<Mobject>, TableAuthoringError> {
        let horizontal_count = self.horizontal_line_count()?;
        let lines = self.line_members()?;
        if horizontal_count > lines.len() {
            return Err(TableAuthoringError::InvalidStructure);
        }
        Ok(lines.into_iter().take(horizontal_count).collect())
    }
    /// Return the retained vertical grid lines in ManimCE's creation order.
    pub fn get_vertical_lines(&self) -> Result<Vec<Mobject>, TableAuthoringError> {
        let horizontal_count = self.horizontal_line_count()?;
        let lines = self.line_members()?;
        if horizontal_count > lines.len() {
            return Err(TableAuthoringError::InvalidStructure);
        }
        Ok(lines.into_iter().skip(horizontal_count).collect())
    }
    fn horizontal_line_count(&self) -> Result<usize, TableAuthoringError> {
        let grid = self.query_grid()?;
        let (rows, _) = queries::displayed_shape(&grid);
        let options = self.options()?;
        Ok(usize::from(options.include_outer_lines) * 2
            + if options.include_inner_lines {
                rows.saturating_sub(1)
            } else {
                0
            })
    }
    fn line_members(&self) -> Result<Vec<Mobject>, TableAuthoringError> {
        let store = Rc::clone(self.line_family.integration_store());
        self.line_family.validate()?;
        let nodes = store
            .borrow()
            .semantic_family_members_checked(self.line_family.node_id())
            .map_err(AuthoringError::from)?;
        nodes
            .into_iter()
            .map(|node| Mobject::from_node(Rc::clone(&store), node).map_err(Into::into))
            .collect()
    }
    pub fn highlight_family(&self) -> &MobjectFamily {
        &self.highlight_family
    }
    pub fn row_label_family(&self) -> Option<&MobjectFamily> {
        self.row_label_family.as_ref()
    }
    pub fn column_label_family(&self) -> Option<&MobjectFamily> {
        self.column_label_family.as_ref()
    }
    pub fn top_left_entry_family(&self) -> Option<&MobjectFamily> {
        self.top_left_entry_family.as_ref()
    }
    pub fn shape(&self) -> Result<(usize, usize), TableAuthoringError> {
        let grid = self.query_grid()?;
        Ok((grid.body_rows().len(), grid.body_rows()[0].len()))
    }
    pub fn get_cell(&self, row: usize, column: usize) -> Result<Mobject, TableAuthoringError> {
        admission::cell(&self.family, self.options()?, row, column)
    }
    pub fn get_cell_in_live_session(
        &self,
        live: &mut crate::LiveSession<'_>,
        row: usize,
        column: usize,
    ) -> Result<Mobject, TableAuthoringError> {
        admission::cell_in_publisher(
            TablePublisher::Live(live),
            &self.family,
            self.options()?,
            row,
            column,
        )
    }
    pub fn highlight_cell(
        &self,
        row: usize,
        column: usize,
        color: Color,
        opacity: f64,
    ) -> Result<Mobject, TableAuthoringError> {
        admission::highlight(
            &self.family,
            &self.highlight_family,
            self.options()?,
            row,
            column,
            color,
            opacity,
        )
    }
    /// Return a detached colored cell rectangle. Use [`Self::highlight_cell`]
    /// when the rectangle should become a retained table highlight.
    pub fn get_highlighted_cell(
        &self,
        row: usize,
        column: usize,
        color: Color,
        opacity: f64,
    ) -> Result<Mobject, TableAuthoringError> {
        admission::highlighted_cell(&self.family, self.options()?, row, column, color, opacity)
    }
    pub fn get_highlighted_cell_in_live_session(
        &self,
        live: &mut crate::LiveSession<'_>,
        row: usize,
        column: usize,
        color: Color,
        opacity: f64,
    ) -> Result<Mobject, TableAuthoringError> {
        admission::highlighted_cell_in_publisher(
            TablePublisher::Live(live),
            &self.family,
            self.options()?,
            row,
            column,
            color,
            opacity,
        )
    }
    pub fn highlight_cell_in_live_session(
        &self,
        live: &mut crate::LiveSession<'_>,
        row: usize,
        column: usize,
        color: Color,
        opacity: f64,
    ) -> Result<Mobject, TableAuthoringError> {
        admission::highlight_in_publisher(
            TablePublisher::Live(live),
            &self.family,
            &self.highlight_family,
            self.options()?,
            row,
            column,
            color,
            opacity,
        )
    }
}

#[cfg(feature = "latex")]
impl MathTable {
    pub fn from_rows<I, J, S>(
        scene: &mut Scene,
        backend: &mut impl LatexBackend,
        rows: I,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self::from_rows_with_options(scene, backend, rows, TableOptions::default())
    }
    pub fn from_rows_with_options<I, J, S>(
        scene: &mut Scene,
        backend: &mut impl LatexBackend,
        rows: I,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let rows = collect_text_rows(rows);
        let shape = table_shape(&rows)?;
        publish_text_table(
            TablePublisher::Scene(scene),
            backend,
            rows.into_iter().flatten().collect(),
            shape,
            None,
            None,
            options,
        )
        .map(Self)
    }
    pub fn from_rows_in_live_session<I, J, S>(
        live: &mut crate::LiveSession<'_>,
        backend: &mut impl LatexBackend,
        rows: I,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let rows = collect_text_rows(rows);
        let shape = table_shape(&rows)?;
        publish_text_table(
            TablePublisher::Live(live),
            backend,
            rows.into_iter().flatten().collect(),
            shape,
            None,
            None,
            options,
        )
        .map(Self)
    }
    pub fn table(&self) -> &Table {
        &self.0
    }
    pub fn into_table(self) -> Table {
        self.0
    }
}
#[cfg(feature = "latex")]
impl IntegerTable {
    pub fn from_rows<I, J>(
        scene: &mut Scene,
        backend: &mut impl LatexBackend,
        rows: I,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = f64>,
    {
        Self::from_rows_with_options(scene, backend, rows, TableOptions::default())
    }
    pub fn from_rows_with_options<I, J>(
        scene: &mut Scene,
        backend: &mut impl LatexBackend,
        rows: I,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = f64>,
    {
        let rows = collect_number_rows(rows);
        let shape = table_shape(&rows)?;
        publish_numeric_table(
            TablePublisher::Scene(scene),
            backend,
            rows.into_iter().flatten().collect(),
            shape,
            integer_format(),
            None,
            None,
            options,
        )
        .map(Self)
    }
    pub fn from_rows_in_live_session<I, J>(
        live: &mut crate::LiveSession<'_>,
        backend: &mut impl LatexBackend,
        rows: I,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = f64>,
    {
        let rows = collect_number_rows(rows);
        let shape = table_shape(&rows)?;
        publish_numeric_table(
            TablePublisher::Live(live),
            backend,
            rows.into_iter().flatten().collect(),
            shape,
            integer_format(),
            None,
            None,
            options,
        )
        .map(Self)
    }
    pub fn table(&self) -> &Table {
        &self.0
    }
    pub fn into_table(self) -> Table {
        self.0
    }
}
#[cfg(feature = "latex")]
impl DecimalTable {
    pub fn from_rows<I, J>(
        scene: &mut Scene,
        backend: &mut impl LatexBackend,
        rows: I,
        format: DecimalFormat,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = f64>,
    {
        Self::from_rows_with_options(scene, backend, rows, format, TableOptions::default())
    }
    pub fn from_rows_with_options<I, J>(
        scene: &mut Scene,
        backend: &mut impl LatexBackend,
        rows: I,
        format: DecimalFormat,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = f64>,
    {
        let rows = collect_number_rows(rows);
        let shape = table_shape(&rows)?;
        publish_numeric_table(
            TablePublisher::Scene(scene),
            backend,
            rows.into_iter().flatten().collect(),
            shape,
            format,
            None,
            None,
            options,
        )
        .map(Self)
    }
    pub fn from_rows_in_live_session<I, J>(
        live: &mut crate::LiveSession<'_>,
        backend: &mut impl LatexBackend,
        rows: I,
        format: DecimalFormat,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = f64>,
    {
        let rows = collect_number_rows(rows);
        let shape = table_shape(&rows)?;
        publish_numeric_table(
            TablePublisher::Live(live),
            backend,
            rows.into_iter().flatten().collect(),
            shape,
            format,
            None,
            None,
            options,
        )
        .map(Self)
    }
    pub fn table(&self) -> &Table {
        &self.0
    }
    pub fn into_table(self) -> Table {
        self.0
    }
}
impl MobjectTable {
    /// Construct a table from retained object or family roots.  A family stays
    /// one cell while its ordered leaves are translated together at commit.
    pub fn from_target_rows<'a, I, J>(
        scene: &mut Scene,
        rows: I,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = MobjectTarget<'a>>,
    {
        Self::from_target_rows_with_options(scene, rows, TableOptions::default())
    }
    pub fn from_target_rows_with_options<'a, I, J>(
        scene: &mut Scene,
        rows: I,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = MobjectTarget<'a>>,
    {
        let rows = collect_target_rows(rows);
        let shape = table_shape(&rows)?;
        publish_target_table(
            TablePublisher::Scene(scene),
            rows.into_iter().flatten().collect(),
            shape,
            None,
            None,
            options,
        )
        .map(Self)
    }
    pub fn from_target_rows_in_store<'a, I, J>(
        store: Rc<RefCell<noon_core::SemanticStore>>,
        rows: I,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = MobjectTarget<'a>>,
    {
        let rows = collect_target_rows(rows);
        let shape = table_shape(&rows)?;
        publish_target_table(
            TablePublisher::Store(store),
            rows.into_iter().flatten().collect(),
            shape,
            None,
            None,
            options,
        )
        .map(Self)
    }
    pub fn from_target_rows_in_live_session<'a, I, J>(
        live: &mut crate::LiveSession<'_>,
        rows: I,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = MobjectTarget<'a>>,
    {
        let rows = collect_target_rows(rows);
        let shape = table_shape(&rows)?;
        publish_target_table(
            TablePublisher::Live(live),
            rows.into_iter().flatten().collect(),
            shape,
            None,
            None,
            options,
        )
        .map(Self)
    }
    /// Construct a retained MobjectTable with individual shared row/column labels.
    ///
    /// Labels are arranged in the same Rust-owned grid as entries. When present,
    /// `row_labels` contains one Mobject per row and `column_labels` one per column.
    pub fn from_target_rows_with_labels<'a, I, J>(
        scene: &mut Scene,
        rows: I,
        row_labels: Option<Vec<Mobject>>,
        column_labels: Option<Vec<Mobject>>,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = MobjectTarget<'a>>,
    {
        let rows = collect_target_rows(rows);
        let shape = table_shape(&rows)?;
        publish_target_table(
            TablePublisher::Scene(scene),
            rows.into_iter().flatten().collect(),
            shape,
            row_labels,
            column_labels,
            options,
        )
        .map(Self)
    }
    /// Construct a retained MobjectTable with object or family-valued labels.
    /// Each supplied label root keeps its identity and all descendant leaves.
    pub fn from_target_rows_with_composite_labels<'a, I, J>(
        scene: &mut Scene,
        rows: I,
        row_labels: Option<Vec<MobjectTarget<'a>>>,
        column_labels: Option<Vec<MobjectTarget<'a>>>,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = MobjectTarget<'a>>,
    {
        let rows = collect_target_rows(rows);
        let shape = table_shape(&rows)?;
        publish_target_table_with_composite_labels(
            TablePublisher::Scene(scene),
            rows.into_iter().flatten().collect(),
            shape,
            row_labels,
            column_labels,
            options,
        )
        .map(Self)
    }
    /// Construct a retained MobjectTable with both label sets and a corner entry.
    pub fn from_target_rows_with_labels_and_top_left_entry<'a, I, J>(
        scene: &mut Scene,
        rows: I,
        row_labels: Vec<Mobject>,
        column_labels: Vec<Mobject>,
        top_left_entry: Mobject,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = MobjectTarget<'a>>,
    {
        let rows = collect_target_rows(rows);
        let shape = table_shape(&rows)?;
        publish_target_table_with_top_left_entry(
            TablePublisher::Scene(scene),
            rows.into_iter().flatten().collect(),
            shape,
            Some(row_labels),
            Some(column_labels),
            Some(top_left_entry),
            options,
        )
        .map(Self)
    }
    /// Construct a retained MobjectTable with object or family-valued labels
    /// and a shared object or family in the top-left corner.
    pub fn from_target_rows_with_composite_labels_and_top_left_entry<'a, I, J>(
        scene: &mut Scene,
        rows: I,
        row_labels: Vec<MobjectTarget<'a>>,
        column_labels: Vec<MobjectTarget<'a>>,
        top_left_entry: MobjectTarget<'a>,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = MobjectTarget<'a>>,
    {
        Self::from_target_rows_with_optional_composite_labels(
            scene,
            rows,
            Some(row_labels),
            Some(column_labels),
            Some(top_left_entry),
            options,
        )
    }
    /// Construct using optional label lists, preserving absent versus empty input.
    pub fn from_target_rows_with_optional_composite_labels<'a, I, J>(
        scene: &mut Scene,
        rows: I,
        row_labels: Option<Vec<MobjectTarget<'a>>>,
        column_labels: Option<Vec<MobjectTarget<'a>>>,
        top_left_entry: Option<MobjectTarget<'a>>,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = MobjectTarget<'a>>,
    {
        let rows = collect_target_rows(rows);
        let shape = table_shape(&rows)?;
        publish_target_table_with_composite_labels_and_top_left_entry(
            TablePublisher::Scene(scene),
            rows.into_iter().flatten().collect(),
            shape,
            row_labels,
            column_labels,
            top_left_entry,
            options,
        )
        .map(Self)
    }
    /// Live-session counterpart of [`Self::from_target_rows_with_labels`].
    pub fn from_target_rows_with_labels_in_live_session<'a, I, J>(
        live: &mut crate::LiveSession<'_>,
        rows: I,
        row_labels: Option<Vec<Mobject>>,
        column_labels: Option<Vec<Mobject>>,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = MobjectTarget<'a>>,
    {
        let rows = collect_target_rows(rows);
        let shape = table_shape(&rows)?;
        publish_target_table(
            TablePublisher::Live(live),
            rows.into_iter().flatten().collect(),
            shape,
            row_labels,
            column_labels,
            options,
        )
        .map(Self)
    }
    /// Live-session counterpart of [`Self::from_target_rows_with_composite_labels`].
    pub fn from_target_rows_with_composite_labels_in_live_session<'a, I, J>(
        live: &mut crate::LiveSession<'_>,
        rows: I,
        row_labels: Option<Vec<MobjectTarget<'a>>>,
        column_labels: Option<Vec<MobjectTarget<'a>>>,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = MobjectTarget<'a>>,
    {
        let rows = collect_target_rows(rows);
        let shape = table_shape(&rows)?;
        publish_target_table_with_composite_labels(
            TablePublisher::Live(live),
            rows.into_iter().flatten().collect(),
            shape,
            row_labels,
            column_labels,
            options,
        )
        .map(Self)
    }
    /// Live-session counterpart of
    /// [`Self::from_target_rows_with_labels_and_top_left_entry`].
    pub fn from_target_rows_with_labels_and_top_left_entry_in_live_session<'a, I, J>(
        live: &mut crate::LiveSession<'_>,
        rows: I,
        row_labels: Vec<Mobject>,
        column_labels: Vec<Mobject>,
        top_left_entry: Mobject,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = MobjectTarget<'a>>,
    {
        let rows = collect_target_rows(rows);
        let shape = table_shape(&rows)?;
        publish_target_table_with_top_left_entry(
            TablePublisher::Live(live),
            rows.into_iter().flatten().collect(),
            shape,
            Some(row_labels),
            Some(column_labels),
            Some(top_left_entry),
            options,
        )
        .map(Self)
    }
    /// Live-session counterpart of
    /// [`Self::from_target_rows_with_composite_labels_and_top_left_entry`].
    pub fn from_target_rows_with_composite_labels_and_top_left_entry_in_live_session<'a, I, J>(
        live: &mut crate::LiveSession<'_>,
        rows: I,
        row_labels: Vec<MobjectTarget<'a>>,
        column_labels: Vec<MobjectTarget<'a>>,
        top_left_entry: MobjectTarget<'a>,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = MobjectTarget<'a>>,
    {
        Self::from_target_rows_with_optional_composite_labels_in_live_session(
            live,
            rows,
            Some(row_labels),
            Some(column_labels),
            Some(top_left_entry),
            options,
        )
    }
    /// Construct using optional label lists, preserving absent versus empty input.
    pub fn from_target_rows_with_optional_composite_labels_in_live_session<'a, I, J>(
        live: &mut crate::LiveSession<'_>,
        rows: I,
        row_labels: Option<Vec<MobjectTarget<'a>>>,
        column_labels: Option<Vec<MobjectTarget<'a>>>,
        top_left_entry: Option<MobjectTarget<'a>>,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = MobjectTarget<'a>>,
    {
        let rows = collect_target_rows(rows);
        let shape = table_shape(&rows)?;
        publish_target_table_with_composite_labels_and_top_left_entry(
            TablePublisher::Live(live),
            rows.into_iter().flatten().collect(),
            shape,
            row_labels,
            column_labels,
            top_left_entry,
            options,
        )
        .map(Self)
    }
    pub fn from_rows<I, J>(scene: &mut Scene, rows: I) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = Mobject>,
    {
        Self::from_rows_with_options(scene, rows, TableOptions::default())
    }
    pub fn from_rows_with_options<I, J>(
        scene: &mut Scene,
        rows: I,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = Mobject>,
    {
        let rows = collect_mobject_rows(rows);
        let shape = table_shape(&rows)?;
        publish_existing_table(
            TablePublisher::Scene(scene),
            rows.into_iter().flatten().collect(),
            shape,
            None,
            None,
            options,
        )
        .map(Self)
    }
    pub fn from_rows_in_store<I, J>(
        store: Rc<RefCell<noon_core::SemanticStore>>,
        rows: I,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = Mobject>,
    {
        let rows = collect_mobject_rows(rows);
        let shape = table_shape(&rows)?;
        publish_existing_table(
            TablePublisher::Store(store),
            rows.into_iter().flatten().collect(),
            shape,
            None,
            None,
            options,
        )
        .map(Self)
    }
    pub fn from_rows_in_live_session<I, J>(
        live: &mut crate::LiveSession<'_>,
        rows: I,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = Mobject>,
    {
        let rows = collect_mobject_rows(rows);
        let shape = table_shape(&rows)?;
        publish_existing_table(
            TablePublisher::Live(live),
            rows.into_iter().flatten().collect(),
            shape,
            None,
            None,
            options,
        )
        .map(Self)
    }
    pub fn from_rows_with_labels<I, J>(
        scene: &mut Scene,
        rows: I,
        row_labels: Vec<Mobject>,
        column_labels: Vec<Mobject>,
        options: TableOptions,
    ) -> Result<Self, TableAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = Mobject>,
    {
        let rows = collect_mobject_rows(rows);
        let shape = table_shape(&rows)?;
        publish_existing_table(
            TablePublisher::Scene(scene),
            rows.into_iter().flatten().collect(),
            shape,
            Some(row_labels),
            Some(column_labels),
            options,
        )
        .map(Self)
    }
    pub fn table(&self) -> &Table {
        &self.0
    }
    pub fn into_table(self) -> Table {
        self.0
    }
}
macro_rules! table_deref { ($($type:ty),+ $(,)?) => { $(impl Deref for $type { type Target=Table; fn deref(&self)->&Table { &self.0 } })+ }; }
table_deref!(MobjectTable);
#[cfg(feature = "latex")]
table_deref!(MathTable, IntegerTable, DecimalTable);
fn collect_text_rows<I, J, S>(rows: I) -> Vec<Vec<String>>
where
    I: IntoIterator<Item = J>,
    J: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    rows.into_iter()
        .map(|r| r.into_iter().map(|s| s.as_ref().to_owned()).collect())
        .collect()
}
#[cfg(feature = "latex")]
fn collect_number_rows<I, J>(rows: I) -> Vec<Vec<f64>>
where
    I: IntoIterator<Item = J>,
    J: IntoIterator<Item = f64>,
{
    rows.into_iter().map(|r| r.into_iter().collect()).collect()
}
fn collect_mobject_rows<I, J>(rows: I) -> Vec<Vec<Mobject>>
where
    I: IntoIterator<Item = J>,
    J: IntoIterator<Item = Mobject>,
{
    rows.into_iter().map(|r| r.into_iter().collect()).collect()
}
fn collect_target_rows<'a, I, J>(rows: I) -> Vec<Vec<MobjectTarget<'a>>>
where
    I: IntoIterator<Item = J>,
    J: IntoIterator<Item = MobjectTarget<'a>>,
{
    rows.into_iter()
        .map(|row| row.into_iter().collect())
        .collect()
}
#[cfg(feature = "latex")]
fn integer_format() -> DecimalFormat {
    DecimalFormat {
        decimal_places: 0,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Bounds2D64;
    use noon_core::{SemanticObjectState, StoredGeometry};

    fn circle(store: &Rc<RefCell<noon_core::SemanticStore>>, radius: f32) -> Mobject {
        Mobject::new(
            Rc::clone(store),
            SemanticObjectState::new(StoredGeometry::Circle { radius }),
        )
        .unwrap()
    }

    #[test]
    fn arrange_in_grid_cell_alignment_places_unequal_entries_on_requested_edges() {
        let mut scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let entries = [
            circle(&store, 1.0),
            circle(&store, 0.5),
            circle(&store, 0.5),
            circle(&store, 1.0),
        ];
        let table = MobjectTable::from_rows_with_options(
            &mut scene,
            vec![
                vec![entries[0].clone(), entries[1].clone()],
                vec![entries[2].clone(), entries[3].clone()],
            ],
            TableOptions {
                cell_alignment_x: -1.0,
                cell_alignment_y: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
        let rows = table.rows().unwrap();
        let bounds = |entry: &TableEntry| match entry {
            TableEntry::Mobject(object) => object.layout_bounds().unwrap().unwrap(),
            TableEntry::Family(family) => family.layout_bounds().unwrap().unwrap(),
        };
        let center = |bounds: Bounds2D64| {
            (
                (bounds.min_x + bounds.max_x) * 0.5,
                (bounds.min_y + bounds.max_y) * 0.5,
            )
        };
        let first = center(bounds(&rows[0][0]));
        let small = center(bounds(&rows[0][1]));
        let lower_small = center(bounds(&rows[1][0]));
        let lower_large = center(bounds(&rows[1][1]));
        assert!((small.0 - lower_large.0 + 0.5).abs() < 1e-6);
        assert!((small.1 - first.1 - 0.5).abs() < 1e-6);
        assert!((lower_small.0 - first.0 + 0.5).abs() < 1e-6);
        assert!((lower_small.1 - lower_large.1 - 0.5).abs() < 1e-6);
    }

    #[test]
    fn mobject_table_keeps_entries_and_exposes_row_column_and_cell_families() {
        let mut scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let entries = vec![
            circle(&store, 1.0),
            circle(&store, 0.5),
            circle(&store, 0.25),
            circle(&store, 0.75),
        ];
        let table = MobjectTable::from_rows_with_options(
            &mut scene,
            vec![
                vec![entries[0].clone(), entries[1].clone()],
                vec![entries[2].clone(), entries[3].clone()],
            ],
            TableOptions {
                include_outer_lines: true,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(table.shape().unwrap(), (2, 2));
        assert_eq!(
            table
                .get_entries()
                .unwrap()
                .into_iter()
                .map(|entry| match entry {
                    TableEntry::Mobject(object) => object,
                    TableEntry::Family(_) => panic!("leaf input remains a leaf entry"),
                })
                .collect::<Vec<_>>(),
            entries
        );
        assert_eq!(
            table.get_entry(1, 0).unwrap().as_target().node_id(),
            entries[2].node_id()
        );
        assert_eq!(table.row_families().unwrap().len(), 2);
        assert_eq!(table.column_families().unwrap().len(), 2);
        assert_eq!(
            store
                .borrow()
                .semantic_family_members_checked(table.line_family().node_id())
                .unwrap()
                .len(),
            6
        );
        let horizontal_lines = table.get_horizontal_lines().unwrap();
        let vertical_lines = table.get_vertical_lines().unwrap();
        assert_eq!(horizontal_lines.len(), 3);
        assert_eq!(vertical_lines.len(), 3);
        let line_nodes = store
            .borrow()
            .semantic_family_members_checked(table.line_family().node_id())
            .unwrap();
        assert_eq!(
            horizontal_lines
                .iter()
                .map(Mobject::node_id)
                .collect::<Vec<_>>(),
            line_nodes[..3]
        );
        assert_eq!(
            vertical_lines
                .iter()
                .map(Mobject::node_id)
                .collect::<Vec<_>>(),
            line_nodes[3..]
        );
        assert_eq!(
            store
                .borrow()
                .semantic_family_members_checked(table.family().node_id())
                .unwrap(),
            vec![
                table.highlight_family().node_id(),
                table.painted_entries.node_id(),
                table.line_family().node_id(),
            ]
        );
        let cell = table.get_cell(0, 0).unwrap();
        let highlight = table.highlight_cell(0, 0, Color::YELLOW, 0.4).unwrap();
        assert_eq!(
            cell.layout_bounds().unwrap(),
            highlight.layout_bounds().unwrap()
        );
        let highlight_nodes = store
            .borrow()
            .semantic_family_members_checked(table.highlight_family().node_id())
            .unwrap();
        assert_eq!(highlight_nodes, vec![highlight.node_id()]);
        let newest_highlight = table.highlight_cell(1, 1, Color::BLUE, 0.5).unwrap();
        assert_eq!(
            store
                .borrow()
                .semantic_family_members_checked(table.highlight_family().node_id())
                .unwrap(),
            vec![newest_highlight.node_id(), highlight.node_id()]
        );
        assert_eq!(newest_highlight.state().unwrap().z_index(), 0.0);
        let detached = table.get_highlighted_cell(0, 1, Color::BLUE, 0.5).unwrap();
        assert_eq!(
            store
                .borrow()
                .semantic_family_members_checked(table.highlight_family().node_id())
                .unwrap(),
            vec![newest_highlight.node_id(), highlight.node_id()]
        );
        assert!(detached.layout_bounds().unwrap().is_some());

        let unrelated = scene.circle(0.25).unwrap();
        scene.add(&unrelated).unwrap();
        scene
            .add_many(&[MobjectTarget::Family(table.family())])
            .unwrap();
        let table_leaves = store
            .borrow()
            .ordered_leaf_nodes(table.family().node_id())
            .unwrap();
        let scene_leaves = store.borrow().ordered_leaf_nodes(scene.root()).unwrap();
        assert_eq!(
            scene_leaves,
            std::iter::once(unrelated.node_id())
                .chain(table_leaves)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn table_can_omit_inner_lines_and_rehydrates_the_option() {
        let mut scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let entries = [
            circle(&store, 0.3),
            circle(&store, 0.2),
            circle(&store, 0.25),
            circle(&store, 0.15),
        ];
        let table = MobjectTable::from_rows_with_options(
            &mut scene,
            vec![
                vec![entries[0].clone(), entries[1].clone()],
                vec![entries[2].clone(), entries[3].clone()],
            ],
            TableOptions {
                include_outer_lines: true,
                include_inner_lines: false,
                line_stroke_color: Color::rgba(0.25, 0.5, 0.75, 0.8),
                line_stroke_width: 0.06,
                line_stroke_opacity: 0.35,
                line_stroke_join: StrokeJoin::Bevel,
                line_stroke_cap: StrokeCap::Square,
                cell_alignment_x: -1.0,
                cell_alignment_y: 1.0,
                ..Default::default()
            },
        )
        .unwrap()
        .into_table();

        let line_nodes = store
            .borrow()
            .semantic_family_members_checked(table.line_family().node_id())
            .unwrap();
        assert_eq!(
            line_nodes.len(),
            4,
            "outer rules remain while both inner rules are omitted"
        );
        assert_eq!(table.get_horizontal_lines().unwrap().len(), 2);
        assert_eq!(table.get_vertical_lines().unwrap().len(), 2);
        let semantic = store.borrow();
        let line_state = semantic
            .node(line_nodes[0])
            .unwrap()
            .semantic_object_state()
            .unwrap();
        assert_eq!(
            line_state.style.stroke,
            Some(noon_core::SemanticPaint::Solid(Color::rgba(
                0.25, 0.5, 0.75, 0.8
            )))
        );
        assert_eq!(line_state.style.stroke_width, 0.06);
        assert_eq!(line_state.style.stroke_opacity, 0.35);
        assert_eq!(line_state.style.stroke_join, StrokeJoin::Bevel);
        assert_eq!(line_state.style.stroke_cap, StrokeCap::Square);
        drop(semantic);
        let rehydrated = Table::from_family(table.family().clone()).unwrap();
        let options = rehydrated.options().unwrap();
        assert!(options.include_outer_lines);
        assert!(!options.include_inner_lines);
        assert_eq!(options.line_stroke_color, Color::rgba(0.25, 0.5, 0.75, 0.8));
        assert_eq!(options.line_stroke_width, 0.06);
        assert_eq!(options.line_stroke_opacity, 0.35);
        assert_eq!(options.line_stroke_join, StrokeJoin::Bevel);
        assert_eq!(options.line_stroke_cap, StrokeCap::Square);
        assert_eq!(options.cell_alignment_x, -1.0);
        assert_eq!(options.cell_alignment_y, 1.0);
    }

    #[test]
    fn mobject_table_labels_keep_identity_and_use_table_spacing() {
        let mut scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let entry = circle(&store, 0.5);
        let row_label = circle(&store, 0.2);
        let column_label = circle(&store, 0.15);
        let options = TableOptions {
            h_buff: 0.75,
            v_buff: 0.4,
            ..Default::default()
        };

        let table = MobjectTable::from_target_rows_with_labels(
            &mut scene,
            vec![vec![MobjectTarget::from(&entry)]],
            Some(vec![row_label.clone()]),
            Some(vec![column_label.clone()]),
            options,
        )
        .unwrap()
        .into_table();

        let store = store.borrow();
        assert_eq!(
            store
                .semantic_family_members_checked(table.row_label_family().unwrap().node_id())
                .unwrap(),
            vec![row_label.node_id()]
        );
        assert_eq!(
            store
                .semantic_family_members_checked(table.column_label_family().unwrap().node_id())
                .unwrap(),
            vec![column_label.node_id()]
        );
        let entry_bounds = entry.layout_bounds().unwrap().unwrap();
        let row_bounds = row_label.layout_bounds().unwrap().unwrap();
        let column_bounds = column_label.layout_bounds().unwrap().unwrap();
        assert!((entry_bounds.min_x - row_bounds.max_x - options.h_buff).abs() < 1e-6);
        assert!((column_bounds.min_y - entry_bounds.max_y - options.v_buff).abs() < 1e-6);
    }

    #[test]
    fn mobject_table_family_labels_keep_roots_and_place_all_descendants() {
        let mut scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let entry = circle(&store, 0.5);
        let row_label_left = circle(&store, 0.2);
        let mut row_label_right = circle(&store, 0.1);
        row_label_right.shift(0.8, 0.0).unwrap();
        let row_label = MobjectFamily::create(
            Rc::clone(&store),
            &[(&row_label_left).into(), (&row_label_right).into()],
        )
        .unwrap();
        let column_label_top = circle(&store, 0.15);
        let mut column_label_bottom = circle(&store, 0.1);
        column_label_bottom.shift(0.0, -0.5).unwrap();
        let column_label = MobjectFamily::create(
            Rc::clone(&store),
            &[(&column_label_top).into(), (&column_label_bottom).into()],
        )
        .unwrap();
        let corner_label_left = circle(&store, 0.25);
        let mut corner_label_right = circle(&store, 0.12);
        corner_label_right.shift(0.5, 0.0).unwrap();
        let corner_label = MobjectFamily::create(
            Rc::clone(&store),
            &[(&corner_label_left).into(), (&corner_label_right).into()],
        )
        .unwrap();
        let initial_offsets = [
            row_label_right.state().unwrap().transform.translation.x
                - row_label_left.state().unwrap().transform.translation.x,
            column_label_bottom.state().unwrap().transform.translation.y
                - column_label_top.state().unwrap().transform.translation.y,
            corner_label_right.state().unwrap().transform.translation.x
                - corner_label_left.state().unwrap().transform.translation.x,
        ];
        let options = TableOptions {
            h_buff: 0.75,
            v_buff: 0.4,
            ..Default::default()
        };

        let table = MobjectTable::from_target_rows_with_composite_labels_and_top_left_entry(
            &mut scene,
            vec![vec![MobjectTarget::from(&entry)]],
            vec![MobjectTarget::from(&row_label)],
            vec![MobjectTarget::from(&column_label)],
            MobjectTarget::from(&corner_label),
            options,
        )
        .unwrap()
        .into_table();

        let members = store.borrow();
        assert_eq!(
            members
                .semantic_family_members_checked(table.row_label_family().unwrap().node_id())
                .unwrap(),
            vec![row_label.node_id()]
        );
        assert_eq!(
            members
                .semantic_family_members_checked(table.column_label_family().unwrap().node_id())
                .unwrap(),
            vec![column_label.node_id()]
        );
        assert_eq!(
            members
                .semantic_family_members_checked(table.top_left_entry_family().unwrap().node_id())
                .unwrap(),
            vec![corner_label.node_id()]
        );
        drop(members);

        let entry_bounds = entry.layout_bounds().unwrap().unwrap();
        let row_bounds = row_label.layout_bounds().unwrap().unwrap();
        let column_bounds = column_label.layout_bounds().unwrap().unwrap();
        let corner_bounds = corner_label.layout_bounds().unwrap().unwrap();
        let center_x = |bounds: Bounds2D64| (bounds.min_x + bounds.max_x) * 0.5;
        let center_y = |bounds: Bounds2D64| (bounds.min_y + bounds.max_y) * 0.5;
        assert!((entry_bounds.min_x - row_bounds.max_x - options.h_buff).abs() < 1e-6);
        assert!((column_bounds.min_y - entry_bounds.max_y - options.v_buff).abs() < 1e-6);
        assert!((center_x(corner_bounds) - center_x(row_bounds)).abs() < 1e-6);
        assert!((center_y(corner_bounds) - center_y(column_bounds)).abs() < 1e-6);
        assert_eq!(
            row_label_right.state().unwrap().transform.translation.x
                - row_label_left.state().unwrap().transform.translation.x,
            initial_offsets[0]
        );
        assert_eq!(
            column_label_bottom.state().unwrap().transform.translation.y
                - column_label_top.state().unwrap().transform.translation.y,
            initial_offsets[1]
        );
        assert_eq!(
            corner_label_right.state().unwrap().transform.translation.x
                - corner_label_left.state().unwrap().transform.translation.x,
            initial_offsets[2]
        );
    }

    #[test]
    fn labeled_queries_include_supplied_corner_and_keep_entry_roots() {
        fn key(entry: &CompositeEntryHandle) -> (bool, noon_core::SemanticNodeId) {
            match entry {
                CompositeEntryHandle::Mobject(object) => (false, object.node_id()),
                CompositeEntryHandle::Family(family) => (true, family.node_id()),
            }
        }
        let mut scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let entry = circle(&store, 0.5);
        let row = circle(&store, 0.2);
        let column = circle(&store, 0.15);
        let corner = circle(&store, 0.35);
        let table = MobjectTable::from_target_rows_with_labels_and_top_left_entry(
            &mut scene,
            vec![vec![MobjectTarget::from(&entry)]],
            vec![row.clone()],
            vec![column.clone()],
            corner.clone(),
            TableOptions::default(),
        )
        .unwrap()
        .into_table();

        let roots = [corner, column, row, entry].map(|object| (false, object.node_id()));
        assert_eq!(
            table
                .get_rows()
                .unwrap()
                .iter()
                .map(|row| row.iter().map(key).collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            vec![vec![roots[0], roots[1]], vec![roots[2], roots[3]],]
        );
        assert_eq!(
            table
                .get_columns()
                .unwrap()
                .iter()
                .map(|column| column.iter().map(key).collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            vec![vec![roots[0], roots[2]], vec![roots[1], roots[3]],]
        );
        assert_eq!(
            table
                .get_entries()
                .unwrap()
                .iter()
                .map(key)
                .collect::<Vec<_>>(),
            roots
        );
    }

    #[test]
    fn labeled_missing_corner_is_stable_copied_and_ignored_by_cell_bounds() {
        let mut scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let entry = circle(&store, 0.5);
        let row = circle(&store, 0.2);
        let column = circle(&store, 0.15);
        let table = MobjectTable::from_target_rows_with_labels(
            &mut scene,
            vec![vec![MobjectTarget::from(&entry)]],
            Some(vec![row.clone()]),
            Some(vec![column.clone()]),
            TableOptions::default(),
        )
        .unwrap()
        .into_table();
        let placeholder = match &table.get_rows().unwrap()[0][0] {
            TableEntry::Mobject(object) => object.clone(),
            _ => panic!("the missing corner is a VMobject"),
        };
        assert!(placeholder.layout_bounds().unwrap().is_none());
        assert_eq!(
            table.get_rows().unwrap()[0][0].as_target().node_id(),
            placeholder.node_id()
        );
        assert_eq!(table.get_entries().unwrap().len(), 3);
        assert_eq!(
            table.entry_at_position(0, 0).unwrap().as_target().node_id(),
            entry.node_id()
        );
        assert_eq!(
            table.entry_at_position(1, 1).unwrap().as_target().node_id(),
            entry.node_id()
        );
        let cell = table
            .get_cell(0, 0)
            .unwrap()
            .layout_bounds()
            .unwrap()
            .unwrap();
        let row_bounds = column.layout_bounds().unwrap().unwrap();
        let column_bounds = row.layout_bounds().unwrap().unwrap();
        let options = table.options().unwrap();
        assert!((cell.min_x - (column_bounds.min_x - options.h_buff * 0.5)).abs() < 1e-6);
        assert!((cell.max_y - (row_bounds.max_y + options.v_buff * 0.5)).abs() < 1e-6);
        let copied = table.family().copy_family().unwrap();
        let copied = Table::from_family(copied.root().clone()).unwrap();
        let copied_corner = copied.get_rows().unwrap()[0][0].as_target().node_id();
        assert_ne!(copied_corner, placeholder.node_id());
        assert_eq!(
            copied.get_rows().unwrap()[0][0].as_target().node_id(),
            copied_corner
        );
        assert_eq!(copied.get_entries().unwrap().len(), 3);
        assert_eq!(
            copied.get_cell(0, 0).unwrap().layout_bounds().unwrap(),
            Some(cell)
        );
    }

    #[test]
    fn labeled_ignored_corner_survives_copy_without_becoming_painted() {
        let mut scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let entry = circle(&store, 0.5);
        let corner = circle(&store, 0.2);
        let initial = corner.state().unwrap();
        let table = MobjectTable::from_target_rows_with_optional_composite_labels(
            &mut scene,
            vec![vec![MobjectTarget::from(&entry)]],
            None::<Vec<MobjectTarget<'_>>>,
            None::<Vec<MobjectTarget<'_>>>,
            Some(MobjectTarget::from(&corner)),
            TableOptions::default(),
        )
        .unwrap()
        .into_table();
        assert!(table.top_left_entry_family().is_none());
        assert_eq!(corner.state().unwrap(), initial);
        assert_eq!(
            table.get_labels().unwrap()[0].as_target().node_id(),
            corner.node_id()
        );
        assert_eq!(table.get_entries().unwrap().len(), 1);
        let copied = table.family().copy_family().unwrap();
        let copied = Table::from_family(copied.root().clone()).unwrap();
        let labels = copied.get_labels().unwrap();
        assert_ne!(labels[0].as_target().node_id(), corner.node_id());
        assert_eq!(copied.get_entries().unwrap().len(), 1);
        assert!(copied.top_left_entry_family().is_none());
        assert!(!store
            .borrow()
            .ordered_leaf_nodes(copied.family().node_id())
            .unwrap()
            .contains(&labels[0].as_target().node_id()));
    }

    #[test]
    fn labeled_column_headers_are_counted_as_horizontal_grid_lines() {
        let mut scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let first = circle(&store, 0.5);
        let second = circle(&store, 0.4);
        let third = circle(&store, 0.3);
        let fourth = circle(&store, 0.2);
        let table = MobjectTable::from_target_rows_with_labels(
            &mut scene,
            vec![
                vec![(&first).into(), (&second).into()],
                vec![(&third).into(), (&fourth).into()],
            ],
            None,
            Some(vec![circle(&store, 0.15), circle(&store, 0.1)]),
            TableOptions::default(),
        )
        .unwrap()
        .into_table();
        let horizontal = table.get_horizontal_lines().unwrap();
        let vertical = table.get_vertical_lines().unwrap();
        assert_eq!((horizontal.len(), vertical.len()), (2, 1));
        for line in horizontal {
            let bounds = line.layout_bounds().unwrap().unwrap();
            assert!(bounds.width() > 0.0);
            assert_eq!(bounds.height(), 0.0);
        }
        for line in vertical {
            let bounds = line.layout_bounds().unwrap().unwrap();
            assert_eq!(bounds.width(), 0.0);
            assert!(bounds.height() > 0.0);
        }
    }

    #[test]
    fn labeled_constructor_counts_follow_pinned_reference_errors() {
        // Actual ManimCE 0.21.0: rows/columns of length 0..3 on a 2x2 body,
        // with and without a supplied corner. O=success, I=IndexError, V=ValueError.
        let expected = [*b"OVOI", *b"IVII", *b"OVOI", *b"IIII"];
        for corner in [false, true] {
            for (row_count, outcomes) in expected.into_iter().enumerate() {
                for (column_count, outcome) in outcomes.into_iter().enumerate() {
                    let mut scene = Scene::new();
                    let store = Rc::clone(scene.integration_store());
                    let cells: Vec<_> = (0..4).map(|_| circle(&store, 0.5)).collect();
                    let rows: Vec<_> = (0..row_count).map(|_| circle(&store, 0.2)).collect();
                    let columns: Vec<_> = (0..column_count).map(|_| circle(&store, 0.15)).collect();
                    let top = circle(&store, 0.1);
                    let before = cells[0].state().unwrap();
                    let result = MobjectTable::from_target_rows_with_optional_composite_labels(
                        &mut scene,
                        cells
                            .chunks(2)
                            .map(|row| row.iter().map(MobjectTarget::from).collect::<Vec<_>>()),
                        Some(rows.iter().map(MobjectTarget::from).collect()),
                        Some(columns.iter().map(MobjectTarget::from).collect()),
                        corner.then(|| MobjectTarget::from(&top)),
                        TableOptions::default(),
                    );
                    match outcome {
                        b'O' => {
                            let table = result.unwrap().into_table();
                            assert_eq!(
                                table.get_rows().unwrap().len(),
                                2 + usize::from(column_count == 2)
                            );
                            assert_eq!(
                                table.get_columns().unwrap().len(),
                                2 + usize::from(row_count == 2)
                            );
                        }
                        b'I' => {
                            let error = result
                                .expect_err("the pinned constructor rejects this label count");
                            assert!(matches!(
                                error,
                                TableAuthoringError::InvalidLabelIndex { .. }
                            ));
                            assert_eq!(error.to_string(), "list index out of range");
                            assert_eq!(cells[0].state().unwrap(), before);
                        }
                        b'V' => {
                            let error = result
                                .expect_err("the pinned constructor rejects this label count");
                            assert!(matches!(
                                error,
                                TableAuthoringError::InsufficientColumnLabels { .. }
                            ));
                            assert_eq!(
                                error.to_string(),
                                "Too few rows and columns to fit all submobjetcs."
                            );
                            assert_eq!(cells[0].state().unwrap(), before);
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
    }

    #[cfg(feature = "native-text")]
    #[test]
    fn labeled_native_text_empty_labels_use_shared_normalization() {
        let mut scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let column = circle(&store, 0.15);
        let table = Table::from_rows_with_composite_labels(
            &mut scene,
            [["body"]],
            Some(vec![]),
            Some(vec![(&column).into()]),
            None,
            TableOptions::default(),
        )
        .unwrap();
        assert_eq!(table.get_rows().unwrap().len(), 2);
        assert_eq!(table.get_columns().unwrap().len(), 1);
        assert!(table.row_label_family().is_none());
        assert!(table.column_label_family().is_some());
    }

    #[test]
    fn labeled_column_only_roles_survive_rehydration_and_copy() {
        let mut scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let entry = circle(&store, 0.5);
        let column = circle(&store, 0.15);
        let table = MobjectTable::from_target_rows_with_labels(
            &mut scene,
            vec![vec![MobjectTarget::from(&entry)]],
            None,
            Some(vec![column]),
            TableOptions::default(),
        )
        .unwrap()
        .into_table();
        assert!(table.row_label_family().is_none());
        assert!(table.column_label_family().is_some());
        let restored = Table::from_family(table.family().clone()).unwrap();
        assert!(restored.row_label_family().is_none());
        assert_eq!(
            restored.column_label_family().unwrap().node_id(),
            table.column_label_family().unwrap().node_id()
        );
        let copied = table.family().copy_family().unwrap();
        let copied = Table::from_family(copied.root().clone()).unwrap();
        assert!(copied.row_label_family().is_none());
        assert!(copied.column_label_family().is_some());
    }

    #[test]
    fn mobject_table_top_left_entry_uses_corner_cell_and_keeps_identity() {
        let mut scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let entry = circle(&store, 0.5);
        let row_label = circle(&store, 0.2);
        let column_label = circle(&store, 0.15);
        let top_left_entry = circle(&store, 0.35);
        let options = TableOptions {
            h_buff: 0.75,
            v_buff: 0.4,
            ..Default::default()
        };

        let table = MobjectTable::from_target_rows_with_labels_and_top_left_entry(
            &mut scene,
            vec![vec![MobjectTarget::from(&entry)]],
            vec![row_label.clone()],
            vec![column_label.clone()],
            top_left_entry.clone(),
            options,
        )
        .unwrap()
        .into_table();

        let restored = Table::from_family(table.family().clone()).unwrap();
        let store = store.borrow();
        let top_left_family = table.top_left_entry_family().unwrap();
        let restored_top_left_family = restored.top_left_entry_family().unwrap();
        for family in [top_left_family, restored_top_left_family] {
            assert_eq!(
                store
                    .semantic_family_members_checked(family.node_id())
                    .unwrap(),
                vec![top_left_entry.node_id()]
            );
        }

        let top_left_bounds = top_left_entry.layout_bounds().unwrap().unwrap();
        let row_bounds = row_label.layout_bounds().unwrap().unwrap();
        let column_bounds = column_label.layout_bounds().unwrap().unwrap();
        let entry_bounds = entry.layout_bounds().unwrap().unwrap();
        let center_x = |bounds: Bounds2D64| (bounds.min_x + bounds.max_x) * 0.5;
        let center_y = |bounds: Bounds2D64| (bounds.min_y + bounds.max_y) * 0.5;
        assert!((center_x(top_left_bounds) - center_x(row_bounds)).abs() < 1e-6);
        assert!((center_y(top_left_bounds) - center_y(column_bounds)).abs() < 1e-6);
        assert!((entry_bounds.min_x - top_left_bounds.max_x - options.h_buff).abs() < 1e-6);
        assert!((top_left_bounds.min_y - entry_bounds.max_y - options.v_buff).abs() < 1e-6);
    }

    #[test]
    fn foreign_entry_rejects_the_whole_table_before_any_entry_moves() {
        let mut scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let first = circle(&store, 1.0);
        let initial = first.state().unwrap();
        let foreign_store = Rc::new(RefCell::new(noon_core::SemanticStore::new()));
        let foreign = circle(&foreign_store, 1.0);

        let error =
            MobjectTable::from_rows(&mut scene, vec![vec![first.clone(), foreign]]).unwrap_err();
        assert!(matches!(
            error,
            TableAuthoringError::Semantic(AuthoringError::ForeignStore)
        ));
        assert_eq!(first.state().unwrap(), initial);
    }

    #[test]
    fn family_entry_keeps_its_root_and_translates_each_leaf_once() {
        let mut scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let first = circle(&store, 1.0);
        let mut second = circle(&store, 0.5);
        second.shift(3.0, 0.0).unwrap();
        let family =
            MobjectFamily::create(Rc::clone(&store), &[(&first).into(), (&second).into()]).unwrap();
        let before = (first.state().unwrap(), second.state().unwrap());

        let table =
            MobjectTable::from_target_rows(&mut scene, vec![vec![MobjectTarget::from(&family)]])
                .unwrap();

        assert!(matches!(
            table.get_entry(0, 0).unwrap(),
            TableEntry::Family(entry) if entry.node_id() == family.node_id()
        ));
        let after = (first.state().unwrap(), second.state().unwrap());
        assert_eq!(
            after.1.transform.translation.x - after.0.transform.translation.x,
            before.1.transform.translation.x - before.0.transform.translation.x
        );
    }

    #[test]
    fn overlapping_family_entries_retain_shared_leaf_identity() {
        let mut scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let first = circle(&store, 1.0);
        let family = MobjectFamily::create(Rc::clone(&store), &[(&first).into()]).unwrap();
        let table = MobjectTable::from_target_rows(
            &mut scene,
            vec![vec![
                MobjectTarget::from(&first),
                MobjectTarget::from(&family),
            ]],
        )
        .unwrap()
        .into_table();
        assert_eq!(
            table
                .get_entries()
                .unwrap()
                .iter()
                .map(|entry| entry.as_target().node_id())
                .collect::<Vec<_>>(),
            vec![first.node_id(), family.node_id()]
        );
        assert_eq!(
            store
                .borrow()
                .ordered_leaf_nodes(table.family().node_id())
                .unwrap()
                .iter()
                .filter(|id| **id == first.node_id())
                .count(),
            1
        );
    }

    #[test]
    fn cell_queries_keep_authored_spacing_after_entry_transform() {
        let mut scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let mut scaled = circle(&store, 0.5);
        scaled.set_scale(2.0, 1.5).unwrap();
        let table = MobjectTable::from_rows_with_options(
            &mut scene,
            vec![
                vec![scaled, circle(&store, 0.25)],
                vec![circle(&store, 0.75), circle(&store, 0.4)],
            ],
            TableOptions {
                h_buff: 2.25,
                v_buff: 1.4,
                ..Default::default()
            },
        )
        .unwrap()
        .into_table();
        let authored = table
            .get_cell(1, 1)
            .unwrap()
            .layout_bounds()
            .unwrap()
            .expect("table cells have concrete bounds");
        let restored = Table::from_family(table.family().clone()).unwrap();
        assert_eq!(
            restored
                .get_cell(1, 1)
                .unwrap()
                .layout_bounds()
                .unwrap()
                .expect("rehydrated table cells have concrete bounds"),
            authored
        );
        let copied = table.family().copy_family().unwrap();
        let copied = Table::from_family(copied.root().clone()).unwrap();
        assert_eq!(
            copied
                .get_cell(1, 1)
                .unwrap()
                .layout_bounds()
                .unwrap()
                .expect("copied table cells have concrete bounds"),
            authored
        );

        table.entry_family().shift(3.0, -2.0).unwrap();
        let shifted = table
            .get_cell(1, 1)
            .unwrap()
            .layout_bounds()
            .unwrap()
            .expect("shifted table cells have concrete bounds");
        assert_eq!(shifted.min_x, authored.min_x + 3.0);
        assert_eq!(shifted.max_x, authored.max_x + 3.0);
        assert_eq!(shifted.min_y, authored.min_y - 2.0);
        assert_eq!(shifted.max_y, authored.max_y - 2.0);
        assert_eq!(
            table
                .highlight_cell(1, 1, Color::BLUE, 0.5)
                .unwrap()
                .layout_bounds()
                .unwrap()
                .expect("highlight cells have concrete bounds"),
            shifted
        );
    }

    #[test]
    fn cells_use_selected_row_and_column_extrema_with_copied_custom_buffers() {
        let mut scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let table = MobjectTable::from_rows_with_options(
            &mut scene,
            vec![
                vec![circle(&store, 0.2), circle(&store, 1.5)],
                vec![circle(&store, 2.0), circle(&store, 0.3)],
            ],
            TableOptions {
                h_buff: 1.75,
                v_buff: 0.6,
                ..Default::default()
            },
        )
        .unwrap()
        .into_table();
        let copy =
            Table::from_family(table.family().copy_family().unwrap().root().clone()).unwrap();
        table.entry_family().shift(2.0, -1.0).unwrap();
        copy.entry_family().shift(2.0, -1.0).unwrap();
        let entries = table.rows().unwrap();
        let bounds = |entry: &TableEntry| match entry {
            TableEntry::Mobject(value) => value.layout_bounds().unwrap().unwrap(),
            TableEntry::Family(value) => value.layout().unwrap().bounds().unwrap(),
        };
        let column = [bounds(&entries[0][0]), bounds(&entries[1][0])];
        let row = [bounds(&entries[1][0]), bounds(&entries[1][1])];
        let cell = table
            .get_cell(1, 0)
            .unwrap()
            .layout_bounds()
            .unwrap()
            .expect("table cells have concrete bounds");
        // Retained Rectangle dimensions are f32; queries widen them to f64.
        let assert_near = |actual: f64, expected: f64| {
            assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
        };
        assert_near(
            cell.min_x,
            column
                .iter()
                .map(|bound| bound.min_x)
                .fold(f64::INFINITY, f64::min)
                - 0.875,
        );
        assert_near(
            cell.max_x,
            column
                .iter()
                .map(|bound| bound.max_x)
                .fold(f64::NEG_INFINITY, f64::max)
                + 0.875,
        );
        assert_near(
            cell.min_y,
            row.iter()
                .map(|bound| bound.min_y)
                .fold(f64::INFINITY, f64::min)
                - 0.3,
        );
        assert_near(
            cell.max_y,
            row.iter()
                .map(|bound| bound.max_y)
                .fold(f64::NEG_INFINITY, f64::max)
                + 0.3,
        );
        assert_eq!(
            copy.get_cell(1, 0)
                .unwrap()
                .layout_bounds()
                .unwrap()
                .expect("copied table cells have concrete bounds"),
            cell
        );
    }

    #[test]
    fn direct_table_scale_updates_root_buffers_but_entry_family_scale_does_not() {
        let mut scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let table = MobjectTable::from_rows_with_options(
            &mut scene,
            vec![vec![circle(&store, 0.5), circle(&store, 1.0)]],
            TableOptions {
                h_buff: 1.75,
                v_buff: 0.6,
                ..Default::default()
            },
        )
        .unwrap()
        .into_table();
        let alias = Table::from_family(table.family().clone()).unwrap();
        let copied =
            Table::from_family(table.family().copy_family().unwrap().root().clone()).unwrap();
        let assert_near = |actual: f64, expected: f64| {
            assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
        };

        table.entry_family().scale(0.8, 0.8).unwrap();
        assert_near(table.options().unwrap().h_buff, 1.75);
        assert_near(table.options().unwrap().v_buff, 0.6);

        table.family().scale(0.8, 0.8).unwrap();
        assert_near(table.options().unwrap().h_buff, 1.4);
        assert_near(table.options().unwrap().v_buff, 0.48);
        assert_eq!(alias.options().unwrap(), table.options().unwrap());

        copied.family().scale(0.5, 0.5).unwrap();
        assert_near(copied.options().unwrap().h_buff, 0.875);
        assert_near(copied.options().unwrap().v_buff, 0.3);
        assert_near(table.options().unwrap().h_buff, 1.4);
        assert_near(table.options().unwrap().v_buff, 0.48);

        crate::LayoutAnchor::from(table.family())
            .scale(0.5, 0.5, crate::ManimRotationPivot::Point(0.0, 0.0))
            .unwrap();
        assert_near(table.options().unwrap().h_buff, 0.7);
        assert_near(table.options().unwrap().v_buff, 0.24);
        crate::LayoutAnchor::from(table.family())
            .stretch(
                1.5,
                crate::LayoutDimension::Width,
                crate::ManimRotationPivot::Center,
            )
            .unwrap();
        assert_near(table.options().unwrap().h_buff, 0.7);
        assert_near(table.options().unwrap().v_buff, 0.24);

        // Table cell buffers have a non-negative durable declaration.  A
        // reflected direct family scale therefore fails atomically rather than
        // committing transformed entries with an unusable cell layout.
        let before_negative_scale = table.options().unwrap();
        let entry_before = match table.get_entry(0, 0).unwrap() {
            TableEntry::Mobject(entry) => entry.state().unwrap(),
            TableEntry::Family(_) => panic!("this fixture has leaf entries"),
        };
        assert!(table.family().scale(-1.0, 1.0).is_err());
        assert_eq!(table.options().unwrap(), before_negative_scale);
        assert_eq!(
            match table.get_entry(0, 0).unwrap() {
                TableEntry::Mobject(entry) => entry.state().unwrap(),
                TableEntry::Family(_) => panic!("this fixture has leaf entries"),
            },
            entry_before
        );
    }

    #[test]
    fn live_table_scale_updates_the_same_root_layout_declaration() {
        let scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let first = circle(&store, 0.5);
        let second = circle(&store, 1.0);
        let mut execution = scene.execution_session().unwrap();
        let table = {
            let mut live = scene.live(&mut execution);
            let table = MobjectTable::from_rows_in_live_session(
                &mut live,
                vec![vec![first, second]],
                TableOptions {
                    h_buff: 1.25,
                    v_buff: 0.75,
                    ..Default::default()
                },
            )
            .unwrap()
            .into_table();
            live.scale_family(table.family(), 0.8, 0.8).unwrap();
            table
        };
        let alias = Table::from_family(table.family().clone()).unwrap();
        assert!((table.options().unwrap().h_buff - 1.0).abs() < 1e-12);
        assert!((table.options().unwrap().v_buff - 0.6).abs() < 1e-12);
        assert_eq!(alias.options().unwrap(), table.options().unwrap());
    }

    #[test]
    fn stale_live_table_publication_rolls_back_every_staged_node() {
        let mut scene = Scene::new();
        let mut sentinel = scene.circle(0.25).unwrap();
        scene.add(&sentinel).unwrap();
        let execution = scene.execution_session().unwrap();
        scene.install_execution(execution);
        sentinel.shift(1.0, 0.0).unwrap();
        let revision = scene.revision();
        let nodes = scene.integration_store().borrow().len();
        let result = MobjectTable::from_rows(&mut scene, vec![vec![sentinel.clone()]]);
        assert!(matches!(
            result,
            Err(TableAuthoringError::Semantic(
                AuthoringError::ExecutionPublication(
                    crate::ExecutionSessionPublicationError::StaleSceneRevision { .. }
                )
            ))
        ));
        assert_eq!(scene.revision(), revision);
        assert_eq!(scene.integration_store().borrow().len(), nodes);
    }

    #[cfg(feature = "native-text")]
    #[test]
    fn plain_table_admits_native_text_without_a_latex_backend() {
        let mut scene = Scene::new();
        let table = Table::from_rows(&mut scene, [["plain", "text"]]).unwrap();
        let entries = table.entries().unwrap();
        assert!(entries
            .iter()
            .all(|entry| matches!(entry, TableEntry::Mobject(object)
            if object.state().unwrap().content.text().is_some())));
    }

    #[cfg(feature = "native-text")]
    #[test]
    fn plain_table_positions_composite_labels_and_corner_entry() {
        let mut scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let row_a = circle(&store, 0.15);
        let row_b = circle(&store, 0.2);
        let row_a_family = MobjectFamily::create(Rc::clone(&store), &[(&row_a).into()]).unwrap();
        let row_b_family = MobjectFamily::create(Rc::clone(&store), &[(&row_b).into()]).unwrap();
        let column = circle(&store, 0.25);
        let corner = circle(&store, 0.3);
        let table = Table::from_rows_with_composite_labels(
            &mut scene,
            [["first"], ["second"]],
            Some(vec![(&row_a_family).into(), (&row_b_family).into()]),
            Some(vec![(&column).into()]),
            Some((&corner).into()),
            TableOptions::default(),
        )
        .unwrap();

        let row_label_roots = store
            .borrow()
            .semantic_family_members_checked(table.row_label_family().unwrap().node_id())
            .unwrap();
        assert_eq!(
            row_label_roots,
            vec![row_a_family.node_id(), row_b_family.node_id()]
        );
        assert_eq!(
            store
                .borrow()
                .semantic_family_members_checked(table.column_label_family().unwrap().node_id())
                .unwrap(),
            vec![column.node_id()]
        );
        let top_left_family = table.top_left_entry_family().unwrap();
        assert_eq!(
            store
                .borrow()
                .semantic_family_members_checked(top_left_family.node_id())
                .unwrap(),
            vec![corner.node_id()]
        );

        let entries = table.rows().unwrap();
        let entry_bounds = entries
            .iter()
            .skip(1)
            .map(|row| match &row[1] {
                TableEntry::Mobject(object) => object.layout_bounds().unwrap().unwrap(),
                TableEntry::Family(family) => family.layout_bounds().unwrap().unwrap(),
            })
            .collect::<Vec<_>>();
        let center = |bounds: Bounds2D64| {
            (
                (bounds.min_x + bounds.max_x) * 0.5,
                (bounds.min_y + bounds.max_y) * 0.5,
            )
        };
        assert_eq!(
            center(row_a_family.layout_bounds().unwrap().unwrap()).1,
            center(entry_bounds[0]).1
        );
        assert_eq!(
            center(row_b_family.layout_bounds().unwrap().unwrap()).1,
            center(entry_bounds[1]).1
        );
        assert_eq!(
            center(column.layout_bounds().unwrap().unwrap()).0,
            center(entry_bounds[0]).0
        );
        assert_eq!(
            center(corner.layout_bounds().unwrap().unwrap()).0,
            center(row_a_family.layout_bounds().unwrap().unwrap()).0
        );
        assert_eq!(
            center(corner.layout_bounds().unwrap().unwrap()).1,
            center(column.layout_bounds().unwrap().unwrap()).1
        );
    }
}
