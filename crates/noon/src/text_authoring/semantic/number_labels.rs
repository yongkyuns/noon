//! Numeric labels become ordinary Text leaves through one cold transaction.
use super::super::{Text, NATIVE_POINT_TO_SCENE_SCALE};
use crate::family_layout::RelativePlacement;
use crate::plot_presentation::{
    number_labels, NumberLabelAuthoringError as Error, NumberLabelOptions, PlotPresentationError,
};
use crate::{
    AuthoringError, Bounds2D64, ManimAxes, ManimNextToArgs, ManimNumberLine, MobjectFamily,
    NumberLineFrame,
};
use noon_core::{
    FontResourceArena, SemanticMutationTransaction, SemanticNodeCreation, SemanticNodeId,
    SemanticObjectState, SemanticPaint, SemanticStore, SemanticStyle, SemanticTransform2_5D,
    SemanticVec3, TextResource,
};
use std::{cell::RefCell, rc::Rc};

struct PreparedLabel {
    resource: TextResource,
    fonts: FontResourceArena,
    transform: SemanticTransform2_5D,
    style: SemanticStyle,
}

fn prepare(
    frame: NumberLineFrame,
    numbers: Option<&[f64]>,
    options: &NumberLabelOptions,
) -> Result<Vec<PreparedLabel>, Error> {
    let [dx, dy] = options.direction;
    if !dx.is_finite()
        || !dy.is_finite()
        || (dx == 0.0 && dy == 0.0)
        || !options.buff.is_finite()
        || !options.font_size.is_finite()
        || options.font_size <= 0.0
    {
        return Err(
            PlotPresentationError::InvalidInput("invalid number-label presentation").into(),
        );
    }
    let style = SemanticStyle {
        fill: Some(SemanticPaint::Solid(options.color)),
        stroke: None,
        stroke_width: 0.0,
        ..SemanticStyle::default()
    };
    if !style.is_finite() {
        return Err(AuthoringError::NonFiniteStyle.into());
    }
    let labels = number_labels(frame, numbers, options.decimal_places, options.exclude_zero)?;
    let mut prepared = Vec::new();
    prepared.try_reserve_exact(labels.len())?;
    for label in labels {
        let text = Text::new(label.text)
            .with_font(options.font.as_str())
            .with_font_size(options.font_size);
        let artifact = text.compile_artifact_with_fill(None)?;
        // Native glyph resources are in font pixels. This is the same point-to-
        // scene scale used by ordinary Text, before any authored transform.
        let scale = f64::from(NATIVE_POINT_TO_SCENE_SCALE);
        let local = artifact.resource.bounds;
        let bounds = Bounds2D64 {
            min_x: f64::from(local.min.x) * scale,
            min_y: f64::from(local.min.y) * scale,
            max_x: f64::from(local.max.x) * scale,
            max_y: f64::from(local.max.y) * scale,
        };
        let (x, y) = RelativePlacement::Next(ManimNextToArgs {
            direction: (dx, dy),
            buff: options.buff,
            aligned_edge: (0.0, 0.0),
            mask: (1.0, 1.0),
        })
        .delta::<Error>(Some(bounds), |_, _| Ok((label.point[0], label.point[1])))?;
        let translation = crate::semantic_mobject::authoring_xy_f64(x, y)?;
        prepared.push(PreparedLabel {
            resource: artifact.resource,
            fonts: artifact.fonts,
            transform: SemanticTransform2_5D {
                translation,
                scale: SemanticVec3::new(scale, scale, 1.0),
                rotation_z: 0.0,
            },
            style: style.clone(),
        });
    }
    Ok(prepared)
}

fn publish(
    store: Rc<RefCell<SemanticStore>>,
    groups: Vec<(Option<SemanticNodeId>, Vec<PreparedLabel>)>,
) -> Result<Vec<MobjectFamily>, Error> {
    let count: usize = groups.iter().map(|(_, labels)| labels.len()).sum();
    let mut inputs = Vec::new();
    inputs.try_reserve_exact(count)?;
    let mut presentations = Vec::new();
    presentations.try_reserve_exact(count)?;
    let mut targets = Vec::new();
    targets.try_reserve_exact(groups.len())?;
    for (parent, labels) in groups {
        targets.push((parent, labels.len()));
        for label in labels {
            inputs.push((label.resource, label.fonts));
            presentations.push((label.transform, label.style));
        }
    }
    let mut roots = Vec::new();
    roots.try_reserve_exact(targets.len())?;
    let result = store
        .borrow_mut()
        .apply_glyph_text_transaction::<Error>(inputs, |handles| {
            let mut transaction = SemanticMutationTransaction::new();
            let mut entries = handles.iter().zip(presentations);
            for (parent, count) in targets {
                let root = transaction.create_node(SemanticNodeCreation::family());
                roots.push(root);
                for _ in 0..count {
                    let (handle, (transform, style)) =
                        entries.next().expect("prepared label count");
                    let mut state = SemanticObjectState::new(*handle);
                    state.transform = transform;
                    state.style = style;
                    let leaf = transaction.create_node(SemanticNodeCreation::object(state));
                    transaction.add_member(root, leaf);
                }
                if let Some(parent) = parent {
                    transaction.add_member(parent, root);
                }
            }
            Ok(transaction)
        })?;
    roots
        .into_iter()
        .map(|root| {
            let node = result
                .resolve(root)
                .ok_or(AuthoringError::UnresolvedCreatedNode(root))?;
            Ok(MobjectFamily::from_node(Rc::clone(&store), node)?)
        })
        .collect()
}

impl ManimNumberLine {
    /// Cold constructor for a detached family of native Text labels. None uses
    /// tick values; Some(&[]) is an explicit empty family. Shared group operations
    /// can attach, copy, style or transform the result without re-shaping text.
    pub fn get_number_mobjects(
        &self,
        numbers: Option<&[f64]>,
        options: &NumberLabelOptions,
    ) -> Result<MobjectFamily, Error> {
        let labels = prepare(self.authored_frame()?, numbers, options)?;
        Ok(publish(
            Rc::clone(self.family().integration_store()),
            vec![(None, labels)],
        )?
        .remove(0))
    }

    /// Cold-only atomic construction AND attachment. Each call appends a label
    /// family, matching ordinary add semantics; it does not silently replace
    /// earlier labels. It does not publish into an already running execution.
    pub fn add_numbers(
        &self,
        numbers: Option<&[f64]>,
        options: &NumberLabelOptions,
    ) -> Result<MobjectFamily, Error> {
        let labels = prepare(self.authored_frame()?, numbers, options)?;
        Ok(publish(
            Rc::clone(self.family().integration_store()),
            vec![(Some(self.family().node_id()), labels)],
        )?
        .remove(0))
    }
}

impl ManimAxes {
    /// Cold-only construction and attachment of both axis label families in one
    /// transaction. A failure preparing Y cannot leave X labels/resources behind.
    /// Text labels are an explicit Noon native-Text subset, not DecimalNumber or
    /// TeX glyph parity. Return handles to the X/Y label families respectively.
    pub fn add_coordinates(
        &self,
        x_numbers: Option<&[f64]>,
        y_numbers: Option<&[f64]>,
        x_options: &NumberLabelOptions,
        y_options: &NumberLabelOptions,
    ) -> Result<[MobjectFamily; 2], Error> {
        let x = self.x_axis()?;
        let y = self.y_axis()?;
        let frame = self.authored_frame()?;
        let x_labels = prepare(frame.x(), x_numbers, x_options)?;
        let y_labels = prepare(frame.y(), y_numbers, y_options)?;
        let mut families = publish(
            Rc::clone(self.family().integration_store()),
            vec![
                (Some(x.family().node_id()), x_labels),
                (Some(y.family().node_id()), y_labels),
            ],
        )?;
        let y = families.pop().expect("Y label family");
        let x = families.pop().expect("X label family");
        Ok([x, y])
    }
}

#[cfg(all(test, feature = "bundled-fonts"))]
mod tests;
