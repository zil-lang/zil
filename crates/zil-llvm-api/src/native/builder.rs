//! The only direct LLVMBuilder adapter. Inputs have passed machine preflight.
//! Every pointer is owned by the current Context/Module and stays on this thread.
use super::*;
use crate::{model as m, preflight::Schedule};
use std::collections::BTreeMap;

unsafe extern "C" {
    fn LLVMModuleCreateWithNameInContext(name: *const c_char, context: Handle) -> Handle;
    fn LLVMSetTarget(module: Handle, triple: *const c_char);
    fn LLVMVoidTypeInContext(context: Handle) -> Handle;
    fn LLVMIntTypeInContext(context: Handle, bits: c_uint) -> Handle;
    fn LLVMPointerTypeInContext(context: Handle, address_space: c_uint) -> Handle;
    fn LLVMArrayType2(element: Handle, count: u64) -> Handle;
    fn LLVMStructTypeInContext(
        context: Handle,
        elements: *mut Handle,
        count: c_uint,
        packed: c_int,
    ) -> Handle;
    fn LLVMStructCreateNamed(context: Handle, name: *const c_char) -> Handle;
    fn LLVMStructSetBody(ty: Handle, elements: *mut Handle, count: c_uint, packed: c_int);
    fn LLVMFunctionType(
        result: Handle,
        params: *mut Handle,
        count: c_uint,
        variadic: c_int,
    ) -> Handle;
    fn LLVMAddFunction(module: Handle, name: *const c_char, ty: Handle) -> Handle;
    fn LLVMAddGlobal(module: Handle, ty: Handle, name: *const c_char) -> Handle;
    fn LLVMSetLinkage(value: Handle, linkage: c_int);
    fn LLVMSetGlobalConstant(global: Handle, constant: c_int);
    fn LLVMSetUnnamedAddress(global: Handle, kind: c_int);
    fn LLVMSetInitializer(global: Handle, value: Handle);
    fn LLVMSetAlignment(value: Handle, align: c_uint);
    fn LLVMGetParam(function: Handle, index: c_uint) -> Handle;
    fn LLVMSetValueName2(value: Handle, name: *const c_char, len: usize);
    fn LLVMAppendBasicBlockInContext(
        context: Handle,
        function: Handle,
        name: *const c_char,
    ) -> Handle;
    fn LLVMCreateBuilderInContext(context: Handle) -> Handle;
    fn LLVMDisposeBuilder(builder: Handle);
    fn LLVMPositionBuilderAtEnd(builder: Handle, block: Handle);
    fn LLVMConstInt(ty: Handle, value: u64, sign_extend: c_int) -> Handle;
    fn LLVMConstNull(ty: Handle) -> Handle;
    fn LLVMGetUndef(ty: Handle) -> Handle;
    fn LLVMGetPoison(ty: Handle) -> Handle;
    fn LLVMConstArray2(element: Handle, values: *mut Handle, count: u64) -> Handle;
    fn LLVMConstStringInContext(
        context: Handle,
        data: *const c_char,
        len: c_uint,
        no_nul: c_int,
    ) -> Handle;
    fn LLVMConstGEP2(ty: Handle, pointer: Handle, indices: *mut Handle, count: c_uint) -> Handle;
    fn LLVMBuildAlloca(builder: Handle, ty: Handle, name: *const c_char) -> Handle;
    fn LLVMBuildLoad2(builder: Handle, ty: Handle, pointer: Handle, name: *const c_char) -> Handle;
    fn LLVMBuildStore(builder: Handle, value: Handle, pointer: Handle) -> Handle;
    fn LLVMBuildAdd(builder: Handle, a: Handle, b: Handle, name: *const c_char) -> Handle;
    fn LLVMBuildSub(builder: Handle, a: Handle, b: Handle, name: *const c_char) -> Handle;
    fn LLVMBuildMul(builder: Handle, a: Handle, b: Handle, name: *const c_char) -> Handle;
    fn LLVMBuildSDiv(builder: Handle, a: Handle, b: Handle, name: *const c_char) -> Handle;
    fn LLVMBuildUDiv(builder: Handle, a: Handle, b: Handle, name: *const c_char) -> Handle;
    fn LLVMBuildSRem(builder: Handle, a: Handle, b: Handle, name: *const c_char) -> Handle;
    fn LLVMBuildURem(builder: Handle, a: Handle, b: Handle, name: *const c_char) -> Handle;
    fn LLVMBuildAnd(builder: Handle, a: Handle, b: Handle, name: *const c_char) -> Handle;
    fn LLVMBuildOr(builder: Handle, a: Handle, b: Handle, name: *const c_char) -> Handle;
    fn LLVMBuildXor(builder: Handle, a: Handle, b: Handle, name: *const c_char) -> Handle;
    fn LLVMBuildShl(builder: Handle, a: Handle, b: Handle, name: *const c_char) -> Handle;
    fn LLVMBuildLShr(builder: Handle, a: Handle, b: Handle, name: *const c_char) -> Handle;
    fn LLVMBuildAShr(builder: Handle, a: Handle, b: Handle, name: *const c_char) -> Handle;
    fn LLVMBuildICmp(
        builder: Handle,
        predicate: c_int,
        a: Handle,
        b: Handle,
        name: *const c_char,
    ) -> Handle;
    fn LLVMBuildSExt(builder: Handle, value: Handle, ty: Handle, name: *const c_char) -> Handle;
    fn LLVMBuildZExt(builder: Handle, value: Handle, ty: Handle, name: *const c_char) -> Handle;
    fn LLVMBuildTrunc(builder: Handle, value: Handle, ty: Handle, name: *const c_char) -> Handle;
    fn LLVMBuildPtrToInt(builder: Handle, value: Handle, ty: Handle, name: *const c_char)
    -> Handle;
    fn LLVMBuildGEP2(
        builder: Handle,
        ty: Handle,
        pointer: Handle,
        indices: *mut Handle,
        count: c_uint,
        name: *const c_char,
    ) -> Handle;
    fn LLVMBuildExtractValue(
        builder: Handle,
        value: Handle,
        index: c_uint,
        name: *const c_char,
    ) -> Handle;
    fn LLVMBuildInsertValue(
        builder: Handle,
        aggregate: Handle,
        value: Handle,
        index: c_uint,
        name: *const c_char,
    ) -> Handle;
    fn LLVMBuildCall2(
        builder: Handle,
        ty: Handle,
        callee: Handle,
        args: *mut Handle,
        count: c_uint,
        name: *const c_char,
    ) -> Handle;
    fn LLVMBuildFreeze(builder: Handle, value: Handle, name: *const c_char) -> Handle;
    fn LLVMBuildPhi(builder: Handle, ty: Handle, name: *const c_char) -> Handle;
    fn LLVMAddIncoming(phi: Handle, values: *mut Handle, blocks: *mut Handle, count: c_uint);
    fn LLVMBuildBr(builder: Handle, block: Handle) -> Handle;
    fn LLVMBuildCondBr(builder: Handle, condition: Handle, yes: Handle, no: Handle) -> Handle;
    fn LLVMBuildRet(builder: Handle, value: Handle) -> Handle;
    fn LLVMBuildRetVoid(builder: Handle) -> Handle;
    fn LLVMBuildUnreachable(builder: Handle) -> Handle;
    fn LLVMGetEnumAttributeKindForName(name: *const c_char, len: usize) -> c_uint;
    fn LLVMCreateEnumAttribute(context: Handle, kind: c_uint, value: u64) -> Handle;
    fn LLVMAddAttributeAtIndex(function: Handle, index: c_uint, attribute: Handle);
    fn LLVMAddCallSiteAttribute(call: Handle, index: c_uint, attribute: Handle);
}
owned!(Builder, LLVMDisposeBuilder);

fn cstring(s: &str) -> Result<CString, Error> {
    CString::new(s).map_err(|_| error(ErrorKind::Verify, "NUL in machine symbol"))
}
fn linkage(link: m::Linkage) -> c_int {
    // LLVMLinkage from llvmorg-23.1.4/llvm/include/llvm-c/Core.h.
    match link {
        m::Linkage::External => 0,
        m::Linkage::AvailableExternally => 1,
        m::Linkage::Internal => 8,
        m::Linkage::Private => 9,
    }
}
struct Construction<'a> {
    context: Handle,
    module: &'a Module,
    builder: Builder,
    types: BTreeMap<m::Ty, Handle>,
    globals: BTreeMap<&'a str, Handle>,
    function_types: BTreeMap<&'a str, Handle>,
    locals: BTreeMap<&'a str, Handle>,
    blocks: BTreeMap<&'a str, Handle>,
}
impl<'a> Construction<'a> {
    fn ty(&mut self, ty: &m::Ty) -> Result<Handle, Error> {
        if let Some(&raw) = self.types.get(ty) {
            return Ok(raw);
        }
        // SAFETY: preflight checked the finite type graph and array/field counts.
        // All child handles belong to this context, which outlives this adapter.
        let raw = unsafe {
            match ty {
                m::Ty::Void => LLVMVoidTypeInContext(self.context),
                m::Ty::Int(bits) => LLVMIntTypeInContext(self.context, *bits),
                m::Ty::Ptr => LLVMPointerTypeInContext(self.context, 0),
                m::Ty::Array(n, t) => LLVMArrayType2(self.ty(t)?, *n),
                m::Ty::Struct(fields) => {
                    let mut fields = fields
                        .iter()
                        .map(|t| self.ty(t))
                        .collect::<Result<Vec<_>, _>>()?;
                    LLVMStructTypeInContext(
                        self.context,
                        fields.as_mut_ptr(),
                        fields.len() as u32,
                        0,
                    )
                }
                m::Ty::Named(n) => {
                    return Err(error(ErrorKind::Verify, format!("missing named type {n}")));
                }
            }
        };
        if raw.is_null() {
            return Err(error(ErrorKind::Emit, "LLVM type construction failed"));
        }
        self.types.insert(ty.clone(), raw);
        Ok(raw)
    }
    fn value(&mut self, v: &m::Val, ty: &m::Ty) -> Result<Handle, Error> {
        // SAFETY: preflight established each operand's type and scope, all names
        // resolve in this context, and constants contain no local references.
        unsafe {
            Ok(match v {
                m::Val::Local(n) => *self
                    .locals
                    .get(n.as_str())
                    .ok_or_else(|| error(ErrorKind::Verify, format!("unbuilt SSA %{n}")))?,
                m::Val::Global(n) => *self
                    .globals
                    .get(n.as_str())
                    .ok_or_else(|| error(ErrorKind::Verify, format!("unbuilt symbol @{n}")))?,
                m::Val::Int(n) => {
                    let m::Ty::Int(bits @ (1 | 8 | 16 | 32 | 64)) = ty else {
                        return Err(error(ErrorKind::Verify, "integer constant type mismatch"));
                    };
                    // LLVM 23's APInt constructor requires a canonical bit
                    // pattern when SignExtend is false. Passing a negative i32
                    // as full-width u64 leaves hidden high bits: printing hides
                    // them, but IPSCCP can then mismerge distinct return values.
                    let value = (*n as u64) & (u64::MAX >> (64 - bits));
                    LLVMConstInt(self.ty(ty)?, value, 0)
                }
                m::Val::Bool(b) => LLVMConstInt(self.ty(ty)?, u64::from(*b), 0),
                m::Val::Null | m::Val::Zero => LLVMConstNull(self.ty(ty)?),
                m::Val::Undef => LLVMGetUndef(self.ty(ty)?),
                m::Val::Poison => LLVMGetPoison(self.ty(ty)?),
                m::Val::Bytes(bytes) => LLVMConstStringInContext(
                    self.context,
                    bytes.as_ptr().cast(),
                    bytes.len() as u32,
                    1,
                ),
                m::Val::Array(elements) => {
                    let m::Ty::Array(_, element) = ty else {
                        return Err(error(ErrorKind::Verify, "array type mismatch"));
                    };
                    let element = self.ty(element)?;
                    let mut values = elements
                        .iter()
                        .map(|a| self.value(&a.value, &a.ty))
                        .collect::<Result<Vec<_>, _>>()?;
                    LLVMConstArray2(element, values.as_mut_ptr(), values.len() as u64)
                }
                m::Val::Gep { ty, ptr, indices } => {
                    let source = self.ty(ty)?;
                    let pointer = self.value(ptr, &m::Ty::Ptr)?;
                    let mut values = indices
                        .iter()
                        .map(|a| self.value(&a.value, &a.ty))
                        .collect::<Result<Vec<_>, _>>()?;
                    LLVMConstGEP2(source, pointer, values.as_mut_ptr(), values.len() as u32)
                }
                m::Val::Invalid(_) => return Err(error(ErrorKind::Verify, "invalid machine atom")),
            })
        }
    }
    fn signature(&mut self, sig: &m::Signature) -> Result<Handle, Error> {
        let result = self.ty(&sig.result)?;
        let mut params = sig
            .params
            .iter()
            .map(|p| self.ty(&p.ty))
            .collect::<Result<Vec<_>, _>>()?;
        // SAFETY: supported return/parameter types and arity were checked.
        Ok(unsafe {
            LLVMFunctionType(
                result,
                params.as_mut_ptr(),
                params.len() as u32,
                i32::from(sig.variadic),
            )
        })
    }
    fn attributes(
        &self,
        value: Handle,
        index: u32,
        attrs: &[m::Attr],
        call: bool,
    ) -> Result<(), Error> {
        for a in attrs {
            let n = a.name();
            // SAFETY: static attribute name and a live function/call handle from
            // this module; preflight checked the attribute position and type.
            unsafe {
                let kind = LLVMGetEnumAttributeKindForName(n.as_ptr().cast(), n.len());
                if kind == 0 {
                    return Err(error(
                        ErrorKind::Version,
                        format!("unknown LLVM attribute {n}"),
                    ));
                }
                let attr = LLVMCreateEnumAttribute(self.context, kind, 0);
                if call {
                    LLVMAddCallSiteAttribute(value, index, attr);
                } else {
                    LLVMAddAttributeAtIndex(value, index, attr);
                }
            }
        }
        Ok(())
    }
    fn emit(&mut self, inst: &'a m::Inst) -> Result<Handle, Error> {
        let name = inst.result.as_deref().map(cstring).transpose()?;
        let n = name.as_ref().map_or(c"".as_ptr(), |name| name.as_ptr());
        let b = self.builder.0;
        // SAFETY: the whole module has passed type/SSA/CFG preflight. The
        // dependency schedule makes operands available before use. Index and
        // attribute bounds were checked before calling assert-prone LLVM APIs.
        let out = unsafe {
            match &inst.op {
                m::Op::Alloca { ty, align } => {
                    let v = LLVMBuildAlloca(b, self.ty(ty)?, n);
                    if *align != 0 {
                        LLVMSetAlignment(v, *align);
                    }
                    v
                }
                m::Op::Load { ty, ptr, align } => {
                    let t = self.ty(ty)?;
                    let p = self.value(ptr, &m::Ty::Ptr)?;
                    let v = LLVMBuildLoad2(b, t, p, n);
                    if *align != 0 {
                        LLVMSetAlignment(v, *align);
                    }
                    v
                }
                m::Op::Store { arg, ptr, align } => {
                    let v = self.value(&arg.value, &arg.ty)?;
                    let p = self.value(ptr, &m::Ty::Ptr)?;
                    let inst = LLVMBuildStore(b, v, p);
                    if *align != 0 {
                        LLVMSetAlignment(inst, *align);
                    }
                    inst
                }
                m::Op::Binary {
                    opcode,
                    ty,
                    left,
                    right,
                } => {
                    let l = self.value(left, ty)?;
                    let r = self.value(right, ty)?;
                    match opcode.as_str() {
                        "add" => LLVMBuildAdd(b, l, r, n),
                        "sub" => LLVMBuildSub(b, l, r, n),
                        "mul" => LLVMBuildMul(b, l, r, n),
                        "sdiv" => LLVMBuildSDiv(b, l, r, n),
                        "udiv" => LLVMBuildUDiv(b, l, r, n),
                        "srem" => LLVMBuildSRem(b, l, r, n),
                        "urem" => LLVMBuildURem(b, l, r, n),
                        "and" => LLVMBuildAnd(b, l, r, n),
                        "or" => LLVMBuildOr(b, l, r, n),
                        "xor" => LLVMBuildXor(b, l, r, n),
                        "shl" => LLVMBuildShl(b, l, r, n),
                        "lshr" => LLVMBuildLShr(b, l, r, n),
                        "ashr" => LLVMBuildAShr(b, l, r, n),
                        _ => return Err(error(ErrorKind::Verify, "unknown binary opcode")),
                    }
                }
                m::Op::Compare {
                    predicate,
                    ty,
                    left,
                    right,
                } => {
                    let p = match predicate.as_str() {
                        "eq" => 32,
                        "ne" => 33,
                        "ugt" => 34,
                        "uge" => 35,
                        "ult" => 36,
                        "ule" => 37,
                        "sgt" => 38,
                        "sge" => 39,
                        "slt" => 40,
                        "sle" => 41,
                        _ => return Err(error(ErrorKind::Verify, "unknown predicate")),
                    };
                    LLVMBuildICmp(b, p, self.value(left, ty)?, self.value(right, ty)?, n)
                }
                m::Op::Cast { opcode, arg, to } => {
                    let value = self.value(&arg.value, &arg.ty)?;
                    let ty = self.ty(to)?;
                    match opcode.as_str() {
                        "sext" => LLVMBuildSExt(b, value, ty, n),
                        "zext" => LLVMBuildZExt(b, value, ty, n),
                        "trunc" => LLVMBuildTrunc(b, value, ty, n),
                        "ptrtoint" => LLVMBuildPtrToInt(b, value, ty, n),
                        _ => return Err(error(ErrorKind::Verify, "unknown cast")),
                    }
                }
                m::Op::Gep { ty, ptr, indices } => {
                    let t = self.ty(ty)?;
                    let p = self.value(ptr, &m::Ty::Ptr)?;
                    let mut ix = indices
                        .iter()
                        .map(|a| self.value(&a.value, &a.ty))
                        .collect::<Result<Vec<_>, _>>()?;
                    LLVMBuildGEP2(b, t, p, ix.as_mut_ptr(), ix.len() as u32, n)
                }
                m::Op::Extract { arg, index } => {
                    LLVMBuildExtractValue(b, self.value(&arg.value, &arg.ty)?, *index, n)
                }
                m::Op::Insert { arg, field, index } => {
                    let a = self.value(&arg.value, &arg.ty)?;
                    let v = self.value(&field.value, &field.ty)?;
                    LLVMBuildInsertValue(b, a, v, *index, n)
                }
                m::Op::Freeze(arg) => LLVMBuildFreeze(b, self.value(&arg.value, &arg.ty)?, n),
                m::Op::Call {
                    result,
                    result_attrs,
                    callee,
                    args,
                    vararg,
                } => {
                    // Direct calls reuse the exact declared LLVM function type.
                    // Machine preflight has already checked every argument and ABI attribute.
                    let t = if let m::Val::Global(name) = callee {
                        *self
                            .function_types
                            .get(name.as_str())
                            .ok_or_else(|| error(ErrorKind::Verify, "missing declared call type"))?
                    } else {
                        let result = self.ty(result)?;
                        let mut params = if let Some(types) = vararg {
                            types
                                .iter()
                                .map(|t| self.ty(t))
                                .collect::<Result<Vec<_>, _>>()?
                        } else {
                            args.iter()
                                .map(|a| self.ty(&a.ty))
                                .collect::<Result<Vec<_>, _>>()?
                        };
                        LLVMFunctionType(
                            result,
                            params.as_mut_ptr(),
                            params.len() as u32,
                            i32::from(vararg.is_some()),
                        )
                    };
                    let callee = self.value(callee, &m::Ty::Ptr)?;
                    let mut values = args
                        .iter()
                        .map(|a| self.value(&a.value, &a.ty))
                        .collect::<Result<Vec<_>, _>>()?;
                    let call =
                        LLVMBuildCall2(b, t, callee, values.as_mut_ptr(), values.len() as u32, n);
                    self.attributes(call, 0, result_attrs, true)?;
                    for (i, a) in args.iter().enumerate() {
                        self.attributes(call, (i + 1) as u32, &a.attrs, true)?;
                    }
                    call
                }
                m::Op::Phi { ty, .. } => LLVMBuildPhi(b, self.ty(ty)?, n),
                m::Op::Branch(target) => LLVMBuildBr(b, self.blocks[target.as_str()]),
                m::Op::CondBranch { condition, yes, no } => LLVMBuildCondBr(
                    b,
                    self.value(condition, &m::Ty::Int(1))?,
                    self.blocks[yes.as_str()],
                    self.blocks[no.as_str()],
                ),
                m::Op::Return(None) => LLVMBuildRetVoid(b),
                m::Op::Return(Some(a)) => LLVMBuildRet(b, self.value(&a.value, &a.ty)?),
                m::Op::Unreachable => LLVMBuildUnreachable(b),
            }
        };
        if out.is_null() {
            return Err(error(ErrorKind::Emit, "LLVMBuilder returned NULL"));
        }
        if let Some(name) = &inst.result {
            self.locals.insert(name.as_str(), out);
        }
        Ok(out)
    }
    fn function(
        &mut self,
        f: &'a m::Function,
        schedule: &[(usize, usize)],
        cancelled: &(dyn Fn() -> bool + Sync),
    ) -> Result<(), Error> {
        let Some(blocks) = &f.blocks else {
            return Ok(());
        };
        self.locals.clear();
        self.blocks.clear();
        let function = self.globals[f.name.as_str()];
        for (i, p) in f.signature.params.iter().enumerate() {
            let name = p
                .name
                .as_deref()
                .ok_or_else(|| error(ErrorKind::Verify, "parameter name absent"))?;
            // SAFETY: signature has this parameter and name is a checked atom.
            let value = unsafe { LLVMGetParam(function, i as u32) };
            unsafe {
                LLVMSetValueName2(value, name.as_ptr().cast(), name.len());
            }
            self.locals.insert(name, value);
        }
        for block in blocks {
            let name = cstring(&block.name)?;
            let raw =
                unsafe { LLVMAppendBasicBlockInContext(self.context, function, name.as_ptr()) };
            if raw.is_null() {
                return Err(error(ErrorKind::Emit, "LLVM block creation failed"));
            }
            self.blocks.insert(block.name.as_str(), raw);
        }
        let mut phis = vec![];
        for block in blocks {
            unsafe {
                LLVMPositionBuilderAtEnd(self.builder.0, self.blocks[block.name.as_str()]);
            }
            for inst in &block.instructions {
                if let m::Op::Phi { ty, incoming } = &inst.op {
                    let raw = self.emit(inst)?;
                    phis.push((raw, ty, incoming));
                } else {
                    break;
                }
            }
        }
        for (step, &(b, j)) in schedule.iter().enumerate() {
            if step % 256 == 0 {
                check(cancelled)?;
            }
            unsafe {
                LLVMPositionBuilderAtEnd(self.builder.0, self.blocks[blocks[b].name.as_str()]);
            }
            self.emit(&blocks[b].instructions[j])?;
        }
        // Back-edge operands can only be resolved after all definitions exist.
        for (phi, ty, incoming) in phis {
            let mut values = vec![];
            let mut predecessors = vec![];
            for (v, block) in incoming {
                values.push(self.value(v, ty)?);
                predecessors.push(self.blocks[block.as_str()]);
            }
            unsafe {
                LLVMAddIncoming(
                    phi,
                    values.as_mut_ptr(),
                    predecessors.as_mut_ptr(),
                    values.len() as u32,
                );
            }
        }
        Ok(())
    }
}

pub(super) fn build(
    context: Handle,
    program: &m::Module,
    layout: Handle,
    schedule: &Schedule,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Module, Error> {
    // SAFETY: context/layout belong to the parent invocation. Module and
    // Builder have exclusive RAII ownership and are dropped before context.
    let module =
        Module(unsafe { LLVMModuleCreateWithNameInContext(c"zil-unit".as_ptr(), context) });
    if module.0.is_null() {
        return Err(error(ErrorKind::Emit, "LLVM module creation failed"));
    }
    let triple = cstring(&program.triple)?;
    unsafe {
        LLVMSetTarget(module.0, triple.as_ptr());
        LLVMSetModuleDataLayout(module.0, layout);
    }
    let builder = Builder(unsafe { LLVMCreateBuilderInContext(context) });
    if builder.0.is_null() {
        return Err(error(ErrorKind::Emit, "LLVM builder creation failed"));
    }
    let mut state = Construction {
        context,
        module: &module,
        builder,
        types: BTreeMap::new(),
        globals: BTreeMap::new(),
        function_types: BTreeMap::new(),
        locals: BTreeMap::new(),
        blocks: BTreeMap::new(),
    };
    for item in &program.items {
        if let m::Item::NamedType(name, _) = item {
            let c = cstring(name)?;
            let ty = unsafe { LLVMStructCreateNamed(context, c.as_ptr()) };
            if ty.is_null() {
                return Err(error(ErrorKind::Emit, "named LLVM type failed"));
            }
            state.types.insert(m::Ty::Named(name.clone()), ty);
        }
    }
    for item in &program.items {
        if let m::Item::NamedType(name, fields) = item {
            let ty = state.types[&m::Ty::Named(name.clone())];
            let mut fields = fields
                .iter()
                .map(|f| state.ty(f))
                .collect::<Result<Vec<_>, _>>()?;
            unsafe {
                LLVMStructSetBody(ty, fields.as_mut_ptr(), fields.len() as u32, 0);
            }
        }
    }
    for item in &program.items {
        check(cancelled)?;
        match item {
            m::Item::Function(f) => {
                let name = cstring(&f.name)?;
                let ty = state.signature(&f.signature)?;
                let raw = unsafe { LLVMAddFunction(state.module.0, name.as_ptr(), ty) };
                unsafe {
                    LLVMSetLinkage(raw, linkage(f.linkage));
                }
                state.attributes(raw, u32::MAX, &f.attrs, false)?;
                state.attributes(raw, 0, &f.signature.result_attrs, false)?;
                for (i, p) in f.signature.params.iter().enumerate() {
                    state.attributes(raw, (i + 1) as u32, &p.attrs, false)?;
                }
                state.globals.insert(f.name.as_str(), raw);
                state.function_types.insert(f.name.as_str(), ty);
            }
            m::Item::Global(g) => {
                let name = cstring(&g.name)?;
                let ty = state.ty(&g.ty)?;
                let raw = unsafe { LLVMAddGlobal(state.module.0, ty, name.as_ptr()) };
                unsafe {
                    LLVMSetLinkage(raw, linkage(g.linkage));
                    LLVMSetGlobalConstant(raw, i32::from(g.constant));
                    if g.unnamed_addr {
                        LLVMSetUnnamedAddress(raw, 2);
                    }
                    if g.alignment != 0 {
                        LLVMSetAlignment(raw, g.alignment);
                    }
                }
                state.globals.insert(g.name.as_str(), raw);
            }
            _ => {}
        }
    }
    for item in &program.items {
        if let m::Item::Global(g) = item
            && let Some(value) = &g.initializer
        {
            let value = state.value(value, &g.ty)?;
            unsafe {
                LLVMSetInitializer(state.globals[g.name.as_str()], value);
            }
        }
    }
    for item in &program.items {
        if let m::Item::Function(f) = item {
            check(cancelled)?;
            state.function(f, &schedule.functions[&f.name], cancelled)?;
        }
    }
    drop(state);
    Ok(module)
}
