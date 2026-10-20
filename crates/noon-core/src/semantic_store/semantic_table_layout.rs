//! Sparse authored Table layout declarations on ordinary family roots.

use crate::{Color, StrokeCap, StrokeJoin};

#[derive(Clone, Copy, Debug, PartialEq)]
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
}
impl SemanticTableLayout {
    pub const fn new(h_buff: f64, v_buff: f64, label_buff: f64, include_outer_lines: bool) -> Self {
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
        }
    }
    pub const fn with_inner_lines(mut self, include_inner_lines: bool) -> Self {
        self.include_inner_lines = include_inner_lines;
        self
    }
    pub const fn with_line_style(mut self, color: Color, width: f64, opacity: f64) -> Self {
        self.line_stroke_color = color;
        self.line_stroke_width = width;
        self.line_stroke_opacity = opacity;
        self
    }
    pub const fn with_line_geometry(mut self, join: StrokeJoin, cap: StrokeCap) -> Self {
        self.line_stroke_join = join;
        self.line_stroke_cap = cap;
        self
    }
    pub const fn with_cell_alignment(mut self, x: f64, y: f64) -> Self {
        self.cell_alignment_x = x;
        self.cell_alignment_y = y;
        self
    }
    pub const fn h_buff(self) -> f64 {
        self.h_buff
    }
    pub const fn v_buff(self) -> f64 {
        self.v_buff
    }
    pub const fn label_buff(self) -> f64 {
        self.label_buff
    }
    pub const fn include_outer_lines(self) -> bool {
        self.include_outer_lines
    }
    pub const fn include_inner_lines(self) -> bool {
        self.include_inner_lines
    }
    pub const fn line_stroke_color(self) -> Color {
        self.line_stroke_color
    }
    pub const fn line_stroke_width(self) -> f64 {
        self.line_stroke_width
    }
    pub const fn line_stroke_opacity(self) -> f64 {
        self.line_stroke_opacity
    }
    pub const fn line_stroke_join(self) -> StrokeJoin {
        self.line_stroke_join
    }
    pub const fn line_stroke_cap(self) -> StrokeCap {
        self.line_stroke_cap
    }
    pub const fn cell_alignment_x(self) -> f64 {
        self.cell_alignment_x
    }
    pub const fn cell_alignment_y(self) -> f64 {
        self.cell_alignment_y
    }
    /// Apply the world-axis factors from a direct Table family scale.
    ///
    /// This declaration follows Manim's `Table.scale`: horizontal and vertical
    /// cell buffers remain table-owned spacing and scale with the table itself.
    /// `label_buff` is not part of that override.
    pub const fn scaled_buffers(self, x: f64, y: f64) -> Self {
        Self {
            h_buff: self.h_buff * x,
            v_buff: self.v_buff * y,
            ..self
        }
    }
    pub fn is_valid(self) -> bool {
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
            && self.cell_alignment_x.is_finite()
            && self.cell_alignment_y.is_finite()
            && self.line_stroke_opacity <= 1.0
            && self.line_stroke_color.red <= 1.0
            && self.line_stroke_color.green <= 1.0
            && self.line_stroke_color.blue <= 1.0
            && self.line_stroke_color.alpha <= 1.0
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
        tx.set_table_layout(root, original);
        tx.set_table_layout(root, changed);
        let result = tx.apply(&mut store).unwrap();
        let root = result.resolve(root).unwrap();
        assert_eq!(store.semantic_table_layout(root).unwrap(), Some(changed));

        // Returning to the value in the base store must still publish the
        // second write, since the first write changes the transaction overlay.
        let mut tx = SemanticMutationTransaction::new();
        tx.set_table_layout(root, original);
        tx.set_table_layout(root, changed);
        tx.apply(&mut store).unwrap();
        assert_eq!(store.semantic_table_layout(root).unwrap(), Some(changed));
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
}
