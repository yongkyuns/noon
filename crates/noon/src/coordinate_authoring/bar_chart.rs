//! Retained BarChart composition over ordinary axes and rectangle leaves.
//!
//! The chart retains semantic handles only.  Its values are authoring input for
//! the next explicit update; coordinate mapping, family traversal and mutation
//! publication remain shared Rust operations.

use std::{rc::Rc, sync::Arc};

use noon_core::{
    Color, SemanticBarMetadata, SemanticMutationTransaction, SemanticNodeCreation,
    SemanticObjectState, SemanticPaint, SemanticStyle, SemanticVec3, StoredGeometry,
};

use super::{CoordinateAuthoringError, ManimAxes, ManimAxesOptions};
use crate::{AuthoringError, MobjectFamily, Scene};

#[cfg(all(feature = "native-text", feature = "latex"))]
pub(crate) mod labels;

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
    /// Manim stroke units, converted once to Noon's canonical width on admission.
    pub bar_stroke_width: f64,
    pub bar_colors: Vec<Color>,
    pub bar_names: Option<Vec<String>>,
    pub name_font_size: f32,
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
            bar_names: None,
            name_font_size: 24.0,
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
            maximum.max(0.0),
            (maximum / y_length * 100.0).round_ties_even() / 100.0,
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

/// One semantic family containing the bar family followed by axes.
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

    /// Optional retained names attached to the horizontal axis.
    pub fn x_labels(&self) -> Result<Option<MobjectFamily>, CoordinateAuthoringError> {
        let axis = self.axes.x_axis()?;
        let store = self.family.integration_store();
        let node = store
            .borrow()
            .semantic_family_checked(axis.family().node_id())
            .map_err(AuthoringError::from)?
            .members_iter()
            .nth(2);
        node.map(|node| MobjectFamily::from_node(Rc::clone(store), node).map_err(Into::into))
            .transpose()
    }

    /// The retained DecimalNumber family attached to the vertical axis.
    #[cfg(all(feature = "native-text", feature = "latex"))]
    pub fn y_labels(&self) -> Result<MobjectFamily, CoordinateAuthoringError> {
        let axis = self.axes.y_axis()?;
        let store = self.family.integration_store();
        let node = store
            .borrow()
            .semantic_family_checked(axis.family().node_id())
            .map_err(AuthoringError::from)?
            .members_iter()
            .nth(2)
            .ok_or(CoordinateAuthoringError::InvalidTopology)?;
        MobjectFamily::from_node(Rc::clone(store), node).map_err(Into::into)
    }

    /// Resolve an authoritative bar prefix without materializing the remaining
    /// chart leaves. This keeps wrapper refreshes local after zero-value swaps.
    pub fn bar_prefix(
        &self,
        count: usize,
    ) -> Result<Vec<crate::Mobject>, CoordinateAuthoringError> {
        let store = self.family.integration_store();
        direct_bar_nodes_prefix(&store.borrow(), &self.bars, count)?
            .into_iter()
            .map(|node| crate::Mobject::from_node(Rc::clone(store), node).map_err(Into::into))
            .collect()
    }
    /// Read authored numeric inputs, independently of current rectangle geometry.
    pub fn values(&self) -> Result<Vec<f64>, CoordinateAuthoringError> {
        let store = self.family.integration_store().borrow();
        direct_bar_nodes(&self.bars)?
            .into_iter()
            .map(|node| {
                Ok(bar_metadata(
                    store
                        .semantic_object_state_checked(node)
                        .map_err(AuthoringError::from)?,
                )?
                .value)
            })
            .collect()
    }

    pub fn change_bar_values(
        &mut self,
        scene: &mut Scene,
        values: &[f64],
        update_colors: bool,
    ) -> Result<(), CoordinateAuthoringError> {
        scene.change_chart_values(self, values, update_colors)
    }

    pub fn change_bar_values_cold(
        &mut self,
        values: &[f64],
        update_colors: bool,
    ) -> Result<(), CoordinateAuthoringError> {
        let store = Rc::clone(self.family.integration_store());
        let frame = self.axes.authored_frame()?;
        let prepared = {
            let borrowed = store.borrow();
            prepare_value_update(
                &borrowed,
                frame,
                &self.bars,
                values,
                update_colors,
                |node| {
                    borrowed
                        .semantic_object_state_checked(node)
                        .cloned()
                        .map_err(AuthoringError::from)
                        .map_err(Into::into)
                },
            )?
        };
        prepared.publish(&mut store.borrow_mut(), |store, transaction| {
            transaction.apply(store).map_err(AuthoringError::from)
        })?;
        Ok(())
    }

    pub fn change_bar_values_live(
        &mut self,
        live: &mut crate::LiveSession<'_>,
        values: &[f64],
        update_colors: bool,
    ) -> Result<(), CoordinateAuthoringError> {
        if !Rc::ptr_eq(live.integration_store(), self.family.integration_store()) {
            return Err(AuthoringError::ForeignStore.into());
        }
        let frame = live.effective_axes_frame(&self.axes)?;
        let prepared = {
            let store = live.integration_store().borrow();
            prepare_value_update(&store, frame, &self.bars, values, update_colors, |node| {
                let object =
                    crate::Mobject::from_node(Rc::clone(self.family.integration_store()), node)?;
                live.capture_mobject_state(&object).map_err(Into::into)
            })?
        };
        live.publish_path_edits(prepared)?;
        Ok(())
    }
}

fn bar_metadata(
    state: &SemanticObjectState,
) -> Result<SemanticBarMetadata, CoordinateAuthoringError> {
    state
        .bar_metadata()
        .copied()
        .ok_or(CoordinateAuthoringError::InvalidTopology)
}

/// Prepare only the requested bars, plus all paint when explicitly recoloring.
/// Rotation/shear uses the ordinary world-axis deformation and resource admission.
fn prepare_value_update(
    store: &noon_core::SemanticStore,
    frame: noon_geometry::AxesFrame,
    bars: &MobjectFamily,
    values: &[f64],
    update_colors: bool,
    mut capture: impl FnMut(
        noon_core::SemanticNodeId,
    ) -> Result<SemanticObjectState, CoordinateAuthoringError>,
) -> Result<crate::path_editing::PreparedPathEdits, CoordinateAuthoringError> {
    use crate::semantic_mobject::{
        boundary_for_content, scale_state_about_center, stage_state_changes,
    };
    if values.iter().any(|v| !v.is_finite()) {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "bar values must be finite",
        ));
    }
    let nodes = if update_colors {
        direct_bar_nodes(bars)?
    } else {
        direct_bar_nodes_prefix(store, bars, values.len())?
    };
    if values.len() > nodes.len() {
        return Err(CoordinateAuthoringError::InvalidTopology);
    }
    let mut transaction = SemanticMutationTransaction::new();
    let mut replacements = Vec::new();
    for (index, node) in nodes.into_iter().enumerate() {
        let authored = store
            .semantic_object_state_checked(node)
            .map_err(AuthoringError::from)?;
        let mut state = capture(node)?;
        let mut metadata = bar_metadata(authored)?;
        let mut target = noon_core::SemanticTransactionNodeRef::from(node);
        let mut path = None;
        if let Some(&value) = values.get(index) {
            if metadata.value == 0.0 {
                // Manim admits a fresh leaf when the previous authored value was zero.
                state = fresh_bar(frame, index, value, metadata)?;
                let fresh = transaction.create_node(SemanticNodeCreation::object(state.clone()));
                transaction.remove_member(bars.node_id(), node);
                transaction.add_member(bars.node_id(), fresh);
                transaction.reorder_member(
                    bars.node_id(),
                    fresh,
                    store
                        .semantic_family_checked(bars.node_id())
                        .map_err(AuthoringError::from)?
                        .next_member(node),
                );
                target = fresh.into();
            } else {
                let bounds = boundary_for_content(store, state.content, state.transform)?
                    .ok_or(AuthoringError::MissingLayoutBounds(node))?;
                let center = (
                    (bounds.min_x + bounds.max_x) * 0.5,
                    (bounds.min_y + bounds.max_y) * 0.5,
                );
                let factor = (value / metadata.value).abs();
                let old_edge = if metadata.value > 0.0 {
                    bounds.min_y
                } else {
                    bounds.max_y
                };
                let edge = if value / metadata.value < 0.0 {
                    if metadata.value > 0.0 {
                        1.0
                    } else {
                        -1.0
                    }
                } else if metadata.value > 0.0 {
                    -1.0
                } else {
                    1.0
                };
                let destination = (
                    center.0,
                    old_edge - edge * (bounds.max_y - bounds.min_y) * factor * 0.5,
                );
                match crate::dimension_fit::world_scale_factors(
                    state.transform.rotation_z,
                    1.0,
                    factor,
                ) {
                    Ok((x, y)) => scale_state_about_center(store, &mut state, x, y, destination)?,
                    Err(_) => {
                        path = Some(crate::family_affine::world_scaled_path(
                            store,
                            &state,
                            1.0,
                            factor,
                            center,
                            destination,
                        )?);
                    }
                }
            }
            metadata.value = value;
            transaction.set_bar_metadata(target, Some(Arc::new(metadata)));
        }
        if update_colors {
            set_bar_color(&mut state.style, metadata.original_color);
        }
        if target == noon_core::SemanticTransactionNodeRef::from(node) {
            if let Some(path) = path {
                replacements.push((node, state, path));
            } else {
                stage_state_changes(&mut transaction, node, authored, &state);
            }
        } else if update_colors {
            transaction.replace_style(target, state.style);
        }
    }
    Ok(
        crate::path_editing::PreparedPathEdits::prepare(store, replacements)?
            .with_transaction(transaction),
    )
}

fn fresh_bar(
    frame: noon_geometry::AxesFrame,
    index: usize,
    value: f64,
    metadata: SemanticBarMetadata,
) -> Result<SemanticObjectState, CoordinateAuthoringError> {
    let (translation, scale, rotation) = bar_transform(frame, index, metadata.width, value)?;
    let mut state = SemanticObjectState::new(StoredGeometry::Rectangle {
        size: noon_core::Vec2::new(1.0, 1.0),
    });
    state.transform.translation = translation;
    state.transform.scale = scale;
    state.transform.rotation_z = rotation;
    state.style.fill_opacity = metadata.fill_opacity;
    state.style.stroke_width = metadata.stroke_width;
    state.style.stroke = Some(SemanticPaint::Solid(noon_core::WHITE));
    state.style.stroke_width_mode = noon_core::StrokeWidthMode::ScreenSpace;
    state.set_bar_metadata(Some(Arc::new(SemanticBarMetadata { value, ..metadata })));
    Ok(state)
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
        let [bars, axes] = members.as_slice() else {
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
    if options.bar_names.is_some() {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "bar names require create_with_axis_labels and a LaTeX backend",
        ));
    }
    let prepared = prepare_chart(options)?;
    Ok((
        prepared.transaction,
        prepared.chart,
        prepared.axes,
        prepared.bars,
    ))
}

struct PreparedChart {
    transaction: SemanticMutationTransaction,
    chart: noon_core::SemanticLocalNodeToken,
    axes: noon_core::SemanticLocalNodeToken,
    bars: noon_core::SemanticLocalNodeToken,
    x_axis: noon_core::SemanticLocalNodeToken,
    y_axis: noon_core::SemanticLocalNodeToken,
    frame: noon_geometry::AxesFrame,
}

fn prepare_chart(
    options: &ManimBarChartOptions,
) -> Result<PreparedChart, CoordinateAuthoringError> {
    validate_options(options)?;
    let count = options.values.len();
    let colors = crate::color_gradient(&options.bar_colors, count)?;
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
    let stroke_width = options.bar_stroke_width * 0.01;
    for (index, &value) in options.values.iter().enumerate() {
        let (translation, scale, rotation_z) =
            bar_transform(frame, index, options.bar_width, value)?;
        let mut style = SemanticStyle::default();
        style.stroke_width = stroke_width;
        style.stroke_width_mode = noon_core::StrokeWidthMode::ScreenSpace;
        style.fill_opacity = options.bar_fill_opacity;
        set_bar_color(&mut style, colors[index]);
        let mut state = noon_core::SemanticObjectState::new(StoredGeometry::Rectangle {
            size: noon_core::Vec2::new(1.0, 1.0),
        });
        state.transform.translation = translation;
        state.transform.scale = scale;
        state.transform.rotation_z = rotation_z;
        state.style = style;
        state.set_bar_metadata(Some(Arc::new(SemanticBarMetadata {
            value,
            original_color: colors[index],
            width: options.bar_width,
            fill_opacity: options.bar_fill_opacity,
            stroke_width,
        })));
        let bar = transaction.create_node(SemanticNodeCreation::object(state));
        transaction.add_member(bars, bar);
    }
    transaction.add_member(chart, bars);
    transaction.add_member(chart, axes);
    Ok(PreparedChart {
        transaction,
        chart,
        axes,
        bars,
        x_axis: x,
        y_axis: y,
        frame,
    })
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
    if !options.name_font_size.is_finite() || options.name_font_size <= 0.0 {
        return Err(CoordinateAuthoringError::InvalidOptions(
            "invalid bar-name font size",
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

fn bar_transform(
    frame: noon_geometry::AxesFrame,
    index: usize,
    bar_width: f64,
    value: f64,
) -> Result<(SemanticVec3, SemanticVec3, f64), CoordinateAuthoringError> {
    let origin = frame.coords_to_point(0.0, 0.0)?;
    let across = frame.coords_to_point(bar_width, 0.0)?;
    let top = frame.coords_to_point(0.0, value)?;
    let base = frame.coords_to_point(index as f64 + 0.5, 0.0)?;
    let width = across[0] - origin[0];
    let height = (top[1] - origin[1]).abs();
    let sign = if value >= 0.0 { 1.0 } else { -1.0 };
    Ok((
        SemanticVec3::new(base[0], base[1] + sign * height * 0.5, 0.0),
        SemanticVec3::new(width, height, 1.0),
        0.0,
    ))
}

fn set_bar_color(style: &mut SemanticStyle, color: Color) {
    style.fill = Some(SemanticPaint::Solid(color));
    style.stroke = Some(SemanticPaint::Solid(color));
}

#[cfg(all(feature = "native-text", feature = "latex"))]
pub(crate) struct PreparedLabeledChart {
    chart: PreparedChart,
    labels: crate::text_authoring::PreparedDecimalLabels,
    names: Option<labels::PreparedBarLabels>,
}

#[cfg(all(feature = "native-text", feature = "latex"))]
impl PreparedLabeledChart {
    pub(crate) fn prepare(
        options: &ManimBarChartOptions,
        label_options: &crate::plot_presentation::NumberLabelOptions,
        backend: &mut impl crate::LatexBackend,
    ) -> Result<Self, crate::plot_presentation::NumberLabelAuthoringError> {
        let chart = prepare_chart(options)?;
        let labels = crate::text_authoring::PreparedDecimalLabels::prepare(
            backend,
            chart.frame.y(),
            None,
            label_options,
        )?;
        let names = options
            .bar_names
            .as_ref()
            .map(|names| {
                labels::PreparedBarLabels::prepare_names(
                    names,
                    &options.values,
                    chart.frame.x(),
                    options.name_font_size,
                    backend,
                )
            })
            .transpose()?;
        Ok(Self {
            chart,
            labels,
            names,
        })
    }

    pub(crate) fn publish(
        self,
        store: &mut noon_core::SemanticStore,
        publish: impl FnOnce(
            &mut noon_core::SemanticStore,
            SemanticMutationTransaction,
        ) -> Result<
            noon_core::SemanticMutationTransactionResult,
            crate::TextAuthoringError,
        >,
    ) -> Result<
        (
            noon_core::SemanticMutationTransactionResult,
            [noon_core::SemanticLocalNodeToken; 3],
        ),
        crate::plot_presentation::NumberLabelAuthoringError,
    > {
        let chart = self.chart;
        let (result, _) = self.labels.publish_with_transaction(
            store,
            Some(chart.y_axis.into()),
            chart.transaction,
            |store, transaction| match self.names {
                Some(names) => names
                    .publish_into(store, Some(chart.x_axis.into()), transaction, publish)
                    .map(|(result, _)| result),
                None => publish(store, transaction),
            },
        )?;
        Ok((result, [chart.chart, chart.axes, chart.bars]))
    }
}

#[cfg(all(feature = "native-text", feature = "latex"))]
impl ManimBarChart {
    /// Prepare bars, names, and retained DecimalNumber Y labels before one
    /// semantic publication. Language adapters only provide inert options and a
    /// compiler; family ownership and placement remain Rust-owned.
    pub fn create_with_axis_labels(
        store: Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        options: &ManimBarChartOptions,
        label_options: &crate::plot_presentation::NumberLabelOptions,
        backend: &mut impl crate::LatexBackend,
    ) -> Result<Self, crate::plot_presentation::NumberLabelAuthoringError> {
        let prepared = PreparedLabeledChart::prepare(options, label_options, backend)?;
        let (result, [chart, axes, bars]) =
            prepared.publish(&mut store.borrow_mut(), |store, transaction| {
                transaction
                    .apply(store)
                    .map_err(AuthoringError::from)
                    .map_err(crate::TextAuthoringError::Semantic)
            })?;
        Self::from_result(store, &result, chart, axes, bars).map_err(Into::into)
    }
}

#[cfg(all(feature = "native-text", feature = "latex"))]
impl Scene {
    /// Publish bars, names, and axis text together through this Scene's owner.
    pub fn bar_chart_with_axis_labels(
        &mut self,
        options: &ManimBarChartOptions,
        labels: &crate::plot_presentation::NumberLabelOptions,
        backend: &mut impl crate::LatexBackend,
    ) -> Result<ManimBarChart, crate::plot_presentation::NumberLabelAuthoringError> {
        let prepared = PreparedLabeledChart::prepare(options, labels, backend)?;
        let (result, [chart, axes, bars]) =
            self.with_semantic_publication(|store, publish| {
                Ok(prepared.publish(store, |store, transaction| {
                    publish(store, transaction).map_err(crate::TextAuthoringError::Semantic)
                }))
            })??;
        ManimBarChart::from_result(
            Rc::clone(self.integration_store()),
            &result,
            chart,
            axes,
            bars,
        )
        .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests;
