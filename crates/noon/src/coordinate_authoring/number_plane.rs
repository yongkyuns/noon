//! Retained NumberPlane semantics over the shared coordinate transaction substrate.
use super::*;
use noon_core::{StrokeCap, StrokeJoin, BLUE_D};

/// Inert Manim-compatible cartesian plane request.  The result contains only
/// ordinary line objects and families; it does not introduce a grid primitive.
#[derive(Clone, Debug)]
pub struct ManimNumberPlaneOptions {
    pub x_range: [f64; 3],
    pub y_range: [f64; 3],
    pub x_length: Option<f64>,
    pub y_length: Option<f64>,
    pub axis_style: SemanticStyle,
    pub background_line_style: SemanticStyle,
    pub faded_line_style: Option<SemanticStyle>,
    pub faded_line_ratio: u32,
    /// Fixed admission cap for all retained background-line leaves.
    pub line_limit: usize,
}

impl ManimNumberPlaneOptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Manim's explicit faded-line dictionary starts from the ordinary Line
    /// defaults; omitted faded style instead derives from the background style.
    pub fn default_faded_line_style() -> SemanticStyle {
        SemanticStyle {
            fill: None,
            stroke: Some(SemanticPaint::Solid(WHITE)),
            stroke_width: 0.04,
            stroke_width_mode: StrokeWidthMode::ScreenSpace,
            // Manim's AUTO cap/join leaves Cairo's butt/miter defaults.
            stroke_join: StrokeJoin::Miter,
            stroke_cap: StrokeCap::Butt,
            ..SemanticStyle::default()
        }
    }

    pub fn faded_line_style_mut(&mut self) -> &mut SemanticStyle {
        self.faded_line_style
            .get_or_insert_with(Self::default_faded_line_style)
    }
}

impl Default for ManimNumberPlaneOptions {
    fn default() -> Self {
        let background_line_style = SemanticStyle {
            fill: None,
            stroke: Some(SemanticPaint::Solid(BLUE_D)),
            stroke_width: 0.02,
            stroke_width_mode: StrokeWidthMode::ScreenSpace,
            stroke_join: StrokeJoin::Miter,
            stroke_cap: StrokeCap::Butt,
            ..SemanticStyle::default()
        };
        Self {
            x_range: [-64.0 / 9.0, 64.0 / 9.0, 1.0],
            y_range: [-4.0, 4.0, 1.0],
            x_length: None,
            y_length: None,
            axis_style: number_plane_axis_style(),
            background_line_style,
            faded_line_style: None,
            faded_line_ratio: 1,
            line_limit: 20_000,
        }
    }
}

fn number_plane_axis_style() -> SemanticStyle {
    let mut style = default_axis_style();
    // Manim's NumberPlane leaves Line's cap/join at Cairo's AUTO defaults.
    style.stroke_join = StrokeJoin::Miter;
    style.stroke_cap = StrokeCap::Butt;
    style
}

/// Handle to one ordinary grid/axes family; all topology remains semantic-owned.
#[derive(Clone, Debug)]
pub struct ManimNumberPlane {
    family: MobjectFamily,
}

impl ManimNumberPlane {
    pub fn create(
        store: Rc<RefCell<SemanticStore>>,
        options: &ManimNumberPlaneOptions,
    ) -> Result<Self, CoordinateAuthoringError> {
        let (transaction, root) = prepare_number_plane(options)?;
        let result = transaction
            .apply(&mut store.borrow_mut())
            .map_err(AuthoringError::from)?;
        Self::from_family(resolve_family(store, &result, root)?)
    }

    pub fn from_family(family: MobjectFamily) -> Result<Self, CoordinateAuthoringError> {
        let plane = Self { family };
        plane.faded_lines()?;
        plane.background_lines()?;
        plane.x_axis()?;
        plane.y_axis()?;
        Ok(plane)
    }

    pub fn family(&self) -> &MobjectFamily {
        &self.family
    }

    pub fn faded_lines(&self) -> Result<MobjectFamily, CoordinateAuthoringError> {
        self.grid_family(0)
    }

    pub fn background_lines(&self) -> Result<MobjectFamily, CoordinateAuthoringError> {
        self.grid_family(1)
    }

    pub fn x_axis(&self) -> Result<ManimNumberLine, CoordinateAuthoringError> {
        self.axis(2)
    }

    pub fn y_axis(&self) -> Result<ManimNumberLine, CoordinateAuthoringError> {
        self.axis(3)
    }

    pub fn authored_frame(&self) -> Result<AxesFrame, CoordinateAuthoringError> {
        Ok(AxesFrame::new(
            self.x_axis()?.authored_frame()?,
            self.y_axis()?.authored_frame()?,
        ))
    }

    pub fn effective_frame(
        &self,
        execution: &ExecutionSession,
    ) -> Result<AxesFrame, CoordinateAuthoringError> {
        Ok(AxesFrame::new(
            self.x_axis()?.effective_frame(execution)?,
            self.y_axis()?.effective_frame(execution)?,
        ))
    }

    fn grid_family(&self, index: usize) -> Result<MobjectFamily, CoordinateAuthoringError> {
        let id = plane_members(&self.family)?[index];
        MobjectFamily::from_node(Rc::clone(self.family.integration_store()), id).map_err(Into::into)
    }

    fn axis(&self, index: usize) -> Result<ManimNumberLine, CoordinateAuthoringError> {
        let id = plane_members(&self.family)?[index];
        ManimNumberLine::from_family(MobjectFamily::from_node(
            Rc::clone(self.family.integration_store()),
            id,
        )?)
    }
}

impl Scene {
    /// Construct the complete retained plane through one transaction.  The
    /// same route publishes coherently after this Scene has started execution.
    pub fn number_plane(
        &mut self,
        options: &ManimNumberPlaneOptions,
    ) -> Result<ManimNumberPlane, CoordinateAuthoringError> {
        let (transaction, root) = prepare_number_plane(options)?;
        let result = self.apply_semantic_transaction(transaction)?;
        ManimNumberPlane::from_family(resolve_family(
            Rc::clone(self.integration_store()),
            &result,
            root,
        )?)
    }

    pub fn effective_number_plane_frame(
        &self,
        plane: &ManimNumberPlane,
    ) -> Result<AxesFrame, CoordinateAuthoringError> {
        Ok(AxesFrame::new(
            plane
                .x_axis()?
                .snapshot_with(&mut |shaft| self.effective_path_query(shaft))?,
            plane
                .y_axis()?
                .snapshot_with(&mut |shaft| self.effective_path_query(shaft))?,
        ))
    }
}

fn plane_members(family: &MobjectFamily) -> Result<[SemanticNodeId; 4], CoordinateAuthoringError> {
    family.validate()?;
    let store = family.integration_store().borrow();
    let node = store.node(family.node_id()).expect("validated family");
    let mut members = node.members_iter();
    Ok([
        members
            .next()
            .ok_or(CoordinateAuthoringError::InvalidTopology)?,
        members
            .next()
            .ok_or(CoordinateAuthoringError::InvalidTopology)?,
        members
            .next()
            .ok_or(CoordinateAuthoringError::InvalidTopology)?,
        members
            .next()
            .ok_or(CoordinateAuthoringError::InvalidTopology)?,
    ])
}

pub(crate) fn prepare_number_plane(
    options: &ManimNumberPlaneOptions,
) -> Result<(SemanticMutationTransaction, SemanticLocalNodeToken), CoordinateAuthoringError> {
    let x_length = options
        .x_length
        .unwrap_or(options.x_range[1] - options.x_range[0]);
    let y_length = options
        .y_length
        .unwrap_or(options.y_range[1] - options.y_range[0]);
    let frame = AxesFrame::centered(options.x_range, options.y_range, x_length, y_length)?;
    validate_plane_style(&options.axis_style, "invalid axis style")?;
    validate_plane_style(
        &options.background_line_style,
        "invalid background line style",
    )?;
    let faded_style = options
        .faded_line_style
        .clone()
        .unwrap_or_else(|| faded_style(&options.background_line_style));
    validate_plane_style(&faded_style, "invalid faded line style")?;

    let ratio = options.faded_line_ratio.max(1) as usize;
    let horizontal = grid_offsets(options.y_range, ratio, options.line_limit)?;
    let remaining = options.line_limit.checked_sub(horizontal.len()).ok_or(
        CoordinateAuthoringError::InvalidOptions("number plane line budget exceeded"),
    )?;
    let vertical = grid_offsets(options.x_range, ratio, remaining)?;
    let mut transaction = SemanticMutationTransaction::new();
    let root = transaction.create_node(SemanticNodeCreation::family());
    let faded = transaction.create_node(SemanticNodeCreation::family());
    let background = transaction.create_node(SemanticNodeCreation::family());
    let x_axis = stage_line(
        &mut transaction,
        prepare_line(
            frame.x(),
            CoordinateTicks {
                enabled: false,
                ..CoordinateTicks::default()
            },
            &options.axis_style,
        )?,
    );
    let y_axis = stage_line(
        &mut transaction,
        prepare_line(
            frame.y(),
            CoordinateTicks {
                enabled: false,
                ..CoordinateTicks::default()
            },
            &options.axis_style,
        )?,
    );
    stage_grid_lines(
        &mut transaction,
        faded,
        background,
        frame.x(),
        frame.y(),
        &horizontal,
        &options.background_line_style,
        &faded_style,
    )?;
    stage_grid_lines(
        &mut transaction,
        faded,
        background,
        frame.y(),
        frame.x(),
        &vertical,
        &options.background_line_style,
        &faded_style,
    )?;
    // Pinned Manim add_to_back ordering: faded, background, then axes.
    for child in [faded, background, x_axis, y_axis] {
        transaction.add_member(root, child);
    }
    Ok((transaction, root))
}

fn validate_plane_style(
    style: &SemanticStyle,
    message: &'static str,
) -> Result<(), CoordinateAuthoringError> {
    if !style.is_finite() || style.stroke_width < 0.0 {
        return Err(CoordinateAuthoringError::InvalidOptions(message));
    }
    Ok(())
}

fn faded_style(background: &SemanticStyle) -> SemanticStyle {
    // NumberPlane only consumes stroke paint; these are the two numeric stroke
    // fields Manim's default background configuration provides.
    let mut faded = background.clone();
    faded.stroke_width *= 0.5;
    faded.stroke_opacity *= 0.5;
    faded
}

/// Pinned NumberPlane offset generation.  The first item is the axis itself;
/// subsequent positive and negative sequences classify independently.
fn grid_offsets(
    range: [f64; 3],
    ratio: usize,
    limit: usize,
) -> Result<Vec<(f64, bool)>, CoordinateAuthoringError> {
    noon_geometry::validate_coordinate_range(range)?;
    let step = range[2] / ratio as f64;
    if !step.is_finite() || step <= 0.0 {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "invalid faded line ratio",
        ));
    }
    let mut offsets = Vec::new();
    offsets
        .try_reserve_exact(limit.min(64))
        .map_err(|_| CoordinateError::AllocationFailed)?;
    push_grid_offset(&mut offsets, 0.0, ratio == 1, limit)?;
    let positive_end = (range[1] - range[0]).min(range[1]);
    let negative_end = (range[0] - range[1]).max(range[0]);
    // Match numpy.arange(start, stop, step): its ceil-based length can retain
    // a rounded endpoint (e.g. thirds approaching 2). A comparison loop would
    // silently change both the line count and major/faded classification.
    for (start, stop, stride) in [(step, positive_end, step), (-step, negative_end, -step)] {
        let count = ((stop - start) / stride).ceil().max(0.0);
        let remaining = limit - offsets.len();
        if !count.is_finite() || count > remaining as f64 {
            return Err(CoordinateAuthoringError::InvalidOptions(
                "number plane line budget exceeded",
            ));
        }
        let count = count as usize;
        offsets
            .try_reserve_exact(count)
            .map_err(|_| CoordinateError::AllocationFailed)?;
        for index in 0..count {
            offsets.push((start + index as f64 * stride, (index + 1) % ratio == 0));
        }
    }
    Ok(offsets)
}

fn push_grid_offset(
    offsets: &mut Vec<(f64, bool)>,
    offset: f64,
    background: bool,
    limit: usize,
) -> Result<(), CoordinateAuthoringError> {
    if offsets.len() == limit {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "number plane line budget exceeded",
        ));
    }
    offsets.push((offset, background));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn stage_grid_lines(
    transaction: &mut SemanticMutationTransaction,
    faded: SemanticLocalNodeToken,
    background: SemanticLocalNodeToken,
    parallel: NumberLineFrame,
    perpendicular: NumberLineFrame,
    offsets: &[(f64, bool)],
    background_style: &SemanticStyle,
    faded_style: &SemanticStyle,
) -> Result<(), CoordinateAuthoringError> {
    let start = perpendicular.start();
    let end = perpendicular.end();
    // NumberLine overrides get_unit_vector: its magnitude is one coordinate
    // unit, rather than one scene unit. Divide directly to avoid subtracting
    // two large translated number_to_point results for narrow offset ranges.
    let range = perpendicular.range();
    let span = range[1] - range[0];
    let unit = [(end[0] - start[0]) / span, (end[1] - start[1]) / span];
    for &(offset, is_background) in offsets {
        let delta = [unit[0] * offset, unit[1] * offset];
        let style = if is_background {
            background_style
        } else {
            faded_style
        };
        let line = transaction.create_node(SemanticNodeCreation::object(line_state(
            [
                parallel.start()[0] + delta[0],
                parallel.start()[1] + delta[1],
            ],
            [parallel.end()[0] + delta[0], parallel.end()[1] + delta[1]],
            style,
        )?));
        transaction.add_member(if is_background { background } else { faded }, line);
    }
    Ok(())
}
