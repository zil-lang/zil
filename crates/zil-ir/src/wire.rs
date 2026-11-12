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

pub const WIRE_VERSION: u32 = 48;
#[path = "dynamic_wire.rs"]
mod dynamic;
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
fn flow_count(before: usize, amount: usize, maximum: usize) -> WireResult<usize> {
    before
        .checked_add(amount)
        .filter(|total| *total <= maximum)
        .ok_or_else(|| quota("candidate callable flow cumulative quota exceeded"))
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
                CtInstructionKind::ConvertNumber { value, target } => {
                    writer.u8(4)?;
                    writer.u32(value.0)?;
                    writer.ty(target, 0)?;
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
                4 => CtInstructionKind::ConvertNumber {
                    value: ValueId(reader.u32()?),
                    target: reader.ty(0)?,
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
    fn storage_step(&mut self, p: PlaceProjection) -> WireResult<()> {
        match p {
            PlaceProjection::Field(i) => {
                self.u8(0)?;
                self.u32(i)
            }
            PlaceProjection::Variant(v) => {
                self.u8(1)?;
                self.u32(v.variant)?;
                self.u32(v.index)
            }
            PlaceProjection::ErrorArm(e) => {
                self.u8(2)?;
                self.boolean(e)
            }
        }
    }
    fn storage_path(&mut self, path: &[PlaceProjection]) -> WireResult<()> {
        self.count(path.len(), self.limits.max_call_depth)?;
        for p in path {
            self.storage_step(*p)?;
        }
        Ok(())
    }
    fn life_type_path(&mut self, path: &[LifeTypeProjection]) -> WireResult<()> {
        self.count(path.len(), self.limits.max_call_depth)?;
        for step in path {
            match step {
                LifeTypeProjection::Place(p) => {
                    self.u8(0)?;
                    self.storage_step(*p)?;
                }
                LifeTypeProjection::TypeArgument(i) => {
                    self.u8(1)?;
                    self.u32(*i)?;
                }
                LifeTypeProjection::Pointee => self.u8(2)?,
                LifeTypeProjection::Element => self.u8(3)?,
                LifeTypeProjection::Parameter(i) => {
                    self.u8(4)?;
                    self.u32(*i)?;
                }
                LifeTypeProjection::Result => self.u8(5)?,
                LifeTypeProjection::Port(i) => {
                    self.u8(6)?;
                    self.u32(*i)?;
                }
                LifeTypeProjection::AssociatedType(i) => {
                    self.u8(7)?;
                    self.u32(*i)?;
                }
            }
        }
        Ok(())
    }
    pub(super) fn declaration_id(&mut self, id: &LifeDeclarationId) -> WireResult<()> {
        self.u32(id.module.0)?;
        self.u32(id.declaration.0)?;
        self.boolean(id.compiler)
    }
    pub(super) fn declaration_identity(&mut self, id: &LifeDeclarationIdentity) -> WireResult<()> {
        self.text(&id.package)?;
        for path in [&id.module_path, &id.declaration_path] {
            self.count(path.len(), self.limits.max_nodes)?;
            for part in path {
                self.text(part)?;
            }
        }
        self.count(id.generation_path.len(), self.limits.max_call_depth)?;
        for part in &id.generation_path {
            self.u32(*part)?;
        }
        Ok(())
    }
    pub(super) fn parameter_kind(
        &mut self,
        kind: &LifeParameterKind,
        depth: usize,
    ) -> WireResult<()> {
        match kind {
            LifeParameterKind::Type => self.u8(0),
            LifeParameterKind::Const(ty) => {
                self.u8(1)?;
                self.ty(ty, depth + 1)
            }
            LifeParameterKind::Lifetime => self.u8(2),
        }
    }
    pub(super) fn storage_binder(&mut self, binder: &StorageBinder) -> WireResult<()> {
        match binder {
            StorageBinder::Fixed(i) => {
                self.u8(0)?;
                self.u32(*i)
            }
            StorageBinder::Invocation(i) => {
                self.u8(1)?;
                self.u32(*i)
            }
        }
    }
    pub(super) fn storage_position(&mut self, position: &StoragePosition) -> WireResult<()> {
        match position.base {
            StorageBase::Parameter(i) => {
                self.u8(0)?;
                self.u32(i)?;
            }
            StorageBase::Capture(i) => {
                self.u8(1)?;
                self.u32(i)?;
            }
            StorageBase::Result => self.u8(2)?,
        }
        self.count(position.path.len(), self.limits.max_call_depth)?;
        for step in &position.path {
            match step {
                ContractProjection::Place(p) => {
                    self.u8(0)?;
                    self.storage_step(*p)?;
                }
                ContractProjection::RetainedPort(i) => {
                    self.u8(1)?;
                    self.u32(*i)?;
                }
                ContractProjection::RetainedClass(i) => {
                    self.u8(2)?;
                    self.u32(*i)?;
                }
                ContractProjection::Capture(i) => {
                    self.u8(3)?;
                    self.u32(*i)?;
                }
            }
        }
        Ok(())
    }
    fn life_maps(
        &mut self,
        retained: &[StorageRetainedBinding],
        nominal_arguments: &[NominalLifeArgument],
    ) -> WireResult<()> {
        self.count(retained.len(), self.limits.max_nodes)?;
        for binding in retained {
            self.life_type_path(&binding.path)?;
            self.u32(binding.class)?;
            self.u32(binding.binder)?;
        }
        self.count(nominal_arguments.len(), self.limits.max_nodes)?;
        for argument in nominal_arguments {
            self.life_type_path(&argument.path)?;
            self.declaration_id(&argument.declaration)?;
            self.u32(argument.argument)?;
            self.u32(argument.binder)?;
        }
        Ok(())
    }
    pub(super) fn retained_shape(
        &mut self,
        shape: &RetainedLifeShape,
        depth: usize,
    ) -> WireResult<()> {
        if depth > self.limits.max_call_depth {
            return Err(quota("retained life shape nesting exceeds wire limit"));
        }
        self.count(shape.binder_count as usize, self.limits.max_nodes)?;
        self.count(shape.ports.len(), self.limits.max_nodes)?;
        for port in &shape.ports {
            self.ty(&port.leaf, depth + 1)?;
            self.u8(port.mode as u8)?;
            self.u32(port.binder)?;
        }
        self.life_maps(&shape.retained, &shape.nominal_arguments)
    }
    fn storage_shape(&mut self, shape: &StorageLifeShape) -> WireResult<()> {
        self.count(shape.binder_count as usize, self.limits.max_nodes)?;
        self.count(shape.slots.len(), self.limits.max_nodes)?;
        for slot in &shape.slots {
            self.storage_path(&slot.path)?;
            self.u32(slot.binder)?;
            self.u8(slot.mode as u8)?;
        }
        self.life_maps(&shape.retained, &shape.nominal_arguments)
    }
    pub(super) fn required_storage_contract(
        &mut self,
        contract: &StorageLifeContract,
    ) -> WireResult<()> {
        self.count(contract.fixed_binder_count as usize, self.limits.max_nodes)?;
        self.count(
            contract.invocation_binder_count as usize,
            self.limits.max_nodes,
        )?;
        self.count(contract.bindings.len(), self.limits.max_nodes)?;
        for binding in &contract.bindings {
            self.storage_position(&binding.position)?;
            self.storage_binder(&binding.binder)?;
        }
        self.count(contract.classes.len(), self.limits.max_nodes)?;
        for binding in &contract.classes {
            match binding.position.base {
                StorageBase::Parameter(i) => {
                    self.u8(0)?;
                    self.u32(i)?;
                }
                StorageBase::Capture(i) => {
                    self.u8(1)?;
                    self.u32(i)?;
                }
                StorageBase::Result => self.u8(2)?,
            }
            self.life_type_path(&binding.position.path)?;
            self.u32(binding.position.class)?;
            self.storage_binder(&binding.binder)?;
        }
        Ok(())
    }
    fn storage_contract(&mut self, contract: &Option<StorageLifeContract>) -> WireResult<()> {
        self.boolean(contract.is_some())?;
        if let Some(contract) = contract {
            self.required_storage_contract(contract)?;
        }
        Ok(())
    }

    pub(super) fn retained_port_binding(
        &mut self,
        binding: &RetainedPortBinding,
    ) -> WireResult<()> {
        self.u32(binding.port)?;
        self.storage_position(&binding.source)
    }
    fn life_inventory(
        &mut self,
        declarations: &[LifeNominalDeclaration],
        instances: &[LifeNominalInstance],
    ) -> WireResult<()> {
        self.count(declarations.len(), self.limits.max_nodes)?;
        for declaration in declarations {
            self.declaration_id(&declaration.id)?;
            self.declaration_identity(&declaration.identity)?;
            self.u8(match declaration.kind {
                LifeNominalKind::Record => 0,
                LifeNominalKind::Package => 1,
                LifeNominalKind::Resource => 2,
                LifeNominalKind::Enum => 3,
            })?;
            self.count(declaration.parameters.len(), self.limits.max_nodes)?;
            for parameter in &declaration.parameters {
                self.parameter_kind(parameter, 0)?;
            }
        }
        self.count(instances.len(), self.limits.max_nodes)?;
        for instance in instances {
            self.declaration_id(&instance.declaration)?;
            self.ty(&instance.ty, 0)?;
            self.count(instance.arguments.len(), self.limits.max_nodes)?;
            for argument in &instance.arguments {
                match argument {
                    LifeNominalActual::Type(ty) => {
                        self.u8(0)?;
                        self.ty(ty, 0)?;
                    }
                    LifeNominalActual::Const(value) => {
                        self.u8(1)?;
                        self.constant(value)?;
                    }
                    LifeNominalActual::Lifetime(i) => {
                        self.u8(2)?;
                        self.u32(*i)?;
                    }
                }
            }
        }
        Ok(())
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
                Type::OwnedDyn { interface } => {
                    self.u8(40)?;
                    self.dyn_interface_id(interface)?;
                }
                Type::Dyn { interface } => {
                    self.u8(36)?;
                    self.dyn_interface_id(interface)?;
                }
                Type::BorrowedStorage { storage, life } => {
                    self.u8(35)?;
                    self.storage_shape(life)?;
                    pending.push(Task::Type(storage, depth + 1));
                }
                Type::Unit => self.u8(0)?,
                Type::Bool => self.u8(1)?,
                Type::I32 => self.u8(2)?,
                Type::I8 => self.u8(42)?,
                Type::I16 => self.u8(43)?,
                Type::Isize => self.u8(44)?,
                Type::I64 => self.u8(3)?,
                Type::U64 => self.u8(26)?,
                Type::Usize => self.u8(27)?,
                Type::F32 => self.u8(28)?,
                Type::F64 => self.u8(29)?,
                Type::U8 => self.u8(18)?,
                Type::U16 => self.u8(19)?,
                Type::U32 => self.u8(41)?,
                Type::ByteBuffer => self.u8(24)?,
                Type::Registration { environment } => {
                    self.u8(32)?;
                    pending.push(Task::Type(environment, depth + 1));
                }
                Type::AsyncRegistration { environment } => {
                    self.u8(37)?;
                    pending.push(Task::Type(environment, depth + 1));
                }
                Type::TaskI32 => self.u8(33)?,
                Type::Task {
                    result,
                    retained_life,
                } => {
                    self.u8(34)?;
                    self.retained_shape(retained_life, depth)?;
                    pending.push(Task::Type(result, depth + 1));
                }
                Type::ForeignHandle => self.u8(20)?,
                Type::SyncCell(kind) => {
                    self.u8(30)?;
                    self.u8(*kind as u8)?;
                }
                Type::Mutex { payload } => {
                    self.u8(38)?;
                    pending.push(Task::Type(payload, depth + 1));
                }
                Type::MutexGuard {
                    payload,
                    retained_life,
                } => {
                    self.u8(39)?;
                    self.retained_shape(retained_life, depth)?;
                    pending.push(Task::Type(payload, depth + 1));
                }
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
                Type::BorrowedFunctionPointer {
                    parameters,
                    result,
                    life,
                } => {
                    self.u8(31)?;
                    self.required_storage_contract(life)?;
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
            Constant::U32(value) => {
                self.u8(13)?;
                self.u32(*value)
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
            Constant::I8(value) => {
                self.u8(14)?;
                self.bytes(&value.to_le_bytes())
            }
            Constant::I16(value) => {
                self.u8(15)?;
                self.bytes(&value.to_le_bytes())
            }
            Constant::Isize(value) => {
                self.u8(16)?;
                self.bytes(&value.to_le_bytes())
            }
            Constant::I64(value) => {
                self.u8(3)?;
                self.bytes(&value.to_le_bytes())
            }
            Constant::U64(value) => {
                self.u8(9)?;
                self.bytes(&value.to_le_bytes())
            }
            Constant::Usize(value) => {
                self.u8(10)?;
                self.bytes(&value.to_le_bytes())
            }
            Constant::F32(bits) => {
                self.u8(11)?;
                self.bytes(&bits.to_le_bytes())
            }
            Constant::F64(bits) => {
                self.u8(12)?;
                self.bytes(&bits.to_le_bytes())
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
        self.life_inventory(&module.life_declarations, &module.life_instances)?;
        self.count(module.dyn_declarations.len(), self.limits.max_nodes)?;
        for row in &module.dyn_declarations {
            self.dyn_declaration(row)?;
        }
        self.count(module.dyn_interfaces.len(), self.limits.max_nodes)?;
        for row in &module.dyn_interfaces {
            self.dyn_interface(row)?;
        }
        self.count(module.dyn_vtables.len(), self.limits.max_nodes)?;
        for row in &module.dyn_vtables {
            self.dyn_vtable(row)?;
        }

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
            self.count(closure.lending_captures.len(), self.limits.max_locals)?;
            for index in &closure.lending_captures {
                self.u32(*index)?;
            }
            self.storage_contract(&closure.storage_life)?;
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
        self.count(c.roles.len(), self.limits.max_locals)?;
        let mut metadata = 0usize;
        for role in &c.roles {
            self.u32(role.parameter)?;
            self.u32(role.closure.0)?;
            self.u8(callable_kind_tag(role.kind))?;
            self.boolean(role.life.is_some())?;
            if let Some(life) = &role.life {
                self.required_storage_contract(life)?;
            }
            for indices in [&role.hold, &role.call, &role.writes] {
                metadata = flow_count(metadata, indices.len(), self.limits.max_analysis_steps)?;
                self.count(indices.len(), self.limits.max_locals)?;
                for index in indices {
                    self.u32(*index)?;
                }
            }
            self.u8(match role.cleanup {
                CallableCleanup::Owned => 0,
                CallableCleanup::Borrowed => 1,
                CallableCleanup::Returned => 2,
            })?;
        }
        match &c.traces {
            CallableTraces::Finite(traces) => {
                self.u8(0)?;
                self.count(traces.len(), self.limits.max_analysis_steps)?;
                for trace in traces {
                    self.callable_calls(&trace.calls)?;
                    self.callable_exit(trace.exit)?;
                }
            }
            CallableTraces::Flow(flow) => {
                self.u8(1)?;
                self.u32(flow.entry)?;
                self.count(flow.nodes.len(), self.limits.max_nodes)?;
                let (mut calls, mut edges) = (0usize, 0usize);
                for node in &flow.nodes {
                    calls = flow_count(calls, node.calls.len(), self.limits.max_instructions)?;
                    self.callable_calls(&node.calls)?;
                    match &node.next {
                        CallableFlowNext::Edges(next) => {
                            self.u8(0)?;
                            edges = flow_count(edges, next.len(), self.limits.max_analysis_steps)?;
                            self.count(next.len(), self.limits.max_analysis_steps)?;
                            for edge in next {
                                self.u32(*edge)?;
                            }
                        }
                        CallableFlowNext::Exit(exit) => {
                            self.u8(1)?;
                            self.callable_exit(*exit)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn callable_calls(&mut self, calls: &[CallEvent]) -> WireResult<()> {
        self.count(calls.len(), self.limits.max_instructions)?;
        for call in calls {
            self.u32(call.parameter)?;
            self.u8(callable_kind_tag(call.kind))?;
        }
        Ok(())
    }
    fn callable_exit(&mut self, exit: CallableExit) -> WireResult<()> {
        self.u8(match exit {
            CallableExit::Return => 0,
            CallableExit::Trap => 1,
        })
    }
    fn function(&mut self, function: &TypedFunction) -> WireResult<()> {
        self.boolean(function.retained_return_sources.is_some())?;
        if let Some(bindings) = &function.retained_return_sources {
            self.count(bindings.len(), self.limits.max_nodes)?;
            for binding in bindings {
                self.storage_position(&binding.position)?;
                self.count(binding.sources.len(), self.limits.max_nodes)?;
                for source in &binding.sources {
                    self.storage_position(source)?;
                }
            }
        }
        self.u8(function.call_kind as u8)?;
        self.u8(function.resume_bound as u8)?;
        self.storage_contract(&function.storage_life)?;
        self.boolean(function.reference_return_bound.is_some())?;
        if let Some(bound) = &function.reference_return_bound {
            self.count(bound.len(), self.limits.max_locals)?;
            for parameter in bound {
                self.u32(*parameter)?;
            }
        }
        self.boolean(function.callable_return_contract.is_some())?;
        if let Some(c) = &function.callable_return_contract {
            self.boolean(c.capture_sources.is_some())?;
            if let Some(bindings) = &c.capture_sources {
                self.count(bindings.len(), self.limits.max_nodes)?;
                for binding in bindings {
                    self.storage_position(&binding.position)?;
                    self.count(binding.sources.len(), self.limits.max_nodes)?;
                    for source in &binding.sources {
                        self.storage_position(source)?;
                    }
                }
            }

            match c.source {
                CallableReturnSource::Fresh => self.u8(0)?,
                CallableReturnSource::Parameter(parameter) => {
                    self.u8(1)?;
                    self.u32(parameter)?;
                }
            }
            self.u32(c.closure.0)?;
            self.u8(callable_kind_tag(c.kind))?;
            for indices in [&c.hold, &c.call, &c.writes, &c.cleanup, &c.lending_captures] {
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
        self.count(function.protocol_contracts.len(), self.limits.max_locals)?;
        for contract in &function.protocol_contracts {
            self.u8(match contract.protocol {
                ProtocolKind::AsyncRegistrationV1 => 0,
            })?;
            self.u32(contract.parameter)?;
            self.count(contract.entry.len(), self.limits.max_nodes)?;
            for state in &contract.entry {
                self.u8(protocol_state_tag(*state))?;
            }
            self.count(contract.exits.len(), self.limits.max_nodes)?;
            for row in &contract.exits {
                self.u8(protocol_state_tag(row.input))?;
                self.u8(protocol_exit_tag(row.exit))?;
                self.count(row.output.len(), self.limits.max_nodes)?;
                for state in &row.output {
                    self.u8(protocol_state_tag(*state))?;
                }
            }
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
            self.boolean(foreign.thread_driver_transfer)?;
            self.u8(scoped_callback_role_tag(foreign.scoped_callback))?;
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
            InstructionKind::StaticUtf8 { bytes } => {
                self.u8(139)?;
                self.count(bytes.len(), self.limits.max_source_bytes)?;
                self.bytes(bytes)?;
            }
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
            InstructionKind::OwnedStoreBorrow { store, index } => {
                self.u8(86)?;
                self.u32(store.0)?;
                self.u32(index.0)?;
            }
            InstructionKind::OwnedStoreBorrowMut { store, index } => {
                self.u8(88)?;
                self.u32(store.0)?;
                self.u32(index.0)?;
            }
            InstructionKind::BorrowPackageField {
                local,
                index,
                mutable,
            } => {
                self.u8(87)?;
                self.u32(local.0)?;
                self.u32(*index)?;
                self.boolean(*mutable)?;
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
            InstructionKind::MakeDynBorrow {
                interface,
                vtable,
                source,
                mutable,
            } => {
                self.u8(131)?;
                self.dyn_interface_id(interface)?;
                self.dyn_vtable_id(vtable)?;
                self.u32(source.0)?;
                self.boolean(*mutable)?;
            }
            InstructionKind::MakeDynOwned {
                interface,
                vtable,
                source,
                route,
            } => {
                self.u8(148)?;
                self.dyn_interface_id(interface)?;
                self.dyn_vtable_id(vtable)?;
                self.u32(source.0)?;
                self.u8(match route {
                    DynOwnedStorage::Heap => 1,
                })?;
            }
            InstructionKind::BorrowDynOwned { local, mutable } => {
                self.u8(149)?;
                self.u32(local.0)?;
                self.boolean(*mutable)?;
            }
            InstructionKind::DropDynOwned { local } => {
                self.u8(150)?;
                self.u32(local.0)?;
            }
            InstructionKind::CallDynSlot {
                callee,
                interface,
                slot,
                arguments,
            } => {
                self.u8(132)?;
                self.u32(callee.0)?;
                self.dyn_interface_id(interface)?;
                self.u32(*slot)?;
                self.count(arguments.len(), self.limits.max_values)?;
                for argument in arguments {
                    self.u32(argument.0)?;
                }
            }
            InstructionKind::LoadStoredBorrow {
                storage,
                path,
                mode,
            } => {
                self.u8(129)?;
                self.u32(storage.0)?;
                self.storage_path(path)?;
                match mode {
                    StoredBorrowLoad::CopyShared => self.u8(0)?,
                    StoredBorrowLoad::CopyDescription => self.u8(1)?,
                    StoredBorrowLoad::Reborrow { mutable } => {
                        self.u8(2)?;
                        self.boolean(*mutable)?;
                    }
                    StoredBorrowLoad::Take => self.u8(3)?,
                }
            }
            InstructionKind::StoreStoredBorrow {
                storage,
                path,
                value,
            } => {
                self.u8(130)?;
                self.u32(storage.0)?;
                self.storage_path(path)?;
                self.u32(value.0)?;
            }
            InstructionKind::DescribeAddress { address } => {
                self.u8(109)?;
                self.u32(address.0)?;
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
            InstructionKind::ConvertNumber { value, target } => {
                self.u8(98)?;
                self.u32(value.0)?;
                self.ty(target, 0)?;
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
            InstructionKind::WithForeignCallback {
                register,
                close,
                callback,
                body,
                region,
            } => {
                self.u8(94)?;
                self.u32(register.0)?;
                self.u32(close.0)?;
                self.u32(callback.0)?;
                self.u32(body.0)?;
                self.u32(*region)?;
            }
            InstructionKind::WithScopedThread {
                worker,
                argument,
                body,
            } => {
                self.u8(95)?;
                self.u32(worker.0)?;
                self.u32(argument.0)?;
                self.u32(body.0)?;
            }
            InstructionKind::WithSyncCell {
                kind,
                initial,
                body,
            } => {
                self.u8(96)?;
                self.u8(*kind as u8)?;
                self.u32(initial.0)?;
                self.u32(body.0)?;
            }
            InstructionKind::SyncI32 {
                op,
                address,
                value,
                ordering,
            } => {
                self.u8(97)?;
                self.u8(*op as u8)?;
                self.u32(address.0)?;
                self.boolean(value.is_some())?;
                if let Some(value) = value {
                    self.u32(value.0)?;
                }
                self.u8(*ordering as u8)?;
            }
            InstructionKind::SyncI32CompareExchange {
                address,
                expected,
                desired,
                success,
                failure,
            } => {
                self.u8(133)?;
                self.u32(address.0)?;
                self.u32(expected.0)?;
                self.u32(desired.0)?;
                self.u8(*success as u8)?;
                self.u8(*failure as u8)?;
            }
            InstructionKind::SyncInteger {
                op,
                address,
                value,
                ordering,
            } => {
                self.u8(146)?;
                self.u8(*op as u8)?;
                self.u32(address.0)?;
                self.boolean(value.is_some())?;
                if let Some(value) = value {
                    self.u32(value.0)?;
                }
                self.u8(*ordering as u8)?;
            }
            InstructionKind::SyncIntegerCompareExchange {
                address,
                expected,
                desired,
                success,
                failure,
            } => {
                self.u8(147)?;
                self.u32(address.0)?;
                self.u32(expected.0)?;
                self.u32(desired.0)?;
                self.u8(*success as u8)?;
                self.u8(*failure as u8)?;
            }
            InstructionKind::MutexCreate { value } => {
                self.u8(141)?;
                self.u32(value.0)?;
            }
            InstructionKind::MutexLock { mutex } => {
                self.u8(142)?;
                self.u32(mutex.0)?;
            }
            InstructionKind::MutexValue { guard } => {
                self.u8(143)?;
                self.u32(guard.0)?;
            }
            InstructionKind::MutexUnlock { guard } => {
                self.u8(144)?;
                self.u32(guard.0)?;
            }
            InstructionKind::DropMutexGuard { local } => {
                self.u8(145)?;
                self.u32(local.0)?;
            }
            InstructionKind::TaskStart { worker, argument } => {
                self.u8(99)?;
                self.u32(worker.0)?;
                self.u32(argument.0)?;
            }
            InstructionKind::SuspendCall {
                function,
                arguments,
            } => {
                self.u8(125)?;
                self.u32(function.0)?;
                self.count(arguments.len(), self.limits.max_values)?;
                for argument in arguments {
                    self.u32(argument.0)?;
                }
            }
            InstructionKind::TaskCreate {
                worker,
                arguments,
                port_bindings,
            }
            | InstructionKind::TaskCreateOwned {
                worker,
                arguments,
                port_bindings,
            } => {
                self.u8(
                    if matches!(instruction.kind, InstructionKind::TaskCreateOwned { .. }) {
                        151
                    } else {
                        128
                    },
                )?;
                self.u32(worker.0)?;
                self.count(arguments.len(), self.limits.max_values)?;
                for argument in arguments {
                    self.u32(argument.0)?;
                }
                self.count(port_bindings.len(), self.limits.max_values)?;
                for binding in port_bindings {
                    self.retained_port_binding(binding)?;
                }
            }
            InstructionKind::TaskYield => self.u8(100)?,
            InstructionKind::TaskValid { task } => {
                self.u8(101)?;
                self.u32(task.0)?;
            }
            InstructionKind::TaskPoll { task } => {
                self.u8(102)?;
                self.u32(task.0)?;
            }
            InstructionKind::TaskRequestCancel { task } => {
                self.u8(103)?;
                self.u32(task.0)?;
            }
            InstructionKind::TaskPollClose { task } => {
                self.u8(104)?;
                self.u32(task.0)?;
            }
            InstructionKind::TaskFinishDestroy { task } => {
                self.u8(105)?;
                self.u32(task.0)?;
            }
            InstructionKind::RegistrationStart {
                register,
                close,
                callback,
                region,
            } => {
                self.u8(111)?;
                self.u32(register.0)?;
                self.u32(close.0)?;
                self.u32(callback.0)?;
                self.u32(*region)?;
            }
            InstructionKind::RegistrationValid { registration } => {
                self.u8(112)?;
                self.u32(registration.0)?;
            }
            InstructionKind::RegistrationClose { registration } => {
                self.u8(113)?;
                self.u32(registration.0)?;
            }
            InstructionKind::AsyncRegistrationStart {
                register,
                request_close,
                poll_drain,
                callback,
                region,
            } => {
                self.u8(134)?;
                self.u32(register.0)?;
                self.u32(request_close.0)?;
                self.u32(poll_drain.0)?;
                self.u32(callback.0)?;
                self.u32(*region)?;
            }
            InstructionKind::AsyncRegistrationValid { registration } => {
                self.u8(135)?;
                self.u32(registration.0)?;
            }
            InstructionKind::AsyncRegistrationRequestClose { registration } => {
                self.u8(136)?;
                self.u32(registration.0)?;
            }
            InstructionKind::AsyncRegistrationPollClose { registration } => {
                self.u8(137)?;
                self.u32(registration.0)?;
            }
            InstructionKind::AsyncRegistrationFinishDestroy { registration } => {
                self.u8(138)?;
                self.u32(registration.0)?;
            }
            InstructionKind::AsyncRegistrationFinishDestroyChecked { registration } => {
                self.u8(140)?;
                self.u32(registration.0)?;
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
            InstructionKind::MakeOwnedTuple { fields } => {
                self.u8(114)?;
                self.count(fields.len(), self.limits.max_values)?;
                for field in fields {
                    self.u32(field.0)?;
                }
            }
            InstructionKind::UnpackTuple { local, fields } => {
                self.u8(115)?;
                self.u32(local.0)?;
                self.count(fields.len(), self.limits.max_locals)?;
                for field in fields {
                    self.u32(field.0)?;
                }
            }
            InstructionKind::DropTuple { local } => {
                self.u8(116)?;
                self.u32(local.0)?;
            }
            InstructionKind::TakeTupleField { local, path }
            | InstructionKind::TakeVariantField { local, path }
            | InstructionKind::BorrowPlaceLocal { local, path, .. }
            | InstructionKind::PlaceTagLocal { local, path } => {
                self.u8(match &instruction.kind {
                    InstructionKind::TakeTupleField { .. } => 117,
                    InstructionKind::TakeVariantField { .. } => 118,
                    InstructionKind::BorrowPlaceLocal { .. } => 119,
                    _ => 120,
                })?;
                self.u32(local.0)?;
                self.count(path.len(), self.limits.max_call_depth)?;
                for step in path {
                    match step {
                        PlaceProjection::Field(index) => {
                            self.u8(0)?;
                            self.u32(*index)?;
                        }
                        PlaceProjection::Variant(part) => {
                            self.u8(1)?;
                            self.u32(part.variant)?;
                            self.u32(part.index)?;
                        }
                        PlaceProjection::ErrorArm(is_error) => {
                            self.u8(2)?;
                            self.boolean(*is_error)?;
                        }
                    }
                }
                if let InstructionKind::BorrowPlaceLocal { mutable, .. } = &instruction.kind {
                    self.boolean(*mutable)?;
                }
            }

            InstructionKind::BorrowPlaceAddress { address, path, .. }
            | InstructionKind::PlaceTagAddress { address, path } => {
                self.u8(
                    if matches!(
                        &instruction.kind,
                        InstructionKind::BorrowPlaceAddress { .. }
                    ) {
                        126
                    } else {
                        127
                    },
                )?;
                self.u32(address.0)?;
                self.count(path.len(), self.limits.max_call_depth)?;
                for step in path {
                    match step {
                        PlaceProjection::Field(index) => {
                            self.u8(0)?;
                            self.u32(*index)?;
                        }
                        PlaceProjection::Variant(part) => {
                            self.u8(1)?;
                            self.u32(part.variant)?;
                            self.u32(part.index)?;
                        }
                        PlaceProjection::ErrorArm(is_error) => {
                            self.u8(2)?;
                            self.boolean(*is_error)?;
                        }
                    }
                }
                if let InstructionKind::BorrowPlaceAddress { mutable, .. } = &instruction.kind {
                    self.boolean(*mutable)?;
                }
            }

            InstructionKind::ReservePlaceLocal {
                reservation,
                local,
                path,
            } => {
                self.u8(121)?;
                self.u32(reservation.0)?;
                self.u32(local.0)?;
                self.count(path.len(), self.limits.max_call_depth)?;
                for step in path {
                    match step {
                        PlaceProjection::Field(index) => {
                            self.u8(0)?;
                            self.u32(*index)?;
                        }
                        PlaceProjection::Variant(part) => {
                            self.u8(1)?;
                            self.u32(part.variant)?;
                            self.u32(part.index)?;
                        }
                        PlaceProjection::ErrorArm(is_error) => {
                            self.u8(2)?;
                            self.boolean(*is_error)?;
                        }
                    }
                }
            }
            InstructionKind::ReservePlaceAddress {
                reservation,
                address,
                path,
            } => {
                self.u8(122)?;
                self.u32(reservation.0)?;
                self.u32(address.0)?;
                self.count(path.len(), self.limits.max_call_depth)?;
                for step in path {
                    match step {
                        PlaceProjection::Field(index) => {
                            self.u8(0)?;
                            self.u32(*index)?;
                        }
                        PlaceProjection::Variant(part) => {
                            self.u8(1)?;
                            self.u32(part.variant)?;
                            self.u32(part.index)?;
                        }
                        PlaceProjection::ErrorArm(is_error) => {
                            self.u8(2)?;
                            self.boolean(*is_error)?;
                        }
                    }
                }
            }
            InstructionKind::CommitPlaceReplace { reservation, value } => {
                self.u8(123)?;
                self.u32(reservation.0)?;
                self.u32(value.0)?;
            }
            InstructionKind::AbortPlaceReservation { reservation } => {
                self.u8(124)?;
                self.u32(reservation.0)?;
            }
            InstructionKind::BorrowFieldLocal {
                local,
                path,
                mutable,
            } => {
                self.u8(110)?;
                self.u32(local.0)?;
                self.count(path.len(), self.limits.max_call_depth)?;
                for field in path {
                    self.u32(*field)?;
                }
                self.boolean(*mutable)?;
            }
            InstructionKind::VariantTagLocal { local, path }
            | InstructionKind::ReadVariantLocal { local, path } => {
                self.u8(
                    if matches!(&instruction.kind, InstructionKind::VariantTagLocal { .. }) {
                        90
                    } else {
                        91
                    },
                )?;
                self.u32(local.0)?;
                self.count(path.len(), self.limits.max_call_depth)?;
                for step in path {
                    self.u32(step.variant)?;
                    self.u32(step.index)?;
                }
            }
            InstructionKind::BorrowVariantLocal {
                local,
                path,
                mutable,
            } => {
                self.u8(106)?;
                self.u32(local.0)?;
                self.count(path.len(), self.limits.max_call_depth)?;
                for step in path {
                    self.u32(step.variant)?;
                    self.u32(step.index)?;
                }
                self.boolean(*mutable)?;
            }
            InstructionKind::VariantTagAddress { address, path }
            | InstructionKind::BorrowVariantAddress { address, path, .. } => {
                self.u8(
                    if matches!(&instruction.kind, InstructionKind::VariantTagAddress { .. }) {
                        107
                    } else {
                        108
                    },
                )?;
                self.u32(address.0)?;
                self.count(path.len(), self.limits.max_call_depth)?;
                for step in path {
                    self.u32(step.variant)?;
                    self.u32(step.index)?;
                }
                if let InstructionKind::BorrowVariantAddress { mutable, .. } = &instruction.kind {
                    self.boolean(*mutable)?;
                }
            }
            InstructionKind::UnpackVariant {
                local,
                variant,
                fields,
            } => {
                self.u8(92)?;
                self.u32(local.0)?;
                self.u32(*variant)?;
                self.count(fields.len(), self.limits.max_locals)?;
                for field in fields {
                    self.u32(field.0)?;
                }
            }
            InstructionKind::DropEnum { local } => {
                self.u8(93)?;
                self.u32(local.0)?;
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
            InstructionKind::RegisterDefer { scope, local } => {
                self.u8(89)?;
                self.u32(scope.0)?;
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
                        HoldRoot::Static => self.u8(3)?,
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
    fn storage_step(&mut self) -> WireResult<PlaceProjection> {
        Ok(match self.u8()? {
            0 => PlaceProjection::Field(self.u32()?),
            1 => PlaceProjection::Variant(VariantProjection {
                variant: self.u32()?,
                index: self.u32()?,
            }),
            2 => PlaceProjection::ErrorArm(self.boolean()?),
            _ => return Err(malformed("E-WIRE-TAG", "unknown stored place projection")),
        })
    }
    fn storage_path(&mut self) -> WireResult<Vec<PlaceProjection>> {
        let n = self.count(self.limits.max_call_depth, 2)?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(self.storage_step()?);
        }
        Ok(out)
    }
    fn life_type_path(&mut self) -> WireResult<Vec<LifeTypeProjection>> {
        let n = self.count(self.limits.max_call_depth, 1)?;
        let mut path = Vec::with_capacity(n);
        for _ in 0..n {
            path.push(match self.u8()? {
                0 => LifeTypeProjection::Place(self.storage_step()?),
                1 => LifeTypeProjection::TypeArgument(self.u32()?),
                2 => LifeTypeProjection::Pointee,
                3 => LifeTypeProjection::Element,
                4 => LifeTypeProjection::Parameter(self.u32()?),
                5 => LifeTypeProjection::Result,
                6 => LifeTypeProjection::Port(self.u32()?),
                7 => LifeTypeProjection::AssociatedType(self.u32()?),
                _ => return Err(malformed("E-WIRE-TAG", "unknown life type projection")),
            });
        }
        Ok(path)
    }
    pub(super) fn declaration_id(&mut self) -> WireResult<LifeDeclarationId> {
        Ok(LifeDeclarationId {
            module: ModuleId(self.u32()?),
            declaration: NodeId(self.u32()?),
            compiler: self.boolean()?,
        })
    }
    pub(super) fn declaration_identity(&mut self) -> WireResult<LifeDeclarationIdentity> {
        let package = self.text()?;
        let mut paths = Vec::with_capacity(2);
        for _ in 0..2 {
            let n = self.count(self.limits.max_nodes, 4)?;
            let mut path = Vec::with_capacity(n);
            for _ in 0..n {
                path.push(self.text()?);
            }
            paths.push(path);
        }
        let n = self.count(self.limits.max_call_depth, 4)?;
        let mut generation_path = Vec::with_capacity(n);
        for _ in 0..n {
            generation_path.push(self.u32()?);
        }
        let declaration_path = paths.pop().expect("two declaration paths");
        let module_path = paths.pop().expect("two declaration paths");
        Ok(LifeDeclarationIdentity {
            package,
            module_path,
            declaration_path,
            generation_path,
        })
    }
    pub(super) fn parameter_kind(&mut self, depth: usize) -> WireResult<LifeParameterKind> {
        Ok(match self.u8()? {
            0 => LifeParameterKind::Type,
            1 => LifeParameterKind::Const(self.ty(depth + 1)?),
            2 => LifeParameterKind::Lifetime,
            _ => return Err(malformed("E-WIRE-TAG", "unknown nominal parameter kind")),
        })
    }
    pub(super) fn storage_binder(&mut self) -> WireResult<StorageBinder> {
        Ok(match self.u8()? {
            0 => StorageBinder::Fixed(self.u32()?),
            1 => StorageBinder::Invocation(self.u32()?),
            _ => {
                return Err(malformed(
                    "E-WIRE-TAG",
                    "unknown storage life binder domain",
                ));
            }
        })
    }
    pub(super) fn storage_position(&mut self) -> WireResult<StoragePosition> {
        let base = match self.u8()? {
            0 => StorageBase::Parameter(self.u32()?),
            1 => StorageBase::Capture(self.u32()?),
            2 => StorageBase::Result,
            _ => return Err(malformed("E-WIRE-TAG", "unknown storage contract base")),
        };
        let n = self.count(self.limits.max_call_depth, 3)?;
        let mut path = Vec::with_capacity(n);
        for _ in 0..n {
            path.push(match self.u8()? {
                0 => ContractProjection::Place(self.storage_step()?),
                1 => ContractProjection::RetainedPort(self.u32()?),
                2 => ContractProjection::RetainedClass(self.u32()?),
                3 => ContractProjection::Capture(self.u32()?),
                _ => return Err(malformed("E-WIRE-TAG", "unknown contract projection")),
            });
        }
        Ok(StoragePosition { base, path })
    }
    fn stored_borrow_mode(&mut self) -> WireResult<StoredBorrowMode> {
        Ok(match self.u8()? {
            0 => StoredBorrowMode::SharedAddress,
            1 => StoredBorrowMode::ExclusiveAddress,
            2 => StoredBorrowMode::ReadOnlyDescription,
            3 => StoredBorrowMode::WritableDescription,
            _ => return Err(malformed("E-WIRE-TAG", "unknown stored borrow mode")),
        })
    }
    fn life_maps(&mut self) -> WireResult<(Vec<StorageRetainedBinding>, Vec<NominalLifeArgument>)> {
        let n = self.count(self.limits.max_nodes, 12)?;
        let mut retained = Vec::with_capacity(n);
        for _ in 0..n {
            retained.push(StorageRetainedBinding {
                path: self.life_type_path()?,
                class: self.u32()?,
                binder: self.u32()?,
            });
        }
        let n = self.count(self.limits.max_nodes, 21)?;
        let mut nominal_arguments = Vec::with_capacity(n);
        for _ in 0..n {
            nominal_arguments.push(NominalLifeArgument {
                path: self.life_type_path()?,
                declaration: self.declaration_id()?,
                argument: self.u32()?,
                binder: self.u32()?,
            });
        }
        Ok((retained, nominal_arguments))
    }
    pub(super) fn retained_shape(&mut self, depth: usize) -> WireResult<RetainedLifeShape> {
        if depth > self.limits.max_call_depth {
            return Err(quota("retained life shape nesting exceeds wire limit"));
        }
        let binder_count = self.count(self.limits.max_nodes, 0)? as u32;
        let n = self.count(self.limits.max_nodes, 6)?;
        let mut ports = Vec::with_capacity(n);
        for _ in 0..n {
            ports.push(RetainedBorrowPort {
                leaf: self.ty(depth + 1)?,
                mode: self.stored_borrow_mode()?,
                binder: self.u32()?,
            });
        }
        let (retained, nominal_arguments) = self.life_maps()?;
        Ok(RetainedLifeShape {
            binder_count,
            ports,
            retained,
            nominal_arguments,
        })
    }
    fn storage_shape(&mut self) -> WireResult<StorageLifeShape> {
        let binder_count = self.count(self.limits.max_nodes, 0)? as u32;
        let n = self.count(self.limits.max_nodes, 9)?;
        let mut slots = Vec::with_capacity(n);
        for _ in 0..n {
            slots.push(StorageLifeSlot {
                path: self.storage_path()?,
                binder: self.u32()?,
                mode: self.stored_borrow_mode()?,
            });
        }
        let (retained, nominal_arguments) = self.life_maps()?;
        Ok(StorageLifeShape {
            binder_count,
            slots,
            retained,
            nominal_arguments,
        })
    }
    pub(super) fn required_storage_contract(&mut self) -> WireResult<StorageLifeContract> {
        let fixed_binder_count = self.count(self.limits.max_nodes, 0)? as u32;
        let invocation_binder_count = self.count(self.limits.max_nodes, 0)? as u32;
        let n = self.count(self.limits.max_nodes, 10)?;
        let mut bindings = Vec::with_capacity(n);
        for _ in 0..n {
            bindings.push(StorageLifeBinding {
                position: self.storage_position()?,
                binder: self.storage_binder()?,
            });
        }
        let n = self.count(self.limits.max_nodes, 14)?;
        let mut classes = Vec::with_capacity(n);
        for _ in 0..n {
            let base = match self.u8()? {
                0 => StorageBase::Parameter(self.u32()?),
                1 => StorageBase::Capture(self.u32()?),
                2 => StorageBase::Result,
                _ => return Err(malformed("E-WIRE-TAG", "unknown semantic class base")),
            };
            classes.push(SignatureLifeClassBinding {
                position: SignatureLifeClass {
                    base,
                    path: self.life_type_path()?,
                    class: self.u32()?,
                },
                binder: self.storage_binder()?,
            });
        }
        Ok(StorageLifeContract {
            fixed_binder_count,
            invocation_binder_count,
            bindings,
            classes,
        })
    }
    fn storage_contract(&mut self) -> WireResult<Option<StorageLifeContract>> {
        if self.boolean()? {
            self.required_storage_contract().map(Some)
        } else {
            Ok(None)
        }
    }

    pub(super) fn retained_port_binding(&mut self) -> WireResult<RetainedPortBinding> {
        Ok(RetainedPortBinding {
            port: self.u32()?,
            source: self.storage_position()?,
        })
    }
    fn life_inventory(
        &mut self,
    ) -> WireResult<(Vec<LifeNominalDeclaration>, Vec<LifeNominalInstance>)> {
        let n = self.count(self.limits.max_nodes, 30)?;
        let mut declarations = Vec::with_capacity(n);
        for _ in 0..n {
            let id = self.declaration_id()?;
            let identity = self.declaration_identity()?;
            let kind = match self.u8()? {
                0 => LifeNominalKind::Record,
                1 => LifeNominalKind::Package,
                2 => LifeNominalKind::Resource,
                3 => LifeNominalKind::Enum,
                _ => return Err(malformed("E-WIRE-TAG", "unknown nominal declaration kind")),
            };
            let n = self.count(self.limits.max_nodes, 1)?;
            let mut parameters = Vec::with_capacity(n);
            for _ in 0..n {
                parameters.push(self.parameter_kind(0)?);
            }
            declarations.push(LifeNominalDeclaration {
                id,
                identity,
                kind,
                parameters,
            });
        }
        let n = self.count(self.limits.max_nodes, 14)?;
        let mut instances = Vec::with_capacity(n);
        for _ in 0..n {
            let declaration = self.declaration_id()?;
            let ty = self.ty(0)?;
            let n = self.count(self.limits.max_nodes, 1)?;
            let mut arguments = Vec::with_capacity(n);
            for _ in 0..n {
                arguments.push(match self.u8()? {
                    0 => LifeNominalActual::Type(self.ty(0)?),
                    1 => LifeNominalActual::Const(self.constant()?),
                    2 => LifeNominalActual::Lifetime(self.u32()?),
                    _ => {
                        return Err(malformed(
                            "E-WIRE-TAG",
                            "unknown nominal actual argument kind",
                        ));
                    }
                });
            }
            instances.push(LifeNominalInstance {
                declaration,
                ty,
                arguments,
            });
        }
        Ok((declarations, instances))
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
            BorrowedFunction {
                start: usize,
                life: StorageLifeContract,
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
            BorrowedStorage(StorageLifeShape),
            RetainedFrame(RetainedLifeShape),
            RetainedGuard(RetainedLifeShape),
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
                Task::BorrowedFunction { start, life } => {
                    let result = Box::new(values.pop().expect("read borrowed function result"));
                    let parameters = values.split_off(start);
                    values.push(Type::BorrowedFunctionPointer {
                        parameters,
                        result,
                        life,
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
                Task::BorrowedStorage(life) => {
                    let storage = Box::new(values.pop().expect("read storage type"));
                    values.push(Type::BorrowedStorage { storage, life });
                    continue;
                }
                Task::RetainedFrame(retained_life) => {
                    let result = Box::new(values.pop().expect("read task result type"));
                    values.push(Type::Task {
                        result,
                        retained_life,
                    });
                    continue;
                }
                Task::RetainedGuard(retained_life) => {
                    let payload = Box::new(values.pop().expect("read guard payload type"));
                    values.push(Type::MutexGuard {
                        payload,
                        retained_life,
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
                        32 => Type::Registration { environment: inner },
                        37 => Type::AsyncRegistration { environment: inner },
                        38 => Type::Mutex { payload: inner },
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
                35 => {
                    let life = self.storage_shape()?;
                    pending.push(Task::BorrowedStorage(life));
                    pending.push(Task::Type(depth + 1));
                    continue;
                }
                0 => Type::Unit,
                1 => Type::Bool,
                2 => Type::I32,
                3 => Type::I64,
                36 => Type::Dyn {
                    interface: self.dyn_interface_id()?,
                },
                40 => Type::OwnedDyn {
                    interface: self.dyn_interface_id()?,
                },
                26 => Type::U64,
                27 => Type::Usize,
                28 => Type::F32,
                29 => Type::F64,
                18 => Type::U8,
                19 => Type::U16,
                41 => Type::U32,
                42 => Type::I8,
                43 => Type::I16,
                44 => Type::Isize,

                33 => Type::TaskI32,
                34 => {
                    let retained_life = self.retained_shape(depth)?;
                    pending.push(Task::RetainedFrame(retained_life));
                    pending.push(Task::Type(depth + 1));
                    continue;
                }
                39 => {
                    let retained_life = self.retained_shape(depth)?;
                    pending.push(Task::RetainedGuard(retained_life));
                    pending.push(Task::Type(depth + 1));
                    continue;
                }
                20 => Type::ForeignHandle,
                30 => Type::SyncCell(match self.u8()? {
                    0 => SyncCellKind::MutexI32,
                    1 => SyncCellKind::AtomicI32,
                    2 => SyncCellKind::AtomicI64,
                    3 => SyncCellKind::AtomicU64,
                    4 => SyncCellKind::AtomicUsize,
                    _ => return Err(malformed("E-WIRE-TYPE", "unknown sealed cell kind")),
                }),
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
                31 => {
                    let life = self.required_storage_contract()?;
                    let count = self.count(self.limits.max_locals, 1)?;
                    pending.push(Task::BorrowedFunction {
                        start: values.len(),
                        life,
                    });
                    pending.push(Task::Type(depth + 1));
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
                7 | 8 | 23 | 25 | 32 | 37 | 38 => {
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
            13 => Constant::U32(u32::from_le_bytes(
                self.take(4)?.try_into().expect("checked length"),
            )),
            14 => Constant::I8(i8::from_le_bytes(
                self.take(1)?.try_into().expect("checked length"),
            )),
            15 => Constant::I16(i16::from_le_bytes(
                self.take(2)?.try_into().expect("checked length"),
            )),
            16 => Constant::Isize(i64::from_le_bytes(
                self.take(8)?.try_into().expect("checked length"),
            )),

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
            11 => Constant::F32(u32::from_le_bytes(
                self.take(4)?.try_into().expect("four float32 bytes"),
            )),
            12 => Constant::F64(u64::from_le_bytes(
                self.take(8)?.try_into().expect("eight float64 bytes"),
            )),
            3 => Constant::I64(i64::from_le_bytes(
                self.take(8)?.try_into().expect("checked length"),
            )),
            9 => Constant::U64(u64::from_le_bytes(
                self.take(8)?.try_into().expect("checked length"),
            )),
            10 => Constant::Usize(u64::from_le_bytes(
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
        let (life_declarations, life_instances) = self.life_inventory()?;
        let n = self.count(self.limits.max_nodes, 1)?;
        let mut dyn_declarations = Vec::with_capacity(n);
        for _ in 0..n {
            dyn_declarations.push(self.dyn_declaration()?);
        }
        let n = self.count(self.limits.max_nodes, 1)?;
        let mut dyn_interfaces = Vec::with_capacity(n);
        for _ in 0..n {
            dyn_interfaces.push(self.dyn_interface()?);
        }
        let n = self.count(self.limits.max_nodes, 1)?;
        let mut dyn_vtables = Vec::with_capacity(n);
        for _ in 0..n {
            dyn_vtables.push(self.dyn_vtable()?);
        }

        let count = self.count(self.limits.max_functions, 4)?;
        let mut functions = Vec::with_capacity(count);
        for _ in 0..count {
            functions.push(self.function()?);
        }
        let count = self.count(self.limits.max_functions, 22)?;
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
                lending_captures: self.callable_indices()?,
                storage_life: self.storage_contract()?,
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
            dyn_declarations,
            dyn_interfaces,
            dyn_vtables,
            life_declarations,
            life_instances,
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
    fn callable_exit(&mut self) -> WireResult<CallableExit> {
        match self.u8()? {
            0 => Ok(CallableExit::Return),
            1 => Ok(CallableExit::Trap),
            _ => Err(malformed("E-WIRE-TAG", "unknown callable exit")),
        }
    }
    fn callable_calls(&mut self, total: &mut usize) -> WireResult<Vec<CallEvent>> {
        let count = self.count(self.limits.max_instructions, 5)?;
        *total = flow_count(*total, count, self.limits.max_instructions)?;
        let mut calls = Vec::with_capacity(count);
        for _ in 0..count {
            calls.push(CallEvent {
                parameter: self.u32()?,
                kind: self.callable_kind()?,
            });
        }
        Ok(calls)
    }
    fn callable_contract(&mut self) -> WireResult<CallableParameterContract> {
        let count = self.count(self.limits.max_locals, 23)?;
        let mut roles = Vec::with_capacity(count);
        let mut metadata = 0usize;
        for _ in 0..count {
            let parameter = self.u32()?;
            let closure = ClosureId(self.u32()?);
            let kind = self.callable_kind()?;
            let life = if self.boolean()? {
                Some(self.required_storage_contract()?)
            } else {
                None
            };
            let mut fields = Vec::with_capacity(3);
            for _ in 0..3 {
                let count = self.count(self.limits.max_locals, 4)?;
                metadata = flow_count(metadata, count, self.limits.max_analysis_steps)?;
                let mut indices = Vec::with_capacity(count);
                for _ in 0..count {
                    indices.push(self.u32()?);
                }
                fields.push(indices);
            }
            let [hold, call, writes]: [Vec<u32>; 3] =
                fields.try_into().expect("three envelope fields");
            let cleanup = match self.u8()? {
                0 => CallableCleanup::Owned,
                1 => CallableCleanup::Borrowed,
                2 => CallableCleanup::Returned,
                _ => return Err(malformed("E-WIRE-TAG", "unknown callable cleanup")),
            };
            roles.push(CallableRole {
                life,
                parameter,
                closure,
                kind,
                hold,
                call,
                writes,
                cleanup,
            });
        }
        let traces = match self.u8()? {
            0 => {
                let count = self.count(self.limits.max_analysis_steps, 5)?;
                let mut traces = Vec::with_capacity(count);
                for _ in 0..count {
                    let mut total = 0;
                    let calls = self.callable_calls(&mut total)?;
                    let exit = self.callable_exit()?;
                    traces.push(CallableTrace { calls, exit });
                }
                CallableTraces::Finite(traces)
            }
            1 => {
                let entry = self.u32()?;
                let count = self.count(self.limits.max_nodes, 6)?;
                let mut nodes = Vec::with_capacity(count);
                let (mut call_count, mut edge_count) = (0usize, 0usize);
                for _ in 0..count {
                    let calls = self.callable_calls(&mut call_count)?;
                    let next = match self.u8()? {
                        0 => {
                            let count = self.count(self.limits.max_analysis_steps, 4)?;
                            edge_count =
                                flow_count(edge_count, count, self.limits.max_analysis_steps)?;
                            let mut edges = Vec::with_capacity(count);
                            for _ in 0..count {
                                edges.push(self.u32()?);
                            }
                            CallableFlowNext::Edges(edges)
                        }
                        1 => CallableFlowNext::Exit(self.callable_exit()?),
                        _ => {
                            return Err(malformed("E-WIRE-TAG", "unknown callable flow successor"));
                        }
                    };
                    nodes.push(CallableFlowNode { calls, next });
                }
                CallableTraces::Flow(CallableFlow { entry, nodes })
            }
            _ => {
                return Err(malformed(
                    "E-WIRE-TAG",
                    "unknown callable traces representation",
                ));
            }
        };
        Ok(CallableParameterContract { roles, traces })
    }

    fn protocol_state(&mut self) -> WireResult<ProtocolState> {
        match self.u8()? {
            0 => Ok(ProtocolState::Open),
            1 => Ok(ProtocolState::Closing),
            2 => Ok(ProtocolState::Closed),
            _ => Err(malformed("E-WIRE-TAG", "unknown protocol state")),
        }
    }
    fn function(&mut self) -> WireResult<TypedFunction> {
        let retained_return_sources = if self.boolean()? {
            let count = self.count(self.limits.max_nodes, 9)?;
            let mut bindings = Vec::with_capacity(count);
            for _ in 0..count {
                let position = self.storage_position()?;
                let count = self.count(self.limits.max_nodes, 5)?;
                let mut sources = Vec::with_capacity(count);
                for _ in 0..count {
                    sources.push(self.storage_position()?);
                }
                bindings.push(StorageSourceBinding { position, sources });
            }
            Some(bindings)
        } else {
            None
        };
        let call_kind = match self.u8()? {
            0 => CallKind::Direct,
            1 => CallKind::Suspend,
            _ => return Err(malformed("E-WIRE-TAG", "unknown function call kind")),
        };
        let resume_bound = match self.u8()? {
            0 => TaskResumeBound::Local,
            1 => TaskResumeBound::Transfer,
            _ => return Err(malformed("E-WIRE-TAG", "unknown Task resume bound")),
        };
        let storage_life = self.storage_contract()?;
        let reference_return_bound = if self.boolean()? {
            Some(self.callable_indices()?)
        } else {
            None
        };
        let callable_return_contract = if self.boolean()? {
            Some(CallableReturnContract {
                capture_sources: if self.boolean()? {
                    let count = self.count(self.limits.max_nodes, 9)?;
                    let mut bindings = Vec::with_capacity(count);
                    for _ in 0..count {
                        let position = self.storage_position()?;
                        let count = self.count(self.limits.max_nodes, 5)?;
                        let mut sources = Vec::with_capacity(count);
                        for _ in 0..count {
                            sources.push(self.storage_position()?);
                        }
                        bindings.push(StorageSourceBinding { position, sources });
                    }
                    Some(bindings)
                } else {
                    None
                },

                source: match self.u8()? {
                    0 => CallableReturnSource::Fresh,
                    1 => CallableReturnSource::Parameter(self.u32()?),
                    _ => return Err(malformed("E-WIRE-TAG", "unknown callable return source")),
                },
                closure: ClosureId(self.u32()?),
                kind: self.callable_kind()?,
                hold: self.callable_indices()?,
                call: self.callable_indices()?,
                writes: self.callable_indices()?,
                cleanup: self.callable_indices()?,
                lending_captures: self.callable_indices()?,
            })
        } else {
            None
        };
        let callable_contract = if self.boolean()? {
            Some(self.callable_contract()?)
        } else {
            None
        };
        // Even a semantically invalid empty role occupies kind/parameter and
        // both vector headers (13 bytes). Bound the complete row count before
        // allocating; validation independently requires nonempty real states.
        let contract_count = self.count(self.limits.max_locals, 13)?;
        let mut protocol_contracts = Vec::with_capacity(contract_count);
        for _ in 0..contract_count {
            let protocol = match self.u8()? {
                0 => ProtocolKind::AsyncRegistrationV1,
                _ => return Err(malformed("E-WIRE-TAG", "unknown protocol kind")),
            };
            let parameter = self.u32()?;
            let count = self.count(self.limits.max_nodes, 1)?;
            let mut entry = Vec::with_capacity(count);
            for _ in 0..count {
                entry.push(self.protocol_state()?);
            }
            let count = self.count(self.limits.max_nodes, 6)?;
            let mut exits = Vec::with_capacity(count);
            for _ in 0..count {
                let input = self.protocol_state()?;
                let exit = match self.u8()? {
                    0 => ProtocolExit::Return,
                    1 => ProtocolExit::ErrorOk,
                    2 => ProtocolExit::ErrorErr,
                    3 => ProtocolExit::PollPending,
                    4 => ProtocolExit::PollReady,
                    _ => return Err(malformed("E-WIRE-TAG", "unknown protocol exit")),
                };
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut output = Vec::with_capacity(count);
                for _ in 0..count {
                    output.push(self.protocol_state()?);
                }
                exits.push(ProtocolTransition {
                    input,
                    exit,
                    output,
                });
            }
            protocol_contracts.push(FunctionProtocolContract {
                protocol,
                parameter,
                entry,
                exits,
            });
        }
        let id = FunctionId(self.u32()?);
        let name = self.text()?;
        let foreign = if self.boolean()? {
            Some(ForeignDeclaration {
                abi: self.text()?,
                symbol: self.text()?,
                noescape_parameters: self.callable_indices()?,
                static_callbacks: self.boolean()?,
                thread_driver_transfer: self.boolean()?,
                scoped_callback: match self.u8()? {
                    0 => None,
                    1 => Some(ScopedCallbackRole::Register),
                    2 => Some(ScopedCallbackRole::Close),
                    3 => Some(ScopedCallbackRole::AsyncRegister),
                    4 => Some(ScopedCallbackRole::AsyncRequestClose),
                    5 => Some(ScopedCallbackRole::AsyncPollDrain),
                    6 => Some(ScopedCallbackRole::ThreadAsyncRegister),
                    7 => Some(ScopedCallbackRole::ThreadAsyncRequestClose),
                    8 => Some(ScopedCallbackRole::ThreadAsyncPollDrain),
                    _ => return Err(malformed("E-WIRE-TAG", "unknown scoped callback role")),
                },
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
            retained_return_sources,
            storage_life,
            call_kind,
            resume_bound,
            reference_return_bound,
            callable_return_contract,
            callable_contract,
            protocol_contracts,
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
    fn atomic_ordering(&mut self) -> WireResult<AtomicOrdering> {
        match self.u8()? {
            0 => Ok(AtomicOrdering::Relaxed),
            1 => Ok(AtomicOrdering::Acquire),
            2 => Ok(AtomicOrdering::Release),
            3 => Ok(AtomicOrdering::AcqRel),
            4 => Ok(AtomicOrdering::SeqCst),
            _ => Err(malformed("E-WIRE-OPCODE", "unknown atomic ordering")),
        }
    }
    fn instruction(&mut self) -> WireResult<Instruction> {
        let result = self.optional_value()?;
        let kind = match self.u8()? {
            139 => {
                let count = self.count(self.limits.max_source_bytes, 1)?;
                InstructionKind::StaticUtf8 {
                    bytes: self.take(count)?.to_vec(),
                }
            }
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
            86 => InstructionKind::OwnedStoreBorrow {
                store: ValueId(self.u32()?),
                index: ValueId(self.u32()?),
            },
            87 => InstructionKind::BorrowPackageField {
                local: LocalId(self.u32()?),
                index: self.u32()?,
                mutable: self.boolean()?,
            },
            88 => InstructionKind::OwnedStoreBorrowMut {
                store: ValueId(self.u32()?),
                index: ValueId(self.u32()?),
            },
            0 => InstructionKind::Constant(self.constant()?),
            1 => InstructionKind::Load(LocalId(self.u32()?)),
            2 => InstructionKind::Store {
                local: LocalId(self.u32()?),
                value: ValueId(self.u32()?),
            },
            98 => InstructionKind::ConvertNumber {
                value: ValueId(self.u32()?),
                target: self.ty(0)?,
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
            114 => {
                let count = self.count(self.limits.max_values, 4)?;
                let mut fields = Vec::with_capacity(count);
                for _ in 0..count {
                    fields.push(ValueId(self.u32()?));
                }
                InstructionKind::MakeOwnedTuple { fields }
            }
            115 => {
                let local = LocalId(self.u32()?);
                let count = self.count(self.limits.max_locals, 4)?;
                let mut fields = Vec::with_capacity(count);
                for _ in 0..count {
                    fields.push(LocalId(self.u32()?));
                }
                InstructionKind::UnpackTuple { local, fields }
            }
            116 => InstructionKind::DropTuple {
                local: LocalId(self.u32()?),
            },
            tag @ 117..=120 => {
                let local = LocalId(self.u32()?);
                // ErrorArm uses two bytes; all longer steps debit through the
                // same primitive reads before extending the bounded vector.
                let count = self.count(self.limits.max_call_depth, 2)?;
                let mut path = Vec::with_capacity(count);
                for _ in 0..count {
                    path.push(match self.u8()? {
                        0 => PlaceProjection::Field(self.u32()?),
                        1 => PlaceProjection::Variant(VariantProjection {
                            variant: self.u32()?,
                            index: self.u32()?,
                        }),
                        2 => PlaceProjection::ErrorArm(self.boolean()?),
                        _ => {
                            return Err(malformed(
                                "E-WIRE-TAG",
                                "unknown inline place projection tag",
                            ));
                        }
                    });
                }
                match tag {
                    117 => InstructionKind::TakeTupleField { local, path },
                    118 => InstructionKind::TakeVariantField { local, path },
                    119 => InstructionKind::BorrowPlaceLocal {
                        local,
                        path,
                        mutable: self.boolean()?,
                    },
                    _ => InstructionKind::PlaceTagLocal { local, path },
                }
            }
            tag @ 126..=127 => {
                let address = ValueId(self.u32()?);
                // ErrorArm uses two bytes; all longer steps debit through the
                // same primitive reads before extending the bounded vector.
                let count = self.count(self.limits.max_call_depth, 2)?;
                let mut path = Vec::with_capacity(count);
                for _ in 0..count {
                    path.push(match self.u8()? {
                        0 => PlaceProjection::Field(self.u32()?),
                        1 => PlaceProjection::Variant(VariantProjection {
                            variant: self.u32()?,
                            index: self.u32()?,
                        }),
                        2 => PlaceProjection::ErrorArm(self.boolean()?),
                        _ => {
                            return Err(malformed(
                                "E-WIRE-TAG",
                                "unknown inline place projection tag",
                            ));
                        }
                    });
                }
                if tag == 126 {
                    InstructionKind::BorrowPlaceAddress {
                        address,
                        path,
                        mutable: self.boolean()?,
                    }
                } else {
                    InstructionKind::PlaceTagAddress { address, path }
                }
            }
            tag @ 121..=122 => {
                let reservation = ReservationId(self.u32()?);
                let base = self.u32()?;
                // ErrorArm uses two bytes; all longer steps debit through the
                // same primitive reads before extending the bounded vector.
                let count = self.count(self.limits.max_call_depth, 2)?;
                let mut path = Vec::with_capacity(count);
                for _ in 0..count {
                    path.push(match self.u8()? {
                        0 => PlaceProjection::Field(self.u32()?),
                        1 => PlaceProjection::Variant(VariantProjection {
                            variant: self.u32()?,
                            index: self.u32()?,
                        }),
                        2 => PlaceProjection::ErrorArm(self.boolean()?),
                        _ => {
                            return Err(malformed(
                                "E-WIRE-TAG",
                                "unknown inline place projection tag",
                            ));
                        }
                    });
                }
                if tag == 121 {
                    InstructionKind::ReservePlaceLocal {
                        reservation,
                        local: LocalId(base),
                        path,
                    }
                } else {
                    InstructionKind::ReservePlaceAddress {
                        reservation,
                        address: ValueId(base),
                        path,
                    }
                }
            }
            123 => InstructionKind::CommitPlaceReplace {
                reservation: ReservationId(self.u32()?),
                value: ValueId(self.u32()?),
            },
            124 => InstructionKind::AbortPlaceReservation {
                reservation: ReservationId(self.u32()?),
            },
            110 => {
                let local = LocalId(self.u32()?);
                let count = self.count(self.limits.max_call_depth, 4)?;
                let mut path = Vec::with_capacity(count);
                for _ in 0..count {
                    path.push(self.u32()?);
                }
                InstructionKind::BorrowFieldLocal {
                    local,
                    path,
                    mutable: self.boolean()?,
                }
            }
            tag @ (107 | 108) => {
                let address = ValueId(self.u32()?);
                let count = self.count(self.limits.max_call_depth, 8)?;
                let mut path = Vec::with_capacity(count);
                for _ in 0..count {
                    path.push(VariantProjection {
                        variant: self.u32()?,
                        index: self.u32()?,
                    });
                }
                if tag == 107 {
                    InstructionKind::VariantTagAddress { address, path }
                } else {
                    InstructionKind::BorrowVariantAddress {
                        address,
                        path,
                        mutable: self.boolean()?,
                    }
                }
            }
            106 => {
                let local = LocalId(self.u32()?);
                let count = self.count(self.limits.max_call_depth, 8)?;
                let mut path = Vec::with_capacity(count);
                for _ in 0..count {
                    path.push(VariantProjection {
                        variant: self.u32()?,
                        index: self.u32()?,
                    });
                }
                InstructionKind::BorrowVariantLocal {
                    local,
                    path,
                    mutable: self.boolean()?,
                }
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
            129 => {
                let storage = ValueId(self.u32()?);
                let path = self.storage_path()?;
                let mode = match self.u8()? {
                    0 => StoredBorrowLoad::CopyShared,
                    1 => StoredBorrowLoad::CopyDescription,
                    2 => StoredBorrowLoad::Reborrow {
                        mutable: self.boolean()?,
                    },
                    3 => StoredBorrowLoad::Take,
                    _ => return Err(malformed("E-WIRE-TAG", "unknown stored load mode")),
                };
                InstructionKind::LoadStoredBorrow {
                    storage,
                    path,
                    mode,
                }
            }
            131 => InstructionKind::MakeDynBorrow {
                interface: self.dyn_interface_id()?,
                vtable: self.dyn_vtable_id()?,
                source: ValueId(self.u32()?),
                mutable: self.boolean()?,
            },
            148 => InstructionKind::MakeDynOwned {
                interface: self.dyn_interface_id()?,
                vtable: self.dyn_vtable_id()?,
                source: ValueId(self.u32()?),
                route: match self.u8()? {
                    1 => DynOwnedStorage::Heap,
                    _ => return Err(malformed("E-WIRE-TAG", "unknown owned dyn storage route")),
                },
            },
            149 => InstructionKind::BorrowDynOwned {
                local: LocalId(self.u32()?),
                mutable: self.boolean()?,
            },
            150 => InstructionKind::DropDynOwned {
                local: LocalId(self.u32()?),
            },
            132 => {
                let callee = ValueId(self.u32()?);
                let interface = self.dyn_interface_id()?;
                let slot = self.u32()?;
                let count = self.count(self.limits.max_values, 4)?;
                let mut arguments = Vec::with_capacity(count);
                for _ in 0..count {
                    arguments.push(ValueId(self.u32()?));
                }
                InstructionKind::CallDynSlot {
                    callee,
                    interface,
                    slot,
                    arguments,
                }
            }
            130 => InstructionKind::StoreStoredBorrow {
                storage: ValueId(self.u32()?),
                path: self.storage_path()?,
                value: ValueId(self.u32()?),
            },
            109 => InstructionKind::DescribeAddress {
                address: ValueId(self.u32()?),
            },
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
            opcode @ (90 | 91) => {
                let local = LocalId(self.u32()?);
                let count = self.count(self.limits.max_call_depth, 8)?;
                let mut path = Vec::with_capacity(count);
                for _ in 0..count {
                    path.push(VariantProjection {
                        variant: self.u32()?,
                        index: self.u32()?,
                    });
                }
                if opcode == 90 {
                    InstructionKind::VariantTagLocal { local, path }
                } else {
                    InstructionKind::ReadVariantLocal { local, path }
                }
            }
            92 => {
                let local = LocalId(self.u32()?);
                let variant = self.u32()?;
                let count = self.count(self.limits.max_locals, 4)?;
                let mut fields = Vec::with_capacity(count);
                for _ in 0..count {
                    fields.push(LocalId(self.u32()?));
                }
                InstructionKind::UnpackVariant {
                    local,
                    variant,
                    fields,
                }
            }
            93 => InstructionKind::DropEnum {
                local: LocalId(self.u32()?),
            },
            89 => InstructionKind::RegisterDefer {
                scope: ScopeId(self.u32()?),
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
            94 => InstructionKind::WithForeignCallback {
                register: FunctionId(self.u32()?),
                close: FunctionId(self.u32()?),
                callback: ValueId(self.u32()?),
                body: ValueId(self.u32()?),
                region: self.u32()?,
            },
            95 => InstructionKind::WithScopedThread {
                worker: FunctionId(self.u32()?),
                argument: ValueId(self.u32()?),
                body: ValueId(self.u32()?),
            },
            96 => InstructionKind::WithSyncCell {
                kind: match self.u8()? {
                    0 => SyncCellKind::MutexI32,
                    1 => SyncCellKind::AtomicI32,
                    2 => SyncCellKind::AtomicI64,
                    3 => SyncCellKind::AtomicU64,
                    4 => SyncCellKind::AtomicUsize,
                    _ => return Err(malformed("E-WIRE-OPCODE", "unknown sealed cell kind")),
                },
                initial: ValueId(self.u32()?),
                body: ValueId(self.u32()?),
            },
            97 => InstructionKind::SyncI32 {
                op: match self.u8()? {
                    0 => SyncI32Op::Load,
                    1 => SyncI32Op::Store,
                    2 => SyncI32Op::FetchAdd,
                    _ => return Err(malformed("E-WIRE-OPCODE", "unknown cell operation")),
                },
                address: ValueId(self.u32()?),
                value: if self.boolean()? {
                    Some(ValueId(self.u32()?))
                } else {
                    None
                },
                ordering: match self.u8()? {
                    0 => AtomicOrdering::Relaxed,
                    1 => AtomicOrdering::Acquire,
                    2 => AtomicOrdering::Release,
                    3 => AtomicOrdering::AcqRel,
                    4 => AtomicOrdering::SeqCst,
                    _ => return Err(malformed("E-WIRE-OPCODE", "unknown atomic ordering")),
                },
            },
            133 => InstructionKind::SyncI32CompareExchange {
                address: ValueId(self.u32()?),
                expected: ValueId(self.u32()?),
                desired: ValueId(self.u32()?),
                success: self.atomic_ordering()?,
                failure: self.atomic_ordering()?,
            },
            146 => InstructionKind::SyncInteger {
                op: match self.u8()? {
                    0 => SyncIntegerOp::Load,
                    1 => SyncIntegerOp::Store,
                    2 => SyncIntegerOp::FetchAdd,
                    _ => {
                        return Err(malformed(
                            "E-WIRE-OPCODE",
                            "unknown integer atomic operation",
                        ));
                    }
                },
                address: ValueId(self.u32()?),
                value: if self.boolean()? {
                    Some(ValueId(self.u32()?))
                } else {
                    None
                },
                ordering: self.atomic_ordering()?,
            },
            147 => InstructionKind::SyncIntegerCompareExchange {
                address: ValueId(self.u32()?),
                expected: ValueId(self.u32()?),
                desired: ValueId(self.u32()?),
                success: self.atomic_ordering()?,
                failure: self.atomic_ordering()?,
            },
            141 => InstructionKind::MutexCreate {
                value: ValueId(self.u32()?),
            },
            142 => InstructionKind::MutexLock {
                mutex: ValueId(self.u32()?),
            },
            143 => InstructionKind::MutexValue {
                guard: ValueId(self.u32()?),
            },
            144 => InstructionKind::MutexUnlock {
                guard: ValueId(self.u32()?),
            },
            145 => InstructionKind::DropMutexGuard {
                local: LocalId(self.u32()?),
            },
            99 => InstructionKind::TaskStart {
                worker: FunctionId(self.u32()?),
                argument: ValueId(self.u32()?),
            },
            125 => {
                let function = FunctionId(self.u32()?);
                let count = self.count(self.limits.max_values, 4)?;
                let mut arguments = Vec::with_capacity(count);
                for _ in 0..count {
                    arguments.push(ValueId(self.u32()?));
                }
                InstructionKind::SuspendCall {
                    function,
                    arguments,
                }
            }
            tag @ (128 | 151) => {
                let worker = FunctionId(self.u32()?);
                let count = self.count(self.limits.max_values, 4)?;
                let mut arguments = Vec::with_capacity(count);
                for _ in 0..count {
                    arguments.push(ValueId(self.u32()?));
                }
                let count = self.count(self.limits.max_values, 4)?;
                let mut port_bindings = Vec::with_capacity(count);
                for _ in 0..count {
                    port_bindings.push(self.retained_port_binding()?);
                }
                if tag == 151 {
                    InstructionKind::TaskCreateOwned {
                        worker,
                        arguments,
                        port_bindings,
                    }
                } else {
                    InstructionKind::TaskCreate {
                        worker,
                        arguments,
                        port_bindings,
                    }
                }
            }
            100 => InstructionKind::TaskYield,
            101 => InstructionKind::TaskValid {
                task: ValueId(self.u32()?),
            },
            102 => InstructionKind::TaskPoll {
                task: ValueId(self.u32()?),
            },
            103 => InstructionKind::TaskRequestCancel {
                task: ValueId(self.u32()?),
            },
            104 => InstructionKind::TaskPollClose {
                task: ValueId(self.u32()?),
            },
            105 => InstructionKind::TaskFinishDestroy {
                task: ValueId(self.u32()?),
            },
            111 => InstructionKind::RegistrationStart {
                register: FunctionId(self.u32()?),
                close: FunctionId(self.u32()?),
                callback: ValueId(self.u32()?),
                region: self.u32()?,
            },
            112 => InstructionKind::RegistrationValid {
                registration: ValueId(self.u32()?),
            },
            113 => InstructionKind::RegistrationClose {
                registration: ValueId(self.u32()?),
            },
            134 => InstructionKind::AsyncRegistrationStart {
                register: FunctionId(self.u32()?),
                request_close: FunctionId(self.u32()?),
                poll_drain: FunctionId(self.u32()?),
                callback: ValueId(self.u32()?),
                region: self.u32()?,
            },
            135 => InstructionKind::AsyncRegistrationValid {
                registration: ValueId(self.u32()?),
            },
            136 => InstructionKind::AsyncRegistrationRequestClose {
                registration: ValueId(self.u32()?),
            },
            137 => InstructionKind::AsyncRegistrationPollClose {
                registration: ValueId(self.u32()?),
            },
            138 => InstructionKind::AsyncRegistrationFinishDestroy {
                registration: ValueId(self.u32()?),
            },
            140 => InstructionKind::AsyncRegistrationFinishDestroyChecked {
                registration: ValueId(self.u32()?),
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
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut roots = Vec::with_capacity(count);
                for _ in 0..count {
                    roots.push(match self.u8()? {
                        0 => HoldRoot::Local(LocalId(self.u32()?)),
                        1 => HoldRoot::Capture(self.u32()?),
                        2 => HoldRoot::Parameter(self.u32()?),
                        3 => HoldRoot::Static,
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

#[cfg(test)]
mod state_field_tests;

#[cfg(test)]
mod flow_tests;

#[cfg(test)]
mod owned_sum_tests;

#[cfg(test)]
mod retained_tests;
