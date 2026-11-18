//! Instantiate pending nominal applications at their real emitted use. Source
//! templates containing F are not complete type applications. The physical use
//! proposes only a layout; the ordinary-domain and original-field passes still
//! check every resulting row before publication.
use super::*;
#[path = "materialize_qualified.rs"]
mod qualified;
#[path = "materialize_qualified_rewrite.rs"]
mod rewrite;

struct Pending {
    rows: Vec<(Type, LifeNominalInstance)>,
}

impl Eval<'_, '_, '_> {
    fn deferred_result(&mut self, result: R<()>, blocked: &mut BTreeSet<ClosureId>) -> R<()> {
        let dependency = self.pending_header.take();
        match result {
            Err(diagnostic) if is_pending_header(&diagnostic) && dependency.is_some() => {
                self.fee(blocked.len().saturating_add(1).ilog2() as usize + 1)?;
                blocked.insert(dependency.expect("checked exact missing actual header"));
                Ok(())
            }
            result => result,
        }
    }
    fn deferred_logical_nominal(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        hint: LogicalExpected<'_>,
        depth: usize,
        pending: &mut Pending,
    ) -> R<()> {
        hint.materialize(|ty| self.deferred_physical_nominal(scope, recipe, ty, depth, pending))
    }
    fn deferred_physical_nominal(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        hint: &Type,
        depth: usize,
        pending: &mut Pending,
    ) -> R<()> {
        self.node(depth)?;
        match (recipe, hint) {
            (
                SemanticTypeRecipe::OriginalType {
                    scope: selector,
                    source,
                },
                _,
            ) => {
                let recipe = self.original_type_view(scope, *selector, source, depth + 1)?;
                self.deferred_physical_nominal(scope, &recipe, hint, depth + 1, pending)
            }
            (
                SemanticTypeRecipe::Nominal {
                    definition,
                    arguments,
                },
                _,
            ) => {
                let original = self.definition(*definition)?;
                let SemanticDefinitionBody::Nominal { kind, .. } = &original.body else {
                    return Err(bad(
                        "pending nominal use does not select an original nominal",
                    ));
                };
                if original.parameters.len() != arguments.len() {
                    return Err(bad("pending nominal omits its full mixed actuals"));
                }
                if !matches!(
                    (kind, stored::physical(hint)),
                    (
                        LifeNominalKind::Record | LifeNominalKind::Resource,
                        Type::Record { .. }
                    ) | (LifeNominalKind::Package, Type::Package { .. })
                        | (LifeNominalKind::Enum, Type::Enum { .. })
                ) {
                    // This is not an exact physical use from which to mint
                    // an application. Effective resource/descriptor modes are
                    // resolved by their regular materializer and world check;
                    // this proposal pass never unwraps them speculatively.
                    return Ok(());
                }
                let row = self.qualified_nominal(scope, *definition, arguments, hint, depth + 1)?;
                self.fee(1)?;
                pending.rows.push((self.physical(hint)?, row));
                Ok(())
            }
            (SemanticTypeRecipe::Managed(recipe), Type::Managed(actual))
            | (SemanticTypeRecipe::Desc { inner: recipe, .. }, Type::Desc(actual))
            | (
                SemanticTypeRecipe::Address { inner: recipe, .. },
                Type::Address { inner: actual, .. },
            )
            | (SemanticTypeRecipe::OwnedStore(recipe), Type::OwnedStore { element: actual })
            | (SemanticTypeRecipe::Mutex(recipe), Type::Mutex { payload: actual })
            | (SemanticTypeRecipe::Slice(recipe), Type::Slice { element: actual })
            | (
                SemanticTypeRecipe::Array {
                    element: recipe, ..
                },
                Type::Array {
                    element: actual, ..
                },
            ) => self.deferred_physical_nominal(scope, recipe, actual, depth + 1, pending),
            (SemanticTypeRecipe::Tuple(recipes), Type::Tuple(actuals)) => {
                self.fee(recipes.len().saturating_add(actuals.len()))?;
                if recipes.len() != actuals.len() {
                    return Err(bad("pending tuple use changes its original arity"));
                }
                for (recipe, actual) in recipes.iter().zip(actuals) {
                    self.deferred_physical_nominal(scope, recipe, actual, depth + 1, pending)?;
                }
                Ok(())
            }
            (
                SemanticTypeRecipe::ErrorUnion { error, result },
                Type::ErrorUnion {
                    error: actual_error,
                    result: actual_result,
                },
            ) => {
                self.deferred_physical_nominal(scope, error, actual_error, depth + 1, pending)?;
                self.deferred_physical_nominal(scope, result, actual_result, depth + 1, pending)
            }
            _ => Ok(()),
        }
    }
}

/// Only real Typed entry parameters/results and relocated source sites supply
/// a physical proposal. Unused global aggregate prototypes supply no scope and
/// are deliberately not promoted to complete applications.
pub(crate) fn populate(
    program: &mut SemanticModule,
    module: &mut TypedModule,
    ledger: &mut Ledger,
    limits: &Limits,
    left: &mut usize,
) -> R<(bool, Formation<()>)> {
    let mut blocked = BTreeSet::new();
    let mut pending = Pending { rows: Vec::new() };
    {
        let mut eval = Eval {
            input: program,
            module: &ActualDirectory::from_module_at(module, "deferred nominals"),
            ledger,
            limits,
            left,
            active: BTreeSet::new(),
            invocation_probe: None,
            pending_header: None,
        };
        eval.fee(program.instances.len().saturating_add(program.sites.len()))?;
        for instance in &program.instances {
            if !matches!(
                instance.target,
                SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(_))
            ) {
                continue;
            }
            let function = eval.function(instance.id)?;
            eval.fee(function.parameters.len())?;
            for (index, local) in function.parameters.iter().enumerate() {
                let ty = &function
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| bad("nominal parameter has no actual local"))?
                    .ty;
                let recipe = SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source: SemanticOriginalTypeSource::Parameter(index as u32),
                };
                let request =
                    eval.deferred_physical_nominal(instance.id, &recipe, ty, 0, &mut pending);
                eval.deferred_result(request, &mut blocked)?;
            }
            let recipe = SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Result,
            };
            let request = eval.deferred_physical_nominal(
                instance.id,
                &recipe,
                function.result()?,
                0,
                &mut pending,
            );
            eval.deferred_result(request, &mut blocked)?;
        }
        for site in &program.sites {
            let function = eval.function(site.instance)?;
            if matches!(
                site.action,
                SemanticSiteAction::CoreBodyCall
                    | SemanticSiteAction::BodyCall { .. }
                    | SemanticSiteAction::Call { .. }
                    | SemanticSiteAction::CallableCall { .. }
            ) {
                let SemanticSiteAddress::Typed {
                    block, instruction, ..
                } = site.at
                else {
                    return Err(bad("pending call result has no actual emitted boundary"));
                };
                eval.fee(function.blocks.len())?;
                let operation = function
                    .blocks
                    .iter()
                    .find(|body| body.id == block)
                    .and_then(|body| body.instructions.get(instruction as usize))
                    .ok_or_else(|| bad("pending call result leaves its actual instruction"))?;
                let Some(result) = operation.result else {
                    continue;
                };
                let hint = logical_operand(
                    eval.ledger,
                    eval.module,
                    function,
                    &SemanticOperand::Value(result),
                    eval.limits,
                    eval.left,
                )?;
                let owner = eval.instance(site.instance)?.definition;
                let original = eval.definition(owner)?;
                let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &original.body
                else {
                    continue;
                };
                eval.fee(site_nodes.len())?;
                let mut nodes = site_nodes.iter().filter(|node| node.site == site.recipe);
                let node = nodes
                    .next()
                    .ok_or_else(|| bad("pending call has no exact original source node"))?;
                if nodes.next().is_some() {
                    return Err(bad("pending call has multiple original source nodes"));
                }
                let SemanticOriginalNode::Expression(node) = node.node else {
                    return Err(bad("pending call source is not an original expression"));
                };
                let recipe = SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source: SemanticOriginalTypeSource::Expression(node),
                };
                let request =
                    eval.deferred_logical_nominal(site.instance, &recipe, hint, 0, &mut pending);
                eval.deferred_result(request, &mut blocked)?;
                continue;
            }
            let (value, hint) = match &site.action {
                SemanticSiteAction::Construct {
                    result: SemanticOperand::Value(result),
                    value,
                } => (
                    value,
                    logical_operand(
                        eval.ledger,
                        eval.module,
                        function,
                        &SemanticOperand::Value(*result),
                        eval.limits,
                        eval.left,
                    )?,
                ),
                SemanticSiteAction::DeclareLocal {
                    local: SemanticLocalTarget::Typed(local),
                    expected,
                } => (
                    expected,
                    LogicalExpected::local(eval.ledger.logical_local_type(
                        function,
                        *local,
                        eval.limits,
                        eval.left,
                    )?),
                ),
                SemanticSiteAction::TypeUse {
                    subject: SemanticOperand::Value(value),
                    expected,
                } => (
                    expected,
                    logical_operand(
                        eval.ledger,
                        eval.module,
                        function,
                        &SemanticOperand::Value(*value),
                        eval.limits,
                        eval.left,
                    )?,
                ),
                SemanticSiteAction::TypeUse {
                    subject: SemanticOperand::Local(local),
                    expected,
                } => (
                    expected,
                    LogicalExpected::local(eval.ledger.logical_local_type(
                        function,
                        *local,
                        eval.limits,
                        eval.left,
                    )?),
                ),
                _ => continue,
            };
            if let SemanticTypeValue::Core { scope, source } = value {
                match source {
                    SemanticCoreTypeSource::OriginalRecipe(recipe)
                    | SemanticCoreTypeSource::Expression { recipe, .. } => {
                        let request =
                            eval.deferred_logical_nominal(*scope, recipe, hint, 0, &mut pending);
                        eval.deferred_result(request, &mut blocked)?;
                    }
                    SemanticCoreTypeSource::OriginalExpression { source, .. } => {
                        let recipe = eval.original_type_view(
                            *scope,
                            SemanticScopeRef::Ancestor(0),
                            source,
                            0,
                        )?;
                        let request =
                            eval.deferred_logical_nominal(*scope, &recipe, hint, 0, &mut pending);
                        eval.deferred_result(request, &mut blocked)?;
                    }
                }
            }
        }
    }
    let mut replacements = Vec::new();
    let mut changed = false;
    // All proposals are complete before any actual payload is rewritten.
    // Repeated use sites must select exactly one original full application.
    for index in 0..pending.rows.len() {
        let (before, row) = &pending.rows[index];
        inference::charge_type_snapshot(before, Span::default(), limits, left, 1)?;
        charge_row(row, limits, left)?;
        for (other_before, other) in &pending.rows[..index] {
            inference::charge_type_snapshot(other_before, Span::default(), limits, left, 1)?;
            charge_row(other, limits, left)?;
            if before == other_before && row != other {
                return Err(bad(
                    "one actual nominal selects conflicting original full applications",
                ));
            }
        }
        if before != &row.ty {
            inference::charge_type_snapshot(before, Span::default(), limits, left, 1)?;
            inference::charge_type_snapshot(&row.ty, Span::default(), limits, left, 1)?;
            replacements.push((before.clone(), row.ty.clone()));
            changed = true;
        }
    }
    for (before, row) in pending.rows {
        let mut position = None;
        for (index, prior) in module.life_instances.iter().enumerate() {
            charge_row(prior, limits, left)?;
            inference::charge_type_snapshot(&before, Span::default(), limits, left, 1)?;
            charge_row(&row, limits, left)?;
            if prior.ty == before || prior.ty == row.ty {
                if position.replace(index).is_some() {
                    return Err(bad(
                        "canonical nominal application has duplicate catalogue rows",
                    ));
                }
                if prior.declaration != row.declaration {
                    return Err(bad("canonical nominal changes its original declaration"));
                }
            }
        }
        if let Some(index) = position {
            charge_row(&row, limits, left)?;
            charge_row(&module.life_instances[index], limits, left)?;
            if module.life_instances[index] != row {
                module.life_instances[index] = row;
                changed = true;
            }
        } else {
            module.life_instances.push(row);
            changed = true;
        }
    }
    rewrite::apply(program, module, ledger, &replacements, limits, left)?;
    Ok((
        changed,
        if blocked.is_empty() {
            Formation::Complete(())
        } else {
            Formation::NotReady {
                dependencies: blocked,
            }
        },
    ))
}

fn charge_row(row: &LifeNominalInstance, limits: &Limits, left: &mut usize) -> R<()> {
    inference::charge_type_snapshot(&row.ty, Span::default(), limits, left, 1)?;
    for actual in &row.arguments {
        match actual {
            LifeNominalActual::Type(ty) => {
                inference::charge_type_snapshot(ty, Span::default(), limits, left, 1)?
            }
            LifeNominalActual::QualifiedType { ty, life } => {
                inference::charge_type_snapshot(ty, Span::default(), limits, left, 1)?;
                let work = life
                    .slots
                    .iter()
                    .map(|r| r.path.len().saturating_add(1))
                    .chain(life.retained.iter().map(|r| r.path.len().saturating_add(1)))
                    .chain(
                        life.nominal_arguments
                            .iter()
                            .map(|r| r.path.len().saturating_add(1)),
                    )
                    .fold(life.binder_count as usize + 1, usize::saturating_add);
                *left = left.checked_sub(work).ok_or_else(|| {
                    Diagnostic::limit(Phase::Types, "qualified catalogue snapshot exceeded", None)
                })?;
            }
            LifeNominalActual::Const(Constant::WideString(units)) => {
                *left = left
                    .checked_sub(units.len().saturating_add(1))
                    .ok_or_else(|| {
                        Diagnostic::limit(Phase::Types, "qualified Const snapshot exceeded", None)
                    })?;
            }
            LifeNominalActual::Const(_) | LifeNominalActual::Lifetime(_) => {
                *left = left.checked_sub(1).ok_or_else(|| {
                    Diagnostic::limit(Phase::Types, "qualified catalogue snapshot exceeded", None)
                })?;
            }
        }
    }
    Ok(())
}
