//! Family paint uses the same style edits and atomic semantic transaction as leaves.
use crate::AuthoringError;
use crate::{
    semantic_mobject::{
        edit_color, edit_fill_color, edit_fill_opacity, edit_manim_opacity, edit_stroke_color,
        edit_stroke_opacity, edit_stroke_width, PaintStyleEdit,
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
}

impl StyleUpdate {
    pub(crate) fn apply<S: PaintStyleEdit>(&self, style: &mut S) -> Result<(), AuthoringError> {
        fill(style, self.fill_color, self.fill_opacity)?;
        stroke(
            style,
            self.stroke_color,
            self.stroke_width,
            self.stroke_opacity,
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

impl MobjectFamily {
    /// Atomically update supplied paint fields on each unique leaf.
    pub fn set_style(&self, update: StyleUpdate) -> Result<(), AuthoringError> {
        self.edit_style(|style| update.apply(style))
    }

    /// Match paint through the shared topology/alias pairing contract.
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
        let pairs = match self
            .integration_store()
            .borrow()
            .ordered_family_leaf_pairs(self.node_id(), target.node_id())
        {
            Ok(pairs) => pairs,
            Err(noon_core::SemanticFamilyPairingError::Empty) => {
                return Ok(SemanticMutationTransaction::new())
            }
            Err(error) => return Err(AuthoringError::from(error).into()),
        };
        let mut staged = HashMap::<noon_core::SemanticNodeId, SemanticStyle>::new();
        let mut transaction = SemanticMutationTransaction::new();
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
        self.edit_style(|style| edit_color(style, red, green, blue, alpha))
    }

    /// Change supplied fill fields atomically. Omitted fields remain unchanged.
    pub fn set_fill(
        &self,
        color: Option<Color>,
        opacity: Option<f64>,
    ) -> Result<(), AuthoringError> {
        self.edit_style(|style| fill(style, color, opacity))
    }

    /// Change supplied stroke fields in one transaction. Omitted fields remain unchanged.
    /// Width is in scene units, as for `Mobject::set_stroke_width`.
    pub fn set_stroke(
        &self,
        color: Option<Color>,
        width: Option<f64>,
        opacity: Option<f64>,
    ) -> Result<(), AuthoringError> {
        self.edit_style(|style| stroke(style, color, width, opacity))
    }

    /// Set enabled fill/stroke opacity, preserving the object-composite multiplier.
    pub fn set_opacity(&self, opacity: f64) -> Result<(), AuthoringError> {
        self.edit_style(|style| edit_manim_opacity(style, opacity))
    }

    fn edit_style(
        &self,
        edit: impl Fn(&mut SemanticStyle) -> Result<(), AuthoringError>,
    ) -> Result<(), AuthoringError> {
        self.style_transaction(edit)?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    pub(crate) fn style_transaction(
        &self,
        edit: impl Fn(&mut SemanticStyle) -> Result<(), AuthoringError>,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        self.validate()?;
        // Validate arguments even for an empty family, before staging any writes.
        edit(&mut SemanticStyle::default())?;
        let store = self.integration_store().borrow();
        let leaves = store
            .ordered_leaf_nodes(self.node_id())
            .map_err(AuthoringError::from)?;
        let mut transaction = SemanticMutationTransaction::new();
        for leaf in leaves {
            let previous = &store
                .semantic_object_state_checked(leaf)
                .map_err(AuthoringError::from)?
                .style;
            let mut next = previous.clone();
            edit(&mut next)?;
            if next != *previous {
                transaction.replace_style(leaf, next);
            }
        }
        Ok(transaction)
    }
}
