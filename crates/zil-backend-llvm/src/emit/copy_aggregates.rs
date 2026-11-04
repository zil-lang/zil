//! Private stack snapshots for large ordinary Copy values. These pointers never
//! represent source addresses. Source LIR types remain unchanged.
use super::*;
use zil_base::Limits;

const MIN_BYTES: u64 = 4096;

#[derive(Clone)]
pub(super) struct Slot {
    pub(super) ty: m::Ty,
    pub(super) bytes: u64,
    pub(super) align: u32,
    work: usize,
}
pub(super) type Plan = BTreeMap<ValueId, Slot>;

pub(super) fn plan(
    lir: &LirView<'_>,
    limits: &Limits,
) -> CompileResult<super::large_copy_abi::Plans> {
    let mut work = limits.max_analysis_steps;
    let mut plans = BTreeMap::new();
    for function in lir.functions {
        let mut values = Plan::new();
        for (index, ty) in function.values.iter().enumerate() {
            if !matches!(
                ty,
                Type::Array { .. }
                    | Type::Tuple(_)
                    | Type::Record { .. }
                    | Type::Enum { .. }
                    | Type::ErrorUnion { .. }
            ) {
                continue;
            }
            // Reserve all recursive predicates, layout construction and bounded
            // metadata clones before examining or retaining the complete shape.
            let before = work;
            reserve(ty, &mut work, 0, limits)?;
            if !ty.is_copy_value() {
                continue;
            }
            let machine = llvm_type(ty)?;
            let (bytes, align) = layout(&machine)?;
            if bytes >= MIN_BYTES {
                if matches!(ty, Type::Enum { .. }) {
                    // make_variant/read_variant clone the source enum as well
                    // as its machine layout. Source names are absent from the
                    // machine type; pay their bytes before retaining the slot,
                    // then include this fee in repeated emission reservations.
                    reserve_enum_metadata(ty, &mut work, 0, limits)?;
                }
                values.insert(
                    ValueId(index as u32),
                    Slot {
                        ty: machine,
                        bytes,
                        align,
                        work: before - work,
                    },
                );
            }
        }
        if !values.is_empty() {
            // Reserve repeated machine-type clones, slot accesses and emitted
            // operations before growing function event buffers. Use a paid
            // shape upper bound; wide construction/call operands count too.
            let shape = values.values().map(|slot| slot.work).max().unwrap();
            for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                let operands = match &instruction.kind {
                    InstructionKind::MakeAggregate { fields }
                    | InstructionKind::MakeVariant { fields, .. } => fields.len(),
                    InstructionKind::Call { arguments, .. }
                    | InstructionKind::CallIndirect { arguments, .. } => arguments.len(),
                    _ => 2,
                };
                let cost = shape
                    .saturating_mul(operands.saturating_add(4))
                    .saturating_add(32);
                work = work.checked_sub(cost).ok_or_else(|| {
                    vec![Diagnostic::limit(
                        Phase::Codegen,
                        "Copy aggregate snapshot emission budget exceeded",
                        Some(instruction.span),
                    )]
                })?;
                if let InstructionKind::MakeVariant { fields, .. } = &instruction.kind
                    && instruction
                        .result
                        .is_some_and(|id| values.contains_key(&id))
                {
                    // Small fields retain canonical enum payload stores. Their
                    // recursive scalar expansion (including array cardinality)
                    // must be paid before store_payload grows event buffers.
                    // Large fields use one already-paid snapshot memcpy.
                    for field in fields {
                        if !values.contains_key(field) {
                            let cost = canonical_payload_work(&function.values[field.0 as usize])
                                .saturating_mul(16);
                            work = work.checked_sub(cost).ok_or_else(|| {
                                vec![Diagnostic::limit(
                                    Phase::Codegen,
                                    "Copy enum canonical payload budget exceeded",
                                    Some(instruction.span),
                                )]
                            })?;
                        }
                    }
                }
            }
            plans.insert(function.id, values);
        }
    }
    let abi = super::large_copy_abi::plan(lir, limits, &mut work)?;
    if !plans.is_empty() || abi.functions.values().any(|s| s.large()) {
        work.checked_sub(256).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Codegen,
                "Copy helper emission budget exceeded",
                None,
            )]
        })?;
    }
    Ok(super::large_copy_abi::Plans {
        values: plans,
        functions: abi.functions,
        indirect: abi.indirect,
    })
}

fn reserve_enum_metadata(
    ty: &Type,
    work: &mut usize,
    depth: usize,
    limits: &Limits,
) -> CompileResult<()> {
    let exhausted = || {
        vec![Diagnostic::limit(
            Phase::Codegen,
            "Copy enum source metadata budget exceeded",
            None,
        )]
    };
    let containers = match ty {
        Type::Enum { variants, .. } => variants.len().saturating_mul(16),
        _ => 0,
    };
    // Every variant owns a String/Vec header even when its payload is empty.
    // Reserve all those fixed containers before inspecting names or fields.
    *work = work
        .checked_sub(16usize.saturating_add(containers))
        .ok_or_else(exhausted)?;
    if depth > limits.max_call_depth {
        return Err(exhausted());
    }
    let mut name = |name: &str| -> CompileResult<()> {
        *work = work.checked_sub(name.len()).ok_or_else(exhausted)?;
        Ok(())
    };
    match ty {
        Type::Named(text) | Type::Unsupported(text) => name(text)?,
        Type::Record { name: text, fields } | Type::Package { name: text, fields } => {
            name(text)?;
            for (text, _) in fields {
                name(text)?;
            }
            for (_, field) in fields {
                reserve_enum_metadata(field, work, depth + 1, limits)?;
            }
        }
        Type::Enum {
            name: text,
            variants,
        } => {
            name(text)?;
            for (text, _) in variants {
                name(text)?;
            }
            for (_, fields) in variants {
                for field in fields {
                    reserve_enum_metadata(field, work, depth + 1, limits)?;
                }
            }
        }
        Type::Array { element, .. }
        | Type::Slice { element }
        | Type::OwnedStore { element }
        | Type::Address { inner: element, .. }
        | Type::Managed(element)
        | Type::Desc(element)
        | Type::Opaque {
            hidden: element, ..
        } => reserve_enum_metadata(element, work, depth + 1, limits)?,
        Type::Tuple(fields) => {
            for field in fields {
                reserve_enum_metadata(field, work, depth + 1, limits)?;
            }
        }
        Type::ErrorUnion { error, result } => {
            reserve_enum_metadata(error, work, depth + 1, limits)?;
            reserve_enum_metadata(result, work, depth + 1, limits)?;
        }
        Type::FunctionPointer { parameters, result }
        | Type::ForeignCallback { parameters, result } => {
            for parameter in parameters {
                reserve_enum_metadata(parameter, work, depth + 1, limits)?;
            }
            reserve_enum_metadata(result, work, depth + 1, limits)?;
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn reserve(
    ty: &Type,
    work: &mut usize,
    depth: usize,
    limits: &Limits,
) -> CompileResult<()> {
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
        Type::FunctionPointer { parameters, result }
        | Type::ForeignCallback { parameters, result } => {
            for parameter in parameters {
                reserve(parameter, work, depth + 1, limits)?;
            }
            reserve(result, work, depth + 1, limits)?;
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn classify(
    ty: &Type,
    work: &mut usize,
    limits: &Limits,
) -> CompileResult<Option<Slot>> {
    if !matches!(
        ty,
        Type::Array { .. }
            | Type::Tuple(_)
            | Type::Record { .. }
            | Type::Enum { .. }
            | Type::ErrorUnion { .. }
    ) {
        return Ok(None);
    }
    let before = *work;
    reserve(ty, work, 0, limits)?;
    if !ty.is_copy_value() {
        return Ok(None);
    }
    let machine = llvm_type(ty)?;
    let (bytes, align) = layout(&machine)?;
    Ok((bytes >= MIN_BYTES).then_some(Slot {
        ty: machine,
        bytes,
        align,
        work: before - *work,
    }))
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
        m::Ty::F32 => (4, 4),
        m::Ty::F64 => (8, 8),
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
                    m::Param::new(second.clone(), "source"),
                    m::Param::new(m::Ty::Int(64), "bytes"),
                    m::Param::new(m::Ty::Int(1), "volatile"),
                ],
                variadic: false,
            },
            attrs: vec![],
            blocks: None,
        }));
        // Retain the requested optimization pipeline while keeping one bounded
        // memory operation opaque to caller SROA. Inlining this helper would
        // recreate hundreds of thousands of promoted scalar allocas at O2.
        code.push(m::Event::Begin(m::Function {
            name: if second == m::Ty::Ptr {
                "zil.copy.bytes"
            } else {
                "zil.zero.bytes"
            }
            .into(),
            linkage: m::Linkage::Internal,
            signature: m::Signature {
                result: m::Ty::Void,
                result_attrs: vec![],
                variadic: false,
                params: vec![
                    m::Param::new(m::Ty::Ptr, "destination"),
                    m::Param::new(second.clone(), "source"),
                    m::Param::new(m::Ty::Int(64), "bytes"),
                ],
            },
            attrs: vec![m::Attr::NoInline],
            blocks: None,
        }));
        code.push(m::Event::Label("entry".into()));
        code.push(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Call {
                result: m::Ty::Void,
                result_attrs: vec![],
                callee: m::global(name.into()),
                args: vec![
                    m::Arg::new(m::Ty::Ptr, m::local("destination".into())),
                    m::Arg::new(second, m::local("source".into())),
                    m::Arg::new(m::Ty::Int(64), m::local("bytes".into())),
                    m::Arg::new(m::Ty::Int(1), m::Val::Bool(false)),
                ],
                vararg: None,
            },
        )));
        code.push(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Return(None),
        )));
        code.push(m::Event::End);
    }
    code
}

pub(super) fn pointer(value: ValueId) -> m::Val {
    m::local(format!("copy.value.{}", value.0))
}

// reserve has already bounded/paid this complete type walk and its depth.
// Estimate the existing canonical store's actual repeated field operations
// without allocating aggregate_fields or expanding an array in the planner.
fn canonical_payload_work(ty: &Type) -> usize {
    match ty {
        Type::Unit => 0,
        Type::Bool => 2,
        Type::Array { element, length } => canonical_payload_work(element)
            .saturating_add(2)
            .saturating_mul(*length as usize),
        Type::Tuple(fields) => fields.iter().fold(0usize, |n, field| {
            n.saturating_add(canonical_payload_work(field).saturating_add(2))
        }),
        Type::Record { fields, .. } => fields.iter().fold(0usize, |n, (_, field)| {
            n.saturating_add(canonical_payload_work(field).saturating_add(2))
        }),
        Type::ErrorUnion { error, result } => canonical_payload_work(error)
            .saturating_add(canonical_payload_work(result))
            .saturating_add(8),
        Type::Enum { .. } => 8,
        _ => 1,
    }
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
    pub(super) fn copy_bytes(&mut self, destination: m::Val, source: m::Val, bytes: u64) {
        // One end is always a private, independently allocated value snapshot.
        // Source addresses cannot name it; these ranges cannot overlap.
        self.text.push(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Call {
                result: m::Ty::Void,
                result_attrs: vec![],
                callee: m::global("zil.copy.bytes".into()),
                args: vec![
                    m::Arg::new(m::Ty::Ptr, destination),
                    m::Arg::new(m::Ty::Ptr, source),
                    m::Arg::new(m::Ty::Int(64), m::Val::Int(i128::from(bytes))),
                ],
                vararg: None,
            },
        )));
    }
    pub(super) fn copy_zero(&mut self, output: ValueId) {
        self.text.push(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Call {
                result: m::Ty::Void,
                result_attrs: vec![],
                callee: m::global("zil.zero.bytes".into()),
                args: vec![
                    m::Arg::new(m::Ty::Ptr, pointer(output)),
                    m::Arg::new(m::Ty::Int(8), 0),
                    m::Arg::new(
                        m::Ty::Int(64),
                        m::Val::Int(i128::from(self.copy_values[&output].bytes)),
                    ),
                ],
                vararg: None,
            },
        )));
    }
    pub(super) fn copy_read(&mut self, output: ValueId, source: m::Val) -> CompileResult<()> {
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
    pub(super) fn copy_write(&mut self, destination: m::Val, input: ValueId) -> CompileResult<()> {
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
    pub(super) fn copy_field(&mut self, aggregate: ValueId, index: m::Arg) -> m::Val {
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
                        callee: m::global("zil.zero.bytes".into()),
                        args: vec![
                            m::Arg::new(m::Ty::Ptr, pointer(out)),
                            m::Arg::new(m::Ty::Int(8), 0),
                            m::Arg::new(m::Ty::Int(64), m::Val::Int(i128::from(bytes))),
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

    // Ordinary memory instructions and LargeCopy ABI consumers are handled
    // explicitly. Any remaining snapshot consumer must stop before publishing
    // machine IR; never materialize a giant SSA bridge for an unsupported path.
    fn check_snapshot_argument(&mut self, argument: &m::Arg) {
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
        if self
            .copy_values
            .get(&id)
            .is_some_and(|slot| slot.ty == argument.ty)
        {
            self.unsupported_large_copy = true;
        }
    }
    pub(super) fn copy_event(&mut self, event: m::Event) {
        if let m::Event::Instruction(instruction) = &event {
            match &instruction.op {
                m::Op::Store { arg, .. }
                | m::Op::Cast { arg, .. }
                | m::Op::Extract { arg, .. }
                | m::Op::Freeze(arg)
                | m::Op::Return(Some(arg)) => self.check_snapshot_argument(arg),
                m::Op::Insert { arg, field, .. } => {
                    self.check_snapshot_argument(arg);
                    self.check_snapshot_argument(field);
                }
                m::Op::Call { args, .. } => {
                    for argument in args {
                        self.check_snapshot_argument(argument);
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
            if output.is_some_and(|id| self.copy_values.contains_key(&id)) {
                self.unsupported_large_copy = true;
                return;
            }
        }
        self.text.push(event);
    }
}
