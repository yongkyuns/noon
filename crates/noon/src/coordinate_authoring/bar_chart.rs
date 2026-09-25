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
    for (index, &value) in options.values.iter().enumerate() {
        let (translation, scale, rotation_z) =
            bar_transform(frame, index, options.bar_width, value)?;
        let mut style = SemanticStyle::default();
        style.stroke_width = options.bar_stroke_width;
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
            stroke_width: options.bar_stroke_width,
        })));
        let bar = transaction.create_node(SemanticNodeCreation::object(state));
        transaction.add_member(bars, bar);
    }
    transaction.add_member(chart, bars);
    transaction.add_member(chart, axes);
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
        let updated_nodes = direct_bar_nodes(chart.bars()).unwrap();
        assert_ne!(updated_nodes[1], nodes[1]);
        let after: Vec<_> = updated_nodes
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
        let rolled_back: Vec<_> = updated_nodes
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
    fn skewed_axes_do_not_replace_nonzero_bar_geometry() {
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
        chart.change_bar_values(&mut scene, &[2.0], false).unwrap();
        let after = scene
            .integration_store()
            .borrow()
            .semantic_object_state_checked(node)
            .unwrap()
            .clone();
        assert_eq!(after.content, before.content);
        assert_eq!(after.transform.scale, before.transform.scale);
        assert_eq!(bar_metadata(&after).unwrap().value, 2.0);
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
    #[test]
    fn updates_use_authored_values_and_restore_constructor_palette() {
        let mut scene = Scene::new();
        let mut chart = scene.bar_chart(&options()).unwrap();
        let node = direct_bar_nodes(chart.bars()).unwrap()[0];
        let mut bar =
            crate::Mobject::from_node(Rc::clone(scene.integration_store()), node).unwrap();
        let original = bar.state().unwrap();
        bar.shift(1.0, 2.0).unwrap();
        bar.set_color(1.0, 0.0, 0.0, 1.0).unwrap();
        assert_eq!(chart.values().unwrap(), vec![-2.0, 0.0, 3.0]);
        let edge = bar.layout_bounds().unwrap().unwrap().max_y;
        chart.change_bar_values(&mut scene, &[4.0], true).unwrap();
        let after = bar.state().unwrap();
        let bounds = bar.layout_bounds().unwrap().unwrap();
        assert!((bounds.min_y - edge).abs() < 1e-8);
        assert!((bounds.height() - 2.0).abs() < 1e-8);
        assert_eq!(after.content, original.content);
        assert_eq!(after.style.fill, original.style.fill);
        assert_eq!(chart.values().unwrap(), vec![4.0, 0.0, 3.0]);
        let copy = chart.family().copy_family().unwrap();
        let copied = ManimBarChart::from_family(copy.root().clone()).unwrap();
        assert_eq!(copied.values().unwrap(), chart.values().unwrap());
    }
    #[test]
    fn rotated_value_change_stretches_current_world_height_locally() {
        let mut scene = Scene::new();
        let mut chart = scene.bar_chart(&options()).unwrap();
        chart
            .family()
            .rotate(0.37, crate::ManimRotationPivot::Point(0.0, 0.0))
            .unwrap();
        let nodes = direct_bar_nodes(chart.bars()).unwrap();
        let bar =
            crate::Mobject::from_node(Rc::clone(scene.integration_store()), nodes[0]).unwrap();
        let before = bar.layout_bounds().unwrap().unwrap();
        let unchanged = scene
            .integration_store()
            .borrow()
            .semantic_object_state_checked(nodes[2])
            .unwrap()
            .clone();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        chart.change_bar_values(&mut scene, &[4.0], false).unwrap();
        let after = bar.layout_bounds().unwrap().unwrap();
        assert!((after.width() - before.width()).abs() < 1e-6);
        assert!((after.height() - before.height() * 2.0).abs() < 1e-6);
        assert!((after.min_y - before.max_y).abs() < 1e-6);
        let store = scene.integration_store().borrow();
        assert_eq!(store.geometry_resources().len(), resources + 1);
        assert_eq!(
            store.semantic_object_state_checked(nodes[2]).unwrap(),
            &unchanged
        );
    }
}
