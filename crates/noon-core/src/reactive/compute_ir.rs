use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};
use std::fmt::Write;

use crate::{ReactiveError, ReactiveExpr, ReactiveValue, SignalId, ValueKind};

/// Dense register index in the native compute program.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ComputeRegister(u32);

impl ComputeRegister {
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

/// Typed, backend-neutral operation used by native reactive execution.
///
/// Signal references are lowered to dense signal indices. There is no recursive
/// expression walking or semantic-ID map lookup while instructions execute.
#[derive(Clone, Debug, PartialEq)]
pub enum ComputeInstruction {
    Constant {
        dst: ComputeRegister,
        value: ReactiveValue,
    },
    LoadSignal {
        dst: ComputeRegister,
        signal_index: u32,
        kind: ValueKind,
    },
    Add {
        dst: ComputeRegister,
        lhs: ComputeRegister,
        rhs: ComputeRegister,
        kind: ValueKind,
    },
    Sub {
        dst: ComputeRegister,
        lhs: ComputeRegister,
        rhs: ComputeRegister,
        kind: ValueKind,
    },
    Mul {
        dst: ComputeRegister,
        lhs: ComputeRegister,
        rhs: ComputeRegister,
        result_kind: ValueKind,
    },
    Neg {
        dst: ComputeRegister,
        value: ComputeRegister,
        kind: ValueKind,
    },
    Sin {
        dst: ComputeRegister,
        value: ComputeRegister,
    },
    Cos {
        dst: ComputeRegister,
        value: ComputeRegister,
    },
}

impl ComputeInstruction {
    pub const fn destination(&self) -> ComputeRegister {
        match *self {
            Self::Constant { dst, .. }
            | Self::LoadSignal { dst, .. }
            | Self::Add { dst, .. }
            | Self::Sub { dst, .. }
            | Self::Mul { dst, .. }
            | Self::Neg { dst, .. }
            | Self::Sin { dst, .. }
            | Self::Cos { dst, .. } => dst,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComputeExecutionStats {
    pub instructions_executed: usize,
}

/// One lowered expression. The format is intentionally small and deterministic
/// so future SIMD and WGSL backends can consume the same typed program.
#[derive(Clone, Debug, PartialEq)]
pub struct ComputeProgram {
    instructions: Vec<ComputeInstruction>,
    register_kinds: Vec<ValueKind>,
    output: ComputeRegister,
    output_kind: ValueKind,
}

impl ComputeProgram {
    pub fn lower(
        expression: &ReactiveExpr,
        signal_indices: &BTreeMap<SignalId, usize>,
        signal_kinds: &[ValueKind],
        owner: SignalId,
    ) -> Result<Self, ReactiveError> {
        let mut builder = ComputeBuilder {
            instructions: Vec::new(),
            register_kinds: Vec::new(),
            signal_indices,
            signal_kinds,
            owner,
        };
        let (output, output_kind) = builder.lower(expression)?;
        Ok(Self {
            instructions: builder.instructions,
            register_kinds: builder.register_kinds,
            output,
            output_kind,
        })
    }

    pub fn instructions(&self) -> &[ComputeInstruction] {
        &self.instructions
    }

    pub fn register_kinds(&self) -> &[ValueKind] {
        &self.register_kinds
    }

    pub const fn output(&self) -> ComputeRegister {
        self.output
    }

    pub const fn output_kind(&self) -> ValueKind {
        self.output_kind
    }

    /// Deterministic backend/debug representation without making the internal
    /// timeline `ValueKind` part of a persisted serde schema.
    pub fn debug_text(&self) -> String {
        let mut output = String::new();
        for instruction in &self.instructions {
            let _ = writeln!(&mut output, "{instruction:?}");
        }
        let _ = writeln!(
            &mut output,
            "output=r{} kind={:?}",
            self.output.get(),
            self.output_kind
        );
        output
    }

    pub fn evaluate(&self, signals: &[ReactiveValue]) -> Result<ReactiveValue, ComputeVmError> {
        self.evaluate_with_stats(signals).map(|(value, _)| value)
    }

    pub fn evaluate_with_stats(
        &self,
        signals: &[ReactiveValue],
    ) -> Result<(ReactiveValue, ComputeExecutionStats), ComputeVmError> {
        let mut registers = vec![ReactiveValue::Scalar(0.0); self.register_kinds.len()];
        let mut stats = ComputeExecutionStats::default();

        for instruction in &self.instructions {
            stats.instructions_executed += 1;
            match instruction {
                ComputeInstruction::Constant { dst, value } => {
                    registers[dst.index()] = value.clone();
                }
                ComputeInstruction::LoadSignal {
                    dst,
                    signal_index,
                    kind,
                } => {
                    let value = signals
                        .get(*signal_index as usize)
                        .ok_or(ComputeVmError::MissingSignal(*signal_index))?;
                    if value.value_kind() != *kind {
                        return Err(ComputeVmError::SignalTypeMismatch {
                            signal_index: *signal_index,
                            expected: *kind,
                            actual: value.value_kind(),
                        });
                    }
                    registers[dst.index()] = value.clone();
                }
                ComputeInstruction::Add {
                    dst,
                    lhs,
                    rhs,
                    kind,
                } => {
                    registers[dst.index()] = binary_add(
                        &registers[lhs.index()],
                        &registers[rhs.index()],
                        *kind,
                    )?;
                }
                ComputeInstruction::Sub {
                    dst,
                    lhs,
                    rhs,
                    kind,
                } => {
                    registers[dst.index()] = binary_sub(
                        &registers[lhs.index()],
                        &registers[rhs.index()],
                        *kind,
                    )?;
                }
                ComputeInstruction::Mul {
                    dst,
                    lhs,
                    rhs,
                    result_kind,
                } => {
                    registers[dst.index()] = binary_mul(
                        &registers[lhs.index()],
                        &registers[rhs.index()],
                        *result_kind,
                    )?;
                }
                ComputeInstruction::Neg { dst, value, kind } => {
                    registers[dst.index()] = match (&registers[value.index()], kind) {
                        (ReactiveValue::Scalar(value), ValueKind::Scalar) => {
                            ReactiveValue::Scalar(-value)
                        }
                        (ReactiveValue::Vec2(value), ValueKind::Vec2) => {
                            ReactiveValue::Vec2(-*value)
                        }
                        _ => return Err(ComputeVmError::InvalidTypedInstruction("neg")),
                    };
                }
                ComputeInstruction::Sin { dst, value } => {
                    let ReactiveValue::Scalar(value) = registers[value.index()] else {
                        return Err(ComputeVmError::InvalidTypedInstruction("sin"));
                    };
                    registers[dst.index()] = ReactiveValue::Scalar(value.sin());
                }
                ComputeInstruction::Cos { dst, value } => {
                    let ReactiveValue::Scalar(value) = registers[value.index()] else {
                        return Err(ComputeVmError::InvalidTypedInstruction("cos"));
                    };
                    registers[dst.index()] = ReactiveValue::Scalar(value.cos());
                }
            }
        }

        Ok((registers[self.output.index()].clone(), stats))
    }
}

struct ComputeBuilder<'a> {
    instructions: Vec<ComputeInstruction>,
    register_kinds: Vec<ValueKind>,
    signal_indices: &'a BTreeMap<SignalId, usize>,
    signal_kinds: &'a [ValueKind],
    owner: SignalId,
}

impl ComputeBuilder<'_> {
    fn allocate(&mut self, kind: ValueKind) -> Result<ComputeRegister, ReactiveError> {
        let raw = u32::try_from(self.register_kinds.len()).map_err(|_| {
            ReactiveError::InvalidExpression {
                signal: self.owner,
                operation: "register_space_exhausted",
            }
        })?;
        self.register_kinds.push(kind);
        Ok(ComputeRegister::new(raw))
    }

    fn lower(
        &mut self,
        expression: &ReactiveExpr,
    ) -> Result<(ComputeRegister, ValueKind), ReactiveError> {
        match expression {
            ReactiveExpr::Constant(value) => {
                let kind = value.value_kind();
                let dst = self.allocate(kind)?;
                self.instructions.push(ComputeInstruction::Constant {
                    dst,
                    value: value.clone(),
                });
                Ok((dst, kind))
            }
            ReactiveExpr::Signal(signal) => {
                let index = *self
                    .signal_indices
                    .get(signal)
                    .ok_or(ReactiveError::UnknownSignal(*signal))?;
                let kind = *self
                    .signal_kinds
                    .get(index)
                    .ok_or(ReactiveError::UnknownSignal(*signal))?;
                let signal_index = u32::try_from(index).map_err(|_| {
                    ReactiveError::InvalidExpression {
                        signal: self.owner,
                        operation: "signal_space_exhausted",
                    }
                })?;
                let dst = self.allocate(kind)?;
                self.instructions.push(ComputeInstruction::LoadSignal {
                    dst,
                    signal_index,
                    kind,
                });
                Ok((dst, kind))
            }
            ReactiveExpr::Add(lhs, rhs) => {
                let (lhs, lhs_kind) = self.lower(lhs)?;
                let (rhs, rhs_kind) = self.lower(rhs)?;
                if lhs_kind != rhs_kind || !matches!(lhs_kind, ValueKind::Scalar | ValueKind::Vec2)
                {
                    return self.invalid("add");
                }
                let dst = self.allocate(lhs_kind)?;
                self.instructions.push(ComputeInstruction::Add {
                    dst,
                    lhs,
                    rhs,
                    kind: lhs_kind,
                });
                Ok((dst, lhs_kind))
            }
            ReactiveExpr::Sub(lhs, rhs) => {
                let (lhs, lhs_kind) = self.lower(lhs)?;
                let (rhs, rhs_kind) = self.lower(rhs)?;
                if lhs_kind != rhs_kind || !matches!(lhs_kind, ValueKind::Scalar | ValueKind::Vec2)
                {
                    return self.invalid("sub");
                }
                let dst = self.allocate(lhs_kind)?;
                self.instructions.push(ComputeInstruction::Sub {
                    dst,
                    lhs,
                    rhs,
                    kind: lhs_kind,
                });
                Ok((dst, lhs_kind))
            }
            ReactiveExpr::Mul(lhs, rhs) => {
                let (lhs, lhs_kind) = self.lower(lhs)?;
                let (rhs, rhs_kind) = self.lower(rhs)?;
                let result_kind = match (lhs_kind, rhs_kind) {
                    (ValueKind::Scalar, ValueKind::Scalar) => ValueKind::Scalar,
                    (ValueKind::Scalar, ValueKind::Vec2)
                    | (ValueKind::Vec2, ValueKind::Scalar) => ValueKind::Vec2,
                    _ => return self.invalid("mul"),
                };
                let dst = self.allocate(result_kind)?;
                self.instructions.push(ComputeInstruction::Mul {
                    dst,
                    lhs,
                    rhs,
                    result_kind,
                });
                Ok((dst, result_kind))
            }
            ReactiveExpr::Neg(value) => {
                let (value, kind) = self.lower(value)?;
                if !matches!(kind, ValueKind::Scalar | ValueKind::Vec2) {
                    return self.invalid("neg");
                }
                let dst = self.allocate(kind)?;
                self.instructions
                    .push(ComputeInstruction::Neg { dst, value, kind });
                Ok((dst, kind))
            }
            ReactiveExpr::Sin(value) => self.lower_unary_scalar(value, true),
            ReactiveExpr::Cos(value) => self.lower_unary_scalar(value, false),
        }
    }

    fn lower_unary_scalar(
        &mut self,
        value: &ReactiveExpr,
        sine: bool,
    ) -> Result<(ComputeRegister, ValueKind), ReactiveError> {
        let (value, kind) = self.lower(value)?;
        if kind != ValueKind::Scalar {
            return self.invalid(if sine { "sin" } else { "cos" });
        }
        let dst = self.allocate(ValueKind::Scalar)?;
        self.instructions.push(if sine {
            ComputeInstruction::Sin { dst, value }
        } else {
            ComputeInstruction::Cos { dst, value }
        });
        Ok((dst, ValueKind::Scalar))
    }

    fn invalid<T>(&self, operation: &'static str) -> Result<T, ReactiveError> {
        Err(ReactiveError::InvalidExpression {
            signal: self.owner,
            operation,
        })
    }
}

fn binary_add(
    lhs: &ReactiveValue,
    rhs: &ReactiveValue,
    kind: ValueKind,
) -> Result<ReactiveValue, ComputeVmError> {
    match (lhs, rhs, kind) {
        (ReactiveValue::Scalar(lhs), ReactiveValue::Scalar(rhs), ValueKind::Scalar) => {
            Ok(ReactiveValue::Scalar(lhs + rhs))
        }
        (ReactiveValue::Vec2(lhs), ReactiveValue::Vec2(rhs), ValueKind::Vec2) => {
            Ok(ReactiveValue::Vec2(*lhs + *rhs))
        }
        _ => Err(ComputeVmError::InvalidTypedInstruction("add")),
    }
}

fn binary_sub(
    lhs: &ReactiveValue,
    rhs: &ReactiveValue,
    kind: ValueKind,
) -> Result<ReactiveValue, ComputeVmError> {
    match (lhs, rhs, kind) {
        (ReactiveValue::Scalar(lhs), ReactiveValue::Scalar(rhs), ValueKind::Scalar) => {
            Ok(ReactiveValue::Scalar(lhs - rhs))
        }
        (ReactiveValue::Vec2(lhs), ReactiveValue::Vec2(rhs), ValueKind::Vec2) => {
            Ok(ReactiveValue::Vec2(*lhs - *rhs))
        }
        _ => Err(ComputeVmError::InvalidTypedInstruction("sub")),
    }
}

fn binary_mul(
    lhs: &ReactiveValue,
    rhs: &ReactiveValue,
    result_kind: ValueKind,
) -> Result<ReactiveValue, ComputeVmError> {
    match (lhs, rhs, result_kind) {
        (ReactiveValue::Scalar(lhs), ReactiveValue::Scalar(rhs), ValueKind::Scalar) => {
            Ok(ReactiveValue::Scalar(lhs * rhs))
        }
        (ReactiveValue::Scalar(lhs), ReactiveValue::Vec2(rhs), ValueKind::Vec2) => {
            Ok(ReactiveValue::Vec2(*lhs * *rhs))
        }
        (ReactiveValue::Vec2(lhs), ReactiveValue::Scalar(rhs), ValueKind::Vec2) => {
            Ok(ReactiveValue::Vec2(*lhs * *rhs))
        }
        _ => Err(ComputeVmError::InvalidTypedInstruction("mul")),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComputeVmError {
    MissingSignal(u32),
    SignalTypeMismatch {
        signal_index: u32,
        expected: ValueKind,
        actual: ValueKind,
    },
    InvalidTypedInstruction(&'static str),
}

impl std::fmt::Display for ComputeVmError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingSignal(index) => {
                write!(formatter, "compute program references missing dense signal {index}")
            }
            Self::SignalTypeMismatch {
                signal_index,
                expected,
                actual,
            } => write!(
                formatter,
                "compute signal {signal_index} type mismatch: expected {expected:?}, got {actual:?}"
            ),
            Self::InvalidTypedInstruction(operation) => {
                write!(formatter, "invalid typed compute instruction {operation}")
            }
        }
    }
}

impl std::error::Error for ComputeVmError {}

/// Dense dirty scheduler used by native reactive execution.
///
/// A bitset prevents duplicate queue entries while a compact min-heap preserves
/// topological rank. Semantic IDs and ordered maps never participate in scheduling.
#[derive(Clone, Debug)]
pub struct DenseDirtyQueue {
    pending: Vec<bool>,
    heap: BinaryHeap<Reverse<(usize, usize)>>,
}

impl DenseDirtyQueue {
    pub fn new(node_count: usize) -> Self {
        Self {
            pending: vec![false; node_count],
            heap: BinaryHeap::new(),
        }
    }

    pub fn schedule(&mut self, rank: usize, index: usize) {
        if !self.pending[index] {
            self.pending[index] = true;
            self.heap.push(Reverse((rank, index)));
        }
    }

    pub fn pop(&mut self) -> Option<(usize, usize)> {
        let Reverse((rank, index)) = self.heap.pop()?;
        self.pending[index] = false;
        Some((rank, index))
    }

    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    pub fn clear(&mut self) {
        while let Some(Reverse((_, index))) = self.heap.pop() {
            self.pending[index] = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vec2;

    fn signal_table() -> (
        BTreeMap<SignalId, usize>,
        Vec<ValueKind>,
        Vec<ReactiveValue>,
    ) {
        let a = SignalId::new(1);
        let b = SignalId::new(2);
        (
            BTreeMap::from([(a, 0), (b, 1)]),
            vec![ValueKind::Scalar, ValueKind::Vec2],
            vec![
                ReactiveValue::Scalar(2.0),
                ReactiveValue::Vec2(Vec2::new(3.0, 4.0)),
            ],
        )
    }

    #[test]
    fn lowers_expression_to_dense_typed_register_program() {
        let (indices, kinds, values) = signal_table();
        let expression = ReactiveExpr::Add(
            Box::new(ReactiveExpr::Mul(
                Box::new(ReactiveExpr::signal(SignalId::new(1))),
                Box::new(ReactiveExpr::scalar(3.0)),
            )),
            Box::new(ReactiveExpr::scalar(1.0)),
        );
        let program =
            ComputeProgram::lower(&expression, &indices, &kinds, SignalId::new(9)).unwrap();
        assert_eq!(program.output_kind(), ValueKind::Scalar);
        assert!(program.instructions().iter().any(|instruction| matches!(
            instruction,
            ComputeInstruction::LoadSignal {
                signal_index: 0,
                ..
            }
        )));
        let (value, stats) = program.evaluate_with_stats(&values).unwrap();
        assert_eq!(value, ReactiveValue::Scalar(7.0));
        assert_eq!(stats.instructions_executed, program.instructions().len());
        assert_eq!(program.debug_text(), program.debug_text());
    }

    #[test]
    fn typed_vm_supports_scalar_vector_multiplication() {
        let (indices, kinds, values) = signal_table();
        let expression = ReactiveExpr::Mul(
            Box::new(ReactiveExpr::signal(SignalId::new(1))),
            Box::new(ReactiveExpr::signal(SignalId::new(2))),
        );
        let program =
            ComputeProgram::lower(&expression, &indices, &kinds, SignalId::new(9)).unwrap();
        assert_eq!(
            program.evaluate(&values).unwrap(),
            ReactiveValue::Vec2(Vec2::new(6.0, 8.0))
        );
    }

    #[test]
    fn lowering_rejects_invalid_types_before_execution() {
        let indices = BTreeMap::from([(SignalId::new(1), 0)]);
        let kinds = vec![ValueKind::Bool];
        let expression = ReactiveExpr::Sin(Box::new(ReactiveExpr::signal(SignalId::new(1))));
        assert!(matches!(
            ComputeProgram::lower(&expression, &indices, &kinds, SignalId::new(7)),
            Err(ReactiveError::InvalidExpression {
                operation: "sin",
                ..
            })
        ));
    }

    #[test]
    fn dense_dirty_queue_deduplicates_and_preserves_rank() {
        let mut queue = DenseDirtyQueue::new(8);
        queue.schedule(5, 5);
        queue.schedule(2, 2);
        queue.schedule(5, 5);
        queue.schedule(3, 7);
        assert_eq!(queue.pop(), Some((2, 2)));
        assert_eq!(queue.pop(), Some((3, 7)));
        assert_eq!(queue.pop(), Some((5, 5)));
        assert!(queue.is_empty());
    }

    #[test]
    fn vm_matches_reference_math_over_many_inputs() {
        let signal = SignalId::new(1);
        let indices = BTreeMap::from([(signal, 0)]);
        let kinds = vec![ValueKind::Scalar];
        let expression = ReactiveExpr::Add(
            Box::new(ReactiveExpr::Sin(Box::new(ReactiveExpr::signal(signal)))),
            Box::new(ReactiveExpr::Mul(
                Box::new(ReactiveExpr::signal(signal)),
                Box::new(ReactiveExpr::scalar(0.25)),
            )),
        );
        let program =
            ComputeProgram::lower(&expression, &indices, &kinds, SignalId::new(8)).unwrap();
        for step in -100..=100 {
            let x = step as f32 * 0.03125;
            let actual = program.evaluate(&[ReactiveValue::Scalar(x)]).unwrap();
            let expected = ReactiveValue::Scalar(x.sin() + x * 0.25);
            assert_eq!(actual, expected);
        }
    }
}
