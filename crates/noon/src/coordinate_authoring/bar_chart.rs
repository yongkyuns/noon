//! Retained BarChart composition over ordinary axes and rectangle leaves.
//!
//! The chart retains semantic handles only.  Its values are authoring input for
//! the next explicit update; coordinate mapping, family traversal and mutation
//! publication remain shared Rust operations.

use std::rc::Rc;

use noon_core::{
    Color, SemanticMutationTransaction, SemanticNodeCreation, SemanticObjectProperty,
    SemanticPaint, SemanticStyle, SemanticVec3, StoredGeometry,
};

use super::{CoordinateAuthoringError, ManimAxes, ManimAxesOptions};
use crate::{AuthoringError, MobjectFamily, Scene};

const DEFAULT_BAR_COLORS: [Color; 5] = [
    Color::from_hex(0x003F5C),
    Color::from_hex(0x58508D),
    Color::from_hex(0xBC5090),
    Color::from_hex(0xFF6361),
    Color::from_hex(0xFFA600),
];

/// Inert inputs for one retained BarChart.
#[derive(Clone, Debug)]
pub struct ManimBarChartOptions {
    pub values: Vec<f64>,
    pub y_range: [f64; 3],
    pub x_length: f64,
    pub y_length: f64,
    pub bar_width: f64,
    pub bar_fill_opacity: f64,
    pub bar_stroke_width: f64,
    pub bar_colors: Vec<Color>,
}

impl ManimBarChartOptions {
    pub fn new(values: Vec<f64>, y_range: [f64; 3], x_length: f64, y_length: f64) -> Self {
        Self {
            values,
            y_range,
            x_length,
            y_length,
            bar_width: 0.6,
            bar_fill_opacity: 0.7,
            bar_stroke_width: 3.0,
            bar_colors: DEFAULT_BAR_COLORS.to_vec(),
        }
    }

    /// Resolve Manim Community's constructor defaults once in the shared
    /// authoring layer.  Language adapters pass absent optional arguments here;
    /// they do not retain a parallel range/layout policy.
    pub fn manim_defaults(
        values: Vec<f64>,
        y_range: Option<&[f64]>,
        x_length: Option<f64>,
        y_length: Option<f64>,
    ) -> Result<Self, CoordinateAuthoringError> {
        const FRAME_WIDTH: f64 = 14.222_222_222_222_221;
        const FRAME_HEIGHT: f64 = 8.0;
        let y_length = y_length.unwrap_or(FRAME_HEIGHT - 4.0);
        let x_length = x_length.unwrap_or((values.len() as f64).min(FRAME_WIDTH - 2.0));
        let maximum = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let inferred = [
            values.iter().copied().fold(0.0_f64, f64::min),
            maximum,
            (maximum / y_length * 100.0).round() / 100.0,
        ];
        let y_range = match y_range {
            None | Some([]) => inferred,
            Some([start, end]) => [*start, *end, inferred[2]],
            Some([start, end, step]) => [*start, *end, *step],
            Some(_) => {
                return Err(CoordinateAuthoringError::InvalidOptions(
                    "bar chart y_range requires two or three values",
                ))
            }
        };
        Ok(Self::new(values, y_range, x_length, y_length))
    }
}

/// One semantic family containing axes followed by the bar family.
#[derive(Clone, Debug)]
pub struct ManimBarChart {
    family: MobjectFamily,
    axes: ManimAxes,
    bars: MobjectFamily,
}

impl ManimBarChart {
    /// Cold-store construction used by the typed WASM authoring adapter. The
    /// transaction admits axes, bars and their nested families together.
    pub fn create(
        store: Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        options: &ManimBarChartOptions,
    ) -> Result<Self, CoordinateAuthoringError> {
        let (transaction, chart, axes, bars) = prepare(options)?;
        let result = transaction
            .apply(&mut store.borrow_mut())
            .map_err(AuthoringError::from)?;
        Self::from_result(store, &result, chart, axes, bars)
    }

    pub fn family(&self) -> &MobjectFamily {
        &self.family
    }
    pub fn axes(&self) -> &ManimAxes {
        &self.axes
    }
    pub fn bars(&self) -> &MobjectFamily {
        &self.bars
    }
    /// Read values from the authoritative bar leaves and current axes frame.
    pub fn values(&self) -> Result<Vec<f64>, CoordinateAuthoringError> {
        let frame = self.axes.authored_frame()?;
        let baseline = noon_geometry::origin_shift(frame.y().range());
        let store = Rc::clone(self.family.integration_store());
        let store = store.borrow();
        direct_bar_nodes(&self.bars)?
            .into_iter()
            .map(|node| {
                let state = store
                    .semantic_object_state_checked(node)
                    .map_err(AuthoringError::from)?;
                let point = [state.transform.translation.x, state.transform.translation.y];
                Ok((frame.point_to_coords(point)?[1] - baseline) * 2.0)
            })
            .collect()
    }

    /// Update the prefix represented by `values` in one atomic semantic
    /// transaction. Bars keep their identities, geometry resources and style
    /// unless `update_colors` explicitly requests the standard recoloring.
    pub fn change_bar_values(
        &mut self,
        scene: &mut Scene,
        values: &[f64],
        update_colors: bool,
    ) -> Result<(), CoordinateAuthoringError> {
        if !Rc::ptr_eq(scene.integration_store(), self.family.integration_store()) {
            return Err(AuthoringError::ForeignStore.into());
        }
        if values.iter().any(|value| !value.is_finite()) {
            return Err(CoordinateAuthoringError::InvalidOptions(
                "bar values must be finite",
            ));
        }
        let frame = self.axes.authored_frame()?;
        let store = scene.integration_store().borrow();
        let transaction = prepare_value_update(&store, frame, &self.bars, values, update_colors)?;
        drop(store);
        scene.apply_semantic_transaction(transaction)?;
        Ok(())
    }

    /// Cold-store counterpart for typed WASM authoring before a live execution
    /// context exists. It uses the same prepared transaction as Scene-owned
    /// updates, without introducing a frontend mutation model.
    pub fn change_bar_values_in_store(
        &mut self,
        store: &Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        values: &[f64],
        update_colors: bool,
    ) -> Result<(), CoordinateAuthoringError> {
        if !Rc::ptr_eq(store, self.family.integration_store()) {
            return Err(AuthoringError::ForeignStore.into());
        }
        if values.iter().any(|value| !value.is_finite()) {
            return Err(CoordinateAuthoringError::InvalidOptions(
                "bar values must be finite",
            ));
        }
        let frame = self.axes.authored_frame()?;
        let borrowed = store.borrow();
        let transaction =
            prepare_value_update(&borrowed, frame, &self.bars, values, update_colors)?;
        drop(borrowed);
        transaction
            .apply(&mut store.borrow_mut())
            .map_err(AuthoringError::from)?;
        Ok(())
    }

    /// Apply a cold typed-authoring update through this chart's own semantic
    /// store. The store identity is derived from the retained family handle.
    pub fn change_bar_values_cold(
        &mut self,
        values: &[f64],
        update_colors: bool,
    ) -> Result<(), CoordinateAuthoringError> {
        let store = Rc::clone(self.family.integration_store());
        self.change_bar_values_in_store(&store, values, update_colors)
    }

    /// Running-session update from one effective axes observation, published by
    /// the same prepared mutation transaction as cold and Scene-owned edits.
    pub fn change_bar_values_live(
        &mut self,
        live: &mut crate::LiveSession<'_>,
        values: &[f64],
        update_colors: bool,
    ) -> Result<(), CoordinateAuthoringError> {
        self.family.validate()?;
        if !Rc::ptr_eq(live.integration_store(), self.family.integration_store()) {
            return Err(AuthoringError::ForeignStore.into());
        }
        if values.iter().any(|value| !value.is_finite()) {
            return Err(CoordinateAuthoringError::InvalidOptions(
                "bar values must be finite",
            ));
        }
        let frame = live.effective_axes_frame(&self.axes)?;
        let store = live.integration_store().borrow();
        let transaction = prepare_value_update(&store, frame, &self.bars, values, update_colors)?;
        drop(store);
        live.apply(transaction)?;
        Ok(())
    }
}

pub(crate) fn prepare_value_update(
    store: &noon_core::SemanticStore,
    frame: noon_geometry::AxesFrame,
    bars: &MobjectFamily,
    values: &[f64],
    update_colors: bool,
) -> Result<SemanticMutationTransaction, CoordinateAuthoringError> {
    let nodes = direct_bar_nodes_prefix(store, bars, values.len())?;
    let first = first_bar_node(store, bars)?;
    let (unit_x, unit_y) = axis_vector(frame, true)?;
    let unit = unit_x.hypot(unit_y);
    let width = store
        .semantic_object_state_checked(first)
        .map_err(AuthoringError::from)?
        .transform
        .scale
        .x
        .abs()
        / unit;
    let colors = if update_colors {
        let all_nodes = direct_bar_nodes(bars)?;
        let colors = bar_colors(store, &all_nodes)?;
        Some((all_nodes, colors))
    } else {
        None
    };
    let mut transaction = SemanticMutationTransaction::new();
    for (index, (&node, &value)) in nodes.iter().zip(values).enumerate() {
        let (translation, scale, rotation_z) = bar_transform(frame, index, width, value)?;
        transaction.set_property(node, SemanticObjectProperty::Translation, translation);
        transaction.set_property(node, SemanticObjectProperty::Scale, scale);
        transaction.set_property(node, SemanticObjectProperty::RotationZ, rotation_z);
    }
    if let Some((nodes, colors)) = colors {
        let count = nodes.len();
        for (index, node) in nodes.into_iter().enumerate() {
            let mut style = store
                .semantic_object_state_checked(node)
                .map_err(AuthoringError::from)?
                .style
                .clone();
            set_bar_color(&mut style, gradient_color(&colors, index, count));
            transaction.replace_style(node, style);
        }
    }
    Ok(transaction)
}

impl Scene {
    /// Construct a detached retained BarChart from shared axes and rectangle
    /// semantics. Add `chart.family()` to the scene explicitly.
    pub fn bar_chart(
        &mut self,
        options: &ManimBarChartOptions,
    ) -> Result<ManimBarChart, CoordinateAuthoringError> {
        let (transaction, chart, axes, bars) = prepare(options)?;
        let result = self.apply_semantic_transaction(transaction)?;
        ManimBarChart::from_result(
            Rc::clone(self.integration_store()),
            &result,
            chart,
            axes,
            bars,
        )
    }
}

impl ManimBarChart {
    pub(crate) fn from_result(
        store: Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        result: &noon_core::SemanticMutationTransactionResult,
        chart: noon_core::SemanticLocalNodeToken,
        axes: noon_core::SemanticLocalNodeToken,
        bars: noon_core::SemanticLocalNodeToken,
    ) -> Result<Self, CoordinateAuthoringError> {
        let resolve = |token| {
            result
                .resolve(token)
                .ok_or(AuthoringError::UnresolvedCreatedNode(token))
        };
        let family = MobjectFamily::from_node(Rc::clone(&store), resolve(chart)?)?;
        let axes =
            ManimAxes::from_family(MobjectFamily::from_node(Rc::clone(&store), resolve(axes)?)?)?;
        let bars = MobjectFamily::from_node(store, resolve(bars)?)?;
        Ok(Self { family, axes, bars })
    }

    pub fn from_family(family: MobjectFamily) -> Result<Self, CoordinateAuthoringError> {
        let members = family
            .integration_store()
            .borrow()
            .semantic_family_members_checked(family.node_id())
            .map_err(AuthoringError::from)?;
        let [axes, bars] = members.as_slice() else {
            return Err(CoordinateAuthoringError::InvalidTopology);
        };
        let store = Rc::clone(family.integration_store());
        Ok(Self {
            axes: ManimAxes::from_family(MobjectFamily::from_node(Rc::clone(&store), *axes)?)?,
            bars: MobjectFamily::from_node(store, *bars)?,
            family,
        })
    }
}

pub(crate) fn prepare(
    options: &ManimBarChartOptions,
) -> Result<
    (
        SemanticMutationTransaction,
        noon_core::SemanticLocalNodeToken,
        noon_core::SemanticLocalNodeToken,
        noon_core::SemanticLocalNodeToken,
    ),
    CoordinateAuthoringError,
> {
    validate_options(options)?;
    let count = options.values.len();
    let frame = noon_geometry::AxesFrame::centered(
        [0.0, count as f64, 1.0],
        options.y_range,
        options.x_length,
        options.y_length,
    )?;
    let axes_options = ManimAxesOptions::new(
        [0.0, count as f64, 1.0],
        options.y_range,
        options.x_length,
        options.y_length,
    );
    let x = super::prepare_line(frame.x(), axes_options.ticks, &axes_options.style)?;
    let y = super::prepare_line(frame.y(), axes_options.ticks, &axes_options.style)?;
    let mut transaction = SemanticMutationTransaction::new();
    let chart = transaction.create_node(SemanticNodeCreation::family());
    let axes = transaction.create_node(SemanticNodeCreation::family());
    let x = super::stage_line(&mut transaction, x);
    let y = super::stage_line(&mut transaction, y);
    transaction.add_member(axes, x);
    transaction.add_member(axes, y);
    let bars = transaction.create_node(SemanticNodeCreation::family());
    for (index, &value) in options.values.iter().enumerate() {
        let (translation, scale, rotation_z) =
            bar_transform(frame, index, options.bar_width, value)?;
        let mut style = SemanticStyle::default();
        style.stroke_width = options.bar_stroke_width;
        style.fill_opacity = options.bar_fill_opacity;
        set_bar_color(
            &mut style,
            gradient_color(&options.bar_colors, index, count),
        );
        let mut state = noon_core::SemanticObjectState::new(StoredGeometry::Rectangle {
            size: noon_core::Vec2::new(1.0, 1.0),
        });
        state.transform.translation = translation;
        state.transform.scale = scale;
        state.transform.rotation_z = rotation_z;
        state.style = style;
        let bar = transaction.create_node(SemanticNodeCreation::object(state));
        transaction.add_member(bars, bar);
    }
    transaction.add_member(chart, axes);
    transaction.add_member(chart, bars);
    Ok((transaction, chart, axes, bars))
}

fn validate_options(options: &ManimBarChartOptions) -> Result<(), CoordinateAuthoringError> {
    if options.values.is_empty() || options.values.iter().any(|value| !value.is_finite()) {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "bar chart requires finite values",
        ));
    }
    if !options.bar_width.is_finite() || !(0.0 < options.bar_width && options.bar_width <= 1.0) {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "bar_width must be in (0, 1]",
        ));
    }
    if !options.bar_fill_opacity.is_finite()
        || !options.bar_stroke_width.is_finite()
        || options.bar_stroke_width < 0.0
    {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "invalid bar style",
        ));
    }
    if options.bar_colors.is_empty() {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "bar_colors must not be empty",
        ));
    }
    Ok(())
}

fn direct_bar_nodes(
    family: &MobjectFamily,
) -> Result<Vec<noon_core::SemanticNodeId>, CoordinateAuthoringError> {
    family.validate()?;
    family
        .integration_store()
        .borrow()
        .semantic_family_members_checked(family.node_id())
        .map_err(AuthoringError::from)
        .map_err(Into::into)
}

/// Select a direct prefix by following the store's authoritative O(1) sibling
/// links.  Prefix updates must not allocate or inspect unrelated bars.
fn direct_bar_nodes_prefix(
    store: &noon_core::SemanticStore,
    family: &MobjectFamily,
    count: usize,
) -> Result<Vec<noon_core::SemanticNodeId>, CoordinateAuthoringError> {
    family.validate()?;
    let family = store
        .semantic_family_checked(family.node_id())
        .map_err(AuthoringError::from)?;
    let mut nodes = Vec::with_capacity(count);
    let mut current = family.first_member();
    while nodes.len() < count {
        let node = current.ok_or(CoordinateAuthoringError::InvalidTopology)?;
        nodes.push(node);
        current = family.next_member(node);
    }
    Ok(nodes)
}

fn first_bar_node(
    store: &noon_core::SemanticStore,
    family: &MobjectFamily,
) -> Result<noon_core::SemanticNodeId, CoordinateAuthoringError> {
    family.validate()?;
    store
        .semantic_family_checked(family.node_id())
        .map_err(AuthoringError::from)?
        .first_member()
        .ok_or(CoordinateAuthoringError::InvalidTopology)
}

fn bar_colors(
    store: &noon_core::SemanticStore,
    nodes: &[noon_core::SemanticNodeId],
) -> Result<Vec<Color>, CoordinateAuthoringError> {
    nodes
        .iter()
        .map(|node| {
            let state = store
                .semantic_object_state_checked(*node)
                .map_err(AuthoringError::from)?;
            match state.style.fill {
                Some(SemanticPaint::Solid(color)) => Ok(color),
                _ => Err(CoordinateAuthoringError::InvalidTopology),
            }
        })
        .collect()
}

fn bar_transform(
    frame: noon_geometry::AxesFrame,
    index: usize,
    bar_width: f64,
    value: f64,
) -> Result<(SemanticVec3, SemanticVec3, f64), CoordinateAuthoringError> {
    let x = index as f64 + 0.5;
    let baseline = noon_geometry::origin_shift(frame.y().range());
    let center = frame.coords_to_point(x, baseline + value * 0.5)?;
    let (x_unit_x, x_unit_y) = axis_vector(frame, true)?;
    let (y_unit_x, y_unit_y) = axis_vector(frame, false)?;
    let width = x_unit_x.hypot(x_unit_y) * bar_width;
    let height = y_unit_x.hypot(y_unit_y) * value.abs();
    let x_unit = x_unit_x.hypot(x_unit_y);
    let y_unit = y_unit_x.hypot(y_unit_y);
    // A retained Rectangle has rotation plus independent local scales, but no
    // shear component.  Reject a skewed authoritative axes frame before
    // staging any mutation rather than publishing a visually different chart.
    if x_unit == 0.0
        || y_unit == 0.0
        || (x_unit_x * y_unit_x + x_unit_y * y_unit_y).abs() > 1.0e-9 * x_unit * y_unit
    {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "bar chart requires orthogonal axes",
        ));
    }
    let rotation_z = x_unit_y.atan2(x_unit_x);
    // Rectangle transforms encode a rotated orthogonal basis.  Preserve the
    // y-axis orientation (including reflected axes) in its signed local scale.
    let orientation = (x_unit_x * y_unit_y - x_unit_y * y_unit_x).signum();
    Ok((
        SemanticVec3::new(center[0], center[1], 0.0),
        SemanticVec3::new(width, height * orientation, 1.0),
        rotation_z,
    ))
}

fn axis_vector(
    frame: noon_geometry::AxesFrame,
    x_axis: bool,
) -> Result<(f64, f64), CoordinateAuthoringError> {
    let baseline = noon_geometry::origin_shift(frame.y().range());
    let origin = frame.coords_to_point(0.0, baseline)?;
    let point = if x_axis {
        frame.coords_to_point(1.0, baseline)?
    } else {
        frame.coords_to_point(0.0, baseline + 1.0)?
    };
    Ok((point[0] - origin[0], point[1] - origin[1]))
}

fn set_bar_color(style: &mut SemanticStyle, color: Color) {
    style.fill = Some(SemanticPaint::Solid(color));
    style.stroke = Some(SemanticPaint::Solid(color));
}

fn gradient_color(colors: &[Color], index: usize, count: usize) -> Color {
    if colors.len() == 1 || count <= 1 {
        return colors[0];
    }
    let position = index as f64 * (colors.len() - 1) as f64 / (count - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    let t = (position - lower as f64) as f32;
    let from = colors[lower];
    let to = colors[upper];
    Color::rgba(
        from.red + (to.red - from.red) * t,
        from.green + (to.green - from.green) * t,
        from.blue + (to.blue - from.blue) * t,
        from.alpha + (to.alpha - from.alpha) * t,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> ManimBarChartOptions {
        ManimBarChartOptions::new(vec![-2.0, 0.0, 3.0], [-4.0, 4.0, 1.0], 6.0, 4.0)
    }

    #[test]
    fn bars_share_one_axes_frame_and_support_signed_and_zero_values() {
        let mut scene = Scene::new();
        let chart = scene.bar_chart(&options()).unwrap();
        let nodes = direct_bar_nodes(chart.bars()).unwrap();
        let store = scene.integration_store().borrow();
        let states: Vec<_> = nodes
            .iter()
            .map(|node| store.semantic_object_state_checked(*node).unwrap().clone())
            .collect();
        assert_eq!(states.len(), 3);
        assert!(states[0].transform.translation.y < states[1].transform.translation.y);
        assert_eq!(states[1].transform.scale.y, 0.0);
        assert!(states[2].transform.translation.y > states[1].transform.translation.y);
    }

    #[test]
    fn updates_are_atomic_and_touch_only_requested_bars_when_color_is_unchanged() {
        let mut scene = Scene::new();
        let mut chart = scene.bar_chart(&options()).unwrap();
        let nodes = direct_bar_nodes(chart.bars()).unwrap();
        let before: Vec<_> = nodes
            .iter()
            .map(|node| {
                scene
                    .integration_store()
                    .borrow()
                    .semantic_object_state_checked(*node)
                    .unwrap()
                    .clone()
            })
            .collect();
        chart
            .change_bar_values(&mut scene, &[1.0, -1.0], false)
            .unwrap();
        let after: Vec<_> = nodes
            .iter()
            .map(|node| {
                scene
                    .integration_store()
                    .borrow()
                    .semantic_object_state_checked(*node)
                    .unwrap()
                    .clone()
            })
            .collect();
        assert_ne!(before[0].transform, after[0].transform);
        assert_ne!(before[1].transform, after[1].transform);
        assert_eq!(before[2], after[2]);
        assert_eq!(before[0].style, after[0].style);

        let snapshot = after.clone();
        assert!(chart
            .change_bar_values(&mut scene, &[f64::NAN], false)
            .is_err());
        let rolled_back: Vec<_> = nodes
            .iter()
            .map(|node| {
                scene
                    .integration_store()
                    .borrow()
                    .semantic_object_state_checked(*node)
                    .unwrap()
                    .clone()
            })
            .collect();
        assert_eq!(snapshot, rolled_back);
    }

    #[test]
    fn update_uses_the_current_transformed_axes_frame() {
        let mut scene = Scene::new();
        let mut chart = scene.bar_chart(&options()).unwrap();
        chart.family().shift(2.0, -1.0).unwrap();
        chart.change_bar_values(&mut scene, &[2.0], false).unwrap();
        let node = direct_bar_nodes(chart.bars()).unwrap()[0];
        let state = scene
            .integration_store()
            .borrow()
            .semantic_object_state_checked(node)
            .unwrap()
            .clone();
        let expected = chart
            .axes()
            .authored_frame()
            .unwrap()
            .coords_to_point(0.5, 1.0)
            .unwrap();
        assert!((state.transform.translation.x - expected[0]).abs() < 1e-9);
        assert!((state.transform.translation.y - expected[1]).abs() < 1e-9);
    }

    #[test]
    fn updates_preserve_a_ninety_degree_axes_basis() {
        let mut scene = Scene::new();
        let mut chart = scene.bar_chart(&options()).unwrap();
        chart
            .family()
            .rotate(
                std::f64::consts::FRAC_PI_2,
                crate::ManimRotationPivot::Center,
            )
            .unwrap();
        chart.change_bar_values(&mut scene, &[2.0], false).unwrap();

        let node = direct_bar_nodes(chart.bars()).unwrap()[0];
        let state = scene
            .integration_store()
            .borrow()
            .semantic_object_state_checked(node)
            .unwrap()
            .clone();
        assert!((state.transform.rotation_z - std::f64::consts::FRAC_PI_2).abs() < 1.0e-9);
        assert!(state.transform.scale.x > 0.0);
        assert!(state.transform.scale.y > 0.0);
    }

    #[test]
    fn short_update_leaves_a_large_chart_suffix_unchanged() {
        let mut scene = Scene::new();
        let values = (0..2_000).map(|index| index as f64).collect();
        let mut chart = scene
            .bar_chart(&ManimBarChartOptions::new(
                values,
                [0.0, 2_000.0, 1.0],
                12.0,
                4.0,
            ))
            .unwrap();
        let nodes = direct_bar_nodes(chart.bars()).unwrap();
        let suffix = nodes[1_999];
        let before = scene
            .integration_store()
            .borrow()
            .semantic_object_state_checked(suffix)
            .unwrap()
            .clone();
        chart
            .change_bar_values(&mut scene, &[123.0], false)
            .unwrap();
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .semantic_object_state_checked(suffix)
                .unwrap(),
            &before
        );
    }

    #[test]
    fn skewed_axes_are_rejected_without_mutating_bars() {
        let mut scene = Scene::new();
        let mut chart = scene.bar_chart(&options()).unwrap();
        chart
            .axes()
            .y_axis()
            .unwrap()
            .family()
            .rotate(0.1, crate::ManimRotationPivot::Center)
            .unwrap();
        let node = direct_bar_nodes(chart.bars()).unwrap()[0];
        let before = scene
            .integration_store()
            .borrow()
            .semantic_object_state_checked(node)
            .unwrap()
            .clone();
        assert!(chart.change_bar_values(&mut scene, &[2.0], false).is_err());
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .semantic_object_state_checked(node)
                .unwrap(),
            &before
        );
    }

    #[test]
    fn manim_defaults_match_the_compatibility_constructor_policy() {
        let options =
            ManimBarChartOptions::manim_defaults(vec![-2.0, 3.0], None, None, None).unwrap();
        assert_eq!(options.y_range, [-2.0, 3.0, 0.75]);
        assert_eq!(options.x_length, 2.0);
        assert_eq!(options.y_length, 4.0);

        let explicit = ManimBarChartOptions::manim_defaults(
            vec![1.0, 2.0],
            Some(&[-1.0, 5.0]),
            Some(7.0),
            Some(2.0),
        )
        .unwrap();
        assert_eq!(explicit.y_range, [-1.0, 5.0, 1.0]);
    }

    #[test]
    fn copied_chart_reconstructs_semantics_from_its_family() {
        let mut scene = Scene::new();
        let chart = scene.bar_chart(&options()).unwrap();
        let copied = chart.family().copy_family().unwrap();
        let reconstructed = ManimBarChart::from_family(copied.root().clone()).unwrap();
        assert_eq!(reconstructed.values().unwrap(), vec![-2.0, 0.0, 3.0]);
        assert_ne!(reconstructed.family().node_id(), chart.family().node_id());
    }
}
