//! Grid policy and sparse cell geometry for the shared family arrangement transaction.
use crate::{AuthoringError, Bounds2D64};
use std::collections::BTreeMap;

/// Order in which direct family members fill a grid.
#[derive(Clone, Copy, Debug, Default)]
pub enum GridFlow {
    #[default]
    RightDown,
    DownRight,
    LeftDown,
    DownLeft,
    RightUp,
    UpRight,
    LeftUp,
    UpLeft,
}

impl std::str::FromStr for GridFlow {
    type Err = AuthoringError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "rd" => Ok(Self::RightDown),
            "dr" => Ok(Self::DownRight),
            "ld" => Ok(Self::LeftDown),
            "dl" => Ok(Self::DownLeft),
            "ru" => Ok(Self::RightUp),
            "ur" => Ok(Self::UpRight),
            "lu" => Ok(Self::LeftUp),
            "ul" => Ok(Self::UpLeft),
            _ => Err(AuthoringError::InvalidGridOption("flow_order")),
        }
    }
}

/// Manim-compatible grid placement. Rows are specified top to bottom, columns
/// left to right; alignment characters are `u/c/d` and `l/c/r`, respectively.
/// Explicit dimensions take precedence over lengths inferred from options.
#[derive(Clone, Debug)]
pub struct FamilyGridOptions {
    pub rows: Option<usize>,
    pub columns: Option<usize>,
    pub gap: (f64, f64),
    pub cell_alignment: (f64, f64),
    pub row_alignments: Option<String>,
    pub column_alignments: Option<String>,
    pub row_heights: Option<Vec<Option<f64>>>,
    pub column_widths: Option<Vec<Option<f64>>>,
    pub flow: GridFlow,
}

impl Default for FamilyGridOptions {
    fn default() -> Self {
        Self {
            rows: None,
            columns: None,
            gap: (0.25, 0.25),
            cell_alignment: (0.0, 0.0),
            row_alignments: None,
            column_alignments: None,
            row_heights: None,
            column_widths: None,
            flow: GridFlow::default(),
        }
    }
}

pub(crate) struct GridPlan {
    options: FamilyGridOptions,
    rows: usize,
    columns: usize,
}

impl GridPlan {
    pub(crate) fn new(options: &FamilyGridOptions, count: usize) -> Result<Self, AuthoringError> {
        let infer = |count: Option<usize>,
                     alignments: &Option<String>,
                     sizes: &Option<Vec<Option<f64>>>| {
            count
                .or_else(|| alignments.as_ref().map(String::len))
                .or_else(|| sizes.as_ref().map(Vec::len))
        };
        let mut rows = infer(options.rows, &options.row_alignments, &options.row_heights);
        let mut columns = infer(
            options.columns,
            &options.column_alignments,
            &options.column_widths,
        );
        if rows == Some(0) || columns == Some(0) {
            return Err(AuthoringError::InvalidGridDimensions { rows, columns });
        }
        if rows.is_none() && columns.is_none() {
            columns = Some((count as f64).sqrt().ceil().max(1.0) as usize);
        }
        if rows.is_none() {
            rows = Some(count.div_ceil(columns.unwrap()).max(1));
        }
        if columns.is_none() {
            columns = Some(count.div_ceil(rows.unwrap()).max(1));
        }
        let (rows, columns) = (rows.unwrap(), columns.unwrap());
        if count.div_ceil(columns) > rows {
            return Err(AuthoringError::InsufficientGridCapacity {
                rows: Some(rows),
                columns,
                members: count,
            });
        }
        for (name, value) in [
            ("grid horizontal gap", options.gap.0),
            ("grid vertical gap", options.gap.1),
            ("grid horizontal alignment", options.cell_alignment.0),
            ("grid vertical alignment", options.cell_alignment.1),
        ] {
            crate::semantic_mobject::authoring_render_f64(name, value)?;
        }
        for (name, len, alignments, valid, sizes) in [
            (
                "row",
                rows,
                &options.row_alignments,
                "ucd",
                &options.row_heights,
            ),
            (
                "column",
                columns,
                &options.column_alignments,
                "lcr",
                &options.column_widths,
            ),
        ] {
            if let Some(a) = alignments {
                if a.len() != len || !a.chars().all(|c| valid.contains(c)) {
                    return Err(AuthoringError::InvalidGridOption(name));
                }
            }
            if let Some(sizes) = sizes {
                if !sizes.is_empty() && sizes.len() != len {
                    return Err(AuthoringError::InvalidGridOption(name));
                }
                for &size in sizes.iter().flatten() {
                    crate::semantic_mobject::authoring_render_f64("grid cell size", size)?;
                }
            }
        }
        Ok(Self {
            options: options.clone(),
            rows,
            columns,
        })
    }

    /// Bottom-up row/left-to-right column coordinates, matching upstream commit order.
    pub(crate) fn cell(&self, index: usize) -> (usize, usize) {
        let (vertical, left, down) = match self.options.flow {
            GridFlow::RightDown => (false, false, true),
            GridFlow::DownRight => (true, false, true),
            GridFlow::LeftDown => (false, true, true),
            GridFlow::DownLeft => (true, true, true),
            GridFlow::RightUp => (false, false, false),
            GridFlow::UpRight => (true, false, false),
            GridFlow::LeftUp => (false, true, false),
            GridFlow::UpLeft => (true, true, false),
        };
        let (r, c) = if vertical {
            (index % self.rows, index / self.rows)
        } else {
            (index / self.columns, index % self.columns)
        };
        (
            if down { self.rows - 1 - r } else { r },
            if left { self.columns - 1 - c } else { c },
        )
    }

    pub(crate) fn targets(
        &self,
        cells: impl Iterator<Item = ((usize, usize), Option<Bounds2D64>)>,
    ) -> BTreeMap<(usize, usize), (Bounds2D64, (f64, f64))> {
        let mut widths = BTreeMap::<usize, f64>::new();
        let mut heights = BTreeMap::<usize, f64>::new();
        let cells: Vec<_> = cells
            .map(|(cell, bounds)| {
                let w = widths.entry(cell.1).or_default();
                let h = heights.entry(cell.0).or_default();
                if let Some(b) = bounds {
                    *w = w.max(b.width());
                    *h = h.max(b.height());
                }
                cell
            })
            .collect();
        let override_sizes = |measured: &mut BTreeMap<usize, f64>,
                              sizes: &Option<Vec<Option<f64>>>,
                              reverse: bool,
                              count: usize| {
            if let Some(sizes) = sizes {
                for (index, value) in sizes.iter().enumerate() {
                    if let Some(value) = value {
                        measured.insert(if reverse { count - 1 - index } else { index }, *value);
                    }
                }
            }
        };
        let first_column = widths.first_key_value().map_or(0, |(&index, _)| index);
        let first_row = heights.first_key_value().map_or(0, |(&index, _)| index);
        override_sizes(
            &mut widths,
            &self.options.column_widths,
            false,
            self.columns,
        );
        override_sizes(&mut heights, &self.options.row_heights, true, self.rows);
        let edges = |sizes: BTreeMap<usize, f64>, gap: f64, first: usize| {
            let mut sum = 0.0;
            sizes
                .into_iter()
                .filter(|(index, _)| *index >= first)
                .map(|(index, size)| {
                    // Omit a common offset below the first occupied cell. The
                    // final family centering removes it, and this avoids losing
                    // small cell extents beside huge unused grid capacities.
                    let start = sum + (index - first) as f64 * gap;
                    sum += size;
                    (index, (start.min(start + size), start.max(start + size)))
                })
                .collect::<BTreeMap<_, _>>()
        };
        let xs = edges(widths, self.options.gap.0, first_column);
        let ys = edges(heights, self.options.gap.1, first_row);
        cells
            .into_iter()
            .map(|(r, c)| {
                // Match pinned Manim's axis-specific fallback when only one explicit
                // alignment list overrides cell_alignment.
                let (row_x, row_y) = self.options.row_alignments.as_ref().map_or(
                    (self.options.cell_alignment.0, 0.0),
                    |a| {
                        (
                            0.0,
                            match a.as_bytes()[self.rows - 1 - r] {
                                b'u' => 1.0,
                                b'd' => -1.0,
                                _ => 0.0,
                            },
                        )
                    },
                );
                let (col_x, col_y) = self.options.column_alignments.as_ref().map_or(
                    (0.0, self.options.cell_alignment.1),
                    |a| {
                        (
                            match a.as_bytes()[c] {
                                b'l' => -1.0,
                                b'r' => 1.0,
                                _ => 0.0,
                            },
                            0.0,
                        )
                    },
                );
                (
                    (r, c),
                    (
                        Bounds2D64 {
                            min_x: xs[&c].0,
                            max_x: xs[&c].1,
                            min_y: ys[&r].0,
                            max_y: ys[&r].1,
                        },
                        (row_x + col_x, row_y + col_y),
                    ),
                )
            })
            .collect()
    }
}
