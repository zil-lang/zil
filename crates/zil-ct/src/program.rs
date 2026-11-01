//! A bounded explicit-stack VM for independently verified pure value CFG.
use zil_base::{CompileResult, Diagnostic, Phase};
use zil_ir::{Constant, InstructionKind, Terminator, Type, TypedFunction, ValueId};
use zil_verify::VerifiedCTProgram;

/// Serializable data only. No member can carry a VM address or a runtime capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileTimeValue {
    Scalar(Constant),
    Aggregate(Vec<CompileTimeValue>),
    Variant {
        variant: u32,
        fields: Vec<CompileTimeValue>,
    },
}
impl CompileTimeValue {
    fn scalar(&self) -> &Constant {
        let Self::Scalar(value) = self else {
            unreachable!("verified scalar operand")
        };
        value
    }
    fn fields(&self) -> &[Self] {
        let Self::Aggregate(fields) = self else {
            unreachable!("verified aggregate operand")
        };
        fields
    }
    fn variant(&self) -> (u32, &[Self]) {
        let Self::Variant { variant, fields } = self else {
            unreachable!("verified enum operand")
        };
        (*variant, fields)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramEvaluation {
    pub value: CompileTimeValue,
    pub steps: u64,
    pub peak_live_slots: usize,
    pub peak_call_depth: usize,
}
struct Shape {
    slots: usize,
    values: Vec<usize>,
}
struct Frame<'a> {
    function: &'a TypedFunction,
    shape: &'a Shape,
    locals: Vec<Option<CompileTimeValue>>,
    values: Vec<Option<CompileTimeValue>>,
    block: usize,
    instruction: usize,
    destination: Option<ValueId>,
}
impl<'a> Frame<'a> {
    fn new(
        function: &'a TypedFunction,
        shape: &'a Shape,
        arguments: Vec<CompileTimeValue>,
        destination: Option<ValueId>,
    ) -> Self {
        let mut locals = vec![None; function.locals.len()];
        for (local, value) in function.parameters.iter().zip(arguments) {
            locals[local.0 as usize] = Some(value);
        }
        Self {
            function,
            shape,
            locals,
            values: vec![None; function.values.len()],
            block: function.entry.0 as usize,
            instruction: 0,
            destination,
        }
    }
    fn value(&self, id: ValueId) -> &CompileTimeValue {
        self.values[id.0 as usize]
            .as_ref()
            .expect("verified definition dominates use")
    }
}
fn quota(message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::limit(Phase::Ct, message, None)]
}
fn cells(ty: &Type, cap: usize) -> CompileResult<usize> {
    let add = |a: usize, b: usize| {
        a.checked_add(b)
            .filter(|n| *n <= cap)
            .ok_or_else(|| quota("CT aggregate cell budget exhausted"))
    };
    match ty {
        Type::Unit | Type::Bool | Type::U8 | Type::U16 | Type::I32 | Type::I64 => Ok(1),
        Type::Tuple(fields) => fields
            .iter()
            .try_fold(1usize, |n, t| add(n, cells(t, cap)?)),
        Type::Record { fields, .. } => fields
            .iter()
            .try_fold(1usize, |n, (_, t)| add(n, cells(t, cap)?)),
        Type::Array { element, length } => add(
            1,
            cells(element, cap)?
                .checked_mul(*length as usize)
                .ok_or_else(|| quota("CT array cell count overflow"))?,
        ),
        // Reserve every possible payload as well as the container and tag.
        // This static upper bound is known before any active data is allocated.
        Type::Enum { variants, .. } => variants
            .iter()
            .flat_map(|(_, fields)| fields)
            .try_fold(add(1, 1)?, |n, t| add(n, cells(t, cap)?)),
        _ => unreachable!("independent CT whitelist excludes nonserializable values"),
    }
}
fn charge(steps: &mut u64, amount: usize, maximum: u64) -> CompileResult<()> {
    *steps = steps
        .checked_add(amount as u64)
        .filter(|n| *n <= maximum)
        .ok_or_else(|| quota("CT program execution step budget exhausted"))?;
    Ok(())
}
fn index(value: &CompileTimeValue, length: usize) -> CompileResult<usize> {
    let value = match value.scalar() {
        Constant::U8(v) => i64::from(*v),
        Constant::U16(v) => i64::from(*v),
        Constant::I32(v) => i64::from(*v),
        Constant::I64(v) => *v,
        _ => unreachable!("verified integer index"),
    };
    usize::try_from(value)
        .ok()
        .filter(|i| *i < length)
        .ok_or_else(|| {
            vec![Diagnostic::error(
                Phase::Ct,
                "E-CT-BOUNDS",
                None,
                "compile-time array index is outside its bounds",
            )]
        })
}

pub fn evaluate_program(program: &VerifiedCTProgram) -> CompileResult<ProgramEvaluation> {
    let module = program.module();
    // Verified input has already bounded every type tree. Reserve expanded data
    // before allocating a frame or cloning any aggregate value.
    let shapes = module
        .functions
        .iter()
        .map(|f| {
            let values = f
                .values
                .iter()
                .map(|ty| cells(ty, program.max_live_slots()))
                .collect::<CompileResult<Vec<_>>>()?;
            let mut slots = values.iter().try_fold(0usize, |n, v| {
                n.checked_add(*v)
                    .ok_or_else(|| quota("CT frame cell overflow"))
            })?;
            for local in &f.locals {
                slots = slots
                    .checked_add(cells(&local.ty, program.max_live_slots())?)
                    .ok_or_else(|| quota("CT frame cell overflow"))?;
            }
            if slots > program.max_live_slots() {
                return Err(quota("CT live value-slot budget exhausted"));
            }
            Ok(Shape { slots, values })
        })
        .collect::<CompileResult<Vec<_>>>()?;
    let entry = &module.functions[program.entry().0 as usize];
    let shape = &shapes[program.entry().0 as usize];
    let mut live_slots = shape.slots;
    let mut peak_live_slots = live_slots;
    let mut peak_call_depth = 1;
    let mut frames = vec![Frame::new(entry, shape, Vec::new(), None)];
    let mut steps = 0u64;
    loop {
        charge(&mut steps, 1, program.max_steps())?;
        let depth = frames.len();
        let frame = frames.last_mut().expect("root frame returns from VM");
        let block = &frame.function.blocks[frame.block];
        if let Some(instruction) = block.instructions.get(frame.instruction) {
            frame.instruction += 1;
            let weight = instruction
                .result
                .map_or(1, |v| frame.shape.values[v.0 as usize]);
            let copied = match &instruction.kind {
                InstructionKind::Store { value, .. } => {
                    frame.shape.values[value.0 as usize].saturating_sub(1)
                }
                InstructionKind::Call { arguments, .. } => arguments
                    .iter()
                    .map(|v| frame.shape.values[v.0 as usize].saturating_sub(1))
                    .sum(),
                InstructionKind::UpdateField { value, .. }
                | InstructionKind::UpdateIndex { value, .. } => weight
                    .saturating_add(frame.shape.values[value.0 as usize])
                    .saturating_sub(1),
                // Besides copying the result, a checked projection reads a tag.
                InstructionKind::ReadVariant { .. } => weight,
                _ => weight.saturating_sub(1),
            };
            charge(&mut steps, copied, program.max_steps())?;
            // A cloned aggregate may coexist briefly with the previous loop
            // value or destination local. Reserve that transient data too.
            let transient = match &instruction.kind {
                InstructionKind::Call { .. } => 0, // charged as the incoming frame below
                InstructionKind::Store { value, .. } => frame.shape.values[value.0 as usize],
                _ => weight,
            };
            if transient > 1 {
                let peak = live_slots
                    .checked_add(transient)
                    .filter(|n| *n <= program.max_live_slots())
                    .ok_or_else(|| quota("CT aggregate temporary cell budget exhausted"))?;
                peak_live_slots = peak_live_slots.max(peak);
            }
            let value = match &instruction.kind {
                InstructionKind::Constant(value) => Some(CompileTimeValue::Scalar(value.clone())),
                InstructionKind::ConvertInteger { value, target } => {
                    Some(CompileTimeValue::Scalar(super::convert_integer(
                        frame.value(*value).scalar(),
                        target,
                    )?))
                }
                InstructionKind::Load(local) => Some(
                    frame.locals[local.0 as usize]
                        .as_ref()
                        .expect("verified local initialized")
                        .clone(),
                ),
                InstructionKind::RegisterDefer { .. } => {
                    return Err(vec![Diagnostic::error(
                        Phase::Ct,
                        "E-CT-FORBIDDEN",
                        Some(instruction.span),
                        "defer is forbidden during compile-time execution",
                    )]);
                }
                InstructionKind::ScopeEnter { .. } | InstructionKind::ScopeExit { .. } => None,
                InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local) => {
                    frame.locals[local.0 as usize] = None;
                    None
                }
                InstructionKind::Store { local, value } => {
                    frame.locals[local.0 as usize] = Some(frame.value(*value).clone());
                    None
                }
                InstructionKind::Unary { op, operand } => Some(CompileTimeValue::Scalar(
                    crate::unary(*op, frame.value(*operand).scalar())?,
                )),
                InstructionKind::Binary { op, left, right } => {
                    Some(CompileTimeValue::Scalar(crate::binary(
                        *op,
                        frame.value(*left).scalar(),
                        frame.value(*right).scalar(),
                    )?))
                }
                InstructionKind::MakeAggregate { fields } => Some(CompileTimeValue::Aggregate(
                    fields.iter().map(|v| frame.value(*v).clone()).collect(),
                )),
                InstructionKind::MakeVariant { variant, fields } => {
                    Some(CompileTimeValue::Variant {
                        variant: *variant,
                        fields: fields.iter().map(|v| frame.value(*v).clone()).collect(),
                    })
                }
                InstructionKind::VariantTag { value } => Some(CompileTimeValue::Scalar(
                    Constant::I32(frame.value(*value).variant().0 as i32),
                )),
                InstructionKind::ReadVariant {
                    value,
                    variant,
                    index,
                } => {
                    let (active, fields) = frame.value(*value).variant();
                    if active != *variant {
                        return Err(vec![Diagnostic::error(
                            Phase::Ct,
                            "E-CT-TRAP",
                            Some(instruction.span),
                            "compile-time enum projection reached an inactive variant",
                        )]);
                    }
                    Some(fields[*index as usize].clone())
                }
                InstructionKind::ExtractField { aggregate, index } => {
                    Some(frame.value(*aggregate).fields()[*index as usize].clone())
                }
                InstructionKind::UpdateField {
                    aggregate,
                    index,
                    value,
                } => {
                    let fields = frame
                        .value(*aggregate)
                        .fields()
                        .iter()
                        .enumerate()
                        .map(|(i, old)| {
                            if i == *index as usize {
                                frame.value(*value).clone()
                            } else {
                                old.clone()
                            }
                        })
                        .collect();
                    Some(CompileTimeValue::Aggregate(fields))
                }
                InstructionKind::ExtractIndex {
                    aggregate,
                    index: offset,
                } => {
                    let fields = frame.value(*aggregate).fields();
                    let index = index(frame.value(*offset), fields.len())?;
                    Some(fields[index].clone())
                }
                InstructionKind::UpdateIndex {
                    aggregate,
                    index: offset,
                    value,
                } => {
                    let old = frame.value(*aggregate).fields();
                    let index = index(frame.value(*offset), old.len())?;
                    let fields = old
                        .iter()
                        .enumerate()
                        .map(|(i, old)| {
                            if i == index {
                                frame.value(*value).clone()
                            } else {
                                old.clone()
                            }
                        })
                        .collect();
                    Some(CompileTimeValue::Aggregate(fields))
                }
                InstructionKind::Call {
                    function,
                    arguments,
                } => {
                    let target = &module.functions[function.0 as usize];
                    let shape = &shapes[function.0 as usize];
                    let destination = instruction.result;
                    let incoming = live_slots
                        .checked_add(shape.slots)
                        .filter(|n| *n <= program.max_live_slots())
                        .ok_or_else(|| quota("CT live value-slot budget exhausted"))?;
                    if depth >= program.max_call_depth() {
                        return Err(quota("CT call depth budget exhausted"));
                    }
                    let arguments = arguments.iter().map(|v| frame.value(*v).clone()).collect();
                    live_slots = incoming;
                    frames.push(Frame::new(target, shape, arguments, destination));
                    peak_live_slots = peak_live_slots.max(live_slots);
                    peak_call_depth = peak_call_depth.max(frames.len());
                    continue;
                }
                _ => unreachable!(
                    "independent CT whitelist excludes effects and runtime capabilities"
                ),
            };
            if let Some(id) = instruction.result {
                frame.values[id.0 as usize] = value;
            }
        } else {
            match block.terminator {
                Terminator::Return(value) => {
                    let extra =
                        value.map_or(0, |v| frame.shape.values[v.0 as usize].saturating_sub(1));
                    charge(&mut steps, extra, program.max_steps())?;
                    if extra > 0 {
                        let peak = live_slots
                            .checked_add(extra + 1)
                            .filter(|n| *n <= program.max_live_slots())
                            .ok_or_else(|| quota("CT return temporary cell budget exhausted"))?;
                        peak_live_slots = peak_live_slots.max(peak);
                    }
                    let value = value
                        .map(|v| frame.value(v).clone())
                        .unwrap_or(CompileTimeValue::Scalar(Constant::Unit));
                    let completed = frames.pop().expect("active frame");
                    live_slots -= completed.shape.slots;
                    if let Some(parent) = frames.last_mut() {
                        if let Some(destination) = completed.destination {
                            parent.values[destination.0 as usize] = Some(value);
                        }
                    } else {
                        return Ok(ProgramEvaluation {
                            value,
                            steps,
                            peak_live_slots,
                            peak_call_depth,
                        });
                    }
                }
                Terminator::Jump(target) => {
                    frame.block = target.0 as usize;
                    frame.instruction = 0;
                }
                Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                } => {
                    let Constant::Bool(value) = frame.value(condition).scalar() else {
                        unreachable!("verified Bool branch")
                    };
                    frame.block = if *value { then_block.0 } else { else_block.0 } as usize;
                    frame.instruction = 0;
                }
                Terminator::Unreachable => {
                    unreachable!("independent verifier rejects reachable unreachable")
                }
                Terminator::Trap => {
                    return Err(vec![Diagnostic::error(
                        Phase::Ct,
                        "E-CT-TRAP",
                        Some(frame.function.span),
                        "compile-time execution reached an explicit checked trap",
                    )]);
                }
            }
        }
    }
}
