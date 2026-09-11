//! Shared retained Arrow, Vector, and DoubleArrow authoring.
//!
//! One Arrow is a semantic family whose renderable leaves are an analytic Line
//! shaft plus retained filled triangular tip paths. Geometry, buff shortening,
//! tip sizing, stroke-width capping, dependent scaling, and family publication
//! are Rust-owned. Frontends retain only typed handles.

use crate::{AuthoringError, ManimGeometryOptions, Mobject, MobjectFamily};
use noon_core::{
    Color, SemanticMutationTransaction, SemanticNodeCreation, SemanticObjectState, SemanticPaint,
    SemanticStore, StoredGeometry, Vec2, VectorPath,
};
use std::{cell::RefCell, rc::Rc};

pub const DEFAULT_ARROW_TIP_LENGTH: f64 = 0.35;
pub const DEFAULT_ARROW_TIP_LENGTH_RATIO: f64 = 0.25;
/// Manim's default 5 stroke-width units per scene unit after Cairo's 0.01 conversion.
pub const DEFAULT_ARROW_STROKE_WIDTH_RATIO: f64 = 0.05;
/// Manim's default Arrow stroke width 6 after Cairo's 0.01 conversion.
pub const DEFAULT_ARROW_STROKE_WIDTH: f64 = 0.06;

/// Inert shared constructor intent for Arrow, Vector, or DoubleArrow.
///
/// Width values are in Noon's semantic scene units, matching
/// [`ManimGeometryOptions::set_stroke_width`]. The Python facade performs only
/// Manim's public stroke-unit conversion before forwarding them here.
#[derive(Clone, Debug)]
pub struct ManimArrowOptions {
    prototype: ManimGeometryOptions,
    start: (f64, f64),
    end: (f64, f64),
    buff: f64,
    tip_length: f64,
    max_tip_length_to_length_ratio: f64,
    max_stroke_width_to_length_ratio: f64,
    start_tip: bool,
    z_index: f64,
}

impl ManimArrowOptions {
    pub fn arrow(
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
    ) -> Result<Self, AuthoringError> {
        let mut prototype = ManimGeometryOptions::line(start_x, start_y, end_x, end_y)?;
        prototype.set_stroke_width(DEFAULT_ARROW_STROKE_WIDTH)?;
        Ok(Self {
            prototype,
            start: checked_point("arrow start", start_x, start_y)?,
            end: checked_point("arrow end", end_x, end_y)?,
            buff: 0.25,
            tip_length: DEFAULT_ARROW_TIP_LENGTH,
            max_tip_length_to_length_ratio: DEFAULT_ARROW_TIP_LENGTH_RATIO,
            max_stroke_width_to_length_ratio: DEFAULT_ARROW_STROKE_WIDTH_RATIO,
            start_tip: false,
            z_index: 0.0,
        })
    }

    pub fn vector(direction_x: f64, direction_y: f64) -> Result<Self, AuthoringError> {
        let mut options = Self::arrow(0.0, 0.0, direction_x, direction_y)?;
        options.buff = 0.0;
        Ok(options)
    }

    pub fn double_arrow(
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
    ) -> Result<Self, AuthoringError> {
        let mut options = Self::arrow(start_x, start_y, end_x, end_y)?;
        options.start_tip = true;
        Ok(options)
    }

    pub fn set_buff(&mut self, buff: f64) -> Result<(), AuthoringError> {
        self.buff = crate::integration::authoring_render_f64("arrow buff", buff)?;
        Ok(())
    }

    pub fn set_tip_length(&mut self, length: f64) -> Result<(), AuthoringError> {
        self.tip_length = nonnegative("arrow tip length", length)?;
        Ok(())
    }

    pub fn set_max_tip_length_to_length_ratio(&mut self, ratio: f64) -> Result<(), AuthoringError> {
        self.max_tip_length_to_length_ratio = nonnegative("arrow tip length ratio", ratio)?;
        Ok(())
    }

    pub fn set_max_stroke_width_to_length_ratio(
        &mut self,
        ratio: f64,
    ) -> Result<(), AuthoringError> {
        self.max_stroke_width_to_length_ratio = nonnegative("arrow stroke width ratio", ratio)?;
        Ok(())
    }

    pub fn set_z_index(&mut self, value: f64) -> Result<(), AuthoringError> {
        self.prototype.set_z_index(value)?;
        self.z_index = value;
        Ok(())
    }

    pub fn set_translation(&mut self, x: f64, y: f64) -> Result<(), AuthoringError> {
        self.prototype.set_translation(x, y)
    }

    pub fn set_scale(&mut self, x: f64, y: f64) -> Result<(), AuthoringError> {
        self.prototype.set_scale(x, y)
    }

    pub fn set_rotation(&mut self, angle: f64) -> Result<(), AuthoringError> {
        self.prototype.set_rotation(angle)
    }

    pub fn set_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), AuthoringError> {
        self.prototype.set_color(red, green, blue, alpha)
    }

    pub fn set_stroke_width(&mut self, width: f64) -> Result<(), AuthoringError> {
        self.prototype.set_stroke_width(width)
    }

    pub fn set_stroke_width_mode(&mut self, mode: &str) -> Result<(), AuthoringError> {
        self.prototype.set_stroke_width_mode(mode)
    }

    pub fn set_stroke_join(&mut self, join: &str) -> Result<(), AuthoringError> {
        self.prototype.set_stroke_join(join)
    }

    pub fn set_stroke_cap(&mut self, cap: &str) -> Result<(), AuthoringError> {
        self.prototype.set_stroke_cap(cap)
    }

    pub fn set_object_opacity(&mut self, opacity: f64) -> Result<(), AuthoringError> {
        self.prototype.set_object_opacity(opacity)
    }

    fn prepare(self, store: &mut SemanticStore) -> Result<PreparedArrow, AuthoringError> {
        let ShortenedLine {
            visible_start,
            visible_end,
            direction,
            length,
        } = shortened_line(self.start, self.end, self.buff)?;
        let effective_tip_length = self
            .tip_length
            .min(self.max_tip_length_to_length_ratio * length);
        let end_base = (
            visible_end.0 - direction.0 * effective_tip_length,
            visible_end.1 - direction.1 * effective_tip_length,
        );
        let start_base = (
            visible_start.0 + direction.0 * effective_tip_length,
            visible_start.1 + direction.1 * effective_tip_length,
        );
        let shaft_start = if self.start_tip {
            start_base
        } else {
            visible_start
        };
        let shaft_end = end_base;

        let mut shaft = self.prototype.into_state(store)?;
        shaft.content = StoredGeometry::Line {
            start: lower_point("arrow shaft start", shaft_start)?,
            end: lower_point("arrow shaft end", shaft_end)?,
        }
        .into();
        let initial_stroke_width = shaft.style.stroke_width;
        shaft.style.stroke_width =
            initial_stroke_width.min(self.max_stroke_width_to_length_ratio * length);

        let tip_color = manim_visible_color(&shaft.style);
        let end_tip = triangle_tip_path(visible_end, direction, effective_tip_length)?;
        let start_tip = self
            .start_tip
            .then(|| {
                triangle_tip_path(
                    visible_start,
                    (-direction.0, -direction.1),
                    effective_tip_length,
                )
            })
            .transpose()?;

        Ok(PreparedArrow {
            shaft,
            end_tip,
            start_tip,
            tip_color,
            z_index: self.z_index,
            initial_stroke_width,
            max_stroke_width_to_length_ratio: self.max_stroke_width_to_length_ratio,
        })
    }
}

/// Typed semantic handles for one atomically-created Arrow family.
///
/// The class-specific scaling policy is retained here in shared Rust. Optional
/// language frontends may retain this opaque handle, but they never mirror the
/// policy values or recompute Arrow geometry.
#[derive(Clone, Debug)]
pub struct ManimArrow {
    family: MobjectFamily,
    shaft: Mobject,
    end_tip: Mobject,
    start_tip: Option<Mobject>,
    initial_stroke_width: f64,
    max_stroke_width_to_length_ratio: f64,
}

impl ManimArrow {
    pub fn create(
        store: Rc<RefCell<SemanticStore>>,
        options: ManimArrowOptions,
    ) -> Result<Self, AuthoringError> {
        let committed = {
            let mut store_ref = store.borrow_mut();
            let prepared = options.prepare(&mut store_ref)?;
            commit_prepared(&mut store_ref, prepared)?
        };
        Self::from_committed(store, committed)
    }

    pub fn family(&self) -> &MobjectFamily {
        &self.family
    }

    pub fn shaft(&self) -> &Mobject {
        &self.shaft
    }

    pub fn end_tip(&self) -> &Mobject {
        &self.end_tip
    }

    pub fn start_tip(&self) -> Option<&Mobject> {
        self.start_tip.as_ref()
    }

    /// Current public Arrow endpoints. Tip apexes, not the shortened shaft bases,
    /// are the observable endpoints when tips are present.
    pub fn endpoints(&self) -> Result<((f64, f64), (f64, f64)), AuthoringError> {
        self.validate_components()?;
        let shaft = self.shaft.manim_line_endpoints()?;
        let start = match self.start_tip.as_ref() {
            Some(tip) => tip.path_query()?.start()?,
            None => shaft.start,
        };
        let end = self.end_tip.path_query()?.start()?;
        Ok((start, end))
    }

    pub fn length(&self) -> Result<f64, AuthoringError> {
        let (start, end) = self.endpoints()?;
        Ok((end.0 - start.0).hypot(end.1 - start.1))
    }

    pub fn angle(&self) -> Result<f64, AuthoringError> {
        let (start, end) = self.endpoints()?;
        Ok((end.1 - start.1).atan2(end.0 - start.0))
    }

    /// Match ManimCE Arrow.scale for the straight retained Arrow family.
    ///
    /// With `scale_tips=false`, public shaft endpoints scale about the shaft
    /// center while the existing tip resources retain their current size and are
    /// only re-oriented/re-positioned. With `scale_tips=true`, all Arrow leaves
    /// scale about the aggregate family center. Both modes recap the shaft from
    /// the originally authored stroke width and publish one semantic transaction.
    pub fn scale(&self, factor: f64, scale_tips: bool) -> Result<(), AuthoringError> {
        let factor = crate::integration::authoring_render_f64("arrow scale factor", factor)?;
        self.validate_components()?;
        let (start, end) = self.endpoints()?;
        let old_length = distance(start, end);
        // Pinned ManimCE returns immediately for a zero-length Arrow.
        if old_length == 0.0 {
            return Ok(());
        }

        let (previous_shaft, previous_end_tip, previous_start_tip) = (
            self.shaft.state()?,
            self.end_tip.state()?,
            self.start_tip.as_ref().map(Mobject::state).transpose()?,
        );

        let store = self.family.integration_store();
        let (next_shaft, next_end_tip, next_start_tip) = {
            let store_ref = store.borrow();
            if scale_tips {
                self.scaled_with_tips(
                    &store_ref,
                    factor,
                    old_length,
                    &previous_shaft,
                    &previous_end_tip,
                    previous_start_tip.as_ref(),
                )?
            } else {
                self.scaled_preserving_tips(
                    factor,
                    start,
                    end,
                    &previous_shaft,
                    &previous_end_tip,
                    previous_start_tip.as_ref(),
                )?
            }
        };

        let mut transaction = SemanticMutationTransaction::new();
        crate::semantic_mobject::stage_state_changes(
            &mut transaction,
            self.shaft.node_id(),
            &previous_shaft,
            &next_shaft,
        );
        crate::semantic_mobject::stage_state_changes(
            &mut transaction,
            self.end_tip.node_id(),
            &previous_end_tip,
            &next_end_tip,
        );
        if let (Some(handle), Some(previous), Some(next)) = (
            self.start_tip.as_ref(),
            previous_start_tip.as_ref(),
            next_start_tip.as_ref(),
        ) {
            crate::semantic_mobject::stage_state_changes(
                &mut transaction,
                handle.node_id(),
                previous,
                next,
            );
        }
        transaction
            .apply(&mut store.borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    fn validate_components(&self) -> Result<(), AuthoringError> {
        self.family.validate()?;
        self.shaft.validate()?;
        self.end_tip.validate()?;
        if let Some(tip) = self.start_tip.as_ref() {
            tip.validate()?;
        }
        Ok(())
    }

    fn scaled_with_tips(
        &self,
        store: &SemanticStore,
        factor: f64,
        old_length: f64,
        previous_shaft: &SemanticObjectState,
        previous_end_tip: &SemanticObjectState,
        previous_start_tip: Option<&SemanticObjectState>,
    ) -> Result<
        (
            SemanticObjectState,
            SemanticObjectState,
            Option<SemanticObjectState>,
        ),
        AuthoringError,
    > {
        let bounds = self
            .family
            .layout_bounds()?
            .ok_or(AuthoringError::MissingLayoutBounds(self.family.node_id()))?;
        let pivot = (
            (bounds.min_x + bounds.max_x) * 0.5,
            (bounds.min_y + bounds.max_y) * 0.5,
        );

        let scale_one =
            |previous: &SemanticObjectState| -> Result<SemanticObjectState, AuthoringError> {
                let old_center = crate::semantic_mobject::state_center(store, previous)?;
                let target_center = (
                    pivot.0 + (old_center.0 - pivot.0) * factor,
                    pivot.1 + (old_center.1 - pivot.1) * factor,
                );
                let mut next = previous.clone();
                crate::semantic_mobject::scale_state_about_center(
                    store,
                    &mut next,
                    factor,
                    factor,
                    target_center,
                )?;
                Ok(next)
            };

        let mut shaft = scale_one(previous_shaft)?;
        shaft.style.stroke_width = self
            .initial_stroke_width
            .min(self.max_stroke_width_to_length_ratio * old_length * factor.abs());
        let end_tip = scale_one(previous_end_tip)?;
        let start_tip = previous_start_tip.map(scale_one).transpose()?;
        Ok((shaft, end_tip, start_tip))
    }

    fn scaled_preserving_tips(
        &self,
        factor: f64,
        start: (f64, f64),
        end: (f64, f64),
        previous_shaft: &SemanticObjectState,
        previous_end_tip: &SemanticObjectState,
        previous_start_tip: Option<&SemanticObjectState>,
    ) -> Result<
        (
            SemanticObjectState,
            SemanticObjectState,
            Option<SemanticObjectState>,
        ),
        AuthoringError,
    > {
        let pivot = midpoint(start, end);
        let new_start = scale_point_about(start, pivot, factor);
        let new_end = scale_point_about(end, pivot, factor);
        let new_length = distance(new_start, new_end);

        let shaft_endpoints = self.shaft.manim_line_endpoints()?;
        let end_tip_length = distance(end, shaft_endpoints.end);
        let start_tip_length = previous_start_tip
            .map(|_| distance(start, shaft_endpoints.start))
            .unwrap_or(0.0);

        let mut next_end_tip = previous_end_tip.clone();
        let mut next_start_tip = previous_start_tip.cloned();
        let (shaft_start, shaft_end) = if new_length == 0.0 {
            reposition_tip(
                &mut next_end_tip,
                end,
                current_tip_direction(shaft_endpoints.end, end),
                new_end,
                None,
            )?;
            if let (Some(previous), Some(next)) = (previous_start_tip, next_start_tip.as_mut()) {
                reposition_tip(
                    next,
                    start,
                    current_tip_direction(shaft_endpoints.start, start),
                    new_start,
                    None,
                )?;
                debug_assert_eq!(previous.content, next.content);
            }
            (new_start, new_end)
        } else {
            let direction = unit_direction(new_start, new_end);
            reposition_tip(
                &mut next_end_tip,
                end,
                current_tip_direction(shaft_endpoints.end, end),
                new_end,
                Some(direction),
            )?;
            let end_base = (
                new_end.0 - direction.0 * end_tip_length,
                new_end.1 - direction.1 * end_tip_length,
            );

            let start_base = if let Some(next) = next_start_tip.as_mut() {
                // Manim re-adds the end tip first. The start-tip tangent is therefore
                // based on the line from the public start to the newly shortened end.
                let tangent = unit_direction_or(new_start, end_base, direction);
                let start_direction = (-tangent.0, -tangent.1);
                reposition_tip(
                    next,
                    start,
                    current_tip_direction(shaft_endpoints.start, start),
                    new_start,
                    Some(start_direction),
                )?;
                (
                    new_start.0 + tangent.0 * start_tip_length,
                    new_start.1 + tangent.1 * start_tip_length,
                )
            } else {
                new_start
            };
            (start_base, end_base)
        };

        let mut next_shaft = previous_shaft.clone();
        set_line_world_endpoints(
            &mut next_shaft,
            shaft_start,
            shaft_end,
            "scaled arrow shaft",
        )?;
        next_shaft.style.stroke_width = self
            .initial_stroke_width
            .min(self.max_stroke_width_to_length_ratio * new_length);

        Ok((next_shaft, next_end_tip, next_start_tip))
    }

    fn from_committed(
        store: Rc<RefCell<SemanticStore>>,
        committed: CommittedArrow,
    ) -> Result<Self, AuthoringError> {
        Ok(Self {
            family: MobjectFamily::from_node(Rc::clone(&store), committed.family)?,
            shaft: Mobject::from_node(Rc::clone(&store), committed.shaft)?,
            end_tip: Mobject::from_node(Rc::clone(&store), committed.end_tip)?,
            start_tip: committed
                .start_tip
                .map(|node| Mobject::from_node(Rc::clone(&store), node))
                .transpose()?,
            initial_stroke_width: committed.initial_stroke_width,
            max_stroke_width_to_length_ratio: committed.max_stroke_width_to_length_ratio,
        })
    }
}

/// Publish several already-validated Arrow requests and one outer family in one
/// semantic transaction. This crate-private seam lets composite authoring reuse
/// the single Arrow implementation without exposing a second geometry path.
pub(crate) fn create_arrow_batch_family(
    store: Rc<RefCell<SemanticStore>>,
    options: Vec<ManimArrowOptions>,
) -> Result<(MobjectFamily, Vec<ManimArrow>), AuthoringError> {
    let (family_node, committed) = {
        let mut store_ref = store.borrow_mut();
        let prepared = options
            .into_iter()
            .map(|options| options.prepare(&mut store_ref))
            .collect::<Result<Vec<_>, _>>()?;
        let mut paths = Vec::with_capacity(
            prepared.len()
                + prepared
                    .iter()
                    .filter(|arrow| arrow.start_tip.is_some())
                    .count(),
        );
        for arrow in &prepared {
            paths.push(arrow.end_tip.clone());
            if let Some(path) = &arrow.start_tip {
                paths.push(path.clone());
            }
        }

        store_ref.with_geometry_paths(paths, |store, handles| -> Result<_, AuthoringError> {
            let mut transaction = SemanticMutationTransaction::new();
            let family = transaction.create_node(SemanticNodeCreation::family());
            let mut staged = Vec::with_capacity(prepared.len());
            let mut handle_index = 0usize;
            for arrow in &prepared {
                let end_tip_handle = handles[handle_index];
                handle_index += 1;
                let start_tip_handle = if arrow.start_tip.is_some() {
                    let handle = handles[handle_index];
                    handle_index += 1;
                    Some(handle)
                } else {
                    None
                };
                let staged_arrow =
                    stage_prepared_arrow(&mut transaction, arrow, end_tip_handle, start_tip_handle);
                transaction.add_member(family, staged_arrow.family);
                staged.push(staged_arrow);
            }
            debug_assert_eq!(handle_index, handles.len());

            let result = transaction.apply(store).map_err(AuthoringError::from)?;
            let family_node = result
                .resolve(family)
                .ok_or(AuthoringError::UnresolvedCreatedNode(family))?;
            let committed = staged
                .into_iter()
                .map(|arrow| resolve_staged_arrow(arrow, |token| result.resolve(token)))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((family_node, committed))
        })?
    };

    let family = MobjectFamily::from_node(Rc::clone(&store), family_node)?;
    let arrows = committed
        .into_iter()
        .map(|arrow| ManimArrow::from_committed(Rc::clone(&store), arrow))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((family, arrows))
}

struct PreparedArrow {
    shaft: SemanticObjectState,
    end_tip: VectorPath,
    start_tip: Option<VectorPath>,
    tip_color: Color,
    z_index: f64,
    initial_stroke_width: f64,
    max_stroke_width_to_length_ratio: f64,
}

struct StagedArrow {
    family: noon_core::SemanticLocalNodeToken,
    shaft: noon_core::SemanticLocalNodeToken,
    end_tip: noon_core::SemanticLocalNodeToken,
    start_tip: Option<noon_core::SemanticLocalNodeToken>,
    initial_stroke_width: f64,
    max_stroke_width_to_length_ratio: f64,
}

struct CommittedArrow {
    family: noon_core::SemanticNodeId,
    shaft: noon_core::SemanticNodeId,
    end_tip: noon_core::SemanticNodeId,
    start_tip: Option<noon_core::SemanticNodeId>,
    initial_stroke_width: f64,
    max_stroke_width_to_length_ratio: f64,
}

struct ShortenedLine {
    visible_start: (f64, f64),
    visible_end: (f64, f64),
    direction: (f64, f64),
    length: f64,
}

fn commit_prepared(
    store: &mut SemanticStore,
    prepared: PreparedArrow,
) -> Result<CommittedArrow, AuthoringError> {
    let end_path = prepared.end_tip.clone();
    store.with_geometry_path(end_path, |store, end_tip_handle| {
        if let Some(start_path) = prepared.start_tip.clone() {
            store.with_geometry_path(start_path, |store, start_tip_handle| {
                commit_transaction(store, &prepared, end_tip_handle, Some(start_tip_handle))
            })
        } else {
            commit_transaction(store, &prepared, end_tip_handle, None)
        }
    })
}

fn commit_transaction(
    store: &mut SemanticStore,
    prepared: &PreparedArrow,
    end_tip_handle: noon_core::GeometryResourceHandle,
    start_tip_handle: Option<noon_core::GeometryResourceHandle>,
) -> Result<CommittedArrow, AuthoringError> {
    let mut transaction = SemanticMutationTransaction::new();
    let staged = stage_prepared_arrow(&mut transaction, prepared, end_tip_handle, start_tip_handle);
    let result = transaction.apply(store).map_err(AuthoringError::from)?;
    resolve_staged_arrow(staged, |token| result.resolve(token))
}

fn stage_prepared_arrow(
    transaction: &mut SemanticMutationTransaction,
    prepared: &PreparedArrow,
    end_tip_handle: noon_core::GeometryResourceHandle,
    start_tip_handle: Option<noon_core::GeometryResourceHandle>,
) -> StagedArrow {
    let shaft = transaction.create_node(SemanticNodeCreation::object(prepared.shaft.clone()));
    let end_tip = transaction.create_node(SemanticNodeCreation::object(tip_state(
        prepared,
        end_tip_handle,
    )));
    let start_tip = start_tip_handle.map(|handle| {
        transaction.create_node(SemanticNodeCreation::object(tip_state(prepared, handle)))
    });
    let family = transaction.create_node(SemanticNodeCreation::family());
    if prepared.z_index != 0.0 {
        transaction.set_z_index(family, prepared.z_index);
    }
    transaction.add_member(family, shaft);
    transaction.add_member(family, end_tip);
    if let Some(start_tip) = start_tip {
        transaction.add_member(family, start_tip);
    }
    StagedArrow {
        family,
        shaft,
        end_tip,
        start_tip,
        initial_stroke_width: prepared.initial_stroke_width,
        max_stroke_width_to_length_ratio: prepared.max_stroke_width_to_length_ratio,
    }
}

fn resolve_staged_arrow(
    staged: StagedArrow,
    mut resolve: impl FnMut(noon_core::SemanticLocalNodeToken) -> Option<noon_core::SemanticNodeId>,
) -> Result<CommittedArrow, AuthoringError> {
    Ok(CommittedArrow {
        family: resolve(staged.family)
            .ok_or(AuthoringError::UnresolvedCreatedNode(staged.family))?,
        shaft: resolve(staged.shaft).ok_or(AuthoringError::UnresolvedCreatedNode(staged.shaft))?,
        end_tip: resolve(staged.end_tip)
            .ok_or(AuthoringError::UnresolvedCreatedNode(staged.end_tip))?,
        start_tip: staged
            .start_tip
            .map(|token| resolve(token).ok_or(AuthoringError::UnresolvedCreatedNode(token)))
            .transpose()?,
        initial_stroke_width: staged.initial_stroke_width,
        max_stroke_width_to_length_ratio: staged.max_stroke_width_to_length_ratio,
    })
}

fn tip_state(
    prepared: &PreparedArrow,
    handle: noon_core::GeometryResourceHandle,
) -> SemanticObjectState {
    let mut state = SemanticObjectState::new(StoredGeometry::Resource(handle));
    state.transform = prepared.shaft.transform;
    state.style = prepared.shaft.style.clone();
    state.style.fill = Some(SemanticPaint::Solid(prepared.tip_color));
    state.style.fill_opacity = 1.0;
    state.style.stroke = Some(SemanticPaint::Solid(prepared.tip_color));
    state.style.stroke_width = 0.0;
    state.style.stroke_opacity = 1.0;
    state.set_z_index(prepared.z_index);
    state
}

fn shortened_line(
    start: (f64, f64),
    end: (f64, f64),
    buff: f64,
) -> Result<ShortenedLine, AuthoringError> {
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let length = dx.hypot(dy);
    let direction = if length == 0.0 {
        (1.0, 0.0)
    } else {
        (dx / length, dy / length)
    };
    let (visible_start, visible_end) = if buff > 0.0 && length >= 2.0 * buff && length > 0.0 {
        (
            (start.0 + direction.0 * buff, start.1 + direction.1 * buff),
            (end.0 - direction.0 * buff, end.1 - direction.1 * buff),
        )
    } else {
        (start, end)
    };
    let visible_length = (visible_end.0 - visible_start.0).hypot(visible_end.1 - visible_start.1);
    for (name, value) in [
        ("arrow visible start x", visible_start.0),
        ("arrow visible start y", visible_start.1),
        ("arrow visible end x", visible_end.0),
        ("arrow visible end y", visible_end.1),
        ("arrow visible length", visible_length),
    ] {
        crate::integration::authoring_render_f64(name, value)?;
    }
    Ok(ShortenedLine {
        visible_start,
        visible_end,
        direction,
        length: visible_length,
    })
}

fn triangle_tip_path(
    apex: (f64, f64),
    direction: (f64, f64),
    length: f64,
) -> Result<VectorPath, AuthoringError> {
    let base = (apex.0 - direction.0 * length, apex.1 - direction.1 * length);
    let half_width = length * 0.5;
    let perpendicular = (-direction.1, direction.0);
    let first_base = (
        base.0 + perpendicular.0 * half_width,
        base.1 + perpendicular.1 * half_width,
    );
    let second_base = (
        base.0 - perpendicular.0 * half_width,
        base.1 - perpendicular.1 * half_width,
    );
    Ok(VectorPath::new()
        .move_to(lower_point("arrow tip apex", apex)?)
        .line_to(lower_point("arrow tip base 1", first_base)?)
        .line_to(lower_point("arrow tip base 2", second_base)?)
        .close())
}

fn manim_visible_color(style: &noon_core::SemanticStyle) -> Color {
    let fill_visible = matches!(
        style.fill.as_ref(),
        Some(SemanticPaint::Solid(color))
            if f64::from(color.alpha) * style.fill_opacity > 0.0
    );
    let paint = if fill_visible {
        style.fill.as_ref()
    } else {
        style.stroke.as_ref()
    };
    match paint {
        Some(SemanticPaint::Solid(color)) => Color::rgb(color.red, color.green, color.blue),
        _ => Color::WHITE,
    }
}

fn checked_point(name: &str, x: f64, y: f64) -> Result<(f64, f64), AuthoringError> {
    crate::integration::authoring_render_f64(&format!("{name}.x"), x)?;
    crate::integration::authoring_render_f64(&format!("{name}.y"), y)?;
    Ok((x, y))
}

fn lower_point(name: &str, point: (f64, f64)) -> Result<Vec2, AuthoringError> {
    crate::semantic_mobject::authoring_xy_f64(point.0, point.1)
        .and_then(|value| value.lower_xy_f32().map_err(AuthoringError::from))
        .map_err(|error| match error {
            AuthoringError::VectorLowering(_) => AuthoringError::InvalidRenderNumber {
                name: name.to_owned(),
                value: point.0.abs().max(point.1.abs()),
            },
            other => other,
        })
}

fn nonnegative(name: &str, value: f64) -> Result<f64, AuthoringError> {
    let value = crate::integration::authoring_render_f64(name, value)?;
    if value < 0.0 {
        return Err(AuthoringError::NegativeStrokeWidth(value));
    }
    Ok(value)
}

fn distance(left: (f64, f64), right: (f64, f64)) -> f64 {
    (right.0 - left.0).hypot(right.1 - left.1)
}

fn midpoint(left: (f64, f64), right: (f64, f64)) -> (f64, f64) {
    ((left.0 + right.0) * 0.5, (left.1 + right.1) * 0.5)
}

fn scale_point_about(point: (f64, f64), pivot: (f64, f64), factor: f64) -> (f64, f64) {
    (
        pivot.0 + (point.0 - pivot.0) * factor,
        pivot.1 + (point.1 - pivot.1) * factor,
    )
}

fn unit_direction(start: (f64, f64), end: (f64, f64)) -> (f64, f64) {
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let length = dx.hypot(dy);
    debug_assert!(length > 0.0);
    (dx / length, dy / length)
}

fn unit_direction_or(start: (f64, f64), end: (f64, f64), fallback: (f64, f64)) -> (f64, f64) {
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let length = dx.hypot(dy);
    if length == 0.0 {
        fallback
    } else {
        (dx / length, dy / length)
    }
}

fn current_tip_direction(base: (f64, f64), apex: (f64, f64)) -> Option<(f64, f64)> {
    let dx = apex.0 - base.0;
    let dy = apex.1 - base.1;
    let length = dx.hypot(dy);
    (length > 0.0).then_some((dx / length, dy / length))
}

fn reposition_tip(
    state: &mut SemanticObjectState,
    old_apex: (f64, f64),
    old_direction: Option<(f64, f64)>,
    new_apex: (f64, f64),
    new_direction: Option<(f64, f64)>,
) -> Result<(), AuthoringError> {
    if let (Some(old), Some(new)) = (old_direction, new_direction) {
        let angle = new.1.atan2(new.0) - old.1.atan2(old.0);
        let ((translation_x, translation_y), rotation) =
            crate::semantic_mobject::rotate_affine_about_point(
                (state.transform.translation.x, state.transform.translation.y),
                state.transform.rotation_z,
                angle,
                old_apex,
            )?;
        state.transform.translation.x = translation_x;
        state.transform.translation.y = translation_y;
        state.transform.rotation_z = rotation;
    }
    state.transform.translation.x += new_apex.0 - old_apex.0;
    state.transform.translation.y += new_apex.1 - old_apex.1;
    state
        .transform
        .translation
        .lower_xy_f32()
        .map_err(AuthoringError::from)?;
    Ok(())
}

fn set_line_world_endpoints(
    state: &mut SemanticObjectState,
    start: (f64, f64),
    end: (f64, f64),
    name: &str,
) -> Result<(), AuthoringError> {
    let start = inverse_transform_point(state.transform, start, name)?;
    let end = inverse_transform_point(state.transform, end, name)?;
    state.content = StoredGeometry::Line {
        start: lower_point(&format!("{name} start"), start)?,
        end: lower_point(&format!("{name} end"), end)?,
    }
    .into();
    Ok(())
}

fn inverse_transform_point(
    transform: noon_core::SemanticTransform2_5D,
    point: (f64, f64),
    name: &str,
) -> Result<(f64, f64), AuthoringError> {
    let translated_x = point.0 - transform.translation.x;
    let translated_y = point.1 - transform.translation.y;
    let (sin, cos) = transform.rotation_z.sin_cos();
    let rotated_x = translated_x * cos + translated_y * sin;
    let rotated_y = -translated_x * sin + translated_y * cos;
    checked_point(
        name,
        if transform.scale.x == 0.0 {
            0.0
        } else {
            rotated_x / transform.scale.x
        },
        if transform.scale.y == 0.0 {
            0.0
        } else {
            rotated_y / transform.scale.y
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Scene;
    use noon_core::SemanticObjectContent;

    fn line_endpoints(state: &SemanticObjectState) -> (Vec2, Vec2) {
        let SemanticObjectContent::Geometry(StoredGeometry::Line { start, end }) = state.content
        else {
            panic!("arrow shaft must stay an analytic Line");
        };
        (start, end)
    }

    fn resource_handle(state: &SemanticObjectState) -> noon_core::GeometryResourceHandle {
        let SemanticObjectContent::Geometry(StoredGeometry::Resource(handle)) = state.content
        else {
            panic!("arrow tip must remain a retained geometry resource");
        };
        handle
    }

    #[test]
    fn arrow_uses_manim_buff_tip_and_stroke_caps() {
        let scene = Scene::new();
        let arrow = ManimArrow::create(
            Rc::clone(scene.integration_store()),
            ManimArrowOptions::arrow(-1.0, 0.0, 1.0, 0.0).unwrap(),
        )
        .unwrap();
        let (start, end) = line_endpoints(&arrow.shaft().state().unwrap());
        assert!((start.x + 0.75).abs() < 1e-6);
        assert!((end.x - 0.40).abs() < 1e-6);
        assert!((arrow.shaft().state().unwrap().style.stroke_width - 0.06).abs() < 1e-12);
        assert_eq!(
            arrow
                .family()
                .integration_store()
                .borrow()
                .semantic_family_members_checked(arrow.family().node_id())
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn short_arrow_caps_tip_and_stroke_from_post_buff_length() {
        let scene = Scene::new();
        let mut options = ManimArrowOptions::arrow(0.0, 0.0, 0.4, 0.0).unwrap();
        options.set_buff(0.0).unwrap();
        let arrow = ManimArrow::create(Rc::clone(scene.integration_store()), options).unwrap();
        let (_, end) = line_endpoints(&arrow.shaft().state().unwrap());
        assert!((end.x - 0.3).abs() < 1e-6);
        assert!((arrow.shaft().state().unwrap().style.stroke_width - 0.02).abs() < 1e-12);
    }

    #[test]
    fn oversized_buff_matches_manim_and_leaves_line_untrimmed() {
        let scene = Scene::new();
        let mut options = ManimArrowOptions::arrow(-0.2, 0.0, 0.2, 0.0).unwrap();
        options.set_buff(0.25).unwrap();
        let arrow = ManimArrow::create(Rc::clone(scene.integration_store()), options).unwrap();
        let (start, _) = line_endpoints(&arrow.shaft().state().unwrap());
        assert!((start.x + 0.2).abs() < 1e-6);
    }

    #[test]
    fn vector_starts_at_origin_without_arrow_default_buff() {
        let scene = Scene::new();
        let vector = ManimArrow::create(
            Rc::clone(scene.integration_store()),
            ManimArrowOptions::vector(2.0, 1.0).unwrap(),
        )
        .unwrap();
        let (start, _) = line_endpoints(&vector.shaft().state().unwrap());
        assert_eq!(start, Vec2::ZERO);
    }

    #[test]
    fn double_arrow_publishes_both_tips_in_source_order() {
        let scene = Scene::new();
        let arrow = ManimArrow::create(
            Rc::clone(scene.integration_store()),
            ManimArrowOptions::double_arrow(-2.0, 0.0, 2.0, 0.0).unwrap(),
        )
        .unwrap();
        assert!(arrow.start_tip().is_some());
        let members = scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(arrow.family().node_id())
            .unwrap();
        assert_eq!(
            members,
            vec![
                arrow.shaft().node_id(),
                arrow.end_tip().node_id(),
                arrow.start_tip().unwrap().node_id()
            ]
        );
    }

    #[test]
    fn zero_length_arrow_is_valid_and_degenerate_like_manim() {
        let scene = Scene::new();
        let arrow = ManimArrow::create(
            Rc::clone(scene.integration_store()),
            ManimArrowOptions::arrow(1.0, 2.0, 1.0, 2.0).unwrap(),
        )
        .unwrap();
        let (start, end) = line_endpoints(&arrow.shaft().state().unwrap());
        assert_eq!(start, end);
        assert_eq!(arrow.shaft().state().unwrap().style.stroke_width, 0.0);
    }

    #[test]
    fn scale_preserves_tip_resource_and_recovers_authored_stroke_cap() {
        let scene = Scene::new();
        let mut options = ManimArrowOptions::arrow(0.0, 0.0, 0.4, 0.0).unwrap();
        options.set_buff(0.0).unwrap();
        let arrow = ManimArrow::create(Rc::clone(scene.integration_store()), options).unwrap();
        let tip_resource = resource_handle(&arrow.end_tip().state().unwrap());
        let before_revision = scene.integration_store().borrow().scene_revision();

        arrow.scale(0.5, false).unwrap();
        let scaled_length = arrow.length().unwrap();
        assert!((scaled_length - 0.2).abs() < 1e-6);
        assert_eq!(
            resource_handle(&arrow.end_tip().state().unwrap()),
            tip_resource
        );
        assert!(
            (arrow.shaft().state().unwrap().style.stroke_width
                - DEFAULT_ARROW_STROKE_WIDTH_RATIO * scaled_length)
                .abs()
                < 1e-12
        );
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            before_revision.checked_next().unwrap()
        );

        arrow.scale(10.0, false).unwrap();
        assert!((arrow.length().unwrap() - 2.0).abs() < 1e-5);
        assert_eq!(
            resource_handle(&arrow.end_tip().state().unwrap()),
            tip_resource
        );
        assert!((arrow.shaft().state().unwrap().style.stroke_width - 0.06).abs() < 1e-12);
    }

    #[test]
    fn scale_tips_true_scales_tip_with_family_in_one_publication() {
        let scene = Scene::new();
        let mut options = ManimArrowOptions::arrow(-1.0, 0.0, 1.0, 0.0).unwrap();
        options.set_buff(0.0).unwrap();
        let arrow = ManimArrow::create(Rc::clone(scene.integration_store()), options).unwrap();
        let tip_resource = resource_handle(&arrow.end_tip().state().unwrap());
        let tip_scale = arrow.end_tip().state().unwrap().transform.scale;
        let before_revision = scene.integration_store().borrow().scene_revision();

        arrow.scale(2.0, true).unwrap();

        let after = arrow.end_tip().state().unwrap();
        assert_eq!(resource_handle(&after), tip_resource);
        assert!((after.transform.scale.x - tip_scale.x * 2.0).abs() < 1e-12);
        assert!((after.transform.scale.y - tip_scale.y * 2.0).abs() < 1e-12);
        assert!((arrow.length().unwrap() - 4.0).abs() < 1e-5);
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            before_revision.checked_next().unwrap()
        );
    }

    #[test]
    fn negative_scale_preserves_tip_size_and_reverses_public_direction() {
        let scene = Scene::new();
        let mut options = ManimArrowOptions::arrow(-1.0, 0.0, 1.0, 0.0).unwrap();
        options.set_buff(0.0).unwrap();
        let arrow = ManimArrow::create(Rc::clone(scene.integration_store()), options).unwrap();
        let before_tip_scale = arrow.end_tip().state().unwrap().transform.scale;

        arrow.scale(-1.0, false).unwrap();

        let (start, end) = arrow.endpoints().unwrap();
        assert!((start.0 - 1.0).abs() < 1e-5);
        assert!((end.0 + 1.0).abs() < 1e-5);
        assert_eq!(
            arrow.end_tip().state().unwrap().transform.scale,
            before_tip_scale
        );
        assert!((arrow.angle().unwrap() - std::f64::consts::PI).abs() < 1e-5);
    }

    #[test]
    fn zero_length_scale_is_a_noop_like_manim() {
        let scene = Scene::new();
        let arrow = ManimArrow::create(
            Rc::clone(scene.integration_store()),
            ManimArrowOptions::arrow(1.0, 2.0, 1.0, 2.0).unwrap(),
        )
        .unwrap();
        let before_revision = scene.integration_store().borrow().scene_revision();
        let before_shaft = arrow.shaft().state().unwrap();

        arrow.scale(3.0, false).unwrap();

        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            before_revision
        );
        assert_eq!(arrow.shaft().state().unwrap(), before_shaft);
    }

    #[test]
    fn invalid_option_rejects_before_identity_or_resource_publication() {
        let scene = Scene::new();
        let before_revision = scene.integration_store().borrow().scene_revision();
        let before_resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats();
        let mut options = ManimArrowOptions::arrow(-1.0, 0.0, 1.0, 0.0).unwrap();
        assert!(options.set_tip_length(f64::NAN).is_err());
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            before_revision
        );
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .stats(),
            before_resources
        );
    }
}
