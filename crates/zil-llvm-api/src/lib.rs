#![deny(unsafe_code)]
//! LLVM 17.0.6 object-generation boundary. This is NOT a ZIL safety checker.
//! One call owns one context and returns owned bytes; no LLVM pointer escapes.
//! Native support is optional for library/reference builds; the mainline CLI enables it.

pub mod model;
mod preflight;
pub mod timing;

#[cfg(feature = "native")]
#[allow(unsafe_code)]
mod native;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Optimization {
    O0,
    O2,
}

impl Optimization {
    /// The exact fixed pass recipe used by native emission and object-cache keys.
    pub const fn pipeline(self) -> &'static std::ffi::CStr {
        match self {
            Self::O0 => c"default<O0>,function(mem2reg)",
            Self::O2 => c"default<O2>",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config<'a> {
    pub triple: &'a str,
    pub optimization: Optimization,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub llvm_version: String,
    pub triple: String,
    pub cpu: String,
    pub features: String,
    pub data_layout: String,
    pub pipeline: String,
    pub relocation: String,
    pub code_model: String,
    pub verify_before_after: bool,
    pub input_kind: String,
    pub preflight_ns: u64,
    pub builder_ns: u64,
    pub parse_ns: u64,
    pub optimize_ns: u64,
    pub emit_ns: u64,
    pub total_ns: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Object {
    pub bytes: Vec<u8>,
    pub receipt: Receipt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Unavailable,
    Version,
    Target,
    Parse,
    Verify,
    Optimize,
    Emit,
    Cancelled,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}
impl std::error::Error for Error {}

pub const fn available() -> bool {
    cfg!(feature = "native")
}

/// Cancellation is cooperative at phase boundaries, never an unsafe interruption
/// inside LLVM. The caller must recheck before publishing the returned object.
/// Legacy differential-test entry; disabled in the standard CLI.
/// Input must be trusted compiler-generated IR. Parse/verifier errors are returned
/// but this wrapper does not sandbox LLVM bugs or make hostile IR a safe input.
pub fn compile(
    ir: &str,
    config: &Config<'_>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Object, Error> {
    #[cfg(feature = "text-reference")]
    {
        native::compile(ir, config, cancelled)
    }
    #[cfg(not(feature = "text-reference"))]
    {
        let _ = (ir, config, cancelled);
        Err(Error {
            kind: ErrorKind::Unavailable,
            message: "text parsing is reference-only; use compile_module or enable text-reference"
                .into(),
        })
    }
}

/// Check the machine representation before any LLVMBuilder operation.
/// This is not and cannot create a ZIL VerifiedModule capability.
pub fn validate_module(module: &model::Module) -> Result<(), Error> {
    preflight::validate(module)
        .map(|_| ())
        .map_err(|message| Error {
            kind: ErrorKind::Verify,
            message,
        })
}

/// Generate an object directly from immutable, structured machine IR.
/// No assembly serialization or LLVMParseIRInContext is involved. Machine
/// preflight and the LLVM verifier are not substitutes for the caller's ZIL Verify.
pub fn compile_module(
    module: &model::Module,
    config: &Config<'_>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Object, Error> {
    compile_module_with_trace(module, config, cancelled, &timing::TimingTrace::default())
}

/// Optional diagnostic intervals, including failures and native-handle cleanup.
/// This does not change Config, object identity, verification, or optimization.
pub fn compile_module_with_trace(
    module: &model::Module,
    config: &Config<'_>,
    cancelled: &(dyn Fn() -> bool + Sync),
    trace: &timing::TimingTrace,
) -> Result<Object, Error> {
    #[cfg(feature = "native")]
    {
        trace.measure("llvm", "stage", |trace| {
            native::compile_module(module, config, cancelled, trace)
        })
    }
    #[cfg(not(feature = "native"))]
    {
        let _ = (module, config, cancelled, trace);
        Err(Error {
            kind: ErrorKind::Unavailable,
            message: "rebuild with llvm-api feature and LLVM 17.0.6".into(),
        })
    }
}

/// Runtime checks and the resolved LLVM library for a trusted local cache.
/// This does not create a module or bypass per-request language verification.
pub fn cache_runtime(config: &Config<'_>) -> Result<(String, std::path::PathBuf), Error> {
    #[cfg(all(feature = "native", target_os = "linux"))]
    {
        let v = native::check_config(config)?;
        Ok((
            format!("{}.{}.{}", v[0], v[1], v[2]),
            native::runtime_library()?,
        ))
    }
    #[cfg(not(all(feature = "native", target_os = "linux")))]
    {
        let _ = config;
        Err(Error {
            kind: ErrorKind::Unavailable,
            message: "local object cache currently requires a Linux LLVM API host".into(),
        })
    }
}
