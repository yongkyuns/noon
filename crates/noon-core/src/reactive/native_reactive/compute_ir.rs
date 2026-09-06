use serde::{Deserialize, Serialize};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{SignalId, ValueKind, Vec2};

use super::{
    validate_reactive_value, ReactiveError, ReactiveEvaluationStats, ReactiveExpr, ReactiveProgram,
    ReactivePropertyChange, ReactiveUpdate, ReactiveValue, SignalChange, SignalSource,
};

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
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
///
/// The instruction/register representation derives serde so tools and future
/// backends can consume one deterministic language-neutral form.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComputeKernel {
    owner: SignalId,
    instructions: Vec<ComputeInstruction>,
    register_kinds: Vec<ComputeValueKind>,
    output: ComputeRegister,
}

impl ComputeKernel {
    pub const fn owner(&self) -> SignalId {
        self.owner
    }

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

    fn evaluate(
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

/// Validated reactive program lowered into dense typed kernels.
///
/// `ReactiveProgram` remains the reference semantic evaluator. This representation
/// is the execution form used by the runtime and is intentionally suitable for a
/// later SIMD or WGSL backend without changing the authoring expression language.
#[derive(Clone, Debug, PartialEq)]
pub struct ComputeProgram {
    reference: ReactiveProgram,
    kernels: Vec<Option<ComputeKernel>>,
    max_registers: usize,
}

impl ReactiveProgram {
    /// Lower this validated reactive dependency graph into the typed compute IR.
    pub fn into_compute(self) -> Result<ComputeProgram, ReactiveError> {
        ComputeProgram::lower(self)
    }
}

impl ComputeProgram {
    fn lower(reference: ReactiveProgram) -> Result<Self, ReactiveError> {
        let signal_count = reference.signals.len();
        let mut kernels = vec![None; signal_count];
        let mut signal_kinds = vec![None; signal_count];
        let mut max_registers = 0;

        for index in reference.topological_order.iter().copied() {
            let signal = &reference.signals[index];
            match &signal.source {
                SignalSource::Input(value) => {
                    signal_kinds[index] = Some(ComputeValueKind::from_reactive(value));
                }
                SignalSource::Derived(expression) => {
                    let kernel = lower_reactive_expression(
                        signal.id,
                        expression,
                        &reference.signal_indices,
                        &signal_kinds,
                    )?;
                    let kind = kernel.output_kind();
                    debug_assert_eq!(
                        kind.as_value_kind(),
                        reference.initial_values[index].value_kind()
                    );
                    max_registers = max_registers.max(kernel.register_count());
                    signal_kinds[index] = Some(kind);
                    kernels[index] = Some(kernel);
                }
            }
        }

        Ok(Self {
            reference,
            kernels,
            max_registers,
        })
    }

    pub fn signal_count(&self) -> usize {
        self.reference.signal_count()
    }

    pub fn kernel(&self, signal: SignalId) -> Option<&ComputeKernel> {
        let index = *self.reference.signal_indices.get(&signal)?;
        self.kernels[index].as_ref()
    }

    pub fn kernels(&self) -> impl Iterator<Item = &ComputeKernel> {
        self.kernels.iter().filter_map(Option::as_ref)
    }

    pub fn instantiate(self) -> ComputeState {
        let signal_count = self.reference.signals.len();
        let values = self.reference.initial_values.clone();
        let max_registers = self.max_registers;
        ComputeState {
            identity: next_compute_identity(),
            revision: 0,
            program: self,
            values,
            scratch: vec![ReactiveValue::Scalar(0.0); max_registers],
            queued: vec![false; signal_count],
            pending: BinaryHeap::new(),
        }
    }
}

/// Dense native reactive VM.
///
/// Scheduling uses a dense adjacency table, a reusable queued-bitset, and a
/// vector-backed min-heap keyed by topological rank. It performs no recursive AST
/// evaluation and uses no ordered maps/sets to schedule dirty work.
#[derive(Debug)]
pub struct ComputeState {
    identity: u64,
    revision: u64,
    program: ComputeProgram,
    values: Vec<ReactiveValue>,
    scratch: Vec<ReactiveValue>,
    queued: Vec<bool>,
    pending: BinaryHeap<Reverse<(usize, usize)>>,
}

impl Clone for ComputeState {
    fn clone(&self) -> Self {
        Self {
            identity: next_compute_identity(),
            revision: self.revision,
            program: self.program.clone(),
            values: self.values.clone(),
            scratch: self.scratch.clone(),
            queued: vec![false; self.queued.len()],
            pending: BinaryHeap::new(),
        }
    }
}

static NEXT_COMPUTE_IDENTITY: AtomicU64 = AtomicU64::new(1);

fn next_compute_identity() -> u64 {
    NEXT_COMPUTE_IDENTITY
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .expect("Noon compute-state identity space exhausted")
}

#[derive(Clone, Debug)]
pub struct PreparedComputeInputBatch {
    identity: u64,
    expected_revision: u64,
    changed_values: BTreeMap<usize, ReactiveValue>,
    update: ReactiveUpdate,
}

#[derive(Clone, Debug)]
pub struct PreparedComputeInputEnrollment {
    identity: u64,
    expected_revision: u64,
    expected_signal: Option<SignalId>,
    value: ReactiveValue,
}

impl PreparedComputeInputBatch {
    pub fn update(&self) -> &ReactiveUpdate {
        &self.update
    }

    pub fn is_empty(&self) -> bool {
        self.changed_values.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreparedComputeCommitError {
    ForeignState,
    StaleRevision,
    SignalMismatch {
        expected: SignalId,
        actual: SignalId,
    },
    DuplicateSignal(SignalId),
    RevisionExhausted,
}

impl std::fmt::Display for PreparedComputeCommitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ForeignState => {
                formatter.write_str("prepared reactive batch belongs to another compute state")
            }
            Self::StaleRevision => {
                formatter.write_str("prepared reactive batch targets a stale compute revision")
            }
            Self::SignalMismatch { expected, actual } => write!(
                formatter,
                "prepared reactive enrollment reserved signal {} but commit supplied {}",
                expected.get(),
                actual.get()
            ),
            Self::DuplicateSignal(signal) => write!(
                formatter,
                "prepared reactive enrollment cannot replace existing signal {}",
                signal.get()
            ),
            Self::RevisionExhausted => {
                formatter.write_str("Noon compute-state revision space exhausted")
            }
        }
    }
}

impl std::error::Error for PreparedComputeCommitError {}

impl ComputeState {
    pub fn value(&self, signal: SignalId) -> Option<&ReactiveValue> {
        let index = self.program.reference.signal_indices.get(&signal)?;
        self.values.get(*index)
    }

    /// Observe one value through an unpublished prepared input batch.
    pub fn prepared_value(
        &self,
        prepared: &PreparedComputeInputBatch,
        signal: SignalId,
    ) -> Option<ReactiveValue> {
        if prepared.identity != self.identity || prepared.expected_revision != self.revision {
            return None;
        }
        let index = *self.program.reference.signal_indices.get(&signal)?;
        prepared
            .changed_values
            .get(&index)
            .cloned()
            .or_else(|| self.values.get(index).cloned())
    }

    /// Reserve one sparse input slot without assigning a new semantic identity.
    pub fn prepare_input_enrollment(
        &self,
        signal: Option<SignalId>,
        value: ReactiveValue,
    ) -> Result<PreparedComputeInputEnrollment, ReactiveError> {
        if let Some(signal) = signal {
            if self.program.reference.signal_indices.contains_key(&signal) {
                return Err(ReactiveError::DuplicateSignal(signal));
            }
            validate_reactive_value(signal, &value)?;
        } else if !value.is_finite() {
            return Err(ReactiveError::NonFiniteValue(SignalId::new(0)));
        }
        self.revision
            .checked_add(1)
            .ok_or(ReactiveError::ComputeRevisionExhausted)?;
        Ok(PreparedComputeInputEnrollment {
            identity: self.identity,
            expected_revision: self.revision,
            expected_signal: signal,
            value,
        })
    }

    pub fn commit_input_enrollment(
        &mut self,
        prepared: PreparedComputeInputEnrollment,
        signal: SignalId,
    ) -> Result<(), PreparedComputeCommitError> {
        if prepared.identity != self.identity {
            return Err(PreparedComputeCommitError::ForeignState);
        }
        if prepared.expected_revision != self.revision {
            return Err(PreparedComputeCommitError::StaleRevision);
        }
        if let Some(expected) = prepared.expected_signal {
            if expected != signal {
                return Err(PreparedComputeCommitError::SignalMismatch {
                    expected,
                    actual: signal,
                });
            }
        }
        if self.program.reference.signal_indices.contains_key(&signal) {
            return Err(PreparedComputeCommitError::DuplicateSignal(signal));
        }
        let index = self.program.reference.signals.len();
        self.program.reference.signal_indices.insert(signal, index);
        self.program.reference.signals.push(super::CompiledSignal {
            id: signal,
            source: SignalSource::Input(prepared.value.clone()),
        });
        self.program.reference.topological_order.push(index);
        self.program.reference.topological_rank.push(index);
        self.program.reference.dependents.push(Vec::new());
        self.program.reference.bindings_by_signal.push(Vec::new());
        self.program
            .reference
            .initial_values
            .push(prepared.value.clone());
        self.program.kernels.push(None);
        self.values.push(prepared.value);
        self.queued.push(false);
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or(PreparedComputeCommitError::RevisionExhausted)?;
        Ok(())
    }

    pub fn set_input(
        &mut self,
        signal: SignalId,
        value: impl Into<ReactiveValue>,
    ) -> Result<ReactiveUpdate, ReactiveError> {
        let prepared = self.prepare_input_batch(&[(signal, value.into())])?;
        self.commit_prepared_input_batch(prepared)
            .map_err(unreachable_same_state_commit)
    }

    /// Evaluate an ordered input batch against sparse journaled state, then restore
    /// the coherent live values. Commit reapplies only values in the dependency closure.
    pub fn prepare_input_batch(
        &mut self,
        inputs: &[(SignalId, ReactiveValue)],
    ) -> Result<PreparedComputeInputBatch, ReactiveError> {
        debug_assert!(self.pending.is_empty());
        let mut staged_inputs = BTreeMap::new();
        for (signal, value) in inputs {
            let index = *self
                .program
                .reference
                .signal_indices
                .get(signal)
                .ok_or(ReactiveError::UnknownSignal(*signal))?;
            if !matches!(
                self.program.reference.signals[index].source,
                SignalSource::Input(_)
            ) {
                return Err(ReactiveError::NotInputSignal(*signal));
            }
            validate_reactive_value(*signal, value)?;
            let expected = self.values[index].value_kind();
            let actual = value.value_kind();
            if expected != actual {
                return Err(ReactiveError::InputTypeMismatch {
                    signal: *signal,
                    expected,
                    actual,
                });
            }
            staged_inputs.insert(index, value.clone());
        }
        staged_inputs.retain(|index, value| self.values[*index] != *value);
        if staged_inputs.is_empty() {
            return Ok(PreparedComputeInputBatch {
                identity: self.identity,
                expected_revision: self.revision,
                changed_values: BTreeMap::new(),
                update: ReactiveUpdate::default(),
            });
        }
        if self.revision.checked_add(1).is_none() {
            return Err(ReactiveError::ComputeRevisionExhausted);
        }

        let mut affected = std::collections::BTreeSet::new();
        let mut stack = Vec::new();
        for &index in staged_inputs.keys() {
            stack.push(index);
        }
        while let Some(index) = stack.pop() {
            if !affected.insert(index) {
                continue;
            }
            stack.extend(self.program.reference.dependents[index].iter().copied());
        }
        let journal = affected
            .iter()
            .map(|&index| (index, self.values[index].clone()))
            .collect::<Vec<_>>();
        let mut changed = Vec::new();
        for (index, value) in staged_inputs {
            if self.values[index] == value {
                continue;
            }
            self.values[index] = value;
            changed.push(index);
            enqueue_dependents(
                &self.program.reference.dependents[index],
                &self.program.reference.topological_rank,
                &mut self.queued,
                &mut self.pending,
            );
        }

        let mut stats = ReactiveEvaluationStats::default();
        let evaluation = (|| {
            while let Some(Reverse((rank, current))) = self.pending.pop() {
                self.queued[current] = false;
                debug_assert_eq!(rank, self.program.reference.topological_rank[current]);
                let kernel = self.program.kernels[current]
                    .as_ref()
                    .expect("only derived signals can be scheduled");
                stats.derived_signals_evaluated += 1;
                let signal_id = self.program.reference.signals[current].id;
                let next = kernel.evaluate(&self.values, &mut self.scratch)?;
                validate_reactive_value(signal_id, &next)?;
                if self.values[current] == next {
                    continue;
                }
                self.values[current] = next;
                changed.push(current);
                enqueue_dependents(
                    &self.program.reference.dependents[current],
                    &self.program.reference.topological_rank,
                    &mut self.queued,
                    &mut self.pending,
                );
            }
            Ok::<(), ReactiveError>(())
        })();

        if let Err(error) = evaluation {
            while let Some(Reverse((_, index))) = self.pending.pop() {
                self.queued[index] = false;
            }
            for (index, value) in journal {
                self.values[index] = value;
            }
            return Err(error);
        }

        changed.sort_by_key(|index| self.program.reference.topological_rank[*index]);
        changed.dedup();
        let mut signal_changes = Vec::with_capacity(changed.len());
        let mut property_changes = Vec::new();
        for &changed_index in &changed {
            let changed_signal = self.program.reference.signals[changed_index].id;
            let changed_value = self.values[changed_index].clone();
            signal_changes.push(SignalChange {
                signal: changed_signal,
                value: changed_value.clone(),
            });
            for binding in &self.program.reference.bindings_by_signal[changed_index] {
                property_changes.push(ReactivePropertyChange {
                    object: binding.object,
                    property: binding.property,
                    value: changed_value.clone(),
                });
            }
        }
        stats.bindings_invalidated = property_changes.len();
        let update = ReactiveUpdate {
            signal_changes,
            property_changes,
            stats,
        };
        let changed_values = journal
            .iter()
            .filter(|(index, previous)| self.values[*index] != *previous)
            .map(|(index, _)| (*index, self.values[*index].clone()))
            .collect();
        for (index, value) in journal {
            self.values[index] = value;
        }
        Ok(PreparedComputeInputBatch {
            identity: self.identity,
            expected_revision: self.revision,
            changed_values,
            update,
        })
    }

    pub fn commit_prepared_input_batch(
        &mut self,
        prepared: PreparedComputeInputBatch,
    ) -> Result<ReactiveUpdate, PreparedComputeCommitError> {
        if prepared.identity != self.identity {
            return Err(PreparedComputeCommitError::ForeignState);
        }
        if prepared.expected_revision != self.revision {
            return Err(PreparedComputeCommitError::StaleRevision);
        }
        let changed = !prepared.changed_values.is_empty();
        let next_revision = if changed {
            Some(
                self.revision
                    .checked_add(1)
                    .ok_or(PreparedComputeCommitError::RevisionExhausted)?,
            )
        } else {
            None
        };
        for (index, value) in prepared.changed_values {
            self.values[index] = value;
        }
        if let Some(revision) = next_revision {
            self.revision = revision;
        }
        Ok(prepared.update)
    }
}

fn unreachable_same_state_commit(_: PreparedComputeCommitError) -> ReactiveError {
    unreachable!("a batch prepared and immediately committed on one compute state must be valid")
}

fn enqueue_dependents(
    dependents: &[usize],
    topological_rank: &[usize],
    queued: &mut [bool],
    pending: &mut BinaryHeap<Reverse<(usize, usize)>>,
) {
    for &signal_index in dependents {
        if queued[signal_index] {
            continue;
        }
        queued[signal_index] = true;
        pending.push(Reverse((topological_rank[signal_index], signal_index)));
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
                let signal_index =
                    u32::try_from(signal_index).expect("reactive signal index space exhausted");
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
                let instruction = match (lhs_kind, rhs_kind) {
                    (ComputeValueKind::Scalar, ComputeValueKind::Scalar) => {
                        let dst = self.register(ComputeValueKind::Scalar);
                        ComputeInstruction::MulScalar { dst, lhs, rhs }
                    }
                    (ComputeValueKind::Scalar, ComputeValueKind::Vec2) => {
                        let dst = self.register(ComputeValueKind::Vec2);
                        ComputeInstruction::MulScalarVec2 {
                            dst,
                            scalar: lhs,
                            vector: rhs,
                        }
                    }
                    (ComputeValueKind::Vec2, ComputeValueKind::Scalar) => {
                        let dst = self.register(ComputeValueKind::Vec2);
                        ComputeInstruction::MulVec2Scalar {
                            dst,
                            vector: lhs,
                            scalar: rhs,
                        }
                    }
                    _ => return Err(self.invalid("mul")),
                };
                let dst = match &instruction {
                    ComputeInstruction::MulScalar { dst, .. }
                    | ComputeInstruction::MulScalarVec2 { dst, .. }
                    | ComputeInstruction::MulVec2Scalar { dst, .. } => *dst,
                    _ => unreachable!(),
                };
                self.instructions.push(instruction);
                Ok(dst)
            }
            ReactiveExpr::Neg(value) => {
                let value = self.lower(value)?;
                let kind = self.register_kinds[value.index()];
                if kind == ComputeValueKind::Bool {
                    return Err(self.invalid("neg"));
                }
                let dst = self.register(kind);
                self.instructions.push(match kind {
                    ComputeValueKind::Scalar => ComputeInstruction::NegScalar { dst, value },
                    ComputeValueKind::Vec2 => ComputeInstruction::NegVec2 { dst, value },
                    ComputeValueKind::Bool => unreachable!(),
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

fn lower_reactive_expression(
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

#[cfg(test)]
mod tests {
    use crate::{GeometryRef, Property, SemanticScene};

    use super::*;

    #[test]
    fn lowers_scalar_vector_expression_to_typed_flat_ir() {
        let mut scene = SemanticScene::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let scalar = scene.add_input(2.0_f32);
        let vector = scene.add_input(Vec2::new(3.0, 4.0));
        let result = scene.add_derived(ReactiveExpr::Mul(
            Box::new(ReactiveExpr::signal(scalar)),
            Box::new(ReactiveExpr::signal(vector)),
        ));
        scene.bind(result, object, Property::Position);

        let program = scene.compile_reactive().unwrap().into_compute().unwrap();
        let kernel = program.kernel(result).expect("derived signal has a kernel");
        assert_eq!(kernel.output_kind(), ComputeValueKind::Vec2);
        assert_eq!(kernel.instructions().len(), 3);
        assert!(matches!(
            kernel.instructions()[0],
            ComputeInstruction::LoadScalar { .. }
        ));
        assert!(matches!(
            kernel.instructions()[1],
            ComputeInstruction::LoadVec2 { .. }
        ));
        assert!(matches!(
            kernel.instructions()[2],
            ComputeInstruction::MulScalarVec2 { .. }
        ));
        assert!(kernel
            .instructions()
            .iter()
            .all(ComputeInstruction::is_backend_lowerable));
    }

    #[test]
    fn dense_vm_matches_reference_interpreter_on_generated_graphs() {
        for seed in 1_u64..=24 {
            let mut random = seed;
            let mut scene = SemanticScene::new();
            let object = scene.add(GeometryRef::circle(1.0));
            let first = scene.add_input(0.25_f32);
            let second = scene.add_input(-0.75_f32);
            let mut signals = vec![first, second];

            for _ in 0..32 {
                let lhs = signals[next(&mut random) as usize % signals.len()];
                let rhs = signals[next(&mut random) as usize % signals.len()];
                let expression = match next(&mut random) % 6 {
                    0 => ReactiveExpr::Add(
                        Box::new(ReactiveExpr::signal(lhs)),
                        Box::new(ReactiveExpr::signal(rhs)),
                    ),
                    1 => ReactiveExpr::Sub(
                        Box::new(ReactiveExpr::signal(lhs)),
                        Box::new(ReactiveExpr::signal(rhs)),
                    ),
                    2 => ReactiveExpr::Mul(
                        Box::new(ReactiveExpr::signal(lhs)),
                        Box::new(ReactiveExpr::scalar(0.25)),
                    ),
                    3 => ReactiveExpr::Neg(Box::new(ReactiveExpr::signal(lhs))),
                    4 => ReactiveExpr::Sin(Box::new(ReactiveExpr::signal(lhs))),
                    _ => ReactiveExpr::Cos(Box::new(ReactiveExpr::signal(lhs))),
                };
                signals.push(scene.add_derived(expression));
            }
            scene.bind(*signals.last().unwrap(), object, Property::Rotation);

            let program = scene.compile_reactive().expect("generated graph compiles");
            let mut reference = program.instantiate();
            let mut compute = program.into_compute().unwrap().instantiate();

            for update_index in 0..16 {
                let target = if update_index % 2 == 0 { first } else { second };
                let raw = (next(&mut random) % 2001) as f32;
                let value = (raw - 1000.0) / 1000.0;
                let reference_update = reference.set_input(target, value).unwrap();
                let compute_update = compute.set_input(target, value).unwrap();
                assert_eq!(compute_update, reference_update, "seed={seed}");
                for signal in &signals {
                    assert_eq!(
                        compute.value(*signal),
                        reference.value(*signal),
                        "seed={seed}"
                    );
                }
            }
        }
    }

    #[test]
    fn dense_vm_preserves_change_stopping() {
        let mut scene = SemanticScene::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let input = scene.add_input(1.0_f32);
        let zero = scene.add_derived(ReactiveExpr::Mul(
            Box::new(ReactiveExpr::signal(input)),
            Box::new(ReactiveExpr::scalar(0.0)),
        ));
        let downstream = scene.add_derived(ReactiveExpr::Add(
            Box::new(ReactiveExpr::signal(zero)),
            Box::new(ReactiveExpr::scalar(1.0)),
        ));
        scene.bind(downstream, object, Property::Opacity);

        let mut state = scene
            .compile_reactive()
            .unwrap()
            .into_compute()
            .unwrap()
            .instantiate();
        let update = state.set_input(input, 42.0_f32).unwrap();
        assert_eq!(update.stats().derived_signals_evaluated, 1);
        assert_eq!(update.stats().bindings_invalidated, 0);
        assert_eq!(state.value(downstream), Some(&ReactiveValue::Scalar(1.0)));
    }

    #[test]
    fn prepared_batch_stages_all_inputs_before_one_shared_closure_evaluation() {
        let mut scene = SemanticScene::new();
        let first = scene.add_input(0.0_f32);
        let second = scene.add_input(0.0_f32);
        let sum = scene.add_derived(ReactiveExpr::Add(
            Box::new(ReactiveExpr::signal(first)),
            Box::new(ReactiveExpr::signal(second)),
        ));
        let mut state = scene
            .compile_reactive()
            .unwrap()
            .into_compute()
            .unwrap()
            .instantiate();

        let prepared = state
            .prepare_input_batch(&[
                (first, ReactiveValue::Scalar(2.0)),
                (second, ReactiveValue::Scalar(3.0)),
            ])
            .unwrap();
        assert_eq!(prepared.update().stats().derived_signals_evaluated, 1);
        assert_eq!(state.value(sum), Some(&ReactiveValue::Scalar(0.0)));
        state.commit_prepared_input_batch(prepared).unwrap();
        assert_eq!(state.value(sum), Some(&ReactiveValue::Scalar(5.0)));
    }

    #[test]
    fn prepared_batch_failure_restores_values_and_scheduler_scratch() {
        let mut scene = SemanticScene::new();
        let first = scene.add_input(1.0_f32);
        let second = scene.add_input(1.0_f32);
        let sum = scene.add_derived(ReactiveExpr::Add(
            Box::new(ReactiveExpr::signal(first)),
            Box::new(ReactiveExpr::signal(second)),
        ));
        let square = scene.add_derived(ReactiveExpr::Mul(
            Box::new(ReactiveExpr::signal(sum)),
            Box::new(ReactiveExpr::signal(sum)),
        ));
        let downstream =
            scene.add_derived(ReactiveExpr::Sin(Box::new(ReactiveExpr::signal(square))));
        let mut state = scene
            .compile_reactive()
            .unwrap()
            .into_compute()
            .unwrap()
            .instantiate();

        assert!(matches!(
            state.prepare_input_batch(&[
                (first, ReactiveValue::Scalar(1.0e20)),
                (second, ReactiveValue::Scalar(1.0e20)),
            ]),
            Err(ReactiveError::NonFiniteValue(signal)) if signal == square
        ));
        assert_eq!(state.value(first), Some(&ReactiveValue::Scalar(1.0)));
        assert_eq!(
            state.value(downstream),
            Some(&ReactiveValue::Scalar(4.0_f32.sin()))
        );

        let prepared = state
            .prepare_input_batch(&[
                (first, ReactiveValue::Scalar(2.0)),
                (second, ReactiveValue::Scalar(3.0)),
            ])
            .unwrap();
        state.commit_prepared_input_batch(prepared).unwrap();
        assert_eq!(state.value(sum), Some(&ReactiveValue::Scalar(5.0)));
        assert_eq!(state.value(square), Some(&ReactiveValue::Scalar(25.0)));
        assert_eq!(
            state.value(downstream),
            Some(&ReactiveValue::Scalar(25.0_f32.sin()))
        );
    }

    #[test]
    fn prepared_batches_are_bound_to_one_compute_incarnation_and_revision() {
        let mut scene = SemanticScene::new();
        let input = scene.add_input(0.0_f32);
        let program = scene.compile_reactive().unwrap().into_compute().unwrap();
        let mut first = program.clone().instantiate();
        let mut second = program.instantiate();
        let foreign = first
            .prepare_input_batch(&[(input, ReactiveValue::Scalar(1.0))])
            .unwrap();
        assert_eq!(
            second.commit_prepared_input_batch(foreign),
            Err(PreparedComputeCommitError::ForeignState)
        );

        let current = first
            .prepare_input_batch(&[(input, ReactiveValue::Scalar(2.0))])
            .unwrap();
        let stale = first
            .prepare_input_batch(&[(input, ReactiveValue::Scalar(3.0))])
            .unwrap();
        first.commit_prepared_input_batch(current).unwrap();
        assert_eq!(
            first.commit_prepared_input_batch(stale),
            Err(PreparedComputeCommitError::StaleRevision)
        );
    }

    #[test]
    fn prepared_enrollment_rejects_a_different_signal_without_mutation() {
        let scene = SemanticScene::new();
        let mut state = scene
            .compile_reactive()
            .unwrap()
            .into_compute()
            .unwrap()
            .instantiate();
        let expected = SignalId::new(41);
        let actual = SignalId::new(42);
        let prepared = state
            .prepare_input_enrollment(Some(expected), ReactiveValue::Scalar(2.0))
            .unwrap();

        assert_eq!(
            state.commit_input_enrollment(prepared, actual),
            Err(PreparedComputeCommitError::SignalMismatch { expected, actual })
        );
        assert_eq!(state.value(expected), None);
        assert_eq!(state.value(actual), None);
    }

    #[test]
    fn prepared_enrollment_rejects_an_existing_signal_without_mutation() {
        let mut scene = SemanticScene::new();
        let existing = scene.add_input(1.0_f32);
        let mut state = scene
            .compile_reactive()
            .unwrap()
            .into_compute()
            .unwrap()
            .instantiate();
        let prepared = state
            .prepare_input_enrollment(None, ReactiveValue::Scalar(2.0))
            .unwrap();

        assert_eq!(
            state.commit_input_enrollment(prepared, existing),
            Err(PreparedComputeCommitError::DuplicateSignal(existing))
        );
        assert_eq!(state.value(existing), Some(&ReactiveValue::Scalar(1.0)));
    }

    fn next(state: &mut u64) -> u64 {
        *state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *state
    }
}
