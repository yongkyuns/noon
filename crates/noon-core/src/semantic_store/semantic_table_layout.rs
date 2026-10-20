//! Sparse authored Table layout declarations on ordinary family roots.

use crate::{Color, SemanticNodeId, SemanticTransactionNodeRef, StrokeCap, StrokeJoin};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticTableLayout {
    h_buff: f64,
    v_buff: f64,
    label_buff: f64,
    include_outer_lines: bool,
    include_inner_lines: bool,
    line_stroke_color: Color,
    line_stroke_width: f64,
    line_stroke_opacity: f64,
    line_stroke_join: StrokeJoin,
    line_stroke_cap: StrokeCap,
    cell_alignment_x: f64,
    cell_alignment_y: f64,
    structure: Option<Arc<SemanticTableStructure>>,
}
impl SemanticTableLayout {
    pub fn new(h_buff: f64, v_buff: f64, label_buff: f64, include_outer_lines: bool) -> Self {
        Self {
            h_buff,
            v_buff,
            label_buff,
            include_outer_lines,
            include_inner_lines: true,
            line_stroke_color: Color::WHITE,
            line_stroke_width: 0.04,
            line_stroke_opacity: 1.0,
            line_stroke_join: StrokeJoin::Miter,
            line_stroke_cap: StrokeCap::Butt,
            cell_alignment_x: 0.0,
            cell_alignment_y: 0.0,
            structure: None,
        }
    }
    pub fn with_inner_lines(mut self, include_inner_lines: bool) -> Self {
        self.include_inner_lines = include_inner_lines;
        self
    }
    pub fn with_line_style(mut self, color: Color, width: f64, opacity: f64) -> Self {
        self.line_stroke_color = color;
        self.line_stroke_width = width;
        self.line_stroke_opacity = opacity;
        self
    }
    pub fn with_line_geometry(mut self, join: StrokeJoin, cap: StrokeCap) -> Self {
        self.line_stroke_join = join;
        self.line_stroke_cap = cap;
        self
    }
    pub fn with_cell_alignment(mut self, x: f64, y: f64) -> Self {
        self.cell_alignment_x = x;
        self.cell_alignment_y = y;
        self
    }
    pub fn with_structure(mut self, structure: Option<SemanticTableStructure>) -> Self {
        self.structure = structure.map(Arc::new);
        self
    }
    pub fn structure(&self) -> Option<&SemanticTableStructure> {
        self.structure.as_deref()
    }
    pub fn shared_structure(&self) -> Option<Arc<SemanticTableStructure>> {
        self.structure.clone()
    }
    pub fn has_row_labels(&self) -> bool {
        self.structure()
            .is_some_and(|grid| grid.row_labels.is_some())
    }
    pub fn has_column_labels(&self) -> bool {
        self.structure()
            .is_some_and(|grid| grid.column_labels.is_some())
    }
    pub fn has_top_left_entry(&self) -> bool {
        self.structure().is_some_and(|grid| {
            grid.row_labels.is_some()
                && grid.column_labels.is_some()
                && grid.top_left_entry.is_some()
        })
    }
    pub fn referenced_nodes(&self) -> impl Iterator<Item = SemanticNodeId> + '_ {
        self.structure
            .iter()
            .flat_map(|grid| grid.referenced_nodes())
    }
    pub fn h_buff(&self) -> f64 {
        self.h_buff
    }
    pub fn v_buff(&self) -> f64 {
        self.v_buff
    }
    pub fn label_buff(&self) -> f64 {
        self.label_buff
    }
    pub fn include_outer_lines(&self) -> bool {
        self.include_outer_lines
    }
    pub fn include_inner_lines(&self) -> bool {
        self.include_inner_lines
    }
    pub fn line_stroke_color(&self) -> Color {
        self.line_stroke_color
    }
    pub fn line_stroke_width(&self) -> f64 {
        self.line_stroke_width
    }
    pub fn line_stroke_opacity(&self) -> f64 {
        self.line_stroke_opacity
    }
    pub fn line_stroke_join(&self) -> StrokeJoin {
        self.line_stroke_join
    }
    pub fn line_stroke_cap(&self) -> StrokeCap {
        self.line_stroke_cap
    }
    pub fn cell_alignment_x(&self) -> f64 {
        self.cell_alignment_x
    }
    pub fn cell_alignment_y(&self) -> f64 {
        self.cell_alignment_y
    }
    /// Apply the world-axis factors from a direct Table family scale.
    ///
    /// This declaration follows Manim's `Table.scale`: horizontal and vertical
    /// cell buffers remain table-owned spacing and scale with the table itself.
    /// `label_buff` is not part of that override.
    pub fn scaled_buffers(self, x: f64, y: f64) -> Self {
        Self {
            h_buff: self.h_buff * x,
            v_buff: self.v_buff * y,
            ..self
        }
    }
    pub fn is_valid(&self) -> bool {
        [
            self.h_buff,
            self.v_buff,
            self.label_buff,
            self.line_stroke_width,
            self.line_stroke_opacity,
            f64::from(self.line_stroke_color.red),
            f64::from(self.line_stroke_color.green),
            f64::from(self.line_stroke_color.blue),
            f64::from(self.line_stroke_color.alpha),
        ]
        .into_iter()
        .all(|v| v.is_finite() && v >= 0.0)
            && self.structure.as_ref().is_none_or(|grid| grid.is_valid())
            && self.cell_alignment_x.is_finite()
            && self.cell_alignment_y.is_finite()
            && self.line_stroke_opacity <= 1.0
            && self.line_stroke_color.red <= 1.0
            && self.line_stroke_color.green <= 1.0
            && self.line_stroke_color.blue <= 1.0
            && self.line_stroke_color.alpha <= 1.0
    }
}

/// Logical table entries retain aliases and non-painted identities independently
/// of the ordinary family projection used for drawing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticTableStructure<N = SemanticNodeId> {
    body: Vec<Vec<N>>,
    row_labels: Option<Vec<N>>,
    column_labels: Option<Vec<N>>,
    top_left_entry: Option<N>,
    corner_placeholder: Option<N>,
}
impl<N: Copy> SemanticTableStructure<N> {
    pub fn new(
        body: Vec<Vec<N>>,
        row_labels: Option<Vec<N>>,
        column_labels: Option<Vec<N>>,
        top_left_entry: Option<N>,
        corner_placeholder: Option<N>,
    ) -> Self {
        Self {
            body,
            row_labels,
            column_labels,
            top_left_entry,
            corner_placeholder,
        }
    }
    pub fn body_rows(&self) -> &[Vec<N>] {
        &self.body
    }
    pub fn row_labels(&self) -> Option<&[N]> {
        self.row_labels.as_deref()
    }
    pub fn column_labels(&self) -> Option<&[N]> {
        self.column_labels.as_deref()
    }
    pub fn top_left_entry(&self) -> Option<N> {
        self.top_left_entry
    }
    pub fn corner_placeholder(&self) -> Option<N> {
        self.corner_placeholder
    }
    pub fn referenced_nodes(&self) -> impl Iterator<Item = N> + '_ {
        self.body
            .iter()
            .flatten()
            .copied()
            .chain(self.row_labels.iter().flatten().copied())
            .chain(self.column_labels.iter().flatten().copied())
            .chain(self.top_left_entry)
            .chain(self.corner_placeholder)
    }
    fn is_valid(&self) -> bool {
        let Some(first) = self.body.first().filter(|row| !row.is_empty()) else {
            return false;
        };
        self.body.iter().all(|row| row.len() == first.len())
            && self
                .row_labels
                .as_ref()
                .is_none_or(|rows| rows.len() == self.body.len())
            && self
                .column_labels
                .as_ref()
                .is_none_or(|columns| columns.len() == first.len())
            && self.corner_placeholder.is_some()
                == (self.row_labels.is_some()
                    && self.column_labels.is_some()
                    && self.top_left_entry.is_none())
    }
    pub fn map_nodes<M: Copy>(&self, mut map: impl FnMut(N) -> M) -> SemanticTableStructure<M> {
        SemanticTableStructure {
            body: self
                .body
                .iter()
                .map(|row| row.iter().copied().map(&mut map).collect())
                .collect(),
            row_labels: self
                .row_labels
                .as_ref()
                .map(|rows| rows.iter().copied().map(&mut map).collect()),
            column_labels: self
                .column_labels
                .as_ref()
                .map(|columns| columns.iter().copied().map(&mut map).collect()),
            top_left_entry: self.top_left_entry.map(&mut map),
            corner_placeholder: self.corner_placeholder.map(map),
        }
    }
}

/// Transaction-local references resolve into the same semantic table component.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticTransactionTableLayout {
    layout: SemanticTableLayout,
    structure: Option<Arc<SemanticTableStructure<SemanticTransactionNodeRef>>>,
}
impl From<SemanticTableLayout> for SemanticTransactionTableLayout {
    fn from(layout: SemanticTableLayout) -> Self {
        Self::map_existing(layout, Into::into)
    }
}
impl SemanticTransactionTableLayout {
    pub fn new(
        layout: SemanticTableLayout,
        structure: Option<SemanticTableStructure<SemanticTransactionNodeRef>>,
    ) -> Self {
        Self {
            layout: layout.with_structure(None),
            structure: structure.map(Arc::new),
        }
    }
    pub fn map_existing(
        layout: SemanticTableLayout,
        map: impl FnMut(SemanticNodeId) -> SemanticTransactionNodeRef,
    ) -> Self {
        let structure = layout.structure().map(|grid| grid.map_nodes(map));
        Self::new(layout, structure)
    }
    pub(crate) fn node_references(&self) -> impl Iterator<Item = SemanticTransactionNodeRef> + '_ {
        self.structure
            .iter()
            .flat_map(|grid| grid.referenced_nodes())
    }
    pub(crate) fn placeholder(&self) -> Option<SemanticTransactionNodeRef> {
        self.structure
            .as_ref()
            .and_then(|grid| grid.corner_placeholder())
    }
    pub(crate) fn is_valid(&self) -> bool {
        self.layout.is_valid() && self.structure.as_ref().is_none_or(|grid| grid.is_valid())
    }
    pub(crate) fn resolve(
        self,
        resolve: impl FnMut(SemanticTransactionNodeRef) -> SemanticNodeId,
    ) -> SemanticTableLayout {
        self.layout
            .with_structure(self.structure.map(|grid| grid.map_nodes(resolve)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SemanticMutationTransaction, SemanticNodeCreation, SemanticStore};

    #[test]
    fn repeated_layout_writes_observe_prior_transaction_values() {
        let mut store = SemanticStore::new();
        let original = SemanticTableLayout::new(1.0, 1.0, 0.5, false);
        let changed = SemanticTableLayout::new(2.0, 0.5, 0.25, true).with_inner_lines(false);
        let changed = changed.with_cell_alignment(-1.0, 1.0);
        let mut tx = SemanticMutationTransaction::new();
        let root = tx.create_node(SemanticNodeCreation::family());
        tx.set_table_layout(root, original.clone());
        tx.set_table_layout(root, changed.clone());
        let result = tx.apply(&mut store).unwrap();
        let root = result.resolve(root).unwrap();
        assert_eq!(
            store.semantic_table_layout(root).unwrap(),
            Some(changed.clone())
        );

        // Returning to the value in the base store must still publish the
        // second write, since the first write changes the transaction overlay.
        let mut tx = SemanticMutationTransaction::new();
        tx.set_table_layout(root, original.clone());
        tx.set_table_layout(root, changed.clone());
        tx.apply(&mut store).unwrap();
        assert_eq!(
            store.semantic_table_layout(root).unwrap(),
            Some(changed.clone())
        );
        let retained = store.semantic_table_layout(root).unwrap().unwrap();
        assert_eq!(retained.cell_alignment_x(), -1.0);
        assert_eq!(retained.cell_alignment_y(), 1.0);
    }

    #[test]
    fn invalid_owner_or_value_rolls_back_and_retirement_releases_layout() {
        let mut store = SemanticStore::new();
        let mut tx = SemanticMutationTransaction::new();
        let object = tx.create_node(SemanticNodeCreation::object(
            crate::SemanticObjectState::new(crate::StoredGeometry::Circle { radius: 1.0 }),
        ));
        tx.set_table_layout(object, SemanticTableLayout::new(1.0, 1.0, 0.5, false));
        assert!(tx.apply(&mut store).is_err());

        let mut tx = SemanticMutationTransaction::new();
        let root = tx.create_node(SemanticNodeCreation::family());
        tx.set_table_layout(root, SemanticTableLayout::new(f64::NAN, 1.0, 0.5, false));
        assert!(tx.apply(&mut store).is_err());

        let mut tx = SemanticMutationTransaction::new();
        let root = tx.create_node(SemanticNodeCreation::family());
        tx.set_table_layout(
            root,
            SemanticTableLayout::new(2.0, 1.0, 0.5, true).with_inner_lines(false),
        );
        let result = tx.apply(&mut store).unwrap();
        let root = result.resolve(root).unwrap();
        assert_eq!(
            store.semantic_table_layout(root).unwrap().unwrap().h_buff(),
            2.0
        );
        store.remove_node(root).unwrap();
        assert!(store.semantic_table_layout(root).is_err());
    }
    #[test]
    fn pending_grid_references_resolve_and_retire_their_owner() {
        let mut store = SemanticStore::new();
        let mut tx = SemanticMutationTransaction::new();
        let root = tx.create_node(SemanticNodeCreation::family());
        let entry = tx.create_node(SemanticNodeCreation::object(
            crate::SemanticObjectState::new(crate::StoredGeometry::Circle { radius: 1.0 }),
        ));
        let corner = tx.create_node(SemanticNodeCreation::object(
            crate::SemanticObjectState::new(crate::StoredGeometry::Circle { radius: 0.0 }),
        ));
        tx.set_table_layout(
            root,
            SemanticTransactionTableLayout::new(
                SemanticTableLayout::new(1.0, 1.0, 0.5, false),
                Some(SemanticTableStructure::new(
                    vec![vec![entry.into()]],
                    Some(vec![entry.into()]),
                    Some(vec![entry.into()]),
                    None,
                    Some(corner.into()),
                )),
            ),
        );
        let result = tx.apply(&mut store).unwrap();
        let root = result.resolve(root).unwrap();
        let entry = result.resolve(entry).unwrap();
        let corner = result.resolve(corner).unwrap();
        let layout = store.semantic_table_layout(root).unwrap().unwrap();
        assert_eq!(layout.structure().unwrap().body_rows(), &[vec![entry]]);
        assert_eq!(
            layout.structure().unwrap().corner_placeholder(),
            Some(corner)
        );
        // The corner is outside paint membership, but remains a hard identity
        // dependency. Retiring it must never leave a query with a dangling ID.
        let mut tx = SemanticMutationTransaction::new();
        tx.remove_node(corner);
        tx.apply(&mut store).unwrap();
        assert!(store.node(root).is_none());
    }

    #[test]
    fn replacing_grid_dependencies_releases_old_reverse_references() {
        let mut store = SemanticStore::new();
        let mut tx = SemanticMutationTransaction::new();
        let root = tx.create_node(SemanticNodeCreation::family());
        let old = tx.create_node(SemanticNodeCreation::family());
        let new = tx.create_node(SemanticNodeCreation::family());
        let result = tx.apply(&mut store).unwrap();
        let root = result.resolve(root).unwrap();
        let old = result.resolve(old).unwrap();
        let new = result.resolve(new).unwrap();
        for entry in [old, new] {
            let layout = SemanticTableLayout::new(1.0, 1.0, 0.5, false).with_structure(Some(
                SemanticTableStructure::new(vec![vec![entry]], None, None, None, None),
            ));
            let mut tx = SemanticMutationTransaction::new();
            tx.set_table_layout(root, layout);
            tx.apply(&mut store).unwrap();
        }
        let mut tx = SemanticMutationTransaction::new();
        tx.remove_node(old);
        tx.apply(&mut store).unwrap();
        assert!(store.node(root).is_some());
        let mut tx = SemanticMutationTransaction::new();
        tx.remove_node(new);
        tx.apply(&mut store).unwrap();
        assert!(store.node(root).is_none());
    }
}
