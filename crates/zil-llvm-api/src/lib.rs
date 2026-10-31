#![deny(unsafe_code)]
//! Pinned LLVM object-generation boundary. This is NOT a ZIL safety checker.
//! One call owns one context and returns owned bytes; no LLVM pointer escapes.
//! Native support is optional for library/reference builds; the mainline CLI enables it.

pub mod model;
mod preflight;

include!(concat!(env!("OUT_DIR"), "/llvm_version.rs"));

#[cfg(feature = "native")]
#[allow(unsafe_code)]
mod native;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Optimization {
    O0,
    O2,
}

/// Machine optimization policy, independent of language verification and ABI.
/// Neither enabled policy grants permission to alias distinct function addresses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FunctionMerging {
    #[default]
    Disabled,
    AfterOptimization,
    BeforeAndAfter,
}

/// Exact pass policy used by both native receipts and object-cache namespaces.
pub const fn pipeline_identity(
    optimization: Optimization,
    function_merging: FunctionMerging,
) -> &'static str {
    match (optimization, function_merging) {
        (Optimization::O0, FunctionMerging::Disabled) => "default<O0>",
        (Optimization::O0, _) => "unsupported<O0,function-merging>",
        (Optimization::O2, FunctionMerging::Disabled) => "default<O2>",
        (Optimization::O2, FunctionMerging::AfterOptimization) => {
            "default<O2>|merge-functions=after|aliases=false"
        }
        (Optimization::O2, FunctionMerging::BeforeAndAfter) => {
            "mergefunc,default<O2>|merge-functions=after|aliases=false"
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
    compile_with_function_merging(ir, config, FunctionMerging::Disabled, cancelled)
}

/// Text-reference differential entry with an explicit, fully identified policy.
pub fn compile_with_function_merging(
    ir: &str,
    config: &Config<'_>,
    function_merging: FunctionMerging,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Object, Error> {
    #[cfg(feature = "text-reference")]
    {
        native::compile(ir, config, function_merging, cancelled)
    }
    #[cfg(not(feature = "text-reference"))]
    {
        let _ = (ir, config, function_merging, cancelled);
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
    compile_module_with_function_merging(module, config, FunctionMerging::Disabled, cancelled)
}

/// Structured machine entry with an explicit pass policy. Current preflight and
/// LLVM verification before/after optimization are identical to compile_module.
pub fn compile_module_with_function_merging(
    module: &model::Module,
    config: &Config<'_>,
    function_merging: FunctionMerging,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Object, Error> {
    #[cfg(feature = "native")]
    {
        native::compile_module(module, config, function_merging, cancelled)
    }
    #[cfg(not(feature = "native"))]
    {
        let _ = (module, config, function_merging, cancelled);
        Err(Error {
            kind: ErrorKind::Unavailable,
            message: format!("rebuild with llvm-api feature and LLVM {LLVM_VERSION}"),
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
