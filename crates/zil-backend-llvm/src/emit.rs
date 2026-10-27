mod arrays;
mod byte_buffer;
mod enums;
mod errors;
mod foreign;
mod packages;
mod resource_closures;
mod resources;
mod slices;
use std::collections::{BTreeMap, BTreeSet};

use zil_base::{CompileResult, Diagnostic, Phase, SCHEMA_VERSION, Span, TargetProfile};
use zil_ir::{
    BinaryOp, CallableKind, ClosureDefinition, ClosureId, Constant, FunctionId, Instruction,
    InstructionKind, LirModule, LocalId, ResourceDefinition, ScalarLayout, Terminator, Type,
    TypedFunction, UnaryOp, ValueId,
};
use zil_verify::VerifiedModule;

use crate::{EmitOptions, EntryPoint};

/// Emit a module without an executable entry wrapper. Use `emit_with_options`
/// when an executable or the full-return-value harness is required.
pub fn emit(verified: &VerifiedModule, lir: &LirModule) -> CompileResult<String> {
    emit_with_options(verified, lir, &EmitOptions::default())
}

pub fn emit_with_options(
    verified: &VerifiedModule,
    lir: &LirModule,
    options: &EmitOptions,
) -> CompileResult<String> {
    bind_lir(verified, lir)?;
    let functions = lir
        .functions
        .iter()
        .map(|function| (function.id, function))
        .collect::<BTreeMap<_, _>>();
    let closures = lir
        .closures
        .iter()
        .map(|closure| (closure.id, closure))
        .collect::<BTreeMap<_, _>>();
    let wrapper = entry_wrapper(&functions, options.entry)?;
    let windows = lir.target.triple == "x86_64-pc-windows-msvc";
    let mut intrinsics = BTreeSet::new();
    let mut body = String::new();
    for function in &lir.functions {
        if function.foreign.is_some() {
            if matches!(options.entry, EntryPoint::ReturnValue(_))
                && function
                    .foreign
                    .as_ref()
                    .is_some_and(|f| f.symbol == "printf")
            {
                return invalid(
                    "E-CODEGEN-FOREIGN",
                    "foreign printf conflicts with the optional return-value harness",
                    Some(function.span),
                );
            }
            body.push_str(&foreign::declaration(function, &lir.target)?);
            continue;
        }
        body.push_str(
            &FunctionEmitter {
                function,
                target: &lir.target,
                functions: &functions,
                closures: &closures,
                resources: &lir.resources,
                intrinsics: &mut intrinsics,
                text: String::new(),
                temporary: 0,
                continuation: 0,
                partial_packages: BTreeSet::new(),
            }
            .emit()?,
        );
    }
    let mut text = format!(
        "; ZIL verified LLVM module\n; verified-fingerprint: {}\n\
         target triple = \"{}\"\n\n",
        verified.fingerprint().to_hex(),
        lir.target.triple,
    );
    for closure in &lir.closures {
        let mut fields = closure
            .captures
            .iter()
            .map(capture_type)
            .collect::<CompileResult<Vec<_>>>()?;
        fields.extend(
            closure
                .captures
                .iter()
                .flat_map(|ty| std::iter::repeat_n("i1".into(), resource_closures::flag_count(ty))),
        );
        let fields = fields.join(", ");
        text.push_str(&format!(
            "{} = type {{ {fields} }}\n",
            environment_type(closure.id)
        ));
    }
    text.push_str(&resource_closures::drop_functions(
        &lir.closures,
        &lir.resources,
    )?);
    for function in &lir.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                if let InstructionKind::Constant(Constant::WideString(units)) = &instruction.kind {
                    let id = result(instruction)?;
                    let data = units
                        .iter()
                        .map(|unit| format!("i16 {unit}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    text.push_str(&format!("@zil.wstr.{}.{} = private unnamed_addr constant [{} x i16] [{data}], align 2\n", function.id.0, id.0, units.len()));
                }
            }
        }
    }
    text.push_str("declare void @llvm.trap()\n");
    text.push_str(&byte_buffer::runtime(lir)?);
    for (operation, bits) in intrinsics {
        text.push_str(&format!(
            "declare {{ i{bits}, i1 }} @llvm.{operation}.with.overflow.i{bits}(i{bits}, i{bits})\n"
        ));
    }
    if matches!(options.entry, EntryPoint::ReturnValue(_)) {
        text.push_str(
            "@zil.return.format = private unnamed_addr constant [6 x i8] c\"%lld\\0A\\00\"\n",
        );
        text.push_str("declare i32 @printf(ptr, ...)\n");
    }
    text.push('\n');
    text.push_str(&body);
    text.push_str(&wrapper);
    if windows && options.entry != EntryPoint::None {
        if let Some(existing) = lir.functions.iter().find(|f| {
            f.foreign
                .as_ref()
                .is_some_and(|x| x.symbol == "ExitProcess")
        }) {
            if existing.return_type != Type::Unit
                || existing.parameters.len() != 1
                || existing.locals[existing.parameters[0].0 as usize].ty != Type::I32
            {
                return invalid(
                    "E-CODEGEN-FOREIGN",
                    "ExitProcess conflicts with the Win64 entry ABI",
                    Some(existing.span),
                );
            }
        } else {
            text.push_str("declare void @ExitProcess(i32)\n");
        }
        text.push_str("define void @zil.pe.entry() {\nentry:\n  %code = call i32 @main()\n  call void @ExitProcess(i32 %code)\n  unreachable\n}\n");
    }
    Ok(text)
}

fn bind_lir(verified: &VerifiedModule, lir: &LirModule) -> CompileResult<()> {
    let module = verified.module();
    let layouts = vec![
        ScalarLayout {
            ty: Type::Unit,
            size: 0,
            alignment: 1,
        },
        ScalarLayout {
            ty: Type::Bool,
            size: 1,
            alignment: 1,
        },
        ScalarLayout {
            ty: Type::I32,
            size: 4,
            alignment: 4,
        },
        ScalarLayout {
            ty: Type::I64,
            size: 8,
            alignment: 8,
        },
        ScalarLayout {
            ty: Type::U8,
            size: 1,
            alignment: 1,
        },
        ScalarLayout {
            ty: Type::U16,
            size: 2,
            alignment: 2,
        },
        ScalarLayout {
            ty: Type::ForeignHandle,
            size: 8,
            alignment: 8,
        },
        ScalarLayout {
            ty: Type::CWideString,
            size: 8,
            alignment: 8,
        },
    ];
    // LIR is publicly constructible. A copied fingerprint alone is insufficient:
    // the actual executable CFG, concrete environments, target and layout must
    // match this capability.
    if lir.schema_version != SCHEMA_VERSION
        || lir.schema_version != module.schema_version
        || lir.module_id != module.module_id
        || lir.verified_fingerprint != verified.fingerprint()
        || lir.target != module.target
        || lir.functions != module.functions
        || lir.closures != module.closures
        || lir.resources != module.resources
        || lir.layouts != layouts
    {
        return invalid(
            "E-CODEGEN-UNBOUND-LIR",
            "LIR does not exactly match the supplied verified module, environments and scalar layout",
            None,
        );
    }
    if !lir.target.is_supported_x64_llvm() {
        return unsupported(
            "LLVM code generation supports checked x86_64 Linux and Win64 targets",
            None,
        );
    }
    for closure in &lir.closures {
        if !matches!(
            closure.kind,
            CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
        ) || !closure.result.is_copy_value()
            || !closure.captures.iter().all(|ty| {
                ty.is_copy_value()
                    || matches!(ty, Type::Closure(_))
                    || matches!(ty, Type::Address { inner, .. } if inner.is_copy_value() || matches!(inner.as_ref(), Type::Closure(_)))
                    || (resources::supported_type(ty, &lir.resources)
                        && !matches!(ty, Type::Desc(p) if p.as_ref() == &Type::ByteBuffer))
                    || packages::supported_type(ty, &lir.resources)
            })
            || !closure.parameters.iter().all(Type::is_copy_value)
        {
            return unsupported("LLVM finite resource closure representation", None);
        }
    }
    for function in &lir.functions {
        if !(function.return_type.is_copy_value()
            || function.return_type.is_scalar_or_address()
            || resources::supported_type(&function.return_type, &lir.resources)
            || packages::supported_type(&function.return_type, &lir.resources)
            || arrays::supported_type(&function.return_type, &lir.resources)
            || (matches!(function.return_type, Type::Closure(_))
                && function.callable_return_contract.is_some()))
            || function.parameters.iter().any(|id| {
                let ty = &function.locals[id.0 as usize].ty;
                !(ty.is_copy_value()
                    || ty.is_scalar_or_address()
                    || resources::supported_type(ty, &lir.resources)
                    || packages::supported_type(ty, &lir.resources)
                    || arrays::supported_type(ty, &lir.resources)
                    || callable_parameter_type(ty, &lir.closures))
            })
        {
            return unsupported("LLVM scalar function signature", Some(function.span));
        }
        for ty in function
            .locals
            .iter()
            .map(|local| &local.ty)
            .chain(function.values.iter())
        {
            let concrete_closure = callable_parameter_type(ty, &lir.closures);
            if !ty.is_copy_value()
                && !concrete_closure
                && !matches!(ty, Type::Address { inner, .. } if inner.is_address_pointee())
                && !resources::supported_type(ty, &lir.resources)
                && !packages::supported_type(ty, &lir.resources)
                && !arrays::supported_type(ty, &lir.resources)
            {
                return unsupported(
                    format!("LLVM representation for {}", ty.name()),
                    Some(function.span),
                );
            }
        }
    }
    Ok(())
}

fn entry_wrapper(
    functions: &BTreeMap<FunctionId, &TypedFunction>,
    entry: EntryPoint,
) -> CompileResult<String> {
    let function = match entry {
        EntryPoint::None => return Ok(String::new()),
        EntryPoint::Main => {
            let mains = functions
                .values()
                .filter(|function| function.name == "main")
                .collect::<Vec<_>>();
            if mains.len() != 1 {
                return invalid(
                    "E-CODEGEN-ENTRY",
                    "an executable requires exactly one main function",
                    None,
                );
            }
            *mains[0]
        }
        EntryPoint::ReturnValue(id) => *functions.get(&id).ok_or_else(|| {
            vec![Diagnostic::error(
                Phase::Codegen,
                "E-CODEGEN-ENTRY",
                None,
                "the requested entry function is absent",
            )]
        })?,
    };
    if function.foreign.is_some() {
        return invalid(
            "E-CODEGEN-ENTRY",
            "a foreign declaration cannot be an executable entry",
            Some(function.span),
        );
    }
    if function.environment.is_some() {
        return invalid(
            "E-CODEGEN-ENTRY",
            "a closure entry requires its environment and cannot be a native entry",
            Some(function.span),
        );
    }
    if !function.parameters.is_empty() {
        return invalid(
            "E-CODEGEN-ENTRY",
            "the executable entry must have no parameters",
            Some(function.span),
        );
    }
    let symbol = symbol(function.id);
    let mut text = String::from("define i32 @main() {\nentry:\n");
    match entry {
        EntryPoint::Main => match &function.return_type {
            Type::Unit => text.push_str(&format!("  call void @{symbol}()\n  ret i32 0\n")),
            Type::I32 => text.push_str(&format!(
                "  %entry.result = call i32 @{symbol}()\n  ret i32 %entry.result\n"
            )),
            Type::ErrorUnion { result, .. }
                if matches!(result.as_ref(), Type::I32 | Type::Unit) =>
            {
                let llvm = llvm_type(&function.return_type)?;
                text.push_str(&format!(
                    "  %entry.result = call {llvm} @{symbol}()\n  %entry.error = extractvalue {llvm} %entry.result, 0\n  br i1 %entry.error, label %error, label %success\nerror:\n  ret i32 1\nsuccess:\n"
                ));
                if **result == Type::I32 {
                    text.push_str(&format!(
                        "  %entry.success = extractvalue {llvm} %entry.result, 2\n  ret i32 %entry.success\n"
                    ));
                } else {
                    text.push_str("  ret i32 0\n");
                }
            }
            _ => {
                return unsupported(
                    "native main must return i32, unit, E!i32, or E!unit",
                    Some(function.span),
                );
            }
        },
        EntryPoint::ReturnValue(_) => {
            match function.return_type {
                Type::I64 => text.push_str(&format!("  %entry.wide = call i64 @{symbol}()\n")),
                Type::U16 => text.push_str(&format!("  %entry.result = call i16 @{symbol}()\n  %entry.wide = zext i16 %entry.result to i64\n")),
                Type::U8 => text.push_str(&format!("  %entry.result = call i8 @{symbol}()\n  %entry.wide = zext i8 %entry.result to i64\n")),
                Type::I32 => text.push_str(&format!(
                    "  %entry.result = call i32 @{symbol}()\n  %entry.wide = sext i32 %entry.result to i64\n")),
                Type::Bool => text.push_str(&format!(
                    "  %entry.result = call i1 @{symbol}()\n  %entry.wide = zext i1 %entry.result to i64\n")),
                _ => return unsupported("a return-value harness requires u8, u16, i32, i64, or bool", Some(function.span)),
            }
            text.push_str("  %entry.printed = call i32 (ptr, ...) @printf(ptr @zil.return.format, i64 %entry.wide)\n  ret i32 0\n");
        }
        EntryPoint::None => unreachable!("the no-wrapper case returns above"),
    }
    text.push_str("}\n");
    Ok(text)
}

struct FunctionEmitter<'a> {
    function: &'a TypedFunction,
    target: &'a TargetProfile,
    functions: &'a BTreeMap<FunctionId, &'a TypedFunction>,
    closures: &'a BTreeMap<ClosureId, &'a ClosureDefinition>,
    resources: &'a [ResourceDefinition],
    intrinsics: &'a mut BTreeSet<(&'static str, u32)>,
    text: String,
    temporary: u64,
    continuation: u64,
    // Only locals with field ownership/update operations need field flags.
    // Complete values retain the existing inline ABI and whole-local path.
    partial_packages: BTreeSet<LocalId>,
}

impl FunctionEmitter<'_> {
    fn emit(mut self) -> CompileResult<String> {
        let mut parameters = self
            .function
            .parameters
            .iter()
            .enumerate()
            .map(|(index, local)| {
                let ty = self.local_type(*local)?;
                if *ty == Type::Unit {
                    Ok(None)
                } else {
                    Ok(Some(format!("{} %arg.{index}", llvm_type(ty)?)))
                }
            })
            .collect::<CompileResult<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        if let Some(environment) = self.function.environment {
            let ty = if resource_closures::by_pointer(self.closure_definition(environment)?) {
                "ptr".into()
            } else {
                environment_type(environment)
            };
            parameters.insert(0, format!("{ty} %env"));
        }
        let parameters = parameters.join(", ");
        self.line(format!(
            "define {} @{}({parameters}) {{",
            llvm_type(&self.function.return_type)?,
            symbol(self.function.id)
        ));
        self.line("entry:");
        let mut address_taken = BTreeSet::new();
        let mut package_views = BTreeMap::new();
        for instruction in self
            .function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
        {
            match instruction.kind {
                InstructionKind::TakePackageField { local, .. }
                | InstructionKind::DropPackageField { local, .. }
                | InstructionKind::StorePackageField { local, .. } => {
                    self.partial_packages.insert(local);
                }
                InstructionKind::BindPackageCapture { local, index } => {
                    self.partial_packages.insert(local);
                    package_views.insert(local, index);
                }
                InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local } => {
                    address_taken.insert(local);
                }
                _ => {}
            }
        }
        for local in &self.function.locals {
            if let Some(index) = package_views.get(&local.id) {
                self.package_capture_storage(local.id, *index)?;
                continue;
            }
            if self.owns_storage(&local.ty) {
                self.line(format!(
                    "  %resource.live.{} = alloca i1, align 1",
                    local.id.0
                ));
                self.line(format!(
                    "  store i1 {}, ptr %resource.live.{}, align 1",
                    self.function.parameters.contains(&local.id),
                    local.id.0
                ));
            }
            if local.ty == Type::Unit && address_taken.contains(&local.id) {
                self.line(format!("  %local.{} = alloca i8, align 1", local.id.0));
            }
            if local.ty != Type::Unit {
                self.line(format!(
                    "  %local.{} = alloca {}, align {}",
                    local.id.0,
                    llvm_type(&local.ty)?,
                    alignment(&local.ty, self.closures)?
                ));
            }
            self.package_flags(local.id, true, self.function.parameters.contains(&local.id))?;
            self.owned_array_flags(local.id, true, self.function.parameters.contains(&local.id))?;
        }
        // Fixed entry-block scratch slots prevent dynamic indexing inside a
        // loop from growing the stack on every iteration.
        let mut once_slots = BTreeSet::new();
        for instruction in self.function.blocks.iter().flat_map(|b| &b.instructions) {
            self.enum_slot(instruction)?;
            if let InstructionKind::CallClosureOnce { callee, .. } = instruction.kind
                && once_slots.insert(callee)
            {
                let ty = self.value_type(callee)?;
                self.line(format!(
                    "  %closure.once.{} = alloca {}, align {}",
                    callee.0,
                    llvm_type(ty)?,
                    alignment(ty, self.closures)?
                ));
            }
            if let InstructionKind::ExtractIndex { aggregate, .. }
            | InstructionKind::UpdateIndex { aggregate, .. } = &instruction.kind
            {
                let result = instruction.result.expect("verified index result");
                let ty = &self.function.values[aggregate.0 as usize];
                self.line(format!(
                    "  %index.slot.{} = alloca {}, align {}",
                    result.0,
                    llvm_type(ty)?,
                    alignment(ty, self.closures)?
                ));
            }
        }
        for (index, local) in self.function.parameters.iter().enumerate() {
            let ty = self.local_type(*local)?;
            if *ty != Type::Unit {
                self.line(format!(
                    "  store {} %arg.{index}, ptr %local.{}, align {}",
                    llvm_type(ty)?,
                    local.0,
                    alignment(ty, self.closures)?
                ));
            }
        }
        self.line(format!("  br label %bb.{}", self.function.entry.0));
        for block in &self.function.blocks {
            self.line(format!("bb.{}:", block.id.0));
            for instruction in &block.instructions {
                self.instruction(instruction)?;
            }
            self.terminator(&block.terminator)?;
        }
        // Even a statically unreachable source block cannot grant LLVM UB. All
        // explicit failure edges, including checked arithmetic, end in a trap.
        self.line("zil.trap:");
        self.line("  call void @llvm.trap()");
        self.line("  unreachable");
        self.line("}\n");
        Ok(self.text)
    }

    fn instruction(&mut self, instruction: &Instruction) -> CompileResult<()> {
        match &instruction.kind {
            InstructionKind::ByteBufferNew
            | InstructionKind::ByteBufferReserve { .. }
            | InstructionKind::ByteBufferTruncate { .. }
            | InstructionKind::ByteBufferPush { .. }
            | InstructionKind::ByteBufferLen { .. }
            | InstructionKind::ByteBufferCapacity { .. }
            | InstructionKind::ByteBufferBorrow { .. } => {
                self.byte_buffer_instruction(instruction)?
            }
            InstructionKind::ReadPackageField { local, index } => {
                self.read_package_field(instruction, *local, *index)?
            }
            InstructionKind::ProjectOwnedArrayField { .. }
            | InstructionKind::TakeOwnedArrayElement { .. }
            | InstructionKind::DropOwnedArrayElement { .. }
            | InstructionKind::StoreOwnedArrayElement { .. }
            | InstructionKind::DropOwnedArray { .. }
            | InstructionKind::BorrowOwnedArrayElement { .. } => {
                self.owned_array_instruction(instruction)?;
            }
            InstructionKind::DescribePackageField { local, index } => {
                self.describe_package_field(instruction, *local, *index)?
            }
            InstructionKind::DropPackage { local } => self.drop_package(instruction, *local)?,
            InstructionKind::TakePackageField { local, index } => {
                self.take_package_field(instruction, *local, *index)?
            }
            InstructionKind::DropPackageField { local, index } => {
                self.drop_package_field(instruction, *local, *index)?
            }
            InstructionKind::StorePackageField {
                local,
                index,
                value,
            } => self.store_package_field(instruction, *local, *index, *value)?,
            InstructionKind::DescribeLocal { local } => {
                self.line(format!(
                    "  %v.{} = getelementptr i8, ptr %local.{}, i64 0",
                    result(instruction)?.0,
                    local.0
                ));
            }
            InstructionKind::ReadDesc { desc, index } => {
                self.read_desc(instruction, *desc, *index)?
            }
            InstructionKind::WriteDesc { desc, index, value } => {
                self.write_desc(instruction, *desc, *index, *value)?
            }
            InstructionKind::DropManaged { local } => self.drop_managed(instruction, *local)?,
            InstructionKind::DropClosure { local } => self.drop_closure(instruction, *local)?,
            InstructionKind::BorrowCapture { index, .. } => {
                self.borrow_capture(instruction, *index)?
            }
            InstructionKind::DescribeCapture { index } => {
                self.describe_capture(instruction, *index)?
            }
            InstructionKind::TakeCapture { index } => self.take_capture(instruction, *index)?,
            InstructionKind::CallClosureBorrowed {
                callee, arguments, ..
            } => {
                self.call_closure_borrowed(instruction, *callee, arguments)?;
            }
            InstructionKind::CallClosureShared { callee, arguments } => {
                self.call_closure_shared(instruction, *callee, arguments)?
            }
            InstructionKind::CallClosureOnce { callee, arguments } => {
                self.call_closure_once(instruction, *callee, arguments)?
            }
            InstructionKind::BindPackageCapture { .. }
            | InstructionKind::EndPackageCapture { .. }
            | InstructionKind::UnsafeEnter { .. }
            | InstructionKind::UnsafeExit { .. }
            | InstructionKind::ScopeEnter { .. }
            | InstructionKind::ScopeExit { .. } => {}
            InstructionKind::MakeErrorUnion { is_error, value } => {
                self.make_error_union(instruction, *is_error, *value)?;
            }
            InstructionKind::ErrorTag { value } => self.error_tag(instruction, *value)?,
            InstructionKind::ReadErrorUnion { value, is_error } => {
                self.read_error_union(instruction, *value, *is_error)?;
            }
            InstructionKind::ForeignCall {
                function,
                arguments,
                ..
            } => {
                self.foreign_call(instruction, *function, arguments)?;
            }
            InstructionKind::Constant(constant) => {
                let result = result(instruction)?;
                match constant {
                    Constant::Unit => {}
                    Constant::Bool(value) => {
                        self.line(format!("  %v.{} = or i1 false, {value}", result.0))
                    }
                    Constant::I32(value) => {
                        self.line(format!("  %v.{} = add i32 0, {value}", result.0))
                    }
                    Constant::I64(value) => {
                        self.line(format!("  %v.{} = add i64 0, {value}", result.0))
                    }
                    Constant::U8(value) => {
                        self.line(format!("  %v.{} = add i8 0, {value}", result.0))
                    }
                    Constant::U16(value) => {
                        self.line(format!("  %v.{} = add i16 0, {value}", result.0))
                    }
                    Constant::NullHandle | Constant::NullWideString => {
                        self.line(format!("  %v.{} = freeze ptr null", result.0))
                    }
                    Constant::WideString(_) => self.line(format!(
                        "  %v.{} = freeze ptr @zil.wstr.{}.{}",
                        result.0, self.function.id.0, result.0
                    )),
                }
            }
            InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                let result = result(instruction)?;
                let ty = self.local_type(*local)?;
                if *ty != Type::Unit {
                    self.line(format!(
                        "  %v.{} = load {}, ptr %local.{}, align {}",
                        result.0,
                        llvm_type(ty)?,
                        local.0,
                        alignment(ty, self.closures)?
                    ));
                }
                if matches!(instruction.kind, InstructionKind::TakeLocal { .. })
                    && self.owns_storage(self.local_type(*local)?)
                {
                    self.line(format!(
                        "  store i1 false, ptr %resource.live.{}, align 1",
                        local.0
                    ));
                    self.package_flags(*local, false, false)?;
                    self.owned_array_flags(*local, false, false)?;
                }
            }
            InstructionKind::Store { local, value } => {
                let ty = self.local_type(*local)?;
                if *ty != Type::Unit {
                    self.line(format!(
                        "  store {} %v.{}, ptr %local.{}, align {}",
                        llvm_type(ty)?,
                        value.0,
                        local.0,
                        alignment(ty, self.closures)?
                    ));
                }
                if self.owns_storage(self.local_type(*local)?) {
                    self.line(format!(
                        "  store i1 true, ptr %resource.live.{}, align 1",
                        local.0
                    ));
                    self.package_flags(*local, false, true)?;
                    self.owned_array_flags(*local, false, true)?;
                }
            }
            InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local } => {
                self.line(format!(
                    "  %v.{} = getelementptr i8, ptr %local.{}, i64 0",
                    result(instruction)?.0,
                    local.0
                ));
            }
            InstructionKind::AddressField { address, index } => {
                let Type::Address { inner, .. } = self.value_type(*address)? else {
                    return invalid(
                        "E-CODEGEN-CFG",
                        "field projection requires an address",
                        Some(instruction.span),
                    );
                };
                self.line(format!(
                    "  %v.{} = getelementptr {}, ptr %v.{}, i32 0, i32 {}",
                    result(instruction)?.0,
                    llvm_type(inner)?,
                    address.0,
                    index
                ));
            }
            InstructionKind::AddressIndex { address, index } => {
                if self.value_type(*address)?.is_slice_address() {
                    return self.slice_index(instruction, *address, *index);
                }
                let Type::Address { inner, .. } = self.value_type(*address)? else {
                    return invalid(
                        "E-CODEGEN-CFG",
                        "index projection requires an address",
                        Some(instruction.span),
                    );
                };
                let Type::Array { length, .. } = inner.as_ref() else {
                    return invalid(
                        "E-CODEGEN-CFG",
                        "index projection requires an array",
                        Some(instruction.span),
                    );
                };
                let length = *length;
                let array_type = llvm_type(inner)?;
                let index_value = self.widened_index(*index)?;
                let valid = self.temp();
                self.line(format!("  {valid} = icmp ult i64 {index_value}, {length}"));
                let continuation = self.continuation;
                self.continuation += 1;
                self.line(format!(
                    "  br i1 {valid}, label %address.ok.{continuation}, label %zil.trap"
                ));
                self.line(format!("address.ok.{continuation}:"));
                self.line(format!(
                    "  %v.{} = getelementptr {array_type}, ptr %v.{}, i64 0, i64 {index_value}",
                    result(instruction)?.0,
                    address.0
                ));
            }
            InstructionKind::ReborrowAddress { address, .. } => {
                if self.value_type(*address)?.is_slice_address() {
                    return self.reborrow_slice(instruction, *address);
                }
                self.line(format!(
                    "  %v.{} = getelementptr i8, ptr %v.{}, i64 0",
                    result(instruction)?.0,
                    address.0
                ));
            }
            InstructionKind::AddressSlice {
                address,
                start,
                end,
            } => {
                self.slice_range(instruction, *address, *start, *end)?;
            }
            InstructionKind::SliceLength { address } => {
                self.line(format!(
                    "  %v.{} = extractvalue {{ ptr, i64 }} %v.{}, 1",
                    result(instruction)?.0,
                    address.0
                ));
            }
            InstructionKind::WriteAddress { address, value } => {
                let Type::Address { inner, .. } = self.value_type(*address)? else {
                    return invalid(
                        "E-CODEGEN-CFG",
                        "write requires address",
                        Some(instruction.span),
                    );
                };
                if **inner != Type::Unit {
                    self.line(format!(
                        "  store {} %v.{}, ptr %v.{}, align {}",
                        llvm_type(inner)?,
                        value.0,
                        address.0,
                        alignment(inner, self.closures)?
                    ));
                }
            }
            InstructionKind::ReadAddress { address } => {
                let Type::Address { inner, .. } = self.value_type(*address)? else {
                    return invalid(
                        "E-CODEGEN-CFG",
                        "ReadAddress requires an address",
                        Some(instruction.span),
                    );
                };
                if **inner != Type::Unit {
                    self.line(format!(
                        "  %v.{} = load {}, ptr %v.{}, align {}",
                        result(instruction)?.0,
                        llvm_type(inner)?,
                        address.0,
                        alignment(inner, self.closures)?
                    ));
                }
            }
            InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local) => {
                if self.owns_storage(self.local_type(*local)?) {
                    self.line(format!(
                        "  store i1 false, ptr %resource.live.{}, align 1",
                        local.0
                    ));
                    self.package_flags(*local, false, false)?;
                    self.owned_array_flags(*local, false, false)?;
                }
                // S0 keeps the real stack allocation until return. Lexical
                // validity was independently proved; no runtime table needed.
            }
            InstructionKind::ConvertInteger { value, target } => {
                self.convert_integer(*value, target, result(instruction)?)?;
            }
            InstructionKind::Unary { op, operand } => {
                let result = result(instruction)?;
                let ty = self.value_type(*operand)?.clone();
                match op {
                    UnaryOp::Negate => {
                        self.checked_binary("ssub", &ty, "0", &value(*operand), result)?
                    }
                    UnaryOp::Not => {
                        self.line(format!("  %v.{} = xor i1 %v.{}, true", result.0, operand.0))
                    }
                    UnaryOp::BitNot => self.line(format!(
                        "  %v.{} = xor {} %v.{}, -1",
                        result.0,
                        llvm_type(&ty)?,
                        operand.0
                    )),
                    _ => {
                        return unsupported(
                            format!("LLVM unary operation {op:?}"),
                            Some(instruction.span),
                        );
                    }
                }
            }
            InstructionKind::Binary { op, left, right } => {
                self.binary(*op, *left, *right, result(instruction)?, instruction.span)?;
            }
            InstructionKind::Call {
                function,
                arguments,
            } => {
                let callee = self.functions.get(function).ok_or_else(|| {
                    vec![Diagnostic::error(
                        Phase::Codegen,
                        "E-CODEGEN-CFG",
                        Some(instruction.span),
                        "call target is absent",
                    )]
                })?;
                if callee.environment.is_some() {
                    return invalid(
                        "E-CODEGEN-CFG",
                        "an ordinary call cannot enter a closure environment function",
                        Some(instruction.span),
                    );
                }
                let mut emitted_arguments = Vec::new();
                for argument in arguments {
                    let ty = self.value_type(*argument)?;
                    if *ty != Type::Unit {
                        emitted_arguments.push(format!("{} %v.{}", llvm_type(ty)?, argument.0));
                    }
                }
                let assignment = if callee.return_type == Type::Unit {
                    String::new()
                } else {
                    format!("%v.{} = ", result(instruction)?.0)
                };
                self.line(format!(
                    "  {assignment}call {} @{}({})",
                    llvm_type(&callee.return_type)?,
                    symbol(*function),
                    emitted_arguments.join(", ")
                ));
            }
            InstructionKind::MakeVariant { variant, fields } => {
                self.make_variant(instruction, *variant, fields)?
            }
            InstructionKind::VariantTag { value } => self.variant_tag(instruction, *value)?,
            InstructionKind::ReadVariant {
                value,
                variant,
                index,
            } => self.read_variant(instruction, *value, *variant, *index)?,
            InstructionKind::FunctionRef { function }
            | InstructionKind::ForeignFunctionRef { function } => {
                self.line(format!(
                    "  %v.{} = freeze ptr @{}",
                    result(instruction)?.0,
                    symbol(*function)
                ));
            }
            InstructionKind::CallIndirect { callee, arguments } => {
                let Type::FunctionPointer {
                    result: return_type,
                    ..
                } = self.value_type(*callee)?
                else {
                    return invalid(
                        "E-CODEGEN-CFG",
                        "indirect call requires a function pointer",
                        Some(instruction.span),
                    );
                };
                let mut emitted_arguments = Vec::new();
                for argument in arguments {
                    let ty = self.value_type(*argument)?;
                    if *ty != Type::Unit {
                        emitted_arguments.push(format!("{} %v.{}", llvm_type(ty)?, argument.0));
                    }
                }
                let assignment = if **return_type == Type::Unit {
                    String::new()
                } else {
                    format!("%v.{} = ", result(instruction)?.0)
                };
                self.line(format!(
                    "  {assignment}call {} %v.{}({})",
                    llvm_type(return_type)?,
                    callee.0,
                    emitted_arguments.join(", ")
                ));
            }
            InstructionKind::ExtractIndex { aggregate, index } => {
                self.indexed(instruction, *aggregate, *index, None)?
            }
            InstructionKind::UpdateIndex {
                aggregate,
                index,
                value,
            } => self.indexed(instruction, *aggregate, *index, Some(*value))?,
            InstructionKind::MakeAggregate { fields }
            | InstructionKind::MakeManaged { fields }
            | InstructionKind::MakeOwnedArray { elements: fields }
            | InstructionKind::MakePackage { fields } => {
                let result = instruction.result.expect("verified aggregate result");
                let ty = &self.function.values[result.0 as usize];
                let llvm = llvm_type(ty)?;
                let mut previous = "zeroinitializer".to_owned();
                if fields.is_empty() {
                    self.line(format!("{} = freeze {llvm} zeroinitializer", value(result)));
                }
                for (index, field) in fields.iter().enumerate() {
                    let ft = &self.function.values[field.0 as usize];
                    let target = if index + 1 == fields.len() {
                        value(result)
                    } else {
                        self.temp()
                    };
                    let fv = if *ft == Type::Unit {
                        "zeroinitializer".into()
                    } else {
                        value(*field)
                    };
                    self.line(format!(
                        "{target} = insertvalue {llvm} {previous}, {} {fv}, {index}",
                        aggregate_field_type(ft)?
                    ));
                    previous = target;
                }
            }
            InstructionKind::ExtractField { aggregate, index } => {
                let result = instruction.result.expect("verified field result");
                if self.function.values[result.0 as usize] != Type::Unit {
                    self.line(format!(
                        "{} = extractvalue {} {}, {index}",
                        value(result),
                        llvm_type(&self.function.values[aggregate.0 as usize])?,
                        value(*aggregate)
                    ));
                }
            }
            InstructionKind::UpdateField {
                aggregate,
                index,
                value: field,
            } => {
                let result = instruction.result.expect("verified update result");
                let ft = &self.function.values[field.0 as usize];
                let fv = if *ft == Type::Unit {
                    "zeroinitializer".into()
                } else {
                    value(*field)
                };
                self.line(format!(
                    "{} = insertvalue {} {}, {} {fv}, {index}",
                    value(result),
                    llvm_type(&self.function.values[aggregate.0 as usize])?,
                    value(*aggregate),
                    aggregate_field_type(ft)?
                ));
            }
            InstructionKind::MakeClosure { closure, captures } => {
                let output = result(instruction)?;
                let environment = environment_type(*closure);
                let definition = self.closure_definition(*closure)?;
                let mut fields = Vec::new();
                for (index, capture) in captures.iter().enumerate() {
                    let ty = self.value_type(*capture)?;
                    fields.push((
                        index,
                        capture_type(ty)?,
                        if *ty == Type::Unit {
                            "0".into()
                        } else {
                            value(*capture)
                        },
                    ));
                }
                let owned_count = definition
                    .captures
                    .iter()
                    .map(resource_closures::flag_count)
                    .sum::<usize>();
                for offset in 0..owned_count {
                    fields.push((captures.len() + offset, "i1".into(), "true".into()));
                }
                if fields.is_empty() {
                    self.line(format!(
                        "  {} = freeze {environment} zeroinitializer",
                        value(output)
                    ));
                } else {
                    let mut aggregate = String::from("zeroinitializer");
                    let count = fields.len();
                    for (position, (index, field, captured)) in fields.into_iter().enumerate() {
                        let next = if position + 1 == count {
                            value(output)
                        } else {
                            self.temp()
                        };
                        self.line(format!("  {next} = insertvalue {environment} {aggregate}, {field} {captured}, {index}"));
                        aggregate = next;
                    }
                }
            }
            InstructionKind::ReadCapture { index } => {
                let environment = self
                    .function
                    .environment
                    .expect("verified capture environment");
                let definition = self.closure_definition(environment)?;
                let ty = definition.captures[*index as usize].clone();
                let mutable = resource_closures::by_pointer(definition);
                if ty != Type::Unit {
                    if mutable {
                        let address = self.temp();
                        self.line(format!(
                            "  {address} = getelementptr {}, ptr %env, i32 0, i32 {index}",
                            environment_type(environment)
                        ));
                        self.line(format!(
                            "  %v.{} = load {}, ptr {address}, align {}",
                            result(instruction)?.0,
                            llvm_type(&ty)?,
                            alignment(&ty, self.closures)?
                        ));
                    } else {
                        self.line(format!(
                            "  %v.{} = extractvalue {} %env, {index}",
                            result(instruction)?.0,
                            environment_type(environment)
                        ));
                    }
                }
            }
            InstructionKind::WriteCapture {
                index,
                value: written,
            } => {
                let environment = self
                    .function
                    .environment
                    .expect("verified capture environment");
                let ty = self.closure_definition(environment)?.captures[*index as usize].clone();
                if ty != Type::Unit {
                    let address = self.temp();
                    self.line(format!(
                        "  {address} = getelementptr {}, ptr %env, i32 0, i32 {index}",
                        environment_type(environment)
                    ));
                    self.line(format!(
                        "  store {} {}, ptr {address}, align {}",
                        llvm_type(&ty)?,
                        value(*written),
                        alignment(&ty, self.closures)?
                    ));
                }
            }
            InstructionKind::CallClosureMut { callee, arguments } => {
                // Requested Exclusive may adapt a concrete Shared entry. Use its
                // actual ABI; Verify already checked the mutable location.
                self.call_closure_shared(instruction, *callee, arguments)?;
            }
            InstructionKind::CallClosure { callee, arguments } => {
                let Type::Closure(environment) = self.value_type(*callee)? else {
                    return invalid(
                        "E-CODEGEN-CFG",
                        "closure call has no concrete environment type",
                        Some(instruction.span),
                    );
                };
                let definition = self.closure_definition(*environment)?;
                let function = definition.function;
                let return_type = llvm_type(&definition.result)?;
                let assignment = if definition.result == Type::Unit {
                    String::new()
                } else {
                    format!("%v.{} = ", result(instruction)?.0)
                };
                let mut emitted_arguments = vec![format!(
                    "{} {}",
                    environment_type(*environment),
                    value(*callee)
                )];
                for argument in arguments {
                    let ty = self.value_type(*argument)?;
                    if *ty != Type::Unit {
                        emitted_arguments.push(format!("{} {}", llvm_type(ty)?, value(*argument)));
                    }
                }
                self.line(format!(
                    "  {assignment}call {return_type} @{}({})",
                    symbol(function),
                    emitted_arguments.join(", ")
                ));
            }
            InstructionKind::Unsupported(feature) => {
                return unsupported(feature.clone(), Some(instruction.span));
            }
        }
        Ok(())
    }

    fn binary(
        &mut self,
        operation: BinaryOp,
        left: ValueId,
        right: ValueId,
        result: ValueId,
        span: Span,
    ) -> CompileResult<()> {
        let ty = self.value_type(left)?.clone();
        let operand_type = llvm_type(&ty)?;
        let (left_text, right_text) = (value(left), value(right));
        match operation {
            BinaryOp::Add => self.checked_binary("sadd", &ty, &left_text, &right_text, result)?,
            BinaryOp::Subtract => {
                self.checked_binary("ssub", &ty, &left_text, &right_text, result)?
            }
            BinaryOp::Multiply => {
                self.checked_binary("smul", &ty, &left_text, &right_text, result)?
            }
            BinaryOp::Divide | BinaryOp::Remainder => {
                let is_zero = self.temp();
                self.line(format!(
                    "  {is_zero} = icmp eq {operand_type} {right_text}, 0"
                ));
                if matches!(ty, Type::U8 | Type::U16) {
                    self.trap_if(&is_zero);
                } else {
                    let minimum = match ty {
                        Type::I32 => i32::MIN.to_string(),
                        Type::I64 => i64::MIN.to_string(),
                        _ => return unsupported("non-integer division", Some(span)),
                    };
                    let is_minimum = self.temp();
                    let is_minus_one = self.temp();
                    let is_overflow = self.temp();
                    let is_invalid = self.temp();
                    self.line(format!(
                        "  {is_minimum} = icmp eq {operand_type} {left_text}, {minimum}"
                    ));
                    self.line(format!(
                        "  {is_minus_one} = icmp eq {operand_type} {right_text}, -1"
                    ));
                    self.line(format!(
                        "  {is_overflow} = and i1 {is_minimum}, {is_minus_one}"
                    ));
                    self.line(format!("  {is_invalid} = or i1 {is_zero}, {is_overflow}"));
                    self.trap_if(&is_invalid);
                }
                let op = match (
                    matches!(ty, Type::U8 | Type::U16),
                    operation == BinaryOp::Divide,
                ) {
                    (true, true) => "udiv",
                    (true, false) => "urem",
                    (false, true) => "sdiv",
                    (false, false) => "srem",
                };
                self.line(format!(
                    "  %v.{} = {op} {operand_type} {left_text}, {right_text}",
                    result.0
                ));
            }
            BinaryOp::AddWrapping | BinaryOp::SubtractWrapping | BinaryOp::MultiplyWrapping => {
                let op = match operation {
                    BinaryOp::AddWrapping => "add",
                    BinaryOp::SubtractWrapping => "sub",
                    _ => "mul",
                };
                self.line(format!(
                    "  %v.{} = {op} {operand_type} {left_text}, {right_text}",
                    result.0
                ));
            }
            BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual => {
                if ty == Type::Unit {
                    match operation {
                        BinaryOp::Equal => {
                            self.line(format!("  %v.{} = or i1 false, true", result.0))
                        }
                        BinaryOp::NotEqual => {
                            self.line(format!("  %v.{} = or i1 false, false", result.0))
                        }
                        _ => return unsupported("ordered comparison of unit", Some(span)),
                    }
                } else {
                    let predicate = match operation {
                        BinaryOp::Equal => "eq",
                        BinaryOp::NotEqual => "ne",
                        BinaryOp::Less if matches!(ty, Type::U8 | Type::U16) => "ult",
                        BinaryOp::LessEqual if matches!(ty, Type::U8 | Type::U16) => "ule",
                        BinaryOp::Greater if matches!(ty, Type::U8 | Type::U16) => "ugt",
                        BinaryOp::GreaterEqual if matches!(ty, Type::U8 | Type::U16) => "uge",
                        BinaryOp::Less => "slt",
                        BinaryOp::LessEqual => "sle",
                        BinaryOp::Greater => "sgt",
                        _ => "sge",
                    };
                    self.line(format!(
                        "  %v.{} = icmp {predicate} {operand_type} {left_text}, {right_text}",
                        result.0
                    ));
                }
            }
            BinaryOp::And
            | BinaryOp::Or
            | BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor => {
                let op = match operation {
                    BinaryOp::And | BinaryOp::BitAnd => "and",
                    BinaryOp::Or | BinaryOp::BitOr => "or",
                    _ => "xor",
                };
                self.line(format!(
                    "  %v.{} = {op} {operand_type} {left_text}, {right_text}",
                    result.0
                ));
            }
            BinaryOp::ShiftLeft | BinaryOp::ShiftRight => {
                let bits = match ty {
                    Type::U8 => 8,
                    Type::U16 => 16,
                    Type::I32 => 32,
                    Type::I64 => 64,
                    _ => return unsupported("LLVM non-integer shift lowering", Some(span)),
                };
                let invalid = self.temp();
                // Unsigned comparison also rejects every negative signed count.
                // Check before the shift, whose out-of-range LLVM result is poison.
                self.line(format!(
                    "  {invalid} = icmp uge {operand_type} {right_text}, {bits}"
                ));
                self.trap_if(&invalid);
                let op = if operation == BinaryOp::ShiftLeft {
                    "shl"
                } else if matches!(ty, Type::I32 | Type::I64) {
                    "ashr"
                } else {
                    "lshr"
                };
                self.line(format!(
                    "  %v.{} = {op} {operand_type} {left_text}, {right_text}",
                    result.0
                ));
            }
        }
        Ok(())
    }

    fn widened_index(&mut self, index: ValueId) -> CompileResult<String> {
        match self.value_type(index)? {
            Type::U8 | Type::U16 => {
                let from = llvm_type(self.value_type(index)?)?;
                let extended = self.temp();
                self.line(format!(
                    "  {extended} = zext {from} {} to i64",
                    value(index)
                ));
                Ok(extended)
            }
            Type::I32 => {
                let extended = self.temp();
                self.line(format!("  {extended} = sext i32 {} to i64", value(index)));
                Ok(extended)
            }
            Type::I64 => Ok(value(index)),
            _ => unsupported(
                "array index must have a verified integer type",
                Some(self.function.span),
            ),
        }
    }

    fn convert_integer(
        &mut self,
        input: ValueId,
        target: &Type,
        result: ValueId,
    ) -> CompileResult<()> {
        let source = self.value_type(input)?.clone();
        let from = llvm_type(&source)?;
        let to = llvm_type(target)?;
        let input = value(input);
        let operation = match (&source, target) {
            (a, b) if a == b && a.is_integer() => {
                self.line(format!("  %v.{} = add {to} {input}, 0", result.0));
                return Ok(());
            }
            (Type::U8, Type::U16 | Type::I32 | Type::I64) | (Type::U16, Type::I32 | Type::I64) => {
                "zext"
            }
            (Type::I32, Type::I64) => "sext",
            (Type::I32 | Type::I64 | Type::U16, Type::U8) | (Type::I32 | Type::I64, Type::U16) => {
                // Unsigned comparison also rejects every negative signed input.
                let maximum = if *target == Type::U8 { 255 } else { 65535 };
                let invalid = self.temp();
                self.line(format!("  {invalid} = icmp ugt {from} {input}, {maximum}"));
                self.trap_if(&invalid);
                "trunc"
            }
            (Type::I64, Type::I32) => {
                let below = self.temp();
                let above = self.temp();
                let invalid = self.temp();
                self.line(format!("  {below} = icmp slt i64 {input}, {}", i32::MIN));
                self.line(format!("  {above} = icmp sgt i64 {input}, {}", i32::MAX));
                self.line(format!("  {invalid} = or i1 {below}, {above}"));
                self.trap_if(&invalid);
                "trunc"
            }
            _ => return unsupported("verified integer conversion type", Some(self.function.span)),
        };
        self.line(format!(
            "  %v.{} = {operation} {from} {input} to {to}",
            result.0
        ));
        Ok(())
    }

    fn checked_binary(
        &mut self,
        operation: &'static str,
        ty: &Type,
        left: &str,
        right: &str,
        result: ValueId,
    ) -> CompileResult<()> {
        let operation = if matches!(ty, Type::U8 | Type::U16) {
            match operation {
                "sadd" => "uadd",
                "ssub" => "usub",
                "smul" => "umul",
                other => other,
            }
        } else {
            operation
        };
        let bits = match ty {
            Type::U8 => 8,
            Type::U16 => 16,
            Type::I32 => 32,
            Type::I64 => 64,
            _ => return unsupported("checked non-integer arithmetic", Some(self.function.span)),
        };
        self.intrinsics.insert((operation, bits));
        let pair = self.temp();
        let overflow = self.temp();
        self.line(format!("  {pair} = call {{ i{bits}, i1 }} @llvm.{operation}.with.overflow.i{bits}(i{bits} {left}, i{bits} {right})"));
        self.line(format!(
            "  %v.{} = extractvalue {{ i{bits}, i1 }} {pair}, 0",
            result.0
        ));
        self.line(format!(
            "  {overflow} = extractvalue {{ i{bits}, i1 }} {pair}, 1"
        ));
        self.trap_if(&overflow);
        Ok(())
    }

    fn trap_if(&mut self, condition: &str) {
        let continuation = self.continuation;
        self.continuation += 1;
        self.line(format!(
            "  br i1 {condition}, label %zil.trap, label %checked.{continuation}"
        ));
        self.line(format!("checked.{continuation}:"));
    }

    fn terminator(&mut self, terminator: &Terminator) -> CompileResult<()> {
        match terminator {
            Terminator::Return(value) => {
                if self.function.return_type == Type::Unit {
                    self.line("  ret void");
                } else {
                    let returned = value.ok_or_else(|| {
                        vec![Diagnostic::error(
                            Phase::Codegen,
                            "E-CODEGEN-CFG",
                            Some(self.function.span),
                            "a non-unit return has no value",
                        )]
                    })?;
                    self.line(format!(
                        "  ret {} %v.{}",
                        llvm_type(&self.function.return_type)?,
                        returned.0
                    ));
                }
            }
            Terminator::Jump(block) => self.line(format!("  br label %bb.{}", block.0)),
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => self.line(format!(
                "  br i1 %v.{}, label %bb.{}, label %bb.{}",
                condition.0, then_block.0, else_block.0
            )),
            Terminator::Unreachable | Terminator::Trap => self.line("  br label %zil.trap"),
        }
        Ok(())
    }

    fn local_type(&self, id: LocalId) -> CompileResult<&Type> {
        self.function
            .locals
            .get(id.0 as usize)
            .filter(|local| local.id == id)
            .map(|local| &local.ty)
            .ok_or_else(|| {
                vec![Diagnostic::error(
                    Phase::Codegen,
                    "E-CODEGEN-CFG",
                    Some(self.function.span),
                    "invalid local identity",
                )]
            })
    }
    fn value_type(&self, id: ValueId) -> CompileResult<&Type> {
        self.function.values.get(id.0 as usize).ok_or_else(|| {
            vec![Diagnostic::error(
                Phase::Codegen,
                "E-CODEGEN-CFG",
                Some(self.function.span),
                "invalid value identity",
            )]
        })
    }
    fn closure_definition(&self, id: ClosureId) -> CompileResult<&ClosureDefinition> {
        self.closures.get(&id).copied().ok_or_else(|| {
            vec![Diagnostic::error(
                Phase::Codegen,
                "E-CODEGEN-CFG",
                Some(self.function.span),
                "concrete closure definition is absent",
            )]
        })
    }
    fn indexed(
        &mut self,
        instruction: &Instruction,
        aggregate: ValueId,
        index: ValueId,
        update: Option<ValueId>,
    ) -> CompileResult<()> {
        let result = instruction.result.expect("verified array operation result");
        let ty = &self.function.values[aggregate.0 as usize];
        let Type::Array { element, length } = ty else {
            return unsupported("verified index lost array type", Some(instruction.span));
        };
        let element = (**element).clone();
        let length = *length;
        let llvm = llvm_type(ty)?;
        let align = alignment(ty, self.closures)?;
        let index_value = self.widened_index(index)?;
        let valid = self.temp();
        self.line(format!("  {valid} = icmp ult i64 {index_value}, {length}"));
        let continuation = self.continuation;
        self.continuation += 1;
        self.line(format!(
            "  br i1 {valid}, label %index.ok.{continuation}, label %zil.trap"
        ));
        self.line(format!("index.ok.{continuation}:"));
        self.line(format!(
            "  store {llvm} {}, ptr %index.slot.{}, align {align}",
            value(aggregate),
            result.0
        ));
        let address = self.temp();
        self.line(format!(
            "  {address} = getelementptr {llvm}, ptr %index.slot.{}, i64 0, i64 {index_value}",
            result.0
        ));
        if let Some(update) = update {
            let v = if element == Type::Unit {
                "zeroinitializer".into()
            } else {
                value(update)
            };
            self.line(format!(
                "  store {} {v}, ptr {address}, align {}",
                aggregate_field_type(&element)?,
                alignment(&element, self.closures)?
            ));
            self.line(format!(
                "  {} = load {llvm}, ptr %index.slot.{}, align {align}",
                value(result),
                result.0
            ));
        } else if element != Type::Unit {
            self.line(format!(
                "  {} = load {}, ptr {address}, align {}",
                value(result),
                llvm_type(&element)?,
                alignment(&element, self.closures)?
            ));
        }
        Ok(())
    }
    fn temp(&mut self) -> String {
        let id = self.temporary;
        self.temporary += 1;
        format!("%tmp.{id}")
    }
    fn line(&mut self, text: impl AsRef<str>) {
        self.text.push_str(text.as_ref());
        self.text.push('\n');
    }
}

fn result(instruction: &Instruction) -> CompileResult<ValueId> {
    instruction.result.ok_or_else(|| {
        vec![Diagnostic::error(
            Phase::Codegen,
            "E-CODEGEN-CFG",
            Some(instruction.span),
            "instruction result is absent",
        )]
    })
}
fn symbol(id: FunctionId) -> String {
    format!("zil.fn.{}", id.0)
}
fn value(id: ValueId) -> String {
    format!("%v.{}", id.0)
}
fn environment_type(id: ClosureId) -> String {
    format!("%zil_env_{}", id.0)
}
fn llvm_type(ty: &Type) -> CompileResult<String> {
    match ty {
        Type::Unit => Ok("void".into()),
        Type::Bool => Ok("i1".into()),
        Type::I32 => Ok("i32".into()),
        Type::I64 => Ok("i64".into()),
        Type::U8 => Ok("i8".into()),
        Type::U16 => Ok("i16".into()),
        Type::ByteBuffer => Ok("{ ptr, i64, i64 }".into()),
        Type::ForeignHandle | Type::CWideString | Type::ForeignCallback { .. } => Ok("ptr".into()),
        Type::Array { element, length } => {
            Ok(format!("[{length} x {}]", aggregate_field_type(element)?))
        }
        Type::Tuple(fields) => Ok(format!(
            "{{ {} }}",
            fields
                .iter()
                .map(aggregate_field_type)
                .collect::<CompileResult<Vec<_>>>()?
                .join(", ")
        )),
        Type::Record { fields, .. } | Type::Package { fields, .. } => Ok(format!(
            "{{ {} }}",
            fields
                .iter()
                .map(|(_, t)| aggregate_field_type(t))
                .collect::<CompileResult<Vec<_>>>()?
                .join(", ")
        )),
        Type::Enum { .. } => enums::llvm_enum_type(ty),
        Type::ErrorUnion { error, result } => Ok(format!(
            "{{ i1, {}, {} }}",
            aggregate_field_type(error)?,
            aggregate_field_type(result)?
        )),
        Type::Closure(id) => Ok(environment_type(*id)),
        Type::FunctionPointer { .. } => Ok("ptr".into()),
        Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Slice { .. }) => {
            Ok("{ ptr, i64 }".into())
        }
        Type::Address { inner, .. }
            if inner.is_copy_value() || matches!(inner.as_ref(), Type::Closure(_)) =>
        {
            Ok("ptr".into())
        }
        Type::Desc(payload)
            if (matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value())
                || payload.as_ref() == &Type::ByteBuffer =>
        {
            Ok("ptr".into())
        }
        Type::Managed(payload)
            if (matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value())
                || payload.as_ref() == &Type::ByteBuffer =>
        {
            llvm_type(payload)
        }
        _ => unsupported(format!("LLVM type {}", ty.name()), None),
    }
}
fn aggregate_field_type(ty: &Type) -> CompileResult<String> {
    if *ty == Type::Unit {
        Ok("{}".into())
    } else {
        llvm_type(ty)
    }
}
fn capture_type(ty: &Type) -> CompileResult<String> {
    match ty {
        Type::Unit => Ok("i8".into()),
        _ if ty.is_copy_value()
            || matches!(ty, Type::Closure(_))
            || matches!(ty, Type::Address { inner, .. } if inner.is_copy_value() || matches!(inner.as_ref(), Type::Closure(_)))
            || matches!(ty, Type::Managed(payload) | Type::Desc(payload) if payload.is_copy_value())
            || matches!(ty, Type::Managed(payload) if payload.as_ref() == &Type::ByteBuffer)
            || matches!(ty, Type::Package { .. }) =>
        {
            llvm_type(ty)
        }
        _ => unsupported(format!("LLVM capture field {}", ty.name()), None),
    }
}
fn alignment(ty: &Type, closures: &BTreeMap<ClosureId, &ClosureDefinition>) -> CompileResult<u32> {
    match ty {
        Type::Unit | Type::Bool => Ok(1),
        Type::U8 => Ok(1),
        Type::U16 => Ok(2),
        Type::ByteBuffer => Ok(8),
        Type::ForeignHandle | Type::CWideString | Type::ForeignCallback { .. } => Ok(8),
        Type::I32 => Ok(4),
        Type::I64 => Ok(8),
        Type::FunctionPointer { .. } => Ok(8),
        Type::Address { inner, .. }
            if inner.is_address_pointee() || matches!(inner.as_ref(), Type::Closure(_)) =>
        {
            Ok(8)
        }
        Type::Desc(payload)
            if (matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value())
                || payload.as_ref() == &Type::ByteBuffer =>
        {
            Ok(8)
        }
        Type::Managed(payload)
            if (matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value())
                || payload.as_ref() == &Type::ByteBuffer =>
        {
            alignment(payload, closures)
        }
        Type::Array { element, .. } => alignment(element, closures),
        Type::Tuple(fields) => fields
            .iter()
            .try_fold(1, |a, t| Ok(a.max(alignment(t, closures)?))),
        Type::Record { fields, .. } | Type::Package { fields, .. } => fields
            .iter()
            .try_fold(1, |a, (_, t)| Ok(a.max(alignment(t, closures)?))),
        Type::Enum { .. } => Ok(enums::union_layout(ty)?.align),
        Type::ErrorUnion { error, result } => {
            Ok(alignment(error, closures)?.max(alignment(result, closures)?))
        }
        Type::Closure(id) => {
            let definition = closures.get(id).ok_or_else(|| {
                vec![Diagnostic::error(
                    Phase::Codegen,
                    "E-CODEGEN-CFG",
                    None,
                    "closure layout has no definition",
                )]
            })?;
            definition.captures.iter().try_fold(1, |maximum, field| {
                // Verify checked the complete inline environment DAG and depth
                // before this recursive machine-alignment calculation.
                if !field.is_copy_value()
                    && !matches!(field, Type::Closure(_))
                    && !matches!(field,Type::Address { inner, .. } if inner.is_copy_value() || matches!(inner.as_ref(), Type::Closure(_)))
                    && !matches!(field,Type::Managed(payload) | Type::Desc(payload) if payload.is_copy_value())
                    && !matches!(field,Type::Managed(payload) if payload.as_ref() == &Type::ByteBuffer)
                    && !matches!(field, Type::Package { .. })
                {
                    return unsupported("unsupported closure field alignment", None);
                }
                let field_alignment = alignment(field, closures)?;
                Ok(maximum.max(field_alignment))
            })
        }
        _ => unsupported(format!("LLVM alignment for {}", ty.name()), None),
    }
}
fn invalid<T>(code: &str, message: impl Into<String>, span: Option<Span>) -> CompileResult<T> {
    Err(vec![Diagnostic::error(Phase::Codegen, code, span, message)])
}
fn unsupported<T>(message: impl Into<String>, span: Option<Span>) -> CompileResult<T> {
    Err(vec![Diagnostic::unsupported(Phase::Codegen, message, span)])
}

fn callable_parameter_type(ty: &Type, closures: &[ClosureDefinition]) -> bool {
    let id = match ty {
        Type::Closure(id) => id,
        Type::Address { inner, .. } => match inner.as_ref() {
            Type::Closure(id) => id,
            _ => return false,
        },
        _ => return false,
    };
    closures
        .get(id.0 as usize)
        .is_some_and(|definition| definition.id == *id)
}
