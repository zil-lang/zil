//! Nominal identity and complete field substitution in the original domain.
//! Physical equality never selects a declaration or supplies a lifetime root.
use super::*;

fn frame(definition: SemanticDefinitionId, formals: Vec<Term>) -> Frame {
    Frame {
        definition,
        formals,
        parameters: Vec::new(),
        result: 0,
        locals: Vec::new(),
        inherited: 0,
        alive: true,
        environment: Vec::new(),
        expressions: BTreeMap::new(),
        bindings: BTreeMap::new(),
    }
}

impl Solver<'_, '_> {
    pub(super) fn nominal(
        &mut self,
        context: &Frame,
        definition: SemanticDefinitionId,
        recipes: &[SemanticActualRecipe],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        let declaration = self.inventory.definition(definition, b)?;
        if declaration.parameters.len() != recipes.len() {
            return Err(fail(
                "original nominal application changes its full mixed arity",
            ));
        }
        let mut arguments = Vec::new();
        for (formal, recipe) in declaration.parameters.iter().zip(recipes) {
            let argument = self.actual(context, formal, recipe, b, depth + 1)?;
            Self::push(&mut arguments, argument, b)?;
        }
        self.application(definition, arguments, b, depth + 1)
    }

    pub(super) fn application(
        &mut self,
        definition: SemanticDefinitionId,
        arguments: Vec<Term>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        let declaration = self.inventory.definition(definition, b)?;
        let SemanticDefinitionBody::Nominal { kind, body, .. } = &declaration.body else {
            return Err(fail(
                "original nominal recipe selects a nonnominal declaration",
            ));
        };
        if declaration.parent.is_some()
            || !declaration.environment.is_empty()
            || !declaration.implicit_lifetimes.is_empty()
        {
            return Err(unknown(
                "original nested nominal needs its complete lexical lifetime context",
            ));
        }
        if !matches!(
            (kind, body),
            (
                LifeNominalKind::Record | LifeNominalKind::Package | LifeNominalKind::Resource,
                SemanticNominalBody::Product { .. }
            ) | (LifeNominalKind::Enum, SemanticNominalBody::Enum { .. })
        ) {
            return Err(fail(
                "original nominal kind differs from its original body family",
            ));
        }
        if *kind == LifeNominalKind::Resource {
            return Err(unknown(
                "original resource nominal needs its actual Drop definition and cleanup proof",
            ));
        }
        if arguments.len() != declaration.parameters.len() {
            return Err(fail("original nominal application omits a formal"));
        }
        let context = frame(definition, arguments);
        for formal in &declaration.parameters {
            if let SemanticFormal::Type { requirements } = formal {
                self.premises_well_formed(definition, requirements, b)?;
                self.role_premises_wf(&context, requirements, b)?;
                self.premise_needs(&context, requirements, b)?;
            }
        }
        self.premises_well_formed(definition, &declaration.requirements, b)?;
        self.role_premises_wf(&context, &declaration.requirements, b)?;
        self.premise_needs(&context, &declaration.requirements, b)?;
        let mut fields = Vec::new();
        match body {
            SemanticNominalBody::Product { fields: recipes } => {
                let mut names = BTreeSet::new();
                for field in recipes {
                    b.spend(field.name.len().saturating_add(names.len() + 1), WORK)?;
                    if !names.insert(&field.name) {
                        return Err(fail("original nominal repeats a field name"));
                    }
                    let field = self.recipe(&context, &field.ty, b, depth + 1)?;
                    if matches!(self.state.arena[self.root(field, b)?], Kind::Slice(_)) {
                        return Err(fail(
                            "original nominal field requires a sized value or reference carrier",
                        ));
                    }
                    Self::push(&mut fields, field, b)?;
                }
            }
            SemanticNominalBody::Enum { variants } => {
                let mut names = BTreeSet::new();
                for variant in variants {
                    b.spend(variant.name.len().saturating_add(names.len() + 1), WORK)?;
                    if !names.insert(&variant.name) {
                        return Err(fail("original enum repeats a variant name"));
                    }
                    for field in &variant.fields {
                        let field = self.recipe(&context, field, b, depth + 1)?;
                        if matches!(self.state.arena[self.root(field, b)?], Kind::Slice(_)) {
                            return Err(fail(
                                "original variant field requires a sized value or reference carrier",
                            ));
                        }
                        Self::push(&mut fields, field, b)?;
                    }
                }
            }
        }
        self.term(
            Kind::Nominal {
                definition,
                arguments: context.formals,
                fields,
            },
            b,
        )
    }

    pub(super) fn record_fields(
        &mut self,
        context: &mut Frame,
        term: Term,
        values: &[(String, SemanticOriginalExpr)],
        complete: bool,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        let term = self.root(term, b)?;
        let Kind::Nominal {
            definition, fields, ..
        } = &self.state.arena[term]
        else {
            return Err(fail(
                "original record construction has no declared nominal owner",
            ));
        };
        let declaration = self.inventory.definition(*definition, b)?;
        let SemanticDefinitionBody::Nominal {
            body: SemanticNominalBody::Product { fields: recipes },
            ..
        } = &declaration.body
        else {
            return Err(fail(
                "original record construction selects a non-product declaration",
            ));
        };
        b.spend(fields.len().saturating_add(values.len()), WORK)?;
        let fields = fields.clone();
        if complete && values.len() != recipes.len() {
            return Err(fail("original record construction omits a field"));
        }
        let mut seen = BTreeSet::new();
        for (name, value) in values {
            b.spend(name.len().saturating_add(seen.len() + 1), WORK)?;
            if !seen.insert(name) {
                return Err(fail("original record construction repeats a field"));
            }
            let mut selected = None;
            for (index, field) in recipes.iter().enumerate() {
                b.spend(field.name.len().saturating_add(name.len() + 1), WORK)?;
                if field.name == *name {
                    selected = Some(index);
                }
            }
            let index = selected.ok_or_else(|| {
                fail("original record field is absent from its nominal declaration")
            })?;
            let actual = self.expression(context, value, b, depth + 1)?;
            self.unify(actual, fields[index], depth + 1, b)?;
            if !complete {
                Self::push(
                    &mut self.needs,
                    Need {
                        term: fields[index],
                        predicate: Predicate::SyncDrop,
                    },
                    b,
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn member(
        &mut self,
        term: Term,
        name: &str,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        let root = self.type_root(term, b)?;
        if let Kind::Tuple(fields) = &self.state.arena[root] {
            b.spend(name.len().saturating_add(fields.len() + 1), WORK)?;
            if name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(fail(
                    "original tuple selection is not an exact positional member",
                ));
            }
            let ordinal = name
                .parse::<usize>()
                .map_err(|_| fail("original tuple member ordinal is invalid"))?;
            let selected = *fields
                .get(ordinal)
                .ok_or_else(|| fail("original tuple member exceeds its complete arity"))?;
            for (index, field) in fields.iter().enumerate() {
                if index != ordinal {
                    Self::push(
                        &mut self.needs,
                        Need {
                            term: *field,
                            predicate: Predicate::SyncDrop,
                        },
                        b,
                    )?;
                }
            }
            return Ok(selected);
        }
        let Kind::Nominal {
            definition, fields, ..
        } = &self.state.arena[root]
        else {
            return Err(unknown(
                "original field selection needs its complete nominal or borrowed place type",
            ));
        };
        let declaration = self.inventory.definition(*definition, b)?;
        let SemanticDefinitionBody::Nominal {
            body: SemanticNominalBody::Product { fields: recipes },
            ..
        } = &declaration.body
        else {
            return Err(fail("original field selection does not select a product"));
        };
        let mut selected = None;
        for (index, field) in recipes.iter().enumerate() {
            b.spend(name.len().saturating_add(field.name.len() + 1), WORK)?;
            if field.name == name {
                selected = Some(index);
            }
        }
        let selected = selected
            .ok_or_else(|| fail("original field is absent from its exact nominal declaration"))?;
        b.spend(fields.len(), WORK)?;
        let result = fields[selected];
        // Selection consumes this whole value. Every remaining owning field
        // still has a real cleanup obligation; a selected field is no Copy proof.
        for (index, field) in fields.iter().enumerate() {
            if index != selected {
                Self::push(
                    &mut self.needs,
                    Need {
                        term: *field,
                        predicate: Predicate::SyncDrop,
                    },
                    b,
                )?;
            }
        }
        Ok(result)
    }
}

pub(super) fn materialize(
    definition: SemanticDefinitionId,
    arguments: &[Type],
    physical: &[Type],
    inventory: &Inventory<'_>,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<Type> {
    b.spend(1, WORK)?;
    if depth > b.partial_depth {
        return Err(resource(
            "original nominal materialization exceeds its finite depth",
        ));
    }
    let original = inventory.definition(definition, b)?;
    let SemanticDefinitionBody::Nominal {
        declaration,
        kind,
        body,
    } = &original.body
    else {
        return Err(fail(
            "actual nominal selects a nonnominal original definition",
        ));
    };
    b.spend(inventory.module.life_declarations.len(), WORK)?;
    let mut declared = None;
    for item in &inventory.module.life_declarations {
        if item.id == *declaration && declared.replace(item).is_some() {
            return Err(fail("actual nominal repeats its declaration identity"));
        }
    }
    let declared =
        declared.ok_or_else(|| fail("actual nominal lacks its original declaration inventory"))?;
    if declared.identity != original.identity
        || declared.kind != *kind
        || declared.parameters.len() != original.parameters.len()
        || arguments.len() != original.parameters.len()
    {
        return Err(fail(
            "actual nominal changes its original identity, kind or full arity",
        ));
    }
    for (formal, kind) in original.parameters.iter().zip(&declared.parameters) {
        b.spend(1, WORK)?;
        if !matches!(
            (formal, kind),
            (SemanticFormal::Type { .. }, LifeParameterKind::Type)
        ) {
            return Err(unknown(
                "actual nominal needs its complete original constant and lifetime arguments",
            ));
        }
    }
    let mut found = None;
    for instance in &inventory.module.life_instances {
        b.spend(
            instance.arguments.len().saturating_add(arguments.len() + 1),
            WORK,
        )?;
        if instance.declaration != *declaration || instance.arguments.len() != arguments.len() {
            continue;
        }
        let mut equal = true;
        for (actual, expected) in instance.arguments.iter().zip(arguments) {
            crate::dyn_metadata::ty(expected, b)?;
            match actual {
                LifeNominalActual::Type(ty) => {
                    crate::dyn_metadata::ty(ty, b)?;
                    equal &= ty == expected;
                }
                LifeNominalActual::QualifiedType { ty, life } => {
                    crate::dyn_metadata::ty(ty, b)?;
                    equal &= ty == expected
                        && life.binder_count == 0
                        && life.slots.is_empty()
                        && life.retained.is_empty()
                        && life.nominal_arguments.is_empty();
                }
                _ => equal = false,
            }
        }
        if equal && found.replace(instance).is_some() {
            return Err(fail(
                "original nominal has multiple actual instance identities",
            ));
        }
    }
    let instance =
        found.ok_or_else(|| fail("original nominal has no exact complete actual instance"))?;
    crate::stored_flow::validate_plain_nominal_zero(
        inventory.module,
        &instance.ty,
        inventory.limits,
        b,
    )?;
    crate::dyn_metadata::ty(&instance.ty, b)?;
    let storage = match &instance.ty {
        Type::BorrowedStorage { storage, .. } => storage.as_ref(),
        ty => ty,
    };
    match (kind, body, storage) {
        (
            LifeNominalKind::Record,
            SemanticNominalBody::Product { fields },
            Type::Record { fields: actual, .. },
        )
        | (
            LifeNominalKind::Package,
            SemanticNominalBody::Product { fields },
            Type::Package { fields: actual, .. },
        ) => {
            if fields.len() != actual.len() || physical.len() != fields.len() {
                return Err(fail(
                    "actual nominal changes its complete product field count",
                ));
            }
            for ((source, (name, ty)), expected) in fields.iter().zip(actual).zip(physical) {
                b.spend(source.name.len().saturating_add(name.len() + 1), WORK)?;
                crate::dyn_metadata::ty(ty, b)?;
                crate::dyn_metadata::ty(expected, b)?;
                if source.name != *name || ty != expected {
                    return Err(fail(
                        "actual nominal product differs from original field substitution",
                    ));
                }
            }
        }
        (
            LifeNominalKind::Enum,
            SemanticNominalBody::Enum { variants },
            Type::Enum {
                variants: actual, ..
            },
        ) => {
            if variants.len() != actual.len() {
                return Err(fail("actual nominal changes its full variant directory"));
            }
            let mut offset = 0usize;
            for (source, (name, fields)) in variants.iter().zip(actual) {
                b.spend(
                    source
                        .name
                        .len()
                        .saturating_add(name.len() + fields.len() + 1),
                    WORK,
                )?;
                if source.name != *name || source.fields.len() != fields.len() {
                    return Err(fail("actual nominal changes an original variant"));
                }
                for ty in fields {
                    let expected = physical
                        .get(offset)
                        .ok_or_else(|| fail("actual nominal lacks an original payload field"))?;
                    crate::dyn_metadata::ty(ty, b)?;
                    crate::dyn_metadata::ty(expected, b)?;
                    if ty != expected {
                        return Err(fail(
                            "actual nominal enum differs from original payload substitution",
                        ));
                    }
                    offset = offset
                        .checked_add(1)
                        .ok_or_else(|| resource("original variant field count overflow"))?;
                }
            }
            if offset != physical.len() {
                return Err(fail("original nominal has extra physical payload fields"));
            }
        }
        _ => {
            return Err(unknown(
                "actual nominal kind requires its complete original ownership and cleanup proof",
            ));
        }
    }
    Ok(instance.ty.clone())
}

/// Evaluate fields from their original nominal declaration with a complete
/// actual vector. This is substitution, not inference from a physical layout.
pub(in crate::semantic) fn actual(
    definition: SemanticDefinitionId,
    arguments: &[Type],
    inventory: &Inventory<'_>,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<Type> {
    b.spend(1, WORK)?;
    if depth > b.partial_depth {
        return Err(resource(
            "original nominal substitution exceeds its finite depth",
        ));
    }
    let declaration = inventory.definition(definition, b)?;
    let SemanticDefinitionBody::Nominal { kind, body, .. } = &declaration.body else {
        return Err(fail(
            "nominal actual selects a different original definition kind",
        ));
    };
    if declaration.parent.is_some()
        || !declaration.environment.is_empty()
        || !declaration.implicit_lifetimes.is_empty()
        || *kind == LifeNominalKind::Resource
    {
        return Err(unknown(
            "nominal actual needs its full lexical, lifetime and Drop definition proof",
        ));
    }
    if arguments.len() != declaration.parameters.len() {
        return Err(fail("nominal actual omits full original mixed formals"));
    }
    for (formal, argument) in declaration.parameters.iter().zip(arguments) {
        let SemanticFormal::Type { requirements } = formal else {
            return Err(unknown(
                "nominal actual needs scoped constant and lifetime substitution",
            ));
        };
        crate::stored_flow::validate_plain_nominal_zero(
            inventory.module,
            argument,
            inventory.limits,
            b,
        )?;
        requirements_actual(definition, arguments, requirements, inventory, b, depth + 1)?;
    }
    requirements_actual(
        definition,
        arguments,
        &declaration.requirements,
        inventory,
        b,
        depth + 1,
    )?;
    let mut physical = Vec::new();
    match body {
        SemanticNominalBody::Product { fields } => {
            for field in fields {
                let ty = substitute(definition, arguments, &field.ty, inventory, b, depth + 1)?;
                Solver::push(&mut physical, ty, b)?;
            }
        }
        SemanticNominalBody::Enum { variants } => {
            for variant in variants {
                for field in &variant.fields {
                    let ty = substitute(definition, arguments, field, inventory, b, depth + 1)?;
                    Solver::push(&mut physical, ty, b)?;
                }
            }
        }
    }
    materialize(definition, arguments, &physical, inventory, b, depth + 1)
}

fn substitute(
    owner: SemanticDefinitionId,
    arguments: &[Type],
    recipe: &SemanticTypeRecipe,
    inventory: &Inventory<'_>,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<Type> {
    b.spend(1, WORK)?;
    if depth > b.partial_depth {
        return Err(resource(
            "original field substitution exceeds its finite depth",
        ));
    }
    match recipe {
        SemanticTypeRecipe::Atom(ty) if ty.is_b0_scalar() => Ok(ty.clone()),
        SemanticTypeRecipe::Formal(SemanticFormalRef {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal,
        }) => {
            let declaration = inventory.definition(owner, b)?;
            if !matches!(
                declaration.parameters.get(*ordinal as usize),
                Some(SemanticFormal::Type { .. })
            ) {
                return Err(fail("original field Type changes its full formal kind"));
            }
            let ty = arguments
                .get(*ordinal as usize)
                .ok_or_else(|| fail("original field Type omits a nominal actual"))?;
            crate::dyn_metadata::ty(ty, b)?;
            Ok(ty.clone())
        }
        SemanticTypeRecipe::Tuple(fields) => {
            if fields.is_empty() {
                return Ok(Type::Unit);
            }
            let mut actuals = Vec::new();
            for field in fields {
                let ty = substitute(owner, arguments, field, inventory, b, depth + 1)?;
                Solver::push(&mut actuals, ty, b)?;
            }
            Ok(Type::Tuple(actuals))
        }
        SemanticTypeRecipe::Nominal {
            definition,
            arguments: recipes,
        } => {
            let mut actuals = Vec::new();
            for recipe in recipes {
                let SemanticActualRecipe::Type(recipe) = recipe else {
                    return Err(unknown(
                        "nested nominal field needs complete mixed substitution",
                    ));
                };
                let ty = substitute(owner, arguments, recipe, inventory, b, depth + 1)?;
                Solver::push(&mut actuals, ty, b)?;
            }
            actual(*definition, &actuals, inventory, b, depth + 1)
        }
        SemanticTypeRecipe::Exact(value)
            if value.ty.is_b0_scalar()
                && value.life.binder_count == 0
                && value.life.slots.is_empty()
                && value.life.retained.is_empty()
                && value.life.nominal_arguments.is_empty()
                && value.classes.is_empty() =>
        {
            Ok(value.ty.clone())
        }
        _ => Err(unknown(
            "original nominal field requires its complete scoped type constructor proof",
        )),
    }
}

fn requirements_actual(
    owner: SemanticDefinitionId,
    arguments: &[Type],
    requirements: &[SemanticRequirementRecipe],
    inventory: &Inventory<'_>,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<()> {
    for requirement in requirements {
        b.spend(1, WORK)?;
        let (recipe, predicate) = match requirement {
            SemanticRequirementRecipe::Structural { ty, bound } => (
                ty,
                match bound {
                    DynStructuralBound::Copy => Predicate::Copy,
                    DynStructuralBound::Value => Predicate::Value,
                    DynStructuralBound::SyncDrop => Predicate::SyncDrop,
                    DynStructuralBound::StoreElement => Predicate::StoreElement,
                },
            ),
            SemanticRequirementRecipe::Builtin { ty, trait_kind } => (
                ty,
                match trait_kind {
                    SemanticBuiltinTrait::Copy => Predicate::Copy,
                    SemanticBuiltinTrait::Value => Predicate::Value,
                    SemanticBuiltinTrait::SyncDrop => Predicate::SyncDrop,
                    SemanticBuiltinTrait::StoreElement => Predicate::StoreElement,
                    _ => {
                        return Err(unknown(
                            "nominal actual sealed provider requires its complete execution proof",
                        ));
                    }
                },
            ),
            _ => {
                return Err(unknown(
                    "nominal actual needs its complete original nonstructural premise proof",
                ));
            }
        };
        let ty = substitute(owner, arguments, recipe, inventory, b, depth + 1)?;
        if !predicate_actual(&ty, predicate, inventory, b, depth + 1)? {
            return Err(fail("nominal actual fails an original structural premise"));
        }
    }
    Ok(())
}

pub(in crate::semantic) fn predicate_actual(
    ty: &Type,
    predicate: Predicate,
    inventory: &Inventory<'_>,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<bool> {
    b.spend(1, WORK)?;
    if depth > b.partial_depth {
        return Err(resource(
            "nominal structural predicate exceeds its finite depth",
        ));
    }
    if matches!(predicate, Predicate::Capability(_)) {
        return Err(unknown(
            "actual sealed capability requires its complete original environment, provider and execution proof",
        ));
    }
    if ty.is_b0_scalar() || *ty == Type::ForeignHandle {
        return Ok(!matches!(predicate, Predicate::StoreElement) || *ty != Type::Unit);
    }
    let complete = ty;
    let ty = match ty {
        Type::BorrowedStorage { storage, life }
            if life.binder_count == 0
                && life.slots.is_empty()
                && life.retained.is_empty()
                && life.nominal_arguments.is_empty() =>
        {
            crate::stored_flow::validate_plain_nominal_zero(
                inventory.module,
                complete,
                inventory.limits,
                b,
            )?;
            storage.as_ref()
        }
        _ => ty,
    };
    match ty {
        Type::Tuple(fields) => {
            if matches!(predicate, Predicate::StoreElement) {
                return Err(unknown(
                    "product StoreElement needs complete actual layout qualification",
                ));
            }
            for field in fields {
                if !predicate_actual(field, predicate, inventory, b, depth + 1)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            crate::stored_flow::validate_plain_nominal_zero(
                inventory.module,
                ty,
                inventory.limits,
                b,
            )?;
            if matches!(predicate, Predicate::StoreElement) {
                return Err(unknown(
                    "nominal StoreElement needs complete actual layout qualification",
                ));
            }
            if matches!(ty, Type::Package { .. }) && matches!(predicate, Predicate::Copy) {
                return Ok(false);
            }
            for (_, field) in fields {
                if !predicate_actual(field, predicate, inventory, b, depth + 1)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Type::Enum { variants, .. } => {
            crate::stored_flow::validate_plain_nominal_zero(
                inventory.module,
                ty,
                inventory.limits,
                b,
            )?;
            if matches!(predicate, Predicate::StoreElement) {
                return Err(unknown(
                    "enum StoreElement needs complete actual layout qualification",
                ));
            }
            for (_, fields) in variants {
                for field in fields {
                    if !predicate_actual(field, predicate, inventory, b, depth + 1)? {
                        return Ok(false);
                    }
                }
            }
            Ok(true)
        }
        _ => Err(unknown(
            "structural predicate needs its complete actual ownership and cleanup proof",
        )),
    }
}

pub(in crate::semantic) fn validate_actual_type(
    ty: &Type,
    inventory: &Inventory<'_>,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<()> {
    crate::stored_flow::validate_plain_nominal_zero(inventory.module, ty, inventory.limits, b)?;
    let mut instance = None;
    for item in &inventory.module.life_instances {
        crate::dyn_metadata::ty(&item.ty, b)?;
        crate::dyn_metadata::ty(ty, b)?;
        if item.ty == *ty && instance.replace(item).is_some() {
            return Err(fail("actual nominal repeats an exact type catalogue entry"));
        }
    }
    let instance =
        instance.ok_or_else(|| fail("actual nominal has no exact original catalogue entry"))?;
    let mut definition = None;
    for item in inventory.definitions.values() {
        b.spend(1, WORK)?;
        if matches!(&item.body,SemanticDefinitionBody::Nominal {declaration,..} if *declaration==instance.declaration)
            && definition.replace(item.id).is_some()
        {
            return Err(fail(
                "actual nominal has ambiguous original declaration definitions",
            ));
        }
    }
    let definition =
        definition.ok_or_else(|| fail("actual nominal lacks its complete original definition"))?;
    let mut arguments = Vec::new();
    for argument in &instance.arguments {
        let ty = match argument {
            LifeNominalActual::Type(ty) => ty,
            LifeNominalActual::QualifiedType { ty, life }
                if life.binder_count == 0
                    && life.slots.is_empty()
                    && life.retained.is_empty()
                    && life.nominal_arguments.is_empty() =>
            {
                ty
            }
            _ => {
                return Err(unknown(
                    "actual nominal catalogue requires complete nonzero or constant domain substitution",
                ));
            }
        };
        crate::dyn_metadata::ty(ty, b)?;
        Solver::push(&mut arguments, ty.clone(), b)?;
    }
    if actual(definition, &arguments, inventory, b, depth + 1)? != *ty {
        return Err(fail(
            "actual nominal loses its original complete substitution",
        ));
    }
    Ok(())
}
