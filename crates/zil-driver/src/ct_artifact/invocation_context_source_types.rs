//! Independent admission from immutable original syntax. Private producer
//! events locate sites; their claimed graphs and actual vectors prove nothing.
use super::*;
use crate::ct_artifact::compiler::{
    SourceCompilation, SourceInstanceCall,
    source_sites::{self, LocatedNode, Selection, SourceNode, SourceSite, SourceTreeId},
};

#[derive(Clone)]
pub(super) struct AuthenticatedTypeSite {
    function: CtFunctionId,
    definition: CtSymbolId,
    value: CtValueId,
    graph: CtTypeRef,
}
#[derive(Clone)]
pub(super) struct AuthenticatedSchemaSite {
    enumeration: bool,
    schema: u32,
    graph: CtTypeRef,
}
struct NominalEnvironment<'a> {
    definition: &'a CtSymbolId,
    parameters: &'a [s::GenericParam],
    arguments: &'a [CtSchemaArgument],
    graph: Option<&'a CtTypeRef>,
}
pub(super) struct SourceEnvironment<'a> {
    nominal: Option<NominalEnvironment<'a>>,
    function: &'a s::Function,
    scope: &'a CtAmbientLifeScope,
    scope_id: CtAmbientScopeId,
    host: Option<(
        &'a [s::GenericParam],
        &'a CtAmbientLifeScope,
        CtAmbientScopeId,
    )>,
}
pub(super) fn step(budget: &mut CtExecutionBudget, depth: usize) -> CompileResult<()> {
    budget.reserve_analysis_work(1)?;
    if depth > budget.remaining().max_call_depth as usize {
        return Err(vec![Diagnostic::limit(
            Phase::Ct,
            "CT source authentication depth exceeded",
            None,
        )]);
    }
    Ok(())
}
fn eq_text(left: &str, right: &str, budget: &mut CtExecutionBudget) -> CompileResult<bool> {
    budget.reserve_analysis_work(left.len().min(right.len()) as u64 + 1)?;
    Ok(left == right)
}
pub(super) fn scalar_domain(
    ty: &s::TypeRef,
    budget: &mut CtExecutionBudget,
) -> CompileResult<CtScalarTy> {
    let mut ty = ty;
    let mut depth = 0;
    while let s::TypeKind::Group(inner) = &ty.kind {
        step(budget, depth)?;
        depth += 1;
        ty = inner;
    }
    step(budget, depth)?;
    let s::TypeKind::Named { path, args } = &ty.kind else {
        return Err(unsupported(
            "CT source const domain is not an explicit scalar",
        ));
    };
    budget.reserve_analysis_work(path.len() as u64 + args.len() as u64)?;
    let [name] = path.as_slice() else {
        return Err(unsupported("CT source const domain is not a builtin"));
    };
    budget.reserve_analysis_work(name.len() as u64)?;
    if !args.is_empty() {
        return Err(unsupported("CT source const domain has actuals"));
    }
    match name.as_str() {
        "bool" => Ok(CtScalarTy::Bool),
        "u8" => Ok(CtScalarTy::U8),
        "u16" => Ok(CtScalarTy::U16),
        "u64" => Ok(CtScalarTy::U64),
        "usize" => Ok(CtScalarTy::Usize),
        "i32" => Ok(CtScalarTy::I32),
        "i64" => Ok(CtScalarTy::I64),
        _ => Err(unsupported(
            "CT source const domain is outside the scalar profile",
        )),
    }
}
fn primitive(name: &str) -> Option<CtScalarTy> {
    Some(match name {
        "void" => CtScalarTy::Unit,
        "bool" => CtScalarTy::Bool,
        "u8" => CtScalarTy::U8,
        "u16" => CtScalarTy::U16,
        "i32" => CtScalarTy::I32,
        "i64" => CtScalarTy::I64,
        "u64" => CtScalarTy::U64,
        "usize" => CtScalarTy::Usize,
        "f32" => CtScalarTy::F32,
        "f64" => CtScalarTy::F64,
        _ => return None,
    })
}
fn parameter<'a>(
    env: &'a SourceEnvironment<'a>,
    name: &str,
    budget: &mut CtExecutionBudget,
) -> CompileResult<(
    usize,
    &'a s::GenericParam,
    &'a CtAmbientLifeScope,
    CtAmbientScopeId,
)> {
    for (i, p) in env.function.generics.iter().enumerate().rev() {
        if eq_text(name, &p.name, budget)? {
            return Ok((i, p, env.scope, env.scope_id));
        }
    }
    if let Some((generics, scope, id)) = env.host {
        for (i, p) in generics.iter().enumerate().rev() {
            if eq_text(name, &p.name, budget)? {
                return Ok((i, p, scope, id));
            }
        }
    }
    Err(unsupported(
        "CT source name is not an original complete generic ordinal",
    ))
}
impl FrozenInvocationContext {
    pub(super) fn source_location<'a>(
        &'a self,
        site: SourceSite,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<LocatedNode<'a>> {
        let module = self.source_site_module(site, budget)?;
        source_sites::select_in_tree(
            module,
            site.tree,
            Selection::Site(site),
            budget.remaining().max_call_depth as usize,
            |n| budget.reserve_analysis_work(n as u64),
        )?
        .ok_or_else(|| unsupported("CT source event has no actual frozen structural site"))
    }
    fn source_site_module(
        &self,
        site: SourceSite,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<&s::Module> {
        match site.tree {
            SourceTreeId::Original => {
                budget.reserve_analysis_work(self.modules.len() as u64)?;
                self.modules
                    .iter()
                    .find(|m| m.module.id == site.module)
                    .map(|m| &m.module)
                    .ok_or_else(|| unsupported("CT original source module absent"))
            }
            SourceTreeId::Generated(row) => {
                budget.reserve_analysis_work(1)?;
                let tree = self
                    .generated_catalog
                    .as_ref()
                    .and_then(|c| c.get(row))
                    .ok_or_else(|| {
                        unsupported(
                            "CT generated source site has no authenticated frozen catalog row",
                        )
                    })?;
                if tree.identity.module() != site.module || tree.module.id != site.module {
                    return Err(unsupported(
                        "CT generated source site differs from its actual module",
                    ));
                }
                Ok(&tree.module)
            }
        }
    }
    fn validate_entry_slice(
        &self,
        site: SourceSite,
        definition: &CtSymbolId,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        budget.reserve_analysis_work(32)?;
        if definition != &self.entry_definition {
            return Ok(());
        }
        let Some(root_site) = self.entry_site else {
            return Ok(());
        };
        if root_site.tree != site.tree || root_site.module != site.module {
            return Err(unsupported(
                "CT source event differs from the actual frozen invocation tree",
            ));
        }
        let root = self.source_location(root_site, budget)?;
        let candidate = self.source_location(site, budget)?;
        let module = self.source_site_module(root_site, budget)?;
        if !source_sites::contains_node(
            module,
            root.node,
            candidate.node,
            budget.remaining().max_call_depth as usize,
            |n| budget.reserve_analysis_work(n as u64),
        )? {
            return Err(unsupported(
                "CT source event is outside the actual frozen invocation expression",
            ));
        }
        Ok(())
    }
    pub(super) fn source_environment<'a>(
        &'a self,
        scope: &'a CtAmbientLifeScope,
        scope_id: CtAmbientScopeId,
        scopes: &'a [CtAmbientLifeScope],
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<SourceEnvironment<'a>> {
        let function = self.original_scope_function(scope, budget)?;
        let host = if let Some(parent_id) = scope.enclosing {
            let parent = scopes
                .get(parent_id.0 as usize)
                .ok_or_else(|| unsupported("CT source enclosing host scope is absent"))?;
            self.validate_scope(parent, budget)?;
            budget.reserve_analysis_work(64)?;
            let (module, original, tree) =
                self.source_function_source(&scope.declaration, budget)?;
            let local_parent = original
                .lexical_parent
                .ok_or_else(|| unsupported("CT source member has no actual lexical host"))?;
            let expected_parent = if let Some(tree) = tree {
                *tree
                    .declarations
                    .get(local_parent.0 as usize)
                    .ok_or_else(|| {
                        unsupported("CT generated lexical host output placement absent")
                    })?
            } else {
                local_parent
            };
            if parent.declaration.module != scope.declaration.module
                || parent.declaration.generation_path != scope.declaration.generation_path
                || parent.declaration.declaration != expected_parent
                || parent.enclosing.is_some()
            {
                return Err(unsupported(
                    "CT source member enclosing scope is not its actual lexical host",
                ));
            }
            let original_parent = super::super::compiler::source_scope::original_declaration(
                module,
                local_parent,
                budget.remaining().max_call_depth as usize,
                |n| budget.reserve_analysis_work(n as u64),
            )?
            .ok_or_else(|| unsupported("CT source lexical host declaration absent"))?;
            let s::ItemKind::Aggregate(aggregate) = &original_parent.item.kind else {
                return Err(unsupported(
                    "CT source lexical host is not an original aggregate",
                ));
            };
            Some((aggregate.generics.as_slice(), parent, parent_id))
        } else {
            None
        };
        Ok(SourceEnvironment {
            nominal: None,
            function,
            scope,
            scope_id,
            host,
        })
    }
    fn original_scope_function<'a>(
        &'a self,
        scope: &CtAmbientLifeScope,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<&'a s::Function> {
        Ok(self
            .source_function_source(&scope.declaration, budget)?
            .1
            .function)
    }
    pub(super) fn source_function_source<'a>(
        &'a self,
        symbol: &CtSymbolId,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<(
        &'a s::Module,
        super::super::compiler::source_scope::OriginalFunction<'a>,
        Option<&'a zil_expand::source_context::GeneratedSourceTree>,
    )> {
        if symbol.generation_path.is_some() {
            let catalog = self
                .generated_catalog
                .as_ref()
                .ok_or_else(|| unsupported("CT generated function has no frozen source catalog"))?;
            let (row, original) =
                super::super::compiler::source_scope::generated_original_function(
                    catalog,
                    symbol,
                    budget.remaining().max_call_depth as usize,
                    |n| budget.reserve_analysis_work(n as u64),
                )?
                .ok_or_else(|| {
                    unsupported("CT generated function has no actual original declaration")
                })?;
            let tree = &*catalog.trees[row];
            Ok((&tree.module, original, Some(tree)))
        } else {
            budget.reserve_analysis_work(self.modules.len() as u64)?;
            let module = self
                .modules
                .iter()
                .find(|m| m.module.id == symbol.module)
                .ok_or_else(|| unsupported("CT source owner module absent"))?;
            let original = super::super::compiler::source_scope::original_function(
                &module.module,
                symbol.declaration,
                budget.remaining().max_call_depth as usize,
                |n| budget.reserve_analysis_work(n as u64),
            )?
            .ok_or_else(|| unsupported("CT source owner is not an original function"))?;
            Ok((&module.module, original, None))
        }
    }
    pub(super) fn source_symbol<'a>(
        &'a self,
        requester: &CtSymbolId,
        module_id: zil_base::ModuleId,
        path: &[String],
        index: &SourceIndex,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<(CtSymbolId, &'a s::Item)> {
        budget.reserve_analysis_work(path.len() as u64 + index.symbol_lookup_work())?;
        for name in path {
            budget.reserve_analysis_work(name.len() as u64)?;
        }
        let (module_id, path) = if path.len() > 1 {
            if let Some(id) = index.imported_module(module_id, &path[0]) {
                (id, &path[1..])
            } else {
                (module_id, path)
            }
        } else {
            (module_id, path)
        };
        budget.reserve_analysis_work(self.modules.len() as u64)?;
        let module = self
            .modules
            .iter()
            .find(|p| p.module.id == module_id)
            .ok_or_else(|| unsupported("CT source path module absent"))?;
        let name = path
            .first()
            .ok_or_else(|| unsupported("CT source path empty"))?;
        budget.reserve_analysis_work(
            self.generated_catalog.as_ref().map_or(0, |c| c.trees.len()) as u64,
        )?;
        let mut selected = None;
        let mut scan = |syntax: &'a s::Module,
                        tree: Option<&'a zil_expand::source_context::GeneratedSourceTree>|
         -> CompileResult<()> {
            for (ordinal, item) in syntax.items.iter().enumerate() {
                budget.reserve_analysis_work(1)?;
                let original_name = match &item.kind {
                    s::ItemKind::Aggregate(a) => Some(&a.name),
                    s::ItemKind::Enum(e) => Some(&e.name),
                    s::ItemKind::Function(f) => Some(&f.name),
                    s::ItemKind::TypeAlias(a) => Some(&a.name),
                    _ => None,
                };
                if let Some(original_name) = original_name
                    && eq_text(name, original_name, budget)?
                    && selected.replace((ordinal, item, syntax, tree)).is_some()
                {
                    return Err(unsupported("CT source declaration path is ambiguous"));
                }
            }
            Ok(())
        };
        scan(&module.module, None)?;
        if let Some(catalog) = &self.generated_catalog {
            for tree in &catalog.trees {
                if tree.identity.module() == module_id {
                    scan(&tree.module, Some(tree))?;
                }
            }
        }
        drop(scan);
        let (ordinal, mut item, syntax, tree) = selected
            .ok_or_else(|| unsupported("CT source path has no actual original declaration"))?;
        let mut local = zil_base::NodeId(ordinal as u32);
        if path.len() == 2 {
            let s::ItemKind::Aggregate(host) = &item.kind else {
                return Err(unsupported("CT source member has no original host"));
            };
            let mut member = None;
            for candidate in &host.methods {
                budget.reserve_analysis_work(1)?;
                if let s::ItemKind::Function(f) = &candidate.kind
                    && eq_text(&path[1], &f.name, budget)?
                    && member.replace(candidate).is_some()
                {
                    return Err(unsupported("CT source member path is ambiguous"));
                }
            }
            item = member.ok_or_else(|| unsupported("CT source path has no original member"))?;
            let original = super::super::compiler::source_scope::original_item(
                syntax,
                item,
                budget.remaining().max_call_depth as usize,
                |n| budget.reserve_analysis_work(n as u64),
            )?
            .ok_or_else(|| unsupported("CT source member original ordinal absent"))?;
            local = original.declaration;
        } else if path.len() != 1 {
            return Err(unsupported(
                "CT source path requires a resolved original declaration",
            ));
        }
        let symbol = if let Some(tree) = tree {
            let work = match &tree.identity {
                zil_expand::source_context::SourceTreeIdentity::Generated {
                    event_path, ..
                } => event_path.len() as u64 * 4 + 32,
                _ => 0,
            };
            budget.reserve_analysis_work(work + 1)?;
            CtSymbolId {
                module: module_id,
                declaration: *tree.declarations.get(local.0 as usize).ok_or_else(|| {
                    unsupported("CT generated source declaration output placement absent")
                })?,
                generation_path: tree.generation_fingerprint(),
            }
        } else {
            CtSymbolId {
                module: module_id,
                declaration: local,
                generation_path: None,
            }
        };
        budget.reserve_analysis_work(index.symbol_lookup_work())?;
        if !index.visible(requester, &symbol) {
            return Err(unsupported(
                "CT source declaration is not originally visible",
            ));
        }
        Ok((symbol, item))
    }
    pub(super) fn source_scalar(
        &self,
        arg: &s::GenericArg,
        domain: CtScalarTy,
        env: &SourceEnvironment<'_>,
        budget: &mut CtExecutionBudget,
        depth: usize,
    ) -> CompileResult<zil_ir::Constant> {
        step(budget, depth)?;
        let binder = match arg {
            s::GenericArg::Type(t) => match &t.kind {
                s::TypeKind::Named { path, args } if path.len() == 1 && args.is_empty() => {
                    Some(path[0].as_str())
                }
                _ => None,
            },
            s::GenericArg::Const(e) => match &e.kind {
                s::ExprKind::Name(n) => Some(n.as_str()),
                _ => None,
            },
            _ => None,
        };
        if let Some(name) = binder {
            if env.nominal.is_some() {
                return self.nominal_scalar(name, domain, env, budget);
            }
            let (ordinal, p, actual_scope, _) = parameter(env, name, budget)?;
            let s::GenericParamKind::Const(ty) = &p.kind else {
                return Err(unsupported(
                    "CT source scalar name selects another generic kind",
                ));
            };
            if scalar_domain(ty, budget)? != domain {
                return Err(unsupported("CT source scalar binder domain differs"));
            }
            let CtScopeInstance::Instantiated(actuals) = &actual_scope.instance else {
                return Err(unsupported(
                    "CT source scalar binder lacks its actual instance",
                ));
            };
            return match actuals.get(ordinal) {
                Some(CtScopeActual::Scalar(value)) => {
                    budget.reserve_analysis_work(32)?;
                    Ok(value.clone())
                }
                _ => Err(unsupported("CT source scalar instance ordinal differs")),
            };
        }
        let s::GenericArg::Const(e) = arg else {
            return Err(unsupported(
                "CT source scalar has no original value expression",
            ));
        };
        match &e.kind {
            s::ExprKind::Literal(s::Literal::Bool(v)) if domain == CtScalarTy::Bool => {
                Ok(zil_ir::Constant::Bool(*v))
            }
            s::ExprKind::Literal(s::Literal::Integer(text)) => {
                budget.reserve_analysis_work(text.len() as u64)?;
                let runtime = match domain {
                    CtScalarTy::U8 => zil_ir::Type::U8,
                    CtScalarTy::U16 => zil_ir::Type::U16,
                    CtScalarTy::U64 => zil_ir::Type::U64,
                    CtScalarTy::Usize => zil_ir::Type::Usize,
                    CtScalarTy::I32 => zil_ir::Type::I32,
                    CtScalarTy::I64 => zil_ir::Type::I64,
                    _ => return Err(unsupported("CT source integer domain differs")),
                };
                zil_types::parse_integer_literal(text, Some(&runtime), false, e.span)
            }
            _ => Err(unsupported(
                "CT source scalar requires an independently known literal or instance binder",
            )),
        }
    }
    fn source_lifetime(
        &self,
        name: &str,
        env: &SourceEnvironment<'_>,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtAmbientLifeReference> {
        if let Some(nominal) = &env.nominal {
            let (parameter, actual) =
                self.nominal_parameter(name, nominal, budget)?
                    .ok_or_else(|| {
                        unsupported("CT nominal field life is not an original complete parameter")
                    })?;
            if !matches!(parameter.kind, s::GenericParamKind::Lifetime) {
                return Err(unsupported(
                    "CT nominal field life selects another complete generic kind",
                ));
            }
            let CtSchemaArgument::Lifetime(actual) = actual else {
                return Err(unsupported("CT nominal field life actual has another kind"));
            };
            let graph = nominal.graph.ok_or_else(|| {
                unsupported("CT nominal field life has no authentic parent graph")
            })?;
            return self.graph_lifetime(graph, *actual, budget);
        }
        let (ordinal, p, _, scope_id) = parameter(env, name, budget)?;
        if !matches!(p.kind, s::GenericParamKind::Lifetime) {
            return Err(unsupported(
                "CT source lifetime selects another complete generic kind",
            ));
        }
        Ok(CtAmbientLifeReference {
            scope: scope_id,
            parameter: ordinal as u32,
        })
    }
    fn nominal_parameter<'a>(
        &self,
        name: &str,
        nominal: &'a NominalEnvironment<'a>,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<Option<(&'a s::GenericParam, &'a CtSchemaArgument)>> {
        for (ordinal, parameter) in nominal.parameters.iter().enumerate().rev() {
            if eq_text(name, &parameter.name, budget)? {
                let actual = nominal.arguments.get(ordinal).ok_or_else(|| {
                    unsupported("CT nominal field actual loses its original full ordinal")
                })?;
                return Ok(Some((parameter, actual)));
            }
        }
        Ok(None)
    }
    fn nominal_scalar(
        &self,
        name: &str,
        domain: CtScalarTy,
        env: &SourceEnvironment<'_>,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<zil_ir::Constant> {
        let nominal = env.nominal.as_ref().ok_or_else(|| {
            unsupported("CT nominal scalar has no authentic parameter environment")
        })?;
        let (parameter, actual) =
            self.nominal_parameter(name, nominal, budget)?
                .ok_or_else(|| {
                    unsupported("CT nominal field scalar is not an original complete parameter")
                })?;
        let s::GenericParamKind::Const(ty) = &parameter.kind else {
            return Err(unsupported(
                "CT nominal field scalar selects another complete generic kind",
            ));
        };
        if scalar_domain(ty, budget)? != domain {
            return Err(unsupported(
                "CT nominal field scalar domain differs from its original parameter",
            ));
        }
        let CtSchemaArgument::Scalar(value) = actual else {
            return Err(unsupported(
                "CT nominal field scalar actual has another kind",
            ));
        };
        budget.reserve_analysis_work(32)?;
        Ok(value.clone())
    }
    fn graph_lifetime(
        &self,
        graph: &CtTypeRef,
        life: CtLifetimeArgument,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtAmbientLifeReference> {
        let zil_ir::StorageBinder::Fixed(class) = life.binder else {
            return Err(unsupported("CT ambient source lifetime is not Fixed"));
        };
        let mut found = None;
        for row in &graph.ambient_classes {
            budget.reserve_analysis_work(1)?;
            if row.owner == life.owner && row.class == class && found.replace(row.source).is_some()
            {
                return Err(unsupported(
                    "CT source lifetime has repeated ambient binding",
                ));
            }
        }
        found.ok_or_else(|| unsupported("CT source lifetime has no ambient binding"))
    }
    fn graph_equal(
        &self,
        left: &CtTypeRef,
        right: &CtTypeRef,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        let cost = super::super::execute::value_context::graph_cost(left, budget)?.saturating_add(
            super::super::execute::value_context::graph_cost(right, budget)?,
        );
        budget.reserve_analysis_work(cost)?;
        if left != right {
            return Err(unsupported(
                "CT source Type instance differs from the actual complete Type argument",
            ));
        }
        Ok(())
    }
    // Keep the source, candidate, scope and request meter explicit at this proof boundary.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn source_type(
        &self,
        source: &s::TypeRef,
        graph: &CtTypeRef,
        id: CtTypeNodeId,
        env: &SourceEnvironment<'_>,
        raw: &CtArtifactProgram,
        index: &SourceIndex,
        budget: &mut CtExecutionBudget,
        depth: usize,
    ) -> CompileResult<()> {
        step(budget, depth)?;
        use s::TypeKind as T;
        if let T::Group(inner) = &source.kind {
            return self.source_type(inner, graph, id, env, raw, index, budget, depth + 1);
        }
        let node = graph
            .nodes
            .get(id.0 as usize)
            .ok_or_else(|| unsupported("CT source Type graph node absent"))?;
        if let T::Named { path, args } = &source.kind {
            return self.source_named(path, args, graph, id, env, raw, index, budget, depth + 1);
        }
        match (&source.kind, node) {
            // Source `()` is an empty tuple; the CT graph canonically stores
            // Unit as a primitive. Only that exact zero-field source qualifies.
            (T::Tuple(types), CtTypeNode::Primitive(CtScalarTy::Unit)) if types.is_empty() => {
                Ok(())
            }
            (T::Tuple(types), CtTypeNode::Tuple(ids))
            | (T::Union(types), CtTypeNode::AnonymousSum(ids)) => {
                budget.reserve_analysis_work(types.len() as u64 + ids.len() as u64)?;
                if types.len() != ids.len() {
                    return Err(unsupported("CT source structural Type arity differs"));
                }
                for (t, id) in types.iter().zip(ids) {
                    self.source_type(t, graph, *id, env, raw, index, budget, depth + 1)?;
                }
                Ok(())
            }
            (
                T::Array {
                    element,
                    length: Some(length),
                },
                CtTypeNode::Array {
                    element: actual,
                    length: count,
                },
            ) => {
                let expected =
                    self.source_scalar_expr(length, CtScalarTy::U64, env, budget, depth + 1)?;
                if expected != zil_ir::Constant::U64(u64::from(*count)) {
                    return Err(unsupported("CT source Array length differs"));
                }
                self.source_type(element, graph, *actual, env, raw, index, budget, depth + 1)
            }
            (
                T::Reference { mutable, inner, .. },
                CtTypeNode::Address {
                    mutable: actual,
                    inner: child,
                },
            ) if mutable == actual => {
                self.source_type(inner, graph, *child, env, raw, index, budget, depth + 1)
            }
            (
                T::Function { params, result },
                CtTypeNode::FunctionPointer {
                    parameters,
                    result: actual,
                },
            ) => {
                budget.reserve_analysis_work(params.len() as u64 + parameters.len() as u64)?;
                if params.len() != parameters.len() {
                    return Err(unsupported("CT source function Type arity differs"));
                }
                for (t, id) in params.iter().zip(parameters) {
                    self.source_type(t, graph, *id, env, raw, index, budget, depth + 1)?;
                }
                self.source_type(result, graph, *actual, env, raw, index, budget, depth + 1)
            }
            (
                T::Error { error, result },
                CtTypeNode::ErrorUnion {
                    error: a,
                    result: b,
                },
            ) => {
                self.source_type(error, graph, *a, env, raw, index, budget, depth + 1)?;
                self.source_type(result, graph, *b, env, raw, index, budget, depth + 1)
            }
            _ => Err(unsupported(
                "CT source contextual Type form has no independent source rule",
            )),
        }
    }
    // Keep the source, candidate, scope and request meter explicit at this proof boundary.
    #[allow(clippy::too_many_arguments)]
    fn source_named(
        &self,
        path: &[String],
        args: &[s::GenericArg],
        graph: &CtTypeRef,
        id: CtTypeNodeId,
        env: &SourceEnvironment<'_>,
        raw: &CtArtifactProgram,
        index: &SourceIndex,
        budget: &mut CtExecutionBudget,
        depth: usize,
    ) -> CompileResult<()> {
        step(budget, depth)?;
        let node = graph
            .nodes
            .get(id.0 as usize)
            .ok_or_else(|| unsupported("CT source Type graph node absent"))?;
        if path.len() == 1 && args.is_empty() {
            budget.reserve_analysis_work(path[0].len() as u64)?;
            if let Some(scalar) = primitive(&path[0]) {
                if node == &CtTypeNode::Primitive(scalar) {
                    return Ok(());
                }
                return Err(unsupported("CT source primitive Type differs"));
            }
            if let Some(nominal) = &env.nominal {
                if let Some((parameter, actual)) =
                    self.nominal_parameter(&path[0], nominal, budget)?
                {
                    if !matches!(parameter.kind, s::GenericParamKind::Type(_)) {
                        return Err(unsupported(
                            "CT nominal field Type name selects another complete generic kind",
                        ));
                    }
                    return match actual {
                        CtSchemaArgument::Type(actual) => self.graph_subtype_equal(
                            graph,
                            id,
                            actual,
                            actual.root,
                            budget,
                            depth + 1,
                        ),
                        CtSchemaArgument::TypeInContext(actual) => {
                            let parent = nominal.graph.ok_or_else(|| {
                                unsupported(
                                    "CT nominal field Type actual has no authentic parent context",
                                )
                            })?;
                            self.graph_subtype_equal(graph, id, parent, *actual, budget, depth + 1)
                        }
                        _ => Err(unsupported(
                            "CT nominal field Type actual has another complete generic kind",
                        )),
                    };
                }
            } else if let Ok((ordinal, p, actual_scope, _)) = parameter(env, &path[0], budget)
                && matches!(p.kind, s::GenericParamKind::Type(_))
            {
                let CtScopeInstance::Instantiated(actuals) = &actual_scope.instance else {
                    return Err(unsupported(
                        "CT source Type parameter lacks an actual instance",
                    ));
                };
                let Some(CtScopeActual::Type(actual)) = actuals.get(ordinal) else {
                    return Err(unsupported("CT source Type actual full ordinal differs"));
                };
                if id == graph.root {
                    return self.graph_equal(graph, actual, budget);
                }
                return self.graph_subtype_equal(graph, id, actual, actual.root, budget, depth + 1);
            }
        }
        let declaration = env
            .nominal
            .as_ref()
            .map_or(&env.scope.declaration, |nominal| nominal.definition);
        let (symbol, item) =
            self.source_symbol(declaration, declaration.module, path, index, budget)?;
        if let s::ItemKind::TypeAlias(alias) = &item.kind {
            if !alias.generics.is_empty() || !args.is_empty() {
                return Err(unsupported(
                    "CT source generic alias needs its original instance",
                ));
            }
            let value = alias
                .value
                .as_ref()
                .ok_or_else(|| unsupported("CT source alias value absent"))?;
            return self.source_type(value, graph, id, env, raw, index, budget, depth + 1);
        }
        let parameters = match &item.kind {
            s::ItemKind::Aggregate(a) => &a.generics,
            s::ItemKind::Enum(e) => &e.generics,
            _ => return Err(unsupported("CT source Type name is not a nominal")),
        };
        let (nominal, wrapper) = match node {
            CtTypeNode::BorrowedStorage { storage, life } => (
                graph
                    .nodes
                    .get(storage.0 as usize)
                    .ok_or_else(|| unsupported("CT source nominal storage absent"))?,
                Some(life),
            ),
            _ => (node, None),
        };
        let CtTypeNode::Nominal {
            symbol: actual_symbol,
            arguments,
        } = nominal
        else {
            return Err(unsupported("CT source nominal Type shape differs"));
        };
        budget.reserve_analysis_work(
            32 + parameters.len() as u64 + args.len() as u64 + arguments.len() as u64,
        )?;
        if actual_symbol != &symbol
            || parameters.len() != args.len()
            || args.len() != arguments.len()
        {
            return Err(unsupported(
                "CT source nominal identity or complete argument arity differs",
            ));
        }
        let mut lifetime_count = 0usize;
        for (ordinal, ((p, arg), actual)) in parameters.iter().zip(args).zip(arguments).enumerate()
        {
            step(budget, depth + 1)?;
            match (&p.kind, arg, actual) {
                (
                    s::GenericParamKind::Type(_),
                    s::GenericArg::Type(t),
                    CtTypeArgument::Type(child),
                ) => self.source_type(t, graph, *child, env, raw, index, budget, depth + 1)?,
                (s::GenericParamKind::Const(domain), _, CtTypeArgument::Scalar(value)) => {
                    let expected = self.source_scalar(
                        arg,
                        scalar_domain(domain, budget)?,
                        env,
                        budget,
                        depth + 1,
                    )?;
                    budget.reserve_analysis_work(32)?;
                    if &expected != value {
                        return Err(unsupported("CT source nominal scalar actual differs"));
                    }
                }
                (
                    s::GenericParamKind::Lifetime,
                    s::GenericArg::Lifetime(name),
                    CtTypeArgument::Lifetime(actual),
                ) => {
                    let expected = self.source_lifetime(name, env, budget)?;
                    if actual.owner != id
                        || self.graph_lifetime(graph, *actual, budget)? != expected
                    {
                        return Err(unsupported(
                            "CT source nominal lifetime differs from its original complete ordinal",
                        ));
                    }
                    let shape = wrapper.ok_or_else(|| {
                        unsupported("CT source life nominal lacks its actual owner")
                    })?;
                    let zil_ir::StorageBinder::Fixed(binder) = actual.binder else {
                        return Err(unsupported("CT source nominal life is not Fixed"));
                    };
                    let mut matched = 0;
                    for row in &shape.nominal_arguments {
                        budget.reserve_analysis_work(row.path.len() as u64 + 1)?;
                        if row.path.is_empty()
                            && row.declaration.module == symbol.module
                            && row.declaration.declaration == symbol.declaration
                            && !row.declaration.compiler
                            && row.argument == ordinal as u32
                            && row.binder == binder
                        {
                            matched += 1;
                        }
                    }
                    if matched != 1 {
                        return Err(unsupported(
                            "CT source nominal life row differs from original declaration and ordinal",
                        ));
                    }
                    lifetime_count += 1;
                }
                _ => {
                    return Err(unsupported(
                        "CT source nominal actual kind differs from the full original ordinal",
                    ));
                }
            }
        }
        if let Some(shape) = wrapper {
            budget.reserve_analysis_work(
                shape.slots.len() as u64 + shape.retained.len() as u64 + 1,
            )?;
            if !shape.slots.is_empty()
                || !shape.retained.is_empty()
                || shape.nominal_arguments.len() != lifetime_count
            {
                return Err(unsupported(
                    "CT source nominal relation contains unsupported extra authority",
                ));
            }
        }
        Ok(())
    }
    fn source_scalar_expr(
        &self,
        e: &s::Expr,
        domain: CtScalarTy,
        env: &SourceEnvironment<'_>,
        budget: &mut CtExecutionBudget,
        depth: usize,
    ) -> CompileResult<zil_ir::Constant> {
        // This tiny borrowed wrapper never clones an AST; source_scalar's
        // literal branch is kept separate below to avoid manufacturing a site.
        step(budget, depth)?;
        match &e.kind {
            s::ExprKind::Group(e) => self.source_scalar_expr(e, domain, env, budget, depth + 1),
            s::ExprKind::Name(name) => {
                if env.nominal.is_some() {
                    return self.nominal_scalar(name, domain, env, budget);
                }
                let (ordinal, p, actual_scope, _) = parameter(env, name, budget)?;
                let s::GenericParamKind::Const(ty) = &p.kind else {
                    return Err(unsupported("CT source scalar name has another kind"));
                };
                if scalar_domain(ty, budget)? != domain {
                    return Err(unsupported("CT source scalar domain differs"));
                }
                let CtScopeInstance::Instantiated(actuals) = &actual_scope.instance else {
                    return Err(unsupported("CT source scalar lacks instance"));
                };
                match actuals.get(ordinal) {
                    Some(CtScopeActual::Scalar(c)) => {
                        budget.reserve_analysis_work(32)?;
                        Ok(c.clone())
                    }
                    _ => Err(unsupported("CT source scalar ordinal differs")),
                }
            }
            s::ExprKind::Literal(s::Literal::Bool(v)) if domain == CtScalarTy::Bool => {
                Ok(zil_ir::Constant::Bool(*v))
            }
            s::ExprKind::Literal(s::Literal::Integer(text)) => {
                budget.reserve_analysis_work(text.len() as u64)?;
                let runtime = match domain {
                    CtScalarTy::U8 => zil_ir::Type::U8,
                    CtScalarTy::U16 => zil_ir::Type::U16,
                    CtScalarTy::U64 => zil_ir::Type::U64,
                    CtScalarTy::Usize => zil_ir::Type::Usize,
                    CtScalarTy::I32 => zil_ir::Type::I32,
                    CtScalarTy::I64 => zil_ir::Type::I64,
                    _ => return Err(unsupported("CT source integer domain differs")),
                };
                zil_types::parse_integer_literal(text, Some(&runtime), false, e.span)
            }
            _ => Err(unsupported(
                "CT source scalar expression requires an independent value rule",
            )),
        }
    }
    fn graph_subtype_equal(
        &self,
        left: &CtTypeRef,
        a: CtTypeNodeId,
        right: &CtTypeRef,
        b: CtTypeNodeId,
        budget: &mut CtExecutionBudget,
        depth: usize,
    ) -> CompileResult<()> {
        step(budget, depth)?;
        let l = left
            .nodes
            .get(a.0 as usize)
            .ok_or_else(|| unsupported("CT source actual Type node absent"))?;
        let r = right
            .nodes
            .get(b.0 as usize)
            .ok_or_else(|| unsupported("CT source expected Type node absent"))?;
        let recurse = |x, y, budget: &mut CtExecutionBudget| {
            self.graph_subtype_equal(left, x, right, y, budget, depth + 1)
        };
        match (l, r) {
            (CtTypeNode::Primitive(x), CtTypeNode::Primitive(y)) if x == y => {}
            (CtTypeNode::Builtin(x), CtTypeNode::Builtin(y)) if x == y => {}
            (CtTypeNode::Opaque { identity: x }, CtTypeNode::Opaque { identity: y }) if x == y => {}
            (
                CtTypeNode::BorrowedStorage {
                    storage: x,
                    life: p,
                },
                CtTypeNode::BorrowedStorage {
                    storage: y,
                    life: q,
                },
            ) => {
                budget.reserve_analysis_work(
                    (p.slots.len()
                        + q.slots.len()
                        + p.retained.len()
                        + q.retained.len()
                        + p.nominal_arguments.len()
                        + q.nominal_arguments.len()) as u64,
                )?;
                for row in p.nominal_arguments.iter().chain(&q.nominal_arguments) {
                    budget.reserve_analysis_work(row.path.len() as u64 + 32)?;
                }
                for row in p.slots.iter().chain(&q.slots) {
                    budget.reserve_analysis_work(row.path.len() as u64 + 1)?;
                }
                for row in p.retained.iter().chain(&q.retained) {
                    budget.reserve_analysis_work(row.path.len() as u64 + 1)?;
                }
                if p != q {
                    return Err(unsupported("CT source Type actual relation differs"));
                }
                recurse(*x, *y, budget)?;
            }
            (
                CtTypeNode::Nominal {
                    symbol: x,
                    arguments: p,
                },
                CtTypeNode::Nominal {
                    symbol: y,
                    arguments: q,
                },
            ) => {
                budget.reserve_analysis_work(32 + (p.len() + q.len()) as u64)?;
                if x != y || p.len() != q.len() {
                    return Err(unsupported("CT source Type actual nominal differs"));
                }
                for (p, q) in p.iter().zip(q) {
                    match (p, q) {
                        (CtTypeArgument::Type(x), CtTypeArgument::Type(y)) => {
                            recurse(*x, *y, budget)?
                        }
                        (CtTypeArgument::Scalar(x), CtTypeArgument::Scalar(y)) if x == y => {}
                        (CtTypeArgument::Lifetime(x), CtTypeArgument::Lifetime(y)) => {
                            if self.graph_lifetime(left, *x, budget)?
                                != self.graph_lifetime(right, *y, budget)?
                            {
                                return Err(unsupported("CT source Type actual lifetime differs"));
                            }
                        }
                        _ => {
                            return Err(unsupported(
                                "CT source Type actual complete arguments differ",
                            ));
                        }
                    }
                }
            }
            (CtTypeNode::Tuple(p), CtTypeNode::Tuple(q))
            | (CtTypeNode::AnonymousSum(p), CtTypeNode::AnonymousSum(q)) => {
                budget.reserve_analysis_work((p.len() + q.len()) as u64)?;
                if p.len() != q.len() {
                    return Err(unsupported(
                        "CT source Type actual structural arity differs",
                    ));
                }
                for (x, y) in p.iter().zip(q) {
                    recurse(*x, *y, budget)?;
                }
            }
            (
                CtTypeNode::Array {
                    element: x,
                    length: p,
                },
                CtTypeNode::Array {
                    element: y,
                    length: q,
                },
            ) if p == q => recurse(*x, *y, budget)?,
            (
                CtTypeNode::Address {
                    inner: x,
                    mutable: p,
                },
                CtTypeNode::Address {
                    inner: y,
                    mutable: q,
                },
            ) if p == q => recurse(*x, *y, budget)?,
            (CtTypeNode::Slice(x), CtTypeNode::Slice(y))
            | (CtTypeNode::Managed(x), CtTypeNode::Managed(y))
            | (CtTypeNode::Desc(x), CtTypeNode::Desc(y))
            | (CtTypeNode::OwnedStore(x), CtTypeNode::OwnedStore(y)) => recurse(*x, *y, budget)?,
            (
                CtTypeNode::FunctionPointer {
                    parameters: p,
                    result: x,
                },
                CtTypeNode::FunctionPointer {
                    parameters: q,
                    result: y,
                },
            ) => {
                budget.reserve_analysis_work((p.len() + q.len()) as u64)?;
                if p.len() != q.len() {
                    return Err(unsupported("CT source Type actual function arity differs"));
                }
                for (x, y) in p.iter().zip(q) {
                    recurse(*x, *y, budget)?;
                }
                recurse(*x, *y, budget)?;
            }
            (
                CtTypeNode::ErrorUnion {
                    error: x,
                    result: p,
                },
                CtTypeNode::ErrorUnion {
                    error: y,
                    result: q,
                },
            ) => {
                recurse(*x, *y, budget)?;
                recurse(*p, *q, budget)?;
            }
            _ => return Err(unsupported("CT source Type actual graph differs")),
        }
        Ok(())
    }
    pub(super) fn source_path(
        &self,
        e: &s::Expr,
        out: &mut Vec<String>,
        budget: &mut CtExecutionBudget,
        depth: usize,
    ) -> CompileResult<()> {
        step(budget, depth)?;
        match &e.kind {
            s::ExprKind::Group(e) | s::ExprKind::TypeApply { callee: e, .. } => {
                self.source_path(e, out, budget, depth + 1)
            }
            s::ExprKind::Name(name) => {
                budget.reserve_analysis_work(name.len() as u64)?;
                budget.reserve_temporary_bytes(
                    std::mem::size_of::<String>() as u64 + name.len() as u64,
                )?;
                out.reserve_exact(1);
                out.push(name.clone());
                Ok(())
            }
            s::ExprKind::Member {
                object,
                member: s::MemberName::Name(name),
            } => {
                self.source_path(object, out, budget, depth + 1)?;
                budget.reserve_analysis_work(name.len() as u64)?;
                budget.reserve_temporary_bytes(
                    std::mem::size_of::<String>() as u64 + name.len() as u64,
                )?;
                out.reserve_exact(1);
                out.push(name.clone());
                Ok(())
            }
            _ => Err(unsupported(
                "CT source callee is not an original declaration path",
            )),
        }
    }
    // Keep the source, candidate, scope and request meter explicit at this proof boundary.
    #[allow(clippy::too_many_arguments)]
    fn source_expression_type(
        &self,
        e: &s::Expr,
        graph: &CtTypeRef,
        env: &SourceEnvironment<'_>,
        raw: &CtArtifactProgram,
        index: &SourceIndex,
        budget: &mut CtExecutionBudget,
        depth: usize,
    ) -> CompileResult<()> {
        step(budget, depth)?;
        match &e.kind {
            s::ExprKind::Group(e) => {
                self.source_expression_type(e, graph, env, raw, index, budget, depth + 1)
            }
            s::ExprKind::Record { ty: Some(ty), .. } => {
                self.source_expression_type(ty, graph, env, raw, index, budget, depth + 1)
            }
            s::ExprKind::TypeApply { callee, arguments } => {
                let before = budget.live_temporary_bytes();
                let mut path = Vec::new();
                let result = (|| {
                    self.source_path(callee, &mut path, budget, depth + 1)?;
                    self.source_named(
                        &path,
                        arguments,
                        graph,
                        graph.root,
                        env,
                        raw,
                        index,
                        budget,
                        depth + 1,
                    )
                })();
                budget
                    .release_temporary_bytes(budget.live_temporary_bytes().saturating_sub(before));
                result
            }
            s::ExprKind::Name(name) => self.source_named(
                std::slice::from_ref(name),
                &[],
                graph,
                graph.root,
                env,
                raw,
                index,
                budget,
                depth + 1,
            ),
            _ => Err(unsupported(
                "CT source Type event is not an original Type expression",
            )),
        }
    }
    fn match_location(
        &self,
        location: LocatedNode<'_>,
        graph: &CtTypeRef,
        env: &SourceEnvironment<'_>,
        raw: &CtArtifactProgram,
        index: &SourceIndex,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        if !location
            .function
            .is_some_and(|f| std::ptr::eq(f, env.function))
        {
            return Err(unsupported(
                "CT source site is outside its actual original lexical function",
            ));
        }
        match location.node {
            SourceNode::Type(t) => {
                self.source_type(t, graph, graph.root, env, raw, index, budget, 0)
            }
            SourceNode::Expr(e) => {
                self.source_expression_type(e, graph, env, raw, index, budget, 0)
            }
        }
    }
    pub(super) fn extend_call_host(
        &mut self,
        call: &SourceInstanceCall,
        source_scopes: &[CtAmbientLifeScope],
        raw: &CtArtifactProgram,
        index: &SourceIndex,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        let Some(host_id) = call.enclosing else {
            return Ok(());
        };
        self.validate_entry_slice(call.site, &call.caller_scope.declaration, budget)?;
        let location = self.source_location(call.site, budget)?;
        let caller_env = self.source_environment(
            &call.caller_scope,
            call.caller_scope_id,
            source_scopes,
            budget,
        )?;
        if !location
            .function
            .is_some_and(|f| std::ptr::eq(f, caller_env.function))
        {
            return Err(unsupported(
                "CT host call source site is outside its original caller",
            ));
        }
        let SourceNode::Expr(s::Expr {
            kind: s::ExprKind::Call { callee, .. },
            ..
        }) = location.node
        else {
            return Err(unsupported(
                "CT host instance event does not select an original call",
            ));
        };
        let mut member = callee.as_ref();
        while let s::ExprKind::Group(inner) = &member.kind {
            step(budget, 0)?;
            member = inner;
        }
        if let s::ExprKind::TypeApply { callee, .. } = &member.kind {
            member = callee;
        }
        let s::ExprKind::Member { object, .. } = &member.kind else {
            return Err(unsupported(
                "CT host call has no actual lexical member expression",
            ));
        };
        let s::ExprKind::TypeApply {
            callee: host_path,
            arguments,
        } = &object.kind
        else {
            return Err(unsupported(
                "CT host call omits its original complete enclosing actuals",
            ));
        };
        let before = budget.live_temporary_bytes();
        let mut path = Vec::new();
        let result = (|| {
            self.source_path(host_path, &mut path, budget, 0)?;
            let (host_symbol, host_item) = self.source_symbol(
                &call.caller_scope.declaration,
                call.site.module,
                &path,
                index,
                budget,
            )?;
            let s::ItemKind::Aggregate(aggregate) = &host_item.kind else {
                return Err(unsupported(
                    "CT enclosing source is not an original aggregate",
                ));
            };
            if aggregate.kind != s::AggregateKind::Struct {
                return Err(unsupported("CT enclosing source is not a pure struct host"));
            }
            let host = source_scopes
                .get(host_id.0 as usize)
                .ok_or_else(|| unsupported("CT call enclosing instance row absent"))?;
            budget.reserve_analysis_work(64)?;
            if host.declaration != host_symbol
                || host.enclosing.is_some()
                || !host.binder_site.is_empty()
            {
                return Err(unsupported(
                    "CT enclosing instance does not name the actual original lexical host",
                ));
            }
            let CtScopeInstance::Instantiated(actuals) = &host.instance else {
                return Err(unsupported(
                    "CT generic host has no complete actual instance",
                ));
            };
            budget.reserve_analysis_work(
                (aggregate.generics.len() + arguments.len() + actuals.len()) as u64,
            )?;
            if aggregate.generics.len() != arguments.len() || arguments.len() != actuals.len() {
                return Err(unsupported(
                    "CT host original complete actual arity differs",
                ));
            }
            for ((parameter, source), actual) in
                aggregate.generics.iter().zip(arguments).zip(actuals)
            {
                step(budget, 0)?;
                match (&parameter.kind, source, actual) {
                    (
                        s::GenericParamKind::Lifetime,
                        s::GenericArg::Lifetime(name),
                        CtScopeActual::Lifetime(actual),
                    ) => {
                        if *actual != self.source_lifetime(name, &caller_env, budget)? {
                            return Err(unsupported(
                                "CT host lifetime actual differs from its original full ordinal",
                            ));
                        }
                    }
                    (
                        s::GenericParamKind::Type(_),
                        s::GenericArg::Type(ty),
                        CtScopeActual::Type(graph),
                    ) => {
                        self.source_type(ty, graph, graph.root, &caller_env, raw, index, budget, 0)?
                    }
                    (s::GenericParamKind::Const(domain), _, CtScopeActual::Scalar(value)) => {
                        let expected = self.source_scalar(
                            source,
                            scalar_domain(domain, budget)?,
                            &caller_env,
                            budget,
                            0,
                        )?;
                        budget.reserve_analysis_work(32)?;
                        if &expected != value {
                            return Err(unsupported(
                                "CT host scalar actual differs from original source",
                            ));
                        }
                    }
                    _ => {
                        return Err(unsupported(
                            "CT host actual kind differs from original complete ordinal",
                        ));
                    }
                }
            }
            budget.reserve_analysis_work(self.active.len() as u64 * 32)?;
            for active in &self.active {
                if active.raw.declaration == host.declaration {
                    let cost = super::super::execute::value_context::ambient_cost(
                        std::slice::from_ref(&active.raw),
                        budget,
                    )?
                    .saturating_add(
                        super::super::execute::value_context::ambient_cost(
                            std::slice::from_ref(host),
                            budget,
                        )?,
                    );
                    budget.reserve_analysis_work(cost)?;
                    if active.raw == *host {
                        return Ok(None);
                    }
                }
            }
            let origins = self
                .origins
                .as_ref()
                .ok_or_else(|| unsupported("CT host original lineage absent"))?;
            budget.reserve_analysis_work(origins.len() as u64)?;
            let generated_identity;
            let identity = if let Some(fingerprint) = host_symbol.generation_path {
                let catalog = self.generated_catalog.as_ref().ok_or_else(|| {
                    unsupported("CT generated host has no actual frozen source catalog")
                })?;
                budget.reserve_analysis_work(catalog.trees.len() as u64)?;
                let mut selected = None;
                for (row, tree) in catalog.trees.iter().enumerate() {
                    budget.reserve_analysis_work(
                        (tree.identity_path_len() as u64)
                            .saturating_mul(4)
                            .saturating_add(tree.declarations.len() as u64)
                            .saturating_add(32),
                    )?;
                    if tree.module.id != host_symbol.module
                        || tree.generation_fingerprint() != Some(fingerprint)
                    {
                        continue;
                    }
                    let Some(local) = tree
                        .declarations
                        .iter()
                        .position(|id| *id == host_symbol.declaration)
                    else {
                        continue;
                    };
                    let actual = super::super::compiler::source_scope::original_declaration(
                        &tree.module,
                        zil_base::NodeId(local as u32),
                        budget.remaining().max_call_depth as usize,
                        |work| budget.reserve_analysis_work(work as u64),
                    )?
                    .ok_or_else(|| {
                        unsupported("CT generated host has no actual declaration item")
                    })?;
                    if !std::ptr::eq(actual.item, host_item) {
                        return Err(unsupported(
                            "CT generated host source item differs from its actual catalog placement",
                        ));
                    }
                    if selected.replace((row, actual.top)).is_some() {
                        return Err(unsupported(
                            "CT generated host source placement is ambiguous",
                        ));
                    }
                }
                let (row, top) = selected.ok_or_else(|| {
                    unsupported("CT generated host declaration inventory is absent")
                })?;
                generated_identity =
                    super::generated::root_identity(catalog, row, top, origins, budget)?;
                &generated_identity
            } else {
                budget.reserve_analysis_work(self.modules.len() as u64)?;
                let module = self
                    .modules
                    .iter()
                    .find(|module| module.module.id == host_symbol.module)
                    .ok_or_else(|| unsupported("CT host actual original module is absent"))?;
                let actual = super::super::compiler::source_scope::original_declaration(
                    &module.module,
                    host_symbol.declaration,
                    budget.remaining().max_call_depth as usize,
                    |work| budget.reserve_analysis_work(work as u64),
                )?
                .ok_or_else(|| unsupported("CT host actual original declaration is absent"))?;
                if !std::ptr::eq(actual.item, host_item) {
                    return Err(unsupported(
                        "CT host source declaration differs from its actual original placement",
                    ));
                }
                &origins
                    .iter()
                    .find(|row| row.module == host_symbol.module)
                    .and_then(|row| row.items.get(actual.top))
                    .ok_or_else(|| unsupported("CT host original declaration identity absent"))?
                    .identity
            };
            let mut cost = super::super::execute::value_context::ambient_cost(
                std::slice::from_ref(host),
                budget,
            )?;
            cost = cost
                .saturating_add(identity_cost(identity, budget)?)
                .saturating_add(std::mem::size_of::<ActiveScope>() as u64)
                .saturating_add(
                    aggregate.generics.len() as u64
                        * std::mem::size_of::<CtScopeParameterKind>() as u64,
                );
            budget.reserve_temporary_bytes(cost)?;
            let mut parameters = Vec::with_capacity(aggregate.generics.len());
            for p in &aggregate.generics {
                step(budget, 0)?;
                parameters.push(match &p.kind {
                    s::GenericParamKind::Type(_) => CtScopeParameterKind::Type,
                    s::GenericParamKind::Lifetime => CtScopeParameterKind::Lifetime,
                    s::GenericParamKind::Const(ty) => {
                        CtScopeParameterKind::Scalar(scalar_domain(ty, budget)?)
                    }
                });
            }
            Ok(Some((
                ActiveScope {
                    raw: host.clone(),
                    identity: identity.clone(),
                    parameters,
                },
                cost,
            )))
        })();
        let keep = result
            .as_ref()
            .ok()
            .and_then(|row| row.as_ref())
            .map_or(0, |(_, cost)| *cost);
        budget.release_temporary_bytes(
            budget
                .live_temporary_bytes()
                .saturating_sub(before)
                .saturating_sub(keep),
        );
        if let Some((active, cost)) = result? {
            self.active.reserve_exact(1);
            self.active.push(active);
            self.bytes = self.bytes.saturating_add(cost);
        }
        Ok(())
    }
    pub(super) fn validate_source_call_actuals(
        &self,
        call: &SourceInstanceCall,
        source_scopes: &[CtAmbientLifeScope],
        raw: &CtArtifactProgram,
        index: &SourceIndex,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        self.validate_entry_slice(call.site, &call.caller_scope.declaration, budget)?;
        let location = self.source_location(call.site, budget)?;
        let function = self.original_scope_function(&call.caller_scope, budget)?;
        if !location.function.is_some_and(|f| std::ptr::eq(f, function)) {
            return Err(unsupported(
                "CT source call site is outside the original caller",
            ));
        }
        let SourceNode::Expr(e) = location.node else {
            return Err(unsupported(
                "CT source call event does not select an expression",
            ));
        };
        let s::ExprKind::Call { callee, .. } = &e.kind else {
            return Err(unsupported(
                "CT source call event does not select an original call",
            ));
        };
        let mut callee = callee.as_ref();
        while let s::ExprKind::Group(inner) = &callee.kind {
            step(budget, 0)?;
            callee = inner;
        }
        let (callee, explicit) = match &callee.kind {
            s::ExprKind::TypeApply { callee, arguments } => {
                (callee.as_ref(), Some(arguments.as_slice()))
            }
            _ => (callee, None),
        };
        let arguments = explicit.unwrap_or(&[]);
        let before = budget.live_temporary_bytes();
        let mut path = Vec::new();
        let result = (|| {
            self.source_path(callee, &mut path, budget, 0)?;
            let (symbol, item) = self.source_symbol(
                &call.caller_scope.declaration,
                call.site.module,
                &path,
                index,
                budget,
            )?;
            budget.reserve_analysis_work(32)?;
            if symbol != call.callee {
                return Err(unsupported(
                    "CT source call callee differs from its original declaration path",
                ));
            }
            let s::ItemKind::Function(declaration) = &item.kind else {
                return Err(unsupported(
                    "CT source call callee is not an original function",
                ));
            };
            budget.reserve_analysis_work(
                (declaration.generics.len() + arguments.len() + call.actuals.len()) as u64,
            )?;
            if explicit.is_none() && !declaration.generics.is_empty() {
                return Err(unsupported(
                    "CT source call omits its original complete explicit actuals",
                ));
            }
            if declaration.generics.len() != arguments.len()
                || arguments.len() != call.actuals.len()
            {
                return Err(unsupported("CT source call complete actual arity differs"));
            }
            if source_scopes.get(call.caller_scope_id.0 as usize) != Some(&call.caller_scope) {
                return Err(unsupported("CT source call original caller scope differs"));
            }
            let env = self.source_environment(
                &call.caller_scope,
                call.caller_scope_id,
                source_scopes,
                budget,
            )?;
            for ((parameter, source), actual) in declaration
                .generics
                .iter()
                .zip(arguments)
                .zip(&call.actuals)
            {
                step(budget, 0)?;
                match (&parameter.kind, source, actual) {
                    (
                        s::GenericParamKind::Lifetime,
                        s::GenericArg::Lifetime(name),
                        CtScopeActual::Lifetime(actual),
                    ) => {
                        if *actual != self.source_lifetime(name, &env, budget)? {
                            return Err(unsupported(
                                "CT source call lifetime actual differs from its original full ordinal",
                            ));
                        }
                    }
                    (
                        s::GenericParamKind::Type(_),
                        s::GenericArg::Type(ty),
                        CtScopeActual::Type(graph),
                    ) => self.source_type(ty, graph, graph.root, &env, raw, index, budget, 0)?,
                    (s::GenericParamKind::Const(domain), _, CtScopeActual::Scalar(value)) => {
                        let expected = self.source_scalar(
                            source,
                            scalar_domain(domain, budget)?,
                            &env,
                            budget,
                            0,
                        )?;
                        budget.reserve_analysis_work(32)?;
                        if &expected != value {
                            return Err(unsupported(
                                "CT source call scalar actual differs from original source",
                            ));
                        }
                    }
                    _ => {
                        return Err(unsupported(
                            "CT source call actual kind differs from original complete ordinal",
                        ));
                    }
                }
            }
            Ok(())
        })();
        budget.release_temporary_bytes(budget.live_temporary_bytes().saturating_sub(before));
        result
    }
    fn function_environment<'a>(
        &'a self,
        function: &CtFunction,
        facts: &SourceCompilation,
        raw: &'a CtArtifactProgram,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<SourceEnvironment<'a>> {
        if function.id == raw.entry
            && let Some(scope_id) = self.runtime_scope_id
        {
            let scope = raw
                .ambient_life_scopes
                .get(scope_id.0 as usize)
                .ok_or_else(|| unsupported("CT runtime entry scope placement is absent"))?;
            // The only setter independently checked this exact complete
            // instance against the real original runtime call. Reconcile it
            // again with the retained admission before using its environment.
            self.validate_scope(scope, budget)?;
            return self.source_environment(scope, scope_id, &raw.ambient_life_scopes, budget);
        }
        let mut selected = None;
        budget.reserve_analysis_work(self.active.len() as u64 * 32)?;
        for active in &self.active {
            if active.raw.declaration != function.definition {
                continue;
            }
            let authentic = match &active.raw.instance {
                CtScopeInstance::Definition => {
                    budget.reserve_analysis_work(facts.calls.len() as u64)?;
                    !facts.calls.iter().any(|call| call.instance == function.id)
                }
                CtScopeInstance::Instantiated(actuals) => {
                    let mut found = false;
                    for call in &facts.calls {
                        budget.reserve_analysis_work(32)?;
                        if call.instance != function.id
                            || call.callee != active.raw.declaration
                            || call.enclosing != active.raw.enclosing
                        {
                            continue;
                        }
                        let cost = super::super::execute::value_context::ambient_cost(
                            std::slice::from_ref(&active.raw),
                            budget,
                        )?;
                        budget.reserve_analysis_work(cost)?;
                        if &call.actuals == actuals {
                            found = true;
                        }
                    }
                    found
                }
            };
            if authentic && selected.replace(&active.raw).is_some() {
                return Err(unsupported(
                    "CT source Type function has ambiguous independently authenticated instances",
                ));
            }
        }
        let scope = selected.ok_or_else(|| {
            unsupported("CT source Type function lacks an authentic complete instance")
        })?;
        // A callee whose only ambient data arrives inside T need not publish
        // a row for its own unused life parameters. Its private actual-call
        // environment remains authoritative; it creates no graph scope handle.
        let mut scope_id = CtAmbientScopeId(u32::MAX);
        for (id, candidate) in raw.ambient_life_scopes.iter().enumerate() {
            budget.reserve_analysis_work(32)?;
            if candidate.declaration != scope.declaration || candidate.enclosing != scope.enclosing
            {
                continue;
            }
            let cost = super::super::execute::value_context::ambient_cost(
                std::slice::from_ref(candidate),
                budget,
            )?
            .saturating_add(super::super::execute::value_context::ambient_cost(
                std::slice::from_ref(scope),
                budget,
            )?);
            budget.reserve_analysis_work(cost)?;
            if candidate == scope {
                if scope_id.0 != u32::MAX {
                    return Err(unsupported(
                        "CT source Type actual instance has repeated graph scope rows",
                    ));
                }
                scope_id = CtAmbientScopeId(id as u32);
            }
        }
        self.source_environment(scope, scope_id, &raw.ambient_life_scopes, budget)
    }
    pub(in crate::ct_artifact) fn validate_type_contexts(
        &mut self,
        facts: &SourceCompilation,
        raw: &CtArtifactProgram,
        index: &SourceIndex,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        if facts.bindings.is_empty()
            && facts.type_sites.is_empty()
            && raw.ambient_life_scopes.is_empty()
        {
            return Ok(());
        }
        // Omitting the producer ledger cannot turn a contextual schema into
        // an unexamined closed value. Every actual schema context needs one
        // independently checked source event, including when all events were
        // removed together. Closed scalar programs retain the early return.
        budget.reserve_analysis_work((raw.record_schemas.len() + raw.enum_schemas.len()) as u64)?;
        for (enumeration, schema, contextual) in raw
            .record_schemas
            .iter()
            .enumerate()
            .map(|(id, schema)| (false, id, schema.life_context.is_some()))
            .chain(
                raw.enum_schemas
                    .iter()
                    .enumerate()
                    .map(|(id, schema)| (true, id, schema.life_context.is_some())),
            )
        {
            if !contextual {
                continue;
            }
            budget.reserve_analysis_work(facts.bindings.len() as u64)?;
            if facts
                .bindings
                .iter()
                .filter(|binding| {
                    binding.enumeration == enumeration && binding.schema as usize == schema
                })
                .count()
                != 1
            {
                return Err(unsupported(
                    "CT contextual schema requires one independently authenticated source event",
                ));
            }
        }
        budget.reserve_analysis_work((facts.bindings.len() + facts.type_sites.len()) as u64)?;
        for (position, binding) in facts.bindings.iter().enumerate() {
            let graph = if binding.enumeration {
                raw.enum_schemas
                    .get(binding.schema as usize)
                    .and_then(|s| s.life_context.as_ref())
            } else {
                raw.record_schemas
                    .get(binding.schema as usize)
                    .and_then(|s| s.life_context.as_ref())
            }
            .ok_or_else(|| unsupported("CT source nominal binding has no actual schema context"))?;
            if binding.parent.is_some() {
                self.validate_nominal_field_source(
                    binding, position, facts, graph, raw, index, budget,
                )?;
            } else {
                let site = binding.site.ok_or_else(|| {
                    unsupported("CT source nominal binding has no original structural site")
                })?;
                self.validate_entry_slice(site, &binding.definition, budget)?;
                let location = self.source_location(site, budget)?;
                let caller = binding.caller.ok_or_else(|| {
                    unsupported(
                        "CT source nominal binding lacks its actual function instance event",
                    )
                })?;
                let function = raw
                    .functions
                    .get(caller.0 as usize)
                    .ok_or_else(|| unsupported("CT source nominal function placement is absent"))?;
                budget.reserve_analysis_work(32)?;
                if function.definition != binding.definition {
                    return Err(unsupported(
                        "CT source nominal function original owner differs",
                    ));
                }
                let environment = self.function_environment(function, facts, raw, budget)?;
                self.match_location(location, graph, &environment, raw, index, budget)?;
            }
            self.validate_nominal_schema_fields(binding, position, facts, raw, index, budget)?;
            let cost = super::super::execute::value_context::graph_cost(graph, budget)?
                .saturating_add(std::mem::size_of::<AuthenticatedSchemaSite>() as u64);
            budget.reserve_temporary_bytes(cost)?;
            self.authenticated_schemas.reserve_exact(1);
            self.authenticated_schemas.push(AuthenticatedSchemaSite {
                enumeration: binding.enumeration,
                schema: binding.schema,
                graph: graph.clone(),
            });
            self.bytes = self.bytes.saturating_add(cost);
        }
        for (position, event) in facts.type_sites.iter().enumerate() {
            budget.reserve_analysis_work(position as u64 + 1)?;
            if facts.type_sites[..position]
                .iter()
                .any(|old| old.caller == event.caller && old.value == event.value)
            {
                return Err(unsupported("CT source Type repeats an output event"));
            }
            let function = raw
                .functions
                .get(event.caller.0 as usize)
                .ok_or_else(|| unsupported("CT source Type function absent"))?;
            let mut selected = None;
            for block in &function.blocks {
                budget.reserve_analysis_work(block.instructions.len() as u64 + 1)?;
                for instruction in &block.instructions {
                    if instruction.result == Some(event.value)
                        && selected.replace(instruction).is_some()
                    {
                        return Err(unsupported(
                            "CT source Type output has repeated definitions",
                        ));
                    }
                }
            }
            let instruction =
                selected.ok_or_else(|| unsupported("CT source Type output instruction absent"))?;
            let CtInstructionKind::TypeInContext { ty: graph } = &instruction.kind else {
                return Err(unsupported(
                    "CT source Type event output is not a contextual Type constructor",
                ));
            };
            self.validate_entry_slice(event.site, &function.definition, budget)?;
            let location = self.source_location(event.site, budget)?;
            let environment = self.function_environment(function, facts, raw, budget)?;
            self.match_location(location, graph, &environment, raw, index, budget)?;
            let cost = super::super::execute::value_context::graph_cost(graph, budget)?
                + std::mem::size_of::<AuthenticatedTypeSite>() as u64;
            budget.reserve_temporary_bytes(cost)?;
            self.bytes = self.bytes.saturating_add(cost);
            self.authenticated_types.reserve_exact(1);
            self.authenticated_types.push(AuthenticatedTypeSite {
                function: function.id,
                definition: function.definition.clone(),
                value: event.value,
                graph: graph.clone(),
            });
        }
        // A raw instruction cannot obtain authority by omitting its event.
        for function in &raw.functions {
            for block in &function.blocks {
                budget.reserve_analysis_work(block.instructions.len() as u64 + 1)?;
                for instruction in &block.instructions {
                    if let CtInstructionKind::TypeInContext { ty } = &instruction.kind {
                        budget.reserve_analysis_work(self.authenticated_types.len() as u64)?;
                        for row in &self.authenticated_types {
                            let cost = super::super::execute::value_context::graph_cost(
                                &row.graph, budget,
                            )?;
                            budget
                                .reserve_analysis_work(cost.saturating_mul(2).saturating_add(64))?;
                        }
                        if !self.authenticated_type_context(function, instruction, ty) {
                            return Err(unsupported(
                                "CT contextual Type instruction has no independently authenticated source event",
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }
    pub(in crate::ct_artifact) fn authenticated_schema_count(&self) -> usize {
        self.authenticated_schemas.len()
    }
    pub(in crate::ct_artifact) fn authenticated_source_schema(
        &self,
        enumeration: bool,
        schema: u32,
        graph: &CtTypeRef,
    ) -> bool {
        self.authenticated_schemas
            .iter()
            .find(|row| row.enumeration == enumeration && row.schema == schema)
            .is_some_and(|row| &row.graph == graph)
    }
    pub(in crate::ct_artifact) fn authenticated_type_context(
        &self,
        function: &CtFunction,
        instruction: &CtInstruction,
        ty: &CtTypeRef,
    ) -> bool {
        let Some(value) = instruction.result else {
            return false;
        };
        let Some(row) = self
            .authenticated_types
            .iter()
            .find(|row| row.function == function.id && row.value == value)
        else {
            return false;
        };
        row.definition == function.definition
            && &row.graph == ty
            && matches!(&instruction.kind, CtInstructionKind::TypeInContext {ty:actual} if actual == ty)
    }
    pub(in crate::ct_artifact) fn authenticated_type_site_count(&self) -> usize {
        self.authenticated_types.len()
    }
}

#[cfg(test)]
#[path = "invocation_context_source_tests.rs"]
mod tests;

#[path = "invocation_context_schema_fields.rs"]
mod schema_fields;
