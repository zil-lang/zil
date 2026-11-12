#![forbid(unsafe_code)]

//! Public, untrusted compiler schemas. Constructing these records never grants
//! safety authority. Only `zil-verify` can produce an immutable verified wrapper.

use zil_base::{
    Fingerprint, FingerprintBuilder, ModuleId, NodeId, SCHEMA_VERSION, Span, TargetProfile,
};

mod closure_context;
pub mod ct_artifact;
pub mod ct_wire;
pub use closure_context::*;
mod dynamic;
mod dynamic_fingerprint;
pub use dynamic::*;
use dynamic_fingerprint::*;
mod persistent_vec;
mod program;
pub use persistent_vec::{PersistentItem, PersistentVec};
mod shared_vec;
pub use shared_vec::SharedVec;
pub mod wire;
pub use program::*;

/// Reserved raw identity for the sealed byte storage resource. Not authority.
pub const BYTE_BUFFER_RESOURCE_NAME: &str = "@builtin:ByteBuffer";
/// Exact sealed scalar cooperative-task resource identity, never producer authority.
pub const TASK_I32_RESOURCE_NAME: &str = "@builtin:TaskI32";
/// General task family prefix; actual result identity and every responsibility remain untrusted.
pub const TASK_RESOURCE_NAME_PREFIX: &str = "@builtin:Task:";

macro_rules! identity {
    ($($name:ident),+ $(,)?) => {$(
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u32);
    )+};
}
identity!(
    FunctionId,
    LocalId,
    ValueId,
    BlockId,
    PlaceId,
    ClosureId,
    OriginId,
    ScopeId,
    ReservationId
);

/// Raw callable ABI/effect selection. Both kernels rebuild actual reachable effects.
/// Suspend may be explicitly declared even when an implementation does not yield.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CallKind {
    Direct = 0,
    Suspend = 1,
}

/// Sealed native cell kind; this raw tag grants no permission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SyncCellKind {
    MutexI32 = 0,
    AtomicI32 = 1,
    AtomicI64 = 2,
    AtomicU64 = 3,
    AtomicUsize = 4,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SyncI32Op {
    Load = 0,
    Store = 1,
    FetchAdd = 2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SyncIntegerOp {
    Load = 0,
    Store = 1,
    FetchAdd = 2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AtomicOrdering {
    Relaxed = 0,
    Acquire = 1,
    Release = 2,
    AcqRel = 3,
    SeqCst = 4,
}

/// Canonical invocation-local address relation. Numeric binders describe no
/// runtime object or access authority.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FunctionLifeContract {
    pub binder_count: u32,
    pub parameter_lives: Vec<Option<u32>>,
    pub result_life: Option<u32>,
}

/// Untrusted permitted leaf access. Desc variants are uppers, not authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StoredBorrowMode {
    SharedAddress,
    ExclusiveAddress,
    ReadOnlyDescription,
    WritableDescription,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StorageLifeSlot {
    pub path: Vec<PlaceProjection>,
    pub binder: u32,
    pub mode: StoredBorrowMode,
}
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

/// Canonical dense fixed classes across actual stored leaves; never target roots.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StorageLifeShape {
    pub binder_count: u32,
    pub slots: Vec<StorageLifeSlot>,
    pub retained: Vec<StorageRetainedBinding>,
    pub nominal_arguments: Vec<NominalLifeArgument>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StorageBase {
    Parameter(u32),
    Capture(u32),
    Result,
}
/// Contract-only selection; retained ports/classes never authorize byte access.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContractProjection {
    Place(PlaceProjection),
    /// Typed opaque port selection for access/effect/result substitution.
    RetainedPort(u32),
    /// Fixed class selection for binding only, never read/write/manage access.
    RetainedClass(u32),
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StoragePosition {
    pub base: StorageBase,
    pub path: Vec<ContractProjection>,
}
/// Existing Fixed classes cannot be narrowed or freshened by an invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StorageBinder {
    Fixed(u32),
    Invocation(u32),
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StorageLifeBinding {
    pub position: StoragePosition,
    pub binder: StorageBinder,
}
/// Selects one persistent semantic class of an actual qualified type. This is
/// not an access selector, target root, loan, GEP or retained permission port.
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
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StorageLifeContract {
    pub fixed_binder_count: u32,
    pub invocation_binder_count: u32,
    pub bindings: Vec<StorageLifeBinding>,
    pub classes: Vec<SignatureLifeClassBinding>,
}
impl StorageLifeContract {
    /// Lossless raw embedding of a legacy outer-address relation. Callers must
    /// prepay the input/output traversal; this does not validate the signature.
    pub fn from_flat(life: FunctionLifeContract) -> Option<Self> {
        if life.parameter_lives.len() > u32::MAX as usize {
            return None;
        }
        let mut bindings = Vec::with_capacity(life.parameter_lives.len().saturating_add(1));
        for (index, binder) in life.parameter_lives.into_iter().enumerate() {
            if let Some(binder) = binder {
                bindings.push(StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Parameter(index as u32),
                        path: Vec::new(),
                    },
                    binder: StorageBinder::Invocation(binder),
                });
            }
        }
        if let Some(binder) = life.result_life {
            bindings.push(StorageLifeBinding {
                position: StoragePosition {
                    base: StorageBase::Result,
                    path: Vec::new(),
                },
                binder: StorageBinder::Invocation(binder),
            });
        }
        Some(Self {
            fixed_binder_count: 0,
            invocation_binder_count: life.binder_count,
            bindings,
            classes: Vec::new(),
        })
    }
    /// Exact raw outer-address projection, for consumers with that explicit
    /// limited profile. Rich classes, Fixed domains, captures and nonempty paths
    /// cannot disappear through this adapter. The caller bounds/prepays output
    /// arity and independently checks the actual complete signature afterward.
    pub fn to_flat(&self, parameters: usize) -> Option<FunctionLifeContract> {
        if self.fixed_binder_count != 0
            || !self.classes.is_empty()
            || parameters > u32::MAX as usize
        {
            return None;
        }
        let mut parameter_lives = vec![None; parameters];
        let mut result_life = None;
        let mut last_parameter = None;
        let mut saw_result = false;
        for binding in &self.bindings {
            if !binding.position.path.is_empty() {
                return None;
            }
            let StorageBinder::Invocation(index) = binding.binder else {
                return None;
            };
            if index >= self.invocation_binder_count {
                return None;
            }
            let slot = match binding.position.base {
                StorageBase::Parameter(index) => {
                    if saw_result || last_parameter.is_some_and(|previous| previous >= index) {
                        return None;
                    }
                    last_parameter = Some(index);
                    parameter_lives.get_mut(index as usize)?
                }
                StorageBase::Result => {
                    saw_result = true;
                    &mut result_life
                }
                StorageBase::Capture(_) => return None,
            };
            if slot.replace(index).is_some() {
                return None;
            }
        }
        Some(FunctionLifeContract {
            binder_count: self.invocation_binder_count,
            parameter_lives,
            result_life,
        })
    }
    /// Complete untrusted relation identity. Validate and prepay every row and
    /// selector before hashing; this fingerprint grants no borrowing authority.
    pub fn fingerprint(&self) -> Fingerprint {
        let mut bytes = FingerprintStream::new();
        bytes.extend_from_slice(b"ZIL-STORAGE-LIFE-CONTRACT-2\0");
        encode_required_storage_contract(&mut bytes, self);
        bytes.finish()
    }
}
/// Actual permission, initialization, target/parent loans and epochs are checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoredBorrowLoad {
    CopyShared,
    CopyDescription,
    Reborrow { mutable: bool },
    Take,
}

/// Stable cross-request spelling of a REAL source/generated declaration. This
/// is identity data, never visibility, builtin, trait or borrowing authority.
/// The producer obtains it through SourceIndex/Origins, not parsed type names.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LifeDeclarationIdentity {
    pub package: String,
    pub module_path: Vec<String>,
    pub declaration_path: Vec<String>,
    pub generation_path: Vec<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LifeNominalKind {
    Record,
    Package,
    Resource,
    Enum,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LifeParameterKind {
    Type,
    Const(Type),
    Lifetime,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LifeNominalDeclaration {
    pub id: LifeDeclarationId,
    pub identity: LifeDeclarationIdentity,
    pub kind: LifeNominalKind,
    /// Full original declaration order, including phantom parameters.
    pub parameters: Vec<LifeParameterKind>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LifeNominalActual {
    Type(Type),
    Const(Constant),
    /// The instance type's OWN qualified fixed scope, not an enclosing numeric
    /// class coincidentally having this ordinal. Outer maps explicitly compose.
    Lifetime(u32),
}
/// Complete semantic instance descriptor. Unique exact qualified Type mapping;
/// duplicate type -> different declaration/actual vectors must reject.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LifeNominalInstance {
    pub declaration: LifeDeclarationId,
    pub ty: Type,
    pub arguments: Vec<LifeNominalActual>,
}

/// Complete declaration context for table-bearing stored types. Callers must
/// validate and prepay the full inventory, including each type and identity.
/// This raw context digest is not an independent semantic or permission proof.
pub fn life_inventory_fingerprint(
    declarations: &[LifeNominalDeclaration],
    instances: &[LifeNominalInstance],
) -> Fingerprint {
    let mut bytes = FingerprintStream::new();
    bytes.extend_from_slice(b"ZIL-LIFE-INVENTORY-V46\0");
    encode_life_inventory(&mut bytes, declarations, instances);
    bytes.finish()
}

/// Scalar B0 checking is deliberately a subset of this raw schema. All other
/// variants must be diagnosed as Unsupported until their checker is implemented.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Type {
    /// Unsized pointee. Address<Dyn> is the fat (environment,vtable) address.
    /// Raw references require the complete declaration/interface context.
    Dyn {
        interface: DynInterfaceId,
    },
    /// Affine two-pointer owner of an explicitly allocated concrete dyn Self.
    OwnedDyn {
        interface: DynInterfaceId,
    },
    /// Relational shape; machine layout is exactly the actual storage layout.
    BorrowedStorage {
        storage: Box<Type>,
        life: StorageLifeShape,
    },
    Unit,
    Bool,
    I32,
    I64,
    U64,
    /// Target pointer-width unsigned integer; constants retain mathematical bits.
    Usize,
    F32,
    F64,
    U8,
    U16,
    U32,
    I8,
    I16,
    Isize,
    /// Sealed dynamic byte-storage payload, legal only in Managed/Desc and its
    /// exact resource definition. Not a Copy value or a forgeable raw pointer.
    ByteBuffer,
    /// Sealed dynamic storage payload. Only an exact Managed/Desc wrapper and
    /// independently checked element/resource definitions may establish a head.
    OwnedStore {
        element: Box<Type>,
    },
    /// An opaque external token, never a ZIL address or an ownership proof.
    ForeignHandle,
    /// Opaque native payload, only available behind a checked shared address.
    SyncCell(SyncCellKind),
    /// Stable owned mutex payload. A Desc names the head; only a checked guard
    /// may access the protected value. The payload type alone grants no Share.
    Mutex {
        payload: Box<Type>,
    },
    /// Affine inline unlock obligation tied to one actual mutex head. The
    /// retained class records the parent lifetime, not execution authority.
    MutexGuard {
        payload: Box<Type>,
        retained_life: RetainedLifeShape,
    },
    /// Stable retained callback payload; only an exact Managed/Desc wrapper is legal.
    /// The actual environment must independently resolve to a self-contained Closure.
    Registration {
        environment: Box<Type>,
    },
    /// Untrusted pollable same-thread registration payload. It grants no
    /// ClosedEnvironment, no-future, synchronous Drop or frame admission proof.
    AsyncRegistration {
        environment: Box<Type>,
    },
    /// Sealed stable task frame payload, legal only under Managed/Desc or an exact unique head address.
    TaskI32,
    /// Sealed stable owning task payload. Result T does not qualify hidden frame
    /// inputs/captures/children. Empty shape still requires actual PC0/state proof.
    /// The shared retained shape carries no Transfer, Share, close or Drop proof.
    Task {
        result: Box<Type>,
        retained_life: RetainedLifeShape,
    },
    /// Readonly static UTF-16 plus NUL, or null. No foreign return admission.
    CWideString,
    /// Static C ABI code only; deliberately distinct from FunctionPointer.
    ForeignCallback {
        parameters: Vec<Type>,
        result: Box<Type>,
    },
    Tuple(Vec<Type>),
    Array {
        element: Box<Type>,
        length: u32,
    },
    /// Unsized pointee only. A legal borrowed slice is Address<Slice<T>>;
    /// this raw shape never grants a lifetime or an access permission.
    Slice {
        element: Box<Type>,
    },
    Named(String),
    /// A resolved, nominal, finite Copy record. Name includes its module identity.
    Record {
        name: String,
        fields: Vec<(String, Type)>,
    },
    /// A nominal, finite, non-Copy value. SSA/ABI values are always complete;
    /// local storage may temporarily hold a checked subset of owning fields.
    /// At least one field is Managed; the remainder follow the independently
    /// checked finite Copy / complete owning-array field contract.
    Package {
        name: String,
        fields: Vec<(String, Type)>,
    },
    /// Nominal, finite tagged union. Variant numbers follow declaration order.
    Enum {
        name: String,
        variants: Vec<(String, Vec<Type>)>,
    },
    /// Synchronous tagged success/error value; its two alternatives stay distinct.
    ErrorUnion {
        error: Box<Type>,
        result: Box<Type>,
    },
    Address {
        mutable: bool,
        inner: Box<Type>,
    },
    Managed(Box<Type>),
    Desc(Box<Type>),
    FunctionPointer {
        parameters: Vec<Type>,
        result: Box<Type>,
    },
    /// Thin code pointer with a fresh address relation at every invocation.
    /// Its target, complete signature and actual return sources remain untrusted.
    BorrowedFunctionPointer {
        parameters: Vec<Type>,
        result: Box<Type>,
        life: StorageLifeContract,
    },
    Closure(ClosureId),
    Opaque {
        identity: NodeId,
        hidden: Box<Type>,
    },
    Unsupported(String),
}
impl Type {
    /// Raw structural identity, never a proof of type or access validity.
    /// Table-bearing forms (Dyn and qualified nominal life inventory) require
    /// a context-aware semantic key for exports/cross-request caches. Raw table
    /// relocation numbers here are not independently meaningful identities.
    /// Untrusted callers must validate and prepay the type traversal first.
    pub fn fingerprint(&self) -> Fingerprint {
        let mut bytes = FingerprintStream::new();
        bytes.extend_from_slice(b"ZIL-TYPE-IDENTITY-V46\0");
        encode_type(&mut bytes, self);
        bytes.finish()
    }
    pub fn is_b0_scalar(&self) -> bool {
        matches!(
            self,
            Self::Unit
                | Self::Bool
                | Self::I32
                | Self::I8
                | Self::I16
                | Self::Isize
                | Self::I64
                | Self::U64
                | Self::Usize
                | Self::F32
                | Self::F64
                | Self::U8
                | Self::U16
                | Self::U32
        )
    }
    /// Scalars or one shared address layer to a finite Copy value or the exact
    /// shared user-resource head shape. This does not establish loan authority.
    pub fn is_scalar_or_shared_address(&self) -> bool {
        self.is_shared_sync_cell_address()
            || self.is_b0_scalar()
            || self.is_shared_resource_head_address()
            || self.is_shared_package_address()
            || matches!(self, Self::Address { mutable: false, inner } if inner.is_address_pointee())
    }
    /// One address layer; admission is not a permission proof.
    pub fn is_scalar_or_address(&self) -> bool {
        self.is_exclusive_task_head_address()
            || self.is_shared_sync_cell_address()
            || self.is_b0_scalar()
            || self.is_shared_resource_head_address()
            || self.is_package_address()
            || matches!(self, Self::Address { inner, .. } if inner.is_address_pointee())
    }
    pub fn is_exclusive_task_head_address(&self) -> bool {
        matches!(self, Self::Address { mutable: true, inner } if matches!(inner.as_ref(), Self::Managed(payload) if matches!(payload.as_ref(), Self::TaskI32 | Self::Task { .. })))
    }
    pub fn is_shared_sync_cell_address(&self) -> bool {
        matches!(self, Self::Address { mutable: false, inner } if matches!(inner.as_ref(), Self::SyncCell(_)))
    }
    /// Structural eligibility only; each checker separately proves sources,
    /// bounds, liveness, permissions and the supported target profile.
    pub fn is_address_pointee(&self) -> bool {
        self.is_copy_value()
            || matches!(self, Self::Dyn { .. })
            || matches!(self, Self::Slice { element } if element.is_copy_value())
    }
    /// Structural eligibility for a shared actual user-resource header. This
    /// does not establish its nominal definition, initialization, provenance or
    /// permission. General pointee admission intentionally remains unchanged.
    pub fn is_shared_resource_head_address(&self) -> bool {
        matches!(self, Self::Address { mutable: false, inner }
            if matches!(inner.as_ref(), Self::Managed(payload)
                if matches!(payload.as_ref(), Self::Record { .. })
                    && payload.is_copy_value()))
    }
    /// Shape only: a Store element window requires separate exact element,
    /// provenance, initialization and loan checks. General pointees stay Copy.
    pub fn is_shared_package_address(&self) -> bool {
        matches!(self, Self::Address { mutable: false, inner }
            if matches!(inner.as_ref(), Self::Package { .. }))
    }
    /// Raw shape only. A mutable Package window still needs independent exact
    /// Store-element, origin, initialized-prefix and exclusive-loan checks.
    /// This does not make Package a general address pointee or a Copy value.
    pub fn is_package_address(&self) -> bool {
        matches!(self, Self::Address { inner, .. }
            if matches!(inner.as_ref(), Self::Package { .. }))
    }
    pub fn is_slice_address(&self) -> bool {
        matches!(self, Self::Address { inner, .. } if matches!(inner.as_ref(), Self::Slice { .. }))
    }
    pub fn is_copy_value(&self) -> bool {
        match self {
            Self::Unit
            | Self::Bool
            | Self::I32
            | Self::I8
            | Self::I16
            | Self::Isize
            | Self::I64
            | Self::U64
            | Self::Usize
            | Self::F32
            | Self::F64
            | Self::U8
            | Self::U16
            | Self::U32
            | Self::ForeignHandle
            | Self::CWideString
            | Self::ForeignCallback { .. } => true,
            Self::Tuple(fields) => fields.iter().all(Self::is_copy_value),
            Self::Array { element, .. } => element.is_copy_value(),
            Self::Record { fields, .. } => fields.iter().all(|(_, ty)| ty.is_copy_value()),
            Self::Enum { variants, .. } => variants
                .iter()
                .all(|(_, fields)| fields.iter().all(Self::is_copy_value)),
            Self::ErrorUnion { error, result } => error.is_copy_value() && result.is_copy_value(),
            Self::FunctionPointer { parameters, result } => {
                parameters.iter().all(Self::is_copy_value) && result.is_copy_value()
            }
            // Copying the code address never copies its invocation payload.
            // Signature and relation eligibility must be validated separately.
            Self::BorrowedFunctionPointer { .. } => true,
            _ => false,
        }
    }
    pub fn aggregate_fields(&self) -> Option<Vec<&Type>> {
        match self {
            Self::Tuple(fields) => Some(fields.iter().collect()),
            Self::Array { element, length } => Some(vec![element.as_ref(); *length as usize]),
            Self::Record { fields, .. } => Some(fields.iter().map(|(_, ty)| ty).collect()),
            _ => None,
        }
    }
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            Self::I32
                | Self::I8
                | Self::I16
                | Self::Isize
                | Self::I64
                | Self::U64
                | Self::Usize
                | Self::U8
                | Self::U16
                | Self::U32
        )
    }
    pub fn is_float(&self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }
    pub fn is_numeric(&self) -> bool {
        self.is_integer() || self.is_float()
    }
    pub fn name(&self) -> String {
        match self {
            Self::Unit => "()".into(),
            Self::Bool => "bool".into(),
            Self::I32 => "i32".into(),
            Self::I8 => "i8".into(),
            Self::I16 => "i16".into(),
            Self::Isize => "isize".into(),
            Self::I64 => "i64".into(),
            Self::U64 => "u64".into(),
            Self::Usize => "usize".into(),
            Self::F32 => "f32".into(),
            Self::F64 => "f64".into(),
            Self::U8 => "u8".into(),
            Self::U16 => "u16".into(),
            Self::U32 => "u32".into(),
            Self::ByteBuffer => "ByteBuffer".into(),
            Self::Registration { environment } => format!("Registration<{}>", environment.name()),
            Self::AsyncRegistration { environment } => {
                format!("AsyncRegistration<{}>", environment.name())
            }
            Self::TaskI32 => "TaskI32".into(),
            Self::Task { result, .. } => format!("Task<{}>", result.name()),
            Self::OwnedStore { element } => format!("Store<{}>", element.name()),
            Self::ForeignHandle => "c_handle".into(),
            Self::SyncCell(kind) => match kind {
                SyncCellKind::MutexI32 => "MutexI32",
                SyncCellKind::AtomicI32 => "AtomicI32",
                SyncCellKind::AtomicI64 => "AtomicI64",
                SyncCellKind::AtomicU64 => "AtomicU64",
                SyncCellKind::AtomicUsize => "AtomicUsize",
            }
            .into(),
            Self::CWideString => "c_wstr".into(),
            other => format!("{other:?}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnaryOp {
    Negate,
    Not,
    BitNot,
    Dereference,
    Borrow,
    BorrowMut,
    Give,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    AddWrapping,
    SubtractWrapping,
    MultiplyWrapping,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Constant {
    Unit,
    Bool(bool),
    I32(i32),
    I64(i64),
    U64(u64),
    Usize(u64),
    /// Exact IEEE bits; identity preserves signed zero and every NaN spelling.
    F32(u32),
    F64(u64),
    U8(u8),
    U16(u16),
    U32(u32),
    I8(i8),
    I16(i16),
    Isize(i64),
    NullHandle,
    NullWideString,
    /// Includes exactly one trailing NUL. Untrusted until independently checked.
    WideString(Vec<u16>),
}
impl Constant {
    pub const fn ty(&self) -> Type {
        match self {
            Self::Unit => Type::Unit,
            Self::Bool(_) => Type::Bool,
            Self::I32(_) => Type::I32,
            Self::I8(_) => Type::I8,
            Self::I16(_) => Type::I16,
            Self::Isize(_) => Type::Isize,
            Self::I64(_) => Type::I64,
            Self::U64(_) => Type::U64,
            Self::Usize(_) => Type::Usize,
            Self::F32(_) => Type::F32,
            Self::F64(_) => Type::F64,
            Self::U8(_) => Type::U8,
            Self::U16(_) => Type::U16,
            Self::U32(_) => Type::U32,
            Self::NullHandle => Type::ForeignHandle,
            Self::NullWideString | Self::WideString(_) => Type::CWideString,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsupportedFeature {
    pub feature: String,
    pub span: Option<Span>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreModule {
    pub dyn_declarations: Vec<DynDeclaration>,
    pub dyn_interfaces: Vec<DynInterface>,
    pub dyn_vtables: Vec<CoreDynVtable>,
    /// Raw nominal kind/full-actual inventory; no structural capability proof.
    pub life_declarations: Vec<LifeNominalDeclaration>,
    pub life_instances: Vec<LifeNominalInstance>,
    /// Raw source context; no class, loan or calling authority.
    pub closure_life_contexts: Vec<CoreClosureLifeContext>,
    pub schema_version: u32,
    pub module_id: ModuleId,
    pub source_fingerprint: Fingerprint,
    pub functions: Vec<CoreFunction>,
    pub aggregates: Vec<CoreAggregate>,
    /// Untrusted finite Product recipes; empty on pre-237 source paths.
    pub aggregate_templates: Vec<CoreAggregateTemplate>,
    /// Exact nominal applications, never ownership certificates.
    pub aggregate_applications: Vec<CoreAggregateApplication>,
    pub enums: Vec<CoreEnum>,
    pub unsupported: Vec<UnsupportedFeature>,
}
/// A compiler-reserved source bound. This raw declaration grants no permission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreTypeBound {
    StoreElement,
}

/// Lexically scoped ordinary type binder, distinct from actual callable roles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreTypeParameter {
    pub index: u32,
    pub name: String,
    pub bound: CoreTypeBound,
}

/// A finite Package family's original declaration and field recipe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreAggregateTemplate {
    pub id: NodeId,
    pub name: String,
    /// Declared order; each bounded value parameter has its own dense index.
    pub parameters: Vec<CoreTypeParameter>,
    pub fields: Vec<CoreAggregateFieldRecipe>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreAggregateFieldRecipe {
    pub name: String,
    pub public: bool,
    pub manages: bool,
    pub ty: CoreAggregateTypeRecipe,
    pub span: Span,
}
/// Recipe Parameter refers only to this template's formal declaration slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreAggregateTypeRecipe {
    Concrete(Type),
    Parameter {
        index: u32,
    },
    Store {
        element: Box<CoreAggregateTypeRecipe>,
    },
    /// Sealed owning registration application; the environment recipe remains actual.
    Registration {
        environment: Box<CoreAggregateTypeRecipe>,
    },
    AsyncRegistration {
        environment: Box<CoreAggregateTypeRecipe>,
    },
    /// Task application with actual result and retained-port type recipes.
    Task {
        result: Box<CoreAggregateTypeRecipe>,
        binder_count: u32,
        ports: Vec<CoreTaskPortRecipe>,
        /// Complete result class mapping, checked independently of the ports.
        retained: Vec<StorageRetainedBinding>,
    },
    Apply {
        module: ModuleId,
        template: NodeId,
        arguments: Vec<CoreAggregateTypeRecipe>,
    },
}
/// Core-only recipe; each pointee is resolved against the actual template Type.
/// Neither the label nor the recipe grants a loan or runtime address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreTaskPortRecipe {
    pub pointee: Box<CoreAggregateTypeRecipe>,
    pub mutable: bool,
    pub binder: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreAggregateApplication {
    pub aggregate: NodeId,
    pub template_module: ModuleId,
    pub template: NodeId,
    pub owner: Option<CoreAggregateOwner>,
    pub arguments: Vec<CoreAggregateArgument>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoreAggregateOwner {
    pub module: ModuleId,
    pub function: NodeId,
}
/// Application Parameter refers only to the actual owner's function binder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreAggregateArgument {
    Concrete(Type),
    Parameter { index: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreFunction {
    /// Raw relation; each kernel separately checks complete actual CFG/targets.
    pub storage_life: Option<StorageLifeContract>,
    /// Source declaration, or None for body-derived selection. Never an effect proof.
    pub call_kind: Option<CallKind>,
    /// Source-declared lexical life bindings. Raw metadata, never a proof.
    pub life: Option<CoreLifeContract>,
    /// Retained ordinary template metadata; only a checked full instance may run.
    pub type_parameters: Vec<CoreTypeParameter>,
    /// Independent declared bindings within one owning State parameter.
    /// This raw table grants neither call authority nor ownership.
    pub state_parameters: Vec<CoreStateParameter>,
    /// Raw finite state relation. A declaration grants no transition authority.
    pub protocol_contract: Option<FunctionProtocolContract>,
    /// Source-declared opaque callable result; still untrusted and unresolved.
    pub callable_return: Option<CoreCallableReturn>,
    pub id: NodeId,
    pub name: String,
    /// Retained callable template metadata; absent on ordinary concrete functions.
    pub callable_parameters: Vec<CoreCallableParameter>,
    pub foreign: Option<ForeignDeclaration>,
    pub parameters: Vec<CoreParameter>,
    pub return_type: Type,
    pub result_mode: PassingMode,
    pub body: CoreBlock,
    pub receiver: Option<ReceiverShape>,
    pub span: Span,
}
/// Life binder identity is local to this function, including a receiver at index 0.
/// The dense parameter map names source-declared shared input dependencies only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreLifeContract {
    pub binders: Vec<String>,
    pub parameter_lives: Vec<Option<u32>>,
    pub result_life: Option<u32>,
}

/// Source-declared bound for a finite, self-contained callable factory result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreCallableReturn {
    pub kind: CallableKind,
    pub parameters: Vec<Type>,
    pub result: Type,
}

/// Raw bound for one retained, nonescaping callable template parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreCallableParameter {
    /// Invocation-local raw binder relation, or the source-bound P/R proposal
    /// when source_life is present. Neither supplies environment Hold rights.
    pub life: Option<StorageLifeContract>,
    /// Original bound-to-header bridge. Requires life=Some; not a proof.
    pub source_life: Option<CoreCallableLifeContext>,
    pub index: u32,
    /// Position among callable generic declarations, independent of runtime role order.
    pub generic_index: u32,
    pub generic_name: String,
    /// Explicit sealed requirement; never a producer proof of actual captures.
    pub closed_environment: bool,
    /// Explicit sealed Transfer premise for the value and cleanup. Actual data
    /// is rechecked; this premise does not establish thread-call authority.
    pub transfer: bool,
    /// Explicit sealed entry-call premise, independently checked for every F.
    /// It grants neither environment Transfer/Share nor concurrent invocation.
    pub thread_callable: bool,
    pub kind: CallableKind,
    pub parameters: Vec<Type>,
    pub result: Type,
}

/// One real owning State parameter, evaluated and transferred only once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreStateParameter {
    pub index: u32,
    pub bindings: Vec<CoreStateBinding>,
}

/// One lexical callable declaration and its actual direct symbolic fields.
/// Declaration order is independent of Package field order and ClosureId.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreStateBinding {
    pub generic_index: u32,
    pub generic_name: String,
    pub kind: CallableKind,
    pub parameters: Vec<Type>,
    pub result: Type,
    pub fields: Vec<u32>,
}

/// Finite static phase of the sealed pollable registration. It never creates
/// an owning responsibility or proves the provider's runtime drain promise.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProtocolState {
    Open,
    Closing,
    Closed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProtocolKind {
    AsyncRegistrationV1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProtocolExit {
    Return,
    ErrorOk,
    ErrorErr,
    PollPending,
    PollReady,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolTransition {
    pub input: ProtocolState,
    pub exit: ProtocolExit,
    pub output: Vec<ProtocolState>,
}
/// Untrusted upper relation for one actual parameter position. Each entry row
/// must be proved from the complete callee CFG before callers may consume it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionProtocolContract {
    pub protocol: ProtocolKind,
    pub parameter: u32,
    pub entry: Vec<ProtocolState>,
    pub exits: Vec<ProtocolTransition>,
}

/// An explicit unsafe promise for one end of the scoped callback protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopedCallbackRole {
    Register,
    Close,
    /// Candidate same-thread register(cb,userdata)->c_handle; failed installation
    /// must synchronously withdraw and drain every raw/backend userdata access.
    AsyncRegister,
    /// Candidate Direct C request(token)->Unit; does not establish quiescence.
    AsyncRequestClose,
    /// Candidate Direct C poll(token)->i32: 0 Pending, 1 complete raw/backend
    /// no-future, other tags violate the provider promise and grant no release.
    AsyncPollDrain,
    /// Thread-capable registration. Publish initialized F before exposing the raw
    /// pair. Failure must withdraw and synchronously drain every raw holder and
    /// backend access, including pre-trampoline holders, with their writes visible
    /// before returning null. This promise grants no capability to F or its owner.
    ThreadAsyncRegister,
    /// Direct C request, called once after the runtime closes its own gate.
    /// The provider retains that request; return does not establish drain.
    ThreadAsyncRequestClose,
    /// 0 Pending; 1 promises that, when this call returns, all raw holders and
    /// backend userdata accesses (including rejected and pre-trampoline entries)
    /// have synchronously ended, their writes are visible, and none can start.
    /// Other tags violate the promise and never authorize cleanup or free.
    ThreadAsyncPollDrain,
}

fn scoped_callback_role_tag(role: Option<ScopedCallbackRole>) -> u8 {
    match role {
        None => 0,
        Some(ScopedCallbackRole::Register) => 1,
        Some(ScopedCallbackRole::Close) => 2,
        Some(ScopedCallbackRole::AsyncRegister) => 3,
        Some(ScopedCallbackRole::AsyncRequestClose) => 4,
        Some(ScopedCallbackRole::AsyncPollDrain) => 5,
        Some(ScopedCallbackRole::ThreadAsyncRegister) => 6,
        Some(ScopedCallbackRole::ThreadAsyncRequestClose) => 7,
        Some(ScopedCallbackRole::ThreadAsyncPollDrain) => 8,
    }
}

/// An untrusted external ABI/symbol declaration, never proof of its C implementation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignDeclaration {
    pub abi: String,
    pub symbol: String,
    /// Sorted exact indices of synchronous, non-retained borrowed parameters.
    /// An external promise only: independent checking still proves each loan.
    pub noescape_parameters: Vec<u32>,
    /// Explicit external promise allowing only the finite static callback profile.
    pub static_callbacks: bool,
    pub scoped_callback: Option<ScopedCallbackRole>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreParameter {
    pub name: String,
    pub ty: Type,
    pub mode: PassingMode,
    pub span: Span,
}
/// A source closure parameter may await a unique type constraint. An absent
/// annotation is not an unsupported type and must not reach final typed IR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreClosureParameter {
    pub name: String,
    pub ty: Option<Type>,
    pub mode: PassingMode,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreAggregate {
    pub id: NodeId,
    pub name: String,
    pub kind: AggregateKind,
    /// Explicit builtin Drop implementation symbol; not an inferred method name.
    pub teardown: Option<String>,
    pub fields: Vec<CoreField>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreEnum {
    pub id: NodeId,
    pub name: String,
    pub variants: Vec<(String, Vec<Type>)>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorePattern {
    pub kind: CorePatternKind,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CorePatternKind {
    Name(String),
    Wildcard,
    Tuple(Vec<CorePattern>),
    /// Named fields retain their labels and nominal source type until Types
    /// checks the actual record layout. Core input is never authority alone.
    Record {
        ty: Type,
        fields: Vec<(String, CorePattern)>,
    },
    Reference {
        mutable: bool,
        pattern: Box<CorePattern>,
    },
    /// Explicit ErrorUnion tag selection; never a nominal-name authority.
    ErrorArm {
        is_error: bool,
        pattern: Box<CorePattern>,
    },
    Variant {
        ty: Type,
        variant: String,
        fields: Vec<CorePattern>,
    },
    Integer(String),
    Bool(bool),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreMatchArm {
    pub pattern: CorePattern,
    pub body: CoreBlock,
    pub span: Span,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggregateKind {
    Struct,
    Package,
    Resource,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreField {
    pub name: String,
    pub ty: Type,
    /// Source visibility; declaring module is the containing ProgramModule.
    pub public: bool,
    pub manages: bool,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreBlock {
    pub statements: Vec<CoreStatement>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreStatement {
    pub id: NodeId,
    pub kind: CoreStatementKind,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreStatementKind {
    Binding {
        name: String,
        mutable: bool,
        ty: Option<Type>,
        initializer: Option<CoreExpr>,
    },
    /// Untrusted irrefutable tuple pattern; Types must check exact shape and
    /// Copy eligibility before introducing any of its lexical bindings.
    TupleBinding {
        pattern: CorePattern,
        mutable: bool,
        ty: Option<Type>,
        initializer: CoreExpr,
    },
    Assign {
        target: CoreExpr,
        value: CoreExpr,
    },
    Expression(CoreExpr),
    Return(Option<CoreExpr>),
    If {
        condition: CoreExpr,
        then_body: CoreBlock,
        else_body: Option<CoreBlock>,
    },
    IfLet {
        pattern: CorePattern,
        value: CoreExpr,
        then_body: CoreBlock,
        else_body: Option<CoreBlock>,
    },
    While {
        condition: CoreExpr,
        body: CoreBlock,
    },
    WhileLet {
        pattern: CorePattern,
        value: CoreExpr,
        body: CoreBlock,
    },
    ForRange {
        name: String,
        start: CoreExpr,
        end: CoreExpr,
        body: CoreBlock,
    },
    Match {
        value: CoreExpr,
        arms: Vec<CoreMatchArm>,
    },
    Block(CoreBlock),
    /// Register an ordinary synchronous cleanup body at this declaration point.
    Defer(CoreBlock),
    Unsafe(CoreBlock),
    Break,
    Continue,
    Unsupported(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreExpr {
    pub id: NodeId,
    pub kind: CoreExprKind,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreExprKind {
    /// Source is an actual borrowed concrete Self address, evaluated once.
    MakeDynBorrow {
        interface: DynInterfaceId,
        vtable: DynVtableId,
        source: Box<CoreExpr>,
        mutable: bool,
    },
    /// Explicit fallible Heap erasure, consuming one complete concrete owner.
    MakeDynOwned {
        interface: DynInterfaceId,
        vtable: DynVtableId,
        source: Box<CoreExpr>,
        route: DynOwnedStorage,
    },
    /// Callee is evaluated once before nonreceiver arguments and supplies arg0.
    CallDynSlot {
        callee: Box<CoreExpr>,
        interface: DynInterfaceId,
        slot: u32,
        arguments: Vec<CoreExpr>,
    },
    /// Compiler-normalized static foreign literal; still untrusted Core data.
    Literal(Constant),
    /// Internal value type context; ordinary checking must validate its child.
    Ascribed {
        ty: Type,
        value: Box<CoreExpr>,
    },
    Integer(String),
    Float(String),
    /// Numeric conversion with numeric endpoints before Types resolves the operand.
    NumericConvert {
        value: Box<CoreExpr>,
        target: Type,
    },
    IntegerConvert {
        value: Box<CoreExpr>,
        target: Type,
    },
    Bool(bool),
    String(String),
    Unit,
    Name(String),
    Unary {
        op: UnaryOp,
        operand: Box<CoreExpr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<CoreExpr>,
        right: Box<CoreExpr>,
    },
    /// Untrusted complete type constraints, legal only at a static Call callee.
    TypeApply {
        callee: Box<CoreExpr>,
        type_arguments: Vec<Type>,
    },
    Call {
        callee: Box<CoreExpr>,
        arguments: Vec<CoreExpr>,
    },
    Tuple(Vec<CoreExpr>),
    Array(Vec<CoreExpr>),
    Index {
        object: Box<CoreExpr>,
        index: Box<CoreExpr>,
    },
    /// A source range place. Only a borrow may turn it into a slice address.
    Slice {
        object: Box<CoreExpr>,
        start: Option<Box<CoreExpr>>,
        end: Option<Box<CoreExpr>>,
    },
    Member {
        object: Box<CoreExpr>,
        member: String,
        /// A reflected field binds its complete nominal owner, including all
        /// generic arguments. This constrains selection without changing the
        /// receiver's value, place, ownership or address permissions.
        owner: Option<Type>,
    },
    ReceiverMember(String),
    Record {
        ty: Type,
        fields: Vec<(String, CoreExpr)>,
    },
    RecordUpdate {
        ty: Option<Type>,
        base: Box<CoreExpr>,
        fields: Vec<(String, CoreExpr)>,
    },
    /// Ordered hygienic temporaries. Names use `$`, which source identifiers cannot contain.
    Sequence {
        bindings: Vec<(String, CoreExpr)>,
        value: Box<CoreExpr>,
    },
    /// Unevaluated source CT expression; it must enter VerifiedCTIR before use.
    CompileTime(Box<CoreExpr>),
    /// Single evaluation with propagation along the actual error edge.
    Try(Box<CoreExpr>),
    Closure {
        move_capture: bool,
        parameters: Vec<CoreClosureParameter>,
        return_type: Option<Type>,
        body: CoreBlock,
    },
    If {
        condition: Box<CoreExpr>,
        then_value: Box<CoreExpr>,
        else_value: Box<CoreExpr>,
    },
    Unsupported(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValueCategory {
    Value,
    Managed,
    Desc,
    Address,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PassingMode {
    Copy,
    Move,
    Describe,
    Create,
    Give,
    Borrow,
    BorrowMut,
    Infer,
}
fn passing_mode_tag(mode: PassingMode) -> u8 {
    match mode {
        PassingMode::Copy => 0,
        PassingMode::Move => 1,
        PassingMode::Describe => 2,
        PassingMode::Create => 3,
        PassingMode::Give => 4,
        PassingMode::Borrow => 5,
        PassingMode::BorrowMut => 6,
        PassingMode::Infer => 7,
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiverShape {
    pub ty: Type,
    pub mode: PassingMode,
    pub implicit: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CallableKind {
    FunctionPointer,
    Fn,
    FnMut,
    FnOnce,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallableShape {
    pub kind: CallableKind,
    pub parameters: Vec<Type>,
    pub result: Type,
    pub environment: Option<ClosureId>,
    pub captures: Vec<CaptureField>,
    pub contracts: CallableContracts,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureField {
    pub name: String,
    pub ty: Type,
    pub category: ValueCategory,
    pub mode: PassingMode,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CallableContracts {
    pub hold: Vec<String>,
    pub call: Vec<String>,
    pub cleanup: Vec<String>,
    pub resume: Vec<String>,
}

/// Public helper claim. It is checked from the callee CFG before use;
/// neither construction nor transport confers authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallableParameterContract {
    /// Sorted unique actual parameter roles; runtime identity is never inferred from type identity.
    pub roles: Vec<CallableRole>,
    /// One joint ordered call language preserves correlation across every role.
    pub traces: CallableTraces,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallableRole {
    /// Untrusted invocation-local relation; actual closure CFG roots are independently checked.
    pub life: Option<StorageLifeContract>,
    pub parameter: u32,
    pub closure: ClosureId,
    /// Requested bound mode; the concrete closure may support an adaptation.
    pub kind: CallableKind,
    /// Captured Address indices whose targets must stay alive while held.
    pub hold: Vec<u32>,
    /// Captured Desc/Address indices potentially used during each callback call.
    pub call: Vec<u32>,
    /// Conservative target-write upper bound: Desc and mutable Address captures.
    pub writes: Vec<u32>,
    pub cleanup: CallableCleanup,
}
/// A raw opaque callable result claim. The first implemented form is a
/// self-contained concrete environment with no external Hold/Call dependencies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallableReturnContract {
    /// Untrusted result provenance; each reachable return must prove this relation.
    pub source: CallableReturnSource,
    pub closure: ClosureId,
    pub kind: CallableKind,
    pub hold: Vec<u32>,
    pub call: Vec<u32>,
    pub writes: Vec<u32>,
    /// Direct Managed capture indices, in exact environment field order.
    pub cleanup: Vec<u32>,
    /// Sorted unique actual environment captures lending the shared result.
    /// Raw provenance claim only; independent CFG and loan checks are required.
    pub lending_captures: Vec<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CallableReturnSource {
    /// A newly constructed self-contained environment under the factory profile.
    Fresh,
    /// The same live instance supplied in this actual function parameter position.
    Parameter(u32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CallableCleanup {
    /// The by-value helper settles its environment on every returning path.
    Owned,
    /// The helper preserves the borrowed carrier and acquires no cleanup debt.
    Borrowed,
    /// On normal return, transfer the actual remaining environment to the result.
    Returned,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CallableExit {
    /// Ordinary Return, including either tag of a Copy ErrorUnion.
    Return,
    /// Process abort; no unwind cleanup guarantee is implied.
    Trap,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CallEvent {
    pub parameter: u32,
    pub kind: CallableKind,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CallableTrace {
    pub calls: Vec<CallEvent>,
    pub exit: CallableExit,
}
/// Exact callback behavior, transported as an untrusted claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallableTraces {
    Finite(Vec<CallableTrace>),
    Flow(CallableFlow),
}
impl CallableTraces {
    /// Explicit projection: a cyclic graph is never an empty finite trace set.
    pub fn as_finite(&self) -> Option<&[CallableTrace]> {
        match self {
            Self::Finite(traces) => Some(traces),
            Self::Flow(_) => None,
        }
    }
    pub fn as_finite_mut(&mut self) -> Option<&mut Vec<CallableTrace>> {
        match self {
            Self::Finite(traces) => Some(traces),
            Self::Flow(_) => None,
        }
    }
}
/// Flat graph. Canonical semantic claims use entry zero and ordered BFS IDs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallableFlow {
    pub entry: u32,
    pub nodes: Vec<CallableFlowNode>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallableFlowNode {
    pub calls: Vec<CallEvent>,
    pub next: CallableFlowNext,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallableFlowNext {
    Edges(Vec<u32>),
    Exit(CallableExit),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedModule {
    pub dyn_declarations: Vec<DynDeclaration>,
    pub dyn_interfaces: Vec<DynInterface>,
    pub dyn_vtables: Vec<DynVtable>,
    /// Raw nominal kind/full-actual inventory; no structural capability proof.
    pub life_declarations: Vec<LifeNominalDeclaration>,
    pub life_instances: Vec<LifeNominalInstance>,
    pub schema_version: u32,
    pub module_id: ModuleId,
    pub source_fingerprint: Fingerprint,
    pub target: TargetProfile,
    pub functions: PersistentVec<TypedFunction>,
    pub closures: Vec<ClosureDefinition>,
    pub resources: Vec<ResourceDefinition>,
    pub unsupported: Vec<UnsupportedFeature>,
}
/// Raw nominal cleanup binding. Every payload and entry is independently checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceDefinition {
    pub name: String,
    pub payload: Type,
    pub teardown: Option<FunctionId>,
}
/// Raw concrete environment claims. Supported captures are checked independently;
/// this record does not grant address lifetime or calling authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosureDefinition {
    /// Raw relation; each kernel separately checks complete actual CFG/targets.
    pub storage_life: Option<StorageLifeContract>,
    pub id: ClosureId,
    pub function: FunctionId,
    pub captures: Vec<Type>,
    pub parameters: Vec<Type>,
    pub result: Type,
    pub kind: CallableKind,
    /// Sorted unique source capture positions used by the returned borrow.
    /// May select inline owned storage or a qualified direct owning-base Address;
    /// each actual target and parent loan is independently reconstructed. This
    /// does not make arbitrary captured Address/Desc or ordinary parameters eligible.
    pub lending_captures: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedFunction {
    /// Raw relation; each kernel separately checks complete actual CFG/targets.
    pub storage_life: Option<StorageLifeContract>,
    /// Raw ABI/effect selection; actual reachable effects are checked independently.
    pub call_kind: CallKind,
    /// Raw declared upper bound for a shared or mutable reference result; sorted unique
    /// source parameter indices. Each kernel checks the actual CFG separately.
    /// None preserves body-derived origins for unannotated functions.
    pub reference_return_bound: Option<Vec<u32>>,
    /// Raw opaque callable result contract, independently checked by Verify.
    pub callable_return_contract: Option<CallableReturnContract>,
    /// Raw finite helper relation, independently reconstructed by Verify.
    pub callable_contract: Option<CallableParameterContract>,
    /// Raw per-result resource relation, checked from the actual CFG by both kernels.
    pub protocol_contract: Option<FunctionProtocolContract>,
    pub id: FunctionId,
    pub name: String,
    /// A foreign declaration has parameter locals but no executable CFG.
    pub foreign: Option<ForeignDeclaration>,
    /// A closure entry needs its matching hidden environment value. It is not
    /// callable through an ordinary Call or an exported native entry wrapper.
    pub environment: Option<ClosureId>,
    /// Local IDs in source parameter order; parameters are initialized on entry.
    pub parameters: Vec<LocalId>,
    pub return_type: Type,
    pub result_mode: PassingMode,
    pub locals: SharedVec<LocalDecl>,
    /// ValueId is the index here. Every entry must have exactly one definition.
    pub values: SharedVec<Type>,
    pub blocks: SharedVec<BasicBlock>,
    pub entry: BlockId,
    pub span: Span,
}
pub type TypedCFG = TypedFunction;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalDecl {
    pub id: LocalId,
    pub name: String,
    pub ty: Type,
    pub mutable: bool,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BasicBlock {
    pub id: BlockId,
    pub instructions: Vec<Instruction>,
    pub terminator: Terminator,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    pub result: Option<ValueId>,
    pub kind: InstructionKind,
    pub span: Span,
}
/// Reason for leaving one lexical cleanup activation. This tag grants no authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CleanupExitKind {
    Fallthrough,
    Return,
    Error,
    Break,
    Continue,
}
fn cleanup_exit_tag(kind: CleanupExitKind) -> u8 {
    match kind {
        CleanupExitKind::Fallthrough => 0,
        CleanupExitKind::Return => 1,
        CleanupExitKind::Error => 2,
        CleanupExitKind::Break => 3,
        CleanupExitKind::Continue => 4,
    }
}
/// One checked step through a real Enum variant payload. It never creates a
/// view or grants ownership; each selected tag must be independently proved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VariantProjection {
    pub variant: u32,
    pub index: u32,
}
/// One actual inline place step. Field never crosses Managed, Address or
/// external stable storage; Variant always retains its selected tag and field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PlaceProjection {
    Field(u32),
    Variant(VariantProjection),
    /// Follow the actual active error/success slot, retaining its discriminator.
    ErrorArm(bool),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstructionKind {
    MakeDynBorrow {
        interface: DynInterfaceId,
        vtable: DynVtableId,
        source: ValueId,
        mutable: bool,
    },
    MakeDynOwned {
        interface: DynInterfaceId,
        vtable: DynVtableId,
        source: ValueId,
        route: DynOwnedStorage,
    },
    BorrowDynOwned {
        local: LocalId,
        mutable: bool,
    },
    DropDynOwned {
        local: LocalId,
    },
    /// arguments excludes the receiver supplied by this exact callee.
    CallDynSlot {
        callee: ValueId,
        interface: DynInterfaceId,
        slot: u32,
        arguments: Vec<ValueId>,
    },
    /// Real stored-leaf selection from an actual storage Address.
    LoadStoredBorrow {
        storage: ValueId,
        path: Vec<PlaceProjection>,
        mode: StoredBorrowLoad,
    },
    /// Invariant initialized/vacant leaf store; new Hold precedes old release.
    StoreStoredBorrow {
        storage: ValueId,
        path: Vec<PlaceProjection>,
        value: ValueId,
    },
    /// Construct an empty owning prefix; no allocation. Element metadata is a
    /// raw claim and must be independently checked before executable admission.
    OwnedStoreNew {
        element: Type,
    },
    /// Explicit absolute capacity request; failure preserves the old header and
    /// every initialized element. Never expands runtime capacity into IR slots.
    OwnedStoreReserve {
        store: ValueId,
        capacity: ValueId,
    },
    /// Consume one complete element. Full capacity returns it in the error
    /// alternative of Element!Unit; success commits one initialized prefix slot.
    OwnedStorePush {
        store: ValueId,
        value: ValueId,
    },
    /// Consume the last initialized slot into Unit!Element, or return Unit error
    /// when empty. The returned element carries its complete responsibility.
    OwnedStorePop {
        store: ValueId,
    },
    OwnedStoreLen {
        store: ValueId,
    },
    OwnedStoreCapacity {
        store: ValueId,
    },
    /// Borrow a real initialized prefix element after a checked i64 index.
    /// Shared only; the Store retains responsibility for the complete element.
    OwnedStoreBorrow {
        store: ValueId,
        index: ValueId,
    },
    /// Exclusive, synchronous element window. The Store retains all element
    /// responsibility; only independently admitted Copy projections may change.
    OwnedStoreBorrowMut {
        store: ValueId,
        index: ValueId,
    },
    /// New empty managed storage; no allocation and no implicit user code.
    ByteBufferNew,
    /// Absolute-capacity request; failure preserves the complete old state.
    ByteBufferReserve {
        buffer: ValueId,
        capacity: ValueId,
    },
    /// Append one initialized byte without allocation; false if capacity is full.
    ByteBufferPush {
        buffer: ValueId,
        value: ValueId,
    },
    /// Shrink initialized prefix; false without mutation for negative/growing lengths.
    ByteBufferTruncate {
        buffer: ValueId,
        length: ValueId,
    },
    ByteBufferLen {
        buffer: ValueId,
    },
    ByteBufferCapacity {
        buffer: ValueId,
    },
    /// A fresh loan of the initialized prefix, rooted in the real carrier.
    ByteBufferBorrow {
        buffer: ValueId,
        mutable: bool,
    },
    /// Start a lexical activation; storage is owned by the innermost scope.
    ScopeEnter {
        scope: ScopeId,
    },
    /// End one activation after its storage and stored obligations are cleared.
    /// Owned SSA block results may cross this boundary; the independent resource
    /// checker still conserves their responsibilities through the whole function.
    ScopeExit {
        scope: ScopeId,
        kind: CleanupExitKind,
    },
    /// Register an initialized hidden FnOnce local in the current activation.
    /// This fact has no machine action; TakeLocal followed by CallClosureOnce
    /// performs the required cleanup, independently checked at every exit.
    RegisterDefer {
        scope: ScopeId,
        local: LocalId,
    },
    MakeErrorUnion {
        is_error: bool,
        value: ValueId,
    },
    ErrorTag {
        value: ValueId,
    },
    /// Runtime checked tag projection, never an unchecked reinterpretation.
    ReadErrorUnion {
        value: ValueId,
        is_error: bool,
    },
    /// Consume the complete non-Copy union and select its guarded active payload.
    /// The tag assertion grants no authority: Verify reconstructs branch guards.
    TakeErrorUnion {
        value: ValueId,
        is_error: bool,
    },
    /// Conditionally discharge the active owned payload of a local error union.
    DropErrorUnion {
        local: LocalId,
    },
    /// Construct a complete owned resource header; result is Managed<payload>.
    MakeManaged {
        fields: Vec<ValueId>,
    },
    /// Atomically consume fully prepared fields into a complete owned package.
    MakePackage {
        fields: Vec<ValueId>,
    },
    /// Atomically consume prepared homogeneous owned elements into an array.
    MakeOwnedArray {
        elements: Vec<ValueId>,
    },
    /// Read a Copy Package field or describe its Managed field at a static slot.
    /// This projection retains the owning array and its element responsibility.
    ProjectOwnedArrayField {
        local: LocalId,
        index: u32,
        field: u32,
    },
    /// Transfer one complete live element, retaining the array's local shell.
    TakeOwnedArrayElement {
        local: LocalId,
        index: u32,
    },
    /// Conditionally discharge one owned element before a prepared replacement.
    DropOwnedArrayElement {
        local: LocalId,
        index: u32,
    },
    /// Consume a prepared element into an empty slot of a mutable array local.
    StoreOwnedArrayElement {
        local: LocalId,
        index: u32,
        value: ValueId,
    },
    /// Conditionally discharge live elements in ascending index order.
    DropOwnedArray {
        local: LocalId,
    },
    /// Borrow one concrete Closure element at its actual static array slot.
    /// The owning array and element retain all cleanup responsibilities.
    BorrowOwnedArrayElement {
        local: LocalId,
        index: u32,
        mutable: bool,
    },
    /// Borrow a concrete Closure at its actual initialized Package field.
    /// The owner and field retain their cleanup responsibility.
    BorrowPackageField {
        local: LocalId,
        index: u32,
        mutable: bool,
    },
    /// Read a Copy field without loading or moving its owning package.
    ReadPackageField {
        local: LocalId,
        index: u32,
    },
    /// Borrow the actual resource header at this package field position.
    DescribePackageField {
        local: LocalId,
        index: u32,
    },
    /// Conditional whole cleanup, in declaration field order. The whole live
    /// flag is cleared before any field's user teardown is invoked.
    DropPackage {
        local: LocalId,
    },
    /// Transfer one supported live owning field, retaining the package shell.
    /// The result is an existing responsibility, never a fresh allocation.
    TakePackageField {
        local: LocalId,
        index: u32,
    },
    /// Conditionally discharge one owning field before a prepared replacement.
    /// This explicit cleanup never changes the shell or sibling field debts.
    DropPackageField {
        local: LocalId,
        index: u32,
    },
    /// Write a matching field of an active, mutable package local. Owning
    /// destinations must have no live debt and consume the input exactly once.
    StorePackageField {
        local: LocalId,
        index: u32,
        value: ValueId,
    },
    /// Activate the unique lexical view of one complete owned Package capture.
    /// Data, shell and field live flags alias the actual environment storage.
    /// Every Package capture is bound once in the entry ScopeEnter prefix;
    /// ordinary StorageLive/StorageDead and Package Read/TakeCapture are invalid.
    BindPackageCapture {
        local: LocalId,
        index: u32,
    },
    /// End the lexical view without changing any environment responsibility.
    /// Fn/FnMut exits retain a complete package; FnOnce may leave partial debt
    /// for caller-side environment cleanup in capture/field declaration order.
    EndPackageCapture {
        local: LocalId,
        index: u32,
    },
    /// Borrow an access description of the actual local header, not ownership.
    DescribeLocal {
        local: LocalId,
    },
    /// A weak access description of a live, actually borrowed resource header.
    /// Address provenance and permissions remain independent proof obligations.
    DescribeAddress {
        address: ValueId,
    },
    ReadDesc {
        desc: ValueId,
        index: u32,
    },
    WriteDesc {
        desc: ValueId,
        index: u32,
        value: ValueId,
    },
    /// Conditional lexical cleanup, tied to the actual runtime live state.
    DropManaged {
        local: LocalId,
    },
    /// Immutable program-static UTF8 bytes, with no terminator or runtime owner.
    /// The exact result is Address<Slice<U8>> with mutable=false. Raw bytes and
    /// the claimed result type require independent validation. Runtime only.
    StaticUtf8 {
        bytes: Vec<u8>,
    },
    Constant(Constant),
    /// Preserve the mathematical integer value, trapping when out of range.
    ConvertInteger {
        value: ValueId,
        target: Type,
    },
    /// Checked numeric conversion with numeric endpoints before Types resolves the operand.
    ConvertNumber {
        value: ValueId,
        target: Type,
    },
    Load(LocalId),
    /// Transfer a non-Copy carrier and make its source uninitialized.
    TakeLocal {
        local: LocalId,
    },
    /// Begin/end one activation of a lexical stack position; neither initializes it.
    StorageLive(LocalId),
    StorageDead(LocalId),
    /// Shared scalar address. The root is (enclosing FunctionId, LocalId).
    BorrowLocal {
        local: LocalId,
    },
    BorrowLocalMut {
        local: LocalId,
    },
    /// A child window; the parent retains no conflicting execution authority.
    ReborrowAddress {
        address: ValueId,
        mutable: bool,
    },
    ReadAddress {
        address: ValueId,
    },
    WriteAddress {
        address: ValueId,
        value: ValueId,
    },
    AddressField {
        address: ValueId,
        index: u32,
    },
    AddressIndex {
        address: ValueId,
        index: ValueId,
    },
    /// Range projection preserving its source authority; not a new reborrow.
    /// Bounds are checked before pointer arithmetic by the native consumer.
    AddressSlice {
        address: ValueId,
        start: Option<ValueId>,
        end: Option<ValueId>,
    },
    /// Metadata read still requires a live, accessible slice window.
    SliceLength {
        address: ValueId,
    },
    Store {
        local: LocalId,
        value: ValueId,
    },
    Unary {
        op: UnaryOp,
        operand: ValueId,
    },
    Binary {
        op: BinaryOp,
        left: ValueId,
        right: ValueId,
    },
    /// Unit calls have no result. All other calls have exactly one result.
    Call {
        function: FunctionId,
        arguments: Vec<ValueId>,
    },
    /// Execute one static Suspend child. Arguments commit once on initial entry;
    /// repeated polls resume that same child. The result is produced only at Ready.
    /// A non-Unit result uses the instruction's actual SSA result slot and type.
    SuspendCall {
        function: FunctionId,
        arguments: Vec<ValueId>,
    },
    /// Explicit lexical unsafe boundary; independently reconstructed over the CFG.
    UnsafeEnter {
        region: u32,
    },
    UnsafeExit {
        region: u32,
    },
    /// Never interchangeable with Call or an ordinary FunctionRef.
    ForeignCall {
        function: FunctionId,
        arguments: Vec<ValueId>,
        region: u32,
    },
    /// A synchronous external registration bounded by this structured call.
    /// Borrows the actual FnMut environment and consumes the body exactly once
    /// (call on successful registration, cleanup on failed registration).
    WithForeignCallback {
        register: FunctionId,
        close: FunctionId,
        callback: ValueId,
        body: ValueId,
        region: u32,
    },
    /// Direct structured thread call. Only the verified i32 worker payload
    /// crosses threads; body runs and is consumed in the parent thread.
    /// Result: (created, worker_result, body_result). Failure cleans body once.
    WithScopedThread {
        worker: FunctionId,
        argument: ValueId,
        body: ValueId,
    },
    /// Stable scoped native cell. Body is consumed once, or dropped on init failure.
    WithSyncCell {
        kind: SyncCellKind,
        initial: ValueId,
        body: ValueId,
    },
    /// Dedicated cell operation; ordinary payload access is never authorized.
    SyncI32 {
        op: SyncI32Op,
        address: ValueId,
        value: Option<ValueId>,
        ordering: AtomicOrdering,
    },
    /// Strong conditional RMW on an actual live sealed AtomicI32 cell.
    /// Result is ordinary Tuple[I32, Bool]: observed value, then exchanged.
    /// Raw fields carry no root, order compatibility or synchronization proof.
    SyncI32CompareExchange {
        address: ValueId,
        expected: ValueId,
        desired: ValueId,
        success: AtomicOrdering,
        failure: AtomicOrdering,
    },
    /// Width follows the actual sealed atomic cell, never the operation tag.
    SyncInteger {
        op: SyncIntegerOp,
        address: ValueId,
        value: Option<ValueId>,
        ordering: AtomicOrdering,
    },
    /// Strong conditional RMW; observed integer width follows the cell.
    SyncIntegerCompareExchange {
        address: ValueId,
        expected: ValueId,
        desired: ValueId,
        success: AtomicOrdering,
        failure: AtomicOrdering,
    },
    /// Consumes a complete value on every path. Allocation/init failure
    /// synchronously cleans the value and returns an Allocation error.
    MutexCreate {
        value: ValueId,
    },
    /// Lock borrows the actual stable head until the guard is consumed.
    MutexLock {
        mutex: ValueId,
    },
    /// Project an exclusive child address from a live guard, tied to its debt.
    MutexValue {
        guard: ValueId,
    },
    /// Consume the unique unlock debt after all projected children end.
    MutexUnlock {
        guard: ValueId,
    },
    /// Conditional implicit unlock for a live guard local on scope exit.
    DropMutexGuard {
        local: LocalId,
    },
    /// Explicit allocation/execution root for a verified scalar Suspend worker.
    TaskStart {
        worker: FunctionId,
        argument: ValueId,
    },
    /// General fallible execution root. Arguments are evaluated/transferred once.
    /// Result is ordinary TaskCreateError ! Managed(Task<actual worker result>).
    /// Failure cleans all transferred inputs once; success owns a non-null frame.
    TaskCreate {
        worker: FunctionId,
        arguments: Vec<ValueId>,
        /// Maps each retained port to an actual constructor input. A binding is
        /// checked against the worker and the executable borrow provenance.
        port_bindings: Vec<RetainedPortBinding>,
    },
    /// A source suspension point. Its live scalar state is spilled in the SABI frame.
    TaskYield,
    /// Observe the explicit fallible nullable task head without consuming it.
    TaskValid {
        task: ValueId,
    },
    /// Unique head authority. TaskI32 keeps its compatibility status/value;
    /// general Task<T> returns ordinary TaskPollState ! Poll<T>, moving Ready T once.
    TaskPoll {
        task: ValueId,
    },
    /// Unique head authority; sticky and idempotent, never proof of quiescence.
    TaskRequestCancel {
        task: ValueId,
    },
    /// Unique head authority. TaskI32 retains Bool; general Task<T> returns
    /// ordinary Poll<Unit>. Pending retains all frame/child/result/loan obligations.
    TaskPollClose {
        task: ValueId,
    },
    /// Consumes the owning head; the runtime requires Closed before destruction.
    TaskFinishDestroy {
        task: ValueId,
    },
    /// Consumes the actual closure into fallible stable owning storage.
    /// Region denotes the explicit unsafe call promise, never a retained lifetime.
    RegistrationStart {
        register: FunctionId,
        close: FunctionId,
        callback: ValueId,
        region: u32,
    },
    /// Readonly observation of the explicit nullable owning carrier.
    RegistrationValid {
        registration: ValueId,
    },
    /// Consumes the owner; strong backend close drains all raw entries before free.
    RegistrationClose {
        registration: ValueId,
    },
    /// Candidate only: the three actual foreign roles and unsafe source region
    /// must be independently checked. Failed installation requires complete
    /// synchronous raw/backend no-future before returning the nullable head.
    AsyncRegistrationStart {
        register: FunctionId,
        request_close: FunctionId,
        poll_drain: FunctionId,
        callback: ValueId,
        region: u32,
    },
    AsyncRegistrationValid {
        registration: ValueId,
    },
    /// Exclusive owning head; request never proves no-future or Closed.
    AsyncRegistrationRequestClose {
        registration: ValueId,
    },
    /// Exclusive owning head -> ordinary Poll<Unit>. Pending retains all users,
    /// environment and owning responsibility. Raw metadata grants no proof.
    AsyncRegistrationPollClose {
        registration: ValueId,
    },
    /// Consumes the head; actual CLOSED state must dominate reclamation.
    AsyncRegistrationFinishDestroy {
        registration: ValueId,
    },
    /// Statically admitted Closed phase; runtime still enforces no-future/drain.
    AsyncRegistrationFinishDestroyChecked {
        registration: ValueId,
    },
    /// A real, environment-free function address with its exact Copy signature.
    FunctionRef {
        function: FunctionId,
    },
    /// Static environment-free C ABI entry, never an ordinary indirect callee.
    ForeignFunctionRef {
        function: FunctionId,
    },
    /// The callee is evaluated before arguments. Unit calls have no result.
    CallIndirect {
        callee: ValueId,
        arguments: Vec<ValueId>,
    },
    MakeAggregate {
        fields: Vec<ValueId>,
    },
    MakeVariant {
        variant: u32,
        fields: Vec<ValueId>,
    },
    VariantTag {
        value: ValueId,
    },
    /// A checked projection: execution traps unless the active tag matches.
    ReadVariant {
        value: ValueId,
        variant: u32,
        index: u32,
    },
    /// Construct a complete ordinary owning Tuple, preserving field order and
    /// each actual input responsibility. No implicit allocation or clone.
    MakeOwnedTuple {
        fields: Vec<ValueId>,
    },
    /// Atomically consume a complete Tuple into exact distinct vacant locals.
    /// Registers the actual destinations in reverse order; no SSA result.
    UnpackTuple {
        local: LocalId,
        fields: Vec<LocalId>,
    },
    /// Conditionally drop only the Tuple's actual remaining initialized fields.
    /// Independent CFG reconstruction qualifies storage state and cleanup age.
    DropTuple {
        local: LocalId,
    },
    /// Selectively transfer one initialized subtree of a real owning Tuple or Package local.
    /// Nonempty mixed path; every active tag and every field initialization is
    /// checked anew. The parent retains storage and all remaining responsibility.
    TakeTupleField {
        local: LocalId,
        path: Vec<PlaceProjection>,
    },
    /// Selectively transfer one initialized subtree of a real Enum/ErrorUnion local.
    /// Same path/loan rules as Tuple Take; every variant step names tag+field.
    TakeVariantField {
        local: LocalId,
        path: Vec<PlaceProjection>,
    },
    /// Borrow one initialized subtree of actual inline local storage. The
    /// nonempty bounded path retains every Field/Variant/ErrorArm ancestor,
    /// active tag and actual parent authority; no intermediate whole loan.
    BorrowPlaceLocal {
        local: LocalId,
        path: Vec<PlaceProjection>,
        mutable: bool,
    },
    /// Read the actual final Enum/ErrorUnion header through a bounded mixed
    /// path; empty observes the root. Every ancestor tag/init is checked anew.
    /// Result i32: Enum ordinal, or ErrorUnion0 success/1 error. Reading the
    /// header grants no payload access or parent replacement permission.
    PlaceTagLocal {
        local: LocalId,
        path: Vec<PlaceProjection>,
    },
    /// Borrow actual addressed inline storage through a nonempty bounded
    /// mixed path. Derive all authority and root/parent loans from address;
    /// every selected ancestor must be active and initialized. Never treats
    /// an Address-valued local as inline pointee storage or crosses a view.
    BorrowPlaceAddress {
        address: ValueId,
        path: Vec<PlaceProjection>,
        mutable: bool,
    },
    /// Observe the final live Enum/ErrorUnion header of actual addressed
    /// storage. Empty path observes the pointee root. Every selected ancestor
    /// tag/init is checked; i32 ordinal or ErrorUnion0 success/1 error. Header
    /// reads grant neither payload authority nor permission to replace parent.
    PlaceTagAddress {
        address: ValueId,
        path: Vec<PlaceProjection>,
    },
    /// Prepare a replacement at actual initialized owning local storage.
    /// This independent ticket is not a ValueId or an Address permission.
    /// Preparing pins the location while allowing ordinary legal reads;
    /// intersecting write/move/drop/storage invalidation is forbidden.
    ReservePlaceLocal {
        reservation: ReservationId,
        local: LocalId,
        path: Vec<PlaceProjection>,
    },
    /// Prepare through a real mutable Address and retain its parent authority.
    /// Never copies an exclusive permission or crosses a view implicitly.
    ReservePlaceAddress {
        reservation: ReservationId,
        address: ValueId,
        path: Vec<PlaceProjection>,
    },
    /// Consume this activation after checking a complete exact-type new owner,
    /// its actual Hold and all live loans. Activate the write, drop the old
    /// owner once and commit with no recoverable failure between drop/store.
    CommitPlaceReplace {
        reservation: ReservationId,
        value: ValueId,
    },
    /// Settle a prepared activation without changing its old initialized value.
    /// Every exit and CFG join must settle active tickets; repeated loop
    /// execution may create a new activation only after the previous settled.
    AbortPlaceReservation {
        reservation: ReservationId,
    },
    /// Borrow a real inline finite Copy Tuple/Record field place. The bounded
    /// nonempty field path retains parent storage, initialization and mutability.
    BorrowFieldLocal {
        local: LocalId,
        path: Vec<u32>,
        mutable: bool,
    },
    /// Observe a complete live local through Enum-only checked projections.
    /// Empty path observes the root tag; the terminal type must be Enum.
    VariantTagLocal {
        local: LocalId,
        path: Vec<VariantProjection>,
    },
    /// Copy a strict finite Copy terminal through a nonempty Enum-only path.
    /// Every prefix must have the selected active tag; never produces a view.
    ReadVariantLocal {
        local: LocalId,
        path: Vec<VariantProjection>,
    },
    /// Borrow a real active payload place through an Enum-only path.
    /// The nonempty path, all active tags and exact local mutability are raw
    /// claims checked independently. The address retains its parent local loan.
    BorrowVariantLocal {
        local: LocalId,
        path: Vec<VariantProjection>,
        mutable: bool,
    },
    /// Observe the actual Enum addressed by this value through active prefixes.
    /// Address identity, current tag epochs and permission are untrusted claims.
    VariantTagAddress {
        address: ValueId,
        path: Vec<VariantProjection>,
    },
    /// Borrow an actual addressed payload, retaining its real parent loan.
    BorrowVariantAddress {
        address: ValueId,
        path: Vec<VariantProjection>,
        mutable: bool,
    },
    /// Atomically consume one complete Enum and initialize all variant fields.
    /// Fields are exact distinct vacant locals, registered in reverse order;
    /// this instruction has no SSA result and inherits no cleanup age.
    UnpackVariant {
        local: LocalId,
        variant: u32,
        fields: Vec<LocalId>,
    },
    /// Conditionally close only the actual active variant of an available Enum.
    /// Storage-live vacant/transferred states are no-ops; an exact all-path
    /// cleaned duplicate or storage-dead access remains invalid.
    DropEnum {
        local: LocalId,
    },
    ExtractIndex {
        aggregate: ValueId,
        index: ValueId,
    },
    UpdateIndex {
        aggregate: ValueId,
        index: ValueId,
        value: ValueId,
    },
    ExtractField {
        aggregate: ValueId,
        index: u32,
    },
    UpdateField {
        aggregate: ValueId,
        index: u32,
        value: ValueId,
    },
    MakeClosure {
        closure: ClosureId,
        captures: Vec<ValueId>,
    },
    ReadCapture {
        index: u32,
    },
    /// Describe a live Managed field of the active environment without moving it.
    DescribeCapture {
        index: u32,
    },
    /// Transfer a live owned field out of an active FnOnce environment.
    TakeCapture {
        index: u32,
    },
    /// Borrow a captured concrete closure at its actual environment address.
    /// No responsibility moves; Verify reconstructs capture and call authority.
    BorrowCapture {
        index: u32,
        mutable: bool,
    },
    /// Borrow the actual local Fn environment; never copy its owned fields.
    CallClosureShared {
        callee: LocalId,
        arguments: Vec<ValueId>,
    },
    /// Invoke a borrowed concrete environment at its real address. Only Fn and
    /// FnMut are valid requested modes; loading/copying the environment is not.
    CallClosureBorrowed {
        callee: ValueId,
        kind: CallableKind,
        arguments: Vec<ValueId>,
    },
    /// Consume FnOnce, then clean its remaining owned fields on normal return.
    CallClosureOnce {
        callee: ValueId,
        arguments: Vec<ValueId>,
    },
    /// Conditionally clean a live owning closure local in fixed capture order.
    DropClosure {
        local: LocalId,
    },
    /// Write a pure Copy field of the active FnMut environment.
    WriteCapture {
        index: u32,
        value: ValueId,
    },
    /// Invoke FnMut on its actual initialized, mutable local environment.
    CallClosureMut {
        callee: LocalId,
        arguments: Vec<ValueId>,
    },
    /// As with Call, Unit results have no result ValueId.
    CallClosure {
        callee: ValueId,
        arguments: Vec<ValueId>,
    },
    Unsupported(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Terminator {
    Return(Option<ValueId>),
    Jump(BlockId),
    Branch {
        condition: ValueId,
        then_block: BlockId,
        else_block: BlockId,
    },
    /// Defined, non-returning runtime failure; unlike Unreachable it may be reached.
    Trap,
    /// B0 rejects reachable Unreachable; it never licenses undefined behavior.
    Unreachable,
}

/// Raw candidate facts. The verifier recomputes and checks, never trusts them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidatePlan {
    pub schema_version: u32,
    pub module_fingerprint: Fingerprint,
    pub target_fingerprint: Fingerprint,
    pub functions: PersistentVec<FunctionPlan>,
    pub origins: Vec<OriginClaim>,
    pub responsibilities: Vec<ResponsibilityClaim>,
    pub unsupported: Vec<UnsupportedFeature>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionPlan {
    pub function: FunctionId,
    /// Immutable raw rows shared with their producer; mutation isolates by COW.
    pub blocks: SharedVec<BlockPlan>,
    /// Dense ValueId order; sorted unique roots. Raw claims, independently checked.
    pub holds: SharedVec<ValueHold>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueHold {
    pub value: ValueId,
    pub roots: Vec<HoldRoot>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HoldRoot {
    /// A position in the enclosing function. Names are not identities.
    Local(LocalId),
    /// An incoming shared address environment field, held by the caller.
    Capture(u32),
    /// Target of an incoming shared reference, indexed by source parameter order.
    Parameter(u32),
    /// Immutable program-static storage; only actual checked producers establish it.
    Static,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockPlan {
    pub block: BlockId,
    /// Sorted, duplicate-free definite initialization sets at block boundaries.
    pub initialized_in: Vec<LocalId>,
    pub initialized_out: Vec<LocalId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OriginClaim {
    pub origin: OriginId,
    pub source: NodeId,
    pub multiplicity: OriginMultiplicity,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OriginMultiplicity {
    One,
    Many,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResponsibilityClaim {
    pub function: FunctionId,
    pub carrier: LocalId,
    pub origin: OriginId,
    pub state: ResponsibilityState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponsibilityState {
    Uninitialized,
    Live,
    Done,
    Moved,
}

/// B0 lowering retains the checked scalar CFG exactly. Backends must receive the
/// matching VerifiedModule and compare these public fields before code emission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LirModule {
    pub schema_version: u32,
    pub module_id: ModuleId,
    pub verified_fingerprint: Fingerprint,
    pub target: TargetProfile,
    pub functions: PersistentVec<TypedFunction>,
    pub closures: Vec<ClosureDefinition>,
    pub resources: Vec<ResourceDefinition>,
    pub layouts: Vec<ScalarLayout>,
}
/// Borrowed raw LIR, not verification authority. Backends must still bind every
/// field to the current VerifiedModule. This view avoids copying the checked CFG
/// when lowering preserves it; the owned LirModule and its wire identity remain.
#[derive(Clone)]
pub struct LirView<'a> {
    pub schema_version: u32,
    pub module_id: ModuleId,
    pub verified_fingerprint: Fingerprint,
    pub target: &'a TargetProfile,
    pub functions: &'a PersistentVec<TypedFunction>,
    pub closures: &'a [ClosureDefinition],
    pub resources: &'a [ResourceDefinition],
    pub layouts: std::borrow::Cow<'a, [ScalarLayout]>,
}
impl LirModule {
    pub fn as_view(&self) -> LirView<'_> {
        LirView {
            schema_version: self.schema_version,
            module_id: self.module_id,
            verified_fingerprint: self.verified_fingerprint,
            target: &self.target,
            functions: &self.functions,
            closures: &self.closures,
            resources: &self.resources,
            layouts: std::borrow::Cow::Borrowed(&self.layouts),
        }
    }
}
impl LirView<'_> {
    pub fn to_owned(&self) -> LirModule {
        LirModule {
            schema_version: self.schema_version,
            module_id: self.module_id,
            verified_fingerprint: self.verified_fingerprint,
            target: self.target.clone(),
            functions: self.functions.clone(),
            closures: self.closures.to_vec(),
            resources: self.resources.to_vec(),
            layouts: self.layouts.to_vec(),
        }
    }
}
impl std::fmt::Debug for LirView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LirModule")
            .field("schema_version", &self.schema_version)
            .field("module_id", &self.module_id)
            .field("verified_fingerprint", &self.verified_fingerprint)
            .field("target", &self.target)
            .field("functions", &self.functions)
            .field("closures", &self.closures)
            .field("resources", &self.resources)
            .field("layouts", &self.layouts)
            .finish()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScalarLayout {
    pub ty: Type,
    pub size: u32,
    pub alignment: u32,
}

/// A raw, pure compile-time CFG candidate. The ordinary IR does not grant CT
/// authority: the independent verifier checks the closed operation set as well
/// as the ordinary control-flow/type/provenance invariants before execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtProgram {
    pub module: TypedModule,
    pub plan: CandidatePlan,
    pub entry: FunctionId,
    pub max_steps: u64,
    pub max_call_depth: usize,
    pub max_live_slots: usize,
}

/// A CT query's operation set is explicit and closed; unknown/forbidden
/// operations are data to reject, never dynamically executable host callbacks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtModule {
    pub schema_version: u32,
    pub input_fingerprint: Fingerprint,
    pub target: TargetProfile,
    pub instructions: Vec<CtInstruction>,
    pub result: ValueId,
    pub max_steps: u64,
    pub dependencies: Vec<CtDependency>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDependency {
    pub name: String,
    pub fingerprint: Fingerprint,
    pub completed: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtInstruction {
    pub result: ValueId,
    pub ty: Type,
    pub kind: CtInstructionKind,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtInstructionKind {
    Constant(Constant),
    ConvertInteger {
        value: ValueId,
        target: Type,
    },
    /// Checked numeric conversion with numeric endpoints before Types resolves the operand.
    ConvertNumber {
        value: ValueId,
        target: Type,
    },
    Unary {
        op: UnaryOp,
        operand: ValueId,
    },
    Binary {
        op: BinaryOp,
        left: ValueId,
        right: ValueId,
    },
    Forbidden(String),
}

impl PersistentItem for TypedFunction {
    fn persistent_fingerprint(&self) -> Fingerprint {
        let mut bytes = FingerprintStream::new();
        bytes.extend_from_slice(b"ZIL-TYPED-FUNCTION-STRUCTURAL-V1\0");
        encode_function(&mut bytes, self);
        bytes.finish()
    }
}
impl TypedModule {
    /// Versioned product identity. Unlike the legacy Wire digest, function
    /// encodings and subtree hashes are retained across immutable snapshots.
    /// All non-function inputs are encoded in full; this grants no authority.
    pub fn structural_fingerprint(&self) -> Fingerprint {
        let mut bytes = FingerprintStream::new();
        bytes.extend_from_slice(b"ZIL-TYPED-MODULE-STRUCTURAL-V1\0");
        encode_module_with_functions(&mut bytes, self, true);
        bytes.finish()
    }
    pub fn fingerprint(&self) -> Fingerprint {
        // Versioned, length-delimited encoding is implemented below. This must
        // not depend on Debug formatting or process-randomized hashing.
        let mut bytes = FingerprintStream::new();
        encode_module(&mut bytes, self);
        bytes.finish()
    }
}
impl CandidatePlan {
    pub fn empty_for(module: &TypedModule) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            module_fingerprint: module.fingerprint(),
            target_fingerprint: module.target.fingerprint(),
            functions: PersistentVec::new(),
            origins: Vec::new(),
            responsibilities: Vec::new(),
            unsupported: Vec::new(),
        }
    }
}

// Byte sinks share the exact versioned encoder. Vec remains available to the
// existing callable fingerprints and byte-level controls; only module hashing
// uses fixed storage instead of materializing the complete encoding.
trait FingerprintSink {
    fn push(&mut self, byte: u8);
    fn extend_from_slice(&mut self, bytes: &[u8]);
}
impl FingerprintSink for Vec<u8> {
    fn push(&mut self, byte: u8) {
        Vec::push(self, byte);
    }
    fn extend_from_slice(&mut self, bytes: &[u8]) {
        Vec::extend_from_slice(self, bytes);
    }
}
struct FingerprintStream {
    hash: FingerprintBuilder,
    buffer: [u8; 4096],
    used: usize,
}
impl FingerprintStream {
    fn new() -> Self {
        Self {
            hash: FingerprintBuilder::new(),
            buffer: [0; 4096],
            used: 0,
        }
    }
    fn flush(&mut self) {
        self.hash.update(&self.buffer[..self.used]);
        self.used = 0;
    }
    fn finish(mut self) -> Fingerprint {
        self.flush();
        self.hash.digest()
    }
}
impl FingerprintSink for FingerprintStream {
    fn push(&mut self, byte: u8) {
        if self.used == self.buffer.len() {
            self.flush();
        }
        self.buffer[self.used] = byte;
        self.used += 1;
    }
    fn extend_from_slice(&mut self, bytes: &[u8]) {
        if bytes.len() > self.buffer.len() - self.used {
            self.flush();
        }
        if bytes.len() >= self.buffer.len() {
            self.hash.update(bytes);
        } else {
            self.buffer[self.used..self.used + bytes.len()].copy_from_slice(bytes);
            self.used += bytes.len();
        }
    }
}

fn encode_module(bytes: &mut impl FingerprintSink, module: &TypedModule) {
    encode_module_with_functions(bytes, module, false);
}
fn encode_module_with_functions(
    bytes: &mut impl FingerprintSink,
    module: &TypedModule,
    structural: bool,
) {
    bytes.extend_from_slice(b"ZIL-TYPED-MODULE-V46\0");
    encode_u32(bytes, module.schema_version);
    encode_u32(bytes, module.module_id.0);
    bytes.extend_from_slice(&module.source_fingerprint.0);
    encode_text(bytes, &module.target.to_json());
    encode_life_inventory(bytes, &module.life_declarations, &module.life_instances);
    encode_len(bytes, module.dyn_declarations.len());
    for row in &module.dyn_declarations {
        encode_dyn_declaration(bytes, row);
    }
    encode_len(bytes, module.dyn_interfaces.len());
    for row in &module.dyn_interfaces {
        encode_dyn_interface(bytes, row);
    }
    encode_len(bytes, module.dyn_vtables.len());
    for row in &module.dyn_vtables {
        encode_dyn_vtable(bytes, row);
    }

    encode_len(bytes, module.functions.len());
    if structural {
        bytes.extend_from_slice(&module.functions.structural_fingerprint().0);
    } else {
        for f in &module.functions {
            encode_function(bytes, f);
        }
    }
    encode_len(bytes, module.closures.len());
    for closure in &module.closures {
        encode_u32(bytes, closure.id.0);
        encode_u32(bytes, closure.function.0);
        encode_len(bytes, closure.captures.len());
        for ty in &closure.captures {
            encode_type(bytes, ty);
        }
        encode_len(bytes, closure.parameters.len());
        for ty in &closure.parameters {
            encode_type(bytes, ty);
        }
        encode_type(bytes, &closure.result);
        bytes.push(match closure.kind {
            CallableKind::FunctionPointer => 0,
            CallableKind::Fn => 1,
            CallableKind::FnMut => 2,
            CallableKind::FnOnce => 3,
        });
        encode_len(bytes, closure.lending_captures.len());
        for index in &closure.lending_captures {
            encode_u32(bytes, *index);
        }
        encode_storage_contract(bytes, &closure.storage_life);
    }
    encode_len(bytes, module.resources.len());
    for resource in &module.resources {
        encode_text(bytes, &resource.name);
        encode_type(bytes, &resource.payload);
        bytes.push(u8::from(resource.teardown.is_some()));
        if let Some(teardown) = resource.teardown {
            encode_u32(bytes, teardown.0);
        }
    }
    encode_len(bytes, module.unsupported.len());
    for feature in &module.unsupported {
        encode_text(bytes, &feature.feature);
        encode_optional_span(bytes, feature.span);
    }
}
fn encode_u32(b: &mut impl FingerprintSink, n: u32) {
    b.extend_from_slice(&n.to_le_bytes());
}
fn encode_len(b: &mut impl FingerprintSink, n: usize) {
    b.extend_from_slice(&(n as u64).to_le_bytes());
}
fn encode_text(b: &mut impl FingerprintSink, s: &str) {
    encode_len(b, s.len());
    b.extend_from_slice(s.as_bytes());
}
fn encode_span(b: &mut impl FingerprintSink, s: Span) {
    encode_u32(b, s.file.0);
    encode_len(b, s.start);
    encode_len(b, s.end);
}
fn encode_optional_span(b: &mut impl FingerprintSink, s: Option<Span>) {
    b.push(u8::from(s.is_some()));
    if let Some(s) = s {
        encode_span(b, s);
    }
}
fn encode_storage_path(b: &mut impl FingerprintSink, path: &[PlaceProjection]) {
    encode_len(b, path.len());
    for step in path {
        match step {
            PlaceProjection::Field(i) => {
                b.push(0);
                encode_u32(b, *i);
            }
            PlaceProjection::Variant(p) => {
                b.push(1);
                encode_u32(b, p.variant);
                encode_u32(b, p.index);
            }
            PlaceProjection::ErrorArm(error) => b.extend_from_slice(&[2, u8::from(*error)]),
        }
    }
}
fn encode_life_type_path(b: &mut impl FingerprintSink, path: &[LifeTypeProjection]) {
    encode_len(b, path.len());
    for step in path {
        match step {
            LifeTypeProjection::Place(p) => {
                b.push(0);
                encode_storage_path(b, std::slice::from_ref(p));
            }
            LifeTypeProjection::TypeArgument(i) => {
                b.push(1);
                encode_u32(b, *i);
            }
            LifeTypeProjection::Pointee => b.push(2),
            LifeTypeProjection::Element => b.push(3),
            LifeTypeProjection::Parameter(i) => {
                b.push(4);
                encode_u32(b, *i);
            }
            LifeTypeProjection::Result => b.push(5),
            LifeTypeProjection::Port(i) => {
                b.push(6);
                encode_u32(b, *i);
            }
            LifeTypeProjection::AssociatedType(i) => {
                b.push(7);
                encode_u32(b, *i);
            }
        }
    }
}
fn encode_storage_shape(b: &mut impl FingerprintSink, shape: &StorageLifeShape) {
    encode_u32(b, shape.binder_count);
    encode_len(b, shape.slots.len());
    for slot in &shape.slots {
        encode_storage_path(b, &slot.path);
        encode_u32(b, slot.binder);
        b.push(slot.mode as u8);
    }
    encode_len(b, shape.retained.len());
    for binding in &shape.retained {
        encode_life_type_path(b, &binding.path);
        encode_u32(b, binding.class);
        encode_u32(b, binding.binder);
    }
    encode_len(b, shape.nominal_arguments.len());
    for argument in &shape.nominal_arguments {
        encode_life_type_path(b, &argument.path);
        encode_u32(b, argument.declaration.module.0);
        encode_u32(b, argument.declaration.declaration.0);
        b.push(u8::from(argument.declaration.compiler));
        encode_u32(b, argument.argument);
        encode_u32(b, argument.binder);
    }
}
fn encode_declaration_id(b: &mut impl FingerprintSink, id: &LifeDeclarationId) {
    encode_u32(b, id.module.0);
    encode_u32(b, id.declaration.0);
    b.push(u8::from(id.compiler));
}
fn encode_declaration_identity(b: &mut impl FingerprintSink, id: &LifeDeclarationIdentity) {
    encode_text(b, &id.package);
    for path in [&id.module_path, &id.declaration_path] {
        encode_len(b, path.len());
        for part in path {
            encode_text(b, part);
        }
    }
    encode_len(b, id.generation_path.len());
    for part in &id.generation_path {
        encode_u32(b, *part);
    }
}
fn encode_parameter_kind(b: &mut impl FingerprintSink, kind: &LifeParameterKind) {
    match kind {
        LifeParameterKind::Type => b.push(0),
        LifeParameterKind::Const(ty) => {
            b.push(1);
            encode_type(b, ty);
        }
        LifeParameterKind::Lifetime => b.push(2),
    }
}
fn encode_storage_binder(b: &mut impl FingerprintSink, binder: &StorageBinder) {
    match binder {
        StorageBinder::Fixed(i) => {
            b.push(0);
            encode_u32(b, *i)
        }
        StorageBinder::Invocation(i) => {
            b.push(1);
            encode_u32(b, *i)
        }
    }
}
fn encode_storage_position(b: &mut impl FingerprintSink, position: &StoragePosition) {
    match position.base {
        StorageBase::Parameter(i) => {
            b.push(0);
            encode_u32(b, i)
        }
        StorageBase::Capture(i) => {
            b.push(1);
            encode_u32(b, i)
        }
        StorageBase::Result => b.push(2),
    }
    encode_len(b, position.path.len());
    for step in &position.path {
        match step {
            ContractProjection::Place(p) => {
                b.push(0);
                encode_storage_path(b, std::slice::from_ref(p))
            }
            ContractProjection::RetainedPort(i) => {
                b.push(1);
                encode_u32(b, *i)
            }
            ContractProjection::RetainedClass(i) => {
                b.push(2);
                encode_u32(b, *i)
            }
        }
    }
}
fn encode_life_maps(
    b: &mut impl FingerprintSink,
    retained: &[StorageRetainedBinding],
    nominal_arguments: &[NominalLifeArgument],
) {
    encode_len(b, retained.len());
    for binding in retained {
        encode_life_type_path(b, &binding.path);
        encode_u32(b, binding.class);
        encode_u32(b, binding.binder);
    }
    encode_len(b, nominal_arguments.len());
    for argument in nominal_arguments {
        encode_life_type_path(b, &argument.path);
        encode_declaration_id(b, &argument.declaration);
        encode_u32(b, argument.argument);
        encode_u32(b, argument.binder);
    }
}
fn encode_retained_shape(b: &mut impl FingerprintSink, shape: &RetainedLifeShape) {
    encode_u32(b, shape.binder_count);
    encode_len(b, shape.ports.len());
    for port in &shape.ports {
        encode_type(b, &port.leaf);
        b.push(port.mode as u8);
        encode_u32(b, port.binder);
    }
    encode_life_maps(b, &shape.retained, &shape.nominal_arguments);
}
fn encode_required_storage_contract(b: &mut impl FingerprintSink, contract: &StorageLifeContract) {
    encode_u32(b, contract.fixed_binder_count);
    encode_u32(b, contract.invocation_binder_count);
    encode_len(b, contract.bindings.len());
    for binding in &contract.bindings {
        encode_storage_position(b, &binding.position);
        encode_storage_binder(b, &binding.binder);
    }
    encode_len(b, contract.classes.len());
    for binding in &contract.classes {
        match binding.position.base {
            StorageBase::Parameter(i) => {
                b.push(0);
                encode_u32(b, i);
            }
            StorageBase::Capture(i) => {
                b.push(1);
                encode_u32(b, i);
            }
            StorageBase::Result => b.push(2),
        }
        encode_life_type_path(b, &binding.position.path);
        encode_u32(b, binding.position.class);
        encode_storage_binder(b, &binding.binder);
    }
}
fn encode_storage_contract(b: &mut impl FingerprintSink, contract: &Option<StorageLifeContract>) {
    b.push(u8::from(contract.is_some()));
    if let Some(contract) = contract {
        encode_required_storage_contract(b, contract);
    }
}
fn encode_retained_port_binding(b: &mut impl FingerprintSink, binding: &RetainedPortBinding) {
    encode_u32(b, binding.port);
    encode_storage_position(b, &binding.source);
}
fn encode_life_inventory(
    b: &mut impl FingerprintSink,
    declarations: &[LifeNominalDeclaration],
    instances: &[LifeNominalInstance],
) {
    encode_len(b, declarations.len());
    for declaration in declarations {
        encode_declaration_id(b, &declaration.id);
        encode_declaration_identity(b, &declaration.identity);
        b.push(match declaration.kind {
            LifeNominalKind::Record => 0,
            LifeNominalKind::Package => 1,
            LifeNominalKind::Resource => 2,
            LifeNominalKind::Enum => 3,
        });
        encode_len(b, declaration.parameters.len());
        for parameter in &declaration.parameters {
            encode_parameter_kind(b, parameter);
        }
    }
    encode_len(b, instances.len());
    for instance in instances {
        encode_declaration_id(b, &instance.declaration);
        encode_type(b, &instance.ty);
        encode_len(b, instance.arguments.len());
        for argument in &instance.arguments {
            match argument {
                LifeNominalActual::Type(ty) => {
                    b.push(0);
                    encode_type(b, ty)
                }
                LifeNominalActual::Const(value) => {
                    b.push(1);
                    encode_constant(b, value)
                }
                LifeNominalActual::Lifetime(i) => {
                    b.push(2);
                    encode_u32(b, *i)
                }
            }
        }
    }
}

fn encode_type(b: &mut impl FingerprintSink, t: &Type) {
    match t {
        Type::OwnedDyn { interface } => {
            b.push(40);
            encode_dyn_interface_id(b, interface);
        }
        Type::Dyn { interface } => {
            b.push(36);
            encode_dyn_interface_id(b, interface);
        }
        Type::BorrowedStorage { storage, life } => {
            b.push(35);
            encode_storage_shape(b, life);
            encode_type(b, storage)
        }
        Type::Unit => b.push(0),
        Type::Bool => b.push(1),
        Type::I32 => b.push(2),
        Type::I8 => b.push(42),
        Type::I16 => b.push(43),
        Type::Isize => b.push(44),
        Type::I64 => b.push(3),
        Type::U64 => b.push(26),
        Type::Usize => b.push(27),
        Type::F32 => b.push(28),
        Type::F64 => b.push(29),
        Type::U8 => b.push(18),
        Type::U16 => b.push(19),
        Type::U32 => b.push(41),
        Type::ByteBuffer => b.push(24),
        Type::OwnedStore { element } => {
            b.push(25);
            encode_type(b, element);
        }
        Type::Registration { environment } => {
            b.push(32);
            encode_type(b, environment);
        }
        Type::AsyncRegistration { environment } => {
            b.push(37);
            encode_type(b, environment);
        }
        Type::TaskI32 => b.push(33),
        Type::Task {
            result,
            retained_life,
        } => {
            b.push(34);
            encode_retained_shape(b, retained_life);
            encode_type(b, result);
        }
        Type::ForeignHandle => b.push(20),
        Type::SyncCell(kind) => {
            b.push(30);
            b.push(*kind as u8);
        }
        Type::Mutex { payload } => {
            b.push(38);
            encode_type(b, payload);
        }
        Type::MutexGuard {
            payload,
            retained_life,
        } => {
            b.push(39);
            encode_retained_shape(b, retained_life);
            encode_type(b, payload);
        }
        Type::CWideString => b.push(21),
        Type::ForeignCallback { parameters, result } => {
            b.push(22);
            encode_len(b, parameters.len());
            for ty in parameters {
                encode_type(b, ty);
            }
            encode_type(b, result);
        }
        Type::Tuple(ts) => {
            b.push(4);
            encode_len(b, ts.len());
            for t in ts {
                encode_type(b, t);
            }
        }
        Type::Array { element, length } => {
            b.push(14);
            encode_type(b, element);
            encode_u32(b, *length);
        }
        Type::Slice { element } => {
            b.push(23);
            encode_type(b, element);
        }
        Type::Record { name, fields } => {
            b.push(13);
            encode_text(b, name);
            encode_len(b, fields.len());
            for (name, ty) in fields {
                encode_text(b, name);
                encode_type(b, ty);
            }
        }
        Type::Package { name, fields } => {
            b.push(17);
            encode_text(b, name);
            encode_len(b, fields.len());
            for (name, ty) in fields {
                encode_text(b, name);
                encode_type(b, ty);
            }
        }
        Type::Enum { name, variants } => {
            b.push(15);
            encode_text(b, name);
            encode_len(b, variants.len());
            for (name, fields) in variants {
                encode_text(b, name);
                encode_len(b, fields.len());
                for ty in fields {
                    encode_type(b, ty);
                }
            }
        }
        Type::ErrorUnion { error, result } => {
            b.push(16);
            encode_type(b, error);
            encode_type(b, result);
        }
        Type::Named(s) => {
            b.push(5);
            encode_text(b, s);
        }
        Type::Address { mutable, inner } => {
            b.extend_from_slice(&[6, u8::from(*mutable)]);
            encode_type(b, inner);
        }
        Type::Managed(t) => {
            b.push(7);
            encode_type(b, t);
        }
        Type::Desc(t) => {
            b.push(8);
            encode_type(b, t);
        }
        Type::FunctionPointer { parameters, result } => {
            b.push(9);
            encode_len(b, parameters.len());
            for t in parameters {
                encode_type(b, t);
            }
            encode_type(b, result);
        }
        Type::BorrowedFunctionPointer {
            parameters,
            result,
            life,
        } => {
            b.push(31);
            encode_required_storage_contract(b, life);
            encode_len(b, parameters.len());
            for ty in parameters {
                encode_type(b, ty);
            }
            encode_type(b, result);
        }
        Type::Closure(id) => {
            b.push(10);
            encode_u32(b, id.0);
        }
        Type::Opaque { identity, hidden } => {
            b.push(11);
            encode_u32(b, identity.0);
            encode_type(b, hidden);
        }
        Type::Unsupported(s) => {
            b.push(12);
            encode_text(b, s);
        }
    }
}
fn encode_function(b: &mut impl FingerprintSink, f: &TypedFunction) {
    b.push(f.call_kind as u8);
    encode_storage_contract(b, &f.storage_life);
    b.push(u8::from(f.reference_return_bound.is_some()));
    if let Some(bound) = &f.reference_return_bound {
        encode_len(b, bound.len());
        for parameter in bound {
            encode_u32(b, *parameter);
        }
    }
    b.push(u8::from(f.callable_return_contract.is_some()));
    if let Some(contract) = &f.callable_return_contract {
        encode_callable_return_contract(b, contract);
    }
    b.push(u8::from(f.callable_contract.is_some()));
    if let Some(contract) = &f.callable_contract {
        encode_callable_contract(b, contract);
    }
    encode_u32(b, f.id.0);
    encode_text(b, &f.name);
    if let Some(contract) = &f.protocol_contract {
        b.push(2); // distinct from the following legacy foreign presence bit
        encode_text(b, "ZIL-FUNCTION-PROTOCOL-1");
        encode_protocol_contract(b, contract);
    }
    b.push(u8::from(f.foreign.is_some()));
    if let Some(foreign) = &f.foreign {
        encode_text(b, &foreign.abi);
        encode_text(b, &foreign.symbol);
        encode_len(b, foreign.noescape_parameters.len());
        for parameter in &foreign.noescape_parameters {
            encode_u32(b, *parameter);
        }
        b.push(u8::from(foreign.static_callbacks));
        b.push(scoped_callback_role_tag(foreign.scoped_callback));
    }
    b.push(u8::from(f.environment.is_some()));
    if let Some(environment) = f.environment {
        encode_u32(b, environment.0);
    }
    encode_len(b, f.parameters.len());
    for p in &f.parameters {
        encode_u32(b, p.0);
    }
    encode_type(b, &f.return_type);
    b.push(passing_mode_tag(f.result_mode));
    encode_len(b, f.locals.len());
    for l in &f.locals {
        encode_u32(b, l.id.0);
        encode_text(b, &l.name);
        encode_type(b, &l.ty);
        b.push(u8::from(l.mutable));
        encode_span(b, l.span);
    }
    encode_len(b, f.values.len());
    for t in &f.values {
        encode_type(b, t);
    }
    encode_len(b, f.blocks.len());
    for block in &f.blocks {
        encode_u32(b, block.id.0);
        encode_len(b, block.instructions.len());
        for i in &block.instructions {
            b.push(u8::from(i.result.is_some()));
            if let Some(v) = i.result {
                encode_u32(b, v.0);
            }
            encode_instruction(b, &i.kind);
            encode_span(b, i.span);
        }
        match &block.terminator {
            Terminator::Return(v) => {
                b.push(0);
                b.push(u8::from(v.is_some()));
                if let Some(v) = v {
                    encode_u32(b, v.0);
                }
            }
            Terminator::Jump(t) => {
                b.push(1);
                encode_u32(b, t.0);
            }
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                b.push(2);
                encode_u32(b, condition.0);
                encode_u32(b, then_block.0);
                encode_u32(b, else_block.0);
            }
            Terminator::Unreachable => b.push(3),
            Terminator::Trap => b.push(4),
        }
    }
    encode_u32(b, f.entry.0);
    encode_span(b, f.span);
}
fn encode_constant(b: &mut impl FingerprintSink, c: &Constant) {
    match c {
        Constant::Unit => b.push(0),
        Constant::Bool(v) => b.extend_from_slice(&[1, u8::from(*v)]),
        Constant::U8(v) => b.extend_from_slice(&[4, *v]),
        Constant::U16(v) => {
            b.push(5);
            b.extend_from_slice(&v.to_le_bytes());
        }
        Constant::U32(v) => {
            b.push(13);
            b.extend_from_slice(&v.to_le_bytes());
        }
        Constant::NullHandle => b.push(6),
        Constant::NullWideString => b.push(7),
        Constant::WideString(units) => {
            b.push(8);
            encode_len(b, units.len());
            for unit in units {
                b.extend_from_slice(&unit.to_le_bytes());
            }
        }
        Constant::I32(v) => {
            b.push(2);
            b.extend_from_slice(&v.to_le_bytes());
        }
        Constant::I8(v) => {
            b.push(14);
            b.extend_from_slice(&v.to_le_bytes());
        }
        Constant::I16(v) => {
            b.push(15);
            b.extend_from_slice(&v.to_le_bytes());
        }
        Constant::Isize(v) => {
            b.push(16);
            b.extend_from_slice(&v.to_le_bytes());
        }
        Constant::I64(v) => {
            b.push(3);
            b.extend_from_slice(&v.to_le_bytes());
        }
        Constant::U64(v) => {
            b.push(9);
            b.extend_from_slice(&v.to_le_bytes());
        }
        Constant::Usize(v) => {
            b.push(10);
            b.extend_from_slice(&v.to_le_bytes());
        }
        Constant::F32(bits) => {
            b.push(11);
            b.extend_from_slice(&bits.to_le_bytes());
        }
        Constant::F64(bits) => {
            b.push(12);
            b.extend_from_slice(&bits.to_le_bytes());
        }
    }
}
fn encode_instruction(b: &mut impl FingerprintSink, i: &InstructionKind) {
    match i {
        InstructionKind::MakeDynOwned {
            interface,
            vtable,
            source,
            route,
        } => {
            b.push(148);
            encode_dyn_interface_id(b, interface);
            encode_dyn_vtable_id(b, vtable);
            encode_u32(b, source.0);
            b.push(match route {
                DynOwnedStorage::Heap => 1,
            });
        }
        InstructionKind::BorrowDynOwned { local, mutable } => {
            b.push(149);
            encode_u32(b, local.0);
            b.push(u8::from(*mutable));
        }
        InstructionKind::DropDynOwned { local } => {
            b.push(150);
            encode_u32(b, local.0);
        }
        InstructionKind::MakeDynBorrow {
            interface,
            vtable,
            source,
            mutable,
        } => {
            b.push(131);
            encode_dyn_interface_id(b, interface);
            encode_dyn_vtable_id(b, vtable);
            encode_u32(b, source.0);
            b.push(u8::from(*mutable));
        }
        InstructionKind::CallDynSlot {
            callee,
            interface,
            slot,
            arguments,
        } => {
            b.push(132);
            encode_u32(b, callee.0);
            encode_dyn_interface_id(b, interface);
            encode_u32(b, *slot);
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::ConvertNumber { value, target } => {
            b.push(98);
            encode_u32(b, value.0);
            encode_type(b, target);
        }
        InstructionKind::ConvertInteger { value, target } => {
            b.push(59);
            encode_u32(b, value.0);
            encode_type(b, target);
        }
        InstructionKind::MakePackage { fields } => {
            b.push(49);
            encode_len(b, fields.len());
            for field in fields {
                encode_u32(b, field.0);
            }
        }
        InstructionKind::MakeOwnedArray { elements } => {
            b.push(72);
            encode_len(b, elements.len());
            for element in elements {
                encode_u32(b, element.0);
            }
        }
        InstructionKind::ProjectOwnedArrayField {
            local,
            index,
            field,
        } => {
            b.push(73);
            encode_u32(b, local.0);
            encode_u32(b, *index);
            encode_u32(b, *field);
        }
        InstructionKind::TakeOwnedArrayElement { local, index } => {
            b.push(74);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::DropOwnedArrayElement { local, index } => {
            b.push(75);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::StoreOwnedArrayElement {
            local,
            index,
            value,
        } => {
            b.push(76);
            encode_u32(b, local.0);
            encode_u32(b, *index);
            encode_u32(b, value.0);
        }
        InstructionKind::DropOwnedArray { local } => {
            b.push(77);
            encode_u32(b, local.0);
        }
        InstructionKind::BorrowOwnedArrayElement {
            local,
            index,
            mutable,
        } => {
            b.push(79);
            encode_u32(b, local.0);
            encode_u32(b, *index);
            b.push(u8::from(*mutable));
        }
        InstructionKind::BorrowPackageField {
            local,
            index,
            mutable,
        } => {
            b.push(87);
            encode_u32(b, local.0);
            encode_u32(b, *index);
            b.push(u8::from(*mutable));
        }
        InstructionKind::ReadPackageField { local, index } => {
            b.push(50);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::DescribePackageField { local, index } => {
            b.push(51);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::DropPackage { local } => {
            b.push(52);
            encode_u32(b, local.0);
        }
        InstructionKind::TakePackageField { local, index } => {
            b.push(54);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::DropPackageField { local, index } => {
            b.push(55);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::StorePackageField {
            local,
            index,
            value,
        } => {
            b.push(56);
            encode_u32(b, local.0);
            encode_u32(b, *index);
            encode_u32(b, value.0);
        }
        InstructionKind::BindPackageCapture { local, index } => {
            b.push(57);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::EndPackageCapture { local, index } => {
            b.push(58);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::RegisterDefer { scope, local } => {
            b.push(89);
            encode_u32(b, scope.0);
            encode_u32(b, local.0);
        }
        InstructionKind::ScopeEnter { scope } => {
            b.push(44);
            encode_u32(b, scope.0);
        }
        InstructionKind::ScopeExit { scope, kind } => {
            b.push(45);
            encode_u32(b, scope.0);
            b.push(cleanup_exit_tag(*kind));
        }
        InstructionKind::MakeErrorUnion { is_error, value } => {
            b.push(46);
            b.push(u8::from(*is_error));
            encode_u32(b, value.0);
        }
        InstructionKind::ErrorTag { value } => {
            b.push(47);
            encode_u32(b, value.0);
        }
        InstructionKind::ReadErrorUnion { value, is_error } => {
            b.push(48);
            encode_u32(b, value.0);
            b.push(u8::from(*is_error));
        }
        InstructionKind::TakeErrorUnion { value, is_error } => {
            b.push(70);
            encode_u32(b, value.0);
            b.push(u8::from(*is_error));
        }
        InstructionKind::DropErrorUnion { local } => {
            b.push(71);
            encode_u32(b, local.0);
        }
        InstructionKind::MakeManaged { fields } => {
            b.push(34);
            encode_len(b, fields.len());
            for field in fields {
                encode_u32(b, field.0);
            }
        }
        InstructionKind::LoadStoredBorrow {
            storage,
            path,
            mode,
        } => {
            b.push(129);
            encode_u32(b, storage.0);
            encode_storage_path(b, path);
            match mode {
                StoredBorrowLoad::CopyShared => b.push(0),
                StoredBorrowLoad::CopyDescription => b.push(1),
                StoredBorrowLoad::Reborrow { mutable } => {
                    b.extend_from_slice(&[2, u8::from(*mutable)])
                }
                StoredBorrowLoad::Take => b.push(3),
            }
        }
        InstructionKind::StoreStoredBorrow {
            storage,
            path,
            value,
        } => {
            b.push(130);
            encode_u32(b, storage.0);
            encode_storage_path(b, path);
            encode_u32(b, value.0);
        }
        InstructionKind::DescribeAddress { address } => {
            b.push(109);
            encode_u32(b, address.0);
        }
        InstructionKind::DescribeLocal { local } => {
            b.push(35);
            encode_u32(b, local.0);
        }
        InstructionKind::ReadDesc { desc, index } => {
            b.push(36);
            encode_u32(b, desc.0);
            encode_u32(b, *index);
        }
        InstructionKind::WriteDesc { desc, index, value } => {
            b.push(37);
            encode_u32(b, desc.0);
            encode_u32(b, *index);
            encode_u32(b, value.0);
        }
        InstructionKind::DropManaged { local } => {
            b.push(38);
            encode_u32(b, local.0);
        }
        InstructionKind::DescribeCapture { index } => {
            b.push(39);
            encode_u32(b, *index);
        }
        InstructionKind::TakeCapture { index } => {
            b.push(40);
            encode_u32(b, *index);
        }
        InstructionKind::BorrowCapture { index, mutable } => {
            b.push(78);
            encode_u32(b, *index);
            b.push(u8::from(*mutable));
        }
        InstructionKind::CallClosureBorrowed {
            callee,
            kind,
            arguments,
        } => {
            b.push(53);
            encode_u32(b, callee.0);
            b.push(callable_kind_tag(*kind));
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::CallClosureShared { callee, arguments } => {
            b.push(41);
            encode_u32(b, callee.0);
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::CallClosureOnce { callee, arguments } => {
            b.push(42);
            encode_u32(b, callee.0);
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::DropClosure { local } => {
            b.push(43);
            encode_u32(b, local.0);
        }
        InstructionKind::UnsafeEnter { region } => {
            b.push(31);
            encode_u32(b, *region);
        }
        InstructionKind::UnsafeExit { region } => {
            b.push(32);
            encode_u32(b, *region);
        }
        InstructionKind::ForeignCall {
            function,
            arguments,
            region,
        } => {
            b.push(33);
            encode_u32(b, function.0);
            encode_u32(b, *region);
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::StaticUtf8 { bytes } => {
            b.push(139);
            encode_len(b, bytes.len());
            b.extend_from_slice(bytes);
        }
        InstructionKind::Constant(c) => {
            b.push(0);
            encode_constant(b, c);
        }
        InstructionKind::Load(l) => {
            b.push(1);
            encode_u32(b, l.0);
        }
        InstructionKind::Store { local, value } => {
            b.push(2);
            encode_u32(b, local.0);
            encode_u32(b, value.0);
        }
        InstructionKind::Unary { op, operand } => {
            b.push(3);
            b.push(*op as u8);
            encode_u32(b, operand.0);
        }
        InstructionKind::Binary { op, left, right } => {
            b.push(4);
            b.push(*op as u8);
            encode_u32(b, left.0);
            encode_u32(b, right.0);
        }
        InstructionKind::Call {
            function,
            arguments,
        } => {
            b.push(5);
            encode_u32(b, function.0);
            encode_len(b, arguments.len());
            for a in arguments {
                encode_u32(b, a.0);
            }
        }
        InstructionKind::WithForeignCallback {
            register,
            close,
            callback,
            body,
            region,
        } => {
            b.push(94);
            encode_u32(b, register.0);
            encode_u32(b, close.0);
            encode_u32(b, callback.0);
            encode_u32(b, body.0);
            encode_u32(b, *region);
        }
        InstructionKind::WithScopedThread {
            worker,
            argument,
            body,
        } => {
            b.push(95);
            encode_u32(b, worker.0);
            encode_u32(b, argument.0);
            encode_u32(b, body.0);
        }
        InstructionKind::WithSyncCell {
            kind,
            initial,
            body,
        } => {
            b.push(96);
            b.push(*kind as u8);
            encode_u32(b, initial.0);
            encode_u32(b, body.0);
        }
        InstructionKind::SyncI32 {
            op,
            address,
            value,
            ordering,
        } => {
            b.push(97);
            b.push(*op as u8);
            encode_u32(b, address.0);
            b.push(u8::from(value.is_some()));
            if let Some(value) = value {
                encode_u32(b, value.0);
            }
            b.push(*ordering as u8);
        }
        InstructionKind::SyncI32CompareExchange {
            address,
            expected,
            desired,
            success,
            failure,
        } => {
            b.push(133);
            encode_u32(b, address.0);
            encode_u32(b, expected.0);
            encode_u32(b, desired.0);
            b.push(*success as u8);
            b.push(*failure as u8);
        }
        InstructionKind::SyncInteger {
            op,
            address,
            value,
            ordering,
        } => {
            b.push(146);
            b.push(*op as u8);
            encode_u32(b, address.0);
            b.push(u8::from(value.is_some()));
            if let Some(value) = value {
                encode_u32(b, value.0);
            }
            b.push(*ordering as u8);
        }
        InstructionKind::SyncIntegerCompareExchange {
            address,
            expected,
            desired,
            success,
            failure,
        } => {
            b.push(147);
            encode_u32(b, address.0);
            encode_u32(b, expected.0);
            encode_u32(b, desired.0);
            b.push(*success as u8);
            b.push(*failure as u8);
        }
        InstructionKind::MutexCreate { value } => {
            b.push(141);
            encode_u32(b, value.0);
        }
        InstructionKind::MutexLock { mutex } => {
            b.push(142);
            encode_u32(b, mutex.0);
        }
        InstructionKind::MutexValue { guard } => {
            b.push(143);
            encode_u32(b, guard.0);
        }
        InstructionKind::MutexUnlock { guard } => {
            b.push(144);
            encode_u32(b, guard.0);
        }
        InstructionKind::DropMutexGuard { local } => {
            b.push(145);
            encode_u32(b, local.0);
        }
        InstructionKind::TaskStart { worker, argument } => {
            b.push(99);
            encode_u32(b, worker.0);
            encode_u32(b, argument.0);
        }
        InstructionKind::SuspendCall {
            function,
            arguments,
        } => {
            b.push(125);
            encode_u32(b, function.0);
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::TaskCreate {
            worker,
            arguments,
            port_bindings,
        } => {
            b.push(128);
            encode_u32(b, worker.0);
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
            encode_len(b, port_bindings.len());
            for binding in port_bindings {
                encode_retained_port_binding(b, binding);
            }
        }
        InstructionKind::TaskYield => b.push(100),
        InstructionKind::TaskValid { task } => {
            b.push(101);
            encode_u32(b, task.0);
        }
        InstructionKind::TaskPoll { task } => {
            b.push(102);
            encode_u32(b, task.0);
        }
        InstructionKind::TaskRequestCancel { task } => {
            b.push(103);
            encode_u32(b, task.0);
        }
        InstructionKind::TaskPollClose { task } => {
            b.push(104);
            encode_u32(b, task.0);
        }
        InstructionKind::TaskFinishDestroy { task } => {
            b.push(105);
            encode_u32(b, task.0);
        }
        InstructionKind::RegistrationStart {
            register,
            close,
            callback,
            region,
        } => {
            b.push(111);
            encode_u32(b, register.0);
            encode_u32(b, close.0);
            encode_u32(b, callback.0);
            encode_u32(b, *region);
        }
        InstructionKind::RegistrationValid { registration } => {
            b.push(112);
            encode_u32(b, registration.0);
        }
        InstructionKind::RegistrationClose { registration } => {
            b.push(113);
            encode_u32(b, registration.0);
        }
        InstructionKind::AsyncRegistrationStart {
            register,
            request_close,
            poll_drain,
            callback,
            region,
        } => {
            b.push(134);
            encode_u32(b, register.0);
            encode_u32(b, request_close.0);
            encode_u32(b, poll_drain.0);
            encode_u32(b, callback.0);
            encode_u32(b, *region);
        }
        InstructionKind::AsyncRegistrationValid { registration } => {
            b.push(135);
            encode_u32(b, registration.0);
        }
        InstructionKind::AsyncRegistrationRequestClose { registration } => {
            b.push(136);
            encode_u32(b, registration.0);
        }
        InstructionKind::AsyncRegistrationPollClose { registration } => {
            b.push(137);
            encode_u32(b, registration.0);
        }
        InstructionKind::AsyncRegistrationFinishDestroy { registration } => {
            b.push(138);
            encode_u32(b, registration.0);
        }
        InstructionKind::AsyncRegistrationFinishDestroyChecked { registration } => {
            b.push(140);
            encode_u32(b, registration.0);
        }
        InstructionKind::ForeignFunctionRef { function } => {
            b.push(60);
            encode_u32(b, function.0);
        }
        InstructionKind::FunctionRef { function } => {
            b.push(25);
            encode_u32(b, function.0);
        }
        InstructionKind::CallIndirect { callee, arguments } => {
            b.push(26);
            encode_u32(b, callee.0);
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::ExtractIndex { aggregate, index } => {
            b.push(16);
            encode_u32(b, aggregate.0);
            encode_u32(b, index.0);
        }
        InstructionKind::UpdateIndex {
            aggregate,
            index,
            value,
        } => {
            b.push(17);
            encode_u32(b, aggregate.0);
            encode_u32(b, index.0);
            encode_u32(b, value.0);
        }
        InstructionKind::MakeAggregate { fields } => {
            b.push(13);
            encode_len(b, fields.len());
            for value in fields {
                encode_u32(b, value.0);
            }
        }
        InstructionKind::MakeVariant { variant, fields } => {
            b.push(18);
            encode_u32(b, *variant);
            encode_len(b, fields.len());
            for value in fields {
                encode_u32(b, value.0);
            }
        }
        InstructionKind::VariantTag { value } => {
            b.push(19);
            encode_u32(b, value.0);
        }
        InstructionKind::ReadVariant {
            value,
            variant,
            index,
        } => {
            b.push(20);
            encode_u32(b, value.0);
            encode_u32(b, *variant);
            encode_u32(b, *index);
        }
        InstructionKind::MakeOwnedTuple { fields } => {
            b.push(114);
            encode_len(b, fields.len());
            for field in fields {
                encode_u32(b, field.0);
            }
        }
        InstructionKind::UnpackTuple { local, fields } => {
            b.push(115);
            encode_u32(b, local.0);
            encode_len(b, fields.len());
            for field in fields {
                encode_u32(b, field.0);
            }
        }
        InstructionKind::DropTuple { local } => {
            b.push(116);
            encode_u32(b, local.0);
        }
        InstructionKind::TakeTupleField { local, path }
        | InstructionKind::TakeVariantField { local, path }
        | InstructionKind::BorrowPlaceLocal { local, path, .. }
        | InstructionKind::PlaceTagLocal { local, path } => {
            b.push(match i {
                InstructionKind::TakeTupleField { .. } => 117,
                InstructionKind::TakeVariantField { .. } => 118,
                InstructionKind::BorrowPlaceLocal { .. } => 119,
                _ => 120,
            });
            encode_u32(b, local.0);
            encode_len(b, path.len());
            for step in path {
                match step {
                    PlaceProjection::Field(index) => {
                        b.push(0);
                        encode_u32(b, *index);
                    }
                    PlaceProjection::Variant(part) => {
                        b.push(1);
                        encode_u32(b, part.variant);
                        encode_u32(b, part.index);
                    }
                    PlaceProjection::ErrorArm(is_error) => {
                        b.push(2);
                        b.push(u8::from(*is_error));
                    }
                }
            }
            if let InstructionKind::BorrowPlaceLocal { mutable, .. } = i {
                b.push(u8::from(*mutable));
            }
        }
        InstructionKind::BorrowPlaceAddress { address, path, .. }
        | InstructionKind::PlaceTagAddress { address, path } => {
            b.push(if matches!(i, InstructionKind::BorrowPlaceAddress { .. }) {
                126
            } else {
                127
            });
            encode_u32(b, address.0);
            encode_len(b, path.len());
            for step in path {
                match step {
                    PlaceProjection::Field(index) => {
                        b.push(0);
                        encode_u32(b, *index);
                    }
                    PlaceProjection::Variant(part) => {
                        b.push(1);
                        encode_u32(b, part.variant);
                        encode_u32(b, part.index);
                    }
                    PlaceProjection::ErrorArm(is_error) => {
                        b.push(2);
                        b.push(u8::from(*is_error));
                    }
                }
            }
            if let InstructionKind::BorrowPlaceAddress { mutable, .. } = i {
                b.push(u8::from(*mutable));
            }
        }
        InstructionKind::ReservePlaceLocal {
            reservation,
            local,
            path,
        } => {
            b.push(121);
            encode_u32(b, reservation.0);
            encode_u32(b, local.0);
            encode_len(b, path.len());
            for step in path {
                match step {
                    PlaceProjection::Field(index) => {
                        b.push(0);
                        encode_u32(b, *index);
                    }
                    PlaceProjection::Variant(part) => {
                        b.push(1);
                        encode_u32(b, part.variant);
                        encode_u32(b, part.index);
                    }
                    PlaceProjection::ErrorArm(is_error) => {
                        b.push(2);
                        b.push(u8::from(*is_error));
                    }
                }
            }
        }
        InstructionKind::ReservePlaceAddress {
            reservation,
            address,
            path,
        } => {
            b.push(122);
            encode_u32(b, reservation.0);
            encode_u32(b, address.0);
            encode_len(b, path.len());
            for step in path {
                match step {
                    PlaceProjection::Field(index) => {
                        b.push(0);
                        encode_u32(b, *index);
                    }
                    PlaceProjection::Variant(part) => {
                        b.push(1);
                        encode_u32(b, part.variant);
                        encode_u32(b, part.index);
                    }
                    PlaceProjection::ErrorArm(is_error) => {
                        b.push(2);
                        b.push(u8::from(*is_error));
                    }
                }
            }
        }
        InstructionKind::CommitPlaceReplace { reservation, value } => {
            b.push(123);
            encode_u32(b, reservation.0);
            encode_u32(b, value.0);
        }
        InstructionKind::AbortPlaceReservation { reservation } => {
            b.push(124);
            encode_u32(b, reservation.0);
        }
        InstructionKind::BorrowFieldLocal {
            local,
            path,
            mutable,
        } => {
            b.push(110);
            encode_u32(b, local.0);
            encode_len(b, path.len());
            for field in path {
                encode_u32(b, *field);
            }
            b.push(u8::from(*mutable));
        }
        InstructionKind::VariantTagLocal { local, path }
        | InstructionKind::ReadVariantLocal { local, path } => {
            b.push(if matches!(i, InstructionKind::VariantTagLocal { .. }) {
                90
            } else {
                91
            });
            encode_u32(b, local.0);
            encode_len(b, path.len());
            for step in path {
                encode_u32(b, step.variant);
                encode_u32(b, step.index);
            }
        }
        InstructionKind::BorrowVariantLocal {
            local,
            path,
            mutable,
        } => {
            b.push(106);
            encode_u32(b, local.0);
            encode_len(b, path.len());
            for step in path {
                encode_u32(b, step.variant);
                encode_u32(b, step.index);
            }
            b.push(u8::from(*mutable));
        }
        InstructionKind::VariantTagAddress { address, path }
        | InstructionKind::BorrowVariantAddress { address, path, .. } => {
            b.push(if matches!(i, InstructionKind::VariantTagAddress { .. }) {
                107
            } else {
                108
            });
            encode_u32(b, address.0);
            encode_len(b, path.len());
            for step in path {
                encode_u32(b, step.variant);
                encode_u32(b, step.index);
            }
            if let InstructionKind::BorrowVariantAddress { mutable, .. } = i {
                b.push(u8::from(*mutable));
            }
        }
        InstructionKind::UnpackVariant {
            local,
            variant,
            fields,
        } => {
            b.push(92);
            encode_u32(b, local.0);
            encode_u32(b, *variant);
            encode_len(b, fields.len());
            for field in fields {
                encode_u32(b, field.0);
            }
        }
        InstructionKind::DropEnum { local } => {
            b.push(93);
            encode_u32(b, local.0);
        }
        InstructionKind::ExtractField { aggregate, index } => {
            b.push(14);
            encode_u32(b, aggregate.0);
            encode_u32(b, *index);
        }
        InstructionKind::UpdateField {
            aggregate,
            index,
            value,
        } => {
            b.push(15);
            encode_u32(b, aggregate.0);
            encode_u32(b, *index);
            encode_u32(b, value.0);
        }
        InstructionKind::MakeClosure { closure, captures } => {
            b.push(6);
            encode_u32(b, closure.0);
            encode_len(b, captures.len());
            for capture in captures {
                encode_u32(b, capture.0);
            }
        }
        InstructionKind::ReadCapture { index } => {
            b.push(7);
            encode_u32(b, *index);
        }
        InstructionKind::WriteCapture { index, value } => {
            b.push(22);
            encode_u32(b, *index);
            encode_u32(b, value.0);
        }
        InstructionKind::CallClosureMut { callee, arguments } => {
            b.push(21);
            encode_u32(b, callee.0);
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::CallClosure { callee, arguments } => {
            b.push(8);
            encode_u32(b, callee.0);
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::BorrowLocal { local } => {
            b.push(9);
            encode_u32(b, local.0);
        }
        InstructionKind::BorrowLocalMut { local } => {
            b.push(27);
            encode_u32(b, local.0);
        }
        InstructionKind::ReborrowAddress { address, mutable } => {
            b.push(28);
            encode_u32(b, address.0);
            b.push(u8::from(*mutable));
        }
        InstructionKind::WriteAddress { address, value } => {
            b.push(29);
            encode_u32(b, address.0);
            encode_u32(b, value.0);
        }
        InstructionKind::TakeLocal { local } => {
            b.push(30);
            encode_u32(b, local.0);
        }
        InstructionKind::ReadAddress { address } => {
            b.push(10);
            encode_u32(b, address.0);
        }
        InstructionKind::AddressField { address, index } => {
            b.push(23);
            encode_u32(b, address.0);
            encode_u32(b, *index);
        }
        InstructionKind::AddressIndex { address, index } => {
            b.push(24);
            encode_u32(b, address.0);
            encode_u32(b, index.0);
        }
        InstructionKind::AddressSlice {
            address,
            start,
            end,
        } => {
            b.push(61);
            encode_u32(b, address.0);
            for endpoint in [start, end] {
                b.push(u8::from(endpoint.is_some()));
                if let Some(value) = endpoint {
                    encode_u32(b, value.0);
                }
            }
        }
        InstructionKind::SliceLength { address } => {
            b.push(62);
            encode_u32(b, address.0);
        }
        InstructionKind::ByteBufferNew => b.push(63),
        InstructionKind::OwnedStoreNew { element } => {
            b.push(80);
            encode_type(b, element);
        }
        InstructionKind::OwnedStoreReserve { store, capacity } => {
            b.push(81);
            encode_u32(b, store.0);
            encode_u32(b, capacity.0);
        }
        InstructionKind::OwnedStorePush { store, value } => {
            b.push(82);
            encode_u32(b, store.0);
            encode_u32(b, value.0);
        }
        InstructionKind::OwnedStorePop { store } => {
            b.push(83);
            encode_u32(b, store.0);
        }
        InstructionKind::OwnedStoreLen { store } => {
            b.push(84);
            encode_u32(b, store.0);
        }
        InstructionKind::OwnedStoreCapacity { store } => {
            b.push(85);
            encode_u32(b, store.0);
        }
        InstructionKind::OwnedStoreBorrow { store, index } => {
            b.push(86);
            encode_u32(b, store.0);
            encode_u32(b, index.0);
        }
        InstructionKind::OwnedStoreBorrowMut { store, index } => {
            b.push(88);
            encode_u32(b, store.0);
            encode_u32(b, index.0);
        }
        InstructionKind::ByteBufferReserve { buffer, capacity } => {
            b.push(64);
            encode_u32(b, buffer.0);
            encode_u32(b, capacity.0);
        }
        InstructionKind::ByteBufferPush { buffer, value } => {
            b.push(65);
            encode_u32(b, buffer.0);
            encode_u32(b, value.0);
        }
        InstructionKind::ByteBufferTruncate { buffer, length } => {
            b.push(69);
            encode_u32(b, buffer.0);
            encode_u32(b, length.0);
        }
        InstructionKind::ByteBufferLen { buffer } => {
            b.push(66);
            encode_u32(b, buffer.0);
        }
        InstructionKind::ByteBufferCapacity { buffer } => {
            b.push(67);
            encode_u32(b, buffer.0);
        }
        InstructionKind::ByteBufferBorrow { buffer, mutable } => {
            b.push(68);
            encode_u32(b, buffer.0);
            b.push(u8::from(*mutable));
        }
        InstructionKind::StorageLive(local) => {
            b.push(11);
            encode_u32(b, local.0);
        }
        InstructionKind::StorageDead(local) => {
            b.push(12);
            encode_u32(b, local.0);
        }
        InstructionKind::Unsupported(s) => {
            b.push(255);
            encode_text(b, s);
        }
    }
}

/// Deterministic binding for a raw callable relation, not a verification token.
pub fn callable_contract_fingerprint(contract: &CallableParameterContract) -> Fingerprint {
    let mut bytes = b"ZIL-CALLABLE-CONTRACT-6\0".to_vec();
    encode_callable_contract(&mut bytes, contract);
    Fingerprint::of(&bytes)
}
/// Deterministic identity only; this hash never grants verification authority.
pub fn callable_return_contract_fingerprint(contract: &CallableReturnContract) -> Fingerprint {
    let mut bytes = b"ZIL-CALLABLE-RETURN-CONTRACT-3\0".to_vec();
    encode_callable_return_contract(&mut bytes, contract);
    Fingerprint::of(&bytes)
}
pub fn protocol_contract_fingerprint(contract: &FunctionProtocolContract) -> Fingerprint {
    let mut bytes = b"ZIL-FUNCTION-PROTOCOL-1\0".to_vec();
    encode_protocol_contract(&mut bytes, contract);
    Fingerprint::of(&bytes)
}
fn encode_protocol_contract(b: &mut impl FingerprintSink, c: &FunctionProtocolContract) {
    b.push(match c.protocol {
        ProtocolKind::AsyncRegistrationV1 => 0,
    });
    encode_u32(b, c.parameter);
    encode_len(b, c.entry.len());
    for state in &c.entry {
        b.push(protocol_state_tag(*state));
    }
    encode_len(b, c.exits.len());
    for row in &c.exits {
        b.push(protocol_state_tag(row.input));
        b.push(protocol_exit_tag(row.exit));
        encode_len(b, row.output.len());
        for state in &row.output {
            b.push(protocol_state_tag(*state));
        }
    }
}
fn protocol_state_tag(s: ProtocolState) -> u8 {
    match s {
        ProtocolState::Open => 0,
        ProtocolState::Closing => 1,
        ProtocolState::Closed => 2,
    }
}
fn protocol_exit_tag(s: ProtocolExit) -> u8 {
    match s {
        ProtocolExit::Return => 0,
        ProtocolExit::ErrorOk => 1,
        ProtocolExit::ErrorErr => 2,
        ProtocolExit::PollPending => 3,
        ProtocolExit::PollReady => 4,
    }
}
fn encode_callable_return_contract(b: &mut impl FingerprintSink, c: &CallableReturnContract) {
    match c.source {
        CallableReturnSource::Fresh => b.push(0),
        CallableReturnSource::Parameter(parameter) => {
            b.push(1);
            encode_u32(b, parameter);
        }
    }
    encode_u32(b, c.closure.0);
    b.push(callable_kind_tag(c.kind));
    for indices in [&c.hold, &c.call, &c.writes, &c.cleanup, &c.lending_captures] {
        encode_len(b, indices.len());
        for index in indices {
            encode_u32(b, *index);
        }
    }
}
fn callable_kind_tag(kind: CallableKind) -> u8 {
    match kind {
        CallableKind::FunctionPointer => 0,
        CallableKind::Fn => 1,
        CallableKind::FnMut => 2,
        CallableKind::FnOnce => 3,
    }
}
fn encode_callable_contract(b: &mut impl FingerprintSink, c: &CallableParameterContract) {
    encode_len(b, c.roles.len());
    for role in &c.roles {
        encode_u32(b, role.parameter);
        encode_u32(b, role.closure.0);
        b.push(callable_kind_tag(role.kind));
        b.push(u8::from(role.life.is_some()));
        if let Some(life) = &role.life {
            encode_required_storage_contract(b, life);
        }
        for indices in [&role.hold, &role.call, &role.writes] {
            encode_len(b, indices.len());
            for index in indices {
                encode_u32(b, *index);
            }
        }
        b.push(match role.cleanup {
            CallableCleanup::Owned => 0,
            CallableCleanup::Borrowed => 1,
            CallableCleanup::Returned => 2,
        });
    }
    match &c.traces {
        CallableTraces::Finite(traces) => {
            b.push(0);
            encode_len(b, traces.len());
            for trace in traces {
                encode_callable_calls(b, &trace.calls);
                encode_callable_exit(b, trace.exit);
            }
        }
        CallableTraces::Flow(flow) => {
            b.push(1);
            encode_u32(b, flow.entry);
            encode_len(b, flow.nodes.len());
            for node in &flow.nodes {
                encode_callable_calls(b, &node.calls);
                match &node.next {
                    CallableFlowNext::Edges(edges) => {
                        b.push(0);
                        encode_len(b, edges.len());
                        for edge in edges {
                            encode_u32(b, *edge);
                        }
                    }
                    CallableFlowNext::Exit(exit) => {
                        b.push(1);
                        encode_callable_exit(b, *exit);
                    }
                }
            }
        }
    }
}
fn encode_callable_calls(b: &mut impl FingerprintSink, calls: &[CallEvent]) {
    encode_len(b, calls.len());
    for call in calls {
        encode_u32(b, call.parameter);
        b.push(callable_kind_tag(call.kind));
    }
}
fn encode_callable_exit(b: &mut impl FingerprintSink, exit: CallableExit) {
    b.push(match exit {
        CallableExit::Return => 0,
        CallableExit::Trap => 1,
    });
}
