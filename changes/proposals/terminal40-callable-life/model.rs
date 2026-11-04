// PROPOSED SUCCESSOR DATA MODEL, not production, codecs, or acceptance.
// All referenced types are the existing public IR types. These are raw claims.
// Runtime/Wire40, TypedModuleV40, TypeIdentityV40; CT artifact/schema5.

/// Selects the persistent class domain of an actual qualified semantic type.
/// This is NEVER an actual target/root, loan, access path, or GEP.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SignatureLifeClass {
    pub base: StorageBase,
    pub path: Vec<LifeTypeProjection>,
    pub class: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SignatureLifeClassBinding {
    pub position: SignatureLifeClass,
    pub binder: StorageBinder,
}

/// Sole declared relation for a complete function/callable signature.
/// `bindings` still select actual borrowed positions. `classes` preserve nested
/// semantic class relations, including phantom positions with no target root.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StorageLifeContract {
    pub fixed_binder_count: u32,
    pub invocation_binder_count: u32,
    pub bindings: Vec<StorageLifeBinding>,
    pub classes: Vec<SignatureLifeClassBinding>,
}

// REPLACEMENT FIELDS (not additional competing contracts):
// Type::BorrowedFunctionPointer { parameters, result, life: StorageLifeContract }
// CoreCallableParameter.life: Option<StorageLifeContract>
// CallableRole.life: Option<StorageLifeContract>
// CoreFunction / TypedFunction / ClosureDefinition / ScalarFunction.storage_life
// remain Option<StorageLifeContract>, now using the extended shape above.
// DynSlot.storage_life is authoritative; remove serialized invocation_life.
// CoreLifeContract / FunctionLifeContract can remain private/source/legacy
// projections, but are never independent permission or identity authorities.
// CtTypeNode::BorrowedFunctionPointer and CtTypeConstructor BFP carry the SAME
// extended contract; physical children still name full actual parameter types.

// CT5 AMBIENT CONTEXT COMPANION. Local CtLifetimeArgument stays unchanged:
// { owner: CtTypeNodeId, binder: StorageBinder }. Its owner is always local to
// the complete graph. Source scopes use the separate namespace below.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CtAmbientScopeId(pub u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CtAmbientLifeReference {
    pub scope: CtAmbientScopeId,
    /// Full original generic parameter ordinal, including intervening kinds.
    pub parameter: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtScopeActual {
    /// Full graph interpreted in this SAME artifact ambient namespace.
    Type(CtTypeRef),
    Scalar(Constant),
    Lifetime(CtAmbientLifeReference),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtScopeInstance {
    /// Actual original lexical generic definition, not a guessed instance.
    Definition,
    /// Complete actuals in original full generic parameter order.
    /// The private genuine specialization operation supplies this vector.
    Instantiated(Vec<CtScopeActual>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtAmbientLifeScope {
    pub declaration: CtSymbolId,
    /// Real declaration/member/binder traversal in the frozen source inventory.
    /// Raw relocation only; all steps resolve before semantic key construction.
    pub binder_site: Vec<u32>,
    pub instance: CtScopeInstance,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtAmbientClassBinding {
    /// Actual local Fixed-class owner in THIS graph. Never a physical child.
    pub owner: CtTypeNodeId,
    pub class: u32,
    pub source: CtAmbientLifeReference,
}
// CtTypeRef gains ambient_classes: Vec<CtAmbientClassBinding>.
// CtArtifactProgram gains ambient_life_scopes: Vec<CtAmbientLifeScope>.
// All free Fixed classes need a unique authentic ambient mapping, or a real
// enclosing rich callable class relation. No mapping creates a root/permission.
// Type-local owner backrefs and external ambient references have DISJOINT typed
// namespaces. Schema TypeInContext still selects its one joint physical graph.
