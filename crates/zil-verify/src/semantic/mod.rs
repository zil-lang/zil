//! Independent interpretation of the untrusted lexical ghost program.
//! The outer Some gate remains closed until all actual-site/class-flow and
//! external witness obligations are discharged. A checked catalogue alone is
//! never an executable module or a permission.
use crate::*;
pub(crate) mod api;
pub(crate) use api::*;
pub(crate) mod scalar;
use std::collections::BTreeMap;
use std::cell::RefCell;
mod lifetimes;
mod relations;
mod evaluate;
mod requirements;
mod constants;
mod nominal;
mod closures;
mod catalogue;
mod instances;
mod flow;
mod capture_heads;
mod original;
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
    definitions: BTreeMap<SemanticDefinitionId, usize>,
    instances: BTreeMap<SemanticInstanceId, usize>,
    premises: RefCell<BTreeMap<ScopeKey, Premises>>,
    occurrences: RefCell<BTreeMap<(ScopeKey, OperandKey), CheckedType>>,
    seals: RefCell<BTreeMap<SemanticInstanceId, Vec<CheckedActual>>>,
    pending_closures: RefCell<Vec<ClosureCursor>>,
    origin_aliases: RefCell<BTreeMap<IntrinsicOrigin, IntrinsicOrigin>>,
    original: RefCell<Option<original::CheckedOriginal>>,
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
    pub(crate) fn register_closure_cursor(&self,instance:SemanticInstanceId,environment:&EvalEnv,arguments:&[CheckedActual],sealed:&[CheckedActual],expected:&CheckedType,b:&mut Budget)->Check<()> {
        dynamic_metadata::ty(&expected.ty,b)?;
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
fn intrinsic_count(module:&TypedModule, ty:&Type,b:&mut Budget)->Check<u32> {
    dynamic_witness::intrinsic_domain(module,ty,b).map(|(count,_)|count)
}

#[cfg(test)]
mod tests;
