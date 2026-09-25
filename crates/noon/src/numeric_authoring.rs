//! Resource-backed DecimalNumber and Integer over shared MathTex semantics.

use crate::{
    format_decimal, DecimalFormat, LatexBackend, MathTex, Mobject, NumericFormatError,
    TextAuthoringError,
};
use noon_core::{SemanticDecimalNumber, SemanticMutationTransaction, SemanticStore};
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[derive(Clone, Debug, PartialEq)]
pub enum NumericAuthoringError {
    Format(NumericFormatError),
    Text(TextAuthoringError),
    Semantic(crate::AuthoringError),
    NotDecimalNumber,
    IntegerOutOfRange { value: f64 },
}
impl std::fmt::Display for NumericAuthoringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Format(error) => error.fmt(f),
            Self::Text(error) => error.fmt(f),
            Self::Semantic(error) => error.fmt(f),
            Self::NotDecimalNumber => f.write_str("semantic object is not a DecimalNumber"),
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
        Self::construct(store, backend, value, format, font_size)
    }

    fn construct(
        store: Rc<RefCell<SemanticStore>>,
        backend: &mut impl LatexBackend,
        value: f64,
        format: DecimalFormat,
        font_size: f32,
    ) -> Result<Self, NumericAuthoringError> {
        let source = format_decimal(value, &format)?;
        let admission = crate::latex_authoring::prepare_math_tex(
            numeric_math_tex(&source, font_size)?,
            backend,
        )?;
        let number = decimal_metadata(value, &format, font_size);
        let result = admission.publish_with_state(
            &mut store.borrow_mut(),
            move |mut state| {
                state.set_decimal_number(Some(number));
                state
            },
            |semantic, transaction| {
                transaction
                    .apply(semantic)
                    .map_err(crate::AuthoringError::from)
                    .map_err(TextAuthoringError::Semantic)
            },
        )?;
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
    pub fn value(&self) -> Result<f64, NumericAuthoringError> {
        Ok(self.metadata()?.value())
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

    pub fn set_value(
        &mut self,
        backend: &mut impl LatexBackend,
        value: f64,
    ) -> Result<&mut Self, NumericAuthoringError> {
        let before = self.object.state()?;
        let format = self.format()?;
        let font_size = self.metadata()?.font_size();
        let source = format_decimal(value, &format)?;
        let admission = crate::latex_authoring::prepare_math_tex(
            numeric_math_tex(&source, font_size)?,
            backend,
        )?;
        let (resource, fonts, geometry) = admission.into_resource_parts();
        let node = self.object.node_id();
        let old = before.content.text();
        let mut store = self.object.integration_store().borrow_mut();
        store.with_compiled_text_resource(resource, fonts, &geometry, |store, handle| {
            let mut next = before.clone();
            let fixed_left =
                crate::semantic_mobject::boundary_for_content(store, next.content, next.transform)
                    .map_err(TextAuthoringError::Semantic)?
                    .map_or(
                        crate::semantic_mobject::state_center(store, &next)
                            .map_err(TextAuthoringError::Semantic)?
                            .0,
                        |bounds| bounds.min_x,
                    );
            next.content = handle.into();
            let new_left =
                crate::semantic_mobject::boundary_for_content(store, next.content, next.transform)
                    .map_err(TextAuthoringError::Semantic)?
                    .map_or(fixed_left, |bounds| bounds.min_x);
            next.transform.translation.x += fixed_left - new_left;
            let mut transaction = SemanticMutationTransaction::new();
            crate::semantic_mobject::stage_state_changes(&mut transaction, node, &before, &next);
            transaction.replace_decimal_number(node, decimal_metadata(value, &format, font_size));
            transaction
                .apply(store)
                .map_err(crate::AuthoringError::from)
                .map_err(TextAuthoringError::Semantic)
        })?;
        let _ = old; // Resource retirement is owned by the semantic arena lifecycle.
        drop(store);
        Ok(self)
    }
    pub fn increment_value(
        &mut self,
        backend: &mut impl LatexBackend,
        delta: f64,
    ) -> Result<&mut Self, NumericAuthoringError> {
        self.set_value(backend, self.value()? + delta)
    }
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
    #[test]
    fn integer_uses_bankers_rounding() {
        assert_eq!(integer_value(2.5).unwrap(), 2);
        assert_eq!(integer_value(3.5).unwrap(), 4);
        assert!(integer_value(f64::NAN).is_err());
    }
}
