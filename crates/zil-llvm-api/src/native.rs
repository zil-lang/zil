//! Audited raw LLVM C API boundary (LLVM tag llvmorg-17.0.6).
//! Invariants: every owned handle is released once; module dies before context;
//! ParseIR consumes its memory buffer even on failure; target refs are borrowed;
//! all LLVM work for one unit stays on one thread. No raw handle is Send/Sync.
use super::{Config, Error, ErrorKind, Object, Optimization, Receipt};
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
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Object, Error> {
    compile_input(Input::Text(ir), config, cancelled)
}
pub(super) fn compile_module(
    module: &crate::model::Module,
    config: &Config<'_>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Object, Error> {
    compile_input(Input::Structured(module), config, cancelled)
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
    if version != [17, 0, 6] {
        return Err(error(
            ErrorKind::Version,
            format!("expected LLVM 17.0.6, got {version:?}"),
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
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Object, Error> {
    let start = Instant::now();
    check(cancelled)?;
    check_config(config)?;
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
    let (module, input_kind, preflight_ns, builder_ns, parse_ns) = match input {
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
            (module, "llvm-text", 0, 0, nanos(t))
        }
        Input::Structured(program) => {
            if program.triple != config.triple {
                return Err(error(
                    ErrorKind::Target,
                    "structured module target differs from configuration",
                ));
            }
            let t = Instant::now();
            let schedule = crate::preflight::validate(program)
                .map_err(|message| error(ErrorKind::Verify, message))?;
            let preflight_ns = nanos(t);
            check(cancelled)?;
            let t = Instant::now();
            let module = builder::build(context.0, program, layout.0, &schedule, cancelled)?;
            (module, "builder", preflight_ns, nanos(t), 0)
        }
    };
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
    let pipeline = if optimized {
        c"default<O2>"
    } else {
        c"default<O0>"
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
    }
    let t = Instant::now();
    let failure = unsafe { LLVMRunPasses(module.0, pipeline.as_ptr(), machine.0, opts.0) };
    if !failure.is_null() {
        // GetErrorMessage consumes LLVMErrorRef; different message disposer.
        let msg = unsafe { LLVMGetErrorMessage(failure) };
        let message = unsafe { text(msg) };
        if !msg.is_null() {
            unsafe { LLVMDisposeErrorMessage(msg) };
        }
        return Err(error(ErrorKind::Optimize, message));
    }
    let optimize_ns = nanos(t);
    verify(&module)?;
    check(cancelled)?;
    let t = Instant::now();
    let mut object = ptr::null_mut();
    let mut msg = ptr::null_mut();
    // LLVMObjectFile=1. No file is created until the full object is returned.
    let failed = unsafe {
        LLVMTargetMachineEmitToMemoryBuffer(machine.0, module.0, 1, &mut msg, &mut object)
    };
    let object = Buffer(object);
    let message = unsafe { take_message(msg) };
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
    check(cancelled)?;
    Ok(Object {
        bytes,
        receipt: Receipt {
            llvm_version: "17.0.6".into(),
            triple: config.triple.into(),
            cpu: "x86-64".into(),
            features: String::new(),
            data_layout,
            pipeline: pipeline.to_string_lossy().into_owned(),
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
        },
    })
}
