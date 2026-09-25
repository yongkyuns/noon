//! Retained PolarPlane semantics over ordinary circles, lines and axis families.
use super::*;
use noon_core::{SemanticTransform2_5D, SemanticVec3, BLUE_D};
use noon_geometry::PolarFrame;

/// Direction used for azimuth labels; coordinate conversion remains Cartesian.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolarAzimuthDirection {
    Clockwise,
    Counterclockwise,
}

/// Inert Manim-compatible polar-grid request.
#[derive(Clone, Debug)]
pub struct ManimPolarPlaneOptions {
    pub radius_max: f64,
    pub size: Option<f64>,
    pub radius_step: f64,
    pub azimuth_step: Option<f64>,
    pub azimuth_offset: f64,
    pub azimuth_direction: PolarAzimuthDirection,
    pub axis_style: SemanticStyle,
    pub background_line_style: SemanticStyle,
    pub faded_line_style: Option<SemanticStyle>,
    pub faded_line_ratio: u32,
    /// Fixed admission cap for all retained circles and radial rays.
    pub line_limit: usize,
}

impl ManimPolarPlaneOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn faded_line_style_mut(&mut self) -> &mut SemanticStyle {
        self.faded_line_style
            .get_or_insert_with(|| ManimNumberPlaneOptions::default_faded_line_style())
    }
}

impl Default for ManimPolarPlaneOptions {
    fn default() -> Self {
        Self {
            radius_max: 4.0,
            size: None,
            radius_step: 1.0,
            azimuth_step: None,
            azimuth_offset: 0.0,
            azimuth_direction: PolarAzimuthDirection::Counterclockwise,
            axis_style: default_axis_style(),
            background_line_style: SemanticStyle {
                fill: None,
                stroke: Some(SemanticPaint::Solid(BLUE_D)),
                stroke_width: 0.02,
                stroke_width_mode: StrokeWidthMode::ScreenSpace,
                stroke_join: StrokeJoin::Miter,
                stroke_cap: StrokeCap::Butt,
                ..SemanticStyle::default()
            },
            faded_line_style: None,
            faded_line_ratio: 1,
            line_limit: 20_000,
        }
    }
}

/// A reconstructed semantic polar family. Geometry, ordering, style and
/// coordinate frame all live in its shared retained leaves.
#[derive(Clone, Debug)]
pub struct ManimPolarPlane {
    grid: ManimNumberPlane,
}

impl std::ops::Deref for ManimPolarPlane {
    type Target = ManimNumberPlane;
    fn deref(&self) -> &Self::Target {
        &self.grid
    }
}

impl ManimPolarPlane {
    pub fn create(
        store: Rc<RefCell<SemanticStore>>,
        options: &ManimPolarPlaneOptions,
    ) -> Result<Self, CoordinateAuthoringError> {
        let (transaction, root) = prepare_polar_plane(options)?;
        let result = transaction
            .apply(&mut store.borrow_mut())
            .map_err(AuthoringError::from)?;
        Self::from_family(resolve_family(store, &result, root)?)
    }

    pub fn from_family(family: MobjectFamily) -> Result<Self, CoordinateAuthoringError> {
        Ok(Self {
            grid: ManimNumberPlane::from_family(family)?,
        })
    }

    pub fn authored_polar_frame(&self) -> Result<PolarFrame, CoordinateAuthoringError> {
        Ok(PolarFrame::new(self.authored_frame()?))
    }

    pub fn effective_polar_frame(
        &self,
        execution: &ExecutionSession,
    ) -> Result<PolarFrame, CoordinateAuthoringError> {
        Ok(PolarFrame::new(self.effective_frame(execution)?))
    }
}

impl Scene {
    pub fn polar_plane(
        &mut self,
        options: &ManimPolarPlaneOptions,
    ) -> Result<ManimPolarPlane, CoordinateAuthoringError> {
        let (transaction, root) = prepare_polar_plane(options)?;
        let result = self.apply_semantic_transaction(transaction)?;
        ManimPolarPlane::from_family(resolve_family(
            Rc::clone(self.integration_store()),
            &result,
            root,
        )?)
    }

    pub fn effective_polar_plane_frame(
        &self,
        plane: &ManimPolarPlane,
    ) -> Result<PolarFrame, CoordinateAuthoringError> {
        let axes = AxesFrame::new(
            plane
                .x_axis()?
                .snapshot_with(&mut |shaft| self.effective_path_query(shaft))?,
            plane
                .y_axis()?
                .snapshot_with(&mut |shaft| self.effective_path_query(shaft))?,
        );
        Ok(PolarFrame::new(axes))
    }
}

pub(crate) fn prepare_polar_plane(
    options: &ManimPolarPlaneOptions,
) -> Result<(SemanticMutationTransaction, SemanticLocalNodeToken), CoordinateAuthoringError> {
    if !options.radius_max.is_finite()
        || options.radius_max <= 0.0
        || !options.radius_step.is_finite()
        || options.radius_step <= 0.0
        || !options.azimuth_offset.is_finite()
    {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "invalid polar plane radius or azimuth",
        ));
    }
    let size = options.size.unwrap_or(options.radius_max * 2.0);
    let range = [-options.radius_max, options.radius_max, options.radius_step];
    let frame = AxesFrame::centered(range, range, size, size)?;
    validate_style(&options.axis_style, "invalid polar axis style")?;
    validate_style(
        &options.background_line_style,
        "invalid polar background line style",
    )?;
    let faded_style = options
        .faded_line_style
        .clone()
        .unwrap_or_else(|| faded_grid_style(&options.background_line_style));
    validate_style(&faded_style, "invalid polar faded line style")?;
    let azimuth_step = options.azimuth_step.unwrap_or(20.0);
    if !azimuth_step.is_finite() || azimuth_step <= 0.0 {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "invalid polar azimuth step",
        ));
    }
    let ratio = options.faded_line_ratio.max(1) as usize;
    let circles = polar_steps(
        options.radius_max,
        options.radius_step / ratio as f64,
        true,
        options.line_limit,
    )?;
    let remaining = options.line_limit.checked_sub(circles.len()).ok_or(
        CoordinateAuthoringError::InvalidOptions("polar plane line budget exceeded"),
    )?;
    let rays = polar_steps(
        std::f64::consts::TAU,
        std::f64::consts::TAU / azimuth_step / ratio as f64,
        false,
        remaining,
    )?;
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
    let center = frame.coords_to_point(0.0, 0.0)?;
    let unit = frame.x().unit_size();
    // Match Manim's `_get_lines`: radial lines precede concentric circles in
    // both major and faded families.
    stage_polar_rays(
        &mut transaction,
        faded,
        background,
        &rays,
        ratio,
        frame,
        options.azimuth_offset,
        options.radius_max,
        &options.background_line_style,
        &faded_style,
    )?;
    stage_polar_circles(
        &mut transaction,
        faded,
        background,
        &circles,
        ratio,
        unit,
        center,
        &options.background_line_style,
        &faded_style,
    )?;
    for child in [faded, background, x_axis, y_axis] {
        transaction.add_member(root, child);
    }
    Ok((transaction, root))
}

fn validate_style(
    style: &SemanticStyle,
    message: &'static str,
) -> Result<(), CoordinateAuthoringError> {
    if !style.is_finite() || style.stroke_width < 0.0 {
        return Err(CoordinateAuthoringError::InvalidOptions(message));
    }
    Ok(())
}

fn faded_grid_style(background: &SemanticStyle) -> SemanticStyle {
    let mut faded = background.clone();
    faded.stroke_width *= 0.5;
    faded.stroke_opacity *= 0.5;
    faded
}

fn polar_steps(
    end: f64,
    step: f64,
    inclusive: bool,
    limit: usize,
) -> Result<Vec<f64>, CoordinateAuthoringError> {
    if !step.is_finite() || step <= 0.0 {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "invalid polar step",
        ));
    }
    let count = ((end + if inclusive { step } else { 0.0 }) / step).ceil();
    if !count.is_finite() || count < 0.0 || count > limit as f64 {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "polar plane line budget exceeded",
        ));
    }
    let count = count as usize;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| CoordinateError::AllocationFailed)?;
    for index in 0..count {
        values.push(index as f64 * step);
    }
    Ok(values)
}

#[allow(clippy::too_many_arguments)]
fn stage_polar_circles(
    transaction: &mut SemanticMutationTransaction,
    faded: SemanticLocalNodeToken,
    background: SemanticLocalNodeToken,
    circles: &[f64],
    ratio: usize,
    unit: f64,
    center: [f64; 2],
    background_style: &SemanticStyle,
    faded_style: &SemanticStyle,
) -> Result<(), CoordinateAuthoringError> {
    for (index, radius) in circles.iter().copied().enumerate() {
        let mut state = SemanticObjectState::new(StoredGeometry::Circle {
            radius: crate::integration::authoring_render_f64("polar radius", radius * unit)? as f32,
        });
        state.transform = SemanticTransform2_5D {
            translation: SemanticVec3::new(center[0], center[1], 0.0),
            ..SemanticTransform2_5D::default()
        };
        let is_background = index % ratio == 0;
        state.style = if is_background {
            background_style.clone()
        } else {
            faded_style.clone()
        };
        let node = transaction.create_node(SemanticNodeCreation::object(state));
        transaction.add_member(if is_background { background } else { faded }, node);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn stage_polar_rays(
    transaction: &mut SemanticMutationTransaction,
    faded: SemanticLocalNodeToken,
    background: SemanticLocalNodeToken,
    rays: &[f64],
    ratio: usize,
    frame: AxesFrame,
    offset: f64,
    radius_max: f64,
    background_style: &SemanticStyle,
    faded_style: &SemanticStyle,
) -> Result<(), CoordinateAuthoringError> {
    let polar = PolarFrame::new(frame);
    let center = polar.polar_to_point(0.0, 0.0)?;
    for (index, azimuth) in rays.iter().copied().enumerate() {
        let is_background = index % ratio == 0;
        let state = line_state(
            center,
            polar.polar_to_point(radius_max, azimuth + offset)?,
            if is_background {
                background_style
            } else {
                faded_style
            },
        )?;
        let node = transaction.create_node(SemanticNodeCreation::object(state));
        transaction.add_member(if is_background { background } else { faded }, node);
    }
    Ok(())
}
