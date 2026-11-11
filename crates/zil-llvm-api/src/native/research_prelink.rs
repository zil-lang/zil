//! Research-only manual prelink reuse. Ordinary compile_input is unchanged.
//! Same pinned LLVM instance, full O2, pure direct-use scope, no native cache.
use super::*;
use crate::research_prelink::{PrelinkedUnit, ResearchObject, ResearchReceipt, UnitFacts};
use std::collections::{BTreeMap, BTreeSet};

unsafe extern "C" {
    fn LLVMCreateMemoryBufferWithMemoryRangeCopy(
        data: *const c_char,
        len: usize,
        name: *const c_char,
    ) -> Handle;
    fn LLVMWriteBitcodeToMemoryBuffer(module: Handle) -> Handle;
    fn LLVMParseBitcodeInContext2(context: Handle, buffer: Handle, module: *mut Handle) -> c_int;
    fn LLVMLinkModules2(dest: Handle, source: Handle) -> c_int;
    fn LLVMGetFirstFunction(module: Handle) -> Handle;
    fn LLVMGetNextFunction(function: Handle) -> Handle;
    fn LLVMGetValueName2(value: Handle, len: *mut usize) -> *const c_char;
    fn LLVMGetNamedFunctionWithLength(module: Handle, name: *const c_char, len: usize) -> Handle;
    fn LLVMIsDeclaration(value: Handle) -> c_int;
    fn LLVMGetLinkage(value: Handle) -> c_int;
    fn LLVMSetLinkage(value: Handle, linkage: c_int);
    fn LLVMSetValueName2(value: Handle, name: *const c_char, len: usize);
    fn LLVMGlobalGetValueType(value: Handle) -> Handle;
    fn LLVMPrintTypeToString(ty: Handle) -> *mut c_char;
    fn LLVMGetIntrinsicID(function: Handle) -> c_uint;
    fn LLVMGetFirstGlobal(module: Handle) -> Handle;
    fn LLVMGetFirstGlobalAlias(module: Handle) -> Handle;
    fn LLVMGetFirstGlobalIFunc(module: Handle) -> Handle;
    fn LLVMGetComdat(value: Handle) -> Handle;
    fn LLVMHasPersonalityFn(function: Handle) -> c_int;
    fn LLVMHasPrefixData(function: Handle) -> c_int;
    fn LLVMHasPrologueData(function: Handle) -> c_int;
    fn LLVMGetFirstBasicBlock(function: Handle) -> Handle;
    fn LLVMGetNextBasicBlock(block: Handle) -> Handle;
    fn LLVMGetFirstInstruction(block: Handle) -> Handle;
    fn LLVMGetNextInstruction(instruction: Handle) -> Handle;
    fn LLVMIsACallInst(value: Handle) -> Handle;
    fn LLVMIsAInvokeInst(value: Handle) -> Handle;
    fn LLVMIsACallBrInst(value: Handle) -> Handle;
    fn LLVMIsAFunction(value: Handle) -> Handle;
    fn LLVMGetCalledValue(call: Handle) -> Handle;
    fn LLVMGetCalledFunctionType(call: Handle) -> Handle;
    fn LLVMGetNumArgOperands(call: Handle) -> c_uint;
    fn LLVMGetNumOperandBundles(call: Handle) -> c_uint;
    fn LLVMGetNumOperands(value: Handle) -> c_int;
    fn LLVMGetFirstUse(value: Handle) -> Handle;
    fn LLVMGetNextUse(use_: Handle) -> Handle;
    fn LLVMGetUser(use_: Handle) -> Handle;
    fn LLVMGetOperandUse(value: Handle, index: c_uint) -> Handle;
    fn LLVMGetFirstNamedMetadata(module: Handle) -> Handle;
    fn LLVMGetNextNamedMetadata(metadata: Handle) -> Handle;
    fn LLVMGetNamedMetadataName(metadata: Handle, len: *mut usize) -> *const c_char;
    fn LLVMGetNamedMetadataNumOperands(module: Handle, name: *const c_char) -> c_uint;
    fn LLVMGetNamedMetadataOperands(module: Handle, name: *const c_char, dest: *mut Handle);
    fn LLVMGlobalCopyAllMetadata(value: Handle, len: *mut usize) -> Handle;
    fn LLVMInstructionGetAllMetadataOtherThanDebugLoc(value: Handle, len: *mut usize) -> Handle;
    fn LLVMValueMetadataEntriesGetMetadata(entries: Handle, index: c_uint) -> Handle;
    fn LLVMDisposeValueMetadataEntries(entries: Handle);
    fn LLVMMetadataAsValue(context: Handle, metadata: Handle) -> Handle;
    fn LLVMIsAMDNode(value: Handle) -> Handle;
    fn LLVMIsAMDString(value: Handle) -> Handle;
    fn LLVMIsAConstantInt(value: Handle) -> Handle;
    fn LLVMIsAConstantFP(value: Handle) -> Handle;
    fn LLVMGetMDNodeNumOperands(value: Handle) -> c_uint;
    fn LLVMGetMDNodeOperands(value: Handle, dest: *mut Handle);
}

fn reject(message: impl Into<String>) -> Error {
    error(ErrorKind::Verify, message)
}
fn json(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
fn names_json(names: &BTreeSet<String>) -> String {
    format!(
        "[{}]",
        names.iter().map(|s| json(s)).collect::<Vec<_>>().join(",")
    )
}
fn calls_json(calls: &BTreeMap<String, Vec<String>>) -> String {
    format!(
        "{{{}}}",
        calls
            .iter()
            .map(|(n, targets)| format!(
                "{}:[{}]",
                json(n),
                targets
                    .iter()
                    .map(|t| json(t))
                    .collect::<Vec<_>>()
                    .join(",")
            ))
            .collect::<Vec<_>>()
            .join(",")
    )
}
fn name(value: Handle) -> Result<String, Error> {
    let mut len = 0;
    let p = unsafe { LLVMGetValueName2(value, &mut len) };
    if p.is_null() || len == 0 || len > isize::MAX as usize {
        return Err(reject("missing LLVM name"));
    }
    let bytes = unsafe { std::slice::from_raw_parts(p.cast::<u8>(), len) };
    String::from_utf8(bytes.to_vec()).map_err(|_| reject("non-UTF8 LLVM name"))
}
fn signature(f: Handle) -> String {
    unsafe { take_message(LLVMPrintTypeToString(LLVMGlobalGetValueType(f))) }
}
fn find(module: &Module, n: &str) -> Handle {
    unsafe { LLVMGetNamedFunctionWithLength(module.0, n.as_ptr().cast(), n.len()) }
}
fn guard(config: &Config<'_>, merging: FunctionMerging) -> Result<(), Error> {
    check_config(config)?;
    if config.triple != "x86_64-unknown-linux-gnu"
        || config.optimization != Optimization::O2
        || merging != FunctionMerging::AfterOptimization
    {
        return Err(reject(
            "research prelink requires Linux x64 O2 after-optimization merging",
        ));
    }
    Ok(())
}
// Fields drop in this order; all modules are declared after Environment and die
// first. Context/target/module pointers never leave the current worker thread.
struct Environment {
    machine: Machine,
    layout: Layout,
    context: Context,
    data_layout: String,
}
impl Environment {
    fn new(config: &Config<'_>) -> Result<Self, Error> {
        INIT.call_once(|| unsafe {
            LLVMInitializeX86TargetInfo();
            LLVMInitializeX86Target();
            LLVMInitializeX86TargetMC();
            LLVMInitializeX86AsmPrinter();
            LLVMInitializeX86AsmParser();
        });
        let context = Context(unsafe { LLVMContextCreate() });
        if context.0.is_null() {
            return Err(reject("context creation failed"));
        }
        let triple = CString::new(config.triple).map_err(|_| reject("invalid triple"))?;
        let mut target = ptr::null_mut();
        let mut msg = ptr::null_mut();
        let failed = unsafe { LLVMGetTargetFromTriple(triple.as_ptr(), &mut target, &mut msg) };
        let message = unsafe { take_message(msg) };
        if failed != 0 || target.is_null() {
            return Err(error(ErrorKind::Target, message));
        }
        let machine = Machine(unsafe {
            LLVMCreateTargetMachine(
                target,
                triple.as_ptr(),
                c"x86-64".as_ptr(),
                c"".as_ptr(),
                2,
                2,
                0,
            )
        });
        if machine.0.is_null() {
            return Err(reject("target machine creation failed"));
        }
        let layout = Layout(unsafe { LLVMCreateTargetDataLayout(machine.0) });
        if layout.0.is_null()
            || unsafe { LLVMPointerSize(layout.0) != 8 || LLVMByteOrder(layout.0) != 1 }
        {
            return Err(reject("invalid checked x64 layout"));
        }
        let data_layout = unsafe { take_message(LLVMCopyStringRepOfTargetData(layout.0)) };
        Ok(Self {
            machine,
            layout,
            context,
            data_layout,
        })
    }
    fn target(&self, module: &Module, triple: &str) -> Result<(), Error> {
        if unsafe { text(LLVMGetTarget(module.0)) } != triple {
            return Err(reject("module triple mismatch"));
        }
        let layout = unsafe { text(LLVMGetDataLayoutStr(module.0)) };
        if !layout.is_empty() && layout != self.data_layout {
            return Err(reject("module layout mismatch"));
        }
        unsafe { LLVMSetModuleDataLayout(module.0, self.layout.0) };
        Ok(())
    }
    fn passes(&self, module: &Module, pipeline: &std::ffi::CStr) -> Result<(), Error> {
        let opts = PassOptions(unsafe { LLVMCreatePassBuilderOptions() });
        if opts.0.is_null() {
            return Err(reject("pass options creation failed"));
        }
        unsafe {
            LLVMPassBuilderOptionsSetVerifyEach(opts.0, 0);
            LLVMPassBuilderOptionsSetLoopInterleaving(opts.0, 1);
            LLVMPassBuilderOptionsSetLoopVectorization(opts.0, 1);
            LLVMPassBuilderOptionsSetSLPVectorization(opts.0, 1);
            LLVMPassBuilderOptionsSetMergeFunctions(opts.0, 1);
        }
        let e = unsafe { LLVMRunPasses(module.0, pipeline.as_ptr(), self.machine.0, opts.0) };
        if e.is_null() {
            return Ok(());
        }
        let m = unsafe { LLVMGetErrorMessage(e) };
        let message = unsafe { text(m) };
        if !m.is_null() {
            unsafe { LLVMDisposeErrorMessage(m) };
        }
        Err(error(ErrorKind::Optimize, message))
    }
    fn parse(&self, bytes: &[u8]) -> Result<Module, Error> {
        let buffer = Buffer(unsafe {
            LLVMCreateMemoryBufferWithMemoryRangeCopy(
                bytes.as_ptr().cast(),
                bytes.len(),
                c"zil-prelinked-unit".as_ptr(),
            )
        });
        if buffer.0.is_null() {
            return Err(reject("bitcode buffer allocation failed"));
        }
        let mut raw = ptr::null_mut();
        // Eager BitReader borrows, unlike ParseIR and the lazy BitReader API.
        let failed = unsafe { LLVMParseBitcodeInContext2(self.context.0, buffer.0, &mut raw) };
        let module = Module(raw);
        if failed != 0 || module.0.is_null() {
            return Err(error(
                ErrorKind::Parse,
                "current prelink bitcode parse failed",
            ));
        }
        verify(&module)?;
        Ok(module)
    }
}

fn metadata_value(v: Handle, visited: &mut BTreeSet<usize>) -> Result<(), Error> {
    if v.is_null() || !visited.insert(v as usize) {
        return Ok(());
    }
    if unsafe {
        !LLVMIsAMDString(v).is_null()
            || !LLVMIsAConstantInt(v).is_null()
            || !LLVMIsAConstantFP(v).is_null()
    } {
        return Ok(());
    }
    if unsafe { LLVMIsAMDNode(v).is_null() } {
        return Err(reject("non-scalar value escapes through metadata"));
    }
    let n = unsafe { LLVMGetMDNodeNumOperands(v) };
    let mut operands = vec![ptr::null_mut(); n as usize];
    unsafe { LLVMGetMDNodeOperands(v, operands.as_mut_ptr()) };
    for value in operands {
        metadata_value(value, visited)?;
    }
    Ok(())
}
fn metadata_entries(ctx: Handle, entries: Handle, count: usize) -> Result<(), Error> {
    // The opaque entry array is LLVM owned and must be disposed even on reject.
    struct Entries(Handle);
    impl Drop for Entries {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { LLVMDisposeValueMetadataEntries(self.0) };
            }
        }
    }
    let entries = Entries(entries);
    let mut visited = BTreeSet::new();
    if count > c_uint::MAX as usize {
        return Err(reject("metadata entry count overflow"));
    }
    for i in 0..count {
        let md = unsafe { LLVMValueMetadataEntriesGetMetadata(entries.0, i as c_uint) };
        metadata_value(unsafe { LLVMMetadataAsValue(ctx, md) }, &mut visited)?;
    }
    Ok(())
}
fn direct_use(f: Handle, use_: Handle) -> Result<(), Error> {
    let call = unsafe { LLVMGetUser(use_) };
    if unsafe { LLVMIsACallInst(call).is_null() || LLVMGetNumOperandBundles(call) != 0 } {
        return Err(reject("function has non-direct-callee use"));
    }
    let n = unsafe { LLVMGetNumArgOperands(call) };
    let count = n
        .checked_add(1)
        .ok_or_else(|| reject("call operand count overflow"))?;
    if count > c_int::MAX as u32
        || unsafe {
            LLVMGetNumOperands(call) != count as c_int
                || LLVMGetCalledValue(call) != f
                || LLVMGetOperandUse(call, n) != use_
                || LLVMGetCalledFunctionType(call) != LLVMGlobalGetValueType(f)
        }
    {
        return Err(reject("call Use role or exact signature mismatch"));
    }
    Ok(())
}
struct Scan {
    defs: BTreeSet<String>,
    decls: BTreeSet<String>,
    calls: BTreeMap<String, Vec<String>>,
    signatures: BTreeMap<String, String>,
    intrinsics: BTreeSet<String>,
}
fn scan(ctx: Handle, module: &Module) -> Result<Scan, Error> {
    if unsafe {
        !LLVMGetFirstGlobal(module.0).is_null()
            || !LLVMGetFirstGlobalAlias(module.0).is_null()
            || !LLVMGetFirstGlobalIFunc(module.0).is_null()
    } {
        return Err(reject("global/alias/ifunc outside pure research scope"));
    }
    let mut md = unsafe { LLVMGetFirstNamedMetadata(module.0) };
    while !md.is_null() {
        let mut len = 0;
        let p = unsafe { LLVMGetNamedMetadataName(md, &mut len) };
        if p.is_null() || len > isize::MAX as usize {
            return Err(reject("invalid named metadata"));
        }
        let n = CString::new(unsafe { std::slice::from_raw_parts(p.cast::<u8>(), len) })
            .map_err(|_| reject("invalid metadata name"))?;
        let count = unsafe { LLVMGetNamedMetadataNumOperands(module.0, n.as_ptr()) };
        let mut operands = vec![ptr::null_mut(); count as usize];
        unsafe { LLVMGetNamedMetadataOperands(module.0, n.as_ptr(), operands.as_mut_ptr()) };
        let mut visited = BTreeSet::new();
        for v in operands {
            metadata_value(v, &mut visited)?;
        }
        md = unsafe { LLVMGetNextNamedMetadata(md) };
    }
    let mut out = Scan {
        defs: BTreeSet::new(),
        decls: BTreeSet::new(),
        calls: BTreeMap::new(),
        signatures: BTreeMap::new(),
        intrinsics: BTreeSet::new(),
    };
    let mut f = unsafe { LLVMGetFirstFunction(module.0) };
    while !f.is_null() {
        let n = name(f)?;
        if out.signatures.insert(n.clone(), signature(f)).is_some() {
            return Err(reject("duplicate function name"));
        }
        if unsafe { LLVMIsDeclaration(f) } != 0 {
            out.decls.insert(n.clone());
        } else {
            out.defs.insert(n.clone());
        }
        if unsafe { LLVMGetIntrinsicID(f) } != 0 {
            out.intrinsics.insert(n.clone());
        }
        if unsafe {
            !LLVMGetComdat(f).is_null()
                || LLVMHasPersonalityFn(f) != 0
                || LLVMHasPrefixData(f) != 0
                || LLVMHasPrologueData(f) != 0
        } {
            return Err(reject(
                "COMDAT/personality/prefix/prologue outside pure scope",
            ));
        }
        let mut u = unsafe { LLVMGetFirstUse(f) };
        while !u.is_null() {
            direct_use(f, u)?;
            u = unsafe { LLVMGetNextUse(u) };
        }
        let mut count = 0;
        let entries = unsafe { LLVMGlobalCopyAllMetadata(f, &mut count) };
        metadata_entries(ctx, entries, count)?;
        let mut calls = BTreeSet::new();
        let mut block = unsafe { LLVMGetFirstBasicBlock(f) };
        while !block.is_null() {
            let mut inst = unsafe { LLVMGetFirstInstruction(block) };
            while !inst.is_null() {
                let mut count = 0;
                let entries =
                    unsafe { LLVMInstructionGetAllMetadataOtherThanDebugLoc(inst, &mut count) };
                metadata_entries(ctx, entries, count)?;
                if unsafe {
                    !LLVMIsAInvokeInst(inst).is_null() || !LLVMIsACallBrInst(inst).is_null()
                } {
                    return Err(reject("invoke/callbr outside pure scope"));
                }
                if unsafe { !LLVMIsACallInst(inst).is_null() } {
                    let callee = unsafe { LLVMGetCalledValue(inst) };
                    if unsafe {
                        LLVMIsAFunction(callee).is_null()
                            || LLVMGetNumOperandBundles(inst) != 0
                            || LLVMGetCalledFunctionType(inst) != LLVMGlobalGetValueType(callee)
                    } {
                        return Err(reject("indirect/bundled/signature-mismatched call"));
                    }
                    calls.insert(name(callee)?);
                }
                inst = unsafe { LLVMGetNextInstruction(inst) };
            }
            block = unsafe { LLVMGetNextBasicBlock(block) };
        }
        if out.defs.contains(&n) {
            out.calls.insert(n, calls.into_iter().collect());
        }
        f = unsafe { LLVMGetNextFunction(f) };
    }
    Ok(out)
}

fn check_sealed(ctx: Handle, module: &Module, p: &PrelinkedUnit) -> Result<(), Error> {
    let actual = scan(ctx, module)?;
    if actual.defs.iter().cloned().collect::<Vec<_>>() != p.facts.definitions
        || actual.decls.iter().cloned().collect::<Vec<_>>() != p.facts.declarations
        || actual.calls != p.facts.direct_calls
        || actual.signatures != p.signatures
        || actual.intrinsics != p.intrinsics
    {
        return Err(reject(
            "actual parsed bitcode differs from sealed current producer facts",
        ));
    }
    Ok(())
}

pub(crate) fn prelink_module(
    unit: usize,
    program: &crate::model::Module,
    config: &Config<'_>,
    merging: FunctionMerging,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<PrelinkedUnit, Error> {
    let start = Instant::now();
    guard(config, merging)?;
    check(cancelled)?;
    if program.triple != config.triple {
        return Err(reject("structured module target mismatch"));
    }
    let t = Instant::now();
    let schedule = crate::preflight::validate(program).map_err(reject)?;
    let preflight_ns = nanos(t);
    let env = Environment::new(config)?;
    let t = Instant::now();
    let module = builder::build(env.context.0, program, env.layout.0, &schedule, cancelled)?;
    let builder_ns = nanos(t);
    env.target(&module, config.triple)?;
    verify(&module)?;
    let original = scan(env.context.0, &module)?;
    for n in &original.defs {
        if unsafe { LLVMGetLinkage(find(&module, n)) } != 0 {
            return Err(reject("canonical definition must be External"));
        }
    }
    let t = Instant::now();
    env.passes(&module, c"thinlto-pre-link<O2>")?;
    let optimize_ns = nanos(t);
    verify(&module)?;
    check(cancelled)?;
    let actual = scan(env.context.0, &module)?;
    if actual.defs != original.defs {
        return Err(reject("prelink changed canonical definition set"));
    }
    for (n, sig) in &actual.signatures {
        if let Some(old) = original.signatures.get(n) {
            if old != sig {
                return Err(reject("prelink changed function signature"));
            }
        } else if !actual.intrinsics.contains(n) || !actual.decls.contains(n) {
            return Err(reject("unknown prelink symbol"));
        }
    }
    let t = Instant::now();
    let buffer = Buffer(unsafe { LLVMWriteBitcodeToMemoryBuffer(module.0) });
    if buffer.0.is_null() {
        return Err(reject("bitcode serialization failed"));
    }
    let len = unsafe { LLVMGetBufferSize(buffer.0) };
    let p = unsafe { LLVMGetBufferStart(buffer.0) };
    if len == 0 || len > isize::MAX as usize || p.is_null() {
        return Err(reject("invalid bitcode buffer"));
    }
    let bytes = unsafe { std::slice::from_raw_parts(p.cast::<u8>(), len) }.to_vec();
    let serialize_ns = nanos(t);
    let mut foreign = original
        .decls
        .iter()
        .map(|n| (n.clone(), original.signatures[n].clone()))
        .collect::<BTreeMap<_, _>>();
    for n in &actual.intrinsics {
        foreign.insert(n.clone(), actual.signatures[n].clone());
    }
    let receipt = format!(
        "{{\"format\":\"zil-research-manual-prelink-unit/1\",\"unit\":{unit},\"llvm_version\":{},\"triple\":{},\"cpu\":\"x86-64\",\"features\":\"\",\"relocation\":\"PIC\",\"code_model\":\"default\",\"pipeline\":\"thinlto-pre-link<O2>\",\"bitcode_kind\":\"ordinary-no-summary\",\"verify_before\":true,\"verify_after\":true,\"current_structured_preflight\":true,\"canonical\":{},\"definitions\":{},\"declarations\":{},\"direct_calls\":{},\"intrinsics\":{},\"bitcode_bytes\":{len},\"preflight_ns\":{preflight_ns},\"builder_ns\":{builder_ns},\"optimize_ns\":{optimize_ns},\"serialize_ns\":{serialize_ns},\"total_ns\":{}}}",
        json(crate::LLVM_VERSION),
        json(config.triple),
        names_json(&original.defs),
        names_json(&actual.defs),
        names_json(&actual.decls),
        calls_json(&actual.calls),
        names_json(&actual.intrinsics),
        nanos(start)
    );
    Ok(PrelinkedUnit {
        bytes,
        facts: UnitFacts {
            unit,
            canonical: original.defs.into_iter().collect(),
            definitions: actual.defs.into_iter().collect(),
            declarations: actual.decls.into_iter().collect(),
            direct_calls: actual.calls,
        },
        signatures: actual.signatures,
        foreign,
        intrinsics: actual.intrinsics,
        triple: config.triple.into(),
        receipt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model as m;

    fn config() -> Config<'static> {
        Config {
            triple: "x86_64-unknown-linux-gnu",
            optimization: Optimization::O2,
        }
    }
    fn inst(name: Option<&str>, op: m::Op) -> m::Inst {
        m::Inst::new(name.map(str::to_owned), op)
    }
    fn function(n: &str, params: Vec<m::Param>, instructions: Vec<m::Inst>) -> m::Function {
        m::Function {
            name: n.into(),
            linkage: m::Linkage::External,
            signature: m::Signature::new(m::Ty::Int(32), params),
            attrs: vec![],
            blocks: Some(vec![m::Block {
                name: "entry".into(),
                instructions,
            }]),
        }
    }
    fn constant(n: &str) -> m::Function {
        function(
            n,
            vec![],
            vec![inst(
                None,
                m::Op::Return(Some(m::Arg::new(m::Ty::Int(32), m::Val::Int(42)))),
            )],
        )
    }
    fn model(items: Vec<m::Item>) -> m::Module {
        m::Module {
            triple: config().triple.into(),
            fingerprint: "current-native-research-control".into(),
            items,
        }
    }
    fn call(n: &str, args: Vec<m::Arg>) -> m::Inst {
        inst(
            Some("result"),
            m::Op::Call {
                result: m::Ty::Int(32),
                result_attrs: vec![],
                callee: m::Val::Global(n.into()),
                args,
                vararg: None,
            },
        )
    }

    #[test]
    fn valid_call_same_user_callee_and_address_have_distinct_use_roles() {
        for address in [false, true] {
            let callee = function(
                "callee",
                vec![m::Param::new(m::Ty::Ptr, "p")],
                vec![inst(
                    None,
                    m::Op::Return(Some(m::Arg::new(m::Ty::Int(32), m::Val::Int(42)))),
                )],
            );
            let arg = if address {
                m::Val::Global("callee".into())
            } else {
                m::Val::Null
            };
            let caller = function(
                "caller",
                vec![],
                vec![
                    call("callee", vec![m::Arg::new(m::Ty::Ptr, arg)]),
                    inst(
                        None,
                        m::Op::Return(Some(m::Arg::new(
                            m::Ty::Int(32),
                            m::Val::Local("result".into()),
                        ))),
                    ),
                ],
            );
            let input = model(vec![m::Item::Function(callee), m::Item::Function(caller)]);
            // These controls pass BOTH complete typed preflight and actual LLVM
            // verification. The rejection below is solely the Use-role gate.
            let schedule = crate::preflight::validate(&input).unwrap();
            let env = Environment::new(&config()).unwrap();
            let module =
                builder::build(env.context.0, &input, env.layout.0, &schedule, &|| false).unwrap();
            verify(&module).unwrap();
            let f = find(&module, "callee");
            let mut u = unsafe { LLVMGetFirstUse(f) };
            let (mut accepted, mut rejected) = (0, 0);
            while !u.is_null() {
                if direct_use(f, u).is_ok() {
                    accepted += 1
                } else {
                    rejected += 1
                }
                u = unsafe { LLVMGetNextUse(u) };
            }
            assert_eq!((accepted, rejected), if address { (1, 1) } else { (1, 0) });
            assert_eq!(scan(env.context.0, &module).is_err(), address);
        }
    }
    fn prelinked_fixture() -> Vec<PrelinkedUnit> {
        (0..8)
            .map(|unit| {
                let f = if unit == 0 {
                    function(
                        "f0",
                        vec![],
                        vec![
                            call("f1", vec![]),
                            inst(
                                None,
                                m::Op::Return(Some(m::Arg::new(
                                    m::Ty::Int(32),
                                    m::Val::Local("result".into()),
                                ))),
                            ),
                        ],
                    )
                } else {
                    constant(&format!("f{unit}"))
                };
                let mut items = vec![m::Item::Function(f)];
                if unit == 0 {
                    items.push(m::Item::Function(constant("f1").declaration()));
                }
                prelink_module(
                    unit,
                    &model(items),
                    &config(),
                    FunctionMerging::AfterOptimization,
                    &|| false,
                )
                .unwrap()
            })
            .collect()
    }
    #[test]
    fn current_eight_units_generate_object_and_private_direct_import() {
        let units = prelinked_fixture();
        let result = compile_prelinked_unit(
            0,
            &units,
            &["f1".into()],
            &config(),
            FunctionMerging::AfterOptimization,
            &|| false,
        )
        .unwrap();
        assert!(result.object.bytes.starts_with(b"\x7fELF"));
        assert!(result.object.receipt.verify_before_after);
        assert!(
            result
                .receipt
                .to_json()
                .contains("\"private\":\"zil.import.0.f1\"")
        );
        assert!(result.receipt.to_json().contains("\"required\":[\"f1\"]"));
    }
    #[test]
    fn actual_required_without_current_allowed_is_rejected() {
        let units = prelinked_fixture();
        let error = compile_prelinked_unit(
            0,
            &units,
            &[],
            &config(),
            FunctionMerging::AfterOptimization,
            &|| false,
        )
        .err()
        .expect("Required must not silently become a declaration");
        assert!(error.message.contains("Required exceeds"));
    }
    #[test]
    fn eager_bitreader_failure_and_linker_consumed_source_failure() {
        let env = Environment::new(&config()).unwrap();
        assert!(env.parse(b"not LLVM bitcode").is_err());
        let input = model(vec![m::Item::Function(constant("duplicate"))]);
        let schedule = crate::preflight::validate(&input).unwrap();
        let dest =
            builder::build(env.context.0, &input, env.layout.0, &schedule, &|| false).unwrap();
        let mut source =
            builder::build(env.context.0, &input, env.layout.0, &schedule, &|| false).unwrap();
        verify(&dest).unwrap();
        verify(&source).unwrap();
        let raw = std::mem::replace(&mut source.0, ptr::null_mut());
        assert_ne!(unsafe { LLVMLinkModules2(dest.0, raw) }, 0);
        // Source is already destroyed by LLVM on failure, never dropped twice.
        assert!(source.0.is_null());
    }
}

pub(crate) fn compile_prelinked_unit(
    unit: usize,
    units: &[PrelinkedUnit],
    allowed: &[String],
    config: &Config<'_>,
    merging: FunctionMerging,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<ResearchObject, Error> {
    let start = Instant::now();
    guard(config, merging)?;
    check(cancelled)?;
    if units.len() != 8 || unit >= units.len() {
        return Err(reject("exact eight current prelinked units required"));
    }
    let allowed_set = allowed.iter().cloned().collect::<BTreeSet<_>>();
    if allowed_set.len() != allowed.len() {
        return Err(reject("duplicate Allowed"));
    }
    let mut owners = BTreeMap::new();
    let mut foreign = BTreeMap::new();
    for (i, p) in units.iter().enumerate() {
        if p.facts.unit != i
            || p.triple != config.triple
            || p.facts.definitions != p.facts.canonical
        {
            return Err(reject("prelinked unit identity mismatch"));
        }
        for n in &p.facts.canonical {
            if owners.insert(n.clone(), i).is_some() {
                return Err(reject("duplicate canonical owner"));
            }
        }
        for (n, sig) in &p.foreign {
            if let Some(old) = foreign.insert(n.clone(), sig.clone()) {
                if old != *sig {
                    return Err(reject("foreign signature disagreement"));
                }
            }
        }
    }
    let own = units[unit]
        .facts
        .canonical
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if allowed_set
        .iter()
        .any(|n| !owners.contains_key(n) || owners[n] == unit)
    {
        return Err(reject("unknown/own symbol in Allowed"));
    }
    // These private immutable facts and bytes were produced together from each
    // current verified LLVM module. No cache/deserializer/receipt constructor
    // exists. Reuse the actual producer graph; parse only own/Required donors.
    let env = Environment::new(config)?;
    let t = Instant::now();
    let mut graph = BTreeMap::new();
    let mut parsed_sigs = BTreeMap::new();
    for p in units {
        check(cancelled)?;
        for (n, sig) in &p.signatures {
            if let Some(old) = parsed_sigs.insert(n.clone(), sig.clone()) {
                if old != *sig {
                    return Err(reject("cross-module signature disagreement"));
                }
            }
        }
        for (n, calls) in &p.facts.direct_calls {
            if graph.insert(n.clone(), calls).is_some() {
                return Err(reject("duplicate actual definition"));
            }
        }
    }
    let intrinsic = units
        .iter()
        .flat_map(|p| p.intrinsics.iter().cloned())
        .collect::<BTreeSet<_>>();
    for (n, sig) in &parsed_sigs {
        if !owners.contains_key(n) {
            let exact_c =
                (n == "getchar" && sig == "i32 ()") || (n == "putchar" && sig == "i32 (i32)");
            if foreign.get(n) != Some(sig) || (!exact_c && !intrinsic.contains(n)) {
                return Err(reject("unknown actual callable"));
            }
        }
    }
    let mut pending = own.iter().cloned().collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    let mut required = BTreeSet::new();
    while let Some(n) = pending.pop() {
        if !seen.insert(n.clone()) {
            continue;
        }
        let calls = graph
            .get(&n)
            .ok_or_else(|| reject("required canonical body missing"))?;
        for target in calls.iter() {
            if let Some(owner) = owners.get(target) {
                if *owner != unit {
                    required.insert(target.clone());
                }
                pending.push(target.clone());
            } else if !foreign.contains_key(target) {
                return Err(reject("unknown actual closure target"));
            }
        }
    }
    if !required.is_subset(&allowed_set) {
        return Err(reject("actual prelink Required exceeds current Allowed"));
    }
    let dest = env.parse(&units[unit].bytes)?;
    env.target(&dest, config.triple)?;
    check_sealed(env.context.0, &dest, &units[unit])?;
    let parse_graph_ns = nanos(t);
    let t = Instant::now();
    let mut expected = own.clone();
    let mut donor_count = 0;
    for (i, p) in units.iter().enumerate() {
        let selected = required
            .iter()
            .filter(|n| owners[*n] == i)
            .cloned()
            .collect::<BTreeSet<_>>();
        if selected.is_empty() {
            continue;
        }
        check(cancelled)?;
        donor_count += 1;
        let mut donor = env.parse(&p.bytes)?;
        env.target(&donor, config.triple)?;
        check_sealed(env.context.0, &donor, p)?;
        for n in &p.facts.canonical {
            if !selected.contains(n) {
                unsafe { LLVMSetLinkage(find(&donor, n), 8) };
            }
        }
        verify(&donor)?;
        env.passes(&donor, c"globaldce")?;
        verify(&donor)?;
        if scan(env.context.0, &donor)?.defs != selected {
            return Err(reject(
                "DCE retained missing/unselected dependency or dropped selected body",
            ));
        }
        // LinkModules consumes Src even on failure: disarm RAII before call.
        let raw = std::mem::replace(&mut donor.0, ptr::null_mut());
        if unsafe { LLVMLinkModules2(dest.0, raw) } != 0 {
            return Err(reject("current donor link failed"));
        }
        verify(&dest)?;
        expected.extend(selected);
        if scan(env.context.0, &dest)?.defs != expected {
            return Err(reject("link changed exact own/import definition set"));
        }
    }
    let link_ns = nanos(t);
    verify(&dest)?;
    let before = scan(env.context.0, &dest)?;
    if before.defs != own.union(&required).cloned().collect() {
        return Err(reject("assembled definition coverage mismatch"));
    }
    // Collect and validate ALL changes before the first mutation. Never add a
    // canonical declaration, copy attributes, change thresholds or rewrite body.
    let mut changes = Vec::new();
    for n in &required {
        let f = find(&dest, n);
        let private = format!("zil.import.{unit}.{n}");
        if f.is_null() || unsafe { LLVMIsDeclaration(f) } != 0 || !find(&dest, &private).is_null() {
            return Err(reject("missing import or private name collision"));
        }
        let mut u = unsafe { LLVMGetFirstUse(f) };
        let mut uses = 0usize;
        while !u.is_null() {
            direct_use(f, u)?;
            uses = uses
                .checked_add(1)
                .ok_or_else(|| reject("use count overflow"))?;
            u = unsafe { LLVMGetNextUse(u) };
        }
        changes.push((n.clone(), private, f, uses));
    }
    for (_, private, f, _) in &changes {
        unsafe {
            LLVMSetValueName2(*f, private.as_ptr().cast(), private.len());
            LLVMSetLinkage(*f, 8)
        };
        if name(*f)? != *private {
            return Err(reject("LLVM silently renamed private import"));
        }
    }
    verify(&dest)?;
    let after = scan(env.context.0, &dest)?;
    let mut private_defs = own.clone();
    private_defs.extend(changes.iter().map(|(_, p, _, _)| p.clone()));
    if after.defs != private_defs || required.iter().any(|n| !find(&dest, n).is_null()) {
        return Err(reject("private definition/declaration invariant failed"));
    }
    check(cancelled)?;
    let t = Instant::now();
    env.passes(&dest, c"thinlto<O2>")?;
    let optimize_ns = nanos(t);
    verify(&dest)?;
    check(cancelled)?;
    let t = Instant::now();
    let mut raw = ptr::null_mut();
    let mut msg = ptr::null_mut();
    let failed = unsafe {
        LLVMTargetMachineEmitToMemoryBuffer(env.machine.0, dest.0, 1, &mut msg, &mut raw)
    };
    let output = Buffer(raw);
    let message = unsafe { take_message(msg) };
    if failed != 0 || output.0.is_null() {
        return Err(error(ErrorKind::Emit, message));
    }
    let len = unsafe { LLVMGetBufferSize(output.0) };
    let p = unsafe { LLVMGetBufferStart(output.0) };
    if len == 0 || len > isize::MAX as usize || p.is_null() {
        return Err(reject("invalid native object buffer"));
    }
    let bytes = unsafe { std::slice::from_raw_parts(p.cast::<u8>(), len) }.to_vec();
    let emit_ns = nanos(t);
    check(cancelled)?;
    let imports = changes
        .iter()
        .map(|(n, p, _, uses)| {
            format!(
                "{{\"canonical\":{},\"private\":{},\"direct_uses\":{uses}}}",
                json(n),
                json(p)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let pipeline = "manual-prelink<O2>|budgeted-direct-imports=private|thinlto<O2>|merge-functions=after|aliases=false";
    let receipt = ResearchReceipt(format!(
        "{{\"format\":\"zil-research-manual-prelink-object/1\",\"unit\":{unit},\"official_thinlto_importer\":false,\"native_cache\":false,\"actual_bitcode_graph_checked\":true,\"required\":{},\"allowed\":{},\"imports\":[{imports}],\"donor_modules\":{donor_count},\"verify_parse_trim_link_private_postlink\":true,\"parse_graph_ns\":{parse_graph_ns},\"trim_link_ns\":{link_ns},\"postlink_optimize_ns\":{optimize_ns},\"emit_ns\":{emit_ns},\"total_ns\":{}}}",
        names_json(&required),
        names_json(&allowed_set),
        nanos(start)
    ));
    Ok(ResearchObject {
        object: Object {
            bytes,
            receipt: Receipt {
                llvm_version: crate::LLVM_VERSION.into(),
                triple: config.triple.into(),
                cpu: "x86-64".into(),
                features: String::new(),
                data_layout: env.data_layout.clone(),
                pipeline: pipeline.into(),
                relocation: "PIC".into(),
                code_model: "default".into(),
                verify_before_after: true,
                input_kind: "research-current-prelinked-bitcode".into(),
                preflight_ns: 0,
                builder_ns: 0,
                parse_ns: parse_graph_ns,
                optimize_ns,
                emit_ns,
                total_ns: nanos(start),
            },
        },
        receipt,
    })
}
