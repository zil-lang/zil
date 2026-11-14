//! Ephemeral, current-module machine-body sharing. No retained LLVM owners or
//! cross-request capability. The complete original module has already passed
//! preflight, Builder, IR O2 and LLVM verification. Speculation uses a clone;
//! refusal leaves the original optimized module untouched for ordinary emission.
use super::*;
use crate::EmissionReuseStats;
use std::collections::BTreeMap;

unsafe extern "C" {
    fn LLVMGetFirstFunction(module: Handle) -> Handle;
    fn LLVMGetNextFunction(function: Handle) -> Handle;
    fn LLVMIsDeclaration(function: Handle) -> c_int;
    fn LLVMGetLinkage(function: Handle) -> c_int;
    fn LLVMSetLinkage(function: Handle, linkage: c_int);
    fn LLVMGetVisibility(function: Handle) -> c_int;
    fn LLVMGetDLLStorageClass(function: Handle) -> c_int;
    fn LLVMGetValueName2(value: Handle, len: *mut usize) -> *const c_char;
    fn LLVMGlobalGetValueType(value: Handle) -> Handle;
    fn LLVMGetTypeKind(ty: Handle) -> c_int;
    fn LLVMGetIntTypeWidth(ty: Handle) -> c_uint;
    fn LLVMGetReturnType(ty: Handle) -> Handle;
    fn LLVMCountParamTypes(ty: Handle) -> c_uint;
    fn LLVMGetParamTypes(ty: Handle, out: *mut Handle);
    fn LLVMIsFunctionVarArg(ty: Handle) -> c_int;
    fn LLVMGetFunctionCallConv(function: Handle) -> c_uint;
    fn LLVMGetAttributeCountAtIndex(function: Handle, index: c_uint) -> c_uint;
    fn LLVMGetAttributesAtIndex(function: Handle, index: c_uint, out: *mut Handle);
    fn LLVMIsEnumAttribute(attribute: Handle) -> c_int;
    fn LLVMGetEnumAttributeKind(attribute: Handle) -> c_uint;
    fn LLVMGetEnumAttributeValue(attribute: Handle) -> u64;
    fn LLVMGetEnumAttributeKindForName(name: *const c_char, len: usize) -> c_uint;
    fn LLVMCountBasicBlocks(function: Handle) -> c_uint;
    fn LLVMGetFirstBasicBlock(function: Handle) -> Handle;
    fn LLVMGetNextBasicBlock(block: Handle) -> Handle;
    fn LLVMGetFirstInstruction(block: Handle) -> Handle;
    fn LLVMGetNextInstruction(instruction: Handle) -> Handle;
    fn LLVMPrintValueToString(value: Handle) -> *mut c_char;
    fn LLVMCloneModule(module: Handle) -> Handle;
    fn LLVMGetNamedFunction(module: Handle, name: *const c_char) -> Handle;
    fn LLVMGetSourceFileName(module: Handle, len: *mut usize) -> *const c_char;
    fn LLVMModuleCreateWithNameInContext(name: *const c_char, context: Handle) -> Handle;
    fn LLVMSetSourceFileName(module: Handle, name: *const c_char, len: usize);
    fn LLVMSetTarget(module: Handle, triple: *const c_char);
    fn LLVMSetModuleInlineAsm2(module: Handle, assembly: *const c_char, len: usize);
    fn LLVMContextSetDiagnosticHandler(
        context: Handle,
        handler: Option<unsafe extern "C" fn(Handle, *mut c_void)>,
        payload: *mut c_void,
    );
    fn LLVMGetDiagInfoSeverity(info: Handle) -> c_int;
}

const MAX_FUNCTIONS: usize = 4096;
const MAX_KEY_BYTES: usize = 4 * 1024 * 1024;
const MAX_ASSEMBLY_BYTES: usize = 32 * 1024 * 1024;

fn source_symbol(name: &str) -> bool {
    name.strip_prefix("zil.fn.").is_some_and(|id| {
        !id.is_empty() && id.len() <= 10 && id.bytes().all(|b| b.is_ascii_digit())
    })
}

// SAFETY: these helpers receive live functions/types in the caller's current
// module/context, on this thread. No handle leaves this complete invocation.
unsafe fn primitive(ty: Handle) -> bool {
    !ty.is_null()
        && unsafe { LLVMGetTypeKind(ty) } == 8
        && matches!(unsafe { LLVMGetIntTypeWidth(ty) }, 1 | 8 | 16 | 32 | 64)
}
unsafe fn eligible_signature(function: Handle) -> bool {
    if unsafe { LLVMGetLinkage(function) } != 0
        || unsafe { LLVMGetVisibility(function) } != 0
        || unsafe { LLVMGetDLLStorageClass(function) } != 0
        || unsafe { LLVMGetFunctionCallConv(function) } != 0
    {
        return false;
    }
    let ty = unsafe { LLVMGlobalGetValueType(function) };
    if ty.is_null()
        || unsafe { LLVMGetTypeKind(ty) } != 9
        || unsafe { LLVMIsFunctionVarArg(ty) } != 0
        || !unsafe { primitive(LLVMGetReturnType(ty)) }
    {
        return false;
    }
    let count = unsafe { LLVMCountParamTypes(ty) } as usize;
    if count > 8 {
        return false;
    }
    let mut params = vec![ptr::null_mut(); count];
    unsafe { LLVMGetParamTypes(ty, params.as_mut_ptr()) };
    if !params.into_iter().all(|ty| unsafe { primitive(ty) }) {
        return false;
    }
    if unsafe { LLVMCountBasicBlocks(function) } > 4 {
        return false;
    }
    let mut block = unsafe { LLVMGetFirstBasicBlock(function) };
    let mut instructions = 0;
    while !block.is_null() {
        let mut instruction = unsafe { LLVMGetFirstInstruction(block) };
        while !instruction.is_null() {
            instructions += 1;
            if instructions > 64 {
                return false;
            }
            instruction = unsafe { LLVMGetNextInstruction(instruction) };
        }
        block = unsafe { LLVMGetNextBasicBlock(block) };
    }
    true
}

unsafe fn attributes(function: Handle) -> Option<Vec<(u32, u32, u64)>> {
    static ALLOWED: std::sync::OnceLock<std::collections::BTreeSet<u32>> =
        std::sync::OnceLock::new();
    let allowed = ALLOWED.get_or_init(|| {
        [
            "noinline",
            "alwaysinline",
            "optnone",
            "optsize",
            "minsize",
            "noreturn",
            "nounwind",
            "norecurse",
            "nofree",
            "nosync",
            "willreturn",
            "mustprogress",
            "memory",
            "speculatable",
            "noundef",
            "zeroext",
            "signext",
            "inreg",
            "returned",
        ]
        .into_iter()
        .map(|name| unsafe { LLVMGetEnumAttributeKindForName(name.as_ptr().cast(), name.len()) })
        .filter(|kind| *kind != 0)
        .collect()
    });
    let ty = unsafe { LLVMGlobalGetValueType(function) };
    let count = unsafe { LLVMCountParamTypes(ty) };
    let mut result = Vec::new();
    // Bind typed current attributes, never printer slot IDs alone. Unknown,
    // string/target-specific and type attributes conservatively refuse reuse.
    for index in (0..=count).chain(std::iter::once(u32::MAX)) {
        let count = unsafe { LLVMGetAttributeCountAtIndex(function, index) } as usize;
        if count > 64 {
            return None;
        }
        let mut list = vec![ptr::null_mut(); count];
        unsafe { LLVMGetAttributesAtIndex(function, index, list.as_mut_ptr()) };
        for attribute in list {
            if attribute.is_null() || unsafe { LLVMIsEnumAttribute(attribute) } == 0 {
                return None;
            }
            let kind = unsafe { LLVMGetEnumAttributeKind(attribute) };
            if !allowed.contains(&kind) {
                return None;
            }
            result.push((index, kind, unsafe { LLVMGetEnumAttributeValue(attribute) }));
        }
    }
    result.sort_unstable();
    Some(result)
}

fn integer_intrinsic(name: &str) -> bool {
    if name == "llvm.trap" {
        return true;
    }
    ["sadd", "uadd", "ssub", "usub", "smul", "umul"]
        .iter()
        .any(|op| {
            name.strip_prefix(&format!("llvm.{op}.with.overflow.i"))
                .is_some_and(|bits| matches!(bits, "8" | "16" | "32" | "64"))
        })
}

fn closed_key(text: &str, name: &str) -> Option<String> {
    if text.len() > 16 * 1024 || text.lines().count() > 128 {
        return None;
    }
    // No memory, constant pools, address identity, exceptional state, module
    // metadata, inline asm, vectors, or CFG-dependent tables. Attributes remain
    // in the exact text. Attribute slot numbers are used ONLY within this SAME
    // immutable module, before any linkage change; never across CGUs/requests.
    for denied in [
        "ptr",
        "float",
        "double",
        "half",
        "alloca",
        " load ",
        " store ",
        "atomic",
        "volatile",
        "personality",
        "comdat",
        " section ",
        " gc ",
        "blockaddress",
        "indirectbr",
        "callbr",
        "invoke",
        "landingpad",
        "switch",
        "phi ",
        " asm ",
        "!",
        "[",
        "<",
        "vscale",
        "token",
        "freeze",
    ] {
        if text.contains(denied) {
            return None;
        }
    }
    let bare = format!("@{name}(");
    let quoted = format!("@\"{name}\"(");
    let (at, len) = text
        .find(&bare)
        .map(|at| (at, bare.len() - 1))
        .or_else(|| text.find(&quoted).map(|at| (at, quoted.len() - 1)))?;
    let open = text.find('{')?;
    if at >= open || !text[..at].contains("define ") {
        return None;
    }
    let body = &text[open..];
    let mut tail = body;
    while let Some(at) = tail.find('@') {
        tail = &tail[at + 1..];
        let end = tail.find(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '_'))?;
        if !integer_intrinsic(&tail[..end]) {
            return None;
        }
        tail = &tail[end..];
    }
    let mut key = String::with_capacity(text.len());
    key.push_str(&text[..at]);
    key.push_str("@__zil_current_body");
    key.push_str(&text[at + len..]);
    Some(key)
}

struct Selection {
    aliases: BTreeMap<String, String>,
    definitions: Vec<String>,
}

fn select(module: &Module, stats: &mut EmissionReuseStats) -> Option<Selection> {
    let mut definitions = Vec::new();
    let mut aliases = BTreeMap::new();
    let mut bodies = BTreeMap::<String, String>::new();
    let mut bytes = 0usize;
    // SAFETY: iteration and all inspection finish while this module stays live.
    let mut function = unsafe { LLVMGetFirstFunction(module.0) };
    let mut count = 0usize;
    while !function.is_null() {
        count = count.checked_add(1)?;
        if count > MAX_FUNCTIONS {
            return None;
        }
        if unsafe { LLVMIsDeclaration(function) } == 0 {
            let mut len = 0;
            let ptr = unsafe { LLVMGetValueName2(function, &mut len) };
            if ptr.is_null() || len > 256 {
                return None;
            }
            let name = std::str::from_utf8(unsafe { std::slice::from_raw_parts(ptr.cast(), len) })
                .ok()?
                .to_owned();
            definitions.push(name.clone());
            if source_symbol(&name) && unsafe { eligible_signature(function) } {
                stats.considered_functions += 1;
                let printed = unsafe { take_message(LLVMPrintValueToString(function)) };
                bytes = bytes.checked_add(printed.len())?;
                if bytes > MAX_KEY_BYTES {
                    return None;
                }
                if let Some(mut key) = closed_key(&printed, &name)
                    && let Some(attrs) = unsafe { attributes(function) }
                {
                    use std::fmt::Write;
                    let _ = write!(key, "\n; typed-current-attributes:{attrs:?}");
                    if let Some(canonical) = bodies.get(&key) {
                        aliases.insert(name, canonical.clone());
                    } else {
                        bodies.insert(key, name);
                    }
                }
            }
        }
        function = unsafe { LLVMGetNextFunction(function) };
    }
    stats.distinct_bodies = bodies.len();
    (aliases.len() >= 4).then_some(Selection {
        aliases,
        definitions,
    })
}

fn emit_buffer(machine: &Machine, module: &Module, kind: c_int) -> Result<Vec<u8>, Error> {
    let mut raw = ptr::null_mut();
    let mut msg = ptr::null_mut();
    // SAFETY: current live same-thread owners; LLVM returns a newly owned buffer.
    let failed = unsafe {
        LLVMTargetMachineEmitToMemoryBuffer(machine.0, module.0, kind, &mut msg, &mut raw)
    };
    let buffer = Buffer(raw);
    let message = unsafe { take_message(msg) };
    if failed != 0 || buffer.0.is_null() {
        return Err(error(ErrorKind::Emit, message));
    }
    let len = unsafe { LLVMGetBufferSize(buffer.0) };
    let data = unsafe { LLVMGetBufferStart(buffer.0) };
    if len == 0 || len > MAX_ASSEMBLY_BYTES || data.is_null() {
        return Err(error(
            ErrorKind::Emit,
            "machine assembly buffer outside optional bound",
        ));
    }
    Ok(unsafe { std::slice::from_raw_parts(data.cast(), len) }.to_vec())
}

unsafe extern "C" fn diagnostic(info: Handle, payload: *mut c_void) {
    // SAFETY: payload is the stack-owned bool installed for this synchronous
    // private context. This callback never unwinds or invokes user callbacks.
    if !info.is_null() && !payload.is_null() && unsafe { LLVMGetDiagInfoSeverity(info) } == 0 {
        unsafe {
            *payload.cast::<bool>() = true;
        }
    }
}

fn assemble(
    source: &Module,
    machine: &Machine,
    config: &Config<'_>,
    assembly: &[u8],
) -> Result<Vec<u8>, Error> {
    let context = Context(unsafe { LLVMContextCreate() });
    if context.0.is_null() {
        return Err(error(ErrorKind::Emit, "assembly context unavailable"));
    }
    let mut failed = false;
    unsafe {
        LLVMContextSetDiagnosticHandler(
            context.0,
            Some(diagnostic),
            (&mut failed as *mut bool).cast(),
        )
    };
    let module =
        Module(unsafe { LLVMModuleCreateWithNameInContext(c"zil-emission".as_ptr(), context.0) });
    if module.0.is_null() {
        return Err(error(ErrorKind::Emit, "assembly module unavailable"));
    }
    let mut len = 0;
    let name = unsafe { LLVMGetSourceFileName(source.0, &mut len) };
    if name.is_null() || len > 1024 {
        return Err(error(
            ErrorKind::Emit,
            "source filename outside optional bound",
        ));
    }
    let triple =
        CString::new(config.triple).map_err(|e| error(ErrorKind::Target, e.to_string()))?;
    unsafe {
        LLVMSetSourceFileName(module.0, name, len);
        LLVMSetTarget(module.0, triple.as_ptr());
        LLVMSetModuleInlineAsm2(module.0, assembly.as_ptr().cast(), assembly.len());
    }
    verify(&module)?;
    let result = emit_buffer(machine, &module, 1);
    if failed {
        return Err(error(
            ErrorKind::Emit,
            "reconstructed assembly refused by current LLVM",
        ));
    }
    result
}

pub(super) fn try_emit(
    module: &Module,
    machine: &Machine,
    config: &Config<'_>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<(Option<Vec<u8>>, EmissionReuseStats), Error> {
    let mut stats = EmissionReuseStats::default();
    let t = Instant::now();
    let selection = select(module, &mut stats);
    stats.selection_ns = nanos(t);
    let Some(selection) = selection else {
        return Ok((None, stats));
    };
    check(cancelled)?;
    // A distinct clone contains every original optimized definition. Only the
    // redundant machine generation is suppressed. Original parent IR stays
    // untouched even if codegen preparation mutates the speculative clone.
    let speculative = Module(unsafe { LLVMCloneModule(module.0) });
    if speculative.0.is_null() {
        stats.fallback = true;
        return Ok((None, stats));
    }
    for name in selection.aliases.keys() {
        let name = CString::new(name.as_str()).expect("bounded generated symbol");
        let function = unsafe { LLVMGetNamedFunction(speculative.0, name.as_ptr()) };
        if function.is_null() {
            stats.fallback = true;
            return Ok((None, stats));
        }
        unsafe { LLVMSetLinkage(function, 1) }; // available_externally, not an alias
    }
    if verify(&speculative).is_err() {
        stats.fallback = true;
        return Ok((None, stats));
    }
    let t = Instant::now();
    let assembly = emit_buffer(machine, &speculative, 0);
    stats.machine_assembly_ns = nanos(t);
    check(cancelled)?;
    let Ok(assembly) = assembly else {
        stats.fallback = true;
        return Ok((None, stats));
    };
    let t = Instant::now();
    let full = super::emission_assembly::reconstruct(
        &assembly,
        &selection.aliases,
        &selection.definitions,
    );
    stats.reconstruction_ns = nanos(t);
    let Some(full) = full else {
        stats.fallback = true;
        return Ok((None, stats));
    };
    check(cancelled)?;
    let t = Instant::now();
    let object = assemble(module, machine, config, &full);
    stats.assembly_ns = nanos(t);
    check(cancelled)?;
    match object {
        Ok(bytes) => {
            stats.reused_functions = selection.aliases.len();
            Ok((Some(bytes), stats))
        }
        Err(_) => {
            stats.fallback = true;
            Ok((None, stats))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keys_bind_full_current_definition_and_refuse_external_state() {
        let a = "define i32 @zil.fn.1(i32 %x) #0 {\nentry:\n %v = add i32 %x, 1\n ret i32 %v\n}";
        let b = a.replace("@zil.fn.1(", "@zil.fn.10(");
        assert_eq!(closed_key(a, "zil.fn.1"), closed_key(&b, "zil.fn.10"));
        assert_ne!(
            closed_key(a, "zil.fn.1"),
            closed_key(&a.replace("#0", "#1"), "zil.fn.1")
        );
        for bad in [
            a.replace("add i32 %x, 1", "call i32 @external(i32 %x)"),
            a.replace("add i32 %x, 1", "load i32, ptr @global"),
            a.replace("#0", "!dbg !1"),
            a.replace("add i32 %x, 1", "call i32 @zil.fn.1(i32 %x)"),
        ] {
            assert!(closed_key(&bad, "zil.fn.1").is_none());
        }
        assert!(closed_key(a, "zil.fn.10").is_none());
    }

    #[test]
    fn current_machine_bodies_keep_complete_ordinary_object_and_cancellation() {
        use crate::model::{
            self as m, Arg, Attr, Block, Function, Inst, Item, Linkage, Op, Param, Signature, Ty,
            Val,
        };
        let module = m::Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "current-emission-test".into(),
            items: (0..12)
                .map(|i| {
                    Item::Function(Function {
                        name: format!("zil.fn.{i}"),
                        linkage: Linkage::External,
                        signature: Signature::new(
                            Ty::Int(32),
                            vec![Param::new(Ty::Int(32), "arg.0")],
                        ),
                        attrs: vec![Attr::NoInline],
                        blocks: Some(
                            vec![Block {
                                name: "entry".into(),
                                instructions: vec![
                                    Inst::new(
                                        Some("v.0".into()),
                                        Op::Binary {
                                            opcode: "add".into(),
                                            ty: Ty::Int(32),
                                            left: m::local("arg.0".into()),
                                            right: Val::Int(i % 3),
                                        },
                                    ),
                                    Inst::new(
                                        None,
                                        Op::Return(Some(Arg::new(
                                            Ty::Int(32),
                                            m::local("v.0".into()),
                                        ))),
                                    ),
                                ],
                            }]
                            .into(),
                        ),
                    })
                })
                .collect::<Vec<_>>()
                .into(),
        };
        let config = Config {
            triple: "x86_64-unknown-linux-gnu",
            optimization: Optimization::O2,
        };
        let ordinary =
            super::super::compile_module(&module, &config, FunctionMerging::Disabled, &|| false)
                .unwrap();
        let (reused, proof) = super::super::compile_module_with_emission_reuse(
            &module,
            &config,
            FunctionMerging::Disabled,
            &|| false,
        )
        .unwrap();
        assert_eq!(
            ordinary.bytes, reused.bytes,
            "complete object, including all distinct symbols and layout"
        );
        assert!(
            crate::validate_module_with_receipt(&module, Some(&proof))
                .unwrap()
                .1
        );
        let stats = reused.receipt.emission_reuse.unwrap();
        assert!(!stats.fallback, "{stats:?}");
        assert_eq!(stats.reused_functions, 9, "{stats:?}");
        assert_eq!(stats.distinct_bodies, 3);
        assert!(reused.receipt.verify_before_after);
        assert_eq!(reused.receipt.pipeline, "default<O2>");
        let error = super::super::compile_module_with_emission_reuse(
            &module,
            &config,
            FunctionMerging::Disabled,
            &|| true,
        )
        .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Cancelled);
        let retried = super::super::compile_module_with_emission_reuse(
            &module,
            &config,
            FunctionMerging::Disabled,
            &|| false,
        )
        .unwrap();
        assert_eq!(retried.0.bytes, ordinary.bytes);
    }
}
