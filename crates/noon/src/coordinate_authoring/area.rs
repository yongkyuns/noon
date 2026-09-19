//! Pure area/partition preparation plus atomic path-family admission.

use super::*;

pub(crate) fn axes_area(
    frame: AxesFrame,
    graph: &Mobject,
    graph_path: &PathQuery,
    x_range: Option<[f64; 2]>,
    bounded: Option<(&Mobject, &PathQuery)>,
) -> Result<ManimGeometryOptions, CoordinateAuthoringError> {
    let mut interval = clip_interval(x_range.unwrap_or(graph_range(graph)?), graph_range(graph)?)?;
    if let Some((bound, _)) = bounded {
        interval = clip_interval(interval, graph_range(bound)?)?;
    }
    let mut top = graph_interval_points(frame, graph_path, interval)?;
    if let Some((_, bound_path)) = bounded {
        let mut bottom = graph_interval_points(frame, bound_path, interval)?;
        bottom.reverse();
        top.extend(bottom);
    } else {
        // Manim c2p(x) omits the y component: use the physical x-axis
        // baseline, including ranges that do not contain numeric zero.
        let baseline = noon_geometry::origin_shift(frame.y().range());
        top.push(frame.coords_to_point(interval[1], baseline)?);
        top.insert(0, frame.coords_to_point(interval[0], baseline)?);
    }
    let mut options = ManimGeometryOptions::path(closed_path(&top)?)?;
    let color = noon_core::BLUE;
    options.set_color(color.red.into(), color.green.into(), color.blue.into(), 1.0)?;
    options.set_fill_opacity(0.3)?;
    options.set_stroke_opacity(0.3)?;
    Ok(options)
}

pub(crate) fn riemann_sample_inputs(
    graph: &Mobject,
    bounded: Option<&Mobject>,
    options: &RiemannRectangleOptions,
) -> Result<(Vec<f64>, Vec<f64>), CoordinateAuthoringError> {
    if !options.dx.is_finite() || options.dx <= 0.0 || !options.width_scale_factor.is_finite() {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "invalid Riemann rectangle dimensions",
        ));
    }
    let mut interval = clip_interval(
        options.x_range.unwrap_or(graph_range(graph)?),
        graph_range(graph)?,
    )?;
    if let Some(bound) = bounded {
        interval = clip_interval(interval, graph_range(bound)?)?;
    }
    let plan = riemann_partition_plan(interval, options.dx).map_err(PlotAuthoringError::from)?;
    let starts = plan.parameters()[..plan.parameters().len() - 1].to_vec();
    let sample_xs = starts
        .iter()
        .map(|&x| match options.sample {
            RiemannSample::Left => x,
            RiemannSample::Right => x + options.dx,
            RiemannSample::Center => x + options.dx * 0.5,
        })
        .collect();
    Ok((starts, sample_xs))
}

pub(crate) fn prepare_riemann_paths(
    frame: AxesFrame,
    graph: &Mobject,
    graph_path: &PathQuery,
    bounded: Option<(&Mobject, &PathQuery)>,
    options: RiemannRectangleOptions,
) -> Result<Vec<(noon_core::VectorPath, SemanticStyle)>, CoordinateAuthoringError> {
    let (starts, sample_xs) =
        riemann_sample_inputs(graph, bounded.map(|(object, _)| object), &options)?;
    let top_values = sample_xs
        .iter()
        .map(|&x| graph_y_at(frame, graph_path, x))
        .collect::<Result<Vec<_>, _>>()?;
    let baseline_values = bounded
        .map(|(_, path)| {
            starts
                .iter()
                .map(|&x| graph_y_at(frame, path, x))
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    prepare_riemann_paths_with_values(
        frame,
        graph,
        bounded.map(|(object, _)| object),
        options,
        &starts,
        &sample_xs,
        &top_values,
        baseline_values.as_deref(),
    )
}

pub(crate) fn prepare_riemann_paths_with_values(
    frame: AxesFrame,
    graph: &Mobject,
    bounded: Option<&Mobject>,
    options: RiemannRectangleOptions,
    starts: &[f64],
    sample_xs: &[f64],
    top_values: &[f64],
    baseline_values: Option<&[f64]>,
) -> Result<Vec<(noon_core::VectorPath, SemanticStyle)>, CoordinateAuthoringError> {
    if starts.len() != sample_xs.len()
        || starts.len() != top_values.len()
        || baseline_values.is_some_and(|values| values.len() != starts.len())
        || top_values.iter().any(|value| !value.is_finite())
        || baseline_values.is_some_and(|values| values.iter().any(|value| !value.is_finite()))
    {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "Riemann callback values must match the Rust sample plan and be finite",
        ));
    }
    let (planned_starts, planned_sample_xs) = riemann_sample_inputs(graph, bounded, &options)?;
    if planned_starts != starts || planned_sample_xs != sample_xs {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "Riemann callback positions do not match the Rust sample plan",
        ));
    }
    let colors = crate::color_gradient(&options.colors, starts.len())?;
    let style = SemanticStyle {
        fill_opacity: options.fill_opacity,
        stroke: Some(SemanticPaint::Solid(options.stroke_color)),
        stroke_width: options.stroke_width * 0.01,
        stroke_width_mode: StrokeWidthMode::ScreenSpace,
        stroke_cap: StrokeCap::Butt,
        stroke_join: StrokeJoin::Miter,
        ..Default::default()
    };
    if !style.is_finite()
        || !(0.0..=1.0).contains(&options.fill_opacity)
        || options.stroke_width < 0.0
    {
        return Err(CoordinateAuthoringError::InvalidOptions("invalid Riemann paint"));
    }
    let mut paths = Vec::new();
    paths.try_reserve_exact(starts.len()).map_err(|_| CoordinateError::AllocationFailed)?;
    for (index, (&x, &sample_x)) in starts.iter().zip(sample_xs).enumerate() {
        let top = top_values[index];
        let baseline = baseline_values.map_or(
            noon_geometry::origin_shift(frame.y().range()),
            |values| values[index],
        );
        let width_end = x + options.dx * options.width_scale_factor;
        let bottom_left = frame.coords_to_point(x, baseline)?;
        let bottom_right = frame.coords_to_point(width_end, baseline)?;
        let graph_point = frame.coords_to_point(sample_x, top)?;
        let min_x = bottom_left[0].min(bottom_right[0]).min(graph_point[0]);
        let max_x = bottom_left[0].max(bottom_right[0]).max(graph_point[0]);
        let min_y = bottom_left[1].min(bottom_right[1]).min(graph_point[1]);
        let max_y = bottom_left[1].max(bottom_right[1]).max(graph_point[1]);
        let mut color = colors[index];
        let height_precision =
            4.0 * f64::from(f32::EPSILON) * graph_point[1].abs().max(bottom_left[1].abs()).max(1.0)
                / frame.y().unit_size();
        if top < baseline - height_precision && options.show_signed_area {
            color = noon_core::Color::rgba(
                1.0 - color.red, 1.0 - color.green, 1.0 - color.blue, color.alpha,
            );
        }
        let mut style = style.clone();
        style.fill = Some(SemanticPaint::Solid(color));
        if options.blend {
            style.stroke = Some(SemanticPaint::Solid(color));
        }
        paths.push((closed_path(&[
            [max_x, max_y], [min_x, max_y], [min_x, min_y], [max_x, min_y],
        ])?, style));
    }
    Ok(paths)
}

/// Reuse the existing bounded NumPy-compatible plot sample planner. Its extra
/// endpoint is preparation data, not an extra rectangle charged to admission.
fn riemann_partition_plan(
    interval: [f64; 2],
    dx: f64,
) -> Result<noon_geometry::PlotSamplingPlan, PlotPreparationError> {
    let mut options = PlotSamplingOptions::parametric(&[interval[0], interval[1], dx])?;
    options.max_samples = noon_geometry::DEFAULT_PLOT_SAMPLE_LIMIT + 1;
    options.plan()
}

pub(crate) fn publish_path_family(
    store: Rc<RefCell<SemanticStore>>,
    paths: Vec<(noon_core::VectorPath, SemanticStyle)>,
) -> Result<MobjectFamily, CoordinateAuthoringError> {
    let family = publish_path_family_with(&mut store.borrow_mut(), paths, |store, transaction| {
        transaction.apply(store).map_err(AuthoringError::from)
    })?;
    Ok(MobjectFamily::from_node(store, family)?)
}

pub(crate) fn publish_path_family_with(
    store: &mut SemanticStore,
    paths: Vec<(noon_core::VectorPath, SemanticStyle)>,
    publish: impl FnOnce(
        &mut SemanticStore,
        SemanticMutationTransaction,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError>,
) -> Result<SemanticNodeId, AuthoringError> {
    let (paths, styles): (Vec<_>, Vec<_>) = paths.into_iter().unzip();
    store.with_geometry_paths(paths, |store, handles| {
        let mut transaction = SemanticMutationTransaction::new();
        let family = transaction.create_node(SemanticNodeCreation::family());
        for (handle, style) in handles.iter().zip(styles) {
            let mut state = crate::semantic_mobject::manim_path_resource_state(*handle);
            state.style = style;
            let leaf = transaction.create_node(SemanticNodeCreation::object(state));
            transaction.add_member(family, leaf);
        }
        publish(store, transaction)?
            .resolve(family)
            .ok_or(AuthoringError::UnresolvedCreatedNode(family))
    })
}

fn closed_path(points: &[[f64; 2]]) -> Result<noon_core::VectorPath, CoordinateAuthoringError> {
    if points.len() < 3 || points.iter().flatten().any(|value| !value.is_finite()) {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "area geometry requires finite polygon points",
        ));
    }
    let mut path = noon_core::VectorPath::new();
    for (index, &[x, y]) in points.iter().enumerate() {
        let point = noon_core::Vec2::new(
            crate::integration::authoring_render_f64("area x", x)? as f32,
            crate::integration::authoring_render_f64("area y", y)? as f32,
        );
        path = if index == 0 {
            path.move_to(point)
        } else {
            path.line_to(point)
        };
    }
    Ok(path.close())
}

pub(super) fn graph_range(graph: &Mobject) -> Result<[f64; 2], CoordinateAuthoringError> {
    match graph.state()?.role() {
        SemanticObjectRole::FunctionPlot(role) if role.is_valid() => Ok(role.range()),
        _ => Err(CoordinateAuthoringError::InvalidOptions(
            "area helpers require an Axes graph",
        )),
    }
}

fn clip_interval(
    mut interval: [f64; 2],
    range: [f64; 2],
) -> Result<[f64; 2], CoordinateAuthoringError> {
    if interval.iter().any(|value| !value.is_finite()) || interval[0] >= interval[1] {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "area interval must be finite and increasing",
        ));
    }
    interval[0] = interval[0].max(range[0]);
    interval[1] = interval[1].min(range[1]);
    if interval[0] >= interval[1] {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "area interval does not overlap graph",
        ));
    }
    Ok(interval)
}

fn graph_interval_points(
    frame: AxesFrame,
    graph: &PathQuery,
    interval: [f64; 2],
) -> Result<Vec<[f64; 2]>, CoordinateAuthoringError> {
    let mut points = Vec::with_capacity(graph.curve_count() * 4 + 2);
    points.push(frame.coords_to_point(interval[0], graph_y_at(frame, graph, interval[0])?)?);
    for index in 0..graph.curve_count() {
        for point in graph.curve_points(index)? {
            let x = frame.point_to_coords(point.into())?[0];
            if interval[0] <= x && x <= interval[1] {
                points.push([point.0, point.1]);
            }
        }
    }
    points.push(frame.coords_to_point(interval[1], graph_y_at(frame, graph, interval[1])?)?);
    Ok(points)
}

fn graph_y_at(
    frame: AxesFrame,
    graph: &PathQuery,
    x: f64,
) -> Result<f64, CoordinateAuthoringError> {
    let start = frame.point_to_coords(graph.point_from_proportion(0.0)?.into())?;
    let end = frame.point_to_coords(graph.point_from_proportion(1.0)?.into())?;
    if x < start[0].min(end[0]) || x > start[0].max(end[0]) {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "graph path does not cover requested input",
        ));
    }
    let ascending = start[0] <= end[0];
    let mut low = 0.0;
    let mut high = 1.0;
    for _ in 0..60 {
        let mid = (low + high) * 0.5;
        let point = frame.point_to_coords(graph.point_from_proportion(mid)?.into())?;
        if (point[0] < x) == ascending {
            low = mid;
        } else {
            high = mid;
        }
    }
    Ok(frame.point_to_coords(graph.point_from_proportion((low + high) * 0.5)?.into())?[1])
}

#[cfg(test)]
mod tests;
