//! Raw project identities and scalar interfaces; none confer verification authority.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreImport {
    pub alias: String,
    pub module: ModuleId,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramModule {
    pub core: CoreModule,
    pub imports: Vec<CoreImport>,
    pub exports: Vec<NodeId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreProgram {
    pub root: ModuleId,
    pub source_fingerprint: Fingerprint,
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedProgram {
    /// Raw semantic Suspend exports; never a physical frame-layout assertion.
    pub task_interfaces: Vec<TaskInterface>,
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
