//! Instantiate pending nominal applications at their real emitted use. Source
//! templates containing F are not complete type applications. The physical use
//! proposes only a layout; the ordinary-domain and original-field passes still
//! check every resulting row before publication.
use super::*;
use std::collections::BTreeMap;

struct Pending<'a> {
    known: BTreeMap<zil_base::Fingerprint, Vec<&'a Type>>,
    rows: Vec<LifeNominalInstance>,
}

impl Eval<'_, '_, '_> {
    fn deferred_nominal(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        hint: &Type,
        depth: usize,
        pending: &mut Pending<'_>,
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
                let recipe = self.original_type(scope, *selector, source, depth + 1)?;
                self.deferred_nominal(scope, &recipe, hint, depth + 1, pending)
            }
            (
                SemanticTypeRecipe::Nominal {
                    definition,
                    arguments,
                },
                _,
            ) => {
                let original = self.definition(*definition)?;
                let SemanticDefinitionBody::Nominal {
                    declaration, kind, ..
                } = &original.body
                else {
                    return Err(bad(
                        "pending nominal use does not select an original nominal",
                    ));
                };
                inference::charge_type_snapshot(hint, Span::default(), self.limits, self.left, 1)?;
                let key = hint.fingerprint();
                self.fee(
                    key.0
                        .len()
                        .saturating_add(pending.known.len().saturating_add(1).ilog2() as usize + 1),
                )?;
                if let Some(types) = pending.known.get(&key) {
                    self.fee(types.len())?;
                    for actual in types {
                        inference::charge_type_snapshot(
                            actual,
                            Span::default(),
                            self.limits,
                            self.left,
                            1,
                        )?;
                        if *actual == hint {
                            // The structural index only locates exact types.
                            // Regular materialization still checks full actuals.
                            return Ok(());
                        }
                    }
                }
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
                if let Type::BorrowedStorage { life, .. } = hint {
                    let _paid = self.copy_shape(life)?;
                    if life.binder_count != 0
                        || !life.slots.is_empty()
                        || !life.retained.is_empty()
                        || !life.nominal_arguments.is_empty()
                    {
                        return Err(unknown(
                            "pending nonzero nominal requires canonical full-type rewriting at every actual use",
                        ));
                    }
                }
                self.fee(arguments.len())?;
                let mut actuals = Vec::new();
                for (formal, argument) in original.parameters.iter().zip(arguments) {
                    let actual = self.recipe_actual(scope, argument, depth + 1)?;
                    actuals.push(match (formal, actual) {
                        (SemanticFormal::Type { .. }, SemanticActual::Type(SemanticTypeValue::Resolved { ty, life, classes })) => {
                            if life.binder_count != 0 || !classes.is_empty() || !life.slots.is_empty()
                                || !life.retained.is_empty() || !life.nominal_arguments.is_empty() {
                                return Err(unknown("pending nominal Type actual retains a nonzero or incomplete intrinsic domain"));
                            }
                            if matches!(ty, Type::BorrowedStorage { .. }) {
                                LifeNominalActual::QualifiedType { ty, life }
                            } else {
                                LifeNominalActual::Type(ty)
                            }
                        }
                        (SemanticFormal::Const { .. }, SemanticActual::Const { ty: SemanticTypeValue::Resolved { ty, life, classes }, value }) => {
                            if life.binder_count != 0 || !classes.is_empty() || !life.slots.is_empty()
                                || !life.retained.is_empty() || !life.nominal_arguments.is_empty() || value.ty() != ty {
                                return Err(bad("pending nominal Const actual changes its complete carrier"));
                            }
                            LifeNominalActual::Const(value)
                        }
                        (SemanticFormal::Lifetime, SemanticActual::Life(_)) => return Err(unknown("pending phantom lifetime requires a complete qualified nominal boundary")),
                        _ => return Err(bad("pending nominal changes a full mixed actual kind")),
                    });
                }
                self.fee(1)?;
                pending.rows.push(LifeNominalInstance {
                    declaration: *declaration,
                    ty: self.physical(hint)?,
                    arguments: actuals,
                });
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
            ) => self.deferred_nominal(scope, recipe, actual, depth + 1, pending),
            (SemanticTypeRecipe::Tuple(recipes), Type::Tuple(actuals)) => {
                self.fee(recipes.len().saturating_add(actuals.len()))?;
                if recipes.len() != actuals.len() {
                    return Err(bad("pending tuple use changes its original arity"));
                }
                for (recipe, actual) in recipes.iter().zip(actuals) {
                    self.deferred_nominal(scope, recipe, actual, depth + 1, pending)?;
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
                self.deferred_nominal(scope, error, actual_error, depth + 1, pending)?;
                self.deferred_nominal(scope, result, actual_result, depth + 1, pending)
            }
            _ => Ok(()),
        }
    }
}

/// Only real Typed entry parameters/results and relocated source sites supply
/// a physical proposal. Unused global aggregate prototypes supply no scope and
/// are deliberately not promoted to complete applications.
pub(crate) fn populate(
    program: &SemanticModule,
    module: &mut TypedModule,
    ledger: &Ledger,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let mut pending = Pending {
        known: BTreeMap::new(),
        rows: Vec::new(),
    };
    for row in &module.life_instances {
        inference::charge_type_snapshot(&row.ty, Span::default(), limits, left, 1)?;
        let key = row.ty.fingerprint();
        let work = key
            .0
            .len()
            .saturating_add(pending.known.len().saturating_add(1).ilog2() as usize + 1);
        *left = left.checked_sub(work).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "pending nominal index work exceeded", None)
        })?;
        pending.known.entry(key).or_default().push(&row.ty);
    }
    {
        let mut eval = Eval {
            input: program,
            module: &ActualDirectory::from_module_at(module, "deferred nominals"),
            ledger,
            limits,
            left,
            active: BTreeSet::new(),
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
                eval.deferred_nominal(instance.id, &recipe, ty, 0, &mut pending)?;
            }
            let recipe = SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Result,
            };
            eval.deferred_nominal(instance.id, &recipe, function.result()?, 0, &mut pending)?;
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
                let hint = function
                    .values
                    .get(result.0 as usize)
                    .ok_or_else(|| bad("pending call result has no actual whole type"))?;
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
                eval.deferred_nominal(site.instance, &recipe, hint, 0, &mut pending)?;
                continue;
            }
            let (value, hint) = match &site.action {
                SemanticSiteAction::Construct {
                    result: SemanticOperand::Value(result),
                    value,
                } => (value, function.values.get(result.0 as usize)),
                SemanticSiteAction::DeclareLocal {
                    local: SemanticLocalTarget::Typed(local),
                    expected,
                } => (
                    expected,
                    function.locals.get(local.0 as usize).map(|local| &local.ty),
                ),
                SemanticSiteAction::TypeUse {
                    subject: SemanticOperand::Value(value),
                    expected,
                } => (expected, function.values.get(value.0 as usize)),
                SemanticSiteAction::TypeUse {
                    subject: SemanticOperand::Local(local),
                    expected,
                } => (
                    expected,
                    function.locals.get(local.0 as usize).map(|local| &local.ty),
                ),
                _ => continue,
            };
            let hint = hint.ok_or_else(|| bad("pending nominal use has no actual emitted type"))?;
            if let SemanticTypeValue::Core { scope, source } = value {
                match source {
                    SemanticCoreTypeSource::OriginalRecipe(recipe)
                    | SemanticCoreTypeSource::Expression { recipe, .. } => {
                        eval.deferred_nominal(*scope, recipe, hint, 0, &mut pending)?;
                    }
                    SemanticCoreTypeSource::OriginalExpression { source, .. } => {
                        let recipe =
                            eval.original_type(*scope, SemanticScopeRef::Ancestor(0), source, 0)?;
                        eval.deferred_nominal(*scope, &recipe, hint, 0, &mut pending)?;
                    }
                }
            }
        }
    }
    drop(pending.known);
    for row in pending.rows {
        let mut exists = false;
        for prior in &module.life_instances {
            inference::charge_type_snapshot(&prior.ty, Span::default(), limits, left, 1)?;
            inference::charge_type_snapshot(&row.ty, Span::default(), limits, left, 1)?;
            if prior.ty == row.ty {
                // Prepay full mixed-vector comparison, including opaque types.
                for actual in prior.arguments.iter().chain(&row.arguments) {
                    match actual {
                        LifeNominalActual::Type(ty) => {
                            inference::charge_type_snapshot(ty, Span::default(), limits, left, 1)?
                        }
                        LifeNominalActual::QualifiedType { ty, life } => {
                            inference::charge_type_snapshot(ty, Span::default(), limits, left, 1)?;
                            let work = life
                                .slots
                                .iter()
                                .map(|row| row.path.len().saturating_add(1))
                                .chain(
                                    life.retained
                                        .iter()
                                        .map(|row| row.path.len().saturating_add(1)),
                                )
                                .chain(
                                    life.nominal_arguments
                                        .iter()
                                        .map(|row| row.path.len().saturating_add(1)),
                                )
                                .try_fold(1usize, |work, add| work.checked_add(add))
                                .ok_or_else(|| {
                                    Diagnostic::limit(
                                        Phase::Types,
                                        "pending nominal qualification work overflow",
                                        None,
                                    )
                                })?;
                            *left = left.checked_sub(work).ok_or_else(|| {
                                Diagnostic::limit(
                                    Phase::Types,
                                    "pending nominal qualification work exceeded",
                                    None,
                                )
                            })?;
                        }
                        LifeNominalActual::Const(Constant::WideString(units)) => {
                            *left = left.checked_sub(units.len()).ok_or_else(|| {
                                Diagnostic::limit(
                                    Phase::Types,
                                    "pending nominal comparison work exceeded",
                                    None,
                                )
                            })?;
                        }
                        _ => {}
                    }
                }
                if prior != &row {
                    return Err(bad(
                        "one actual nominal use selects conflicting original full actuals",
                    ));
                }
                exists = true;
            }
        }
        if !exists {
            module.life_instances.push(row);
        }
    }
    Ok(())
}
