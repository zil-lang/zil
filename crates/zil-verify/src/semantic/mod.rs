//! Independent interpretation of the untrusted lexical ghost program.
//! The outer Some gate remains closed until all actual-site/class-flow and
//! external witness obligations are discharged. A checked catalogue alone is
//! never an executable module or a permission.
use crate::*;
pub(crate) mod api;
pub(crate) use api::*;
pub(crate) mod scalar;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
mod evaluate;
mod lifetimes;
mod relations;
mod type_storage;
pub(crate) use type_storage::closed_storage;
pub(crate) use type_storage::PhysicalConstructor;
mod constants;
mod dyn_types;
mod nominal;
mod nominal_origins;
mod requirements;
pub(crate) use nominal_origins::charge_origin as charge_intrinsic_origin;
mod call_results;
pub(crate) mod callable_obligations;
mod callbacks;
mod capture_heads;
mod catalogue;
mod closure_boundaries;
mod closures;
pub(crate) mod completion;
pub(crate) mod constructor_classes;
mod flow;
mod frozen;
mod instances;
pub(crate) mod local_roles;
mod method_obligations;
mod opaque;
mod original;
mod receiver_heads;
mod resources;
mod retained;
mod sites;
mod static_members;
pub(crate) mod task_domains;
pub(crate) use flow::check_program;
pub(super) use frozen::FrozenContext;

#[derive(Clone, Default)]
struct Premises {
    // Appending preserves the existing journal prefix. Retraction replaces
    // this identity, including when its old length is subsequently restored.
    prefix: std::rc::Rc<()>,
    relations: Relations,
    external: Vec<ExternalRequest>,
}

fn bad(message: &str) -> Diagnostic {
    reject("E-VERIFY-SEMANTIC", None, message)
}

/// Indices into the same immutable candidate. No mirrored raw table is a
/// source of builtin, visibility, ownership or runtime borrow authority.
pub(crate) struct CheckedContext {
    pub(super) domain_only: Cell<bool>,
    definitions: BTreeMap<SemanticDefinitionId, usize>,
    instances: BTreeMap<SemanticInstanceId, usize>,
    site_index: sites::Index,
    rigid_worlds: RefCell<flow::RigidWorlds>,
    premises: RefCell<BTreeMap<ScopeKey, Premises>>,
    premises_epoch: Cell<u64>,
    static_identity: std::rc::Rc<()>,
    static_members: RefCell<Vec<static_members::StaticMember>>,
    local_role_writes: RefCell<BTreeMap<(SemanticInstanceId, LocalId), local_roles::Writes>>,
    local_role_transfers: RefCell<BTreeMap<FunctionId, Vec<local_roles::Transfer>>>,
    closed_nominals: RefCell<nominal::closed::ClosedNominals>,
    empty_domains: RefCell<type_storage::EmptyDomains>,
    closed_frames:
        RefCell<BTreeMap<(usize, SemanticInstanceId, bool, bool), instances::ClosedFrames>>,
    occurrences: RefCell<BTreeMap<(ScopeKey, OperandKey), CheckedType>>,
    seals: RefCell<BTreeMap<SemanticInstanceId, Vec<CheckedActual>>>,
    pending_closures: RefCell<Vec<ClosureCursor>>,
    published: Cell<bool>,
    origin_aliases: RefCell<BTreeMap<IntrinsicOrigin, IntrinsicOrigin>>,
    original: RefCell<Option<original::CheckedOriginal>>,
    closure_boundaries: RefCell<Vec<ClosureBoundary>>,
    frozen_functions: BTreeMap<SemanticInstanceId, frozen::FrozenFunction>,
    shared_types: RefCell<BTreeMap<usize, (std::rc::Weak<CheckedTypeData>, (usize, usize))>>,
    constructed_types:
        RefCell<BTreeMap<type_storage::ConstructionKey, type_storage::ConstructedType>>,
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

pub(crate) struct ClosureCursor {
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
    pub(crate) fn depth(&self) -> usize {
        self.frames.len()
    }
    pub(crate) fn new() -> Self {
        Self {
            frames: Vec::new(),
            next_token: 0,
        }
    }
    // A leaf reads a sealed operand or forms an atom, so it cannot reenter
    // another query. It still occupies one level in the current session.
    fn check_leaf(&self, b: &mut Budget) -> Check<()> {
        b.spend(1)?;
        if self.frames.len() >= b.limits.max_call_depth.min(128) {
            return Err(limit("semantic/witness dependency depth exceeded"));
        }
        Ok(())
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
    pub(crate) fn static_identity(&self) -> std::rc::Rc<()> {
        self.static_identity.clone()
    }
    pub(crate) fn same_static_identity(&self, identity: &std::rc::Rc<()>) -> bool {
        std::rc::Rc::ptr_eq(&self.static_identity, identity)
    }
    /// A reentrant source-method proof must continue cold.
    pub(crate) fn completed_original_epoch(&self) -> Option<u64> {
        if self.domain_only.get() {
            return None;
        }
        self.original
            .try_borrow()
            .ok()
            .and_then(|original| original.as_ref().map(|_| self.premises_epoch.get()))
    }
    fn invalidate_closed_frames(&self) {
        if let Some(next) = self.premises_epoch.get().checked_add(1) {
            self.premises_epoch.set(next);
        } else {
            self.closed_frames.borrow_mut().clear();
            self.premises_epoch.set(0);
        }
    }

    /// Share only an exact live allocation, or an exact registered immutable
    /// module node. A fresh temporary Type is always copied and paid cold.
    pub(crate) fn shared_type_cell(
        &self,
        ty: &Type,
        b: &mut Budget,
    ) -> Check<std::rc::Rc<CheckedTypeData>> {
        let ptr = ty as *const Type as usize;
        let profile = (b.limits.max_nodes, b.limits.max_call_depth.min(128));
        let fee = self
            .shared_types
            .borrow()
            .len()
            .checked_ilog2()
            .map_or(1, |n| n as usize + 2);
        b.spend(fee)?;
        let known = self
            .shared_types
            .borrow()
            .get(&ptr)
            .and_then(|(cell, checked)| cell.upgrade().map(|cell| (cell, *checked)));
        if let Some((cell, checked)) = known {
            if checked != profile {
                dynamic_metadata::bounds_type(ty, b)?;
            }
            return Ok(cell);
        }
        let cell = dynamic_metadata::validated_type_cell(ty, b)?;
        b.spend(fee.saturating_add(2))?;
        self.shared_types.borrow_mut().insert(
            &cell.ty as *const Type as usize,
            (std::rc::Rc::downgrade(&cell), profile),
        );
        Ok(cell)
    }

    pub(crate) fn register_closure_cursor(
        &self,
        module: &TypedModule,
        instance: SemanticInstanceId,
        environment: &EvalEnv,
        arguments: &[CheckedActual],
        sealed: &[CheckedActual],
        expected: &CheckedType,
        b: &mut Budget,
    ) -> Check<()> {
        expected.charge_clone(b)?;
        self.charge_cursor_copies(environment, arguments, sealed, b)?;
        b.spend(
            arguments
                .len()
                .saturating_add(sealed.len())
                .saturating_add(expected.classes.len())
                .saturating_add(environment.lexical.len()),
        )?;
        let cursor = ClosureCursor {
            instance,
            environment: environment.clone(),
            arguments: arguments.to_vec(),
            sealed: sealed.to_vec(),
            expected: expected.clone(),
        };
        if self.published.get() {
            let session = b.semantic_checked.clone().ok_or_else(|| {
                bad("published closure query lacks its completed exact-module session")
            })?;
            session.finish_late_closure_cursor(module, self, &cursor, b)?;
        }
        self.pending_closures.borrow_mut().push(cursor);
        Ok(())
    }
    pub(crate) fn finish_late_closure_cursor(
        &self,
        module: &TypedModule,
        cursor: &ClosureCursor,
        worlds: &[(SemanticInstanceId, EvalEnv)],
        b: &mut Budget,
    ) -> Check<()> {
        if !self.published.get() {
            return Err(bad(
                "late closure query uses an unpublished original context",
            ));
        }
        let instance = self.instance(module, cursor.instance, b)?;
        let definition = self.definition(module, instance.definition, b)?;
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) =
            instance.target
        else {
            return Err(bad("late closure cursor selects a nonexecutable scope"));
        };
        let entry = module
            .functions
            .get(function.0 as usize)
            .filter(|entry| entry.id == function)
            .ok_or_else(|| bad("late closure cursor entry is absent"))?;
        let closure = entry
            .environment
            .ok_or_else(|| bad("late closure cursor omits its environment identity"))?;
        let relation = entry
            .storage_life
            .as_ref()
            .ok_or_else(|| bad("late closure cursor full header is Unknown"))?;
        if !self.seals.borrow().contains_key(&instance.id)
            || cursor.expected.ty != Type::Closure(closure)
            || cursor.expected.life.binder_count != relation.fixed_binder_count
            || cursor.expected.classes.len() != relation.fixed_binder_count as usize
            || cursor.arguments.len() != definition.parameters.len()
            || cursor.sealed.len() != definition.environment.len()
        {
            return Err(bad(
                "late closure cursor contradicts its completed entry/seal/domain",
            ));
        }
        b.spend(worlds.len())?;
        let actual = worlds
            .iter()
            .find(|(id, _)| *id == instance.id)
            .ok_or_else(|| {
                bad("late closure cursor has no independently completed original world")
            })?;
        self.finish_closure_cursor(module, cursor, &actual.1, b)
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

pub(crate) fn empty_life() -> StorageLifeShape {
    StorageLifeShape {
        binder_count: 0,
        slots: vec![],
        retained: vec![],
        nominal_arguments: vec![],
    }
}

#[cfg(test)]
mod tests;
