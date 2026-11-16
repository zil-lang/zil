//! Nominal identity and complete field substitution in the original domain.
//! Physical equality never selects a declaration or supplies a lifetime root.
use super::*;

pub(super) fn frame(definition: SemanticDefinitionId, formals: Vec<Term>) -> Frame {
    Frame {
        definition,
        formals,
        parameters: Vec::new(),
        result: 0,
        locals: Vec::new(),
        inherited: 0,
        captured: BTreeMap::new(),
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
            resources::drop_method(definition, self.inventory, b)?;
        } else {
            resources::ordinary_drop_absent(definition, self.inventory, b)?;
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
        // Canonical compiler carriers are parameterized schemas, not user
        // declarations asserting that their unbounded formal already has a
        // finite inline layout. Authenticate the entire sealed template and
        // defer only its own rigid placeholder formation. Every application
        // with actual arguments retains the ordinary field obligations.
        b.spend(context.formals.len(), WORK)?;
        let rigid_template = context.formals.iter().enumerate().all(|(ordinal, term)| {
            matches!(self.state.arena[*term],Kind::Rigid(owner,index)
                if owner==definition && index as usize==ordinal)
        });
        let canonical_template = if rigid_template {
            let SemanticDefinitionBody::Nominal {
                declaration: id, ..
            } = &declaration.body
            else {
                return Err(fail("nominal schema loses its original declaration"));
            };
            if *id == zil_ir::builtin_life::POLL {
                self.inventory.protocol_poll_definition(b)? == definition
            } else if *id == zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD
                || *id == zil_ir::builtin_life::TASK_CREATE_FAILURE_PACKAGE
            {
                self.inventory.protocol_task_definition(*id, b)? == definition
            } else {
                false
            }
        } else {
            false
        };
        for (index, field) in fields.iter().enumerate() {
            // A declaration preserves conditional inline formation. Its
            // fields are checked under every real application in the caller's
            // original Gamma; the scheme does not grant Value to its formals.
            let conditional = if let Some(owner) = self.nominal_declaration {
                self.conditional_field(*field, owner, b, depth + 1)?
            } else {
                false
            };
            if canonical_template {
                continue;
            }
            if conditional && let Some(owner) = self.nominal_declaration {
                self.conditional_known_fields(*field, owner, b, depth + 1)?;
                continue;
            }
            Self::push(
                &mut self.field_needs,
                (
                    *field,
                    definition,
                    u32::try_from(index)
                        .map_err(|_| resource("original field ordinal overflow"))?,
                ),
                b,
            )?;
            Self::push(
                &mut self.needs,
                Need {
                    term: *field,
                    predicate: Predicate::Value,
                },
                b,
            )?;
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

    fn conditional_known_fields(
        &mut self,
        term: Term,
        owner: SemanticDefinitionId,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        let root = self.type_root(term, b)?;
        let children = match &self.state.arena[root] {
            Kind::Tuple(fields) | Kind::Nominal { fields, .. } => {
                b.spend(fields.len(), WORK)?;
                fields.clone()
            }
            Kind::Array { element, .. } => vec![*element],
            Kind::ErrorUnion { error, result } => vec![*error, *result],
            // Pointer and manager formation has its own existing complete
            // boundary; an unknown pointee does not make its carrier non-Value.
            _ => return Ok(()),
        };
        for child in children {
            if self.conditional_field(child, owner, b, depth + 1)? {
                self.conditional_known_fields(child, owner, b, depth + 1)?;
            } else {
                Self::push(
                    &mut self.needs,
                    Need {
                        term: child,
                        predicate: Predicate::Value,
                    },
                    b,
                )?;
            }
        }
        Ok(())
    }

    fn conditional_field(
        &self,
        term: Term,
        owner: SemanticDefinitionId,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        match &self.state.arena[self.type_root(term, b)?] {
            Kind::Rigid(scope, _) | Kind::SelfType(scope) => Ok(*scope == owner),
            Kind::Wrapped(_, inner)
            | Kind::Slice(inner)
            | Kind::Address { inner, .. }
            | Kind::Desc { inner, .. } => self.conditional_field(*inner, owner, b, depth + 1),
            Kind::Array { element, .. } => self.conditional_field(*element, owner, b, depth + 1),
            Kind::ErrorUnion { error, result } => {
                Ok(self.conditional_field(*error, owner, b, depth + 1)?
                    || self.conditional_field(*result, owner, b, depth + 1)?)
            }
            Kind::Tuple(fields) | Kind::Nominal { fields, .. } => {
                for field in fields {
                    if self.conditional_field(*field, owner, b, depth + 1)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Kind::Associated { receiver, .. } => {
                self.conditional_field(*receiver, owner, b, depth + 1)
            }
            _ => Ok(false),
        }
    }

    fn record_constructor_template(
        &mut self,
        caller: &Frame,
        node: NodeId,
        definition: SemanticDefinitionId,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        b.lookup(self.constructor_templates.len(), WORK)?;
        if let Some(term) = self.constructor_templates.get(&(caller.definition, node)) {
            if !matches!(self.state.arena[self.root(*term, b)?], Kind::Nominal { definition: actual, .. } if actual == definition)
            {
                return Err(fail(
                    "original constructor node changes its exact nominal declaration",
                ));
            }
            return Ok(*term);
        }
        let declaration = self.inventory.definition(definition, b)?;
        if !matches!(
            declaration.body,
            SemanticDefinitionBody::Nominal {
                body: SemanticNominalBody::Product { .. },
                ..
            }
        ) {
            return Err(fail(
                "original record constructor selects a non-product declaration",
            ));
        }
        let mut actuals = Vec::new();
        for formal in &declaration.parameters {
            let actual =
                match formal {
                    SemanticFormal::Lifetime => self.call_variable(
                        call_regions::Role::ConstructorLife(u32::try_from(actuals.len()).map_err(
                            |_| resource("original constructor mixed ordinal overflows"),
                        )?),
                        b,
                    )?,
                    SemanticFormal::Type { .. } | SemanticFormal::Const { .. } => self.var(b)?,
                };
            Self::push(&mut actuals, actual, b)?;
        }
        b.spend(actuals.len(), WORK)?;
        let context = frame(definition, actuals.clone());
        for (formal, actual) in declaration.parameters.iter().zip(&actuals) {
            if let SemanticFormal::Const { carrier } = formal {
                let carrier = self.recipe(&context, carrier, b, depth + 1)?;
                b.lookup(self.constructor_constants.len(), WORK)?;
                if self
                    .constructor_constants
                    .insert(*actual, carrier)
                    .is_some()
                {
                    return Err(fail(
                        "original constructor repeats a Const inference coordinate",
                    ));
                }
            }
        }
        // The ordinary application checks the full declaration, all field
        // recipes and where/WF obligations. Variables are solved only from
        // genuine wanted/field constraints; they grant no nominal premise.
        let term = self.application(definition, actuals, b, depth + 1)?;
        b.lookup(self.constructor_templates.len(), WORK)?;
        self.constructor_templates
            .insert((caller.definition, node), term);
        Ok(term)
    }

    fn constructor_field_indices(
        &self,
        term: Term,
        values: &[(String, SemanticOriginalExpr)],
        b: &mut Budget,
    ) -> CompileResult<Vec<usize>> {
        let Kind::Nominal {
            definition, fields, ..
        } = &self.state.arena[self.root(term, b)?]
        else {
            return Err(fail(
                "original constructor has no complete nominal field domain",
            ));
        };
        let SemanticDefinitionBody::Nominal {
            body: SemanticNominalBody::Product { fields: recipes },
            ..
        } = &self.inventory.definition(*definition, b)?.body
        else {
            return Err(fail(
                "original record constructor selects a non-product declaration",
            ));
        };
        if recipes.len() != values.len() || fields.len() != recipes.len() {
            return Err(fail("original record construction omits a field"));
        }
        let mut seen = BTreeSet::new();
        let mut indices = Vec::new();
        for (name, _) in values {
            b.spend(name.len().saturating_add(1), WORK)?;
            b.lookup(seen.len(), WORK)?;
            if !seen.insert(name) {
                return Err(fail("original record construction repeats a field"));
            }
            let mut found = None;
            for (ordinal, field) in recipes.iter().enumerate() {
                b.spend(field.name.len().saturating_add(name.len() + 1), WORK)?;
                if field.name == *name && found.replace(ordinal).is_some() {
                    return Err(fail("original nominal repeats a field name"));
                }
            }
            Self::push(
                &mut indices,
                found.ok_or_else(|| {
                    fail("original record field is absent from its nominal declaration")
                })?,
                b,
            )?;
        }
        Ok(indices)
    }

    pub(super) fn record_constructor_hint(
        &mut self,
        caller: &Frame,
        node: NodeId,
        definition: Option<SemanticDefinitionId>,
        values: &[(String, SemanticOriginalExpr)],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Term>> {
        self.step(depth, b)?;
        let Some(definition) = definition else {
            return Ok(None);
        };
        let term = self.record_constructor_template(caller, node, definition, b, depth + 1)?;
        let indices = self.constructor_field_indices(term, values, b)?;
        let Kind::Nominal { fields, .. } = &self.state.arena[self.root(term, b)?] else {
            unreachable!()
        };
        b.spend(fields.len(), WORK)?;
        let fields = fields.clone();
        // Pure Type hints never evaluate, borrow or consume a field. Lifetime
        // and Const equations remain for the full ordered original values.
        for ((_, value), index) in values.iter().zip(indices) {
            if let Some(actual) = self.lexical_input_hint(caller, value, b, depth + 1)? {
                self.input_hint(fields[index], actual, b, depth + 1)?;
            }
        }
        if self.resource_payload(term, b)? {
            Ok(Some(self.term(Kind::Wrapped(Wrapper::Managed, term), b)?))
        } else {
            Ok(Some(term))
        }
    }

    pub(super) fn record_constructor(
        &mut self,
        caller: &mut Frame,
        node: NodeId,
        definition: Option<SemanticDefinitionId>,
        values: &[(String, SemanticOriginalExpr)],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        let wanted = self.receiving_for(caller, node, b)?;
        let term = if let Some(definition) = definition {
            self.record_constructor_template(caller, node, definition, b, depth + 1)?
        } else {
            let wanted = wanted.ok_or_else(|| {
                fail("anonymous original record constructor has no complete wanted nominal type")
            })?;
            match self.state.arena[self.type_root(wanted, b)?] {
                Kind::Wrapped(Wrapper::Managed, inner) => inner,
                _ => wanted,
            }
        };
        let output = if self.resource_payload(term, b)? {
            self.term(Kind::Wrapped(Wrapper::Managed, term), b)?
        } else {
            term
        };
        if let Some(wanted) = wanted {
            self.unify(output, wanted, depth + 1, b)?;
        }
        let indices = self.constructor_field_indices(term, values, b)?;
        let Kind::Nominal { fields, .. } = &self.state.arena[self.root(term, b)?] else {
            unreachable!()
        };
        b.spend(fields.len(), WORK)?;
        let fields = fields.clone();
        let Kind::Nominal {
            definition: declaration,
            arguments,
            ..
        } = &self.state.arena[self.root(term, b)?]
        else {
            unreachable!()
        };
        let declaration = *declaration;
        b.spend(arguments.len(), WORK)?;
        let arguments = arguments.clone();
        let original = self.inventory.definition(declaration, b)?;
        let SemanticDefinitionBody::Nominal {
            body: SemanticNominalBody::Product { fields: recipes },
            ..
        } = &self.inventory.definition(declaration, b)?.body
        else {
            unreachable!()
        };
        for index in &indices {
            self.mark_call_parameter(fields[*index], &recipes[*index].ty, b)?;
        }
        let mut actuals = Vec::new();
        for ((_, expression), index) in values.iter().zip(&indices) {
            // Outer sigma is evaluated once and remains unchanged. Complete
            // inner nominal/Type/Const equations are enforced by collection;
            // unresolved outer roles are settled only after every real field.
            let actual = self.call_input_value(caller, expression, fields[*index], b, depth + 1)?;
            Self::push(&mut actuals, actual, b)?;
        }
        let mut inputs = Vec::new();
        for (((_, expression), actual), index) in values.iter().zip(actuals).zip(indices) {
            if let Some(mut input) = self.collect_call_input(
                caller,
                expression,
                actual,
                fields[index],
                index,
                b,
                depth + 1,
            )? {
                for (ordinal, (formal, argument)) in
                    original.parameters.iter().zip(&arguments).enumerate()
                {
                    b.spend(1, WORK)?;
                    if matches!(formal, SemanticFormal::Lifetime)
                        && self.root(*argument, b)? == self.root(input.life, b)?
                    {
                        input.role = Some(call_regions::Role::ConstructorLife(
                            u32::try_from(ordinal).map_err(|_| {
                                resource("original constructor mixed ordinal overflows")
                            })?,
                        ));
                        break;
                    }
                }
                Self::push(&mut inputs, input, b)?;
            }
        }
        self.settle_call_inputs(caller, node, Some(declaration), &inputs, b, depth + 1)?;
        Ok(output)
    }

    fn constructor_complete(
        &self,
        term: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        let term = self.root(term, b)?;
        let children: Vec<Term> = match &self.state.arena[term] {
            Kind::Infer(None) | Kind::LifeVariable(None) => return Ok(false),
            Kind::Tuple(fields) => {
                b.spend(fields.len(), WORK)?;
                fields.clone()
            }
            Kind::Nominal {
                arguments, fields, ..
            } => {
                b.spend(arguments.len().saturating_add(fields.len()), WORK)?;
                arguments.iter().chain(fields).copied().collect()
            }
            Kind::Array { element, length } => vec![*element, *length],
            Kind::ErrorUnion { error, result } => vec![*error, *result],
            Kind::Address { life, inner, .. }
            | Kind::Desc { life, inner }
            | Kind::MutexGuard {
                life,
                payload: inner,
            } => vec![*life, *inner],
            Kind::Wrapped(_, inner)
            | Kind::Slice(inner)
            | Kind::CallResult { hidden: inner, .. }
            | Kind::ConstRigid { carrier: inner, .. } => vec![*inner],
            Kind::Code {
                parameters, result, ..
            } => {
                b.spend(parameters.len().saturating_add(1), WORK)?;
                parameters
                    .iter()
                    .chain(std::iter::once(result))
                    .copied()
                    .collect()
            }
            Kind::Closure {
                parameters,
                result,
                environment,
                ..
            } => {
                b.spend(parameters.len().saturating_add(environment.len() + 1), WORK)?;
                parameters
                    .iter()
                    .chain(std::iter::once(result))
                    .chain(environment)
                    .copied()
                    .collect()
            }
            Kind::Associated {
                receiver,
                arguments,
                ..
            } => {
                b.spend(arguments.len().saturating_add(1), WORK)?;
                std::iter::once(*receiver)
                    .chain(arguments.iter().copied())
                    .collect()
            }
            Kind::Task(task) => {
                b.spend(task.work(), WORK)?;
                task.terms().collect()
            }
            Kind::Dyn(ty) => {
                b.spend(ty.work(), WORK)?;
                ty.terms().collect()
            }
            Kind::Atom(_)
            | Kind::Rigid(_, _)
            | Kind::SelfType(_)
            | Kind::Life(_)
            | Kind::ConstValue(_) => return Ok(true),
            Kind::Infer(Some(_)) | Kind::LifeVariable(Some(_)) => unreachable!(),
        };
        b.spend(children.len(), WORK)?;
        for child in children {
            if !self.constructor_complete(child, b, depth + 1)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn check_record_constructor_actuals(&mut self, b: &mut Budget) -> CompileResult<()> {
        b.spend(self.constructor_templates.len(), WORK)?;
        let constructors: Vec<Term> = self.constructor_templates.values().copied().collect();
        for term in constructors {
            let Kind::Nominal {
                definition,
                arguments,
                ..
            } = &self.state.arena[self.root(term, b)?]
            else {
                return Err(fail(
                    "original constructor loses its full nominal application",
                ));
            };
            let declaration = self.inventory.definition(*definition, b)?;
            b.spend(arguments.len().saturating_mul(2), WORK)?;
            let arguments = arguments.clone();
            let context = frame(*definition, arguments.clone());
            for (formal, actual) in declaration.parameters.iter().zip(arguments) {
                let root = self.root(actual, b)?;
                let valid = match formal {
                    SemanticFormal::Lifetime => matches!(self.state.arena[root], Kind::Life(_)),
                    SemanticFormal::Type { .. } => !matches!(
                        self.state.arena[root],
                        Kind::Life(_)
                            | Kind::LifeVariable(_)
                            | Kind::ConstRigid { .. }
                            | Kind::ConstValue(_)
                            | Kind::Infer(_)
                    ),
                    SemanticFormal::Const { carrier } => {
                        let carrier = self.recipe(&context, carrier, b, 0)?;
                        let actual_carrier = match self.state.arena[root] {
                            Kind::ConstRigid { carrier, .. } => carrier,
                            Kind::ConstValue(ref value) => self.atom(value.ty(), b)?,
                            _ => {
                                return Err(fail(
                                    "original constructor Const actual remains unresolved or changes kind",
                                ));
                            }
                        };
                        self.unify(carrier, actual_carrier, 0, b)?;
                        true
                    }
                };
                if !valid || !self.constructor_complete(actual, b, 0)? {
                    return Err(fail(
                        "original constructor has an unresolved or wrong-kind full mixed actual",
                    ));
                }
            }
        }
        Ok(())
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
            let actual = self.expression_expected(context, value, fields[index], b, depth + 1)?;
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
            LifeNominalKind::Record | LifeNominalKind::Resource,
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
    actual_qualified(
        definition,
        arguments,
        inventory,
        b,
        depth,
        &|owner, args, requirements, b, depth| {
            requirements_actual(owner, args, requirements, inventory, b, depth)
        },
    )
}

/// Concrete consumers supply their complete original premise interpreter;
/// every nested nominal application retains the same interpretation.
#[allow(clippy::type_complexity)]
pub(in crate::semantic) fn actual_qualified(
    definition: SemanticDefinitionId,
    arguments: &[Type],
    inventory: &Inventory<'_>,
    b: &mut Budget,
    depth: usize,
    qualify: &dyn Fn(
        SemanticDefinitionId,
        &[Type],
        &[SemanticRequirementRecipe],
        &mut Budget,
        usize,
    ) -> CompileResult<()>,
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
    {
        return Err(unknown(
            "nominal actual needs its full lexical, lifetime and Drop definition proof",
        ));
    }
    if arguments.len() != declaration.parameters.len() {
        return Err(fail("nominal actual omits full original mixed formals"));
    }
    for (formal, argument) in declaration.parameters.iter().zip(arguments) {
        let SemanticFormal::Type { .. } = formal else {
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
    }
    for formal in &declaration.parameters {
        let SemanticFormal::Type { requirements } = formal else {
            return Err(fail("validated nominal formal changed its kind"));
        };
        qualify(definition, arguments, requirements, b, depth + 1)?;
    }
    qualify(
        definition,
        arguments,
        &declaration.requirements,
        b,
        depth + 1,
    )?;
    let mut physical = Vec::new();
    match body {
        SemanticNominalBody::Product { fields } => {
            for field in fields {
                let ty = substitute(
                    definition,
                    arguments,
                    &field.ty,
                    inventory,
                    b,
                    depth + 1,
                    qualify,
                )?;
                Solver::push(&mut physical, ty, b)?;
            }
        }
        SemanticNominalBody::Enum { variants } => {
            for variant in variants {
                for field in &variant.fields {
                    let ty = substitute(
                        definition,
                        arguments,
                        field,
                        inventory,
                        b,
                        depth + 1,
                        qualify,
                    )?;
                    Solver::push(&mut physical, ty, b)?;
                }
            }
        }
    }
    materialize(definition, arguments, &physical, inventory, b, depth + 1)
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn substitute(
    owner: SemanticDefinitionId,
    arguments: &[Type],
    recipe: &SemanticTypeRecipe,
    inventory: &Inventory<'_>,
    b: &mut Budget,
    depth: usize,
    qualify: &dyn Fn(
        SemanticDefinitionId,
        &[Type],
        &[SemanticRequirementRecipe],
        &mut Budget,
        usize,
    ) -> CompileResult<()>,
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
        SemanticTypeRecipe::Managed(inner) => Ok(Type::Managed(Box::new(substitute(
            owner,
            arguments,
            inner,
            inventory,
            b,
            depth + 1,
            qualify,
        )?))),
        SemanticTypeRecipe::OwnedStore(element_recipe) => {
            let element = substitute(
                owner,
                arguments,
                element_recipe,
                inventory,
                b,
                depth + 1,
                qualify,
            )?;
            // Store formation has its own sealed premise, even when no formal
            // declaration spells StoreElement on the enclosing nominal.
            crate::dyn_metadata::recipe(element_recipe.as_ref(), b)?;
            qualify(
                owner,
                arguments,
                &[SemanticRequirementRecipe::Structural {
                    ty: element_recipe.as_ref().clone(),
                    bound: DynStructuralBound::StoreElement,
                }],
                b,
                depth + 1,
            )?;
            Ok(Type::OwnedStore {
                element: Box::new(element),
            })
        }
        SemanticTypeRecipe::ErrorUnion { error, result } => Ok(Type::ErrorUnion {
            error: Box::new(substitute(
                owner,
                arguments,
                error,
                inventory,
                b,
                depth + 1,
                qualify,
            )?),
            result: Box::new(substitute(
                owner,
                arguments,
                result,
                inventory,
                b,
                depth + 1,
                qualify,
            )?),
        }),
        SemanticTypeRecipe::Tuple(fields) => {
            if fields.is_empty() {
                return Ok(Type::Unit);
            }
            let mut actuals = Vec::new();
            for field in fields {
                let ty = substitute(owner, arguments, field, inventory, b, depth + 1, qualify)?;
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
                let ty = substitute(owner, arguments, recipe, inventory, b, depth + 1, qualify)?;
                Solver::push(&mut actuals, ty, b)?;
            }
            actual_qualified(*definition, &actuals, inventory, b, depth + 1, qualify)
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
        let ty = substitute(
            owner,
            arguments,
            recipe,
            inventory,
            b,
            depth + 1,
            &|owner, args, requirements, b, depth| {
                requirements_actual(owner, args, requirements, inventory, b, depth)
            },
        )?;
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
                for field in fields {
                    if !predicate_actual(field, Predicate::Value, inventory, b, depth + 1)?
                        || !predicate_actual(field, Predicate::SyncDrop, inventory, b, depth + 1)?
                    {
                        return Ok(false);
                    }
                }
                crate::pod_store::semantic_element(inventory.module, complete, b)?;
                return Ok(true);
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
            let mut declaration = None;
            for instance in &inventory.module.life_instances {
                crate::dyn_metadata::ty(&instance.ty, b)?;
                crate::dyn_metadata::ty(complete, b)?;
                if instance.ty == *complete && declaration.replace(instance.declaration).is_some() {
                    return Err(fail(
                        "actual nominal predicate has competing exact identities",
                    ));
                }
            }
            let declaration = declaration
                .ok_or_else(|| fail("actual nominal predicate has no original identity"))?;
            let mut owner = None;
            for definition in inventory.definitions.values() {
                b.spend(1, WORK)?;
                if let SemanticDefinitionBody::Nominal {
                    declaration: id,
                    kind,
                    ..
                } = &definition.body
                    && *id == declaration
                {
                    if *kind == LifeNominalKind::Resource {
                        return Ok(false);
                    }
                    if owner.replace(definition.id).is_some() {
                        return Err(fail(
                            "actual nominal predicate has ambiguous original definitions",
                        ));
                    }
                }
            }
            resources::ordinary_drop_absent(
                owner.ok_or_else(|| {
                    fail("actual nominal predicate has no complete original declaration")
                })?,
                inventory,
                b,
            )?;
            if matches!(predicate, Predicate::StoreElement) {
                if matches!(ty, Type::Record { .. }) {
                    for (_, field) in fields {
                        if !predicate_actual(field, Predicate::Value, inventory, b, depth + 1)?
                            || !predicate_actual(
                                field,
                                Predicate::SyncDrop,
                                inventory,
                                b,
                                depth + 1,
                            )?
                        {
                            return Ok(false);
                        }
                    }
                    crate::pod_store::semantic_element(inventory.module, complete, b)?;
                    return Ok(true);
                }
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
        Type::Array { element, .. } => {
            if matches!(predicate, Predicate::StoreElement) {
                if !predicate_actual(element, Predicate::Value, inventory, b, depth + 1)?
                    || !predicate_actual(element, Predicate::SyncDrop, inventory, b, depth + 1)?
                {
                    return Ok(false);
                }
                crate::pod_store::semantic_element(inventory.module, complete, b)?;
                Ok(true)
            } else {
                predicate_actual(element, predicate, inventory, b, depth + 1)
            }
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
