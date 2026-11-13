//! Independent interpretation of the untrusted lexical ghost program.
//! The outer Some gate remains closed until all actual-site/class-flow and
//! external witness obligations are discharged. A checked catalogue alone is
//! never an executable module or a permission.
use crate::*;
pub(crate) mod api;
pub(crate) use api::*;
pub(crate) mod scalar;
use std::collections::BTreeMap;

fn bad(message: &str) -> Diagnostic {
    reject("E-VERIFY-SEMANTIC", None, message)
}

/// Indices into the same immutable candidate. No mirrored raw table is a
/// source of builtin, visibility, ownership or runtime borrow authority.
pub(crate) struct CheckedContext {
    definitions: BTreeMap<SemanticDefinitionId, usize>,
    instances: BTreeMap<SemanticInstanceId, usize>,
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

    pub(crate) fn environment_for_instance(
        &self,
        _module: &TypedModule,
        _instance: SemanticInstanceId,
        _active: &mut ActiveQueries,
        _b: &mut Budget,
    ) -> Check<EvalEnv> {
        Err(unsupported(
            "semantic instance requires complete rigid/header/environment validation",
            None,
        ))
    }
    pub(crate) fn evaluate_type(
        &self,
        _module: &TypedModule,
        _recipe: &SemanticTypeRecipe,
        _environment: &EvalEnv,
        _external: &mut impl ExternalProofs,
        _active: &mut ActiveQueries,
        _b: &mut Budget,
    ) -> Check<CheckedType> {
        Err(unsupported(
            "semantic type requires its complete scoped interpretation",
            None,
        ))
    }
    pub(crate) fn evaluate_actual(
        &self,
        _module: &TypedModule,
        _recipe: &SemanticActualRecipe,
        _environment: &EvalEnv,
        _external: &mut impl ExternalProofs,
        _active: &mut ActiveQueries,
        _b: &mut Budget,
    ) -> Check<CheckedActual> {
        Err(unsupported(
            "semantic actual requires its complete scoped interpretation",
            None,
        ))
    }
    pub(crate) fn evaluate_signature(
        &self,
        _module: &TypedModule,
        _recipe: &SemanticSignatureRecipe,
        _environment: &EvalEnv,
        _external: &mut impl ExternalProofs,
        _active: &mut ActiveQueries,
        _b: &mut Budget,
    ) -> Check<CheckedSignature> {
        Err(unsupported(
            "semantic signature requires its complete scoped interpretation",
            None,
        ))
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
