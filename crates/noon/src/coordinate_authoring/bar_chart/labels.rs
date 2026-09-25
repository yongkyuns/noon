//! Retained chart names and value labels over the shared text admission path.

use super::*;
use crate::plot_presentation::NumberLabelAuthoringError as Error;
use crate::{Bounds2D64, LatexBackend, ManimNextToArgs, TextAuthoringError};
use noon_core::{SemanticMutationTransactionResult, SemanticStore, SemanticTransactionNodeRef};

/// Presentation options for retained labels placed at each bar edge.
#[derive(Clone, Debug)]
pub struct BarLabelOptions {
    pub font_size: f32,
    pub buff: f64,
    pub color: Option<Color>,
    pub math: bool,
}

impl Default for BarLabelOptions {
    fn default() -> Self {
        Self {
            font_size: 24.0,
            buff: 0.25,
            color: None,
            math: false,
        }
    }
}

struct Label {
    text: crate::latex_authoring::LatexAdmission,
    target: Option<Bounds2D64>,
    direction: f64,
}

pub(crate) struct PreparedBarLabels {
    labels: Vec<Label>,
    buff: f64,
}

impl PreparedBarLabels {
    pub(crate) fn prepare(
        chart: &ManimBarChart,
        backend: &mut impl LatexBackend,
        options: &BarLabelOptions,
        mut capture: impl FnMut(
            &crate::Mobject,
        ) -> Result<SemanticObjectState, CoordinateAuthoringError>,
    ) -> Result<Self, Error> {
        if !options.buff.is_finite() || !options.font_size.is_finite() || options.font_size <= 0.0 {
            return Err(AuthoringError::NonFiniteTransform.into());
        }
        let nodes = direct_bar_nodes(chart.bars())?;
        let store = chart.family().integration_store();
        let mut labels = Vec::with_capacity(nodes.len());
        for node in nodes {
            let object = crate::Mobject::from_node(Rc::clone(store), node)?;
            let state = capture(&object)?;
            let value = bar_metadata(&state)?.value;
            let target = crate::semantic_mobject::boundary_for_content(
                &store.borrow(),
                state.content,
                state.transform,
            )?;
            let color = options.color.unwrap_or_else(|| match state.style.fill {
                Some(SemanticPaint::Solid(color)) => color,
                _ => Color::WHITE,
            });
            let source = value.to_string();
            let text = if options.math {
                crate::latex_authoring::prepare_math_tex(
                    crate::MathTex::new(source)?
                        .with_font_size(options.font_size)
                        .color(color),
                    backend,
                )?
            } else {
                crate::latex_authoring::prepare_tex(
                    crate::Tex::new(source)?
                        .with_font_size(options.font_size)
                        .color(color),
                    backend,
                )?
            };
            labels.push(Label {
                text,
                target,
                direction: if value >= 0.0 { 1.0 } else { -1.0 },
            });
        }
        Ok(Self {
            labels,
            buff: options.buff,
        })
    }

    pub(crate) fn prepare_names(
        names: &[String],
        values: &[f64],
        frame: crate::NumberLineFrame,
        font_size: f32,
        backend: &mut impl LatexBackend,
    ) -> Result<Self, Error> {
        if names.len() > values.len() {
            return Err(
                CoordinateAuthoringError::InvalidOptions("bar names exceed bar count").into(),
            );
        }
        if !font_size.is_finite() || font_size <= 0.0 {
            return Err(AuthoringError::NonFiniteTransform.into());
        }
        let mut labels = Vec::with_capacity(names.len());
        for (index, source) in names.iter().enumerate() {
            let point = frame
                .number_to_point(index as f64 + 0.5)
                .map_err(CoordinateAuthoringError::from)?;
            let text = crate::latex_authoring::prepare_tex(
                crate::Tex::new(source.as_str())?.with_font_size(font_size),
                backend,
            )?;
            labels.push(Label {
                text,
                target: Some(Bounds2D64::point(point[0], point[1])),
                direction: if values[index] < 0.0 { 1.0 } else { -1.0 },
            });
        }
        Ok(Self { labels, buff: 0.25 })
    }

    pub(crate) fn publish_into(
        self,
        store: &mut SemanticStore,
        parent: Option<SemanticTransactionNodeRef>,
        mut transaction: SemanticMutationTransaction,
        publish: impl FnOnce(
            &mut SemanticStore,
            SemanticMutationTransaction,
        ) -> Result<SemanticMutationTransactionResult, TextAuthoringError>,
    ) -> Result<(SemanticMutationTransactionResult, noon_core::SemanticNodeId), TextAuthoringError>
    {
        let mut dependencies = Vec::with_capacity(self.labels.len());
        let mut placements = Vec::with_capacity(self.labels.len());
        for label in self.labels {
            let (identity, resource, fonts, geometry, transform, style, font_size) =
                label.text.into_compiled_resource_parts_with_presentation();
            let baseline = crate::latex_authoring::latex_presentation_baseline(
                &resource, transform, font_size,
            )?;
            dependencies.push((identity, resource, fonts, geometry));
            placements.push((transform, style, label.target, label.direction, baseline));
        }
        let mut root = None;
        let result = store.with_compiled_text_dependency_batch::<TextAuthoringError, _>(
            dependencies,
            |store, handles| {
                handles
                    .iter()
                    .map(|handle| {
                        store
                            .text_resources()
                            .get(*handle)
                            .cloned()
                            .ok_or(TextAuthoringError::MissingGeometryResource)
                    })
                    .collect()
            },
            |store, handles| {
                let family = transaction.create_node(SemanticNodeCreation::family());
                root = Some(family);
                for (handle, (transform, style, target, direction, baseline)) in
                    handles.iter().zip(placements)
                {
                    let mut state = SemanticObjectState::new(*handle);
                    state.transform = transform;
                    state.style = style;
                    state.set_text_presentation_baseline(baseline);
                    let bounds = crate::semantic_mobject::boundary_for_content(
                        store,
                        state.content,
                        state.transform,
                    )
                    .map_err(TextAuthoringError::Semantic)?;
                    let delta = crate::family_layout::RelativePlacement::Next(ManimNextToArgs {
                        direction: (0.0, direction),
                        buff: self.buff,
                        aligned_edge: (0.0, 0.0),
                        mask: (1.0, 1.0),
                    })
                    .delta::<AuthoringError>(bounds, |x, y| {
                        Ok(crate::family_layout::bounds_critical_point(target, x, y))
                    })
                    .map_err(TextAuthoringError::Semantic)?;
                    state.transform.translation.x += delta.0;
                    state.transform.translation.y += delta.1;
                    let leaf = transaction.create_node(SemanticNodeCreation::object(state));
                    transaction.add_member(family, leaf);
                }
                if let Some(parent) = parent {
                    transaction.add_member(parent, family);
                }
                publish(store, transaction)
            },
        )?;
        let node = result
            .resolve(root.expect("label family"))
            .ok_or(AuthoringError::UnresolvedCreatedNode(
                root.expect("label family"),
            ))
            .map_err(TextAuthoringError::Semantic)?;
        Ok((result, node))
    }
}

impl ManimBarChart {
    /// Construct detached retained Tex or MathTex labels from current bar state.
    pub fn get_bar_labels(
        &self,
        backend: &mut impl LatexBackend,
        options: &BarLabelOptions,
    ) -> Result<MobjectFamily, Error> {
        let prepared = PreparedBarLabels::prepare(self, backend, options, |object| {
            object.state().map_err(Into::into)
        })?;
        let store = Rc::clone(self.family().integration_store());
        let (_, node) = prepared.publish_into(
            &mut store.borrow_mut(),
            None,
            SemanticMutationTransaction::new(),
            |store, transaction| {
                transaction
                    .apply(store)
                    .map_err(AuthoringError::from)
                    .map_err(TextAuthoringError::Semantic)
            },
        )?;
        MobjectFamily::from_node(store, node).map_err(Into::into)
    }
}
