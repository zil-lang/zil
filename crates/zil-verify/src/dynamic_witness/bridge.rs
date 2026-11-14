//! Exact original owner lookup for the shared semantic type vocabulary. A
//! successful lookup is metadata only, never type evaluation or a W proof.
use super::*;

pub(super) fn owner<'a>(
    module: &'a TypedModule,
    expected: &SemanticDynOwner,
    requested: SemanticDefinitionId,
    b: &mut Budget,
) -> Check<&'a SemanticDefinition> {
    let semantic = module
        .semantic
        .as_ref()
        .ok_or_else(|| bad("semantic type bridge original program is Unknown"))?;
    b.spend(
        semantic
            .dyn_scopes
            .len()
            .saturating_add(semantic.definitions.len()),
    )?;
    let mut bindings = semantic
        .dyn_scopes
        .iter()
        .filter(|row| &row.owner == expected);
    let binding = bindings
        .next()
        .ok_or_else(|| bad("semantic type bridge has no exact original Dyn owner binding"))?;
    if bindings.next().is_some() || binding.scope != requested {
        return Err(bad(
            "semantic type bridge selects a different or duplicate original owner scope",
        ));
    }
    let mut definitions = semantic
        .definitions
        .iter()
        .filter(|definition| definition.id == requested);
    let definition = definitions
        .next()
        .ok_or_else(|| bad("semantic bridge owner definition is absent"))?;
    if definitions.next().is_some() {
        return Err(bad(
            "semantic bridge owner definition identity is duplicated",
        ));
    }
    match expected {
        SemanticDynOwner::Declaration(id) => match &definition.body {
            SemanticDefinitionBody::Scope(
                SemanticScopeRecipe::Trait { declaration }
                | SemanticScopeRecipe::InherentImpl { declaration, .. }
                | SemanticScopeRecipe::TraitImpl { declaration, .. },
            ) if declaration == id => {}
            _ => {
                return Err(bad(
                    "semantic bridge declaration binding is not its actual lexical scope",
                ));
            }
        },
        SemanticDynOwner::MethodSignature { .. } | SemanticDynOwner::MethodDefinition(_) => {
            if definition.kind != SemanticDefinitionKind::Method
                || !matches!(
                    definition.body,
                    SemanticDefinitionBody::Function { .. }
                        | SemanticDefinitionBody::FunctionWithBody { .. }
                )
            {
                return Err(bad(
                    "semantic bridge method binding is not an original method scope",
                ));
            }
        }
    }
    let (declaration, formals, original_identity) = match expected {
        SemanticDynOwner::Declaration(id)
        | SemanticDynOwner::MethodSignature {
            declaration: id, ..
        } => {
            b.spend(module.dyn_declarations.len())?;
            let mut rows = module.dyn_declarations.iter().filter(|d| d.id == *id);
            let declaration = rows
                .next()
                .ok_or_else(|| bad("semantic bridge has no actual original Dyn declaration"))?;
            if rows.next().is_some() {
                return Err(bad("semantic bridge original Dyn declaration is ambiguous"));
            }
            match expected {
                SemanticDynOwner::Declaration(_) => (
                    declaration,
                    &declaration.parameters,
                    Some(&declaration.identity),
                ),
                SemanticDynOwner::MethodSignature { member, .. } => {
                    let Some(DynMemberDeclaration {
                        kind: DynMemberKind::Method(formals),
                        ..
                    }) = declaration.members.get(*member as usize)
                    else {
                        return Err(bad(
                            "semantic bridge selects a different original member kind or ordinal",
                        ));
                    };
                    if !declaration
                        .method_signatures
                        .as_ref()
                        .is_some_and(|rows| rows.iter().any(|row| row.member == *member))
                    {
                        return Err(bad("semantic bridge original method signature is absent"));
                    }
                    (declaration, formals, None)
                }
                _ => unreachable!(),
            }
        }
        SemanticDynOwner::MethodDefinition(id) => {
            let mut selected = None;
            for declaration in &module.dyn_declarations {
                let rows = declaration
                    .method_definitions
                    .as_ref()
                    .ok_or_else(|| bad("semantic bridge original definitions are Unknown"))?;
                b.spend(rows.len().saturating_add(1))?;
                for row in rows.iter().filter(|row| row.id == *id) {
                    let Some(DynMemberDeclaration {
                        kind: DynMemberKind::Method(formals),
                        ..
                    }) = declaration.members.get(row.member as usize)
                    else {
                        return Err(bad(
                            "semantic bridge definition selects a nonmethod original member",
                        ));
                    };
                    if selected
                        .replace((declaration, formals, Some(&row.identity)))
                        .is_some()
                    {
                        return Err(bad(
                            "semantic bridge original method definition is ambiguous",
                        ));
                    }
                }
            }
            selected.ok_or_else(|| bad("semantic bridge original method definition is absent"))?
        }
    };
    if let Some(identity) = original_identity {
        for identity in [identity, &definition.identity] {
            b.spend(
                identity
                    .module_path
                    .len()
                    .saturating_add(identity.declaration_path.len())
                    .saturating_add(identity.generation_path.len())
                    .saturating_add(identity.package.len()),
            )?;
            for text in identity
                .module_path
                .iter()
                .chain(&identity.declaration_path)
            {
                b.spend(text.len())?;
            }
        }
        if identity != &definition.identity {
            return Err(bad(
                "semantic bridge changes the full original declaration identity",
            ));
        }
    }
    b.spend(formals.len().saturating_add(definition.parameters.len()))?;
    if formals.len() != definition.parameters.len()
        || !formals.iter().zip(&definition.parameters).all(|(a, z)| {
            matches!(
                (a, z),
                (LifeParameterKind::Type, SemanticFormal::Type { .. })
                    | (LifeParameterKind::Lifetime, SemanticFormal::Lifetime)
                    | (LifeParameterKind::Const(_), SemanticFormal::Const { .. })
            )
        })
    {
        return Err(bad(
            "semantic bridge changes complete original mixed formal order",
        ));
    }
    if !matches!(expected, SemanticDynOwner::Declaration(_)) {
        b.spend(semantic.dyn_scopes.len())?;
        let mut parents = semantic
            .dyn_scopes
            .iter()
            .filter(|binding| binding.owner == SemanticDynOwner::Declaration(declaration.id));
        let parent = parents
            .next()
            .ok_or_else(|| bad("semantic method bridge lacks its actual original owner parent"))?;
        if parents.next().is_some() || definition.parent != Some(parent.scope) {
            return Err(bad(
                "semantic method bridge changes its actual lexical parent",
            ));
        }
    }
    Ok(definition)
}

/// Original lexical grammar only. This does not discharge WF or where clauses;
/// the independent original checker and concrete interpreter retain those
/// obligations. In particular no implicit lifetime becomes an empty domain.
pub(super) fn original_type(
    module: &TypedModule,
    expected: &SemanticDynOwner,
    requested: SemanticDefinitionId,
    recipe: &SemanticTypeRecipe,
    bounds: &[u32],
    invocation: u32,
    b: &mut Budget,
) -> Check<()> {
    let definition = owner(module, expected, requested, b)?;
    b.spend(bounds.len())?;
    let mut grammar = Grammar {
        module,
        definition,
        bounds: bounds.to_vec(),
        invocation,
    };
    grammar.ty(recipe, b, 0)
}

struct Grammar<'a> {
    module: &'a TypedModule,
    definition: &'a SemanticDefinition,
    bounds: Vec<u32>,
    invocation: u32,
}
impl<'a> Grammar<'a> {
    fn step(&self, depth: usize, b: &mut Budget) -> Check<()> {
        b.spend(1)?;
        if depth >= b.limits.max_call_depth.min(128) {
            return Err(limit("original semantic grammar depth exceeded"));
        }
        Ok(())
    }
    fn definition(
        &self,
        reference: SemanticScopeRef,
        b: &mut Budget,
    ) -> Check<&'a SemanticDefinition> {
        let SemanticScopeRef::Ancestor(depth) = reference;
        let mut current = self.definition;
        for _ in 0..depth {
            let id = current
                .parent
                .ok_or_else(|| bad("original semantic recipe escapes its real lexical parent"))?;
            let program = self
                .module
                .semantic
                .as_ref()
                .ok_or_else(|| bad("original semantic program is absent"))?;
            b.spend(program.definitions.len())?;
            let mut rows = program.definitions.iter().filter(|r| r.id == id);
            current = rows
                .next()
                .ok_or_else(|| bad("original semantic parent is absent"))?;
            if rows.next().is_some() {
                return Err(bad("original semantic parent is duplicated"));
            }
        }
        Ok(current)
    }
    fn formal(&self, p: &SemanticFormalRef, b: &mut Budget) -> Check<&'a SemanticFormal> {
        self.definition(p.scope, b)?
            .parameters
            .get(p.ordinal as usize)
            .ok_or_else(|| bad("original semantic recipe selects an absent full formal"))
    }
    fn environment(
        &self,
        scope: SemanticScopeRef,
        ordinal: u32,
        b: &mut Budget,
    ) -> Check<&'a SemanticFormal> {
        self.definition(scope, b)?
            .environment
            .get(ordinal as usize)
            .map(|r| &r.kind)
            .ok_or_else(|| {
                bad("original semantic recipe selects an absent sealed environment slot")
            })
    }
    fn operand(&self, p: &SemanticRecipeOperand, b: &mut Budget) -> Check<()> {
        match p {
            SemanticRecipeOperand::TypeFormal(p) => {
                if !matches!(self.formal(p, b)?, SemanticFormal::Type { .. }) {
                    return Err(bad("original class operand selects a non-Type formal"));
                }
            }
            SemanticRecipeOperand::Environment { scope, ordinal } => {
                if !matches!(
                    self.environment(*scope, *ordinal, b)?,
                    SemanticFormal::Type { .. }
                ) {
                    return Err(bad(
                        "original class operand selects a non-Type environment slot",
                    ));
                }
            }
            SemanticRecipeOperand::Parameter(_) | SemanticRecipeOperand::Receiver => {
                if !matches!(
                    self.definition.body,
                    SemanticDefinitionBody::Function { .. }
                        | SemanticDefinitionBody::FunctionWithBody { .. }
                ) {
                    return Err(bad(
                        "original declaration type recipe reads a runtime operand outside a function",
                    ));
                }
            }
        }
        Ok(())
    }
    fn life(&self, p: &SemanticLifeRecipe, b: &mut Budget) -> Check<()> {
        b.spend(1)?;
        match p {
            SemanticLifeRecipe::Formal(p) => {
                if !matches!(self.formal(p, b)?, SemanticFormal::Lifetime) {
                    return Err(bad("original semantic life selects another formal kind"));
                }
            }
            SemanticLifeRecipe::Environment { scope, ordinal } => {
                if !matches!(
                    self.environment(*scope, *ordinal, b)?,
                    SemanticFormal::Lifetime
                ) {
                    return Err(bad(
                        "original semantic life selects another environment kind",
                    ));
                }
            }
            SemanticLifeRecipe::Implicit { scope, ordinal } => {
                let definition = self.definition(*scope, b)?;
                b.spend(definition.implicit_lifetimes.len())?;
                let mut rows = definition
                    .implicit_lifetimes
                    .iter()
                    .filter(|r| r.ordinal == *ordinal);
                let row = rows
                    .next()
                    .ok_or_else(|| bad("original semantic implicit occurrence is absent"))?;
                b.spend(row.occurrence_path.len())?;
                if rows.next().is_some() {
                    return Err(bad("original semantic implicit ordinal is duplicated"));
                }
            }
            SemanticLifeRecipe::Bound { depth, index } => {
                if !self
                    .bounds
                    .iter()
                    .rev()
                    .nth(*depth as usize)
                    .is_some_and(|n| *index < *n)
                {
                    return Err(bad(
                        "original semantic Bound escapes its complete quantifier domain",
                    ));
                }
            }
            SemanticLifeRecipe::Invocation(i) => {
                if *i >= self.invocation {
                    return Err(bad(
                        "original semantic Invocation escapes its signature domain",
                    ));
                }
            }
            SemanticLifeRecipe::OuterOccurrence { operand, path, .. }
            | SemanticLifeRecipe::IntrinsicClass { operand, path, .. } => {
                b.spend(path.len())?;
                self.operand(operand, b)?;
            }
        }
        Ok(())
    }
    fn actual(
        &mut self,
        p: &SemanticActualRecipe,
        formal: &SemanticFormal,
        b: &mut Budget,
        depth: usize,
    ) -> Check<()> {
        match (p, formal) {
            (SemanticActualRecipe::Type(p), SemanticFormal::Type { .. }) => {
                self.ty(p, b, depth + 1)
            }
            (SemanticActualRecipe::Life(p), SemanticFormal::Lifetime) => self.life(p, b),
            (SemanticActualRecipe::Const { carrier, value }, SemanticFormal::Const { .. }) => {
                self.ty(carrier, b, depth + 1)?;
                self.constant(value, b, depth + 1)
            }
            _ => Err(bad(
                "original semantic application changes its complete mixed formal kind",
            )),
        }
    }
    fn application(&mut self, p: &SemanticTraitRecipe, b: &mut Budget, depth: usize) -> Check<()> {
        b.spend(self.module.dyn_declarations.len())?;
        let mut rows = self
            .module
            .dyn_declarations
            .iter()
            .filter(|r| r.id == p.declaration);
        let declaration = rows
            .next()
            .ok_or_else(|| bad("original semantic trait application declaration is absent"))?;
        if rows.next().is_some()
            || declaration.kind != DynDeclarationKind::Trait
            || declaration.parameters.len() != p.arguments.len()
        {
            return Err(bad(
                "original semantic trait application has another identity or full arity",
            ));
        }
        for (formal, actual) in declaration.parameters.iter().zip(&p.arguments) {
            match (formal, actual) {
                (LifeParameterKind::Type, SemanticActualRecipe::Type(p)) => {
                    self.ty(p, b, depth + 1)?
                }
                (LifeParameterKind::Lifetime, SemanticActualRecipe::Life(p)) => self.life(p, b)?,
                (LifeParameterKind::Const(_), SemanticActualRecipe::Const { carrier, value }) => {
                    self.ty(carrier, b, depth + 1)?;
                    self.constant(value, b, depth + 1)?;
                }
                _ => {
                    return Err(bad(
                        "original semantic trait application changes a full formal kind",
                    ));
                }
            }
        }
        Ok(())
    }
    fn member(
        &self,
        p: &SemanticTraitRecipe,
        member: u32,
        constant: bool,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(self.module.dyn_declarations.len())?;
        let mut rows = self
            .module
            .dyn_declarations
            .iter()
            .filter(|r| r.id == p.declaration);
        let row = rows
            .next()
            .ok_or_else(|| bad("original associated declaration is absent"))?;
        if rows.next().is_some() {
            return Err(bad("original associated declaration duplicated"));
        }
        if !matches!(
            (row.members.get(member as usize).map(|m| &m.kind), constant),
            (Some(DynMemberKind::Type), false) | (Some(DynMemberKind::Constant), true)
        ) {
            return Err(bad(
                "original associated projection selects another full member kind",
            ));
        }
        Ok(())
    }
    fn constant(&mut self, p: &SemanticConstRecipe, b: &mut Budget, depth: usize) -> Check<()> {
        self.step(depth, b)?;
        match p {
            SemanticConstRecipe::Formal(p) => {
                if !matches!(self.formal(p, b)?, SemanticFormal::Const { .. }) {
                    return Err(bad("original constant selects another formal kind"));
                }
            }
            SemanticConstRecipe::Environment { scope, ordinal } => {
                if !matches!(
                    self.environment(*scope, *ordinal, b)?,
                    SemanticFormal::Const { .. }
                ) {
                    return Err(bad("original constant selects another environment kind"));
                }
            }
            SemanticConstRecipe::Value(v) => dynamic_metadata::ty(&v.ty(), b)?,
            SemanticConstRecipe::IntegerLiteral(v) | SemanticConstRecipe::FloatLiteral(v) => {
                b.spend(v.len())?;
                if v.is_empty() {
                    return Err(bad("original constant literal is empty"));
                }
            }
            SemanticConstRecipe::Unary {
                operand_type,
                value,
                ..
            } => {
                self.ty(operand_type, b, depth + 1)?;
                self.constant(value, b, depth + 1)?;
            }
            SemanticConstRecipe::Binary {
                operand_type,
                left,
                right,
                ..
            } => {
                self.ty(operand_type, b, depth + 1)?;
                self.constant(left, b, depth + 1)?;
                self.constant(right, b, depth + 1)?;
            }
            SemanticConstRecipe::Convert {
                source_type,
                target_type,
                value,
            } => {
                self.ty(source_type, b, depth + 1)?;
                self.ty(target_type, b, depth + 1)?;
                self.constant(value, b, depth + 1)?;
            }
            SemanticConstRecipe::AssociatedConst {
                receiver,
                application,
                member,
            } => {
                self.ty(receiver, b, depth + 1)?;
                self.application(application, b, depth + 1)?;
                self.member(application, *member, true, b)?;
            }
        }
        Ok(())
    }
    fn sequence(
        &mut self,
        p: &SemanticClassSequenceRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> Check<()> {
        for p in &p.parts {
            match p {
                SemanticClassSequencePart::One(p) => self.life(p, b)?,
                SemanticClassSequencePart::AllClasses { ty, path } => {
                    b.spend(path.len())?;
                    self.ty(ty, b, depth + 1)?;
                }
            }
        }
        Ok(())
    }
    fn retained(
        &mut self,
        retained: &[SemanticRetainedPart],
        nominal: &[SemanticNominalPart],
        b: &mut Budget,
        depth: usize,
    ) -> Check<()> {
        for p in retained {
            match p {
                SemanticRetainedPart::One { parent, path, .. } => {
                    b.spend(path.len())?;
                    self.life(parent, b)?;
                }
                SemanticRetainedPart::All { parents, path } => {
                    b.spend(path.len())?;
                    self.sequence(parents, b, depth + 1)?;
                }
            }
        }
        for p in nominal {
            match p {
                SemanticNominalPart::One { class, path, .. } => {
                    b.spend(path.len())?;
                    self.life(class, b)?;
                }
                SemanticNominalPart::FromType { ty, classes, path } => {
                    b.spend(path.len())?;
                    self.ty(ty, b, depth + 1)?;
                    self.sequence(classes, b, depth + 1)?;
                }
            }
        }
        Ok(())
    }
    fn retained_shape(
        &mut self,
        p: &SemanticRetainedShapeRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> Check<()> {
        self.sequence(&p.domain, b, depth + 1)?;
        for p in &p.ports {
            match p {
                SemanticPortPart::One { leaf, class, .. } => {
                    self.ty(leaf, b, depth + 1)?;
                    self.life(class, b)?;
                }
                SemanticPortPart::FromType { ty, classes } => {
                    self.ty(ty, b, depth + 1)?;
                    self.sequence(classes, b, depth + 1)?;
                }
            }
        }
        self.retained(&p.retained, &p.nominal, b, depth + 1)
    }
    fn signature(
        &mut self,
        p: &SemanticSignatureRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> Check<()> {
        let invocation = self.invocation;
        self.invocation = p.invocation_lifetimes;
        if p.bound_lifetimes != 0 {
            self.bounds.push(p.bound_lifetimes);
        }
        for p in &p.parameters {
            self.ty(&p.ty, b, depth + 1)?;
        }
        self.ty(&p.result, b, depth + 1)?;
        if p.bound_lifetimes != 0 {
            self.bounds.pop();
        }
        self.invocation = invocation;
        Ok(())
    }
    fn ty(&mut self, p: &SemanticTypeRecipe, b: &mut Budget, depth: usize) -> Check<()> {
        self.step(depth, b)?;
        match p {
            SemanticTypeRecipe::Atom(ty) => dynamic_metadata::ty(ty, b)?,
            SemanticTypeRecipe::Exact(p) => {
                dynamic_metadata::ty(&p.ty, b)?;
                if p.classes.len() != p.life.binder_count as usize {
                    return Err(bad("original exact type omits its complete class domain"));
                }
                for p in &p.classes {
                    self.life(p, b)?;
                }
            }
            SemanticTypeRecipe::Formal(p) => {
                if !matches!(self.formal(p, b)?, SemanticFormal::Type { .. }) {
                    return Err(bad("original type selects another formal kind"));
                }
            }
            SemanticTypeRecipe::Environment { scope, ordinal } => {
                if !matches!(
                    self.environment(*scope, *ordinal, b)?,
                    SemanticFormal::Type { .. }
                ) {
                    return Err(bad("original type selects another environment kind"));
                }
            }
            SemanticTypeRecipe::SelfType { scope } => {
                let definition = self.definition(*scope, b)?;
                if !matches!(definition.body, SemanticDefinitionBody::Scope(_)) {
                    return Err(bad("original Self selects a nonowner scope"));
                }
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            }
            | SemanticTypeRecipe::Closure {
                definition,
                arguments,
                ..
            }
            | SemanticTypeRecipe::Opaque {
                definition,
                arguments,
                ..
            } => {
                let program = self
                    .module
                    .semantic
                    .as_ref()
                    .ok_or_else(|| bad("original type program is absent"))?;
                b.spend(program.definitions.len())?;
                let mut rows = program.definitions.iter().filter(|r| r.id == *definition);
                let target = rows
                    .next()
                    .ok_or_else(|| bad("original type target definition absent"))?;
                if rows.next().is_some() || target.parameters.len() != arguments.len() {
                    return Err(bad(
                        "original type target identity/full mixed arity differs",
                    ));
                }
                for (p, f) in arguments.iter().zip(&target.parameters) {
                    self.actual(p, f, b, depth + 1)?;
                }
                if let SemanticTypeRecipe::Opaque { hidden, .. } = p {
                    self.ty(hidden, b, depth + 1)?;
                }
                if let SemanticTypeRecipe::Closure { environment, .. } = p {
                    for (p, f) in environment.iter().zip(&target.environment) {
                        self.actual(p, &f.kind, b, depth + 1)?;
                    }
                    if environment.len() != target.environment.len() {
                        return Err(bad("original closure omits sealed environment domain"));
                    }
                }
            }
            SemanticTypeRecipe::AssociatedType {
                receiver,
                application,
                member,
            } => {
                self.ty(receiver, b, depth + 1)?;
                self.application(application, b, depth + 1)?;
                self.member(application, *member, false, b)?;
            }
            SemanticTypeRecipe::Tuple(ps) | SemanticTypeRecipe::AnonymousSum(ps) => {
                for p in ps {
                    self.ty(p, b, depth + 1)?;
                }
            }
            SemanticTypeRecipe::Array { element, length } => {
                self.ty(element, b, depth + 1)?;
                self.constant(length, b, depth + 1)?;
            }
            SemanticTypeRecipe::Slice(p)
            | SemanticTypeRecipe::Managed(p)
            | SemanticTypeRecipe::OwnedStore(p)
            | SemanticTypeRecipe::Mutex(p)
            | SemanticTypeRecipe::Registration(p)
            | SemanticTypeRecipe::AsyncRegistration(p) => self.ty(p, b, depth + 1)?,
            SemanticTypeRecipe::Address {
                lifetime, inner, ..
            }
            | SemanticTypeRecipe::Desc { lifetime, inner } => {
                if let SemanticOuterLifeRecipe::Class(p) = lifetime {
                    self.life(p, b)?;
                }
                self.ty(inner, b, depth + 1)?;
            }
            SemanticTypeRecipe::Task { result, retained }
            | SemanticTypeRecipe::MutexGuard {
                payload: result,
                retained,
            } => {
                self.ty(result, b, depth + 1)?;
                self.retained_shape(retained, b, depth + 1)?;
            }
            SemanticTypeRecipe::ErrorUnion { error, result } => {
                self.ty(error, b, depth + 1)?;
                self.ty(result, b, depth + 1)?;
            }
            SemanticTypeRecipe::Qualified { storage, life } => {
                self.ty(storage, b, depth + 1)?;
                self.sequence(&life.domain, b, depth + 1)?;
                for p in &life.slots {
                    self.life(&p.class, b)?;
                }
                self.retained(&life.retained, &life.nominal, b, depth + 1)?;
            }
            SemanticTypeRecipe::Code { signature, .. } => {
                self.signature(signature, b, depth + 1)?
            }
            SemanticTypeRecipe::Dyn { interface, .. } => {
                self.application(&interface.application, b, depth + 1)?;
                match &interface.context {
                    SemanticDynContextRecipe::Closed => {}
                    SemanticDynContextRecipe::Type(p) => self.ty(p, b, depth + 1)?,
                    SemanticDynContextRecipe::Shape(p) => self.retained_shape(p, b, depth + 1)?,
                };
                for p in &interface.associated {
                    self.application(&p.application, b, depth + 1)?;
                    match &p.value {
                        SemanticAssociatedRecipe::Type(ty) => {
                            self.member(&p.application, p.member, false, b)?;
                            self.ty(ty, b, depth + 1)?;
                        }
                        SemanticAssociatedRecipe::Const { carrier, value } => {
                            self.member(&p.application, p.member, true, b)?;
                            self.ty(carrier, b, depth + 1)?;
                            self.constant(value, b, depth + 1)?;
                        }
                    }
                }
            }
            SemanticTypeRecipe::OriginalType { scope, .. } => {
                if !matches!(
                    self.definition(*scope, b)?.body,
                    SemanticDefinitionBody::FunctionWithBody { .. }
                ) {
                    return Err(bad(
                        "original inferred type does not select a complete source body",
                    ));
                }
            }
            SemanticTypeRecipe::UnresolvedNamed(_) | SemanticTypeRecipe::Unsupported(_) => {
                return Err(bad("original semantic type retains an unresolved recipe"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bridge_scope_is_selected_from_actual_owner_not_its_self_asserted_scope() {
        let (mut module, _) = crate::tests::baseline();
        let decl = |index| LifeDeclarationId {
            module: module.module_id,
            declaration: zil_base::NodeId(index),
            compiler: false,
        };
        let scope = |index| SemanticDefinitionId {
            module: module.module_id,
            index,
        };
        let definition = |index, declaration| SemanticDefinition {
            id: scope(index),
            identity: LifeDeclarationIdentity {
                package: "bridge".into(),
                module_path: vec![],
                declaration_path: vec![format!("Scope{index}")],
                generation_path: vec![],
            },
            parent: None,
            kind: SemanticDefinitionKind::Scope,
            parameters: vec![],
            implicit_lifetimes: vec![],
            requirements: vec![],
            environment: vec![],
            body: SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { declaration }),
        };
        let wanted = SemanticDynOwner::Declaration(decl(80));
        module.semantic = Some(SemanticModule {
            definitions: vec![definition(0, decl(80)), definition(1, decl(81))],
            instances: vec![],
            sites: vec![],
            dyn_scopes: vec![
                SemanticDynScopeBinding {
                    owner: wanted.clone(),
                    scope: scope(0),
                },
                SemanticDynScopeBinding {
                    owner: SemanticDynOwner::Declaration(decl(81)),
                    scope: scope(1),
                },
            ],
        });
        module.dyn_declarations = (0..2)
            .map(|index| DynDeclaration {
                implicit_lifetimes: Some(vec![]),
                direct_supertraits: Some(vec![]),
                requirements: Some(vec![]),
                associated_definitions: Some(vec![]),
                method_signatures: Some(vec![]),
                method_definitions: Some(vec![]),
                self_pattern: None,
                implemented_trait: None,
                id: decl(80 + index),
                identity: definition(index, decl(80 + index)).identity,
                kind: DynDeclarationKind::Trait,
                parameters: vec![],
                members: vec![],
            })
            .collect();
        let limits = Limits::default();
        assert_eq!(
            owner(&module, &wanted, scope(0), &mut Budget::new(&limits))
                .unwrap()
                .id,
            scope(0)
        );
        assert!(owner(&module, &wanted, scope(1), &mut Budget::new(&limits)).is_err());
        let mut wrong = module.clone();
        wrong.semantic.as_mut().unwrap().dyn_scopes[0].scope = scope(1);
        assert!(owner(&wrong, &wanted, scope(1), &mut Budget::new(&limits)).is_err());
        let mut duplicated = module.clone();
        duplicated
            .semantic
            .as_mut()
            .unwrap()
            .dyn_scopes
            .push(SemanticDynScopeBinding {
                owner: wanted.clone(),
                scope: scope(0),
            });
        assert!(owner(&duplicated, &wanted, scope(0), &mut Budget::new(&limits)).is_err());
        let bytes = zil_ir::wire::encode_module_candidate(
            &module,
            &CandidatePlan::empty_for(&module),
            &limits,
        )
        .unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        assert_eq!(
            owner(&decoded, &wanted, scope(0), &mut Budget::new(&limits))
                .unwrap()
                .id,
            scope(0)
        );
        assert!(owner(&decoded, &wanted, scope(1), &mut Budget::new(&limits)).is_err());
    }
}

/// Enter the shared V evaluator at the independently selected original owner.
/// The caller supplies its existing environment/active graph; no fresh budget,
/// bound frame or invocation namespace is created at this boundary.
#[allow(clippy::too_many_arguments)]
pub(super) fn evaluate(
    module: &TypedModule,
    context: &crate::semantic::CheckedContext,
    expected: &SemanticDynOwner,
    requested: SemanticDefinitionId,
    recipe: &SemanticTypeRecipe,
    environment: &crate::semantic::EvalEnv,
    external: &mut impl crate::semantic::ExternalProofs,
    active: &mut crate::semantic::ActiveQueries,
    b: &mut Budget,
) -> Check<crate::semantic::CheckedType> {
    let selected = owner(module, expected, requested, b)?;
    let checked = context.definition(module, requested, b)?;
    if !std::ptr::eq(selected, checked) {
        return Err(bad(
            "semantic bridge context does not refer to the same actual candidate owner",
        ));
    }
    let first = environment
        .lexical
        .first()
        .ok_or_else(|| bad("semantic bridge lexical environment is absent"))?;
    if first.definition != requested {
        return Err(bad(
            "semantic bridge starts in another original definition domain",
        ));
    }
    b.spend(environment.lexical.len())?;
    let mut parent = Some(requested);
    for frame in &environment.lexical {
        if parent != Some(frame.definition) {
            return Err(bad(
                "semantic bridge lexical ancestor chain is not the actual owner chain",
            ));
        }
        let definition = context.definition(module, frame.definition, b)?;
        match frame.key {
            crate::semantic::ScopeKey::Witness {
                definition: actual, ..
            } if actual == frame.definition => {}
            crate::semantic::ScopeKey::Instance(instance) => {
                if context.instance(module, instance, b)?.definition != frame.definition {
                    return Err(bad(
                        "semantic bridge instance belongs to another lexical definition",
                    ));
                }
            }
            _ => return Err(bad("semantic bridge frame key names another definition")),
        }
        parent = definition.parent;
    }
    if parent.is_some() {
        return Err(bad("semantic bridge omits a real lexical ancestor"));
    }
    context.evaluate_type(module, recipe, environment, external, active, b)
}

/// An executable method uses its real semantic instance, never a same-shaped
/// signature or the first observed caller. This preserves rigid/partition and
/// sealed creator-environment checks owned by the shared V context.
pub(super) fn environment_for_function(
    module: &TypedModule,
    context: &crate::semantic::CheckedContext,
    expected: &SemanticDynOwner,
    requested: SemanticDefinitionId,
    function: FunctionId,
    active: &mut crate::semantic::ActiveQueries,
    b: &mut Budget,
) -> Check<crate::semantic::EvalEnv> {
    owner(module, expected, requested, b)?;
    let program = module
        .semantic
        .as_ref()
        .ok_or_else(|| bad("actual method semantic program is Unknown"))?;
    b.spend(program.instances.len())?;
    let mut instances = program.instances.iter().filter(|instance| {
        instance.definition == requested
            && instance.target
                == SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))
    });
    let instance = instances
        .next()
        .ok_or_else(|| bad("actual method has no exact semantic instance"))?;
    if instances.next().is_some() {
        return Err(bad(
            "actual method has duplicate original semantic instances",
        ));
    }
    context.environment_for_instance(module, instance.id, active, b)
}
