//! Independent interpretation of the untrusted lexical ghost program.
//! The outer Some gate remains closed until all actual-site/class-flow and
//! external witness obligations are discharged. A checked catalogue alone is
//! never an executable module or a permission.
use crate::*;
pub(crate) mod api;
pub(crate) use api::*;
pub(crate) mod scalar;
use std::collections::BTreeMap;
use std::cell::{RefCell,Cell};
mod lifetimes;
mod relations;
mod evaluate;
mod type_storage;
pub(crate) use type_storage::PhysicalConstructor;
mod dyn_types;
mod requirements;
mod constants;
mod nominal;
mod nominal_origins;
pub(crate) use nominal_origins::charge_origin as charge_intrinsic_origin;
mod closures;
mod opaque;
mod method_obligations;
pub(crate) mod callable_obligations;
pub(crate) mod task_domains;
mod catalogue;
mod instances;
mod flow;
mod capture_heads;
mod closure_boundaries;
mod call_results;
mod callbacks;
pub(crate) mod constructor_classes;
mod original;
mod resources;
mod retained;
mod frozen;
pub(super) use frozen::FrozenContext;
pub(crate) use flow::check_program;

#[derive(Clone, Default)]
struct Premises {
    relations: Relations,
    external: Vec<ExternalRequest>,
}

fn bad(message: &str) -> Diagnostic {
    reject("E-VERIFY-SEMANTIC", None, message)
}

/// Indices into the same immutable candidate. No mirrored raw table is a
/// source of builtin, visibility, ownership or runtime borrow authority.
pub(crate) struct CheckedContext {
    pub(super) domain_only:Cell<bool>,
    definitions: BTreeMap<SemanticDefinitionId, usize>,
    instances: BTreeMap<SemanticInstanceId, usize>,
    premises: RefCell<BTreeMap<ScopeKey, Premises>>,
    premises_epoch:Cell<u64>,
    closed_frames:RefCell<BTreeMap<(usize,SemanticInstanceId,u64,bool,bool),instances::ClosedFrames>>,
    occurrences: RefCell<BTreeMap<(ScopeKey, OperandKey), CheckedType>>,
    seals: RefCell<BTreeMap<SemanticInstanceId, Vec<CheckedActual>>>,
    pending_closures: RefCell<Vec<ClosureCursor>>,
    origin_aliases: RefCell<BTreeMap<IntrinsicOrigin, IntrinsicOrigin>>,
    original: RefCell<Option<original::CheckedOriginal>>,
    closure_boundaries: RefCell<Vec<ClosureBoundary>>,
    frozen_functions: BTreeMap<SemanticInstanceId,frozen::FrozenFunction>,
    shared_types: RefCell<BTreeMap<usize,(std::rc::Weak<CheckedTypeData>,(usize,usize))>>,
    constructed_types:RefCell<BTreeMap<type_storage::ConstructionKey,type_storage::ConstructedType>>,
}

/// A provisional constructor fact. It becomes trusted only when every actual
/// child entry and the complete module's remaining obligations have passed.
struct ClosureBoundary {
    creator: ScopeKey,
    node: zil_base::NodeId,
    at: SitePoint,
    child: SemanticInstanceId,
    value: CheckedType,
}

struct ClosureCursor {
    instance: SemanticInstanceId,
    environment: EvalEnv,
    arguments: Vec<CheckedActual>,
    sealed: Vec<CheckedActual>,
    expected: CheckedType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueryKind {
    Type,
    Const,
    Requirement,
    Instance,
    Signature,
    External,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DependencyKey {
    pub(crate) kind: QueryKind,
    /// A client owns/interns each complete query key using full equality;
    /// this token is private session identity, never an input certificate.
    pub(crate) token: FrameToken,
}
/// One active session crosses the Semantic/FullW boundary. Neither side may
/// reset it or the request's remaining Budget during a recursive callback.
pub(crate) struct ActiveQueries {
    frames: Vec<DependencyKey>,
    next_token: u64,
}
impl ActiveQueries {
    pub(crate) fn new() -> Self {
        Self {
            frames: Vec::new(),
            next_token: 0,
        }
    }
    pub(crate) fn fresh(&mut self, b: &mut Budget) -> Check<FrameToken> {
        b.spend(1)?;
        let token = self.next_token;
        self.next_token = token
            .checked_add(1)
            .ok_or_else(|| limit("semantic frame identity exhausted"))?;
        Ok(FrameToken(token))
    }
    pub(crate) fn with_query<T>(
        &mut self,
        key: DependencyKey,
        b: &mut Budget,
        run: impl FnOnce(&mut Self, &mut Budget) -> Check<T>,
    ) -> Check<T> {
        b.spend(self.frames.len().saturating_add(2))?;
        if self.frames.len() >= b.limits.max_call_depth.min(128) {
            return Err(limit("semantic/witness dependency depth exceeded"));
        }
        if self.frames.contains(&key) {
            return Err(bad("cyclic semantic/witness dependency"));
        }
        self.frames.push(key);
        let result = run(self, b);
        self.frames.pop();
        result
    }
}

impl CheckedContext {
    fn invalidate_closed_frames(&self){
        if let Some(next)=self.premises_epoch.get().checked_add(1){self.premises_epoch.set(next);}else{self.closed_frames.borrow_mut().clear();self.premises_epoch.set(0);}
    }

    /// Share only an exact live allocation, or an exact registered immutable
    /// module node. A fresh temporary Type is always copied and paid cold.
    pub(super) fn shared_type_cell(&self,ty:&Type,b:&mut Budget)->Check<std::rc::Rc<CheckedTypeData>>{
        let ptr=ty as *const Type as usize;
        let profile=(b.limits.max_nodes,b.limits.max_call_depth.min(128));
        let fee=self.shared_types.borrow().len().checked_ilog2().map_or(1,|n|n as usize+2);
        b.spend(fee)?;
        let known=self.shared_types.borrow().get(&ptr).and_then(|(cell,checked)|cell.upgrade().map(|cell|(cell,*checked)));
        if let Some((cell,checked))=known{
            if checked!=profile{dynamic_metadata::bounds_type(ty,b)?;}
            return Ok(cell);
        }
        dynamic_metadata::bounds_type(ty,b)?;
        let cell=if let Some(cell)=dynamic_metadata::shared_type(ty,b)?{cell}else{
            dynamic_metadata::ty(ty,b)?;
            CheckedTypeData::new(ty.clone())
        };
        b.spend(fee.saturating_add(2))?;
        self.shared_types.borrow_mut().insert(&cell.ty as *const Type as usize,(std::rc::Rc::downgrade(&cell),profile));
        Ok(cell)
    }

    pub(crate) fn register_closure_cursor(&self,instance:SemanticInstanceId,environment:&EvalEnv,arguments:&[CheckedActual],sealed:&[CheckedActual],expected:&CheckedType,b:&mut Budget)->Check<()> {
        expected.charge_clone(b)?;
        b.spend(arguments.len().saturating_add(sealed.len()).saturating_add(expected.classes.len()).saturating_add(environment.lexical.len()))?;
        self.pending_closures.borrow_mut().push(ClosureCursor{instance,environment:environment.clone(),arguments:arguments.to_vec(),sealed:sealed.to_vec(),expected:expected.clone()});
        Ok(())
    }
    pub(crate) fn definition<'m>(
        &self,
        module: &'m TypedModule,
        id: SemanticDefinitionId,
        b: &mut Budget,
    ) -> Check<&'m SemanticDefinition> {
        b.spend(
            self.definitions
                .len()
                .checked_ilog2()
                .map_or(1, |n| n as usize + 2),
        )?;
        let row = self
            .definitions
            .get(&id)
            .ok_or_else(|| bad("semantic definition is absent"))?;
        module
            .semantic
            .as_ref()
            .and_then(|program| program.definitions.get(*row))
            .ok_or_else(|| bad("semantic definition index has no actual candidate row"))
    }
    pub(crate) fn instance<'m>(
        &self,
        module: &'m TypedModule,
        id: SemanticInstanceId,
        b: &mut Budget,
    ) -> Check<&'m SemanticInstance> {
        b.spend(
            self.instances
                .len()
                .checked_ilog2()
                .map_or(1, |n| n as usize + 2),
        )?;
        let row = self
            .instances
            .get(&id)
            .ok_or_else(|| bad("semantic instance is absent"))?;
        module
            .semantic
            .as_ref()
            .and_then(|program| program.instances.get(*row))
            .ok_or_else(|| bad("semantic instance index has no actual candidate row"))
    }


}

/// Centralized Typed boundary. Static outer occurrences come from the real
/// original recipe and later actual-source proof, never an empty class vector.
pub(crate) fn resolved(
    value: &SemanticTypeValue,
) -> Check<(&Type, &StorageLifeShape, &[SemanticClassExpr])> {
    match value {
        SemanticTypeValue::Resolved { ty, life, classes } => Ok((ty, life, classes)),
        SemanticTypeValue::Core { .. } => {
            Err(bad("Typed semantic program retains a Core type operand"))
        }
    }
}

fn empty_life() -> StorageLifeShape {
    StorageLifeShape { binder_count:0, slots:vec![], retained:vec![], nominal_arguments:vec![] }
}

#[cfg(test)]
mod tests;
