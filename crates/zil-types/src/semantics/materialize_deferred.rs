//! Instantiate pending nominal applications at their real emitted use. Source
//! templates containing F are not complete type applications. The physical use
//! proposes only a layout; the ordinary-domain and original-field passes still
//! check every resulting row before publication.
use super::*;
#[path = "materialize_qualified.rs"]
mod qualified;
#[path = "materialize_qualified_rewrite.rs"]
mod rewrite;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::semantics) enum NominalPass {
    Discover,
    Validate,
}

#[derive(Clone, Copy)]
pub(super) enum NominalPosition {
    Committed(usize),
    Proposed(usize),
}

/// A digest narrows the actual Type comparisons. Each candidate still needs
/// its complete Type equality, and the selected declaration still resolves
/// all mixed arguments at the actual use.
pub(in crate::semantics) struct NominalIndex<Row> {
    buckets: BTreeMap<zil_base::Fingerprint, Vec<Row>>,
}

impl<Row> Default for NominalIndex<Row> {
    fn default() -> Self {
        Self { buckets: BTreeMap::new() }
    }
}

impl<Row> NominalIndex<Row> {
    pub(in crate::semantics) fn key(
        ty: &Type,
        limits: &Limits,
        left: &mut usize,
    ) -> R<zil_base::Fingerprint> {
        if let Type::Record { name, .. } | Type::Package { name, .. } | Type::Enum { name, .. } =
            stored::physical(ty)
        {
            // A physical name selects a bucket only. Every successful
            // candidate still matches the complete Type and declaration.
            Self::pay(left, name.len().saturating_add(1))?;
            return Ok(zil_base::Fingerprint::of(name.as_bytes()));
        }
        inference::charge_type_snapshot(ty, Span::default(), limits, left, 1)?;
        let mut digest = zil_base::FingerprintBuilder::new();
        crate::semantics::original::protocol::hash_into(&mut digest, left, ty)?;
        Ok(digest.digest())
    }

    fn lookup_fee(&self) -> usize {
        self.buckets
            .len()
            .saturating_add(1)
            .ilog2()
            .saturating_add(2) as usize * 12
    }

    fn pay(left: &mut usize, amount: usize) -> R<()> {
        *left = left.checked_sub(amount).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "deferred nominal index work exceeded", None)
        })?;
        Ok(())
    }

    pub(in crate::semantics) fn insert(
        &mut self,
        key: zil_base::Fingerprint,
        row: Row,
        left: &mut usize,
    ) -> R<()> {
        Self::pay(
            left,
            self.lookup_fee()
                .saturating_add(std::mem::size_of::<(zil_base::Fingerprint, Row)>()),
        )?;
        self.buckets.entry(key).or_default().push(row);
        Ok(())
    }

    pub(in crate::semantics) fn get(&self, key: &zil_base::Fingerprint, left: &mut usize) -> R<&[Row]> {
        Self::pay(left, self.lookup_fee())?;
        Ok(self.buckets.get(key).map_or(&[], Vec::as_slice))
    }
}

impl Eval<'_, '_, '_> {
    pub(super) fn nominal_candidates_for_hint(&mut self, hint: &Type) -> R<Vec<NominalPosition>> {
        let module = self.module;
        let mut index = module.deferred_nominals.borrow_mut();
        if index.is_none() {
            let mut directory = NominalIndex::default();
            self.fee(module.life_instances.len())?;
            for (position, row) in module.life_instances.iter().enumerate() {
                let key = NominalIndex::<NominalPosition>::key(&row.ty, self.limits, self.left)?;
                directory.insert(key, NominalPosition::Committed(position), self.left)?;
            }
            *index = Some(directory);
        }
        let index = index.as_ref().expect("initialized nominal index");
        let hint_key = NominalIndex::<NominalPosition>::key(hint, self.limits, self.left)?;
        let bucket = index.get(&hint_key, self.left)?;
        self.fee(bucket.len().saturating_mul(std::mem::size_of::<NominalPosition>()))?;
        Ok(bucket.to_vec())
    }

    fn existing_deferred_nominal(
        &mut self,
        hint: &Type,
        declaration: LifeDeclarationId,
    ) -> R<Option<NominalPosition>> {
        let bucket = self.nominal_candidates_for_hint(hint)?;
        let module = self.module;
        let staged = module.nominal_proposals.borrow();
        let mut found = None;
        for position in bucket {
            self.fee(1)?;
            let (before, row) = match position {
                NominalPosition::Committed(position) => (None, &module.life_instances[position]),
                NominalPosition::Proposed(position) => {
                    let (before, row) = &staged[position];
                    (Some(before), row)
                }
            };
            inference::charge_type_snapshot(hint, Span::default(), self.limits, self.left, 1)?;
            inference::charge_type_snapshot(&row.ty, Span::default(), self.limits, self.left, 1)?;
            let matches = if &row.ty == hint {
                true
            } else if let Some(before) = before {
                inference::charge_type_snapshot(
                    hint,
                    Span::default(),
                    self.limits,
                    self.left,
                    1,
                )?;
                inference::charge_type_snapshot(
                    before,
                    Span::default(),
                    self.limits,
                    self.left,
                    1,
                )?;
                before == hint
            } else {
                false
            };
            if matches {
                if found.is_some() || row.declaration != declaration {
                    return Err(bad(match position {
                        NominalPosition::Committed(_) => {
                            "existing actual nominal has ambiguous original identity"
                        }
                        NominalPosition::Proposed(_) => {
                            "actual nominal repeats or changes its original declaration"
                        }
                    }));
                }
                found = Some(position);
            }
        }
        Ok(found)
    }

    fn propose_deferred_nominal(&mut self, before: Type, row: LifeNominalInstance) -> R<()> {
        let mut staged = self.module.nominal_proposals.borrow_mut();
        let position = NominalPosition::Proposed(staged.len());
        let mut index = self.module.deferred_nominals.borrow_mut();
        let index = index.as_mut().expect("nominal proposal follows indexed lookup");
        let previous = NominalIndex::<NominalPosition>::key(&before, self.limits, self.left)?;
        let canonical = NominalIndex::<NominalPosition>::key(&row.ty, self.limits, self.left)?;
        index.insert(previous, position, self.left)?;
        if previous != canonical {
            index.insert(canonical, position, self.left)?;
        }
        self.fee(1)?;
        staged.push((before, row));
        Ok(())
    }

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
        pass: NominalPass,
        depth: usize,
    ) -> R<()> {
        hint.materialize(|ty| self.deferred_physical_nominal(scope, recipe, ty, pass, depth))
    }
    fn deferred_physical_nominal(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        hint: &Type,
        pass: NominalPass,
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        // The formation pass can add only a complete nominal physically
        // present at this use. Scalar source equations are interpreted later
        // by full materialization and checked against the reaching SSA.
        if hint.is_b0_scalar() {
            return Ok(());
        }
        match (recipe, hint) {
            (
                SemanticTypeRecipe::OriginalType {
                    scope: selector,
                    source,
                },
                _,
            ) => {
                let recipe = self.original_type_view(scope, *selector, source, depth + 1)?;
                self.deferred_physical_nominal(scope, &recipe, hint, pass, depth + 1)
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
                // An existing full application owns its intrinsic coordinates.
                // In particular a scalar address field may use its exact slot
                // instead of an optional redundant retained-field row. Do not
                // replace that valid directory with a newly proposed forest.
                // Selection still checks the original declaration, all mixed
                // actuals and their complete domains through the ordinary reader;
                // finish separately proves every original field and where clause.
                if let Some(position) = self.existing_deferred_nominal(hint, *declaration)? {
                    if pass == NominalPass::Validate {
                        self.check_nominal_application(scope, *definition, arguments, position, depth + 1)?;
                    }
                    return Ok(());
                }
                let row = self.qualified_nominal(scope, *definition, arguments, hint, depth + 1)?;
                let before = self.physical(hint)?;
                self.propose_deferred_nominal(before, row)
            }
            (SemanticTypeRecipe::Qualified { storage, .. }, _) => {
                self.deferred_physical_nominal(scope, storage, hint, pass, depth + 1)
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
            ) => self.deferred_physical_nominal(scope, recipe, actual, pass, depth + 1),
            (SemanticTypeRecipe::Tuple(recipes), _) => {
                let Type::Tuple(actuals) = stored::physical(hint) else {
                    return Ok(());
                };
                self.fee(recipes.len().saturating_add(actuals.len()))?;
                if recipes.len() != actuals.len() {
                    return Err(bad("pending tuple use changes its original arity"));
                }
                for (recipe, actual) in recipes.iter().zip(actuals) {
                    self.deferred_physical_nominal(scope, recipe, actual, pass, depth + 1)?;
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
                self.deferred_physical_nominal(scope, error, actual_error, pass, depth + 1)?;
                self.deferred_physical_nominal(scope, result, actual_result, pass, depth + 1)
            }
            _ => Ok(()),
        }
    }
}

/// Only real Typed entry parameters/results and relocated source sites supply
/// a physical proposal. Unused global aggregate prototypes supply no scope and
/// are deliberately not promoted to complete applications.
pub(in crate::semantics) struct NominalFormation {
    pub catalogue_changed: bool,
    pub representation_changed: bool,
    pub state: Formation<()>,
}

pub(crate) fn populate(
    program: &mut SemanticModule,
    module: &mut TypedModule,
    ledger: &mut Ledger,
    pass: NominalPass,
    limits: &Limits,
    left: &mut usize,
) -> R<NominalFormation> {
    let mut blocked = BTreeSet::new();
    let pending = {
        // Rebuild the immutable source directory each formation round. A
        // nominal rewrite or newly completed closure changes its inputs.
        let directory = if let Some(context) = ledger.original.context() {
            ActualDirectory::indexed(module, program, context, "deferred nominals", limits, left)?
        } else {
            ActualDirectory::from_module_at(module, "deferred nominals")
        };
        let mut eval = Eval {
            input: program,
            module: &directory,
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
                let request = eval.deferred_physical_nominal(instance.id, &recipe, ty, pass, 0);
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
                pass,
                0,
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
                if hint.ty.is_b0_scalar() {
                    continue;
                }
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
                let request = eval.deferred_logical_nominal(site.instance, &recipe, hint, pass, 0);
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
            if hint.ty.is_b0_scalar() {
                continue;
            }
            if let SemanticTypeValue::Core { scope, source } = value {
                match source {
                    SemanticCoreTypeSource::OriginalRecipe(recipe)
                    | SemanticCoreTypeSource::Expression { recipe, .. } => {
                        let request = eval.deferred_logical_nominal(*scope, recipe, hint, pass, 0);
                        eval.deferred_result(request, &mut blocked)?;
                    }
                    SemanticCoreTypeSource::OriginalExpression { source, .. } => {
                        let recipe = eval.original_type_view(
                            *scope,
                            SemanticScopeRef::Ancestor(0),
                            source,
                            0,
                        )?;
                        let request = eval.deferred_logical_nominal(*scope, &recipe, hint, pass, 0);
                        eval.deferred_result(request, &mut blocked)?;
                    }
                }
            }
        }
        directory.nominal_proposals.into_inner()
    };
    let mut replacements = Vec::new();
    let mut changed = false;
    // All proposals are complete before any actual payload is rewritten.
    // Repeated use sites must select exactly one original full application.
    for index in 0..pending.len() {
        let (before, row) = &pending[index];
        inference::charge_type_snapshot(before, Span::default(), limits, left, 1)?;
        charge_row(row, limits, left)?;
        for (other_before, other) in &pending[..index] {
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
    for (before, row) in pending {
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
    let representation_changed = !replacements.is_empty();
    rewrite::apply(program, module, ledger, &replacements, limits, left)?;
    Ok(NominalFormation {
        catalogue_changed: changed,
        representation_changed,
        state: if blocked.is_empty() {
            Formation::Complete(())
        } else {
            Formation::NotReady {
                dependencies: blocked,
            }
        },
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deferred_nominals_require_complete_types_and_the_actual_declaration() {
        let limits = Limits::default();
        let mut left = limits.max_analysis_steps;
        let declaration = LifeDeclarationId {
            module: zil_base::ModuleId(0),
            declaration: zil_base::NodeId(1),
            compiler: false,
        };
        let record = |leaf| Type::Record {
            name: "Same".into(),
            fields: vec![("item".into(), leaf)],
        };
        let row = |ty| LifeNominalInstance {
            declaration,
            ty,
            arguments: Vec::new(),
        };
        let committed = [row(record(Type::I32))];
        let directory = ActualDirectory::building(
            Vec::new(),
            Vec::new(),
            &committed,
            &[],
            &[],
            BTreeMap::new(),
            &limits,
            &mut left,
        )
        .unwrap();
        let program = SemanticModule {
            definitions: Vec::new(),
            instances: Vec::new(),
            sites: Vec::new(),
            dyn_scopes: Vec::new(),
        };
        let ledger = Ledger::default();
        let hint = record(Type::Bool);
        let key = NominalIndex::<NominalPosition>::key(&hint, &limits, &mut left).unwrap();
        assert_eq!(
            key,
            NominalIndex::<NominalPosition>::key(&committed[0].ty, &limits, &mut left).unwrap(),
        );
        let mut index = NominalIndex::default();
        index
            .insert(key, NominalPosition::Committed(0), &mut left)
            .unwrap();
        // Deliberately place another complete Type in the same bucket.
        *directory.deferred_nominals.borrow_mut() = Some(index);
        let mut eval = Eval {
            input: &program,
            module: &directory,
            ledger: &ledger,
            limits: &limits,
            left: &mut left,
            active: BTreeSet::new(),
            invocation_probe: None,
            pending_header: None,
        };
        assert!(eval.existing_deferred_nominal(&hint, declaration).unwrap().is_none());
        let formed = Type::BorrowedStorage {
            storage: Box::new(hint.clone()),
            life: empty(),
        };
        eval.propose_deferred_nominal(hint.clone(), row(formed.clone()))
            .unwrap();
        assert!(eval.existing_deferred_nominal(&hint, declaration).unwrap().is_some());
        assert!(eval.existing_deferred_nominal(&formed, declaration).unwrap().is_some());
        let other = LifeDeclarationId {
            declaration: zil_base::NodeId(2),
            ..declaration
        };
        assert!(eval.existing_deferred_nominal(&formed, other).is_err());
        eval.propose_deferred_nominal(hint.clone(), row(formed))
            .unwrap();
        assert!(eval.existing_deferred_nominal(&hint, declaration).is_err());
    }
}
