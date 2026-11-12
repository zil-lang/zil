#![deny(unsafe_code)]
//! Pinned LLVM object-generation boundary. This is NOT a ZIL safety checker.
//! One call owns one context and returns owned bytes; no LLVM pointer escapes.
//! Native support is optional for library/reference builds; the mainline CLI enables it.

pub mod model;
mod preflight;
mod preflight_receipt;
mod shared_vec;
pub use preflight_receipt::{PreflightReceipt, validate_module_with_receipt};

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

/// Actual native hook work in this invocation. Availability is explicit: zero
/// available means this LLVM DSO did not provide the audited hook capability.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeProductStats {
    pub available: u64,
    pub optimizer_function_stage_executions: u64,
    pub optimizer_function_stage_reuses: u64,
    pub optimizer_module_pass_executions: u64,
    pub optimizer_cgscc_pass_executions: u64,
    pub analysis_reconstructions: u64,
    pub ir_key_bytes: u64,
    pub ir_snapshot_bytes: u64,
    pub ir_replay_bytes: u64,
    pub machine_function_executions: u64,
    pub machine_function_reuses: u64,
    pub machine_pass_skips: u64,
    pub asm_printer_runs: u64,
    pub mir_key_bytes: u64,
    pub mir_snapshot_bytes: u64,
    pub mir_replay_bytes: u64,
    pub retained_bytes: u64,
    pub failed_replays: u64,
    pub fresh_context_fallbacks: u64,
    pub fallback_ns: u64,
}
impl NativeProductStats {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"available\":{},\"optimizer_function_stage_executions\":{},\"optimizer_function_stage_reuses\":{},\"optimizer_module_pass_executions\":{},\"optimizer_cgscc_pass_executions\":{},\"analysis_reconstructions\":{},\"ir_key_bytes\":{},\"ir_snapshot_bytes\":{},\"ir_replay_bytes\":{},\"machine_function_executions\":{},\"machine_function_reuses\":{},\"machine_pass_skips\":{},\"asm_printer_runs\":{},\"mir_key_bytes\":{},\"mir_snapshot_bytes\":{},\"mir_replay_bytes\":{},\"retained_bytes\":{},\"failed_replays\":{},\"fresh_context_fallbacks\":{},\"fallback_ns\":{}}}",
            self.available,
            self.optimizer_function_stage_executions,
            self.optimizer_function_stage_reuses,
            self.optimizer_module_pass_executions,
            self.optimizer_cgscc_pass_executions,
            self.analysis_reconstructions,
            self.ir_key_bytes,
            self.ir_snapshot_bytes,
            self.ir_replay_bytes,
            self.machine_function_executions,
            self.machine_function_reuses,
            self.machine_pass_skips,
            self.asm_printer_runs,
            self.mir_key_bytes,
            self.mir_snapshot_bytes,
            self.mir_replay_bytes,
            self.retained_bytes,
            self.failed_replays,
            self.fresh_context_fallbacks,
            self.fallback_ns,
        )
    }
}

/// Opaque immutable snapshots and closed X86 target value state, without LLVM
/// context/module/instruction ownership. Only the opt-in native-products build
/// can construct a resident native product.
/// Equality only identifies the same retained allocation; it authorizes no reuse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeWorkProducts {
    #[cfg(all(feature = "native", feature = "native-products"))]
    inner: std::sync::Arc<native::products::Product>,
    #[cfg(not(all(feature = "native", feature = "native-products")))]
    _private: (),
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
    pub native_products: Option<NativeProductStats>,
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
    preflight::validate_only(module)
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

/// Emit an object and retain a receipt from the same successful machine
/// preflight already required by Builder. No second preflight is performed.
///
/// The selected Config, target/layout checks, LLVM verification before and
/// after optimization, cancellation, and object emission follow the ordinary
/// compile entry. Failure returns neither an object nor a receipt. The returned
/// machine receipt can authorize a later exact-input validation-only request;
/// it cannot replace any native compilation check or supply a Builder schedule.
pub fn compile_module_with_preflight_receipt(
    module: &model::Module,
    config: &Config<'_>,
    function_merging: FunctionMerging,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<(Object, PreflightReceipt), Error> {
    #[cfg(feature = "native")]
    {
        native::compile_module_with_preflight_receipt(module, config, function_merging, cancelled)
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

/// Explicit native optimization/machine work products. Current machine preflight,
/// Builder, target/layout checks and LLVM verification remain mandatory. A failed
/// replay destroys the entire LLVM request before a fresh ordinary compilation;
/// cancellation or ordinary compiler failure publishes no products.
pub fn compile_module_with_native_products(
    module: &model::Module,
    config: &Config<'_>,
    function_merging: FunctionMerging,
    cancelled: &(dyn Fn() -> bool + Sync),
    prior: Option<&NativeWorkProducts>,
) -> Result<(Object, PreflightReceipt, Option<NativeWorkProducts>), Error> {
    #[cfg(all(feature = "native", feature = "native-products"))]
    {
        native::compile_module_with_native_products(
            module,
            config,
            function_merging,
            cancelled,
            prior,
        )
    }
    #[cfg(not(all(feature = "native", feature = "native-products")))]
    {
        let _ = (module, config, function_merging, cancelled, prior);
        Err(Error {
            kind: ErrorKind::Unavailable,
            message: format!(
                "rebuild with native-products feature and audited LLVM {LLVM_VERSION}"
            ),
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

#[cfg(all(test, not(feature = "native-products")))]
mod native_products_disabled_tests {
    #[test]
    fn explicit_native_product_entry_is_unavailable_without_its_feature() {
        let module = crate::model::Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "disabled-feature".into(),
            items: Vec::new().into(),
        };
        let error = crate::compile_module_with_native_products(
            &module,
            &crate::Config {
                triple: &module.triple,
                optimization: crate::Optimization::O2,
            },
            crate::FunctionMerging::Disabled,
            &|| false,
            None,
        )
        .unwrap_err();
        assert_eq!(error.kind, crate::ErrorKind::Unavailable);
        assert!(error.message.contains("native-products feature"));
    }
}
