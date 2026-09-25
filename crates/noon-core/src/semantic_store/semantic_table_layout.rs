//! Sparse authored Table layout declarations on ordinary family roots.

use crate::SemanticNodeId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticTableLayout {
    h_buff: f64,
    v_buff: f64,
    label_buff: f64,
    include_outer_lines: bool,
}
impl SemanticTableLayout {
    pub const fn new(h_buff: f64, v_buff: f64, label_buff: f64, include_outer_lines: bool) -> Self {
        Self {
            h_buff,
            v_buff,
            label_buff,
            include_outer_lines,
        }
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
    pub fn is_valid(self) -> bool {
        [self.h_buff, self.v_buff, self.label_buff]
            .into_iter()
            .all(|v| v.is_finite() && v >= 0.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticTransactionTableLayout {
    layout: SemanticTableLayout,
}
impl SemanticTransactionTableLayout {
    pub const fn new(layout: SemanticTableLayout) -> Self {
        Self { layout }
    }
    pub const fn layout(self) -> SemanticTableLayout {
        self.layout
    }
}

pub type SemanticTableLayoutDeclarations =
    std::collections::HashMap<SemanticNodeId, SemanticTableLayout>;
