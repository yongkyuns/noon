//! Resource-backed DecimalNumber and Integer over shared MathTex semantics.

use crate::{
    format_decimal, DecimalFormat, LatexBackend, MathTex, Mobject, NumericFormatError,
    TextAuthoringError, ValueTracker,
};
use noon_core::{
    compose_numeric_text_resource as compose_numeric_resource, numeric_text_layout,
    NumericTextLayoutToken, NumericTextResourceError, SemanticDecimalNumber,
    SemanticMutationTransaction, SemanticNodeCreation, SemanticNumericTextBinding,
    SemanticObjectContent, SemanticObjectState, SemanticPaint, SemanticStore,
    SemanticTransform2_5D, TextPresentationBaseline, TextResource, WHITE,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[derive(Clone, Debug, PartialEq)]
pub enum NumericAuthoringError {
    Format(NumericFormatError),
    Text(TextAuthoringError),
    Semantic(crate::AuthoringError),
    NotDecimalNumber,
    MissingPresentationBaseline,
    InvalidEffectiveFontSize { value: f64 },
    MissingEffectiveNumericSignal { signal: noon_core::SemanticNodeId },
    NonScalarEffectiveNumericSignal { signal: noon_core::SemanticNodeId },
    IntegerOutOfRange { value: f64 },
}
impl std::fmt::Display for NumericAuthoringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Format(error) => error.fmt(f),
            Self::Text(error) => error.fmt(f),
            Self::Semantic(error) => error.fmt(f),
            Self::NotDecimalNumber => f.write_str("semantic object is not a DecimalNumber"),
            Self::MissingPresentationBaseline => {
                f.write_str("DecimalNumber has no retained text presentation baseline")
            }
            Self::InvalidEffectiveFontSize { value } => {
                write!(f, "DecimalNumber effective font size is invalid: {value}")
            }
            Self::MissingEffectiveNumericSignal { signal } => {
                write!(
                    f,
                    "DecimalNumber bound signal {signal:?} has no effective value"
                )
            }
            Self::NonScalarEffectiveNumericSignal { signal } => {
                write!(f, "DecimalNumber bound signal {signal:?} is not scalar")
            }
            Self::IntegerOutOfRange { value } => {
                write!(f, "Integer value {value} is outside i64 range")
            }
        }
    }
}
impl std::error::Error for NumericAuthoringError {}
impl From<NumericFormatError> for NumericAuthoringError {
    fn from(value: NumericFormatError) -> Self {
        Self::Format(value)
    }
}
impl From<TextAuthoringError> for NumericAuthoringError {
    fn from(value: TextAuthoringError) -> Self {
        Self::Text(value)
    }
}
impl From<crate::AuthoringError> for NumericAuthoringError {
    fn from(value: crate::AuthoringError) -> Self {
        Self::Semantic(value)
    }
}

pub(crate) fn numeric_live_error(error: NumericAuthoringError) -> crate::LiveSessionError {
    match error {
        NumericAuthoringError::Text(TextAuthoringError::Semantic(
            crate::AuthoringError::ExecutionPublication(error),
        )) => crate::LiveSessionError::Publication(error),
        NumericAuthoringError::Text(error) => crate::LiveSessionError::Text(error),
        NumericAuthoringError::Semantic(error) => crate::LiveSessionError::from(error),
        other => crate::LiveSessionError::Mobject(other.to_string()),
    }
}

/// Shared Manim `Integer.get_value` conversion: ties go to the even integer
/// and out-of-range values are rejected rather than saturated.
pub fn integer_value(value: f64) -> Result<i64, NumericAuthoringError> {
    if !value.is_finite() {
        return Err(NumericAuthoringError::IntegerOutOfRange { value });
    }
    let value = value.round_ties_even();
    if value < i64::MIN as f64 || value >= 9_223_372_036_854_775_808.0 {
        return Err(NumericAuthoringError::IntegerOutOfRange { value });
    }
    Ok(value as i64)
}

#[derive(Clone, Debug)]
pub struct DecimalNumber {
    object: Mobject,
}

/// One DecimalNumber display prepared from individually compiled MathTex parts.
/// The immutable glyph dependencies can be admitted once and composed through
/// the same transaction owner for cold and live authoring.
pub(crate) struct PreparedDecimalValue {
    source: Arc<str>,
    tokens: Vec<NumericTextLayoutToken>,
    dependencies: Vec<NumericCompiledDependency>,
    value: f64,
    format: DecimalFormat,
    font_size: f32,
}

pub(crate) struct PreparedNumericBinding {
    token_sources: Vec<Arc<str>>,
    dependencies: Vec<NumericCompiledDependency>,
}

pub(crate) type NumericCompiledDependency = (
    noon_core::TextCompilationIdentity,
    TextResource,
    noon_core::FontResourceArena,
    noon_core::GeometryResourceArena,
);

type NumericLayoutToken = NumericTextLayoutToken;

impl DecimalNumber {
    pub fn new(
        store: Rc<RefCell<SemanticStore>>,
        backend: &mut impl LatexBackend,
        value: f64,
        format: DecimalFormat,
    ) -> Result<Self, NumericAuthoringError> {
        Self::with_font_size(store, backend, value, format, 48.0)
    }

    pub fn with_font_size(
        store: Rc<RefCell<SemanticStore>>,
        backend: &mut impl LatexBackend,
        value: f64,
        format: DecimalFormat,
        font_size: f32,
    ) -> Result<Self, NumericAuthoringError> {
        Self::construct_with(
            store,
            backend,
            value,
            format,
            font_size,
            |store, transaction| {
                transaction
                    .apply(store)
                    .map_err(crate::AuthoringError::from)
                    .map_err(TextAuthoringError::Semantic)
            },
        )
    }

    pub(crate) fn construct_with(
        store: Rc<RefCell<SemanticStore>>,
        backend: &mut impl LatexBackend,
        value: f64,
        format: DecimalFormat,
        font_size: f32,
        publish: impl FnOnce(
            &mut SemanticStore,
            SemanticMutationTransaction,
        )
            -> Result<noon_core::SemanticMutationTransactionResult, TextAuthoringError>,
    ) -> Result<Self, NumericAuthoringError> {
        let prepared = prepare_numeric_value(backend, value, format.clone(), font_size)?;
        let number = decimal_metadata(value, &format, font_size);
        let result = prepared.publish(&mut store.borrow_mut(), move |semantic, handle| {
            let mut state = SemanticObjectState::new(handle);
            state.style.fill = Some(SemanticPaint::Solid(WHITE));
            state.set_decimal_number(Some(number));
            state.set_text_presentation_baseline(numeric_presentation_baseline(
                semantic, handle, font_size,
            )?);
            let mut transaction = SemanticMutationTransaction::new();
            transaction.add_node(SemanticNodeCreation::object(state));
            publish(semantic, transaction)
        })?;
        let [noon_core::SemanticMutationImpact::NodeAdded { node }] = result.impacts() else {
            unreachable!("numeric admission adds one node")
        };
        Ok(Self {
            object: Mobject::from_node(store, *node)?,
        })
    }

    pub fn from_mobject(object: Mobject) -> Result<Self, NumericAuthoringError> {
        if object.state()?.decimal_number().is_some() {
            Ok(Self { object })
        } else {
            Err(NumericAuthoringError::NotDecimalNumber)
        }
    }
    pub fn mobject(&self) -> &Mobject {
        &self.object
    }
    fn metadata(&self) -> Result<SemanticDecimalNumber, NumericAuthoringError> {
        self.object
            .state()?
            .decimal_number()
            .cloned()
            .ok_or(NumericAuthoringError::NotDecimalNumber)
    }
    /// Read the retained authored/base value. A tracker binding never mutates this
    /// declaration; use the live-session query for the current runtime value.
    pub fn value(&self) -> Result<f64, NumericAuthoringError> {
        Ok(self.metadata()?.value())
    }

    /// Read the coherent current value from this number's declared tracker binding.
    /// Unbound numbers retain their authored/base value in every execution.
    pub(crate) fn current_value(
        &self,
        execution: &crate::ExecutionSession,
    ) -> Result<f64, NumericAuthoringError> {
        let metadata = self.metadata()?;
        let Some(binding) = metadata.binding() else {
            return Ok(metadata.value());
        };
        match execution.effective_signal_value(binding.signal()) {
            Some(ReactiveValue::Scalar(value)) => Ok(f64::from(*value)),
            Some(_) => Err(NumericAuthoringError::NonScalarEffectiveNumericSignal {
                signal: binding.signal(),
            }),
            None => Err(NumericAuthoringError::MissingEffectiveNumericSignal {
                signal: binding.signal(),
            }),
        }
    }
    pub fn format(&self) -> Result<DecimalFormat, NumericAuthoringError> {
        let number = self.metadata()?;
        Ok(DecimalFormat {
            decimal_places: number.decimal_places(),
            include_sign: number.include_sign(),
            group_with_commas: number.group_with_commas(),
            show_ellipsis: number.show_ellipsis(),
            unit: number.unit().map(str::to_owned),
        })
    }
    pub fn text(&self) -> Result<String, NumericAuthoringError> {
        Ok(format_decimal(self.value()?, &self.format()?)?)
    }

    /// Manim's current font-size observation derives from effective ink height
    /// and the receiver-owned initial presentation baseline.
    pub fn font_size(&self) -> Result<f64, NumericAuthoringError> {
        let state = self.object.state()?;
        effective_font_size(&self.object.integration_store().borrow(), &state)
    }

    pub(crate) fn font_size_at(
        &self,
        state: &SemanticObjectState,
    ) -> Result<f64, NumericAuthoringError> {
        effective_font_size(&self.object.integration_store().borrow(), state)
    }

    pub fn set_value(
        &mut self,
        backend: &mut impl LatexBackend,
        value: f64,
    ) -> Result<&mut Self, NumericAuthoringError> {
        let authored = self.object.state()?;
        self.publish_value(
            self.prepare_value(backend, value)?,
            authored.clone(),
            authored,
            |store, transaction| {
                transaction
                    .apply(store)
                    .map_err(crate::AuthoringError::from)
                    .map_err(TextAuthoringError::Semantic)
            },
        )?;
        Ok(self)
    }
    pub fn increment_value(
        &mut self,
        backend: &mut impl LatexBackend,
        delta: f64,
    ) -> Result<&mut Self, NumericAuthoringError> {
        self.set_value(backend, self.value()? + delta)
    }

    /// Bind this retained number to a shared scalar tracker.
    ///
    /// Every glyph/token resource needed by the configured format is compiled and
    /// admitted now. Runtime updates therefore perform only formatting, retained
    /// composition and one effective resource publication for this object.
    pub fn bind_to_tracker(
        &mut self,
        backend: &mut impl LatexBackend,
        tracker: &ValueTracker,
    ) -> Result<&mut Self, NumericAuthoringError> {
        self.bind_to_tracker_with(backend, tracker, |store, transaction| {
            transaction
                .apply(store)
                .map_err(crate::AuthoringError::from)
                .map_err(TextAuthoringError::Semantic)
        })?;
        Ok(self)
    }

    pub(crate) fn bind_to_tracker_live(
        &self,
        backend: &mut impl LatexBackend,
        tracker: &ValueTracker,
        publish: impl FnOnce(
            &mut SemanticStore,
            SemanticMutationTransaction,
        )
            -> Result<noon_core::SemanticMutationTransactionResult, TextAuthoringError>,
    ) -> Result<(), NumericAuthoringError> {
        self.bind_to_tracker_with(backend, tracker, publish)
    }

    fn bind_to_tracker_with(
        &self,
        backend: &mut impl LatexBackend,
        tracker: &ValueTracker,
        publish: impl FnOnce(
            &mut SemanticStore,
            SemanticMutationTransaction,
        )
            -> Result<noon_core::SemanticMutationTransactionResult, TextAuthoringError>,
    ) -> Result<(), NumericAuthoringError> {
        tracker.require_store(self.object.integration_store())?;
        let metadata = self.metadata()?;
        let format = self.format()?;
        let font_size = metadata.font_size();
        let prepared_binding = PreparedNumericBinding::prepare(backend, &format, font_size)?;

        let node = self.object.node_id();
        let signal = tracker.node_id();
        let store = self.object.integration_store();
        let captured = RefCell::new(None::<Vec<noon_core::TextResourceHandle>>);
        store
            .borrow_mut()
            .with_compiled_text_dependency_batch::<TextAuthoringError, _>(
                prepared_binding.dependencies.clone(),
                |_store, handles| {
                    *captured.borrow_mut() = Some(handles.to_vec());
                    Ok(Vec::new())
                },
                |store, _derived| {
                    let handles = captured
                        .borrow_mut()
                        .take()
                        .expect("numeric template composition captures dependency handles");
                    let binding = prepared_binding.binding(signal, &handles);
                    let number = metadata.clone().with_binding(binding);
                    // The current authored value remains the DecimalNumber base value.
                    debug_assert_eq!(number.value(), metadata.value());
                    let mut transaction = SemanticMutationTransaction::new();
                    transaction.replace_decimal_number(node, number);
                    publish(store, transaction).map(|_| ())
                },
            )?;
        Ok(())
    }

    pub(crate) fn set_value_live(
        &self,
        backend: &mut impl LatexBackend,
        value: f64,
        authored: noon_core::SemanticObjectState,
        effective: noon_core::SemanticObjectState,
        publish: impl FnOnce(
            &mut SemanticStore,
            SemanticMutationTransaction,
        )
            -> Result<noon_core::SemanticMutationTransactionResult, TextAuthoringError>,
    ) -> Result<(), NumericAuthoringError> {
        self.publish_value(
            self.prepare_value(backend, value)?,
            authored,
            effective,
            publish,
        )
    }

    fn prepare_value(
        &self,
        backend: &mut impl LatexBackend,
        value: f64,
    ) -> Result<PreparedDecimalValue, NumericAuthoringError> {
        let metadata = self.metadata()?;
        prepare_numeric_value(backend, value, self.format()?, metadata.font_size())
    }

    fn publish_value(
        &self,
        prepared: PreparedDecimalValue,
        authored: SemanticObjectState,
        effective: SemanticObjectState,
        publish: impl FnOnce(
            &mut SemanticStore,
            SemanticMutationTransaction,
        )
            -> Result<noon_core::SemanticMutationTransactionResult, TextAuthoringError>,
    ) -> Result<(), NumericAuthoringError> {
        let value = prepared.value;
        let format = prepared.format.clone();
        let font_size = prepared.font_size;
        let node = self.object.node_id();
        let store = self.object.integration_store();
        prepared.publish(&mut store.borrow_mut(), |store, handle| {
            let transaction = decimal_replacement_transaction(
                store, node, &authored, effective, handle, value, &format, font_size,
            )?;
            publish(store, transaction)
        })?;
        Ok(())
    }
}

impl PreparedNumericBinding {
    pub(crate) fn prepare(
        backend: &mut impl LatexBackend,
        format: &DecimalFormat,
        font_size: f32,
    ) -> Result<Self, NumericAuthoringError> {
        let token_sources = numeric_template_sources(format);
        let mut dependencies = Vec::with_capacity(token_sources.len());
        for source in &token_sources {
            dependencies.push(
                crate::latex_authoring::prepare_math_tex(
                    numeric_math_tex(source.as_ref(), font_size)?,
                    backend,
                )?
                .into_compiled_resource_parts(),
            );
        }
        Ok(Self {
            token_sources,
            dependencies,
        })
    }

    pub(crate) fn dependencies(&self) -> &[NumericCompiledDependency] {
        &self.dependencies
    }

    pub(crate) fn binding(
        &self,
        signal: noon_core::SemanticNodeId,
        handles: &[noon_core::TextResourceHandle],
    ) -> SemanticNumericTextBinding {
        debug_assert_eq!(self.token_sources.len(), handles.len());
        SemanticNumericTextBinding::new(
            signal,
            self.token_sources
                .iter()
                .cloned()
                .zip(handles.iter().copied())
                .collect::<Vec<_>>()
                .into(),
            crate::latex_authoring::LATEX_POINT_TO_SCENE_SCALE,
        )
    }
}

impl PreparedDecimalValue {
    /// Prepare a DecimalNumber resource without mutating semantic state.  Composite
    /// authors use [`publish_batch`] to admit a complete set through one
    /// resource/transaction rollback boundary.
    pub(crate) fn prepare(
        backend: &mut impl LatexBackend,
        value: f64,
        format: DecimalFormat,
        font_size: f32,
    ) -> Result<Self, NumericAuthoringError> {
        prepare_numeric_value(backend, value, format, font_size)
    }

    pub(crate) fn dependencies(&self) -> &[NumericCompiledDependency] {
        &self.dependencies
    }

    pub(crate) fn decimal_number(&self) -> SemanticDecimalNumber {
        decimal_metadata(self.value, &self.format, self.font_size)
    }

    pub(crate) fn compose_resource(
        &self,
        store: &SemanticStore,
        handles: &[noon_core::TextResourceHandle],
    ) -> Result<TextResource, TextAuthoringError> {
        compose_numeric_text_resource(
            store,
            Arc::clone(&self.source),
            &self.tokens,
            handles,
            self.font_size,
        )
    }

    pub(crate) fn decimal_state(
        &self,
        store: &SemanticStore,
        handle: noon_core::TextResourceHandle,
        transform: SemanticTransform2_5D,
    ) -> Result<SemanticObjectState, TextAuthoringError> {
        let mut state = SemanticObjectState::new(handle);
        state.transform = transform;
        state.style.fill = Some(SemanticPaint::Solid(WHITE));
        state.set_decimal_number(Some(decimal_metadata(
            self.value,
            &self.format,
            self.font_size,
        )));
        state.set_text_presentation_baseline(numeric_presentation_baseline(
            store,
            handle,
            self.font_size,
        )?);
        Ok(state)
    }

    /// Admit a batch of independently prepared numeric displays and let the
    /// caller publish their one semantic transaction.  Compiler dependencies,
    /// composed number resources, and the caller transaction share one rollback
    /// scope for both cold and live authoring.
    pub(crate) fn publish_batch<E, T>(
        store: &mut SemanticStore,
        prepared: Vec<Self>,
        publish: impl FnOnce(
            &mut SemanticStore,
            &[noon_core::TextResourceHandle],
            &[PreparedDecimalValue],
        ) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<TextAuthoringError>
            + From<noon_core::SemanticTextImportError>
            + From<std::collections::TryReserveError>
            + From<noon_core::GeometryResourceError>,
    {
        let mut dependencies = Vec::new();
        let mut ranges = Vec::with_capacity(prepared.len());
        for value in &prepared {
            let start = dependencies.len();
            dependencies.extend(value.dependencies.iter().cloned());
            ranges.push(start..dependencies.len());
        }
        store.with_compiled_text_dependency_batch::<E, _>(
            dependencies,
            |store, handles| {
                prepared
                    .iter()
                    .zip(&ranges)
                    .map(|(value, range)| value.compose_resource(store, &handles[range.clone()]))
                    .collect::<Result<Vec<_>, TextAuthoringError>>()
                    .map_err(E::from)
            },
            |store, handles| publish(store, handles, &prepared),
        )
    }

    fn publish<T>(
        self,
        store: &mut SemanticStore,
        publish: impl FnOnce(
            &mut SemanticStore,
            noon_core::TextResourceHandle,
        ) -> Result<T, TextAuthoringError>,
    ) -> Result<T, TextAuthoringError> {
        let Self {
            source,
            tokens,
            dependencies,
            font_size,
            ..
        } = self;
        store.with_compiled_text_dependencies(
            dependencies,
            |store, handles| {
                compose_numeric_text_resource(store, source, &tokens, handles, font_size)
            },
            publish,
        )
    }
}

fn prepare_numeric_value(
    backend: &mut impl LatexBackend,
    value: f64,
    format: DecimalFormat,
    font_size: f32,
) -> Result<PreparedDecimalValue, NumericAuthoringError> {
    let (source, tokens) = numeric_layout_tokens(value, &format)?;
    let mut dependencies = Vec::new();
    dependencies
        .try_reserve_exact(tokens.len())
        .map_err(TextAuthoringError::from)?;
    for token in &tokens {
        dependencies.push(
            crate::latex_authoring::prepare_math_tex(
                numeric_math_tex(token.tex.as_ref(), font_size)?,
                backend,
            )?
            .into_compiled_resource_parts(),
        );
    }
    Ok(PreparedDecimalValue {
        source,
        tokens,
        dependencies,
        value,
        format,
        font_size,
    })
}

fn numeric_template_sources(format: &DecimalFormat) -> Vec<Arc<str>> {
    let mut sources = (b'0'..=b'9')
        .map(|digit| Arc::<str>::from(char::from(digit).to_string()))
        .collect::<Vec<_>>();
    sources.push(Arc::from("-"));
    if format.include_sign {
        sources.push(Arc::from("+"));
    }
    if format.group_with_commas {
        sources.push(Arc::from(","));
    }
    if format.decimal_places > 0 {
        sources.push(Arc::from("."));
    }
    if format.show_ellipsis {
        sources.push(Arc::from("\\dots"));
    }
    if let Some(unit) = format.unit.as_deref().filter(|unit| !unit.is_empty()) {
        sources.push(Arc::from(unit));
    }
    sources.sort_unstable_by(|left, right| left.as_ref().cmp(right.as_ref()));
    sources.dedup();
    sources
}

fn numeric_layout_tokens(
    value: f64,
    format: &DecimalFormat,
) -> Result<(Arc<str>, Vec<NumericLayoutToken>), NumericAuthoringError> {
    numeric_text_layout(value, format).map_err(|error| match error {
        NumericTextResourceError::Format(error) => NumericAuthoringError::Format(error),
        other => NumericAuthoringError::Text(numeric_resource_error(other)),
    })
}

fn compose_numeric_text_resource(
    store: &SemanticStore,
    source: Arc<str>,
    tokens: &[NumericLayoutToken],
    handles: &[noon_core::TextResourceHandle],
    font_size: f32,
) -> Result<TextResource, TextAuthoringError> {
    let children: Result<Vec<_>, _> = handles
        .iter()
        .map(|handle| {
            store
                .text_resources()
                .get(*handle)
                .ok_or(TextAuthoringError::Text(
                    noon_core::TextResourceValidationError::InvalidSourceSpan,
                ))
        })
        .collect();
    compose_numeric_text_resource_from_children(source, tokens, &children?, font_size)
}

fn compose_numeric_text_resource_from_children(
    source: Arc<str>,
    tokens: &[NumericLayoutToken],
    children: &[&TextResource],
    font_size: f32,
) -> Result<TextResource, TextAuthoringError> {
    compose_numeric_resource(
        source,
        tokens,
        children,
        font_size,
        crate::latex_authoring::LATEX_POINT_TO_SCENE_SCALE,
    )
    .map_err(numeric_resource_error)
}

fn numeric_resource_error(error: NumericTextResourceError) -> TextAuthoringError {
    match error {
        NumericTextResourceError::Format(_) => unreachable!("composition does not format values"),
        NumericTextResourceError::InvalidFontSize { bits } => {
            TextAuthoringError::InvalidFontSize(f32::from_bits(bits))
        }
        NumericTextResourceError::InvalidResource(error) => TextAuthoringError::Text(error),
        NumericTextResourceError::InvalidSourceSpan
        | NumericTextResourceError::TokenCountMismatch => {
            TextAuthoringError::Text(noon_core::TextResourceValidationError::InvalidSourceSpan)
        }
    }
}

#[allow(clippy::too_many_arguments)] // Stages one decimal content/state replacement transaction.
fn decimal_replacement_transaction(
    store: &SemanticStore,
    node: noon_core::SemanticNodeId,
    authored: &SemanticObjectState,
    mut effective: SemanticObjectState,
    handle: noon_core::TextResourceHandle,
    value: f64,
    format: &DecimalFormat,
    font_size: f32,
) -> Result<SemanticMutationTransaction, TextAuthoringError> {
    let fixed_left = left_edge_center(store, &effective)?;
    let effective_font_size =
        effective_font_size(store, &effective).map_err(|error| match error {
            NumericAuthoringError::Text(error) => error,
            NumericAuthoringError::Semantic(error) => TextAuthoringError::Semantic(error),
            NumericAuthoringError::InvalidEffectiveFontSize { value } => {
                TextAuthoringError::Semantic(crate::AuthoringError::InvalidRenderNumber {
                    name: "DecimalNumber effective font size".into(),
                    value,
                })
            }
            NumericAuthoringError::MissingPresentationBaseline => {
                TextAuthoringError::Semantic(crate::AuthoringError::NonPositiveNumber {
                    name: "DecimalNumber initial presentation height".into(),
                    value: 0.0,
                })
            }
            NumericAuthoringError::Format(_)
            | NumericAuthoringError::NotDecimalNumber
            | NumericAuthoringError::MissingEffectiveNumericSignal { .. }
            | NumericAuthoringError::NonScalarEffectiveNumericSignal { .. }
            | NumericAuthoringError::IntegerOutOfRange { .. } => {
                unreachable!("numeric replacement started from validated DecimalNumber state")
            }
        })?;
    effective.content = SemanticObjectContent::Text(handle);
    effective.transform = noon_core::SemanticTransform2_5D::default();
    let scale = effective_font_size / f64::from(font_size);
    effective.transform.scale.x = scale;
    effective.transform.scale.y = scale;
    let new_left = left_edge_center(store, &effective)?;
    effective.transform.translation.x += fixed_left.0 - new_left.0;
    effective.transform.translation.y += fixed_left.1 - new_left.1;
    let baseline = numeric_presentation_baseline(store, handle, font_size)?;
    let mut transaction = SemanticMutationTransaction::new();
    crate::semantic_mobject::stage_state_changes(&mut transaction, node, authored, &effective);
    if authored.z_index() != effective.z_index() {
        transaction.set_z_index(node, effective.z_index());
    }
    let mut metadata = decimal_metadata(value, format, font_size);
    if let Some(binding) = authored
        .decimal_number()
        .and_then(SemanticDecimalNumber::binding)
    {
        metadata = metadata.with_binding(binding.clone());
    }
    transaction.replace_decimal_number(node, metadata);
    transaction.replace_text_presentation_baseline(node, Some(baseline));
    Ok(transaction)
}

fn left_edge_center(
    store: &SemanticStore,
    state: &SemanticObjectState,
) -> Result<(f64, f64), TextAuthoringError> {
    Ok(
        crate::semantic_mobject::boundary_for_content(store, state.content, state.transform)
            .map_err(TextAuthoringError::Semantic)?
            .map_or(
                (state.transform.translation.x, state.transform.translation.y),
                |bounds| (bounds.min_x, (bounds.min_y + bounds.max_y) * 0.5),
            ),
    )
}

fn numeric_presentation_baseline(
    store: &SemanticStore,
    handle: noon_core::TextResourceHandle,
    font_size: f32,
) -> Result<TextPresentationBaseline, TextAuthoringError> {
    let resource = store
        .text_resources()
        .get(handle)
        .ok_or(TextAuthoringError::Text(
            noon_core::TextResourceValidationError::InvalidSourceSpan,
        ))?;
    TextPresentationBaseline::new(f64::from(font_size), f64::from(resource.bounds.height()))
        .ok_or(TextAuthoringError::InvalidFontSize(font_size))
}

fn effective_font_size(
    store: &SemanticStore,
    state: &SemanticObjectState,
) -> Result<f64, NumericAuthoringError> {
    let baseline = state
        .text_presentation_baseline()
        .ok_or(NumericAuthoringError::MissingPresentationBaseline)?;
    if baseline.initial_height == 0.0 {
        return Ok(baseline.initial_font_size);
    }
    let height =
        crate::semantic_mobject::boundary_for_content(store, state.content, state.transform)
            .map_err(TextAuthoringError::Semantic)?
            .map_or(0.0, |bounds| bounds.height());
    let value = height / baseline.initial_height * baseline.initial_font_size;
    if !value.is_finite() || value <= 0.0 || value > f64::from(f32::MAX) {
        return Err(NumericAuthoringError::InvalidEffectiveFontSize { value });
    }
    Ok(value)
}

fn numeric_math_tex(source: &str, font_size: f32) -> Result<MathTex, NumericAuthoringError> {
    Ok(MathTex::from_strings([source])?.with_font_size(font_size))
}
fn decimal_metadata(value: f64, format: &DecimalFormat, font_size: f32) -> SemanticDecimalNumber {
    SemanticDecimalNumber::new(
        value,
        format.decimal_places,
        format.include_sign,
        format.group_with_commas,
        format.show_ellipsis,
        format.unit.as_deref().map(Arc::<str>::from),
        font_size,
    )
}
#[derive(Clone, Debug)]
pub struct Integer(DecimalNumber);
impl Integer {
    pub fn new(
        store: Rc<RefCell<SemanticStore>>,
        backend: &mut impl LatexBackend,
        value: f64,
    ) -> Result<Self, NumericAuthoringError> {
        Ok(Self(DecimalNumber::new(
            store,
            backend,
            value,
            DecimalFormat {
                decimal_places: 0,
                ..Default::default()
            },
        )?))
    }
    pub fn value(&self) -> Result<i64, NumericAuthoringError> {
        integer_value(self.0.value()?)
    }
    pub fn set_value(
        &mut self,
        backend: &mut impl LatexBackend,
        value: f64,
    ) -> Result<&mut Self, NumericAuthoringError> {
        self.0.set_value(backend, value)?;
        Ok(self)
    }
    pub fn increment_value(
        &mut self,
        backend: &mut impl LatexBackend,
        delta: f64,
    ) -> Result<&mut Self, NumericAuthoringError> {
        self.0.increment_value(backend, delta)?;
        Ok(self)
    }
    pub fn mobject(&self) -> &Mobject {
        self.0.mobject()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{
        FontResourceArena, GeometryResourceArena, Rect, TextSourceKind, TextSourceSpan, Vec2,
    };

    type NumericLayoutPart = noon_core::NumericTextPart;

    struct RuleBackend;

    impl LatexBackend for RuleBackend {
        fn identity(&self) -> &str {
            "numeric-variable-rule-fixture"
        }

        fn format(&self) -> crate::LatexFormat {
            crate::LatexFormat::Preloaded
        }

        fn font(&mut self, _: &str) -> Result<crate::DviFontResource, String> {
            Err("font-free fixture".into())
        }

        fn compile(&mut self, _: &str) -> Result<Vec<u8>, String> {
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

    fn text_box(source: &str, bounds: Rect) -> TextResource {
        TextResource {
            source: Arc::from(source),
            kind: TextSourceKind::MathTex,
            runs: Arc::from([]),
            vector_items: Arc::from([]),
            render_items: Arc::from([]),
            parts: Arc::from([]),
            bounds,
            baseline: 0.0,
            layout_artifact: None,
        }
    }

    #[test]
    fn layout_applies_manim_minus_comma_and_superscript_rules() {
        let mut store = SemanticStore::new();
        let fonts = FontResourceArena::new();
        let geometry = GeometryResourceArena::new();
        let handles = [
            store
                .import_text_resource(
                    text_box("-", Rect::new(Vec2::ZERO, Vec2::new(4.0, 1.0))),
                    &fonts,
                    &geometry,
                )
                .unwrap(),
            store
                .import_text_resource(
                    text_box("1", Rect::new(Vec2::ZERO, Vec2::new(2.0, 4.0))),
                    &fonts,
                    &geometry,
                )
                .unwrap(),
            store
                .import_text_resource(
                    text_box(",", Rect::new(Vec2::ZERO, Vec2::new(1.0, 2.0))),
                    &fonts,
                    &geometry,
                )
                .unwrap(),
            store
                .import_text_resource(
                    text_box("^", Rect::new(Vec2::ZERO, Vec2::new(3.0, 3.0))),
                    &fonts,
                    &geometry,
                )
                .unwrap(),
        ];
        let tokens = [
            NumericLayoutToken {
                tex: Arc::from("-"),
                span: TextSourceSpan::new(0, 1),
                part: NumericLayoutPart::Minus,
            },
            NumericLayoutToken {
                tex: Arc::from("1"),
                span: TextSourceSpan::new(1, 2),
                part: NumericLayoutPart::Digit,
            },
            NumericLayoutToken {
                tex: Arc::from(","),
                span: TextSourceSpan::new(2, 3),
                part: NumericLayoutPart::Comma,
            },
            NumericLayoutToken {
                tex: Arc::from("^"),
                span: TextSourceSpan::new(3, 4),
                part: NumericLayoutPart::Unit { superscript: true },
            },
        ];
        let resource =
            compose_numeric_text_resource(&store, Arc::from("-1,^"), &tokens, &handles, 48.0)
                .unwrap();

        let scale = 48.0 * crate::latex_authoring::LATEX_POINT_TO_SCENE_SCALE;
        assert!((resource.bounds.width() - (10.0 * scale + 4.0 * 0.001 * 48.0)).abs() < 1e-6);
        assert!((resource.bounds.height() - 5.0 * scale).abs() < 1e-6);
        assert_eq!(resource.parts.len(), 4);
        assert_eq!(resource.parts[2].source_span, TextSourceSpan::new(2, 3));
    }

    #[test]
    fn numeric_tokens_keep_punctuation_ellipsis_and_unit_as_distinct_parts() {
        let format = DecimalFormat {
            decimal_places: 2,
            include_sign: true,
            group_with_commas: true,
            show_ellipsis: true,
            unit: Some("^\\circ".into()),
        };
        let (source, tokens) = numeric_layout_tokens(12_345.6, &format).unwrap();

        assert_eq!(source.as_ref(), "+12,345.60...^\\circ");
        assert_eq!(tokens[0].part, NumericLayoutPart::Sign);
        assert_eq!(tokens[3].part, NumericLayoutPart::Comma);
        assert_eq!(tokens[7].part, NumericLayoutPart::DecimalPoint);
        assert_eq!(tokens[10].part, NumericLayoutPart::Ellipsis);
        assert_eq!(
            tokens[11].part,
            NumericLayoutPart::Unit { superscript: true }
        );
        assert_eq!(tokens[10].span, TextSourceSpan::new(10, 13));
        assert_eq!(tokens[11].span, TextSourceSpan::new(13, 19));
    }

    #[test]
    fn integer_uses_bankers_rounding() {
        assert_eq!(integer_value(2.5).unwrap(), 2);
        assert_eq!(integer_value(3.5).unwrap(), 4);
        assert!(integer_value(f64::NAN).is_err());
    }

    #[test]
    fn replacement_resets_rotation_preserves_left_edge_and_effective_font() {
        let mut store = SemanticStore::new();
        let fonts = FontResourceArena::new();
        let geometry = GeometryResourceArena::new();
        let old = store
            .import_text_resource(
                text_box("1", Rect::new(Vec2::new(-2.0, -1.0), Vec2::new(2.0, 1.0))),
                &fonts,
                &geometry,
            )
            .unwrap();
        let fresh = store
            .import_text_resource(
                text_box("22", Rect::new(Vec2::new(-3.0, -1.0), Vec2::new(3.0, 1.0))),
                &fonts,
                &geometry,
            )
            .unwrap();
        let mut authored = SemanticObjectState::new(old);
        authored.set_decimal_number(Some(decimal_metadata(1.0, &DecimalFormat::default(), 48.0)));
        authored.set_text_presentation_baseline(TextPresentationBaseline::new(48.0, 2.0).unwrap());
        let node = store.insert_semantic_object(authored.clone());
        let mut effective = authored.clone();
        effective.transform.scale.x = 2.0;
        effective.transform.scale.y = 2.0;
        effective.transform.rotation_z = 0.6;
        effective.transform.translation.x = 3.0;
        effective.transform.translation.y = -2.0;
        let before = left_edge_center(&store, &effective).unwrap();
        let font = effective_font_size(&store, &effective).unwrap();
        let tx = decimal_replacement_transaction(
            &store,
            node,
            &authored,
            effective,
            fresh,
            2.0,
            &DecimalFormat::default(),
            48.0,
        )
        .unwrap();
        tx.apply(&mut store).unwrap();
        let after = store.semantic_object_state_checked(node).unwrap();
        assert_eq!(after.transform.rotation_z, 0.0);
        assert!((left_edge_center(&store, after).unwrap().0 - before.0).abs() < 1e-9);
        assert!((left_edge_center(&store, after).unwrap().1 - before.1).abs() < 1e-9);
        assert!((effective_font_size(&store, after).unwrap() - font).abs() < 1e-9);
    }

    #[test]
    fn live_decimal_value_reads_bound_signal_without_mutating_authored_metadata() {
        let mut backend = RuleBackend;
        let scene = crate::Scene::new();
        let tracker = scene.value_tracker(1.25).unwrap();
        let mut number = DecimalNumber::new(
            Rc::clone(scene.integration_store()),
            &mut backend,
            1.25,
            DecimalFormat::default(),
        )
        .unwrap();
        number.bind_to_tracker(&mut backend, &tracker).unwrap();
        scene.add(number.mobject()).unwrap();

        let mut execution = scene.execution_session().unwrap();
        assert_eq!(
            scene.live(&mut execution).decimal_value(&number).unwrap(),
            1.25
        );

        execution
            .set_reactive_input(tracker.node_id(), 7.5)
            .unwrap();
        assert_eq!(
            scene.live(&mut execution).decimal_value(&number).unwrap(),
            7.5
        );
        assert_eq!(
            integer_value(scene.live(&mut execution).decimal_value(&number).unwrap()).unwrap(),
            8
        );
        assert_eq!(number.value().unwrap(), 1.25);
    }

    #[test]
    fn tracker_binding_updates_effective_text_with_stable_authored_revision_and_bounded_slot() {
        use noon_core::TextResourceLookup;

        let mut backend = RuleBackend;
        let mut scene = crate::Scene::new();
        let tracker = scene.value_tracker(-0.004).unwrap();
        let unrelated = scene.value_tracker(1.0).unwrap();
        let mut number = DecimalNumber::new(
            Rc::clone(scene.integration_store()),
            &mut backend,
            -0.004,
            DecimalFormat {
                decimal_places: 2,
                include_sign: true,
                group_with_commas: true,
                show_ellipsis: true,
                unit: Some("m".into()),
            },
        )
        .unwrap();
        number.bind_to_tracker(&mut backend, &tracker).unwrap();
        scene.add(number.mobject()).unwrap();

        let mut session = scene.execution_session().unwrap();
        let initial_context = session.publication_context();
        let initial_handle = session.frame().objects[0].text().unwrap();
        assert_eq!(
            session
                .text_resources()
                .get(initial_handle)
                .unwrap()
                .source
                .as_ref(),
            "+0.00...m"
        );
        assert_eq!(session.effective_text_resource_stats().live_resources, 1);
        assert_eq!(session.effective_text_resource_slot_capacity(), 1);

        session
            .set_reactive_input(tracker.node_id(), 12_345.6_f32)
            .unwrap();
        let changed = session.publication_context();
        assert_eq!(changed.scene_revision(), initial_context.scene_revision());
        assert_eq!(
            changed.execution_revision(),
            initial_context.execution_revision()
        );
        assert_ne!(changed.frame_epoch(), initial_context.frame_epoch());
        let changed_handle = session.frame().objects[0].text().unwrap();
        assert_eq!(changed_handle.id, initial_handle.id);
        assert!(changed_handle.version > initial_handle.version);
        assert!(session.text_resources().get(initial_handle).is_none());
        assert_eq!(
            session
                .text_resources()
                .get(changed_handle)
                .unwrap()
                .source
                .as_ref(),
            "+12,345.60...m"
        );

        session
            .set_reactive_input(unrelated.node_id(), 2.0_f32)
            .unwrap();
        assert_eq!(session.frame().objects[0].text(), Some(changed_handle));
        let before_rejection = session.publication_context();
        assert!(session
            .set_reactive_input(tracker.node_id(), f32::NAN)
            .is_err());
        assert_eq!(session.publication_context(), before_rejection);
        assert_eq!(session.frame().objects[0].text(), Some(changed_handle));

        for value in 0..64 {
            session
                .set_reactive_input(tracker.node_id(), value as f32 + 0.25)
                .unwrap();
        }
        assert_eq!(session.effective_text_resource_stats().live_resources, 1);
        assert_eq!(session.effective_text_resource_slot_capacity(), 1);
        let current = session.frame().objects[0].text().unwrap();
        assert_eq!(
            session
                .text_resources()
                .get(current)
                .unwrap()
                .source
                .as_ref(),
            "+63.25...m"
        );
    }

    #[test]
    fn live_binding_survives_detach_reentry_and_reuses_effective_slot() {
        use noon_core::TextResourceLookup;

        let mut backend = RuleBackend;
        let mut scene = crate::Scene::new();
        let tracker = scene.value_tracker(1.25).unwrap();
        let number = DecimalNumber::new(
            Rc::clone(scene.integration_store()),
            &mut backend,
            0.0,
            DecimalFormat::default(),
        )
        .unwrap();
        scene.add(number.mobject()).unwrap();
        let mut session = scene.execution_session().unwrap();

        scene
            .live(&mut session)
            .bind_decimal_to_tracker(&number, &mut backend, &tracker)
            .unwrap();
        let bound = session.frame().objects[0].text().unwrap();
        assert_eq!(
            session.text_resources().get(bound).unwrap().source.as_ref(),
            "1.25"
        );
        assert_eq!(session.effective_text_resource_stats().live_resources, 1);
        assert_eq!(session.effective_text_resource_slot_capacity(), 1);

        session
            .set_reactive_input(tracker.node_id(), 7.5_f32)
            .unwrap();
        assert_eq!(
            session
                .text_resources()
                .get(session.frame().objects[0].text().unwrap())
                .unwrap()
                .source
                .as_ref(),
            "7.50"
        );

        scene.live(&mut session).remove(number.mobject()).unwrap();
        assert_eq!(session.effective_text_resource_stats().live_resources, 0);
        assert_eq!(session.effective_text_resource_slot_capacity(), 1);

        scene.live(&mut session).add(number.mobject()).unwrap();
        assert_eq!(session.effective_text_resource_stats().live_resources, 1);
        assert_eq!(session.effective_text_resource_slot_capacity(), 1);
        assert_eq!(
            session
                .text_resources()
                .get(session.frame().objects[0].text().unwrap())
                .unwrap()
                .source
                .as_ref(),
            "7.50"
        );
    }

    #[test]
    fn shared_tracker_bindings_retire_and_reenter_without_resource_growth() {
        use noon_core::TextResourceLookup;

        const DISPLAY_COUNT: usize = 32;
        let mut backend = RuleBackend;
        let mut scene = crate::Scene::new();
        let tracker = scene.value_tracker(2.5).unwrap();
        let mut numbers = Vec::with_capacity(DISPLAY_COUNT);
        for _ in 0..DISPLAY_COUNT {
            let mut number = DecimalNumber::new(
                Rc::clone(scene.integration_store()),
                &mut backend,
                0.0,
                DecimalFormat::default(),
            )
            .unwrap();
            number.bind_to_tracker(&mut backend, &tracker).unwrap();
            scene.add(number.mobject()).unwrap();
            numbers.push(number);
        }

        let mut session = scene.execution_session().unwrap();
        assert_eq!(
            session.effective_text_resource_stats().live_resources,
            DISPLAY_COUNT
        );
        assert_eq!(
            session.effective_text_resource_slot_capacity(),
            DISPLAY_COUNT
        );
        session
            .set_reactive_input(tracker.node_id(), 9.75_f32)
            .unwrap();
        for object in &session.frame().objects {
            let handle = object.text().unwrap();
            assert_eq!(
                session
                    .text_resources()
                    .get(handle)
                    .unwrap()
                    .source
                    .as_ref(),
                "9.75"
            );
        }

        for number in &numbers {
            scene.live(&mut session).remove(number.mobject()).unwrap();
        }
        assert_eq!(session.effective_text_resource_stats().live_resources, 0);
        assert_eq!(
            session.effective_text_resource_slot_capacity(),
            DISPLAY_COUNT
        );

        for number in &numbers {
            scene.live(&mut session).add(number.mobject()).unwrap();
        }
        assert_eq!(
            session.effective_text_resource_stats().live_resources,
            DISPLAY_COUNT
        );
        assert_eq!(
            session.effective_text_resource_slot_capacity(),
            DISPLAY_COUNT
        );
    }
}
