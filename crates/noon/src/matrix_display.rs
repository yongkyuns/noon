//! Retained Manim-compatible matrix display families.
//!
//! Matrix topology is represented by ordinary nested semantic families:
//! matrix -> entries -> rows -> leaves, plus two bracket leaves.  That makes
//! shape reconstructible from the durable scene topology without adding a
//! feature-specific semantic metadata channel.

mod admission;

use crate::{
    AuthoringError, CompositeEntryHandle, DecimalFormat, LatexBackend, Mobject, MobjectFamily,
    MobjectTarget, NumericAuthoringError, Scene, TextAuthoringError,
};
use std::{cell::RefCell, rc::Rc};

use admission::{
    matrix_shape, publish_existing_mobject_matrix, publish_numeric_matrix,
    publish_target_mobject_matrix, publish_text_matrix, MatrixPublisher,
};

pub const DEFAULT_MATRIX_V_BUFF: f64 = 0.8;
pub const DEFAULT_MATRIX_H_BUFF: f64 = 1.3;
pub const DEFAULT_MATRIX_BRACKET_H_BUFF: f64 = 0.25;
pub const DEFAULT_MATRIX_BRACKET_V_BUFF: f64 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MatrixOptions {
    pub v_buff: f64,
    pub h_buff: f64,
    pub bracket_h_buff: f64,
    pub bracket_v_buff: f64,
    pub stretch_brackets: bool,
}

impl Default for MatrixOptions {
    fn default() -> Self {
        Self {
            v_buff: DEFAULT_MATRIX_V_BUFF,
            h_buff: DEFAULT_MATRIX_H_BUFF,
            bracket_h_buff: DEFAULT_MATRIX_BRACKET_H_BUFF,
            bracket_v_buff: DEFAULT_MATRIX_BRACKET_V_BUFF,
            stretch_brackets: true,
        }
    }
}

impl MatrixOptions {
    pub(crate) fn validate(self) -> Result<Self, MatrixAuthoringError> {
        for (name, value) in [
            ("v_buff", self.v_buff),
            ("h_buff", self.h_buff),
            ("bracket_h_buff", self.bracket_h_buff),
            ("bracket_v_buff", self.bracket_v_buff),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(MatrixAuthoringError::InvalidOption { name, value });
            }
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MatrixAuthoringError {
    EmptyMatrix,
    RaggedRows { expected: usize, actual: usize },
    DuplicateEntry,
    InvalidOption { name: &'static str, value: f64 },
    InvalidStructure,
    Text(TextAuthoringError),
    Numeric(NumericAuthoringError),
    Semantic(AuthoringError),
    LiveSession(crate::LiveSessionError),
}

impl std::fmt::Display for MatrixAuthoringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyMatrix => f.write_str("a Matrix requires at least one non-empty row"),
            Self::RaggedRows { expected, actual } => {
                write!(f, "matrix row has {actual} entries; expected {expected}")
            }
            Self::DuplicateEntry => f.write_str("a MobjectMatrix entry may occur only once"),
            Self::InvalidOption { name, value } => write!(f, "invalid matrix {name}: {value}"),
            Self::InvalidStructure => f.write_str("semantic family is not a valid Matrix"),
            Self::Text(error) => error.fmt(f),
            Self::Numeric(error) => error.fmt(f),
            Self::Semantic(error) => error.fmt(f),
            Self::LiveSession(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for MatrixAuthoringError {}
impl From<TextAuthoringError> for MatrixAuthoringError {
    fn from(value: TextAuthoringError) -> Self {
        Self::Text(value)
    }
}
impl From<NumericAuthoringError> for MatrixAuthoringError {
    fn from(value: NumericAuthoringError) -> Self {
        Self::Numeric(value)
    }
}
impl From<AuthoringError> for MatrixAuthoringError {
    fn from(value: AuthoringError) -> Self {
        Self::Semantic(value)
    }
}

/// A matrix root, its durable entry topology, and bracket identities.
#[derive(Clone, Debug)]
pub struct Matrix {
    family: MobjectFamily,
    entry_family: MobjectFamily,
    left_bracket: Mobject,
    right_bracket: Mobject,
}

#[derive(Clone, Debug)]
pub struct IntegerMatrix(Matrix);
#[derive(Clone, Debug)]
pub struct DecimalMatrix(Matrix);
#[derive(Clone, Debug)]
pub struct MobjectMatrix(Matrix);

/// One retained Matrix entry root, shared with Table entry queries.
pub type MatrixEntry = CompositeEntryHandle;

impl Matrix {
    pub fn from_rows<I, J, S>(
        scene: &mut Scene,
        backend: &mut impl LatexBackend,
        rows: I,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self::from_rows_with_options(scene, backend, rows, MatrixOptions::default())
    }

    pub fn from_rows_with_options<I, J, S>(
        scene: &mut Scene,
        backend: &mut impl LatexBackend,
        rows: I,
        options: MatrixOptions,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let rows = collect_text_rows(rows);
        let shape = matrix_shape(&rows)?;
        publish_text_matrix(
            MatrixPublisher::Scene(scene),
            backend,
            rows.into_iter().flatten().collect(),
            shape,
            options,
        )
    }

    pub fn from_rows_in_store<I, J, S>(
        store: Rc<RefCell<noon_core::SemanticStore>>,
        backend: &mut impl LatexBackend,
        rows: I,
        options: MatrixOptions,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let rows = collect_text_rows(rows);
        let shape = matrix_shape(&rows)?;
        publish_text_matrix(
            MatrixPublisher::Store(store),
            backend,
            rows.into_iter().flatten().collect(),
            shape,
            options,
        )
    }

    pub fn from_rows_in_live_session<I, J, S>(
        live: &mut crate::LiveSession<'_>,
        backend: &mut impl LatexBackend,
        rows: I,
        options: MatrixOptions,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let rows = collect_text_rows(rows);
        let shape = matrix_shape(&rows)?;
        publish_text_matrix(
            MatrixPublisher::Live(live),
            backend,
            rows.into_iter().flatten().collect(),
            shape,
            options,
        )
    }

    pub fn family(&self) -> &MobjectFamily {
        &self.family
    }
    pub fn entry_family(&self) -> &MobjectFamily {
        &self.entry_family
    }
    pub fn left_bracket(&self) -> &Mobject {
        &self.left_bracket
    }
    pub fn right_bracket(&self) -> &Mobject {
        &self.right_bracket
    }

    pub fn shape(&self) -> Result<(usize, usize), MatrixAuthoringError> {
        let rows = self.row_nodes()?;
        let columns = rows.first().map_or(0, Vec::len);
        if columns == 0 || rows.is_empty() || rows.iter().any(|row| row.len() != columns) {
            return Err(MatrixAuthoringError::InvalidStructure);
        }
        Ok((rows.len(), columns))
    }

    pub fn entries(&self) -> Result<Vec<MatrixEntry>, MatrixAuthoringError> {
        Ok(self.row_nodes()?.into_iter().flatten().collect())
    }
    pub fn rows(&self) -> Result<Vec<Vec<MatrixEntry>>, MatrixAuthoringError> {
        self.row_nodes()
    }
    pub fn columns(&self) -> Result<Vec<Vec<MatrixEntry>>, MatrixAuthoringError> {
        let rows = self.row_nodes()?;
        let columns = self.shape()?.1;
        Ok((0..columns)
            .map(|column| rows.iter().map(|row| row[column].clone()).collect())
            .collect())
    }
    pub fn row_families(&self) -> Result<Vec<MobjectFamily>, MatrixAuthoringError> {
        let store = Rc::clone(self.entry_family.integration_store());
        let nodes = store
            .borrow()
            .semantic_family_members_checked(self.entry_family.node_id())
            .map_err(AuthoringError::from)?;
        nodes
            .into_iter()
            .map(|node| MobjectFamily::from_node(Rc::clone(&store), node).map_err(Into::into))
            .collect()
    }
    pub fn column_families(&self) -> Result<Vec<MobjectFamily>, MatrixAuthoringError> {
        self.columns()?
            .iter()
            .map(|column| alias_target_family(self.entry_family.integration_store(), column))
            .collect()
    }
    pub fn bracket_family(&self) -> Result<MobjectFamily, MatrixAuthoringError> {
        let brackets = [self.left_bracket.clone(), self.right_bracket.clone()];
        let targets: Vec<_> = brackets.iter().map(MobjectTarget::from).collect();
        Ok(MobjectFamily::create(
            Rc::clone(self.entry_family.integration_store()),
            &targets,
        )?)
    }

    pub fn from_family(family: MobjectFamily) -> Result<Self, MatrixAuthoringError> {
        family.validate()?;
        let store = Rc::clone(family.integration_store());
        let members = store
            .borrow()
            .semantic_family_members_checked(family.node_id())
            .map_err(AuthoringError::from)?;
        let [entries, left, right] = members.as_slice() else {
            return Err(MatrixAuthoringError::InvalidStructure);
        };
        let result = Self {
            family,
            entry_family: MobjectFamily::from_node(Rc::clone(&store), *entries)?,
            left_bracket: Mobject::from_node(Rc::clone(&store), *left)?,
            right_bracket: Mobject::from_node(store, *right)?,
        };
        result.shape()?;
        Ok(result)
    }

    fn row_nodes(&self) -> Result<Vec<Vec<MatrixEntry>>, MatrixAuthoringError> {
        self.entry_family.validate()?;
        let store = Rc::clone(self.entry_family.integration_store());
        let rows = store
            .borrow()
            .semantic_family_members_checked(self.entry_family.node_id())
            .map_err(AuthoringError::from)?;
        rows.into_iter()
            .map(|row| {
                let leaves = store
                    .borrow()
                    .semantic_family_members_checked(row)
                    .map_err(AuthoringError::from)?;
                leaves
                    .into_iter()
                    .map(|leaf| {
                        CompositeEntryHandle::from_node(Rc::clone(&store), leaf).map_err(Into::into)
                    })
                    .collect()
            })
            .collect()
    }
}

fn collect_text_rows<I, J, S>(rows: I) -> Vec<Vec<String>>
where
    I: IntoIterator<Item = J>,
    J: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    rows.into_iter()
        .map(|row| {
            row.into_iter()
                .map(|entry| entry.as_ref().to_owned())
                .collect()
        })
        .collect()
}
fn alias_target_family(
    store: &Rc<RefCell<noon_core::SemanticStore>>,
    entries: &[MatrixEntry],
) -> Result<MobjectFamily, MatrixAuthoringError> {
    let targets: Vec<_> = entries.iter().map(MatrixEntry::as_target).collect();
    Ok(MobjectFamily::create(Rc::clone(store), &targets)?)
}

impl DecimalMatrix {
    pub fn from_rows<I, J>(
        scene: &mut Scene,
        backend: &mut impl LatexBackend,
        rows: I,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = f64>,
    {
        Self::from_rows_with_format(
            scene,
            backend,
            rows,
            DecimalFormat {
                decimal_places: 1,
                ..Default::default()
            },
        )
    }
    pub fn from_rows_with_format<I, J>(
        scene: &mut Scene,
        backend: &mut impl LatexBackend,
        rows: I,
        format: DecimalFormat,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = f64>,
    {
        Self::from_rows_with_format_and_options(
            scene,
            backend,
            rows,
            format,
            MatrixOptions::default(),
        )
    }
    pub fn from_rows_with_format_and_options<I, J>(
        scene: &mut Scene,
        backend: &mut impl LatexBackend,
        rows: I,
        format: DecimalFormat,
        options: MatrixOptions,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = f64>,
    {
        let rows = collect_numeric_rows(rows);
        let shape = matrix_shape(&rows)?;
        Ok(Self(publish_numeric_matrix(
            MatrixPublisher::Scene(scene),
            backend,
            rows.into_iter().flatten().collect(),
            shape,
            format,
            options,
        )?))
    }
    pub fn from_rows_in_store<I, J>(
        store: Rc<RefCell<noon_core::SemanticStore>>,
        backend: &mut impl LatexBackend,
        rows: I,
        format: DecimalFormat,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = f64>,
    {
        Self::from_rows_in_store_with_options(
            store,
            backend,
            rows,
            format,
            MatrixOptions::default(),
        )
    }
    pub fn from_rows_in_store_with_options<I, J>(
        store: Rc<RefCell<noon_core::SemanticStore>>,
        backend: &mut impl LatexBackend,
        rows: I,
        format: DecimalFormat,
        options: MatrixOptions,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = f64>,
    {
        let rows = collect_numeric_rows(rows);
        let shape = matrix_shape(&rows)?;
        Ok(Self(publish_numeric_matrix(
            MatrixPublisher::Store(store),
            backend,
            rows.into_iter().flatten().collect(),
            shape,
            format,
            options,
        )?))
    }
    pub fn from_rows_in_live_session<I, J>(
        live: &mut crate::LiveSession<'_>,
        backend: &mut impl LatexBackend,
        rows: I,
        format: DecimalFormat,
        options: MatrixOptions,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = f64>,
    {
        let rows = collect_numeric_rows(rows);
        let shape = matrix_shape(&rows)?;
        Ok(Self(publish_numeric_matrix(
            MatrixPublisher::Live(live),
            backend,
            rows.into_iter().flatten().collect(),
            shape,
            format,
            options,
        )?))
    }
    pub fn matrix(&self) -> &Matrix {
        &self.0
    }
    pub fn into_matrix(self) -> Matrix {
        self.0
    }
}

impl IntegerMatrix {
    pub fn from_rows<I, J>(
        scene: &mut Scene,
        backend: &mut impl LatexBackend,
        rows: I,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = f64>,
    {
        Self::from_rows_with_options(scene, backend, rows, MatrixOptions::default())
    }
    pub fn from_rows_with_options<I, J>(
        scene: &mut Scene,
        backend: &mut impl LatexBackend,
        rows: I,
        options: MatrixOptions,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = f64>,
    {
        let rows = collect_numeric_rows(rows);
        let shape = matrix_shape(&rows)?;
        Ok(Self(publish_numeric_matrix(
            MatrixPublisher::Scene(scene),
            backend,
            rows.into_iter().flatten().collect(),
            shape,
            integer_format(),
            options,
        )?))
    }
    pub fn from_rows_in_store<I, J>(
        store: Rc<RefCell<noon_core::SemanticStore>>,
        backend: &mut impl LatexBackend,
        rows: I,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = f64>,
    {
        Self::from_rows_in_store_with_options(store, backend, rows, MatrixOptions::default())
    }
    pub fn from_rows_in_store_with_options<I, J>(
        store: Rc<RefCell<noon_core::SemanticStore>>,
        backend: &mut impl LatexBackend,
        rows: I,
        options: MatrixOptions,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = f64>,
    {
        let rows = collect_numeric_rows(rows);
        let shape = matrix_shape(&rows)?;
        Ok(Self(publish_numeric_matrix(
            MatrixPublisher::Store(store),
            backend,
            rows.into_iter().flatten().collect(),
            shape,
            integer_format(),
            options,
        )?))
    }
    pub fn from_rows_in_live_session<I, J>(
        live: &mut crate::LiveSession<'_>,
        backend: &mut impl LatexBackend,
        rows: I,
        options: MatrixOptions,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = f64>,
    {
        let rows = collect_numeric_rows(rows);
        let shape = matrix_shape(&rows)?;
        Ok(Self(publish_numeric_matrix(
            MatrixPublisher::Live(live),
            backend,
            rows.into_iter().flatten().collect(),
            shape,
            integer_format(),
            options,
        )?))
    }
    pub fn matrix(&self) -> &Matrix {
        &self.0
    }
    pub fn into_matrix(self) -> Matrix {
        self.0
    }
}

fn integer_format() -> DecimalFormat {
    DecimalFormat {
        decimal_places: 0,
        ..Default::default()
    }
}
fn collect_numeric_rows<I, J>(rows: I) -> Vec<Vec<f64>>
where
    I: IntoIterator<Item = J>,
    J: IntoIterator<Item = f64>,
{
    rows.into_iter()
        .map(|row| row.into_iter().collect())
        .collect()
}

impl MobjectMatrix {
    /// Construct a matrix from retained object or family roots. A family remains
    /// one matrix entry and moves its ordered leaves atomically.
    pub fn from_target_rows<'a, I, J>(
        scene: &mut Scene,
        backend: &mut impl LatexBackend,
        rows: I,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = MobjectTarget<'a>>,
    {
        Self::from_target_rows_with_options(scene, backend, rows, MatrixOptions::default())
    }
    pub fn from_target_rows_with_options<'a, I, J>(
        scene: &mut Scene,
        backend: &mut impl LatexBackend,
        rows: I,
        options: MatrixOptions,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = MobjectTarget<'a>>,
    {
        let rows = collect_target_rows(rows);
        let shape = matrix_shape(&rows)?;
        Ok(Self(publish_target_mobject_matrix(
            MatrixPublisher::Scene(scene),
            backend,
            rows.into_iter().flatten().collect(),
            shape,
            options,
        )?))
    }
    pub fn from_target_rows_in_store<'a, I, J>(
        store: Rc<RefCell<noon_core::SemanticStore>>,
        backend: &mut impl LatexBackend,
        rows: I,
        options: MatrixOptions,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = MobjectTarget<'a>>,
    {
        let rows = collect_target_rows(rows);
        let shape = matrix_shape(&rows)?;
        Ok(Self(publish_target_mobject_matrix(
            MatrixPublisher::Store(store),
            backend,
            rows.into_iter().flatten().collect(),
            shape,
            options,
        )?))
    }
    pub fn from_target_rows_in_live_session<'a, I, J>(
        live: &mut crate::LiveSession<'_>,
        backend: &mut impl LatexBackend,
        rows: I,
        options: MatrixOptions,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = MobjectTarget<'a>>,
    {
        let rows = collect_target_rows(rows);
        let shape = matrix_shape(&rows)?;
        Ok(Self(publish_target_mobject_matrix(
            MatrixPublisher::Live(live),
            backend,
            rows.into_iter().flatten().collect(),
            shape,
            options,
        )?))
    }
    pub fn from_rows<I, J>(
        scene: &mut Scene,
        backend: &mut impl LatexBackend,
        rows: I,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = Mobject>,
    {
        Self::from_rows_with_options(scene, backend, rows, MatrixOptions::default())
    }
    pub fn from_rows_with_options<I, J>(
        scene: &mut Scene,
        backend: &mut impl LatexBackend,
        rows: I,
        options: MatrixOptions,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = Mobject>,
    {
        let rows = collect_mobject_rows(rows);
        let shape = matrix_shape(&rows)?;
        let store = Rc::clone(scene.integration_store());
        validate_entries(&store, &rows)?;
        Ok(Self(publish_existing_mobject_matrix(
            MatrixPublisher::Scene(scene),
            backend,
            rows.into_iter().flatten().collect(),
            shape,
            options,
        )?))
    }
    pub fn from_rows_in_store<I, J>(
        store: Rc<RefCell<noon_core::SemanticStore>>,
        backend: &mut impl LatexBackend,
        rows: I,
        options: MatrixOptions,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = Mobject>,
    {
        let rows = collect_mobject_rows(rows);
        let shape = matrix_shape(&rows)?;
        validate_entries(&store, &rows)?;
        Ok(Self(publish_existing_mobject_matrix(
            MatrixPublisher::Store(store),
            backend,
            rows.into_iter().flatten().collect(),
            shape,
            options,
        )?))
    }
    pub fn from_rows_in_live_session<I, J>(
        live: &mut crate::LiveSession<'_>,
        backend: &mut impl LatexBackend,
        rows: I,
        options: MatrixOptions,
    ) -> Result<Self, MatrixAuthoringError>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = Mobject>,
    {
        let rows = collect_mobject_rows(rows);
        let shape = matrix_shape(&rows)?;
        let store = Rc::clone(live.integration_store());
        validate_entries(&store, &rows)?;
        Ok(Self(publish_existing_mobject_matrix(
            MatrixPublisher::Live(live),
            backend,
            rows.into_iter().flatten().collect(),
            shape,
            options,
        )?))
    }
    pub fn matrix(&self) -> &Matrix {
        &self.0
    }
    pub fn into_matrix(self) -> Matrix {
        self.0
    }
}
fn collect_mobject_rows<I, J>(rows: I) -> Vec<Vec<Mobject>>
where
    I: IntoIterator<Item = J>,
    J: IntoIterator<Item = Mobject>,
{
    rows.into_iter()
        .map(|row| row.into_iter().collect())
        .collect()
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
fn validate_entries(
    store: &Rc<RefCell<noon_core::SemanticStore>>,
    rows: &[Vec<Mobject>],
) -> Result<(), MatrixAuthoringError> {
    let mut seen = std::collections::BTreeSet::new();
    for entry in rows.iter().flatten() {
        if !Rc::ptr_eq(entry.integration_store(), store) {
            return Err(AuthoringError::ForeignStore.into());
        }
        entry.validate()?;
        if !seen.insert(entry.node_id()) {
            return Err(MatrixAuthoringError::DuplicateEntry);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
