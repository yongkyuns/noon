//! Linear coordinate families over the existing Semantic Scene and path queries.
//!
//! Only ordinary Line leaves are rendered. Ranges live on semantic shafts;
//! wrappers retain family identity, never ranges, endpoints, or a second scene.
//! Initial constructors require explicit ranges and do not add tips or labels.

use std::{cell::RefCell, rc::Rc};

use crate::{
    AuthoringError, ExecutionSession, ManimGeometryOptions, Mobject, MobjectFamily, PathQuery,
    PlotAuthoringError, PlotPreparationError, PlotSamplingOptions, Scene,
};
use noon_core::{
    SemanticLocalNodeToken, SemanticMutationTransaction, SemanticMutationTransactionResult,
    SemanticNodeCreation, SemanticNodeId, SemanticNumberLineRole, SemanticObjectRole,
    SemanticObjectState, SemanticPaint, SemanticStore, SemanticStyle, StoredGeometry,
    StrokeWidthMode, Vec2, WHITE,
};
use noon_geometry::{number_line_tick_values, AxesFrame, CoordinateError, NumberLineFrame};

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub enum CoordinateAuthoringError {
    Coordinate(CoordinateError),
    Authoring(AuthoringError),
    Plot(PlotAuthoringError),
    Live(crate::LiveSessionError),
    InvalidOptions(&'static str),
    InvalidTopology,
}

impl std::fmt::Display for CoordinateAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Coordinate(error) => std::fmt::Display::fmt(error, formatter),
            Self::Authoring(error) => std::fmt::Display::fmt(error, formatter),
            Self::Plot(error) => std::fmt::Display::fmt(error, formatter),
            Self::Live(error) => std::fmt::Display::fmt(error, formatter),
            Self::InvalidOptions(reason) => formatter.write_str(reason),
            Self::InvalidTopology => formatter.write_str("coordinate family topology is invalid"),
        }
    }
}

impl std::error::Error for CoordinateAuthoringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Coordinate(error) => Some(error),
            Self::Authoring(error) => Some(error),
            Self::Plot(error) => Some(error),
            Self::Live(error) => Some(error),
            Self::InvalidOptions(_) | Self::InvalidTopology => None,
        }
    }
}

impl From<CoordinateError> for CoordinateAuthoringError {
    fn from(error: CoordinateError) -> Self {
        Self::Coordinate(error)
    }
}
impl From<AuthoringError> for CoordinateAuthoringError {
    fn from(error: AuthoringError) -> Self {
        Self::Authoring(error)
    }
}
impl From<PlotAuthoringError> for CoordinateAuthoringError {
    fn from(error: PlotAuthoringError) -> Self {
        Self::Plot(error)
    }
}

impl From<crate::LiveSessionError> for CoordinateAuthoringError {
    fn from(error: crate::LiveSessionError) -> Self {
        Self::Live(error)
    }
}

/// Tick admission and geometry. `half_length` matches Manim's `tick_size`:
/// a vertical tick extends from -tick_size to +tick_size, not half that length.
#[derive(Clone, Copy, Debug)]
pub struct CoordinateTicks {
    pub enabled: bool,
    pub half_length: f64,
    pub exclude_origin: bool,
    pub limit: usize,
}

impl Default for CoordinateTicks {
    fn default() -> Self {
        Self {
            enabled: true,
            half_length: 0.1,
            exclude_origin: false,
            limit: 10_000,
        }
    }
}

fn default_axis_style() -> SemanticStyle {
    SemanticStyle {
        fill: None,
        stroke: Some(SemanticPaint::Solid(WHITE)),
        stroke_width: 0.02,
        stroke_width_mode: StrokeWidthMode::ScreenSpace,
        ..SemanticStyle::default()
    }
}

/// Inert tipless, linear NumberLine constructor request.
#[derive(Clone, Debug)]
pub struct ManimNumberLineOptions {
    pub range: [f64; 3],
    pub length: Option<f64>,
    pub unit_size: f64,
    pub rotation: f64,
    pub ticks: CoordinateTicks,
    pub style: SemanticStyle,
}

impl ManimNumberLineOptions {
    pub fn new(range: [f64; 3]) -> Self {
        Self {
            range,
            length: None,
            unit_size: 1.0,
            rotation: 0.0,
            ticks: CoordinateTicks::default(),
            style: default_axis_style(),
        }
    }

    fn frame(&self) -> Result<NumberLineFrame, CoordinateAuthoringError> {
        noon_geometry::validate_coordinate_range(self.range)?;
        if !self.unit_size.is_finite() || self.unit_size <= 0.0 {
            return Err(CoordinateError::InvalidLength.into());
        }
        let length = self
            .length
            .unwrap_or_else(|| (self.range[1] - self.range[0]) * self.unit_size);
        Ok(NumberLineFrame::centered(
            self.range,
            length,
            self.rotation,
        )?)
    }
}

/// Inert explicitly sized, tipless linear Axes request. Coordinate ranges, not
/// tick or label bounds, determine placement. Rotate/shift the resulting family.
#[derive(Clone, Debug)]
pub struct ManimAxesOptions {
    pub x_range: [f64; 3],
    pub y_range: [f64; 3],
    pub x_length: f64,
    pub y_length: f64,
    pub ticks: CoordinateTicks,
    pub style: SemanticStyle,
}

impl ManimAxesOptions {
    pub fn new(x_range: [f64; 3], y_range: [f64; 3], x_length: f64, y_length: f64) -> Self {
        Self {
            x_range,
            y_range,
            x_length,
            y_length,
            ticks: CoordinateTicks {
                exclude_origin: true,
                ..CoordinateTicks::default()
            },
            style: default_axis_style(),
        }
    }
}

/// One ordinary semantic family: shaft first, tick family second.
/// Clone aliases this identity. Shared family-copy operations allocate new nodes.
#[derive(Clone, Debug)]
pub struct ManimNumberLine {
    family: MobjectFamily,
}

impl ManimNumberLine {
    /// Cold integration entry point. Scene-owned construction also supports the
    /// existing coherent transaction path after Scene execution has started.
    pub fn create(
        store: Rc<RefCell<SemanticStore>>,
        options: &ManimNumberLineOptions,
    ) -> Result<Self, CoordinateAuthoringError> {
        let (transaction, root) = prepare_number_line(options)?;
        let result = transaction
            .apply(&mut store.borrow_mut())
            .map_err(AuthoringError::from)?;
        Self::from_family(resolve_family(store, &result, root)?)
    }

    /// Reconstruct from semantic identity, including a shared family copy.
    pub fn from_family(family: MobjectFamily) -> Result<Self, CoordinateAuthoringError> {
        let line = Self { family };
        line.range()?;
        line.ticks()?;
        Ok(line)
    }

    pub fn family(&self) -> &MobjectFamily {
        &self.family
    }

    pub fn shaft(&self) -> Result<Mobject, CoordinateAuthoringError> {
        let [shaft, _] = coordinate_members(&self.family)?;
        Ok(Mobject::from_node(
            Rc::clone(self.family.integration_store()),
            shaft,
        )?)
    }

    pub fn ticks(&self) -> Result<MobjectFamily, CoordinateAuthoringError> {
        let [_, ticks] = coordinate_members(&self.family)?;
        Ok(MobjectFamily::from_node(
            Rc::clone(self.family.integration_store()),
            ticks,
        )?)
    }

    pub fn range(&self) -> Result<[f64; 3], CoordinateAuthoringError> {
        match self.shaft()?.state()?.role() {
            SemanticObjectRole::NumberLine(role) if role.is_valid() => Ok(role.range()),
            _ => Err(CoordinateAuthoringError::InvalidTopology),
        }
    }

    pub fn authored_frame(&self) -> Result<NumberLineFrame, CoordinateAuthoringError> {
        self.snapshot_with(&mut Mobject::path_query)
    }

    /// Read current path endpoints and transforms from one immutable execution
    /// borrow. A stale/foreign execution or unsupported path override is an error;
    /// this method never falls back to authored state.
    pub fn effective_frame(
        &self,
        execution: &ExecutionSession,
    ) -> Result<NumberLineFrame, CoordinateAuthoringError> {
        self.snapshot_with(&mut |shaft| {
            crate::path_queries::effective_path_query(
                self.family.integration_store(),
                execution,
                shaft,
            )
        })
    }

    pub(crate) fn snapshot_with(
        &self,
        query: &mut impl FnMut(&Mobject) -> Result<PathQuery, AuthoringError>,
    ) -> Result<NumberLineFrame, CoordinateAuthoringError> {
        let shaft = self.shaft()?;
        let range = match shaft.state()?.role() {
            SemanticObjectRole::NumberLine(role) if role.is_valid() => role.range(),
            _ => return Err(CoordinateAuthoringError::InvalidTopology),
        };
        let path = query(&shaft)?;
        let (sx, sy) = path.start()?;
        let (ex, ey) = path.end()?;
        Ok(NumberLineFrame::new(range, [sx, sy], [ex, ey])?)
    }
}

/// Ordinary family containing X then Y NumberLine families. Only family identity
/// is retained here; every query resolves members and ranges from semantic state.
#[derive(Clone, Debug)]
pub struct ManimAxes {
    family: MobjectFamily,
}

impl ManimAxes {
    pub fn create(
        store: Rc<RefCell<SemanticStore>>,
        options: &ManimAxesOptions,
    ) -> Result<Self, CoordinateAuthoringError> {
        let (transaction, root) = prepare_axes(options)?;
        let result = transaction
            .apply(&mut store.borrow_mut())
            .map_err(AuthoringError::from)?;
        Self::from_family(resolve_family(store, &result, root)?)
    }

    pub fn from_family(family: MobjectFamily) -> Result<Self, CoordinateAuthoringError> {
        let axes = Self { family };
        axes.x_axis()?;
        axes.y_axis()?;
        Ok(axes)
    }

    pub fn family(&self) -> &MobjectFamily {
        &self.family
    }

    pub fn x_axis(&self) -> Result<ManimNumberLine, CoordinateAuthoringError> {
        self.axis(0)
    }
    pub fn y_axis(&self) -> Result<ManimNumberLine, CoordinateAuthoringError> {
        self.axis(1)
    }

    fn axis(&self, index: usize) -> Result<ManimNumberLine, CoordinateAuthoringError> {
        let members = coordinate_members(&self.family)?;
        ManimNumberLine::from_family(MobjectFamily::from_node(
            Rc::clone(self.family.integration_store()),
            members[index],
        )?)
    }

    pub fn authored_frame(&self) -> Result<AxesFrame, CoordinateAuthoringError> {
        self.snapshot_with(&mut Mobject::path_query)
    }

    /// Both axes use the same immutable execution borrow. No host callback or
    /// publication occurs between their observations, and no ranges are cached.
    pub fn effective_frame(
        &self,
        execution: &ExecutionSession,
    ) -> Result<AxesFrame, CoordinateAuthoringError> {
        self.snapshot_with(&mut |shaft| {
            crate::path_queries::effective_path_query(
                self.family.integration_store(),
                execution,
                shaft,
            )
        })
    }

    pub fn plot_sampling(
        &self,
        range: Option<&[f64]>,
    ) -> Result<PlotSamplingOptions, CoordinateAuthoringError> {
        PlotSamplingOptions::axes(self.x_axis()?.range()?, range)
            .map_err(PlotAuthoringError::from)
            .map_err(CoordinateAuthoringError::from)
    }

    /// Construct a detached static y=f(x) path in this axes' semantic store.
    ///
    /// The authored coordinate frame is captured once before the callback runs,
    /// and the callback is evaluated only while preparing the ordinary retained
    /// path. Add the returned [`Mobject`] to any family explicitly. Runtime
    /// effective coordinates remain an explicit `effective_frame` capture rather
    /// than an implicit live graph model.
    pub fn plot(
        &self,
        function: impl FnMut(f64) -> f64,
        range: Option<&[f64]>,
        use_smoothing: bool,
    ) -> Result<Mobject, CoordinateAuthoringError> {
        let frame = self.authored_frame()?;
        let sampling = PlotSamplingOptions::axes(frame.x().range(), range)
            .map_err(PlotAuthoringError::from)?;
        let options =
            ManimGeometryOptions::axes_function_plot(frame, &sampling, function, use_smoothing)?;
        Ok(Mobject::from_manim_geometry(
            Rc::clone(self.family.integration_store()),
            options,
        )?)
    }

    /// Construct a detached static data polyline in this axes' semantic store.
    ///
    /// Supplied order and repeated x values are preserved. Mapping uses one
    /// authored coordinate-frame snapshot and creates no graph-side semantics.
    pub fn plot_samples(&self, points: &[[f64; 2]]) -> Result<Mobject, CoordinateAuthoringError> {
        let options = ManimGeometryOptions::axes_sampled_plot(self.authored_frame()?, points)?;
        Ok(Mobject::from_manim_geometry(
            Rc::clone(self.family.integration_store()),
            options,
        )?)
    }

    pub(crate) fn snapshot_with(
        &self,
        query: &mut impl FnMut(&Mobject) -> Result<PathQuery, AuthoringError>,
    ) -> Result<AxesFrame, CoordinateAuthoringError> {
        Ok(AxesFrame::new(
            self.x_axis()?.snapshot_with(query)?,
            self.y_axis()?.snapshot_with(query)?,
        ))
    }
}

impl Scene {
    /// Atomically construct shaft, ticks and family using this Scene's ordinary
    /// transaction routing. Invalid preparation publishes no partial identity.
    pub fn number_line(
        &mut self,
        options: &ManimNumberLineOptions,
    ) -> Result<ManimNumberLine, CoordinateAuthoringError> {
        let (transaction, root) = prepare_number_line(options)?;
        let result = self.apply_semantic_transaction(transaction)?;
        ManimNumberLine::from_family(resolve_family(
            Rc::clone(self.integration_store()),
            &result,
            root,
        )?)
    }

    pub fn axes(
        &mut self,
        options: &ManimAxesOptions,
    ) -> Result<ManimAxes, CoordinateAuthoringError> {
        let (transaction, root) = prepare_axes(options)?;
        let result = self.apply_semantic_transaction(transaction)?;
        ManimAxes::from_family(resolve_family(
            Rc::clone(self.integration_store()),
            &result,
            root,
        )?)
    }

    /// Current coordinate snapshot from this Scene's owned execution. Cold
    /// Scenes fail explicitly, just like Scene::effective_path_query.
    pub fn effective_number_line_frame(
        &self,
        line: &ManimNumberLine,
    ) -> Result<NumberLineFrame, CoordinateAuthoringError> {
        line.snapshot_with(&mut |shaft| self.effective_path_query(shaft))
    }

    pub fn effective_axes_frame(
        &self,
        axes: &ManimAxes,
    ) -> Result<AxesFrame, CoordinateAuthoringError> {
        axes.snapshot_with(&mut |shaft| self.effective_path_query(shaft))
    }
}

impl ManimGeometryOptions {
    /// Prepare y=f(x) in a captured coordinate frame. Capture once before calling
    /// this method; evaluation cannot mix axis publications between samples.
    pub fn axes_function_plot(
        frame: AxesFrame,
        sampling: &PlotSamplingOptions,
        mut function: impl FnMut(f64) -> f64,
        use_smoothing: bool,
    ) -> Result<Self, CoordinateAuthoringError> {
        let plan = sampling.plan().map_err(PlotAuthoringError::from)?;
        let mut points = Vec::new();
        points
            .try_reserve_exact(plan.parameters().len())
            .map_err(|_| PlotAuthoringError::from(PlotPreparationError::AllocationFailed))?;
        for &x in plan.parameters() {
            points.push(frame.coords_to_point(x, function(x))?);
        }
        Ok(Self::plot_samples(&plan, &points, use_smoothing)?)
    }

    /// Map data without sorting, smoothing, or connecting across invented samples.
    pub fn axes_sampled_plot(
        frame: AxesFrame,
        points: &[[f64; 2]],
    ) -> Result<Self, CoordinateAuthoringError> {
        let mut mapped = Vec::new();
        mapped
            .try_reserve_exact(points.len())
            .map_err(|_| PlotAuthoringError::from(PlotPreparationError::AllocationFailed))?;
        for &[x, y] in points {
            mapped.push(frame.coords_to_point(x, y)?);
        }
        Ok(Self::sampled_plot(&mapped)?)
    }
}

fn coordinate_members(
    family: &MobjectFamily,
) -> Result<[SemanticNodeId; 2], CoordinateAuthoringError> {
    family.validate()?;
    let store = family.integration_store().borrow();
    let node = store.node(family.node_id()).expect("validated family");
    let first = node
        .first_member()
        .ok_or(CoordinateAuthoringError::InvalidTopology)?;
    let second = node
        .next_member(first)
        .ok_or(CoordinateAuthoringError::InvalidTopology)?;
    Ok([first, second])
}

pub(crate) fn resolve_family(
    store: Rc<RefCell<SemanticStore>>,
    result: &SemanticMutationTransactionResult,
    root: SemanticLocalNodeToken,
) -> Result<MobjectFamily, CoordinateAuthoringError> {
    let id = result
        .resolve(root)
        .ok_or(AuthoringError::UnresolvedCreatedNode(root))?;
    Ok(MobjectFamily::from_node(store, id)?)
}

pub(crate) fn prepare_number_line(
    options: &ManimNumberLineOptions,
) -> Result<(SemanticMutationTransaction, SemanticLocalNodeToken), CoordinateAuthoringError> {
    let states = prepare_line(options.frame()?, options.ticks, &options.style)?;
    let mut transaction = SemanticMutationTransaction::new();
    let root = stage_line(&mut transaction, states);
    Ok((transaction, root))
}

pub(crate) fn prepare_axes(
    options: &ManimAxesOptions,
) -> Result<(SemanticMutationTransaction, SemanticLocalNodeToken), CoordinateAuthoringError> {
    let frame = AxesFrame::centered(
        options.x_range,
        options.y_range,
        options.x_length,
        options.y_length,
    )?;
    let x = prepare_line(frame.x(), options.ticks, &options.style)?;
    let y = prepare_line(frame.y(), options.ticks, &options.style)?;
    let mut transaction = SemanticMutationTransaction::new();
    let root = transaction.create_node(SemanticNodeCreation::family());
    let x = stage_line(&mut transaction, x);
    let y = stage_line(&mut transaction, y);
    transaction.add_member(root, x);
    transaction.add_member(root, y);
    Ok((transaction, root))
}

fn prepare_line(
    frame: NumberLineFrame,
    ticks: CoordinateTicks,
    style: &SemanticStyle,
) -> Result<Vec<SemanticObjectState>, CoordinateAuthoringError> {
    if !ticks.half_length.is_finite() || ticks.half_length < 0.0 {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "invalid tick size",
        ));
    }
    if !style.is_finite() || style.stroke_width < 0.0 {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "invalid axis style",
        ));
    }
    let values = if ticks.enabled {
        number_line_tick_values(frame.range(), false, ticks.exclude_origin, ticks.limit)?
    } else {
        Vec::new()
    };
    let mut states = Vec::new();
    states
        .try_reserve_exact(values.len() + 1)
        .map_err(|_| CoordinateError::AllocationFailed)?;
    let mut shaft = line_state(frame.start(), frame.end(), style)?;
    shaft.set_role(SemanticObjectRole::NumberLine(SemanticNumberLineRole::new(
        frame.range(),
    )));
    states.push(shaft);
    let start = frame.start();
    let end = frame.end();
    let length = (end[0] - start[0]).hypot(end[1] - start[1]);
    let nx = -(end[1] - start[1]) / length * ticks.half_length;
    let ny = (end[0] - start[0]) / length * ticks.half_length;
    for value in values {
        let [x, y] = frame.number_to_point(value)?;
        states.push(line_state([x - nx, y - ny], [x + nx, y + ny], style)?);
    }
    Ok(states)
}

fn line_state(
    start: [f64; 2],
    end: [f64; 2],
    style: &SemanticStyle,
) -> Result<SemanticObjectState, CoordinateAuthoringError> {
    let lower = |[x, y]: [f64; 2]| -> Result<Vec2, AuthoringError> {
        Ok(Vec2::new(
            crate::integration::authoring_render_f64("axis x", x)? as f32,
            crate::integration::authoring_render_f64("axis y", y)? as f32,
        ))
    };
    let mut state = SemanticObjectState::new(StoredGeometry::Line {
        start: lower(start)?,
        end: lower(end)?,
    });
    state.style = style.clone();
    Ok(state)
}

fn stage_line(
    transaction: &mut SemanticMutationTransaction,
    states: Vec<SemanticObjectState>,
) -> SemanticLocalNodeToken {
    let mut states = states.into_iter();
    let root = transaction.create_node(SemanticNodeCreation::family());
    let shaft = transaction.create_node(SemanticNodeCreation::object(
        states.next().expect("prepared coordinate shaft"),
    ));
    let ticks = transaction.create_node(SemanticNodeCreation::family());
    transaction.add_member(root, shaft);
    transaction.add_member(root, ticks);
    for state in states {
        let tick = transaction.create_node(SemanticNodeCreation::object(state));
        transaction.add_member(ticks, tick);
    }
    root
}
