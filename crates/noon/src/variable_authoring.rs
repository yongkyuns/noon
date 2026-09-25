//! Atomic shared Variable composition over retained LaTeX and numeric resources.

use crate::{
    AuthoringError, DecimalNumber, ExecutionSession, LatexBackend, LatexParts, MathTex, Mobject,
    MobjectFamily, NumericAuthoringError, TextAuthoringError, ValueTracker,
};
use noon_core::{
    Bounds2D64, DecimalFormat, SemanticMutationTransaction, SemanticNodeCreation, SemanticStore,
};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone)]
pub struct Variable {
    family: MobjectFamily,
    label: LatexParts,
    equals: Mobject,
    value: DecimalNumber,
    tracker: ValueTracker,
}

impl Variable {
    pub fn family(&self) -> &MobjectFamily {
        &self.family
    }
    pub fn label(&self) -> &LatexParts {
        &self.label
    }
    pub fn equals(&self) -> Result<Mobject, AuthoringError> {
        self.equals.validate()?;
        Ok(self.equals.clone())
    }
    pub fn value(&self) -> &DecimalNumber {
        &self.value
    }
    pub fn tracker(&self) -> &ValueTracker {
        &self.tracker
    }
}

#[derive(Debug)]
pub enum VariableAuthoringError {
    Text(TextAuthoringError),
    Numeric(NumericAuthoringError),
    Authoring(AuthoringError),
}
impl std::fmt::Display for VariableAuthoringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(error) => error.fmt(f),
            Self::Numeric(error) => error.fmt(f),
            Self::Authoring(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for VariableAuthoringError {}
impl From<TextAuthoringError> for VariableAuthoringError {
    fn from(error: TextAuthoringError) -> Self {
        Self::Text(error)
    }
}
impl From<NumericAuthoringError> for VariableAuthoringError {
    fn from(error: NumericAuthoringError) -> Self {
        Self::Numeric(error)
    }
}
impl From<AuthoringError> for VariableAuthoringError {
    fn from(error: AuthoringError) -> Self {
        Self::Authoring(error)
    }
}

pub(crate) fn variable_live_error(error: VariableAuthoringError) -> crate::LiveSessionError {
    match error {
        VariableAuthoringError::Text(error) => crate::LiveSessionError::Text(error),
        VariableAuthoringError::Numeric(error) => {
            crate::numeric_authoring::numeric_live_error(error)
        }
        VariableAuthoringError::Authoring(error) => crate::LiveSessionError::from(error),
    }
}

struct PublishedVariable {
    result: noon_core::SemanticMutationTransactionResult,
    signal: noon_core::SemanticLocalNodeToken,
    label_family: noon_core::SemanticLocalNodeToken,
    label_members: Vec<noon_core::SemanticLocalNodeToken>,
    equals: noon_core::SemanticLocalNodeToken,
    value: noon_core::SemanticLocalNodeToken,
    family: noon_core::SemanticLocalNodeToken,
}

/// Compile, arrange, and publish every Variable component through one resource
/// rollback scope and one semantic transaction.
#[allow(clippy::too_many_arguments)] // One resource and semantic-transaction admission boundary.
pub(crate) fn construct_variable(
    store: &Rc<RefCell<SemanticStore>>,
    root: noon_core::SemanticNodeId,
    execution: Option<&mut ExecutionSession>,
    backend: &mut impl LatexBackend,
    label_source: String,
    initial: f64,
    format: DecimalFormat,
    font_size: f32,
) -> Result<Variable, VariableAuthoringError> {
    let label = crate::latex_authoring::prepare_math_tex(
        MathTex::new(label_source)?.with_font_size(font_size),
        backend,
    )?;
    let (
        label_identity,
        label_resource,
        label_fonts,
        label_geometry,
        label_transform,
        label_style,
        label_font_size,
    ) = label.into_compiled_resource_parts_with_presentation();
    let label_baseline = crate::latex_authoring::latex_presentation_baseline(
        &label_resource,
        label_transform,
        label_font_size,
    )?;
    let (label_source, label_parts) = crate::latex_authoring::text_resource_parts(&label_resource)?;
    let equals = crate::latex_authoring::prepare_math_tex(
        MathTex::new("=")?.with_font_size(font_size),
        backend,
    )?;
    let (
        equals_identity,
        equals_resource,
        equals_fonts,
        equals_geometry,
        equals_transform,
        equals_style,
        equals_font_size,
    ) = equals.into_compiled_resource_parts_with_presentation();
    let equals_baseline = crate::latex_authoring::latex_presentation_baseline(
        &equals_resource,
        equals_transform,
        equals_font_size,
    )?;
    let (_, equals_parts) = crate::latex_authoring::text_resource_parts(&equals_resource)?;
    let number = crate::numeric_authoring::PreparedDecimalValue::prepare(
        backend,
        initial,
        format.clone(),
        font_size,
    )?;
    let binding =
        crate::numeric_authoring::PreparedNumericBinding::prepare(backend, &format, font_size)?;

    let mut dependencies = vec![
        (label_identity, label_resource, label_fonts, label_geometry),
        (
            equals_identity,
            equals_resource,
            equals_fonts,
            equals_geometry,
        ),
    ];
    let number_range = dependencies.len()..dependencies.len() + number.dependencies().len();
    dependencies.extend(number.dependencies().iter().cloned());
    let binding_range = dependencies.len()..dependencies.len() + binding.dependencies().len();
    dependencies.extend(binding.dependencies().iter().cloned());

    let captured_binding_handles = RefCell::new(None);
    let published = store
        .borrow_mut()
        .with_compiled_text_dependency_batch::<TextAuthoringError, _>(
            dependencies,
            |semantic, handles| {
                *captured_binding_handles.borrow_mut() =
                    Some(handles[binding_range.clone()].to_vec());
                let label_resource = semantic
                    .text_resources()
                    .get(handles[0])
                    .ok_or(TextAuthoringError::MissingGeometryResource)?;
                let mut derived = label_parts
                    .iter()
                    .map(|part| {
                        label_resource
                            .projected_part(part, semantic.geometry_resources())
                            .map_err(TextAuthoringError::from)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let equals_resource = semantic
                    .text_resources()
                    .get(handles[1])
                    .ok_or(TextAuthoringError::MissingGeometryResource)?;
                derived.extend(
                    equals_parts
                        .iter()
                        .map(|part| {
                            equals_resource
                                .projected_part(part, semantic.geometry_resources())
                                .map_err(TextAuthoringError::from)
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                );
                derived.push(number.compose_resource(semantic, &handles[number_range.clone()])?);
                Ok(derived)
            },
            |semantic, handles| {
                let label_count = label_parts.len();
                let mut label_states = handles[..label_count]
                    .iter()
                    .map(|handle| {
                        crate::latex_authoring::semantic_text_state(
                            *handle,
                            label_transform,
                            label_style.clone(),
                            Some(label_baseline),
                        )
                    })
                    .collect::<Vec<_>>();
                let equals_end = label_count + equals_parts.len();
                let mut equals_states = handles[label_count..equals_end]
                    .iter()
                    .map(|handle| {
                        crate::latex_authoring::semantic_text_state(
                            *handle,
                            equals_transform,
                            equals_style.clone(),
                            Some(equals_baseline),
                        )
                    })
                    .collect::<Vec<_>>();
                let mut number_state = number.decimal_state(
                    semantic,
                    handles[equals_end],
                    noon_core::SemanticTransform2_5D::default(),
                )?;
                arrange_variable_states(
                    semantic,
                    &label_states,
                    &mut equals_states,
                    &mut number_state,
                )?;
                label_states.extend(equals_states);

                let mut transaction = SemanticMutationTransaction::new();
                let signal = transaction.create_node(
                    SemanticNodeCreation::input_signal(initial)
                        .map_err(AuthoringError::from)
                        .map_err(TextAuthoringError::Semantic)?,
                );
                transaction.scope_signal(root, signal);
                let label_family = transaction.create_node(SemanticNodeCreation::family());
                let label_members = label_states
                    .into_iter()
                    .map(|state| {
                        let member = transaction.create_node(SemanticNodeCreation::object(state));
                        transaction.add_member(label_family, member);
                        member
                    })
                    .collect::<Vec<_>>();
                let equals = label_members[label_count];
                let value = transaction.create_node(SemanticNodeCreation::object(number_state));
                let family = transaction.create_node(SemanticNodeCreation::family());
                transaction.add_member(family, label_family);
                transaction.add_member(family, value);

                let prepared = transaction
                    .prepare(semantic)
                    .map_err(AuthoringError::from)
                    .map_err(TextAuthoringError::Semantic)?;
                let signal_id = prepared
                    .planned_node_id(signal)
                    .expect("fresh Variable signal survives preflight");
                let metadata = number.decimal_number().with_binding(
                    binding.binding(
                        signal_id,
                        &captured_binding_handles
                            .borrow_mut()
                            .take()
                            .expect("binding handles captured during composition"),
                    ),
                );
                let prepared = prepared
                    .with_decimal_number(value, metadata)
                    .map_err(AuthoringError::from)
                    .map_err(TextAuthoringError::Semantic)?;
                let result = match execution {
                    Some(execution) => execution
                        .publish_prepared_scoped_value_tracker(prepared, root, signal, initial)
                        .map_err(AuthoringError::from)
                        .map_err(TextAuthoringError::Semantic)?,
                    None => prepared.commit(),
                };
                Ok(PublishedVariable {
                    result,
                    signal,
                    label_family,
                    label_members,
                    equals,
                    value,
                    family,
                })
            },
        )?;

    let resolve = |token| {
        published
            .result
            .resolve(token)
            .ok_or(AuthoringError::UnresolvedCreatedNode(token))
    };
    let label = crate::latex_authoring::finish_latex_parts(
        Rc::clone(store),
        &published.result,
        published.label_family,
        published.label_members,
        label_source,
        label_parts,
    )?;
    let equals = Mobject::from_node(Rc::clone(store), resolve(published.equals)?)?;
    let value = DecimalNumber::from_mobject(Mobject::from_node(
        Rc::clone(store),
        resolve(published.value)?,
    )?)?;
    Ok(Variable {
        family: MobjectFamily::from_node(Rc::clone(store), resolve(published.family)?)?,
        label,
        equals,
        value,
        tracker: ValueTracker::from_semantic_node(Rc::clone(store), resolve(published.signal)?),
    })
}

fn arrange_variable_states(
    store: &SemanticStore,
    label: &[noon_core::SemanticObjectState],
    equals: &mut [noon_core::SemanticObjectState],
    value: &mut noon_core::SemanticObjectState,
) -> Result<(), TextAuthoringError> {
    let label_bounds = states_bounds(store, label)?;
    let equals_bounds = states_bounds(store, equals)?;
    let shifted_equals_bounds = shift_next_to(label_bounds, equals_bounds, equals);

    let mut complete_label_bounds = Some(label_bounds);
    union_bounds(&mut complete_label_bounds, Some(shifted_equals_bounds));
    let complete_label_bounds = complete_label_bounds.expect("label bounds were initialized");
    let value_bounds = state_bounds(store, value)?;
    shift_next_to(
        complete_label_bounds,
        value_bounds,
        std::slice::from_mut(value),
    );
    Ok(())
}

fn state_bounds(
    store: &SemanticStore,
    state: &noon_core::SemanticObjectState,
) -> Result<Bounds2D64, TextAuthoringError> {
    Ok(
        crate::semantic_mobject::boundary_for_content(store, state.content, state.transform)
            .map_err(TextAuthoringError::Semantic)?
            .unwrap_or_else(|| Bounds2D64::point(0.0, 0.0)),
    )
}

fn states_bounds(
    store: &SemanticStore,
    states: &[noon_core::SemanticObjectState],
) -> Result<Bounds2D64, TextAuthoringError> {
    let mut bounds = None;
    for state in states {
        union_bounds(&mut bounds, Some(state_bounds(store, state)?));
    }
    Ok(bounds.unwrap_or_else(|| Bounds2D64::point(0.0, 0.0)))
}

fn shift_next_to(
    anchor: Bounds2D64,
    moving: Bounds2D64,
    states: &mut [noon_core::SemanticObjectState],
) -> Bounds2D64 {
    let shift_x = anchor.max_x + f64::from(crate::DEFAULT_MOBJECT_TO_MOBJECT_BUFFER) - moving.min_x;
    let shift_y = (anchor.min_y + anchor.max_y - moving.min_y - moving.max_y) * 0.5;
    for state in states {
        state.transform.translation.x += shift_x;
        state.transform.translation.y += shift_y;
    }
    translated_bounds(moving, shift_x, shift_y)
}

fn translated_bounds(bounds: Bounds2D64, x: f64, y: f64) -> Bounds2D64 {
    Bounds2D64 {
        min_x: bounds.min_x + x,
        min_y: bounds.min_y + y,
        max_x: bounds.max_x + x,
        max_y: bounds.max_y + y,
    }
}

fn union_bounds(target: &mut Option<Bounds2D64>, value: Option<Bounds2D64>) {
    let Some(value) = value else { return };
    match target {
        Some(target) => {
            target.include(value.min_x, value.min_y);
            target.include(value.max_x, value.max_y);
        }
        None => *target = Some(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::TextResourceLookup;

    struct RuleBackend {
        compilations: usize,
        fail_after: Option<usize>,
    }

    impl RuleBackend {
        fn working() -> Self {
            Self {
                compilations: 0,
                fail_after: None,
            }
        }
    }

    impl LatexBackend for RuleBackend {
        fn identity(&self) -> &str {
            "atomic-variable-rule-fixture"
        }
        fn format(&self) -> crate::LatexFormat {
            crate::LatexFormat::Preloaded
        }
        fn font(&mut self, _: &str) -> Result<crate::DviFontResource, String> {
            Err("font-free fixture".into())
        }
        fn compile(&mut self, _: &str) -> Result<Vec<u8>, String> {
            if self
                .fail_after
                .is_some_and(|limit| self.compilations >= limit)
            {
                return Err("planned compiler failure".into());
            }
            self.compilations += 1;
            let mut dvi = vec![247, 2];
            for value in [25_400_000u32, 473_628_672, 1000] {
                dvi.extend(value.to_be_bytes());
            }
            dvi.push(0);
            dvi.push(139);
            dvi.extend([0; 44]);
            dvi.push(132);
            dvi.extend(655_360i32.to_be_bytes());
            dvi.extend(327_680i32.to_be_bytes());
            dvi.push(140);
            dvi.push(248);
            dvi.extend([0; 28]);
            dvi.push(249);
            dvi.extend([0; 4]);
            dvi.push(2);
            dvi.extend([223; 4]);
            Ok(dvi)
        }
    }

    #[test]
    fn compiler_failure_after_label_publishes_nothing() {
        let mut scene = crate::Scene::new();
        let revision = scene.integration_store().borrow().scene_revision();
        let resources = scene.integration_store().borrow().text_resources().stats();
        let root_members = scene
            .integration_store()
            .borrow()
            .node(scene.root())
            .unwrap()
            .members()
            .to_vec();
        let mut backend = RuleBackend {
            compilations: 0,
            fail_after: Some(1),
        };

        assert!(scene
            .variable(&mut backend, "x", 1.25, DecimalFormat::default(), 48.0,)
            .is_err());
        let store = scene.integration_store().borrow();
        assert_eq!(store.scene_revision(), revision);
        assert_eq!(store.text_resources().stats(), resources);
        assert_eq!(store.node(scene.root()).unwrap().members(), root_members);
    }

    #[test]
    fn live_variable_constructs_after_wait_and_tracks_without_authored_text_edits() {
        let scene = crate::Scene::new();
        let mut session = scene.execution_session().unwrap();
        {
            let mut live = scene.live(&mut session);
            let wait = live.wait_segment(1.0).unwrap();
            live.advance_segment_to(wait, wait.end_time()).unwrap();
            live.complete_segment(wait).unwrap();
        }
        let before = session.publication_context();
        let mut backend = RuleBackend::working();
        let variable = scene
            .live(&mut session)
            .create_variable(&mut backend, "x", 1.25, DecimalFormat::default(), 48.0)
            .unwrap();
        let label_members = variable.label().current_members().unwrap();
        assert_eq!(label_members.len(), 2);
        assert_eq!(
            variable.equals().unwrap().node_id(),
            label_members[1].node_id()
        );
        assert_eq!(
            label_members[0].state().unwrap().transform.translation,
            noon_core::SemanticVec3::default(),
            "Variable preserves the independently centered label position"
        );
        let label_bounds = label_members[0].layout_bounds().unwrap().unwrap();
        let equals_bounds = label_members[1].layout_bounds().unwrap().unwrap();
        let value_bounds = variable.value().mobject().layout_bounds().unwrap().unwrap();
        let buffer = f64::from(crate::DEFAULT_MOBJECT_TO_MOBJECT_BUFFER);
        assert!((equals_bounds.min_x - label_bounds.max_x - buffer).abs() < 1.0e-9);
        assert!((value_bounds.min_x - equals_bounds.max_x - buffer).abs() < 1.0e-9);
        assert!(
            ((label_bounds.min_y + label_bounds.max_y - equals_bounds.min_y - equals_bounds.max_y)
                * 0.5)
                .abs()
                < 1.0e-9
        );
        assert_eq!(
            session.publication_context().scene_revision(),
            before.scene_revision().checked_next().unwrap()
        );
        assert_eq!(session.effective_text_resource_stats().live_resources, 0);

        scene
            .live(&mut session)
            .add_many(&[crate::MobjectTarget::Family(variable.family())])
            .unwrap();
        assert_eq!(session.effective_text_resource_stats().live_resources, 1);
        let authored_revision = session.publication_context().scene_revision();
        session
            .set_reactive_input(variable.tracker().node_id(), 7.5_f32)
            .unwrap();
        assert_eq!(
            session.publication_context().scene_revision(),
            authored_revision
        );
        let store = scene.integration_store().borrow();
        let effective = session
            .effective_semantic_object(&store, variable.value().mobject().node_id())
            .unwrap();
        let handle = effective.object.text().unwrap();
        assert_eq!(
            session
                .text_resources()
                .get(handle)
                .unwrap()
                .source
                .as_ref(),
            "7.50"
        );
    }
}
