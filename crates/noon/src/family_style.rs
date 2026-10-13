//! Family paint uses the same style edits and atomic semantic transaction as leaves.
use crate::AuthoringError;
use crate::{
    semantic_mobject::{
        edit_color, edit_fade, edit_fade_occurrences, edit_fill_color, edit_fill_opacity,
        edit_manim_opacity, edit_stroke_color, edit_stroke_opacity, edit_stroke_width,
        PaintStyleEdit,
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
                return Ok(SemanticMutationTransaction::new());
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

    /// Multiply enabled fill and stroke alpha, matching Manim's `fade`.
    pub fn fade(&self, darkness: f64) -> Result<(), AuthoringError> {
        self.fade_transaction(darkness)?
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
        let leaves = store.ordered_leaf_multiplicities(self.node_id())?;
        let mut transaction = SemanticMutationTransaction::new();
        for (leaf, occurrences) in leaves {
            let previous = &store.semantic_object_state_checked(leaf)?.style;
            let mut next = previous.clone();
            edit_fade_occurrences(&mut next, darkness, occurrences)?;
            if next != *previous {
                transaction.replace_style(leaf, next);
            }
        }
        Ok(transaction)
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
        self.style_transaction_indexed(|_, _, style| edit(style))
    }

    pub(crate) fn style_transaction_indexed(
        &self,
        edit: impl Fn(usize, usize, &mut SemanticStyle) -> Result<(), AuthoringError>,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        self.validate()?;
        // Validate arguments even for an empty family, before staging any writes.
        edit(0, 1, &mut SemanticStyle::default())?;
        let store = self.integration_store().borrow();
        let leaves = store
            .ordered_leaf_nodes(self.node_id())
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
    use crate::Scene;

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
