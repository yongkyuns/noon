//! Shared Manim SampleSpace composition over ordinary rectangles and families.

use crate::{AuthoringError, Color, ManimGeometryOptions, Mobject, MobjectFamily, Scene};
use noon_core::{
    Bounds2D64, SemanticLocalNodeToken, SemanticMutationTransaction, SemanticNodeCreation,
    SemanticNodeId, SemanticObjectRole, SemanticStore,
};
use std::{cell::RefCell, collections::HashSet, rc::Rc};

const PINNED_EPSILON: f64 = 0.0001;
const MANIM_CAIRO_LINE_WIDTH_MULTIPLE: f64 = 0.01;

/// Manim SampleSpace construction inputs. Stroke widths use Manim's Cairo
/// pixel-width units and are converted to scene units when authoring geometry.
#[derive(Clone, Debug)]
pub struct SampleSpaceOptions {
    pub width: f64,
    pub height: f64,
    pub fill_color: Color,
    pub fill_opacity: f64,
    pub stroke_color: Color,
    pub stroke_width: f64,
}

impl Default for SampleSpaceOptions {
    fn default() -> Self {
        Self {
            width: 3.0,
            height: 3.0,
            fill_color: Color::from_hex(0x525252), // Manim DARK_GREY
            fill_opacity: 1.0,
            stroke_color: Color::from_hex(0xBBBBBB), // Manim LIGHT_GREY
            stroke_width: 0.5,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SampleSpace {
    rectangle: Mobject,
    family: MobjectFamily,
}

#[derive(Debug)]
pub enum SampleSpaceError {
    Authoring(AuthoringError),
    Live(crate::LiveSessionError),
    InvalidProbability,
    InvalidFamily,
}
impl From<AuthoringError> for SampleSpaceError {
    fn from(value: AuthoringError) -> Self {
        Self::Authoring(value)
    }
}
impl From<crate::LiveSessionError> for SampleSpaceError {
    fn from(value: crate::LiveSessionError) -> Self {
        Self::Live(value)
    }
}
impl std::fmt::Display for SampleSpaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Authoring(e) => e.fmt(f),
            Self::Live(e) => e.fmt(f),
            Self::InvalidProbability => {
                f.write_str("probabilities must be finite, nonnegative, and sum to at most one")
            }
            Self::InvalidFamily => f.write_str("semantic family is not a SampleSpace"),
        }
    }
}
impl std::error::Error for SampleSpaceError {}

impl From<SampleSpaceError> for crate::LiveSessionError {
    fn from(value: SampleSpaceError) -> Self {
        match value {
            SampleSpaceError::Live(error) => error,
            SampleSpaceError::Authoring(error) => error.into(),
            error => Self::Mobject(error.to_string()),
        }
    }
}

impl SampleSpace {
    /// Construct the SampleSpace base rectangle and family in one semantic
    /// transaction owned by `scene`.
    pub fn new(
        scene: &mut Scene,
        width: f64,
        height: f64,
        fill: Color,
        fill_opacity: f64,
        stroke: Color,
        stroke_width: f64,
    ) -> Result<Self, SampleSpaceError> {
        Self::new_with_options(
            scene,
            &SampleSpaceOptions {
                width,
                height,
                fill_color: fill,
                fill_opacity,
                stroke_color: stroke,
                stroke_width,
            },
        )
    }

    pub fn new_with_options(
        scene: &mut Scene,
        options: &SampleSpaceOptions,
    ) -> Result<Self, SampleSpaceError> {
        let store = Rc::clone(scene.integration_store());
        let (transaction, family, rectangle) = {
            let mut semantic_store = store.borrow_mut();
            prepare_sample_space(&mut semantic_store, options)?
        };
        let result = scene.apply_semantic_transaction(transaction)?;
        Self::from_creation_result(store, result, family, rectangle)
    }

    /// Construct and publish a SampleSpace through an existing live session.
    pub fn new_live(
        session: &mut crate::LiveSession<'_>,
        options: &SampleSpaceOptions,
    ) -> Result<Self, SampleSpaceError> {
        let store = Rc::clone(session.integration_store());
        let (result, (family, rectangle)) = session.apply_resource_transaction(
            |semantic_store| -> Result<_, SampleSpaceError> {
                let (transaction, family, rectangle) =
                    prepare_sample_space(semantic_store, options)?;
                Ok((transaction, (family, rectangle)))
            },
        )?;
        Self::from_creation_result(store, result, family, rectangle)
    }

    /// Construct the same Rust-owned composition directly in an existing
    /// semantic store. This is the typed in-process entry used by the WASM and
    /// Python adapters; no scene or frontend layout model is introduced.
    pub fn detached(
        store: Rc<RefCell<SemanticStore>>,
        options: &SampleSpaceOptions,
    ) -> Result<Self, SampleSpaceError> {
        let (transaction, family, rectangle) = {
            let mut semantic_store = store.borrow_mut();
            prepare_sample_space(&mut semantic_store, options)?
        };
        let result = transaction
            .apply(&mut store.borrow_mut())
            .map_err(AuthoringError::from)?;
        Self::from_creation_result(store, result, family, rectangle)
    }

    fn from_creation_result(
        store: Rc<RefCell<SemanticStore>>,
        result: noon_core::SemanticMutationTransactionResult,
        family: SemanticLocalNodeToken,
        rectangle: SemanticLocalNodeToken,
    ) -> Result<Self, SampleSpaceError> {
        let family = result
            .resolve(family)
            .ok_or(AuthoringError::UnresolvedCreatedNode(family))?;
        let rectangle = result
            .resolve(rectangle)
            .ok_or(AuthoringError::UnresolvedCreatedNode(rectangle))?;
        Ok(Self {
            rectangle: Mobject::from_node(Rc::clone(&store), rectangle)?,
            family: MobjectFamily::from_node(store, family)?,
        })
    }

    /// Rehydrate a SampleSpace wrapper from its retained semantic family.
    /// Partition relations are recovered from the partition object roles in the
    /// current family tree, so cloned/mutated families need no frontend cache.
    pub fn from_family(family: MobjectFamily) -> Result<Self, SampleSpaceError> {
        family.validate()?;
        let store = Rc::clone(family.integration_store());
        let first = store
            .borrow()
            .semantic_family_members_checked(family.node_id())
            .map_err(AuthoringError::from)?
            .into_iter()
            .next()
            .ok_or(SampleSpaceError::InvalidFamily)?;
        let rectangle = Mobject::from_node(Rc::clone(&store), first)
            .map_err(|_| SampleSpaceError::InvalidFamily)?;
        Ok(Self { rectangle, family })
    }

    pub fn rectangle(&self) -> &Mobject {
        &self.rectangle
    }
    pub fn family(&self) -> &MobjectFamily {
        &self.family
    }
    pub fn horizontal_parts(&self) -> Result<Option<MobjectFamily>, SampleSpaceError> {
        self.partition_family(SemanticObjectRole::SampleSpaceHorizontalPart)
    }
    pub fn vertical_parts(&self) -> Result<Option<MobjectFamily>, SampleSpaceError> {
        self.partition_family(SemanticObjectRole::SampleSpaceVerticalPart)
    }

    fn partition_family(
        &self,
        role: SemanticObjectRole,
    ) -> Result<Option<MobjectFamily>, SampleSpaceError> {
        self.family.validate()?;
        let store = self.family.integration_store();
        let matching = {
            let semantic_store = store.borrow();
            let roots = semantic_store
                .semantic_family_members_checked(self.family.node_id())
                .map_err(AuthoringError::from)?;
            let mut matching = None;
            for member in roots.into_iter().skip(1) {
                let Ok(children) = semantic_store.semantic_family_members_checked(member) else {
                    continue;
                };
                if family_contains_role(&semantic_store, &children, &role)? {
                    matching = Some(member);
                }
            }
            matching
        };
        matching
            .map(|id| MobjectFamily::from_node(Rc::clone(store), id).map_err(Into::into))
            .transpose()
    }
    pub fn complete_p_list(
        values: impl IntoIterator<Item = f64>,
    ) -> Result<Vec<f64>, SampleSpaceError> {
        let mut values: Vec<_> = values.into_iter().collect();
        if values.iter().any(|v| !v.is_finite() || *v < 0.0) {
            return Err(SampleSpaceError::InvalidProbability);
        }
        let remainder = 1.0 - values.iter().sum::<f64>();
        if remainder < -PINNED_EPSILON {
            return Err(SampleSpaceError::InvalidProbability);
        }
        // Pinned Manim appends a remainder only when abs(remainder) exceeds
        // EPSILON (0.0001); small negative roundoff is intentionally omitted.
        if remainder.abs() > PINNED_EPSILON {
            values.push(remainder);
        }
        Ok(values)
    }

    pub fn divide_horizontally(
        &mut self,
        scene: &mut Scene,
        probabilities: impl IntoIterator<Item = f64>,
        colors: &[Color],
    ) -> Result<MobjectFamily, SampleSpaceError> {
        let store = Rc::clone(scene.integration_store());
        let (transaction, family_token) =
            self.prepare_division(&store, probabilities, colors, false, true)?;
        let result = scene.apply_semantic_transaction(transaction)?;
        self.commit_division(store, result, family_token)
    }

    pub fn divide_vertically(
        &mut self,
        scene: &mut Scene,
        probabilities: impl IntoIterator<Item = f64>,
        colors: &[Color],
    ) -> Result<MobjectFamily, SampleSpaceError> {
        let store = Rc::clone(scene.integration_store());
        let (transaction, family_token) =
            self.prepare_division(&store, probabilities, colors, true, true)?;
        let result = scene.apply_semantic_transaction(transaction)?;
        self.commit_division(store, result, family_token)
    }

    /// Shared-store form for adapters whose store already owns the semantic
    /// scene. Geometry and family edges still commit as one transaction.
    pub fn divide_horizontally_detached(
        &mut self,
        probabilities: impl IntoIterator<Item = f64>,
        colors: &[Color],
    ) -> Result<MobjectFamily, SampleSpaceError> {
        let store = Rc::clone(self.family.integration_store());
        let (transaction, family_token) =
            self.prepare_division(&store, probabilities, colors, false, true)?;
        let result = transaction
            .apply(&mut store.borrow_mut())
            .map_err(AuthoringError::from)?;
        self.commit_division(store, result, family_token)
    }

    pub fn divide_vertically_detached(
        &mut self,
        probabilities: impl IntoIterator<Item = f64>,
        colors: &[Color],
    ) -> Result<MobjectFamily, SampleSpaceError> {
        let store = Rc::clone(self.family.integration_store());
        let (transaction, family_token) =
            self.prepare_division(&store, probabilities, colors, true, true)?;
        let result = transaction
            .apply(&mut store.borrow_mut())
            .map_err(AuthoringError::from)?;
        self.commit_division(store, result, family_token)
    }

    pub fn divide_horizontally_live(
        &mut self,
        session: &mut crate::LiveSession<'_>,
        probabilities: impl IntoIterator<Item = f64>,
        colors: &[Color],
    ) -> Result<MobjectFamily, SampleSpaceError> {
        self.divide_live(session, probabilities, colors, false, true)
    }

    pub fn divide_vertically_live(
        &mut self,
        session: &mut crate::LiveSession<'_>,
        probabilities: impl IntoIterator<Item = f64>,
        colors: &[Color],
    ) -> Result<MobjectFamily, SampleSpaceError> {
        self.divide_live(session, probabilities, colors, true, true)
    }

    pub fn get_horizontal_division_live(
        &self,
        session: &mut crate::LiveSession<'_>,
        probabilities: impl IntoIterator<Item = f64>,
        colors: &[Color],
    ) -> Result<MobjectFamily, SampleSpaceError> {
        self.get_division_live(session, probabilities, colors, false)
    }

    pub fn get_vertical_division_live(
        &self,
        session: &mut crate::LiveSession<'_>,
        probabilities: impl IntoIterator<Item = f64>,
        colors: &[Color],
    ) -> Result<MobjectFamily, SampleSpaceError> {
        self.get_division_live(session, probabilities, colors, true)
    }

    fn divide_live(
        &mut self,
        session: &mut crate::LiveSession<'_>,
        probabilities: impl IntoIterator<Item = f64>,
        colors: &[Color],
        vertical: bool,
        attach_to_space: bool,
    ) -> Result<MobjectFamily, SampleSpaceError> {
        let store = Rc::clone(session.integration_store());
        let bounds = self.effective_bounds(session)?;
        let (result, family_token) = session.apply_resource_transaction(|semantic_store| {
            self.prepare_division_in_store(
                semantic_store,
                probabilities,
                colors,
                vertical,
                attach_to_space,
                bounds,
            )
        })?;
        self.commit_division(store, result, family_token)
    }

    fn get_division_live(
        &self,
        session: &mut crate::LiveSession<'_>,
        probabilities: impl IntoIterator<Item = f64>,
        colors: &[Color],
        vertical: bool,
    ) -> Result<MobjectFamily, SampleSpaceError> {
        let store = Rc::clone(session.integration_store());
        let bounds = self.effective_bounds(session)?;
        let (result, family_token) = session.apply_resource_transaction(|semantic_store| {
            self.prepare_division_in_store(
                semantic_store,
                probabilities,
                colors,
                vertical,
                false,
                bounds,
            )
        })?;
        let id = result
            .resolve(family_token)
            .ok_or(AuthoringError::UnresolvedCreatedNode(family_token))?;
        MobjectFamily::from_node(store, id).map_err(Into::into)
    }

    fn effective_bounds(
        &self,
        session: &crate::LiveSession<'_>,
    ) -> Result<Bounds2D64, SampleSpaceError> {
        if !Rc::ptr_eq(session.integration_store(), self.family.integration_store()) {
            return Err(AuthoringError::ForeignStore.into());
        }
        session
            .capture_boundary_bounds(&self.rectangle)?
            .ok_or(SampleSpaceError::InvalidProbability)
    }

    /// Build horizontal parts as a detached family without adding them to this
    /// SampleSpace or changing its remembered `horizontal_parts` query.
    pub fn get_horizontal_division(
        &self,
        probabilities: impl IntoIterator<Item = f64>,
        colors: &[Color],
    ) -> Result<MobjectFamily, SampleSpaceError> {
        self.get_division(probabilities, colors, false)
    }

    /// Build vertical parts as a detached family without adding them to this
    /// SampleSpace or changing its remembered `vertical_parts` query.
    pub fn get_vertical_division(
        &self,
        probabilities: impl IntoIterator<Item = f64>,
        colors: &[Color],
    ) -> Result<MobjectFamily, SampleSpaceError> {
        self.get_division(probabilities, colors, true)
    }

    fn get_division(
        &self,
        probabilities: impl IntoIterator<Item = f64>,
        colors: &[Color],
        vertical: bool,
    ) -> Result<MobjectFamily, SampleSpaceError> {
        let store = Rc::clone(self.family.integration_store());
        let (transaction, family_token) =
            self.prepare_division(&store, probabilities, colors, vertical, false)?;
        let result = transaction
            .apply(&mut store.borrow_mut())
            .map_err(AuthoringError::from)?;
        let id = result
            .resolve(family_token)
            .ok_or(AuthoringError::UnresolvedCreatedNode(family_token))?;
        MobjectFamily::from_node(store, id).map_err(Into::into)
    }

    fn prepare_division(
        &self,
        store: &Rc<RefCell<SemanticStore>>,
        probabilities: impl IntoIterator<Item = f64>,
        colors: &[Color],
        vertical: bool,
        attach_to_space: bool,
    ) -> Result<(SemanticMutationTransaction, SemanticLocalNodeToken), SampleSpaceError> {
        let bounds = self
            .rectangle
            .layout_bounds()?
            .ok_or(SampleSpaceError::InvalidProbability)?;
        self.prepare_division_at_bounds(
            store,
            probabilities,
            colors,
            vertical,
            attach_to_space,
            bounds,
        )
    }

    fn prepare_division_at_bounds(
        &self,
        store: &Rc<RefCell<SemanticStore>>,
        probabilities: impl IntoIterator<Item = f64>,
        colors: &[Color],
        vertical: bool,
        attach_to_space: bool,
        bounds: Bounds2D64,
    ) -> Result<(SemanticMutationTransaction, SemanticLocalNodeToken), SampleSpaceError> {
        if !Rc::ptr_eq(store, self.family.integration_store())
            || !Rc::ptr_eq(store, self.rectangle.integration_store())
        {
            return Err(AuthoringError::ForeignStore.into());
        }
        self.family.validate()?;
        self.rectangle.validate()?;

        self.prepare_division_in_store(
            &mut store.borrow_mut(),
            probabilities,
            colors,
            vertical,
            attach_to_space,
            bounds,
        )
    }

    fn prepare_division_in_store(
        &self,
        semantic_store: &mut SemanticStore,
        probabilities: impl IntoIterator<Item = f64>,
        colors: &[Color],
        vertical: bool,
        attach_to_space: bool,
        bounds: Bounds2D64,
    ) -> Result<(SemanticMutationTransaction, SemanticLocalNodeToken), SampleSpaceError> {
        let probabilities = Self::complete_p_list(probabilities)?;
        // Manim samples its reference colors evenly over all parts. Validate an
        // empty palette even if a caller provides an empty probability iterable.
        let colors = crate::color_gradient(colors, probabilities.len())?;
        let mut cursor = if vertical { bounds.min_x } else { bounds.max_y };

        let mut transaction = SemanticMutationTransaction::new();
        let parts_family = transaction.create_node(SemanticNodeCreation::family());
        for (index, probability) in probabilities.into_iter().enumerate() {
            let (width, height, x, y) = if vertical {
                let width = bounds.width() * probability;
                let x = cursor + width * 0.5;
                cursor += width;
                (
                    width,
                    bounds.height(),
                    x,
                    (bounds.min_y + bounds.max_y) * 0.5,
                )
            } else {
                let height = bounds.height() * probability;
                let y = cursor - height * 0.5;
                cursor -= height;
                (
                    bounds.width(),
                    height,
                    (bounds.min_x + bounds.max_x) * 0.5,
                    y,
                )
            };
            let color = colors[index];
            let mut geometry = ManimGeometryOptions::rectangle(width, height)?;
            geometry.set_semantic_role(if vertical {
                SemanticObjectRole::SampleSpaceVerticalPart
            } else {
                SemanticObjectRole::SampleSpaceHorizontalPart
            });
            geometry.set_fill(
                f64::from(color.red),
                f64::from(color.green),
                f64::from(color.blue),
                1.0,
            )?;
            let outline = Color::from_hex(0xBBBBBB); // Manim LIGHT_GREY from SampleSpace()
            geometry.set_stroke_color(
                f64::from(outline.red),
                f64::from(outline.green),
                f64::from(outline.blue),
                1.0,
            )?;
            geometry.set_stroke_width(0.5 * MANIM_CAIRO_LINE_WIDTH_MULTIPLE)?;
            geometry.set_translation(x, y)?;
            let state = geometry.with_state(semantic_store, |_, state| Ok(state))?;
            let part = transaction.create_node(SemanticNodeCreation::object(state));
            transaction.add_member(parts_family, part);
        }
        if attach_to_space {
            // Manim appends each new partition family to the SampleSpace in one
            // operation, preserving previous partitions and identity.
            transaction.add_member(self.family.node_id(), parts_family);
        }
        Ok((transaction, parts_family))
    }

    fn commit_division(
        &self,
        store: Rc<RefCell<SemanticStore>>,
        result: noon_core::SemanticMutationTransactionResult,
        family_token: SemanticLocalNodeToken,
    ) -> Result<MobjectFamily, SampleSpaceError> {
        let id = result
            .resolve(family_token)
            .ok_or(AuthoringError::UnresolvedCreatedNode(family_token))?;
        MobjectFamily::from_node(store, id).map_err(Into::into)
    }
}

fn family_contains_role(
    store: &SemanticStore,
    children: &[SemanticNodeId],
    role: &SemanticObjectRole,
) -> Result<bool, SampleSpaceError> {
    let mut pending = children.to_vec();
    let mut visited = HashSet::new();
    while let Some(node) = pending.pop() {
        if !visited.insert(node) {
            continue;
        }
        if let Ok(state) = store.semantic_object_state_checked(node) {
            if &state.role() == role {
                return Ok(true);
            }
        } else if let Ok(members) = store.semantic_family_members_checked(node) {
            pending.extend(members);
        }
    }
    Ok(false)
}

fn prepare_sample_space(
    store: &mut SemanticStore,
    options: &SampleSpaceOptions,
) -> Result<
    (
        SemanticMutationTransaction,
        SemanticLocalNodeToken,
        SemanticLocalNodeToken,
    ),
    SampleSpaceError,
> {
    let mut geometry = ManimGeometryOptions::rectangle(options.width, options.height)?;
    geometry.set_fill(
        f64::from(options.fill_color.red),
        f64::from(options.fill_color.green),
        f64::from(options.fill_color.blue),
        options.fill_opacity,
    )?;
    geometry.set_stroke_color(
        f64::from(options.stroke_color.red),
        f64::from(options.stroke_color.green),
        f64::from(options.stroke_color.blue),
        1.0,
    )?;
    geometry.set_stroke_width(options.stroke_width * MANIM_CAIRO_LINE_WIDTH_MULTIPLE)?;
    let state = geometry.with_state(store, |_, state| Ok(state))?;

    let mut transaction = SemanticMutationTransaction::new();
    let family = transaction.create_node(SemanticNodeCreation::family());
    let rectangle = transaction.create_node(SemanticNodeCreation::object(state));
    transaction.add_member(family, rectangle);
    Ok((transaction, family, rectangle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BLUE, GREEN, RED, WHITE};
    use std::rc::Rc;

    #[test]
    fn partitions_complete_remainder_and_match_pinned_color_gradient() {
        let mut scene = Scene::new();
        let mut space =
            SampleSpace::new(&mut scene, 4.0, 2.0, crate::GRAY, 1.0, WHITE, 0.5).unwrap();
        let parts = space
            .divide_vertically(&mut scene, [0.25, 0.5], &[RED, GREEN, BLUE])
            .unwrap();
        assert_eq!(parts.layout().unwrap().leaves().len(), 3);
        assert_eq!(
            SampleSpace::complete_p_list([0.25, 0.5]).unwrap(),
            vec![0.25, 0.5, 0.25]
        );
        let layout = parts.layout().unwrap();
        let store = parts.integration_store().borrow();
        let actual_styles: Vec<_> = layout
            .leaves()
            .iter()
            .map(|leaf| {
                let style = &store.semantic_object_state_checked(*leaf).unwrap().style;
                let fill = match style.fill.as_ref().unwrap() {
                    noon_core::SemanticPaint::Solid(color) => *color,
                    _ => panic!("SampleSpace partition fill should be solid"),
                };
                let stroke = match style.stroke.as_ref().unwrap() {
                    noon_core::SemanticPaint::Solid(color) => *color,
                    _ => panic!("SampleSpace partition stroke should be solid"),
                };
                (fill, stroke, style.stroke_width)
            })
            .collect();
        assert_eq!(
            actual_styles
                .iter()
                .map(|style| style.0)
                .collect::<Vec<_>>(),
            crate::color_gradient(&[RED, GREEN, BLUE], 3).unwrap(),
        );
        assert!(actual_styles.iter().all(|(_, stroke, width)| {
            *stroke == Color::from_hex(0xBBBBBB) && (*width - 0.005).abs() < 1e-12
        }));
        assert!((parts.layout().unwrap().bounds().unwrap().width() - 4.0).abs() < 1e-6);
    }

    #[test]
    fn defaults_match_pinned_sample_space_rectangle_style() {
        let mut scene = Scene::new();
        let space =
            SampleSpace::new_with_options(&mut scene, &SampleSpaceOptions::default()).unwrap();
        let state = space.rectangle().state().unwrap();
        assert_eq!(
            state.style.fill,
            Some(noon_core::SemanticPaint::Solid(Color::from_hex(0x525252)))
        );
        assert_eq!(
            state.style.stroke,
            Some(noon_core::SemanticPaint::Solid(Color::from_hex(0xBBBBBB)))
        );
        assert_eq!(state.style.fill_opacity, 1.0);
        assert!((state.style.stroke_width - 0.005).abs() < 1e-12);
    }

    #[test]
    fn probability_tolerance_matches_pinned_manim_epsilon() {
        assert_eq!(
            SampleSpace::complete_p_list([0.4, 0.59995]).unwrap().len(),
            2
        );
        assert_eq!(
            SampleSpace::complete_p_list([0.4, 0.5998]).unwrap().len(),
            3
        );
        assert!(SampleSpace::complete_p_list([0.6, 0.4002]).is_err());
    }

    #[test]
    fn construction_and_partition_reject_invalid_work_without_partial_publication() {
        let mut scene = Scene::new();
        let before = scene.revision();
        assert!(SampleSpace::new(&mut scene, f64::NAN, 2.0, crate::GRAY, 1.0, WHITE, 0.5).is_err());
        assert_eq!(scene.revision(), before);

        let mut space =
            SampleSpace::new(&mut scene, 4.0, 2.0, crate::GRAY, 1.0, WHITE, 0.5).unwrap();
        let before = scene.revision();
        assert!(
            space
                .divide_horizontally(&mut scene, [0.5, 0.7], &[BLUE])
                .is_err()
        );
        assert_eq!(scene.revision(), before);
        assert!(space.horizontal_parts().unwrap().is_none());
    }

    #[test]
    fn division_rejects_foreign_scene_before_publishing_nodes() {
        let mut owner = Scene::new();
        let mut other = Scene::new();
        let mut space =
            SampleSpace::new(&mut owner, 4.0, 2.0, crate::GRAY, 1.0, WHITE, 0.5).unwrap();
        let before = other.revision();
        assert!(
            space
                .divide_horizontally(&mut other, [0.5], &[BLUE])
                .is_err()
        );
        assert_eq!(other.revision(), before);
        assert!(space.horizontal_parts().unwrap().is_none());
    }

    #[test]
    fn partitions_tile_the_transformed_rectangle_world_axis_bounds() {
        let mut scene = Scene::new();
        let mut space =
            SampleSpace::new(&mut scene, 4.0, 2.0, crate::GRAY, 1.0, WHITE, 0.5).unwrap();
        space
            .rectangle()
            .clone()
            .rotate(std::f64::consts::FRAC_PI_4)
            .unwrap();
        let target = space.rectangle().layout_bounds().unwrap().unwrap();

        let parts = space
            .divide_vertically(&mut scene, [0.25, 0.75], &[RED, BLUE])
            .unwrap();
        let leaves = parts.layout().unwrap().leaves().to_vec();
        let part_bounds: Vec<_> = leaves
            .into_iter()
            .map(|leaf| {
                Mobject::from_node(Rc::clone(parts.integration_store()), leaf)
                    .unwrap()
                    .layout_bounds()
                    .unwrap()
                    .unwrap()
            })
            .collect();
        assert_eq!(part_bounds.len(), 2);
        for bounds in &part_bounds {
            assert!((bounds.min_y - target.min_y).abs() < 1e-6);
            assert!((bounds.max_y - target.max_y).abs() < 1e-6);
        }
        assert!((part_bounds[0].min_x - target.min_x).abs() < 1e-6);
        assert!((part_bounds[0].max_x - part_bounds[1].min_x).abs() < 1e-6);
        assert!((part_bounds[1].max_x - target.max_x).abs() < 1e-6);
    }

    #[test]
    fn partition_queries_follow_family_structure_and_survive_semantic_copy() {
        let mut scene = Scene::new();
        let mut space =
            SampleSpace::new(&mut scene, 4.0, 2.0, crate::GRAY, 1.0, WHITE, 0.5).unwrap();
        let first = space
            .divide_horizontally(&mut scene, [0.5], &[RED])
            .unwrap();
        let second = space
            .divide_horizontally(&mut scene, [0.25], &[BLUE])
            .unwrap();
        assert_eq!(
            space.horizontal_parts().unwrap().unwrap().node_id(),
            second.node_id()
        );

        space.family().remove((&second).into()).unwrap();
        assert_eq!(
            space.horizontal_parts().unwrap().unwrap().node_id(),
            first.node_id()
        );
        space.family().remove((&first).into()).unwrap();
        assert!(space.horizontal_parts().unwrap().is_none());

        let copied_family = space.family().copy_family().unwrap();
        let copied = SampleSpace::from_family(copied_family.root().clone()).unwrap();
        assert_eq!(
            copied.rectangle().node_id(),
            copied_family.mobject(space.rectangle()).unwrap().node_id()
        );
        assert!(copied.horizontal_parts().unwrap().is_none());

        let mut populated =
            SampleSpace::new_with_options(&mut scene, &SampleSpaceOptions::default()).unwrap();
        populated
            .divide_horizontally(&mut scene, [0.25, 0.5], &[RED, BLUE])
            .unwrap();
        populated
            .divide_vertically(&mut scene, [0.4], &[GREEN])
            .unwrap();
        let clone = populated.family().copy_family().unwrap();
        let rehydrated = SampleSpace::from_family(clone.root().clone()).unwrap();
        assert_ne!(rehydrated.family().node_id(), populated.family().node_id());
        assert_eq!(
            rehydrated
                .horizontal_parts()
                .unwrap()
                .unwrap()
                .layout()
                .unwrap()
                .leaves()
                .len(),
            3
        );
        assert_eq!(
            rehydrated
                .vertical_parts()
                .unwrap()
                .unwrap()
                .layout()
                .unwrap()
                .leaves()
                .len(),
            2
        );
    }
}
