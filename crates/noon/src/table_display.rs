//! Retained Manim-compatible table families.
//!
//! Tables are ordinary semantic families.  Their entries, labels, grid lines,
//! and highlights keep normal [`Mobject`] identities; this module only owns the
//! admission, layout and topology convention.

mod admission;

use crate::{
    AuthoringError, Color, DecimalFormat, LatexBackend, Mobject, MobjectFamily, MobjectTarget,
    NumericAuthoringError, Scene, TextAuthoringError,
};
use std::{cell::RefCell, ops::Deref, rc::Rc};

use admission::{
    publish_existing_table, publish_numeric_table, publish_text_table, table_shape, TablePublisher,
};

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
    RaggedRows { expected: usize, actual: usize },
    DuplicateEntry,
    InvalidLabels { expected: usize, actual: usize },
    InvalidOption { name: &'static str, value: f64 },
    InvalidStructure,
    Text(TextAuthoringError),
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
    options: TableOptions,
}
#[derive(Clone, Debug)]
pub struct MathTable(Table);
#[derive(Clone, Debug)]
pub struct IntegerTable(Table);
#[derive(Clone, Debug)]
pub struct DecimalTable(Table);
#[derive(Clone, Debug)]
pub struct MobjectTable(Table);

impl Table {
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
    }
    pub fn from_rows_in_store<I, J, S>(
        store: Rc<RefCell<noon_core::SemanticStore>>,
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
            TablePublisher::Store(store),
            backend,
            rows.into_iter().flatten().collect(),
            shape,
            None,
            None,
            options,
        )
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
    pub fn get_entries(&self) -> Result<Vec<Mobject>, TableAuthoringError> {
        admission::entries(&self.entry_family)
    }
    pub fn entries(&self) -> Result<Vec<Mobject>, TableAuthoringError> {
        self.get_entries()
    }
    pub fn rows(&self) -> Result<Vec<Vec<Mobject>>, TableAuthoringError> {
        admission::rows(&self.entry_family)
    }
    pub fn get_rows(&self) -> Result<Vec<Vec<Mobject>>, TableAuthoringError> {
        self.rows()
    }
    pub fn columns(&self) -> Result<Vec<Vec<Mobject>>, TableAuthoringError> {
        admission::columns(&self.entry_family)
    }
    pub fn get_columns(&self) -> Result<Vec<Vec<Mobject>>, TableAuthoringError> {
        self.columns()
    }
    pub fn get_entry(&self, row: usize, column: usize) -> Result<Mobject, TableAuthoringError> {
        self.rows()?
            .get(row)
            .and_then(|items| items.get(column))
            .cloned()
            .ok_or(TableAuthoringError::InvalidStructure)
    }
    pub fn entry(&self, row: usize, column: usize) -> Result<Mobject, TableAuthoringError> {
        self.get_entry(row, column)
    }
    pub fn row_families(&self) -> Result<Vec<MobjectFamily>, TableAuthoringError> {
        admission::row_families(&self.entry_family)
    }
    pub fn column_families(&self) -> Result<Vec<MobjectFamily>, TableAuthoringError> {
        admission::column_families(&self.entry_family)
    }
    pub fn get_cell(&self, row: usize, column: usize) -> Result<Mobject, TableAuthoringError> {
        admission::cell(&self.entry_family, self.options, row, column)
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
            self.options,
            row,
            column,
            color,
            opacity,
        )
    }
}

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
        Table::from_rows(scene, backend, rows).map(Self)
    }
    pub fn table(&self) -> &Table {
        &self.0
    }
    pub fn into_table(self) -> Table {
        self.0
    }
}
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
table_deref!(MathTable, IntegerTable, DecimalTable, MobjectTable);
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
        assert_eq!(table.get_entries().unwrap(), entries);
        assert_eq!(table.get_entry(1, 0).unwrap(), entries[2]);
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
}
