//! Private stack snapshots for large ordinary Copy values. These pointers never
//! represent source addresses and do not change function signatures or LIR.
use super::*;
use zil_base::Limits;

const MIN_BYTES: u64 = 4096;

#[derive(Clone)]
pub(super) struct Slot {
    ty: m::Ty,
    bytes: u64,
    align: u32,
}
pub(super) type Plan = BTreeMap<ValueId, Slot>;

pub(super) fn plan(
    lir: &LirView<'_>,
    limits: &Limits,
) -> CompileResult<BTreeMap<FunctionId, Plan>> {
    fn reserve(ty: &Type, work: &mut usize, depth: usize, limits: &Limits) -> CompileResult<()> {
        *work = work.checked_sub(16).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Codegen,
                "Copy aggregate snapshot planning budget exceeded",
                None,
            )]
        })?;
        if depth > limits.max_call_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Codegen,
                "Copy snapshot type depth exceeded",
                None,
            )]);
        }
        match ty {
            Type::Array { element, .. } => reserve(element, work, depth + 1, limits)?,
            Type::Tuple(fields) => {
                for field in fields {
                    reserve(field, work, depth + 1, limits)?;
                }
            }
            Type::Record { fields, .. } => {
                for (_, field) in fields {
                    reserve(field, work, depth + 1, limits)?;
                }
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    for field in fields {
                        reserve(field, work, depth + 1, limits)?;
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                reserve(error, work, depth + 1, limits)?;
                reserve(result, work, depth + 1, limits)?;
            }
            _ => {}
        }
        Ok(())
    }
    let mut work = limits.max_analysis_steps;
    let mut plans = BTreeMap::new();
    for function in lir.functions {
        let mut values = Plan::new();
        for (index, ty) in function.values.iter().enumerate() {
            if !matches!(
                ty,
                Type::Array { .. } | Type::Tuple(_) | Type::Record { .. }
            ) {
                continue;
            }
            // Reserve all recursive predicates, layout construction and bounded
            // metadata clones before examining or retaining the complete shape.
            reserve(ty, &mut work, 0, limits)?;
            if !ty.is_copy_value() {
                continue;
            }
            let machine = llvm_type(ty)?;
            let (bytes, align) = layout(&machine)?;
            if bytes >= MIN_BYTES {
                values.insert(
                    ValueId(index as u32),
                    Slot {
                        ty: machine,
                        bytes,
                        align,
                    },
                );
            }
        }
        plans.insert(function.id, values);
    }
    Ok(plans)
}

fn layout(ty: &m::Ty) -> CompileResult<(u64, u32)> {
    let overflow = || {
        vec![Diagnostic::limit(
            Phase::Codegen,
            "Copy snapshot layout overflow",
            None,
        )]
    };
    let round = |bytes: u64, align: u32| {
        bytes
            .checked_add(u64::from(align) - 1)
            .map(|n| n / u64::from(align) * u64::from(align))
            .ok_or_else(overflow)
    };
    Ok(match ty {
        m::Ty::Void => (0, 1),
        m::Ty::Int(bits) => {
            let bytes = u64::from(*bits).div_ceil(8);
            (bytes, bytes.next_power_of_two().min(8) as u32)
        }
        m::Ty::Ptr => (8, 8),
        m::Ty::Array(length, element) => {
            let (bytes, align) = layout(element)?;
            (
                bytes
                    .checked_mul(*length)
                    .filter(|n| *n <= i64::MAX as u64)
                    .ok_or_else(overflow)?,
                align,
            )
        }
        m::Ty::Struct(fields) => {
            let (mut bytes, mut align) = (0u64, 1u32);
            for field in fields {
                let (size, field_align) = layout(field)?;
                align = align.max(field_align);
                bytes = round(bytes, field_align)?
                    .checked_add(size)
                    .ok_or_else(overflow)?;
            }
            (round(bytes, align)?, align)
        }
        m::Ty::Named(_) => {
            return invalid(
                "E-CODEGEN-CFG",
                "ordinary Copy snapshot has an unresolved machine type",
                None,
            );
        }
    })
}

pub(super) fn declarations() -> m::Code {
    let mut code = m::Code::new();
    for (name, second) in [
        ("llvm.memcpy.p0.p0.i64", m::Ty::Ptr),
        ("llvm.memset.p0.i64", m::Ty::Int(8)),
    ] {
        code.push(m::Event::Declare(m::Function {
            name: name.into(),
            linkage: m::Linkage::External,
            signature: m::Signature {
                result: m::Ty::Void,
                result_attrs: vec![],
                params: vec![
                    m::Param::new(m::Ty::Ptr, "destination"),
                    m::Param::new(second, "source"),
                    m::Param::new(m::Ty::Int(64), "bytes"),
                    m::Param::new(m::Ty::Int(1), "volatile"),
                ],
                variadic: false,
            },
            attrs: vec![],
            blocks: None,
        }));
    }
    code
}

fn pointer(value: ValueId) -> m::Val {
    m::local(format!("copy.value.{}", value.0))
}

impl FunctionEmitter<'_> {
    pub(super) fn copy_allocations(&mut self) {
        for (id, slot) in self.copy_values {
            self.text.push(m::Event::Instruction(m::Inst::new(
                Some(format!("copy.value.{}", id.0)),
                m::Op::Alloca {
                    ty: slot.ty.clone(),
                    align: slot.align,
                },
            )));
        }
    }
    fn copy_bytes(&mut self, destination: m::Val, source: m::Val, bytes: u64) {
        // One end is always a private, independently allocated value snapshot.
        // Source addresses cannot name it; these ranges cannot overlap.
        self.text.push(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Call {
                result: m::Ty::Void,
                result_attrs: vec![],
                callee: m::global("llvm.memcpy.p0.p0.i64".into()),
                args: vec![
                    m::Arg::new(m::Ty::Ptr, destination),
                    m::Arg::new(m::Ty::Ptr, source),
                    m::Arg::new(m::Ty::Int(64), m::Val::Int(i128::from(bytes))),
                    m::Arg::new(m::Ty::Int(1), m::Val::Bool(false)),
                ],
                vararg: None,
            },
        )));
    }
    fn copy_read(&mut self, output: ValueId, source: m::Val) -> CompileResult<()> {
        if let Some(slot) = self.copy_values.get(&output) {
            self.copy_bytes(pointer(output), source, slot.bytes);
        } else if self.value_type(output)? != &Type::Unit {
            self.event(m::Event::Instruction(m::Inst::new(
                Some(format!("v.{}", output.0)),
                m::Op::Load {
                    ty: llvm_type(self.value_type(output)?)?,
                    ptr: source,
                    align: alignment(self.value_type(output)?, self.closures)? as u32,
                },
            )));
        }
        Ok(())
    }
    fn copy_write(&mut self, destination: m::Val, input: ValueId) -> CompileResult<()> {
        if let Some(slot) = self.copy_values.get(&input) {
            self.copy_bytes(destination, pointer(input), slot.bytes);
        } else {
            let ty = self.value_type(input)?;
            self.event(m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Store {
                    arg: m::Arg::new(
                        aggregate_field_type(ty)?,
                        if *ty == Type::Unit {
                            m::Val::Zero
                        } else {
                            m::local(format!("v.{}", input.0))
                        },
                    ),
                    ptr: destination,
                    align: alignment(ty, self.closures)? as u32,
                },
            )));
        }
        Ok(())
    }
    fn copy_field(&mut self, aggregate: ValueId, index: m::Arg) -> m::Val {
        let address = self.temp();
        let slot = &self.copy_values[&aggregate];
        self.text.push(m::Event::Instruction(m::Inst::new(
            Some(m::name(address.clone())),
            m::Op::Gep {
                ty: slot.ty.clone(),
                ptr: pointer(aggregate),
                indices: vec![m::Arg::new(m::Ty::Int(32), 0), index],
            },
        )));
        m::val(address)
    }
    pub(super) fn copy_instruction(&mut self, instruction: &Instruction) -> CompileResult<bool> {
        let output = instruction.result;
        let output_memory = output.is_some_and(|id| self.copy_values.contains_key(&id));
        match &instruction.kind {
            InstructionKind::Load(local) | InstructionKind::TakeLocal { local }
                if output_memory =>
            {
                self.copy_read(result(instruction)?, m::local(format!("local.{}", local.0)))?;
            }
            InstructionKind::Store { local, value } if self.copy_values.contains_key(value) => {
                self.copy_write(m::local(format!("local.{}", local.0)), *value)?;
            }
            InstructionKind::ReadAddress { address } if output_memory => {
                self.copy_read(result(instruction)?, m::local(format!("v.{}", address.0)))?;
            }
            InstructionKind::WriteAddress { address, value }
                if self.copy_values.contains_key(value) =>
            {
                self.copy_write(m::local(format!("v.{}", address.0)), *value)?;
            }
            InstructionKind::MakeAggregate { fields } if output_memory => {
                let out = result(instruction)?;
                let bytes = self.copy_values[&out].bytes;
                // Initialize padding/Unit placeholders too; no undefined bytes
                // are promoted into a source value while assembling the fields.
                self.text.push(m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Call {
                        result: m::Ty::Void,
                        result_attrs: vec![],
                        callee: m::global("llvm.memset.p0.i64".into()),
                        args: vec![
                            m::Arg::new(m::Ty::Ptr, pointer(out)),
                            m::Arg::new(m::Ty::Int(8), 0),
                            m::Arg::new(m::Ty::Int(64), m::Val::Int(i128::from(bytes))),
                            m::Arg::new(m::Ty::Int(1), m::Val::Bool(false)),
                        ],
                        vararg: None,
                    },
                )));
                for (index, field) in fields.iter().enumerate() {
                    let address = self
                        .copy_field(out, m::Arg::new(m::Ty::Int(32), m::Val::Int(index as i128)));
                    self.copy_write(address, *field)?;
                }
            }
            InstructionKind::ExtractField { aggregate, index }
                if self.copy_values.contains_key(aggregate) =>
            {
                let address = self.copy_field(
                    *aggregate,
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(i128::from(*index))),
                );
                self.copy_read(result(instruction)?, address)?;
            }
            InstructionKind::UpdateField {
                aggregate,
                index,
                value,
            } if output_memory => {
                let out = result(instruction)?;
                self.copy_bytes(
                    pointer(out),
                    pointer(*aggregate),
                    self.copy_values[&out].bytes,
                );
                let address = self.copy_field(
                    out,
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(i128::from(*index))),
                );
                self.copy_write(address, *value)?;
            }
            InstructionKind::ExtractIndex { aggregate, index }
                if self.copy_values.contains_key(aggregate) =>
            {
                self.copy_index(instruction, *aggregate, *index, None)?;
            }
            InstructionKind::UpdateIndex {
                aggregate,
                index,
                value,
            } if output_memory => {
                self.copy_index(instruction, *aggregate, *index, Some(*value))?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
    fn copy_index(
        &mut self,
        instruction: &Instruction,
        aggregate: ValueId,
        index: ValueId,
        update: Option<ValueId>,
    ) -> CompileResult<()> {
        let Type::Array { length, .. } = self.value_type(aggregate)? else {
            unreachable!("verified array")
        };
        let length = *length;
        let index_value = self.widened_index(index)?;
        let valid = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name(valid.clone())),
            m::Op::Compare {
                predicate: "ult".into(),
                ty: m::Ty::Int(64),
                left: m::val(index_value.clone()),
                right: m::val(length),
            },
        )));
        let continuation = self.continuation;
        self.continuation += 1;
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::CondBranch {
                condition: m::val(valid),
                yes: format!("index.ok.{continuation}"),
                no: "zil.trap".into(),
            },
        )));
        self.event(m::Event::Label(format!("index.ok.{continuation}")));
        let selected = if let Some(input) = update {
            let out = result(instruction)?;
            self.copy_bytes(
                pointer(out),
                pointer(aggregate),
                self.copy_values[&out].bytes,
            );
            let address = self.copy_field(out, m::Arg::new(m::Ty::Int(64), m::val(index_value)));
            self.copy_write(address, input)?;
            return Ok(());
        } else {
            self.copy_field(aggregate, m::Arg::new(m::Ty::Int(64), m::val(index_value)))
        };
        self.copy_read(result(instruction)?, selected)
    }

    // ABI and other legacy consumers retain their exact signatures/operations.
    // Materialization is explicit and may still expose LLVM's large-value ABI
    // limitation; this is not a silent backend/optimization fallback.
    fn copy_argument(&mut self, argument: &mut m::Arg) {
        let m::Val::Local(name) = &argument.value else {
            return;
        };
        let Some(id) = name
            .strip_prefix("v.")
            .and_then(|n| n.parse::<u32>().ok())
            .map(ValueId)
        else {
            return;
        };
        let Some(slot) = self
            .copy_values
            .get(&id)
            .filter(|slot| slot.ty == argument.ty)
            .cloned()
        else {
            return;
        };
        let temporary = self.temp();
        self.text.push(m::Event::Instruction(m::Inst::new(
            Some(m::name(temporary.clone())),
            m::Op::Load {
                ty: slot.ty,
                ptr: pointer(id),
                align: slot.align,
            },
        )));
        argument.value = m::val(temporary);
    }
    pub(super) fn copy_event(&mut self, mut event: m::Event) {
        if let m::Event::Instruction(instruction) = &mut event {
            match &mut instruction.op {
                m::Op::Store { arg, .. }
                | m::Op::Cast { arg, .. }
                | m::Op::Extract { arg, .. }
                | m::Op::Freeze(arg)
                | m::Op::Return(Some(arg)) => self.copy_argument(arg),
                m::Op::Insert { arg, field, .. } => {
                    self.copy_argument(arg);
                    self.copy_argument(field);
                }
                m::Op::Call { args, .. } => {
                    for argument in args {
                        self.copy_argument(argument);
                    }
                }
                // The source LIR has no aggregate phi; scalar_locals only adds
                // scalar phis after this emitter has finished.
                _ => {}
            }
            let output = instruction
                .result
                .as_deref()
                .and_then(|name| name.strip_prefix("v."))
                .and_then(|id| id.parse::<u32>().ok())
                .map(ValueId);
            if let Some((id, slot)) =
                output.and_then(|id| self.copy_values.get(&id).cloned().map(|slot| (id, slot)))
            {
                let temporary = self.temp();
                instruction.result = Some(m::name(temporary.clone()));
                self.text.push(event);
                self.text.push(m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Store {
                        arg: m::Arg::new(slot.ty, m::val(temporary)),
                        ptr: pointer(id),
                        align: slot.align,
                    },
                )));
                return;
            }
        }
        self.text.push(event);
    }
}
