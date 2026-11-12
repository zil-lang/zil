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
                let life = lifetime.as_ref().ok_or_else(|| {
                    bad("complete original Address recipe has an unknown lifetime domain")
                })?;
                self.life(life, b)?;
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
    if module.dyn_declarations.len() > b.limits.max_nodes {
        return Err(limit(
            "dynamic declaration inventory exceeds selected bound",
        ));
    }
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
        let associated = declaration
            .associated_definitions
            .as_ref()
            .ok_or_else(|| bad("original associated definitions are Unknown"))?;
        let mut seen_associated = std::collections::BTreeSet::new();
        for row in associated {
            b.spend(declaration.members.len().saturating_add(1))?;
            let member = declaration
                .members
                .get(row.member as usize)
                .ok_or_else(|| bad("original associated definition member is absent"))?;
            if !seen_associated.insert(row.member) {
                return Err(bad("duplicate original associated definition"));
            }
            match (&member.kind, &row.carrier, &row.value) {
                (DynMemberKind::Type, None, value) => {
                    if let Some(value) = value {
                        let DynAssociatedValueRecipe::Type(ty) = value else {
                            return Err(bad("associated type definition has a constant RHS"));
                        };
                        scope.ty(module, ty, 0, b)?;
                    }
                }
                (DynMemberKind::Constant, Some(carrier), value) => {
                    scope.ty(module, carrier, 0, b)?;
                    if let Some(value) = value {
                        let DynAssociatedValueRecipe::Constant { ty, value } = value else {
                            return Err(bad("associated constant definition has a type RHS"));
                        };
                        scope.ty(module, ty, 0, b)?;
                        scope.constant(module, value, 0, b)?;
                    }
                }
                _ => return Err(bad("associated original member kind or carrier differs")),
            }
            if declaration.kind == DynDeclarationKind::Impl && row.value.is_none() {
                return Err(bad("impl associated override has no original RHS"));
            }
            b.spend(row.requirements.len())?;
            for requirement in &row.requirements {
                scope.obligation(module, requirement, 0, b)?;
            }
        }
        for (index, member) in declaration.members.iter().enumerate() {
            b.spend(1)?;
            if matches!(member.kind, DynMemberKind::Type | DynMemberKind::Constant)
                && !seen_associated.contains(&(index as u32))
            {
                return Err(bad(
                    "original declaration omits an associated member's requirements",
                ));
            }
        }
        let signatures = declaration
            .method_signatures
            .as_ref()
            .ok_or_else(|| bad("original method signatures are Unknown"))?;
        let mut seen_signatures = std::collections::BTreeSet::new();
        for row in signatures {
            b.spend(declaration.members.len().saturating_add(1))?;
            let Some(DynMemberDeclaration {
                kind: DynMemberKind::Method(kinds),
                ..
            }) = declaration.members.get(row.member as usize)
            else {
                return Err(bad("original method recipe selects a nonmethod member"));
            };
            if !seen_signatures.insert(row.member) {
                return Err(bad("duplicate original method signature"));
            }
            let mut method = Scope {
                declaration: &declaration.parameters,
                method: kinds,
                invocation: 0,
                bound: vec![],
                established_self: true,
            };
            method.signature(module, &row.signature, b)?;
            for requirement in &row.requirements {
                method.obligation(module, requirement, 0, b)?;
            }
        }
        let definitions = declaration
            .method_definitions
            .as_ref()
            .ok_or_else(|| bad("original method definitions are Unknown"))?;
        let mut seen_definitions = std::collections::BTreeSet::new();
        for row in definitions {
            b.spend(declaration.members.len().saturating_add(1))?;
            let Some(DynMemberDeclaration {
                kind: DynMemberKind::Method(kinds),
                ..
            }) = declaration.members.get(row.member as usize)
            else {
                return Err(bad("original method definition selects a nonmethod member"));
            };
            if !seen_definitions.insert(row.member)
                || row.scope
                    != if declaration.kind == DynDeclarationKind::Trait {
                        DynDefinitionScope::TraitDefault
                    } else {
                        DynDefinitionScope::ImplOverride
                    }
            {
                return Err(bad(
                    "original method definition scope is duplicated or mismatched",
                ));
            }
            let mut method = Scope {
                declaration: &declaration.parameters,
                method: kinds,
                invocation: 0,
                bound: vec![],
                established_self: true,
            };
            method.signature(module, &row.signature, b)?;
            for requirement in &row.requirements {
                method.obligation(module, requirement, 0, b)?;
            }
        }
        for (index, member) in declaration.members.iter().enumerate() {
            b.spend(1)?;
            if matches!(member.kind, DynMemberKind::Method(_)) {
                if declaration.kind == DynDeclarationKind::Trait
                    && !seen_signatures.contains(&(index as u32))
                {
                    return Err(bad(
                        "original trait omits a method signature or prerequisites",
                    ));
                }
                if declaration.kind == DynDeclarationKind::Impl
                    && !seen_definitions.contains(&(index as u32))
                {
                    return Err(bad(
                        "original impl omits a method definition or prerequisites",
                    ));
                }
            }
        }
        for signature in signatures {
            if let Some(default) = signature.default_definition {
                b.spend(definitions.len())?;
                let mut rows = definitions.iter().filter(|row| {
                    row.id == default
                        && row.member == signature.member
                        && row.scope == DynDefinitionScope::TraitDefault
                });
                if rows.next().is_none() || rows.next().is_some() {
                    return Err(bad(
                        "trait default link does not select its unique original definition",
                    ));
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{ModuleId, NodeId};
    fn declaration(index: u32) -> DynDeclaration {
        DynDeclaration {
            id: LifeDeclarationId {
                module: ModuleId(0),
                declaration: NodeId(index),
                compiler: false,
            },
            identity: LifeDeclarationIdentity {
                package: "witness-test".into(),
                module_path: vec![],
                declaration_path: vec![format!("Trait{index}")],
                generation_path: vec![],
            },
            kind: DynDeclarationKind::Trait,
            parameters: vec![LifeParameterKind::Type],
            members: vec![],
            self_pattern: None,
            implemented_trait: None,
            direct_supertraits: Some(vec![]),
            requirements: Some(vec![]),
            associated_definitions: Some(vec![]),
            method_signatures: Some(vec![]),
            method_definitions: Some(vec![]),
        }
    }
    fn module() -> TypedModule {
        let (mut module, _) = crate::tests::baseline();
        module.dyn_declarations = vec![declaration(10), declaration(11)];
        module
    }
    fn run(module: &TypedModule) -> Check<()> {
        let limits = Limits::default();
        let raw = check(module, &mut Budget::new(&limits));
        let (_, plan) = crate::tests::baseline();
        let bytes = zil_ir::wire::encode_module_candidate(module, &plan, &limits).unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        let wire = check(&decoded, &mut Budget::new(&limits));
        assert_eq!(
            raw.as_ref().map(|_| ()).map_err(|e| (&e.code, &e.message)),
            wire.as_ref().map(|_| ()).map_err(|e| (&e.code, &e.message))
        );
        raw
    }
    #[test]
    fn complete_original_super_graph_and_every_unused_premise_are_scoped() {
        let mut module = module();
        module.dyn_declarations[0].direct_supertraits = Some(vec![DynSelfPattern {
            declaration: module.dyn_declarations[1].id,
            arguments: vec![DynPatternActual::Type(DynTypePattern::Parameter(0))],
        }]);
        assert!(run(&module).is_ok());
        module.dyn_declarations[0].requirements = Some(vec![DynObligation::Structural {
            receiver: DynTypePattern::MethodParameter(0),
            bound: DynStructuralBound::Copy,
        }]);
        assert_eq!(run(&module).unwrap_err().code, "E-VERIFY-DYN-WITNESS");
        module.dyn_declarations[0].requirements = None;
        assert!(run(&module).is_err());
    }
    #[test]
    fn changing_type_actuals_cannot_disguise_a_growing_original_super_cycle() {
        let mut module = module();
        let id = module.dyn_declarations[0].id;
        module.dyn_declarations[0].direct_supertraits = Some(vec![DynSelfPattern {
            declaration: id,
            arguments: vec![DynPatternActual::Type(DynTypePattern::OwnedStore(
                Box::new(DynTypePattern::Parameter(0)),
            ))],
        }]);
        assert!(run(&module).unwrap_err().message.contains("cycle"));
        module.dyn_declarations[0].direct_supertraits = Some(vec![DynSelfPattern {
            declaration: module.dyn_declarations[1].id,
            arguments: vec![DynPatternActual::Type(DynTypePattern::Parameter(0))],
        }]);
        module.dyn_declarations[1].direct_supertraits = Some(vec![DynSelfPattern {
            declaration: id,
            arguments: vec![DynPatternActual::Type(DynTypePattern::Parameter(0))],
        }]);
        assert!(run(&module).unwrap_err().message.contains("cycle"));
    }
    #[test]
    fn original_associated_carrier_and_full_formal_kind_remain_obligations() {
        let mut module = module();
        let trait_id = module.dyn_declarations[1].id;
        module.dyn_declarations[1]
            .members
            .push(DynMemberDeclaration {
                name: "Item".into(),
                kind: DynMemberKind::Type,
            });
        module.dyn_declarations[1].associated_definitions = Some(vec![DynAssociatedDefinition {
            member: 0,
            carrier: None,
            value: None,
            requirements: vec![],
        }]);
        module.dyn_declarations[0].requirements = Some(vec![DynObligation::TypeEqual {
            left: DynTypePattern::AssociatedProjection {
                receiver: Box::new(DynTypePattern::Parameter(0)),
                application: Box::new(DynSelfPattern {
                    declaration: trait_id,
                    arguments: vec![DynPatternActual::Type(DynTypePattern::Concrete(Type::I32))],
                }),
                member: 0,
            },
            right: DynTypePattern::Concrete(Type::I32),
        }]);
        assert!(run(&module).is_ok());
        module.dyn_declarations[1]
            .associated_definitions
            .as_mut()
            .unwrap()[0]
            .carrier = Some(DynTypePattern::Concrete(Type::U8));
        assert!(run(&module).is_err());
        module.dyn_declarations[1]
            .associated_definitions
            .as_mut()
            .unwrap()[0]
            .carrier = None;
        module.dyn_declarations[0].direct_supertraits = Some(vec![DynSelfPattern {
            declaration: trait_id,
            arguments: vec![DynPatternActual::Life(
                DynLifePattern::DeclarationParameter(0),
            )],
        }]);
        assert!(run(&module).is_err());
    }
}
