//! Raw project identities and scalar interfaces; none confer verification authority.
use super::*;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreImport {
    pub alias: String,
    pub module: ModuleId,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramModule {
    /// Immutable raw module storage; sharing confers no verification authority.
    /// Raw transformations use `Arc::make_mut` before modifying a snapshot.
    pub core: Arc<CoreModule>,
    pub imports: Vec<CoreImport>,
    pub exports: Vec<NodeId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreProgram {
    pub root: ModuleId,
    pub source_fingerprint: Fingerprint,
    /// Each module carries its complete dyn/declaration inventory. Raw source
    /// module IDs remain lookup/relocation data, never stable semantic names.
    pub modules: Vec<ProgramModule>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScalarFunction {
    pub storage_life: Option<StorageLifeContract>,
    /// Declared address-return upper bound, included in signature identity.
    /// Verified actual result origins are published separately.
    pub reference_return_bound: Option<Vec<u32>>,
    pub name: String,
    pub function: FunctionId,
    pub parameters: Vec<(PassingMode, Type)>,
    pub result: Type,
    pub result_mode: PassingMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScalarInterface {
    /// Unavailable before independent verification of every factory definition.
    pub callable_returns: Option<Vec<FunctionCallableReturn>>,
    pub module: ModuleId,
    pub exports: Vec<ScalarFunction>,
    pub signature_fingerprint: Fingerprint,
    pub implementation_fingerprint: Fingerprint,
    /// Absent at Types/Analyze; populated from an independently verified module.
    pub result_origins: Option<Vec<FunctionResultOrigins>>,
    pub contract_fingerprint: Option<Fingerprint>,
    /// Absent before Verify; only checked concrete helper instances are published.
    pub callable_contracts: Option<Vec<FunctionCallableContract>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionResultOrigins {
    pub name: String,
    /// Input parameter positions whose targets may be returned; scalar results are empty.
    pub parameters: Vec<u32>,
    /// Independently verified program-static read-only storage may be returned.
    /// An empty parameter set alone never implies this fact.
    pub static_storage: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedProgram {
    /// Raw semantic Suspend exports; never a physical frame-layout assertion.
    pub task_interfaces: Vec<TaskInterface>,
    /// Unified callable CFG plus all source-module dyn/life inventories. Every
    /// CoreDynEntry resolves through the same actual (module,NodeId)->FunctionId
    /// map as ordinary calls. All slots and cleanup entries must relocate; a
    /// missing/private/nonmatching entry rejects, never defaults to function0.
    pub module: TypedModule,
    pub interfaces: Vec<ScalarInterface>,
    pub root_functions: Vec<ScalarFunction>,
}

/// Untrusted semantic interface candidate. Verify rebuilds each worker profile;
/// Lower/backend publish physical frame descriptors separately after layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskInterface {
    pub module: ModuleId,
    pub exports: Vec<TaskExport>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskExport {
    pub name: String,
    pub worker: FunctionId,
    pub parameter: (PassingMode, Type),
    pub result: (PassingMode, Type),
    pub abi_version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionCallableContract {
    pub name: String,
    pub contract: CallableParameterContract,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionCallableReturn {
    pub name: String,
    pub contract: CallableReturnContract,
}
