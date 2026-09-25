//! Retained Manim-compatible table families.
//!
//! Tables are ordinary semantic families.  Their entries, labels, grid lines,
//! and highlights keep normal [`Mobject`] identities; this module only owns the
//! admission, layout and topology convention.

mod admission;

use crate::{
    AuthoringError, Color, CompositeEntryHandle, Mobject, MobjectFamily, MobjectTarget, Scene,
    TextAuthoringError,
};
#[cfg(feature = "latex")]
use crate::{DecimalFormat, LatexBackend, NumericAuthoringError};
use std::{cell::RefCell, ops::Deref, rc::Rc};

#[cfg(feature = "native-text")]
use admission::publish_native_text_table;
use admission::{publish_existing_table, publish_target_table, table_shape, TablePublisher};

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
    pub label_buff: f64,
}

impl Default for TableOptions {
    fn default() -> Self {
        Self {
            h_buff: DEFAULT_TABLE_H_BUFF,
            v_buff: DEFAULT_TABLE_V_BUFF,
            include_outer_lines: false,
            label_buff: DEFAULT_TABLE_LABEL_BUFF,
        }
    }
}

impl TableOptions {
    pub(crate) fn validate(self) -> Result<Self, TableAuthoringError> {
        for (name, value) in [
            ("h_buff", self.h_buff),
            ("v_buff", self.v_buff),
            ("label_buff", self.label_buff),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(TableAuthoringError::InvalidOption { name, value });
            }
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
    InvalidOption {
        name: &'static str,
        value: f64,
    },
    InvalidStructure,
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
            Self::InvalidOption { name, value } => write!(f, "invalid table {name}: {value}"),
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
    line_family: MobjectFamily,
    highlight_family: MobjectFamily,
    row_label_family: Option<MobjectFamily>,
    column_label_family: Option<MobjectFamily>,
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
        let [highlights, entries, lines, labels @ ..] = members.as_slice() else {
            return Err(TableAuthoringError::InvalidStructure);
        };
        if labels.len() > 2 {
            return Err(TableAuthoringError::InvalidStructure);
        }
        let entry_family = MobjectFamily::from_node(Rc::clone(&store), *entries)?;
        let table = Self {
            family,
            line_family: MobjectFamily::from_node(Rc::clone(&store), *lines)?,
            highlight_family: MobjectFamily::from_node(Rc::clone(&store), *highlights)?,
            row_label_family: labels
                .first()
                .map(|node| MobjectFamily::from_node(Rc::clone(&store), *node))
                .transpose()?,
            column_label_family: labels
                .get(1)
                .map(|node| MobjectFamily::from_node(Rc::clone(&store), *node))
                .transpose()?,
            entry_family,
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
    pub fn family(&self) -> &MobjectFamily {
        &self.family
    }
    pub fn entry_family(&self) -> &MobjectFamily {
        &self.entry_family
    }
    pub fn line_family(&self) -> &MobjectFamily {
        &self.line_family
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
    pub fn shape(&self) -> Result<(usize, usize), TableAuthoringError> {
        admission::shape_from_family(&self.entry_family)
    }
    pub fn get_entries(&self) -> Result<Vec<TableEntry>, TableAuthoringError> {
        admission::entries(&self.entry_family)
    }
    pub fn entries(&self) -> Result<Vec<TableEntry>, TableAuthoringError> {
        self.get_entries()
    }
    pub fn rows(&self) -> Result<Vec<Vec<TableEntry>>, TableAuthoringError> {
        admission::rows(&self.entry_family)
    }
    pub fn get_rows(&self) -> Result<Vec<Vec<TableEntry>>, TableAuthoringError> {
        self.rows()
    }
    pub fn columns(&self) -> Result<Vec<Vec<TableEntry>>, TableAuthoringError> {
        admission::columns(&self.entry_family)
    }
    pub fn get_columns(&self) -> Result<Vec<Vec<TableEntry>>, TableAuthoringError> {
        self.columns()
    }
    pub fn get_entry(&self, row: usize, column: usize) -> Result<TableEntry, TableAuthoringError> {
        let store = self.entry_family.integration_store();
        let row = store
            .borrow()
            .semantic_family_member_at_checked(self.entry_family.node_id(), row)
            .map_err(AuthoringError::from)?
            .ok_or(TableAuthoringError::InvalidStructure)?;
        let entry = store
            .borrow()
            .semantic_family_member_at_checked(row, column)
            .map_err(AuthoringError::from)?
            .ok_or(TableAuthoringError::InvalidStructure)?;
        CompositeEntryHandle::from_node(Rc::clone(store), entry).map_err(Into::into)
    }
    pub fn entry(&self, row: usize, column: usize) -> Result<TableEntry, TableAuthoringError> {
        self.get_entry(row, column)
    }
    pub fn row_families(&self) -> Result<Vec<MobjectFamily>, TableAuthoringError> {
        admission::row_families(&self.entry_family)
    }
    pub fn column_families(&self) -> Result<Vec<MobjectFamily>, TableAuthoringError> {
        admission::column_families(&self.entry_family)
    }
    pub fn get_cell(&self, row: usize, column: usize) -> Result<Mobject, TableAuthoringError> {
        admission::cell(&self.entry_family, self.options()?, row, column)
    }
    pub fn get_cell_in_live_session(
        &self,
        live: &mut crate::LiveSession<'_>,
        row: usize,
        column: usize,
    ) -> Result<Mobject, TableAuthoringError> {
        admission::cell_in_publisher(
            TablePublisher::Live(live),
            &self.entry_family,
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
            &self.entry_family,
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
        admission::highlighted_cell(
            &self.entry_family,
            self.options()?,
            row,
            column,
            color,
            opacity,
        )
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
            &self.entry_family,
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
            &self.entry_family,
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
    use noon_core::{SemanticObjectState, StoredGeometry};

    fn circle(store: &Rc<RefCell<noon_core::SemanticStore>>, radius: f32) -> Mobject {
        Mobject::new(
            Rc::clone(store),
            SemanticObjectState::new(StoredGeometry::Circle { radius }),
        )
        .unwrap()
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
        assert_eq!(
            store
                .borrow()
                .semantic_family_members_checked(table.family().node_id())
                .unwrap(),
            vec![
                table.highlight_family().node_id(),
                table.entry_family().node_id(),
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
    fn overlapping_family_entries_fail_without_moving_a_leaf() {
        let mut scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let first = circle(&store, 1.0);
        let family = MobjectFamily::create(Rc::clone(&store), &[(&first).into()]).unwrap();
        let before = first.state().unwrap();

        let result = MobjectTable::from_target_rows(
            &mut scene,
            vec![vec![
                MobjectTarget::from(&first),
                MobjectTarget::from(&family),
            ]],
        );

        assert!(matches!(
            result,
            Err(TableAuthoringError::Semantic(AuthoringError::Semantic(
                noon_core::SemanticSceneOperationError::DuplicateMembershipTarget(_)
            )))
        ));
        assert_eq!(first.state().unwrap(), before);
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
        let mut scene = Scene::new();
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
}
