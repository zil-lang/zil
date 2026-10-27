//! Versioned binary transport for **raw candidates only**. Decoding is bounded
//! and rejects unknown tags, partial records, noncanonical bools, and trailing
//! bytes. A successful decode does not certify types, control flow, or safety.
//!
//! CTIR contains an operation section followed by mandatory `CERT` typed
//! claims: one `(ValueId, Type)` row per operation. These are the raw declared
//! result types already present in `CtInstruction`; their presence is not proof.
//! `zil-verify` independently checks every claim before creating VerifiedCTIR.

use crate::*;
use zil_base::{CompileResult, Diagnostic, Endian, FileId, Limits, Phase};

pub const WIRE_VERSION: u32 = 20;
const CT_MAGIC: &[u8; 8] = b"ZILCTIR\0";
const MODULE_MAGIC: &[u8; 8] = b"ZILMODP\0";
const CT_CLAIMS: &[u8; 4] = b"CERT";
type WireResult<T> = Result<T, Diagnostic>;

fn malformed(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::error(Phase::Verify, code, None, message)
}
fn quota(message: impl Into<String>) -> Diagnostic {
    Diagnostic::limit(Phase::Verify, message, None)
}
fn public<T>(result: WireResult<T>) -> CompileResult<T> {
    result.map_err(|error| vec![error])
}

/// Serialize a public CT candidate. No trust or Verified marker is serialized.
pub fn encode_ct(module: &CtModule, limits: &Limits) -> CompileResult<Vec<u8>> {
    public((|| {
        let mut writer = Writer::new(limits);
        writer.header(CT_MAGIC)?;
        writer.u32(module.schema_version)?;
        writer.fingerprint(module.input_fingerprint)?;
        writer.target(&module.target)?;
        writer.u64(module.max_steps)?;
        writer.u32(module.result.0)?;
        writer.count(module.dependencies.len(), limits.max_nodes)?;
        for dependency in &module.dependencies {
            writer.text(&dependency.name)?;
            writer.fingerprint(dependency.fingerprint)?;
            writer.boolean(dependency.completed)?;
        }
        writer.count(module.instructions.len(), limits.max_instructions)?;
        for instruction in &module.instructions {
            writer.u32(instruction.result.0)?;
            match &instruction.kind {
                CtInstructionKind::Constant(constant) => {
                    writer.u8(0)?;
                    writer.constant(constant)?;
                }
                CtInstructionKind::ConvertInteger { value, target } => {
                    writer.u8(3)?;
                    writer.u32(value.0)?;
                    writer.ty(target, 0)?;
                }
                CtInstructionKind::Unary { op, operand } => {
                    writer.u8(1)?;
                    writer.unary(*op)?;
                    writer.u32(operand.0)?;
                }
                CtInstructionKind::Binary { op, left, right } => {
                    writer.u8(2)?;
                    writer.binary(*op)?;
                    writer.u32(left.0)?;
                    writer.u32(right.0)?;
                }
                CtInstructionKind::Forbidden(name) => {
                    writer.u8(255)?;
                    writer.text(name)?;
                }
            }
        }
        writer.bytes(CT_CLAIMS)?;
        writer.count(module.instructions.len(), limits.max_instructions)?;
        for instruction in &module.instructions {
            writer.u32(instruction.result.0)?;
            writer.ty(&instruction.ty, 0)?;
        }
        Ok(writer.output)
    })())
}

/// Decode into public, forgeable CtModule data. Call `zil_verify::verify_ct`
/// before interpreting it; a cache hash or a `CERT` section grants no authority.
pub fn decode_ct(bytes: &[u8], limits: &Limits) -> CompileResult<CtModule> {
    public((|| {
        let mut reader = Reader::new(bytes, limits)?;
        reader.header(CT_MAGIC)?;
        let schema_version = reader.u32()?;
        let input_fingerprint = reader.fingerprint()?;
        let target = reader.target()?;
        let max_steps = reader.u64()?;
        if max_steps > limits.max_ct_steps {
            return Err(quota(
                "CTIR declared execution budget exceeds the selected quota",
            ));
        }
        let result = ValueId(reader.u32()?);
        let count = reader.count(limits.max_nodes, 37)?;
        let mut dependencies = Vec::with_capacity(count);
        for _ in 0..count {
            dependencies.push(CtDependency {
                name: reader.text()?,
                fingerprint: reader.fingerprint()?,
                completed: reader.boolean()?,
            });
        }
        let count = reader.count(limits.max_instructions, 6)?;
        let mut operations = Vec::with_capacity(count);
        for _ in 0..count {
            let result = ValueId(reader.u32()?);
            let kind = match reader.u8()? {
                0 => CtInstructionKind::Constant(reader.constant()?),
                1 => CtInstructionKind::Unary {
                    op: reader.unary()?,
                    operand: ValueId(reader.u32()?),
                },
                2 => CtInstructionKind::Binary {
                    op: reader.binary()?,
                    left: ValueId(reader.u32()?),
                    right: ValueId(reader.u32()?),
                },
                3 => CtInstructionKind::ConvertInteger {
                    value: ValueId(reader.u32()?),
                    target: reader.ty(0)?,
                },
                255 => CtInstructionKind::Forbidden(reader.text()?),
                opcode => {
                    return Err(malformed(
                        "E-CTIR-OPCODE",
                        format!("unknown CTIR opcode {opcode}"),
                    ));
                }
            };
            operations.push((result, kind));
        }
        if reader.remaining() == 0 {
            return Err(malformed(
                "E-CTIR-CERTIFICATE",
                "CTIR typed-claims section is missing",
            ));
        }
        if reader.take(4)? != CT_CLAIMS {
            return Err(malformed(
                "E-CTIR-CERTIFICATE",
                "CTIR typed-claims section marker is invalid",
            ));
        }
        let claims = reader.count(limits.max_instructions, 5)?;
        if claims != count {
            return Err(malformed(
                "E-CTIR-CERTIFICATE",
                "CTIR typed claims do not cover every operation exactly once",
            ));
        }
        let mut instructions = Vec::with_capacity(count);
        for (result, kind) in operations {
            if reader.u32()? != result.0 {
                return Err(malformed(
                    "E-CTIR-CERTIFICATE",
                    "CTIR typed claim refers to a different result",
                ));
            }
            instructions.push(CtInstruction {
                result,
                kind,
                ty: reader.ty(0)?,
            });
        }
        reader.finish()?;
        Ok(CtModule {
            schema_version,
            input_fingerprint,
            target,
            instructions,
            result,
            max_steps,
            dependencies,
        })
    })())
}

/// Raw module plus candidate proof cache. The wire cannot encode a Verified
/// wrapper; every recovered pair must pass `zil_verify::verify_module` again.
pub fn encode_module_candidate(
    module: &TypedModule,
    plan: &CandidatePlan,
    limits: &Limits,
) -> CompileResult<Vec<u8>> {
    public((|| {
        let mut writer = Writer::new(limits);
        writer.header(MODULE_MAGIC)?;
        writer.module(module)?;
        writer.plan(plan)?;
        Ok(writer.output)
    })())
}
pub fn decode_module_candidate(
    bytes: &[u8],
    limits: &Limits,
) -> CompileResult<(TypedModule, CandidatePlan)> {
    public((|| {
        let mut reader = Reader::new(bytes, limits)?;
        reader.header(MODULE_MAGIC)?;
        let module = reader.module()?;
        let plan = reader.plan()?;
        reader.finish()?;
        Ok((module, plan))
    })())
}

/// Serialize the raw CFG form of CTIR; it carries no executable authority.
pub fn encode_ct_program(program: &CtProgram, limits: &Limits) -> CompileResult<Vec<u8>> {
    public((|| {
        let mut writer = Writer::new(limits);
        writer.header(b"ZILCTPG\0")?;
        writer.u32(program.entry.0)?;
        writer.u64(program.max_steps)?;
        if program.max_call_depth > limits.max_call_depth
            || program.max_live_slots > limits.max_values.saturating_add(limits.max_locals)
        {
            return Err(quota("CT stack budget exceeds quota"));
        }
        writer.u32(
            u32::try_from(program.max_call_depth)
                .map_err(|_| quota("CT call quota does not fit wire"))?,
        )?;
        writer.u32(
            u32::try_from(program.max_live_slots)
                .map_err(|_| quota("CT slot quota does not fit wire"))?,
        )?;
        writer.module(&program.module)?;
        writer.plan(&program.plan)?;
        Ok(writer.output)
    })())
}
pub fn decode_ct_program(bytes: &[u8], limits: &Limits) -> CompileResult<CtProgram> {
    public((|| {
        let mut reader = Reader::new(bytes, limits)?;
        reader.header(b"ZILCTPG\0")?;
        let entry = FunctionId(reader.u32()?);
        let max_steps = reader.u64()?;
        if max_steps > limits.max_ct_steps {
            return Err(quota("CT execution budget exceeds quota"));
        }
        // These are scalar quotas, not lengths of immediately following arrays.
        let max_call_depth = reader.u32()? as usize;
        let max_live_slots = reader.u32()? as usize;
        if max_call_depth > limits.max_call_depth
            || max_live_slots > limits.max_values.saturating_add(limits.max_locals)
        {
            return Err(quota("CT stack budget exceeds quota"));
        }
        let module = reader.module()?;
        let plan = reader.plan()?;
        reader.finish()?;
        Ok(CtProgram {
            module,
            plan,
            entry,
            max_steps,
            max_call_depth,
            max_live_slots,
        })
    })())
}

struct Writer<'a> {
    output: Vec<u8>,
    limits: &'a Limits,
    nodes: usize,
}
impl<'a> Writer<'a> {
    fn new(limits: &'a Limits) -> Self {
        Self {
            output: Vec::new(),
            limits,
            nodes: limits.max_nodes,
        }
    }
    fn bytes(&mut self, bytes: &[u8]) -> WireResult<()> {
        if self
            .output
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > self.limits.max_source_bytes)
        {
            return Err(quota("candidate wire byte quota exceeded"));
        }
        self.output.extend_from_slice(bytes);
        Ok(())
    }
    fn u8(&mut self, value: u8) -> WireResult<()> {
        self.bytes(&[value])
    }
    fn u16(&mut self, value: u16) -> WireResult<()> {
        self.bytes(&value.to_le_bytes())
    }
    fn u32(&mut self, value: u32) -> WireResult<()> {
        self.bytes(&value.to_le_bytes())
    }
    fn u64(&mut self, value: u64) -> WireResult<()> {
        self.bytes(&value.to_le_bytes())
    }
    fn boolean(&mut self, value: bool) -> WireResult<()> {
        self.u8(u8::from(value))
    }
    fn fingerprint(&mut self, value: Fingerprint) -> WireResult<()> {
        self.bytes(&value.0)
    }
    fn header(&mut self, magic: &[u8]) -> WireResult<()> {
        self.bytes(magic)?;
        self.u32(WIRE_VERSION)
    }
    fn charge(&mut self, count: usize) -> WireResult<()> {
        self.nodes = self
            .nodes
            .checked_sub(count)
            .ok_or_else(|| quota("candidate wire node quota exceeded"))?;
        Ok(())
    }
    fn count(&mut self, count: usize, maximum: usize) -> WireResult<()> {
        if count > maximum {
            return Err(quota("candidate wire collection quota exceeded"));
        }
        self.charge(count)?;
        self.u32(
            u32::try_from(count)
                .map_err(|_| quota("candidate wire collection length does not fit u32"))?,
        )
    }
    fn text(&mut self, text: &str) -> WireResult<()> {
        self.u32(
            u32::try_from(text.len())
                .map_err(|_| quota("candidate wire string length does not fit u32"))?,
        )?;
        self.bytes(text.as_bytes())
    }
    fn span(&mut self, span: Span) -> WireResult<()> {
        self.u32(span.file.0)?;
        self.u64(span.start as u64)?;
        self.u64(span.end as u64)
    }
    fn optional_span(&mut self, span: Option<Span>) -> WireResult<()> {
        self.boolean(span.is_some())?;
        if let Some(span) = span {
            self.span(span)?;
        }
        Ok(())
    }
    fn optional_value(&mut self, value: Option<ValueId>) -> WireResult<()> {
        self.boolean(value.is_some())?;
        if let Some(value) = value {
            self.u32(value.0)?;
        }
        Ok(())
    }
    fn target(&mut self, target: &TargetProfile) -> WireResult<()> {
        self.text(&target.id)?;
        self.text(&target.triple)?;
        self.u16(target.pointer_width)?;
        self.u8(match target.endian {
            Endian::Little => 0,
            Endian::Big => 1,
        })?;
        self.text(&target.backend)?;
        self.text(&target.rule_version)?;
        self.boolean(target.checked_arithmetic)
    }
    fn ty(&mut self, ty: &Type, depth: usize) -> WireResult<()> {
        // Keep the wire's preorder and accounting without consuming one Rust
        // stack frame per untrusted type level. Collection continuations visit
        // one item at a time, after the original collection charge.
        enum Task<'t> {
            Type(&'t Type, usize),
            Types(&'t [Type], usize),
            Fields(&'t [(String, Type)], usize),
            Variants(&'t [(String, Vec<Type>)], usize),
            Length(u32),
        }
        let mut pending = vec![Task::Type(ty, depth)];
        while let Some(task) = pending.pop() {
            let (ty, depth) = match task {
                Task::Type(ty, depth) => (ty, depth),
                Task::Length(length) => {
                    self.u32(length)?;
                    continue;
                }
                Task::Types(types, depth) => {
                    if let Some((first, rest)) = types.split_first() {
                        pending.push(Task::Types(rest, depth));
                        pending.push(Task::Type(first, depth));
                    }
                    continue;
                }
                Task::Fields(fields, depth) => {
                    if let Some(((name, ty), rest)) = fields.split_first() {
                        self.text(name)?;
                        pending.push(Task::Fields(rest, depth));
                        pending.push(Task::Type(ty, depth));
                    }
                    continue;
                }
                Task::Variants(variants, depth) => {
                    if let Some(((name, fields), rest)) = variants.split_first() {
                        self.text(name)?;
                        self.count(fields.len(), self.limits.max_nodes)?;
                        pending.push(Task::Variants(rest, depth));
                        pending.push(Task::Types(fields, depth));
                    }
                    continue;
                }
            };
            if depth >= self.limits.max_call_depth {
                return Err(quota("candidate wire type nesting quota exceeded"));
            }
            self.charge(1)?;
            match ty {
                Type::Unit => self.u8(0)?,
                Type::Bool => self.u8(1)?,
                Type::I32 => self.u8(2)?,
                Type::I64 => self.u8(3)?,
                Type::U8 => self.u8(18)?,
                Type::U16 => self.u8(19)?,
                Type::ByteBuffer => self.u8(24)?,
                Type::ForeignHandle => self.u8(20)?,
                Type::CWideString => self.u8(21)?,
                Type::Tuple(types) => {
                    self.u8(4)?;
                    self.count(types.len(), self.limits.max_nodes)?;
                    pending.push(Task::Types(types, depth + 1));
                }
                Type::Array { element, length } => {
                    self.u8(14)?;
                    pending.push(Task::Length(*length));
                    pending.push(Task::Type(element, depth + 1));
                }
                Type::Slice { element } | Type::OwnedStore { element } => {
                    self.u8(if matches!(ty, Type::Slice { .. }) {
                        23
                    } else {
                        25
                    })?;
                    pending.push(Task::Type(element, depth + 1));
                }
                Type::Record { name, fields } | Type::Package { name, fields } => {
                    self.u8(if matches!(ty, Type::Record { .. }) {
                        13
                    } else {
                        17
                    })?;
                    self.text(name)?;
                    self.count(fields.len(), self.limits.max_nodes)?;
                    pending.push(Task::Fields(fields, depth + 1));
                }
                Type::Enum { name, variants } => {
                    self.u8(15)?;
                    self.text(name)?;
                    self.count(variants.len(), self.limits.max_nodes)?;
                    pending.push(Task::Variants(variants, depth + 1));
                }
                Type::ErrorUnion { error, result } => {
                    self.u8(16)?;
                    pending.push(Task::Type(result, depth + 1));
                    pending.push(Task::Type(error, depth + 1));
                }
                Type::Named(name) | Type::Unsupported(name) => {
                    self.u8(if matches!(ty, Type::Named(_)) { 5 } else { 12 })?;
                    self.text(name)?;
                }
                Type::Address { mutable, inner } => {
                    self.u8(6)?;
                    self.boolean(*mutable)?;
                    pending.push(Task::Type(inner, depth + 1));
                }
                Type::Managed(inner) | Type::Desc(inner) => {
                    self.u8(if matches!(ty, Type::Managed(_)) { 7 } else { 8 })?;
                    pending.push(Task::Type(inner, depth + 1));
                }
                Type::FunctionPointer { parameters, result }
                | Type::ForeignCallback { parameters, result } => {
                    self.u8(if matches!(ty, Type::FunctionPointer { .. }) {
                        9
                    } else {
                        22
                    })?;
                    self.count(parameters.len(), self.limits.max_locals)?;
                    pending.push(Task::Type(result, depth + 1));
                    pending.push(Task::Types(parameters, depth + 1));
                }
                Type::Closure(id) => {
                    self.u8(10)?;
                    self.u32(id.0)?;
                }
                Type::Opaque { identity, hidden } => {
                    self.u8(11)?;
                    self.u32(identity.0)?;
                    pending.push(Task::Type(hidden, depth + 1));
                }
            }
        }
        Ok(())
    }
    fn constant(&mut self, constant: &Constant) -> WireResult<()> {
        match constant {
            Constant::Unit => self.u8(0),
            Constant::U16(value) => {
                self.u8(5)?;
                self.u16(*value)
            }
            Constant::NullHandle => self.u8(6),
            Constant::NullWideString => self.u8(7),
            Constant::WideString(units) => {
                self.u8(8)?;
                self.count(units.len(), self.limits.max_nodes)?;
                for unit in units {
                    self.u16(*unit)?;
                }
                Ok(())
            }
            Constant::U8(value) => {
                self.u8(4)?;
                self.u8(*value)
            }
            Constant::Bool(value) => {
                self.u8(1)?;
                self.boolean(*value)
            }
            Constant::I32(value) => {
                self.u8(2)?;
                self.bytes(&value.to_le_bytes())
            }
            Constant::I64(value) => {
                self.u8(3)?;
                self.bytes(&value.to_le_bytes())
            }
        }
    }
    fn unary(&mut self, op: UnaryOp) -> WireResult<()> {
        self.u8(match op {
            UnaryOp::Negate => 0,
            UnaryOp::Not => 1,
            UnaryOp::BitNot => 2,
            UnaryOp::Dereference => 3,
            UnaryOp::Borrow => 4,
            UnaryOp::BorrowMut => 5,
            UnaryOp::Give => 6,
        })
    }
    fn binary(&mut self, op: BinaryOp) -> WireResult<()> {
        self.u8(match op {
            BinaryOp::Add => 0,
            BinaryOp::Subtract => 1,
            BinaryOp::Multiply => 2,
            BinaryOp::Divide => 3,
            BinaryOp::Remainder => 4,
            BinaryOp::AddWrapping => 5,
            BinaryOp::SubtractWrapping => 6,
            BinaryOp::MultiplyWrapping => 7,
            BinaryOp::Equal => 8,
            BinaryOp::NotEqual => 9,
            BinaryOp::Less => 10,
            BinaryOp::LessEqual => 11,
            BinaryOp::Greater => 12,
            BinaryOp::GreaterEqual => 13,
            BinaryOp::And => 14,
            BinaryOp::Or => 15,
            BinaryOp::BitAnd => 16,
            BinaryOp::BitOr => 17,
            BinaryOp::BitXor => 18,
            BinaryOp::ShiftLeft => 19,
            BinaryOp::ShiftRight => 20,
        })
    }
    fn unsupported(&mut self, features: &[UnsupportedFeature]) -> WireResult<()> {
        self.count(features.len(), self.limits.max_nodes)?;
        for feature in features {
            self.text(&feature.feature)?;
            self.optional_span(feature.span)?;
        }
        Ok(())
    }
    fn module(&mut self, module: &TypedModule) -> WireResult<()> {
        self.u32(module.schema_version)?;
        self.u32(module.module_id.0)?;
        self.fingerprint(module.source_fingerprint)?;
        self.target(&module.target)?;
        self.count(module.functions.len(), self.limits.max_functions)?;
        for function in &module.functions {
            self.function(function)?;
        }
        self.count(module.closures.len(), self.limits.max_functions)?;
        for closure in &module.closures {
            self.u32(closure.id.0)?;
            self.u32(closure.function.0)?;
            self.count(closure.captures.len(), self.limits.max_locals)?;
            for ty in &closure.captures {
                self.ty(ty, 0)?;
            }
            self.count(closure.parameters.len(), self.limits.max_locals)?;
            for ty in &closure.parameters {
                self.ty(ty, 0)?;
            }
            self.ty(&closure.result, 0)?;
            self.u8(match closure.kind {
                CallableKind::FunctionPointer => 0,
                CallableKind::Fn => 1,
                CallableKind::FnMut => 2,
                CallableKind::FnOnce => 3,
            })?;
        }
        self.count(module.resources.len(), self.limits.max_nodes)?;
        for resource in &module.resources {
            self.text(&resource.name)?;
            self.ty(&resource.payload, 0)?;
            self.boolean(resource.teardown.is_some())?;
            if let Some(teardown) = resource.teardown {
                self.u32(teardown.0)?;
            }
        }
        self.unsupported(&module.unsupported)
    }
    fn callable_contract(&mut self, c: &CallableParameterContract) -> WireResult<()> {
        self.u32(c.parameter)?;
        self.u32(c.closure.0)?;
        self.u8(callable_kind_tag(c.kind))?;
        for indices in [&c.hold, &c.call, &c.writes] {
            self.count(indices.len(), self.limits.max_locals)?;
            for index in indices {
                self.u32(*index)?;
            }
        }
        self.u8(match c.cleanup {
            CallableCleanup::Owned => 0,
            CallableCleanup::Borrowed => 1,
        })?;
        self.count(c.traces.len(), self.limits.max_analysis_steps)?;
        for trace in &c.traces {
            self.count(trace.calls.len(), self.limits.max_instructions)?;
            for call in &trace.calls {
                self.u8(callable_kind_tag(*call))?;
            }
            self.u8(match trace.exit {
                CallableExit::Return => 0,
                CallableExit::Trap => 1,
            })?;
        }
        Ok(())
    }
    fn function(&mut self, function: &TypedFunction) -> WireResult<()> {
        self.boolean(function.callable_return_contract.is_some())?;
        if let Some(c) = &function.callable_return_contract {
            self.u32(c.closure.0)?;
            self.u8(callable_kind_tag(c.kind))?;
            for indices in [&c.hold, &c.call, &c.writes, &c.cleanup] {
                self.count(indices.len(), self.limits.max_locals)?;
                for index in indices {
                    self.u32(*index)?;
                }
            }
        }
        self.boolean(function.callable_contract.is_some())?;
        if let Some(contract) = &function.callable_contract {
            self.callable_contract(contract)?;
        }
        self.u32(function.id.0)?;
        self.text(&function.name)?;
        self.boolean(function.foreign.is_some())?;
        if let Some(foreign) = &function.foreign {
            self.text(&foreign.abi)?;
            self.text(&foreign.symbol)?;
            self.count(foreign.noescape_parameters.len(), self.limits.max_locals)?;
            for parameter in &foreign.noescape_parameters {
                self.u32(*parameter)?;
            }
            self.boolean(foreign.static_callbacks)?;
        }
        self.boolean(function.environment.is_some())?;
        if let Some(environment) = function.environment {
            self.u32(environment.0)?;
        }
        self.count(function.parameters.len(), self.limits.max_locals)?;
        for parameter in &function.parameters {
            self.u32(parameter.0)?;
        }
        self.ty(&function.return_type, 0)?;
        self.u8(passing_mode_tag(function.result_mode))?;
        self.count(function.locals.len(), self.limits.max_locals)?;
        for local in &function.locals {
            self.u32(local.id.0)?;
            self.text(&local.name)?;
            self.ty(&local.ty, 0)?;
            self.boolean(local.mutable)?;
            self.span(local.span)?;
        }
        self.count(function.values.len(), self.limits.max_values)?;
        for ty in &function.values {
            self.ty(ty, 0)?;
        }
        self.count(function.blocks.len(), self.limits.max_blocks)?;
        for block in &function.blocks {
            self.u32(block.id.0)?;
            self.count(block.instructions.len(), self.limits.max_instructions)?;
            for instruction in &block.instructions {
                self.instruction(instruction)?;
            }
            match &block.terminator {
                Terminator::Return(value) => {
                    self.u8(0)?;
                    self.optional_value(*value)?;
                }
                Terminator::Jump(target) => {
                    self.u8(1)?;
                    self.u32(target.0)?;
                }
                Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                } => {
                    self.u8(2)?;
                    self.u32(condition.0)?;
                    self.u32(then_block.0)?;
                    self.u32(else_block.0)?;
                }
                Terminator::Unreachable => self.u8(3)?,
                Terminator::Trap => self.u8(4)?,
            }
        }
        self.u32(function.entry.0)?;
        self.span(function.span)
    }
    fn instruction(&mut self, instruction: &Instruction) -> WireResult<()> {
        self.optional_value(instruction.result)?;
        match &instruction.kind {
            InstructionKind::OwnedStoreNew { element } => {
                self.u8(80)?;
                self.ty(element, 0)?;
            }
            InstructionKind::OwnedStoreReserve { store, capacity } => {
                self.u8(81)?;
                self.u32(store.0)?;
                self.u32(capacity.0)?;
            }
            InstructionKind::OwnedStorePush { store, value } => {
                self.u8(82)?;
                self.u32(store.0)?;
                self.u32(value.0)?;
            }
            InstructionKind::OwnedStorePop { store } => {
                self.u8(83)?;
                self.u32(store.0)?;
            }
            InstructionKind::OwnedStoreLen { store } => {
                self.u8(84)?;
                self.u32(store.0)?;
            }
            InstructionKind::OwnedStoreCapacity { store } => {
                self.u8(85)?;
                self.u32(store.0)?;
            }
            InstructionKind::MakePackage { fields } => {
                self.u8(49)?;
                self.count(fields.len(), self.limits.max_values)?;
                for field in fields {
                    self.u32(field.0)?;
                }
            }
            InstructionKind::ReadPackageField { local, index } => {
                self.u8(50)?;
                self.u32(local.0)?;
                self.u32(*index)?;
            }
            InstructionKind::DescribePackageField { local, index } => {
                self.u8(51)?;
                self.u32(local.0)?;
                self.u32(*index)?;
            }
            InstructionKind::DropPackage { local } => {
                self.u8(52)?;
                self.u32(local.0)?;
            }
            InstructionKind::TakePackageField { local, index } => {
                self.u8(54)?;
                self.u32(local.0)?;
                self.u32(*index)?;
            }
            InstructionKind::DropPackageField { local, index } => {
                self.u8(55)?;
                self.u32(local.0)?;
                self.u32(*index)?;
            }
            InstructionKind::StorePackageField {
                local,
                index,
                value,
            } => {
                self.u8(56)?;
                self.u32(local.0)?;
                self.u32(*index)?;
                self.u32(value.0)?;
            }
            InstructionKind::BindPackageCapture { local, index } => {
                self.u8(57)?;
                self.u32(local.0)?;
                self.u32(*index)?;
            }
            InstructionKind::EndPackageCapture { local, index } => {
                self.u8(58)?;
                self.u32(local.0)?;
                self.u32(*index)?;
            }
            InstructionKind::MakeManaged { fields } => {
                self.u8(34)?;
                self.count(fields.len(), self.limits.max_values)?;
                for field in fields {
                    self.u32(field.0)?;
                }
            }
            InstructionKind::DescribeLocal { local } => {
                self.u8(35)?;
                self.u32(local.0)?;
            }
            InstructionKind::ReadDesc { desc, index } => {
                self.u8(36)?;
                self.u32(desc.0)?;
                self.u32(*index)?;
            }
            InstructionKind::WriteDesc { desc, index, value } => {
                self.u8(37)?;
                self.u32(desc.0)?;
                self.u32(*index)?;
                self.u32(value.0)?;
            }
            InstructionKind::DropManaged { local } => {
                self.u8(38)?;
                self.u32(local.0)?;
            }
            InstructionKind::DescribeCapture { index } => {
                self.u8(39)?;
                self.u32(*index)?;
            }
            InstructionKind::TakeCapture { index } => {
                self.u8(40)?;
                self.u32(*index)?;
            }
            InstructionKind::BorrowCapture { index, mutable } => {
                self.u8(78)?;
                self.u32(*index)?;
                self.boolean(*mutable)?;
            }
            InstructionKind::CallClosureBorrowed {
                callee,
                kind,
                arguments,
            } => {
                self.u8(53)?;
                self.u32(callee.0)?;
                self.u8(callable_kind_tag(*kind))?;
                self.count(arguments.len(), self.limits.max_values)?;
                for argument in arguments {
                    self.u32(argument.0)?;
                }
            }
            InstructionKind::CallClosureShared { callee, arguments } => {
                self.u8(41)?;
                self.u32(callee.0)?;
                self.count(arguments.len(), self.limits.max_values)?;
                for argument in arguments {
                    self.u32(argument.0)?;
                }
            }
            InstructionKind::CallClosureOnce { callee, arguments } => {
                self.u8(42)?;
                self.u32(callee.0)?;
                self.count(arguments.len(), self.limits.max_values)?;
                for argument in arguments {
                    self.u32(argument.0)?;
                }
            }
            InstructionKind::DropClosure { local } => {
                self.u8(43)?;
                self.u32(local.0)?;
            }
            InstructionKind::UnsafeEnter { region } => {
                self.u8(31)?;
                self.u32(*region)?;
            }
            InstructionKind::UnsafeExit { region } => {
                self.u8(32)?;
                self.u32(*region)?;
            }
            InstructionKind::ForeignCall {
                function,
                arguments,
                region,
            } => {
                self.u8(33)?;
                self.u32(function.0)?;
                self.u32(*region)?;
                self.count(arguments.len(), self.limits.max_values)?;
                for argument in arguments {
                    self.u32(argument.0)?;
                }
            }
            InstructionKind::Constant(constant) => {
                self.u8(0)?;
                self.constant(constant)?;
            }
            InstructionKind::Load(local) => {
                self.u8(1)?;
                self.u32(local.0)?;
            }
            InstructionKind::Store { local, value } => {
                self.u8(2)?;
                self.u32(local.0)?;
                self.u32(value.0)?;
            }
            InstructionKind::ConvertInteger { value, target } => {
                self.u8(59)?;
                self.u32(value.0)?;
                self.ty(target, 0)?;
            }
            InstructionKind::Unary { op, operand } => {
                self.u8(3)?;
                self.unary(*op)?;
                self.u32(operand.0)?;
            }
            InstructionKind::Binary { op, left, right } => {
                self.u8(4)?;
                self.binary(*op)?;
                self.u32(left.0)?;
                self.u32(right.0)?;
            }
            InstructionKind::Call {
                function,
                arguments,
            } => {
                self.u8(5)?;
                self.u32(function.0)?;
                self.count(arguments.len(), self.limits.max_values)?;
                for argument in arguments {
                    self.u32(argument.0)?;
                }
            }
            InstructionKind::ForeignFunctionRef { function } => {
                self.u8(60)?;
                self.u32(function.0)?;
            }
            InstructionKind::FunctionRef { function } => {
                self.u8(25)?;
                self.u32(function.0)?;
            }
            InstructionKind::CallIndirect { callee, arguments } => {
                self.u8(26)?;
                self.u32(callee.0)?;
                self.count(arguments.len(), self.limits.max_values)?;
                for argument in arguments {
                    self.u32(argument.0)?;
                }
            }
            InstructionKind::ExtractIndex { aggregate, index } => {
                self.u8(16)?;
                self.u32(aggregate.0)?;
                self.u32(index.0)?;
            }
            InstructionKind::UpdateIndex {
                aggregate,
                index,
                value,
            } => {
                self.u8(17)?;
                self.u32(aggregate.0)?;
                self.u32(index.0)?;
                self.u32(value.0)?;
            }
            InstructionKind::MakeAggregate { fields } => {
                self.u8(13)?;
                self.count(fields.len(), self.limits.max_values)?;
                for value in fields {
                    self.u32(value.0)?;
                }
            }
            InstructionKind::MakeVariant { variant, fields } => {
                self.u8(18)?;
                self.u32(*variant)?;
                self.count(fields.len(), self.limits.max_values)?;
                for value in fields {
                    self.u32(value.0)?;
                }
            }
            InstructionKind::VariantTag { value } => {
                self.u8(19)?;
                self.u32(value.0)?;
            }
            InstructionKind::ReadVariant {
                value,
                variant,
                index,
            } => {
                self.u8(20)?;
                self.u32(value.0)?;
                self.u32(*variant)?;
                self.u32(*index)?;
            }
            InstructionKind::ExtractField { aggregate, index } => {
                self.u8(14)?;
                self.u32(aggregate.0)?;
                self.u32(*index)?;
            }
            InstructionKind::UpdateField {
                aggregate,
                index,
                value,
            } => {
                self.u8(15)?;
                self.u32(aggregate.0)?;
                self.u32(*index)?;
                self.u32(value.0)?;
            }
            InstructionKind::MakeClosure { closure, captures } => {
                self.u8(6)?;
                self.u32(closure.0)?;
                self.count(captures.len(), self.limits.max_values)?;
                for capture in captures {
                    self.u32(capture.0)?;
                }
            }
            InstructionKind::ReadCapture { index } => {
                self.u8(7)?;
                self.u32(*index)?;
            }
            InstructionKind::WriteCapture { index, value } => {
                self.u8(22)?;
                self.u32(*index)?;
                self.u32(value.0)?;
            }
            InstructionKind::CallClosureMut { callee, arguments } => {
                self.u8(21)?;
                self.u32(callee.0)?;
                self.count(arguments.len(), self.limits.max_values)?;
                for argument in arguments {
                    self.u32(argument.0)?;
                }
            }
            InstructionKind::CallClosure { callee, arguments } => {
                self.u8(8)?;
                self.u32(callee.0)?;
                self.count(arguments.len(), self.limits.max_values)?;
                for argument in arguments {
                    self.u32(argument.0)?;
                }
            }
            InstructionKind::BorrowLocal { local } => {
                self.u8(9)?;
                self.u32(local.0)?;
            }
            InstructionKind::BorrowLocalMut { local } => {
                self.u8(27)?;
                self.u32(local.0)?;
            }
            InstructionKind::ReborrowAddress { address, mutable } => {
                self.u8(28)?;
                self.u32(address.0)?;
                self.boolean(*mutable)?;
            }
            InstructionKind::WriteAddress { address, value } => {
                self.u8(29)?;
                self.u32(address.0)?;
                self.u32(value.0)?;
            }
            InstructionKind::TakeLocal { local } => {
                self.u8(30)?;
                self.u32(local.0)?;
            }
            InstructionKind::ReadAddress { address } => {
                self.u8(10)?;
                self.u32(address.0)?;
            }
            InstructionKind::AddressField { address, index } => {
                self.u8(23)?;
                self.u32(address.0)?;
                self.u32(*index)?;
            }
            InstructionKind::AddressIndex { address, index } => {
                self.u8(24)?;
                self.u32(address.0)?;
                self.u32(index.0)?;
            }
            InstructionKind::AddressSlice {
                address,
                start,
                end,
            } => {
                self.u8(61)?;
                self.u32(address.0)?;
                self.optional_value(*start)?;
                self.optional_value(*end)?;
            }
            InstructionKind::SliceLength { address } => {
                self.u8(62)?;
                self.u32(address.0)?;
            }
            InstructionKind::ByteBufferNew => self.u8(63)?,
            InstructionKind::ByteBufferReserve { buffer, capacity } => {
                self.u8(64)?;
                self.u32(buffer.0)?;
                self.u32(capacity.0)?;
            }
            InstructionKind::ByteBufferPush { buffer, value } => {
                self.u8(65)?;
                self.u32(buffer.0)?;
                self.u32(value.0)?;
            }
            InstructionKind::ByteBufferTruncate { buffer, length } => {
                self.u8(69)?;
                self.u32(buffer.0)?;
                self.u32(length.0)?;
            }
            InstructionKind::ByteBufferLen { buffer } => {
                self.u8(66)?;
                self.u32(buffer.0)?;
            }
            InstructionKind::ByteBufferCapacity { buffer } => {
                self.u8(67)?;
                self.u32(buffer.0)?;
            }
            InstructionKind::ByteBufferBorrow { buffer, mutable } => {
                self.u8(68)?;
                self.u32(buffer.0)?;
                self.boolean(*mutable)?;
            }
            InstructionKind::StorageLive(local) => {
                self.u8(11)?;
                self.u32(local.0)?;
            }
            InstructionKind::StorageDead(local) => {
                self.u8(12)?;
                self.u32(local.0)?;
            }
            InstructionKind::ScopeEnter { scope } => {
                self.u8(44)?;
                self.u32(scope.0)?;
            }
            InstructionKind::ScopeExit { scope, kind } => {
                self.u8(45)?;
                self.u32(scope.0)?;
                self.u8(cleanup_exit_tag(*kind))?;
            }
            InstructionKind::MakeErrorUnion { is_error, value } => {
                self.u8(46)?;
                self.boolean(*is_error)?;
                self.u32(value.0)?;
            }
            InstructionKind::MakeOwnedArray { elements } => {
                self.u8(72)?;
                self.count(elements.len(), self.limits.max_values)?;
                for element in elements {
                    self.u32(element.0)?;
                }
            }
            InstructionKind::ProjectOwnedArrayField {
                local,
                index,
                field,
            } => {
                self.u8(73)?;
                self.u32(local.0)?;
                self.u32(*index)?;
                self.u32(*field)?;
            }
            InstructionKind::TakeOwnedArrayElement { local, index } => {
                self.u8(74)?;
                self.u32(local.0)?;
                self.u32(*index)?;
            }
            InstructionKind::DropOwnedArrayElement { local, index } => {
                self.u8(75)?;
                self.u32(local.0)?;
                self.u32(*index)?;
            }
            InstructionKind::StoreOwnedArrayElement {
                local,
                index,
                value,
            } => {
                self.u8(76)?;
                self.u32(local.0)?;
                self.u32(*index)?;
                self.u32(value.0)?;
            }
            InstructionKind::DropOwnedArray { local } => {
                self.u8(77)?;
                self.u32(local.0)?;
            }
            InstructionKind::BorrowOwnedArrayElement {
                local,
                index,
                mutable,
            } => {
                self.u8(79)?;
                self.u32(local.0)?;
                self.u32(*index)?;
                self.boolean(*mutable)?;
            }
            InstructionKind::ErrorTag { value } => {
                self.u8(47)?;
                self.u32(value.0)?;
            }
            InstructionKind::ReadErrorUnion { value, is_error } => {
                self.u8(48)?;
                self.u32(value.0)?;
                self.boolean(*is_error)?;
            }
            InstructionKind::TakeErrorUnion { value, is_error } => {
                self.u8(70)?;
                self.u32(value.0)?;
                self.boolean(*is_error)?;
            }
            InstructionKind::DropErrorUnion { local } => {
                self.u8(71)?;
                self.u32(local.0)?;
            }
            InstructionKind::Unsupported(feature) => {
                self.u8(255)?;
                self.text(feature)?;
            }
        }
        self.span(instruction.span)
    }
    fn locals(&mut self, locals: &[LocalId]) -> WireResult<()> {
        self.count(locals.len(), self.limits.max_locals)?;
        for local in locals {
            self.u32(local.0)?;
        }
        Ok(())
    }
    fn plan(&mut self, plan: &CandidatePlan) -> WireResult<()> {
        self.u32(plan.schema_version)?;
        self.fingerprint(plan.module_fingerprint)?;
        self.fingerprint(plan.target_fingerprint)?;
        self.count(plan.functions.len(), self.limits.max_functions)?;
        for function in &plan.functions {
            self.u32(function.function.0)?;
            self.count(function.blocks.len(), self.limits.max_blocks)?;
            for block in &function.blocks {
                self.u32(block.block.0)?;
                self.locals(&block.initialized_in)?;
                self.locals(&block.initialized_out)?;
            }
            self.count(function.holds.len(), self.limits.max_values)?;
            for hold in &function.holds {
                self.u32(hold.value.0)?;
                self.count(hold.roots.len(), self.limits.max_nodes)?;
                for root in &hold.roots {
                    match root {
                        HoldRoot::Local(local) => {
                            self.u8(0)?;
                            self.u32(local.0)?;
                        }
                        HoldRoot::Capture(index) => {
                            self.u8(1)?;
                            self.u32(*index)?;
                        }
                        HoldRoot::Parameter(index) => {
                            self.u8(2)?;
                            self.u32(*index)?;
                        }
                    }
                }
            }
        }
        self.count(plan.origins.len(), self.limits.max_nodes)?;
        for origin in &plan.origins {
            self.u32(origin.origin.0)?;
            self.u32(origin.source.0)?;
            self.u8(match origin.multiplicity {
                OriginMultiplicity::One => 0,
                OriginMultiplicity::Many => 1,
            })?;
        }
        self.count(plan.responsibilities.len(), self.limits.max_nodes)?;
        for claim in &plan.responsibilities {
            self.u32(claim.function.0)?;
            self.u32(claim.carrier.0)?;
            self.u32(claim.origin.0)?;
            self.u8(match claim.state {
                ResponsibilityState::Uninitialized => 0,
                ResponsibilityState::Live => 1,
                ResponsibilityState::Done => 2,
                ResponsibilityState::Moved => 3,
            })?;
        }
        self.unsupported(&plan.unsupported)
    }
}

struct Reader<'a> {
    input: &'a [u8],
    position: usize,
    limits: &'a Limits,
    nodes: usize,
}
impl<'a> Reader<'a> {
    fn new(input: &'a [u8], limits: &'a Limits) -> WireResult<Self> {
        if input.len() > limits.max_source_bytes {
            return Err(quota("candidate wire byte quota exceeded"));
        }
        Ok(Self {
            input,
            position: 0,
            limits,
            nodes: limits.max_nodes,
        })
    }
    fn remaining(&self) -> usize {
        self.input.len() - self.position
    }
    fn take(&mut self, count: usize) -> WireResult<&'a [u8]> {
        let end = self
            .position
            .checked_add(count)
            .filter(|&end| end <= self.input.len())
            .ok_or_else(|| {
                malformed(
                    "E-WIRE-TRUNCATED",
                    "candidate wire ends inside a required record",
                )
            })?;
        let bytes = &self.input[self.position..end];
        self.position = end;
        Ok(bytes)
    }
    fn u8(&mut self) -> WireResult<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> WireResult<u16> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("checked length"),
        ))
    }
    fn u32(&mut self) -> WireResult<u32> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("checked length"),
        ))
    }
    fn u64(&mut self) -> WireResult<u64> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("checked length"),
        ))
    }
    fn boolean(&mut self) -> WireResult<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(malformed("E-WIRE-BOOL", "wire boolean must be zero or one")),
        }
    }
    fn fingerprint(&mut self) -> WireResult<Fingerprint> {
        Ok(Fingerprint(
            self.take(32)?.try_into().expect("checked length"),
        ))
    }
    fn header(&mut self, magic: &[u8]) -> WireResult<()> {
        if self.take(magic.len())? != magic {
            return Err(malformed("E-WIRE-MAGIC", "candidate wire kind is invalid"));
        }
        if self.u32()? != WIRE_VERSION {
            return Err(malformed(
                "E-WIRE-VERSION",
                "candidate wire version is unsupported",
            ));
        }
        Ok(())
    }
    fn finish(&self) -> WireResult<()> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(malformed(
                "E-WIRE-TRAILING",
                "trailing bytes after the candidate record",
            ))
        }
    }
    fn charge(&mut self, count: usize) -> WireResult<()> {
        self.nodes = self
            .nodes
            .checked_sub(count)
            .ok_or_else(|| quota("candidate wire node quota exceeded"))?;
        Ok(())
    }
    fn count(&mut self, maximum: usize, minimum_record_bytes: usize) -> WireResult<usize> {
        let count = self.u32()? as usize;
        if count > maximum {
            return Err(quota("candidate wire collection quota exceeded"));
        }
        self.charge(count)?;
        if count
            .checked_mul(minimum_record_bytes)
            .is_none_or(|n| n > self.remaining())
        {
            return Err(malformed(
                "E-WIRE-TRUNCATED",
                "declared collection cannot fit in the remaining wire bytes",
            ));
        }
        Ok(count)
    }
    fn text(&mut self) -> WireResult<String> {
        let length = self.u32()? as usize;
        if length > self.limits.max_source_bytes {
            return Err(quota("candidate wire string quota exceeded"));
        }
        let bytes = self.take(length)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| malformed("E-WIRE-UTF8", "wire string is not UTF-8"))
    }
    fn span(&mut self) -> WireResult<Span> {
        let file = FileId(self.u32()?);
        let start = usize::try_from(self.u64()?)
            .map_err(|_| quota("wire source position exceeds host address width"))?;
        let end = usize::try_from(self.u64()?)
            .map_err(|_| quota("wire source position exceeds host address width"))?;
        if start > end {
            return Err(malformed("E-WIRE-SPAN", "wire span has reversed bounds"));
        }
        Ok(Span::new(file, start, end))
    }
    fn optional_span(&mut self) -> WireResult<Option<Span>> {
        if self.boolean()? {
            Ok(Some(self.span()?))
        } else {
            Ok(None)
        }
    }
    fn optional_value(&mut self) -> WireResult<Option<ValueId>> {
        if self.boolean()? {
            Ok(Some(ValueId(self.u32()?)))
        } else {
            Ok(None)
        }
    }
    fn target(&mut self) -> WireResult<TargetProfile> {
        let id = self.text()?;
        let triple = self.text()?;
        let pointer_width = self.u16()?;
        let endian = match self.u8()? {
            0 => Endian::Little,
            1 => Endian::Big,
            _ => return Err(malformed("E-WIRE-TAG", "unknown endian tag")),
        };
        let backend = self.text()?;
        let rule_version = self.text()?;
        let checked_arithmetic = self.boolean()?;
        Ok(TargetProfile {
            id,
            triple,
            pointer_width,
            endian,
            backend,
            rule_version,
            checked_arithmetic,
        })
    }
    fn ty(&mut self, depth: usize) -> WireResult<Type> {
        // Finish tasks assemble already charged children. Names and variants
        // have their own stacks so nested records/enums cannot consume a
        // parent's unfinished fields. All lengths retain count() admission.
        enum Task {
            Type(usize),
            Types {
                left: usize,
                depth: usize,
            },
            Fields {
                left: usize,
                depth: usize,
            },
            Variants {
                left: usize,
                depth: usize,
            },
            Tuple(usize),
            Function {
                start: usize,
                foreign: bool,
            },
            Record {
                start: usize,
                names: usize,
                name: String,
                package: bool,
            },
            Enum {
                start: usize,
                name: String,
            },
            Variant {
                start: usize,
                name: String,
            },
            Array,
            Unary(u8),
            Address(bool),
            Opaque(NodeId),
            ErrorUnion,
        }
        let mut pending = vec![Task::Type(depth)];
        let mut values: Vec<Type> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        let mut variants: Vec<(String, Vec<Type>)> = Vec::new();
        while let Some(task) = pending.pop() {
            let depth = match task {
                Task::Type(depth) => depth,
                Task::Types { left, depth } => {
                    if left > 0 {
                        pending.push(Task::Types {
                            left: left - 1,
                            depth,
                        });
                        pending.push(Task::Type(depth));
                    }
                    continue;
                }
                Task::Fields { left, depth } => {
                    if left > 0 {
                        names.push(self.text()?);
                        pending.push(Task::Fields {
                            left: left - 1,
                            depth,
                        });
                        pending.push(Task::Type(depth));
                    }
                    continue;
                }
                Task::Variants { left, depth } => {
                    if left > 0 {
                        let name = self.text()?;
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Task::Variants {
                            left: left - 1,
                            depth,
                        });
                        pending.push(Task::Variant {
                            start: values.len(),
                            name,
                        });
                        pending.push(Task::Types { left: count, depth });
                    }
                    continue;
                }
                Task::Tuple(start) => {
                    let types = values.split_off(start);
                    values.push(Type::Tuple(types));
                    continue;
                }
                Task::Function { start, foreign } => {
                    let result = Box::new(values.pop().expect("read function result"));
                    let parameters = values.split_off(start);
                    values.push(if foreign {
                        Type::ForeignCallback { parameters, result }
                    } else {
                        Type::FunctionPointer { parameters, result }
                    });
                    continue;
                }
                Task::Record {
                    start,
                    names: names_start,
                    name,
                    package,
                } => {
                    let types = values.split_off(start);
                    let fields = names
                        .split_off(names_start)
                        .into_iter()
                        .zip(types)
                        .collect();
                    values.push(if package {
                        Type::Package { name, fields }
                    } else {
                        Type::Record { name, fields }
                    });
                    continue;
                }
                Task::Enum { start, name } => {
                    let members = variants.split_off(start);
                    values.push(Type::Enum {
                        name,
                        variants: members,
                    });
                    continue;
                }
                Task::Variant { start, name } => {
                    variants.push((name, values.split_off(start)));
                    continue;
                }
                Task::Array => {
                    let element = Box::new(values.pop().expect("read array element"));
                    values.push(Type::Array {
                        element,
                        length: self.u32()?,
                    });
                    continue;
                }
                Task::Unary(tag) => {
                    let inner = Box::new(values.pop().expect("read unary type child"));
                    values.push(match tag {
                        7 => Type::Managed(inner),
                        8 => Type::Desc(inner),
                        23 => Type::Slice { element: inner },
                        25 => Type::OwnedStore { element: inner },
                        _ => unreachable!("scheduled unary type tag"),
                    });
                    continue;
                }
                Task::Address(mutable) => {
                    let inner = Box::new(values.pop().expect("read address child"));
                    values.push(Type::Address { mutable, inner });
                    continue;
                }
                Task::Opaque(identity) => {
                    let hidden = Box::new(values.pop().expect("read opaque child"));
                    values.push(Type::Opaque { identity, hidden });
                    continue;
                }
                Task::ErrorUnion => {
                    let result = Box::new(values.pop().expect("read error-union result"));
                    let error = Box::new(values.pop().expect("read error-union error"));
                    values.push(Type::ErrorUnion { error, result });
                    continue;
                }
            };
            if depth >= self.limits.max_call_depth {
                return Err(quota("candidate wire type nesting quota exceeded"));
            }
            self.charge(1)?;
            let tag = self.u8()?;
            let ty = match tag {
                0 => Type::Unit,
                1 => Type::Bool,
                2 => Type::I32,
                3 => Type::I64,
                18 => Type::U8,
                19 => Type::U16,
                20 => Type::ForeignHandle,
                21 => Type::CWideString,
                24 => Type::ByteBuffer,
                4 => {
                    let count = self.count(self.limits.max_nodes, 1)?;
                    pending.push(Task::Tuple(values.len()));
                    pending.push(Task::Types {
                        left: count,
                        depth: depth + 1,
                    });
                    continue;
                }
                9 | 22 => {
                    let count = self.count(self.limits.max_locals, 1)?;
                    pending.push(Task::Function {
                        start: values.len(),
                        foreign: tag == 22,
                    });
                    pending.push(Task::Type(depth + 1));
                    pending.push(Task::Types {
                        left: count,
                        depth: depth + 1,
                    });
                    continue;
                }
                13 | 17 => {
                    let name = self.text()?;
                    let count = self.count(self.limits.max_nodes, 2)?;
                    pending.push(Task::Record {
                        start: values.len(),
                        names: names.len(),
                        name,
                        package: tag == 17,
                    });
                    pending.push(Task::Fields {
                        left: count,
                        depth: depth + 1,
                    });
                    continue;
                }
                15 => {
                    let name = self.text()?;
                    let count = self.count(self.limits.max_nodes, 2)?;
                    pending.push(Task::Enum {
                        start: variants.len(),
                        name,
                    });
                    pending.push(Task::Variants {
                        left: count,
                        depth: depth + 1,
                    });
                    continue;
                }
                14 => {
                    pending.push(Task::Array);
                    pending.push(Task::Type(depth + 1));
                    continue;
                }
                7 | 8 | 23 | 25 => {
                    pending.push(Task::Unary(tag));
                    pending.push(Task::Type(depth + 1));
                    continue;
                }
                6 => {
                    pending.push(Task::Address(self.boolean()?));
                    pending.push(Task::Type(depth + 1));
                    continue;
                }
                11 => {
                    pending.push(Task::Opaque(NodeId(self.u32()?)));
                    pending.push(Task::Type(depth + 1));
                    continue;
                }
                16 => {
                    pending.push(Task::ErrorUnion);
                    pending.push(Task::Type(depth + 1));
                    pending.push(Task::Type(depth + 1));
                    continue;
                }
                5 => Type::Named(self.text()?),
                10 => Type::Closure(ClosureId(self.u32()?)),
                12 => Type::Unsupported(self.text()?),
                _ => return Err(malformed("E-WIRE-TYPE", "unknown wire type tag")),
            };
            values.push(ty);
        }
        Ok(values.pop().expect("read exactly one root type"))
    }
    fn constant(&mut self) -> WireResult<Constant> {
        Ok(match self.u8()? {
            0 => Constant::Unit,
            1 => Constant::Bool(self.boolean()?),
            4 => Constant::U8(self.u8()?),
            5 => Constant::U16(self.u16()?),
            6 => Constant::NullHandle,
            7 => Constant::NullWideString,
            8 => {
                let count = self.count(self.limits.max_nodes, 2)?;
                let mut units = Vec::with_capacity(count);
                for _ in 0..count {
                    units.push(self.u16()?);
                }
                Constant::WideString(units)
            }
            2 => Constant::I32(i32::from_le_bytes(
                self.take(4)?.try_into().expect("checked length"),
            )),
            3 => Constant::I64(i64::from_le_bytes(
                self.take(8)?.try_into().expect("checked length"),
            )),
            _ => return Err(malformed("E-WIRE-CONSTANT", "unknown wire constant tag")),
        })
    }
    fn unary(&mut self) -> WireResult<UnaryOp> {
        Ok(match self.u8()? {
            0 => UnaryOp::Negate,
            1 => UnaryOp::Not,
            2 => UnaryOp::BitNot,
            3 => UnaryOp::Dereference,
            4 => UnaryOp::Borrow,
            5 => UnaryOp::BorrowMut,
            6 => UnaryOp::Give,
            _ => return Err(malformed("E-WIRE-OPCODE", "unknown unary opcode")),
        })
    }
    fn binary(&mut self) -> WireResult<BinaryOp> {
        Ok(match self.u8()? {
            0 => BinaryOp::Add,
            1 => BinaryOp::Subtract,
            2 => BinaryOp::Multiply,
            3 => BinaryOp::Divide,
            4 => BinaryOp::Remainder,
            5 => BinaryOp::AddWrapping,
            6 => BinaryOp::SubtractWrapping,
            7 => BinaryOp::MultiplyWrapping,
            8 => BinaryOp::Equal,
            9 => BinaryOp::NotEqual,
            10 => BinaryOp::Less,
            11 => BinaryOp::LessEqual,
            12 => BinaryOp::Greater,
            13 => BinaryOp::GreaterEqual,
            14 => BinaryOp::And,
            15 => BinaryOp::Or,
            16 => BinaryOp::BitAnd,
            17 => BinaryOp::BitOr,
            18 => BinaryOp::BitXor,
            19 => BinaryOp::ShiftLeft,
            20 => BinaryOp::ShiftRight,
            _ => return Err(malformed("E-WIRE-OPCODE", "unknown binary opcode")),
        })
    }
    fn unsupported(&mut self) -> WireResult<Vec<UnsupportedFeature>> {
        let count = self.count(self.limits.max_nodes, 5)?;
        let mut features = Vec::with_capacity(count);
        for _ in 0..count {
            features.push(UnsupportedFeature {
                feature: self.text()?,
                span: self.optional_span()?,
            });
        }
        Ok(features)
    }
    fn module(&mut self) -> WireResult<TypedModule> {
        let schema_version = self.u32()?;
        let module_id = ModuleId(self.u32()?);
        let source_fingerprint = self.fingerprint()?;
        let target = self.target()?;
        let count = self.count(self.limits.max_functions, 4)?;
        let mut functions = Vec::with_capacity(count);
        for _ in 0..count {
            functions.push(self.function()?);
        }
        let count = self.count(self.limits.max_functions, 18)?;
        let mut closures = Vec::with_capacity(count);
        for _ in 0..count {
            let id = ClosureId(self.u32()?);
            let function = FunctionId(self.u32()?);
            let count = self.count(self.limits.max_locals, 1)?;
            let mut captures = Vec::with_capacity(count);
            for _ in 0..count {
                captures.push(self.ty(0)?);
            }
            let count = self.count(self.limits.max_locals, 1)?;
            let mut parameters = Vec::with_capacity(count);
            for _ in 0..count {
                parameters.push(self.ty(0)?);
            }
            let result = self.ty(0)?;
            let kind = match self.u8()? {
                0 => CallableKind::FunctionPointer,
                1 => CallableKind::Fn,
                2 => CallableKind::FnMut,
                3 => CallableKind::FnOnce,
                _ => return Err(malformed("E-WIRE-TAG", "unknown callable kind")),
            };
            closures.push(ClosureDefinition {
                id,
                function,
                captures,
                parameters,
                result,
                kind,
            });
        }
        let count = self.count(self.limits.max_nodes, 6)?;
        let mut resources = Vec::with_capacity(count);
        for _ in 0..count {
            resources.push(ResourceDefinition {
                name: self.text()?,
                payload: self.ty(0)?,
                teardown: if self.boolean()? {
                    Some(FunctionId(self.u32()?))
                } else {
                    None
                },
            });
        }
        Ok(TypedModule {
            resources,
            schema_version,
            module_id,
            source_fingerprint,
            target,
            functions,
            closures,
            unsupported: self.unsupported()?,
        })
    }
    fn callable_kind(&mut self) -> WireResult<CallableKind> {
        match self.u8()? {
            0 => Ok(CallableKind::FunctionPointer),
            1 => Ok(CallableKind::Fn),
            2 => Ok(CallableKind::FnMut),
            3 => Ok(CallableKind::FnOnce),
            _ => Err(malformed("E-WIRE-TAG", "unknown callable kind")),
        }
    }
    fn callable_indices(&mut self) -> WireResult<Vec<u32>> {
        let count = self.count(self.limits.max_locals, 4)?;
        let mut result = Vec::with_capacity(count);
        for _ in 0..count {
            result.push(self.u32()?);
        }
        Ok(result)
    }
    fn callable_contract(&mut self) -> WireResult<CallableParameterContract> {
        let parameter = self.u32()?;
        let closure = ClosureId(self.u32()?);
        let kind = self.callable_kind()?;
        let hold = self.callable_indices()?;
        let call = self.callable_indices()?;
        let writes = self.callable_indices()?;
        let cleanup = match self.u8()? {
            0 => CallableCleanup::Owned,
            1 => CallableCleanup::Borrowed,
            _ => return Err(malformed("E-WIRE-TAG", "unknown callable cleanup")),
        };
        let count = self.count(self.limits.max_analysis_steps, 5)?;
        let mut traces = Vec::with_capacity(count);
        for _ in 0..count {
            let count = self.count(self.limits.max_instructions, 1)?;
            let mut calls = Vec::with_capacity(count);
            for _ in 0..count {
                calls.push(self.callable_kind()?);
            }
            let exit = match self.u8()? {
                0 => CallableExit::Return,
                1 => CallableExit::Trap,
                _ => return Err(malformed("E-WIRE-TAG", "unknown callable exit")),
            };
            traces.push(CallableTrace { calls, exit });
        }
        Ok(CallableParameterContract {
            parameter,
            closure,
            kind,
            hold,
            call,
            writes,
            cleanup,
            traces,
        })
    }
    fn function(&mut self) -> WireResult<TypedFunction> {
        let callable_return_contract = if self.boolean()? {
            Some(CallableReturnContract {
                closure: ClosureId(self.u32()?),
                kind: self.callable_kind()?,
                hold: self.callable_indices()?,
                call: self.callable_indices()?,
                writes: self.callable_indices()?,
                cleanup: self.callable_indices()?,
            })
        } else {
            None
        };
        let callable_contract = if self.boolean()? {
            Some(self.callable_contract()?)
        } else {
            None
        };
        let id = FunctionId(self.u32()?);
        let name = self.text()?;
        let foreign = if self.boolean()? {
            Some(ForeignDeclaration {
                abi: self.text()?,
                symbol: self.text()?,
                noescape_parameters: self.callable_indices()?,
                static_callbacks: self.boolean()?,
            })
        } else {
            None
        };
        let environment = if self.boolean()? {
            Some(ClosureId(self.u32()?))
        } else {
            None
        };
        let count = self.count(self.limits.max_locals, 4)?;
        let mut parameters = Vec::with_capacity(count);
        for _ in 0..count {
            parameters.push(LocalId(self.u32()?));
        }
        let return_type = self.ty(0)?;
        let result_mode = match self.u8()? {
            0 => PassingMode::Copy,
            1 => PassingMode::Move,
            2 => PassingMode::Describe,
            3 => PassingMode::Create,
            4 => PassingMode::Give,
            5 => PassingMode::Borrow,
            6 => PassingMode::BorrowMut,
            7 => PassingMode::Infer,
            _ => return Err(malformed("E-WIRE-TAG", "unknown result passing mode")),
        };
        let count = self.count(self.limits.max_locals, 30)?;
        let mut locals = Vec::with_capacity(count);
        for _ in 0..count {
            locals.push(LocalDecl {
                id: LocalId(self.u32()?),
                name: self.text()?,
                ty: self.ty(0)?,
                mutable: self.boolean()?,
                span: self.span()?,
            });
        }
        let count = self.count(self.limits.max_values, 1)?;
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(self.ty(0)?);
        }
        let count = self.count(self.limits.max_blocks, 9)?;
        let mut blocks = Vec::with_capacity(count);
        for _ in 0..count {
            let id = BlockId(self.u32()?);
            let count = self.count(self.limits.max_instructions, 22)?;
            let mut instructions = Vec::with_capacity(count);
            for _ in 0..count {
                instructions.push(self.instruction()?);
            }
            let terminator = match self.u8()? {
                0 => Terminator::Return(self.optional_value()?),
                1 => Terminator::Jump(BlockId(self.u32()?)),
                2 => Terminator::Branch {
                    condition: ValueId(self.u32()?),
                    then_block: BlockId(self.u32()?),
                    else_block: BlockId(self.u32()?),
                },
                3 => Terminator::Unreachable,
                4 => Terminator::Trap,
                _ => return Err(malformed("E-WIRE-OPCODE", "unknown terminator opcode")),
            };
            blocks.push(BasicBlock {
                id,
                instructions,
                terminator,
            });
        }
        Ok(TypedFunction {
            callable_return_contract,
            callable_contract,
            result_mode,
            foreign,
            id,
            name,
            environment,
            parameters,
            return_type,
            locals,
            values,
            blocks,
            entry: BlockId(self.u32()?),
            span: self.span()?,
        })
    }
    fn instruction(&mut self) -> WireResult<Instruction> {
        let result = self.optional_value()?;
        let kind = match self.u8()? {
            80 => InstructionKind::OwnedStoreNew {
                element: self.ty(0)?,
            },
            81 => InstructionKind::OwnedStoreReserve {
                store: ValueId(self.u32()?),
                capacity: ValueId(self.u32()?),
            },
            82 => InstructionKind::OwnedStorePush {
                store: ValueId(self.u32()?),
                value: ValueId(self.u32()?),
            },
            83 => InstructionKind::OwnedStorePop {
                store: ValueId(self.u32()?),
            },
            84 => InstructionKind::OwnedStoreLen {
                store: ValueId(self.u32()?),
            },
            85 => InstructionKind::OwnedStoreCapacity {
                store: ValueId(self.u32()?),
            },
            0 => InstructionKind::Constant(self.constant()?),
            1 => InstructionKind::Load(LocalId(self.u32()?)),
            2 => InstructionKind::Store {
                local: LocalId(self.u32()?),
                value: ValueId(self.u32()?),
            },
            59 => InstructionKind::ConvertInteger {
                value: ValueId(self.u32()?),
                target: self.ty(0)?,
            },
            3 => InstructionKind::Unary {
                op: self.unary()?,
                operand: ValueId(self.u32()?),
            },
            4 => InstructionKind::Binary {
                op: self.binary()?,
                left: ValueId(self.u32()?),
                right: ValueId(self.u32()?),
            },
            5 => {
                let function = FunctionId(self.u32()?);
                let count = self.count(self.limits.max_values, 4)?;
                let mut arguments = Vec::with_capacity(count);
                for _ in 0..count {
                    arguments.push(ValueId(self.u32()?));
                }
                InstructionKind::Call {
                    function,
                    arguments,
                }
            }
            6 => {
                let closure = ClosureId(self.u32()?);
                let count = self.count(self.limits.max_values, 4)?;
                let mut captures = Vec::with_capacity(count);
                for _ in 0..count {
                    captures.push(ValueId(self.u32()?));
                }
                InstructionKind::MakeClosure { closure, captures }
            }
            7 => InstructionKind::ReadCapture { index: self.u32()? },
            8 => {
                let callee = ValueId(self.u32()?);
                let count = self.count(self.limits.max_values, 4)?;
                let mut arguments = Vec::with_capacity(count);
                for _ in 0..count {
                    arguments.push(ValueId(self.u32()?));
                }
                InstructionKind::CallClosure { callee, arguments }
            }
            22 => InstructionKind::WriteCapture {
                index: self.u32()?,
                value: ValueId(self.u32()?),
            },
            21 => {
                let callee = LocalId(self.u32()?);
                let count = self.count(self.limits.max_values, 4)?;
                let mut arguments = Vec::with_capacity(count);
                for _ in 0..count {
                    arguments.push(ValueId(self.u32()?));
                }
                InstructionKind::CallClosureMut { callee, arguments }
            }
            9 => InstructionKind::BorrowLocal {
                local: LocalId(self.u32()?),
            },
            10 => InstructionKind::ReadAddress {
                address: ValueId(self.u32()?),
            },
            27 => InstructionKind::BorrowLocalMut {
                local: LocalId(self.u32()?),
            },
            28 => InstructionKind::ReborrowAddress {
                address: ValueId(self.u32()?),
                mutable: self.boolean()?,
            },
            29 => InstructionKind::WriteAddress {
                address: ValueId(self.u32()?),
                value: ValueId(self.u32()?),
            },
            30 => InstructionKind::TakeLocal {
                local: LocalId(self.u32()?),
            },
            11 => InstructionKind::StorageLive(LocalId(self.u32()?)),
            12 => InstructionKind::StorageDead(LocalId(self.u32()?)),
            13 => {
                let count = self.count(self.limits.max_values, 4)?;
                let mut fields = Vec::with_capacity(count);
                for _ in 0..count {
                    fields.push(ValueId(self.u32()?));
                }
                InstructionKind::MakeAggregate { fields }
            }
            14 => InstructionKind::ExtractField {
                aggregate: ValueId(self.u32()?),
                index: self.u32()?,
            },
            15 => InstructionKind::UpdateField {
                aggregate: ValueId(self.u32()?),
                index: self.u32()?,
                value: ValueId(self.u32()?),
            },
            16 => InstructionKind::ExtractIndex {
                aggregate: ValueId(self.u32()?),
                index: ValueId(self.u32()?),
            },
            17 => InstructionKind::UpdateIndex {
                aggregate: ValueId(self.u32()?),
                index: ValueId(self.u32()?),
                value: ValueId(self.u32()?),
            },
            23 => InstructionKind::AddressField {
                address: ValueId(self.u32()?),
                index: self.u32()?,
            },
            24 => InstructionKind::AddressIndex {
                address: ValueId(self.u32()?),
                index: ValueId(self.u32()?),
            },
            61 => InstructionKind::AddressSlice {
                address: ValueId(self.u32()?),
                start: self.optional_value()?,
                end: self.optional_value()?,
            },
            62 => InstructionKind::SliceLength {
                address: ValueId(self.u32()?),
            },
            63 => InstructionKind::ByteBufferNew,
            64 => InstructionKind::ByteBufferReserve {
                buffer: ValueId(self.u32()?),
                capacity: ValueId(self.u32()?),
            },
            65 => InstructionKind::ByteBufferPush {
                buffer: ValueId(self.u32()?),
                value: ValueId(self.u32()?),
            },
            66 => InstructionKind::ByteBufferLen {
                buffer: ValueId(self.u32()?),
            },
            67 => InstructionKind::ByteBufferCapacity {
                buffer: ValueId(self.u32()?),
            },
            69 => InstructionKind::ByteBufferTruncate {
                buffer: ValueId(self.u32()?),
                length: ValueId(self.u32()?),
            },
            68 => InstructionKind::ByteBufferBorrow {
                buffer: ValueId(self.u32()?),
                mutable: self.boolean()?,
            },
            18 => {
                let variant = self.u32()?;
                let count = self.count(self.limits.max_values, 4)?;
                let mut fields = Vec::with_capacity(count);
                for _ in 0..count {
                    fields.push(ValueId(self.u32()?));
                }
                InstructionKind::MakeVariant { variant, fields }
            }
            19 => InstructionKind::VariantTag {
                value: ValueId(self.u32()?),
            },
            20 => InstructionKind::ReadVariant {
                value: ValueId(self.u32()?),
                variant: self.u32()?,
                index: self.u32()?,
            },
            60 => InstructionKind::ForeignFunctionRef {
                function: FunctionId(self.u32()?),
            },
            25 => InstructionKind::FunctionRef {
                function: FunctionId(self.u32()?),
            },
            26 => {
                let callee = ValueId(self.u32()?);
                let count = self.count(self.limits.max_values, 4)?;
                let mut arguments = Vec::with_capacity(count);
                for _ in 0..count {
                    arguments.push(ValueId(self.u32()?));
                }
                InstructionKind::CallIndirect { callee, arguments }
            }
            31 => InstructionKind::UnsafeEnter {
                region: self.u32()?,
            },
            32 => InstructionKind::UnsafeExit {
                region: self.u32()?,
            },
            33 => {
                let function = FunctionId(self.u32()?);
                let region = self.u32()?;
                let count = self.count(self.limits.max_values, 4)?;
                let mut arguments = Vec::with_capacity(count);
                for _ in 0..count {
                    arguments.push(ValueId(self.u32()?));
                }
                InstructionKind::ForeignCall {
                    function,
                    arguments,
                    region,
                }
            }
            34 => {
                let count = self.count(self.limits.max_values, 4)?;
                let mut fields = Vec::with_capacity(count);
                for _ in 0..count {
                    fields.push(ValueId(self.u32()?));
                }
                InstructionKind::MakeManaged { fields }
            }
            35 => InstructionKind::DescribeLocal {
                local: LocalId(self.u32()?),
            },
            36 => InstructionKind::ReadDesc {
                desc: ValueId(self.u32()?),
                index: self.u32()?,
            },
            37 => InstructionKind::WriteDesc {
                desc: ValueId(self.u32()?),
                index: self.u32()?,
                value: ValueId(self.u32()?),
            },
            38 => InstructionKind::DropManaged {
                local: LocalId(self.u32()?),
            },
            39 => InstructionKind::DescribeCapture { index: self.u32()? },
            40 => InstructionKind::TakeCapture { index: self.u32()? },
            78 => InstructionKind::BorrowCapture {
                index: self.u32()?,
                mutable: self.boolean()?,
            },
            41 => {
                let callee = LocalId(self.u32()?);
                let count = self.count(self.limits.max_values, 4)?;
                let mut arguments = Vec::with_capacity(count);
                for _ in 0..count {
                    arguments.push(ValueId(self.u32()?));
                }
                InstructionKind::CallClosureShared { callee, arguments }
            }
            42 => {
                let callee = ValueId(self.u32()?);
                let count = self.count(self.limits.max_values, 4)?;
                let mut arguments = Vec::with_capacity(count);
                for _ in 0..count {
                    arguments.push(ValueId(self.u32()?));
                }
                InstructionKind::CallClosureOnce { callee, arguments }
            }
            43 => InstructionKind::DropClosure {
                local: LocalId(self.u32()?),
            },
            44 => InstructionKind::ScopeEnter {
                scope: ScopeId(self.u32()?),
            },
            45 => InstructionKind::ScopeExit {
                scope: ScopeId(self.u32()?),
                kind: match self.u8()? {
                    0 => CleanupExitKind::Fallthrough,
                    1 => CleanupExitKind::Return,
                    2 => CleanupExitKind::Error,
                    3 => CleanupExitKind::Break,
                    4 => CleanupExitKind::Continue,
                    _ => return Err(malformed("E-WIRE-SCOPE-EXIT", "unknown cleanup exit kind")),
                },
            },
            46 => InstructionKind::MakeErrorUnion {
                is_error: self.boolean()?,
                value: ValueId(self.u32()?),
            },
            47 => InstructionKind::ErrorTag {
                value: ValueId(self.u32()?),
            },
            48 => InstructionKind::ReadErrorUnion {
                value: ValueId(self.u32()?),
                is_error: self.boolean()?,
            },
            70 => InstructionKind::TakeErrorUnion {
                value: ValueId(self.u32()?),
                is_error: self.boolean()?,
            },
            71 => InstructionKind::DropErrorUnion {
                local: LocalId(self.u32()?),
            },
            72 => {
                let count = self.count(self.limits.max_values, 4)?;
                let mut elements = Vec::with_capacity(count);
                for _ in 0..count {
                    elements.push(ValueId(self.u32()?));
                }
                InstructionKind::MakeOwnedArray { elements }
            }
            73 => InstructionKind::ProjectOwnedArrayField {
                local: LocalId(self.u32()?),
                index: self.u32()?,
                field: self.u32()?,
            },
            74 => InstructionKind::TakeOwnedArrayElement {
                local: LocalId(self.u32()?),
                index: self.u32()?,
            },
            75 => InstructionKind::DropOwnedArrayElement {
                local: LocalId(self.u32()?),
                index: self.u32()?,
            },
            76 => InstructionKind::StoreOwnedArrayElement {
                local: LocalId(self.u32()?),
                index: self.u32()?,
                value: ValueId(self.u32()?),
            },
            77 => InstructionKind::DropOwnedArray {
                local: LocalId(self.u32()?),
            },
            79 => InstructionKind::BorrowOwnedArrayElement {
                local: LocalId(self.u32()?),
                index: self.u32()?,
                mutable: self.boolean()?,
            },
            49 => {
                let count = self.count(self.limits.max_values, 4)?;
                let mut fields = Vec::with_capacity(count);
                for _ in 0..count {
                    fields.push(ValueId(self.u32()?));
                }
                InstructionKind::MakePackage { fields }
            }
            50 => InstructionKind::ReadPackageField {
                local: LocalId(self.u32()?),
                index: self.u32()?,
            },
            51 => InstructionKind::DescribePackageField {
                local: LocalId(self.u32()?),
                index: self.u32()?,
            },
            52 => InstructionKind::DropPackage {
                local: LocalId(self.u32()?),
            },
            53 => {
                let callee = ValueId(self.u32()?);
                let kind = self.callable_kind()?;
                let count = self.count(self.limits.max_values, 4)?;
                let mut arguments = Vec::with_capacity(count);
                for _ in 0..count {
                    arguments.push(ValueId(self.u32()?));
                }
                InstructionKind::CallClosureBorrowed {
                    callee,
                    kind,
                    arguments,
                }
            }
            54 => InstructionKind::TakePackageField {
                local: LocalId(self.u32()?),
                index: self.u32()?,
            },
            55 => InstructionKind::DropPackageField {
                local: LocalId(self.u32()?),
                index: self.u32()?,
            },
            56 => InstructionKind::StorePackageField {
                local: LocalId(self.u32()?),
                index: self.u32()?,
                value: ValueId(self.u32()?),
            },
            57 => InstructionKind::BindPackageCapture {
                local: LocalId(self.u32()?),
                index: self.u32()?,
            },
            58 => InstructionKind::EndPackageCapture {
                local: LocalId(self.u32()?),
                index: self.u32()?,
            },
            255 => InstructionKind::Unsupported(self.text()?),
            _ => return Err(malformed("E-WIRE-OPCODE", "unknown CFG instruction opcode")),
        };
        Ok(Instruction {
            result,
            kind,
            span: self.span()?,
        })
    }
    fn locals(&mut self) -> WireResult<Vec<LocalId>> {
        let count = self.count(self.limits.max_locals, 4)?;
        let mut locals = Vec::with_capacity(count);
        for _ in 0..count {
            locals.push(LocalId(self.u32()?));
        }
        Ok(locals)
    }
    fn plan(&mut self) -> WireResult<CandidatePlan> {
        let schema_version = self.u32()?;
        let module_fingerprint = self.fingerprint()?;
        let target_fingerprint = self.fingerprint()?;
        let count = self.count(self.limits.max_functions, 8)?;
        let mut functions = Vec::with_capacity(count);
        for _ in 0..count {
            let function = FunctionId(self.u32()?);
            let count = self.count(self.limits.max_blocks, 12)?;
            let mut blocks = Vec::with_capacity(count);
            for _ in 0..count {
                blocks.push(BlockPlan {
                    block: BlockId(self.u32()?),
                    initialized_in: self.locals()?,
                    initialized_out: self.locals()?,
                });
            }
            let count = self.count(self.limits.max_values, 8)?;
            let mut holds = Vec::with_capacity(count);
            for _ in 0..count {
                let value = ValueId(self.u32()?);
                let count = self.count(self.limits.max_nodes, 5)?;
                let mut roots = Vec::with_capacity(count);
                for _ in 0..count {
                    roots.push(match self.u8()? {
                        0 => HoldRoot::Local(LocalId(self.u32()?)),
                        1 => HoldRoot::Capture(self.u32()?),
                        2 => HoldRoot::Parameter(self.u32()?),
                        _ => return Err(malformed("E-WIRE-TAG", "unknown Hold root tag")),
                    });
                }
                holds.push(ValueHold { value, roots });
            }
            functions.push(FunctionPlan {
                function,
                blocks,
                holds,
            });
        }
        let count = self.count(self.limits.max_nodes, 9)?;
        let mut origins = Vec::with_capacity(count);
        for _ in 0..count {
            let origin = OriginId(self.u32()?);
            let source = NodeId(self.u32()?);
            let multiplicity = match self.u8()? {
                0 => OriginMultiplicity::One,
                1 => OriginMultiplicity::Many,
                _ => return Err(malformed("E-WIRE-TAG", "unknown origin multiplicity tag")),
            };
            origins.push(OriginClaim {
                origin,
                source,
                multiplicity,
            });
        }
        let count = self.count(self.limits.max_nodes, 13)?;
        let mut responsibilities = Vec::with_capacity(count);
        for _ in 0..count {
            let function = FunctionId(self.u32()?);
            let carrier = LocalId(self.u32()?);
            let origin = OriginId(self.u32()?);
            let state = match self.u8()? {
                0 => ResponsibilityState::Uninitialized,
                1 => ResponsibilityState::Live,
                2 => ResponsibilityState::Done,
                3 => ResponsibilityState::Moved,
                _ => return Err(malformed("E-WIRE-TAG", "unknown responsibility state tag")),
            };
            responsibilities.push(ResponsibilityClaim {
                function,
                carrier,
                origin,
                state,
            });
        }
        Ok(CandidatePlan {
            schema_version,
            module_fingerprint,
            target_fingerprint,
            functions,
            origins,
            responsibilities,
            unsupported: self.unsupported()?,
        })
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod owned_carrier_tests;

#[cfg(test)]
mod store_tests;

#[cfg(test)]
mod type_stack_tests;
