//! Original-world proof for complete programs without borrowed values. This
//! discharges the empty class-flow case; nonempty worlds continue to the class,
//! seal and partition consumers rather than being relabelled as empty.
use super::emission::Ledger;
use crate::*;
use std::collections::BTreeMap;

fn bad(message: &str) -> Diagnostic {
    error("E-SEMANTIC-PROOF", Span::default(), message)
}
fn pending(message: &str) -> Diagnostic {
    Diagnostic::unsupported(Phase::Types, message, None)
}
struct Proof<'a, 'b> {
    program: &'a SemanticModule,
    module: &'a TypedModule,
    ledger: &'a Ledger,
    limits: &'b Limits,
    left: &'b mut usize,
}
impl Proof<'_, '_> {
    fn pay(&mut self, count: usize) -> R<()> {
        *self.left = self.left.checked_sub(count).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "original semantic proof work exceeded", None)
        })?;
        Ok(())
    }
    fn zero(&mut self, ty: &Type, depth: usize) -> R<bool> {
        self.pay(1)?;
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original semantic proof depth exceeded",
                None,
            ));
        }
        if ty.is_b0_scalar() {
            return Ok(true);
        }
        Ok(match ty {
            Type::Tuple(fields) => {
                let mut all = true;
                for field in fields {
                    all &= self.zero(field, depth + 1)?;
                }
                all
            }
            Type::Array { element, .. } => self.zero(element, depth + 1)?,
            Type::Closure(id) => {
                self.pay(self.module.closures.len())?;
                let definition = self
                    .module
                    .closures
                    .iter()
                    .find(|row| row.id == *id)
                    .ok_or_else(|| bad("actual closure type has no definition"))?;
                let life = definition
                    .storage_life
                    .as_ref()
                    .ok_or_else(|| pending("actual closure class directory remains unknown"))?;
                if life.fixed_binder_count != 0 {
                    false
                } else {
                    let mut all = true;
                    for capture in &definition.captures {
                        all &= self.zero(capture, depth + 1)?;
                    }
                    all
                }
            }
            // A nominal or opaque zero domain needs its original complete
            // instantiation proof, not a physical-field or empty-vector guess.
            _ => false,
        })
    }
    fn value(&mut self, value: &SemanticTypeValue) -> R<()> {
        let SemanticTypeValue::Resolved { ty, life, classes } = value else {
            return Err(bad("Core type survived semantic materialization"));
        };
        if life.binder_count != 0
            || !life.slots.is_empty()
            || !life.retained.is_empty()
            || !life.nominal_arguments.is_empty()
            || !classes.is_empty()
            || !self.zero(ty, 0)?
        {
            return Err(pending(
                "nonempty original classes require the complete body class and environment proof",
            ));
        }
        Ok(())
    }
    fn actual(&mut self, value: &SemanticActual) -> R<()> {
        self.pay(1)?;
        match value {
            SemanticActual::Type(value) => self.value(value),
            SemanticActual::Const { ty, value } => {
                self.value(ty)?;
                let SemanticTypeValue::Resolved { ty, .. } = ty else {
                    unreachable!()
                };
                if *ty != value.ty() {
                    return Err(bad("original const actual has a different carrier"));
                }
                Ok(())
            }
            SemanticActual::Life(_) => Err(pending(
                "original lifetime actual needs its rigid world proof",
            )),
            SemanticActual::CoreRecipe { .. } => {
                Err(bad("Core actual survived semantic materialization"))
            }
        }
    }
    fn match_type(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        ty: &Type,
    ) -> R<()> {
        let value = super::materialize::type_at(
            self.program,
            self.module,
            self.ledger,
            scope,
            recipe,
            Some(ty),
            self.limits,
            self.left,
        )?;
        self.value(&value)
    }
    // A concrete caller cannot justify a type-specific operation in an
    // unconstrained definition. Opaque forwarding is proved from the original
    // parameter/result recipes, independently of the current Type actual.
    fn rigid_body(
        &mut self,
        definition: &SemanticDefinition,
        signature: &SemanticSignatureRecipe,
        function: &TypedFunction,
    ) -> R<()> {
        if definition.parameters.is_empty() {
            return Ok(());
        }
        if function.blocks.len() != 1 || !definition.environment.is_empty() {
            return Err(pending(
                "generic CFG needs its complete symbolic original-body proof",
            ));
        }
        self.pay(
            function
                .values
                .len()
                .saturating_add(function.parameters.len()),
        )?;
        let mut values = vec![None; function.values.len()];
        for instruction in &function.blocks[0].instructions {
            self.pay(1)?;
            match instruction.kind {
                InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                    let index = function.parameters.iter().position(|p| *p == local)
                        .ok_or_else(|| pending("generic local needs its original declaration and symbolic assignment proof"))?;
                    if function.locals[local.0 as usize].mutable {
                        return Err(pending(
                            "mutable generic parameter requires full original reaching assignments",
                        ));
                    }
                    let result = instruction
                        .result
                        .ok_or_else(|| bad("generic parameter transfer has no actual result"))?;
                    let slot = values
                        .get_mut(result.0 as usize)
                        .ok_or_else(|| bad("generic value is outside its table"))?;
                    *slot = Some(&signature.parameters[index].ty);
                }
                InstructionKind::ScopeEnter { .. }
                | InstructionKind::ScopeExit { .. }
                | InstructionKind::StorageDead(_) => {}
                _ => {
                    return Err(pending(
                        "generic operation requires its original symbolic typing premise",
                    ))
                }
            }
        }
        match function.blocks[0].terminator {
            Terminator::Return(Some(value)) => {
                let source = values
                    .get(value.0 as usize)
                    .and_then(|r| *r)
                    .ok_or_else(|| bad("generic result has no original whole-type source"))?;
                crate::dyn_metadata::semantic_operand_snapshot(source, self.limits, self.left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(
                    &signature.result,
                    self.limits,
                    self.left,
                    1,
                )?;
                if source != &signature.result {
                    return Err(bad("generic result changes its original type formal despite equal concrete actuals"));
                }
            }
            _ => {
                return Err(pending(
                    "generic control flow requires its full symbolic result proof",
                ))
            }
        }
        Ok(())
    }
    fn requirements(
        &mut self,
        scope: SemanticInstanceId,
        requirements: &[SemanticRequirementRecipe],
    ) -> R<()> {
        self.pay(requirements.len())?;
        for requirement in requirements {
            match requirement {
                SemanticRequirementRecipe::Structural {
                    ty,
                    bound: DynStructuralBound::Copy,
                } => {
                    let value = super::materialize::type_at(
                        self.program,
                        self.module,
                        self.ledger,
                        scope,
                        ty,
                        None,
                        self.limits,
                        self.left,
                    )?;
                    self.value(&value)?;
                    let SemanticTypeValue::Resolved { ty, .. } = value else {
                        unreachable!()
                    };
                    if !ty.is_copy_value() {
                        return Err(bad("original Copy premise fails for the actual whole type"));
                    }
                }
                SemanticRequirementRecipe::TypeEqual { left, right } => {
                    let left = super::materialize::type_at(
                        self.program,
                        self.module,
                        self.ledger,
                        scope,
                        left,
                        None,
                        self.limits,
                        self.left,
                    )?;
                    let right = super::materialize::type_at(
                        self.program,
                        self.module,
                        self.ledger,
                        scope,
                        right,
                        None,
                        self.limits,
                        self.left,
                    )?;
                    self.value(&left)?;
                    self.value(&right)?;
                    crate::dyn_metadata::semantic_operand_snapshot(
                        &left,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    crate::dyn_metadata::semantic_operand_snapshot(
                        &right,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    if left != right {
                        return Err(bad(
                            "original whole-type equality does not hold in this actual world",
                        ));
                    }
                }
                _ => {
                    return Err(pending(
                        "original semantic premise requires its complete world solver",
                    ))
                }
            }
        }
        Ok(())
    }
}

pub(super) fn program(
    program: &SemanticModule,
    module: &TypedModule,
    ledger: &Ledger,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let mut proof = Proof {
        program,
        module,
        ledger,
        limits,
        left,
    };
    proof.pay(
        program
            .definitions
            .len()
            .saturating_mul(2)
            .saturating_add(program.instances.len().saturating_mul(3)),
    )?;
    let definitions: BTreeMap<_, _> = program
        .definitions
        .iter()
        .map(|row| (row.id, row))
        .collect();
    let mut targets = BTreeMap::new();
    for instance in &program.instances {
        if instance.lifetime_instantiation != SemanticLifetimeInstantiation::Rigid {
            return Err(pending(
                "lifetime partition requires full rigid CFG and type substitution correspondence",
            ));
        }
        let definition = definitions
            .get(&instance.definition)
            .ok_or_else(|| bad("actual semantic instance has no original definition"))?;
        if !definition.implicit_lifetimes.is_empty()
            || definition
                .parameters
                .iter()
                .any(|p| matches!(p, SemanticFormal::Lifetime))
        {
            return Err(pending("original implicit or declared lifetime inputs require their complete rigid class world"));
        }
        if instance.self_type.is_some() {
            return Err(pending(
                "actual Self world requires its complete lexical method proof",
            ));
        }
        proof.pay(instance.actuals.len())?;
        for actual in &instance.actuals {
            proof.actual(actual)?;
        }
        if !instance.header_embedding.is_empty() {
            return Err(bad("empty class world has nonempty header bindings"));
        }
        if let Some(environment) = &instance.environment {
            proof.pay(environment.actuals.len())?;
            if environment.actuals.len() != definition.environment.len() {
                return Err(bad("actual environment omits original mixed slots"));
            }
            for actual in &environment.actuals {
                proof.actual(actual)?;
            }
        } else if !definition.environment.is_empty() {
            return Err(bad("actual closure omits its original ghost environment"));
        }
        proof.requirements(instance.id, &definition.requirements)?;
        for formal in &definition.parameters {
            if let SemanticFormal::Type { requirements } = formal {
                proof.requirements(instance.id, requirements)?;
            }
        }
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) =
            instance.target
        else {
            return Err(pending(
                "original lexical nominal/trait instance requires its complete scope proof",
            ));
        };
        proof.pay(module.functions.len().saturating_add(targets.len()))?;
        let function = module
            .functions
            .iter()
            .find(|row| row.id == function)
            .ok_or_else(|| bad("semantic instance target is not an actual Typed function"))?;
        if targets.insert(function.id, instance).is_some() {
            return Err(bad(
                "one actual function has multiple unproved original worlds",
            ));
        }
        let SemanticDefinitionBody::Function { signature, .. } = &definition.body else {
            return Err(bad(
                "function instance changes its original declaration kind",
            ));
        };
        if signature.bound_lifetimes != 0 || signature.invocation_lifetimes != 0 {
            return Err(pending(
                "signature quantifiers require their complete scoped lifetime interpretation",
            ));
        }
        if signature.parameters.len() != function.parameters.len()
            || signature.call_kind != function.call_kind
            || signature.result_mode != function.result_mode
        {
            return Err(bad(
                "actual function signature changes its original complete ABI/effects",
            ));
        }
        if let Some(life) = &function.storage_life {
            if life.fixed_binder_count != 0
                || life.invocation_binder_count != 0
                || !life.bindings.is_empty()
                || !life.classes.is_empty()
                || !life.static_bindings.is_empty()
            {
                return Err(pending(
                    "nonempty function header needs the original class/occurrence proof",
                ));
            }
        }
        for (parameter, recipe) in function.parameters.iter().zip(&signature.parameters) {
            let local = function
                .locals
                .get(parameter.0 as usize)
                .ok_or_else(|| bad("actual signature parameter is outside its local table"))?;
            proof.match_type(instance.id, &recipe.ty, &local.ty)?;
        }
        proof.match_type(instance.id, &signature.result, &function.return_type)?;
        proof.rigid_body(definition, signature, function)?;
        for ty in function
            .locals
            .iter()
            .map(|row| &row.ty)
            .chain(&function.values)
        {
            if !proof.zero(ty, 0)? {
                return Err(pending(
                    "actual CFG contains a class/borrow operation needing full class-flow proof",
                ));
            }
        }
    }
    let mut sites = BTreeMap::new();
    proof.pay(program.sites.len().saturating_mul(2))?;
    for site in &program.sites {
        let instance = program
            .instances
            .iter()
            .find(|row| row.id == site.instance)
            .ok_or_else(|| bad("actual semantic site has no world"))?;
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) =
            instance.target
        else {
            return Err(bad("actual site is outside a function world"));
        };
        if !matches!(
            site.action,
            SemanticSiteAction::Call { .. }
                | SemanticSiteAction::CallableCall { .. }
                | SemanticSiteAction::Closure { .. }
        ) {
            return Err(pending(
                "body type-use or constructor requires original recipe class-flow proof",
            ));
        }
        let SemanticSiteAddress::Typed {
            block, instruction, ..
        } = site.at
        else {
            return Err(bad("Core site survived actual relocation"));
        };
        proof.pay(sites.len().saturating_add(1).ilog2() as usize + 1)?;
        let rows = sites
            .entry((function, block, instruction))
            .or_insert_with(Vec::new);
        rows.push(site);
    }
    proof.pay(module.functions.len())?;
    for function in &module.functions {
        let instance = targets
            .get(&function.id)
            .ok_or_else(|| bad("actual function has no complete original semantic instance"))?;
        let definition = definitions[&instance.definition];
        let SemanticDefinitionBody::Function {
            sites: original_sites,
            ..
        } = &definition.body
        else {
            unreachable!()
        };
        for block in &function.blocks {
            proof.pay(block.instructions.len())?;
            for (index, instruction) in block.instructions.iter().enumerate() {
                let is_call = matches!(
                    instruction.kind,
                    InstructionKind::Call { .. }
                        | InstructionKind::SuspendCall { .. }
                        | InstructionKind::ForeignCall { .. }
                        | InstructionKind::CallIndirect { .. }
                        | InstructionKind::CallClosure { .. }
                        | InstructionKind::CallClosureShared { .. }
                        | InstructionKind::CallClosureMut { .. }
                        | InstructionKind::CallClosureBorrowed { .. }
                        | InstructionKind::CallClosureOnce { .. }
                );
                let is_closure = matches!(instruction.kind, InstructionKind::MakeClosure { .. });
                if !is_call && !is_closure {
                    continue;
                }
                proof.pay(sites.len().saturating_add(1).ilog2() as usize + 1)?;
                let rows = sites
                    .get(&(function.id, block.id, index as u32))
                    .ok_or_else(|| {
                        bad("actual call or closure constructor lacks its original semantic site")
                    })?;
                let mut covered = false;
                for site in rows {
                    match (&instruction.kind, &site.action) {
                        (InstructionKind::Call { function: callee, .. } | InstructionKind::SuspendCall { function: callee, .. }
                            | InstructionKind::ForeignCall { function: callee, .. }, SemanticSiteAction::Call { target: SemanticFunctionTarget::Typed(target), actuals }) if callee == target => {
                            let target = targets.get(callee).ok_or_else(|| bad("static call has no actual original callee world"))?;
                            proof.pay(original_sites.len())?;
                            let recipe = original_sites.iter().find(|row| row.id == site.recipe).ok_or_else(|| bad("actual call has no original recipe"))?;
                            let SemanticSiteRecipeKind::Call { target: original, actuals: expected } = &recipe.kind else { return Err(bad("actual static call changes its original recipe kind")); };
                            if *original != target.definition || actuals.len() != target.actuals.len() || actuals.len() != expected.len() { return Err(bad("actual static call omits original mixed actuals or selects another definition")); }
                            for ((actual, expected), target) in actuals.iter().zip(expected).zip(&target.actuals) {
                                proof.actual(actual)?;
                                let value = super::materialize::actual_at(program, module, ledger, instance.id, expected, limits, proof.left)?;
                                crate::dyn_metadata::semantic_operand_snapshot(actual, limits, proof.left, 2)?;
                                crate::dyn_metadata::semantic_operand_snapshot(&value, limits, proof.left, 1)?;
                                if actual != &value || actual != target { return Err(bad("actual call differs from original recipe or complete callee Type/Const world")); }
                            }
                            covered = true;
                        }
                        (InstructionKind::MakeClosure { .. }, SemanticSiteAction::Closure { .. })
                        | (_, SemanticSiteAction::CallableCall { .. }) => return Err(pending("callable operation requires the complete original callee and environment proof")),
                        _ => {}
                    }
                }
                if !covered {
                    return Err(bad(
                        "actual call/constructor has no matching original operation",
                    ));
                }
            }
        }
    }
    Ok(())
}
