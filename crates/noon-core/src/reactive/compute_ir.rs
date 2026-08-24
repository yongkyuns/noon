use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::{SignalId, ValueKind, Vec2};

use super::{ReactiveError, ReactiveExpr, ReactiveValue};

/// Compact value types understood by the native reactive compute IR.
///
/// This is deliberately narrower than `ValueKind`: object snapshots remain outside
/// native compute until geometry/structural mutation has its own execution model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputeValueKind {
    Bool,
    Scalar,
    Vec2,
}

impl ComputeValueKind {
    pub const fn from_reactive(value: &ReactiveValue) -> Self {
        match value {
            ReactiveValue::Bool(_) => Self::Bool,
            ReactiveValue::Scalar(_) => Self::Scalar,
            ReactiveValue::Vec2(_) => Self::Vec2,
        }
    }

    pub const fn as_value_kind(self) -> ValueKind {
        match self {
            Self::Bool => ValueKind::Bool,
            Self::Scalar => ValueKind::Scalar,
            Self::Vec2 => ValueKind::Vec2,
        }
    }
}

/// Dense SSA-style register identifier local to one reactive kernel.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct ComputeRegister(u32);

impl ComputeRegister {
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Typed scalar/vector/bool instruction set used by native reactive execution.
///
/// Operands are validated while lowering `ReactiveExpr`, so execution does not
/// recursively inspect the authoring AST or rediscover operand types every frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputeInstruction {
    ConstantBool {
        dst: ComputeRegister,
        value: bool,
    },
    ConstantScalar {
        dst: ComputeRegister,
        value: f32,
    },
    ConstantVec2 {
        dst: ComputeRegister,
        value: Vec2,
    },
    LoadBool {
        dst: ComputeRegister,
        signal_index: u32,
    },
    LoadScalar {
        dst: ComputeRegister,
        signal_index: u32,
    },
    LoadVec2 {
        dst: ComputeRegister,
        signal_index: u32,
    },
    AddScalar {
        dst: ComputeRegister,
        lhs: ComputeRegister,
        rhs: ComputeRegister,
    },
    AddVec2 {
        dst: ComputeRegister,
        lhs: ComputeRegister,
        rhs: ComputeRegister,
    },
    SubScalar {
        dst: ComputeRegister,
        lhs: ComputeRegister,
        rhs: ComputeRegister,
    },
    SubVec2 {
        dst: ComputeRegister,
        lhs: ComputeRegister,
        rhs: ComputeRegister,
    },
    MulScalar {
        dst: ComputeRegister,
        lhs: ComputeRegister,
        rhs: ComputeRegister,
    },
    MulScalarVec2 {
        dst: ComputeRegister,
        scalar: ComputeRegister,
        vector: ComputeRegister,
    },
    MulVec2Scalar {
        dst: ComputeRegister,
        vector: ComputeRegister,
        scalar: ComputeRegister,
    },
    NegScalar {
        dst: ComputeRegister,
        value: ComputeRegister,
    },
    NegVec2 {
        dst: ComputeRegister,
        value: ComputeRegister,
    },
    SinScalar {
        dst: ComputeRegister,
        value: ComputeRegister,
    },
    CosScalar {
        dst: ComputeRegister,
        value: ComputeRegister,
    },
}

impl ComputeInstruction {
    /// All current instructions are deterministic pure operations and are suitable
    /// for future SIMD/WGSL lowering. Keeping this query on the IR avoids making a
    /// second, backend-specific expression language later.
    pub const fn is_backend_lowerable(&self) -> bool {
        true
    }
}

/// One flattened compute kernel for a derived reactive signal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComputeKernel {
    owner: SignalId,
    instructions: Vec<ComputeInstruction>,
    register_kinds: Vec<ComputeValueKind>,
    output: ComputeRegister,
}

impl ComputeKernel {
    pub fn instructions(&self) -> &[ComputeInstruction] {
        &self.instructions
    }

    pub fn register_kinds(&self) -> &[ComputeValueKind] {
        &self.register_kinds
    }

    pub const fn output(&self) -> ComputeRegister {
        self.output
    }

    pub fn output_kind(&self) -> ComputeValueKind {
        self.register_kinds[self.output.index()]
    }

    pub fn register_count(&self) -> usize {
        self.register_kinds.len()
    }

    pub(crate) fn evaluate(
        &self,
        signals: &[ReactiveValue],
        scratch: &mut [ReactiveValue],
    ) -> Result<ReactiveValue, ReactiveError> {
        debug_assert!(scratch.len() >= self.register_count());
        for instruction in &self.instructions {
            execute_instruction(instruction, signals, scratch, self.owner)?;
        }
        Ok(scratch[self.output.index()].clone())
    }
}

struct Lowerer<'a> {
    owner: SignalId,
    signal_indices: &'a BTreeMap<SignalId, usize>,
    signal_kinds: &'a [Option<ComputeValueKind>],
    instructions: Vec<ComputeInstruction>,
    register_kinds: Vec<ComputeValueKind>,
}

impl<'a> Lowerer<'a> {
    fn new(
        owner: SignalId,
        signal_indices: &'a BTreeMap<SignalId, usize>,
        signal_kinds: &'a [Option<ComputeValueKind>],
    ) -> Self {
        Self {
            owner,
            signal_indices,
            signal_kinds,
            instructions: Vec::new(),
            register_kinds: Vec::new(),
        }
    }

    fn register(&mut self, kind: ComputeValueKind) -> ComputeRegister {
        let index = u32::try_from(self.register_kinds.len())
            .expect("reactive compute kernel register space exhausted");
        self.register_kinds.push(kind);
        ComputeRegister::new(index)
    }

    fn lower(&mut self, expression: &ReactiveExpr) -> Result<ComputeRegister, ReactiveError> {
        match expression {
            ReactiveExpr::Constant(ReactiveValue::Bool(value)) => {
                let dst = self.register(ComputeValueKind::Bool);
                self.instructions
                    .push(ComputeInstruction::ConstantBool { dst, value: *value });
                Ok(dst)
            }
            ReactiveExpr::Constant(ReactiveValue::Scalar(value)) => {
                let dst = self.register(ComputeValueKind::Scalar);
                self.instructions
                    .push(ComputeInstruction::ConstantScalar { dst, value: *value });
                Ok(dst)
            }
            ReactiveExpr::Constant(ReactiveValue::Vec2(value)) => {
                let dst = self.register(ComputeValueKind::Vec2);
                self.instructions
                    .push(ComputeInstruction::ConstantVec2 { dst, value: *value });
                Ok(dst)
            }
            ReactiveExpr::Signal(signal) => {
                let signal_index = *self
                    .signal_indices
                    .get(signal)
                    .ok_or(ReactiveError::UnknownSignal(*signal))?;
                let kind = self.signal_kinds[signal_index]
                    .expect("topological lowering must know dependency value types");
                let dst = self.register(kind);
                let signal_index = u32::try_from(signal_index)
                    .expect("reactive signal index space exhausted");
                self.instructions.push(match kind {
                    ComputeValueKind::Bool => ComputeInstruction::LoadBool { dst, signal_index },
                    ComputeValueKind::Scalar => {
                        ComputeInstruction::LoadScalar { dst, signal_index }
                    }
                    ComputeValueKind::Vec2 => ComputeInstruction::LoadVec2 { dst, signal_index },
                });
                Ok(dst)
            }
            ReactiveExpr::Add(lhs, rhs) => {
                let lhs = self.lower(lhs)?;
                let rhs = self.lower(rhs)?;
                let lhs_kind = self.register_kinds[lhs.index()];
                let rhs_kind = self.register_kinds[rhs.index()];
                let kind = match (lhs_kind, rhs_kind) {
                    (ComputeValueKind::Scalar, ComputeValueKind::Scalar) => {
                        ComputeValueKind::Scalar
                    }
                    (ComputeValueKind::Vec2, ComputeValueKind::Vec2) => ComputeValueKind::Vec2,
                    _ => return Err(self.invalid("add")),
                };
                let dst = self.register(kind);
                self.instructions.push(match kind {
                    ComputeValueKind::Scalar => ComputeInstruction::AddScalar { dst, lhs, rhs },
                    ComputeValueKind::Vec2 => ComputeInstruction::AddVec2 { dst, lhs, rhs },
                    ComputeValueKind::Bool => unreachable!(),
                });
                Ok(dst)
            }
            ReactiveExpr::Sub(lhs, rhs) => {
                let lhs = self.lower(lhs)?;
                let rhs = self.lower(rhs)?;
                let lhs_kind = self.register_kinds[lhs.index()];
                let rhs_kind = self.register_kinds[rhs.index()];
                let kind = match (lhs_kind, rhs_kind) {
                    (ComputeValueKind::Scalar, ComputeValueKind::Scalar) => {
                        ComputeValueKind::Scalar
                    }
                    (ComputeValueKind::Vec2, ComputeValueKind::Vec2) => ComputeValueKind::Vec2,
                    _ => return Err(self.invalid("sub")),
                };
                let dst = self.register(kind);
                self.instructions.push(match kind {
                    ComputeValueKind::Scalar => ComputeInstruction::SubScalar { dst, lhs, rhs },
                    ComputeValueKind::Vec2 => ComputeInstruction::SubVec2 { dst, lhs, rhs },
                    ComputeValueKind::Bool => unreachable!(),
                });
                Ok(dst)
            }
            ReactiveExpr::Mul(lhs, rhs) => {
                let lhs = self.lower(lhs)?;
                let rhs = self.lower(rhs)?;
                let lhs_kind = self.register_kinds[lhs.index()];
                let rhs_kind = self.register_kinds[rhs.index()];
                let (kind, instruction) = match (lhs_kind, rhs_kind) {
                    (ComputeValueKind::Scalar, ComputeValueKind::Scalar) => {
                        let dst = self.register(ComputeValueKind::Scalar);
                        (
                            ComputeValueKind::Scalar,
                            ComputeInstruction::MulScalar { dst, lhs, rhs },
                        )
                    }
                    (ComputeValueKind::Scalar, ComputeValueKind::Vec2) => {
                        let dst = self.register(ComputeValueKind::Vec2);
                        (
                            ComputeValueKind::Vec2,
                            ComputeInstruction::MulScalarVec2 {
                                dst,
                                scalar: lhs,
                                vector: rhs,
                            },
                        )
                    }
                    (ComputeValueKind::Vec2, ComputeValueKind::Scalar) => {
                        let dst = self.register(ComputeValueKind::Vec2);
                        (
                            ComputeValueKind::Vec2,
                            ComputeInstruction::MulVec2Scalar {
                                dst,
                                vector: lhs,
                                scalar: rhs,
                            },
                        )
                    }
                    _ => return Err(self.invalid("mul")),
                };
                let dst = match &instruction {
                    ComputeInstruction::MulScalar { dst, .. }
                    | ComputeInstruction::MulScalarVec2 { dst, .. }
                    | ComputeInstruction::MulVec2Scalar { dst, .. } => *dst,
                    _ => unreachable!(),
                };
                debug_assert_eq!(self.register_kinds[dst.index()], kind);
                self.instructions.push(instruction);
                Ok(dst)
            }
            ReactiveExpr::Neg(value) => {
                let value = self.lower(value)?;
                let kind = self.register_kinds[value.index()];
                let dst = self.register(kind);
                self.instructions.push(match kind {
                    ComputeValueKind::Scalar => ComputeInstruction::NegScalar { dst, value },
                    ComputeValueKind::Vec2 => ComputeInstruction::NegVec2 { dst, value },
                    ComputeValueKind::Bool => return Err(self.invalid("neg")),
                });
                Ok(dst)
            }
            ReactiveExpr::Sin(value) => self.lower_scalar_unary(value, "sin", |dst, value| {
                ComputeInstruction::SinScalar { dst, value }
            }),
            ReactiveExpr::Cos(value) => self.lower_scalar_unary(value, "cos", |dst, value| {
                ComputeInstruction::CosScalar { dst, value }
            }),
        }
    }

    fn lower_scalar_unary(
        &mut self,
        expression: &ReactiveExpr,
        operation: &'static str,
        make_instruction: impl FnOnce(ComputeRegister, ComputeRegister) -> ComputeInstruction,
    ) -> Result<ComputeRegister, ReactiveError> {
        let value = self.lower(expression)?;
        if self.register_kinds[value.index()] != ComputeValueKind::Scalar {
            return Err(self.invalid(operation));
        }
        let dst = self.register(ComputeValueKind::Scalar);
        self.instructions.push(make_instruction(dst, value));
        Ok(dst)
    }

    const fn invalid(&self, operation: &'static str) -> ReactiveError {
        ReactiveError::InvalidExpression {
            signal: self.owner,
            operation,
        }
    }
}

pub(crate) fn lower_reactive_expression(
    owner: SignalId,
    expression: &ReactiveExpr,
    signal_indices: &BTreeMap<SignalId, usize>,
    signal_kinds: &[Option<ComputeValueKind>],
) -> Result<ComputeKernel, ReactiveError> {
    let mut lowerer = Lowerer::new(owner, signal_indices, signal_kinds);
    let output = lowerer.lower(expression)?;
    Ok(ComputeKernel {
        owner,
        instructions: lowerer.instructions,
        register_kinds: lowerer.register_kinds,
        output,
    })
}

fn execute_instruction(
    instruction: &ComputeInstruction,
    signals: &[ReactiveValue],
    scratch: &mut [ReactiveValue],
    owner: SignalId,
) -> Result<(), ReactiveError> {
    match *instruction {
        ComputeInstruction::ConstantBool { dst, value } => {
            scratch[dst.index()] = ReactiveValue::Bool(value);
        }
        ComputeInstruction::ConstantScalar { dst, value } => {
            scratch[dst.index()] = ReactiveValue::Scalar(value);
        }
        ComputeInstruction::ConstantVec2 { dst, value } => {
            scratch[dst.index()] = ReactiveValue::Vec2(value);
        }
        ComputeInstruction::LoadBool { dst, signal_index } => {
            scratch[dst.index()] = match signals[signal_index as usize] {
                ReactiveValue::Bool(value) => ReactiveValue::Bool(value),
                _ => return Err(type_bug(owner, "load_bool")),
            };
        }
        ComputeInstruction::LoadScalar { dst, signal_index } => {
            scratch[dst.index()] = match signals[signal_index as usize] {
                ReactiveValue::Scalar(value) => ReactiveValue::Scalar(value),
                _ => return Err(type_bug(owner, "load_scalar")),
            };
        }
        ComputeInstruction::LoadVec2 { dst, signal_index } => {
            scratch[dst.index()] = match signals[signal_index as usize] {
                ReactiveValue::Vec2(value) => ReactiveValue::Vec2(value),
                _ => return Err(type_bug(owner, "load_vec2")),
            };
        }
        ComputeInstruction::AddScalar { dst, lhs, rhs } => {
            let lhs = scalar(scratch, lhs, owner, "add")?;
            let rhs = scalar(scratch, rhs, owner, "add")?;
            scratch[dst.index()] = ReactiveValue::Scalar(lhs + rhs);
        }
        ComputeInstruction::AddVec2 { dst, lhs, rhs } => {
            let lhs = vec2(scratch, lhs, owner, "add")?;
            let rhs = vec2(scratch, rhs, owner, "add")?;
            scratch[dst.index()] = ReactiveValue::Vec2(lhs + rhs);
        }
        ComputeInstruction::SubScalar { dst, lhs, rhs } => {
            let lhs = scalar(scratch, lhs, owner, "sub")?;
            let rhs = scalar(scratch, rhs, owner, "sub")?;
            scratch[dst.index()] = ReactiveValue::Scalar(lhs - rhs);
        }
        ComputeInstruction::SubVec2 { dst, lhs, rhs } => {
            let lhs = vec2(scratch, lhs, owner, "sub")?;
            let rhs = vec2(scratch, rhs, owner, "sub")?;
            scratch[dst.index()] = ReactiveValue::Vec2(lhs - rhs);
        }
        ComputeInstruction::MulScalar { dst, lhs, rhs } => {
            let lhs = scalar(scratch, lhs, owner, "mul")?;
            let rhs = scalar(scratch, rhs, owner, "mul")?;
            scratch[dst.index()] = ReactiveValue::Scalar(lhs * rhs);
        }
        ComputeInstruction::MulScalarVec2 {
            dst,
            scalar: scalar_register,
            vector,
        } => {
            let scalar = scalar(scratch, scalar_register, owner, "mul")?;
            let vector = vec2(scratch, vector, owner, "mul")?;
            scratch[dst.index()] = ReactiveValue::Vec2(scalar * vector);
        }
        ComputeInstruction::MulVec2Scalar {
            dst,
            vector,
            scalar: scalar_register,
        } => {
            let vector = vec2(scratch, vector, owner, "mul")?;
            let scalar = scalar(scratch, scalar_register, owner, "mul")?;
            scratch[dst.index()] = ReactiveValue::Vec2(vector * scalar);
        }
        ComputeInstruction::NegScalar { dst, value } => {
            scratch[dst.index()] = ReactiveValue::Scalar(-scalar(scratch, value, owner, "neg")?);
        }
        ComputeInstruction::NegVec2 { dst, value } => {
            scratch[dst.index()] = ReactiveValue::Vec2(-vec2(scratch, value, owner, "neg")?);
        }
        ComputeInstruction::SinScalar { dst, value } => {
            scratch[dst.index()] =
                ReactiveValue::Scalar(scalar(scratch, value, owner, "sin")?.sin());
        }
        ComputeInstruction::CosScalar { dst, value } => {
            scratch[dst.index()] =
                ReactiveValue::Scalar(scalar(scratch, value, owner, "cos")?.cos());
        }
    }
    Ok(())
}

fn scalar(
    scratch: &[ReactiveValue],
    register: ComputeRegister,
    owner: SignalId,
    operation: &'static str,
) -> Result<f32, ReactiveError> {
    match scratch[register.index()] {
        ReactiveValue::Scalar(value) => Ok(value),
        _ => Err(type_bug(owner, operation)),
    }
}

fn vec2(
    scratch: &[ReactiveValue],
    register: ComputeRegister,
    owner: SignalId,
    operation: &'static str,
) -> Result<Vec2, ReactiveError> {
    match scratch[register.index()] {
        ReactiveValue::Vec2(value) => Ok(value),
        _ => Err(type_bug(owner, operation)),
    }
}

const fn type_bug(owner: SignalId, operation: &'static str) -> ReactiveError {
    ReactiveError::InvalidExpression {
        signal: owner,
        operation,
    }
}
