//! Untrusted complete-consuming uppers for original opaque FnOnce results.
//! A Task still possibly owned at a normal exit requests Suspend. This is not
//! a Closed/SyncDrop/NoExternal proof, and does not change the closure body ABI.
use super::*;

enum Value<'a> {
    Plain,
    Task,
    Unknown,
    Tuple(Vec<Value<'a>>),
    Alternatives(Vec<Value<'a>>),
    Record(Vec<(&'a str, Value<'a>)>),
    Callable {
        definition: Option<usize>,
        scope: usize,
        kind: CallableKind,
        signature: &'a SemanticSignatureRecipe,
        suspend: bool,
        cleanup: bool,
    },
    Function(usize),
}
struct Flow<'a> {
    value: Option<Value<'a>>,
    live: bool,
    cleanup: bool,
    incomplete: bool,
}
struct TypeFrame<'a> {
    declaration: usize,
    caller: usize,
    arguments: &'a [SemanticActualRecipe],
    context: Option<usize>,
}
struct Walk<'a, 'b, 'l> {
    definitions: &'a [&'a SemanticDefinition],
    active: Vec<usize>,
    frames: Vec<TypeFrame<'a>>,
    context: Option<usize>,
    execution: bool,
    budget: &'b mut Budget<'l>,
}
type Env<'a> = Vec<(&'a str, Value<'a>)>;
impl<'a> Walk<'a, '_, '_> {
    fn copy(&mut self, value: &Value<'a>, depth: usize) -> R<Value<'a>> {
        self.budget.node(Span::default(), depth)?;
        Ok(match value {
            Value::Plain => Value::Plain,
            Value::Task => Value::Task,
            Value::Unknown => Value::Unknown,
            Value::Function(owner) => Value::Function(*owner),
            Value::Callable {
                definition,
                scope,
                kind,
                signature,
                suspend,
                cleanup,
            } => Value::Callable {
                definition: *definition,
                scope: *scope,
                kind: *kind,
                signature,
                suspend: *suspend,
                cleanup: *cleanup,
            },
            Value::Alternatives(fields) => {
                let mut result = Vec::new();
                for field in fields {
                    let value = self.copy(field, depth + 1)?;
                    self.budget
                        .lineage_push(&mut result, value, Span::default())?;
                }
                Value::Alternatives(result)
            }
            Value::Tuple(fields) => {
                let mut result = Vec::new();
                for field in fields {
                    let value = self.copy(field, depth + 1)?;
                    self.budget
                        .lineage_push(&mut result, value, Span::default())?;
                }
                Value::Tuple(result)
            }
            Value::Record(fields) => {
                let mut result = Vec::new();
                for (name, field) in fields {
                    let value = self.copy(field, depth + 1)?;
                    self.budget
                        .lineage_push(&mut result, (*name, value), Span::default())?;
                }
                Value::Record(result)
            }
        })
    }
    fn env_copy(&mut self, env: &Env<'a>, depth: usize) -> R<Env<'a>> {
        let mut result = Vec::new();
        for (name, value) in env {
            let value = self.copy(value, depth + 1)?;
            self.budget
                .lineage_push(&mut result, (*name, value), Span::default())?;
        }
        Ok(result)
    }
    fn pending(&mut self, value: &Value<'a>, depth: usize) -> R<bool> {
        self.budget.node(Span::default(), depth)?;
        Ok(match value {
            Value::Task | Value::Unknown => true,
            Value::Tuple(fields) | Value::Alternatives(fields) => {
                let mut pending = false;
                for field in fields {
                    pending |= self.pending(field, depth + 1)?;
                }
                pending
            }
            Value::Record(fields) => {
                let mut pending = false;
                for (_, field) in fields {
                    pending |= self.pending(field, depth + 1)?;
                }
                pending
            }
            // Calling FnOnce may settle its remaining environment; merely
            // owning/ignoring any callable never establishes SyncDrop.
            Value::Callable { cleanup, .. } => *cleanup,
            Value::Plain | Value::Function(_) => false,
        })
    }
    fn residual(&mut self, env: &Env<'a>, depth: usize) -> R<bool> {
        let mut result = false;
        for (_, value) in env {
            result |= self.pending(value, depth + 1)?;
        }
        Ok(result)
    }
    fn join(&mut self, left: Value<'a>, right: Value<'a>, depth: usize) -> R<Value<'a>> {
        self.budget.node(Span::default(), depth)?;
        Ok(match (left, right) {
            (Value::Plain, Value::Plain) => Value::Plain,
            (Value::Task, Value::Task) => Value::Task,
            (Value::Tuple(left), Value::Tuple(right)) if left.len() == right.len() => {
                let mut result = Vec::new();
                for (left, right) in left.into_iter().zip(right) {
                    let value = self.join(left, right, depth + 1)?;
                    self.budget
                        .lineage_push(&mut result, value, Span::default())?;
                }
                Value::Tuple(result)
            }
            (Value::Record(left), Value::Record(right)) if left.len() == right.len() => {
                let mut result = Vec::new();
                for ((name, left), (other, right)) in left.into_iter().zip(right) {
                    self.budget.text(name, Span::default())?;
                    self.budget.text(other, Span::default())?;
                    if name != other {
                        return Ok(Value::Unknown);
                    }
                    let value = self.join(left, right, depth + 1)?;
                    self.budget
                        .lineage_push(&mut result, (name, value), Span::default())?;
                }
                Value::Record(result)
            }
            (
                Value::Callable {
                    definition: a,
                    scope,
                    kind,
                    signature,
                    suspend: left,
                    cleanup: left_cleanup,
                },
                Value::Callable {
                    definition: b,
                    scope: other_scope,
                    kind: other,
                    signature: other_signature,
                    suspend: right,
                    cleanup: right_cleanup,
                },
            ) if a == b
                && scope == other_scope
                && kind == other
                && std::ptr::eq(signature, other_signature) =>
            {
                Value::Callable {
                    definition: a,
                    scope,
                    kind,
                    signature,
                    suspend: left || right,
                    cleanup: left_cleanup || right_cleanup,
                }
            }
            (
                left @ (Value::Callable { .. } | Value::Alternatives(_)),
                right @ (Value::Callable { .. } | Value::Alternatives(_)),
            ) => {
                // Preserve each original creator, not a common actual image.
                // Only the operation upper is joined, never its types/classes.
                let mut alternatives = Vec::new();
                self.budget
                    .lineage_push(&mut alternatives, left, Span::default())?;
                self.budget
                    .lineage_push(&mut alternatives, right, Span::default())?;
                Value::Alternatives(alternatives)
            }
            _ => Value::Unknown,
        })
    }
    fn merge_env(&mut self, env: &mut Env<'a>, other: Env<'a>, depth: usize) -> R<()> {
        if env.len() != other.len() {
            return Err(error(
                Span::default(),
                "cleanup candidate lexical branch differs",
            ));
        }
        for ((name, value), (other_name, other)) in env.iter_mut().zip(other) {
            self.budget.text(name, Span::default())?;
            self.budget.text(other_name, Span::default())?;
            if *name != other_name {
                return Err(error(
                    Span::default(),
                    "cleanup candidate shadow scope differs",
                ));
            }
            *value = self.join(std::mem::replace(value, Value::Plain), other, depth + 1)?;
        }
        Ok(())
    }
    fn lookup(&mut self, env: &Env<'a>, name: &str, depth: usize) -> R<Option<Value<'a>>> {
        for (binding, value) in env.iter().rev() {
            self.budget.node(Span::default(), depth)?;
            self.budget.text(name, Span::default())?;
            if *binding == name {
                return self.copy(value, depth + 1).map(Some);
            }
        }
        Ok(None)
    }
    fn take(
        &mut self,
        env: &mut Env<'a>,
        expression: &SemanticOriginalExpr,
        depth: usize,
    ) -> R<Value<'a>> {
        self.budget.node(expression.span, depth)?;
        match &expression.kind {
            CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. } => {
                self.take(env, inner, depth + 1)
            }
            CoreExprKind::Name(name) => {
                for (binding, value) in env.iter_mut().rev() {
                    self.budget.text(name, expression.span)?;
                    if *binding == name {
                        return Ok(std::mem::replace(value, Value::Plain));
                    }
                }
                Ok(Value::Unknown)
            }
            CoreExprKind::Member { .. } => {
                let mut path = Vec::new();
                let mut root = expression;
                loop {
                    self.budget.node(root.span, depth + path.len())?;
                    match &root.kind {
                        CoreExprKind::Group(inner)
                        | CoreExprKind::Ascribed { value: inner, .. } => root = inner,
                        CoreExprKind::Member { object, member, .. } => {
                            self.budget
                                .lineage_push(&mut path, member.as_str(), root.span)?;
                            root = object;
                        }
                        _ => break,
                    }
                }
                let result = if let CoreExprKind::Name(name) = &root.kind {
                    path.reverse();
                    let mut result = Value::Unknown;
                    for (binding, value) in env.iter_mut().rev() {
                        self.budget.text(name, root.span)?;
                        if *binding == name {
                            result = self.take_path(value, &path, depth + 1)?;
                            break;
                        }
                    }
                    result
                } else {
                    Value::Unknown
                };
                self.budget.drop_lineage_vec(path, expression.span)?;
                Ok(result)
            }
            // An unresolved projected owner is never erased wholesale.
            _ => Ok(Value::Unknown),
        }
    }
    fn take_path(&mut self, value: &mut Value<'a>, path: &[&str], depth: usize) -> R<Value<'a>> {
        self.budget.node(Span::default(), depth)?;
        let Some((first, rest)) = path.split_first() else {
            return Ok(std::mem::replace(value, Value::Plain));
        };
        match value {
            Value::Tuple(fields) => {
                let Some(field) = first
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| fields.get_mut(index))
                else {
                    return Ok(Value::Unknown);
                };
                self.take_path(field, rest, depth + 1)
            }
            Value::Record(fields) => {
                for (name, field) in fields {
                    self.budget.text(name, Span::default())?;
                    if name == first {
                        return self.take_path(field, rest, depth + 1);
                    }
                }
                Ok(Value::Unknown)
            }
            _ => Ok(Value::Unknown),
        }
    }
    fn transfer(
        &mut self,
        env: &mut Env<'a>,
        expression: &SemanticOriginalExpr,
        value: &Value<'a>,
        depth: usize,
    ) -> R<()> {
        self.budget.node(expression.span, depth)?;
        if matches!(
            value,
            Value::Callable { .. }
                | Value::Tuple(_)
                | Value::Record(_)
                | Value::Alternatives(_)
                | Value::Unknown
        ) {
            self.take(env, expression, depth + 1)?;
        }
        Ok(())
    }
    fn bound(
        &mut self,
        owner: usize,
        requirements: &'a [SemanticRequirementRecipe],
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<Option<Value<'a>>> {
        let mut result = None;
        for requirement in requirements {
            self.budget.node(Span::default(), depth)?;
            prepay_recipe_work(requirement, self.budget, Span::default(), depth)?;
            let candidate = match requirement {
                SemanticRequirementRecipe::Callable {
                    ty: subject,
                    kind,
                    signature,
                } if subject == ty => Some(Value::Callable {
                    definition: None,
                    scope: owner,
                    kind: *kind,
                    signature,
                    suspend: signature.call_kind == CallKind::Suspend,
                    cleanup: true,
                }),
                SemanticRequirementRecipe::Structural {
                    ty: subject,
                    bound: DynStructuralBound::Copy | DynStructuralBound::SyncDrop,
                } if subject == ty => Some(Value::Plain),
                SemanticRequirementRecipe::ForAll { requirements, .. } => {
                    self.bound(owner, requirements, ty, depth + 1)?
                }
                _ => None,
            };
            if let Some(candidate) = candidate {
                result = Some(match result {
                    None => candidate,
                    Some(old) => self.premise_join(old, candidate, depth + 1)?,
                });
            }
        }
        Ok(result)
    }
    fn premise_join(&mut self, left: Value<'a>, right: Value<'a>, depth: usize) -> R<Value<'a>> {
        // Plain here can only be the exact subject's original Copy/SyncDrop
        // premise, never the absence of an occupant on one control-flow arm.
        Ok(match (left, right) {
            (
                Value::Plain,
                Value::Callable {
                    definition,
                    scope,
                    kind,
                    signature,
                    suspend,
                    ..
                },
            )
            | (
                Value::Callable {
                    definition,
                    scope,
                    kind,
                    signature,
                    suspend,
                    ..
                },
                Value::Plain,
            ) => Value::Callable {
                definition,
                scope,
                kind,
                signature,
                suspend,
                cleanup: false,
            },
            (left, right) => self.join(left, right, depth + 1)?,
        })
    }
    fn ty(&mut self, owner: usize, ty: &'a SemanticTypeRecipe, depth: usize) -> R<Value<'a>> {
        self.budget.node(Span::default(), depth)?;
        Ok(match ty {
            SemanticTypeRecipe::Task { .. } | SemanticTypeRecipe::AsyncRegistration(_) => {
                Value::Task
            }
            SemanticTypeRecipe::Managed(inner)
            | SemanticTypeRecipe::Qualified { storage: inner, .. } => {
                self.ty(owner, inner, depth + 1)?
            }
            SemanticTypeRecipe::Address { .. }
            | SemanticTypeRecipe::Desc { .. }
            | SemanticTypeRecipe::Atom(_) => Value::Plain,
            SemanticTypeRecipe::Code { signature, .. } => Value::Callable {
                definition: None,
                scope: owner,
                kind: CallableKind::Fn,
                signature,
                suspend: signature.call_kind == CallKind::Suspend,
                cleanup: false,
            },
            SemanticTypeRecipe::Tuple(fields) => {
                let mut result = Vec::new();
                for field in fields {
                    let value = self.ty(owner, field, depth + 1)?;
                    self.budget
                        .lineage_push(&mut result, value, Span::default())?;
                }
                Value::Tuple(result)
            }
            SemanticTypeRecipe::Array { element, .. } | SemanticTypeRecipe::Slice(element) => {
                let element = self.ty(owner, element, depth + 1)?;
                if self.pending(&element, depth + 1)? {
                    Value::Unknown
                } else {
                    Value::Plain
                }
            }
            SemanticTypeRecipe::Formal(reference) => {
                let SemanticScopeRef::Ancestor(distance) = &reference.scope;
                let mut declaration = owner;
                for _ in 0..*distance {
                    self.budget.node(Span::default(), depth + 1)?;
                    let Some(parent) = self.definitions[declaration].parent else {
                        return Ok(Value::Unknown);
                    };
                    let Some(parent) =
                        declaration_index(self.definitions, parent, self.budget, Span::default())?
                    else {
                        return Ok(Value::Unknown);
                    };
                    declaration = parent;
                }
                let mut context = self.context;
                while let Some(index) = context {
                    self.budget.node(Span::default(), depth + 1)?;
                    let frame = &self.frames[index];
                    context = frame.context;
                    if frame.declaration == declaration {
                        let caller = frame.caller;
                        let arguments = frame.arguments;
                        let saved = std::mem::replace(&mut self.context, context);
                        let result = match arguments.get(reference.ordinal as usize) {
                            Some(SemanticActualRecipe::Type(ty)) => self.ty(caller, ty, depth + 1),
                            _ => Ok(Value::Unknown),
                        };
                        self.context = saved;
                        return result;
                    }
                }
                let definition = self.definitions[declaration];
                let local = SemanticTypeRecipe::Formal(SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal: reference.ordinal,
                });
                let own = match definition.parameters.get(reference.ordinal as usize) {
                    Some(SemanticFormal::Type { requirements }) => {
                        self.bound(declaration, requirements, &local, depth + 1)?
                    }
                    _ => None,
                };
                match (
                    own,
                    self.bound(declaration, &definition.requirements, &local, depth + 1)?,
                ) {
                    (Some(value), None) | (None, Some(value)) => value,
                    (Some(left), Some(right)) => self.premise_join(left, right, depth + 1)?,
                    _ => Value::Unknown,
                }
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => {
                let Some(index) =
                    declaration_index(self.definitions, *definition, self.budget, Span::default())?
                else {
                    return Ok(Value::Unknown);
                };
                if self.definitions[index].parameters.len() != arguments.len() {
                    return Ok(Value::Unknown);
                }
                let context = self.context;
                let frame = self.frames.len();
                self.budget.lineage_push(
                    &mut self.frames,
                    TypeFrame {
                        declaration: index,
                        caller: owner,
                        arguments,
                        context,
                    },
                    Span::default(),
                )?;
                self.context = Some(frame);
                let value = match &self.definitions[index].body {
                    SemanticDefinitionBody::Nominal {
                        body: SemanticNominalBody::Product { fields },
                        ..
                    } => {
                        let mut result = Vec::new();
                        for field in fields {
                            let value = self.ty(index, &field.ty, depth + 1)?;
                            self.budget.lineage_push(
                                &mut result,
                                (field.name.as_str(), value),
                                Span::default(),
                            )?;
                        }
                        Value::Record(result)
                    }
                    SemanticDefinitionBody::Nominal {
                        body: SemanticNominalBody::Enum { variants },
                        ..
                    } => {
                        let mut pending = false;
                        for variant in variants {
                            for field in &variant.fields {
                                let value = self.ty(index, field, depth + 1)?;
                                pending |= self.pending(&value, depth + 1)?;
                            }
                        }
                        if pending {
                            Value::Unknown
                        } else {
                            Value::Plain
                        }
                    }
                    _ => Value::Unknown,
                };
                self.frames.pop();
                self.context = context;
                value
            }
            SemanticTypeRecipe::Opaque { definition, .. } => {
                if let Some(index) =
                    declaration_index(self.definitions, *definition, self.budget, Span::default())?
                {
                    self.function_result(index, depth + 1)?
                } else {
                    Value::Unknown
                }
            }
            _ => Value::Unknown,
        })
    }
    fn target(
        &mut self,
        owner: usize,
        env: &Env<'a>,
        expression: &SemanticOriginalExpr,
        depth: usize,
    ) -> R<Option<usize>> {
        self.budget.node(expression.span, depth)?;
        let mut path = Vec::new();
        let mut current = expression;
        loop {
            match &current.kind {
                CoreExprKind::Group(inner)
                | CoreExprKind::TypeApply { callee: inner, .. }
                | CoreExprKind::GenericApply { callee: inner, .. } => current = inner,
                CoreExprKind::Member { object, member, .. } => {
                    self.budget
                        .lineage_push(&mut path, member.as_str(), expression.span)?;
                    current = object;
                }
                CoreExprKind::Name(name) => {
                    if self.lookup(env, name, depth + 1)?.is_some() {
                        return Ok(None);
                    }
                    self.budget
                        .lineage_push(&mut path, name.as_str(), expression.span)?;
                    break;
                }
                _ => return Ok(None),
            }
        }
        path.reverse();
        let SemanticDefinitionBody::FunctionWithBody { names, .. } = &self.definitions[owner].body
        else {
            return Ok(None);
        };
        let mut found = None;
        for binding in names {
            self.budget
                .identity_work(path.len().saturating_add(1), expression.span)?;
            if binding.namespace == SemanticOriginalNamespace::Value
                && binding
                    .path
                    .iter()
                    .map(String::as_str)
                    .eq(path.iter().copied())
            {
                let SemanticOriginalNameValue::Definition(id) = &binding.value else {
                    continue;
                };
                let index = declaration_index(self.definitions, *id, self.budget, expression.span)?;
                if found.is_some() && found != index {
                    return Ok(None);
                }
                found = index;
            }
        }
        self.budget.drop_lineage_vec(path, expression.span)?;
        Ok(found)
    }
    fn operation(&mut self, value: &Value<'a>, depth: usize) -> R<Option<(bool, bool)>> {
        self.budget.node(Span::default(), depth)?;
        Ok(match value {
            Value::Callable {
                suspend, cleanup, ..
            } => Some((*suspend, *cleanup)),
            Value::Function(index) => signature(self.definitions[*index])
                .map(|signature| (signature.call_kind == CallKind::Suspend, false)),
            Value::Alternatives(values) => {
                let mut suspend = false;
                let mut cleanup = false;
                if values.is_empty() {
                    return Ok(None);
                }
                for value in values {
                    let Some((next, remaining)) = self.operation(value, depth + 1)? else {
                        return Ok(None);
                    };
                    suspend |= next;
                    cleanup |= remaining;
                }
                Some((suspend, cleanup))
            }
            _ => None,
        })
    }
    fn function_result(&mut self, owner: usize, depth: usize) -> R<Value<'a>> {
        self.budget.node(Span::default(), depth)?;
        self.budget
            .identity_work(self.active.len(), Span::default())?;
        if self.active.contains(&owner) {
            return Ok(Value::Unknown);
        }
        let definition = self.definitions[owner];
        let mut bounds = Vec::new();
        opaque_bounds(
            &definition.requirements,
            &mut bounds,
            self.budget,
            depth + 1,
        )?;
        let (kind, bound) = match bounds.as_slice() {
            [(kind, bound)] => (*kind, *bound),
            [] => {
                return match signature(definition) {
                    Some(signature)
                        if !matches!(signature.result, SemanticTypeRecipe::Opaque { .. }) =>
                    {
                        self.ty(owner, &signature.result, depth + 1)
                    }
                    _ => Ok(Value::Unknown),
                };
            }
            _ => return Ok(Value::Unknown),
        };
        self.budget.drop_lineage_vec(bounds, Span::default())?;
        if !matches!(
            definition.body,
            SemanticDefinitionBody::FunctionWithBody {
                body: SemanticOriginalFunctionBody::Defined(_),
                ..
            }
        ) {
            // An abstract declaration's written interface is a conditional
            // premise, not an authored creator whose cleanup we can infer.
            return Ok(Value::Callable {
                definition: Some(owner),
                scope: owner,
                kind,
                signature: bound,
                suspend: bound.call_kind == CallKind::Suspend,
                cleanup: true,
            });
        }
        self.budget
            .lineage_push(&mut self.active, owner, Span::default())?;
        // A factory's exported operation is formed in its own complete Γ,
        // never under a particular nominal caller's substitution frame.
        let frames = std::mem::take(&mut self.frames);
        let context = self.context.take();
        let execution = std::mem::replace(&mut self.execution, false);
        let mut env = Vec::new();
        let flow = if let SemanticDefinitionBody::FunctionWithBody {
            signature,
            parameters,
            body: SemanticOriginalFunctionBody::Defined(body),
            ..
        } = &definition.body
        {
            for (parameter, value) in parameters.iter().zip(&signature.parameters) {
                let value = self.ty(owner, &value.ty, depth + 1)?;
                self.budget.lineage_push(
                    &mut env,
                    (parameter.name.as_str(), value),
                    parameter.span,
                )?;
            }
            self.block(owner, &mut env, body, depth + 1)?
        } else {
            Flow {
                value: None,
                live: true,
                cleanup: true,
                incomplete: true,
            }
        };
        self.active.pop();
        self.budget
            .drop_lineage_vec(std::mem::replace(&mut self.frames, frames), Span::default())?;
        self.context = context;
        self.execution = execution;
        // The complete declared result remains the public operation interface;
        // candidate creators are never substituted for its P/R or life domain.
        let (suspend, cleanup) = if !flow.live && !flow.incomplete {
            match flow.value {
                Some(value) => self.operation(&value, depth + 1)?.unwrap_or((true, true)),
                None => (true, true),
            }
        } else {
            (true, true)
        };
        // Nonconsuming factories keep their declared operation upper. Their
        // real capture cleanup candidate is still needed when a later FnOnce
        // wrapper owns that returned value; a scalar-only Fn/FnMut environment
        // must not be mistaken for an unknown asynchronous Task environment.
        let suspend = if kind == CallableKind::FnOnce {
            suspend
        } else {
            bound.call_kind == CallKind::Suspend
        };
        Ok(Value::Callable {
            definition: Some(owner),
            scope: owner,
            kind,
            signature: bound,
            suspend,
            cleanup,
        })
    }
    fn expr(
        &mut self,
        owner: usize,
        env: &mut Env<'a>,
        expression: &'a SemanticOriginalExpr,
        depth: usize,
    ) -> R<Value<'a>> {
        self.budget.node(expression.span, depth)?;
        use CoreExprKind as E;
        Ok(match &expression.kind {
            E::Name(name) => {
                if let Some(value) = self.lookup(env, name, depth + 1)? {
                    value
                } else {
                    self.target(owner, env, expression, depth + 1)?
                        .map_or(Value::Unknown, Value::Function)
                }
            }
            E::Group(value) | E::Ascribed { value, .. } | E::Try(value) | E::Spread(value) => {
                self.expr(owner, env, value, depth + 1)?
            }
            E::Unary {
                op: UnaryOp::Give,
                operand,
            } => self.take(env, operand, depth + 1)?,
            E::Unary {
                op: UnaryOp::Borrow | UnaryOp::BorrowMut,
                ..
            } => Value::Plain,
            E::Unary { operand, .. }
            | E::NumericConvert { value: operand, .. }
            | E::IntegerConvert { value: operand, .. } => {
                self.expr(owner, env, operand, depth + 1)?;
                Value::Plain
            }
            E::Tuple(fields) | E::Array(fields) => {
                let mut values = Vec::new();
                for field in fields {
                    let value = self.expr(owner, env, field, depth + 1)?;
                    self.budget.lineage_push(&mut values, value, field.span)?;
                }
                Value::Tuple(values)
            }
            E::Member { object, member, .. } => {
                let value = self.expr(owner, env, object, depth + 1)?;
                match value {
                    Value::Tuple(fields) => match member
                        .parse::<usize>()
                        .ok()
                        .and_then(|index| fields.get(index))
                    {
                        Some(field) => self.copy(field, depth + 1)?,
                        None => Value::Unknown,
                    },
                    Value::Record(fields) => {
                        self.budget.identity_work(fields.len(), expression.span)?;
                        match fields.iter().find(|(name, _)| *name == member) {
                            Some((_, value)) => self.copy(value, depth + 1)?,
                            None => Value::Unknown,
                        }
                    }
                    _ => Value::Unknown,
                }
            }
            E::Record { fields, .. } | E::RecordConstructor { fields, .. } => {
                let mut values = Vec::new();
                for (name, field) in fields {
                    let value = self.expr(owner, env, field, depth + 1)?;
                    self.budget
                        .lineage_push(&mut values, (name.as_str(), value), field.span)?;
                }
                Value::Record(values)
            }
            E::Call { callee, arguments } => {
                let target = self.target(owner, env, callee, depth + 1)?;
                let callable = self.expr(owner, env, callee, depth + 1)?;
                // Explicit give transfers the actual owner; observations do
                // not. The independent callee check still proves every input
                // mode, full type, result and cleanup obligation.
                for (ordinal, argument) in arguments.iter().enumerate() {
                    let value = self.expr(owner, env, argument, depth + 1)?;
                    let parameters = match &callable {
                        Value::Callable { signature, .. } => Some(signature.parameters.as_slice()),
                        Value::Function(index) => signature(self.definitions[*index])
                            .map(|signature| signature.parameters.as_slice()),
                        _ => target
                            .and_then(|index| signature(self.definitions[index]))
                            .map(|signature| signature.parameters.as_slice()),
                    };
                    if let Some(parameter) =
                        parameters.and_then(|parameters| parameters.get(ordinal))
                    {
                        if !matches!(
                            parameter.ty,
                            SemanticTypeRecipe::Address { .. } | SemanticTypeRecipe::Desc { .. }
                        ) {
                            self.transfer(env, argument, &value, depth + 1)?;
                        }
                    }
                }
                match callable {
                    Value::Callable {
                        scope,
                        kind,
                        signature,
                        suspend,
                        ..
                    } => {
                        self.execution |= suspend;
                        if kind == CallableKind::FnOnce {
                            self.take(env, callee, depth + 1)?;
                        }
                        self.ty(scope, &signature.result, depth + 1)?
                    }
                    Value::Function(index) => {
                        self.execution |= signature(self.definitions[index])
                            .is_none_or(|signature| signature.call_kind == CallKind::Suspend);
                        self.function_result(index, depth + 1)?
                    }
                    _ => {
                        if let Some(index) = target {
                            self.function_result(index, depth + 1)?
                        } else {
                            let mut head = callee.as_ref();
                            while let E::Group(inner)
                            | E::TypeApply { callee: inner, .. }
                            | E::GenericApply { callee: inner, .. } = &head.kind
                            {
                                head = inner;
                            }
                            match &head.kind {
                                E::Name(name)
                                    if self.lookup(env, name, depth + 1)?.is_none()
                                        && crate::primitive_name(name) =>
                                {
                                    // These sealed operations return scalar observations
                                    // or no owner. A view never retires its Task source.
                                    if matches!(
                                        name.as_str(),
                                        "task_valid"
                                            | "task_yield"
                                            | "task_request_cancel"
                                            | "task_finish_destroy"
                                            | "task_finish_destroy_checked"
                                    ) {
                                        Value::Plain
                                    } else {
                                        Value::Unknown
                                    }
                                }
                                _ => Value::Unknown,
                            }
                        }
                    }
                }
            }
            E::Closure { move_capture, .. } => {
                let definition = self.definitions[owner];
                let SemanticDefinitionBody::FunctionWithBody {
                    sites, site_nodes, ..
                } = &definition.body
                else {
                    return Ok(Value::Unknown);
                };
                self.budget.identity_work(
                    sites.len().saturating_add(site_nodes.len()),
                    expression.span,
                )?;
                let mut child = None;
                for node in site_nodes {
                    if node.node != SemanticOriginalNode::Expression(expression.id) {
                        continue;
                    }
                    if let Some(SemanticSiteRecipe {
                        kind: SemanticSiteRecipeKind::Closure { definition, .. },
                        ..
                    }) = sites.iter().find(|site| site.id == node.site)
                    {
                        let found = declaration_index(
                            self.definitions,
                            *definition,
                            self.budget,
                            expression.span,
                        )?;
                        if child.is_some() && child != found {
                            return Err(error(
                                expression.span,
                                "opaque cleanup has ambiguous original creator",
                            ));
                        }
                        child = found;
                    }
                }
                let Some(child) = child else {
                    return Ok(Value::Unknown);
                };
                if self.definitions[child].parent != Some(definition.id) {
                    return Err(error(
                        expression.span,
                        "opaque cleanup creator changes original parent",
                    ));
                }
                let SemanticDefinitionBody::FunctionWithBody {
                    signature: child_signature,
                    parameters,
                    body: SemanticOriginalFunctionBody::Defined(body),
                    ..
                } = &self.definitions[child].body
                else {
                    return Ok(Value::Unknown);
                };
                let mut captured = Vec::new();
                for slot in &self.definitions[child].environment {
                    self.budget.node(expression.span, depth + 1)?;
                    let SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType {
                        source: SemanticOriginalTypeSource::NameAt { at, name },
                        ..
                    }) = &slot.source
                    else {
                        return Ok(Value::Unknown);
                    };
                    if *at != SemanticOriginalNode::Expression(expression.id) {
                        return Err(error(
                            expression.span,
                            "opaque cleanup capture selects another creator",
                        ));
                    }
                    let value = if *move_capture {
                        self.lookup(env, name, depth + 1)?.unwrap_or(Value::Unknown)
                    } else {
                        Value::Plain
                    };
                    self.budget.lineage_push(
                        &mut captured,
                        (name.as_str(), value),
                        expression.span,
                    )?;
                    if *move_capture {
                        for (binding, value) in env.iter_mut().rev() {
                            self.budget.text(name, expression.span)?;
                            if *binding == name {
                                *value = Value::Plain;
                                break;
                            }
                        }
                    }
                }
                // Uncalled cleanup owns only captures, never hypothetical
                // invocation inputs which have not been supplied.
                let cleanup = self.residual(&captured, depth + 1)?;
                for (parameter, ty) in parameters.iter().zip(&child_signature.parameters) {
                    let value = self.ty(child, &ty.ty, depth + 1)?;
                    self.budget.lineage_push(
                        &mut captured,
                        (parameter.name.as_str(), value),
                        parameter.span,
                    )?;
                }
                let execution = std::mem::replace(&mut self.execution, false);
                let flow = self.block(child, &mut captured, body, depth + 1)?;
                let pending = self.execution
                    || flow.cleanup
                    || flow.incomplete
                    || (flow.live && self.residual(&captured, depth + 1)?);
                self.execution = execution;
                Value::Callable {
                    definition: Some(child),
                    scope: child,
                    kind: CallableKind::FnMut,
                    signature: child_signature,
                    suspend: pending || child_signature.call_kind == CallKind::Suspend,
                    cleanup,
                }
            }
            E::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(owner, env, condition, depth + 1)?;
                let mut other = self.env_copy(env, depth + 1)?;
                let left = self.expr(owner, env, then_value, depth + 1)?;
                let right = self.expr(owner, &mut other, else_value, depth + 1)?;
                self.merge_env(env, other, depth + 1)?;
                self.join(left, right, depth + 1)?
            }
            E::Sequence { bindings, value } => {
                let base = env.len();
                for (name, expression) in bindings {
                    let value = self.expr(owner, env, expression, depth + 1)?;
                    self.budget
                        .lineage_push(env, (name.as_str(), value), expression.span)?;
                }
                let value = self.expr(owner, env, value, depth + 1)?;
                // Hygienic temporaries are real owners too; untransported
                // responsibility cannot disappear at a lexical boundary.
                let mut extra = false;
                for (_, value) in &env[base..] {
                    extra |= self.pending(value, depth + 1)?;
                }
                env.truncate(base);
                if extra { Value::Unknown } else { value }
            }
            E::Binary { left, right, .. }
            | E::Range {
                start: left,
                end: right,
            } => {
                self.expr(owner, env, left, depth + 1)?;
                self.expr(owner, env, right, depth + 1)?;
                Value::Plain
            }
            E::TypeApply { callee, .. } | E::GenericApply { callee, .. } => {
                self.expr(owner, env, callee, depth + 1)?
            }
            E::Literal(_)
            | E::Integer(_)
            | E::Float(_)
            | E::Bool(_)
            | E::String(_)
            | E::Character(_)
            | E::Unit => Value::Plain,
            // No unsupported projection/control is treated as evidence that
            // a previously owned Task was transferred or synchronously closed.
            _ => Value::Unknown,
        })
    }
    fn pattern_owns(&mut self, pattern: &SemanticOriginalPattern, depth: usize) -> R<bool> {
        self.budget.node(pattern.span, depth)?;
        Ok(match &pattern.kind {
            CorePatternKind::Reference { .. } => false,
            CorePatternKind::Tuple(fields)
            | CorePatternKind::Variant { fields, .. }
            | CorePatternKind::Constructor { fields, .. } => {
                let mut owns = true;
                for field in fields {
                    owns &= self.pattern_owns(field, depth + 1)?;
                }
                owns
            }
            CorePatternKind::Record { fields, .. }
            | CorePatternKind::RecordConstructor { fields, .. } => {
                let mut owns = true;
                for (_, field) in fields {
                    owns &= self.pattern_owns(field, depth + 1)?;
                }
                owns
            }
            CorePatternKind::ErrorArm { pattern, .. } => self.pattern_owns(pattern, depth + 1)?,
            _ => true,
        })
    }
    fn pattern(
        &mut self,
        env: &mut Env<'a>,
        pattern: &'a SemanticOriginalPattern,
        value: Value<'a>,
        depth: usize,
    ) -> R<bool> {
        self.budget.node(pattern.span, depth)?;
        Ok(match (&pattern.kind, value) {
            (CorePatternKind::Name(name), value) => {
                self.budget
                    .lineage_push(env, (name.as_str(), value), pattern.span)?;
                false
            }
            (CorePatternKind::Tuple(patterns), Value::Tuple(values))
                if patterns.len() == values.len() =>
            {
                let mut dropped = false;
                for (pattern, value) in patterns.iter().zip(values) {
                    dropped |= self.pattern(env, pattern, value, depth + 1)?;
                }
                dropped
            }
            (
                CorePatternKind::Tuple(patterns)
                | CorePatternKind::Variant {
                    fields: patterns, ..
                }
                | CorePatternKind::Constructor {
                    fields: patterns, ..
                },
                Value::Plain,
            ) => {
                for pattern in patterns {
                    self.pattern(env, pattern, Value::Plain, depth + 1)?;
                }
                false
            }
            (
                CorePatternKind::Record {
                    fields: patterns, ..
                }
                | CorePatternKind::RecordConstructor {
                    fields: patterns, ..
                },
                Value::Record(fields),
            ) => {
                let mut dropped = false;
                for (name, value) in fields {
                    self.budget.identity_work(patterns.len(), pattern.span)?;
                    if let Some((_, field)) =
                        patterns.iter().find(|(field, _)| field.as_str() == name)
                    {
                        dropped |= self.pattern(env, field, value, depth + 1)?;
                    } else {
                        dropped |= self.pending(&value, depth + 1)?;
                    }
                }
                dropped
            }
            (CorePatternKind::Reference { pattern, .. }, value) => {
                let pending = self.pending(&value, depth + 1)?;
                self.pattern(env, pattern, Value::Plain, depth + 1)?;
                pending
            }
            (CorePatternKind::ErrorArm { pattern, .. }, value) => {
                self.pattern(env, pattern, value, depth + 1)?
            }
            (_, value) => self.pending(&value, depth + 1)?,
        })
    }
    fn result(
        &mut self,
        output: &mut Option<Value<'a>>,
        value: Option<Value<'a>>,
        depth: usize,
    ) -> R<()> {
        if let Some(value) = value {
            *output = Some(match output.take() {
                Some(old) => self.join(old, value, depth + 1)?,
                None => value,
            });
        }
        Ok(())
    }
    fn block(
        &mut self,
        owner: usize,
        env: &mut Env<'a>,
        body: &'a SemanticOriginalBlock,
        depth: usize,
    ) -> R<Flow<'a>> {
        self.budget.node(body.span, depth)?;
        let base = env.len();
        let mut output = Flow {
            value: None,
            live: true,
            cleanup: false,
            incomplete: false,
        };
        for statement in &body.statements {
            if !output.live {
                break;
            }
            self.budget.node(statement.span, depth + 1)?;
            use CoreStatementKind as S;
            match &statement.kind {
                S::Binding {
                    name,
                    initializer,
                    ty,
                    ..
                } => {
                    let value = if let Some(expression) = initializer {
                        let value = self.expr(owner, env, expression, depth + 1)?;
                        self.transfer(env, expression, &value, depth + 1)?;
                        if matches!(expression.kind, CoreExprKind::Name(_))
                            && matches!(value, Value::Task)
                        {
                            Value::Plain
                        } else {
                            value
                        }
                    } else if let Some(ty) = ty {
                        self.ty(owner, ty, depth + 1)?
                    } else {
                        Value::Unknown
                    };
                    self.budget
                        .lineage_push(env, (name.as_str(), value), statement.span)?;
                }
                S::TupleBinding {
                    pattern,
                    initializer,
                    ..
                } => {
                    let value = self.expr(owner, env, initializer, depth + 1)?;
                    if self.pattern_owns(pattern, depth + 1)? {
                        self.transfer(env, initializer, &value, depth + 1)?;
                    }
                    output.cleanup |= self.pattern(env, pattern, value, depth + 1)?;
                }
                S::Return(value) => {
                    let value = if let Some(expression) = value {
                        let value = self.expr(owner, env, expression, depth + 1)?;
                        self.transfer(env, expression, &value, depth + 1)?;
                        value
                    } else {
                        Value::Plain
                    };
                    // Returning an owned value transports its full responsibility.
                    output.cleanup |= self.residual(env, depth + 1)?;
                    self.result(&mut output.value, Some(value), depth + 1)?;
                    output.live = false;
                }
                S::Expression(value) | S::Yield(value) => {
                    let value = self.expr(owner, env, value, depth + 1)?;
                    output.cleanup |= self.pending(&value, depth + 1)?;
                }
                S::CompoundAssign { value, .. } => {
                    self.expr(owner, env, value, depth + 1)?;
                }
                S::Assign { target, value } => {
                    let value = self.expr(owner, env, value, depth + 1)?;
                    if let CoreExprKind::Name(name) = &target.kind {
                        for (binding, old) in env.iter_mut().rev() {
                            self.budget.text(name, statement.span)?;
                            if *binding == name {
                                output.cleanup |= self.pending(old, depth + 1)?;
                                *old = value;
                                break;
                            }
                        }
                    } else {
                        output.cleanup |= self.pending(&value, depth + 1)?;
                    }
                }
                S::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.expr(owner, env, condition, depth + 1)?;
                    let mut other = self.env_copy(env, depth + 1)?;
                    let left = self.block(owner, env, then_body, depth + 1)?;
                    let right = if let Some(body) = else_body {
                        self.block(owner, &mut other, body, depth + 1)?
                    } else {
                        Flow {
                            value: None,
                            live: true,
                            cleanup: false,
                            incomplete: false,
                        }
                    };
                    output.cleanup |= left.cleanup || right.cleanup;
                    output.incomplete |= left.incomplete || right.incomplete;
                    self.result(&mut output.value, left.value, depth + 1)?;
                    self.result(&mut output.value, right.value, depth + 1)?;
                    if left.live && right.live {
                        self.merge_env(env, other, depth + 1)?;
                    } else if right.live {
                        *env = other;
                    }
                    output.live = left.live || right.live;
                }
                S::Match { value, arms } => {
                    let subject = self.expr(owner, env, value, depth + 1)?;
                    let mut continuing = None;
                    for arm in arms {
                        let mut branch = self.env_copy(env, depth + 1)?;
                        let before = branch.len();
                        let subject = self.copy(&subject, depth + 1)?;
                        if self.pattern_owns(&arm.pattern, depth + 1)? {
                            self.transfer(&mut branch, value, &subject, depth + 1)?;
                        }
                        output.cleanup |=
                            self.pattern(&mut branch, &arm.pattern, subject, depth + 1)?;
                        let flow = self.block(owner, &mut branch, &arm.body, depth + 1)?;
                        output.cleanup |= flow.cleanup;
                        output.incomplete |= flow.incomplete;
                        self.result(&mut output.value, flow.value, depth + 1)?;
                        if flow.live {
                            for (_, value) in &branch[before..] {
                                output.cleanup |= self.pending(value, depth + 1)?;
                            }
                        }
                        branch.truncate(before);
                        if flow.live {
                            if let Some(previous) = continuing.as_mut() {
                                self.merge_env(previous, branch, depth + 1)?;
                            } else {
                                continuing = Some(branch);
                            }
                        }
                    }
                    output.live = continuing.is_some();
                    if let Some(branch) = continuing {
                        *env = branch;
                    }
                    if arms.is_empty() {
                        output.cleanup = true;
                    }
                }
                S::Block(body) | S::Unsafe(body) => {
                    let flow = self.block(owner, env, body, depth + 1)?;
                    output.cleanup |= flow.cleanup;
                    output.incomplete |= flow.incomplete;
                    self.result(&mut output.value, flow.value, depth + 1)?;
                    output.live = flow.live;
                }
                S::Loop(body)
                | S::While { body, .. }
                | S::ForRange { body, .. }
                | S::WhileLet { body, .. }
                | S::ForEach { body, .. } => {
                    let mut iteration = self.env_copy(env, depth + 1)?;
                    let flow = self.block(owner, &mut iteration, body, depth + 1)?;
                    output.cleanup |= flow.cleanup;
                    output.incomplete |= flow.incomplete;
                    self.result(&mut output.value, flow.value, depth + 1)?;
                    self.merge_env(env, iteration, depth + 1)?;
                }
                S::Defer(body) => {
                    let mut deferred = self.env_copy(env, depth + 1)?;
                    let flow = self.block(owner, &mut deferred, body, depth + 1)?;
                    output.cleanup |= flow.cleanup;
                    output.incomplete |= flow.incomplete || flow.value.is_some();
                    // Do not clear a current owner merely because a deferred
                    // body might retire it: intervening exits/updates retain
                    // that may-live responsibility until actual proof.
                }
                S::IfLet {
                    pattern,
                    value,
                    then_body,
                    else_body,
                } => {
                    let subject = self.expr(owner, env, value, depth + 1)?;
                    let mut other = self.env_copy(env, depth + 1)?;
                    let before = env.len();
                    if self.pattern_owns(pattern, depth + 1)? {
                        self.transfer(env, value, &subject, depth + 1)?;
                    }
                    output.cleanup |= self.pattern(env, pattern, subject, depth + 1)?;
                    let left = self.block(owner, env, then_body, depth + 1)?;
                    if left.live {
                        for (_, value) in &env[before..] {
                            output.cleanup |= self.pending(value, depth + 1)?;
                        }
                    }
                    env.truncate(before);
                    let right = if let Some(body) = else_body {
                        self.block(owner, &mut other, body, depth + 1)?
                    } else {
                        Flow {
                            value: None,
                            live: true,
                            cleanup: false,
                            incomplete: false,
                        }
                    };
                    output.cleanup |= left.cleanup || right.cleanup;
                    output.incomplete |= left.incomplete || right.incomplete;
                    self.result(&mut output.value, left.value, depth + 1)?;
                    self.result(&mut output.value, right.value, depth + 1)?;
                    if left.live && right.live {
                        self.merge_env(env, other, depth + 1)?;
                    } else if right.live {
                        *env = other;
                    }
                    output.live = left.live || right.live;
                }
                S::With { name, value, body } => {
                    let value = self.expr(owner, env, value, depth + 1)?;
                    let before = env.len();
                    self.budget
                        .lineage_push(env, (name.as_str(), value), statement.span)?;
                    let flow = self.block(owner, env, body, depth + 1)?;
                    output.cleanup |= flow.cleanup;
                    output.incomplete |= flow.incomplete;
                    if flow.live {
                        for (_, value) in &env[before..] {
                            output.cleanup |= self.pending(value, depth + 1)?;
                        }
                    }
                    env.truncate(before);
                    self.result(&mut output.value, flow.value, depth + 1)?;
                    output.live = flow.live;
                }
                S::Unsupported(_) => {
                    output.cleanup = true;
                    output.incomplete = true;
                }
                S::Break | S::Continue => {
                    break;
                }
                S::CompileTime(_) => {}
            }
        }
        if output.live {
            for (_, value) in &env[base..] {
                output.cleanup |= self.pending(value, depth + 1)?;
            }
        }
        env.truncate(base);
        Ok(output)
    }
}
fn opaque_bounds<'a>(
    rows: &'a [SemanticRequirementRecipe],
    output: &mut Vec<(CallableKind, &'a SemanticSignatureRecipe)>,
    budget: &mut Budget<'_>,
    depth: usize,
) -> R<()> {
    for row in rows {
        budget.node(Span::default(), depth)?;
        match row {
            SemanticRequirementRecipe::Callable {
                ty:
                    SemanticTypeRecipe::OriginalType {
                        scope: SemanticScopeRef::Ancestor(0),
                        source: SemanticOriginalTypeSource::Result,
                    },
                kind,
                signature,
            } => budget.lineage_push(output, (*kind, signature.as_ref()), Span::default())?,
            SemanticRequirementRecipe::ForAll { requirements, .. } => {
                opaque_bounds(requirements, output, budget, depth + 1)?
            }
            _ => {}
        }
    }
    Ok(())
}
fn propose_inner(modules: &mut [(ModuleId, SemanticModule)], budget: &mut Budget<'_>) -> R<bool> {
    let mut definitions = Vec::new();
    for (_, module) in modules.iter() {
        for definition in &module.definitions {
            budget.lineage_push(&mut definitions, definition, Span::default())?;
        }
    }
    let mut selected = Vec::new();
    for (index, definition) in definitions.iter().enumerate() {
        budget.node(Span::default(), 0)?;
        if !matches!(
            definition.body,
            SemanticDefinitionBody::FunctionWithBody {
                body: SemanticOriginalFunctionBody::Defined(_),
                ..
            }
        ) {
            continue;
        }
        // All candidate trees are temporary. Nothing but this bool escapes;
        // actual original definitions/signatures and their allocations remain
        // owned by the surrounding Source program.
        let mark = budget.lineage_checkpoint();
        let suspend = {
            let mut walk = Walk {
                definitions: &definitions,
                active: Vec::new(),
                frames: Vec::new(),
                context: None,
                execution: false,
                budget: &mut *budget,
            };
            let mut bounds = Vec::new();
            opaque_bounds(&definition.requirements, &mut bounds, walk.budget, 0)?;
            walk.budget.identity_work(bounds.len(), Span::default())?;
            if bounds.iter().any(|(kind, _)| *kind == CallableKind::FnOnce) {
                matches!(
                    walk.function_result(index, 0)?,
                    Value::Callable { suspend: true, .. } | Value::Unknown
                )
            } else {
                false
            }
        };
        budget.discard_lineage(mark, (), Span::default())?;
        if suspend {
            budget.lineage_push(&mut selected, definition.id, Span::default())?;
        }
    }
    budget.drop_lineage_vec(definitions, Span::default())?;
    fn publish(
        rows: &mut [SemanticRequirementRecipe],
        budget: &mut Budget<'_>,
        depth: usize,
    ) -> R<bool> {
        let mut changed = false;
        for row in rows {
            budget.node(Span::default(), depth)?;
            match row {
                SemanticRequirementRecipe::Callable {
                    ty:
                        SemanticTypeRecipe::OriginalType {
                            scope: SemanticScopeRef::Ancestor(0),
                            source: SemanticOriginalTypeSource::Result,
                        },
                    kind: CallableKind::FnOnce,
                    signature,
                } => {
                    changed |= signature.call_kind != CallKind::Suspend;
                    signature.call_kind = CallKind::Suspend;
                }
                SemanticRequirementRecipe::ForAll { requirements, .. } => {
                    changed |= publish(requirements, budget, depth + 1)?
                }
                _ => {}
            }
        }
        Ok(changed)
    }
    let mut changed = false;
    for (_, module) in modules {
        for definition in &mut module.definitions {
            budget.identity_work(selected.len(), Span::default())?;
            if selected.contains(&definition.id) {
                changed |= publish(&mut definition.requirements, budget, 0)?;
            }
        }
    }
    budget.drop_lineage_vec(selected, Span::default())?;
    Ok(changed)
}

pub(super) fn propose(
    modules: &mut [(ModuleId, SemanticModule)],
    budget: &mut Budget<'_>,
) -> R<bool> {
    let mark = budget.lineage_checkpoint();
    let result = propose_inner(modules, budget);
    // The only publication is CallKind on existing original signatures; all
    // helper values are dropped on both success and error before this reset.
    budget.discard_lineage(mark, (), Span::default())?;
    result
}
