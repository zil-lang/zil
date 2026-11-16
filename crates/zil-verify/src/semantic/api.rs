//! Private interfaces between independent lexical semantics and FullW.
//! These records are proof requests and scoped terms, never IR certificates.
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FrameToken(pub(crate) u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ScopeKey {
    Instance(SemanticInstanceId),
    Witness {
        definition: SemanticDefinitionId,
        frame: FrameToken,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SitePoint {
    pub(crate) function: FunctionId,
    pub(crate) block: BlockId,
    pub(crate) instruction: u32,
    pub(crate) after: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum OperandKey {
    Parameter(u32),
    Capture(u32),
    Receiver,
    Value(ValueId),
    Local { local: LocalId, at: SitePoint },
    TypeArgument { scope: ScopeKey, ordinal: u32 },
    Environment { scope: ScopeKey, ordinal: u32 },
}
/// Identity is not apartness. Original where premises can relate distinct
/// terms; an actual caller must prove its obligations without adding such
/// premises merely to make the call type-check.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ClassKey {
    Formal {
        scope: ScopeKey,
        ordinal: u32,
    },
    Implicit {
        scope: ScopeKey,
        ordinal: u32,
    },
    Bound {
        frame: FrameToken,
        index: u32,
    },
    Invocation {
        frame: FrameToken,
        index: u32,
    },
    Outer {
        scope: ScopeKey,
        operand: OperandKey,
        path: Vec<LifeTypeProjection>,
        mode: StoredBorrowMode,
    },
    Intrinsic {
        scope: ScopeKey,
        operand: OperandKey,
        path: Vec<LifeTypeProjection>,
        class: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum IntrinsicOrigin {
    Existing {
        scope: ScopeKey,
        operand: OperandKey,
        path: Vec<LifeTypeProjection>,
        class: u32,
    },
    /// A freshly built nominal/structural class derives from a real scoped
    /// semantic lifetime, not a fictional TypeArgument(LifeOrdinal).
    DerivedClass { source: ClassKey },
    /// One complete nominal intrinsic domain. Its outside Life actuals are
    /// arguments of this type identity, not identities of its intrinsic κ.
    Nominal { source: std::rc::Rc<NominalOrigin>, class: u32 },
}
#[derive(Clone, Debug)]
pub(crate) struct NominalOrigin {
    pub(crate) declaration: LifeDeclarationId,
    pub(crate) actuals: Vec<CheckedActual>,
    pub(crate) environment: Vec<CheckedActual>,
    pub(crate) domain: StorageLifeShape,
    pub(crate) class_sources: Vec<IntrinsicOrigin>,
}
impl PartialEq for NominalOrigin{fn eq(&self,other:&Self)->bool{self.declaration==other.declaration&&self.actuals==other.actuals&&self.environment==other.environment&&self.domain==other.domain}}
impl Eq for NominalOrigin{}
impl PartialOrd for NominalOrigin{fn partial_cmp(&self,other:&Self)->Option<std::cmp::Ordering>{Some(self.cmp(other))}}
impl Ord for NominalOrigin{fn cmp(&self,other:&Self)->std::cmp::Ordering{(&self.declaration,&self.actuals,&self.environment,&self.domain).cmp(&(&other.declaration,&other.actuals,&other.environment,&other.domain))}}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum StaticOccurrence {
    Direct {
        /// Type metadata only, never a physical GEP.
        path: Vec<LifeTypeProjection>,
        leaf: Type,
        mode: StoredBorrowMode,
    },
    Logical {
        /// The actual opaque definition owns this pending invariant. Capture
        /// and RetainedPort paths remain logical selectors, not type/GEP paths.
        closure: ClosureId,
        position: StoragePosition,
        leaf: Type,
        mode: StoredBorrowMode,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CheckedTypeData {
    pub(crate) ty: Type,
}
impl CheckedTypeData {
    pub(crate) fn new(ty: Type) -> std::rc::Rc<Self> {std::rc::Rc::new(Self{ty})}
}
/// Only immutable physical type structure is shared. Every value continues to
/// own its current class image, intrinsic origins and pending source directory.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CheckedType {
    pub(crate) type_cell: std::rc::Rc<CheckedTypeData>,
    pub(crate) life: StorageLifeShape,
    pub(crate) classes: Vec<ClassKey>,
    /// Intrinsic domains survive many-to-one external images unchanged.
    pub(crate) origins: Vec<IntrinsicOrigin>,
    /// Pending actual-source obligations, not literal or Known0 evidence.
    pub(crate) statics: Vec<StaticOccurrence>,
}
impl std::ops::Deref for CheckedType {
    type Target=CheckedTypeData;
    fn deref(&self)->&Self::Target{&self.type_cell}
}
impl CheckedType {
    pub(crate) fn same_raw_type(&self,other:&Self,b:&mut crate::Budget)->crate::Check<bool>{
        b.spend(1)?;
        if std::rc::Rc::ptr_eq(&self.type_cell,&other.type_cell){return Ok(true);}
        crate::dynamic_metadata::ty(&self.ty,b)?;
        crate::dynamic_metadata::ty(&other.ty,b)?;
        Ok(self.ty==other.ty)
    }
    pub(crate) fn charge_clone(&self,b:&mut crate::Budget)->crate::Check<()>{
        b.spend(1)?;
        crate::dynamic_metadata::life_shape(&self.life,b)?;
        b.spend(self.classes.len().saturating_add(self.origins.len()).saturating_add(self.statics.len()))?;
        for key in &self.classes{if let ClassKey::Outer{path,..}|ClassKey::Intrinsic{path,..}=key{b.spend(path.len())?;}}
        for origin in &self.origins{
            // NominalOrigin is already an immutable Rc. A value clone copies
            // that reference, not its actual Type trees. Equality/normalization
            // still use the complete independent origin traversal.
            b.spend(1)?;
            match origin{
                IntrinsicOrigin::Existing{path,..}=>b.spend(path.len())?,
                IntrinsicOrigin::DerivedClass{source:ClassKey::Outer{path,..}|ClassKey::Intrinsic{path,..}}=>b.spend(path.len())?,
                _=>{},
            }
        }
        for row in &self.statics{match row{
            StaticOccurrence::Direct{path,leaf,..}=>{b.spend(path.len())?;crate::dynamic_metadata::ty(leaf,b)?;},
            StaticOccurrence::Logical{position,leaf,..}=>{crate::dynamic_metadata::position(position,b)?;crate::dynamic_metadata::ty(leaf,b)?;},
        }}Ok(())
    }
    pub(crate) fn replace_type(&mut self,ty:Type){self.type_cell=CheckedTypeData::new(ty);}
    pub(crate) fn raw_type_copy(&self,b:&mut crate::Budget)->crate::Check<Type>{
        crate::dynamic_metadata::ty(&self.ty,b)?;Ok(self.ty.clone())
    }
    pub(crate) fn into_raw_type(self,b:&mut crate::Budget)->crate::Check<Type>{
        b.spend(1)?;
        match std::rc::Rc::try_unwrap(self.type_cell){
            Ok(value)=>Ok(value.ty),
            Err(shared)=>{crate::dynamic_metadata::ty(&shared.ty,b)?;Ok(shared.ty.clone())},
        }
    }
}
#[cfg(test)]
mod shared_type_tests {
    use super::*;
    fn value()->CheckedType{
        CheckedType{type_cell:CheckedTypeData::new(Type::Record{name:"shared_immutable_payload_".repeat(128),fields:vec![("whole".repeat(32),Type::I32)]}),life:StorageLifeShape{binder_count:0,slots:vec![],retained:vec![],nominal_arguments:vec![]},classes:vec![],origins:vec![],statics:vec![]}
    }
    #[test]
    fn shared_storage_keeps_temporary_nodes_cold_and_rechecks_changed_bounds(){
        let limits=Limits::default();
        let(mut module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        module.functions[0].values.push(value().into_raw_type(&mut crate::Budget::new(&limits)).unwrap());
        let raw=module.functions[0].values.last().unwrap();
        let mut b=crate::Budget::new(&limits);
        b.structural=Some(crate::dynamic_metadata::Structure::register(&module,&mut b).unwrap());
        let context=super::super::CheckedContext::catalogue(&module,&mut b).unwrap();
        let first=context.shared_type_cell(raw,&mut b).unwrap();
        let second=context.shared_type_cell(raw,&mut b).unwrap();
        let owned=context.shared_type_cell(&first.ty,&mut b).unwrap();
        assert!(std::rc::Rc::ptr_eq(&first,&second));assert!(std::rc::Rc::ptr_eq(&first,&owned));
        let temporary=raw.clone();
        let a=context.shared_type_cell(&temporary,&mut b).unwrap();
        let z=context.shared_type_cell(&temporary,&mut b).unwrap();
        assert!(!std::rc::Rc::ptr_eq(&a,&z),"equal temporary shapes are not allocation identities");
        b.limits.max_nodes=1;
        assert!(context.shared_type_cell(&first.ty,&mut b).is_err());
        assert!(context.shared_type_cell(raw,&mut b).is_err());
        b.limits.max_nodes=limits.max_nodes;
        assert!(std::rc::Rc::ptr_eq(&first,&context.shared_type_cell(&first.ty,&mut b).unwrap()));
    }
    #[test]
    fn checked_value_shares_only_type_storage_and_raw_export_pays_real_copy(){
        let original=value();let mut copied=original.clone();
        assert!(std::rc::Rc::ptr_eq(&original.type_cell,&copied.type_cell));
        copied.life.binder_count=1;copied.classes.push(ClassKey::Formal{scope:ScopeKey::Instance(SemanticInstanceId(0)),ordinal:0});
        assert_eq!(original.life.binder_count,0);assert!(original.classes.is_empty());
        let mut b=crate::Budget::new(&Limits::default());
        assert!(original.same_raw_type(&copied,&mut b).unwrap());
        assert_ne!(original,copied,"sharing type bytes does not identify current class domains");
        let before=b.remaining;original.charge_clone(&mut b).unwrap();let reference=before-b.remaining;
        let before=b.remaining;let raw=original.clone().into_raw_type(&mut b).unwrap();let deep=before-b.remaining;
        assert_eq!(raw,original.ty);assert!(deep>reference);
        let unique=value();let before=b.remaining;unique.into_raw_type(&mut b).unwrap();assert_eq!(before-b.remaining,1);
        copied.replace_type(Type::Bool);assert!(!original.same_raw_type(&copied,&mut b).unwrap());
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CheckedActual {
    Type(CheckedType),
    Const {
        carrier: CheckedType,
        value: Constant,
    },
    Life(ClassKey),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LexicalFrame {
    pub(crate) definition: SemanticDefinitionId,
    pub(crate) key: ScopeKey,
    pub(crate) actuals: Vec<CheckedActual>,
    pub(crate) environment: Vec<CheckedActual>,
    pub(crate) self_type: Option<CheckedType>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BoundFrame {
    pub(crate) token: FrameToken,
    pub(crate) classes: Vec<ClassKey>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EvalEnv {
    /// Current exact lexical scope first, followed by its real parents.
    pub(crate) lexical: Vec<LexicalFrame>,
    /// Already checked caller Γ; evaluation substitution does not grant the
    /// target declaration's own requirements as assumptions.
    pub(crate) assumptions: Vec<ScopeKey>,
    /// Scoped boundary substitution, never global equality or a new premise.
    pub(crate) images: Vec<(ClassKey, ClassKey)>,
    /// Nearest higher-ranked binder first; never reset across a callback.
    pub(crate) bounds: Vec<BoundFrame>,
    pub(crate) invocations: Vec<ClassKey>,
    pub(crate) at: Option<SitePoint>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CheckedApplication {
    pub(crate) declaration: LifeDeclarationId,
    pub(crate) arguments: Vec<CheckedActual>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CheckedSignature {
    pub(crate) parameters: Vec<(CheckedType, PassingMode)>,
    pub(crate) result: CheckedType,
    pub(crate) result_mode: PassingMode,
    pub(crate) call_kind: CallKind,
    pub(crate) bound: Vec<ClassKey>,
    pub(crate) invocation: Vec<ClassKey>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RequirementMode {
    DeclarationPremise,
    ActualObligation,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExternalRequest {
    Builtin { receiver: CheckedType, trait_kind: SemanticBuiltinTrait },
    Trait {
        receiver: CheckedType,
        application: CheckedApplication,
    },
    AssociatedType {
        receiver: CheckedType,
        application: CheckedApplication,
        member: u32,
    },
    AssociatedConst {
        receiver: CheckedType,
        application: CheckedApplication,
        member: u32,
    },
    Callable {
        receiver: CheckedType,
        kind: CallableKind,
        signature: CheckedSignature,
    },
    Structural {
        receiver: CheckedType,
        bound: DynStructuralBound,
    },
    Dyn {
        owned: bool,
        application: CheckedApplication,
        context: Option<CheckedType>,
        associated: Vec<(CheckedApplication, u32, CheckedActual)>,
    },
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Relations {
    pub(crate) equalities: Vec<(ClassKey, ClassKey)>,
    pub(crate) outlives: Vec<(ClassKey, ClassKey)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExternalValue {
    Type(CheckedType),
    Const {
        carrier: CheckedType,
        value: Constant,
    },
    // A role-specific trait/Callable satisfaction never exports global class
    // equalities or Outlives facts into the caller's semantic congruence.
    Proved,
}
/// A FullW callback receives the same context, environment, active dependency
/// session and remaining Budget. It cannot treat scope WF as a trait witness.
pub(crate) trait ExternalProofs {
    fn query(
        &mut self,
        module: &TypedModule,
        context: &super::CheckedContext,
        environment: &EvalEnv,
        request: &ExternalRequest,
        mode: RequirementMode,
        active: &mut super::ActiveQueries,
        budget: &mut Budget,
    ) -> Check<ExternalValue>;
}
