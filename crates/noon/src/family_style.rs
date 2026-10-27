//! Family paint uses the same style edits and atomic semantic transaction as leaves.
use crate::AuthoringError;
use crate::{
    semantic_mobject::{
        edit_background_stroke_color, edit_background_stroke_opacity, edit_background_stroke_width,
        edit_color, edit_fade, edit_fade_occurrences, edit_fade_to_occurrences, edit_fill_color,
        edit_fill_opacity, edit_manim_opacity, edit_object_opacity, edit_stroke_color,
        edit_stroke_opacity, edit_stroke_width, fade_factor, interpolate_manim_color,
        interpolate_manim_scalar, interpolate_manim_vmobject_style_values, PaintStyleEdit,
    },
    Color, Mobject, MobjectFamily,
};
use noon_core::{SemanticMutationTransaction, SemanticStyle};

/// Optional common paint fields, applied together through one semantic mutation.
/// Omission preserves a field; disabling paint remains an explicit Rust operation.
#[derive(Clone, Copy, Debug, Default)]
pub struct StyleUpdate {
    pub fill_color: Option<Color>,
    pub fill_opacity: Option<f64>,
    pub stroke_color: Option<Color>,
    pub stroke_width: Option<f64>,
    pub stroke_opacity: Option<f64>,
    pub background_stroke_color: Option<Color>,
    pub background_stroke_width: Option<f64>,
    pub background_stroke_opacity: Option<f64>,
}

impl StyleUpdate {
    pub(crate) fn apply<S: PaintStyleEdit>(&self, style: &mut S) -> Result<(), AuthoringError> {
        fill(style, self.fill_color, self.fill_opacity)?;
        stroke(
            style,
            self.stroke_color,
            self.stroke_width,
            self.stroke_opacity,
        )?;
        background_stroke(
            style,
            self.background_stroke_color,
            self.background_stroke_width,
            self.background_stroke_opacity,
        )
    }
}

/// Manim paint matching excludes object opacity, cap/join, and width mode.
pub(crate) fn match_paint(source: &mut SemanticStyle, target: &SemanticStyle) {
    source.fill = target.fill.clone();
    source.fill_opacity = target.fill_opacity;
    source.stroke = target.stroke.clone();
    source.stroke_opacity = target.stroke_opacity;
    source.stroke_width = target.stroke_width;
    source.background_stroke = target.background_stroke.clone();
    source.background_stroke_opacity = target.background_stroke_opacity;
    source.background_stroke_width = target.background_stroke_width;
}

impl Mobject {
    /// Atomically update supplied paint fields, preserving identity and content.
    pub fn set_style(&mut self, update: StyleUpdate) -> Result<(), AuthoringError> {
        let mut state = self.state()?;
        update.apply(&mut state.style)?;
        self.commit_state(state)
    }

    /// Match authored paint; use LiveSession to capture current effective paint.
    pub fn match_style(&self, target: &Self) -> Result<(), AuthoringError> {
        self.require_same_store(target)?;
        let mut state = self.state()?;
        match_paint(&mut state.style, &target.state()?.style);
        self.clone().commit_state(state)
    }
}

pub(crate) fn fill<S: PaintStyleEdit>(
    style: &mut S,
    color: Option<Color>,
    opacity: Option<f64>,
) -> Result<(), AuthoringError> {
    if let Some(c) = color {
        edit_fill_color(
            style,
            c.red.into(),
            c.green.into(),
            c.blue.into(),
            c.alpha.into(),
        )?;
    }
    if let Some(opacity) = opacity {
        edit_fill_opacity(style, opacity)?;
    }
    Ok(())
}

pub(crate) fn stroke<S: PaintStyleEdit>(
    style: &mut S,
    color: Option<Color>,
    width: Option<f64>,
    opacity: Option<f64>,
) -> Result<(), AuthoringError> {
    if let Some(c) = color {
        edit_stroke_color(
            style,
            c.red.into(),
            c.green.into(),
            c.blue.into(),
            c.alpha.into(),
        )?;
    }
    if let Some(width) = width {
        edit_stroke_width(style, width)?;
    }
    if let Some(opacity) = opacity {
        edit_stroke_opacity(style, opacity)?;
    }
    Ok(())
}

pub(crate) fn background_stroke<S: PaintStyleEdit>(
    style: &mut S,
    color: Option<Color>,
    width: Option<f64>,
    opacity: Option<f64>,
) -> Result<(), AuthoringError> {
    if let Some(color) = color {
        edit_background_stroke_color(
            style,
            color.red.into(),
            color.green.into(),
            color.blue.into(),
            color.alpha.into(),
        )?;
    }
    if let Some(width) = width {
        edit_background_stroke_width(style, width)?;
    }
    if let Some(opacity) = opacity {
        edit_background_stroke_opacity(style, opacity)?;
    }
    Ok(())
}

impl MobjectFamily {
    /// Read this non-rendered family root's independent VMobject paint style.
    pub fn own_style(&self) -> Result<SemanticStyle, AuthoringError> {
        self.validate()?;
        Ok(self
            .integration_store()
            .borrow()
            .node(self.node_id())
            .expect("validated family")
            .family_style()
            .expect("family style")
            .clone())
    }

    pub fn own_stroke_cap(&self) -> Result<noon_core::StrokeCap, AuthoringError> {
        Ok(self.own_style()?.stroke_cap)
    }

    pub fn own_background_stroke_opacity(&self) -> Result<f64, AuthoringError> {
        self.validate()?;
        let store = self.integration_store().borrow();
        Ok(store
            .node(self.node_id())
            .expect("validated family")
            .family_background_stroke_opacity()
            .expect("family background stroke opacity"))
    }

    pub fn own_background_stroke_color(&self) -> Result<Color, AuthoringError> {
        self.validate()?;
        let store = self.integration_store().borrow();
        Ok(store
            .node(self.node_id())
            .expect("validated family")
            .family_background_stroke_color()
            .expect("family background stroke color"))
    }

    pub fn own_background_stroke_width(&self) -> Result<f64, AuthoringError> {
        self.validate()?;
        let store = self.integration_store().borrow();
        Ok(store
            .node(self.node_id())
            .expect("validated family")
            .family_background_stroke_width()
            .expect("family background stroke width"))
    }

    /// Interpolate a VGroup root's VMobject style from two family roots.
    /// Family members remain unchanged, matching VMobject.interpolate_color(),
    /// which interpolates only this object's style arrays.
    pub fn interpolate_manim_style(
        &self,
        start: &Self,
        end: &Self,
        alpha: f64,
    ) -> Result<(), AuthoringError> {
        self.interpolate_manim_style_transaction(start, end, alpha)?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    pub(crate) fn interpolate_manim_style_transaction(
        &self,
        start: &Self,
        end: &Self,
        alpha: f64,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        self.validate()?;
        start.validate()?;
        end.validate()?;
        if !std::rc::Rc::ptr_eq(self.integration_store(), start.integration_store())
            || !std::rc::Rc::ptr_eq(self.integration_store(), end.integration_store())
        {
            return Err(AuthoringError::ForeignStore);
        }

        let receiver_style = self.own_style()?;
        let start_style = start.own_style()?;
        let end_style = end.own_style()?;
        let (style, direction) = interpolate_manim_vmobject_style_values(
            &receiver_style,
            &start_style,
            &end_style,
            start.own_sheen_direction()?,
            end.own_sheen_direction()?,
            alpha,
        )?;
        let background_color = interpolate_manim_color(
            start.own_background_stroke_color()?,
            end.own_background_stroke_color()?,
            alpha,
        )?;
        let background_width = interpolate_manim_scalar(
            start.own_background_stroke_width()?,
            end.own_background_stroke_width()?,
            alpha,
        )?;
        let background_opacity = interpolate_manim_scalar(
            start.own_background_stroke_opacity()?,
            end.own_background_stroke_opacity()?,
            alpha,
        )?;

        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .set_family_style_with_background_stroke_update(
                self.node_id(),
                style,
                Some(background_color),
                Some(background_width),
                Some(background_opacity),
            )
            .set_sheen_direction(self.node_id(), direction);
        Ok(transaction)
    }

    pub fn set_own_background_stroke_opacity(&self, opacity: f64) -> Result<(), AuthoringError> {
        self.own_background_stroke_opacity_transaction(opacity)?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    pub(crate) fn own_background_stroke_opacity_transaction(
        &self,
        opacity: f64,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        self.own_style_with_background_stroke_opacity_transaction(
            StyleUpdate::default(),
            None,
            None,
            Some(opacity),
        )
    }

    /// Fade only a VMobject family's non-rendered root paint. Its default
    /// background stroke has zero width, so the opacity is observable but
    /// has no raster effect.
    pub fn fade_own(&self, darkness: f64) -> Result<(), AuthoringError> {
        self.fade_own_transaction(darkness)?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    pub(crate) fn fade_own_transaction(
        &self,
        darkness: f64,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        let mut style = self.own_style()?;
        edit_fade(&mut style, darkness)?;
        let background_opacity = self.own_background_stroke_opacity()? * (1.0 - darkness);
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_family_style_with_background_stroke_opacity(
            self.node_id(),
            style,
            background_opacity,
        );
        Ok(transaction)
    }

    pub fn set_own_stroke_cap(&self, cap: &str) -> Result<(), AuthoringError> {
        let mut style = self.own_style()?;
        style.stroke_cap = crate::semantic_mobject::parse_stroke_cap(cap)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_family_style(self.node_id(), style);
        transaction
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    /// Atomically update the non-rendered family root without touching members.
    pub fn set_own_style(&self, update: StyleUpdate) -> Result<(), AuthoringError> {
        self.own_style_transaction(update)?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    pub(crate) fn own_style_transaction(
        &self,
        update: StyleUpdate,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        let mut style = self.own_style()?;
        update.apply(&mut style)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_family_style(self.node_id(), style);
        Ok(transaction)
    }

    /// Atomically update a family's own foreground style and default-width
    /// background-stroke opacity.
    pub fn set_own_style_with_background_stroke_opacity(
        &self,
        update: StyleUpdate,
        background_stroke_opacity: f64,
    ) -> Result<(), AuthoringError> {
        self.set_own_style_with_background_stroke(
            update,
            None,
            None,
            Some(background_stroke_opacity),
        )
    }

    /// Atomically update foreground and selected background-stroke fields on a
    /// non-rendered family root.
    pub fn set_own_style_with_background_stroke(
        &self,
        update: StyleUpdate,
        background_stroke_color: Option<Color>,
        background_stroke_width: Option<f64>,
        background_stroke_opacity: Option<f64>,
    ) -> Result<(), AuthoringError> {
        self.own_style_with_background_stroke_opacity_transaction(
            update,
            background_stroke_color,
            background_stroke_width,
            background_stroke_opacity,
        )?
        .apply(&mut self.integration_store().borrow_mut())
        .map(|_| ())
        .map_err(AuthoringError::from)
    }

    pub(crate) fn own_style_with_background_stroke_opacity_transaction(
        &self,
        update: StyleUpdate,
        background_stroke_color: Option<Color>,
        background_stroke_width: Option<f64>,
        background_stroke_opacity: Option<f64>,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        let mut style = self.own_style()?;
        update.apply(&mut style)?;
        let mut validation = noon_core::SemanticStyle::default();
        if let Some(color) = background_stroke_color {
            edit_stroke_color(
                &mut validation,
                color.red.into(),
                color.green.into(),
                color.blue.into(),
                color.alpha.into(),
            )?;
        }
        if let Some(width) = background_stroke_width {
            edit_stroke_width(&mut validation, width)?;
        }
        if let Some(opacity) = background_stroke_opacity {
            edit_stroke_opacity(&mut validation, opacity)?;
        }
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_family_style_with_background_stroke_update(
            self.node_id(),
            style,
            background_stroke_color,
            background_stroke_width,
            background_stroke_opacity,
        );
        Ok(transaction)
    }

    /// Match Manim VMobject.set_opacity() for this root's foreground and
    /// default-width background-stroke opacity.
    pub fn set_own_opacity(&self, opacity: f64) -> Result<(), AuthoringError> {
        self.own_opacity_transaction(opacity)?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    /// Multiply the final composed family image once. Child paint and object
    /// opacities remain independent, including when this family has no effects.
    pub fn set_object_opacity(&self, opacity: f64) -> Result<(), AuthoringError> {
        self.object_opacity_transaction(opacity)?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    pub(crate) fn object_opacity_transaction(
        &self,
        opacity: f64,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        let mut style = self.own_style()?;
        edit_object_opacity(&mut style, opacity)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_family_style_with_explicit_object_opacity(self.node_id(), style);
        Ok(transaction)
    }

    pub(crate) fn own_opacity_transaction(
        &self,
        opacity: f64,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        let mut style = self.own_style()?;
        StyleUpdate {
            fill_opacity: Some(opacity),
            stroke_opacity: Some(opacity),
            ..StyleUpdate::default()
        }
        .apply(&mut style)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_family_style_with_background_stroke_opacity(self.node_id(), style, opacity);
        Ok(transaction)
    }

    /// Atomically recolor selected leaves by current authoritative family order.
    /// `None` preserves a leaf. The exact length is validated before staging.
    pub fn set_member_colors(&self, colors: &[Option<Color>]) -> Result<(), AuthoringError> {
        self.validate()?;
        let expected = self
            .integration_store()
            .borrow()
            .ordered_leaf_nodes(self.node_id())
            .map_err(AuthoringError::from)?
            .len();
        if colors.len() != expected {
            return Err(AuthoringError::FamilyMemberValueCount {
                expected,
                actual: colors.len(),
            });
        }
        self.style_transaction_indexed(|index, _, style| {
            if let Some(Some(color)) = colors.get(index) {
                edit_color(
                    style,
                    color.red.into(),
                    color.green.into(),
                    color.blue.into(),
                    color.alpha.into(),
                )?;
            }
            Ok(())
        })?
        .apply(&mut self.integration_store().borrow_mut())
        .map(|_| ())
        .map_err(AuthoringError::from)
    }

    /// Atomically update supplied paint fields on every family root and leaf.
    pub fn set_style(&self, update: StyleUpdate) -> Result<(), AuthoringError> {
        self.style_update_transaction(update)?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    pub(crate) fn style_update_transaction(
        &self,
        update: StyleUpdate,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        update.apply(&mut SemanticStyle::default())?;
        self.style_update_transaction_with(|_, _| update)
    }

    pub(crate) fn opacity_transaction(
        &self,
        opacity: f64,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        edit_manim_opacity(&mut SemanticStyle::default(), opacity)?;
        self.style_update_transaction_with(|style, is_family| StyleUpdate {
            fill_opacity: style.fill.is_some().then_some(opacity),
            stroke_opacity: style.stroke.is_some().then_some(opacity),
            // Non-rendered VMobject roots always expose this channel, even
            // when its default width is zero. Disabled leaf paint stays disabled.
            background_stroke_opacity: (is_family || style.background_stroke.is_some())
                .then_some(opacity),
            ..StyleUpdate::default()
        })
    }

    fn style_update_transaction_with(
        &self,
        update_for: impl Fn(&SemanticStyle, bool) -> StyleUpdate,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        self.validate()?;
        let store = self.integration_store().borrow();
        let nodes = store
            .ordered_authoring_nodes(self.node_id())
            .map_err(AuthoringError::from)?;
        let mut transaction = SemanticMutationTransaction::new();
        for node in nodes {
            if let Some(previous) = store.node(node).and_then(|entry| entry.family_style()) {
                let update = update_for(previous, true);
                let mut style = previous.clone();
                update.apply(&mut style)?;
                transaction.set_family_style_with_background_stroke_update(
                    node,
                    style,
                    update.background_stroke_color,
                    update.background_stroke_width,
                    update.background_stroke_opacity,
                );
            } else {
                let previous = &store
                    .semantic_object_state_checked(node)
                    .map_err(AuthoringError::from)?
                    .style;
                let update = update_for(previous, false);
                let mut style = previous.clone();
                update.apply(&mut style)?;
                if style != *previous {
                    transaction.replace_style(node, style);
                }
            }
        }
        Ok(transaction)
    }

    /// Match root and leaf paint through the shared topology/alias pairing contract.
    /// Unequal topology is rejected before any style changes are published.
    pub fn match_style(&self, target: &Self) -> Result<(), AuthoringError> {
        self.match_style_transaction(target, |object| object.state().map(|state| state.style))?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    pub(crate) fn match_style_transaction<E: From<AuthoringError>>(
        &self,
        target: &Self,
        mut capture: impl FnMut(&Mobject) -> Result<SemanticStyle, E>,
    ) -> Result<SemanticMutationTransaction, E> {
        use std::{collections::HashMap, rc::Rc};
        if !Rc::ptr_eq(self.integration_store(), target.integration_store()) {
            return Err(AuthoringError::ForeignStore.into());
        }
        self.validate()?;
        target.validate()?;
        let target_is_empty = self
            .integration_store()
            .borrow()
            .node(target.node_id())
            .expect("validated family")
            .member_count()
            == 0;
        if target_is_empty {
            // Manim reuses the empty target's root style for each receiver child.
            // Preserve the current topology and publish all resulting paint edits
            // together instead of materializing duplicated child wrappers.
            let target_style = target.own_style()?;
            let receiver_nodes = self
                .integration_store()
                .borrow()
                .ordered_authoring_nodes(self.node_id())
                .map_err(AuthoringError::from)?;
            let mut transaction = SemanticMutationTransaction::new();
            for node in receiver_nodes {
                let family_style = self
                    .integration_store()
                    .borrow()
                    .node(node)
                    .and_then(|entry| entry.family_style())
                    .cloned();
                if let Some(original) = family_style {
                    let mut matched = original.clone();
                    match_paint(&mut matched, &target_style);
                    if matched != original {
                        transaction.set_family_style(node, matched);
                    }
                } else {
                    let object = Mobject::from_node(Rc::clone(self.integration_store()), node)?;
                    let mut matched = capture(&object)?;
                    let original = matched.clone();
                    match_paint(&mut matched, &target_style);
                    if matched != original {
                        transaction.replace_style(node, matched);
                    }
                }
            }
            return Ok(transaction);
        }
        let root_pairs = self
            .integration_store()
            .borrow()
            .ordered_family_root_pairs(self.node_id(), target.node_id())
            .map_err(AuthoringError::from)?;
        let mut transaction = SemanticMutationTransaction::new();
        {
            let store = self.integration_store().borrow();
            for (source, target) in root_pairs {
                let original = store
                    .node(source)
                    .expect("paired family node")
                    .family_style()
                    .expect("family style");
                let target_style = store
                    .node(target)
                    .expect("paired family node")
                    .family_style()
                    .expect("family style");
                let mut matched = original.clone();
                match_paint(&mut matched, target_style);
                if &matched != original {
                    transaction.set_family_style(source, matched);
                }
            }
        }
        let pairs = match self
            .integration_store()
            .borrow()
            .ordered_family_leaf_pairs(self.node_id(), target.node_id())
        {
            Ok(pairs) => pairs,
            Err(noon_core::SemanticFamilyPairingError::Empty) => return Ok(transaction),
            Err(error) => return Err(AuthoringError::from(error).into()),
        };
        let mut staged = HashMap::<noon_core::SemanticNodeId, SemanticStyle>::new();
        for (source, target) in pairs {
            let mut style = capture(&Mobject::from_node(
                Rc::clone(self.integration_store()),
                source,
            )?)?;
            let target_style = match staged.get(&target) {
                Some(style) => style.clone(),
                None => capture(&Mobject::from_node(
                    Rc::clone(self.integration_store()),
                    target,
                )?)?,
            };
            match_paint(&mut style, &target_style);
            if style
                != self
                    .integration_store()
                    .borrow()
                    .semantic_object_state_checked(source)
                    .map_err(AuthoringError::from)?
                    .style
            {
                transaction.replace_style(source, style.clone());
            }
            staged.insert(source, style);
        }
        Ok(transaction)
    }

    /// Recolor each unique leaf's enabled paint channels, preserving their opacity.
    pub fn set_color(
        &self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), AuthoringError> {
        self.color_transaction(red, green, blue, alpha, true)?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    /// Change this root without recoloring any descendant.
    pub fn set_own_color(
        &self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), AuthoringError> {
        self.color_transaction(red, green, blue, alpha, false)?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    pub(crate) fn color_transaction(
        &self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        recursive: bool,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        self.validate()?;
        // The established shared paint edit validates all channels before any staging.
        edit_color(&mut SemanticStyle::default(), red, green, blue, alpha)?;
        let color = Color::rgba(red as f32, green as f32, blue as f32, alpha as f32);
        let mut transaction = if recursive {
            self.style_transaction(|style| edit_color(style, red, green, blue, alpha))?
        } else {
            SemanticMutationTransaction::new()
        };
        let store = self.integration_store().borrow();
        let nodes = if recursive {
            store.ordered_last_occurrence_authoring_nodes(self.node_id())?
        } else {
            vec![self.node_id()]
        };
        for node in nodes {
            if let Some(previous) = store
                .node(node)
                .and_then(noon_core::SemanticNode::family_color)
            {
                if previous != color {
                    transaction.set_family_color(node, color);
                }
            }
        }
        Ok(transaction)
    }

    /// Match a donor leaf's authored RGB through one unique-leaf transaction.
    pub fn match_color(&self, donor: &Mobject) -> Result<(), AuthoringError> {
        let color = donor.manim_color()?;
        self.set_color(color.red.into(), color.green.into(), color.blue.into(), 1.0)
    }

    /// Change supplied fill fields atomically. Omitted fields remain unchanged.
    pub fn set_fill(
        &self,
        color: Option<Color>,
        opacity: Option<f64>,
    ) -> Result<(), AuthoringError> {
        self.set_style(StyleUpdate {
            fill_color: color,
            fill_opacity: opacity,
            ..StyleUpdate::default()
        })
    }

    /// Change supplied stroke fields in one transaction. Omitted fields remain unchanged.
    /// Width is in scene units, as for `Mobject::set_stroke_width`.
    pub fn set_stroke(
        &self,
        color: Option<Color>,
        width: Option<f64>,
        opacity: Option<f64>,
    ) -> Result<(), AuthoringError> {
        self.set_style(StyleUpdate {
            stroke_color: color,
            stroke_width: width,
            stroke_opacity: opacity,
            ..StyleUpdate::default()
        })
    }

    /// Set enabled fill/stroke opacity, preserving the object-composite multiplier.
    pub fn set_opacity(&self, opacity: f64) -> Result<(), AuthoringError> {
        self.opacity_transaction(opacity)?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    /// Multiply enabled fill and stroke alpha, matching Manim's `fade`.
    pub fn fade(&self, darkness: f64) -> Result<(), AuthoringError> {
        self.fade_transaction(darkness)?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    /// Interpolate every recursively owned object's color toward the target.
    pub fn fade_to(&self, target: Color, amount: f64) -> Result<(), AuthoringError> {
        self.fade_to_transaction(target, amount)?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    /// Prepare one validated paint transaction so authored and live owners
    /// publish the same family fade semantics.
    pub(crate) fn fade_transaction(
        &self,
        darkness: f64,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        self.validate()?;
        edit_fade(&mut SemanticStyle::default(), darkness)?;
        let store = self.integration_store().borrow();
        let nodes = store.ordered_authoring_multiplicities(self.node_id())?;
        let mut transaction = SemanticMutationTransaction::new();
        for (node, occurrences) in nodes {
            let entry = store.node(node).expect("validated family member");
            if let Some(previous) = entry.family_style() {
                let mut next = previous.clone();
                edit_fade_occurrences(&mut next, darkness, occurrences)?;
                let background_opacity = entry
                    .family_background_stroke_opacity()
                    .expect("family background opacity")
                    * fade_factor(darkness, occurrences)?;
                transaction.set_family_style_with_background_stroke_opacity(
                    node,
                    next,
                    background_opacity,
                );
                continue;
            }
            let previous = &store.semantic_object_state_checked(node)?.style;
            let mut next = previous.clone();
            edit_fade_occurrences(&mut next, darkness, occurrences)?;
            if next != *previous {
                transaction.replace_style(node, next);
            }
        }
        Ok(transaction)
    }

    pub(crate) fn fade_to_transaction(
        &self,
        target: Color,
        amount: f64,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        self.validate()?;
        let store = self.integration_store().borrow();
        let leaves = store.ordered_leaf_multiplicities(self.node_id())?;
        let mut transaction = SemanticMutationTransaction::new();
        for (leaf, occurrences) in leaves {
            let mobject = Mobject::from_node(self.integration_store().clone(), leaf)?;
            if !mobject.has_fade_to_points()? {
                continue;
            }
            let previous = &store.semantic_object_state_checked(leaf)?.style;
            let mut next = previous.clone();
            edit_fade_to_occurrences(&mut next, target, amount, occurrences)?;
            if next != *previous {
                transaction.replace_style(leaf, next);
            }
        }
        Ok(transaction)
    }

    pub(crate) fn style_transaction(
        &self,
        edit: impl Fn(&mut SemanticStyle) -> Result<(), AuthoringError>,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        self.style_transaction_indexed(|_, _, style| edit(style))
    }

    pub(crate) fn style_transaction_indexed(
        &self,
        edit: impl Fn(usize, usize, &mut SemanticStyle) -> Result<(), AuthoringError>,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        self.style_transaction_ordered(false, edit)
    }

    pub(crate) fn style_transaction_last_occurrence(
        &self,
        edit: impl Fn(usize, usize, &mut SemanticStyle) -> Result<(), AuthoringError>,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        self.style_transaction_ordered(true, edit)
    }

    fn style_transaction_ordered(
        &self,
        keep_last: bool,
        edit: impl Fn(usize, usize, &mut SemanticStyle) -> Result<(), AuthoringError>,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        self.validate()?;
        // Validate arguments even for an empty family, before staging any writes.
        edit(0, 1, &mut SemanticStyle::default())?;
        let store = self.integration_store().borrow();
        let leaves = if keep_last {
            store.ordered_last_occurrence_leaf_nodes(self.node_id())
        } else {
            store.ordered_leaf_nodes(self.node_id())
        }
        .map_err(AuthoringError::from)?;
        let mut transaction = SemanticMutationTransaction::new();
        let count = leaves.len();
        for (index, leaf) in leaves.into_iter().enumerate() {
            let previous = &store
                .semantic_object_state_checked(leaf)
                .map_err(AuthoringError::from)?
                .style;
            let mut next = previous.clone();
            edit(index, count, &mut next)?;
            if next != *previous {
                transaction.replace_style(leaf, next);
            }
        }
        Ok(transaction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ManimGeometryOptions, Scene};
    use noon_core::{SemanticPaint, Style, VectorPath};

    #[test]
    fn recursive_fade_counts_alias_paths_but_reads_and_publishes_unique_leaves() {
        let mut scene = Scene::new();
        let mut a = scene.square(0.6).unwrap();
        let mut b = scene.square(0.6).unwrap();
        for object in [&mut a, &mut b] {
            object.set_fill(1.0, 0.0, 0.0, 0.8).unwrap();
            object.set_stroke_opacity(0.4).unwrap();
            object.set_object_opacity(0.3).unwrap();
        }
        let nested = scene.family(&[(&a).into(), (&b).into()]).unwrap();
        let family = scene.family(&[(&nested).into(), (&a).into()]).unwrap();
        family.fade(0.5).unwrap();
        assert_eq!(a.fill_opacity().unwrap(), 0.2);
        assert_eq!(b.fill_opacity().unwrap(), 0.4);
        assert_eq!(a.stroke_opacity().unwrap(), 0.1);
        assert_eq!(b.stroke_opacity().unwrap(), 0.2);
        assert_eq!(a.state().unwrap().style.object_opacity, 0.3);
        let before = (a.state().unwrap(), b.state().unwrap());
        assert!(family.fade(f64::NAN).is_err());
        assert_eq!((a.state().unwrap(), b.state().unwrap()), before);

        let revision = scene.integration_store().borrow().scene_revision();
        let mut reads = Vec::new();
        let phase_style = noon_core::Style {
            fill: Some(Color::rgba(1.0, 0.0, 0.0, 0.5)),
            stroke: Some(Color::rgba(0.0, 0.0, 1.0, 0.75)),
            opacity: 0.3,
            ..Default::default()
        };
        let changes = family
            .prepare_callback_paint(revision, crate::FamilyPaint::Fade(0.5), |node| {
                reads.push(node);
                Ok(phase_style)
            })
            .unwrap();
        assert_eq!(reads, vec![a.node_id(), b.node_id()]);
        assert_eq!(changes[0].1.fill.unwrap().alpha, 0.125);
        assert_eq!(changes[1].1.fill.unwrap().alpha, 0.25);
        assert_eq!(changes[0].1.stroke.unwrap().alpha, 0.1875);
        assert_eq!(changes[0].1.opacity, phase_style.opacity);
        assert_eq!((a.state().unwrap(), b.state().unwrap()), before);
    }

    #[test]
    fn family_root_style_is_independent_and_updates_atomically() {
        let mut scene = Scene::new();
        let mut leaf = scene.square(1.0).unwrap();
        leaf.set_fill(1.0, 0.0, 0.0, 0.35).unwrap();
        let family = scene.family(&[(&leaf).into()]).unwrap();
        let leaf_before = leaf.state().unwrap().style;
        let root_before = family.own_style().unwrap();
        assert_eq!(root_before.fill_opacity, 0.0);
        assert_eq!(root_before.stroke_width, 0.04);
        assert_eq!(root_before.stroke_cap, noon_core::StrokeCap::Auto);

        let revision_before = scene.integration_store().borrow().scene_revision();
        family
            .set_own_style(StyleUpdate {
                fill_color: Some(Color::BLUE),
                fill_opacity: Some(0.7),
                ..StyleUpdate::default()
            })
            .unwrap();
        let root_after = family.own_style().unwrap();
        assert_eq!(
            root_after.fill,
            Some(noon_core::SemanticPaint::Solid(Color::BLUE))
        );
        assert_eq!(root_after.fill_opacity, 0.7);
        assert_eq!(leaf.state().unwrap().style, leaf_before);
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision_before.checked_next().unwrap()
        );

        family.set_own_opacity(0.25).unwrap();
        let opacity_style = family.own_style().unwrap();
        assert_eq!(opacity_style.fill_opacity, 0.25);
        assert_eq!(opacity_style.stroke_opacity, 0.25);
        assert_eq!(family.own_background_stroke_opacity().unwrap(), 0.25);
        assert_eq!(leaf.state().unwrap().style, leaf_before);
        let root_before = (
            family.own_style().unwrap(),
            family.own_background_stroke_opacity().unwrap(),
        );
        let revision_before = scene.integration_store().borrow().scene_revision();
        assert!(family.set_own_opacity(1.2).is_err());
        assert_eq!(
            (
                family.own_style().unwrap(),
                family.own_background_stroke_opacity().unwrap(),
            ),
            root_before
        );
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision_before
        );

        family.set_own_stroke_cap("square").unwrap();
        assert_eq!(
            family.own_stroke_cap().unwrap(),
            noon_core::StrokeCap::Square
        );
        assert_eq!(leaf.state().unwrap().style, leaf_before);

        let revision_after = scene.integration_store().borrow().scene_revision();
        let style_after = family.own_style().unwrap();
        assert!(family
            .set_own_style(StyleUpdate {
                stroke_width: Some(f64::NAN),
                ..StyleUpdate::default()
            })
            .is_err());
        assert_eq!(family.own_style().unwrap(), style_after);
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision_after
        );
    }

    #[test]
    fn family_root_fade_updates_background_opacity_without_touching_children() {
        let mut scene = Scene::new();
        let mut leaf = scene.square(1.0).unwrap();
        leaf.set_fill(1.0, 0.0, 0.0, 0.6).unwrap();
        leaf.set_stroke_opacity(0.4).unwrap();
        let family = scene.family(&[(&leaf).into()]).unwrap();
        let child_before = leaf.state().unwrap().style;
        assert_eq!(family.own_background_stroke_opacity().unwrap(), 1.0);

        let revision = scene.integration_store().borrow().scene_revision();
        family.fade_own(0.25).unwrap();
        assert_eq!(family.own_style().unwrap().fill_opacity, 0.0);
        assert_eq!(family.own_style().unwrap().stroke_opacity, 0.75);
        assert_eq!(family.own_background_stroke_opacity().unwrap(), 0.75);
        assert_eq!(leaf.state().unwrap().style, child_before);
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision.checked_next().unwrap()
        );

        let root_before = (
            family.own_style().unwrap(),
            family.own_background_stroke_opacity().unwrap(),
        );
        let revision = scene.integration_store().borrow().scene_revision();
        assert!(family.fade_own(f64::NAN).is_err());
        assert_eq!(
            (
                family.own_style().unwrap(),
                family.own_background_stroke_opacity().unwrap()
            ),
            root_before
        );
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision
        );

        family.set_own_background_stroke_opacity(0.2).unwrap();
        assert_eq!(family.own_background_stroke_opacity().unwrap(), 0.2);
        assert_eq!(leaf.state().unwrap().style, child_before);
        let root_before = family.own_background_stroke_opacity().unwrap();
        assert!(family.set_own_background_stroke_opacity(1.2).is_err());
        assert_eq!(family.own_background_stroke_opacity().unwrap(), root_before);
    }

    #[test]
    fn family_root_interpolation_matches_vmobject_style_channels_atomically() {
        let mut scene = Scene::new();
        let leaf = scene.square(1.0).unwrap();
        let receiver = scene.family(&[(&leaf).into()]).unwrap();
        let start = scene.family(&[(&leaf).into()]).unwrap();
        let end = scene.family(&[(&leaf).into()]).unwrap();
        let child_before = leaf.state().unwrap();

        receiver
            .set_own_style_with_background_stroke(
                StyleUpdate {
                    fill_color: Some(Color::rgba(1.0, 0.0, 0.0, 1.0)),
                    fill_opacity: Some(0.65),
                    stroke_color: Some(Color::GREEN),
                    stroke_width: Some(3.0),
                    stroke_opacity: Some(0.5),
                    ..StyleUpdate::default()
                },
                Some(Color::BLACK),
                Some(0.0),
                Some(0.7),
            )
            .unwrap();
        start
            .set_own_style_with_background_stroke(
                StyleUpdate {
                    fill_color: Some(Color::rgba(1.0, 0.0, 0.0, 1.0)),
                    fill_opacity: Some(0.2),
                    stroke_color: Some(Color::rgba(0.0, 0.0, 1.0, 1.0)),
                    stroke_width: Some(2.0),
                    stroke_opacity: Some(0.25),
                    ..StyleUpdate::default()
                },
                Some(Color::rgba(0.0, 1.0, 0.0, 1.0)),
                Some(2.0),
                Some(0.2),
            )
            .unwrap();
        end.set_own_style_with_background_stroke(
            StyleUpdate {
                fill_color: Some(Color::rgba(0.0, 0.0, 1.0, 1.0)),
                fill_opacity: Some(0.8),
                stroke_color: Some(Color::rgba(1.0, 0.0, 0.0, 1.0)),
                stroke_width: Some(6.0),
                stroke_opacity: Some(0.75),
                ..StyleUpdate::default()
            },
            Some(Color::WHITE),
            Some(6.0),
            Some(0.6),
        )
        .unwrap();
        start
            .set_sheen_direction(
                noon_core::SemanticSheenDirection::new(
                    noon_core::SemanticVec3::new(-1.0, 0.0, 0.0),
                    3,
                ),
                false,
            )
            .unwrap();
        end.set_sheen_direction(
            noon_core::SemanticSheenDirection::new(noon_core::SemanticVec3::new(1.0, 2.0, 0.0), 3),
            false,
        )
        .unwrap();

        let revision = scene.integration_store().borrow().scene_revision();
        receiver
            .interpolate_manim_style(&start, &end, 0.25)
            .unwrap();

        let style = receiver.own_style().unwrap();
        assert_eq!(
            style.fill,
            Some(SemanticPaint::Solid(Color::rgba(0.75, 0.0, 0.25, 1.0)))
        );
        assert!((style.fill_opacity - 0.35).abs() < 1e-6);
        assert_eq!(
            style.stroke,
            Some(SemanticPaint::Solid(Color::rgba(0.25, 0.0, 0.75, 1.0)))
        );
        assert!((style.stroke_opacity - 0.375).abs() < 1e-6);
        assert!((style.stroke_width - 3.0).abs() < 1e-6);
        assert_eq!(
            receiver.own_background_stroke_color().unwrap(),
            Color::rgba(0.25, 1.0, 0.25, 1.0)
        );
        assert!((receiver.own_background_stroke_width().unwrap() - 3.0).abs() < 1e-6);
        assert!((receiver.own_background_stroke_opacity().unwrap() - 0.3).abs() < 1e-6);
        assert_eq!(
            receiver.own_sheen_direction().unwrap().components(),
            [-0.5, 0.5, 0.0]
        );
        assert_eq!(leaf.state().unwrap(), child_before);
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision.checked_next().unwrap()
        );

        let root_before = (
            receiver.own_style().unwrap(),
            receiver.own_background_stroke_color().unwrap(),
            receiver.own_background_stroke_width().unwrap(),
            receiver.own_background_stroke_opacity().unwrap(),
            receiver.own_sheen_direction().unwrap(),
        );
        let revision = scene.integration_store().borrow().scene_revision();
        assert!(receiver
            .interpolate_manim_style(&start, &end, f64::NAN)
            .is_err());
        assert_eq!(
            (
                receiver.own_style().unwrap(),
                receiver.own_background_stroke_color().unwrap(),
                receiver.own_background_stroke_width().unwrap(),
                receiver.own_background_stroke_opacity().unwrap(),
                receiver.own_sheen_direction().unwrap(),
            ),
            root_before
        );
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision
        );
    }

    #[test]
    fn family_root_style_and_background_opacity_update_atomically() {
        let mut scene = Scene::new();
        let mut leaf = scene.square(1.0).unwrap();
        leaf.set_fill(1.0, 0.0, 0.0, 0.6).unwrap();
        let family = scene.family(&[(&leaf).into()]).unwrap();
        let child_before = leaf.state().unwrap().style;
        let revision = scene.integration_store().borrow().scene_revision();

        family
            .set_own_style_with_background_stroke(
                StyleUpdate {
                    fill_color: Some(Color::GREEN),
                    fill_opacity: Some(0.35),
                    stroke_color: Some(Color::BLUE),
                    stroke_width: Some(4.0),
                    stroke_opacity: Some(0.45),
                    ..StyleUpdate::default()
                },
                Some(Color::BLUE),
                Some(0.06),
                Some(0.55),
            )
            .unwrap();

        let root_style = family.own_style().unwrap();
        assert_eq!(root_style.fill, Some(SemanticPaint::Solid(Color::GREEN)));
        assert_eq!(root_style.fill_opacity, 0.35);
        assert_eq!(root_style.stroke, Some(SemanticPaint::Solid(Color::BLUE)));
        assert_eq!(root_style.stroke_width, 4.0);
        assert_eq!(root_style.stroke_opacity, 0.45);
        assert_eq!(family.own_background_stroke_color().unwrap(), Color::BLUE);
        assert_eq!(family.own_background_stroke_width().unwrap(), 0.06);
        assert_eq!(family.own_background_stroke_opacity().unwrap(), 0.55);
        assert_eq!(leaf.state().unwrap().style, child_before);
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision.checked_next().unwrap()
        );

        let root_before = (
            family.own_style().unwrap(),
            family.own_background_stroke_color().unwrap(),
            family.own_background_stroke_width().unwrap(),
            family.own_background_stroke_opacity().unwrap(),
        );
        let revision = scene.integration_store().borrow().scene_revision();
        assert!(family
            .set_own_style_with_background_stroke_opacity(
                StyleUpdate {
                    stroke_width: Some(f64::NAN),
                    ..StyleUpdate::default()
                },
                0.2,
            )
            .is_err());
        assert_eq!(
            (
                family.own_style().unwrap(),
                family.own_background_stroke_color().unwrap(),
                family.own_background_stroke_width().unwrap(),
                family.own_background_stroke_opacity().unwrap(),
            ),
            root_before
        );
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision
        );
        assert_eq!(leaf.state().unwrap().style, child_before);
    }

    #[test]
    fn family_root_style_survives_copy_and_become() {
        let mut scene = Scene::new();
        let first = scene.square(0.5).unwrap();
        let second = scene.circle(0.25).unwrap();
        let source = scene.family(&[(&first).into()]).unwrap();
        let target = scene.family(&[(&second).into()]).unwrap();
        source
            .set_own_style(StyleUpdate {
                fill_color: Some(Color::BLUE),
                fill_opacity: Some(0.6),
                stroke_color: Some(Color::GREEN),
                stroke_width: Some(2.5),
                ..StyleUpdate::default()
            })
            .unwrap();
        source
            .set_own_style_with_background_stroke(
                StyleUpdate::default(),
                Some(Color::BLUE),
                Some(0.06),
                Some(0.7),
            )
            .unwrap();
        let expected = source.own_style().unwrap();
        assert_eq!(source.own_color().unwrap(), Color::BLUE);

        let copy = source.copy_family().unwrap();
        assert_eq!(copy.root().own_style().unwrap(), expected);
        assert_eq!(copy.root().own_background_stroke_opacity().unwrap(), 0.7);
        assert_eq!(
            copy.root().own_background_stroke_color().unwrap(),
            Color::BLUE
        );
        assert_eq!(copy.root().own_background_stroke_width().unwrap(), 0.06);
        assert_eq!(copy.root().own_color().unwrap(), Color::BLUE);
        target
            .set_own_style(StyleUpdate {
                fill_color: Some(Color::RED),
                ..StyleUpdate::default()
            })
            .unwrap();
        target
            .become_family(&source, crate::ManimBecomeOptions::default())
            .unwrap();
        assert_eq!(target.own_style().unwrap(), expected);
        assert_eq!(target.own_background_stroke_opacity().unwrap(), 0.7);
        assert_eq!(target.own_background_stroke_color().unwrap(), Color::BLUE);
        assert_eq!(target.own_background_stroke_width().unwrap(), 0.06);
        assert_eq!(target.own_color().unwrap(), Color::BLUE);
    }

    #[test]
    fn family_root_style_survives_persistent_become_reconciliation() {
        let mut scene = Scene::new();
        let source_leaf = scene.square(0.5).unwrap();
        let target_leaf_a = scene.circle(0.25).unwrap();
        let target_leaf_b = scene.square(0.3).unwrap();
        let source = scene.family(&[(&source_leaf).into()]).unwrap();
        let target = scene
            .family(&[(&target_leaf_a).into(), (&target_leaf_b).into()])
            .unwrap();
        target
            .set_own_style(StyleUpdate {
                fill_color: Some(Color::BLUE),
                fill_opacity: Some(0.4),
                stroke_color: Some(Color::GREEN),
                stroke_opacity: Some(0.75),
                stroke_width: Some(2.5),
                ..StyleUpdate::default()
            })
            .unwrap();
        let expected = target.own_style().unwrap();

        source
            .become_family(&target, crate::ManimBecomeOptions::default())
            .unwrap();

        assert_eq!(source.own_style().unwrap(), expected);
        assert_eq!(source.own_color().unwrap(), Color::BLUE);
    }

    #[test]
    fn live_family_root_style_publishes_without_changing_child_paint() {
        let mut scene = Scene::new();
        let leaf = scene.square(0.5).unwrap();
        let family = scene.family(&[(&leaf).into()]).unwrap();
        scene.add_many(&[(&family).into()]).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let leaf_before = live.effective(&leaf).unwrap();

        let result = live
            .set_family_own_style_with_background_stroke(
                &family,
                StyleUpdate {
                    fill_color: Some(Color::GREEN),
                    fill_opacity: Some(0.4),
                    stroke_opacity: Some(0.25),
                    ..StyleUpdate::default()
                },
                Some(Color::BLUE),
                Some(0.06),
                Some(0.55),
            )
            .unwrap();

        assert_eq!(
            result.impacts(),
            &[noon_core::SemanticMutationImpact::FamilyStyle {
                node: family.node_id()
            }]
        );
        assert_eq!(family.own_style().unwrap().fill_opacity, 0.4);
        assert_eq!(family.own_style().unwrap().stroke_opacity, 0.25);
        assert_eq!(family.own_background_stroke_color().unwrap(), Color::BLUE);
        assert_eq!(family.own_background_stroke_width().unwrap(), 0.06);
        assert_eq!(family.own_background_stroke_opacity().unwrap(), 0.55);
        assert_eq!(live.effective(&leaf).unwrap().style, leaf_before.style);

        live.set_family_own_stroke_cap(&family, "square").unwrap();
        assert_eq!(
            family.own_stroke_cap().unwrap(),
            noon_core::StrokeCap::Square
        );
        assert_eq!(live.effective(&leaf).unwrap().style, leaf_before.style);
        live.set_stroke_cap(&leaf, "butt").unwrap();
        assert_eq!(
            live.effective_stroke_cap(&leaf).unwrap(),
            noon_core::StrokeCap::Butt
        );
        assert_eq!(
            family.own_stroke_cap().unwrap(),
            noon_core::StrokeCap::Square
        );
    }

    #[test]
    fn recursive_fade_to_observes_alias_multiplicity_and_preserves_paint_opacity() {
        let mut scene = Scene::new();
        let mut a = scene.square(0.6).unwrap();
        let mut direct = scene.square(0.4).unwrap();
        direct.set_fill_color(1.0, 0.0, 0.0, 1.0).unwrap();
        direct.set_fill_opacity(1.0).unwrap();
        a.set_fill_color(1.0, 0.0, 0.0, 1.0).unwrap();
        a.set_fill_opacity(0.5).unwrap();
        a.set_object_opacity(0.3).unwrap();
        let nested = scene.family(&[(&a).into()]).unwrap();
        let family = scene.family(&[(&nested).into(), (&a).into()]).unwrap();

        let target = Color::rgba(0.0, 0.0, 1.0, 0.25);
        let mut empty = scene
            .geometry(ManimGeometryOptions::path(VectorPath::new()).unwrap())
            .unwrap();
        let empty_before = empty.state().unwrap();
        empty.fade_to(target, 0.5).unwrap();
        assert_eq!(empty.state().unwrap(), empty_before);

        direct.fade_to(target, 0.5).unwrap();
        let Some(SemanticPaint::Solid(direct_fill)) = direct.state().unwrap().style.fill else {
            panic!("direct fade_to should preserve solid fill paint");
        };
        assert!((direct_fill.red - 0.5).abs() < 1e-6);
        assert!((direct_fill.blue - 0.5).abs() < 1e-6);
        assert_eq!(direct_fill.alpha, 1.0);
        assert_eq!(direct.fill_opacity().unwrap(), 1.0);

        family.fade_to(target, 0.5).unwrap();
        let style = &a.state().unwrap().style;
        let Some(SemanticPaint::Solid(fill)) = style.fill.as_ref() else {
            panic!("fade_to should preserve solid fill paint");
        };
        assert!((fill.red - 0.25).abs() < 1e-6);
        assert!((fill.blue - 0.75).abs() < 1e-6);
        assert_eq!(fill.alpha, 1.0);
        assert!((style.fill_opacity - 0.5).abs() < 1e-6);
        assert!((style.stroke_opacity - 1.0).abs() < 1e-6);
        assert_eq!(style.object_opacity, 0.3);

        let mut black = scene.square(0.3).unwrap();
        black.set_fill_color(0.0, 0.0, 0.0, 1.0).unwrap();
        black.set_fill_opacity(1.0).unwrap();
        let black_family = scene.family(&[(&black).into()]).unwrap();
        black_family
            .fade_to(Color::rgb(0.0, 0.0, 0.0), 1.5)
            .unwrap();
        assert_eq!(black.manim_color().unwrap(), Color::rgb(0.0, 0.0, 0.0));
        let black_callback = black_family
            .prepare_callback_paint(
                scene.integration_store().borrow().scene_revision(),
                crate::FamilyPaint::FadeTo {
                    color: Color::rgb(0.0, 0.0, 0.0),
                    amount: 1.5,
                },
                |_| {
                    Ok(Style {
                        fill: Some(Color::rgb(0.0, 0.0, 0.0)),
                        ..Default::default()
                    })
                },
            )
            .unwrap();
        assert!(black_callback.is_empty());

        scene.add(&direct).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        live.fade_to(&direct, target, 0.5).unwrap();
        let Some(live_fill) = live.effective(&direct).unwrap().style.fill else {
            panic!("live fade_to should preserve solid fill paint");
        };
        assert!((live_fill.red - 0.25).abs() < 1e-6);
        assert!((live_fill.blue - 0.75).abs() < 1e-6);
        assert_eq!(live_fill.alpha, 1.0);
        assert_eq!(live.effective(&direct).unwrap().fill_opacity(), 1.0);

        let before = a.state().unwrap();
        assert!(family.fade_to(target, f64::NAN).is_err());
        assert_eq!(a.state().unwrap(), before);

        let revision = scene.integration_store().borrow().scene_revision();
        let phase_style = Style {
            fill: Some(Color::rgba(1.0, 0.0, 0.0, 0.5)),
            stroke: Some(Color::rgba(0.0, 0.0, 1.0, 0.25)),
            opacity: 0.3,
            ..Default::default()
        };
        let changes = family
            .prepare_callback_paint(
                revision,
                crate::FamilyPaint::FadeTo {
                    color: target,
                    amount: 0.5,
                },
                |_| Ok(phase_style),
            )
            .unwrap();
        assert_eq!(changes.len(), 1);
        assert!((changes[0].1.fill.unwrap().red - 0.25).abs() < 1e-6);
        assert!((changes[0].1.fill.unwrap().blue - 0.75).abs() < 1e-6);
        assert_eq!(changes[0].1.fill.unwrap().alpha, 0.5);
        assert_eq!(changes[0].1.stroke.unwrap().alpha, 0.25);
        assert_eq!(changes[0].1.opacity, 0.3);
        assert_eq!(a.state().unwrap(), before);
    }

    #[test]
    fn member_colors_validate_count_before_atomic_publication() {
        let mut scene = Scene::new();
        let first = scene.rectangle(1.0, 1.0).unwrap();
        let second = scene.circle(0.5).unwrap();
        let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        let before = (first.state().unwrap(), second.state().unwrap());

        assert!(matches!(
            family.set_member_colors(&[Some(crate::RED)]),
            Err(AuthoringError::FamilyMemberValueCount {
                expected: 2,
                actual: 1
            })
        ));
        assert_eq!(first.state().unwrap(), before.0);
        assert_eq!(second.state().unwrap(), before.1);

        family
            .set_member_colors(&[Some(crate::RED), Some(crate::BLUE)])
            .unwrap();
        assert_eq!(
            first.state().unwrap().style.fill,
            Some(noon_core::SemanticPaint::Solid(crate::RED))
        );
        assert_eq!(
            second.state().unwrap().style.fill,
            Some(noon_core::SemanticPaint::Solid(crate::BLUE))
        );
    }
}
