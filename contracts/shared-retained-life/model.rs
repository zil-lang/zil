/// Metadata-only traversal of a complete semantic type. These selectors never
/// authorize a byte address. Every edge must match the actual constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LifeTypeProjection {
    Place(PlaceProjection),
    TypeArgument(u32),
    /// Full typed opaque-port leaf; metadata only.
    Port(u32),
    /// Fixed associated type of an actual dyn interface.
    AssociatedType(u32),
    Pointee,
    Element,
    Parameter(u32),
    Result,
}

/// Raw declaration reference, resolved through the actual declaration inventory.
/// Cross-request identity uses its stable module/declaration identity, not these
/// relocation numbers or the compiler flag alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LifeDeclarationId {
    pub module: ModuleId,
    pub declaration: NodeId,
    pub compiler: bool,
}

/// One nested carrier CLASS maps once into its enclosing fixed domain. Several
/// actual ports may share this class; they are not independently freshened.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StorageRetainedBinding {
    pub path: Vec<LifeTypeProjection>,
    pub class: u32,
    pub binder: u32,
}

/// Full nominal actual-position identity, including phantom lifetime actuals.
/// `argument` is the position in ALL ordered generic arguments, not the index
/// among lifetime arguments. The selected declaration and kind must agree.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NominalLifeArgument {
    pub path: Vec<LifeTypeProjection>,
    pub declaration: LifeDeclarationId,
    pub argument: u32,
    pub binder: u32,
}

/// A typed abstract borrowed position of an opaque carrier. The port is not a
/// physical field and cannot be loaded by a PlaceProjection operation.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RetainedBorrowPort {
    pub leaf: Type,
    pub mode: StoredBorrowMode,
    pub binder: u32,
}

/// Persistent class domain of an opaque carrier such as dyn or a borrowed Task.
/// The complete owner contributes nominal/trait/result class occurrences when
/// checking canonical numbering; a phantom occurrence never supplies a root.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RetainedLifeShape {
    pub binder_count: u32,
    pub ports: Vec<RetainedBorrowPort>,
    pub retained: Vec<StorageRetainedBinding>,
    pub nominal_arguments: Vec<NominalLifeArgument>,
}

/// Untrusted mapping to actual constructor/entry parameter or capture storage.
/// The concrete owner (vtable or task worker) determines the legal base domain.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RetainedPortBinding {
    pub port: u32,
    pub source: StoragePosition,
}
