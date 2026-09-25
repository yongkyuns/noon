//! DecimalNumber coordinate labels prepared and published as one semantic batch.

use crate::numeric_authoring::PreparedDecimalValue;
use crate::plot_presentation::{
    number_labels, NumberLabelAuthoringError as Error, NumberLabelOptions,
};
use crate::{
    AuthoringError, Bounds2D64, DecimalFormat, LatexBackend, ManimNextToArgs,
    ManimNumberLine, MobjectFamily, NumberLineFrame, TextAuthoringError,
};
use noon_core::{
    SemanticMutationTransaction, SemanticMutationTransactionResult, SemanticNodeCreation,
    SemanticNodeId, SemanticPaint, SemanticStore, SemanticTransactionNodeRef, SemanticVec3,
};
use std::rc::Rc;

/// A fully prepared family of DecimalNumber labels. Glyph compilation happens
/// before admission; publication admits every dependency and every leaf through
/// one transaction so a rejected family leaves no resources or identities.
pub(crate) struct PreparedDecimalLabels {
    values: Vec<PreparedDecimalValue>,
    placements: Vec<(f64, [f64; 2])>,
    options: NumberLabelOptions,
}

impl PreparedDecimalLabels {
    pub(crate) fn prepare(
        backend: &mut impl LatexBackend,
        frame: NumberLineFrame,
        numbers: Option<&[f64]>,
        options: &NumberLabelOptions,
    ) -> Result<Self, Error> {
        if !options.buff.is_finite()
            || options.direction.iter().any(|value| !value.is_finite())
            || options.direction == [0.0, 0.0]
            || !options.font_size.is_finite()
            || options.font_size <= 0.0
        {
            return Err(AuthoringError::NonFiniteTransform.into());
        }
        let labels = number_labels(frame, numbers, options.decimal_places, options.exclude_zero)?;
        let mut values = Vec::with_capacity(labels.len());
        let mut placements = Vec::with_capacity(labels.len());
        for label in labels {
            values.push(PreparedDecimalValue::prepare(
                backend,
                label.number,
                DecimalFormat {
                    decimal_places: options.decimal_places,
                    ..Default::default()
                },
                options.font_size,
            )?);
            placements.push((label.number, label.point));
        }
        Ok(Self { values, placements, options: options.clone() })
    }

    pub(crate) fn publish(
        self,
        store: &mut SemanticStore,
        parent: Option<SemanticTransactionNodeRef>,
        publish: impl FnOnce(
            &mut SemanticStore,
            SemanticMutationTransaction,
        ) -> Result<SemanticMutationTransactionResult, TextAuthoringError>,
    ) -> Result<(SemanticMutationTransactionResult, SemanticNodeId), Error> {
        let Self { values, placements, options } = self;
        let mut root = None;
        let result = PreparedDecimalValue::publish_batch(store, values, |store, handles, values| {
            let mut transaction = SemanticMutationTransaction::new();
            let family = transaction.create_node(SemanticNodeCreation::family());
            root = Some(family);
            for ((handle, value), (number, point)) in handles.iter().zip(values).zip(placements) {
                let resource = store.text_resources().get(*handle).expect("staged decimal label");
                let bounds = resource.bounds;
                let (mut x, y) = crate::family_layout::RelativePlacement::Next(ManimNextToArgs {
                    direction: (options.direction[0], options.direction[1]),
                    buff: options.buff,
                    aligned_edge: (0.0, 0.0),
                    mask: (1.0, 1.0),
                })
                .delta::<AuthoringError>(Some(Bounds2D64 {
                    min_x: f64::from(bounds.min.x), min_y: f64::from(bounds.min.y),
                    max_x: f64::from(bounds.max.x), max_y: f64::from(bounds.max.y),
                }), |_, _| Ok((point[0], point[1])))
                .map_err(TextAuthoringError::Semantic)?;
                if number < 0.0 && options.direction[0] == 0.0 && resource.source.starts_with('-') {
                    if let Some(glyph) = resource.runs.first().and_then(|run| run.glyphs.first()) {
                        x -= f64::from(glyph.bounds.width()) / 2.0;
                    }
                }
                let mut state = value.decimal_state(
                    store,
                    *handle,
                    noon_core::SemanticTransform2_5D {
                        translation: SemanticVec3::new(x, y, 0.0),
                        ..Default::default()
                    },
                )?;
                state.style.fill = Some(SemanticPaint::Solid(options.color));
                state.style.stroke = None;
                let leaf = transaction.create_node(SemanticNodeCreation::object(state));
                transaction.add_member(family, leaf);
            }
            if let Some(parent) = parent { transaction.add_member(parent, family); }
            publish(store, transaction)
        })?;
        let token = root.expect("decimal label family");
        let root = result
            .resolve(token)
            .ok_or(AuthoringError::UnresolvedCreatedNode(token))?;
        Ok((result, root))
    }
}

impl ManimNumberLine {
    /// Construct a detached DecimalNumber family. `None` selects tick values.
    pub fn decimal_number_labels(
        &self, backend: &mut impl LatexBackend, numbers: Option<&[f64]>, options: &NumberLabelOptions,
    ) -> Result<MobjectFamily, Error> {
        let prepared = PreparedDecimalLabels::prepare(backend, self.authored_frame()?, numbers, options)?;
        let store = Rc::clone(self.family().integration_store());
        let (_, root) = prepared.publish(&mut store.borrow_mut(), None, |store, transaction| {
            transaction.apply(store).map_err(AuthoringError::from).map_err(TextAuthoringError::Semantic)
        })?;
        Ok(MobjectFamily::from_node(store, root)?)
    }

    /// Construct and attach a DecimalNumber family atomically.
    pub fn add_decimal_numbers(
        &self, backend: &mut impl LatexBackend, numbers: Option<&[f64]>, options: &NumberLabelOptions,
    ) -> Result<MobjectFamily, Error> {
        let prepared = PreparedDecimalLabels::prepare(backend, self.authored_frame()?, numbers, options)?;
        let store = Rc::clone(self.family().integration_store());
        let (_, root) = prepared.publish(&mut store.borrow_mut(), Some(self.family().node_id().into()), |store, transaction| {
            transaction.apply(store).map_err(AuthoringError::from).map_err(TextAuthoringError::Semantic)
        })?;
        Ok(MobjectFamily::from_node(store, root)?)
    }
}
