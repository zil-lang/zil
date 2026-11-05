//! Runtime CT recipes originate at the actual specialization operation. The
//! driver independently checks these against its immutable source inventory.
use super::*;
use crate::runtime_ct::{RuntimeArgument, RuntimeCallSite, RuntimeDefinition, RuntimeInstance};

struct Expressions<'a, 'b> {
    budget: &'a mut Budget<'b>,
    nodes: BTreeMap<usize, u32>,
    next: u32,
    has_ct: bool,
    meta: BTreeMap<usize, u32>,
}
impl crate::ct_selection::Visitor for Expressions<'_, '_> {
    fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
        self.budget.node(span, depth).map_err(|e| vec![e])
    }
    fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
        self.budget.text(text, span).map_err(|e| vec![e])
    }
    fn expression(&mut self, expression: &Expr, depth: usize) -> CompileResult<()> {
        self.budget
            .node(expression.span, depth)
            .map_err(|e| vec![e])?;
        let ordinal = self.next;
        self.next = self.next.checked_add(1).ok_or_else(|| {
            vec![error(
                expression.span,
                "runtime CT expression ordinal overflow",
            )]
        })?;
        self.has_ct |= matches!(
            expression.kind,
            ExprKind::MetaEval(_) | ExprKind::MetaBlock(_)
        );
        if matches!(expression.kind, ExprKind::MetaEval(_)) {
            self.budget
                .lineage_bytes(std::mem::size_of::<(usize, u32)>(), expression.span)
                .map_err(|e| vec![e])?;
            let meta_ordinal = u32::try_from(self.meta.len())
                .map_err(|_| vec![error(expression.span, "runtime CT meta ordinal overflow")])?;
            self.meta
                .insert(expression as *const Expr as usize, meta_ordinal);
        }
        self.budget
            .identity_work(
                1 + self.nodes.len().checked_ilog2().unwrap_or(0) as usize,
                expression.span,
            )
            .map_err(|e| vec![e])?;
        self.budget
            .lineage_bytes(std::mem::size_of::<(usize, u32)>(), expression.span)
            .map_err(|e| vec![e])?;
        self.nodes
            .insert(expression as *const Expr as usize, ordinal);
        crate::ct_selection::walk_expression(self, expression, depth)
    }
}

// Computed CT types carry frozen source nominal names. Ordinary generic
// substitution deliberately replaces only bare type variables, so applied
// nominals need their own origin-backed binding relocation.
fn relocate_computed_nominals(
    ty: &mut TypeRef,
    bindings: &BTreeMap<String, TypeRef>,
    budget: &mut Budget<'_>,
    depth: usize,
) -> R<()> {
    budget.node(ty.span, depth)?;
    match &mut ty.kind {
        TypeKind::Named { path, args } => {
            if path.len() == 1 {
                budget.text(&path[0], ty.span)?;
                if let Some(binding) = bindings.get(&path[0]).and_then(trait_bounds::plain) {
                    budget.text(binding, ty.span)?;
                    path[0] = binding.to_owned();
                }
            }
            for argument in args {
                budget.node(ty.span, depth)?;
                if let GenericArg::Type(inner) = argument {
                    relocate_computed_nominals(inner, bindings, budget, depth + 1)?;
                }
            }
        }
        TypeKind::Tuple(parts) => {
            for part in parts {
                relocate_computed_nominals(part, bindings, budget, depth + 1)?;
            }
        }
        TypeKind::Array { element, .. } | TypeKind::Group(element) => {
            relocate_computed_nominals(element, bindings, budget, depth + 1)?;
        }
        _ => {
            return Err(unsupported(
                ty.span,
                "computed CT type needs a source nominal relocation adapter",
            ));
        }
    }
    Ok(())
}

pub(super) fn symbolic_types(
    item: &Item,
    origin: Option<origins::Origin>,
    origins: Option<&origins::Origins>,
    module: &Module,
    callback: Option<&dyn crate::runtime_ct::RuntimeCt>,
    budget: &mut Budget<'_>,
) -> R<Option<BTreeMap<usize, TypeRef>>> {
    let Some(callback) = callback else {
        return Ok(None);
    };
    let ItemKind::Function(function) = &item.kind else {
        return Ok(None);
    };
    let mut visitor = Expressions {
        budget,
        nodes: BTreeMap::new(),
        next: 0,
        has_ct: false,
        meta: BTreeMap::new(),
    };
    crate::ct_selection::Visitor::item(&mut visitor, item, 0).map_err(|mut e| e.remove(0))?;
    if !visitor.has_ct {
        return Ok(None);
    }
    let meta = visitor.meta;
    if meta.is_empty() {
        return Err(unsupported(
            item.span,
            "runtime generic CT blocks need symbolic result checking",
        ));
    }
    let origin =
        origin.ok_or_else(|| error(item.span, "symbolic CT source declaration is absent"))?;
    let origins =
        origins.ok_or_else(|| error(item.span, "symbolic CT source inventory is absent"))?;
    let row = origins.identity(origin.declaration, budget, item.span)?;
    origins::pay_identity(&row.identity, budget, item.span)?;
    origins::pay_declaration_copy(row, budget, item.span)?;
    let definition = RuntimeDefinition {
        module: origin.declaration.module,
        origin: row.clone(),
        member: origin.member,
    };
    let body_ids = trait_instances::expression_ids(&function.body, budget)?;
    let mut substitutions = BTreeMap::new();
    // The actual normalizer's declaration ledger relates a source nominal to
    // its current binding. No generated spelling is interpreted as an owner.
    for (ordinal, candidate) in module.items.iter().enumerate() {
        budget.node(candidate.span, 0)?;
        if !matches!(candidate.kind, ItemKind::Aggregate(_) | ItemKind::Enum(_)) {
            continue;
        }
        let candidate_origin = origins.get(ordinal, candidate.span)?;
        if candidate_origin.compiler || candidate_origin.declaration.module != definition.module {
            continue;
        }
        let original = origins.identity(candidate_origin.declaration, budget, candidate.span)?;
        let Some(source_name) = original.identity.declaration_path.get(1) else {
            continue;
        };
        if function.generics.iter().any(|p| p.name == *source_name) {
            continue;
        }
        if let Some(normalized) = name(candidate) {
            budget.text(source_name, candidate.span)?;
            budget.text(normalized, candidate.span)?;
            substitutions.insert(source_name.clone(), named(normalized, candidate.span));
        }
    }
    for (ordinal, parameter) in function
        .generics
        .iter()
        .filter(|p| matches!(p.kind, GenericParamKind::Const(_)))
        .enumerate()
    {
        budget.text(&parameter.name, parameter.span)?;
        let value = Expr {
            span: parameter.span,
            kind: ExprKind::Name(consts::symbol(ordinal)),
        };
        substitutions.insert(parameter.name.clone(), consts::carrier(&value, budget)?);
    }
    let mut output = BTreeMap::new();
    for (address, source_ordinal) in meta {
        let body_ordinal = body_ids.get(&address).ok_or_else(|| {
            unsupported(
                item.span,
                "runtime CT in generic signature needs symbolic signature checking",
            )
        })?;
        let mut remaining = budget.remaining_limits();
        let mut result = callback
            .definition_expression(&definition, source_ordinal, &mut remaining)
            .map_err(|mut e| e.remove(0))?;
        budget.adopt_remaining(&remaining);
        relocate_computed_nominals(&mut result, &substitutions, budget, 0)?;
        budget.substituted_type(&result, &substitutions)?;
        Substitution {
            substitutions: &substitutions,
        }
        .ty(&mut result);
        budget.lineage_bytes(std::mem::size_of::<(usize, TypeRef)>(), item.span)?;
        output.insert(*body_ordinal, result);
    }
    Ok(Some(output))
}

impl Engine<'_> {
    pub(super) fn runtime_call_anchors(&mut self, item: &Item) -> R<BTreeMap<usize, u32>> {
        let mut anchors = BTreeMap::new();
        if self.runtime_ct.is_none() {
            return Ok(anchors);
        }
        crate::runtime_ct::observe_runtime_calls(item, self.limits.max_call_depth, |event| {
            match event {
                crate::runtime_ct::RuntimeCallEvent::Work(work) => {
                    self.budget.identity_work(work, item.span)
                }
                crate::runtime_ct::RuntimeCallEvent::Call {
                    expression,
                    ordinal,
                } => {
                    self.budget
                        .lineage_bytes(std::mem::size_of::<(usize, u32)>(), expression.span)
                        .map_err(|e| vec![e])?;
                    let ExprKind::Call { callee, .. } = &expression.kind else {
                        unreachable!("runtime call visitor emits calls")
                    };
                    anchors.insert(callee.as_ref() as *const Expr as usize, ordinal);
                    Ok(())
                }
            }
            .map_err(|e| vec![e])
        })
        .map_err(|mut e| e.remove(0))?;
        Ok(anchors)
    }

    fn runtime_definition(
        &mut self,
        origin: Option<origins::Origin>,
        span: Span,
    ) -> R<RuntimeDefinition> {
        let origin =
            origin.ok_or_else(|| error(span, "runtime CT original declaration is absent"))?;
        if origin.compiler || origin.provider {
            return Err(unsupported(
                span,
                "runtime CT needs an actual source declaration",
            ));
        }
        let origins = self
            .origins
            .ok_or_else(|| error(span, "runtime CT source inventory is absent"))?;
        let identity = origins.identity(origin.declaration, &mut self.budget, span)?;
        origins::pay_identity(&identity.identity, &mut self.budget, span)?;
        origins::pay_declaration_copy(identity, &mut self.budget, span)?;
        Ok(RuntimeDefinition {
            module: origin.declaration.module,
            origin: identity.clone(),
            member: origin.member,
        })
    }

    pub(super) fn runtime_materialize(
        &mut self,
        template: &Item,
        substitution: &BTreeMap<String, TypeRef>,
        generated: &str,
        instance: &mut Item,
    ) -> R<()> {
        let Some(callback) = self.runtime_ct else {
            return Ok(());
        };
        let ItemKind::Function(function) = &template.kind else {
            return Ok(());
        };
        let mut visitor = Expressions {
            budget: &mut self.budget,
            nodes: BTreeMap::new(),
            next: 0,
            has_ct: false,
            meta: BTreeMap::new(),
        };
        crate::ct_selection::Visitor::item(&mut visitor, template, 0)
            .map_err(|mut e| e.remove(0))?;
        let has_ct = visitor.has_ct;
        let origin = self.template_origin(template)?;
        if !has_ct
            && (self.runtime_call.is_none()
                || origin.is_none_or(|o| o.compiler || o.provider)
                || self.current_origin.is_none_or(|o| o.compiler || o.provider))
        {
            return Ok(());
        }
        let definition = self.runtime_definition(origin, template.span)?;
        let caller = self.runtime_definition(self.current_origin, template.span)?;
        let call_ordinal = self
            .runtime_call
            .ok_or_else(|| error(template.span, "runtime CT actual call site is absent"))?;
        let mut arguments = Vec::new();
        for (ordinal, parameter) in function.generics.iter().enumerate() {
            self.budget.node(parameter.span, 0)?;
            self.budget
                .lineage_bytes(std::mem::size_of::<RuntimeArgument>(), parameter.span)?;
            arguments.push(match &parameter.kind {
                GenericParamKind::Lifetime => RuntimeArgument::LifeDefinition {
                    ordinal: u32::try_from(ordinal).map_err(|_| {
                        error(parameter.span, "runtime CT parameter ordinal overflow")
                    })?,
                },
                GenericParamKind::Type(_) => {
                    let Some(ty) = substitution.get(&parameter.name) else {
                        if !has_ct {
                            return Ok(());
                        }
                        return Err(unsupported(
                            parameter.span,
                            "runtime CT retained type binder needs a concrete instance",
                        ));
                    };
                    self.budget.ty(ty)?;
                    RuntimeArgument::Type(ty.clone())
                }
                GenericParamKind::Const(_) => {
                    let ty = substitution.get(&parameter.name).ok_or_else(|| {
                        error(parameter.span, "runtime CT const actual is absent")
                    })?;
                    let value = consts::expression(ty).ok_or_else(|| {
                        error(
                            parameter.span,
                            "runtime CT const actual is not a constant recipe",
                        )
                    })?;
                    self.budget.const_expression(value)?;
                    RuntimeArgument::Const(value.clone())
                }
            });
        }
        let id = self.runtime_next_instance;
        self.runtime_next_instance = id
            .checked_add(1)
            .ok_or_else(|| error(template.span, "runtime CT instance ordinal overflow"))?;
        let recipe = RuntimeInstance {
            id,
            definition,
            arguments,
            call: RuntimeCallSite {
                caller,
                parent_instance: self.runtime_instance,
                call_ordinal,
            },
        };
        if has_ct {
            let mut remaining = self.budget.remaining_limits();
            callback
                .instantiated(
                    &recipe,
                    &self.products.runtime_instances,
                    template,
                    instance,
                    &mut remaining,
                )
                .map_err(|mut e| e.remove(0))?;
            self.budget.adopt_remaining(&remaining);
        }
        let mut visitor = Expressions {
            budget: &mut self.budget,
            nodes: BTreeMap::new(),
            next: 0,
            has_ct: false,
            meta: BTreeMap::new(),
        };
        crate::ct_selection::Visitor::item(&mut visitor, instance, 0)
            .map_err(|mut e| e.remove(0))?;
        if visitor.has_ct {
            return Err(error(
                instance.span,
                "runtime CT consumer left a deferred expression in the instance",
            ));
        }
        self.budget.text(generated, template.span)?;
        self.budget
            .lineage_bytes(std::mem::size_of::<(String, u32)>(), template.span)?;
        self.runtime_instances.insert(generated.to_owned(), id);
        self.budget
            .lineage_bytes(std::mem::size_of::<RuntimeInstance>(), template.span)?;
        self.products.runtime_instances.push(recipe);
        Ok(())
    }
}
