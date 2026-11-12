//! Full original ordinal scopes, independent of entry ABI or table reachability.
use super::*;

struct Scope<'a> {
    declaration: &'a [LifeParameterKind],
    method: &'a [LifeParameterKind],
    invocation: u32,
    bound: Vec<u32>,
    established_self: bool,
}
fn lookup<'a>(
    module: &'a TypedModule,
    id: LifeDeclarationId,
    b: &mut Budget,
) -> Check<&'a DynDeclaration> {
    b.spend(module.dyn_declarations.len().saturating_add(1))?;
    let mut rows = module.dyn_declarations.iter().filter(|row| row.id == id);
    let row = rows
        .next()
        .ok_or_else(|| bad("original dyn declaration is absent"))?;
    if rows.next().is_some() {
        return Err(bad("original dyn declaration is ambiguous"));
    }
    Ok(row)
}
impl Scope<'_> {
    fn depth(&self, depth: usize, b: &mut Budget) -> Check<()> {
        b.spend(1)?;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("dynamic original recipe scope depth exceeded"));
        }
        Ok(())
    }
    fn life(&self, life: &DynLifePattern, b: &mut Budget) -> Check<()> {
        b.spend(1)?;
        let valid = match life {
            DynLifePattern::DeclarationParameter(i) => matches!(
                self.declaration.get(*i as usize),
                Some(LifeParameterKind::Lifetime)
            ),
            DynLifePattern::MethodParameter(i) => matches!(
                self.method.get(*i as usize),
                Some(LifeParameterKind::Lifetime)
            ),
            DynLifePattern::Invocation(i) => *i < self.invocation,
            DynLifePattern::Bound { depth, index } => self
                .bound
                .iter()
                .rev()
                .nth(*depth as usize)
                .is_some_and(|count| *index < *count),
        };
        if !valid {
            return Err(bad(
                "original life recipe selects a different formal kind or lexical scope",
            ));
        }
        Ok(())
    }
    fn concrete(&self, ty: &Type, b: &mut Budget) -> Check<()> {
        dynamic_metadata::ty(ty, b)?;
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            b.spend(1)?;
            match ty {
                Type::Named(_) | Type::Unsupported(_) => {
                    return Err(bad(
                        "concrete dynamic type contains an unresolved placeholder",
                    ));
                }
                Type::BorrowedStorage { storage, .. }
                | Type::Address { inner: storage, .. }
                | Type::Managed(storage)
                | Type::Desc(storage)
                | Type::Array {
                    element: storage, ..
                }
                | Type::Slice { element: storage }
                | Type::OwnedStore { element: storage }
                | Type::Mutex { payload: storage }
                | Type::Registration {
                    environment: storage,
                }
                | Type::AsyncRegistration {
                    environment: storage,
                }
                | Type::Opaque {
                    hidden: storage, ..
                } => pending.push(storage),
                Type::Record { fields, .. } | Type::Package { fields, .. } => {
                    b.spend(fields.len())?;
                    pending.extend(fields.iter().map(|(_, ty)| ty));
                }
                Type::Tuple(fields) => {
                    b.spend(fields.len())?;
                    pending.extend(fields);
                }
                Type::Enum { variants, .. } => {
                    for (_, fields) in variants {
                        b.spend(fields.len().saturating_add(1))?;
                        pending.extend(fields);
                    }
                }
                Type::ErrorUnion { error, result } => {
                    pending.extend([error.as_ref(), result.as_ref()])
                }
                Type::Task {
                    result,
                    retained_life,
                }
                | Type::MutexGuard {
                    payload: result,
                    retained_life,
                } => {
                    b.spend(retained_life.ports.len())?;
                    pending.push(result);
                    pending.extend(retained_life.ports.iter().map(|row| &row.leaf));
                }
                Type::FunctionPointer { parameters, result }
                | Type::ForeignCallback { parameters, result }
                | Type::BorrowedFunctionPointer {
                    parameters, result, ..
                } => {
                    b.spend(parameters.len())?;
                    pending.push(result);
                    pending.extend(parameters);
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn application(
        &self,
        module: &TypedModule,
        app: &DynSelfPattern,
        trait_application: bool,
        depth: usize,
        b: &mut Budget,
    ) -> Check<()> {
        self.depth(depth, b)?;
        let kinds = if trait_application {
            let declaration = lookup(module, app.declaration, b)?;
            if declaration.kind != DynDeclarationKind::Trait {
                return Err(bad("trait application selects a nontrait declaration"));
            }
            &declaration.parameters
        } else {
            b.spend(module.life_declarations.len().saturating_add(1))?;
            let mut rows = module
                .life_declarations
                .iter()
                .filter(|row| row.id == app.declaration);
            let declaration = rows
                .next()
                .ok_or_else(|| bad("nominal recipe has no original declaration"))?;
            if rows.next().is_some() {
                return Err(bad("nominal recipe declaration is ambiguous"));
            }
            &declaration.parameters
        };
        b.spend(kinds.len().saturating_add(app.arguments.len()))?;
        if kinds.len() != app.arguments.len() {
            return Err(bad("original application omits full ordered actuals"));
        }
        for (kind, actual) in kinds.iter().zip(&app.arguments) {
            match (kind, actual) {
                (LifeParameterKind::Type, DynPatternActual::Type(ty)) => {
                    self.ty(module, ty, depth + 1, b)?
                }
                (LifeParameterKind::Lifetime, DynPatternActual::Life(life)) => {
                    self.life(life, b)?
                }
                (LifeParameterKind::Const(_), DynPatternActual::Const(value)) => {
                    self.constant(module, value, depth + 1, b)?
                }
                _ => {
                    return Err(bad(
                        "original application actual has a different formal kind",
                    ));
                }
            }
        }
        Ok(())
    }
    fn member(
        &self,
        module: &TypedModule,
        app: &DynSelfPattern,
        member: u32,
        constant: bool,
        b: &mut Budget,
    ) -> Check<()> {
        let declaration = lookup(module, app.declaration, b)?;
        b.spend(1)?;
        let actual = declaration
            .members
            .get(member as usize)
            .ok_or_else(|| bad("associated projection original member is absent"))?;
        if !matches!(
            (&actual.kind, constant),
            (DynMemberKind::Type, false) | (DynMemberKind::Constant, true)
        ) {
            return Err(bad(
                "associated projection selects a different original member kind",
            ));
        }
        Ok(())
    }
    fn ty(
        &self,
        module: &TypedModule,
        ty: &DynTypePattern,
        depth: usize,
        b: &mut Budget,
    ) -> Check<()> {
        self.depth(depth, b)?;
        match ty {
            DynTypePattern::SelfType => {
                if !self.established_self {
                    return Err(bad("impl Self recipe circularly refers to Self"));
                }
            }
            DynTypePattern::Parameter(i) => {
                if !matches!(
                    self.declaration.get(*i as usize),
                    Some(LifeParameterKind::Type)
                ) {
                    return Err(bad("type recipe selects a non-Type declaration formal"));
                }
            }
            DynTypePattern::MethodParameter(i) => {
                if !matches!(self.method.get(*i as usize), Some(LifeParameterKind::Type)) {
                    return Err(bad("type recipe selects a non-Type method formal"));
                }
            }
            DynTypePattern::Concrete(ty) => self.concrete(ty, b)?,
            DynTypePattern::Nominal(app) => self.application(module, app, false, depth + 1, b)?,
            DynTypePattern::AssociatedProjection {
                receiver,
                application,
                member,
            } => {
                self.ty(module, receiver, depth + 1, b)?;
                self.application(module, application, true, depth + 1, b)?;
                self.member(module, application, *member, false, b)?;
            }
            DynTypePattern::Address {
                lifetime, inner, ..
            } => {
                if let Some(life) = lifetime {
                    self.life(life, b)?;
                }
                self.ty(module, inner, depth + 1, b)?;
            }
            DynTypePattern::Tuple(fields) => {
                b.spend(fields.len())?;
                for field in fields {
                    self.ty(module, field, depth + 1, b)?;
                }
            }
            DynTypePattern::Array { element, length } => {
                self.ty(module, element, depth + 1, b)?;
                self.constant(module, length, depth + 1, b)?;
            }
            DynTypePattern::Slice(inner)
            | DynTypePattern::Managed(inner)
            | DynTypePattern::Desc(inner)
            | DynTypePattern::OwnedStore(inner) => self.ty(module, inner, depth + 1, b)?,
            DynTypePattern::ErrorUnion { error, result } => {
                self.ty(module, error, depth + 1, b)?;
                self.ty(module, result, depth + 1, b)?;
            }
        }
        Ok(())
    }
    fn constant(
        &self,
        module: &TypedModule,
        value: &DynConstPattern,
        depth: usize,
        b: &mut Budget,
    ) -> Check<()> {
        self.depth(depth, b)?;
        match value {
            DynConstPattern::Parameter(i) => {
                if !matches!(
                    self.declaration.get(*i as usize),
                    Some(LifeParameterKind::Const(_))
                ) {
                    return Err(bad(
                        "constant recipe selects a non-Const declaration formal",
                    ));
                }
            }
            DynConstPattern::MethodParameter(i) => {
                if !matches!(
                    self.method.get(*i as usize),
                    Some(LifeParameterKind::Const(_))
                ) {
                    return Err(bad("constant recipe selects a non-Const method formal"));
                }
            }
            DynConstPattern::Value(value) => self.concrete(&value.ty(), b)?,
            DynConstPattern::IntegerLiteral(text) | DynConstPattern::FloatLiteral(text) => {
                b.spend(text.len().saturating_add(1))?;
                if text.is_empty() {
                    return Err(bad("original constant literal is empty"));
                }
            }
            DynConstPattern::Unary { value, ty, .. } | DynConstPattern::Convert { value, ty } => {
                self.constant(module, value, depth + 1, b)?;
                self.ty(module, ty, depth + 1, b)?;
            }
            DynConstPattern::Binary {
                left, right, ty, ..
            } => {
                self.constant(module, left, depth + 1, b)?;
                self.constant(module, right, depth + 1, b)?;
                self.ty(module, ty, depth + 1, b)?;
            }
            DynConstPattern::AssociatedProjection {
                receiver,
                application,
                member,
            } => {
                self.ty(module, receiver, depth + 1, b)?;
                self.application(module, application, true, depth + 1, b)?;
                self.member(module, application, *member, true, b)?;
            }
        }
        Ok(())
    }
    fn signature(
        &mut self,
        module: &TypedModule,
        signature: &DynSignatureRecipe,
        b: &mut Budget,
    ) -> Check<()> {
        let old = self.invocation;
        self.invocation = signature.invocation_binder_count;
        b.spend(signature.parameters.len())?;
        for parameter in &signature.parameters {
            self.ty(module, &parameter.ty, 0, b)?;
        }
        self.ty(module, &signature.result, 0, b)?;
        let c = &signature.contract;
        if c.requires.is_none()
            || c.guarantees.is_none()
            || c.hold.is_none()
            || c.call.is_none()
            || c.result.is_none()
            || c.cleanup.is_none()
            || c.effects.is_none()
            || c.resume.is_none()
            || c.threads.is_none()
        {
            return Err(bad(
                "original dynamic signature contains an unknown contract component",
            ));
        }
        for requirement in c.requires.iter().flatten() {
            b.spend(1)?;
            if let DynRequirementRecipe::State { ty, .. } = requirement {
                self.ty(module, ty, 0, b)?;
            }
        }
        for guarantee in c.guarantees.iter().flatten() {
            b.spend(1)?;
            if let DynGuaranteeRecipe::State { ty, .. } = guarantee {
                self.ty(module, ty, 0, b)?;
            }
        }
        for effect in c
            .effects
            .iter()
            .flatten()
            .chain(c.cleanup.iter().flat_map(|cleanup| &cleanup.effects))
        {
            b.spend(1)?;
            if let DynEffectRecipe::Cleanup(ty) = effect {
                self.ty(module, ty, 0, b)?;
            }
        }
        self.invocation = old;
        Ok(())
    }
    fn obligation(
        &mut self,
        module: &TypedModule,
        value: &DynObligation,
        depth: usize,
        b: &mut Budget,
    ) -> Check<()> {
        self.depth(depth, b)?;
        match value {
            DynObligation::Structural { receiver, .. } => {
                self.ty(module, receiver, depth + 1, b)?
            }
            DynObligation::Trait {
                receiver,
                application,
            } => {
                self.ty(module, receiver, depth + 1, b)?;
                self.application(module, application, true, depth + 1, b)?;
            }
            DynObligation::Callable {
                receiver,
                quantified_lifetimes,
                signature,
                ..
            } => {
                self.ty(module, receiver, depth + 1, b)?;
                b.spend(1)?;
                self.bound.push(*quantified_lifetimes);
                self.signature(module, signature, b)?;
                self.bound.pop();
            }
            DynObligation::TypeEqual { left, right } => {
                self.ty(module, left, depth + 1, b)?;
                self.ty(module, right, depth + 1, b)?;
            }
            DynObligation::ConstEqual { ty, left, right } => {
                self.ty(module, ty, depth + 1, b)?;
                self.constant(module, left, depth + 1, b)?;
                self.constant(module, right, depth + 1, b)?;
            }
        }
        Ok(())
    }
}

pub(super) fn check(module: &TypedModule, b: &mut Budget) -> Check<()> {
    b.spend(module.dyn_declarations.len())?;
    for declaration in &module.dyn_declarations {
        dynamic_metadata::declaration(declaration, b)?;
        dynamic_metadata::recipe_declaration(declaration, b)?;
    }
    let mut edges = Vec::new();
    for declaration in &module.dyn_declarations {
        let mut scope = Scope {
            declaration: &declaration.parameters,
            method: &[],
            invocation: 0,
            bound: vec![],
            established_self: declaration.kind == DynDeclarationKind::Trait,
        };
        let requirements = declaration
            .requirements
            .as_ref()
            .ok_or_else(|| bad("original declaration requirements are Unknown"))?;
        let supers = declaration
            .direct_supertraits
            .as_ref()
            .ok_or_else(|| bad("original direct super edges are Unknown"))?;
        let mut targets = Vec::new();
        b.spend(supers.len())?;
        if declaration.kind == DynDeclarationKind::Impl {
            if !supers.is_empty() {
                return Err(bad("implementation declaration invents direct super edges"));
            }
            let own = declaration
                .self_pattern
                .as_ref()
                .ok_or_else(|| bad("implementation Self recipe is Unknown"))?;
            scope.ty(module, own, 0, b)?;
            scope.established_self = true;
            scope.application(
                module,
                declaration
                    .implemented_trait
                    .as_ref()
                    .ok_or_else(|| bad("implemented trait recipe is Unknown"))?,
                true,
                0,
                b,
            )?;
        } else {
            if declaration.self_pattern.is_some() || declaration.implemented_trait.is_some() {
                return Err(bad("trait declaration claims an implementation"));
            }
            for edge in supers {
                scope.application(module, edge, true, 0, b)?;
                b.spend(module.dyn_declarations.len())?;
                let target = module
                    .dyn_declarations
                    .iter()
                    .position(|d| d.id == edge.declaration)
                    .ok_or_else(|| bad("super declaration absent"))?;
                targets.push(target);
            }
        }
        b.spend(requirements.len())?;
        for requirement in requirements {
            scope.obligation(module, requirement, 0, b)?;
        }
        edges.push(targets);
    }
    // Traverse the original graph, before applications can disguise a growing
    // edge. Equal concrete method signatures never make a declaration SCC legal.
    let mut marks = vec![0u8; edges.len()];
    for root in 0..edges.len() {
        if marks[root] != 0 {
            continue;
        }
        let mut pending = vec![(root, 0usize)];
        marks[root] = 1;
        while let Some((node, next)) = pending.last_mut() {
            b.spend(1)?;
            if *next == edges[*node].len() {
                marks[*node] = 2;
                pending.pop();
                continue;
            }
            let child = edges[*node][*next];
            *next += 1;
            if marks[child] == 1 {
                return Err(bad("original direct super graph contains a cycle"));
            }
            if marks[child] == 0 {
                marks[child] = 1;
                pending.push((child, 0));
            }
        }
    }
    Ok(())
}
