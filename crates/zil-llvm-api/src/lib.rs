#![deny(unsafe_code)]
//! Pinned LLVM object-generation boundary. This is NOT a ZIL safety checker.
//! One call owns one context and returns owned bytes; no LLVM pointer escapes.
//! Native support is optional for library/reference builds; the mainline CLI enables it.

mod late_closed;
pub mod model;
pub use late_closed::{LateClosedPlan, LateClosedReport};
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

/// Independent V1 observations of the first hooked attempt. Captured functions
/// and capacity count sealed pending products, not proof of publication. Block
/// counts can include partial captures later declined. Replays still execute current
/// AsmPrinter boundaries, original CFI and the current assembler. These timers
/// are nested scopes, not a partition of the object emission cost.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeMachineEmissionStats {
    pub available: u64,
    pub captured_functions: u64,
    pub captured_blocks: u64,
    pub replayed_functions: u64,
    pub replayed_blocks: u64,
    pub machine_instructions_skipped: u64,
    pub mc_replay_events: u64,
    pub current_cfi_events: u64,
    pub declined_functions: u64,
    pub budget_declines: u64,
    pub captured_capacity_bytes: u64,
    pub capture_ns: u64,
    pub preflight_ns: u64,
    pub replay_ns: u64,
    pub failed_replays: u64,
}
impl NativeMachineEmissionStats {
    fn to_json(&self) -> String {
        format!(
            "{{\"observer_schema\":1,\"available\":{},\"captured_functions\":{},\"captured_blocks\":{},\"replayed_functions\":{},\"replayed_blocks\":{},\"machine_instructions_skipped\":{},\"mc_replay_events\":{},\"current_cfi_events\":{},\"declined_functions\":{},\"budget_declines\":{},\"captured_capacity_bytes\":{},\"capture_ns\":{},\"preflight_ns\":{},\"replay_ns\":{},\"failed_replays\":{}}}",
            self.available,
            self.captured_functions,
            self.captured_blocks,
            self.replayed_functions,
            self.replayed_blocks,
            self.machine_instructions_skipped,
            self.mc_replay_events,
            self.current_cfi_events,
            self.declined_functions,
            self.budget_declines,
            self.captured_capacity_bytes,
            self.capture_ns,
            self.preflight_ns,
            self.replay_ns,
            self.failed_replays,
        )
    }
}

/// Actual native hook work in this invocation. Availability is explicit: zero
/// available means this LLVM DSO did not provide the audited hook capability.
/// Hook counters include a discarded attempt if fresh_context_fallbacks is nonzero.
/// Its subsequent ordinary compilation has no per-pass hook observations; those
/// counts are unavailable, not zero. Receipt phase/total times include both calls.
/// Reuse in a discarded attempt is not work avoided by the final compilation.
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
    // Actual scoped MIR costs, not a partition of total native time. Dispatch,
    // optimization, emission and other unassigned work remain in receipt totals.
    pub mir_key_ns: u64,
    /// Combined reconstruction: tiny MIR header parsing, typed current
    /// MI/MBB/use-chain construction, and parser/header destruction. This is
    /// not pure YAML parsing time; restoration/validation below is excluded.
    pub mir_parse_ns: u64,
    /// Target, numbering and block-state restoration, typed-body recapture
    /// equality, current domain/MachineVerifier checks, and full MIR equality.
    pub mir_restore_validate_ns: u64,
    pub mir_capture_ns: u64,
    pub retained_bytes: u64,
    pub failed_replays: u64,
    pub fresh_context_fallbacks: u64,
    pub fallback_ns: u64,
    pub world_projection_direct: u64,
    pub world_projection_fallback: u64,
    pub tli_projection_builds: u64,
    pub tli_projection_reuses: u64,
    /// Supported-entry eligibility and complete current IR/analysis key creation.
    pub ir_key_ns: u64,
    /// Complete selected-slot transport decode and typed metadata reconstruction.
    pub ir_unpack_ns: u64,
    /// Complete replay, including parsing, cloning, validation, AM and cleanup.
    pub ir_replay_ns: u64,
    /// MemoryBuffer and parseBitcodeFile construction only; subset of replay.
    /// Excludes subsequent parsed Module destruction. Do not add to replay.
    pub ir_parse_ns: u64,
    /// Complete after-stage callback, including early-return gates and packing.
    pub ir_capture_ns: u64,
    /// New-stage admission/frame/SHA/zstd/typed metadata/publication; subset of
    /// capture. Do not add to capture. All six are scopes, not a time partition.
    pub ir_pack_ns: u64,
    /// Independent V1 body observations; absent if its SDK capability is missing.
    /// On a fallback these describe only the discarded hooked attempt.
    pub machine_emission: Option<NativeMachineEmissionStats>,
}
impl NativeProductStats {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"available\":{},\"optimizer_function_stage_executions\":{},\"optimizer_function_stage_reuses\":{},\"optimizer_module_pass_executions\":{},\"optimizer_cgscc_pass_executions\":{},\"analysis_reconstructions\":{},\"ir_key_bytes\":{},\"ir_snapshot_bytes\":{},\"ir_replay_bytes\":{},\"machine_function_executions\":{},\"machine_function_reuses\":{},\"machine_pass_skips\":{},\"asm_printer_runs\":{},\"mir_key_bytes\":{},\"mir_snapshot_bytes\":{},\"mir_replay_bytes\":{},\"mir_key_ns\":{},\"mir_parse_ns\":{},\"mir_restore_validate_ns\":{},\"mir_capture_ns\":{},\"retained_bytes\":{},\"failed_replays\":{},\"fresh_context_fallbacks\":{},\"fallback_ns\":{},\"observer_schema\":3,\"world_projection_direct\":{},\"world_projection_fallback\":{},\"tli_projection_builds\":{},\"tli_projection_reuses\":{},\"ir_key_ns\":{},\"ir_unpack_ns\":{},\"ir_replay_ns\":{},\"ir_parse_ns\":{},\"ir_capture_ns\":{},\"ir_pack_ns\":{},\"machine_emission\":{}}}",
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
            self.mir_key_ns,
            self.mir_parse_ns,
            self.mir_restore_validate_ns,
            self.mir_capture_ns,
            self.retained_bytes,
            self.failed_replays,
            self.fresh_context_fallbacks,
            self.fallback_ns,
            self.world_projection_direct,
            self.world_projection_fallback,
            self.tli_projection_builds,
            self.tli_projection_reuses,
            self.ir_key_ns,
            self.ir_unpack_ns,
            self.ir_replay_ns,
            self.ir_parse_ns,
            self.ir_capture_ns,
            self.ir_pack_ns,
            self.machine_emission
                .map_or_else(|| "null".into(), |stats| stats.to_json()),
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
    #[cfg(all(feature = "native", feature = "native-products"))]
    retained_payload_bytes: u64,
    #[cfg(not(all(feature = "native", feature = "native-products")))]
    _private: (),
}

impl NativeWorkProducts {
    /// Encoded payload measured by the native producer, for optional cache
    /// retention admission. This is not total allocator usage or resident RSS.
    pub fn retained_payload_bytes(&self) -> u64 {
        #[cfg(all(feature = "native", feature = "native-products"))]
        {
            self.retained_payload_bytes
        }
        #[cfg(not(all(feature = "native", feature = "native-products")))]
        {
            0
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub late_closed: Option<LateClosedReport>,
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

/// Explicit no-external-ZIL-export executable policy. The original selected
/// O2 pipeline completes first; only then may zero-use, closure-checked names
/// be internalized and removed by a separate GlobalDCE pass. Current preflight,
/// target/layout, cancellation and all LLVM verification remain mandatory.
pub fn compile_module_with_late_closed(
    module: &model::Module,
    config: &Config<'_>,
    function_merging: FunctionMerging,
    cancelled: &(dyn Fn() -> bool + Sync),
    plan: &LateClosedPlan,
    native_products: bool,
    prior: Option<&NativeWorkProducts>,
) -> Result<(Object, PreflightReceipt, Option<NativeWorkProducts>), Error> {
    #[cfg(feature = "native")]
    {
        native::compile_module_with_late_closed(
            module,
            config,
            function_merging,
            cancelled,
            plan,
            native_products,
            prior,
        )
    }
    #[cfg(not(feature = "native"))]
    {
        let _ = (
            module,
            config,
            function_merging,
            cancelled,
            plan,
            native_products,
            prior,
        );
        Err(Error {
            kind: ErrorKind::Unavailable,
            message: "late closed executable requires native LLVM".into(),
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

#[cfg(test)]
mod native_product_observer_tests {
    #[test]
    fn json_observer_v3_retains_all_six_ir_scopes() {
        let stats = super::NativeProductStats {
            ir_key_ns: 31,
            ir_unpack_ns: 32,
            ir_replay_ns: 40,
            ir_parse_ns: 34,
            ir_capture_ns: 50,
            ir_pack_ns: 36,
            ..Default::default()
        };
        let json = stats.to_json();
        assert!(json.contains("\"observer_schema\":3"));
        assert!(json.contains("\"ir_key_ns\":31"));
        assert!(json.contains("\"ir_unpack_ns\":32"));
        assert!(json.contains("\"ir_replay_ns\":40"));
        assert!(json.contains("\"ir_parse_ns\":34"));
        assert!(json.contains("\"ir_capture_ns\":50"));
        assert!(json.contains("\"ir_pack_ns\":36"));
    }
}
