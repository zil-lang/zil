//! Audited raw LLVM C API boundary (LLVM tag llvmorg-23.1.4).
//! Invariants: every owned handle is released once; module dies before context;
//! ParseIR consumes its memory buffer even on failure; target refs are borrowed;
//! all LLVM work for one unit stays on one thread. No raw handle is Send/Sync.
use super::{
    Config, Error, ErrorKind, FunctionMerging, Object, Optimization, PreflightReceipt, Receipt,
};
enum Input<'a> {
    #[cfg(feature = "text-reference")]
    Text(&'a str),
    Structured(&'a crate::model::Module),
}
use std::{
    ffi::{CStr, CString, c_char, c_int, c_uint, c_void},
    ptr,
    sync::Once,
    time::Instant,
};

type Handle = *mut c_void;
unsafe extern "C" {
    fn LLVMGetVersion(major: *mut c_uint, minor: *mut c_uint, patch: *mut c_uint);
    fn LLVMInitializeX86TargetInfo();
    fn LLVMInitializeX86Target();
    fn LLVMInitializeX86TargetMC();
    fn LLVMInitializeX86AsmPrinter();
    fn LLVMInitializeX86AsmParser();
    fn LLVMContextCreate() -> Handle;
    fn LLVMContextDispose(ctx: Handle);
    #[cfg(feature = "text-reference")]
    fn LLVMCreateMemoryBufferWithMemoryRangeCopy(
        data: *const c_char,
        len: usize,
        name: *const c_char,
    ) -> Handle;
    #[cfg(feature = "text-reference")]
    fn LLVMParseIRInContext(
        ctx: Handle,
        buffer: Handle,
        module: *mut Handle,
        error: *mut *mut c_char,
    ) -> c_int;
    fn LLVMDisposeModule(module: Handle);
    fn LLVMVerifyModule(module: Handle, action: c_int, error: *mut *mut c_char) -> c_int;
    fn LLVMDisposeMessage(message: *mut c_char);
    fn LLVMGetTarget(module: Handle) -> *const c_char;
    fn LLVMGetDataLayoutStr(module: Handle) -> *const c_char;
    fn LLVMGetTargetFromTriple(
        triple: *const c_char,
        target: *mut Handle,
        error: *mut *mut c_char,
    ) -> c_int;
    fn LLVMCreateTargetMachine(
        target: Handle,
        triple: *const c_char,
        cpu: *const c_char,
        features: *const c_char,
        opt: c_int,
        reloc: c_int,
        model: c_int,
    ) -> Handle;
    fn LLVMDisposeTargetMachine(machine: Handle);
    fn LLVMCreateTargetDataLayout(machine: Handle) -> Handle;
    fn LLVMDisposeTargetData(layout: Handle);
    fn LLVMCopyStringRepOfTargetData(layout: Handle) -> *mut c_char;
    fn LLVMPointerSize(layout: Handle) -> c_uint;
    fn LLVMByteOrder(layout: Handle) -> c_int;
    fn LLVMSetModuleDataLayout(module: Handle, layout: Handle);
    fn LLVMCreatePassBuilderOptions() -> Handle;
    fn LLVMDisposePassBuilderOptions(options: Handle);
    fn LLVMPassBuilderOptionsSetVerifyEach(options: Handle, value: c_int);
    fn LLVMPassBuilderOptionsSetLoopInterleaving(options: Handle, value: c_int);
    fn LLVMPassBuilderOptionsSetLoopVectorization(options: Handle, value: c_int);
    fn LLVMPassBuilderOptionsSetSLPVectorization(options: Handle, value: c_int);
    fn LLVMPassBuilderOptionsSetMergeFunctions(options: Handle, value: c_int);
    fn LLVMRunPasses(
        module: Handle,
        pipeline: *const c_char,
        machine: Handle,
        options: Handle,
    ) -> Handle;
    fn LLVMGetErrorMessage(error: Handle) -> *mut c_char;
    fn LLVMDisposeErrorMessage(message: *mut c_char);
    fn LLVMTargetMachineEmitToMemoryBuffer(
        machine: Handle,
        module: Handle,
        filetype: c_int,
        error: *mut *mut c_char,
        buffer: *mut Handle,
    ) -> c_int;
    fn LLVMGetBufferStart(buffer: Handle) -> *const c_char;
    fn LLVMGetBufferSize(buffer: Handle) -> usize;
    fn LLVMDisposeMemoryBuffer(buffer: Handle);
}

// These wrappers are deliberately private. Raw pointers auto-prevent Send/Sync.
macro_rules! owned {
    ($name:ident, $dispose:ident) => {
        struct $name(Handle);
        impl Drop for $name {
            fn drop(&mut self) {
                if !self.0.is_null() {
                    // SAFETY: this wrapper exclusively owns the matching handle.
                    unsafe { $dispose(self.0) }
                }
            }
        }
    };
}
owned!(Context, LLVMContextDispose);
owned!(Module, LLVMDisposeModule);
owned!(Machine, LLVMDisposeTargetMachine);
owned!(Layout, LLVMDisposeTargetData);
owned!(PassOptions, LLVMDisposePassBuilderOptions);
owned!(Buffer, LLVMDisposeMemoryBuffer);
mod builder;
pub(crate) mod products;
static INIT: Once = Once::new();
fn error(kind: ErrorKind, message: impl Into<String>) -> Error {
    Error {
        kind,
        message: message.into(),
    }
}
fn check(cancelled: &(dyn Fn() -> bool + Sync)) -> Result<(), Error> {
    if cancelled() {
        Err(error(ErrorKind::Cancelled, "cancelled between LLVM phases"))
    } else {
        Ok(())
    }
}
fn nanos(start: Instant) -> u64 {
    start.elapsed().as_nanos().min(u64::MAX as u128) as u64
}
// Only called for LLVM-owned, NUL-terminated strings while their owner is alive.
unsafe fn text(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        // SAFETY: callers obtain strings from the pinned LLVM C API.
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}
unsafe fn take_message(p: *mut c_char) -> String {
    // SAFETY: p is an LLVM message, or NULL, and consumed exactly once.
    let result = unsafe { text(p) };
    if !p.is_null() {
        unsafe { LLVMDisposeMessage(p) };
    }
    result
}
fn verify(module: &Module) -> Result<(), Error> {
    let mut msg = ptr::null_mut();
    // SAFETY: module/context are live. ReturnStatusAction=2 avoids abort-on-error.
    let failed = unsafe { LLVMVerifyModule(module.0, 2, &mut msg) };
    let message = unsafe { take_message(msg) };
    if failed != 0 {
        Err(error(ErrorKind::Verify, message))
    } else {
        Ok(())
    }
}

#[cfg(feature = "text-reference")]
pub(super) fn compile(
    ir: &str,
    config: &Config<'_>,
    function_merging: FunctionMerging,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Object, Error> {
    compile_input(
        Input::Text(ir),
        config,
        function_merging,
        cancelled,
        false,
        None,
    )
    .map(|(object, _)| object)
}
pub(super) fn compile_module(
    module: &crate::model::Module,
    config: &Config<'_>,
    function_merging: FunctionMerging,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Object, Error> {
    compile_input(
        Input::Structured(module),
        config,
        function_merging,
        cancelled,
        false,
        None,
    )
    .map(|(object, _)| object)
}

pub(super) fn compile_module_with_preflight_receipt(
    module: &crate::model::Module,
    config: &Config<'_>,
    function_merging: FunctionMerging,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<(Object, PreflightReceipt), Error> {
    let (object, receipt) = compile_input(
        Input::Structured(module),
        config,
        function_merging,
        cancelled,
        true,
        None,
    )?;
    // Structured input with issue_receipt=true always takes the checker-owned
    // paired-output branch. No failure can return this pending proof.
    Ok((
        object,
        receipt.expect("structured preflight issued a receipt"),
    ))
}
#[derive(Default)]
struct AttemptTimings {
    preflight_ns: u64,
    builder_ns: u64,
    parse_ns: u64,
    optimize_ns: u64,
    emit_ns: u64,
}
struct NativeAttempt<'a> {
    prior: Option<&'a crate::NativeWorkProducts>,
    stats: crate::NativeProductStats,
    replay_failed: bool,
    products: Option<crate::NativeWorkProducts>,
    timings: AttemptTimings,
}
#[cfg(feature = "native-products")]
pub(super) fn compile_module_with_native_products(
    module: &crate::model::Module,
    config: &Config<'_>,
    function_merging: FunctionMerging,
    cancelled: &(dyn Fn() -> bool + Sync),
    prior: Option<&crate::NativeWorkProducts>,
) -> Result<(Object, PreflightReceipt, Option<crate::NativeWorkProducts>), Error> {
    let start = Instant::now();
    let mut attempt = NativeAttempt {
        prior,
        stats: crate::NativeProductStats::default(),
        replay_failed: false,
        products: None,
        timings: AttemptTimings::default(),
    };
    let result = compile_input(
        Input::Structured(module),
        config,
        function_merging,
        cancelled,
        true,
        Some(&mut attempt),
    );
    // compile_input has fully returned: its Scope, Module, TargetMachine and
    // Context have all been destroyed. Never retry in a partly replayed context.
    let (mut object, receipt) = match result {
        Err(error) if attempt.replay_failed && error.kind != ErrorKind::Cancelled => {
            check(cancelled)?;
            attempt.stats.fresh_context_fallbacks = 1;
            attempt.stats.fallback_ns = nanos(start);
            let (mut object, receipt) = compile_input(
                Input::Structured(module),
                config,
                function_merging,
                cancelled,
                true,
                None,
            )?;
            object.receipt.preflight_ns = object
                .receipt
                .preflight_ns
                .saturating_add(attempt.timings.preflight_ns);
            object.receipt.builder_ns = object
                .receipt
                .builder_ns
                .saturating_add(attempt.timings.builder_ns);
            object.receipt.parse_ns = object
                .receipt
                .parse_ns
                .saturating_add(attempt.timings.parse_ns);
            object.receipt.optimize_ns = object
                .receipt
                .optimize_ns
                .saturating_add(attempt.timings.optimize_ns);
            object.receipt.emit_ns = object
                .receipt
                .emit_ns
                .saturating_add(attempt.timings.emit_ns);
            attempt.products = None;
            (object, receipt)
        }
        other => other?,
    };
    object.receipt.total_ns = nanos(start);
    object.receipt.native_products = Some(attempt.stats);
    Ok((
        object,
        receipt.expect("structured preflight issued a receipt"),
        attempt.products,
    ))
}

pub(super) fn check_config(config: &Config<'_>) -> Result<[u32; 3], Error> {
    if !matches!(
        config.triple,
        "x86_64-unknown-linux-gnu" | "x86_64-pc-windows-msvc"
    ) {
        return Err(error(
            ErrorKind::Target,
            "LLVM API v1 supports only the two checked x86-64 profiles",
        ));
    }
    let mut version = [0; 3];
    // SAFETY: distinct, initialized u32 outputs. No target initialization needed.
    unsafe { LLVMGetVersion(&mut version[0], &mut version[1], &mut version[2]) };
    if version != super::LLVM_VERSION_COMPONENTS {
        return Err(error(
            ErrorKind::Version,
            format!("expected LLVM {}, got {version:?}", super::LLVM_VERSION),
        ));
    }
    Ok(version)
}

/// Report the actual shared object containing the C API symbol, not an
/// environment-variable guess. Only used by the opt-in trusted local cache.
#[cfg(target_os = "linux")]
pub(super) fn runtime_library() -> Result<std::path::PathBuf, Error> {
    #[repr(C)]
    struct DlInfo {
        filename: *const c_char,
        base: *mut c_void,
        symbol: *const c_char,
        address: *mut c_void,
    }
    #[link(name = "dl")]
    unsafe extern "C" {
        fn dladdr(address: *const c_void, info: *mut DlInfo) -> c_int;
    }
    let mut info = DlInfo {
        filename: ptr::null(),
        base: ptr::null_mut(),
        symbol: ptr::null(),
        address: ptr::null_mut(),
    };
    // SAFETY: symbol belongs to the linked shared object, info is correctly
    // laid out and writable. The loader-owned filename lives with the DSO.
    if unsafe { dladdr(LLVMGetVersion as *const () as *const c_void, &mut info) } == 0
        || info.filename.is_null()
    {
        return Err(error(
            ErrorKind::Unavailable,
            "cannot identify loaded LLVM library",
        ));
    }
    use std::os::unix::ffi::OsStrExt;
    let bytes = unsafe { CStr::from_ptr(info.filename) }.to_bytes();
    std::fs::canonicalize(std::ffi::OsStr::from_bytes(bytes)).map_err(|e| {
        error(
            ErrorKind::Unavailable,
            format!("cannot identify loaded LLVM library: {e}"),
        )
    })
}

fn compile_input(
    input: Input<'_>,
    config: &Config<'_>,
    function_merging: FunctionMerging,
    cancelled: &(dyn Fn() -> bool + Sync),
    issue_receipt: bool,
    native_attempt: Option<&mut NativeAttempt<'_>>,
) -> Result<(Object, Option<PreflightReceipt>), Error> {
    let start = Instant::now();
    // A cancellation callback may reenter ordinary/native compilation. Suspend
    // its outer request for this complete invocation, including fresh fallback.
    // This guard outlives every LLVM owner declared below and restores TLS last.
    let _native_suspension = products::Suspension::new();
    let (mut hook_state, mut attempt_timings) = match native_attempt {
        Some(NativeAttempt {
            prior,
            stats,
            replay_failed,
            products,
            timings,
        }) => (
            Some((*prior, stats, replay_failed, products)),
            Some(timings),
        ),
        None => (None, None),
    };
    check(cancelled)?;
    check_config(config)?;
    if config.optimization != Optimization::O2 && function_merging != FunctionMerging::Disabled {
        return Err(error(ErrorKind::Optimize, "function merging requires O2"));
    }
    INIT.call_once(|| {
        // SAFETY: target registry is initialized once, before worker lookups.
        unsafe {
            LLVMInitializeX86TargetInfo();
            LLVMInitializeX86Target();
            LLVMInitializeX86TargetMC();
            LLVMInitializeX86AsmPrinter();
            LLVMInitializeX86AsmParser();
        }
    });
    // SAFETY below: handles never leave this thread, owned guards release in
    // reverse order, and all C string inputs remain alive through each call.
    let context = Context(unsafe { LLVMContextCreate() });
    if context.0.is_null() {
        return Err(error(ErrorKind::Emit, "LLVMContextCreate returned NULL"));
    }
    let triple =
        CString::new(config.triple).map_err(|e| error(ErrorKind::Target, e.to_string()))?;
    let mut target = ptr::null_mut();
    let mut msg = ptr::null_mut();
    let failed = unsafe { LLVMGetTargetFromTriple(triple.as_ptr(), &mut target, &mut msg) };
    let message = unsafe { take_message(msg) };
    if failed != 0 || target.is_null() {
        return Err(error(ErrorKind::Target, message));
    }
    let optimized = config.optimization == Optimization::O2;
    // Fixed generic CPU; never inherit host features. PIC=2, Default code model=0.
    let machine = Machine(unsafe {
        LLVMCreateTargetMachine(
            target,
            triple.as_ptr(),
            c"x86-64".as_ptr(),
            c"".as_ptr(),
            if optimized { 2 } else { 0 },
            2,
            0,
        )
    });
    if machine.0.is_null() {
        return Err(error(ErrorKind::Target, "cannot create target machine"));
    }
    let layout = Layout(unsafe { LLVMCreateTargetDataLayout(machine.0) });
    if layout.0.is_null() {
        return Err(error(ErrorKind::Target, "missing target data layout"));
    }
    // LLVMLittleEndian=1, byte width=8: do not use host pointer size/alignment.
    if unsafe { LLVMPointerSize(layout.0) != 8 || LLVMByteOrder(layout.0) != 1 } {
        return Err(error(
            ErrorKind::Target,
            "target data layout violates the checked x64 little-endian profile",
        ));
    }
    let data_layout = unsafe { take_message(LLVMCopyStringRepOfTargetData(layout.0)) };
    let (module, input_kind, preflight_ns, builder_ns, parse_ns, preflight_receipt) = match input {
        #[cfg(feature = "text-reference")]
        Input::Text(ir) => {
            let t = Instant::now();
            // LLVMParseIRInContext consumes its buffer on success and failure.
            let buffer = unsafe {
                LLVMCreateMemoryBufferWithMemoryRangeCopy(
                    ir.as_ptr().cast(),
                    ir.len(),
                    c"zil-unit".as_ptr(),
                )
            };
            if buffer.is_null() {
                return Err(error(ErrorKind::Parse, "cannot allocate IR buffer"));
            }
            let mut raw = ptr::null_mut();
            let mut msg = ptr::null_mut();
            let failed = unsafe { LLVMParseIRInContext(context.0, buffer, &mut raw, &mut msg) };
            let module = Module(raw);
            let message = unsafe { take_message(msg) };
            if failed != 0 || module.0.is_null() {
                return Err(error(ErrorKind::Parse, message));
            }
            (module, "llvm-text", 0, 0, nanos(t), None)
        }
        Input::Structured(program) => {
            if program.triple != config.triple {
                return Err(error(
                    ErrorKind::Target,
                    "structured module target differs from configuration",
                ));
            }
            let t = Instant::now();
            let (schedule, receipt) = if issue_receipt {
                let (schedule, receipt) = crate::preflight_receipt::validate_for_builder(program)
                    .map_err(|message| error(ErrorKind::Verify, message))?;
                (schedule, Some(receipt))
            } else {
                let schedule = crate::preflight::validate(program)
                    .map_err(|message| error(ErrorKind::Verify, message))?;
                (schedule, None)
            };
            let preflight_ns = nanos(t);
            check(cancelled)?;
            let t = Instant::now();
            let module = builder::build(context.0, program, layout.0, &schedule, cancelled)?;
            (module, "builder", preflight_ns, nanos(t), 0, receipt)
        }
    };
    if let Some(timings) = attempt_timings.as_deref_mut() {
        timings.preflight_ns = preflight_ns;
        timings.builder_ns = builder_ns;
        timings.parse_ns = parse_ns;
    }
    verify(&module)?;
    check(cancelled)?;
    if unsafe { text(LLVMGetTarget(module.0)) } != config.triple {
        return Err(error(
            ErrorKind::Target,
            "IR target differs from the verified target configuration",
        ));
    }
    let previous = unsafe { text(LLVMGetDataLayoutStr(module.0)) };
    if !previous.is_empty() && previous != data_layout {
        return Err(error(
            ErrorKind::Target,
            "IR data layout differs from target machine",
        ));
    }
    unsafe { LLVMSetModuleDataLayout(module.0, layout.0) };
    let pipeline = match (optimized, function_merging) {
        (false, _) => c"default<O0>",
        (true, FunctionMerging::BeforeAndAfter) => c"mergefunc,default<O2>",
        (true, _) => c"default<O2>",
    };
    let opts = PassOptions(unsafe { LLVMCreatePassBuilderOptions() });
    if opts.0.is_null() {
        return Err(error(ErrorKind::Optimize, "cannot create pass options"));
    }
    unsafe {
        LLVMPassBuilderOptionsSetVerifyEach(opts.0, 0);
        LLVMPassBuilderOptionsSetLoopInterleaving(opts.0, i32::from(optimized));
        LLVMPassBuilderOptionsSetLoopVectorization(opts.0, i32::from(optimized));
        LLVMPassBuilderOptionsSetSLPVectorization(opts.0, i32::from(optimized));
        // LLVM's aliases option remains false. Do not add unnamed_addr: emitted
        // canonical functions keep their address identity and external names.
        LLVMPassBuilderOptionsSetMergeFunctions(
            opts.0,
            i32::from(function_merging != FunctionMerging::Disabled),
        );
    }
    // Config contains every adapter-controlled codegen policy. This is only a
    // native cache binding: the current checker/Builder/LLVM checks above ran.
    let product_config = hook_state.is_some().then(|| {
        format!(
            "{:?}",
            (
                super::LLVM_VERSION,
                config.triple,
                super::pipeline_identity(config.optimization, function_merging),
                "x86-64",
                "",
                &data_layout,
                "PIC",
                "default",
                optimized,
                "verify-before-after",
                "native-products-v2"
            )
        )
    });
    let mut product_scope = hook_state.as_mut().map(|(prior, stats, failed, _)| {
        products::Scope::new(
            module.0,
            machine.0,
            *prior,
            product_config
                .as_deref()
                .expect("explicit native scope config"),
            stats,
            failed,
        )
    });
    if let Some(scope) = &product_scope {
        scope.phase(1);
    }
    let t = Instant::now();
    let failure = unsafe { LLVMRunPasses(module.0, pipeline.as_ptr(), machine.0, opts.0) };
    let optimize_ns = nanos(t);
    if let Some(timings) = attempt_timings.as_deref_mut() {
        timings.optimize_ns = optimize_ns;
    }
    let replay_failed = product_scope
        .as_mut()
        .is_some_and(products::Scope::replay_failed);
    if !failure.is_null() {
        // GetErrorMessage consumes LLVMErrorRef; different message disposer.
        let msg = unsafe { LLVMGetErrorMessage(failure) };
        let message = unsafe { text(msg) };
        if !msg.is_null() {
            unsafe { LLVMDisposeErrorMessage(msg) };
        }
        return Err(error(ErrorKind::Optimize, message));
    }
    if replay_failed {
        return Err(error(
            ErrorKind::Optimize,
            "native optimization replay failed; discard request",
        ));
    }
    verify(&module)?;
    check(cancelled)?;
    if let Some(scope) = &product_scope {
        scope.phase(2);
    }
    let t = Instant::now();
    let mut object = ptr::null_mut();
    let mut msg = ptr::null_mut();
    // LLVMObjectFile=1. No file is created until the full object is returned.
    let failed = unsafe {
        LLVMTargetMachineEmitToMemoryBuffer(machine.0, module.0, 1, &mut msg, &mut object)
    };
    let object = Buffer(object);
    let message = unsafe { take_message(msg) };
    let replay_failed = product_scope
        .as_mut()
        .is_some_and(products::Scope::replay_failed);
    if let Some(timings) = attempt_timings.as_deref_mut() {
        timings.emit_ns = nanos(t);
    }
    if replay_failed {
        return Err(error(
            ErrorKind::Emit,
            "native machine replay failed; discard request",
        ));
    }
    if failed != 0 || object.0.is_null() {
        return Err(error(ErrorKind::Emit, message));
    }
    let len = unsafe { LLVMGetBufferSize(object.0) };
    let ptr = unsafe { LLVMGetBufferStart(object.0) };
    if len == 0 || len > isize::MAX as usize || ptr.is_null() {
        return Err(error(ErrorKind::Emit, "invalid object buffer"));
    }
    // SAFETY: immutable borrowed byte range from a live LLVM memory buffer.
    let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) }.to_vec();
    let emit_ns = nanos(t);
    if let Some(timings) = attempt_timings.as_deref_mut() {
        timings.emit_ns = emit_ns;
    }
    check(cancelled)?;
    let completed_products = product_scope.take().and_then(products::Scope::finish);
    drop(product_scope);
    if let Some((_, _, failed, output)) = hook_state {
        if *failed {
            return Err(error(
                ErrorKind::Emit,
                "native product finalization failed; discard request",
            ));
        }
        // Finalization is a real phase too: a cancellation arriving while the
        // native snapshots close must discard their object and product together.
        check(cancelled)?;
        *output = completed_products;
    }
    Ok((
        Object {
            bytes,
            receipt: Receipt {
                llvm_version: super::LLVM_VERSION.into(),
                triple: config.triple.into(),
                cpu: "x86-64".into(),
                features: String::new(),
                data_layout,
                pipeline: super::pipeline_identity(config.optimization, function_merging).into(),
                relocation: "PIC".into(),
                code_model: "default".into(),
                verify_before_after: true,
                input_kind: input_kind.into(),
                preflight_ns,
                builder_ns,
                parse_ns,
                optimize_ns,
                emit_ns,
                total_ns: nanos(start),
                native_products: None,
            },
        },
        preflight_receipt,
    ))
}

#[cfg(all(test, feature = "native-products"))]
mod native_product_tests {
    use super::*;
    use crate::model::{Arg, Block, Function, Inst, Item, Linkage, Op, Signature, Ty};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn module(value: i64) -> crate::model::Module {
        let function = |name: &str, value| {
            Item::Function(Function {
                name: name.into(),
                linkage: Linkage::External,
                signature: Signature::new(Ty::Int(32), Vec::new()),
                attrs: Vec::new(),
                blocks: Some(
                    vec![Block {
                        name: "entry".into(),
                        instructions: vec![Inst::new(
                            None,
                            Op::Return(Some(Arg::new(Ty::Int(32), value))),
                        )],
                    }]
                    .into(),
                ),
            })
        };
        crate::model::Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "diagnostic-only".into(),
            items: vec![function("answer", value), function("sibling", 7)].into(),
        }
    }
    fn config() -> Config<'static> {
        Config {
            triple: "x86_64-unknown-linux-gnu",
            optimization: Optimization::O2,
        }
    }
    fn compile_products(
        module: &crate::model::Module,
        prior: Option<&crate::NativeWorkProducts>,
    ) -> (Object, PreflightReceipt, Option<crate::NativeWorkProducts>) {
        compile_module_with_native_products(
            module,
            &config(),
            FunctionMerging::Disabled,
            &|| false,
            prior,
        )
        .unwrap()
    }

    #[test]
    fn native_products_keep_current_preflight_and_exact_object_for_changed_input() {
        let initial = module(42);
        let ordinary =
            compile_module(&initial, &config(), FunctionMerging::Disabled, &|| false).unwrap();
        assert!(ordinary.receipt.native_products.is_none());
        let (cold, _, products) = compile_products(&initial, None);
        assert_eq!(cold.bytes, ordinary.bytes);
        assert!(cold.receipt.verify_before_after);
        let stats = cold.receipt.native_products.unwrap();
        assert!(stats.available <= 1);
        if stats.available == 0 {
            assert!(products.is_none());
            assert_eq!(stats.optimizer_function_stage_reuses, 0);
            assert_eq!(stats.machine_function_reuses, 0);
        }
        let (warm, _, _) = compile_products(&initial, products.as_ref());
        assert_eq!(warm.bytes, ordinary.bytes);
        let changed = module(43);
        let (edited, receipt, _) = compile_products(&changed, products.as_ref());
        let reference =
            compile_module(&changed, &config(), FunctionMerging::Disabled, &|| false).unwrap();
        assert_eq!(edited.bytes, reference.bytes);
        assert_ne!(edited.bytes, ordinary.bytes);
        assert!(
            crate::validate_module_with_receipt(&changed, Some(&receipt))
                .unwrap()
                .1
        );

        let mut invalid = changed.clone();
        let Item::Function(function) = &mut invalid.items[0] else {
            panic!()
        };
        function.signature.result = Ty::Void;
        assert_eq!(
            compile_module_with_native_products(
                &invalid,
                &config(),
                FunctionMerging::Disabled,
                &|| false,
                products.as_ref()
            )
            .unwrap_err()
            .kind,
            ErrorKind::Verify
        );
        assert_eq!(
            compile_module_with_native_products(
                &initial,
                &Config {
                    triple: "x86_64-pc-windows-msvc",
                    ..config()
                },
                FunctionMerging::Disabled,
                &|| false,
                products.as_ref()
            )
            .unwrap_err()
            .kind,
            ErrorKind::Target
        );
    }

    #[test]
    fn native_products_abort_at_each_boundary_without_poisoning_the_next_request() {
        let source = module(42);
        let (reference, _, products) = compile_products(&source, None);
        for boundary in 0..6 {
            let polls = AtomicUsize::new(0);
            let cancelled = || polls.fetch_add(1, Ordering::SeqCst) >= boundary;
            assert_eq!(
                compile_module_with_native_products(
                    &source,
                    &config(),
                    FunctionMerging::Disabled,
                    &cancelled,
                    products.as_ref()
                )
                .unwrap_err()
                .kind,
                ErrorKind::Cancelled
            );
            let (recovered, _, _) = compile_products(&source, products.as_ref());
            assert_eq!(recovered.bytes, reference.bytes);
            assert!(recovered.receipt.verify_before_after);
        }
    }

    #[test]
    fn installed_native_extension_reuses_real_o2_stages_and_machine_functions() {
        let _diagnostics = products::analysis_diagnostics_for_test();
        let source = module(42);
        let (cold, _, products) = compile_products(&source, None);
        let stats = cold
            .receipt
            .native_products
            .expect("current native consumer statistics");
        assert_eq!(
            stats.available, 1,
            "strict native-products acceptance requires the audited extension DSO"
        );
        assert!(stats.optimizer_function_stage_executions > 0);
        assert_eq!(stats.machine_function_executions, 2);
        let products = products.expect("successful cold compilation publishes native products");
        let (warm, _, _) = compile_products(&source, Some(&products));
        let stats = warm.receipt.native_products.unwrap();
        assert_eq!(stats.available, 1);
        assert!(stats.optimizer_function_stage_reuses > 0);
        assert_eq!(stats.machine_function_reuses, 2);
        assert_eq!(stats.machine_function_executions, 0);
        assert_eq!(stats.fresh_context_fallbacks, 0);
        assert!(
            warm.receipt.preflight_ns > 0,
            "same-input native work still runs current preflight"
        );
        assert!(warm.receipt.verify_before_after);
        assert_eq!(warm.bytes, cold.bytes);
        assert_eq!(
            warm.bytes,
            compile_module(&source, &config(), FunctionMerging::Disabled, &|| false)
                .unwrap()
                .bytes
        );

        // Only answer's implementation changes; its signature and sibling stay
        // byte-identical. Exactly that sibling may reuse its machine function.
        let edited = module(43);
        let (changed, _, _) = compile_products(&edited, Some(&products));
        let stats = changed.receipt.native_products.unwrap();
        assert_eq!(stats.available, 1);
        assert!(stats.optimizer_function_stage_reuses > 0);
        assert!(stats.optimizer_function_stage_executions > 0);
        assert_eq!(stats.machine_function_reuses, 1);
        assert_eq!(stats.machine_function_executions, 1);
        assert_eq!(stats.fresh_context_fallbacks, 0);
        assert!(changed.receipt.preflight_ns > 0);
        assert!(changed.receipt.verify_before_after);
        assert_eq!(
            changed.bytes,
            compile_module(&edited, &config(), FunctionMerging::Disabled, &|| false)
                .unwrap()
                .bytes
        );
        assert_ne!(changed.bytes, cold.bytes);
    }

    #[test]
    fn native_product_scope_does_not_capture_a_reentrant_ordinary_compilation() {
        let source = module(42);
        let nested = module(99);
        let reference =
            compile_module(&nested, &config(), FunctionMerging::Disabled, &|| false).unwrap();
        let (cold, _, products) = compile_products(&source, None);
        assert_eq!(
            cold.receipt.native_products.unwrap().available,
            1,
            "strict lifecycle acceptance requires the audited extension DSO"
        );
        let products = products.expect("cold product");
        let polls = AtomicUsize::new(0);
        let cancelled = || {
            // The fourth boundary is after optimization, while the outer
            // native scope remains installed and before machine emission.
            if polls.fetch_add(1, Ordering::SeqCst) == 3 {
                let actual =
                    compile_module(&nested, &config(), FunctionMerging::Disabled, &|| false)
                        .unwrap();
                assert_eq!(actual.bytes, reference.bytes);
                assert!(actual.receipt.native_products.is_none());
            }
            false
        };
        let (warm, _, _) = compile_module_with_native_products(
            &source,
            &config(),
            FunctionMerging::Disabled,
            &cancelled,
            Some(&products),
        )
        .unwrap();
        assert!(polls.load(Ordering::SeqCst) >= 5);
        assert_eq!(warm.bytes, cold.bytes);
        let stats = warm.receipt.native_products.unwrap();
        assert_eq!(stats.available, 1);
        assert_eq!(stats.machine_function_reuses, 2);
        assert_eq!(stats.machine_function_executions, 0);
        assert_eq!(stats.fresh_context_fallbacks, 0);
    }

    #[test]
    fn late_native_finish_failure_discards_the_object_and_retries_in_a_fresh_context() {
        let source = module(42);
        let (cold, _, products) = compile_products(&source, None);
        assert_eq!(
            cold.receipt.native_products.unwrap().available,
            1,
            "strict late-failure acceptance requires the audited extension DSO"
        );
        let products = products.expect("cold product");
        let ordinary =
            compile_module(&source, &config(), FunctionMerging::Disabled, &|| false).unwrap();
        // Exercise each independent source of failure: the explicit Finish
        // status and a replay-failure counter even if status wrongly says OK.
        for counter_only in [false, true] {
            let _injection = products::fail_next_finish_for_test(counter_only);
            let (recovered, preflight, unpublished) = compile_products(&source, Some(&products));
            assert!(
                unpublished.is_none(),
                "failed attempt must never publish native products"
            );
            let stats = recovered.receipt.native_products.unwrap();
            assert_eq!(stats.available, 1);
            assert_eq!(stats.fresh_context_fallbacks, 1);
            if counter_only {
                assert!(stats.failed_replays > 0);
            }
            assert!(stats.fallback_ns > 0);
            assert!(recovered.receipt.total_ns >= stats.fallback_ns);
            assert!(recovered.receipt.preflight_ns > 0);
            assert!(recovered.receipt.optimize_ns > 0);
            assert!(recovered.receipt.emit_ns > 0);
            assert!(recovered.receipt.verify_before_after);
            assert_eq!(recovered.bytes, ordinary.bytes);
            assert!(
                crate::validate_module_with_receipt(&source, Some(&preflight))
                    .unwrap()
                    .1
            );
        }
        let (after, _, _) = compile_products(&source, Some(&products));
        assert_eq!(
            after
                .receipt
                .native_products
                .unwrap()
                .fresh_context_fallbacks,
            0
        );
        assert_eq!(after.bytes, ordinary.bytes);
    }
}

#[cfg(all(test, feature = "native-products"))]
mod native_machine_numbering_tests {
    use super::*;
    use crate::model::{Arg, Block, Function, Inst, Item, Linkage, Op, Signature, Ty};

    fn source() -> crate::model::Module {
        let function = |name: &str, value: i64| {
            Item::Function(Function {
                name: name.into(),
                linkage: Linkage::External,
                signature: Signature::new(Ty::Int(32), Vec::new()),
                attrs: Vec::new(),
                blocks: Some(
                    vec![Block {
                        name: "entry".into(),
                        instructions: vec![Inst::new(
                            None,
                            Op::Return(Some(Arg::new(Ty::Int(32), value))),
                        )],
                    }]
                    .into(),
                ),
            })
        };
        crate::model::Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "number-state-test".into(),
            items: vec![function("answer", 42), function("sibling", 7)].into(),
        }
    }

    #[test]
    fn scratch_number_state_checks_real_graph_updates_and_rejections() {
        // High bit proves the scratch procedure actually ran; bits 0..14 prove
        // sparse/permuted numbering, real dominators, malformed/cap rejections,
        // insertion/deletion and empty-table reallocation all checked current MF
        // state. A disabled hook, skipped path or one missing assertion fails.
        const ALL_CHECKS: u32 = 0x8000_7fff;
        let source = source();
        let config = Config {
            triple: &source.triple,
            optimization: Optimization::O2,
        };
        let ordinary =
            compile_module(&source, &config, FunctionMerging::Disabled, &|| false).unwrap();
        assert!(ordinary.receipt.native_products.is_none());
        let (cold, _, prior) = {
            let scope = products::machine_numbering_tests_for_test();
            assert_eq!(
                scope.results(),
                0,
                "new scope must not inherit another run's receipt"
            );
            let result = compile_module_with_native_products(
                &source,
                &config,
                FunctionMerging::Disabled,
                &|| false,
                None,
            )
            .unwrap();
            assert_eq!(
                scope.results(),
                ALL_CHECKS,
                "every scratch case must execute and pass"
            );
            result
        };
        let cold_stats = cold.receipt.native_products.unwrap();
        assert_eq!(
            cold_stats.available, 1,
            "requires the actual installed extension"
        );
        assert_eq!(cold_stats.machine_function_executions, 2);
        assert_eq!(cold_stats.asm_printer_runs, 2);
        assert_eq!(cold_stats.failed_replays, 0);
        assert_eq!(cold_stats.fresh_context_fallbacks, 0);
        assert!(cold.receipt.verify_before_after);
        assert_eq!(cold.bytes, ordinary.bytes);
        let prior = prior.expect("successful native snapshot");
        let warm = {
            let scope = products::machine_numbering_tests_for_test();
            assert_eq!(
                scope.results(),
                0,
                "warm checks must rerun rather than reuse the cold mask"
            );
            let (warm, _, _) = compile_module_with_native_products(
                &source,
                &config,
                FunctionMerging::Disabled,
                &|| false,
                Some(&prior),
            )
            .unwrap();
            assert_eq!(scope.results(), ALL_CHECKS);
            warm
        };
        let warm_stats = warm.receipt.native_products.unwrap();
        assert_eq!(warm_stats.available, 1);
        assert_eq!(warm_stats.machine_function_reuses, 2);
        assert_eq!(warm_stats.machine_function_executions, 0);
        assert_eq!(
            warm_stats.asm_printer_runs, 2,
            "real current emission must still execute"
        );
        assert_eq!(warm_stats.failed_replays, 0);
        assert_eq!(
            warm_stats.fresh_context_fallbacks, 0,
            "a false replay failure is not success"
        );
        assert!(warm.receipt.verify_before_after);
        assert_eq!(warm.bytes, ordinary.bytes);
        // Guard drop restores production's disabled mode; an ordinary call
        // afterwards remains uninstrumented and still emits exactly the oracle.
        let after = compile_module(&source, &config, FunctionMerging::Disabled, &|| false).unwrap();
        assert!(after.receipt.native_products.is_none());
        assert_eq!(after.bytes, ordinary.bytes);
    }
}

#[cfg(all(test, feature = "native-products", feature = "text-reference"))]
mod native_machine_alignment_tests {
    use super::*;

    // Exact fn6/fn7 bodies from the 425-function measurement workload. Keeping
    // these branches makes the actual AsmPrinter consume block alignment state.
    const SOURCE: &str = include_str!("../tests/fixtures/scalar-native-products.ll");

    fn config() -> Config<'static> {
        Config {
            triple: "x86_64-unknown-linux-gnu",
            optimization: Optimization::O2,
        }
    }

    fn compile(
        prior: Option<&crate::NativeWorkProducts>,
    ) -> (Object, crate::NativeWorkProducts, crate::NativeProductStats) {
        let mut attempt = NativeAttempt {
            prior,
            stats: crate::NativeProductStats::default(),
            replay_failed: false,
            products: None,
            timings: AttemptTimings::default(),
        };
        let (object, _) = compile_input(
            Input::Text(SOURCE),
            &config(),
            FunctionMerging::Disabled,
            &|| false,
            false,
            Some(&mut attempt),
        )
        .expect("current native compile must succeed without replay failure");
        assert!(!attempt.replay_failed);
        assert_eq!(attempt.stats.available, 1);
        assert_eq!(attempt.stats.failed_replays, 0);
        assert_eq!(attempt.stats.fresh_context_fallbacks, 0);
        assert_eq!(attempt.stats.asm_printer_runs, 2);
        assert!(object.receipt.verify_before_after);
        (
            object,
            attempt.products.expect("actual native snapshot"),
            attempt.stats,
        )
    }

    #[test]
    fn actual_block_alignment_consumers_restore_max_bytes_and_bind_policy() {
        let (unlimited, _, unlimited_stats) = {
            let _policy = products::machine_alignment_test_for_test(0);
            compile(None)
        };
        assert_eq!(unlimited_stats.machine_function_executions, 2);
        let (limited, limited_prior, limited_stats) = {
            let _policy = products::machine_alignment_test_for_test(1);
            compile(None)
        };
        assert_eq!(limited_stats.machine_function_executions, 2);
        assert_ne!(
            unlimited.bytes, limited.bytes,
            "max alignment padding must reach the actual AsmPrinter consumer"
        );
        let (warm, _, warm_stats) = {
            let _policy = products::machine_alignment_test_for_test(1);
            compile(Some(&limited_prior))
        };
        assert_eq!(warm_stats.machine_function_reuses, 2);
        assert_eq!(warm_stats.machine_function_executions, 0);
        assert_eq!(
            warm.bytes, limited.bytes,
            "warm path restores captured max-bytes state without the cold setter"
        );
        let (changed_policy, _, changed_stats) = {
            let _policy = products::machine_alignment_test_for_test(0);
            compile(Some(&limited_prior))
        };
        assert_eq!(changed_stats.machine_function_reuses, 0);
        assert_eq!(changed_stats.machine_function_executions, 2);
        assert_eq!(
            changed_policy.bytes, unlimited.bytes,
            "different current policy must invalidate the old machine product"
        );
        let (ordinary, _) = compile_input(
            Input::Text(SOURCE),
            &config(),
            FunctionMerging::Disabled,
            &|| false,
            false,
            None,
        )
        .unwrap();
        let (default_native, _, default_stats) = {
            let _disabled = products::machine_alignment_test_for_test(-1);
            compile(Some(&limited_prior))
        };
        assert_eq!(default_stats.machine_function_reuses, 0);
        assert_eq!(default_stats.machine_function_executions, 2);
        assert_eq!(
            default_native.bytes, ordinary.bytes,
            "disabled production policy must emit the ordinary LLVM object"
        );
    }
}
